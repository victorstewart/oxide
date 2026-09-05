use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::trace::{cell_display, parse_message_field, parse_trace_rows, required_display, required_u64, TraceRow};
use super::mach_continuous_time;

pub(crate) const MACOS_COMMON_GPU_CADENCE_NS: u64 = 50_000_000;
const MACOS_COMMON_GPU_CADENCE: Duration = Duration::from_nanos(MACOS_COMMON_GPU_CADENCE_NS);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsCommonGpuSample
{
   pub mach_continuous_time: u64,
   pub task_gpu_time_ns: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsCommonGpuSamples
{
   pub schema_version: u32,
   pub pid: u32,
   pub cadence_ns: u64,
   pub source: String,
   pub samples: Vec<MacOsCommonGpuSample>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsCommonGpuSummary
{
   pub schema_version: u32,
   pub pid: u32,
   pub process: String,
   pub source: String,
   pub availability: String,
   pub comparison_eligible: bool,
   pub cadence_ns: u64,
   pub measured_gpu_time_ns: u64,
   pub phases: Vec<MacOsCommonGpuPhaseSummary>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsCommonGpuPhaseSummary
{
   pub scenario_index: u64,
   pub scenario_identifier: u64,
   pub phase_identifier: u64,
   pub phase_start_ticks: u64,
   pub phase_end_ticks: u64,
   pub observed_start_ticks: u64,
   pub observed_end_ticks: u64,
   pub sample_count: u64,
   pub gpu_time_ns: u64,
}

#[derive(Clone, Debug)]
struct Boundary
{
   time_ticks: u64,
   scenario_index: u64,
   identifier: u64,
   measured: bool,
}

#[derive(Clone, Debug)]
struct Interval
{
   scenario_index: u64,
   scenario_identifier: u64,
   phase_identifier: u64,
   start_ticks: u64,
   end_ticks: u64,
}

enum CollectorCommand
{
   Finish,
   Abort,
}

pub(crate) struct MacOsCommonGpuCollector
{
   command: Sender<CollectorCommand>,
   thread: Option<JoinHandle<Result<Vec<MacOsCommonGpuSample>>>>,
   pid: u32,
}

impl MacOsCommonGpuCollector
{
   pub(crate) fn start(pid: u32) -> Result<Self>
   {
      if pid == 0
      {
         bail!("macOS common-GPU collector requires a nonzero exact PID");
      }
      let port = TaskNamePort::open(pid)?;
      let initial = sample_task_gpu(&port)?;
      let (command, commands) = mpsc::channel();
      let thread = thread::Builder::new()
         .name(String::from("macos-common-gpu-sampler"))
         .spawn(move || collect_task_gpu(port, initial, commands))
         .context("starting exact-process macOS common-GPU sampler")?;
      Ok(Self {command, thread: Some(thread), pid})
   }

   pub(crate) fn finish(mut self) -> Result<MacOsCommonGpuSamples>
   {
      self.command.send(CollectorCommand::Finish).context("stopping macOS common-GPU sampler")?;
      let samples = self.thread.take().context("macOS common-GPU sampler has no thread")?.join().map_err(|_| anyhow::anyhow!("macOS common-GPU sampler panicked"))??;
      if samples.len() < 2
      {
         bail!("macOS common-GPU sampler emitted fewer than two samples");
      }
      Ok(MacOsCommonGpuSamples {
         schema_version: 1,
         pid: self.pid,
         cadence_ns: MACOS_COMMON_GPU_CADENCE_NS,
         source: String::from("task-info-power-v2-task-gpu-utilisation-ns"),
         samples,
      })
   }
}

impl Drop for MacOsCommonGpuCollector
{
   fn drop(&mut self)
   {
      let _ = self.command.send(CollectorCommand::Abort);
      if let Some(thread) = self.thread.take()
      {
         let _ = thread.join();
      }
   }
}

pub fn reduce_macos_common_gpu(samples: &MacOsCommonGpuSamples, signposts_xml: &str, exact_pid: u32) -> Result<MacOsCommonGpuSummary>
{
   if exact_pid == 0 || samples.pid != exact_pid || samples.schema_version != 1
      || samples.cadence_ns != MACOS_COMMON_GPU_CADENCE_NS
      || samples.source != "task-info-power-v2-task-gpu-utilisation-ns"
   {
      bail!("macOS common-GPU samples have invalid source or exact-PID identity");
   }
   validate_samples(&samples.samples)?;
   let rows = parse_trace_rows(signposts_xml, "os-signpost")?;
   let (process, intervals) = parse_intervals(&rows, exact_pid)?;
   let mut phases = Vec::with_capacity(intervals.len());
   for interval in intervals
   {
      let bounded = samples.samples.iter().filter(|sample| sample.mach_continuous_time >= interval.start_ticks && sample.mach_continuous_time <= interval.end_ticks).collect::<Vec<_>>();
      if bounded.len() < 2
      {
         bail!("macOS common-GPU measured phase has fewer than two phase-bounded samples");
      }
      let first = bounded.first().context("common-GPU phase has no first sample")?;
      let last = bounded.last().context("common-GPU phase has no last sample")?;
      phases.push(MacOsCommonGpuPhaseSummary {
         scenario_index: interval.scenario_index,
         scenario_identifier: interval.scenario_identifier,
         phase_identifier: interval.phase_identifier,
         phase_start_ticks: interval.start_ticks,
         phase_end_ticks: interval.end_ticks,
         observed_start_ticks: first.mach_continuous_time,
         observed_end_ticks: last.mach_continuous_time,
         sample_count: bounded.len() as u64,
         gpu_time_ns: last.task_gpu_time_ns.checked_sub(first.task_gpu_time_ns).context("macOS task GPU time regressed within a measured phase")?,
      });
   }
   let measured_gpu_time_ns = phases.iter().try_fold(0_u64, |total, phase| total.checked_add(phase.gpu_time_ns).context("macOS common-GPU measured time overflow"))?;
   if measured_gpu_time_ns == 0
   {
      bail!("TASK_POWER_INFO_V2 exposed no GPU time in any measured phase; common-GPU evidence is unavailable on this process/toolchain");
   }
   Ok(MacOsCommonGpuSummary {
      schema_version: 1,
      pid: exact_pid,
      process,
      source: samples.source.clone(),
      availability: String::from("process-scoped-diagnostic-only-compositor-ownership-asymmetric"),
      comparison_eligible: false,
      cadence_ns: samples.cadence_ns,
      measured_gpu_time_ns,
      phases,
   })
}

fn validate_samples(samples: &[MacOsCommonGpuSample]) -> Result<()>
{
   if samples.len() < 2
   {
      bail!("macOS common-GPU evidence has fewer than two samples");
   }
   for pair in samples.windows(2)
   {
      if pair[0].mach_continuous_time >= pair[1].mach_continuous_time || pair[0].task_gpu_time_ns > pair[1].task_gpu_time_ns
      {
         bail!("macOS common-GPU samples are nonmonotonic");
      }
   }
   Ok(())
}

fn parse_intervals(rows: &[TraceRow], exact_pid: u32) -> Result<(String, Vec<Interval>)>
{
   let suffix = format!(" ({})", exact_pid);
   let processes = rows.iter().filter_map(|row| cell_display(row, "process")).filter(|process| process.ends_with(&suffix)).collect::<BTreeSet<_>>();
   if processes.len() != 1
   {
      bail!("macOS common-GPU signposts have missing or ambiguous exact-PID process identity");
   }
   let process = String::from(*processes.iter().next().context("missing common-GPU process")?);
   let mut scenario_begins = BTreeMap::<u64, Boundary>::new();
   let mut scenario_intervals = BTreeMap::<u64, (u64, u64, u64)>::new();
   let mut phase_begins = BTreeMap::<(u64, u64), Boundary>::new();
   let mut intervals = Vec::new();
   for row in rows
   {
      if cell_display(row, "subsystem") != Some("com.oxide.comparison")
         || cell_display(row, "category") != Some("Presentation")
         || cell_display(row, "event-type") != Some("Event")
         || cell_display(row, "process") != Some(process.as_str())
      {
         continue;
      }
      let name = required_display(row, "name", "os-signpost")?;
      if !["ScenarioBegin", "ScenarioEnd", "PhaseBegin", "PhaseEnd"].contains(&name)
      {
         continue;
      }
      let message = required_display(row, "message", "os-signpost")?;
      let measured = if name == "PhaseBegin" || name == "PhaseEnd"
      {
         match parse_message_field(message, "measured")?
         {
            0 => false,
            1 => true,
            _ => bail!("macOS common-GPU phase boundary has a non-Boolean measured field"),
         }
      }
      else {false};
      let boundary = Boundary {
         time_ticks: required_u64(row, "time", "os-signpost")?,
         scenario_index: parse_message_field(message, "scenario")?,
         identifier: parse_message_field(message, "identifier")?,
         measured,
      };
      match name
      {
         "ScenarioBegin" =>
         {
            if scenario_begins.insert(boundary.scenario_index, boundary).is_some()
            {
               bail!("duplicate common-GPU ScenarioBegin");
            }
         }
         "ScenarioEnd" =>
         {
            let begin = scenario_begins.remove(&boundary.scenario_index).context("common-GPU ScenarioEnd has no matching begin")?;
            if begin.identifier != boundary.identifier || begin.time_ticks >= boundary.time_ticks || scenario_intervals.insert(boundary.scenario_index, (begin.identifier, begin.time_ticks, boundary.time_ticks)).is_some()
            {
               bail!("macOS common-GPU scenario boundaries are ambiguous or reversed");
            }
         }
         "PhaseBegin" =>
         {
            if phase_begins.insert((boundary.scenario_index, boundary.identifier), boundary).is_some()
            {
               bail!("duplicate common-GPU PhaseBegin");
            }
         }
         "PhaseEnd" =>
         {
            let key = (boundary.scenario_index, boundary.identifier);
            let begin = phase_begins.remove(&key).context("common-GPU PhaseEnd has no matching begin")?;
            if begin.measured != boundary.measured || begin.time_ticks >= boundary.time_ticks
            {
               bail!("macOS common-GPU phase boundaries disagree or are reversed");
            }
            if begin.measured
            {
               intervals.push(Interval {scenario_index: boundary.scenario_index, scenario_identifier: 0, phase_identifier: boundary.identifier, start_ticks: begin.time_ticks, end_ticks: boundary.time_ticks});
            }
         }
         _ => unreachable!(),
      }
   }
   if !scenario_begins.is_empty() || !phase_begins.is_empty() || scenario_intervals.is_empty() || intervals.is_empty()
   {
      bail!("macOS common-GPU signpost boundaries are missing or incomplete");
   }
   for interval in &mut intervals
   {
      let (identifier, start, end) = scenario_intervals.get(&interval.scenario_index).context("common-GPU measured phase has no scenario interval")?;
      if interval.start_ticks < *start || interval.end_ticks > *end
      {
         bail!("macOS common-GPU measured phase escapes its scenario interval");
      }
      interval.scenario_identifier = *identifier;
   }
   intervals.sort_by_key(|interval| interval.start_ticks);
   if intervals.windows(2).any(|pair| pair[0].end_ticks > pair[1].start_ticks)
   {
      bail!("macOS common-GPU measured phase intervals overlap");
   }
   Ok((process, intervals))
}

fn collect_task_gpu(port: TaskNamePort, initial: MacOsCommonGpuSample, commands: Receiver<CollectorCommand>) -> Result<Vec<MacOsCommonGpuSample>>
{
   let mut samples = vec![initial];
   let mut deadline = Instant::now() + MACOS_COMMON_GPU_CADENCE;
   loop
   {
      match commands.recv_timeout(deadline.saturating_duration_since(Instant::now()))
      {
         Ok(CollectorCommand::Finish) =>
         {
            samples.push(sample_task_gpu(&port)?);
            return Ok(samples);
         }
         Ok(CollectorCommand::Abort) | Err(RecvTimeoutError::Disconnected) => return Ok(samples),
         Err(RecvTimeoutError::Timeout) =>
         {
            let sample = sample_task_gpu(&port)?;
            if let Some(previous) = samples.last()
            {
               if sample.mach_continuous_time <= previous.mach_continuous_time || sample.task_gpu_time_ns < previous.task_gpu_time_ns
               {
                  bail!("macOS TASK_POWER_INFO_V2 common-GPU counter is nonmonotonic");
               }
            }
            samples.push(sample);
            let now = Instant::now();
            while deadline <= now {deadline += MACOS_COMMON_GPU_CADENCE;}
         }
      }
   }
}

#[cfg(target_os = "macos")]
fn sample_task_gpu(port: &TaskNamePort) -> Result<MacOsCommonGpuSample>
{
   let mut info = TaskPowerInfoV2::default();
   let mut count = u32::try_from(std::mem::size_of::<TaskPowerInfoV2>() / std::mem::size_of::<u32>()).context("TASK_POWER_INFO_V2 count overflow")?;
   let status = unsafe {task_info(port.0, 26, (&mut info as *mut TaskPowerInfoV2).cast::<i32>(), &mut count)};
   if status != 0 || count != 26
   {
      bail!("task_info(TASK_POWER_INFO_V2) failed with status {} and count {}", status, count);
   }
   Ok(MacOsCommonGpuSample {mach_continuous_time: mach_continuous_time(), task_gpu_time_ns: info.task_gpu_utilisation})
}

#[cfg(not(target_os = "macos"))]
fn sample_task_gpu(_port: &TaskNamePort) -> Result<MacOsCommonGpuSample>
{
   bail!("TASK_POWER_INFO_V2 common-GPU sampling is available only on macOS")
}

struct TaskNamePort(u32);

impl TaskNamePort
{
   #[cfg(target_os = "macos")]
   fn open(pid: u32) -> Result<Self>
   {
      let pid = i32::try_from(pid).context("macOS common-GPU PID exceeds signed range")?;
      let mut port = 0_u32;
      let status = unsafe {task_name_for_pid(mach_task_self_, pid, &mut port)};
      if status != 0 || port == 0
      {
         bail!("task_name_for_pid failed for exact common-GPU PID with status {}", status);
      }
      Ok(Self(port))
   }

   #[cfg(not(target_os = "macos"))]
   fn open(_pid: u32) -> Result<Self>
   {
      bail!("task name ports are available only on macOS")
   }
}

#[cfg(target_os = "macos")]
impl Drop for TaskNamePort
{
   fn drop(&mut self)
   {
      let _ = unsafe {mach_port_deallocate(mach_task_self_, self.0)};
   }
}

#[repr(C)]
#[derive(Default)]
struct TaskPowerInfoV2
{
   total_user: u64,
   total_system: u64,
   task_interrupt_wakeups: u64,
   task_platform_idle_wakeups: u64,
   task_timer_wakeups_bin_1: u64,
   task_timer_wakeups_bin_2: u64,
   task_gpu_utilisation: u64,
   task_gpu_stat_reserved0: u64,
   task_gpu_stat_reserved1: u64,
   task_gpu_stat_reserved2: u64,
   task_energy: u64,
   task_ptime: u64,
   task_pset_switches: u64,
}

#[cfg(target_os = "macos")]
const _: [(); 104] = [(); std::mem::size_of::<TaskPowerInfoV2>()];

#[cfg(target_os = "macos")]
unsafe extern "C"
{
   static mach_task_self_: u32;
   fn task_name_for_pid(target_task: u32, pid: i32, task: *mut u32) -> i32;
   fn task_info(target_task: u32, flavor: i32, info: *mut i32, count: *mut u32) -> i32;
   fn mach_port_deallocate(task: u32, name: u32) -> i32;
}
