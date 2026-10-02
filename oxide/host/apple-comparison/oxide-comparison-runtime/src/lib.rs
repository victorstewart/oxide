//! Fixed iOS comparison fixtures consuming the production public rendering APIs.

use std::ffi::{c_char, c_void, CStr};
use std::io::Cursor;
use std::sync::{Arc, Mutex, OnceLock};
#[cfg(feature = "native-measurement")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "native-measurement")]
use std::time::{Duration, Instant};

use serde::Deserialize;

use oxide_renderer_api as gfx;
use oxide_renderer_api::Renderer;
use oxide_renderer_metal as metal;
use oxide_text as text;
use oxide_ui_core as ui;
use ui::elements::{Button, ButtonState, ImageFit, ImageView, Label, TextCtx};

mod visual_controls;
mod visual_structure;
mod visual_boards;
mod visual_extended;
mod visual_editing_edges;
mod native_metrics;
#[cfg(all(test, target_os = "macos"))]
mod resource_rebuild_tests;

const FIXTURE: &str = include_str!("../../fixtures/core.json");
const FONT_BYTES: &[u8] = include_bytes!("../../../../crates/text/tests/fixtures/NotoSans-VF.ttf");
const IMAGE_BYTES: [&[u8]; 4] = [
   include_bytes!("../../fixtures/image-0.png"), include_bytes!("../../fixtures/image-1.png"),
   include_bytes!("../../fixtures/image-2.png"), include_bytes!("../../fixtures/image-3.png"),
];

pub const NATIVE_CASES: &[&str] = &[
   "shapes", "text", "local", "images", "animation", "scroll", "visual-controls", "visual-editing",
   "visual-typography", "visual-composition", "visual-layout", "visual-pickers", "visual-opacity",
   "visual-images", "visual-geometry", "visual-editing-edges",
];

#[derive(Deserialize)]
struct Fixture
{
   color_space: String, compositing: String, viewport: [u32; 3], text_color: [f32; 4], shapes: Shapes, text: TextCase,
   local: LocalCase, images_case: ImagesCase, animation: AnimationCase, scroll: ScrollCase,
}

#[derive(Deserialize)]
struct Shapes
{
   count: usize, clip_rects: Vec<[f32; 4]>, tile_size: [f32; 2], column_stride: f32,
   row_stride: f32, origin: [f32; 2], radii: [f32; 2], alpha: f32, updates_per_frame: usize,
}

#[derive(Deserialize)]
struct TextCase
{
   count: usize, origin: [f32; 2], stride: [f32; 2], size: [f32; 2], columns: usize,
   font_size: f32, template: String, interval: f64,
}

#[derive(Deserialize)]
struct LocalCase
{
   count: usize, origin: [f32; 2], stride: [f32; 2], columns: usize, label_size: [f32; 2],
   button_rect: [f32; 4], progress_rect: [f32; 4], font_size: f32, template: String,
   button_title: String, button_color: [f32; 4], progress_track_color: [f32; 4],
   progress_fill_color: [f32; 4], interval: f64,
}

#[derive(Deserialize)]
struct ImagesCase
{
   count: usize, origin: [f32; 2], stride: [f32; 2], columns: usize, size: [f32; 2],
   scale_amplitude: f32, period: f64,
}

#[derive(Deserialize)]
struct AnimationCase
{
   count: usize, origin: [f32; 2], stride: [f32; 2], columns: usize, size: [f32; 2],
   image_rect: [f32; 4], label_rect: [f32; 4], font_size: f32, period: f64,
   translation: [f32; 2], scale_center: f32, scale_amplitude: f32,
   alpha_center: f32, alpha_amplitude: f32, template: String,
}

#[derive(Deserialize)]
struct ScrollCase
{
   count: usize, row_height: f32, distance: f32, half_period: f64,
   image_rect: [f32; 4], label_rect: [f32; 4], font_size: f32, template: String,
}

#[derive(Clone, Copy, Debug)]
enum Case {Shapes, Text, Local, Images, Animation, Scroll}

impl Case
{
   fn parse(name: &str) -> Option<Self>
   {
      match name
      {
         "shapes" => Some(Self::Shapes), "text" => Some(Self::Text),
         "local" => Some(Self::Local), "images" => Some(Self::Images),
         "animation" => Some(Self::Animation), "scroll" => Some(Self::Scroll),
         _ => None,
      }
   }
}

struct Image
{
   handle: gfx::ImageHandle,
   width: u32,
   height: u32,
}

struct Suite
{
   case: Case,
   checkpoint: bool,
   fixture: Fixture,
   images: Vec<Image>,
   shapes: Vec<f32>,
   text_values: Vec<u32>,
   local_values: Vec<u32>,
   last_text_step: Option<i64>,
   last_local_step: Option<i64>,
   text_labels: Vec<Label>,
   local_labels: Vec<Label>,
   buttons: Vec<Button>,
   button_states: Vec<ButtonState>,
   card_labels: Vec<Label>,
   row_labels: Vec<Label>,
   collection: ui::collection::CollectionView,
   animation_sequence: Option<gfx::RenderChunkSequence>,
   retained: Option<RetainedSuite>,
}

struct RetainedSuite
{
   background: gfx::RenderChunk,
   units: Vec<gfx::RenderChunkInstance>,
   builder: ui::DrawListBuilder,
   revisions: Vec<u64>,
}

struct Advance
{
   reset: bool,
   first: i64,
   last: i64,
}

struct Runtime
{
   renderer: metal::MetalRenderer,
   builder: ui::DrawListBuilder,
   coalesced: Vec<gfx::DrawCmd>,
   text: TextCtx,
   suite: Option<Suite>,
   visual: Option<visual_boards::VisualBoards>,
   visual_checkpoint: bool,
   visual_stage: usize,
   native_measurement: native_metrics::NativeMeasurement,
}

static RUNTIME: OnceLock<Mutex<Option<Runtime>>> = OnceLock::new();
#[cfg(feature = "native-measurement")]
static RETAINED_TEXT_ENABLED: AtomicBool = AtomicBool::new(false);

struct MtlUploader
{
   renderer: *mut metal::MetalRenderer,
}

impl ui::elements::ImageUploader for MtlUploader
{
   fn create_a8(&mut self, w: u32, h: u32, data: &[u8], row_bytes: usize) -> gfx::ImageHandle
   {
      unsafe {(*self.renderer).image_create_a8(w, h, data, row_bytes)}
   }

   fn update_a8(&mut self, handle: gfx::ImageHandle, x: u32, y: u32, w: u32, h: u32, data: &[u8], row_bytes: usize)
   {
      unsafe {(*self.renderer).image_update_a8(handle, x, y, w, h, data, row_bytes)}
   }

   fn append_a8(&mut self, handle: gfx::ImageHandle, x: u32, y: u32, w: u32, h: u32, data: &[u8], row_bytes: usize)
   {
      unsafe {(*self.renderer).image_append_a8(handle, x, y, w, h, data, row_bytes)}
   }

   fn release_a8(&mut self, handle: gfx::ImageHandle)
   {
      unsafe {(*self.renderer).image_release(handle)}
   }
}

fn new_runtime() -> Result<Runtime, ()>
{
   let mut renderer = metal::MetalRenderer::new_with_config_and_sdr_compositing(metal::MetalRendererConfig::visible_host(), metal::SdrCompositing::SrgbSourceOver).map_err(|_| ())?;
   renderer.resize(1_170, 2_532, 3.0).map_err(|_| ())?;
   Ok(Runtime {renderer, builder: ui::DrawListBuilder::new(), coalesced: Vec::new(), text: TextCtx::default(), suite: None, visual: None,
      visual_checkpoint: false, visual_stage: 0,
      native_measurement: native_metrics::NativeMeasurement::default()})
}

#[no_mangle]
pub extern "C" fn oxide_core_init() -> i32
{
   let mut slot = RUNTIME.get_or_init(|| Mutex::new(None)).lock().unwrap();
   if slot.is_none() {*slot = new_runtime().ok();}
   if slot.is_some() {0} else {-1}
}

/// Retained feasibility-probe entry point; no suite state is needed.
#[no_mangle]
pub unsafe extern "C" fn oxide_core_draw(drawable: *mut c_void, values: *const f32, count: usize) -> i32
{
   if drawable.is_null() || values.is_null() || count != 64 {return -1;}
   let values = unsafe {std::slice::from_raw_parts(values, count)};
   if values.iter().any(|value| !value.is_finite() || !(0.0..=1.0).contains(value)) {return -2;}
   let mut slot = RUNTIME.get_or_init(|| Mutex::new(None)).lock().unwrap();
   let Some(runtime) = slot.as_mut() else {return -3;};
   runtime.builder.clear();
   runtime.builder.rrect(gfx::RectF::new(0.0, 0.0, 390.0, 844.0), [0.0; 4], rgba([1.0; 4]));
   for (index, value) in values.iter().enumerate()
   {
      let rect = gfx::RectF::new(15.0 + (index % 8) as f32 * 45.0, 22.0 + (index / 8) as f32 * 100.0, 36.0, 80.0);
      runtime.builder.rrect(rect, [6.0; 4], gfx::Color::from_srgba(*value, 0.25, 0.5, 1.0));
   }
   submit(runtime, drawable, None)
}

#[no_mangle]
pub unsafe extern "C" fn oxide_core_suite_init(name: *const c_char, checkpoint: u8) -> i32
{
   if name.is_null() {return -1;}
   let Ok(name) = unsafe {CStr::from_ptr(name)}.to_str() else {return -1;};
   if name.starts_with("visual-")
   {
      if !visual_boards::is_case(name) {return -2;}
      let mut slot = RUNTIME.get_or_init(|| Mutex::new(None)).lock().unwrap();
      if slot.is_none() {*slot = new_runtime().ok();}
      let Some(runtime) = slot.as_mut() else {return -4;};
      runtime.visual = visual_boards::VisualBoards::new(name, &mut runtime.text, &mut runtime.renderer).ok();
      runtime.visual_checkpoint = checkpoint != 0;
      runtime.visual_stage = 0;
      runtime.suite = None;
      return if runtime.visual.is_some() {0} else {-5};
   }
   let Some(case) = Case::parse(name) else {return -2;};
   let Ok(fixture) = serde_json::from_str::<Fixture>(FIXTURE) else {return -3;};
   if fixture.color_space != "srgb" || fixture.compositing != "srgb-source-over" {return -3;}
   let mut slot = RUNTIME.get_or_init(|| Mutex::new(None)).lock().unwrap();
   if slot.is_none() {*slot = new_runtime().ok();}
   let Some(runtime) = slot.as_mut() else {return -4;};
   let font_id = runtime.text.fonts.add_font(text::Font::from_bytes(FONT_BYTES.to_vec()));
   let mut images = Vec::with_capacity(4);
   for bytes in IMAGE_BYTES
   {
      let Ok((pixels, width, height)) = decode_png(bytes) else {return -5;};
      let handle = runtime.renderer.image_create_rgba8_immutable(width, height, &pixels, width as usize * 4, true);
      if handle.0 == 0 {return -6;}
      images.push(Image {handle, width, height});
   }
   let collection = ui::collection::CollectionView::new(ui::collection::CollectionMode::VerticalGrid {col_width: fixture.viewport[0] as f32, spacing: 0.0});
   let text_labels = labels(&fixture.text.template, fixture.text.count, font_id, fixture.text.font_size, fixture.text_color, false);
   let local_labels = labels(&fixture.local.template, fixture.local.count, font_id, fixture.local.font_size, fixture.text_color, false);
   let card_labels = labels(&fixture.animation.template, fixture.animation.count, font_id, fixture.animation.font_size, fixture.text_color, false);
   let row_labels = labels(&fixture.scroll.template, fixture.scroll.count, font_id, fixture.scroll.font_size, fixture.text_color, true);
   let buttons = (0..fixture.local.count).map(|_| Button {
      text: fixture.local.button_title.clone(),
      style: ui::elements::ButtonStyle {text_px: fixture.local.font_size, color: rgba(fixture.local.button_color), ..Default::default()},
   }).collect();
   runtime.visual = None;
   runtime.visual_checkpoint = false;
   runtime.visual_stage = 0;
   runtime.suite = Some(Suite {
      case, checkpoint: checkpoint != 0, shapes: vec![0.0; fixture.shapes.count],
      text_values: vec![0; fixture.text.count], local_values: vec![0; fixture.local.count],
      last_text_step: None, last_local_step: None, text_labels, local_labels, buttons,
      button_states: (0..fixture.local.count).map(|_| ButtonState::default()).collect(),
      card_labels, row_labels, collection, fixture, images, animation_sequence: None, retained: None,
   });
   warm_glyphs(runtime, font_id);
   if matches!(case, Case::Animation) && prepare_animation(runtime).is_err() {return -7;}
   if matches!(case, Case::Text | Case::Local) && retained_text_enabled() && prepare_retained_suite(runtime).is_err() {return -8;}
   0
}

#[no_mangle]
pub unsafe extern "C" fn oxide_core_suite_draw(drawable: *mut c_void, time: f64, generation: u64) -> i32
{
   if drawable.is_null() || !time.is_finite() {return -1;}
   let mut slot = RUNTIME.get_or_init(|| Mutex::new(None)).lock().unwrap();
   let Some(runtime) = slot.as_mut() else {return -2;};
   suite_draw(runtime, drawable, time, generation)
}

fn suite_draw(runtime: &mut Runtime, drawable: *mut c_void, time: f64, generation: u64) -> i32
{
   let timer = native_frame_timer(runtime);
   if let Some(visual) = runtime.visual.as_mut()
   {
      let (stage, timed_stage_elapsed_ms) = if runtime.visual_checkpoint
      {
         if ![0.0, 1.0, 2.0].contains(&time) {return -3;}
         (time as usize, None)
      }
      else
      {
         runtime.visual_stage = visual_stage_at(time);
         (runtime.visual_stage, Some(visual_stage_elapsed_ms(time)))
      };
      let snapshot = if let Some(stage_elapsed_ms) = timed_stage_elapsed_ms
      {
         visual.draw_timed(stage, (time.max(0.0) * 1_000.0).floor() as u64, stage_elapsed_ms, &mut runtime.text, &mut runtime.renderer, &mut runtime.builder)
      }
      else {visual.draw(stage, &mut runtime.text, &mut runtime.renderer, &mut runtime.builder)};
      let Ok(snapshot) = snapshot else {return -8;};
      return submit_snapshot(runtime, drawable, &snapshot, timer);
   }
   let Some(suite) = runtime.suite.as_ref() else {return -3;};
   if matches!(suite.case, Case::Animation) {return submit_animation(runtime, drawable, time, generation, timer);}
   if matches!(suite.case, Case::Text | Case::Local) && suite.retained.is_some() {return submit_retained_suite(runtime, drawable, time, timer);}
   render_suite(runtime, time, generation);
   submit(runtime, drawable, timer)
}

/// Draws the timed visual control condition on its existing board instance.
/// It holds stage zero for the full control window while production component
/// activity, such as indeterminate progress, advances with real elapsed time.
#[no_mangle]
pub unsafe extern "C" fn oxide_core_suite_draw_control(drawable: *mut c_void, elapsed: f64, _: u64) -> i32
{
   if drawable.is_null() || !elapsed.is_finite() {return -1;}
   let mut slot = RUNTIME.get_or_init(|| Mutex::new(None)).lock().unwrap();
   let Some(runtime) = slot.as_mut() else {return -2;};
   if runtime.visual.is_none() || runtime.visual_checkpoint {return -3;}
   let timer = native_frame_timer(runtime);
   runtime.visual_stage = 0;
   let elapsed_ms = (elapsed.max(0.0) * 1_000.0).floor() as u64;
   let visual = runtime.visual.as_mut().unwrap();
   let Ok(snapshot) = visual.draw_timed(0, elapsed_ms, elapsed_ms, &mut runtime.text, &mut runtime.renderer, &mut runtime.builder) else {return -8;};
   submit_snapshot(runtime, drawable, &snapshot, timer)
}

/// Resets a timed visual board. The next draw restores stage zero on the same
/// production board instance, retaining its keyed identities and resources.
#[no_mangle]
pub extern "C" fn oxide_core_suite_reset()
{
   let mut slot = RUNTIME.get_or_init(|| Mutex::new(None)).lock().unwrap();
   if let Some(runtime) = slot.as_mut()
   {
      if runtime.visual.is_some() && !runtime.visual_checkpoint {runtime.visual_stage = 0;}
   }
}

/// Advances timed visual work to a real elapsed timestamp. Returns one when a
/// visual action boundary changed, zero during a hold, and a negative error for
/// an invalid timestamp or an inactive/non-timed visual suite.
#[no_mangle]
pub extern "C" fn oxide_core_suite_advance(elapsed: f64) -> i32
{
   if !elapsed.is_finite() {return -1;}
   let mut slot = RUNTIME.get_or_init(|| Mutex::new(None)).lock().unwrap();
   let Some(runtime) = slot.as_mut() else {return -2;};
   if runtime.visual.is_none() || runtime.visual_checkpoint {return -3;}
   let stage = visual_stage_at(elapsed);
   let changed = runtime.visual_stage != stage;
   runtime.visual_stage = stage;
   i32::from(changed)
}

/// Returns the next absolute elapsed-time wakeup for a timed visual board. A
/// result equal to `elapsed` means production animation is active and the host
/// should retain its native display cadence; future values are idle deadlines.
#[no_mangle]
pub extern "C" fn oxide_core_suite_next_wakeup(elapsed: f64) -> f64
{
   if !elapsed.is_finite() {return f64::NAN;}
   let slot = RUNTIME.get_or_init(|| Mutex::new(None)).lock().unwrap();
   let Some(runtime) = slot.as_ref() else {return f64::NAN;};
   if runtime.visual.is_none() || runtime.visual_checkpoint {return f64::NAN;}
   let stage = visual_stage_at(elapsed);
   let stage_elapsed_ms = visual_stage_elapsed_ms(elapsed);
   let boundary = visual_next_wakeup(elapsed);
   runtime.visual.as_ref().and_then(|visual| visual.next_wakeup(stage, stage_elapsed_ms))
      .map_or(boundary, |delta_ms| if delta_ms == 0 {elapsed.max(0.0)} else {boundary.min(elapsed.max(0.0) + delta_ms as f64 / 1_000.0)})
}

/// Returns the next absolute elapsed-time wakeup for a timed visual control
/// window. An equal value retains the native display cadence; NaN is static.
#[no_mangle]
pub extern "C" fn oxide_core_suite_next_control_wakeup(elapsed: f64) -> f64
{
   if !elapsed.is_finite() {return f64::NAN;}
   let slot = RUNTIME.get_or_init(|| Mutex::new(None)).lock().unwrap();
   let Some(runtime) = slot.as_ref() else {return f64::NAN;};
   if runtime.visual.is_none() || runtime.visual_checkpoint {return f64::NAN;}
   let elapsed_ms = (elapsed.max(0.0) * 1_000.0).floor() as u64;
   runtime.visual.as_ref().and_then(|visual| visual.next_wakeup(0, elapsed_ms))
      .map_or(f64::NAN, |_| elapsed.max(0.0))
}

#[no_mangle]
pub extern "C" fn oxide_core_suite_settle() {}

#[no_mangle]
pub unsafe extern "C" fn oxide_core_suite_gpu_ms(frame_id: *mut u64) -> f64
{
   if frame_id.is_null() {return f64::NAN;}
   let slot = RUNTIME.get_or_init(|| Mutex::new(None)).lock().unwrap();
   let Some(runtime) = slot.as_ref() else {return f64::NAN;};
   let stats = runtime.renderer.last_stats();
   unsafe {*frame_id = stats.gpu_frame_id;}
   stats.gpu_ms
}

#[cfg(feature = "native-measurement")]
pub use native_metrics::NativeFrameMetrics;

#[cfg(feature = "native-measurement")]
pub fn native_set_retained_text_enabled(enabled: bool)
{
   RETAINED_TEXT_ENABLED.store(enabled, Ordering::Relaxed);
}

#[cfg(not(feature = "native-measurement"))]
fn retained_text_enabled() -> bool {false}

#[cfg(feature = "native-measurement")]
fn retained_text_enabled() -> bool {RETAINED_TEXT_ENABLED.load(Ordering::Relaxed)}

#[cfg(feature = "native-measurement")]
pub fn native_draw_offscreen(time: f64, generation: u64) -> i32
{
   if !time.is_finite() {return -1;}
   objc::rc::autoreleasepool(|| {
      let mut slot = RUNTIME.get_or_init(|| Mutex::new(None)).lock().unwrap();
      let Some(runtime) = slot.as_mut() else {return -2;};
      suite_draw(runtime, std::ptr::null_mut(), time, generation)
   })
}

#[cfg(feature = "native-measurement")]
pub fn native_capture_offscreen(path: &std::path::Path) -> Result<(), String>
{
   objc::rc::autoreleasepool(|| {
      let mut slot = RUNTIME.get_or_init(|| Mutex::new(None)).lock().unwrap();
      let Some(runtime) = slot.as_mut() else {return Err("runtime is not initialized".into());};
      let Some((width, height, pixels)) = runtime.renderer.readback_bgra8() else
      {
         return Err("offscreen target readback failed".into());
      };
      write_bgra_png(path, width, height, pixels)
   })
}

#[cfg(feature = "native-measurement")]
pub fn native_wait_for_frame_capacity() -> Result<(), String>
{
   native_wait_for_slots(false)
}

#[cfg(feature = "native-measurement")]
pub fn native_wait_for_gpu() -> Result<(), String>
{
   native_wait_for_slots(true)
}

#[cfg(feature = "native-measurement")]
fn native_wait_for_slots(drain: bool) -> Result<(), String>
{
   let deadline = Instant::now() + Duration::from_secs(5);
   loop
   {
      let ready = {
         let slot = RUNTIME.get_or_init(|| Mutex::new(None)).lock().unwrap();
         let Some(runtime) = slot.as_ref() else {return Err("runtime is not initialized".into());};
         let in_flight = runtime.renderer.frame_slots_in_flight_for_snapshot();
         if drain {in_flight == 0} else {in_flight < runtime.renderer.frame_resource_depth_for_snapshot()}
      };
      if ready {return Ok(());}
      if Instant::now() >= deadline {return Err("timed out waiting for renderer frame capacity".into());}
      std::thread::sleep(Duration::from_micros(100));
   }
}

#[cfg(feature = "native-measurement")]
pub fn enable_native_measurement(accounting: bool)
{
   let mut slot = RUNTIME.get_or_init(|| Mutex::new(None)).lock().unwrap();
   if let Some(runtime) = slot.as_mut()
   {
      runtime.renderer.set_accounting_stats_enabled_for_benchmark(accounting);
      runtime.native_measurement.metrics = NativeFrameMetrics::default();
      runtime.native_measurement.accounting = accounting;
   }
}

#[cfg(feature = "native-measurement")]
pub fn native_frame_metrics() -> NativeFrameMetrics
{
   let slot = RUNTIME.get_or_init(|| Mutex::new(None)).lock().unwrap();
   slot.as_ref().map_or_else(NativeFrameMetrics::default, |runtime| {
      let mut metrics = runtime.native_measurement.metrics;
      metrics.renderer_perf = runtime.renderer.last_stats();
      metrics
   })
}

pub fn native_next_wakeup(name: &str, elapsed: f64) -> f64
{
   if !elapsed.is_finite() {return f64::NAN;}
   let elapsed = elapsed.max(0.0);
   match Case::parse(name)
   {
      Some(Case::Text | Case::Local) => ((elapsed * 10.0).floor() + 1.0) / 10.0,
      Some(_) => elapsed,
      None if visual_boards::is_case(name) => oxide_core_suite_next_wakeup(elapsed),
      None => f64::NAN,
   }
}

#[cfg(feature = "native-measurement")]
#[allow(unexpected_cfgs)]
pub unsafe fn native_capture_drawable(drawable: *mut c_void, path: &std::path::Path) -> Result<(), String>
{
   use metal_rs::foreign_types::ForeignTypeRef;
   use objc::msg_send;
   use objc::runtime::Object;
   use objc::{sel, sel_impl};

   if drawable.is_null() {return Err("drawable is null".into());}
   let drawable = drawable.cast::<Object>();
   let texture: *mut metal_rs::MTLTexture = unsafe {msg_send![drawable, texture]};
   if texture.is_null() {return Err("drawable did not provide a texture".into());}
   let texture = unsafe {metal_rs::TextureRef::from_ptr(texture)};
   let slot = RUNTIME.get_or_init(|| Mutex::new(None)).lock().unwrap();
   let Some(runtime) = slot.as_ref() else {return Err("runtime is not initialized".into());};
   let Some((width, height, pixels)) = runtime.renderer.readback_direct_present_texture_for_snapshot(texture) else
   {
      return Err("drawable texture readback failed".into());
   };
   write_bgra_png(path, width, height, pixels)
}

#[cfg(feature = "native-measurement")]
fn write_bgra_png(path: &std::path::Path, width: u32, height: u32, mut pixels: Vec<u8>) -> Result<(), String>
{
   for pixel in pixels.chunks_exact_mut(4) {pixel.swap(0, 2);}
   let file = std::fs::File::create(path).map_err(|error| error.to_string())?;
   let mut encoder = png::Encoder::new(file, width, height);
   encoder.set_color(png::ColorType::Rgba);
   encoder.set_depth(png::BitDepth::Eight);
   let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
   writer.write_image_data(&pixels).map_err(|error| error.to_string())
}

const VISUAL_CYCLE_SECONDS: f64 = 6.0;
const VISUAL_ACTION_SECONDS: f64 = 2.0;

fn visual_stage_at(elapsed: f64) -> usize
{
   let position = elapsed.max(0.0).rem_euclid(VISUAL_CYCLE_SECONDS);
   if position < VISUAL_ACTION_SECONDS {0}
   else if position < VISUAL_ACTION_SECONDS * 2.0 {1}
   else {2}
}

fn visual_next_wakeup(elapsed: f64) -> f64
{
   let elapsed = elapsed.max(0.0);
   let cycle_start = (elapsed / VISUAL_CYCLE_SECONDS).floor() * VISUAL_CYCLE_SECONDS;
   let position = elapsed - cycle_start;
   if position < VISUAL_ACTION_SECONDS {cycle_start + VISUAL_ACTION_SECONDS}
   else if position < VISUAL_ACTION_SECONDS * 2.0 {cycle_start + VISUAL_ACTION_SECONDS * 2.0}
   else {cycle_start + VISUAL_CYCLE_SECONDS}
}

fn visual_stage_elapsed_ms(elapsed: f64) -> u64
{
   let position = elapsed.max(0.0).rem_euclid(VISUAL_CYCLE_SECONDS);
   let stage_start = if position < VISUAL_ACTION_SECONDS {0.0}
      else if position < VISUAL_ACTION_SECONDS * 2.0 {VISUAL_ACTION_SECONDS}
      else {VISUAL_ACTION_SECONDS * 2.0};
   ((position - stage_start) * 1_000.0).floor() as u64
}

fn native_frame_timer(runtime: &Runtime) -> Option<native_metrics::NativeFrameTimer>
{
   native_metrics::NativeFrameTimer::start(&runtime.renderer, runtime.native_measurement.accounting)
}

fn submit(runtime: &mut Runtime, drawable: *mut c_void, mut timer: Option<native_metrics::NativeFrameTimer>) -> i32
{
   ui::coalesce_adjacent_draws_reuse(runtime.builder.drawlist_mut(), &mut runtime.coalesced);
   if unsafe {runtime.renderer.prepare_present_drawable(drawable)}.is_err() {return -4;}
   if let Some(timer) = timer.as_mut() {timer.prepared(&mut runtime.native_measurement.metrics, &runtime.renderer);}
   let token = runtime.renderer.begin_frame(&gfx::FrameTarget, None);
   runtime.renderer.encode_pass(runtime.builder.drawlist());
   if runtime.renderer.submit(token).is_err() {return -5;}
   if let Some(timer) = timer {timer.submitted(&mut runtime.native_measurement.metrics, &runtime.renderer);}
   0
}

fn submit_snapshot(runtime: &mut Runtime, drawable: *mut c_void, snapshot: &gfx::RenderSnapshot, mut timer: Option<native_metrics::NativeFrameTimer>) -> i32
{
   if unsafe {runtime.renderer.prepare_present_drawable(drawable)}.is_err() {return -4;}
   if let Some(timer) = timer.as_mut() {timer.prepared(&mut runtime.native_measurement.metrics, &runtime.renderer);}
   let token = runtime.renderer.begin_frame(&gfx::FrameTarget, None);
   if let Err(error) = runtime.renderer.encode_snapshot(snapshot)
   {
      let _ = std::fs::write(std::env::temp_dir().join("oxide-visual-render-error.txt"), format!("{error:?}\n{snapshot:#?}"));
      return -9;
   }
   if runtime.renderer.submit(token).is_err() {return -5;}
   if let Some(timer) = timer {timer.submitted(&mut runtime.native_measurement.metrics, &runtime.renderer);}
   0
}

fn warm_glyphs(runtime: &mut Runtime, font_id: usize)
{
   runtime.builder.clear();
   runtime.text.begin_frame_at_scale(3.0);
   let mut uploader = MtlUploader {renderer: &mut runtime.renderer};
   for size in [12.0, 14.0]
   {
      let label = Label {
         text: "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz :".into(),
         color: rgba([0.0, 0.0, 0.0, 1.0]), align: ui::elements::Align::Left,
         wrap: false, font_id, font_px: size,
      };
      label.encode(gfx::RectF::new(0.0, 0.0, 2000.0, 60.0), 3.0, &mut runtime.text, &mut uploader, &mut runtime.builder);
   }
   runtime.text.finish_frame(&mut uploader, &mut runtime.builder);
   runtime.builder.clear();
}

fn render_suite(runtime: &mut Runtime, time: f64, generation: u64)
{
   let Runtime {renderer, builder, text, suite, ..} = runtime;
   let suite = suite.as_mut().unwrap();
   builder.clear();
   builder.rrect(gfx::RectF::new(0.0, 0.0, 390.0, 844.0), [0.0; 4], rgba([1.0; 4]));
   text.begin_frame_at_scale(3.0);
   match suite.case
   {
      Case::Shapes => draw_shapes(suite, time, generation, builder),
      Case::Text => draw_text(suite, time, text, renderer, builder),
      Case::Local => draw_local(suite, time, text, renderer, builder),
      Case::Images => draw_images(suite, time, builder),
      Case::Scroll => draw_scroll(suite, time, text, renderer, builder),
      Case::Animation => unreachable!("animation uses retained chunks"),
   }
   text.finish_frame(&mut MtlUploader {renderer}, builder);
}

fn draw_shapes(suite: &mut Suite, time: f64, generation: u64, builder: &mut ui::DrawListBuilder)
{
   let spec = &suite.fixture.shapes;
   if time < 0.0 {suite.shapes.fill(0.0);}
   else if suite.checkpoint
   {
      suite.shapes.fill(0.0);
      for frame in generation.saturating_sub(7)..=generation
      {
         let start = (frame as usize % 8) * spec.updates_per_frame;
         let value = (((frame as f64 / 120.0) * 3.0).sin() as f32 + 1.0) * 0.5;
         suite.shapes[start..start + spec.updates_per_frame].fill(value);
      }
   }
   else
   {
      let start = (generation as usize % 8) * spec.updates_per_frame;
      suite.shapes[start..start + spec.updates_per_frame].fill(((time * 3.0).sin() as f32 + 1.0) * 0.5);
   }
   for (container, clip) in spec.clip_rects.iter().enumerate()
   {
      builder.clip_push(rect_i(*clip));
      for local in 0..16
      {
         let index = container * 16 + local;
         let x = clip[0] + spec.origin[0] + (local % 4) as f32 * spec.column_stride;
         let y = clip[1] + spec.origin[1] + (local / 4) as f32 * spec.row_stride;
         builder.rrect(gfx::RectF::new(x, y, spec.tile_size[0], spec.tile_size[1]), [spec.radii[index % 2]; 4], gfx::Color::from_srgba(suite.shapes[index], 0.25, 0.5, spec.alpha));
      }
      builder.clip_pop();
   }
}

fn advance(values: &mut [u32], labels: &mut [Label], last: &mut Option<i64>, time: f64, interval: f64, template_text: &str) -> Option<Advance>
{
   // Quantized fixture times such as 0.3 / 0.1 must agree across Swift and Rust.
   let target = (time / interval + 1e-7).floor() as i64;
   if time < 0.0 {return None;}
   let reset = last.is_some_and(|prior| prior > target);
   if reset
   {
      values.fill(0);
      *last = None;
      for (index, label) in labels.iter_mut().enumerate() {label.text = template(template_text, index, 0);}
   }
   let first = last.map_or(0, |prior| prior + 1);
   for step in first..=target
   {
      let index = step as usize % values.len();
      values[index] = step as u32 + 1;
      labels[index].text = template(template_text, index, values[index]);
   }
   *last = Some(target);
   Some(Advance {reset, first, last: target})
}

fn labels(template_text: &str, count: usize, font_id: usize, font_px: f32, color: [f32; 4], row_numbers: bool) -> Vec<Label>
{
   (0..count).map(|index| Label {
      text: template(template_text, index, if row_numbers {index as u32} else {0}),
      color: rgba(color), align: ui::elements::Align::Left, wrap: true, font_id, font_px,
   }).collect()
}

fn template(pattern: &str, index: usize, value: u32) -> String
{
   use std::fmt::Write;

   let index_width = decimal_width(index).max(2);
   let value_width = decimal_width(value as usize).max(4);
   let index_count = pattern.matches("%02d").count();
   let value_count = pattern.matches("%04d").count();
   let mut output = String::with_capacity(
      pattern.len().saturating_sub((index_count + value_count) * 4)
         + index_count * index_width + value_count * value_width,
   );
   let mut remaining = pattern;
   while let Some((offset, is_index)) = next_template_slot(remaining)
   {
      output.push_str(&remaining[..offset]);
      if is_index
      {
         let _ = write!(&mut output, "{index:0index_width$}");
      }
      else
      {
         let _ = write!(&mut output, "{value:0value_width$}");
      }
      remaining = &remaining[offset + 4..];
   }
   output.push_str(remaining);
   output
}

fn next_template_slot(pattern: &str) -> Option<(usize, bool)>
{
   match (pattern.find("%02d"), pattern.find("%04d"))
   {
      (Some(index), Some(value)) if index <= value => Some((index, true)),
      (Some(_), Some(value)) => Some((value, false)),
      (Some(index), None) => Some((index, true)),
      (None, Some(value)) => Some((value, false)),
      (None, None) => None,
   }
}

fn decimal_width(mut value: usize) -> usize
{
   let mut width = 1;
   while value >= 10
   {
      value /= 10;
      width += 1;
   }
   width
}

fn draw_text(suite: &mut Suite, time: f64, text: &mut TextCtx, renderer: &mut metal::MetalRenderer, builder: &mut ui::DrawListBuilder)
{
   let spec = &suite.fixture.text;
   let _ = advance(&mut suite.text_values, &mut suite.text_labels, &mut suite.last_text_step, time, spec.interval, &spec.template);
   let mut uploader = MtlUploader {renderer};
   for index in 0..suite.text_labels.len() {encode_text_unit(suite, index, text, &mut uploader, builder);}
}

fn draw_local(suite: &mut Suite, time: f64, text: &mut TextCtx, renderer: &mut metal::MetalRenderer, builder: &mut ui::DrawListBuilder)
{
   let spec = &suite.fixture.local;
   let _ = advance(&mut suite.local_values, &mut suite.local_labels, &mut suite.last_local_step, time, spec.interval, &spec.template);
   let mut uploader = MtlUploader {renderer};
   for index in 0..spec.count {encode_local_unit(suite, index, time, text, &mut uploader, builder);}
}

fn encode_text_unit(suite: &Suite, index: usize, text: &mut TextCtx, uploader: &mut MtlUploader, builder: &mut ui::DrawListBuilder)
{
   let spec = &suite.fixture.text;
   let x = spec.origin[0] + (index % spec.columns) as f32 * spec.stride[0];
   let y = spec.origin[1] + (index / spec.columns) as f32 * spec.stride[1];
   suite.text_labels[index].encode(gfx::RectF::new(x, y, spec.size[0], spec.size[1]), 3.0, text, uploader, builder);
}

fn encode_local_unit(suite: &Suite, index: usize, time: f64, text: &mut TextCtx, uploader: &mut MtlUploader, builder: &mut ui::DrawListBuilder)
{
   let spec = &suite.fixture.local;
   let origin = [spec.origin[0] + (index % spec.columns) as f32 * spec.stride[0], spec.origin[1] + (index / spec.columns) as f32 * spec.stride[1]];
   suite.local_labels[index].encode(gfx::RectF::new(origin[0], origin[1], spec.label_size[0], spec.label_size[1]), 3.0, text, uploader, builder);
   suite.buttons[index].encode(offset(origin, spec.button_rect), 3.0, text, uploader, &suite.button_states[index], builder);
   ui::elements::ProgressBar {
      value: Some((suite.local_values[index] % 100) as f32 / 100.0),
      track: rgba(spec.progress_track_color), fill: rgba(spec.progress_fill_color),
      corner: spec.progress_rect[3] * 0.5,
   }.encode(offset(origin, spec.progress_rect), time.max(0.0) as f32, builder);
}

fn prepare_retained_suite(runtime: &mut Runtime) -> Result<(), ()>
{
   let Runtime {renderer, builder, text, suite, ..} = runtime;
   let suite = suite.as_mut().ok_or(())?;
   let mut unit_builder = ui::DrawListBuilder::new();
   builder.clear();
   builder.rrect(gfx::RectF::new(0.0, 0.0, 390.0, 844.0), [0.0; 4], rgba([1.0; 4]));
   let background = gfx::RenderChunk::new(gfx::RenderChunkId(10_000), gfx::RenderChunkRevisions::default(), builder.drawlist().clone(), gfx::ChunkIndexMode::Local, &[]).map_err(|_| ())?;
   let count = match suite.case {Case::Text => suite.text_labels.len(), Case::Local => suite.local_labels.len(), _ => return Err(())};
   let mut units = Vec::with_capacity(count);
   let mut revisions = Vec::with_capacity(count);
   for index in 0..count
   {
      encode_retained_unit(suite, index, 0.0, text, renderer, &mut unit_builder);
      let chunk = gfx::RenderChunk::new(gfx::RenderChunkId(10_001 + index as u64), gfx::RenderChunkRevisions {structural: 1, ..Default::default()}, unit_builder.drawlist().clone(), gfx::ChunkIndexMode::Local, &[]).map_err(|_| ())?;
      units.push(gfx::RenderChunkInstance::new(chunk, [0.0; 2]));
      revisions.push(1);
   }
   suite.retained = Some(RetainedSuite {background, units, builder: unit_builder, revisions});
   Ok(())
}

fn encode_retained_unit(suite: &Suite, index: usize, time: f64, text: &mut TextCtx, renderer: &mut metal::MetalRenderer, builder: &mut ui::DrawListBuilder)
{
   builder.clear();
   text.begin_frame_at_scale(3.0);
   let mut uploader = MtlUploader {renderer};
   match suite.case
   {
      Case::Text => encode_text_unit(suite, index, text, &mut uploader, builder),
      Case::Local => encode_local_unit(suite, index, time, text, &mut uploader, builder),
      _ => unreachable!(),
   }
   text.finish_frame(&mut uploader, builder);
}

fn submit_retained_suite(runtime: &mut Runtime, drawable: *mut c_void, time: f64, timer: Option<native_metrics::NativeFrameTimer>) -> i32
{
   let Ok(snapshot) = retained_snapshot(runtime, time) else {return -8;};
   submit_snapshot(runtime, drawable, &snapshot, timer)
}

fn retained_snapshot(runtime: &mut Runtime, time: f64) -> Result<gfx::RenderSnapshot, ()>
{
   let Runtime {renderer, text, suite, ..} = runtime;
   let suite = suite.as_mut().unwrap();
   let advance = match suite.case
   {
      Case::Text => advance(&mut suite.text_values, &mut suite.text_labels, &mut suite.last_text_step, time, suite.fixture.text.interval, &suite.fixture.text.template),
      Case::Local => advance(&mut suite.local_values, &mut suite.local_labels, &mut suite.last_local_step, time, suite.fixture.local.interval, &suite.fixture.local.template),
      _ => return Err(()),
   };
   let mut retained = suite.retained.take().unwrap();
   if let Some(advance) = advance
   {
      let count = retained.units.len() as i64;
      let rebuild_all = advance.reset || advance.last - advance.first + 1 >= count;
      if rebuild_all
      {
         for index in 0..retained.units.len()
         {
            if rebuild_retained_unit(suite, &mut retained, index, time, text, renderer).is_err()
            {
               suite.retained = Some(retained);
               return Err(());
            }
         }
      }
      else
      {
         for step in advance.first..=advance.last
         {
            let index = step as usize % retained.units.len();
            if rebuild_retained_unit(suite, &mut retained, index, time, text, renderer).is_err()
            {
               suite.retained = Some(retained);
               return Err(());
            }
         }
      }
   }
   let mut instances = Vec::with_capacity(retained.units.len() + 1);
   instances.push(gfx::RenderChunkInstance::new(retained.background.clone(), [0.0; 2]));
   instances.extend(retained.units.iter().cloned());
   let snapshot = gfx::RenderSnapshot::new(instances, Vec::new(), gfx::Damage {rects: Vec::new()});
   suite.retained = Some(retained);
   snapshot.map_err(|_| ())
}

fn rebuild_retained_unit(suite: &Suite, retained: &mut RetainedSuite, index: usize, time: f64, text: &mut TextCtx, renderer: &mut metal::MetalRenderer) -> Result<(), ()>
{
   encode_retained_unit(suite, index, time, text, renderer, &mut retained.builder);
   retained.revisions[index] = retained.revisions[index].wrapping_add(1).max(1);
   let chunk = gfx::RenderChunk::new(gfx::RenderChunkId(10_001 + index as u64), gfx::RenderChunkRevisions {structural: retained.revisions[index], ..Default::default()}, retained.builder.drawlist().clone(), gfx::ChunkIndexMode::Local, &[]).map_err(|_| ())?;
   retained.units[index] = gfx::RenderChunkInstance::new(chunk, [0.0; 2]);
   Ok(())
}

fn image(image: &Image, rect: gfx::RectF, alpha: f32, builder: &mut ui::DrawListBuilder)
{
   builder.clip_push(rect_i([rect.x, rect.y, rect.w, rect.h]));
   ImageView {image: image.handle, natural_w: image.width, natural_h: image.height, fit: ImageFit::Cover, alpha}.encode(rect, None, builder);
   builder.clip_pop();
}

fn draw_images(suite: &Suite, time: f64, builder: &mut ui::DrawListBuilder)
{
   let spec = &suite.fixture.images_case;
   for index in 0..spec.count
   {
      let rect = gfx::RectF::new(spec.origin[0] + (index % spec.columns) as f32 * spec.stride[0], spec.origin[1] + (index / spec.columns) as f32 * spec.stride[1], spec.size[0], spec.size[1]);
      let phase = std::f64::consts::TAU * time.max(0.0) / spec.period + index as f64 * 0.3;
      let zoom = ui::elements::ImageZoomState {scale: 1.0 + spec.scale_amplitude * (phase.sin() as f32 + 1.0) * 0.5, offset: [phase.sin() as f32 * rect.w * 0.1, 0.0]};
      let source = &suite.images[index % suite.images.len()];
      builder.clip_push(rect_i([rect.x, rect.y, rect.w, rect.h]));
      ImageView {image: source.handle, natural_w: source.width, natural_h: source.height, fit: ImageFit::Cover, alpha: 1.0}.encode(rect, Some(&zoom), builder);
      builder.clip_pop();
   }
}

fn prepare_animation(runtime: &mut Runtime) -> Result<(), ()>
{
   let Runtime {renderer, builder, text, suite, ..} = runtime;
   let suite = suite.as_mut().unwrap();
   let spec = &suite.fixture.animation;
   let mut instances = Vec::with_capacity(spec.count + 1);
   builder.clear();
   builder.rrect(gfx::RectF::new(0.0, 0.0, 390.0, 844.0), [0.0; 4], rgba([1.0; 4]));
   let background = gfx::RenderChunk::new(gfx::RenderChunkId(1), gfx::RenderChunkRevisions::default(), builder.drawlist().clone(), gfx::ChunkIndexMode::Local, &[]).map_err(|_| ())?;
   instances.push(gfx::RenderChunkInstance::new(background, [0.0; 2]));
   for index in 0..spec.count
   {
      builder.clear();
      text.begin_frame_at_scale(3.0);
      builder.rrect(gfx::RectF::new(0.0, 0.0, spec.size[0], spec.size[1]), [8.0; 4], rgba([0.94, 0.95, 0.97, 1.0]));
      let source = &suite.images[index % suite.images.len()];
      image(source, offset([0.0; 2], spec.image_rect), 1.0, builder);
      let mut uploader = MtlUploader {renderer};
      suite.card_labels[index].encode(offset([0.0; 2], spec.label_rect), 3.0, text, &mut uploader, builder);
      text.finish_frame(&mut uploader, builder);
      let dependency = gfx::RenderResourceDependency {image: source.handle, generation: renderer.image_generation(source.handle).ok_or(())?};
      let chunk = gfx::RenderChunk::new(gfx::RenderChunkId(index as u64 + 2), gfx::RenderChunkRevisions::default(), builder.drawlist().clone(), gfx::ChunkIndexMode::Local, &[dependency]).map_err(|_| ())?;
      let mut instance = gfx::RenderChunkInstance::new(chunk, [0.0; 2]);
      instance.property_slots = Arc::from([gfx::RenderPropertySlotId(index as u32 * 2 + 1), gfx::RenderPropertySlotId(index as u32 * 2 + 2)]);
      instances.push(instance);
   }
   suite.animation_sequence = Some(gfx::RenderChunkSequence::new(instances));
   Ok(())
}

fn submit_animation(runtime: &mut Runtime, drawable: *mut c_void, time: f64, generation: u64, timer: Option<native_metrics::NativeFrameTimer>) -> i32
{
   let suite = runtime.suite.as_ref().unwrap();
   let spec = &suite.fixture.animation;
   let mut properties = Vec::with_capacity(spec.count * 2);
   for index in 0..spec.count
   {
      let phase = std::f64::consts::TAU * time.max(0.0) / spec.period + index as f64 * 0.3;
      let sin = phase.sin() as f32;
      let scale = spec.scale_center + spec.scale_amplitude * sin;
      let x = spec.origin[0] + (index % spec.columns) as f32 * spec.stride[0] + spec.translation[0] * sin + spec.size[0] * (1.0 - scale) * 0.5;
      let y = spec.origin[1] + (index / spec.columns) as f32 * spec.stride[1] + spec.translation[1] * phase.cos() as f32 + spec.size[1] * (1.0 - scale) * 0.5;
      properties.push(gfx::RenderPropertySlot {id: gfx::RenderPropertySlotId(index as u32 * 2 + 1), revision: generation, value: gfx::RenderPropertyValue::Transform([scale, 0.0, 0.0, scale, x, y])});
      properties.push(gfx::RenderPropertySlot {id: gfx::RenderPropertySlotId(index as u32 * 2 + 2), revision: generation, value: gfx::RenderPropertyValue::Opacity(spec.alpha_center + spec.alpha_amplitude * sin)});
   }
   // Public snapshots are immutable. Geometry/text chunks and their GPU buffers
   // stay retained; each frame supplies only the current transform/opacity slots.
   let snapshot = gfx::RenderSnapshot::from_sequences(vec![suite.animation_sequence.as_ref().unwrap().clone()], properties, gfx::Damage {rects: Vec::new()});
   let Ok(snapshot) = snapshot else {return -8;};
   submit_snapshot(runtime, drawable, &snapshot, timer)
}

struct ScrollMeasure {row_height: f32}

impl ui::collection::Measure for ScrollMeasure
{
   fn measure(&mut self, _: usize, _: f32) -> f32 {self.row_height}
   fn fixed_extent(&self, _: f32) -> Option<f32> {Some(self.row_height)}
}

struct ScrollRenderer<'a>
{
   fixture: &'a Fixture,
   images: &'a [Image],
   labels: &'a [Label],
   text: &'a mut TextCtx,
   renderer: &'a mut metal::MetalRenderer,
}

impl ui::collection::CellRenderer for ScrollRenderer<'_>
{
   fn render(&mut self, _: u32, index: usize, rect: gfx::RectF, _: bool, _: bool, builder: &mut ui::DrawListBuilder)
   {
      if index % 2 == 1 {builder.rrect(rect, [0.0; 4], rgba([0.94, 0.95, 0.97, 1.0]));}
      let spec = &self.fixture.scroll;
      image(&self.images[index % self.images.len()], offset([rect.x, rect.y], spec.image_rect), 1.0, builder);
      self.labels[index].encode(offset([rect.x, rect.y], spec.label_rect), 3.0, self.text, &mut MtlUploader {renderer: self.renderer}, builder);
   }
}

fn draw_scroll(suite: &mut Suite, time: f64, text: &mut TextCtx, renderer: &mut metal::MetalRenderer, builder: &mut ui::DrawListBuilder)
{
   let spec = &suite.fixture.scroll;
   // Repeat the established out-and-back path for the full timed workload.
   // Checkpoint callers retain their original timestamps and therefore pixels.
   let t = time.max(0.0).rem_euclid(spec.half_period * 2.0);
   let progress = if t <= spec.half_period {t / spec.half_period} else {(spec.half_period * 2.0 - t) / spec.half_period};
   suite.collection.set_count(spec.count);
   suite.collection.set_scroll(spec.distance * progress as f32);
   let mut measure = ScrollMeasure {row_height: spec.row_height};
   let mut cells = ScrollRenderer {fixture: &suite.fixture, images: &suite.images, labels: &suite.row_labels, text, renderer};
   suite.collection.layout_and_render(gfx::RectF::new(0.0, 0.0, 390.0, 844.0), &mut measure, &mut cells, builder);
}

// Fixture numbers are sRGB; renderer Color values remain linear.
fn rgba(value: [f32; 4]) -> gfx::Color {gfx::Color::from_srgba(value[0], value[1], value[2], value[3])}
fn rect_i(value: [f32; 4]) -> gfx::RectI {gfx::RectI::new(value[0].round() as i32, value[1].round() as i32, value[2].round() as i32, value[3].round() as i32)}
fn offset(origin: [f32; 2], value: [f32; 4]) -> gfx::RectF {gfx::RectF::new(origin[0] + value[0], origin[1] + value[1], value[2], value[3])}

fn decode_png(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), ()>
{
   let decoder = png::Decoder::new(Cursor::new(bytes));
   let mut reader = decoder.read_info().map_err(|_| ())?;
   let mut data = vec![0; reader.output_buffer_size()];
   let info = reader.next_frame(&mut data).map_err(|_| ())?;
   // These four versioned fixtures are generated RGBA8, with no decode in a run.
   if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {return Err(());}
   data.truncate(info.buffer_size());
   Ok((data, info.width, info.height))
}

#[cfg(test)]
mod native_api_tests
{
   use super::*;

   #[test]
   fn native_case_list_and_core_wakeups_match_the_host_contract()
   {
      assert_eq!(NATIVE_CASES.len(), 16);
      for name in NATIVE_CASES {assert!(Case::parse(name).is_some() || visual_boards::is_case(name));}
      assert_eq!(native_next_wakeup("text", 0.0), 0.1);
      assert_eq!(native_next_wakeup("local", 0.1), 0.2);
      assert_eq!(native_next_wakeup("shapes", 0.25), 0.25);
      assert!(native_next_wakeup("missing", 0.0).is_nan());
   }

   #[test]
   fn template_replaces_every_placeholder_without_changing_unicode()
   {
      assert_eq!(template("μ %04d / %02d / %04d", 7, 42), "μ 0042 / 07 / 0042");
      assert_eq!(template("é%02d🙂%04d", 123, 12_345), "é123🙂12345");
      assert_eq!(template("literal %0xd", 3, 9), "literal %0xd");
   }
}
