use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const MACOS_ENERGY_STABILIZATION_SECONDS: u64 = 120;
pub const MACOS_ENERGY_MEASUREMENT_SECONDS: u64 = 120;
pub const MACOS_ENERGY_AVAILABILITY: &str = "available-direct-external-meter-phase-bound";
pub const MACOS_ENERGY_UNAVAILABLE: &str = "unavailable-no-direct-external-meter-configured";

const TELEMETRY_HEADER_BYTES: usize = 136;
const TELEMETRY_RECORD_BYTES: usize = 44;
const TELEMETRY_FOOTER_BYTES: usize = 32;
const TELEMETRY_PHASE_BEGIN: u16 = 3;
const TELEMETRY_PHASE_END: u16 = 4;
const TELEMETRY_MEASURED_FLAG: u16 = 1;
const BOUNDARY_TOLERANCE_NS: u64 = 50_000_000;
const ENERGY_ADAPTER_OUTPUT_LIMIT_BYTES: u64 = 64 * 1_024 * 1_024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MacOsEnergyRefreshBehavior
{
   NativeAdaptive,
   Fixed60Hz,
   Fixed120Hz,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MacOsEnergyPowerTopology
{
   AcMainsCompleteSystem,
   DcInlineCompleteSystem,
   DcInlineHostOnly,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MacOsEnergyBatteryState
{
   Absent,
   PresentNotCharging,
   PresentCharging,
   Discharging,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsEnergyCalibration
{
   pub calibration_id: String,
   pub calibrated_at_utc: String,
   pub meter_model: String,
   pub meter_serial: String,
   pub sampling_rate_hz: f64,
   pub integration_uncertainty_joules: f64,
   pub baseline_watts: f64,
   pub display_included: bool,
   pub display_luminance_nits: f64,
   pub refresh_behavior: MacOsEnergyRefreshBehavior,
   pub power_topology: MacOsEnergyPowerTopology,
   pub battery_state: MacOsEnergyBatteryState,
   pub battery_charge_min_percent: f64,
   pub battery_charge_max_percent: f64,
   pub room_temperature_celsius: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsExternalMeterConfig
{
   pub schema_version: u32,
   pub adapter_kind: String,
   pub adapter_path: PathBuf,
   pub adapter_sha256: String,
   pub calibration: MacOsEnergyCalibration,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsExternalMeterSample
{
   pub host_ticks: u64,
   pub watts: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsExternalMeterRawArtifact
{
   pub schema_version: u32,
   pub adapter_sha256: String,
   pub calibration_sha256: String,
   pub run_id: String,
   pub plan_sha256: String,
   pub generation: String,
   pub pid: u32,
   pub timebase_numerator: u32,
   pub timebase_denominator: u32,
   pub capture_start_ticks: u64,
   pub capture_end_ticks: u64,
   pub samples: Vec<MacOsExternalMeterSample>,
   pub complete: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsEnergyUnavailableArtifact
{
   pub schema_version: u32,
   pub availability: String,
   pub reason: String,
   pub measurement_claimed: bool,
   pub complete: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsEnergyAdapterRequest
{
   pub schema_version: u32,
   pub protocol: String,
   pub run_id: String,
   pub plan_sha256: String,
   pub generation: String,
   pub pid: u32,
   pub adapter_sha256: String,
   pub config_sha256: String,
   pub calibration_sha256: String,
   pub stabilization_seconds: u64,
   pub measurement_seconds: u64,
   pub response_path: PathBuf,
   pub ready_path: PathBuf,
   pub start_notification: String,
   pub stop_notification: String,
   pub profiler_allowed: bool,
   pub screen_recording_allowed: bool,
   pub debug_transport_allowed: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsEnergyAdapterReady
{
   pub schema_version: u32,
   pub protocol: String,
   pub request_sha256: String,
   pub adapter_sha256: String,
   pub calibration_sha256: String,
   pub ready_ticks: u64,
   pub hardware_ready: bool,
   pub complete: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsEnergyPhaseSummary
{
   pub phase_identifier: u64,
   pub begin_ticks: u64,
   pub end_ticks: u64,
   pub duration_seconds: f64,
   pub joules: f64,
   pub average_watts: f64,
   pub baseline_adjusted_joules: f64,
   pub baseline_adjusted_average_watts: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsEnergySummary
{
   pub schema_version: u32,
   pub availability: String,
   pub calibration_sha256: String,
   pub calibration: MacOsEnergyCalibration,
   pub stabilization_begin_ticks: u64,
   pub stabilization_end_ticks: u64,
   pub stabilization_seconds: f64,
   pub measured_begin_ticks: u64,
   pub measured_end_ticks: u64,
   pub measured_seconds: f64,
   pub joules: f64,
   pub average_watts: f64,
   pub baseline_adjusted_joules: f64,
   pub baseline_adjusted_average_watts: f64,
   pub integration_uncertainty_joules: f64,
   pub phases: Vec<MacOsEnergyPhaseSummary>,
}

#[derive(Clone, Copy, Debug)]
struct TelemetryHeader
{
   timebase_numerator: u32,
   timebase_denominator: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PhaseBoundary
{
   begin: bool,
   measured: bool,
   identifier: u64,
   ticks: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PhaseInterval
{
   identifier: u64,
   begin_ticks: u64,
   end_ticks: u64,
}

pub fn load_macos_external_meter_config(path: &Path) -> Result<(MacOsExternalMeterConfig, String)>
{
   let bytes = fs::read(path).with_context(|| format!("reading external-meter config {}", path.display()))?;
   let config: MacOsExternalMeterConfig = serde_json::from_slice(&bytes).with_context(|| format!("decoding external-meter config {}", path.display()))?;
   validate_macos_external_meter_config(&config)?;
   Ok((config, format!("{:x}", Sha256::digest(&bytes))))
}

pub fn macos_energy_unavailable_artifact(reason: &str) -> Result<MacOsEnergyUnavailableArtifact>
{
   if reason.is_empty()
   {
      bail!("macOS energy unavailability reason is empty");
   }
   Ok(MacOsEnergyUnavailableArtifact {
      schema_version: 1,
      availability: String::from(MACOS_ENERGY_UNAVAILABLE),
      reason: String::from(reason),
      measurement_claimed: false,
      complete: true,
   })
}

pub fn validate_macos_energy_adapter_request(request: &MacOsEnergyAdapterRequest) -> Result<()>
{
   if request.schema_version != 1
      || request.protocol != "oxide-direct-external-meter-v1"
      || request.run_id.is_empty()
      || request.generation.is_empty()
      || request.pid == 0
      || request.stabilization_seconds != MACOS_ENERGY_STABILIZATION_SECONDS
      || request.measurement_seconds != MACOS_ENERGY_MEASUREMENT_SECONDS
      || !request.response_path.is_absolute()
      || !request.ready_path.is_absolute()
      || request.response_path == request.ready_path
      || request.start_notification.is_empty()
      || request.stop_notification.is_empty()
      || request.profiler_allowed
      || request.screen_recording_allowed
      || request.debug_transport_allowed
   {
      bail!("macOS direct-meter adapter request violates the isolated energy protocol");
   }
   validate_sha256(&request.plan_sha256)?;
   validate_sha256(&request.adapter_sha256)?;
   validate_sha256(&request.config_sha256)?;
   validate_sha256(&request.calibration_sha256)
}

pub(crate) struct MacOsEnergyAdapterProcess
{
   child: Child,
   process_group: u32,
   response_path: PathBuf,
   ready_path: PathBuf,
   request_sha256: String,
   adapter_sha256: String,
   calibration_sha256: String,
   process_group_active: bool,
   retain_response: bool,
}

impl MacOsEnergyAdapterProcess
{
   pub(crate) fn start(config: &MacOsExternalMeterConfig, request: &MacOsEnergyAdapterRequest, request_path: &Path, stdout: File, stderr: File) -> Result<Self>
   {
      validate_macos_external_meter_config(config)?;
      validate_macos_energy_adapter_request(request)?;
      if request.adapter_sha256 != config.adapter_sha256 || request.calibration_sha256 != macos_energy_calibration_sha256(&config.calibration)?
      {
         bail!("macOS energy request differs from its configured adapter or calibration");
      }
      for path in [request_path, request.response_path.as_path(), request.ready_path.as_path()]
      {
         if path.exists()
         {
            bail!("refusing to overwrite energy adapter artifact {}", path.display());
         }
      }
      persist_json(request, request_path)?;
      let request_bytes = fs::read(request_path).with_context(|| format!("reading energy request {}", request_path.display()))?;
      let request_sha256 = format!("{:x}", Sha256::digest(&request_bytes));
      let child = Command::new(&config.adapter_path)
         .args(["capture", "--request"])
         .arg(request_path)
         .process_group(0)
         .stdin(Stdio::null())
         .stdout(Stdio::from(stdout))
         .stderr(Stdio::from(stderr))
         .spawn()
         .with_context(|| format!("launching direct external-meter adapter {}", config.adapter_path.display()))?;
      Ok(Self {
         process_group: child.id(),
         child,
         response_path: request.response_path.clone(),
         ready_path: request.ready_path.clone(),
         request_sha256,
         adapter_sha256: config.adapter_sha256.clone(),
         calibration_sha256: request.calibration_sha256.clone(),
         process_group_active: true,
         retain_response: false,
      })
   }

   pub(crate) fn wait_ready(&mut self, timeout: Duration) -> Result<MacOsEnergyAdapterReady>
   {
      let deadline = Instant::now().checked_add(timeout).context("external-meter ready timeout overflow")?;
      loop
      {
         self.enforce_output_limit()?;
         if self.ready_path.is_file()
         {
            let ready_bytes = fs::read(&self.ready_path).with_context(|| format!("reading energy adapter ready receipt {}", self.ready_path.display()))?;
            let ready: MacOsEnergyAdapterReady = serde_json::from_slice(&ready_bytes).with_context(|| format!("decoding energy adapter ready receipt {}", self.ready_path.display()))?;
            if ready.schema_version != 1
               || ready.protocol != "oxide-direct-external-meter-v1"
               || ready.request_sha256 != self.request_sha256
               || ready.adapter_sha256 != self.adapter_sha256
               || ready.calibration_sha256 != self.calibration_sha256
               || ready.ready_ticks == 0
               || !ready.hardware_ready
               || !ready.complete
            {
               bail!("external-meter ready receipt differs from its request or hardware is unavailable");
            }
            if self.child.try_wait().context("checking external-meter process at readiness")?.is_some()
            {
               bail!("external-meter adapter exited at its ready boundary");
            }
            return Ok(ready);
         }
         if let Some(status) = self.child.try_wait().context("checking external-meter process before readiness")?
         {
            bail!("external-meter adapter exited before readiness with {}", status);
         }
         if Instant::now() >= deadline
         {
            bail!("external-meter adapter did not become ready within {} seconds", timeout.as_secs());
         }
         thread::sleep(Duration::from_millis(25));
      }
   }

   pub(crate) fn enforce_output_limit(&mut self) -> Result<()>
   {
      let bytes = if self.response_path.exists() {fs::metadata(&self.response_path).with_context(|| format!("reading energy adapter response metadata {}", self.response_path.display()))?.len()} else {0};
      if bytes > ENERGY_ADAPTER_OUTPUT_LIMIT_BYTES
      {
         self.terminate_group();
         bail!("external-meter raw response exceeded the 64 MiB limit");
      }
      Ok(())
   }

   pub(crate) fn finish(mut self, raw_path: &Path, calibration: &MacOsEnergyCalibration, timeout: Duration) -> Result<MacOsExternalMeterRawArtifact>
   {
      let deadline = Instant::now().checked_add(timeout).context("external-meter finish timeout overflow")?;
      let status = loop
      {
         self.enforce_output_limit()?;
         if let Some(status) = self.child.try_wait().context("polling external-meter adapter completion")?
         {
            break status;
         }
         if Instant::now() >= deadline
         {
            self.terminate_group();
            bail!("external-meter adapter did not finish within {} seconds", timeout.as_secs());
         }
         thread::sleep(Duration::from_millis(25));
      };
      if !status.success()
      {
         bail!("external-meter adapter exited with {}", status);
      }
      self.terminate_group();
      let bytes = fs::read(&self.response_path).with_context(|| format!("reading external-meter raw response {}", self.response_path.display()))?;
      if bytes.is_empty() || bytes.len() as u64 > ENERGY_ADAPTER_OUTPUT_LIMIT_BYTES
      {
         bail!("external-meter raw response is empty or oversized");
      }
      let raw: MacOsExternalMeterRawArtifact = serde_json::from_slice(&bytes).with_context(|| format!("decoding external-meter raw response {}", self.response_path.display()))?;
      if raw.adapter_sha256 != self.adapter_sha256 || raw.calibration_sha256 != self.calibration_sha256 || raw.calibration_sha256 != macos_energy_calibration_sha256(calibration)?
      {
         bail!("external-meter raw response differs from its adapter or calibration identity");
      }
      persist_json(&raw, raw_path)?;
      fs::remove_file(&self.response_path).with_context(|| format!("removing external-meter partial response {}", self.response_path.display()))?;
      self.retain_response = true;
      Ok(raw)
   }

   fn terminate_group(&mut self)
   {
      if !self.process_group_active
      {
         return;
      }
      let group = format!("-{}", self.process_group);
      let _ = Command::new("/bin/kill").args(["-TERM", "--", &group]).status();
      thread::sleep(Duration::from_millis(25));
      let _ = Command::new("/bin/kill").args(["-KILL", "--", &group]).status();
      let _ = self.child.wait();
      self.process_group_active = false;
   }
}

impl Drop for MacOsEnergyAdapterProcess
{
   fn drop(&mut self)
   {
      self.terminate_group();
      if !self.retain_response && self.response_path.exists()
      {
         let _ = fs::remove_file(&self.response_path);
      }
   }
}

pub fn validate_macos_external_meter_config(config: &MacOsExternalMeterConfig) -> Result<()>
{
   if config.schema_version != 1 || config.adapter_kind != "direct-external-meter"
   {
      bail!("macOS energy config is not the direct external-meter v1 contract");
   }
   if !config.adapter_path.is_absolute() || !config.adapter_path.is_file()
   {
      bail!("macOS energy adapter must be an existing absolute file");
   }
   if fs::metadata(&config.adapter_path).with_context(|| format!("reading energy adapter metadata {}", config.adapter_path.display()))?.permissions().mode() & 0o111 == 0
   {
      bail!("macOS energy adapter is not executable");
   }
   validate_sha256(&config.adapter_sha256)?;
   let adapter_bytes = fs::read(&config.adapter_path).with_context(|| format!("reading energy adapter {}", config.adapter_path.display()))?;
   if format!("{:x}", Sha256::digest(&adapter_bytes)) != config.adapter_sha256
   {
      bail!("macOS energy adapter bytes differ from the configured SHA-256");
   }
   validate_calibration(&config.calibration)
}

pub fn macos_energy_calibration_sha256(calibration: &MacOsEnergyCalibration) -> Result<String>
{
   validate_calibration(calibration)?;
   let bytes = serde_json::to_vec(calibration).context("encoding external-meter calibration")?;
   Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub fn reduce_macos_energy(raw: &MacOsExternalMeterRawArtifact, calibration: &MacOsEnergyCalibration, telemetry: &[u8]) -> Result<MacOsEnergySummary>
{
   validate_calibration(calibration)?;
   validate_sha256(&raw.adapter_sha256)?;
   validate_sha256(&raw.calibration_sha256)?;
   if raw.schema_version != 1 || raw.pid == 0 || raw.timebase_numerator == 0 || raw.timebase_denominator == 0 || !raw.complete
   {
      bail!("external-meter raw artifact identity or completion is invalid");
   }
   if raw.calibration_sha256 != macos_energy_calibration_sha256(calibration)?
   {
      bail!("external-meter raw artifact calibration identity differs");
   }
   validate_samples(raw, calibration)?;
   let (header, boundaries) = parse_energy_telemetry(telemetry)?;
   if header.timebase_numerator != raw.timebase_numerator || header.timebase_denominator != raw.timebase_denominator
   {
      bail!("external-meter and comparator telemetry timebases differ");
   }
   let stabilization_identifier = stable_id("prewarm");
   let mut stabilization = Vec::new();
   let mut measured = Vec::new();
   let mut open = BTreeMap::<(bool, u64), u64>::new();
   for boundary in boundaries
   {
      let key = (boundary.measured, boundary.identifier);
      if boundary.begin
      {
         if open.insert(key, boundary.ticks).is_some()
         {
            bail!("energy telemetry has duplicate phase begin for identifier {}", boundary.identifier);
         }
         continue;
      }
      let begin_ticks = open.remove(&key).with_context(|| format!("energy telemetry phase end has no begin for identifier {}", boundary.identifier))?;
      if boundary.ticks <= begin_ticks
      {
         bail!("energy telemetry phase interval is non-positive");
      }
      let interval = PhaseInterval {identifier: boundary.identifier, begin_ticks, end_ticks: boundary.ticks};
      if boundary.measured
      {
         measured.push(interval);
      }
      else if boundary.identifier == stabilization_identifier
      {
         stabilization.push(interval);
      }
   }
   if !open.is_empty() || stabilization.len() != 1 || measured.is_empty()
   {
      bail!("energy telemetry does not contain one stabilization interval and complete measured phases");
   }
   measured.sort_by_key(|phase| phase.begin_ticks);
   for pair in measured.windows(2)
   {
      if pair[0].end_ticks > pair[1].begin_ticks
      {
         bail!("energy measured phase intervals overlap");
      }
   }
   let stabilization = stabilization[0];
   let measured_begin_ticks = measured.first().context("energy measured phases disappeared")?.begin_ticks;
   let measured_end_ticks = measured.last().context("energy measured phases disappeared")?.end_ticks;
   validate_exact_window(stabilization.begin_ticks, stabilization.end_ticks, MACOS_ENERGY_STABILIZATION_SECONDS, header)?;
   validate_exact_window(measured_begin_ticks, measured_end_ticks, MACOS_ENERGY_MEASUREMENT_SECONDS, header)?;
   validate_boundary_adjacency(stabilization.end_ticks, measured_begin_ticks, header)?;
   for pair in measured.windows(2)
   {
      validate_boundary_adjacency(pair[0].end_ticks, pair[1].begin_ticks, header)?;
   }
   if raw.capture_start_ticks > stabilization.begin_ticks || raw.capture_end_ticks < measured_end_ticks
   {
      bail!("external-meter samples do not cover stabilization through measured completion");
   }
   let mut phases = Vec::with_capacity(measured.len());
   let mut total_joules = 0.0;
   let mut total_adjusted_joules = 0.0;
   for phase in measured
   {
      let duration_seconds = ticks_to_seconds(phase.end_ticks - phase.begin_ticks, header)?;
      let joules = integrate_samples(&raw.samples, phase.begin_ticks, phase.end_ticks, header)?;
      let baseline_adjusted_joules = (joules - calibration.baseline_watts * duration_seconds).max(0.0);
      total_joules += joules;
      total_adjusted_joules += baseline_adjusted_joules;
      phases.push(MacOsEnergyPhaseSummary {
         phase_identifier: phase.identifier,
         begin_ticks: phase.begin_ticks,
         end_ticks: phase.end_ticks,
         duration_seconds,
         joules,
         average_watts: joules / duration_seconds,
         baseline_adjusted_joules,
         baseline_adjusted_average_watts: baseline_adjusted_joules / duration_seconds,
      });
   }
   let measured_seconds = ticks_to_seconds(measured_end_ticks - measured_begin_ticks, header)?;
   Ok(MacOsEnergySummary {
      schema_version: 1,
      availability: String::from(MACOS_ENERGY_AVAILABILITY),
      calibration_sha256: raw.calibration_sha256.clone(),
      calibration: calibration.clone(),
      stabilization_begin_ticks: stabilization.begin_ticks,
      stabilization_end_ticks: stabilization.end_ticks,
      stabilization_seconds: ticks_to_seconds(stabilization.end_ticks - stabilization.begin_ticks, header)?,
      measured_begin_ticks,
      measured_end_ticks,
      measured_seconds,
      joules: total_joules,
      average_watts: total_joules / measured_seconds,
      baseline_adjusted_joules: total_adjusted_joules,
      baseline_adjusted_average_watts: total_adjusted_joules / measured_seconds,
      integration_uncertainty_joules: calibration.integration_uncertainty_joules,
      phases,
   })
}

fn validate_calibration(calibration: &MacOsEnergyCalibration) -> Result<()>
{
   for (label, value) in [
      ("sampling rate", calibration.sampling_rate_hz),
      ("integration uncertainty", calibration.integration_uncertainty_joules),
      ("baseline", calibration.baseline_watts),
      ("display luminance", calibration.display_luminance_nits),
      ("battery minimum", calibration.battery_charge_min_percent),
      ("battery maximum", calibration.battery_charge_max_percent),
      ("room temperature", calibration.room_temperature_celsius),
   ]
   {
      if !value.is_finite()
      {
         bail!("external-meter calibration {} is not finite", label);
      }
   }
   if calibration.calibration_id.is_empty()
      || calibration.calibrated_at_utc.is_empty()
      || calibration.meter_model.is_empty()
      || calibration.meter_serial.is_empty()
   {
      bail!("external-meter calibration metadata is incomplete");
   }
   if calibration.sampling_rate_hz < 1.0
      || calibration.integration_uncertainty_joules < 0.0
      || calibration.baseline_watts < 0.0
      || calibration.display_luminance_nits <= 0.0
      || !(-10.0..=50.0).contains(&calibration.room_temperature_celsius)
      || !(0.0..=100.0).contains(&calibration.battery_charge_min_percent)
      || !(0.0..=100.0).contains(&calibration.battery_charge_max_percent)
      || calibration.battery_charge_min_percent > calibration.battery_charge_max_percent
   {
      bail!("external-meter calibration numeric bounds are invalid");
   }
   let topology_includes_display = matches!(calibration.power_topology, MacOsEnergyPowerTopology::AcMainsCompleteSystem | MacOsEnergyPowerTopology::DcInlineCompleteSystem);
   if calibration.display_included != topology_includes_display
   {
      bail!("external-meter display inclusion disagrees with its power topology");
   }
   if calibration.battery_state == MacOsEnergyBatteryState::Absent
      && (calibration.battery_charge_min_percent != 0.0 || calibration.battery_charge_max_percent != 0.0)
   {
      bail!("external-meter calibration gives a charge range for an absent battery");
   }
   Ok(())
}

fn validate_samples(raw: &MacOsExternalMeterRawArtifact, calibration: &MacOsEnergyCalibration) -> Result<()>
{
   if raw.capture_end_ticks <= raw.capture_start_ticks || raw.samples.len() < 2
   {
      bail!("external-meter raw sample stream is empty or reversed");
   }
   let max_gap_seconds = 2.0 / calibration.sampling_rate_hz;
   for sample in &raw.samples
   {
      if !sample.watts.is_finite() || sample.watts < 0.0
      {
         bail!("external-meter sample power is invalid");
      }
      if sample.host_ticks < raw.capture_start_ticks || sample.host_ticks > raw.capture_end_ticks
      {
         bail!("external-meter sample lies outside its capture interval");
      }
   }
   for pair in raw.samples.windows(2)
   {
      if pair[1].host_ticks <= pair[0].host_ticks
      {
         bail!("external-meter sample timestamps are not strictly increasing");
      }
      let header = TelemetryHeader {timebase_numerator: raw.timebase_numerator, timebase_denominator: raw.timebase_denominator};
      if ticks_to_seconds(pair[1].host_ticks - pair[0].host_ticks, header)? > max_gap_seconds
      {
         bail!("external-meter sample stream has a gap larger than two configured periods");
      }
   }
   Ok(())
}

fn validate_exact_window(begin_ticks: u64, end_ticks: u64, expected_seconds: u64, header: TelemetryHeader) -> Result<()>
{
   let observed_ns = ticks_to_nanoseconds(end_ticks.checked_sub(begin_ticks).context("energy window ticks are reversed")?, header)?;
   let expected_ns = expected_seconds.checked_mul(1_000_000_000).context("energy window nanoseconds overflow")?;
   if observed_ns.abs_diff(expected_ns) > BOUNDARY_TOLERANCE_NS
   {
      bail!("energy window is not the required {} seconds", expected_seconds);
   }
   Ok(())
}

fn validate_boundary_adjacency(first: u64, second: u64, header: TelemetryHeader) -> Result<()>
{
   let delta = if second >= first {second - first} else {first - second};
   if ticks_to_nanoseconds(delta, header)? > BOUNDARY_TOLERANCE_NS
   {
      bail!("energy stabilization or measured phase boundaries are not contiguous");
   }
   Ok(())
}

fn integrate_samples(samples: &[MacOsExternalMeterSample], begin_ticks: u64, end_ticks: u64, header: TelemetryHeader) -> Result<f64>
{
   let begin_watts = interpolated_watts(samples, begin_ticks)?;
   let end_watts = interpolated_watts(samples, end_ticks)?;
   let mut points = Vec::new();
   points.push((begin_ticks, begin_watts));
   points.extend(samples.iter().filter(|sample| sample.host_ticks > begin_ticks && sample.host_ticks < end_ticks).map(|sample| (sample.host_ticks, sample.watts)));
   points.push((end_ticks, end_watts));
   let mut joules = 0.0;
   for pair in points.windows(2)
   {
      let seconds = ticks_to_seconds(pair[1].0 - pair[0].0, header)?;
      joules += (pair[0].1 + pair[1].1) * 0.5 * seconds;
   }
   if !joules.is_finite() || joules < 0.0
   {
      bail!("external-meter energy integration is invalid");
   }
   Ok(joules)
}

fn interpolated_watts(samples: &[MacOsExternalMeterSample], ticks: u64) -> Result<f64>
{
   if let Some(sample) = samples.iter().find(|sample| sample.host_ticks == ticks)
   {
      return Ok(sample.watts);
   }
   let index = samples.partition_point(|sample| sample.host_ticks < ticks);
   if index == 0 || index == samples.len()
   {
      bail!("external-meter samples do not bracket an energy boundary");
   }
   let before = &samples[index - 1];
   let after = &samples[index];
   let position = (ticks - before.host_ticks) as f64 / (after.host_ticks - before.host_ticks) as f64;
   Ok(before.watts + (after.watts - before.watts) * position)
}

fn parse_energy_telemetry(bytes: &[u8]) -> Result<(TelemetryHeader, Vec<PhaseBoundary>)>
{
   if bytes.len() < TELEMETRY_HEADER_BYTES + TELEMETRY_FOOTER_BYTES || &bytes[..8] != b"OXBTEL02"
   {
      bail!("energy telemetry header is missing or truncated");
   }
   let schema = read_u32(bytes, 8)?;
   let header_bytes = read_u32(bytes, 12)? as usize;
   let record_bytes = read_u32(bytes, 16)? as usize;
   let flags = read_u32(bytes, 20)?;
   let count = read_u64(bytes, 24)?;
   let capacity = read_u64(bytes, 32)?;
   if schema != 2 || header_bytes != TELEMETRY_HEADER_BYTES || record_bytes != TELEMETRY_RECORD_BYTES || flags != 1 || count > capacity
   {
      bail!("energy telemetry header contract is invalid");
   }
   let count = usize::try_from(count).context("energy telemetry record count exceeds usize")?;
   let expected_len = TELEMETRY_HEADER_BYTES.checked_add(count.checked_mul(TELEMETRY_RECORD_BYTES).context("energy telemetry record bytes overflow")?).and_then(|value| value.checked_add(TELEMETRY_FOOTER_BYTES)).context("energy telemetry length overflow")?;
   if bytes.len() != expected_len || Sha256::digest(&bytes[..bytes.len() - TELEMETRY_FOOTER_BYTES])[..] != bytes[bytes.len() - TELEMETRY_FOOTER_BYTES..]
   {
      bail!("energy telemetry length or integrity footer differs");
   }
   let header = TelemetryHeader {
      timebase_numerator: read_u32(bytes, 128)?,
      timebase_denominator: read_u32(bytes, 132)?,
   };
   if header.timebase_numerator == 0 || header.timebase_denominator == 0
   {
      bail!("energy telemetry timebase is zero");
   }
   let mut boundaries = Vec::new();
   let mut previous_ticks = 0_u64;
   for index in 0..count
   {
      let offset = TELEMETRY_HEADER_BYTES + index * TELEMETRY_RECORD_BYTES;
      if read_u64(bytes, offset)? != index as u64
      {
         bail!("energy telemetry sequence is not contiguous");
      }
      let ticks = read_u64(bytes, offset + 8)?;
      if index > 0 && ticks < previous_ticks
      {
         bail!("energy telemetry timestamp regressed");
      }
      previous_ticks = ticks;
      let kind = read_u16(bytes, offset + 16)?;
      if matches!(kind, TELEMETRY_PHASE_BEGIN | TELEMETRY_PHASE_END)
      {
         let flags = read_u16(bytes, offset + 18)?;
         if flags & !TELEMETRY_MEASURED_FLAG != 0
         {
            bail!("energy telemetry phase flags are unsupported");
         }
         boundaries.push(PhaseBoundary {
            begin: kind == TELEMETRY_PHASE_BEGIN,
            measured: flags & TELEMETRY_MEASURED_FLAG != 0,
            identifier: read_u64(bytes, offset + 20)?,
            ticks,
         });
      }
   }
   Ok((header, boundaries))
}

fn ticks_to_nanoseconds(ticks: u64, header: TelemetryHeader) -> Result<u64>
{
   let value = u128::from(ticks).checked_mul(u128::from(header.timebase_numerator)).context("energy tick conversion overflow")? / u128::from(header.timebase_denominator);
   u64::try_from(value).context("energy nanoseconds exceed u64")
}

fn ticks_to_seconds(ticks: u64, header: TelemetryHeader) -> Result<f64>
{
   Ok(ticks_to_nanoseconds(ticks, header)? as f64 / 1_000_000_000.0)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16>
{
   let slice = bytes.get(offset..offset + 2).context("truncated energy telemetry u16")?;
   Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32>
{
   let slice = bytes.get(offset..offset + 4).context("truncated energy telemetry u32")?;
   Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64>
{
   let slice = bytes.get(offset..offset + 8).context("truncated energy telemetry u64")?;
   Ok(u64::from_le_bytes([slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7]]))
}

fn stable_id(value: &str) -> u64
{
   value.as_bytes().iter().fold(0xcbf29ce484222325, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3))
}

fn validate_sha256(value: &str) -> Result<()>
{
   if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
   {
      bail!("energy SHA-256 is not canonical lowercase hexadecimal");
   }
   Ok(())
}

fn persist_json<T: Serialize>(value: &T, destination: &Path) -> Result<()>
{
   if destination.exists()
   {
      bail!("refusing to overwrite durable energy artifact {}", destination.display());
   }
   let bytes = serde_json::to_vec_pretty(value).context("encoding durable energy JSON")?;
   let directory = destination.parent().context("durable energy artifact has no parent")?;
   fs::create_dir_all(directory).with_context(|| format!("creating {}", directory.display()))?;
   let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).context("system clock precedes Unix epoch")?.as_nanos();
   let file_name = destination.file_name().and_then(|value| value.to_str()).context("durable energy artifact has no UTF-8 file name")?;
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
