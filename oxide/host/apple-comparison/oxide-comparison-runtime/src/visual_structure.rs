//! Structural visual boards: retained composition and keyed, measured layout.


use serde_json::Value;

use oxide_renderer_api as gfx;
use oxide_renderer_metal as metal;
use oxide_ui_core as ui;
use ui::elements::{Align, ImageFit, ImageView, Label, TextCtx};

use super::MtlUploader;
use super::visual_boards::VisualChunk;

const FIXTURE: &str = include_str!("../../fixtures/visual.json");
const SCALE: f32 = 3.0;

pub(super) struct VisualStructure
{
   pub(super) chunks: Vec<VisualChunk>,
   fixture: Value,
   font_id: usize,
   images: Vec<(gfx::ImageHandle, u32, u32)>,
   layout_tree: ui::NodeTree,
   layout_containers: [ui::NodeId; 2],
   layout_labels: [ui::NodeId; 2],
   layout_buttons: [ui::NodeId; 2],
   collection: ui::collection::CollectionView,
   keys: Vec<u64>,
   list_stage: Option<usize>,
   composition_stage: Option<usize>,
}

impl VisualStructure
{
   pub(super) fn new(font_id: usize, images: Vec<(gfx::ImageHandle, u32, u32)>) -> Self
   {
      let fixture = serde_json::from_str(FIXTURE).expect("visual fixture must be valid JSON");
      let (layout_tree, layout_containers, layout_labels, layout_buttons) = make_layout_tree();
      Self {
         chunks: Vec::new(), fixture, font_id, images, layout_tree, layout_containers, layout_labels, layout_buttons,
         collection: ui::collection::CollectionView::new(ui::collection::CollectionMode::VerticalGrid {col_width: 350.0, spacing: 4.0}),
         keys: (0..24).collect(), list_stage: None, composition_stage: None,
      }
   }

   pub(super) fn draw(&mut self, name: &str, stage: usize, text: &mut TextCtx, renderer: &mut metal::MetalRenderer, builder: &mut ui::DrawListBuilder)
   {
      self.chunks.clear();
      match name
      {
         "visual-composition" => self.draw_composition(stage.min(2), text, renderer, builder),
         "visual-layout" => self.draw_layout(stage.min(2), text, renderer, builder),
         _ => {}
      }
   }

   fn draw_composition(&mut self, stage: usize, text: &mut TextCtx, renderer: &mut metal::MetalRenderer, builder: &mut ui::DrawListBuilder)
   {
      let spec = board(&self.fixture, "visual-composition");
      let outer = rect(&spec["outer"]);
      let inner = rect(&spec["inner"]);
      builder.rrect(outer, [0.0; 4], color(&self.fixture, "panel"));
      builder.rrect(inner, [0.0; 4], gfx::Color::from_srgba(1.0, 1.0, 1.0, 1.0));


      let mut local = ui::DrawListBuilder::new();
      if stage == 1
      {
         local.rrect(rect(&spec["tile_b"]), [0.0; 4], rgba_value(&spec["colors"][1]));
         local.rrect(rect(&spec["tile_a"]), [0.0; 4], rgba_value(&spec["colors"][0]));
      }
      else
      {
         if stage != 2 {local.rrect(rect(&spec["tile_a"]), [0.0; 4], rgba_value(&spec["colors"][0]));}
         local.rrect(rect(&spec["tile_b"]), [0.0; 4], rgba_value(&spec["colors"][1]));
      }
      if let Some((handle, width, height)) = self.images.first().copied()
      {
         ImageView {image: handle, natural_w: width, natural_h: height, fit: ImageFit::Cover, alpha: 1.0}.encode(rect(&spec["image"]), None, &mut local);
         let mut uploader = MtlUploader {renderer};
         label(spec["label_text"].as_str().unwrap(), self.font_id, 18.0, rect(&spec["label"]), text, &mut uploader, &mut local);
         let mut chunk = VisualChunk::new(local, 90, stage as u64);
         chunk.resources.push(gfx::RenderResourceDependency {image: handle, generation: renderer.image_generation(handle).expect("uploaded image")});
         chunk.clips = vec![gfx::RenderDynamicClip {rect: outer, transform: gfx::RenderPropertySlotId(903)}, gfx::RenderDynamicClip {rect: inner, transform: gfx::RenderPropertySlotId(903)}];
         chunk.slots = vec![gfx::RenderPropertySlotId(901), gfx::RenderPropertySlotId(902)];
         let transform = &spec["transforms"][stage];
         chunk.properties = vec![
            gfx::RenderPropertySlot {id: gfx::RenderPropertySlotId(901), revision: stage as u64, value: gfx::RenderPropertyValue::Transform([number(&transform[2]), 0.0, 0.0, number(&transform[2]), number(&transform[0]), number(&transform[1])])},
            gfx::RenderPropertySlot {id: gfx::RenderPropertySlotId(902), revision: stage as u64, value: gfx::RenderPropertyValue::Opacity(1.0)},
            gfx::RenderPropertySlot {id: gfx::RenderPropertySlotId(903), revision: 0, value: gfx::RenderPropertyValue::Transform([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])},
         ];
         self.chunks.push(chunk);
      }

      let retained = rect(&spec["retained_rect"]);
      let dirty = self.composition_stage != Some(stage);
      self.composition_stage = Some(stage);
      let mut layer = ui::DrawListBuilder::new();
      layer.rrect(retained, [12.0; 4], color(&self.fixture, "panel"));
      if stage != 2 {layer.rrect(rect(&spec["retained_tile"]), [10.0; 4], color(&self.fixture, "blue"));}
      let mut uploader = MtlUploader {renderer};
      label(spec["retained_strings"][stage].as_str().unwrap(), self.font_id, 18.0, rect(&spec["retained_text"]), text, &mut uploader, &mut layer);
      let mut chunk = VisualChunk::new(layer, 91, stage as u64);
      chunk.layer = Some(gfx::RenderLayerInstance {id: 73, rect: retained, dirty});
      self.chunks.push(chunk);
   }

   fn draw_layout(&mut self, stage: usize, text: &mut TextCtx, renderer: &mut metal::MetalRenderer, builder: &mut ui::DrawListBuilder)
   {
      let spec = board(&self.fixture, "visual-layout");
      for index in 0..2
      {
         let value = if index == 0 {spec["short"].as_str().unwrap()} else {spec["long"].as_str().unwrap()};
         let value = if stage > 0 && index == 0 {spec["long"].as_str().unwrap()} else {value};
         let width = number(&spec["container_widths"][index]) - number(&spec["padding"]) * 2.0;
         let measured = Label {text: value.into(), color: color(&self.fixture, "text"), align: Align::Left, wrap: true, font_id: self.font_id, font_px: number(&spec["font_px"])}.measure(width, text).unwrap_or([0.0, 0.0]);
         self.layout_tree.style_mut(self.layout_labels[index]).unwrap().size.h = ui::Dim::Px(measured[1]);
      }
      self.layout_tree.layout(390.0, 844.0);
      for index in 0..2
      {
         let value = if index == 0 && stage == 0 {spec["short"].as_str().unwrap()} else {spec["long"].as_str().unwrap()};
         let container = layout_rect(&self.layout_tree, self.layout_containers[index]);
         let label_rect = layout_rect(&self.layout_tree, self.layout_labels[index]);
         let button_rect = layout_rect(&self.layout_tree, self.layout_buttons[index]);
         builder.rrect(container, [8.0; 4], color(&self.fixture, "panel"));
         let mut uploader = MtlUploader {renderer};
         label(value, self.font_id, number(&spec["font_px"]), label_rect, text, &mut uploader, builder);
         ui::elements::Button {text: "Continue".into(), style: ui::elements::ButtonStyle {color: color(&self.fixture, "blue"), ..Default::default()}}
            .encode(button_rect, SCALE, text, &mut uploader, &ui::elements::ButtonState::default(), builder);
      }
      if self.list_stage != Some(stage)
      {
         self.keys = keys_for_stage(stage);
         self.list_stage = Some(stage);
      }
      self.collection.set_count(self.keys.len());
      let list_rect = rect(&spec["list_rect"]);
      builder.rrect(list_rect, [0.0; 4], color(&self.fixture, "panel"));
      if stage == 2
      {
         self.collection.set_scroll(number(&spec["scroll_away"]));
         let mut scratch = ui::DrawListBuilder::new();
         let mut measure = LayoutMeasure {keys: &self.keys, stage};
         let mut cells = LayoutCells {keys: &self.keys, font_id: self.font_id, font_px: number(&spec["font_px"]), text, renderer};
         self.collection.layout_and_render(list_rect, &mut measure, &mut cells, &mut scratch);
      }
      self.collection.set_scroll(visible_scroll(stage, number(&spec["scroll_changed"])));
      let mut measure = LayoutMeasure {keys: &self.keys, stage};
      let mut cells = LayoutCells {keys: &self.keys, font_id: self.font_id, font_px: number(&spec["font_px"]), text, renderer};
      builder.clip_push(rect_i(list_rect));
      self.collection.layout_and_render(list_rect, &mut measure, &mut cells, builder);
      builder.clip_pop();
   }
}

struct LayoutMeasure<'a> {keys: &'a [u64], stage: usize}
impl ui::collection::Measure for LayoutMeasure<'_>
{
   fn measure(&mut self, index: usize, _: f32) -> f32 {48.0 + (self.keys[index] % 3) as f32 * 18.0 + if self.stage > 0 && self.keys[index] == 1 {36.0} else {0.0}}
   fn item_key(&self, index: usize) -> ui::collection::ItemKey {ui::collection::ItemKey(self.keys[index])}
   fn item_index_for_key(&self, key: ui::collection::ItemKey) -> Option<usize> {self.keys.iter().position(|candidate| *candidate == key.0)}
   fn item_revision(&self, index: usize) -> u64 {self.keys[index] ^ if self.keys[index] == 1 {self.stage as u64 + 1} else {0}}
   fn collection_revision(&self) -> Option<u64> {Some(self.stage as u64)}
}

fn make_layout_tree() -> (ui::NodeTree, [ui::NodeId; 2], [ui::NodeId; 2], [ui::NodeId; 2])
{
   let mut tree = ui::NodeTree::new_root(ui::NodeStyle {
      axis: ui::Axis::Row, size: ui::Size2D {w: ui::Dim::Px(390.0), h: ui::Dim::Px(844.0)},
      padding: ui::Edges {left: 20.0, top: 66.0, right: 20.0, bottom: 0.0}, gap: 30.0,
      background: gfx::Color::from_srgba(1.0, 1.0, 1.0, 1.0), ..Default::default()
   });
   let left = tree.add_node(tree.root(), ui::NodeStyle {axis: ui::Axis::Column, size: ui::Size2D {w: ui::Dim::Px(170.0), h: ui::Dim::Px(220.0)}, padding: ui::Edges {left: 10.0, top: 10.0, right: 10.0, bottom: 10.0}, gap: 8.0, clip: true, ..Default::default()});
   let right = tree.add_node(tree.root(), ui::NodeStyle {axis: ui::Axis::Column, size: ui::Size2D {w: ui::Dim::Px(150.0), h: ui::Dim::Px(220.0)}, padding: ui::Edges {left: 10.0, top: 10.0, right: 10.0, bottom: 10.0}, gap: 8.0, clip: true, ..Default::default()});
   let labels = [tree.add_node(left, ui::NodeStyle::default()), tree.add_node(right, ui::NodeStyle::default())];
   let button = ui::NodeStyle {size: ui::Size2D {w: ui::Dim::Auto, h: ui::Dim::Px(40.0)}, ..Default::default()};
   let buttons = [tree.add_node(left, button), tree.add_node(right, button)];
   tree.layout(390.0, 844.0);
   (tree, [left, right], labels, buttons)
}

fn keys_for_stage(stage: usize) -> Vec<u64>
{
   if stage == 0 {(0..24).collect()} else {vec![3, 1, 99, 0, 4].into_iter().chain(5..24).collect()}
}

fn visible_scroll(stage: usize, changed: f32) -> f32 {if stage == 1 {changed} else {0.0}}

struct LayoutCells<'a> {keys: &'a [u64], font_id: usize, font_px: f32, text: &'a mut TextCtx, renderer: &'a mut metal::MetalRenderer}
impl ui::collection::CellRenderer for LayoutCells<'_>
{
   fn render(&mut self, _: u32, index: usize, rect: gfx::RectF, _: bool, _: bool, builder: &mut ui::DrawListBuilder)
   {
      builder.rrect(rect, [6.0; 4], gfx::Color::from_srgba(1.0, 1.0, 1.0, 1.0));
      let mut uploader = MtlUploader {renderer: self.renderer};
      label(&format!("Row {}", self.keys[index]), self.font_id, self.font_px, gfx::RectF::new(rect.x + 10.0, rect.y, rect.w - 20.0, rect.h), self.text, &mut uploader, builder);
   }
}

fn board<'a>(fixture: &'a Value, name: &str) -> &'a Value {&fixture["boards"][name]}
fn number(value: &Value) -> f32 {value.as_f64().unwrap() as f32}
fn rect(value: &Value) -> gfx::RectF {gfx::RectF::new(number(&value[0]), number(&value[1]), number(&value[2]), number(&value[3]))}
fn rect_i(value: gfx::RectF) -> gfx::RectI {gfx::RectI::new(value.x.round() as i32, value.y.round() as i32, value.w.round() as i32, value.h.round() as i32)}
fn color(fixture: &Value, name: &str) -> gfx::Color {rgba_value(&fixture["palette"][name])}
fn rgba_value(value: &Value) -> gfx::Color {gfx::Color::from_srgba(number(&value[0]), number(&value[1]), number(&value[2]), number(&value[3]))}
fn layout_rect(tree: &ui::NodeTree, node: ui::NodeId) -> gfx::RectF
{
   let rect = tree.layout_rect(node).unwrap();
   gfx::RectF::new(rect.x, rect.y, rect.w, rect.h)
}
fn label(value: &str, font_id: usize, font_px: f32, rect: gfx::RectF, text: &mut TextCtx, uploader: &mut MtlUploader, builder: &mut ui::DrawListBuilder)
{
   Label {text: value.into(), color: gfx::Color::from_srgba(0.12, 0.16, 0.2, 1.0), align: Align::Left, wrap: true, font_id, font_px}.encode(rect, SCALE, text, uploader, builder);
}

#[cfg(test)]
mod tests
{
   use ui::collection::Measure;
   use super::*;

   #[test]
   fn growing_label_moves_the_production_button()
   {
      let (mut tree, _, labels, buttons) = make_layout_tree();
      tree.style_mut(labels[0]).unwrap().size.h = ui::Dim::Px(18.0);
      tree.layout(390.0, 844.0);
      let initial = tree.layout_rect(buttons[0]).unwrap().y;
      tree.style_mut(labels[0]).unwrap().size.h = ui::Dim::Px(72.0);
      tree.layout(390.0, 844.0);
      assert_eq!(tree.layout_rect(buttons[0]).unwrap().y - initial, 54.0);
   }

   #[test]
   fn keyed_transition_preserves_identity_and_changes_key_one_revision()
   {
      let initial = keys_for_stage(0);
      let changed = keys_for_stage(1);
      assert_eq!(&changed[..5], &[3, 1, 99, 0, 4]);
      assert!(!changed.contains(&2));
      assert!(initial.contains(&1) && changed.contains(&1));
      let before = LayoutMeasure {keys: &initial, stage: 0};
      let mut after = LayoutMeasure {keys: &changed, stage: 1};
      let old = initial.iter().position(|key| *key == 1).unwrap();
      let new = changed.iter().position(|key| *key == 1).unwrap();
      assert_ne!(before.item_revision(old), after.item_revision(new));
      assert_eq!(after.measure(new, 350.0), 102.0);
   }

   #[test]
   fn settled_layout_returns_visible_scroll_to_origin()
   {
      assert_eq!(visible_scroll(0, 120.0), 0.0);
      assert_eq!(visible_scroll(1, 120.0), 120.0);
      assert_eq!(visible_scroll(2, 120.0), 0.0);
   }
}
