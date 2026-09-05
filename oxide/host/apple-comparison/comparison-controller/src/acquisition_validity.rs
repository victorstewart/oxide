use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, ensure, Context, Result};
use oxide_benchmark_spec::{ArtifactIdentity, InstrumentationCalibrationReport, INSTRUMENTATION_CALIBRATION_CPU_MARGIN_RATIO, INSTRUMENTATION_CALIBRATION_P50_MARGIN_RATIO, INSTRUMENTATION_CALIBRATION_P95_MARGIN_RATIO, INSTRUMENTATION_CALIBRATION_SCHEMA_VERSION};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{ComparisonSide, MacOsCampaignPlan, MacOsCampaignSession, MacOsCampaignSessionResult, MacOsPresentationCorrelationArtifact, MacOsSurfaceReceipt, MacOsSurfaceSnapshot, MacOsTrustedInputReceiptManifest, SessionEnvelope};

pub const MACOS_MINIMUM_CORRELATED_OPPORTUNITIES_PER_SESSION: u64 = 30;
pub const MACOS_AUTHORITATIVE_INPUT_SCOPE: &str = "trusted-os-input-plus-declared-application-stimuli";
pub const MACOS_RAW_APPLICATION_RECEIPT_STATUS_COMPLETE: &str = "complete-canonical-request-controller-application-v1";
pub const MACOS_RAW_APPLICATION_RECEIPT_STATUS_UNAVAILABLE: &str = "raw-application-input-receipt-contract-not-implemented";
pub const MACOS_BACKGROUND_OBSERVATION_SECONDS: u64 = 10;
const MACOS_BACKGROUND_MEDIAN_CPU_LIMIT_BASIS_POINTS: u64 = 500;
const MACOS_BACKGROUND_PEAK_CPU_LIMIT_BASIS_POINTS: u64 = 2_000;
const MACOS_BACKGROUND_WAKEUP_LIMIT: u64 = MACOS_BACKGROUND_OBSERVATION_SECONDS * 5;
const MACOS_BACKGROUND_GPU_LIMIT_BASIS_POINTS: u64 = 100;
const MACOS_BACKGROUND_SAMPLE_COUNT: usize = MACOS_BACKGROUND_OBSERVATION_SECONDS as usize;
const MACOS_BACKGROUND_SOURCE: &str = "top-11-samples-drop-first-1hz-idlew-v1";
const MACOS_UNAVAILABLE_SOURCE: &str = "unavailable-no-supported-read-only-macos-api";
const MACOS_TOP_OUTPUT_LIMIT_BYTES: u64 = 64 * 1_024 * 1_024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MacOsThermalState
{
   Nominal,
   Warning,
   Critical,
   Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MacOsPowerSource
{
   AcPower,
   BatteryPower,
   UpsPower,
   Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MacOsObservationAvailability
{
   Available,
   Unavailable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MacOsEnvironmentComparisonPolicy
{
   Exact,
   Tolerance,
   RecordOnly,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsEnvironmentFieldPolicy
{
   pub comparison: MacOsEnvironmentComparisonPolicy,
   pub unit: String,
   pub target: Option<i64>,
   pub absolute_tolerance: Option<u64>,
   pub relative_tolerance_basis_points: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsEnvironmentValue<T>
{
   pub availability: MacOsObservationAvailability,
   pub value: Option<T>,
   pub source: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsDisplayObservation
{
   pub display_id: String,
   pub mode: String,
   pub logical_width: u64,
   pub logical_height: u64,
   pub pixel_width: u64,
   pub pixel_height: u64,
   pub scale_x_milli: u64,
   pub scale_y_milli: u64,
   pub refresh_millihz: u64,
   pub rotation_degrees: u64,
   pub mirrored: bool,
   pub connection_type: String,
   pub display_type: String,
   pub auto_brightness_enabled: MacOsEnvironmentValue<bool>,
   pub color_profile: MacOsEnvironmentValue<String>,
   pub edr_headroom_milli: MacOsEnvironmentValue<u64>,
   pub main: bool,
   pub online: bool,
   pub asleep: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsNoisyProcess
{
   pub pid: u32,
   pub median_cpu_basis_points: u64,
   pub peak_cpu_basis_points: u64,
   pub wakeups: u64,
   pub command: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsBackgroundObservation
{
   pub start_timestamp_unix_ns: u64,
   pub end_timestamp_unix_ns: u64,
   pub requested_duration_ns: u64,
   pub source: String,
   pub sample_count: u64,
   pub median_cpu_basis_points: u64,
   pub peak_cpu_basis_points: u64,
   pub wakeups: MacOsEnvironmentValue<u64>,
   pub noisy_processes: Vec<MacOsNoisyProcess>,
   pub unrelated_gpu_busy_basis_points: MacOsEnvironmentValue<u64>,
   pub device_gpu_busy_basis_points: MacOsEnvironmentValue<u64>,
   pub raw_observation_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsEnvironmentSnapshot
{
   pub timestamp_unix_ns: u64,
   pub thermal_state: MacOsThermalState,
   pub thermal_observation_sha256: String,
   pub displays: Vec<MacOsDisplayObservation>,
   pub display_observation_sha256: String,
   pub power_source: MacOsPowerSource,
   pub power_observation_sha256: String,
   pub surface: MacOsSurfaceContractObservation,
   pub fan_rpm: MacOsEnvironmentValue<u64>,
   pub brightness_basis_points: MacOsEnvironmentValue<u64>,
   pub luminance_millinits: MacOsEnvironmentValue<u64>,
   pub true_tone_enabled: MacOsEnvironmentValue<bool>,
   pub night_shift_enabled: MacOsEnvironmentValue<bool>,
   pub reduce_motion_enabled: MacOsEnvironmentValue<bool>,
   pub notifications_suppressed: MacOsEnvironmentValue<bool>,
   pub background_sync_quiescent: MacOsEnvironmentValue<bool>,
   pub screen_recording_inactive: MacOsEnvironmentValue<bool>,
   pub room_temperature_millicelsius: MacOsEnvironmentValue<i64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsSurfaceContractObservation
{
   pub window_logical_size: MacOsEnvironmentValue<String>,
   pub viewport_logical_size: MacOsEnvironmentValue<String>,
   pub backing_pixel_size: MacOsEnvironmentValue<String>,
   pub content_insets: MacOsEnvironmentValue<String>,
   pub backing_scale_milli: MacOsEnvironmentValue<u64>,
   pub color_format: MacOsEnvironmentValue<String>,
   pub color_space: MacOsEnvironmentValue<String>,
   pub alpha_mode: MacOsEnvironmentValue<String>,
   pub sample_count: MacOsEnvironmentValue<u64>,
   pub compositor_scaling: MacOsEnvironmentValue<String>,
   pub target_refresh_policy: MacOsEnvironmentValue<String>,
   pub target_refresh_millihz: MacOsEnvironmentValue<u64>,
   pub observed_refresh_millihz: MacOsEnvironmentValue<u64>,
   pub surface_implementation: MacOsEnvironmentValue<String>,
   pub internal_color_format: MacOsEnvironmentValue<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsPrePairEnvironmentObservation
{
   pub chunk_id: String,
   pub pass_id: String,
   pub pack_id: String,
   pub pair_index: u32,
   pub snapshot: MacOsEnvironmentSnapshot,
   pub background: MacOsBackgroundObservation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsOpportunityObservation
{
   pub pair_index: u32,
   pub side: ComparisonSide,
   pub correlated_count: u64,
   pub display_opportunity_count: u64,
   pub uncorrelated_count: u64,
   pub correlation_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsMeasuredInputObservation
{
   pub pair_index: u32,
   pub side: ComparisonSide,
   pub complete_envelope_sha256: String,
   pub injection_scope: String,
   pub validation: String,
   pub raw_application_receipt_status: String,
   pub raw_application_receipt_manifest: Option<ArtifactIdentity>,
   pub surface_receipt: ArtifactIdentity,
   pub surface_contract: MacOsSurfaceContractObservation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsAcquisitionValidityReport
{
   pub schema_version: u32,
   pub run_id: String,
   pub plan_sha256: String,
   pub build_manifest_sha256: String,
   pub before: MacOsEnvironmentSnapshot,
   pub expected_pair_count: u64,
   pub pre_pair: Vec<MacOsPrePairEnvironmentObservation>,
   pub after: MacOsEnvironmentSnapshot,
   pub opportunities: Vec<MacOsOpportunityObservation>,
   pub measured_input: Vec<MacOsMeasuredInputObservation>,
   pub presentation_calibration_statuses: Vec<String>,
   pub trace_overhead_calibration_sha256: Option<String>,
   pub external_sensor_calibration_sha256: Option<String>,
   pub common_gpu_evidence_sha256: Vec<String>,
   pub environment_policies: BTreeMap<String, MacOsEnvironmentFieldPolicy>,
   pub surface_contract_gate_passed: bool,
   pub pre_pair_environment_gate_passed: bool,
   pub fan_gate_passed: bool,
   pub brightness_gate_passed: bool,
   pub auto_brightness_gate_passed: bool,
   pub true_tone_gate_passed: bool,
   pub night_shift_gate_passed: bool,
   pub reduce_motion_gate_passed: bool,
   pub notifications_gate_passed: bool,
   pub background_sync_gate_passed: bool,
   pub screen_recording_gate_passed: bool,
   pub room_temperature_gate_passed: bool,
   pub comparable_gpu_claims_available: bool,
   pub display_sensitive_claims_available: bool,
   pub short_run_energy_equivalence_available: bool,
   pub interaction_environment_claims_available: bool,
   pub thermal_gate_passed: bool,
   pub display_gate_passed: bool,
   pub background_noise_gate_passed: bool,
   pub power_source_gate_passed: bool,
   pub opportunity_gate_passed: bool,
   pub measured_input_gate_passed: bool,
   pub presentation_calibration_gate_passed: bool,
   pub trace_overhead_gate_passed: bool,
   pub external_sensor_gate_passed: bool,
   pub all_observed_gates_passed: bool,
   pub authoritative_eligible: bool,
}

pub fn collect_macos_environment_snapshot() -> Result<MacOsEnvironmentSnapshot>
{
   let thermal = command_output("/usr/bin/pmset", &["-g", "therm"])?;
   let display = command_output("/usr/sbin/system_profiler", &["SPDisplaysDataType", "-json"])?;
   let power = command_output("/usr/bin/pmset", &["-g", "batt"])?;
   Ok(MacOsEnvironmentSnapshot {
      timestamp_unix_ns: SystemTime::now().duration_since(UNIX_EPOCH).context("macOS environment clock precedes Unix epoch")?.as_nanos().try_into().context("macOS environment timestamp exceeds u64")?,
      thermal_state: parse_thermal_state(&thermal),
      thermal_observation_sha256: sha256(&thermal),
      displays: parse_displays(&display)?,
      display_observation_sha256: sha256(&display),
      power_source: parse_power_source(&power),
      power_observation_sha256: sha256(&power),
      surface: unavailable_surface_contract(),
      fan_rpm: unavailable(),
      brightness_basis_points: unavailable(),
      luminance_millinits: unavailable(),
      true_tone_enabled: unavailable(),
      night_shift_enabled: unavailable(),
      reduce_motion_enabled: unavailable(),
      notifications_suppressed: unavailable(),
      background_sync_quiescent: unavailable(),
      screen_recording_inactive: unavailable(),
      room_temperature_millicelsius: unavailable(),
   })
}

pub fn collect_macos_pre_pair_environment(first: &MacOsCampaignSession, second: &MacOsCampaignSession) -> Result<MacOsPrePairEnvironmentObservation>
{
   super::validate_macos_pair_shape(first, second)?;
   Ok(MacOsPrePairEnvironmentObservation {
      chunk_id: first.chunk_id.clone(),
      pass_id: first.pass_id.clone(),
      pack_id: first.pack_id.clone(),
      pair_index: first.pair_index,
      snapshot: collect_macos_environment_snapshot()?,
      background: collect_macos_background_observation()?,
   })
}

pub fn collect_macos_background_observation() -> Result<MacOsBackgroundObservation>
{
   let start_timestamp_unix_ns = unix_timestamp_ns()?;
   let raw = command_output_limited(
      "/usr/bin/top",
      &["-l", "11", "-s", "1", "-F", "-o", "cpu", "-n", "10000", "-stats", "pid,cpu,idlew,command"],
      MACOS_TOP_OUTPUT_LIMIT_BYTES,
   )?;
   let end_timestamp_unix_ns = unix_timestamp_ns()?;
   let samples = parse_top_samples(&raw)?;
   let summary = reduce_background_samples(&samples)?;
   Ok(MacOsBackgroundObservation {
      start_timestamp_unix_ns,
      end_timestamp_unix_ns,
      requested_duration_ns: MACOS_BACKGROUND_OBSERVATION_SECONDS * 1_000_000_000,
      source: String::from(MACOS_BACKGROUND_SOURCE),
      sample_count: summary.sample_count,
      median_cpu_basis_points: summary.median_cpu_basis_points,
      peak_cpu_basis_points: summary.peak_cpu_basis_points,
      wakeups: available(summary.wakeups, MACOS_BACKGROUND_SOURCE),
      noisy_processes: summary.noisy_processes,
      unrelated_gpu_busy_basis_points: unavailable(),
      device_gpu_busy_basis_points: unavailable(),
      raw_observation_sha256: sha256(&raw),
   })
}

pub fn reduce_macos_acquisition_validity(campaign_root: &Path, campaign: &MacOsCampaignPlan, build_manifest_sha256: &str, before: MacOsEnvironmentSnapshot, after: MacOsEnvironmentSnapshot, sessions: &[MacOsCampaignSessionResult], instrumentation_calibration: Option<&ArtifactIdentity>) -> Result<MacOsAcquisitionValidityReport>
{
   reduce_macos_acquisition_validity_with_pre_pair_environment(campaign_root, campaign, build_manifest_sha256, before, Vec::new(), after, sessions, instrumentation_calibration)
}

pub fn reduce_macos_acquisition_validity_with_pre_pair_environment(campaign_root: &Path, campaign: &MacOsCampaignPlan, build_manifest_sha256: &str, before: MacOsEnvironmentSnapshot, pre_pair: Vec<MacOsPrePairEnvironmentObservation>, after: MacOsEnvironmentSnapshot, sessions: &[MacOsCampaignSessionResult], instrumentation_calibration: Option<&ArtifactIdentity>) -> Result<MacOsAcquisitionValidityReport>
{
   super::validate_sha256(build_manifest_sha256)?;
   let measured = sessions.iter().filter(|result| result.session.pass_role == oxide_benchmark_spec::AppleCampaignPassRole::Primary && result.session.evidence_role == oxide_benchmark_spec::AppleCampaignEvidenceRole::ClaimBearing).collect::<Vec<_>>();
   let mut opportunities = Vec::with_capacity(measured.len());
   let mut calibration_statuses = BTreeSet::new();
   for result in measured
   {
      let side = match result.session.side {ComparisonSide::Native => "native", ComparisonSide::Oxide => "oxide"};
      let expected_path = campaign_root
         .join("Runs")
         .join(&campaign.run_id)
         .join(&result.session.chunk_id)
         .join(&result.session.pass_id)
         .join(&result.session.pack_id)
         .join(result.session.pair_index.to_string())
         .join(format!("{}.presentation.correlation.json", side));
      let path = result.trace_correlation_path.as_ref().context("claim-bearing macOS session has no presentation correlation path")?;
      let expected_sha256 = result.trace_correlation_sha256.as_deref().context("claim-bearing macOS session has no presentation correlation hash")?;
      ensure!(Path::new(path) == expected_path, "macOS validity correlation path is not bound to the campaign root");
      let bytes = fs::read(path).with_context(|| format!("reading {}", path))?;
      let correlation_sha256 = sha256(&bytes);
      ensure!(expected_sha256 == correlation_sha256, "macOS validity correlation hash differs from its session result");
      let correlation: MacOsPresentationCorrelationArtifact = serde_json::from_slice(&bytes).context("decoding macOS validity presentation correlation")?;
      calibration_statuses.insert(correlation.calibration_status.clone());
      opportunities.push(MacOsOpportunityObservation {
         pair_index: result.session.pair_index,
         side: result.session.side,
         correlated_count: correlation.correlations.len() as u64,
         display_opportunity_count: correlation.correlations.iter().filter(|row| row.display_opportunity_ns.is_some()).count() as u64,
         uncorrelated_count: correlation.uncorrelated_visual_generations.len() as u64,
         correlation_sha256,
      });
   }
   opportunities.sort_by_key(|item| (item.pair_index, match item.side {ComparisonSide::Native => 0, ComparisonSide::Oxide => 1}));
   let snapshots = std::iter::once(&before).chain(pre_pair.iter().map(|observation| &observation.snapshot)).chain(std::iter::once(&after)).collect::<Vec<_>>();
   let thermal_gate_passed = snapshots.iter().all(|snapshot| snapshot.thermal_state == MacOsThermalState::Nominal);
   let display_gate_passed = stable_main_displays(&snapshots, false);
   let measured_input = collect_measured_input_evidence(campaign_root, campaign, sessions)?;
   let surface_contract_gate_passed = measured_surface_contract_gate(&measured_input);
   let pre_pair_environment_gate_passed = valid_pre_pair_environment(campaign, &pre_pair);
   let background_noise_gate_passed = pre_pair_environment_gate_passed && pre_pair.iter().all(|observation| background_within_limits(&observation.background));
   let power_source_gate_passed = snapshots.iter().all(|snapshot| snapshot.power_source == MacOsPowerSource::AcPower);
   let fan_gate_passed = snapshots.iter().all(|snapshot| observation_well_formed(&snapshot.fan_rpm));
   let brightness_gate_passed = stable_available(&snapshots, |snapshot| &snapshot.brightness_basis_points) && luminance_within_tolerance(&snapshots);
   let auto_brightness_gate_passed = snapshots.iter().all(|snapshot| main_display(snapshot).is_some_and(|display| observed_equals(&display.auto_brightness_enabled, false)));
   let true_tone_gate_passed = snapshots.iter().all(|snapshot| observed_equals(&snapshot.true_tone_enabled, false));
   let night_shift_gate_passed = snapshots.iter().all(|snapshot| observed_equals(&snapshot.night_shift_enabled, false));
   let reduce_motion_gate_passed = snapshots.iter().all(|snapshot| observed_equals(&snapshot.reduce_motion_enabled, false));
   let notifications_gate_passed = snapshots.iter().all(|snapshot| observed_equals(&snapshot.notifications_suppressed, true));
   let background_sync_gate_passed = snapshots.iter().all(|snapshot| observed_equals(&snapshot.background_sync_quiescent, true));
   let screen_recording_gate_passed = snapshots.iter().all(|snapshot| observed_equals(&snapshot.screen_recording_inactive, true));
   let room_temperature_gate_passed = room_temperature_within_tolerance(&snapshots);
   let comparable_gpu_claims_available = pre_pair_environment_gate_passed && pre_pair.iter().all(|observation| observed_at_most(&observation.background.unrelated_gpu_busy_basis_points, MACOS_BACKGROUND_GPU_LIMIT_BASIS_POINTS) && observation.background.device_gpu_busy_basis_points.availability == MacOsObservationAvailability::Available);
   let display_sensitive_claims_available = surface_contract_gate_passed && brightness_gate_passed && auto_brightness_gate_passed && true_tone_gate_passed && night_shift_gate_passed && screen_recording_gate_passed;
   let short_run_energy_equivalence_available = display_sensitive_claims_available && room_temperature_gate_passed;
   let interaction_environment_claims_available = reduce_motion_gate_passed && notifications_gate_passed && background_sync_gate_passed && screen_recording_gate_passed;
   let opportunity_gate_passed = !opportunities.is_empty() && opportunities.iter().all(|item| item.correlated_count >= MACOS_MINIMUM_CORRELATED_OPPORTUNITIES_PER_SESSION && item.display_opportunity_count == item.correlated_count && item.uncorrelated_count == 0);
   let presentation_calibration_statuses = calibration_statuses.into_iter().collect::<Vec<_>>();
   let presentation_calibration_gate_passed = !presentation_calibration_statuses.is_empty() && presentation_calibration_statuses.iter().all(|status| status.starts_with("accepted-"));
   let instrumentation_calibration_sha256 = instrumentation_calibration.map(|identity| validate_instrumentation_calibration_evidence(campaign_root, identity)).transpose()?;
   let trace_overhead_calibration_sha256 = instrumentation_calibration_sha256.clone();
   let external_sensor_calibration_sha256 = instrumentation_calibration_sha256;
   let trace_overhead_gate_passed = trace_overhead_calibration_sha256.is_some();
   let external_sensor_gate_passed = external_sensor_calibration_sha256.is_some();
   let measured_input_gate_passed = measured_input_gate(&measured_input);
   let all_observed_gates_passed = surface_contract_gate_passed && pre_pair_environment_gate_passed && thermal_gate_passed && display_gate_passed && background_noise_gate_passed && power_source_gate_passed && opportunity_gate_passed && measured_input_gate_passed && presentation_calibration_gate_passed && trace_overhead_gate_passed && external_sensor_gate_passed;
   let report = MacOsAcquisitionValidityReport {
      schema_version: 2,
      run_id: campaign.run_id.clone(),
      plan_sha256: campaign.plan_sha256.clone(),
      build_manifest_sha256: String::from(build_manifest_sha256),
      before,
      expected_pair_count: (campaign.sessions.len() / 2) as u64,
      pre_pair,
      after,
      opportunities,
      measured_input,
      presentation_calibration_statuses,
      trace_overhead_calibration_sha256,
      external_sensor_calibration_sha256,
      common_gpu_evidence_sha256: collect_common_gpu_evidence(sessions)?,
      environment_policies: macos_environment_policies(),
      surface_contract_gate_passed,
      pre_pair_environment_gate_passed,
      fan_gate_passed,
      brightness_gate_passed,
      auto_brightness_gate_passed,
      true_tone_gate_passed,
      night_shift_gate_passed,
      reduce_motion_gate_passed,
      notifications_gate_passed,
      background_sync_gate_passed,
      screen_recording_gate_passed,
      room_temperature_gate_passed,
      comparable_gpu_claims_available,
      display_sensitive_claims_available,
      short_run_energy_equivalence_available,
      interaction_environment_claims_available,
      thermal_gate_passed,
      display_gate_passed,
      background_noise_gate_passed,
      power_source_gate_passed,
      opportunity_gate_passed,
      measured_input_gate_passed,
      presentation_calibration_gate_passed,
      trace_overhead_gate_passed,
      external_sensor_gate_passed,
      all_observed_gates_passed,
      authoritative_eligible: all_observed_gates_passed,
   };
   validate_macos_acquisition_validity(&report)?;
   Ok(report)
}

fn validate_instrumentation_calibration_evidence(campaign_root: &Path, identity: &ArtifactIdentity) -> Result<String>
{
   ensure!(identity.path == "instrumentation.calibration.json", "macOS instrumentation calibration path is not campaign-root-relative");
   super::validate_sha256(&identity.sha256)?;
   let path = campaign_root.join(&identity.path);
   let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
   ensure!(sha256(&bytes) == identity.sha256, "macOS instrumentation calibration hash differs from its evidence identity");
   let report: InstrumentationCalibrationReport = serde_json::from_slice(&bytes).context("decoding macOS instrumentation calibration report")?;
   ensure!(report.schema_version == INSTRUMENTATION_CALIBRATION_SCHEMA_VERSION && report.platform_role == "macos-apple-silicon" && report.template_id == "Animation Hitches", "macOS instrumentation calibration report has the wrong platform or template identity");
   ensure!(report.sensor_sample_hz >= 1_000 && (8..=24).contains(&report.pair_count) && report.pair_count % 4 == 0, "macOS instrumentation calibration report has insufficient sensor or pair coverage");
   ensure!(report.alpha.is_finite() && report.alpha > 0.0 && report.alpha <= 0.05 && report.p50.margin_ratio == INSTRUMENTATION_CALIBRATION_P50_MARGIN_RATIO && report.p95.margin_ratio == INSTRUMENTATION_CALIBRATION_P95_MARGIN_RATIO && report.process_cpu_upper.p_value.is_finite(), "macOS instrumentation calibration report uses the wrong admission margins");
   ensure!(report.p50.accepted && report.p95.accepted && report.process_cpu_upper.p_value <= report.alpha && report.median_process_cpu_added_ratio <= INSTRUMENTATION_CALIBRATION_CPU_MARGIN_RATIO && report.accepted, "macOS instrumentation calibration report is rejected");
   super::validate_sha256(&report.input_sha256)?;
   Ok(identity.sha256.clone())
}

fn collect_common_gpu_evidence(sessions: &[MacOsCampaignSessionResult]) -> Result<Vec<String>>
{
   let mut evidence = BTreeSet::new();
   for hash in sessions.iter().filter_map(|result| result.common_gpu_artifact_sha256.as_ref())
   {
      super::validate_sha256(hash)?;
      evidence.insert(hash.clone());
   }
   Ok(evidence.into_iter().collect())
}

fn collect_measured_input_evidence(campaign_root: &Path, campaign: &MacOsCampaignPlan, sessions: &[MacOsCampaignSessionResult]) -> Result<Vec<MacOsMeasuredInputObservation>>
{
   let mut observations = Vec::new();
   for result in sessions.iter().filter(|result| result.session.pass_role == oxide_benchmark_spec::AppleCampaignPassRole::Primary && result.session.evidence_role == oxide_benchmark_spec::AppleCampaignEvidenceRole::ClaimBearing)
   {
      let side = match result.session.side {ComparisonSide::Native => "native", ComparisonSide::Oxide => "oxide"};
      let path = campaign_root
         .join("Runs")
         .join(&campaign.run_id)
         .join(&result.session.chunk_id)
         .join(&result.session.pass_id)
         .join(&result.session.pack_id)
         .join(result.session.pair_index.to_string())
         .join(format!("{}.complete.json", side));
      let bytes = fs::read(&path).with_context(|| format!("reading measured input envelope {}", path.display()))?;
      let complete_envelope_sha256 = sha256(&bytes);
      ensure!(complete_envelope_sha256 == result.artifact_sha256, "measured input envelope hash differs from its session result");
      let envelope: SessionEnvelope = serde_json::from_slice(&bytes).context("decoding measured input envelope")?;
      ensure!(envelope.run_id == campaign.run_id && envelope.plan_sha256 == campaign.plan_sha256 && envelope.chunk_id == result.session.chunk_id && envelope.pass_id == result.session.pass_id && envelope.pair_index == result.session.pair_index && envelope.side == result.session.side && envelope.generation == result.generation && envelope.pack_id == result.session.pack_id, "measured input envelope identity differs from its session result");
      let manifest_relative_path = Path::new("Runs")
         .join(&campaign.run_id)
         .join(&result.session.chunk_id)
         .join(&result.session.pass_id)
         .join(&result.session.pack_id)
         .join(result.session.pair_index.to_string())
         .join(format!("{}.trusted-input.manifest.json", side));
      let manifest_path = campaign_root.join(&manifest_relative_path);
      let recorded_path = result.trusted_input_receipt_manifest_path.as_deref().context("Primary macOS session has no trusted-input receipt manifest path")?;
      let recorded_sha256 = result.trusted_input_receipt_manifest_sha256.as_deref().context("Primary macOS session has no trusted-input receipt manifest hash")?;
      ensure!(Path::new(recorded_path) == manifest_relative_path, "trusted-input receipt manifest path is not campaign-root-relative and bound to the session");
      let manifest_bytes = fs::read(&manifest_path).with_context(|| format!("reading {}", manifest_path.display()))?;
      let manifest_sha256 = sha256(&manifest_bytes);
      ensure!(recorded_sha256 == manifest_sha256, "trusted-input receipt manifest hash differs from its session result");
      let manifest: MacOsTrustedInputReceiptManifest = serde_json::from_slice(&manifest_bytes).context("decoding trusted-input receipt manifest")?;
      ensure!(manifest.schema_version == 1 && manifest.run_id == campaign.run_id && manifest.plan_sha256 == campaign.plan_sha256 && manifest.chunk_id == result.session.chunk_id && manifest.pass_id == result.session.pass_id && manifest.pack_id == result.session.pack_id && manifest.pair_index == result.session.pair_index && manifest.side == result.session.side && manifest.generation == result.generation && manifest.expected_command_count == manifest.receipts.len() as u64 && manifest.complete, "trusted-input receipt manifest identity or completeness differs from its session");
      let surface_path = result.surface_receipt_path.as_ref().context("Primary macOS session has no surface receipt path")?;
      let surface_recorded_sha256 = result.surface_receipt_sha256.as_ref().context("Primary macOS session has no surface receipt hash")?;
      let surface_bytes = fs::read(surface_path).with_context(|| format!("reading {}", surface_path))?;
      let surface_sha256 = sha256(&surface_bytes);
      ensure!(&surface_sha256 == surface_recorded_sha256 && envelope.surface_receipt.sha256 == surface_sha256, "macOS surface receipt hash differs from its session result or complete envelope");
      let surface: MacOsSurfaceReceipt = serde_json::from_slice(&surface_bytes).context("decoding acquisition macOS surface receipt")?;
      ensure!(surface.run_id == campaign.run_id && surface.plan_sha256 == campaign.plan_sha256 && surface.chunk_id == result.session.chunk_id && surface.pass_id == result.session.pass_id && surface.pack_id == result.session.pack_id && surface.pair_index == result.session.pair_index && surface.side == result.session.side && surface.generation == result.generation, "macOS surface receipt identity differs from measured input");
      observations.push(MacOsMeasuredInputObservation {
         pair_index: result.session.pair_index,
         side: result.session.side,
         complete_envelope_sha256,
         injection_scope: envelope.injection_scope,
         validation: envelope.validation,
         raw_application_receipt_status: String::from(MACOS_RAW_APPLICATION_RECEIPT_STATUS_COMPLETE),
         raw_application_receipt_manifest: Some(ArtifactIdentity {path: manifest_relative_path.to_string_lossy().into_owned(), sha256: manifest_sha256}),
         surface_receipt: ArtifactIdentity {path: envelope.surface_receipt.path, sha256: surface_sha256},
         surface_contract: surface_contract_observation(&surface.snapshot, surface.observed_refresh_millihz),
      });
   }
   observations.sort_by_key(|item| (item.pair_index, match item.side {ComparisonSide::Native => 0, ComparisonSide::Oxide => 1}));
   Ok(observations)
}

fn surface_contract_observation(surface: &MacOsSurfaceSnapshot, observed_refresh_millihz: Option<u64>) -> MacOsSurfaceContractObservation
{
   let source = "app-reported-hash-bound-surface-receipt-v1";
   MacOsSurfaceContractObservation {
      window_logical_size: available(format!("{}x{}-millipoints", surface.window_logical_width_milli_points, surface.window_logical_height_milli_points), source),
      viewport_logical_size: available(format!("{}x{}-millipoints", surface.viewport_logical_width_milli_points, surface.viewport_logical_height_milli_points), source),
      backing_pixel_size: available(format!("{}x{}-pixels", surface.backing_pixel_width, surface.backing_pixel_height), source),
      content_insets: available(format!("{}:{}:{}:{}-millipoints", surface.inset_top_milli_points, surface.inset_left_milli_points, surface.inset_bottom_milli_points, surface.inset_right_milli_points), source),
      backing_scale_milli: available(surface.backing_scale_milli, source),
      color_format: available(surface.final_color_format.clone(), source),
      color_space: available(surface.final_color_space.clone(), source),
      alpha_mode: available(surface.alpha_mode.clone(), source),
      sample_count: available(surface.sample_count, source),
      compositor_scaling: available(surface.compositor_scaling.clone(), source),
      target_refresh_policy: available(surface.target_refresh_policy.clone(), source),
      target_refresh_millihz: available(surface.target_refresh_millihz, source),
      observed_refresh_millihz: observed_refresh_millihz.map(|value| available(value, source)).unwrap_or_else(unavailable),
      surface_implementation: available(surface.surface_implementation.clone(), source),
      internal_color_format: available(surface.internal_color_format.clone(), source),
   }
}

fn measured_input_gate(observations: &[MacOsMeasuredInputObservation]) -> bool
{
   let scope_ready = !observations.is_empty() && observations.iter().all(|item| item.injection_scope == MACOS_AUTHORITATIVE_INPUT_SCOPE);
   let receipts_ready = observations.iter().all(|item| item.raw_application_receipt_status == MACOS_RAW_APPLICATION_RECEIPT_STATUS_COMPLETE && item.raw_application_receipt_manifest.is_some());
   scope_ready && receipts_ready
}

fn measured_surface_contract_gate(observations: &[MacOsMeasuredInputObservation]) -> bool
{
   if observations.is_empty() || observations.len() % 2 != 0
   {
      return false;
   }
   let first = &observations[0].surface_contract;
   observations.iter().all(|observation| exact_surface_fields_equal(&observation.surface_contract, first) && observed_refresh_within_tolerance(&observation.surface_contract, first) && surface_contract_available(&observation.surface_contract))
}

fn exact_surface_fields_equal(left: &MacOsSurfaceContractObservation, right: &MacOsSurfaceContractObservation) -> bool
{
   left.window_logical_size == right.window_logical_size
      && left.viewport_logical_size == right.viewport_logical_size
      && left.backing_pixel_size == right.backing_pixel_size
      && left.content_insets == right.content_insets
      && left.backing_scale_milli == right.backing_scale_milli
      && left.color_format == right.color_format
      && left.color_space == right.color_space
      && left.alpha_mode == right.alpha_mode
      && left.sample_count == right.sample_count
      && left.compositor_scaling == right.compositor_scaling
      && left.target_refresh_policy == right.target_refresh_policy
      && left.target_refresh_millihz == right.target_refresh_millihz
}

fn observed_refresh_within_tolerance(left: &MacOsSurfaceContractObservation, right: &MacOsSurfaceContractObservation) -> bool
{
   let (Some(left), Some(right)) = (left.observed_refresh_millihz.value, right.observed_refresh_millihz.value) else {return false};
   let difference = left.abs_diff(right) as u128;
   difference * 10_000 <= u128::from(left.max(right)) * 50
}

fn surface_contract_available(surface: &MacOsSurfaceContractObservation) -> bool
{
   [&surface.window_logical_size, &surface.viewport_logical_size, &surface.backing_pixel_size, &surface.content_insets, &surface.color_format, &surface.color_space, &surface.alpha_mode, &surface.compositor_scaling, &surface.target_refresh_policy, &surface.surface_implementation, &surface.internal_color_format].iter().all(|value| value.availability == MacOsObservationAvailability::Available && value.value.as_ref().is_some_and(|value| !value.is_empty()))
      && [&surface.backing_scale_milli, &surface.sample_count, &surface.target_refresh_millihz, &surface.observed_refresh_millihz].iter().all(|value| value.availability == MacOsObservationAvailability::Available && value.value.is_some_and(|value| value > 0))
}

pub fn validate_macos_acquisition_validity(report: &MacOsAcquisitionValidityReport) -> Result<()>
{
   ensure!(report.schema_version == 2 && !report.run_id.is_empty(), "macOS acquisition validity identity is incomplete");
   super::validate_sha256(&report.plan_sha256)?;
   super::validate_sha256(&report.build_manifest_sha256)?;
   for hash in [&report.before.thermal_observation_sha256, &report.before.display_observation_sha256, &report.before.power_observation_sha256, &report.after.thermal_observation_sha256, &report.after.display_observation_sha256, &report.after.power_observation_sha256]
   {
      super::validate_sha256(hash)?;
   }
   ensure!(report.expected_pair_count > 0, "macOS acquisition validity has no frozen campaign pair count");
   ensure!(pre_pair_observations_well_formed(&report.pre_pair), "macOS pre-pair environment observations are malformed or duplicated");
   for observation in &report.pre_pair
   {
      for hash in [&observation.snapshot.thermal_observation_sha256, &observation.snapshot.display_observation_sha256, &observation.snapshot.power_observation_sha256, &observation.background.raw_observation_sha256]
      {
         super::validate_sha256(hash)?;
      }
   }
   for opportunity in &report.opportunities
   {
      super::validate_sha256(&opportunity.correlation_sha256)?;
   }
   for observation in &report.measured_input
   {
      super::validate_sha256(&observation.complete_envelope_sha256)?;
      ensure!(!observation.surface_receipt.path.is_empty(), "macOS measured surface receipt path is empty");
      super::validate_sha256(&observation.surface_receipt.sha256)?;
      match (&*observation.raw_application_receipt_status, &observation.raw_application_receipt_manifest)
      {
         (MACOS_RAW_APPLICATION_RECEIPT_STATUS_COMPLETE, Some(identity)) =>
         {
            ensure!(!identity.path.is_empty(), "macOS raw application receipt manifest path is empty");
            super::validate_sha256(&identity.sha256)?;
         }
         (MACOS_RAW_APPLICATION_RECEIPT_STATUS_UNAVAILABLE, None) => (),
         _ => bail!("macOS raw application receipt status and identity disagree"),
      }
   }
   for hash in report.common_gpu_evidence_sha256.iter().chain(report.trace_overhead_calibration_sha256.iter()).chain(report.external_sensor_calibration_sha256.iter())
   {
      super::validate_sha256(hash)?;
   }
   ensure!(report.environment_policies == macos_environment_policies(), "macOS environment comparison policies differ from the schema-owned contract");
   let snapshots = std::iter::once(&report.before).chain(report.pre_pair.iter().map(|observation| &observation.snapshot)).chain(std::iter::once(&report.after)).collect::<Vec<_>>();
   let expected_thermal = snapshots.iter().all(|snapshot| snapshot.thermal_state == MacOsThermalState::Nominal);
   let expected_display = stable_main_displays(&snapshots, false);
   let expected_surface = measured_surface_contract_gate(&report.measured_input);
   let expected_pre_pair = report.expected_pair_count == report.pre_pair.len() as u64 && pre_pair_observations_complete(&report.pre_pair);
   let expected_noise = expected_pre_pair && report.pre_pair.iter().all(|observation| background_within_limits(&observation.background));
   let expected_power = snapshots.iter().all(|snapshot| snapshot.power_source == MacOsPowerSource::AcPower);
   let expected_fan = snapshots.iter().all(|snapshot| observation_well_formed(&snapshot.fan_rpm));
   let expected_brightness = stable_available(&snapshots, |snapshot| &snapshot.brightness_basis_points) && luminance_within_tolerance(&snapshots);
   let expected_auto_brightness = snapshots.iter().all(|snapshot| main_display(snapshot).is_some_and(|display| observed_equals(&display.auto_brightness_enabled, false)));
   let expected_true_tone = snapshots.iter().all(|snapshot| observed_equals(&snapshot.true_tone_enabled, false));
   let expected_night_shift = snapshots.iter().all(|snapshot| observed_equals(&snapshot.night_shift_enabled, false));
   let expected_reduce_motion = snapshots.iter().all(|snapshot| observed_equals(&snapshot.reduce_motion_enabled, false));
   let expected_notifications = snapshots.iter().all(|snapshot| observed_equals(&snapshot.notifications_suppressed, true));
   let expected_background_sync = snapshots.iter().all(|snapshot| observed_equals(&snapshot.background_sync_quiescent, true));
   let expected_screen_recording = snapshots.iter().all(|snapshot| observed_equals(&snapshot.screen_recording_inactive, true));
   let expected_room_temperature = room_temperature_within_tolerance(&snapshots);
   let expected_comparable_gpu = expected_pre_pair && report.pre_pair.iter().all(|observation| observed_at_most(&observation.background.unrelated_gpu_busy_basis_points, MACOS_BACKGROUND_GPU_LIMIT_BASIS_POINTS) && observation.background.device_gpu_busy_basis_points.availability == MacOsObservationAvailability::Available);
   let expected_display_sensitive = expected_surface && expected_brightness && expected_auto_brightness && expected_true_tone && expected_night_shift && expected_screen_recording;
   let expected_short_run_energy = expected_display_sensitive && expected_room_temperature;
   let expected_interaction_environment = expected_reduce_motion && expected_notifications && expected_background_sync && expected_screen_recording;
   let expected_opportunities = !report.opportunities.is_empty() && report.opportunities.iter().all(|item| item.correlated_count >= MACOS_MINIMUM_CORRELATED_OPPORTUNITIES_PER_SESSION && item.display_opportunity_count == item.correlated_count && item.uncorrelated_count == 0);
   let expected_measured_input = measured_input_gate(&report.measured_input);
   let expected_presentation_calibration = !report.presentation_calibration_statuses.is_empty() && report.presentation_calibration_statuses.iter().all(|status| status.starts_with("accepted-"));
   ensure!(report.trace_overhead_calibration_sha256 == report.external_sensor_calibration_sha256, "macOS trace-overhead and external-sensor gates must share one calibration evidence identity");
   let expected_trace_overhead = report.trace_overhead_calibration_sha256.is_some();
   let expected_external_sensor = report.external_sensor_calibration_sha256.is_some();
   ensure!(report.surface_contract_gate_passed == expected_surface && report.pre_pair_environment_gate_passed == expected_pre_pair && report.fan_gate_passed == expected_fan && report.brightness_gate_passed == expected_brightness && report.auto_brightness_gate_passed == expected_auto_brightness && report.true_tone_gate_passed == expected_true_tone && report.night_shift_gate_passed == expected_night_shift && report.reduce_motion_gate_passed == expected_reduce_motion && report.notifications_gate_passed == expected_notifications && report.background_sync_gate_passed == expected_background_sync && report.screen_recording_gate_passed == expected_screen_recording && report.room_temperature_gate_passed == expected_room_temperature && report.comparable_gpu_claims_available == expected_comparable_gpu && report.display_sensitive_claims_available == expected_display_sensitive && report.short_run_energy_equivalence_available == expected_short_run_energy && report.interaction_environment_claims_available == expected_interaction_environment, "macOS environment contract gate flags differ from observed evidence");
   ensure!(report.thermal_gate_passed == expected_thermal && report.display_gate_passed == expected_display && report.background_noise_gate_passed == expected_noise && report.power_source_gate_passed == expected_power && report.opportunity_gate_passed == expected_opportunities && report.measured_input_gate_passed == expected_measured_input && report.presentation_calibration_gate_passed == expected_presentation_calibration && report.trace_overhead_gate_passed == expected_trace_overhead && report.external_sensor_gate_passed == expected_external_sensor, "macOS acquisition validity gate flags differ from observed evidence");
   let expected_all = expected_surface && expected_pre_pair && expected_thermal && expected_display && expected_noise && expected_power && expected_opportunities && expected_measured_input && expected_presentation_calibration && expected_trace_overhead && expected_external_sensor;
   ensure!(report.all_observed_gates_passed == expected_all && report.authoritative_eligible == expected_all, "macOS acquisition authoritative eligibility differs from the complete observed gate conjunction");
   Ok(())
}

fn stable_main_displays(snapshots: &[&MacOsEnvironmentSnapshot], full_surface: bool) -> bool
{
   let Some(first) = snapshots.first().and_then(|snapshot| main_display(snapshot)) else {return false};
   if first.display_id.is_empty() || first.logical_width == 0 || first.logical_height == 0 || first.pixel_width == 0 || first.pixel_height == 0 || first.scale_x_milli == 0 || first.scale_y_milli == 0 || first.refresh_millihz == 0
   {
      return false;
   }
   let display_stable = snapshots.iter().all(|snapshot| {
      main_display(snapshot).is_some_and(|display| {
         let essential = display.display_id == first.display_id && display.mode == first.mode && display.logical_width == first.logical_width && display.logical_height == first.logical_height && display.pixel_width == first.pixel_width && display.pixel_height == first.pixel_height && display.scale_x_milli == first.scale_x_milli && display.scale_y_milli == first.scale_y_milli && display.refresh_millihz == first.refresh_millihz && display.rotation_degrees == first.rotation_degrees && display.mirrored == first.mirrored && display.connection_type == first.connection_type && display.display_type == first.display_type;
         essential && stable_when_available(&display.color_profile, &first.color_profile) && stable_when_available(&display.edr_headroom_milli, &first.edr_headroom_milli)
      })
   });
   display_stable && (!full_surface || stable_surface_contracts(snapshots))
}

fn stable_surface_contracts(snapshots: &[&MacOsEnvironmentSnapshot]) -> bool
{
   let Some(first) = snapshots.first().map(|snapshot| &snapshot.surface) else {return false};
   snapshots.iter().all(|snapshot| {
      let surface = &snapshot.surface;
      stable_observation(&surface.window_logical_size, &first.window_logical_size)
         && stable_observation(&surface.viewport_logical_size, &first.viewport_logical_size)
         && stable_observation(&surface.backing_pixel_size, &first.backing_pixel_size)
         && stable_observation(&surface.content_insets, &first.content_insets)
         && stable_observation(&surface.backing_scale_milli, &first.backing_scale_milli)
         && stable_observation(&surface.color_format, &first.color_format)
         && stable_observation(&surface.color_space, &first.color_space)
         && stable_observation(&surface.alpha_mode, &first.alpha_mode)
         && stable_observation(&surface.sample_count, &first.sample_count)
         && stable_observation(&surface.compositor_scaling, &first.compositor_scaling)
         && stable_observation(&surface.target_refresh_policy, &first.target_refresh_policy)
         && stable_observation(&surface.target_refresh_millihz, &first.target_refresh_millihz)
         && stable_observation(&surface.observed_refresh_millihz, &first.observed_refresh_millihz)
   })
}

fn main_display(snapshot: &MacOsEnvironmentSnapshot) -> Option<&MacOsDisplayObservation>
{
   let mut displays = snapshot.displays.iter().filter(|display| display.main && display.online && !display.asleep);
   let display = displays.next()?;
   displays.next().is_none().then_some(display)
}

fn valid_pre_pair_environment(campaign: &MacOsCampaignPlan, observations: &[MacOsPrePairEnvironmentObservation]) -> bool
{
   if campaign.sessions.len() % 2 != 0 || !pre_pair_observations_complete(observations)
   {
      return false;
   }
   let expected = campaign.sessions.chunks_exact(2).map(|pair| (pair[0].chunk_id.clone(), pair[0].pass_id.clone(), pair[0].pack_id.clone(), pair[0].pair_index)).collect::<BTreeSet<_>>();
   let observed = observations.iter().map(|observation| (observation.chunk_id.clone(), observation.pass_id.clone(), observation.pack_id.clone(), observation.pair_index)).collect::<BTreeSet<_>>();
   expected.len() == campaign.sessions.len() / 2 && expected == observed
}

fn pre_pair_observations_complete(observations: &[MacOsPrePairEnvironmentObservation]) -> bool
{
   !observations.is_empty() && pre_pair_observations_well_formed(observations)
}

fn pre_pair_observations_well_formed(observations: &[MacOsPrePairEnvironmentObservation]) -> bool
{
   let identities = observations.iter().map(|observation| (&observation.chunk_id, &observation.pass_id, &observation.pack_id, observation.pair_index)).collect::<BTreeSet<_>>();
   identities.len() == observations.len() && observations.iter().all(|observation| {
      observation.background.source == MACOS_BACKGROUND_SOURCE
         && observation.background.sample_count == MACOS_BACKGROUND_SAMPLE_COUNT as u64
         && observation.background.requested_duration_ns == MACOS_BACKGROUND_OBSERVATION_SECONDS * 1_000_000_000
         && observation.background.end_timestamp_unix_ns >= observation.background.start_timestamp_unix_ns
         && observation.background.wakeups.availability == MacOsObservationAvailability::Available
         && observation.background.wakeups.value.is_some()
   })
}

fn stable_available<T, F>(snapshots: &[&MacOsEnvironmentSnapshot], select: F) -> bool where T: Eq, F: Fn(&MacOsEnvironmentSnapshot) -> &MacOsEnvironmentValue<T>
{
   let Some(first) = snapshots.first().map(|snapshot| select(snapshot)) else {return false};
   first.availability == MacOsObservationAvailability::Available && first.value.is_some() && snapshots.iter().all(|snapshot| stable_observation(select(snapshot), first))
}

fn stable_observation<T: Eq>(left: &MacOsEnvironmentValue<T>, right: &MacOsEnvironmentValue<T>) -> bool
{
   left.availability == MacOsObservationAvailability::Available && right.availability == MacOsObservationAvailability::Available && left.value.is_some() && left.value == right.value
}

fn stable_when_available<T: Eq>(left: &MacOsEnvironmentValue<T>, right: &MacOsEnvironmentValue<T>) -> bool
{
   match (left.availability, right.availability)
   {
      (MacOsObservationAvailability::Available, MacOsObservationAvailability::Available) => left.value.is_some() && left.value == right.value,
      (MacOsObservationAvailability::Unavailable, MacOsObservationAvailability::Unavailable) => left.value.is_none() && right.value.is_none(),
      _ => false,
   }
}

fn observation_well_formed<T>(observation: &MacOsEnvironmentValue<T>) -> bool
{
   !observation.source.is_empty() && match observation.availability
   {
      MacOsObservationAvailability::Available => observation.value.is_some(),
      MacOsObservationAvailability::Unavailable => observation.value.is_none(),
   }
}

fn observed_equals<T: Eq>(observation: &MacOsEnvironmentValue<T>, expected: T) -> bool
{
   observation.availability == MacOsObservationAvailability::Available && observation.value.as_ref() == Some(&expected)
}

fn observed_at_most(observation: &MacOsEnvironmentValue<u64>, maximum: u64) -> bool
{
   observation.availability == MacOsObservationAvailability::Available && observation.value.is_some_and(|value| value <= maximum)
}

fn luminance_within_tolerance(snapshots: &[&MacOsEnvironmentSnapshot]) -> bool
{
   let Some(reference) = snapshots.first().and_then(|snapshot| snapshot.luminance_millinits.value) else {return false};
   let tolerance = 5_000u64.max(reference.saturating_mul(200) / 10_000);
   snapshots.iter().all(|snapshot| {
      snapshot.luminance_millinits.availability == MacOsObservationAvailability::Available && snapshot.luminance_millinits.value.is_some_and(|value| value.abs_diff(reference) <= tolerance)
   })
}

fn room_temperature_within_tolerance(snapshots: &[&MacOsEnvironmentSnapshot]) -> bool
{
   snapshots.iter().all(|snapshot| {
      snapshot.room_temperature_millicelsius.availability == MacOsObservationAvailability::Available && snapshot.room_temperature_millicelsius.value.is_some_and(|value| (20_000..=24_000).contains(&value))
   })
}

fn background_within_limits(observation: &MacOsBackgroundObservation) -> bool
{
   observation.median_cpu_basis_points <= MACOS_BACKGROUND_MEDIAN_CPU_LIMIT_BASIS_POINTS
      && observation.peak_cpu_basis_points <= MACOS_BACKGROUND_PEAK_CPU_LIMIT_BASIS_POINTS
      && observed_at_most(&observation.wakeups, MACOS_BACKGROUND_WAKEUP_LIMIT)
}

pub fn macos_environment_policies() -> BTreeMap<String, MacOsEnvironmentFieldPolicy>
{
   let mut policies = BTreeMap::new();
   policies.insert(String::from("surface-contract"), exact_policy("typed-surface-fields"));
   policies.insert(String::from("surface-implementation"), record_only_policy("implementation-identity"));
   policies.insert(String::from("surface-internal-format"), record_only_policy("implementation-format"));
   policies.insert(String::from("observed-refresh"), tolerance_policy("millihz", None, 0, Some(50)));
   policies.insert(String::from("power-source"), exact_policy("enum"));
   policies.insert(String::from("thermal-state-class"), exact_policy("enum"));
   policies.insert(String::from("brightness-setting"), exact_policy("basis-points"));
   policies.insert(String::from("auto-brightness"), exact_policy("boolean"));
   policies.insert(String::from("true-tone"), exact_policy("boolean"));
   policies.insert(String::from("night-shift"), exact_policy("boolean"));
   policies.insert(String::from("reduce-motion"), exact_policy("boolean"));
   policies.insert(String::from("notifications-suppressed"), exact_policy("boolean"));
   policies.insert(String::from("background-sync-quiescent"), exact_policy("boolean"));
   policies.insert(String::from("screen-recording-inactive"), exact_policy("boolean"));
   policies.insert(String::from("fan-rpm"), record_only_policy("rpm"));
   policies.insert(String::from("measured-luminance"), tolerance_policy("millinits", None, 5_000, Some(200)));
   policies.insert(String::from("room-temperature"), tolerance_policy("millicelsius", Some(22_000), 2_000, None));
   policies.insert(String::from("pre-run-unrelated-cpu-median"), tolerance_policy("basis-points-of-one-core", Some(0), MACOS_BACKGROUND_MEDIAN_CPU_LIMIT_BASIS_POINTS, None));
   policies.insert(String::from("pre-run-unrelated-cpu-peak"), tolerance_policy("basis-points-of-one-core", Some(0), MACOS_BACKGROUND_PEAK_CPU_LIMIT_BASIS_POINTS, None));
   policies.insert(String::from("pre-run-unrelated-wakeups"), tolerance_policy("wakeups-per-10-seconds", Some(0), MACOS_BACKGROUND_WAKEUP_LIMIT, None));
   policies.insert(String::from("pre-run-unrelated-gpu"), tolerance_policy("active-basis-points", Some(0), MACOS_BACKGROUND_GPU_LIMIT_BASIS_POINTS, None));
   policies
}

fn exact_policy(unit: &str) -> MacOsEnvironmentFieldPolicy
{
   MacOsEnvironmentFieldPolicy {comparison: MacOsEnvironmentComparisonPolicy::Exact, unit: String::from(unit), target: None, absolute_tolerance: None, relative_tolerance_basis_points: None}
}

fn tolerance_policy(unit: &str, target: Option<i64>, absolute_tolerance: u64, relative_tolerance_basis_points: Option<u64>) -> MacOsEnvironmentFieldPolicy
{
   MacOsEnvironmentFieldPolicy {comparison: MacOsEnvironmentComparisonPolicy::Tolerance, unit: String::from(unit), target, absolute_tolerance: Some(absolute_tolerance), relative_tolerance_basis_points}
}

fn record_only_policy(unit: &str) -> MacOsEnvironmentFieldPolicy
{
   MacOsEnvironmentFieldPolicy {comparison: MacOsEnvironmentComparisonPolicy::RecordOnly, unit: String::from(unit), target: None, absolute_tolerance: None, relative_tolerance_basis_points: None}
}

fn available<T>(value: T, source: &str) -> MacOsEnvironmentValue<T>
{
   MacOsEnvironmentValue {availability: MacOsObservationAvailability::Available, value: Some(value), source: String::from(source)}
}

fn unavailable<T>() -> MacOsEnvironmentValue<T>
{
   MacOsEnvironmentValue {availability: MacOsObservationAvailability::Unavailable, value: None, source: String::from(MACOS_UNAVAILABLE_SOURCE)}
}

fn unavailable_surface_contract() -> MacOsSurfaceContractObservation
{
   MacOsSurfaceContractObservation {
      window_logical_size: unavailable(),
      viewport_logical_size: unavailable(),
      backing_pixel_size: unavailable(),
      content_insets: unavailable(),
      backing_scale_milli: unavailable(),
      color_format: unavailable(),
      color_space: unavailable(),
      alpha_mode: unavailable(),
      sample_count: unavailable(),
      compositor_scaling: unavailable(),
      target_refresh_policy: unavailable(),
      target_refresh_millihz: unavailable(),
      observed_refresh_millihz: unavailable(),
      surface_implementation: unavailable(),
      internal_color_format: unavailable(),
   }
}

fn unix_timestamp_ns() -> Result<u64>
{
   SystemTime::now().duration_since(UNIX_EPOCH).context("macOS environment clock precedes Unix epoch")?.as_nanos().try_into().context("macOS environment timestamp exceeds u64")
}

fn parse_thermal_state(bytes: &[u8]) -> MacOsThermalState
{
   let value = String::from_utf8_lossy(bytes).to_ascii_lowercase();
   if value.contains("no thermal warning level") && value.contains("no performance warning level") {MacOsThermalState::Nominal}
   else if value.contains("critical") {MacOsThermalState::Critical}
   else if value.contains("warning") {MacOsThermalState::Warning}
   else {MacOsThermalState::Unknown}
}

fn parse_power_source(bytes: &[u8]) -> MacOsPowerSource
{
   let value = String::from_utf8_lossy(bytes);
   if value.contains("Now drawing from 'AC Power'") {MacOsPowerSource::AcPower}
   else if value.contains("Now drawing from 'Battery Power'") {MacOsPowerSource::BatteryPower}
   else if value.contains("UPS Power") {MacOsPowerSource::UpsPower}
   else {MacOsPowerSource::Unknown}
}

fn parse_displays(bytes: &[u8]) -> Result<Vec<MacOsDisplayObservation>>
{
   let root: serde_json::Value = serde_json::from_slice(bytes).context("decoding system_profiler display observation")?;
   let adapters = root.get("SPDisplaysDataType").and_then(serde_json::Value::as_array).context("system_profiler display observation has no adapter list")?;
   let mut displays = Vec::new();
   for adapter in adapters
   {
      for display in adapter.get("spdisplays_ndrvs").and_then(serde_json::Value::as_array).into_iter().flatten()
      {
         let mode = display.get("_spdisplays_resolution").and_then(serde_json::Value::as_str).unwrap_or("");
         let pixels = display.get("_spdisplays_pixels").and_then(serde_json::Value::as_str).unwrap_or("");
         let (logical_width, logical_height) = parse_dimensions(mode).unwrap_or((0, 0));
         let (pixel_width, pixel_height) = parse_dimensions(pixels).unwrap_or((0, 0));
         let auto_brightness_enabled = match display.get("spdisplays_ambient_brightness").and_then(serde_json::Value::as_str)
         {
            Some("spdisplays_yes") => available(true, "system-profiler-SPDisplaysDataType-spdisplays_ambient_brightness-v1"),
            Some("spdisplays_no") => available(false, "system-profiler-SPDisplaysDataType-spdisplays_ambient_brightness-v1"),
            _ => unavailable(),
         };
         displays.push(MacOsDisplayObservation {
            display_id: display.get("_spdisplays_displayID").and_then(serde_json::Value::as_str).unwrap_or("").to_owned(),
            mode: String::from(mode),
            logical_width,
            logical_height,
            pixel_width,
            pixel_height,
            scale_x_milli: scale_milli(pixel_width, logical_width),
            scale_y_milli: scale_milli(pixel_height, logical_height),
            refresh_millihz: parse_refresh_millihz(mode).unwrap_or(0),
            rotation_degrees: display.get("spdisplays_rotation").and_then(serde_json::Value::as_str).and_then(|value| value.parse().ok()).unwrap_or(0),
            mirrored: display.get("spdisplays_mirror").and_then(serde_json::Value::as_str) == Some("spdisplays_on"),
            connection_type: display.get("spdisplays_connection_type").and_then(serde_json::Value::as_str).unwrap_or("").to_owned(),
            display_type: display.get("spdisplays_display_type").and_then(serde_json::Value::as_str).unwrap_or("").to_owned(),
            auto_brightness_enabled,
            color_profile: unavailable(),
            edr_headroom_milli: unavailable(),
            main: display.get("spdisplays_main").and_then(serde_json::Value::as_str) == Some("spdisplays_yes"),
            online: display.get("spdisplays_online").and_then(serde_json::Value::as_str) == Some("spdisplays_yes"),
            asleep: display.get("spdisplays_asleep").and_then(serde_json::Value::as_str) == Some("spdisplays_yes"),
         });
      }
   }
   ensure!(!displays.is_empty(), "system_profiler reported no displays");
   Ok(displays)
}

fn parse_refresh_millihz(mode: &str) -> Option<u64>
{
   let (_, refresh) = mode.rsplit_once('@')?;
   let value = refresh.trim().strip_suffix("Hz")?.trim().parse::<f64>().ok()?;
   (value.is_finite() && value > 0.0).then(|| (value * 1_000.0).round() as u64)
}

fn parse_dimensions(value: &str) -> Option<(u64, u64)>
{
   let dimensions = value.split_once('@').map(|(dimensions, _)| dimensions).unwrap_or(value).trim();
   let (width, height) = dimensions.split_once('x')?;
   Some((width.trim().parse().ok()?, height.trim().parse().ok()?))
}

fn scale_milli(pixels: u64, logical: u64) -> u64
{
   if logical == 0 {0} else {pixels.saturating_mul(1_000).saturating_add(logical / 2) / logical}
}

#[derive(Clone)]
struct TopProcessSample
{
   pid: u32,
   cpu_basis_points: u64,
   idle_wakeups: u64,
   command: String,
}

struct BackgroundSummary
{
   sample_count: u64,
   median_cpu_basis_points: u64,
   peak_cpu_basis_points: u64,
   wakeups: u64,
   noisy_processes: Vec<MacOsNoisyProcess>,
}

fn parse_top_samples(bytes: &[u8]) -> Result<Vec<Vec<TopProcessSample>>>
{
   let text = std::str::from_utf8(bytes).context("top observation is not UTF-8")?;
   let mut samples = Vec::new();
   let mut current: Option<Vec<TopProcessSample>> = None;
   for line in text.lines()
   {
      let trimmed = line.trim();
      if trimmed.starts_with("PID ") && trimmed.contains("%CPU") && trimmed.contains("IDLEW")
      {
         if let Some(sample) = current.take()
         {
            samples.push(sample);
         }
         current = Some(Vec::new());
         continue;
      }
      let Some(sample) = current.as_mut() else {continue};
      let mut fields = trimmed.split_whitespace();
      let Some(pid) = fields.next().and_then(|value| value.trim_end_matches('*').parse::<u32>().ok()) else {continue};
      let Some(cpu) = fields.next().and_then(|value| value.parse::<f64>().ok()).filter(|value| value.is_finite() && *value >= 0.0) else {continue};
      let Some(idle_wakeups) = fields.next().and_then(|value| value.parse::<u64>().ok()) else {continue};
      let command = fields.collect::<Vec<_>>().join(" ");
      if pid != std::process::id() && command != "top"
      {
         sample.push(TopProcessSample {pid, cpu_basis_points: (cpu * 100.0).round() as u64, idle_wakeups, command});
      }
   }
   if let Some(sample) = current
   {
      samples.push(sample);
   }
   ensure!(samples.len() >= MACOS_BACKGROUND_SAMPLE_COUNT + 1, "top observation returned {} process samples, expected at least {}", samples.len(), MACOS_BACKGROUND_SAMPLE_COUNT + 1);
   Ok(samples.split_off(samples.len() - MACOS_BACKGROUND_SAMPLE_COUNT))
}

fn reduce_background_samples(samples: &[Vec<TopProcessSample>]) -> Result<BackgroundSummary>
{
   ensure!(samples.len() == MACOS_BACKGROUND_SAMPLE_COUNT, "background observation sample count is not ten");
   let mut totals = samples.iter().map(|sample| sample.iter().fold(0u64, |total, process| total.saturating_add(process.cpu_basis_points))).collect::<Vec<_>>();
   let peak_cpu_basis_points = totals.iter().copied().max().unwrap_or(0);
   let median_cpu_basis_points = median(&mut totals);
   let mut processes = BTreeMap::<(u32, String), Vec<(u64, u64)>>::new();
   for sample in samples
   {
      for process in sample
      {
         processes.entry((process.pid, process.command.clone())).or_default().push((process.cpu_basis_points, process.idle_wakeups));
      }
   }
   let mut wakeups = 0u64;
   let mut noisy_processes = Vec::new();
   for ((pid, command), values) in processes
   {
      let mut cpu = values.iter().map(|(cpu, _)| *cpu).collect::<Vec<_>>();
      let median_cpu_basis_points = median(&mut cpu);
      let peak_cpu_basis_points = cpu.iter().copied().max().unwrap_or(0);
      let process_wakeups = values.first().zip(values.last()).map(|(first, last)| last.1.saturating_sub(first.1)).unwrap_or(0);
      wakeups = wakeups.saturating_add(process_wakeups);
      if median_cpu_basis_points > MACOS_BACKGROUND_MEDIAN_CPU_LIMIT_BASIS_POINTS || peak_cpu_basis_points > MACOS_BACKGROUND_PEAK_CPU_LIMIT_BASIS_POINTS
      {
         noisy_processes.push(MacOsNoisyProcess {pid, median_cpu_basis_points, peak_cpu_basis_points, wakeups: process_wakeups, command});
      }
   }
   noisy_processes.sort_by(|left, right| right.median_cpu_basis_points.cmp(&left.median_cpu_basis_points).then_with(|| right.peak_cpu_basis_points.cmp(&left.peak_cpu_basis_points)).then_with(|| left.pid.cmp(&right.pid)));
   Ok(BackgroundSummary {sample_count: samples.len() as u64, median_cpu_basis_points, peak_cpu_basis_points, wakeups, noisy_processes})
}

fn median(values: &mut [u64]) -> u64
{
   if values.is_empty()
   {
      return 0;
   }
   values.sort_unstable();
   let middle = values.len() / 2;
   if values.len() % 2 == 0 {values[middle - 1].saturating_add(values[middle]) / 2} else {values[middle]}
}

fn command_output(program: &str, arguments: &[&str]) -> Result<Vec<u8>>
{
   let output = Command::new(program).args(arguments).output().with_context(|| format!("launching macOS validity observer {}", program))?;
   if !output.status.success()
   {
      bail!("macOS validity observer {} failed with {}", program, output.status);
   }
   ensure!(!output.stdout.is_empty(), "macOS validity observer {} returned no output", program);
   Ok(output.stdout)
}

fn command_output_limited(program: &str, arguments: &[&str], maximum_bytes: u64) -> Result<Vec<u8>>
{
   let mut child = Command::new(program).args(arguments).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().with_context(|| format!("launching bounded macOS validity observer {}", program))?;
   let mut stdout = child.stdout.take().context("bounded macOS validity observer has no stdout pipe")?;
   let mut bytes = Vec::new();
   stdout.by_ref().take(maximum_bytes.saturating_add(1)).read_to_end(&mut bytes).with_context(|| format!("reading bounded macOS validity observer {}", program))?;
   if bytes.len() as u64 > maximum_bytes
   {
      let _ = child.kill();
      let _ = child.wait();
      bail!("macOS validity observer {} exceeded its {}-byte output limit", program, maximum_bytes);
   }
   let status = child.wait().with_context(|| format!("waiting for bounded macOS validity observer {}", program))?;
   ensure!(status.success(), "bounded macOS validity observer {} failed with {}", program, status);
   ensure!(!bytes.is_empty(), "bounded macOS validity observer {} returned no output", program);
   Ok(bytes)
}

fn sha256(bytes: &[u8]) -> String
{
   format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests
{
   use oxide_benchmark_spec::{AppleCampaignEvidenceRole, AppleCampaignPassRole, ComparisonOrder, DecimalU64};
   use serde_json::json;

   use super::*;
   use crate::MacOsCampaignSession;

   #[test]
   fn common_gpu_evidence_includes_attribution_sessions()
   {
      let expected = sha256(b"attribution-common-gpu");
      let sessions = vec![result(AppleCampaignPassRole::Primary, None), result(AppleCampaignPassRole::Attribution, Some(expected.clone()))];
      assert_eq!(collect_common_gpu_evidence(&sessions).expect("common-GPU evidence"), vec![expected]);
   }

   #[test]
   fn measured_input_rereads_diagnostic_envelope_and_complete_receipt_manifest()
   {
      let root = tempfile::tempdir().expect("temporary input evidence root");
      let mut measured = result(AppleCampaignPassRole::Primary, None);
      let campaign = MacOsCampaignPlan {
         schema_version: 2,
         run_id: String::from("run"),
         plan_sha256: sha256(b"plan"),
         seed: DecimalU64(1),
         plan_resource_path: None,
         sessions: vec![measured.session.clone()],
      };
      measured.generation = sha256(b"generation");
      let directory = root.path().join("Runs/run/chunk/pass/pack/0");
      fs::create_dir_all(&directory).expect("input evidence directory");
      let bytes = serde_json::to_vec(&json!({
         "schemaVersion": 1,
         "runID": campaign.run_id,
         "planSHA256": campaign.plan_sha256,
         "chunkID": "chunk",
         "passID": "pass",
         "pairIndex": 0,
         "side": "native",
         "generation": measured.generation,
         "packID": "pack",
         "telemetrySHA256": sha256(b"telemetry"),
         "telemetryByteCount": 1,
         "timebaseNumerator": 1,
         "timebaseDenominator": 1,
         "injectionScope": "direct-callback-diagnostic",
         "validation": "complete-diagnostic-not-claim-bearing"
      })).expect("input envelope bytes");
      fs::write(directory.join("native.complete.json"), &bytes).expect("input envelope");
      measured.artifact_sha256 = sha256(&bytes);
      let manifest = MacOsTrustedInputReceiptManifest {
         schema_version: 1,
         run_id: campaign.run_id.clone(),
         plan_sha256: campaign.plan_sha256.clone(),
         chunk_id: String::from("chunk"),
         pass_id: String::from("pass"),
         pack_id: String::from("pack"),
         pair_index: 0,
         side: ComparisonSide::Native,
         generation: measured.generation.clone(),
         expected_command_count: 0,
         receipts: Vec::new(),
         complete: true,
      };
      let mut manifest_bytes = serde_json::to_vec_pretty(&manifest).expect("receipt manifest bytes");
      manifest_bytes.push(b'\n');
      fs::write(directory.join("native.trusted-input.manifest.json"), &manifest_bytes).expect("receipt manifest");
      measured.trusted_input_receipt_manifest_path = Some(String::from("Runs/run/chunk/pass/pack/0/native.trusted-input.manifest.json"));
      measured.trusted_input_receipt_manifest_sha256 = Some(sha256(&manifest_bytes));
      let observations = collect_measured_input_evidence(root.path(), &campaign, &[measured]).expect("measured input observation");
      assert_eq!(observations[0].injection_scope, "direct-callback-diagnostic");
      assert_eq!(observations[0].raw_application_receipt_status, MACOS_RAW_APPLICATION_RECEIPT_STATUS_COMPLETE);
      assert!(observations[0].raw_application_receipt_manifest.is_some());
      assert!(!measured_input_gate(&observations));
   }

   fn result(pass_role: AppleCampaignPassRole, common_gpu_artifact_sha256: Option<String>) -> MacOsCampaignSessionResult
   {
      MacOsCampaignSessionResult {
         session: MacOsCampaignSession {
            chunk_id: String::from("chunk"),
            pass_id: String::from("pass"),
            pack_id: String::from("pack"),
            pass_role,
            evidence_role: AppleCampaignEvidenceRole::ClaimBearing,
            collector: None,
            launch_class: None,
            timing: None,
            pair_index: 0,
            order: ComparisonOrder::Ab,
            side: ComparisonSide::Native,
            max_occupied_seconds: 1,
            scale_overlay: None,
         },
         generation: String::from("generation"),
         executable_sha256: sha256(b"executable"),
         artifact_sha256: sha256(b"artifact"),
         acknowledgement_sha256: sha256(b"acknowledgement"),
         surface_receipt_path: None,
         surface_receipt_sha256: None,
         trace_path: None,
         trace_toc_sha256: None,
         trace_signposts_sha256: None,
         trace_updates_sha256: None,
         trace_frame_lifetimes_sha256: None,
         trace_correlation_path: None,
         trace_correlation_sha256: None,
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
         common_gpu_artifact_sha256,
         launch_evidence_path: None,
         launch_evidence_sha256: None,
         energy_config_sha256: None,
         energy_request_sha256: None,
         energy_ready_sha256: None,
         energy_raw_path: None,
         energy_raw_sha256: None,
         energy_summary_path: None,
         energy_summary_sha256: None,
         resource_path: String::from("resource"),
         resource_sha256: sha256(b"resource"),
         resource_availability: String::from("test"),
         disposition: String::from("fresh"),
      }
   }
}
