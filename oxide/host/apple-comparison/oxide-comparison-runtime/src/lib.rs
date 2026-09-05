//! Comparison-only Metal runtime for the Apple production benchmark applications.

mod package;

use oxide_benchmark_spec::{InlineTextAtlas, MacOsComparatorScaleOverlay, TraceEvent};
use oxide_renderer_api as gfx;
use oxide_renderer_api::Renderer;
use oxide_renderer_metal as metal;
use oxide_test_scenes as scenes;
use oxide_text as text;
use oxide_timing as timing;
use oxide_ui_core as ui;
use std::{
   path::Path,
   sync::{Mutex, MutexGuard, OnceLock},
};

struct MetalUploader
{
   renderer: *mut metal::MetalRenderer,
}

unsafe impl Send for MetalUploader {}
unsafe impl Sync for MetalUploader {}

impl ui::elements::ImageUploader for MetalUploader
{
   fn create_a8(&mut self, width: u32, height: u32, data: &[u8], row_bytes: usize) -> gfx::ImageHandle
   {
      unsafe {(*self.renderer).image_create_a8(width, height, data, row_bytes)}
   }

   fn update_a8(&mut self, handle: gfx::ImageHandle, x: u32, y: u32, width: u32, height: u32, data: &[u8], row_bytes: usize)
   {
      unsafe {(*self.renderer).image_update_a8(handle, x, y, width, height, data, row_bytes)}
   }

   fn append_a8(&mut self, handle: gfx::ImageHandle, x: u32, y: u32, width: u32, height: u32, data: &[u8], row_bytes: usize)
   {
      unsafe {(*self.renderer).image_append_a8(handle, x, y, width, height, data, row_bytes)}
   }

   fn release_a8(&mut self, handle: gfx::ImageHandle)
   {
      unsafe {(*self.renderer).image_release(handle)}
   }
}

struct ComparisonReset
{
   scenario: package::ComparisonScenario,
   fixture: Vec<u8>,
   scale_overlay: Option<MacOsComparatorScaleOverlay>,
   atlas: Option<gfx::ImageHandle>,
   accented_atlas: Option<gfx::ImageHandle>,
   inline_text: Option<(Vec<gfx::ImageHandle>, InlineTextAtlas)>,
   fonts: [usize; 3],
   image: Option<ComparisonImageRuntime>,
}

struct ComparisonImageRuntime
{
   source_png: Vec<u8>,
   source_sha256: String,
   source_width: u32,
   source_height: u32,
   source_bgra: Option<Vec<u8>>,
   source: Option<gfx::ImageHandle>,
   thumbnail: gfx::ImageHandle,
   thumbnail_sha256: String,
}

#[derive(Clone, Copy, Default)]
struct ComparisonEncodeDiagnostics
{
   frame_id: u64,
   render_prepare_begin_ticks: u64,
   render_prepare_end_ticks: u64,
   encode_begin_ticks: u64,
   encode_end_ticks: u64,
   command_submit_ticks: u64,
   encoded_bytes: u64,
   draw_calls: u64,
   damage_pixels: u64,
   damage_rects: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct OxideComparisonRendererDiagnostics
{
   pub schema_version: u32,
   pub reserved: u32,
   pub submitted_frame_id: u64,
   pub completed_frame_id: u64,
   pub render_prepare_begin_ticks: u64,
   pub render_prepare_end_ticks: u64,
   pub encode_begin_ticks: u64,
   pub encode_end_ticks: u64,
   pub command_submit_ticks: u64,
   pub encoded_bytes: u64,
   pub draw_calls: u64,
   pub damage_pixels: u64,
   pub damage_rects: u64,
   pub gpu_duration_ns: u64,
   pub gpu_render_duration_ns: u64,
}

#[derive(Clone, Copy)]
struct MacOsPointerState
{
   x: f32,
   y: f32,
   moved: bool,
}

#[derive(Default)]
struct Runtime
{
   renderer: Option<Box<metal::MetalRenderer>>,
   router: Option<scenes::Router<MetalUploader>>,
   builder: ui::DrawListBuilder,
   coalesced: Vec<gfx::DrawCmd>,
   pending_damage: Vec<gfx::RectI>,
   reusable_damage: Vec<gfx::RectI>,
   traces: Vec<(String, Vec<TraceEvent>)>,
   reset: Option<ComparisonReset>,
   pending_scale_overlay: Option<MacOsComparatorScaleOverlay>,
   snapshot_bgra: Vec<u8>,
   snapshot_png: Vec<u8>,
   snapshot_status: String,
   last_ms: u64,
   virtual_time_us: Option<u64>,
   launch_probe_generation: u64,
   interaction_generation: u64,
   macos_pointer: Option<MacOsPointerState>,
   launch_probe_response_visible: bool,
   renderer_diagnostics_enabled: bool,
   latest_encode_diagnostics: ComparisonEncodeDiagnostics,
   pending_render_prepare_begin_ticks: u64,
   pending_render_prepare_end_ticks: u64,
   last_viewport: Option<gfx::RectF>,
   last_scale: f32,
   prepared: bool,
   initialized: bool,
}

static RUNTIME: OnceLock<Mutex<Runtime>> = OnceLock::new();

fn runtime() -> &'static Mutex<Runtime>
{
   RUNTIME.get_or_init(|| Mutex::new(Runtime::default()))
}

fn lock_runtime() -> MutexGuard<'static, Runtime>
{
   runtime().lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn with_runtime<T>(operation: impl FnOnce(&mut Runtime) -> T) -> T
{
   operation(&mut lock_runtime())
}

fn mark_dirty(runtime: &mut Runtime)
{
   runtime.pending_damage.clear();
}

fn record_interaction(runtime: &mut Runtime, changed: bool) -> ::libc::c_int
{
   if !changed
   {
      return 0;
   }
   runtime.interaction_generation = runtime.interaction_generation.saturating_add(1);
   mark_dirty(runtime);
   1
}

#[no_mangle]
pub extern "C" fn oxide_comparison_init(width: u32, height: u32, scale: f32) -> ::libc::c_int
{
   let mut runtime = lock_runtime();
   if runtime.initialized
   {
      return 0;
   }
   let mut renderer = match metal::MetalRenderer::new_with_config(metal::MetalRendererConfig::visible_host())
   {
      Ok(renderer) => renderer,
      Err(error) =>
      {
         eprintln!("oxide.comparison: Metal initialization failed: {error}");
         return -1;
      }
   };
   if renderer.resize(width, height, scale).is_err()
   {
      return -2;
   }
   renderer.set_damage_options(true, 0.70, 0.25);
   let mut renderer = Box::new(renderer);
   let renderer_ptr = &mut *renderer as *mut metal::MetalRenderer;
   let mut router = scenes::Router::new(MetalUploader {renderer: renderer_ptr});
   router.damage_set_options(true, 0.70, 0.25);
   runtime.renderer = Some(renderer);
   runtime.router = Some(router);
   runtime.last_ms = timing::now_ms();
   runtime.snapshot_status.clear();
   runtime.initialized = true;
   runtime.prepared = false;
   0
}

#[no_mangle]
pub extern "C" fn oxide_comparison_shutdown()
{
   let mut runtime = lock_runtime();
   runtime.router = None;
   runtime.renderer = None;
   *runtime = Runtime::default();
}

#[no_mangle]
pub extern "C" fn oxide_comparison_teardown_scenario()
{
   let mut runtime = lock_runtime();
   if let Some(router) = runtime.router.as_mut()
   {
      router.text.trim_memory_with_uploader(&mut router.uploader);
   }
   let reset = runtime.reset.take();
   if let (Some(renderer), Some(reset)) = (runtime.renderer.as_deref_mut(), reset)
   {
      if let Some(atlas) = reset.atlas
      {
         renderer.image_release(atlas);
      }
      if let Some(atlas) = reset.accented_atlas
      {
         renderer.image_release(atlas);
      }
      if let Some((images, _)) = reset.inline_text
      {
         for image in images
         {
            renderer.image_release(image);
         }
      }
      if let Some(image) = reset.image
      {
         renderer.image_release(image.thumbnail);
         if let Some(source) = image.source
         {
            renderer.image_release(source);
         }
      }
   }
   if let Some(renderer) = runtime.renderer.as_deref_mut()
   {
      renderer.purge_prepared_chunks();
      renderer.purge_layer_cache();
      let renderer = renderer as *mut metal::MetalRenderer;
      runtime.router = Some(scenes::Router::new(MetalUploader {renderer}));
   }
   runtime.builder = ui::DrawListBuilder::new();
   runtime.coalesced = Vec::new();
   runtime.pending_damage = Vec::new();
   runtime.reusable_damage = Vec::new();
   runtime.traces = Vec::new();
   runtime.snapshot_status = String::new();
   runtime.pending_scale_overlay = None;
   runtime.virtual_time_us = None;
   runtime.launch_probe_generation = 0;
   runtime.macos_pointer = None;
   runtime.launch_probe_response_visible = false;
   runtime.prepared = false;
}

#[no_mangle]
pub extern "C" fn oxide_comparison_set_virtual_time_us(time_us: u64) -> ::libc::c_int
{
   with_runtime(|runtime| {
      let now_ms = time_us / 1_000;
      let delta_ms = runtime.virtual_time_us.map_or(0, |previous| {
         let delta_us = time_us.saturating_sub(previous).min(u64::from(u32::MAX) * 1_000);
         (delta_us / 1_000) as u32
      });
      let Some(router) = runtime.router.as_mut() else {return -1};
      router.update(now_ms, delta_ms);
      runtime.virtual_time_us = Some(time_us);
      runtime.last_ms = now_ms;
      0
   })
}

#[no_mangle]
pub extern "C" fn oxide_comparison_prepare_frame(width: u32, height: u32, scale: f32) -> ::libc::c_int
{
   let mut runtime = lock_runtime();
   if !runtime.initialized || runtime.prepared
   {
      return -1;
   }
   let diagnostics_enabled = runtime.renderer_diagnostics_enabled;
   let prepare_begin_ticks = if diagnostics_enabled {mach_continuous_time()} else {0};
   let Some(renderer) = runtime.renderer.as_deref_mut() else {return -2};
   if renderer.resize(width, height, scale).is_err()
   {
      return -3;
   }
   let now = runtime.virtual_time_us.map_or_else(timing::now_ms, |time| time / 1_000);
   let delta_ms = if runtime.virtual_time_us.is_some() {0} else {now.saturating_sub(runtime.last_ms) as u32};
   runtime.last_ms = now;
   let mut builder = core::mem::take(&mut runtime.builder);
   let mut damage = core::mem::take(&mut runtime.reusable_damage);
   builder.clear();
   let viewport = gfx::RectF::new(0.0, 0.0, width as f32 / scale.max(1.0), height as f32 / scale.max(1.0));
   runtime.last_viewport = Some(viewport);
   runtime.last_scale = scale;
   let launch_probe_response_visible = runtime.launch_probe_response_visible;
   let Some(router) = runtime.router.as_mut() else {return -4};
   router.update(now, delta_ms);
   router.draw(viewport, scale, &mut builder);
   if launch_probe_response_visible
   {
      builder.rrect(gfx::RectF::new(16.0, 776.0, 24.0, 24.0), [0.0; 4], gfx::Color::rgba(1.0, 1.0, 1.0, 1.0));
   }
   router.take_damage_into(&mut damage);
   oxide_ui_core::coalesce_adjacent_draws_reuse(builder.drawlist_mut(), &mut runtime.coalesced);
   runtime.builder = builder;
   if runtime.pending_damage.is_empty()
   {
      core::mem::swap(&mut runtime.pending_damage, &mut damage);
   }
   else
   {
      runtime.pending_damage.append(&mut damage);
   }
   runtime.reusable_damage = damage;
   runtime.prepared = true;
   if diagnostics_enabled
   {
      runtime.pending_render_prepare_begin_ticks = prepare_begin_ticks;
      runtime.pending_render_prepare_end_ticks = mach_continuous_time();
   }
   0
}

#[no_mangle]
pub extern "C" fn oxide_comparison_pointer_down(x: f32, y: f32, timestamp_seconds: f64) -> ::libc::c_int
{
   with_runtime(|runtime| {
      if !timestamp_seconds.is_finite() || timestamp_seconds < 0.0 {return -1}
      let startup = runtime.reset.as_ref().is_some_and(|reset| reset.scenario.id() == "startup.first-screen");
      if !startup || !(16.0..374.0).contains(&x) || !(776.0..824.0).contains(&y)
      {
         return 0;
      }
      runtime.launch_probe_generation = runtime.launch_probe_generation.saturating_add(1);
      runtime.interaction_generation = runtime.interaction_generation.saturating_add(1);
      runtime.launch_probe_response_visible = true;
      mark_dirty(runtime);
      1
   })
}

#[no_mangle]
pub extern "C" fn oxide_comparison_launch_probe_generation() -> u64
{
   with_runtime(|runtime| runtime.launch_probe_generation)
}

#[no_mangle]
pub extern "C" fn oxide_comparison_interaction_generation() -> u64
{
   with_runtime(|runtime| runtime.interaction_generation)
}

#[no_mangle]
pub extern "C" fn oxide_comparison_macos_pointer_event(kind: u32, x: f32, y: f32, timestamp_seconds: f64) -> ::libc::c_int
{
   if !x.is_finite() || !y.is_finite() || !timestamp_seconds.is_finite() || timestamp_seconds < 0.0
   {
      return -1;
   }
   with_runtime(|runtime| {
      let Some(router) = runtime.router.as_mut() else {return -2};
      let changed = match kind
      {
         0 =>
         {
            runtime.macos_pointer = Some(MacOsPointerState {x, y, moved: false});
            router.comparison_host_pointer_down(x, y)
         }
         1 =>
         {
            let Some(mut pointer) = runtime.macos_pointer else {return -3};
            let dx = x - pointer.x;
            let dy = y - pointer.y;
            pointer.x = x;
            pointer.y = y;
            pointer.moved = pointer.moved || dx != 0.0 || dy != 0.0;
            runtime.macos_pointer = Some(pointer);
            router.comparison_host_pointer_move(x, y, dx, dy)
         }
         2 =>
         {
            let Some(pointer) = runtime.macos_pointer.take() else {return -3};
            if pointer.moved
            {
               let moved = router.comparison_host_pointer_move(x, y, x - pointer.x, y - pointer.y);
               router.comparison_host_pointer_up() || moved
            }
            else
            {
               let released = router.comparison_host_pointer_up();
               router.comparison_host_click(x, y) || released
            }
         }
         _ => return -4,
      };
      record_interaction(runtime, changed)
   })
}

#[no_mangle]
pub extern "C" fn oxide_comparison_macos_scroll(delta_y_points: f32) -> ::libc::c_int
{
   if !delta_y_points.is_finite()
   {
      return -1;
   }
   with_runtime(|runtime| {
      let Some(height) = runtime.last_viewport.map(|viewport| viewport.h).filter(|height| *height > 0.0) else {return -2};
      let delta = (delta_y_points / height * 1_000_000.0).round().clamp(i32::MIN as f32, i32::MAX as f32) as i32;
      let Some(router) = runtime.router.as_mut() else {return -2};
      let changed = router.comparison_host_wheel(delta);
      record_interaction(runtime, changed)
   })
}

#[no_mangle]
pub extern "C" fn oxide_comparison_macos_text(value: *const u8, value_length: usize) -> ::libc::c_int
{
   let Some(value) = ffi_utf8(value, value_length) else {return -1};
   with_runtime(|runtime| {
      let Some(router) = runtime.router.as_mut() else {return -2};
      let changed = router.comparison_host_text(value);
      record_interaction(runtime, changed)
   })
}

#[no_mangle]
pub extern "C" fn oxide_comparison_macos_key(key_code: u16, shift: ::libc::c_int, value: *const u8, value_length: usize) -> ::libc::c_int
{
   let value = if value_length == 0
   {
      ""
   }
   else
   {
      let Some(value) = ffi_utf8(value, value_length) else {return -1};
      value
   };
   with_runtime(|runtime| {
      let Some(router) = runtime.router.as_mut() else {return -2};
      let changed = router.comparison_host_key(key_code, shift != 0, value);
      record_interaction(runtime, changed)
   })
}

#[no_mangle]
pub extern "C" fn oxide_comparison_submit_prepared_frame(drawable: *mut ::libc::c_void) -> ::libc::c_int
{
   let mut runtime = lock_runtime();
   if !runtime.initialized || !runtime.prepared
   {
      return -1;
   }
   let mut damage = core::mem::take(&mut runtime.pending_damage);
   let builder = core::mem::take(&mut runtime.builder);
   let diagnostics_enabled = runtime.renderer_diagnostics_enabled;
   let render_prepare_begin_ticks = runtime.pending_render_prepare_begin_ticks;
   let render_prepare_end_ticks = runtime.pending_render_prepare_end_ticks;
   let Some(renderer) = runtime.renderer.as_deref_mut() else {return -2};
   if !drawable.is_null() && unsafe {renderer.prepare_present_drawable(drawable.cast())}.is_err()
   {
      runtime.builder = builder;
      runtime.pending_damage = damage;
      runtime.prepared = false;
      return -3;
   }
   let frame_damage = gfx::Damage {rects: core::mem::take(&mut damage)};
   let encode_begin_ticks = if diagnostics_enabled {mach_continuous_time()} else {0};
   let token = renderer.begin_frame(&gfx::FrameTarget, Some(&frame_damage));
   let submitted_frame_id = token.0;
   damage = frame_damage.rects;
   renderer.encode_pass(builder.drawlist());
   let encode_end_ticks = if diagnostics_enabled {mach_continuous_time()} else {0};
   let command_submit_ticks = if diagnostics_enabled {mach_continuous_time()} else {0};
   let result = renderer.submit(token);
   let encode_diagnostics = if diagnostics_enabled && result.is_ok()
   {
      let stats = renderer.last_stats();
      Some(ComparisonEncodeDiagnostics {
         frame_id: submitted_frame_id,
         render_prepare_begin_ticks,
         render_prepare_end_ticks,
         encode_begin_ticks,
         encode_end_ticks,
         command_submit_ticks,
         encoded_bytes: stats.vb_bytes.saturating_add(stats.ib_bytes).saturating_add(stats.ub_bytes),
         draw_calls: u64::from(stats.draws),
         damage_pixels: stats.damage_px,
         damage_rects: u64::from(stats.damage_rects),
      })
   }
   else
   {
      None
   };
   runtime.builder = builder;
   runtime.prepared = false;
   if result.is_err()
   {
      runtime.pending_damage = damage;
      return -4;
   }
   if let Some(diagnostics) = encode_diagnostics
   {
      runtime.latest_encode_diagnostics = diagnostics;
   }
   damage.clear();
   runtime.reusable_damage = damage;
   0
}

#[no_mangle]
pub extern "C" fn oxide_comparison_set_renderer_diagnostics_enabled(enabled: ::libc::c_int)
{
   with_runtime(|runtime| {
      runtime.renderer_diagnostics_enabled = enabled != 0;
      if !runtime.renderer_diagnostics_enabled
      {
         runtime.latest_encode_diagnostics = ComparisonEncodeDiagnostics::default();
         runtime.pending_render_prepare_begin_ticks = 0;
         runtime.pending_render_prepare_end_ticks = 0;
      }
   });
}

#[no_mangle]
pub extern "C" fn oxide_comparison_renderer_diagnostics(output: *mut OxideComparisonRendererDiagnostics) -> ::libc::c_int
{
   if output.is_null()
   {
      return -1;
   }
   with_runtime(|runtime| {
      if !runtime.renderer_diagnostics_enabled
      {
         return 0;
      }
      let Some(renderer) = runtime.renderer.as_deref() else {return -2};
      let encode = runtime.latest_encode_diagnostics;
      let gpu = renderer.last_stats();
      unsafe
      {
         output.write(OxideComparisonRendererDiagnostics {
            schema_version: 1,
            reserved: 0,
            submitted_frame_id: encode.frame_id,
            completed_frame_id: gpu.gpu_frame_id,
            render_prepare_begin_ticks: encode.render_prepare_begin_ticks,
            render_prepare_end_ticks: encode.render_prepare_end_ticks,
            encode_begin_ticks: encode.encode_begin_ticks,
            encode_end_ticks: encode.encode_end_ticks,
            command_submit_ticks: encode.command_submit_ticks,
            encoded_bytes: encode.encoded_bytes,
            draw_calls: encode.draw_calls,
            damage_pixels: encode.damage_pixels,
            damage_rects: encode.damage_rects,
            gpu_duration_ns: milliseconds_to_nanoseconds(gpu.gpu_ms),
            gpu_render_duration_ns: milliseconds_to_nanoseconds(gpu.gpu_render_ms),
         });
      }
      1
   })
}

fn milliseconds_to_nanoseconds(value: f64) -> u64
{
   if !value.is_finite() || value <= 0.0
   {
      return 0;
   }
   let nanoseconds = value * 1_000_000.0;
   if nanoseconds >= u64::MAX as f64
   {
      u64::MAX
   }
   else
   {
      nanoseconds.round() as u64
   }
}

#[cfg(target_os = "macos")]
fn mach_continuous_time() -> u64
{
   unsafe extern "C"
   {
      fn mach_continuous_time() -> u64;
   }
   // SAFETY: mach_continuous_time has no arguments or caller-owned memory.
   unsafe {mach_continuous_time()}
}

#[cfg(not(target_os = "macos"))]
fn mach_continuous_time() -> u64
{
   0
}

#[no_mangle]
pub extern "C" fn oxide_comparison_cancel_prepared_frame()
{
   let mut runtime = lock_runtime();
   runtime.prepared = false;
}

fn ffi_utf8<'a>(pointer: *const u8, length: usize) -> Option<&'a str>
{
   if pointer.is_null()
   {
      return None;
   }
   std::str::from_utf8(unsafe {std::slice::from_raw_parts(pointer, length)}).ok()
}

#[no_mangle]
pub extern "C" fn oxide_comparison_configure_scale_overlay(json: *const u8, json_length: usize) -> ::libc::c_int
{
   if json.is_null()
   {
      return -1;
   }
   let bytes = unsafe {std::slice::from_raw_parts(json, json_length)};
   let Ok(overlay) = serde_json::from_slice::<MacOsComparatorScaleOverlay>(bytes) else {return -2};
   with_runtime(|runtime| {
      if runtime.reset.is_some()
      {
         return -3;
      }
      runtime.pending_scale_overlay = Some(overlay);
      0
   })
}

#[no_mangle]
pub extern "C" fn oxide_comparison_prepare_scenario(root: *const u8, root_length: usize, scenario: *const u8, scenario_length: usize) -> ::libc::c_int
{
   let Some(root) = ffi_utf8(root, root_length) else {return -2};
   let Some(scenario_id) = ffi_utf8(scenario, scenario_length) else {return -2};
   let package = match package::load(Path::new(root), scenario_id)
   {
      Ok(package) => package,
      Err(error) =>
      {
         eprintln!("oxide.comparison: package load failed: {error}");
         return -3;
      }
   };
   prepare_loaded_package(package)
}

#[no_mangle]
pub extern "C" fn oxide_comparison_prepare_release_candidate(root: *const u8, root_length: usize, scenario: *const u8, scenario_length: usize) -> ::libc::c_int
{
   let Some(root) = ffi_utf8(root, root_length) else {return -2};
   let Some(scenario_id) = ffi_utf8(scenario, scenario_length) else {return -2};
   let package = match package::load_release_candidate(Path::new(root), scenario_id)
   {
      Ok(package) => package,
      Err(error) =>
      {
         eprintln!("oxide.comparison: release candidate package load failed: {error}");
         return -3;
      }
   };
   prepare_loaded_package(package)
}

fn prepare_loaded_package(package: package::ComparisonPackage) -> ::libc::c_int
{
   let package::ComparisonPackage {
      scenario,
      fixture,
      traces,
      atlas,
      inline_text,
      latin_font,
      arabic_font,
      cjk_font,
      image,
   } = package;
   let fonts = [latin_font, arabic_font, cjk_font].map(|font| {
      let variations = font.variations.iter().filter_map(|variation| {
         let tag = variation.tag.as_bytes().try_into().ok()?;
         Some(text::FontVariation {
            tag,
            value: variation.value_millionths as f32 / 1_000_000.0,
         })
      }).collect::<Vec<_>>();
      text::Font::from_bytes_with_variations(font.bytes, &variations)
   });
   with_runtime(|runtime| {
      let Some(renderer) = runtime.renderer.as_deref_mut() else {return -1};
      let accented_atlas = if scenario.id() == "grid.large-scroll"
      {
         atlas.as_ref().map(|(width, height, bgra)| {
            let mut accented = bgra.clone();
            for pixel in accented.chunks_exact_mut(4)
            {
               pixel[0] = ((u16::from(pixel[0]) * 4 + 239 + 2) / 5) as u8;
               pixel[1] = ((u16::from(pixel[1]) * 4 + 110 + 2) / 5) as u8;
               pixel[2] = ((u16::from(pixel[2]) * 4 + 61 + 2) / 5) as u8;
            }
            renderer.image_create_rgba8_immutable(*width, *height, &accented, *width as usize * 4, false)
         })
      }
      else
      {
         None
      };
      let atlas = atlas.map(|(width, height, bgra)| renderer.image_create_rgba8_immutable(width, height, &bgra, width as usize * 4, false));
      let inline_text = inline_text.map(|inline| {
         let images = inline.rasters.into_iter().map(|raster| renderer.image_create_rgba8_immutable(raster.width, raster.height, &raster.bgra, raster.width as usize * 4, false)).collect::<Vec<_>>();
         (images, inline.atlas)
      });
      let image = image.map(|image| {
         let thumbnail = renderer.image_create_rgba8_immutable(image.thumbnail_width, image.thumbnail_height, &image.thumbnail_bgra, image.thumbnail_width as usize * 4, false);
         ComparisonImageRuntime {
            source_png: image.source_png,
            source_sha256: image.source_sha256,
            source_width: image.source_width,
            source_height: image.source_height,
            source_bgra: None,
            source: None,
            thumbnail,
            thumbnail_sha256: image.thumbnail_sha256,
         }
      });
      let Some(router) = runtime.router.as_mut() else {return -1};
      let scale_overlay = runtime.pending_scale_overlay.take();
      let prepared = prepare_router_scenario(router, &scenario, &fixture, scale_overlay.as_ref());
      if prepared.is_err()
      {
         return -5;
      }
      let [latin, arabic, cjk] = fonts;
      let fonts = [router.text.fonts.add_font(latin), router.text.fonts.add_font(arabic), router.text.fonts.add_font(cjk)];
      router.text.set_fallback_fonts(&fonts);
      let resources = if let Some(atlas) = atlas
      {
         router.set_comparison_resources(atlas, fonts)
      }
      else
      {
         router.set_comparison_fonts(fonts)
      };
      if resources.is_err()
      {
         return -6;
      }
      if let Some(accented_atlas) = accented_atlas
      {
         if router.set_comparison_thumbnail_variant_resource(accented_atlas).is_err()
         {
            return -6;
         }
      }
      if let Some((images, atlas)) = inline_text.as_ref()
      {
         if router.set_comparison_inline_text_resources(images.clone(), atlas.clone()).is_err()
         {
            return -7;
         }
      }
      if let Some(image) = image.as_ref()
      {
         if router.set_comparison_image_thumbnail_resource(image.thumbnail, &image.thumbnail_sha256).is_err()
         {
            return -8;
         }
      }
      runtime.traces = traces;
      runtime.reset = Some(ComparisonReset {scenario, fixture, scale_overlay, atlas, accented_atlas, inline_text, fonts, image});
      mark_dirty(runtime);
      0
   })
}

fn prepare_router_scenario(router: &mut scenes::Router<MetalUploader>, scenario: &package::ComparisonScenario, fixture: &[u8], scale_overlay: Option<&MacOsComparatorScaleOverlay>) -> Result<(), String>
{
   match (scenario, scale_overlay)
   {
      (package::ComparisonScenario::Runnable(scenario), Some(overlay)) => router.prepare_scaled_comparison_scenario(scenario, fixture, overlay),
      (package::ComparisonScenario::Runnable(scenario), None) => router.prepare_comparison_scenario(scenario, fixture),
      (package::ComparisonScenario::ReleaseCandidate(_), Some(_)) => Err(String::from("release-candidate capture does not admit comparator scale overlays")),
      (package::ComparisonScenario::ReleaseCandidate(scenario_id), None) => router.prepare_comparison_scenario_id(scenario_id, fixture),
   }
}

#[no_mangle]
pub extern "C" fn oxide_comparison_reset_scenario() -> ::libc::c_int
{
   with_runtime(|runtime| {
      let Some(reset) = runtime.reset.as_ref() else {return -1};
      let Some(router) = runtime.router.as_mut() else {return -1};
      let prepared = prepare_router_scenario(router, &reset.scenario, &reset.fixture, reset.scale_overlay.as_ref());
      if prepared.is_err()
      {
         return -2;
      }
      let resources = if let Some(atlas) = reset.atlas
      {
         router.set_comparison_resources(atlas, reset.fonts)
      }
      else
      {
         router.set_comparison_fonts(reset.fonts)
      };
      if resources.is_err()
      {
         return -3;
      }
      if let Some(accented_atlas) = reset.accented_atlas
      {
         if router.set_comparison_thumbnail_variant_resource(accented_atlas).is_err()
         {
            return -3;
         }
      }
      if let Some((images, atlas)) = reset.inline_text.as_ref()
      {
         if router.set_comparison_inline_text_resources(images.clone(), atlas.clone()).is_err()
         {
            return -4;
         }
      }
      if let Some(image) = reset.image.as_ref()
      {
         if router.set_comparison_image_thumbnail_resource(image.thumbnail, &image.thumbnail_sha256).is_err()
         {
            return -5;
         }
         if let Some(source) = image.source
         {
            if router.set_comparison_image_source_resource(source, &image.source_sha256).is_err()
            {
               return -6;
            }
         }
      }
      mark_dirty(runtime);
      0
   })
}

#[no_mangle]
pub extern "C" fn oxide_comparison_apply_trace_event(phase: *const u8, phase_length: usize, event_index: usize) -> ::libc::c_int
{
   let Some(phase_id) = ffi_utf8(phase, phase_length) else {return -2};
   with_runtime(|runtime| {
      let event = runtime.traces.iter().find(|(id, _)| id == phase_id).and_then(|(_, events)| events.get(event_index)).map(|event| event as *const TraceEvent);
      let Some(event) = event else {return -3};
      let decode_source = unsafe {&*event}.target.as_deref() == Some("image:decoded");
      let upload_source = unsafe {&*event}.target.as_deref() == Some("image:texture");
      let Runtime {renderer, router, reset, ..} = runtime;
      let Some(router) = router.as_mut() else {return -1};
      if decode_source
      {
         let Some(image) = reset.as_mut().and_then(|reset| reset.image.as_mut()) else {return -4};
         if image.source_bgra.is_none()
         {
            let Ok((width, height, bgra)) = package::decode_png_bgra(&image.source_png) else {return -5};
            if width != image.source_width || height != image.source_height
            {
               return -6;
            }
            image.source_bgra = Some(bgra);
         }
      }
      if upload_source
      {
         let Some(renderer) = renderer.as_deref_mut() else {return -1};
         let Some(image) = reset.as_mut().and_then(|reset| reset.image.as_mut()) else {return -4};
         if image.source.is_none()
         {
            let Some(bgra) = image.source_bgra.take() else {return -7};
            image.source = Some(renderer.image_create_rgba8_immutable(image.source_width, image.source_height, &bgra, image.source_width as usize * 4, false));
         }
         if router.set_comparison_image_source_resource(image.source.unwrap_or(gfx::ImageHandle(0)), &image.source_sha256).is_err()
         {
            return -8;
         }
      }
      if router.apply_comparison_event(unsafe {&*event}).is_err()
      {
         return -9;
      }
      mark_dirty(runtime);
      0
   })
}

#[no_mangle]
pub extern "C" fn oxide_comparison_scale_attestation(effective_cardinality: *mut u64, completed: *mut u32) -> ::libc::c_int
{
   if effective_cardinality.is_null() || completed.is_null()
   {
      return -1;
   }
   with_runtime(|runtime| {
      let Some((cardinality, is_completed)) = runtime.router.as_ref().and_then(scenes::Router::comparison_scale_attestation) else {return 0};
      unsafe
      {
         effective_cardinality.write(cardinality);
         completed.write(u32::from(is_completed));
      }
      1
   })
}

#[no_mangle]
pub extern "C" fn oxide_comparison_role_count() -> u32
{
   with_runtime(|runtime| runtime.router.as_ref().and_then(scenes::Router::comparison_role_counts).map_or(0, |roles| roles.len() as u32))
}

#[no_mangle]
pub extern "C" fn oxide_comparison_role(index: u32, name: *mut u8, name_length: usize, count: *mut u32) -> u32
{
   with_runtime(|runtime| {
      let Some(roles) = runtime.router.as_ref().and_then(scenes::Router::comparison_role_counts) else {return 0};
      let Some(role) = roles.get(index as usize) else {return 0};
      if !count.is_null()
      {
         unsafe {*count = role.count};
      }
      let bytes = role.role.as_bytes();
      let needed = bytes.len().saturating_add(1);
      if !name.is_null() && name_length >= needed
      {
         unsafe
         {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), name, bytes.len());
            *name.add(bytes.len()) = 0;
         }
      }
      needed as u32
   })
}

#[no_mangle]
pub extern "C" fn oxide_comparison_checkpoint_json(checkpoint: *const u8, checkpoint_length: usize, artifact: u32, output: *mut u8, output_length: usize) -> u32
{
   let Some(checkpoint_id) = ffi_utf8(checkpoint, checkpoint_length) else {return 0};
   with_runtime(|runtime| {
      let Some(router) = runtime.router.as_ref() else {return 0};
      let Ok((state, accessibility)) = router.comparison_checkpoint_json(checkpoint_id) else {return 0};
      let bytes = match artifact
      {
         0 => &state,
         1 => &accessibility,
         _ => return 0,
      };
      if !output.is_null() && output_length >= bytes.len()
      {
         unsafe {std::ptr::copy_nonoverlapping(bytes.as_ptr(), output, bytes.len())};
      }
      bytes.len() as u32
   })
}

#[no_mangle]
pub extern "C" fn oxide_comparison_geometry_nodes_json(output: *mut u8, output_length: usize) -> u32
{
   with_runtime(|runtime| {
      let Some(viewport) = runtime.last_viewport else {return 0};
      let scale = runtime.last_scale;
      let Some(router) = runtime.router.as_mut() else {return 0};
      let Ok(geometry) = router.capture_comparison_geometry(viewport, scale) else {return 0};
      let canonical = |value: f32| (f64::from(value) * 3.0).round() / 3.0;
      let nodes = geometry.iter().enumerate().map(|(ordinal, node)| serde_json::json!({
         "ordinal": ordinal,
         "kind": if node.text_line_bounds.is_empty() {"element"} else {"text"},
         "role": node.role,
         "identifier": node.identifier,
         "bounds": {"x": canonical(node.bounds.x), "y": canonical(node.bounds.y), "width": canonical(node.bounds.w), "height": canonical(node.bounds.h)},
         "text_line_bounds": node.text_line_bounds.iter().map(|line| serde_json::json!({"x": canonical(line.x), "y": canonical(line.y), "width": canonical(line.w), "height": canonical(line.h)})).collect::<Vec<_>>(),
      })).collect::<Vec<_>>();
      let Ok(bytes) = serde_json::to_vec(&nodes) else {return 0};
      if !output.is_null() && output_length >= bytes.len()
      {
         unsafe {std::ptr::copy_nonoverlapping(bytes.as_ptr(), output, bytes.len())};
      }
      bytes.len() as u32
   })
}

#[no_mangle]
pub extern "C" fn oxide_comparison_quiesce() -> ::libc::c_int
{
   with_runtime(|runtime| {
      let Some(renderer) = runtime.renderer.as_deref_mut() else {return -1};
      if renderer.readback_bgra8().is_some() {0} else {-1}
   })
}

#[no_mangle]
pub extern "C" fn oxide_comparison_take_snapshot() -> ::libc::c_int
{
   with_runtime(|runtime| {
      let outcome = (|| -> Result<(u32, u32, usize), String> {
         let Runtime {renderer, snapshot_bgra, snapshot_png, ..} = runtime;
         let Some(renderer) = renderer.as_deref_mut() else {return Err(String::from("renderer unavailable"))};
         let Some((width, height, pixels)) = renderer.readback_bgra8() else
         {
            return Err(String::from("no readable target"));
         };
         *snapshot_bgra = pixels;
         for pixel in snapshot_bgra.chunks_exact_mut(4)
         {
            pixel.swap(0, 2);
            pixel[3] = 255;
         }
         snapshot_png.clear();
         let mut encoder = png::Encoder::new(&mut *snapshot_png, width, height);
         encoder.set_color(png::ColorType::Rgba);
         encoder.set_depth(png::BitDepth::Eight);
         {
            let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
            writer.write_chunk(png::chunk::sRGB, &[0]).map_err(|error| error.to_string())?;
            writer.write_image_data(snapshot_bgra).map_err(|error| error.to_string())?;
         }
         Ok((width, height, snapshot_png.len()))
      })();
      let (message, code) = match outcome
      {
         Ok((width, height, bytes)) => (format!("Captured snapshot {width}x{height} in {bytes} bytes"), 0),
         Err(error) =>
         {
            runtime.snapshot_png.clear();
            (format!("Snapshot failed: {error}"), -2)
         }
      };
      runtime.snapshot_status = message;
      code
   })
}

#[no_mangle]
pub extern "C" fn oxide_comparison_snapshot_png(output: *mut u8, output_length: usize) -> usize
{
   with_runtime(|runtime| {
      let bytes = &runtime.snapshot_png;
      if !output.is_null() && output_length >= bytes.len()
      {
         unsafe {std::ptr::copy_nonoverlapping(bytes.as_ptr(), output, bytes.len())};
      }
      bytes.len()
   })
}

#[no_mangle]
pub extern "C" fn oxide_comparison_snapshot_status(output: *mut ::libc::c_char, output_length: u32) -> u32
{
   if output.is_null() || output_length == 0
   {
      return 0;
   }
   with_runtime(|runtime| {
      let bytes = runtime.snapshot_status.as_bytes();
      let copied = bytes.len().min((output_length as usize).saturating_sub(1));
      unsafe
      {
         std::ptr::copy_nonoverlapping(bytes.as_ptr(), output.cast::<u8>(), copied);
         *output.add(copied) = 0;
      }
      bytes.len() as u32
   })
}
