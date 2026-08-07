use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::{self, File};
use std::io::{BufReader, Write};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const FIXTURE_SCHEMA: &str = "oxide.feed-v1.fixture";
const FIXTURE_REVISION: u32 = 1;
const FIXTURE_SHA256: &str = "a1de9b4a914734fe21d21e9b6f8a9b61970f7e22e0fa4ef0103031e399881473";
const FIXTURE_BYTE_COUNT: u64 = 717_745;
const RUN_SCHEMA: &str = "oxide.feed-v1.run";
const RUN_SCHEMA_REVISION: u32 = 1;
const HOST_WIDTH_POINTS: u32 = 440;
const HOST_HEIGHT_POINTS: u32 = 956;
const SURFACE_ORIGIN_X_POINTS: u32 = 25;
const SURFACE_ORIGIN_Y_POINTS: u32 = 56;
const SURFACE_WIDTH_POINTS: u32 = 390;
const SURFACE_HEIGHT_POINTS: u32 = 844;
const SCALE: u32 = 3;
const ROW_COUNT: u32 = 2_000;
const COMPONENT_COUNT: u32 = 12_000;
const CONTENT_EXTENT_POINTS: i32 = 237_460;
const MAXIMUM_OFFSET_POINTS: i32 = 236_616;
const SSIM_THRESHOLD: f64 = 0.96;
const TILE_SIDE: u32 = 48;
const TILE_MAE_THRESHOLD: f64 = 18.0;
const COMPONENT_TOLERANCE_PX: i32 = 1;
const PERIOD_MIN_SECONDS: f64 = 0.0075;
const PERIOD_MAX_SECONDS: f64 = 0.0092;
const PERIOD_ADMISSION_RATIO: f64 = 0.95;
const MINIMUM_TRAVEL_POINTS: f64 = 524.0;
const TRAVEL_MEDIAN_EQUIVALENCE_MARGIN: f64 = 0.05;
const TRAVEL_CONFIDENCE_EQUIVALENCE_MARGIN: f64 = 0.10;
const TRAVEL_EQUIVALENCE_EPSILON: f64 = 1e-12;
const ATTACHMENT_COUNT: usize = 6;
const ATTACHMENT_TEST_IDENTIFIER: &str = "FeedV1ControllerTests/testFeedV1PhysicalDevicePilot()";
const BOOTSTRAP_RESAMPLES: usize = 100_000;
const BOOTSTRAP_SEED: u64 = 0x6f78_6964_655f_7631;
const NON_INFERIOR_MARGIN: f64 = 0.05;
const MISSED_GUARDRAIL_DELTA: f64 = 0.005;
const MISSED_GUARDRAIL_ABSOLUTE: f64 = 0.02;
const HITCH_GUARDRAIL_DELTA_MS_S: f64 = 2.0;
const HITCH_GUARDRAIL_ABSOLUTE_MS_S: f64 = 10.0;
const VISUAL_GATE_ID: &str = "feed-v1:ssim8-luma>=0.96:tile48-rgb-mae<=18:rgba8:surface1170x2532:v1";

#[derive(Clone, Debug)]
pub struct ReducePaths
{
   pub run_root: PathBuf,
   pub output_json: PathBuf,
   pub output_markdown: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Population
{
   Smoke,
   Full,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureIdentity
{
   schema: String,
   revision: u32,
   canonical_sha256: String,
   canonical_byte_count: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunIdentity
{
   nonce: String,
   phase: String,
   session_index: u32,
   pair_index: u32,
   order_index: u32,
   treatment: String,
   start_state: String,
   direction: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Canvas
{
   host_width_points: u32,
   host_height_points: u32,
   surface_origin_x_points: u32,
   surface_origin_y_points: u32,
   surface_width_points: u32,
   surface_height_points: u32,
   scale: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Rect
{
   pub x: i32,
   pub y: i32,
   pub width: i32,
   pub height: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Component
{
   pub id: String,
   pub kind: String,
   pub row_index: u32,
   pub content_rect_px: Rect,
   pub viewport_clip_px: Rect,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Geometry
{
   row_count: u32,
   manifest_component_count: u32,
   content_extent_points: f64,
   maximum_content_offset_points: f64,
   captured_content_offset_points: f64,
   viewport_clip_px: Rect,
   visible_components: Vec<Component>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrameRateRange
{
   minimum: f64,
   maximum: f64,
   preferred: f64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvironmentState
{
   thermal_state: String,
   low_power_mode: bool,
   maximum_frames_per_second: u32,
   configured_frame_rate: FrameRateRange,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Environment
{
   before: EnvironmentState,
   after: EnvironmentState,
   thermal_state_change_count: u32,
   low_power_mode_change_count: u32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Gesture
{
   start_offset_points: f64,
   end_offset_points: f64,
   signed_travel_points: f64,
   travel_distance_points: f64,
   duration_seconds: f64,
   inertia_observed: bool,
   settled: bool,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DisplaySample
{
   timestamp_seconds: f64,
   target_timestamp_seconds: f64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DisplayLink
{
   clock: String,
   callback_only: bool,
   samples: Vec<DisplaySample>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunRecord
{
   schema: String,
   schema_revision: u32,
   fixture: FixtureIdentity,
   run: RunIdentity,
   canvas: Canvas,
   geometry: Geometry,
   environment: Environment,
   gesture: Gesture,
   display_link: DisplayLink,
   status: String,
   failure: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FailureRecord
{
   schema: String,
   schema_revision: u32,
   fixture: FixtureIdentity,
   nonce: Option<String>,
   treatment: Option<String>,
   stage: String,
   message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct VisualMetrics
{
   pub ssim: f64,
   pub worst_tile_rgb_mae: f64,
   pub exact_rgb_mae: f64,
   pub passes: bool,
}

#[derive(Clone, Debug)]
pub struct RgbaImage
{
   pub width: u32,
   pub height: u32,
   pub pixels: Vec<u8>,
}

impl RgbaImage
{
   fn validate(&self) -> Result<(), String>
   {
      let expected = self.width as usize * self.height as usize * 4;
      if self.pixels.len() != expected
      {
         return Err(format!("RGBA byte count {} does not match {expected}", self.pixels.len()));
      }
      Ok(())
   }
}

#[derive(Clone, Debug, Serialize)]
struct AdversarialResult
{
   mutation: String,
   rejected: bool,
   ssim: f64,
   worst_tile_rgb_mae: f64,
}

pub fn visual_metrics(reference: &RgbaImage, candidate: &RgbaImage) -> Result<VisualMetrics, String>
{
   reference.validate()?;
   candidate.validate()?;
   if reference.width != candidate.width || reference.height != candidate.height
   {
      return Err("visual inputs have different dimensions".to_string());
   }
   let ssim = windowed_luma_ssim(reference, candidate, 8)?;
   let (worst_tile_rgb_mae, exact_rgb_mae) = tile_rgb_mae(reference, candidate, TILE_SIDE)?;
   Ok(VisualMetrics {
      ssim,
      worst_tile_rgb_mae,
      exact_rgb_mae,
      passes: ssim >= SSIM_THRESHOLD && worst_tile_rgb_mae <= TILE_MAE_THRESHOLD,
   })
}

pub fn strict_validate_run_json(bytes: &[u8]) -> Result<(), String>
{
   let record: RunRecord = serde_json::from_slice(bytes).map_err(|error| format!("strict run schema: {error}"))?;
   validate_run(&record)
}

pub fn strict_validate_failure_json(bytes: &[u8]) -> Result<(), String>
{
   let record: FailureRecord = serde_json::from_slice(bytes).map_err(|error| format!("strict failure schema: {error}"))?;
   if record.schema != "oxide.feed-v1.failure" || record.schema_revision != 1
   {
      return Err("failure schema identity mismatch".to_string());
   }
   validate_fixture(&record.fixture)
}

pub fn frozen_components(start_state: &str) -> Result<Vec<Component>, String>
{
   frozen_visible_components(start_state)
}

pub fn frozen_order_index(phase: &str, session: u32, pair: u32, treatment: &str) -> Result<u32, String>
{
   expected_order_index(phase, session, pair, treatment)
}

fn windowed_luma_ssim(reference: &RgbaImage, candidate: &RgbaImage, side: u32) -> Result<f64, String>
{
   if side == 0
   {
      return Err("SSIM window side is zero".to_string());
   }
   let c1 = (0.01_f64 * 255.0).powi(2);
   let c2 = (0.03_f64 * 255.0).powi(2);
   let mut total = 0.0;
   let mut windows = 0_u64;
   let mut y = 0;
   while y < reference.height
   {
      let max_y = (y + side).min(reference.height);
      let mut x = 0;
      while x < reference.width
      {
         let max_x = (x + side).min(reference.width);
         let count = (max_x - x) as f64 * (max_y - y) as f64;
         let mut reference_mean = 0.0;
         let mut candidate_mean = 0.0;
         for py in y .. max_y
         {
            for px in x .. max_x
            {
               reference_mean += luma(reference, px, py);
               candidate_mean += luma(candidate, px, py);
            }
         }
         reference_mean /= count;
         candidate_mean /= count;

         let mut reference_variance = 0.0;
         let mut candidate_variance = 0.0;
         let mut covariance = 0.0;
         for py in y .. max_y
         {
            for px in x .. max_x
            {
               let reference_delta = luma(reference, px, py) - reference_mean;
               let candidate_delta = luma(candidate, px, py) - candidate_mean;
               reference_variance += reference_delta * reference_delta;
               candidate_variance += candidate_delta * candidate_delta;
               covariance += reference_delta * candidate_delta;
            }
         }
         let denominator = (count - 1.0).max(1.0);
         reference_variance /= denominator;
         candidate_variance /= denominator;
         covariance /= denominator;
         total += ((2.0 * reference_mean * candidate_mean + c1) * (2.0 * covariance + c2))
            / ((reference_mean.powi(2) + candidate_mean.powi(2) + c1)
               * (reference_variance + candidate_variance + c2));
         windows += 1;
         x += side;
      }
      y += side;
   }
   if windows == 0
   {
      return Err("SSIM received an empty image".to_string());
   }
   Ok(total / windows as f64)
}

fn luma(image: &RgbaImage, x: u32, y: u32) -> f64
{
   let index = ((y * image.width + x) * 4) as usize;
   0.2126 * f64::from(image.pixels[index])
      + 0.7152 * f64::from(image.pixels[index + 1])
      + 0.0722 * f64::from(image.pixels[index + 2])
}

fn tile_rgb_mae(reference: &RgbaImage, candidate: &RgbaImage, side: u32) -> Result<(f64, f64), String>
{
   if side == 0
   {
      return Err("tile side is zero".to_string());
   }
   let mut worst: f64 = 0.0;
   let mut total_error = 0_u64;
   let mut total_channels = 0_u64;
   let mut y = 0;
   while y < reference.height
   {
      let mut x = 0;
      while x < reference.width
      {
         let (error, channels) = rgb_error(reference, candidate, Rect {
            x: x as i32,
            y: y as i32,
            width: side.min(reference.width - x) as i32,
            height: side.min(reference.height - y) as i32,
         })?;
         worst = worst.max(error as f64 / channels as f64);
         total_error = total_error.checked_add(error)
            .ok_or_else(|| "RGB MAE error sum overflowed".to_string())?;
         total_channels = total_channels.checked_add(channels)
            .ok_or_else(|| "RGB MAE channel count overflowed".to_string())?;
         x += side;
      }
      y += side;
   }
   Ok((worst, total_error as f64 / total_channels as f64))
}

fn rgb_error(reference: &RgbaImage, candidate: &RgbaImage, rect: Rect) -> Result<(u64, u64), String>
{
   let rect = bounded_rect(rect, reference.width, reference.height)?;
   let mut error = 0_u64;
   let mut channels = 0_u64;
   for y in rect.y as u32 .. (rect.y + rect.height) as u32
   {
      for x in rect.x as u32 .. (rect.x + rect.width) as u32
      {
         let index = ((y * reference.width + x) * 4) as usize;
         for channel in 0 .. 3
         {
            error += u64::from(reference.pixels[index + channel].abs_diff(candidate.pixels[index + channel]));
            channels += 1;
         }
      }
   }
   if channels == 0
   {
      return Err("RGB MAE received an empty rectangle".to_string());
   }
   Ok((error, channels))
}

fn bounded_rect(rect: Rect, width: u32, height: u32) -> Result<Rect, String>
{
   let min_x = rect.x.max(0).min(width as i32);
   let min_y = rect.y.max(0).min(height as i32);
   let max_x = rect.x.saturating_add(rect.width).max(0).min(width as i32);
   let max_y = rect.y.saturating_add(rect.height).max(0).min(height as i32);
   if min_x >= max_x || min_y >= max_y
   {
      return Err("rectangle does not intersect the image".to_string());
   }
   Ok(Rect { x: min_x, y: min_y, width: max_x - min_x, height: max_y - min_y })
}

fn decode_png(path: &Path) -> Result<RgbaImage, String>
{
   let file = File::open(path).map_err(|error| format!("open {}: {error}", path.display()))?;
   let mut decoder = png::Decoder::new(BufReader::new(file));
   decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
   let mut reader = decoder.read_info().map_err(|error| format!("decode {}: {error}", path.display()))?;
   let mut bytes = vec![0; reader.output_buffer_size()];
   let info = reader.next_frame(&mut bytes).map_err(|error| format!("read {}: {error}", path.display()))?;
   let source = &bytes[.. info.buffer_size()];
   let pixel_count = info.width as usize * info.height as usize;
   let mut rgba = Vec::with_capacity(pixel_count * 4);
   match info.color_type
   {
      png::ColorType::Rgba => rgba.extend_from_slice(source),
      png::ColorType::Rgb =>
      {
         for pixel in source.chunks_exact(3)
         {
            rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
         }
      }
      png::ColorType::Grayscale =>
      {
         for &value in source
         {
            rgba.extend_from_slice(&[value, value, value, 255]);
         }
      }
      png::ColorType::GrayscaleAlpha =>
      {
         for pixel in source.chunks_exact(2)
         {
            rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]);
         }
      }
      png::ColorType::Indexed => return Err(format!("{} remained indexed after PNG expansion", path.display())),
   }
   let image = RgbaImage { width: info.width, height: info.height, pixels: rgba };
   image.validate()?;
   Ok(image)
}

fn crop_surface(image: &RgbaImage) -> Result<RgbaImage, String>
{
   let surface_width = SURFACE_WIDTH_POINTS * SCALE;
   let surface_height = SURFACE_HEIGHT_POINTS * SCALE;
   let host_width = HOST_WIDTH_POINTS * SCALE;
   let host_height = HOST_HEIGHT_POINTS * SCALE;
   if image.width != host_width || image.height != host_height
   {
      return Err(format!(
         "capture is {}x{}, expected full XCUIScreen canvas {}x{}",
         image.width,
         image.height,
         host_width,
         host_height
      ));
   }
   let origin_x = SURFACE_ORIGIN_X_POINTS * SCALE;
   let origin_y = SURFACE_ORIGIN_Y_POINTS * SCALE;
   let mut pixels = Vec::with_capacity(surface_width as usize * surface_height as usize * 4);
   for y in origin_y .. origin_y + surface_height
   {
      let start = ((y * image.width + origin_x) * 4) as usize;
      let end = start + surface_width as usize * 4;
      pixels.extend_from_slice(&image.pixels[start .. end]);
   }
   Ok(RgbaImage { width: surface_width, height: surface_height, pixels })
}

fn validate_run(record: &RunRecord) -> Result<(), String>
{
   if record.schema != RUN_SCHEMA || record.schema_revision != RUN_SCHEMA_REVISION
   {
      return Err("run schema identity mismatch".to_string());
   }
   validate_fixture(&record.fixture)?;
   if record.status != "complete" || record.failure.is_some()
   {
      return Err(format!("run {} is not complete", record.run.nonce));
   }
   let expected_direction = match record.run.start_state.as_str()
   {
      "top" => "forward",
      "bottom" => "reverse",
      _ => return Err(format!("run {} has unknown start state", record.run.nonce)),
   };
   if record.run.direction != expected_direction
   {
      return Err(format!("run {} direction contradicts start state", record.run.nonce));
   }
   if !matches!(record.run.phase.as_str(), "smoke" | "primary")
   {
      return Err(format!("run {} has unknown phase", record.run.nonce));
   }
   if !matches!(record.run.treatment.as_str(), "uikit-idiomatic" | "uikit-optimized" | "oxide")
   {
      return Err(format!("run {} has unknown treatment", record.run.nonce));
   }
   validate_canvas(&record.canvas)?;
   validate_geometry(&record.geometry, &record.run)?;
   validate_environment(&record.environment, &record.run.nonce)?;
   validate_gesture(&record.gesture, &record.run, &record.geometry)?;
   if record.display_link.clock != "CADisplayLink.timestamp/targetTimestamp" || !record.display_link.callback_only
   {
      return Err(format!("run {} collector identity mismatch", record.run.nonce));
   }
   if record.display_link.samples.len() < 2
   {
      return Err(format!("run {} has fewer than two display-link samples", record.run.nonce));
   }
   Ok(())
}

fn validate_fixture(fixture: &FixtureIdentity) -> Result<(), String>
{
   if fixture.schema != FIXTURE_SCHEMA
      || fixture.revision != FIXTURE_REVISION
      || fixture.canonical_sha256 != FIXTURE_SHA256
      || fixture.canonical_byte_count != FIXTURE_BYTE_COUNT
   {
      return Err("fixture identity mismatch".to_string());
   }
   Ok(())
}

fn validate_canvas(canvas: &Canvas) -> Result<(), String>
{
   let valid = canvas.host_width_points == HOST_WIDTH_POINTS
      && canvas.host_height_points == HOST_HEIGHT_POINTS
      && canvas.surface_origin_x_points == SURFACE_ORIGIN_X_POINTS
      && canvas.surface_origin_y_points == SURFACE_ORIGIN_Y_POINTS
      && canvas.surface_width_points == SURFACE_WIDTH_POINTS
      && canvas.surface_height_points == SURFACE_HEIGHT_POINTS
      && canvas.scale == SCALE;
   if !valid
   {
      return Err("canvas identity mismatch".to_string());
   }
   Ok(())
}

fn validate_geometry(geometry: &Geometry, run: &RunIdentity) -> Result<(), String>
{
   if geometry.row_count != ROW_COUNT || geometry.manifest_component_count != COMPONENT_COUNT
   {
      return Err(format!("run {} component identity count mismatch", run.nonce));
   }
   finite(&[
      geometry.content_extent_points,
      geometry.maximum_content_offset_points,
      geometry.captured_content_offset_points,
   ], "geometry")?;
   if (geometry.content_extent_points - f64::from(CONTENT_EXTENT_POINTS)).abs()
      > 1.0 / f64::from(SCALE)
      || (geometry.maximum_content_offset_points - f64::from(MAXIMUM_OFFSET_POINTS)).abs()
         > 1.0 / f64::from(SCALE)
   {
      return Err(format!("run {} frozen extent or maximum offset mismatch", run.nonce));
   }
   let expected_viewport = Rect {
      x: 0,
      y: 0,
      width: (SURFACE_WIDTH_POINTS * SCALE) as i32,
      height: (SURFACE_HEIGHT_POINTS * SCALE) as i32,
   };
   if geometry.viewport_clip_px != expected_viewport
   {
      return Err(format!("run {} viewport clip mismatch", run.nonce));
   }
   if (geometry.maximum_content_offset_points
      - (geometry.content_extent_points - f64::from(SURFACE_HEIGHT_POINTS))).abs()
      > 1.0 / f64::from(SCALE)
   {
      return Err(format!("run {} maximum offset is inconsistent", run.nonce));
   }
   let expected_capture = if run.start_state == "top" { 0.0 } else { geometry.maximum_content_offset_points };
   if (geometry.captured_content_offset_points - expected_capture).abs() > 1.0 / f64::from(SCALE)
   {
      return Err(format!("run {} capture offset differs from frozen state", run.nonce));
   }
   let expected_components = frozen_visible_components(&run.start_state)?;
   if geometry.visible_components.len() != expected_components.len()
   {
      return Err(format!("run {} visible component count differs from the frozen manifest", run.nonce));
   }
   let mut prior: Option<(u32, usize)> = None;
   let mut ids = BTreeSet::new();
   for (component, expected_component) in geometry.visible_components.iter().zip(&expected_components)
   {
      if component.content_rect_px.width <= 0
         || component.content_rect_px.height <= 0
         || component.viewport_clip_px.width <= 0
         || component.viewport_clip_px.height <= 0
      {
         return Err(format!("run {} has an empty component rectangle", run.nonce));
      }
      let kind_order = component_kind_order(&component.kind)?;
      if let Some(previous) = prior
      {
         if (component.row_index, kind_order) <= previous
         {
            return Err(format!("run {} visible components are unsorted", run.nonce));
         }
      }
      prior = Some((component.row_index, kind_order));
      if !ids.insert(component.id.clone())
      {
         return Err(format!("run {} has a duplicate component ID", run.nonce));
      }
      if component.id != expected_component.id
         || component.kind != expected_component.kind
         || component.row_index != expected_component.row_index
         || !rect_within(component.content_rect_px, expected_component.content_rect_px, COMPONENT_TOLERANCE_PX)
         || !rect_within(component.viewport_clip_px, expected_component.viewport_clip_px, COMPONENT_TOLERANCE_PX)
      {
         return Err(format!("run {} component manifest differs from the frozen recipe", run.nonce));
      }
      if component.viewport_clip_px.x < 0
         || component.viewport_clip_px.y < 0
         || component.viewport_clip_px.x + component.viewport_clip_px.width > expected_viewport.width
         || component.viewport_clip_px.y + component.viewport_clip_px.height > expected_viewport.height
      {
         return Err(format!("run {} component clip leaves the viewport", run.nonce));
      }
   }
   if geometry.visible_components.is_empty()
   {
      return Err(format!("run {} has no visible components", run.nonce));
   }
   Ok(())
}

fn frozen_visible_components(start_state: &str) -> Result<Vec<Component>, String>
{
   let offset_points = match start_state
   {
      "top" => 0,
      "bottom" => MAXIMUM_OFFSET_POINTS,
      _ => return Err("unknown frozen start state".to_string()),
   };
   let offset_px = offset_points * SCALE as i32;
   let viewport = Rect {
      x: 0,
      y: offset_px,
      width: (SURFACE_WIDTH_POINTS * SCALE) as i32,
      height: (SURFACE_HEIGHT_POINTS * SCALE) as i32,
   };
   let prefix = frozen_prefix();
   if prefix.last().copied() != Some(CONTENT_EXTENT_POINTS)
   {
      return Err("reducer frozen row recipe extent mismatch".to_string());
   }
   let mut components = Vec::new();
   for row_index in 0 .. ROW_COUNT as usize
   {
      let row_top = prefix[row_index];
      let row_bottom = prefix[row_index + 1];
      if row_bottom <= offset_points || row_top >= offset_points + SURFACE_HEIGHT_POINTS as i32
      {
         continue;
      }
      for kind in ["row", "image", "title", "caption", "metadata", "separator"]
      {
         let content_rect = frozen_component_rect(row_index, kind, &prefix)?;
         let Some(intersection) = intersect(content_rect, viewport) else
         {
            continue;
         };
         components.push(Component {
            id: format!("feed-v1-row-{row_index:04}/{kind}"),
            kind: kind.to_string(),
            row_index: row_index as u32,
            content_rect_px: content_rect,
            viewport_clip_px: Rect {
               x: intersection.x,
               y: intersection.y - offset_px,
               width: intersection.width,
               height: intersection.height,
            },
         });
      }
   }
   Ok(components)
}

fn frozen_prefix() -> Vec<i32>
{
   let mut prefix = Vec::with_capacity(ROW_COUNT as usize + 1);
   prefix.push(0);
   for row_index in 0 .. ROW_COUNT
   {
      let mixed = mix32(row_index);
      let height = [92, 110, 128, 146][((mixed >> 5) & 3) as usize];
      let next = prefix[prefix.len() - 1] + height;
      prefix.push(next);
   }
   prefix
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

fn frozen_component_rect(row_index: usize, kind: &str, prefix: &[i32]) -> Result<Rect, String>
{
   let scale = SCALE as i32;
   let row_y = prefix[row_index];
   let row_height = prefix[row_index + 1] - row_y;
   let text_x = 14 + 56 + 12;
   let text_width = SURFACE_WIDTH_POINTS as i32 - text_x - 14;
   let height_index = [92, 110, 128, 146]
      .iter()
      .position(|height| *height == row_height)
      .ok_or_else(|| "unknown frozen row height".to_string())?;
   let caption_height = (height_index as i32 + 1) * 18;
   let metadata_y = 36 + caption_height + 4;
   let rect = match kind
   {
      "row" => Rect { x: 0, y: row_y * scale, width: SURFACE_WIDTH_POINTS as i32 * scale, height: row_height * scale },
      "image" => Rect { x: 14 * scale, y: (row_y + 12) * scale, width: 56 * scale, height: 56 * scale },
      "title" => Rect { x: text_x * scale, y: (row_y + 12) * scale, width: text_width * scale, height: 20 * scale },
      "caption" => Rect { x: text_x * scale, y: (row_y + 36) * scale, width: text_width * scale, height: caption_height * scale },
      "metadata" => Rect { x: text_x * scale, y: (row_y + metadata_y) * scale, width: text_width * scale, height: 16 * scale },
      "separator" => Rect { x: 0, y: (row_y + row_height) * scale - 1, width: SURFACE_WIDTH_POINTS as i32 * scale, height: 1 },
      _ => return Err(format!("unknown component kind {kind}")),
   };
   Ok(rect)
}

fn intersect(left: Rect, right: Rect) -> Option<Rect>
{
   let min_x = left.x.max(right.x);
   let min_y = left.y.max(right.y);
   let max_x = (left.x + left.width).min(right.x + right.width);
   let max_y = (left.y + left.height).min(right.y + right.height);
   (min_x < max_x && min_y < max_y).then_some(Rect {
      x: min_x,
      y: min_y,
      width: max_x - min_x,
      height: max_y - min_y,
   })
}

fn component_kind_order(kind: &str) -> Result<usize, String>
{
   ["row", "image", "title", "caption", "metadata", "separator"]
      .iter()
      .position(|candidate| *candidate == kind)
      .ok_or_else(|| format!("unknown component kind {kind}"))
}

fn validate_environment(environment: &Environment, nonce: &str) -> Result<(), String>
{
   if environment.thermal_state_change_count != 0 || environment.low_power_mode_change_count != 0
   {
      return Err(format!("run {nonce} observed a thermal or Low Power Mode transition"));
   }
   for state in [&environment.before, &environment.after]
   {
      finite(&[
         state.configured_frame_rate.minimum,
         state.configured_frame_rate.maximum,
         state.configured_frame_rate.preferred,
      ], "frame-rate range")?;
      if state.thermal_state != "nominal"
      {
         return Err(format!("run {nonce} thermal state is not nominal"));
      }
      if state.low_power_mode
      {
         return Err(format!("run {nonce} has Low Power Mode enabled"));
      }
      if state.maximum_frames_per_second < 120
      {
         return Err(format!("run {nonce} display reports less than 120 Hz"));
      }
      if state.configured_frame_rate.minimum != 120.0
         || state.configured_frame_rate.maximum != 120.0
         || state.configured_frame_rate.preferred != 120.0
      {
         return Err(format!("run {nonce} configured frame-rate range mismatch"));
      }
   }
   Ok(())
}

fn validate_gesture(gesture: &Gesture, run: &RunIdentity, geometry: &Geometry) -> Result<(), String>
{
   finite(&[
      gesture.start_offset_points,
      gesture.end_offset_points,
      gesture.signed_travel_points,
      gesture.travel_distance_points,
      gesture.duration_seconds,
   ], "gesture")?;
   if !gesture.inertia_observed
   {
      return Err(format!("run {} did not enter inertial motion", run.nonce));
   }
   if !gesture.settled || gesture.duration_seconds <= 0.0 || gesture.duration_seconds > 6.0
   {
      return Err(format!("run {} did not settle inside the app-owned deadline", run.nonce));
   }
   let point_tolerance = 1.0 / f64::from(SCALE);
   if (gesture.start_offset_points - geometry.captured_content_offset_points).abs() > point_tolerance
   {
      return Err(format!("run {} gesture did not start at the frozen captured offset", run.nonce));
   }
   if gesture.end_offset_points < -point_tolerance
      || gesture.end_offset_points > geometry.maximum_content_offset_points + point_tolerance
   {
      return Err(format!("run {} gesture ended outside the content range", run.nonce));
   }
   if (gesture.end_offset_points - gesture.start_offset_points - gesture.signed_travel_points).abs() > 1e-6
      || (gesture.signed_travel_points.abs() - gesture.travel_distance_points).abs() > 1e-6
   {
      return Err(format!("run {} travel fields are inconsistent", run.nonce));
   }
   if (run.direction == "forward" && gesture.signed_travel_points <= 0.0)
      || (run.direction == "reverse" && gesture.signed_travel_points >= 0.0)
   {
      return Err(format!("run {} traveled opposite the frozen direction", run.nonce));
   }
   if gesture.travel_distance_points < MINIMUM_TRAVEL_POINTS
   {
      return Err(format!(
         "run {} traveled {:.3} pt, below the frozen {:.0} pt minimum",
         run.nonce,
         gesture.travel_distance_points,
         MINIMUM_TRAVEL_POINTS
      ));
   }
   Ok(())
}

fn finite(values: &[f64], name: &str) -> Result<(), String>
{
   if values.iter().any(|value| !value.is_finite())
   {
      return Err(format!("{name} contains a non-finite value"));
   }
   Ok(())
}

fn rect_within(left: Rect, right: Rect, tolerance: i32) -> bool
{
   (left.x - right.x).abs() <= tolerance
      && (left.y - right.y).abs() <= tolerance
      && (left.x + left.width - right.x - right.width).abs() <= tolerance
      && (left.y + left.height - right.y - right.height).abs() <= tolerance
}

fn fill(image: &mut RgbaImage, rect: Rect, rgba: [u8; 4]) -> Result<(), String>
{
   let rect = bounded_rect(rect, image.width, image.height)?;
   for y in rect.y as u32 .. (rect.y + rect.height) as u32
   {
      for x in rect.x as u32 .. (rect.x + rect.width) as u32
      {
         let index = ((y * image.width + x) * 4) as usize;
         image.pixels[index .. index + 4].copy_from_slice(&rgba);
      }
   }
   Ok(())
}

fn component_rect<'a>(components: &'a [Component], kind: &str, from_end: bool) -> Result<Rect, String>
{
   let mut matches = components.iter().filter(|component| component.kind == kind);
   if from_end
   {
      matches.next_back().map(|component| component.viewport_clip_px)
   }
   else
   {
      matches.next().map(|component| component.viewport_clip_px)
   }
   .ok_or_else(|| format!("reference geometry has no visible {kind}"))
}

fn mutate_missing_row(reference: &RgbaImage, components: &[Component]) -> Result<RgbaImage, String>
{
   let mut image = reference.clone();
   fill(&mut image, component_rect(components, "row", false)?, [247, 244, 238, 255])?;
   Ok(image)
}

fn mutate_missing_caption(reference: &RgbaImage, components: &[Component]) -> Result<RgbaImage, String>
{
   let mut image = reference.clone();
   fill(&mut image, component_rect(components, "caption", false)?, [247, 244, 238, 255])?;
   Ok(image)
}

fn mutate_checker(reference: &RgbaImage, components: &[Component], missing: bool) -> Result<RgbaImage, String>
{
   let mut image = reference.clone();
   let color = if missing { [247, 244, 238, 255] } else { [16, 225, 237, 255] };
   fill(&mut image, component_rect(components, "image", false)?, color)?;
   Ok(image)
}

fn mutate_wrong_color(reference: &RgbaImage, components: &[Component]) -> Result<RgbaImage, String>
{
   let mut image = reference.clone();
   fill(&mut image, component_rect(components, "title", false)?, [210, 32, 190, 255])?;
   Ok(image)
}

fn mutate_shifted_image(reference: &RgbaImage, components: &[Component]) -> Result<RgbaImage, String>
{
   let source_rect = bounded_rect(component_rect(components, "image", false)?, reference.width, reference.height)?;
   let mut image = reference.clone();
   fill(&mut image, source_rect, [247, 244, 238, 255])?;
   let shift = 24_i32;
   for y in 0 .. source_rect.height
   {
      for x in 0 .. source_rect.width
      {
         let destination_x = source_rect.x + x + shift;
         if destination_x >= image.width as i32
         {
            continue;
         }
         let source_index = (((source_rect.y + y) as u32 * image.width + (source_rect.x + x) as u32) * 4) as usize;
         let destination_index = (((source_rect.y + y) as u32 * image.width + destination_x as u32) * 4) as usize;
         image.pixels[destination_index .. destination_index + 4]
            .copy_from_slice(&reference.pixels[source_index .. source_index + 4]);
      }
   }
   Ok(image)
}

fn mutate_half_image(reference: &RgbaImage, components: &[Component]) -> Result<RgbaImage, String>
{
   let rect = bounded_rect(component_rect(components, "image", false)?, reference.width, reference.height)?;
   let mut image = reference.clone();
   fill(&mut image, rect, [247, 244, 238, 255])?;
   let half_width = rect.width / 2;
   let half_height = rect.height / 2;
   for y in 0 .. half_height
   {
      for x in 0 .. half_width
      {
         let source_x = rect.x + x * 2;
         let source_y = rect.y + y * 2;
         let source_index = ((source_y as u32 * image.width + source_x as u32) * 4) as usize;
         let destination_index = (((rect.y + y) as u32 * image.width + (rect.x + x) as u32) * 4) as usize;
         image.pixels[destination_index .. destination_index + 4]
            .copy_from_slice(&reference.pixels[source_index .. source_index + 4]);
      }
   }
   Ok(image)
}

fn mutate_bad_clipping(reference: &RgbaImage, components: &[Component]) -> Result<RgbaImage, String>
{
   let mut image = reference.clone();
   let rect = component_rect(components, "row", true)?;
   let corrupt = Rect {
      x: rect.x,
      y: (rect.y + rect.height - 48).max(rect.y),
      width: rect.width,
      height: 48.min(rect.height),
   };
   fill(&mut image, corrupt, [1, 1, 1, 255])?;
   Ok(image)
}

fn mutate_corrupt_tile(reference: &RgbaImage) -> Result<RgbaImage, String>
{
   let mut image = reference.clone();
   let origin_x = (image.width / 2 / TILE_SIDE) * TILE_SIDE;
   let origin_y = (image.height / 2 / TILE_SIDE) * TILE_SIDE;
   for y in origin_y .. (origin_y + TILE_SIDE).min(image.height)
   {
      for x in origin_x .. (origin_x + TILE_SIDE).min(image.width)
      {
         let index = ((y * image.width + x) * 4) as usize;
         image.pixels[index] = 255 - image.pixels[index];
         image.pixels[index + 1] = 255 - image.pixels[index + 1];
         image.pixels[index + 2] = 255 - image.pixels[index + 2];
      }
   }
   Ok(image)
}

fn adversarial_gate(reference: &RgbaImage, components: &[Component]) -> Result<Vec<AdversarialResult>, String>
{
   let mut results = Vec::with_capacity(9);
   results.push(adversarial_result("missing-row", reference, mutate_missing_row(reference, components)?)?);
   results.push(adversarial_result("sparse-missing-caption", reference, mutate_missing_caption(reference, components)?)?);
   results.push(adversarial_result("wrong-checker-variant", reference, mutate_checker(reference, components, false)?)?);
   results.push(adversarial_result("missing-image", reference, mutate_checker(reference, components, true)?)?);
   results.push(adversarial_result("wrong-color", reference, mutate_wrong_color(reference, components)?)?);
   results.push(adversarial_result("shifted-image", reference, mutate_shifted_image(reference, components)?)?);
   results.push(adversarial_result("half-sized-image", reference, mutate_half_image(reference, components)?)?);
   results.push(adversarial_result("bad-clipping", reference, mutate_bad_clipping(reference, components)?)?);
   results.push(adversarial_result("localized-corrupt-tile", reference, mutate_corrupt_tile(reference)?)?);
   Ok(results)
}

fn adversarial_result(name: &str, reference: &RgbaImage, mutation: RgbaImage) -> Result<AdversarialResult, String>
{
   let metrics = visual_metrics(reference, &mutation)?;
   Ok(AdversarialResult {
      mutation: name.to_string(),
      rejected: !metrics.passes,
      ssim: metrics.ssim,
      worst_tile_rgb_mae: metrics.worst_tile_rgb_mae,
   })
}

fn expected_order_index(phase: &str, session: u32, pair: u32, treatment: &str) -> Result<u32, String>
{
   let treatment_index = match treatment
   {
      "uikit-idiomatic" => 0,
      "uikit-optimized" => 1,
      "oxide" => 2,
      _ => return Err(format!("unknown treatment {treatment} in order schedule")),
   };
   let rotation = match phase
   {
      "smoke" => 0,
      "primary" if session < 3 && pair < 3 => (session + pair) % 3,
      "primary" => return Err("primary order indices are outside the frozen population".to_string()),
      _ => return Err(format!("unknown phase {phase} in order schedule")),
   };
   Ok((treatment_index + 3 - rotation) % 3)
}

