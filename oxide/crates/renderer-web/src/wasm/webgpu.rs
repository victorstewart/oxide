use super::{
   a8_to_rgba, copy_a8_rows, copy_rgba_rows, document, index_slice, normalized_index_mode,
   resolve_index, sanitize_scale, source_rect, vertex_slice,
};
use crate::WebRendererStats;
use js_sys::Reflect;
use oxide_renderer_api as api;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::HtmlCanvasElement;

const VERTEX_STRIDE: wgpu::BufferAddress = 32;
const VERTEX_STRIDE_BYTES: usize = 32;
const MAX_BLUR_SIGMA: f32 = 96.0;

#[derive(Clone, Copy)]
struct GpuVertex
{
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
enum GpuImageKind
{
   Rgba,
   A8,
}

struct GpuImage
{
   texture: wgpu::Texture,
   bind_group: wgpu::BindGroup,
   width: u32,
   height: u32,
   kind: GpuImageKind,
}

#[derive(Clone, Copy)]
enum DrawKind
{
   Solid,
   Rgba { image: usize },
   A8 { image: usize },
   Sdf { image: usize },
   Backdrop { sigma: f32 },
}

#[derive(Clone, Copy)]
struct GpuDraw
{
   kind: DrawKind,
   first_index: u32,
   index_count: u32,
   clip: api::RectI,
}

struct FrameData
{
   vertices: Vec<GpuVertex>,
   indices: Vec<u32>,
   draws: Vec<GpuDraw>,
}

impl FrameData
{
   fn clear(&mut self)
   {
      self.vertices.clear();
      self.indices.clear();
      self.draws.clear();
   }
}

struct GpuPrograms
{
   viewport_layout: wgpu::BindGroupLayout,
   texture_layout: wgpu::BindGroupLayout,
   effect_layout: wgpu::BindGroupLayout,
   solid_pipeline: wgpu::RenderPipeline,
   rgba_pipeline: wgpu::RenderPipeline,
   a8_pipeline: wgpu::RenderPipeline,
   sdf_pipeline: wgpu::RenderPipeline,
   effect_pipeline: wgpu::RenderPipeline,
   sampler: wgpu::Sampler,
}

/// Browser renderer for production WebAssembly hosts.
///
/// WebGPU device creation is asynchronous in browsers. If WebGPU is unavailable, construction
/// returns `RenderError::Unsupported` instead of falling back to a CPU/Canvas2D visual path.
pub struct BrowserRenderer
{
   inner: WebGpuRenderer,
}

impl BrowserRenderer
{
   pub async fn from_canvas_id_webgpu(id: &str) -> Result<Self, api::RenderError>
   {
      let canvas = canvas_by_id(id)?;
      Self::from_canvas_webgpu(canvas).await
   }

   pub async fn from_canvas_webgpu(canvas: HtmlCanvasElement) -> Result<Self, api::RenderError>
   {
      if !browser_webgpu_present() {
         return Err(api::RenderError::Unsupported("webgpu unavailable"));
      }
      WebGpuRenderer::from_canvas(canvas).await.map(|inner| Self { inner })
   }

   #[must_use]
   pub fn backend_name(&self) -> &'static str
   {
      "webgpu"
   }

   #[must_use]
   pub fn canvas(&self) -> HtmlCanvasElement
   {
      self.inner.canvas()
   }

   #[must_use]
   pub fn last_stats(&self) -> WebRendererStats
   {
      self.inner.last_stats()
   }

   #[must_use]
   pub fn image_create_rgba8(&mut self, width: u32, height: u32, data: &[u8], row_bytes: usize) -> api::ImageHandle
   {
      self.inner.image_create_rgba8(width, height, data, row_bytes)
   }

   #[must_use]
   pub fn image_create_a8(&mut self, width: u32, height: u32, data: &[u8], row_bytes: usize) -> api::ImageHandle
   {
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
   )
   {
      self.inner.image_update_a8(handle, x, y, width, height, data, row_bytes);
   }

   pub fn set_camera_background_rgba8(
      &mut self,
      width: u32,
      height: u32,
      data: &[u8],
      row_bytes: usize,
   ) -> Result<(), api::RenderError>
   {
      self.inner.set_camera_background_rgba8(width, height, data, row_bytes)
   }
}

impl api::Renderer for BrowserRenderer
{
   fn device_caps(&self) -> api::DeviceCaps
   {
      self.inner.device_caps()
   }

   fn begin_frame(&mut self, fb: &api::FrameTarget, damage: Option<&api::Damage>) -> api::FrameToken
   {
      self.inner.begin_frame(fb, damage)
   }

   fn encode_pass(&mut self, list: &api::DrawList)
   {
      self.inner.encode_pass(list);
   }

   fn submit(&mut self, token: api::FrameToken) -> Result<(), api::RenderError>
   {
      self.inner.submit(token)
   }

   fn resize(&mut self, width: u32, height: u32, scale: f32) -> Result<(), api::RenderError>
   {
      self.inner.resize(width, height, scale)
   }
}

/// WebGPU implementation of the Oxide browser renderer.
pub struct WebGpuRenderer
{
   canvas: HtmlCanvasElement,
   surface: wgpu::Surface<'static>,
   device: wgpu::Device,
   queue: wgpu::Queue,
   config: wgpu::SurfaceConfiguration,
   programs: GpuPrograms,
   scene_texture: wgpu::Texture,
   scene_view: wgpu::TextureView,
   scene_bind_group: wgpu::BindGroup,
   scratch_texture: wgpu::Texture,
   scratch_view: wgpu::TextureView,
   scratch_bind_group: wgpu::BindGroup,
   viewport_buffer: wgpu::Buffer,
   viewport_bind_group: wgpu::BindGroup,
   effect_buffer: wgpu::Buffer,
   effect_bind_group: wgpu::BindGroup,
   vertex_buffer: Option<wgpu::Buffer>,
   vertex_capacity: u64,
   index_buffer: Option<wgpu::Buffer>,
   index_capacity: u64,
   present_vertex_buffer: wgpu::Buffer,
   present_index_buffer: wgpu::Buffer,
   vertex_bytes: Vec<u8>,
   index_bytes: Vec<u8>,
   images: Vec<Option<GpuImage>>,
   camera_background: Option<api::ImageHandle>,
   frame: FrameData,
   clip_stack: Vec<api::RectI>,
   width: u32,
   height: u32,
   scale: f32,
   frame_id: u64,
   active_token: Option<api::FrameToken>,
   stats: WebRendererStats,
}

impl WebGpuRenderer
{
   pub async fn from_canvas_id(id: &str) -> Result<Self, api::RenderError>
   {
      Self::from_canvas(canvas_by_id(id)?).await
   }

   pub async fn from_canvas(canvas: HtmlCanvasElement) -> Result<Self, api::RenderError>
   {
      let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
         backends: wgpu::Backends::BROWSER_WEBGPU,
         ..Default::default()
      });
      let surface = instance
         .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
         .map_err(|err| api::RenderError::Unsupported(match err {
            _ => "webgpu surface unavailable",
         }))?;
      let adapter = instance
         .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
         })
         .await
         .map_err(|_| api::RenderError::Unsupported("webgpu adapter unavailable"))?;
      let (device, queue) = adapter
         .request_device(&wgpu::DeviceDescriptor {
            label: Some("oxide-webgpu-device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            ..Default::default()
         })
         .await
         .map_err(|err| api::RenderError::Io(format!("webgpu device unavailable: {err}")))?;
      let width = canvas.width().max(1);
      let height = canvas.height().max(1);
      let mut config = surface
         .get_default_config(&adapter, width, height)
         .ok_or(api::RenderError::Unsupported("webgpu surface format unavailable"))?;
      config.width = width;
      config.height = height;
      config.usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
      surface.configure(&device, &config);

      let programs = create_programs(&device, config.format);
      let (scene_texture, scene_view, scene_bind_group) =
         create_target_texture(&device, &programs, "oxide-webgpu-scene", config.format, width, height);
      let (scratch_texture, scratch_view, scratch_bind_group) =
         create_target_texture(&device, &programs, "oxide-webgpu-scratch", config.format, width, height);
      let (viewport_buffer, viewport_bind_group) = create_viewport_bind_group(&device, &programs);
      let (effect_buffer, effect_bind_group) = create_effect_bind_group(&device, &programs);
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
         scratch_texture,
         scratch_view,
         scratch_bind_group,
         viewport_buffer,
         viewport_bind_group,
         effect_buffer,
         effect_bind_group,
         vertex_buffer: None,
         vertex_capacity: 0,
         index_buffer: None,
         index_capacity: 0,
         present_vertex_buffer,
         present_index_buffer,
         vertex_bytes: Vec::new(),
         index_bytes: Vec::new(),
         images: vec![None],
         camera_background: None,
         frame: FrameData { vertices: Vec::new(), indices: Vec::new(), draws: Vec::new() },
         clip_stack: Vec::new(),
         width,
         height,
         scale: 1.0,
         frame_id: 0,
         active_token: None,
         stats: WebRendererStats::default(),
      })
   }

   #[must_use]
   pub fn canvas(&self) -> HtmlCanvasElement
   {
      self.canvas.clone()
   }

   #[must_use]
   pub fn last_stats(&self) -> WebRendererStats
   {
      self.stats
   }

   #[must_use]
   pub fn image_create_rgba8(&mut self, width: u32, height: u32, data: &[u8], row_bytes: usize) -> api::ImageHandle
   {
      match self.try_image_create_rgba8(width, height, data, row_bytes) {
         Ok(handle) => handle,
         Err(_) => api::ImageHandle(0),
      }
   }

   #[must_use]
   pub fn image_create_a8(&mut self, width: u32, height: u32, data: &[u8], row_bytes: usize) -> api::ImageHandle
   {
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
   )
   {
      let _ = self.try_image_update_a8(handle, x, y, width, height, data, row_bytes);
   }

   pub fn set_camera_background_rgba8(
      &mut self,
      width: u32,
      height: u32,
      data: &[u8],
      row_bytes: usize,
   ) -> Result<(), api::RenderError>
   {
      if let Some(handle) = self.camera_background {
         let same_size = self
            .image(handle)
            .map(|image| image.width == width && image.height == height)
            .unwrap_or(false);
         if same_size {
            self.try_image_update_rgba8(handle, 0, 0, width, height, data, row_bytes)?;
            return Ok(());
         }
      }
      let handle = self.try_image_create_rgba8(width, height, data, row_bytes)?;
      self.camera_background = Some(handle);
      Ok(())
   }

   pub fn try_image_create_rgba8(
      &mut self,
      width: u32,
      height: u32,
      data: &[u8],
      row_bytes: usize,
   ) -> Result<api::ImageHandle, api::RenderError>
   {
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
   ) -> Result<api::ImageHandle, api::RenderError>
   {
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
   ) -> Result<(), api::RenderError>
   {
      let alpha = copy_a8_rows(width, height, data, row_bytes)
         .ok_or(api::RenderError::InvalidOperation("invalid a8 update rows"))?;
      self.update_image(handle, x, y, width, height, GpuImageKind::A8, &a8_to_rgba(&alpha))
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
   ) -> Result<(), api::RenderError>
   {
      let rgba = copy_rgba_rows(width, height, data, row_bytes)
         .ok_or(api::RenderError::InvalidOperation("invalid rgba update rows"))?;
      self.update_image(handle, x, y, width, height, GpuImageKind::Rgba, &rgba)
   }

   fn push_image(
      &mut self,
      width: u32,
      height: u32,
      kind: GpuImageKind,
      rgba: &[u8],
   ) -> Result<api::ImageHandle, api::RenderError>
   {
      let image = self.create_image(width, height, kind, rgba)?;
      let handle = api::ImageHandle(self.images.len() as u32);
      self.images.push(Some(image));
      Ok(handle)
   }

   fn create_image(
      &self,
      width: u32,
      height: u32,
      kind: GpuImageKind,
      rgba: &[u8],
   ) -> Result<GpuImage, api::RenderError>
   {
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
      let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
      let bind_group = create_texture_bind_group(&self.device, &self.programs, &view, &self.programs.sampler);
      drop(view);
      Ok(GpuImage { texture, bind_group, width, height, kind })
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
   ) -> Result<(), api::RenderError>
   {
      let Some(image) = self.image(handle) else {
         return Err(api::RenderError::ResourceNotFound("image handle not found"));
      };
      if core::mem::discriminant(&image.kind) != core::mem::discriminant(&kind) {
         return Err(api::RenderError::InvalidOperation("image kind mismatch"));
      }
      if x.saturating_add(width) > image.width || y.saturating_add(height) > image.height {
         return Err(api::RenderError::InvalidOperation("image update outside bounds"));
      }
      self.queue.write_texture(
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
      Ok(())
   }

   fn image(&self, handle: api::ImageHandle) -> Option<&GpuImage>
   {
      self.images.get(handle.0 as usize).and_then(Option::as_ref)
   }

   fn current_clip(&self) -> api::RectI
   {
      self.clip_stack
         .last()
         .copied()
         .unwrap_or_else(|| api::RectI::new(0, 0, logical_dimension(self.width, self.scale) as i32, logical_dimension(self.height, self.scale) as i32))
   }

   fn push_draw(&mut self, kind: DrawKind, vertices: &[GpuVertex], indices: &[u32])
   {
      if vertices.is_empty() || indices.is_empty() {
         return;
      }
      let base = self.frame.vertices.len() as u32;
      let first_index = self.frame.indices.len() as u32;
      self.frame.vertices.extend_from_slice(vertices);
      self.frame.indices.extend(indices.iter().map(|index| base.saturating_add(*index)));
      self.frame.draws.push(GpuDraw {
         kind,
         first_index,
         index_count: indices.len() as u32,
         clip: self.current_clip(),
      });
   }

   fn encode_items(&mut self, list: &api::DrawList, index: &mut usize, stop_at_layer_end: bool)
   {
      while *index < list.items.len() {
         match &list.items[*index] {
            api::DrawCmd::LayerBegin { .. } => {
               *index += 1;
               self.encode_items(list, index, true);
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

   fn encode_draw_cmd(&mut self, list: &api::DrawList, item: &api::DrawCmd)
   {
      match item {
         api::DrawCmd::LayerBegin { .. } | api::DrawCmd::LayerEnd => {}
         api::DrawCmd::Solid { vb, ib, color } => self.encode_solid(list, *vb, *ib, *color),
         api::DrawCmd::Image { tex, dst, src, alpha } => self.encode_image(*tex, *dst, *src, *alpha, false),
         api::DrawCmd::GlyphRun { run } => self.encode_glyph_run(list, run),
         api::DrawCmd::RRect { rect, radii, color } => self.encode_rrect(*rect, *radii, *color),
         api::DrawCmd::NineSlice { tex, rect, slice, alpha } => self.encode_nine_slice(*tex, *rect, *slice, *alpha),
         api::DrawCmd::Backdrop { rect, sigma, tint, alpha } => self.encode_backdrop(*rect, *sigma, *tint, *alpha),
         api::DrawCmd::VisualEffect { rect, effect } => {
            let tint = effect.tint();
            self.encode_backdrop(*rect, effect.blur_intensity() * 72.0, tint, tint.a);
         }
         api::DrawCmd::CameraBg { rect, tint, alpha, .. } => {
            if let Some(handle) = self.camera_background {
               self.encode_image(handle, *rect, api::RectF::new(0.0, 0.0, 0.0, 0.0), *alpha, false);
            }
            if tint.a > 0.0 {
               self.encode_rect(*rect, api::Color::rgba(tint.r, tint.g, tint.b, tint.a * alpha.clamp(0.0, 1.0)));
            }
         }
         api::DrawCmd::Spinner { center, atom, alpha } => self.encode_spinner(*center, *atom, *alpha),
         api::DrawCmd::ClipPush { rect } => self.clip_stack.push(*rect),
         api::DrawCmd::ClipPop => {
            let _ = self.clip_stack.pop();
         }
      }
   }

   fn encode_solid(&mut self, list: &api::DrawList, vb: api::VertexSpan, ib: api::IndexSpan, color: api::Color)
   {
      let Some(vertices) = vertex_slice(list, vb) else {
         return;
      };
      let mut out = Vec::new();
      let mut idx = Vec::new();
      if ib.len > 0 {
         let Some(indices) = index_slice(list, ib) else {
            return;
         };
         let Some(mode) = normalized_index_mode(indices, vb.offset, vb.len) else {
            return;
         };
         for tri in indices.chunks_exact(3) {
            for index in tri {
               if let Some(vertex) = resolve_index(*index, mode).and_then(|offset| vertices.get(offset)) {
                  idx.push(out.len() as u32);
                  out.push(gpu_vertex(vertex.x, vertex.y, vertex.u, vertex.v, color));
               }
            }
         }
      } else if vertices.len() == 4 {
         out.extend(vertices.iter().map(|vertex| gpu_vertex(vertex.x, vertex.y, vertex.u, vertex.v, color)));
         idx.extend_from_slice(&[0, 1, 2, 2, 1, 3]);
      } else {
         out.extend(vertices.iter().map(|vertex| gpu_vertex(vertex.x, vertex.y, vertex.u, vertex.v, color)));
         idx.extend(0..out.len() as u32);
      }
      self.push_draw(DrawKind::Solid, &out, &idx);
      self.stats.solid_tris = self.stats.solid_tris.saturating_add((idx.len() / 3) as u32);
   }

   fn encode_image(&mut self, handle: api::ImageHandle, dst: api::RectF, src: api::RectF, alpha: f32, sdf: bool)
   {
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
         (GpuImageKind::Rgba, _) => DrawKind::Rgba { image: handle.0 as usize },
         (GpuImageKind::A8, false) => DrawKind::A8 { image: handle.0 as usize },
         (GpuImageKind::A8, true) => DrawKind::Sdf { image: handle.0 as usize },
      };
      self.push_draw(kind, &vertices, &[0, 1, 2, 2, 1, 3]);
      self.stats.image_draws = self.stats.image_draws.saturating_add(1);
   }

   fn encode_glyph_run(&mut self, list: &api::DrawList, run: &api::GlyphRun)
   {
      let Some(vertices) = vertex_slice(list, run.vb) else {
         return;
      };
      let indices = index_slice(list, run.ib).unwrap_or(&[]);
      self.encode_glyph_vertices(run, vertices, indices);
   }

   fn encode_glyph_vertices(&mut self, run: &api::GlyphRun, vertices: &[api::Vertex], indices: &[u16])
   {
      let Some(image) = self.image(run.atlas) else {
         return;
      };
      let kind = if run.sdf {
         DrawKind::Sdf { image: run.atlas.0 as usize }
      } else {
         match image.kind {
            GpuImageKind::Rgba => DrawKind::Rgba { image: run.atlas.0 as usize },
            GpuImageKind::A8 => DrawKind::A8 { image: run.atlas.0 as usize },
         }
      };
      let mut out = Vec::new();
      let mut idx = Vec::new();
      if !indices.is_empty() {
         for index in indices {
            if let Some(vertex) = vertices.get(*index as usize) {
               idx.push(out.len() as u32);
               out.push(gpu_vertex(vertex.x, vertex.y, vertex.u, vertex.v, run.color));
            }
         }
      } else {
         out.extend(vertices.iter().map(|vertex| gpu_vertex(vertex.x, vertex.y, vertex.u, vertex.v, run.color)));
         idx.extend(0..out.len() as u32);
      }
      self.push_draw(kind, &out, &idx);
      self.stats.glyph_quads = self.stats.glyph_quads.saturating_add((idx.len() / 6) as u32);
   }

   fn encode_rect(&mut self, rect: api::RectF, color: api::Color)
   {
      if rect.w <= 0.0 || rect.h <= 0.0 || color.a <= 0.0 {
         return;
      }
      let vertices = quad_vertices(rect, 0.0, 0.0, 1.0, 1.0, color);
      self.push_draw(DrawKind::Solid, &vertices, &[0, 1, 2, 2, 1, 3]);
      self.stats.solid_tris = self.stats.solid_tris.saturating_add(2);
   }

   fn encode_rrect(&mut self, rect: api::RectF, radii: [f32; 4], color: api::Color)
   {
      let (vertices, indices) = rounded_rect_mesh(rect, radii, color);
      self.push_draw(DrawKind::Solid, &vertices, &indices);
      self.stats.solid_tris = self.stats.solid_tris.saturating_add((indices.len() / 3) as u32);
   }

   fn encode_nine_slice(&mut self, handle: api::ImageHandle, rect: api::RectF, slice: api::Insets, alpha: f32)
   {
      let Some(image) = self.image(handle) else {
         return;
      };
      let iw = image.width as f32;
      let ih = image.height as f32;
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
            let dst = api::RectF::new(dx[col], dy[row], dx[col + 1] - dx[col], dy[row + 1] - dy[row]);
            let src = api::RectF::new(sx[col], sy[row], sx[col + 1] - sx[col], sy[row + 1] - sy[row]);
            self.encode_image(handle, dst, src, alpha, false);
         }
      }
   }

   fn encode_backdrop(&mut self, rect: api::RectF, sigma: f32, tint: api::Color, alpha: f32)
   {
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
      self.push_draw(DrawKind::Backdrop { sigma: sigma.clamp(0.0, MAX_BLUR_SIGMA) }, &vertices, &[0, 1, 2, 2, 1, 3]);
   }

   fn encode_spinner(&mut self, center: [f32; 2], atom: f32, alpha: f32)
   {
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

   fn recreate_targets(&mut self)
   {
      let (scene_texture, scene_view, scene_bind_group) =
         create_target_texture(&self.device, &self.programs, "oxide-webgpu-scene", self.config.format, self.width, self.height);
      let (scratch_texture, scratch_view, scratch_bind_group) =
         create_target_texture(&self.device, &self.programs, "oxide-webgpu-scratch", self.config.format, self.width, self.height);
      self.scene_texture = scene_texture;
      self.scene_view = scene_view;
      self.scene_bind_group = scene_bind_group;
      self.scratch_texture = scratch_texture;
      self.scratch_view = scratch_view;
      self.scratch_bind_group = scratch_bind_group;
   }

   fn upload_frame_buffers(&mut self)
   {
      self.vertex_bytes.clear();
      self.index_bytes.clear();
      encode_vertices(&self.frame.vertices, &mut self.vertex_bytes);
      encode_indices(&self.frame.indices, &mut self.index_bytes);
      ensure_buffer(
         &self.device,
         &mut self.vertex_buffer,
         &mut self.vertex_capacity,
         self.vertex_bytes.len() as u64,
         wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
         "oxide-webgpu-vertices",
      );
      ensure_buffer(
         &self.device,
         &mut self.index_buffer,
         &mut self.index_capacity,
         self.index_bytes.len() as u64,
         wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
         "oxide-webgpu-indices",
      );
      if let Some(buffer) = &self.vertex_buffer {
         self.queue.write_buffer(buffer, 0, &self.vertex_bytes);
      }
      if let Some(buffer) = &self.index_buffer {
         self.queue.write_buffer(buffer, 0, &self.index_bytes);
      }
   }

   fn write_viewport_uniform(&self)
   {
      let logical_w = logical_dimension(self.width, self.scale).max(1.0);
      let logical_h = logical_dimension(self.height, self.scale).max(1.0);
      let bytes = f32x4_bytes([logical_w, logical_h, 0.0, 0.0]);
      self.queue.write_buffer(&self.viewport_buffer, 0, &bytes);
   }

   fn write_effect_uniform(&self, sigma: f32)
   {
      let radius = sigma.clamp(0.0, MAX_BLUR_SIGMA);
      let texel_x = 1.0 / self.width.max(1) as f32;
      let texel_y = 1.0 / self.height.max(1) as f32;
      let bytes = f32x4_bytes([texel_x, texel_y, radius, 0.0]);
      self.queue.write_buffer(&self.effect_buffer, 0, &bytes);
   }
}

impl api::Renderer for WebGpuRenderer
{
   fn device_caps(&self) -> api::DeviceCaps
   {
      api::DeviceCaps {
         max_framerate_hz: 60,
         supports_edr: false,
         supports_msaa4x: false,
         native_scale: self.scale,
      }
   }

   fn begin_frame(&mut self, _fb: &api::FrameTarget, damage: Option<&api::Damage>) -> api::FrameToken
   {
      self.frame_id = self.frame_id.wrapping_add(1);
      self.frame.clear();
      self.clip_stack.clear();
      self.stats = WebRendererStats {
         frame_id: self.frame_id,
         width: self.width,
         height: self.height,
         scale: self.scale,
         damage_rects: damage.map(|d| d.rects.len() as u32).unwrap_or(0),
         ..WebRendererStats::default()
      };
      let token = api::FrameToken(self.frame_id);
      self.active_token = Some(token);
      token
   }

   fn encode_pass(&mut self, list: &api::DrawList)
   {
      let mut index = 0;
      self.encode_items(list, &mut index, false);
   }

   fn submit(&mut self, token: api::FrameToken) -> Result<(), api::RenderError>
   {
      if self.active_token != Some(token) {
         return Err(api::RenderError::InvalidOperation("frame token mismatch"));
      }
      self.active_token = None;
      self.upload_frame_buffers();
      self.write_viewport_uniform();

      let surface_texture = match self.surface.get_current_texture() {
         Ok(texture) => texture,
         Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
            self.surface.configure(&self.device, &self.config);
            self.surface.get_current_texture().map_err(|_| api::RenderError::DeviceLost)?
         }
         Err(wgpu::SurfaceError::OutOfMemory) => return Err(api::RenderError::OutOfMemory),
         Err(_) => return Err(api::RenderError::DeviceLost),
      };
      let surface_view = surface_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
      let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
         label: Some("oxide-webgpu-frame"),
      });
      if self.frame_uses_backdrop() {
         self.render_scene_with_effects(&mut encoder);
         self.render_present(&mut encoder, &surface_view);
      } else {
         self.render_direct(&mut encoder, &surface_view);
      }
      self.queue.submit([encoder.finish()]);
      surface_texture.present();
      Ok(())
   }

   fn resize(&mut self, width: u32, height: u32, scale: f32) -> Result<(), api::RenderError>
   {
      self.width = width.max(1);
      self.height = height.max(1);
      self.scale = sanitize_scale(scale);
      self.canvas.set_width(self.width);
      self.canvas.set_height(self.height);
      let style = self.canvas.style();
      style
         .set_property("width", &format!("{}px", logical_dimension(self.width, self.scale).round().max(1.0)))
         .map_err(|err| js_error("canvas style width", err))?;
      style
         .set_property("height", &format!("{}px", logical_dimension(self.height, self.scale).round().max(1.0)))
         .map_err(|err| js_error("canvas style height", err))?;
      self.config.width = self.width;
      self.config.height = self.height;
      self.surface.configure(&self.device, &self.config);
      self.recreate_targets();
      Ok(())
   }
}

impl WebGpuRenderer
{
   fn frame_uses_backdrop(&self) -> bool
   {
      self.frame
         .draws
         .iter()
         .any(|draw| matches!(draw.kind, DrawKind::Backdrop { .. }))
   }

   fn render_direct(&mut self, encoder: &mut wgpu::CommandEncoder, surface_view: &wgpu::TextureView)
   {
      self.render_draw_range(
         encoder,
         surface_view,
         0,
         self.frame.draws.len(),
         wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
      );
   }

   fn render_scene_with_effects(&mut self, encoder: &mut wgpu::CommandEncoder)
   {
      self.clear_scene(encoder);
      let scene_view = self.scene_view.clone();
      let mut start = 0_usize;
      while start < self.frame.draws.len() {
         if let DrawKind::Backdrop { sigma } = self.frame.draws[start].kind {
            encoder.copy_texture_to_texture(
               wgpu::TexelCopyTextureInfo {
                  texture: &self.scene_texture,
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
               wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
            );
            self.write_effect_uniform(sigma);
            self.render_draw_range(encoder, &scene_view, start, start + 1, wgpu::LoadOp::Load);
            start += 1;
         } else {
            let mut end = start + 1;
            while end < self.frame.draws.len() && !matches!(self.frame.draws[end].kind, DrawKind::Backdrop { .. }) {
               end += 1;
            }
            self.render_draw_range(encoder, &scene_view, start, end, wgpu::LoadOp::Load);
            start = end;
         }
      }
   }

   fn clear_scene(&self, encoder: &mut wgpu::CommandEncoder)
   {
      let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
         label: Some("oxide-webgpu-clear-scene"),
         color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &self.scene_view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
         })],
         depth_stencil_attachment: None,
         timestamp_writes: None,
         occlusion_query_set: None,
      });
   }

   fn render_draw_range(
      &mut self,
      encoder: &mut wgpu::CommandEncoder,
      target_view: &wgpu::TextureView,
      start: usize,
      end: usize,
      load: wgpu::LoadOp<wgpu::Color>,
   )
   {
      let Some(vertex_buffer) = &self.vertex_buffer else {
         return;
      };
      let Some(index_buffer) = &self.index_buffer else {
         return;
      };
      if start >= end {
         return;
      }

      let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
         label: Some("oxide-webgpu-draw-pass"),
         color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target_view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
         })],
         depth_stencil_attachment: None,
         timestamp_writes: None,
         occlusion_query_set: None,
      });
      pass.set_bind_group(0, &self.viewport_bind_group, &[]);
      pass.set_vertex_buffer(0, vertex_buffer.slice(..));
      pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);

      for draw_index in start..end {
         let draw = self.frame.draws[draw_index];
         set_scissor(&mut pass, draw.clip, self.scale, self.width, self.height);
         match draw.kind {
            DrawKind::Solid => {
               pass.set_pipeline(&self.programs.solid_pipeline);
            }
            DrawKind::Rgba { image } => {
               let Some(image) = self.images.get(image).and_then(Option::as_ref) else {
                  continue;
               };
               pass.set_pipeline(&self.programs.rgba_pipeline);
               pass.set_bind_group(1, &image.bind_group, &[]);
            }
            DrawKind::A8 { image } => {
               let Some(image) = self.images.get(image).and_then(Option::as_ref) else {
                  continue;
               };
               pass.set_pipeline(&self.programs.a8_pipeline);
               pass.set_bind_group(1, &image.bind_group, &[]);
            }
            DrawKind::Sdf { image } => {
               let Some(image) = self.images.get(image).and_then(Option::as_ref) else {
                  continue;
               };
               pass.set_pipeline(&self.programs.sdf_pipeline);
               pass.set_bind_group(1, &image.bind_group, &[]);
            }
            DrawKind::Backdrop { .. } => {
               pass.set_pipeline(&self.programs.effect_pipeline);
               pass.set_bind_group(1, &self.scratch_bind_group, &[]);
               pass.set_bind_group(2, &self.effect_bind_group, &[]);
            }
         }
         pass.draw_indexed(draw.first_index..draw.first_index + draw.index_count, 0, 0..1);
         self.stats.draws = self.stats.draws.saturating_add(1);
      }
   }

   fn render_present(&self, encoder: &mut wgpu::CommandEncoder, surface_view: &wgpu::TextureView)
   {
      let vertices = quad_vertices(
         api::RectF::new(0.0, 0.0, logical_dimension(self.width, self.scale), logical_dimension(self.height, self.scale)),
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
      let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
         label: Some("oxide-webgpu-present-pass"),
         color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: surface_view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
         })],
         depth_stencil_attachment: None,
         timestamp_writes: None,
         occlusion_query_set: None,
      });
      pass.set_pipeline(&self.programs.rgba_pipeline);
      pass.set_bind_group(0, &self.viewport_bind_group, &[]);
      pass.set_bind_group(1, &self.scene_bind_group, &[]);
      pass.set_vertex_buffer(0, self.present_vertex_buffer.slice(..));
      pass.set_index_buffer(self.present_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
      pass.draw_indexed(0..6, 0, 0..1);
   }
}

fn canvas_by_id(id: &str) -> Result<HtmlCanvasElement, api::RenderError>
{
   let element = document()?
      .get_element_by_id(id)
      .ok_or(api::RenderError::ResourceNotFound("canvas id not found"))?;
   element
      .dyn_into::<HtmlCanvasElement>()
      .map_err(|_| api::RenderError::InvalidOperation("element is not a canvas"))
}

fn browser_webgpu_present() -> bool
{
   let Some(window) = web_sys::window() else {
      return false;
   };
   let navigator = window.navigator();
   Reflect::get(navigator.as_ref(), &JsValue::from_str("gpu"))
      .ok()
      .filter(|value| !value.is_undefined() && !value.is_null())
      .is_some()
}

fn create_programs(device: &wgpu::Device, format: wgpu::TextureFormat) -> GpuPrograms
{
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
            has_dynamic_offset: false,
            min_binding_size: None,
         },
         count: None,
      }],
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
   let vertex_layout = vertex_layout();
   let color_target = [Some(wgpu::ColorTargetState {
      format,
      blend: Some(wgpu::BlendState::ALPHA_BLENDING),
      write_mask: wgpu::ColorWrites::ALL,
   })];
   let solid_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
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

   let solid_pipeline = create_pipeline(device, &shader, &solid_layout, &vertex_layout, &color_target, "fs_solid");
   let rgba_pipeline = create_pipeline(device, &shader, &texture_pipeline_layout, &vertex_layout, &color_target, "fs_rgba");
   let a8_pipeline = create_pipeline(device, &shader, &texture_pipeline_layout, &vertex_layout, &color_target, "fs_a8");
   let sdf_pipeline = create_pipeline(device, &shader, &texture_pipeline_layout, &vertex_layout, &color_target, "fs_sdf");
   let effect_pipeline = create_pipeline(device, &shader, &effect_pipeline_layout, &vertex_layout, &color_target, "fs_backdrop");

   GpuPrograms {
      viewport_layout,
      texture_layout,
      effect_layout,
      solid_pipeline,
      rgba_pipeline,
      a8_pipeline,
      sdf_pipeline,
      effect_pipeline,
      sampler,
   }
}

fn create_pipeline(
   device: &wgpu::Device,
   shader: &wgpu::ShaderModule,
   layout: &wgpu::PipelineLayout,
   vertex_layout: &wgpu::VertexBufferLayout<'_>,
   color_target: &[Option<wgpu::ColorTargetState>],
   fragment: &'static str,
) -> wgpu::RenderPipeline
{
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

fn vertex_layout() -> wgpu::VertexBufferLayout<'static>
{
   const ATTRIBUTES: [wgpu::VertexAttribute; 3] = [
      wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 0, shader_location: 0 },
      wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 8, shader_location: 1 },
      wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 16, shader_location: 2 },
   ];
   wgpu::VertexBufferLayout {
      array_stride: VERTEX_STRIDE,
      step_mode: wgpu::VertexStepMode::Vertex,
      attributes: &ATTRIBUTES,
   }
}

fn create_viewport_bind_group(device: &wgpu::Device, programs: &GpuPrograms) -> (wgpu::Buffer, wgpu::BindGroup)
{
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

fn create_effect_bind_group(device: &wgpu::Device, programs: &GpuPrograms) -> (wgpu::Buffer, wgpu::BindGroup)
{
   let buffer = device.create_buffer(&wgpu::BufferDescriptor {
      label: Some("oxide-webgpu-effect-buffer"),
      size: 16,
      usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
      mapped_at_creation: false,
   });
   let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("oxide-webgpu-effect-bind-group"),
      layout: &programs.effect_layout,
      entries: &[wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() }],
   });
   (buffer, bind_group)
}

fn create_target_texture(
   device: &wgpu::Device,
   programs: &GpuPrograms,
   label: &'static str,
   format: wgpu::TextureFormat,
   width: u32,
   height: u32,
) -> (wgpu::Texture, wgpu::TextureView, wgpu::BindGroup)
{
   let texture = device.create_texture(&wgpu::TextureDescriptor {
      label: Some(label),
      size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
      mip_level_count: 1,
      sample_count: 1,
      dimension: wgpu::TextureDimension::D2,
      format,
      usage: wgpu::TextureUsages::RENDER_ATTACHMENT
         | wgpu::TextureUsages::TEXTURE_BINDING
         | wgpu::TextureUsages::COPY_SRC
         | wgpu::TextureUsages::COPY_DST,
      view_formats: &[],
   });
   let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
   let bind_group = create_texture_bind_group(device, programs, &view, &programs.sampler);
   (texture, view, bind_group)
}

fn create_texture_bind_group(
   device: &wgpu::Device,
   programs: &GpuPrograms,
   view: &wgpu::TextureView,
   sampler: &wgpu::Sampler,
) -> wgpu::BindGroup
{
   device.create_bind_group(&wgpu::BindGroupDescriptor {
      label: Some("oxide-webgpu-texture-bind-group"),
      layout: &programs.texture_layout,
      entries: &[
         wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
         wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
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
)
{
   if needed == 0 {
      return;
   }
   if buffer.is_some() && *capacity >= needed {
      return;
   }
   let next = needed.next_power_of_two().max(1024);
   *buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
      label: Some(label),
      size: next,
      usage,
      mapped_at_creation: false,
   }));
   *capacity = next;
}

fn set_scissor(pass: &mut wgpu::RenderPass<'_>, clip: api::RectI, scale: f32, width: u32, height: u32)
{
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

fn quad_vertices(rect: api::RectF, u0: f32, v0: f32, u1: f32, v1: f32, color: api::Color) -> [GpuVertex; 4]
{
   [
      gpu_vertex(rect.x, rect.y, u0, v0, color),
      gpu_vertex(rect.x + rect.w, rect.y, u1, v0, color),
      gpu_vertex(rect.x, rect.y + rect.h, u0, v1, color),
      gpu_vertex(rect.x + rect.w, rect.y + rect.h, u1, v1, color),
   ]
}

fn rounded_rect_mesh(rect: api::RectF, radii: [f32; 4], color: api::Color) -> (Vec<GpuVertex>, Vec<u32>)
{
   if rect.w <= 0.0 || rect.h <= 0.0 || color.a <= 0.0 {
      return (Vec::new(), Vec::new());
   }
   let max_r = (rect.w.abs() * 0.5).min(rect.h.abs() * 0.5);
   let radii = [
      radii[0].clamp(0.0, max_r),
      radii[1].clamp(0.0, max_r),
      radii[2].clamp(0.0, max_r),
      radii[3].clamp(0.0, max_r),
   ];
   let mut points = Vec::new();
   append_arc(&mut points, rect.x + radii[0], rect.y + radii[0], radii[0], core::f32::consts::PI, 1.5 * core::f32::consts::PI);
   append_arc(&mut points, rect.x + rect.w - radii[1], rect.y + radii[1], radii[1], 1.5 * core::f32::consts::PI, 2.0 * core::f32::consts::PI);
   append_arc(&mut points, rect.x + rect.w - radii[2], rect.y + rect.h - radii[2], radii[2], 0.0, 0.5 * core::f32::consts::PI);
   append_arc(&mut points, rect.x + radii[3], rect.y + rect.h - radii[3], radii[3], 0.5 * core::f32::consts::PI, core::f32::consts::PI);
   if points.len() < 3 {
      return (quad_vertices(rect, 0.0, 0.0, 1.0, 1.0, color).to_vec(), vec![0, 1, 2, 2, 1, 3]);
   }
   let mut vertices = Vec::with_capacity(points.len() + 1);
   vertices.push(gpu_vertex(rect.x + rect.w * 0.5, rect.y + rect.h * 0.5, 0.5, 0.5, color));
   for (x, y) in points {
      vertices.push(gpu_vertex(x, y, 0.0, 0.0, color));
   }
   let mut indices = Vec::with_capacity((vertices.len() - 1) * 3);
   for idx in 1..vertices.len() {
      indices.push(0);
      indices.push(idx as u32);
      indices.push(if idx + 1 < vertices.len() { idx as u32 + 1 } else { 1 });
   }
   (vertices, indices)
}

fn append_arc(points: &mut Vec<(f32, f32)>, cx: f32, cy: f32, radius: f32, start: f32, end: f32)
{
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

fn gpu_vertex(x: f32, y: f32, u: f32, v: f32, color: api::Color) -> GpuVertex
{
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

fn logical_dimension(physical: u32, scale: f32) -> f32
{
   physical as f32 / sanitize_scale(scale)
}

fn encode_vertices(vertices: &[GpuVertex], out: &mut Vec<u8>)
{
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

fn encode_indices(indices: &[u32], out: &mut Vec<u8>)
{
   out.reserve(indices.len().saturating_mul(4));
   for index in indices {
      out.extend_from_slice(&index.to_le_bytes());
   }
}

fn f32x4_bytes(values: [f32; 4]) -> [u8; 16]
{
   let mut out = [0; 16];
   let mut offset = 0;
   for value in values {
      write_f32(&mut out, &mut offset, value);
   }
   out
}

fn vertex4_bytes(vertices: &[GpuVertex; 4]) -> [u8; VERTEX_STRIDE_BYTES * 4]
{
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

fn index6_bytes(indices: [u32; 6]) -> [u8; 24]
{
   let mut out = [0; 24];
   let mut offset = 0;
   for index in indices {
      write_u32(&mut out, &mut offset, index);
   }
   out
}

fn push_f32(out: &mut Vec<u8>, value: f32)
{
   out.extend_from_slice(&value.to_le_bytes());
}

fn write_f32(out: &mut [u8], offset: &mut usize, value: f32)
{
   let bytes = value.to_le_bytes();
   out[*offset..*offset + 4].copy_from_slice(&bytes);
   *offset += 4;
}

fn write_u32(out: &mut [u8], offset: &mut usize, value: u32)
{
   let bytes = value.to_le_bytes();
   out[*offset..*offset + 4].copy_from_slice(&bytes);
   *offset += 4;
}

fn js_error(stage: &'static str, err: JsValue) -> api::RenderError
{
   let message = err.as_string().unwrap_or_else(|| format!("{err:?}"));
   api::RenderError::Io(format!("{stage}: {message}"))
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
