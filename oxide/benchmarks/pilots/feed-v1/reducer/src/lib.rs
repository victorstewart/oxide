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
const MAX_BUILD_ROOT_BYTES: u64 = 4_294_967_296;
const MAX_RETAINED_EVIDENCE_BYTES: u64 = 536_870_912;
const MAX_RETAINED_FILE_COUNT: usize = 512;
const MAX_RETAINED_FILE_BYTES: u64 = 134_217_728;
const MAX_RESULT_BUNDLE_BYTES: u64 = 536_870_912;
const QUANTILE_METHOD: &str = "one-based-nearest-rank";
const CONFIDENCE_INTERVAL_METHOD: &str = "exact-binomial-median";
const CONFIDENCE_TARGET_COVERAGE: f64 = 0.95;
// Omitting zero or one successes from each Binomial(9, 0.5) tail leaves 492 / 512 coverage.
const CONFIDENCE_ACHIEVED_COVERAGE: f64 = 492.0 / 512.0;
const CONFIDENCE_PAIR_COUNT: usize = 9;
const CONFIDENCE_LOWER_RANK: usize = 2;
const CONFIDENCE_UPPER_RANK: usize = 8;
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

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct EvidenceManifest
{
   schema: String,
   schema_revision: u32,
   fixture_sha256: String,
   repository_ref: String,
   repository_head_commit: String,
   repository_tree: String,
   build_provenance: BuildProvenance,
   source_files: BTreeMap<String, String>,
   uikit_app_sha256: String,
   oxide_app_sha256: String,
   controller_runner_sha256: String,
   controller_runner_binary_sha256: String,
   controller_xctest_sha256: String,
   controller_xctest_binary_sha256: String,
   reducer_binary_sha256: String,
   regular_font_sha256: String,
   bold_font_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BuildProvenance
{
   schema: String,
   schema_revision: u32,
   core_device_id: String,
   hardware_udid: String,
   device_model: String,
   device_product_type: String,
   os_version: String,
   os_build: String,
   maximum_refresh_hz: u32,
   xcode_version: String,
   xcode_build: String,
   iphoneos_sdk_version: String,
   iphoneos_sdk_build: String,
   rustc_release: String,
   rustc_commit_hash: String,
   rustc_host: String,
   cargo_version: String,
   release_build_settings_sha256: String,
   production_cargo_lock_sha256: String,
   production_cargo_metadata_sha256: String,
   uikit_signing: SigningIdentity,
   oxide_signing: SigningIdentity,
   controller_runner_signing: SigningIdentity,
   controller_xctest_signing: SigningIdentity,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SigningIdentity
{
   authority: String,
   team_identifier: String,
   cdhash: String,
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
struct CallbackMetrics
{
   callback_count: usize,
   achieved_cadence_hz: f64,
   interval_p50_ms: f64,
   interval_p95_ms: f64,
   interval_p99_ms: f64,
   interval_peak_ms: f64,
   missed_callback_deadlines: u64,
   expected_callbacks: u64,
   missed_callback_deadline_ratio: f64,
   callback_hitch_ms_per_elapsed_second: f64,
   target_period_admission_ratio: f64,
}

#[derive(Clone, Debug)]
struct MeasuredRun
{
   record: RunRecord,
   intervals: Vec<f64>,
   metrics: CallbackMetrics,
}

#[derive(Clone, Debug, Serialize)]
struct RunSummary
{
   nonce: String,
   phase: String,
   session_index: u32,
   pair_index: u32,
   order_index: u32,
   treatment: String,
   direction: String,
   gesture_duration_seconds: f64,
   inertia_observed: bool,
   thermal_state_change_count: u32,
   low_power_mode_change_count: u32,
   signed_travel_points: f64,
   travel_distance_points: f64,
   callback_count: usize,
   achieved_cadence_hz: f64,
   interval_p50_ms: f64,
   interval_p95_ms: f64,
   interval_p99_ms: f64,
   interval_peak_ms: f64,
   missed_callback_deadlines: u64,
   expected_callbacks: u64,
   missed_callback_deadline_ratio: f64,
   callback_hitch_ms_per_elapsed_second: f64,
   target_period_admission_ratio: f64,
   callback_samples: Vec<DisplaySample>,
}

#[derive(Clone, Debug, Serialize)]
struct ClusterSummary
{
   session_index: u32,
   pair_index: u32,
   run_count: usize,
   interval_p50_ms: f64,
   interval_p95_ms: f64,
}

#[derive(Clone, Debug, Serialize)]
struct TreatmentSummary
{
   treatment: String,
   run_count: usize,
   cluster_count: usize,
   aggregate_interval_p50_ms: f64,
   aggregate_interval_p95_ms: f64,
   interval_peak_ms: f64,
   missed_callback_deadline_ratio: f64,
   callback_hitch_ms_per_elapsed_second: f64,
   clusters: Vec<ClusterSummary>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct MedianConfidenceInterval
{
   pub method: &'static str,
   pub target_coverage: f64,
   pub achieved_coverage: f64,
   pub sample_count: usize,
   pub lower_rank: usize,
   pub upper_rank: usize,
   pub bounds: [f64; 2],
}

#[derive(Clone, Debug, Serialize)]
struct TravelEquivalenceResult
{
   treatment: String,
   direction: String,
   pair_count: usize,
   median_relative_delta: f64,
   confidence_interval: MedianConfidenceInterval,
   median_margin: f64,
   confidence_margin: f64,
   passes: bool,
}

#[derive(Clone, Debug, Default)]
struct TravelValidation
{
   results: Vec<TravelEquivalenceResult>,
   blockers: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct Comparison
{
   comparator: String,
   classification: String,
   median_pair_relative_p95_delta: f64,
   confidence_interval: MedianConfidenceInterval,
   oxide_aggregate_interval_p50_ms: f64,
   comparator_aggregate_interval_p50_ms: f64,
   oxide_aggregate_interval_p95_ms: f64,
   comparator_aggregate_interval_p95_ms: f64,
   oxide_missed_deadline_ratio: f64,
   comparator_missed_deadline_ratio: f64,
   oxide_hitch_ms_per_second: f64,
   comparator_hitch_ms_per_second: f64,
}

#[derive(Clone, Debug, Serialize)]
struct StatisticalPolicy
{
   callback_quantile_method: &'static str,
   callback_quantile_rank_formula: &'static str,
   cluster_identity_fields: [&'static str; 3],
   directions_per_cluster: usize,
   clusters_per_treatment: usize,
   treatment_aggregate_method: &'static str,
   confidence_interval_method: &'static str,
   confidence_target_coverage: f64,
   confidence_achieved_coverage: f64,
   confidence_sample_count: usize,
   confidence_lower_rank: usize,
   confidence_upper_rank: usize,
}

#[derive(Clone, Debug, Serialize)]
struct AdmissionThresholdPolicy
{
   visual_ssim_minimum: f64,
   visual_tile_side_px: u32,
   visual_worst_tile_rgb_mae_maximum: f64,
   component_edge_tolerance_px: i32,
   maximum_frame_rate_minimum_hz: u32,
   configured_frame_rate_hz: f64,
   target_period_minimum_ms: f64,
   target_period_maximum_ms: f64,
   target_period_admission_ratio_minimum: f64,
   minimum_travel_points: f64,
   travel_median_absolute_relative_delta_maximum: f64,
   travel_interval_absolute_bound_maximum: f64,
}

#[derive(Clone, Debug, Serialize)]
struct ClassificationThresholdPolicy
{
   slower_interval_lower_bound_exclusive: f64,
   faster_interval_upper_bound_exclusive: f64,
   faster_requires_lower_aggregate_p50: bool,
   faster_requires_lower_aggregate_p95: bool,
   non_inferior_interval_upper_bound_inclusive: f64,
   precedence: [&'static str; 4],
}

#[derive(Clone, Debug, Serialize)]
struct GuardrailPolicy
{
   missed_deadline_ratio_maximum_comparator_delta: f64,
   missed_deadline_ratio_absolute_maximum: f64,
   callback_hitch_ms_per_second_maximum_comparator_delta: f64,
   callback_hitch_ms_per_second_absolute_maximum: f64,
}

#[derive(Clone, Debug, Serialize)]
struct PublicationPolicy
{
   statistics: StatisticalPolicy,
   admission_thresholds: AdmissionThresholdPolicy,
   classification_thresholds: ClassificationThresholdPolicy,
   guardrails: GuardrailPolicy,
}

impl PublicationPolicy
{
   fn frozen() -> Self
   {
      Self {
         statistics: StatisticalPolicy {
            callback_quantile_method: QUANTILE_METHOD,
            callback_quantile_rank_formula: "ceil(q * n), one-based",
            cluster_identity_fields: ["treatment", "session_index", "pair_index"],
            directions_per_cluster: 2,
            clusters_per_treatment: CONFIDENCE_PAIR_COUNT,
            treatment_aggregate_method: "median-of-nine-cluster-quantiles",
            confidence_interval_method: CONFIDENCE_INTERVAL_METHOD,
            confidence_target_coverage: CONFIDENCE_TARGET_COVERAGE,
            confidence_achieved_coverage: CONFIDENCE_ACHIEVED_COVERAGE,
            confidence_sample_count: CONFIDENCE_PAIR_COUNT,
            confidence_lower_rank: CONFIDENCE_LOWER_RANK,
            confidence_upper_rank: CONFIDENCE_UPPER_RANK,
         },
         admission_thresholds: AdmissionThresholdPolicy {
            visual_ssim_minimum: SSIM_THRESHOLD,
            visual_tile_side_px: TILE_SIDE,
            visual_worst_tile_rgb_mae_maximum: TILE_MAE_THRESHOLD,
            component_edge_tolerance_px: COMPONENT_TOLERANCE_PX,
            maximum_frame_rate_minimum_hz: 120,
            configured_frame_rate_hz: 120.0,
            target_period_minimum_ms: PERIOD_MIN_SECONDS * 1_000.0,
            target_period_maximum_ms: PERIOD_MAX_SECONDS * 1_000.0,
            target_period_admission_ratio_minimum: PERIOD_ADMISSION_RATIO,
            minimum_travel_points: MINIMUM_TRAVEL_POINTS,
            travel_median_absolute_relative_delta_maximum: TRAVEL_MEDIAN_EQUIVALENCE_MARGIN,
            travel_interval_absolute_bound_maximum: TRAVEL_CONFIDENCE_EQUIVALENCE_MARGIN,
         },
         classification_thresholds: ClassificationThresholdPolicy {
            slower_interval_lower_bound_exclusive: NON_INFERIOR_MARGIN,
            faster_interval_upper_bound_exclusive: 0.0,
            faster_requires_lower_aggregate_p50: true,
            faster_requires_lower_aggregate_p95: true,
            non_inferior_interval_upper_bound_inclusive: NON_INFERIOR_MARGIN,
            precedence: ["slower", "faster", "non-inferior", "inconclusive"],
         },
         guardrails: GuardrailPolicy {
            missed_deadline_ratio_maximum_comparator_delta: MISSED_GUARDRAIL_DELTA,
            missed_deadline_ratio_absolute_maximum: MISSED_GUARDRAIL_ABSOLUTE,
            callback_hitch_ms_per_second_maximum_comparator_delta: HITCH_GUARDRAIL_DELTA_MS_S,
            callback_hitch_ms_per_second_absolute_maximum: HITCH_GUARDRAIL_ABSOLUTE_MS_S,
         },
      }
   }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct EvidenceFile
{
   relative_path: String,
   bytes: u64,
   sha256: String,
}

#[derive(Clone, Debug, Serialize)]
struct AdversarialResult
{
   mutation: String,
   rejected: bool,
   ssim: f64,
   worst_tile_rgb_mae: f64,
}

#[derive(Clone, Debug, Serialize)]
struct VisualTreatmentResult
{
   state: String,
   treatment: String,
   metrics: VisualMetrics,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct DeviceSummary
{
   marketing_name: String,
   product_type: String,
   os_version: String,
   os_build: String,
   cpu: String,
}

#[derive(Clone, Debug)]
struct DeviceEvidence
{
   identifier: String,
   hardware_udid: String,
   summary: DeviceSummary,
}

#[derive(Clone, Debug, Serialize)]
struct Report
{
   schema: &'static str,
   schema_revision: u32,
   fixture_sha256: &'static str,
   fixture_byte_count: u64,
   device: Option<DeviceSummary>,
   status: String,
   decision: String,
   policy: PublicationPolicy,
   blockers: Vec<String>,
   visual_gate_id: &'static str,
   visual_gate_spec_sha256: String,
   visual_gate_source_sha256: Option<String>,
   repository_ref: Option<String>,
   repository_head_commit: Option<String>,
   repository_tree: Option<String>,
   visual_treatments: Vec<VisualTreatmentResult>,
   adversarial_results: Vec<AdversarialResult>,
   travel_equivalence: Vec<TravelEquivalenceResult>,
   treatments: Vec<TreatmentSummary>,
   comparisons: Vec<Comparison>,
   runs: Vec<RunSummary>,
   missing_metrics: Vec<&'static str>,
   cleanup: CleanupEvidence,
   evidence_inventory: Vec<EvidenceFile>,
   evidence_manifest_sha256: Option<String>,
   retained_input_bytes: u64,
   run_count_total: usize,
   run_count_primary: usize,
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

pub fn exact_median_confidence_interval(pair_deltas: &[f64]) -> Result<MedianConfidenceInterval, String>
{
   exact_median_analysis(pair_deltas).map(|(_, interval)| interval)
}

fn exact_median_analysis(pair_deltas: &[f64]) -> Result<(f64, MedianConfidenceInterval), String>
{
   if pair_deltas.len() != CONFIDENCE_PAIR_COUNT
   {
      return Err(format!(
         "exact median confidence interval has {} gesture pairs, expected {CONFIDENCE_PAIR_COUNT}",
         pair_deltas.len()
      ));
   }
   finite(pair_deltas, "pair deltas")?;
   let mut sorted = pair_deltas.to_vec();
   sorted.sort_by(f64::total_cmp);
   let median = sorted[CONFIDENCE_PAIR_COUNT / 2];
   Ok((median, MedianConfidenceInterval {
      method: CONFIDENCE_INTERVAL_METHOD,
      target_coverage: CONFIDENCE_TARGET_COVERAGE,
      achieved_coverage: CONFIDENCE_ACHIEVED_COVERAGE,
      sample_count: CONFIDENCE_PAIR_COUNT,
      lower_rank: CONFIDENCE_LOWER_RANK,
      upper_rank: CONFIDENCE_UPPER_RANK,
      bounds: [sorted[CONFIDENCE_LOWER_RANK - 1], sorted[CONFIDENCE_UPPER_RANK - 1]],
   }))
}

pub fn frozen_order_index(phase: &str, session: u32, pair: u32, treatment: &str) -> Result<u32, String>
{
   expected_order_index(phase, session, pair, treatment)
}

pub fn callback_deadline_counts(samples: &[(f64, f64)]) -> Result<(u64, u64), String>
{
   let samples: Vec<DisplaySample> = samples.iter().map(|sample| DisplaySample {
      timestamp_seconds: sample.0,
      target_timestamp_seconds: sample.1,
   }).collect();
   let (_, metrics) = callback_metrics(&samples)?;
   Ok((metrics.missed_callback_deadlines, metrics.expected_callbacks))
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

fn callback_metrics(samples: &[DisplaySample]) -> Result<(Vec<f64>, CallbackMetrics), String>
{
   if samples.len() < 2
   {
      return Err("callback metrics require two samples".to_string());
   }
   for sample in samples
   {
      finite(&[sample.timestamp_seconds, sample.target_timestamp_seconds], "display-link sample")?;
      if sample.target_timestamp_seconds <= sample.timestamp_seconds
      {
         return Err("display-link target timestamp is not after its callback timestamp".to_string());
      }
   }
   let mut intervals = Vec::with_capacity(samples.len() - 1);
   let mut admitted_periods = 0_usize;
   let mut missed = 0_u64;
   let mut expected = 0_u64;
   let mut hitch_seconds = 0.0;
   for index in 1 .. samples.len()
   {
      let current = samples[index];
      let previous = samples[index - 1];
      let interval = current.timestamp_seconds - previous.timestamp_seconds;
      let period = previous.target_timestamp_seconds - previous.timestamp_seconds;
      if interval <= 0.0 || period <= 0.0
      {
         return Err("display-link interval or target period is not positive".to_string());
      }
      if (PERIOD_MIN_SECONDS ..= PERIOD_MAX_SECONDS).contains(&period)
      {
         admitted_periods += 1;
      }
      let callback_slots = (interval / period).round().max(1.0) as u64;
      expected += callback_slots;
      missed += callback_slots.saturating_sub(1);
      hitch_seconds += (interval - period).max(0.0);
      intervals.push(interval);
   }
   let elapsed = samples[samples.len() - 1].timestamp_seconds - samples[0].timestamp_seconds;
   if elapsed <= 0.0
   {
      return Err("display-link elapsed time is not positive".to_string());
   }
   let admission_ratio = admitted_periods as f64 / intervals.len() as f64;
   let metrics = CallbackMetrics {
      callback_count: samples.len(),
      achieved_cadence_hz: intervals.len() as f64 / elapsed,
      interval_p50_ms: nearest_rank_quantile(&intervals, 0.50)? * 1_000.0,
      interval_p95_ms: nearest_rank_quantile(&intervals, 0.95)? * 1_000.0,
      interval_p99_ms: nearest_rank_quantile(&intervals, 0.99)? * 1_000.0,
      interval_peak_ms: intervals.iter().copied().fold(0.0, f64::max) * 1_000.0,
      missed_callback_deadlines: missed,
      expected_callbacks: expected,
      missed_callback_deadline_ratio: if expected == 0 { 0.0 } else { missed as f64 / expected as f64 },
      callback_hitch_ms_per_elapsed_second: hitch_seconds * 1_000.0 / elapsed,
      target_period_admission_ratio: admission_ratio,
   };
   if metrics.target_period_admission_ratio < PERIOD_ADMISSION_RATIO
   {
      return Err(format!(
         "only {:.2}% of target periods are inside 7.5-9.2 ms",
         metrics.target_period_admission_ratio * 100.0
      ));
   }
   Ok((intervals, metrics))
}

pub fn nearest_rank_quantile(values: &[f64], quantile: f64) -> Result<f64, String>
{
   if values.is_empty() || !quantile.is_finite() || quantile <= 0.0 || quantile > 1.0
   {
      return Err(format!("invalid {QUANTILE_METHOD} input"));
   }
   finite(values, QUANTILE_METHOD)?;
   let mut sorted = values.to_vec();
   sorted.sort_by(f64::total_cmp);
   let rank = (quantile * sorted.len() as f64).ceil() as usize;
   Ok(sorted[rank - 1])
}

fn geometry_matches(reference: &Geometry, candidate: &Geometry) -> bool
{
   if reference.visible_components.len() != candidate.visible_components.len()
      || (reference.content_extent_points - candidate.content_extent_points).abs() > 1.0 / f64::from(SCALE)
      || (reference.maximum_content_offset_points - candidate.maximum_content_offset_points).abs()
         > 1.0 / f64::from(SCALE)
      || (reference.captured_content_offset_points - candidate.captured_content_offset_points).abs()
         > 1.0 / f64::from(SCALE)
   {
      return false;
   }
   reference.visible_components.iter().zip(&candidate.visible_components).all(|(left, right)| {
      left.id == right.id
         && left.kind == right.kind
         && left.row_index == right.row_index
         && rect_within(left.content_rect_px, right.content_rect_px, COMPONENT_TOLERANCE_PX)
         && rect_within(left.viewport_clip_px, right.viewport_clip_px, COMPONENT_TOLERANCE_PX)
   })
}

fn rect_within(left: Rect, right: Rect, tolerance: i32) -> bool
{
   (left.x - right.x).abs() <= tolerance
      && (left.y - right.y).abs() <= tolerance
      && (left.x + left.width - right.x - right.width).abs() <= tolerance
      && (left.y + left.height - right.y - right.height).abs() <= tolerance
}

fn validate_travel(runs: &[MeasuredRun], population: Population) -> TravelValidation
{
   let mut validation = TravelValidation::default();
   if population == Population::Smoke
   {
      return validation;
   }
   for treatment in ["uikit-optimized", "oxide"]
   {
      for direction in ["forward", "reverse"]
      {
         let candidates = runs.iter().filter(|run| {
            run.record.run.phase == "primary"
               && run.record.run.treatment == treatment
               && run.record.run.direction == direction
         });
         let mut deltas = Vec::with_capacity(9);
         for candidate in candidates
         {
            let reference = runs.iter().find(|reference| {
               reference.record.run.phase == "primary"
                  && reference.record.run.treatment == "uikit-idiomatic"
                  && reference.record.run.session_index == candidate.record.run.session_index
                  && reference.record.run.pair_index == candidate.record.run.pair_index
                  && reference.record.run.direction == direction
            });
            let Some(reference) = reference else
            {
               validation.blockers.push(format!("missing idiomatic UIKit travel match for {}", candidate.record.run.nonce));
               continue;
            };
            deltas.push(
               candidate.record.gesture.travel_distance_points
                  / reference.record.gesture.travel_distance_points
                  - 1.0,
            );
         }
         if deltas.len() != 9
         {
            validation.blockers.push(format!(
               "primary travel equivalence for {treatment} {direction} has {} pairs, expected 9",
               deltas.len()
            ));
            continue;
         }
         let (median, confidence_interval) = match exact_median_analysis(&deltas)
         {
            Ok(analysis) => analysis,
            Err(error) =>
            {
               validation.blockers.push(format!("primary travel equivalence confidence interval failed for {treatment} {direction}: {error}"));
               continue;
            }
         };
         let passes = travel_equivalence_passes(
            median,
            confidence_interval.bounds[0],
            confidence_interval.bounds[1],
         );
         validation.results.push(TravelEquivalenceResult {
            treatment: treatment.to_string(),
            direction: direction.to_string(),
            pair_count: deltas.len(),
            median_relative_delta: median,
            confidence_interval,
            median_margin: TRAVEL_MEDIAN_EQUIVALENCE_MARGIN,
            confidence_margin: TRAVEL_CONFIDENCE_EQUIVALENCE_MARGIN,
            passes,
         });
         if !passes
         {
            validation.blockers.push(format!(
               "primary travel equivalence {treatment} {direction}: median {:+.2}% and 95% interval [{:+.2}%, {:+.2}%] violate the frozen 5%/10% margins",
               median * 100.0,
               confidence_interval.bounds[0] * 100.0,
               confidence_interval.bounds[1] * 100.0
            ));
         }
      }
   }
   validation
}

pub fn travel_equivalence_passes(median: f64, lower: f64, upper: f64) -> bool
{
   median.abs() <= TRAVEL_MEDIAN_EQUIVALENCE_MARGIN + TRAVEL_EQUIVALENCE_EPSILON
      && lower >= -TRAVEL_CONFIDENCE_EQUIVALENCE_MARGIN - TRAVEL_EQUIVALENCE_EPSILON
      && upper <= TRAVEL_CONFIDENCE_EQUIVALENCE_MARGIN + TRAVEL_EQUIVALENCE_EPSILON
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

fn treatment_summary(runs: &[&MeasuredRun], treatment: &str) -> Result<TreatmentSummary, String>
{
   if runs.len() != CONFIDENCE_PAIR_COUNT * 2
   {
      return Err(format!(
         "{treatment} has {} primary runs, expected {}",
         runs.len(),
         CONFIDENCE_PAIR_COUNT * 2
      ));
   }
   let mut clusters = Vec::with_capacity(CONFIDENCE_PAIR_COUNT);
   let mut cluster_p50s = Vec::with_capacity(CONFIDENCE_PAIR_COUNT);
   let mut cluster_p95s = Vec::with_capacity(CONFIDENCE_PAIR_COUNT);
   let mut peak = 0.0_f64;
   let mut missed = 0_u64;
   let mut expected = 0_u64;
   let mut elapsed = 0.0;
   let mut hitch_ms = 0.0;
   for run in runs
   {
      peak = peak.max(run.intervals.iter().copied().fold(0.0, f64::max));
      missed += run.metrics.missed_callback_deadlines;
      expected += run.metrics.expected_callbacks;
      let run_elapsed: f64 = run.intervals.iter().sum();
      elapsed += run_elapsed;
      hitch_ms += run.metrics.callback_hitch_ms_per_elapsed_second * run_elapsed;
   }
   for session_index in 0 .. 3
   {
      for pair_index in 0 .. 3
      {
         let interval_p50_ms = cluster_quantile(runs, treatment, session_index, pair_index, 0.50)? * 1_000.0;
         let interval_p95_ms = cluster_quantile(runs, treatment, session_index, pair_index, 0.95)? * 1_000.0;
         cluster_p50s.push(interval_p50_ms);
         cluster_p95s.push(interval_p95_ms);
         clusters.push(ClusterSummary {
            session_index,
            pair_index,
            run_count: 2,
            interval_p50_ms,
            interval_p95_ms,
         });
      }
   }
   Ok(TreatmentSummary {
      treatment: treatment.to_string(),
      run_count: runs.len(),
      cluster_count: clusters.len(),
      aggregate_interval_p50_ms: nearest_rank_quantile(&cluster_p50s, 0.50)?,
      aggregate_interval_p95_ms: nearest_rank_quantile(&cluster_p95s, 0.50)?,
      interval_peak_ms: peak * 1_000.0,
      missed_callback_deadline_ratio: if expected == 0 { 0.0 } else { missed as f64 / expected as f64 },
      callback_hitch_ms_per_elapsed_second: if elapsed == 0.0 { 0.0 } else { hitch_ms / elapsed },
      clusters,
   })
}

fn run_summary(run: &MeasuredRun) -> RunSummary
{
   RunSummary {
      nonce: run.record.run.nonce.clone(),
      phase: run.record.run.phase.clone(),
      session_index: run.record.run.session_index,
      pair_index: run.record.run.pair_index,
      order_index: run.record.run.order_index,
      treatment: run.record.run.treatment.clone(),
      direction: run.record.run.direction.clone(),
      gesture_duration_seconds: run.record.gesture.duration_seconds,
      inertia_observed: run.record.gesture.inertia_observed,
      thermal_state_change_count: run.record.environment.thermal_state_change_count,
      low_power_mode_change_count: run.record.environment.low_power_mode_change_count,
      signed_travel_points: run.record.gesture.signed_travel_points,
      travel_distance_points: run.record.gesture.travel_distance_points,
      callback_count: run.metrics.callback_count,
      achieved_cadence_hz: run.metrics.achieved_cadence_hz,
      interval_p50_ms: run.metrics.interval_p50_ms,
      interval_p95_ms: run.metrics.interval_p95_ms,
      interval_p99_ms: run.metrics.interval_p99_ms,
      interval_peak_ms: run.metrics.interval_peak_ms,
      missed_callback_deadlines: run.metrics.missed_callback_deadlines,
      expected_callbacks: run.metrics.expected_callbacks,
      missed_callback_deadline_ratio: run.metrics.missed_callback_deadline_ratio,
      callback_hitch_ms_per_elapsed_second: run.metrics.callback_hitch_ms_per_elapsed_second,
      target_period_admission_ratio: run.metrics.target_period_admission_ratio,
      callback_samples: run.record.display_link.samples.clone(),
   }
}

fn two_comparator_decision(comparisons: &[Comparison]) -> Result<String, String>
{
   let idiomatic = comparisons.iter().find(|comparison| comparison.comparator == "uikit-idiomatic")
      .ok_or_else(|| "missing idiomatic UIKit classification".to_string())?;
   let optimized = comparisons.iter().find(|comparison| comparison.comparator == "uikit-optimized")
      .ok_or_else(|| "missing optimized UIKit classification".to_string())?;
   Ok(format!(
      "uikit-idiomatic={};uikit-optimized={}",
      idiomatic.classification,
      optimized.classification
   ))
}

fn cluster_quantile(runs: &[&MeasuredRun], treatment: &str, session: u32, pair: u32, quantile: f64) -> Result<f64, String>
{
   let selected: Vec<&&MeasuredRun> = runs.iter().filter(|run| {
      run.record.run.treatment == treatment
         && run.record.run.session_index == session
         && run.record.run.pair_index == pair
   }).collect();
   if selected.len() != 2
   {
      return Err(format!("{treatment} session {session} pair {pair} does not have two directions"));
   }
   let directions: BTreeSet<&str> = selected.iter().map(|run| run.record.run.direction.as_str()).collect();
   if directions != BTreeSet::from(["forward", "reverse"])
   {
      return Err(format!("{treatment} session {session} pair {pair} direction set mismatch"));
   }
   let mut intervals = Vec::new();
   for run in selected
   {
      intervals.extend_from_slice(&run.intervals);
   }
   nearest_rank_quantile(&intervals, quantile)
}

fn comparison(runs: &[&MeasuredRun], summaries: &[TreatmentSummary], comparator: &str, policy: &PublicationPolicy) -> Result<Comparison, String>
{
   let oxide = summaries.iter().find(|summary| summary.treatment == "oxide")
      .ok_or_else(|| "missing Oxide treatment summary".to_string())?;
   let native = summaries.iter().find(|summary| summary.treatment == comparator)
      .ok_or_else(|| format!("missing {comparator} summary"))?;
   let mut deltas = Vec::with_capacity(9);
   for session in 0 .. 3
   {
      for pair in 0 .. 3
      {
         let oxide_p95 = cluster_quantile(runs, "oxide", session, pair, 0.95)?;
         let comparator_p95 = cluster_quantile(runs, comparator, session, pair, 0.95)?;
         if comparator_p95 <= 0.0
         {
            return Err("comparator pair p95 is not positive".to_string());
         }
         deltas.push((oxide_p95 / comparator_p95) - 1.0);
      }
   }
   let (median_delta, confidence_interval) = exact_median_analysis(&deltas)?;
   let thresholds = &policy.classification_thresholds;
   let guardrails = &policy.guardrails;
   let missed_guardrail = oxide.missed_callback_deadline_ratio
      <= native.missed_callback_deadline_ratio + guardrails.missed_deadline_ratio_maximum_comparator_delta
      && oxide.missed_callback_deadline_ratio <= guardrails.missed_deadline_ratio_absolute_maximum;
   let hitch_guardrail = oxide.callback_hitch_ms_per_elapsed_second
      <= native.callback_hitch_ms_per_elapsed_second
         + guardrails.callback_hitch_ms_per_second_maximum_comparator_delta
      && oxide.callback_hitch_ms_per_elapsed_second
         <= guardrails.callback_hitch_ms_per_second_absolute_maximum;
   let aggregate_p50_passes = !thresholds.faster_requires_lower_aggregate_p50
      || oxide.aggregate_interval_p50_ms < native.aggregate_interval_p50_ms;
   let aggregate_p95_passes = !thresholds.faster_requires_lower_aggregate_p95
      || oxide.aggregate_interval_p95_ms < native.aggregate_interval_p95_ms;
   let classification = if confidence_interval.bounds[0] > thresholds.slower_interval_lower_bound_exclusive
      || !missed_guardrail
      || !hitch_guardrail
   {
      "slower"
   }
   else if aggregate_p50_passes
      && aggregate_p95_passes
      && confidence_interval.bounds[1] < thresholds.faster_interval_upper_bound_exclusive
   {
      "faster"
   }
   else if confidence_interval.bounds[1] <= thresholds.non_inferior_interval_upper_bound_inclusive
      && missed_guardrail
      && hitch_guardrail
   {
      "non-inferior"
   }
   else
   {
      "inconclusive"
   };
   Ok(Comparison {
      comparator: comparator.to_string(),
      classification: classification.to_string(),
      median_pair_relative_p95_delta: median_delta,
      confidence_interval,
      oxide_aggregate_interval_p50_ms: oxide.aggregate_interval_p50_ms,
      comparator_aggregate_interval_p50_ms: native.aggregate_interval_p50_ms,
      oxide_aggregate_interval_p95_ms: oxide.aggregate_interval_p95_ms,
      comparator_aggregate_interval_p95_ms: native.aggregate_interval_p95_ms,
      oxide_missed_deadline_ratio: oxide.missed_callback_deadline_ratio,
      comparator_missed_deadline_ratio: native.missed_callback_deadline_ratio,
      oxide_hitch_ms_per_second: oxide.callback_hitch_ms_per_elapsed_second,
      comparator_hitch_ms_per_second: native.callback_hitch_ms_per_elapsed_second,
   })
}

fn validate_population(runs: &[MeasuredRun], population: Population) -> Vec<String>
{
   let mut blockers = Vec::new();
   let expected_total = if population == Population::Full { 60 } else { 6 };
   if runs.len() != expected_total
   {
      blockers.push(format!("run population is {}, expected {expected_total}", runs.len()));
   }
   let mut expected_runs = BTreeSet::new();
   for treatment in ["uikit-idiomatic", "uikit-optimized", "oxide"]
   {
      for direction in ["forward", "reverse"]
      {
         let smoke = ("smoke".to_string(), 0, 0, treatment.to_string(), direction.to_string());
         expected_runs.insert(smoke);
         if population == Population::Full
         {
            for session in 0 .. 3
            {
               for pair in 0 .. 3
               {
                  expected_runs.insert(("primary".to_string(), session, pair, treatment.to_string(), direction.to_string()));
               }
            }
         }
      }
   }
   let observed: BTreeSet<_> = runs.iter().map(|run| (
      run.record.run.phase.clone(),
      run.record.run.session_index,
      run.record.run.pair_index,
      run.record.run.treatment.clone(),
      run.record.run.direction.clone(),
   )).collect();
   if observed != expected_runs || observed.len() != runs.len()
   {
      blockers.push("run tuple population is missing, duplicated, or out of scope".to_string());
   }
   if population == Population::Full
   {
      for session in 0 .. 3
      {
         for pair in 0 .. 3
         {
            let block: Vec<&MeasuredRun> = runs.iter().filter(|run| {
               run.record.run.phase == "primary"
                  && run.record.run.session_index == session
                  && run.record.run.pair_index == pair
                  && run.record.run.direction == "forward"
            }).collect();
            let orders: BTreeSet<u32> = block.iter().map(|run| run.record.run.order_index).collect();
            if block.len() != 3 || orders != BTreeSet::from([0, 1, 2])
            {
               blockers.push(format!("session {session} pair {pair} treatment order is not balanced"));
            }
         }
      }
   }
   for run in runs
   {
      match expected_order_index(
         &run.record.run.phase,
         run.record.run.session_index,
         run.record.run.pair_index,
         &run.record.run.treatment,
      )
      {
         Ok(expected_order) if run.record.run.order_index == expected_order => {}
         Ok(expected_order) => blockers.push(format!(
            "run {} order {} differs from frozen order {}",
            run.record.run.nonce,
            run.record.run.order_index,
            expected_order
         )),
         Err(error) => blockers.push(error),
      }
   }
   blockers
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CleanupProof
{
   schema: String,
   schema_revision: u32,
   test_succeeded: bool,
   verified_attachment_count: usize,
   apps_uninstalled: bool,
   controller_uninstalled: bool,
   uikit_process_absent: bool,
   oxide_process_absent: bool,
   controller_process_absent: bool,
   prelaunch_fuses_admitted: bool,
   resource_limits: ResourceLimits,
   source_snapshot_preserved: bool,
   external_build_removed: bool,
   result_bundle_removed: bool,
   reducer_binary_absent_from_result_root: bool,
   external_build_bytes: u64,
   result_bundle_bytes: u64,
   raw_evidence_bytes: u64,
   retained_file_count: usize,
   largest_retained_file_bytes: u64,
   runtime_seconds: f64,
}

#[derive(Clone, Debug, Serialize)]
struct CleanupEvidence
{
   admitted: bool,
   proof: Option<CleanupProof>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ResourceLimits
{
   external_build_bytes: u64,
   result_bundle_bytes: u64,
   retained_evidence_bytes: u64,
   retained_file_count: usize,
   retained_file_bytes: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ControllerRuntimeProof
{
   schema: String,
   schema_revision: u32,
   mode: String,
   total_runtime_seconds: f64,
   uikit_idiomatic_runtime_seconds: f64,
   uikit_optimized_runtime_seconds: f64,
   oxide_runtime_seconds: f64,
}

fn validate_controller_runtime(proof: &ControllerRuntimeProof, population: Population) -> Result<(), String>
{
   let expected_mode = if population == Population::Full { "full" } else { "smoke" };
   let runtimes = [
      proof.total_runtime_seconds,
      proof.uikit_idiomatic_runtime_seconds,
      proof.uikit_optimized_runtime_seconds,
      proof.oxide_runtime_seconds,
   ];
   finite(&runtimes, "controller runtime proof")?;
   let treatment_total = proof.uikit_idiomatic_runtime_seconds
      + proof.uikit_optimized_runtime_seconds
      + proof.oxide_runtime_seconds;
   if proof.schema != "oxide.feed-v1.controller-runtime"
      || proof.schema_revision != 1
      || proof.mode != expected_mode
      || runtimes.iter().any(|runtime| *runtime <= 0.0)
      || proof.total_runtime_seconds > 20.0 * 60.0
      || proof.uikit_idiomatic_runtime_seconds > 10.0 * 60.0
      || proof.uikit_optimized_runtime_seconds > 10.0 * 60.0
      || proof.oxide_runtime_seconds > 10.0 * 60.0
      || treatment_total > proof.total_runtime_seconds + 1.0
   {
      return Err("controller runtime proof violates the frozen 20/10-minute limits".to_string());
   }
   Ok(())
}

fn required_json_string(value: &serde_json::Value, pointer: &str, field: &str) -> Result<String, String>
{
   value.pointer(pointer).and_then(serde_json::Value::as_str)
      .filter(|value| !value.is_empty())
      .map(str::to_string)
      .ok_or_else(|| format!("device evidence is missing {field}"))
}

fn parse_device_details(path: &Path) -> Result<DeviceEvidence, String>
{
   let bytes = fs::read(path).map_err(|error| format!("read device evidence {}: {error}", path.display()))?;
   let value: serde_json::Value = serde_json::from_slice(&bytes)
      .map_err(|error| format!("parse device evidence {}: {error}", path.display()))?;
   let identifier = required_json_string(&value, "/result/identifier", "identifier")?;
   let hardware_udid = required_json_string(&value, "/result/hardwareProperties/udid", "hardware UDID")?;
   let marketing_name = required_json_string(&value, "/result/hardwareProperties/marketingName", "marketing name")?;
   let product_type = required_json_string(&value, "/result/hardwareProperties/productType", "product type")?;
   let reality = required_json_string(&value, "/result/hardwareProperties/reality", "hardware reality")?;
   let platform = required_json_string(&value, "/result/hardwareProperties/platform", "platform")?;
   let device_type = required_json_string(&value, "/result/hardwareProperties/deviceType", "device type")?;
   let cpu = required_json_string(&value, "/result/hardwareProperties/cpuType/name", "CPU type")?;
   let os_version = required_json_string(&value, "/result/deviceProperties/osVersionNumber", "OS version")?;
   let os_build = required_json_string(&value, "/result/deviceProperties/osBuildUpdate", "OS build")?;
   let boot_state = required_json_string(&value, "/result/deviceProperties/bootState", "boot state")?;
   let supported_cpus = value.pointer("/result/hardwareProperties/supportedCPUTypes")
      .and_then(serde_json::Value::as_array)
      .ok_or_else(|| "device evidence is missing supported CPU types".to_string())?;
   let supports_arm64 = supported_cpus.iter().any(|entry| {
      entry.get("name").and_then(serde_json::Value::as_str) == Some("arm64")
   });
   if reality != "physical"
      || platform != "iOS"
      || device_type != "iPhone"
      || boot_state != "booted"
      || !cpu.starts_with("arm64")
      || !supports_arm64
   {
      return Err("device evidence is not a booted physical arm64 iPhone".to_string());
   }
   Ok(DeviceEvidence {
      identifier,
      hardware_udid,
      summary: DeviceSummary {
         marketing_name,
         product_type,
         os_version,
         os_build,
         cpu,
      },
   })
}

fn validate_lock_state(path: &Path, expected_identifier: &str) -> Result<(), String>
{
   let bytes = fs::read(path).map_err(|error| format!("read lock-state evidence {}: {error}", path.display()))?;
   let value: serde_json::Value = serde_json::from_slice(&bytes)
      .map_err(|error| format!("parse lock-state evidence {}: {error}", path.display()))?;
   let identifier = required_json_string(&value, "/result/deviceIdentifier", "lock-state device identifier")?;
   let passcode_required = value.pointer("/result/passcodeRequired").and_then(serde_json::Value::as_bool)
      .ok_or_else(|| "lock-state evidence is missing passcodeRequired".to_string())?;
   if identifier != expected_identifier || passcode_required
   {
      return Err("lock-state evidence does not prove the admitted device was unlocked".to_string());
   }
   Ok(())
}

fn admit_device(run_root: &Path) -> Result<DeviceSummary, String>
{
   let raw = run_root.join("raw");
   let before = parse_device_details(&raw.join("device-before.json"))?;
   let after = parse_device_details(&raw.join("device-after.json"))?;
   if before.identifier != "1DEDF2A3-EC8E-5FCC-A437-8BD3A6F3D659"
      || before.hardware_udid != "00008150-001529C434F8401C"
      || before.identifier != after.identifier
      || before.hardware_udid != after.hardware_udid
      || before.summary != after.summary
   {
      return Err("physical device identity or OS changed across the pilot".to_string());
   }
   validate_lock_state(&raw.join("lock-before-build.json"), &before.identifier)?;
   validate_lock_state(&raw.join("lock-before-test.json"), &before.identifier)?;
   Ok(before.summary)
}

fn same_path(left: &Path, right: &Path) -> bool
{
   left == right || match (fs::canonicalize(left), fs::canonicalize(right))
   {
      (Ok(left), Ok(right)) => left == right,
      _ => false,
   }
}

fn validate_run_record_source(run_root: &Path, path: &Path, record: &RunRecord) -> Result<(), String>
{
   let document_directory = match record.run.treatment.as_str()
   {
      "oxide" => run_root.join("raw/oxide-documents"),
      "uikit-idiomatic" | "uikit-optimized" => run_root.join("raw/uikit-documents"),
      treatment => return Err(format!("unknown treatment {treatment} in run-record source")),
   };
   let expected_name = format!("oxide-feed-v1-{}.json", record.run.nonce);
   if path.file_name().and_then(|name| name.to_str()) != Some(expected_name.as_str())
   {
      return Err(format!("run {} has a noncanonical record filename", record.run.nonce));
   }
   let canonical_directory = fs::canonicalize(&document_directory)
      .map_err(|error| format!("canonicalize run-record directory {}: {error}", document_directory.display()))?;
   let canonical_path = fs::canonicalize(path)
      .map_err(|error| format!("canonicalize run record {}: {error}", path.display()))?;
   if !canonical_path.starts_with(&canonical_directory)
   {
      return Err(format!("run {} came from the wrong app container", record.run.nonce));
   }
   Ok(())
}

fn build_provenance_admitted(provenance: &BuildProvenance) -> bool
{
   provenance.schema == "oxide.feed-v1.build-provenance"
      && provenance.schema_revision == 1
      && provenance.core_device_id == "1DEDF2A3-EC8E-5FCC-A437-8BD3A6F3D659"
      && provenance.hardware_udid == "00008150-001529C434F8401C"
      && !provenance.device_model.is_empty()
      && !provenance.device_product_type.is_empty()
      && !provenance.os_version.is_empty()
      && !provenance.os_build.is_empty()
      && provenance.maximum_refresh_hz == 120
      && !provenance.xcode_version.is_empty()
      && !provenance.xcode_build.is_empty()
      && !provenance.iphoneos_sdk_version.is_empty()
      && !provenance.iphoneos_sdk_build.is_empty()
      && !provenance.rustc_release.is_empty()
      && is_git_object_id(&provenance.rustc_commit_hash)
      && provenance.rustc_host == "aarch64-apple-darwin"
      && provenance.cargo_version.starts_with("cargo ")
      && is_sha256(&provenance.release_build_settings_sha256)
      && is_sha256(&provenance.production_cargo_lock_sha256)
      && is_sha256(&provenance.production_cargo_metadata_sha256)
      && signing_identity_admitted(&provenance.uikit_signing)
      && signing_identity_admitted(&provenance.oxide_signing)
      && signing_identity_admitted(&provenance.controller_runner_signing)
      && signing_identity_admitted(&provenance.controller_xctest_signing)
      && provenance.uikit_signing.team_identifier == provenance.oxide_signing.team_identifier
      && provenance.uikit_signing.team_identifier == provenance.controller_runner_signing.team_identifier
      && provenance.uikit_signing.team_identifier == provenance.controller_xctest_signing.team_identifier
}

fn signing_identity_admitted(identity: &SigningIdentity) -> bool
{
   identity.authority.starts_with("Apple Development:")
      && identity.team_identifier.len() == 10
      && identity.team_identifier.bytes().all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
      && is_git_object_id(&identity.cdhash)
}

fn cleanup_admitted(proof: &CleanupProof) -> bool
{
   proof.schema == "oxide.feed-v1.cleanup"
      && proof.schema_revision == 3
      && proof.test_succeeded
      && proof.verified_attachment_count == ATTACHMENT_COUNT
      && proof.apps_uninstalled
      && proof.controller_uninstalled
      && proof.uikit_process_absent
      && proof.oxide_process_absent
      && proof.controller_process_absent
      && proof.prelaunch_fuses_admitted
      && proof.resource_limits.external_build_bytes == MAX_BUILD_ROOT_BYTES
      && proof.resource_limits.result_bundle_bytes == MAX_RESULT_BUNDLE_BYTES
      && proof.resource_limits.retained_evidence_bytes == MAX_RETAINED_EVIDENCE_BYTES
      && proof.resource_limits.retained_file_count == MAX_RETAINED_FILE_COUNT
      && proof.resource_limits.retained_file_bytes == MAX_RETAINED_FILE_BYTES
      && proof.source_snapshot_preserved
      && proof.external_build_removed
      && proof.result_bundle_removed
      && proof.reducer_binary_absent_from_result_root
      && proof.external_build_bytes > 0
      && proof.external_build_bytes <= MAX_BUILD_ROOT_BYTES
      && proof.result_bundle_bytes > 0
      && proof.result_bundle_bytes <= MAX_RESULT_BUNDLE_BYTES
      && proof.raw_evidence_bytes <= MAX_RETAINED_EVIDENCE_BYTES
      && proof.retained_file_count > 0
      && proof.retained_file_count <= MAX_RETAINED_FILE_COUNT
      && proof.largest_retained_file_bytes > 0
      && proof.largest_retained_file_bytes <= MAX_RETAINED_FILE_BYTES
      && proof.runtime_seconds.is_finite()
      && proof.runtime_seconds >= 0.0
      && proof.runtime_seconds <= 20.0 * 60.0
}

fn evaluate(paths: &ReducePaths, population: Population) -> Result<Report, String>
{
   if !paths.run_root.is_dir()
   {
      return Err(format!("run root {} is not a directory", paths.run_root.display()));
   }
   let files: Vec<PathBuf> = collect_files(&paths.run_root)?.into_iter().filter(|path| {
      population == Population::Smoke
         || (!same_path(path, &paths.output_json) && !same_path(path, &paths.output_markdown))
   }).collect();
   let retained_input_bytes = files.iter().try_fold(0_u64, |total, path| {
      let bytes = fs::metadata(path).map_err(|error| format!("read retained metadata {}: {error}", path.display()))?.len();
      total.checked_add(bytes).ok_or_else(|| "retained evidence byte count overflowed".to_string())
   })?;
   let largest_retained_file_bytes = files.iter().try_fold(0_u64, |maximum, path| {
      let bytes = fs::metadata(path)
         .map_err(|error| format!("read retained metadata {}: {error}", path.display()))?.len();
      Ok::<u64, String>(maximum.max(bytes))
   })?;
   let evidence_files = collect_raw_evidence_files(&paths.run_root, &files)?;
   let mut run_records = Vec::new();
   let mut failure_records = Vec::new();
   let mut blockers = Vec::new();
   let attachment_exports = match attachment_export_map(&files)
   {
      Ok(attachments) => attachments,
      Err(error) =>
      {
         blockers.push(format!("attachment map: {error}"));
         BTreeMap::new()
      }
   };
   let mut cleanup: Option<CleanupProof> = None;
   let mut controller_runtime: Option<ControllerRuntimeProof> = None;
   let mut evidence_manifest: Option<EvidenceManifest> = None;
   let mut evidence_manifest_path: Option<PathBuf> = None;
   let mut visual_gate_source_sha256: Option<String> = None;
   let mut repository_ref: Option<String> = None;
   let mut repository_head_commit: Option<String> = None;
   let mut repository_tree: Option<String> = None;
   if let Err(error) = verify_attachment_export(&paths.run_root.join("raw/attachments"))
   {
      blockers.push(format!("attachment admission: {error}"));
   }
   if retained_input_bytes > MAX_RETAINED_EVIDENCE_BYTES
   {
      blockers.push(format!("retained reducer input is {retained_input_bytes} bytes, above 512 MiB"));
   }
   if files.len() > MAX_RETAINED_FILE_COUNT
   {
      blockers.push(format!("retained reducer input has {} files, above {MAX_RETAINED_FILE_COUNT}", files.len()));
   }
   if largest_retained_file_bytes > MAX_RETAINED_FILE_BYTES
   {
      blockers.push(format!("retained reducer input contains a {largest_retained_file_bytes}-byte file, above {MAX_RETAINED_FILE_BYTES}"));
   }
   let device = match admit_device(&paths.run_root)
   {
      Ok(device) => Some(device),
      Err(error) =>
      {
         blockers.push(error);
         None
      }
   };
   if paths.run_root.join("tools").exists()
      || files.iter().any(|path| {
         path.components().any(|component| component.as_os_str().to_string_lossy().ends_with(".xcresult"))
            || path.file_name().and_then(|name| name.to_str()) == Some("oxide-feed-v1-reducer")
      })
   {
      blockers.push("retained package contains a result bundle or reducer executable".to_string());
   }

   for path in files.iter().filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("json"))
   {
      let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("");
      let controlled_name = name.starts_with("oxide-feed-v1-") || name.starts_with("feed-v1-");
      let bytes = match fs::read(path)
      {
         Ok(bytes) => bytes,
         Err(error) =>
         {
            blockers.push(format!("read {}: {error}", path.display()));
            continue;
         }
      };
      let value: serde_json::Value = match serde_json::from_slice(&bytes)
      {
         Ok(value) => value,
         Err(error) =>
         {
            if controlled_name
            {
               blockers.push(format!("malformed controlled JSON {}: {error}", path.display()));
            }
            continue;
         }
      };
      let schema = value.get("schema").and_then(serde_json::Value::as_str);
      match schema
      {
         Some(RUN_SCHEMA) => match serde_json::from_value::<RunRecord>(value)
         {
            Ok(record) =>
            {
               if let Err(error) = validate_run_record_source(&paths.run_root, path, &record)
               {
                  blockers.push(error);
               }
               run_records.push(record);
            }
            Err(error) => blockers.push(format!("strict run schema {}: {error}", path.display())),
         },
         Some("oxide.feed-v1.failure") => match serde_json::from_value::<FailureRecord>(value)
         {
            Ok(record) => failure_records.push(record),
            Err(error) => blockers.push(format!("strict failure schema {}: {error}", path.display())),
         },
         Some("oxide.feed-v1.cleanup") => match serde_json::from_value::<CleanupProof>(value)
         {
            Ok(proof) =>
            {
               if !same_path(path, &paths.run_root.join("raw/cleanup.json"))
               {
                  blockers.push(format!("cleanup proof is outside raw/cleanup.json: {}", path.display()));
               }
               if cleanup.replace(proof).is_some()
               {
                  blockers.push("multiple cleanup proofs".to_string());
               }
            }
            Err(error) => blockers.push(format!("strict cleanup schema {}: {error}", path.display())),
         },
         Some("oxide.feed-v1.controller-runtime") => match serde_json::from_value::<ControllerRuntimeProof>(value)
         {
            Ok(proof) =>
            {
               let expected_name = "oxide-feed-v1-controller-runtime.json";
               let controller_documents = paths.run_root.join("raw/controller-documents");
               let canonical_documents = fs::canonicalize(&controller_documents);
               let canonical_path = fs::canonicalize(path);
               if path.file_name().and_then(|name| name.to_str()) != Some(expected_name)
                  || !matches!((canonical_documents, canonical_path),
                     (Ok(documents), Ok(path)) if path.starts_with(&documents))
               {
                  blockers.push(format!("controller runtime proof has a noncanonical source: {}", path.display()));
               }
               if controller_runtime.replace(proof).is_some()
               {
                  blockers.push("multiple controller runtime proofs".to_string());
               }
            }
            Err(error) => blockers.push(format!("strict controller runtime schema {}: {error}", path.display())),
         },
         Some("oxide.feed-v1.evidence-manifest") => match serde_json::from_value::<EvidenceManifest>(value)
         {
            Ok(manifest) if manifest.schema == "oxide.feed-v1.evidence-manifest"
               && manifest.schema_revision == 3
               && manifest.fixture_sha256 == FIXTURE_SHA256
               && manifest.repository_ref.starts_with("refs/heads/")
               && is_git_object_id(&manifest.repository_head_commit)
               && is_git_object_id(&manifest.repository_tree)
               && build_provenance_admitted(&manifest.build_provenance)
               && manifest.regular_font_sha256 == "7d494f276293fb0a8e2aab1fc0e386baa3e8a1d90927f518abb152b5c73e29f9"
               && manifest.bold_font_sha256 == "7f4feacd835eed23e104413f800a74b9f0270ce8c754c990bfc09b796a3ca628"
               && is_sha256(&manifest.uikit_app_sha256)
               && is_sha256(&manifest.oxide_app_sha256)
               && is_sha256(&manifest.controller_runner_sha256)
               && is_sha256(&manifest.controller_runner_binary_sha256)
               && is_sha256(&manifest.controller_xctest_sha256)
               && is_sha256(&manifest.controller_xctest_binary_sha256)
               && is_sha256(&manifest.reducer_binary_sha256)
               && manifest.source_files.keys().any(|name| name == "reducer/src/lib.rs") =>
            {
               if !same_path(path, &paths.run_root.join("raw/evidence-manifest.json"))
               {
                  blockers.push(format!("evidence manifest is outside raw/evidence-manifest.json: {}", path.display()));
               }
               if evidence_manifest_path.is_some()
               {
                  blockers.push("multiple evidence manifests".to_string());
               }
               visual_gate_source_sha256 = manifest.source_files.get("reducer/src/lib.rs").cloned();
               repository_ref = Some(manifest.repository_ref.clone());
               repository_head_commit = Some(manifest.repository_head_commit.clone());
               repository_tree = Some(manifest.repository_tree.clone());
               evidence_manifest = Some(manifest);
               evidence_manifest_path = Some(path.clone());
            }
            Ok(_) => blockers.push(format!("evidence manifest identity mismatch in {}", path.display())),
            Err(error) => blockers.push(format!("strict evidence manifest {}: {error}", path.display())),
         },
         Some(schema) if schema.starts_with("oxide.feed-v1.") =>
         {
            blockers.push(format!("unknown controlled schema {schema} in {}", path.display()));
         }
         schema if controlled_name =>
         {
            blockers.push(format!(
               "unrecognized controlled schema {} in {}",
               schema.unwrap_or("missing"),
               path.display()
            ));
         }
         _ => {}
      }
   }

   for failure in failure_records
   {
      if failure.schema != "oxide.feed-v1.failure" || failure.schema_revision != 1
      {
         blockers.push("failure record schema identity mismatch".to_string());
      }
      if let Err(error) = validate_fixture(&failure.fixture)
      {
         blockers.push(format!("failure record fixture: {error}"));
      }
      blockers.push(format!(
         "app failure nonce={} treatment={} stage={}: {}",
         failure.nonce.as_deref().unwrap_or("missing"),
         failure.treatment.as_deref().unwrap_or("missing"),
         failure.stage,
         failure.message
      ));
   }

   match controller_runtime
   {
      Some(proof) =>
      {
         if let Err(error) = validate_controller_runtime(&proof, population)
         {
            blockers.push(error);
         }
      }
      None => blockers.push("missing controller runtime proof".to_string()),
   }

   let mut nonces = BTreeSet::new();
   let mut measured_runs = Vec::new();
   for record in run_records
   {
      if !nonces.insert(record.run.nonce.clone())
      {
         blockers.push(format!("duplicate run nonce {}", record.run.nonce));
         continue;
      }
      if let Err(error) = validate_run(&record)
      {
         blockers.push(format!("run {}: {error}", record.run.nonce));
         continue;
      }
      match callback_metrics(&record.display_link.samples)
      {
         Ok((intervals, metrics)) => measured_runs.push(MeasuredRun { record, intervals, metrics }),
         Err(error) => blockers.push(format!("callback admission {}: {error}", record.run.nonce)),
      }
   }

   let smoke_runs: Vec<&MeasuredRun> = measured_runs.iter()
      .filter(|run| run.record.run.phase == "smoke")
      .collect();
   let expected_attachment_names: BTreeSet<String> = smoke_runs.iter()
      .map(|run| format!("feed-v1-{}.png", run.record.run.nonce))
      .collect();
   let actual_attachment_names: BTreeSet<String> = attachment_exports.keys().cloned().collect();
   if actual_attachment_names != expected_attachment_names
   {
      blockers.push("attachment manifest names are not exactly the six frozen smoke captures".to_string());
   }
   let population_blockers = validate_population(&measured_runs, population);
   blockers.extend(population_blockers);
   let travel_validation = validate_travel(&measured_runs, population);
   blockers.extend(travel_validation.blockers);
   let travel_equivalence = travel_validation.results;
   let population_admitted = blockers.is_empty();

   let mut visual_treatments = Vec::new();
   let mut adversarial_results = Vec::new();
   if population_admitted
   {
      let mut canonical_images: BTreeMap<(String, String), RgbaImage> = BTreeMap::new();
      for run in &smoke_runs
      {
         let capture_name = format!("feed-v1-{}.png", run.record.run.nonce);
         let Some(capture_path) = attachment_exports.get(&capture_name).map(PathBuf::as_path) else
         {
            blockers.push(format!("missing smoke capture {capture_name}"));
            continue;
         };
         let capture = match decode_png(capture_path).and_then(|image| crop_surface(&image))
         {
            Ok(capture) => capture,
            Err(error) =>
            {
               blockers.push(format!("capture decode/crop failed for {}: {error}", run.record.run.nonce));
               continue;
            }
         };
         let key = (
            run.record.run.treatment.clone(),
            run.record.run.start_state.clone(),
         );
         if canonical_images.insert(key, capture).is_some()
         {
            blockers.push(format!(
               "duplicate smoke capture for {} {}",
               run.record.run.treatment,
               run.record.run.start_state
            ));
         }
      }

      for state in ["top", "bottom"]
      {
         let reference_key = ("uikit-idiomatic".to_string(), state.to_string());
         let Some(reference) = canonical_images.get(&reference_key) else
         {
            blockers.push(format!("missing idiomatic UIKit {state} visual reference"));
            continue;
         };
         for treatment in ["uikit-idiomatic", "uikit-optimized", "oxide"]
         {
            let key = (treatment.to_string(), state.to_string());
            let Some(candidate) = canonical_images.get(&key) else
            {
               blockers.push(format!("missing {treatment} {state} visual"));
               continue;
            };
            let metrics = if treatment == "uikit-idiomatic"
            {
               Ok(VisualMetrics {
                  ssim: 1.0,
                  worst_tile_rgb_mae: 0.0,
                  exact_rgb_mae: 0.0,
                  passes: true,
               })
            }
            else
            {
               visual_metrics(reference, candidate)
            };
            match metrics
            {
               Ok(metrics) =>
               {
                  if !metrics.passes
                  {
                     blockers.push(format!(
                        "visual gate failed for {treatment} {state}: SSIM {:.6}, worst tile MAE {:.6}",
                        metrics.ssim,
                        metrics.worst_tile_rgb_mae
                     ));
                  }
                  visual_treatments.push(VisualTreatmentResult {
                     state: state.to_string(),
                     treatment: treatment.to_string(),
                     metrics,
                  });
               }
               Err(error) => blockers.push(format!("visual metrics {treatment} {state}: {error}")),
            }
         }
      }

      for state in ["top", "bottom"]
      {
         let references: Vec<&MeasuredRun> = measured_runs.iter().filter(|run| {
            run.record.run.treatment == "uikit-idiomatic"
               && run.record.run.start_state == state
         }).collect();
         let Some(reference) = references.first() else
         {
            continue;
         };
         for run in &measured_runs
         {
            if run.record.run.start_state == state && !geometry_matches(&reference.record.geometry, &run.record.geometry)
            {
               blockers.push(format!("cross-treatment observed geometry mismatch for {}", run.record.run.nonce));
            }
         }
      }

      if let (Some(reference), Some(reference_run)) = (
         canonical_images.get(&("uikit-idiomatic".to_string(), "top".to_string())),
         measured_runs.iter().find(|run| run.record.run.treatment == "uikit-idiomatic" && run.record.run.start_state == "top"),
      )
      {
         match adversarial_gate(reference, &reference_run.record.geometry.visible_components)
         {
            Ok(results) =>
            {
               for result in &results
               {
                  if !result.rejected
                  {
                     blockers.push(format!("visual gate admitted hostile mutation {}", result.mutation));
                  }
               }
               adversarial_results = results;
            }
            Err(error) => blockers.push(format!("hostile mutation suite: {error}")),
         }
      }
      else
      {
         blockers.push("missing actual idiomatic UIKit top capture for hostile mutation suite".to_string());
      }
   }

   let cleanup = match cleanup
   {
      Some(proof) =>
      {
         let admitted = cleanup_admitted(&proof);
         if !admitted
         {
            blockers.push("cleanup/runtime/cap proof failed".to_string());
         }
         CleanupEvidence { admitted, proof: Some(proof) }
      }
      None =>
      {
         blockers.push("missing cleanup proof".to_string());
         CleanupEvidence { admitted: false, proof: None }
      }
   };

   if let (Some(device), Some(manifest)) = (&device, &evidence_manifest)
   {
      let provenance = &manifest.build_provenance;
      if provenance.device_model != device.marketing_name
         || provenance.device_product_type != device.product_type
         || provenance.os_version != device.os_version
         || provenance.os_build != device.os_build
      {
         blockers.push("build provenance device model or OS differs from device evidence".to_string());
      }
   }

   let evidence_manifest_sha256 = match evidence_manifest_path
   {
      Some(path) => match sha256_file(&path)
      {
         Ok(hash) => Some(hash),
         Err(error) =>
         {
            blockers.push(error);
            None
         }
      },
      None =>
      {
         blockers.push("missing evidence manifest".to_string());
         None
      }
   };

   blockers.sort();
   blockers.dedup();
   let primary: Vec<&MeasuredRun> = measured_runs.iter().filter(|run| run.record.run.phase == "primary").collect();
   let mut summaries = Vec::new();
   let mut comparisons = Vec::new();
   let mut runs = Vec::new();
   let policy = PublicationPolicy::frozen();
   if blockers.is_empty() && population == Population::Full
   {
      for treatment in ["uikit-idiomatic", "uikit-optimized", "oxide"]
      {
         let selected: Vec<&MeasuredRun> = primary.iter().copied().filter(|run| run.record.run.treatment == treatment).collect();
         summaries.push(treatment_summary(&selected, treatment)?);
      }
      comparisons.push(comparison(&primary, &summaries, "uikit-idiomatic", &policy)?);
      comparisons.push(comparison(&primary, &summaries, "uikit-optimized", &policy)?);
      runs = primary.iter().map(|run| run_summary(run)).collect();
      runs.sort_by(|left, right| {
         left.session_index.cmp(&right.session_index)
            .then(left.pair_index.cmp(&right.pair_index))
            .then(left.order_index.cmp(&right.order_index))
            .then(left.treatment.cmp(&right.treatment))
            .then(left.direction.cmp(&right.direction))
            .then(left.nonce.cmp(&right.nonce))
      });
   }
   let status = if blockers.is_empty() { "complete" } else { "blocked" };
   let decision = if population == Population::Smoke
   {
      "smoke-verification-only-no-publication".to_string()
   }
   else if !blockers.is_empty()
   {
      "blocked".to_string()
   }
   else
   {
      two_comparator_decision(&comparisons)?
   };
   let report = Report {
      schema: "oxide.feed-v1.report",
      schema_revision: 5,
      fixture_sha256: FIXTURE_SHA256,
      fixture_byte_count: FIXTURE_BYTE_COUNT,
      device,
      status: status.to_string(),
      decision,
      policy,
      blockers,
      visual_gate_id: VISUAL_GATE_ID,
      visual_gate_spec_sha256: sha256_bytes(VISUAL_GATE_ID.as_bytes()),
      visual_gate_source_sha256,
      repository_ref,
      repository_head_commit,
      repository_tree,
      visual_treatments,
      adversarial_results,
      travel_equivalence,
      treatments: summaries,
      comparisons,
      runs,
      missing_metrics: vec![
         "presented-frame pacing",
         "visible-frame pacing",
         "input-to-visible latency",
         "main-thread CPU",
         "process CPU",
         "resident memory",
         "symmetric direct GPU time",
         "direct energy",
      ],
      cleanup,
      evidence_inventory: evidence_files,
      evidence_manifest_sha256,
      retained_input_bytes,
      run_count_total: measured_runs.len(),
      run_count_primary: primary.len(),
   };
   Ok(report)
}

fn collect_raw_evidence_files(run_root: &Path, files: &[PathBuf]) -> Result<Vec<EvidenceFile>, String>
{
   let raw_root = run_root.join("raw");
   let mut output = Vec::new();
   for path in files.iter().filter(|path| path.starts_with(&raw_root))
   {
      output.push(EvidenceFile {
         relative_path: relative_path_string(run_root, path)?,
         bytes: fs::metadata(path)
            .map_err(|error| format!("read evidence inventory metadata {}: {error}", path.display()))?.len(),
         sha256: sha256_file(path)?,
      });
   }
   output.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
   Ok(output)
}

fn output_excluded_raw_evidence_files(paths: &ReducePaths) -> Result<Vec<EvidenceFile>, String>
{
   let files: Vec<PathBuf> = collect_files(&paths.run_root)?.into_iter().filter(|path| {
      !same_path(path, &paths.output_json) && !same_path(path, &paths.output_markdown)
   }).collect();
   collect_raw_evidence_files(&paths.run_root, &files)
}

fn relative_path_string(root: &Path, path: &Path) -> Result<String, String>
{
   let relative = path.strip_prefix(root)
      .map_err(|_| format!("path {} is outside evidence root {}", path.display(), root.display()))?;
   if relative.as_os_str().is_empty()
   {
      return Err("evidence-relative path is empty".to_string());
   }
   relative.to_str().map(str::to_string)
      .ok_or_else(|| format!("evidence-relative path is not UTF-8: {}", relative.display()))
}

pub fn reduce(paths: &ReducePaths) -> Result<(), String>
{
   let report = evaluate(paths, Population::Full)?;
   if output_excluded_raw_evidence_files(paths)? != report.evidence_inventory
   {
      return Err("raw evidence changed during publication reduction".to_string());
   }
   write_report(paths, &report)
}

pub fn verify_smoke(run_root: &Path) -> Result<(), String>
{
   let paths = ReducePaths {
      run_root: run_root.to_path_buf(),
      output_json: run_root.join("unused-smoke-report.json"),
      output_markdown: run_root.join("unused-smoke-report.md"),
   };
   let report = evaluate(&paths, Population::Smoke)?;
   if report.blockers.is_empty()
   {
      Ok(())
   }
   else
   {
      Err(format!("smoke verification blocked:\n{}", report.blockers.join("\n")))
   }
}

fn write_report(paths: &ReducePaths, report: &Report) -> Result<(), String>
{
   let canonical_json_path = relative_path_string(&paths.run_root, &paths.output_json)?;
   let mut json = serde_json::to_vec_pretty(report).map_err(|error| format!("serialize report: {error}"))?;
   json.push(b'\n');
   let canonical_json_sha256 = sha256_bytes(&json);
   write_atomic(&paths.output_json, &json)?;
   let markdown = render_markdown(report, &canonical_json_path, &canonical_json_sha256);
   write_atomic(&paths.output_markdown, markdown.as_bytes())
}

fn render_markdown(report: &Report, canonical_json_path: &str, canonical_json_sha256: &str) -> String
{
   let mut output = String::new();
   output.push_str("# feed-v1 physical-device evidence\n\n");
   output.push_str(&format!("- Status: `{}`\n", report.status));
   output.push_str(&format!("- Decision: `{}`\n", report.decision));
   output.push_str(&format!(
      "- Canonical JSON: `{canonical_json_path}` (SHA-256 `{canonical_json_sha256}`)\n"
   ));
   output.push_str(&format!("- Fixture: `{}` ({} bytes)\n", report.fixture_sha256, report.fixture_byte_count));
   if let Some(device) = &report.device
   {
      output.push_str(&format!(
         "- Device: {} (`{}`), iOS {} (`{}`), CPU `{}`\n",
         device.marketing_name,
         device.product_type,
         device.os_version,
         device.os_build,
         device.cpu
      ));
   }
   else
   {
      output.push_str("- Device: `missing or inadmissible`\n");
   }
   output.push_str(&format!("- Visual gate: `{}`\n", report.visual_gate_id));
   output.push_str(&format!("- Runs: {} total, {} primary\n", report.run_count_total, report.run_count_primary));
   output.push_str(&format!(
      "- Repository: `{}` at commit `{}` (tree `{}`)\n",
      report.repository_ref.as_deref().unwrap_or("missing"),
      report.repository_head_commit.as_deref().unwrap_or("missing"),
      report.repository_tree.as_deref().unwrap_or("missing")
   ));
   output.push_str("\n## Frozen policy\n\n");
   output.push_str(&format!(
      "- Callback quantiles: `{}` with `{}`.\n",
      report.policy.statistics.callback_quantile_method,
      report.policy.statistics.callback_quantile_rank_formula
   ));
   output.push_str(&format!(
      "- Treatment aggregates: `{}` over {} clusters of {} directions.\n",
      report.policy.statistics.treatment_aggregate_method,
      report.policy.statistics.clusters_per_treatment,
      report.policy.statistics.directions_per_cluster
   ));
   output.push_str(&format!(
      "- Exact median interval: ranks {}-{} of {}, target {:.3}%, achieved {:.6}%.\n",
      report.policy.statistics.confidence_lower_rank,
      report.policy.statistics.confidence_upper_rank,
      report.policy.statistics.confidence_sample_count,
      report.policy.statistics.confidence_target_coverage * 100.0,
      report.policy.statistics.confidence_achieved_coverage * 100.0
   ));
   output.push_str(&format!(
      "- Classification: slower above +{:.1}% at the lower bound; faster below {:+.1}% at the upper bound with lower aggregate p50/p95; non-inferior at or below +{:.1}% at the upper bound.\n",
      report.policy.classification_thresholds.slower_interval_lower_bound_exclusive * 100.0,
      report.policy.classification_thresholds.faster_interval_upper_bound_exclusive * 100.0,
      report.policy.classification_thresholds.non_inferior_interval_upper_bound_inclusive * 100.0
   ));
   output.push_str(&format!(
      "- Guardrails: missed ratio <= comparator + {:.1} percentage points and <= {:.1}% absolute; hitch <= comparator + {:.1} ms/s and <= {:.1} ms/s absolute.\n",
      report.policy.guardrails.missed_deadline_ratio_maximum_comparator_delta * 100.0,
      report.policy.guardrails.missed_deadline_ratio_absolute_maximum * 100.0,
      report.policy.guardrails.callback_hitch_ms_per_second_maximum_comparator_delta,
      report.policy.guardrails.callback_hitch_ms_per_second_absolute_maximum
   ));
   if !report.blockers.is_empty()
   {
      output.push_str("\n## Blockers\n\n");
      for blocker in &report.blockers
      {
         output.push_str(&format!("- {blocker}\n"));
      }
   }
   if !report.visual_treatments.is_empty()
   {
      output.push_str("\n## Visual admission\n\n");
      output.push_str("| State | Treatment | SSIM | Worst 48x48 RGB MAE | Full-surface RGB MAE | Pass |\n");
      output.push_str("|---|---|---:|---:|---:|---:|\n");
      for visual in &report.visual_treatments
      {
         output.push_str(&format!(
            "| {} | {} | {:.6} | {:.3} | {:.3} | {} |\n",
            visual.state,
            visual.treatment,
            visual.metrics.ssim,
            visual.metrics.worst_tile_rgb_mae,
            visual.metrics.exact_rgb_mae,
            if visual.metrics.passes { "yes" } else { "no" }
         ));
      }
   }
   if !report.travel_equivalence.is_empty()
   {
      output.push_str("\n## Travel equivalence\n\n");
      output.push_str("| Treatment | Direction | Pairs | Median relative delta | 95% interval | Frozen median/CI margins | Pass |\n");
      output.push_str("|---|---|---:|---:|---:|---:|---:|\n");
      for result in &report.travel_equivalence
      {
         output.push_str(&format!(
            "| {} | {} | {} | {:+.3}% | {:+.3}% to {:+.3}% | +/-{:.1}% / +/-{:.1}% | {} |\n",
            result.treatment,
            result.direction,
            result.pair_count,
            result.median_relative_delta * 100.0,
            result.confidence_interval.bounds[0] * 100.0,
            result.confidence_interval.bounds[1] * 100.0,
            result.median_margin * 100.0,
            result.confidence_margin * 100.0,
            if result.passes { "yes" } else { "no" }
         ));
      }
   }
   if !report.treatments.is_empty()
   {
      output.push_str("\n## Callback pacing\n\n");
      output.push_str("| Treatment | Runs/clusters | Aggregate p50 ms | Aggregate p95 ms | Peak ms | Missed deadlines | Hitch ms/s |\n");
      output.push_str("|---|---:|---:|---:|---:|---:|---:|\n");
      for treatment in &report.treatments
      {
         output.push_str(&format!(
            "| {} | {}/{} | {:.3} | {:.3} | {:.3} | {:.3}% | {:.3} |\n",
            treatment.treatment,
            treatment.run_count,
            treatment.cluster_count,
            treatment.aggregate_interval_p50_ms,
            treatment.aggregate_interval_p95_ms,
            treatment.interval_peak_ms,
            treatment.missed_callback_deadline_ratio * 100.0,
            treatment.callback_hitch_ms_per_elapsed_second
         ));
      }
   }
   if !report.comparisons.is_empty()
   {
      output.push_str("\n## Oxide comparisons\n\n");
      output.push_str("| Comparator | Classification | Median pair p95 delta | 95% interval | p50 ms O/C | p95 ms O/C | Missed O/C | Hitch ms/s O/C |\n");
      output.push_str("|---|---|---:|---:|---:|---:|---:|---:|\n");
      for comparison in &report.comparisons
      {
         output.push_str(&format!(
            "| {} | {} | {:+.3}% | {:+.3}% to {:+.3}% | {:.3}/{:.3} | {:.3}/{:.3} | {:.3}%/{:.3}% | {:.3}/{:.3} |\n",
            comparison.comparator,
            comparison.classification,
            comparison.median_pair_relative_p95_delta * 100.0,
            comparison.confidence_interval.bounds[0] * 100.0,
            comparison.confidence_interval.bounds[1] * 100.0,
            comparison.oxide_aggregate_interval_p50_ms,
            comparison.comparator_aggregate_interval_p50_ms,
            comparison.oxide_aggregate_interval_p95_ms,
            comparison.comparator_aggregate_interval_p95_ms,
            comparison.oxide_missed_deadline_ratio * 100.0,
            comparison.comparator_missed_deadline_ratio * 100.0,
            comparison.oxide_hitch_ms_per_second,
            comparison.comparator_hitch_ms_per_second
         ));
      }
   }
   output.push_str("\n## Evidence and limitations\n\n");
   output.push_str(&format!("- Visual-gate specification SHA-256: `{}`\n", report.visual_gate_spec_sha256));
   output.push_str(&format!(
      "- Visual-gate source SHA-256: `{}`\n",
      report.visual_gate_source_sha256.as_deref().unwrap_or("missing")
   ));
   output.push_str(&format!(
      "- Evidence manifest SHA-256: `{}`\n",
      report.evidence_manifest_sha256.as_deref().unwrap_or("missing")
   ));
   output.push_str(&format!(
      "- Raw evidence inventory: {} sorted files with byte counts and SHA-256 values in canonical JSON.\n",
      report.evidence_inventory.len()
   ));
   output.push_str(&format!(
      "- Primary callback rows: {} with raw sample arrays in canonical JSON only.\n",
      report.runs.len()
   ));
   output.push_str(&format!("- Retained reducer input: {} bytes\n", report.retained_input_bytes));
   output.push_str("\n## Cleanup\n\n");
   output.push_str(&format!("- Cleanup proof admitted: `{}`\n", report.cleanup.admitted));
   if let Some(proof) = &report.cleanup.proof
   {
      output.push_str(&format!(
         "- Runner observations: test succeeded `{}`, apps uninstalled `{}`, controller uninstalled `{}`, controller process absent `{}`, source preserved `{}`, build removed `{}`, result bundle removed `{}`.\n",
         proof.test_succeeded,
         proof.apps_uninstalled,
         proof.controller_uninstalled,
         proof.controller_process_absent,
         proof.source_snapshot_preserved,
         proof.external_build_removed,
         proof.result_bundle_removed
      ));
   }
   output.push_str(&format!("- Missing metrics: {}\n", report.missing_metrics.join(", ")));
   output.push_str("\nThese figures are display-link callback pacing only. They make no presented-frame, visible-frame, or photon-latency claim.\n");
   output
}

pub fn verify_attachment_export(root: &Path) -> Result<(), String>
{
   if !root.is_dir()
   {
      return Err(format!("attachment export {} is not a directory", root.display()));
   }
   let files = collect_files(root)?;
   let manifest_count = files.iter().filter(|path| {
      path.file_name().and_then(|name| name.to_str()) == Some("manifest.json")
   }).count();
   if manifest_count != 1
   {
      return Err(format!("attachment export has {manifest_count} manifests, expected 1"));
   }
   let attachments = attachment_export_map(&files)?;
   if attachments.len() != ATTACHMENT_COUNT
   {
      return Err(format!(
         "attachment export has {} referenced files, expected {ATTACHMENT_COUNT}",
         attachments.len()
      ));
   }
   if files.len() != ATTACHMENT_COUNT + 1
   {
      return Err(format!(
         "attachment export has {} total files, expected {} referenced files plus one manifest",
         files.len(),
         ATTACHMENT_COUNT
      ));
   }
   let canonical_root = fs::canonicalize(root)
      .map_err(|error| format!("canonicalize attachment root {}: {error}", root.display()))?;
   for (name, path) in attachments
   {
      if !name.starts_with("feed-v1-")
      {
         return Err(format!("attachment export contains unexpected name {name}"));
      }
      let canonical_path = fs::canonicalize(&path)
         .map_err(|error| format!("canonicalize attachment {}: {error}", path.display()))?;
      if !canonical_path.starts_with(&canonical_root)
      {
         return Err(format!("attachment {name} leaves the export root"));
      }
      let size = fs::metadata(&canonical_path)
         .map_err(|error| format!("read attachment metadata {}: {error}", canonical_path.display()))?.len();
      if size == 0
      {
         return Err(format!("attachment {name} is empty"));
      }
   }
   Ok(())
}

pub fn build_evidence_manifest(source_root: &Path, repository_root: &Path, uikit_app: &Path, oxide_app: &Path, controller_runner: &Path, controller_xctest: &Path, build_provenance: &Path, output: &Path) -> Result<(), String>
{
   if !source_root.is_dir()
      || !repository_root.is_dir()
      || !uikit_app.is_dir()
      || !oxide_app.is_dir()
      || !controller_runner.is_dir()
      || !controller_xctest.is_dir()
      || !build_provenance.is_file()
   {
      return Err("manifest products must be directories and build provenance must be a file".to_string());
   }
   let controller_runner_binary = controller_runner.join("FeedV1Controller-Runner");
   let controller_xctest_binary = controller_xctest.join("FeedV1Controller");
   if !controller_runner_binary.is_file() || !controller_xctest_binary.is_file()
   {
      return Err("controller products do not contain the frozen executables".to_string());
   }
   let (repository_ref, repository_head_commit, repository_tree) = repository_snapshot(repository_root)?;
   let build_provenance: BuildProvenance = serde_json::from_slice(
      &fs::read(build_provenance)
         .map_err(|error| format!("read build provenance {}: {error}", build_provenance.display()))?,
   ).map_err(|error| format!("strict build provenance {}: {error}", build_provenance.display()))?;
   if !build_provenance_admitted(&build_provenance)
   {
      return Err("build provenance does not match the frozen device/toolchain/signing contract".to_string());
   }
   let source_files = collect_evidence_source_files(source_root)?;
   let mut hashes = BTreeMap::new();
   for path in source_files
   {
      let relative = path.strip_prefix(source_root)
         .map_err(|error| format!("manifest relative path {}: {error}", path.display()))?;
      hashes.insert(relative.to_string_lossy().replace('\\', "/"), sha256_file(&path)?);
   }
   let regular_font = source_root.join("../../../crates/ui-core/assets/Asap-Regular.ttf");
   let bold_font = source_root.join("../../../crates/ui-core/assets/Asap-Bold.ttf");
   let reducer_binary = env::current_exe().map_err(|error| format!("resolve reducer executable: {error}"))?;
   let manifest = EvidenceManifest {
      schema: "oxide.feed-v1.evidence-manifest".to_string(),
      schema_revision: 3,
      fixture_sha256: FIXTURE_SHA256.to_string(),
      repository_ref,
      repository_head_commit,
      repository_tree,
      build_provenance,
      source_files: hashes,
      uikit_app_sha256: hash_directory(uikit_app)?,
      oxide_app_sha256: hash_directory(oxide_app)?,
      controller_runner_sha256: hash_directory(controller_runner)?,
      controller_runner_binary_sha256: sha256_file(&controller_runner_binary)?,
      controller_xctest_sha256: hash_directory(controller_xctest)?,
      controller_xctest_binary_sha256: sha256_file(&controller_xctest_binary)?,
      reducer_binary_sha256: sha256_file(&reducer_binary)?,
      regular_font_sha256: sha256_file(&regular_font)?,
      bold_font_sha256: sha256_file(&bold_font)?,
   };
   let mut json = serde_json::to_vec_pretty(&manifest).map_err(|error| format!("serialize evidence manifest: {error}"))?;
   json.push(b'\n');
   write_atomic(output, &json)
}

fn repository_snapshot(root: &Path) -> Result<(String, String, String), String>
{
   let status = git_output(root, &["status", "--porcelain=v1", "--untracked-files=all"])?;
   if !status.is_empty()
   {
      return Err("repository worktree is not clean at evidence capture".to_string());
   }
   let repository_ref = git_output(root, &["symbolic-ref", "--quiet", "HEAD"])?;
   if !repository_ref.starts_with("refs/heads/")
   {
      return Err("repository HEAD is not on a named branch".to_string());
   }
   let head = git_output(root, &["rev-parse", "--verify", "HEAD^{commit}"])?;
   let tree = git_output(root, &["rev-parse", "--verify", "HEAD^{tree}"])?;
   if !is_git_object_id(&head) || !is_git_object_id(&tree)
   {
      return Err("repository commit or tree identity is malformed".to_string());
   }
   Ok((repository_ref, head, tree))
}

fn git_output(root: &Path, args: &[&str]) -> Result<String, String>
{
   let output = Command::new("git").arg("-C").arg(root).args(args).output()
      .map_err(|error| format!("run git {}: {error}", args.join(" ")))?;
   if !output.status.success()
   {
      return Err(format!(
         "git {} failed: {}",
         args.join(" "),
         String::from_utf8_lossy(&output.stderr).trim()
      ));
   }
   let text = String::from_utf8(output.stdout).map_err(|error| format!("git output is not UTF-8: {error}"))?;
   Ok(text.trim().to_string())
}

fn is_git_object_id(value: &str) -> bool
{
   matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn collect_evidence_source_files(root: &Path) -> Result<Vec<PathBuf>, String>
{
   let root_metadata = fs::symlink_metadata(root)
      .map_err(|error| format!("read evidence source root {}: {error}", root.display()))?;
   if root_metadata.file_type().is_symlink() || !root_metadata.is_dir()
   {
      return Err(format!("evidence source root is not a real directory: {}", root.display()));
   }
   let protocol = root.join("protocol.md");
   let protocol_metadata = fs::symlink_metadata(&protocol)
      .map_err(|error| format!("read evidence protocol {}: {error}", protocol.display()))?;
   if protocol_metadata.file_type().is_symlink() || !protocol_metadata.is_file()
   {
      return Err(format!("evidence protocol is not a real file: {}", protocol.display()));
   }
   let mut files = vec![protocol];
   for name in ["ios", "reducer"]
   {
      let tree = root.join(name);
      let tree_metadata = fs::symlink_metadata(&tree)
         .map_err(|error| format!("read evidence source tree {}: {error}", tree.display()))?;
      if tree_metadata.file_type().is_symlink() || !tree_metadata.is_dir()
      {
         return Err(format!("evidence source tree is not a real directory: {}", tree.display()));
      }
      collect_evidence_source_tree(root, &tree, &mut files)?;
   }
   files.sort();
   Ok(files)
}

fn collect_evidence_source_tree(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String>
{
   let entries = fs::read_dir(directory).map_err(|error| format!("read source directory {}: {error}", directory.display()))?;
   for entry in entries
   {
      let entry = entry.map_err(|error| format!("read source directory entry {}: {error}", directory.display()))?;
      let path = entry.path();
      let relative = path.strip_prefix(root)
         .map_err(|error| format!("source relative path {}: {error}", path.display()))?;
      let file_type = entry.file_type().map_err(|error| format!("source file type {}: {error}", path.display()))?;
      if file_type.is_symlink()
      {
         return Err(format!("evidence source path is a symlink: {}", relative.display()));
      }
      if evidence_runtime_path(relative)
      {
         continue;
      }
      if file_type.is_dir()
      {
         collect_evidence_source_tree(root, &path, files)?;
      }
      else if file_type.is_file()
      {
         if evidence_runtime_file(relative)
         {
            continue;
         }
         if evidence_source_file(relative)
         {
            files.push(path);
         }
         else
         {
            return Err(format!("unclassified evidence source file: {}", relative.display()));
         }
      }
   }
   Ok(())
}

fn evidence_runtime_path(relative: &Path) -> bool
{
   relative.components().any(|component| {
      let name = component.as_os_str().to_string_lossy();
      matches!(name.as_ref(),
         "target" | "build" | "DerivedData" | "tools" | "raw" | "result" | "results"
         | "evidence" | "artifacts" | "attachments" | "uikit-documents" | "oxide-documents"
         | "xcuserdata"
      ) || name.ends_with(".xcresult")
         || name.starts_with("oxide-feed-v1-result")
         || name.starts_with("feed-v1-result")
   })
}

fn evidence_source_file(relative: &Path) -> bool
{
   let name = relative.file_name().and_then(|name| name.to_str()).unwrap_or("");
   if name == ".gitignore"
   {
      return true;
   }
   matches!(relative.extension().and_then(|extension| extension.to_str()),
      Some("rs" | "swift" | "m" | "h" | "plist" | "yml" | "yaml" | "md" | "sh"
         | "pbxproj" | "xcworkspacedata" | "xcscheme" | "xcconfig" | "toml" | "lock" | "resolved"
         | "json" | "png" | "ttf")
   )
}

fn evidence_runtime_file(relative: &Path) -> bool
{
   let name = relative.file_name().and_then(|name| name.to_str()).unwrap_or("");
   name == ".DS_Store"
      || name == "latest.json"
      || name == "latest.md"
      || name == "evidence-manifest.json"
      || name == "cleanup.json"
      || name == "device-before.json"
      || name == "device-after.json"
      || name == "lock-before-build.json"
      || name == "lock-before-test.json"
      || name == "xcode-destinations.txt"
      || name.starts_with("oxide-feed-v1-")
      || name.starts_with("feed-v1-")
      || matches!(relative.extension().and_then(|extension| extension.to_str()),
         Some("log" | "trace" | "atrc")
      )
}

fn collect_files(root: &Path) -> Result<Vec<PathBuf>, String>
{
   let root_type = fs::symlink_metadata(root)
      .map_err(|error| format!("read evidence root metadata {}: {error}", root.display()))?
      .file_type();
   if root_type.is_symlink()
   {
      return Err(format!("evidence root is a symlink: {}", root.display()));
   }
   if !root_type.is_dir()
   {
      return Err(format!("evidence root is not a directory: {}", root.display()));
   }
   let mut pending = vec![root.to_path_buf()];
   let mut files = Vec::new();
   while let Some(directory) = pending.pop()
   {
      let entries = fs::read_dir(&directory).map_err(|error| format!("read directory {}: {error}", directory.display()))?;
      for entry in entries
      {
         let entry = entry.map_err(|error| format!("read directory entry {}: {error}", directory.display()))?;
         let path = entry.path();
         let file_type = entry.file_type().map_err(|error| format!("file type {}: {error}", path.display()))?;
         if file_type.is_symlink()
         {
            return Err(format!("evidence path is a symlink: {}", path.display()));
         }
         if file_type.is_dir()
         {
            pending.push(path);
         }
         else if file_type.is_file()
         {
            files.push(path);
         }
         else
         {
            return Err(format!("evidence path is not a regular file or directory: {}", path.display()));
         }
      }
   }
   files.sort();
   Ok(files)
}

fn attachment_export_map(files: &[PathBuf]) -> Result<BTreeMap<String, PathBuf>, String>
{
   let mut output = BTreeMap::new();
   let mut exported_paths = BTreeSet::new();
   #[cfg(unix)]
   let mut exported_identities = BTreeSet::new();
   for manifest in files.iter().filter(|path| path.file_name().and_then(|name| name.to_str()) == Some("manifest.json"))
   {
      let bytes = fs::read(manifest).map_err(|error| format!("read attachment manifest {}: {error}", manifest.display()))?;
      let value: serde_json::Value = serde_json::from_slice(&bytes)
         .map_err(|error| format!("parse attachment manifest {}: {error}", manifest.display()))?;
      let test_details = value.as_array()
         .ok_or_else(|| format!("attachment manifest {} is not an array", manifest.display()))?;
      if test_details.len() != 1
      {
         return Err(format!(
            "attachment manifest {} has {} test details, expected 1",
            manifest.display(),
            test_details.len()
         ));
      }
      let detail = &test_details[0];
      let identifier = detail.get("testIdentifier").and_then(serde_json::Value::as_str)
         .ok_or_else(|| format!("attachment manifest {} has no test identifier", manifest.display()))?;
      if identifier != ATTACHMENT_TEST_IDENTIFIER
      {
         return Err(format!(
            "attachment manifest {} test identifier {identifier} differs from {ATTACHMENT_TEST_IDENTIFIER}",
            manifest.display()
         ));
      }
      let attachments = detail.get("attachments").and_then(serde_json::Value::as_array)
         .ok_or_else(|| format!("attachment manifest {} has no attachment array", manifest.display()))?;
      if attachments.len() != ATTACHMENT_COUNT
      {
         return Err(format!(
            "attachment manifest {} has {} attachments, expected {ATTACHMENT_COUNT}",
            manifest.display(),
            attachments.len()
         ));
      }
      for attachment in attachments
      {
         let suggested = attachment.get("suggestedHumanReadableName").and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("attachment manifest {} contains an attachment without a suggested name", manifest.display()))?;
         let exported = attachment.get("exportedFileName").and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("attachment manifest {} contains an attachment without an exported filename", manifest.display()))?;
         let canonical_name = canonical_attachment_name(suggested)?;
         let path = manifest.parent().unwrap_or(Path::new("")).join(exported);
         if !path.is_file()
         {
            return Err(format!("attachment manifest references missing {}", path.display()));
         }
         let canonical_path = fs::canonicalize(&path)
            .map_err(|error| format!("canonicalize attachment {}: {error}", path.display()))?;
         if !exported_paths.insert(canonical_path.clone())
         {
            return Err(format!("multiple attachment names alias {}", canonical_path.display()));
         }
         #[cfg(unix)]
         {
            let metadata = fs::metadata(&canonical_path)
               .map_err(|error| format!("read attachment metadata {}: {error}", canonical_path.display()))?;
            if !exported_identities.insert((metadata.dev(), metadata.ino()))
            {
               return Err(format!("multiple attachment names alias one file identity at {}", canonical_path.display()));
            }
         }
         if output.insert(canonical_name.clone(), canonical_path).is_some()
         {
            return Err(format!("duplicate canonical exported attachment name {canonical_name}"));
         }
      }
   }
   Ok(output)
}

fn canonical_attachment_name(suggested: &str) -> Result<String, String>
{
   let Some(stem) = suggested.strip_suffix(".png") else
   {
      return Err(format!("exported attachment name {suggested} is not a PNG"));
   };
   let Some((canonical, suffix)) = stem.rsplit_once("_0_") else
   {
      return Ok(suggested.to_string());
   };
   if !uuid_is_valid(suffix)
   {
      return Err(format!("exported attachment name {suggested} has a malformed Xcode suffix"));
   }
   Ok(format!("{canonical}.png"))
}

fn uuid_is_valid(value: &str) -> bool
{
   value.len() == 36 && value.bytes().enumerate().all(|(index, byte)| {
      if matches!(index, 8 | 13 | 18 | 23)
      {
         byte == b'-'
      }
      else
      {
         byte.is_ascii_hexdigit()
      }
   })
}

fn hash_directory(root: &Path) -> Result<String, String>
{
   let files = collect_files(root)?;
   let mut digest = Sha256::new();
   for path in files
   {
      let relative = path.strip_prefix(root)
         .map_err(|error| format!("artifact relative path {}: {error}", path.display()))?;
      let relative_bytes = relative.to_string_lossy();
      digest.update((relative_bytes.len() as u64).to_le_bytes());
      digest.update(relative_bytes.as_bytes());
      let bytes = fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
      digest.update((bytes.len() as u64).to_le_bytes());
      digest.update(&bytes);
   }
   Ok(hex(&digest.finalize()))
}

fn sha256_file(path: &Path) -> Result<String, String>
{
   let bytes = fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
   Ok(sha256_bytes(&bytes))
}

fn is_sha256(value: &str) -> bool
{
   value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a' ..= b'f').contains(&byte))
}

fn sha256_bytes(bytes: &[u8]) -> String
{
   let mut digest = Sha256::new();
   digest.update(bytes);
   hex(&digest.finalize())
}

fn hex(bytes: &[u8]) -> String
{
   const DIGITS: &[u8; 16] = b"0123456789abcdef";
   let mut output = String::with_capacity(bytes.len() * 2);
   for &byte in bytes
   {
      output.push(DIGITS[(byte >> 4) as usize] as char);
      output.push(DIGITS[(byte & 15) as usize] as char);
   }
   output
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String>
{
   let parent = path.parent().ok_or_else(|| format!("output {} has no parent", path.display()))?;
   fs::create_dir_all(parent).map_err(|error| format!("create output directory {}: {error}", parent.display()))?;
   let file_name = path.file_name().and_then(|name| name.to_str())
      .ok_or_else(|| format!("output {} has a non-UTF8 filename", path.display()))?;
   let temporary = parent.join(format!(".{file_name}.tmp"));
   let mut file = File::create(&temporary).map_err(|error| format!("create {}: {error}", temporary.display()))?;
   file.write_all(bytes).map_err(|error| format!("write {}: {error}", temporary.display()))?;
   file.sync_all().map_err(|error| format!("sync {}: {error}", temporary.display()))?;
   fs::rename(&temporary, path).map_err(|error| format!("rename {} to {}: {error}", temporary.display(), path.display()))
}
