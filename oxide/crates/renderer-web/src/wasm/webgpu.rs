use super::{
    a8_to_rgba, copy_a8_rows, copy_a8_rows_to_rgba_into, copy_rgba_rows, copy_rgba_rows_into,
    document, index_slice, normalized_index_mode, resolve_index, sanitize_scale, source_rect,
    vertex_slice,
};
use crate::image_slots::ImageSlots;
use crate::{id_mask_compositor, neon_marker, scene3d};
use crate::{NormalizedIndexMode, WebRendererStats};
use crate::solid_color::resolve_vertex_color;
use js_sys::Reflect;
use oxide_renderer_api as api;
use oxide_wasm_alloc_counter::AllocationSnapshot;
use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::HtmlCanvasElement;

const VERTEX_STRIDE: wgpu::BufferAddress = 32;
const VERTEX_STRIDE_BYTES: usize = 32;
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
const ID_MASK_FIELD_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const EFFECT_UNIFORM_SIZE_BYTES: usize = 16;
const EFFECT_UNIFORM_SIZE: u64 = EFFECT_UNIFORM_SIZE_BYTES as u64;
const MAX_BLUR_SIGMA: f32 = 96.0;
const TIMESTAMP_MAX_PASSES: u32 = 64;
const TIMESTAMP_QUERY_COUNT: u32 = TIMESTAMP_MAX_PASSES * 2;
const TIMESTAMP_READBACK_SLOTS: usize = 48;
const TIMESTAMP_READBACK_INTERVAL_FRAMES: u64 = 8;

#[derive(Clone, Copy)]
struct GpuVertex {
    x: f32,
    y: f32,
    u: f32,
    v: f32,
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}

#[derive(Clone, Copy)]
enum GpuImageKind {
    Rgba,
    A8,
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
    rect: api::RectF,
    width: u32,
    height: u32,
    scale: f32,
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

#[derive(Clone, Copy)]
enum DrawKind {
    Solid,
    Rgba { image: u32 },
    A8 { image: u32 },
    Sdf { image: u32 },
    Layer { id: u32 },
    Backdrop { rect: api::RectF, sigma: f32 },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DrawPipelineKey {
    Solid,
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

#[derive(Clone, Copy)]
struct GpuDraw {
    kind: DrawKind,
    first_index: u32,
    index_count: u32,
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

struct FrameData {
    vertices: Vec<GpuVertex>,
    indices: Vec<u32>,
    draws: Vec<GpuDraw>,
    layer_passes: Vec<FrameLayerPass>,
    effect_count: usize,
    effect_first_sigma_bits: u32,
    effect_shared_sigma: f32,
    effect_single_uniform_slot: bool,
}

impl FrameData {
    fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
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
        if frame_id % TIMESTAMP_READBACK_INTERVAL_FRAMES != 0 {
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
    id_mask_field_layout: wgpu::BindGroupLayout,
    id_mask_compositor_layout: wgpu::BindGroupLayout,
    solid_pipeline: wgpu::RenderPipeline,
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
    id_mask_field_seed_pipeline: wgpu::RenderPipeline,
    id_mask_field_jump_pipeline: wgpu::RenderPipeline,
    id_mask_compositor_pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
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
    scene_texture: wgpu::Texture,
    scene_view: wgpu::TextureView,
    scene_bind_group: wgpu::BindGroup,
    scene_depth_texture: wgpu::Texture,
    scene_depth_view: wgpu::TextureView,
    scratch_texture: wgpu::Texture,
    scratch_view: wgpu::TextureView,
    scratch_bind_group: wgpu::BindGroup,
    viewport_buffer: wgpu::Buffer,
    viewport_bind_group: wgpu::BindGroup,
    effect_buffer: wgpu::Buffer,
    effect_bind_group: wgpu::BindGroup,
    effect_uniform_capacity: u64,
    effect_uniform_stride: u64,
    vertex_buffer: Option<wgpu::Buffer>,
    vertex_capacity: u64,
    index_buffer: Option<wgpu::Buffer>,
    index_capacity: u64,
    scene3d_uniform_buffer: Option<wgpu::Buffer>,
    scene3d_uniform_capacity: u64,
    scene3d_bind_group: Option<wgpu::BindGroup>,
    present_vertex_buffer: wgpu::Buffer,
    present_index_buffer: wgpu::Buffer,
    present_width: u32,
    present_height: u32,
    present_scale: f32,
    vertex_bytes: Vec<u8>,
    index_bytes: Vec<u8>,
    scene3d_uniform_bytes: Vec<u8>,
    effect_uniform_bytes: Vec<u8>,
    scene3d_draws: Vec<Scene3dDraw>,
    scene3d_overlay_draws: Vec<Scene3dDraw>,
    id_mask_draws: Vec<IdMaskDraw>,
    id_mask_draw_chunk_indices: Vec<usize>,
    id_mask_vertex_caches: Vec<IdMaskVertexCache>,
    id_mask_width: u32,
    id_mask_height: u32,
    id_mask_city_texture: Option<wgpu::Texture>,
    id_mask_neighborhood_texture: Option<wgpu::Texture>,
    id_mask_city_field_a_texture: Option<wgpu::Texture>,
    id_mask_city_field_b_texture: Option<wgpu::Texture>,
    id_mask_seam_field_a_texture: Option<wgpu::Texture>,
    id_mask_seam_field_b_texture: Option<wgpu::Texture>,
    id_mask_city_view: Option<wgpu::TextureView>,
    id_mask_neighborhood_view: Option<wgpu::TextureView>,
    id_mask_city_field_a_view: Option<wgpu::TextureView>,
    id_mask_city_field_b_view: Option<wgpu::TextureView>,
    id_mask_seam_field_a_view: Option<wgpu::TextureView>,
    id_mask_seam_field_b_view: Option<wgpu::TextureView>,
    id_mask_raster_uniform_buffer: Option<wgpu::Buffer>,
    id_mask_raster_bind_group: Option<wgpu::BindGroup>,
    id_mask_field_uniform_buffer: Option<wgpu::Buffer>,
    id_mask_field_bind_group_a: Option<wgpu::BindGroup>,
    id_mask_field_bind_group_b: Option<wgpu::BindGroup>,
    id_mask_compositor_uniform_buffer: Option<wgpu::Buffer>,
    id_mask_compositor_uniform_capacity: u64,
    id_mask_compositor_bind_group_a: Option<wgpu::BindGroup>,
    id_mask_compositor_bind_group_b: Option<wgpu::BindGroup>,
    scene3d_clear_color: Option<api::Color>,
    scene3d_clear_depth: bool,
    scene3d_active: bool,
    images: ImageSlots<GpuImage>,
    layers: BTreeMap<u32, GpuLayer>,
    meshes_3d: Vec<Option<GpuMesh3d>>,
    frame: FrameData,
    scratch_vertices: Vec<GpuVertex>,
    scratch_indices: Vec<u32>,
    scratch_points: Vec<(f32, f32)>,
    image_upload_scratch: Vec<u8>,
    id_mask_raster_uniform_bytes: Vec<u8>,
    id_mask_compositor_uniform_bytes: Vec<u8>,
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
    rgba: &[u8],
) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &image.texture,
            mip_level: 0,
            origin: wgpu::Origin3d { x, y, z: 0 },
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width.saturating_mul(4)),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    stats.texture_upload_bytes = stats.texture_upload_bytes.saturating_add(rgba.len() as u64);
}

impl WebGpuRenderer {
    pub async fn from_canvas_id(id: &str) -> Result<Self, api::RenderError> {
        Self::from_canvas(canvas_by_id(id)?).await
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

        let programs = create_programs(&device, config.format);
        let (scene_texture, scene_view, scene_bind_group) = create_target_texture(
            &device,
            &programs,
            "oxide-webgpu-scene",
            config.format,
            width,
            height,
        );
        let (scene_depth_texture, scene_depth_view) =
            create_depth_texture(&device, "oxide-webgpu-scene-depth", width, height);
        let (scratch_texture, scratch_view, scratch_bind_group) = create_target_texture(
            &device,
            &programs,
            "oxide-webgpu-scratch",
            config.format,
            width,
            height,
        );
        let (viewport_buffer, viewport_bind_group) = create_viewport_bind_group(&device, &programs);
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

        Ok(Self {
            canvas,
            surface,
            device,
            queue,
            config,
            programs,
            scene_texture,
            scene_view,
            scene_bind_group,
            scene_depth_texture,
            scene_depth_view,
            scratch_texture,
            scratch_view,
            scratch_bind_group,
            viewport_buffer,
            viewport_bind_group,
            effect_buffer,
            effect_bind_group,
            effect_uniform_capacity,
            effect_uniform_stride,
            vertex_buffer: None,
            vertex_capacity: 0,
            index_buffer: None,
            index_capacity: 0,
            scene3d_uniform_buffer: None,
            scene3d_uniform_capacity: 0,
            scene3d_bind_group: None,
            present_vertex_buffer,
            present_index_buffer,
            present_width: 0,
            present_height: 0,
            present_scale: 0.0,
            vertex_bytes: Vec::new(),
            index_bytes: Vec::new(),
            scene3d_uniform_bytes: Vec::new(),
            effect_uniform_bytes: Vec::new(),
            scene3d_draws: Vec::new(),
            scene3d_overlay_draws: Vec::new(),
            id_mask_draws: Vec::new(),
            id_mask_draw_chunk_indices: Vec::new(),
            id_mask_vertex_caches: Vec::new(),
            id_mask_width: 0,
            id_mask_height: 0,
            id_mask_city_texture: None,
            id_mask_neighborhood_texture: None,
            id_mask_city_field_a_texture: None,
            id_mask_city_field_b_texture: None,
            id_mask_seam_field_a_texture: None,
            id_mask_seam_field_b_texture: None,
            id_mask_city_view: None,
            id_mask_neighborhood_view: None,
            id_mask_city_field_a_view: None,
            id_mask_city_field_b_view: None,
            id_mask_seam_field_a_view: None,
            id_mask_seam_field_b_view: None,
            id_mask_raster_uniform_buffer: None,
            id_mask_raster_bind_group: None,
            id_mask_field_uniform_buffer: None,
            id_mask_field_bind_group_a: None,
            id_mask_field_bind_group_b: None,
            id_mask_compositor_uniform_buffer: None,
            id_mask_compositor_uniform_capacity: 0,
            id_mask_compositor_bind_group_a: None,
            id_mask_compositor_bind_group_b: None,
            scene3d_clear_color: None,
            scene3d_clear_depth: true,
            scene3d_active: false,
            images: ImageSlots::new(),
            layers: BTreeMap::new(),
            meshes_3d: vec![None],
            frame: FrameData {
                vertices: Vec::new(),
                indices: Vec::new(),
                draws: Vec::new(),
                layer_passes: Vec::new(),
                effect_count: 0,
                effect_first_sigma_bits: 0,
                effect_shared_sigma: 0.0,
                effect_single_uniform_slot: true,
            },
            scratch_vertices: Vec::new(),
            scratch_indices: Vec::new(),
            scratch_points: Vec::new(),
            image_upload_scratch: Vec::new(),
            id_mask_raster_uniform_bytes: Vec::new(),
            id_mask_compositor_uniform_bytes: Vec::new(),
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

    fn scratch_capacity_breakdown(&self) -> ScratchCapacityBreakdown {
        let mut capacity = ScratchCapacityBreakdown::default();
        capacity.draw = capacity.draw.saturating_add(self.vertex_bytes.capacity());
        capacity.draw = capacity.draw.saturating_add(self.index_bytes.capacity());
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
            self.id_mask_vertex_caches
                .capacity()
                .saturating_mul(core::mem::size_of::<IdMaskVertexCache>()),
        );
        for cache in &self.id_mask_vertex_caches {
            capacity.id_mask = capacity.id_mask.saturating_add(cache.bytes.capacity());
        }
        capacity.resource_table = capacity.resource_table.saturating_add(
            self.images.storage_capacity_bytes(),
        );
        capacity.resource_table = capacity.resource_table.saturating_add(
            self.meshes_3d.capacity().saturating_mul(core::mem::size_of::<Option<GpuMesh3d>>()),
        );
        capacity.draw = capacity.draw.saturating_add(
            self.frame.vertices.capacity().saturating_mul(core::mem::size_of::<GpuVertex>()),
        );
        capacity.draw = capacity.draw.saturating_add(
            self.frame.indices.capacity().saturating_mul(core::mem::size_of::<u32>()),
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
            self.scratch_vertices.capacity().saturating_mul(core::mem::size_of::<GpuVertex>()),
        );
        capacity.draw = capacity.draw.saturating_add(
            self.scratch_indices.capacity().saturating_mul(core::mem::size_of::<u32>()),
        );
        capacity.draw = capacity.draw.saturating_add(
            self.scratch_points.capacity().saturating_mul(core::mem::size_of::<(f32, f32)>()),
        );
        capacity.image_upload =
            capacity.image_upload.saturating_add(self.image_upload_scratch.capacity());
        capacity.id_mask =
            capacity.id_mask.saturating_add(self.id_mask_raster_uniform_bytes.capacity());
        capacity.id_mask =
            capacity.id_mask.saturating_add(self.id_mask_compositor_uniform_bytes.capacity());
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
        self.stats.gpu_timestamp_readback_interval = TIMESTAMP_READBACK_INTERVAL_FRAMES as u32;
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
        self.push_image(width, height, GpuImageKind::Rgba, &rgba)
    }

    pub fn try_image_create_a8(
        &mut self,
        width: u32,
        height: u32,
        data: &[u8],
        row_bytes: usize,
    ) -> Result<api::ImageHandle, api::RenderError> {
        let alpha = copy_a8_rows(width, height, data, row_bytes)
            .ok_or(api::RenderError::InvalidOperation("invalid a8 image rows"))?;
        self.push_image(width, height, GpuImageKind::A8, &a8_to_rgba(&alpha))
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
        if self.image_upload_scratch_enabled {
            let grew = copy_a8_rows_to_rgba_into(
                &mut self.image_upload_scratch,
                width,
                height,
                data,
                row_bytes,
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
        let alpha = copy_a8_rows(width, height, data, row_bytes)
            .ok_or(api::RenderError::InvalidOperation("invalid a8 update rows"))?;
        let rgba = a8_to_rgba(&alpha);
        self.record_image_upload_temp(alpha.len().saturating_add(rgba.len()), 2);
        self.update_image(handle, x, y, width, height, GpuImageKind::A8, &rgba)
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
      self.images.remove(handle.0).is_some()
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
        self.stats.id_mask_draws = self.stats.id_mask_draws.saturating_add(1);
        let vertex_cache_first = self.id_mask_draw_chunk_indices.len() as u32;
        let mut vertex_count = 0usize;
        for chunk in pass.raster.chunks {
            let end = chunk.first_vertex.saturating_add(chunk.vertex_count);
            let Some(vertices) = pass.raster.vertices.get(chunk.first_vertex..end) else {
                return Err(api::RenderError::InvalidOperation(
                    "id-mask GPU raster chunk range is outside vertex data",
                ));
            };
            let vertex_cache_index = self.id_mask_vertex_cache_index(chunk.content_hash, vertices);
            self.id_mask_draw_chunk_indices.push(vertex_cache_index);
            vertex_count = vertex_count.saturating_add(chunk.vertex_count);
        }
        let vertex_cache_count =
            self.id_mask_draw_chunk_indices.len().saturating_sub(vertex_cache_first as usize)
                as u32;
        self.id_mask_draws.push(IdMaskDraw {
            viewport: pass.raster.viewport,
            mask_width: pass.raster.mask_width as u32,
            mask_height: pass.raster.mask_height as u32,
            mask_scale: pass.raster.mask_scale,
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
            let halo = marker.bounds();
            let halo_color = api::Color::rgba(
                marker.ring_color.r,
                marker.ring_color.g,
                marker.ring_color.b,
                marker.halo_alpha_max,
            );
            self.encode_rrect(halo, [halo.w * 0.5; 4], halo_color);
            let ring = api::RectF::new(
                marker.center[0] - marker.ring_radius_px,
                marker.center[1] - marker.ring_radius_px,
                marker.ring_radius_px * 2.0,
                marker.ring_radius_px * 2.0,
            );
            self.encode_rrect(
                ring,
                [marker.ring_radius_px; 4],
                api::Color::rgba(
                    marker.ring_color.r,
                    marker.ring_color.g,
                    marker.ring_color.b,
                    marker.ring_alpha_max,
                ),
            );
            let core = api::RectF::new(
                marker.center[0] - marker.core_radius_px,
                marker.center[1] - marker.core_radius_px,
                marker.core_radius_px * 2.0,
                marker.core_radius_px * 2.0,
            );
            self.encode_rrect(core, [marker.core_radius_px; 4], marker.core_color);
        }
        Ok(())
    }

    fn push_image(
        &mut self,
        width: u32,
        height: u32,
        kind: GpuImageKind,
        rgba: &[u8],
    ) -> Result<api::ImageHandle, api::RenderError> {
        if !self.images.has_capacity() {
            return Err(api::RenderError::InvalidOperation("gpu image slot capacity exhausted"));
        }
        let image = self.create_image(width, height, kind, rgba)?;
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
        rgba: &[u8],
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
            format: wgpu::TextureFormat::Rgba8Unorm,
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
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width.saturating_mul(4)),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        self.stats.texture_upload_bytes =
            self.stats.texture_upload_bytes.saturating_add(rgba.len() as u64);
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
        );
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
        write_image_update(&self.queue, &mut self.stats, image, x, y, width, height, rgba);
        Ok(())
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
        first_index: u32,
        index_count: u32,
        clip: api::RectI,
        target: Option<u32>,
    ) -> bool {
        if !self.draw_item_coalescing_enabled {
            return false;
        }
        let Some(last) = self.frame.draws.last_mut() else {
            return false;
        };
        if last.first_index.saturating_add(last.index_count) == first_index
            && last.clip == clip
            && last.target == target
            && last.effect_uniform_offset == 0
            && coalescible_draw_kind(last.kind, kind)
        {
            last.index_count = last.index_count.saturating_add(index_count);
            self.stats.draw_items_coalesced = self.stats.draw_items_coalesced.saturating_add(1);
            true
        } else {
            false
        }
    }

    fn push_draw(&mut self, kind: DrawKind, vertices: &[GpuVertex], indices: &[u32]) {
        if vertices.is_empty() || indices.is_empty() {
            return;
        }
        let base = self.frame.vertices.len() as u32;
        let first_index = self.frame.indices.len() as u32;
        let index_count = indices.len() as u32;
        let clip = self.current_clip();
        let target = self.current_target();
        self.frame.vertices.extend_from_slice(vertices);
        self.frame.indices.extend(indices.iter().map(|index| base.saturating_add(*index)));
        self.frame.record_draw_kind(kind);
        if self.try_coalesce_draw_item(kind, first_index, index_count, clip, target) {
            return;
        }
        self.frame.draws.push(GpuDraw {
            kind,
            first_index,
            index_count,
            clip,
            effect_uniform_offset: 0,
            target,
        });
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
        let base = self.frame.vertices.len() as u32;
        let first_index = self.frame.indices.len() as u32;
        let index_count = self.scratch_indices.len() as u32;
        self.frame.vertices.extend_from_slice(&self.scratch_vertices);
        self.frame
            .indices
            .extend(self.scratch_indices.iter().map(|index| base.saturating_add(*index)));
        self.frame.record_draw_kind(kind);
        if self.try_coalesce_draw_item(kind, first_index, index_count, clip, target) {
            return;
        }
        self.frame.draws.push(GpuDraw {
            kind,
            first_index,
            index_count,
            clip,
            effect_uniform_offset: 0,
            target,
        });
    }

    fn cached_layer(&self, id: u32, rect: api::RectF) -> Option<&GpuLayer> {
        let layer = self.layers.get(&id)?;
        if layer.width == self.width
            && layer.height == self.height
            && (layer.scale - self.scale).abs() <= f32::EPSILON
            && layer.rect == rect
        {
            Some(layer)
        } else {
            None
        }
    }

    fn ensure_layer(&mut self, id: u32, rect: api::RectF) {
        let width = self.width.max(1);
        let height = self.height.max(1);
        let recreate = self.layers.get(&id).map_or(true, |layer| {
            layer.width != width
                || layer.height != height
                || (layer.scale - self.scale).abs() > f32::EPSILON
        });
        if recreate {
            let (texture, view, bind_group) = create_target_texture(
                &self.device,
                &self.programs,
                "oxide-webgpu-layer",
                self.config.format,
                width,
                height,
            );
            self.layers.insert(
                id,
                GpuLayer { texture, view, bind_group, rect, width, height, scale: self.scale },
            );
            self.stats.texture_creates = self.stats.texture_creates.saturating_add(1);
            self.stats.bind_group_creates = self.stats.bind_group_creates.saturating_add(1);
            self.stats.target_texture_creates = self.stats.target_texture_creates.saturating_add(1);
            self.stats.target_bind_group_creates =
                self.stats.target_bind_group_creates.saturating_add(1);
            self.stats.layer_texture_creates = self.stats.layer_texture_creates.saturating_add(1);
            self.stats.layer_bind_group_creates =
                self.stats.layer_bind_group_creates.saturating_add(1);
        } else if let Some(layer) = self.layers.get_mut(&id) {
            layer.rect = rect;
        }
    }

    fn push_layer_draw(&mut self, id: u32, rect: api::RectF) {
        let logical_w = logical_dimension(self.width, self.scale).max(1.0);
        let logical_h = logical_dimension(self.height, self.scale).max(1.0);
        let u0 = rect.x / logical_w;
        let v0 = rect.y / logical_h;
        let u1 = (rect.x + rect.w) / logical_w;
        let v1 = (rect.y + rect.h) / logical_h;
        let color = api::Color::rgba(1.0, 1.0, 1.0, 1.0);
        let vertices = quad_vertices(rect, u0, v0, u1, v1, color);
        self.push_draw(DrawKind::Layer { id }, &vertices, &[0, 1, 2, 2, 1, 3]);
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

        if !dirty && self.cached_layer(id, rect).is_some() {
            let skipped = skip_layer_body(list, index);
            self.stats.layer_cache_hits = self.stats.layer_cache_hits.saturating_add(1);
            self.stats.layer_cache_skipped_draws =
                self.stats.layer_cache_skipped_draws.saturating_add(skipped);
            self.push_layer_draw(id, rect);
            return;
        }

        self.stats.layer_cache_misses = self.stats.layer_cache_misses.saturating_add(1);
        self.ensure_layer(id, rect);
        let start = self.frame.draws.len();
        self.target_stack.push(id);
        self.encode_items(list, index, true);
        let _ = self.target_stack.pop();
        let end = self.frame.draws.len();
        self.frame.layer_passes.push(FrameLayerPass { id, start, end });
        self.push_layer_draw(id, rect);
    }

    fn encode_items(&mut self, list: &api::DrawList, index: &mut usize, stop_at_layer_end: bool) {
        while *index < list.items.len() {
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
        let color = api::Color::rgba(1.0, 1.0, 1.0, alpha.clamp(0.0, 1.0));
        let vertices = quad_vertices(dst, u0, v0, u1, v1, color);
        let kind = match (image.kind, sdf) {
            (GpuImageKind::Rgba, _) => DrawKind::Rgba { image: handle.0 },
            (GpuImageKind::A8, false) => DrawKind::A8 { image: handle.0 },
            (GpuImageKind::A8, true) => DrawKind::Sdf { image: handle.0 },
        };
        self.push_draw(kind, &vertices, &[0, 1, 2, 2, 1, 3]);
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
        self.clear_scratch_draw();
        rounded_rect_mesh_into(
            rect,
            radii,
            color,
            &mut self.scratch_points,
            &mut self.scratch_vertices,
            &mut self.scratch_indices,
        );
        let triangles = self.scratch_indices.len() / 3;
        self.push_scratch_draw(DrawKind::Solid);
        self.stats.solid_tris = self.stats.solid_tris.saturating_add(triangles as u32);
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
        let iw = image.width as f32;
        let ih = image.height as f32;
        self.stats.nine_slice_draws = self.stats.nine_slice_draws.saturating_add(1);
        let left = slice.left.clamp(0.0, iw);
        let right = slice.right.clamp(0.0, iw - left);
        let top = slice.top.clamp(0.0, ih);
        let bottom = slice.bottom.clamp(0.0, ih - top);
        let dx = [rect.x, rect.x + left, rect.x + (rect.w - right).max(left), rect.x + rect.w];
        let dy = [rect.y, rect.y + top, rect.y + (rect.h - bottom).max(top), rect.y + rect.h];
        let sx = [0.0, left, iw - right, iw];
        let sy = [0.0, top, ih - bottom, ih];

        for row in 0..3 {
            for col in 0..3 {
                let dst =
                    api::RectF::new(dx[col], dy[row], dx[col + 1] - dx[col], dy[row + 1] - dy[row]);
                let src =
                    api::RectF::new(sx[col], sy[row], sx[col + 1] - sx[col], sy[row + 1] - sy[row]);
                self.encode_image(handle, dst, src, alpha, false);
            }
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
        self.push_draw(DrawKind::Backdrop { rect, sigma }, &vertices, &[0, 1, 2, 2, 1, 3]);
    }

    fn encode_spinner(&mut self, center: [f32; 2], atom: f32, alpha: f32) {
        let radius = (atom * 1.5).max(1.0);
        for idx in 0..12 {
            let t = idx as f32 / 12.0;
            let angle = t * core::f32::consts::TAU;
            let x = center[0] + angle.cos() * radius;
            let y = center[1] + angle.sin() * radius;
            let rect = api::RectF::new(x - atom * 0.12, y - atom * 0.12, atom * 0.24, atom * 0.24);
            let a = alpha.clamp(0.0, 1.0) * (0.25 + t * 0.75);
            self.encode_rrect(rect, [atom * 0.12; 4], api::Color::rgba(0.15, 0.15, 0.15, a));
        }
    }

    fn recreate_targets(&mut self) {
        let (scene_texture, scene_view, scene_bind_group) = create_target_texture(
            &self.device,
            &self.programs,
            "oxide-webgpu-scene",
            self.config.format,
            self.width,
            self.height,
        );
        let (scene_depth_texture, scene_depth_view) =
            create_depth_texture(&self.device, "oxide-webgpu-scene-depth", self.width, self.height);
        let (scratch_texture, scratch_view, scratch_bind_group) = create_target_texture(
            &self.device,
            &self.programs,
            "oxide-webgpu-scratch",
            self.config.format,
            self.width,
            self.height,
        );
        self.scene_texture = scene_texture;
        self.scene_view = scene_view;
        self.scene_bind_group = scene_bind_group;
        self.scene_depth_texture = scene_depth_texture;
        self.scene_depth_view = scene_depth_view;
        self.scratch_texture = scratch_texture;
        self.scratch_view = scratch_view;
        self.scratch_bind_group = scratch_bind_group;
        self.stats.texture_creates = self.stats.texture_creates.saturating_add(3);
        self.stats.bind_group_creates = self.stats.bind_group_creates.saturating_add(2);
        self.stats.target_texture_creates = self.stats.target_texture_creates.saturating_add(3);
        self.stats.target_bind_group_creates =
            self.stats.target_bind_group_creates.saturating_add(2);
    }

    fn upload_frame_buffers(&mut self) {
        self.vertex_bytes.clear();
        self.index_bytes.clear();
        encode_vertices(&self.frame.vertices, &mut self.vertex_bytes);
        encode_indices(&self.frame.indices, &mut self.index_bytes);
        if ensure_buffer(
            &self.device,
            &mut self.vertex_buffer,
            &mut self.vertex_capacity,
            self.vertex_bytes.len() as u64,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            "oxide-webgpu-vertices",
        ) {
            self.stats.buffer_grows = self.stats.buffer_grows.saturating_add(1);
            self.stats.draw_buffer_grows = self.stats.draw_buffer_grows.saturating_add(1);
        }
        if ensure_buffer(
            &self.device,
            &mut self.index_buffer,
            &mut self.index_capacity,
            self.index_bytes.len() as u64,
            wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            "oxide-webgpu-indices",
        ) {
            self.stats.buffer_grows = self.stats.buffer_grows.saturating_add(1);
            self.stats.draw_buffer_grows = self.stats.draw_buffer_grows.saturating_add(1);
        }
        if let Some(buffer) = &self.vertex_buffer {
            self.queue.write_buffer(buffer, 0, &self.vertex_bytes);
            self.stats.buffer_upload_bytes =
                self.stats.buffer_upload_bytes.saturating_add(self.vertex_bytes.len() as u64);
        }
        if let Some(buffer) = &self.index_buffer {
            self.queue.write_buffer(buffer, 0, &self.index_bytes);
            self.stats.buffer_upload_bytes =
                self.stats.buffer_upload_bytes.saturating_add(self.index_bytes.len() as u64);
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

    fn write_viewport_uniform(&mut self) {
        let logical_w = logical_dimension(self.width, self.scale).max(1.0);
        let logical_h = logical_dimension(self.height, self.scale).max(1.0);
        let bytes = f32x4_bytes([logical_w, logical_h, 0.0, 0.0]);
        self.queue.write_buffer(&self.viewport_buffer, 0, &bytes);
        self.stats.buffer_upload_bytes =
            self.stats.buffer_upload_bytes.saturating_add(bytes.len() as u64);
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

    fn ensure_id_mask_resources(&mut self, width: u32, height: u32, compositor_uniform_len: usize) {
        let size_changed = self.id_mask_width != width
            || self.id_mask_height != height
            || self.id_mask_city_view.is_none()
            || self.id_mask_neighborhood_view.is_none();
        if size_changed {
            let city_texture =
                create_id_mask_texture(&self.device, "oxide-webgpu-id-mask-city", width, height);
            let neighborhood_texture = create_id_mask_texture(
                &self.device,
                "oxide-webgpu-id-mask-neighborhood",
                width,
                height,
            );
            let city_field_a_texture = create_id_mask_field_texture(
                &self.device,
                "oxide-webgpu-id-mask-city-field-a",
                width,
                height,
            );
            let city_field_b_texture = create_id_mask_field_texture(
                &self.device,
                "oxide-webgpu-id-mask-city-field-b",
                width,
                height,
            );
            let seam_field_a_texture = create_id_mask_field_texture(
                &self.device,
                "oxide-webgpu-id-mask-seam-field-a",
                width,
                height,
            );
            let seam_field_b_texture = create_id_mask_field_texture(
                &self.device,
                "oxide-webgpu-id-mask-seam-field-b",
                width,
                height,
            );
            self.id_mask_city_view =
                Some(city_texture.create_view(&wgpu::TextureViewDescriptor::default()));
            self.id_mask_neighborhood_view =
                Some(neighborhood_texture.create_view(&wgpu::TextureViewDescriptor::default()));
            self.id_mask_city_field_a_view =
                Some(city_field_a_texture.create_view(&wgpu::TextureViewDescriptor::default()));
            self.id_mask_city_field_b_view =
                Some(city_field_b_texture.create_view(&wgpu::TextureViewDescriptor::default()));
            self.id_mask_seam_field_a_view =
                Some(seam_field_a_texture.create_view(&wgpu::TextureViewDescriptor::default()));
            self.id_mask_seam_field_b_view =
                Some(seam_field_b_texture.create_view(&wgpu::TextureViewDescriptor::default()));
            self.id_mask_city_texture = Some(city_texture);
            self.id_mask_neighborhood_texture = Some(neighborhood_texture);
            self.id_mask_city_field_a_texture = Some(city_field_a_texture);
            self.id_mask_city_field_b_texture = Some(city_field_b_texture);
            self.id_mask_seam_field_a_texture = Some(seam_field_a_texture);
            self.id_mask_seam_field_b_texture = Some(seam_field_b_texture);
            self.id_mask_width = width;
            self.id_mask_height = height;
            self.id_mask_field_bind_group_a = None;
            self.id_mask_field_bind_group_b = None;
            self.id_mask_compositor_bind_group_a = None;
            self.id_mask_compositor_bind_group_b = None;
            self.stats.texture_creates = self.stats.texture_creates.saturating_add(6);
            self.stats.id_mask_texture_creates =
                self.stats.id_mask_texture_creates.saturating_add(6);
        }

        if self.id_mask_raster_uniform_buffer.is_none() {
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("oxide-webgpu-id-mask-raster-uniforms"),
                size: ID_MASK_RASTER_UNIFORM_SIZE,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("oxide-webgpu-id-mask-raster-bind-group"),
                layout: &self.programs.id_mask_raster_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            });
            self.id_mask_raster_uniform_buffer = Some(buffer);
            self.id_mask_raster_bind_group = Some(bind_group);
            self.stats.buffer_grows = self.stats.buffer_grows.saturating_add(1);
            self.stats.bind_group_creates = self.stats.bind_group_creates.saturating_add(1);
            self.stats.id_mask_buffer_grows = self.stats.id_mask_buffer_grows.saturating_add(1);
            self.stats.id_mask_bind_group_creates =
                self.stats.id_mask_bind_group_creates.saturating_add(1);
        }

        if self.id_mask_field_uniform_buffer.is_none() {
            self.id_mask_field_uniform_buffer =
                Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("oxide-webgpu-id-mask-field-uniforms"),
                    size: ID_MASK_FIELD_UNIFORM_SIZE,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
            self.id_mask_field_bind_group_a = None;
            self.id_mask_field_bind_group_b = None;
            self.stats.buffer_grows = self.stats.buffer_grows.saturating_add(1);
            self.stats.id_mask_buffer_grows = self.stats.id_mask_buffer_grows.saturating_add(1);
        }

        let compositor_needed = compositor_uniform_len.max(1) as u64;
        if self.id_mask_compositor_uniform_buffer.is_none()
            || self.id_mask_compositor_uniform_capacity < compositor_needed
        {
            let capacity = compositor_needed.next_power_of_two();
            self.id_mask_compositor_uniform_buffer =
                Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("oxide-webgpu-id-mask-compositor-uniforms"),
                    size: capacity,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
            self.id_mask_compositor_uniform_capacity = capacity;
            self.id_mask_compositor_bind_group_a = None;
            self.id_mask_compositor_bind_group_b = None;
            self.stats.buffer_grows = self.stats.buffer_grows.saturating_add(1);
            self.stats.id_mask_buffer_grows = self.stats.id_mask_buffer_grows.saturating_add(1);
        }

        if self.id_mask_field_bind_group_a.is_none() || self.id_mask_field_bind_group_b.is_none() {
            let Some(uniform_buffer) = self.id_mask_field_uniform_buffer.as_ref() else {
                return;
            };
            let Some(city_view) = self.id_mask_city_view.as_ref() else { return };
            let Some(neighborhood_view) = self.id_mask_neighborhood_view.as_ref() else { return };
            let Some(city_field_a_view) = self.id_mask_city_field_a_view.as_ref() else { return };
            let Some(city_field_b_view) = self.id_mask_city_field_b_view.as_ref() else { return };
            let Some(seam_field_a_view) = self.id_mask_seam_field_a_view.as_ref() else { return };
            let Some(seam_field_b_view) = self.id_mask_seam_field_b_view.as_ref() else { return };
            self.id_mask_field_bind_group_a = Some(create_id_mask_field_bind_group(
                &self.device,
                &self.programs.id_mask_field_layout,
                uniform_buffer,
                city_view,
                neighborhood_view,
                city_field_a_view,
                seam_field_a_view,
                "oxide-webgpu-id-mask-field-bind-group-a",
            ));
            self.id_mask_field_bind_group_b = Some(create_id_mask_field_bind_group(
                &self.device,
                &self.programs.id_mask_field_layout,
                uniform_buffer,
                city_view,
                neighborhood_view,
                city_field_b_view,
                seam_field_b_view,
                "oxide-webgpu-id-mask-field-bind-group-b",
            ));
            self.stats.bind_group_creates = self.stats.bind_group_creates.saturating_add(2);
            self.stats.id_mask_bind_group_creates =
                self.stats.id_mask_bind_group_creates.saturating_add(2);
        }

        if self.id_mask_compositor_bind_group_a.is_none()
            || self.id_mask_compositor_bind_group_b.is_none()
        {
            let Some(uniform_buffer) = self.id_mask_compositor_uniform_buffer.as_ref() else {
                return;
            };
            let Some(city_view) = self.id_mask_city_view.as_ref() else { return };
            let Some(neighborhood_view) = self.id_mask_neighborhood_view.as_ref() else { return };
            let Some(city_field_a_view) = self.id_mask_city_field_a_view.as_ref() else { return };
            let Some(city_field_b_view) = self.id_mask_city_field_b_view.as_ref() else { return };
            let Some(seam_field_a_view) = self.id_mask_seam_field_a_view.as_ref() else { return };
            let Some(seam_field_b_view) = self.id_mask_seam_field_b_view.as_ref() else { return };
            self.id_mask_compositor_bind_group_a = Some(create_id_mask_compositor_bind_group(
                &self.device,
                &self.programs.id_mask_compositor_layout,
                uniform_buffer,
                city_view,
                neighborhood_view,
                city_field_a_view,
                seam_field_a_view,
                "oxide-webgpu-id-mask-compositor-bind-group-a",
            ));
            self.id_mask_compositor_bind_group_b = Some(create_id_mask_compositor_bind_group(
                &self.device,
                &self.programs.id_mask_compositor_layout,
                uniform_buffer,
                city_view,
                neighborhood_view,
                city_field_b_view,
                seam_field_b_view,
                "oxide-webgpu-id-mask-compositor-bind-group-b",
            ));
            self.stats.bind_group_creates = self.stats.bind_group_creates.saturating_add(2);
            self.stats.id_mask_bind_group_creates =
                self.stats.id_mask_bind_group_creates.saturating_add(2);
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
        self.scene3d_uniform_bytes.clear();
        self.scene3d_draws.clear();
        self.scene3d_overlay_draws.clear();
        self.id_mask_draws.clear();
        self.id_mask_draw_chunk_indices.clear();
        self.scene3d_clear_color = None;
        self.scene3d_clear_depth = true;
        self.scene3d_active = false;
        self.clip_stack.clear();
        self.target_stack.clear();
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
            return Err(api::RenderError::InvalidOperation("frame token mismatch"));
        }
        self.active_token = None;
        let alloc_before = oxide_wasm_alloc_counter::snapshot();
        self.upload_frame_buffers();
        self.upload_scene3d_uniforms();
        self.write_viewport_uniform();
        self.prepare_effect_uniforms();
        self.record_submit_allocation_stage(SubmitAllocationStage::Upload, alloc_before);

        let alloc_before = oxide_wasm_alloc_counter::snapshot();
        let surface_texture = match self.surface.get_current_texture() {
            Ok(texture) => texture,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                self.surface.get_current_texture().map_err(|_| api::RenderError::DeviceLost)?
            }
            Err(wgpu::SurfaceError::OutOfMemory) => return Err(api::RenderError::OutOfMemory),
            Err(_) => return Err(api::RenderError::DeviceLost),
        };
        let surface_view =
            surface_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.record_submit_allocation_stage(SubmitAllocationStage::Surface, alloc_before);

        let alloc_before = oxide_wasm_alloc_counter::snapshot();
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("oxide-webgpu-frame"),
        });
        self.stats.command_buffers = self.stats.command_buffers.saturating_add(1);
        self.record_submit_allocation_stage(SubmitAllocationStage::Encoder, alloc_before);

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

        let alloc_before = oxide_wasm_alloc_counter::snapshot();
        let timestamp_readback = self.prepare_timestamp_readback(&mut encoder);
        self.record_submit_allocation_stage(SubmitAllocationStage::Timestamp, alloc_before);

        let alloc_before = oxide_wasm_alloc_counter::snapshot();
        self.record_scratch_growth_stats();
        self.record_submit_allocation_stage(SubmitAllocationStage::ScratchStats, alloc_before);

        let alloc_before = oxide_wasm_alloc_counter::snapshot();
        let command_buffer = encoder.finish();
        self.queue.submit(core::iter::once(command_buffer));
        self.record_submit_allocation_stage(SubmitAllocationStage::FinishQueue, alloc_before);

        let alloc_before = oxide_wasm_alloc_counter::snapshot();
        surface_texture.present();
        self.record_submit_allocation_stage(SubmitAllocationStage::Present, alloc_before);

        let alloc_before = oxide_wasm_alloc_counter::snapshot();
        if let Some((slot_index, bytes)) = timestamp_readback {
            self.map_timestamp_readback(slot_index, bytes);
            self.apply_timestamp_stats();
        }
        self.record_submit_allocation_stage(SubmitAllocationStage::TimestampMap, alloc_before);
        Ok(())
    }

    fn resize(&mut self, width: u32, height: u32, scale: f32) -> Result<(), api::RenderError> {
        let width = width.max(1);
        let height = height.max(1);
        let scale = sanitize_scale(scale);
        if self.width == width
            && self.height == height
            && (self.scale - scale).abs() <= f32::EPSILON
        {
            return Ok(());
        }
        self.width = width;
        self.height = height;
        self.scale = scale;
        self.canvas.set_width(self.width);
        self.canvas.set_height(self.height);
        self.config.width = self.width;
        self.config.height = self.height;
        self.surface.configure(&self.device, &self.config);
        self.recreate_targets();
        self.layers.clear();
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
            return index;
        }
        if let Some(index) = self.id_mask_reusable_vertex_cache_index() {
            let cache = &mut self.id_mask_vertex_caches[index];
            cache.key = key;
            write_id_mask_raster_vertex_bytes(vertices, &mut cache.bytes);
            cache.uploaded = false;
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

    fn id_mask_field_seed_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.programs.id_mask_field_seed_pipeline
    }

    fn id_mask_field_jump_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.programs.id_mask_field_jump_pipeline
    }

    fn id_mask_compositor_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.programs.id_mask_compositor_pipeline
    }

    fn render_direct(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        surface_view: &wgpu::TextureView,
    ) {
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

    fn render_scene_with_effects(&mut self, encoder: &mut wgpu::CommandEncoder) {
        if self.scene3d_active {
            let scene_view = self.scene_view.clone();
            self.render_scene3d(encoder, &scene_view);
        } else {
            self.clear_scene(encoder);
        }
        let scene_view = self.scene_view.clone();
        if !self.id_mask_draws.is_empty() {
            self.render_id_mask_compositors(encoder, &scene_view, wgpu::LoadOp::Load);
        }
        if !self.scene3d_overlay_draws.is_empty() {
            self.render_scene3d_overlay(encoder, &scene_view);
        }
        let scene_texture = self.scene_texture.clone();
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
                encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: target_texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyTextureInfo {
                        texture: &self.scratch_texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::Extent3d {
                        width: self.width,
                        height: self.height,
                        depth_or_array_layers: 1,
                    },
                );
                self.stats.texture_copies = self.stats.texture_copies.saturating_add(1);
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

    fn clear_scene(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let scene_view = self.scene_view.clone();
        self.clear_target(encoder, &scene_view, "oxide-webgpu-clear-scene");
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
        let timestamp_pair = self.reserve_timestamp_pass(TimestampPassFamily::Scene3d);
        let timestamp_writes = self.timestamp_writes(timestamp_pair);
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
                view: &self.scene_depth_view,
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
        let timestamp_pair = self.reserve_timestamp_pass(TimestampPassFamily::Scene3dOverlay);
        let timestamp_writes = self.timestamp_writes(timestamp_pair);
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
                view: &self.scene_depth_view,
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
        for draw_index in 0..self.id_mask_draws.len() {
            let draw = self.id_mask_draws[draw_index];
            let width = draw.mask_width.max(1);
            let height = draw.mask_height.max(1);
            let cache_start = draw.vertex_cache_first as usize;
            let cache_end = cache_start.saturating_add(draw.vertex_cache_count as usize);
            if draw.vertex_count == 0 || cache_end > self.id_mask_draw_chunk_indices.len() {
                continue;
            }
            write_id_mask_raster_uniform_bytes(
                &mut self.id_mask_raster_uniform_bytes,
                width,
                height,
                draw.projection,
            );
            write_id_mask_compositor_uniform_bytes(
                &mut self.id_mask_compositor_uniform_bytes,
                &draw,
            );
            let compositor_uniform_len = self.id_mask_compositor_uniform_bytes.len();

            for cache_pos in cache_start..cache_end {
                let cache_index = self.id_mask_draw_chunk_indices[cache_pos];
                if let Some(upload_bytes) = self.ensure_id_mask_vertex_cache_uploaded(cache_index) {
                    encoded_buffer_upload_bytes =
                        encoded_buffer_upload_bytes.saturating_add(upload_bytes);
                }
            }

            self.ensure_id_mask_resources(width, height, compositor_uniform_len);
            let Some(city_view) = self.id_mask_city_view.as_ref() else { continue };
            let Some(neighborhood_view) = self.id_mask_neighborhood_view.as_ref() else {
                continue;
            };
            let Some(city_field_a_view) = self.id_mask_city_field_a_view.as_ref() else {
                continue;
            };
            let Some(city_field_b_view) = self.id_mask_city_field_b_view.as_ref() else {
                continue;
            };
            let Some(seam_field_a_view) = self.id_mask_seam_field_a_view.as_ref() else {
                continue;
            };
            let Some(seam_field_b_view) = self.id_mask_seam_field_b_view.as_ref() else {
                continue;
            };
            let Some(raster_uniform_buffer) = self.id_mask_raster_uniform_buffer.as_ref() else {
                continue;
            };
            let Some(raster_bind_group) = self.id_mask_raster_bind_group.as_ref() else {
                continue;
            };
            let Some(field_uniform_buffer) = self.id_mask_field_uniform_buffer.as_ref() else {
                continue;
            };
            let Some(field_bind_group_a) = self.id_mask_field_bind_group_a.as_ref() else {
                continue;
            };
            let Some(field_bind_group_b) = self.id_mask_field_bind_group_b.as_ref() else {
                continue;
            };
            let Some(compositor_uniform_buffer) = self.id_mask_compositor_uniform_buffer.as_ref()
            else {
                continue;
            };
            let Some(compositor_bind_group_a) = self.id_mask_compositor_bind_group_a.as_ref()
            else {
                continue;
            };
            let Some(compositor_bind_group_b) = self.id_mask_compositor_bind_group_b.as_ref()
            else {
                continue;
            };

            self.queue.write_buffer(raster_uniform_buffer, 0, &self.id_mask_raster_uniform_bytes);
            encoded_buffer_upload_bytes = encoded_buffer_upload_bytes
                .saturating_add(self.id_mask_raster_uniform_bytes.len() as u64);
            self.queue.write_buffer(
                compositor_uniform_buffer,
                0,
                &self.id_mask_compositor_uniform_bytes,
            );
            encoded_buffer_upload_bytes = encoded_buffer_upload_bytes
                .saturating_add(self.id_mask_compositor_uniform_bytes.len() as u64);

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
                            view: &city_view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                        }),
                        Some(wgpu::RenderPassColorAttachment {
                            view: &neighborhood_view,
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
                pass.set_bind_group(0, raster_bind_group, &[]);
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
            let seed_field_uniform = id_mask_field_uniform_bytes(width, height, 0.0);
            self.queue.write_buffer(field_uniform_buffer, 0, &seed_field_uniform);
            encoded_buffer_upload_bytes =
                encoded_buffer_upload_bytes.saturating_add(seed_field_uniform.len() as u64);
            {
                let timestamp_pair = reserve_webgpu_timestamp_pass(
                    &mut self.timestamp_queries,
                    TimestampPassFamily::IdMaskFieldSeed,
                );
                let timestamp_writes =
                    webgpu_timestamp_writes(&self.timestamp_queries, timestamp_pair);
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("oxide-webgpu-id-mask-field-seed-pass"),
                    color_attachments: &[
                        Some(wgpu::RenderPassColorAttachment {
                            view: city_field_a_view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                        }),
                        Some(wgpu::RenderPassColorAttachment {
                            view: seam_field_a_view,
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
                pass.set_pipeline(self.id_mask_field_seed_pipeline());
                pass.set_bind_group(0, field_bind_group_b, &[]);
                pass.draw(0..6, 0..1);
            }
            encoded_render_passes = encoded_render_passes.saturating_add(1);
            self.stats.id_mask_field_seed_passes =
                self.stats.id_mask_field_seed_passes.saturating_add(1);

            let mut src_is_a = true;
            let mut jump = width.max(height).next_power_of_two() / 2;
            while jump >= 1 {
                let jump_field_uniform = id_mask_field_uniform_bytes(width, height, jump as f32);
                self.queue.write_buffer(field_uniform_buffer, 0, &jump_field_uniform);
                encoded_buffer_upload_bytes =
                    encoded_buffer_upload_bytes.saturating_add(jump_field_uniform.len() as u64);
                let (src_bind_group, dst_city_view, dst_seam_view) = if src_is_a {
                    (field_bind_group_a, city_field_b_view, seam_field_b_view)
                } else {
                    (field_bind_group_b, city_field_a_view, seam_field_a_view)
                };
                {
                    let timestamp_pair = reserve_webgpu_timestamp_pass(
                        &mut self.timestamp_queries,
                        TimestampPassFamily::IdMaskFieldJump,
                    );
                    let timestamp_writes =
                        webgpu_timestamp_writes(&self.timestamp_queries, timestamp_pair);
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("oxide-webgpu-id-mask-field-jump-pass"),
                        color_attachments: &[
                            Some(wgpu::RenderPassColorAttachment {
                                view: dst_city_view,
                                depth_slice: None,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                    store: wgpu::StoreOp::Store,
                                },
                            }),
                            Some(wgpu::RenderPassColorAttachment {
                                view: dst_seam_view,
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
                    pass.set_pipeline(self.id_mask_field_jump_pipeline());
                    pass.set_bind_group(0, src_bind_group, &[]);
                    pass.draw(0..6, 0..1);
                }
                encoded_render_passes = encoded_render_passes.saturating_add(1);
                self.stats.id_mask_field_jump_passes =
                    self.stats.id_mask_field_jump_passes.saturating_add(1);
                src_is_a = !src_is_a;
                jump /= 2;
            }
            let compositor_bind_group =
                if src_is_a { compositor_bind_group_a } else { compositor_bind_group_b };

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
                pass.set_pipeline(self.id_mask_compositor_pipeline());
                pass.set_bind_group(0, compositor_bind_group, &[]);
                pass.draw(0..6, 0..1);
            }
            encoded_render_passes = encoded_render_passes.saturating_add(1);
            self.stats.id_mask_compositor_passes =
                self.stats.id_mask_compositor_passes.saturating_add(1);
            load = wgpu::LoadOp::Load;
            encoded_draws = encoded_draws
                .saturating_add(3)
                .saturating_add(width.max(height).next_power_of_two().trailing_zeros());
        }
        self.stats.draws = self.stats.draws.saturating_add(encoded_draws);
        self.stats.render_passes = self.stats.render_passes.saturating_add(encoded_render_passes);
        self.stats.buffer_upload_bytes =
            self.stats.buffer_upload_bytes.saturating_add(encoded_buffer_upload_bytes);
    }

    fn draw_state_key(&self, draw: GpuDraw) -> Option<DrawStateKey> {
        let (pipeline, bind) = match draw.kind {
            DrawKind::Solid => (DrawPipelineKey::Solid, DrawBindKey::None),
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
        if self.vertex_buffer.is_none() || self.index_buffer.is_none() {
            return;
        }
        if !self.frame.draws[start..end].iter().any(|draw| draw.target == target) {
            return;
        }
        self.stats.render_passes = self.stats.render_passes.saturating_add(1);
        self.stats.draw_passes = self.stats.draw_passes.saturating_add(1);

        let timestamp_pair = self.reserve_timestamp_pass(TimestampPassFamily::Draw);
        let timestamp_writes = self.timestamp_writes(timestamp_pair);
        let Some(vertex_buffer) = &self.vertex_buffer else {
            return;
        };
        let Some(index_buffer) = &self.index_buffer else {
            return;
        };
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
        pass.set_bind_group(0, &self.viewport_bind_group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);

        let mut encoded_draws = 0_u32;
        let mut draw_items = 0_u32;
        let mut pipeline_binds = 0_u32;
        let mut draw_bind_group_binds = 0_u32;
        let mut scissor_sets = 0_u32;
        let mut bound_pipeline: Option<DrawPipelineKey> = None;
        let mut bound_bind: Option<DrawBindKey> = None;
        let mut bound_clip: Option<api::RectI> = None;
        for draw_index in start..end {
            let draw = self.frame.draws[draw_index];
            if draw.target != target {
                continue;
            }
            let Some(state) = self.draw_state_key(draw) else {
                continue;
            };
            let force_bind = !self.draw_state_cache_enabled;
            draw_items = draw_items.saturating_add(1);
            if force_bind || bound_clip != Some(state.clip) {
                set_scissor(&mut pass, state.clip, self.scale, self.width, self.height);
                scissor_sets = scissor_sets.saturating_add(1);
                bound_clip = Some(state.clip);
            }
            if force_bind || bound_pipeline != Some(state.pipeline) {
                match state.pipeline {
                    DrawPipelineKey::Solid => pass.set_pipeline(self.solid_pipeline()),
                    DrawPipelineKey::Rgba => pass.set_pipeline(self.rgba_pipeline()),
                    DrawPipelineKey::A8 => pass.set_pipeline(self.a8_pipeline()),
                    DrawPipelineKey::Sdf => pass.set_pipeline(self.sdf_pipeline()),
                    DrawPipelineKey::Effect => pass.set_pipeline(self.effect_pipeline()),
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
                        pass.set_bind_group(1, &self.scratch_bind_group, &[]);
                        pass.set_bind_group(2, &self.effect_bind_group, &[offset]);
                        draw_bind_group_binds = draw_bind_group_binds.saturating_add(2);
                    }
                }
                bound_bind = Some(state.bind);
            }
            if force_bind && matches!(state.bind, DrawBindKey::None) {
                bound_bind = None;
            }
            pass.draw_indexed(draw.first_index..draw.first_index + draw.index_count, 0, 0..1);
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
        pass.set_bind_group(0, &self.viewport_bind_group, &[]);
        pass.set_bind_group(1, &self.scene_bind_group, &[]);
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

fn create_programs(device: &wgpu::Device, format: wgpu::TextureFormat) -> GpuPrograms {
    let viewport_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("oxide-webgpu-viewport-layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
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
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let id_mask_field_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("oxide-webgpu-id-mask-field-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
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
    let id_mask_compositor_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("oxide-webgpu-id-mask-compositor-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
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
    let id_mask_field_pipeline_layout =
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("oxide-webgpu-id-mask-field-pipeline-layout"),
            bind_group_layouts: &[&id_mask_field_layout],
            push_constant_ranges: &[],
        });
    let id_mask_compositor_pipeline_layout =
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("oxide-webgpu-id-mask-compositor-pipeline-layout"),
            bind_group_layouts: &[&id_mask_compositor_layout],
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
    let id_mask_field_seed_pipeline = create_id_mask_field_pipeline(
        device,
        &id_mask_field_shader,
        &id_mask_field_pipeline_layout,
        "fs_id_mask_field_seed",
        "oxide-webgpu-id-mask-field-seed",
    );
    let id_mask_field_jump_pipeline = create_id_mask_field_pipeline(
        device,
        &id_mask_field_shader,
        &id_mask_field_pipeline_layout,
        "fs_id_mask_field_jump",
        "oxide-webgpu-id-mask-field-jump",
    );
    let id_mask_compositor_pipeline = create_id_mask_compositor_pipeline(
        device,
        &id_mask_shader,
        &id_mask_compositor_pipeline_layout,
        format,
    );

    GpuPrograms {
        viewport_layout,
        texture_layout,
        effect_layout,
        scene3d_layout,
        id_mask_raster_layout,
        id_mask_field_layout,
        id_mask_compositor_layout,
        solid_pipeline,
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
        id_mask_field_seed_pipeline,
        id_mask_field_jump_pipeline,
        id_mask_compositor_pipeline,
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
) -> wgpu::RenderPipeline {
    let color_target = [Some(wgpu::ColorTargetState {
        format,
        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
        write_mask: wgpu::ColorWrites::ALL,
    })];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("oxide-webgpu-id-mask-compositor"),
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
            entry_point: Some("fs_id_mask_compositor"),
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
) -> wgpu::RenderPipeline {
    let color_targets = [
        Some(wgpu::ColorTargetState {
            format: ID_MASK_FIELD_FORMAT,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        }),
        Some(wgpu::ColorTargetState {
            format: ID_MASK_FIELD_FORMAT,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        }),
    ];
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
            targets: &color_targets,
        }),
        multiview: None,
        cache: None,
    })
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
            format: wgpu::VertexFormat::Float32x4,
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
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("oxide-webgpu-viewport-bind-group"),
        layout: &programs.viewport_layout,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() }],
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

fn create_id_mask_texture(
    device: &wgpu::Device,
    label: &'static str,
    width: u32,
    height: u32,
) -> wgpu::Texture {
    create_texture_2d(
        device,
        label,
        wgpu::TextureFormat::R8Uint,
        width,
        height,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
    )
}

fn create_id_mask_field_texture(
    device: &wgpu::Device,
    label: &'static str,
    width: u32,
    height: u32,
) -> wgpu::Texture {
    create_texture_2d(
        device,
        label,
        ID_MASK_FIELD_FORMAT,
        width,
        height,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
    )
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

fn create_id_mask_field_bind_group(
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
            wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
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

fn create_id_mask_compositor_bind_group(
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
            wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
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
    width: u32,
    height: u32,
) {
    let scale = sanitize_scale(scale);
    let x = ((clip.x.max(0) as f32) * scale).floor() as u32;
    let y = ((clip.y.max(0) as f32) * scale).floor() as u32;
    let w = ((clip.w.max(0) as f32) * scale).ceil() as u32;
    let h = ((clip.h.max(0) as f32) * scale).ceil() as u32;
    let x = x.min(width.saturating_sub(1));
    let y = y.min(height.saturating_sub(1));
    let w = w.min(width.saturating_sub(x)).max(1);
    let h = h.min(height.saturating_sub(y)).max(1);
    pass.set_scissor_rect(x, y, w, h);
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
) -> [GpuVertex; 4] {
    [
        gpu_vertex(rect.x, rect.y, u0, v0, 0, color),
        gpu_vertex(rect.x + rect.w, rect.y, u1, v0, 0, color),
        gpu_vertex(rect.x, rect.y + rect.h, u0, v1, 0, color),
        gpu_vertex(rect.x + rect.w, rect.y + rect.h, u1, v1, 0, color),
    ]
}

fn rounded_rect_mesh_into(
    rect: api::RectF,
    radii: [f32; 4],
    color: api::Color,
    points: &mut Vec<(f32, f32)>,
    vertices: &mut Vec<GpuVertex>,
    indices: &mut Vec<u32>,
) {
    points.clear();
    vertices.clear();
    indices.clear();
    if rect.w <= 0.0 || rect.h <= 0.0 || color.a <= 0.0 {
        return;
    }
    let max_r = (rect.w.abs() * 0.5).min(rect.h.abs() * 0.5);
    let radii = [
        radii[0].clamp(0.0, max_r),
        radii[1].clamp(0.0, max_r),
        radii[2].clamp(0.0, max_r),
        radii[3].clamp(0.0, max_r),
    ];
    append_arc(
        points,
        rect.x + radii[0],
        rect.y + radii[0],
        radii[0],
        core::f32::consts::PI,
        1.5 * core::f32::consts::PI,
    );
    append_arc(
        points,
        rect.x + rect.w - radii[1],
        rect.y + radii[1],
        radii[1],
        1.5 * core::f32::consts::PI,
        2.0 * core::f32::consts::PI,
    );
    append_arc(
        points,
        rect.x + rect.w - radii[2],
        rect.y + rect.h - radii[2],
        radii[2],
        0.0,
        0.5 * core::f32::consts::PI,
    );
    append_arc(
        points,
        rect.x + radii[3],
        rect.y + rect.h - radii[3],
        radii[3],
        0.5 * core::f32::consts::PI,
        core::f32::consts::PI,
    );
    if points.len() < 3 {
        vertices.extend_from_slice(&quad_vertices(rect, 0.0, 0.0, 1.0, 1.0, color));
        indices.extend_from_slice(&[0, 1, 2, 2, 1, 3]);
        return;
    }
    vertices.reserve(points.len() + 1);
    vertices.push(gpu_vertex(rect.x + rect.w * 0.5, rect.y + rect.h * 0.5, 0.5, 0.5, 0, color));
    for (x, y) in points.iter().copied() {
        vertices.push(gpu_vertex(x, y, 0.0, 0.0, 0, color));
    }
    indices.reserve((vertices.len() - 1) * 3);
    for idx in 1..vertices.len() {
        indices.push(0);
        indices.push(idx as u32);
        indices.push(if idx + 1 < vertices.len() { idx as u32 + 1 } else { 1 });
    }
}

fn append_arc(points: &mut Vec<(f32, f32)>, cx: f32, cy: f32, radius: f32, start: f32, end: f32) {
    if radius <= 0.0 {
        points.push((cx, cy));
        return;
    }
    const SEGMENTS: usize = 8;
    for step in 0..=SEGMENTS {
        let t = step as f32 / SEGMENTS as f32;
        let angle = start + (end - start) * t;
        points.push((cx + angle.cos() * radius, cy + angle.sin() * radius));
    }
}

fn gpu_vertex(x: f32, y: f32, u: f32, v: f32, rgba: u32, uniform: api::Color) -> GpuVertex {
    let color = resolve_vertex_color(rgba, uniform);
    GpuVertex {
        x,
        y,
        u,
        v,
        r: color.r.clamp(0.0, 1.0),
        g: color.g.clamp(0.0, 1.0),
        b: color.b.clamp(0.0, 1.0),
        a: color.a.clamp(0.0, 1.0),
    }
}

fn append_gpu_vertices(
    out: &mut Vec<GpuVertex>,
    idx: &mut Vec<u32>,
    vertices: &[api::Vertex],
    color: api::Color,
    preserve_vertex_color: bool,
) {
    let base = out.len() as u32;
    out.extend(
        vertices.iter().map(|vertex| {
            gpu_vertex(
                vertex.x,
                vertex.y,
                vertex.u,
                vertex.v,
                if preserve_vertex_color { vertex.rgba } else { 0 },
                color,
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
    out: &mut Vec<GpuVertex>,
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
    out.extend(
        vertices.iter().map(|vertex| {
            gpu_vertex(
                vertex.x,
                vertex.y,
                vertex.u,
                vertex.v,
                if preserve_vertex_color { vertex.rgba } else { 0 },
                color,
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
    out: &mut Vec<GpuVertex>,
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
    out.extend(
        vertices.iter().map(|vertex| {
            gpu_vertex(
                vertex.x,
                vertex.y,
                vertex.u,
                vertex.v,
                if preserve_vertex_color { vertex.rgba } else { 0 },
                color,
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
    out.clear();
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
    out.clear();
    out.reserve(16 * (4 + 4 + 4 + 4 + id_mask_compositor::ID_MASK_MAX_NEIGHBORHOOD_COLORS));
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

fn encode_vertices(vertices: &[GpuVertex], out: &mut Vec<u8>) {
    out.reserve(vertices.len().saturating_mul(VERTEX_STRIDE as usize));
    for vertex in vertices {
        push_f32(out, vertex.x);
        push_f32(out, vertex.y);
        push_f32(out, vertex.u);
        push_f32(out, vertex.v);
        push_f32(out, vertex.r);
        push_f32(out, vertex.g);
        push_f32(out, vertex.b);
        push_f32(out, vertex.a);
    }
}

fn encode_indices(indices: &[u32], out: &mut Vec<u8>) {
    out.reserve(indices.len().saturating_mul(4));
    for index in indices {
        out.extend_from_slice(&index.to_le_bytes());
    }
}

fn f32x4_bytes(values: [f32; 4]) -> [u8; 16] {
    let mut out = [0; 16];
    let mut offset = 0;
    for value in values {
        write_f32(&mut out, &mut offset, value);
    }
    out
}

fn vertex4_bytes(vertices: &[GpuVertex; 4]) -> [u8; VERTEX_STRIDE_BYTES * 4] {
    let mut out = [0; VERTEX_STRIDE_BYTES * 4];
    let mut offset = 0;
    for vertex in vertices {
        write_f32(&mut out, &mut offset, vertex.x);
        write_f32(&mut out, &mut offset, vertex.y);
        write_f32(&mut out, &mut offset, vertex.u);
        write_f32(&mut out, &mut offset, vertex.v);
        write_f32(&mut out, &mut offset, vertex.r);
        write_f32(&mut out, &mut offset, vertex.g);
        write_f32(&mut out, &mut offset, vertex.b);
        write_f32(&mut out, &mut offset, vertex.a);
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
   let local = (input.pos - origin) / size;
   var out: VertexOut;
   out.pos = vec4<f32>(local.x * 2.0 - 1.0, 1.0 - local.y * 2.0, 0.0, 1.0);
   out.uv = input.uv;
   out.color = input.color;
   return out;
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
   let coverage = textureSample(source_tex, source_sampler, input.uv).a;
   return vec4<f32>(input.color.rgb, input.color.a * coverage);
}

@fragment
fn fs_sdf(input: VertexOut) -> @location(0) vec4<f32> {
   let distance = textureSample(source_tex, source_sampler, input.uv).a;
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
"#;
