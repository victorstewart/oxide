use anyhow::{bail, Context, Result};
use oxide_benchmark_spec::AppleCampaignPassRole;
use serde::{Deserialize, Serialize};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::{mach_continuous_time, validate_sha256, ComparisonSide, MacOsCampaignPlan, MacOsCampaignSession};

pub(crate) const MACOS_RESOURCE_CADENCE_NS: u64 = 50_000_000;
const MACOS_RESOURCE_CADENCE: Duration = Duration::from_nanos(MACOS_RESOURCE_CADENCE_NS);
pub(crate) const MACOS_LOW_FREQUENCY_RESOURCE_CADENCE_NS: u64 = 1_000_000_000;
const RESOURCE_AVAILABLE: &str = "available-proc-pid-rusage-v4";
const RESOURCE_NOT_APPLICABLE: &str = "not-applicable-isolated-correctness-or-energy-pass";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MacOsResourceSample
{
   pub mach_continuous_time: u64,
   pub user_cpu_ns: u64,
   pub system_cpu_ns: u64,
   #[serde(default)]
   pub package_idle_wakeups: u64,
   #[serde(default)]
   pub interrupt_wakeups: u64,
   #[serde(default)]
   pub pageins: u64,
   #[serde(default)]
   pub wired_bytes: u64,
   pub resident_bytes: u64,
   pub physical_footprint_bytes: u64,
   pub lifetime_max_physical_footprint_bytes: u64,
   pub interval_max_physical_footprint_bytes: u64,
   pub disk_read_bytes: u64,
   pub disk_written_bytes: u64,
   pub instructions: u64,
   pub cycles: u64,
   pub billed_system_time_ns: u64,
   pub serviced_system_time_ns: u64,
   pub billed_energy: u64,
   pub serviced_energy: u64,
   #[serde(default)]
   pub logical_writes: u64,
   #[serde(default)]
   pub runnable_time_ns: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MacOsResourceArtifact
{
   pub schema_version: u32,
   #[serde(rename = "runID")]
   pub run_id: String,
   #[serde(rename = "planSHA256")]
   pub plan_sha256: String,
   #[serde(rename = "chunkID")]
   pub chunk_id: String,
   #[serde(rename = "passID")]
   pub pass_id: String,
   pub pair_index: u32,
   pub side: ComparisonSide,
   pub generation: String,
   #[serde(rename = "executableSHA256")]
   pub executable_sha256: String,
   #[serde(rename = "packID")]
   pub pack_id: String,
   pub pid: u32,
   pub process_uuid: String,
   pub process_start_abstime: u64,
   #[serde(default)]
   pub process_start_continuous_time: u64,
   #[serde(default)]
   pub process_start_clock_uncertainty_ticks: u64,
   #[serde(default)]
   pub timebase_numerator: u32,
   #[serde(default)]
   pub timebase_denominator: u32,
   pub cadence_ns: u64,
   pub launch_t0: u64,
   pub durable_complete_timestamp: u64,
   pub availability: String,
   pub samples: Vec<MacOsResourceSample>,
   pub complete: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsResourceSummary
{
   pub schema_version: u32,
   pub source_schema_version: u32,
   pub window_start_ticks: u64,
   pub window_end_ticks: u64,
   pub wall_time_ns: u64,
   pub sample_count: u64,
   pub user_cpu_ns: u64,
   pub system_cpu_ns: u64,
   pub process_cpu_ms_per_wall_s: f64,
   pub runnable_time_ns: u64,
   pub wakeups: u64,
   pub wakeups_per_wall_s: f64,
   pub pageins: u64,
   pub disk_read_bytes: u64,
   pub disk_written_bytes: u64,
   pub logical_writes: u64,
   pub instructions: u64,
   pub cycles: u64,
   pub billed_system_time_ns: u64,
   pub serviced_system_time_ns: u64,
   pub billed_energy: u64,
   pub serviced_energy: u64,
   pub wired_start_bytes: u64,
   pub wired_end_bytes: u64,
   pub wired_peak_bytes: u64,
   pub resident_start_bytes: u64,
   pub resident_end_bytes: u64,
   pub resident_peak_bytes: u64,
   pub physical_footprint_start_bytes: u64,
   pub physical_footprint_end_bytes: u64,
   pub physical_footprint_peak_bytes: u64,
   pub retained_slope_bytes_per_min: f64,
}

pub(crate) struct MacOsResourceIdentity<'a>
{
   pub plan: &'a MacOsCampaignPlan,
   pub session: &'a MacOsCampaignSession,
   pub generation: &'a str,
   pub executable_sha256: &'a str,
   pub pid: u32,
   pub launch_t0: u64,
}

enum CollectorCommand
{
   Finish,
   Abort,
}

pub(crate) struct MacOsResourceCollector
{
   command: Sender<CollectorCommand>,
   thread: Option<JoinHandle<Result<CollectedSamples>>>,
}

struct CollectedSamples
{
   process_uuid: String,
   process_start_abstime: u64,
   process_start_continuous_time: u64,
   process_start_clock_uncertainty_ticks: u64,
   timebase_numerator: u32,
   timebase_denominator: u32,
   cadence_ns: u64,
   samples: Vec<MacOsResourceSample>,
}

impl MacOsResourceArtifact
{
   pub(crate) fn not_applicable(identity: &MacOsResourceIdentity<'_>, durable_complete_timestamp: u64) -> Self
   {
      Self {
         schema_version: 4,
         run_id: identity.plan.run_id.clone(),
         plan_sha256: identity.plan.plan_sha256.clone(),
         chunk_id: identity.session.chunk_id.clone(),
         pass_id: identity.session.pass_id.clone(),
         pair_index: identity.session.pair_index,
         side: identity.session.side,
         generation: String::from(identity.generation),
         executable_sha256: String::from(identity.executable_sha256),
         pack_id: identity.session.pack_id.clone(),
         pid: identity.pid,
         process_uuid: String::new(),
         process_start_abstime: 0,
         process_start_continuous_time: 0,
         process_start_clock_uncertainty_ticks: 0,
         timebase_numerator: 0,
         timebase_denominator: 0,
         cadence_ns: 0,
         launch_t0: identity.launch_t0,
         durable_complete_timestamp,
         availability: String::from(RESOURCE_NOT_APPLICABLE),
         samples: Vec::new(),
         complete: true,
      }
   }

   fn measured(identity: &MacOsResourceIdentity<'_>, durable_complete_timestamp: u64, collected: CollectedSamples) -> Self
   {
      Self {
         schema_version: 4,
         run_id: identity.plan.run_id.clone(),
         plan_sha256: identity.plan.plan_sha256.clone(),
         chunk_id: identity.session.chunk_id.clone(),
         pass_id: identity.session.pass_id.clone(),
         pair_index: identity.session.pair_index,
         side: identity.session.side,
         generation: String::from(identity.generation),
         executable_sha256: String::from(identity.executable_sha256),
         pack_id: identity.session.pack_id.clone(),
         pid: identity.pid,
         process_uuid: collected.process_uuid,
         process_start_abstime: collected.process_start_abstime,
         process_start_continuous_time: collected.process_start_continuous_time,
         process_start_clock_uncertainty_ticks: collected.process_start_clock_uncertainty_ticks,
         timebase_numerator: collected.timebase_numerator,
         timebase_denominator: collected.timebase_denominator,
         cadence_ns: collected.cadence_ns,
         launch_t0: identity.launch_t0,
         durable_complete_timestamp,
         availability: String::from(RESOURCE_AVAILABLE),
         samples: collected.samples,
         complete: true,
      }
   }
}

impl MacOsResourceCollector
{
   pub(crate) fn start(pid: u32, pass_id: &str) -> Result<Self>
   {
      let initial = process_resource_snapshot(pid)?;
      let cadence = resource_cadence(pass_id);
      let (timebase_numerator, timebase_denominator) = resource_timebase()?;
      let (command, commands) = mpsc::channel();
      let thread = thread::Builder::new().name(String::from("macos-resource-sampler")).spawn(move || collect_process_resources(pid, initial, cadence, timebase_numerator, timebase_denominator, commands)).context("starting macOS process resource collector")?;
      Ok(Self {command, thread: Some(thread)})
   }

   pub(crate) fn finish(mut self, identity: &MacOsResourceIdentity<'_>, durable_complete_timestamp: u64) -> Result<MacOsResourceArtifact>
   {
      self.command.send(CollectorCommand::Finish).context("stopping macOS process resource collector")?;
      let collected = self.join()?;
      let artifact = MacOsResourceArtifact::measured(identity, durable_complete_timestamp, collected);
      validate_resource_artifact(&artifact, identity)?;
      Ok(artifact)
   }

   fn join(&mut self) -> Result<CollectedSamples>
   {
      let thread = self.thread.take().context("macOS process resource collector was already joined")?;
      thread.join().map_err(|_| anyhow::anyhow!("macOS process resource collector panicked"))?
   }
}

impl Drop for MacOsResourceCollector
{
   fn drop(&mut self)
   {
      if self.thread.is_none()
      {
         return;
      }
      let _ = self.command.send(CollectorCommand::Abort);
      let _ = self.join();
   }
}

pub(crate) fn validate_resource_artifact(artifact: &MacOsResourceArtifact, identity: &MacOsResourceIdentity<'_>) -> Result<()>
{
   validate_sha256(&artifact.generation)?;
   validate_sha256(&artifact.executable_sha256)?;
   let schema_supported = if identity.session.pass_id == "correctness"
   {
      matches!(artifact.schema_version, 1 | 2 | 3 | 4)
   }
   else
   {
      artifact.schema_version == 4
   };
   if !schema_supported
      || artifact.run_id != identity.plan.run_id
      || artifact.plan_sha256 != identity.plan.plan_sha256
      || artifact.chunk_id != identity.session.chunk_id
      || artifact.pass_id != identity.session.pass_id
      || artifact.pair_index != identity.session.pair_index
      || artifact.side != identity.session.side
      || artifact.generation != identity.generation
      || artifact.executable_sha256 != identity.executable_sha256
      || artifact.pack_id != identity.session.pack_id
      || artifact.pid != identity.pid
      || artifact.pid == 0
      || artifact.launch_t0 != identity.launch_t0
      || artifact.durable_complete_timestamp < artifact.launch_t0
      || !artifact.complete
   {
      bail!("macOS process resource artifact identity differs from its run plan, process, or build");
   }
   if identity.session.pass_id == "correctness" || identity.session.pass_role == AppleCampaignPassRole::Energy
   {
      if artifact.availability != RESOURCE_NOT_APPLICABLE
         || artifact.cadence_ns != 0
         || artifact.process_start_abstime != 0
         || artifact.process_start_continuous_time != 0
         || artifact.process_start_clock_uncertainty_ticks != 0
         || artifact.timebase_numerator != 0
         || artifact.timebase_denominator != 0
         || !artifact.process_uuid.is_empty()
         || !artifact.samples.is_empty()
      {
         bail!("isolated correctness or energy resource artifact must be explicitly not applicable");
      }
      return Ok(());
   }
   if artifact.availability != RESOURCE_AVAILABLE
      || artifact.cadence_ns != resource_cadence(identity.session.pass_id.as_str()).as_nanos() as u64
      || artifact.process_start_abstime == 0
      || artifact.process_start_continuous_time == 0
      || artifact.process_start_clock_uncertainty_ticks == 0
      || artifact.timebase_numerator == 0
      || artifact.timebase_denominator == 0
      || artifact.process_uuid.len() != 32
      || !artifact.process_uuid.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
      || artifact.samples.len() < 2
   {
      bail!("measured macOS process resource artifact is unavailable or incomplete");
   }
   let first = artifact.samples.first().context("macOS process resource artifact has no first sample")?;
   let last = artifact.samples.last().context("macOS process resource artifact has no last sample")?;
   if artifact.process_start_continuous_time < artifact.launch_t0
      || artifact.process_start_continuous_time > first.mach_continuous_time.saturating_add(artifact.process_start_clock_uncertainty_ticks)
      || first.mach_continuous_time < artifact.launch_t0
      || last.mach_continuous_time < artifact.durable_complete_timestamp
   {
      bail!("macOS process resource samples do not cover PID discovery through durable completion");
   }
   for pair in artifact.samples.windows(2)
   {
      validate_monotonic_sample(&pair[0], &pair[1])?;
   }
   Ok(())
}

pub fn reduce_macos_resource_artifact(bytes: &[u8], window_start_ticks: Option<u64>, window_end_ticks: Option<u64>) -> Result<MacOsResourceSummary>
{
   let artifact: MacOsResourceArtifact = serde_json::from_slice(bytes).context("decoding macOS resource artifact")?;
   if artifact.schema_version != 4
      || artifact.availability != RESOURCE_AVAILABLE
      || !artifact.complete
      || artifact.timebase_numerator == 0
      || artifact.timebase_denominator == 0
      || artifact.samples.len() < 2
   {
      bail!("macOS resource artifact is not a complete measured schema-v4 stream");
   }
   for pair in artifact.samples.windows(2)
   {
      validate_monotonic_sample(&pair[0], &pair[1])?;
   }
   let default_start = artifact.samples.first().context("macOS resource artifact has no first sample")?.mach_continuous_time;
   let default_end = artifact.samples.last().context("macOS resource artifact has no last sample")?.mach_continuous_time;
   let start = window_start_ticks.unwrap_or(default_start);
   let end = window_end_ticks.unwrap_or(default_end);
   if start >= end
   {
      bail!("macOS resource reduction window is empty or reversed");
   }
   let samples = artifact.samples.iter().filter(|sample| sample.mach_continuous_time >= start && sample.mach_continuous_time <= end).collect::<Vec<_>>();
   if samples.len() < 2
   {
      bail!("macOS resource reduction window contains fewer than two samples");
   }
   let first = samples[0];
   let last = samples[samples.len() - 1];
   let wall_time_ns = resource_ticks_to_nanoseconds(last.mach_continuous_time - first.mach_continuous_time, artifact.timebase_numerator, artifact.timebase_denominator)?;
   if wall_time_ns == 0
   {
      bail!("macOS resource reduction window has zero converted duration");
   }
   let user_cpu_ns = last.user_cpu_ns - first.user_cpu_ns;
   let system_cpu_ns = last.system_cpu_ns - first.system_cpu_ns;
   let process_cpu_ns = user_cpu_ns.checked_add(system_cpu_ns).context("macOS process CPU delta overflow")?;
   let wakeups = (last.package_idle_wakeups - first.package_idle_wakeups).checked_add(last.interrupt_wakeups - first.interrupt_wakeups).context("macOS wakeup delta overflow")?;
   let wall_seconds = wall_time_ns as f64 / 1_000_000_000.0;
   let wired_peak_bytes = samples.iter().map(|sample| sample.wired_bytes).max().unwrap_or(0);
   let resident_peak_bytes = samples.iter().map(|sample| sample.resident_bytes).max().unwrap_or(0);
   let physical_footprint_peak_bytes = samples.iter().map(|sample| sample.physical_footprint_bytes.max(sample.interval_max_physical_footprint_bytes)).max().unwrap_or(0);
   Ok(MacOsResourceSummary {
      schema_version: 1,
      source_schema_version: artifact.schema_version,
      window_start_ticks: first.mach_continuous_time,
      window_end_ticks: last.mach_continuous_time,
      wall_time_ns,
      sample_count: samples.len() as u64,
      user_cpu_ns,
      system_cpu_ns,
      process_cpu_ms_per_wall_s: process_cpu_ns as f64 * 1_000.0 / wall_time_ns as f64,
      runnable_time_ns: last.runnable_time_ns - first.runnable_time_ns,
      wakeups,
      wakeups_per_wall_s: wakeups as f64 / wall_seconds,
      pageins: last.pageins - first.pageins,
      disk_read_bytes: last.disk_read_bytes - first.disk_read_bytes,
      disk_written_bytes: last.disk_written_bytes - first.disk_written_bytes,
      logical_writes: last.logical_writes - first.logical_writes,
      instructions: last.instructions - first.instructions,
      cycles: last.cycles - first.cycles,
      billed_system_time_ns: last.billed_system_time_ns - first.billed_system_time_ns,
      serviced_system_time_ns: last.serviced_system_time_ns - first.serviced_system_time_ns,
      billed_energy: last.billed_energy - first.billed_energy,
      serviced_energy: last.serviced_energy - first.serviced_energy,
      wired_start_bytes: first.wired_bytes,
      wired_end_bytes: last.wired_bytes,
      wired_peak_bytes,
      resident_start_bytes: first.resident_bytes,
      resident_end_bytes: last.resident_bytes,
      resident_peak_bytes,
      physical_footprint_start_bytes: first.physical_footprint_bytes,
      physical_footprint_end_bytes: last.physical_footprint_bytes,
      physical_footprint_peak_bytes,
      retained_slope_bytes_per_min: theil_sen_footprint_slope_bytes_per_min(&samples, artifact.timebase_numerator, artifact.timebase_denominator)?,
   })
}

fn resource_ticks_to_nanoseconds(ticks: u64, numerator: u32, denominator: u32) -> Result<u64>
{
   let scaled = u128::from(ticks).checked_mul(u128::from(numerator)).context("macOS resource tick conversion overflow")? / u128::from(denominator);
   u64::try_from(scaled).context("macOS resource nanoseconds exceed u64")
}

fn theil_sen_footprint_slope_bytes_per_min(samples: &[&MacOsResourceSample], numerator: u32, denominator: u32) -> Result<f64>
{
   let pair_count = samples.len().checked_mul(samples.len().saturating_sub(1)).context("Theil-Sen pair count overflow")? / 2;
   let mut slopes = Vec::with_capacity(pair_count);
   for (index, first) in samples.iter().enumerate()
   {
      for second in &samples[index + 1..]
      {
         let elapsed_ns = resource_ticks_to_nanoseconds(second.mach_continuous_time - first.mach_continuous_time, numerator, denominator)?;
         if elapsed_ns == 0 {continue}
         slopes.push((second.physical_footprint_bytes as f64 - first.physical_footprint_bytes as f64) * 60_000_000_000.0 / elapsed_ns as f64);
      }
   }
   if slopes.is_empty()
   {
      bail!("macOS resource stream has no Theil-Sen slope pairs");
   }
   slopes.sort_by(|left, right| left.total_cmp(right));
   let middle = slopes.len() / 2;
   Ok(if slopes.len() % 2 == 0 {(slopes[middle - 1] + slopes[middle]) * 0.5} else {slopes[middle]})
}

fn validate_monotonic_sample(previous: &MacOsResourceSample, current: &MacOsResourceSample) -> Result<()>
{
   if current.mach_continuous_time <= previous.mach_continuous_time
      || current.user_cpu_ns < previous.user_cpu_ns
      || current.system_cpu_ns < previous.system_cpu_ns
      || current.lifetime_max_physical_footprint_bytes < previous.lifetime_max_physical_footprint_bytes
      || current.disk_read_bytes < previous.disk_read_bytes
      || current.disk_written_bytes < previous.disk_written_bytes
      || current.package_idle_wakeups < previous.package_idle_wakeups
      || current.interrupt_wakeups < previous.interrupt_wakeups
      || current.pageins < previous.pageins
      || current.instructions < previous.instructions
      || current.cycles < previous.cycles
      || current.billed_system_time_ns < previous.billed_system_time_ns
      || current.serviced_system_time_ns < previous.serviced_system_time_ns
      || current.billed_energy < previous.billed_energy
      || current.serviced_energy < previous.serviced_energy
      || current.logical_writes < previous.logical_writes
      || current.runnable_time_ns < previous.runnable_time_ns
   {
      bail!("macOS process resource samples are not monotonic");
   }
   Ok(())
}

fn resource_cadence(pass_id: &str) -> Duration
{
   match pass_id
   {
      "idle" | "endurance" | "energy" | "memory" | "attribution-physical-footprint" => Duration::from_nanos(MACOS_LOW_FREQUENCY_RESOURCE_CADENCE_NS),
      _ => MACOS_RESOURCE_CADENCE,
   }
}

fn collect_process_resources(pid: u32, initial: ResourceSnapshot, cadence: Duration, timebase_numerator: u32, timebase_denominator: u32, commands: Receiver<CollectorCommand>) -> Result<CollectedSamples>
{
   let process_uuid = uuid_hex(&initial.usage.uuid);
   let process_start_abstime = initial.usage.process_start_abstime;
   let process_start_continuous_time = initial.process_start_continuous_time;
   let process_start_clock_uncertainty_ticks = initial.process_start_clock_uncertainty_ticks;
   if process_start_abstime == 0
   {
      bail!("macOS process resource collector observed no process start identity");
   }
   let mut samples = Vec::with_capacity(512);
   samples.push(initial.sample);
   let mut deadline = Instant::now() + cadence;
   loop
   {
      match commands.recv_timeout(deadline.saturating_duration_since(Instant::now()))
      {
         Ok(CollectorCommand::Finish) =>
         {
            let snapshot = process_resource_snapshot(pid)?;
            validate_process_identity(&snapshot, &process_uuid, process_start_abstime)?;
            samples.push(snapshot.sample);
            break;
         }
         Ok(CollectorCommand::Abort) => break,
         Err(RecvTimeoutError::Disconnected) => break,
         Err(RecvTimeoutError::Timeout) =>
         {
            let snapshot = process_resource_snapshot(pid)?;
            validate_process_identity(&snapshot, &process_uuid, process_start_abstime)?;
            samples.push(snapshot.sample);
            let now = Instant::now();
            while deadline <= now
            {
               deadline += cadence;
            }
         }
      }
   }
   Ok(CollectedSamples {
      process_uuid,
      process_start_abstime,
      process_start_continuous_time,
      process_start_clock_uncertainty_ticks,
      timebase_numerator,
      timebase_denominator,
      cadence_ns: cadence.as_nanos() as u64,
      samples,
   })
}

#[cfg(target_os = "macos")]
fn resource_timebase() -> Result<(u32, u32)>
{
   #[repr(C)]
   struct MachTimebaseInfo
   {
      numerator: u32,
      denominator: u32,
   }
   unsafe extern "C"
   {
      fn mach_timebase_info(info: *mut MachTimebaseInfo) -> i32;
   }
   let mut info = MachTimebaseInfo {numerator: 0, denominator: 0};
   // SAFETY: `info` is a writable, correctly sized timebase-info buffer and
   // the system call does not retain its address.
   let status = unsafe {mach_timebase_info(&mut info)};
   if status != 0 || info.numerator == 0 || info.denominator == 0
   {
      bail!("mach_timebase_info failed with {}", status);
   }
   Ok((info.numerator, info.denominator))
}

#[cfg(not(target_os = "macos"))]
fn resource_timebase() -> Result<(u32, u32)>
{
   bail!("mach timebase is available only on macOS")
}

fn validate_process_identity(snapshot: &ResourceSnapshot, process_uuid: &str, process_start_abstime: u64) -> Result<()>
{
   if snapshot.usage.process_start_abstime != process_start_abstime || uuid_hex(&snapshot.usage.uuid) != process_uuid
   {
      bail!("macOS process identity changed while collecting resources");
   }
   Ok(())
}

fn uuid_hex(uuid: &[u8; 16]) -> String
{
   let mut value = String::with_capacity(32);
   for byte in uuid
   {
      use std::fmt::Write as _;
      let _ = write!(value, "{:02x}", byte);
   }
   value
}

struct ResourceSnapshot
{
   usage: ResourceUsageInfoV4,
   process_start_continuous_time: u64,
   process_start_clock_uncertainty_ticks: u64,
   sample: MacOsResourceSample,
}

#[cfg(target_os = "macos")]
fn process_resource_snapshot(pid: u32) -> Result<ResourceSnapshot>
{
   let pid = i32::try_from(pid).context("macOS process id exceeds signed process range")?;
   let mut usage = ResourceUsageInfoV4::default();
   let continuous_before = mach_continuous_time();
   let absolute_time = unsafe {mach_absolute_time()};
   let continuous_after = mach_continuous_time();
   // SAFETY: `usage` is a writable, correctly sized and aligned V4 buffer for
   // the duration of the call. `pid` names the exact live process discovered by
   // the controller; libproc does not retain the buffer.
   let result = unsafe {proc_pid_rusage(pid, 4, &mut usage)};
   if result != 0
   {
      return Err(std::io::Error::last_os_error()).context("reading RUSAGE_INFO_V4 for exact macOS comparison process");
   }
   let timestamp = mach_continuous_time();
   let continuous_midpoint = continuous_before.saturating_add(continuous_after.saturating_sub(continuous_before) / 2);
   let continuous_offset = continuous_midpoint.checked_sub(absolute_time).context("mach continuous clock precedes absolute clock")?;
   let process_start_continuous_time = usage.process_start_abstime.checked_add(continuous_offset).context("mapped process start time overflow")?;
   let process_start_clock_uncertainty_ticks = continuous_after.saturating_sub(continuous_before).saturating_add(2);
   let sample = MacOsResourceSample {
      mach_continuous_time: timestamp,
      user_cpu_ns: usage.user_time,
      system_cpu_ns: usage.system_time,
      package_idle_wakeups: usage.package_idle_wakeups,
      interrupt_wakeups: usage.interrupt_wakeups,
      pageins: usage.pageins,
      wired_bytes: usage.wired_size,
      resident_bytes: usage.resident_size,
      physical_footprint_bytes: usage.phys_footprint,
      lifetime_max_physical_footprint_bytes: usage.lifetime_max_phys_footprint,
      interval_max_physical_footprint_bytes: usage.interval_max_phys_footprint,
      disk_read_bytes: usage.diskio_bytes_read,
      disk_written_bytes: usage.diskio_bytes_written,
      instructions: usage.instructions,
      cycles: usage.cycles,
      billed_system_time_ns: usage.billed_system_time,
      serviced_system_time_ns: usage.serviced_system_time,
      billed_energy: usage.billed_energy,
      serviced_energy: usage.serviced_energy,
      logical_writes: usage.logical_writes,
      runnable_time_ns: usage.runnable_time,
   };
   Ok(ResourceSnapshot {
      usage,
      process_start_continuous_time,
      process_start_clock_uncertainty_ticks,
      sample,
   })
}

#[cfg(not(target_os = "macos"))]
fn process_resource_snapshot(_pid: u32) -> Result<ResourceSnapshot>
{
   bail!("RUSAGE_INFO_V4 process resources are available only on macOS")
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ResourceUsageInfoV4
{
   uuid: [u8; 16],
   user_time: u64,
   system_time: u64,
   package_idle_wakeups: u64,
   interrupt_wakeups: u64,
   pageins: u64,
   wired_size: u64,
   resident_size: u64,
   phys_footprint: u64,
   process_start_abstime: u64,
   process_exit_abstime: u64,
   child_user_time: u64,
   child_system_time: u64,
   child_package_idle_wakeups: u64,
   child_interrupt_wakeups: u64,
   child_pageins: u64,
   child_elapsed_abstime: u64,
   diskio_bytes_read: u64,
   diskio_bytes_written: u64,
   cpu_time_qos_default: u64,
   cpu_time_qos_maintenance: u64,
   cpu_time_qos_background: u64,
   cpu_time_qos_utility: u64,
   cpu_time_qos_legacy: u64,
   cpu_time_qos_user_initiated: u64,
   cpu_time_qos_user_interactive: u64,
   billed_system_time: u64,
   serviced_system_time: u64,
   logical_writes: u64,
   lifetime_max_phys_footprint: u64,
   instructions: u64,
   cycles: u64,
   billed_energy: u64,
   serviced_energy: u64,
   interval_max_phys_footprint: u64,
   runnable_time: u64,
}

#[cfg(target_os = "macos")]
const _: [(); 296] = [(); std::mem::size_of::<ResourceUsageInfoV4>()];

#[cfg(target_os = "macos")]
#[link(name = "proc")]
unsafe extern "C"
{
   fn mach_absolute_time() -> u64;
   fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut ResourceUsageInfoV4) -> i32;
}

#[cfg(test)]
mod tests
{
   use super::*;
   use oxide_benchmark_spec::{AppleCampaignEvidenceRole, AppleCampaignPassRole, ComparisonOrder, DecimalU64};

   fn session(pass_id: &str) -> (MacOsCampaignPlan, MacOsCampaignSession)
   {
      let session = MacOsCampaignSession {
         chunk_id: String::from("chunk"),
         pass_id: String::from(pass_id),
         pack_id: String::from("pack"),
         pass_role: AppleCampaignPassRole::Primary,
         evidence_role: AppleCampaignEvidenceRole::ClaimBearing,
         collector: None,
         launch_class: None,
         timing: None,
         pair_index: 2,
         order: ComparisonOrder::Ab,
         side: ComparisonSide::Oxide,
         max_occupied_seconds: 90,
         scale_overlay: None,
      };
      let plan = MacOsCampaignPlan {
         schema_version: 2,
         run_id: String::from("resource-test"),
         plan_sha256: "a".repeat(64),
         seed: DecimalU64(0xaaaa_aaaa_aaaa_aaaa),
         plan_resource_path: None,
         sessions: vec![session.clone()],
      };
      (plan, session)
   }

   fn sample(timestamp: u64, cumulative: u64) -> MacOsResourceSample
   {
      MacOsResourceSample {
         mach_continuous_time: timestamp,
         user_cpu_ns: cumulative,
         system_cpu_ns: cumulative,
         package_idle_wakeups: cumulative,
         interrupt_wakeups: cumulative,
         pageins: cumulative,
         wired_bytes: 8,
         resident_bytes: 10,
         physical_footprint_bytes: 9,
         lifetime_max_physical_footprint_bytes: cumulative,
         interval_max_physical_footprint_bytes: 9,
         disk_read_bytes: cumulative,
         disk_written_bytes: cumulative,
         instructions: cumulative,
         cycles: cumulative,
         billed_system_time_ns: cumulative,
         serviced_system_time_ns: cumulative,
         billed_energy: cumulative,
         serviced_energy: cumulative,
         logical_writes: cumulative,
         runnable_time_ns: cumulative,
      }
   }

   #[test]
   fn measured_artifact_requires_identity_coverage_and_monotonic_counters()
   {
      let (plan, session) = session("minimal-presentation");
      let identity = MacOsResourceIdentity {
         plan: &plan,
         session: &session,
         generation: &"b".repeat(64),
         executable_sha256: &"c".repeat(64),
         pid: 42,
         launch_t0: 100,
      };
      let mut artifact = MacOsResourceArtifact {
         schema_version: 4,
         run_id: plan.run_id.clone(),
         plan_sha256: plan.plan_sha256.clone(),
         chunk_id: session.chunk_id.clone(),
         pass_id: session.pass_id.clone(),
         pair_index: session.pair_index,
         side: session.side,
         generation: String::from(identity.generation),
         executable_sha256: String::from(identity.executable_sha256),
         pack_id: session.pack_id.clone(),
         pid: identity.pid,
         process_uuid: "d".repeat(32),
         process_start_abstime: 50,
         process_start_continuous_time: 105,
         process_start_clock_uncertainty_ticks: 2,
         timebase_numerator: 1,
         timebase_denominator: 1,
         cadence_ns: MACOS_RESOURCE_CADENCE_NS,
         launch_t0: identity.launch_t0,
         durable_complete_timestamp: 190,
         availability: String::from(RESOURCE_AVAILABLE),
         samples: vec![sample(110, 1), sample(200, 2)],
         complete: true,
      };
      validate_resource_artifact(&artifact, &identity).expect("valid measured resource artifact");
      artifact.schema_version = 1;
      assert!(validate_resource_artifact(&artifact, &identity).is_err());
      artifact.schema_version = 4;
      artifact.samples[1].cycles = 0;
      assert!(validate_resource_artifact(&artifact, &identity).is_err());
      artifact.samples[1].cycles = 2;
      artifact.samples[1].runnable_time_ns = 0;
      assert!(validate_resource_artifact(&artifact, &identity).is_err());
      artifact.samples[1].runnable_time_ns = 2;
      artifact.pid = 43;
      assert!(validate_resource_artifact(&artifact, &identity).is_err());
   }

   #[test]
   fn correctness_artifact_is_explicitly_not_applicable()
   {
      let (plan, session) = session("correctness");
      let identity = MacOsResourceIdentity {
         plan: &plan,
         session: &session,
         generation: &"b".repeat(64),
         executable_sha256: &"c".repeat(64),
         pid: 42,
         launch_t0: 100,
      };
      let mut artifact = MacOsResourceArtifact::not_applicable(&identity, 200);
      validate_resource_artifact(&artifact, &identity).expect("not-applicable correctness artifact");
      artifact.schema_version = 1;
      validate_resource_artifact(&artifact, &identity).expect("legacy untimed correctness artifact");
   }

   #[test]
   fn resource_cadence_is_low_frequency_only_for_soak_energy_and_footprint_passes()
   {
      for pass_id in ["idle", "endurance", "energy", "memory", "attribution-physical-footprint"]
      {
         assert_eq!(resource_cadence(pass_id).as_nanos(), u128::from(MACOS_LOW_FREQUENCY_RESOURCE_CADENCE_NS));
      }
      for pass_id in ["minimal-presentation", "canonical-launch", "attribution-time-profiler", "attribution-system-trace", "common-gpu", "full-attribution"]
      {
         assert_eq!(resource_cadence(pass_id).as_nanos(), u128::from(MACOS_RESOURCE_CADENCE_NS));
      }
   }

   #[test]
   fn reducer_uses_recorded_timebase_and_robust_footprint_slope()
   {
      let (plan, session) = session("minimal-presentation");
      let identity = MacOsResourceIdentity {
         plan: &plan,
         session: &session,
         generation: &"b".repeat(64),
         executable_sha256: &"c".repeat(64),
         pid: 42,
         launch_t0: 50,
      };
      let mut samples = vec![sample(100, 10), sample(1_000_000_100, 30), sample(2_000_000_100, 60)];
      samples[0].physical_footprint_bytes = 1_000;
      samples[1].physical_footprint_bytes = 1_100;
      samples[2].physical_footprint_bytes = 1_200;
      samples[0].interval_max_physical_footprint_bytes = 1_000;
      samples[1].interval_max_physical_footprint_bytes = 1_250;
      samples[2].interval_max_physical_footprint_bytes = 1_200;
      let artifact = MacOsResourceArtifact {
         schema_version: 4,
         run_id: plan.run_id.clone(),
         plan_sha256: plan.plan_sha256.clone(),
         chunk_id: session.chunk_id.clone(),
         pass_id: session.pass_id.clone(),
         pair_index: session.pair_index,
         side: session.side,
         generation: String::from(identity.generation),
         executable_sha256: String::from(identity.executable_sha256),
         pack_id: session.pack_id.clone(),
         pid: identity.pid,
         process_uuid: "d".repeat(32),
         process_start_abstime: 25,
         process_start_continuous_time: 75,
         process_start_clock_uncertainty_ticks: 2,
         timebase_numerator: 1,
         timebase_denominator: 1,
         cadence_ns: MACOS_RESOURCE_CADENCE_NS,
         launch_t0: identity.launch_t0,
         durable_complete_timestamp: 2_000_000_100,
         availability: String::from(RESOURCE_AVAILABLE),
         samples,
         complete: true,
      };
      let bytes = serde_json::to_vec(&artifact).expect("serialize resource artifact");
      let summary = reduce_macos_resource_artifact(&bytes, None, None).expect("reduce resource artifact");
      assert_eq!(summary.wall_time_ns, 2_000_000_000);
      assert_eq!(summary.sample_count, 3);
      assert_eq!(summary.user_cpu_ns, 50);
      assert_eq!(summary.system_cpu_ns, 50);
      assert!((summary.process_cpu_ms_per_wall_s - 0.00005).abs() < f64::EPSILON);
      assert_eq!(summary.wakeups, 100);
      assert!((summary.wakeups_per_wall_s - 50.0).abs() < f64::EPSILON);
      assert_eq!(summary.disk_read_bytes, 50);
      assert_eq!(summary.disk_written_bytes, 50);
      assert_eq!(summary.logical_writes, 50);
      assert_eq!(summary.instructions, 50);
      assert_eq!(summary.cycles, 50);
      assert_eq!(summary.billed_system_time_ns, 50);
      assert_eq!(summary.serviced_system_time_ns, 50);
      assert_eq!(summary.billed_energy, 50);
      assert_eq!(summary.serviced_energy, 50);
      assert_eq!(summary.wired_peak_bytes, 8);
      assert_eq!(summary.resident_peak_bytes, 10);
      assert_eq!(summary.physical_footprint_peak_bytes, 1_250);
      assert!((summary.retained_slope_bytes_per_min - 6_000.0).abs() < f64::EPSILON);
   }

   #[cfg(target_os = "macos")]
   #[test]
   fn rusage_v4_layout_reads_the_exact_current_process()
   {
      let snapshot = process_resource_snapshot(std::process::id()).expect("current process RUSAGE_INFO_V4");
      assert!(snapshot.sample.mach_continuous_time > 0);
      assert!(snapshot.usage.process_start_abstime > 0);
      assert_eq!(std::mem::size_of::<ResourceUsageInfoV4>(), 296);
   }
}
