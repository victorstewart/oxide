use oxide_apple_comparison_controller::{
   collect_macos_background_observation, macos_environment_policies, validate_macos_acquisition_validity, ComparisonSide, MacOsAcquisitionValidityReport,
   MacOsBackgroundObservation, MacOsDisplayObservation, MacOsEnvironmentSnapshot,
   MacOsEnvironmentValue, MacOsMeasuredInputObservation, MacOsObservationAvailability,
   MacOsOpportunityObservation, MacOsPowerSource, MacOsPrePairEnvironmentObservation,
   MacOsSurfaceContractObservation, MacOsThermalState,
   MACOS_MINIMUM_CORRELATED_OPPORTUNITIES_PER_SESSION, MACOS_RAW_APPLICATION_RECEIPT_STATUS_COMPLETE,
};
use oxide_benchmark_spec::ArtifactIdentity;
use sha2::{Digest, Sha256};

#[test]
fn complete_receipts_stay_blocked_by_diagnostic_input_scope()
{
   let report = valid_report();
   validate_macos_acquisition_validity(&report).expect("complete available validity evidence");
   assert!(!report.authoritative_eligible);
}

#[test]
fn forged_gate_flag_is_rejected()
{
   let mut report = valid_report();
   report.opportunities[0].correlated_count = MACOS_MINIMUM_CORRELATED_OPPORTUNITIES_PER_SESSION - 1;
   let error = validate_macos_acquisition_validity(&report).expect_err("forged opportunity gate must fail");
   assert!(error.to_string().contains("gate flags differ"));
}

#[test]
fn surface_internal_fields_are_record_only_and_observed_refresh_uses_half_percent_tolerance()
{
   let mut report = valid_report();
   report.measured_input[1].surface_contract.surface_implementation = observed(String::from("different-implementation"));
   report.measured_input[1].surface_contract.internal_color_format = observed(String::from("different-internal-format"));
   report.measured_input[1].surface_contract.observed_refresh_millihz = observed(120_500);
   validate_macos_acquisition_validity(&report).expect("record-only internals and refresh within 0.5 percent");
}

#[test]
fn trace_and_external_sensor_gates_reject_distinct_calibration_identities()
{
   let mut report = valid_report();
   report.external_sensor_calibration_sha256 = Some(hash("different-calibration"));
   let error = validate_macos_acquisition_validity(&report).expect_err("split calibration identity must fail");
   assert!(error.to_string().contains("must share one calibration evidence identity"));
}

#[test]
fn unavailable_calibrations_remain_explicit_and_non_authoritative()
{
   let mut report = valid_report();
   report.trace_overhead_calibration_sha256 = None;
   report.external_sensor_calibration_sha256 = None;
   report.trace_overhead_gate_passed = false;
   report.external_sensor_gate_passed = false;
   report.all_observed_gates_passed = false;
   report.authoritative_eligible = false;
   validate_macos_acquisition_validity(&report).expect("honest unavailable evidence");
   assert!(report.common_gpu_evidence_sha256.is_empty());
}

#[test]
fn diagnostic_direct_callback_input_remains_non_authoritative()
{
   let report = valid_report();
   validate_macos_acquisition_validity(&report).expect("honest diagnostic input evidence");
   assert!(!report.authoritative_eligible);
}

#[test]
fn unavailable_gpu_observation_cannot_support_comparable_gpu_claims()
{
   let mut report = valid_report();
   admit_global_claims(&mut report);
   report.pre_pair[0].background.unrelated_gpu_busy_basis_points = unavailable();
   report.pre_pair[0].background.device_gpu_busy_basis_points = unavailable();
   report.comparable_gpu_claims_available = false;
   validate_macos_acquisition_validity(&report).expect("explicit unavailable GPU observation");
   assert!(report.authoritative_eligible);
   assert!(!report.comparable_gpu_claims_available);
}

#[test]
fn unavailable_room_temperature_only_blocks_short_run_energy_equivalence()
{
   let mut report = valid_report();
   admit_global_claims(&mut report);
   report.before.room_temperature_millicelsius = unavailable();
   report.pre_pair[0].snapshot.room_temperature_millicelsius = unavailable();
   report.after.room_temperature_millicelsius = unavailable();
   report.room_temperature_gate_passed = false;
   report.short_run_energy_equivalence_available = false;
   validate_macos_acquisition_validity(&report).expect("record-only unavailable room temperature");
   assert!(report.authoritative_eligible);
}

#[test]
fn missing_pre_pair_observation_is_valid_but_fail_closed()
{
   let mut report = valid_report();
   report.pre_pair.clear();
   report.pre_pair_environment_gate_passed = false;
   report.background_noise_gate_passed = false;
   report.comparable_gpu_claims_available = false;
   report.all_observed_gates_passed = false;
   report.authoritative_eligible = false;
   validate_macos_acquisition_validity(&report).expect("honest missing pre-pair observation");
}

#[test]
fn forged_comparable_gpu_availability_is_rejected()
{
   let mut report = valid_report();
   report.pre_pair[0].background.device_gpu_busy_basis_points = unavailable();
   let error = validate_macos_acquisition_validity(&report).expect_err("forged GPU claim availability must fail");
   assert!(error.to_string().contains("environment contract gate flags differ"));
}

#[test]
fn section_nine_cpu_and_wakeup_limits_are_enforced()
{
   let mut report = valid_report();
   report.pre_pair[0].background.median_cpu_basis_points = 501;
   let error = validate_macos_acquisition_validity(&report).expect_err("CPU median over five percent of one core must fail");
   assert!(error.to_string().contains("gate flags differ"));

   let mut report = valid_report();
   report.pre_pair[0].background.peak_cpu_basis_points = 2_001;
   let error = validate_macos_acquisition_validity(&report).expect_err("CPU peak over twenty percent of one core must fail");
   assert!(error.to_string().contains("gate flags differ"));

   let mut report = valid_report();
   report.pre_pair[0].background.wakeups = observed(51);
   let error = validate_macos_acquisition_validity(&report).expect_err("more than five unrelated wakeups per second must fail");
   assert!(error.to_string().contains("gate flags differ"));
}

#[test]
fn schema_owned_environment_policy_cannot_be_rewritten()
{
   let mut report = valid_report();
   report.environment_policies.remove("pre-run-unrelated-gpu");
   let error = validate_macos_acquisition_validity(&report).expect_err("environment policy deletion must fail");
   assert!(error.to_string().contains("schema-owned contract"));
}

#[cfg(target_os = "macos")]
#[test]
fn live_background_observer_collects_exact_ten_second_sample_contract()
{
   let observation = collect_macos_background_observation().expect("bounded live macOS background observation");
   assert_eq!(observation.sample_count, 10);
   assert_eq!(observation.requested_duration_ns, 10_000_000_000);
   assert!(observation.end_timestamp_unix_ns >= observation.start_timestamp_unix_ns);
   assert_eq!(observation.raw_observation_sha256.len(), 64);
}

fn valid_report() -> MacOsAcquisitionValidityReport
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
   let environment = MacOsEnvironmentSnapshot {
      timestamp_unix_ns: 1,
      thermal_state: MacOsThermalState::Nominal,
      thermal_observation_sha256: hash("thermal"),
      displays: vec![display],
      display_observation_sha256: hash("display"),
      power_source: MacOsPowerSource::AcPower,
      power_observation_sha256: hash("power"),
      surface: surface_contract(),
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
   };
   let pre_pair = MacOsPrePairEnvironmentObservation {
      chunk_id: String::from("chunk"),
      pass_id: String::from("pass"),
      pack_id: String::from("pack"),
      pair_index: 0,
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
   };
   MacOsAcquisitionValidityReport {
      schema_version: 2,
      run_id: String::from("validity-run"),
      plan_sha256: hash("plan"),
      build_manifest_sha256: hash("build"),
      before: environment.clone(),
      expected_pair_count: 1,
      pre_pair: vec![pre_pair],
      after: environment,
      opportunities: vec![
         opportunity(ComparisonSide::Native, "native-correlation"),
         opportunity(ComparisonSide::Oxide, "oxide-correlation"),
      ],
      measured_input: vec![measured_input(ComparisonSide::Native, "native-envelope"), measured_input(ComparisonSide::Oxide, "oxide-envelope")],
      presentation_calibration_statuses: vec![String::from("accepted-controlled-display")],
      trace_overhead_calibration_sha256: Some(hash("instrumentation-calibration")),
      external_sensor_calibration_sha256: Some(hash("instrumentation-calibration")),
      common_gpu_evidence_sha256: Vec::new(),
      environment_policies: macos_environment_policies(),
      surface_contract_gate_passed: true,
      pre_pair_environment_gate_passed: true,
      fan_gate_passed: true,
      brightness_gate_passed: true,
      auto_brightness_gate_passed: true,
      true_tone_gate_passed: true,
      night_shift_gate_passed: true,
      reduce_motion_gate_passed: true,
      notifications_gate_passed: true,
      background_sync_gate_passed: true,
      screen_recording_gate_passed: true,
      room_temperature_gate_passed: true,
      comparable_gpu_claims_available: true,
      display_sensitive_claims_available: true,
      short_run_energy_equivalence_available: true,
      interaction_environment_claims_available: true,
      thermal_gate_passed: true,
      display_gate_passed: true,
      background_noise_gate_passed: true,
      power_source_gate_passed: true,
      opportunity_gate_passed: true,
      measured_input_gate_passed: false,
      presentation_calibration_gate_passed: true,
      trace_overhead_gate_passed: true,
      external_sensor_gate_passed: true,
      all_observed_gates_passed: false,
      authoritative_eligible: false,
   }
}

fn observed<T>(value: T) -> MacOsEnvironmentValue<T>
{
   MacOsEnvironmentValue {availability: MacOsObservationAvailability::Available, value: Some(value), source: String::from("test-observation")}
}

fn surface_contract() -> MacOsSurfaceContractObservation
{
   MacOsSurfaceContractObservation {
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
   }
}

fn admit_global_claims(report: &mut MacOsAcquisitionValidityReport)
{
   for observation in &mut report.measured_input
   {
      observation.injection_scope = String::from("trusted-os-input-plus-declared-application-stimuli");
   }
   report.measured_input_gate_passed = true;
   report.all_observed_gates_passed = true;
   report.authoritative_eligible = true;
}

fn unavailable<T>() -> MacOsEnvironmentValue<T>
{
   MacOsEnvironmentValue {availability: MacOsObservationAvailability::Unavailable, value: None, source: String::from("unavailable-test")}
}

fn measured_input(side: ComparisonSide, identity: &str) -> MacOsMeasuredInputObservation
{
   MacOsMeasuredInputObservation {
      pair_index: 0,
      side,
      complete_envelope_sha256: hash(identity),
      injection_scope: String::from("direct-callback-diagnostic"),
      validation: String::from("complete-diagnostic-not-claim-bearing"),
      raw_application_receipt_status: String::from(MACOS_RAW_APPLICATION_RECEIPT_STATUS_COMPLETE),
      raw_application_receipt_manifest: Some(ArtifactIdentity {
         path: format!("Runs/run/chunk/pass/pack/0/{identity}.trusted-input.manifest.json"),
         sha256: hash(&format!("{identity}-receipts")),
      }),
      surface_receipt: ArtifactIdentity {
         path: format!("Runs/run/chunk/pass/pack/0/{identity}.surface.json"),
         sha256: hash(&format!("{identity}-surface")),
      },
      surface_contract: surface_contract(),
   }
}

fn opportunity(side: ComparisonSide, identity: &str) -> MacOsOpportunityObservation
{
   MacOsOpportunityObservation {
      pair_index: 0,
      side,
      correlated_count: MACOS_MINIMUM_CORRELATED_OPPORTUNITIES_PER_SESSION,
      display_opportunity_count: MACOS_MINIMUM_CORRELATED_OPPORTUNITIES_PER_SESSION,
      uncorrelated_count: 0,
      correlation_sha256: hash(identity),
   }
}

fn hash(value: &str) -> String
{
   format!("{:x}", Sha256::digest(value.as_bytes()))
}
