mod analysis_bridge;
mod acquisition_validity;
mod common_gpu;
mod comparator_qualification;
mod correctness;
mod deep_attribution;
mod energy;
mod launch;
mod resource;
mod system_trace;
mod time_profiler;
mod trace;
mod trusted_input;

use anyhow::{bail, ensure, Context, Result};
use oxide_benchmark_spec::{admit_comparator_acceptance, balanced_comparison_order, canonical_apple_campaign_plan_json, canonical_instrumentation_calibration_input_json, comparison_seed_from_content_sha256, load_release_candidate_capture_plan, promote_release_candidates, reduce_instrumentation_calibration, validate_apple_campaign_contract, validate_runnable_apple_campaign_plan, validate_scenario, AcquisitionChunkBudget, AppleCampaignBudgetComponent, AppleCampaignEvidenceRole, AppleCampaignPassRole, AppleCampaignPassTimingSpec, AppleCampaignPlanSpec, ApplePrAcquisitionSpec, ApplePrPlanSpec, ArtifactIdentity, BudgetSpec, ComparatorAdmissionExpectation, ComparatorIdentity, ComparisonOrder, DecimalU64, InstrumentationCalibrationInput, InstrumentationCalibrationReport, Platform, ReleasePromotionReport, RoleCount, ScenarioSpec, Tier, TraceEvent};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::ops::{Deref, DerefMut};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub use analysis_bridge::{materialize_macos_analyzer_bundle, MacOsAnalyzerBundleManifest, MACOS_COMMON_GPU_METRIC_SOURCE_PREFIX, MACOS_ENERGY_METRIC_SOURCE_PREFIX, MACOS_INPUT_TO_PRESENT_METRIC_SOURCE, MACOS_LAUNCH_METRIC_SOURCE_PREFIX, MACOS_RESOURCE_METRIC_SOURCE_PREFIX, MACOS_VISUAL_TO_PRESENT_METRIC_SOURCE};
pub use acquisition_validity::{collect_macos_background_observation, collect_macos_environment_snapshot, collect_macos_pre_pair_environment, macos_environment_policies, reduce_macos_acquisition_validity, reduce_macos_acquisition_validity_with_pre_pair_environment, validate_macos_acquisition_validity, MacOsAcquisitionValidityReport, MacOsBackgroundObservation, MacOsDisplayObservation, MacOsEnvironmentComparisonPolicy, MacOsEnvironmentFieldPolicy, MacOsEnvironmentSnapshot, MacOsEnvironmentValue, MacOsMeasuredInputObservation, MacOsNoisyProcess, MacOsObservationAvailability, MacOsOpportunityObservation, MacOsPowerSource, MacOsPrePairEnvironmentObservation, MacOsSurfaceContractObservation, MacOsThermalState, MACOS_AUTHORITATIVE_INPUT_SCOPE, MACOS_BACKGROUND_OBSERVATION_SECONDS, MACOS_MINIMUM_CORRELATED_OPPORTUNITIES_PER_SESSION, MACOS_RAW_APPLICATION_RECEIPT_STATUS_COMPLETE, MACOS_RAW_APPLICATION_RECEIPT_STATUS_UNAVAILABLE};
pub use common_gpu::{reduce_macos_common_gpu, MacOsCommonGpuPhaseSummary, MacOsCommonGpuSample, MacOsCommonGpuSamples, MacOsCommonGpuSummary};
pub use comparator_qualification::{canonical_macos_comparator_qualification_plan_json, reduce_macos_comparator_qualification, validate_macos_comparator_qualification_plan, MacOsComparatorCpuFinding, MacOsComparatorFindingDisposition, MacOsComparatorFindingKind, MacOsComparatorProfileEvidence, MacOsComparatorProfileQualification, MacOsComparatorQualificationPlan, MacOsComparatorQualificationReport, MacOsComparatorRuntimeAttestation, MacOsComparatorScale, MacOsComparatorScaleDimension, MacOsComparatorScaleOverlay, MacOsComparatorScaleTransform, MacOsComparatorScaleVariant, MacOsComparatorSide, MacOsComparatorStall, MacOsComparatorStallFinding, MACOS_COMPARATOR_PROFILE_MAX_SECONDS, MACOS_COMPARATOR_QUALIFICATION_SCHEMA_VERSION};
pub use energy::{load_macos_external_meter_config, macos_energy_calibration_sha256, macos_energy_unavailable_artifact, reduce_macos_energy, validate_macos_energy_adapter_request, validate_macos_external_meter_config, MacOsEnergyAdapterReady, MacOsEnergyAdapterRequest, MacOsEnergyBatteryState, MacOsEnergyCalibration, MacOsEnergyPhaseSummary, MacOsEnergyPowerTopology, MacOsEnergyRefreshBehavior, MacOsEnergySummary, MacOsEnergyUnavailableArtifact, MacOsExternalMeterConfig, MacOsExternalMeterRawArtifact, MacOsExternalMeterSample, MACOS_ENERGY_AVAILABILITY, MACOS_ENERGY_MEASUREMENT_SECONDS, MACOS_ENERGY_STABILIZATION_SECONDS, MACOS_ENERGY_UNAVAILABLE};
pub use resource::{reduce_macos_resource_artifact, MacOsResourceSummary};
pub use system_trace::{reduce_macos_system_trace, MacOsSystemTracePhaseSummary, MacOsSystemTraceSummary, MACOS_SYSTEM_TRACE_AVAILABILITY};
pub use time_profiler::{reduce_macos_time_profiler_trace, MacOsTimeProfilerArtifact, MacOsTimeProfilerPhase, MacOsTimeProfilerStack};
use common_gpu::MacOsCommonGpuCollector;
use energy::MacOsEnergyAdapterProcess;
use resource::{MacOsResourceArtifact, MacOsResourceCollector, MacOsResourceIdentity};
use system_trace::{MacOsSystemTraceCollector, MacOsSystemTracePaths};
use launch::{validate_macos_launch_application_receipts, validate_macos_launch_cache_primer_receipts, validate_macos_launch_ui_controller_receipt, validate_macos_launch_ui_controller_start_receipt, MacOsLaunchApplicationDidFinishReceipt, MacOsLaunchCompleteReceipt, MacOsLaunchFirstCompleteUIReceipt, MacOsLaunchReadinessReceipt, MacOsLaunchUIControllerReceipt, MacOsLaunchUIControllerStartReceipt};

pub use correctness::{compare_apple_correctness_evidence, compare_apple_rapid_visual_checkpoint, reduce_apple_correctness_evidence, AppleCorrectnessVisualCheckpoint, AppleCorrectnessVisualReport, AppleRapidVisualCheckpointReport};
pub use deep_attribution::{build_macos_deep_attribution_schedule, reduce_macos_deep_attribution_exports, validate_macos_deep_attribution_receipt, MacOsDeepAttributionCollector, MacOsDeepAttributionMetricSummary, MacOsDeepAttributionPaths, MacOsDeepAttributionReceipt, MacOsDeepAttributionSchedule, MacOsDeepAttributionSchemaExport, MacOsDeepAttributionSchemaPhase, MacOsDeepAttributionSchemaSummary, MacOsDeepAttributionSession, MacOsDeepAttributionSummary, MACOS_DEEP_ATTRIBUTION_CROSS_FRAMEWORK_SCOPE, MACOS_DEEP_ATTRIBUTION_WORKING_SET_LIMIT_BYTES};
pub use launch::{validate_macos_launch_evidence, validate_macos_launch_preparation_receipt, MacOsLaunchClass, MacOsLaunchEvidence, MacOsLaunchExpectation, MacOsLaunchPreparationReceipt, MACOS_LAUNCH_PRESENTATION_CALIBRATION_PENDING};
pub use trace::{correlate_macos_launch_presentation_trace, correlate_macos_presentation_trace, MacOsLaunchClockAnchorInterval, MacOsLaunchPresentationCorrelation, MacOsPresentationCorrelation, MacOsPresentationCorrelationArtifact, MacOsUncorrelatedVisualGeneration};
pub use trusted_input::{compile_macos_trusted_input_trace, MacOsDeclaredApplicationStimulus, MacOsTrustedInputCommand, MacOsTrustedInputPlan, MacOsTrustedInputReceiptManifest, MacOsTrustedInputReceiptManifestEntry, MacOsTrustedInputScenarioPreflight};

const MACOS_TRACE_WORKING_SET_LIMIT_BYTES: u64 = 512 * 1_024 * 1_024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ComparisonSide
{
   Oxide,
   Native,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MacOsCampaignScope
{
   CorrectnessOnly,
   QualificationOnly,
   Full,
}

impl ComparisonSide
{
   fn as_str(self) -> &'static str
   {
      match self
      {
         Self::Oxide => "oxide",
         Self::Native => "native",
      }
   }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsCampaignSession
{
   pub chunk_id: String,
   pub pass_id: String,
   pub pack_id: String,
   pub pass_role: AppleCampaignPassRole,
   pub evidence_role: AppleCampaignEvidenceRole,
   pub collector: Option<String>,
   pub launch_class: Option<String>,
   #[serde(default, skip_serializing_if = "Option::is_none")]
   pub timing: Option<AppleCampaignPassTimingSpec>,
   pub pair_index: u32,
   pub order: ComparisonOrder,
   pub side: ComparisonSide,
   pub max_occupied_seconds: u64,
   #[serde(default, skip_serializing_if = "Option::is_none")]
   pub scale_overlay: Option<ArtifactIdentity>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsCampaignPlan
{
   pub schema_version: u32,
   pub run_id: String,
   pub plan_sha256: String,
   pub seed: DecimalU64,
   #[serde(default, skip_serializing_if = "Option::is_none")]
   pub plan_resource_path: Option<String>,
   pub sessions: Vec<MacOsCampaignSession>,
}

#[derive(Clone, Debug)]
pub struct MacOsCampaignConfig
{
   pub build_manifest_path: PathBuf,
   pub plan_path: PathBuf,
   pub acquisition_path: PathBuf,
   pub output_root: PathBuf,
   pub run_id: String,
   pub capture_presentation_traces: bool,
   pub xctrace_template: String,
   pub scope: MacOsCampaignScope,
   pub resume: bool,
   pub require_live_gui_session: bool,
   pub energy_meter_config_path: Option<PathBuf>,
   pub instrumentation_calibration_path: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct MacOsDensitySessionConfig
{
   pub build_manifest_path: PathBuf,
   pub plan_path: PathBuf,
   pub output_root: PathBuf,
   pub run_id: String,
   pub sessions: Vec<MacOsDensitySession>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsDensitySession
{
   pub session_id: String,
   pub pass_id: String,
   pub pack_id: String,
   pub pair_index: u32,
   pub side: ComparisonSide,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MacOsDensitySessionResult
{
   pub session_id: String,
   pub campaign: MacOsCampaignSessionResult,
   pub telemetry_path: String,
   pub telemetry_sha256: String,
}

#[derive(Clone, Debug)]
pub struct MacOsReleaseCandidateCaptureConfig
{
   pub build_manifest_path: PathBuf,
   pub capture_plan_path: PathBuf,
   pub spec_root: PathBuf,
   pub output_root: PathBuf,
   pub promoted_spec_root: PathBuf,
}

#[derive(Debug, Serialize)]
pub struct MacOsReleaseCandidateCaptureReport
{
   pub schema_version: u32,
   pub build_manifest_sha256: String,
   pub capture_plan_sha256: String,
   pub captured_scenario_ids: Vec<String>,
   pub captured_checkpoint_count: u32,
   pub promotion: ReleasePromotionReport,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MacOsRapidMode
{
   Visual,
   Timing,
}

impl MacOsRapidMode
{
   fn as_str(self) -> &'static str
   {
      match self
      {
         Self::Visual => "visual",
         Self::Timing => "timing",
      }
   }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MacOsRapidSide
{
   Oxide,
   Native,
   Both,
}

#[derive(Clone, Debug)]
pub struct MacOsRapidRunConfig
{
   pub build_manifest_path: PathBuf,
   pub spec_root: PathBuf,
   pub output_root: PathBuf,
   pub scenario_id: String,
   pub checkpoint_id: String,
   pub side: MacOsRapidSide,
   pub mode: MacOsRapidMode,
   pub iterations: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MacOsRapidSideResult
{
   pub side: ComparisonSide,
   pub evidence_path: String,
   pub evidence_sha256: String,
   pub samples_ns: Vec<u64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MacOsRapidRunReport
{
   pub schema_version: u32,
   pub mode: MacOsRapidMode,
   pub scenario_id: String,
   pub checkpoint_id: String,
   pub iterations: u32,
   pub sides: Vec<MacOsRapidSideResult>,
   pub visual: Option<AppleRapidVisualCheckpointReport>,
   pub accepted: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsCampaignSessionResult
{
   pub session: MacOsCampaignSession,
   pub generation: String,
   pub executable_sha256: String,
   pub artifact_sha256: String,
   pub acknowledgement_sha256: String,
   #[serde(default)]
   pub surface_receipt_path: Option<String>,
   #[serde(default)]
   pub surface_receipt_sha256: Option<String>,
   pub trace_path: Option<String>,
   pub trace_toc_sha256: Option<String>,
   pub trace_signposts_sha256: Option<String>,
   pub trace_updates_sha256: Option<String>,
   pub trace_frame_lifetimes_sha256: Option<String>,
   pub trace_correlation_path: Option<String>,
   pub trace_correlation_sha256: Option<String>,
   #[serde(default)]
   pub trusted_input_receipt_manifest_path: Option<String>,
   #[serde(default)]
   pub trusted_input_receipt_manifest_sha256: Option<String>,
   #[serde(default)]
   pub time_profiler_trace_path: Option<String>,
   #[serde(default)]
   pub time_profiler_toc_sha256: Option<String>,
   #[serde(default)]
   pub time_profiler_signposts_sha256: Option<String>,
   #[serde(default)]
   pub time_profiler_samples_sha256: Option<String>,
   #[serde(default)]
   pub time_profiler_artifact_path: Option<String>,
   #[serde(default)]
   pub time_profiler_artifact_sha256: Option<String>,
   #[serde(default)]
   pub system_trace_path: Option<String>,
   #[serde(default)]
   pub system_trace_toc_sha256: Option<String>,
   #[serde(default)]
   pub system_trace_signposts_sha256: Option<String>,
   #[serde(default)]
   pub system_trace_thread_info_sha256: Option<String>,
   #[serde(default)]
   pub system_trace_thread_state_sha256: Option<String>,
   #[serde(default)]
   pub system_trace_context_switch_sha256: Option<String>,
   #[serde(default)]
   pub system_trace_artifact_path: Option<String>,
   #[serde(default)]
   pub system_trace_artifact_sha256: Option<String>,
   #[serde(default)]
   pub common_gpu_trace_path: Option<String>,
   #[serde(default)]
   pub common_gpu_toc_sha256: Option<String>,
   #[serde(default)]
   pub common_gpu_signposts_sha256: Option<String>,
   #[serde(default)]
   pub common_gpu_samples_path: Option<String>,
   #[serde(default)]
   pub common_gpu_samples_sha256: Option<String>,
   #[serde(default)]
   pub common_gpu_artifact_path: Option<String>,
   #[serde(default)]
   pub common_gpu_artifact_sha256: Option<String>,
   #[serde(default)]
   pub launch_evidence_path: Option<String>,
   #[serde(default)]
   pub launch_evidence_sha256: Option<String>,
   #[serde(default)]
   pub energy_config_sha256: Option<String>,
   #[serde(default)]
   pub energy_request_sha256: Option<String>,
   #[serde(default)]
   pub energy_ready_sha256: Option<String>,
   #[serde(default)]
   pub energy_raw_path: Option<String>,
   #[serde(default)]
   pub energy_raw_sha256: Option<String>,
   #[serde(default)]
   pub energy_summary_path: Option<String>,
   #[serde(default)]
   pub energy_summary_sha256: Option<String>,
   pub resource_path: String,
   pub resource_sha256: String,
   pub resource_availability: String,
   pub disposition: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MacOsCampaignReport
{
   pub schema_version: u32,
   pub run_id: String,
   pub plan_sha256: String,
   pub build_manifest_sha256: String,
   #[serde(default, skip_serializing_if = "Option::is_none")]
   pub acquisition_validity: Option<ArtifactIdentity>,
   pub sessions: Vec<MacOsCampaignSessionResult>,
   pub correctness_pairs: Vec<MacOsCorrectnessPairResult>,
   pub scope: MacOsCampaignScope,
   pub acquisition_complete: bool,
   pub correctness_accepted: bool,
   pub correctness_eligible_for_measured_acquisition: bool,
   pub authoritative_eligible: bool,
   pub complete: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MacOsCorrectnessPairResult
{
   pub pack_id: String,
   pub pair_index: u32,
   pub report_path: String,
   pub report_sha256: String,
   pub accepted_checkpoint_count: u64,
   pub rejected_checkpoint_count: u64,
   pub accepted: bool,
   pub report: AppleCorrectnessVisualReport,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MacOsPairCheckpoint
{
   pub schema_version: u32,
   pub run_id: String,
   pub plan_sha256: String,
   pub build_manifest_sha256: String,
   pub chunk_id: String,
   pub pass_id: String,
   pub pack_id: String,
   pub pair_index: u32,
   pub predecessor_pair_sha256: Option<String>,
   pub sessions: Vec<MacOsCampaignSessionResult>,
   #[serde(default)]
   pub correctness: Option<MacOsCorrectnessPairResult>,
   pub complete: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuildManifest
{
   schema_version: u32,
   platform: String,
   configuration: String,
   architecture: String,
   workspace_git_head: String,
   workspace_status_sha256: String,
   benchmark_source_manifest_sha256: String,
   specification_manifest_sha256: String,
   xcode_version: String,
   swift_version: String,
   rust_version: String,
   build_command_sha256: String,
   controller: BuildController,
   products: Vec<BuildProduct>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuildController
{
   scheme: String,
   xctestrun_path: String,
   xctestrun_sha256: String,
   runner_bundle_path: String,
   runner_bundle_manifest_sha256: String,
   runner_bundle_file_count: u64,
   runner_bundle_bytes: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuildProduct
{
   implementation_id: String,
   scheme: String,
   bundle_path: String,
   executable_path: String,
   executable_sha256: String,
   bundle_manifest_sha256: String,
   bundle_file_count: u64,
   bundle_bytes: u64,
}

#[derive(Clone, Debug)]
struct VerifiedLaunchController
{
   xctestrun: PathBuf,
   xctestrun_sha256: String,
   runner_bundle: PathBuf,
   runner_bundle_manifest_sha256: String,
   runner_bundle_file_count: u64,
   runner_bundle_bytes: u64,
}

#[derive(Debug)]
struct VerifiedProduct
{
   implementation_id: String,
   bundle: PathBuf,
   executable: PathBuf,
   executable_sha256: String,
   bundle_manifest_sha256: String,
   bundle_file_count: u64,
   bundle_bytes: u64,
   launch_controller: VerifiedLaunchController,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionEnvelope
{
   schema_version: u32,
   #[serde(rename = "runID")]
   run_id: String,
   #[serde(rename = "planSHA256")]
   plan_sha256: String,
   #[serde(rename = "chunkID")]
   chunk_id: String,
   #[serde(rename = "passID")]
   pass_id: String,
   pair_index: u32,
   side: ComparisonSide,
   generation: String,
   #[serde(rename = "packID")]
   pack_id: String,
   #[serde(rename = "telemetrySHA256")]
   telemetry_sha256: String,
   #[serde(rename = "telemetryByteCount")]
   telemetry_byte_count: u64,
   telemetry_coverage: Option<SessionArtifactIdentity>,
   surface_receipt: SessionArtifactIdentity,
   timebase_numerator: u32,
   timebase_denominator: u32,
   injection_scope: String,
   validation: String,
}

#[derive(Debug, Deserialize)]
struct SessionArtifactIdentity
{
   path: String,
   sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct MacOsSurfaceReceipt
{
   schema_version: u32,
   #[serde(rename = "runID")]
   run_id: String,
   #[serde(rename = "planSHA256")]
   plan_sha256: String,
   #[serde(rename = "chunkID")]
   chunk_id: String,
   #[serde(rename = "passID")]
   pass_id: String,
   pair_index: u32,
   side: ComparisonSide,
   generation: String,
   #[serde(rename = "packID")]
   pack_id: String,
   snapshot: MacOsSurfaceSnapshot,
   observed_refresh_millihz: Option<u64>,
   observed_refresh_source: String,
   validation: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct MacOsSurfaceSnapshot
{
   window_logical_width_milli_points: u64,
   window_logical_height_milli_points: u64,
   viewport_logical_width_milli_points: u64,
   viewport_logical_height_milli_points: u64,
   backing_pixel_width: u64,
   backing_pixel_height: u64,
   inset_top_milli_points: u64,
   inset_left_milli_points: u64,
   inset_bottom_milli_points: u64,
   inset_right_milli_points: u64,
   backing_scale_milli: u64,
   final_color_format: String,
   final_color_space: String,
   alpha_mode: String,
   sample_count: u64,
   compositor_scaling: String,
   target_refresh_policy: String,
   target_refresh_millihz: u64,
   surface_implementation: String,
   internal_color_format: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionTelemetryCoverage
{
   schema_version: u32,
   side: ComparisonSide,
   #[serde(rename = "passID")]
   pass_id: String,
   entries: Vec<SessionTelemetryCoverageEntry>,
   validation: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionTelemetryCoverageEntry
{
   kind: String,
   raw_value: u16,
   availability: String,
   observed_count: u64,
   source: String,
}

const MACOS_TELEMETRY_KIND_NAMES: [&str; 33] = [
   "scenarioBegin", "scenarioEnd", "phaseBegin", "phaseEnd", "checkpointBegin", "checkpointEnd",
   "inputReceived", "mutationBegin", "mutationEnd", "layoutBegin", "layoutEnd", "sceneUpdateBegin",
   "sceneUpdateEnd", "renderPrepareBegin", "renderPrepareEnd", "encodeBegin", "encodeEnd", "commandSubmit",
   "gpuStart", "gpuEnd", "presentation", "firstMeaningfulFrame", "readyToInput", "displayOpportunity",
   "resetComplete", "callbackCadence", "drawableWait", "inflightDepth", "updateBacklog", "logicalUpdateCompleted",
   "logicalUpdateSkipped", "quiescence", "gpuDuration",
];

pub fn validate_macos_telemetry_coverage(coverage_bytes: &[u8], telemetry: &[u8], side: ComparisonSide, pass_id: &str) -> Result<()>
{
   const HEADER_BYTES: usize = 136;
   const RECORD_BYTES: usize = 44;
   const FOOTER_BYTES: usize = 32;
   if telemetry.len() < HEADER_BYTES + FOOTER_BYTES || &telemetry[..8] != b"OXBTEL02"
   {
      bail!("macOS telemetry coverage source has an invalid binary header");
   }
   let schema = telemetry_u32(telemetry, 8)?;
   let header_bytes = telemetry_u32(telemetry, 12)? as usize;
   let record_bytes = telemetry_u32(telemetry, 16)? as usize;
   let record_count = usize::try_from(telemetry_u64(telemetry, 24)?).context("macOS telemetry coverage record count exceeds usize")?;
   let expected_bytes = HEADER_BYTES.checked_add(record_count.checked_mul(RECORD_BYTES).context("macOS telemetry coverage record bytes overflow")?)
      .and_then(|value| value.checked_add(FOOTER_BYTES)).context("macOS telemetry coverage byte count overflow")?;
   if schema != 2 || header_bytes != HEADER_BYTES || record_bytes != RECORD_BYTES || telemetry.len() != expected_bytes
      || Sha256::digest(&telemetry[..telemetry.len() - FOOTER_BYTES])[..] != telemetry[telemetry.len() - FOOTER_BYTES..]
   {
      bail!("macOS telemetry coverage source binary is malformed");
   }
   let mut observed = [0_u64; 33];
   for index in 0..record_count
   {
      let offset = HEADER_BYTES + index * RECORD_BYTES;
      let kind = telemetry_u16(telemetry, offset + 16)?;
      if !(1..=33).contains(&kind)
      {
         bail!("macOS telemetry coverage source contains unknown event kind {}", kind);
      }
      observed[usize::from(kind - 1)] += 1;
   }
   let coverage: SessionTelemetryCoverage = serde_json::from_slice(coverage_bytes).context("decoding macOS telemetry coverage")?;
   if coverage.schema_version != 1 || coverage.side != side || coverage.pass_id != pass_id
      || coverage.validation != "complete-kind-availability-and-observed-counts"
      || coverage.entries.len() != MACOS_TELEMETRY_KIND_NAMES.len()
   {
      bail!("macOS telemetry coverage identity or shape differs from its session");
   }
   let diagnostics_enabled = pass_id == "common-gpu" || pass_id == "full-attribution";
   for (index, entry) in coverage.entries.iter().enumerate()
   {
      let raw_value = u16::try_from(index + 1).context("macOS telemetry kind index exceeds u16")?;
      let expected_availability = match raw_value
      {
         1..=6 | 23 | 24 | 26 => "ring-required",
         7..=9 | 12 | 13 | 25 | 30 | 32 => "ring-conditional",
         21 | 22 => "external-evidence",
         14..=18 | 27 | 33 if side == ComparisonSide::Oxide && diagnostics_enabled => "oxide-diagnostic",
         14..=18 | 27 | 33 if side == ComparisonSide::Oxide => "diagnostic-not-enabled-for-pass",
         _ => "unavailable",
      };
      if entry.raw_value != raw_value || entry.kind != MACOS_TELEMETRY_KIND_NAMES[index]
         || entry.availability != expected_availability || entry.observed_count != observed[index] || entry.source.is_empty()
      {
         bail!("macOS telemetry coverage entry {} differs from its binary or availability contract", raw_value);
      }
      if (expected_availability == "ring-required" && entry.observed_count == 0)
         || matches!(expected_availability, "external-evidence" | "unavailable" | "diagnostic-not-enabled-for-pass") && entry.observed_count != 0
      {
         bail!("macOS telemetry coverage entry {} has an invalid observed count", raw_value);
      }
   }
   for (begin, end) in [(1_usize, 2_usize), (3, 4), (5, 6), (8, 9), (12, 13)]
   {
      if observed[begin - 1] != observed[end - 1]
      {
         bail!("macOS telemetry coverage paired event counts differ for {} and {}", begin, end);
      }
   }
   if observed[12] != observed[29]
   {
      bail!("macOS telemetry coverage scene-update and logical-completion counts differ");
   }
   Ok(())
}

fn telemetry_u16(bytes: &[u8], offset: usize) -> Result<u16>
{
   let value = bytes.get(offset..offset + 2).context("macOS telemetry coverage u16 is truncated")?;
   Ok(u16::from_le_bytes(value.try_into().context("macOS telemetry coverage u16 has invalid width")?))
}

fn telemetry_u32(bytes: &[u8], offset: usize) -> Result<u32>
{
   let value = bytes.get(offset..offset + 4).context("macOS telemetry coverage u32 is truncated")?;
   Ok(u32::from_le_bytes(value.try_into().context("macOS telemetry coverage u32 has invalid width")?))
}

fn telemetry_u64(bytes: &[u8], offset: usize) -> Result<u64>
{
   let value = bytes.get(offset..offset + 8).context("macOS telemetry coverage u64 is truncated")?;
   Ok(u64::from_le_bytes(value.try_into().context("macOS telemetry coverage u64 has invalid width")?))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionAcknowledgement
{
   schema_version: u32,
   generation: String,
   #[serde(rename = "artifactSHA256")]
   artifact_sha256: String,
   durable: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredControllerReceipt
{
   schema_version: u32,
   generation: String,
   #[serde(rename = "executableSHA256")]
   executable_sha256: String,
   #[serde(rename = "resourceSHA256")]
   resource_sha256: String,
   pid: u32,
   launch_t0: u64,
   complete_timestamp: u64,
   complete: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionReady
{
   schema_version: u32,
   #[serde(rename = "runID")]
   run_id: String,
   #[serde(rename = "planSHA256")]
   plan_sha256: String,
   #[serde(rename = "chunkID")]
   chunk_id: String,
   #[serde(rename = "passID")]
   pass_id: String,
   pair_index: u32,
   side: ComparisonSide,
   generation: String,
   #[serde(rename = "packID")]
   pack_id: String,
   ready_timestamp: u64,
   durable: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ControllerReceipt<'a>
{
   schema_version: u32,
   #[serde(rename = "runID")]
   run_id: &'a str,
   #[serde(rename = "planSHA256")]
   plan_sha256: &'a str,
   #[serde(rename = "chunkID")]
   chunk_id: &'a str,
   #[serde(rename = "passID")]
   pass_id: &'a str,
   pair_index: u32,
   side: ComparisonSide,
   generation: &'a str,
   #[serde(rename = "executableSHA256")]
   executable_sha256: &'a str,
   #[serde(rename = "packID")]
   pack_id: &'a str,
   #[serde(rename = "resourceSHA256")]
   resource_sha256: &'a str,
   pid: u32,
   launch_t0: u64,
   ready_timestamp: u64,
   complete_timestamp: u64,
   complete: bool,
   primary_availability: &'a str,
}

pub fn build_macos_campaign_plan(acquisition: &ApplePrAcquisitionSpec, run_id: &str, plan_sha256: &str) -> Result<MacOsCampaignPlan>
{
   validate_run_id(run_id)?;
   validate_sha256(plan_sha256)?;
   let seed = comparison_seed_from_content_sha256(plan_sha256)?;
   let mut sessions = Vec::new();
   for chunk in &acquisition.controller_chunks
   {
      append_chunk_sessions(acquisition, chunk, seed.0, &mut sessions)?;
   }
   if sessions.len() != 20
   {
      bail!("macOS Apple PR campaign must contain exactly twenty process sessions, observed {}", sessions.len());
   }
   Ok(MacOsCampaignPlan {
      schema_version: 2,
      run_id: String::from(run_id),
      plan_sha256: String::from(plan_sha256),
      seed,
      plan_resource_path: None,
      sessions,
   })
}

pub fn build_generic_macos_campaign_plan(plan: &AppleCampaignPlanSpec, budget: &BudgetSpec, spec_root: &Path, plan_resource_path: &str, run_id: &str, plan_sha256: &str) -> Result<MacOsCampaignPlan>
{
   validate_run_id(run_id)?;
   validate_sha256(plan_sha256)?;
   let seed = comparison_seed_from_content_sha256(plan_sha256)?;
   validate_apple_campaign_contract(plan, budget)?;
   validate_runnable_apple_campaign_plan(spec_root, plan, budget)?;
   let resource_path = Path::new(plan_resource_path);
   if resource_path.components().count() != 2
      || resource_path.components().next() != Some(std::path::Component::Normal(std::ffi::OsStr::new("plans")))
      || resource_path.extension().and_then(|extension| extension.to_str()) != Some("json")
   {
      bail!("generic macOS campaign plan resource path must be plans/<name>.json");
   }
   let mut sessions = Vec::new();
   for pass in &plan.passes
   {
      let pass_budget = plan.budget_components.iter()
         .find(|component| component.pass_ids.contains(&pass.id))
         .with_context(|| format!("generic macOS pass {} has no budget component", pass.id))?
         .occupied_seconds;
      let selected_packs = if pass.pack_ids.is_empty()
      {
         vec![(String::from("direct"), pass.scenario_ids.as_slice())]
      }
      else
      {
         pass.pack_ids.iter().map(|pack_id| {
            let pack = plan.packs.iter().find(|pack| &pack.id == pack_id).with_context(|| format!("generic macOS pass {} refers to unknown pack {}", pass.id, pack_id))?;
            Ok((pack.id.clone(), pack.ordered_scenario_ids.as_slice()))
         }).collect::<Result<Vec<_>>>()?
      };
      let pair_count = if pass.role == AppleCampaignPassRole::Correctness {1} else {pass.pair_count};
      let pair_orders = balanced_comparison_order(seed.0, pair_count as usize);
      let launch_classes = expand_launch_classes(pass)?;
      for (pack_id, scenario_ids) in selected_packs
      {
         if scenario_ids.is_empty() || scenario_ids.iter().any(|id| !pass.scenario_ids.contains(id))
         {
            bail!("generic macOS pass {} pack {} differs from its scenario selection", pass.id, pack_id);
         }
         let timing = generic_macos_session_timing(plan, pass, scenario_ids)?;
         let max_occupied_seconds = generic_macos_session_timeout_seconds(spec_root, plan, scenario_ids, pass.role, timing.as_ref(), pass_budget)?;
         for pair_index in 0..pair_count
         {
            let order = pair_orders[pair_index as usize];
            let sides = if order == ComparisonOrder::Ab
            {
               [ComparisonSide::Native, ComparisonSide::Oxide]
            }
            else
            {
               [ComparisonSide::Oxide, ComparisonSide::Native]
            };
            for side in sides
            {
               sessions.push(MacOsCampaignSession {
                  chunk_id: pass.id.clone(),
                  pass_id: pass.id.clone(),
                  pack_id: pack_id.clone(),
                  pass_role: pass.role,
                  evidence_role: pass.evidence_role,
                  collector: pass.collector.clone(),
                  launch_class: launch_classes.get(pair_index as usize).cloned(),
                  timing: timing.clone(),
                  pair_index,
                  order,
                  side,
                  max_occupied_seconds,
                  scale_overlay: None,
               });
            }
         }
      }
   }
   if sessions.is_empty()
   {
      bail!("generic macOS campaign expanded to no process sessions");
   }
   Ok(MacOsCampaignPlan {
      schema_version: 2,
      run_id: String::from(run_id),
      plan_sha256: String::from(plan_sha256),
      seed,
      plan_resource_path: Some(String::from(plan_resource_path)),
      sessions,
   })
}

fn generic_macos_session_timing(plan: &AppleCampaignPlanSpec, pass: &oxide_benchmark_spec::AppleCampaignPassSpec, scenario_ids: &[String]) -> Result<Option<AppleCampaignPassTimingSpec>>
{
   if !matches!(pass.role, AppleCampaignPassRole::Primary | AppleCampaignPassRole::Attribution | AppleCampaignPassRole::Idle | AppleCampaignPassRole::Endurance | AppleCampaignPassRole::Energy)
   {
      return Ok(None);
   }
   let timing = plan.timing.passes.iter().find(|timing| timing.pass_id == pass.id).with_context(|| format!("generic macOS pass {} has no timing overlay", pass.id))?;
   let scenarios = scenario_ids.iter().map(|scenario_id| {
      timing.scenarios.iter().find(|scenario| &scenario.scenario_id == scenario_id).cloned().with_context(|| format!("generic macOS pass {} timing omits scenario {}", pass.id, scenario_id))
   }).collect::<Result<Vec<_>>>()?;
   Ok(Some(AppleCampaignPassTimingSpec {
      pass_id: timing.pass_id.clone(),
      reset_seconds_per_session: timing.reset_seconds_per_session,
      readiness_timeout_seconds: timing.readiness_timeout_seconds,
      scenarios,
   }))
}

fn expand_launch_classes(pass: &oxide_benchmark_spec::AppleCampaignPassSpec) -> Result<Vec<String>>
{
   if pass.role != AppleCampaignPassRole::Launch
   {
      if !pass.launch_classes.is_empty()
      {
         bail!("non-launch pass {} declares launch classes", pass.id);
      }
      return Ok(vec![]);
   }
   let mut expanded = Vec::new();
   for declaration in &pass.launch_classes
   {
      let (launch_class, count) = declaration.rsplit_once(':').with_context(|| format!("launch pass {} has malformed launch class declaration {}", pass.id, declaration))?;
      if launch_class.is_empty()
      {
         bail!("launch pass {} has an empty launch class", pass.id);
      }
      let count = count.parse::<u32>().with_context(|| format!("launch pass {} has invalid launch class count {}", pass.id, declaration))?;
      if count == 0
      {
         bail!("launch pass {} has zero sessions for launch class {}", pass.id, launch_class);
      }
      for _ in 0..count
      {
         expanded.push(String::from(launch_class));
      }
   }
   if expanded.len() != pass.pair_count as usize
   {
      bail!("launch pass {} declares {} launch-class pairs but pair_count is {}", pass.id, expanded.len(), pass.pair_count);
   }
   Ok(expanded)
}

fn generic_macos_session_timeout_seconds(spec_root: &Path, plan: &AppleCampaignPlanSpec, scenario_ids: &[String], role: AppleCampaignPassRole, timing: Option<&AppleCampaignPassTimingSpec>, pass_budget_seconds: u64) -> Result<u64>
{
   if pass_budget_seconds == 0
   {
      bail!("generic macOS pass has no occupied-time budget");
   }
   if role == AppleCampaignPassRole::Correctness
   {
      return Ok(pass_budget_seconds.min(300).max(30));
   }
   if let Some(timing) = timing
   {
      let declared_seconds = timing.scenarios.iter().try_fold(timing.reset_seconds_per_session, |total, scenario| {
         total.checked_add(scenario.setup_seconds)
            .and_then(|value| value.checked_add(scenario.warmup_seconds))
            .and_then(|value| value.checked_add(scenario.measurement.occupied_seconds()))
            .context("generic macOS timing overlay duration overflow")
      })?;
      let timeout = declared_seconds.checked_add(timing.readiness_timeout_seconds).context("generic macOS timing overlay timeout overflow")?;
      if timeout > pass_budget_seconds
      {
         bail!("generic macOS timing overlay requires {}s but its complete pass budget is {}s", timeout, pass_budget_seconds);
      }
      return Ok(timeout.max(30));
   }
   let mut duration_us = 0_u64;
   for scenario_id in scenario_ids
   {
      let binding = plan.scenarios.iter().find(|binding| &binding.id == scenario_id).with_context(|| format!("generic macOS scenario {} is unbound", scenario_id))?;
      let artifact = binding.artifact.as_ref().with_context(|| format!("generic macOS scenario {} has no artifact", scenario_id))?;
      let scenario_bytes = fs::read(spec_root.join(&artifact.path)).with_context(|| format!("reading generic macOS scenario {}", artifact.path))?;
      let scenario: ScenarioSpec = serde_json::from_slice(&scenario_bytes).with_context(|| format!("decoding generic macOS scenario {}", scenario_id))?;
      for phase in &scenario.phases
      {
         let phase_us = if let Some(duration_ms) = phase.duration_ms
         {
            duration_ms.checked_mul(1_000).context("generic macOS phase duration overflow")?
         }
         else
         {
            let trace_us = if let Some(trace) = &phase.trace
            {
               let trace_bytes = fs::read(spec_root.join(&trace.path)).with_context(|| format!("reading generic macOS trace {}", trace.path))?;
               serde_json::from_slice::<Vec<TraceEvent>>(&trace_bytes).with_context(|| format!("decoding generic macOS trace {}", trace.path))?.iter().map(|event| event.at_us).max().unwrap_or(0)
            }
            else {0};
            let checkpoint_us = scenario.parity_checkpoints.iter().filter(|checkpoint| checkpoint.phase_id == phase.id).filter_map(|checkpoint| checkpoint.at_us).max().unwrap_or(0);
            trace_us.max(checkpoint_us)
         };
         duration_us = duration_us.checked_add(phase_us).context("generic macOS session duration overflow")?;
      }
   }
   let declared_seconds = duration_us / 1_000_000 + u64::from(duration_us % 1_000_000 != 0);
   let timeout = declared_seconds.checked_add(30).context("generic macOS session timeout overflow")?;
   if timeout > pass_budget_seconds
   {
      bail!("generic macOS session requires {}s but its complete pass budget is {}s", timeout, pass_budget_seconds);
   }
   Ok(timeout.max(30))
}

pub fn macos_campaign_sessions_for_scope(plan: &MacOsCampaignPlan, scope: MacOsCampaignScope) -> Vec<&MacOsCampaignSession>
{
   plan.sessions.iter().filter(|session| {
      scope != MacOsCampaignScope::CorrectnessOnly || session.pass_id == "correctness"
   }).collect()
}

pub fn validate_macos_build_manifest(path: &Path) -> Result<()>
{
   let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
   let build: BuildManifest = serde_json::from_slice(&bytes).with_context(|| format!("decoding {}", path.display()))?;
   validate_build_manifest(&build)?;
   Ok(())
}

pub fn validate_macos_build_input_identity(build_manifest_path: &Path, plan_path: &Path) -> Result<()>
{
   let bytes = fs::read(build_manifest_path).with_context(|| format!("reading {}", build_manifest_path.display()))?;
   let build: BuildManifest = serde_json::from_slice(&bytes).with_context(|| format!("decoding {}", build_manifest_path.display()))?;
   validate_build_manifest(&build)?;
   validate_build_input_identity(&build, plan_path)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MacOsReleaseCandidateCaptureComplete
{
   schema_version: u32,
   #[serde(rename = "planSHA256")]
   plan_sha256: String,
   side: String,
   #[serde(rename = "scenarioIDs")]
   scenario_ids: Vec<String>,
   checkpoint_count: u32,
   timing_claim: String,
   validation: String,
}

pub fn capture_macos_release_candidates(config: &MacOsReleaseCandidateCaptureConfig) -> Result<MacOsReleaseCandidateCaptureReport>
{
   validate_macos_gui_session()?;
   let _wake_assertion = MacOsWakeAssertion::acquire()?;
   let build_bytes = fs::read(&config.build_manifest_path).with_context(|| format!("reading {}", config.build_manifest_path.display()))?;
   let build_manifest_sha256 = sha256(&build_bytes);
   let build: BuildManifest = serde_json::from_slice(&build_bytes).with_context(|| format!("decoding {}", config.build_manifest_path.display()))?;
   let (oxide, native) = validate_build_manifest(&build)?;
   validate_build_input_identity(&build, &config.capture_plan_path)?;
   let capture_plan_bytes = fs::read(&config.capture_plan_path).with_context(|| format!("reading {}", config.capture_plan_path.display()))?;
   let capture_plan_sha256 = sha256(&capture_plan_bytes);
   let capture_plan = load_release_candidate_capture_plan(&config.spec_root)?;
   if config.capture_plan_path != config.spec_root.join("plans/macos-release-candidate-capture.json")
   {
      bail!("release-candidate capture requires the canonical plan path");
   }
   if config.output_root.exists()
   {
      if !config.output_root.is_dir() || fs::read_dir(&config.output_root)?.next().is_some()
      {
         bail!("release-candidate capture output must be a new or empty directory: {}", config.output_root.display());
      }
   }
   else
   {
      fs::create_dir_all(&config.output_root).with_context(|| format!("creating {}", config.output_root.display()))?;
   }
   for (side, product) in [("native", &native), ("oxide", &oxide)]
   {
      validate_verified_product(product, &product.implementation_id)?;
      if exact_process_id(&product.executable)?.is_some()
      {
         bail!("release-candidate comparator is already running: {}", product.executable.display());
      }
      let stdout = File::create(config.output_root.join(format!("{side}.stdout.log")))?;
      let stderr = File::create(config.output_root.join(format!("{side}.stderr.log")))?;
      let mut command = Command::new("/usr/bin/open");
      command.args(["-n", "-W"]).arg(&product.bundle).arg("--args")
         .arg("-oxide-compare-release-candidates")
         .arg("-oxide-compare-release-candidate-plan-sha")
         .arg(&capture_plan_sha256)
         .arg("-oxide-compare-output-root")
         .arg(&config.output_root);
      let status = run_child(command, stdout, stderr, 600).with_context(|| format!("capturing {side} release candidates"))?;
      if !status.success()
      {
         bail!("{side} release-candidate comparator exited with {status}");
      }
      let complete_path = config.output_root.join(format!("{side}.evidence/capture.complete.json"));
      let complete = read_json::<MacOsReleaseCandidateCaptureComplete>(&complete_path)?;
      let expected = capture_plan.candidates.iter().map(|candidate| candidate.id.clone()).collect::<Vec<_>>();
      if complete.schema_version != 1
         || complete.plan_sha256 != capture_plan_sha256
         || complete.side != side
         || complete.scenario_ids != expected
         || complete.checkpoint_count != 18
         || complete.timing_claim != "none-correctness-untimed"
         || complete.validation != "complete-release-candidate-state-accessibility-geometry-and-png"
      {
         bail!("{side} release-candidate completion receipt is invalid");
      }
   }
   let promotion = promote_release_candidates(&config.spec_root, &config.output_root, &config.promoted_spec_root, 3)?;
   Ok(MacOsReleaseCandidateCaptureReport {
      schema_version: 1,
      build_manifest_sha256,
      capture_plan_sha256,
      captured_scenario_ids: capture_plan.candidates.iter().map(|candidate| candidate.id.clone()).collect(),
      captured_checkpoint_count: 18,
      promotion,
   })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MacOsRapidComplete
{
   schema_version: u32,
   side: ComparisonSide,
   mode: MacOsRapidMode,
   #[serde(rename = "scenarioID")]
   scenario_id: String,
   #[serde(rename = "checkpointID")]
   checkpoint_id: String,
   iterations: u32,
   evidence: ArtifactIdentity,
   validation: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MacOsRapidEvidence
{
   schema_version: u32,
   side: ComparisonSide,
   mode: MacOsRapidMode,
   #[serde(rename = "scenarioID")]
   scenario_id: String,
   #[serde(rename = "checkpointID")]
   checkpoint_id: String,
   actual_state: ArtifactIdentity,
   actual_geometry: Option<ArtifactIdentity>,
   actual_screenshot: Option<ArtifactIdentity>,
   visible_role_counts: Vec<RoleCount>,
   samples_ns: Vec<u64>,
   output_ready_boundary: Option<String>,
   validation: String,
}

pub fn run_macos_rapid(config: &MacOsRapidRunConfig) -> Result<MacOsRapidRunReport>
{
   validate_macos_gui_session()?;
   let _wake_assertion = MacOsWakeAssertion::acquire()?;
   validate_rapid_path_component(&config.scenario_id, "scenario")?;
   validate_rapid_path_component(&config.checkpoint_id, "checkpoint")?;
   if !(1..=100).contains(&config.iterations) || config.mode == MacOsRapidMode::Visual && config.iterations != 1
   {
      bail!("rapid macOS iteration count is invalid for {:?}", config.mode);
   }
   let plan_path = config.spec_root.join("plans/apple-pr.json");
   let build_bytes = fs::read(&config.build_manifest_path).with_context(|| format!("reading {}", config.build_manifest_path.display()))?;
   let build: BuildManifest = serde_json::from_slice(&build_bytes).with_context(|| format!("decoding {}", config.build_manifest_path.display()))?;
   let (oxide, native) = validate_build_manifest(&build)?;
   validate_build_input_identity(&build, &plan_path)?;
   let scenario_path = config.spec_root.join("scenarios").join(format!("{}.json", config.scenario_id));
   let scenario_bytes = fs::read(&scenario_path).with_context(|| format!("reading {}", scenario_path.display()))?;
   let scenario: ScenarioSpec = serde_json::from_slice(&scenario_bytes).with_context(|| format!("decoding {}", scenario_path.display()))?;
   validate_scenario(&scenario)?;
   if scenario.id != config.scenario_id
      || !scenario.parity_checkpoints.iter().any(|checkpoint| checkpoint.id == config.checkpoint_id)
   {
      bail!("rapid macOS scenario/checkpoint selection does not exist");
   }
   if config.output_root.exists()
   {
      if !config.output_root.is_dir() || fs::read_dir(&config.output_root)?.next().is_some()
      {
         bail!("rapid macOS output must be a new or empty directory: {}", config.output_root.display());
      }
   }
   else
   {
      fs::create_dir_all(&config.output_root).with_context(|| format!("creating {}", config.output_root.display()))?;
   }
   let selected: Vec<(ComparisonSide, &VerifiedProduct)> = match config.side
   {
      MacOsRapidSide::Oxide => vec![(ComparisonSide::Oxide, &oxide)],
      MacOsRapidSide::Native => vec![(ComparisonSide::Native, &native)],
      MacOsRapidSide::Both => vec![(ComparisonSide::Native, &native), (ComparisonSide::Oxide, &oxide)],
   };
   let mut sides = Vec::with_capacity(selected.len());
   for (side, product) in selected
   {
      validate_verified_product(product, &product.implementation_id)?;
      if exact_process_id(&product.executable)?.is_some()
      {
         bail!("rapid macOS comparator is already running: {}", product.executable.display());
      }
      let side_name = side.as_str();
      let stdout = File::create(config.output_root.join(format!("{side_name}.stdout.log")))?;
      let stderr = File::create(config.output_root.join(format!("{side_name}.stderr.log")))?;
      let mut command = Command::new("/usr/bin/open");
      command.args(["-n", "-W"]).arg(&product.bundle).arg("--args")
         .arg("-oxide-compare-targeted")
         .arg("-oxide-compare-targeted-scenario").arg(&config.scenario_id)
         .arg("-oxide-compare-targeted-checkpoint").arg(&config.checkpoint_id)
         .arg("-oxide-compare-targeted-mode").arg(config.mode.as_str())
         .arg("-oxide-compare-targeted-iterations").arg(config.iterations.to_string())
         .arg("-oxide-compare-output-root").arg(&config.output_root);
      let status = run_child(command, stdout, stderr, 30).with_context(|| format!("running rapid macOS {side_name} {} cell", config.mode.as_str()))?;
      if !status.success()
      {
         bail!("rapid macOS {side_name} comparator exited with {status}");
      }
      if exact_process_id(&product.executable)?.is_some()
      {
         bail!("rapid macOS {side_name} comparator remained running after its cell");
      }
      let complete_path = config.output_root.join(format!("{side_name}.{}.complete.json", config.mode.as_str()));
      let complete = read_json::<MacOsRapidComplete>(&complete_path)?;
      if complete.schema_version != 1
         || complete.side != side
         || complete.mode != config.mode
         || complete.scenario_id != config.scenario_id
         || complete.checkpoint_id != config.checkpoint_id
         || complete.iterations != config.iterations
         || complete.validation != format!("complete-targeted-{}-v1", config.mode.as_str())
      {
         bail!("rapid macOS {side_name} completion receipt differs from the requested cell");
      }
      let evidence_bytes = read_rapid_artifact(&config.output_root, &complete.evidence)?;
      let evidence: MacOsRapidEvidence = serde_json::from_slice(&evidence_bytes).with_context(|| format!("decoding rapid macOS {side_name} evidence"))?;
      validate_macos_rapid_evidence(config, side, &scenario, &evidence)?;
      sides.push(MacOsRapidSideResult {
         side,
         evidence_path: complete.evidence.path,
         evidence_sha256: sha256(&evidence_bytes),
         samples_ns: evidence.samples_ns,
      });
   }
   let visual = if config.mode == MacOsRapidMode::Visual && config.side == MacOsRapidSide::Both
   {
      Some(compare_apple_rapid_visual_checkpoint(
         &config.output_root.join("oxide.evidence"),
         &config.output_root.join("native.evidence"),
         &config.spec_root,
         &config.scenario_id,
         &config.checkpoint_id,
         &config.output_root.join("visual.report.json"),
      )?)
   }
   else
   {
      None
   };
   let accepted = match config.mode
   {
      MacOsRapidMode::Visual => visual.as_ref().is_some_and(|report| report.accepted),
      MacOsRapidMode::Timing => sides.len() == if config.side == MacOsRapidSide::Both {2} else {1},
   };
   let report = MacOsRapidRunReport {
      schema_version: 1,
      mode: config.mode,
      scenario_id: config.scenario_id.clone(),
      checkpoint_id: config.checkpoint_id.clone(),
      iterations: config.iterations,
      sides,
      visual,
      accepted,
   };
   durable_json(&report, &config.output_root.join("rapid.report.json"))?;
   Ok(report)
}

fn validate_macos_rapid_evidence(config: &MacOsRapidRunConfig, side: ComparisonSide, scenario: &ScenarioSpec, evidence: &MacOsRapidEvidence) -> Result<()>
{
   let checkpoint = scenario.parity_checkpoints.iter().find(|checkpoint| checkpoint.id == config.checkpoint_id)
      .context("validated rapid scenario lost its selected checkpoint")?;
   let expected_state = fs::read(config.spec_root.join(&checkpoint.state.path)).with_context(|| format!("reading rapid expected state {}", checkpoint.state.path))?;
   if sha256(&expected_state) != checkpoint.state.sha256
   {
      bail!("rapid expected state hash differs from its scenario identity");
   }
   let state = read_rapid_artifact(&config.output_root, &evidence.actual_state)?;
   let state_value: serde_json::Value = serde_json::from_slice(&state).context("decoding rapid actual state")?;
   let expected_value: serde_json::Value = serde_json::from_slice(&expected_state).context("decoding rapid expected state")?;
   if evidence.schema_version != 1
      || evidence.side != side
      || evidence.mode != config.mode
      || evidence.scenario_id != config.scenario_id
      || evidence.checkpoint_id != config.checkpoint_id
      || state_value != expected_value
      || evidence.visible_role_counts != checkpoint.expected_visible_role_counts
   {
      bail!("rapid macOS evidence identity, state, or role counts differ from the requested cell");
   }
   match config.mode
   {
      MacOsRapidMode::Visual if !evidence.samples_ns.is_empty()
         || evidence.actual_geometry.is_none()
         || evidence.actual_screenshot.is_none()
         || evidence.output_ready_boundary.is_some()
         || evidence.validation != "exact-canonical-state-role-counts-geometry-and-png" =>
      {
         bail!("rapid macOS visual evidence has timing fields or missing raster fields");
      }
      MacOsRapidMode::Timing if evidence.samples_ns.len() != config.iterations as usize
         || evidence.samples_ns.contains(&0)
         || evidence.actual_geometry.is_some()
         || evidence.actual_screenshot.is_some()
         || evidence.output_ready_boundary.as_deref() != Some("benchmark-mutation-dispatch-to-app-output-ready")
         || evidence.validation != "exact-canonical-state-and-role-counts-after-every-sample" =>
      {
         bail!("rapid macOS timing evidence has an invalid output-ready contract");
      }
      _ => (),
   }
   Ok(())
}

fn read_rapid_artifact(root: &Path, identity: &ArtifactIdentity) -> Result<Vec<u8>>
{
   validate_sha256(&identity.sha256)?;
   let path = Path::new(&identity.path);
   if path.as_os_str().is_empty() || path.is_absolute() || path.components().any(|component| !matches!(component, Component::Normal(_)))
   {
      bail!("rapid macOS artifact path is not normalized and relative: {}", path.display());
   }
   let full_path = root.join(path);
   let bytes = fs::read(&full_path).with_context(|| format!("reading {}", full_path.display()))?;
   if sha256(&bytes) != identity.sha256
   {
      bail!("rapid macOS artifact hash mismatch: {}", identity.path);
   }
   Ok(bytes)
}

fn validate_rapid_path_component(value: &str, kind: &str) -> Result<()>
{
   let path = Path::new(value);
   if path.components().count() != 1 || !matches!(path.components().next(), Some(Component::Normal(_)))
   {
      bail!("rapid macOS {} is not a single safe path component: {}", kind, value);
   }
   Ok(())
}

pub fn run_macos_campaign(config: &MacOsCampaignConfig) -> Result<MacOsCampaignReport>
{
   validate_run_id(&config.run_id)?;
   if config.scope != MacOsCampaignScope::CorrectnessOnly && !config.require_live_gui_session
   {
      bail!("measured macOS acquisition cannot disable live GUI-session validation");
   }
   let instrumentation_calibration = admit_macos_instrumentation_calibration(config)?;
   let _wake_assertion = if config.require_live_gui_session
   {
      validate_macos_gui_session()?;
      Some(MacOsWakeAssertion::acquire()?)
   }
   else {None};
   if config.scope == MacOsCampaignScope::Full
   {
      if !config.capture_presentation_traces || config.xctrace_template != "Animation Hitches"
      {
         bail!("full macOS acquisition requires the exact Animation Hitches minimal-presentation template");
      }
   }
   else if config.scope == MacOsCampaignScope::QualificationOnly && config.capture_presentation_traces
   {
      bail!("macOS comparator qualification must run standalone Time Profiler without concurrent presentation tracing");
   }
   let plan_bytes = fs::read(&config.plan_path).with_context(|| format!("reading {}", config.plan_path.display()))?;
   let plan_sha256 = sha256(&plan_bytes);
   let build_bytes = fs::read(&config.build_manifest_path).with_context(|| format!("reading {}", config.build_manifest_path.display()))?;
   let build_manifest_sha256 = sha256(&build_bytes);
   let build: BuildManifest = serde_json::from_slice(&build_bytes).with_context(|| format!("decoding {}", config.build_manifest_path.display()))?;
   let executables = validate_build_manifest(&build)?;
   validate_build_input_identity(&build, &config.plan_path)?;
   let campaign = load_macos_campaign_plan(config, &plan_bytes, &plan_sha256)?;
   validate_macos_recapture_admission(config, &campaign)?;
   validate_generic_macos_execution_support(&campaign, config.scope)?;
   if config.scope == MacOsCampaignScope::Full
   {
      if campaign.plan_resource_path.is_some()
      {
         validate_macos_generic_appkit_comparator_admission(&config.plan_path)?;
      }
      else
      {
         validate_macos_appkit_comparator_admission(&config.plan_path)?;
      }
   }
   validate_bundled_campaign_plan(&executables, &campaign)?;
   fs::create_dir_all(&config.output_root).with_context(|| format!("creating {}", config.output_root.display()))?;
   durable_json(&campaign, &config.output_root.join("campaign.plan.json"))?;
   let environment_before = if config.scope == MacOsCampaignScope::Full {Some(collect_macos_environment_snapshot()?)} else {None};

   let sessions = macos_campaign_sessions_for_scope(&campaign, config.scope);
   if sessions.iter().any(|session| session.pass_role == AppleCampaignPassRole::Energy)
   {
      if !config.output_root.is_absolute()
      {
         bail!("macOS energy acquisition requires an absolute output root for the external-meter protocol");
      }
      if let Some(path) = config.energy_meter_config_path.as_ref()
      {
         load_macos_external_meter_config(path)?;
      }
      else
      {
         let unavailable = macos_energy_unavailable_artifact("the release energy pass requires --energy-meter-config with a calibrated direct external meter")?;
         durable_json(&unavailable, &config.output_root.join("energy.unavailable.json"))?;
         bail!("macOS energy acquisition is unavailable without a configured calibrated direct external meter");
      }
   }
   let mut results = Vec::with_capacity(sessions.len());
   let expected_correctness_pair_count = campaign.sessions.iter().filter(|session| session.pass_id == "correctness").count() / 2;
   let mut correctness_pairs = Vec::with_capacity(expected_correctness_pair_count);
   let mut pre_pair_environment = Vec::with_capacity(sessions.len() / 2);
   let mut predecessor_pair_sha256 = None;
   let mut pairs = sessions.chunks_exact(2);
   for pair in &mut pairs
   {
      let first = pair[0];
      let second = pair[1];
      validate_macos_pair_shape(first, second)?;
      if first.pass_id != "correctness" && !correctness_gate_accepted(&correctness_pairs, expected_correctness_pair_count)
      {
         let report = macos_campaign_report(
            config,
            &plan_sha256,
            &build_manifest_sha256,
            None,
            results,
            correctness_pairs,
            expected_correctness_pair_count,
            false,
         );
         durable_json(&report, &config.output_root.join("campaign.complete.json"))?;
         bail!("macOS measured acquisition is blocked because the complete calibrated-static correctness matrix was not accepted");
      }
      if config.scope == MacOsCampaignScope::Full
      {
         pre_pair_environment.push(collect_macos_pre_pair_environment(first, second)?);
      }
      let (mut pair_results, checkpoint_sha256, correctness) = run_or_resume_pair(
         config,
         &campaign,
         [first, second],
         &executables,
         &build_manifest_sha256,
         predecessor_pair_sha256.as_deref(),
      )?;
      results.append(&mut pair_results);
      if let Some(correctness) = correctness
      {
         correctness_pairs.push(correctness);
      }
      predecessor_pair_sha256 = Some(checkpoint_sha256);
   }
   if !pairs.remainder().is_empty()
   {
      bail!("macOS campaign session scope does not end at a complete pair boundary");
   }
   let acquisition_validity = if let Some(before) = environment_before
   {
      let current_calibration = admit_macos_instrumentation_calibration(config)?.context("full macOS acquisition lost its instrumentation calibration")?;
      if instrumentation_calibration.as_ref() != Some(&current_calibration)
      {
         bail!("macOS instrumentation calibration identity changed during acquisition");
      }
      let validity = reduce_macos_acquisition_validity_with_pre_pair_environment(&config.output_root, &campaign, &build_manifest_sha256, before, pre_pair_environment, collect_macos_environment_snapshot()?, &results, Some(&current_calibration))?;
      let path = config.output_root.join("acquisition.validity.json");
      durable_json(&validity, &path)?;
      let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
      let stored: MacOsAcquisitionValidityReport = serde_json::from_slice(&bytes).with_context(|| format!("decoding {}", path.display()))?;
      validate_macos_acquisition_validity(&stored)?;
      Some((ArtifactIdentity {path: path.to_string_lossy().into_owned(), sha256: sha256(&bytes)}, validity.authoritative_eligible))
   }
   else {None};
   let report = macos_campaign_report(
      config,
      &plan_sha256,
      &build_manifest_sha256,
      acquisition_validity,
      results,
      correctness_pairs,
      expected_correctness_pair_count,
      true,
   );
   durable_json(&report, &config.output_root.join("campaign.complete.json"))?;
   Ok(report)
}

pub fn run_macos_density_sessions(config: &MacOsDensitySessionConfig) -> Result<Vec<MacOsDensitySessionResult>>
{
   validate_run_id(&config.run_id)?;
   if !config.output_root.is_absolute()
   {
      bail!("macOS density sessions require an absolute output root");
   }
   if config.sessions.is_empty()
   {
      bail!("macOS density session request is empty");
   }
   validate_macos_gui_session()?;
   let _wake_assertion = MacOsWakeAssertion::acquire()?;
   let plan_bytes = fs::read(&config.plan_path).with_context(|| format!("reading {}", config.plan_path.display()))?;
   let plan_sha256 = sha256(&plan_bytes);
   let build_bytes = fs::read(&config.build_manifest_path).with_context(|| format!("reading {}", config.build_manifest_path.display()))?;
   let build: BuildManifest = serde_json::from_slice(&build_bytes).with_context(|| format!("decoding {}", config.build_manifest_path.display()))?;
   let products = validate_build_manifest(&build)?;
   validate_build_input_identity(&build, &config.plan_path)?;
   let campaign_config = MacOsCampaignConfig {
      build_manifest_path: config.build_manifest_path.clone(),
      plan_path: config.plan_path.clone(),
      acquisition_path: config.plan_path.clone(),
      output_root: config.output_root.clone(),
      run_id: config.run_id.clone(),
      capture_presentation_traces: false,
      xctrace_template: String::new(),
      scope: MacOsCampaignScope::Full,
      resume: false,
      require_live_gui_session: true,
      energy_meter_config_path: None,
      instrumentation_calibration_path: None,
   };
   let campaign = load_macos_campaign_plan(&campaign_config, &plan_bytes, &plan_sha256)?;
   if campaign.plan_resource_path.is_none()
   {
      bail!("macOS density sessions require a generic content-addressed campaign plan");
   }
   validate_macos_recapture_admission(&campaign_config, &campaign)?;
   validate_macos_generic_appkit_comparator_admission(&config.plan_path)?;
   validate_bundled_campaign_plan(&products, &campaign)?;
   let mut identities = BTreeSet::new();
   let mut results = Vec::with_capacity(config.sessions.len());
   for requested in &config.sessions
   {
      validate_run_id(&requested.session_id)?;
      if !identities.insert((requested.session_id.as_str(), requested.pair_index, requested.side))
      {
         bail!("macOS density session identity is duplicated");
      }
      let template = campaign.sessions.iter().find(|session| {
         session.pass_id == requested.pass_id && session.pack_id == requested.pack_id && session.side == requested.side
      }).with_context(|| format!("macOS density session has no campaign template for pass {} pack {} {:?}", requested.pass_id, requested.pack_id, requested.side))?;
      if template.pass_role != AppleCampaignPassRole::Primary || template.collector.is_some()
      {
         bail!("macOS density sessions require an unprofiled primary comparison-controller pass");
      }
      let mut session = template.clone();
      session.chunk_id = requested.session_id.clone();
      session.pair_index = requested.pair_index;
      let product = match requested.side
      {
         ComparisonSide::Oxide => &products.0,
         ComparisonSide::Native => &products.1,
      };
      let campaign_result = run_or_resume_session(&campaign_config, &campaign, &session, product)?;
      let paths = session_paths(&config.output_root, &campaign, &session);
      let telemetry_bytes = fs::read(&paths.telemetry).with_context(|| format!("reading macOS density telemetry {}", paths.telemetry.display()))?;
      results.push(MacOsDensitySessionResult {
         session_id: requested.session_id.clone(),
         campaign: campaign_result,
         telemetry_path: paths.telemetry.to_string_lossy().into_owned(),
         telemetry_sha256: sha256(&telemetry_bytes),
      });
   }
   Ok(results)
}

fn admit_macos_instrumentation_calibration(config: &MacOsCampaignConfig) -> Result<Option<ArtifactIdentity>>
{
   if config.scope != MacOsCampaignScope::Full
   {
      return Ok(None);
   }
   let input_path = config.instrumentation_calibration_path.as_ref().context("full macOS acquisition requires --instrumentation-calibration with canonical trace-on/off input")?;
   let input_bytes = fs::read(input_path).with_context(|| format!("reading macOS instrumentation calibration {}", input_path.display()))?;
   let input: InstrumentationCalibrationInput = serde_json::from_slice(&input_bytes).with_context(|| format!("decoding macOS instrumentation calibration {}", input_path.display()))?;
   if canonical_instrumentation_calibration_input_json(&input)? != input_bytes
   {
      bail!("macOS instrumentation calibration input is not canonical JSON");
   }
   let report = reduce_instrumentation_calibration(&input)?;
   if report.platform_role != "macos-apple-silicon" || report.template_id != config.xctrace_template || !report.accepted
   {
      bail!("macOS instrumentation calibration is rejected or differs from the active platform/template");
   }
   let path = config.output_root.join("instrumentation.calibration.json");
   let mut expected = serde_json::to_vec_pretty(&report).context("encoding macOS instrumentation calibration report")?;
   expected.push(b'\n');
   if path.exists()
   {
      let stored = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
      if stored != expected
      {
         bail!("existing macOS instrumentation calibration report differs from the independently reduced input");
      }
   }
   else
   {
      durable_json(&report, &path)?;
   }
   let stored = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
   let persisted: InstrumentationCalibrationReport = serde_json::from_slice(&stored).context("decoding persisted macOS instrumentation calibration report")?;
   if persisted != report || stored != expected
   {
      bail!("persisted macOS instrumentation calibration report did not round-trip exactly");
   }
   Ok(Some(ArtifactIdentity {path: String::from("instrumentation.calibration.json"), sha256: sha256(&stored)}))
}

pub fn validate_generic_macos_execution_support(plan: &MacOsCampaignPlan, scope: MacOsCampaignScope) -> Result<()>
{
   if plan.plan_resource_path.is_none()
   {
      return Ok(());
   }
   let mut unsupported = Vec::new();
   for session in macos_campaign_sessions_for_scope(plan, scope)
   {
      if matches!(session.pass_role, AppleCampaignPassRole::Primary | AppleCampaignPassRole::Attribution | AppleCampaignPassRole::Idle | AppleCampaignPassRole::Endurance | AppleCampaignPassRole::Energy) && session.timing.is_none()
      {
         push_unique(&mut unsupported, "generic primary, attribution, idle, endurance, and energy sessions require their typed tier timing overlay");
      }
      match session.pass_role
      {
         AppleCampaignPassRole::Primary => (),
         AppleCampaignPassRole::Attribution =>
         {
            match session.collector.as_deref()
            {
               Some("physical-footprint") => (),
               Some("time-profiler") => (),
               Some("system-trace") => (),
               Some("common-gpu") if session.evidence_role == AppleCampaignEvidenceRole::ClaimBearing => push_unique(&mut unsupported, "claim-bearing common-gpu is unavailable because exact-process task GPU time does not symmetrically own AppKit compositor work; no winner may be inferred"),
               Some("common-gpu") => (),
               Some(collector) => unsupported.push(format!("the macOS generic controller does not support collector {}", collector)),
               None => push_unique(&mut unsupported, "an attribution session lost its required collector identity"),
            }
         }
         AppleCampaignPassRole::Launch => match session.launch_class.as_deref()
         {
            Some("terminated-warm-system-cache" | "fresh-install-first-launch" | "warm-resume") => (),
            Some(launch_class) => unsupported.push(format!("the macOS generic controller does not support launch class {}", launch_class)),
            None => push_unique(&mut unsupported, "a launch session lost its required launch-class identity"),
         },
         AppleCampaignPassRole::Energy => (),
         AppleCampaignPassRole::Correctness | AppleCampaignPassRole::Idle | AppleCampaignPassRole::Endurance => (),
      }
   }
   if !unsupported.is_empty()
   {
      bail!("generic macOS acquisition is not executable without silently violating its plan: {}", unsupported.join("; "));
   }
   Ok(())
}

pub fn validate_macos_recapture_admission(config: &MacOsCampaignConfig, campaign: &MacOsCampaignPlan) -> Result<()>
{
   if config.scope == MacOsCampaignScope::CorrectnessOnly
   {
      return Ok(());
   }
   let spec_root = config.plan_path.parent().and_then(Path::parent).context("macOS recapture-admission plan is not under a specification plans directory")?;
   let bindings = if campaign.plan_resource_path.is_some()
   {
      let bytes = fs::read(&config.plan_path).with_context(|| format!("reading recapture-admission plan {}", config.plan_path.display()))?;
      let plan: AppleCampaignPlanSpec = serde_json::from_slice(&bytes).with_context(|| format!("decoding recapture-admission plan {}", config.plan_path.display()))?;
      plan.scenarios.into_iter().map(|binding| {
         let artifact = binding.artifact.with_context(|| format!("recapture-admission scenario {} has no artifact", binding.id))?;
         Ok((binding.id, artifact))
      }).collect::<Result<Vec<_>>>()?
   }
   else
   {
      let plan_bytes = fs::read(&config.plan_path).with_context(|| format!("reading recapture-admission Apple PR plan {}", config.plan_path.display()))?;
      let plan: ApplePrPlanSpec = serde_json::from_slice(&plan_bytes).with_context(|| format!("decoding recapture-admission Apple PR plan {}", config.plan_path.display()))?;
      let acquisition_bytes = fs::read(&config.acquisition_path).with_context(|| format!("reading recapture-admission acquisition {}", config.acquisition_path.display()))?;
      let acquisition: ApplePrAcquisitionSpec = serde_json::from_slice(&acquisition_bytes).with_context(|| format!("decoding recapture-admission acquisition {}", config.acquisition_path.display()))?;
      let selected_packs = macos_campaign_sessions_for_scope(campaign, config.scope).into_iter().map(|session| session.pack_id.as_str()).collect::<BTreeSet<_>>();
      let selected_ids = acquisition.packs.iter().filter(|pack| selected_packs.contains(pack.id.as_str())).flat_map(|pack| pack.ordered_scenario_ids.iter()).collect::<BTreeSet<_>>();
      selected_ids.into_iter().map(|scenario_id| {
         let binding = plan.scenarios.iter().find(|binding| binding.id == scenario_id.as_str()).with_context(|| format!("recapture-admission Apple PR plan does not bind scenario {scenario_id}"))?;
         Ok((scenario_id.clone(), binding.artifact.clone()))
      }).collect::<Result<Vec<_>>>()?
   };
   for (scenario_id, artifact) in bindings
   {
      ensure_safe_recapture_artifact_path(&artifact.path)?;
      let bytes = fs::read(spec_root.join(&artifact.path)).with_context(|| format!("reading recapture-admission scenario {scenario_id}"))?;
      if sha256(&bytes) != artifact.sha256
      {
         bail!("recapture-admission scenario {} differs from its plan binding", scenario_id);
      }
      let scenario: ScenarioSpec = serde_json::from_slice(&bytes).with_context(|| format!("decoding recapture-admission scenario {scenario_id}"))?;
      validate_scenario(&scenario).with_context(|| format!("validating recapture-admission scenario {scenario_id}"))?;
      if scenario.id != scenario_id
      {
         bail!("recapture-admission scenario binding differs from its manifest: {}", scenario_id);
      }
      for checkpoint in &scenario.parity_checkpoints
      {
         if let Some(status) = &checkpoint.recapture_status
         {
            bail!("macOS {:?} acquisition cannot launch while selected checkpoint {}/{} has pending recapture status {}", config.scope, scenario.id, checkpoint.id, status);
         }
      }
   }
   Ok(())
}

fn ensure_safe_recapture_artifact_path(path: &str) -> Result<()>
{
   if path.is_empty() || !Path::new(path).components().all(|component| matches!(component, Component::Normal(_)))
   {
      bail!("recapture-admission scenario path is unsafe: {}", path);
   }
   Ok(())
}

fn push_unique(messages: &mut Vec<String>, message: &str)
{
   if !messages.iter().any(|existing| existing == message)
   {
      messages.push(String::from(message));
   }
}

fn load_macos_campaign_plan(config: &MacOsCampaignConfig, plan_bytes: &[u8], plan_sha256: &str) -> Result<MacOsCampaignPlan>
{
   #[derive(Deserialize)]
   struct PlanHeader
   {
      tier: String,
   }

   let header: PlanHeader = serde_json::from_slice(plan_bytes).with_context(|| format!("decoding {} identity", config.plan_path.display()))?;
   if header.tier == "pr"
   {
      let acquisition_bytes = fs::read(&config.acquisition_path).with_context(|| format!("reading {}", config.acquisition_path.display()))?;
      let acquisition: ApplePrAcquisitionSpec = serde_json::from_slice(&acquisition_bytes).with_context(|| format!("decoding {}", config.acquisition_path.display()))?;
      return build_macos_campaign_plan(&acquisition, &config.run_id, plan_sha256);
   }
   let plan: AppleCampaignPlanSpec = serde_json::from_slice(plan_bytes).with_context(|| format!("decoding generic macOS plan {}", config.plan_path.display()))?;
   let canonical = canonical_apple_campaign_plan_json(&plan)?;
   if canonical != plan_bytes
   {
      bail!("generic macOS campaign plan is not canonical benchmark-spec JSON");
   }
   let spec_root = config.plan_path.parent().and_then(Path::parent).context("generic macOS plan must be under a specification plans directory")?;
   let resource_path = config.plan_path.strip_prefix(spec_root).context("generic macOS plan is outside its specification root")?;
   let resource_path = resource_path.to_str().context("generic macOS plan resource path is not UTF-8")?;
   if config.scope == MacOsCampaignScope::QualificationOnly
   {
      if plan.id != "macos-comparator-qualification" || plan.tier != Tier::Extended
      {
         bail!("macOS qualification-only acquisition requires the promoted comparator qualification execution plan");
      }
      let component = |target| plan.budget_components.iter().find(|component| component.component == target).map(|component| component.occupied_seconds).unwrap_or(0);
      let attribution_seconds = component(AppleCampaignBudgetComponent::Attribution);
      let pre_reserve_seconds = component(AppleCampaignBudgetComponent::CorrectnessInstallPulls)
         .checked_add(component(AppleCampaignBudgetComponent::PrimaryDynamicPresentation)).context("qualification budget overflow")?
         .checked_add(component(AppleCampaignBudgetComponent::LaunchOrStartupDelivery)).context("qualification budget overflow")?
         .checked_add(component(AppleCampaignBudgetComponent::IdleEndurance)).context("qualification budget overflow")?
         .checked_add(component(AppleCampaignBudgetComponent::Energy)).context("qualification budget overflow")?
         .checked_add(attribution_seconds).context("qualification budget overflow")?;
      let reserve_seconds = pre_reserve_seconds.checked_add(4).context("qualification reserve overflow")? / 5;
      let hard_total_seconds = pre_reserve_seconds.checked_add(reserve_seconds).context("qualification total overflow")?;
      let budget = BudgetSpec {
         schema_version: 1,
         id: plan.id.clone(),
         platform: Platform::Apple,
         tier: Tier::Extended,
         correctness_install_pulls_seconds: component(AppleCampaignBudgetComponent::CorrectnessInstallPulls),
         primary_dynamic_presentation_seconds: component(AppleCampaignBudgetComponent::PrimaryDynamicPresentation),
         launch_or_startup_delivery_seconds: component(AppleCampaignBudgetComponent::LaunchOrStartupDelivery),
         idle_endurance_seconds: component(AppleCampaignBudgetComponent::IdleEndurance),
         energy_seconds: component(AppleCampaignBudgetComponent::Energy),
         attribution_seconds,
         pre_reserve_seconds,
         reserve_seconds,
         hard_total_seconds,
         campaign_critical_wall_seconds: hard_total_seconds,
         campaign_aggregate_seconds: hard_total_seconds,
      };
      let qualification_path = spec_root.join("qualification/macos-comparator-qualification.json");
      let qualification_bytes = fs::read(&qualification_path).with_context(|| format!("reading macOS comparator qualification plan {}", qualification_path.display()))?;
      let qualification: MacOsComparatorQualificationPlan = serde_json::from_slice(&qualification_bytes).context("decoding macOS comparator qualification plan")?;
      if canonical_macos_comparator_qualification_plan_json(&qualification)? != qualification_bytes
      {
         bail!("macOS comparator qualification plan is not canonical JSON");
      }
      let retained = plan.scenarios.iter().map(|scenario| scenario.id.clone()).collect::<Vec<_>>();
      validate_macos_comparator_qualification_plan(spec_root, &qualification, &retained)?;
      let mut campaign = build_generic_macos_campaign_plan(&plan, &budget, spec_root, resource_path, &config.run_id, plan_sha256)?;
      for session in &mut campaign.sessions
      {
         let scale = match session.pair_index
         {
            0 => MacOsComparatorScale::OneX,
            1 => MacOsComparatorScale::TwoX,
            other => bail!("macOS comparator qualification has unexpected scale pair index {}", other),
         };
         let variant = qualification.variants.iter().find(|variant| variant.scenario_id == session.pack_id && variant.scale == scale)
            .with_context(|| format!("macOS comparator qualification has no overlay for {} {:?}", session.pack_id, scale))?;
         session.scale_overlay = Some(variant.overlay.clone());
      }
      if campaign.sessions.len() != retained.len() * 4
      {
         bail!("macOS comparator qualification expanded to {} sessions instead of {}", campaign.sessions.len(), retained.len() * 4);
      }
      return Ok(campaign);
   }
   let budget_path = spec_root.join("budgets").join(format!("{}.json", plan.budget_id));
   let budget_bytes = fs::read(&budget_path).with_context(|| format!("reading generic macOS budget {}", budget_path.display()))?;
   let budget: BudgetSpec = serde_json::from_slice(&budget_bytes).with_context(|| format!("decoding generic macOS budget {}", budget_path.display()))?;
   build_generic_macos_campaign_plan(&plan, &budget, spec_root, resource_path, &config.run_id, plan_sha256)
}

fn validate_bundled_campaign_plan(executables: &(VerifiedProduct, VerifiedProduct), plan: &MacOsCampaignPlan) -> Result<()>
{
   let Some(resource_path) = &plan.plan_resource_path else {return Ok(())};
   for product in [&executables.0, &executables.1]
   {
      let path = product.bundle.join("Contents/Resources/v1").join(resource_path);
      let bytes = fs::read(&path).with_context(|| format!("reading bundled generic macOS plan {}", path.display()))?;
      if sha256(&bytes) != plan.plan_sha256
      {
         bail!("{} bundled generic macOS plan differs from the controller plan", product.implementation_id);
      }
   }
   Ok(())
}

fn validate_macos_gui_session() -> Result<()>
{
   let console = Command::new("/usr/sbin/ioreg").args(["-n", "Root", "-d1", "-a"]).output().context("querying macOS IOConsoleLocked")?;
   if !console.status.success()
   {
      bail!("macOS GUI session lock state is unavailable");
   }
   validate_macos_gui_lock_ioreg(&console.stdout)?;
   let script = "tell application \"System Events\" to get name of first application process whose frontmost is true";
   let output = Command::new("/usr/bin/osascript").args(["-e", script]).output().context("querying the macOS foreground GUI session")?;
   if !output.status.success()
   {
      let detail = String::from_utf8_lossy(&output.stderr);
      bail!("macOS GUI session is locked or unavailable; unlock the Mac before acquisition: {}", detail.trim());
   }
   let frontmost = String::from_utf8(output.stdout).context("macOS frontmost application name is not UTF-8")?;
   let frontmost = frontmost.trim();
   if frontmost.is_empty() || frontmost == "loginwindow"
   {
      bail!("macOS GUI session is locked or unavailable; frontmost process is `{}`", frontmost);
   }
   Ok(())
}

pub fn validate_macos_gui_lock_ioreg(bytes: &[u8]) -> Result<()>
{
   let value = std::str::from_utf8(bytes).context("macOS IOConsoleLocked response is not UTF-8")?;
   let key = value.find("<key>IOConsoleLocked</key>").context("macOS IOConsoleLocked property is unavailable")?;
   let state = &value[key..value.len().min(key + 128)];
   if state.contains("<true/>")
   {
      bail!("macOS GUI session is locked; unlock the Mac before acquisition");
   }
   if !state.contains("<false/>")
   {
      bail!("macOS IOConsoleLocked property has an unsupported value");
   }
   Ok(())
}

pub fn validate_macos_appkit_comparator_admission(plan_path: &Path) -> Result<()>
{
   let plan_bytes = fs::read(plan_path).with_context(|| format!("reading {}", plan_path.display()))?;
   let plan: ApplePrPlanSpec = serde_json::from_slice(&plan_bytes).with_context(|| format!("decoding {}", plan_path.display()))?;
   let identity = ComparatorIdentity {
      platform: String::from("macos"),
      framework: String::from("appkit"),
      implementation: String::from("appkit-production"),
      variant: String::from("native.production"),
   };
   let matching = plan.comparator_audits.iter().filter(|binding| binding.identity == identity).collect::<Vec<_>>();
   if matching.len() != 1
   {
      bail!("Apple PR plan must bind exactly one macOS AppKit native.production comparator audit, observed {}", matching.len());
   }
   let spec_root = plan_path.parent().and_then(Path::parent).context("Apple PR plan is not under a spec plans directory")?;
   let workspace_root = spec_root.ancestors().find(|ancestor| {
      ancestor.join("Cargo.toml").is_file() && ancestor.join("benchmarks/comparative/specs/v1").is_dir()
   }).context("Apple PR plan has no containing comparison workspace")?;
   let relative_spec_root = spec_root.strip_prefix(workspace_root).context("Apple PR spec root is outside its comparison workspace")?;
   let audit_path = relative_spec_root.join(&matching[0].audit.path);
   let audit = ArtifactIdentity {
      path: audit_path.to_string_lossy().into_owned(),
      sha256: matching[0].audit.sha256.clone(),
   };
   let expectation = ComparatorAdmissionExpectation {
      identity,
      retained_scenario_ids: plan.scenarios.iter().map(|scenario| scenario.id.clone()).collect(),
      primary_cell_ids: plan.scenarios.iter().map(|scenario| format!("macos.{}.primary", scenario.id)).collect(),
      required_source_paths: macos_appkit_required_source_paths(),
      required_dependency_paths: macos_appkit_required_dependency_paths(),
      required_build_recipe_path: Some(String::from("host/apple-comparison/project.yml")),
   };
   admit_comparator_acceptance(workspace_root, &audit, &expectation)?;
   Ok(())
}

fn validate_macos_generic_appkit_comparator_admission(plan_path: &Path) -> Result<()>
{
   let plan_bytes = fs::read(plan_path).with_context(|| format!("reading {}", plan_path.display()))?;
   let plan: AppleCampaignPlanSpec = serde_json::from_slice(&plan_bytes).with_context(|| format!("decoding {}", plan_path.display()))?;
   let spec_root = plan_path.parent().and_then(Path::parent).context("generic Apple plan is not under a spec plans directory")?;
   let workspace_root = spec_root.ancestors().find(|ancestor| {
      ancestor.join("Cargo.toml").is_file() && ancestor.join("benchmarks/comparative/specs/v1").is_dir()
   }).context("generic Apple plan has no containing comparison workspace")?;
   let relative_spec_root = spec_root.strip_prefix(workspace_root).context("generic Apple spec root is outside its comparison workspace")?;
   let audit_relative_path = Path::new("audits/macos-appkit-native-production.json");
   let audit_path = spec_root.join(audit_relative_path);
   let audit_bytes = fs::read(&audit_path).with_context(|| format!("reading {}", audit_path.display()))?;
   let identity = ComparatorIdentity {
      platform: String::from("macos"),
      framework: String::from("appkit"),
      implementation: String::from("appkit-production"),
      variant: String::from("native.production"),
   };
   let audit = ArtifactIdentity {
      path: relative_spec_root.join(audit_relative_path).to_string_lossy().into_owned(),
      sha256: sha256(&audit_bytes),
   };
   let expectation = ComparatorAdmissionExpectation {
      identity,
      retained_scenario_ids: plan.scenarios.iter().map(|scenario| scenario.id.clone()).collect(),
      primary_cell_ids: plan.scenarios.iter().map(|scenario| format!("macos.{}.primary", scenario.id)).collect(),
      required_source_paths: macos_appkit_required_source_paths(),
      required_dependency_paths: macos_appkit_required_dependency_paths(),
      required_build_recipe_path: Some(String::from("host/apple-comparison/project.yml")),
   };
   admit_comparator_acceptance(workspace_root, &audit, &expectation)?;
   Ok(())
}

fn macos_appkit_required_source_paths() -> Vec<String>
{
   [
      "host/apple-comparison/AppKit-macOS/AppKitProductionScenarioAdapter.swift",
      "host/apple-comparison/AppKit-macOS/AppKitProductionReferenceFoundation.swift",
      "host/apple-comparison/AppKit-macOS/AppKitProductionReferenceDataSources.swift",
      "host/apple-comparison/AppKit-macOS/AppKitProductionReferenceScenarioModel.swift",
      "host/apple-comparison/AppKit-macOS/AppKitProductionReferenceScenes.swift",
      "host/apple-comparison/AppKit-macOS/AppKitProductionReleaseScenes.swift",
      "host/apple-comparison/AppKit-macOS/AppKitProductionVisualComponents.swift",
      "host/apple-comparison/AppKit-macOS/AppKitProductionEdgeRaster.swift",
      "host/apple-comparison/AppKit-macOS/main.swift",
      "host/apple-comparison/AppKit-macOS/Info.plist",
      "host/apple-comparison/Shared/Foundation/BenchmarkCampaign.swift",
      "host/apple-comparison/Shared/ComparatorApp/BenchmarkCampaignExecutor.swift",
      "host/apple-comparison/Shared/Foundation/BenchmarkContract.swift",
      "host/apple-comparison/Shared/Foundation/BenchmarkPhaseScheduler.swift",
      "host/apple-comparison/Shared/Foundation/BenchmarkTelemetryRing.swift",
      "host/apple-comparison/Shared/Foundation/ComparisonContract.swift",
      "host/apple-comparison/Shared/Foundation/ComparisonTransport.swift",
      "host/apple-comparison/Shared/iOSControl/DarwinControlSignal.swift",
      "host/apple-comparison/Shared/Foundation/DurableArtifactStore.swift",
      "host/apple-comparison/Shared/ComparatorApp/MacOSCampaignControl.swift",
      "host/apple-comparison/Shared/ComparatorApp/MacOSCanonicalLaunchExecutor.swift",
      "host/apple-comparison/Shared/ComparatorApp/MacOSTrustedInputControl.swift",
   ].into_iter().map(String::from).collect()
}

fn macos_appkit_required_dependency_paths() -> Vec<String>
{
   [
      "host/apple-comparison/AppleComparison.xcodeproj/project.pbxproj",
      "host/apple-comparison/MacOSComparisonControllerUITests/Info.plist",
      "host/apple-comparison/MacOSComparisonControllerUITests/MacOSComparisonControllerUITests.swift",
      "host/apple-comparison/MacOSComparisonControllerUITests/MacOSTrustedInputXCUIController.swift",
   ].into_iter().map(String::from).collect()
}

fn macos_campaign_report(
   config: &MacOsCampaignConfig,
   plan_sha256: &str,
   build_manifest_sha256: &str,
   acquisition_validity: Option<(ArtifactIdentity, bool)>,
   sessions: Vec<MacOsCampaignSessionResult>,
   correctness_pairs: Vec<MacOsCorrectnessPairResult>,
   expected_correctness_pair_count: usize,
   acquisition_complete: bool,
) -> MacOsCampaignReport
{
   let correctness_accepted = correctness_gate_accepted(&correctness_pairs, expected_correctness_pair_count);
   let authoritative_eligible = config.scope == MacOsCampaignScope::Full && acquisition_complete && correctness_accepted && acquisition_validity.as_ref().is_some_and(|(_, eligible)| *eligible);
   MacOsCampaignReport {
      schema_version: 4,
      run_id: config.run_id.clone(),
      plan_sha256: String::from(plan_sha256),
      build_manifest_sha256: String::from(build_manifest_sha256),
      acquisition_validity: acquisition_validity.map(|(identity, _)| identity),
      sessions,
      correctness_pairs,
      scope: config.scope,
      acquisition_complete,
      correctness_accepted,
      correctness_eligible_for_measured_acquisition: correctness_accepted,
      authoritative_eligible,
      complete: acquisition_complete,
   }
}

fn correctness_gate_accepted(correctness_pairs: &[MacOsCorrectnessPairResult], expected_pair_count: usize) -> bool
{
   correctness_pairs.len() == expected_pair_count && correctness_pairs.iter().all(|pair| pair.accepted)
}

pub fn macos_session_arguments(plan: &MacOsCampaignPlan, session: &MacOsCampaignSession, output_root: &Path) -> Vec<String>
{
   let mut arguments = vec![
      String::from("-oxide-compare-plan-sha"), plan.plan_sha256.clone(),
      String::from("-oxide-compare-run-id"), plan.run_id.clone(),
      String::from("-oxide-compare-chunk"), session.chunk_id.clone(),
      String::from("-oxide-compare-pass"), session.pass_id.clone(),
      String::from("-oxide-compare-pack"), session.pack_id.clone(),
      String::from("-oxide-compare-pair"), session.pair_index.to_string(),
      String::from("-oxide-compare-side"), String::from(session.side.as_str()),
      String::from("-oxide-compare-generation"), macos_session_generation(plan, session),
      String::from("-oxide-compare-output-root"), output_root.to_string_lossy().into_owned(),
   ];
   if let Some(path) = &plan.plan_resource_path
   {
      arguments.extend([String::from("-oxide-compare-plan-path"), path.clone()]);
   }
   if let Some(overlay) = &session.scale_overlay
   {
      arguments.extend([
         String::from("-oxide-compare-scale-overlay-path"), overlay.path.clone(),
         String::from("-oxide-compare-scale-overlay-sha"), overlay.sha256.clone(),
      ]);
   }
   arguments.push(String::from("-oxide-compare-controlled-start"));
   arguments
}

pub fn macos_session_generation(plan: &MacOsCampaignPlan, session: &MacOsCampaignSession) -> String
{
   let identity = format!(
      "{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}",
      plan.plan_sha256,
      plan.run_id,
      session.chunk_id,
      session.pass_id,
      session.pack_id,
      session.pair_index,
      session.side.as_str(),
      session.scale_overlay.as_ref().map(|overlay| overlay.sha256.as_str()).unwrap_or("unscaled"),
   );
   sha256(identity.as_bytes())
}

pub fn macos_application_bundle(executable: &Path) -> Result<PathBuf>
{
   let macos = executable.parent().context("macOS comparison executable has no parent")?;
   if macos.file_name().and_then(|value| value.to_str()) != Some("MacOS")
   {
      bail!("macOS comparison executable is not inside Contents/MacOS: {}", executable.display());
   }
   let contents = macos.parent().context("macOS comparison executable has no Contents parent")?;
   if contents.file_name().and_then(|value| value.to_str()) != Some("Contents")
   {
      bail!("macOS comparison executable is not inside an application Contents directory: {}", executable.display());
   }
   let bundle = contents.parent().context("macOS comparison executable has no application bundle parent")?;
   if bundle.extension().and_then(|value| value.to_str()) != Some("app") || !bundle.is_dir()
   {
      bail!("macOS comparison executable has no existing .app bundle: {}", executable.display());
   }
   Ok(bundle.to_path_buf())
}

pub fn macos_process_id_from_ps(executable: &Path, output: &str) -> Result<Option<u32>>
{
   let executable = executable.to_string_lossy();
   let mut matched = None;
   for line in output.lines()
   {
      let line = line.trim_start();
      let Some(separator) = line.find(char::is_whitespace) else {continue};
      let (pid, command) = line.split_at(separator);
      let command = command.trim_start();
      if command != executable && !command.strip_prefix(executable.as_ref()).is_some_and(|suffix| suffix.chars().next().is_some_and(char::is_whitespace))
      {
         continue;
      }
      let pid = pid.parse::<u32>().with_context(|| format!("invalid process id in ps output: {}", pid))?;
      if matched.replace(pid).is_some()
      {
         bail!("multiple exact comparison processes are running for {}", executable);
      }
   }
   Ok(matched)
}

pub fn audit_comparison_source_boundary(workspace_root: &Path) -> Result<()>
{
   let required = [
      "host/apple-comparison/oxide-comparison-runtime/Cargo.toml",
      "host/apple-comparison/comparison-controller/Cargo.toml",
      "host/web-comparison/Cargo.toml",
   ];
   for relative in required
   {
      if !workspace_root.join(relative).is_file()
      {
         bail!("comparison harness crate is missing: {}", relative);
      }
   }

   let markers = ["-oxide-compare-plan-sha", "BenchmarkCampaignExecutor", "ComparisonControllerUITests"];
   audit_crates_excluding_harnesses(&workspace_root.join("crates"), &markers)?;
   for relative in ["host/ios-app", "host/macos-app", "host/web-app"]
   {
      audit_forbidden_markers(&workspace_root.join(relative), &markers)?;
   }
   Ok(())
}

pub fn validate_macos_trace_exports(toc: &str, signposts: &str) -> Result<()>
{
   let normalized_toc = toc.to_ascii_lowercase();
   if !normalized_toc.contains("os-signpost")
   {
      bail!("macOS presentation trace has no os-signpost schema");
   }
   for schema in ["hitches-updates", "hitches-frame-lifetimes"]
   {
      if !normalized_toc.contains(schema)
      {
         bail!("macOS presentation trace has no {} schema", schema);
      }
   }
   for marker in ["InputReceived", "VisualGeneration", "DisplayOpportunity"]
   {
      if !signposts.contains(marker)
      {
         bail!("macOS presentation trace is missing comparison marker {}", marker);
      }
   }
   Ok(())
}

fn audit_crates_excluding_harnesses(root: &Path, markers: &[&str]) -> Result<()>
{
   let allowed = ["benchmark-spec", "harness-registry", "perf-runner", "snapshot-runner", "test-scenes"];
   for entry in fs::read_dir(root).with_context(|| format!("reading {}", root.display()))?
   {
      let path = entry?.path();
      if !path.is_dir()
      {
         continue;
      }
      let name = path.file_name().and_then(|value| value.to_str()).context("crate directory has no UTF-8 name")?;
      if !allowed.contains(&name)
      {
         audit_forbidden_markers(&path, markers)?;
      }
   }
   Ok(())
}

fn append_chunk_sessions(acquisition: &ApplePrAcquisitionSpec, chunk: &AcquisitionChunkBudget, seed: u64, sessions: &mut Vec<MacOsCampaignSession>) -> Result<()>
{
   if chunk.pass_id == "correctness"
   {
      if !chunk.ordered_pair_indices.is_empty()
      {
         bail!("correctness chunk must not declare timed pair indices");
      }
      for (execution_index, pack_id) in chunk.pack_ids.iter().enumerate()
      {
         validate_pack(acquisition, pack_id)?;
         let pair_index = u32::try_from(execution_index).context("correctness execution index exceeds u32")?;
         let order = balanced_comparison_order(seed, execution_index + 1)[execution_index];
         let sides = if order == ComparisonOrder::Ab
         {
            [ComparisonSide::Native, ComparisonSide::Oxide]
         }
         else
         {
            [ComparisonSide::Oxide, ComparisonSide::Native]
         };
         for side in sides
         {
            sessions.push(MacOsCampaignSession {
               chunk_id: chunk.id.clone(),
               pass_id: chunk.pass_id.clone(),
               pack_id: pack_id.clone(),
               pass_role: AppleCampaignPassRole::Correctness,
               evidence_role: AppleCampaignEvidenceRole::CorrectnessOnly,
               collector: None,
               launch_class: None,
               timing: None,
               pair_index,
               order,
               side,
               max_occupied_seconds: chunk.max_occupied_seconds,
               scale_overlay: None,
            });
         }
      }
      return Ok(());
   }

   for pack_id in &chunk.pack_ids
   {
      let pack = validate_pack(acquisition, pack_id)?;
      for pair_index in &chunk.ordered_pair_indices
      {
         if *pair_index >= pack.pair_count
         {
            bail!("chunk {} pair {} exceeds pack {}", chunk.id, pair_index, pack_id);
         }
         let order = balanced_comparison_order(seed, *pair_index as usize + 1)[*pair_index as usize];
         let sides = if order == ComparisonOrder::Ab
         {
            [ComparisonSide::Native, ComparisonSide::Oxide]
         }
         else
         {
            [ComparisonSide::Oxide, ComparisonSide::Native]
         };
         for side in sides
         {
            let (pass_role, evidence_role, launch_class) = pr_session_contract(&chunk.pass_id);
            sessions.push(MacOsCampaignSession {
               chunk_id: chunk.id.clone(),
               pass_id: chunk.pass_id.clone(),
               pack_id: pack_id.clone(),
               pass_role,
               evidence_role,
               collector: None,
               launch_class,
               timing: None,
               pair_index: *pair_index,
               order,
               side,
               max_occupied_seconds: pack.side_seconds.checked_add(30).context("macOS session timeout overflow")?,
               scale_overlay: None,
            });
         }
      }
   }
   Ok(())
}

fn pr_session_contract(pass_id: &str) -> (AppleCampaignPassRole, AppleCampaignEvidenceRole, Option<String>)
{
   match pass_id
   {
      "correctness" => (AppleCampaignPassRole::Correctness, AppleCampaignEvidenceRole::CorrectnessOnly, None),
      "canonical-launch" => (AppleCampaignPassRole::Launch, AppleCampaignEvidenceRole::ClaimBearing, Some(String::from("terminated-warm-system-cache"))),
      _ => (AppleCampaignPassRole::Primary, AppleCampaignEvidenceRole::ClaimBearing, None),
   }
}

fn validate_pack<'a>(acquisition: &'a ApplePrAcquisitionSpec, pack_id: &str) -> Result<&'a oxide_benchmark_spec::AcquisitionPackBudget>
{
   acquisition.packs.iter().find(|pack| pack.id == pack_id).with_context(|| format!("unknown acquisition pack {}", pack_id))
}

fn validate_build_manifest(build: &BuildManifest) -> Result<(VerifiedProduct, VerifiedProduct)>
{
   if build.schema_version != 2 || build.platform != "macos" || build.configuration != "Release" || build.architecture != "arm64"
   {
      bail!("comparison build manifest is not a Release arm64 macOS build");
   }
   validate_git_object_id(&build.workspace_git_head)?;
   for identity in [
      &build.workspace_status_sha256,
      &build.benchmark_source_manifest_sha256,
      &build.specification_manifest_sha256,
      &build.build_command_sha256,
   ]
   {
      validate_sha256(identity)?;
   }
   if build.xcode_version.trim().is_empty() || build.swift_version.trim().is_empty() || build.rust_version.trim().is_empty()
   {
      bail!("comparison build manifest has an empty toolchain identity");
   }
   if build.products.len() != 2
   {
      bail!("comparison build manifest must contain exactly two products");
   }
   let launch_controller = verified_launch_controller(&build.controller)?;
   let oxide = verified_product(build, "oxide.production", "OxideMacComparison", launch_controller.clone())?;
   let native = verified_product(build, "native.production", "AppKitComparison", launch_controller)?;
   Ok((oxide, native))
}

fn validate_build_input_identity(build: &BuildManifest, plan_path: &Path) -> Result<()>
{
   let requested_spec_root = plan_path.parent().and_then(Path::parent).context("macOS comparison plan is not under a specification plans directory")?;
   let spec_root = fs::canonicalize(requested_spec_root).with_context(|| format!("resolving comparison specification root {}", requested_spec_root.display()))?;
   let workspace_root = spec_root.ancestors().find(|ancestor| {
      ancestor.join("Cargo.toml").is_file()
         && ancestor.join("host/apple-comparison").is_dir()
         && ancestor.join("benchmarks/comparative/specs/v1").is_dir()
   }).context("comparison specification root has no containing Oxide workspace")?;
   let source_root = workspace_root.join("host/apple-comparison");
   let (benchmark_source_manifest_sha256, _, _) = comparison_tree_manifest(&source_root, true)?;
   if benchmark_source_manifest_sha256 != build.benchmark_source_manifest_sha256
   {
      bail!("comparison benchmark source tree differs from the build manifest");
   }
   let (specification_manifest_sha256, _, _) = comparison_tree_manifest(&spec_root, false)?;
   if specification_manifest_sha256 != build.specification_manifest_sha256
   {
      bail!("comparison specification tree differs from the build manifest");
   }
   Ok(())
}

fn verified_launch_controller(controller: &BuildController) -> Result<VerifiedLaunchController>
{
   if controller.scheme != "MacOSComparisonController"
   {
      bail!("comparison build manifest has an unexpected macOS launch-controller scheme");
   }
   validate_sha256(&controller.xctestrun_sha256)?;
   validate_sha256(&controller.runner_bundle_manifest_sha256)?;
   if controller.runner_bundle_file_count == 0 || controller.runner_bundle_bytes == 0
   {
      bail!("comparison launch-controller bundle identity is empty");
   }
   let xctestrun = fs::canonicalize(&controller.xctestrun_path).with_context(|| format!("resolving {}", controller.xctestrun_path))?;
   let runner_bundle = fs::canonicalize(&controller.runner_bundle_path).with_context(|| format!("resolving {}", controller.runner_bundle_path))?;
   if xctestrun.extension().and_then(|extension| extension.to_str()) != Some("xctestrun")
      || runner_bundle.file_name().and_then(|name| name.to_str()) != Some("MacOSComparisonControllerUITests-Runner.app")
      || runner_bundle.parent().and_then(Path::parent) != xctestrun.parent()
   {
      bail!("comparison launch-controller artifacts do not have the frozen build-for-testing layout");
   }
   let verified = VerifiedLaunchController {
      xctestrun,
      xctestrun_sha256: controller.xctestrun_sha256.clone(),
      runner_bundle,
      runner_bundle_manifest_sha256: controller.runner_bundle_manifest_sha256.clone(),
      runner_bundle_file_count: controller.runner_bundle_file_count,
      runner_bundle_bytes: controller.runner_bundle_bytes,
   };
   validate_verified_launch_controller(&verified)?;
   Ok(verified)
}

fn verified_product(build: &BuildManifest, implementation_id: &str, expected_scheme: &str, launch_controller: VerifiedLaunchController) -> Result<VerifiedProduct>
{
   let mut matches = build.products.iter().filter(|product| product.implementation_id == implementation_id);
   let product = matches.next().with_context(|| format!("build manifest has no {} product", implementation_id))?;
   if matches.next().is_some() || product.scheme != expected_scheme
   {
      bail!("build manifest has an ambiguous or unexpected {} product", implementation_id);
   }
   validate_sha256(&product.executable_sha256)?;
   validate_sha256(&product.bundle_manifest_sha256)?;
   if product.bundle_file_count == 0 || product.bundle_bytes == 0
   {
      bail!("{} bundle identity is empty", implementation_id);
   }
   let bundle = PathBuf::from(&product.bundle_path);
   let executable = PathBuf::from(&product.executable_path);
   if bundle.extension().and_then(|extension| extension.to_str()) != Some("app")
   {
      bail!("{} bundle is not an application bundle", implementation_id);
   }
   let canonical_bundle = fs::canonicalize(&bundle).with_context(|| format!("resolving {}", bundle.display()))?;
   let canonical_executable = fs::canonicalize(&executable).with_context(|| format!("resolving {}", executable.display()))?;
   if !canonical_executable.starts_with(canonical_bundle.join("Contents/MacOS"))
   {
      bail!("{} executable is outside its declared application bundle", implementation_id);
   }
   let verified = VerifiedProduct {
      implementation_id: String::from(implementation_id),
      bundle,
      executable,
      executable_sha256: product.executable_sha256.clone(),
      bundle_manifest_sha256: product.bundle_manifest_sha256.clone(),
      bundle_file_count: product.bundle_file_count,
      bundle_bytes: product.bundle_bytes,
      launch_controller,
   };
   validate_verified_product(&verified, implementation_id)?;
   Ok(verified)
}

fn validate_verified_product(product: &VerifiedProduct, implementation_id: &str) -> Result<()>
{
   validate_verified_launch_controller(&product.launch_controller)?;
   let executable = fs::read(&product.executable).with_context(|| format!("reading {}", product.executable.display()))?;
   if sha256(&executable) != product.executable_sha256
   {
      bail!("{} executable differs from the build manifest", implementation_id);
   }
   let (bundle_manifest_sha256, bundle_file_count, bundle_bytes) = tree_manifest(&product.bundle)?;
   if bundle_manifest_sha256 != product.bundle_manifest_sha256
      || bundle_file_count != product.bundle_file_count
      || bundle_bytes != product.bundle_bytes
   {
      bail!("{} application bundle differs from the build manifest", implementation_id);
   }
   Ok(())
}

fn validate_verified_launch_controller(controller: &VerifiedLaunchController) -> Result<()>
{
   let xctestrun = fs::read(&controller.xctestrun).with_context(|| format!("reading {}", controller.xctestrun.display()))?;
   if sha256(&xctestrun) != controller.xctestrun_sha256
   {
      bail!("macOS launch-controller xctestrun differs from the build manifest");
   }
   let (bundle_manifest_sha256, bundle_file_count, bundle_bytes) = tree_manifest(&controller.runner_bundle)?;
   if bundle_manifest_sha256 != controller.runner_bundle_manifest_sha256
      || bundle_file_count != controller.runner_bundle_file_count
      || bundle_bytes != controller.runner_bundle_bytes
   {
      bail!("macOS launch-controller runner bundle differs from the build manifest");
   }
   Ok(())
}

pub fn validate_macos_pair_shape(first: &MacOsCampaignSession, second: &MacOsCampaignSession) -> Result<()>
{
   if first.chunk_id != second.chunk_id
      || first.pass_id != second.pass_id
      || first.pack_id != second.pack_id
      || first.timing != second.timing
      || first.pair_index != second.pair_index
      || first.order != second.order
      || first.max_occupied_seconds != second.max_occupied_seconds
      || first.side == second.side
      || !matches!(
         (first.order, first.side, second.side),
         (ComparisonOrder::Ab, ComparisonSide::Native, ComparisonSide::Oxide)
            | (ComparisonOrder::Ba, ComparisonSide::Oxide, ComparisonSide::Native)
      )
   {
      bail!("macOS campaign pair does not contain two symmetric opposite-side sessions");
   }
   Ok(())
}

fn product_for_side(executables: &(VerifiedProduct, VerifiedProduct), side: ComparisonSide) -> &VerifiedProduct
{
   match side
   {
      ComparisonSide::Oxide => &executables.0,
      ComparisonSide::Native => &executables.1,
   }
}

fn run_or_resume_pair(
   config: &MacOsCampaignConfig,
   plan: &MacOsCampaignPlan,
   pair: [&MacOsCampaignSession; 2],
   executables: &(VerifiedProduct, VerifiedProduct),
   build_manifest_sha256: &str,
   predecessor_pair_sha256: Option<&str>,
) -> Result<(Vec<MacOsCampaignSessionResult>, String, Option<MacOsCorrectnessPairResult>)>
{
   validate_macos_pair_shape(pair[0], pair[1])?;
   validate_sha256(build_manifest_sha256)?;
   if let Some(predecessor) = predecessor_pair_sha256
   {
      validate_sha256(predecessor)?;
   }
   let primary_checkpoint_path = pair_checkpoint_path(&config.output_root, plan, pair[0]);
   let upgraded_checkpoint_path = pair_upgraded_checkpoint_path(&config.output_root, plan, pair[0]);
   if upgraded_checkpoint_path.exists()
   {
      if !config.resume
      {
         bail!("existing upgraded macOS pair checkpoint requires an explicit --resume acquisition");
      }
      return validate_pair_checkpoint(
         &upgraded_checkpoint_path,
         config,
         plan,
         pair,
         executables,
         build_manifest_sha256,
         predecessor_pair_sha256,
      );
   }
   let legacy_checkpoint_present = primary_checkpoint_path.exists() && pair_checkpoint_schema_version(&primary_checkpoint_path)? == 1;
   let checkpoint_path = if legacy_checkpoint_present {&upgraded_checkpoint_path} else {&primary_checkpoint_path};
   if primary_checkpoint_path.exists() && !legacy_checkpoint_present
   {
      if !config.resume
      {
         bail!("existing macOS pair checkpoint requires an explicit --resume acquisition");
      }
      return validate_pair_checkpoint(
         &primary_checkpoint_path,
         config,
         plan,
         pair,
         executables,
         build_manifest_sha256,
         predecessor_pair_sha256,
      );
   }
   if legacy_checkpoint_present && !config.resume
   {
      bail!("schema-1 macOS pair checkpoint lacks a correctness binding; explicit --resume recovery is required");
   }

   let first_paths = session_paths(&config.output_root, plan, pair[0]);
   let second_paths = session_paths(&config.output_root, plan, pair[1]);
   let first_has_artifacts = session_side_has_artifacts(&first_paths.directory, pair[0].side)?;
   let second_has_artifacts = session_side_has_artifacts(&second_paths.directory, pair[1].side)?;
   let mut results = Vec::with_capacity(2);
   if first_has_artifacts || second_has_artifacts
   {
      if !config.resume
      {
         bail!("existing macOS pair artifacts require an explicit --resume acquisition");
      }
      if !first_has_artifacts || !second_has_artifacts
      {
         bail!(
            "macOS pair {}/{}/{} stopped inside a pair; cross-invocation side completion is forbidden and the partial pair requires explicit inspection",
            pair[0].chunk_id,
            pair[0].pass_id,
            pair[0].pair_index,
         );
      }
      for session in pair
      {
         let product = product_for_side(executables, session.side);
         validate_verified_product(product, &product.implementation_id)?;
         let paths = session_paths(&config.output_root, plan, session);
         let trusted_input_preflight = preflight_macos_session_trusted_input(config, plan, session)?;
         results.push(validate_session_files(
            plan,
            session,
            &paths,
            &product.executable_sha256,
            "recovered-before-pair-checkpoint",
            &trusted_input_preflight,
         )?);
      }
   }
   else if legacy_checkpoint_present
   {
      bail!("schema-1 macOS pair checkpoint is preserved but has no complete side evidence for schema-2 recovery");
   }
   else
   {
      for session in pair
      {
         let product = product_for_side(executables, session.side);
         results.push(run_or_resume_session(config, plan, session, product)?);
      }
   }
   persist_pair_checkpoint(
      checkpoint_path,
      config,
      plan,
      pair,
      executables,
      build_manifest_sha256,
      predecessor_pair_sha256,
      results,
   )
}

fn pair_checkpoint_path(output_root: &Path, plan: &MacOsCampaignPlan, session: &MacOsCampaignSession) -> PathBuf
{
   session_paths(output_root, plan, session).directory.join("pair.complete.json")
}

fn pair_upgraded_checkpoint_path(output_root: &Path, plan: &MacOsCampaignPlan, session: &MacOsCampaignSession) -> PathBuf
{
   session_paths(output_root, plan, session).directory.join("pair.complete.v2.json")
}

fn pair_checkpoint_schema_version(path: &Path) -> Result<u32>
{
   #[derive(Deserialize)]
   struct Schema
   {
      schema_version: u32,
   }

   let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
   let schema: Schema = serde_json::from_slice(&bytes).with_context(|| format!("decoding {}", path.display()))?;
   if schema.schema_version != 1 && schema.schema_version != 2
   {
      bail!("unsupported atomic macOS pair checkpoint schema {}", schema.schema_version);
   }
   Ok(schema.schema_version)
}

fn persist_pair_checkpoint(
   checkpoint_path: &Path,
   config: &MacOsCampaignConfig,
   plan: &MacOsCampaignPlan,
   pair: [&MacOsCampaignSession; 2],
   _executables: &(VerifiedProduct, VerifiedProduct),
   build_manifest_sha256: &str,
   predecessor_pair_sha256: Option<&str>,
   results: Vec<MacOsCampaignSessionResult>,
) -> Result<(Vec<MacOsCampaignSessionResult>, String, Option<MacOsCorrectnessPairResult>)>
{
   if checkpoint_path.exists()
   {
      bail!("refusing to overwrite atomic pair checkpoint {}", checkpoint_path.display());
   }
   let correctness = reduce_or_validate_pair_correctness(config, plan, pair, true)?;
   let checkpoint = MacOsPairCheckpoint {
      schema_version: 2,
      run_id: plan.run_id.clone(),
      plan_sha256: plan.plan_sha256.clone(),
      build_manifest_sha256: String::from(build_manifest_sha256),
      chunk_id: pair[0].chunk_id.clone(),
      pass_id: pair[0].pass_id.clone(),
      pack_id: pair[0].pack_id.clone(),
      pair_index: pair[0].pair_index,
      predecessor_pair_sha256: predecessor_pair_sha256.map(String::from),
      sessions: results,
      correctness,
      complete: true,
   };
   durable_json(&checkpoint, checkpoint_path)?;
   let bytes = fs::read(checkpoint_path).with_context(|| format!("reading {}", checkpoint_path.display()))?;
   let _: MacOsPairCheckpoint = serde_json::from_slice(&bytes).with_context(|| format!("decoding {}", checkpoint_path.display()))?;
   let mut expected_bytes = serde_json::to_vec_pretty(&checkpoint).context("encoding current atomic macOS pair checkpoint")?;
   expected_bytes.push(b'\n');
   if bytes != expected_bytes
   {
      bail!("new atomic macOS pair checkpoint did not round-trip exactly");
   }
   Ok((checkpoint.sessions, sha256(&bytes), checkpoint.correctness))
}

fn validate_pair_checkpoint(
   checkpoint_path: &Path,
   config: &MacOsCampaignConfig,
   plan: &MacOsCampaignPlan,
   pair: [&MacOsCampaignSession; 2],
   executables: &(VerifiedProduct, VerifiedProduct),
   build_manifest_sha256: &str,
   predecessor_pair_sha256: Option<&str>,
) -> Result<(Vec<MacOsCampaignSessionResult>, String, Option<MacOsCorrectnessPairResult>)>
{
   let bytes = fs::read(checkpoint_path).with_context(|| format!("reading {}", checkpoint_path.display()))?;
   let checkpoint: MacOsPairCheckpoint = serde_json::from_slice(&bytes).with_context(|| format!("decoding {}", checkpoint_path.display()))?;
   if checkpoint.schema_version != 2
      || checkpoint.run_id != plan.run_id
      || checkpoint.plan_sha256 != plan.plan_sha256
      || checkpoint.build_manifest_sha256 != build_manifest_sha256
      || checkpoint.chunk_id != pair[0].chunk_id
      || checkpoint.pass_id != pair[0].pass_id
      || checkpoint.pack_id != pair[0].pack_id
      || checkpoint.pair_index != pair[0].pair_index
      || checkpoint.predecessor_pair_sha256.as_deref() != predecessor_pair_sha256
      || checkpoint.sessions.len() != 2
      || !checkpoint.complete
   {
      bail!("atomic macOS pair checkpoint identity differs from the active campaign");
   }
   let mut validated = Vec::with_capacity(2);
   for (stored, session) in checkpoint.sessions.iter().zip(pair)
   {
      if stored.session != *session
      {
         bail!("atomic macOS pair checkpoint stores a different session identity");
      }
      let product = product_for_side(executables, session.side);
      validate_verified_product(product, &product.implementation_id)?;
      let paths = session_paths(&config.output_root, plan, session);
      let trusted_input_preflight = preflight_macos_session_trusted_input(config, plan, session)?;
      let current = validate_session_files(plan, session, &paths, &product.executable_sha256, "resumed-pair", &trusted_input_preflight)?;
      if !session_evidence_matches(stored, &current)
      {
         bail!("atomic macOS pair checkpoint evidence differs from its current durable side artifacts");
      }
      validated.push(current);
   }
   let correctness = reduce_or_validate_pair_correctness(config, plan, pair, false)?;
   let stored_correctness_bytes = serde_json::to_vec(&checkpoint.correctness).context("encoding stored atomic macOS correctness evidence")?;
   let current_correctness_bytes = serde_json::to_vec(&correctness).context("encoding current atomic macOS correctness evidence")?;
   if stored_correctness_bytes != current_correctness_bytes
   {
      bail!("atomic macOS pair checkpoint correctness evidence differs from the current calibrated-static reduction");
   }
   Ok((validated, sha256(&bytes), correctness))
}

fn pair_correctness_path(output_root: &Path, plan: &MacOsCampaignPlan, session: &MacOsCampaignSession) -> PathBuf
{
   session_paths(output_root, plan, session).directory.join("pair.correctness.json")
}

fn reduce_or_validate_pair_correctness(
   config: &MacOsCampaignConfig,
   plan: &MacOsCampaignPlan,
   pair: [&MacOsCampaignSession; 2],
   persist_if_missing: bool,
) -> Result<Option<MacOsCorrectnessPairResult>>
{
   if pair[0].pass_id != "correctness"
   {
      return Ok(None);
   }
   let spec_root = config.plan_path.parent().and_then(Path::parent).context("macOS correctness plan must be under the specification plans directory")?;
   let directory = session_paths(&config.output_root, plan, pair[0]).directory;
   let oxide_root = directory.join("oxide.evidence");
   let native_root = directory.join("native.evidence");
   let expected = if plan.plan_resource_path.is_some()
   {
      correctness::reduce_generic_apple_correctness_evidence(&oxide_root, &native_root, spec_root, &config.plan_path)?
   }
   else
   {
      reduce_apple_correctness_evidence(&oxide_root, &native_root, spec_root, Some(&pair[0].pack_id))?
   };
   if expected.plan_sha256 != plan.plan_sha256 || expected.pack_id.as_deref() != Some(pair[0].pack_id.as_str())
   {
      bail!("macOS correctness reduction identity differs from the active plan and pack");
   }
   let report_path = pair_correctness_path(&config.output_root, plan, pair[0]);
   let report = if report_path.exists()
   {
      let bytes = fs::read(&report_path).with_context(|| format!("reading {}", report_path.display()))?;
      let stored: AppleCorrectnessVisualReport = serde_json::from_slice(&bytes).with_context(|| format!("decoding {}", report_path.display()))?;
      let mut expected_bytes = serde_json::to_vec_pretty(&expected).context("encoding current macOS correctness report")?;
      expected_bytes.push(b'\n');
      if bytes != expected_bytes
      {
         bail!("existing macOS correctness report differs from the current durable evidence: {}", report_path.display());
      }
      stored
   }
   else if persist_if_missing
   {
      durable_json(&expected, &report_path)?;
      expected
   }
   else
   {
      bail!("atomic macOS correctness checkpoint has no durable reducer report: {}", report_path.display());
   };
   let report_bytes = fs::read(&report_path).with_context(|| format!("reading {}", report_path.display()))?;
   Ok(Some(MacOsCorrectnessPairResult {
      pack_id: pair[0].pack_id.clone(),
      pair_index: pair[0].pair_index,
      report_path: report_path.to_string_lossy().into_owned(),
      report_sha256: sha256(&report_bytes),
      accepted_checkpoint_count: report.accepted_checkpoint_count,
      rejected_checkpoint_count: report.rejected_checkpoint_count,
      accepted: report.accepted,
      report,
   }))
}

fn session_evidence_matches(stored: &MacOsCampaignSessionResult, current: &MacOsCampaignSessionResult) -> bool
{
   stored.session == current.session
      && stored.generation == current.generation
      && stored.executable_sha256 == current.executable_sha256
      && stored.artifact_sha256 == current.artifact_sha256
      && stored.acknowledgement_sha256 == current.acknowledgement_sha256
      && stored.trace_path.is_some() == current.trace_path.is_some()
      && stored.trace_toc_sha256 == current.trace_toc_sha256
      && stored.trace_signposts_sha256 == current.trace_signposts_sha256
      && stored.trace_updates_sha256 == current.trace_updates_sha256
      && stored.trace_frame_lifetimes_sha256 == current.trace_frame_lifetimes_sha256
      && stored.trace_correlation_path.is_some() == current.trace_correlation_path.is_some()
      && stored.trace_correlation_sha256 == current.trace_correlation_sha256
      && stored.trusted_input_receipt_manifest_path.is_some() == current.trusted_input_receipt_manifest_path.is_some()
      && stored.trusted_input_receipt_manifest_sha256 == current.trusted_input_receipt_manifest_sha256
      && stored.system_trace_path.is_some() == current.system_trace_path.is_some()
      && stored.system_trace_toc_sha256 == current.system_trace_toc_sha256
      && stored.system_trace_signposts_sha256 == current.system_trace_signposts_sha256
      && stored.system_trace_thread_info_sha256 == current.system_trace_thread_info_sha256
      && stored.system_trace_thread_state_sha256 == current.system_trace_thread_state_sha256
      && stored.system_trace_context_switch_sha256 == current.system_trace_context_switch_sha256
      && stored.system_trace_artifact_path.is_some() == current.system_trace_artifact_path.is_some()
      && stored.system_trace_artifact_sha256 == current.system_trace_artifact_sha256
      && stored.common_gpu_trace_path.is_some() == current.common_gpu_trace_path.is_some()
      && stored.common_gpu_toc_sha256 == current.common_gpu_toc_sha256
      && stored.common_gpu_signposts_sha256 == current.common_gpu_signposts_sha256
      && stored.common_gpu_samples_path.is_some() == current.common_gpu_samples_path.is_some()
      && stored.common_gpu_samples_sha256 == current.common_gpu_samples_sha256
      && stored.common_gpu_artifact_path.is_some() == current.common_gpu_artifact_path.is_some()
      && stored.common_gpu_artifact_sha256 == current.common_gpu_artifact_sha256
      && stored.launch_evidence_path.is_some() == current.launch_evidence_path.is_some()
      && stored.launch_evidence_sha256 == current.launch_evidence_sha256
      && stored.energy_config_sha256 == current.energy_config_sha256
      && stored.energy_request_sha256 == current.energy_request_sha256
      && stored.energy_ready_sha256 == current.energy_ready_sha256
      && stored.energy_raw_path.is_some() == current.energy_raw_path.is_some()
      && stored.energy_raw_sha256 == current.energy_raw_sha256
      && stored.energy_summary_path.is_some() == current.energy_summary_path.is_some()
      && stored.energy_summary_sha256 == current.energy_summary_sha256
      && stored.resource_sha256 == current.resource_sha256
      && stored.resource_availability == current.resource_availability
}

fn run_or_resume_canonical_launch_session(config: &MacOsCampaignConfig, plan: &MacOsCampaignPlan, session: &MacOsCampaignSession, product: &VerifiedProduct) -> Result<MacOsCampaignSessionResult>
{
   if !config.capture_presentation_traces
   {
      bail!("canonical macOS launch requires its preregistered all-process presentation trace");
   }
   let paths = session_paths(&config.output_root, plan, session);
   if paths.launch_complete.is_file()
      && paths.launch_acknowledgement.is_file()
      && paths.launch_ui_controller.is_file()
      && paths.launch_evidence.is_file()
      && paths.resource.is_file()
   {
      return validate_launch_session_files(plan, session, &paths, &product.executable_sha256, "resumed");
   }
   if session_side_has_artifacts(&paths.directory, session.side)?
   {
      bail!("incomplete canonical-launch session requires explicit inspection before retry: {}", paths.directory.display());
   }
   fs::create_dir_all(&paths.directory).with_context(|| format!("creating {}", paths.directory.display()))?;
   let stdout = File::create(&paths.stdout).with_context(|| format!("creating {}", paths.stdout.display()))?;
   let stderr = File::create(&paths.stderr).with_context(|| format!("creating {}", paths.stderr.display()))?;
   let generation = macos_session_generation(plan, session);
   let (prepared, expected) = prepare_macos_launch(
      plan,
      session,
      product,
      &paths,
      &generation,
      &stdout,
      &stderr,
   )?;
   if exact_process_id(&prepared.executable)?.is_some()
   {
      bail!("comparison executable is already running before its isolated launch session: {}", prepared.executable.display());
   }
   let (mut trace, trace_started_ticks) = start_launch_presentation_trace(config, session, &paths, &generation, &stdout, &stderr)?;
   let (mut controller, controller_result_bundle) = launch_controller_command(
      plan,
      session,
      product,
      &prepared.bundle,
      prepared.data_root.as_deref(),
      &expected,
      &config.output_root,
      "measure",
   )?;
   let controller_result = controller
      .stdout(Stdio::from(stdout.try_clone().context("cloning canonical-launch stdout")?))
      .stderr(Stdio::from(stderr.try_clone().context("cloning canonical-launch stderr")?))
      .spawn()
      .context("starting the macOS XCUIApplication launch controller");
   let mut controller = match controller_result
   {
      Ok(controller) => controller,
      Err(error) =>
      {
         terminate_child(&mut trace);
         return Err(error);
      }
   };
   let process_observed_ticks;
   let pid = match wait_for_exact_process_with_controller(&prepared.executable, &mut controller, Duration::from_secs(30))
   {
      Ok(pid) =>
      {
         process_observed_ticks = mach_continuous_time();
         pid
      }
      Err(error) =>
      {
         terminate_child(&mut controller);
         terminate_child(&mut trace);
         return Err(error);
      }
   };
   let resource_collector = match MacOsResourceCollector::start(pid, &session.pass_id)
   {
      Ok(collector) => collector,
      Err(error) =>
      {
         terminate_exact_process(&prepared.executable, pid);
         terminate_child(&mut controller);
         terminate_child(&mut trace);
         return Err(error);
      }
   };
   let acquisition = (|| -> Result<(MacOsResourceArtifact, u64)> {
      wait_for_canonical_launch_application_completion(&paths, &prepared.executable, pid, &mut controller, Duration::from_secs(session.max_occupied_seconds))?;
      let complete_timestamp = mach_continuous_time();
      let application_did_finish = read_json::<MacOsLaunchApplicationDidFinishReceipt>(&paths.launch_application_did_finish)?;
      let first_complete_ui = read_json::<MacOsLaunchFirstCompleteUIReceipt>(&paths.launch_first_complete_ui)?;
      let ready = read_json::<MacOsLaunchReadinessReceipt>(&paths.launch_ready)?;
      let complete = read_json::<MacOsLaunchCompleteReceipt>(&paths.launch_complete)?;
      validate_macos_launch_application_receipts(&application_did_finish, &first_complete_ui, &ready, &complete, &expected)?;
      let controller_start = read_json::<MacOsLaunchUIControllerStartReceipt>(&paths.launch_ui_controller_start)?;
      validate_macos_launch_ui_controller_start_receipt(&controller_start, &expected)?;
      let identity = MacOsResourceIdentity {
         plan,
         session,
         generation: &generation,
         executable_sha256: &product.executable_sha256,
         pid,
         launch_t0: controller_start.launch_request_ticks,
      };
      let resource = resource_collector.finish(&identity, complete_timestamp)?;
      Ok((resource, complete_timestamp))
   })();
   let (resource_artifact, complete_timestamp) = match acquisition
   {
      Ok(value) => value,
      Err(error) =>
      {
         let _ = post_notification(&format!("com.oxide.compare.launch.controller-finished.g{}", generation));
         terminate_exact_process(&prepared.executable, pid);
         terminate_child(&mut controller);
         terminate_child(&mut trace);
         return Err(error);
      }
   };
   post_notification(&format!("com.oxide.compare.launch.controller-finished.g{}", generation))?;
   let controller_status = wait_for_child(&mut controller, Duration::from_secs(30));
   if let Err(error) = controller_status
   {
      terminate_exact_process(&prepared.executable, pid);
      terminate_child(&mut trace);
      return Err(error);
   }
   let controller_status = controller_status?;
   if !controller_status.success()
   {
      terminate_exact_process(&prepared.executable, pid);
      terminate_child(&mut trace);
      bail!("macOS XCUIApplication launch controller failed with {}", controller_status);
   }
   controller_result_bundle.cleanup()?;
   wait_for_exact_process_exit(&prepared.executable, pid, Duration::from_secs(10))?;
   finish_presentation_trace(&mut trace, &paths.trace)?;
   let ui_controller = read_json::<MacOsLaunchUIControllerReceipt>(&paths.launch_ui_controller)?;
   validate_macos_launch_ui_controller_receipt(&ui_controller, &expected, "measure")?;
   let preparation_completed_ticks = finish_macos_launch_preparation(plan, session, product, &paths, &expected, &ui_controller)?;
   let controller_start = read_json::<MacOsLaunchUIControllerStartReceipt>(&paths.launch_ui_controller_start)?;
   validate_macos_launch_ui_controller_start_receipt(&controller_start, &expected)?;
   if controller_start.launch_request_ticks != ui_controller.launch_request_ticks
      || controller_start.ready_observed_ticks != ui_controller.ready_observed_ticks
      || ui_controller.clock_anchors.first() != Some(&controller_start.first_clock_anchor)
   {
      bail!("macOS launch controller start and completion receipts disagree");
   }
   let resource_identity = MacOsResourceIdentity {
      plan,
      session,
      generation: &generation,
      executable_sha256: &product.executable_sha256,
      pid,
      launch_t0: ui_controller.launch_request_ticks,
   };
   resource::validate_resource_artifact(&resource_artifact, &resource_identity)?;
   persist_resource_artifact(&resource_artifact, &resource_identity, &paths.resource)?;
   let correlation = export_and_validate_launch_trace(&paths, pid, &ui_controller.clock_anchors)?;
   let application_did_finish = read_json::<MacOsLaunchApplicationDidFinishReceipt>(&paths.launch_application_did_finish)?;
   let first_complete_ui = read_json::<MacOsLaunchFirstCompleteUIReceipt>(&paths.launch_first_complete_ui)?;
   let complete = read_json::<MacOsLaunchCompleteReceipt>(&paths.launch_complete)?;
   validate_launch_clock_correlation(&correlation, &first_complete_ui, &complete)?;
   let preparation_bytes = fs::read(&paths.cache_primer_receipt).with_context(|| format!("reading {}", paths.cache_primer_receipt.display()))?;
   let evidence = MacOsLaunchEvidence {
      schema_version: 4,
      run_id: plan.run_id.clone(),
      plan_sha256: plan.plan_sha256.clone(),
      generation: generation.clone(),
      executable_sha256: product.executable_sha256.clone(),
      side: session.side,
      launch_class: expected.launch_class,
      pid,
      preparation_completed_ticks,
      launch_request_ticks: ui_controller.launch_request_ticks,
      process_start_ticks: resource_artifact.process_start_continuous_time,
      process_observed_ticks,
      application_did_finish_launching_ticks: application_did_finish.application_did_finish_timestamp,
      first_complete_ui_generation_ticks: first_complete_ui.generation_marker_timestamp,
      first_attributed_present_proxy_ticks: correlation.first_attributed_present_proxy_ticks,
      first_interactive_input_request_ticks: ui_controller.input_request_ticks.context("measured macOS launch controller has no trusted input request timestamp")?,
      first_interactive_input_received_ticks: complete.trusted_input_received_timestamp,
      first_interactive_response_generation_ticks: complete.response_generation_timestamp,
      first_interactive_response_present_proxy_ticks: correlation.response_attributed_present_proxy_ticks,
      trace_started_before_launch_request: trace_started_ticks < ui_controller.launch_request_ticks,
      trace_all_processes: true,
      exact_pid_filtered: correlation.exact_pid_filtered,
      preparation_receipt_sha256: sha256(&preparation_bytes),
      presentation_calibration_status: String::from(MACOS_LAUNCH_PRESENTATION_CALIBRATION_PENDING),
      complete: true,
   };
   validate_macos_launch_evidence(&evidence, &expected)?;
   durable_json(&evidence, &paths.launch_evidence)?;
   let _ = complete_timestamp;
   validate_launch_session_files(plan, session, &paths, &product.executable_sha256, "launched")
}

fn macos_launch_expectation(plan: &MacOsCampaignPlan, session: &MacOsCampaignSession, product: &VerifiedProduct, generation: &str, installed_bundle: &Path, data_root: Option<&Path>) -> Result<MacOsLaunchExpectation>
{
   Ok(MacOsLaunchExpectation {
      run_id: plan.run_id.clone(),
      plan_sha256: plan.plan_sha256.clone(),
      chunk_id: session.chunk_id.clone(),
      pack_id: session.pack_id.clone(),
      pair_index: session.pair_index,
      generation: String::from(generation),
      executable_sha256: product.executable_sha256.clone(),
      side: session.side,
      launch_class: session_macos_launch_class(session)?,
      installed_bundle_path: normalized_existing_path(installed_bundle)?,
      data_container_path: data_root.map(normalized_future_path).transpose()?,
   })
}

fn normalized_existing_path(path: &Path) -> Result<String>
{
   Ok(fs::canonicalize(path).with_context(|| format!("canonicalizing {}", path.display()))?.to_string_lossy().into_owned())
}

fn normalized_future_path(path: &Path) -> Result<String>
{
   let parent = path.parent().context("future launch path has no parent")?;
   let name = path.file_name().context("future launch path has no file name")?;
   Ok(fs::canonicalize(parent).with_context(|| format!("canonicalizing {}", parent.display()))?.join(name).to_string_lossy().into_owned())
}

fn session_macos_launch_class(session: &MacOsCampaignSession) -> Result<MacOsLaunchClass>
{
   match session.launch_class.as_deref()
   {
      Some("terminated-warm-system-cache") => Ok(MacOsLaunchClass::TerminatedProcessWarmSystemCache),
      Some("fresh-install-first-launch") => Ok(MacOsLaunchClass::FreshInstallFirstLaunch),
      Some("warm-resume") => Ok(MacOsLaunchClass::WarmResume),
      Some(launch_class) => bail!("unsupported macOS canonical launch class {}", launch_class),
      None => bail!("macOS canonical launch session has no launch-class identity"),
   }
}

struct MacOsPreparedLaunch
{
   bundle: PathBuf,
   executable: PathBuf,
   data_root: Option<PathBuf>,
   cleanup_root: Option<PathBuf>,
}

impl Drop for MacOsPreparedLaunch
{
   fn drop(&mut self)
   {
      if let Some(root) = &self.cleanup_root
      {
         let _ = fs::remove_dir_all(root);
      }
   }
}

fn prepare_macos_launch(plan: &MacOsCampaignPlan, session: &MacOsCampaignSession, product: &VerifiedProduct, paths: &SessionPaths, generation: &str, stdout: &File, stderr: &File) -> Result<(MacOsPreparedLaunch, MacOsLaunchExpectation)>
{
   if paths.cache_primer_receipt.exists()
   {
      bail!("refusing to overwrite macOS launch preparation evidence");
   }
   let launch_class = session_macos_launch_class(session)?;
   match launch_class
   {
      MacOsLaunchClass::TerminatedProcessWarmSystemCache =>
      {
         let prepared = MacOsPreparedLaunch {
            bundle: product.bundle.clone(),
            executable: product.executable.clone(),
            data_root: None,
            cleanup_root: None,
         };
         let expected = macos_launch_expectation(plan, session, product, generation, &prepared.bundle, None)?;
         run_macos_launch_cache_primer(plan, session, product, paths, &expected, stdout, stderr)?;
         Ok((prepared, expected))
      }
      MacOsLaunchClass::FreshInstallFirstLaunch =>
      {
         if paths.cache_primer_root.exists()
         {
            bail!("refusing to overwrite a macOS fresh-install root");
         }
         fs::create_dir_all(&paths.cache_primer_root).with_context(|| format!("creating {}", paths.cache_primer_root.display()))?;
         let result = (|| -> Result<(MacOsPreparedLaunch, MacOsLaunchExpectation)> {
            let bundle_name = product.bundle.file_name().context("fresh-install source bundle has no file name")?;
            let bundle = paths.cache_primer_root.join(bundle_name);
            let executable_relative = product.executable.strip_prefix(&product.bundle).context("comparison executable is outside its verified bundle")?;
            let executable = bundle.join(executable_relative);
            let data_root = paths.cache_primer_root.join("data-container");
            let status = run_child(
               {
                  let mut command = Command::new("/usr/bin/ditto");
                  command.arg(&product.bundle).arg(&bundle);
                  command
               },
               stdout.try_clone().context("cloning fresh-install stdout")?,
               stderr.try_clone().context("cloning fresh-install stderr")?,
               session.max_occupied_seconds,
            )?;
            if !status.success()
            {
               bail!("copying the session-owned fresh-install app failed with {}", status);
            }
            if sha256(&fs::read(&executable).with_context(|| format!("reading copied executable {}", executable.display()))?) != product.executable_sha256
            {
               bail!("session-owned fresh-install executable differs from the verified build");
            }
            if data_root.exists()
            {
               bail!("fresh-install data container existed before controller provisioning");
            }
            let prepared = MacOsPreparedLaunch {
               bundle,
               executable,
               data_root: Some(data_root),
               cleanup_root: Some(paths.cache_primer_root.clone()),
            };
            let expected = macos_launch_expectation(plan, session, product, generation, &prepared.bundle, prepared.data_root.as_deref())?;
            let receipt = MacOsLaunchPreparationReceipt {
               schema_version: 1,
               run_id: plan.run_id.clone(),
               plan_sha256: plan.plan_sha256.clone(),
               generation: String::from(generation),
               side: session.side,
               launch_class,
               executable_sha256: product.executable_sha256.clone(),
               installed_bundle_path: expected.installed_bundle_path.clone(),
               data_container_path: expected.data_container_path.clone(),
               cache_primed: false,
               installed_bundle_was_absent: Some(true),
               data_container_was_absent: Some(true),
               completed_ticks: mach_continuous_time(),
               complete: true,
            };
            validate_macos_launch_preparation_receipt(&receipt, &expected)?;
            durable_json(&receipt, &paths.cache_primer_receipt)?;
            Ok((prepared, expected))
         })();
         if result.is_err()
         {
            let _ = fs::remove_dir_all(&paths.cache_primer_root);
         }
         result
      }
      MacOsLaunchClass::WarmResume =>
      {
         if paths.cache_primer_root.exists()
         {
            bail!("warm-resume session unexpectedly has a fresh-install or cache-primer root");
         }
         let prepared = MacOsPreparedLaunch {
            bundle: product.bundle.clone(),
            executable: product.executable.clone(),
            data_root: None,
            cleanup_root: None,
         };
         let expected = macos_launch_expectation(plan, session, product, generation, &prepared.bundle, None)?;
         Ok((prepared, expected))
      }
   }
}

fn run_macos_launch_cache_primer(plan: &MacOsCampaignPlan, session: &MacOsCampaignSession, product: &VerifiedProduct, paths: &SessionPaths, expected: &MacOsLaunchExpectation, stdout: &File, stderr: &File) -> Result<u64>
{
   if paths.cache_primer_root.exists() || paths.cache_primer_receipt.exists()
   {
      bail!("refusing to overwrite macOS launch cache-primer evidence");
   }
   fs::create_dir_all(&paths.cache_primer_root).with_context(|| format!("creating {}", paths.cache_primer_root.display()))?;
   let (command, controller_result_bundle) = launch_controller_command(
      plan,
      session,
      product,
      &product.bundle,
      None,
      expected,
      &paths.cache_primer_root,
      "cache-primer",
   )?;
   let status = run_child(
      command,
      stdout.try_clone().context("cloning cache-primer stdout")?,
      stderr.try_clone().context("cloning cache-primer stderr")?,
      session.max_occupied_seconds,
   )?;
   controller_result_bundle.cleanup()?;
   if !status.success()
   {
      bail!("macOS XCUIApplication cache primer failed with {}", status);
   }
   if exact_process_id(&product.executable)?.is_some()
   {
      bail!("macOS cache-primer application remained running after its UI controller returned");
   }
   let primer_paths = session_paths(&paths.cache_primer_root, plan, session);
   let ready_bytes = fs::read(&primer_paths.launch_ready).with_context(|| format!("reading {}", primer_paths.launch_ready.display()))?;
   let controller_bytes = fs::read(&primer_paths.launch_ui_controller).with_context(|| format!("reading {}", primer_paths.launch_ui_controller.display()))?;
   let ready: MacOsLaunchReadinessReceipt = serde_json::from_slice(&ready_bytes).with_context(|| format!("decoding {}", primer_paths.launch_ready.display()))?;
   let controller: MacOsLaunchUIControllerReceipt = serde_json::from_slice(&controller_bytes).with_context(|| format!("decoding {}", primer_paths.launch_ui_controller.display()))?;
   validate_macos_launch_cache_primer_receipts(&ready, &controller, expected)?;
   let completed_ticks = mach_continuous_time();
   let receipt = MacOsLaunchPreparationReceipt {
      schema_version: 1,
      run_id: plan.run_id.clone(),
      plan_sha256: plan.plan_sha256.clone(),
      generation: expected.generation.clone(),
      side: session.side,
      launch_class: expected.launch_class,
      executable_sha256: product.executable_sha256.clone(),
      installed_bundle_path: expected.installed_bundle_path.clone(),
      data_container_path: None,
      cache_primed: true,
      installed_bundle_was_absent: None,
      data_container_was_absent: None,
      completed_ticks,
      complete: true,
   };
   validate_macos_launch_preparation_receipt(&receipt, expected)?;
   durable_json(&receipt, &paths.cache_primer_receipt)?;
   Ok(completed_ticks)
}

fn finish_macos_launch_preparation(plan: &MacOsCampaignPlan, session: &MacOsCampaignSession, product: &VerifiedProduct, paths: &SessionPaths, expected: &MacOsLaunchExpectation, controller: &MacOsLaunchUIControllerReceipt) -> Result<u64>
{
   if expected.launch_class == MacOsLaunchClass::WarmResume
   {
      let completed_ticks = controller.suspended_observed_ticks.context("warm-resume controller has no proven suspension timestamp")?;
      let receipt = MacOsLaunchPreparationReceipt {
         schema_version: 1,
         run_id: plan.run_id.clone(),
         plan_sha256: plan.plan_sha256.clone(),
         generation: expected.generation.clone(),
         side: session.side,
         launch_class: expected.launch_class,
         executable_sha256: product.executable_sha256.clone(),
         installed_bundle_path: expected.installed_bundle_path.clone(),
         data_container_path: None,
         cache_primed: false,
         installed_bundle_was_absent: None,
         data_container_was_absent: None,
         completed_ticks,
         complete: true,
      };
      validate_macos_launch_preparation_receipt(&receipt, expected)?;
      durable_json(&receipt, &paths.cache_primer_receipt)?;
   }
   let receipt = read_json::<MacOsLaunchPreparationReceipt>(&paths.cache_primer_receipt)?;
   validate_macos_launch_preparation_receipt(&receipt, expected)?;
   if expected.launch_class == MacOsLaunchClass::FreshInstallFirstLaunch
   {
      let data_root = expected.data_container_path.as_deref().context("fresh-install expectation has no data container")?;
      let marker = Path::new(data_root).join(format!("install-identity.{}", expected.generation));
      let marker_generation = fs::read_to_string(&marker).with_context(|| format!("reading fresh-install identity marker {}", marker.display()))?;
      if marker_generation != expected.generation
      {
         bail!("fresh-install app did not claim its empty session-owned data container");
      }
   }
   Ok(receipt.completed_ticks)
}

struct MacOsControllerResultBundle
{
   path: PathBuf,
   xctestrun_path: PathBuf,
}

impl MacOsControllerResultBundle
{
   fn cleanup(self) -> Result<()>
   {
      self.remove()
   }

   fn remove(&self) -> Result<()>
   {
      if self.path.exists()
      {
         fs::remove_dir_all(&self.path).with_context(|| format!("removing session-owned XCTest result bundle {}", self.path.display()))?;
      }
      if self.xctestrun_path.exists()
      {
         fs::remove_file(&self.xctestrun_path).with_context(|| format!("removing session-owned XCTest run specification {}", self.xctestrun_path.display()))?;
      }
      Ok(())
   }
}

impl Drop for MacOsControllerResultBundle
{
   fn drop(&mut self)
   {
      let _ = self.remove();
   }
}

fn materialize_controller_xctestrun(source: &Path, destination: &Path, environment: &BTreeMap<String, String>) -> Result<()>
{
   if destination.exists()
   {
      bail!("refusing to overwrite session-owned XCTest run specification {}", destination.display());
   }
   let mut root = plist::Value::from_file(source).with_context(|| format!("reading {}", source.display()))?;
   {
      let root = root.as_dictionary_mut().context("macOS launch-controller xctestrun root is not a dictionary")?;
      let target = root.get_mut("MacOSComparisonControllerUITests").context("macOS launch-controller xctestrun has no UI-test target")?
         .as_dictionary_mut().context("macOS launch-controller xctestrun UI-test target is not a dictionary")?;
      let variables = target.get_mut("EnvironmentVariables").context("macOS launch-controller xctestrun has no environment dictionary")?
         .as_dictionary_mut().context("macOS launch-controller xctestrun environment is not a dictionary")?;
      for (key, value) in environment
      {
         variables.insert(key.clone(), plist::Value::String(value.clone()));
      }
   }
   plist::to_file_xml(destination, &root).with_context(|| format!("writing {}", destination.display()))
}

fn session_controller_xctestrun_path(plan: &MacOsCampaignPlan, session: &MacOsCampaignSession, controller: &VerifiedLaunchController, mode: &str) -> Result<PathBuf>
{
   let parent = controller.xctestrun.parent().context("macOS launch-controller xctestrun has no build root")?;
   Ok(parent.join(format!(".oxide-controller-{}.{}.{}.xctestrun", macos_session_generation(plan, session), session.side.as_str(), mode)))
}

pub fn macos_controller_result_bundle_path(plan: &MacOsCampaignPlan, session: &MacOsCampaignSession, output_root: &Path, mode: &str) -> Result<PathBuf>
{
   if mode != "cache-primer" && mode != "measure"
   {
      bail!("unknown macOS launch-controller mode {}", mode);
   }
   Ok(session_paths(output_root, plan, session).directory.join(format!("{}.{}.controller.xcresult", session.side.as_str(), mode)))
}

fn launch_controller_command(plan: &MacOsCampaignPlan, session: &MacOsCampaignSession, product: &VerifiedProduct, launch_bundle: &Path, data_root: Option<&Path>, expected: &MacOsLaunchExpectation, output_root: &Path, mode: &str) -> Result<(Command, MacOsControllerResultBundle)>
{
   validate_verified_launch_controller(&product.launch_controller)?;
   let result_bundle = MacOsControllerResultBundle {
      path: macos_controller_result_bundle_path(plan, session, output_root, mode)?,
      xctestrun_path: session_controller_xctestrun_path(plan, session, &product.launch_controller, mode)?,
   };
   if result_bundle.path.exists() || result_bundle.xctestrun_path.exists()
   {
      bail!("refusing to overwrite session-owned XCTest controller artifacts in {}", result_bundle.path.parent().unwrap_or(Path::new(".")).display());
   }
   let mut environment = BTreeMap::new();
   environment.insert(String::from("OXIDE_COMPARISON_APP_BUNDLE"), launch_bundle.to_string_lossy().into_owned());
   environment.insert(String::from("OXIDE_COMPARISON_PLAN_SHA"), plan.plan_sha256.clone());
   environment.insert(String::from("OXIDE_COMPARISON_RUN_ID"), plan.run_id.clone());
   environment.insert(String::from("OXIDE_COMPARISON_CHUNK"), session.chunk_id.clone());
   environment.insert(String::from("OXIDE_COMPARISON_PASS_ID"), session.pass_id.clone());
   environment.insert(String::from("OXIDE_COMPARISON_PACK_ID"), session.pack_id.clone());
   environment.insert(String::from("OXIDE_COMPARISON_PAIR_INDEX"), session.pair_index.to_string());
   environment.insert(String::from("OXIDE_COMPARISON_SIDE"), String::from(session.side.as_str()));
   environment.insert(String::from("OXIDE_COMPARISON_GENERATION"), macos_session_generation(plan, session));
   environment.insert(String::from("OXIDE_COMPARISON_OUTPUT_ROOT"), output_root.to_string_lossy().into_owned());
   environment.insert(String::from("OXIDE_COMPARISON_TIMEOUT_SECONDS"), session.max_occupied_seconds.to_string());
   environment.insert(String::from("OXIDE_COMPARISON_LAUNCH_MODE"), String::from(mode));
   environment.insert(String::from("OXIDE_COMPARISON_LAUNCH_CLASS"), String::from(expected.launch_class.as_str()));
   if let Some(data_root) = data_root
   {
      environment.insert(String::from("OXIDE_COMPARISON_DATA_ROOT"), data_root.to_string_lossy().into_owned());
   }
   if let Some(path) = &plan.plan_resource_path
   {
      environment.insert(String::from("OXIDE_COMPARISON_PLAN_PATH"), path.clone());
   }
   materialize_controller_xctestrun(&product.launch_controller.xctestrun, &result_bundle.xctestrun_path, &environment)?;
   let mut command = Command::new("/usr/bin/arch");
   command.args(["-arm64", "/usr/bin/xcodebuild", "test-without-building", "-destination", "platform=macOS,arch=arm64", "-xctestrun"])
      .arg(&result_bundle.xctestrun_path)
      .arg("-resultBundlePath")
      .arg(&result_bundle.path)
      .arg("-only-testing:MacOSComparisonControllerUITests/MacOSComparisonControllerUITests/testExactForegroundSession")
      .env("ARCHPREFERENCE", "arm64,arm64e")
      .envs(&environment);
   Ok((command, result_bundle))
}

fn foreground_controller_command(plan: &MacOsCampaignPlan, session: &MacOsCampaignSession, product: &VerifiedProduct, launch_bundle: &Path, output_root: &Path) -> Result<(Command, MacOsControllerResultBundle)>
{
   validate_verified_launch_controller(&product.launch_controller)?;
   let result_bundle = MacOsControllerResultBundle {
      path: macos_controller_result_bundle_path(plan, session, output_root, "measure")?,
      xctestrun_path: session_controller_xctestrun_path(plan, session, &product.launch_controller, "measure")?,
   };
   if result_bundle.path.exists() || result_bundle.xctestrun_path.exists()
   {
      bail!("refusing to overwrite session-owned XCTest controller artifacts in {}", result_bundle.path.parent().unwrap_or(Path::new(".")).display());
   }
   let mut environment = BTreeMap::new();
   environment.insert(String::from("OXIDE_COMPARISON_APP_BUNDLE"), launch_bundle.to_string_lossy().into_owned());
   environment.insert(String::from("OXIDE_COMPARISON_PLAN_SHA"), plan.plan_sha256.clone());
   environment.insert(String::from("OXIDE_COMPARISON_RUN_ID"), plan.run_id.clone());
   environment.insert(String::from("OXIDE_COMPARISON_CHUNK"), session.chunk_id.clone());
   environment.insert(String::from("OXIDE_COMPARISON_PASS_ID"), session.pass_id.clone());
   environment.insert(String::from("OXIDE_COMPARISON_PACK_ID"), session.pack_id.clone());
   environment.insert(String::from("OXIDE_COMPARISON_PAIR_INDEX"), session.pair_index.to_string());
   environment.insert(String::from("OXIDE_COMPARISON_SIDE"), String::from(session.side.as_str()));
   environment.insert(String::from("OXIDE_COMPARISON_GENERATION"), macos_session_generation(plan, session));
   environment.insert(String::from("OXIDE_COMPARISON_OUTPUT_ROOT"), output_root.to_string_lossy().into_owned());
   environment.insert(String::from("OXIDE_COMPARISON_TIMEOUT_SECONDS"), session.max_occupied_seconds.to_string());
   if let Some(path) = &plan.plan_resource_path
   {
      environment.insert(String::from("OXIDE_COMPARISON_PLAN_PATH"), path.clone());
   }
   if let Some(overlay) = &session.scale_overlay
   {
      environment.insert(String::from("OXIDE_COMPARISON_SCALE_OVERLAY_PATH"), overlay.path.clone());
      environment.insert(String::from("OXIDE_COMPARISON_SCALE_OVERLAY_SHA"), overlay.sha256.clone());
   }
   materialize_controller_xctestrun(&product.launch_controller.xctestrun, &result_bundle.xctestrun_path, &environment)?;
   let mut command = Command::new("/usr/bin/arch");
   command.args(["-arm64", "/usr/bin/xcodebuild", "test-without-building", "-destination", "platform=macOS,arch=arm64", "-xctestrun"])
      .arg(&result_bundle.xctestrun_path)
      .arg("-resultBundlePath")
      .arg(&result_bundle.path)
      .arg("-only-testing:MacOSComparisonControllerUITests/MacOSComparisonControllerUITests/testExactForegroundSession")
      .env("ARCHPREFERENCE", "arm64,arm64e")
      .envs(&environment);
   Ok((command, result_bundle))
}

fn wait_for_exact_process_with_controller(executable: &Path, controller: &mut Child, timeout: Duration) -> Result<u32>
{
   let deadline = Instant::now().checked_add(timeout).context("launch process discovery timeout overflow")?;
   loop
   {
      if let Some(pid) = exact_process_id(executable)?
      {
         return Ok(pid);
      }
      if let Some(status) = controller.try_wait().context("checking macOS launch UI controller during process discovery")?
      {
         bail!("macOS launch UI controller exited before launching the comparator with {}", status);
      }
      if Instant::now() >= deadline
      {
         bail!("XCUIApplication did not launch the exact comparison process within {} seconds", timeout.as_secs());
      }
      thread::sleep(Duration::from_millis(25));
   }
}

fn wait_for_canonical_launch_application_completion(paths: &SessionPaths, executable: &Path, pid: u32, controller: &mut Child, timeout: Duration) -> Result<()>
{
   let deadline = Instant::now().checked_add(timeout).context("canonical launch completion timeout overflow")?;
   loop
   {
      if paths.launch_complete.is_file() && paths.launch_acknowledgement.is_file()
      {
         return Ok(());
      }
      if trace_working_set_bytes(&paths.trace, &paths.trace_scratch)? > MACOS_TRACE_WORKING_SET_LIMIT_BYTES
      {
         bail!("canonical macOS launch trace exceeded the 512 MiB bundle-plus-scratch limit");
      }
      if exact_process_id(executable)? != Some(pid)
      {
         bail!("macOS canonical-launch PID {} exited before durable response completion", pid);
      }
      if let Some(status) = controller.try_wait().context("checking macOS launch UI controller during acquisition")?
      {
         bail!("macOS launch UI controller exited before durable response completion with {}", status);
      }
      if Instant::now() >= deadline
      {
         bail!("macOS canonical launch did not complete within {} seconds", timeout.as_secs());
      }
      thread::sleep(Duration::from_millis(25));
   }
}

fn start_launch_presentation_trace(config: &MacOsCampaignConfig, session: &MacOsCampaignSession, paths: &SessionPaths, generation: &str, stdout: &File, stderr: &File) -> Result<(MacOsPresentationTrace, u64)>
{
   if paths.trace.exists()
   {
      bail!("refusing to overwrite trace {}", paths.trace.display());
   }
   let started_notification = format!("com.oxide.compare.launch.trace-started.g{}", generation);
   let mut started_waiter = Command::new("/usr/bin/notifyutil")
      .args(["-q", "-1", &started_notification])
      .stdout(Stdio::null())
      .stderr(Stdio::from(stderr.try_clone().context("cloning launch trace-started stderr")?))
      .spawn()
      .context("registering launch trace-started notification")?;
   let scratch = match MacOsTraceScratch::create(&paths.trace_scratch)
   {
      Ok(scratch) => scratch,
      Err(error) =>
      {
         terminate_child(&mut started_waiter);
         return Err(error);
      }
   };
   let mut command = native_xcrun_command();
   command.env("TMPDIR", &scratch.path).env("TMP", &scratch.path).env("TEMP", &scratch.path);
   let trace_result = command.args([
      "xctrace", "record",
      "--template", &config.xctrace_template,
      "--instrument", "os_signpost",
      "--notify-tracing-started", &started_notification,
      "--output",
   ])
      .arg(&paths.trace)
      .args(["--time-limit", &format!("{}s", session.max_occupied_seconds), "--no-prompt", "--all-processes"])
      .stdout(Stdio::from(stdout.try_clone().context("cloning launch xctrace stdout")?))
      .stderr(Stdio::from(stderr.try_clone().context("cloning launch xctrace stderr")?))
      .spawn()
      .context("starting prelaunch all-process presentation trace");
   let trace = match trace_result
   {
      Ok(trace) => trace,
      Err(error) =>
      {
         terminate_child(&mut started_waiter);
         return Err(error);
      }
   };
   let mut trace = MacOsPresentationTrace::new(trace, scratch);
   if let Err(error) = wait_for_presentation_trace_start(&mut trace, &paths.trace, &mut started_waiter, Duration::from_secs(20))
   {
      terminate_child(&mut trace);
      terminate_child(&mut started_waiter);
      return Err(error);
   }
   Ok((trace, mach_continuous_time()))
}

fn interrupt_presentation_trace(trace: &mut Child) -> Result<()>
{
   if trace.try_wait().context("checking presentation trace before interrupt")?.is_some()
   {
      return Ok(());
   }
   let status = Command::new("/bin/kill").args(["-INT", &trace.id().to_string()]).status().context("interrupting xctrace for graceful finalization")?;
   if !status.success()
   {
      bail!("sending SIGINT to xctrace failed with {}", status);
   }
   Ok(())
}

fn export_and_validate_launch_trace(paths: &SessionPaths, pid: u32, anchors: &[launch::MacOsLaunchClockAnchor]) -> Result<MacOsLaunchPresentationCorrelation>
{
   validate_macos_presentation_trace_bundle(&paths.trace)?;
   export_trace_xml(&paths.trace, &["--toc"], &paths.trace_toc)?;
   export_trace_xml(&paths.trace, &["--xpath", "/trace-toc/run[@number=\"1\"]/data/table[@schema=\"os-signpost\"]"], &paths.trace_signposts)?;
   export_trace_xml(&paths.trace, &["--xpath", "/trace-toc/run[@number=\"1\"]/data/table[@schema=\"hitches-updates\"]"], &paths.trace_updates)?;
   export_trace_xml(&paths.trace, &["--xpath", "/trace-toc/run[@number=\"1\"]/data/table[@schema=\"hitches-frame-lifetimes\"]"], &paths.trace_frame_lifetimes)?;
   let toc = fs::read_to_string(&paths.trace_toc).with_context(|| format!("reading {}", paths.trace_toc.display()))?;
   let normalized_toc = toc.to_ascii_lowercase();
   for schema in ["os-signpost", "hitches-updates", "hitches-frame-lifetimes"]
   {
      if !normalized_toc.contains(schema)
      {
         bail!("macOS launch trace has no {} schema", schema);
      }
   }
   let signposts = fs::read_to_string(&paths.trace_signposts).with_context(|| format!("reading {}", paths.trace_signposts.display()))?;
   let updates = fs::read_to_string(&paths.trace_updates).with_context(|| format!("reading {}", paths.trace_updates.display()))?;
   let frame_lifetimes = fs::read_to_string(&paths.trace_frame_lifetimes).with_context(|| format!("reading {}", paths.trace_frame_lifetimes.display()))?;
   let anchors = anchors.iter().map(|anchor| MacOsLaunchClockAnchorInterval {
      id: anchor.id,
      before_ticks: anchor.before_ticks,
      after_ticks: anchor.after_ticks,
   }).collect::<Vec<_>>();
   let correlation = correlate_macos_launch_presentation_trace(&signposts, &updates, &frame_lifetimes, pid, &anchors)?;
   durable_json(&correlation, &paths.trace_correlation)?;
   Ok(correlation)
}

fn validate_launch_clock_correlation(correlation: &MacOsLaunchPresentationCorrelation, first_complete_ui: &MacOsLaunchFirstCompleteUIReceipt, complete: &MacOsLaunchCompleteReceipt) -> Result<()>
{
   let tolerance = mach_ticks_for_nanoseconds(1_000_000)?.saturating_add(correlation.clock_mapping_max_uncertainty_ticks);
   for (mapped, application, label) in [
      (correlation.first_visual_generation_ticks, first_complete_ui.generation_marker_timestamp, "first visual generation"),
      (correlation.input_received_ticks, complete.trusted_input_received_timestamp, "trusted input"),
      (correlation.response_visual_generation_ticks, complete.response_generation_timestamp, "response visual generation"),
   ]
   {
      if mapped.abs_diff(application) > tolerance
      {
         bail!("macOS launch {} clock correlation exceeds the one-millisecond pre-calibration bound", label);
      }
   }
   Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T>
{
   let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
   serde_json::from_slice(&bytes).with_context(|| format!("decoding {}", path.display()))
}

fn run_or_resume_session(config: &MacOsCampaignConfig, plan: &MacOsCampaignPlan, session: &MacOsCampaignSession, product: &VerifiedProduct) -> Result<MacOsCampaignSessionResult>
{
   validate_verified_product(product, &product.implementation_id)?;
   if session.pass_id == "canonical-launch"
   {
      return run_or_resume_canonical_launch_session(config, plan, session, product);
   }
   let trusted_input_preflight = preflight_macos_session_trusted_input(config, plan, session)?;
   let executable = &product.executable;
   let paths = session_paths(&config.output_root, plan, session);
   if paths.complete.is_file() && paths.acknowledgement.is_file() && paths.controller_receipt.is_file()
   {
      return validate_session_files(plan, session, &paths, &product.executable_sha256, "resumed", &trusted_input_preflight);
   }
   if session_side_has_artifacts(&paths.directory, session.side)?
   {
      bail!("incomplete session directory requires explicit inspection before retry: {}", paths.directory.display());
   }
   fs::create_dir_all(&paths.directory).with_context(|| format!("creating {}", paths.directory.display()))?;
   let stdout = File::create(&paths.stdout).with_context(|| format!("creating {}", paths.stdout.display()))?;
   let stderr = File::create(&paths.stderr).with_context(|| format!("creating {}", paths.stderr.display()))?;
   let arguments = macos_session_arguments(plan, session, &config.output_root);
   let generation = macos_session_generation(plan, session);
   let application_bundle = macos_application_bundle(executable)?;
   if exact_process_id(executable)?.is_some()
   {
      bail!("comparison executable is already running before its isolated session: {}", executable.display());
   }
   let ready_notification = format!("com.oxide.compare.ready.g{}", generation);
   let start_notification = format!("com.oxide.compare.start.g{}", generation);
   let stop_notification = format!("com.oxide.compare.stop.g{}", generation);
   let stop_ready_notification = format!("com.oxide.compare.stop-ready.g{}", generation);
   let mut ready_waiter = Command::new("/usr/bin/notifyutil")
      .args(["-q", "-1", &ready_notification])
      .stdout(Stdio::null())
      .stderr(Stdio::from(stderr.try_clone().context("cloning comparison stderr for ready waiter")?))
      .spawn()
      .context("registering macOS campaign ready notification")?;
   thread::sleep(Duration::from_millis(100));
   if let Some(status) = ready_waiter.try_wait().context("checking macOS ready waiter")?
   {
      bail!("macOS ready notification waiter exited before launch with {}", status);
   }

   let launch_t0 = mach_continuous_time();
   let mut controller = None;
   let mut controller_result_bundle = None;
   let pid = if session.pass_role == AppleCampaignPassRole::Correctness
   {
      let mut open = Command::new("/usr/bin/open");
      open.args(["-n"]).arg(&application_bundle).arg("--args").args(&arguments);
      let open_status = run_child(
         open,
         stdout.try_clone().context("cloning comparison stdout for bundle launch")?,
         stderr.try_clone().context("cloning comparison stderr for bundle launch")?,
         10,
      )?;
      if !open_status.success()
      {
         terminate_child(&mut ready_waiter);
         bail!("opening exact comparison bundle failed with {}: {}", open_status, application_bundle.display());
      }
      match wait_for_exact_process(executable, Duration::from_secs(10))
      {
         Ok(pid) => pid,
         Err(error) =>
         {
            terminate_child(&mut ready_waiter);
            return Err(error);
         }
      }
   }
   else
   {
      let (mut command, result_bundle) = match foreground_controller_command(plan, session, product, &application_bundle, &config.output_root)
      {
         Ok(value) => value,
         Err(error) =>
         {
            terminate_child(&mut ready_waiter);
            return Err(error);
         }
      };
      let child = match command
         .stdout(Stdio::from(stdout.try_clone().context("cloning comparison stdout for XCUI controller")?))
         .stderr(Stdio::from(stderr.try_clone().context("cloning comparison stderr for XCUI controller")?))
         .spawn()
         .context("starting the measured macOS XCUI input controller")
      {
         Ok(child) => child,
         Err(error) =>
         {
            terminate_child(&mut ready_waiter);
            return Err(error);
         }
      };
      controller = Some(child);
      controller_result_bundle = Some(result_bundle);
      match wait_for_exact_process_with_controller(executable, controller.as_mut().expect("controller child"), Duration::from_secs(30))
      {
         Ok(pid) => pid,
         Err(error) =>
         {
            terminate_child(&mut ready_waiter);
            if let Some(controller) = controller.as_mut()
            {
               terminate_child(controller);
            }
            return Err(error);
         }
      }
   };
   let stop_ready_waiter = (|| -> Result<Child> {
      Command::new("/usr/bin/notifyutil")
         .args(["-q", "-1", &stop_ready_notification])
         .stdout(Stdio::null())
         .stderr(Stdio::from(stderr.try_clone().context("cloning comparison stderr for stop-ready waiter")?))
         .spawn()
         .context("registering macOS campaign stop-ready notification")
   })();
   let mut stop_ready_waiter = match stop_ready_waiter
   {
      Ok(waiter) => waiter,
      Err(error) =>
      {
         terminate_child(&mut ready_waiter);
         terminate_exact_process(executable, pid);
         if let Some(controller) = controller.as_mut()
         {
            terminate_child(controller);
         }
         return Err(error);
      }
   };
   thread::sleep(Duration::from_millis(100));
   let stop_ready_status = stop_ready_waiter.try_wait().context("checking macOS stop-ready waiter");
   if let Err(error) = stop_ready_status
   {
      terminate_child(&mut ready_waiter);
      terminate_child(&mut stop_ready_waiter);
      terminate_exact_process(executable, pid);
      if let Some(controller) = controller.as_mut()
      {
         terminate_child(controller);
      }
      return Err(error);
   }
   if let Some(status) = stop_ready_status.expect("stop-ready status was checked")
   {
      terminate_child(&mut ready_waiter);
      terminate_exact_process(executable, pid);
      if let Some(controller) = controller.as_mut()
      {
         terminate_child(controller);
      }
      bail!("macOS stop-ready notification waiter exited before measurement with {}", status);
   }
   let result = run_foreground_session(
      config,
      plan,
      session,
      executable,
      &paths,
      pid,
      launch_t0,
      &generation,
      &product.executable_sha256,
      &start_notification,
      &stop_notification,
      &mut stop_ready_waiter,
      &mut ready_waiter,
      stdout,
      stderr,
   );
   terminate_child(&mut stop_ready_waiter);
   if result.is_err()
   {
      terminate_exact_process(executable, pid);
      if let Some(controller) = controller.as_mut()
      {
         terminate_child(controller);
      }
   }
   result?;
   if let Some(controller) = controller.as_mut()
   {
      let status = wait_for_child(controller, Duration::from_secs(30)).context("waiting for measured macOS XCUI input controller dismissal")?;
      if !status.success()
      {
         bail!("measured macOS XCUI input controller exited with {}", status);
      }
   }
   if let Some(result_bundle) = controller_result_bundle
   {
      result_bundle.cleanup()?;
   }
   validate_session_files(plan, session, &paths, &product.executable_sha256, "launched", &trusted_input_preflight)
}

fn preflight_macos_session_trusted_input(config: &MacOsCampaignConfig, plan: &MacOsCampaignPlan, session: &MacOsCampaignSession) -> Result<Vec<MacOsTrustedInputScenarioPreflight>>
{
   if session.pass_role == AppleCampaignPassRole::Correctness || session.pass_role == AppleCampaignPassRole::Launch
   {
      return Ok(Vec::new());
   }
   if let Some(timing) = &session.timing
   {
      if timing.pass_id != session.pass_id
      {
         bail!("trusted-input timing overlay differs from session pass {}", session.pass_id);
      }
   }
   let spec_root = config.plan_path.parent().and_then(Path::parent).context("trusted-input plan is not under the specification plans directory")?;
   let bindings = if plan.plan_resource_path.is_some()
   {
      let bytes = fs::read(&config.plan_path).with_context(|| format!("reading trusted-input plan {}", config.plan_path.display()))?;
      let campaign: AppleCampaignPlanSpec = serde_json::from_slice(&bytes).with_context(|| format!("decoding trusted-input plan {}", config.plan_path.display()))?;
      let timing = session.timing.as_ref().context("measured generic macOS session has no timing scenario selection")?;
      timing.scenarios.iter().map(|timing| {
         let binding = campaign.scenarios.iter().find(|binding| binding.id == timing.scenario_id).with_context(|| format!("trusted-input plan does not bind scenario {}", timing.scenario_id))?;
         let artifact = binding.artifact.clone().with_context(|| format!("trusted-input scenario {} has no artifact", timing.scenario_id))?;
         Ok((timing.scenario_id.clone(), artifact))
      }).collect::<Result<Vec<_>>>()?
   }
   else
   {
      let plan_bytes = fs::read(&config.plan_path).with_context(|| format!("reading trusted-input Apple PR plan {}", config.plan_path.display()))?;
      let apple_plan: ApplePrPlanSpec = serde_json::from_slice(&plan_bytes).with_context(|| format!("decoding trusted-input Apple PR plan {}", config.plan_path.display()))?;
      let acquisition_bytes = fs::read(&config.acquisition_path).with_context(|| format!("reading trusted-input acquisition {}", config.acquisition_path.display()))?;
      let acquisition: ApplePrAcquisitionSpec = serde_json::from_slice(&acquisition_bytes).with_context(|| format!("decoding trusted-input acquisition {}", config.acquisition_path.display()))?;
      let pack = acquisition.packs.iter().find(|pack| pack.id == session.pack_id).with_context(|| format!("trusted-input acquisition has no pack {}", session.pack_id))?;
      pack.ordered_scenario_ids.iter().map(|scenario_id| {
         let binding = apple_plan.scenarios.iter().find(|binding| &binding.id == scenario_id).with_context(|| format!("trusted-input Apple PR plan does not bind scenario {scenario_id}"))?;
         Ok((scenario_id.clone(), binding.artifact.clone()))
      }).collect::<Result<Vec<_>>>()?
   };
   let allow_non_claim_lifecycle_stimuli = session.pass_id == "attribution-time-profiler"
      && session.evidence_role == AppleCampaignEvidenceRole::DescriptiveDiagnostic
      && session.scale_overlay.is_some();
   trusted_input::preflight_macos_trusted_input_scenarios(spec_root, &bindings, session.timing.as_ref(), allow_non_claim_lifecycle_stimuli)
}

fn session_side_has_artifacts(directory: &Path, side: ComparisonSide) -> Result<bool>
{
   if !directory.exists()
   {
      return Ok(false);
   }
   let prefix = format!("{}.", side.as_str());
   for entry in fs::read_dir(directory).with_context(|| format!("reading {}", directory.display()))?
   {
      if entry?.file_name().to_string_lossy().starts_with(&prefix)
      {
         return Ok(true);
      }
   }
   Ok(false)
}

#[allow(clippy::too_many_arguments)]
fn run_foreground_session(
   config: &MacOsCampaignConfig,
   plan: &MacOsCampaignPlan,
   session: &MacOsCampaignSession,
   executable: &Path,
   paths: &SessionPaths,
   pid: u32,
   launch_t0: u64,
   generation: &str,
   executable_sha256: &str,
   start_notification: &str,
   stop_notification: &str,
   stop_ready_waiter: &mut Child,
   ready_waiter: &mut Child,
   stdout: File,
   stderr: File,
) -> Result<()>
{
   let measured_session = session.pass_id != "correctness";
   let energy_requested = session.pass_role == AppleCampaignPassRole::Energy;
   let mut resource_collector = if measured_session && !energy_requested
   {
      Some(MacOsResourceCollector::start(pid, &session.pass_id)?)
   }
   else
   {
      None
   };
   match wait_for_ready_notification(
      ready_waiter,
      executable,
      pid,
      &paths.ready,
      Duration::from_secs(session.max_occupied_seconds),
   )
   {
      Ok(()) => (),
      Err(error) =>
      {
         if paths.failure.is_file()
         {
            let failure = fs::read_to_string(&paths.failure).with_context(|| format!("reading {}", paths.failure.display()))?;
            bail!("macOS comparison app reported failure before readiness: {}", failure.trim());
         }
         return Err(error);
      }
   };
   let ready = read_and_validate_ready(plan, session, paths, generation)?;
   if measured_session
   {
      activate_exact_process(pid)?;
   }
   let time_profiler_requested = session.collector.as_deref() == Some("time-profiler");
   let system_trace_requested = session.collector.as_deref() == Some("system-trace");
   let common_gpu_requested = session.collector.as_deref() == Some("common-gpu");
   let low_frequency_window = matches!(session.pass_role, AppleCampaignPassRole::Idle | AppleCampaignPassRole::Endurance);
   let trace_requested = config.capture_presentation_traces && session.pass_id != "correctness" && !low_frequency_window && !energy_requested && !time_profiler_requested && !system_trace_requested && !common_gpu_requested;
   if trace_requested || time_profiler_requested || system_trace_requested || common_gpu_requested
   {
      reject_translated_session_service_hub(generation)?;
   }
   let mut trace = if trace_requested
   {
      Some(start_presentation_trace(config, session, paths, pid, &stdout, &stderr)?)
   }
   else
   {
      None
   };
   let mut time_profiler_trace = if time_profiler_requested
   {
      Some(start_time_profiler_trace(session, paths, pid, &stdout, &stderr)?)
   }
   else
   {
      None
   };
   let mut system_trace = if system_trace_requested
   {
      Some(start_system_trace(session, paths, pid, &stdout, &stderr)?)
   }
   else
   {
      None
   };
   let mut common_gpu_trace = if common_gpu_requested
   {
      Some(start_common_gpu_trace(session, paths, pid, &stdout, &stderr)?)
   }
   else {None};
   let mut common_gpu_collector = if common_gpu_requested
   {
      Some(MacOsCommonGpuCollector::start(pid)?)
   }
   else {None};
   let energy_meter = if energy_requested
   {
      let path = config.energy_meter_config_path.as_ref().context("macOS energy session has no direct external-meter config")?;
      Some(load_macos_external_meter_config(path)?)
   }
   else {None};
   let mut energy_adapter = if let Some((meter, config_sha256)) = energy_meter.as_ref()
   {
      let calibration_sha256 = macos_energy_calibration_sha256(&meter.calibration)?;
      let request = MacOsEnergyAdapterRequest {
         schema_version: 1,
         protocol: String::from("oxide-direct-external-meter-v1"),
         run_id: plan.run_id.clone(),
         plan_sha256: plan.plan_sha256.clone(),
         generation: String::from(generation),
         pid,
         adapter_sha256: meter.adapter_sha256.clone(),
         config_sha256: config_sha256.clone(),
         calibration_sha256,
         stabilization_seconds: MACOS_ENERGY_STABILIZATION_SECONDS,
         measurement_seconds: MACOS_ENERGY_MEASUREMENT_SECONDS,
         response_path: paths.energy_partial_raw.clone(),
         ready_path: paths.energy_ready.clone(),
         start_notification: String::from(start_notification),
         stop_notification: String::from(stop_notification),
         profiler_allowed: false,
         screen_recording_allowed: false,
         debug_transport_allowed: false,
      };
      let adapter_stdout = File::create(&paths.energy_stdout).with_context(|| format!("creating {}", paths.energy_stdout.display()))?;
      let adapter_stderr = File::create(&paths.energy_stderr).with_context(|| format!("creating {}", paths.energy_stderr.display()))?;
      let mut adapter = MacOsEnergyAdapterProcess::start(meter, &request, &paths.energy_request, adapter_stdout, adapter_stderr)?;
      adapter.wait_ready(Duration::from_secs(20))?;
      Some(adapter)
   }
   else {None};
   let measured = (|| -> Result<()> {
      post_notification(start_notification)?;
      wait_for_session_completion(paths, executable, pid, Duration::from_secs(session.max_occupied_seconds), system_trace.as_mut(), energy_adapter.as_mut())
   })();
   if measured.is_err()
   {
      if let Some(trace) = trace.as_mut()
      {
         terminate_child(trace);
      }
      if let Some(trace) = time_profiler_trace.as_mut()
      {
         terminate_child(trace);
      }
      if let Some(trace) = common_gpu_trace.as_mut()
      {
         terminate_child(trace);
      }
      drop(common_gpu_collector.take());
      drop(system_trace.take());
      drop(energy_adapter.take());
      let _ = post_notification(stop_notification);
      terminate_exact_process(executable, pid);
      drop(resource_collector.take());
      return measured;
   }
   let complete_timestamp = mach_continuous_time();
   let resource_identity = MacOsResourceIdentity {
      plan,
      session,
      generation,
      executable_sha256,
      pid,
      launch_t0,
   };
   let resource_artifact = if let Some(collector) = resource_collector.take()
   {
      collector.finish(&resource_identity, complete_timestamp)
   }
   else
   {
      Ok(MacOsResourceArtifact::not_applicable(&resource_identity, complete_timestamp))
   };
   let resource_sha256 = match resource_artifact.and_then(|artifact| persist_resource_artifact(&artifact, &resource_identity, &paths.resource))
   {
      Ok(resource_sha256) => resource_sha256,
      Err(error) =>
      {
         if let Some(trace) = trace.as_mut()
         {
            terminate_child(trace);
         }
         if let Some(trace) = time_profiler_trace.as_mut()
         {
            terminate_child(trace);
         }
         if let Some(trace) = common_gpu_trace.as_mut()
         {
            terminate_child(trace);
         }
         drop(common_gpu_collector.take());
         drop(system_trace.take());
         drop(energy_adapter.take());
         let _ = post_notification(stop_notification);
         terminate_exact_process(executable, pid);
         return Err(error);
      }
   };
   let common_gpu_samples = if let Some(collector) = common_gpu_collector.take()
   {
      let samples = collector.finish()?;
      durable_json(&samples, &paths.common_gpu_samples)?;
      Some(samples)
   }
   else {None};
   let stopped = (|| -> Result<()> {
      wait_for_stop_ready_notification(stop_ready_waiter, executable, pid, Duration::from_secs(10))?;
      post_notification(stop_notification)?;
      wait_for_exact_process_exit(executable, pid, Duration::from_secs(10))
   })();
   if let Err(error) = stopped
   {
      if let Some(trace) = trace.as_mut()
      {
         terminate_child(trace);
      }
      if let Some(trace) = time_profiler_trace.as_mut()
      {
         terminate_child(trace);
      }
      if let Some(trace) = common_gpu_trace.as_mut()
      {
         terminate_child(trace);
      }
      drop(common_gpu_collector.take());
      drop(system_trace.take());
      drop(energy_adapter.take());
      terminate_exact_process(executable, pid);
      return Err(error);
   }
   if let (Some(adapter), Some((meter, _))) = (energy_adapter.take(), energy_meter.as_ref())
   {
      finish_energy_session(adapter, meter, plan, paths, generation, pid)?;
   }
   let trace_result = if let Some(trace) = trace.as_mut()
   {
      finish_presentation_trace(trace, &paths.trace).and_then(|()| export_and_validate_trace(paths))
   }
   else {Ok(())};
   trace_result?;
   let time_profiler_result = if let Some(trace) = time_profiler_trace.as_mut()
   {
      finish_presentation_trace(trace, &paths.time_profiler_trace).and_then(|()| export_and_reduce_time_profiler_trace(paths, pid))
   }
   else {Ok(())};
   time_profiler_result?;
   let system_trace_result = if let Some(collector) = system_trace.take()
   {
      collector.finish(&macos_system_trace_paths(paths)).map(|_| ())
   }
   else {Ok(())};
   system_trace_result?;
   let common_gpu_result = if let (Some(trace), Some(samples)) = (common_gpu_trace.as_mut(), common_gpu_samples.as_ref())
   {
      finish_presentation_trace(trace, &paths.common_gpu_trace).and_then(|()| export_and_reduce_common_gpu(paths, pid, samples))
   }
   else if common_gpu_trace.is_some() || common_gpu_samples.is_some()
   {
      bail!("macOS common-GPU trace and exact-process sample evidence are incomplete");
   }
   else {Ok(())};
   common_gpu_result?;
   let primary_availability = if session.pass_id == "correctness"
   {
      "not-applicable-correctness-untimed"
   }
   else if trace_requested
   {
      trace::MACOS_PRESENTATION_CORRELATION_AVAILABILITY
   }
   else if time_profiler_requested
   {
      "not-applicable-time-profiler-attribution-pass"
   }
   else if system_trace_requested
   {
      MACOS_SYSTEM_TRACE_AVAILABILITY
   }
   else if common_gpu_requested
   {
      "not-applicable-process-scoped-common-gpu-diagnostic-pass"
   }
   else if energy_requested
   {
      MACOS_ENERGY_AVAILABILITY
   }
   else if low_frequency_window
   {
      "not-applicable-low-frequency-idle-endurance-window"
   }
   else
   {
      "unavailable-common-presentation-trace-not-requested"
   };
   durable_json(
      &ControllerReceipt {
         schema_version: 1,
         run_id: &plan.run_id,
         plan_sha256: &plan.plan_sha256,
         chunk_id: &session.chunk_id,
         pass_id: &session.pass_id,
         pair_index: session.pair_index,
         side: session.side,
         generation,
         executable_sha256,
         pack_id: &session.pack_id,
         resource_sha256: &resource_sha256,
         pid,
         launch_t0,
         ready_timestamp: ready.ready_timestamp,
         complete_timestamp,
         complete: true,
         primary_availability,
      },
      &paths.controller_receipt,
   )
}

fn finish_energy_session(adapter: MacOsEnergyAdapterProcess, meter: &MacOsExternalMeterConfig, plan: &MacOsCampaignPlan, paths: &SessionPaths, generation: &str, pid: u32) -> Result<()>
{
   let raw = adapter.finish(&paths.energy_raw, &meter.calibration, Duration::from_secs(10))?;
   if raw.run_id != plan.run_id
      || raw.plan_sha256 != plan.plan_sha256
      || raw.generation != generation
      || raw.pid != pid
      || raw.adapter_sha256 != meter.adapter_sha256
   {
      bail!("external-meter raw response differs from the active macOS energy session");
   }
   let envelope: SessionEnvelope = read_json(&paths.complete)?;
   let telemetry = fs::read(&paths.telemetry).with_context(|| format!("reading {}", paths.telemetry.display()))?;
   if envelope.telemetry_sha256 != sha256(&telemetry)
      || envelope.telemetry_byte_count != telemetry.len() as u64
      || envelope.timebase_numerator != raw.timebase_numerator
      || envelope.timebase_denominator != raw.timebase_denominator
   {
      bail!("macOS energy telemetry differs from the durable comparator envelope or external-meter timebase");
   }
   let summary = reduce_macos_energy(&raw, &meter.calibration, &telemetry)?;
   durable_json(&summary, &paths.energy_summary)
}

fn start_presentation_trace(config: &MacOsCampaignConfig, session: &MacOsCampaignSession, paths: &SessionPaths, pid: u32, stdout: &File, stderr: &File) -> Result<MacOsPresentationTrace>
{
   start_attached_trace(&config.xctrace_template, "presentation", session, &paths.trace, &paths.trace_scratch, pid, stdout, stderr)
}

fn start_time_profiler_trace(session: &MacOsCampaignSession, paths: &SessionPaths, pid: u32, stdout: &File, stderr: &File) -> Result<MacOsPresentationTrace>
{
   start_attached_trace("Time Profiler", "time-profiler", session, &paths.time_profiler_trace, &paths.time_profiler_scratch, pid, stdout, stderr)
}

fn start_common_gpu_trace(session: &MacOsCampaignSession, paths: &SessionPaths, pid: u32, stdout: &File, stderr: &File) -> Result<MacOsPresentationTrace>
{
   start_attached_trace("Points of Interest", "common-gpu", session, &paths.common_gpu_trace, &paths.common_gpu_scratch, pid, stdout, stderr)
}

fn start_system_trace(session: &MacOsCampaignSession, paths: &SessionPaths, pid: u32, stdout: &File, stderr: &File) -> Result<MacOsSystemTraceCollector>
{
   let started_notification = format!("com.oxide.compare.trace-started.p{}.system-trace.{}.{}", pid, session.side.as_str(), session.pair_index);
   let mut started_waiter = Command::new("/usr/bin/notifyutil")
      .args(["-q", "-1", &started_notification])
      .stdout(Stdio::null())
      .stderr(Stdio::from(stderr.try_clone().context("cloning System Trace started-waiter stderr")?))
      .spawn()
      .context("registering macOS System Trace started notification")?;
   thread::sleep(Duration::from_millis(100));
   if let Some(status) = started_waiter.try_wait().context("checking System Trace started waiter before launch")?
   {
      bail!("macOS System Trace started waiter exited before recorder launch with {}", status);
   }
   let system_paths = macos_system_trace_paths(paths);
   let mut collector = match MacOsSystemTraceCollector::start(
      &system_paths,
      pid,
      session.max_occupied_seconds,
      &started_notification,
      stdout,
      stderr,
   )
   {
      Ok(collector) => collector,
      Err(error) =>
      {
         terminate_child(&mut started_waiter);
         return Err(error);
      }
   };
   if let Err(error) = collector.wait_for_started(&mut started_waiter, Duration::from_secs(20))
   {
      terminate_child(&mut started_waiter);
      return Err(error);
   }
   Ok(collector)
}

fn macos_system_trace_paths(paths: &SessionPaths) -> MacOsSystemTracePaths<'_>
{
   MacOsSystemTracePaths {
      trace: &paths.system_trace,
      scratch: &paths.system_trace_scratch,
      toc: &paths.system_trace_toc,
      signposts: &paths.system_trace_signposts,
      thread_info: &paths.system_trace_thread_info,
      thread_state: &paths.system_trace_thread_state,
      context_switch: &paths.system_trace_context_switch,
      summary: &paths.system_trace_artifact,
   }
}

fn start_attached_trace(template: &str, label: &str, session: &MacOsCampaignSession, trace_path: &Path, scratch_path: &Path, pid: u32, stdout: &File, stderr: &File) -> Result<MacOsPresentationTrace>
{
   if trace_path.exists()
   {
      bail!("refusing to overwrite {} trace {}", label, trace_path.display());
   }
   let started_notification = format!(
      "com.oxide.compare.trace-started.p{}.{}.{}",
      pid,
      session.side.as_str(),
      session.pair_index,
   );
   let mut started_waiter = Command::new("/usr/bin/notifyutil")
      .args(["-q", "-1", &started_notification])
      .stdout(Stdio::null())
      .stderr(Stdio::from(stderr.try_clone().context("cloning comparison stderr for trace-started waiter")?))
      .spawn()
      .context("registering macOS trace-started notification")?;
   let trace_seconds = session.max_occupied_seconds.max(1);
   let scratch = match MacOsTraceScratch::create(scratch_path)
   {
      Ok(scratch) => scratch,
      Err(error) =>
      {
         terminate_child(&mut started_waiter);
         return Err(error);
      }
   };
   let mut command = native_xcrun_command();
   command.env("TMPDIR", &scratch.path).env("TMP", &scratch.path).env("TEMP", &scratch.path);
   let trace_result = command
      .args([
         "xctrace", "record",
         "--template", template,
         "--instrument", "os_signpost",
         "--notify-tracing-started", &started_notification,
         "--output",
      ])
      .arg(trace_path)
      .args(["--time-limit", &format!("{}s", trace_seconds), "--no-prompt", "--attach", &pid.to_string()])
      .stdout(Stdio::from(stdout.try_clone().context("cloning comparison stdout for xctrace")?))
      .stderr(Stdio::from(stderr.try_clone().context("cloning comparison stderr for xctrace")?))
      .spawn()
      .with_context(|| format!("attaching macOS {} trace to exact process", label));
   let trace = match trace_result
   {
      Ok(trace) => trace,
      Err(error) =>
      {
         terminate_child(&mut started_waiter);
         return Err(error);
      }
   };
   let mut trace = MacOsPresentationTrace::new(trace, scratch);
   if let Err(error) = wait_for_presentation_trace_start(&mut trace, trace_path, &mut started_waiter, Duration::from_secs(20))
   {
      terminate_child(&mut trace);
      terminate_child(&mut started_waiter);
      return Err(error);
   }
   Ok(trace)
}

fn wait_for_presentation_trace_start(trace: &mut MacOsPresentationTrace, trace_path: &Path, started_waiter: &mut Child, timeout: Duration) -> Result<()>
{
   let deadline = Instant::now().checked_add(timeout).context("trace-started timeout overflow")?;
   loop
   {
      if let Some(status) = trace.try_wait().context("checking attached macOS presentation trace")?
      {
         bail!("attached macOS presentation trace exited before its tracing-started notification with {}", status);
      }
      if trace.working_set_bytes(trace_path)? > MACOS_TRACE_WORKING_SET_LIMIT_BYTES
      {
         bail!("macOS presentation trace exceeded the 512 MiB working-set limit before tracing started");
      }
      if let Some(status) = started_waiter.try_wait().context("checking macOS trace-started waiter")?
      {
         if !status.success()
         {
            bail!("macOS trace-started notification waiter exited with {}", status);
         }
         return Ok(());
      }
      if Instant::now() >= deadline
      {
         bail!("macOS presentation trace did not report tracing started within {} seconds", timeout.as_secs());
      }
      thread::sleep(Duration::from_millis(50));
   }
}

fn finish_presentation_trace(trace: &mut MacOsPresentationTrace, trace_path: &Path) -> Result<()>
{
   interrupt_presentation_trace(trace)?;
   let deadline = Instant::now() + Duration::from_secs(60);
   loop
   {
      if let Some(status) = trace.try_wait().context("polling macOS presentation trace finalization")?
      {
         if !status.success()
         {
            bail!("macOS presentation trace exited with {}", status);
         }
         validate_macos_presentation_trace_bundle(trace_path)?;
         trace.cleanup_scratch()?;
         return Ok(());
      }
      if trace.working_set_bytes(trace_path)? > MACOS_TRACE_WORKING_SET_LIMIT_BYTES
      {
         terminate_child(trace);
         bail!("macOS presentation trace exceeded the 512 MiB bundle-plus-scratch limit while finalizing");
      }
      if Instant::now() >= deadline
      {
         terminate_child(trace);
         bail!("macOS presentation trace did not finalize within 60 seconds");
      }
      thread::sleep(Duration::from_millis(100));
   }
}

fn run_child(mut command: Command, stdout: File, stderr: File, timeout_seconds: u64) -> Result<ExitStatus>
{
   let mut child = command.stdout(Stdio::from(stdout)).stderr(Stdio::from(stderr)).spawn().context("launching macOS comparison session")?;
   wait_for_child(&mut child, Duration::from_secs(timeout_seconds))
}

fn wait_for_child(child: &mut Child, timeout: Duration) -> Result<ExitStatus>
{
   let deadline = Instant::now().checked_add(timeout).context("comparison timeout overflow")?;
   loop
   {
      if let Some(status) = child.try_wait().context("polling macOS comparison session")?
      {
         return Ok(status);
      }
      if Instant::now() >= deadline
      {
         child.kill().context("terminating timed-out macOS comparison session")?;
         let _ = child.wait();
         bail!("macOS comparison session exceeded {} seconds", timeout.as_secs());
      }
      thread::sleep(Duration::from_millis(25));
   }
}

fn wait_for_ready_notification(child: &mut Child, executable: &Path, pid: u32, ready_path: &Path, timeout: Duration) -> Result<()>
{
   let outcome = (|| -> Result<()> {
      let deadline = Instant::now().checked_add(timeout).context("comparison readiness timeout overflow")?;
      loop
      {
         if ready_path.is_file()
         {
            terminate_child(child);
            return Ok(());
         }
         if let Some(status) = child.try_wait().context("polling macOS ready notification waiter")?
         {
            if !status.success()
            {
               bail!("macOS ready notification waiter exited with {}", status);
            }
            return Ok(());
         }
         match exact_process_id(executable)?
         {
            Some(observed) if observed == pid => (),
            Some(observed) => bail!("comparison process identity changed from PID {} to PID {} before readiness", pid, observed),
            None => bail!("macOS comparison PID {} exited before readiness", pid),
         }
         if Instant::now() >= deadline
         {
            bail!("macOS comparison readiness exceeded {} seconds", timeout.as_secs());
         }
         thread::sleep(Duration::from_millis(50));
      }
   })();
   if outcome.is_err()
   {
      terminate_child(child);
   }
   outcome
}

fn wait_for_stop_ready_notification(child: &mut Child, executable: &Path, pid: u32, timeout: Duration) -> Result<()>
{
   let deadline = Instant::now().checked_add(timeout).context("comparison stop readiness timeout overflow")?;
   loop
   {
      if let Some(status) = child.try_wait().context("polling macOS stop-ready notification waiter")?
      {
         if !status.success()
         {
            bail!("macOS stop-ready notification waiter exited with {}", status);
         }
         return Ok(());
      }
      match exact_process_id(executable)?
      {
         Some(observed) if observed == pid => (),
         Some(observed) => bail!("comparison process identity changed from PID {} to PID {} before stop readiness", pid, observed),
         None => bail!("macOS comparison PID {} exited before stop readiness", pid),
      }
      if Instant::now() >= deadline
      {
         terminate_child(child);
         bail!("macOS comparison stop readiness exceeded {} seconds", timeout.as_secs());
      }
      thread::sleep(Duration::from_millis(25));
   }
}

fn exact_process_id(executable: &Path) -> Result<Option<u32>>
{
   let output = Command::new("/bin/ps").args(["-axo", "pid=,command="]).output().context("listing macOS comparison processes")?;
   if !output.status.success()
   {
      bail!("listing macOS comparison processes failed with {}", output.status);
   }
   let output = String::from_utf8(output.stdout).context("ps emitted non-UTF-8 process output")?;
   macos_process_id_from_ps(executable, &output)
}

const DVT_SERVICE_HUB: &str = "/Applications/Xcode.app/Contents/SharedFrameworks/DVTInstrumentsFoundation.framework/Resources/DTServiceHub";
const PROCESS_TRANSLATED_FLAG: u32 = 0x0002_0000;

struct MacOsProcessRow
{
   pid: u32,
   parent_pid: u32,
   flags: u32,
   command: String,
}

fn macos_process_rows(output: &str) -> Result<Vec<MacOsProcessRow>>
{
   output.lines().filter(|line| !line.trim().is_empty()).map(|line| {
      let mut fields = line.split_whitespace();
      let pid = fields.next().context("ps row has no process ID")?.parse::<u32>().context("ps row has an invalid process ID")?;
      let parent_pid = fields.next().context("ps row has no parent process ID")?.parse::<u32>().context("ps row has an invalid parent process ID")?;
      let flags = u32::from_str_radix(fields.next().context("ps row has no process flags")?, 16).context("ps row has invalid hexadecimal process flags")?;
      let command = fields.collect::<Vec<_>>().join(" ");
      Ok(MacOsProcessRow {pid, parent_pid, flags, command})
   }).collect()
}

fn reject_translated_session_service_hub(generation: &str) -> Result<()>
{
   let output = Command::new("/bin/ps").args(["-axo", "pid=,ppid=,flags=,command="]).output().context("listing macOS Instruments service processes")?;
   ensure!(output.status.success(), "listing macOS Instruments service processes failed with {}", output.status);
   let output = String::from_utf8(output.stdout).context("ps emitted non-UTF-8 Instruments service output")?;
   let rows = macos_process_rows(&output)?;
   let translated = rows.iter().filter(|row| row.command == DVT_SERVICE_HUB && row.flags & PROCESS_TRANSLATED_FLAG != 0).collect::<Vec<_>>();
   ensure!(translated.len() <= 1, "multiple translated DTServiceHub processes are running");
   let Some(service) = translated.first() else {return Ok(())};
   let parent = rows.iter().find(|row| row.pid == service.parent_pid).context("translated DTServiceHub has no visible parent process")?;
   let expected_xctestrun = format!(".oxide-controller-{}.", generation);
   ensure!(
      parent.command.starts_with("/Applications/Xcode.app/Contents/Developer/usr/bin/xcodebuild test-without-building ")
         && parent.command.contains(&expected_xctestrun),
      "translated DTServiceHub {} belongs to unrelated parent {}; refusing to disturb it",
      service.pid,
      service.parent_pid,
   );
   bail!("comparison controller spawned translated DTServiceHub {}; refusing to start xctrace until the controller is native", service.pid)
}

fn wait_for_exact_process(executable: &Path, timeout: Duration) -> Result<u32>
{
   let deadline = Instant::now().checked_add(timeout).context("process discovery timeout overflow")?;
   loop
   {
      if let Some(pid) = exact_process_id(executable)?
      {
         return Ok(pid);
      }
      if Instant::now() >= deadline
      {
         bail!("exact comparison process did not launch within {} seconds: {}", timeout.as_secs(), executable.display());
      }
      thread::sleep(Duration::from_millis(25));
   }
}

fn wait_for_exact_process_exit(executable: &Path, pid: u32, timeout: Duration) -> Result<()>
{
   let deadline = Instant::now().checked_add(timeout).context("process exit timeout overflow")?;
   loop
   {
      match exact_process_id(executable)?
      {
         None => return Ok(()),
         Some(observed) if observed == pid => (),
         Some(observed) => bail!("comparison process identity changed from PID {} to PID {}", pid, observed),
      }
      if Instant::now() >= deadline
      {
         bail!("macOS comparison process {} exceeded {} seconds", pid, timeout.as_secs());
      }
      thread::sleep(Duration::from_millis(25));
   }
}

fn wait_for_session_completion(paths: &SessionPaths, executable: &Path, pid: u32, timeout: Duration, mut system_trace: Option<&mut MacOsSystemTraceCollector>, mut energy_adapter: Option<&mut MacOsEnergyAdapterProcess>) -> Result<()>
{
   let deadline = Instant::now().checked_add(timeout).context("session completion timeout overflow")?;
   loop
   {
      if let Some(collector) = system_trace.as_deref_mut()
      {
         collector.enforce()?;
      }
      if let Some(adapter) = energy_adapter.as_deref_mut()
      {
         adapter.enforce_output_limit()?;
      }
      if paths.failure.is_file()
      {
         let failure = fs::read_to_string(&paths.failure).with_context(|| format!("reading {}", paths.failure.display()))?;
         bail!("macOS comparison app reported failure: {}", failure.trim());
      }
      if session_trace_working_set_bytes(paths)? > MACOS_TRACE_WORKING_SET_LIMIT_BYTES
      {
         bail!("macOS session trace exceeded the 512 MiB aggregate bundle-plus-scratch limit");
      }
      if paths.complete.is_file() && paths.acknowledgement.is_file()
      {
         return Ok(());
      }
      if exact_process_id(executable)? != Some(pid)
      {
         if paths.failure.is_file()
         {
            let failure = fs::read_to_string(&paths.failure).with_context(|| format!("reading {}", paths.failure.display()))?;
            bail!("macOS comparison app reported failure: {}", failure.trim());
         }
         bail!("macOS comparison PID {} exited before committing its durable artifact", pid);
      }
      if Instant::now() >= deadline
      {
         bail!("macOS comparison session did not commit within {} seconds", timeout.as_secs());
      }
      thread::sleep(Duration::from_millis(25));
   }
}

fn session_trace_working_set_bytes(paths: &SessionPaths) -> Result<u64>
{
   let presentation = trace_working_set_bytes(&paths.trace, &paths.trace_scratch)?;
   let time_profiler = trace_working_set_bytes(&paths.time_profiler_trace, &paths.time_profiler_scratch)?;
   let common_gpu = trace_working_set_bytes(&paths.common_gpu_trace, &paths.common_gpu_scratch)?;
   presentation.checked_add(time_profiler)
      .and_then(|total| total.checked_add(common_gpu))
      .context("macOS session aggregate trace working-set byte count overflow")
}

fn activate_exact_process(pid: u32) -> Result<()>
{
   let process = format!("first process whose unix id is {}", pid);
   let activate = format!("tell application \"System Events\" to set frontmost of {} to true", process);
   let verify = format!("tell application \"System Events\" to get frontmost of {}", process);
   let deadline = Instant::now() + Duration::from_secs(10);
   loop
   {
      let status = Command::new("/usr/bin/osascript").args(["-e", &activate]).status().context("activating exact macOS comparison process")?;
      if !status.success()
      {
         bail!("System Events could not activate exact comparison PID {}: {}", pid, status);
      }
      let output = Command::new("/usr/bin/osascript").args(["-e", &verify]).output().context("verifying exact macOS comparison foreground state")?;
      if output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "true"
      {
         return Ok(());
      }
      if Instant::now() >= deadline
      {
         bail!("exact comparison PID {} is not foreground after activation", pid);
      }
      thread::sleep(Duration::from_millis(50));
   }
}

fn post_notification(name: &str) -> Result<()>
{
   let status = Command::new("/usr/bin/notifyutil").args(["-p", name]).status().with_context(|| format!("posting Darwin notification {}", name))?;
   if !status.success()
   {
      bail!("posting Darwin notification {} failed with {}", name, status);
   }
   Ok(())
}

fn read_and_validate_ready(plan: &MacOsCampaignPlan, session: &MacOsCampaignSession, paths: &SessionPaths, generation: &str) -> Result<SessionReady>
{
   let bytes = fs::read(&paths.ready).with_context(|| format!("reading {}", paths.ready.display()))?;
   let ready: SessionReady = serde_json::from_slice(&bytes).with_context(|| format!("decoding {}", paths.ready.display()))?;
   if ready.schema_version != 1
      || ready.run_id != plan.run_id
      || ready.plan_sha256 != plan.plan_sha256
      || ready.chunk_id != session.chunk_id
      || ready.pass_id != session.pass_id
      || ready.pair_index != session.pair_index
      || ready.side != session.side
      || ready.generation != generation
      || ready.pack_id != session.pack_id
      || !ready.durable
   {
      bail!("macOS comparison ready artifact identity differs from its run plan");
   }
   Ok(ready)
}

struct MacOsTraceScratch
{
   path: PathBuf,
   spill_root: PathBuf,
}

impl MacOsTraceScratch
{
   fn create(path: &Path) -> Result<Self>
   {
      Self::create_with_spill_root(path, &std::env::temp_dir())
   }

   fn create_with_spill_root(path: &Path, spill_root: &Path) -> Result<Self>
   {
      if path.exists()
      {
         bail!("refusing to reuse macOS trace scratch directory {}", path.display());
      }
      let spills = macos_instruments_spill_files(spill_root)?;
      ensure!(spills.is_empty(), "refusing to start macOS trace with {} unowned root Instruments scratch files under {}", spills.len(), spill_root.display());
      fs::create_dir(path).with_context(|| format!("creating isolated macOS trace scratch directory {}", path.display()))?;
      Ok(Self {path: path.to_path_buf(), spill_root: spill_root.to_path_buf()})
   }

   fn bytes(&self) -> Result<u64>
   {
      let isolated = trace_bundle_bytes(&self.path)?;
      let spilled = macos_instruments_spill_bytes(&self.spill_root)?;
      isolated.checked_add(spilled).context("macOS trace scratch byte count overflow")
   }

   fn cleanup(&mut self) -> Result<()>
   {
      for spill in macos_instruments_spill_files(&self.spill_root)?
      {
         fs::remove_file(&spill).with_context(|| format!("removing macOS root Instruments scratch file {}", spill.display()))?;
      }
      if self.path.exists()
      {
         fs::remove_dir_all(&self.path).with_context(|| format!("removing isolated macOS trace scratch directory {}", self.path.display()))?;
      }
      Ok(())
   }
}

fn macos_instruments_spill_files(root: &Path) -> Result<Vec<PathBuf>>
{
   let mut files = Vec::new();
   for entry in fs::read_dir(root).with_context(|| format!("reading macOS Instruments spill root {}", root.display()))?
   {
      let entry = entry.with_context(|| format!("reading entry under macOS Instruments spill root {}", root.display()))?;
      let name = entry.file_name();
      let name = name.to_string_lossy();
      if !name.starts_with("instruments") || !name.ends_with(".ktrace")
      {
         continue;
      }
      let file_type = entry.file_type().with_context(|| format!("reading macOS Instruments spill file type {}", entry.path().display()))?;
      ensure!(file_type.is_file(), "macOS Instruments spill path is not a file: {}", entry.path().display());
      files.push(entry.path());
   }
   files.sort();
   Ok(files)
}

fn macos_instruments_spill_bytes(root: &Path) -> Result<u64>
{
   let mut bytes = 0_u64;
   for path in macos_instruments_spill_files(root)?
   {
      let size = fs::metadata(&path).with_context(|| format!("reading macOS Instruments spill size {}", path.display()))?.len();
      bytes = bytes.checked_add(size).context("macOS Instruments spill byte count overflow")?;
   }
   Ok(bytes)
}

impl Drop for MacOsTraceScratch
{
   fn drop(&mut self)
   {
      let _ = self.cleanup();
   }
}

struct MacOsPresentationTrace
{
   child: Child,
   scratch: MacOsTraceScratch,
}

struct MacOsWakeAssertion
{
   child: Child,
}

impl MacOsWakeAssertion
{
   fn acquire() -> Result<Self>
   {
      let pid = std::process::id().to_string();
      let mut child = Command::new("/usr/bin/caffeinate")
         .args(["-d", "-i", "-w", &pid])
         .stdout(Stdio::null())
         .stderr(Stdio::null())
         .spawn()
         .context("acquiring the bounded macOS comparison wake assertion")?;
      if let Some(status) = child.try_wait().context("checking the macOS comparison wake assertion")?
      {
         bail!("macOS comparison wake assertion exited immediately with {}", status);
      }
      Ok(Self {child})
   }
}

impl Drop for MacOsWakeAssertion
{
   fn drop(&mut self)
   {
      terminate_child(&mut self.child);
   }
}

impl MacOsPresentationTrace
{
   fn new(child: Child, scratch: MacOsTraceScratch) -> Self
   {
      Self {child, scratch}
   }

   fn working_set_bytes(&self, trace_path: &Path) -> Result<u64>
   {
      let trace_bytes = if trace_path.exists() {trace_bundle_bytes(trace_path)?} else {0};
      trace_bytes.checked_add(self.scratch.bytes()?).context("macOS trace working-set byte count overflow")
   }

   fn cleanup_scratch(&mut self) -> Result<()>
   {
      self.scratch.cleanup()
   }
}

impl Deref for MacOsPresentationTrace
{
   type Target = Child;

   fn deref(&self) -> &Self::Target
   {
      &self.child
   }
}

impl DerefMut for MacOsPresentationTrace
{
   fn deref_mut(&mut self) -> &mut Self::Target
   {
      &mut self.child
   }
}

impl Drop for MacOsPresentationTrace
{
   fn drop(&mut self)
   {
      terminate_child(&mut self.child);
   }
}

fn terminate_child(child: &mut Child)
{
   if child.try_wait().ok().flatten().is_none()
   {
      let _ = child.kill();
      let _ = child.wait();
   }
}

fn terminate_exact_process(executable: &Path, pid: u32)
{
   if exact_process_id(executable).ok().flatten() != Some(pid)
   {
      return;
   }
   let _ = Command::new("/bin/kill").args(["-TERM", &pid.to_string()]).status();
   let deadline = Instant::now() + Duration::from_secs(3);
   while Instant::now() < deadline
   {
      if exact_process_id(executable).ok().flatten() != Some(pid)
      {
         return;
      }
      thread::sleep(Duration::from_millis(25));
   }
   if exact_process_id(executable).ok().flatten() == Some(pid)
   {
      let _ = Command::new("/bin/kill").args(["-KILL", &pid.to_string()]).status();
   }
}

#[cfg(target_os = "macos")]
fn mach_continuous_time() -> u64
{
   unsafe extern "C"
   {
      fn mach_continuous_time() -> u64;
   }
   // SAFETY: mach_continuous_time takes no arguments and has no caller-owned
   // memory or lifetime requirements.
   unsafe {mach_continuous_time()}
}

#[cfg(target_os = "macos")]
fn mach_ticks_for_nanoseconds(nanoseconds: u64) -> Result<u64>
{
   #[repr(C)]
   struct MachTimebaseInfo
   {
      numer: u32,
      denom: u32,
   }
   unsafe extern "C"
   {
      fn mach_timebase_info(info: *mut MachTimebaseInfo) -> i32;
   }
   let mut info = MachTimebaseInfo {numer: 0, denom: 0};
   // SAFETY: `info` is a writable, correctly sized timebase-info structure and
   // the system call does not retain its address.
   let status = unsafe {mach_timebase_info(&mut info)};
   if status != 0 || info.numer == 0 || info.denom == 0
   {
      bail!("mach_timebase_info failed with {}", status);
   }
   let scaled = u128::from(nanoseconds).checked_mul(u128::from(info.denom)).context("mach tick conversion overflow")? / u128::from(info.numer);
   u64::try_from(scaled).context("mach tick conversion exceeds u64")
}

#[cfg(not(target_os = "macos"))]
fn mach_continuous_time() -> u64
{
   0
}

#[cfg(not(target_os = "macos"))]
fn mach_ticks_for_nanoseconds(_nanoseconds: u64) -> Result<u64>
{
   bail!("mach clock conversion is available only on macOS")
}

struct SessionPaths
{
   directory: PathBuf,
   ready: PathBuf,
   complete: PathBuf,
   acknowledgement: PathBuf,
   failure: PathBuf,
   controller_receipt: PathBuf,
   resource: PathBuf,
   stdout: PathBuf,
   stderr: PathBuf,
   trace: PathBuf,
   trace_scratch: PathBuf,
   trace_toc: PathBuf,
   trace_signposts: PathBuf,
   trace_updates: PathBuf,
   trace_frame_lifetimes: PathBuf,
   trace_correlation: PathBuf,
   time_profiler_trace: PathBuf,
   time_profiler_scratch: PathBuf,
   time_profiler_toc: PathBuf,
   time_profiler_signposts: PathBuf,
   time_profiler_samples: PathBuf,
   time_profiler_artifact: PathBuf,
   system_trace: PathBuf,
   system_trace_scratch: PathBuf,
   system_trace_toc: PathBuf,
   system_trace_signposts: PathBuf,
   system_trace_thread_info: PathBuf,
   system_trace_thread_state: PathBuf,
   system_trace_context_switch: PathBuf,
   system_trace_artifact: PathBuf,
   common_gpu_trace: PathBuf,
   common_gpu_scratch: PathBuf,
   common_gpu_toc: PathBuf,
   common_gpu_signposts: PathBuf,
   common_gpu_samples: PathBuf,
   common_gpu_artifact: PathBuf,
   telemetry: PathBuf,
   telemetry_coverage: PathBuf,
   surface_receipt: PathBuf,
   energy_request: PathBuf,
   energy_ready: PathBuf,
   energy_partial_raw: PathBuf,
   energy_raw: PathBuf,
   energy_summary: PathBuf,
   energy_stdout: PathBuf,
   energy_stderr: PathBuf,
   launch_application_did_finish: PathBuf,
   launch_first_complete_ui: PathBuf,
   launch_ready: PathBuf,
   launch_complete: PathBuf,
   launch_acknowledgement: PathBuf,
   launch_ui_controller_start: PathBuf,
   launch_ui_controller: PathBuf,
   launch_evidence: PathBuf,
   cache_primer_root: PathBuf,
   cache_primer_receipt: PathBuf,
}

fn session_paths(output_root: &Path, plan: &MacOsCampaignPlan, session: &MacOsCampaignSession) -> SessionPaths
{
   let directory = output_root
      .join("Runs")
      .join(&plan.run_id)
      .join(&session.chunk_id)
      .join(&session.pass_id)
      .join(&session.pack_id)
      .join(session.pair_index.to_string());
   let prefix = session.side.as_str();
   SessionPaths {
      ready: directory.join(format!("{}.ready.json", prefix)),
      complete: directory.join(format!("{}.complete.json", prefix)),
      acknowledgement: directory.join(format!("{}.complete.ack.json", prefix)),
      failure: directory.join(format!("{}.failure.txt", prefix)),
      controller_receipt: directory.join(format!("{}.controller.json", prefix)),
      resource: directory.join(format!("{}.resources.json", prefix)),
      stdout: directory.join(format!("{}.stdout.log", prefix)),
      stderr: directory.join(format!("{}.stderr.log", prefix)),
      trace: directory.join(format!("{}.presentation.trace", prefix)),
      trace_scratch: directory.join(format!("{}.xctrace-tmp", prefix)),
      trace_toc: directory.join(format!("{}.presentation.toc.xml", prefix)),
      trace_signposts: directory.join(format!("{}.presentation.signposts.xml", prefix)),
      trace_updates: directory.join(format!("{}.presentation.updates.xml", prefix)),
      trace_frame_lifetimes: directory.join(format!("{}.presentation.frame-lifetimes.xml", prefix)),
      trace_correlation: directory.join(format!("{}.presentation.correlation.json", prefix)),
      time_profiler_trace: directory.join(format!("{}.time-profiler.trace", prefix)),
      time_profiler_scratch: directory.join(format!("{}.time-profiler.xctrace-tmp", prefix)),
      time_profiler_toc: directory.join(format!("{}.time-profiler.toc.xml", prefix)),
      time_profiler_signposts: directory.join(format!("{}.time-profiler.signposts.xml", prefix)),
      time_profiler_samples: directory.join(format!("{}.time-profiler.samples.xml", prefix)),
      time_profiler_artifact: directory.join(format!("{}.time-profiler.json", prefix)),
      system_trace: directory.join(format!("{}.system-trace.trace", prefix)),
      system_trace_scratch: directory.join(format!("{}.system-trace.xctrace-tmp", prefix)),
      system_trace_toc: directory.join(format!("{}.system-trace.toc.xml", prefix)),
      system_trace_signposts: directory.join(format!("{}.system-trace.signposts.xml", prefix)),
      system_trace_thread_info: directory.join(format!("{}.system-trace.thread-info.xml", prefix)),
      system_trace_thread_state: directory.join(format!("{}.system-trace.thread-state.xml", prefix)),
      system_trace_context_switch: directory.join(format!("{}.system-trace.context-switch.xml", prefix)),
      system_trace_artifact: directory.join(format!("{}.system-trace.json", prefix)),
      common_gpu_trace: directory.join(format!("{}.common-gpu.trace", prefix)),
      common_gpu_scratch: directory.join(format!("{}.common-gpu.xctrace-tmp", prefix)),
      common_gpu_toc: directory.join(format!("{}.common-gpu.toc.xml", prefix)),
      common_gpu_signposts: directory.join(format!("{}.common-gpu.signposts.xml", prefix)),
      common_gpu_samples: directory.join(format!("{}.common-gpu.samples.json", prefix)),
      common_gpu_artifact: directory.join(format!("{}.common-gpu.json", prefix)),
      telemetry: directory.join(format!("{}.telemetry.bin", prefix)),
      telemetry_coverage: directory.join(format!("{}.telemetry.coverage.json", prefix)),
      surface_receipt: directory.join(format!("{}.surface.json", prefix)),
      energy_request: directory.join(format!("{}.energy.request.json", prefix)),
      energy_ready: directory.join(format!("{}.energy.ready.json", prefix)),
      energy_partial_raw: directory.join(format!("{}.energy.raw.partial.json", prefix)),
      energy_raw: directory.join(format!("{}.energy.raw.json", prefix)),
      energy_summary: directory.join(format!("{}.energy.summary.json", prefix)),
      energy_stdout: directory.join(format!("{}.energy.stdout.log", prefix)),
      energy_stderr: directory.join(format!("{}.energy.stderr.log", prefix)),
      launch_application_did_finish: directory.join(format!("{}.launch.application-did-finish.json", prefix)),
      launch_first_complete_ui: directory.join(format!("{}.launch.first-complete-ui.json", prefix)),
      launch_ready: directory.join(format!("{}.launch.ready.json", prefix)),
      launch_complete: directory.join(format!("{}.launch.complete.json", prefix)),
      launch_acknowledgement: directory.join(format!("{}.launch.complete.ack.json", prefix)),
      launch_ui_controller_start: directory.join(format!("{}.launch.controller-start.json", prefix)),
      launch_ui_controller: directory.join(format!("{}.launch.controller.json", prefix)),
      launch_evidence: directory.join(format!("{}.launch.evidence.json", prefix)),
      cache_primer_root: directory.join(format!("{}.cache-primer", prefix)),
      cache_primer_receipt: directory.join(format!("{}.cache-primer.json", prefix)),
      directory,
   }
}

fn export_and_validate_trace(paths: &SessionPaths) -> Result<()>
{
   validate_macos_presentation_trace_bundle(&paths.trace)?;
   export_trace_xml(&paths.trace, &["--toc"], &paths.trace_toc)?;
   export_trace_xml(
      &paths.trace,
      &["--xpath", "/trace-toc/run[@number=\"1\"]/data/table[@schema=\"os-signpost\"]"],
      &paths.trace_signposts,
   )?;
   export_trace_xml(
      &paths.trace,
      &["--xpath", "/trace-toc/run[@number=\"1\"]/data/table[@schema=\"hitches-updates\"]"],
      &paths.trace_updates,
   )?;
   export_trace_xml(
      &paths.trace,
      &["--xpath", "/trace-toc/run[@number=\"1\"]/data/table[@schema=\"hitches-frame-lifetimes\"]"],
      &paths.trace_frame_lifetimes,
   )?;
   let toc = fs::read_to_string(&paths.trace_toc).with_context(|| format!("reading {}", paths.trace_toc.display()))?;
   let signposts = fs::read_to_string(&paths.trace_signposts).with_context(|| format!("reading {}", paths.trace_signposts.display()))?;
   let updates = fs::read_to_string(&paths.trace_updates).with_context(|| format!("reading {}", paths.trace_updates.display()))?;
   let frame_lifetimes = fs::read_to_string(&paths.trace_frame_lifetimes).with_context(|| format!("reading {}", paths.trace_frame_lifetimes.display()))?;
   validate_macos_trace_exports(&toc, &signposts)?;
   let correlation = correlate_macos_presentation_trace(&signposts, &updates, &frame_lifetimes)?;
   durable_json(&correlation, &paths.trace_correlation)
}

fn export_and_reduce_time_profiler_trace(paths: &SessionPaths, exact_pid: u32) -> Result<()>
{
   validate_macos_presentation_trace_bundle(&paths.time_profiler_trace)?;
   export_trace_xml(&paths.time_profiler_trace, &["--toc"], &paths.time_profiler_toc)?;
   export_trace_xml(
      &paths.time_profiler_trace,
      &["--xpath", "/trace-toc/run[@number=\"1\"]/data/table[@schema=\"os-signpost\"]"],
      &paths.time_profiler_signposts,
   )?;
   export_trace_xml(
      &paths.time_profiler_trace,
      &["--xpath", "/trace-toc/run[@number=\"1\"]/data/table[@schema=\"time-profile\"]"],
      &paths.time_profiler_samples,
   )?;
   let toc = fs::read_to_string(&paths.time_profiler_toc).with_context(|| format!("reading {}", paths.time_profiler_toc.display()))?;
   if !toc.contains("time-profile") || !toc.contains("os-signpost")
   {
      bail!("macOS Time Profiler TOC omits the time-profile or os-signpost schema");
   }
   let signposts = fs::read_to_string(&paths.time_profiler_signposts).with_context(|| format!("reading {}", paths.time_profiler_signposts.display()))?;
   let samples = fs::read_to_string(&paths.time_profiler_samples).with_context(|| format!("reading {}", paths.time_profiler_samples.display()))?;
   let artifact = reduce_macos_time_profiler_trace(&samples, &signposts, exact_pid)?;
   durable_json(&artifact, &paths.time_profiler_artifact)
}

fn export_and_reduce_common_gpu(paths: &SessionPaths, exact_pid: u32, samples: &MacOsCommonGpuSamples) -> Result<()>
{
   validate_macos_presentation_trace_bundle(&paths.common_gpu_trace)?;
   export_trace_xml(&paths.common_gpu_trace, &["--toc"], &paths.common_gpu_toc)?;
   export_trace_xml(
      &paths.common_gpu_trace,
      &["--xpath", "/trace-toc/run[@number=\"1\"]/data/table[@schema=\"os-signpost\"]"],
      &paths.common_gpu_signposts,
   )?;
   let toc = fs::read_to_string(&paths.common_gpu_toc).with_context(|| format!("reading {}", paths.common_gpu_toc.display()))?;
   if !toc.contains("os-signpost")
   {
      bail!("macOS common-GPU trace TOC omits os-signpost");
   }
   let signposts = fs::read_to_string(&paths.common_gpu_signposts).with_context(|| format!("reading {}", paths.common_gpu_signposts.display()))?;
   let summary = reduce_macos_common_gpu(samples, &signposts, exact_pid)?;
   if summary.comparison_eligible
   {
      bail!("process-scoped macOS common-GPU evidence must never become comparison eligible");
   }
   durable_json(&summary, &paths.common_gpu_artifact)
}

fn export_trace_xml(trace: &Path, selector: &[&str], output: &Path) -> Result<()>
{
   if output.exists()
   {
      bail!("refusing to overwrite trace export {}", output.display());
   }
   let file_name = output.file_name().and_then(|value| value.to_str()).context("trace export has no UTF-8 file name")?;
   let scratch_path = output.with_file_name(format!(".{}.xctrace-export-tmp", file_name));
   let mut scratch = MacOsTraceScratch::create(&scratch_path)?;
   let status = native_xcrun_command()
      .env("TMPDIR", &scratch.path)
      .env("TMP", &scratch.path)
      .env("TEMP", &scratch.path)
      .args(["xctrace", "export", "--input"])
      .arg(trace)
      .args(selector)
      .args(["--output"])
      .arg(output)
      .status()
      .context("exporting macOS comparison trace")?;
   scratch.cleanup()?;
   if !status.success()
   {
      bail!("xctrace export failed with {} for {}", status, trace.display());
   }
   Ok(())
}

fn native_xcrun_command() -> Command
{
   let mut command = Command::new("/usr/bin/arch");
   command.args(["-arm64", "xcrun"]);
   command
}

pub fn validate_macos_presentation_trace_bundle(path: &Path) -> Result<()>
{
   if trace_bundle_bytes(path)? == 0
   {
      bail!("macOS presentation trace bundle is empty: {}", path.display());
   }
   let required_files = ["form.template"];
   for relative in required_files
   {
      let file = path.join(relative);
      if !file.is_file() || fs::metadata(&file).with_context(|| format!("reading {}", file.display()))?.len() == 0
      {
         bail!("macOS presentation trace is not finalized; required file is missing or empty: {}", file.display());
      }
   }
   for relative in ["corespace", "instrument_data", "shared_data"]
   {
      let directory = path.join(relative);
      if !directory.is_dir()
      {
         bail!("macOS presentation trace is not finalized; required directory is missing: {}", directory.display());
      }
   }
   Ok(())
}

fn trace_bundle_bytes(path: &Path) -> Result<u64>
{
   let metadata = fs::metadata(path).with_context(|| format!("reading metadata for {}", path.display()))?;
   if metadata.is_file()
   {
      return Ok(metadata.len());
   }
   if !metadata.is_dir()
   {
      bail!("trace bundle is neither a file nor directory: {}", path.display());
   }
   let mut total = 0_u64;
   for entry in fs::read_dir(path).with_context(|| format!("reading {}", path.display()))?
   {
      total = total.checked_add(trace_bundle_bytes(&entry?.path())?).context("trace bundle byte count overflow")?;
   }
   Ok(total)
}

fn trace_working_set_bytes(trace_path: &Path, scratch_path: &Path) -> Result<u64>
{
   let trace_bytes = if trace_path.exists() {trace_bundle_bytes(trace_path)?} else {0};
   let scratch_bytes = if scratch_path.exists() {trace_bundle_bytes(scratch_path)?} else {0};
   trace_bytes.checked_add(scratch_bytes).context("macOS trace bundle-plus-scratch byte count overflow")
}

fn persist_resource_artifact(artifact: &MacOsResourceArtifact, identity: &MacOsResourceIdentity<'_>, path: &Path) -> Result<String>
{
   resource::validate_resource_artifact(artifact, identity)?;
   durable_json(artifact, path)?;
   let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
   let stored: MacOsResourceArtifact = serde_json::from_slice(&bytes).with_context(|| format!("decoding {}", path.display()))?;
   resource::validate_resource_artifact(&stored, identity)?;
   Ok(sha256(&bytes))
}

fn validate_launch_session_files(plan: &MacOsCampaignPlan, session: &MacOsCampaignSession, paths: &SessionPaths, executable_sha256: &str, disposition: &str) -> Result<MacOsCampaignSessionResult>
{
   let generation = macos_session_generation(plan, session);
   let preparation_bytes = fs::read(&paths.cache_primer_receipt).with_context(|| format!("reading {}", paths.cache_primer_receipt.display()))?;
   let preparation: MacOsLaunchPreparationReceipt = serde_json::from_slice(&preparation_bytes).with_context(|| format!("decoding {}", paths.cache_primer_receipt.display()))?;
   let expected = MacOsLaunchExpectation {
      run_id: plan.run_id.clone(),
      plan_sha256: plan.plan_sha256.clone(),
      chunk_id: session.chunk_id.clone(),
      pack_id: session.pack_id.clone(),
      pair_index: session.pair_index,
      generation: generation.clone(),
      executable_sha256: String::from(executable_sha256),
      side: session.side,
      launch_class: session_macos_launch_class(session)?,
      installed_bundle_path: preparation.installed_bundle_path.clone(),
      data_container_path: preparation.data_container_path.clone(),
   };
   validate_macos_launch_preparation_receipt(&preparation, &expected)?;
   let application_did_finish = read_json::<MacOsLaunchApplicationDidFinishReceipt>(&paths.launch_application_did_finish)?;
   let first_complete_ui = read_json::<MacOsLaunchFirstCompleteUIReceipt>(&paths.launch_first_complete_ui)?;
   let ready = read_json::<MacOsLaunchReadinessReceipt>(&paths.launch_ready)?;
   let complete = read_json::<MacOsLaunchCompleteReceipt>(&paths.launch_complete)?;
   validate_macos_launch_application_receipts(&application_did_finish, &first_complete_ui, &ready, &complete, &expected)?;
   let artifact = fs::read(&paths.launch_complete).with_context(|| format!("reading {}", paths.launch_complete.display()))?;
   let acknowledgement = fs::read(&paths.launch_acknowledgement).with_context(|| format!("reading {}", paths.launch_acknowledgement.display()))?;
   let ack: SessionAcknowledgement = serde_json::from_slice(&acknowledgement).with_context(|| format!("decoding {}", paths.launch_acknowledgement.display()))?;
   let artifact_sha256 = sha256(&artifact);
   if ack.schema_version != 1 || ack.generation != generation || ack.artifact_sha256 != artifact_sha256 || !ack.durable
   {
      bail!("canonical-launch acknowledgement does not durably commit the application completion artifact");
   }
   let ui_controller = read_json::<MacOsLaunchUIControllerReceipt>(&paths.launch_ui_controller)?;
   validate_macos_launch_ui_controller_receipt(&ui_controller, &expected, "measure")?;
   let controller_start = read_json::<MacOsLaunchUIControllerStartReceipt>(&paths.launch_ui_controller_start)?;
   validate_macos_launch_ui_controller_start_receipt(&controller_start, &expected)?;
   if controller_start.launch_request_ticks != ui_controller.launch_request_ticks
      || controller_start.ready_observed_ticks != ui_controller.ready_observed_ticks
      || ui_controller.clock_anchors.first() != Some(&controller_start.first_clock_anchor)
   {
      bail!("canonical-launch UI-controller receipts disagree");
   }
   let evidence_bytes = fs::read(&paths.launch_evidence).with_context(|| format!("reading {}", paths.launch_evidence.display()))?;
   let evidence: MacOsLaunchEvidence = serde_json::from_slice(&evidence_bytes).with_context(|| format!("decoding {}", paths.launch_evidence.display()))?;
   validate_macos_launch_evidence(&evidence, &expected)?;
   if preparation.completed_ticks != evidence.preparation_completed_ticks
      || sha256(&preparation_bytes) != evidence.preparation_receipt_sha256
   {
      bail!("canonical-launch preparation receipt differs from its launch evidence");
   }
   let resource_bytes = fs::read(&paths.resource).with_context(|| format!("reading {}", paths.resource.display()))?;
   let resource: MacOsResourceArtifact = serde_json::from_slice(&resource_bytes).with_context(|| format!("decoding {}", paths.resource.display()))?;
   let resource_identity = MacOsResourceIdentity {
      plan,
      session,
      generation: &generation,
      executable_sha256,
      pid: evidence.pid,
      launch_t0: evidence.launch_request_ticks,
   };
   resource::validate_resource_artifact(&resource, &resource_identity)?;
   validate_macos_presentation_trace_bundle(&paths.trace)?;
   let trace_toc_sha256 = optional_file_sha256(&paths.trace_toc)?.context("canonical-launch trace has no TOC export")?;
   let trace_signposts_sha256 = optional_file_sha256(&paths.trace_signposts)?.context("canonical-launch trace has no signpost export")?;
   let trace_updates_sha256 = optional_file_sha256(&paths.trace_updates)?.context("canonical-launch trace has no update export")?;
   let trace_frame_lifetimes_sha256 = optional_file_sha256(&paths.trace_frame_lifetimes)?.context("canonical-launch trace has no frame-lifetime export")?;
   let trace_correlation_bytes = fs::read(&paths.trace_correlation).with_context(|| format!("reading {}", paths.trace_correlation.display()))?;
   let correlation: MacOsLaunchPresentationCorrelation = serde_json::from_slice(&trace_correlation_bytes).with_context(|| format!("decoding {}", paths.trace_correlation.display()))?;
   if correlation.pid != evidence.pid
      || correlation.first_attributed_present_proxy_ticks != evidence.first_attributed_present_proxy_ticks
      || correlation.response_attributed_present_proxy_ticks != evidence.first_interactive_response_present_proxy_ticks
      || !correlation.exact_pid_filtered
   {
      bail!("canonical-launch trace correlation differs from its exact-PID launch evidence");
   }
   validate_launch_clock_correlation(&correlation, &first_complete_ui, &complete)?;
   Ok(MacOsCampaignSessionResult {
      session: session.clone(),
      generation,
      executable_sha256: String::from(executable_sha256),
      artifact_sha256,
      acknowledgement_sha256: sha256(&acknowledgement),
      surface_receipt_path: None,
      surface_receipt_sha256: None,
      trace_path: Some(paths.trace.to_string_lossy().into_owned()),
      trace_toc_sha256: Some(trace_toc_sha256),
      trace_signposts_sha256: Some(trace_signposts_sha256),
      trace_updates_sha256: Some(trace_updates_sha256),
      trace_frame_lifetimes_sha256: Some(trace_frame_lifetimes_sha256),
      trace_correlation_path: Some(paths.trace_correlation.to_string_lossy().into_owned()),
      trace_correlation_sha256: Some(sha256(&trace_correlation_bytes)),
      trusted_input_receipt_manifest_path: None,
      trusted_input_receipt_manifest_sha256: None,
      time_profiler_trace_path: None,
      time_profiler_toc_sha256: None,
      time_profiler_signposts_sha256: None,
      time_profiler_samples_sha256: None,
      time_profiler_artifact_path: None,
      time_profiler_artifact_sha256: None,
      system_trace_path: None,
      system_trace_toc_sha256: None,
      system_trace_signposts_sha256: None,
      system_trace_thread_info_sha256: None,
      system_trace_thread_state_sha256: None,
      system_trace_context_switch_sha256: None,
      system_trace_artifact_path: None,
      system_trace_artifact_sha256: None,
      common_gpu_trace_path: None,
      common_gpu_toc_sha256: None,
      common_gpu_signposts_sha256: None,
      common_gpu_samples_path: None,
      common_gpu_samples_sha256: None,
      common_gpu_artifact_path: None,
      common_gpu_artifact_sha256: None,
      launch_evidence_path: Some(paths.launch_evidence.to_string_lossy().into_owned()),
      launch_evidence_sha256: Some(sha256(&evidence_bytes)),
      energy_config_sha256: None,
      energy_request_sha256: None,
      energy_ready_sha256: None,
      energy_raw_path: None,
      energy_raw_sha256: None,
      energy_summary_path: None,
      energy_summary_sha256: None,
      resource_path: paths.resource.to_string_lossy().into_owned(),
      resource_sha256: sha256(&resource_bytes),
      resource_availability: resource.availability,
      disposition: String::from(disposition),
   })
}

fn validate_macos_surface_receipt(plan: &MacOsCampaignPlan, session: &MacOsCampaignSession, generation: &str, receipt: &MacOsSurfaceReceipt) -> Result<()>
{
   let surface = &receipt.snapshot;
   if receipt.schema_version != 1
      || receipt.run_id != plan.run_id
      || receipt.plan_sha256 != plan.plan_sha256
      || receipt.chunk_id != session.chunk_id
      || receipt.pass_id != session.pass_id
      || receipt.pair_index != session.pair_index
      || receipt.side != session.side
      || receipt.generation != generation
      || receipt.pack_id != session.pack_id
      || receipt.validation != "complete-app-reported-content-hash-bound-surface-v1"
   {
      bail!("macOS surface receipt identity differs from its session");
   }
   if surface.window_logical_width_milli_points == 0
      || surface.window_logical_height_milli_points == 0
      || surface.viewport_logical_width_milli_points == 0
      || surface.viewport_logical_height_milli_points == 0
      || surface.backing_pixel_width == 0
      || surface.backing_pixel_height == 0
      || surface.backing_scale_milli == 0
      || surface.final_color_format.is_empty()
      || surface.final_color_space.is_empty()
      || surface.alpha_mode.is_empty()
      || surface.sample_count == 0
      || surface.compositor_scaling.is_empty()
      || surface.target_refresh_policy.is_empty()
      || surface.target_refresh_millihz == 0
      || surface.surface_implementation.is_empty()
      || surface.internal_color_format.is_empty()
      || receipt.observed_refresh_source.is_empty()
      || session.pass_id != "correctness" && receipt.observed_refresh_millihz.is_none()
   {
      bail!("macOS surface receipt is incomplete");
   }
   Ok(())
}

fn validate_session_files(plan: &MacOsCampaignPlan, session: &MacOsCampaignSession, paths: &SessionPaths, executable_sha256: &str, disposition: &str, trusted_input_preflight: &[MacOsTrustedInputScenarioPreflight]) -> Result<MacOsCampaignSessionResult>
{
   if session.pass_id == "canonical-launch"
   {
      return validate_launch_session_files(plan, session, paths, executable_sha256, disposition);
   }
   let artifact = fs::read(&paths.complete).with_context(|| format!("reading {}", paths.complete.display()))?;
   let acknowledgement = fs::read(&paths.acknowledgement).with_context(|| format!("reading {}", paths.acknowledgement.display()))?;
   let envelope: SessionEnvelope = serde_json::from_slice(&artifact).with_context(|| format!("decoding {}", paths.complete.display()))?;
   let ack: SessionAcknowledgement = serde_json::from_slice(&acknowledgement).with_context(|| format!("decoding {}", paths.acknowledgement.display()))?;
   let controller_bytes = fs::read(&paths.controller_receipt).with_context(|| format!("reading {}", paths.controller_receipt.display()))?;
   let controller: StoredControllerReceipt = serde_json::from_slice(&controller_bytes).with_context(|| format!("decoding {}", paths.controller_receipt.display()))?;
   let resource_bytes = fs::read(&paths.resource).with_context(|| format!("reading {}", paths.resource.display()))?;
   let resource_artifact: MacOsResourceArtifact = serde_json::from_slice(&resource_bytes).with_context(|| format!("decoding {}", paths.resource.display()))?;
   let artifact_sha256 = sha256(&artifact);
   let resource_sha256 = sha256(&resource_bytes);
   if envelope.schema_version != 1
      || envelope.run_id != plan.run_id
      || envelope.plan_sha256 != plan.plan_sha256
      || envelope.chunk_id != session.chunk_id
      || envelope.pass_id != session.pass_id
      || envelope.pair_index != session.pair_index
      || envelope.side != session.side
      || envelope.pack_id != session.pack_id
   {
      bail!("comparison session artifact identity differs from its run plan");
   }
   validate_sha256(&envelope.generation)?;
   if envelope.generation != macos_session_generation(plan, session)
   {
      bail!("comparison session generation differs from its deterministic run identity");
   }
   let expected_surface_path = format!(
      "Runs/{}/{}/{}/{}/{}/{}.surface.json",
      plan.run_id,
      session.chunk_id,
      session.pass_id,
      session.pack_id,
      session.pair_index,
      session.side.as_str(),
   );
   validate_sha256(&envelope.surface_receipt.sha256)?;
   if envelope.surface_receipt.path != expected_surface_path
   {
      bail!("macOS surface receipt path differs from its session identity");
   }
   let surface_bytes = fs::read(&paths.surface_receipt).with_context(|| format!("reading {}", paths.surface_receipt.display()))?;
   let surface_sha256 = sha256(&surface_bytes);
   if envelope.surface_receipt.sha256 != surface_sha256
   {
      bail!("macOS surface receipt hash differs from its completion envelope");
   }
   let surface: MacOsSurfaceReceipt = serde_json::from_slice(&surface_bytes).context("decoding macOS surface receipt")?;
   validate_macos_surface_receipt(plan, session, &envelope.generation, &surface)?;
   if session.pass_id == "correctness"
   {
      if envelope.telemetry_coverage.is_some() || paths.telemetry_coverage.exists()
      {
         bail!("untimed correctness session contains telemetry coverage");
      }
   }
   else
   {
      let coverage_identity = envelope.telemetry_coverage.as_ref().context("timed comparison session has no telemetry coverage identity")?;
      validate_sha256(&coverage_identity.sha256)?;
      let expected_coverage_path = format!(
         "Runs/{}/{}/{}/{}/{}/{}.telemetry.coverage.json",
         plan.run_id,
         session.chunk_id,
         session.pass_id,
         session.pack_id,
         session.pair_index,
         session.side.as_str(),
      );
      if coverage_identity.path != expected_coverage_path
      {
         bail!("timed comparison telemetry coverage path differs from its session identity");
      }
      let telemetry = fs::read(&paths.telemetry).with_context(|| format!("reading {}", paths.telemetry.display()))?;
      if envelope.telemetry_sha256 != sha256(&telemetry) || envelope.telemetry_byte_count != telemetry.len() as u64
      {
         bail!("timed comparison telemetry differs from its completion envelope");
      }
      let coverage_bytes = fs::read(&paths.telemetry_coverage).with_context(|| format!("reading {}", paths.telemetry_coverage.display()))?;
      if coverage_identity.sha256 != sha256(&coverage_bytes)
      {
         bail!("timed comparison telemetry coverage differs from its completion envelope");
      }
      validate_macos_telemetry_coverage(&coverage_bytes, &telemetry, session.side, &session.pass_id)?;
   }
   if ack.schema_version != 1 || ack.generation != envelope.generation || ack.artifact_sha256 != artifact_sha256 || !ack.durable
   {
      bail!("comparison session acknowledgement does not durably commit the artifact");
   }
   if controller.schema_version != 1
      || controller.generation != envelope.generation
      || controller.executable_sha256 != executable_sha256
      || controller.resource_sha256 != resource_sha256
      || controller.pid == 0
      || !controller.complete
   {
      bail!("comparison controller receipt does not bind the exact executable, process resources, and completed generation");
   }
   let resource_identity = MacOsResourceIdentity {
      plan,
      session,
      generation: &envelope.generation,
      executable_sha256,
      pid: controller.pid,
      launch_t0: controller.launch_t0,
   };
   resource::validate_resource_artifact(&resource_artifact, &resource_identity)?;
   if resource_artifact.durable_complete_timestamp != controller.complete_timestamp
   {
      bail!("macOS process resource artifact completion does not match its controller receipt");
   }
   let trace_path = if paths.trace.exists()
   {
      validate_macos_presentation_trace_bundle(&paths.trace)?;
      Some(paths.trace.to_string_lossy().into_owned())
   }
   else
   {
      None
   };
   let trace_toc_sha256 = optional_file_sha256(&paths.trace_toc)?;
   let trace_signposts_sha256 = optional_file_sha256(&paths.trace_signposts)?;
   let trace_updates_sha256 = optional_file_sha256(&paths.trace_updates)?;
   let trace_frame_lifetimes_sha256 = optional_file_sha256(&paths.trace_frame_lifetimes)?;
   let trace_correlation_sha256 = optional_file_sha256(&paths.trace_correlation)?;
   let trace_correlation_path = trace_correlation_sha256.as_ref().map(|_| paths.trace_correlation.to_string_lossy().into_owned());
   let trace_export_count = [
      trace_toc_sha256.as_ref(),
      trace_signposts_sha256.as_ref(),
      trace_updates_sha256.as_ref(),
      trace_frame_lifetimes_sha256.as_ref(),
      trace_correlation_sha256.as_ref(),
   ].iter().filter(|value| value.is_some()).count();
   if trace_path.is_some()
      && trace_export_count != 5
   {
      bail!("presentation trace exists without complete validated correlation exports");
   }
   if trace_path.is_none() && trace_export_count != 0
   {
      bail!("presentation trace exports exist without their source trace");
   }
   if let Some(path) = trace_correlation_path.as_ref()
   {
      let correlation_bytes = fs::read(path).with_context(|| format!("reading {}", path))?;
      let correlation: MacOsPresentationCorrelationArtifact = serde_json::from_slice(&correlation_bytes).with_context(|| format!("decoding {}", path))?;
      if correlation.schema_version != 1
         || correlation.availability != trace::MACOS_PRESENTATION_CORRELATION_AVAILABILITY
         || correlation.calibration_status != trace::MACOS_PRESENTATION_CORRELATION_CALIBRATION
         || correlation.correlations.is_empty()
      {
         bail!("presentation correlation artifact is incomplete or has an unsupported contract");
      }
      let toc = fs::read_to_string(&paths.trace_toc).with_context(|| format!("reading {}", paths.trace_toc.display()))?;
      let signposts = fs::read_to_string(&paths.trace_signposts).with_context(|| format!("reading {}", paths.trace_signposts.display()))?;
      let updates = fs::read_to_string(&paths.trace_updates).with_context(|| format!("reading {}", paths.trace_updates.display()))?;
      let frame_lifetimes = fs::read_to_string(&paths.trace_frame_lifetimes).with_context(|| format!("reading {}", paths.trace_frame_lifetimes.display()))?;
      validate_macos_trace_exports(&toc, &signposts)?;
      let expected = correlate_macos_presentation_trace(&signposts, &updates, &frame_lifetimes)?;
      if correlation != expected
      {
         bail!("presentation correlation artifact differs from its exported trace evidence");
      }
      let process_suffix = format!(" ({})", controller.pid);
      if correlation.correlations.iter().any(|row| !row.process.ends_with(&process_suffix))
         || correlation.uncorrelated_visual_generations.iter().any(|row| !row.process.ends_with(&process_suffix))
      {
         bail!("presentation correlation artifact is not scoped to controller PID {}", controller.pid);
      }
   }
   let time_profiler_requested = session.collector.as_deref() == Some("time-profiler");
   let time_profiler_trace_path = if paths.time_profiler_trace.exists()
   {
      validate_macos_presentation_trace_bundle(&paths.time_profiler_trace)?;
      Some(paths.time_profiler_trace.to_string_lossy().into_owned())
   }
   else
   {
      None
   };
   let time_profiler_toc_sha256 = optional_file_sha256(&paths.time_profiler_toc)?;
   let time_profiler_signposts_sha256 = optional_file_sha256(&paths.time_profiler_signposts)?;
   let time_profiler_samples_sha256 = optional_file_sha256(&paths.time_profiler_samples)?;
   let time_profiler_artifact_sha256 = optional_file_sha256(&paths.time_profiler_artifact)?;
   let time_profiler_export_count = [
      time_profiler_toc_sha256.as_ref(),
      time_profiler_signposts_sha256.as_ref(),
      time_profiler_samples_sha256.as_ref(),
      time_profiler_artifact_sha256.as_ref(),
   ].iter().filter(|value| value.is_some()).count();
   if time_profiler_requested
   {
      if trace_path.is_some() || time_profiler_trace_path.is_none() || time_profiler_export_count != 4
      {
         bail!("Time Profiler attribution must have one isolated complete Time Profiler trace and no Animation Hitches trace");
      }
      let toc = fs::read_to_string(&paths.time_profiler_toc).with_context(|| format!("reading {}", paths.time_profiler_toc.display()))?;
      if !toc.contains("time-profile") || !toc.contains("os-signpost")
      {
         bail!("macOS Time Profiler TOC omits the time-profile or os-signpost schema");
      }
      let signposts = fs::read_to_string(&paths.time_profiler_signposts).with_context(|| format!("reading {}", paths.time_profiler_signposts.display()))?;
      let samples = fs::read_to_string(&paths.time_profiler_samples).with_context(|| format!("reading {}", paths.time_profiler_samples.display()))?;
      let expected = reduce_macos_time_profiler_trace(&samples, &signposts, controller.pid)?;
      let artifact_bytes = fs::read(&paths.time_profiler_artifact).with_context(|| format!("reading {}", paths.time_profiler_artifact.display()))?;
      let artifact: MacOsTimeProfilerArtifact = serde_json::from_slice(&artifact_bytes).with_context(|| format!("decoding {}", paths.time_profiler_artifact.display()))?;
      if artifact != expected
      {
         bail!("macOS Time Profiler artifact differs from its exact-PID XML exports");
      }
   }
   else if time_profiler_trace_path.is_some() || time_profiler_export_count != 0
   {
      bail!("non-Time-Profiler session contains Time Profiler evidence");
   }
   let system_trace_requested = session.collector.as_deref() == Some("system-trace");
   let system_trace_path = if paths.system_trace.exists()
   {
      validate_macos_presentation_trace_bundle(&paths.system_trace)?;
      Some(paths.system_trace.to_string_lossy().into_owned())
   }
   else
   {
      None
   };
   let system_trace_toc_sha256 = optional_file_sha256(&paths.system_trace_toc)?;
   let system_trace_signposts_sha256 = optional_file_sha256(&paths.system_trace_signposts)?;
   let system_trace_thread_info_sha256 = optional_file_sha256(&paths.system_trace_thread_info)?;
   let system_trace_thread_state_sha256 = optional_file_sha256(&paths.system_trace_thread_state)?;
   let system_trace_context_switch_sha256 = optional_file_sha256(&paths.system_trace_context_switch)?;
   let system_trace_artifact_sha256 = optional_file_sha256(&paths.system_trace_artifact)?;
   let system_trace_export_count = [
      system_trace_toc_sha256.as_ref(),
      system_trace_signposts_sha256.as_ref(),
      system_trace_thread_info_sha256.as_ref(),
      system_trace_thread_state_sha256.as_ref(),
      system_trace_context_switch_sha256.as_ref(),
      system_trace_artifact_sha256.as_ref(),
   ].iter().filter(|value| value.is_some()).count();
   if system_trace_requested
   {
      if trace_path.is_some() || time_profiler_trace_path.is_some() || system_trace_path.is_none() || system_trace_export_count != 6
      {
         bail!("System Trace attribution must have one isolated complete System Trace and no Animation Hitches or Time Profiler trace");
      }
      let toc = fs::read_to_string(&paths.system_trace_toc).with_context(|| format!("reading {}", paths.system_trace_toc.display()))?;
      for schema in ["os-signpost", "thread-info", "thread-state", "context-switch"]
      {
         if !toc.contains(&format!("schema=\"{}\"", schema))
         {
            bail!("macOS System Trace TOC omits the {} schema", schema);
         }
      }
      let expected = reduce_macos_system_trace(
         &fs::read_to_string(&paths.system_trace_signposts).with_context(|| format!("reading {}", paths.system_trace_signposts.display()))?,
         &fs::read_to_string(&paths.system_trace_thread_info).with_context(|| format!("reading {}", paths.system_trace_thread_info.display()))?,
         &fs::read_to_string(&paths.system_trace_thread_state).with_context(|| format!("reading {}", paths.system_trace_thread_state.display()))?,
         &fs::read_to_string(&paths.system_trace_context_switch).with_context(|| format!("reading {}", paths.system_trace_context_switch.display()))?,
         controller.pid,
      )?;
      let artifact_bytes = fs::read(&paths.system_trace_artifact).with_context(|| format!("reading {}", paths.system_trace_artifact.display()))?;
      let artifact: MacOsSystemTraceSummary = serde_json::from_slice(&artifact_bytes).with_context(|| format!("decoding {}", paths.system_trace_artifact.display()))?;
      if artifact != expected
      {
         bail!("macOS System Trace artifact differs from its exact-PID XML exports");
      }
   }
   else if system_trace_path.is_some() || system_trace_export_count != 0
   {
      bail!("non-System-Trace session contains System Trace evidence");
   }
   let common_gpu_requested = session.collector.as_deref() == Some("common-gpu");
   let common_gpu_trace_path = if paths.common_gpu_trace.exists()
   {
      validate_macos_presentation_trace_bundle(&paths.common_gpu_trace)?;
      Some(paths.common_gpu_trace.to_string_lossy().into_owned())
   }
   else {None};
   let common_gpu_toc_sha256 = optional_file_sha256(&paths.common_gpu_toc)?;
   let common_gpu_signposts_sha256 = optional_file_sha256(&paths.common_gpu_signposts)?;
   let common_gpu_samples_sha256 = optional_file_sha256(&paths.common_gpu_samples)?;
   let common_gpu_artifact_sha256 = optional_file_sha256(&paths.common_gpu_artifact)?;
   let common_gpu_export_count = [
      common_gpu_toc_sha256.as_ref(),
      common_gpu_signposts_sha256.as_ref(),
      common_gpu_samples_sha256.as_ref(),
      common_gpu_artifact_sha256.as_ref(),
   ].iter().filter(|value| value.is_some()).count();
   if common_gpu_requested
   {
      if session.evidence_role == AppleCampaignEvidenceRole::ClaimBearing
         || trace_path.is_some()
         || time_profiler_trace_path.is_some()
         || system_trace_path.is_some()
         || common_gpu_trace_path.is_none()
         || common_gpu_export_count != 4
      {
         bail!("diagnostic common-GPU must have one isolated complete trace/sample set and cannot satisfy claim-bearing evidence");
      }
      let samples: MacOsCommonGpuSamples = read_json(&paths.common_gpu_samples)?;
      let signposts = fs::read_to_string(&paths.common_gpu_signposts).with_context(|| format!("reading {}", paths.common_gpu_signposts.display()))?;
      let expected = reduce_macos_common_gpu(&samples, &signposts, controller.pid)?;
      let summary: MacOsCommonGpuSummary = read_json(&paths.common_gpu_artifact)?;
      if summary != expected || summary.comparison_eligible
      {
         bail!("macOS common-GPU diagnostic differs from exact-PID phase-bounded evidence or became comparison eligible");
      }
   }
   else if common_gpu_trace_path.is_some() || common_gpu_export_count != 0
   {
      bail!("non-common-GPU session contains common-GPU evidence");
   }
   let energy_requested = session.pass_role == AppleCampaignPassRole::Energy;
   let energy_request_sha256 = optional_file_sha256(&paths.energy_request)?;
   let energy_ready_sha256 = optional_file_sha256(&paths.energy_ready)?;
   let energy_raw_sha256 = optional_file_sha256(&paths.energy_raw)?;
   let energy_summary_sha256 = optional_file_sha256(&paths.energy_summary)?;
   let energy_evidence_count = [energy_request_sha256.as_ref(), energy_ready_sha256.as_ref(), energy_raw_sha256.as_ref(), energy_summary_sha256.as_ref()].iter().filter(|value| value.is_some()).count();
   let (energy_config_sha256, energy_raw_path, energy_summary_path) = if energy_requested
   {
      if trace_path.is_some() || time_profiler_trace_path.is_some() || system_trace_path.is_some() || common_gpu_trace_path.is_some() || energy_evidence_count != 4
      {
         bail!("macOS energy must have one complete isolated external-meter evidence set and no profiler trace");
      }
      let request: MacOsEnergyAdapterRequest = read_json(&paths.energy_request)?;
      validate_macos_energy_adapter_request(&request)?;
      let ready: MacOsEnergyAdapterReady = read_json(&paths.energy_ready)?;
      let raw: MacOsExternalMeterRawArtifact = read_json(&paths.energy_raw)?;
      let summary: MacOsEnergySummary = read_json(&paths.energy_summary)?;
      let request_sha256 = energy_request_sha256.as_ref().context("macOS energy request hash disappeared")?;
      if request.run_id != plan.run_id
         || request.plan_sha256 != plan.plan_sha256
         || request.generation != envelope.generation
         || request.pid != controller.pid
         || ready.schema_version != 1
         || ready.protocol != "oxide-direct-external-meter-v1"
         || ready.request_sha256 != *request_sha256
         || ready.adapter_sha256 != request.adapter_sha256
         || ready.calibration_sha256 != request.calibration_sha256
         || ready.ready_ticks == 0
         || !ready.hardware_ready
         || !ready.complete
         || raw.run_id != plan.run_id
         || raw.plan_sha256 != plan.plan_sha256
         || raw.generation != envelope.generation
         || raw.pid != controller.pid
         || raw.adapter_sha256 != request.adapter_sha256
         || raw.calibration_sha256 != request.calibration_sha256
      {
         bail!("macOS energy request, ready receipt, or raw samples differ from the active session");
      }
      let telemetry = fs::read(&paths.telemetry).with_context(|| format!("reading {}", paths.telemetry.display()))?;
      if envelope.telemetry_sha256 != sha256(&telemetry)
         || envelope.telemetry_byte_count != telemetry.len() as u64
         || envelope.timebase_numerator != raw.timebase_numerator
         || envelope.timebase_denominator != raw.timebase_denominator
         || summary != reduce_macos_energy(&raw, &summary.calibration, &telemetry)?
      {
         bail!("macOS energy summary differs from its phase-bound raw samples or comparator telemetry");
      }
      (Some(request.config_sha256), Some(paths.energy_raw.to_string_lossy().into_owned()), Some(paths.energy_summary.to_string_lossy().into_owned()))
   }
   else
   {
      if energy_evidence_count != 0 || paths.energy_partial_raw.exists()
      {
         bail!("non-energy session contains external-meter evidence");
      }
      (None, None, None)
   };
   let trusted_input_receipt_manifest = if session.pass_role == AppleCampaignPassRole::Primary
   {
      Some(trusted_input::materialize_macos_trusted_input_receipt_manifest(&paths.directory, plan, session, &envelope.generation, trusted_input_preflight)?)
   }
   else {None};
   Ok(MacOsCampaignSessionResult {
      session: session.clone(),
      generation: envelope.generation,
      executable_sha256: String::from(executable_sha256),
      artifact_sha256,
      acknowledgement_sha256: sha256(&acknowledgement),
      surface_receipt_path: Some(paths.surface_receipt.to_string_lossy().into_owned()),
      surface_receipt_sha256: Some(surface_sha256),
      trace_path,
      trace_toc_sha256,
      trace_signposts_sha256,
      trace_updates_sha256,
      trace_frame_lifetimes_sha256,
      trace_correlation_path,
      trace_correlation_sha256,
      trusted_input_receipt_manifest_path: trusted_input_receipt_manifest.as_ref().map(|identity| identity.path.clone()),
      trusted_input_receipt_manifest_sha256: trusted_input_receipt_manifest.map(|identity| identity.sha256),
      time_profiler_trace_path,
      time_profiler_toc_sha256,
      time_profiler_signposts_sha256,
      time_profiler_samples_sha256,
      time_profiler_artifact_path: time_profiler_artifact_sha256.as_ref().map(|_| paths.time_profiler_artifact.to_string_lossy().into_owned()),
      time_profiler_artifact_sha256,
      system_trace_path,
      system_trace_toc_sha256,
      system_trace_signposts_sha256,
      system_trace_thread_info_sha256,
      system_trace_thread_state_sha256,
      system_trace_context_switch_sha256,
      system_trace_artifact_path: system_trace_artifact_sha256.as_ref().map(|_| paths.system_trace_artifact.to_string_lossy().into_owned()),
      system_trace_artifact_sha256,
      common_gpu_trace_path,
      common_gpu_toc_sha256,
      common_gpu_signposts_sha256,
      common_gpu_samples_path: common_gpu_samples_sha256.as_ref().map(|_| paths.common_gpu_samples.to_string_lossy().into_owned()),
      common_gpu_samples_sha256,
      common_gpu_artifact_path: common_gpu_artifact_sha256.as_ref().map(|_| paths.common_gpu_artifact.to_string_lossy().into_owned()),
      common_gpu_artifact_sha256,
      launch_evidence_path: None,
      launch_evidence_sha256: None,
      energy_config_sha256,
      energy_request_sha256,
      energy_ready_sha256,
      energy_raw_path,
      energy_raw_sha256,
      energy_summary_path,
      energy_summary_sha256,
      resource_path: paths.resource.to_string_lossy().into_owned(),
      resource_sha256,
      resource_availability: resource_artifact.availability,
      disposition: String::from(disposition),
   })
}

fn optional_file_sha256(path: &Path) -> Result<Option<String>>
{
   if !path.exists()
   {
      return Ok(None);
   }
   let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
   if bytes.is_empty()
   {
      bail!("trace export is empty: {}", path.display());
   }
   Ok(Some(sha256(&bytes)))
}

fn audit_forbidden_markers(root: &Path, markers: &[&str]) -> Result<()>
{
   if !root.exists()
   {
      return Ok(());
   }
   for entry in fs::read_dir(root).with_context(|| format!("reading {}", root.display()))?
   {
      let path = entry?.path();
      if path.is_dir()
      {
         audit_forbidden_markers(&path, markers)?;
         continue;
      }
      let extension = path.extension().and_then(|value| value.to_str()).unwrap_or_default();
      if !["rs", "m", "mm", "h", "swift", "js", "ts", "html"].contains(&extension)
      {
         continue;
      }
      let source = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
      if let Some(marker) = markers.iter().find(|marker| source.contains(**marker))
      {
         bail!("comparison harness marker `{}` leaked into production source {}", marker, path.display());
      }
   }
   Ok(())
}

fn validate_run_id(value: &str) -> Result<()>
{
   if value.is_empty() || value.len() > 96 || !value.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
   {
      bail!("comparison run id is not a bounded path-safe identifier");
   }
   Ok(())
}

fn validate_git_object_id(value: &str) -> Result<()>
{
   if !matches!(value.len(), 40 | 64) || !value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
   {
      bail!("comparison Git object ID is not canonical lowercase hexadecimal");
   }
   Ok(())
}

fn validate_sha256(value: &str) -> Result<()>
{
   if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
   {
      bail!("comparison SHA-256 is not canonical lowercase hexadecimal");
   }
   Ok(())
}

fn sha256(bytes: &[u8]) -> String
{
   format!("{:x}", Sha256::digest(bytes))
}

pub fn comparison_tree_manifest(root: &Path, exclude_build: bool) -> Result<(String, u64, u64)>
{
   if !root.is_dir()
   {
      bail!("tree manifest root is not a directory: {}", root.display());
   }
   let mut entries = Vec::new();
   collect_tree_entries(root, root, exclude_build, &mut entries)?;
   entries.sort();
   let mut digest = Sha256::new();
   let mut bytes = 0_u64;
   for (relative, is_symlink) in &entries
   {
      let path = root.join(relative);
      let contents = if *is_symlink
      {
         fs::read_link(&path).with_context(|| format!("reading symlink {}", path.display()))?.to_string_lossy().into_owned().into_bytes()
      }
      else
      {
         fs::read(&path).with_context(|| format!("reading {}", path.display()))?
      };
      bytes = bytes.checked_add(contents.len() as u64).context("comparison tree byte count overflow")?;
      let relative_string = relative.to_string_lossy();
      digest.update((relative_string.len() as u64).to_le_bytes());
      digest.update(relative_string.as_bytes());
      digest.update([if *is_symlink {b'L'} else {b'F'}]);
      digest.update((contents.len() as u64).to_le_bytes());
      digest.update(Sha256::digest(&contents));
   }
   Ok((format!("{:x}", digest.finalize()), entries.len() as u64, bytes))
}

fn tree_manifest(root: &Path) -> Result<(String, u64, u64)>
{
   comparison_tree_manifest(root, false)
}

fn collect_tree_entries(root: &Path, directory: &Path, exclude_build: bool, entries: &mut Vec<(PathBuf, bool)>) -> Result<()>
{
   let mut directory_entries = fs::read_dir(directory).with_context(|| format!("reading {}", directory.display()))?.collect::<std::result::Result<Vec<_>, _>>()?;
   directory_entries.sort_by_key(|entry| entry.file_name());
   for entry in directory_entries
   {
      let path = entry.path();
      let relative = path.strip_prefix(root).context("comparison tree path escaped its root")?;
      if exclude_build && relative.components().next().is_some_and(|component| component.as_os_str() == "build")
      {
         continue;
      }
      let metadata = fs::symlink_metadata(&path).with_context(|| format!("reading metadata for {}", path.display()))?;
      if metadata.file_type().is_symlink()
      {
         entries.push((relative.to_path_buf(), true));
      }
      else if metadata.is_dir()
      {
         collect_tree_entries(root, &path, exclude_build, entries)?;
      }
      else if metadata.is_file()
      {
         entries.push((relative.to_path_buf(), false));
      }
      else
      {
         bail!("comparison tree contains an unsupported non-file entry: {}", path.display());
      }
   }
   Ok(())
}

fn durable_json<T: Serialize>(value: &T, destination: &Path) -> Result<()>
{
   let bytes = serde_json::to_vec_pretty(value).context("encoding durable comparison JSON")?;
   let directory = destination.parent().filter(|path| !path.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
   fs::create_dir_all(directory).with_context(|| format!("creating {}", directory.display()))?;
   let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).context("system clock precedes Unix epoch")?.as_nanos();
   let file_name = destination.file_name().and_then(|value| value.to_str()).context("durable output has no UTF-8 file name")?;
   let temporary = directory.join(format!(".{}.{}.{}.tmp", file_name, std::process::id(), timestamp));
   let result = (|| -> Result<()> {
      let mut file = OpenOptions::new().write(true).create_new(true).open(&temporary).with_context(|| format!("creating {}", temporary.display()))?;
      file.write_all(&bytes).with_context(|| format!("writing {}", temporary.display()))?;
      file.write_all(b"\n").with_context(|| format!("terminating {}", temporary.display()))?;
      file.sync_all().with_context(|| format!("synchronizing {}", temporary.display()))?;
      fs::rename(&temporary, destination).with_context(|| format!("renaming {}", destination.display()))?;
      File::open(directory).with_context(|| format!("opening {}", directory.display()))?.sync_all().with_context(|| format!("synchronizing {}", directory.display()))?;
      Ok(())
   })();
   if result.is_err()
   {
      let _ = fs::remove_file(&temporary);
   }
   result
}

#[cfg(test)]
mod storage_tests
{
   use super::*;

   #[test]
   fn session_xctestrun_injects_runner_environment_without_mutating_the_build_artifact()
   {
      let root = tempfile::tempdir().expect("temporary xctestrun root");
      let source = root.path().join("source.xctestrun");
      let destination = root.path().join("session.xctestrun");
      let mut variables = plist::Dictionary::new();
      variables.insert(String::from("PRESERVED"), plist::Value::String(String::from("yes")));
      let mut target = plist::Dictionary::new();
      target.insert(String::from("EnvironmentVariables"), plist::Value::Dictionary(variables));
      let mut document = plist::Dictionary::new();
      document.insert(String::from("MacOSComparisonControllerUITests"), plist::Value::Dictionary(target));
      plist::to_file_xml(&source, &plist::Value::Dictionary(document)).expect("source xctestrun");
      let source_bytes = fs::read(&source).expect("source bytes");
      let environment = BTreeMap::from([(String::from("OXIDE_COMPARISON_APP_BUNDLE"), String::from("/tmp/Oxide.app"))]);
      materialize_controller_xctestrun(&source, &destination, &environment).expect("session xctestrun");
      assert_eq!(fs::read(&source).expect("source bytes after materialization"), source_bytes);
      let session = plist::Value::from_file(&destination).expect("session xctestrun");
      let variables = session.as_dictionary().expect("session root")
         .get("MacOSComparisonControllerUITests").and_then(plist::Value::as_dictionary).expect("session target")
         .get("EnvironmentVariables").and_then(plist::Value::as_dictionary).expect("session environment");
      assert_eq!(variables.get("PRESERVED").and_then(plist::Value::as_string), Some("yes"));
      assert_eq!(variables.get("OXIDE_COMPARISON_APP_BUNDLE").and_then(plist::Value::as_string), Some("/tmp/Oxide.app"));
   }

   #[test]
   fn translated_instruments_service_is_detected_with_its_exact_controller_parent()
   {
      let rows = macos_process_rows(
         "  17   9 34006 /Applications/Xcode.app/Contents/SharedFrameworks/DVTInstrumentsFoundation.framework/Resources/DTServiceHub\n\
            9   1 4006 /Applications/Xcode.app/Contents/Developer/usr/bin/xcodebuild test-without-building -xctestrun /tmp/.oxide-controller-generation.oxide.measure.xctestrun\n\
           22   1 4006 /Applications/Xcode.app/Contents/Developer/usr/bin/xctrace record --template Time Profiler\n"
      ).expect("process rows");
      assert_eq!(rows.len(), 3);
      assert_eq!(rows[0].pid, 17);
      assert_eq!(rows[0].parent_pid, 9);
      assert_eq!(rows[0].flags & PROCESS_TRANSLATED_FLAG, PROCESS_TRANSLATED_FLAG);
      assert_eq!(rows[0].command, DVT_SERVICE_HUB);
      assert!(rows[1].command.contains(".oxide-controller-generation."));
      assert_eq!(rows[2].flags & PROCESS_TRANSLATED_FLAG, 0);
   }

   #[test]
   fn trace_scratch_is_isolated_counted_and_removed_on_drop()
   {
      let root = tempfile::tempdir().expect("temporary trace root");
      let scratch_path = root.path().join("oxide.xctrace-tmp");
      {
         let spill_root = root.path().join("system-temp");
         fs::create_dir(&spill_root).expect("spill root");
         let scratch = MacOsTraceScratch::create_with_spill_root(&scratch_path, &spill_root).expect("trace scratch");
         fs::write(scratch_path.join("instruments-test.ktrace"), [0_u8; 17]).expect("trace scratch bytes");
         assert_eq!(scratch.bytes().expect("trace scratch size"), 17);
      }
      assert!(!scratch_path.exists());
   }

   #[test]
   fn trace_scratch_counts_and_removes_xctrace_root_spills()
   {
      let root = tempfile::tempdir().expect("temporary trace root");
      let scratch_path = root.path().join("oxide.xctrace-tmp");
      let spill_root = root.path().join("system-temp");
      fs::create_dir(&spill_root).expect("spill root");
      {
         let scratch = MacOsTraceScratch::create_with_spill_root(&scratch_path, &spill_root).expect("trace scratch");
         fs::write(spill_root.join("instruments-test.ktrace"), [0_u8; 23]).expect("root spill bytes");
         assert_eq!(scratch.bytes().expect("trace scratch size"), 23);
      }
      assert!(!scratch_path.exists());
      assert!(macos_instruments_spill_files(&spill_root).expect("remaining spills").is_empty());
   }

   #[test]
   fn trace_working_set_counts_bundle_and_scratch_together()
   {
      let root = tempfile::tempdir().expect("temporary trace root");
      let trace = root.path().join("trace");
      let scratch = root.path().join("scratch");
      fs::create_dir(&trace).expect("trace directory");
      fs::create_dir(&scratch).expect("scratch directory");
      fs::write(trace.join("bundle"), [0_u8; 11]).expect("trace bytes");
      fs::write(scratch.join("instruments-test.ktrace"), [0_u8; 13]).expect("scratch bytes");
      assert_eq!(trace_working_set_bytes(&trace, &scratch).expect("working-set bytes"), 24);
   }

   #[test]
   fn trace_owner_terminates_child_and_removes_scratch_on_drop()
   {
      let root = tempfile::tempdir().expect("temporary trace root");
      let scratch_path = root.path().join("oxide.xctrace-tmp");
      let scratch = MacOsTraceScratch::create(&scratch_path).expect("trace scratch");
      let child = Command::new("/bin/sleep").arg("30").spawn().expect("trace stand-in");
      let pid = child.id();
      {
         let _trace = MacOsPresentationTrace::new(child, scratch);
      }
      assert!(!scratch_path.exists());
      let status = Command::new("/bin/kill")
         .args(["-0", &pid.to_string()])
         .stderr(Stdio::null())
         .status()
         .expect("query trace stand-in");
      assert!(!status.success());
   }
}
