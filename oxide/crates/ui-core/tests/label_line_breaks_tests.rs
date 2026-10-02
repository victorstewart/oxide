use oxide_renderer_api::{Color, ImageHandle, RectF};
use oxide_ui_core::elements::{encode_label_text, Align, ImageUploader, TextCtx};
use oxide_ui_core::DrawListBuilder;

struct TestUploader
{
   creates: usize,
   updates: usize,
}

impl ImageUploader for TestUploader
{
   fn create_a8(&mut self, _w: u32, _h: u32, _data: &[u8], _row_bytes: usize) -> ImageHandle
   {
      self.creates += 1;
      ImageHandle(1)
   }

   fn update_a8(&mut self, _handle: ImageHandle, _x: u32, _y: u32, _w: u32, _h: u32, _data: &[u8], _row_bytes: usize)
   {
      self.updates += 1;
   }
}

fn label_vertical_span(text_value: &str, wrap: bool) -> f32
{
   let mut text = TextCtx::default();
   let font_id = text.fonts.add_font(oxide_text::Font::from_bytes(
      include_bytes!("../assets/Asap-Regular.ttf").to_vec(),
   ));
   let mut uploader = TestUploader { creates: 0, updates: 0 };
   let mut builder = DrawListBuilder::new();
   encode_label_text(
      text_value,
      Color::rgba(0.1, 0.1, 0.1, 1.0),
      Align::Left,
      wrap,
      font_id,
      14.0,
      RectF::new(0.0, 40.0, 240.0, 160.0),
      2.0,
      &mut text,
      &mut uploader,
      &mut builder,
   );
   let min_y = builder.drawlist().vertices.iter().map(|vertex| vertex.y).fold(f32::INFINITY, f32::min);
   let max_y = builder.drawlist().vertices.iter().map(|vertex| vertex.y).fold(f32::NEG_INFINITY, f32::max);
   assert!(min_y.is_finite() && max_y.is_finite());
   max_y - min_y
}

#[test]
fn label_lf_forces_lines_with_and_without_wrap()
{
   let one_line = label_vertical_span("Top Bottom", false);
   for wrap in [false, true]
   {
      let forced_lines = label_vertical_span("Top\nBottom", wrap);
      assert!(forced_lines > one_line + 10.0, "wrap={wrap} span={forced_lines}");
   }
}

#[test]
fn label_crlf_matches_lf_line_geometry()
{
   for wrap in [false, true]
   {
      let lf = label_vertical_span("Top\nBottom", wrap);
      let crlf = label_vertical_span("Top\r\nBottom", wrap);
      assert!((lf - crlf).abs() <= 0.001, "wrap={wrap} lf={lf} crlf={crlf}");
   }
}

#[test]
fn label_blank_line_advances_one_extra_line()
{
   for wrap in [false, true]
   {
      let adjacent_lines = label_vertical_span("Top\nBottom", wrap);
      let blank_line = label_vertical_span("Top\n\nBottom", wrap);
      assert!(blank_line > adjacent_lines + 10.0, "wrap={wrap} adjacent={adjacent_lines} blank={blank_line}");
   }
}
