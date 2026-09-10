//! Target-neutral wgpu shader ownership and native headless rendering.
#![forbid(unsafe_code)]

/// Canonical Oxide 2D wgpu shader shared with the browser backend.
pub const UI_WGSL: &str = include_str!("ui.wgsl");

/// Browser and native image-ingest rules shared by the wgpu backends.
pub mod image
{
   #[derive(Clone, Debug, PartialEq, Eq)]
   pub struct RgbaMipLevel
   {
      pub width: u32,
      pub height: u32,
      pub rgba: Vec<u8>,
   }

   /// Copies strided RGBA rows into the top-left-oriented, tightly packed image form.
   pub fn copy_rgba_rows(width: u32, height: u32, data: &[u8], row_bytes: usize) -> Option<Vec<u8>>
   {
      let packed_row = usize::try_from(width).ok()?.checked_mul(4)?;
      let required = row_bytes
         .checked_mul(usize::try_from(height.checked_sub(1)?).ok()?)?
         .checked_add(packed_row)?;
      if width == 0 || height == 0 || row_bytes < packed_row || data.len() < required
      {
         return None;
      }
      if row_bytes == packed_row
      {
         return Some(data[..packed_row.checked_mul(usize::try_from(height).ok()?)?].to_vec());
      }
      let mut rgba = Vec::with_capacity(packed_row.checked_mul(usize::try_from(height).ok()?)?);
      for row in 0..usize::try_from(height).ok()?
      {
         let start = row.checked_mul(row_bytes)?;
         rgba.extend_from_slice(&data[start..start.checked_add(packed_row)?]);
      }
      Some(rgba)
   }

   /// Generates a top-left-oriented sRGB mip chain by averaging RGB in linear light.
   /// RGB is stored straight (not alpha-weighted), matching the shared UI shader's
   /// straight-alpha blend contract.
   pub fn rgba8_srgb_mip_chain(width: u32, height: u32, rgba: Vec<u8>) -> Vec<RgbaMipLevel>
   {
      let mut levels = vec![RgbaMipLevel { width, height, rgba }];
      while levels.last().is_some_and(|level| level.width > 1 || level.height > 1)
      {
         let source = levels.last().expect("mip chain has a base level");
         let next_width = (source.width / 2).max(1);
         let next_height = (source.height / 2).max(1);
         let mut next = vec![0_u8; next_width as usize * next_height as usize * 4];
         for y in 0..next_height
         {
            for x in 0..next_width
            {
               let mut color_sums = [0.0_f32; 3];
               let mut alpha_sum = 0_u32;
               let mut samples = 0_u32;
               for source_y in y * 2..(y * 2 + 2).min(source.height)
               {
                  for source_x in x * 2..(x * 2 + 2).min(source.width)
                  {
                     let index = (source_y as usize * source.width as usize + source_x as usize) * 4;
                     for channel in 0..3
                     {
                        color_sums[channel] += srgb_channel_to_linear(source.rgba[index + channel]);
                     }
                     alpha_sum = alpha_sum.saturating_add(u32::from(source.rgba[index + 3]));
                     samples += 1;
                  }
               }
               let index = (y as usize * next_width as usize + x as usize) * 4;
               for channel in 0..3
               {
                  next[index + channel] = linear_channel_to_srgb(color_sums[channel] / samples as f32);
               }
               next[index + 3] = ((alpha_sum + samples / 2) / samples) as u8;
            }
         }
         levels.push(RgbaMipLevel { width: next_width, height: next_height, rgba: next });
      }
      levels
   }

   fn srgb_channel_to_linear(value: u8) -> f32
   {
      let value = f32::from(value) / 255.0;
      if value <= 0.04045 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
   }

   fn linear_channel_to_srgb(value: f32) -> u8
   {
      let value = value.clamp(0.0, 1.0);
      let encoded = if value <= 0.0031308 { value * 12.92 } else { 1.055 * value.powf(1.0 / 2.4) - 0.055 };
      (encoded * 255.0).round() as u8
   }
}

/// Target-neutral RGBA fixture comparison shared by browser-reference and native runners.
pub mod parity
{
   use std::fmt;

   #[derive(Clone, Copy, Debug, PartialEq)]
   pub struct PixelTolerance
   {
      pub differing_pixels: u64,
      pub max_channel_error: u8,
      pub mean_squared_error: f64,
   }

   impl PixelTolerance
   {
      pub const EXACT: Self =
         Self { differing_pixels: 0, max_channel_error: 0, mean_squared_error: 0.0 };
      pub const ANTIALIASED: Self =
         Self { differing_pixels: 16, max_channel_error: 3, mean_squared_error: 0.02 };
   }

   #[derive(Clone, Copy, Debug)]
   pub struct RgbaImage<'a>
   {
      pub width: u32,
      pub height: u32,
      pub row_bytes: u32,
      pub pixels: &'a [u8],
   }

   #[derive(Clone, Copy, Debug)]
   pub struct BrowserNativeRgbaFixture<'a>
   {
      pub id: &'a str,
      pub width: u32,
      pub height: u32,
      pub tolerance: PixelTolerance,
      pub browser: RgbaImage<'a>,
   }

   impl BrowserNativeRgbaFixture<'_>
   {
      pub fn compare_native(
         &self,
         native: RgbaImage<'_>,
      ) -> Result<RgbaDifference, RgbaParityError>
      {
         if self.browser.width != self.width || self.browser.height != self.height
         {
            return Err(RgbaParityError::FixtureDimensions);
         }
         if native.width != self.width || native.height != self.height
         {
            return Err(RgbaParityError::ImageDimensions);
         }
         compare_rgba(self.browser, native, self.tolerance)
      }
   }

   #[derive(Clone, Copy, Debug, Default, PartialEq)]
   pub struct RgbaDifference
   {
      pub differing_pixels: u64,
      pub max_channel_error: u8,
      pub mean_squared_error: f64,
   }

   #[derive(Clone, Copy, Debug, PartialEq)]
   pub enum RgbaParityError
   {
      FixtureDimensions,
      ImageDimensions,
      RowBytes,
      PixelBuffer,
      ToleranceExceeded(RgbaDifference),
   }

   impl fmt::Display for RgbaParityError
   {
      fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result
      {
         match self
         {
            Self::FixtureDimensions => formatter.write_str("fixture dimensions do not match its browser reference"),
            Self::ImageDimensions => formatter.write_str("browser and native image dimensions differ"),
            Self::RowBytes => formatter.write_str("RGBA row stride is smaller than width * 4"),
            Self::PixelBuffer => formatter.write_str("RGBA pixel buffer is shorter than its declared rows"),
            Self::ToleranceExceeded(difference) => write!(
               formatter,
               "RGBA tolerance exceeded: {} pixels differ, max channel error {}, mse {}",
               difference.differing_pixels,
               difference.max_channel_error,
               difference.mean_squared_error,
            ),
         }
      }
   }

   impl std::error::Error for RgbaParityError {}

   pub fn compare_rgba(
      reference: RgbaImage<'_>,
      candidate: RgbaImage<'_>,
      tolerance: PixelTolerance,
   ) -> Result<RgbaDifference, RgbaParityError>
   {
      if reference.width != candidate.width || reference.height != candidate.height
      {
         return Err(RgbaParityError::ImageDimensions);
      }
      validate_image(reference)?;
      validate_image(candidate)?;

      let tight_row_bytes = reference.width as usize * 4;
      let mut difference = RgbaDifference::default();
      let mut squared_error = 0_u64;
      for row in 0..reference.height as usize
      {
         let reference_start = row * reference.row_bytes as usize;
         let candidate_start = row * candidate.row_bytes as usize;
         let reference_row = &reference.pixels[reference_start..reference_start + tight_row_bytes];
         let candidate_row = &candidate.pixels[candidate_start..candidate_start + tight_row_bytes];
         for (reference_pixel, candidate_pixel) in
            reference_row.chunks_exact(4).zip(candidate_row.chunks_exact(4))
         {
            let mut pixel_differs = false;
            for channel in 0..4
            {
               let channel_error = reference_pixel[channel].abs_diff(candidate_pixel[channel]);
               pixel_differs |= channel_error != 0;
               difference.max_channel_error = difference.max_channel_error.max(channel_error);
               squared_error = squared_error.saturating_add(u64::from(channel_error).pow(2));
            }
            difference.differing_pixels += u64::from(pixel_differs);
         }
      }
      let channel_count = u64::from(reference.width)
         .saturating_mul(u64::from(reference.height))
         .saturating_mul(4);
      if channel_count != 0
      {
         difference.mean_squared_error = squared_error as f64 / channel_count as f64;
      }
      if difference.differing_pixels > tolerance.differing_pixels
         || difference.max_channel_error > tolerance.max_channel_error
         || difference.mean_squared_error > tolerance.mean_squared_error
      {
         return Err(RgbaParityError::ToleranceExceeded(difference));
      }
      Ok(difference)
   }

   fn validate_image(image: RgbaImage<'_>) -> Result<(), RgbaParityError>
   {
      let tight_row_bytes = image.width.checked_mul(4).ok_or(RgbaParityError::RowBytes)?;
      if image.row_bytes < tight_row_bytes
      {
         return Err(RgbaParityError::RowBytes);
      }
      let required_bytes = u64::from(image.row_bytes)
         .checked_mul(u64::from(image.height))
         .ok_or(RgbaParityError::PixelBuffer)?;
      if (image.pixels.len() as u64) < required_bytes
      {
         return Err(RgbaParityError::PixelBuffer);
      }
      Ok(())
   }
}

#[cfg(feature = "headless-vulkan")]
mod headless
{
   use std::num::NonZeroU64;
   use std::sync::atomic::{AtomicU64, Ordering};
   use std::sync::mpsc;

   use bytemuck::{Pod, Zeroable};
   use oxide_image_store::ImageResidencyBackend;
   use oxide_renderer_api as api;
   use crate::{image, UI_WGSL};

   // Chromium configures its WebGPU canvas as a non-sRGB BGRA target. Readback
   // must therefore retain the linear RGBA8 shader values without an attachment
   // transfer conversion.
   const OUTPUT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
   const OUTPUT_TRANSFER: &str = "linear-unorm";
   const DIRECT_RGBA_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
   // This is the source of viewport opacity for the headless target.  Keep
   // opaque replacement tied to it if the viewport uniform ever changes.
   const HEADLESS_VIEWPORT_OPACITY: f32 = 1.0;
   const IMAGE_STORE_RGBA_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
   const COPY_ALIGNMENT: u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
   const INITIAL_VERTEX_BYTES: u64 = 64 * 1024;
   const INITIAL_INDEX_BYTES: u64 = 32 * 1024;
   const INITIAL_RRECT_BYTES: u64 = 16 * 1024;
   static NEXT_DEVICE_GENERATION: AtomicU64 = AtomicU64::new(1);

   /// The only adapter classes accepted by a native headless renderer.
   ///
   /// `Cpu` preserves the artifact renderer's deterministic Lavapipe policy.
   /// `DiscreteGpu` is an explicit opt-in for hardware measurements and rejects
   /// every adapter whose PCI vendor or device identity differs.
   #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   pub enum HeadlessAdapterPolicy
   {
      Cpu,
      DiscreteGpu { vendor: u32, device: u32 },
   }

   /// Stable identity recorded with every headless-rendered artifact.
   #[derive(Clone, Debug, PartialEq, Eq)]
   pub struct CpuAdapterReceipt
   {
      pub name: String,
      pub vendor: u32,
      pub device: u32,
      pub driver: String,
      pub driver_info: String,
      pub backend: String,
      pub device_type: String,
      pub vk_driver_files: Option<String>,
      pub features: String,
      pub limits: CpuAdapterLimits,
      /// Final offscreen attachment identity for artifact comparison receipts.
      pub output_format: String,
      pub output_transfer: String,
   }

   #[derive(Clone, Debug, PartialEq, Eq)]
   pub struct CpuAdapterLimits
   {
      pub max_texture_dimension_2d: u32,
      pub max_bind_groups: u32,
      pub max_uniform_buffer_binding_size: u32,
      pub max_buffer_size: u64,
   }

   impl CpuAdapterReceipt
   {
      fn from_adapter(adapter: &wgpu::Adapter, info: &wgpu::AdapterInfo) -> Self
      {
         let limits = adapter.limits();
         Self {
            name: info.name.clone(),
            vendor: info.vendor,
            device: info.device,
            driver: info.driver.clone(),
            driver_info: info.driver_info.clone(),
            backend: format!("{:?}", info.backend),
            device_type: format!("{:?}", info.device_type),
            vk_driver_files: std::env::var_os("VK_DRIVER_FILES")
               .map(|value| value.to_string_lossy().into_owned()),
            features: format!("{:?}", adapter.features()),
            limits: CpuAdapterLimits {
               max_texture_dimension_2d: limits.max_texture_dimension_2d,
               max_bind_groups: limits.max_bind_groups,
               max_uniform_buffer_binding_size: limits.max_uniform_buffer_binding_size,
               max_buffer_size: limits.max_buffer_size,
            },
            output_format: format!("{OUTPUT_FORMAT:?}"),
            output_transfer: OUTPUT_TRANSFER.into(),
         }
      }
   }

   fn validate_adapter_policy(adapter_policy: HeadlessAdapterPolicy, backend: wgpu::Backend, device_type: wgpu::DeviceType, vendor: u32, device: u32) -> Result<(), api::RenderError>
   {
      if backend != wgpu::Backend::Vulkan
      {
         return Err(api::RenderError::Unsupported("headless wgpu requires Vulkan"));
      }
      match adapter_policy
      {
         HeadlessAdapterPolicy::Cpu if device_type == wgpu::DeviceType::Cpu => Ok(()),
         HeadlessAdapterPolicy::Cpu => Err(api::RenderError::Unsupported("headless wgpu requires a CPU adapter")),
         HeadlessAdapterPolicy::DiscreteGpu { vendor: expected_vendor, device: expected_device }
            if device_type == wgpu::DeviceType::DiscreteGpu && vendor == expected_vendor && device == expected_device => Ok(()),
         HeadlessAdapterPolicy::DiscreteGpu { .. } => Err(api::RenderError::Unsupported("headless wgpu discrete GPU adapter identity mismatch")),
      }
   }

   /// Explicit coverage receipt for the reduced native lowering path.
   #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   pub struct HeadlessWgpuCompatibility
   {
      pub full_draw_list: bool,
      pub basic_primitives: bool,
      pub images_and_glyphs: bool,
      pub clips: bool,
      pub layers: bool,
      pub backdrop_effects: bool,
      pub camera: bool,
      pub scene3d: bool,
      pub id_mask: bool,
      pub camera_draw_command_noop: bool,
   }

   #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   pub enum UnsupportedHeadlessWgpuCapability
   {
      LayerCachingAndComposition,
      BackdropAndVisualEffects,
      CameraFrameComposition,
      Scene3d,
      IdMask,
   }

   /// Operations outside the renderer-api draw list must ask the headless backend
   /// for support before encoding. This prevents browser-only Scene3D and ID-mask
   /// packets from being silently discarded by native snapshot callers.
   #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   pub enum HeadlessWgpuCapability
   {
      LayerCachingAndComposition,
      BackdropAndVisualEffects,
      CameraFrameComposition,
      Scene3d,
      IdMask,
   }

   #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
   pub struct DrawListCompatibilityReport
   {
      pub layer_begin_commands: u32,
      pub layer_end_commands: u32,
      pub backdrop_commands: u32,
      pub visual_effect_commands: u32,
      pub camera_noop_commands: u32,
   }

   impl DrawListCompatibilityReport
   {
      #[must_use]
      pub const fn is_compatible(self) -> bool
      {
         self.layer_begin_commands == 0
            && self.layer_end_commands == 0
            && self.backdrop_commands == 0
            && self.visual_effect_commands == 0
      }
   }

   /// Tightly packed linear RGBA8 pixels read from the unorm offscreen target.
   #[derive(Clone, Debug, PartialEq, Eq)]
   pub struct RgbaFrame
   {
      pub width: u32,
      pub height: u32,
      pub row_bytes: u32,
      pub pixels: Vec<u8>,
      #[cfg(feature = "benchmark-timings")]
      pub stage_timings_ns: [u64; 5],
   }

   impl RgbaFrame
   {
      #[must_use]
      pub fn as_rgba_image(&self) -> crate::parity::RgbaImage<'_>
      {
         crate::parity::RgbaImage {
            width: self.width,
            height: self.height,
            row_bytes: self.row_bytes,
            pixels: &self.pixels,
         }
      }
   }

   #[repr(C)]
   #[derive(Clone, Copy, Pod, Zeroable)]
   struct PackedVertex
   {
      position: [f32; 2],
      uv: [f32; 2],
      rgba: u32,
   }

   #[repr(C)]
   #[derive(Clone, Copy, Pod, Zeroable)]
   struct RRectInstance
   {
      rect: [f32; 4],
      radii: [f32; 4],
      rgba: u32,
   }

   #[derive(Clone, Copy, PartialEq, Eq)]
   enum ImageKind
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
      kind: ImageKind,
      #[cfg(feature = "benchmark-timings")]
      known_opaque: bool,
   }

   #[derive(Clone, Copy)]
   struct PhysicalClip
   {
      x: u32,
      y: u32,
      width: u32,
      height: u32,
   }

   #[derive(Clone, Copy)]
   enum BatchKind
   {
      Solid,
      Rgba(api::ImageHandle),
      A8(api::ImageHandle),
      Sdf(api::ImageHandle),
      RRect,
   }

   struct DrawBatch
   {
      kind: BatchKind,
      first: u32,
      count: u32,
      clip: PhysicalClip,
      #[cfg(feature = "benchmark-timings")]
      opaque_rgba: bool,
   }

   /// Per-frame upper bounds for the opaque-RGBA replace-pipeline candidate.
   /// `eligible_clipped_pixels` deliberately sums scissors, so overlapping
   /// batches may make it larger than the number of distinct output pixels.
   #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
   pub struct OpaqueRgbaCounters
   {
      pub eligible_batches: u32,
      pub eligible_clipped_pixels: u64,
   }

   struct Programs
   {
      solid: wgpu::RenderPipeline,
      rgba: wgpu::RenderPipeline,
      #[cfg(feature = "benchmark-timings")]
      rgba_opaque: wgpu::RenderPipeline,
      a8: wgpu::RenderPipeline,
      sdf: wgpu::RenderPipeline,
      rrect: wgpu::RenderPipeline,
      viewport_layout: wgpu::BindGroupLayout,
      texture_layout: wgpu::BindGroupLayout,
      sampler: wgpu::Sampler,
   }

   /// Native Vulkan headless renderer. Construction rejects every non-CPU adapter.
   pub struct HeadlessWgpuRenderer
   {
      _instance: wgpu::Instance,
      _adapter: wgpu::Adapter,
      device: wgpu::Device,
      queue: wgpu::Queue,
      receipt: CpuAdapterReceipt,
      device_generation: u64,
      programs: Programs,
      viewport_buffer: wgpu::Buffer,
      viewport_bind_group: wgpu::BindGroup,
      target: wgpu::Texture,
      target_view: wgpu::TextureView,
      vertex_buffer: wgpu::Buffer,
      vertex_capacity: u64,
      index_buffer: wgpu::Buffer,
      index_capacity: u64,
      rrect_buffer: wgpu::Buffer,
      rrect_capacity: u64,
      images: Vec<Option<GpuImage>>,
      vertices: Vec<PackedVertex>,
      indices: Vec<u32>,
      rrects: Vec<RRectInstance>,
      batches: Vec<DrawBatch>,
      clip_stack: Vec<api::RectI>,
      layer_depth: usize,
      #[cfg(feature = "benchmark-timings")]
      opaque_rgba_counters: OpaqueRgbaCounters,
      #[cfg(feature = "benchmark-timings")]
      benchmark_opaque_rgba_enabled: bool,
      pending_error: Option<api::RenderError>,
      active_token: Option<api::FrameToken>,
      next_frame: u64,
      width: u32,
      height: u32,
      scale: f32,
   }

   impl HeadlessWgpuRenderer
   {
      /// Requests the default CPU Vulkan fallback adapter and rejects every other adapter.
      pub fn new(width: u32, height: u32, scale: f32) -> Result<Self, api::RenderError>
      {
         pollster::block_on(Self::request(width, height, scale))
      }

      pub async fn request(width: u32, height: u32, scale: f32) -> Result<Self, api::RenderError>
      {
         Self::request_with_adapter_policy(width, height, scale, HeadlessAdapterPolicy::Cpu).await
      }

      /// Requests one explicit Vulkan adapter policy and rejects every mismatch.
      pub fn new_with_adapter_policy(width: u32, height: u32, scale: f32, adapter_policy: HeadlessAdapterPolicy) -> Result<Self, api::RenderError>
      {
         pollster::block_on(Self::request_with_adapter_policy(width, height, scale, adapter_policy))
      }

      pub async fn request_with_adapter_policy(width: u32, height: u32, scale: f32, adapter_policy: HeadlessAdapterPolicy) -> Result<Self, api::RenderError>
      {
         validate_dimensions(width, height, scale)?;
         let driver_files_present = std::env::var_os("VK_DRIVER_FILES")
            .is_some_and(|value| !value.is_empty());
         if !driver_files_present
         {
            return Err(api::RenderError::Unsupported("headless wgpu requires explicit VK_DRIVER_FILES"));
         }
         if !wgpu::Instance::enabled_backend_features().contains(wgpu::Backends::VULKAN)
         {
            return Err(api::RenderError::Unsupported("Vulkan is not compiled for this target"));
         }
         let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
         });
         let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
               power_preference: match adapter_policy {
                  HeadlessAdapterPolicy::Cpu => wgpu::PowerPreference::LowPower,
                  HeadlessAdapterPolicy::DiscreteGpu { .. } => wgpu::PowerPreference::HighPerformance,
               },
               force_fallback_adapter: matches!(adapter_policy, HeadlessAdapterPolicy::Cpu),
               compatible_surface: None,
            })
            .await
            .map_err(|error| api::RenderError::Io(error.to_string()))?;
         let info = adapter.get_info();
         validate_adapter_policy(adapter_policy, info.backend, info.device_type, info.vendor, info.device)?;
         let format_features = adapter.get_texture_format_features(OUTPUT_FORMAT);
         let required_usage = wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::TEXTURE_BINDING;
         if !format_features.allowed_usages.contains(required_usage)
         {
            return Err(api::RenderError::Unsupported("headless Vulkan adapter lacks RGBA8 unorm render/readback support"));
         }
         let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
               label: Some("oxide-headless-wgpu-device"),
               required_features: wgpu::Features::empty(),
               required_limits: wgpu::Limits::default(),
               memory_hints: wgpu::MemoryHints::MemoryUsage,
               trace: wgpu::Trace::Off,
            })
            .await
            .map_err(|error| api::RenderError::Io(error.to_string()))?;
         let receipt = CpuAdapterReceipt::from_adapter(&adapter, &info);
         let programs = create_programs(&device);
         let viewport_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("oxide-headless-wgpu-viewport"),
            size: 48,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
         });
         let viewport_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("oxide-headless-wgpu-viewport-bind-group"),
            layout: &programs.viewport_layout,
            entries: &[wgpu::BindGroupEntry {
               binding: 0,
               resource: viewport_buffer.as_entire_binding(),
            }],
         });
         let (target, target_view) = create_target(&device, width, height);
         let vertex_buffer = create_buffer(
            &device,
            "oxide-headless-wgpu-vertices",
            INITIAL_VERTEX_BYTES,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
         );
         let index_buffer = create_buffer(
            &device,
            "oxide-headless-wgpu-indices",
            INITIAL_INDEX_BYTES,
            wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
         );
         let rrect_buffer = create_buffer(
            &device,
            "oxide-headless-wgpu-rrects",
            INITIAL_RRECT_BYTES,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
         );
         let renderer = Self {
            _instance: instance,
            _adapter: adapter,
            device,
            queue,
            receipt,
            device_generation: NEXT_DEVICE_GENERATION.fetch_add(1, Ordering::Relaxed).max(1),
            programs,
            viewport_buffer,
            viewport_bind_group,
            target,
            target_view,
            vertex_buffer,
            vertex_capacity: INITIAL_VERTEX_BYTES,
            index_buffer,
            index_capacity: INITIAL_INDEX_BYTES,
            rrect_buffer,
            rrect_capacity: INITIAL_RRECT_BYTES,
            images: vec![None],
            vertices: Vec::new(),
            indices: Vec::new(),
            rrects: Vec::new(),
            batches: Vec::new(),
            clip_stack: Vec::new(),
            layer_depth: 0,
            #[cfg(feature = "benchmark-timings")]
            opaque_rgba_counters: OpaqueRgbaCounters::default(),
            #[cfg(feature = "benchmark-timings")]
            benchmark_opaque_rgba_enabled: true,
            pending_error: None,
            active_token: None,
            next_frame: 1,
            width,
            height,
            scale,
         };
         renderer.write_viewport();
         Ok(renderer)
      }

      #[must_use]
      pub fn adapter_receipt(&self) -> &CpuAdapterReceipt
      {
         &self.receipt
      }

      #[must_use]
      pub const fn output_format(&self) -> wgpu::TextureFormat
      {
         OUTPUT_FORMAT
      }

      #[must_use]
      pub const fn opaque_rgba_counters(&self) -> OpaqueRgbaCounters
      {
         #[cfg(feature = "benchmark-timings")]
         {
            self.opaque_rgba_counters
         }
         #[cfg(not(feature = "benchmark-timings"))]
         {
            OpaqueRgbaCounters { eligible_batches: 0, eligible_clipped_pixels: 0 }
         }
      }

      /// Benchmark-only same-binary A/B control; production rendering never
      /// reads this from its environment.
      #[cfg(feature = "benchmark-timings")]
      pub fn set_benchmark_opaque_rgba_enabled(&mut self, enabled: bool)
      {
         self.benchmark_opaque_rgba_enabled = enabled;
      }

      #[must_use]
      pub const fn compatibility() -> HeadlessWgpuCompatibility
      {
         HeadlessWgpuCompatibility {
            full_draw_list: false,
            basic_primitives: true,
            images_and_glyphs: true,
            clips: true,
            layers: false,
            backdrop_effects: false,
            camera: false,
            scene3d: false,
            id_mask: false,
            camera_draw_command_noop: true,
         }
      }

      #[must_use]
      pub const fn residual_unsupported_capabilities() -> &'static [UnsupportedHeadlessWgpuCapability]
      {
         &[
            UnsupportedHeadlessWgpuCapability::LayerCachingAndComposition,
            UnsupportedHeadlessWgpuCapability::BackdropAndVisualEffects,
            UnsupportedHeadlessWgpuCapability::CameraFrameComposition,
            UnsupportedHeadlessWgpuCapability::Scene3d,
            UnsupportedHeadlessWgpuCapability::IdMask,
         ]
      }

      #[must_use]
      pub fn draw_list_compatibility_report(list: &api::DrawList) -> DrawListCompatibilityReport
      {
         let mut report = DrawListCompatibilityReport::default();
         for command in &list.items
         {
            match command
            {
               api::DrawCmd::LayerBegin { .. } => {
                  report.layer_begin_commands = report.layer_begin_commands.saturating_add(1);
               }
               api::DrawCmd::LayerEnd => {
                  report.layer_end_commands = report.layer_end_commands.saturating_add(1);
               }
               api::DrawCmd::Backdrop { .. } => {
                  report.backdrop_commands = report.backdrop_commands.saturating_add(1);
               }
               api::DrawCmd::VisualEffect { .. } => {
                  report.visual_effect_commands = report.visual_effect_commands.saturating_add(1);
               }
               api::DrawCmd::CameraBg { .. } => {
                  report.camera_noop_commands = report.camera_noop_commands.saturating_add(1);
               }
               _ => {}
            }
         }
         report
      }

      pub fn validate_draw_list_compatibility(list: &api::DrawList) -> Result<(), api::RenderError>
      {
         let report = Self::draw_list_compatibility_report(list);
         if report.layer_begin_commands != 0 || report.layer_end_commands != 0
         {
            return Err(api::RenderError::Unsupported("headless wgpu layer semantics are not implemented"));
         }
         if report.backdrop_commands != 0 || report.visual_effect_commands != 0
         {
            return Err(api::RenderError::Unsupported("headless wgpu backdrop effects are not implemented"));
         }
         Ok(())
      }

      pub fn require_capability(capability: HeadlessWgpuCapability) -> Result<(), api::RenderError>
      {
         let message = match capability
         {
            HeadlessWgpuCapability::LayerCachingAndComposition => "headless wgpu layer caching and composition are not implemented",
            HeadlessWgpuCapability::BackdropAndVisualEffects => "headless wgpu backdrop and visual effects are not implemented",
            HeadlessWgpuCapability::CameraFrameComposition => "headless wgpu camera frame composition is not implemented",
            HeadlessWgpuCapability::Scene3d => "headless wgpu Scene3D is not implemented",
            HeadlessWgpuCapability::IdMask => "headless wgpu ID-mask composition is not implemented",
         };
         Err(api::RenderError::Unsupported(message))
      }

      pub fn read_rgba(&self) -> Result<RgbaFrame, api::RenderError>
      {
         #[cfg(feature = "benchmark-timings")]
         let started = std::time::Instant::now();
         let padded_row_bytes = padded_bytes_per_row(self.width)?;
         let size = u64::from(padded_row_bytes)
            .checked_mul(u64::from(self.height))
            .ok_or(api::RenderError::InvalidOperation("readback buffer size overflow"))?;
         let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("oxide-headless-wgpu-readback"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
         });
         #[cfg(feature = "benchmark-timings")]
         let allocate_ns = started.elapsed().as_nanos() as u64;
         #[cfg(feature = "benchmark-timings")]
         let started = std::time::Instant::now();
         let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("oxide-headless-wgpu-readback-encoder"),
         });
         encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
               texture: &self.target,
               mip_level: 0,
               origin: wgpu::Origin3d::ZERO,
               aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
               buffer: &buffer,
               layout: wgpu::TexelCopyBufferLayout {
                  offset: 0,
                  bytes_per_row: Some(padded_row_bytes),
                  rows_per_image: Some(self.height),
               },
            },
            wgpu::Extent3d {
               width: self.width,
               height: self.height,
               depth_or_array_layers: 1,
            },
         );
         let commands = encoder.finish();
         #[cfg(feature = "benchmark-timings")]
         let record_ns = started.elapsed().as_nanos() as u64;
         #[cfg(feature = "benchmark-timings")]
         let started = std::time::Instant::now();
         let submission = self.queue.submit(Some(commands));
         let (sender, receiver) = mpsc::sync_channel(1);
         buffer.slice(..).map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
         });
         #[cfg(feature = "benchmark-timings")]
         let submit_ns = started.elapsed().as_nanos() as u64;
         #[cfg(feature = "benchmark-timings")]
         let started = std::time::Instant::now();
         self.device
            .poll(wgpu::PollType::WaitForSubmissionIndex(submission))
            .map_err(|error| api::RenderError::Io(error.to_string()))?;
         receiver
            .recv()
            .map_err(|error| api::RenderError::Io(error.to_string()))?
            .map_err(|error| api::RenderError::Io(error.to_string()))?;
         let mapped = buffer.slice(..).get_mapped_range();
         #[cfg(feature = "benchmark-timings")]
         let wait_ns = started.elapsed().as_nanos() as u64;
         #[cfg(feature = "benchmark-timings")]
         let started = std::time::Instant::now();
         let row_bytes = self.width.saturating_mul(4);
         let pixels = unpack_padded_rows(&mapped, self.width, self.height, padded_row_bytes)?;
         drop(mapped);
         buffer.unmap();
         Ok(RgbaFrame {
            width: self.width, height: self.height, row_bytes, pixels,
            #[cfg(feature = "benchmark-timings")]
            stage_timings_ns: [allocate_ns, record_ns, submit_ns, wait_ns, started.elapsed().as_nanos() as u64],
         })
      }

      pub fn try_image_create_rgba8(&mut self, width: u32, height: u32, data: &[u8], row_bytes: usize) -> Result<api::ImageHandle, api::RenderError>
      {
         self.create_image(width, height, ImageKind::Rgba, data, row_bytes, false, DIRECT_RGBA_FORMAT)
      }

      /// Creates a direct RGBA texture whose producer has already established
      /// that every uploaded texel has alpha 255. This does not rescan pixels;
      /// callers without that existing proof must use `try_image_create_rgba8`.
      #[cfg(feature = "benchmark-timings")]
      pub fn try_image_create_known_opaque_rgba8(&mut self, width: u32, height: u32, data: &[u8], row_bytes: usize) -> Result<api::ImageHandle, api::RenderError>
      {
         self.create_image_inner(width, height, ImageKind::Rgba, data, row_bytes, false, DIRECT_RGBA_FORMAT, true)
      }

      pub fn try_image_create_a8(&mut self, width: u32, height: u32, data: &[u8], row_bytes: usize) -> Result<api::ImageHandle, api::RenderError>
      {
         self.create_image(width, height, ImageKind::A8, data, row_bytes, false, DIRECT_RGBA_FORMAT)
      }

      fn create_image(&mut self, width: u32, height: u32, kind: ImageKind, data: &[u8], row_bytes: usize, mipmapped: bool, rgba_format: wgpu::TextureFormat) -> Result<api::ImageHandle, api::RenderError>
      {
         self.create_image_inner(width, height, kind, data, row_bytes, mipmapped, rgba_format, false)
      }

      #[allow(unused_variables)]
      fn create_image_inner(&mut self, width: u32, height: u32, kind: ImageKind, data: &[u8], row_bytes: usize, mipmapped: bool, rgba_format: wgpu::TextureFormat, known_opaque: bool) -> Result<api::ImageHandle, api::RenderError>
      {
         let bytes_per_pixel = if kind == ImageKind::Rgba { 4 } else { 1 };
         validate_image_rows(width, height, data, row_bytes, bytes_per_pixel)?;
         let rgba_mips = (kind == ImageKind::Rgba && mipmapped).then(|| {
            image::copy_rgba_rows(width, height, data, row_bytes)
               .map(|rgba| image::rgba8_srgb_mip_chain(width, height, rgba))
               .ok_or(api::RenderError::InvalidOperation("invalid rgba image rows"))
         }).transpose()?;
         let packed_row = width as usize * bytes_per_pixel;
         let a8 = if kind == ImageKind::A8 {
            if row_bytes == packed_row { data[..packed_row * height as usize].to_vec() }
            else {
               let mut packed = Vec::with_capacity(packed_row * height as usize);
               for row in 0..height as usize
               {
                  let start = row * row_bytes;
                  packed.extend_from_slice(&data[start..start + packed_row]);
               }
               packed
            }
         } else { Vec::new() };
         let format = if kind == ImageKind::Rgba { rgba_format } else { wgpu::TextureFormat::R8Unorm };
         let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("oxide-headless-wgpu-image"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: rgba_mips.as_ref().map_or(1, |levels| levels.len() as u32),
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
         });
         if let Some(levels) = rgba_mips
         {
            for (mip_level, level) in levels.iter().enumerate()
            {
               self.queue.write_texture(
                  wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: mip_level as u32, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                  &level.rgba,
                  wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(level.width * 4), rows_per_image: Some(level.height) },
                  wgpu::Extent3d { width: level.width, height: level.height, depth_or_array_layers: 1 },
               );
            }
         }
         else
         {
            let upload = if kind == ImageKind::Rgba {
               image::copy_rgba_rows(width, height, data, row_bytes)
                  .ok_or(api::RenderError::InvalidOperation("invalid rgba image rows"))?
            }
            else { a8 };
            self.queue.write_texture(
               wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
               &upload,
               wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width * bytes_per_pixel as u32), rows_per_image: Some(height) },
               wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            );
         }
         let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
         let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("oxide-headless-wgpu-image-bind-group"),
            layout: &self.programs.texture_layout,
            entries: &[
               wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
               wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.programs.sampler) },
            ],
         });
         let image = GpuImage {
            texture, bind_group, width, height, kind,
            #[cfg(feature = "benchmark-timings")]
            known_opaque,
         };
         let slot = self.images.len() as u32;
         self.images.push(Some(image));
         Ok(api::ImageHandle(slot))
      }

      fn write_viewport(&self)
      {
         let logical_width = self.width as f32 / self.scale;
         let logical_height = self.height as f32 / self.scale;
         let values = [
            logical_width, logical_height, 0.0, 0.0,
            1.0, 0.0, 0.0, 1.0,
            0.0, 0.0, HEADLESS_VIEWPORT_OPACITY, 0.0,
         ];
         self.queue.write_buffer(&self.viewport_buffer, 0, bytemuck::cast_slice(&values));
      }

      fn full_clip(&self) -> PhysicalClip
      {
         PhysicalClip { x: 0, y: 0, width: self.width, height: self.height }
      }

      fn active_clip(&self) -> PhysicalClip
      {
         self.clip_stack.last().map_or_else(|| self.full_clip(), |rect| {
            physical_clip(*rect, self.scale, self.width, self.height)
         })
      }

      fn image(&self, handle: api::ImageHandle) -> Option<&GpuImage>
      {
         self.images.get(handle.0 as usize).and_then(Option::as_ref)
      }

      fn record_error(&mut self, error: api::RenderError)
      {
         if self.pending_error.is_none()
         {
            self.pending_error = Some(error);
         }
      }

      fn encode_list(&mut self, list: &api::DrawList)
      {
         if let Err(error) = Self::validate_draw_list_compatibility(list)
         {
            self.record_error(error);
            return;
         }
         for command in &list.items
         {
            match command
            {
               api::DrawCmd::LayerBegin { .. } => self.layer_depth = self.layer_depth.saturating_add(1),
               api::DrawCmd::LayerEnd => self.layer_depth = self.layer_depth.saturating_sub(1),
               api::DrawCmd::Solid { vb, ib, color } => self.encode_solid(list, *vb, *ib, *color),
               api::DrawCmd::Image { tex, dst, src, alpha } => self.encode_image(*tex, *dst, *src, *alpha),
               api::DrawCmd::ImageMesh { tex, vb, ib, alpha } => self.encode_image_mesh(list, *tex, *vb, *ib, *alpha),
               api::DrawCmd::GlyphRun { run } => self.encode_glyph(list, run),
               api::DrawCmd::RRect { rect, radii, color } => self.encode_rrect(*rect, *radii, *color),
               api::DrawCmd::NineSlice { tex, rect, slice, alpha } => self.encode_nine_slice(*tex, *rect, *slice, *alpha),
               api::DrawCmd::Backdrop { .. } | api::DrawCmd::VisualEffect { .. } => {
                  unreachable!("compatibility checked before lowering")
               }
               api::DrawCmd::CameraBg { .. } => {}
               api::DrawCmd::Spinner { center, atom, alpha } => self.encode_spinner(*center, *atom, *alpha),
               api::DrawCmd::ClipPush { rect } => self.clip_stack.push(*rect),
               api::DrawCmd::ClipPop => {
                  if self.clip_stack.pop().is_none()
                  {
                     self.record_error(api::RenderError::InvalidOperation("unbalanced clip pop"));
                  }
               }
            }
         }
         if self.layer_depth != 0
         {
            self.record_error(api::RenderError::InvalidOperation("unbalanced layer scope"));
         }
         if !self.clip_stack.is_empty()
         {
            self.record_error(api::RenderError::InvalidOperation("unbalanced clip scope"));
            self.clip_stack.clear();
         }
      }

      fn encode_solid(&mut self, list: &api::DrawList, vb: api::VertexSpan, ib: api::IndexSpan, color: api::Color)
      {
         let Some(vertices) = vertex_span(list, vb) else
         {
            self.record_error(api::RenderError::InvalidOperation("solid vertex span outside draw list"));
            return;
         };
         let Some(indices) = index_span_or_implicit(list, ib, vertices.len()) else
         {
            self.record_error(api::RenderError::InvalidOperation("solid index span outside draw list"));
            return;
         };
         let first = self.indices.len() as u32;
         append_vertices(&mut self.vertices, &mut self.indices, vertices, &indices, vb.offset, color, true);
         self.push_index_batch(BatchKind::Solid, first, false, false);
      }

      fn encode_image(&mut self, handle: api::ImageHandle, dst: api::RectF, src: api::RectF, alpha: f32)
      {
         let Some(image) = self.image(handle) else
         {
            self.record_error(api::RenderError::ResourceNotFound("image handle not found"));
            return;
         };
         let kind = image.kind;
         let width = image.width;
         let height = image.height;
         #[cfg(feature = "benchmark-timings")]
         let eligible_opaque_rgba = image.kind == ImageKind::Rgba && image.known_opaque && alpha == 1.0 && HEADLESS_VIEWPORT_OPACITY == 1.0;
         #[cfg(not(feature = "benchmark-timings"))]
         let eligible_opaque_rgba = false;
         #[cfg(feature = "benchmark-timings")]
         let opaque_rgba = eligible_opaque_rgba && self.benchmark_opaque_rgba_enabled;
         #[cfg(not(feature = "benchmark-timings"))]
         let opaque_rgba = false;
         let (u0, v0, u1, v1) = normalized_source(src, width, height);
         let first = self.indices.len() as u32;
         append_quad(
            &mut self.vertices,
            &mut self.indices,
            dst,
            [u0, v0, u1, v1],
            api::Color::rgba(1.0, 1.0, 1.0, alpha.clamp(0.0, 1.0)).pack_rgba8(),
         );
         self.push_index_batch(image_batch(kind, handle), first, opaque_rgba, eligible_opaque_rgba);
      }

      fn encode_image_mesh(&mut self, list: &api::DrawList, handle: api::ImageHandle, vb: api::VertexSpan, ib: api::IndexSpan, alpha: f32)
      {
         let Some(image) = self.image(handle) else
         {
            self.record_error(api::RenderError::ResourceNotFound("image handle not found"));
            return;
         };
         let kind = image.kind;
         // append_vertices intentionally replaces mesh vertex colour with this
         // command alpha, so alpha == 1.0 is the complete modulation proof.
         #[cfg(feature = "benchmark-timings")]
         let eligible_opaque_rgba = image.kind == ImageKind::Rgba && image.known_opaque && alpha == 1.0 && HEADLESS_VIEWPORT_OPACITY == 1.0;
         #[cfg(not(feature = "benchmark-timings"))]
         let eligible_opaque_rgba = false;
         #[cfg(feature = "benchmark-timings")]
         let opaque_rgba = eligible_opaque_rgba && self.benchmark_opaque_rgba_enabled;
         #[cfg(not(feature = "benchmark-timings"))]
         let opaque_rgba = false;
         let Some(vertices) = vertex_span(list, vb) else
         {
            self.record_error(api::RenderError::InvalidOperation("image vertex span outside draw list"));
            return;
         };
         let Some(indices) = index_span_or_implicit(list, ib, vertices.len()) else
         {
            self.record_error(api::RenderError::InvalidOperation("image index span outside draw list"));
            return;
         };
         let first = self.indices.len() as u32;
         append_vertices(
            &mut self.vertices,
            &mut self.indices,
            vertices,
            &indices,
            vb.offset,
            api::Color::rgba(1.0, 1.0, 1.0, alpha.clamp(0.0, 1.0)),
            false,
         );
         self.push_index_batch(image_batch(kind, handle), first, opaque_rgba, eligible_opaque_rgba);
      }

      fn encode_glyph(&mut self, list: &api::DrawList, run: &api::GlyphRun)
      {
         let Some(image) = self.image(run.atlas) else
         {
            self.record_error(api::RenderError::ResourceNotFound("glyph atlas handle not found"));
            return;
         };
         let kind = image.kind;
         let Some(vertices) = vertex_span(list, run.vb) else
         {
            self.record_error(api::RenderError::InvalidOperation("glyph vertex span outside draw list"));
            return;
         };
         let Some(indices) = index_span_or_implicit(list, run.ib, vertices.len()) else
         {
            self.record_error(api::RenderError::InvalidOperation("glyph index span outside draw list"));
            return;
         };
         let first = self.indices.len() as u32;
         // Bitmap-atlas vertices carry opaque white as their source color.  A
         // glyph run's color is authoritative (not a fallback), matching the
         // browser lowerer.  Preserving the source white made black text—most
         // visibly the username inside the white Nametag—render white-on-white.
         append_vertices(
            &mut self.vertices,
            &mut self.indices,
            vertices,
            &indices,
            run.vb.offset,
            run.color,
            false,
         );
         let batch = if run.sdf { BatchKind::Sdf(run.atlas) } else { image_batch(kind, run.atlas) };
         self.push_index_batch(batch, first, false, false);
      }

      fn encode_rrect(&mut self, rect: api::RectF, radii: [f32; 4], color: api::Color)
      {
         if rect.w <= 0.0 || rect.h <= 0.0
         {
            return;
         }
         let first = self.rrects.len() as u32;
         self.rrects.push(RRectInstance {
            rect: [rect.x, rect.y, rect.w, rect.h],
            radii,
            rgba: color.pack_rgba8(),
         });
         self.batches.push(DrawBatch {
            kind: BatchKind::RRect, first, count: 1, clip: self.active_clip(),
            #[cfg(feature = "benchmark-timings")]
            opaque_rgba: false,
         });
      }

      fn encode_nine_slice(&mut self, handle: api::ImageHandle, rect: api::RectF, slice: api::Insets, alpha: f32)
      {
         let Some(image) = self.image(handle) else
         {
            self.record_error(api::RenderError::ResourceNotFound("nine-slice image handle not found"));
            return;
         };
         let kind = image.kind;
         #[cfg(feature = "benchmark-timings")]
         let eligible_opaque_rgba = image.kind == ImageKind::Rgba && image.known_opaque && alpha == 1.0 && HEADLESS_VIEWPORT_OPACITY == 1.0;
         #[cfg(not(feature = "benchmark-timings"))]
         let eligible_opaque_rgba = false;
         #[cfg(feature = "benchmark-timings")]
         let opaque_rgba = eligible_opaque_rgba && self.benchmark_opaque_rgba_enabled;
         #[cfg(not(feature = "benchmark-timings"))]
         let opaque_rgba = false;
         let image_width = image.width as f32;
         let image_height = image.height as f32;
         let dx = [0.0, slice.left, (rect.w - slice.right).max(slice.left), rect.w];
         let dy = [0.0, slice.top, (rect.h - slice.bottom).max(slice.top), rect.h];
         let sx = [0.0, slice.left, image_width - slice.right, image_width];
         let sy = [0.0, slice.top, image_height - slice.bottom, image_height];
         let first = self.indices.len() as u32;
         for row in 0..3
         {
            for column in 0..3
            {
               if dx[column + 1] <= dx[column] || dy[row + 1] <= dy[row]
                  || sx[column + 1] <= sx[column] || sy[row + 1] <= sy[row]
               {
                  continue;
               }
               append_quad(
                  &mut self.vertices,
                  &mut self.indices,
                  api::RectF::new(rect.x + dx[column], rect.y + dy[row], dx[column + 1] - dx[column], dy[row + 1] - dy[row]),
                  [sx[column] / image_width, sy[row] / image_height, sx[column + 1] / image_width, sy[row + 1] / image_height],
                  api::Color::rgba(1.0, 1.0, 1.0, alpha.clamp(0.0, 1.0)).pack_rgba8(),
               );
            }
         }
         self.push_index_batch(image_batch(kind, handle), first, opaque_rgba, eligible_opaque_rgba);
      }

      fn encode_spinner(&mut self, center: [f32; 2], atom: f32, alpha: f32)
      {
         let first = self.rrects.len() as u32;
         let radius = atom * 0.12;
         for index in 0..12
         {
            let angle = index as f32 * core::f32::consts::TAU / 12.0;
            let dot_center = [
               center[0] + angle.cos() * (atom * 1.5).max(1.0),
               center[1] + angle.sin() * (atom * 1.5).max(1.0),
            ];
            let progress = index as f32 / 12.0;
            let dot_alpha = (alpha.clamp(0.0, 1.0) * (0.25 + progress * 0.75) * 255.0).round() / 255.0;
            self.rrects.push(RRectInstance {
               rect: [dot_center[0] - radius, dot_center[1] - radius, radius * 2.0, radius * 2.0],
               radii: [radius; 4],
               rgba: api::Color::rgba(1.0, 1.0, 1.0, dot_alpha).pack_rgba8(),
            });
         }
         self.batches.push(DrawBatch {
            kind: BatchKind::RRect, first, count: 12, clip: self.active_clip(),
            #[cfg(feature = "benchmark-timings")]
            opaque_rgba: false,
         });
      }

      fn push_index_batch(&mut self, kind: BatchKind, first: u32, opaque_rgba: bool, eligible_opaque_rgba: bool)
      {
         #[cfg(not(feature = "benchmark-timings"))]
         let _ = (opaque_rgba, eligible_opaque_rgba);
         let count = self.indices.len() as u32 - first;
         if count > 0
         {
            let clip = self.active_clip();
            #[cfg(feature = "benchmark-timings")]
            if eligible_opaque_rgba
            {
               self.opaque_rgba_counters.eligible_batches = self.opaque_rgba_counters.eligible_batches.saturating_add(1);
               self.opaque_rgba_counters.eligible_clipped_pixels = self.opaque_rgba_counters.eligible_clipped_pixels.saturating_add(u64::from(clip.width) * u64::from(clip.height));
            }
            self.batches.push(DrawBatch {
               kind, first, count, clip,
               #[cfg(feature = "benchmark-timings")]
               opaque_rgba,
            });
         }
      }

      fn ensure_buffers(&mut self)
      {
         ensure_buffer(
            &self.device,
            &mut self.vertex_buffer,
            &mut self.vertex_capacity,
            (self.vertices.len() * core::mem::size_of::<PackedVertex>()) as u64,
            "oxide-headless-wgpu-vertices",
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
         );
         ensure_buffer(
            &self.device,
            &mut self.index_buffer,
            &mut self.index_capacity,
            (self.indices.len() * core::mem::size_of::<u32>()) as u64,
            "oxide-headless-wgpu-indices",
            wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
         );
         ensure_buffer(
            &self.device,
            &mut self.rrect_buffer,
            &mut self.rrect_capacity,
            (self.rrects.len() * core::mem::size_of::<RRectInstance>()) as u64,
            "oxide-headless-wgpu-rrects",
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
         );
         if !self.vertices.is_empty()
         {
            self.queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&self.vertices));
         }
         if !self.indices.is_empty()
         {
            self.queue.write_buffer(&self.index_buffer, 0, bytemuck::cast_slice(&self.indices));
         }
         if !self.rrects.is_empty()
         {
            self.queue.write_buffer(&self.rrect_buffer, 0, bytemuck::cast_slice(&self.rrects));
         }
      }

      fn render(&mut self)
      {
         self.ensure_buffers();
         let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("oxide-headless-wgpu-frame-encoder"),
         });
         {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
               label: Some("oxide-headless-wgpu-frame"),
               color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                  view: &self.target_view,
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
            pass.set_bind_group(0, &self.viewport_bind_group, &[]);
            for batch in &self.batches
            {
               if batch.clip.width == 0 || batch.clip.height == 0
               {
                  continue;
               }
               pass.set_scissor_rect(batch.clip.x, batch.clip.y, batch.clip.width, batch.clip.height);
               match batch.kind
               {
                  BatchKind::Solid => {
                     pass.set_pipeline(&self.programs.solid);
                     pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                     pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                     pass.draw_indexed(batch.first..batch.first + batch.count, 0, 0..1);
                  }
                  BatchKind::Rgba(handle) | BatchKind::A8(handle) | BatchKind::Sdf(handle) => {
                     let image = self.image(handle).expect("validated image batch");
                     let pipeline = match batch.kind {
                        #[cfg(feature = "benchmark-timings")]
                        BatchKind::Rgba(_) if batch.opaque_rgba && image.known_opaque => &self.programs.rgba_opaque,
                        BatchKind::Rgba(_) => &self.programs.rgba,
                        BatchKind::A8(_) => &self.programs.a8,
                        BatchKind::Sdf(_) => &self.programs.sdf,
                        _ => unreachable!(),
                     };
                     pass.set_pipeline(pipeline);
                     pass.set_bind_group(1, &image.bind_group, &[]);
                     pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                     pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                     pass.draw_indexed(batch.first..batch.first + batch.count, 0, 0..1);
                  }
                  BatchKind::RRect => {
                     pass.set_pipeline(&self.programs.rrect);
                     pass.set_vertex_buffer(0, self.rrect_buffer.slice(..));
                     pass.draw(0..6, batch.first..batch.first + batch.count);
                  }
               }
            }
         }
         self.queue.submit(Some(encoder.finish()));
      }
   }

   impl api::Renderer for HeadlessWgpuRenderer
   {
      fn device_caps(&self) -> api::DeviceCaps
      {
         api::DeviceCaps {
            max_framerate_hz: 0,
            supports_edr: false,
            supports_msaa4x: false,
            native_scale: self.scale,
         }
      }

      fn begin_frame(&mut self, _fb: &api::FrameTarget, _damage: Option<&api::Damage>) -> api::FrameToken
      {
         let token = api::FrameToken(self.next_frame);
         self.next_frame = self.next_frame.saturating_add(1);
         self.active_token = Some(token);
         self.pending_error = None;
         self.vertices.clear();
         self.indices.clear();
         self.rrects.clear();
         self.batches.clear();
         self.clip_stack.clear();
         self.layer_depth = 0;
         #[cfg(feature = "benchmark-timings")]
         {
            self.opaque_rgba_counters = OpaqueRgbaCounters::default();
         }
         token
      }

      fn encode_pass(&mut self, list: &api::DrawList)
      {
         if self.active_token.is_none()
         {
            self.record_error(api::RenderError::InvalidOperation("encode outside active frame"));
            return;
         }
         self.encode_list(list);
      }

      fn submit(&mut self, token: api::FrameToken) -> Result<(), api::RenderError>
      {
         if self.active_token.take() != Some(token)
         {
            return Err(api::RenderError::InvalidOperation("invalid frame token"));
         }
         if let Some(error) = self.pending_error.take()
         {
            return Err(error);
         }
         self.render();
         Ok(())
      }

      fn resize(&mut self, width: u32, height: u32, scale: f32) -> Result<(), api::RenderError>
      {
         validate_dimensions(width, height, scale)?;
         self.width = width;
         self.height = height;
         self.scale = scale;
         (self.target, self.target_view) = create_target(&self.device, width, height);
         self.write_viewport();
         Ok(())
      }
   }

   impl api::RuntimeImageUploader for HeadlessWgpuRenderer
   {
      fn create_a8(&mut self, width: u32, height: u32, data: &[u8], row_bytes: usize) -> api::ImageHandle
      {
         self.try_image_create_a8(width, height, data, row_bytes).unwrap_or(api::ImageHandle(0))
      }

      fn update_a8(&mut self, handle: api::ImageHandle, x: u32, y: u32, width: u32, height: u32, data: &[u8], row_bytes: usize)
      {
         let _ = self.update_image(handle, x, y, width, height, ImageKind::A8, data, row_bytes);
      }
   }

   impl HeadlessWgpuRenderer
   {
      fn update_image(&mut self, handle: api::ImageHandle, x: u32, y: u32, width: u32, height: u32, kind: ImageKind, data: &[u8], row_bytes: usize) -> Result<(), api::RenderError>
      {
         let Some(image) = self.images.get(handle.0 as usize).and_then(Option::as_ref) else
         {
            return Err(api::RenderError::ResourceNotFound("image handle not found"));
         };
         if image.kind != kind || x.saturating_add(width) > image.width || y.saturating_add(height) > image.height
         {
            return Err(api::RenderError::InvalidOperation("image update outside compatible image"));
         }
         let bytes_per_pixel = if kind == ImageKind::Rgba { 4 } else { 1 };
         validate_image_rows(width, height, data, row_bytes, bytes_per_pixel)?;
         self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
               texture: &image.texture,
               mip_level: 0,
               origin: wgpu::Origin3d { x, y, z: 0 },
               aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
               offset: 0,
               bytes_per_row: Some(row_bytes as u32),
               rows_per_image: Some(height),
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
         );
         // An update can follow draw-list encoding before the frame is
         // submitted. Never scan the update on this hot path; conservatively
         // revoke the producer proof after a successful RGBA write instead.
         #[cfg(feature = "benchmark-timings")]
         if kind == ImageKind::Rgba
         {
            if let Some(Some(image)) = self.images.get_mut(handle.0 as usize)
            {
               image.known_opaque = false;
            }
         }
         Ok(())
      }
   }

   impl ImageResidencyBackend for HeadlessWgpuRenderer
   {
      fn image_device_generation(&self) -> u64
      {
         self.device_generation
      }

      fn image_create_rgba8(&mut self, width: u32, height: u32, data: &[u8], row_bytes: usize, mipmapped: bool) -> api::ImageHandle
      {
         self.create_image(width, height, ImageKind::Rgba, data, row_bytes, mipmapped, IMAGE_STORE_RGBA_FORMAT).unwrap_or(api::ImageHandle(0))
      }

      fn image_create_rgba8_empty(&mut self, width: u32, height: u32) -> api::ImageHandle
      {
         let clear = vec![0_u8; width as usize * height as usize * 4];
         self.create_image(width, height, ImageKind::Rgba, &clear, width as usize * 4, false, IMAGE_STORE_RGBA_FORMAT).unwrap_or(api::ImageHandle(0))
      }

      fn image_append_rgba8(&mut self, handle: api::ImageHandle, x: u32, y: u32, width: u32, height: u32, data: &[u8], row_bytes: usize)
      {
         let _ = self.update_image(handle, x, y, width, height, ImageKind::Rgba, data, row_bytes);
      }

      fn image_release(&mut self, handle: api::ImageHandle)
      {
         if handle.0 != 0
         {
            if let Some(slot) = self.images.get_mut(handle.0 as usize)
            {
               let _ = slot.take();
            }
         }
      }
   }

   fn create_programs(device: &wgpu::Device) -> Programs
   {
      let viewport_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
         label: Some("oxide-wgpu-viewport-layout"),
         entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
               ty: wgpu::BufferBindingType::Uniform,
               has_dynamic_offset: false,
               min_binding_size: NonZeroU64::new(48),
            },
            count: None,
         }],
      });
      let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
         label: Some("oxide-wgpu-texture-layout"),
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
      let solid_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
         label: Some("oxide-wgpu-solid-pipeline-layout"),
         bind_group_layouts: &[&viewport_layout],
         push_constant_ranges: &[],
      });
      let texture_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
         label: Some("oxide-wgpu-texture-pipeline-layout"),
         bind_group_layouts: &[&viewport_layout, &texture_layout],
         push_constant_ranges: &[],
      });
      let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
         label: Some("oxide-shared-wgpu-ui-shader"),
         source: wgpu::ShaderSource::Wgsl(UI_WGSL.into()),
      });
      let solid = create_pipeline(device, &shader, &solid_layout, &[vertex_layout()], "vs_main", "fs_solid", "oxide-wgpu-solid", Some(wgpu::BlendState::ALPHA_BLENDING));
      let rgba = create_pipeline(device, &shader, &texture_pipeline_layout, &[vertex_layout()], "vs_main", "fs_rgba", "oxide-wgpu-rgba", Some(wgpu::BlendState::ALPHA_BLENDING));
      #[cfg(feature = "benchmark-timings")]
      let rgba_opaque = create_pipeline(device, &shader, &texture_pipeline_layout, &[vertex_layout()], "vs_main", "fs_rgba", "oxide-wgpu-rgba-opaque", None);
      let a8 = create_pipeline(device, &shader, &texture_pipeline_layout, &[vertex_layout()], "vs_main", "fs_a8", "oxide-wgpu-a8", Some(wgpu::BlendState::ALPHA_BLENDING));
      let sdf = create_pipeline(device, &shader, &texture_pipeline_layout, &[vertex_layout()], "vs_main", "fs_sdf", "oxide-wgpu-sdf", Some(wgpu::BlendState::ALPHA_BLENDING));
      let rrect = create_pipeline(device, &shader, &solid_layout, &[rrect_layout()], "vs_rrect", "fs_rrect", "oxide-wgpu-rrect", Some(wgpu::BlendState::ALPHA_BLENDING));
      let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
         label: Some("oxide-wgpu-linear-sampler"),
         address_mode_u: wgpu::AddressMode::ClampToEdge,
         address_mode_v: wgpu::AddressMode::ClampToEdge,
         address_mode_w: wgpu::AddressMode::ClampToEdge,
         mag_filter: wgpu::FilterMode::Linear,
         min_filter: wgpu::FilterMode::Linear,
         mipmap_filter: wgpu::FilterMode::Linear,
         ..Default::default()
      });
      Programs {
         solid, rgba,
         #[cfg(feature = "benchmark-timings")]
         rgba_opaque,
         a8, sdf, rrect, viewport_layout, texture_layout, sampler,
      }
   }

   fn create_pipeline(device: &wgpu::Device, shader: &wgpu::ShaderModule, layout: &wgpu::PipelineLayout, buffers: &[wgpu::VertexBufferLayout<'_>], vertex: &'static str, fragment: &'static str, label: &'static str, blend: Option<wgpu::BlendState>) -> wgpu::RenderPipeline
   {
      device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
         label: Some(label),
         layout: Some(layout),
         vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vertex),
            compilation_options: Default::default(),
            buffers,
         },
         primitive: wgpu::PrimitiveState::default(),
         depth_stencil: None,
         multisample: wgpu::MultisampleState::default(),
         fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
               format: OUTPUT_FORMAT,
               blend,
               write_mask: wgpu::ColorWrites::ALL,
            })],
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
         wgpu::VertexAttribute { format: wgpu::VertexFormat::Unorm8x4, offset: 16, shader_location: 2 },
      ];
      wgpu::VertexBufferLayout {
         array_stride: core::mem::size_of::<PackedVertex>() as u64,
         step_mode: wgpu::VertexStepMode::Vertex,
         attributes: &ATTRIBUTES,
      }
   }

   fn rrect_layout() -> wgpu::VertexBufferLayout<'static>
   {
      const ATTRIBUTES: [wgpu::VertexAttribute; 3] = [
         wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 0, shader_location: 0 },
         wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 16, shader_location: 1 },
         wgpu::VertexAttribute { format: wgpu::VertexFormat::Unorm8x4, offset: 32, shader_location: 2 },
      ];
      wgpu::VertexBufferLayout {
         array_stride: core::mem::size_of::<RRectInstance>() as u64,
         step_mode: wgpu::VertexStepMode::Instance,
         attributes: &ATTRIBUTES,
      }
   }

   fn create_target(device: &wgpu::Device, width: u32, height: u32) -> (wgpu::Texture, wgpu::TextureView)
   {
      let texture = device.create_texture(&wgpu::TextureDescriptor {
         label: Some("oxide-headless-wgpu-rgba-target"),
         size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
         mip_level_count: 1,
         sample_count: 1,
         dimension: wgpu::TextureDimension::D2,
         format: OUTPUT_FORMAT,
         usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING,
         view_formats: &[],
      });
      let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
      (texture, view)
   }

   fn create_buffer(device: &wgpu::Device, label: &'static str, size: u64, usage: wgpu::BufferUsages) -> wgpu::Buffer
   {
      device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage, mapped_at_creation: false })
   }

   fn ensure_buffer(device: &wgpu::Device, buffer: &mut wgpu::Buffer, capacity: &mut u64, needed: u64, label: &'static str, usage: wgpu::BufferUsages)
   {
      if needed <= *capacity
      {
         return;
      }
      *capacity = needed.next_power_of_two().max(1024);
      *buffer = create_buffer(device, label, *capacity, usage);
   }

   fn image_batch(kind: ImageKind, handle: api::ImageHandle) -> BatchKind
   {
      match kind
      {
         ImageKind::Rgba => BatchKind::Rgba(handle),
         ImageKind::A8 => BatchKind::A8(handle),
      }
   }

   fn validate_dimensions(width: u32, height: u32, scale: f32) -> Result<(), api::RenderError>
   {
      if width == 0 || height == 0 || !scale.is_finite() || scale <= 0.0
      {
         return Err(api::RenderError::InvalidOperation("invalid headless target dimensions"));
      }
      Ok(())
   }

   fn validate_image_rows(width: u32, height: u32, data: &[u8], row_bytes: usize, bytes_per_pixel: usize) -> Result<(), api::RenderError>
   {
      let packed = width as usize * bytes_per_pixel;
      let required = row_bytes
         .checked_mul(height.saturating_sub(1) as usize)
         .and_then(|prefix| prefix.checked_add(packed))
         .ok_or(api::RenderError::InvalidOperation("image byte size overflow"))?;
      if width == 0 || height == 0 || row_bytes < packed || data.len() < required
      {
         return Err(api::RenderError::InvalidOperation("invalid image rows"));
      }
      Ok(())
   }

   fn padded_bytes_per_row(width: u32) -> Result<u32, api::RenderError>
   {
      let row = width.checked_mul(4).ok_or(api::RenderError::InvalidOperation("readback row size overflow"))?;
      let padding = COPY_ALIGNMENT - 1;
      row.checked_add(padding)
         .map(|value| value / COPY_ALIGNMENT * COPY_ALIGNMENT)
         .ok_or(api::RenderError::InvalidOperation("readback row alignment overflow"))
   }

   fn unpack_padded_rows(data: &[u8], width: u32, height: u32, padded_row_bytes: u32) -> Result<Vec<u8>, api::RenderError>
   {
      let row_bytes = width.checked_mul(4).ok_or(api::RenderError::InvalidOperation("readback row size overflow"))? as usize;
      let padded = padded_row_bytes as usize;
      let required = padded.checked_mul(height as usize).ok_or(api::RenderError::InvalidOperation("readback size overflow"))?;
      if data.len() < required || padded < row_bytes
      {
         return Err(api::RenderError::InvalidOperation("invalid padded readback"));
      }
      let mut pixels = Vec::with_capacity(row_bytes.saturating_mul(height as usize));
      for row in data[..required].chunks_exact(padded)
      {
         pixels.extend_from_slice(&row[..row_bytes]);
      }
      Ok(pixels)
   }

   fn physical_clip(rect: api::RectI, scale: f32, width: u32, height: u32) -> PhysicalClip
   {
      let x0 = (rect.x as f32 * scale).floor().max(0.0).min(width as f32) as u32;
      let y0 = (rect.y as f32 * scale).floor().max(0.0).min(height as f32) as u32;
      let x1 = (rect.x.saturating_add(rect.w) as f32 * scale).ceil().max(0.0).min(width as f32) as u32;
      let y1 = (rect.y.saturating_add(rect.h) as f32 * scale).ceil().max(0.0).min(height as f32) as u32;
      PhysicalClip { x: x0, y: y0, width: x1.saturating_sub(x0), height: y1.saturating_sub(y0) }
   }

   fn vertex_span(list: &api::DrawList, span: api::VertexSpan) -> Option<&[api::Vertex]>
   {
      let start = span.offset as usize;
      list.vertices.get(start..start.checked_add(span.len as usize)?)
   }

   fn index_span_or_implicit(list: &api::DrawList, span: api::IndexSpan, vertex_count: usize) -> Option<Vec<u16>>
   {
      if span.len > 0
      {
         let start = span.offset as usize;
         return list.indices.get(start..start.checked_add(span.len as usize)?).map(<[u16]>::to_vec);
      }
      if vertex_count == 4
      {
         return Some(vec![0, 1, 2, 2, 1, 3]);
      }
      let count = u16::try_from(vertex_count).ok()?;
      Some((0..count).collect())
   }

   fn append_vertices(
      out_vertices: &mut Vec<PackedVertex>,
      out_indices: &mut Vec<u32>,
      vertices: &[api::Vertex],
      indices: &[u16],
      source_offset: u32,
      color: api::Color,
      preserve_vertex_color: bool,
   )
   {
      let base = out_vertices.len() as u32;
      out_vertices.extend(vertices.iter().map(|vertex| PackedVertex {
         position: [vertex.x, vertex.y],
         uv: [vertex.u, vertex.v],
         rgba: if preserve_vertex_color && vertex.rgba != 0 {
            vertex.rgba
         } else {
            color.pack_rgba8()
         },
      }));
      for index in indices
      {
         let absolute = u32::from(*index);
         let local = if absolute < vertices.len() as u32 { absolute } else { absolute.saturating_sub(source_offset) };
         if local < vertices.len() as u32
         {
            out_indices.push(base + local);
         }
      }
   }

   fn append_quad(vertices: &mut Vec<PackedVertex>, indices: &mut Vec<u32>, rect: api::RectF, uv: [f32; 4], rgba: u32)
   {
      let base = vertices.len() as u32;
      vertices.extend_from_slice(&[
         PackedVertex { position: [rect.x, rect.y], uv: [uv[0], uv[1]], rgba },
         PackedVertex { position: [rect.x + rect.w, rect.y], uv: [uv[2], uv[1]], rgba },
         PackedVertex { position: [rect.x, rect.y + rect.h], uv: [uv[0], uv[3]], rgba },
         PackedVertex { position: [rect.x + rect.w, rect.y + rect.h], uv: [uv[2], uv[3]], rgba },
      ]);
      indices.extend_from_slice(&[base, base + 1, base + 2, base + 2, base + 1, base + 3]);
   }

   fn normalized_source(src: api::RectF, width: u32, height: u32) -> (f32, f32, f32, f32)
   {
      let (x, y, w, h) = if src.w > 0.0 && src.h > 0.0 {
         (src.x.clamp(0.0, width as f32), src.y.clamp(0.0, height as f32), src.w.clamp(0.0, width as f32), src.h.clamp(0.0, height as f32))
      } else {
         (0.0, 0.0, width as f32, height as f32)
      };
      (x / width as f32, y / height as f32, (x + w) / width as f32, (y + h) / height as f32)
   }

   #[cfg(test)]
   mod tests
   {
      use oxide_image_store::ImageResidencyBackend;
      use oxide_renderer_api as api;
      use oxide_renderer_api::Renderer;

      use super::{
         append_vertices, index_span_or_implicit, padded_bytes_per_row, unpack_padded_rows,
         validate_adapter_policy, HeadlessAdapterPolicy, HeadlessWgpuRenderer, PackedVertex,
         UnsupportedHeadlessWgpuCapability, COPY_ALIGNMENT, OpaqueRgbaCounters,
      };

      fn cpu_renderer(width: u32, height: u32) -> Option<HeadlessWgpuRenderer>
      {
         let renderer = HeadlessWgpuRenderer::new(width, height, 1.0).ok();
         if std::env::var_os("OXIDE_REQUIRE_CPU_RENDERER").is_some() && renderer.is_none()
         {
            panic!("required CPU Vulkan renderer is unavailable");
         }
         renderer
      }

      fn solid_list(width: f32, height: f32, color: api::Color) -> api::DrawList
      {
         api::DrawList {
            items: vec![api::DrawCmd::Solid {
               vb: api::VertexSpan { offset: 0, len: 4 },
               ib: api::IndexSpan { offset: 0, len: 6 },
               color,
            }],
            vertices: vec![
               api::Vertex { x: 0.0, y: 0.0, u: 0.0, v: 0.0, rgba: 0 },
               api::Vertex { x: width, y: 0.0, u: 0.0, v: 0.0, rgba: 0 },
               api::Vertex { x: 0.0, y: height, u: 0.0, v: 0.0, rgba: 0 },
               api::Vertex { x: width, y: height, u: 0.0, v: 0.0, rgba: 0 },
            ],
            indices: vec![0, 1, 2, 2, 1, 3],
         }
      }

      fn render(renderer: &mut HeadlessWgpuRenderer, list: &api::DrawList) -> Vec<u8>
      {
         let token = renderer.begin_frame(&api::FrameTarget, None);
         renderer.encode_pass(list);
         renderer.submit(token).unwrap();
         renderer.read_rgba().unwrap().pixels
      }

      fn image_list(image: api::ImageHandle) -> api::DrawList
      {
         api::DrawList {
            items: vec![api::DrawCmd::Image {
               tex: image,
               dst: api::RectF::new(0.0, 0.0, 2.0, 2.0),
               src: api::RectF::new(0.0, 0.0, 1.0, 1.0),
               alpha: 1.0,
            }],
            ..Default::default()
         }
      }

      fn image_list_with_alpha(image: api::ImageHandle, alpha: f32) -> api::DrawList
      {
         api::DrawList {
            items: vec![api::DrawCmd::Image {
               tex: image,
               dst: api::RectF::new(0.0, 0.0, 2.0, 2.0),
               src: api::RectF::new(0.0, 0.0, 1.0, 1.0),
               alpha,
            }],
            ..Default::default()
         }
      }

      fn image_list_with_later_overlay(image: api::ImageHandle) -> api::DrawList
      {
         let mut list = image_list(image);
         list.items.push(api::DrawCmd::Solid {
            vb: api::VertexSpan { offset: 0, len: 4 },
            ib: api::IndexSpan { offset: 0, len: 6 },
            color: api::Color::rgba(0.25, 0.5, 0.75, 0.5),
         });
         list.vertices = solid_list(2.0, 2.0, api::Color::rgba(0.0, 0.0, 0.0, 0.0)).vertices;
         list.indices = vec![0, 1, 2, 2, 1, 3];
         list
      }

      fn image_mesh_list(image: api::ImageHandle) -> api::DrawList
      {
         api::DrawList {
            items: vec![api::DrawCmd::ImageMesh {
               tex: image,
               vb: api::VertexSpan { offset: 0, len: 4 },
               ib: api::IndexSpan { offset: 0, len: 6 },
               alpha: 1.0,
            }],
            vertices: vec![
               api::Vertex { x: 0.0, y: 0.0, u: 0.0, v: 0.0, rgba: 0 },
               api::Vertex { x: 2.0, y: 0.0, u: 1.0, v: 0.0, rgba: 0 },
               api::Vertex { x: 0.0, y: 2.0, u: 0.0, v: 1.0, rgba: 0 },
               api::Vertex { x: 2.0, y: 2.0, u: 1.0, v: 1.0, rgba: 0 },
            ],
            indices: vec![0, 1, 2, 2, 1, 3],
         }
      }

      fn nine_slice_list(image: api::ImageHandle) -> api::DrawList
      {
         api::DrawList {
            items: vec![api::DrawCmd::NineSlice {
               tex: image,
               rect: api::RectF::new(0.0, 0.0, 2.0, 2.0),
               slice: api::Insets::new(0.25, 0.25, 0.25, 0.25),
               alpha: 1.0,
            }],
            ..Default::default()
         }
      }

      #[test]
      fn readback_rows_cover_alignment_boundaries()
      {
         assert_eq!(padded_bytes_per_row(1).unwrap(), COPY_ALIGNMENT);
         assert_eq!(padded_bytes_per_row(63).unwrap(), COPY_ALIGNMENT);
         assert_eq!(padded_bytes_per_row(64).unwrap(), COPY_ALIGNMENT);
         assert_eq!(padded_bytes_per_row(65).unwrap(), COPY_ALIGNMENT * 2);
      }

      #[test]
      fn padded_rows_preserve_rgba_channel_order()
      {
         let mut padded = vec![0xaa; COPY_ALIGNMENT as usize * 2];
         padded[..8].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
         let second = COPY_ALIGNMENT as usize;
         padded[second..second + 8].copy_from_slice(&[9, 10, 11, 12, 13, 14, 15, 16]);
         assert_eq!(
            unpack_padded_rows(&padded, 2, 2, COPY_ALIGNMENT).unwrap(),
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
         );
      }

      #[test]
      fn nonindexed_foundation_solids_preserve_triangle_lists_and_quads()
      {
         let list = api::DrawList::default();
         let empty = api::IndexSpan { offset: 0, len: 0 };
         assert_eq!(index_span_or_implicit(&list, empty, 3), Some(vec![0, 1, 2]));
         assert_eq!(
            index_span_or_implicit(&list, empty, 4),
            Some(vec![0, 1, 2, 2, 1, 3]),
         );
         assert_eq!(
            index_span_or_implicit(&list, empty, 6),
            Some(vec![0, 1, 2, 3, 4, 5]),
         );
      }

      #[test]
      fn glyph_lowering_uses_the_run_color_instead_of_source_vertex_white()
      {
         let vertices = [
            api::Vertex { x: 0.0, y: 0.0, u: 0.0, v: 0.0, rgba: 0xffff_ffff },
            api::Vertex { x: 1.0, y: 0.0, u: 1.0, v: 0.0, rgba: 0xffff_ffff },
            api::Vertex { x: 0.0, y: 1.0, u: 0.0, v: 1.0, rgba: 0xffff_ffff },
            api::Vertex { x: 1.0, y: 1.0, u: 1.0, v: 1.0, rgba: 0xffff_ffff },
         ];
         let indices = [0_u16, 1, 2, 2, 1, 3];
         let black = api::Color::rgba(0.0, 0.0, 0.0, 1.0);
         let mut lowered = Vec::<PackedVertex>::new();
         let mut lowered_indices = Vec::new();
         append_vertices(
            &mut lowered,
            &mut lowered_indices,
            &vertices,
            &indices,
            0,
            black,
            false,
         );
         assert_eq!(lowered_indices, indices.map(u32::from));
         assert!(lowered.iter().all(|vertex| vertex.rgba == black.pack_rgba8()));
      }

      #[test]
      fn adapter_policy_rejects_cpu_integrated_and_identity_mismatches()
      {
         let gpu = HeadlessAdapterPolicy::DiscreteGpu { vendor: 0x10de, device: 0x2bb1 };
         assert!(validate_adapter_policy(HeadlessAdapterPolicy::Cpu, wgpu::Backend::Vulkan, wgpu::DeviceType::Cpu, 0, 0).is_ok());
         assert!(validate_adapter_policy(gpu, wgpu::Backend::Vulkan, wgpu::DeviceType::DiscreteGpu, 0x10de, 0x2bb1).is_ok());
         assert!(validate_adapter_policy(HeadlessAdapterPolicy::Cpu, wgpu::Backend::Vulkan, wgpu::DeviceType::DiscreteGpu, 0x10de, 0x2bb1).is_err());
         assert!(validate_adapter_policy(HeadlessAdapterPolicy::Cpu, wgpu::Backend::Gl, wgpu::DeviceType::Cpu, 0, 0).is_err());
         assert!(validate_adapter_policy(gpu, wgpu::Backend::Gl, wgpu::DeviceType::DiscreteGpu, 0x10de, 0x2bb1).is_err());
         assert!(validate_adapter_policy(gpu, wgpu::Backend::Vulkan, wgpu::DeviceType::Cpu, 0x10005, 0).is_err());
         assert!(validate_adapter_policy(gpu, wgpu::Backend::Vulkan, wgpu::DeviceType::IntegratedGpu, 0x10de, 0x2bb1).is_err());
         assert!(validate_adapter_policy(gpu, wgpu::Backend::Vulkan, wgpu::DeviceType::DiscreteGpu, 0x1002, 0x2bb1).is_err());
         assert!(validate_adapter_policy(gpu, wgpu::Backend::Vulkan, wgpu::DeviceType::DiscreteGpu, 0x10de, 0x2bb2).is_err());
      }

      #[test]
      fn cpu_adapter_receipt_is_fail_closed_and_records_icd_selection()
      {
         let Some(renderer) = cpu_renderer(1, 1) else
         {
            return;
         };
         let receipt = renderer.adapter_receipt();
         assert_eq!(receipt.backend, "Vulkan");
         assert_eq!(receipt.device_type, "Cpu");
         assert_eq!(
            receipt.vk_driver_files,
            std::env::var_os("VK_DRIVER_FILES").map(|value| value.to_string_lossy().into_owned()),
         );
         assert_eq!(receipt.output_format, "Rgba8Unorm");
         assert_eq!(receipt.output_transfer, "linear-unorm");
         assert_eq!(renderer.output_format(), wgpu::TextureFormat::Rgba8Unorm);
      }

      #[test]
      fn offscreen_readback_preserves_rgba_channels()
      {
         let Some(mut renderer) = cpu_renderer(4, 4) else
         {
            return;
         };
         let pixels = render(&mut renderer, &solid_list(4.0, 4.0, api::Color::rgba(1.0, 0.5, 0.25, 1.0)));
         for pixel in pixels.chunks_exact(4)
         {
            assert_eq!(pixel, [255, 128, 64, 255]);
         }
      }

      #[test]
      fn rgba8_unorm_target_preserves_linear_solid_midtone()
      {
         let Some(mut renderer) = cpu_renderer(2, 2) else
         {
            return;
         };
         let pixels = render(&mut renderer, &solid_list(2.0, 2.0, api::Color::rgba(0.5, 0.5, 0.5, 1.0)));
         for pixel in pixels.chunks_exact(4)
         {
            assert_eq!(pixel[0], 128);
            assert_eq!(pixel[0], pixel[1]);
            assert_eq!(pixel[1], pixel[2]);
            assert_eq!(pixel[3], 255);
         }
      }

      #[test]
      fn direct_rgba8_upload_preserves_browser_unorm_texels()
      {
         let Some(mut renderer) = cpu_renderer(2, 2) else
         {
            return;
         };
         let image = renderer.try_image_create_rgba8(1, 1, &[128, 64, 32, 255], 4).unwrap();
         let pixels = render(&mut renderer, &image_list(image));
         for pixel in pixels.chunks_exact(4)
         {
            assert_eq!(pixel, [128, 64, 32, 255]);
         }
      }

      #[test]
      #[cfg(feature = "benchmark-timings")]
      fn known_opaque_rgba_uses_replace_without_changing_later_source_over()
      {
         let Some(mut blended) = cpu_renderer(2, 2) else
         {
            return;
         };
         let Some(mut opaque) = cpu_renderer(2, 2) else
         {
            return;
         };
         let texels = [64, 128, 192, 255];
         let blended_image = blended.try_image_create_rgba8(1, 1, &texels, 4).unwrap();
         let opaque_image = opaque.try_image_create_known_opaque_rgba8(1, 1, &texels, 4).unwrap();
         let expected = render(&mut blended, &image_list_with_later_overlay(blended_image));
         let actual = render(&mut opaque, &image_list_with_later_overlay(opaque_image));
         assert_eq!(actual, expected);
         assert_eq!(blended.opaque_rgba_counters(), OpaqueRgbaCounters::default());
         assert_eq!(opaque.opaque_rgba_counters(), OpaqueRgbaCounters { eligible_batches: 1, eligible_clipped_pixels: 4 });
      }

      #[test]
      #[cfg(feature = "benchmark-timings")]
      fn partial_modulation_keeps_known_opaque_rgba_on_blended_pipeline()
      {
         let Some(mut renderer) = cpu_renderer(2, 2) else
         {
            return;
         };
         let image = renderer.try_image_create_known_opaque_rgba8(1, 1, &[64, 128, 192, 255], 4).unwrap();
         let _ = render(&mut renderer, &image_list_with_alpha(image, 0.5));
         assert_eq!(renderer.opaque_rgba_counters(), OpaqueRgbaCounters::default());
      }

      #[test]
      #[cfg(feature = "benchmark-timings")]
      fn zero_modulation_keeps_known_opaque_rgba_on_blended_pipeline()
      {
         let Some(mut renderer) = cpu_renderer(2, 2) else
         {
            return;
         };
         let image = renderer.try_image_create_known_opaque_rgba8(1, 1, &[64, 128, 192, 255], 4).unwrap();
         let pixels = render(&mut renderer, &image_list_with_alpha(image, 0.0));
         assert_eq!(pixels, vec![0; 16]);
         assert_eq!(renderer.opaque_rgba_counters(), OpaqueRgbaCounters::default());
      }

      #[test]
      #[cfg(feature = "benchmark-timings")]
      fn rgba_upload_after_encode_revokes_known_opaque_before_pipeline_selection()
      {
         let Some(mut expected) = cpu_renderer(2, 2) else
         {
            return;
         };
         let Some(mut actual) = cpu_renderer(2, 2) else
         {
            return;
         };
         let initial = [64, 128, 192, 255];
         let replacement = [9, 30, 70, 96];
         let expected_image = expected.try_image_create_rgba8(1, 1, &initial, 4).unwrap();
         expected.image_append_rgba8(expected_image, 0, 0, 1, 1, &replacement, 4);
         let expected_pixels = render(&mut expected, &image_list(expected_image));

         let actual_image = actual.try_image_create_known_opaque_rgba8(1, 1, &initial, 4).unwrap();
         let token = actual.begin_frame(&api::FrameTarget, None);
         actual.encode_pass(&image_list(actual_image));
         actual.image_append_rgba8(actual_image, 0, 0, 1, 1, &replacement, 4);
         actual.submit(token).unwrap();
         let actual_pixels = actual.read_rgba().unwrap().pixels;

         assert_eq!(actual_pixels, expected_pixels);
         assert_eq!(actual.opaque_rgba_counters(), OpaqueRgbaCounters { eligible_batches: 1, eligible_clipped_pixels: 4 });
      }

      #[test]
      #[cfg(feature = "benchmark-timings")]
      fn benchmark_opaque_rgba_control_uses_blended_pipeline_but_keeps_eligibility_attribution()
      {
         let Some(mut renderer) = cpu_renderer(2, 2) else
         {
            return;
         };
         renderer.set_benchmark_opaque_rgba_enabled(false);
         let image = renderer.try_image_create_known_opaque_rgba8(1, 1, &[64, 128, 192, 255], 4).unwrap();
         let _ = render(&mut renderer, &image_list(image));
         assert_eq!(renderer.opaque_rgba_counters(), OpaqueRgbaCounters { eligible_batches: 1, eligible_clipped_pixels: 4 });
      }

      #[test]
      #[cfg(feature = "benchmark-timings")]
      fn known_opaque_image_mesh_and_nine_slice_are_eligible()
      {
         let Some(mut renderer) = cpu_renderer(2, 2) else
         {
            return;
         };
         let texels = [64, 128, 192, 255];
         let mesh = renderer.try_image_create_known_opaque_rgba8(1, 1, &texels, 4).unwrap();
         let mesh_pixels = render(&mut renderer, &image_mesh_list(mesh));
         assert!(mesh_pixels.chunks_exact(4).all(|pixel| pixel == texels));
         assert_eq!(renderer.opaque_rgba_counters(), OpaqueRgbaCounters { eligible_batches: 1, eligible_clipped_pixels: 4 });

         let nine = renderer.try_image_create_known_opaque_rgba8(1, 1, &texels, 4).unwrap();
         let nine_pixels = render(&mut renderer, &nine_slice_list(nine));
         assert!(nine_pixels.chunks_exact(4).all(|pixel| pixel == texels));
         assert_eq!(renderer.opaque_rgba_counters(), OpaqueRgbaCounters { eligible_batches: 1, eligible_clipped_pixels: 4 });
      }

      #[test]
      fn image_store_rgba8_mips_remain_srgb_before_unorm_output()
      {
         let Some(mut renderer) = cpu_renderer(2, 2) else
         {
            return;
         };
         let image = renderer.image_create_rgba8(1, 1, &[128, 64, 32, 255], 4, true);
         let pixels = render(&mut renderer, &image_list(image));
         for pixel in pixels.chunks_exact(4)
         {
            assert_eq!(pixel, [55, 13, 4, 255]);
         }
      }

      #[test]
      fn image_store_empty_rgba8_remains_srgb_after_append()
      {
         let Some(mut renderer) = cpu_renderer(2, 2) else
         {
            return;
         };
         let image = renderer.image_create_rgba8_empty(1, 1);
         renderer.image_append_rgba8(image, 0, 0, 1, 1, &[128, 64, 32, 255], 4);
         let pixels = render(&mut renderer, &image_list(image));
         for pixel in pixels.chunks_exact(4)
         {
            assert_eq!(pixel, [55, 13, 4, 255]);
         }
      }

      #[test]
      fn repeated_cpu_frames_are_byte_deterministic()
      {
         let Some(mut renderer) = cpu_renderer(65, 3) else
         {
            return;
         };
         let list = solid_list(65.0, 3.0, api::Color::rgba(0.25, 0.5, 0.75, 1.0));
         let first = render(&mut renderer, &list);
         let second = render(&mut renderer, &list);
         assert_eq!(first, second);
      }

      #[test]
      fn unsupported_effects_fail_the_compatibility_gate()
      {
         let list = api::DrawList {
            items: vec![api::DrawCmd::Backdrop {
               rect: api::RectF::new(0.0, 0.0, 10.0, 10.0),
               sigma: 4.0,
               tint: api::Color::rgba(0.0, 0.0, 0.0, 0.5),
               alpha: 1.0,
            }],
            ..Default::default()
         };
         assert!(HeadlessWgpuRenderer::validate_draw_list_compatibility(&list).is_err());
         assert!(!HeadlessWgpuRenderer::compatibility().full_draw_list);
         assert!(!HeadlessWgpuRenderer::compatibility().backdrop_effects);
      }

      #[test]
      fn compatibility_report_names_every_residual_draw_command()
      {
         let list = api::DrawList {
            items: vec![
               api::DrawCmd::LayerBegin {
                  id: 7,
                  rect: api::RectF::new(0.0, 0.0, 10.0, 10.0),
                  dirty: true,
               },
               api::DrawCmd::LayerEnd,
               api::DrawCmd::Backdrop {
                  rect: api::RectF::new(0.0, 0.0, 10.0, 10.0),
                  sigma: 4.0,
                  tint: api::Color::rgba(0.0, 0.0, 0.0, 0.5),
                  alpha: 1.0,
               },
               api::DrawCmd::VisualEffect {
                  rect: api::RectF::new(0.0, 0.0, 10.0, 10.0),
                  effect: api::VisualEffect::UIKitDark,
               },
               api::DrawCmd::CameraBg {
                  rect: api::RectF::new(0.0, 0.0, 10.0, 10.0),
                  tint: api::Color::rgba(0.0, 0.0, 0.0, 0.0),
                  alpha: 1.0,
                  grayscale: false,
                  blur: false,
                  sigma: 0.0,
               },
            ],
            ..Default::default()
         };
         let report = HeadlessWgpuRenderer::draw_list_compatibility_report(&list);
         assert_eq!(report.layer_begin_commands, 1);
         assert_eq!(report.layer_end_commands, 1);
         assert_eq!(report.backdrop_commands, 1);
         assert_eq!(report.visual_effect_commands, 1);
         assert_eq!(report.camera_noop_commands, 1);
         assert!(!report.is_compatible());
         assert!(!HeadlessWgpuRenderer::compatibility().full_draw_list);
         assert_eq!(
            HeadlessWgpuRenderer::residual_unsupported_capabilities(),
            &[
               UnsupportedHeadlessWgpuCapability::LayerCachingAndComposition,
               UnsupportedHeadlessWgpuCapability::BackdropAndVisualEffects,
               UnsupportedHeadlessWgpuCapability::CameraFrameComposition,
               UnsupportedHeadlessWgpuCapability::Scene3d,
               UnsupportedHeadlessWgpuCapability::IdMask,
            ],
         );
      }

      #[test]
      fn camera_command_is_an_explicit_compatible_noop()
      {
         let list = api::DrawList {
            items: vec![api::DrawCmd::CameraBg {
               rect: api::RectF::new(0.0, 0.0, 10.0, 10.0),
               tint: api::Color::rgba(0.0, 0.0, 0.0, 0.0),
               alpha: 1.0,
               grayscale: false,
               blur: false,
               sigma: 0.0,
            }],
            ..Default::default()
         };
         let report = HeadlessWgpuRenderer::draw_list_compatibility_report(&list);
         assert!(report.is_compatible());
         assert_eq!(report.camera_noop_commands, 1);
         assert!(HeadlessWgpuRenderer::validate_draw_list_compatibility(&list).is_ok());
         assert!(HeadlessWgpuRenderer::compatibility().camera_draw_command_noop);
         assert!(!HeadlessWgpuRenderer::compatibility().camera);
      }

      #[test]
      fn browser_only_operation_families_fail_before_native_encoding()
      {
         for capability in [
            super::HeadlessWgpuCapability::LayerCachingAndComposition,
            super::HeadlessWgpuCapability::BackdropAndVisualEffects,
            super::HeadlessWgpuCapability::CameraFrameComposition,
            super::HeadlessWgpuCapability::Scene3d,
            super::HeadlessWgpuCapability::IdMask,
         ]
         {
            assert!(matches!(
               HeadlessWgpuRenderer::require_capability(capability),
               Err(api::RenderError::Unsupported(_)),
            ));
         }
      }
   }
}

#[cfg(feature = "headless-vulkan")]
pub use headless::{
   CpuAdapterLimits, CpuAdapterReceipt, DrawListCompatibilityReport, HeadlessAdapterPolicy,
   HeadlessWgpuCompatibility, HeadlessWgpuCapability, HeadlessWgpuRenderer, OpaqueRgbaCounters, RgbaFrame,
   UnsupportedHeadlessWgpuCapability,
};

#[cfg(test)]
mod parity_tests
{
   use super::image::{copy_rgba_rows, rgba8_srgb_mip_chain};
   use super::parity::{
      compare_rgba, BrowserNativeRgbaFixture, PixelTolerance, RgbaImage, RgbaParityError,
   };

   #[test]
   fn exact_fixture_compares_tight_browser_and_padded_native_rows()
   {
      let browser_pixels = [1, 2, 3, 4, 5, 6, 7, 8];
      let native_pixels = [1, 2, 3, 4, 5, 6, 7, 8, 99, 99, 99, 99];
      let fixture = BrowserNativeRgbaFixture {
         id: "padded",
         width: 2,
         height: 1,
         tolerance: PixelTolerance::EXACT,
         browser: RgbaImage {
            width: 2,
            height: 1,
            row_bytes: 8,
            pixels: &browser_pixels,
         },
      };
      let difference = fixture.compare_native(RgbaImage {
         width: 2,
         height: 1,
         row_bytes: 12,
         pixels: &native_pixels,
      }).expect("padding is not image content");
      assert_eq!(difference.differing_pixels, 0);
   }

   #[test]
   fn antialiasing_tolerance_accepts_bounded_channel_differences()
   {
      let reference = [0_u8; 256 * 4];
      let mut candidate = reference;
      candidate[0] = 1;
      candidate[7] = 2;
      candidate[14] = 3;
      let difference = compare_rgba(
         RgbaImage { width: 256, height: 1, row_bytes: 1024, pixels: &reference },
         RgbaImage { width: 256, height: 1, row_bytes: 1024, pixels: &candidate },
         PixelTolerance::ANTIALIASED,
      ).expect("bounded antialiasing differences");
      assert_eq!(difference.differing_pixels, 3);
      assert_eq!(difference.max_channel_error, 3);
   }

   #[test]
   fn exact_tolerance_returns_measured_failure()
   {
      let reference = [0_u8; 4];
      let candidate = [0, 1, 0, 0];
      let error = compare_rgba(
         RgbaImage { width: 1, height: 1, row_bytes: 4, pixels: &reference },
         RgbaImage { width: 1, height: 1, row_bytes: 4, pixels: &candidate },
         PixelTolerance::EXACT,
      ).expect_err("one changed channel must fail exact parity");
      let RgbaParityError::ToleranceExceeded(difference) = error else
      {
         panic!("unexpected parity error: {error}");
      };
      assert_eq!(difference.differing_pixels, 1);
      assert_eq!(difference.max_channel_error, 1);
      assert_eq!(difference.mean_squared_error, 0.25);
   }

   #[test]
   fn shared_image_ingest_preserves_top_left_row_order_and_discards_stride_padding()
   {
      let source = [
         1, 2, 3, 4, 5, 6, 7, 8, 99, 99,
         9, 10, 11, 12, 13, 14, 15, 16, 88, 88,
      ];
      assert_eq!(
         copy_rgba_rows(2, 2, &source, 10),
         Some(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]),
      );
   }

   #[test]
   fn shared_srgb_mips_average_in_linear_light()
   {
      let levels = rgba8_srgb_mip_chain(
         2,
         2,
         vec![0, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255],
      );
      assert_eq!(levels.len(), 2);
      assert_eq!(levels[1].rgba, vec![188, 188, 188, 255]);
   }
}
