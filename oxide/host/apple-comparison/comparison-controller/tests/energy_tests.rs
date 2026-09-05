#[path = "../src/energy.rs"]
mod energy;

use energy::{macos_energy_calibration_sha256, macos_energy_unavailable_artifact, reduce_macos_energy, validate_macos_energy_adapter_request, validate_macos_external_meter_config, MacOsEnergyAdapterRequest, MacOsEnergyBatteryState, MacOsEnergyCalibration, MacOsEnergyPowerTopology, MacOsEnergyRefreshBehavior, MacOsExternalMeterConfig, MacOsExternalMeterRawArtifact, MacOsExternalMeterSample, MACOS_ENERGY_AVAILABILITY, MACOS_ENERGY_UNAVAILABLE};
use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::PermissionsExt;

const SECOND: u64 = 1_000_000_000;

fn calibration() -> MacOsEnergyCalibration
{
   MacOsEnergyCalibration {
      calibration_id: String::from("fake-calibration-1"),
      calibrated_at_utc: String::from("2026-07-21T00:00:00Z"),
      meter_model: String::from("deterministic-fake-meter"),
      meter_serial: String::from("fake-0001"),
      sampling_rate_hz: 1.0,
      integration_uncertainty_joules: 0.5,
      baseline_watts: 2.0,
      display_included: true,
      display_luminance_nits: 200.0,
      refresh_behavior: MacOsEnergyRefreshBehavior::NativeAdaptive,
      power_topology: MacOsEnergyPowerTopology::AcMainsCompleteSystem,
      battery_state: MacOsEnergyBatteryState::PresentNotCharging,
      battery_charge_min_percent: 79.0,
      battery_charge_max_percent: 81.0,
      room_temperature_celsius: 22.0,
   }
}

fn raw(calibration: &MacOsEnergyCalibration) -> MacOsExternalMeterRawArtifact
{
   MacOsExternalMeterRawArtifact {
      schema_version: 1,
      adapter_sha256: "a".repeat(64),
      calibration_sha256: macos_energy_calibration_sha256(calibration).expect("calibration hash"),
      run_id: String::from("fake-energy-run"),
      plan_sha256: "b".repeat(64),
      generation: "c".repeat(64),
      pid: 42,
      timebase_numerator: 1,
      timebase_denominator: 1,
      capture_start_ticks: 0,
      capture_end_ticks: 240 * SECOND,
      samples: (0..=240).map(|second| MacOsExternalMeterSample {host_ticks: second * SECOND, watts: 10.0}).collect(),
      complete: true,
   }
}

fn telemetry(stabilization_end: u64, measured_end: u64) -> Vec<u8>
{
   let prewarm = stable_id("prewarm");
   let boundaries = [
      (0, 3, 0, prewarm),
      (stabilization_end, 4, 0, prewarm),
      (120 * SECOND, 3, 1, stable_id("first-mount")),
      (150 * SECOND, 4, 1, stable_id("first-mount")),
      (150 * SECOND, 3, 1, stable_id("clean-idle")),
      (180 * SECOND, 4, 1, stable_id("clean-idle")),
      (180 * SECOND, 3, 1, stable_id("leaf-updates")),
      (210 * SECOND, 4, 1, stable_id("leaf-updates")),
      (210 * SECOND, 3, 1, stable_id("update-10-percent")),
      (measured_end, 4, 1, stable_id("update-10-percent")),
   ];
   let mut bytes = Vec::new();
   bytes.extend_from_slice(b"OXBTEL02");
   bytes.extend_from_slice(&2_u32.to_le_bytes());
   bytes.extend_from_slice(&136_u32.to_le_bytes());
   bytes.extend_from_slice(&44_u32.to_le_bytes());
   bytes.extend_from_slice(&1_u32.to_le_bytes());
   bytes.extend_from_slice(&(boundaries.len() as u64).to_le_bytes());
   bytes.extend_from_slice(&(boundaries.len() as u64).to_le_bytes());
   bytes.extend_from_slice(&[0_u8; 88]);
   bytes.extend_from_slice(&1_u32.to_le_bytes());
   bytes.extend_from_slice(&1_u32.to_le_bytes());
   assert_eq!(bytes.len(), 136);
   for (sequence, (ticks, kind, flags, identifier)) in boundaries.into_iter().enumerate()
   {
      bytes.extend_from_slice(&(sequence as u64).to_le_bytes());
      bytes.extend_from_slice(&ticks.to_le_bytes());
      bytes.extend_from_slice(&(kind as u16).to_le_bytes());
      bytes.extend_from_slice(&(flags as u16).to_le_bytes());
      bytes.extend_from_slice(&identifier.to_le_bytes());
      bytes.extend_from_slice(&0_u64.to_le_bytes());
      bytes.extend_from_slice(&0_u64.to_le_bytes());
   }
   let digest = Sha256::digest(&bytes);
   bytes.extend_from_slice(&digest);
   bytes
}

fn stable_id(value: &str) -> u64
{
   value.as_bytes().iter().fold(0xcbf29ce484222325, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3))
}

#[test]
fn deterministic_fake_meter_reduces_exact_windows_and_phase_bound_energy()
{
   let calibration = calibration();
   let summary = reduce_macos_energy(&raw(&calibration), &calibration, &telemetry(120 * SECOND, 240 * SECOND)).expect("energy summary");
   assert_eq!(summary.availability, MACOS_ENERGY_AVAILABILITY);
   assert_eq!(summary.stabilization_seconds, 120.0);
   assert_eq!(summary.measured_seconds, 120.0);
   assert_eq!(summary.phases.len(), 4);
   assert_eq!(summary.joules, 1_200.0);
   assert_eq!(summary.average_watts, 10.0);
   assert_eq!(summary.baseline_adjusted_joules, 960.0);
   assert_eq!(summary.baseline_adjusted_average_watts, 8.0);
   assert_eq!(summary.integration_uncertainty_joules, 0.5);
   assert!(summary.phases.iter().all(|phase| phase.duration_seconds == 30.0 && phase.joules == 300.0 && phase.average_watts == 10.0));
}

#[test]
fn fake_meter_rejects_window_boundary_sample_and_calibration_mismatch()
{
   let calibration = calibration();
   let valid_raw = raw(&calibration);
   assert!(reduce_macos_energy(&valid_raw, &calibration, &telemetry(119 * SECOND, 240 * SECOND)).is_err());
   assert!(reduce_macos_energy(&valid_raw, &calibration, &telemetry(120 * SECOND, 239 * SECOND)).is_err());

   let mut missing_sample = valid_raw.clone();
   missing_sample.samples.remove(100);
   missing_sample.samples.remove(100);
   assert!(reduce_macos_energy(&missing_sample, &calibration, &telemetry(120 * SECOND, 240 * SECOND)).is_err());

   let mut wrong_calibration = valid_raw;
   wrong_calibration.calibration_sha256 = "d".repeat(64);
   assert!(reduce_macos_energy(&wrong_calibration, &calibration, &telemetry(120 * SECOND, 240 * SECOND)).is_err());
}

#[test]
fn direct_meter_config_requires_exact_adapter_bytes_and_complete_calibration()
{
   let root = tempfile::tempdir().expect("fake meter root");
   let adapter = root.path().join("fake-meter");
   fs::write(&adapter, b"deterministic fake meter adapter").expect("fake adapter");
   let mut permissions = fs::metadata(&adapter).expect("fake adapter metadata").permissions();
   permissions.set_mode(0o700);
   fs::set_permissions(&adapter, permissions).expect("fake adapter mode");
   let bytes = fs::read(&adapter).expect("fake adapter bytes");
   let mut config = MacOsExternalMeterConfig {
      schema_version: 1,
      adapter_kind: String::from("direct-external-meter"),
      adapter_path: adapter,
      adapter_sha256: format!("{:x}", Sha256::digest(bytes)),
      calibration: calibration(),
   };
   validate_macos_external_meter_config(&config).expect("valid fake meter config");

   config.calibration.display_included = false;
   assert!(validate_macos_external_meter_config(&config).is_err());
   config.calibration = calibration();
   config.adapter_sha256 = "e".repeat(64);
   assert!(validate_macos_external_meter_config(&config).is_err());
}

#[test]
fn adapter_protocol_prohibits_invasive_tools_and_unavailable_never_claims_measurement()
{
   let root = tempfile::tempdir().expect("fake protocol root");
   let request = MacOsEnergyAdapterRequest {
      schema_version: 1,
      protocol: String::from("oxide-direct-external-meter-v1"),
      run_id: String::from("fake-run"),
      plan_sha256: "a".repeat(64),
      generation: "b".repeat(64),
      pid: 42,
      adapter_sha256: "c".repeat(64),
      config_sha256: "e".repeat(64),
      calibration_sha256: "d".repeat(64),
      stabilization_seconds: 120,
      measurement_seconds: 120,
      response_path: root.path().join("raw.partial.json"),
      ready_path: root.path().join("ready.json"),
      start_notification: String::from("com.oxide.energy.start"),
      stop_notification: String::from("com.oxide.energy.stop"),
      profiler_allowed: false,
      screen_recording_allowed: false,
      debug_transport_allowed: false,
   };
   validate_macos_energy_adapter_request(&request).expect("isolated direct-meter request");
   let mut invasive = request;
   invasive.profiler_allowed = true;
   assert!(validate_macos_energy_adapter_request(&invasive).is_err());

   let unavailable = macos_energy_unavailable_artifact("no configured direct external meter").expect("unavailable artifact");
   assert_eq!(unavailable.availability, MACOS_ENERGY_UNAVAILABLE);
   assert!(!unavailable.measurement_claimed);
   assert!(unavailable.complete);
}
