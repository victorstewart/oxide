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

