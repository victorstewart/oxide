use std::fs;
use std::path::{Path, PathBuf};

use oxide_apple_comparison_controller::{
   materialize_macos_analyzer_bundle, reduce_macos_acquisition_validity_with_pre_pair_environment,
   ComparisonSide, MacOsBackgroundObservation, MacOsCampaignPlan, MacOsCampaignReport, MacOsCampaignScope,
   MacOsCampaignSession, MacOsCampaignSessionResult, MacOsDisplayObservation,
   MacOsEnvironmentSnapshot, MacOsEnvironmentValue, MacOsObservationAvailability,
   MacOsPairCheckpoint, MacOsPowerSource, MacOsPrePairEnvironmentObservation,
   MacOsSurfaceContractObservation, MacOsThermalState,
   MacOsTrustedInputReceiptManifest, MacOsTrustedInputReceiptManifestEntry,
   MACOS_AUTHORITATIVE_INPUT_SCOPE, MACOS_INPUT_TO_PRESENT_METRIC_SOURCE,
   MACOS_RESOURCE_METRIC_SOURCE_PREFIX,
};
use oxide_benchmark_spec::{
   balanced_comparison_order, AppleCampaignEvidenceRole, AppleCampaignMeasurementTimingSpec,
   AppleCampaignPassRole, AppleCampaignPassTimingSpec, AppleCampaignScenarioTimingSpec,
   ArtifactIdentity, CommonIdentity, ComparisonCell, ComparisonOrder, ComparisonPlan,
   ControllerChunk, DecisionAlternative, DecisionClaimKind, DecisionFamily,
   DecisionFamilyMember, DecimalU64, EvidenceRole, ImplementationIdentity, MetricDefinition,
   MetricDirection, Platform, ScenarioPack, Tier, InstrumentationCalibrationInput,
   InstrumentationCalibrationPair, TracePairOrder, reduce_instrumentation_calibration,
};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};

#[test]
fn completed_authoritative_campaign_materializes_resource_summary_rows()
{
   let source = format!("{}process-cpu-ms-per-wall-s", MACOS_RESOURCE_METRIC_SOURCE_PREFIX);
   let fixture = write_fixture(true, &source);
   let output = fixture.root.path().join("bundle");
   let manifest = materialize_macos_analyzer_bundle(fixture.root.path(), &fixture.analyzer_plan, &output).expect("authoritative resource bundle");
   assert_eq!(manifest.session_count.0, 2);
   let raw_path = fs::read_dir(output.join("raw")).expect("raw directory").next().expect("raw entry").expect("raw entry").path();
   let row: oxide_benchmark_spec::RawObservationRow = serde_json::from_str(fs::read_to_string(raw_path).expect("raw row").lines().next().expect("first row")).expect("typed raw row");
   assert_eq!(row.phase_id, "whole-session-resource");
   assert_eq!(row.metric_id, "interaction_to_present_ns");
   assert_eq!(row.value, oxide_benchmark_spec::RawObservationValue::FiniteF64(200.0));
}

#[test]
fn completed_authoritative_campaign_preserves_presentation_rows()
{
   let fixture = write_fixture(true, MACOS_INPUT_TO_PRESENT_METRIC_SOURCE);
   let output = fixture.root.path().join("presentation-bundle");
   materialize_macos_analyzer_bundle(fixture.root.path(), &fixture.analyzer_plan, &output).expect("authoritative presentation bundle");
   let raw_path = fs::read_dir(output.join("raw")).expect("raw directory").next().expect("raw entry").expect("raw entry").path();
   let rows = fs::read_to_string(raw_path).expect("raw presentation rows");
   assert_eq!(rows.lines().count(), 30);
   let first: oxide_benchmark_spec::RawObservationRow = serde_json::from_str(rows.lines().next().expect("first row")).expect("typed raw row");
   assert_eq!(first.phase_id, "correlated-presentation");
   assert_eq!(first.value, oxide_benchmark_spec::RawObservationValue::DecimalU64(DecimalU64(1_000)));
}

#[test]
fn unsupported_required_metric_fails_before_publication()
{
   let fixture = write_fixture(true, "macos-unvalidated-proxy");
   let output = fixture.root.path().join("unsupported-bundle");
   let error = materialize_macos_analyzer_bundle(fixture.root.path(), &fixture.analyzer_plan, &output).expect_err("unsupported metric source must fail");
   assert!(error.to_string().contains("does not support required metric"));
   assert!(!output.exists());
}

#[test]
fn specialized_source_families_reject_wrong_pass_or_claim_scope()
{
   for (name, source) in [
      ("launch", "macos-launch-evidence-summary:launch-request-to-first-attributed-present-proxy-ns"),
      ("energy", "macos-direct-meter-energy-summary:phase-joules"),
   ]
   {
      let fixture = write_fixture(true, source);
      let output = fixture.root.path().join(format!("{name}-bundle"));
      let error = materialize_macos_analyzer_bundle(fixture.root.path(), &fixture.analyzer_plan, &output).expect_err("specialized source on wrong pass must fail");
      assert!(error.to_string().contains("attached to a non-"), "{name}: {error:#}");
      assert!(!output.exists());
   }

   let gpu = write_fixture(true, "macos-task-power-v2-common-gpu-summary:phase-gpu-time-ns");
   let output = gpu.root.path().join("gpu-bundle");
   let error = materialize_macos_analyzer_bundle(gpu.root.path(), &gpu.analyzer_plan, &output).expect_err("comparison-ineligible GPU source must not enter a claim cell");
   assert!(error.to_string().contains("comparison-ineligible"));
   assert!(!output.exists());
}

#[test]
fn bridge_rejects_ineligible_campaign_before_publication()
{
   let ineligible = write_fixture(false, MACOS_INPUT_TO_PRESENT_METRIC_SOURCE);
   let output = ineligible.root.path().join("ineligible-bundle");
   let error = materialize_macos_analyzer_bundle(ineligible.root.path(), &ineligible.analyzer_plan, &output).expect_err("ineligible campaign must fail");
   assert!(error.to_string().contains("authoritative-eligible"));
   assert!(!output.exists());

}

#[test]
fn bridge_reopens_telemetry_coverage_and_surface_before_publication()
{
   for (name, suffix) in [
      ("telemetry", "native.telemetry.bin"),
      ("coverage", "native.telemetry.coverage.json"),
      ("surface", "native.surface.json"),
   ]
   {
      let fixture = write_fixture(true, MACOS_INPUT_TO_PRESENT_METRIC_SOURCE);
      let path = fixture.root.path().join("Runs/bridge-run/minimal-presentation/minimal-presentation/pr-startup/0").join(suffix);
      fs::write(&path, format!("tampered-{name}")).expect("tamper independently reopened artifact");
      let output = fixture.root.path().join(format!("{name}-tampered-bundle"));
      let error = materialize_macos_analyzer_bundle(fixture.root.path(), &fixture.analyzer_plan, &output).expect_err("tampered publication source must fail");
      assert!(error.to_string().contains(name), "{name}: {error:#}");
      assert!(!output.exists());
   }
}

struct Fixture
{
   root: tempfile::TempDir,
   analyzer_plan: PathBuf,
}

fn write_fixture(authoritative_eligible: bool, metric_source: &str) -> Fixture
{
   let root = tempfile::tempdir().expect("temporary bridge fixture");
   let source_plan_sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
   let seed = oxide_benchmark_spec::comparison_seed_from_content_sha256(source_plan_sha256).expect("source seed");
   let order = balanced_comparison_order(seed.0, 1)[0];
   let timing = AppleCampaignPassTimingSpec {
      pass_id: String::from("minimal-presentation"),
      reset_seconds_per_session: 1,
      readiness_timeout_seconds: 1,
      scenarios: vec![AppleCampaignScenarioTimingSpec {
         scenario_id: String::from("startup.first-screen"),
         setup_seconds: 1,
         warmup_seconds: 1,
         measurement: AppleCampaignMeasurementTimingSpec::Duration {duration_seconds: 1},
      }],
   };
   let sides = if order == ComparisonOrder::Ab
   {
      [ComparisonSide::Native, ComparisonSide::Oxide]
   }
   else
   {
      [ComparisonSide::Oxide, ComparisonSide::Native]
   };
   let sessions = sides.into_iter().map(|side| MacOsCampaignSession {
      chunk_id: String::from("minimal-presentation"),
      pass_id: String::from("minimal-presentation"),
      pack_id: String::from("pr-startup"),
      pass_role: AppleCampaignPassRole::Primary,
      evidence_role: AppleCampaignEvidenceRole::ClaimBearing,
      collector: None,
      launch_class: None,
      timing: Some(timing.clone()),
      pair_index: 0,
      order,
      side,
      max_occupied_seconds: 10,
      scale_overlay: None,
   }).collect::<Vec<_>>();
   let campaign = MacOsCampaignPlan {
      schema_version: 2,
      run_id: String::from("bridge-run"),
      plan_sha256: String::from(source_plan_sha256),
      seed,
      plan_resource_path: Some(String::from("plans/test-apple.json")),
      sessions,
   };
   write_json(&root.path().join("campaign.plan.json"), &campaign);

   let mut results = Vec::new();
   for (index, session) in campaign.sessions.iter().enumerate()
   {
      let side = if session.side == ComparisonSide::Native {"native"} else {"oxide"};
      let directory = root.path().join("Runs/bridge-run/minimal-presentation/minimal-presentation/pr-startup/0");
      fs::create_dir_all(&directory).expect("session directory");
      let generation = hash(&format!("generation-{side}"));
      let executable_sha256 = hash(&format!("executable-{side}"));
      let pid = 700 + index as u32;
      let resource_path = directory.join(format!("{side}.resources.json"));
      let resource = resource_json(&campaign, session, &generation, &executable_sha256, pid, index as u64 * 10_000);
      let resource_bytes = write_json(&resource_path, &resource);
      let telemetry_path = directory.join(format!("{side}.telemetry.bin"));
      let telemetry = telemetry_with_kinds(&[1, 2, 3, 4, 5, 6, 23, 24, 26]);
      fs::write(&telemetry_path, &telemetry).expect("telemetry fixture");
      let coverage_relative = Path::new("Runs/bridge-run/minimal-presentation/minimal-presentation/pr-startup/0").join(format!("{side}.telemetry.coverage.json"));
      let coverage = telemetry_coverage_json(session.side, &session.pass_id, &[1, 2, 3, 4, 5, 6, 23, 24, 26]);
      fs::write(root.path().join(&coverage_relative), &coverage).expect("telemetry coverage fixture");
      let surface_relative = Path::new("Runs/bridge-run/minimal-presentation/minimal-presentation/pr-startup/0").join(format!("{side}.surface.json"));
      let surface = surface_json(&campaign, session, &generation);
      let surface_bytes = write_json(&root.path().join(&surface_relative), &surface);
      let complete_path = directory.join(format!("{side}.complete.json"));
      let complete = json!({
         "schemaVersion": 1,
         "runID": campaign.run_id,
         "planSHA256": campaign.plan_sha256,
         "chunkID": session.chunk_id,
         "passID": session.pass_id,
         "pairIndex": session.pair_index,
         "side": session.side,
         "generation": generation,
         "packID": session.pack_id,
         "telemetrySHA256": sha256(&telemetry),
         "telemetryByteCount": telemetry.len(),
         "telemetryCoverage": {
            "path": coverage_relative,
            "sha256": sha256(&coverage)
         },
         "surfaceReceipt": {
            "path": surface_relative,
            "sha256": sha256(&surface_bytes)
         },
         "timebaseNumerator": 1,
         "timebaseDenominator": 1,
         "injectionScope": MACOS_AUTHORITATIVE_INPUT_SCOPE,
         "validation": "complete-canonical-request-controller-application-v1"
      });
      let complete_bytes = write_json(&complete_path, &complete);
      let request = format!("request-{side}").into_bytes();
      let controller = format!("controller-{side}").into_bytes();
      let application = format!("application-{side}").into_bytes();
      for (suffix, bytes) in [("request.json", request.as_slice()), ("controller.json", controller.as_slice()), ("application.json", application.as_slice())]
      {
         fs::write(directory.join(format!("{side}.trusted-input.0.{suffix}")), bytes).expect("raw trusted-input receipt");
      }
      let receipt_manifest = MacOsTrustedInputReceiptManifest {
         schema_version: 1,
         run_id: campaign.run_id.clone(),
         plan_sha256: campaign.plan_sha256.clone(),
         chunk_id: session.chunk_id.clone(),
         pass_id: session.pass_id.clone(),
         pack_id: session.pack_id.clone(),
         pair_index: session.pair_index,
         side: session.side,
         generation: generation.clone(),
         expected_command_count: 1,
         receipts: vec![MacOsTrustedInputReceiptManifestEntry {
            command_sequence: 0,
            scenario_id: String::from("startup.first-screen"),
            request_sha256: sha256(&request),
            controller_receipt_sha256: sha256(&controller),
            application_receipt_sha256: sha256(&application),
            first_event_index: 0,
            last_event_index: 0,
            raw_event_families: vec![String::from("mouse")],
            raw_event_types: vec![1],
            state_generation_before: 1,
            state_generation_after: 2,
         }],
         complete: true,
      };
      let manifest_relative = Path::new("Runs/bridge-run/minimal-presentation/minimal-presentation/pr-non-launch/0").join(format!("{side}.trusted-input.manifest.json"));
      let manifest_bytes = write_json(&root.path().join(&manifest_relative), &receipt_manifest);
      let correlation_path = directory.join(format!("{side}.presentation.correlation.json"));
      let correlations = (0..30).map(|sample| json!({
            "process": format!("App ({pid})"),
            "scenarioIndex": 0,
            "inputGeneration": sample + 1,
            "visualGeneration": sample + 31,
            "displayOpportunityNs": 100 + sample,
            "inputReceivedNs": 200 + sample,
            "visualGenerationNs": 250 + sample,
            "updateStartNs": 260 + sample,
            "updateEndNs": 270 + sample,
            "updateSelection": "containing-visual-generation",
            "display": "Main",
            "swapId": 10 + sample,
            "frameLifetimeStartNs": 270 + sample,
            "frameLifetimeEndNs": 1200 + sample,
            "candidateInputToFrameLifetimeEndNs": 1000,
            "candidateVisualToFrameLifetimeEndNs": 950
         })).collect::<Vec<_>>();
      let correlation = json!({
         "schemaVersion": 1,
         "availability": "available-test-calibrated",
         "calibrationStatus": "accepted-test-calibration",
         "correlations": correlations,
         "uncorrelatedVisualGenerations": []
      });
      let correlation_bytes = write_json(&correlation_path, &correlation);
      results.push(MacOsCampaignSessionResult {
         session: session.clone(),
         generation,
         executable_sha256,
         artifact_sha256: sha256(&complete_bytes),
         acknowledgement_sha256: hash(&format!("ack-{side}")),
         surface_receipt_path: Some(root.path().join(&surface_relative).to_string_lossy().into_owned()),
         surface_receipt_sha256: Some(sha256(&surface_bytes)),
         trace_path: Some(directory.join(format!("{side}.presentation.trace")).to_string_lossy().into_owned()),
         trace_toc_sha256: Some(hash(&format!("toc-{side}"))),
         trace_signposts_sha256: Some(hash(&format!("signposts-{side}"))),
         trace_updates_sha256: Some(hash(&format!("updates-{side}"))),
         trace_frame_lifetimes_sha256: Some(hash(&format!("frames-{side}"))),
         trace_correlation_path: Some(correlation_path.to_string_lossy().into_owned()),
         trace_correlation_sha256: Some(sha256(&correlation_bytes)),
         trusted_input_receipt_manifest_path: Some(manifest_relative.to_string_lossy().into_owned()),
         trusted_input_receipt_manifest_sha256: Some(sha256(&manifest_bytes)),
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
         launch_evidence_path: None,
         launch_evidence_sha256: None,
         energy_config_sha256: None,
         energy_request_sha256: None,
         energy_ready_sha256: None,
         energy_raw_path: None,
         energy_raw_sha256: None,
         energy_summary_path: None,
         energy_summary_sha256: None,
         resource_path: resource_path.to_string_lossy().into_owned(),
         resource_sha256: sha256(&resource_bytes),
         resource_availability: String::from("available-proc-pid-rusage-v4"),
         disposition: String::from("fresh"),
      });
   }
   let directory = root.path().join("Runs/bridge-run/minimal-presentation/minimal-presentation/pr-startup/0");
   let checkpoint = MacOsPairCheckpoint {
      schema_version: 2,
      run_id: campaign.run_id.clone(),
      plan_sha256: campaign.plan_sha256.clone(),
      build_manifest_sha256: hash("build-manifest"),
      chunk_id: String::from("minimal-presentation"),
      pass_id: String::from("minimal-presentation"),
      pack_id: String::from("pr-startup"),
      pair_index: 0,
      predecessor_pair_sha256: None,
      sessions: results.clone(),
      correctness: None,
      complete: true,
   };
   write_json(&directory.join("pair.complete.json"), &checkpoint);
   let calibration_input = InstrumentationCalibrationInput {
      schema_version: 1,
      calibration_id: String::from("bridge-test-calibration"),
      platform_role: String::from("macos-apple-silicon"),
      template_id: String::from("Animation Hitches"),
      sensor_sample_hz: 1_000,
      alpha: 0.05,
      pairs: (0..8).map(|pair_index| InstrumentationCalibrationPair {
         pair_index,
         order: if pair_index % 2 == 0 {TracePairOrder::TraceOnFirst} else {TracePairOrder::TraceOffFirst},
         trace_on_sensor_p50_ms: 10.0,
         trace_off_sensor_p50_ms: 10.0,
         trace_on_sensor_p95_ms: 12.0,
         trace_off_sensor_p95_ms: 12.0,
         trace_on_process_cpu_ms: 5.0,
         trace_off_process_cpu_ms: 5.0,
         valid: true,
      }).collect(),
   };
   let calibration = reduce_instrumentation_calibration(&calibration_input).expect("accepted instrumentation calibration");
   assert!(calibration.accepted);
   let calibration_bytes = write_json(&root.path().join("instrumentation.calibration.json"), &calibration);
   let calibration_identity = ArtifactIdentity {path: String::from("instrumentation.calibration.json"), sha256: sha256(&calibration_bytes)};
   let environment = valid_environment();
   let pre_pair = vec![MacOsPrePairEnvironmentObservation {
      chunk_id: campaign.sessions[0].chunk_id.clone(),
      pass_id: campaign.sessions[0].pass_id.clone(),
      pack_id: campaign.sessions[0].pack_id.clone(),
      pair_index: campaign.sessions[0].pair_index,
      snapshot: environment.clone(),
      background: MacOsBackgroundObservation {
         start_timestamp_unix_ns: 1,
         end_timestamp_unix_ns: 10_000_000_001,
         requested_duration_ns: 10_000_000_000,
         source: String::from("top-11-samples-drop-first-1hz-idlew-v1"),
         sample_count: 10,
         median_cpu_basis_points: 100,
         peak_cpu_basis_points: 200,
         wakeups: observed(20),
         noisy_processes: Vec::new(),
         unrelated_gpu_busy_basis_points: observed(0),
         device_gpu_busy_basis_points: observed(0),
         raw_observation_sha256: hash("background"),
      },
   }];
   let acquisition_validity = reduce_macos_acquisition_validity_with_pre_pair_environment(root.path(), &campaign, &checkpoint.build_manifest_sha256, environment.clone(), pre_pair, environment, &results, Some(&calibration_identity)).expect("valid acquisition evidence");
   assert!(acquisition_validity.authoritative_eligible);
   let acquisition_validity_path = root.path().join("acquisition.validity.json");
   let acquisition_validity_bytes = write_json(&acquisition_validity_path, &acquisition_validity);
   let report = MacOsCampaignReport {
      schema_version: 4,
      run_id: campaign.run_id.clone(),
      plan_sha256: campaign.plan_sha256.clone(),
      build_manifest_sha256: hash("build-manifest"),
      acquisition_validity: Some(ArtifactIdentity {
         path: acquisition_validity_path.to_string_lossy().into_owned(),
         sha256: sha256(&acquisition_validity_bytes),
      }),
      sessions: results.clone(),
      correctness_pairs: Vec::new(),
      scope: MacOsCampaignScope::Full,
      acquisition_complete: true,
      correctness_accepted: true,
      correctness_eligible_for_measured_acquisition: true,
      authoritative_eligible,
      complete: true,
   };
   write_json(&root.path().join("campaign.complete.json"), &report);

   let analyzer = analyzer_plan(&campaign, &results, metric_source);
   let analyzer_plan = root.path().join("analyzer-plan.json");
   write_json(&analyzer_plan, &analyzer);
   Fixture {root, analyzer_plan}
}

fn valid_environment() -> MacOsEnvironmentSnapshot
{
   let display = MacOsDisplayObservation {
      display_id: String::from("main-display"),
      mode: String::from("1728 x 1117 @ 120.00Hz"),
      logical_width: 1728,
      logical_height: 1117,
      pixel_width: 3456,
      pixel_height: 2234,
      scale_x_milli: 2000,
      scale_y_milli: 2000,
      refresh_millihz: 120_000,
      rotation_degrees: 0,
      mirrored: false,
      connection_type: String::from("internal"),
      display_type: String::from("liquid-retina-xdr"),
      auto_brightness_enabled: observed(false),
      color_profile: observed(String::from("display-p3")),
      edr_headroom_milli: observed(1_000),
      main: true,
      online: true,
      asleep: false,
   };
   MacOsEnvironmentSnapshot {
      timestamp_unix_ns: 1,
      thermal_state: MacOsThermalState::Nominal,
      thermal_observation_sha256: hash("thermal"),
      displays: vec![display],
      display_observation_sha256: hash("display"),
      power_source: MacOsPowerSource::AcPower,
      power_observation_sha256: hash("power"),
      surface: MacOsSurfaceContractObservation {
         window_logical_size: observed(String::from("1365x1024")),
         viewport_logical_size: observed(String::from("1365x1024")),
         backing_pixel_size: observed(String::from("2730x2048")),
         content_insets: observed(String::from("0,0,0,0")),
         backing_scale_milli: observed(2_000),
         color_format: observed(String::from("bgra8unorm-srgb")),
         color_space: observed(String::from("srgb")),
         alpha_mode: observed(String::from("premultiplied")),
         sample_count: observed(1),
         compositor_scaling: observed(String::from("none")),
         target_refresh_policy: observed(String::from("native-adaptive")),
         target_refresh_millihz: observed(120_000),
         observed_refresh_millihz: observed(120_000),
         surface_implementation: observed(String::from("test-surface")),
         internal_color_format: observed(String::from("test-internal-format")),
      },
      fan_rpm: observed(2_000),
      brightness_basis_points: observed(5_000),
      luminance_millinits: observed(500_000),
      true_tone_enabled: observed(false),
      night_shift_enabled: observed(false),
      reduce_motion_enabled: observed(false),
      notifications_suppressed: observed(true),
      background_sync_quiescent: observed(true),
      screen_recording_inactive: observed(true),
      room_temperature_millicelsius: observed(22_000),
   }
}

fn observed<T>(value: T) -> MacOsEnvironmentValue<T>
{
   MacOsEnvironmentValue {availability: MacOsObservationAvailability::Available, value: Some(value), source: String::from("test-observation")}
}

fn analyzer_plan(campaign: &MacOsCampaignPlan, results: &[MacOsCampaignSessionResult], metric_source: &str) -> ComparisonPlan
{
   let native = results.iter().find(|result| result.session.side == ComparisonSide::Native).expect("native result");
   let oxide = results.iter().find(|result| result.session.side == ComparisonSide::Oxide).expect("Oxide result");
   let metric_unit = if metric_source.starts_with(MACOS_RESOURCE_METRIC_SOURCE_PREFIX)
   {
      "ms/s"
   }
   else if metric_source.ends_with("joules")
   {
      "J"
   }
   else
   {
      "ns"
   };
   let metric = MetricDefinition {
      id: String::from("interaction_to_present_ns"),
      unit: String::from(metric_unit),
      direction: MetricDirection::LowerIsBetter,
      scope: String::from("trusted-action"),
      comparability: String::from("symmetric-cross-implementation"),
      source: String::from(metric_source),
      owning_pass_id: String::from("minimal-presentation"),
      allowed_primary_cell_types: vec![String::from("dynamic")],
      sample_unit: String::from("trusted-logical-action"),
      within_session_estimator: String::from("median"),
      block_duration: String::from("scenario"),
      pair_effect: String::from("strictly-positive-ratio"),
      across_session_estimator: String::from("paired-median"),
      zero_policy: String::from("strictly-positive"),
      availability_policy: String::from("required"),
      materiality_boundary: String::from("ratio=0.95"),
      guardrail_boundary: None,
      max_interval_width: None,
      decision_alpha: 0.05,
      decision_test: String::from("exact-paired-sign"),
      exact_test_resolution_floor: DecimalU64(1),
      max_clock_uncertainty_ns: DecimalU64(100_000),
   };
   let cell = ComparisonCell {
      id: String::from("macos-startup"),
      platform: Platform::Apple,
      reference_id: String::from("native.production"),
      contender_id: String::from("oxide.production"),
      scenario_id: String::from("startup.first-screen"),
      cache_class: String::from("warm"),
      network_profile: String::from("none"),
      refresh_track: String::from("actual-observed"),
      pack_id: String::from("pr-startup"),
      primary_metric_id: metric.id.clone(),
      owning_pass_id: String::from("minimal-presentation"),
      evidence_role: EvidenceRole::RequiredClaim,
      within_session_estimator: String::from("median"),
      materiality_boundary: String::from("ratio=0.95"),
      sufficiency_rule: String::from("valid_pairs>=1"),
      required_guardrail_metric_ids: Vec::new(),
      guardrail_not_applicable_reasons: Vec::new(),
      oxide_superiority_family_id: Some(String::from("oxide-superiority")),
      reference_superiority_family_id: None,
      equivalence_lower_family_id: None,
      equivalence_upper_family_id: None,
      required_guardrail_family_id: None,
   };
   ComparisonPlan {
      schema_version: 1,
      suite_id: String::from("macos-production-comparison"),
      plan_id: String::from("macos-test"),
      plan_sha256: campaign.plan_sha256.clone(),
      tier: Tier::Pr,
      platform: Platform::Apple,
      reference: implementation("native.production", &native.executable_sha256, "accepted"),
      contender: implementation("oxide.production", &oxide.executable_sha256, "accepted"),
      common: CommonIdentity {
         harness_sha256: hash("harness"),
         pass_instrumentation_sha256: hash("instrumentation"),
         scenario_manifest_sha256: hash("scenario"),
         trace_sha256: hash("trace"),
         fixture_sha256: hash("fixture"),
         asset_manifest_sha256: hash("assets"),
         font_pack_sha256: hash("fonts"),
      },
      environment: json!({"host": "macos"}),
      seed: campaign.seed,
      scenario_ids: vec![String::from("startup.first-screen")],
      scenario_packs: vec![ScenarioPack {
         id: String::from("pr-startup"),
         ordered_scenario_ids: vec![String::from("startup.first-screen")],
         isolation_class: String::from("process"),
         reset_contract: String::from("fresh-process"),
         common_ready_predicate: String::from("durable-ready"),
         fixed_warmup_ns: DecimalU64(1),
         measured_duration_ns: DecimalU64(1),
         max_process_wall_ns: DecimalU64(10),
         sentinel_scenario_id: String::from("startup.first-screen"),
         trace_capacity_limit: DecimalU64(1),
         calibration_evidence_sha256: hash("calibration"),
      }],
      controller_chunks: vec![ControllerChunk {
         id: String::from("minimal-presentation"),
         ordered_pair_indices: vec![DecimalU64(0)],
         pack_ids: vec![String::from("pr-startup")],
         pass_id: String::from("minimal-presentation"),
         max_occupied_ns: DecimalU64(10),
         expected_heartbeat_count: DecimalU64(1),
         bundled_plan_resource_sha256: campaign.plan_sha256.clone(),
         checkpoint_generation: DecimalU64(1),
      }],
      measurement_pass_id: String::from("minimal-presentation"),
      instrumentation_profile: String::from("macos-correlated-presentation"),
      pass_pair_count: DecimalU64(1),
      process_boundary_plan: String::from("one-process-per-side"),
      comparison_cells: vec![cell.clone()],
      metric_definitions: vec![metric.clone()],
      decision_families: vec![DecisionFamily {
         id: String::from("oxide-superiority"),
         claim_kind: DecisionClaimKind::OxideSuperiority,
         alpha: 0.05,
         ordered_members: vec![DecisionFamilyMember {
            comparison_cell_id: cell.id,
            metric_id: metric.id,
            boundary_id: String::from("primary"),
            alternative: DecisionAlternative::Lower,
         }],
         exact_test_resolution_floor: DecimalU64(1),
         maximum_pair_count: DecimalU64(1),
      }],
   }
}

fn resource_json(campaign: &MacOsCampaignPlan, session: &MacOsCampaignSession, generation: &str, executable_sha256: &str, pid: u32, offset: u64) -> serde_json::Value
{
   let sample = |time, cpu| json!({
      "machContinuousTime": time,
      "userCpuNs": cpu,
      "systemCpuNs": cpu,
      "residentBytes": 1000,
      "physicalFootprintBytes": 1000,
      "lifetimeMaxPhysicalFootprintBytes": 1000,
      "intervalMaxPhysicalFootprintBytes": 1000,
      "diskReadBytes": 0,
      "diskWrittenBytes": 0,
      "instructions": cpu,
      "cycles": cpu,
      "billedSystemTimeNs": 0,
      "servicedSystemTimeNs": 0,
      "billedEnergy": 0,
      "servicedEnergy": 0
   });
   json!({
      "schemaVersion": 4,
      "runID": campaign.run_id,
      "planSHA256": campaign.plan_sha256,
      "chunkID": session.chunk_id,
      "passID": session.pass_id,
      "pairIndex": session.pair_index,
      "side": session.side,
      "generation": generation,
      "executableSHA256": executable_sha256,
      "packID": session.pack_id,
      "pid": pid,
      "processUuid": format!("{pid:032x}"),
      "processStartAbstime": 10 + offset,
      "processStartContinuousTime": 100 + offset,
      "processStartClockUncertaintyTicks": 1,
      "timebaseNumerator": 1,
      "timebaseDenominator": 1,
      "cadenceNs": 50_000_000,
      "launchT0": 99 + offset,
      "durableCompleteTimestamp": 190 + offset,
      "availability": "available-proc-pid-rusage-v4",
      "samples": [sample(100 + offset, 10), sample(200 + offset, 20)],
      "complete": true
   })
}

fn telemetry_with_kinds(kinds: &[u16]) -> Vec<u8>
{
   const HEADER_BYTES: usize = 136;
   const RECORD_BYTES: usize = 44;
   let mut bytes = Vec::with_capacity(HEADER_BYTES + kinds.len() * RECORD_BYTES + 32);
   bytes.extend_from_slice(b"OXBTEL02");
   bytes.extend_from_slice(&2_u32.to_le_bytes());
   bytes.extend_from_slice(&(HEADER_BYTES as u32).to_le_bytes());
   bytes.extend_from_slice(&(RECORD_BYTES as u32).to_le_bytes());
   bytes.extend_from_slice(&1_u32.to_le_bytes());
   bytes.extend_from_slice(&(kinds.len() as u64).to_le_bytes());
   bytes.extend_from_slice(&(kinds.len() as u64).to_le_bytes());
   bytes.resize(HEADER_BYTES, 0);
   for (index, kind) in kinds.iter().enumerate()
   {
      bytes.extend_from_slice(&(index as u64).to_le_bytes());
      bytes.extend_from_slice(&(index as u64 + 1).to_le_bytes());
      bytes.extend_from_slice(&kind.to_le_bytes());
      bytes.extend_from_slice(&0_u16.to_le_bytes());
      bytes.extend_from_slice(&u64::from(*kind).to_le_bytes());
      bytes.extend_from_slice(&0_i64.to_le_bytes());
      bytes.extend_from_slice(&0_i64.to_le_bytes());
   }
   let digest = Sha256::digest(&bytes);
   bytes.extend_from_slice(&digest);
   bytes
}

fn telemetry_coverage_json(side: ComparisonSide, pass_id: &str, kinds: &[u16]) -> Vec<u8>
{
   let names = [
      "scenarioBegin", "scenarioEnd", "phaseBegin", "phaseEnd", "checkpointBegin", "checkpointEnd",
      "inputReceived", "mutationBegin", "mutationEnd", "layoutBegin", "layoutEnd", "sceneUpdateBegin",
      "sceneUpdateEnd", "renderPrepareBegin", "renderPrepareEnd", "encodeBegin", "encodeEnd", "commandSubmit",
      "gpuStart", "gpuEnd", "presentation", "firstMeaningfulFrame", "readyToInput", "displayOpportunity",
      "resetComplete", "callbackCadence", "drawableWait", "inflightDepth", "updateBacklog", "logicalUpdateCompleted",
      "logicalUpdateSkipped", "quiescence", "gpuDuration",
   ];
   let entries = names.iter().enumerate().map(|(index, name)| {
      let raw = (index + 1) as u16;
      let availability = match raw
      {
         1..=6 | 23 | 24 | 26 => "ring-required",
         7..=9 | 12 | 13 | 25 | 30 | 32 => "ring-conditional",
         21 | 22 => "external-evidence",
         14..=18 | 27 | 33 if side == ComparisonSide::Oxide => "diagnostic-not-enabled-for-pass",
         _ => "unavailable",
      };
      json!({
         "kind": name,
         "rawValue": raw,
         "availability": availability,
         "observedCount": kinds.iter().filter(|kind| **kind == raw).count(),
         "source": "test-observed-boundary"
      })
   }).collect::<Vec<_>>();
   serde_json::to_vec(&json!({
      "schemaVersion": 1,
      "side": side,
      "passID": pass_id,
      "entries": entries,
      "validation": "complete-kind-availability-and-observed-counts"
   })).expect("coverage JSON")
}

fn surface_json(campaign: &MacOsCampaignPlan, session: &MacOsCampaignSession, generation: &str) -> serde_json::Value
{
   json!({
      "schemaVersion": 1,
      "runID": campaign.run_id,
      "planSHA256": campaign.plan_sha256,
      "chunkID": session.chunk_id,
      "passID": session.pass_id,
      "pairIndex": session.pair_index,
      "side": session.side,
      "generation": generation,
      "packID": session.pack_id,
      "snapshot": {
         "windowLogicalWidthMilliPoints": 1365000,
         "windowLogicalHeightMilliPoints": 1024000,
         "viewportLogicalWidthMilliPoints": 1365000,
         "viewportLogicalHeightMilliPoints": 1024000,
         "backingPixelWidth": 2730,
         "backingPixelHeight": 2048,
         "insetTopMilliPoints": 0,
         "insetLeftMilliPoints": 0,
         "insetBottomMilliPoints": 0,
         "insetRightMilliPoints": 0,
         "backingScaleMilli": 2000,
         "finalColorFormat": "bgra8unorm-srgb",
         "finalColorSpace": "srgb",
         "alphaMode": "premultiplied",
         "sampleCount": 1,
         "compositorScaling": "none",
         "targetRefreshPolicy": "native-adaptive",
         "targetRefreshMillihz": 120000,
         "surfaceImplementation": "test-surface",
         "internalColorFormat": "test-internal-format"
      },
      "observedRefreshMillihz": 120000,
      "observedRefreshSource": "test-observed-refresh",
      "validation": "complete-app-reported-content-hash-bound-surface-v1"
   })
}

fn implementation(id: &str, executable_sha256: &str, status: &str) -> ImplementationIdentity
{
   ImplementationIdentity {
      id: String::from(id),
      variant: String::from("production"),
      source_commit: String::from("0123456789abcdef0123456789abcdef01234567"),
      source_tree: String::from("worktree"),
      build_command_hash: hash("build-command"),
      build_flags: vec![String::from("release")],
      executable_or_bundle_sha256: String::from(executable_sha256),
      shipping_payload_manifest_sha256: hash("payload"),
      reference_audit_sha256: hash("audit"),
      comparator_acceptance_status: String::from(status),
   }
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Vec<u8>
{
   let mut bytes = serde_json::to_vec_pretty(value).expect("fixture JSON");
   bytes.push(b'\n');
   fs::write(path, &bytes).expect("write fixture JSON");
   bytes
}

fn hash(label: &str) -> String
{
   format!("{:x}", Sha256::digest(label.as_bytes()))
}

fn sha256(bytes: &[u8]) -> String
{
   format!("{:x}", Sha256::digest(bytes))
}
