use super::{
    copy_a8_rows_into, copy_rgba_rows, copy_rgba_rows_into, document, index_slice,
    normalized_index_mode, resolve_index, sanitize_scale, source_rect, saturating_texture_bytes,
    vertex_slice,
};
use crate::image_slots::ImageSlots;
use crate::packed_geometry::{
    PackedGeometry, PackedIndexKind, PackedIndexRange, PackedVertex, PACKED_VERTEX_BYTES,
};
use crate::{id_mask_compositor, neon_marker, scene3d};
use crate::{NormalizedIndexMode, WebGpuCpuSubmitTimingSample, WebGpuTimestampSample, WebRendererStats};
use js_sys::Reflect;
use oxide_renderer_api as api;
use oxide_wasm_alloc_counter::AllocationSnapshot;
use std::cell::Cell;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::num::NonZeroU64;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::HtmlCanvasElement;
use wgpu::util::DeviceExt;

const VERTEX_STRIDE: wgpu::BufferAddress = PACKED_VERTEX_BYTES as wgpu::BufferAddress;
const VERTEX_STRIDE_BYTES: usize = PACKED_VERTEX_BYTES;
const RRECT_INSTANCE_BYTES: usize = 36;
const IMAGE_INSTANCE_BYTES: usize = 36;
const NINE_SLICE_INSTANCE_BYTES: usize = 44;
const NINE_SLICE_INDEX_COUNT: u32 = 54;
const SPINNER_INSTANCE_BYTES: usize = 20;
const SPINNER_VERTEX_COUNT: u32 = 72;
const NEON_MARKER_INSTANCE_BYTES: usize = 60;
const NEON_MARKER_VERTEX_COUNT: u32 = 6;

const fn nine_slice_unit_vertices() -> [[u8; 4]; 36]
{
   let mut vertices = [[0_u8; 4]; 36];
   let mut row = 0_usize;
   while row < 3
   {
      let mut col = 0_usize;
      while col < 3
      {
         let base = (row * 3 + col) * 4;
         vertices[base] = [col as u8, row as u8, 0, 0];
         vertices[base + 1] = [col as u8, row as u8, 1, 0];
         vertices[base + 2] = [col as u8, row as u8, 0, 1];
         vertices[base + 3] = [col as u8, row as u8, 1, 1];
         col += 1;
      }
      row += 1;
   }
   vertices
}

const fn nine_slice_unit_indices() -> [u16; NINE_SLICE_INDEX_COUNT as usize]
{
   let mut indices = [0_u16; NINE_SLICE_INDEX_COUNT as usize];
   let mut cell = 0_usize;
   while cell < 9
   {
      let vertex = (cell * 4) as u16;
      let index = cell * 6;
      indices[index] = vertex;
      indices[index + 1] = vertex + 1;
      indices[index + 2] = vertex + 2;
      indices[index + 3] = vertex + 2;
      indices[index + 4] = vertex + 1;
      indices[index + 5] = vertex + 3;
      cell += 1;
   }
   indices
}

const NINE_SLICE_UNIT_VERTICES: [[u8; 4]; 36] = nine_slice_unit_vertices();
const NINE_SLICE_UNIT_INDICES: [u16; NINE_SLICE_INDEX_COUNT as usize] =
   nine_slice_unit_indices();
const SCENE3D_VERTEX_STRIDE: wgpu::BufferAddress = 28;
const SCENE3D_UNIFORM_STRIDE: usize = 256;
const SCENE3D_DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;
const ID_MASK_VERTEX_STRIDE: wgpu::BufferAddress = 32;
const ID_MASK_RASTER_UNIFORM_SIZE_BYTES: usize = 176;
const ID_MASK_RASTER_UNIFORM_SIZE: u64 = ID_MASK_RASTER_UNIFORM_SIZE_BYTES as u64;
// The ID-mask polish path must keep nearest-city / nearest-seam search out of the
// final compositor fragment shader. Chrome traces for a dense map workload showed
// per-fragment radius walks stalling Dawn/IOSurface at 16.708ms p95; seeding and
// jump-flooding these fields first brought that p95 to 0.235ms at mask scale 4.
const ID_MASK_FIELD_UNIFORM_SIZE_BYTES: usize = 16;
const ID_MASK_FIELD_UNIFORM_SIZE: u64 = ID_MASK_FIELD_UNIFORM_SIZE_BYTES as u64;
const ID_MASK_COMPOSITOR_UNIFORM_SIZE_BYTES: usize =
    16 * (16 + id_mask_compositor::ID_MASK_MAX_NEIGHBORHOOD_COLORS);
const ID_MASK_COMPOSITOR_UNIFORM_SIZE: u64 = ID_MASK_COMPOSITOR_UNIFORM_SIZE_BYTES as u64;
const ID_MASK_PACKED_FIELD_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Uint;
const ID_MASK_WIDE_FIELD_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const ID_MASK_PACKED_INVALID: u16 = u16::MAX;
const ID_MASK_FIELD_CACHE_MIN_BUDGET_BYTES: u64 = 64 * 1024 * 1024;
const ID_MASK_FIELD_CACHE_MAX_BUDGET_BYTES: u64 = 512 * 1024 * 1024;
const ID_MASK_FIELD_CACHE_MAX_ENTRIES: usize = 4;
const EFFECT_UNIFORM_SIZE_BYTES: usize = 16;
const EFFECT_UNIFORM_SIZE: u64 = EFFECT_UNIFORM_SIZE_BYTES as u64;
const MAX_BLUR_SIGMA: f32 = 96.0;
const TIMESTAMP_MAX_PASSES: u32 = 64;
const TIMESTAMP_QUERY_COUNT: u32 = TIMESTAMP_MAX_PASSES * 2;
const TIMESTAMP_READBACK_SLOTS: usize = 48;
const TIMESTAMP_READBACK_INTERVAL_FRAMES: u64 = 8;
const TIMESTAMP_COMPLETED_CAPACITY: usize = 4_096;
const PREPARED_CACHE_DEFAULT_BUDGET_BYTES: u64 = 32 * 1024 * 1024;
const PREPARED_BUNDLE_DEFAULT_MIN_DRAWS: usize = 8;
const PREPARED_BUNDLE_SCENE_MIN_DRAWS: u64 = 64;
const PREPARED_PROPERTY_RING_DEPTH: usize = 3;
const PREPARED_PROPERTY_UNIFORM_SIZE: u64 = 48;
const LAYER_CACHE_MIN_BUDGET_BYTES: u64 = 16 * 1024 * 1024;
const LAYER_CACHE_MAX_BUDGET_BYTES: u64 = 128 * 1024 * 1024;
const LAYER_CACHE_POOL_BUDGET_DIVISOR: u64 = 4;
const LAYER_CACHE_ABSENT_FRAMES: u64 = 120;
const LAYER_CACHE_POOL_MAX_AGE_FRAMES: u64 = 60;

const LAYER_PURGE_NONE: u8 = 0;
const LAYER_PURGE_EXPLICIT: u8 = 1;
const LAYER_PURGE_MEMORY_PRESSURE: u8 = 2;
const LAYER_PURGE_DEVICE_LOSS: u8 = 3;
const LAYER_PURGE_SCALE_CHANGE: u8 = 4;

fn cpu_submit_timing_begin(enabled: bool) -> Option<f64> {
    enabled.then(|| {
        web_sys::window()
            .and_then(|window| window.performance())
            .map_or(0.0, |performance| performance.now())
    })
}

fn cpu_submit_timing_end(output: &mut f64, before_ms: Option<f64>) {
    if let Some(before_ms) = before_ms {
        let after_ms = web_sys::window()
            .and_then(|window| window.performance())
            .map_or(before_ms, |performance| performance.now());
        *output = (after_ms - before_ms).max(0.0);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GpuImageKind {
    Rgba,
    A8,
}

impl GpuImageKind {
    const fn format(self) -> wgpu::TextureFormat {
        match self {
            Self::Rgba => wgpu::TextureFormat::Rgba8Unorm,
            Self::A8 => wgpu::TextureFormat::R8Unorm,
        }
    }

    const fn bytes_per_pixel(self) -> u32 {
        match self {
            Self::Rgba => 4,
            Self::A8 => 1,
        }
    }
}

struct GpuImage {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
    kind: GpuImageKind,
}

struct GpuLayer {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    viewport_buffer: wgpu::Buffer,
    viewport_bind_group: wgpu::BindGroup,
    viewport: [f32; 12],
    source_rect: api::RectF,
    rect: api::RectF,
    composite_rect: api::RectF,
    width: u32,
    height: u32,
    scale: f32,
    prepared_key: Option<PreparedLayerKey>,
    resources: Vec<api::RenderResourceDependency>,
    bytes: u64,
    last_used_frame: u64,
}

struct PooledGpuLayer {
    layer: GpuLayer,
    recycled_frame: u64,
}

struct GpuColorTarget {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

struct GpuDepthTarget {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}

struct GpuMesh3d {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    topology: scene3d::MeshTopology,
}

#[derive(Clone, Copy)]
enum Scene3dPipelineKind {
    AlphaDepthRead,
    AlphaDepthWrite,
    AlphaNoDepth,
    AdditiveDepthRead,
    AdditiveDepthWrite,
    AdditiveNoDepth,
}

#[derive(Clone, Copy)]
struct Scene3dDraw {
    mesh: usize,
    uniform_offset: u32,
    pipeline: Scene3dPipelineKind,
}

#[derive(Clone, Copy)]
struct IdMaskDraw {
    viewport: api::RectF,
    mask_width: u32,
    mask_height: u32,
    mask_scale: f32,
    field_key: IdMaskFieldCacheKey,
    vertex_cache_first: u32,
    vertex_cache_count: u32,
    vertex_count: u32,
    projection: id_mask_compositor::IdMaskRasterProjection,
    city_styles: [id_mask_compositor::IdMaskCityStyle; id_mask_compositor::ID_MASK_MAX_CITY_STYLES],
    neighborhood_colors: [[f32; 3]; id_mask_compositor::ID_MASK_MAX_NEIGHBORHOOD_COLORS],
    mode: id_mask_compositor::IdMaskCompositorMode,
    glow_enabled: bool,
    darken_background_alpha: f32,
    polish: id_mask_compositor::IdMaskPolishConfig,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct IdMaskProjectionKey {
    world_to_clip: [[u32; 4]; 4],
    model_to_world: [[u32; 4]; 4],
    camera_eye_unit: [u32; 3],
    normal_scale: [u32; 3],
    visible_front_min: u32,
    use_world_position: bool,
    visible_hemisphere: bool,
}

impl From<id_mask_compositor::IdMaskRasterProjection> for IdMaskProjectionKey {
    fn from(projection: id_mask_compositor::IdMaskRasterProjection) -> Self {
        Self {
            world_to_clip: projection.world_to_clip.map(|row| row.map(f32::to_bits)),
            model_to_world: projection.model_to_world.map(|row| row.map(f32::to_bits)),
            camera_eye_unit: projection.camera_eye_unit.map(f32::to_bits),
            normal_scale: projection.normal_scale.map(f32::to_bits),
            visible_front_min: projection.visible_front_min.to_bits(),
            use_world_position: projection.use_world_position,
            visible_hemisphere: projection.visible_hemisphere,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct IdMaskChunkKey {
    content_hash: u64,
    first_vertex: usize,
    vertex_count: usize,
}

impl From<&id_mask_compositor::IdMaskRasterChunk> for IdMaskChunkKey {
    fn from(chunk: &id_mask_compositor::IdMaskRasterChunk) -> Self {
        Self {
            content_hash: chunk.content_hash,
            first_vertex: chunk.first_vertex,
            vertex_count: chunk.vertex_count,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct IdMaskFieldCacheKey {
    mask_width: usize,
    mask_height: usize,
    mask_scale: u32,
    vertex_revision: u64,
    vertex_count: usize,
    projection: IdMaskProjectionKey,
}

impl IdMaskFieldCacheKey {
    fn new(raster: &id_mask_compositor::IdMaskGpuRasterPass<'_>) -> Self {
        Self {
            mask_width: raster.mask_width,
            mask_height: raster.mask_height,
            mask_scale: raster.mask_scale.to_bits(),
            vertex_revision: raster.vertex_revision,
            vertex_count: raster.vertices.len(),
            projection: raster.projection.into(),
        }
    }
}

#[derive(Clone, Copy, Default)]
struct IdMaskUniformOffsets {
    raster: u32,
    field_first: usize,
    field_count: usize,
    compositor: u32,
    cache_hit: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct IdMaskVertexCacheKey {
    content_hash: u64,
    len: usize,
}

struct IdMaskVertexCache {
    key: IdMaskVertexCacheKey,
    bytes: Vec<u8>,
    buffer: Option<wgpu::Buffer>,
    buffer_capacity: u64,
    uploaded: bool,
}

#[derive(Clone)]
struct IdMaskRenderTargets {
    width: u32,
    height: u32,
    city_texture: wgpu::Texture,
    neighborhood_texture: wgpu::Texture,
    city_view: wgpu::TextureView,
    neighborhood_view: wgpu::TextureView,
    fields: IdMaskFieldTargets,
}

#[derive(Clone)]
enum IdMaskFieldTargets {
    Packed {
        a_texture: wgpu::Texture,
        b_texture: wgpu::Texture,
        a_view: wgpu::TextureView,
        b_view: wgpu::TextureView,
        field_bind_group_a: wgpu::BindGroup,
        field_bind_group_b: wgpu::BindGroup,
        compositor_bind_group_a: wgpu::BindGroup,
        compositor_bind_group_b: wgpu::BindGroup,
    },
    Wide {
        city_a_texture: wgpu::Texture,
        city_b_texture: wgpu::Texture,
        seam_a_texture: wgpu::Texture,
        seam_b_texture: wgpu::Texture,
        city_a_view: wgpu::TextureView,
        city_b_view: wgpu::TextureView,
        seam_a_view: wgpu::TextureView,
        seam_b_view: wgpu::TextureView,
        field_bind_group_a: wgpu::BindGroup,
        field_bind_group_b: wgpu::BindGroup,
        compositor_bind_group_a: wgpu::BindGroup,
        compositor_bind_group_b: wgpu::BindGroup,
    },
}

#[derive(Clone, Copy)]
enum IdMaskFieldPair<'a> {
    Packed { texture: &'a wgpu::Texture, view: &'a wgpu::TextureView },
    Wide {
        city_texture: &'a wgpu::Texture,
        seam_texture: &'a wgpu::Texture,
        city_view: &'a wgpu::TextureView,
        seam_view: &'a wgpu::TextureView,
    },
}

impl IdMaskRenderTargets {
    fn packed_fields(&self) -> bool {
        matches!(self.fields, IdMaskFieldTargets::Packed { .. })
    }

    fn field_pair(&self, use_a: bool) -> IdMaskFieldPair<'_> {
        match &self.fields {
            IdMaskFieldTargets::Packed { a_texture, b_texture, a_view, b_view, .. } => {
                if use_a {
                    IdMaskFieldPair::Packed { texture: a_texture, view: a_view }
                } else {
                    IdMaskFieldPair::Packed { texture: b_texture, view: b_view }
                }
            }
            IdMaskFieldTargets::Wide {
                city_a_texture,
                city_b_texture,
                seam_a_texture,
                seam_b_texture,
                city_a_view,
                city_b_view,
                seam_a_view,
                seam_b_view,
                ..
            } => IdMaskFieldPair::Wide {
                city_texture: if use_a { city_a_texture } else { city_b_texture },
                seam_texture: if use_a { seam_a_texture } else { seam_b_texture },
                city_view: if use_a { city_a_view } else { city_b_view },
                seam_view: if use_a { seam_a_view } else { seam_b_view },
            },
        }
    }

    fn final_fields(&self) -> IdMaskFieldPair<'_> {
        self.field_pair(id_mask_final_fields_are_a(self.width, self.height))
    }

    fn field_bind_group(&self, use_a: bool) -> &wgpu::BindGroup {
        match &self.fields {
            IdMaskFieldTargets::Packed { field_bind_group_a, field_bind_group_b, .. }
            | IdMaskFieldTargets::Wide { field_bind_group_a, field_bind_group_b, .. } => {
                if use_a { field_bind_group_a } else { field_bind_group_b }
            }
        }
    }

    fn compositor_bind_group(&self, use_a: bool) -> &wgpu::BindGroup {
        match &self.fields {
            IdMaskFieldTargets::Packed {
                compositor_bind_group_a,
                compositor_bind_group_b,
                ..
            }
            | IdMaskFieldTargets::Wide {
                compositor_bind_group_a,
                compositor_bind_group_b,
                ..
            } => {
                if use_a { compositor_bind_group_a } else { compositor_bind_group_b }
            }
        }
    }
}

fn encode_id_mask_field_pass(
    encoder: &mut wgpu::CommandEncoder,
    label: &'static str,
    destination: IdMaskFieldPair<'_>,
    pipeline: &wgpu::RenderPipeline,
    bind_group: &wgpu::BindGroup,
    uniform_offset: u32,
    timestamp_writes: Option<wgpu::RenderPassTimestampWrites<'_>>,
) {
    let operations = wgpu::Operations {
        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
        store: wgpu::StoreOp::Store,
    };
    match destination {
        IdMaskFieldPair::Packed { view, .. } => {
            let color_attachments = [Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: operations,
            })];
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(label),
                color_attachments: &color_attachments,
                depth_stencil_attachment: None,
                timestamp_writes,
                occlusion_query_set: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind_group, &[uniform_offset]);
            pass.draw(0..6, 0..1);
        }
        IdMaskFieldPair::Wide { city_view, seam_view, .. } => {
            let color_attachments = [
                Some(wgpu::RenderPassColorAttachment {
                    view: city_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: operations,
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: seam_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: operations,
                }),
            ];
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(label),
                color_attachments: &color_attachments,
                depth_stencil_attachment: None,
                timestamp_writes,
                occlusion_query_set: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind_group, &[uniform_offset]);
            pass.draw(0..6, 0..1);
        }
    }
}

struct IdMaskFieldCacheEntry {
    key: IdMaskFieldCacheKey,
    chunks: Vec<IdMaskChunkKey>,
    targets: IdMaskRenderTargets,
    bytes: u64,
    last_used_frame: u64,
}

#[derive(Clone)]
struct IdMaskResolvedDraw {
    targets: IdMaskRenderTargets,
    cache_hit: bool,
}

impl IdMaskFieldCacheEntry {
    fn matches(&self, key: IdMaskFieldCacheKey, chunks: &[IdMaskChunkKey]) -> bool {
        self.key == key && self.chunks == chunks
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct RRectInstance
{
   rect: [f32; 4],
   radii: [f32; 4],
   rgba: u32,
}

const _: [(); RRECT_INSTANCE_BYTES] = [(); core::mem::size_of::<RRectInstance>()];

impl RRectInstance
{
   fn new(rect: api::RectF, radii: [f32; 4], color: api::Color) -> Option<Self>
   {
      let values = [rect.x, rect.y, rect.w, rect.h, radii[0], radii[1], radii[2], radii[3]];
      if rect.w <= 0.0 || rect.h <= 0.0 || color.a <= 0.0
         || !values.iter().all(|value| value.is_finite())
      {
         return None;
      }
      Some(Self {
         rect: [rect.x, rect.y, rect.w, rect.h],
         radii,
         rgba: color.pack_rgba8(),
      })
   }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct ImageInstance
{
   rect: [f32; 4],
   uv: [f32; 4],
   alpha: f32,
}

const _: [(); IMAGE_INSTANCE_BYTES] = [(); core::mem::size_of::<ImageInstance>()];

impl ImageInstance
{
   fn new(rect: api::RectF, uv: [f32; 4], alpha: f32) -> Option<Self>
   {
      let values = [rect.x, rect.y, rect.w, rect.h, uv[0], uv[1], uv[2], uv[3], alpha];
      if rect.w <= 0.0 || rect.h <= 0.0 || alpha <= 0.0
         || !values.iter().all(|value| value.is_finite())
      {
         return None;
      }
      Some(Self {
         rect: [rect.x, rect.y, rect.w, rect.h],
         uv,
         alpha: (alpha.min(1.0) * 255.0).round() / 255.0,
      })
   }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct NineSliceInstance
{
   rect: [f32; 4],
   image_size: [f32; 2],
   slice: [f32; 4],
   alpha: f32,
}

const _: [(); NINE_SLICE_INSTANCE_BYTES] = [(); core::mem::size_of::<NineSliceInstance>()];

impl NineSliceInstance
{
   fn new(rect: api::RectF, image_size: [f32; 2], slice: api::Insets, alpha: f32) -> Option<Self>
   {
      let [iw, ih] = image_size;
      let left = slice.left.clamp(0.0, iw);
      let right = slice.right.clamp(0.0, iw - left);
      let top = slice.top.clamp(0.0, ih);
      let bottom = slice.bottom.clamp(0.0, ih - top);
      let values = [
         rect.x, rect.y, rect.w, rect.h, iw, ih,
         left, top, right, bottom, alpha,
      ];
      if iw <= 0.0 || ih <= 0.0 || alpha <= 0.0
         || !values.iter().all(|value| value.is_finite())
      {
         return None;
      }
      Some(Self {
         rect: [rect.x, rect.y, rect.w, rect.h],
         image_size,
         slice: [left, top, right, bottom],
         alpha: (alpha.min(1.0) * 255.0).round() / 255.0,
      })
   }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct SpinnerInstance
{
   center: [f32; 2],
   atom: f32,
   alpha: f32,
   rgba: u32,
}

const _: [(); SPINNER_INSTANCE_BYTES] = [(); core::mem::size_of::<SpinnerInstance>()];

impl SpinnerInstance
{
   fn new(center: [f32; 2], atom: f32, alpha: f32) -> Option<Self>
   {
      let values = [center[0], center[1], atom, alpha];
      if atom <= 0.0 || alpha <= 0.0 || !values.iter().all(|value| value.is_finite())
      {
         return None;
      }
      Some(Self {
         center,
         atom,
         alpha: alpha.min(1.0),
         rgba: api::Color::rgba(0.15, 0.15, 0.15, 1.0).pack_rgba8(),
      })
   }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct NeonMarkerInstance
{
   center: [f32; 2],
   shape: [f32; 4],
   alpha: [f32; 3],
   core_rgba: u32,
   ring_rgba: u32,
   viewport: [f32; 4],
}

const _: [(); NEON_MARKER_INSTANCE_BYTES] = [(); core::mem::size_of::<NeonMarkerInstance>()];

impl NeonMarkerInstance
{
   fn new(marker: neon_marker::NeonMarker, viewport: api::RectF) -> Option<Self>
   {
      let values = [
         marker.center[0], marker.center[1], marker.core_radius_px, marker.ring_radius_px,
         marker.ring_width_px, marker.halo_radius_px, marker.halo_sigma_px,
         marker.halo_alpha_max, marker.ring_alpha_max,
         viewport.x, viewport.y, viewport.w, viewport.h,
      ];
      if viewport.w <= 0.0 || viewport.h <= 0.0 || !values.iter().all(|value| value.is_finite())
      {
         return None;
      }
      Some(Self {
         center: marker.center,
         shape: [
            marker.core_radius_px.max(0.0),
            marker.ring_radius_px.max(0.0),
            marker.ring_width_px.max(0.001),
            marker.halo_radius_px.max(0.0),
         ],
         alpha: [
            marker.halo_sigma_px.max(0.001),
            marker.halo_alpha_max.clamp(0.0, 1.0),
            marker.ring_alpha_max.clamp(0.0, 1.0),
         ],
         core_rgba: marker.core_color.pack_rgba8(),
         ring_rgba: marker.ring_color.pack_rgba8(),
         viewport: [viewport.x, viewport.y, viewport.w, viewport.h],
      })
   }
}

#[derive(Clone, Copy, PartialEq)]
enum DrawKind {
    Solid,
    RRect { first_instance: u32, instance_count: u32 },
    Image { image: u32, kind: GpuImageKind, first_instance: u32, instance_count: u32 },
    NineSlice { image: u32, kind: GpuImageKind, first_instance: u32, instance_count: u32 },
    Spinner { first_instance: u32, instance_count: u32 },
    NeonMarker { first_instance: u32, instance_count: u32 },
    Rgba { image: u32 },
    A8 { image: u32 },
    Sdf { image: u32 },
    Layer { id: u32 },
    Backdrop { rect: api::RectF, sigma: f32 },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DrawPipelineKey {
    Solid,
    RRect,
    ImageRgba,
    ImageA8,
    NineSliceRgba,
    NineSliceA8,
    Spinner,
    NeonMarker,
    Rgba,
    A8,
    Sdf,
    Effect,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DrawBindKey {
    None,
    Texture { image: u32 },
    Layer { id: u32 },
    Effect { offset: u32 },
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct DrawStateKey {
    pipeline: DrawPipelineKey,
    bind: DrawBindKey,
    clip: api::RectI,
}

#[derive(Clone, Copy, PartialEq)]
struct GpuDraw {
    kind: DrawKind,
    index_kind: PackedIndexKind,
    first_index: u32,
    index_count: u32,
    base_vertex: i32,
    clip: api::RectI,
    effect_uniform_offset: u32,
    target: Option<u32>,
}

#[derive(Clone, Copy)]
struct FrameLayerPass {
    id: u32,
    start: usize,
    end: usize,
}

#[derive(Default)]
struct FrameData {
    geometry: PackedGeometry,
    rrect_instances: Vec<RRectInstance>,
    image_instances: Vec<ImageInstance>,
    nine_slice_instances: Vec<NineSliceInstance>,
    spinner_instances: Vec<SpinnerInstance>,
    neon_marker_instances: Vec<NeonMarkerInstance>,
    draws: Vec<GpuDraw>,
    layer_passes: Vec<FrameLayerPass>,
    effect_count: usize,
    effect_first_sigma_bits: u32,
    effect_shared_sigma: f32,
    effect_single_uniform_slot: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct PreparedChunkKey
{
   id: api::RenderChunkId,
   structural_revision: u64,
   geometry_revision: u64,
   resource_revision: u64,
   device_generation: u64,
   format: wgpu::TextureFormat,
   bundles_enabled: bool,
}

impl PreparedChunkKey
{
   fn new(chunk: &api::RenderChunk, device_generation: u64, format: wgpu::TextureFormat, bundles_enabled: bool) -> Self
   {
      let revisions = chunk.revisions();
      Self {
         id: chunk.id(),
         structural_revision: revisions.structural,
         geometry_revision: revisions.geometry,
         resource_revision: revisions.resource,
         device_generation,
         format,
         bundles_enabled,
      }
   }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct PreparedLayerKey
{
   id: u32,
   chunk: PreparedChunkKey,
   content_generation: u64,
   nested_generation: u64,
   dynamic_generation: u64,
   bounds: [u32; 4],
   scale: [u32; 2],
   opacity: u32,
   target_scale: u32,
   effect_outset: u32,
   pixel_phase: [u32; 2],
}

#[derive(Clone, Copy)]
struct PreparedLayerFrame
{
   key: PreparedLayerKey,
   source_rect: api::RectF,
   rect: api::RectF,
   viewport: [f32; 12],
   width: u32,
   height: u32,
   force_refresh: bool,
}

#[derive(Clone, Copy)]
struct LocalLayerFrame
{
   source_rect: api::RectF,
   target_rect: api::RectF,
   composite_rect: api::RectF,
   viewport: [f32; 12],
   width: u32,
   height: u32,
}

#[derive(Clone)]
struct PreparedLayerPlanEntry
{
   frame: PreparedLayerFrame,
   chunk: api::RenderChunk,
   duplicate: bool,
   animated: bool,
}

#[derive(Clone, Copy)]
struct PreparedFrameInstance
{
   key: PreparedChunkKey,
   property_offset: Option<u32>,
   clip: Option<api::RectI>,
}

struct PreparedPropertyRing
{
   buffer: wgpu::Buffer,
   bind_group: wgpu::BindGroup,
   stride: u64,
   capacity: usize,
   uniforms: Vec<[f32; 12]>,
   revisions: Vec<u64>,
   ring_revisions: Vec<[u64; PREPARED_PROPERTY_RING_DEPTH]>,
   pending: Vec<usize>,
   bytes: Vec<u8>,
}

impl PreparedPropertyRing
{
   fn new(device: &wgpu::Device, programs: &GpuPrograms) -> Self
   {
      let stride = align_to(
         PREPARED_PROPERTY_UNIFORM_SIZE,
         u64::from(device.limits().min_uniform_buffer_offset_alignment.max(1)),
      );
      let capacity = 16;
      let (buffer, bind_group) = create_prepared_property_buffer(device, programs, stride, capacity);
      Self {
         buffer,
         bind_group,
         stride,
         capacity,
         uniforms: Vec::new(),
         revisions: Vec::new(),
         ring_revisions: Vec::new(),
         pending: Vec::new(),
         bytes: vec![0; prepared_property_ring_bytes(stride, capacity) as usize],
      }
   }

   fn ensure_capacity(&mut self, device: &wgpu::Device, programs: &GpuPrograms, needed: usize) -> bool
   {
      if needed <= self.capacity
      {
         return false;
      }
      let capacity = needed.max(self.capacity + self.capacity / 2);
      let (buffer, bind_group) = create_prepared_property_buffer(device, programs, self.stride, capacity);
      self.buffer = buffer;
      self.bind_group = bind_group;
      self.capacity = capacity;
      self.bytes.resize(prepared_property_ring_bytes(self.stride, capacity) as usize, 0);
      self.bytes.fill(0);
      for revisions in &mut self.ring_revisions
      {
         *revisions = [0; PREPARED_PROPERTY_RING_DEPTH];
      }
      true
   }

   fn begin_frame(&mut self)
   {
      self.pending.clear();
   }

   fn resolve(&mut self, index: usize, slot: usize, uniform: [f32; 12]) -> Option<u32>
   {
      while self.uniforms.len() <= index
      {
         self.uniforms.push(uniform);
         self.revisions.push(1);
         self.ring_revisions.push([0; PREPARED_PROPERTY_RING_DEPTH]);
      }
      if self.uniforms[index] != uniform
      {
         self.uniforms[index] = uniform;
         self.revisions[index] = self.revisions[index].wrapping_add(1).max(1);
      }
      let dynamic_offset = u32::try_from(self.offset(slot, index)).ok()?;
      if self.ring_revisions[index][slot] != self.revisions[index]
      {
         let offset = self.offset(slot, index) as usize;
         let values = bytemuck::cast_slice(&uniform);
         self.bytes[offset..offset + values.len()].copy_from_slice(values);
         self.pending.push(index);
      }
      Some(dynamic_offset)
   }

   fn upload(&mut self, queue: &wgpu::Queue, slot: usize) -> (u64, u64, u32)
   {
      let property_bytes = (self.pending.len() as u64).saturating_mul(PREPARED_PROPERTY_UNIFORM_SIZE);
      let records = self.pending.len().min(u32::MAX as usize) as u32;
      let mut upload_bytes = 0_u64;
      let mut start = 0;
      while start < self.pending.len()
      {
         let mut end = start + 1;
         while end < self.pending.len() && self.pending[end] == self.pending[end - 1] + 1
         {
            end += 1;
         }
         let first = self.offset(slot, self.pending[start]) as usize;
         let last = self.offset(slot, self.pending[end - 1]) as usize + PREPARED_PROPERTY_UNIFORM_SIZE as usize;
         queue.write_buffer(&self.buffer, first as u64, &self.bytes[first..last]);
         upload_bytes = upload_bytes.saturating_add((last - first) as u64);
         start = end;
      }
      for index in self.pending.iter().copied()
      {
         self.ring_revisions[index][slot] = self.revisions[index];
      }
      (upload_bytes, property_bytes, records)
   }

   fn truncate(&mut self, len: usize)
   {
      self.uniforms.truncate(len);
      self.revisions.truncate(len);
      self.ring_revisions.truncate(len);
   }

   #[inline]
   fn offset(&self, slot: usize, index: usize) -> u64
   {
      (slot.saturating_mul(self.capacity).saturating_add(index) as u64).saturating_mul(self.stride)
   }

   #[inline]
   fn byte_size(&self) -> u64
   {
      prepared_property_ring_bytes(self.stride, self.capacity)
   }
}

fn prepared_property_ring_bytes(stride: u64, capacity: usize) -> u64
{
   stride
      .saturating_mul(capacity as u64)
      .saturating_mul(PREPARED_PROPERTY_RING_DEPTH as u64)
}

fn prepared_dynamic_uniform(snapshot: &api::RenderSnapshot, property_slots: &[api::RenderPropertySlotId], origin: [f32; 2], viewport: [f32; 2], animation_phase: f32) -> Option<[f32; 12]>
{
   let mut matrix = [1.0_f32, 0.0, 0.0, 1.0];
   let mut translation = [0.0_f32, 0.0];
   let mut opacity = 1.0_f32;
   for id in property_slots.iter().copied()
   {
      let index = snapshot.properties().binary_search_by_key(&id.0, |property| property.id.0).ok()?;
      match snapshot.properties()[index].value
      {
         api::RenderPropertyValue::Transform(transform) =>
         {
            let next = [
               transform[0] * matrix[0] + transform[2] * matrix[1],
               transform[1] * matrix[0] + transform[3] * matrix[1],
               transform[0] * matrix[2] + transform[2] * matrix[3],
               transform[1] * matrix[2] + transform[3] * matrix[3],
            ];
            translation = [
               transform[0] * translation[0] + transform[2] * translation[1] + transform[4],
               transform[1] * translation[0] + transform[3] * translation[1] + transform[5],
            ];
            matrix = next;
         }
         api::RenderPropertyValue::Opacity(value) => opacity *= value,
      }
   }
   translation = [
      matrix[0] * origin[0] + matrix[2] * origin[1] + translation[0],
      matrix[1] * origin[0] + matrix[3] * origin[1] + translation[1],
   ];
   let values = [
      viewport[0], viewport[1], 0.0, 0.0,
      matrix[0], matrix[1], matrix[2], matrix[3],
      translation[0], translation[1], opacity.clamp(0.0, 1.0), animation_phase,
   ];
   values.iter().all(|value| value.is_finite()).then_some(values)
}

fn prepared_layer_frame(renderer: &WebGpuRenderer, layer: api::RenderLayerInstance, chunk: &api::RenderChunk, uniform: [f32; 12], clip: Option<api::RectI>) -> Option<PreparedLayerFrame>
{
   if clip.is_some()
   {
      return None;
   }
   let [_, _, _, _, scale_x, shear_y, shear_x, scale_y, translate_x, translate_y, opacity, _] = uniform;
   if shear_x != 0.0 || shear_y != 0.0 || scale_x == 0.0 || scale_y == 0.0
   {
      return None;
   }
   let effect_outset = layer_effect_outset(chunk, layer.rect)?;
   let local_rect = api::RectF::new(
      layer.rect.x - effect_outset,
      layer.rect.y - effect_outset,
      layer.rect.w + effect_outset * 2.0,
      layer.rect.h + effect_outset * 2.0,
   );
   if ![
      local_rect.x, local_rect.y, local_rect.w, local_rect.h,
      scale_x, scale_y, translate_x, translate_y, renderer.scale,
   ].iter().all(|value| value.is_finite()) || local_rect.w <= 0.0 || local_rect.h <= 0.0
   {
      return None;
   }
   let transformed_x0 = scale_x * local_rect.x;
   let transformed_x1 = scale_x * (local_rect.x + local_rect.w);
   let transformed_y0 = scale_y * local_rect.y;
   let transformed_y1 = scale_y * (local_rect.y + local_rect.h);
   let min_x = transformed_x0.min(transformed_x1);
   let min_y = transformed_y0.min(transformed_y1);
   let width_dp = (transformed_x1 - transformed_x0).abs();
   let height_dp = (transformed_y1 - transformed_y0).abs();
   let target_scale = sanitize_scale(renderer.scale);
   let unsnapped_rect = api::RectF::new(min_x + translate_x, min_y + translate_y, width_dp, height_dp);
   let (rect, width, height) = layer_target_rect(
      unsnapped_rect,
      target_scale,
      renderer.device.limits().max_texture_dimension_2d,
   )?;
   let phase_x = (unsnapped_rect.x * target_scale).rem_euclid(1.0);
   let phase_y = (unsnapped_rect.y * target_scale).rem_euclid(1.0);
   let revisions = chunk.revisions();
   Some(PreparedLayerFrame {
      key: PreparedLayerKey {
         id: layer.id,
         chunk: PreparedChunkKey::new(chunk, renderer.prepared_device_generation, renderer.config.format, false),
         content_generation: revisions.geometry,
         nested_generation: revisions.structural,
         dynamic_generation: revisions.dynamic_properties,
         bounds: [
            layer.rect.x.to_bits(), layer.rect.y.to_bits(),
            layer.rect.w.to_bits(), layer.rect.h.to_bits(),
         ],
         scale: [scale_x.to_bits(), scale_y.to_bits()],
         opacity: opacity.to_bits(),
         target_scale: target_scale.to_bits(),
         effect_outset: effect_outset.to_bits(),
         pixel_phase: [phase_x.to_bits(), phase_y.to_bits()],
      },
      source_rect: layer.rect,
      rect,
      viewport: [
         rect.w, rect.h, rect.x, rect.y,
         scale_x, 0.0, 0.0, scale_y,
         translate_x, translate_y, opacity, renderer.animation_phase,
      ],
      width,
      height,
      force_refresh: layer.dirty,
   })
}

fn layer_effect_outset(chunk: &api::RenderChunk, rect: api::RectF) -> Option<f32>
{
   let mut outset = 0.0_f32;
   for effect in chunk.effect_dependencies()
   {
      let bounds = match effect.sample_bounds
      {
         api::RenderSpatialBounds::Empty => continue,
         api::RenderSpatialBounds::Finite(bounds) => bounds,
         api::RenderSpatialBounds::Unbounded => return None,
      };
      outset = outset
         .max(rect.x - bounds.x)
         .max(rect.y - bounds.y)
         .max(bounds.x + bounds.w - (rect.x + rect.w))
         .max(bounds.y + bounds.h - (rect.y + rect.h));
   }
   outset.is_finite().then_some(outset.max(0.0))
}

fn layer_target_rect(rect: api::RectF, scale: f32, max_dimension: u32) -> Option<(api::RectF, u32, u32)>
{
   if ![rect.x, rect.y, rect.w, rect.h, scale].iter().all(|value| value.is_finite())
      || rect.w <= 0.0 || rect.h <= 0.0
   {
      return None;
   }
   let scale = sanitize_scale(scale);
   let x0 = (rect.x * scale).floor();
   let y0 = (rect.y * scale).floor();
   let x1 = ((rect.x + rect.w) * scale).ceil();
   let y1 = ((rect.y + rect.h) * scale).ceil();
   let width = x1 - x0;
   let height = y1 - y0;
   if !width.is_finite() || !height.is_finite()
      || width < 1.0 || height < 1.0
      || width > max_dimension as f32 || height > max_dimension as f32
   {
      return None;
   }
   Some((
      api::RectF::new(x0 / scale, y0 / scale, width / scale, height / scale),
      width as u32,
      height as u32,
   ))
}

fn prepared_transform_rect(rect: api::RectF, uniform: [f32; 12]) -> api::RectI
{
   let [_, _, _, _, m11, m12, m21, m22, tx, ty, ..] = uniform;
   let x1 = rect.x + rect.w;
   let y1 = rect.y + rect.h;
   let points = [
      [m11 * rect.x + m21 * rect.y + tx, m12 * rect.x + m22 * rect.y + ty],
      [m11 * x1 + m21 * rect.y + tx, m12 * x1 + m22 * rect.y + ty],
      [m11 * rect.x + m21 * y1 + tx, m12 * rect.x + m22 * y1 + ty],
      [m11 * x1 + m21 * y1 + tx, m12 * x1 + m22 * y1 + ty],
   ];
   let min_x = points.iter().map(|point| point[0]).fold(f32::INFINITY, f32::min).floor();
   let min_y = points.iter().map(|point| point[1]).fold(f32::INFINITY, f32::min).floor();
   let max_x = points.iter().map(|point| point[0]).fold(f32::NEG_INFINITY, f32::max).ceil();
   let max_y = points.iter().map(|point| point[1]).fold(f32::NEG_INFINITY, f32::max).ceil();
   api::RectI::new(min_x as i32, min_y as i32, (max_x - min_x) as i32, (max_y - min_y) as i32)
}

fn prepared_intersect_clip(a: api::RectI, b: api::RectI) -> api::RectI
{
   let x0 = i64::from(a.x).max(i64::from(b.x));
   let y0 = i64::from(a.y).max(i64::from(b.y));
   let x1 = (i64::from(a.x) + i64::from(a.w)).min(i64::from(b.x) + i64::from(b.w));
   let y1 = (i64::from(a.y) + i64::from(a.h)).min(i64::from(b.y) + i64::from(b.h));
   api::RectI::new(
      x0.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
      y0.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
      (x1 - x0).clamp(0, i64::from(i32::MAX)) as i32,
      (y1 - y0).clamp(0, i64::from(i32::MAX)) as i32,
   )
}

struct PreparedChunk
{
   vertex_buffer: Option<wgpu::Buffer>,
   rrect_instance_buffer: Option<wgpu::Buffer>,
   rrect_instances: u32,
   image_instance_buffer: Option<wgpu::Buffer>,
   image_instances: u32,
   nine_slice_instance_buffer: Option<wgpu::Buffer>,
   nine_slice_instances: u32,
   spinner_instance_buffer: Option<wgpu::Buffer>,
   spinner_instances: u32,
   index_buffer_u16: Option<wgpu::Buffer>,
   index_buffer_u32: Option<wgpu::Buffer>,
   draws: Vec<GpuDraw>,
   segments: Vec<PreparedSegment>,
   resources: Box<[api::ImageHandle]>,
   vertex_bytes: u64,
   index_bytes: u64,
   resident_bytes: u64,
   bundle_generation: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct PreparedSnapshotBundleKey
{
   id: api::RenderChunkId,
   generation: u64,
}

struct PreparedSnapshotBundle
{
   keys: Box<[PreparedSnapshotBundleKey]>,
   bundle: wgpu::RenderBundle,
   draws: u32,
   rrect_instances: u32,
   image_instances: u32,
   nine_slice_instances: u32,
   spinner_instances: u32,
}

enum PreparedSegment
{
   Bundle { bundle: wgpu::RenderBundle, draws: u32 },
   Direct { start: usize, end: usize },
}

struct CachedPreparedChunk
{
   key: PreparedChunkKey,
   chunk: PreparedChunk,
   last_used: u64,
   revision_rebuilds: u8,
}

struct PreparedChunkCache
{
   entries: HashMap<api::RenderChunkId, CachedPreparedChunk>,
   budget_bytes: u64,
   resident_bytes: u64,
   clock: u64,
   evictions: u64,
}

impl Default for PreparedChunkCache
{
   fn default() -> Self
   {
      Self {
         entries: HashMap::new(),
         budget_bytes: PREPARED_CACHE_DEFAULT_BUDGET_BYTES,
         resident_bytes: 0,
         clock: 0,
         evictions: 0,
      }
   }
}

impl PreparedChunkCache
{
   fn clear(&mut self)
   {
      self.entries.clear();
      self.resident_bytes = 0;
   }

   fn invalidate_resource(&mut self, handle: api::ImageHandle)
   {
      let ids = self.entries.iter().filter_map(|(id, entry)| {
         entry.chunk.resources.contains(&handle).then_some(*id)
      }).collect::<Vec<_>>();
      for id in ids
      {
         self.evict(id);
      }
   }

   fn remove(&mut self, id: api::RenderChunkId) -> Option<CachedPreparedChunk>
   {
      let entry = self.entries.remove(&id);
      if let Some(entry) = entry.as_ref()
      {
         self.resident_bytes = self.resident_bytes.saturating_sub(entry.chunk.resident_bytes);
      }
      entry
   }

   fn evict(&mut self, id: api::RenderChunkId)
   {
      let resident_bytes = self.resident_bytes;
      let _ = self.remove(id);
      if self.resident_bytes != resident_bytes
      {
         self.evictions = self.evictions.saturating_add(1);
      }
   }

   fn get(&self, key: PreparedChunkKey) -> Option<&PreparedChunk>
   {
      self.entries.get(&key.id).filter(|entry| entry.key == key).map(|entry| &entry.chunk)
   }

   fn touch(&mut self, key: PreparedChunkKey) -> bool
   {
      let Some(entry) = self.entries.get_mut(&key.id).filter(|entry| entry.key == key) else
      {
         return false;
      };
      self.clock = self.clock.wrapping_add(1);
      entry.last_used = self.clock;
      entry.revision_rebuilds = 0;
      true
   }

   fn revision_rebuild_streak(&self, key: PreparedChunkKey) -> u8
   {
      self.entries.get(&key.id).map_or(0, |entry| {
         if entry.key == key
         {
            entry.revision_rebuilds
         }
         else
         {
            entry.revision_rebuilds.saturating_add(1)
         }
      })
   }

   fn insert(&mut self, key: PreparedChunkKey, chunk: PreparedChunk, revision_rebuilds: u8)
   {
      let _ = self.remove(key.id);
      self.clock = self.clock.wrapping_add(1);
      self.resident_bytes = self.resident_bytes.saturating_add(chunk.resident_bytes);
      self.entries.insert(key.id, CachedPreparedChunk {
         key,
         chunk,
         last_used: self.clock,
         revision_rebuilds,
      });
   }

   fn enforce_budget(&mut self, protected: &[PreparedFrameInstance])
   {
      while self.resident_bytes > self.budget_bytes
      {
         let victim = self.entries.iter().filter(|(_, entry)| {
            !protected.iter().any(|instance| instance.key == entry.key)
         }).min_by_key(|(_, entry)| entry.last_used).map(|(id, _)| *id);
         let Some(victim) = victim else { break };
         self.evict(victim);
      }
   }

   fn take_evictions(&mut self) -> u64
   {
      core::mem::take(&mut self.evictions)
   }

   fn vertex_bytes(&self) -> u64
   {
      self.entries.values().fold(0, |total, entry| {
         total.saturating_add(entry.chunk.vertex_bytes)
      })
   }

   fn index_bytes(&self) -> u64
   {
      self.entries.values().fold(0, |total, entry| {
         total.saturating_add(entry.chunk.index_bytes)
      })
   }
}

impl FrameData {
    fn clear(&mut self) {
        self.geometry.clear();
        self.rrect_instances.clear();
        self.image_instances.clear();
        self.nine_slice_instances.clear();
        self.spinner_instances.clear();
        self.neon_marker_instances.clear();
        self.draws.clear();
        self.layer_passes.clear();
        self.effect_count = 0;
        self.effect_first_sigma_bits = 0;
        self.effect_shared_sigma = 0.0;
        self.effect_single_uniform_slot = true;
    }

    fn record_draw_kind(&mut self, kind: DrawKind) {
        let DrawKind::Backdrop { sigma, .. } = kind else {
            return;
        };
        let sigma_bits = sigma.to_bits();
        if self.effect_count == 0 {
            self.effect_first_sigma_bits = sigma_bits;
            self.effect_shared_sigma = sigma;
            self.effect_single_uniform_slot = true;
        } else if self.effect_first_sigma_bits != sigma_bits {
            self.effect_single_uniform_slot = false;
        }
        self.effect_count = self.effect_count.saturating_add(1);
    }
}

fn coalescible_draw_kind(a: DrawKind, b: DrawKind) -> bool {
    match (a, b) {
        (DrawKind::Solid, DrawKind::Solid) => true,
        (DrawKind::RRect { .. }, DrawKind::RRect { .. }) => false,
        (DrawKind::Image { .. }, DrawKind::Image { .. }) => false,
        (DrawKind::NineSlice { .. }, DrawKind::NineSlice { .. }) => false,
        (DrawKind::Spinner { .. }, DrawKind::Spinner { .. }) => false,
        (DrawKind::NeonMarker { .. }, DrawKind::NeonMarker { .. }) => false,
        (DrawKind::Rgba { image: a }, DrawKind::Rgba { image: b }) => a == b,
        (DrawKind::A8 { image: a }, DrawKind::A8 { image: b }) => a == b,
        (DrawKind::Sdf { image: a }, DrawKind::Sdf { image: b }) => a == b,
        (DrawKind::Layer { id: a }, DrawKind::Layer { id: b }) => a == b,
        _ => false,
    }
}

#[derive(Clone, Copy)]
enum TimestampPassFamily {
    Clear,
    Draw,
    Scene3d,
    Scene3dOverlay,
    IdMaskRaster,
    IdMaskFieldSeed,
    IdMaskFieldJump,
    IdMaskCompositor,
    Present,
}

#[derive(Clone, Copy)]
struct TimestampPassRecord {
    family: TimestampPassFamily,
    begin_query: u32,
    end_query: u32,
}

#[derive(Clone, Copy, Default)]
struct TimestampSummary {
    frame_id: u64,
    passes: u32,
    total_ns: u64,
    clear_ns: u64,
    draw_ns: u64,
    scene3d_ns: u64,
    scene3d_overlay_ns: u64,
    id_mask_raster_ns: u64,
    id_mask_field_seed_ns: u64,
    id_mask_field_jump_ns: u64,
    id_mask_compositor_ns: u64,
    present_ns: u64,
    max_pass_ns: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TimestampReadbackState {
    Idle,
    Pending,
}

#[derive(Clone, Copy)]
enum SubmitAllocationStage {
    Upload,
    Surface,
    Encoder,
    Render,
    Timestamp,
    ScratchStats,
    FinishQueue,
    Present,
    TimestampMap,
}

struct TimestampReadbackSlot {
    buffer: wgpu::Buffer,
    mapped: Rc<Cell<bool>>,
    failed: Rc<Cell<bool>>,
    state: TimestampReadbackState,
    frame_id: u64,
    query_count: u32,
    records: Vec<TimestampPassRecord>,
}

struct WebGpuTimestampQueries {
    query_set: wgpu::QuerySet,
    resolve_buffer: wgpu::Buffer,
    slots: Vec<TimestampReadbackSlot>,
    next_slot: usize,
    current_records: Vec<TimestampPassRecord>,
    current_query_count: u32,
    timestamp_period_ns: f64,
    latest: TimestampSummary,
    completed: Option<Box<VecDeque<WebGpuTimestampSample>>>,
    readback_interval_frames: u64,
    readback_skips: u32,
}

impl TimestampSummary {
    fn add(&mut self, family: TimestampPassFamily, ns: u64) {
        self.passes = self.passes.saturating_add(1);
        self.total_ns = self.total_ns.saturating_add(ns);
        self.max_pass_ns = self.max_pass_ns.max(ns);
        match family {
            TimestampPassFamily::Clear => self.clear_ns = self.clear_ns.saturating_add(ns),
            TimestampPassFamily::Draw => self.draw_ns = self.draw_ns.saturating_add(ns),
            TimestampPassFamily::Scene3d => self.scene3d_ns = self.scene3d_ns.saturating_add(ns),
            TimestampPassFamily::Scene3dOverlay => {
                self.scene3d_overlay_ns = self.scene3d_overlay_ns.saturating_add(ns);
            }
            TimestampPassFamily::IdMaskRaster => {
                self.id_mask_raster_ns = self.id_mask_raster_ns.saturating_add(ns);
            }
            TimestampPassFamily::IdMaskFieldSeed => {
                self.id_mask_field_seed_ns = self.id_mask_field_seed_ns.saturating_add(ns);
            }
            TimestampPassFamily::IdMaskFieldJump => {
                self.id_mask_field_jump_ns = self.id_mask_field_jump_ns.saturating_add(ns);
            }
            TimestampPassFamily::IdMaskCompositor => {
                self.id_mask_compositor_ns = self.id_mask_compositor_ns.saturating_add(ns);
            }
            TimestampPassFamily::Present => self.present_ns = self.present_ns.saturating_add(ns),
        }
    }

    fn sample(self) -> WebGpuTimestampSample {
        WebGpuTimestampSample {
            frame_id: self.frame_id,
            passes: self.passes,
            total_ns: self.total_ns,
            clear_ns: self.clear_ns,
            draw_ns: self.draw_ns,
            scene3d_ns: self.scene3d_ns,
            scene3d_overlay_ns: self.scene3d_overlay_ns,
            id_mask_raster_ns: self.id_mask_raster_ns,
            id_mask_field_seed_ns: self.id_mask_field_seed_ns,
            id_mask_field_jump_ns: self.id_mask_field_jump_ns,
            id_mask_compositor_ns: self.id_mask_compositor_ns,
            present_ns: self.present_ns,
            max_pass_ns: self.max_pass_ns,
        }
    }
}

impl WebGpuTimestampQueries {
    fn new(device: &wgpu::Device, timestamp_period_ns: f64) -> Self {
        let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("oxide-webgpu-timestamp-queries"),
            ty: wgpu::QueryType::Timestamp,
            count: TIMESTAMP_QUERY_COUNT,
        });
        let resolve_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("oxide-webgpu-timestamp-resolve"),
            size: timestamp_readback_bytes(TIMESTAMP_QUERY_COUNT),
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let mut slots = Vec::with_capacity(TIMESTAMP_READBACK_SLOTS);
        for index in 0..TIMESTAMP_READBACK_SLOTS {
            slots.push(TimestampReadbackSlot {
                buffer: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("oxide-webgpu-timestamp-readback"),
                    size: timestamp_readback_bytes(TIMESTAMP_QUERY_COUNT),
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                mapped: Rc::new(Cell::new(false)),
                failed: Rc::new(Cell::new(false)),
                state: TimestampReadbackState::Idle,
                frame_id: index as u64,
                query_count: 0,
                records: Vec::with_capacity(TIMESTAMP_MAX_PASSES as usize),
            });
        }
        Self {
            query_set,
            resolve_buffer,
            slots,
            next_slot: 0,
            current_records: Vec::with_capacity(TIMESTAMP_MAX_PASSES as usize),
            current_query_count: 0,
            timestamp_period_ns,
            latest: TimestampSummary::default(),
            completed: None,
            readback_interval_frames: TIMESTAMP_READBACK_INTERVAL_FRAMES,
            readback_skips: 0,
        }
    }

    fn begin_frame(&mut self) {
        self.current_records.clear();
        self.current_query_count = 0;
    }

    fn reserve(&mut self, family: TimestampPassFamily) -> Option<(u32, u32)> {
        if self.current_query_count.saturating_add(2) > TIMESTAMP_QUERY_COUNT {
            return None;
        }
        let begin_query = self.current_query_count;
        let end_query = begin_query + 1;
        self.current_query_count += 2;
        self.current_records.push(TimestampPassRecord { family, begin_query, end_query });
        Some((begin_query, end_query))
    }

    fn prepare_readback(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        frame_id: u64,
    ) -> Option<(usize, u64)> {
        if self.current_query_count == 0 {
            return None;
        }
        self.harvest();
        if frame_id % self.readback_interval_frames != 0 {
            return None;
        }
        let Some(slot_index) = self.next_idle_slot() else {
            self.readback_skips = self.readback_skips.saturating_add(1);
            return None;
        };
        let bytes = timestamp_readback_bytes(self.current_query_count);
        encoder.resolve_query_set(
            &self.query_set,
            0..self.current_query_count,
            &self.resolve_buffer,
            0,
        );
        encoder.copy_buffer_to_buffer(
            &self.resolve_buffer,
            0,
            &self.slots[slot_index].buffer,
            0,
            bytes,
        );
        let slot = &mut self.slots[slot_index];
        slot.records.clear();
        slot.records.extend_from_slice(&self.current_records);
        slot.frame_id = frame_id;
        slot.query_count = self.current_query_count;
        slot.mapped.set(false);
        slot.failed.set(false);
        slot.state = TimestampReadbackState::Pending;
        self.next_slot = (slot_index + 1) % self.slots.len().max(1);
        Some((slot_index, bytes))
    }

    fn map_readback(&mut self, slot_index: usize, bytes: u64) {
        let Some(slot) = self.slots.get_mut(slot_index) else {
            return;
        };
        let mapped = Rc::clone(&slot.mapped);
        let failed = Rc::clone(&slot.failed);
        slot.buffer.map_async(wgpu::MapMode::Read, 0..bytes, move |result| {
            failed.set(result.is_err());
            mapped.set(true);
        });
    }

    fn harvest(&mut self) {
        for slot in &mut self.slots {
            if slot.state != TimestampReadbackState::Pending || !slot.mapped.get() {
                continue;
            }
            if !slot.failed.get() {
                let bytes = timestamp_readback_bytes(slot.query_count);
                let view = slot.buffer.slice(0..bytes).get_mapped_range();
                let mut summary =
                    TimestampSummary { frame_id: slot.frame_id, ..TimestampSummary::default() };
                for record in &slot.records {
                    let Some(begin) = timestamp_sample(&view, record.begin_query) else {
                        continue;
                    };
                    let Some(end) = timestamp_sample(&view, record.end_query) else {
                        continue;
                    };
                    let ns = ((end.saturating_sub(begin) as f64) * self.timestamp_period_ns).round()
                        as u64;
                    summary.add(record.family, ns);
                }
                drop(view);
                self.latest = summary;
                if let Some(completed) = &mut self.completed {
                    if completed.len() == TIMESTAMP_COMPLETED_CAPACITY {
                        completed.pop_front();
                    }
                    completed.push_back(summary.sample());
                }
            }
            slot.buffer.unmap();
            slot.state = TimestampReadbackState::Idle;
            slot.query_count = 0;
            slot.records.clear();
        }
    }

    fn pending_count(&self) -> u32 {
        self.slots
            .iter()
            .filter(|slot| slot.state == TimestampReadbackState::Pending)
            .count()
            .min(u32::MAX as usize) as u32
    }

    fn buffer_bytes(&self) -> u64 {
        self.slots.iter().fold(self.resolve_buffer.size(), |total, slot| {
            total.saturating_add(slot.buffer.size())
        })
    }

    fn set_readback_interval_for_benchmark(&mut self, frames: u64) {
        self.readback_interval_frames = frames.max(1);
    }

    fn clear_completed(&mut self) {
        self.completed
            .get_or_insert_with(|| Box::new(VecDeque::with_capacity(TIMESTAMP_COMPLETED_CAPACITY)))
            .clear();
    }

    fn drain_completed_into(&mut self, output: &mut Vec<WebGpuTimestampSample>) {
        output.clear();
        if let Some(completed) = &mut self.completed {
            output.reserve(completed.len());
            output.extend(completed.drain(..));
        }
    }

    fn next_idle_slot(&self) -> Option<usize> {
        if self.slots.is_empty() {
            return None;
        }
        for offset in 0..self.slots.len() {
            let index = (self.next_slot + offset) % self.slots.len();
            if self.slots[index].state == TimestampReadbackState::Idle {
                return Some(index);
            }
        }
        None
    }
}

struct GpuPrograms {
    viewport_layout: wgpu::BindGroupLayout,
    texture_layout: wgpu::BindGroupLayout,
    effect_layout: wgpu::BindGroupLayout,
    scene3d_layout: wgpu::BindGroupLayout,
    id_mask_raster_layout: wgpu::BindGroupLayout,
    id_mask_wide: IdMaskVariantPrograms,
    id_mask_packed: Option<IdMaskVariantPrograms>,
    solid_pipeline: wgpu::RenderPipeline,
    rrect_pipeline: wgpu::RenderPipeline,
    image_rgba_pipeline: wgpu::RenderPipeline,
    image_a8_pipeline: wgpu::RenderPipeline,
    image_unit_vertex_buffer: wgpu::Buffer,
    image_unit_index_buffer: wgpu::Buffer,
    nine_slice_rgba_pipeline: wgpu::RenderPipeline,
    nine_slice_a8_pipeline: wgpu::RenderPipeline,
    nine_slice_unit_vertex_buffer: wgpu::Buffer,
    nine_slice_unit_index_buffer: wgpu::Buffer,
    spinner_pipeline: wgpu::RenderPipeline,
    neon_marker_pipeline: wgpu::RenderPipeline,
    rgba_pipeline: wgpu::RenderPipeline,
    a8_pipeline: wgpu::RenderPipeline,
    sdf_pipeline: wgpu::RenderPipeline,
    effect_pipeline: wgpu::RenderPipeline,
    scene3d_color_tri_depth_read_pipeline: wgpu::RenderPipeline,
    scene3d_color_tri_depth_write_pipeline: wgpu::RenderPipeline,
    scene3d_color_tri_pipeline: wgpu::RenderPipeline,
    scene3d_color_tri_add_depth_read_pipeline: wgpu::RenderPipeline,
    scene3d_color_tri_add_depth_write_pipeline: wgpu::RenderPipeline,
    scene3d_color_tri_add_pipeline: wgpu::RenderPipeline,
    id_mask_raster_pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
}

struct IdMaskVariantPrograms {
    field_layout: wgpu::BindGroupLayout,
    compositor_layout: wgpu::BindGroupLayout,
    field_seed_pipeline: wgpu::RenderPipeline,
    field_jump_pipeline: wgpu::RenderPipeline,
    compositor_pipeline: wgpu::RenderPipeline,
}

#[cfg(feature = "snapshot-tests")]
#[derive(Clone, Debug, PartialEq)]
pub struct WebIdMaskSnapshotReadback
{
   pub width: usize,
   pub height: usize,
   pub city: Vec<u8>,
   pub neighborhood: Vec<u8>,
   pub city_field: Vec<[f32; 4]>,
   pub seam_field: Vec<[f32; 4]>,
   pub packed_fields: bool,
   pub field_logical_bytes: u64,
   pub wide_field_logical_bytes: u64,
}

#[cfg(feature = "snapshot-tests")]
struct IdMaskReadbackPlane
{
   buffer: wgpu::Buffer,
   padded_row_bytes: u32,
   packed_row_bytes: u32,
}

#[cfg(feature = "snapshot-tests")]
struct PendingIdMaskReadback
{
   width: u32,
   height: u32,
   city: IdMaskReadbackPlane,
   neighborhood: IdMaskReadbackPlane,
   city_field: IdMaskReadbackPlane,
   seam_field: Option<IdMaskReadbackPlane>,
   packed_fields: bool,
   remaining: Rc<Cell<u8>>,
   failed: Rc<Cell<bool>>,
}

/// Browser renderer for production WebAssembly hosts.
///
/// WebGPU device creation is asynchronous in browsers. If WebGPU is unavailable, construction
/// returns `RenderError::Unsupported` instead of falling back to a CPU/Canvas2D visual path.
pub struct BrowserRenderer {
    inner: WebGpuRenderer,
}

impl BrowserRenderer {
    pub async fn from_canvas_id_webgpu(id: &str) -> Result<Self, api::RenderError> {
        let canvas = canvas_by_id(id)?;
        Self::from_canvas_webgpu(canvas).await
    }

    pub async fn from_canvas_webgpu(canvas: HtmlCanvasElement) -> Result<Self, api::RenderError> {
        if !browser_webgpu_present() {
            return Err(api::RenderError::Unsupported("webgpu unavailable"));
        }
        WebGpuRenderer::from_canvas(canvas).await.map(|inner| Self { inner })
    }

    #[must_use]
    pub fn backend_name(&self) -> &'static str {
        "webgpu"
    }

    #[must_use]
    pub fn canvas(&self) -> HtmlCanvasElement {
        self.inner.canvas()
    }

    #[must_use]
    pub fn last_stats(&self) -> WebRendererStats {
        self.inner.last_stats()
    }

    pub fn collect_timestamp_readbacks(&mut self) -> WebRendererStats {
        self.inner.collect_timestamp_readbacks()
    }

    #[must_use]
    pub fn pending_timestamp_readbacks(&self) -> u32 {
        self.inner.pending_timestamp_readbacks()
    }

    pub fn set_timestamp_readback_interval_for_benchmark(&mut self, frames: u64) {
        self.inner.set_timestamp_readback_interval_for_benchmark(frames);
    }

    pub fn set_memory_stats_interval_for_benchmark(&mut self, frames: u64) {
        self.inner.set_memory_stats_interval_for_benchmark(frames);
    }

    pub fn set_memory_stats_enabled_for_benchmark(&mut self, enabled: bool) {
        self.inner.set_memory_stats_enabled_for_benchmark(enabled);
    }

    /// Creates only the auxiliary targets required by the app's declared first-use features.
    ///
    /// Apps that need deterministic first-interaction latency may call this outside the frame
    /// path. Direct UI should leave both flags false and retains no auxiliary targets.
    pub fn prewarm_auxiliary_targets(&mut self, backdrop: bool, scene3d: bool) {
        self.inner.prewarm_auxiliary_targets(backdrop, scene3d);
    }

    #[cfg(feature = "snapshot-tests")]
    pub fn begin_id_mask_snapshot_readback(&mut self) -> Result<(), api::RenderError>
    {
        self.inner.begin_id_mask_snapshot_readback()
    }

    #[cfg(feature = "snapshot-tests")]
    pub fn collect_id_mask_snapshot_readback(
        &mut self,
    ) -> Option<Result<WebIdMaskSnapshotReadback, api::RenderError>>
    {
        self.inner.collect_id_mask_snapshot_readback()
    }

    pub fn queue_completion_flag_for_benchmark(&self) -> Arc<AtomicBool> {
        self.inner.queue_completion_flag_for_benchmark()
    }

    pub fn clear_completed_timestamp_samples(&mut self) {
        self.inner.clear_completed_timestamp_samples();
    }

    pub fn drain_completed_timestamp_samples_into(
        &mut self,
        output: &mut Vec<WebGpuTimestampSample>,
    ) {
        self.inner.drain_completed_timestamp_samples_into(output);
    }

    pub fn set_cpu_submit_timing_enabled_for_benchmark(&mut self, enabled: bool) {
        self.inner.set_cpu_submit_timing_enabled_for_benchmark(enabled);
    }

    pub fn set_animation_time_ms(&mut self, time_ms: f64)
    {
        self.inner.set_animation_time_ms(time_ms);
    }

    #[must_use]
    pub fn last_cpu_submit_timing(&self) -> WebGpuCpuSubmitTimingSample {
        self.inner.last_cpu_submit_timing()
    }

    pub fn set_draw_state_cache_enabled_for_benchmark(&mut self, enabled: bool) {
        self.inner.set_draw_state_cache_enabled_for_benchmark(enabled);
    }

    pub fn set_draw_item_coalescing_enabled_for_benchmark(&mut self, enabled: bool) {
        self.inner.set_draw_item_coalescing_enabled_for_benchmark(enabled);
    }

    pub fn set_image_upload_scratch_enabled_for_benchmark(&mut self, enabled: bool) {
        self.inner.set_image_upload_scratch_enabled_for_benchmark(enabled);
    }

    pub fn set_effect_uniform_batch_enabled_for_benchmark(&mut self, enabled: bool) {
        self.inner.set_effect_uniform_batch_enabled_for_benchmark(enabled);
    }

    pub fn set_backdrop_batch_enabled_for_benchmark(&mut self, enabled: bool) {
        self.inner.set_backdrop_batch_enabled_for_benchmark(enabled);
    }

    pub fn set_direct_surface_enabled_for_benchmark(&mut self, enabled: bool) {
        self.inner.set_direct_surface_enabled_for_benchmark(enabled);
    }

    /// Encodes a retained snapshot through persistent WebGPU buffers and eligible bundles.
    pub fn encode_snapshot(
        &mut self,
        snapshot: &api::RenderSnapshot,
    ) -> Result<(), api::RenderSnapshotError> {
        self.inner.encode_snapshot(snapshot)
    }

    #[must_use]
    pub fn prepared_cache_resident_bytes(&self) -> u64 {
        self.inner.prepared_cache_resident_bytes()
    }

    pub fn set_prepared_cache_budget_bytes(&mut self, budget_bytes: u64) {
        self.inner.set_prepared_cache_budget_bytes(budget_bytes);
    }

    pub fn purge_prepared_chunks(&mut self) {
        self.inner.purge_prepared_chunks();
    }

    #[must_use]
    pub fn layer_cache_budget_bytes(&self) -> u64 {
        self.inner.layer_cache_budget_bytes()
    }

    pub fn set_layer_cache_budget_bytes(&mut self, budget_bytes: u64) {
        self.inner.set_layer_cache_budget_bytes(budget_bytes);
    }

    pub fn purge_layer_cache(&mut self) {
        self.inner.purge_layer_cache();
    }

    pub fn purge_layer_cache_for_memory_pressure(&mut self) {
        self.inner.purge_layer_cache_for_memory_pressure();
    }

    pub fn purge_layer_cache_for_device_loss_for_benchmark(&mut self) {
        self.inner.purge_layer_cache_for_reason(LAYER_PURGE_DEVICE_LOSS);
    }

    #[must_use]
    pub fn id_mask_cache_budget_bytes(&self) -> u64 {
        self.inner.id_mask_cache_budget_bytes()
    }

    #[must_use]
    pub fn id_mask_target_bytes_per_pixel(&self) -> u64 {
        self.inner.id_mask_target_bytes_per_pixel()
    }

    #[must_use]
    pub fn id_mask_packed_fields_supported(&self) -> bool {
        self.inner.id_mask_packed_fields_supported()
    }

    pub fn set_id_mask_cache_budget_bytes(&mut self, budget_bytes: u64) {
        self.inner.set_id_mask_cache_budget_bytes(budget_bytes);
    }

    pub fn purge_id_mask_field_cache(&mut self) {
        self.inner.purge_id_mask_field_cache();
    }

    pub fn purge_id_mask_field_cache_for_memory_pressure(&mut self) {
        self.inner.purge_id_mask_field_cache_for_memory_pressure();
    }

    pub fn purge_id_mask_field_cache_for_device_loss_for_benchmark(&mut self) {
        self.inner
            .purge_id_mask_field_cache_for_reason(LAYER_PURGE_DEVICE_LOSS);
    }

    pub fn set_prepared_bundle_min_draws_for_benchmark(&mut self, draws: usize) {
        self.inner.set_prepared_bundle_min_draws_for_benchmark(draws);
    }

    pub fn advance_prepared_device_generation_for_benchmark(&mut self) {
        self.inner.advance_prepared_device_generation_for_benchmark();
    }

    #[must_use]
    pub fn image_create_rgba8(
        &mut self,
        width: u32,
        height: u32,
        data: &[u8],
        row_bytes: usize,
    ) -> api::ImageHandle {
        self.inner.image_create_rgba8(width, height, data, row_bytes)
    }

    #[must_use]
    pub fn image_create_a8(
        &mut self,
        width: u32,
        height: u32,
        data: &[u8],
        row_bytes: usize,
    ) -> api::ImageHandle {
        self.inner.image_create_a8(width, height, data, row_bytes)
    }

    pub fn image_update_a8(
        &mut self,
        handle: api::ImageHandle,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        data: &[u8],
        row_bytes: usize,
    ) {
        self.inner.image_update_a8(handle, x, y, width, height, data, row_bytes);
    }

    pub fn image_update_rgba8(
        &mut self,
        handle: api::ImageHandle,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        data: &[u8],
        row_bytes: usize,
    ) -> Result<(), api::RenderError> {
        self.inner.try_image_update_rgba8(handle, x, y, width, height, data, row_bytes)
    }

   /// Releases a renderer-owned image and invalidates its handle.
   pub fn image_release(&mut self, handle: api::ImageHandle) -> bool
   {
      self.inner.image_release(handle)
   }

    pub fn mesh3d_create_colored(
        &mut self,
        data: &scene3d::MeshColor3dData<'_>,
    ) -> Result<scene3d::MeshHandle3d, api::RenderError> {
        self.inner.mesh3d_create_colored(data)
    }

    pub fn mesh3d_release(&mut self, handle: scene3d::MeshHandle3d) {
        self.inner.mesh3d_release(handle);
    }

    pub fn encode_scene3d(&mut self, pass: &scene3d::Pass3d<'_>) -> Result<(), api::RenderError> {
        self.inner.encode_scene3d(pass)
    }

    pub fn encode_id_mask_gpu_compositor(
        &mut self,
        pass: &id_mask_compositor::IdMaskGpuCompositorPass<'_>,
    ) -> Result<(), api::RenderError> {
        self.inner.encode_id_mask_gpu_compositor(pass)
    }

    pub fn encode_neon_markers(
        &mut self,
        pass: &neon_marker::NeonMarkerPass<'_>,
    ) -> Result<(), api::RenderError> {
        self.inner.encode_neon_markers(pass)
    }
}

impl api::Renderer for BrowserRenderer {
    fn device_caps(&self) -> api::DeviceCaps {
        self.inner.device_caps()
    }

    fn begin_frame(
        &mut self,
        fb: &api::FrameTarget,
        damage: Option<&api::Damage>,
    ) -> api::FrameToken {
        self.inner.begin_frame(fb, damage)
    }

    fn encode_pass(&mut self, list: &api::DrawList) {
        self.inner.encode_pass(list);
    }

    fn submit(&mut self, token: api::FrameToken) -> Result<(), api::RenderError> {
        self.inner.submit(token)
    }

    fn resize(&mut self, width: u32, height: u32, scale: f32) -> Result<(), api::RenderError> {
        self.inner.resize(width, height, scale)
    }
}

#[derive(Clone, Copy, Default)]
struct ScratchCapacityBreakdown {
    draw: usize,
    scene3d: usize,
    effect: usize,
    id_mask: usize,
    image_upload: usize,
    resource_table: usize,
}

#[derive(Clone, Copy, Default)]
struct WebGpuMemorySnapshot {
    logical_total_bytes: u64,
    vertex_buffer_bytes: u64,
    index_buffer_bytes: u64,
    uniform_buffer_bytes: u64,
    persistent_asset_bytes: u64,
    transient_target_bytes: u64,
    depth_target_bytes: u64,
    bloom_target_bytes: u64,
    layer_texture_bytes: u64,
    id_mask_texture_bytes: u64,
    atlas_texture_bytes: u64,
    image_texture_bytes: u64,
    scene3d_mesh_bytes: u64,
    staging_buffer_bytes: u64,
    bind_buffer_bytes: u64,
    frame_ring_bytes: u64,
    cache_bytes: u64,
}

impl ScratchCapacityBreakdown {
    fn total(self) -> usize {
        self.draw
            .saturating_add(self.scene3d)
            .saturating_add(self.effect)
            .saturating_add(self.id_mask)
            .saturating_add(self.image_upload)
            .saturating_add(self.resource_table)
    }
}

/// WebGPU implementation of the Oxide browser renderer.
pub struct WebGpuRenderer {
    canvas: HtmlCanvasElement,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    programs: GpuPrograms,
    scene_target: Option<GpuColorTarget>,
    scene_depth_target: Option<GpuDepthTarget>,
    scratch_target: Option<GpuColorTarget>,
    viewport_buffer: wgpu::Buffer,
    viewport_bind_group: wgpu::BindGroup,
    effect_buffer: wgpu::Buffer,
    effect_bind_group: wgpu::BindGroup,
    effect_uniform_capacity: u64,
    effect_uniform_stride: u64,
    vertex_buffer: Option<wgpu::Buffer>,
    vertex_capacity: u64,
    rrect_instance_buffer: Option<wgpu::Buffer>,
    rrect_instance_capacity: u64,
    image_instance_buffer: Option<wgpu::Buffer>,
    image_instance_capacity: u64,
    nine_slice_instance_buffer: Option<wgpu::Buffer>,
    nine_slice_instance_capacity: u64,
    spinner_instance_buffer: Option<wgpu::Buffer>,
    spinner_instance_capacity: u64,
    neon_marker_instance_buffer: Option<wgpu::Buffer>,
    neon_marker_instance_capacity: u64,
    animation_phase: f32,
    index_buffer_u16: Option<wgpu::Buffer>,
    index_capacity_u16: u64,
    index_buffer_u32: Option<wgpu::Buffer>,
    index_capacity_u32: u64,
    scene3d_uniform_buffer: Option<wgpu::Buffer>,
    scene3d_uniform_capacity: u64,
    scene3d_bind_group: Option<wgpu::BindGroup>,
    present_vertex_buffer: wgpu::Buffer,
    present_index_buffer: wgpu::Buffer,
    present_width: u32,
    present_height: u32,
    present_scale: f32,
    scene3d_uniform_bytes: Vec<u8>,
    effect_uniform_bytes: Vec<u8>,
    scene3d_draws: Vec<Scene3dDraw>,
    scene3d_overlay_draws: Vec<Scene3dDraw>,
    id_mask_draws: Vec<IdMaskDraw>,
    id_mask_draw_chunk_indices: Vec<usize>,
    id_mask_draw_chunk_keys: Vec<IdMaskChunkKey>,
    id_mask_vertex_caches: Vec<IdMaskVertexCache>,
    id_mask_field_cache: Vec<IdMaskFieldCacheEntry>,
    id_mask_resolved_draws: Vec<IdMaskResolvedDraw>,
    id_mask_cache_budget_bytes: u64,
    id_mask_cache_resident_bytes: u64,
    id_mask_cache_evictions: u64,
    id_mask_cache_purges: u64,
    id_mask_cache_last_purge_reason: u8,
    id_mask_uniform_buffer: Option<wgpu::Buffer>,
    id_mask_uniform_capacity: u64,
    id_mask_raster_bind_group: Option<wgpu::BindGroup>,
    #[cfg(feature = "snapshot-tests")]
    id_mask_snapshot_readback: Option<PendingIdMaskReadback>,
    #[cfg(feature = "snapshot-tests")]
    id_mask_snapshot_targets: Option<IdMaskRenderTargets>,
    scene3d_clear_color: Option<api::Color>,
    scene3d_clear_depth: bool,
    scene3d_active: bool,
    images: ImageSlots<GpuImage>,
    layers: BTreeMap<u32, GpuLayer>,
    layer_pool: Vec<PooledGpuLayer>,
    layer_frame_ids: HashSet<u32>,
    layer_cache_budget_bytes: u64,
    layer_cache_resident_bytes: u64,
    layer_cache_pool_bytes: u64,
    layer_cache_pool_reuses: u64,
    layer_cache_evictions: u64,
    layer_cache_recreations: u64,
    layer_cache_purges: u64,
    layer_cache_last_purge_reason: u8,
    meshes_3d: Vec<Option<GpuMesh3d>>,
    frame: FrameData,
    prepared_chunks: PreparedChunkCache,
    prepared_property_ring: PreparedPropertyRing,
    prepared_frame_plan: Vec<PreparedFrameInstance>,
    prepared_layer_key_indices: HashMap<u32, usize>,
    prepared_layer_plan: Vec<PreparedLayerPlanEntry>,
    prepared_layer_snapshot: Option<api::RenderSnapshot>,
    prepared_snapshot_bundle: Option<PreparedSnapshotBundle>,
    prepared_fallback: api::DrawList,
    prepared_frame_active: bool,
    prepared_snapshot_bundle_active: bool,
    prepared_device_generation: u64,
    prepared_bundle_generation: u64,
    prepared_bundle_min_draws: usize,
    scratch_vertices: Vec<PackedVertex>,
    scratch_indices: Vec<u32>,
    image_upload_scratch: Vec<u8>,
    id_mask_uniform_bytes: Vec<u8>,
    id_mask_uniform_offsets: Vec<IdMaskUniformOffsets>,
    id_mask_field_uniform_offsets: Vec<u32>,
    clip_stack: Vec<api::RectI>,
    target_stack: Vec<u32>,
    width: u32,
    height: u32,
    scale: f32,
    frame_id: u64,
    frame_scratch_capacity: ScratchCapacityBreakdown,
    frame_scratch_capacity_bytes: usize,
    active_token: Option<api::FrameToken>,
    stats: WebRendererStats,
    timestamp_queries: Option<WebGpuTimestampQueries>,
    draw_state_cache_enabled: bool,
    draw_item_coalescing_enabled: bool,
    image_upload_scratch_enabled: bool,
    effect_uniform_batch_enabled: bool,
    backdrop_batch_enabled: bool,
    direct_surface_enabled: bool,
    cpu_submit_timing_enabled: bool,
    cpu_submit_timing: WebGpuCpuSubmitTimingSample,
    memory_stats_interval: u64,
    memory_stats_enabled: bool,
    memory_snapshot: WebGpuMemorySnapshot,
}

fn image_for_update<'a>(
    images: &'a ImageSlots<GpuImage>,
    handle: api::ImageHandle,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    kind: GpuImageKind,
) -> Result<&'a GpuImage, api::RenderError> {
    let Some(image) = images.get(handle.0) else {
        return Err(api::RenderError::ResourceNotFound("image handle not found"));
    };
    if core::mem::discriminant(&image.kind) != core::mem::discriminant(&kind) {
        return Err(api::RenderError::InvalidOperation("image kind mismatch"));
    }
    if x.saturating_add(width) > image.width || y.saturating_add(height) > image.height {
        return Err(api::RenderError::InvalidOperation("image update outside bounds"));
    }
    Ok(image)
}

fn write_image_update(
    queue: &wgpu::Queue,
    stats: &mut WebRendererStats,
    image: &GpuImage,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    data: &[u8],
    row_bytes: u32,
) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &image.texture,
            mip_level: 0,
            origin: wgpu::Origin3d { x, y, z: 0 },
            aspect: wgpu::TextureAspect::All,
        },
        data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(row_bytes),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    let upload_bytes = u64::from(width)
        .saturating_mul(u64::from(height))
        .saturating_mul(u64::from(image.kind.bytes_per_pixel()));
    stats.texture_upload_bytes = stats.texture_upload_bytes.saturating_add(upload_bytes);
}

fn image_row_bytes(
    width: u32,
    height: u32,
    kind: GpuImageKind,
    data: &[u8],
    row_bytes: usize,
) -> Result<u32, api::RenderError> {
    let packed_row_bytes = (width as usize)
        .checked_mul(kind.bytes_per_pixel() as usize)
        .ok_or(api::RenderError::InvalidOperation("image row size overflow"))?;
    if row_bytes < packed_row_bytes {
        return Err(api::RenderError::InvalidOperation("invalid image rows"));
    }
    let required = if height == 0 {
        0
    } else {
        row_bytes
            .checked_mul(height.saturating_sub(1) as usize)
            .and_then(|prefix| prefix.checked_add(packed_row_bytes))
            .ok_or(api::RenderError::InvalidOperation("image byte size overflow"))?
    };
    if data.len() < required {
        return Err(api::RenderError::InvalidOperation("invalid image rows"));
    }
    u32::try_from(row_bytes)
        .map_err(|_| api::RenderError::InvalidOperation("image row stride exceeds WebGPU limit"))
}

fn color_texture_bytes_per_pixel(format: wgpu::TextureFormat) -> u64 {
    match format {
        wgpu::TextureFormat::Rgba16Float | wgpu::TextureFormat::Rgba16Uint => 8,
        _ => 4,
    }
}

fn id_mask_field_texture_usage() -> wgpu::TextureUsages {
    let mut usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
    #[cfg(feature = "snapshot-tests")]
    {
        usage |= wgpu::TextureUsages::COPY_SRC;
    }
    usage
}

fn id_mask_packed_format_supported(features: wgpu::TextureFormatFeatures) -> bool {
    features.allowed_usages.contains(id_mask_field_texture_usage())
}

const fn id_mask_packed_coordinates_fit(width: u32, height: u32) -> bool {
    width <= u16::MAX as u32 && height <= u16::MAX as u32
}

const _: () = {
    assert!(id_mask_packed_coordinates_fit(u16::MAX as u32, u16::MAX as u32));
    assert!(!id_mask_packed_coordinates_fit(u16::MAX as u32 + 1, 1));
    assert!(!id_mask_packed_coordinates_fit(1, u16::MAX as u32 + 1));
};

impl WebGpuRenderer {
    pub async fn from_canvas_id(id: &str) -> Result<Self, api::RenderError> {
        Self::from_canvas(canvas_by_id(id)?).await
    }

    fn sample_memory_stats(&mut self) {
        let color_bytes = color_texture_bytes_per_pixel(self.config.format);
        let target_bytes =
            saturating_texture_bytes(u64::from(self.width), u64::from(self.height), color_bytes);
        let transient_target_count = u64::from(self.scene_target.is_some())
            .saturating_add(u64::from(self.scratch_target.is_some()));
        let transient_target_bytes = target_bytes.saturating_mul(transient_target_count);
        let depth_target_bytes = if self.scene_depth_target.is_some() {
            saturating_texture_bytes(u64::from(self.width), u64::from(self.height), 4)
        } else {
            0
        };
        let layer_texture_bytes = self
            .layers
            .values()
            .map(|layer| layer.bytes)
            .chain(self.layer_pool.iter().map(|entry| entry.layer.bytes))
            .fold(0_u64, u64::saturating_add);
        let mut atlas_texture_bytes = 0_u64;
        let mut image_texture_bytes = 0_u64;
        for image in self.images.values() {
            let bytes = saturating_texture_bytes(
                u64::from(image.width),
                u64::from(image.height),
                u64::from(image.kind.bytes_per_pixel()),
            );
            match image.kind {
                GpuImageKind::Rgba => {
                    image_texture_bytes = image_texture_bytes.saturating_add(bytes);
                }
                GpuImageKind::A8 => {
                    atlas_texture_bytes = atlas_texture_bytes.saturating_add(bytes);
                }
            }
        }
        let id_mask_texture_bytes = self.id_mask_field_cache.iter().fold(0_u64, |total, entry| {
            total.saturating_add(entry.bytes)
        });
        let id_mask_vertex_bytes = self.id_mask_vertex_caches.iter().fold(0_u64, |total, cache| {
            total.saturating_add(cache.buffer.as_ref().map_or(0, wgpu::Buffer::size))
        });
        let vertex_buffer_bytes = self
            .vertex_buffer
            .as_ref()
            .map_or(0, wgpu::Buffer::size)
            .saturating_add(
                self.rrect_instance_buffer.as_ref().map_or(0, wgpu::Buffer::size),
            )
            .saturating_add(
                self.image_instance_buffer.as_ref().map_or(0, wgpu::Buffer::size),
            )
            .saturating_add(
                self.nine_slice_instance_buffer.as_ref().map_or(0, wgpu::Buffer::size),
            )
            .saturating_add(
                self.spinner_instance_buffer.as_ref().map_or(0, wgpu::Buffer::size),
            )
            .saturating_add(
                self.neon_marker_instance_buffer.as_ref().map_or(0, wgpu::Buffer::size),
            )
            .saturating_add(id_mask_vertex_bytes)
            .saturating_add(self.prepared_chunks.vertex_bytes());
        let index_buffer_bytes = self
            .index_buffer_u16
            .as_ref()
            .map_or(0, wgpu::Buffer::size)
            .saturating_add(
                self.index_buffer_u32
                    .as_ref()
                    .map_or(0, wgpu::Buffer::size),
            )
            .saturating_add(self.prepared_chunks.index_bytes());
        let uniform_buffer_bytes = self
            .viewport_buffer
            .size()
            .saturating_add(self.effect_buffer.size())
            .saturating_add(
                self.scene3d_uniform_buffer.as_ref().map_or(0, wgpu::Buffer::size),
            )
            .saturating_add(
                self.id_mask_uniform_buffer
                    .as_ref()
                    .map_or(0, wgpu::Buffer::size),
            )
            .saturating_add(self.prepared_property_ring.byte_size());
        let persistent_asset_bytes = self
            .present_vertex_buffer
            .size()
            .saturating_add(self.present_index_buffer.size())
            .saturating_add(self.programs.image_unit_vertex_buffer.size())
            .saturating_add(self.programs.image_unit_index_buffer.size())
            .saturating_add(self.programs.nine_slice_unit_vertex_buffer.size())
            .saturating_add(self.programs.nine_slice_unit_index_buffer.size());
        let scene3d_mesh_bytes = self.meshes_3d.iter().flatten().fold(0_u64, |total, mesh| {
            total
                .saturating_add(mesh.vertex_buffer.size())
                .saturating_add(mesh.index_buffer.size())
        });
        let staging_buffer_bytes = self
            .timestamp_queries
            .as_ref()
            .map_or(0, WebGpuTimestampQueries::buffer_bytes);
        let cache_bytes = layer_texture_bytes
            .saturating_add(atlas_texture_bytes)
            .saturating_add(image_texture_bytes)
            .saturating_add(id_mask_vertex_bytes)
            .saturating_add(id_mask_texture_bytes)
            .saturating_add(self.prepared_chunks.resident_bytes);
        let logical_total_bytes = vertex_buffer_bytes
            .saturating_add(index_buffer_bytes)
            .saturating_add(uniform_buffer_bytes)
            .saturating_add(persistent_asset_bytes)
            .saturating_add(transient_target_bytes)
            .saturating_add(depth_target_bytes)
            .saturating_add(layer_texture_bytes)
            .saturating_add(id_mask_texture_bytes)
            .saturating_add(atlas_texture_bytes)
            .saturating_add(image_texture_bytes)
            .saturating_add(scene3d_mesh_bytes)
            .saturating_add(staging_buffer_bytes);
        self.memory_snapshot = WebGpuMemorySnapshot {
            logical_total_bytes,
            vertex_buffer_bytes,
            index_buffer_bytes,
            uniform_buffer_bytes,
            persistent_asset_bytes,
            transient_target_bytes,
            depth_target_bytes,
            bloom_target_bytes: 0,
            layer_texture_bytes,
            id_mask_texture_bytes,
            atlas_texture_bytes,
            image_texture_bytes,
            scene3d_mesh_bytes,
            staging_buffer_bytes,
            bind_buffer_bytes: 0,
            frame_ring_bytes: self.prepared_property_ring.byte_size(),
            cache_bytes,
        };
    }

    fn apply_memory_stats(&mut self) {
        let memory = self.memory_snapshot;
        self.stats.gpu_allocated_bytes_available = false;
        self.stats.gpu_logical_total_bytes = memory.logical_total_bytes;
        self.stats.gpu_allocated_total_bytes = 0;
        self.stats.gpu_vertex_buffer_bytes = memory.vertex_buffer_bytes;
        self.stats.gpu_index_buffer_bytes = memory.index_buffer_bytes;
        self.stats.gpu_uniform_buffer_bytes = memory.uniform_buffer_bytes;
        self.stats.gpu_persistent_asset_bytes = memory.persistent_asset_bytes;
        self.stats.gpu_transient_target_bytes = memory.transient_target_bytes;
        self.stats.gpu_depth_target_bytes = memory.depth_target_bytes;
        self.stats.gpu_bloom_target_bytes = memory.bloom_target_bytes;
        self.stats.gpu_layer_texture_bytes = memory.layer_texture_bytes;
        self.stats.gpu_id_mask_texture_bytes = memory.id_mask_texture_bytes;
        self.stats.gpu_atlas_texture_bytes = memory.atlas_texture_bytes;
        self.stats.gpu_image_texture_bytes = memory.image_texture_bytes;
        self.stats.gpu_scene3d_mesh_bytes = memory.scene3d_mesh_bytes;
        self.stats.gpu_staging_buffer_bytes = memory.staging_buffer_bytes;
        self.stats.gpu_bind_buffer_bytes = memory.bind_buffer_bytes;
        self.stats.gpu_frame_ring_bytes = memory.frame_ring_bytes;
        self.stats.gpu_cache_bytes = memory.cache_bytes;
    }

    pub async fn from_canvas(canvas: HtmlCanvasElement) -> Result<Self, api::RenderError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..Default::default()
        });
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(|err| {
                api::RenderError::Unsupported(match err {
                    _ => "webgpu surface unavailable",
                })
            })?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|_| api::RenderError::Unsupported("webgpu adapter unavailable"))?;
        let timestamp_query_supported =
            adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        let packed_id_mask_fields = id_mask_packed_format_supported(
            adapter.get_texture_format_features(ID_MASK_PACKED_FIELD_FORMAT),
        );
        let required_features = if timestamp_query_supported {
            wgpu::Features::TIMESTAMP_QUERY
        } else {
            wgpu::Features::empty()
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("oxide-webgpu-device"),
                required_features,
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            })
            .await
            .map_err(|err| api::RenderError::Io(format!("webgpu device unavailable: {err}")))?;
        let timestamp_queries = if timestamp_query_supported {
            Some(WebGpuTimestampQueries::new(&device, queue.get_timestamp_period() as f64))
        } else {
            None
        };
        let width = canvas.width().max(1);
        let height = canvas.height().max(1);
        let mut config = surface
            .get_default_config(&adapter, width, height)
            .ok_or(api::RenderError::Unsupported("webgpu surface format unavailable"))?;
        config.width = width;
        config.height = height;
        config.usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
        config.desired_maximum_frame_latency = 1;
        config.alpha_mode = wgpu::CompositeAlphaMode::PreMultiplied;
        surface.configure(&device, &config);

        let programs = create_programs(&device, config.format, packed_id_mask_fields);
        let (viewport_buffer, viewport_bind_group) = create_viewport_bind_group(&device, &programs);
        let prepared_property_ring = PreparedPropertyRing::new(&device, &programs);
        write_viewport_uniform(&queue, &viewport_buffer, width, height, 1.0, 0.0);
        let effect_uniform_stride = align_to(
            EFFECT_UNIFORM_SIZE,
            device.limits().min_uniform_buffer_offset_alignment.max(EFFECT_UNIFORM_SIZE as u32)
                as u64,
        );
        let (effect_buffer, effect_bind_group, effect_uniform_capacity) =
            create_effect_bind_group(&device, &programs, EFFECT_UNIFORM_SIZE);
        let present_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("oxide-webgpu-present-vertices"),
            size: 4 * VERTEX_STRIDE,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let present_index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("oxide-webgpu-present-indices"),
            size: 6 * 4,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layer_cache_budget_bytes = saturating_texture_bytes(
            u64::from(width),
            u64::from(height),
            color_texture_bytes_per_pixel(config.format),
        )
        .saturating_mul(8)
        .clamp(LAYER_CACHE_MIN_BUDGET_BYTES, LAYER_CACHE_MAX_BUDGET_BYTES);

        Ok(Self {
            canvas,
            surface,
            device,
            queue,
            config,
            programs,
            scene_target: None,
            scene_depth_target: None,
            scratch_target: None,
            viewport_buffer,
            viewport_bind_group,
            effect_buffer,
            effect_bind_group,
            effect_uniform_capacity,
            effect_uniform_stride,
            vertex_buffer: None,
            vertex_capacity: 0,
            rrect_instance_buffer: None,
            rrect_instance_capacity: 0,
            image_instance_buffer: None,
            image_instance_capacity: 0,
            nine_slice_instance_buffer: None,
            nine_slice_instance_capacity: 0,
            spinner_instance_buffer: None,
            spinner_instance_capacity: 0,
            neon_marker_instance_buffer: None,
            neon_marker_instance_capacity: 0,
            animation_phase: 0.0,
            index_buffer_u16: None,
            index_capacity_u16: 0,
            index_buffer_u32: None,
            index_capacity_u32: 0,
            scene3d_uniform_buffer: None,
            scene3d_uniform_capacity: 0,
            scene3d_bind_group: None,
            present_vertex_buffer,
            present_index_buffer,
            present_width: 0,
            present_height: 0,
            present_scale: 0.0,
            scene3d_uniform_bytes: Vec::new(),
            effect_uniform_bytes: Vec::new(),
            scene3d_draws: Vec::new(),
            scene3d_overlay_draws: Vec::new(),
            id_mask_draws: Vec::new(),
            id_mask_draw_chunk_indices: Vec::new(),
            id_mask_draw_chunk_keys: Vec::new(),
            id_mask_vertex_caches: Vec::new(),
            id_mask_field_cache: Vec::new(),
            id_mask_resolved_draws: Vec::new(),
            id_mask_cache_budget_bytes: saturating_texture_bytes(
                u64::from(width),
                u64::from(height),
                id_mask_target_bytes_per_pixel(packed_id_mask_fields),
            )
            .saturating_mul(8)
            .clamp(
                ID_MASK_FIELD_CACHE_MIN_BUDGET_BYTES,
                ID_MASK_FIELD_CACHE_MAX_BUDGET_BYTES,
            ),
            id_mask_cache_resident_bytes: 0,
            id_mask_cache_evictions: 0,
            id_mask_cache_purges: 0,
            id_mask_cache_last_purge_reason: LAYER_PURGE_NONE,
            id_mask_uniform_buffer: None,
            id_mask_uniform_capacity: 0,
            id_mask_raster_bind_group: None,
            #[cfg(feature = "snapshot-tests")]
            id_mask_snapshot_readback: None,
            #[cfg(feature = "snapshot-tests")]
            id_mask_snapshot_targets: None,
            scene3d_clear_color: None,
            scene3d_clear_depth: true,
            scene3d_active: false,
            images: ImageSlots::new(),
            layers: BTreeMap::new(),
            layer_pool: Vec::new(),
            layer_frame_ids: HashSet::new(),
            layer_cache_budget_bytes,
            layer_cache_resident_bytes: 0,
            layer_cache_pool_bytes: 0,
            layer_cache_pool_reuses: 0,
            layer_cache_evictions: 0,
            layer_cache_recreations: 0,
            layer_cache_purges: 0,
            layer_cache_last_purge_reason: LAYER_PURGE_NONE,
            meshes_3d: vec![None],
            frame: FrameData {
                geometry: PackedGeometry::default(),
                rrect_instances: Vec::new(),
                image_instances: Vec::new(),
                nine_slice_instances: Vec::new(),
                spinner_instances: Vec::new(),
                neon_marker_instances: Vec::new(),
                draws: Vec::new(),
                layer_passes: Vec::new(),
                effect_count: 0,
                effect_first_sigma_bits: 0,
                effect_shared_sigma: 0.0,
                effect_single_uniform_slot: true,
            },
            prepared_chunks: PreparedChunkCache::default(),
            prepared_property_ring,
            prepared_frame_plan: Vec::new(),
            prepared_layer_key_indices: HashMap::new(),
            prepared_layer_plan: Vec::new(),
            prepared_layer_snapshot: None,
            prepared_snapshot_bundle: None,
            prepared_fallback: api::DrawList::default(),
            prepared_frame_active: false,
            prepared_snapshot_bundle_active: false,
            prepared_device_generation: 1,
            prepared_bundle_generation: 1,
            prepared_bundle_min_draws: PREPARED_BUNDLE_DEFAULT_MIN_DRAWS,
            scratch_vertices: Vec::new(),
            scratch_indices: Vec::new(),
            image_upload_scratch: Vec::new(),
            id_mask_uniform_bytes: Vec::new(),
            id_mask_uniform_offsets: Vec::new(),
            id_mask_field_uniform_offsets: Vec::new(),
            clip_stack: Vec::new(),
            target_stack: Vec::new(),
            width,
            height,
            scale: 1.0,
            frame_id: 0,
            frame_scratch_capacity: ScratchCapacityBreakdown::default(),
            frame_scratch_capacity_bytes: 0,
            active_token: None,
            stats: WebRendererStats::default(),
            timestamp_queries,
            draw_state_cache_enabled: true,
            draw_item_coalescing_enabled: true,
            image_upload_scratch_enabled: true,
            effect_uniform_batch_enabled: true,
            backdrop_batch_enabled: true,
            direct_surface_enabled: true,
            cpu_submit_timing_enabled: false,
            cpu_submit_timing: WebGpuCpuSubmitTimingSample::default(),
            memory_stats_interval: 60,
            memory_stats_enabled: true,
            memory_snapshot: WebGpuMemorySnapshot::default(),
        })
    }

    #[must_use]
    pub fn canvas(&self) -> HtmlCanvasElement {
        self.canvas.clone()
    }

    #[must_use]
    pub fn last_stats(&self) -> WebRendererStats {
        self.stats
    }

    pub fn collect_timestamp_readbacks(&mut self) -> WebRendererStats {
        if let Some(timestamps) = &mut self.timestamp_queries {
            timestamps.harvest();
        }
        self.apply_timestamp_stats();
        self.stats
    }

    #[must_use]
    pub fn pending_timestamp_readbacks(&self) -> u32 {
        self.timestamp_queries.as_ref().map_or(0, WebGpuTimestampQueries::pending_count)
    }

    pub fn set_timestamp_readback_interval_for_benchmark(&mut self, frames: u64) {
        if let Some(timestamps) = &mut self.timestamp_queries {
            timestamps.set_readback_interval_for_benchmark(frames);
        }
    }

    pub fn set_memory_stats_interval_for_benchmark(&mut self, frames: u64) {
        self.memory_stats_interval = frames.max(1);
    }

    pub fn set_memory_stats_enabled_for_benchmark(&mut self, enabled: bool) {
        self.memory_stats_enabled = enabled;
        if !enabled {
            self.memory_snapshot = WebGpuMemorySnapshot::default();
            self.apply_memory_stats();
        }
    }

    pub fn queue_completion_flag_for_benchmark(&self) -> Arc<AtomicBool> {
        let completed = Arc::new(AtomicBool::new(false));
        let callback_flag = Arc::clone(&completed);
        self.queue.on_submitted_work_done(move || {
            callback_flag.store(true, Ordering::Release);
        });
        completed
    }

    pub fn clear_completed_timestamp_samples(&mut self) {
        if let Some(timestamps) = &mut self.timestamp_queries {
            timestamps.clear_completed();
        }
    }

    pub fn drain_completed_timestamp_samples_into(
        &mut self,
        output: &mut Vec<WebGpuTimestampSample>,
    ) {
        if let Some(timestamps) = &mut self.timestamp_queries {
            timestamps.harvest();
            timestamps.drain_completed_into(output);
        } else {
            output.clear();
        }
    }

    pub fn set_cpu_submit_timing_enabled_for_benchmark(&mut self, enabled: bool) {
        self.cpu_submit_timing_enabled = enabled;
    }

    pub fn set_animation_time_ms(&mut self, time_ms: f64)
    {
        let phase = if time_ms.is_finite()
        {
            (time_ms.rem_euclid(1_000.0) / 1_000.0) as f32
        }
        else
        {
            0.0
        };
        if (self.animation_phase - phase).abs() <= f32::EPSILON
        {
            return;
        }
        self.animation_phase = phase;
    }

    #[must_use]
    pub fn last_cpu_submit_timing(&self) -> WebGpuCpuSubmitTimingSample {
        self.cpu_submit_timing
    }

    pub fn set_draw_state_cache_enabled_for_benchmark(&mut self, enabled: bool) {
        self.draw_state_cache_enabled = enabled;
    }

    pub fn set_draw_item_coalescing_enabled_for_benchmark(&mut self, enabled: bool) {
        self.draw_item_coalescing_enabled = enabled;
    }

    pub fn set_image_upload_scratch_enabled_for_benchmark(&mut self, enabled: bool) {
        self.image_upload_scratch_enabled = enabled;
    }

    pub fn set_effect_uniform_batch_enabled_for_benchmark(&mut self, enabled: bool) {
        self.effect_uniform_batch_enabled = enabled;
    }

    pub fn set_backdrop_batch_enabled_for_benchmark(&mut self, enabled: bool) {
        self.backdrop_batch_enabled = enabled;
    }

    pub fn set_direct_surface_enabled_for_benchmark(&mut self, enabled: bool) {
        self.direct_surface_enabled = enabled;
    }

    #[must_use]
    pub fn prepared_cache_resident_bytes(&self) -> u64 {
        self.prepared_chunks.resident_bytes
    }

    pub fn set_prepared_cache_budget_bytes(&mut self, budget_bytes: u64) {
        self.prepared_snapshot_bundle = None;
        self.prepared_snapshot_bundle_active = false;
        self.prepared_chunks.budget_bytes = budget_bytes;
        self.prepared_chunks.enforce_budget(&[]);
    }

    pub fn purge_prepared_chunks(&mut self) {
        self.prepared_chunks.clear();
        self.prepared_frame_plan.clear();
        self.prepared_layer_key_indices.clear();
        self.prepared_layer_plan.clear();
        self.prepared_layer_snapshot = None;
        self.prepared_snapshot_bundle = None;
        self.prepared_frame_active = false;
        self.prepared_snapshot_bundle_active = false;
        for layer in self.layers.values_mut()
        {
           layer.prepared_key = None;
           layer.resources.clear();
        }
    }

    #[must_use]
    pub fn layer_cache_budget_bytes(&self) -> u64 {
        self.layer_cache_budget_bytes
    }

    pub fn set_layer_cache_budget_bytes(&mut self, budget_bytes: u64) {
        self.layer_cache_budget_bytes = budget_bytes;
        self.enforce_layer_cache_budget();
        self.apply_layer_cache_stats();
    }

    pub fn purge_layer_cache(&mut self) {
        self.purge_layer_cache_for_reason(LAYER_PURGE_EXPLICIT);
    }

    pub fn purge_layer_cache_for_memory_pressure(&mut self) {
        self.purge_layer_cache_for_reason(LAYER_PURGE_MEMORY_PRESSURE);
    }

    #[must_use]
    pub fn id_mask_cache_budget_bytes(&self) -> u64 {
        self.id_mask_cache_budget_bytes
    }

    fn id_mask_target_bytes_per_pixel(&self) -> u64 {
        id_mask_target_bytes_per_pixel(self.programs.id_mask_packed.is_some())
    }

    fn id_mask_packed_fields_supported(&self) -> bool {
        self.programs.id_mask_packed.is_some()
    }

    pub fn set_id_mask_cache_budget_bytes(&mut self, budget_bytes: u64) {
        self.id_mask_cache_budget_bytes = budget_bytes;
        self.enforce_id_mask_cache_budget();
        self.apply_id_mask_cache_stats();
    }

    pub fn purge_id_mask_field_cache(&mut self) {
        self.purge_id_mask_field_cache_for_reason(LAYER_PURGE_EXPLICIT);
    }

    pub fn purge_id_mask_field_cache_for_memory_pressure(&mut self) {
        self.purge_id_mask_field_cache_for_reason(LAYER_PURGE_MEMORY_PRESSURE);
    }

    fn apply_id_mask_cache_stats(&mut self) {
        self.stats.id_mask_cache_budget_bytes = self.id_mask_cache_budget_bytes;
        self.stats.id_mask_cache_resident_bytes = self.id_mask_cache_resident_bytes;
        self.stats.id_mask_cache_evictions = self.id_mask_cache_evictions;
        self.stats.id_mask_cache_entries = self.id_mask_field_cache.len() as u32;
        self.stats.id_mask_cache_purges = self.id_mask_cache_purges;
        self.stats.id_mask_cache_last_purge_reason = self.id_mask_cache_last_purge_reason;
    }

    fn purge_id_mask_field_cache_for_reason(&mut self, reason: u8) {
        let removed = self.id_mask_field_cache.len();
        self.id_mask_field_cache.clear();
        self.id_mask_resolved_draws.clear();
        self.id_mask_cache_resident_bytes = 0;
        self.id_mask_cache_evictions =
            self.id_mask_cache_evictions.saturating_add(removed as u64);
        self.id_mask_cache_purges = self.id_mask_cache_purges.saturating_add(1);
        self.id_mask_cache_last_purge_reason = reason;
        #[cfg(feature = "snapshot-tests")]
        {
            self.id_mask_snapshot_targets = None;
        }
        self.apply_id_mask_cache_stats();
    }

    fn evict_oldest_id_mask_cache_entry(&mut self) -> Option<IdMaskRenderTargets> {
        let index = self
            .id_mask_field_cache
            .iter()
            .enumerate()
            .min_by_key(|(_, entry)| entry.last_used_frame)
            .map(|(index, _)| index)?;
        let entry = self.id_mask_field_cache.swap_remove(index);
        self.id_mask_cache_resident_bytes =
            self.id_mask_cache_resident_bytes.saturating_sub(entry.bytes);
        self.id_mask_cache_evictions = self.id_mask_cache_evictions.saturating_add(1);
        Some(entry.targets)
    }

    fn enforce_id_mask_cache_budget(&mut self) {
        while self.id_mask_field_cache.len() > ID_MASK_FIELD_CACHE_MAX_ENTRIES
            || self.id_mask_cache_resident_bytes > self.id_mask_cache_budget_bytes
        {
            if self.evict_oldest_id_mask_cache_entry().is_none() {
                break;
            }
        }
    }

    fn prepare_id_mask_cache_admission(
        &mut self,
        required: u64,
        width: u32,
        height: u32,
    ) -> Option<Option<IdMaskRenderTargets>> {
        if required > self.id_mask_cache_budget_bytes {
            return None;
        }
        let mut reusable = None;
        while self.id_mask_field_cache.len() >= ID_MASK_FIELD_CACHE_MAX_ENTRIES
            || self.id_mask_cache_resident_bytes.saturating_add(required)
                > self.id_mask_cache_budget_bytes
        {
            let targets = self.evict_oldest_id_mask_cache_entry()?;
            if reusable.is_none() && targets.width == width && targets.height == height {
                reusable = Some(targets);
            }
        }
        Some(reusable)
    }

    fn id_mask_field_cache_hit(
        &mut self,
        key: IdMaskFieldCacheKey,
        chunk_first: usize,
        chunk_count: usize,
    ) -> Option<IdMaskRenderTargets> {
        let chunk_end = chunk_first.saturating_add(chunk_count);
        let chunks = self.id_mask_draw_chunk_keys.get(chunk_first..chunk_end)?;
        let index = self
            .id_mask_field_cache
            .iter()
            .position(|entry| entry.matches(key, chunks))?;
        let entry = &mut self.id_mask_field_cache[index];
        entry.last_used_frame = self.frame_id;
        self.stats.id_mask_cache_hits = self.stats.id_mask_cache_hits.saturating_add(1);
        self.stats.backend_cache_hits = self.stats.backend_cache_hits.saturating_add(1);
        Some(entry.targets.clone())
    }

    fn retain_id_mask_field_cache_entry(
        &mut self,
        key: IdMaskFieldCacheKey,
        chunk_first: usize,
        chunk_count: usize,
        targets: &IdMaskRenderTargets,
    ) {
        let bytes = id_mask_render_targets_bytes(
            targets.width,
            targets.height,
            targets.packed_fields(),
        );
        while self.id_mask_cache_resident_bytes.saturating_add(bytes)
            > self.id_mask_cache_budget_bytes
        {
            if self.evict_oldest_id_mask_cache_entry().is_none() {
                return;
            }
        }
        if bytes > self.id_mask_cache_budget_bytes
            || self.id_mask_field_cache.len() >= ID_MASK_FIELD_CACHE_MAX_ENTRIES
        {
            return;
        }
        self.id_mask_cache_resident_bytes =
            self.id_mask_cache_resident_bytes.saturating_add(bytes);
        let chunk_end = chunk_first.saturating_add(chunk_count);
        let Some(chunks) = self.id_mask_draw_chunk_keys.get(chunk_first..chunk_end) else {
            return;
        };
        self.id_mask_field_cache.push(IdMaskFieldCacheEntry {
            key,
            chunks: chunks.to_vec(),
            targets: targets.clone(),
            bytes,
            last_used_frame: self.frame_id,
        });
        self.apply_id_mask_cache_stats();
    }

    fn layer_cache_cpu_bytes(&self) -> u64 {
        let active = (self.layers.len() as u64)
            .saturating_mul(core::mem::size_of::<GpuLayer>() as u64);
        let pooled = (self.layer_pool.capacity() as u64)
            .saturating_mul(core::mem::size_of::<PooledGpuLayer>() as u64);
        let resources = self
            .layers
            .values()
            .map(|layer| layer.resources.capacity())
            .chain(self.layer_pool.iter().map(|entry| entry.layer.resources.capacity()))
            .fold(0_u64, |total, capacity| {
                total.saturating_add(
                    (capacity as u64)
                        .saturating_mul(core::mem::size_of::<api::RenderResourceDependency>() as u64),
                )
            });
        active.saturating_add(pooled).saturating_add(resources)
    }

    fn apply_layer_cache_stats(&mut self) {
        self.stats.layer_cache_budget_bytes = self.layer_cache_budget_bytes;
        self.stats.layer_cache_resident_bytes = self.layer_cache_resident_bytes;
        self.stats.layer_cache_pool_bytes = self.layer_cache_pool_bytes;
        self.stats.layer_cache_cpu_bytes = self.layer_cache_cpu_bytes();
        self.stats.layer_cache_oldest_last_used_frame = self
            .layers
            .values()
            .map(|layer| layer.last_used_frame)
            .min()
            .unwrap_or(0);
        self.stats.layer_cache_pool_reuses = self.layer_cache_pool_reuses;
        self.stats.layer_cache_evictions = self.layer_cache_evictions;
        self.stats.layer_cache_recreations = self.layer_cache_recreations;
        self.stats.layer_cache_purges = self.layer_cache_purges;
        self.stats.layer_cache_last_purge_reason = self.layer_cache_last_purge_reason;
    }

    fn purge_layer_cache_for_reason(&mut self, reason: u8) {
        let removed = self.layers.len().saturating_add(self.layer_pool.len());
        self.layers.clear();
        self.layer_pool.clear();
        self.layer_frame_ids.clear();
        self.layer_cache_resident_bytes = 0;
        self.layer_cache_pool_bytes = 0;
        self.layer_cache_evictions = self
            .layer_cache_evictions
            .saturating_add(removed as u64);
        self.layer_cache_purges = self.layer_cache_purges.saturating_add(1);
        self.layer_cache_last_purge_reason = reason;
        self.prepared_layer_snapshot = None;
        self.prepared_layer_plan.clear();
        self.prepared_layer_key_indices.clear();
        self.apply_layer_cache_stats();
    }

    fn trim_layer_pool(&mut self, max_bytes: u64) {
        while self.layer_cache_pool_bytes > max_bytes {
            let Some(index) = self
                .layer_pool
                .iter()
                .enumerate()
                .min_by_key(|(_, entry)| entry.recycled_frame)
                .map(|(index, _)| index)
            else {
                break;
            };
            let removed = self.layer_pool.swap_remove(index);
            self.layer_cache_pool_bytes = self.layer_cache_pool_bytes.saturating_sub(removed.layer.bytes);
            self.layer_cache_evictions = self.layer_cache_evictions.saturating_add(1);
        }
    }

    fn recycle_layer(&mut self, layer: GpuLayer) {
        self.layer_cache_resident_bytes = self.layer_cache_resident_bytes.saturating_sub(layer.bytes);
        let pool_budget = self
            .layer_cache_budget_bytes
            .checked_div(LAYER_CACHE_POOL_BUDGET_DIVISOR)
            .unwrap_or(0);
        if layer.bytes > pool_budget {
            self.layer_cache_evictions = self.layer_cache_evictions.saturating_add(1);
            return;
        }
        self.layer_cache_pool_bytes = self.layer_cache_pool_bytes.saturating_add(layer.bytes);
        self.layer_pool.push(PooledGpuLayer {
            layer,
            recycled_frame: self.frame_id,
        });
        self.trim_layer_pool(pool_budget);
    }

    fn evict_oldest_unprotected_layer(&mut self) -> bool {
        let Some(id) = self
            .layers
            .iter()
            .filter(|(id, _)| !self.layer_frame_ids.contains(id))
            .min_by_key(|(_, layer)| layer.last_used_frame)
            .map(|(id, _)| *id)
        else {
            return false;
        };
        if let Some(layer) = self.layers.remove(&id) {
            self.recycle_layer(layer);
            self.layer_cache_evictions = self.layer_cache_evictions.saturating_add(1);
        }
        true
    }

    fn enforce_layer_cache_budget(&mut self) {
        self.trim_layer_pool(self.layer_cache_budget_bytes.saturating_sub(self.layer_cache_resident_bytes));
        while self
            .layer_cache_resident_bytes
            .saturating_add(self.layer_cache_pool_bytes)
            > self.layer_cache_budget_bytes
        {
            if !self.evict_oldest_unprotected_layer() {
                break;
            }
            self.trim_layer_pool(self.layer_cache_budget_bytes.saturating_sub(self.layer_cache_resident_bytes));
        }
    }

    fn admit_layer_bytes(&mut self, bytes: u64) -> bool {
        if bytes > self.layer_cache_budget_bytes {
            return false;
        }
        let retained_limit = self.layer_cache_budget_bytes.saturating_sub(bytes);
        self.trim_layer_pool(retained_limit.saturating_sub(self.layer_cache_resident_bytes));
        while self
            .layer_cache_resident_bytes
            .saturating_add(self.layer_cache_pool_bytes)
            > retained_limit
        {
            if !self.evict_oldest_unprotected_layer() {
                return false;
            }
            self.trim_layer_pool(retained_limit.saturating_sub(self.layer_cache_resident_bytes));
        }
        true
    }

    fn take_pooled_layer(&mut self, width: u32, height: u32) -> Option<GpuLayer> {
        let index = self.layer_pool.iter().position(|entry| {
            entry.layer.width == width
                && entry.layer.height == height
                && (entry.layer.scale - self.scale).abs() <= f32::EPSILON
        })?;
        let entry = self.layer_pool.swap_remove(index);
        self.layer_cache_pool_bytes = self.layer_cache_pool_bytes.saturating_sub(entry.layer.bytes);
        self.layer_cache_resident_bytes = self.layer_cache_resident_bytes.saturating_add(entry.layer.bytes);
        self.layer_cache_pool_reuses = self.layer_cache_pool_reuses.saturating_add(1);
        Some(entry.layer)
    }

    fn recycle_compatible_unprotected_layer(&mut self, width: u32, height: u32) -> bool {
        let Some(id) = self
            .layers
            .iter()
            .find(|(id, layer)| {
                !self.layer_frame_ids.contains(id)
                    && layer.width == width
                    && layer.height == height
                    && (layer.scale - self.scale).abs() <= f32::EPSILON
            })
            .map(|(id, _)| *id)
        else {
            return false;
        };
        if let Some(layer) = self.layers.remove(&id) {
            self.recycle_layer(layer);
            self.layer_cache_evictions = self.layer_cache_evictions.saturating_add(1);
        }
        true
    }

    fn age_layer_cache(&mut self) {
        if self.frame_id >= LAYER_CACHE_ABSENT_FRAMES {
            let absent_before = self.frame_id - LAYER_CACHE_ABSENT_FRAMES;
            loop {
                let Some(id) = self
                    .layers
                    .iter()
                    .find(|(id, layer)| {
                        !self.layer_frame_ids.contains(id)
                            && layer.last_used_frame <= absent_before
                    })
                    .map(|(id, _)| *id)
                else {
                    break;
                };
                if let Some(layer) = self.layers.remove(&id) {
                    self.recycle_layer(layer);
                }
            }
        }
        if self.frame_id >= LAYER_CACHE_POOL_MAX_AGE_FRAMES {
            let pool_before = self.frame_id - LAYER_CACHE_POOL_MAX_AGE_FRAMES;
            while let Some(index) = self
                .layer_pool
                .iter()
                .position(|entry| entry.recycled_frame <= pool_before)
            {
                let removed = self.layer_pool.swap_remove(index);
                self.layer_cache_pool_bytes = self
                    .layer_cache_pool_bytes
                    .saturating_sub(removed.layer.bytes);
                self.layer_cache_evictions = self.layer_cache_evictions.saturating_add(1);
            }
        }
        self.enforce_layer_cache_budget();
    }

    pub fn set_prepared_bundle_min_draws_for_benchmark(&mut self, draws: usize) {
        let draws = draws.max(1);
        if self.prepared_bundle_min_draws != draws {
            self.prepared_bundle_min_draws = draws;
            self.purge_prepared_chunks();
        }
    }

    pub fn advance_prepared_device_generation_for_benchmark(&mut self) {
        self.prepared_device_generation = self.prepared_device_generation.wrapping_add(1).max(1);
        self.purge_prepared_chunks();
    }

    fn scratch_capacity_breakdown(&self) -> ScratchCapacityBreakdown {
        let mut capacity = ScratchCapacityBreakdown::default();
        capacity.scene3d = capacity.scene3d.saturating_add(self.scene3d_uniform_bytes.capacity());
        capacity.effect = capacity.effect.saturating_add(self.effect_uniform_bytes.capacity());
        capacity.scene3d = capacity.scene3d.saturating_add(
            self.scene3d_draws.capacity().saturating_mul(core::mem::size_of::<Scene3dDraw>()),
        );
        capacity.scene3d = capacity.scene3d.saturating_add(
            self.scene3d_overlay_draws
                .capacity()
                .saturating_mul(core::mem::size_of::<Scene3dDraw>()),
        );
        capacity.id_mask = capacity.id_mask.saturating_add(
            self.id_mask_draws.capacity().saturating_mul(core::mem::size_of::<IdMaskDraw>()),
        );
        capacity.id_mask = capacity.id_mask.saturating_add(
            self.id_mask_draw_chunk_indices
                .capacity()
                .saturating_mul(core::mem::size_of::<usize>()),
        );
        capacity.id_mask = capacity.id_mask.saturating_add(
            self.id_mask_draw_chunk_keys
                .capacity()
                .saturating_mul(core::mem::size_of::<IdMaskChunkKey>()),
        );
        capacity.id_mask = capacity.id_mask.saturating_add(
            self.id_mask_vertex_caches
                .capacity()
                .saturating_mul(core::mem::size_of::<IdMaskVertexCache>()),
        );
        for cache in &self.id_mask_vertex_caches {
            capacity.id_mask = capacity.id_mask.saturating_add(cache.bytes.capacity());
        }
        capacity.id_mask = capacity.id_mask.saturating_add(
            self.id_mask_field_cache
                .capacity()
                .saturating_mul(core::mem::size_of::<IdMaskFieldCacheEntry>()),
        );
        capacity.id_mask = capacity.id_mask.saturating_add(
            self.id_mask_resolved_draws
                .capacity()
                .saturating_mul(core::mem::size_of::<IdMaskResolvedDraw>()),
        );
        for entry in &self.id_mask_field_cache {
            capacity.id_mask = capacity.id_mask.saturating_add(
                entry
                    .chunks
                    .capacity()
                    .saturating_mul(core::mem::size_of::<IdMaskChunkKey>()),
            );
        }
        capacity.resource_table = capacity.resource_table.saturating_add(
            self.images.storage_capacity_bytes(),
        );
        capacity.resource_table = capacity.resource_table.saturating_add(
            self.meshes_3d.capacity().saturating_mul(core::mem::size_of::<Option<GpuMesh3d>>()),
        );
        capacity.draw = capacity.draw.saturating_add(self.frame.geometry.capacity_bytes());
        capacity.draw = capacity.draw.saturating_add(
            self.frame
                .rrect_instances
                .capacity()
                .saturating_mul(core::mem::size_of::<RRectInstance>()),
        );
        capacity.draw = capacity.draw.saturating_add(
            self.frame
                .image_instances
                .capacity()
                .saturating_mul(core::mem::size_of::<ImageInstance>()),
        );
        capacity.draw = capacity.draw.saturating_add(
            self.frame
                .nine_slice_instances
                .capacity()
                .saturating_mul(core::mem::size_of::<NineSliceInstance>()),
        );
        capacity.draw = capacity.draw.saturating_add(
            self.frame
                .spinner_instances
                .capacity()
                .saturating_mul(core::mem::size_of::<SpinnerInstance>()),
        );
        capacity.draw = capacity.draw.saturating_add(
            self.frame
                .neon_marker_instances
                .capacity()
                .saturating_mul(core::mem::size_of::<NeonMarkerInstance>()),
        );
        capacity.draw = capacity.draw.saturating_add(
            self.frame.draws.capacity().saturating_mul(core::mem::size_of::<GpuDraw>()),
        );
        capacity.draw = capacity.draw.saturating_add(
            self.frame
                .layer_passes
                .capacity()
                .saturating_mul(core::mem::size_of::<FrameLayerPass>()),
        );
        capacity.draw = capacity.draw.saturating_add(
            self.scratch_vertices
                .capacity()
                .saturating_mul(core::mem::size_of::<PackedVertex>()),
        );
        capacity.draw = capacity.draw.saturating_add(
            self.scratch_indices.capacity().saturating_mul(core::mem::size_of::<u32>()),
        );
        capacity.image_upload =
            capacity.image_upload.saturating_add(self.image_upload_scratch.capacity());
        capacity.id_mask = capacity.id_mask.saturating_add(self.id_mask_uniform_bytes.capacity());
        capacity.id_mask = capacity.id_mask.saturating_add(
            self.id_mask_uniform_offsets
                .capacity()
                .saturating_mul(core::mem::size_of::<IdMaskUniformOffsets>()),
        );
        capacity.id_mask = capacity.id_mask.saturating_add(
            self.id_mask_field_uniform_offsets
                .capacity()
                .saturating_mul(core::mem::size_of::<u32>()),
        );
        capacity.draw = capacity.draw.saturating_add(
            self.clip_stack.capacity().saturating_mul(core::mem::size_of::<api::RectI>()),
        );
        capacity.draw = capacity.draw.saturating_add(
            self.target_stack.capacity().saturating_mul(core::mem::size_of::<u32>()),
        );
        capacity
    }

    fn apply_scratch_capacity_stats(&mut self, capacity: ScratchCapacityBreakdown) {
        self.stats.cpu_scratch_bytes = capacity.total() as u64;
        self.stats.cpu_draw_scratch_bytes = capacity.draw as u64;
        self.stats.cpu_scene3d_scratch_bytes = capacity.scene3d as u64;
        self.stats.cpu_effect_scratch_bytes = capacity.effect as u64;
        self.stats.cpu_id_mask_scratch_bytes = capacity.id_mask as u64;
        self.stats.cpu_image_upload_scratch_bytes = capacity.image_upload as u64;
        self.stats.cpu_resource_table_scratch_bytes = capacity.resource_table as u64;
        self.stats.image_upload_scratch_bytes = capacity.image_upload as u64;
    }

    fn record_scratch_growth(
        current: usize,
        previous: usize,
        grows: &mut u32,
        growth_bytes: &mut u64,
    ) {
        if current > previous {
            *grows = (*grows).saturating_add(1);
            *growth_bytes = current.saturating_sub(previous) as u64;
        }
    }

    fn record_scratch_growth_stats(&mut self) {
        let capacity = self.scratch_capacity_breakdown();
        self.apply_scratch_capacity_stats(capacity);
        Self::record_scratch_growth(
            capacity.total(),
            self.frame_scratch_capacity_bytes,
            &mut self.stats.cpu_scratch_grows,
            &mut self.stats.cpu_scratch_growth_bytes,
        );
        Self::record_scratch_growth(
            capacity.draw,
            self.frame_scratch_capacity.draw,
            &mut self.stats.cpu_draw_scratch_grows,
            &mut self.stats.cpu_draw_scratch_growth_bytes,
        );
        Self::record_scratch_growth(
            capacity.scene3d,
            self.frame_scratch_capacity.scene3d,
            &mut self.stats.cpu_scene3d_scratch_grows,
            &mut self.stats.cpu_scene3d_scratch_growth_bytes,
        );
        Self::record_scratch_growth(
            capacity.effect,
            self.frame_scratch_capacity.effect,
            &mut self.stats.cpu_effect_scratch_grows,
            &mut self.stats.cpu_effect_scratch_growth_bytes,
        );
        Self::record_scratch_growth(
            capacity.id_mask,
            self.frame_scratch_capacity.id_mask,
            &mut self.stats.cpu_id_mask_scratch_grows,
            &mut self.stats.cpu_id_mask_scratch_growth_bytes,
        );
        Self::record_scratch_growth(
            capacity.image_upload,
            self.frame_scratch_capacity.image_upload,
            &mut self.stats.cpu_image_upload_scratch_grows,
            &mut self.stats.cpu_image_upload_scratch_growth_bytes,
        );
        Self::record_scratch_growth(
            capacity.resource_table,
            self.frame_scratch_capacity.resource_table,
            &mut self.stats.cpu_resource_table_scratch_grows,
            &mut self.stats.cpu_resource_table_scratch_growth_bytes,
        );
    }

    fn apply_timestamp_stats(&mut self) {
        let Some(timestamps) = &self.timestamp_queries else {
            self.stats.gpu_timestamp_query_supported = false;
            return;
        };
        let latest = timestamps.latest;
        self.stats.gpu_timestamp_query_supported = true;
        self.stats.gpu_timestamp_frame_id = latest.frame_id;
        self.stats.gpu_timestamp_passes = latest.passes;
        self.stats.gpu_timestamp_total_ns = latest.total_ns;
        self.stats.gpu_timestamp_clear_ns = latest.clear_ns;
        self.stats.gpu_timestamp_draw_ns = latest.draw_ns;
        self.stats.gpu_timestamp_scene3d_ns = latest.scene3d_ns;
        self.stats.gpu_timestamp_scene3d_overlay_ns = latest.scene3d_overlay_ns;
        self.stats.gpu_timestamp_id_mask_raster_ns = latest.id_mask_raster_ns;
        self.stats.gpu_timestamp_id_mask_field_seed_ns = latest.id_mask_field_seed_ns;
        self.stats.gpu_timestamp_id_mask_field_jump_ns = latest.id_mask_field_jump_ns;
        self.stats.gpu_timestamp_id_mask_compositor_ns = latest.id_mask_compositor_ns;
        self.stats.gpu_timestamp_present_ns = latest.present_ns;
        self.stats.gpu_timestamp_max_pass_ns = latest.max_pass_ns;
        self.stats.gpu_timestamp_readback_skips = timestamps.readback_skips;
        self.stats.gpu_timestamp_readback_interval = timestamps
            .readback_interval_frames
            .min(u32::MAX as u64) as u32;
    }

    fn reserve_timestamp_pass(&mut self, family: TimestampPassFamily) -> Option<(u32, u32)> {
        self.timestamp_queries.as_mut().and_then(|timestamps| timestamps.reserve(family))
    }

    fn timestamp_writes(
        &self,
        pair: Option<(u32, u32)>,
    ) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        let (begin_query, end_query) = pair?;
        let timestamps = self.timestamp_queries.as_ref()?;
        Some(wgpu::RenderPassTimestampWrites {
            query_set: &timestamps.query_set,
            beginning_of_pass_write_index: Some(begin_query),
            end_of_pass_write_index: Some(end_query),
        })
    }

    fn prepare_timestamp_readback(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Option<(usize, u64)> {
        self.timestamp_queries
            .as_mut()
            .and_then(|timestamps| timestamps.prepare_readback(encoder, self.frame_id))
    }

    fn map_timestamp_readback(&mut self, slot_index: usize, bytes: u64) {
        if let Some(timestamps) = &mut self.timestamp_queries {
            timestamps.map_readback(slot_index, bytes);
        }
    }

    fn record_submit_allocation_stage(
        &mut self,
        stage: SubmitAllocationStage,
        before: AllocationSnapshot,
    ) {
        let after = oxide_wasm_alloc_counter::snapshot();
        let alloc_count = after.alloc_count.saturating_sub(before.alloc_count);
        let alloc_bytes = after.alloc_bytes.saturating_sub(before.alloc_bytes);
        let realloc_count = after.realloc_count.saturating_sub(before.realloc_count);
        let realloc_grow_bytes = after.realloc_grow_bytes.saturating_sub(before.realloc_grow_bytes);
        self.stats.submit_total_alloc_count =
            self.stats.submit_total_alloc_count.saturating_add(alloc_count);
        self.stats.submit_total_alloc_bytes =
            self.stats.submit_total_alloc_bytes.saturating_add(alloc_bytes);
        self.stats.submit_total_realloc_count =
            self.stats.submit_total_realloc_count.saturating_add(realloc_count);
        self.stats.submit_total_realloc_grow_bytes =
            self.stats.submit_total_realloc_grow_bytes.saturating_add(realloc_grow_bytes);
        let (stage_alloc_count, stage_alloc_bytes) = match stage {
            SubmitAllocationStage::Upload => (
                &mut self.stats.submit_upload_alloc_count,
                &mut self.stats.submit_upload_alloc_bytes,
            ),
            SubmitAllocationStage::Surface => (
                &mut self.stats.submit_surface_alloc_count,
                &mut self.stats.submit_surface_alloc_bytes,
            ),
            SubmitAllocationStage::Encoder => (
                &mut self.stats.submit_encoder_alloc_count,
                &mut self.stats.submit_encoder_alloc_bytes,
            ),
            SubmitAllocationStage::Render => (
                &mut self.stats.submit_render_alloc_count,
                &mut self.stats.submit_render_alloc_bytes,
            ),
            SubmitAllocationStage::Timestamp => (
                &mut self.stats.submit_timestamp_alloc_count,
                &mut self.stats.submit_timestamp_alloc_bytes,
            ),
            SubmitAllocationStage::ScratchStats => (
                &mut self.stats.submit_scratch_stats_alloc_count,
                &mut self.stats.submit_scratch_stats_alloc_bytes,
            ),
            SubmitAllocationStage::FinishQueue => (
                &mut self.stats.submit_finish_queue_alloc_count,
                &mut self.stats.submit_finish_queue_alloc_bytes,
            ),
            SubmitAllocationStage::Present => (
                &mut self.stats.submit_present_alloc_count,
                &mut self.stats.submit_present_alloc_bytes,
            ),
            SubmitAllocationStage::TimestampMap => (
                &mut self.stats.submit_timestamp_map_alloc_count,
                &mut self.stats.submit_timestamp_map_alloc_bytes,
            ),
        };
        *stage_alloc_count = stage_alloc_count.saturating_add(alloc_count);
        *stage_alloc_bytes = stage_alloc_bytes.saturating_add(alloc_bytes);
    }

    #[must_use]
    pub fn image_create_rgba8(
        &mut self,
        width: u32,
        height: u32,
        data: &[u8],
        row_bytes: usize,
    ) -> api::ImageHandle {
        match self.try_image_create_rgba8(width, height, data, row_bytes) {
            Ok(handle) => handle,
            Err(_) => api::ImageHandle(0),
        }
    }

    #[must_use]
    pub fn image_create_a8(
        &mut self,
        width: u32,
        height: u32,
        data: &[u8],
        row_bytes: usize,
    ) -> api::ImageHandle {
        match self.try_image_create_a8(width, height, data, row_bytes) {
            Ok(handle) => handle,
            Err(_) => api::ImageHandle(0),
        }
    }

    pub fn image_update_a8(
        &mut self,
        handle: api::ImageHandle,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        data: &[u8],
        row_bytes: usize,
    ) {
        let _ = self.try_image_update_a8(handle, x, y, width, height, data, row_bytes);
    }

    pub fn try_image_create_rgba8(
        &mut self,
        width: u32,
        height: u32,
        data: &[u8],
        row_bytes: usize,
    ) -> Result<api::ImageHandle, api::RenderError> {
        let rgba = copy_rgba_rows(width, height, data, row_bytes)
            .ok_or(api::RenderError::InvalidOperation("invalid rgba image rows"))?;
        self.push_image(
            width,
            height,
            GpuImageKind::Rgba,
            &rgba,
            width.saturating_mul(4),
        )
    }

    pub fn try_image_create_a8(
        &mut self,
        width: u32,
        height: u32,
        data: &[u8],
        row_bytes: usize,
    ) -> Result<api::ImageHandle, api::RenderError> {
        let row_bytes = image_row_bytes(width, height, GpuImageKind::A8, data, row_bytes)?;
        if row_bytes == width {
            return self.push_image(width, height, GpuImageKind::A8, data, row_bytes);
        }
        let mut alpha = Vec::new();
        copy_a8_rows_into(&mut alpha, width, height, data, row_bytes as usize)
            .ok_or(api::RenderError::InvalidOperation("invalid a8 image rows"))?;
        self.record_image_upload_temp(alpha.len(), 1);
        self.push_image(width, height, GpuImageKind::A8, &alpha, width)
    }

    pub fn try_image_update_a8(
        &mut self,
        handle: api::ImageHandle,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        data: &[u8],
        row_bytes: usize,
    ) -> Result<(), api::RenderError> {
        let row_bytes = image_row_bytes(width, height, GpuImageKind::A8, data, row_bytes)?;
        if row_bytes != width {
            let grew = copy_a8_rows_into(
                &mut self.image_upload_scratch,
                width,
                height,
                data,
                row_bytes as usize,
            )
            .ok_or(api::RenderError::InvalidOperation("invalid a8 update rows"))?;
            self.record_image_upload_scratch(grew);
            return self.update_image_from_upload_scratch(
                handle,
                x,
                y,
                width,
                height,
                GpuImageKind::A8,
            );
        }
        let image = image_for_update(
            &self.images,
            handle,
            x,
            y,
            width,
            height,
            GpuImageKind::A8,
        )?;
        write_image_update(
            &self.queue,
            &mut self.stats,
            image,
            x,
            y,
            width,
            height,
            data,
            row_bytes,
        );
        self.invalidate_image_dependents(handle);
        Ok(())
    }

    pub fn try_image_update_rgba8(
        &mut self,
        handle: api::ImageHandle,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        data: &[u8],
        row_bytes: usize,
    ) -> Result<(), api::RenderError> {
        if self.image_upload_scratch_enabled {
            let grew =
                copy_rgba_rows_into(&mut self.image_upload_scratch, width, height, data, row_bytes)
                    .ok_or(api::RenderError::InvalidOperation("invalid rgba update rows"))?;
            self.record_image_upload_scratch(grew);
            return self.update_image_from_upload_scratch(
                handle,
                x,
                y,
                width,
                height,
                GpuImageKind::Rgba,
            );
        }
        let rgba = copy_rgba_rows(width, height, data, row_bytes)
            .ok_or(api::RenderError::InvalidOperation("invalid rgba update rows"))?;
        self.record_image_upload_temp(rgba.len(), 1);
        self.update_image(handle, x, y, width, height, GpuImageKind::Rgba, &rgba)
    }

   /// Releases a renderer-owned image and returns whether the handle was live.
   pub fn image_release(&mut self, handle: api::ImageHandle) -> bool
   {
      let released = self.images.remove(handle.0).is_some();
      if released
      {
         self.invalidate_image_dependents(handle);
      }
      self.stats.cache_evictions = self.stats.cache_evictions.saturating_add(released as u32);
      released
   }

    pub fn mesh3d_create_colored(
        &mut self,
        data: &scene3d::MeshColor3dData<'_>,
    ) -> Result<scene3d::MeshHandle3d, api::RenderError> {
        if data.vertices.is_empty() {
            return Err(api::RenderError::InvalidOperation(
                "mesh3d_create_colored requires vertices",
            ));
        }
        if data.indices.is_empty() {
            return Err(api::RenderError::InvalidOperation(
                "mesh3d_create_colored requires indices",
            ));
        }
        match data.topology {
            scene3d::MeshTopology::Triangles if data.indices.len() % 3 != 0 => {
                return Err(api::RenderError::InvalidOperation(
                    "triangle mesh indices must be multiple of 3",
                ));
            }
            scene3d::MeshTopology::Lines if data.indices.len() % 2 != 0 => {
                return Err(api::RenderError::InvalidOperation(
                    "line mesh indices must be multiple of 2",
                ));
            }
            scene3d::MeshTopology::Triangles | scene3d::MeshTopology::Lines => {}
        }
        if data.indices.iter().any(|index| *index as usize >= data.vertices.len()) {
            return Err(api::RenderError::InvalidOperation(
                "mesh3d_create_colored index out of range",
            ));
        }

        let vertex_bytes = scene3d_color_vertex_bytes(data.vertices);
        let index_bytes = scene3d_index_bytes(data.indices);
        let vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("oxide-webgpu-scene3d-vertices"),
            size: vertex_bytes.len().max(1) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let index_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("oxide-webgpu-scene3d-indices"),
            size: index_bytes.len().max(1) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&vertex_buffer, 0, &vertex_bytes);
        self.queue.write_buffer(&index_buffer, 0, &index_bytes);
        self.stats.buffer_grows = self.stats.buffer_grows.saturating_add(2);
        self.stats.scene3d_buffer_grows = self.stats.scene3d_buffer_grows.saturating_add(2);
        self.stats.mesh3d_creates = self.stats.mesh3d_creates.saturating_add(1);
        self.stats.buffer_upload_bytes = self
            .stats
            .buffer_upload_bytes
            .saturating_add(vertex_bytes.len().saturating_add(index_bytes.len()) as u64);
        let handle = scene3d::MeshHandle3d(self.meshes_3d.len() as u32);
        self.meshes_3d.push(Some(GpuMesh3d {
            vertex_buffer,
            index_buffer,
            index_count: data.indices.len() as u32,
            topology: data.topology,
        }));
        Ok(handle)
    }

    pub fn mesh3d_release(&mut self, handle: scene3d::MeshHandle3d) {
        if let Some(slot) = self.meshes_3d.get_mut(handle.0 as usize) {
            *slot = None;
        }
    }

    pub fn encode_scene3d(&mut self, pass: &scene3d::Pass3d<'_>) -> Result<(), api::RenderError> {
        if !self.scene3d_active {
            self.scene3d_clear_color = pass.clear_color;
            self.scene3d_clear_depth = pass.clear_depth;
            self.scene3d_active = true;
        }

        for instance in pass.instances {
            let Some(mesh) = self.meshes_3d.get(instance.mesh.0 as usize).and_then(Option::as_ref)
            else {
                return Err(api::RenderError::ResourceNotFound("mesh3d handle"));
            };
            if !matches!(mesh.topology, scene3d::MeshTopology::Triangles) {
                return Err(api::RenderError::InvalidOperation(
                    "scene3d web path only supports triangle meshes",
                ));
            }
            if !matches!(instance.cull, scene3d::CullMode3d::None) {
                return Err(api::RenderError::Unsupported(
                    "scene3d web path does not support per-instance culling yet",
                ));
            }
            if !instance.color_write {
                continue;
            }
            self.stats.scene3d_draws = self.stats.scene3d_draws.saturating_add(1);
            let uniform_offset = push_scene3d_uniform(
                &mut self.scene3d_uniform_bytes,
                scene3d::mat4_mul(&pass.view_proj, &instance.transform),
                instance.color,
            );
            let pipeline = match (instance.blend, instance.depth_test, instance.depth_write) {
                (scene3d::BlendMode3d::Additive, true, true)
                | (scene3d::BlendMode3d::Additive, false, true) => {
                    Scene3dPipelineKind::AdditiveDepthWrite
                }
                (scene3d::BlendMode3d::Additive, true, false) => {
                    Scene3dPipelineKind::AdditiveDepthRead
                }
                (scene3d::BlendMode3d::Additive, false, false) => {
                    Scene3dPipelineKind::AdditiveNoDepth
                }
                (scene3d::BlendMode3d::Alpha, true, true)
                | (scene3d::BlendMode3d::Alpha, false, true) => {
                    Scene3dPipelineKind::AlphaDepthWrite
                }
                (scene3d::BlendMode3d::Alpha, true, false) => Scene3dPipelineKind::AlphaDepthRead,
                (scene3d::BlendMode3d::Alpha, false, _) => Scene3dPipelineKind::AlphaNoDepth,
            };
            let draw = Scene3dDraw { mesh: instance.mesh.0 as usize, uniform_offset, pipeline };
            if self.id_mask_draws.is_empty() {
                self.scene3d_draws.push(draw);
            } else {
                self.scene3d_overlay_draws.push(draw);
            }
        }
        Ok(())
    }

    pub fn encode_id_mask_gpu_compositor(
        &mut self,
        pass: &id_mask_compositor::IdMaskGpuCompositorPass<'_>,
    ) -> Result<(), api::RenderError> {
        if pass.raster.mask_width == 0 || pass.raster.mask_height == 0 {
            return Err(api::RenderError::InvalidOperation(
                "id-mask GPU raster has zero dimensions",
            ));
        }
        if !pass.raster.valid_triangle_vertex_count() {
            return Err(api::RenderError::InvalidOperation(
                "id-mask GPU raster vertices must be non-empty triangles",
            ));
        }
        let mask_width = u32::try_from(pass.raster.mask_width).map_err(|_| {
            api::RenderError::InvalidOperation("id-mask GPU raster width exceeds WebGPU limits")
        })?;
        let mask_height = u32::try_from(pass.raster.mask_height).map_err(|_| {
            api::RenderError::InvalidOperation("id-mask GPU raster height exceeds WebGPU limits")
        })?;
        self.stats.id_mask_draws = self.stats.id_mask_draws.saturating_add(1);
        let vertex_cache_first = self.id_mask_draw_chunk_indices.len() as u32;
        let mut vertex_count = 0usize;
        for chunk in pass.raster.chunks {
            self.stats.chunks_prepared = self.stats.chunks_prepared.saturating_add(1);
            let end = chunk.first_vertex.saturating_add(chunk.vertex_count);
            let Some(vertices) = pass.raster.vertices.get(chunk.first_vertex..end) else {
                return Err(api::RenderError::InvalidOperation(
                    "id-mask GPU raster chunk range is outside vertex data",
                ));
            };
            let vertex_cache_index = self.id_mask_vertex_cache_index(chunk.content_hash, vertices);
            self.id_mask_draw_chunk_indices.push(vertex_cache_index);
            self.id_mask_draw_chunk_keys.push(IdMaskChunkKey::from(chunk));
            vertex_count = vertex_count.saturating_add(chunk.vertex_count);
        }
        let vertex_cache_count =
            self.id_mask_draw_chunk_indices.len().saturating_sub(vertex_cache_first as usize)
                as u32;
        self.id_mask_draws.push(IdMaskDraw {
            viewport: pass.raster.viewport,
            mask_width,
            mask_height,
            mask_scale: pass.raster.mask_scale,
            field_key: IdMaskFieldCacheKey::new(&pass.raster),
            vertex_cache_first,
            vertex_cache_count,
            vertex_count: vertex_count as u32,
            projection: pass.raster.projection,
            city_styles: pass.city_styles,
            neighborhood_colors: pass.neighborhood_colors,
            mode: pass.mode,
            glow_enabled: pass.glow_enabled,
            darken_background_alpha: pass.darken_background_alpha,
            polish: pass.polish,
        });
        Ok(())
    }

    pub fn encode_neon_markers(
        &mut self,
        pass: &neon_marker::NeonMarkerPass<'_>,
    ) -> Result<(), api::RenderError> {
        for marker in pass.markers.iter().take(pass.clamped_len()) {
            if let Some(instance) = NeonMarkerInstance::new(*marker, pass.viewport)
            {
                self.push_neon_marker_instance(instance);
            }
        }
        Ok(())
    }

    fn push_image(
        &mut self,
        width: u32,
        height: u32,
        kind: GpuImageKind,
        data: &[u8],
        row_bytes: u32,
    ) -> Result<api::ImageHandle, api::RenderError> {
        if !self.images.has_capacity() {
            return Err(api::RenderError::InvalidOperation("gpu image slot capacity exhausted"));
        }
        let image = self.create_image(width, height, kind, data, row_bytes)?;
        self.images
            .insert(image)
            .map(api::ImageHandle)
            .map_err(|_| api::RenderError::InvalidOperation("gpu image slot capacity exhausted"))
    }

    fn create_image(
        &mut self,
        width: u32,
        height: u32,
        kind: GpuImageKind,
        data: &[u8],
        row_bytes: u32,
    ) -> Result<GpuImage, api::RenderError> {
        if width == 0 || height == 0 {
            return Err(api::RenderError::InvalidOperation("zero-sized gpu image"));
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("oxide-webgpu-image"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: kind.format(),
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.stats.texture_creates = self.stats.texture_creates.saturating_add(1);
        self.stats.image_texture_creates = self.stats.image_texture_creates.saturating_add(1);
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row_bytes),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        let upload_bytes = u64::from(width)
            .saturating_mul(u64::from(height))
            .saturating_mul(u64::from(kind.bytes_per_pixel()));
        self.stats.texture_upload_bytes =
            self.stats.texture_upload_bytes.saturating_add(upload_bytes);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group =
            create_texture_bind_group(&self.device, &self.programs, &view, &self.programs.sampler);
        self.stats.bind_group_creates = self.stats.bind_group_creates.saturating_add(1);
        self.stats.image_bind_group_creates = self.stats.image_bind_group_creates.saturating_add(1);
        drop(view);
        Ok(GpuImage { texture, bind_group, width, height, kind })
    }

    fn record_image_upload_scratch(&mut self, grew: bool) {
        self.stats.image_upload_scratch_bytes = self.image_upload_scratch.capacity() as u64;
        if grew {
            self.stats.image_upload_scratch_grows =
                self.stats.image_upload_scratch_grows.saturating_add(1);
        }
    }

    fn record_image_upload_temp(&mut self, bytes: usize, allocs: u32) {
        self.stats.image_upload_temp_allocs =
            self.stats.image_upload_temp_allocs.saturating_add(allocs);
        self.stats.image_upload_temp_bytes =
            self.stats.image_upload_temp_bytes.saturating_add(bytes as u64);
    }

    fn update_image_from_upload_scratch(
        &mut self,
        handle: api::ImageHandle,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        kind: GpuImageKind,
    ) -> Result<(), api::RenderError> {
        let image = image_for_update(&self.images, handle, x, y, width, height, kind)?;
        write_image_update(
            &self.queue,
            &mut self.stats,
            image,
            x,
            y,
            width,
            height,
            &self.image_upload_scratch,
            width.saturating_mul(kind.bytes_per_pixel()),
        );
        self.invalidate_image_dependents(handle);
        Ok(())
    }

    fn update_image(
        &mut self,
        handle: api::ImageHandle,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        kind: GpuImageKind,
        rgba: &[u8],
    ) -> Result<(), api::RenderError> {
        let image = image_for_update(&self.images, handle, x, y, width, height, kind)?;
        write_image_update(
            &self.queue,
            &mut self.stats,
            image,
            x,
            y,
            width,
            height,
            rgba,
            width.saturating_mul(kind.bytes_per_pixel()),
        );
        self.invalidate_image_dependents(handle);
        Ok(())
    }

    fn invalidate_image_dependents(&mut self, handle: api::ImageHandle)
    {
       self.prepared_chunks.invalidate_resource(handle);
       self.invalidate_layers_for_resource(handle);
       self.prepared_snapshot_bundle = None;
       self.prepared_snapshot_bundle_active = false;
    }

    fn image(&self, handle: api::ImageHandle) -> Option<&GpuImage> {
        self.images.get(handle.0)
    }

    fn current_clip(&self) -> api::RectI {
        self.clip_stack.last().copied().unwrap_or_else(|| {
            api::RectI::new(
                0,
                0,
                logical_dimension(self.width, self.scale) as i32,
                logical_dimension(self.height, self.scale) as i32,
            )
        })
    }

    fn current_target(&self) -> Option<u32> {
        self.target_stack.last().copied()
    }

    fn try_coalesce_draw_item(
        &mut self,
        kind: DrawKind,
        range: PackedIndexRange,
        clip: api::RectI,
        target: Option<u32>,
    ) -> bool {
        if !self.draw_item_coalescing_enabled {
            return false;
        }
        let Some(last) = self.frame.draws.last_mut() else {
            return false;
        };
        if last.index_kind == range.kind
            && last.base_vertex == range.base_vertex
            && last.first_index.saturating_add(last.index_count) == range.first_index
            && last.clip == clip
            && last.target == target
            && last.effect_uniform_offset == 0
            && coalescible_draw_kind(last.kind, kind)
        {
            last.index_count = last.index_count.saturating_add(range.index_count);
            self.stats.draw_items_coalesced = self.stats.draw_items_coalesced.saturating_add(1);
            true
        } else {
            false
        }
    }

    fn push_draw(&mut self, kind: DrawKind, vertices: &[PackedVertex; 4]) {
        let clip = self.current_clip();
        let target = self.current_target();
        let before = self.frame.geometry.byte_len();
        let Some(range) = self.frame.geometry.append_quad(vertices) else {
            return;
        };
        let after = self.frame.geometry.byte_len();
        self.stats.geometry_bytes_copied = self
            .stats
            .geometry_bytes_copied
            .saturating_add(after.saturating_sub(before) as u64);
        self.frame.record_draw_kind(kind);
        if self.try_coalesce_draw_item(kind, range, clip, target) {
            return;
        }
        self.frame.draws.push(GpuDraw {
            kind,
            index_kind: range.kind,
            first_index: range.first_index,
            index_count: range.index_count,
            base_vertex: range.base_vertex,
            clip,
            effect_uniform_offset: 0,
            target,
        });
        self.stats.commands_copied = self.stats.commands_copied.saturating_add(1);
    }

    fn clear_scratch_draw(&mut self) {
        self.scratch_vertices.clear();
        self.scratch_indices.clear();
    }

    fn push_scratch_draw(&mut self, kind: DrawKind) {
        if self.scratch_vertices.is_empty() || self.scratch_indices.is_empty() {
            return;
        }
        let clip = self.current_clip();
        let target = self.current_target();
        let before = self.frame.geometry.byte_len();
        let Some(range) = self
            .frame
            .geometry
            .append_validated(&self.scratch_vertices, &self.scratch_indices)
        else {
            return;
        };
        let after = self.frame.geometry.byte_len();
        self.stats.geometry_bytes_copied = self
            .stats
            .geometry_bytes_copied
            .saturating_add(after.saturating_sub(before) as u64);
        self.frame.record_draw_kind(kind);
        if self.try_coalesce_draw_item(kind, range, clip, target) {
            return;
        }
        self.frame.draws.push(GpuDraw {
            kind,
            index_kind: range.kind,
            first_index: range.first_index,
            index_count: range.index_count,
            base_vertex: range.base_vertex,
            clip,
            effect_uniform_offset: 0,
            target,
        });
        self.stats.commands_copied = self.stats.commands_copied.saturating_add(1);
    }

    fn push_rrect(&mut self, instance: RRectInstance)
    {
        let Ok(first_instance) = u32::try_from(self.frame.rrect_instances.len()) else
        {
            return;
        };
        let clip = self.current_clip();
        let target = self.current_target();
        self.frame.rrect_instances.push(instance);
        self.stats.geometry_bytes_copied = self.stats.geometry_bytes_copied
            .saturating_add(RRECT_INSTANCE_BYTES as u64);
        self.stats.rrect_instances = self.stats.rrect_instances.saturating_add(1);
        self.stats.rrect_triangles = self.stats.rrect_triangles.saturating_add(2);
        self.stats.rrect_instance_bytes = self.stats.rrect_instance_bytes
            .saturating_add(RRECT_INSTANCE_BYTES as u64);
        if self.draw_item_coalescing_enabled
        {
            if let Some(last) = self.frame.draws.last_mut()
            {
                if last.clip == clip && last.target == target
                {
                    if let DrawKind::RRect { first_instance: first, instance_count } = last.kind
                    {
                        if first.saturating_add(instance_count) == first_instance
                        {
                            last.kind = DrawKind::RRect {
                                first_instance: first,
                                instance_count: instance_count.saturating_add(1),
                            };
                            self.stats.draw_items_coalesced =
                                self.stats.draw_items_coalesced.saturating_add(1);
                            return;
                        }
                    }
                }
            }
        }
        self.frame.draws.push(GpuDraw {
            kind: DrawKind::RRect { first_instance, instance_count: 1 },
            index_kind: PackedIndexKind::U16,
            first_index: 0,
            index_count: 0,
            base_vertex: 0,
            clip,
            effect_uniform_offset: 0,
            target,
        });
        self.stats.commands_copied = self.stats.commands_copied.saturating_add(1);
    }

    fn push_image_instance(&mut self, image: u32, kind: GpuImageKind, instance: ImageInstance)
    {
        let Ok(first_instance) = u32::try_from(self.frame.image_instances.len()) else
        {
            return;
        };
        let clip = self.current_clip();
        let target = self.current_target();
        self.frame.image_instances.push(instance);
        self.stats.geometry_bytes_copied = self.stats.geometry_bytes_copied
            .saturating_add(IMAGE_INSTANCE_BYTES as u64);
        self.stats.image_instances = self.stats.image_instances.saturating_add(1);
        self.stats.image_triangles = self.stats.image_triangles.saturating_add(2);
        self.stats.image_instance_bytes = self.stats.image_instance_bytes
            .saturating_add(IMAGE_INSTANCE_BYTES as u64);
        if self.draw_item_coalescing_enabled
        {
            if let Some(last) = self.frame.draws.last_mut()
            {
                if last.clip == clip && last.target == target
                {
                    if let DrawKind::Image {
                        image: last_image,
                        kind: last_kind,
                        first_instance: first,
                        instance_count,
                    } = last.kind
                    {
                        if last_image == image && last_kind == kind
                            && first.saturating_add(instance_count) == first_instance
                        {
                            last.kind = DrawKind::Image {
                                image,
                                kind,
                                first_instance: first,
                                instance_count: instance_count.saturating_add(1),
                            };
                            self.stats.draw_items_coalesced =
                                self.stats.draw_items_coalesced.saturating_add(1);
                            return;
                        }
                    }
                }
            }
        }
        self.frame.draws.push(GpuDraw {
            kind: DrawKind::Image { image, kind, first_instance, instance_count: 1 },
            index_kind: PackedIndexKind::U16,
            first_index: 0,
            index_count: 0,
            base_vertex: 0,
            clip,
            effect_uniform_offset: 0,
            target,
        });
        self.stats.commands_copied = self.stats.commands_copied.saturating_add(1);
    }

    fn push_nine_slice_instance(
        &mut self,
        image: u32,
        kind: GpuImageKind,
        instance: NineSliceInstance,
    )
    {
        let Ok(first_instance) = u32::try_from(self.frame.nine_slice_instances.len()) else
        {
            return;
        };
        let clip = self.current_clip();
        let target = self.current_target();
        self.frame.nine_slice_instances.push(instance);
        self.stats.geometry_bytes_copied = self.stats.geometry_bytes_copied
            .saturating_add(NINE_SLICE_INSTANCE_BYTES as u64);
        self.stats.nine_slice_instances = self.stats.nine_slice_instances.saturating_add(1);
        self.stats.nine_slice_triangles = self.stats.nine_slice_triangles.saturating_add(18);
        self.stats.nine_slice_instance_bytes = self.stats.nine_slice_instance_bytes
            .saturating_add(NINE_SLICE_INSTANCE_BYTES as u64);
        if self.draw_item_coalescing_enabled
        {
            if let Some(last) = self.frame.draws.last_mut()
            {
                if last.clip == clip && last.target == target
                {
                    if let DrawKind::NineSlice {
                        image: last_image,
                        kind: last_kind,
                        first_instance: first,
                        instance_count,
                    } = last.kind
                    {
                        if last_image == image && last_kind == kind
                            && first.saturating_add(instance_count) == first_instance
                        {
                            last.kind = DrawKind::NineSlice {
                                image,
                                kind,
                                first_instance: first,
                                instance_count: instance_count.saturating_add(1),
                            };
                            self.stats.draw_items_coalesced =
                                self.stats.draw_items_coalesced.saturating_add(1);
                            return;
                        }
                    }
                }
            }
        }
        self.frame.draws.push(GpuDraw {
            kind: DrawKind::NineSlice { image, kind, first_instance, instance_count: 1 },
            index_kind: PackedIndexKind::U16,
            first_index: 0,
            index_count: 0,
            base_vertex: 0,
            clip,
            effect_uniform_offset: 0,
            target,
        });
        self.stats.commands_copied = self.stats.commands_copied.saturating_add(1);
    }

    fn push_spinner_instance(&mut self, instance: SpinnerInstance)
    {
        let Ok(first_instance) = u32::try_from(self.frame.spinner_instances.len()) else
        {
            return;
        };
        let clip = self.current_clip();
        let target = self.current_target();
        self.frame.spinner_instances.push(instance);
        self.stats.geometry_bytes_copied = self.stats.geometry_bytes_copied
            .saturating_add(SPINNER_INSTANCE_BYTES as u64);
        self.stats.spinner_instances = self.stats.spinner_instances.saturating_add(1);
        self.stats.spinner_triangles = self.stats.spinner_triangles.saturating_add(24);
        self.stats.spinner_instance_bytes = self.stats.spinner_instance_bytes
            .saturating_add(SPINNER_INSTANCE_BYTES as u64);
        if self.draw_item_coalescing_enabled
        {
            if let Some(last) = self.frame.draws.last_mut()
            {
                if last.clip == clip && last.target == target
                {
                    if let DrawKind::Spinner { first_instance: first, instance_count } = last.kind
                    {
                        if first.saturating_add(instance_count) == first_instance
                        {
                            last.kind = DrawKind::Spinner {
                                first_instance: first,
                                instance_count: instance_count.saturating_add(1),
                            };
                            self.stats.draw_items_coalesced =
                                self.stats.draw_items_coalesced.saturating_add(1);
                            return;
                        }
                    }
                }
            }
        }
        self.frame.draws.push(GpuDraw {
            kind: DrawKind::Spinner { first_instance, instance_count: 1 },
            index_kind: PackedIndexKind::U16,
            first_index: 0,
            index_count: 0,
            base_vertex: 0,
            clip,
            effect_uniform_offset: 0,
            target,
        });
        self.stats.commands_copied = self.stats.commands_copied.saturating_add(1);
    }

    fn push_neon_marker_instance(&mut self, instance: NeonMarkerInstance)
    {
        let Ok(first_instance) = u32::try_from(self.frame.neon_marker_instances.len()) else
        {
            return;
        };
        let clip = self.current_clip();
        let target = self.current_target();
        self.frame.neon_marker_instances.push(instance);
        self.stats.geometry_bytes_copied = self.stats.geometry_bytes_copied
            .saturating_add(NEON_MARKER_INSTANCE_BYTES as u64);
        self.stats.neon_marker_instances = self.stats.neon_marker_instances.saturating_add(1);
        self.stats.neon_marker_triangles = self.stats.neon_marker_triangles.saturating_add(2);
        self.stats.neon_marker_instance_bytes = self.stats.neon_marker_instance_bytes
            .saturating_add(NEON_MARKER_INSTANCE_BYTES as u64);
        if self.draw_item_coalescing_enabled
        {
            if let Some(last) = self.frame.draws.last_mut()
            {
                if last.clip == clip && last.target == target
                {
                    if let DrawKind::NeonMarker { first_instance: first, instance_count } = last.kind
                    {
                        if first.saturating_add(instance_count) == first_instance
                        {
                            last.kind = DrawKind::NeonMarker {
                                first_instance: first,
                                instance_count: instance_count.saturating_add(1),
                            };
                            self.stats.draw_items_coalesced =
                                self.stats.draw_items_coalesced.saturating_add(1);
                            return;
                        }
                    }
                }
            }
        }
        self.frame.draws.push(GpuDraw {
            kind: DrawKind::NeonMarker { first_instance, instance_count: 1 },
            index_kind: PackedIndexKind::U16,
            first_index: 0,
            index_count: 0,
            base_vertex: 0,
            clip,
            effect_uniform_offset: 0,
            target,
        });
        self.stats.commands_copied = self.stats.commands_copied.saturating_add(1);
    }

    fn local_layer_frame(&self, rect: api::RectF) -> Option<LocalLayerFrame>
    {
       let parent = self.current_target()
          .and_then(|id| self.layers.get(&id))
          .map_or([
             logical_dimension(self.width, self.scale).max(1.0),
             logical_dimension(self.height, self.scale).max(1.0),
             0.0, 0.0,
             1.0, 0.0, 0.0, 1.0,
             0.0, 0.0, 1.0, 0.0,
          ], |layer| layer.viewport);
       let [_, _, _, _, scale_x, shear_y, shear_x, scale_y, translate_x, translate_y, _, _] = parent;
       if shear_x != 0.0 || shear_y != 0.0 || scale_x == 0.0 || scale_y == 0.0
       {
          return None;
       }
       let x0 = scale_x * rect.x + translate_x;
       let x1 = scale_x * (rect.x + rect.w) + translate_x;
       let y0 = scale_y * rect.y + translate_y;
       let y1 = scale_y * (rect.y + rect.h) + translate_y;
       let unsnapped = api::RectF::new(
          x0.min(x1), y0.min(y1),
          (x1 - x0).abs(), (y1 - y0).abs(),
       );
       let (target_rect, width, height) = layer_target_rect(
          unsnapped,
          self.scale,
          self.device.limits().max_texture_dimension_2d,
       )?;
       let inverse_x0 = (target_rect.x - translate_x) / scale_x;
       let inverse_x1 = (target_rect.x + target_rect.w - translate_x) / scale_x;
       let inverse_y0 = (target_rect.y - translate_y) / scale_y;
       let inverse_y1 = (target_rect.y + target_rect.h - translate_y) / scale_y;
       let composite_rect = api::RectF::new(
          inverse_x0.min(inverse_x1), inverse_y0.min(inverse_y1),
          (inverse_x1 - inverse_x0).abs(), (inverse_y1 - inverse_y0).abs(),
       );
       Some(LocalLayerFrame {
          source_rect: rect,
          target_rect,
          composite_rect,
          viewport: [
             target_rect.w, target_rect.h, target_rect.x, target_rect.y,
             scale_x, 0.0, 0.0, scale_y,
             translate_x, translate_y, 1.0, self.animation_phase,
          ],
          width,
          height,
       })
    }

    fn cached_layer(&self, id: u32, frame: LocalLayerFrame) -> Option<&GpuLayer> {
        let layer = self.layers.get(&id)?;
        if layer.width == frame.width
            && layer.height == frame.height
            && (layer.scale - self.scale).abs() <= f32::EPSILON
            && layer.source_rect == frame.source_rect
            && layer.rect == frame.target_rect
            && layer.composite_rect == frame.composite_rect
            && layer.prepared_key.is_none()
        {
            Some(layer)
        } else {
            None
        }
    }

    fn cached_prepared_layer(&self, layer: PreparedLayerFrame, chunk: &api::RenderChunk) -> Option<&GpuLayer>
    {
       self.layers.get(&layer.key.id).filter(|entry| {
          entry.width == layer.width
             && entry.height == layer.height
             && (entry.scale - self.scale).abs() <= f32::EPSILON
             && entry.prepared_key == Some(layer.key)
             && entry.resources.as_slice() == chunk.resource_dependencies()
       })
    }

    fn invalidate_layers_for_resource(&mut self, handle: api::ImageHandle)
    {
       for layer in self.layers.values_mut()
       {
          if layer.prepared_key.is_some()
             && layer.resources.iter().any(|dependency| dependency.image == handle)
          {
             layer.prepared_key = None;
             layer.resources.clear();
          }
       }
    }

    fn touch_layer(&mut self, id: u32) {
        self.layer_frame_ids.insert(id);
        if let Some(layer) = self.layers.get_mut(&id) {
            layer.last_used_frame = self.frame_id;
        }
    }

    fn ensure_layer(&mut self, id: u32, frame: LocalLayerFrame) -> bool {
        self.ensure_layer_target(
            id,
            frame.source_rect,
            frame.target_rect,
            frame.composite_rect,
            frame.width,
            frame.height,
            frame.viewport,
            None,
            &[],
        )
    }

    fn ensure_prepared_layer(&mut self, layer: PreparedLayerFrame, chunk: &api::RenderChunk) -> bool {
        self.ensure_layer_target(
            layer.key.id,
            layer.source_rect,
            layer.rect,
            layer.rect,
            layer.width,
            layer.height,
            layer.viewport,
            Some(layer.key),
            chunk.resource_dependencies(),
        )
    }

    fn ensure_layer_target(&mut self, id: u32, source_rect: api::RectF, rect: api::RectF, composite_rect: api::RectF, width: u32, height: u32, viewport: [f32; 12], prepared_key: Option<PreparedLayerKey>, resources: &[api::RenderResourceDependency]) -> bool {
        let recreate = self.layers.get(&id).map_or(true, |layer| {
            layer.width != width
                || layer.height != height
                || (layer.scale - self.scale).abs() > f32::EPSILON
        });
        if recreate {
            if let Some(layer) = self.layers.remove(&id) {
                self.layer_cache_recreations = self.layer_cache_recreations.saturating_add(1);
                self.recycle_layer(layer);
            }
            let bytes = saturating_texture_bytes(
                u64::from(width),
                u64::from(height),
                color_texture_bytes_per_pixel(self.config.format),
            );
            let mut pooled = self.take_pooled_layer(width, height);
            if pooled.is_none() && self.recycle_compatible_unprotected_layer(width, height) {
                pooled = self.take_pooled_layer(width, height);
            }
            let mut layer = if let Some(layer) = pooled {
                layer
            } else {
                if !self.admit_layer_bytes(bytes) {
                    return false;
                }
                let (texture, view, bind_group) = create_target_texture(
                    &self.device,
                    &self.programs,
                    "oxide-webgpu-layer",
                    self.config.format,
                    width,
                    height,
                );
                let (viewport_buffer, viewport_bind_group) =
                    create_viewport_bind_group(&self.device, &self.programs);
                self.layer_cache_resident_bytes =
                    self.layer_cache_resident_bytes.saturating_add(bytes);
                self.stats.texture_creates = self.stats.texture_creates.saturating_add(1);
                self.stats.bind_group_creates = self.stats.bind_group_creates.saturating_add(2);
                self.stats.buffer_grows = self.stats.buffer_grows.saturating_add(1);
                self.stats.target_texture_creates =
                    self.stats.target_texture_creates.saturating_add(1);
                self.stats.target_bind_group_creates =
                    self.stats.target_bind_group_creates.saturating_add(1);
                self.stats.layer_texture_creates =
                    self.stats.layer_texture_creates.saturating_add(1);
                self.stats.layer_bind_group_creates =
                    self.stats.layer_bind_group_creates.saturating_add(2);
                GpuLayer {
                    texture,
                    view,
                    bind_group,
                    viewport_buffer,
                    viewport_bind_group,
                    viewport,
                    source_rect,
                    rect,
                    composite_rect,
                    width,
                    height,
                    scale: self.scale,
                    prepared_key,
                    resources: Vec::new(),
                    bytes,
                    last_used_frame: self.frame_id,
                }
            };
            write_uniform(&self.queue, &layer.viewport_buffer, viewport);
            layer.viewport = viewport;
            layer.source_rect = source_rect;
            layer.rect = rect;
            layer.composite_rect = composite_rect;
            layer.scale = self.scale;
            layer.prepared_key = prepared_key;
            layer.resources.clear();
            layer.resources.extend_from_slice(resources);
            layer.last_used_frame = self.frame_id;
            self.layers.insert(id, layer);
        } else if let Some(layer) = self.layers.get_mut(&id) {
            write_uniform(&self.queue, &layer.viewport_buffer, viewport);
            layer.viewport = viewport;
            layer.source_rect = source_rect;
            layer.rect = rect;
            layer.composite_rect = composite_rect;
            layer.prepared_key = prepared_key;
            layer.resources.clear();
            layer.resources.extend_from_slice(resources);
            layer.last_used_frame = self.frame_id;
        }
        self.layer_frame_ids.insert(id);
        true
    }

    fn push_layer_draw(&mut self, id: u32, rect: api::RectF) {
        let color = api::Color::rgba(1.0, 1.0, 1.0, 1.0);
        let vertices = quad_vertices(rect, 0.0, 0.0, 1.0, 1.0, color);
        self.push_draw(DrawKind::Layer { id }, &vertices);
    }

    fn encode_layer(
        &mut self,
        list: &api::DrawList,
        index: &mut usize,
        id: u32,
        rect: api::RectF,
        dirty: bool,
    ) {
        self.stats.layer_draws = self.stats.layer_draws.saturating_add(1);
        if rect.w <= 0.0 || rect.h <= 0.0 {
            let skipped = skip_layer_body(list, index);
            self.stats.layer_cache_skipped_draws =
                self.stats.layer_cache_skipped_draws.saturating_add(skipped);
            return;
        }
        if id == 0 {
            self.encode_items(list, index, true);
            return;
        }

        let Some(frame) = self.local_layer_frame(rect) else {
            self.encode_items(list, index, true);
            return;
        };
        if !dirty {
            let cached_rect = self.cached_layer(id, frame).map(|layer| layer.composite_rect);
            if let Some(cached_rect) = cached_rect {
                self.touch_layer(id);
                let skipped = skip_layer_body(list, index);
                self.stats.layer_cache_hits = self.stats.layer_cache_hits.saturating_add(1);
                self.stats.layer_cache_skipped_draws =
                    self.stats.layer_cache_skipped_draws.saturating_add(skipped);
                self.push_layer_draw(id, cached_rect);
                return;
            }
        }

        if !self.ensure_layer(id, frame) {
            self.encode_items(list, index, true);
            return;
        }
        self.stats.layer_cache_misses = self.stats.layer_cache_misses.saturating_add(1);
        let start = self.frame.draws.len();
        self.target_stack.push(id);
        self.encode_items(list, index, true);
        let _ = self.target_stack.pop();
        let end = self.frame.draws.len();
        self.frame.layer_passes.push(FrameLayerPass { id, start, end });
        self.push_layer_draw(id, frame.composite_rect);
    }

    fn encode_items(&mut self, list: &api::DrawList, index: &mut usize, stop_at_layer_end: bool) {
        while *index < list.items.len() {
            self.stats.commands_traversed = self.stats.commands_traversed.saturating_add(1);
            match &list.items[*index] {
                api::DrawCmd::LayerBegin { id, rect, dirty } => {
                    *index += 1;
                    self.encode_layer(list, index, *id, *rect, *dirty);
                }
                api::DrawCmd::LayerEnd => {
                    *index += 1;
                    if stop_at_layer_end {
                        return;
                    }
                }
                item => {
                    self.encode_draw_cmd(list, item);
                    *index += 1;
                }
            }
        }
    }

    fn encode_draw_cmd(&mut self, list: &api::DrawList, item: &api::DrawCmd) {
        match item {
            api::DrawCmd::LayerBegin { .. } | api::DrawCmd::LayerEnd => {}
            api::DrawCmd::Solid { vb, ib, color } => self.encode_solid(list, *vb, *ib, *color),
            api::DrawCmd::Image { tex, dst, src, alpha } => {
                self.encode_image(*tex, *dst, *src, *alpha, false)
            }
            api::DrawCmd::ImageMesh { tex, vb, ib, alpha } => {
                self.encode_image_mesh(list, *tex, *vb, *ib, *alpha)
            }
            api::DrawCmd::GlyphRun { run } => self.encode_glyph_run(list, run),
            api::DrawCmd::RRect { rect, radii, color } => self.encode_rrect(*rect, *radii, *color),
            api::DrawCmd::NineSlice { tex, rect, slice, alpha } => {
                self.encode_nine_slice(*tex, *rect, *slice, *alpha)
            }
            api::DrawCmd::Backdrop { rect, sigma, tint, alpha } => {
                self.stats.backdrop_draws = self.stats.backdrop_draws.saturating_add(1);
                self.encode_backdrop(*rect, *sigma, *tint, *alpha)
            }
            api::DrawCmd::VisualEffect { rect, effect } => {
                let tint = effect.tint();
                self.stats.visual_effect_draws = self.stats.visual_effect_draws.saturating_add(1);
                self.encode_backdrop(*rect, effect.blur_intensity() * 72.0, tint, tint.a);
            }
            api::DrawCmd::CameraBg { .. } => {}
            api::DrawCmd::Spinner { center, atom, alpha } => {
                self.stats.spinner_draws = self.stats.spinner_draws.saturating_add(1);
                self.encode_spinner(*center, *atom, *alpha)
            }
            api::DrawCmd::ClipPush { rect } => {
                self.clip_stack.push(*rect);
                self.stats.clip_depth_peak =
                    self.stats.clip_depth_peak.max(self.clip_stack.len() as u32);
            }
            api::DrawCmd::ClipPop => {
                let _ = self.clip_stack.pop();
            }
        }
    }

    fn encode_solid(
        &mut self,
        list: &api::DrawList,
        vb: api::VertexSpan,
        ib: api::IndexSpan,
        color: api::Color,
    ) {
        let Some(vertices) = vertex_slice(list, vb) else {
            return;
        };
        self.clear_scratch_draw();
        if ib.len > 0 {
            let Some(indices) = index_slice(list, ib) else {
                return;
            };
            let Some(mode) = normalized_index_mode(indices, vb.offset, vb.len) else {
                return;
            };
            let appended = match mode {
                NormalizedIndexMode::Local => append_local_indexed_gpu_vertices(
                    &mut self.scratch_vertices,
                    &mut self.scratch_indices,
                    vertices,
                    indices,
                    color,
                    true,
                ),
                NormalizedIndexMode::Rebase { .. } => append_indexed_gpu_vertices(
                    &mut self.scratch_vertices,
                    &mut self.scratch_indices,
                    vertices,
                    indices,
                    mode,
                    color,
                    true,
                ),
            };
            if !appended {
                self.clear_scratch_draw();
                return;
            }
        } else if vertices.len() == 4 {
            self.scratch_vertices.extend(
                vertices
                    .iter()
                    .map(|vertex| gpu_vertex(vertex.x, vertex.y, vertex.u, vertex.v, vertex.rgba, color)),
            );
            self.scratch_indices.extend_from_slice(&[0, 1, 2, 2, 1, 3]);
        } else {
            append_gpu_vertices(
                &mut self.scratch_vertices,
                &mut self.scratch_indices,
                vertices,
                color,
                true,
            );
        }
        let triangles = self.scratch_indices.len() / 3;
        self.push_scratch_draw(DrawKind::Solid);
        self.stats.solid_tris = self.stats.solid_tris.saturating_add(triangles as u32);
    }

    fn encode_image(
        &mut self,
        handle: api::ImageHandle,
        dst: api::RectF,
        src: api::RectF,
        alpha: f32,
        sdf: bool,
    ) {
        if dst.w <= 0.0 || dst.h <= 0.0 {
            return;
        }
        let Some(image) = self.image(handle) else {
            return;
        };
        let (sx, sy, sw, sh) = source_rect(src, image.width, image.height);
        let u0 = sx as f32 / image.width.max(1) as f32;
        let v0 = sy as f32 / image.height.max(1) as f32;
        let u1 = (sx + sw) as f32 / image.width.max(1) as f32;
        let v1 = (sy + sh) as f32 / image.height.max(1) as f32;
        if sdf
        {
            let color = api::Color::rgba(1.0, 1.0, 1.0, alpha.clamp(0.0, 1.0));
            let vertices = quad_vertices(dst, u0, v0, u1, v1, color);
            self.push_draw(DrawKind::Sdf { image: handle.0 }, &vertices);
        }
        else if let Some(instance) = ImageInstance::new(dst, [u0, v0, u1, v1], alpha)
        {
            self.push_image_instance(handle.0, image.kind, instance);
        }
        self.stats.image_draws = self.stats.image_draws.saturating_add(1);
    }

    fn encode_glyph_run(&mut self, list: &api::DrawList, run: &api::GlyphRun) {
        let Some(vertices) = vertex_slice(list, run.vb) else {
            return;
        };
        let indices = index_slice(list, run.ib).unwrap_or(&[]);
        self.encode_glyph_vertices(run, vertices, indices);
    }

    fn encode_image_mesh(
        &mut self,
        list: &api::DrawList,
        handle: api::ImageHandle,
        vb: api::VertexSpan,
        ib: api::IndexSpan,
        alpha: f32,
    ) {
        let Some(image) = self.image(handle) else {
            return;
        };
        let Some(vertices) = vertex_slice(list, vb) else {
            return;
        };
        let indices = index_slice(list, ib).unwrap_or(&[]);
        let kind = match image.kind {
            GpuImageKind::Rgba => DrawKind::Rgba { image: handle.0 },
            GpuImageKind::A8 => DrawKind::A8 { image: handle.0 },
        };
        let color = api::Color::rgba(1.0, 1.0, 1.0, alpha.clamp(0.0, 1.0));
        self.clear_scratch_draw();
        if !indices.is_empty() {
            let Some(mode) = normalized_index_mode(indices, vb.offset, vb.len) else {
                return;
            };
            if !append_indexed_gpu_vertices(
                &mut self.scratch_vertices,
                &mut self.scratch_indices,
                vertices,
                indices,
                mode,
                color,
                false,
            ) {
                self.clear_scratch_draw();
                return;
            }
        } else {
            append_gpu_vertices(
                &mut self.scratch_vertices,
                &mut self.scratch_indices,
                vertices,
                color,
                false,
            );
        }
        self.push_scratch_draw(kind);
        self.stats.image_draws = self.stats.image_draws.saturating_add(1);
        self.stats.image_mesh_draws = self.stats.image_mesh_draws.saturating_add(1);
    }

    fn encode_glyph_vertices(
        &mut self,
        run: &api::GlyphRun,
        vertices: &[api::Vertex],
        indices: &[u16],
    ) {
        let Some(image) = self.image(run.atlas) else {
            return;
        };
        let kind = if run.sdf {
            DrawKind::Sdf { image: run.atlas.0 }
        } else {
            match image.kind {
                GpuImageKind::Rgba => DrawKind::Rgba { image: run.atlas.0 },
                GpuImageKind::A8 => DrawKind::A8 { image: run.atlas.0 },
            }
        };
        self.clear_scratch_draw();
        if !indices.is_empty() {
            if !append_local_indexed_gpu_vertices(
                &mut self.scratch_vertices,
                &mut self.scratch_indices,
                vertices,
                indices,
                run.color,
                false,
            ) {
                self.clear_scratch_draw();
                return;
            }
        } else {
            append_gpu_vertices(
                &mut self.scratch_vertices,
                &mut self.scratch_indices,
                vertices,
                run.color,
                false,
            );
        }
        let quads = self.scratch_indices.len() / 6;
        self.push_scratch_draw(kind);
        self.stats.glyph_quads = self.stats.glyph_quads.saturating_add(quads as u32);
        if run.sdf {
            self.stats.sdf_glyph_quads = self.stats.sdf_glyph_quads.saturating_add(quads as u32);
        }
    }

    fn encode_rrect(&mut self, rect: api::RectF, radii: [f32; 4], color: api::Color) {
        if let Some(instance) = RRectInstance::new(rect, radii, color)
        {
            self.push_rrect(instance);
        }
    }

    fn encode_nine_slice(
        &mut self,
        handle: api::ImageHandle,
        rect: api::RectF,
        slice: api::Insets,
        alpha: f32,
    ) {
        let Some(image) = self.image(handle) else {
            return;
        };
        let image_size = [image.width as f32, image.height as f32];
        let kind = image.kind;
        self.stats.nine_slice_draws = self.stats.nine_slice_draws.saturating_add(1);
        if let Some(instance) = NineSliceInstance::new(rect, image_size, slice, alpha)
        {
            self.push_nine_slice_instance(handle.0, kind, instance);
        }
    }

    fn encode_backdrop(&mut self, rect: api::RectF, sigma: f32, tint: api::Color, alpha: f32) {
        if rect.w <= 0.0 || rect.h <= 0.0 {
            return;
        }
        let logical_w = logical_dimension(self.width, self.scale).max(1.0);
        let logical_h = logical_dimension(self.height, self.scale).max(1.0);
        let u0 = rect.x / logical_w;
        let v0 = rect.y / logical_h;
        let u1 = (rect.x + rect.w) / logical_w;
        let v1 = (rect.y + rect.h) / logical_h;
        let color = api::Color::rgba(tint.r, tint.g, tint.b, tint.a * alpha.clamp(0.0, 1.0));
        let vertices = quad_vertices(rect, u0, v0, u1, v1, color);
        let sigma = sigma.clamp(0.0, MAX_BLUR_SIGMA);
        self.push_draw(DrawKind::Backdrop { rect, sigma }, &vertices);
    }

    fn encode_spinner(&mut self, center: [f32; 2], atom: f32, alpha: f32) {
        if let Some(instance) = SpinnerInstance::new(center, atom, alpha)
        {
            self.push_spinner_instance(instance);
        }
    }

    fn ensure_scene_target(&mut self) {
        if self.scene_target.is_some() {
            return;
        }
        self.scene_target = Some(create_color_target(
            &self.device,
            &self.programs,
            "oxide-webgpu-scene",
            self.config.format,
            self.width,
            self.height,
        ));
        self.stats.texture_creates = self.stats.texture_creates.saturating_add(1);
        self.stats.bind_group_creates = self.stats.bind_group_creates.saturating_add(1);
        self.stats.target_texture_creates = self.stats.target_texture_creates.saturating_add(1);
        self.stats.target_bind_group_creates =
            self.stats.target_bind_group_creates.saturating_add(1);
    }

    fn ensure_scene_depth_target(&mut self) {
        if self.scene_depth_target.is_some() {
            return;
        }
        self.scene_depth_target = Some(create_depth_target(
            &self.device,
            "oxide-webgpu-scene-depth",
            self.width,
            self.height,
        ));
        self.stats.texture_creates = self.stats.texture_creates.saturating_add(1);
        self.stats.target_texture_creates = self.stats.target_texture_creates.saturating_add(1);
    }

    fn ensure_scratch_target(&mut self) {
        if self.scratch_target.is_some() {
            return;
        }
        self.scratch_target = Some(create_color_target(
            &self.device,
            &self.programs,
            "oxide-webgpu-scratch",
            self.config.format,
            self.width,
            self.height,
        ));
        self.stats.texture_creates = self.stats.texture_creates.saturating_add(1);
        self.stats.bind_group_creates = self.stats.bind_group_creates.saturating_add(1);
        self.stats.target_texture_creates = self.stats.target_texture_creates.saturating_add(1);
        self.stats.target_bind_group_creates =
            self.stats.target_bind_group_creates.saturating_add(1);
    }

    fn prewarm_auxiliary_targets(&mut self, backdrop: bool, scene3d: bool) {
        if backdrop {
            self.ensure_scene_target();
            self.ensure_scratch_target();
        }
        if scene3d {
            self.ensure_scene_depth_target();
        }
    }

    fn drop_auxiliary_targets(&mut self) {
        self.scene_target = None;
        self.scene_depth_target = None;
        self.scratch_target = None;
    }

    fn upload_frame_buffers(&mut self) {
        let geometry_bytes = self.frame.geometry.byte_len();
        self.frame.geometry.align_uploads();
        self.stats.geometry_bytes_copied = self
            .stats
            .geometry_bytes_copied
            .saturating_add(self.frame.geometry.byte_len().saturating_sub(geometry_bytes) as u64);
        let vertex_bytes = bytemuck::cast_slice(&self.frame.geometry.vertices);
        let rrect_instance_bytes = bytemuck::cast_slice(&self.frame.rrect_instances);
        let image_instance_bytes = bytemuck::cast_slice(&self.frame.image_instances);
        let nine_slice_instance_bytes = bytemuck::cast_slice(&self.frame.nine_slice_instances);
        let spinner_instance_bytes = bytemuck::cast_slice(&self.frame.spinner_instances);
        let neon_marker_instance_bytes = bytemuck::cast_slice(&self.frame.neon_marker_instances);
        if !spinner_instance_bytes.is_empty()
        {
            write_viewport_uniform(
                &self.queue,
                &self.viewport_buffer,
                self.width,
                self.height,
                self.scale,
                self.animation_phase,
            );
            self.stats.buffer_upload_bytes = self.stats.buffer_upload_bytes
                .saturating_add(PREPARED_PROPERTY_UNIFORM_SIZE);
        }
        let index_bytes_u16 = bytemuck::cast_slice(&self.frame.geometry.indices_u16);
        let index_bytes_u32 = bytemuck::cast_slice(&self.frame.geometry.indices_u32);
        if ensure_buffer(
            &self.device,
            &mut self.vertex_buffer,
            &mut self.vertex_capacity,
            vertex_bytes.len() as u64,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            "oxide-webgpu-vertices",
        ) {
            self.stats.buffer_grows = self.stats.buffer_grows.saturating_add(1);
            self.stats.draw_buffer_grows = self.stats.draw_buffer_grows.saturating_add(1);
        }
        if ensure_buffer(
            &self.device,
            &mut self.rrect_instance_buffer,
            &mut self.rrect_instance_capacity,
            rrect_instance_bytes.len() as u64,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            "oxide-webgpu-rrect-instances",
        ) {
            self.stats.buffer_grows = self.stats.buffer_grows.saturating_add(1);
            self.stats.draw_buffer_grows = self.stats.draw_buffer_grows.saturating_add(1);
        }
        if ensure_buffer(
            &self.device,
            &mut self.image_instance_buffer,
            &mut self.image_instance_capacity,
            image_instance_bytes.len() as u64,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            "oxide-webgpu-image-instances",
        ) {
            self.stats.buffer_grows = self.stats.buffer_grows.saturating_add(1);
            self.stats.draw_buffer_grows = self.stats.draw_buffer_grows.saturating_add(1);
        }
        if ensure_buffer(
            &self.device,
            &mut self.nine_slice_instance_buffer,
            &mut self.nine_slice_instance_capacity,
            nine_slice_instance_bytes.len() as u64,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            "oxide-webgpu-nine-slice-instances",
        ) {
            self.stats.buffer_grows = self.stats.buffer_grows.saturating_add(1);
            self.stats.draw_buffer_grows = self.stats.draw_buffer_grows.saturating_add(1);
        }
        if ensure_buffer(
            &self.device,
            &mut self.spinner_instance_buffer,
            &mut self.spinner_instance_capacity,
            spinner_instance_bytes.len() as u64,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            "oxide-webgpu-spinner-instances",
        ) {
            self.stats.buffer_grows = self.stats.buffer_grows.saturating_add(1);
            self.stats.draw_buffer_grows = self.stats.draw_buffer_grows.saturating_add(1);
        }
        if ensure_buffer(
            &self.device,
            &mut self.neon_marker_instance_buffer,
            &mut self.neon_marker_instance_capacity,
            neon_marker_instance_bytes.len() as u64,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            "oxide-webgpu-neon-marker-instances",
        ) {
            self.stats.buffer_grows = self.stats.buffer_grows.saturating_add(1);
            self.stats.draw_buffer_grows = self.stats.draw_buffer_grows.saturating_add(1);
        }
        if ensure_buffer(
            &self.device,
            &mut self.index_buffer_u16,
            &mut self.index_capacity_u16,
            index_bytes_u16.len() as u64,
            wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            "oxide-webgpu-indices-u16",
        ) {
            self.stats.buffer_grows = self.stats.buffer_grows.saturating_add(1);
            self.stats.draw_buffer_grows = self.stats.draw_buffer_grows.saturating_add(1);
        }
        if ensure_buffer(
            &self.device,
            &mut self.index_buffer_u32,
            &mut self.index_capacity_u32,
            index_bytes_u32.len() as u64,
            wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            "oxide-webgpu-indices-u32",
        ) {
            self.stats.buffer_grows = self.stats.buffer_grows.saturating_add(1);
            self.stats.draw_buffer_grows = self.stats.draw_buffer_grows.saturating_add(1);
        }
        if !vertex_bytes.is_empty() {
            let Some(buffer) = &self.vertex_buffer else {
                return;
            };
            self.queue.write_buffer(buffer, 0, vertex_bytes);
            self.stats.buffer_upload_bytes = self
                .stats
                .buffer_upload_bytes
                .saturating_add(vertex_bytes.len() as u64);
        }
        if !rrect_instance_bytes.is_empty() {
            if let Some(buffer) = &self.rrect_instance_buffer {
                self.queue.write_buffer(buffer, 0, rrect_instance_bytes);
                self.stats.buffer_upload_bytes = self
                    .stats
                    .buffer_upload_bytes
                    .saturating_add(rrect_instance_bytes.len() as u64);
            }
        }
        if !image_instance_bytes.is_empty() {
            if let Some(buffer) = &self.image_instance_buffer {
                self.queue.write_buffer(buffer, 0, image_instance_bytes);
                self.stats.buffer_upload_bytes = self
                    .stats
                    .buffer_upload_bytes
                    .saturating_add(image_instance_bytes.len() as u64);
            }
        }
        if !nine_slice_instance_bytes.is_empty() {
            if let Some(buffer) = &self.nine_slice_instance_buffer {
                self.queue.write_buffer(buffer, 0, nine_slice_instance_bytes);
                self.stats.buffer_upload_bytes = self
                    .stats
                    .buffer_upload_bytes
                    .saturating_add(nine_slice_instance_bytes.len() as u64);
            }
        }
        if !spinner_instance_bytes.is_empty() {
            if let Some(buffer) = &self.spinner_instance_buffer {
                self.queue.write_buffer(buffer, 0, spinner_instance_bytes);
                self.stats.buffer_upload_bytes = self
                    .stats
                    .buffer_upload_bytes
                    .saturating_add(spinner_instance_bytes.len() as u64);
            }
        }
        if !neon_marker_instance_bytes.is_empty() {
            if let Some(buffer) = &self.neon_marker_instance_buffer {
                self.queue.write_buffer(buffer, 0, neon_marker_instance_bytes);
                self.stats.buffer_upload_bytes = self
                    .stats
                    .buffer_upload_bytes
                    .saturating_add(neon_marker_instance_bytes.len() as u64);
            }
        }
        if !index_bytes_u16.is_empty() {
            if let Some(buffer) = &self.index_buffer_u16 {
                self.queue.write_buffer(buffer, 0, index_bytes_u16);
                self.stats.buffer_upload_bytes = self
                    .stats
                    .buffer_upload_bytes
                    .saturating_add(index_bytes_u16.len() as u64);
            }
        }
        if !index_bytes_u32.is_empty() {
            if let Some(buffer) = &self.index_buffer_u32 {
                self.queue.write_buffer(buffer, 0, index_bytes_u32);
                self.stats.buffer_upload_bytes = self
                    .stats
                    .buffer_upload_bytes
                    .saturating_add(index_bytes_u32.len() as u64);
            }
        }
    }

    fn upload_scene3d_uniforms(&mut self) {
        if self.scene3d_uniform_bytes.is_empty() {
            return;
        }
        let needed = self.scene3d_uniform_bytes.len() as u64;
        if self.scene3d_uniform_buffer.is_none() || self.scene3d_uniform_capacity < needed {
            let next = needed.next_power_of_two().max(SCENE3D_UNIFORM_STRIDE as u64);
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("oxide-webgpu-scene3d-uniforms"),
                size: next,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("oxide-webgpu-scene3d-bind-group"),
                layout: &self.programs.scene3d_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &buffer,
                        offset: 0,
                        size: core::num::NonZeroU64::new(SCENE3D_UNIFORM_STRIDE as u64),
                    }),
                }],
            });
            self.scene3d_uniform_buffer = Some(buffer);
            self.scene3d_bind_group = Some(bind_group);
            self.scene3d_uniform_capacity = next;
            self.stats.buffer_grows = self.stats.buffer_grows.saturating_add(1);
            self.stats.bind_group_creates = self.stats.bind_group_creates.saturating_add(1);
            self.stats.scene3d_buffer_grows = self.stats.scene3d_buffer_grows.saturating_add(1);
            self.stats.scene3d_bind_group_creates =
                self.stats.scene3d_bind_group_creates.saturating_add(1);
        }
        if let Some(buffer) = &self.scene3d_uniform_buffer {
            self.queue.write_buffer(buffer, 0, &self.scene3d_uniform_bytes);
            self.stats.buffer_upload_bytes = self
                .stats
                .buffer_upload_bytes
                .saturating_add(self.scene3d_uniform_bytes.len() as u64);
        }
    }

    fn write_effect_uniform(&mut self, sigma: f32) {
        self.ensure_effect_uniform_capacity(1);
        let radius = sigma.clamp(0.0, MAX_BLUR_SIGMA);
        let texel_x = 1.0 / self.width.max(1) as f32;
        let texel_y = 1.0 / self.height.max(1) as f32;
        let bytes = f32x4_bytes([texel_x, texel_y, radius, 0.0]);
        self.queue.write_buffer(&self.effect_buffer, 0, &bytes);
        self.stats.buffer_upload_bytes =
            self.stats.buffer_upload_bytes.saturating_add(bytes.len() as u64);
        self.stats.effect_uniform_writes = self.stats.effect_uniform_writes.saturating_add(1);
        self.stats.effect_uniform_bytes =
            self.stats.effect_uniform_bytes.saturating_add(bytes.len() as u64);
        self.stats.effect_uniform_slots = self.stats.effect_uniform_slots.saturating_add(1);
    }

    fn prepare_effect_uniforms(&mut self) {
        if !self.effect_uniform_batch_enabled {
            return;
        }
        let effect_count = self.frame.effect_count;
        if effect_count == 0 {
            return;
        }
        let texel_x = 1.0 / self.width.max(1) as f32;
        let texel_y = 1.0 / self.height.max(1) as f32;
        if self.frame.effect_single_uniform_slot {
            self.ensure_effect_uniform_capacity(1);
            let radius = self.frame.effect_shared_sigma.clamp(0.0, MAX_BLUR_SIGMA);
            let bytes = f32x4_bytes([texel_x, texel_y, radius, 0.0]);
            self.queue.write_buffer(&self.effect_buffer, 0, &bytes);
            self.stats.buffer_upload_bytes =
                self.stats.buffer_upload_bytes.saturating_add(bytes.len() as u64);
            self.stats.effect_uniform_writes = self.stats.effect_uniform_writes.saturating_add(1);
            self.stats.effect_uniform_bytes =
                self.stats.effect_uniform_bytes.saturating_add(bytes.len() as u64);
            self.stats.effect_uniform_slots =
                self.stats.effect_uniform_slots.saturating_add(effect_count as u32);
            return;
        }
        self.ensure_effect_uniform_capacity(effect_count);
        let needed = effect_uniform_needed_bytes(effect_count, self.effect_uniform_stride);
        self.effect_uniform_bytes.clear();
        self.effect_uniform_bytes.resize(needed as usize, 0);
        let mut effect_index = 0_u32;
        for draw in &mut self.frame.draws {
            let DrawKind::Backdrop { sigma, .. } = draw.kind else {
                continue;
            };
            let offset = (effect_index as u64).saturating_mul(self.effect_uniform_stride);
            let offset_usize = offset as usize;
            let radius = sigma.clamp(0.0, MAX_BLUR_SIGMA);
            let bytes = f32x4_bytes([texel_x, texel_y, radius, 0.0]);
            self.effect_uniform_bytes[offset_usize..offset_usize + EFFECT_UNIFORM_SIZE_BYTES]
                .copy_from_slice(&bytes);
            draw.effect_uniform_offset = offset as u32;
            effect_index = effect_index.saturating_add(1);
        }
        self.queue.write_buffer(&self.effect_buffer, 0, &self.effect_uniform_bytes);
        self.stats.buffer_upload_bytes =
            self.stats.buffer_upload_bytes.saturating_add(self.effect_uniform_bytes.len() as u64);
        self.stats.effect_uniform_writes = self.stats.effect_uniform_writes.saturating_add(1);
        self.stats.effect_uniform_bytes =
            self.stats.effect_uniform_bytes.saturating_add(self.effect_uniform_bytes.len() as u64);
        self.stats.effect_uniform_slots =
            self.stats.effect_uniform_slots.saturating_add(effect_count as u32);
    }

    fn ensure_effect_uniform_capacity(&mut self, count: usize) {
        let needed = effect_uniform_needed_bytes(count.max(1), self.effect_uniform_stride);
        if self.effect_uniform_capacity >= needed {
            return;
        }
        let next = needed.next_power_of_two().max(EFFECT_UNIFORM_SIZE);
        let (buffer, bind_group, capacity) =
            create_effect_bind_group(&self.device, &self.programs, next);
        self.effect_buffer = buffer;
        self.effect_bind_group = bind_group;
        self.effect_uniform_capacity = capacity;
        self.stats.buffer_grows = self.stats.buffer_grows.saturating_add(1);
        self.stats.bind_group_creates = self.stats.bind_group_creates.saturating_add(1);
        self.stats.effect_buffer_grows = self.stats.effect_buffer_grows.saturating_add(1);
        self.stats.effect_bind_group_creates =
            self.stats.effect_bind_group_creates.saturating_add(1);
    }

    fn ensure_id_mask_uniform_capacity(&mut self, needed: usize) {
        let needed = needed.max(ID_MASK_COMPOSITOR_UNIFORM_SIZE_BYTES) as u64;
        if self.id_mask_uniform_capacity >= needed {
            return;
        }
        let capacity = needed.next_power_of_two();
        self.id_mask_uniform_buffer = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("oxide-webgpu-id-mask-uniform-arena"),
            size: capacity,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
        self.id_mask_uniform_capacity = capacity;
        self.id_mask_raster_bind_group = None;
        let uniform_buffer = self.id_mask_uniform_buffer.as_ref().unwrap();
        for entry in &mut self.id_mask_field_cache {
            rebuild_id_mask_target_bind_groups(
                &self.device,
                &self.programs,
                uniform_buffer,
                &mut entry.targets,
            );
        }
        self.stats.buffer_grows = self.stats.buffer_grows.saturating_add(1);
        self.stats.id_mask_buffer_grows = self.stats.id_mask_buffer_grows.saturating_add(1);
        let recreated = self.id_mask_field_cache.len().saturating_mul(4) as u32;
        self.stats.bind_group_creates = self.stats.bind_group_creates.saturating_add(recreated);
        self.stats.id_mask_bind_group_creates =
            self.stats.id_mask_bind_group_creates.saturating_add(recreated);
    }

    fn id_mask_uniform_capacity_needed(&self) -> usize {
        let alignment = self.device.limits().min_uniform_buffer_offset_alignment.max(1) as usize;
        self.id_mask_draws.iter().fold(0_usize, |mut bytes, draw| {
            bytes = align_to(bytes as u64, alignment as u64) as usize
                + ID_MASK_RASTER_UNIFORM_SIZE_BYTES;
            bytes = align_to(bytes as u64, alignment as u64) as usize
                + ID_MASK_FIELD_UNIFORM_SIZE_BYTES;
            let mut jump = draw.mask_width.max(draw.mask_height).max(1).next_power_of_two() / 2;
            while jump >= 1 {
                bytes = align_to(bytes as u64, alignment as u64) as usize
                    + ID_MASK_FIELD_UNIFORM_SIZE_BYTES;
                jump /= 2;
            }
            align_to(bytes as u64, alignment as u64) as usize
                + ID_MASK_COMPOSITOR_UNIFORM_SIZE_BYTES
        })
    }

    fn resolve_id_mask_draws(&mut self) -> bool {
        self.id_mask_resolved_draws.clear();
        self.id_mask_resolved_draws.reserve(self.id_mask_draws.len());
        for draw_index in 0..self.id_mask_draws.len() {
            let draw = self.id_mask_draws[draw_index];
            let cache_start = draw.vertex_cache_first as usize;
            let cache_count = draw.vertex_cache_count as usize;
            let cache_end = cache_start.saturating_add(cache_count);
            if draw.vertex_count == 0
                || cache_end > self.id_mask_draw_chunk_indices.len()
                || cache_end > self.id_mask_draw_chunk_keys.len()
            {
                return false;
            }
            if let Some(targets) =
                self.id_mask_field_cache_hit(draw.field_key, cache_start, cache_count)
            {
                self.id_mask_resolved_draws.push(IdMaskResolvedDraw {
                    targets,
                    cache_hit: true,
                });
                continue;
            }
            self.stats.id_mask_cache_misses =
                self.stats.id_mask_cache_misses.saturating_add(1);
            self.stats.backend_cache_misses =
                self.stats.backend_cache_misses.saturating_add(1);
            let width = draw.mask_width.max(1);
            let height = draw.mask_height.max(1);
            let packed = self.programs.id_mask_packed.is_some()
                && id_mask_packed_coordinates_fit(width, height);
            let required = id_mask_render_targets_bytes(width, height, packed);
            let admission = self.prepare_id_mask_cache_admission(required, width, height);
            let cacheable = admission.is_some();
            let reusable = admission.flatten();
            let Some(targets) = self.new_id_mask_render_targets(width, height, reusable) else {
                return false;
            };
            if cacheable {
                self.retain_id_mask_field_cache_entry(
                    draw.field_key,
                    cache_start,
                    cache_count,
                    &targets,
                );
            }
            self.id_mask_resolved_draws.push(IdMaskResolvedDraw {
                targets,
                cache_hit: false,
            });
        }
        true
    }

    fn prepare_id_mask_uniforms(&mut self) {
        self.id_mask_uniform_bytes.clear();
        self.id_mask_uniform_offsets.clear();
        self.id_mask_field_uniform_offsets.clear();
        self.id_mask_uniform_offsets.reserve(self.id_mask_draws.len());
        let alignment = self.device.limits().min_uniform_buffer_offset_alignment.max(1) as usize;
        for (draw_index, draw) in self.id_mask_draws.iter().enumerate() {
            let cache_hit = self.id_mask_resolved_draws[draw_index].cache_hit;
            let raster = if cache_hit {
                0
            } else {
                let offset = align_uniform_bytes(&mut self.id_mask_uniform_bytes, alignment);
                write_id_mask_raster_uniform_bytes(
                    &mut self.id_mask_uniform_bytes,
                    draw.mask_width.max(1),
                    draw.mask_height.max(1),
                    draw.projection,
                );
                offset
            };
            let field_first = self.id_mask_field_uniform_offsets.len();
            if !cache_hit {
                let offset = align_uniform_bytes(&mut self.id_mask_uniform_bytes, alignment);
                self.id_mask_uniform_bytes.extend_from_slice(&id_mask_field_uniform_bytes(
                    draw.mask_width.max(1),
                    draw.mask_height.max(1),
                    0.0,
                ));
                self.id_mask_field_uniform_offsets.push(offset);
                let mut jump =
                    draw.mask_width.max(draw.mask_height).max(1).next_power_of_two() / 2;
                while jump >= 1 {
                    let offset = align_uniform_bytes(&mut self.id_mask_uniform_bytes, alignment);
                    self.id_mask_uniform_bytes.extend_from_slice(&id_mask_field_uniform_bytes(
                        draw.mask_width.max(1),
                        draw.mask_height.max(1),
                        jump as f32,
                    ));
                    self.id_mask_field_uniform_offsets.push(offset);
                    jump /= 2;
                }
            }

            let compositor = align_uniform_bytes(&mut self.id_mask_uniform_bytes, alignment);
            write_id_mask_compositor_uniform_bytes(&mut self.id_mask_uniform_bytes, draw);
            self.id_mask_uniform_offsets.push(IdMaskUniformOffsets {
                raster,
                field_first,
                field_count: self.id_mask_field_uniform_offsets.len() - field_first,
                compositor,
                cache_hit,
            });
        }
    }

    fn ensure_id_mask_raster_bind_group(&mut self) {
        if self.id_mask_raster_bind_group.is_none() {
            let Some(uniform_buffer) = self.id_mask_uniform_buffer.as_ref() else { return };
            self.id_mask_raster_bind_group = Some(self.device.create_bind_group(
                &wgpu::BindGroupDescriptor {
                label: Some("oxide-webgpu-id-mask-raster-bind-group"),
                layout: &self.programs.id_mask_raster_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_binding(uniform_buffer, ID_MASK_RASTER_UNIFORM_SIZE),
                }],
            }));
            self.stats.bind_group_creates = self.stats.bind_group_creates.saturating_add(1);
            self.stats.id_mask_bind_group_creates =
                self.stats.id_mask_bind_group_creates.saturating_add(1);
        }
    }

    fn new_id_mask_render_targets(
        &mut self,
        width: u32,
        height: u32,
        reusable: Option<IdMaskRenderTargets>,
    ) -> Option<IdMaskRenderTargets> {
        let packed = self.programs.id_mask_packed.is_some()
            && id_mask_packed_coordinates_fit(width, height);
        if let Some(targets) = reusable {
            if targets.width == width
                && targets.height == height
                && targets.packed_fields() == packed
            {
                return Some(targets);
            }
        }
        let uniform_buffer = self.id_mask_uniform_buffer.as_ref()?;
        let city_texture =
            create_id_mask_texture(&self.device, "oxide-webgpu-id-mask-city", width, height);
        let neighborhood_texture = create_id_mask_texture(
            &self.device,
            "oxide-webgpu-id-mask-neighborhood",
            width,
            height,
        );
        let city_view = city_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let neighborhood_view =
            neighborhood_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let fields = if packed {
            let programs = self.programs.id_mask_packed.as_ref()?;
            create_packed_id_mask_field_targets(
                &self.device,
                programs,
                uniform_buffer,
                &city_view,
                &neighborhood_view,
                width,
                height,
            )
        } else {
            create_wide_id_mask_field_targets(
                &self.device,
                &self.programs.id_mask_wide,
                uniform_buffer,
                &city_view,
                &neighborhood_view,
                width,
                height,
            )
        };
        let targets = IdMaskRenderTargets {
            width,
            height,
            city_texture,
            neighborhood_texture,
            city_view,
            neighborhood_view,
            fields,
        };
        let texture_creates = if packed { 4 } else { 6 };
        self.stats.texture_creates = self.stats.texture_creates.saturating_add(texture_creates);
        self.stats.id_mask_texture_creates =
            self.stats.id_mask_texture_creates.saturating_add(texture_creates);
        self.stats.bind_group_creates = self.stats.bind_group_creates.saturating_add(4);
        self.stats.id_mask_bind_group_creates =
            self.stats.id_mask_bind_group_creates.saturating_add(4);
        Some(targets)
    }

    #[cfg(feature = "snapshot-tests")]
    fn begin_id_mask_snapshot_readback(&mut self) -> Result<(), api::RenderError>
    {
        if self.id_mask_snapshot_readback.is_some()
        {
            return Err(api::RenderError::InvalidOperation("ID-mask snapshot readback pending"));
        }
        let targets = self.id_mask_snapshot_targets.as_ref().ok_or(
            api::RenderError::InvalidOperation("ID-mask snapshot unavailable"),
        )?;
        let width = targets.width;
        let height = targets.height;
        let final_fields = targets.final_fields();
        let packed_fields = targets.packed_fields();
        let city = create_id_mask_readback_plane(
            &self.device,
            "oxide-webgpu-id-mask-city-readback",
            width,
            height,
            1,
        );
        let neighborhood = create_id_mask_readback_plane(
            &self.device,
            "oxide-webgpu-id-mask-neighborhood-readback",
            width,
            height,
            1,
        );
        let city_field = create_id_mask_readback_plane(
            &self.device,
            "oxide-webgpu-id-mask-city-field-readback",
            width,
            height,
            8,
        );
        let seam_field = if packed_fields {
            None
        } else {
            Some(create_id_mask_readback_plane(
                &self.device,
                "oxide-webgpu-id-mask-seam-field-readback",
                width,
                height,
                8,
            ))
        };
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("oxide-webgpu-id-mask-snapshot-readback"),
        });
        copy_id_mask_texture_to_plane(&mut encoder, &targets.city_texture, &city, width, height);
        copy_id_mask_texture_to_plane(
            &mut encoder,
            &targets.neighborhood_texture,
            &neighborhood,
            width,
            height,
        );
        match final_fields {
            IdMaskFieldPair::Packed { texture, .. } => {
                copy_id_mask_texture_to_plane(
                    &mut encoder,
                    texture,
                    &city_field,
                    width,
                    height,
                );
            }
            IdMaskFieldPair::Wide { city_texture, seam_texture, .. } => {
                copy_id_mask_texture_to_plane(
                    &mut encoder,
                    city_texture,
                    &city_field,
                    width,
                    height,
                );
                if let Some(seam_field) = seam_field.as_ref() {
                    copy_id_mask_texture_to_plane(
                        &mut encoder,
                        seam_texture,
                        seam_field,
                        width,
                        height,
                    );
                }
            }
        }
        self.queue.submit(Some(encoder.finish()));
        let remaining = Rc::new(Cell::new(if packed_fields { 3_u8 } else { 4_u8 }));
        let failed = Rc::new(Cell::new(false));
        for plane in [&city, &neighborhood, &city_field]
        {
            let remaining = Rc::clone(&remaining);
            let failed = Rc::clone(&failed);
            plane.buffer.slice(..).map_async(wgpu::MapMode::Read, move |result| {
                failed.set(failed.get() || result.is_err());
                remaining.set(remaining.get().saturating_sub(1));
            });
        }
        if let Some(seam_field) = seam_field.as_ref()
        {
            let remaining = Rc::clone(&remaining);
            let failed = Rc::clone(&failed);
            seam_field.buffer.slice(..).map_async(wgpu::MapMode::Read, move |result| {
                failed.set(failed.get() || result.is_err());
                remaining.set(remaining.get().saturating_sub(1));
            });
        }
        self.id_mask_snapshot_readback = Some(PendingIdMaskReadback {
            width,
            height,
            city,
            neighborhood,
            city_field,
            seam_field,
            packed_fields,
            remaining,
            failed,
        });
        Ok(())
    }

    #[cfg(feature = "snapshot-tests")]
    fn collect_id_mask_snapshot_readback(
        &mut self,
    ) -> Option<Result<WebIdMaskSnapshotReadback, api::RenderError>>
    {
        let pending = self.id_mask_snapshot_readback.as_ref()?;
        if pending.remaining.get() != 0
        {
            return None;
        }
        let pending = match self.id_mask_snapshot_readback.take()
        {
            Some(pending) => pending,
            None => return None,
        };
        if pending.failed.get()
        {
            return Some(Err(api::RenderError::Io(String::from(
                "ID-mask snapshot buffer mapping failed",
            ))));
        }
        let city = read_id_mask_plane(&pending.city, pending.height);
        let neighborhood = read_id_mask_plane(&pending.neighborhood, pending.height);
        let packed_field_bytes = read_id_mask_plane(&pending.city_field, pending.height);
        let (city_field, seam_field) = if pending.packed_fields
        {
            decode_web_rgba16_uint_fields(
                &packed_field_bytes,
                &city,
                &neighborhood,
                pending.width,
                pending.height,
            )
        }
        else
        {
            let seam_field = match pending.seam_field.as_ref()
            {
                Some(seam_field) => seam_field,
                None => {
                    return Some(Err(api::RenderError::Io(String::from(
                        "ID-mask wide snapshot seam field unavailable",
                    ))));
                }
            };
            (
                decode_web_rgba16_float(&packed_field_bytes),
                decode_web_rgba16_float(&read_id_mask_plane(seam_field, pending.height)),
            )
        };
        pending.city.buffer.unmap();
        pending.neighborhood.buffer.unmap();
        pending.city_field.buffer.unmap();
        if let Some(seam_field) = pending.seam_field.as_ref()
        {
            seam_field.buffer.unmap();
        }
        let pixels = u64::from(pending.width).saturating_mul(u64::from(pending.height));
        let wide_field_logical_bytes = pixels.saturating_mul(32);
        let field_logical_bytes = pixels.saturating_mul(if pending.packed_fields { 16 } else { 32 });
        Some(Ok(WebIdMaskSnapshotReadback {
            width: pending.width as usize,
            height: pending.height as usize,
            city,
            neighborhood,
            city_field,
            seam_field,
            packed_fields: pending.packed_fields,
            field_logical_bytes,
            wide_field_logical_bytes,
        }))
    }
}

impl WebGpuRenderer
{
   fn encode_snapshot_layers(&mut self, snapshot: &api::RenderSnapshot) -> Option<Result<(), api::RenderSnapshotError>>
   {
      let mut all_layers = snapshot.instance_count() != 0;
      snapshot.visit_instances(|instance| {
         all_layers = all_layers && instance.layer.is_some();
      });
      if !all_layers
      {
         return None;
      }
      let mut supported = self.frame.draws.is_empty()
         && self.frame.layer_passes.is_empty()
         && !self.scene3d_active
         && self.id_mask_draws.is_empty();
      let viewport = [
         logical_dimension(self.width, self.scale).max(1.0),
         logical_dimension(self.height, self.scale).max(1.0),
      ];
      let reuse_plan = self.prepared_layer_snapshot.as_ref().is_some_and(|cached| {
         cached.ptr_eq(snapshot)
            && self.prepared_layer_plan.len() as u64 == snapshot.instance_count()
      });
      let mut layer_keys = core::mem::take(&mut self.prepared_layer_key_indices);
      let mut plan = core::mem::take(&mut self.prepared_layer_plan);
      if !reuse_plan
      {
         layer_keys.clear();
         plan.clear();
         snapshot.visit_instances(|instance| {
            if !supported
            {
               return;
            }
            let Some(layer) = instance.layer else
            {
               supported = false;
               return;
            };
            let Some(uniform) = prepared_dynamic_uniform(
               snapshot,
               &instance.property_slots,
               instance.origin,
               viewport,
               self.animation_phase,
            ) else
            {
               supported = false;
               return;
            };
            let Some(frame) = prepared_layer_frame(self, layer, &instance.chunk, uniform, instance.clip) else
            {
               supported = false;
               return;
            };
            if layer.id == 0 || !instance.dynamic_clips.is_empty()
            {
               supported = false;
               return;
            }
            let animated = instance.chunk.draw_list().items.iter().any(|command| {
               matches!(command, api::DrawCmd::Spinner { .. })
            });
            let duplicate = match layer_keys.get(&layer.id).copied()
            {
               None =>
               {
                  layer_keys.insert(layer.id, plan.len());
                  false
               }
               Some(index) if plan[index].frame.key == frame.key =>
               {
                  plan[index].frame.force_refresh |= frame.force_refresh;
                  plan[index].animated |= animated;
                  true
               }
               Some(_) =>
               {
                  supported = false;
                  return;
               }
            };
            plan.push(PreparedLayerPlanEntry {
               frame,
               chunk: instance.chunk.clone(),
               duplicate,
               animated,
            });
         });
      }
      if !supported
      {
         layer_keys.clear();
         plan.clear();
         self.prepared_layer_key_indices = layer_keys;
         self.prepared_layer_plan = plan;
         self.prepared_layer_snapshot = None;
         return Some(self.encode_snapshot_flat(snapshot));
      }
      if !reuse_plan
      {
         self.prepared_layer_snapshot = Some(snapshot.clone());
      }
      let required_layer_bytes = plan.iter().filter(|entry| !entry.duplicate).fold(0_u64, |total, entry| {
         total.saturating_add(saturating_texture_bytes(
            u64::from(entry.frame.width),
            u64::from(entry.frame.height),
            color_texture_bytes_per_pixel(self.config.format),
         ))
      });
      if required_layer_bytes > self.layer_cache_budget_bytes
      {
         self.prepared_layer_key_indices = layer_keys;
         self.prepared_layer_plan = plan;
         self.prepared_layer_snapshot = None;
         return Some(self.encode_snapshot_flat(snapshot));
      }
      for entry in &mut plan
      {
         if entry.animated
         {
            entry.frame.viewport[11] = self.animation_phase;
            entry.frame.force_refresh = true;
         }
         let frame = entry.frame;
         self.stats.layer_draws = self.stats.layer_draws.saturating_add(1);
         let hit = entry.duplicate
            || !frame.force_refresh && self.cached_prepared_layer(frame, &entry.chunk).is_some();
         if hit
         {
            self.touch_layer(frame.key.id);
            self.stats.layer_cache_hits = self.stats.layer_cache_hits.saturating_add(1);
            self.stats.layer_cache_skipped_draws = self.stats.layer_cache_skipped_draws.saturating_add(
               entry.chunk.draw_list().items.len().min(u32::MAX as usize) as u32,
            );
         }
         else
         {
            self.stats.layer_cache_misses = self.stats.layer_cache_misses.saturating_add(1);
            if !self.ensure_prepared_layer(frame, &entry.chunk)
            {
               self.frame.clear();
               self.target_stack.clear();
               self.prepared_layer_key_indices = layer_keys;
               self.prepared_layer_plan.clear();
               self.prepared_layer_snapshot = None;
               return Some(self.encode_snapshot_flat(snapshot));
            }
            let start = self.frame.draws.len();
            self.target_stack.push(frame.key.id);
            let mut index = 0;
            self.encode_items(entry.chunk.draw_list(), &mut index, false);
            let _ = self.target_stack.pop();
            let end = self.frame.draws.len();
            self.frame.layer_passes.push(FrameLayerPass { id: frame.key.id, start, end });
         }
         self.push_layer_draw(frame.key.id, frame.rect);
      }
      self.prepared_layer_key_indices = layer_keys;
      self.prepared_layer_plan = plan;
      self.prepared_frame_active = false;
      self.prepared_snapshot_bundle = None;
      self.prepared_snapshot_bundle_active = false;
      Some(Ok(()))
   }

   /// Encodes immutable snapshot chunks through persistent geometry and a completion-safe
   /// frame ring for dynamic transform and opacity records.
   pub fn encode_snapshot(&mut self, snapshot: &api::RenderSnapshot) -> Result<(), api::RenderSnapshotError>
   {
      self.prepared_frame_plan.clear();
      self.prepared_frame_active = false;
      self.prepared_snapshot_bundle_active = false;
      if let Some(result) = self.encode_snapshot_layers(snapshot)
      {
         return result;
      }
      if self.prepared_chunks.budget_bytes == 0
      {
         return self.encode_snapshot_flat(snapshot);
      }
      let mut supported = self.frame.draws.is_empty()
         && self.frame.layer_passes.is_empty()
         && !self.scene3d_active
         && self.id_mask_draws.is_empty()
         && snapshot.instance_count() != 0;
      let mut snapshot_commands = 0_u64;
      let mut dynamic_snapshot = false;
      snapshot.visit_instances(|instance| {
         supported = supported
            && instance.layer.is_none()
            && self.prepared_chunk_supported(&instance.chunk);
         dynamic_snapshot = dynamic_snapshot
            || instance.origin != [0.0, 0.0]
            || !instance.property_slots.is_empty()
            || !instance.dynamic_clips.is_empty()
            || instance.clip.is_some();
         snapshot_commands = snapshot_commands.saturating_add(
            instance.chunk.draw_list().items.len() as u64,
         );
      });
      if !supported
      {
         return self.encode_snapshot_flat(snapshot);
      }
      let bundles_enabled = !dynamic_snapshot
         && snapshot_commands >= PREPARED_BUNDLE_SCENE_MIN_DRAWS;
      let property_slot = self.frame_id as usize % PREPARED_PROPERTY_RING_DEPTH;
      self.prepared_property_ring.begin_frame();
      if dynamic_snapshot && self.prepared_property_ring.ensure_capacity(
         &self.device,
         &self.programs,
         snapshot.instance_count().min(usize::MAX as u64) as usize,
      )
      {
         self.stats.buffer_grows = self.stats.buffer_grows.saturating_add(1);
         self.stats.bind_group_creates = self.stats.bind_group_creates.saturating_add(1);
      }
      let viewport = [
         logical_dimension(self.width, self.scale).max(1.0),
         logical_dimension(self.height, self.scale).max(1.0),
      ];

      let mut cache = core::mem::take(&mut self.prepared_chunks);
      let mut hits = 0_u64;
      let mut misses = 0_u64;
      let mut commands_traversed = 0_u64;
      let mut upload_bytes = 0_u64;
      let mut buffer_creates = 0_u32;
      let mut bundle_creates = 0_u32;
      snapshot.visit_instances(|instance| {
         if !supported
         {
            return;
         }
         let key = PreparedChunkKey::new(
            &instance.chunk,
            self.prepared_device_generation,
            self.config.format,
            bundles_enabled,
         );
         let plan_index = self.prepared_frame_plan.len();
         let (property_offset, clip) = if dynamic_snapshot
         {
            let Some(uniform) = prepared_dynamic_uniform(
               snapshot,
               &instance.property_slots,
               instance.origin,
               viewport,
               self.animation_phase,
            ) else
            {
               supported = false;
               return;
            };
            let mut clip = instance.clip.map(|rect| prepared_transform_rect(
               api::RectF::new(rect.x as f32, rect.y as f32, rect.w as f32, rect.h as f32),
               uniform,
            ));
            for dynamic_clip in instance.dynamic_clips.iter().copied()
            {
               let Some(clip_uniform) = prepared_dynamic_uniform(
                  snapshot,
                  core::slice::from_ref(&dynamic_clip.transform),
                  [0.0, 0.0],
                  viewport,
                  self.animation_phase,
               ) else
               {
                  supported = false;
                  return;
               };
               let transformed = prepared_transform_rect(dynamic_clip.rect, clip_uniform);
               clip = Some(clip.map_or(transformed, |current| prepared_intersect_clip(current, transformed)));
            }
            let Some(property_offset) = self.prepared_property_ring.resolve(plan_index, property_slot, uniform) else
            {
               supported = false;
               return;
            };
            (Some(property_offset), clip)
         }
         else
         {
            (None, None)
         };
         let plan = PreparedFrameInstance { key, property_offset, clip };
         if cache.touch(key)
         {
            hits = hits.saturating_add(1);
            self.prepared_frame_plan.push(plan);
            return;
         }
         let revision_rebuilds = cache.revision_rebuild_streak(key);
         let reusable = cache.remove(key.id).map(|entry| entry.chunk);
         let command_count = instance.chunk.draw_list().items.len() as u64;
         let Some((prepared, uploaded, buffers, bundles)) = self.prepare_chunk(
            &instance.chunk,
            bundles_enabled && revision_rebuilds < 2,
            reusable,
         )
         else
         {
            supported = false;
            return;
         };
         misses = misses.saturating_add(1);
         commands_traversed = commands_traversed.saturating_add(command_count);
         upload_bytes = upload_bytes.saturating_add(uploaded);
         buffer_creates = buffer_creates.saturating_add(buffers);
         bundle_creates = bundle_creates.saturating_add(bundles);
         cache.insert(key, prepared, revision_rebuilds);
         self.prepared_frame_plan.push(plan);
      });
      if supported
      {
         cache.enforce_budget(&self.prepared_frame_plan);
         supported = cache.resident_bytes <= cache.budget_bytes
            && self.prepared_frame_plan.iter().all(|instance| cache.get(instance.key).is_some());
      }
      if !supported
      {
         cache.enforce_budget(&[]);
         self.prepared_snapshot_bundle = None;
         self.prepared_chunks = cache;
         self.prepared_frame_plan.clear();
         return self.encode_snapshot_flat(snapshot);
      }
      let (_property_buffer_upload_bytes, property_upload_bytes, property_records_updated) = if dynamic_snapshot
      {
         self.prepared_property_ring.truncate(self.prepared_frame_plan.len());
         self.prepared_property_ring.upload(&self.queue, property_slot)
      }
      else
      {
         (0, 0, 0)
      };

      self.stats.backend_cache_hits = self.stats.backend_cache_hits.saturating_add(hits);
      self.stats.backend_cache_misses = self.stats.backend_cache_misses.saturating_add(misses);
      self.stats.chunks_reused = self.stats.chunks_reused.saturating_add(hits);
      self.stats.chunks_rebuilt = self.stats.chunks_rebuilt.saturating_add(misses);
      self.stats.chunks_prepared = self.stats.chunks_prepared.saturating_add(misses);
      self.stats.commands_traversed = self.stats.commands_traversed.saturating_add(commands_traversed);
      self.stats.geometry_bytes_copied = self.stats.geometry_bytes_copied.saturating_add(upload_bytes);
      self.stats.buffer_upload_bytes = self.stats.buffer_upload_bytes.saturating_add(upload_bytes);
      self.stats.property_upload_bytes = self.stats.property_upload_bytes.saturating_add(property_upload_bytes);
      self.stats.property_records_updated = self.stats.property_records_updated.saturating_add(property_records_updated);
      self.stats.property_ring_bytes = self.prepared_property_ring.byte_size();
      self.stats.buffer_grows = self.stats.buffer_grows.saturating_add(buffer_creates);
      self.stats.draw_buffer_grows = self.stats.draw_buffer_grows.saturating_add(buffer_creates);
      self.stats.render_bundle_creates = self.stats.render_bundle_creates.saturating_add(bundle_creates);
      let snapshot_bundle_eligible = bundles_enabled
         && self.prepared_frame_plan.iter().all(|instance| cache.get(instance.key).is_some_and(|chunk| {
            chunk.draws.len() >= self.prepared_bundle_min_draws
               && chunk.draws.iter().all(|draw| self.prepared_draw_bundle_compatible(*draw))
         }));
      if snapshot_bundle_eligible
      {
         let had_snapshot_bundle = self.prepared_snapshot_bundle.is_some();
         let bundle_matches = self.prepared_snapshot_bundle.as_ref().is_some_and(|bundle| {
            bundle.keys.len() == self.prepared_frame_plan.len()
               && bundle.keys.iter().zip(&self.prepared_frame_plan).all(|(bundle_key, instance)| {
                  cache.get(instance.key).is_some_and(|chunk| {
                     bundle_key.id == instance.key.id && bundle_key.generation == chunk.bundle_generation
                  })
               })
         });
         if bundle_matches
         {
            self.prepared_snapshot_bundle_active = true;
         }
         else if !had_snapshot_bundle
         {
            self.prepared_snapshot_bundle = self.create_prepared_snapshot_bundle(
               &cache,
               &self.prepared_frame_plan,
            );
            if self.prepared_snapshot_bundle.is_some()
            {
               self.stats.render_bundle_creates = self.stats.render_bundle_creates.saturating_add(1);
            }
            self.prepared_snapshot_bundle_active = self.prepared_snapshot_bundle.is_some();
         }
         else
         {
            self.prepared_snapshot_bundle = None;
         }
      }
      else
      {
         self.prepared_snapshot_bundle = None;
      }
      self.prepared_chunks = cache;
      self.prepared_frame_active = true;
      Ok(())
   }

   fn encode_snapshot_flat(&mut self, snapshot: &api::RenderSnapshot) -> Result<(), api::RenderSnapshotError>
   {
      let mut fallback = core::mem::take(&mut self.prepared_fallback);
      fallback.items.clear();
      fallback.vertices.clear();
      fallback.indices.clear();
      let stats = snapshot.flatten_into(&mut fallback)?;
      <Self as api::Renderer>::encode_pass(self, &fallback);
      self.stats.commands_copied = self.stats.commands_copied.saturating_add(stats.commands_copied);
      self.stats.geometry_bytes_copied = self.stats.geometry_bytes_copied
         .saturating_add(stats.vertex_bytes_copied)
         .saturating_add(stats.index_bytes_copied);
      self.prepared_fallback = fallback;
      Ok(())
   }

   fn prepare_chunk(&mut self, chunk: &api::RenderChunk, bundles_enabled: bool, reusable: Option<PreparedChunk>) -> Option<(PreparedChunk, u64, u32, u32)>
   {
      if !self.prepared_chunk_supported(chunk)
      {
         return None;
      }

      let (reusable_vertex, reusable_rrect, reusable_image, reusable_nine_slice, reusable_spinner, reusable_u16, reusable_u32, reusable_draws, reusable_generation) = reusable.map_or(
         (None, None, None, None, None, None, None, None, 0),
         |prepared| {
            let PreparedChunk {
               vertex_buffer,
               rrect_instance_buffer,
               image_instance_buffer,
               nine_slice_instance_buffer,
               spinner_instance_buffer,
               index_buffer_u16,
               index_buffer_u32,
               draws,
               bundle_generation,
               ..
            } = prepared;
            (
               vertex_buffer,
               rrect_instance_buffer,
               image_instance_buffer,
               nine_slice_instance_buffer,
               spinner_instance_buffer,
               index_buffer_u16,
               index_buffer_u32,
               Some(draws),
               bundle_generation,
            )
         },
      );

      let saved_stats = self.stats;
      let saved_frame = core::mem::take(&mut self.frame);
      let saved_clip_stack = core::mem::take(&mut self.clip_stack);
      let saved_target_stack = core::mem::take(&mut self.target_stack);
      let mut index = 0;
      self.encode_items(chunk.draw_list(), &mut index, false);
      let mut lowered = core::mem::take(&mut self.frame);
      self.frame = saved_frame;
      self.clip_stack = saved_clip_stack;
      self.target_stack = saved_target_stack;
      self.stats = saved_stats;
      if !lowered.layer_passes.is_empty() || lowered.effect_count != 0 || lowered.draws.is_empty()
      {
         return None;
      }
      lowered.geometry.align_uploads();
      let vertex_bytes = bytemuck::cast_slice(&lowered.geometry.vertices);
      let rrect_instance_bytes = bytemuck::cast_slice(&lowered.rrect_instances);
      let image_instance_bytes = bytemuck::cast_slice(&lowered.image_instances);
      let nine_slice_instance_bytes = bytemuck::cast_slice(&lowered.nine_slice_instances);
      let spinner_instance_bytes = bytemuck::cast_slice(&lowered.spinner_instances);
      let index_bytes_u16 = bytemuck::cast_slice(&lowered.geometry.indices_u16);
      let index_bytes_u32 = bytemuck::cast_slice(&lowered.geometry.indices_u32);
      let (vertex_buffer, vertex_created) = if vertex_bytes.is_empty()
      {
         (None, false)
      }
      else
      {
         let (buffer, created) = create_or_update_prepared_buffer(
            &self.device,
            &self.queue,
            "oxide-webgpu-prepared-vertices",
            vertex_bytes,
            wgpu::BufferUsages::VERTEX,
            reusable_vertex,
         );
         (Some(buffer), created)
      };
      let (rrect_instance_buffer, rrect_created) = if rrect_instance_bytes.is_empty()
      {
         (None, false)
      }
      else
      {
         let (buffer, created) = create_or_update_prepared_buffer(
            &self.device,
            &self.queue,
            "oxide-webgpu-prepared-rrect-instances",
            rrect_instance_bytes,
            wgpu::BufferUsages::VERTEX,
            reusable_rrect,
         );
         (Some(buffer), created)
      };
      let (image_instance_buffer, image_created) = if image_instance_bytes.is_empty()
      {
         (None, false)
      }
      else
      {
         let (buffer, created) = create_or_update_prepared_buffer(
            &self.device,
            &self.queue,
            "oxide-webgpu-prepared-image-instances",
            image_instance_bytes,
            wgpu::BufferUsages::VERTEX,
            reusable_image,
         );
         (Some(buffer), created)
      };
      let (nine_slice_instance_buffer, nine_slice_created) = if nine_slice_instance_bytes.is_empty()
      {
         (None, false)
      }
      else
      {
         let (buffer, created) = create_or_update_prepared_buffer(
            &self.device,
            &self.queue,
            "oxide-webgpu-prepared-nine-slice-instances",
            nine_slice_instance_bytes,
            wgpu::BufferUsages::VERTEX,
            reusable_nine_slice,
         );
         (Some(buffer), created)
      };
      let (spinner_instance_buffer, spinner_created) = if spinner_instance_bytes.is_empty()
      {
         (None, false)
      }
      else
      {
         let (buffer, created) = create_or_update_prepared_buffer(
            &self.device,
            &self.queue,
            "oxide-webgpu-prepared-spinner-instances",
            spinner_instance_bytes,
            wgpu::BufferUsages::VERTEX,
            reusable_spinner,
         );
         (Some(buffer), created)
      };
      let (index_buffer_u16, index_u16_created) = if index_bytes_u16.is_empty()
      {
         (None, false)
      }
      else
      {
         let (buffer, created) = create_or_update_prepared_buffer(
            &self.device,
            &self.queue,
            "oxide-webgpu-prepared-indices-u16",
            index_bytes_u16,
            wgpu::BufferUsages::INDEX,
            reusable_u16,
         );
         (Some(buffer), created)
      };
      let (index_buffer_u32, index_u32_created) = if index_bytes_u32.is_empty()
      {
         (None, false)
      }
      else
      {
         let (buffer, created) = create_or_update_prepared_buffer(
            &self.device,
            &self.queue,
            "oxide-webgpu-prepared-indices-u32",
            index_bytes_u32,
            wgpu::BufferUsages::INDEX,
            reusable_u32,
         );
         (Some(buffer), created)
      };
      let segments = self.create_prepared_segments(
         vertex_buffer.as_ref(),
         rrect_instance_buffer.as_ref(),
         image_instance_buffer.as_ref(),
         nine_slice_instance_buffer.as_ref(),
         spinner_instance_buffer.as_ref(),
         index_buffer_u16.as_ref(),
         index_buffer_u32.as_ref(),
         &lowered.draws,
         bundles_enabled,
      );
      let bundle_count = segments.iter().filter(|segment| {
         matches!(segment, PreparedSegment::Bundle { .. })
      }).count().min(u32::MAX as usize) as u32;
      let vertex_bytes = vertex_buffer.as_ref().map_or(0, wgpu::Buffer::size);
      let vertex_bytes = vertex_bytes.saturating_add(
         rrect_instance_buffer.as_ref().map_or(0, wgpu::Buffer::size),
      );
      let vertex_bytes = vertex_bytes.saturating_add(
         image_instance_buffer.as_ref().map_or(0, wgpu::Buffer::size),
      );
      let vertex_bytes = vertex_bytes.saturating_add(
         nine_slice_instance_buffer.as_ref().map_or(0, wgpu::Buffer::size),
      );
      let vertex_bytes = vertex_bytes.saturating_add(
         spinner_instance_buffer.as_ref().map_or(0, wgpu::Buffer::size),
      );
      let index_bytes = index_buffer_u16.as_ref().map_or(0, wgpu::Buffer::size)
         .saturating_add(index_buffer_u32.as_ref().map_or(0, wgpu::Buffer::size));
      let plan_bytes = (lowered.draws.len() as u64)
         .saturating_mul(core::mem::size_of::<GpuDraw>() as u64)
         .saturating_add(
            (segments.len() as u64).saturating_mul(core::mem::size_of::<PreparedSegment>() as u64),
         )
         .saturating_add(
            (chunk.resource_dependencies().len() as u64)
               .saturating_mul(core::mem::size_of::<api::ImageHandle>() as u64),
         );
      let resident_bytes = vertex_bytes.saturating_add(index_bytes).saturating_add(plan_bytes);
      let upload_bytes = lowered.geometry.byte_len() as u64;
      let upload_bytes = upload_bytes.saturating_add(rrect_instance_bytes.len() as u64);
      let upload_bytes = upload_bytes.saturating_add(image_instance_bytes.len() as u64);
      let upload_bytes = upload_bytes.saturating_add(nine_slice_instance_bytes.len() as u64);
      let upload_bytes = upload_bytes.saturating_add(spinner_instance_bytes.len() as u64);
      let buffer_count = u32::from(vertex_created)
         .saturating_add(u32::from(rrect_created))
         .saturating_add(u32::from(image_created))
         .saturating_add(u32::from(nine_slice_created))
         .saturating_add(u32::from(spinner_created))
         .saturating_add(u32::from(index_u16_created))
         .saturating_add(u32::from(index_u32_created));
      let bundle_generation = if buffer_count == 0
         && reusable_draws.as_deref() == Some(lowered.draws.as_slice())
      {
         reusable_generation
      }
      else
      {
         self.prepared_bundle_generation = self.prepared_bundle_generation.wrapping_add(1).max(1);
         self.prepared_bundle_generation
      };
      let resources = chunk.resource_dependencies().iter()
         .map(|dependency| dependency.image)
         .collect::<Vec<_>>().into_boxed_slice();
      let rrect_instances = lowered.rrect_instances.len().min(u32::MAX as usize) as u32;
      let image_instances = lowered.image_instances.len().min(u32::MAX as usize) as u32;
      let nine_slice_instances = lowered.nine_slice_instances.len().min(u32::MAX as usize) as u32;
      let spinner_instances = lowered.spinner_instances.len().min(u32::MAX as usize) as u32;
      Some((PreparedChunk {
         vertex_buffer,
         rrect_instance_buffer,
         rrect_instances,
         image_instance_buffer,
         image_instances,
         nine_slice_instance_buffer,
         nine_slice_instances,
         spinner_instance_buffer,
         spinner_instances,
         index_buffer_u16,
         index_buffer_u32,
         draws: lowered.draws,
         segments,
         resources,
         vertex_bytes,
         index_bytes,
         resident_bytes,
         bundle_generation,
      }, upload_bytes, buffer_count, bundle_count))
   }

   fn prepared_chunk_supported(&self, chunk: &api::RenderChunk) -> bool
   {
      !chunk.ordering().has_layer && !chunk.draw_list().items.iter().any(|item| {
         matches!(item,
            api::DrawCmd::LayerBegin { .. }
            | api::DrawCmd::LayerEnd
            | api::DrawCmd::Backdrop { .. }
            | api::DrawCmd::VisualEffect { .. }
            | api::DrawCmd::CameraBg { .. })
      }) && !chunk.resource_dependencies().iter().any(|dependency| {
         self.image(dependency.image).is_none()
      })
   }

   fn create_prepared_segments(
      &self,
      vertex_buffer: Option<&wgpu::Buffer>,
      rrect_instance_buffer: Option<&wgpu::Buffer>,
      image_instance_buffer: Option<&wgpu::Buffer>,
      nine_slice_instance_buffer: Option<&wgpu::Buffer>,
      spinner_instance_buffer: Option<&wgpu::Buffer>,
      index_buffer_u16: Option<&wgpu::Buffer>,
      index_buffer_u32: Option<&wgpu::Buffer>,
      draws: &[GpuDraw],
      bundles_enabled: bool,
   ) -> Vec<PreparedSegment>
   {
      if !bundles_enabled
      {
         return vec![PreparedSegment::Direct { start: 0, end: draws.len() }];
      }
      let mut segments = Vec::new();
      let mut start = 0;
      while start < draws.len()
      {
         let compatible = self.prepared_draw_bundle_compatible(draws[start]);
         let mut end = start + 1;
         while end < draws.len()
            && self.prepared_draw_bundle_compatible(draws[end]) == compatible
         {
            end += 1;
         }
         let bundle = compatible
            .then(|| self.create_prepared_bundle(
               vertex_buffer,
               rrect_instance_buffer,
               image_instance_buffer,
               nine_slice_instance_buffer,
               spinner_instance_buffer,
               index_buffer_u16,
               index_buffer_u32,
               &draws[start..end],
            ))
            .flatten();
         if let Some(bundle) = bundle
         {
            segments.push(PreparedSegment::Bundle {
               bundle,
               draws: (end - start).min(u32::MAX as usize) as u32,
            });
         }
         else
         {
            segments.push(PreparedSegment::Direct { start, end });
         }
         start = end;
      }
      segments
   }

   fn create_prepared_bundle(
      &self,
      vertex_buffer: Option<&wgpu::Buffer>,
      rrect_instance_buffer: Option<&wgpu::Buffer>,
      image_instance_buffer: Option<&wgpu::Buffer>,
      nine_slice_instance_buffer: Option<&wgpu::Buffer>,
      spinner_instance_buffer: Option<&wgpu::Buffer>,
      index_buffer_u16: Option<&wgpu::Buffer>,
      index_buffer_u32: Option<&wgpu::Buffer>,
      draws: &[GpuDraw],
   ) -> Option<wgpu::RenderBundle>
   {
      if draws.len() < self.prepared_bundle_min_draws
      {
         return None;
      }
      let formats = [Some(self.config.format)];
      let mut encoder = self.device.create_render_bundle_encoder(&wgpu::RenderBundleEncoderDescriptor {
         label: Some("oxide-webgpu-prepared-bundle"),
         color_formats: &formats,
         depth_stencil: None,
         sample_count: 1,
         multiview: None,
      });
      encoder.set_bind_group(0, &self.viewport_bind_group, &[0]);
      let mut bound_pipeline = None;
      let mut bound_bind = None;
      let mut bound_index = None;
      for draw in draws
      {
         if !matches!(draw.kind, DrawKind::RRect { .. } | DrawKind::Image { .. } | DrawKind::NineSlice { .. } | DrawKind::Spinner { .. })
            && bound_index != Some(draw.index_kind)
         {
            match draw.index_kind
            {
               PackedIndexKind::U16 => encoder.set_index_buffer(
                  index_buffer_u16?.slice(..),
                  wgpu::IndexFormat::Uint16,
               ),
               PackedIndexKind::U32 => encoder.set_index_buffer(
                  index_buffer_u32?.slice(..),
                  wgpu::IndexFormat::Uint32,
               ),
            }
            bound_index = Some(draw.index_kind);
         }
         let state = self.draw_state_key(*draw)?;
         if bound_pipeline != Some(state.pipeline)
         {
            encoder.set_pipeline(self.pipeline_for_draw(state.pipeline)?);
            if state.pipeline == DrawPipelineKey::RRect
            {
               encoder.set_vertex_buffer(0, rrect_instance_buffer?.slice(..));
            }
            else if matches!(state.pipeline, DrawPipelineKey::ImageRgba | DrawPipelineKey::ImageA8)
            {
               encoder.set_vertex_buffer(0, self.programs.image_unit_vertex_buffer.slice(..));
               encoder.set_vertex_buffer(1, image_instance_buffer?.slice(..));
               encoder.set_index_buffer(
                  self.programs.image_unit_index_buffer.slice(..),
                  wgpu::IndexFormat::Uint16,
               );
               bound_index = None;
            }
            else if matches!(state.pipeline, DrawPipelineKey::NineSliceRgba | DrawPipelineKey::NineSliceA8)
            {
               encoder.set_vertex_buffer(0, self.programs.nine_slice_unit_vertex_buffer.slice(..));
               encoder.set_vertex_buffer(1, nine_slice_instance_buffer?.slice(..));
               encoder.set_index_buffer(
                  self.programs.nine_slice_unit_index_buffer.slice(..),
                  wgpu::IndexFormat::Uint16,
               );
               bound_index = None;
            }
            else if state.pipeline == DrawPipelineKey::Spinner
            {
               encoder.set_vertex_buffer(0, spinner_instance_buffer?.slice(..));
               bound_index = None;
            }
            else
            {
               encoder.set_vertex_buffer(0, vertex_buffer?.slice(..));
            }
            bound_pipeline = Some(state.pipeline);
         }
         if bound_bind != Some(state.bind)
         {
            if let DrawBindKey::Texture { image } = state.bind
            {
               encoder.set_bind_group(1, &self.image(api::ImageHandle(image))?.bind_group, &[]);
            }
            bound_bind = Some(state.bind);
         }
         if let DrawKind::RRect { first_instance, instance_count } = draw.kind
         {
            encoder.draw(0..6, first_instance..first_instance.saturating_add(instance_count));
         }
         else if let DrawKind::Image { first_instance, instance_count, .. } = draw.kind
         {
            encoder.draw_indexed(
               0..6,
               0,
               first_instance..first_instance.saturating_add(instance_count),
            );
         }
         else if let DrawKind::NineSlice { first_instance, instance_count, .. } = draw.kind
         {
            encoder.draw_indexed(
               0..NINE_SLICE_INDEX_COUNT,
               0,
               first_instance..first_instance.saturating_add(instance_count),
            );
         }
         else if let DrawKind::Spinner { first_instance, instance_count } = draw.kind
         {
            encoder.draw(
               0..SPINNER_VERTEX_COUNT,
               first_instance..first_instance.saturating_add(instance_count),
            );
         }
         else
         {
            encoder.draw_indexed(
               draw.first_index..draw.first_index.saturating_add(draw.index_count),
               draw.base_vertex,
               0..1,
            );
         }
      }
      Some(encoder.finish(&wgpu::RenderBundleDescriptor {
         label: Some("oxide-webgpu-prepared-bundle"),
      }))
   }

   fn create_prepared_snapshot_bundle(
      &self,
      cache: &PreparedChunkCache,
      plan: &[PreparedFrameInstance],
   ) -> Option<PreparedSnapshotBundle>
   {
      let formats = [Some(self.config.format)];
      let mut encoder = self.device.create_render_bundle_encoder(&wgpu::RenderBundleEncoderDescriptor {
         label: Some("oxide-webgpu-prepared-snapshot-bundle"),
         color_formats: &formats,
         depth_stencil: None,
         sample_count: 1,
         multiview: None,
      });
      encoder.set_bind_group(0, &self.viewport_bind_group, &[0]);
      let mut bound_pipeline = None;
      let mut bound_bind = None;
      let mut draws = 0_u32;
      let mut rrect_instances = 0_u32;
      let mut image_instances = 0_u32;
      let mut nine_slice_instances = 0_u32;
      let mut spinner_instances = 0_u32;
      for instance in plan
      {
         let chunk = cache.get(instance.key)?;
         rrect_instances = rrect_instances.saturating_add(chunk.rrect_instances);
         image_instances = image_instances.saturating_add(chunk.image_instances);
         nine_slice_instances = nine_slice_instances.saturating_add(chunk.nine_slice_instances);
         spinner_instances = spinner_instances.saturating_add(chunk.spinner_instances);
         let mut bound_index = None;
         for draw in &chunk.draws
         {
            if !self.prepared_draw_bundle_compatible(*draw)
            {
               return None;
            }
            if !matches!(draw.kind, DrawKind::RRect { .. } | DrawKind::Image { .. } | DrawKind::NineSlice { .. } | DrawKind::Spinner { .. })
               && bound_index != Some(draw.index_kind)
            {
               match draw.index_kind
               {
                  PackedIndexKind::U16 => encoder.set_index_buffer(
                     chunk.index_buffer_u16.as_ref()?.slice(..),
                     wgpu::IndexFormat::Uint16,
                  ),
                  PackedIndexKind::U32 => encoder.set_index_buffer(
                     chunk.index_buffer_u32.as_ref()?.slice(..),
                     wgpu::IndexFormat::Uint32,
                  ),
               }
               bound_index = Some(draw.index_kind);
            }
            let state = self.draw_state_key(*draw)?;
            if bound_pipeline != Some(state.pipeline)
            {
               encoder.set_pipeline(self.pipeline_for_draw(state.pipeline)?);
               bound_pipeline = Some(state.pipeline);
            }
            if state.pipeline == DrawPipelineKey::RRect
            {
               encoder.set_vertex_buffer(0, chunk.rrect_instance_buffer.as_ref()?.slice(..));
            }
            else if matches!(state.pipeline, DrawPipelineKey::ImageRgba | DrawPipelineKey::ImageA8)
            {
               encoder.set_vertex_buffer(0, self.programs.image_unit_vertex_buffer.slice(..));
               encoder.set_vertex_buffer(1, chunk.image_instance_buffer.as_ref()?.slice(..));
               encoder.set_index_buffer(
                  self.programs.image_unit_index_buffer.slice(..),
                  wgpu::IndexFormat::Uint16,
               );
               bound_index = None;
            }
            else if matches!(state.pipeline, DrawPipelineKey::NineSliceRgba | DrawPipelineKey::NineSliceA8)
            {
               encoder.set_vertex_buffer(0, self.programs.nine_slice_unit_vertex_buffer.slice(..));
               encoder.set_vertex_buffer(1, chunk.nine_slice_instance_buffer.as_ref()?.slice(..));
               encoder.set_index_buffer(
                  self.programs.nine_slice_unit_index_buffer.slice(..),
                  wgpu::IndexFormat::Uint16,
               );
               bound_index = None;
            }
            else if state.pipeline == DrawPipelineKey::Spinner
            {
               encoder.set_vertex_buffer(0, chunk.spinner_instance_buffer.as_ref()?.slice(..));
               bound_index = None;
            }
            else
            {
               encoder.set_vertex_buffer(0, chunk.vertex_buffer.as_ref()?.slice(..));
            }
            if bound_bind != Some(state.bind)
            {
               if let DrawBindKey::Texture { image } = state.bind
               {
                  encoder.set_bind_group(1, &self.image(api::ImageHandle(image))?.bind_group, &[]);
               }
               bound_bind = Some(state.bind);
            }
            if let DrawKind::RRect { first_instance, instance_count } = draw.kind
            {
               encoder.draw(0..6, first_instance..first_instance.saturating_add(instance_count));
            }
            else if let DrawKind::Image { first_instance, instance_count, .. } = draw.kind
            {
               encoder.draw_indexed(
                  0..6,
                  0,
                  first_instance..first_instance.saturating_add(instance_count),
               );
            }
            else if let DrawKind::NineSlice { first_instance, instance_count, .. } = draw.kind
            {
               encoder.draw_indexed(
                  0..NINE_SLICE_INDEX_COUNT,
                  0,
                  first_instance..first_instance.saturating_add(instance_count),
               );
            }
            else if let DrawKind::Spinner { first_instance, instance_count } = draw.kind
            {
               encoder.draw(
                  0..SPINNER_VERTEX_COUNT,
                  first_instance..first_instance.saturating_add(instance_count),
               );
            }
            else
            {
               encoder.draw_indexed(
                  draw.first_index..draw.first_index.saturating_add(draw.index_count),
                  draw.base_vertex,
                  0..1,
               );
            }
            draws = draws.saturating_add(1);
         }
      }
      Some(PreparedSnapshotBundle {
         keys: plan.iter().map(|instance| {
            let chunk = cache.get(instance.key)?;
            Some(PreparedSnapshotBundleKey {
               id: instance.key.id,
               generation: chunk.bundle_generation,
            })
         }).collect::<Option<Vec<_>>>()?.into_boxed_slice(),
         bundle: encoder.finish(&wgpu::RenderBundleDescriptor {
            label: Some("oxide-webgpu-prepared-snapshot-bundle"),
         }),
         draws,
         rrect_instances,
         image_instances,
         nine_slice_instances,
         spinner_instances,
      })
   }

   fn prepared_draw_bundle_compatible(&self, draw: GpuDraw) -> bool
   {
      let full_clip = api::RectI::new(
         0,
         0,
         logical_dimension(self.width, self.scale) as i32,
         logical_dimension(self.height, self.scale) as i32,
      );
      draw.target.is_none()
         && draw.clip == full_clip
         && !matches!(draw.kind, DrawKind::Layer { .. } | DrawKind::Backdrop { .. })
   }

   fn pipeline_for_draw(&self, pipeline: DrawPipelineKey) -> Option<&wgpu::RenderPipeline>
   {
      match pipeline
      {
         DrawPipelineKey::Solid => Some(self.solid_pipeline()),
         DrawPipelineKey::RRect => Some(self.rrect_pipeline()),
         DrawPipelineKey::ImageRgba => Some(self.image_rgba_pipeline()),
         DrawPipelineKey::ImageA8 => Some(self.image_a8_pipeline()),
         DrawPipelineKey::NineSliceRgba => Some(self.nine_slice_rgba_pipeline()),
         DrawPipelineKey::NineSliceA8 => Some(self.nine_slice_a8_pipeline()),
         DrawPipelineKey::Spinner => Some(self.spinner_pipeline()),
         DrawPipelineKey::NeonMarker => Some(self.neon_marker_pipeline()),
         DrawPipelineKey::Rgba => Some(self.rgba_pipeline()),
         DrawPipelineKey::A8 => Some(self.a8_pipeline()),
         DrawPipelineKey::Sdf => Some(self.sdf_pipeline()),
         DrawPipelineKey::Effect => None,
      }
   }
}

impl api::Renderer for WebGpuRenderer {
    fn device_caps(&self) -> api::DeviceCaps {
        api::DeviceCaps {
            max_framerate_hz: 120,
            supports_edr: false,
            supports_msaa4x: false,
            native_scale: self.scale,
        }
    }

    fn begin_frame(
        &mut self,
        _fb: &api::FrameTarget,
        damage: Option<&api::Damage>,
    ) -> api::FrameToken {
        self.frame_id = self.frame_id.wrapping_add(1);
        self.frame.clear();
        self.prepared_frame_plan.clear();
        self.prepared_frame_active = false;
        self.prepared_snapshot_bundle_active = false;
        self.scene3d_uniform_bytes.clear();
        self.scene3d_draws.clear();
        self.scene3d_overlay_draws.clear();
        self.id_mask_draws.clear();
        self.id_mask_draw_chunk_indices.clear();
        self.id_mask_draw_chunk_keys.clear();
        self.id_mask_resolved_draws.clear();
        self.scene3d_clear_color = None;
        self.scene3d_clear_depth = true;
        self.scene3d_active = false;
        self.clip_stack.clear();
        self.target_stack.clear();
        self.layer_frame_ids.clear();
        if let Some(timestamps) = &mut self.timestamp_queries {
            timestamps.harvest();
            timestamps.begin_frame();
        }
        self.stats = WebRendererStats {
            frame_id: self.frame_id,
            width: self.width,
            height: self.height,
            scale: self.scale,
            damage_rects: damage.map(|d| d.rects.len() as u32).unwrap_or(0),
            ..WebRendererStats::default()
        };
        self.apply_layer_cache_stats();
        self.apply_id_mask_cache_stats();
        self.apply_memory_stats();
        self.apply_timestamp_stats();
        self.frame_scratch_capacity = self.scratch_capacity_breakdown();
        self.frame_scratch_capacity_bytes = self.frame_scratch_capacity.total();
        self.apply_scratch_capacity_stats(self.frame_scratch_capacity);
        let token = api::FrameToken(self.frame_id);
        self.active_token = Some(token);
        token
    }

    fn encode_pass(&mut self, list: &api::DrawList) {
        let mut index = 0;
        self.encode_items(list, &mut index, false);
    }

    fn submit(&mut self, token: api::FrameToken) -> Result<(), api::RenderError> {
        if self.active_token != Some(token) {
            self.stats.skipped_submissions = self.stats.skipped_submissions.saturating_add(1);
            return Err(api::RenderError::InvalidOperation("frame token mismatch"));
        }
        self.active_token = None;
        if self.cpu_submit_timing_enabled {
            self.cpu_submit_timing = WebGpuCpuSubmitTimingSample::default();
        }
        let timing_before = cpu_submit_timing_begin(self.cpu_submit_timing_enabled);
        let alloc_before = oxide_wasm_alloc_counter::snapshot();
        self.upload_frame_buffers();
        self.upload_scene3d_uniforms();
        self.prepare_effect_uniforms();
        self.record_submit_allocation_stage(SubmitAllocationStage::Upload, alloc_before);
        cpu_submit_timing_end(&mut self.cpu_submit_timing.upload_ms, timing_before);

        let timing_before = cpu_submit_timing_begin(self.cpu_submit_timing_enabled);
        let alloc_before = oxide_wasm_alloc_counter::snapshot();
        let surface_texture = match self.surface.get_current_texture() {
            Ok(texture) => texture,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                match self.surface.get_current_texture() {
                    Ok(texture) => texture,
                    Err(_) => {
                        self.purge_layer_cache_for_reason(LAYER_PURGE_DEVICE_LOSS);
                        self.purge_id_mask_field_cache_for_reason(LAYER_PURGE_DEVICE_LOSS);
                        return Err(api::RenderError::DeviceLost);
                    }
                }
            }
            Err(wgpu::SurfaceError::OutOfMemory) => {
                self.stats.skipped_submissions = self.stats.skipped_submissions.saturating_add(1);
                self.purge_layer_cache_for_reason(LAYER_PURGE_MEMORY_PRESSURE);
                self.purge_id_mask_field_cache_for_reason(LAYER_PURGE_MEMORY_PRESSURE);
                return Err(api::RenderError::OutOfMemory);
            }
            Err(_) => {
                self.stats.skipped_submissions = self.stats.skipped_submissions.saturating_add(1);
                self.purge_layer_cache_for_reason(LAYER_PURGE_DEVICE_LOSS);
                self.purge_id_mask_field_cache_for_reason(LAYER_PURGE_DEVICE_LOSS);
                return Err(api::RenderError::DeviceLost);
            }
        };
        let surface_view =
            surface_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.record_submit_allocation_stage(SubmitAllocationStage::Surface, alloc_before);
        cpu_submit_timing_end(&mut self.cpu_submit_timing.surface_ms, timing_before);

        let timing_before = cpu_submit_timing_begin(self.cpu_submit_timing_enabled);
        let alloc_before = oxide_wasm_alloc_counter::snapshot();
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("oxide-webgpu-frame"),
        });
        self.stats.command_buffers = self.stats.command_buffers.saturating_add(1);
        self.record_submit_allocation_stage(SubmitAllocationStage::Encoder, alloc_before);
        cpu_submit_timing_end(&mut self.cpu_submit_timing.encoder_create_ms, timing_before);

        let timing_before = cpu_submit_timing_begin(self.cpu_submit_timing_enabled);
        let alloc_before = oxide_wasm_alloc_counter::snapshot();
        self.render_layer_passes(&mut encoder);
        if self.target_uses_backdrop(None, 0, self.frame.draws.len())
            || !self.direct_surface_enabled
        {
            self.render_scene_with_effects(&mut encoder);
            self.render_present(&mut encoder, &surface_view);
        } else {
            self.render_direct(&mut encoder, &surface_view);
        }
        self.record_submit_allocation_stage(SubmitAllocationStage::Render, alloc_before);
        cpu_submit_timing_end(&mut self.cpu_submit_timing.command_encoding_ms, timing_before);

        let timing_before = cpu_submit_timing_begin(self.cpu_submit_timing_enabled);
        let alloc_before = oxide_wasm_alloc_counter::snapshot();
        let timestamp_readback = self.prepare_timestamp_readback(&mut encoder);
        self.record_submit_allocation_stage(SubmitAllocationStage::Timestamp, alloc_before);
        cpu_submit_timing_end(&mut self.cpu_submit_timing.timestamp_readback_ms, timing_before);

        let timing_before = cpu_submit_timing_begin(self.cpu_submit_timing_enabled);
        let alloc_before = oxide_wasm_alloc_counter::snapshot();
        self.record_scratch_growth_stats();
        self.record_submit_allocation_stage(SubmitAllocationStage::ScratchStats, alloc_before);
        cpu_submit_timing_end(&mut self.cpu_submit_timing.scratch_stats_ms, timing_before);

        let timing_before = cpu_submit_timing_begin(self.cpu_submit_timing_enabled);
        let alloc_before = oxide_wasm_alloc_counter::snapshot();
        let command_buffer = encoder.finish();
        self.queue.submit(core::iter::once(command_buffer));
        self.stats.actual_submissions = self.stats.actual_submissions.saturating_add(1);
        self.stats.shaded_damage_pixels = u64::from(self.width).saturating_mul(u64::from(self.height));
        self.record_submit_allocation_stage(SubmitAllocationStage::FinishQueue, alloc_before);
        cpu_submit_timing_end(&mut self.cpu_submit_timing.queue_submit_ms, timing_before);

        let timing_before = cpu_submit_timing_begin(self.cpu_submit_timing_enabled);
        let alloc_before = oxide_wasm_alloc_counter::snapshot();
        surface_texture.present();
        self.record_submit_allocation_stage(SubmitAllocationStage::Present, alloc_before);
        cpu_submit_timing_end(&mut self.cpu_submit_timing.present_ms, timing_before);

        let timing_before = cpu_submit_timing_begin(self.cpu_submit_timing_enabled);
        let alloc_before = oxide_wasm_alloc_counter::snapshot();
        if let Some((slot_index, bytes)) = timestamp_readback {
            self.map_timestamp_readback(slot_index, bytes);
            self.apply_timestamp_stats();
        }
        self.record_submit_allocation_stage(SubmitAllocationStage::TimestampMap, alloc_before);
        cpu_submit_timing_end(&mut self.cpu_submit_timing.timestamp_map_ms, timing_before);
        if self.memory_stats_enabled
            && self.frame_id.saturating_sub(1) % self.memory_stats_interval == 0
        {
            self.sample_memory_stats();
            self.apply_memory_stats();
        }
        self.stats.backend_cache_hits = self
            .stats
            .backend_cache_hits
            .saturating_add(u64::from(self.stats.layer_cache_hits));
        self.stats.backend_cache_misses = self
            .stats
            .backend_cache_misses
            .saturating_add(u64::from(self.stats.layer_cache_misses));
        self.stats.cache_evictions = self.stats.cache_evictions.saturating_add(
            self.prepared_chunks.take_evictions().min(u64::from(u32::MAX)) as u32,
        );
        self.age_layer_cache();
        self.apply_layer_cache_stats();
        self.apply_id_mask_cache_stats();
        self.id_mask_resolved_draws.clear();
        self.stats.render_encoders = self.stats.render_passes;
        Ok(())
    }

    fn resize(&mut self, width: u32, height: u32, scale: f32) -> Result<(), api::RenderError> {
        let width = width.max(1);
        let height = height.max(1);
        let scale = sanitize_scale(scale);
        let size_changed = self.width != width || self.height != height;
        let scale_changed = (self.scale - scale).abs() > f32::EPSILON;
        if !size_changed && !scale_changed {
            return Ok(());
        }
        self.width = width;
        self.height = height;
        self.scale = scale;
        write_viewport_uniform(
            &self.queue,
            &self.viewport_buffer,
            self.width,
            self.height,
            self.scale,
            self.animation_phase,
        );
        if size_changed {
            self.canvas.set_width(self.width);
            self.canvas.set_height(self.height);
            self.config.width = self.width;
            self.config.height = self.height;
            self.surface.configure(&self.device, &self.config);
            self.drop_auxiliary_targets();
        }
        if scale_changed {
            self.purge_layer_cache_for_reason(LAYER_PURGE_SCALE_CHANGE);
            self.purge_prepared_chunks();
        }
        Ok(())
    }
}

impl WebGpuRenderer {
    fn id_mask_vertex_cache_index(
        &mut self,
        content_hash: u64,
        vertices: &[id_mask_compositor::IdMaskRasterVertex],
    ) -> usize {
        let key = IdMaskVertexCacheKey { content_hash, len: vertices.len() };
        if let Some(index) = self.id_mask_vertex_caches.iter().position(|cache| cache.key == key) {
            self.stats.chunks_reused = self.stats.chunks_reused.saturating_add(1);
            self.stats.backend_cache_hits = self.stats.backend_cache_hits.saturating_add(1);
            return index;
        }
        self.stats.backend_cache_misses = self.stats.backend_cache_misses.saturating_add(1);
        if let Some(index) = self.id_mask_reusable_vertex_cache_index() {
            let cache = &mut self.id_mask_vertex_caches[index];
            cache.key = key;
            write_id_mask_raster_vertex_bytes(vertices, &mut cache.bytes);
            cache.uploaded = false;
            self.stats.chunks_rebuilt = self.stats.chunks_rebuilt.saturating_add(1);
            return index;
        }
        let mut bytes = Vec::new();
        write_id_mask_raster_vertex_bytes(vertices, &mut bytes);
        self.id_mask_vertex_caches.push(IdMaskVertexCache {
            key,
            bytes,
            buffer: None,
            buffer_capacity: 0,
            uploaded: false,
        });
        self.stats.chunks_rebuilt = self.stats.chunks_rebuilt.saturating_add(1);
        self.id_mask_vertex_caches.len() - 1
    }

    fn id_mask_reusable_vertex_cache_index(&self) -> Option<usize> {
        'caches: for index in 0..self.id_mask_vertex_caches.len() {
            for entry in &self.id_mask_draw_chunk_indices {
                if *entry == index {
                    continue 'caches;
                }
            }
            return Some(index);
        }
        None
    }

    fn ensure_id_mask_vertex_cache_uploaded(&mut self, index: usize) -> Option<u64> {
        let device = &self.device;
        let queue = &self.queue;
        let stats = &mut self.stats;
        let cache = self.id_mask_vertex_caches.get_mut(index)?;
        ensure_id_mask_vertex_cache_uploaded(device, queue, stats, cache)
    }

    fn target_uses_backdrop(&self, target: Option<u32>, start: usize, end: usize) -> bool {
        let end = end.min(self.frame.draws.len());
        self.frame.draws[start.min(end)..end]
            .iter()
            .any(|draw| draw.target == target && matches!(draw.kind, DrawKind::Backdrop { .. }))
    }

    fn target_copy_region(&self, target: Option<u32>) -> Option<(wgpu::Origin3d, wgpu::Origin3d, wgpu::Extent3d)>
    {
       let Some(id) = target else
       {
          return Some((
             wgpu::Origin3d::ZERO,
             wgpu::Origin3d::ZERO,
             wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
             },
          ));
       };
       let layer = self.layers.get(&id)?;
       let scale = f64::from(sanitize_scale(self.scale));
       let target_x = (f64::from(layer.rect.x) * scale).floor() as i64;
       let target_y = (f64::from(layer.rect.y) * scale).floor() as i64;
       let source_x = (-target_x).max(0).min(i64::from(layer.width)) as u32;
       let source_y = (-target_y).max(0).min(i64::from(layer.height)) as u32;
       let destination_x = target_x.max(0).min(i64::from(self.width)) as u32;
       let destination_y = target_y.max(0).min(i64::from(self.height)) as u32;
       let width = layer.width.saturating_sub(source_x)
          .min(self.width.saturating_sub(destination_x));
       let height = layer.height.saturating_sub(source_y)
          .min(self.height.saturating_sub(destination_y));
       (width != 0 && height != 0).then_some((
          wgpu::Origin3d { x: source_x, y: source_y, z: 0 },
          wgpu::Origin3d { x: destination_x, y: destination_y, z: 0 },
          wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
       ))
    }

    fn backdrop_sample_rect(&self, rect: api::RectF, sigma: f32) -> api::RectF {
        let radius = sigma.clamp(0.0, MAX_BLUR_SIGMA);
        let sample_step = (radius * 0.35).max(1.0) / self.scale.max(1.0);
        api::RectF::new(
            rect.x - sample_step,
            rect.y - sample_step,
            rect.w + sample_step * 2.0,
            rect.h + sample_step * 2.0,
        )
    }

    fn backdrop_sample_rects_overlap(&self, a: api::RectF, b: api::RectF) -> bool {
        let ax1 = a.x + a.w;
        let ay1 = a.y + a.h;
        let bx1 = b.x + b.w;
        let by1 = b.y + b.h;
        a.x < bx1 && ax1 > b.x && a.y < by1 && ay1 > b.y
    }

    fn backdrop_batch_end(&self, start: usize, target: Option<u32>, limit: usize) -> usize {
        if !self.backdrop_batch_enabled {
            return start + 1;
        }
        let first = self.frame.draws[start];
        if first.target != target {
            return start + 1;
        }
        let DrawKind::Backdrop { rect, sigma } = first.kind else {
            return start + 1;
        };
        let mut end = start + 1;
        let first_sample = self.backdrop_sample_rect(rect, sigma);
        let limit = limit.min(self.frame.draws.len());
        while end < limit {
            let draw = self.frame.draws[end];
            if draw.target != target {
                end += 1;
                continue;
            }
            let DrawKind::Backdrop { rect, sigma } = draw.kind else {
                break;
            };
            let candidate = self.backdrop_sample_rect(rect, sigma);
            let mut overlaps = self.backdrop_sample_rects_overlap(first_sample, candidate);
            let mut prior = start + 1;
            while !overlaps && prior < end {
                let prior_draw = self.frame.draws[prior];
                if prior_draw.target != target {
                    prior += 1;
                    continue;
                }
                let DrawKind::Backdrop { rect, sigma } = prior_draw.kind else {
                    break;
                };
                overlaps = self.backdrop_sample_rects_overlap(
                    self.backdrop_sample_rect(rect, sigma),
                    candidate,
                );
                prior += 1;
            }
            if overlaps {
                break;
            }
            end += 1;
        }
        end
    }

    fn solid_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.programs.solid_pipeline
    }

    fn rrect_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.programs.rrect_pipeline
    }

    fn image_rgba_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.programs.image_rgba_pipeline
    }

    fn image_a8_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.programs.image_a8_pipeline
    }

    fn nine_slice_rgba_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.programs.nine_slice_rgba_pipeline
    }

    fn nine_slice_a8_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.programs.nine_slice_a8_pipeline
    }

    fn spinner_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.programs.spinner_pipeline
    }

    fn neon_marker_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.programs.neon_marker_pipeline
    }

    fn rgba_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.programs.rgba_pipeline
    }

    fn a8_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.programs.a8_pipeline
    }

    fn sdf_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.programs.sdf_pipeline
    }

    fn effect_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.programs.effect_pipeline
    }

    fn scene3d_pipeline(&self, kind: Scene3dPipelineKind) -> &wgpu::RenderPipeline {
        match kind {
            Scene3dPipelineKind::AlphaDepthRead => {
                &self.programs.scene3d_color_tri_depth_read_pipeline
            }
            Scene3dPipelineKind::AlphaDepthWrite => {
                &self.programs.scene3d_color_tri_depth_write_pipeline
            }
            Scene3dPipelineKind::AlphaNoDepth => &self.programs.scene3d_color_tri_pipeline,
            Scene3dPipelineKind::AdditiveDepthRead => {
                &self.programs.scene3d_color_tri_add_depth_read_pipeline
            }
            Scene3dPipelineKind::AdditiveDepthWrite => {
                &self.programs.scene3d_color_tri_add_depth_write_pipeline
            }
            Scene3dPipelineKind::AdditiveNoDepth => &self.programs.scene3d_color_tri_add_pipeline,
        }
    }

    fn id_mask_raster_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.programs.id_mask_raster_pipeline
    }

    fn id_mask_programs(&self, packed: bool) -> &IdMaskVariantPrograms {
        match (packed, self.programs.id_mask_packed.as_ref()) {
            (true, Some(programs)) => programs,
            _ => &self.programs.id_mask_wide,
        }
    }

    fn id_mask_field_seed_pipeline(&self, packed: bool) -> &wgpu::RenderPipeline {
        &self.id_mask_programs(packed).field_seed_pipeline
    }

    fn id_mask_field_jump_pipeline(&self, packed: bool) -> &wgpu::RenderPipeline {
        &self.id_mask_programs(packed).field_jump_pipeline
    }

    fn id_mask_compositor_pipeline(&self, packed: bool) -> &wgpu::RenderPipeline {
        &self.id_mask_programs(packed).compositor_pipeline
    }

    fn render_direct(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        surface_view: &wgpu::TextureView,
    ) {
        if self.prepared_frame_active {
            self.render_prepared_direct(encoder, surface_view);
            return;
        }
        if self.scene3d_active {
            self.render_scene3d(encoder, surface_view);
        }
        if !self.id_mask_draws.is_empty() {
            self.render_id_mask_compositors(
                encoder,
                surface_view,
                if self.scene3d_active {
                    wgpu::LoadOp::Load
                } else {
                    wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                },
            );
        }
        if !self.scene3d_overlay_draws.is_empty() {
            self.render_scene3d_overlay(encoder, surface_view);
        }
        self.render_draw_range(
            encoder,
            surface_view,
            0,
            self.frame.draws.len(),
            None,
            if self.scene3d_active
                || !self.id_mask_draws.is_empty()
                || !self.scene3d_overlay_draws.is_empty()
            {
                wgpu::LoadOp::Load
            } else {
                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
            },
        );
    }

    fn render_prepared_direct(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        surface_view: &wgpu::TextureView,
    ) {
        if self.prepared_frame_plan.is_empty() {
            return;
        }
        if self.prepared_snapshot_bundle_active
        {
           let Some(snapshot_bundle) = self.prepared_snapshot_bundle.take() else
           {
              return;
           };
           self.stats.render_passes = self.stats.render_passes.saturating_add(1);
           self.stats.draw_passes = self.stats.draw_passes.saturating_add(1);
           let timestamp_pair = self.reserve_timestamp_pass(TimestampPassFamily::Draw);
           let timestamp_writes = self.timestamp_writes(timestamp_pair);
           {
              let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                 label: Some("oxide-webgpu-prepared-snapshot-pass"),
                 color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: surface_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                       load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                       store: wgpu::StoreOp::Store,
                    },
                 })],
                 depth_stencil_attachment: None,
                 timestamp_writes,
                 occlusion_query_set: None,
              });
              pass.execute_bundles(core::iter::once(&snapshot_bundle.bundle));
           }
           self.stats.draws = self.stats.draws.saturating_add(snapshot_bundle.draws);
           self.stats.draw_items = self.stats.draw_items.saturating_add(snapshot_bundle.draws);
           self.stats.rrect_instances = self.stats.rrect_instances
              .saturating_add(snapshot_bundle.rrect_instances);
           self.stats.rrect_triangles = self.stats.rrect_triangles
              .saturating_add(snapshot_bundle.rrect_instances.saturating_mul(2));
           self.stats.rrect_instance_bytes = self.stats.rrect_instance_bytes
              .saturating_add(u64::from(snapshot_bundle.rrect_instances)
                 .saturating_mul(RRECT_INSTANCE_BYTES as u64));
           self.stats.image_instances = self.stats.image_instances
              .saturating_add(snapshot_bundle.image_instances);
           self.stats.image_triangles = self.stats.image_triangles
              .saturating_add(snapshot_bundle.image_instances.saturating_mul(2));
           self.stats.image_instance_bytes = self.stats.image_instance_bytes
              .saturating_add(u64::from(snapshot_bundle.image_instances)
                 .saturating_mul(IMAGE_INSTANCE_BYTES as u64));
           self.stats.nine_slice_instances = self.stats.nine_slice_instances
              .saturating_add(snapshot_bundle.nine_slice_instances);
           self.stats.nine_slice_triangles = self.stats.nine_slice_triangles
              .saturating_add(snapshot_bundle.nine_slice_instances.saturating_mul(18));
           self.stats.nine_slice_instance_bytes = self.stats.nine_slice_instance_bytes
              .saturating_add(u64::from(snapshot_bundle.nine_slice_instances)
                 .saturating_mul(NINE_SLICE_INSTANCE_BYTES as u64));
           self.stats.spinner_instances = self.stats.spinner_instances
              .saturating_add(snapshot_bundle.spinner_instances);
           self.stats.spinner_triangles = self.stats.spinner_triangles
              .saturating_add(snapshot_bundle.spinner_instances.saturating_mul(24));
           self.stats.spinner_instance_bytes = self.stats.spinner_instance_bytes
              .saturating_add(u64::from(snapshot_bundle.spinner_instances)
                 .saturating_mul(SPINNER_INSTANCE_BYTES as u64));
           if snapshot_bundle.spinner_instances != 0
           {
              write_viewport_uniform(
                 &self.queue,
                 &self.viewport_buffer,
                 self.width,
                 self.height,
                 self.scale,
                 self.animation_phase,
              );
              self.stats.buffer_upload_bytes = self.stats.buffer_upload_bytes
                 .saturating_add(PREPARED_PROPERTY_UNIFORM_SIZE);
           }
           self.stats.render_bundle_replays = self.stats.render_bundle_replays.saturating_add(1);
           self.stats.render_bundle_execute_calls = self.stats.render_bundle_execute_calls.saturating_add(1);
           self.stats.render_bundle_draws = self.stats.render_bundle_draws.saturating_add(snapshot_bundle.draws);
           self.prepared_snapshot_bundle = Some(snapshot_bundle);
           return;
        }
        self.stats.render_passes = self.stats.render_passes.saturating_add(1);
        self.stats.draw_passes = self.stats.draw_passes.saturating_add(1);
        let cache = core::mem::take(&mut self.prepared_chunks);
        let plan = core::mem::take(&mut self.prepared_frame_plan);
        let timestamp_pair = self.reserve_timestamp_pass(TimestampPassFamily::Draw);
        let timestamp_writes = self.timestamp_writes(timestamp_pair);
        let mut draws = 0_u32;
        let mut draw_items = 0_u32;
        let mut pipeline_binds = 0_u32;
        let mut bind_group_binds = 0_u32;
        let mut scissor_sets = 0_u32;
        let mut bundle_replays = 0_u32;
        let mut bundle_execute_calls = 0_u32;
        let mut bundle_draws = 0_u32;
        let mut direct_draws = 0_u32;
        let mut rrect_instances = 0_u32;
        let mut image_instances = 0_u32;
        let mut nine_slice_instances = 0_u32;
        let mut spinner_instances = 0_u32;
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("oxide-webgpu-prepared-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: surface_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes,
                occlusion_query_set: None,
            });
            let mut plan_index = 0;
            while plan_index < plan.len() {
                let instance = plan[plan_index];
                let Some(chunk) = cache.get(instance.key) else {
                    plan_index += 1;
                    continue;
                };
                let chunk_draws = chunk.draws.len().min(u32::MAX as usize) as u32;
                draw_items = draw_items.saturating_add(chunk_draws);
                draws = draws.saturating_add(chunk_draws);
                rrect_instances = rrect_instances.saturating_add(chunk.rrect_instances);
                image_instances = image_instances.saturating_add(chunk.image_instances);
                nine_slice_instances = nine_slice_instances.saturating_add(chunk.nine_slice_instances);
                spinner_instances = spinner_instances.saturating_add(chunk.spinner_instances);
                if matches!(chunk.segments.as_slice(), [PreparedSegment::Bundle { .. }]) {
                    let start = plan_index;
                    while plan_index < plan.len() {
                        let Some(chunk) = cache.get(plan[plan_index].key) else { break };
                        let [PreparedSegment::Bundle { draws: segment_draws, .. }] = chunk.segments.as_slice() else { break };
                        if plan_index != start {
                            let chunk_draws = chunk.draws.len().min(u32::MAX as usize) as u32;
                            draw_items = draw_items.saturating_add(chunk_draws);
                            draws = draws.saturating_add(chunk_draws);
                            rrect_instances = rrect_instances.saturating_add(chunk.rrect_instances);
                            image_instances = image_instances.saturating_add(chunk.image_instances);
                            nine_slice_instances = nine_slice_instances.saturating_add(chunk.nine_slice_instances);
                            spinner_instances = spinner_instances.saturating_add(chunk.spinner_instances);
                        }
                        bundle_replays = bundle_replays.saturating_add(1);
                        bundle_draws = bundle_draws.saturating_add(*segment_draws);
                        plan_index += 1;
                    }
                    pass.execute_bundles(plan[start..plan_index].iter().filter_map(|instance| {
                        let chunk = cache.get(instance.key)?;
                        let [PreparedSegment::Bundle { bundle, .. }] = chunk.segments.as_slice() else { return None };
                        Some(bundle)
                    }));
                    bundle_execute_calls = bundle_execute_calls.saturating_add(1);
                    continue;
                }
                for segment in &chunk.segments {
                    match segment {
                        PreparedSegment::Bundle { bundle, draws } => {
                            pass.execute_bundles(core::iter::once(bundle));
                            bundle_replays = bundle_replays.saturating_add(1);
                            bundle_execute_calls = bundle_execute_calls.saturating_add(1);
                            bundle_draws = bundle_draws.saturating_add(*draws);
                        }
                        PreparedSegment::Direct { start, end } => {
                            let counters = self.encode_prepared_direct_draws(
                                &mut pass,
                                chunk,
                                &chunk.draws[*start..*end],
                                instance.property_offset,
                                instance.clip,
                            );
                            pipeline_binds = pipeline_binds.saturating_add(counters.0);
                            bind_group_binds = bind_group_binds.saturating_add(counters.1);
                            scissor_sets = scissor_sets.saturating_add(counters.2);
                            direct_draws = direct_draws
                                .saturating_add((*end - *start).min(u32::MAX as usize) as u32);
                        }
                    }
                }
                plan_index += 1;
            }
        }
        self.prepared_chunks = cache;
        self.prepared_frame_plan = plan;
        self.stats.draws = self.stats.draws.saturating_add(draws);
        self.stats.draw_items = self.stats.draw_items.saturating_add(draw_items);
        self.stats.rrect_instances = self.stats.rrect_instances.saturating_add(rrect_instances);
        self.stats.rrect_triangles = self.stats.rrect_triangles
            .saturating_add(rrect_instances.saturating_mul(2));
        self.stats.rrect_instance_bytes = self.stats.rrect_instance_bytes
            .saturating_add(u64::from(rrect_instances).saturating_mul(RRECT_INSTANCE_BYTES as u64));
        self.stats.image_instances = self.stats.image_instances.saturating_add(image_instances);
        self.stats.image_triangles = self.stats.image_triangles
            .saturating_add(image_instances.saturating_mul(2));
        self.stats.image_instance_bytes = self.stats.image_instance_bytes
            .saturating_add(u64::from(image_instances).saturating_mul(IMAGE_INSTANCE_BYTES as u64));
        self.stats.nine_slice_instances = self.stats.nine_slice_instances
            .saturating_add(nine_slice_instances);
        self.stats.nine_slice_triangles = self.stats.nine_slice_triangles
            .saturating_add(nine_slice_instances.saturating_mul(18));
        self.stats.nine_slice_instance_bytes = self.stats.nine_slice_instance_bytes
            .saturating_add(u64::from(nine_slice_instances)
               .saturating_mul(NINE_SLICE_INSTANCE_BYTES as u64));
        self.stats.spinner_instances = self.stats.spinner_instances
            .saturating_add(spinner_instances);
        self.stats.spinner_triangles = self.stats.spinner_triangles
            .saturating_add(spinner_instances.saturating_mul(24));
        self.stats.spinner_instance_bytes = self.stats.spinner_instance_bytes
            .saturating_add(u64::from(spinner_instances)
               .saturating_mul(SPINNER_INSTANCE_BYTES as u64));
        if spinner_instances != 0
        {
            write_viewport_uniform(
                &self.queue,
                &self.viewport_buffer,
                self.width,
                self.height,
                self.scale,
                self.animation_phase,
            );
            self.stats.buffer_upload_bytes = self.stats.buffer_upload_bytes
                .saturating_add(PREPARED_PROPERTY_UNIFORM_SIZE);
        }
        self.stats.draw_pipeline_binds = self.stats.draw_pipeline_binds.saturating_add(pipeline_binds);
        self.stats.draw_bind_group_binds = self.stats.draw_bind_group_binds.saturating_add(bind_group_binds);
        self.stats.draw_scissor_sets = self.stats.draw_scissor_sets.saturating_add(scissor_sets);
        self.stats.render_bundle_replays = self.stats.render_bundle_replays.saturating_add(bundle_replays);
        self.stats.render_bundle_execute_calls = self.stats.render_bundle_execute_calls.saturating_add(bundle_execute_calls);
        self.stats.render_bundle_draws = self.stats.render_bundle_draws.saturating_add(bundle_draws);
        self.stats.prepared_direct_draws = self.stats.prepared_direct_draws.saturating_add(direct_draws);
    }

    fn encode_prepared_direct_draws(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        chunk: &PreparedChunk,
        draws: &[GpuDraw],
        property_offset: Option<u32>,
        instance_clip: Option<api::RectI>,
    ) -> (u32, u32, u32) {
        if let Some(offset) = property_offset
        {
            pass.set_bind_group(0, &self.prepared_property_ring.bind_group, &[offset]);
        }
        else
        {
            pass.set_bind_group(0, &self.viewport_bind_group, &[0]);
        }
        let mut pipeline_binds = 0_u32;
        let mut bind_group_binds = 0_u32;
        let mut scissor_sets = 0_u32;
        let mut bound_pipeline = None;
        let mut bound_bind = None;
        let mut bound_clip = None;
        let mut bound_index = None;
        for draw in draws {
            if !matches!(draw.kind, DrawKind::RRect { .. } | DrawKind::Image { .. } | DrawKind::NineSlice { .. } | DrawKind::Spinner { .. })
                && bound_index != Some(draw.index_kind) {
                match draw.index_kind {
                    PackedIndexKind::U16 => {
                        let Some(buffer) = chunk.index_buffer_u16.as_ref() else { continue };
                        pass.set_index_buffer(buffer.slice(..), wgpu::IndexFormat::Uint16);
                    }
                    PackedIndexKind::U32 => {
                        let Some(buffer) = chunk.index_buffer_u32.as_ref() else { continue };
                        pass.set_index_buffer(buffer.slice(..), wgpu::IndexFormat::Uint32);
                    }
                }
                bound_index = Some(draw.index_kind);
            }
            let Some(state) = self.draw_state_key(*draw) else { continue };
            let clip = instance_clip.map_or(state.clip, |instance| prepared_intersect_clip(instance, state.clip));
            if bound_clip != Some(clip) {
                if !set_scissor(
                    pass,
                    clip,
                    self.scale,
                    [0.0, 0.0],
                    [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                    self.width,
                    self.height,
                )
                {
                   continue;
                }
                scissor_sets = scissor_sets.saturating_add(1);
                bound_clip = Some(clip);
            }
            if bound_pipeline != Some(state.pipeline) {
                let Some(pipeline) = self.pipeline_for_draw(state.pipeline) else { continue };
                pass.set_pipeline(pipeline);
                if state.pipeline == DrawPipelineKey::RRect
                {
                    let Some(buffer) = chunk.rrect_instance_buffer.as_ref() else { continue };
                    pass.set_vertex_buffer(0, buffer.slice(..));
                }
                else if matches!(state.pipeline, DrawPipelineKey::ImageRgba | DrawPipelineKey::ImageA8)
                {
                    let Some(buffer) = chunk.image_instance_buffer.as_ref() else { continue };
                    pass.set_vertex_buffer(0, self.programs.image_unit_vertex_buffer.slice(..));
                    pass.set_vertex_buffer(1, buffer.slice(..));
                    pass.set_index_buffer(
                       self.programs.image_unit_index_buffer.slice(..),
                       wgpu::IndexFormat::Uint16,
                    );
                    bound_index = None;
                }
                else if matches!(state.pipeline, DrawPipelineKey::NineSliceRgba | DrawPipelineKey::NineSliceA8)
                {
                    let Some(buffer) = chunk.nine_slice_instance_buffer.as_ref() else { continue };
                    pass.set_vertex_buffer(0, self.programs.nine_slice_unit_vertex_buffer.slice(..));
                    pass.set_vertex_buffer(1, buffer.slice(..));
                    pass.set_index_buffer(
                       self.programs.nine_slice_unit_index_buffer.slice(..),
                       wgpu::IndexFormat::Uint16,
                    );
                    bound_index = None;
                }
                else if state.pipeline == DrawPipelineKey::Spinner
                {
                    let Some(buffer) = chunk.spinner_instance_buffer.as_ref() else { continue };
                    pass.set_vertex_buffer(0, buffer.slice(..));
                    bound_index = None;
                }
                else
                {
                    let Some(buffer) = chunk.vertex_buffer.as_ref() else { continue };
                    pass.set_vertex_buffer(0, buffer.slice(..));
                }
                pipeline_binds = pipeline_binds.saturating_add(1);
                bound_pipeline = Some(state.pipeline);
            }
            if bound_bind != Some(state.bind) {
                if let DrawBindKey::Texture { image } = state.bind {
                    let Some(image) = self.image(api::ImageHandle(image)) else { continue };
                    pass.set_bind_group(1, &image.bind_group, &[]);
                    bind_group_binds = bind_group_binds.saturating_add(1);
                }
                bound_bind = Some(state.bind);
            }
            if let DrawKind::RRect { first_instance, instance_count } = draw.kind
            {
                pass.draw(0..6, first_instance..first_instance.saturating_add(instance_count));
            }
            else if let DrawKind::Image { first_instance, instance_count, .. } = draw.kind
            {
                pass.draw_indexed(
                   0..6,
                   0,
                   first_instance..first_instance.saturating_add(instance_count),
                );
            }
            else if let DrawKind::NineSlice { first_instance, instance_count, .. } = draw.kind
            {
                pass.draw_indexed(
                   0..NINE_SLICE_INDEX_COUNT,
                   0,
                   first_instance..first_instance.saturating_add(instance_count),
                );
            }
            else if let DrawKind::Spinner { first_instance, instance_count } = draw.kind
            {
                pass.draw(
                   0..SPINNER_VERTEX_COUNT,
                   first_instance..first_instance.saturating_add(instance_count),
                );
            }
            else
            {
                pass.draw_indexed(
                    draw.first_index..draw.first_index.saturating_add(draw.index_count),
                    draw.base_vertex,
                    0..1,
                );
            }
        }
        (pipeline_binds, bind_group_binds, scissor_sets)
    }

    fn render_scene_with_effects(&mut self, encoder: &mut wgpu::CommandEncoder) {
        self.ensure_scene_target();
        let Some(scene_target) = self.scene_target.as_ref() else {
            return;
        };
        let scene_texture = scene_target.texture.clone();
        let scene_view = scene_target.view.clone();
        if self.scene3d_active {
            self.render_scene3d(encoder, &scene_view);
        } else {
            self.clear_target(encoder, &scene_view, "oxide-webgpu-clear-scene");
        }
        if !self.id_mask_draws.is_empty() {
            self.render_id_mask_compositors(encoder, &scene_view, wgpu::LoadOp::Load);
        }
        if !self.scene3d_overlay_draws.is_empty() {
            self.render_scene3d_overlay(encoder, &scene_view);
        }
        self.render_draw_target_with_effects(
            encoder,
            &scene_texture,
            &scene_view,
            None,
            0,
            self.frame.draws.len(),
        );
    }

    fn render_layer_passes(&mut self, encoder: &mut wgpu::CommandEncoder) {
        for pass_index in 0..self.frame.layer_passes.len() {
            let layer_pass = self.frame.layer_passes[pass_index];
            let Some(layer) = self.layers.get(&layer_pass.id) else {
                continue;
            };
            let texture = layer.texture.clone();
            let view = layer.view.clone();
            self.clear_target(encoder, &view, "oxide-webgpu-clear-layer");
            self.render_draw_target_with_effects(
                encoder,
                &texture,
                &view,
                Some(layer_pass.id),
                layer_pass.start,
                layer_pass.end,
            );
            self.stats.layer_passes = self.stats.layer_passes.saturating_add(1);
        }
    }

    fn render_draw_target_with_effects(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        target_texture: &wgpu::Texture,
        target_view: &wgpu::TextureView,
        target: Option<u32>,
        start: usize,
        end: usize,
    ) {
        if self.target_uses_backdrop(target, start, end) {
            self.ensure_scratch_target();
        }
        let limit = end.min(self.frame.draws.len());
        let mut start = start.min(limit);
        while start < limit {
            while start < limit && self.frame.draws[start].target != target {
                start += 1;
            }
            if start >= limit {
                break;
            }
            if let DrawKind::Backdrop { sigma, .. } = self.frame.draws[start].kind {
                let end = self.backdrop_batch_end(start, target, limit);
                let Some(scratch_texture) = self
                    .scratch_target
                    .as_ref()
                    .map(|scratch| scratch.texture.clone())
                else {
                    return;
                };
                let Some((source_origin, destination_origin, copy_extent)) =
                    self.target_copy_region(target)
                else {
                    return;
                };
                encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: target_texture,
                        mip_level: 0,
                        origin: source_origin,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyTextureInfo {
                        texture: &scratch_texture,
                        mip_level: 0,
                        origin: destination_origin,
                        aspect: wgpu::TextureAspect::All,
                    },
                    copy_extent,
                );
                self.stats.texture_copies = self.stats.texture_copies.saturating_add(1);
                let copy_pixels = u64::from(copy_extent.width)
                    .saturating_mul(u64::from(copy_extent.height));
                self.stats.texture_copy_pixels =
                    self.stats.texture_copy_pixels.saturating_add(copy_pixels);
                self.stats.texture_copy_bytes = self.stats.texture_copy_bytes.saturating_add(
                    copy_pixels.saturating_mul(color_texture_bytes_per_pixel(self.config.format)),
                );
                if !self.effect_uniform_batch_enabled {
                    self.write_effect_uniform(sigma);
                    self.frame.draws[start].effect_uniform_offset = 0;
                }
                self.render_draw_range(
                    encoder,
                    target_view,
                    start,
                    end,
                    target,
                    wgpu::LoadOp::Load,
                );
                start = end;
            } else {
                let mut end = start + 1;
                while end < limit {
                    let draw = self.frame.draws[end];
                    if draw.target == target && matches!(draw.kind, DrawKind::Backdrop { .. }) {
                        break;
                    }
                    end += 1;
                }
                self.render_draw_range(
                    encoder,
                    target_view,
                    start,
                    end,
                    target,
                    wgpu::LoadOp::Load,
                );
                start = end;
            }
        }
    }

    fn clear_target(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
        label: &'static str,
    ) {
        self.stats.render_passes = self.stats.render_passes.saturating_add(1);
        self.stats.clear_passes = self.stats.clear_passes.saturating_add(1);
        let timestamp_pair = self.reserve_timestamp_pass(TimestampPassFamily::Clear);
        let timestamp_writes = self.timestamp_writes(timestamp_pair);
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes,
            occlusion_query_set: None,
        });
    }

    fn render_scene3d(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
    ) {
        if self.scene3d_bind_group.is_none() {
            return;
        }
        self.ensure_scene_depth_target();
        let timestamp_pair = self.reserve_timestamp_pass(TimestampPassFamily::Scene3d);
        let timestamp_writes = self.timestamp_writes(timestamp_pair);
        let Some(depth_target) = self.scene_depth_target.as_ref() else {
            return;
        };
        let Some(bind_group) = self.scene3d_bind_group.as_ref() else {
            return;
        };
        let clear =
            self.scene3d_clear_color.unwrap_or_else(|| api::Color::rgba(0.0, 0.0, 0.0, 0.0));
        let depth_ops = if self.scene3d_clear_depth {
            wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }
        } else {
            wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("oxide-webgpu-scene3d-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: clear.r as f64,
                        g: clear.g as f64,
                        b: clear.b as f64,
                        a: clear.a as f64,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_target.view,
                depth_ops: Some(depth_ops),
                stencil_ops: None,
            }),
            timestamp_writes,
            occlusion_query_set: None,
        });

        let mut encoded_draws = 0_u32;
        for draw_index in 0..self.scene3d_draws.len() {
            let draw = self.scene3d_draws[draw_index];
            let Some(mesh) = self.meshes_3d.get(draw.mesh).and_then(Option::as_ref) else {
                continue;
            };
            let pipeline = self.scene3d_pipeline(draw.pipeline);
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind_group, &[draw.uniform_offset]);
            pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
            pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            encoded_draws = encoded_draws.saturating_add(1);
        }
        drop(pass);
        self.stats.draws = self.stats.draws.saturating_add(encoded_draws);
        self.stats.render_passes = self.stats.render_passes.saturating_add(1);
        self.stats.scene3d_passes = self.stats.scene3d_passes.saturating_add(1);
    }

    fn render_scene3d_overlay(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
    ) {
        if self.scene3d_bind_group.is_none() {
            return;
        }
        self.ensure_scene_depth_target();
        let timestamp_pair = self.reserve_timestamp_pass(TimestampPassFamily::Scene3dOverlay);
        let timestamp_writes = self.timestamp_writes(timestamp_pair);
        let Some(depth_target) = self.scene_depth_target.as_ref() else {
            return;
        };
        let Some(bind_group) = self.scene3d_bind_group.as_ref() else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("oxide-webgpu-scene3d-overlay-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_target.view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes,
            occlusion_query_set: None,
        });

        let mut encoded_draws = 0_u32;
        for draw_index in 0..self.scene3d_overlay_draws.len() {
            let draw = self.scene3d_overlay_draws[draw_index];
            let Some(mesh) = self.meshes_3d.get(draw.mesh).and_then(Option::as_ref) else {
                continue;
            };
            let pipeline = self.scene3d_pipeline(draw.pipeline);
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind_group, &[draw.uniform_offset]);
            pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
            pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            encoded_draws = encoded_draws.saturating_add(1);
        }
        drop(pass);
        self.stats.draws = self.stats.draws.saturating_add(encoded_draws);
        self.stats.render_passes = self.stats.render_passes.saturating_add(1);
        self.stats.scene3d_overlay_passes = self.stats.scene3d_overlay_passes.saturating_add(1);
    }

    fn render_id_mask_compositors(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
        first_load: wgpu::LoadOp<wgpu::Color>,
    ) {
        let mut load = first_load;
        let mut encoded_draws = 0_u32;
        let mut encoded_render_passes = 0_u32;
        let mut encoded_buffer_upload_bytes = 0_u64;
        if self.id_mask_draws.is_empty() {
            return;
        }
        self.ensure_id_mask_uniform_capacity(self.id_mask_uniform_capacity_needed());
        self.ensure_id_mask_raster_bind_group();
        if !self.resolve_id_mask_draws() {
            return;
        }
        self.prepare_id_mask_uniforms();
        let Some(uniform_buffer) = self.id_mask_uniform_buffer.as_ref() else { return };
        self.queue.write_buffer(uniform_buffer, 0, &self.id_mask_uniform_bytes);
        let uniform_bytes = self.id_mask_uniform_bytes.len() as u64;
        let uniform_slots = self.id_mask_uniform_offsets.iter().fold(0_u32, |total, offsets| {
            total.saturating_add(1).saturating_add(if offsets.cache_hit {
                0
            } else {
                1_u32.saturating_add(offsets.field_count as u32)
            })
        });
        encoded_buffer_upload_bytes = encoded_buffer_upload_bytes.saturating_add(uniform_bytes);
        self.stats.id_mask_uniform_writes = self.stats.id_mask_uniform_writes.saturating_add(1);
        self.stats.id_mask_uniform_bytes =
            self.stats.id_mask_uniform_bytes.saturating_add(uniform_bytes);
        self.stats.id_mask_uniform_slots =
            self.stats.id_mask_uniform_slots.saturating_add(uniform_slots);
        for draw_index in 0..self.id_mask_draws.len() {
            let draw = self.id_mask_draws[draw_index];
            let uniform_offsets = self.id_mask_uniform_offsets[draw_index];
            let width = draw.mask_width.max(1);
            let height = draw.mask_height.max(1);
            let cache_start = draw.vertex_cache_first as usize;
            let cache_end = cache_start.saturating_add(draw.vertex_cache_count as usize);
            let resolved = self.id_mask_resolved_draws[draw_index].clone();
            let targets = resolved.targets;
            let cache_hit = resolved.cache_hit;
            #[cfg(feature = "snapshot-tests")]
            {
                self.id_mask_snapshot_targets = Some(targets.clone());
            }
            let city_view = &targets.city_view;
            let neighborhood_view = &targets.neighborhood_view;
            let packed_fields = targets.packed_fields();
            let raster_bind_group = self.id_mask_raster_bind_group.clone();
            let Some(raster_bind_group) = raster_bind_group.as_ref() else { continue };

            if !cache_hit {
                for cache_pos in cache_start..cache_end {
                    let cache_index = self.id_mask_draw_chunk_indices[cache_pos];
                    if let Some(upload_bytes) =
                        self.ensure_id_mask_vertex_cache_uploaded(cache_index)
                    {
                        encoded_buffer_upload_bytes =
                            encoded_buffer_upload_bytes.saturating_add(upload_bytes);
                    }
                }

            {
                let timestamp_pair = reserve_webgpu_timestamp_pass(
                    &mut self.timestamp_queries,
                    TimestampPassFamily::IdMaskRaster,
                );
                let timestamp_writes =
                    webgpu_timestamp_writes(&self.timestamp_queries, timestamp_pair);
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("oxide-webgpu-id-mask-raster-pass"),
                    color_attachments: &[
                        Some(wgpu::RenderPassColorAttachment {
                            view: city_view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                        }),
                        Some(wgpu::RenderPassColorAttachment {
                            view: neighborhood_view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                        }),
                    ],
                    depth_stencil_attachment: None,
                    timestamp_writes,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(self.id_mask_raster_pipeline());
                pass.set_bind_group(0, raster_bind_group, &[uniform_offsets.raster]);
                for cache_pos in cache_start..cache_end {
                    let cache_index = self.id_mask_draw_chunk_indices[cache_pos];
                    let Some(cache) = self.id_mask_vertex_caches.get(cache_index) else {
                        continue;
                    };
                    let Some(vertex_buffer) = cache.buffer.as_ref() else { continue };
                    let vertex_count = (cache.bytes.len() / ID_MASK_VERTEX_STRIDE as usize) as u32;
                    pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                    pass.draw(0..vertex_count, 0..1);
                }
            }
            encoded_render_passes = encoded_render_passes.saturating_add(1);
            self.stats.id_mask_raster_passes = self.stats.id_mask_raster_passes.saturating_add(1);

            // Seed nearest-city and seam fields from the exact rasterized masks,
            // then jump-flood them. The final beauty compositor should only read
            // these fields; reintroducing radius searches there recreates the GPU
            // scheduling stalls this path was built to remove.
            let mut field_offset_index = uniform_offsets.field_first;
            {
                let timestamp_pair = reserve_webgpu_timestamp_pass(
                    &mut self.timestamp_queries,
                    TimestampPassFamily::IdMaskFieldSeed,
                );
                let timestamp_writes =
                    webgpu_timestamp_writes(&self.timestamp_queries, timestamp_pair);
                encode_id_mask_field_pass(
                    encoder,
                    "oxide-webgpu-id-mask-field-seed-pass",
                    targets.field_pair(true),
                    self.id_mask_field_seed_pipeline(packed_fields),
                    targets.field_bind_group(false),
                    self.id_mask_field_uniform_offsets[field_offset_index],
                    timestamp_writes,
                );
            }
            field_offset_index += 1;
            encoded_render_passes = encoded_render_passes.saturating_add(1);
            self.stats.id_mask_field_seed_passes =
                self.stats.id_mask_field_seed_passes.saturating_add(1);

            let mut src_is_a = true;
            let mut jump = width.max(height).next_power_of_two() / 2;
            while jump >= 1 {
                {
                    let timestamp_pair = reserve_webgpu_timestamp_pass(
                        &mut self.timestamp_queries,
                        TimestampPassFamily::IdMaskFieldJump,
                    );
                    let timestamp_writes =
                        webgpu_timestamp_writes(&self.timestamp_queries, timestamp_pair);
                    encode_id_mask_field_pass(
                        encoder,
                        "oxide-webgpu-id-mask-field-jump-pass",
                        targets.field_pair(!src_is_a),
                        self.id_mask_field_jump_pipeline(packed_fields),
                        targets.field_bind_group(src_is_a),
                        self.id_mask_field_uniform_offsets[field_offset_index],
                        timestamp_writes,
                    );
                }
                field_offset_index += 1;
                encoded_render_passes = encoded_render_passes.saturating_add(1);
                self.stats.id_mask_field_jump_passes =
                    self.stats.id_mask_field_jump_passes.saturating_add(1);
                src_is_a = !src_is_a;
                jump /= 2;
            }
            debug_assert_eq!(field_offset_index, uniform_offsets.field_first + uniform_offsets.field_count);
            encoded_draws = encoded_draws
                .saturating_add(2)
                .saturating_add(width.max(height).next_power_of_two().trailing_zeros());
            }
            let compositor_bind_group = targets.compositor_bind_group(
                id_mask_final_fields_are_a(width, height),
            );

            {
                let timestamp_pair = reserve_webgpu_timestamp_pass(
                    &mut self.timestamp_queries,
                    TimestampPassFamily::IdMaskCompositor,
                );
                let timestamp_writes =
                    webgpu_timestamp_writes(&self.timestamp_queries, timestamp_pair);
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("oxide-webgpu-id-mask-compositor-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes,
                    occlusion_query_set: None,
                });
                // Keep the expensive beauty compositor bounded to the requested
                // surface. The shader's quad is local to the mask, so the
                // hardware viewport/scissor owns page/UI placement.
                set_viewport_and_scissor_rect(
                    &mut pass,
                    draw.viewport,
                    self.scale,
                    self.width,
                    self.height,
                );
                pass.set_pipeline(self.id_mask_compositor_pipeline(packed_fields));
                pass.set_bind_group(0, compositor_bind_group, &[uniform_offsets.compositor]);
                pass.draw(0..6, 0..1);
            }
            encoded_render_passes = encoded_render_passes.saturating_add(1);
            self.stats.id_mask_compositor_passes =
                self.stats.id_mask_compositor_passes.saturating_add(1);
            load = wgpu::LoadOp::Load;
            encoded_draws = encoded_draws.saturating_add(1);
        }
        self.stats.draws = self.stats.draws.saturating_add(encoded_draws);
        self.stats.render_passes = self.stats.render_passes.saturating_add(encoded_render_passes);
        self.stats.buffer_upload_bytes =
            self.stats.buffer_upload_bytes.saturating_add(encoded_buffer_upload_bytes);
    }

    fn draw_state_key(&self, draw: GpuDraw) -> Option<DrawStateKey> {
        let (pipeline, bind) = match draw.kind {
            DrawKind::Solid => (DrawPipelineKey::Solid, DrawBindKey::None),
            DrawKind::RRect { .. } => (DrawPipelineKey::RRect, DrawBindKey::None),
            DrawKind::Image { image, kind, .. } => {
                self.image(api::ImageHandle(image))?;
                let pipeline = match kind {
                    GpuImageKind::Rgba => DrawPipelineKey::ImageRgba,
                    GpuImageKind::A8 => DrawPipelineKey::ImageA8,
                };
                (pipeline, DrawBindKey::Texture { image })
            }
            DrawKind::NineSlice { image, kind, .. } => {
                self.image(api::ImageHandle(image))?;
                let pipeline = match kind {
                    GpuImageKind::Rgba => DrawPipelineKey::NineSliceRgba,
                    GpuImageKind::A8 => DrawPipelineKey::NineSliceA8,
                };
                (pipeline, DrawBindKey::Texture { image })
            }
            DrawKind::Spinner { .. } => (DrawPipelineKey::Spinner, DrawBindKey::None),
            DrawKind::NeonMarker { .. } => (DrawPipelineKey::NeonMarker, DrawBindKey::None),
            DrawKind::Rgba { image } => {
                self.image(api::ImageHandle(image))?;
                (DrawPipelineKey::Rgba, DrawBindKey::Texture { image })
            }
            DrawKind::A8 { image } => {
                self.image(api::ImageHandle(image))?;
                (DrawPipelineKey::A8, DrawBindKey::Texture { image })
            }
            DrawKind::Sdf { image } => {
                self.image(api::ImageHandle(image))?;
                (DrawPipelineKey::Sdf, DrawBindKey::Texture { image })
            }
            DrawKind::Layer { id } => {
                self.layers.get(&id)?;
                (DrawPipelineKey::Rgba, DrawBindKey::Layer { id })
            }
            DrawKind::Backdrop { .. } => (
                DrawPipelineKey::Effect,
                DrawBindKey::Effect { offset: draw.effect_uniform_offset },
            ),
        };
        Some(DrawStateKey { pipeline, bind, clip: draw.clip })
    }

    fn draw_target_space(&self, target: Option<u32>) -> Option<(wgpu::BindGroup, [f32; 2], [f32; 6], u32, u32)>
    {
       match target
       {
          None => Some((
             self.viewport_bind_group.clone(),
             [0.0, 0.0],
             [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
             self.width,
             self.height,
          )),
          Some(id) => self.layers.get(&id).map(|layer| {
             (
                layer.viewport_bind_group.clone(),
                [layer.rect.x, layer.rect.y],
                [
                   layer.viewport[4], layer.viewport[5],
                   layer.viewport[6], layer.viewport[7],
                   layer.viewport[8], layer.viewport[9],
                ],
                layer.width,
                layer.height,
             )
          }),
       }
    }

    fn render_draw_range(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
        start: usize,
        end: usize,
        target: Option<u32>,
        load: wgpu::LoadOp<wgpu::Color>,
    ) {
        if start >= end {
            return;
        }
        if self.vertex_buffer.is_none() && self.rrect_instance_buffer.is_none()
            && self.image_instance_buffer.is_none()
            && self.nine_slice_instance_buffer.is_none()
            && self.spinner_instance_buffer.is_none()
            && self.neon_marker_instance_buffer.is_none() {
            return;
        }
        if !self.frame.draws[start..end].iter().any(|draw| draw.target == target) {
            return;
        }
        let Some((viewport_bind_group, target_origin, target_transform, target_width, target_height)) =
            self.draw_target_space(target)
        else {
            return;
        };
        self.stats.render_passes = self.stats.render_passes.saturating_add(1);
        self.stats.draw_passes = self.stats.draw_passes.saturating_add(1);

        let timestamp_pair = self.reserve_timestamp_pass(TimestampPassFamily::Draw);
        let timestamp_writes = self.timestamp_writes(timestamp_pair);
        let vertex_buffer = self.vertex_buffer.clone();
        let rrect_instance_buffer = self.rrect_instance_buffer.clone();
        let image_instance_buffer = self.image_instance_buffer.clone();
        let nine_slice_instance_buffer = self.nine_slice_instance_buffer.clone();
        let spinner_instance_buffer = self.spinner_instance_buffer.clone();
        let neon_marker_instance_buffer = self.neon_marker_instance_buffer.clone();
        let scratch_bind_group = self
            .scratch_target
            .as_ref()
            .map(|target| target.bind_group.clone());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("oxide-webgpu-draw-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes,
            occlusion_query_set: None,
        });
        pass.set_bind_group(0, &viewport_bind_group, &[0]);

        let mut encoded_draws = 0_u32;
        let mut draw_items = 0_u32;
        let mut pipeline_binds = 0_u32;
        let mut draw_bind_group_binds = 0_u32;
        let mut scissor_sets = 0_u32;
        let mut bound_pipeline: Option<DrawPipelineKey> = None;
        let mut bound_bind: Option<DrawBindKey> = None;
        let mut bound_clip: Option<api::RectI> = None;
        let mut bound_index: Option<PackedIndexKind> = None;
        for draw_index in start..end {
            let draw = self.frame.draws[draw_index];
            if draw.target != target {
                continue;
            }
            if !matches!(draw.kind, DrawKind::RRect { .. } | DrawKind::Image { .. } | DrawKind::NineSlice { .. } | DrawKind::Spinner { .. } | DrawKind::NeonMarker { .. })
                && bound_index != Some(draw.index_kind) {
                match draw.index_kind {
                    PackedIndexKind::U16 => {
                        let Some(buffer) = &self.index_buffer_u16 else {
                            continue;
                        };
                        pass.set_index_buffer(buffer.slice(..), wgpu::IndexFormat::Uint16);
                    }
                    PackedIndexKind::U32 => {
                        let Some(buffer) = &self.index_buffer_u32 else {
                            continue;
                        };
                        pass.set_index_buffer(buffer.slice(..), wgpu::IndexFormat::Uint32);
                    }
                }
                bound_index = Some(draw.index_kind);
            }
            let Some(state) = self.draw_state_key(draw) else {
                continue;
            };
            let force_bind = !self.draw_state_cache_enabled;
            draw_items = draw_items.saturating_add(1);
            if force_bind || bound_clip != Some(state.clip) {
                if !set_scissor(
                    &mut pass,
                    state.clip,
                    self.scale,
                    target_origin,
                    target_transform,
                    target_width,
                    target_height,
                ) {
                    continue;
                }
                scissor_sets = scissor_sets.saturating_add(1);
                bound_clip = Some(state.clip);
            }
            if force_bind || bound_pipeline != Some(state.pipeline) {
                match state.pipeline {
                    DrawPipelineKey::RRect => {
                        let Some(buffer) = rrect_instance_buffer.as_ref() else { continue };
                        pass.set_pipeline(self.rrect_pipeline());
                        pass.set_vertex_buffer(0, buffer.slice(..));
                    }
                    DrawPipelineKey::ImageRgba => {
                        let Some(buffer) = image_instance_buffer.as_ref() else { continue };
                        pass.set_pipeline(self.image_rgba_pipeline());
                        pass.set_vertex_buffer(0, self.programs.image_unit_vertex_buffer.slice(..));
                        pass.set_vertex_buffer(1, buffer.slice(..));
                        pass.set_index_buffer(
                            self.programs.image_unit_index_buffer.slice(..),
                            wgpu::IndexFormat::Uint16,
                        );
                        bound_index = None;
                    }
                    DrawPipelineKey::ImageA8 => {
                        let Some(buffer) = image_instance_buffer.as_ref() else { continue };
                        pass.set_pipeline(self.image_a8_pipeline());
                        pass.set_vertex_buffer(0, self.programs.image_unit_vertex_buffer.slice(..));
                        pass.set_vertex_buffer(1, buffer.slice(..));
                        pass.set_index_buffer(
                            self.programs.image_unit_index_buffer.slice(..),
                            wgpu::IndexFormat::Uint16,
                        );
                        bound_index = None;
                    }
                    DrawPipelineKey::NineSliceRgba => {
                        let Some(buffer) = nine_slice_instance_buffer.as_ref() else { continue };
                        pass.set_pipeline(self.nine_slice_rgba_pipeline());
                        pass.set_vertex_buffer(0, self.programs.nine_slice_unit_vertex_buffer.slice(..));
                        pass.set_vertex_buffer(1, buffer.slice(..));
                        pass.set_index_buffer(
                            self.programs.nine_slice_unit_index_buffer.slice(..),
                            wgpu::IndexFormat::Uint16,
                        );
                        bound_index = None;
                    }
                    DrawPipelineKey::NineSliceA8 => {
                        let Some(buffer) = nine_slice_instance_buffer.as_ref() else { continue };
                        pass.set_pipeline(self.nine_slice_a8_pipeline());
                        pass.set_vertex_buffer(0, self.programs.nine_slice_unit_vertex_buffer.slice(..));
                        pass.set_vertex_buffer(1, buffer.slice(..));
                        pass.set_index_buffer(
                            self.programs.nine_slice_unit_index_buffer.slice(..),
                            wgpu::IndexFormat::Uint16,
                        );
                        bound_index = None;
                    }
                    DrawPipelineKey::Spinner => {
                        let Some(buffer) = spinner_instance_buffer.as_ref() else { continue };
                        pass.set_pipeline(self.spinner_pipeline());
                        pass.set_vertex_buffer(0, buffer.slice(..));
                        bound_index = None;
                    }
                    DrawPipelineKey::NeonMarker => {
                        let Some(buffer) = neon_marker_instance_buffer.as_ref() else { continue };
                        pass.set_pipeline(self.neon_marker_pipeline());
                        pass.set_vertex_buffer(0, buffer.slice(..));
                        bound_index = None;
                    }
                    DrawPipelineKey::Solid => {
                        let Some(buffer) = vertex_buffer.as_ref() else { continue };
                        pass.set_pipeline(self.solid_pipeline());
                        pass.set_vertex_buffer(0, buffer.slice(..));
                    }
                    DrawPipelineKey::Rgba => {
                        let Some(buffer) = vertex_buffer.as_ref() else { continue };
                        pass.set_pipeline(self.rgba_pipeline());
                        pass.set_vertex_buffer(0, buffer.slice(..));
                    }
                    DrawPipelineKey::A8 => {
                        let Some(buffer) = vertex_buffer.as_ref() else { continue };
                        pass.set_pipeline(self.a8_pipeline());
                        pass.set_vertex_buffer(0, buffer.slice(..));
                    }
                    DrawPipelineKey::Sdf => {
                        let Some(buffer) = vertex_buffer.as_ref() else { continue };
                        pass.set_pipeline(self.sdf_pipeline());
                        pass.set_vertex_buffer(0, buffer.slice(..));
                    }
                    DrawPipelineKey::Effect => {
                        let Some(buffer) = vertex_buffer.as_ref() else { continue };
                        pass.set_pipeline(self.effect_pipeline());
                        pass.set_vertex_buffer(0, buffer.slice(..));
                    }
                }
                pipeline_binds = pipeline_binds.saturating_add(1);
                bound_pipeline = Some(state.pipeline);
            }
            if force_bind || bound_bind != Some(state.bind) {
                match state.bind {
                    DrawBindKey::None => {}
                    DrawBindKey::Texture { image } => {
                        let Some(image) = self.image(api::ImageHandle(image)) else {
                            continue;
                        };
                        pass.set_bind_group(1, &image.bind_group, &[]);
                        draw_bind_group_binds = draw_bind_group_binds.saturating_add(1);
                    }
                    DrawBindKey::Layer { id } => {
                        let Some(layer) = self.layers.get(&id) else {
                            continue;
                        };
                        pass.set_bind_group(1, &layer.bind_group, &[]);
                        draw_bind_group_binds = draw_bind_group_binds.saturating_add(1);
                    }
                    DrawBindKey::Effect { offset } => {
                        let Some(bind_group) = scratch_bind_group.as_ref() else {
                            continue;
                        };
                        pass.set_bind_group(1, bind_group, &[]);
                        pass.set_bind_group(2, &self.effect_bind_group, &[offset]);
                        draw_bind_group_binds = draw_bind_group_binds.saturating_add(2);
                    }
                }
                bound_bind = Some(state.bind);
            }
            if force_bind && matches!(state.bind, DrawBindKey::None) {
                bound_bind = None;
            }
            if let DrawKind::RRect { first_instance, instance_count } = draw.kind {
                pass.draw(
                    0..6,
                    first_instance..first_instance.saturating_add(instance_count),
                );
            } else if let DrawKind::Image { first_instance, instance_count, .. } = draw.kind {
                pass.draw_indexed(
                    0..6,
                    0,
                    first_instance..first_instance.saturating_add(instance_count),
                );
            } else if let DrawKind::NineSlice { first_instance, instance_count, .. } = draw.kind {
                pass.draw_indexed(
                    0..NINE_SLICE_INDEX_COUNT,
                    0,
                    first_instance..first_instance.saturating_add(instance_count),
                );
            } else if let DrawKind::Spinner { first_instance, instance_count } = draw.kind {
                pass.draw(
                    0..SPINNER_VERTEX_COUNT,
                    first_instance..first_instance.saturating_add(instance_count),
                );
            } else if let DrawKind::NeonMarker { first_instance, instance_count } = draw.kind {
                pass.draw(
                    0..NEON_MARKER_VERTEX_COUNT,
                    first_instance..first_instance.saturating_add(instance_count),
                );
            } else {
                pass.draw_indexed(
                    draw.first_index..draw.first_index + draw.index_count,
                    draw.base_vertex,
                    0..1,
                );
            }
            encoded_draws = encoded_draws.saturating_add(1);
        }
        drop(pass);
        self.stats.draws = self.stats.draws.saturating_add(encoded_draws);
        self.stats.draw_items = self.stats.draw_items.saturating_add(draw_items);
        self.stats.draw_pipeline_binds =
            self.stats.draw_pipeline_binds.saturating_add(pipeline_binds);
        self.stats.draw_bind_group_binds =
            self.stats.draw_bind_group_binds.saturating_add(draw_bind_group_binds);
        self.stats.draw_scissor_sets = self.stats.draw_scissor_sets.saturating_add(scissor_sets);
    }

    fn ensure_present_buffers(&mut self) {
        if self.present_width == self.width
            && self.present_height == self.height
            && self.present_scale == self.scale
        {
            return;
        }
        let vertices = quad_vertices(
            api::RectF::new(
                0.0,
                0.0,
                logical_dimension(self.width, self.scale),
                logical_dimension(self.height, self.scale),
            ),
            0.0,
            0.0,
            1.0,
            1.0,
            api::Color::rgba(1.0, 1.0, 1.0, 1.0),
        );
        let vertex_bytes = vertex4_bytes(&vertices);
        let index_bytes = index6_bytes([0, 1, 2, 2, 1, 3]);
        self.queue.write_buffer(&self.present_vertex_buffer, 0, &vertex_bytes);
        self.queue.write_buffer(&self.present_index_buffer, 0, &index_bytes);
        self.stats.buffer_upload_bytes = self
            .stats
            .buffer_upload_bytes
            .saturating_add(vertex_bytes.len().saturating_add(index_bytes.len()) as u64);
        self.present_width = self.width;
        self.present_height = self.height;
        self.present_scale = self.scale;
    }

    fn render_present(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        surface_view: &wgpu::TextureView,
    ) {
        self.ensure_scene_target();
        let Some(scene_bind_group) = self
            .scene_target
            .as_ref()
            .map(|target| target.bind_group.clone())
        else {
            return;
        };
        self.ensure_present_buffers();
        self.stats.render_passes = self.stats.render_passes.saturating_add(1);
        self.stats.present_passes = self.stats.present_passes.saturating_add(1);
        let timestamp_pair = self.reserve_timestamp_pass(TimestampPassFamily::Present);
        let timestamp_writes = self.timestamp_writes(timestamp_pair);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("oxide-webgpu-present-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: surface_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes,
            occlusion_query_set: None,
        });
        pass.set_pipeline(self.rgba_pipeline());
        pass.set_bind_group(0, &self.viewport_bind_group, &[0]);
        pass.set_bind_group(1, &scene_bind_group, &[]);
        pass.set_vertex_buffer(0, self.present_vertex_buffer.slice(..));
        pass.set_index_buffer(self.present_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..6, 0, 0..1);
    }
}

fn canvas_by_id(id: &str) -> Result<HtmlCanvasElement, api::RenderError> {
    let element = document()?
        .get_element_by_id(id)
        .ok_or(api::RenderError::ResourceNotFound("canvas id not found"))?;
    element
        .dyn_into::<HtmlCanvasElement>()
        .map_err(|_| api::RenderError::InvalidOperation("element is not a canvas"))
}

fn browser_webgpu_present() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let navigator = window.navigator();
    Reflect::get(navigator.as_ref(), &JsValue::from_str("gpu"))
        .ok()
        .filter(|value| !value.is_undefined() && !value.is_null())
        .is_some()
}

fn timestamp_readback_bytes(query_count: u32) -> u64 {
    u64::from(query_count).saturating_mul(u64::from(wgpu::QUERY_SIZE))
}

fn timestamp_sample(data: &[u8], query_index: u32) -> Option<u64> {
    let start = (query_index as usize).checked_mul(wgpu::QUERY_SIZE as usize)?;
    let bytes = data.get(start..start.checked_add(8)?)?;
    Some(u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

fn reserve_webgpu_timestamp_pass(
    timestamps: &mut Option<WebGpuTimestampQueries>,
    family: TimestampPassFamily,
) -> Option<(u32, u32)> {
    timestamps.as_mut().and_then(|timestamps| timestamps.reserve(family))
}

fn webgpu_timestamp_writes(
    timestamps: &Option<WebGpuTimestampQueries>,
    pair: Option<(u32, u32)>,
) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
    let (begin_query, end_query) = pair?;
    let timestamps = timestamps.as_ref()?;
    Some(wgpu::RenderPassTimestampWrites {
        query_set: &timestamps.query_set,
        beginning_of_pass_write_index: Some(begin_query),
        end_of_pass_write_index: Some(end_query),
    })
}

fn create_programs(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    packed_id_mask_fields: bool,
) -> GpuPrograms {
    let viewport_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("oxide-webgpu-viewport-layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: NonZeroU64::new(PREPARED_PROPERTY_UNIFORM_SIZE),
            },
            count: None,
        }],
    });
    let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("oxide-webgpu-texture-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let effect_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("oxide-webgpu-effect-layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let scene3d_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("oxide-webgpu-scene3d-layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let id_mask_raster_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("oxide-webgpu-id-mask-raster-layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: NonZeroU64::new(ID_MASK_RASTER_UNIFORM_SIZE),
            },
            count: None,
        }],
    });
    let id_mask_wide_field_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("oxide-webgpu-id-mask-wide-field-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: NonZeroU64::new(ID_MASK_FIELD_UNIFORM_SIZE),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Uint,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Uint,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    });
    let id_mask_wide_compositor_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("oxide-webgpu-id-mask-wide-compositor-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: NonZeroU64::new(ID_MASK_COMPOSITOR_UNIFORM_SIZE),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("oxide-webgpu-linear-sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("oxide-webgpu-shader"),
        source: wgpu::ShaderSource::Wgsl(WGSL.into()),
    });
    let scene3d_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("oxide-webgpu-scene3d-shader"),
        source: wgpu::ShaderSource::Wgsl(SCENE3D_WGSL.into()),
    });
    let id_mask_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("oxide-webgpu-id-mask-shader"),
        source: wgpu::ShaderSource::Wgsl(ID_MASK_WGSL.into()),
    });
    let id_mask_field_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("oxide-webgpu-id-mask-field-shader"),
        source: wgpu::ShaderSource::Wgsl(ID_MASK_FIELD_WGSL.into()),
    });
    let solid_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("oxide-webgpu-solid-pipeline-layout"),
        bind_group_layouts: &[&viewport_layout],
        push_constant_ranges: &[],
    });
    let texture_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("oxide-webgpu-texture-pipeline-layout"),
        bind_group_layouts: &[&viewport_layout, &texture_layout],
        push_constant_ranges: &[],
    });
    let effect_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("oxide-webgpu-effect-pipeline-layout"),
        bind_group_layouts: &[&viewport_layout, &texture_layout, &effect_layout],
        push_constant_ranges: &[],
    });
    let scene3d_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("oxide-webgpu-scene3d-pipeline-layout"),
        bind_group_layouts: &[&scene3d_layout],
        push_constant_ranges: &[],
    });
    let id_mask_raster_pipeline_layout =
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("oxide-webgpu-id-mask-raster-pipeline-layout"),
            bind_group_layouts: &[&id_mask_raster_layout],
            push_constant_ranges: &[],
        });
    let id_mask_wide_field_pipeline_layout =
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("oxide-webgpu-id-mask-wide-field-pipeline-layout"),
            bind_group_layouts: &[&id_mask_wide_field_layout],
            push_constant_ranges: &[],
        });
    let id_mask_wide_compositor_pipeline_layout =
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("oxide-webgpu-id-mask-wide-compositor-pipeline-layout"),
            bind_group_layouts: &[&id_mask_wide_compositor_layout],
            push_constant_ranges: &[],
        });

    let draw_vertex_layout = vertex_layout();
    let draw_color_target = alpha_color_target(format);
    let solid_pipeline = create_pipeline(
        device,
        &shader,
        &solid_pipeline_layout,
        &draw_vertex_layout,
        &draw_color_target,
        "fs_solid",
    );
    let rrect_instance_layout = rrect_instance_layout();
    let rrect_vertex_layouts = [rrect_instance_layout];
    let rrect_pipeline = create_instanced_pipeline(
        device,
        &shader,
        &solid_pipeline_layout,
        &rrect_vertex_layouts,
        &draw_color_target,
        "vs_rrect",
        "fs_rrect",
        "oxide-webgpu-rrect",
    );
    let image_instance_layout = image_instance_layout();
    let image_vertex_layouts = [image_unit_vertex_layout(), image_instance_layout];
    let image_rgba_pipeline = create_instanced_pipeline(
        device,
        &shader,
        &texture_pipeline_layout,
        &image_vertex_layouts,
        &draw_color_target,
        "vs_image_instance",
        "fs_rgba",
        "oxide-webgpu-image-rgba",
    );
    let image_a8_pipeline = create_instanced_pipeline(
        device,
        &shader,
        &texture_pipeline_layout,
        &image_vertex_layouts,
        &draw_color_target,
        "vs_image_instance",
        "fs_a8",
        "oxide-webgpu-image-a8",
    );
    let image_unit_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("oxide-webgpu-image-unit-vertices"),
        contents: bytemuck::cast_slice(&[
            [0.0_f32, 0.0_f32],
            [1.0_f32, 0.0_f32],
            [0.0_f32, 1.0_f32],
            [1.0_f32, 1.0_f32],
        ]),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let image_unit_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("oxide-webgpu-image-unit-indices"),
        contents: bytemuck::cast_slice(&[0_u16, 1, 2, 2, 1, 3]),
        usage: wgpu::BufferUsages::INDEX,
    });
    let nine_slice_instance_layout = nine_slice_instance_layout();
    let nine_slice_vertex_layouts = [nine_slice_unit_vertex_layout(), nine_slice_instance_layout];
    let nine_slice_rgba_pipeline = create_instanced_pipeline(
        device,
        &shader,
        &texture_pipeline_layout,
        &nine_slice_vertex_layouts,
        &draw_color_target,
        "vs_nine_slice_instance",
        "fs_rgba",
        "oxide-webgpu-nine-slice-rgba",
    );
    let nine_slice_a8_pipeline = create_instanced_pipeline(
        device,
        &shader,
        &texture_pipeline_layout,
        &nine_slice_vertex_layouts,
        &draw_color_target,
        "vs_nine_slice_instance",
        "fs_a8",
        "oxide-webgpu-nine-slice-a8",
    );
    let nine_slice_unit_vertex_buffer =
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("oxide-webgpu-nine-slice-unit-vertices"),
            contents: bytemuck::cast_slice(&NINE_SLICE_UNIT_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });
    let nine_slice_unit_index_buffer =
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("oxide-webgpu-nine-slice-unit-indices"),
            contents: bytemuck::cast_slice(&NINE_SLICE_UNIT_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });
    let spinner_instance_layout = spinner_instance_layout();
    let spinner_vertex_layouts = [spinner_instance_layout];
    let spinner_pipeline = create_instanced_pipeline(
        device,
        &shader,
        &solid_pipeline_layout,
        &spinner_vertex_layouts,
        &draw_color_target,
        "vs_spinner_instance",
        "fs_rrect",
        "oxide-webgpu-spinner",
    );
    let neon_marker_instance_layout = neon_marker_instance_layout();
    let neon_marker_vertex_layouts = [neon_marker_instance_layout];
    let neon_marker_pipeline = create_instanced_pipeline(
        device,
        &shader,
        &solid_pipeline_layout,
        &neon_marker_vertex_layouts,
        &draw_color_target,
        "vs_neon_marker_instance",
        "fs_neon_marker",
        "oxide-webgpu-neon-marker",
    );
    let rgba_pipeline = create_pipeline(
        device,
        &shader,
        &texture_pipeline_layout,
        &draw_vertex_layout,
        &draw_color_target,
        "fs_rgba",
    );
    let a8_pipeline = create_pipeline(
        device,
        &shader,
        &texture_pipeline_layout,
        &draw_vertex_layout,
        &draw_color_target,
        "fs_a8",
    );
    let sdf_pipeline = create_pipeline(
        device,
        &shader,
        &texture_pipeline_layout,
        &draw_vertex_layout,
        &draw_color_target,
        "fs_sdf",
    );
    let effect_pipeline = create_pipeline(
        device,
        &shader,
        &effect_pipeline_layout,
        &draw_vertex_layout,
        &draw_color_target,
        "fs_backdrop",
    );
    let scene3d_vertex_layout = scene3d_color_vertex_layout();
    let scene3d_color_tri_depth_read_pipeline = create_scene3d_pipeline(
        device,
        &scene3d_shader,
        &scene3d_pipeline_layout,
        &scene3d_vertex_layout,
        format,
        Some(wgpu::BlendState::ALPHA_BLENDING),
        true,
        false,
        "oxide-webgpu-scene3d-color-tri-depth-read",
    );
    let scene3d_color_tri_depth_write_pipeline = create_scene3d_pipeline(
        device,
        &scene3d_shader,
        &scene3d_pipeline_layout,
        &scene3d_vertex_layout,
        format,
        Some(wgpu::BlendState::ALPHA_BLENDING),
        true,
        true,
        "oxide-webgpu-scene3d-color-tri-depth-write",
    );
    let scene3d_color_tri_pipeline = create_scene3d_pipeline(
        device,
        &scene3d_shader,
        &scene3d_pipeline_layout,
        &scene3d_vertex_layout,
        format,
        Some(wgpu::BlendState::ALPHA_BLENDING),
        false,
        false,
        "oxide-webgpu-scene3d-color-tri",
    );
    let scene3d_color_tri_add_depth_read_pipeline = create_scene3d_pipeline(
        device,
        &scene3d_shader,
        &scene3d_pipeline_layout,
        &scene3d_vertex_layout,
        format,
        Some(additive_blend_state()),
        true,
        false,
        "oxide-webgpu-scene3d-color-tri-add-depth-read",
    );
    let scene3d_color_tri_add_depth_write_pipeline = create_scene3d_pipeline(
        device,
        &scene3d_shader,
        &scene3d_pipeline_layout,
        &scene3d_vertex_layout,
        format,
        Some(additive_blend_state()),
        true,
        true,
        "oxide-webgpu-scene3d-color-tri-add-depth-write",
    );
    let scene3d_color_tri_add_pipeline = create_scene3d_pipeline(
        device,
        &scene3d_shader,
        &scene3d_pipeline_layout,
        &scene3d_vertex_layout,
        format,
        Some(additive_blend_state()),
        false,
        false,
        "oxide-webgpu-scene3d-color-tri-add",
    );
    let id_mask_vertex_layout = id_mask_raster_vertex_layout();
    let id_mask_raster_pipeline = create_id_mask_raster_pipeline(
        device,
        &id_mask_shader,
        &id_mask_raster_pipeline_layout,
        &id_mask_vertex_layout,
    );
    let id_mask_wide_field_seed_pipeline = create_id_mask_field_pipeline(
        device,
        &id_mask_field_shader,
        &id_mask_wide_field_pipeline_layout,
        "fs_id_mask_field_seed",
        "oxide-webgpu-id-mask-wide-field-seed",
        ID_MASK_WIDE_FIELD_FORMAT,
        false,
    );
    let id_mask_wide_field_jump_pipeline = create_id_mask_field_pipeline(
        device,
        &id_mask_field_shader,
        &id_mask_wide_field_pipeline_layout,
        "fs_id_mask_field_jump",
        "oxide-webgpu-id-mask-wide-field-jump",
        ID_MASK_WIDE_FIELD_FORMAT,
        false,
    );
    let id_mask_wide_compositor_pipeline = create_id_mask_compositor_pipeline(
        device,
        &id_mask_shader,
        &id_mask_wide_compositor_pipeline_layout,
        format,
        "fs_id_mask_compositor",
        "oxide-webgpu-id-mask-wide-compositor",
    );
    let id_mask_wide = IdMaskVariantPrograms {
        field_layout: id_mask_wide_field_layout,
        compositor_layout: id_mask_wide_compositor_layout,
        field_seed_pipeline: id_mask_wide_field_seed_pipeline,
        field_jump_pipeline: id_mask_wide_field_jump_pipeline,
        compositor_pipeline: id_mask_wide_compositor_pipeline,
    };
    let id_mask_packed = packed_id_mask_fields.then(|| {
        create_packed_id_mask_programs(device, &id_mask_field_shader, &id_mask_shader, format)
    });

    GpuPrograms {
        viewport_layout,
        texture_layout,
        effect_layout,
        scene3d_layout,
        id_mask_raster_layout,
        id_mask_wide,
        id_mask_packed,
        solid_pipeline,
        rrect_pipeline,
        image_rgba_pipeline,
        image_a8_pipeline,
        image_unit_vertex_buffer,
        image_unit_index_buffer,
        nine_slice_rgba_pipeline,
        nine_slice_a8_pipeline,
        nine_slice_unit_vertex_buffer,
        nine_slice_unit_index_buffer,
        spinner_pipeline,
        neon_marker_pipeline,
        rgba_pipeline,
        a8_pipeline,
        sdf_pipeline,
        effect_pipeline,
        scene3d_color_tri_depth_read_pipeline,
        scene3d_color_tri_depth_write_pipeline,
        scene3d_color_tri_pipeline,
        scene3d_color_tri_add_depth_read_pipeline,
        scene3d_color_tri_add_depth_write_pipeline,
        scene3d_color_tri_add_pipeline,
        id_mask_raster_pipeline,
        sampler,
    }
}

fn alpha_color_target(format: wgpu::TextureFormat) -> [Option<wgpu::ColorTargetState>; 1] {
    [Some(wgpu::ColorTargetState {
        format,
        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
        write_mask: wgpu::ColorWrites::ALL,
    })]
}

fn create_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    vertex_layout: &wgpu::VertexBufferLayout<'_>,
    color_target: &[Option<wgpu::ColorTargetState>],
    fragment: &'static str,
) -> wgpu::RenderPipeline {
    let vertex_buffers = [vertex_layout.clone()];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(fragment),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &vertex_buffers,
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment),
            compilation_options: Default::default(),
            targets: color_target,
        }),
        multiview: None,
        cache: None,
    })
}

fn create_instanced_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    vertex_buffers: &[wgpu::VertexBufferLayout<'_>],
    color_target: &[Option<wgpu::ColorTargetState>],
    vertex: &'static str,
    fragment: &'static str,
    label: &'static str,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vertex),
            compilation_options: Default::default(),
            buffers: vertex_buffers,
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment),
            compilation_options: Default::default(),
            targets: color_target,
        }),
        multiview: None,
        cache: None,
    })
}

fn create_scene3d_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    vertex_layout: &wgpu::VertexBufferLayout<'_>,
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
    depth_test: bool,
    depth_write: bool,
    label: &'static str,
) -> wgpu::RenderPipeline {
    let vertex_buffers = [vertex_layout.clone()];
    let color_target =
        [Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_scene3d_color"),
            compilation_options: Default::default(),
            buffers: &vertex_buffers,
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: SCENE3D_DEPTH_FORMAT,
            depth_write_enabled: depth_write,
            depth_compare: if depth_test {
                wgpu::CompareFunction::LessEqual
            } else {
                wgpu::CompareFunction::Always
            },
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_scene3d_color"),
            compilation_options: Default::default(),
            targets: &color_target,
        }),
        multiview: None,
        cache: None,
    })
}

fn create_id_mask_raster_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    vertex_layout: &wgpu::VertexBufferLayout<'_>,
) -> wgpu::RenderPipeline {
    let vertex_buffers = [vertex_layout.clone()];
    let color_targets = [
        Some(wgpu::ColorTargetState {
            format: wgpu::TextureFormat::R8Uint,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        }),
        Some(wgpu::ColorTargetState {
            format: wgpu::TextureFormat::R8Uint,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        }),
    ];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("oxide-webgpu-id-mask-raster"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_id_mask_raster"),
            compilation_options: Default::default(),
            buffers: &vertex_buffers,
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_id_mask_raster"),
            compilation_options: Default::default(),
            targets: &color_targets,
        }),
        multiview: None,
        cache: None,
    })
}

fn create_id_mask_compositor_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    fragment: &'static str,
    label: &'static str,
) -> wgpu::RenderPipeline {
    let color_target = [Some(wgpu::ColorTargetState {
        format,
        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
        write_mask: wgpu::ColorWrites::ALL,
    })];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_id_mask_compositor"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment),
            compilation_options: Default::default(),
            targets: &color_target,
        }),
        multiview: None,
        cache: None,
    })
}

fn create_id_mask_field_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    fragment: &'static str,
    label: &'static str,
    format: wgpu::TextureFormat,
    packed: bool,
) -> wgpu::RenderPipeline {
    let packed_target = [Some(wgpu::ColorTargetState {
        format,
        blend: None,
        write_mask: wgpu::ColorWrites::ALL,
    })];
    let wide_targets = [
        Some(wgpu::ColorTargetState {
            format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        }),
        Some(wgpu::ColorTargetState {
            format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        }),
    ];
    let color_targets = if packed { &packed_target[..] } else { &wide_targets[..] };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_id_mask_field"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment),
            compilation_options: Default::default(),
            targets: color_targets,
        }),
        multiview: None,
        cache: None,
    })
}

fn create_packed_id_mask_programs(
    device: &wgpu::Device,
    field_shader: &wgpu::ShaderModule,
    compositor_shader: &wgpu::ShaderModule,
    output_format: wgpu::TextureFormat,
) -> IdMaskVariantPrograms {
    let field_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("oxide-webgpu-id-mask-packed-field-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: NonZeroU64::new(ID_MASK_FIELD_UNIFORM_SIZE),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Uint,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Uint,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Uint,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    });
    let compositor_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("oxide-webgpu-id-mask-packed-compositor-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: NonZeroU64::new(ID_MASK_COMPOSITOR_UNIFORM_SIZE),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Uint,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Uint,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Uint,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    });
    let field_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("oxide-webgpu-id-mask-packed-field-pipeline-layout"),
        bind_group_layouts: &[&field_layout],
        push_constant_ranges: &[],
    });
    let compositor_pipeline_layout =
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("oxide-webgpu-id-mask-packed-compositor-pipeline-layout"),
            bind_group_layouts: &[&compositor_layout],
            push_constant_ranges: &[],
        });
    let field_seed_pipeline = create_id_mask_field_pipeline(
        device,
        field_shader,
        &field_pipeline_layout,
        "fs_id_mask_field_seed_packed",
        "oxide-webgpu-id-mask-packed-field-seed",
        ID_MASK_PACKED_FIELD_FORMAT,
        true,
    );
    let field_jump_pipeline = create_id_mask_field_pipeline(
        device,
        field_shader,
        &field_pipeline_layout,
        "fs_id_mask_field_jump_packed",
        "oxide-webgpu-id-mask-packed-field-jump",
        ID_MASK_PACKED_FIELD_FORMAT,
        true,
    );
    let compositor_pipeline = create_id_mask_compositor_pipeline(
        device,
        compositor_shader,
        &compositor_pipeline_layout,
        output_format,
        "fs_id_mask_compositor_packed",
        "oxide-webgpu-id-mask-packed-compositor",
    );
    IdMaskVariantPrograms {
        field_layout,
        compositor_layout,
        field_seed_pipeline,
        field_jump_pipeline,
        compositor_pipeline,
    }
}

fn additive_blend_state() -> wgpu::BlendState {
    wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::SrcAlpha,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::SrcAlpha,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        },
    }
}

fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] = [
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 8,
            shader_location: 1,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Unorm8x4,
            offset: 16,
            shader_location: 2,
        },
    ];
    wgpu::VertexBufferLayout {
        array_stride: VERTEX_STRIDE,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &ATTRIBUTES,
    }
}

fn rrect_instance_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] = [
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 0,
            shader_location: 0,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 16,
            shader_location: 1,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Unorm8x4,
            offset: 32,
            shader_location: 2,
        },
    ];
    wgpu::VertexBufferLayout {
        array_stride: RRECT_INSTANCE_BYTES as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATTRIBUTES,
    }
}

fn image_instance_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] = [
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 0,
            shader_location: 0,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 16,
            shader_location: 1,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32,
            offset: 32,
            shader_location: 2,
        },
    ];
    wgpu::VertexBufferLayout {
        array_stride: IMAGE_INSTANCE_BYTES as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATTRIBUTES,
    }
}

fn image_unit_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRIBUTES: [wgpu::VertexAttribute; 1] = [wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 0,
        shader_location: 3,
    }];
    wgpu::VertexBufferLayout {
        array_stride: 8,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &ATTRIBUTES,
    }
}

fn nine_slice_instance_layout() -> wgpu::VertexBufferLayout<'static>
{
   const ATTRIBUTES: [wgpu::VertexAttribute; 4] = [
      wgpu::VertexAttribute {
         format: wgpu::VertexFormat::Float32x4,
         offset: 0,
         shader_location: 0,
      },
      wgpu::VertexAttribute {
         format: wgpu::VertexFormat::Float32x2,
         offset: 16,
         shader_location: 1,
      },
      wgpu::VertexAttribute {
         format: wgpu::VertexFormat::Float32x4,
         offset: 24,
         shader_location: 2,
      },
      wgpu::VertexAttribute {
         format: wgpu::VertexFormat::Float32,
         offset: 40,
         shader_location: 3,
      },
   ];
   wgpu::VertexBufferLayout {
      array_stride: NINE_SLICE_INSTANCE_BYTES as wgpu::BufferAddress,
      step_mode: wgpu::VertexStepMode::Instance,
      attributes: &ATTRIBUTES,
   }
}

fn nine_slice_unit_vertex_layout() -> wgpu::VertexBufferLayout<'static>
{
   const ATTRIBUTES: [wgpu::VertexAttribute; 1] = [wgpu::VertexAttribute {
      format: wgpu::VertexFormat::Uint8x4,
      offset: 0,
      shader_location: 4,
   }];
   wgpu::VertexBufferLayout {
      array_stride: 4,
      step_mode: wgpu::VertexStepMode::Vertex,
      attributes: &ATTRIBUTES,
   }
}

fn spinner_instance_layout() -> wgpu::VertexBufferLayout<'static>
{
   const ATTRIBUTES: [wgpu::VertexAttribute; 4] = [
      wgpu::VertexAttribute {
         format: wgpu::VertexFormat::Float32x2,
         offset: 0,
         shader_location: 0,
      },
      wgpu::VertexAttribute {
         format: wgpu::VertexFormat::Float32,
         offset: 8,
         shader_location: 1,
      },
      wgpu::VertexAttribute {
         format: wgpu::VertexFormat::Float32,
         offset: 12,
         shader_location: 2,
      },
      wgpu::VertexAttribute {
         format: wgpu::VertexFormat::Unorm8x4,
         offset: 16,
         shader_location: 3,
      },
   ];
   wgpu::VertexBufferLayout {
      array_stride: SPINNER_INSTANCE_BYTES as wgpu::BufferAddress,
      step_mode: wgpu::VertexStepMode::Instance,
      attributes: &ATTRIBUTES,
   }
}

fn neon_marker_instance_layout() -> wgpu::VertexBufferLayout<'static>
{
   const ATTRIBUTES: [wgpu::VertexAttribute; 6] = [
      wgpu::VertexAttribute {
         format: wgpu::VertexFormat::Float32x2,
         offset: 0,
         shader_location: 0,
      },
      wgpu::VertexAttribute {
         format: wgpu::VertexFormat::Float32x4,
         offset: 8,
         shader_location: 1,
      },
      wgpu::VertexAttribute {
         format: wgpu::VertexFormat::Float32x3,
         offset: 24,
         shader_location: 2,
      },
      wgpu::VertexAttribute {
         format: wgpu::VertexFormat::Unorm8x4,
         offset: 36,
         shader_location: 3,
      },
      wgpu::VertexAttribute {
         format: wgpu::VertexFormat::Unorm8x4,
         offset: 40,
         shader_location: 4,
      },
      wgpu::VertexAttribute {
         format: wgpu::VertexFormat::Float32x4,
         offset: 44,
         shader_location: 5,
      },
   ];
   wgpu::VertexBufferLayout {
      array_stride: NEON_MARKER_INSTANCE_BYTES as wgpu::BufferAddress,
      step_mode: wgpu::VertexStepMode::Instance,
      attributes: &ATTRIBUTES,
   }
}

fn scene3d_color_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] = [
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x3,
            offset: 0,
            shader_location: 0,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x4,
            offset: 12,
            shader_location: 1,
        },
    ];
    wgpu::VertexBufferLayout {
        array_stride: SCENE3D_VERTEX_STRIDE,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &ATTRIBUTES,
    }
}

fn id_mask_raster_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRIBUTES: [wgpu::VertexAttribute; 4] = [
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x3,
            offset: 8,
            shader_location: 1,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint32,
            offset: 24,
            shader_location: 2,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint32,
            offset: 28,
            shader_location: 3,
        },
    ];
    wgpu::VertexBufferLayout {
        array_stride: ID_MASK_VERTEX_STRIDE,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &ATTRIBUTES,
    }
}

fn create_viewport_bind_group(
    device: &wgpu::Device,
    programs: &GpuPrograms,
) -> (wgpu::Buffer, wgpu::BindGroup) {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("oxide-webgpu-viewport-buffer"),
        size: PREPARED_PROPERTY_UNIFORM_SIZE,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("oxide-webgpu-viewport-bind-group"),
        layout: &programs.viewport_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &buffer,
                offset: 0,
                size: NonZeroU64::new(PREPARED_PROPERTY_UNIFORM_SIZE),
            }),
        }],
    });
    (buffer, bind_group)
}

fn create_prepared_property_buffer(device: &wgpu::Device, programs: &GpuPrograms, stride: u64, capacity: usize) -> (wgpu::Buffer, wgpu::BindGroup)
{
   let buffer = device.create_buffer(&wgpu::BufferDescriptor {
      label: Some("oxide-webgpu-prepared-property-ring"),
      size: prepared_property_ring_bytes(stride, capacity),
      usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
      mapped_at_creation: false,
   });
   let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("oxide-webgpu-prepared-property-bind-group"),
      layout: &programs.viewport_layout,
      entries: &[wgpu::BindGroupEntry {
         binding: 0,
         resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
            buffer: &buffer,
            offset: 0,
            size: NonZeroU64::new(PREPARED_PROPERTY_UNIFORM_SIZE),
         }),
      }],
   });
   (buffer, bind_group)
}

fn create_effect_bind_group(
    device: &wgpu::Device,
    programs: &GpuPrograms,
    size: u64,
) -> (wgpu::Buffer, wgpu::BindGroup, u64) {
    let capacity = size.max(EFFECT_UNIFORM_SIZE);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("oxide-webgpu-effect-buffer"),
        size: capacity,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("oxide-webgpu-effect-bind-group"),
        layout: &programs.effect_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &buffer,
                offset: 0,
                size: core::num::NonZeroU64::new(EFFECT_UNIFORM_SIZE),
            }),
        }],
    });
    (buffer, bind_group, capacity)
}

fn align_to(value: u64, alignment: u64) -> u64 {
    if alignment <= 1 {
        return value;
    }
    let rem = value % alignment;
    if rem == 0 {
        value
    } else {
        value.saturating_add(alignment.saturating_sub(rem))
    }
}

fn effect_uniform_needed_bytes(count: usize, stride: u64) -> u64 {
    if count == 0 {
        return EFFECT_UNIFORM_SIZE;
    }
    (count as u64).saturating_sub(1).saturating_mul(stride).saturating_add(EFFECT_UNIFORM_SIZE)
}

fn write_viewport_uniform(
    queue: &wgpu::Queue,
    buffer: &wgpu::Buffer,
    width: u32,
    height: u32,
    scale: f32,
    animation_phase: f32,
) {
    let logical_w = logical_dimension(width, scale).max(1.0);
    let logical_h = logical_dimension(height, scale).max(1.0);
    let values = [
        logical_w, logical_h, 0.0, 0.0,
        1.0, 0.0, 0.0, 1.0,
        0.0, 0.0, 1.0, animation_phase,
    ];
    write_uniform(queue, buffer, values);
}

fn write_uniform(queue: &wgpu::Queue, buffer: &wgpu::Buffer, values: [f32; 12])
{
   queue.write_buffer(buffer, 0, bytemuck::cast_slice(&values));
}

fn create_color_target(
    device: &wgpu::Device,
    programs: &GpuPrograms,
    label: &'static str,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> GpuColorTarget {
    let (texture, view, bind_group) =
        create_target_texture(device, programs, label, format, width, height);
    GpuColorTarget { texture, view, bind_group }
}

fn create_depth_target(
    device: &wgpu::Device,
    label: &'static str,
    width: u32,
    height: u32,
) -> GpuDepthTarget {
    let (texture, view) = create_depth_texture(device, label, width, height);
    GpuDepthTarget { _texture: texture, view }
}

fn create_target_texture(
    device: &wgpu::Device,
    programs: &GpuPrograms,
    label: &'static str,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView, wgpu::BindGroup) {
    let texture = create_texture_2d(
        device,
        label,
        format,
        width,
        height,
        wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = create_texture_bind_group(device, programs, &view, &programs.sampler);
    (texture, view, bind_group)
}

fn create_depth_texture(
    device: &wgpu::Device,
    label: &'static str,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = create_texture_2d(
        device,
        label,
        SCENE3D_DEPTH_FORMAT,
        width,
        height,
        wgpu::TextureUsages::RENDER_ATTACHMENT,
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn create_packed_id_mask_field_targets(
    device: &wgpu::Device,
    programs: &IdMaskVariantPrograms,
    uniform_buffer: &wgpu::Buffer,
    city_view: &wgpu::TextureView,
    neighborhood_view: &wgpu::TextureView,
    width: u32,
    height: u32,
) -> IdMaskFieldTargets {
    let a_texture = create_id_mask_field_texture(
        device,
        "oxide-webgpu-id-mask-packed-field-a",
        ID_MASK_PACKED_FIELD_FORMAT,
        width,
        height,
    );
    let b_texture = create_id_mask_field_texture(
        device,
        "oxide-webgpu-id-mask-packed-field-b",
        ID_MASK_PACKED_FIELD_FORMAT,
        width,
        height,
    );
    let a_view = a_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let b_view = b_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let field_bind_group_a = create_packed_id_mask_field_bind_group(
        device,
        &programs.field_layout,
        uniform_buffer,
        city_view,
        neighborhood_view,
        &a_view,
        "oxide-webgpu-id-mask-packed-field-bind-group-a",
    );
    let field_bind_group_b = create_packed_id_mask_field_bind_group(
        device,
        &programs.field_layout,
        uniform_buffer,
        city_view,
        neighborhood_view,
        &b_view,
        "oxide-webgpu-id-mask-packed-field-bind-group-b",
    );
    let compositor_bind_group_a = create_packed_id_mask_compositor_bind_group(
        device,
        &programs.compositor_layout,
        uniform_buffer,
        city_view,
        neighborhood_view,
        &a_view,
        "oxide-webgpu-id-mask-packed-compositor-bind-group-a",
    );
    let compositor_bind_group_b = create_packed_id_mask_compositor_bind_group(
        device,
        &programs.compositor_layout,
        uniform_buffer,
        city_view,
        neighborhood_view,
        &b_view,
        "oxide-webgpu-id-mask-packed-compositor-bind-group-b",
    );
    IdMaskFieldTargets::Packed {
        a_texture,
        b_texture,
        a_view,
        b_view,
        field_bind_group_a,
        field_bind_group_b,
        compositor_bind_group_a,
        compositor_bind_group_b,
    }
}

fn create_wide_id_mask_field_targets(
    device: &wgpu::Device,
    programs: &IdMaskVariantPrograms,
    uniform_buffer: &wgpu::Buffer,
    city_view: &wgpu::TextureView,
    neighborhood_view: &wgpu::TextureView,
    width: u32,
    height: u32,
) -> IdMaskFieldTargets {
    let city_a_texture = create_id_mask_field_texture(
        device,
        "oxide-webgpu-id-mask-city-field-a",
        ID_MASK_WIDE_FIELD_FORMAT,
        width,
        height,
    );
    let city_b_texture = create_id_mask_field_texture(
        device,
        "oxide-webgpu-id-mask-city-field-b",
        ID_MASK_WIDE_FIELD_FORMAT,
        width,
        height,
    );
    let seam_a_texture = create_id_mask_field_texture(
        device,
        "oxide-webgpu-id-mask-seam-field-a",
        ID_MASK_WIDE_FIELD_FORMAT,
        width,
        height,
    );
    let seam_b_texture = create_id_mask_field_texture(
        device,
        "oxide-webgpu-id-mask-seam-field-b",
        ID_MASK_WIDE_FIELD_FORMAT,
        width,
        height,
    );
    let city_a_view = city_a_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let city_b_view = city_b_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let seam_a_view = seam_a_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let seam_b_view = seam_b_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let field_bind_group_a = create_wide_id_mask_field_bind_group(
        device,
        &programs.field_layout,
        uniform_buffer,
        city_view,
        neighborhood_view,
        &city_a_view,
        &seam_a_view,
        "oxide-webgpu-id-mask-wide-field-bind-group-a",
    );
    let field_bind_group_b = create_wide_id_mask_field_bind_group(
        device,
        &programs.field_layout,
        uniform_buffer,
        city_view,
        neighborhood_view,
        &city_b_view,
        &seam_b_view,
        "oxide-webgpu-id-mask-wide-field-bind-group-b",
    );
    let compositor_bind_group_a = create_wide_id_mask_compositor_bind_group(
        device,
        &programs.compositor_layout,
        uniform_buffer,
        city_view,
        neighborhood_view,
        &city_a_view,
        &seam_a_view,
        "oxide-webgpu-id-mask-wide-compositor-bind-group-a",
    );
    let compositor_bind_group_b = create_wide_id_mask_compositor_bind_group(
        device,
        &programs.compositor_layout,
        uniform_buffer,
        city_view,
        neighborhood_view,
        &city_b_view,
        &seam_b_view,
        "oxide-webgpu-id-mask-wide-compositor-bind-group-b",
    );
    IdMaskFieldTargets::Wide {
        city_a_texture,
        city_b_texture,
        seam_a_texture,
        seam_b_texture,
        city_a_view,
        city_b_view,
        seam_a_view,
        seam_b_view,
        field_bind_group_a,
        field_bind_group_b,
        compositor_bind_group_a,
        compositor_bind_group_b,
    }
}

fn id_mask_target_bytes_per_pixel(packed: bool) -> u64 {
    2 + if packed {
        2 * color_texture_bytes_per_pixel(ID_MASK_PACKED_FIELD_FORMAT)
    } else {
        4 * color_texture_bytes_per_pixel(ID_MASK_WIDE_FIELD_FORMAT)
    }
}

fn id_mask_render_targets_bytes(width: u32, height: u32, packed: bool) -> u64 {
    saturating_texture_bytes(
        u64::from(width),
        u64::from(height),
        id_mask_target_bytes_per_pixel(packed),
    )
}

fn id_mask_final_fields_are_a(width: u32, height: u32) -> bool {
    let mut src_is_a = true;
    let mut jump = width.max(height).max(1).next_power_of_two() / 2;
    while jump >= 1 {
        src_is_a = !src_is_a;
        jump /= 2;
    }
    src_is_a
}

fn rebuild_id_mask_target_bind_groups(
    device: &wgpu::Device,
    programs: &GpuPrograms,
    uniform_buffer: &wgpu::Buffer,
    targets: &mut IdMaskRenderTargets,
) {
    match &mut targets.fields {
        IdMaskFieldTargets::Packed {
            a_view,
            b_view,
            field_bind_group_a,
            field_bind_group_b,
            compositor_bind_group_a,
            compositor_bind_group_b,
            ..
        } => {
            let Some(programs) = programs.id_mask_packed.as_ref() else { return };
            *field_bind_group_a = create_packed_id_mask_field_bind_group(
                device,
                &programs.field_layout,
                uniform_buffer,
                &targets.city_view,
                &targets.neighborhood_view,
                a_view,
                "oxide-webgpu-id-mask-packed-field-bind-group-a",
            );
            *field_bind_group_b = create_packed_id_mask_field_bind_group(
                device,
                &programs.field_layout,
                uniform_buffer,
                &targets.city_view,
                &targets.neighborhood_view,
                b_view,
                "oxide-webgpu-id-mask-packed-field-bind-group-b",
            );
            *compositor_bind_group_a = create_packed_id_mask_compositor_bind_group(
                device,
                &programs.compositor_layout,
                uniform_buffer,
                &targets.city_view,
                &targets.neighborhood_view,
                a_view,
                "oxide-webgpu-id-mask-packed-compositor-bind-group-a",
            );
            *compositor_bind_group_b = create_packed_id_mask_compositor_bind_group(
                device,
                &programs.compositor_layout,
                uniform_buffer,
                &targets.city_view,
                &targets.neighborhood_view,
                b_view,
                "oxide-webgpu-id-mask-packed-compositor-bind-group-b",
            );
        }
        IdMaskFieldTargets::Wide {
            city_a_view,
            city_b_view,
            seam_a_view,
            seam_b_view,
            field_bind_group_a,
            field_bind_group_b,
            compositor_bind_group_a,
            compositor_bind_group_b,
            ..
        } => {
            let programs = &programs.id_mask_wide;
            *field_bind_group_a = create_wide_id_mask_field_bind_group(
                device,
                &programs.field_layout,
                uniform_buffer,
                &targets.city_view,
                &targets.neighborhood_view,
                city_a_view,
                seam_a_view,
                "oxide-webgpu-id-mask-wide-field-bind-group-a",
            );
            *field_bind_group_b = create_wide_id_mask_field_bind_group(
                device,
                &programs.field_layout,
                uniform_buffer,
                &targets.city_view,
                &targets.neighborhood_view,
                city_b_view,
                seam_b_view,
                "oxide-webgpu-id-mask-wide-field-bind-group-b",
            );
            *compositor_bind_group_a = create_wide_id_mask_compositor_bind_group(
                device,
                &programs.compositor_layout,
                uniform_buffer,
                &targets.city_view,
                &targets.neighborhood_view,
                city_a_view,
                seam_a_view,
                "oxide-webgpu-id-mask-wide-compositor-bind-group-a",
            );
            *compositor_bind_group_b = create_wide_id_mask_compositor_bind_group(
                device,
                &programs.compositor_layout,
                uniform_buffer,
                &targets.city_view,
                &targets.neighborhood_view,
                city_b_view,
                seam_b_view,
                "oxide-webgpu-id-mask-wide-compositor-bind-group-b",
            );
        }
    }
}

fn create_id_mask_texture(
    device: &wgpu::Device,
    label: &'static str,
    width: u32,
    height: u32,
) -> wgpu::Texture {
    let mut usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
    #[cfg(feature = "snapshot-tests")]
    {
        usage |= wgpu::TextureUsages::COPY_SRC;
    }
    create_texture_2d(
        device,
        label,
        wgpu::TextureFormat::R8Uint,
        width,
        height,
        usage,
    )
}

fn create_id_mask_field_texture(
    device: &wgpu::Device,
    label: &'static str,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> wgpu::Texture {
    create_texture_2d(
        device,
        label,
        format,
        width,
        height,
        id_mask_field_texture_usage(),
    )
}

#[cfg(feature = "snapshot-tests")]
fn create_id_mask_readback_plane(
    device: &wgpu::Device,
    label: &'static str,
    width: u32,
    height: u32,
    bytes_per_pixel: u32,
) -> IdMaskReadbackPlane
{
    let packed_row_bytes = width.saturating_mul(bytes_per_pixel);
    let alignment = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_row_bytes = packed_row_bytes.div_ceil(alignment).saturating_mul(alignment);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: u64::from(padded_row_bytes).saturating_mul(u64::from(height)),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    IdMaskReadbackPlane { buffer, padded_row_bytes, packed_row_bytes }
}

#[cfg(feature = "snapshot-tests")]
fn copy_id_mask_texture_to_plane(
    encoder: &mut wgpu::CommandEncoder,
    texture: &wgpu::Texture,
    plane: &IdMaskReadbackPlane,
    width: u32,
    height: u32,
)
{
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &plane.buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(plane.padded_row_bytes),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
}

#[cfg(feature = "snapshot-tests")]
fn read_id_mask_plane(plane: &IdMaskReadbackPlane, height: u32) -> Vec<u8>
{
    let mapped = plane.buffer.slice(..).get_mapped_range();
    let mut packed =
        Vec::with_capacity(plane.packed_row_bytes as usize * height as usize);
    for row in mapped.chunks_exact(plane.padded_row_bytes as usize).take(height as usize)
    {
        packed.extend_from_slice(&row[..plane.packed_row_bytes as usize]);
    }
    drop(mapped);
    packed
}

#[cfg(feature = "snapshot-tests")]
fn decode_web_rgba16_float(bytes: &[u8]) -> Vec<[f32; 4]>
{
    bytes
        .chunks_exact(8)
        .map(|pixel| {
            [
                half_to_f32(u16::from_le_bytes([pixel[0], pixel[1]])),
                half_to_f32(u16::from_le_bytes([pixel[2], pixel[3]])),
                half_to_f32(u16::from_le_bytes([pixel[4], pixel[5]])),
                half_to_f32(u16::from_le_bytes([pixel[6], pixel[7]])),
            ]
        })
        .collect()
}

#[cfg(feature = "snapshot-tests")]
fn decode_web_rgba16_uint_fields(
    bytes: &[u8],
    city: &[u8],
    neighborhood: &[u8],
    width: u32,
    height: u32,
) -> (Vec<[f32; 4]>, Vec<[f32; 4]>)
{
    let pixel_count = width as usize * height as usize;
    let mut city_field = Vec::with_capacity(pixel_count);
    let mut seam_field = Vec::with_capacity(pixel_count);
    for pixel in bytes.chunks_exact(8).take(pixel_count)
    {
        let coordinates = [
            u16::from_le_bytes([pixel[0], pixel[1]]),
            u16::from_le_bytes([pixel[2], pixel[3]]),
            u16::from_le_bytes([pixel[4], pixel[5]]),
            u16::from_le_bytes([pixel[6], pixel[7]]),
        ];
        city_field.push(decode_web_packed_seed(
            coordinates[0],
            coordinates[1],
            city,
            neighborhood,
            width,
            height,
            false,
        ));
        seam_field.push(decode_web_packed_seed(
            coordinates[2],
            coordinates[3],
            city,
            neighborhood,
            width,
            height,
            true,
        ));
    }
    (city_field, seam_field)
}

#[cfg(feature = "snapshot-tests")]
fn decode_web_packed_seed(
    x: u16,
    y: u16,
    city: &[u8],
    neighborhood: &[u8],
    width: u32,
    height: u32,
    seam: bool,
) -> [f32; 4]
{
    if x == ID_MASK_PACKED_INVALID || y == ID_MASK_PACKED_INVALID
    {
        return [-1.0, -1.0, 0.0, 0.0];
    }
    let x = u32::from(x);
    let y = u32::from(y);
    if x >= width || y >= height
    {
        return [-1.0, -1.0, 0.0, 0.0];
    }
    let index = y as usize * width as usize + x as usize;
    let seed_city = match city.get(index)
    {
        Some(seed_city) => *seed_city,
        None => 0,
    };
    if seed_city == 0
    {
        return [-1.0, -1.0, 0.0, 0.0];
    }
    let seed_neighborhood = if seam {
        1
    } else {
        match neighborhood.get(index)
        {
            Some(seed_neighborhood) => *seed_neighborhood,
            None => 0,
        }
    };
    [x as f32, y as f32, f32::from(seed_city), f32::from(seed_neighborhood)]
}

#[cfg(feature = "snapshot-tests")]
fn half_to_f32(bits: u16) -> f32
{
    let sign = u32::from(bits & 0x8000) << 16;
    let exponent = u32::from((bits >> 10) & 0x1f);
    let mantissa = u32::from(bits & 0x03ff);
    let value = match exponent
    {
        0 => {
            if mantissa == 0
            {
                sign
            }
            else
            {
                let shift = mantissa.leading_zeros().saturating_sub(21);
                let normalized = (mantissa << shift) & 0x03ff;
                let exponent = 113_u32.saturating_sub(shift);
                sign | (exponent << 23) | (normalized << 13)
            }
        }
        0x1f => sign | 0x7f80_0000 | (mantissa << 13),
        _ => sign | ((exponent + 112) << 23) | (mantissa << 13),
    };
    f32::from_bits(value)
}

fn create_texture_2d(
    device: &wgpu::Device,
    label: &'static str,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

fn create_texture_bind_group(
    device: &wgpu::Device,
    programs: &GpuPrograms,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("oxide-webgpu-texture-bind-group"),
        layout: &programs.texture_layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
        ],
    })
}

fn create_wide_id_mask_field_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    city_view: &wgpu::TextureView,
    neighborhood_view: &wgpu::TextureView,
    city_field_view: &wgpu::TextureView,
    seam_field_view: &wgpu::TextureView,
    label: &'static str,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_binding(uniform_buffer, ID_MASK_FIELD_UNIFORM_SIZE),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(city_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(neighborhood_view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(city_field_view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(seam_field_view),
            },
        ],
    })
}

fn create_wide_id_mask_compositor_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    city_view: &wgpu::TextureView,
    neighborhood_view: &wgpu::TextureView,
    city_field_view: &wgpu::TextureView,
    seam_field_view: &wgpu::TextureView,
    label: &'static str,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_binding(uniform_buffer, ID_MASK_COMPOSITOR_UNIFORM_SIZE),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(city_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(neighborhood_view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(city_field_view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(seam_field_view),
            },
        ],
    })
}

fn create_packed_id_mask_field_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    city_view: &wgpu::TextureView,
    neighborhood_view: &wgpu::TextureView,
    field_view: &wgpu::TextureView,
    label: &'static str,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_binding(uniform_buffer, ID_MASK_FIELD_UNIFORM_SIZE),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(city_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(neighborhood_view),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(field_view),
            },
        ],
    })
}

fn create_packed_id_mask_compositor_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    city_view: &wgpu::TextureView,
    neighborhood_view: &wgpu::TextureView,
    field_view: &wgpu::TextureView,
    label: &'static str,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_binding(uniform_buffer, ID_MASK_COMPOSITOR_UNIFORM_SIZE),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(city_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(neighborhood_view),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(field_view),
            },
        ],
    })
}

fn uniform_binding(buffer: &wgpu::Buffer, size: u64) -> wgpu::BindingResource<'_> {
    wgpu::BindingResource::Buffer(wgpu::BufferBinding {
        buffer,
        offset: 0,
        size: NonZeroU64::new(size),
    })
}

fn create_or_update_prepared_buffer(
   device: &wgpu::Device,
   queue: &wgpu::Queue,
   label: &'static str,
   bytes: &[u8],
   usage: wgpu::BufferUsages,
   reusable: Option<wgpu::Buffer>,
) -> (wgpu::Buffer, bool)
{
   let needed = bytes.len().max(1) as u64;
   if let Some(buffer) = reusable.filter(|buffer| buffer.size() >= needed)
   {
      if !bytes.is_empty()
      {
         queue.write_buffer(&buffer, 0, bytes);
      }
      return (buffer, false);
   }
   let buffer = device.create_buffer(&wgpu::BufferDescriptor {
      label: Some(label),
      size: needed,
      usage: usage | wgpu::BufferUsages::COPY_DST,
      mapped_at_creation: false,
   });
   if !bytes.is_empty()
   {
      queue.write_buffer(&buffer, 0, bytes);
   }
   (buffer, true)
}

fn ensure_buffer(
    device: &wgpu::Device,
    buffer: &mut Option<wgpu::Buffer>,
    capacity: &mut u64,
    needed: u64,
    usage: wgpu::BufferUsages,
    label: &'static str,
) -> bool {
    if needed == 0 {
        return false;
    }
    if buffer.is_some() && *capacity >= needed {
        return false;
    }
    let next = needed.next_power_of_two().max(1024);
    *buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: next,
        usage,
        mapped_at_creation: false,
    }));
    *capacity = next;
    true
}

fn set_scissor(
    pass: &mut wgpu::RenderPass<'_>,
    clip: api::RectI,
    scale: f32,
    origin: [f32; 2],
    transform: [f32; 6],
    width: u32,
    height: u32,
) -> bool {
    let scale = f64::from(sanitize_scale(scale));
    let source_x0 = f64::from(clip.x);
    let source_y0 = f64::from(clip.y);
    let source_x1 = f64::from(clip.x.saturating_add(clip.w));
    let source_y1 = f64::from(clip.y.saturating_add(clip.h));
    let [m11, m12, m21, m22, tx, ty] = transform.map(f64::from);
    let points = [
        [m11 * source_x0 + m21 * source_y0 + tx, m12 * source_x0 + m22 * source_y0 + ty],
        [m11 * source_x1 + m21 * source_y0 + tx, m12 * source_x1 + m22 * source_y0 + ty],
        [m11 * source_x0 + m21 * source_y1 + tx, m12 * source_x0 + m22 * source_y1 + ty],
        [m11 * source_x1 + m21 * source_y1 + tx, m12 * source_x1 + m22 * source_y1 + ty],
    ];
    let transformed_x0 = points.iter().map(|point| point[0]).fold(f64::INFINITY, f64::min).floor();
    let transformed_y0 = points.iter().map(|point| point[1]).fold(f64::INFINITY, f64::min).floor();
    let transformed_x1 = points.iter().map(|point| point[0]).fold(f64::NEG_INFINITY, f64::max).ceil();
    let transformed_y1 = points.iter().map(|point| point[1]).fold(f64::NEG_INFINITY, f64::max).ceil();
    // Preserve the established canvas scissor contract: clamp a negative origin without
    // shortening the requested extent, then translate that clipped span into target space.
    let clip_x0 = transformed_x0.max(0.0);
    let clip_y0 = transformed_y0.max(0.0);
    let clip_x1 = clip_x0 + (transformed_x1 - transformed_x0).max(0.0);
    let clip_y1 = clip_y0 + (transformed_y1 - transformed_y0).max(0.0);
    let x0 = ((clip_x0 - f64::from(origin[0])) * scale).floor().max(0.0);
    let y0 = ((clip_y0 - f64::from(origin[1])) * scale).floor().max(0.0);
    let x1 = ((clip_x1 - f64::from(origin[0])) * scale).ceil().min(f64::from(width));
    let y1 = ((clip_y1 - f64::from(origin[1])) * scale).ceil().min(f64::from(height));
    if x1 <= x0 || y1 <= y0 {
        return false;
    }
    let x = x0 as u32;
    let y = y0 as u32;
    pass.set_scissor_rect(x, y, (x1 as u32).saturating_sub(x), (y1 as u32).saturating_sub(y));
    true
}

fn set_viewport_and_scissor_rect(
    pass: &mut wgpu::RenderPass<'_>,
    rect: api::RectF,
    scale: f32,
    width: u32,
    height: u32,
) {
    let scale = sanitize_scale(scale);
    let x = (rect.x.max(0.0) * scale).floor();
    let y = (rect.y.max(0.0) * scale).floor();
    let w = (rect.w.max(0.0) * scale).ceil();
    let h = (rect.h.max(0.0) * scale).ceil();
    let x = x.min(width.saturating_sub(1) as f32);
    let y = y.min(height.saturating_sub(1) as f32);
    let w = w.min(width as f32 - x).max(1.0);
    let h = h.min(height as f32 - y).max(1.0);
    pass.set_viewport(x, y, w, h, 0.0, 1.0);
    pass.set_scissor_rect(x as u32, y as u32, w as u32, h as u32);
}

fn quad_vertices(
    rect: api::RectF,
    u0: f32,
    v0: f32,
    u1: f32,
    v1: f32,
    color: api::Color,
) -> [PackedVertex; 4] {
    let rgba = color.pack_rgba8();
    [
        PackedVertex::new(rect.x, rect.y, u0, v0, rgba),
        PackedVertex::new(rect.x + rect.w, rect.y, u1, v0, rgba),
        PackedVertex::new(rect.x, rect.y + rect.h, u0, v1, rgba),
        PackedVertex::new(rect.x + rect.w, rect.y + rect.h, u1, v1, rgba),
    ]
}

fn gpu_vertex(x: f32, y: f32, u: f32, v: f32, rgba: u32, uniform: api::Color) -> PackedVertex {
    PackedVertex::new(x, y, u, v, if rgba == 0 { uniform.pack_rgba8() } else { rgba })
}

fn append_gpu_vertices(
    out: &mut Vec<PackedVertex>,
    idx: &mut Vec<u32>,
    vertices: &[api::Vertex],
    color: api::Color,
    preserve_vertex_color: bool,
) {
    let base = out.len() as u32;
    let inherited = color.pack_rgba8();
    out.extend(
        vertices.iter().map(|vertex| {
            PackedVertex::new(
                vertex.x,
                vertex.y,
                vertex.u,
                vertex.v,
                if preserve_vertex_color && vertex.rgba != 0 { vertex.rgba } else { inherited },
            )
        }),
    );
    if vertices.len() == 4 {
        idx.extend_from_slice(&[base, base + 1, base + 2, base + 2, base + 1, base + 3]);
    } else {
        idx.extend(base..out.len() as u32);
    }
}

fn append_indexed_gpu_vertices(
    out: &mut Vec<PackedVertex>,
    idx: &mut Vec<u32>,
    vertices: &[api::Vertex],
    indices: &[u16],
    mode: NormalizedIndexMode,
    color: api::Color,
    preserve_vertex_color: bool,
) -> bool {
    for index in indices {
        let Some(local_index) = resolve_index(*index, mode) else {
            return false;
        };
        if local_index >= vertices.len() {
            return false;
        }
    }

    let base = out.len() as u32;
    let inherited = color.pack_rgba8();
    out.extend(
        vertices.iter().map(|vertex| {
            PackedVertex::new(
                vertex.x,
                vertex.y,
                vertex.u,
                vertex.v,
                if preserve_vertex_color && vertex.rgba != 0 { vertex.rgba } else { inherited },
            )
        }),
    );
    for index in indices {
        if let Some(local_index) = resolve_index(*index, mode) {
            idx.push(base.saturating_add(local_index as u32));
        }
    }
    true
}

fn append_local_indexed_gpu_vertices(
    out: &mut Vec<PackedVertex>,
    idx: &mut Vec<u32>,
    vertices: &[api::Vertex],
    indices: &[u16],
    color: api::Color,
    preserve_vertex_color: bool,
) -> bool {
    for index in indices {
        if *index as usize >= vertices.len() {
            return false;
        }
    }

    let base = out.len() as u32;
    let inherited = color.pack_rgba8();
    out.extend(
        vertices.iter().map(|vertex| {
            PackedVertex::new(
                vertex.x,
                vertex.y,
                vertex.u,
                vertex.v,
                if preserve_vertex_color && vertex.rgba != 0 { vertex.rgba } else { inherited },
            )
        }),
    );
    idx.extend(indices.iter().map(|index| base.saturating_add(*index as u32)));
    true
}

fn skip_layer_body(list: &api::DrawList, index: &mut usize) -> u32 {
    let mut depth = 1_u32;
    let mut skipped = 0_u32;
    while *index < list.items.len() && depth > 0 {
        match list.items[*index] {
            api::DrawCmd::LayerBegin { .. } => depth = depth.saturating_add(1),
            api::DrawCmd::LayerEnd => depth = depth.saturating_sub(1),
            _ => skipped = skipped.saturating_add(1),
        }
        *index += 1;
    }
    skipped
}

fn logical_dimension(physical: u32, scale: f32) -> f32 {
    physical as f32 / sanitize_scale(scale)
}

fn scene3d_color_vertex_bytes(vertices: &[scene3d::VertexColor3d]) -> Vec<u8> {
    let mut out = Vec::with_capacity(vertices.len().saturating_mul(SCENE3D_VERTEX_STRIDE as usize));
    for vertex in vertices {
        for value in vertex.position {
            push_f32(&mut out, value);
        }
        for value in vertex.color {
            push_f32(&mut out, value);
        }
    }
    out
}

fn scene3d_index_bytes(indices: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(indices.len().saturating_mul(4));
    for index in indices {
        out.extend_from_slice(&index.to_le_bytes());
    }
    out
}

fn write_id_mask_raster_vertex_bytes(
    vertices: &[id_mask_compositor::IdMaskRasterVertex],
    out: &mut Vec<u8>,
) {
    out.clear();
    out.reserve(vertices.len().saturating_mul(ID_MASK_VERTEX_STRIDE as usize));
    for vertex in vertices {
        push_f32(out, vertex.position_px[0]);
        push_f32(out, vertex.position_px[1]);
        push_f32(out, vertex.position_world[0]);
        push_f32(out, vertex.position_world[1]);
        push_f32(out, vertex.position_world[2]);
        push_f32(out, vertex.position_world[3]);
        out.extend_from_slice(&vertex.city_id.to_le_bytes());
        out.extend_from_slice(&vertex.neighborhood_id.to_le_bytes());
    }
}

fn ensure_id_mask_vertex_cache_uploaded(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    stats: &mut WebRendererStats,
    cache: &mut IdMaskVertexCache,
) -> Option<u64> {
    let needed = cache.bytes.len().max(1) as u64;
    if cache.buffer.is_none() || cache.buffer_capacity < needed {
        let capacity = needed.next_power_of_two();
        cache.buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("oxide-webgpu-id-mask-raster-chunk-vertices"),
            size: capacity,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
        cache.buffer_capacity = capacity;
        cache.uploaded = false;
        stats.buffer_grows = stats.buffer_grows.saturating_add(1);
        stats.id_mask_buffer_grows = stats.id_mask_buffer_grows.saturating_add(1);
    }
    if cache.uploaded {
        return Some(0);
    }
    let buffer = cache.buffer.as_ref()?;
    queue.write_buffer(buffer, 0, &cache.bytes);
    cache.uploaded = true;
    Some(cache.bytes.len() as u64)
}

fn write_id_mask_raster_uniform_bytes(
    out: &mut Vec<u8>,
    width: u32,
    height: u32,
    projection: id_mask_compositor::IdMaskRasterProjection,
) {
    let start = out.len();
    out.reserve(ID_MASK_RASTER_UNIFORM_SIZE_BYTES);
    for value in [
        width as f32,
        height as f32,
        if projection.use_world_position { 1.0 } else { 0.0 },
        if projection.visible_hemisphere { 1.0 } else { 0.0 },
    ] {
        push_f32(out, value);
    }
    for column in projection.world_to_clip {
        for value in column {
            push_f32(out, value);
        }
    }
    for column in projection.model_to_world {
        for value in column {
            push_f32(out, value);
        }
    }
    for value in [
        projection.camera_eye_unit[0],
        projection.camera_eye_unit[1],
        projection.camera_eye_unit[2],
        projection.visible_front_min,
    ] {
        push_f32(out, value);
    }
    for value in
        [projection.normal_scale[0], projection.normal_scale[1], projection.normal_scale[2], 0.0]
    {
        push_f32(out, value);
    }
    debug_assert_eq!(out.len() - start, ID_MASK_RASTER_UNIFORM_SIZE_BYTES);
}

fn id_mask_field_uniform_bytes(
    width: u32,
    height: u32,
    jump_px: f32,
) -> [u8; ID_MASK_FIELD_UNIFORM_SIZE_BYTES] {
    let mut out = [0_u8; ID_MASK_FIELD_UNIFORM_SIZE_BYTES];
    out[0..4].copy_from_slice(&(width as f32).to_le_bytes());
    out[4..8].copy_from_slice(&(height as f32).to_le_bytes());
    out[8..12].copy_from_slice(&jump_px.to_le_bytes());
    out[12..16].copy_from_slice(&0.0_f32.to_le_bytes());
    out
}

fn write_id_mask_compositor_uniform_bytes(out: &mut Vec<u8>, draw: &IdMaskDraw) {
    let start = out.len();
    out.reserve(ID_MASK_COMPOSITOR_UNIFORM_SIZE_BYTES);
    for value in [draw.viewport.x, draw.viewport.y, draw.viewport.w, draw.viewport.h] {
        push_f32(out, value);
    }
    for value in [
        draw.mask_width as f32,
        draw.mask_height as f32,
        draw.mask_scale.max(1.0),
        draw.darken_background_alpha.clamp(0.0, 1.0),
    ] {
        push_f32(out, value);
    }
    for value in [
        draw.mode as u32 as f32,
        if draw.glow_enabled { 1.0 } else { 0.0 },
        draw.polish.smooth_radius_px.max(0.0),
        draw.polish.fallback_radius_px.max(0.0),
    ] {
        push_f32(out, value);
    }
    for value in [
        draw.polish.exterior_halo_inner_sigma_px.max(0.0),
        draw.polish.exterior_halo_inner_alpha.max(0.0),
        draw.polish.exterior_halo_outer_sigma_px.max(0.0),
        draw.polish.exterior_halo_outer_alpha.max(0.0),
    ] {
        push_f32(out, value);
    }
    for style in draw.city_styles {
        push_f32(out, style.fill_rgb[0]);
        push_f32(out, style.fill_rgb[1]);
        push_f32(out, style.fill_rgb[2]);
        push_f32(out, 1.0);
    }
    for style in draw.city_styles {
        push_f32(out, style.edge_rgb[0]);
        push_f32(out, style.edge_rgb[1]);
        push_f32(out, style.edge_rgb[2]);
        push_f32(out, 1.0);
    }
    for style in draw.city_styles {
        push_f32(out, style.seam_rgb[0]);
        push_f32(out, style.seam_rgb[1]);
        push_f32(out, style.seam_rgb[2]);
        push_f32(out, 1.0);
    }
    for rgb in draw.neighborhood_colors {
        push_f32(out, rgb[0]);
        push_f32(out, rgb[1]);
        push_f32(out, rgb[2]);
        push_f32(out, 1.0);
    }
    debug_assert_eq!(out.len() - start, ID_MASK_COMPOSITOR_UNIFORM_SIZE_BYTES);
}

fn push_scene3d_uniform(out: &mut Vec<u8>, mvp: scene3d::Mat4, color: api::Color) -> u32 {
    let aligned = align_usize(out.len(), SCENE3D_UNIFORM_STRIDE);
    if out.len() < aligned {
        out.resize(aligned, 0);
    }
    let offset = out.len();
    for column in mvp {
        for value in column {
            push_f32(out, value);
        }
    }
    push_f32(out, color.r);
    push_f32(out, color.g);
    push_f32(out, color.b);
    push_f32(out, color.a);
    out.resize(offset + SCENE3D_UNIFORM_STRIDE, 0);
    offset as u32
}

fn align_usize(value: usize, alignment: usize) -> usize {
    let mask = alignment - 1;
    (value + mask) & !mask
}

fn align_uniform_bytes(out: &mut Vec<u8>, alignment: usize) -> u32 {
    let offset = align_usize(out.len(), alignment);
    out.resize(offset, 0);
    u32::try_from(offset).expect("ID-mask uniform arena exceeds dynamic-offset range")
}

fn f32x4_bytes(values: [f32; 4]) -> [u8; 16] {
    let mut out = [0; 16];
    let mut offset = 0;
    for value in values {
        write_f32(&mut out, &mut offset, value);
    }
    out
}

fn vertex4_bytes(vertices: &[PackedVertex; 4]) -> [u8; VERTEX_STRIDE_BYTES * 4] {
    let mut out = [0; VERTEX_STRIDE_BYTES * 4];
    let mut offset = 0;
    for vertex in vertices {
        write_f32(&mut out, &mut offset, vertex.x);
        write_f32(&mut out, &mut offset, vertex.y);
        write_f32(&mut out, &mut offset, vertex.u);
        write_f32(&mut out, &mut offset, vertex.v);
        write_u32(&mut out, &mut offset, vertex.rgba);
    }
    out
}

fn index6_bytes(indices: [u32; 6]) -> [u8; 24] {
    let mut out = [0; 24];
    let mut offset = 0;
    for index in indices {
        write_u32(&mut out, &mut offset, index);
    }
    out
}

fn push_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_f32(out: &mut [u8], offset: &mut usize, value: f32) {
    let bytes = value.to_le_bytes();
    out[*offset..*offset + 4].copy_from_slice(&bytes);
    *offset += 4;
}

fn write_u32(out: &mut [u8], offset: &mut usize, value: u32) {
    let bytes = value.to_le_bytes();
    out[*offset..*offset + 4].copy_from_slice(&bytes);
    *offset += 4;
}

const WGSL: &str = r#"
struct Viewport {
   size_origin: vec4<f32>,
   matrix: vec4<f32>,
   translation_opacity: vec4<f32>,
};

struct Effect {
   texel_radius: vec4<f32>,
};

@group(0) @binding(0) var<uniform> viewport: Viewport;
@group(1) @binding(0) var source_tex: texture_2d<f32>;
@group(1) @binding(1) var source_sampler: sampler;
@group(2) @binding(0) var<uniform> effect: Effect;

struct VertexIn {
   @location(0) pos: vec2<f32>,
   @location(1) uv: vec2<f32>,
   @location(2) color: vec4<f32>,
};

struct VertexOut {
   @builtin(position) pos: vec4<f32>,
   @location(0) uv: vec2<f32>,
   @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(input: VertexIn) -> VertexOut {
   let size = max(viewport.size_origin.xy, vec2<f32>(1.0, 1.0));
   let origin = viewport.size_origin.zw;
   let transformed = vec2<f32>(
      viewport.matrix.x * input.pos.x + viewport.matrix.z * input.pos.y + viewport.translation_opacity.x,
      viewport.matrix.y * input.pos.x + viewport.matrix.w * input.pos.y + viewport.translation_opacity.y,
   );
   let local = (transformed - origin) / size;
   var out: VertexOut;
   out.pos = vec4<f32>(local.x * 2.0 - 1.0, 1.0 - local.y * 2.0, 0.0, 1.0);
   out.uv = input.uv;
   out.color = vec4<f32>(input.color.rgb, input.color.a * viewport.translation_opacity.z);
   return out;
}

struct ImageInstanceIn {
   @location(0) rect: vec4<f32>,
   @location(1) uv_rect: vec4<f32>,
   @location(2) alpha: f32,
   @location(3) unit: vec2<f32>,
};

@vertex
fn vs_image_instance(input: ImageInstanceIn) -> VertexOut {
   let dp = input.rect.xy + input.unit * input.rect.zw;
   let transformed = vec2<f32>(
      viewport.matrix.x * dp.x + viewport.matrix.z * dp.y + viewport.translation_opacity.x,
      viewport.matrix.y * dp.x + viewport.matrix.w * dp.y + viewport.translation_opacity.y,
   );
   let size = max(viewport.size_origin.xy, vec2<f32>(1.0, 1.0));
   let local = (transformed - viewport.size_origin.zw) / size;
   var out: VertexOut;
   out.pos = vec4<f32>(local.x * 2.0 - 1.0, 1.0 - local.y * 2.0, 0.0, 1.0);
   out.uv = mix(input.uv_rect.xy, input.uv_rect.zw, input.unit);
   out.color = vec4<f32>(1.0, 1.0, 1.0, input.alpha * viewport.translation_opacity.z);
   return out;
}

struct NineSliceInstanceIn {
   @location(0) rect: vec4<f32>,
   @location(1) image_size: vec2<f32>,
   @location(2) slice: vec4<f32>,
   @location(3) alpha: f32,
   @location(4) grid: vec4<u32>,
};

@vertex
fn vs_nine_slice_instance(input: NineSliceInstanceIn) -> VertexOut {
   let x2 = max(input.rect.z - input.slice.z, input.slice.x);
   let y2 = max(input.rect.w - input.slice.w, input.slice.y);
   let dx = array<f32, 4>(0.0, input.slice.x, x2, max(input.rect.z, x2));
   let dy = array<f32, 4>(0.0, input.slice.y, y2, max(input.rect.w, y2));
   let sx = array<f32, 4>(0.0, input.slice.x, input.image_size.x - input.slice.z, input.image_size.x);
   let sy = array<f32, 4>(0.0, input.slice.y, input.image_size.y - input.slice.w, input.image_size.y);
   let corner = vec2<f32>(f32(input.grid.z), f32(input.grid.w));
   let col = input.grid.x;
   let row = input.grid.y;
   let dp = input.rect.xy + vec2<f32>(
      mix(dx[col], dx[col + 1u], corner.x),
      mix(dy[row], dy[row + 1u], corner.y),
   );
   let source_valid = sx[col + 1u] > sx[col] && sy[row + 1u] > sy[row];
   let source_px = vec2<f32>(
      mix(sx[col], sx[col + 1u], corner.x),
      mix(sy[row], sy[row + 1u], corner.y),
   );
   let transformed = vec2<f32>(
      viewport.matrix.x * dp.x + viewport.matrix.z * dp.y + viewport.translation_opacity.x,
      viewport.matrix.y * dp.x + viewport.matrix.w * dp.y + viewport.translation_opacity.y,
   );
   let size = max(viewport.size_origin.xy, vec2<f32>(1.0, 1.0));
   let local = (transformed - viewport.size_origin.zw) / size;
   var out: VertexOut;
   out.pos = vec4<f32>(local.x * 2.0 - 1.0, 1.0 - local.y * 2.0, 0.0, 1.0);
   out.uv = select(corner, source_px / input.image_size, source_valid);
   out.color = vec4<f32>(1.0, 1.0, 1.0, input.alpha * viewport.translation_opacity.z);
   return out;
}

struct RRectIn {
   @location(0) rect: vec4<f32>,
   @location(1) radii: vec4<f32>,
   @location(2) color: vec4<f32>,
};

struct RRectOut {
   @builtin(position) pos: vec4<f32>,
   @location(0) local_px: vec2<f32>,
   @location(1) @interpolate(flat) rect_size: vec2<f32>,
   @location(2) @interpolate(flat) radii: vec4<f32>,
   @location(3) @interpolate(flat) color: vec4<f32>,
};

struct SpinnerInstanceIn {
   @location(0) center: vec2<f32>,
   @location(1) atom: f32,
   @location(2) alpha: f32,
   @location(3) color: vec4<f32>,
};

@vertex
fn vs_spinner_instance(
   @builtin(vertex_index) vertex_index: u32,
   input: SpinnerInstanceIn,
) -> RRectOut {
   let corners = array<vec2<f32>, 6>(
      vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
      vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
   );
   let directions = array<vec2<f32>, 12>(
      vec2<f32>(1.0, 0.0), vec2<f32>(0.8660253882, 0.5),
      vec2<f32>(0.5, 0.8660253882), vec2<f32>(0.0, 1.0),
      vec2<f32>(-0.5, 0.8660253882), vec2<f32>(-0.8660253882, 0.5),
      vec2<f32>(-1.0, 0.0), vec2<f32>(-0.8660253882, -0.5),
      vec2<f32>(-0.5, -0.8660253882), vec2<f32>(0.0, -1.0),
      vec2<f32>(0.5, -0.8660253882), vec2<f32>(0.8660253882, -0.5),
   );
   let atom_index = vertex_index / 6u;
   let unit = corners[vertex_index % 6u];
   let dot_radius = input.atom * 0.12;
   let rect_size = vec2<f32>(dot_radius * 2.0);
   let dot_center = input.center + directions[atom_index] * max(input.atom * 1.5, 1.0);
   let local_px = unit * rect_size;
   let dp = dot_center - vec2<f32>(dot_radius) + local_px;
   let transformed = vec2<f32>(
      viewport.matrix.x * dp.x + viewport.matrix.z * dp.y + viewport.translation_opacity.x,
      viewport.matrix.y * dp.x + viewport.matrix.w * dp.y + viewport.translation_opacity.y,
   );
   let size = max(viewport.size_origin.xy, vec2<f32>(1.0, 1.0));
   let local = (transformed - viewport.size_origin.zw) / size;
   let progress = fract(f32(atom_index) / 12.0 + viewport.translation_opacity.w);
   let dot_alpha = round(clamp(input.alpha, 0.0, 1.0)
      * (0.25 + progress * 0.75) * 255.0) / 255.0;
   var out: RRectOut;
   out.pos = vec4<f32>(local.x * 2.0 - 1.0, 1.0 - local.y * 2.0, 0.0, 1.0);
   out.local_px = local_px;
   out.rect_size = rect_size;
   out.radii = vec4<f32>(dot_radius);
   out.color = vec4<f32>(input.color.rgb, dot_alpha * viewport.translation_opacity.z);
   return out;
}

struct NeonMarkerIn {
   @location(0) center: vec2<f32>,
   @location(1) shape: vec4<f32>,
   @location(2) alpha: vec3<f32>,
   @location(3) core_color: vec4<f32>,
   @location(4) ring_color: vec4<f32>,
   @location(5) marker_viewport: vec4<f32>,
};

struct NeonMarkerOut {
   @builtin(position) pos: vec4<f32>,
   @location(0) pos_dp: vec2<f32>,
   @location(1) @interpolate(flat) center: vec2<f32>,
   @location(2) @interpolate(flat) shape: vec4<f32>,
   @location(3) @interpolate(flat) alpha: vec3<f32>,
   @location(4) @interpolate(flat) core_color: vec4<f32>,
   @location(5) @interpolate(flat) ring_color: vec4<f32>,
};

@vertex
fn vs_neon_marker_instance(
   @builtin(vertex_index) vertex_index: u32,
   input: NeonMarkerIn,
) -> NeonMarkerOut {
   let corners = array<vec2<f32>, 6>(
      vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
      vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
   );
   let radius = max(max(input.shape.w, input.shape.y + input.shape.z), input.shape.x);
   let dp = input.center + corners[vertex_index] * radius;
   let viewport_size = max(input.marker_viewport.zw, vec2<f32>(0.00001));
   let local = (dp - input.marker_viewport.xy) / viewport_size;
   var out: NeonMarkerOut;
   out.pos = vec4<f32>(local.x * 2.0 - 1.0, 1.0 - local.y * 2.0, 0.0, 1.0);
   out.pos_dp = dp;
   out.center = input.center;
   out.shape = input.shape;
   out.alpha = input.alpha;
   out.core_color = input.core_color;
   out.ring_color = input.ring_color;
   return out;
}

@fragment
fn fs_neon_marker(input: NeonMarkerOut) -> @location(0) vec4<f32> {
   let distance = length(input.pos_dp - input.center);
   if (distance > input.shape.w) {
      return vec4<f32>(0.0);
   }
   if (distance <= input.shape.x) {
      let edge = clamp(distance / max(input.shape.x, 0.001), 0.0, 1.0);
      let core_alpha = input.core_color.a * (1.0 - edge * 0.08);
      return vec4<f32>(input.core_color.rgb, core_alpha);
   }
   let ring_width = max(input.shape.z, 0.001);
   let ring_alpha = clamp(1.0 - abs(distance - input.shape.y) / ring_width, 0.0, 1.0)
      * input.alpha.z;
   let sigma = max(input.alpha.x, 0.001);
   let halo_alpha = exp(-(distance * distance) / (2.0 * sigma * sigma)) * input.alpha.y;
   let marker_alpha = max(ring_alpha, halo_alpha) * input.ring_color.a;
   if (marker_alpha <= 0.001) {
      return vec4<f32>(0.0);
   }
   return vec4<f32>(input.ring_color.rgb, clamp(marker_alpha, 0.0, 1.0));
}

@vertex
fn vs_rrect(@builtin(vertex_index) vertex_index: u32, input: RRectIn) -> RRectOut {
   let unit = array<vec2<f32>, 6>(
      vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
      vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
   )[vertex_index];
   let local_px = unit * input.rect.zw;
   let dp = input.rect.xy + local_px;
   let transformed = vec2<f32>(
      viewport.matrix.x * dp.x + viewport.matrix.z * dp.y + viewport.translation_opacity.x,
      viewport.matrix.y * dp.x + viewport.matrix.w * dp.y + viewport.translation_opacity.y,
   );
   let size = max(viewport.size_origin.xy, vec2<f32>(1.0, 1.0));
   let local = (transformed - viewport.size_origin.zw) / size;
   var out: RRectOut;
   out.pos = vec4<f32>(local.x * 2.0 - 1.0, 1.0 - local.y * 2.0, 0.0, 1.0);
   out.local_px = local_px;
   out.rect_size = input.rect.zw;
   out.radii = input.radii;
   out.color = vec4<f32>(input.color.rgb, input.color.a * viewport.translation_opacity.z);
   return out;
}

@fragment
fn fs_rrect(input: RRectOut) -> @location(0) vec4<f32> {
   let center = input.rect_size * 0.5;
   let right = input.local_px.x >= center.x;
   let bottom = input.local_px.y >= center.y;
   let top_radius = select(input.radii.x, input.radii.y, right);
   let bottom_radius = select(input.radii.w, input.radii.z, right);
   let radius = clamp(select(top_radius, bottom_radius, bottom), 0.0, min(center.x, center.y));
   let q = abs(input.local_px - center) - (center - vec2<f32>(radius));
   let distance = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - radius;
   let aa = max(fwidth(distance), 0.0001);
   let coverage = 1.0 - smoothstep(-aa, aa, distance);
   if coverage <= 0.0 {
      discard;
   }
   return vec4<f32>(input.color.rgb, input.color.a * coverage);
}

@fragment
fn fs_solid(input: VertexOut) -> @location(0) vec4<f32> {
   return input.color;
}

@fragment
fn fs_rgba(input: VertexOut) -> @location(0) vec4<f32> {
   return textureSample(source_tex, source_sampler, input.uv) * input.color;
}

@fragment
fn fs_a8(input: VertexOut) -> @location(0) vec4<f32> {
   let coverage = textureSample(source_tex, source_sampler, input.uv).r;
   return vec4<f32>(input.color.rgb, input.color.a * coverage);
}

@fragment
fn fs_sdf(input: VertexOut) -> @location(0) vec4<f32> {
   let distance = textureSample(source_tex, source_sampler, input.uv).r;
   let width = max(fwidth(distance), 0.001);
   let coverage = smoothstep(0.5 - width, 0.5 + width, distance);
   return vec4<f32>(input.color.rgb, input.color.a * coverage);
}

@fragment
fn fs_backdrop(input: VertexOut) -> @location(0) vec4<f32> {
   let texel = effect.texel_radius.xy;
   let radius = max(effect.texel_radius.z, 0.0);
   let step = texel * max(radius * 0.35, 1.0);
   var color = textureSample(source_tex, source_sampler, input.uv) * 0.227027;
   color += textureSample(source_tex, source_sampler, input.uv + vec2<f32>( step.x, 0.0)) * 0.1945946;
   color += textureSample(source_tex, source_sampler, input.uv + vec2<f32>(-step.x, 0.0)) * 0.1945946;
   color += textureSample(source_tex, source_sampler, input.uv + vec2<f32>(0.0,  step.y)) * 0.1216216;
   color += textureSample(source_tex, source_sampler, input.uv + vec2<f32>(0.0, -step.y)) * 0.1216216;
   color += textureSample(source_tex, source_sampler, input.uv + vec2<f32>( step.x,  step.y)) * 0.035135;
   color += textureSample(source_tex, source_sampler, input.uv + vec2<f32>(-step.x,  step.y)) * 0.035135;
   color += textureSample(source_tex, source_sampler, input.uv + vec2<f32>( step.x, -step.y)) * 0.035135;
   color += textureSample(source_tex, source_sampler, input.uv + vec2<f32>(-step.x, -step.y)) * 0.035135;
   let tint = input.color;
   return vec4<f32>(mix(color.rgb, tint.rgb, tint.a), max(color.a, tint.a));
}
"#;

const SCENE3D_WGSL: &str = r#"
struct Scene3dUniforms {
   mvp: mat4x4<f32>,
   color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> scene3d: Scene3dUniforms;

struct Scene3dColorVertexIn {
   @location(0) position: vec3<f32>,
   @location(1) color: vec4<f32>,
};

struct Scene3dColorVertexOut {
   @builtin(position) position: vec4<f32>,
   @location(0) color: vec4<f32>,
};

@vertex
fn vs_scene3d_color(input: Scene3dColorVertexIn) -> Scene3dColorVertexOut {
   let clip = scene3d.mvp * vec4<f32>(input.position, 1.0);
   var out: Scene3dColorVertexOut;
   out.position = vec4<f32>(clip.x, clip.y, clip.z * 0.5 + clip.w * 0.5, clip.w);
   out.color = input.color;
   return out;
}

@fragment
fn fs_scene3d_color(input: Scene3dColorVertexOut) -> @location(0) vec4<f32> {
   return input.color * scene3d.color;
}
"#;

const ID_MASK_FIELD_WGSL: &str = r#"
// Precompute nearest-city and seam seeds with jump flooding so the beauty
// compositor stays constant-cost per pixel. This is intentionally more passes,
// less fragment work: that was the winning Chrome/Dawn profile for dense maps.
struct IdMaskFieldParams {
   mask_size_jump_pad: vec4<f32>,
};

@group(0) @binding(0) var<uniform> field_params: IdMaskFieldParams;
@group(0) @binding(1) var field_city_tex: texture_2d<u32>;
@group(0) @binding(2) var field_neighborhood_tex: texture_2d<u32>;
@group(0) @binding(3) var field_city_src_tex: texture_2d<f32>;
@group(0) @binding(4) var field_seam_src_tex: texture_2d<f32>;
@group(0) @binding(5) var field_packed_src_tex: texture_2d<u32>;

const ID_MASK_PACKED_INVALID: u32 = 0xffffu;

struct IdMaskFieldRaster {
   @builtin(position) position: vec4<f32>,
};

struct IdMaskFieldTargets {
   @location(0) city: vec4<f32>,
   @location(1) seam: vec4<f32>,
};

@vertex
fn vs_id_mask_field(@builtin(vertex_index) vid: u32) -> IdMaskFieldRaster {
   let pos = array<vec2<f32>, 6>(
      vec2<f32>(-1.0, -1.0),
      vec2<f32>(1.0, -1.0),
      vec2<f32>(-1.0, 1.0),
      vec2<f32>(-1.0, 1.0),
      vec2<f32>(1.0, -1.0),
      vec2<f32>(1.0, 1.0),
   );
   var out: IdMaskFieldRaster;
   out.position = vec4<f32>(pos[vid], 0.0, 1.0);
   return out;
}

fn field_size() -> vec2<u32> {
   let raw = max(field_params.mask_size_jump_pad.xy, vec2<f32>(1.0, 1.0));
   return vec2<u32>(u32(raw.x), u32(raw.y));
}

fn field_pixel(pos: vec4<f32>, size: vec2<u32>) -> vec2<i32> {
   return vec2<i32>(clamp(pos.xy, vec2<f32>(0.0), vec2<f32>(size) - vec2<f32>(1.0)));
}

fn read_field_mask(tex: texture_2d<u32>, p: vec2<i32>, size: vec2<u32>) -> u32 {
   if (p.x < 0 || p.y < 0 || p.x >= i32(size.x) || p.y >= i32(size.y)) {
      return 0u;
   }
   return textureLoad(tex, p, 0).r;
}

fn read_seed_field(tex: texture_2d<f32>, p: vec2<i32>, size: vec2<u32>) -> vec4<f32> {
   if (p.x < 0 || p.y < 0 || p.x >= i32(size.x) || p.y >= i32(size.y)) {
      return vec4<f32>(-1.0, -1.0, 0.0, 0.0);
   }
   return textureLoad(tex, p, 0);
}

fn valid_seed(seed: vec4<f32>) -> bool {
   return seed.x >= -0.5 && seed.y >= -0.5 && seed.z >= 0.5;
}

fn seed_distance2(seed: vec4<f32>, p: vec2<i32>) -> f32 {
   if (!valid_seed(seed)) {
      return 1.0e30;
   }
   let delta = seed.xy - vec2<f32>(p);
   return dot(delta, delta);
}

fn seam_seed(p: vec2<i32>, size: vec2<u32>) -> vec4<f32> {
   let city = read_field_mask(field_city_tex, p, size);
   let neighborhood = read_field_mask(field_neighborhood_tex, p, size);
   if (city == 0u || neighborhood == 0u) {
      return vec4<f32>(-1.0, -1.0, 0.0, 0.0);
   }
   for (var oy = -1; oy <= 1; oy = oy + 1) {
      for (var ox = -1; ox <= 1; ox = ox + 1) {
         if (ox == 0 && oy == 0) {
            continue;
         }
         let q = p + vec2<i32>(ox, oy);
         if (read_field_mask(field_city_tex, q, size) == city) {
            let other = read_field_mask(field_neighborhood_tex, q, size);
            if (other != 0u && other != neighborhood) {
               return vec4<f32>(vec2<f32>(p), f32(city), 1.0);
            }
         }
      }
   }
   return vec4<f32>(-1.0, -1.0, 0.0, 0.0);
}

@fragment
fn fs_id_mask_field_seed(input: IdMaskFieldRaster) -> IdMaskFieldTargets {
   let size = field_size();
   let p = field_pixel(input.position, size);
   let city = read_field_mask(field_city_tex, p, size);
   let neighborhood = read_field_mask(field_neighborhood_tex, p, size);
   var out: IdMaskFieldTargets;
   out.city = select(
      vec4<f32>(-1.0, -1.0, 0.0, 0.0),
      vec4<f32>(vec2<f32>(p), f32(city), f32(neighborhood)),
      city != 0u,
   );
   out.seam = seam_seed(p, size);
   return out;
}

fn best_jump_seed(src: texture_2d<f32>, p: vec2<i32>, size: vec2<u32>, jump: i32) -> vec4<f32> {
   var best = read_seed_field(src, p, size);
   var best_distance = seed_distance2(best, p);
   for (var oy = -1; oy <= 1; oy = oy + 1) {
      for (var ox = -1; ox <= 1; ox = ox + 1) {
         if (ox == 0 && oy == 0) {
            continue;
         }
         let candidate = read_seed_field(src, p + vec2<i32>(ox * jump, oy * jump), size);
         let distance = seed_distance2(candidate, p);
         if (distance < best_distance) {
            best = candidate;
            best_distance = distance;
         }
      }
   }
   return best;
}

@fragment
fn fs_id_mask_field_jump(input: IdMaskFieldRaster) -> IdMaskFieldTargets {
   let size = field_size();
   let p = field_pixel(input.position, size);
   let jump = max(i32(round(field_params.mask_size_jump_pad.z)), 1);
   var out: IdMaskFieldTargets;
   out.city = best_jump_seed(field_city_src_tex, p, size, jump);
   out.seam = best_jump_seed(field_seam_src_tex, p, size, jump);
   return out;
}

fn read_packed_seed_field(tex: texture_2d<u32>, p: vec2<i32>, size: vec2<u32>) -> vec4<u32> {
   if (p.x < 0 || p.y < 0 || p.x >= i32(size.x) || p.y >= i32(size.y)) {
      return vec4<u32>(ID_MASK_PACKED_INVALID);
   }
   return textureLoad(tex, p, 0);
}

fn packed_seed_valid(seed: vec2<u32>) -> bool {
   return seed.x != ID_MASK_PACKED_INVALID && seed.y != ID_MASK_PACKED_INVALID;
}

fn packed_seed_distance2(seed: vec2<u32>, p: vec2<i32>) -> f32 {
   if (!packed_seed_valid(seed)) {
      return 1.0e30;
   }
   let delta = vec2<f32>(seed) - vec2<f32>(p);
   return dot(delta, delta);
}

@fragment
fn fs_id_mask_field_seed_packed(input: IdMaskFieldRaster) -> @location(0) vec4<u32> {
   let size = field_size();
   let p = field_pixel(input.position, size);
   let city = read_field_mask(field_city_tex, p, size);
   let invalid = vec2<u32>(ID_MASK_PACKED_INVALID);
   let coordinate = vec2<u32>(p);
   let seam = seam_seed(p, size);
   return vec4<u32>(
      select(invalid, coordinate, city != 0u),
      select(invalid, coordinate, valid_seed(seam)),
   );
}

fn best_jump_packed(p: vec2<i32>, size: vec2<u32>, jump: i32) -> vec4<u32> {
   var best = read_packed_seed_field(field_packed_src_tex, p, size);
   var city_distance = packed_seed_distance2(best.xy, p);
   var seam_distance = packed_seed_distance2(best.zw, p);
   for (var oy = -1; oy <= 1; oy = oy + 1) {
      for (var ox = -1; ox <= 1; ox = ox + 1) {
         if (ox == 0 && oy == 0) {
            continue;
         }
         let candidate = read_packed_seed_field(
            field_packed_src_tex,
            p + vec2<i32>(ox * jump, oy * jump),
            size,
         );
         let candidate_city_distance = packed_seed_distance2(candidate.xy, p);
         if (candidate_city_distance < city_distance) {
            best.x = candidate.x;
            best.y = candidate.y;
            city_distance = candidate_city_distance;
         }
         let candidate_seam_distance = packed_seed_distance2(candidate.zw, p);
         if (candidate_seam_distance < seam_distance) {
            best.z = candidate.z;
            best.w = candidate.w;
            seam_distance = candidate_seam_distance;
         }
      }
   }
   return best;
}

@fragment
fn fs_id_mask_field_jump_packed(input: IdMaskFieldRaster) -> @location(0) vec4<u32> {
   let size = field_size();
   let p = field_pixel(input.position, size);
   let jump = max(i32(round(field_params.mask_size_jump_pad.z)), 1);
   return best_jump_packed(p, size, jump);
}
"#;

const ID_MASK_WGSL: &str = r#"
struct IdMaskRasterParams {
   mask_size_mode: vec4<f32>,
   world_to_clip: mat4x4<f32>,
   model_to_world: mat4x4<f32>,
   camera_eye_front_min: vec4<f32>,
   normal_scale: vec4<f32>,
};

@group(0) @binding(0) var<uniform> raster_params: IdMaskRasterParams;

struct IdMaskRasterVertexIn {
   @location(0) position_px: vec2<f32>,
   @location(1) position_world: vec3<f32>,
   @location(2) city_id: u32,
   @location(3) neighborhood_id: u32,
};

struct IdMaskRasterOut {
   @builtin(position) position: vec4<f32>,
   @location(0) @interpolate(flat) city_id: u32,
   @location(1) @interpolate(flat) neighborhood_id: u32,
   @location(2) frontness: f32,
   @location(3) visible_front_min: vec2<f32>,
};

struct IdMaskRasterTargets {
   @location(0) city: u32,
   @location(1) neighborhood: u32,
};

@vertex
fn vs_id_mask_raster(input: IdMaskRasterVertexIn) -> IdMaskRasterOut {
   var out: IdMaskRasterOut;
   out.frontness = 1.0;
   out.visible_front_min = vec2<f32>(raster_params.mask_size_mode.w, raster_params.camera_eye_front_min.w);
   if (raster_params.mask_size_mode.z > 0.5) {
      let position_world = vec4<f32>(input.position_world, 1.0);
      let clip = raster_params.world_to_clip * position_world;
      out.position = vec4<f32>(clip.x, clip.y, clip.z * 0.5 + clip.w * 0.5, clip.w);
      if (raster_params.mask_size_mode.w > 0.5) {
         let normal = normalize((raster_params.model_to_world * position_world).xyz * raster_params.normal_scale.xyz);
         out.frontness = dot(normal, normalize(raster_params.camera_eye_front_min.xyz));
      }
   } else {
      let mask_size = max(raster_params.mask_size_mode.xy, vec2<f32>(1.0, 1.0));
      let normalized = input.position_px / mask_size;
      out.position = vec4<f32>(normalized.x * 2.0 - 1.0, 1.0 - normalized.y * 2.0, 0.0, 1.0);
   }
   out.city_id = input.city_id;
   out.neighborhood_id = input.neighborhood_id;
   return out;
}

@fragment
fn fs_id_mask_raster(input: IdMaskRasterOut) -> IdMaskRasterTargets {
   if (input.visible_front_min.x > 0.5 && input.frontness < input.visible_front_min.y) {
      discard;
   }
   var out: IdMaskRasterTargets;
   out.city = input.city_id;
   out.neighborhood = input.neighborhood_id;
   return out;
}

struct IdMaskCompositorParams {
   viewport: vec4<f32>,
   mask_size_scale_alpha: vec4<f32>,
   mode_glow_polish_fallback: vec4<f32>,
   exterior_halo: vec4<f32>,
   city_fill_colors: array<vec4<f32>, 4>,
   city_edge_colors: array<vec4<f32>, 4>,
   city_seam_colors: array<vec4<f32>, 4>,
   neighborhood_colors: array<vec4<f32>, 32>,
};

@group(0) @binding(0) var<uniform> compositor_params: IdMaskCompositorParams;
@group(0) @binding(1) var city_tex: texture_2d<u32>;
@group(0) @binding(2) var neighborhood_tex: texture_2d<u32>;
@group(0) @binding(3) var city_field_tex: texture_2d<f32>;
@group(0) @binding(4) var seam_field_tex: texture_2d<f32>;
@group(0) @binding(5) var packed_field_tex: texture_2d<u32>;

const ID_MASK_PACKED_INVALID: u32 = 0xffffu;

struct IdMaskCompositorRaster {
   @builtin(position) position: vec4<f32>,
   @location(0) pos_dp: vec2<f32>,
   @location(1) pos_mask: vec2<f32>,
};

@vertex
fn vs_id_mask_compositor(@builtin(vertex_index) vid: u32) -> IdMaskCompositorRaster {
   let offs = array<vec2<f32>, 6>(
      vec2<f32>(0.0, 0.0),
      vec2<f32>(1.0, 0.0),
      vec2<f32>(0.0, 1.0),
      vec2<f32>(0.0, 1.0),
      vec2<f32>(1.0, 0.0),
      vec2<f32>(1.0, 1.0),
   );
   let viewport = compositor_params.viewport;
   let local = offs[vid] * viewport.zw;
   let dp = viewport.xy + local;
   var out: IdMaskCompositorRaster;
   out.position = vec4<f32>(
      ((dp.x - viewport.x) / max(viewport.z, 0.00001)) * 2.0 - 1.0,
      1.0 - ((dp.y - viewport.y) / max(viewport.w, 0.00001)) * 2.0,
      0.0,
      1.0,
   );
   out.pos_dp = dp;
   out.pos_mask = local * max(compositor_params.mask_size_scale_alpha.z, 1.0);
   return out;
}

fn read_mask(tex: texture_2d<u32>, p: vec2<i32>, size: vec2<u32>) -> u32 {
   if (p.x < 0 || p.y < 0 || p.x >= i32(size.x) || p.y >= i32(size.y)) {
      return 0u;
   }
   return textureLoad(tex, p, 0).r;
}

fn read_field(tex: texture_2d<f32>, p: vec2<i32>, size: vec2<u32>) -> vec4<f32> {
   if (p.x < 0 || p.y < 0 || p.x >= i32(size.x) || p.y >= i32(size.y)) {
      return vec4<f32>(-1.0, -1.0, 0.0, 0.0);
   }
   return textureLoad(tex, p, 0);
}

fn read_packed_field(tex: texture_2d<u32>, p: vec2<i32>, size: vec2<u32>) -> vec4<u32> {
   if (p.x < 0 || p.y < 0 || p.x >= i32(size.x) || p.y >= i32(size.y)) {
      return vec4<u32>(ID_MASK_PACKED_INVALID);
   }
   return textureLoad(tex, p, 0);
}

fn packed_seed_valid(seed: vec2<u32>) -> bool {
   return seed.x != ID_MASK_PACKED_INVALID && seed.y != ID_MASK_PACKED_INVALID;
}

fn unpack_seed_coordinate(seed: vec2<u32>) -> vec4<f32> {
   if (!packed_seed_valid(seed)) {
      return vec4<f32>(-1.0, -1.0, 0.0, 0.0);
   }
   return vec4<f32>(vec2<f32>(seed), 1.0, 0.0);
}

fn field_valid(field: vec4<f32>) -> bool {
   return field.x >= -0.5 && field.y >= -0.5 && field.z >= 0.5;
}

fn field_distance(field: vec4<f32>, p: vec2<i32>) -> f32 {
   if (!field_valid(field)) {
      return 1000000.0;
   }
   return length(field.xy - vec2<f32>(p));
}

fn field_city(field: vec4<f32>) -> u32 {
   return u32(round(clamp(field.z, 0.0, 255.0)));
}

fn field_neighborhood(field: vec4<f32>) -> u32 {
   return u32(round(clamp(field.w, 0.0, 255.0)));
}

fn gaussian_alpha(distance_mask_px: f32, mask_scale: f32, sigma_px: f32, max_alpha: f32, cutoff_sigma: f32) -> f32 {
   let distance_px = distance_mask_px / max(mask_scale, 1.0);
   if (distance_px > sigma_px * cutoff_sigma) {
      return 0.0;
   }
   let sigma = max(sigma_px, 0.001);
   return clamp(max_alpha * exp(-(distance_px * distance_px) / (2.0 * sigma * sigma)), 0.0, 1.0);
}

@fragment
fn fs_id_mask_compositor(input: IdMaskCompositorRaster) -> @location(0) vec4<f32> {
   let mask_size = max(compositor_params.mask_size_scale_alpha.xy, vec2<f32>(1.0, 1.0));
   let size = vec2<u32>(u32(mask_size.x), u32(mask_size.y));
   let p = vec2<i32>(clamp(input.pos_mask, vec2<f32>(0.0, 0.0), mask_size - vec2<f32>(1.0, 1.0)));
   let mask_scale = max(compositor_params.mask_size_scale_alpha.z, 1.0);
   let mode = u32(compositor_params.mode_glow_polish_fallback.x);
   let glow_enabled = compositor_params.mode_glow_polish_fallback.y >= 0.5;
   let polish_radius = i32(ceil(compositor_params.mode_glow_polish_fallback.z * mask_scale));
   let fallback_radius = i32(ceil(compositor_params.mode_glow_polish_fallback.w * mask_scale));
   let nearest_city_field = read_field(city_field_tex, p, size);
   let city_direct = read_mask(city_tex, p, size);
   let city_distance = field_distance(nearest_city_field, p);
   let city = select(
      select(0u, field_city(nearest_city_field), city_distance <= f32(polish_radius)),
      city_direct,
      city_direct != 0u,
   );
   let neighborhood_direct = read_mask(neighborhood_tex, p, size);
   let neighborhood = select(
      select(0u, field_neighborhood(nearest_city_field), city_distance <= f32(fallback_radius) && field_city(nearest_city_field) == city),
      neighborhood_direct,
      city_direct == city && neighborhood_direct != 0u,
   );
   let city_index = min(city, 3u);
   let neighborhood_index = min(neighborhood, 31u);

   if (mode == 2u) {
      return select(vec4<f32>(compositor_params.city_edge_colors[city_index].rgb, 1.0), vec4<f32>(0.0, 0.0, 0.0, 1.0), city == 0u);
   }
   if (mode == 3u) {
      return select(vec4<f32>(compositor_params.neighborhood_colors[neighborhood_index].rgb, 1.0), vec4<f32>(0.0, 0.0, 0.0, 1.0), neighborhood == 0u);
   }

   let seam_field = read_field(seam_field_tex, p, size);
   let seam_distance = select(
      f32(i32(ceil(5.0 * mask_scale)) + 1),
      field_distance(seam_field, p),
      field_valid(seam_field) && field_city(seam_field) == city,
   );
   if (mode == 1u) {
      let core = gaussian_alpha(seam_distance, mask_scale, 0.42, 1.0, 2.1);
      return select(vec4<f32>(0.0, 0.0, 0.0, 1.0), vec4<f32>(1.0, 1.0, 1.0, 1.0), core > 0.04 && city != 0u);
   }

   if (city == 0u) {
      let dark_alpha = clamp(compositor_params.mask_size_scale_alpha.w, 0.0, 1.0);
      if (!glow_enabled) {
         return vec4<f32>(0.0, 0.0, 0.0, dark_alpha);
      }
      let halo_city = field_city(nearest_city_field);
      if (!field_valid(nearest_city_field) || halo_city == 0u) {
         return vec4<f32>(0.0, 0.0, 0.0, dark_alpha);
      }
      let halo_distance = city_distance;
      let alpha = max(
         gaussian_alpha(halo_distance, mask_scale, compositor_params.exterior_halo.x, compositor_params.exterior_halo.y, 3.2),
         gaussian_alpha(halo_distance, mask_scale, compositor_params.exterior_halo.z, compositor_params.exterior_halo.w, 3.2),
      );
      if (alpha <= 0.002) {
         return vec4<f32>(0.0, 0.0, 0.0, dark_alpha);
      }
      return vec4<f32>(compositor_params.city_edge_colors[min(halo_city, 3u)].rgb, alpha);
   }

   let normalized = input.pos_mask / mask_size;
   let top_left_light = clamp((1.0 - normalized.x) * 0.55 + (1.0 - normalized.y) * 0.45, 0.0, 1.0);
   let light = 0.92 + 0.08 * top_left_light;
   var fill = min(compositor_params.neighborhood_colors[neighborhood_index].rgb * light, vec3<f32>(1.0, 1.0, 1.0));

   if (glow_enabled) {
      let seam_halo = gaussian_alpha(seam_distance, mask_scale, 1.10, 0.22, 2.5);
      let seam_core = gaussian_alpha(seam_distance, mask_scale, 0.27, 0.82, 1.7);
      let seam_alpha = max(seam_halo, seam_core);
      if (seam_alpha > 0.002) {
         let seam = compositor_params.city_seam_colors[city_index].rgb;
         fill = mix(fill, seam, clamp(seam_alpha, 0.0, 1.0));
      }
   }

   return vec4<f32>(fill, 0.96);
}

@fragment
fn fs_id_mask_compositor_packed(input: IdMaskCompositorRaster) -> @location(0) vec4<f32> {
   let mask_size = max(compositor_params.mask_size_scale_alpha.xy, vec2<f32>(1.0, 1.0));
   let size = vec2<u32>(u32(mask_size.x), u32(mask_size.y));
   let p = vec2<i32>(clamp(input.pos_mask, vec2<f32>(0.0, 0.0), mask_size - vec2<f32>(1.0, 1.0)));
   let mask_scale = max(compositor_params.mask_size_scale_alpha.z, 1.0);
   let mode = u32(compositor_params.mode_glow_polish_fallback.x);
   let glow_enabled = compositor_params.mode_glow_polish_fallback.y >= 0.5;
   let polish_radius = i32(ceil(compositor_params.mode_glow_polish_fallback.z * mask_scale));
   let fallback_radius = i32(ceil(compositor_params.mode_glow_polish_fallback.w * mask_scale));
   let packed = read_packed_field(packed_field_tex, p, size);
   let nearest_city_field = unpack_seed_coordinate(packed.xy);
   let seam_field = unpack_seed_coordinate(packed.zw);
   let city_direct = read_mask(city_tex, p, size);
   let city_distance = field_distance(nearest_city_field, p);
   var nearest_city = city_direct;
   if (nearest_city == 0u && field_valid(nearest_city_field)) {
      nearest_city = read_mask(city_tex, vec2<i32>(nearest_city_field.xy), size);
   }
   let city = select(
      select(0u, nearest_city, city_distance <= f32(polish_radius)),
      city_direct,
      city_direct != 0u,
   );
   let neighborhood_direct = read_mask(neighborhood_tex, p, size);
   var neighborhood = 0u;
   if (city_direct == city && neighborhood_direct != 0u) {
      neighborhood = neighborhood_direct;
   } else if (city_distance <= f32(fallback_radius)
              && nearest_city == city
              && field_valid(nearest_city_field)) {
      neighborhood = read_mask(neighborhood_tex, vec2<i32>(nearest_city_field.xy), size);
   }
   let city_index = min(city, 3u);
   let neighborhood_index = min(neighborhood, 31u);

   if (mode == 2u) {
      return select(vec4<f32>(compositor_params.city_edge_colors[city_index].rgb, 1.0), vec4<f32>(0.0, 0.0, 0.0, 1.0), city == 0u);
   }
   if (mode == 3u) {
      return select(vec4<f32>(compositor_params.neighborhood_colors[neighborhood_index].rgb, 1.0), vec4<f32>(0.0, 0.0, 0.0, 1.0), neighborhood == 0u);
   }

   if (mode == 1u) {
      if (city == 0u || !field_valid(seam_field)) {
         return vec4<f32>(0.0, 0.0, 0.0, 1.0);
      }
      let seam_distance = field_distance(seam_field, p);
      let core = gaussian_alpha(seam_distance, mask_scale, 0.42, 1.0, 2.1);
      if (core <= 0.04 || read_mask(city_tex, vec2<i32>(seam_field.xy), size) != city) {
         return vec4<f32>(0.0, 0.0, 0.0, 1.0);
      }
      return vec4<f32>(1.0, 1.0, 1.0, 1.0);
   }

   let dark_alpha = clamp(compositor_params.mask_size_scale_alpha.w, 0.0, 1.0);
   if (city == 0u) {
      if (!glow_enabled || !field_valid(nearest_city_field) || nearest_city == 0u) {
         return vec4<f32>(0.0, 0.0, 0.0, dark_alpha);
      }
      let alpha = max(
         gaussian_alpha(city_distance, mask_scale, compositor_params.exterior_halo.x, compositor_params.exterior_halo.y, 3.2),
         gaussian_alpha(city_distance, mask_scale, compositor_params.exterior_halo.z, compositor_params.exterior_halo.w, 3.2),
      );
      if (alpha <= 0.002) {
         return vec4<f32>(0.0, 0.0, 0.0, dark_alpha);
      }
      return vec4<f32>(compositor_params.city_edge_colors[min(nearest_city, 3u)].rgb, alpha);
   }

   let normalized = input.pos_mask / mask_size;
   let top_left_light = clamp((1.0 - normalized.x) * 0.55 + (1.0 - normalized.y) * 0.45, 0.0, 1.0);
   let light = 0.92 + 0.08 * top_left_light;
   var fill = min(compositor_params.neighborhood_colors[neighborhood_index].rgb * light, vec3<f32>(1.0, 1.0, 1.0));

   if (glow_enabled && field_valid(seam_field)) {
      let seam_distance = field_distance(seam_field, p);
      let seam_halo = gaussian_alpha(seam_distance, mask_scale, 1.10, 0.22, 2.5);
      let seam_core = gaussian_alpha(seam_distance, mask_scale, 0.27, 0.82, 1.7);
      let seam_alpha = max(seam_halo, seam_core);
      if (seam_alpha > 0.002
          && read_mask(city_tex, vec2<i32>(seam_field.xy), size) == city) {
         let seam = compositor_params.city_seam_colors[city_index].rgb;
         fill = mix(fill, seam, clamp(seam_alpha, 0.0, 1.0));
      }
   }

   return vec4<f32>(fill, 0.96);
}
"#;
