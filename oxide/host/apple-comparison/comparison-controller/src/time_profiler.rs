use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use super::trace::{cell_display, cell_is_sentinel, parse_message_field, parse_trace_rows, required_display, required_u64, TraceRow};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsTimeProfilerArtifact
{
   pub schema_version: u32,
   pub pid: u32,
   pub process: String,
   pub measured_sample_count: u64,
   pub measured_weight: u64,
   pub measured_main_thread_sample_count: u64,
   pub measured_main_thread_weight: u64,
   pub phases: Vec<MacOsTimeProfilerPhase>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsTimeProfilerPhase
{
   pub scenario_index: u64,
   pub scenario_identifier: u64,
   pub phase_identifier: u64,
   pub start_ns: u64,
   pub end_ns: u64,
   pub sample_count: u64,
   pub weight: u64,
   pub main_thread_sample_count: u64,
   pub main_thread_weight: u64,
   pub top_stacks: Vec<MacOsTimeProfilerStack>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsTimeProfilerStack
{
   pub stack: String,
   pub sample_count: u64,
   pub weight: u64,
}

#[derive(Clone, Debug)]
struct Boundary
{
   time_ns: u64,
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
   start_ns: u64,
   end_ns: u64,
}

#[derive(Clone, Debug)]
struct Sample
{
   time_ns: u64,
   process: String,
   thread: String,
   stack: String,
   weight: u64,
}

pub fn reduce_macos_time_profiler_trace(time_profile_xml: &str, signposts_xml: &str, exact_pid: u32) -> Result<MacOsTimeProfilerArtifact>
{
   if exact_pid == 0
   {
      bail!("macOS Time Profiler reduction requires a nonzero exact PID");
   }
   let suffix = format!(" ({})", exact_pid);
   let samples = parse_samples(&parse_trace_rows(time_profile_xml, "time-profile")?)?;
   let exact_samples = samples.iter().filter(|sample| sample.process.ends_with(&suffix)).collect::<Vec<_>>();
   if exact_samples.is_empty()
   {
      bail!("macOS Time Profiler trace has no samples for exact PID {}", exact_pid);
   }
   let processes = exact_samples.iter().map(|sample| sample.process.as_str()).collect::<BTreeSet<_>>();
   if processes.len() != 1
   {
      bail!("macOS Time Profiler trace has ambiguous formatted process identities for PID {}", exact_pid);
   }
   let process = String::from(*processes.iter().next().context("missing exact-PID Time Profiler process")?);
   let signposts = parse_trace_rows(signposts_xml, "os-signpost")?;
   let intervals = parse_intervals(&signposts, &process)?;
   let mut phases = Vec::with_capacity(intervals.len());
   for interval in intervals
   {
      let phase_samples = exact_samples.iter().filter(|sample| sample.time_ns >= interval.start_ns && sample.time_ns < interval.end_ns).copied().collect::<Vec<_>>();
      let mut stacks = BTreeMap::<String, (u64, u64)>::new();
      let mut weight = 0_u64;
      let mut main_thread_sample_count = 0_u64;
      let mut main_thread_weight = 0_u64;
      for sample in &phase_samples
      {
         weight = weight.checked_add(sample.weight).context("Time Profiler phase weight overflow")?;
         let entry = stacks.entry(sample.stack.clone()).or_insert((0, 0));
         entry.0 = entry.0.checked_add(1).context("Time Profiler stack sample-count overflow")?;
         entry.1 = entry.1.checked_add(sample.weight).context("Time Profiler stack weight overflow")?;
         if sample.thread.contains("Main Thread")
         {
            main_thread_sample_count = main_thread_sample_count.checked_add(1).context("Time Profiler main-thread sample-count overflow")?;
            main_thread_weight = main_thread_weight.checked_add(sample.weight).context("Time Profiler main-thread weight overflow")?;
         }
      }
      let mut top_stacks = stacks.into_iter().map(|(stack, (sample_count, weight))| MacOsTimeProfilerStack {stack, sample_count, weight}).collect::<Vec<_>>();
      top_stacks.sort_by(|left, right| right.weight.cmp(&left.weight).then_with(|| right.sample_count.cmp(&left.sample_count)).then_with(|| left.stack.cmp(&right.stack)));
      top_stacks.truncate(20);
      phases.push(MacOsTimeProfilerPhase {
         scenario_index: interval.scenario_index,
         scenario_identifier: interval.scenario_identifier,
         phase_identifier: interval.phase_identifier,
         start_ns: interval.start_ns,
         end_ns: interval.end_ns,
         sample_count: phase_samples.len() as u64,
         weight,
         main_thread_sample_count,
         main_thread_weight,
         top_stacks,
      });
   }
   let measured_sample_count = phases.iter().try_fold(0_u64, |total, phase| total.checked_add(phase.sample_count).context("Time Profiler measured sample-count overflow"))?;
   let measured_weight = phases.iter().try_fold(0_u64, |total, phase| total.checked_add(phase.weight).context("Time Profiler measured weight overflow"))?;
   let measured_main_thread_sample_count = phases.iter().try_fold(0_u64, |total, phase| total.checked_add(phase.main_thread_sample_count).context("Time Profiler measured main-thread sample-count overflow"))?;
   let measured_main_thread_weight = phases.iter().try_fold(0_u64, |total, phase| total.checked_add(phase.main_thread_weight).context("Time Profiler measured main-thread weight overflow"))?;
   if measured_sample_count == 0 || measured_weight == 0
   {
      bail!("macOS Time Profiler trace has no exact-PID samples inside any measured phase");
   }
   Ok(MacOsTimeProfilerArtifact {
      schema_version: 1,
      pid: exact_pid,
      process,
      measured_sample_count,
      measured_weight,
      measured_main_thread_sample_count,
      measured_main_thread_weight,
      phases,
   })
}

fn parse_samples(rows: &[TraceRow]) -> Result<Vec<Sample>>
{
   rows.iter().map(|row| {
      let stack = ["backtrace", "stack", "extended-backtrace"].into_iter()
         .find_map(|mnemonic| cell_display(row, mnemonic))
         .filter(|value| !value.is_empty())
         .or_else(|| ["backtrace", "stack", "extended-backtrace"].into_iter().any(|mnemonic| cell_is_sentinel(row, mnemonic)).then_some("<unavailable-sentinel>"))
         .context("time-profile row has no stack attribution")?;
      let thread = ["thread", "thread-name"].into_iter().find_map(|mnemonic| cell_display(row, mnemonic)).filter(|value| !value.is_empty()).context("time-profile row has no thread identity")?;
      let weight = required_u64(row, "weight", "time-profile")?;
      if weight == 0
      {
         bail!("time-profile row has zero sample weight");
      }
      Ok(Sample {
         time_ns: required_u64(row, "time", "time-profile")?,
         process: required_display(row, "process", "time-profile")?.to_string(),
         thread: thread.to_string(),
         stack: stack.to_string(),
         weight,
      })
   }).collect()
}

fn parse_intervals(rows: &[TraceRow], process: &str) -> Result<Vec<Interval>>
{
   let mut scenario_begins = BTreeMap::<u64, Boundary>::new();
   let mut scenario_intervals = BTreeMap::<u64, (u64, u64, u64)>::new();
   let mut phase_begins = BTreeMap::<(u64, u64), Boundary>::new();
   let mut intervals = Vec::new();
   for row in rows
   {
      if cell_display(row, "subsystem") != Some("com.oxide.comparison")
         || cell_display(row, "category") != Some("Presentation")
         || cell_display(row, "event-type") != Some("Event")
         || cell_display(row, "process") != Some(process)
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
         let value = parse_message_field(message, "measured")?;
         if value > 1
         {
            bail!("Time Profiler phase boundary has a non-Boolean measured field");
         }
         value == 1
      }
      else
      {
         false
      };
      let boundary = Boundary {
         time_ns: required_u64(row, "time", "os-signpost")?,
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
               bail!("duplicate Time Profiler ScenarioBegin boundary");
            }
         }
         "ScenarioEnd" =>
         {
            let begin = scenario_begins.remove(&boundary.scenario_index).context("Time Profiler ScenarioEnd has no matching begin")?;
            if begin.identifier != boundary.identifier || begin.time_ns >= boundary.time_ns || scenario_intervals.insert(boundary.scenario_index, (begin.identifier, begin.time_ns, boundary.time_ns)).is_some()
            {
               bail!("Time Profiler scenario boundaries are ambiguous or reversed");
            }
         }
         "PhaseBegin" =>
         {
            let key = (boundary.scenario_index, boundary.identifier);
            if phase_begins.insert(key, boundary).is_some()
            {
               bail!("duplicate Time Profiler PhaseBegin boundary");
            }
         }
         "PhaseEnd" =>
         {
            let key = (boundary.scenario_index, boundary.identifier);
            let begin = phase_begins.remove(&key).context("Time Profiler PhaseEnd has no matching begin")?;
            if begin.measured != boundary.measured || begin.time_ns >= boundary.time_ns
            {
               bail!("Time Profiler phase boundaries disagree or are reversed");
            }
            if begin.measured
            {
               intervals.push(Interval {
                  scenario_index: boundary.scenario_index,
                  scenario_identifier: 0,
                  phase_identifier: boundary.identifier,
                  start_ns: begin.time_ns,
                  end_ns: boundary.time_ns,
               });
            }
         }
         _ => unreachable!(),
      }
   }
   if !scenario_begins.is_empty() || !phase_begins.is_empty() || scenario_intervals.is_empty() || intervals.is_empty()
   {
      bail!("Time Profiler signpost boundaries are missing or incomplete");
   }
   for interval in &mut intervals
   {
      let (scenario_identifier, scenario_start, scenario_end) = scenario_intervals.get(&interval.scenario_index).context("measured Time Profiler phase has no scenario interval")?;
      if interval.start_ns < *scenario_start || interval.end_ns > *scenario_end
      {
         bail!("measured Time Profiler phase escapes its scenario interval");
      }
      interval.scenario_identifier = *scenario_identifier;
   }
   intervals.sort_by_key(|interval| interval.start_ns);
   for pair in intervals.windows(2)
   {
      if pair[0].end_ns > pair[1].start_ns
      {
         bail!("measured Time Profiler phase intervals overlap");
      }
   }
   Ok(intervals)
}
