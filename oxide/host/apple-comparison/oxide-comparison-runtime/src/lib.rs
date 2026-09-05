//! Small iOS presentation-measurement probe using the production Metal renderer.

use std::sync::{Mutex, OnceLock};

use oxide_renderer_api as gfx;
use oxide_renderer_api::Renderer;
use oxide_renderer_metal as metal;
use oxide_ui_core as ui;

struct Runtime
{
   renderer: metal::MetalRenderer,
   builder: ui::DrawListBuilder,
   coalesced: Vec<gfx::DrawCmd>,
}

static RUNTIME: OnceLock<Mutex<Option<Runtime>>> = OnceLock::new();

#[no_mangle]
pub extern "C" fn oxide_core_init() -> i32
{
   let mut slot = RUNTIME.get_or_init(|| Mutex::new(None)).lock().unwrap();
   if slot.is_some() {return 0;}
   let Ok(mut renderer) = metal::MetalRenderer::new_with_config(metal::MetalRendererConfig::visible_host()) else {return -1;};
   if renderer.resize(1_170, 2_532, 3.0).is_err() {return -2;}
   *slot = Some(Runtime {renderer, builder: ui::DrawListBuilder::new(), coalesced: Vec::new()});
   0
}

/// The caller supplies a live drawable and matching tile state on the main thread.
#[no_mangle]
pub unsafe extern "C" fn oxide_core_draw(drawable: *mut std::ffi::c_void, values: *const f32, count: usize) -> i32
{
   if drawable.is_null() || values.is_null() || count != 64 {return -1;}
   let values = unsafe {std::slice::from_raw_parts(values, count)};
   if values.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v)) {return -2;}
   let mut slot = RUNTIME.get_or_init(|| Mutex::new(None)).lock().unwrap();
   let Some(runtime) = slot.as_mut() else {return -3;};
   runtime.builder.clear();
   runtime.builder.rrect(gfx::RectF::new(0.0, 0.0, 390.0, 844.0), [0.0; 4], gfx::Color::rgba(1.0, 1.0, 1.0, 1.0));
   for (index, value) in values.iter().enumerate()
   {
      let x = 15.0 + (index % 8) as f32 * 45.0;
      let y = 22.0 + (index / 8) as f32 * 100.0;
      runtime.builder.rrect(gfx::RectF::new(x, y, 36.0, 80.0), [6.0; 4], gfx::Color::rgba(*value, 0.25, 0.5, 1.0));
   }
   ui::coalesce_adjacent_draws_reuse(runtime.builder.drawlist_mut(), &mut runtime.coalesced);
   if unsafe {runtime.renderer.prepare_present_drawable(drawable.cast())}.is_err() {return -4;}
   let token = runtime.renderer.begin_frame(&gfx::FrameTarget, None);
   runtime.renderer.encode_pass(runtime.builder.drawlist());
   if runtime.renderer.submit(token).is_err() {return -5;}
   0
}
