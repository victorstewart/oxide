//! Bounded benchmark observation and atomic nonce completion transport.

use std::ffi::CString;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::contract::{
   self,
   FeedFixture,
   RowRecipe,
   StartState,
};

/// Maximum retained display-link samples in one run.
pub const MAX_CALLBACK_SAMPLES: usize = 1_024;
/// Maximum retained visible components in the admitted viewport.
pub const MAX_VISIBLE_COMPONENTS: usize = 96;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
/// One display-link callback and its declared target timestamp.
pub struct CallbackSample
{
   /// Callback timestamp in monotonic nanoseconds.
   pub timestamp_ns: u64,
   /// Target timestamp in monotonic nanoseconds.
   pub target_timestamp_ns: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
/// Renderer-observed content-space rectangle in points.
pub struct ObservedRect
{
   /// Left coordinate in points.
   pub x: f32,
   /// Top coordinate in points.
   pub y: f32,
   /// Width in points.
   pub width: f32,
   /// Height in points.
   pub height: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
/// One renderer-observed frozen component.
pub struct ObservedComponent
{
   /// Frozen row index.
   pub row_index: usize,
   /// Index into `contract::COMPONENT_KINDS`.
   pub kind_index: usize,
   /// Actual content-space component rectangle.
   pub content_rect_points: ObservedRect,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
/// Live device and configured display-link endpoint state.
pub struct DeviceState
{
   /// Native process thermal-state integer.
   pub thermal_state: i32,
   /// Whether Low Power Mode is active.
   pub low_power_mode: bool,
   /// Device maximum refresh rate in hertz.
   pub maximum_frames_per_second: u32,
   /// Configured display-link minimum rate in hertz.
   pub range_minimum_frames_per_second: f32,
   /// Configured display-link maximum rate in hertz.
   pub range_maximum_frames_per_second: f32,
   /// Configured display-link preferred rate in hertz.
   pub range_preferred_frames_per_second: f32,
}

impl DeviceState
{
   /// Returns the strict record spelling for the thermal integer.
   pub const fn thermal_name(self) -> &'static str
   {
      match self.thermal_state
      {
         0 => "nominal",
         1 => "fair",
         2 => "serious",
         3 => "critical",
         _ => "unknown",
      }
   }
}

/// Borrowed complete-run evidence written after the closing submit.
pub struct RunRecord<'a>
{
   /// Nonce-scoped run identity.
   pub nonce: &'a str,
   /// Controller phase, `smoke` or `primary`.
   pub phase: &'a str,
   /// Sampling session index.
   pub session_index: u32,
   /// Paired-run index within the session.
   pub pair_index: u32,
   /// Treatment order index within the pair.
   pub order_index: u32,
   /// Frozen feed endpoint.
   pub state: StartState,
   /// Complete deterministic fixture.
   pub fixture: &'a FeedFixture,
   /// Actual ready-frame content offset in points.
   pub captured_content_offset_points: f32,
   /// Actual admitted ready-frame components.
   pub visible_components: &'a [ObservedComponent],
   /// Content offset when measurement began.
   pub start_offset_points: f32,
   /// Content offset in the submitted closing frame.
   pub end_offset_points: f32,
   /// Raw-touch timestamp that crossed drag slop.
   pub gesture_start_timestamp_ns: u64,
   /// Callback timestamp at observed settlement.
   pub settled_timestamp_ns: u64,
   /// Whether Rust-owned scrolling entered its inertial phase.
   pub inertia_observed: bool,
   /// Device state sampled at ready admission.
   pub device_before: DeviceState,
   /// Device state sampled at completion.
   pub device_after: DeviceState,
   /// Thermal transitions observed from ready through completion.
   pub thermal_state_change_count: u32,
   /// Low Power Mode transitions observed from ready through completion.
   pub low_power_mode_change_count: u32,
   /// Bounded callback timing samples for the measured gesture.
   pub callbacks: &'a [CallbackSample],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct EnvironmentTransitionCounts
{
   pub thermal_state: u64,
   pub low_power_mode: u64,
}

/// Borrowed non-measurable terminal failure evidence.
pub struct FailureRecord<'a>
{
   /// Validated nonce when launch parsing established one.
   pub nonce: Option<&'a str>,
   /// Treatment value when launch parsing established one.
   pub treatment: Option<&'a str>,
   /// Protocol stage that observed the failure.
   pub stage: &'a str,
   /// Human-readable failure cause.
   pub message: &'a str,
}

#[derive(Debug)]
/// Validation, persistence, or notification failure.
pub enum ObservationError
{
   /// Nonce cannot safely identify a file and notification.
   InvalidNonce,
   /// Sandbox `HOME` was unavailable.
   MissingHome,
   /// Durable filesystem operation failed.
   Io(io::Error),
   /// Darwin notification returned a nonzero status.
   #[cfg(target_os = "ios")]
   Notification(u32),
}

impl fmt::Display for ObservationError
{
   fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result
   {
      match self
      {
         Self::InvalidNonce => formatter.write_str("completion nonce is invalid"),
         Self::MissingHome => formatter.write_str("iOS sandbox HOME is unavailable"),
         Self::Io(error) => write!(formatter, "observation I/O failed: {error}"),
         #[cfg(target_os = "ios")]
         Self::Notification(status) => write!(formatter, "Darwin notification failed: {status}"),
      }
   }
}

impl std::error::Error for ObservationError
{
}

impl From<io::Error> for ObservationError
{
   fn from(error: io::Error) -> Self
   {
      Self::Io(error)
   }
}

/// Captures live thermal, power, device-refresh, and display-link state.
pub fn capture_device_state() -> Option<DeviceState>
{
   #[cfg(target_os = "ios")]
   {
      // These process/device queries are side-effect free and are provided by
      // the linked production iOS host on the main app thread.
      let thermal_state = unsafe { oxide_host_thermal_state() };
      let low_power_mode = unsafe { oxide_host_power_lowpower() } != 0;
      let maximum_frames_per_second = unsafe { oxide_host_max_framerate_hz() }.max(1);
      let range = oxide_host_ios::display_link_frame_rate_range()?;
      if !range.minimum.is_finite()
         || !range.maximum.is_finite()
         || !range.preferred.is_finite()
         || range.minimum <= 0.0
         || range.maximum <= 0.0
         || range.preferred <= 0.0
      {
         return None;
      }
      return Some(DeviceState {
         thermal_state,
         low_power_mode,
         maximum_frames_per_second,
         range_minimum_frames_per_second: range.minimum,
         range_maximum_frames_per_second: range.maximum,
         range_preferred_frames_per_second: range.preferred,
      });
   }
   #[cfg(not(target_os = "ios"))]
   {
      Some(DeviceState {
         thermal_state: 0,
         low_power_mode: false,
         maximum_frames_per_second: 120,
         range_minimum_frames_per_second: 120.0,
         range_maximum_frames_per_second: 120.0,
         range_preferred_frames_per_second: 120.0,
      })
   }
}

pub(crate) fn capture_environment_transition_counts() -> EnvironmentTransitionCounts
{
   #[cfg(target_os = "ios")]
   {
      let mut counts = EnvironmentTransitionCounts::default();
      // Both pointers refer to distinct live `u64` values for the duration of
      // this side-effect-free cumulative-counter snapshot.
      unsafe
      {
         oxide_host_environment_transition_counts(
            &mut counts.thermal_state,
            &mut counts.low_power_mode,
         );
      }
      counts
   }
   #[cfg(not(target_os = "ios"))]
   {
      EnvironmentTransitionCounts::default()
   }
}

/// Posts the nonce-scoped ready notification.
pub fn post_ready(nonce: &str) -> Result<(), ObservationError>
{
   post_prefixed_notification(contract::READY_NOTIFICATION_PREFIX, nonce)
}

/// Posts the nonce-scoped failure notification.
pub fn post_failure(nonce: &str) -> Result<(), ObservationError>
{
   post_prefixed_notification(contract::FAILURE_NOTIFICATION_PREFIX, nonce)
}

/// Returns whether a nonce is safe for both paths and notifications.
pub fn nonce_is_valid(nonce: &str) -> bool
{
   validate_nonce(nonce).is_ok()
}

/// Durably persists a complete record, then posts completion.
pub fn persist_success_and_post(record: &RunRecord<'_>) -> Result<PathBuf, ObservationError>
{
   persist_and_post(
      record.nonce,
      contract::COMPLETION_NOTIFICATION_PREFIX,
      |writer| write_record_json(record, writer),
   )
}

/// Durably persists a failure record, then posts failure.
pub fn persist_failure_and_post(record: &FailureRecord<'_>) -> Result<PathBuf, ObservationError>
{
   let nonce = record.nonce.ok_or(ObservationError::InvalidNonce)?;
   persist_and_post(
      nonce,
      contract::FAILURE_NOTIFICATION_PREFIX,
      |writer| write_failure_json(record, writer),
   )
}

fn persist_and_post<F>(nonce: &str, notification_prefix: &str, write_json: F) -> Result<PathBuf, ObservationError>
where
   F: FnOnce(&mut dyn Write) -> io::Result<()>,
{
   validate_nonce(nonce)?;
   let directory = result_directory()?;
   let final_path = result_path_in(&directory, nonce)?;
   let temporary_path = directory.join(format!(
      ".{}{}{}.tmp",
      contract::RESULT_FILE_PREFIX,
      nonce,
      contract::RESULT_FILE_SUFFIX,
   ));
   fs::create_dir_all(&directory)?;
   let write_result = write_record_atomically(write_json, &temporary_path, &final_path, &directory);
   if write_result.is_err()
   {
      let _ = fs::remove_file(&temporary_path);
   }
   write_result?;
   post_prefixed_notification(notification_prefix, nonce)?;
   Ok(final_path)
}

fn result_directory() -> Result<PathBuf, ObservationError>
{
   let home = std::env::var_os("HOME").ok_or(ObservationError::MissingHome)?;
   Ok(PathBuf::from(home).join(contract::RESULT_DIRECTORY_NAME))
}

fn result_path_in(directory: &Path, nonce: &str) -> Result<PathBuf, ObservationError>
{
   validate_nonce(nonce)?;
   Ok(directory.join(format!(
      "{}{}{}",
      contract::RESULT_FILE_PREFIX,
      nonce,
      contract::RESULT_FILE_SUFFIX,
   )))
}

fn validate_nonce(nonce: &str) -> Result<(), ObservationError>
{
   let valid = !nonce.is_empty()
      && nonce.len() <= 128
      && nonce
         .bytes()
         .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
   if valid
   {
      Ok(())
   }
   else
   {
      Err(ObservationError::InvalidNonce)
   }
}

fn write_record_atomically<F>(write_json: F, temporary_path: &Path, final_path: &Path, directory: &Path) -> Result<(), ObservationError>
where
   F: FnOnce(&mut dyn Write) -> io::Result<()>,
{
   let file = OpenOptions::new().write(true).create_new(true).open(temporary_path)?;
   let mut writer = BufWriter::new(file);
   write_json(&mut writer)?;
   writer.flush()?;
   writer.get_ref().sync_all()?;
   drop(writer);
   fs::rename(temporary_path, final_path)?;
   File::open(directory)?.sync_all()?;
   Ok(())
}

/// Streams the strict complete-run JSON schema to `writer`.
pub fn write_record_json(record: &RunRecord<'_>, writer: &mut dyn Write) -> io::Result<()>
{
   writeln!(writer, "{{")?;
   write!(writer, "  \"schema\": ")?;
   write_json_string(writer, contract::RUN_RECORD_SCHEMA)?;
   writeln!(writer, ",")?;
   writeln!(writer, "  \"schema_revision\": {},", contract::RUN_RECORD_SCHEMA_REVISION)?;
   writeln!(writer, "  \"fixture\": {{")?;
   write!(writer, "    \"schema\": ")?;
   write_json_string(writer, contract::SCHEMA)?;
   writeln!(writer, ",")?;
   writeln!(writer, "    \"revision\": {},", contract::REVISION)?;
   write!(writer, "    \"canonical_sha256\": ")?;
   write_json_string(writer, contract::EXPECTED_CANONICAL_SHA256)?;
   writeln!(writer, ",")?;
   writeln!(
      writer,
      "    \"canonical_byte_count\": {}",
      contract::EXPECTED_CANONICAL_BYTE_COUNT,
   )?;
   writeln!(writer, "  }},")?;
   writeln!(writer, "  \"run\": {{")?;
   write_nested_string_field(writer, "nonce", record.nonce, true)?;
   write_nested_string_field(writer, "phase", record.phase, true)?;
   writeln!(writer, "    \"session_index\": {},", record.session_index)?;
   writeln!(writer, "    \"pair_index\": {},", record.pair_index)?;
   writeln!(writer, "    \"order_index\": {},", record.order_index)?;
   write_nested_string_field(writer, "treatment", contract::OXIDE_VARIANT_VALUE, true)?;
   write_nested_string_field(writer, "start_state", record.state.as_str(), true)?;
   write_nested_string_field(writer, "direction", record.state.direction(), false)?;
   writeln!(writer, "  }},")?;
   write!(
      writer,
      "  \"canvas\": {{\"host_width_points\": {}, \"host_height_points\": {}, \"surface_origin_x_points\": {}, \"surface_origin_y_points\": {}, \"surface_width_points\": {}, \"surface_height_points\": {}, \"scale\": {}}},\n",
      contract::HOST_WIDTH_POINTS,
      contract::HOST_HEIGHT_POINTS,
      contract::SURFACE_ORIGIN_X_POINTS,
      contract::SURFACE_ORIGIN_Y_POINTS,
      contract::SURFACE_WIDTH_POINTS,
      contract::SURFACE_HEIGHT_POINTS,
      contract::SURFACE_SCALE,
   )?;
   write!(
      writer,
      "  \"geometry\": {{\"row_count\": {}, \"manifest_component_count\": {}, \"content_extent_points\": {:.1}, \"maximum_content_offset_points\": {:.1}, \"captured_content_offset_points\": {:.6}, \"viewport_clip_px\": {{\"x\": 0, \"y\": 0, \"width\": {}, \"height\": {}}}, \"visible_components\": [\n",
      contract::ROW_COUNT,
      contract::ROW_COUNT * contract::COMPONENT_KINDS.len(),
      record.fixture.content_extent_points() as f32,
      record.fixture.maximum_content_offset_points() as f32,
      record.captured_content_offset_points,
      contract::SURFACE_WIDTH_POINTS * contract::SURFACE_SCALE,
      contract::SURFACE_HEIGHT_POINTS * contract::SURFACE_SCALE,
   )?;
   write_visible_components(record, writer)?;
   writeln!(writer, "  ]}},")?;
   writeln!(writer, "  \"environment\": {{")?;
   write_device_state(writer, "before", record.device_before, true)?;
   write_device_state(writer, "after", record.device_after, true)?;
   writeln!(
      writer,
      "    \"thermal_state_change_count\": {},",
      record.thermal_state_change_count,
   )?;
   writeln!(
      writer,
      "    \"low_power_mode_change_count\": {}",
      record.low_power_mode_change_count,
   )?;
   writeln!(writer, "  }},")?;
   let signed_travel = record.end_offset_points - record.start_offset_points;
   let duration_ns = record.settled_timestamp_ns.saturating_sub(record.gesture_start_timestamp_ns);
   write!(
      writer,
      "  \"gesture\": {{\"start_offset_points\": {:.6}, \"end_offset_points\": {:.6}, \"signed_travel_points\": {:.6}, \"travel_distance_points\": {:.6}, \"duration_seconds\": {:.10}, \"settled\": {}, \"inertia_observed\": {}}},\n",
      record.start_offset_points,
      record.end_offset_points,
      signed_travel,
      signed_travel.abs(),
      duration_ns as f64 / 1_000_000_000.0,
      true,
      record.inertia_observed,
   )?;
   writeln!(writer, "  \"display_link\": {{")?;
   writeln!(writer, "    \"clock\": \"CADisplayLink.timestamp/targetTimestamp\",")?;
   writeln!(writer, "    \"callback_only\": true,")?;
   write!(writer, "    \"samples\": [")?;
   for (index, sample) in record.callbacks.iter().copied().enumerate()
   {
      if index != 0
      {
         write!(writer, ",")?;
      }
      write!(
         writer,
         "{{\"timestamp_seconds\":{:.10},\"target_timestamp_seconds\":{:.10}}}",
         sample.timestamp_ns as f64 / 1_000_000_000.0,
         sample.target_timestamp_ns as f64 / 1_000_000_000.0,
      )?;
   }
   writeln!(writer, "]")?;
   writeln!(writer, "  }},")?;
   writeln!(writer, "  \"status\": \"complete\",")?;
   writeln!(writer, "  \"failure\": null")?;
   writeln!(writer, "}}")
}

/// Streams the strict non-measurable failure schema to `writer`.
pub fn write_failure_json(record: &FailureRecord<'_>, writer: &mut dyn Write) -> io::Result<()>
{
   writeln!(writer, "{{")?;
   write!(writer, "  \"schema\": ")?;
   write_json_string(writer, contract::FAILURE_RECORD_SCHEMA)?;
   writeln!(writer, ",")?;
   writeln!(writer, "  \"schema_revision\": {},", contract::RUN_RECORD_SCHEMA_REVISION)?;
   writeln!(writer, "  \"fixture\": {{")?;
   write!(writer, "    \"schema\": ")?;
   write_json_string(writer, contract::SCHEMA)?;
   writeln!(writer, ",")?;
   writeln!(writer, "    \"revision\": {},", contract::REVISION)?;
   write!(writer, "    \"canonical_sha256\": ")?;
   write_json_string(writer, contract::EXPECTED_CANONICAL_SHA256)?;
   writeln!(writer, ",")?;
   writeln!(
      writer,
      "    \"canonical_byte_count\": {}",
      contract::EXPECTED_CANONICAL_BYTE_COUNT,
   )?;
   writeln!(writer, "  }},")?;
   write_optional_string_field(writer, "nonce", record.nonce, true)?;
   write_optional_string_field(writer, "treatment", record.treatment, true)?;
   write_optional_string_field(writer, "stage", Some(record.stage), true)?;
   write_optional_string_field(writer, "message", Some(record.message), false)?;
   writeln!(writer, "}}")
}

fn write_visible_components(record: &RunRecord<'_>, writer: &mut dyn Write) -> io::Result<()>
{
   let mut first = true;
   let mut id = String::with_capacity(48);
   for component in record.visible_components.iter().copied()
   {
      let Some(row) = RowRecipe::at(component.row_index) else
      {
         continue;
      };
      let Some(kind) = contract::COMPONENT_KINDS.get(component.kind_index).copied() else
      {
         continue;
      };
      let Some(rect) = physical_rect(component.content_rect_points) else
      {
         continue;
      };
      let Some(clip) = viewport_clip(rect, record.captured_content_offset_points) else
      {
         continue;
      };
      if !first
      {
         writeln!(writer, ",")?;
      }
      first = false;
      row.write_component_id(kind, &mut id);
      write!(writer, "    {{\"id\":")?;
      write_json_string(writer, &id)?;
      write!(writer, ",\"kind\":")?;
      write_json_string(writer, kind)?;
      write!(
         writer,
         ",\"row_index\":{},\"content_rect_px\":{{\"x\":{},\"y\":{},\"width\":{},\"height\":{}}},\"viewport_clip_px\":{{\"x\":{},\"y\":{},\"width\":{},\"height\":{}}}}}",
         component.row_index,
         rect.x,
         rect.y,
         rect.width,
         rect.height,
         clip.x,
         clip.y,
         clip.width,
         clip.height,
      )?;
   }
   if !first
   {
      writeln!(writer)?;
   }
   Ok(())
}

fn write_device_state(writer: &mut dyn Write, name: &str, state: DeviceState, comma: bool) -> io::Result<()>
{
   write!(writer, "  ")?;
   write_json_string(writer, name)?;
   write!(
      writer,
      ": {{\"thermal_state\": \"{}\", \"low_power_mode\": {}, \"maximum_frames_per_second\": {}, \"configured_frame_rate\": {{\"minimum\": {:.1}, \"maximum\": {:.1}, \"preferred\": {:.1}}}}}{}\n",
      state.thermal_name(),
      state.low_power_mode,
      state.maximum_frames_per_second,
      state.range_minimum_frames_per_second,
      state.range_maximum_frames_per_second,
      state.range_preferred_frames_per_second,
      if comma { "," } else { "" },
   )
}

fn physical_rect(rect: ObservedRect) -> Option<contract::PhysicalRect>
{
   if !rect.x.is_finite()
      || !rect.y.is_finite()
      || !rect.width.is_finite()
      || !rect.height.is_finite()
      || rect.x < 0.0
      || rect.y < 0.0
      || rect.width <= 0.0
      || rect.height <= 0.0
   {
      return None;
   }
   let scale = contract::SURFACE_SCALE as f32;
   Some(contract::PhysicalRect {
      x: (rect.x * scale).round() as u32,
      y: (rect.y * scale).round() as u32,
      width: (rect.width * scale).round() as u32,
      height: (rect.height * scale).round() as u32,
   })
}

fn viewport_clip(rect: contract::PhysicalRect, offset_points: f32) -> Option<contract::PhysicalRect>
{
   let viewport_width = contract::SURFACE_WIDTH_POINTS * contract::SURFACE_SCALE;
   let viewport_height = contract::SURFACE_HEIGHT_POINTS * contract::SURFACE_SCALE;
   if !offset_points.is_finite() || offset_points < 0.0
   {
      return None;
   }
   let viewport_top = (offset_points * contract::SURFACE_SCALE as f32).round() as u32;
   let left = rect.x.min(viewport_width);
   let right = rect.x.saturating_add(rect.width).min(viewport_width);
   let top = rect.y.max(viewport_top);
   let bottom = rect
      .y
      .saturating_add(rect.height)
      .min(viewport_top.saturating_add(viewport_height));
   if left >= right || top >= bottom
   {
      return None;
   }
   Some(contract::PhysicalRect {
      x: left,
      y: top - viewport_top,
      width: right - left,
      height: bottom - top,
   })
}

fn write_nested_string_field(writer: &mut dyn Write, name: &str, value: &str, comma: bool) -> io::Result<()>
{
   write!(writer, "    ")?;
   write_json_string(writer, name)?;
   write!(writer, ": ")?;
   write_json_string(writer, value)?;
   writeln!(writer, "{}", if comma { "," } else { "" })
}

fn write_optional_string_field(writer: &mut dyn Write, name: &str, value: Option<&str>, comma: bool) -> io::Result<()>
{
   write!(writer, "  ")?;
   write_json_string(writer, name)?;
   write!(writer, ": ")?;
   if let Some(value) = value
   {
      write_json_string(writer, value)?;
   }
   else
   {
      write!(writer, "null")?;
   }
   writeln!(writer, "{}", if comma { "," } else { "" })
}

fn write_json_string(writer: &mut dyn Write, value: &str) -> io::Result<()>
{
   writer.write_all(b"\"")?;
   for character in value.chars()
   {
      match character
      {
         '"' => writer.write_all(b"\\\"")?,
         '\\' => writer.write_all(b"\\\\")?,
         '\n' => writer.write_all(b"\\n")?,
         '\r' => writer.write_all(b"\\r")?,
         '\t' => writer.write_all(b"\\t")?,
         value if value <= '\u{1f}' => write!(writer, "\\u{:04x}", value as u32)?,
         value => write!(writer, "{value}")?,
      }
   }
   writer.write_all(b"\"")
}

fn post_prefixed_notification(prefix: &str, nonce: &str) -> Result<(), ObservationError>
{
   validate_nonce(nonce)?;
   let name = CString::new(format!("{prefix}{nonce}")).map_err(|_| ObservationError::InvalidNonce)?;
   #[cfg(target_os = "ios")]
   {
      // `name` is a live NUL-terminated string for the duration of this call.
      let status = unsafe { notify_post(name.as_ptr()) };
      if status != 0
      {
         return Err(ObservationError::Notification(status));
      }
   }
   #[cfg(not(target_os = "ios"))]
   {
      let _ = name;
   }
   Ok(())
}

#[cfg(target_os = "ios")]
#[link(name = "System")]
extern "C"
{
   fn notify_post(name: *const core::ffi::c_char) -> u32;
   fn oxide_host_power_lowpower() -> i32;
   fn oxide_host_thermal_state() -> i32;
   fn oxide_host_max_framerate_hz() -> u32;
   fn oxide_host_environment_transition_counts(thermal: *mut u64, low_power: *mut u64);
}
