use font8x8::{UnicodeFonts, BASIC_FONTS};
use oxide_renderer_api::{
   Color, GlyphRun, ImageHandle, Insets, RectF, RectI, RenderEncoder, Vertex,
};
use oxide_ui_core::bitmap_text::{
   draw_text, draw_text_aligned, line_height, resolve_text_with_placeholder, text_width,
   text_width_pixel_snapped, text_width_spans, TextAlign, TextSpan, TextStyle,
};

#[derive(Default)]
struct CollectingEncoder {
   rects: Vec<RectF>,
}

impl RenderEncoder for CollectingEncoder {
   fn set_viewport(&mut self, _vp: RectF) {}

   fn set_clip(&mut self, _scissor: RectI) {}

   fn draw_solid(&mut self, verts: &[Vertex], _color: Color) {
      if verts.is_empty() {
         return;
      }
      let mut min_x = f32::INFINITY;
      let mut min_y = f32::INFINITY;
      let mut max_x = f32::NEG_INFINITY;
      let mut max_y = f32::NEG_INFINITY;
      for v in verts {
         min_x = min_x.min(v.x);
         min_y = min_y.min(v.y);
         max_x = max_x.max(v.x);
         max_y = max_y.max(v.y);
      }
      self.rects.push(RectF::new(
         min_x,
         min_y,
         (max_x - min_x).max(0.0),
         (max_y - min_y).max(0.0),
      ));
   }

   fn draw_image(&mut self, _img: ImageHandle, _dst: RectF, _src: RectF) {}

   fn draw_rrect(&mut self, _rect: RectF, _radii: [f32; 4], _color: Color) {}

   fn draw_nine_slice(&mut self, _img: ImageHandle, _rect: RectF, _slice: Insets, _alpha: f32) {}

   fn draw_backdrop(&mut self, _rect: RectF, _sigma: f32, _tint: Color, _alpha: f32) {}

   fn draw_spinner(&mut self, _center: [f32; 2], _atom: f32, _alpha: f32) {}

   fn draw_glyph_run(&mut self, _run: &GlyphRun) {}
}

#[test]
fn draw_text_uses_lsb_left_to_right_bit_order() {
   let style = TextStyle::new(3.0, Color::rgba(1.0, 1.0, 1.0, 1.0));
   let mut encoder = CollectingEncoder::default();
   draw_text(&mut encoder, "\\", 10.0, 20.0, style);
   assert!(!encoder.rects.is_empty(), "expected bitmap rect draws for glyph");

   let glyph = BASIC_FONTS.get('\\').expect("glyph backslash");
   let (row_index, row_bits) = glyph
      .iter()
      .copied()
      .enumerate()
      .find(|(_, bits)| {
         let lsb_left = (0..8).find(|col| ((bits >> col) & 1) == 1);
         let msb_left = (0..8).find(|col| ((bits >> (7 - col)) & 1) == 1);
         lsb_left.is_some() && lsb_left != msb_left
      })
      .expect("row with orientation-sensitive bits");
   let expected_left_col = (0..8).find(|col| ((row_bits >> col) & 1) == 1).expect("lit row");
   let row_y = 20.0 + row_index as f32;
   let mut min_x = f32::INFINITY;
   for rect in &encoder.rects {
      if (rect.y - row_y).abs() < 0.001 {
         min_x = min_x.min(rect.x);
      }
   }
   assert!(min_x.is_finite(), "expected at least one run on first row");
   assert!((min_x - (10.0 + expected_left_col as f32)).abs() < 0.001);
}

#[test]
fn small_asap_text_uses_smooth_widths() {
   let style = TextStyle::new(5.25, Color::rgba(1.0, 1.0, 1.0, 1.0));
   assert!(text_width("followers", style) < 40.0);
   assert!((line_height(style) - 6.3).abs() < 0.001);
}

#[test]
fn smooth_text_width_includes_trailing_advance() {
   let style = TextStyle::new(12.0, Color::rgba(1.0, 1.0, 1.0, 1.0));
   assert!(text_width("scope    ", style) > text_width("scope", style));
}

#[test]
fn pixel_snapped_text_width_keeps_trailing_advance() {
   let style = TextStyle::new(7.0, Color::rgba(1.0, 1.0, 1.0, 1.0));
   assert!(text_width_pixel_snapped("500    ", style) > text_width_pixel_snapped("500", style));
}

#[test]
fn text_width_spans_matches_sum_of_segment_widths() {
   let style_a = TextStyle::new(5.0, Color::rgba(1.0, 1.0, 1.0, 1.0));
   let style_b = TextStyle::new(5.0, Color::rgba(0.8, 0.8, 0.8, 1.0)).bold();
   let spans = [TextSpan::new("first ", style_a), TextSpan::new("name", style_b)];
   let expected = text_width("first ", style_a) + text_width("name", style_b);
   assert!((text_width_spans(&spans) - expected).abs() < 0.001);
}

#[test]
fn resolve_text_with_placeholder_prefers_placeholder_when_empty() {
   let text_style = TextStyle::new(5.0, Color::rgba(1.0, 1.0, 1.0, 1.0));
   let placeholder_style = TextStyle::new(5.0, Color::rgba(0.6, 0.6, 0.6, 1.0)).italic();
   let span = resolve_text_with_placeholder("", "username", text_style, placeholder_style);
   assert_eq!(span, TextSpan::new("username", placeholder_style));
}

#[test]
fn resolve_text_with_placeholder_prefers_entered_text_when_present() {
   let text_style = TextStyle::new(5.0, Color::rgba(1.0, 1.0, 1.0, 1.0)).bold();
   let placeholder_style = TextStyle::new(5.0, Color::rgba(0.6, 0.6, 0.6, 1.0));
   let span = resolve_text_with_placeholder("victor", "username", text_style, placeholder_style);
   assert_eq!(span, TextSpan::new("victor", text_style));
}

#[test]
fn aligned_draw_centers_and_right_aligns_width() {
   let style = TextStyle::new(3.0, Color::rgba(1.0, 1.0, 1.0, 1.0));
   let rect = RectF::new(12.0, 20.0, 200.0, 24.0);
   let width = text_width("A", style);

   let mut centered = CollectingEncoder::default();
   draw_text_aligned(&mut centered, "A", rect, TextAlign::Center, style);
   let center_min_x = centered.rects.iter().map(|r| r.x).fold(f32::INFINITY, f32::min);
   assert!((center_min_x - (12.0 + (200.0 - width) * 0.5)).abs() < 0.001);

   let mut right = CollectingEncoder::default();
   draw_text_aligned(&mut right, "A", rect, TextAlign::Right, style);
   let right_min_x = right.rects.iter().map(|r| r.x).fold(f32::INFINITY, f32::min);
   assert!((right_min_x - (12.0 + 200.0 - width)).abs() < 0.001);
}
