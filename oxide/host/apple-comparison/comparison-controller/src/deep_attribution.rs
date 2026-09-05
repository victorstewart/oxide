use anyhow::{bail, Context, Result};
use oxide_benchmark_spec::{balanced_comparison_order, canonical_macos_full_attribution_plan_json, comparison_seed_from_content_sha256, validate_macos_full_attribution_plan, ComparisonOrder, MacOsAttributionCollectorKind, MacOsAttributionReplaySpec, MacOsFullAttributionPlan};
use roxmltree::Document;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::ops::{Deref, DerefMut};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::trace::{cell_display, parse_message_field, parse_trace_rows, required_display, required_u64, TraceRow};
use super::{durable_json, native_xcrun_command, terminate_child, validate_macos_presentation_trace_bundle, ComparisonSide};

pub const MACOS_DEEP_ATTRIBUTION_WORKING_SET_LIMIT_BYTES: u64 = 512 * 1_024 * 1_024;
pub const MACOS_DEEP_ATTRIBUTION_CROSS_FRAMEWORK_SCOPE: &str = "descriptive-implementation-attribution-not-cross-framework-comparable";

const FUSE_OK: u8 = 0;
const FUSE_LIMIT_EXCEEDED: u8 = 1;
const FUSE_MEASUREMENT_FAILED: u8 = 2;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsDeepAttributionSession
{
   pub replay_id: String,
   pub collector: MacOsAttributionCollectorKind,
   pub configuration_id: Option<String>,
   pub pair_index: u32,
   pub order: ComparisonOrder,
   pub side: ComparisonSide,
   pub scenario_ids: Vec<String>,
   pub occupied_seconds: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsDeepAttributionSchedule
{
   pub schema_version: u32,
   pub plan_sha256: String,
   pub sessions: Vec<MacOsDeepAttributionSession>,
   pub unavailable_gpu_counter_configurations: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct MacOsDeepAttributionPaths
{
   pub directory: PathBuf,
   pub trace: PathBuf,
   pub scratch: PathBuf,
   pub toc: PathBuf,
   pub signposts: PathBuf,
   pub summary: PathBuf,
   pub receipt: PathBuf,
}

impl MacOsDeepAttributionPaths
{
   pub fn new(root: &Path, replay_id: &str, pair_index: u32, side: &str) -> Result<Self>
   {
      validate_component(replay_id, "replay id")?;
      validate_component(side, "comparison side")?;
      let directory = root.join(format!("{}.pair-{}.{}", replay_id, pair_index, side));
      Ok(Self {
         trace: directory.join("capture.trace"),
         scratch: root.join(format!(".{}.pair-{}.{}.xctrace-tmp", replay_id, pair_index, side)),
         toc: directory.join("toc.xml"),
         signposts: directory.join("signposts.xml"),
         summary: directory.join("summary.json"),
         receipt: directory.join("complete.json"),
         directory,
      })
   }
}

pub fn build_macos_deep_attribution_schedule(plan: &MacOsFullAttributionPlan) -> Result<MacOsDeepAttributionSchedule>
{
   validate_macos_full_attribution_plan(plan)?;
   let plan_bytes = canonical_macos_full_attribution_plan_json(plan)?;
   let plan_sha256 = format!("{:x}", Sha256::digest(&plan_bytes));
   let seed = comparison_seed_from_content_sha256(&plan_sha256)?;
   let occupied_seconds = plan.replays.first().context("macOS full-attribution plan has no replay")?.reset_seconds_per_session
      .checked_add(
         (plan.scenario_ids.len() as u64).checked_mul(
            plan.replays[0].setup_seconds_per_scenario
               .checked_add(plan.replays[0].warmup_seconds_per_scenario)
               .and_then(|seconds| seconds.checked_add(plan.replays[0].measurement_seconds_per_scenario))
               .context("macOS deep-attribution scenario-time overflow")?,
         ).context("macOS deep-attribution session-time overflow")?,
      ).context("macOS deep-attribution session-time overflow")?;
   let mut sessions = Vec::new();
   for (replay_index, replay) in plan.replays.iter().enumerate()
   {
      for pair_index in 0..replay.pair_count
      {
         let order_index = replay_index.checked_mul(replay.pair_count as usize).and_then(|index| index.checked_add(pair_index as usize)).context("macOS deep-attribution order index overflow")?;
         let order = balanced_comparison_order(seed.0, order_index + 1)[order_index];
         let sides = if order == ComparisonOrder::Ab {[ComparisonSide::Native, ComparisonSide::Oxide]} else {[ComparisonSide::Oxide, ComparisonSide::Native]};
         for side in sides
         {
            sessions.push(MacOsDeepAttributionSession {
               replay_id: replay.id.clone(),
               collector: replay.collector,
               configuration_id: replay.configuration_id.clone(),
               pair_index,
               order,
               side,
               scenario_ids: replay.scenario_ids.clone(),
               occupied_seconds,
            });
         }
      }
   }
   let unavailable_gpu_counter_configurations = plan.gpu_counter_configurations.iter()
      .filter(|configuration| configuration.availability != oxide_benchmark_spec::MacOsAttributionAvailability::Available)
      .map(|configuration| format!("{}:{:?}:{}", configuration.id, configuration.availability, configuration.unavailable_reason.as_deref().unwrap_or("missing-reason")))
      .collect();
   Ok(MacOsDeepAttributionSchedule {schema_version: 1, plan_sha256, sessions, unavailable_gpu_counter_configurations})
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsDeepAttributionMetricSummary
{
   pub mnemonic: String,
   #[serde(default, skip_serializing_if = "Option::is_none")]
   pub unit: Option<String>,
   pub sample_count: u64,
   pub minimum: u64,
   pub maximum: u64,
   pub sum: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsDeepAttributionSchemaPhase
{
   pub scenario_index: u64,
   pub phase_identifier: u64,
   pub begin_ns: u64,
   pub end_ns: u64,
   pub measured_row_count: u64,
   pub exact_pid_row_count: u64,
   pub metrics: Vec<MacOsDeepAttributionMetricSummary>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsDeepAttributionSchemaSummary
{
   pub schema: String,
   pub row_count: u64,
   pub timestamp_mnemonic: String,
   pub process_mnemonic: Option<String>,
   pub exact_pid_filter_exposed: bool,
   pub phases: Vec<MacOsDeepAttributionSchemaPhase>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsDeepAttributionSummary
{
   pub schema_version: u32,
   pub replay_id: String,
   pub collector: MacOsAttributionCollectorKind,
   pub configuration_id: Option<String>,
   pub pid: u32,
   pub attached_to_exact_pid: bool,
   pub cross_framework_scope: String,
   pub schemas: Vec<MacOsDeepAttributionSchemaSummary>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsDeepAttributionReceipt
{
   pub schema_version: u32,
   pub replay_id: String,
   pub collector: MacOsAttributionCollectorKind,
   pub configuration_id: Option<String>,
   pub pid: u32,
   pub trace_bundle_sha256: String,
   pub toc_sha256: String,
   pub signposts_sha256: String,
   pub summary_sha256: String,
   pub isolated_replay: bool,
   pub profiler_combination: String,
   pub cross_framework_scope: String,
   pub complete: bool,
}

#[derive(Clone, Debug)]
pub struct MacOsDeepAttributionSchemaExport
{
   pub schema: String,
   pub xml: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MeasuredWindow
{
   scenario_index: u64,
   phase_identifier: u64,
   begin_ns: u64,
   end_ns: u64,
}

#[derive(Clone, Debug)]
struct MetricAccumulator
{
   unit: Option<String>,
   count: u64,
   minimum: u64,
   maximum: u64,
   sum: u64,
}

struct AttributionScratch
{
   path: PathBuf,
}

pub struct MacOsDeepAttributionCollector
{
   child: Child,
   process_group: u32,
   process_group_active: bool,
   watched_directory: PathBuf,
   scratch: AttributionScratch,
   stop_monitor: Arc<AtomicBool>,
   fuse_state: Arc<AtomicU8>,
   monitor: Option<JoinHandle<()>>,
   retain_trace: bool,
   replay: MacOsAttributionReplaySpec,
   pid: u32,
}

impl MacOsDeepAttributionCollector
{
   pub fn start(replay: &MacOsAttributionReplaySpec, paths: &MacOsDeepAttributionPaths, pid: u32, occupied_seconds: u64, notification: &str, stdout: &File, stderr: &File) -> Result<Self>
   {
      validate_replay(replay)?;
      if pid == 0 || occupied_seconds == 0 || notification.trim().is_empty()
      {
         bail!("macOS deep-attribution start requires nonzero PID/time and a tracing-started notification");
      }
      if paths.directory.exists() || paths.scratch.exists()
      {
         bail!("refusing to overwrite or merge macOS deep-attribution evidence {}", paths.directory.display());
      }
      fs::create_dir(&paths.directory).with_context(|| format!("creating macOS deep-attribution directory {}", paths.directory.display()))?;
      let scratch = AttributionScratch::create(&paths.scratch)?;
      let mut command = native_xcrun_command();
      command
         .env("TMPDIR", &scratch.path)
         .env("TMP", &scratch.path)
         .env("TEMP", &scratch.path)
         .process_group(0)
         .arg("xctrace")
         .arg("record");
      match (&replay.selector.template, &replay.selector.instrument)
      {
         (Some(template), None) => {command.args(["--template", template]);}
         (None, Some(instrument)) => {command.args(["--instrument", instrument]);}
         _ => bail!("macOS deep-attribution replay has an invalid trace selector"),
      }
      command
         .args(["--notify-tracing-started", notification, "--output"])
         .arg(&paths.trace)
         .args(["--time-limit", &format!("{}s", occupied_seconds), "--no-prompt", "--attach", &pid.to_string()])
         .stdout(Stdio::from(stdout.try_clone().context("cloning deep-attribution stdout")?))
         .stderr(Stdio::from(stderr.try_clone().context("cloning deep-attribution stderr")?));
      let child = command.spawn().context("attaching isolated macOS deep-attribution recorder to exact PID")?;
      let process_group = child.id();
      let stop_monitor = Arc::new(AtomicBool::new(false));
      let fuse_state = Arc::new(AtomicU8::new(FUSE_OK));
      let monitor = Some(spawn_fuse_monitor(
         process_group,
         paths.directory.clone(),
         scratch.path.clone(),
         Arc::clone(&stop_monitor),
         Arc::clone(&fuse_state),
      ));
      Ok(Self {
         child,
         process_group,
         process_group_active: true,
         watched_directory: paths.directory.clone(),
         scratch,
         stop_monitor,
         fuse_state,
         monitor,
         retain_trace: false,
         replay: replay.clone(),
         pid,
      })
   }

   pub fn wait_for_started(&mut self, waiter: &mut Child, timeout: Duration) -> Result<()>
   {
      let deadline = Instant::now().checked_add(timeout).context("deep-attribution start timeout overflow")?;
      loop
      {
         self.enforce()?;
         if let Some(status) = self.child.try_wait().context("checking deep-attribution recorder before start")?
         {
            bail!("macOS deep-attribution recorder exited before its tracing-started notification with {}", status);
         }
         if let Some(status) = waiter.try_wait().context("checking deep-attribution started waiter")?
         {
            if !status.success()
            {
               bail!("macOS deep-attribution started waiter exited with {}", status);
            }
            return Ok(());
         }
         if Instant::now() >= deadline
         {
            bail!("macOS deep-attribution did not report tracing started within {} seconds", timeout.as_secs());
         }
         thread::sleep(Duration::from_millis(50));
      }
   }

   pub fn enforce(&mut self) -> Result<()>
   {
      match self.fuse_state.load(Ordering::Acquire)
      {
         FUSE_OK => (),
         FUSE_LIMIT_EXCEEDED => bail!("macOS deep-attribution exceeded its 512 MiB artifact-plus-scratch limit"),
         FUSE_MEASUREMENT_FAILED => bail!("macOS deep-attribution was terminated because its working set could not be measured safely"),
         state => bail!("macOS deep-attribution entered unknown storage-fuse state {}", state),
      }
      let bytes = working_set_bytes(&self.watched_directory, &self.scratch.path)?;
      if bytes > MACOS_DEEP_ATTRIBUTION_WORKING_SET_LIMIT_BYTES
      {
         self.trip_fuse(FUSE_LIMIT_EXCEEDED);
         bail!("macOS deep-attribution reached {} bytes, exceeding its 512 MiB limit", bytes);
      }
      Ok(())
   }

   pub fn finish(mut self, paths: &MacOsDeepAttributionPaths) -> Result<MacOsDeepAttributionReceipt>
   {
      self.enforce()?;
      if self.child.try_wait().context("checking deep-attribution recorder before interrupt")?.is_none()
      {
         signal_process(self.child.id(), "-INT").context("interrupting deep-attribution recorder for graceful finalization")?;
      }
      let deadline = Instant::now() + Duration::from_secs(60);
      let status = loop
      {
         self.enforce()?;
         if let Some(status) = self.child.try_wait().context("polling deep-attribution finalization")?
         {
            break status;
         }
         if Instant::now() >= deadline
         {
            self.terminate_group();
            bail!("macOS deep-attribution recorder did not finalize within 60 seconds");
         }
         thread::sleep(Duration::from_millis(100));
      };
      if !status.success()
      {
         bail!("macOS deep-attribution recorder exited with {}", status);
      }
      self.stop_monitor();
      self.enforce()?;
      self.terminate_group();
      validate_macos_presentation_trace_bundle(&paths.trace)?;

      let toc = export_trace(&paths.trace, &["--toc"], &self.scratch.path, "toc.xml")?;
      let signposts = export_schema(&paths.trace, "os-signpost", &self.scratch.path, 0)?;
      atomic_bytes(toc.as_bytes(), &paths.toc)?;
      atomic_bytes(signposts.as_bytes(), &paths.signposts)?;
      let schemas = trace_schemas(&toc)?;
      let mut exports = Vec::new();
      for (index, schema) in schemas.into_iter().filter(|schema| schema != "os-signpost").enumerate()
      {
         let xml = export_schema(&paths.trace, &schema, &self.scratch.path, index + 1)?;
         exports.push(MacOsDeepAttributionSchemaExport {schema, xml});
         self.enforce()?;
      }
      let summary = reduce_macos_deep_attribution_exports(&self.replay, self.pid, &signposts, &exports)?;
      durable_json(&summary, &paths.summary)?;
      self.enforce()?;
      let receipt = MacOsDeepAttributionReceipt {
         schema_version: 1,
         replay_id: self.replay.id.clone(),
         collector: self.replay.collector,
         configuration_id: self.replay.configuration_id.clone(),
         pid: self.pid,
         trace_bundle_sha256: sha256_tree(&paths.trace)?,
         toc_sha256: sha256_file(&paths.toc)?,
         signposts_sha256: sha256_file(&paths.signposts)?,
         summary_sha256: sha256_file(&paths.summary)?,
         isolated_replay: true,
         profiler_combination: String::from("none-isolated-replay"),
         cross_framework_scope: String::from(MACOS_DEEP_ATTRIBUTION_CROSS_FRAMEWORK_SCOPE),
         complete: true,
      };
      durable_json(&receipt, &paths.receipt)?;
      self.scratch.cleanup()?;
      self.retain_trace = true;
      Ok(receipt)
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

impl Deref for MacOsDeepAttributionCollector
{
   type Target = Child;

   fn deref(&self) -> &Self::Target
   {
      &self.child
   }
}

impl DerefMut for MacOsDeepAttributionCollector
{
   fn deref_mut(&mut self) -> &mut Self::Target
   {
      &mut self.child
   }
}

impl Drop for MacOsDeepAttributionCollector
{
   fn drop(&mut self)
   {
      self.stop_monitor();
      self.terminate_group();
      terminate_child(&mut self.child);
      if !self.retain_trace && self.watched_directory.exists()
      {
         let _ = fs::remove_dir_all(&self.watched_directory);
      }
   }
}

impl AttributionScratch
{
   fn create(path: &Path) -> Result<Self>
   {
      if path.exists()
      {
         bail!("refusing to reuse macOS deep-attribution scratch directory {}", path.display());
      }
      fs::create_dir(path).with_context(|| format!("creating isolated macOS deep-attribution scratch directory {}", path.display()))?;
      Ok(Self {path: path.to_path_buf()})
   }

   fn cleanup(&mut self) -> Result<()>
   {
      if self.path.exists()
      {
         fs::remove_dir_all(&self.path).with_context(|| format!("removing macOS deep-attribution scratch directory {}", self.path.display()))?;
      }
      Ok(())
   }
}

impl Drop for AttributionScratch
{
   fn drop(&mut self)
   {
      let _ = self.cleanup();
   }
}

pub fn reduce_macos_deep_attribution_exports(replay: &MacOsAttributionReplaySpec, pid: u32, signposts_xml: &str, exports: &[MacOsDeepAttributionSchemaExport]) -> Result<MacOsDeepAttributionSummary>
{
   validate_replay(replay)?;
   if pid == 0 || exports.is_empty()
   {
      bail!("macOS deep-attribution reduction requires a nonzero exact PID and at least one exported schema");
   }
   let windows = measured_windows(signposts_xml, pid)?;
   let mut observed = BTreeSet::new();
   let mut schemas = Vec::with_capacity(exports.len());
   for export in exports
   {
      if export.schema == "os-signpost" || !observed.insert(export.schema.as_str())
      {
         bail!("macOS deep-attribution export schema {} is reserved or duplicated", export.schema);
      }
      schemas.push(reduce_schema(export, pid, &windows)?);
   }
   schemas.sort_by(|left, right| left.schema.cmp(&right.schema));
   Ok(MacOsDeepAttributionSummary {
      schema_version: 1,
      replay_id: replay.id.clone(),
      collector: replay.collector,
      configuration_id: replay.configuration_id.clone(),
      pid,
      attached_to_exact_pid: true,
      cross_framework_scope: String::from(MACOS_DEEP_ATTRIBUTION_CROSS_FRAMEWORK_SCOPE),
      schemas,
   })
}

pub fn validate_macos_deep_attribution_receipt(paths: &MacOsDeepAttributionPaths, replay: &MacOsAttributionReplaySpec, pid: u32) -> Result<MacOsDeepAttributionReceipt>
{
   let bytes = fs::read(&paths.receipt).with_context(|| format!("reading macOS deep-attribution receipt {}", paths.receipt.display()))?;
   let receipt: MacOsDeepAttributionReceipt = serde_json::from_slice(&bytes).with_context(|| format!("decoding macOS deep-attribution receipt {}", paths.receipt.display()))?;
   if receipt.schema_version != 1
      || receipt.replay_id != replay.id
      || receipt.collector != replay.collector
      || receipt.configuration_id != replay.configuration_id
      || receipt.pid != pid
      || pid == 0
      || !receipt.isolated_replay
      || receipt.profiler_combination != "none-isolated-replay"
      || receipt.cross_framework_scope != MACOS_DEEP_ATTRIBUTION_CROSS_FRAMEWORK_SCOPE
      || !receipt.complete
   {
      bail!("macOS deep-attribution receipt identity or isolation contract differs from the requested replay");
   }
   validate_macos_presentation_trace_bundle(&paths.trace)?;
   let expected = [
      (&receipt.trace_bundle_sha256, sha256_tree(&paths.trace)?),
      (&receipt.toc_sha256, sha256_file(&paths.toc)?),
      (&receipt.signposts_sha256, sha256_file(&paths.signposts)?),
      (&receipt.summary_sha256, sha256_file(&paths.summary)?),
   ];
   if expected.iter().any(|(recorded, observed)| recorded.as_str() != observed)
   {
      bail!("macOS deep-attribution receipt artifact SHA-256 differs from durable evidence");
   }
   let summary: MacOsDeepAttributionSummary = serde_json::from_slice(&fs::read(&paths.summary).with_context(|| format!("reading {}", paths.summary.display()))?).with_context(|| format!("decoding {}", paths.summary.display()))?;
   if summary.replay_id != replay.id || summary.collector != replay.collector || summary.configuration_id != replay.configuration_id || summary.pid != pid || !summary.attached_to_exact_pid || summary.cross_framework_scope != MACOS_DEEP_ATTRIBUTION_CROSS_FRAMEWORK_SCOPE
   {
      bail!("macOS deep-attribution summary identity differs from its receipt");
   }
   Ok(receipt)
}

fn reduce_schema(export: &MacOsDeepAttributionSchemaExport, pid: u32, windows: &[MeasuredWindow]) -> Result<MacOsDeepAttributionSchemaSummary>
{
   let rows = parse_trace_rows(&export.xml, &export.schema)?;
   let timestamp_mnemonic = ["time", "start", "timestamp"].into_iter().find(|mnemonic| rows.iter().any(|row| row.get(*mnemonic).and_then(|cell| cell.raw_u64()).is_some())).with_context(|| format!("macOS deep-attribution schema {} exposes no numeric time/start/timestamp column", export.schema))?;
   let process_mnemonic = ["process", "target-process", "application"].into_iter().find(|mnemonic| rows.iter().any(|row| cell_display(row, mnemonic).is_some())).map(String::from);
   let suffix = format!(" ({})", pid);
   let units = schema_units(&export.xml, &export.schema)?;
   let mut phases = Vec::with_capacity(windows.len());
   for window in windows
   {
      let measured_rows = rows.iter().filter(|row| {
         let Some(time) = row.get(timestamp_mnemonic).and_then(|cell| cell.raw_u64()) else {return false};
         time >= window.begin_ns && time < window.end_ns
      }).collect::<Vec<_>>();
      let exact_pid_row_count = process_mnemonic.as_deref().map(|mnemonic| measured_rows.iter().filter(|row| cell_display(row, mnemonic).is_some_and(|process| process.ends_with(&suffix))).count() as u64).unwrap_or(0);
      let selected = if let Some(mnemonic) = process_mnemonic.as_deref()
      {
         measured_rows.into_iter().filter(|row| cell_display(row, mnemonic).is_some_and(|process| process.ends_with(&suffix))).collect::<Vec<_>>()
      }
      else
      {
         measured_rows
      };
      let metrics = reduce_numeric_metrics(&selected, timestamp_mnemonic, process_mnemonic.as_deref(), &units)?;
      phases.push(MacOsDeepAttributionSchemaPhase {
         scenario_index: window.scenario_index,
         phase_identifier: window.phase_identifier,
         begin_ns: window.begin_ns,
         end_ns: window.end_ns,
         measured_row_count: selected.len() as u64,
         exact_pid_row_count,
         metrics,
      });
   }
   Ok(MacOsDeepAttributionSchemaSummary {
      schema: export.schema.clone(),
      row_count: rows.len() as u64,
      timestamp_mnemonic: String::from(timestamp_mnemonic),
      exact_pid_filter_exposed: process_mnemonic.is_some(),
      process_mnemonic,
      phases,
   })
}

fn reduce_numeric_metrics(rows: &[&TraceRow], timestamp_mnemonic: &str, process_mnemonic: Option<&str>, units: &BTreeMap<String, Option<String>>) -> Result<Vec<MacOsDeepAttributionMetricSummary>>
{
   let mut accumulators = BTreeMap::<String, MetricAccumulator>::new();
   for row in rows
   {
      for (mnemonic, cell) in row.iter()
   {
         if mnemonic == timestamp_mnemonic || process_mnemonic == Some(mnemonic.as_str()) || matches!(mnemonic.as_str(), "pid" | "tid" | "identifier" | "scenario")
         {
            continue;
         }
         let Some(value) = cell.raw_u64() else {continue};
         let accumulator = accumulators.entry(mnemonic.clone()).or_insert(MetricAccumulator {
            unit: units.get(mnemonic).cloned().flatten(),
            count: 0,
            minimum: value,
            maximum: value,
            sum: 0,
         });
         accumulator.count = accumulator.count.checked_add(1).context("macOS deep-attribution metric sample-count overflow")?;
         accumulator.minimum = accumulator.minimum.min(value);
         accumulator.maximum = accumulator.maximum.max(value);
         accumulator.sum = accumulator.sum.checked_add(value).with_context(|| format!("macOS deep-attribution metric {} sum overflow", mnemonic))?;
      }
   }
   Ok(accumulators.into_iter().map(|(mnemonic, accumulator)| MacOsDeepAttributionMetricSummary {
      mnemonic,
      unit: accumulator.unit,
      sample_count: accumulator.count,
      minimum: accumulator.minimum,
      maximum: accumulator.maximum,
      sum: accumulator.sum,
   }).collect())
}

fn measured_windows(signposts_xml: &str, pid: u32) -> Result<Vec<MeasuredWindow>>
{
   let rows = parse_trace_rows(signposts_xml, "os-signpost")?;
   let suffix = format!(" ({})", pid);
   let mut open = BTreeMap::<(u64, u64), u64>::new();
   let mut windows = Vec::new();
   for row in rows
   {
      if cell_display(&row, "subsystem") != Some("com.oxide.comparison")
         || cell_display(&row, "category") != Some("Presentation")
         || cell_display(&row, "event-type") != Some("Event")
         || !cell_display(&row, "process").is_some_and(|process| process.ends_with(&suffix))
      {
         continue;
      }
      let name = required_display(&row, "name", "os-signpost")?;
      if name != "PhaseBegin" && name != "PhaseEnd"
      {
         continue;
      }
      let message = required_display(&row, "message", "os-signpost")?;
      if parse_message_field(message, "measured")? != 1
      {
         continue;
      }
      let key = (parse_message_field(message, "scenario")?, parse_message_field(message, "identifier")?);
      let time_ns = required_u64(&row, "time", "os-signpost")?;
      if name == "PhaseBegin"
      {
         if open.insert(key, time_ns).is_some()
         {
            bail!("duplicate macOS deep-attribution PhaseBegin for scenario {} phase {}", key.0, key.1);
         }
      }
      else
      {
         let begin_ns = open.remove(&key).context("macOS deep-attribution PhaseEnd has no matching begin")?;
         if begin_ns >= time_ns
         {
            bail!("macOS deep-attribution measured phase has a non-positive interval");
         }
         windows.push(MeasuredWindow {scenario_index: key.0, phase_identifier: key.1, begin_ns, end_ns: time_ns});
      }
   }
   if !open.is_empty() || windows.is_empty()
   {
      bail!("macOS deep-attribution signposts have incomplete measured phase windows for exact PID {}", pid);
   }
   windows.sort_by_key(|window| window.begin_ns);
   if windows.windows(2).any(|pair| pair[0].end_ns > pair[1].begin_ns)
   {
      bail!("macOS deep-attribution measured phase windows overlap");
   }
   Ok(windows)
}

fn schema_units(xml: &str, expected_schema: &str) -> Result<BTreeMap<String, Option<String>>>
{
   let document = Document::parse(xml.trim()).with_context(|| format!("parsing {} schema descriptors", expected_schema))?;
   let schema = document.descendants().find(|node| node.has_tag_name("schema") && node.attribute("name") == Some(expected_schema)).with_context(|| format!("macOS deep-attribution export has no {} schema descriptor", expected_schema))?;
   let mut units = BTreeMap::new();
   for column in schema.children().filter(|node| node.is_element() && node.has_tag_name("col"))
   {
      let mnemonic = column.children().find(|node| node.is_element() && node.has_tag_name("mnemonic")).and_then(|node| node.text()).context("macOS deep-attribution schema column has no mnemonic")?;
      let unit = column.descendants().find(|node| node.is_element() && matches!(node.tag_name().name(), "unit" | "engineering-unit")).and_then(|node| node.text()).map(String::from);
      units.insert(String::from(mnemonic), unit);
   }
   Ok(units)
}

fn trace_schemas(toc: &str) -> Result<Vec<String>>
{
   let document = Document::parse(toc.trim()).context("parsing macOS deep-attribution trace TOC")?;
   let schemas = document.descendants().filter(|node| node.is_element() && node.has_tag_name("table")).filter_map(|node| node.attribute("schema")).map(String::from).collect::<BTreeSet<_>>();
   if !schemas.contains("os-signpost") || schemas.len() < 2
   {
      bail!("macOS deep-attribution TOC omits os-signpost or collector data schemas");
   }
   Ok(schemas.into_iter().collect())
}

fn export_schema(trace: &Path, schema: &str, scratch: &Path, index: usize) -> Result<String>
{
   let xpath = format!("/trace-toc/run[@number=\"1\"]/data/table[@schema=\"{}\"]", schema);
   export_trace(trace, &["--xpath", &xpath], scratch, &format!("export-{}.xml", index))
}

fn export_trace(trace: &Path, selector: &[&str], scratch: &Path, output_name: &str) -> Result<String>
{
   let output = scratch.join(output_name);
   if output.exists()
   {
      bail!("refusing to overwrite macOS deep-attribution export {}", output.display());
   }
   let status = native_xcrun_command()
      .env("TMPDIR", scratch)
      .env("TMP", scratch)
      .env("TEMP", scratch)
      .args(["xctrace", "export", "--input"])
      .arg(trace)
      .args(selector)
      .args(["--output"])
      .arg(&output)
      .status()
      .context("exporting macOS deep-attribution evidence")?;
   if !status.success()
   {
      bail!("macOS deep-attribution export failed with {} for {}", status, trace.display());
   }
   let xml = fs::read_to_string(&output).with_context(|| format!("reading macOS deep-attribution export {}", output.display()))?;
   fs::remove_file(&output).with_context(|| format!("removing macOS deep-attribution working export {}", output.display()))?;
   Ok(xml)
}

fn validate_replay(replay: &MacOsAttributionReplaySpec) -> Result<()>
{
   validate_component(&replay.id, "replay id")?;
   if replay.evidence_role != "descriptive-diagnostic" || replay.combination_calibration != "none-isolated-replay"
   {
      bail!("macOS deep-attribution replay must remain an isolated descriptive diagnostic");
   }
   match (&replay.selector.template, &replay.selector.instrument)
   {
      (Some(template), None) if !template.trim().is_empty() => (),
      (None, Some(instrument)) if !instrument.trim().is_empty() => (),
      _ => bail!("macOS deep-attribution replay must select exactly one template or instrument"),
   }
   if replay.collector == MacOsAttributionCollectorKind::GpuCounters && replay.configuration_id.as_ref().is_none_or(String::is_empty)
   {
      bail!("macOS GPU-counter replay has no explicit available configuration id");
   }
   if replay.collector != MacOsAttributionCollectorKind::GpuCounters && replay.configuration_id.is_some()
   {
      bail!("non-GPU macOS attribution replay declares a GPU-counter configuration");
   }
   Ok(())
}

fn validate_component(value: &str, label: &str) -> Result<()>
{
   if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_'))
   {
      bail!("macOS deep-attribution {} is not a path-safe lowercase identifier", label);
   }
   Ok(())
}

fn atomic_bytes(bytes: &[u8], destination: &Path) -> Result<()>
{
   if destination.exists()
   {
      bail!("refusing to overwrite macOS deep-attribution artifact {}", destination.display());
   }
   let name = destination.file_name().and_then(|name| name.to_str()).context("macOS deep-attribution destination has no UTF-8 file name")?;
   let temporary = destination.with_file_name(format!(".{}.partial", name));
   if temporary.exists()
   {
      bail!("refusing to overwrite macOS deep-attribution partial artifact {}", temporary.display());
   }
   fs::write(&temporary, bytes).with_context(|| format!("writing macOS deep-attribution partial artifact {}", temporary.display()))?;
   File::open(&temporary).with_context(|| format!("opening macOS deep-attribution partial artifact {}", temporary.display()))?.sync_all().with_context(|| format!("synchronizing macOS deep-attribution partial artifact {}", temporary.display()))?;
   fs::rename(&temporary, destination).with_context(|| format!("publishing macOS deep-attribution artifact {}", destination.display()))?;
   Ok(())
}

fn sha256_file(path: &Path) -> Result<String>
{
   Ok(format!("{:x}", Sha256::digest(fs::read(path).with_context(|| format!("reading {}", path.display()))?)))
}

fn sha256_tree(root: &Path) -> Result<String>
{
   let mut files = Vec::new();
   collect_files(root, root, &mut files)?;
   files.sort_by(|left, right| left.0.cmp(&right.0));
   let mut digest = Sha256::new();
   for (relative, path) in files
   {
      digest.update((relative.len() as u64).to_le_bytes());
      digest.update(relative.as_bytes());
      let bytes = fs::read(&path).with_context(|| format!("reading trace bundle member {}", path.display()))?;
      digest.update((bytes.len() as u64).to_le_bytes());
      digest.update(&bytes);
   }
   Ok(format!("{:x}", digest.finalize()))
}

fn collect_files(root: &Path, path: &Path, files: &mut Vec<(String, PathBuf)>) -> Result<()>
{
   let metadata = fs::metadata(path).with_context(|| format!("reading metadata for {}", path.display()))?;
   if metadata.is_file()
   {
      let relative = path.strip_prefix(root).context("trace bundle member escapes its root")?.to_str().context("trace bundle member path is not UTF-8")?.to_string();
      files.push((relative, path.to_path_buf()));
      return Ok(());
   }
   if !metadata.is_dir()
   {
      bail!("trace bundle member is neither a regular file nor directory: {}", path.display());
   }
   for entry in fs::read_dir(path).with_context(|| format!("reading {}", path.display()))?
   {
      collect_files(root, &entry?.path(), files)?;
   }
   Ok(())
}

fn spawn_fuse_monitor(process_group: u32, directory: PathBuf, scratch: PathBuf, stop: Arc<AtomicBool>, state: Arc<AtomicU8>) -> JoinHandle<()>
{
   thread::spawn(move || {
      while !stop.load(Ordering::Acquire)
      {
         match working_set_bytes(&directory, &scratch)
         {
            Ok(bytes) if bytes <= MACOS_DEEP_ATTRIBUTION_WORKING_SET_LIMIT_BYTES => (),
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

fn working_set_bytes(directory: &Path, scratch: &Path) -> Result<u64>
{
   path_bytes_if_present(directory)?.checked_add(path_bytes_if_present(scratch)?).context("macOS deep-attribution working-set byte count overflow")
}

fn path_bytes_if_present(path: &Path) -> Result<u64>
{
   if !path.exists()
   {
      return Ok(0);
   }
   let metadata = fs::metadata(path).with_context(|| format!("reading metadata for {}", path.display()))?;
   if metadata.is_file()
   {
      return Ok(metadata.len());
   }
   if !metadata.is_dir()
   {
      bail!("macOS deep-attribution working-set member is neither a file nor directory: {}", path.display());
   }
   let mut bytes = 0_u64;
   for entry in fs::read_dir(path).with_context(|| format!("reading {}", path.display()))?
   {
      bytes = bytes.checked_add(path_bytes_if_present(&entry?.path())?).context("macOS deep-attribution working-set byte count overflow")?;
   }
   Ok(bytes)
}

fn signal_process(pid: u32, signal: &str) -> Result<()>
{
   let status = Command::new("/bin/kill").args([signal, &pid.to_string()]).status().context("signaling macOS deep-attribution process")?;
   if !status.success()
   {
      bail!("signaling macOS deep-attribution process {} failed with {}", pid, status);
   }
   Ok(())
}

fn terminate_process_group(process_group: u32)
{
   if process_group == 0
   {
      return;
   }
   let group = format!("-{}", process_group);
   let _ = Command::new("/bin/kill").args(["-TERM", "--", &group]).status();
   thread::sleep(Duration::from_millis(50));
   let _ = Command::new("/bin/kill").args(["-KILL", "--", &group]).status();
}
