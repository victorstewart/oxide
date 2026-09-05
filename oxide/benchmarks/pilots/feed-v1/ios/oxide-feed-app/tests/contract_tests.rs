use std::fmt::Write as _;

use oxide_feed_v1_app::contract::*;

const REGULAR_FONT_BYTES: &[u8] =
   include_bytes!("../../../../../../crates/ui-core/assets/Asap-Regular.ttf");
const BOLD_FONT_BYTES: &[u8] =
   include_bytes!("../../../../../../crates/ui-core/assets/Asap-Bold.ttf");

#[test]
fn rust_recipe_reproduces_the_complete_swift_canonical_identity()
{
   let Some(identity) = canonical_fixture_identity(&FeedFixture::new()) else
   {
      panic!("the frozen recipe must produce a complete canonical identity");
   };
   let mut observed = String::with_capacity(64);
   for byte in identity.sha256
   {
      let _ = write!(observed, "{byte:02x}");
   }
   assert_eq!(identity.byte_count, EXPECTED_CANONICAL_BYTE_COUNT);
   assert_eq!(observed, EXPECTED_CANONICAL_SHA256);
   assert!(identity.is_expected());
}

#[test]
fn prefix_and_rows_are_deterministic_at_every_index()
{
   let first = FeedFixture::new();
   let second = FeedFixture::new();
   assert_eq!(first.row_height_prefix_points(), second.row_height_prefix_points());
   assert_eq!(first.content_extent_points(), CONTENT_EXTENT_POINTS);
   assert_eq!(first.maximum_content_offset_points(), MAXIMUM_CONTENT_OFFSET_POINTS);
   for index in 0..ROW_COUNT
   {
      let row = RowRecipe::at(index);
      assert_eq!(row.map(RowRecipe::height_points), first.row_height_points(index));
   }
}

#[test]
fn checker_recipe_is_bounded_deterministic_and_opaque()
{
   let mut first = [0; CHECKER_RGBA_BYTE_COUNT];
   let mut second = [0; CHECKER_RGBA_BYTE_COUNT];
   for variant in 0..CHECKER_VARIANT_COUNT
   {
      assert!(checker_rgba_bytes(variant, &mut first));
      assert!(checker_rgba_bytes(variant, &mut second));
      assert_eq!(first, second);
      assert!(first.chunks_exact(4).all(|pixel| pixel[3] == 255));
   }
   assert!(!checker_rgba_bytes(CHECKER_VARIANT_COUNT, &mut first));
}

#[test]
fn checker_payload_is_exactly_the_frozen_source_size()
{
   let mut source = [0; CHECKER_RGBA_BYTE_COUNT];
   assert!(checker_rgba_bytes(37, &mut source));
   assert_eq!(source.len(), CHECKER_SIDE_PIXELS * CHECKER_SIDE_PIXELS * 4);
   assert!(source.chunks_exact(4).all(|pixel| pixel[3] == 255));
}

#[test]
fn uikit_label_box_tops_use_the_embedded_asap_vertical_metrics()
{
   assert_eq!(font_vertical_metrics(REGULAR_FONT_BYTES), Some((1_000, 934, -212)));
   assert_eq!(font_vertical_metrics(BOLD_FONT_BYTES), Some((1_000, 934, -212)));
   assert_eq!(snap_to_device_pixel(font_baseline_from_top(16)), 15.0);
   assert_eq!(snap_to_device_pixel(caption_baseline_from_line_top()), 14.0);
   assert_eq!(snap_to_device_pixel(font_baseline_from_top(12)), 11.0 + (1.0 / 3.0));
}

fn snap_to_device_pixel(value: f32) -> f32
{
   let scale = SURFACE_SCALE as f32;
   (value * scale).round() / scale
}

fn font_vertical_metrics(bytes: &[u8]) -> Option<(u16, i16, i16)>
{
   let table_count = u16::from_be_bytes([*bytes.get(4)?, *bytes.get(5)?]) as usize;
   let mut head_offset = None;
   let mut hhea_offset = None;
   for index in 0..table_count
   {
      let entry = 12 + index * 16;
      let tag = bytes.get(entry..entry + 4)?;
      let offset = u32::from_be_bytes([
         *bytes.get(entry + 8)?,
         *bytes.get(entry + 9)?,
         *bytes.get(entry + 10)?,
         *bytes.get(entry + 11)?,
      ]) as usize;
      if tag == b"head"
      {
         head_offset = Some(offset);
      }
      else if tag == b"hhea"
      {
         hhea_offset = Some(offset);
      }
   }
   let head = head_offset?;
   let hhea = hhea_offset?;
   let units_per_em = u16::from_be_bytes([*bytes.get(head + 18)?, *bytes.get(head + 19)?]);
   let ascender = i16::from_be_bytes([*bytes.get(hhea + 4)?, *bytes.get(hhea + 5)?]);
   let descender = i16::from_be_bytes([*bytes.get(hhea + 6)?, *bytes.get(hhea + 7)?]);
   Some((units_per_em, ascender, descender))
}
