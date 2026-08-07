//! Frozen feed-v1 data recipe shared with the UIKit treatment.

use core::fmt::Write;
use sha2::{Digest, Sha256};

/// Canonical fixture schema identifier.
pub const SCHEMA: &str = "oxide.feed-v1.fixture";
/// Canonical fixture revision.
pub const REVISION: u32 = 1;
/// Locale used by both treatments.
pub const LOCALE_IDENTIFIER: &str = "en_US_POSIX";
/// Named color space for canonical RGBA bytes.
pub const RGBA_COLOR_SPACE_NAME: &str = "sRGB IEC61966-2.1";
/// Canonical checker source-pixel encoding.
pub const CHECKER_PIXEL_RULE: &str = "RGBA8:premultiplied-last:opaque";
/// Canonical checker sampling and corner rule.
pub const IMAGE_RASTER_RULE: &str = "nearest:circular-corner";
/// Frozen interface style, direction, and orientation.
pub const INTERFACE_STYLE_RULE: &str = "light-fixed:left-to-right:portrait-locked";
/// SHA-256 of the complete canonical fixture byte stream.
pub const EXPECTED_CANONICAL_SHA256: &str = "a1de9b4a914734fe21d21e9b6f8a9b61970f7e22e0fa4ef0103031e399881473";
/// Length of the complete canonical fixture byte stream.
pub const EXPECTED_CANONICAL_BYTE_COUNT: usize = 717_745;

/// Number of rows in the frozen feed.
pub const ROW_COUNT: usize = 2_000;
/// Host canvas width in points.
pub const HOST_WIDTH_POINTS: u32 = 440;
/// Host canvas height in points.
pub const HOST_HEIGHT_POINTS: u32 = 956;
/// Feed surface width in points.
pub const SURFACE_WIDTH_POINTS: u32 = 390;
/// Feed surface height in points.
pub const SURFACE_HEIGHT_POINTS: u32 = 844;
/// Physical pixels per logical point.
pub const SURFACE_SCALE: u32 = 3;
/// Feed surface X origin in host points.
pub const SURFACE_ORIGIN_X_POINTS: u32 = 25;
/// Feed surface Y origin in host points.
pub const SURFACE_ORIGIN_Y_POINTS: u32 = 56;
/// Canonical host-to-surface placement description.
pub const SURFACE_PLACEMENT_RULE: &str = "center-exact:host440x956:surface390x844:origin25x56";
/// Frozen top safe-area inset in points.
pub const SAFE_AREA_TOP_POINTS: u32 = 0;
/// Frozen left safe-area inset in points.
pub const SAFE_AREA_LEFT_POINTS: u32 = 0;
/// Frozen bottom safe-area inset in points.
pub const SAFE_AREA_BOTTOM_POINTS: u32 = 0;
/// Frozen right safe-area inset in points.
pub const SAFE_AREA_RIGHT_POINTS: u32 = 0;

/// Environment key selecting the measured treatment.
pub const TREATMENT_ENVIRONMENT_KEY: &str = "OXIDE_FEED_V1_TREATMENT";
/// Environment key selecting the feed endpoint.
pub const START_STATE_ENVIRONMENT_KEY: &str = "OXIDE_FEED_V1_START_STATE";
/// Environment key carrying the nonce-scoped protocol identity.
pub const COMPLETION_NONCE_ENVIRONMENT_KEY: &str = "OXIDE_FEED_V1_COMPLETION_NONCE";
/// Record value for the idiomatic UIKit treatment.
pub const IDIOMATIC_UIKIT_VARIANT_VALUE: &str = "uikit-idiomatic";
/// Record value for the optimized UIKit treatment.
pub const OPTIMIZED_UIKIT_VARIANT_VALUE: &str = "uikit-optimized";
/// Record value for the Oxide treatment.
pub const OXIDE_VARIANT_VALUE: &str = "oxide";
/// Sandbox-relative result directory.
pub const RESULT_DIRECTORY_NAME: &str = "Documents";
/// Prefix for nonce-scoped result files.
pub const RESULT_FILE_PREFIX: &str = "oxide-feed-v1-";
/// Suffix for nonce-scoped result files.
pub const RESULT_FILE_SUFFIX: &str = ".json";
/// Darwin ready-notification prefix.
pub const READY_NOTIFICATION_PREFIX: &str = "com.oxide.feed-v1.ready.";
/// Darwin completion-notification prefix.
pub const COMPLETION_NOTIFICATION_PREFIX: &str = "com.oxide.feed-v1.complete.";
/// Darwin failure-notification prefix.
pub const FAILURE_NOTIFICATION_PREFIX: &str = "com.oxide.feed-v1.failed.";
/// Complete run-record schema identifier.
pub const RUN_RECORD_SCHEMA: &str = "oxide.feed-v1.run";
/// Non-measurable failure-record schema identifier.
pub const FAILURE_RECORD_SCHEMA: &str = "oxide.feed-v1.failure";
/// Shared success/failure record schema revision.
pub const RUN_RECORD_SCHEMA_REVISION: u32 = 1;
/// Minimum configured display-link rate.
pub const DISPLAY_LINK_MINIMUM_FRAMES_PER_SECOND: u32 = 120;
/// Maximum configured display-link rate.
pub const DISPLAY_LINK_MAXIMUM_FRAMES_PER_SECOND: u32 = 120;
/// Preferred configured display-link rate.
pub const DISPLAY_LINK_PREFERRED_FRAMES_PER_SECOND: u32 = 120;

/// Row content leading inset in points.
pub const ROW_LEADING_POINTS: u32 = 14;
/// Row content trailing inset in points.
pub const ROW_TRAILING_POINTS: u32 = 14;
/// Image/title top inset in points.
pub const ROW_TOP_POINTS: u32 = 12;
/// Checker image side length in points.
pub const IMAGE_SIDE_POINTS: u32 = 56;
/// Gap between checker image and text in points.
pub const IMAGE_TEXT_GAP_POINTS: u32 = 12;
/// Checker image corner radius in points.
pub const IMAGE_CORNER_RADIUS_POINTS: u32 = 10;
/// Checker shadow horizontal offset in points.
pub const SHADOW_OFFSET_X_POINTS: u32 = 2;
/// Checker shadow vertical offset in points.
pub const SHADOW_OFFSET_Y_POINTS: u32 = 2;
/// Checker shadow blur radius in points.
pub const SHADOW_BLUR_RADIUS_POINTS: u32 = 0;
/// Checker shadow layer opacity byte.
pub const SHADOW_LAYER_OPACITY_BYTE: u32 = 255;
/// Title label box height in points.
pub const TITLE_HEIGHT_POINTS: u32 = 20;
/// Caption label top in row-local points.
pub const CAPTION_TOP_POINTS: u32 = 36;
/// Caption line box height in points.
pub const CAPTION_LINE_HEIGHT_POINTS: u32 = 18;
/// Frozen caption paragraph layout rule.
pub const CAPTION_PARAGRAPH_RULE: &str =
   "line-height:min=18,max=18;line-spacing=0;paragraph-spacing=0;clip";
/// Gap from caption bottom to metadata in points.
pub const METADATA_GAP_POINTS: u32 = 4;
/// Metadata label box height in points.
pub const METADATA_HEIGHT_POINTS: u32 = 16;
/// Separator thickness in physical pixels.
pub const SEPARATOR_PHYSICAL_PIXELS: u32 = 1;

/// Title font size in points.
pub const TITLE_FONT_POINTS: u32 = 16;
/// Caption font size in points.
pub const CAPTION_FONT_POINTS: u32 = 14;
/// Metadata font size in points.
pub const METADATA_FONT_POINTS: u32 = 12;
/// Asap ascender in font units.
pub const ASAP_ASCENDER_UNITS: f32 = 934.0;
/// Asap descender magnitude in font units.
pub const ASAP_DESCENDER_UNITS: f32 = 212.0;
/// Asap units per em.
pub const ASAP_UNITS_PER_EM: f32 = 1_000.0;

/// Checker side length in source pixels.
pub const CHECKER_SIDE_PIXELS: usize = 12;
/// Number of deterministic checker variants.
pub const CHECKER_VARIANT_COUNT: usize = 64;
/// Bytes in one tightly packed RGBA8 checker.
pub const CHECKER_RGBA_BYTE_COUNT: usize = CHECKER_SIDE_PIXELS * CHECKER_SIDE_PIXELS * 4;
/// Complete feed height in points.
pub const CONTENT_EXTENT_POINTS: u32 = 237_460;
/// Bottom endpoint content offset in points.
pub const MAXIMUM_CONTENT_OFFSET_POINTS: u32 = 236_616;
/// Canonical row-mixing algorithm identity.
pub const MIX_ALGORITHM: &str = "splitmix32-v1:seed=0x6f786964";
/// Canonical checker-generation algorithm identity.
pub const CHECKER_ALGORITHM: &str = "checker12-v1:palette3,tile2|3|4|6,phase";
/// Stable component names in serialization order.
pub const COMPONENT_KINDS: [&str; 6] = [
   "row",
   "image",
   "title",
   "caption",
   "metadata",
   "separator",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Unsigned physical-pixel rectangle.
pub struct PhysicalRect
{
   /// Left coordinate in physical pixels.
   pub x: u32,
   /// Top coordinate in physical pixels.
   pub y: u32,
   /// Width in physical pixels.
   pub width: u32,
   /// Height in physical pixels.
   pub height: u32,
}

/// Repository path of the regular Asap font.
pub const REGULAR_FONT_REPOSITORY_PATH: &str = "oxide/crates/ui-core/assets/Asap-Regular.ttf";
/// Bundle filename of the regular Asap font.
pub const REGULAR_FONT_BUNDLE_NAME: &str = "Asap-Regular.ttf";
/// PostScript name of the regular Asap font.
pub const REGULAR_FONT_POSTSCRIPT_NAME: &str = "Asap-Regular";
/// SHA-256 of the regular Asap font bytes.
pub const REGULAR_FONT_SHA256: &str =
   "7d494f276293fb0a8e2aab1fc0e386baa3e8a1d90927f518abb152b5c73e29f9";
/// Byte count of the regular Asap font.
pub const REGULAR_FONT_BYTE_COUNT: u32 = 30_740;
/// Repository path of the bold Asap font.
pub const BOLD_FONT_REPOSITORY_PATH: &str = "oxide/crates/ui-core/assets/Asap-Bold.ttf";
/// Bundle filename of the bold Asap font.
pub const BOLD_FONT_BUNDLE_NAME: &str = "Asap-Bold.ttf";
/// PostScript name of the bold Asap font.
pub const BOLD_FONT_POSTSCRIPT_NAME: &str = "Asap-Bold";
/// SHA-256 of the bold Asap font bytes.
pub const BOLD_FONT_SHA256: &str =
   "7f4feacd835eed23e104413f800a74b9f0270ce8c754c990bfc09b796a3ca628";
/// Byte count of the bold Asap font.
pub const BOLD_FONT_BYTE_COUNT: u32 = 30_352;

const ROW_HEIGHTS: [u32; 4] = [92, 110, 128, 146];
const AUTHORS: [&str; 12] = [
   "Avery Chen",
   "Maya Singh",
   "Theo Brooks",
   "Nora Kim",
   "Iris Martin",
   "Leo Foster",
   "Zoe Parker",
   "Evan Reyes",
   "Mina Patel",
   "Owen Price",
   "Clara Stone",
   "Noah Grant",
];
const CAPTION_LINES: [&str; 6] = [
   "Stable identity keeps work local.",
   "Cold images arrive during motion.",
   "One viewport, one measured feed.",
   "Frames preserve the visible contract.",
   "Rows reuse exact cached resources.",
   "Geometry is frozen before results.",
];
const PALETTES: [(Rgba8, Rgba8); 8] = [
   (Rgba8::rgb(228, 87, 46), Rgba8::rgb(243, 167, 18)),
   (Rgba8::rgb(46, 134, 171), Rgba8::rgb(113, 180, 141)),
   (Rgba8::rgb(114, 70, 145), Rgba8::rgb(240, 160, 190)),
   (Rgba8::rgb(27, 153, 139), Rgba8::rgb(237, 201, 81)),
   (Rgba8::rgb(197, 61, 75), Rgba8::rgb(95, 173, 199)),
   (Rgba8::rgb(83, 74, 183), Rgba8::rgb(242, 180, 66)),
   (Rgba8::rgb(67, 160, 71), Rgba8::rgb(236, 103, 65)),
   (Rgba8::rgb(36, 106, 115), Rgba8::rgb(232, 164, 74)),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Canonical byte-encoded sRGB color.
pub struct Rgba8
{
   /// Red channel byte.
   pub red: u8,
   /// Green channel byte.
   pub green: u8,
   /// Blue channel byte.
   pub blue: u8,
   /// Alpha channel byte.
   pub alpha: u8,
}

impl Rgba8
{
   /// Creates a color with explicit RGBA channels.
   pub const fn new(red: u8, green: u8, blue: u8, alpha: u8) -> Self
   {
      Self { red, green, blue, alpha }
   }

   /// Creates an opaque color from RGB channels.
   pub const fn rgb(red: u8, green: u8, blue: u8) -> Self
   {
      Self::new(red, green, blue, 255)
   }
}

/// Frozen feed background color.
pub const BACKGROUND: Rgba8 = Rgba8::rgb(247, 244, 238);
/// Frozen title color.
pub const TITLE_COLOR: Rgba8 = Rgba8::rgb(28, 36, 48);
/// Frozen caption color.
pub const CAPTION_COLOR: Rgba8 = Rgba8::rgb(79, 93, 107);
/// Frozen metadata color.
pub const METADATA_COLOR: Rgba8 = Rgba8::rgb(123, 132, 144);
/// Frozen separator color.
pub const SEPARATOR_COLOR: Rgba8 = Rgba8::rgb(215, 209, 199);
/// Frozen translucent checker shadow color.
pub const SHADOW_COLOR: Rgba8 = Rgba8::new(38, 43, 50, 102);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Frozen feed endpoint selected by the controller.
pub enum StartState
{
   /// Zero-offset forward gesture endpoint.
   Top,
   /// Maximum-offset reverse gesture endpoint.
   Bottom,
}

impl StartState
{
   /// Parses the exact controller spelling `top` or `bottom`.
   pub fn parse(value: &str) -> Option<Self>
   {
      match value
      {
         "top" => Some(Self::Top),
         "bottom" => Some(Self::Bottom),
         _ => None,
      }
   }

   /// Returns the exact record spelling.
   pub const fn as_str(self) -> &'static str
   {
      match self
      {
         Self::Top => "top",
         Self::Bottom => "bottom",
      }
   }

   /// Returns the frozen gesture direction for this endpoint.
   pub const fn direction(self) -> &'static str
   {
      match self
      {
         Self::Top => "forward",
         Self::Bottom => "reverse",
      }
   }

   /// Returns the exact starting content offset in points.
   pub const fn offset_points(self) -> u32
   {
      match self
      {
         Self::Top => 0,
         Self::Bottom => MAXIMUM_CONTENT_OFFSET_POINTS,
      }
   }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Deterministic content and geometry recipe for one row.
pub struct RowRecipe
{
   index: usize,
   mixed: u32,
   height_index: usize,
}

impl RowRecipe
{
   /// Constructs a recipe for a valid frozen row index.
   pub fn at(index: usize) -> Option<Self>
   {
      if index >= ROW_COUNT
      {
         return None;
      }
      let mixed = mix32(index as u32);
      Some(Self { index, mixed, height_index: ((mixed >> 5) & 3) as usize })
   }

   /// Returns the frozen row index.
   pub const fn index(self) -> usize
   {
      self.index
   }

   /// Returns the row height in points.
   pub const fn height_points(self) -> u32
   {
      ROW_HEIGHTS[self.height_index]
   }

   /// Returns the number of caption lines.
   pub const fn caption_line_count(self) -> usize
   {
      self.height_index + 1
   }

   /// Returns the checker image variant index.
   pub const fn checker_variant(self) -> usize
   {
      ((self.mixed >> 16) & 63) as usize
   }

   /// Returns the deterministic reply count.
   pub const fn reply_count(self) -> u32
   {
      (self.mixed >> 23) % 97
   }

   /// Returns the deterministic author name.
   pub const fn author(self) -> &'static str
   {
      AUTHORS[(self.mixed % AUTHORS.len() as u32) as usize]
   }

   /// Returns one deterministic caption line when it exists.
   pub const fn caption_line(self, line: usize) -> Option<&'static str>
   {
      if line >= self.caption_line_count()
      {
         return None;
      }
      let start = ((self.mixed >> 9) % CAPTION_LINES.len() as u32) as usize;
      Some(CAPTION_LINES[(start + line) % CAPTION_LINES.len()])
   }

   /// Replaces `output` with the canonical row identifier.
   pub fn write_id(self, output: &mut String)
   {
      output.clear();
      output.push_str("feed-v1-row-");
      push_four_digits(self.index, output);
   }

   /// Replaces `output` with the canonical component identifier.
   pub fn write_component_id(self, kind: &str, output: &mut String)
   {
      self.write_id(output);
      output.push('/');
      output.push_str(kind);
   }

   /// Replaces `output` with the canonical title.
   pub fn write_title(self, output: &mut String)
   {
      output.clear();
      output.push_str(self.author());
      output.push_str(" · Update ");
      push_four_digits(self.index, output);
   }

   /// Replaces `output` with the complete canonical caption.
   pub fn write_caption(self, output: &mut String)
   {
      output.clear();
      for line in 0..self.caption_line_count()
      {
         if line != 0
         {
            output.push('\n');
         }
         if let Some(value) = self.caption_line(line)
         {
            output.push_str(value);
         }
      }
   }

   /// Replaces `output` with the canonical metadata text.
   pub fn write_metadata(self, output: &mut String)
   {
      output.clear();
      output.push_str("Row ");
      push_four_digits(self.index, output);
      output.push_str(" · ");
      let _ = write!(output, "{}", self.reply_count());
      output.push_str(" replies");
   }
}

/// Immutable height prefix and geometry index for the complete feed.
pub struct FeedFixture
{
   row_height_prefix_points: [u32; ROW_COUNT + 1],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Observed identity of the complete canonical fixture byte stream.
pub struct CanonicalFixtureIdentity
{
   /// Number of canonical bytes hashed.
   pub byte_count: usize,
   /// SHA-256 digest of those bytes.
   pub sha256: [u8; 32],
}

impl CanonicalFixtureIdentity
{
   /// Returns whether both observed fields match the frozen Swift identity.
   pub fn is_expected(self) -> bool
   {
      if self.byte_count != EXPECTED_CANONICAL_BYTE_COUNT
      {
         return false;
      }
      let mut observed = String::with_capacity(64);
      for byte in self.sha256
      {
         let _ = write!(observed, "{byte:02x}");
      }
      observed == EXPECTED_CANONICAL_SHA256
   }
}

impl Default for FeedFixture
{
   fn default() -> Self
   {
      Self::new()
   }
}

impl FeedFixture
{
   /// Builds the complete deterministic row-height prefix.
   pub fn new() -> Self
   {
      let mut prefix = [0_u32; ROW_COUNT + 1];
      for index in 0..ROW_COUNT
      {
         let height = RowRecipe::at(index).map_or(0, RowRecipe::height_points);
         prefix[index + 1] = prefix[index].saturating_add(height);
      }
      Self { row_height_prefix_points: prefix }
   }

   /// Returns all inclusive row-height prefix entries.
   pub fn row_height_prefix_points(&self) -> &[u32; ROW_COUNT + 1]
   {
      &self.row_height_prefix_points
   }

   /// Returns one row height when the index is valid.
   pub fn row_height_points(&self, index: usize) -> Option<u32>
   {
      if index >= ROW_COUNT
      {
         return None;
      }
      Some(self.row_height_prefix_points[index + 1] - self.row_height_prefix_points[index])
   }

   /// Returns the complete feed height in points.
   pub fn content_extent_points(&self) -> u32
   {
      self.row_height_prefix_points[ROW_COUNT]
   }

   /// Returns the largest valid viewport offset in points.
   pub fn maximum_content_offset_points(&self) -> u32
   {
      self.content_extent_points().saturating_sub(SURFACE_HEIGHT_POINTS)
   }

   /// Returns rows intersecting the viewport at `offset_points`.
   pub fn visible_row_range(&self, offset_points: u32) -> core::ops::Range<usize>
   {
      let offset = offset_points.min(self.content_extent_points().saturating_sub(1));
      let end = offset_points
         .saturating_add(SURFACE_HEIGHT_POINTS)
         .min(self.content_extent_points());
      let first = self.first_row_intersecting(offset);
      let mut last = first;
      while last < ROW_COUNT && self.row_height_prefix_points[last] < end
      {
         last += 1;
      }
      first..last
   }

   fn first_row_intersecting(&self, content_y: u32) -> usize
   {
      let mut lower = 0_usize;
      let mut upper = ROW_COUNT;
      while lower < upper
      {
         let middle = lower + ((upper - lower) / 2);
         if self.row_height_prefix_points[middle + 1] <= content_y
         {
            lower = middle + 1;
         }
         else
         {
            upper = middle;
         }
      }
      lower.min(ROW_COUNT.saturating_sub(1))
   }

   /// Returns one frozen component rectangle in physical pixels.
   pub fn component_rect_physical_pixels(&self, row_index: usize, kind: &str) -> Option<PhysicalRect>
   {
      let row = RowRecipe::at(row_index)?;
      let row_y = *self.row_height_prefix_points.get(row_index)?;
      let scale = SURFACE_SCALE;
      let text_x = ROW_LEADING_POINTS + IMAGE_SIDE_POINTS + IMAGE_TEXT_GAP_POINTS;
      let text_width = SURFACE_WIDTH_POINTS - text_x - ROW_TRAILING_POINTS;
      let caption_height = row.caption_line_count() as u32 * CAPTION_LINE_HEIGHT_POINTS;
      let metadata_y = CAPTION_TOP_POINTS + caption_height + METADATA_GAP_POINTS;
      match kind
      {
         "row" => Some(PhysicalRect {
            x: 0,
            y: row_y * scale,
            width: SURFACE_WIDTH_POINTS * scale,
            height: row.height_points() * scale,
         }),
         "image" => Some(PhysicalRect {
            x: ROW_LEADING_POINTS * scale,
            y: (row_y + ROW_TOP_POINTS) * scale,
            width: IMAGE_SIDE_POINTS * scale,
            height: IMAGE_SIDE_POINTS * scale,
         }),
         "title" => Some(PhysicalRect {
            x: text_x * scale,
            y: (row_y + ROW_TOP_POINTS) * scale,
            width: text_width * scale,
            height: TITLE_HEIGHT_POINTS * scale,
         }),
         "caption" => Some(PhysicalRect {
            x: text_x * scale,
            y: (row_y + CAPTION_TOP_POINTS) * scale,
            width: text_width * scale,
            height: caption_height * scale,
         }),
         "metadata" => Some(PhysicalRect {
            x: text_x * scale,
            y: (row_y + metadata_y) * scale,
            width: text_width * scale,
            height: METADATA_HEIGHT_POINTS * scale,
         }),
         "separator" => Some(PhysicalRect {
            x: 0,
            y: (row_y + row.height_points()) * scale - SEPARATOR_PHYSICAL_PIXELS,
            width: SURFACE_WIDTH_POINTS * scale,
            height: SEPARATOR_PHYSICAL_PIXELS,
         }),
         _ => None,
      }
   }
}

/// Streams the complete Swift-compatible canonical fixture into SHA-256.
pub fn canonical_fixture_identity(fixture: &FeedFixture) -> Option<CanonicalFixtureIdentity>
{
   let mut canonical = CanonicalHasher::new();
   canonical.append_string(SCHEMA);
   canonical.append_u32(REVISION);
   for value in [
      MIX_ALGORITHM,
      CHECKER_ALGORITHM,
      LOCALE_IDENTIFIER,
      RGBA_COLOR_SPACE_NAME,
      CHECKER_PIXEL_RULE,
      IMAGE_RASTER_RULE,
      INTERFACE_STYLE_RULE,
      SURFACE_PLACEMENT_RULE,
      TREATMENT_ENVIRONMENT_KEY,
      START_STATE_ENVIRONMENT_KEY,
      COMPLETION_NONCE_ENVIRONMENT_KEY,
      IDIOMATIC_UIKIT_VARIANT_VALUE,
      OPTIMIZED_UIKIT_VARIANT_VALUE,
      OXIDE_VARIANT_VALUE,
      RESULT_DIRECTORY_NAME,
      RESULT_FILE_PREFIX,
      RESULT_FILE_SUFFIX,
      READY_NOTIFICATION_PREFIX,
      COMPLETION_NOTIFICATION_PREFIX,
      FAILURE_NOTIFICATION_PREFIX,
      RUN_RECORD_SCHEMA,
   ]
   {
      canonical.append_string(value);
   }
   canonical.append_u32(RUN_RECORD_SCHEMA_REVISION);
   canonical.append_string(CAPTION_PARAGRAPH_RULE);
   for kind in COMPONENT_KINDS
   {
      canonical.append_string(kind);
   }
   for state in [StartState::Top, StartState::Bottom]
   {
      canonical.append_string(state.as_str());
      canonical.append_string(state.direction());
   }
   for value in [
      ROW_COUNT as u32,
      HOST_WIDTH_POINTS,
      HOST_HEIGHT_POINTS,
      SURFACE_WIDTH_POINTS,
      SURFACE_HEIGHT_POINTS,
      SURFACE_SCALE,
      SURFACE_ORIGIN_X_POINTS,
      SURFACE_ORIGIN_Y_POINTS,
      SAFE_AREA_TOP_POINTS,
      SAFE_AREA_LEFT_POINTS,
      SAFE_AREA_BOTTOM_POINTS,
      SAFE_AREA_RIGHT_POINTS,
      DISPLAY_LINK_MINIMUM_FRAMES_PER_SECOND,
      DISPLAY_LINK_MAXIMUM_FRAMES_PER_SECOND,
      DISPLAY_LINK_PREFERRED_FRAMES_PER_SECOND,
      ROW_LEADING_POINTS,
      ROW_TRAILING_POINTS,
      ROW_TOP_POINTS,
      IMAGE_SIDE_POINTS,
      IMAGE_TEXT_GAP_POINTS,
      IMAGE_CORNER_RADIUS_POINTS,
      SHADOW_OFFSET_X_POINTS,
      SHADOW_OFFSET_Y_POINTS,
      SHADOW_BLUR_RADIUS_POINTS,
      SHADOW_LAYER_OPACITY_BYTE,
      TITLE_HEIGHT_POINTS,
      CAPTION_TOP_POINTS,
      CAPTION_LINE_HEIGHT_POINTS,
      METADATA_GAP_POINTS,
      METADATA_HEIGHT_POINTS,
      SEPARATOR_PHYSICAL_PIXELS,
      TITLE_FONT_POINTS,
      CAPTION_FONT_POINTS,
      METADATA_FONT_POINTS,
      CHECKER_SIDE_PIXELS as u32,
      CHECKER_VARIANT_COUNT as u32,
   ]
   {
      canonical.append_u32(value);
   }
   for color in [
      BACKGROUND,
      TITLE_COLOR,
      CAPTION_COLOR,
      METADATA_COLOR,
      SEPARATOR_COLOR,
      SHADOW_COLOR,
   ]
   {
      canonical.append_bytes(&[color.red, color.green, color.blue, color.alpha]);
   }
   for font in [
      (
         REGULAR_FONT_REPOSITORY_PATH,
         REGULAR_FONT_BUNDLE_NAME,
         REGULAR_FONT_POSTSCRIPT_NAME,
         REGULAR_FONT_SHA256,
         REGULAR_FONT_BYTE_COUNT,
      ),
      (
         BOLD_FONT_REPOSITORY_PATH,
         BOLD_FONT_BUNDLE_NAME,
         BOLD_FONT_POSTSCRIPT_NAME,
         BOLD_FONT_SHA256,
         BOLD_FONT_BYTE_COUNT,
      ),
   ]
   {
      canonical.append_string(font.0);
      canonical.append_string(font.1);
      canonical.append_string(font.2);
      canonical.append_string(font.3);
      canonical.append_u32(font.4);
   }
   let mut checker = [0; CHECKER_RGBA_BYTE_COUNT];
   for variant in 0..CHECKER_VARIANT_COUNT
   {
      canonical.append_u32(variant as u32);
      if !checker_rgba_bytes(variant, &mut checker)
      {
         return None;
      }
      canonical.append_u32(checker.len() as u32);
      canonical.append_bytes(&checker);
   }
   canonical.append_u32(ROW_COUNT as u32);
   let mut id = String::with_capacity(32);
   let mut title = String::with_capacity(48);
   let mut caption = String::with_capacity(160);
   let mut metadata = String::with_capacity(32);
   let mut component = String::with_capacity(48);
   for index in 0..ROW_COUNT
   {
      let row = RowRecipe::at(index)?;
      row.write_id(&mut id);
      row.write_title(&mut title);
      row.write_caption(&mut caption);
      row.write_metadata(&mut metadata);
      canonical.append_string(&id);
      canonical.append_string(&title);
      canonical.append_string(&caption);
      canonical.append_string(&metadata);
      canonical.append_u32(row.height_points());
      canonical.append_u32(row.checker_variant() as u32);
      for kind in COMPONENT_KINDS
      {
         row.write_component_id(kind, &mut component);
         canonical.append_string(&component);
      }
   }
   canonical.append_u32(fixture.row_height_prefix_points().len() as u32);
   for value in fixture.row_height_prefix_points()
   {
      canonical.append_u32(*value);
   }
   canonical.append_u32(fixture.content_extent_points());
   canonical.append_u32(fixture.maximum_content_offset_points());
   Some(canonical.finish())
}

struct CanonicalHasher
{
   digest: Sha256,
   byte_count: usize,
}

impl CanonicalHasher
{
   fn new() -> Self
   {
      Self { digest: Sha256::new(), byte_count: 0 }
   }

   fn append_bytes(&mut self, bytes: &[u8])
   {
      self.digest.update(bytes);
      self.byte_count = self.byte_count.saturating_add(bytes.len());
   }

   fn append_u32(&mut self, value: u32)
   {
      self.append_bytes(&value.to_le_bytes());
   }

   fn append_string(&mut self, value: &str)
   {
      self.append_u32(value.len() as u32);
      self.append_bytes(value.as_bytes());
   }

   fn finish(self) -> CanonicalFixtureIdentity
   {
      CanonicalFixtureIdentity {
         byte_count: self.byte_count,
         sha256: self.digest.finalize().into(),
      }
   }
}

/// Fills one deterministic tightly packed RGBA8 checker variant.
pub fn checker_rgba_bytes(variant: usize, output: &mut [u8; CHECKER_RGBA_BYTE_COUNT]) -> bool
{
   if variant >= CHECKER_VARIANT_COUNT
   {
      return false;
   }
   let palette = PALETTES[variant & 7];
   let tile_sizes = [2_usize, 3, 4, 6];
   let tile_size = tile_sizes[(variant >> 3) & 3];
   let phase = (variant >> 5) & 1;
   for y in 0..CHECKER_SIDE_PIXELS
   {
      for x in 0..CHECKER_SIDE_PIXELS
      {
         let color = if (((x / tile_size) + (y / tile_size) + phase) & 1) == 0
         {
            palette.0
         }
         else
         {
            palette.1
         };
         let offset = (y * CHECKER_SIDE_PIXELS + x) * 4;
         output[offset] = color.red;
         output[offset + 1] = color.green;
         output[offset + 2] = color.blue;
         output[offset + 3] = color.alpha;
      }
   }
   true
}

/// Converts an Asap font size to the glyph baseline below a label top.
pub const fn font_baseline_from_top(font_points: u32) -> f32
{
   font_points as f32 * ASAP_ASCENDER_UNITS / ASAP_UNITS_PER_EM
}

/// Returns the Asap caption baseline within the fixed line box.
pub const fn caption_baseline_from_line_top() -> f32
{
   let font_points = CAPTION_FONT_POINTS as f32;
   let ascent = font_points * ASAP_ASCENDER_UNITS / ASAP_UNITS_PER_EM;
   let descent = font_points * ASAP_DESCENDER_UNITS / ASAP_UNITS_PER_EM;
   ascent + (CAPTION_LINE_HEIGHT_POINTS as f32 - ascent - descent) * 0.5
}

fn mix32(value: u32) -> u32
{
   let mut mixed = value.wrapping_add(0x6f78_6964);
   mixed ^= mixed >> 16;
   mixed = mixed.wrapping_mul(0x7feb_352d);
   mixed ^= mixed >> 15;
   mixed = mixed.wrapping_mul(0x846c_a68b);
   mixed ^= mixed >> 16;
   mixed
}

fn push_four_digits(value: usize, output: &mut String)
{
   output.push(char::from(b'0' + ((value / 1_000) % 10) as u8));
   output.push(char::from(b'0' + ((value / 100) % 10) as u8));
   output.push(char::from(b'0' + ((value / 10) % 10) as u8));
   output.push(char::from(b'0' + (value % 10) as u8));
}
