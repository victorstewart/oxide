use serde_json::Value;

use oxide_renderer_api as gfx;
use oxide_renderer_metal as metal;
use oxide_ui_core as ui;
use ui::elements::{Align, ImageFit, ImageRegionView, ImageView, Label, NineSliceImage, TextCtx};

use super::{decode_png, MtlUploader};
use super::visual_boards::VisualChunk;

const IMAGE_A: &[u8] = include_bytes!("../../fixtures/visual-alpha-a.png");
const IMAGE_B: &[u8] = include_bytes!("../../fixtures/visual-alpha-b.png");
const NINE: &[u8] = include_bytes!("../../fixtures/visual-nine.png");

pub(super) struct VisualExtended
{
   pub(super) chunks: Vec<VisualChunk>,
   fixture: Value,
   font_id: usize,
   image: gfx::ImageHandle,
   nine: gfx::ImageHandle,
   replacement: gfx::ImageHandle,
   replacement_stage: usize,
   pixels: [Vec<u8>; 2],
   dimensions: [u32; 2],
}

impl VisualExtended
{
   pub(super) fn new(font_id: usize, renderer: &mut metal::MetalRenderer) -> Result<Self, ()>
   {
      let (a, width, height) = decode_png(IMAGE_A)?;
      let (b, bw, bh) = decode_png(IMAGE_B)?;
      if [width, height] != [bw, bh] {return Err(());}
      let (nine, nw, nh) = decode_png(NINE)?;
      let image = renderer.image_create_rgba8(width, height, &a, width as usize * 4);
      let replacement = renderer.image_create_rgba8(width, height, &a, width as usize * 4);
      let nine = renderer.image_create_rgba8(nw, nh, &nine, nw as usize * 4);
      if [image.0, replacement.0, nine.0].contains(&0) {return Err(());}
      Ok(Self {chunks: Vec::new(), fixture: serde_json::from_str(include_str!("../../fixtures/visual.json")).map_err(|_| ())?, font_id,
         image, nine, replacement, replacement_stage: 0, pixels: [a, b], dimensions: [width, height]})
   }

   pub(super) fn draw(&mut self, name: &str, stage: usize, text: &mut TextCtx, renderer: &mut metal::MetalRenderer, builder: &mut ui::DrawListBuilder)
   {
      self.chunks.clear();
      match name
      {
         "visual-opacity" => self.opacity(stage, builder),
         "visual-images" => self.images(stage, text, renderer, builder),
         "visual-geometry" => self.geometry(stage, text, renderer, builder),
         _ => {}
      }
   }

   pub(super) fn resources(&self, name: &str, stage: usize, renderer: &metal::MetalRenderer) -> Vec<gfx::RenderResourceDependency>
   {
      if name != "visual-images" {return Vec::new();}
      [self.image, self.nine, self.replacement].into_iter().take(if stage < 2 {3} else {2}).map(|image| gfx::RenderResourceDependency {image, generation: renderer.image_generation(image).expect("uploaded image")}).collect()
   }

   fn opacity(&self, stage: usize, builder: &mut ui::DrawListBuilder)
   {
      let spec = &self.fixture["boards"]["visual-opacity"];
      for panel in spec["panels"].as_array().unwrap() {builder.rrect(rect(panel), [0.0; 4], palette(&self.fixture, "panel"));}
      let colors = &spec["colors"];
      let rectangles = &spec["rectangles"];
      builder.layer_begin_with_opacity(800, rect(&spec["panels"][0]), false, number(&spec["group_alpha"][stage]));
      builder.rrect(rect(&rectangles[0]), [0.0; 4], color(&colors[0]));
      builder.rrect(rect(&rectangles[1]), [0.0; 4], color(&colors[1]));
      builder.layer_end();
      builder.layer_begin_with_opacity(801, rect(&spec["panels"][1]), false, number(&spec["outer_alpha"][stage]));
      builder.rrect(rect(&rectangles[2]), [0.0; 4], color(&colors[2]));
      builder.layer_begin_with_opacity(802, gfx::RectF::new(100.0, 350.0, 230.0, 120.0), false, number(&spec["inner_alpha"]));
      builder.rrect(rect(&rectangles[3]), [0.0; 4], color(&colors[0]));
      builder.rrect(rect(&rectangles[4]), [0.0; 4], color(&colors[1]));
      builder.layer_end();
      builder.layer_end();
      builder.layer_begin_with_opacity(803, rect(&spec["panels"][2]), false, number(&spec["restore_alpha"][stage]));
      builder.rrect(rect(&rectangles[5]), [0.0; 4], color(&colors[0]));
      builder.rrect(rect(&rectangles[6]), [0.0; 4], color(&colors[1]));
      builder.layer_end();
   }

   fn images(&mut self, stage: usize, text: &mut TextCtx, renderer: &mut metal::MetalRenderer, builder: &mut ui::DrawListBuilder)
   {
      let spec = &self.fixture["boards"]["visual-images"];
      if stage < 2 && self.replacement_stage != stage
      {
         renderer.image_update_rgba8(self.replacement, 0, 0, self.dimensions[0], self.dimensions[1], &self.pixels[stage], self.dimensions[0] as usize * 4);
         self.replacement_stage = stage;
      }
      for (index, fit) in [ImageFit::Contain, ImageFit::Cover, ImageFit::Stretch].into_iter().enumerate()
      {
         let bounds = rect(&spec["fit_rects"][index]);
         builder.rrect(bounds, [0.0; 4], palette(&self.fixture, "panel"));
         self.image_view(self.image, fit).encode(bounds, None, builder);
      }
      for index in 0..2
      {
         let bounds = rect(&spec["alpha_rects"][index]);
         builder.rrect(bounds, [0.0; 4], if index == 0 {gfx::Color::from_srgba(1.0, 1.0, 1.0, 1.0)} else {palette(&self.fixture, "text")});
         self.image_view(self.image, ImageFit::Stretch).encode(bounds, None, builder);
      }
      ImageRegionView {image: self.image, source: rect(&spec["crop_source"]), fit: ImageFit::Stretch, alpha: 1.0}.encode(rect(&spec["crop_rects"][stage]), builder);
      let cap = number(&spec["slice"]);
      NineSliceImage {tex: self.nine, slice: gfx::Insets::new(cap, cap, cap, cap), alpha: 1.0}.encode(rect(&spec["nine_rects"][stage]), builder);
      let replacement_rect = rect(&spec["replacement_rect"]);
      builder.rrect(replacement_rect, [0.0; 4], palette(&self.fixture, "panel"));
      if stage < 2 {self.image_view(self.replacement, ImageFit::Stretch).encode(replacement_rect, None, builder);}
      for (index, y) in [54.0, 220.0, 394.0, 594.0].into_iter().enumerate()
      {
         self.label(spec["captions"][index].as_str().unwrap(), 13.0).encode(gfx::RectF::new(20.0, y, 350.0, 24.0), 3.0, text, &mut MtlUploader {renderer}, builder);
      }
   }

   fn geometry(&mut self, stage: usize, text: &mut TextCtx, renderer: &mut metal::MetalRenderer, builder: &mut ui::DrawListBuilder)
   {
      let spec = &self.fixture["boards"]["visual-geometry"];
      let offset = number(&spec["offsets"][stage]);
      let border = number(&spec["border_width"]);
      for index in 0..2
      {
         let mut bounds = rect(&spec["border_rects"][index]);
         bounds.x += offset;
         bounds.y += offset;
         builder.rrect(bounds, [0.0; 4], palette(&self.fixture, if index == 0 {"blue"} else {"text"}));
         builder.rrect(gfx::RectF::new(bounds.x + border, bounds.y + border, bounds.w - border * 2.0, bounds.h - border * 2.0), [0.0; 4], gfx::Color::from_srgba(1.0, 1.0, 1.0, 1.0));
         let mut bounds = rect(&spec["rrects"][index]);
         bounds.x += offset;
         bounds.y += offset;
         let radii = core::array::from_fn(|corner| number(&spec["radii"][index][corner]));
         builder.rrect(bounds, radii, palette(&self.fixture, if index == 0 {"blue"} else {"green"}));
      }
      for index in 0..3
      {
         self.label(spec["texts"][index].as_str().unwrap(), number(&spec["font_sizes"][index])).encode(rect(&spec["text_rects"][index]), 3.0, text, &mut MtlUploader {renderer}, builder);
      }
      let bounds = rect(&spec["scaled_rect"]);
      let scale = number(&spec["text_scales"][stage]);
      let mut local = ui::DrawListBuilder::new();
      self.label(spec["scaled_text"].as_str().unwrap(), number(&spec["scaled_font_px"])).encode(gfx::RectF::new(0.0, 0.0, bounds.w, bounds.h), 3.0, text, &mut MtlUploader {renderer}, &mut local);
      let mut chunk = VisualChunk::new(local, 810, stage as u64);
      chunk.slots.push(gfx::RenderPropertySlotId(8100));
      chunk.properties.push(gfx::RenderPropertySlot {id: gfx::RenderPropertySlotId(8100), revision: stage as u64,
         value: gfx::RenderPropertyValue::Transform([scale, 0.0, 0.0, scale, bounds.x, bounds.y])});
      self.chunks.push(chunk);
   }

   fn image_view(&self, image: gfx::ImageHandle, fit: ImageFit) -> ImageView
   {
      ImageView {image, natural_w: self.dimensions[0], natural_h: self.dimensions[1], fit, alpha: 1.0}
   }

   fn label(&self, value: &str, font_px: f32) -> Label
   {
      Label {text: value.into(), font_id: self.font_id, font_px, color: palette(&self.fixture, "text"), align: Align::Left, wrap: false}
   }
}

fn number(value: &Value) -> f32 {value.as_f64().unwrap() as f32}
fn rect(value: &Value) -> gfx::RectF {gfx::RectF::new(number(&value[0]), number(&value[1]), number(&value[2]), number(&value[3]))}
fn color(value: &Value) -> gfx::Color {gfx::Color::from_srgba(number(&value[0]), number(&value[1]), number(&value[2]), number(&value[3]))}
fn palette(fixture: &Value, name: &str) -> gfx::Color {color(&fixture["palette"][name])}
