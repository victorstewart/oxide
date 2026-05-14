use super::{
    a8_to_rgba, copy_a8_rows, copy_rgba_rows, document, index_slice, normalized_index_mode,
    resolve_index, sanitize_scale, source_rect, vertex_slice,
};
use crate::WebRendererStats;
use crate::{id_mask_compositor, neon_marker, scene3d};
use js_sys::Reflect;
use oxide_renderer_api as api;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::HtmlCanvasElement;

const VERTEX_STRIDE: wgpu::BufferAddress = 32;
const VERTEX_STRIDE_BYTES: usize = 32;
const SCENE3D_VERTEX_STRIDE: wgpu::BufferAddress = 28;
const SCENE3D_UNIFORM_STRIDE: usize = 256;
const SCENE3D_DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;
const ID_MASK_VERTEX_STRIDE: wgpu::BufferAddress = 16;
const ID_MASK_RASTER_UNIFORM_SIZE_BYTES: usize = 16;
const ID_MASK_RASTER_UNIFORM_SIZE: u64 = ID_MASK_RASTER_UNIFORM_SIZE_BYTES as u64;
const MAX_BLUR_SIGMA: f32 = 96.0;

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

struct IdMaskDraw {
    viewport: api::RectF,
    mask_width: u32,
    mask_height: u32,
    mask_scale: f32,
    vertices: Vec<id_mask_compositor::IdMaskRasterVertex>,
    city_styles: [id_mask_compositor::IdMaskCityStyle; id_mask_compositor::ID_MASK_MAX_CITY_STYLES],
    neighborhood_colors: [[f32; 3]; id_mask_compositor::ID_MASK_MAX_NEIGHBORHOOD_COLORS],
    mode: id_mask_compositor::IdMaskCompositorMode,
    glow_enabled: bool,
    darken_background_alpha: f32,
    polish: id_mask_compositor::IdMaskPolishConfig,
}

#[derive(Clone, Copy)]
enum DrawKind {
    Solid,
    Rgba { image: usize },
    A8 { image: usize },
    Sdf { image: usize },
    Backdrop { sigma: f32 },
}

#[derive(Clone, Copy)]
struct GpuDraw {
    kind: DrawKind,
    first_index: u32,
    index_count: u32,
    clip: api::RectI,
}

struct FrameData {
    vertices: Vec<GpuVertex>,
    indices: Vec<u32>,
    draws: Vec<GpuDraw>,
}

impl FrameData {
    fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
        self.draws.clear();
    }
}

struct GpuPrograms {
    viewport_layout: wgpu::BindGroupLayout,
    texture_layout: wgpu::BindGroupLayout,
    effect_layout: wgpu::BindGroupLayout,
    scene3d_layout: wgpu::BindGroupLayout,
    id_mask_raster_layout: wgpu::BindGroupLayout,
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

    pub fn set_camera_background_rgba8(
        &mut self,
        width: u32,
        height: u32,
        data: &[u8],
        row_bytes: usize,
    ) -> Result<(), api::RenderError> {
        self.inner.set_camera_background_rgba8(width, height, data, row_bytes)
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
    scene3d_draws: Vec<Scene3dDraw>,
    id_mask_draws: Vec<IdMaskDraw>,
    scene3d_clear_color: Option<api::Color>,
    scene3d_clear_depth: bool,
    scene3d_active: bool,
    images: Vec<Option<GpuImage>>,
    meshes_3d: Vec<Option<GpuMesh3d>>,
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
            scene_depth_texture,
            scene_depth_view,
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
            scene3d_draws: Vec::new(),
            id_mask_draws: Vec::new(),
            scene3d_clear_color: None,
            scene3d_clear_depth: true,
            scene3d_active: false,
            images: vec![None],
            meshes_3d: vec![None],
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
    pub fn canvas(&self) -> HtmlCanvasElement {
        self.canvas.clone()
    }

    #[must_use]
    pub fn last_stats(&self) -> WebRendererStats {
        self.stats
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

    pub fn set_camera_background_rgba8(
        &mut self,
        width: u32,
        height: u32,
        data: &[u8],
        row_bytes: usize,
    ) -> Result<(), api::RenderError> {
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
    ) -> Result<(), api::RenderError> {
        let rgba = copy_rgba_rows(width, height, data, row_bytes)
            .ok_or(api::RenderError::InvalidOperation("invalid rgba update rows"))?;
        self.update_image(handle, x, y, width, height, GpuImageKind::Rgba, &rgba)
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
            self.scene3d_draws.push(Scene3dDraw {
                mesh: instance.mesh.0 as usize,
                uniform_offset,
                pipeline,
            });
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
        self.id_mask_draws.push(IdMaskDraw {
            viewport: pass.raster.viewport,
            mask_width: pass.raster.mask_width as u32,
            mask_height: pass.raster.mask_height as u32,
            mask_scale: pass.raster.mask_scale,
            vertices: pass.raster.vertices.to_vec(),
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
        let bind_group =
            create_texture_bind_group(&self.device, &self.programs, &view, &self.programs.sampler);
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
    ) -> Result<(), api::RenderError> {
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

    fn image(&self, handle: api::ImageHandle) -> Option<&GpuImage> {
        self.images.get(handle.0 as usize).and_then(Option::as_ref)
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

    fn push_draw(&mut self, kind: DrawKind, vertices: &[GpuVertex], indices: &[u32]) {
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

    fn encode_items(&mut self, list: &api::DrawList, index: &mut usize, stop_at_layer_end: bool) {
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

    fn encode_draw_cmd(&mut self, list: &api::DrawList, item: &api::DrawCmd) {
        match item {
            api::DrawCmd::LayerBegin { .. } | api::DrawCmd::LayerEnd => {}
            api::DrawCmd::Solid { vb, ib, color } => self.encode_solid(list, *vb, *ib, *color),
            api::DrawCmd::Image { tex, dst, src, alpha } => {
                self.encode_image(*tex, *dst, *src, *alpha, false)
            }
            api::DrawCmd::GlyphRun { run } => self.encode_glyph_run(list, run),
            api::DrawCmd::RRect { rect, radii, color } => self.encode_rrect(*rect, *radii, *color),
            api::DrawCmd::NineSlice { tex, rect, slice, alpha } => {
                self.encode_nine_slice(*tex, *rect, *slice, *alpha)
            }
            api::DrawCmd::Backdrop { rect, sigma, tint, alpha } => {
                self.encode_backdrop(*rect, *sigma, *tint, *alpha)
            }
            api::DrawCmd::VisualEffect { rect, effect } => {
                let tint = effect.tint();
                self.encode_backdrop(*rect, effect.blur_intensity() * 72.0, tint, tint.a);
            }
            api::DrawCmd::CameraBg { rect, tint, alpha, .. } => {
                if let Some(handle) = self.camera_background {
                    self.encode_image(
                        handle,
                        *rect,
                        api::RectF::new(0.0, 0.0, 0.0, 0.0),
                        *alpha,
                        false,
                    );
                }
                if tint.a > 0.0 {
                    self.encode_rect(
                        *rect,
                        api::Color::rgba(tint.r, tint.g, tint.b, tint.a * alpha.clamp(0.0, 1.0)),
                    );
                }
            }
            api::DrawCmd::Spinner { center, atom, alpha } => {
                self.encode_spinner(*center, *atom, *alpha)
            }
            api::DrawCmd::ClipPush { rect } => self.clip_stack.push(*rect),
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
                    if let Some(vertex) =
                        resolve_index(*index, mode).and_then(|offset| vertices.get(offset))
                    {
                        idx.push(out.len() as u32);
                        out.push(gpu_vertex(vertex.x, vertex.y, vertex.u, vertex.v, color));
                    }
                }
            }
        } else if vertices.len() == 4 {
            out.extend(
                vertices
                    .iter()
                    .map(|vertex| gpu_vertex(vertex.x, vertex.y, vertex.u, vertex.v, color)),
            );
            idx.extend_from_slice(&[0, 1, 2, 2, 1, 3]);
        } else {
            out.extend(
                vertices
                    .iter()
                    .map(|vertex| gpu_vertex(vertex.x, vertex.y, vertex.u, vertex.v, color)),
            );
            idx.extend(0..out.len() as u32);
        }
        self.push_draw(DrawKind::Solid, &out, &idx);
        self.stats.solid_tris = self.stats.solid_tris.saturating_add((idx.len() / 3) as u32);
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
            (GpuImageKind::Rgba, _) => DrawKind::Rgba { image: handle.0 as usize },
            (GpuImageKind::A8, false) => DrawKind::A8 { image: handle.0 as usize },
            (GpuImageKind::A8, true) => DrawKind::Sdf { image: handle.0 as usize },
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
            out.extend(
                vertices
                    .iter()
                    .map(|vertex| gpu_vertex(vertex.x, vertex.y, vertex.u, vertex.v, run.color)),
            );
            idx.extend(0..out.len() as u32);
        }
        self.push_draw(kind, &out, &idx);
        self.stats.glyph_quads = self.stats.glyph_quads.saturating_add((idx.len() / 6) as u32);
    }

    fn encode_rect(&mut self, rect: api::RectF, color: api::Color) {
        if rect.w <= 0.0 || rect.h <= 0.0 || color.a <= 0.0 {
            return;
        }
        let vertices = quad_vertices(rect, 0.0, 0.0, 1.0, 1.0, color);
        self.push_draw(DrawKind::Solid, &vertices, &[0, 1, 2, 2, 1, 3]);
        self.stats.solid_tris = self.stats.solid_tris.saturating_add(2);
    }

    fn encode_rrect(&mut self, rect: api::RectF, radii: [f32; 4], color: api::Color) {
        let (vertices, indices) = rounded_rect_mesh(rect, radii, color);
        self.push_draw(DrawKind::Solid, &vertices, &indices);
        self.stats.solid_tris = self.stats.solid_tris.saturating_add((indices.len() / 3) as u32);
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
        self.push_draw(
            DrawKind::Backdrop { sigma: sigma.clamp(0.0, MAX_BLUR_SIGMA) },
            &vertices,
            &[0, 1, 2, 2, 1, 3],
        );
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
    }

    fn upload_frame_buffers(&mut self) {
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
        }
        if let Some(buffer) = &self.scene3d_uniform_buffer {
            self.queue.write_buffer(buffer, 0, &self.scene3d_uniform_bytes);
        }
    }

    fn write_viewport_uniform(&self) {
        let logical_w = logical_dimension(self.width, self.scale).max(1.0);
        let logical_h = logical_dimension(self.height, self.scale).max(1.0);
        let bytes = f32x4_bytes([logical_w, logical_h, 0.0, 0.0]);
        self.queue.write_buffer(&self.viewport_buffer, 0, &bytes);
    }

    fn write_effect_uniform(&self, sigma: f32) {
        let radius = sigma.clamp(0.0, MAX_BLUR_SIGMA);
        let texel_x = 1.0 / self.width.max(1) as f32;
        let texel_y = 1.0 / self.height.max(1) as f32;
        let bytes = f32x4_bytes([texel_x, texel_y, radius, 0.0]);
        self.queue.write_buffer(&self.effect_buffer, 0, &bytes);
    }
}

impl api::Renderer for WebGpuRenderer {
    fn device_caps(&self) -> api::DeviceCaps {
        api::DeviceCaps {
            max_framerate_hz: 60,
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
        self.id_mask_draws.clear();
        self.scene3d_clear_color = None;
        self.scene3d_clear_depth = true;
        self.scene3d_active = false;
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

    fn encode_pass(&mut self, list: &api::DrawList) {
        let mut index = 0;
        self.encode_items(list, &mut index, false);
    }

    fn submit(&mut self, token: api::FrameToken) -> Result<(), api::RenderError> {
        if self.active_token != Some(token) {
            return Err(api::RenderError::InvalidOperation("frame token mismatch"));
        }
        self.active_token = None;
        self.upload_frame_buffers();
        self.upload_scene3d_uniforms();
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
        let surface_view =
            surface_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
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

    fn resize(&mut self, width: u32, height: u32, scale: f32) -> Result<(), api::RenderError> {
        self.width = width.max(1);
        self.height = height.max(1);
        self.scale = sanitize_scale(scale);
        self.canvas.set_width(self.width);
        self.canvas.set_height(self.height);
        self.config.width = self.width;
        self.config.height = self.height;
        self.surface.configure(&self.device, &self.config);
        self.recreate_targets();
        Ok(())
    }
}

impl WebGpuRenderer {
    fn frame_uses_backdrop(&self) -> bool {
        self.frame.draws.iter().any(|draw| matches!(draw.kind, DrawKind::Backdrop { .. }))
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
        self.render_draw_range(
            encoder,
            surface_view,
            0,
            self.frame.draws.len(),
            if self.scene3d_active || !self.id_mask_draws.is_empty() {
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
                    wgpu::Extent3d {
                        width: self.width,
                        height: self.height,
                        depth_or_array_layers: 1,
                    },
                );
                self.write_effect_uniform(sigma);
                self.render_draw_range(encoder, &scene_view, start, start + 1, wgpu::LoadOp::Load);
                start += 1;
            } else {
                let mut end = start + 1;
                while end < self.frame.draws.len()
                    && !matches!(self.frame.draws[end].kind, DrawKind::Backdrop { .. })
                {
                    end += 1;
                }
                self.render_draw_range(encoder, &scene_view, start, end, wgpu::LoadOp::Load);
                start = end;
            }
        }
    }

    fn clear_scene(&self, encoder: &mut wgpu::CommandEncoder) {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("oxide-webgpu-clear-scene"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.scene_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
    }

    fn render_scene3d(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
    ) {
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
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        for draw in &self.scene3d_draws {
            let Some(mesh) = self.meshes_3d.get(draw.mesh).and_then(Option::as_ref) else {
                continue;
            };
            let pipeline = match draw.pipeline {
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
                Scene3dPipelineKind::AdditiveNoDepth => {
                    &self.programs.scene3d_color_tri_add_pipeline
                }
            };
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind_group, &[draw.uniform_offset]);
            pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
            pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            self.stats.draws = self.stats.draws.saturating_add(1);
        }
    }

    fn render_id_mask_compositors(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
        first_load: wgpu::LoadOp<wgpu::Color>,
    ) {
        let mut load = first_load;
        let mut encoded_draws = 0_u32;
        for draw in &self.id_mask_draws {
            let width = draw.mask_width.max(1);
            let height = draw.mask_height.max(1);
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

            let vertex_bytes = id_mask_raster_vertex_bytes(&draw.vertices);
            let vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("oxide-webgpu-id-mask-raster-vertices"),
                size: vertex_bytes.len().max(1) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue.write_buffer(&vertex_buffer, 0, &vertex_bytes);

            let raster_uniform_bytes = id_mask_raster_uniform_bytes(width, height);
            let raster_uniform_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("oxide-webgpu-id-mask-raster-uniforms"),
                size: ID_MASK_RASTER_UNIFORM_SIZE,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue.write_buffer(&raster_uniform_buffer, 0, &raster_uniform_bytes);
            let raster_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("oxide-webgpu-id-mask-raster-bind-group"),
                layout: &self.programs.id_mask_raster_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: raster_uniform_buffer.as_entire_binding(),
                }],
            });

            {
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
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.programs.id_mask_raster_pipeline);
                pass.set_bind_group(0, &raster_bind_group, &[]);
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.draw(0..draw.vertices.len() as u32, 0..1);
            }

            let compositor_uniform_bytes = id_mask_compositor_uniform_bytes(draw);
            let compositor_uniform_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("oxide-webgpu-id-mask-compositor-uniforms"),
                size: compositor_uniform_bytes.len() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue.write_buffer(&compositor_uniform_buffer, 0, &compositor_uniform_bytes);
            let compositor_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("oxide-webgpu-id-mask-compositor-bind-group"),
                layout: &self.programs.id_mask_compositor_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: compositor_uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&city_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&neighborhood_view),
                    },
                ],
            });

            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("oxide-webgpu-id-mask-compositor-pass"),
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
                pass.set_pipeline(&self.programs.id_mask_compositor_pipeline);
                pass.set_bind_group(0, &compositor_bind_group, &[]);
                pass.draw(0..6, 0..1);
            }
            load = wgpu::LoadOp::Load;
            encoded_draws = encoded_draws.saturating_add(2);
        }
        self.stats.draws = self.stats.draws.saturating_add(encoded_draws);
    }

    fn render_draw_range(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
        start: usize,
        end: usize,
        load: wgpu::LoadOp<wgpu::Color>,
    ) {
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
                has_dynamic_offset: false,
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
    let id_mask_compositor_pipeline_layout =
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("oxide-webgpu-id-mask-compositor-pipeline-layout"),
            bind_group_layouts: &[&id_mask_compositor_layout],
            push_constant_ranges: &[],
        });

    let solid_pipeline =
        create_pipeline(device, &shader, &solid_layout, &vertex_layout, &color_target, "fs_solid");
    let rgba_pipeline = create_pipeline(
        device,
        &shader,
        &texture_pipeline_layout,
        &vertex_layout,
        &color_target,
        "fs_rgba",
    );
    let a8_pipeline = create_pipeline(
        device,
        &shader,
        &texture_pipeline_layout,
        &vertex_layout,
        &color_target,
        "fs_a8",
    );
    let sdf_pipeline = create_pipeline(
        device,
        &shader,
        &texture_pipeline_layout,
        &vertex_layout,
        &color_target,
        "fs_sdf",
    );
    let effect_pipeline = create_pipeline(
        device,
        &shader,
        &effect_pipeline_layout,
        &vertex_layout,
        &color_target,
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
    let id_mask_raster_pipeline = create_id_mask_raster_pipeline(
        device,
        &id_mask_shader,
        &id_mask_raster_pipeline_layout,
        &id_mask_raster_vertex_layout(),
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
        id_mask_compositor_pipeline,
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
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] = [
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        },
        wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 8, shader_location: 1 },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint32,
            offset: 12,
            shader_location: 2,
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
) -> (wgpu::Buffer, wgpu::BindGroup) {
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
) -> (wgpu::Texture, wgpu::TextureView, wgpu::BindGroup) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
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

fn create_depth_texture(
    device: &wgpu::Device,
    label: &'static str,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: SCENE3D_DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn create_id_mask_texture(
    device: &wgpu::Device,
    label: &'static str,
    width: u32,
    height: u32,
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
        format: wgpu::TextureFormat::R8Uint,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
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

fn ensure_buffer(
    device: &wgpu::Device,
    buffer: &mut Option<wgpu::Buffer>,
    capacity: &mut u64,
    needed: u64,
    usage: wgpu::BufferUsages,
    label: &'static str,
) {
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

fn quad_vertices(
    rect: api::RectF,
    u0: f32,
    v0: f32,
    u1: f32,
    v1: f32,
    color: api::Color,
) -> [GpuVertex; 4] {
    [
        gpu_vertex(rect.x, rect.y, u0, v0, color),
        gpu_vertex(rect.x + rect.w, rect.y, u1, v0, color),
        gpu_vertex(rect.x, rect.y + rect.h, u0, v1, color),
        gpu_vertex(rect.x + rect.w, rect.y + rect.h, u1, v1, color),
    ]
}

fn rounded_rect_mesh(
    rect: api::RectF,
    radii: [f32; 4],
    color: api::Color,
) -> (Vec<GpuVertex>, Vec<u32>) {
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
    append_arc(
        &mut points,
        rect.x + radii[0],
        rect.y + radii[0],
        radii[0],
        core::f32::consts::PI,
        1.5 * core::f32::consts::PI,
    );
    append_arc(
        &mut points,
        rect.x + rect.w - radii[1],
        rect.y + radii[1],
        radii[1],
        1.5 * core::f32::consts::PI,
        2.0 * core::f32::consts::PI,
    );
    append_arc(
        &mut points,
        rect.x + rect.w - radii[2],
        rect.y + rect.h - radii[2],
        radii[2],
        0.0,
        0.5 * core::f32::consts::PI,
    );
    append_arc(
        &mut points,
        rect.x + radii[3],
        rect.y + rect.h - radii[3],
        radii[3],
        0.5 * core::f32::consts::PI,
        core::f32::consts::PI,
    );
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

fn gpu_vertex(x: f32, y: f32, u: f32, v: f32, color: api::Color) -> GpuVertex {
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

fn id_mask_raster_vertex_bytes(vertices: &[id_mask_compositor::IdMaskRasterVertex]) -> Vec<u8> {
    let mut out = Vec::with_capacity(vertices.len().saturating_mul(ID_MASK_VERTEX_STRIDE as usize));
    for vertex in vertices {
        push_f32(&mut out, vertex.position_px[0]);
        push_f32(&mut out, vertex.position_px[1]);
        out.extend_from_slice(&vertex.city_id.to_le_bytes());
        out.extend_from_slice(&vertex.neighborhood_id.to_le_bytes());
    }
    out
}

fn id_mask_raster_uniform_bytes(
    width: u32,
    height: u32,
) -> [u8; ID_MASK_RASTER_UNIFORM_SIZE_BYTES] {
    f32x4_bytes([width as f32, height as f32, 0.0, 0.0])
}

fn id_mask_compositor_uniform_bytes(draw: &IdMaskDraw) -> Vec<u8> {
    let mut out = Vec::with_capacity(
        16 * (3 + 4 + 4 + 4 + id_mask_compositor::ID_MASK_MAX_NEIGHBORHOOD_COLORS),
    );
    for value in [draw.viewport.x, draw.viewport.y, draw.viewport.w, draw.viewport.h] {
        push_f32(&mut out, value);
    }
    for value in [
        draw.mask_width as f32,
        draw.mask_height as f32,
        draw.mask_scale.max(1.0),
        draw.darken_background_alpha.clamp(0.0, 1.0),
    ] {
        push_f32(&mut out, value);
    }
    for value in [
        draw.mode as u32 as f32,
        if draw.glow_enabled { 1.0 } else { 0.0 },
        draw.polish.smooth_radius_px.max(0.0),
        draw.polish.fallback_radius_px.max(0.0),
    ] {
        push_f32(&mut out, value);
    }
    for style in draw.city_styles {
        push_f32(&mut out, style.fill_rgb[0]);
        push_f32(&mut out, style.fill_rgb[1]);
        push_f32(&mut out, style.fill_rgb[2]);
        push_f32(&mut out, 1.0);
    }
    for style in draw.city_styles {
        push_f32(&mut out, style.edge_rgb[0]);
        push_f32(&mut out, style.edge_rgb[1]);
        push_f32(&mut out, style.edge_rgb[2]);
        push_f32(&mut out, 1.0);
    }
    for style in draw.city_styles {
        push_f32(&mut out, style.seam_rgb[0]);
        push_f32(&mut out, style.seam_rgb[1]);
        push_f32(&mut out, style.seam_rgb[2]);
        push_f32(&mut out, 1.0);
    }
    for rgb in draw.neighborhood_colors {
        push_f32(&mut out, rgb[0]);
        push_f32(&mut out, rgb[1]);
        push_f32(&mut out, rgb[2]);
        push_f32(&mut out, 1.0);
    }
    out
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

const ID_MASK_WGSL: &str = r#"
struct IdMaskRasterParams {
   mask_size: vec4<f32>,
};

@group(0) @binding(0) var<uniform> raster_params: IdMaskRasterParams;

struct IdMaskRasterVertexIn {
   @location(0) position_px: vec2<f32>,
   @location(1) city_id: u32,
   @location(2) neighborhood_id: u32,
};

struct IdMaskRasterOut {
   @builtin(position) position: vec4<f32>,
   @location(0) @interpolate(flat) city_id: u32,
   @location(1) @interpolate(flat) neighborhood_id: u32,
};

struct IdMaskRasterTargets {
   @location(0) city: u32,
   @location(1) neighborhood: u32,
};

@vertex
fn vs_id_mask_raster(input: IdMaskRasterVertexIn) -> IdMaskRasterOut {
   let mask_size = max(raster_params.mask_size.xy, vec2<f32>(1.0, 1.0));
   let normalized = input.position_px / mask_size;
   var out: IdMaskRasterOut;
   out.position = vec4<f32>(normalized.x * 2.0 - 1.0, 1.0 - normalized.y * 2.0, 0.0, 1.0);
   out.city_id = input.city_id;
   out.neighborhood_id = input.neighborhood_id;
   return out;
}

@fragment
fn fs_id_mask_raster(input: IdMaskRasterOut) -> IdMaskRasterTargets {
   var out: IdMaskRasterTargets;
   out.city = input.city_id;
   out.neighborhood = input.neighborhood_id;
   return out;
}

struct IdMaskCompositorParams {
   viewport: vec4<f32>,
   mask_size_scale_alpha: vec4<f32>,
   mode_glow_polish_fallback: vec4<f32>,
   city_fill_colors: array<vec4<f32>, 4>,
   city_edge_colors: array<vec4<f32>, 4>,
   city_seam_colors: array<vec4<f32>, 4>,
   neighborhood_colors: array<vec4<f32>, 32>,
};

@group(0) @binding(0) var<uniform> compositor_params: IdMaskCompositorParams;
@group(0) @binding(1) var city_tex: texture_2d<u32>;
@group(0) @binding(2) var neighborhood_tex: texture_2d<u32>;

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

fn direction_sample_offset(direction_index: i32, radius: i32) -> vec2<i32> {
   let dirs = array<vec2<f32>, 24>(
      vec2<f32>(1.0, 0.0), vec2<f32>(0.966, 0.259), vec2<f32>(0.866, 0.5),
      vec2<f32>(0.707, 0.707), vec2<f32>(0.5, 0.866), vec2<f32>(0.259, 0.966),
      vec2<f32>(0.0, 1.0), vec2<f32>(-0.259, 0.966), vec2<f32>(-0.5, 0.866),
      vec2<f32>(-0.707, 0.707), vec2<f32>(-0.866, 0.5), vec2<f32>(-0.966, 0.259),
      vec2<f32>(-1.0, 0.0), vec2<f32>(-0.966, -0.259), vec2<f32>(-0.866, -0.5),
      vec2<f32>(-0.707, -0.707), vec2<f32>(-0.5, -0.866), vec2<f32>(-0.259, -0.966),
      vec2<f32>(0.0, -1.0), vec2<f32>(0.259, -0.966), vec2<f32>(0.5, -0.866),
      vec2<f32>(0.707, -0.707), vec2<f32>(0.866, -0.5), vec2<f32>(0.966, -0.259),
   );
   return vec2<i32>(round(dirs[u32(direction_index)] * f32(radius)));
}

fn conservative_polished_city(p: vec2<i32>, size: vec2<u32>, max_radius: i32) -> u32 {
   let direct = read_mask(city_tex, p, size);
   if (direct != 0u || max_radius <= 0) {
      return direct;
   }
   var counts = array<u32, 4>(0u, 0u, 0u, 0u);
   var sample_count = 0u;
   let radius_squared = max_radius * max_radius;
   for (var oy = -max_radius; oy <= max_radius; oy = oy + 1) {
      for (var ox = -max_radius; ox <= max_radius; ox = ox + 1) {
         if (ox * ox + oy * oy > radius_squared) {
            continue;
         }
         sample_count = sample_count + 1u;
         let city = min(read_mask(city_tex, p + vec2<i32>(ox, oy), size), 3u);
         if (city != 0u) {
            counts[city] = counts[city] + 1u;
         }
      }
   }
   var best_city = 0u;
   var best_count = 0u;
   for (var city = 1u; city <= 3u; city = city + 1u) {
      if (counts[city] > best_count) {
         best_city = city;
         best_count = counts[city];
      }
   }
   let coverage = select(0.0, f32(best_count) / f32(sample_count), sample_count != 0u);
   return select(0u, best_city, coverage >= 0.68);
}

fn is_internal_seam_pixel(p: vec2<i32>, size: vec2<u32>) -> bool {
   let city = read_mask(city_tex, p, size);
   let neighborhood = read_mask(neighborhood_tex, p, size);
   if (city == 0u || neighborhood == 0u) {
      return false;
   }
   for (var oy = -1; oy <= 1; oy = oy + 1) {
      for (var ox = -1; ox <= 1; ox = ox + 1) {
         if (ox == 0 && oy == 0) {
            continue;
         }
         let q = p + vec2<i32>(ox, oy);
         if (read_mask(city_tex, q, size) == city) {
            let other = read_mask(neighborhood_tex, q, size);
            if (other != 0u && other != neighborhood) {
               return true;
            }
         }
      }
   }
   return false;
}

fn nearest_city(p: vec2<i32>, size: vec2<u32>, max_radius: i32) -> vec2<u32> {
   let direct = read_mask(city_tex, p, size);
   if (direct != 0u) {
      return vec2<u32>(direct, 0u);
   }
   for (var r = 1; r <= max_radius; r = r + 2) {
      for (var i = 0; i < 24; i = i + 1) {
         let city = read_mask(city_tex, p + direction_sample_offset(i, r), size);
         if (city != 0u) {
            return vec2<u32>(city, u32(r));
         }
      }
   }
   return vec2<u32>(0u, u32(max_radius + 1));
}

fn nearest_neighborhood_for_city(p: vec2<i32>, size: vec2<u32>, city: u32, max_radius: i32) -> u32 {
   let direct_city = read_mask(city_tex, p, size);
   let direct_neighborhood = read_mask(neighborhood_tex, p, size);
   if (direct_city == city && direct_neighborhood != 0u) {
      return direct_neighborhood;
   }
   for (var r = 1; r <= max_radius; r = r + 1) {
      for (var i = 0; i < 24; i = i + 1) {
         let q = p + direction_sample_offset(i, r);
         if (read_mask(city_tex, q, size) == city) {
            let neighborhood = read_mask(neighborhood_tex, q, size);
            if (neighborhood != 0u) {
               return neighborhood;
            }
         }
      }
   }
   return 0u;
}

fn nearest_seam_distance(p: vec2<i32>, size: vec2<u32>, max_radius: i32) -> f32 {
   if (is_internal_seam_pixel(p, size)) {
      return 0.0;
   }
   for (var r = 1; r <= max_radius; r = r + 1) {
      for (var i = 0; i < 24; i = i + 1) {
         if (is_internal_seam_pixel(p + direction_sample_offset(i, r), size)) {
            return f32(r);
         }
      }
   }
   return f32(max_radius + 1);
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
   let city = conservative_polished_city(p, size, polish_radius);
   let neighborhood = nearest_neighborhood_for_city(p, size, city, fallback_radius);
   let city_index = min(city, 3u);
   let neighborhood_index = min(neighborhood, 31u);

   if (mode == 2u) {
      return select(vec4<f32>(compositor_params.city_edge_colors[city_index].rgb, 1.0), vec4<f32>(0.0, 0.0, 0.0, 1.0), city == 0u);
   }
   if (mode == 3u) {
      return select(vec4<f32>(compositor_params.neighborhood_colors[neighborhood_index].rgb, 1.0), vec4<f32>(0.0, 0.0, 0.0, 1.0), neighborhood == 0u);
   }

   let seam_distance = nearest_seam_distance(p, size, i32(ceil(5.0 * mask_scale)));
   if (mode == 1u) {
      let core = gaussian_alpha(seam_distance, mask_scale, 0.42, 1.0, 2.1);
      return select(vec4<f32>(0.0, 0.0, 0.0, 1.0), vec4<f32>(1.0, 1.0, 1.0, 1.0), core > 0.04 && city != 0u);
   }

   if (city == 0u) {
      let dark_alpha = clamp(compositor_params.mask_size_scale_alpha.w, 0.0, 1.0);
      if (!glow_enabled) {
         return vec4<f32>(0.0, 0.0, 0.0, dark_alpha);
      }
      let halo = nearest_city(p, size, i32(ceil(18.0 * mask_scale)));
      let halo_city = halo.x;
      if (halo_city == 0u) {
         return vec4<f32>(0.0, 0.0, 0.0, dark_alpha);
      }
      let halo_distance = f32(halo.y);
      let alpha = max(
         gaussian_alpha(halo_distance, mask_scale, 16.0, 0.04, 3.2),
         gaussian_alpha(halo_distance, mask_scale, 8.5, 0.15, 3.2),
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
