use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use oxide_benchmark_spec::{
   validate_comparison_plan, ArtifactIdentity, ComparisonOrder, ComparisonPlan,
   ComparisonSession, DecimalU64, MetricDefinition, RawObservationRow,
   RawObservationTimestamp, RawObservationValue, InstrumentationCalibrationReport,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use super::resource::{validate_resource_artifact, MacOsResourceArtifact, MacOsResourceIdentity};
use super::{
   reduce_macos_common_gpu, reduce_macos_energy, reduce_macos_resource_artifact,
   validate_macos_launch_evidence, MacOsCommonGpuSamples, MacOsCommonGpuSummary,
   MacOsEnergySummary, MacOsExternalMeterRawArtifact, MacOsLaunchClass,
   MacOsLaunchEvidence, MacOsLaunchExpectation, MacOsLaunchPresentationCorrelation,
   validate_macos_acquisition_validity, validate_macos_pair_shape,
   validate_macos_surface_receipt, validate_macos_telemetry_coverage, validate_sha256,
   ComparisonSide, MacOsAcquisitionValidityReport, MacOsCampaignPlan, MacOsCampaignReport,
   MacOsCampaignScope, MacOsCampaignSessionResult, MacOsPairCheckpoint, MacOsSurfaceReceipt,
   MacOsPresentationCorrelationArtifact, MacOsTrustedInputReceiptManifest, SessionEnvelope,
};

pub const MACOS_INPUT_TO_PRESENT_METRIC_SOURCE: &str = "macos-correlated-input-to-frame-lifetime-end-ns";
pub const MACOS_VISUAL_TO_PRESENT_METRIC_SOURCE: &str = "macos-correlated-visual-to-frame-lifetime-end-ns";
pub const MACOS_RESOURCE_METRIC_SOURCE_PREFIX: &str = "macos-rusage-v4-resource-summary:";
pub const MACOS_LAUNCH_METRIC_SOURCE_PREFIX: &str = "macos-launch-evidence-summary:";
pub const MACOS_COMMON_GPU_METRIC_SOURCE_PREFIX: &str = "macos-task-power-v2-common-gpu-summary:";
pub const MACOS_ENERGY_METRIC_SOURCE_PREFIX: &str = "macos-direct-meter-energy-summary:";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsAnalyzerBundleManifest
{
   pub schema_version: u32,
   pub campaign_plan_sha256: String,
   pub campaign_report_sha256: String,
   pub analyzer_plan_artifact_sha256: String,
   pub sessions_artifact_sha256: String,
   pub raw_artifact_sha256: BTreeMap<String, String>,
   pub session_count: DecimalU64,
}

pub fn materialize_macos_analyzer_bundle(campaign_root: &Path, analyzer_plan_path: &Path, output_root: &Path) -> Result<MacOsAnalyzerBundleManifest>
{
   ensure!(campaign_root.is_dir(), "macOS analyzer bridge campaign root is not a directory: {}", campaign_root.display());
   ensure!(!output_root.exists(), "macOS analyzer bundle destination already exists: {}", output_root.display());
   let campaign_plan_path = campaign_root.join("campaign.plan.json");
   let campaign_report_path = campaign_root.join("campaign.complete.json");
   let campaign_plan_bytes = fs::read(&campaign_plan_path).with_context(|| format!("reading {}", campaign_plan_path.display()))?;
   let campaign_report_bytes = fs::read(&campaign_report_path).with_context(|| format!("reading {}", campaign_report_path.display()))?;
   let analyzer_plan_bytes = fs::read(analyzer_plan_path).with_context(|| format!("reading {}", analyzer_plan_path.display()))?;
   let campaign: MacOsCampaignPlan = serde_json::from_slice(&campaign_plan_bytes).context("decoding materialized macOS campaign plan")?;
   let report: MacOsCampaignReport = serde_json::from_slice(&campaign_report_bytes).context("decoding completed macOS campaign report")?;
   let analyzer: ComparisonPlan = serde_json::from_slice(&analyzer_plan_bytes).context("decoding macOS generic analyzer plan")?;
   validate_bridge_contract(campaign_root, &campaign, &report, &analyzer)?;

   let pack = &analyzer.scenario_packs[0];
   let mut selected = report.sessions.iter()
      .filter(|result| result.session.pass_id == analyzer.measurement_pass_id && result.session.pack_id == pack.id)
      .collect::<Vec<_>>();
   selected.sort_by_key(|result| {
      let side_index = match result.session.order
      {
         ComparisonOrder::Ab => u8::from(result.session.side == ComparisonSide::Oxide),
         ComparisonOrder::Ba => u8::from(result.session.side == ComparisonSide::Native),
      };
      (result.session.pair_index, side_index)
   });
   ensure!(selected.len() == analyzer.pass_pair_count.0 as usize * 2, "macOS campaign does not contain exactly two selected sessions per analyzer pair");

   let mut raw_artifacts = Vec::with_capacity(selected.len());
   let mut comparison_sessions = Vec::with_capacity(selected.len());
   let mut predecessor_checkpoint_sha256 = None;
   for (checkpoint_generation, pair) in selected.chunks_exact(2).enumerate()
   {
      let first = pair[0];
      let second = pair[1];
      validate_macos_pair_shape(&first.session, &second.session)?;
      ensure!(first.session.pair_index as u64 == checkpoint_generation as u64, "macOS selected analyzer pairs are not contiguous from zero");
      let (checkpoint_sha256, checkpoint) = load_pair_checkpoint(campaign_root, &campaign, first)?;
      ensure!(checkpoint.build_manifest_sha256 == report.build_manifest_sha256, "macOS analyzer checkpoint build identity differs from the completed campaign report");
      ensure!(checkpoint.sessions == vec![first.clone(), second.clone()], "macOS analyzer bridge checkpoint sessions differ from the completed campaign report");
      if let Some(predecessor) = predecessor_checkpoint_sha256.as_deref()
      {
         ensure!(checkpoint.predecessor_pair_sha256.as_deref() == Some(predecessor), "macOS analyzer bridge selected checkpoint chain is discontinuous");
      }
      predecessor_checkpoint_sha256 = Some(checkpoint_sha256.clone());
      for result in pair
      {
         let converted = convert_session(
            campaign_root,
            &campaign,
            &report,
            &analyzer,
            result,
            checkpoint_generation as u64 + 1,
            &checkpoint_sha256,
         )?;
         raw_artifacts.push((converted.raw_path, converted.raw_bytes));
         comparison_sessions.push(converted.session);
      }
   }

   let sessions_bytes = json_lines(&comparison_sessions)?;
   let staging = staging_path(output_root)?;
   let publication = (|| -> Result<MacOsAnalyzerBundleManifest> {
      fs::create_dir_all(staging.join("raw")).with_context(|| format!("creating {}", staging.display()))?;
      fs::write(staging.join("plan.json"), &analyzer_plan_bytes).context("writing staged macOS analyzer plan")?;
      fs::write(staging.join("sessions.jsonl"), &sessions_bytes).context("writing staged macOS analyzer sessions")?;
      let mut raw_hashes = BTreeMap::new();
      for (relative, bytes) in &raw_artifacts
      {
         fs::write(staging.join(relative), bytes).with_context(|| format!("writing staged macOS analyzer raw artifact {}", relative.display()))?;
         raw_hashes.insert(relative.to_string_lossy().into_owned(), sha256(bytes));
      }
      let manifest = MacOsAnalyzerBundleManifest {
         schema_version: 1,
         campaign_plan_sha256: sha256(&campaign_plan_bytes),
         campaign_report_sha256: sha256(&campaign_report_bytes),
         analyzer_plan_artifact_sha256: sha256(&analyzer_plan_bytes),
         sessions_artifact_sha256: sha256(&sessions_bytes),
         raw_artifact_sha256: raw_hashes,
         session_count: DecimalU64(comparison_sessions.len() as u64),
      };
      let mut manifest_bytes = serde_json::to_vec_pretty(&manifest).context("encoding macOS analyzer bundle manifest")?;
      manifest_bytes.push(b'\n');
      fs::write(staging.join("manifest.json"), manifest_bytes).context("writing staged macOS analyzer manifest")?;
      fs::rename(&staging, output_root).with_context(|| format!("publishing macOS analyzer bundle {}", output_root.display()))?;
      Ok(manifest)
   })();
   if publication.is_err() && staging.exists()
   {
      let _ = fs::remove_dir_all(&staging);
   }
   publication
}

struct ConvertedSession
{
   session: ComparisonSession,
   raw_path: PathBuf,
   raw_bytes: Vec<u8>,
}

fn validate_bridge_contract(campaign_root: &Path, campaign: &MacOsCampaignPlan, report: &MacOsCampaignReport, analyzer: &ComparisonPlan) -> Result<()>
{
   validate_comparison_plan(analyzer).context("validating macOS generic analyzer plan")?;
   ensure!(campaign.schema_version == 2, "macOS analyzer bridge requires materialized campaign schema 2");
   ensure!(report.schema_version == 4 && report.scope == MacOsCampaignScope::Full, "macOS analyzer bridge requires a schema-4 Full campaign report");
   ensure!(report.complete && report.acquisition_complete && report.correctness_accepted && report.correctness_eligible_for_measured_acquisition && report.authoritative_eligible, "macOS campaign is not complete, correctness-accepted, and authoritative-eligible");
   ensure!(report.run_id == campaign.run_id && report.plan_sha256 == campaign.plan_sha256, "macOS campaign plan and report identities differ");
   ensure!(report.sessions.len() == campaign.sessions.len() && report.sessions.iter().zip(&campaign.sessions).all(|(result, session)| result.session == *session), "macOS campaign report session sequence differs from its materialized plan");
   ensure!(analyzer.platform == oxide_benchmark_spec::Platform::Apple, "macOS analyzer plan must use the Apple platform");
   ensure!(analyzer.plan_sha256 == campaign.plan_sha256 && analyzer.seed == campaign.seed, "macOS analyzer plan does not bind the source campaign content identity and seed");
   ensure!(analyzer.reference.id == "native.production" && analyzer.contender.id == "oxide.production", "macOS analyzer implementation roles must be native.production reference and oxide.production contender");
   ensure!(analyzer.reference.comparator_acceptance_status == "accepted", "macOS analyzer reference comparator is not accepted");
   let validity_identity = report.acquisition_validity.as_ref().context("authoritative macOS campaign report has no acquisition validity identity")?;
   let validity_path = campaign_root.join("acquisition.validity.json");
   ensure!(Path::new(&validity_identity.path) == validity_path, "macOS acquisition validity path is not bound to the campaign root");
   let validity_bytes = fs::read(&validity_path).with_context(|| format!("reading {}", validity_path.display()))?;
   ensure!(sha256(&validity_bytes) == validity_identity.sha256, "macOS acquisition validity artifact hash differs from the campaign report");
   let validity: MacOsAcquisitionValidityReport = serde_json::from_slice(&validity_bytes).context("decoding macOS acquisition validity artifact")?;
   validate_macos_acquisition_validity(&validity)?;
   ensure!(validity.run_id == report.run_id && validity.plan_sha256 == report.plan_sha256 && validity.build_manifest_sha256 == report.build_manifest_sha256 && validity.authoritative_eligible, "macOS acquisition validity artifact differs from the authoritative campaign report");
   validate_validity_correlations(campaign_root, report, &validity)?;
   validate_validity_input_evidence(campaign_root, report, &validity)?;
   validate_validity_instrumentation_calibration(campaign_root, &validity)?;
   ensure!(analyzer.scenario_packs.len() == 1, "macOS analyzer bridge requires exactly one selected scenario pack per bundle");
   let pack = &analyzer.scenario_packs[0];
   ensure!(analyzer.controller_chunks.iter().all(|chunk| chunk.pass_id == analyzer.measurement_pass_id && chunk.pack_ids == vec![pack.id.clone()]), "macOS analyzer chunks must select only the bundle's one pass and pack");
   ensure!(analyzer.comparison_cells.iter().all(|cell| cell.owning_pass_id == analyzer.measurement_pass_id && cell.pack_id == pack.id), "macOS analyzer cells must belong to the selected pass and pack");
   let required_metric_ids = analyzer.comparison_cells.iter().flat_map(|cell| std::iter::once(&cell.primary_metric_id).chain(cell.required_guardrail_metric_ids.iter())).collect::<BTreeSet<_>>();
   for metric_id in required_metric_ids
   {
      let metric = analyzer.metric_definitions.iter().find(|metric| &metric.id == metric_id).context("validated macOS analyzer metric disappeared")?;
      validate_metric_source(metric, &analyzer.measurement_pass_id)?;
      if metric.source.starts_with(MACOS_RESOURCE_METRIC_SOURCE_PREFIX)
      {
         ensure!(pack.ordered_scenario_ids.len() == 1, "whole-session macOS resource metric {} cannot be assigned to a multi-scenario pack", metric.id);
      }
      if metric.source.starts_with(MACOS_COMMON_GPU_METRIC_SOURCE_PREFIX)
      {
         ensure!(analyzer.comparison_cells.iter().filter(|cell| cell.primary_metric_id == metric.id || cell.required_guardrail_metric_ids.contains(&metric.id)).all(|cell| cell.evidence_role == oxide_benchmark_spec::EvidenceRole::DescriptiveDiagnostic), "comparison-ineligible macOS common-GPU metric {} cannot enter a required-claim cell", metric.id);
      }
   }
   Ok(())
}

fn validate_validity_instrumentation_calibration(campaign_root: &Path, validity: &MacOsAcquisitionValidityReport) -> Result<()>
{
   let expected_sha256 = validity.trace_overhead_calibration_sha256.as_deref().context("macOS validity has no trace-overhead calibration identity")?;
   ensure!(validity.external_sensor_calibration_sha256.as_deref() == Some(expected_sha256), "macOS validity does not use one calibration identity for tracing and external sensing");
   let path = campaign_root.join("instrumentation.calibration.json");
   let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
   ensure!(sha256(&bytes) == expected_sha256, "macOS instrumentation calibration report hash differs from acquisition validity");
   let report: InstrumentationCalibrationReport = serde_json::from_slice(&bytes).context("decoding validity-bound instrumentation calibration report")?;
   ensure!(report.accepted && report.platform_role == "macos-apple-silicon" && report.template_id == "Animation Hitches", "macOS instrumentation calibration report is rejected or has the wrong acquisition identity");
   Ok(())
}

fn validate_validity_input_evidence(campaign_root: &Path, report: &MacOsCampaignReport, validity: &MacOsAcquisitionValidityReport) -> Result<()>
{
   let measured = report.sessions.iter().filter(|result| result.session.pass_role == oxide_benchmark_spec::AppleCampaignPassRole::Primary && result.session.evidence_role == oxide_benchmark_spec::AppleCampaignEvidenceRole::ClaimBearing).collect::<Vec<_>>();
   ensure!(validity.measured_input.len() == measured.len(), "macOS acquisition validity input coverage differs from the primary claim-bearing sessions");
   let mut seen = BTreeSet::new();
   for result in measured
   {
      let side = match result.session.side {ComparisonSide::Native => "native", ComparisonSide::Oxide => "oxide"};
      ensure!(seen.insert((result.session.pair_index, side)), "macOS acquisition validity contains duplicate primary input identity");
      let observation = validity.measured_input.iter().find(|item| item.pair_index == result.session.pair_index && item.side == result.session.side).context("macOS acquisition validity omitted a primary input observation")?;
      let directory = campaign_root
         .join("Runs")
         .join(&report.run_id)
         .join(&result.session.chunk_id)
         .join(&result.session.pass_id)
         .join(&result.session.pack_id)
         .join(result.session.pair_index.to_string());
      let envelope_path = directory.join(format!("{}.complete.json", side));
      let bytes = fs::read(&envelope_path).with_context(|| format!("reading {}", envelope_path.display()))?;
      let hash = sha256(&bytes);
      ensure!(result.artifact_sha256 == hash && observation.complete_envelope_sha256 == hash, "macOS acquisition validity complete-envelope identity differs from measured session evidence");
      let envelope: SessionEnvelope = serde_json::from_slice(&bytes).context("decoding validity-bound measured input envelope")?;
      ensure!(observation.injection_scope == envelope.injection_scope && observation.validation == envelope.validation, "macOS acquisition validity input scope differs from the complete envelope");
      if let Some(identity) = &observation.raw_application_receipt_manifest
      {
         let path = Path::new(&identity.path);
         let expected_relative = Path::new("Runs")
            .join(&report.run_id)
            .join(&result.session.chunk_id)
            .join(&result.session.pass_id)
            .join(&result.session.pack_id)
            .join(result.session.pair_index.to_string())
            .join(format!("{}.trusted-input.manifest.json", side));
         ensure!(path == expected_relative, "macOS raw application receipt manifest is not campaign-root-relative and bound to its session directory");
         let manifest_path = campaign_root.join(path);
         let receipt_bytes = fs::read(&manifest_path).with_context(|| format!("reading {}", manifest_path.display()))?;
         ensure!(sha256(&receipt_bytes) == identity.sha256, "macOS raw application receipt manifest hash differs from validity evidence");
         let manifest: MacOsTrustedInputReceiptManifest = serde_json::from_slice(&receipt_bytes).context("decoding validity-bound raw application receipt manifest")?;
         ensure!(manifest.schema_version == 1 && manifest.run_id == report.run_id && manifest.plan_sha256 == report.plan_sha256 && manifest.chunk_id == result.session.chunk_id && manifest.pass_id == result.session.pass_id && manifest.pack_id == result.session.pack_id && manifest.pair_index == result.session.pair_index && manifest.side == result.session.side && manifest.generation == result.generation && manifest.expected_command_count == manifest.receipts.len() as u64 && manifest.complete, "macOS raw application receipt manifest identity differs from the measured session");
         validate_raw_receipt_hash_closure(&directory, side, &manifest)?;
      }
   }
   Ok(())
}

fn validate_raw_receipt_hash_closure(directory: &Path, side: &str, manifest: &MacOsTrustedInputReceiptManifest) -> Result<()>
{
   let prefix = format!("{}.trusted-input.", side);
   let manifest_name = format!("{}.trusted-input.manifest.json", side);
   let mut expected_names = BTreeSet::new();
   for (index, receipt) in manifest.receipts.iter().enumerate()
   {
      let sequence = u64::try_from(index).context("macOS trusted-input receipt sequence exceeds u64")?;
      ensure!(receipt.command_sequence == sequence && !receipt.scenario_id.is_empty(), "macOS raw application receipt manifest sequence is not contiguous");
      for (suffix, expected_sha256) in [
         ("request.json", receipt.request_sha256.as_str()),
         ("controller.json", receipt.controller_receipt_sha256.as_str()),
         ("application.json", receipt.application_receipt_sha256.as_str()),
      ]
      {
         validate_sha256(expected_sha256)?;
         let name = format!("{}{}.{}", prefix, sequence, suffix);
         expected_names.insert(name.clone());
         let raw_path = directory.join(&name);
         let raw_bytes = fs::read(&raw_path).with_context(|| format!("reading {}", raw_path.display()))?;
         ensure!(sha256(&raw_bytes) == expected_sha256, "macOS raw trusted-input artifact hash differs from its manifest: {}", raw_path.display());
      }
   }
   for entry in fs::read_dir(directory).with_context(|| format!("reading {}", directory.display()))?
   {
      let name = entry?.file_name().to_string_lossy().into_owned();
      if name.starts_with(&prefix) && name != manifest_name && !expected_names.remove(&name)
      {
         bail!("macOS trusted-input session contains an extra or duplicate raw artifact: {}", name);
      }
   }
   ensure!(expected_names.is_empty(), "macOS trusted-input session is missing raw artifacts named by its manifest: {:?}", expected_names);
   Ok(())
}

fn validate_validity_correlations(campaign_root: &Path, report: &MacOsCampaignReport, validity: &MacOsAcquisitionValidityReport) -> Result<()>
{
   let measured = report.sessions.iter().filter(|result| result.session.pass_role == oxide_benchmark_spec::AppleCampaignPassRole::Primary && result.session.evidence_role == oxide_benchmark_spec::AppleCampaignEvidenceRole::ClaimBearing).collect::<Vec<_>>();
   ensure!(validity.opportunities.len() == measured.len(), "macOS acquisition validity opportunity coverage differs from the primary claim-bearing sessions");
   let mut seen = BTreeSet::new();
   let mut calibration_statuses = BTreeSet::new();
   for result in measured
   {
      let side = match result.session.side {ComparisonSide::Native => "native", ComparisonSide::Oxide => "oxide"};
      let key = (result.session.pair_index, side);
      ensure!(seen.insert(key), "macOS acquisition validity contains duplicate primary session identity");
      let opportunity = validity.opportunities.iter().find(|item| item.pair_index == result.session.pair_index && item.side == result.session.side).context("macOS acquisition validity omitted a primary claim-bearing session")?;
      let expected_path = campaign_root
         .join("Runs")
         .join(&report.run_id)
         .join(&result.session.chunk_id)
         .join(&result.session.pass_id)
         .join(&result.session.pack_id)
         .join(result.session.pair_index.to_string())
         .join(format!("{}.presentation.correlation.json", side));
      ensure!(result.trace_correlation_path.as_deref() == expected_path.to_str(), "macOS acquisition validity correlation path is not bound to the campaign root");
      let bytes = fs::read(&expected_path).with_context(|| format!("reading {}", expected_path.display()))?;
      let hash = sha256(&bytes);
      ensure!(result.trace_correlation_sha256.as_deref() == Some(hash.as_str()) && opportunity.correlation_sha256 == hash, "macOS acquisition validity correlation identity differs from measured session evidence");
      let correlation: MacOsPresentationCorrelationArtifact = serde_json::from_slice(&bytes).context("decoding validity-bound macOS presentation correlation")?;
      ensure!(opportunity.correlated_count == correlation.correlations.len() as u64 && opportunity.display_opportunity_count == correlation.correlations.iter().filter(|row| row.display_opportunity_ns.is_some()).count() as u64 && opportunity.uncorrelated_count == correlation.uncorrelated_visual_generations.len() as u64, "macOS acquisition validity opportunity counts differ from correlation evidence");
      calibration_statuses.insert(correlation.calibration_status);
   }
   ensure!(validity.presentation_calibration_statuses == calibration_statuses.into_iter().collect::<Vec<_>>(), "macOS acquisition validity calibration identities differ from correlation evidence");
   Ok(())
}

fn validate_metric_source(metric: &MetricDefinition, measurement_pass_id: &str) -> Result<()>
{
   ensure!(metric.owning_pass_id == measurement_pass_id, "macOS analyzer metric {} does not belong to the selected pass", metric.id);
   let expected_unit = match metric.source.as_str()
   {
      MACOS_INPUT_TO_PRESENT_METRIC_SOURCE | MACOS_VISUAL_TO_PRESENT_METRIC_SOURCE => "ns",
      source if source.starts_with(MACOS_RESOURCE_METRIC_SOURCE_PREFIX) => resource_metric_unit(source)?,
      source if source.starts_with(MACOS_LAUNCH_METRIC_SOURCE_PREFIX) => launch_metric_unit(source)?,
      source if source.starts_with(MACOS_COMMON_GPU_METRIC_SOURCE_PREFIX) => common_gpu_metric_unit(source)?,
      source if source.starts_with(MACOS_ENERGY_METRIC_SOURCE_PREFIX) => energy_metric_unit(source)?,
      _ => bail!("macOS analyzer bridge does not support required metric {} source {}", metric.id, metric.source),
   };
   ensure!(metric.unit == expected_unit, "macOS analyzer metric {} source {} requires unit {}, not {}", metric.id, metric.source, expected_unit, metric.unit);
   Ok(())
}

fn source_field<'a>(source: &'a str, prefix: &str) -> Result<&'a str>
{
   source.strip_prefix(prefix).filter(|field| !field.is_empty()).context("macOS analyzer metric source has no summary field")
}

fn resource_metric_unit(source: &str) -> Result<&'static str>
{
   match source_field(source, MACOS_RESOURCE_METRIC_SOURCE_PREFIX)?
   {
      "wall-time-ns" | "user-cpu-ns" | "system-cpu-ns" | "runnable-time-ns" => Ok("ns"),
      "process-cpu-ms-per-wall-s" => Ok("ms/s"),
      "wakeups" | "pageins" | "logical-writes" | "instructions" | "cycles" => Ok("count"),
      "wakeups-per-wall-s" => Ok("count/s"),
      "disk-read-bytes" | "disk-written-bytes" | "wired-start-bytes" | "wired-end-bytes" | "wired-peak-bytes"
      | "resident-start-bytes" | "resident-end-bytes" | "resident-peak-bytes" | "physical-footprint-start-bytes"
      | "physical-footprint-end-bytes" | "physical-footprint-peak-bytes" => Ok("bytes"),
      "retained-slope-bytes-per-min" => Ok("bytes/min"),
      field => bail!("macOS analyzer bridge does not support resource-summary field {}", field),
   }
}

fn launch_metric_unit(source: &str) -> Result<&'static str>
{
   match source_field(source, MACOS_LAUNCH_METRIC_SOURCE_PREFIX)?
   {
      "launch-request-to-first-complete-ui-generation-ns"
      | "launch-request-to-first-attributed-present-proxy-ns"
      | "input-request-to-response-generation-ns"
      | "input-request-to-response-attributed-present-proxy-ns" => Ok("ns"),
      field => bail!("macOS analyzer bridge does not support launch-summary field {}", field),
   }
}

fn common_gpu_metric_unit(source: &str) -> Result<&'static str>
{
   match source_field(source, MACOS_COMMON_GPU_METRIC_SOURCE_PREFIX)?
   {
      "phase-gpu-time-ns" => Ok("ns"),
      field => bail!("macOS analyzer bridge does not support common-GPU summary field {}", field),
   }
}

fn energy_metric_unit(source: &str) -> Result<&'static str>
{
   match source_field(source, MACOS_ENERGY_METRIC_SOURCE_PREFIX)?
   {
      "phase-joules" | "phase-baseline-adjusted-joules" => Ok("J"),
      "phase-average-watts" | "phase-baseline-adjusted-average-watts" => Ok("W"),
      field => bail!("macOS analyzer bridge does not support direct-energy summary field {}", field),
   }
}

fn convert_session(campaign_root: &Path, campaign: &MacOsCampaignPlan, report: &MacOsCampaignReport, analyzer: &ComparisonPlan, result: &MacOsCampaignSessionResult, checkpoint_generation: u64, checkpoint_sha256: &str) -> Result<ConvertedSession>
{
   let implementation_id = match result.session.side
   {
      ComparisonSide::Native => &analyzer.reference.id,
      ComparisonSide::Oxide => &analyzer.contender.id,
   };
   let expected_executable = match result.session.side
   {
      ComparisonSide::Native => &analyzer.reference.executable_or_bundle_sha256,
      ComparisonSide::Oxide => &analyzer.contender.executable_or_bundle_sha256,
   };
   ensure!(&result.executable_sha256 == expected_executable, "macOS campaign executable identity differs from analyzer implementation {}", implementation_id);
   for hash in [&result.artifact_sha256, &result.acknowledgement_sha256, checkpoint_sha256]
   {
      validate_sha256(hash)?;
   }
   ensure!(result.disposition == "fresh" || result.disposition == "resumed" || result.disposition == "resumed-pair" || result.disposition == "recovered-before-pair-checkpoint", "macOS campaign session has unsupported disposition {}", result.disposition);

   let expected_directory = campaign_root
      .join("Runs")
      .join(&campaign.run_id)
      .join(&result.session.chunk_id)
      .join(&result.session.pass_id)
      .join(&result.session.pack_id)
      .join(result.session.pair_index.to_string());
   let side = match result.session.side {ComparisonSide::Native => "native", ComparisonSide::Oxide => "oxide"};
   let resource_path = expected_directory.join(format!("{}.resources.json", side));
   ensure!(Path::new(&result.resource_path) == resource_path, "macOS campaign resource path is not bound to the selected campaign root");
   let resource_bytes = fs::read(&resource_path).with_context(|| format!("reading {}", resource_path.display()))?;
   ensure!(sha256(&resource_bytes) == result.resource_sha256, "macOS campaign resource artifact hash differs from its session result");
   let resource: MacOsResourceArtifact = serde_json::from_slice(&resource_bytes).context("decoding macOS analyzer resource evidence")?;
   let identity = MacOsResourceIdentity {
      plan: campaign,
      session: &result.session,
      generation: &result.generation,
      executable_sha256: &result.executable_sha256,
      pid: resource.pid,
      launch_t0: resource.launch_t0,
   };
   validate_resource_artifact(&resource, &identity)?;
   ensure!(result.resource_availability == resource.availability, "macOS campaign resource availability differs from its artifact");

   let scenario_ids = if let Some(timing) = result.session.timing.as_ref()
   {
      timing.scenarios.iter().map(|scenario| scenario.scenario_id.as_str()).collect::<Vec<_>>()
   }
   else
   {
      analyzer.scenario_packs[0].ordered_scenario_ids.iter().map(String::as_str).collect::<Vec<_>>()
   };
   ensure!(scenario_ids == analyzer.scenario_packs[0].ordered_scenario_ids.iter().map(String::as_str).collect::<Vec<_>>(), "macOS campaign scenario order differs from its analyzer pack");
   let selected_metric_ids = analyzer.comparison_cells.iter().flat_map(|cell| std::iter::once(&cell.primary_metric_id).chain(cell.required_guardrail_metric_ids.iter())).collect::<BTreeSet<_>>();
   let required_metrics = analyzer.metric_definitions.iter().filter(|metric| selected_metric_ids.contains(&metric.id)).collect::<Vec<_>>();
   let mut rows = Vec::new();
   let mut artifact_hashes = BTreeMap::new();
   artifact_hashes.insert(String::from("application-complete"), result.artifact_sha256.clone());
   artifact_hashes.insert(String::from("application-acknowledgement"), result.acknowledgement_sha256.clone());
   artifact_hashes.insert(String::from("atomic-pair-checkpoint"), String::from(checkpoint_sha256));
   artifact_hashes.insert(String::from("process-resource"), result.resource_sha256.clone());
   validate_session_publication_closure(campaign_root, campaign, result, &expected_directory, side, &mut artifact_hashes)?;
   let mut session_start_ticks = resource.process_start_continuous_time;
   let mut session_end_ticks = resource.durable_complete_timestamp;
   let mut session_timebase_numerator = resource.timebase_numerator;
   let mut session_timebase_denominator = resource.timebase_denominator;

   let presentation_metrics = required_metrics.iter().filter(|metric| matches!(metric.source.as_str(), MACOS_INPUT_TO_PRESENT_METRIC_SOURCE | MACOS_VISUAL_TO_PRESENT_METRIC_SOURCE)).copied().collect::<Vec<_>>();
   if !presentation_metrics.is_empty()
   {
      let correlation_path = expected_directory.join(format!("{}.presentation.correlation.json", side));
      ensure!(result.trace_correlation_path.as_deref() == Some(correlation_path.to_string_lossy().as_ref()), "macOS campaign correlation path is not bound to the selected campaign root");
      let correlation_bytes = fs::read(&correlation_path).with_context(|| format!("reading {}", correlation_path.display()))?;
      ensure!(result.trace_correlation_sha256.as_deref() == Some(sha256(&correlation_bytes).as_str()), "macOS campaign correlation hash differs from its session result");
      let correlation: MacOsPresentationCorrelationArtifact = serde_json::from_slice(&correlation_bytes).context("decoding macOS presentation correlation evidence")?;
      ensure!(correlation.schema_version == 1 && !correlation.availability.is_empty() && !correlation.calibration_status.is_empty(), "macOS presentation correlation identity is incomplete");
      ensure!(correlation.uncorrelated_visual_generations.is_empty(), "authoritative macOS analyzer session contains uncorrelated visual generations");
      let mut scenario_counts = vec![0_u64; scenario_ids.len()];
      for sample in &correlation.correlations
      {
         let scenario_index = usize::try_from(sample.scenario_index).context("macOS correlation scenario index exceeds usize")?;
         let scenario_id = *scenario_ids.get(scenario_index).context("macOS correlation scenario index exceeds the selected pack")?;
         ensure!(sample.process.ends_with(&format!(" ({})", resource.pid)), "macOS correlation process does not bind exact resource PID {}", resource.pid);
         let sample_index = scenario_counts[scenario_index];
         scenario_counts[scenario_index] += 1;
         for metric in &presentation_metrics
         {
            let value = match metric.source.as_str()
            {
               MACOS_INPUT_TO_PRESENT_METRIC_SOURCE => sample.candidate_input_to_frame_lifetime_end_ns,
               MACOS_VISUAL_TO_PRESENT_METRIC_SOURCE => sample.candidate_visual_to_frame_lifetime_end_ns,
               _ => unreachable!(),
            };
            rows.push(raw_u64_row(result, analyzer, scenario_id, "correlated-presentation", sample_index, metric, value, "instruments-continuous-nanoseconds", sample.frame_lifetime_end_ns, vec![correlation.availability.clone(), correlation.calibration_status.clone()], Some(format!("input-generation-{}", sample.input_generation)), Some(format!("swap-{}", sample.swap_id)))?);
         }
      }
      ensure!(scenario_counts.iter().all(|count| *count > 0), "macOS analyzer session does not contain correlated evidence for every selected scenario");
      artifact_hashes.insert(String::from("presentation-correlation"), sha256(&correlation_bytes));
   }

   append_resource_rows(&mut rows, result, analyzer, &scenario_ids, &required_metrics, &resource_bytes, &resource)?;
   append_launch_rows(&mut rows, &mut artifact_hashes, campaign, result, analyzer, &scenario_ids, &required_metrics, &expected_directory, side, &resource)?;
   append_common_gpu_rows(&mut rows, &mut artifact_hashes, result, analyzer, &scenario_ids, &required_metrics, &expected_directory, side, &resource)?;
   if let Some((start, end, numerator, denominator)) = append_energy_rows(&mut rows, &mut artifact_hashes, campaign, result, analyzer, &scenario_ids, &required_metrics, &expected_directory, side, &resource)?
   {
      session_start_ticks = start;
      session_end_ticks = end;
      session_timebase_numerator = numerator;
      session_timebase_denominator = denominator;
   }
   ensure!(!rows.is_empty(), "macOS analyzer session produced no supported raw observations");
   let raw_bytes = json_lines(&rows)?;
   let raw_path = PathBuf::from("raw").join(format!("{}.jsonl", result.generation));

   let environment = json!({
      "campaign_run_id": campaign.run_id,
      "build_manifest_sha256": report.build_manifest_sha256,
      "evidence": "macos-campaign-controller-validated",
   });
   let start_ns = ticks_to_ns(session_start_ticks, session_timebase_numerator, session_timebase_denominator)?;
   let end_ns = ticks_to_ns(session_end_ticks, session_timebase_numerator, session_timebase_denominator)?;
   ensure!(start_ns < end_ns, "macOS analyzer session has a nonpositive process interval");
   let pass_artifact_hash = sha256(&serde_json::to_vec(&artifact_hashes).context("encoding macOS analyzer pass artifact identities")?);
   Ok(ConvertedSession {
      session: ComparisonSession {
         measurement_pass_id: analyzer.measurement_pass_id.clone(),
         pair_index: DecimalU64(result.session.pair_index as u64),
         order: result.session.order,
         implementation_id: implementation_id.clone(),
         process_id: DecimalU64(resource.pid as u64),
         monotonic_start_ns: DecimalU64(start_ns),
         end_ns: DecimalU64(end_ns),
         environment_before: environment.clone(),
         environment_after: environment,
         warmup_samples: Vec::new(),
         raw_sample_artifact: ArtifactIdentity {path: raw_path.to_string_lossy().into_owned(), sha256: sha256(&raw_bytes)},
         pass_artifact_hash,
         validation: String::from("valid"),
         invalid_reason: None,
         terminal_hard_outcome: None,
         durable_checkpoint_generation: DecimalU64(checkpoint_generation),
         atomic_commit_sha256: String::from(checkpoint_sha256),
         artifact_hashes,
      },
      raw_path,
      raw_bytes,
   })
}

fn validate_session_publication_closure(campaign_root: &Path, campaign: &MacOsCampaignPlan, result: &MacOsCampaignSessionResult, directory: &Path, side: &str, artifact_hashes: &mut BTreeMap<String, String>) -> Result<()>
{
   if result.session.pass_role == oxide_benchmark_spec::AppleCampaignPassRole::Launch
   {
      return Ok(());
   }
   let complete_path = directory.join(format!("{}.complete.json", side));
   let complete_bytes = fs::read(&complete_path).with_context(|| format!("reopening macOS analyzer complete envelope {}", complete_path.display()))?;
   ensure!(sha256(&complete_bytes) == result.artifact_sha256, "macOS analyzer complete envelope changed after campaign validation");
   let envelope: SessionEnvelope = serde_json::from_slice(&complete_bytes).context("decoding independently reopened macOS analyzer complete envelope")?;
   ensure!(envelope.run_id == campaign.run_id && envelope.plan_sha256 == campaign.plan_sha256 && envelope.chunk_id == result.session.chunk_id && envelope.pass_id == result.session.pass_id && envelope.pack_id == result.session.pack_id && envelope.pair_index == result.session.pair_index && envelope.side == result.session.side && envelope.generation == result.generation, "macOS analyzer complete envelope identity differs from its selected session");

   let telemetry_path = directory.join(format!("{}.telemetry.bin", side));
   let telemetry = fs::read(&telemetry_path).with_context(|| format!("independently reopening macOS analyzer telemetry {}", telemetry_path.display()))?;
   let telemetry_sha256 = sha256(&telemetry);
   ensure!(envelope.telemetry_sha256 == telemetry_sha256 && envelope.telemetry_byte_count == telemetry.len() as u64, "macOS analyzer telemetry changed after campaign validation");

   let coverage_identity = envelope.telemetry_coverage.as_ref().context("selected macOS analyzer session has no telemetry coverage identity")?;
   let coverage_relative = Path::new("Runs")
      .join(&campaign.run_id)
      .join(&result.session.chunk_id)
      .join(&result.session.pass_id)
      .join(&result.session.pack_id)
      .join(result.session.pair_index.to_string())
      .join(format!("{}.telemetry.coverage.json", side));
   ensure!(Path::new(&coverage_identity.path) == coverage_relative, "macOS analyzer telemetry coverage path is not campaign-root-relative and session-bound");
   let coverage_path = campaign_root.join(&coverage_relative);
   let coverage = fs::read(&coverage_path).with_context(|| format!("independently reopening macOS analyzer telemetry coverage {}", coverage_path.display()))?;
   let coverage_sha256 = sha256(&coverage);
   ensure!(coverage_identity.sha256 == coverage_sha256, "macOS analyzer telemetry coverage changed after campaign validation");
   validate_macos_telemetry_coverage(&coverage, &telemetry, result.session.side, &result.session.pass_id)?;

   let surface_relative = Path::new("Runs")
      .join(&campaign.run_id)
      .join(&result.session.chunk_id)
      .join(&result.session.pass_id)
      .join(&result.session.pack_id)
      .join(result.session.pair_index.to_string())
      .join(format!("{}.surface.json", side));
   ensure!(Path::new(&envelope.surface_receipt.path) == surface_relative, "macOS analyzer surface receipt path is not campaign-root-relative and session-bound");
   let surface_path = campaign_root.join(&surface_relative);
   ensure!(result.surface_receipt_path.as_deref() == surface_path.to_str(), "macOS analyzer surface receipt result path differs from the selected campaign root");
   let surface = fs::read(&surface_path).with_context(|| format!("independently reopening macOS analyzer surface receipt {}", surface_path.display()))?;
   let surface_sha256 = sha256(&surface);
   ensure!(envelope.surface_receipt.sha256 == surface_sha256 && result.surface_receipt_sha256.as_deref() == Some(surface_sha256.as_str()), "macOS analyzer surface receipt changed after campaign validation");
   let receipt: MacOsSurfaceReceipt = serde_json::from_slice(&surface).context("decoding independently reopened macOS analyzer surface receipt")?;
   validate_macos_surface_receipt(campaign, &result.session, &result.generation, &receipt)?;

   artifact_hashes.insert(String::from("telemetry-bin"), telemetry_sha256);
   artifact_hashes.insert(String::from("telemetry-coverage"), coverage_sha256);
   artifact_hashes.insert(String::from("surface-receipt"), surface_sha256);
   Ok(())
}

fn raw_u64_row(result: &MacOsCampaignSessionResult, analyzer: &ComparisonPlan, scenario_id: &str, phase_id: &str, sample_index: u64, metric: &MetricDefinition, value: u64, clock_id: &str, timestamp_ns: u64, quality_flags: Vec<String>, event_id: Option<String>, state_id: Option<String>) -> Result<RawObservationRow>
{
   ensure!(value <= (1_u64 << 53), "macOS analyzer metric {} exceeds exact JSON-to-f64 integer range", metric.id);
   Ok(RawObservationRow {
      session_id: result.generation.clone(),
      measurement_pass_id: analyzer.measurement_pass_id.clone(),
      scenario_id: String::from(scenario_id),
      phase_id: String::from(phase_id),
      sample_index: DecimalU64(sample_index),
      timestamps: vec![RawObservationTimestamp {clock_id: String::from(clock_id), timestamp_ns: DecimalU64(timestamp_ns)}],
      metric_id: metric.id.clone(),
      value: RawObservationValue::DecimalU64(DecimalU64(value)),
      event_id,
      state_id,
      quality_flags,
   })
}

fn raw_f64_row(result: &MacOsCampaignSessionResult, analyzer: &ComparisonPlan, scenario_id: &str, phase_id: &str, sample_index: u64, metric: &MetricDefinition, value: f64, clock_id: &str, timestamp_ns: u64, quality_flags: Vec<String>, event_id: Option<String>) -> Result<RawObservationRow>
{
   ensure!(value.is_finite(), "macOS analyzer metric {} is not finite", metric.id);
   Ok(RawObservationRow {
      session_id: result.generation.clone(),
      measurement_pass_id: analyzer.measurement_pass_id.clone(),
      scenario_id: String::from(scenario_id),
      phase_id: String::from(phase_id),
      sample_index: DecimalU64(sample_index),
      timestamps: vec![RawObservationTimestamp {clock_id: String::from(clock_id), timestamp_ns: DecimalU64(timestamp_ns)}],
      metric_id: metric.id.clone(),
      value: RawObservationValue::FiniteF64(value),
      event_id,
      state_id: None,
      quality_flags,
   })
}

fn append_resource_rows(rows: &mut Vec<RawObservationRow>, result: &MacOsCampaignSessionResult, analyzer: &ComparisonPlan, scenario_ids: &[&str], metrics: &[&MetricDefinition], resource_bytes: &[u8], resource: &MacOsResourceArtifact) -> Result<()>
{
   let metrics = metrics.iter().filter(|metric| metric.source.starts_with(MACOS_RESOURCE_METRIC_SOURCE_PREFIX)).copied().collect::<Vec<_>>();
   if metrics.is_empty()
   {
      return Ok(());
   }
   ensure!(scenario_ids.len() == 1, "whole-session macOS resource summaries cannot be duplicated across a multi-scenario analyzer pack");
   let summary = reduce_macos_resource_artifact(resource_bytes, None, None)?;
   let timestamp_ns = ticks_to_ns(summary.window_end_ticks, resource.timebase_numerator, resource.timebase_denominator)?;
   for metric in metrics
   {
      let field = source_field(&metric.source, MACOS_RESOURCE_METRIC_SOURCE_PREFIX)?;
      let flags = vec![String::from("available-proc-pid-rusage-v4"), String::from("validated-whole-session-resource-summary")];
      if let Some(value) = resource_u64_value(&summary, field)
      {
         rows.push(raw_u64_row(result, analyzer, scenario_ids[0], "whole-session-resource", 0, metric, value, "mach-continuous-nanoseconds", timestamp_ns, flags, None, None)?);
      }
      else
      {
         let value = resource_f64_value(&summary, field).with_context(|| format!("unsupported macOS resource-summary field {}", field))?;
         rows.push(raw_f64_row(result, analyzer, scenario_ids[0], "whole-session-resource", 0, metric, value, "mach-continuous-nanoseconds", timestamp_ns, flags, None)?);
      }
   }
   Ok(())
}

fn resource_u64_value(summary: &super::MacOsResourceSummary, field: &str) -> Option<u64>
{
   match field
   {
      "wall-time-ns" => Some(summary.wall_time_ns),
      "user-cpu-ns" => Some(summary.user_cpu_ns),
      "system-cpu-ns" => Some(summary.system_cpu_ns),
      "runnable-time-ns" => Some(summary.runnable_time_ns),
      "wakeups" => Some(summary.wakeups),
      "pageins" => Some(summary.pageins),
      "disk-read-bytes" => Some(summary.disk_read_bytes),
      "disk-written-bytes" => Some(summary.disk_written_bytes),
      "logical-writes" => Some(summary.logical_writes),
      "instructions" => Some(summary.instructions),
      "cycles" => Some(summary.cycles),
      "wired-start-bytes" => Some(summary.wired_start_bytes),
      "wired-end-bytes" => Some(summary.wired_end_bytes),
      "wired-peak-bytes" => Some(summary.wired_peak_bytes),
      "resident-start-bytes" => Some(summary.resident_start_bytes),
      "resident-end-bytes" => Some(summary.resident_end_bytes),
      "resident-peak-bytes" => Some(summary.resident_peak_bytes),
      "physical-footprint-start-bytes" => Some(summary.physical_footprint_start_bytes),
      "physical-footprint-end-bytes" => Some(summary.physical_footprint_end_bytes),
      "physical-footprint-peak-bytes" => Some(summary.physical_footprint_peak_bytes),
      _ => None,
   }
}

fn resource_f64_value(summary: &super::MacOsResourceSummary, field: &str) -> Option<f64>
{
   match field
   {
      "process-cpu-ms-per-wall-s" => Some(summary.process_cpu_ms_per_wall_s),
      "wakeups-per-wall-s" => Some(summary.wakeups_per_wall_s),
      "retained-slope-bytes-per-min" => Some(summary.retained_slope_bytes_per_min),
      _ => None,
   }
}

fn append_launch_rows(rows: &mut Vec<RawObservationRow>, artifact_hashes: &mut BTreeMap<String, String>, campaign: &MacOsCampaignPlan, result: &MacOsCampaignSessionResult, analyzer: &ComparisonPlan, scenario_ids: &[&str], metrics: &[&MetricDefinition], directory: &Path, side: &str, resource: &MacOsResourceArtifact) -> Result<()>
{
   let metrics = metrics.iter().filter(|metric| metric.source.starts_with(MACOS_LAUNCH_METRIC_SOURCE_PREFIX)).copied().collect::<Vec<_>>();
   if metrics.is_empty()
   {
      return Ok(());
   }
   ensure!(result.session.pass_role == oxide_benchmark_spec::AppleCampaignPassRole::Launch, "macOS launch metric source is attached to a non-launch pass");
   ensure!(scenario_ids.len() == 1, "macOS launch summaries require exactly one analyzer scenario");
   let evidence_path = directory.join(format!("{}.launch.evidence.json", side));
   ensure!(result.launch_evidence_path.as_deref() == evidence_path.to_str(), "macOS launch evidence path is not bound to the selected campaign root");
   let evidence_bytes = fs::read(&evidence_path).with_context(|| format!("reading {}", evidence_path.display()))?;
   ensure!(result.launch_evidence_sha256.as_deref() == Some(sha256(&evidence_bytes).as_str()), "macOS launch evidence hash differs from its session result");
   let evidence: MacOsLaunchEvidence = serde_json::from_slice(&evidence_bytes).context("decoding macOS launch analyzer evidence")?;
   let launch_class = macos_launch_class(result.session.launch_class.as_deref().context("macOS launch analyzer session has no launch class")?)?;
   let expected = MacOsLaunchExpectation {
      run_id: campaign.run_id.clone(),
      plan_sha256: campaign.plan_sha256.clone(),
      chunk_id: result.session.chunk_id.clone(),
      pack_id: result.session.pack_id.clone(),
      pair_index: result.session.pair_index,
      generation: result.generation.clone(),
      executable_sha256: result.executable_sha256.clone(),
      side: result.session.side,
      launch_class,
      installed_bundle_path: String::new(),
      data_container_path: None,
   };
   validate_macos_launch_evidence(&evidence, &expected)?;
   ensure!(evidence.pid == resource.pid && evidence.launch_request_ticks == resource.launch_t0, "macOS launch evidence PID or launch boundary differs from resource evidence");

   let correlation_path = directory.join(format!("{}.presentation.correlation.json", side));
   ensure!(result.trace_correlation_path.as_deref() == correlation_path.to_str(), "macOS launch correlation path is not bound to the selected campaign root");
   let correlation_bytes = fs::read(&correlation_path).with_context(|| format!("reading {}", correlation_path.display()))?;
   ensure!(result.trace_correlation_sha256.as_deref() == Some(sha256(&correlation_bytes).as_str()), "macOS launch correlation hash differs from its session result");
   let correlation: MacOsLaunchPresentationCorrelation = serde_json::from_slice(&correlation_bytes).context("decoding macOS launch presentation correlation")?;
   ensure!(correlation.schema_version == 1 && correlation.pid == evidence.pid && correlation.process.ends_with(&format!(" ({})", evidence.pid)) && correlation.exact_pid_filtered && !correlation.calibration_status.is_empty(), "macOS launch correlation identity is incomplete");
   ensure!(correlation.first_attributed_present_proxy_ticks == evidence.first_attributed_present_proxy_ticks && correlation.response_attributed_present_proxy_ticks == evidence.first_interactive_response_present_proxy_ticks, "macOS launch correlation endpoints differ from launch evidence");
   for metric in metrics
   {
      let field = source_field(&metric.source, MACOS_LAUNCH_METRIC_SOURCE_PREFIX)?;
      let (start, end) = match field
      {
         "launch-request-to-first-complete-ui-generation-ns" => (evidence.launch_request_ticks, evidence.first_complete_ui_generation_ticks),
         "launch-request-to-first-attributed-present-proxy-ns" => (evidence.launch_request_ticks, evidence.first_attributed_present_proxy_ticks),
         "input-request-to-response-generation-ns" => (evidence.first_interactive_input_request_ticks, evidence.first_interactive_response_generation_ticks),
         "input-request-to-response-attributed-present-proxy-ns" => (evidence.first_interactive_input_request_ticks, evidence.first_interactive_response_present_proxy_ticks),
         _ => unreachable!(),
      };
      let delta = end.checked_sub(start).context("macOS launch summary endpoint precedes its start")?;
      let value = ticks_to_ns(delta, resource.timebase_numerator, resource.timebase_denominator)?;
      let timestamp_ns = ticks_to_ns(end, resource.timebase_numerator, resource.timebase_denominator)?;
      rows.push(raw_u64_row(result, analyzer, scenario_ids[0], "canonical-launch", 0, metric, value, "mach-continuous-nanoseconds", timestamp_ns, vec![String::from("validated-exact-pid-launch-evidence"), correlation.calibration_status.clone()], Some(String::from(launch_class.as_str())), None)?);
   }
   artifact_hashes.insert(String::from("launch-evidence"), sha256(&evidence_bytes));
   artifact_hashes.insert(String::from("launch-presentation-correlation"), sha256(&correlation_bytes));
   Ok(())
}

fn macos_launch_class(value: &str) -> Result<MacOsLaunchClass>
{
   match value
   {
      "terminated-warm-system-cache" => Ok(MacOsLaunchClass::TerminatedProcessWarmSystemCache),
      "fresh-install-first-launch" => Ok(MacOsLaunchClass::FreshInstallFirstLaunch),
      "warm-resume" => Ok(MacOsLaunchClass::WarmResume),
      _ => bail!("macOS analyzer session has unsupported launch class {}", value),
   }
}

fn append_common_gpu_rows(rows: &mut Vec<RawObservationRow>, artifact_hashes: &mut BTreeMap<String, String>, result: &MacOsCampaignSessionResult, analyzer: &ComparisonPlan, scenario_ids: &[&str], metrics: &[&MetricDefinition], directory: &Path, side: &str, resource: &MacOsResourceArtifact) -> Result<()>
{
   let metrics = metrics.iter().filter(|metric| metric.source.starts_with(MACOS_COMMON_GPU_METRIC_SOURCE_PREFIX)).copied().collect::<Vec<_>>();
   if metrics.is_empty()
   {
      return Ok(());
   }
   ensure!(result.session.collector.as_deref() == Some("common-gpu"), "macOS common-GPU metric source is attached to a different collector");
   let samples_path = directory.join(format!("{}.common-gpu.samples.json", side));
   let signposts_path = directory.join(format!("{}.common-gpu.signposts.xml", side));
   let summary_path = directory.join(format!("{}.common-gpu.json", side));
   ensure!(result.common_gpu_samples_path.as_deref() == samples_path.to_str() && result.common_gpu_artifact_path.as_deref() == summary_path.to_str(), "macOS common-GPU paths are not bound to the selected campaign root");
   let samples_bytes = fs::read(&samples_path).with_context(|| format!("reading {}", samples_path.display()))?;
   let signposts_bytes = fs::read(&signposts_path).with_context(|| format!("reading {}", signposts_path.display()))?;
   let summary_bytes = fs::read(&summary_path).with_context(|| format!("reading {}", summary_path.display()))?;
   ensure!(result.common_gpu_samples_sha256.as_deref() == Some(sha256(&samples_bytes).as_str()) && result.common_gpu_signposts_sha256.as_deref() == Some(sha256(&signposts_bytes).as_str()) && result.common_gpu_artifact_sha256.as_deref() == Some(sha256(&summary_bytes).as_str()), "macOS common-GPU artifact hash differs from its session result");
   let samples: MacOsCommonGpuSamples = serde_json::from_slice(&samples_bytes).context("decoding macOS common-GPU samples")?;
   let summary: MacOsCommonGpuSummary = serde_json::from_slice(&summary_bytes).context("decoding macOS common-GPU summary")?;
   let signposts = std::str::from_utf8(&signposts_bytes).context("macOS common-GPU signposts are not UTF-8")?;
   ensure!(summary == reduce_macos_common_gpu(&samples, signposts, resource.pid)?, "macOS common-GPU summary differs from raw exact-PID evidence");
   for (sample_index, phase) in summary.phases.iter().enumerate()
   {
      let scenario_index = usize::try_from(phase.scenario_index).context("macOS common-GPU scenario index exceeds usize")?;
      let scenario_id = *scenario_ids.get(scenario_index).context("macOS common-GPU scenario index exceeds the selected pack")?;
      let timestamp_ns = ticks_to_ns(phase.observed_end_ticks, resource.timebase_numerator, resource.timebase_denominator)?;
      for metric in &metrics
      {
         rows.push(raw_u64_row(result, analyzer, scenario_id, "phase-bounded-common-gpu", sample_index as u64, metric, phase.gpu_time_ns, "mach-continuous-nanoseconds", timestamp_ns, vec![summary.availability.clone(), String::from("comparison-ineligible-diagnostic")], Some(format!("phase-{}", phase.phase_identifier)), None)?);
      }
   }
   ensure!(!summary.phases.is_empty(), "macOS common-GPU summary has no phase rows");
   artifact_hashes.insert(String::from("common-gpu-samples"), sha256(&samples_bytes));
   artifact_hashes.insert(String::from("common-gpu-signposts"), sha256(&signposts_bytes));
   artifact_hashes.insert(String::from("common-gpu-summary"), sha256(&summary_bytes));
   Ok(())
}

fn append_energy_rows(rows: &mut Vec<RawObservationRow>, artifact_hashes: &mut BTreeMap<String, String>, campaign: &MacOsCampaignPlan, result: &MacOsCampaignSessionResult, analyzer: &ComparisonPlan, scenario_ids: &[&str], metrics: &[&MetricDefinition], directory: &Path, side: &str, resource: &MacOsResourceArtifact) -> Result<Option<(u64, u64, u32, u32)>>
{
   let metrics = metrics.iter().filter(|metric| metric.source.starts_with(MACOS_ENERGY_METRIC_SOURCE_PREFIX)).copied().collect::<Vec<_>>();
   if metrics.is_empty()
   {
      return Ok(None);
   }
   ensure!(result.session.pass_role == oxide_benchmark_spec::AppleCampaignPassRole::Energy, "macOS direct-energy metric source is attached to a non-energy pass");
   ensure!(scenario_ids.len() == 1, "macOS energy summaries require exactly one analyzer scenario");
   let raw_path = directory.join(format!("{}.energy.raw.json", side));
   let summary_path = directory.join(format!("{}.energy.summary.json", side));
   let telemetry_path = directory.join(format!("{}.telemetry.bin", side));
   let complete_path = directory.join(format!("{}.complete.json", side));
   ensure!(result.energy_raw_path.as_deref() == raw_path.to_str() && result.energy_summary_path.as_deref() == summary_path.to_str(), "macOS direct-energy paths are not bound to the selected campaign root");
   let raw_bytes = fs::read(&raw_path).with_context(|| format!("reading {}", raw_path.display()))?;
   let summary_bytes = fs::read(&summary_path).with_context(|| format!("reading {}", summary_path.display()))?;
   let telemetry = fs::read(&telemetry_path).with_context(|| format!("reading {}", telemetry_path.display()))?;
   let complete_bytes = fs::read(&complete_path).with_context(|| format!("reading {}", complete_path.display()))?;
   ensure!(sha256(&complete_bytes) == result.artifact_sha256, "macOS energy complete envelope hash differs from its session result");
   ensure!(result.energy_raw_sha256.as_deref() == Some(sha256(&raw_bytes).as_str()) && result.energy_summary_sha256.as_deref() == Some(sha256(&summary_bytes).as_str()), "macOS direct-energy artifact hash differs from its session result");
   let raw: MacOsExternalMeterRawArtifact = serde_json::from_slice(&raw_bytes).context("decoding macOS direct-energy raw samples")?;
   let summary: MacOsEnergySummary = serde_json::from_slice(&summary_bytes).context("decoding macOS direct-energy summary")?;
   let envelope: SessionEnvelope = serde_json::from_slice(&complete_bytes).context("decoding macOS energy complete envelope")?;
   ensure!(envelope.run_id == campaign.run_id && envelope.plan_sha256 == campaign.plan_sha256 && envelope.chunk_id == result.session.chunk_id && envelope.pass_id == result.session.pass_id && envelope.pair_index == result.session.pair_index && envelope.side == result.session.side && envelope.generation == result.generation && envelope.pack_id == result.session.pack_id, "macOS direct-energy complete envelope differs from the selected session");
   ensure!(raw.run_id == campaign.run_id && raw.plan_sha256 == campaign.plan_sha256 && raw.generation == result.generation && raw.pid == resource.pid && envelope.telemetry_sha256 == sha256(&telemetry) && envelope.telemetry_byte_count == telemetry.len() as u64 && envelope.timebase_numerator == raw.timebase_numerator && envelope.timebase_denominator == raw.timebase_denominator, "macOS direct-energy raw, telemetry, and complete-envelope identities differ");
   ensure!(summary == reduce_macos_energy(&raw, &summary.calibration, &telemetry)?, "macOS direct-energy summary differs from raw phase-bound samples");
   for (sample_index, phase) in summary.phases.iter().enumerate()
   {
      let timestamp_ns = ticks_to_ns(phase.end_ticks, raw.timebase_numerator, raw.timebase_denominator)?;
      for metric in &metrics
      {
         let field = source_field(&metric.source, MACOS_ENERGY_METRIC_SOURCE_PREFIX)?;
         let value = match field
         {
            "phase-joules" => phase.joules,
            "phase-baseline-adjusted-joules" => phase.baseline_adjusted_joules,
            "phase-average-watts" => phase.average_watts,
            "phase-baseline-adjusted-average-watts" => phase.baseline_adjusted_average_watts,
            _ => unreachable!(),
         };
         rows.push(raw_f64_row(result, analyzer, scenario_ids[0], "phase-bound-direct-energy", sample_index as u64, metric, value, "mach-continuous-nanoseconds", timestamp_ns, vec![summary.availability.clone(), String::from("direct-external-meter")], Some(format!("phase-{}", phase.phase_identifier)))?);
      }
   }
   ensure!(!summary.phases.is_empty(), "macOS direct-energy summary has no phase rows");
   artifact_hashes.insert(String::from("direct-energy-raw"), sha256(&raw_bytes));
   artifact_hashes.insert(String::from("direct-energy-summary"), sha256(&summary_bytes));
   artifact_hashes.insert(String::from("direct-energy-telemetry"), sha256(&telemetry));
   Ok(Some((raw.capture_start_ticks, raw.capture_end_ticks, raw.timebase_numerator, raw.timebase_denominator)))
}

fn load_pair_checkpoint(campaign_root: &Path, campaign: &MacOsCampaignPlan, result: &MacOsCampaignSessionResult) -> Result<(String, MacOsPairCheckpoint)>
{
   let directory = campaign_root
      .join("Runs")
      .join(&campaign.run_id)
      .join(&result.session.chunk_id)
      .join(&result.session.pass_id)
      .join(&result.session.pack_id)
      .join(result.session.pair_index.to_string());
   let upgraded = directory.join("pair.complete.v2.json");
   let primary = directory.join("pair.complete.json");
   let path = if upgraded.exists() {upgraded} else {primary};
   let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
   let checkpoint: MacOsPairCheckpoint = serde_json::from_slice(&bytes).context("decoding macOS atomic pair checkpoint")?;
   ensure!(checkpoint.complete && checkpoint.run_id == campaign.run_id && checkpoint.plan_sha256 == campaign.plan_sha256, "macOS atomic pair checkpoint differs from the active campaign");
   Ok((sha256(&bytes), checkpoint))
}

fn staging_path(output_root: &Path) -> Result<PathBuf>
{
   let parent = output_root.parent().context("macOS analyzer bundle destination has no parent")?;
   let name = output_root.file_name().and_then(|value| value.to_str()).context("macOS analyzer bundle destination name is not UTF-8")?;
   let staging = parent.join(format!(".{}.partial-{}", name, std::process::id()));
   ensure!(!staging.exists(), "macOS analyzer bundle staging path already exists: {}", staging.display());
   Ok(staging)
}

fn ticks_to_ns(ticks: u64, numerator: u32, denominator: u32) -> Result<u64>
{
   ensure!(numerator > 0 && denominator > 0, "macOS analyzer resource timebase is zero");
   let value = u128::from(ticks).checked_mul(u128::from(numerator)).context("macOS analyzer clock conversion overflow")? / u128::from(denominator);
   u64::try_from(value).context("macOS analyzer nanoseconds exceed u64")
}

fn json_lines<T: Serialize>(values: &[T]) -> Result<Vec<u8>>
{
   let mut bytes = Vec::new();
   for value in values
   {
      serde_json::to_writer(&mut bytes, value).context("encoding macOS analyzer JSONL row")?;
      bytes.push(b'\n');
   }
   Ok(bytes)
}

fn sha256(bytes: &[u8]) -> String
{
   format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests
{
   use super::*;
   use crate::MacOsTrustedInputReceiptManifestEntry;

   #[test]
   fn raw_receipt_hash_closure_rejects_missing_tampered_and_extra_files()
   {
      let fixture = tempfile::tempdir().expect("raw receipt fixture");
      let request = b"request";
      let controller = b"controller";
      let application = b"application";
      let manifest = MacOsTrustedInputReceiptManifest {
         schema_version: 1,
         run_id: String::from("run"),
         plan_sha256: sha256(b"plan"),
         chunk_id: String::from("chunk"),
         pass_id: String::from("pass"),
         pack_id: String::from("pack"),
         pair_index: 0,
         side: ComparisonSide::Native,
         generation: sha256(b"generation"),
         expected_command_count: 1,
         receipts: vec![MacOsTrustedInputReceiptManifestEntry {
            command_sequence: 0,
            scenario_id: String::from("scenario"),
            request_sha256: sha256(request),
            controller_receipt_sha256: sha256(controller),
            application_receipt_sha256: sha256(application),
            first_event_index: 0,
            last_event_index: 0,
            raw_event_families: vec![String::from("mouse")],
            raw_event_types: vec![1],
            state_generation_before: 1,
            state_generation_after: 2,
         }],
         complete: true,
      };
      for (suffix, bytes) in [("request.json", request.as_slice()), ("controller.json", controller.as_slice()), ("application.json", application.as_slice())]
      {
         fs::write(fixture.path().join(format!("native.trusted-input.0.{suffix}")), bytes).expect("raw receipt");
      }
      validate_raw_receipt_hash_closure(fixture.path(), "native", &manifest).expect("complete hash closure");

      fs::write(fixture.path().join("native.trusted-input.0.request.json"), b"tampered").expect("tampered receipt");
      assert!(validate_raw_receipt_hash_closure(fixture.path(), "native", &manifest).expect_err("tampering must fail").to_string().contains("hash differs"));
      fs::write(fixture.path().join("native.trusted-input.0.request.json"), request).expect("restore receipt");
      fs::remove_file(fixture.path().join("native.trusted-input.0.application.json")).expect("remove receipt");
      assert!(validate_raw_receipt_hash_closure(fixture.path(), "native", &manifest).expect_err("deletion must fail").to_string().contains("reading"));
      fs::write(fixture.path().join("native.trusted-input.0.application.json"), application).expect("restore application receipt");
      fs::write(fixture.path().join("native.trusted-input.1.request.json"), b"extra").expect("extra receipt");
      assert!(validate_raw_receipt_hash_closure(fixture.path(), "native", &manifest).expect_err("extra receipt must fail").to_string().contains("extra or duplicate"));
   }
}
