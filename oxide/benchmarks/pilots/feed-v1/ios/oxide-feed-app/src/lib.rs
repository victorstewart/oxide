//! Production-path Oxide treatment for the frozen feed-v1 device pilot.

#![deny(unsafe_op_in_unsafe_fn, rust_2018_idioms)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::large_enum_variant)]
#![warn(missing_docs)]

/// Frozen workload identity and deterministic row recipe.
pub mod contract;
/// Strict run-record schema and nonce-scoped observation transport.
pub mod observation;

use std::string::String;

use oxide_renderer_api as gfx;
use oxide_text as text;
use oxide_ui_core as ui;
use ui::collection::{CellRenderer, Measure};
use ui::elements::{Align, ImageUploader, TextCtx};

use contract::{FeedFixture, Rgba8, RowRecipe, StartState};
use observation::{
   ObservedComponent,
   ObservedRect,
   MAX_VISIBLE_COMPONENTS,
};

const ROUNDED_BOUNDARY_POINTS: usize = 68;
const ROUNDED_VERTEX_COUNT: usize = ROUNDED_BOUNDARY_POINTS + 1;
const ROUNDED_INDEX_COUNT: usize = ROUNDED_BOUNDARY_POINTS * 3;
const EMPTY_VERTEX: gfx::Vertex = gfx::Vertex { x: 0.0, y: 0.0, u: 0.0, v: 0.0, rgba: 0 };
const QUARTER_CIRCLE: [(f32, f32); 17] = [
   (1.000_000_0, 0.000_000_0),
   (0.995_184_7, 0.098_017_1),
   (0.980_785_3, 0.195_090_3),
   (0.956_940_4, 0.290_284_7),
   (0.923_879_5, 0.382_683_4),
   (0.881_921_3, 0.471_396_7),
   (0.831_469_6, 0.555_570_2),
   (0.773_010_4, 0.634_393_3),
   (0.707_106_8, 0.707_106_8),
   (0.634_393_3, 0.773_010_4),
   (0.555_570_2, 0.831_469_6),
   (0.471_396_7, 0.881_921_3),
   (0.382_683_4, 0.923_879_5),
   (0.290_284_7, 0.956_940_4),
   (0.195_090_3, 0.980_785_3),
   (0.098_017_1, 0.995_184_7),
   (0.000_000_0, 1.000_000_0),
];

#[derive(Clone, Copy)]
struct FeedColors
{
   background: gfx::Color,
   title: gfx::Color,
   caption: gfx::Color,
   metadata: gfx::Color,
   separator: gfx::Color,
   shadow: gfx::Color,
}

impl FeedColors
{
   fn frozen() -> Self
   {
      Self {
         background: renderer_color(contract::BACKGROUND),
         title: renderer_color(contract::TITLE_COLOR),
         caption: renderer_color(contract::CAPTION_COLOR),
         metadata: renderer_color(contract::METADATA_COLOR),
         separator: renderer_color(contract::SEPARATOR_COLOR),
         shadow: renderer_color(contract::SHADOW_COLOR),
      }
   }
}

struct FeedMeasure<'a>
{
   fixture: &'a FeedFixture,
}

impl Measure for FeedMeasure<'_>
{
   fn measure(&mut self, index: usize, _constraint: f32) -> f32
   {
      self.fixture.row_height_points(index).unwrap_or(1) as f32
   }

   fn collection_revision(&self) -> Option<u64>
   {
      Some(1)
   }
}

struct RuntimeTextUploader<'a>
{
   uploader: &'a mut dyn gfx::RuntimeImageUploader,
}

impl ImageUploader for RuntimeTextUploader<'_>
{
   fn create_a8(&mut self, width: u32, height: u32, data: &[u8], row_bytes: usize) -> gfx::ImageHandle
   {
      self.uploader.create_a8(width, height, data, row_bytes)
   }

   fn update_a8(&mut self, handle: gfx::ImageHandle, x: u32, y: u32, width: u32, height: u32, data: &[u8], row_bytes: usize)
   {
      self.uploader.update_a8(handle, x, y, width, height, data, row_bytes);
   }
}

struct FeedRenderer<'a>
{
   text: &'a mut TextCtx,
   regular_font_id: usize,
   bold_font_id: usize,
   title_scratch: &'a mut String,
   metadata_scratch: &'a mut String,
   checker_source: &'a mut [u8; contract::CHECKER_RGBA_BYTE_COUNT],
   checker_handles: &'a mut [Option<gfx::ImageHandle>; contract::CHECKER_VARIANT_COUNT],
   colors: FeedColors,
   scale: f32,
   scroll_offset_points: f32,
   observe_components: bool,
   observed_components: &'a mut [ObservedComponent; MAX_VISIBLE_COMPONENTS],
   observed_component_count: &'a mut usize,
   uploader: &'a mut dyn gfx::RuntimeImageUploader,
   failure: Option<&'static str>,
}

impl CellRenderer for FeedRenderer<'_>
{
   fn render(&mut self, _cell_id: u32, index: usize, rect: gfx::RectF, _focused: bool, _hovered: bool, builder: &mut ui::DrawListBuilder)
   {
      let Some(row) = RowRecipe::at(index) else
      {
         self.failure = Some("CollectionView requested an out-of-range feed-v1 row");
         return;
      };
      let text_x = rect.x
         + (contract::ROW_LEADING_POINTS
            + contract::IMAGE_SIDE_POINTS
            + contract::IMAGE_TEXT_GAP_POINTS) as f32;
      let text_width = (contract::SURFACE_WIDTH_POINTS
         - contract::ROW_LEADING_POINTS
         - contract::IMAGE_SIDE_POINTS
         - contract::IMAGE_TEXT_GAP_POINTS
         - contract::ROW_TRAILING_POINTS) as f32;
      let image = gfx::RectF::new(
         rect.x + contract::ROW_LEADING_POINTS as f32,
         rect.y + contract::ROW_TOP_POINTS as f32,
         contract::IMAGE_SIDE_POINTS as f32,
         contract::IMAGE_SIDE_POINTS as f32,
      );
      let shadow = gfx::RectF::new(
         image.x + contract::SHADOW_OFFSET_X_POINTS as f32,
         image.y + contract::SHADOW_OFFSET_Y_POINTS as f32,
         image.w,
         image.h,
      );
      let caption_height =
         row.caption_line_count() as f32 * contract::CAPTION_LINE_HEIGHT_POINTS as f32;
      let metadata_y = rect.y
         + contract::CAPTION_TOP_POINTS as f32
         + caption_height
         + contract::METADATA_GAP_POINTS as f32;
      let separator_height =
         contract::SEPARATOR_PHYSICAL_PIXELS as f32 / contract::SURFACE_SCALE as f32;
      if self.observe_components
      {
         for (kind_index, component_rect) in [
            (0, rect),
            (1, image),
            (
               2,
               gfx::RectF::new(
                  text_x,
                  rect.y + contract::ROW_TOP_POINTS as f32,
                  text_width,
                  contract::TITLE_HEIGHT_POINTS as f32,
               ),
            ),
            (
               3,
               gfx::RectF::new(
                  text_x,
                  rect.y + contract::CAPTION_TOP_POINTS as f32,
                  text_width,
                  caption_height,
               ),
            ),
            (
               4,
               gfx::RectF::new(
                  text_x,
                  metadata_y,
                  text_width,
                  contract::METADATA_HEIGHT_POINTS as f32,
               ),
            ),
            (
               5,
               gfx::RectF::new(
                  rect.x,
                  rect.y + rect.h - separator_height,
                  rect.w,
                  separator_height,
               ),
            ),
         ]
         {
            self.observe_component(index, kind_index, component_rect);
         }
      }
      if self.failure.is_some()
      {
         return;
      }
      builder.rrect(
         shadow,
         [contract::IMAGE_CORNER_RADIUS_POINTS as f32; 4],
         self.colors.shadow,
      );
      let Some(handle) = self.ensure_checker(row.checker_variant()) else
      {
         self.failure = Some("iOS renderer does not support the required nearest RGBA8 upload");
         return;
      };
      encode_rounded_checker(builder, handle, image);

      row.write_title(self.title_scratch);
      let mut text_uploader = RuntimeTextUploader { uploader: self.uploader };
      // Oxide's label origin is a glyph baseline. UIKit's frozen label frames
      // are top-aligned boxes, so plain labels add UIFont's Asap ascender and
      // the fixed-height caption additionally splits its paragraph leading.
      ui::elements::encode_label_text(
         self.title_scratch,
         self.colors.title,
         Align::Left,
         false,
         self.bold_font_id,
         contract::TITLE_FONT_POINTS as f32,
         gfx::RectF::new(
            text_x,
            rect.y + contract::ROW_TOP_POINTS as f32
               + contract::font_baseline_from_top(contract::TITLE_FONT_POINTS),
            text_width,
            contract::TITLE_HEIGHT_POINTS as f32,
         ),
         self.scale,
         self.text,
         &mut text_uploader,
         builder,
      );
      for line in 0..row.caption_line_count()
      {
         let Some(caption) = row.caption_line(line) else
         {
            continue;
         };
         ui::elements::encode_label_text(
            caption,
            self.colors.caption,
            Align::Left,
            false,
            self.regular_font_id,
            contract::CAPTION_FONT_POINTS as f32,
            gfx::RectF::new(
               text_x,
               rect.y
                  + contract::CAPTION_TOP_POINTS as f32
                  + line as f32 * contract::CAPTION_LINE_HEIGHT_POINTS as f32
                  + contract::caption_baseline_from_line_top(),
               text_width,
               contract::CAPTION_LINE_HEIGHT_POINTS as f32,
            ),
            self.scale,
            self.text,
            &mut text_uploader,
            builder,
         );
      }
      row.write_metadata(self.metadata_scratch);
      ui::elements::encode_label_text(
         self.metadata_scratch,
         self.colors.metadata,
         Align::Left,
         false,
         self.regular_font_id,
         contract::METADATA_FONT_POINTS as f32,
         gfx::RectF::new(
            text_x,
            metadata_y + contract::font_baseline_from_top(contract::METADATA_FONT_POINTS),
            text_width,
            contract::METADATA_HEIGHT_POINTS as f32,
         ),
         self.scale,
         self.text,
         &mut text_uploader,
         builder,
      );
      builder.rrect(
         gfx::RectF::new(rect.x, rect.y + rect.h - separator_height, rect.w, separator_height),
         [0.0; 4],
         self.colors.separator,
      );
   }
}

impl FeedRenderer<'_>
{
   fn observe_component(&mut self, row_index: usize, kind_index: usize, rect: gfx::RectF)
   {
      if *self.observed_component_count >= self.observed_components.len()
      {
         self.failure = Some("feed-v1 ready-frame component capacity exceeded");
         return;
      }
      self.observed_components[*self.observed_component_count] = ObservedComponent {
         row_index,
         kind_index,
         content_rect_points: ObservedRect {
            x: rect.x - contract::SURFACE_ORIGIN_X_POINTS as f32,
            y: rect.y - contract::SURFACE_ORIGIN_Y_POINTS as f32 + self.scroll_offset_points,
            width: rect.w,
            height: rect.h,
         },
      };
      *self.observed_component_count += 1;
   }

   fn ensure_checker(&mut self, variant: usize) -> Option<gfx::ImageHandle>
   {
      if let Some(handle) = self.checker_handles.get(variant).copied().flatten()
      {
         return Some(handle);
      }
      if !contract::checker_rgba_bytes(variant, self.checker_source)
      {
         return None;
      }
      let handle = self.uploader.try_create_rgba8_sampled(
         contract::CHECKER_SIDE_PIXELS as u32,
         contract::CHECKER_SIDE_PIXELS as u32,
         self.checker_source,
         contract::CHECKER_SIDE_PIXELS * 4,
         gfx::ImageSampling::Nearest,
      )?;
      if let Some(slot) = self.checker_handles.get_mut(variant)
      {
         *slot = Some(handle);
      }
      Some(handle)
   }
}

fn encode_rounded_checker(builder: &mut ui::DrawListBuilder, handle: gfx::ImageHandle, rect: gfx::RectF)
{
   let mut vertices = [EMPTY_VERTEX; ROUNDED_VERTEX_COUNT];
   let mut indices = [0_u16; ROUNDED_INDEX_COUNT];
   vertices[0] = image_vertex(rect, rect.w * 0.5, rect.h * 0.5);
   let radius = contract::IMAGE_CORNER_RADIUS_POINTS as f32;
   let corners = [
      (radius, radius, 0_u8),
      (rect.w - radius, radius, 1_u8),
      (rect.w - radius, rect.h - radius, 2_u8),
      (radius, rect.h - radius, 3_u8),
   ];
   let mut boundary = 0_usize;
   for (center_x, center_y, corner) in corners
   {
      for (cosine, sine) in QUARTER_CIRCLE
      {
         let (x, y) = match corner
         {
            0 => (center_x - cosine * radius, center_y - sine * radius),
            1 => (center_x + sine * radius, center_y - cosine * radius),
            2 => (center_x + cosine * radius, center_y + sine * radius),
            _ => (center_x - sine * radius, center_y + cosine * radius),
         };
         vertices[boundary + 1] = image_vertex(rect, x, y);
         boundary += 1;
      }
   }
   for triangle in 0..ROUNDED_BOUNDARY_POINTS
   {
      let offset = triangle * 3;
      indices[offset] = 0;
      indices[offset + 1] = triangle as u16 + 1;
      indices[offset + 2] = ((triangle + 1) % ROUNDED_BOUNDARY_POINTS) as u16 + 1;
   }
   builder.image_mesh(handle, &vertices, &indices, 1.0);
}

fn image_vertex(rect: gfx::RectF, x: f32, y: f32) -> gfx::Vertex
{
   gfx::Vertex {
      x: rect.x + x,
      y: rect.y + y,
      u: x / rect.w,
      v: y / rect.h,
      rgba: 0,
   }
}

fn renderer_color(color: Rgba8) -> gfx::Color
{
   gfx::Color::rgba(
      srgb_channel_to_linear(color.red),
      srgb_channel_to_linear(color.green),
      srgb_channel_to_linear(color.blue),
      color.alpha as f32 / 255.0,
   )
}

fn srgb_channel_to_linear(channel: u8) -> f32
{
   let encoded = channel as f32 / 255.0;
   if encoded <= 0.040_45
   {
      encoded / 12.92
   }
   else
   {
      ((encoded + 0.055) / 1.055).powf(2.4)
   }
}

fn observed_components_match_fixture(fixture: &FeedFixture, state: StartState, captured_offset_points: f32, observed: &[ObservedComponent]) -> bool
{
   let scale = contract::SURFACE_SCALE as f32;
   if !captured_offset_points.is_finite()
      || ((captured_offset_points - state.offset_points() as f32) * scale).abs() > 1.0
   {
      return false;
   }
   let visible_rows = fixture.visible_row_range(state.offset_points());
   if observed.len() != visible_rows.clone().count() * contract::COMPONENT_KINDS.len()
   {
      return false;
   }
   let mut observed_index = 0;
   for row_index in visible_rows
   {
      for (kind_index, kind) in contract::COMPONENT_KINDS.iter().copied().enumerate()
      {
         let Some(component) = observed.get(observed_index) else
         {
            return false;
         };
         let Some(expected) = fixture.component_rect_physical_pixels(row_index, kind) else
         {
            return false;
         };
         if component.row_index != row_index
            || component.kind_index != kind_index
            || !component_rect_within_one_physical_pixel(component.content_rect_points, expected)
         {
            return false;
         }
         observed_index += 1;
      }
   }
   true
}

fn component_rect_within_one_physical_pixel(actual: ObservedRect, expected: contract::PhysicalRect) -> bool
{
   let scale = contract::SURFACE_SCALE as f32;
   let actual_right = (actual.x + actual.width) * scale;
   let actual_bottom = (actual.y + actual.height) * scale;
   let expected_right = (expected.x + expected.width) as f32;
   let expected_bottom = (expected.y + expected.height) as f32;
   actual.x.is_finite()
      && actual.y.is_finite()
      && actual.width.is_finite()
      && actual.height.is_finite()
      && actual_right.is_finite()
      && actual_bottom.is_finite()
      && (actual.x * scale - expected.x as f32).abs() <= 1.0
      && (actual.y * scale - expected.y as f32).abs() <= 1.0
      && (actual_right - expected_right).abs() <= 1.0
      && (actual_bottom - expected_bottom).abs() <= 1.0
}
