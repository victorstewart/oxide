use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const PAIRED_EXPERIMENT_SCHEMA_VERSION: u32 = 1;
pub const PAIRED_BOOTSTRAP_RESAMPLES: usize = 100_000;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PairOrder
{
   Ab,
   Ba,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkloadKind
{
   WorkspaceCpu,
   BrowserThroughput,
   BrowserDisplayedFrames,
   BrowserStartup,
   GpuTimestamps,
   InputJourney,
   PhysicalDeviceFrames,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AcceptancePolicy
{
   Performance,
   NoMaterialRegression,
}

impl WorkloadKind
{
   fn minimum_pairs(self) -> usize
   {
      match self
      {
         Self::WorkspaceCpu | Self::BrowserThroughput | Self::GpuTimestamps | Self::InputJourney => 15,
         Self::BrowserDisplayedFrames => 10,
         Self::BrowserStartup => 25,
         Self::PhysicalDeviceFrames => 5,
      }
   }

   fn minimum_samples_per_side(self) -> usize
   {
      match self
      {
         Self::BrowserDisplayedFrames | Self::GpuTimestamps | Self::PhysicalDeviceFrames => 2_000,
         Self::InputJourney => 200,
         _ => self.minimum_pairs(),
      }
   }

   fn requires_production_path(self) -> bool
   {
      matches!(self, Self::BrowserDisplayedFrames | Self::PhysicalDeviceFrames)
   }

   fn requires_warmup(self) -> bool
   {
      self != Self::BrowserStartup
   }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExperimentIdentity
{
   pub baseline_sha: String,
   pub candidate_tree_sha: String,
   pub instrumentation_sha256: String,
   pub baseline_binary_sha256: String,
   pub candidate_binary_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EnvironmentFingerprint
{
   pub hardware: String,
   pub os: String,
   pub toolchain: String,
   pub browser_or_device: String,
   pub viewport: String,
   pub scale: String,
   pub refresh_mode: String,
   pub cache_state: String,
   pub build_flags: String,
   pub instrumentation_enabled: bool,
   pub production_path: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SamplePair
{
   pub index: usize,
   pub order: PairOrder,
   pub warmup_samples_a: Vec<f64>,
   pub warmup_samples_b: Vec<f64>,
   pub samples_a: Vec<f64>,
   pub samples_b: Vec<f64>,
   #[serde(default)]
   pub invalid_reason: Option<String>,
   pub environment_a: EnvironmentFingerprint,
   pub environment_b: EnvironmentFingerprint,
   pub artifact_hashes_a: BTreeMap<String, String>,
   pub artifact_hashes_b: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PairedExperimentInput
{
   pub schema_version: u32,
   pub experiment_id: String,
   pub workload: WorkloadKind,
   pub metric: String,
   pub lower_is_better: bool,
   pub acceptance_policy: AcceptancePolicy,
   pub seed: u64,
   pub identity: ExperimentIdentity,
   pub pairs: Vec<SamplePair>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DistributionSummary
{
   pub p50: f64,
   pub p95: f64,
   pub p99: f64,
   pub peak: f64,
   pub median_absolute_deviation: f64,
   pub coefficient_of_variation: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PairedDecision
{
   pub accepted: bool,
   pub median_speedup_pct: f64,
   pub confidence_interval_95_pct: [f64; 2],
   pub pair_wins: usize,
   pub valid_pairs: usize,
   pub reasons: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PairedExperimentReport
{
   pub schema_version: u32,
   pub experiment_id: String,
   pub workload: WorkloadKind,
   pub metric: String,
   pub acceptance_policy: AcceptancePolicy,
   pub seed: u64,
   pub bootstrap_resamples: usize,
   pub identity: ExperimentIdentity,
   pub baseline_sample_count: usize,
   pub candidate_sample_count: usize,
   pub baseline: DistributionSummary,
   pub candidate: DistributionSummary,
   pub decision: PairedDecision,
   pub pairs: Vec<SamplePair>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkflowAdapter
{
   WorkspaceCpu,
   Metal,
   WebGpu,
   BrowserStartup,
   Device,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceIdentityKind
{
   BaselineCommit,
   CandidateIndex,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkflowCommand
{
   pub adapter: WorkflowAdapter,
   pub program: String,
   pub args: Vec<String>,
   pub current_dir: PathBuf,
   #[serde(default)]
   pub env: BTreeMap<String, String>,
   pub source_root: PathBuf,
   pub source_identity_kind: SourceIdentityKind,
   pub artifact_path: PathBuf,
   pub samples_json_pointer: String,
   pub warmups_json_pointer: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PairedWorkflowPlan
{
   pub schema_version: u32,
   pub experiment_id: String,
   pub workload: WorkloadKind,
   pub metric: String,
   pub lower_is_better: bool,
   pub acceptance_policy: AcceptancePolicy,
   pub seed: u64,
   pub pair_count: usize,
   pub baseline_sha: String,
   pub candidate_tree_sha: String,
   pub instrumentation_patch: PathBuf,
   pub environment: EnvironmentFingerprint,
   pub baseline: WorkflowCommand,
   pub candidate: WorkflowCommand,
   pub evidence_root: PathBuf,
}

struct CollectedSide
{
   warmups: Vec<f64>,
   samples: Vec<f64>,
   hashes: BTreeMap<String, String>,
   invalid_reason: Option<String>,
}

pub fn run_paired_workflow(plan: PairedWorkflowPlan) -> Result<PairedExperimentReport>
{
   ensure!(plan.schema_version == PAIRED_EXPERIMENT_SCHEMA_VERSION, "unsupported paired workflow schema {}", plan.schema_version);
   validate_adapter(plan.workload, plan.baseline.adapter)?;
   validate_adapter(plan.workload, plan.candidate.adapter)?;
   ensure!(plan.baseline.adapter == plan.candidate.adapter, "A and B use different workflow adapters");
   ensure!(plan.baseline.source_identity_kind == SourceIdentityKind::BaselineCommit, "A must use a baseline commit identity");
   ensure!(plan.candidate.source_identity_kind == SourceIdentityKind::CandidateIndex, "B must use a candidate index identity");
   ensure!(plan.pair_count >= plan.workload.minimum_pairs(), "workflow declares {} pairs below the {:?} minimum of {}", plan.pair_count, plan.workload, plan.workload.minimum_pairs());
   validate_source_identity(&plan.baseline, &plan.baseline_sha)?;
   validate_source_identity(&plan.candidate, &plan.candidate_tree_sha)?;

   fs::create_dir_all(&plan.evidence_root)
      .with_context(|| format!("create paired evidence root {}", plan.evidence_root.display()))?;
   let instrumentation_sha256 = sha256_file(&plan.instrumentation_patch)?;
   let baseline_binary_sha256 = sha256_file(&plan.baseline.artifact_path)?;
   let candidate_binary_sha256 = sha256_file(&plan.candidate.artifact_path)?;
   let identity = ExperimentIdentity {
      baseline_sha: plan.baseline_sha.clone(),
      candidate_tree_sha: plan.candidate_tree_sha.clone(),
      instrumentation_sha256,
      baseline_binary_sha256,
      candidate_binary_sha256,
   };
   validate_identity(&identity)?;

   let orders = balanced_pair_order(plan.seed, plan.pair_count);
   let mut pairs = Vec::with_capacity(plan.pair_count);
   for (index, order) in orders.into_iter().enumerate()
   {
      let (a, b) = match order
      {
         PairOrder::Ab => (
            collect_side(&plan.baseline, &identity, &plan.evidence_root, index, "a")?,
            collect_side(&plan.candidate, &identity, &plan.evidence_root, index, "b")?,
         ),
         PairOrder::Ba => {
            let b = collect_side(&plan.candidate, &identity, &plan.evidence_root, index, "b")?;
            let a = collect_side(&plan.baseline, &identity, &plan.evidence_root, index, "a")?;
            (a, b)
         }
      };
      let invalid_reason = pair_invalid_reason(&a, &b);
      pairs.push(SamplePair {
         index,
         order,
         warmup_samples_a: a.warmups,
         warmup_samples_b: b.warmups,
         samples_a: a.samples,
         samples_b: b.samples,
         invalid_reason,
         environment_a: plan.environment.clone(),
         environment_b: plan.environment.clone(),
         artifact_hashes_a: a.hashes,
         artifact_hashes_b: b.hashes,
      });
   }

   let input = PairedExperimentInput {
      schema_version: PAIRED_EXPERIMENT_SCHEMA_VERSION,
      experiment_id: plan.experiment_id,
      workload: plan.workload,
      metric: plan.metric,
      lower_is_better: plan.lower_is_better,
      acceptance_policy: plan.acceptance_policy,
      seed: plan.seed,
      identity,
      pairs,
   };
   let input_bytes = serde_json::to_vec_pretty(&input).context("serialize paired workflow input")?;
   fs::write(plan.evidence_root.join("input.json"), input_bytes).context("write paired workflow input")?;
   analyze_paired_experiment(input)
}

pub fn create_instrumentation_patch(source_root: &Path, paths: &[PathBuf], output_path: &Path) -> Result<String>
{
   ensure!(!paths.is_empty(), "instrumentation patch requires at least one declared path");
   let mut command = Command::new("git");
   command
      .arg("-C")
      .arg(source_root)
      .args(["diff", "--binary", "--no-ext-diff", "HEAD", "--"])
      .args(paths);
   let output = command.output().with_context(|| format!("create instrumentation patch from {}", source_root.display()))?;
   ensure!(output.status.success(), "git failed to create instrumentation patch in {}", source_root.display());
   ensure!(!output.stdout.is_empty(), "declared instrumentation paths produced an empty patch");
   if let Some(parent) = output_path.parent()
   {
      fs::create_dir_all(parent).with_context(|| format!("create instrumentation patch directory {}", parent.display()))?;
   }
   fs::write(output_path, &output.stdout).with_context(|| format!("write instrumentation patch {}", output_path.display()))?;
   sha256_file(output_path)
}

fn validate_adapter(workload: WorkloadKind, adapter: WorkflowAdapter) -> Result<()>
{
   let valid = match adapter
   {
      WorkflowAdapter::WorkspaceCpu => workload == WorkloadKind::WorkspaceCpu,
      WorkflowAdapter::Metal => matches!(workload, WorkloadKind::GpuTimestamps | WorkloadKind::PhysicalDeviceFrames),
      WorkflowAdapter::WebGpu => matches!(workload, WorkloadKind::BrowserThroughput | WorkloadKind::BrowserDisplayedFrames | WorkloadKind::GpuTimestamps | WorkloadKind::InputJourney),
      WorkflowAdapter::BrowserStartup => workload == WorkloadKind::BrowserStartup,
      WorkflowAdapter::Device => matches!(workload, WorkloadKind::PhysicalDeviceFrames | WorkloadKind::InputJourney),
   };
   ensure!(valid, "{:?} adapter does not support {:?}", adapter, workload);
   Ok(())
}

fn validate_source_identity(command: &WorkflowCommand, expected: &str) -> Result<()>
{
   let args = match command.source_identity_kind
   {
      SourceIdentityKind::BaselineCommit => vec!["rev-parse", "HEAD"],
      SourceIdentityKind::CandidateIndex => vec!["write-tree"],
   };
   let output = Command::new("git")
      .arg("-C")
      .arg(&command.source_root)
      .args(args)
      .output()
      .with_context(|| format!("inspect source identity in {}", command.source_root.display()))?;
   ensure!(output.status.success(), "git source identity command failed in {}", command.source_root.display());
   let actual = String::from_utf8(output.stdout).context("source identity is not UTF-8")?;
   ensure!(actual.trim() == expected, "source identity {} does not match expected {}", actual.trim(), expected);
   Ok(())
}

fn collect_side(command: &WorkflowCommand, identity: &ExperimentIdentity, root: &Path, pair: usize, side: &str) -> Result<CollectedSide>
{
   let prefix = format!("pair-{pair:03}-{side}");
   let result_path = root.join(format!("{prefix}-result.json"));
   let stdout_path = root.join(format!("{prefix}-stdout.txt"));
   let stderr_path = root.join(format!("{prefix}-stderr.txt"));
   let expected_binary = if side == "a" { &identity.baseline_binary_sha256 } else { &identity.candidate_binary_sha256 };
   let binary_before = sha256_file(&command.artifact_path)?;
   ensure!(&binary_before == expected_binary, "pair {} side {} artifact changed before execution", pair, side);

   let mut process = Command::new(&command.program);
   process.current_dir(&command.current_dir);
   for arg in &command.args
   {
      process.arg(expand_argument(arg, pair, side, &result_path));
   }
   for (key, value) in &command.env
   {
      process.env(key, expand_argument(value, pair, side, &result_path));
   }
   let output = process.output().with_context(|| format!("launch pair {} side {} with {}", pair, side, command.program))?;
   fs::write(&stdout_path, &output.stdout).with_context(|| format!("write {}", stdout_path.display()))?;
   fs::write(&stderr_path, &output.stderr).with_context(|| format!("write {}", stderr_path.display()))?;
   let binary_after = sha256_file(&command.artifact_path)?;
   ensure!(binary_after == binary_before, "pair {} side {} artifact changed during execution", pair, side);

   let mut hashes = BTreeMap::from([
      (String::from("binary"), binary_before),
      (String::from("instrumentation"), identity.instrumentation_sha256.clone()),
      (String::from("stdout"), sha256_file(&stdout_path)?),
      (String::from("stderr"), sha256_file(&stderr_path)?),
   ]);
   if !output.status.success()
   {
      return Ok(CollectedSide {
         warmups: Vec::new(),
         samples: Vec::new(),
         hashes,
         invalid_reason: Some(format!("command exited with {}", output.status)),
      });
   }
   if !result_path.is_file()
   {
      return Ok(CollectedSide {
         warmups: Vec::new(),
         samples: Vec::new(),
         hashes,
         invalid_reason: Some(String::from("command did not create {result}")),
      });
   }
   hashes.insert(String::from("result"), sha256_file(&result_path)?);
   let result_bytes = fs::read(&result_path).with_context(|| format!("read {}", result_path.display()))?;
   let value = match serde_json::from_slice::<serde_json::Value>(&result_bytes)
   {
      Ok(value) => value,
      Err(error) => return Ok(CollectedSide {
         warmups: Vec::new(),
         samples: Vec::new(),
         hashes,
         invalid_reason: Some(format!("result JSON is invalid: {error}")),
      }),
   };
   let warmups = match optional_json_samples(&value, &command.warmups_json_pointer)
   {
      Ok(samples) => samples,
      Err(error) => return Ok(CollectedSide {
         warmups: Vec::new(),
         samples: Vec::new(),
         hashes,
         invalid_reason: Some(format!("warmup extraction failed: {error}")),
      }),
   };
   let samples = match json_samples(&value, &command.samples_json_pointer)
   {
      Ok(samples) => samples,
      Err(error) => return Ok(CollectedSide {
         warmups,
         samples: Vec::new(),
         hashes,
         invalid_reason: Some(format!("sample extraction failed: {error}")),
      }),
   };
   Ok(CollectedSide { warmups, samples, hashes, invalid_reason: None })
}

fn pair_invalid_reason(a: &CollectedSide, b: &CollectedSide) -> Option<String>
{
   match (&a.invalid_reason, &b.invalid_reason)
   {
      (None, None) => None,
      (Some(a), None) => Some(format!("A: {a}")),
      (None, Some(b)) => Some(format!("B: {b}")),
      (Some(a), Some(b)) => Some(format!("A: {a}; B: {b}")),
   }
}

fn expand_argument(value: &str, pair: usize, side: &str, result_path: &Path) -> String
{
   value
      .replace("{pair}", &pair.to_string())
      .replace("{side}", side)
      .replace("{result}", &result_path.to_string_lossy())
}

fn json_samples(value: &serde_json::Value, pointer: &str) -> Result<Vec<f64>>
{
   let value = value.pointer(pointer).with_context(|| format!("JSON pointer {pointer} is missing"))?;
   let values = if let Some(values) = value.as_array() { values.as_slice() } else { core::slice::from_ref(value) };
   values
      .iter()
      .map(|value| value.as_f64().context("sample is not a JSON number"))
      .collect()
}

fn optional_json_samples(value: &serde_json::Value, pointer: &str) -> Result<Vec<f64>>
{
   if pointer.is_empty()
   {
      Ok(Vec::new())
   }
   else
   {
      json_samples(value, pointer)
   }
}

fn sha256_file(path: &Path) -> Result<String>
{
   let mut file = fs::File::open(path).with_context(|| format!("open artifact {}", path.display()))?;
   let mut digest = Sha256::new();
   let mut buffer = [0_u8; 64 * 1_024];
   loop
   {
      let read = file.read(&mut buffer).with_context(|| format!("read artifact {}", path.display()))?;
      if read == 0
      {
         break;
      }
      digest.update(&buffer[..read]);
   }
   Ok(format!("{:x}", digest.finalize()))
}

pub fn balanced_pair_order(seed: u64, pair_count: usize) -> Vec<PairOrder>
{
   let mut state = seed.max(1);
   let mut orders = Vec::with_capacity(pair_count);
   while orders.len() < pair_count
   {
      state = xorshift64(state);
      let block = if state & 1 == 0
      {
         [PairOrder::Ab, PairOrder::Ba, PairOrder::Ba, PairOrder::Ab]
      }
      else
      {
         [PairOrder::Ba, PairOrder::Ab, PairOrder::Ab, PairOrder::Ba]
      };
      let remaining = pair_count - orders.len();
      orders.extend_from_slice(&block[..remaining.min(block.len())]);
   }
   orders
}

pub fn analyze_paired_experiment(input: PairedExperimentInput) -> Result<PairedExperimentReport>
{
   validate_input(&input)?;
   let valid_pairs = input.pairs.iter().filter(|pair| pair.invalid_reason.is_none()).collect::<Vec<_>>();
   let mut baseline_samples = Vec::new();
   let mut candidate_samples = Vec::new();
   let mut speedups = Vec::with_capacity(valid_pairs.len());
   let mut pair_wins = 0;

   for pair in &valid_pairs
   {
      let baseline = median(&pair.samples_a);
      let candidate = median(&pair.samples_b);
      let speedup = relative_speedup_pct(baseline, candidate, input.lower_is_better);
      baseline_samples.extend_from_slice(&pair.samples_a);
      candidate_samples.extend_from_slice(&pair.samples_b);
      speedups.push(speedup);
      if speedup > 0.0
      {
         pair_wins += 1;
      }
   }

   let baseline = summarize(&baseline_samples);
   let candidate = summarize(&candidate_samples);
   let median_speedup_pct = median(&speedups);
   let confidence_interval_95_pct = paired_bootstrap_ci(&speedups, input.seed);
   let mut reasons = Vec::new();
   if input.acceptance_policy == AcceptancePolicy::Performance
   {
      if median_speedup_pct < 5.0
      {
         reasons.push(String::from("median speedup is below 5%"));
      }
      if confidence_interval_95_pct[0] < 2.0
      {
         reasons.push(String::from("paired 95% confidence lower bound is below 2%"));
      }
      if pair_wins * 5 < valid_pairs.len() * 4
      {
         reasons.push(String::from("candidate wins fewer than 80% of valid pairs"));
      }
   }
   else if median_speedup_pct < -3.0
   {
      reasons.push(String::from("candidate median regresses by more than 3%"));
   }
   if percentile(&candidate_samples, 0.95) > percentile(&baseline_samples, 0.95) * 1.03
   {
      reasons.push(String::from("candidate p95 regresses by more than 3%"));
   }
   if percentile(&candidate_samples, 0.99) > percentile(&baseline_samples, 0.99) * 1.03
   {
      reasons.push(String::from("candidate p99 regresses by more than 3%"));
   }
   if candidate.peak > baseline.peak * 1.05
   {
      reasons.push(String::from("candidate peak regresses by more than 5%"));
   }

   Ok(PairedExperimentReport {
      schema_version: PAIRED_EXPERIMENT_SCHEMA_VERSION,
      experiment_id: input.experiment_id,
      workload: input.workload,
      metric: input.metric,
      acceptance_policy: input.acceptance_policy,
      seed: input.seed,
      bootstrap_resamples: PAIRED_BOOTSTRAP_RESAMPLES,
      identity: input.identity,
      baseline_sample_count: baseline_samples.len(),
      candidate_sample_count: candidate_samples.len(),
      baseline,
      candidate,
      decision: PairedDecision {
         accepted: reasons.is_empty(),
         median_speedup_pct,
         confidence_interval_95_pct,
         pair_wins,
         valid_pairs: valid_pairs.len(),
         reasons,
      },
      pairs: input.pairs,
   })
}

pub fn report_json(report: &PairedExperimentReport) -> Result<Vec<u8>>
{
   let mut bytes = Vec::with_capacity(16_384);
   serde_json::to_writer_pretty(&mut bytes, report).context("serialize paired experiment report")?;
   bytes.push(b'\n');
   Ok(bytes)
}

fn validate_input(input: &PairedExperimentInput) -> Result<()>
{
   ensure!(input.schema_version == PAIRED_EXPERIMENT_SCHEMA_VERSION, "unsupported paired experiment schema {}", input.schema_version);
   ensure!(!input.experiment_id.trim().is_empty(), "experiment id is empty");
   ensure!(!input.metric.trim().is_empty(), "primary metric is empty");
   validate_identity(&input.identity)?;

   let expected_orders = balanced_pair_order(input.seed, input.pairs.len());
   let mut valid_pairs = 0;
   let mut samples_a = 0;
   let mut samples_b = 0;
   let mut shared_environment: Option<&EnvironmentFingerprint> = None;
   for (expected_index, pair) in input.pairs.iter().enumerate()
   {
      ensure!(pair.index == expected_index, "pair index {} is not contiguous at position {}", pair.index, expected_index);
      ensure!(pair.order == expected_orders[expected_index], "pair {} order does not match fixed-seed balanced order", pair.index);
      if let Some(reason) = &pair.invalid_reason
      {
         ensure!(!reason.trim().is_empty(), "pair {} has an empty invalidation reason", pair.index);
         continue;
      }
      valid_pairs += 1;
      samples_a += pair.samples_a.len();
      samples_b += pair.samples_b.len();
      if input.workload.requires_warmup()
      {
         ensure!(!pair.warmup_samples_a.is_empty() && !pair.warmup_samples_b.is_empty(), "pair {} is missing warmup samples", pair.index);
      }
      ensure!(!pair.samples_a.is_empty() && !pair.samples_b.is_empty(), "pair {} is missing raw samples", pair.index);
      validate_samples(&pair.warmup_samples_a, pair.index, "A warmup")?;
      validate_samples(&pair.warmup_samples_b, pair.index, "B warmup")?;
      validate_samples(&pair.samples_a, pair.index, "A")?;
      validate_samples(&pair.samples_b, pair.index, "B")?;
      ensure!(pair.environment_a == pair.environment_b, "pair {} mixes environments or cache states", pair.index);
      validate_environment(&pair.environment_a, pair.index)?;
      if let Some(environment) = shared_environment
      {
         ensure!(pair.environment_a == *environment, "pair {} differs from the experiment environment or cache state", pair.index);
      }
      else
      {
         shared_environment = Some(&pair.environment_a);
      }
      if input.workload.requires_production_path()
      {
         ensure!(pair.environment_a.production_path, "pair {} does not exercise the production path", pair.index);
      }
      ensure!(!pair.artifact_hashes_a.is_empty() && !pair.artifact_hashes_b.is_empty(), "pair {} is missing artifact hashes", pair.index);
      validate_artifact_identity(pair, &input.identity)?;
   }
   ensure!(valid_pairs >= input.workload.minimum_pairs(), "{} valid pairs are below the {:?} minimum of {}", valid_pairs, input.workload, input.workload.minimum_pairs());
   ensure!(samples_a >= input.workload.minimum_samples_per_side(), "{} A samples are below the {:?} minimum of {}", samples_a, input.workload, input.workload.minimum_samples_per_side());
   ensure!(samples_b >= input.workload.minimum_samples_per_side(), "{} B samples are below the {:?} minimum of {}", samples_b, input.workload, input.workload.minimum_samples_per_side());
   Ok(())
}

fn validate_identity(identity: &ExperimentIdentity) -> Result<()>
{
   validate_hex_identity("baseline SHA", &identity.baseline_sha, 40)?;
   validate_hex_identity("candidate tree SHA", &identity.candidate_tree_sha, 40)?;
   validate_hex_identity("instrumentation SHA-256", &identity.instrumentation_sha256, 64)?;
   validate_hex_identity("baseline binary SHA-256", &identity.baseline_binary_sha256, 64)?;
   validate_hex_identity("candidate binary SHA-256", &identity.candidate_binary_sha256, 64)?;
   Ok(())
}

fn validate_hex_identity(label: &str, value: &str, length: usize) -> Result<()>
{
   ensure!(value.len() == length && value.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()), "{} is not a lowercase {}-character hexadecimal identity", label, length);
   Ok(())
}

fn validate_artifact_identity(pair: &SamplePair, identity: &ExperimentIdentity) -> Result<()>
{
   for (label, hashes) in [("A", &pair.artifact_hashes_a), ("B", &pair.artifact_hashes_b)]
   {
      for (name, hash) in hashes
      {
         validate_hex_identity(&format!("pair {} {} {} artifact SHA-256", pair.index, label, name), hash, 64)?;
      }
   }
   let binary_a = pair.artifact_hashes_a.get("binary").context("A artifact hashes omit binary")?;
   let binary_b = pair.artifact_hashes_b.get("binary").context("B artifact hashes omit binary")?;
   let instrumentation_a = pair.artifact_hashes_a.get("instrumentation").context("A artifact hashes omit instrumentation")?;
   let instrumentation_b = pair.artifact_hashes_b.get("instrumentation").context("B artifact hashes omit instrumentation")?;
   ensure!(binary_a == &identity.baseline_binary_sha256, "pair {} A binary hash differs from the declared identity", pair.index);
   ensure!(binary_b == &identity.candidate_binary_sha256, "pair {} B binary hash differs from the declared identity", pair.index);
   ensure!(instrumentation_a == &identity.instrumentation_sha256 && instrumentation_b == &identity.instrumentation_sha256, "pair {} instrumentation differs between A and B", pair.index);
   Ok(())
}

fn validate_environment(environment: &EnvironmentFingerprint, pair: usize) -> Result<()>
{
   for (name, value) in [
      ("hardware", environment.hardware.as_str()),
      ("os", environment.os.as_str()),
      ("toolchain", environment.toolchain.as_str()),
      ("browser_or_device", environment.browser_or_device.as_str()),
      ("viewport", environment.viewport.as_str()),
      ("scale", environment.scale.as_str()),
      ("refresh_mode", environment.refresh_mode.as_str()),
      ("cache_state", environment.cache_state.as_str()),
      ("build_flags", environment.build_flags.as_str()),
   ]
   {
      ensure!(!value.trim().is_empty(), "pair {} environment field {} is empty", pair, name);
   }
   Ok(())
}

fn validate_samples(samples: &[f64], pair: usize, label: &str) -> Result<()>
{
   for sample in samples
   {
      if !sample.is_finite() || *sample < 0.0
      {
         bail!("pair {} {} contains invalid sample {}", pair, label, sample);
      }
   }
   Ok(())
}

fn relative_speedup_pct(baseline: f64, candidate: f64, lower_is_better: bool) -> f64
{
   if baseline == 0.0
   {
      return if candidate == 0.0 { 0.0 } else { f64::NEG_INFINITY };
   }
   if lower_is_better
   {
      (baseline - candidate) / baseline * 100.0
   }
   else
   {
      (candidate - baseline) / baseline * 100.0
   }
}

fn paired_bootstrap_ci(speedups: &[f64], seed: u64) -> [f64; 2]
{
   let mut state = seed.max(1);
   let mut medians = Vec::with_capacity(PAIRED_BOOTSTRAP_RESAMPLES);
   let mut resample = vec![0.0; speedups.len()];
   for _ in 0..PAIRED_BOOTSTRAP_RESAMPLES
   {
      for value in &mut resample
      {
         state = xorshift64(state);
         *value = speedups[(state as usize) % speedups.len()];
      }
      medians.push(median(&resample));
   }
   medians.sort_unstable_by(f64::total_cmp);
   [percentile_sorted(&medians, 0.025), percentile_sorted(&medians, 0.975)]
}

fn summarize(samples: &[f64]) -> DistributionSummary
{
   let mut sorted = samples.to_vec();
   sorted.sort_unstable_by(f64::total_cmp);
   let p50 = percentile_sorted(&sorted, 0.50);
   let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
   let variance = sorted.iter().map(|value| (value - mean) * (value - mean)).sum::<f64>() / sorted.len() as f64;
   let mut deviations = sorted.iter().map(|value| (value - p50).abs()).collect::<Vec<_>>();
   deviations.sort_unstable_by(f64::total_cmp);
   DistributionSummary {
      p50,
      p95: percentile_sorted(&sorted, 0.95),
      p99: percentile_sorted(&sorted, 0.99),
      peak: sorted.last().copied().unwrap_or(0.0),
      median_absolute_deviation: percentile_sorted(&deviations, 0.50),
      coefficient_of_variation: if mean == 0.0 { 0.0 } else { variance.sqrt() / mean },
   }
}

fn median(samples: &[f64]) -> f64
{
   let mut sorted = samples.to_vec();
   sorted.sort_unstable_by(f64::total_cmp);
   percentile_sorted(&sorted, 0.50)
}

fn percentile(samples: &[f64], quantile: f64) -> f64
{
   let mut sorted = samples.to_vec();
   sorted.sort_unstable_by(f64::total_cmp);
   percentile_sorted(&sorted, quantile)
}

fn percentile_sorted(sorted: &[f64], quantile: f64) -> f64
{
   if sorted.is_empty()
   {
      return 0.0;
   }
   let rank = quantile.clamp(0.0, 1.0) * (sorted.len() - 1) as f64;
   let lower = rank.floor() as usize;
   let upper = rank.ceil() as usize;
   if lower == upper
   {
      sorted[lower]
   }
   else
   {
      sorted[lower] + (sorted[upper] - sorted[lower]) * (rank - lower as f64)
   }
}

fn xorshift64(mut value: u64) -> u64
{
   value ^= value << 13;
   value ^= value >> 7;
   value ^= value << 17;
   value
}
