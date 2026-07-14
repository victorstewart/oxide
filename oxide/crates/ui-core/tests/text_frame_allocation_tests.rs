use oxide_renderer_api as api;
use oxide_ui_core::elements::{encode_label_text_profiled, Align, ImageUploader, TextCtx};
use oxide_ui_core::{
   bitmap_text::BitmapTextAtlas, draw_text_input_options_popover, text_input_options_layout,
   DrawListBuilder, TextInputOptionsConfig, TextInputOptionsPopoverStyle,
};
use oxide_wasm_alloc_counter::{snapshot, CountingAllocator};
use std::alloc::System;
use std::sync::Mutex;

#[global_allocator]
static ALLOCATOR: CountingAllocator<System> = CountingAllocator::new(System);
static TEST_LOCK: Mutex<()> = Mutex::new(());

#[derive(Default)]
struct Uploader
{
   next: u32,
}

#[derive(Default)]
struct Encoder
{
   commands: u64,
}

impl api::RenderEncoder for Encoder
{
   fn set_viewport(&mut self, _vp: api::RectF) {}

   fn set_clip(&mut self, _scissor: api::RectI) {}

   fn draw_solid(&mut self, _verts: &[api::Vertex], _color: api::Color)
   {
      self.commands = self.commands.wrapping_add(1);
   }

   fn draw_image(&mut self, _img: api::ImageHandle, _dst: api::RectF, _src: api::RectF) {}

   fn draw_rrect(&mut self, _rect: api::RectF, _radii: [f32; 4], _color: api::Color)
   {
      self.commands = self.commands.wrapping_add(1);
   }

   fn draw_nine_slice(
      &mut self,
      _img: api::ImageHandle,
      _rect: api::RectF,
      _slice: api::Insets,
      _alpha: f32,
   )
   {
   }

   fn draw_backdrop(&mut self, _rect: api::RectF, _sigma: f32, _tint: api::Color, _alpha: f32) {}

   fn draw_spinner(&mut self, _center: [f32; 2], _atom: f32, _alpha: f32) {}

   fn draw_glyph_run(&mut self, _run: &api::GlyphRun)
   {
      self.commands = self.commands.wrapping_add(1);
   }
}

impl ImageUploader for Uploader
{
   fn create_a8(&mut self, _w: u32, _h: u32, _data: &[u8], _row_bytes: usize) -> api::ImageHandle
   {
      self.next = self.next.wrapping_add(1).max(1);
      api::ImageHandle(self.next)
   }

   fn update_a8(
      &mut self,
      _handle: api::ImageHandle,
      _x: u32,
      _y: u32,
      _w: u32,
      _h: u32,
      _data: &[u8],
      _row_bytes: usize,
   )
   {
   }
}

fn encode_labels(
   labels: &[String],
   text: &mut TextCtx,
   uploader: &mut Uploader,
   builder: &mut DrawListBuilder,
)
{
   text.begin_frame();
   for (index, label) in labels.iter().enumerate()
   {
      encode_label_text_profiled(
         label,
         api::Color::rgba(0.1, 0.1, 0.1, 1.0),
         Align::Left,
         false,
         0,
         14.0,
         api::RectF::new(0.0, index as f32 * 18.0, 320.0, 18.0),
         2.0,
         text,
         uploader,
         builder,
      );
   }
   let _ = text.finish_frame(uploader, builder);
}

#[test]
fn warm_thousand_label_frame_is_allocation_free()
{
   let _guard = TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
   let mut text = TextCtx::default();
   text.set_frame_stats_enabled(true);
   let _ = text.fonts.add_font(oxide_text::Font::from_bytes(
      include_bytes!("../assets/Asap-Regular.ttf").to_vec(),
   ));
   let labels = (0..1_000).map(|index| format!("Warm label {index:04}")).collect::<Vec<_>>();
   let mut uploader = Uploader::default();
   let mut builder = DrawListBuilder::new();

   encode_labels(&labels, &mut text, &mut uploader, &mut builder);
   builder.clear();
   encode_labels(&labels, &mut text, &mut uploader, &mut builder);
   builder.clear();
   let before = snapshot();
   encode_labels(&labels, &mut text, &mut uploader, &mut builder);
   let after = snapshot();

   assert_eq!(
      after.alloc_count - before.alloc_count,
      0,
      "allocated_bytes={} dealloc_count={} deallocated_bytes={}",
      after.alloc_bytes - before.alloc_bytes,
      after.dealloc_count - before.dealloc_count,
      after.dealloc_bytes - before.dealloc_bytes,
   );
   assert_eq!(
      after.realloc_count - before.realloc_count,
      0,
      "realloc_grow_bytes={} realloc_shrink_bytes={}",
      after.realloc_grow_bytes - before.realloc_grow_bytes,
      after.realloc_shrink_bytes - before.realloc_shrink_bytes,
   );
   let stats = text.last_frame_stats();
   assert_eq!(stats.shaping_calls, 0);
   assert_eq!(stats.rasterizations, 0);
   assert_eq!(stats.atlas_upload_calls, 0);
}

#[test]
fn warm_text_input_options_are_allocation_free()
{
   let _guard = TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
   let layout = text_input_options_layout(
      api::RectF::new(260.0, 80.0, 120.0, 44.0),
      api::RectF::new(0.0, 0.0, 640.0, 480.0),
      1.0,
      TextInputOptionsConfig::all(),
      10.6,
   )
   .expect("option layout");
   let style = TextInputOptionsPopoverStyle {
      background: api::Color::rgba(0.01, 0.01, 0.01, 0.96),
      divider: api::Color::rgba(1.0, 1.0, 1.0, 0.78),
      text: api::Color::rgba(1.0, 1.0, 1.0, 0.96),
      text_px: 10.6,
   };
   let mut atlas = BitmapTextAtlas::new();
   atlas.set_handle(api::ImageHandle(1));
   let mut encoder = Encoder::default();
   for _ in 0..2
   {
      assert!(draw_text_input_options_popover(
         &mut encoder,
         &mut atlas,
         2.0,
         layout,
         style,
      ));
      encoder.commands = 0;
   }

   let before = snapshot();
   assert!(draw_text_input_options_popover(
      &mut encoder,
      &mut atlas,
      2.0,
      layout,
      style,
   ));
   let after = snapshot();

   assert_eq!(
      after.alloc_count - before.alloc_count,
      0,
      "allocated_bytes={} dealloc_count={} deallocated_bytes={}",
      after.alloc_bytes - before.alloc_bytes,
      after.dealloc_count - before.dealloc_count,
      after.dealloc_bytes - before.dealloc_bytes,
   );
   assert_eq!(
      after.realloc_count - before.realloc_count,
      0,
      "realloc_grow_bytes={} realloc_shrink_bytes={}",
      after.realloc_grow_bytes - before.realloc_grow_bytes,
      after.realloc_shrink_bytes - before.realloc_shrink_bytes,
   );
   assert_eq!(encoder.commands, 11);
   atlas.clear_dirty();
   assert!(atlas.dirty_rect().is_none());
}
