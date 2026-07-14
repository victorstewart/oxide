use oxide_renderer_api as api;
use oxide_ui_core::elements::{encode_label_text_profiled, Align, ImageUploader, TextCtx};
use oxide_ui_core::DrawListBuilder;
use oxide_wasm_alloc_counter::{snapshot, CountingAllocator};
use std::alloc::System;

#[global_allocator]
static ALLOCATOR: CountingAllocator<System> = CountingAllocator::new(System);

#[derive(Default)]
struct Uploader
{
   next: u32,
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

   assert_eq!(after.alloc_count - before.alloc_count, 0);
   assert_eq!(after.realloc_count - before.realloc_count, 0);
   let stats = text.last_frame_stats();
   assert_eq!(stats.shaping_calls, 0);
   assert_eq!(stats.rasterizations, 0);
   assert_eq!(stats.atlas_upload_calls, 0);
}
