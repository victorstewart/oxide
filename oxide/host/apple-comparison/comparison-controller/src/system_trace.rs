use anyhow::{bail, Context, Result};
use roxmltree::Document;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::ops::{Deref, DerefMut};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::{durable_json, native_xcrun_command, terminate_child, validate_macos_presentation_trace_bundle};

pub const MACOS_SYSTEM_TRACE_AVAILABILITY: &str = "available-exact-pid-main-thread-system-trace";
const SYSTEM_TRACE_WORKING_SET_LIMIT_BYTES: u64 = 512 * 1_024 * 1_024;
const FUSE_OK: u8 = 0;
const FUSE_LIMIT_EXCEEDED: u8 = 1;
const FUSE_MEASUREMENT_FAILED: u8 = 2;

const OS_SIGNPOST_COLUMNS: &[&str] = &["time", "thread", "process", "event-type", "scope", "identifier", "name", "format-string", "backtrace", "subsystem", "category", "message", "emit-location"];
const THREAD_INFO_COLUMNS: &[&str] = &["time", "pid", "tid", "process", "thread", "name", "main-thread"];
const THREAD_STATE_COLUMNS: &[&str] = &["start", "thread", "state", "duration", "process", "core", "cputime", "waittime", "priority", "note", "summary"];
const CONTEXT_SWITCH_COLUMNS: &[&str] = &["time", "thread", "event", "process", "cpu", "priority", "note"];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsSystemTracePhaseSummary
{
   pub scenario_index: u64,
   pub phase_identifier: u64,
   pub begin_ns: u64,
   pub end_ns: u64,
   pub main_thread_running_ns: u64,
   pub runnable_wait_ns: u64,
   pub context_switches: u64,
   pub wakeups: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsSystemTraceSummary
{
   pub schema_version: u32,
   pub availability: String,
   pub pid: u32,
   pub main_thread_tid: u64,
   pub exact_pid_filtered: bool,
   pub exact_main_thread_filtered: bool,
   pub phases: Vec<MacOsSystemTracePhaseSummary>,
}

#[derive(Clone, Debug, Default)]
struct TraceCell
{
   raw: Option<String>,
   formatted: Option<String>,
   pid: Option<u32>,
   tid: Option<u64>,
}

impl TraceCell
{
   fn display(&self) -> Option<&str>
   {
      self.formatted.as_deref().or(self.raw.as_deref())
   }

   fn raw_u64(&self) -> Option<u64>
   {
      self.raw.as_deref()?.parse::<u64>().ok()
   }
}

type TraceRow = BTreeMap<String, TraceCell>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PhaseBoundaryKind
{
   Begin,
   End,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PhaseBoundary
{
   kind: PhaseBoundaryKind,
   scenario_index: u64,
   identifier: u64,
   time_ns: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ThreadStateInterval
{
   start_ns: u64,
   end_ns: u64,
   state: String,
}

pub fn reduce_macos_system_trace(signposts_xml: &str, thread_info_xml: &str, thread_state_xml: &str, context_switch_xml: &str, pid: u32) -> Result<MacOsSystemTraceSummary>
{
   if pid == 0
   {
      bail!("macOS System Trace target PID cannot be zero");
   }
   let signposts = parse_trace_rows(signposts_xml, "os-signpost", OS_SIGNPOST_COLUMNS)?;
   let thread_info = parse_trace_rows(thread_info_xml, "thread-info", THREAD_INFO_COLUMNS)?;
   let thread_states = parse_trace_rows(thread_state_xml, "thread-state", THREAD_STATE_COLUMNS)?;
   let context_switches = parse_trace_rows(context_switch_xml, "context-switch", CONTEXT_SWITCH_COLUMNS)?;
   let main_thread_tid = exact_main_thread_tid(&thread_info, pid)?;
   let phases = measured_phases(&signposts, pid, main_thread_tid)?;
   let states = exact_main_thread_states(&thread_states, pid, main_thread_tid)?;
   let switches = exact_main_thread_context_switches(&context_switches, pid, main_thread_tid)?;
   let mut summaries = Vec::with_capacity(phases.len());
   for (begin, end) in phases
   {
      validate_state_coverage(&states, begin.time_ns, end.time_ns)?;
      let main_thread_running_ns = state_overlap(&states, begin.time_ns, end.time_ns, "Running")?;
      let runnable_wait_ns = state_overlap(&states, begin.time_ns, end.time_ns, "Runnable")?;
      let context_switches = switches.range(begin.time_ns..end.time_ns).count() as u64;
      let wakeups = count_wakeups(&states, begin.time_ns, end.time_ns);
      summaries.push(MacOsSystemTracePhaseSummary {
         scenario_index: begin.scenario_index,
         phase_identifier: begin.identifier,
         begin_ns: begin.time_ns,
         end_ns: end.time_ns,
         main_thread_running_ns,
         runnable_wait_ns,
         context_switches,
         wakeups,
      });
   }
   if summaries.is_empty()
   {
      bail!("macOS System Trace has no complete measured phase");
   }
   Ok(MacOsSystemTraceSummary {
      schema_version: 1,
      availability: String::from(MACOS_SYSTEM_TRACE_AVAILABILITY),
      pid,
      main_thread_tid,
      exact_pid_filtered: true,
      exact_main_thread_filtered: true,
      phases: summaries,
   })
}

fn exact_main_thread_tid(rows: &[TraceRow], pid: u32) -> Result<u64>
{
   let mut tids = BTreeSet::new();
   for row in rows
   {
      let row_pid = required_u32(row, "pid", "thread-info")?;
      validate_cell_pid(row, "process", row_pid, "thread-info")?;
      let tid = required_u64(row, "tid", "thread-info")?;
      validate_cell_tid(row, "thread", tid, "thread-info")?;
      if row_pid == pid && required_boolean(row, "main-thread", "thread-info")?
      {
         tids.insert(tid);
      }
   }
   if tids.len() != 1
   {
      bail!("expected exactly one main thread for System Trace PID {}, observed {}", pid, tids.len());
   }
   tids.into_iter().next().context("main-thread identity disappeared")
}

fn measured_phases(rows: &[TraceRow], pid: u32, main_thread_tid: u64) -> Result<Vec<(PhaseBoundary, PhaseBoundary)>>
{
   let mut boundaries = Vec::new();
   for row in rows
   {
      if cell_display(row, "subsystem") != Some("com.oxide.comparison")
         || cell_display(row, "category") != Some("Presentation")
         || cell_display(row, "event-type") != Some("Event")
      {
         continue;
      }
      let kind = match cell_display(row, "name")
      {
         Some("PhaseBegin") => PhaseBoundaryKind::Begin,
         Some("PhaseEnd") => PhaseBoundaryKind::End,
         _ => continue,
      };
      let process = row.get("process").context("os-signpost phase row has no process")?;
      let thread = row.get("thread").context("os-signpost phase row has no thread")?;
      if process.pid != Some(pid) || thread.pid.is_some_and(|observed| observed != pid) || thread.tid != Some(main_thread_tid)
      {
         bail!("macOS System Trace phase boundary does not belong to exact PID {} main thread {}", pid, main_thread_tid);
      }
      let message = required_display(row, "message", "os-signpost")?;
      let measured = parse_message_field(message, "measured")?;
      if measured > 1
      {
         bail!("macOS System Trace phase boundary has invalid measured flag {}", measured);
      }
      if measured == 0
      {
         continue;
      }
      boundaries.push(PhaseBoundary {
         kind,
         scenario_index: parse_message_field(message, "scenario")?,
         identifier: parse_message_field(message, "identifier")?,
         time_ns: required_u64(row, "time", "os-signpost")?,
      });
   }
   boundaries.sort_by_key(|boundary| boundary.time_ns);
   let mut open = BTreeMap::<(u64, u64), PhaseBoundary>::new();
   let mut phases = Vec::new();
   for boundary in boundaries
   {
      let key = (boundary.scenario_index, boundary.identifier);
      match boundary.kind
      {
         PhaseBoundaryKind::Begin =>
         {
            if open.insert(key, boundary).is_some()
            {
               bail!("duplicate measured PhaseBegin for scenario {} identifier {}", key.0, key.1);
            }
         }
         PhaseBoundaryKind::End =>
         {
            let begin = open.remove(&key).with_context(|| format!("measured PhaseEnd has no PhaseBegin for scenario {} identifier {}", key.0, key.1))?;
            if boundary.time_ns <= begin.time_ns
            {
               bail!("measured phase has a non-positive interval for scenario {} identifier {}", key.0, key.1);
            }
            phases.push((begin, boundary));
         }
      }
   }
   if !open.is_empty()
   {
      bail!("macOS System Trace ended with {} unmatched measured PhaseBegin markers", open.len());
   }
   phases.sort_by_key(|(begin, _)| begin.time_ns);
   for pair in phases.windows(2)
   {
      if pair[0].1.time_ns > pair[1].0.time_ns
      {
         bail!("macOS System Trace measured phase intervals overlap");
      }
   }
   Ok(phases)
}

fn exact_main_thread_states(rows: &[TraceRow], pid: u32, main_thread_tid: u64) -> Result<Vec<ThreadStateInterval>>
{
   let mut states = Vec::new();
   for row in rows
   {
      let Some(thread) = row.get("thread") else {continue};
      if thread.tid != Some(main_thread_tid)
      {
         continue;
      }
      if thread.pid.is_some_and(|observed| observed != pid)
      {
         bail!("System Trace main-thread row has a mismatched owning PID");
      }
      if let Some(process) = row.get("process")
      {
         if process.pid != Some(pid)
         {
            bail!("System Trace main-thread state row has a mismatched process identity");
         }
      }
      let start_ns = required_u64(row, "start", "thread-state")?;
      let duration_ns = required_u64(row, "duration", "thread-state")?;
      if duration_ns == 0
      {
         bail!("System Trace main-thread state interval has zero duration");
      }
      states.push(ThreadStateInterval {
         start_ns,
         end_ns: start_ns.checked_add(duration_ns).context("System Trace thread-state interval overflow")?,
         state: required_display(row, "state", "thread-state")?.to_string(),
      });
   }
   states.sort_by_key(|state| state.start_ns);
   if states.is_empty()
   {
      bail!("System Trace has no exact main-thread state intervals");
   }
   for pair in states.windows(2)
   {
      if pair[0].end_ns > pair[1].start_ns
      {
         bail!("System Trace exact main-thread state intervals overlap");
      }
   }
   Ok(states)
}

fn exact_main_thread_context_switches(rows: &[TraceRow], pid: u32, main_thread_tid: u64) -> Result<BTreeSet<u64>>
{
   let mut switches = BTreeSet::new();
   let mut observed_main_thread = false;
   for row in rows
   {
      let Some(thread) = row.get("thread") else {continue};
      if thread.tid != Some(main_thread_tid)
      {
         continue;
      }
      observed_main_thread = true;
      if thread.pid.is_some_and(|observed| observed != pid)
      {
         bail!("System Trace context switch has a mismatched main-thread PID");
      }
      if let Some(process) = row.get("process")
      {
         if process.pid != Some(pid)
         {
            bail!("System Trace context switch has a mismatched process identity");
         }
      }
      required_display(row, "event", "context-switch")?;
      switches.insert(required_u64(row, "time", "context-switch")?);
   }
   if !observed_main_thread
   {
      bail!("System Trace has no exact main-thread context-switch evidence");
   }
   Ok(switches)
}

fn validate_state_coverage(states: &[ThreadStateInterval], begin_ns: u64, end_ns: u64) -> Result<()>
{
   let overlapping = states.iter().filter(|state| state.start_ns < end_ns && state.end_ns > begin_ns).collect::<Vec<_>>();
   let first = overlapping.first().context("System Trace measured phase has no main-thread state coverage")?;
   if first.start_ns > begin_ns
   {
      bail!("System Trace main-thread states begin after a measured PhaseBegin");
   }
   let mut covered_until = first.end_ns;
   for state in overlapping.iter().skip(1)
   {
      if state.start_ns > covered_until
      {
         bail!("System Trace main-thread states have a gap inside a measured phase");
      }
      covered_until = covered_until.max(state.end_ns);
   }
   if covered_until < end_ns
   {
      bail!("System Trace main-thread states end before a measured PhaseEnd");
   }
   Ok(())
}

fn state_overlap(states: &[ThreadStateInterval], begin_ns: u64, end_ns: u64, wanted: &str) -> Result<u64>
{
   let mut total = 0_u64;
   for state in states.iter().filter(|state| state.state == wanted && state.start_ns < end_ns && state.end_ns > begin_ns)
   {
      let overlap = state.end_ns.min(end_ns).checked_sub(state.start_ns.max(begin_ns)).context("System Trace overlap underflow")?;
      total = total.checked_add(overlap).context("System Trace state duration overflow")?;
   }
   Ok(total)
}

fn count_wakeups(states: &[ThreadStateInterval], begin_ns: u64, end_ns: u64) -> u64
{
   states.windows(2).filter(|pair| {
      pair[1].start_ns >= begin_ns
         && pair[1].start_ns < end_ns
         && pair[1].state == "Runnable"
         && !matches!(pair[0].state.as_str(), "Running" | "Runnable")
   }).count() as u64
}

fn parse_message_field(message: &str, field: &str) -> Result<u64>
{
   let prefix = format!("{}=", field);
   let start = message.find(&prefix).with_context(|| format!("System Trace marker message has no {} field", field))? + prefix.len();
   let tail = message[start..].trim_start();
   let digits = tail.bytes().take_while(u8::is_ascii_digit).count();
   if digits == 0
   {
      bail!("System Trace marker message has no numeric {} value", field);
   }
   tail[..digits].parse::<u64>().with_context(|| format!("parsing System Trace marker {} value", field))
}

fn required_boolean(row: &TraceRow, mnemonic: &str, schema: &str) -> Result<bool>
{
   match required_u64(row, mnemonic, schema)?
   {
      0 => Ok(false),
      1 => Ok(true),
      value => bail!("{} row has invalid boolean {} value {}", schema, mnemonic, value),
   }
}

fn required_u32(row: &TraceRow, mnemonic: &str, schema: &str) -> Result<u32>
{
   u32::try_from(required_u64(row, mnemonic, schema)?).with_context(|| format!("{} {} exceeds u32", schema, mnemonic))
}

fn required_u64(row: &TraceRow, mnemonic: &str, schema: &str) -> Result<u64>
{
   row.get(mnemonic).and_then(TraceCell::raw_u64).with_context(|| format!("{} row has no numeric {} value", schema, mnemonic))
}

fn required_display<'a>(row: &'a TraceRow, mnemonic: &str, schema: &str) -> Result<&'a str>
{
   cell_display(row, mnemonic).filter(|value| !value.is_empty()).with_context(|| format!("{} row has no {} value", schema, mnemonic))
}

fn cell_display<'a>(row: &'a TraceRow, mnemonic: &str) -> Option<&'a str>
{
   row.get(mnemonic).and_then(TraceCell::display)
}

fn validate_cell_pid(row: &TraceRow, mnemonic: &str, expected: u32, schema: &str) -> Result<()>
{
   if row.get(mnemonic).and_then(|cell| cell.pid) != Some(expected)
   {
      bail!("{} row has mismatched {} PID identity", schema, mnemonic);
   }
   Ok(())
}

fn validate_cell_tid(row: &TraceRow, mnemonic: &str, expected: u64, schema: &str) -> Result<()>
{
   if row.get(mnemonic).and_then(|cell| cell.tid) != Some(expected)
   {
      bail!("{} row has mismatched {} thread identity", schema, mnemonic);
   }
   Ok(())
}

fn parse_trace_rows(xml: &str, expected_schema: &str, expected_columns: &[&str]) -> Result<Vec<TraceRow>>
{
   let document = Document::parse(xml.trim()).with_context(|| format!("parsing {} System Trace XML", expected_schema))?;
   let mut observed_schema = false;
   let mut rows = Vec::new();
   for node in document.descendants().filter(|node| node.has_tag_name("node"))
   {
      let Some(schema) = node.children().find(|child| child.is_element() && child.has_tag_name("schema")) else {continue};
      let schema_name = schema.attribute("name").context("System Trace schema has no name")?;
      if schema_name != expected_schema
      {
         bail!("expected System Trace schema {}, observed {}", expected_schema, schema_name);
      }
      observed_schema = true;
      let columns = schema.children().filter(|child| child.is_element() && child.has_tag_name("col")).map(|column| {
         column.children().find(|child| child.is_element() && child.has_tag_name("mnemonic")).and_then(|mnemonic| mnemonic.text()).map(str::to_string).context("System Trace column has no mnemonic")
      }).collect::<Result<Vec<_>>>()?;
      if columns.iter().map(String::as_str).collect::<Vec<_>>() != expected_columns
      {
         bail!("unsupported Xcode System Trace {} column schema", expected_schema);
      }
      let mut references = BTreeMap::<String, TraceCell>::new();
      for row_node in node.children().filter(|child| child.is_element() && child.has_tag_name("row"))
      {
         let values = row_node.children().filter(|child| child.is_element()).collect::<Vec<_>>();
         if values.len() != columns.len()
         {
            bail!("System Trace {} row width differs from its schema", expected_schema);
         }
         let mut row = BTreeMap::new();
         for (mnemonic, value_node) in columns.iter().zip(values)
         {
            if value_node.has_tag_name("sentinel")
            {
               continue;
            }
            let cell = resolve_trace_cell(&value_node, &references)?;
            if let Some(identifier) = value_node.attribute("id")
            {
               references.insert(identifier.to_string(), cell.clone());
            }
            for descendant in value_node.descendants().filter(|descendant| descendant.is_element()).skip(1)
            {
               if let Some(identifier) = descendant.attribute("id")
               {
                  references.insert(identifier.to_string(), resolve_trace_cell(&descendant, &references)?);
               }
            }
            row.insert(mnemonic.clone(), cell);
         }
         rows.push(row);
      }
   }
   if !observed_schema
   {
      bail!("System Trace export has no {} schema", expected_schema);
   }
   if rows.is_empty()
   {
      bail!("System Trace export has no {} rows", expected_schema);
   }
   Ok(rows)
}

fn resolve_trace_cell(node: &roxmltree::Node<'_, '_>, references: &BTreeMap<String, TraceCell>) -> Result<TraceCell>
{
   let mut cell = TraceCell {
      raw: node.text().map(str::trim).filter(|value| !value.is_empty()).map(str::to_string),
      formatted: node.attribute("fmt").map(str::to_string),
      pid: descendant_u32(node, "pid")?,
      tid: descendant_u64(node, "tid")?,
   };
   if let Some(reference) = node.attribute("ref")
   {
      let referenced = references.get(reference).with_context(|| format!("unresolved System Trace reference {}", reference))?;
      if cell.raw.is_none() {cell.raw = referenced.raw.clone();}
      if cell.formatted.is_none() {cell.formatted = referenced.formatted.clone();}
      if cell.pid.is_none() {cell.pid = referenced.pid;}
      if cell.tid.is_none() {cell.tid = referenced.tid;}
   }
   Ok(cell)
}

fn descendant_u32(node: &roxmltree::Node<'_, '_>, tag: &str) -> Result<Option<u32>>
{
   descendant_u64(node, tag)?.map(|value| u32::try_from(value).context("System Trace nested PID exceeds u32")).transpose()
}

fn descendant_u64(node: &roxmltree::Node<'_, '_>, tag: &str) -> Result<Option<u64>>
{
   node.descendants().filter(|descendant| descendant.is_element() && descendant.has_tag_name(tag)).map(|descendant| {
      descendant.text().context("System Trace nested identity has no value")?.trim().parse::<u64>().context("parsing System Trace nested identity")
   }).next().transpose()
}

pub(crate) struct MacOsSystemTracePaths<'a>
{
   pub trace: &'a Path,
   pub scratch: &'a Path,
   pub toc: &'a Path,
   pub signposts: &'a Path,
   pub thread_info: &'a Path,
   pub thread_state: &'a Path,
   pub context_switch: &'a Path,
   pub summary: &'a Path,
}

struct SystemTraceScratch
{
   path: PathBuf,
}

impl SystemTraceScratch
{
   fn create(path: &Path) -> Result<Self>
   {
      if path.exists()
      {
         bail!("refusing to reuse macOS System Trace scratch directory {}", path.display());
      }
      fs::create_dir(path).with_context(|| format!("creating isolated macOS System Trace scratch directory {}", path.display()))?;
      Ok(Self {path: path.to_path_buf()})
   }

   fn cleanup(&mut self) -> Result<()>
   {
      if self.path.exists()
      {
         fs::remove_dir_all(&self.path).with_context(|| format!("removing isolated macOS System Trace scratch directory {}", self.path.display()))?;
      }
      Ok(())
   }
}

impl Drop for SystemTraceScratch
{
   fn drop(&mut self)
   {
      let _ = self.cleanup();
   }
}

pub(crate) struct MacOsSystemTraceCollector
{
   child: Child,
   process_group: u32,
   trace_path: PathBuf,
   scratch: SystemTraceScratch,
   stop_monitor: Arc<AtomicBool>,
   fuse_state: Arc<AtomicU8>,
   monitor: Option<JoinHandle<()>>,
   process_group_active: bool,
   retain_trace: bool,
   pid: u32,
}

impl MacOsSystemTraceCollector
{
   pub(crate) fn start(paths: &MacOsSystemTracePaths<'_>, pid: u32, occupied_seconds: u64, notification: &str, stdout: &File, stderr: &File) -> Result<Self>
   {
      for path in [paths.trace, paths.toc, paths.signposts, paths.thread_info, paths.thread_state, paths.context_switch, paths.summary]
      {
         if path.exists()
         {
            bail!("refusing to overwrite macOS System Trace evidence {}", path.display());
         }
      }
      let scratch = SystemTraceScratch::create(paths.scratch)?;
      let mut command = native_xcrun_command();
      command
         .env("TMPDIR", &scratch.path)
         .env("TMP", &scratch.path)
         .env("TEMP", &scratch.path)
         .process_group(0)
         .args(["xctrace", "record", "--template", "System Trace", "--notify-tracing-started", notification, "--output"])
         .arg(paths.trace)
         .args(["--time-limit", &format!("{}s", occupied_seconds.max(1)), "--no-prompt", "--attach", &pid.to_string()])
         .stdout(Stdio::from(stdout.try_clone().context("cloning System Trace stdout")?))
         .stderr(Stdio::from(stderr.try_clone().context("cloning System Trace stderr")?));
      let child = command.spawn().context("attaching standalone macOS System Trace to exact process")?;
      let process_group = child.id();
      let stop_monitor = Arc::new(AtomicBool::new(false));
      let fuse_state = Arc::new(AtomicU8::new(FUSE_OK));
      let monitor = Some(spawn_fuse_monitor(process_group, paths.trace.to_path_buf(), scratch.path.clone(), Arc::clone(&stop_monitor), Arc::clone(&fuse_state)));
      Ok(Self {
         child,
         process_group,
         trace_path: paths.trace.to_path_buf(),
         scratch,
         stop_monitor,
         fuse_state,
         monitor,
         process_group_active: true,
         retain_trace: false,
         pid,
      })
   }

   pub(crate) fn wait_for_started(&mut self, waiter: &mut Child, timeout: Duration) -> Result<()>
   {
      let deadline = Instant::now().checked_add(timeout).context("System Trace start timeout overflow")?;
      loop
      {
         self.enforce()?;
         if let Some(status) = self.child.try_wait().context("checking System Trace recorder before start")?
         {
            bail!("macOS System Trace recorder exited before its tracing-started notification with {}", status);
         }
         if let Some(status) = waiter.try_wait().context("checking System Trace started waiter")?
         {
            if !status.success()
            {
               bail!("macOS System Trace started waiter exited with {}", status);
            }
            return Ok(());
         }
         if Instant::now() >= deadline
         {
            bail!("macOS System Trace did not report tracing started within {} seconds", timeout.as_secs());
         }
         thread::sleep(Duration::from_millis(50));
      }
   }

   pub(crate) fn enforce(&mut self) -> Result<()>
   {
      match self.fuse_state.load(Ordering::Acquire)
      {
         FUSE_OK => (),
         FUSE_LIMIT_EXCEEDED => bail!("macOS System Trace exceeded its 512 MiB bundle-plus-scratch limit"),
         FUSE_MEASUREMENT_FAILED => bail!("macOS System Trace was terminated because its working set could not be measured safely"),
         state => bail!("macOS System Trace entered unknown storage-fuse state {}", state),
      }
      let bytes = trace_working_set_bytes(&self.trace_path, &self.scratch.path)?;
      if bytes > SYSTEM_TRACE_WORKING_SET_LIMIT_BYTES
      {
         self.trip_fuse(FUSE_LIMIT_EXCEEDED);
         bail!("macOS System Trace reached {} bytes, exceeding its 512 MiB bundle-plus-scratch limit", bytes);
      }
      Ok(())
   }

   pub(crate) fn finish(mut self, paths: &MacOsSystemTracePaths<'_>) -> Result<MacOsSystemTraceSummary>
   {
      self.enforce()?;
      if self.child.try_wait().context("checking System Trace before interrupt")?.is_none()
      {
         signal_process(self.child.id(), "-INT").context("interrupting System Trace for graceful finalization")?;
      }
      let deadline = Instant::now() + Duration::from_secs(60);
      let status = loop
      {
         self.enforce()?;
         if let Some(status) = self.child.try_wait().context("polling System Trace finalization")?
         {
            break status;
         }
         if Instant::now() >= deadline
         {
            self.terminate_group();
            bail!("macOS System Trace did not finalize within 60 seconds");
         }
         thread::sleep(Duration::from_millis(100));
      };
      if !status.success()
      {
         bail!("macOS System Trace exited with {}", status);
      }
      self.stop_monitor();
      self.enforce()?;
      self.terminate_group();
      validate_macos_presentation_trace_bundle(paths.trace)?;
      export_system_trace_xml(paths.trace, &["--toc"], paths.toc, &self.scratch.path)?;
      export_system_trace_table(paths.trace, "os-signpost", paths.signposts, &self.scratch.path)?;
      export_system_trace_table(paths.trace, "thread-info", paths.thread_info, &self.scratch.path)?;
      export_system_trace_table(paths.trace, "thread-state", paths.thread_state, &self.scratch.path)?;
      export_system_trace_table(paths.trace, "context-switch", paths.context_switch, &self.scratch.path)?;
      let toc = fs::read_to_string(paths.toc).with_context(|| format!("reading {}", paths.toc.display()))?;
      validate_system_trace_toc(&toc)?;
      let summary = reduce_macos_system_trace(
         &fs::read_to_string(paths.signposts).with_context(|| format!("reading {}", paths.signposts.display()))?,
         &fs::read_to_string(paths.thread_info).with_context(|| format!("reading {}", paths.thread_info.display()))?,
         &fs::read_to_string(paths.thread_state).with_context(|| format!("reading {}", paths.thread_state.display()))?,
         &fs::read_to_string(paths.context_switch).with_context(|| format!("reading {}", paths.context_switch.display()))?,
         self.pid,
      )?;
      durable_json(&summary, paths.summary)?;
      self.scratch.cleanup()?;
      self.retain_trace = true;
      Ok(summary)
   }

   fn trip_fuse(&mut self, state: u8)
   {
      self.fuse_state.store(state, Ordering::Release);
      self.terminate_group();
   }

   fn stop_monitor(&mut self)
   {
      self.stop_monitor.store(true, Ordering::Release);
      if let Some(monitor) = self.monitor.take()
      {
         let _ = monitor.join();
      }
   }

   fn terminate_group(&mut self)
   {
      if self.process_group_active
      {
         terminate_process_group(self.process_group);
         self.process_group_active = false;
      }
   }
}

impl Deref for MacOsSystemTraceCollector
{
   type Target = Child;

   fn deref(&self) -> &Self::Target
   {
      &self.child
   }
}

impl DerefMut for MacOsSystemTraceCollector
{
   fn deref_mut(&mut self) -> &mut Self::Target
   {
      &mut self.child
   }
}

impl Drop for MacOsSystemTraceCollector
{
   fn drop(&mut self)
   {
      self.stop_monitor();
      self.terminate_group();
      terminate_child(&mut self.child);
      if !self.retain_trace && self.trace_path.exists()
      {
         let _ = fs::remove_dir_all(&self.trace_path);
      }
   }
}

fn export_system_trace_table(trace: &Path, schema: &str, output: &Path, scratch: &Path) -> Result<()>
{
   let xpath = format!("/trace-toc/run[@number=\"1\"]/data/table[@schema=\"{}\"]", schema);
   export_system_trace_xml(trace, &["--xpath", &xpath], output, scratch)
}

fn export_system_trace_xml(trace: &Path, selector: &[&str], output: &Path, scratch: &Path) -> Result<()>
{
   if output.exists()
   {
      bail!("refusing to overwrite System Trace export {}", output.display());
   }
   let status = native_xcrun_command()
      .env("TMPDIR", scratch)
      .env("TMP", scratch)
      .env("TEMP", scratch)
      .args(["xctrace", "export", "--input"])
      .arg(trace)
      .args(selector)
      .args(["--output"])
      .arg(output)
      .status()
      .context("exporting macOS System Trace evidence")?;
   if !status.success()
   {
      bail!("System Trace export failed with {} for {}", status, trace.display());
   }
   Ok(())
}

fn validate_system_trace_toc(toc: &str) -> Result<()>
{
   let normalized = toc.to_ascii_lowercase();
   for schema in ["os-signpost", "thread-info", "thread-state", "context-switch"]
   {
      if !normalized.contains(&format!("schema=\"{}\"", schema))
      {
         bail!("macOS System Trace TOC has no {} schema", schema);
      }
   }
   Ok(())
}

fn spawn_fuse_monitor(process_group: u32, trace: PathBuf, scratch: PathBuf, stop: Arc<AtomicBool>, state: Arc<AtomicU8>) -> JoinHandle<()>
{
   thread::spawn(move || {
      while !stop.load(Ordering::Acquire)
      {
         match trace_working_set_bytes(&trace, &scratch)
         {
            Ok(bytes) if bytes <= SYSTEM_TRACE_WORKING_SET_LIMIT_BYTES => (),
            Ok(_) =>
            {
               state.store(FUSE_LIMIT_EXCEEDED, Ordering::Release);
               terminate_process_group(process_group);
               return;
            }
            Err(_) =>
            {
               state.store(FUSE_MEASUREMENT_FAILED, Ordering::Release);
               terminate_process_group(process_group);
               return;
            }
         }
         thread::sleep(Duration::from_millis(100));
      }
   })
}

fn trace_working_set_bytes(trace: &Path, scratch: &Path) -> Result<u64>
{
   path_bytes_if_present(trace)?.checked_add(path_bytes_if_present(scratch)?).context("System Trace working-set byte count overflow")
}

fn path_bytes_if_present(path: &Path) -> Result<u64>
{
   if !path.exists()
   {
      return Ok(0);
   }
   let metadata = fs::symlink_metadata(path).with_context(|| format!("reading metadata for {}", path.display()))?;
   if metadata.file_type().is_symlink()
   {
      bail!("System Trace working set contains unsupported symlink {}", path.display());
   }
   if metadata.is_file()
   {
      return Ok(metadata.len());
   }
   if !metadata.is_dir()
   {
      bail!("System Trace working-set entry is neither file nor directory: {}", path.display());
   }
   let mut total = 0_u64;
   for entry in fs::read_dir(path).with_context(|| format!("reading {}", path.display()))?
   {
      total = total.checked_add(path_bytes_if_present(&entry?.path())?).context("System Trace working-set byte count overflow")?;
   }
   Ok(total)
}

fn signal_process(pid: u32, signal: &str) -> Result<ExitStatus>
{
   let status = Command::new("/bin/kill").args([signal, &pid.to_string()]).status().context("signaling System Trace process")?;
   if !status.success()
   {
      bail!("signaling System Trace process {} with {} failed with {}", pid, signal, status);
   }
   Ok(status)
}

fn terminate_process_group(process_group: u32)
{
   let group = format!("-{}", process_group);
   let _ = Command::new("/bin/kill").args(["-TERM", "--", &group]).status();
   thread::sleep(Duration::from_millis(25));
   let _ = Command::new("/bin/kill").args(["-KILL", "--", &group]).status();
}
