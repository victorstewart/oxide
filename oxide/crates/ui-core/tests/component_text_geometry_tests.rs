use oxide_renderer_api::{Color, DrawCmd, ImageHandle, RectF, RectI};
use oxide_ui_core::elements::{encode_label_text, Align, Button, ButtonState, ButtonStyle, ImageUploader, Label, TextCtx};
use oxide_ui_core::DrawListBuilder;

struct Uploader;
impl ImageUploader for Uploader
{
   fn create_a8(&mut self, _: u32, _: u32, _: &[u8], _: usize) -> ImageHandle {ImageHandle(1)}
   fn update_a8(&mut self, _: ImageHandle, _: u32, _: u32, _: u32, _: u32, _: &[u8], _: usize) {}
}

fn context(bytes: &[u8]) -> TextCtx
{
   let mut text = TextCtx::default();
   text.fonts.add_font(oxide_text::Font::from_bytes(bytes.to_vec()));
   text
}

fn ink_bounds(builder: &DrawListBuilder) -> RectF
{
   let vertices = &builder.drawlist().vertices;
   assert!(!vertices.is_empty(), "expected visible title glyphs");
   let x = vertices.iter().map(|v| v.x).fold(f32::INFINITY, f32::min);
   let y = vertices.iter().map(|v| v.y).fold(f32::INFINITY, f32::min);
   let right = vertices.iter().map(|v| v.x).fold(f32::NEG_INFINITY, f32::max);
   let bottom = vertices.iter().map(|v| v.y).fold(f32::NEG_INFINITY, f32::max);
   RectF::new(x, y, right - x, bottom - y)
}

#[test]
fn labels_use_top_boxes_while_legacy_helpers_keep_baselines()
{
   for font in [include_bytes!("../assets/Asap-Regular.ttf").as_slice(), include_bytes!("../../text/tests/fixtures/NotoSans-VF.ttf").as_slice()]
   {
      for scale in [1.0, 2.0, 3.0]
      {
         let mut text = context(font);
         let label = Label {text: "Hg".into(), font_px: 18.0, ..Label::default()};
         let rect = RectF::new(20.0, 40.0, 160.0, 60.0);
         let mut builder = DrawListBuilder::new();
         label.encode(rect, scale, &mut text, &mut Uploader, &mut builder);
         let top = ink_bounds(&builder);
         assert!(top.y >= rect.y - 1.0 / scale, "glyph ascenders escaped top: {top:?}");
         assert!(top.y + top.h < rect.y + rect.h);
         builder.clear();
         encode_label_text("Hg", Color::rgba(0.0, 0.0, 0.0, 1.0), Align::Left, false, 0, 18.0, rect, scale, &mut text, &mut Uploader, &mut builder);
         let baseline = ink_bounds(&builder);
         assert!(baseline.y < rect.y, "legacy baseline convention changed");
         assert!((baseline.w - top.w).abs() < 0.001);
      }
   }
}

#[test]
fn button_title_is_centered_inside_background_and_padding_owns_clip()
{
   let mut text = context(include_bytes!("../assets/Asap-Regular.ttf"));
   let button = Button {text: "Hgy".into(), style: ButtonStyle {text_px: 14.0, pad_x: 8.0, pad_y: 6.0, ..ButtonStyle::default()}};
   let mut builder = DrawListBuilder::new();
   let rect = RectF::new(10.0, 40.0, 100.0, 40.0);
   let parent = RectI::new(0, 0, 200, 200);
   builder.clip_push(parent);
   button.encode(rect, 3.0, &mut text, &mut Uploader, &ButtonState::default(), &mut builder);
   let ink = ink_bounds(&builder);
   assert!(ink.y >= rect.y + 6.0 && ink.y + ink.h <= rect.y + rect.h - 6.0);
   assert!((ink.x + ink.w / 2.0 - 60.0).abs() < 2.0);
   assert!((ink.y + ink.h / 2.0 - 60.0).abs() < 4.0);
   let clips: Vec<_> = builder.drawlist().items.iter().filter_map(|cmd| match cmd {DrawCmd::ClipPush {rect} => Some(*rect), _ => None}).collect();
   assert_eq!(clips, [parent, RectI::new(18, 46, 84, 28)]);
   assert!(matches!(builder.drawlist().items.last(), Some(DrawCmd::ClipPop)));
   builder.clip_pop();
}

#[test]
fn excessive_padding_leaves_background_without_overflowing_title()
{
   let mut text = context(include_bytes!("../assets/Asap-Regular.ttf"));
   let button = Button {text: "Title".into(), style: ButtonStyle {pad_y: 30.0, ..ButtonStyle::default()}};
   let mut builder = DrawListBuilder::new();
   button.encode(RectF::new(10.0, 40.0, 100.0, 40.0), 3.0, &mut text, &mut Uploader, &ButtonState::default(), &mut builder);
   assert!(builder.drawlist().vertices.is_empty());
   assert!(matches!(builder.drawlist().items.as_slice(), [DrawCmd::RRect {..}]));
}
