use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const PAIRED_EXPERIMENT_SCHEMA_VERSION: u32 = 3;
pub const PAIRED_CONFIDENCE_TARGET_COVERAGE: f64 = 0.95;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PairOrder
{
   Ab,
   Ba,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PairInvalidationReason
{
   MissingWarmupSamples,
   MissingMeasuredSamples,
   UnequalMeasuredSampleCounts,
   EnvironmentMismatch,
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
   NoiseControl,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConfidenceIntervalMethod
{
   ExactBinomialMedian,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MedianConfidenceInterval
{
   pub method: ConfidenceIntervalMethod,
   pub target_coverage: f64,
   pub achieved_coverage: f64,
   pub lower_rank: usize,
   pub upper_rank: usize,
   pub bounds_pct: [f64; 2],
}

impl WorkloadKind
{
   fn minimum_pairs(self) -> usize
   {
      match self
      {
         Self::WorkspaceCpu | Self::PhysicalDeviceFrames => 6,
         Self::BrowserThroughput | Self::GpuTimestamps | Self::InputJourney => 15,
         Self::BrowserDisplayedFrames => 10,
         Self::BrowserStartup => 25,
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
#[serde(deny_unknown_fields)]
pub struct ExperimentIdentity
{
   pub baseline_sha: String,
   pub candidate_tree_sha: String,
   pub instrumentation_sha256: String,
   pub baseline_binary_sha256: String,
   pub candidate_binary_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct SamplePair
{
   pub index: usize,
   pub order: PairOrder,
   pub warmup_samples_a: Vec<f64>,
   pub warmup_samples_b: Vec<f64>,
   pub samples_a: Vec<f64>,
   pub samples_b: Vec<f64>,
   #[serde(default)]
   pub invalid_reason: Option<PairInvalidationReason>,
   pub environment_a: EnvironmentFingerprint,
   pub environment_b: EnvironmentFingerprint,
   pub artifact_hashes_a: BTreeMap<String, String>,
   pub artifact_hashes_b: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct DistributionSummary
{
   pub p50: f64,
   pub p95: f64,
   pub p99: f64,
   pub peak: f64,
   pub p05: f64,
   pub p01: f64,
   pub minimum: f64,
   pub median_absolute_deviation: f64,
   pub coefficient_of_variation: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PairedDecision
{
   pub accepted: bool,
   pub median_speedup_pct: f64,
   pub confidence_interval: MedianConfidenceInterval,
   pub pair_wins: usize,
   pub valid_pairs: usize,
   pub reasons: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PairedExperimentReport
{
   pub schema_version: u32,
   pub experiment_id: String,
   pub workload: WorkloadKind,
   pub metric: String,
   pub lower_is_better: bool,
   pub acceptance_policy: AcceptancePolicy,
   pub seed: u64,
   pub identity: ExperimentIdentity,
   pub baseline_sample_count: usize,
   pub candidate_sample_count: usize,
   pub baseline: DistributionSummary,
   pub candidate: DistributionSummary,
   pub decision: PairedDecision,
   pub pairs: Vec<SamplePair>,
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
      ensure!(baseline > 0.0, "pair {} baseline median is zero; relative speedup is undefined", pair.index);
      let speedup = relative_speedup_pct(baseline, candidate, input.lower_is_better);
      ensure!(speedup.is_finite(), "pair {} relative speedup is not finite", pair.index);
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
   let (median_speedup_pct, confidence_interval) = exact_median_confidence_interval(&speedups)?;
   let mut reasons = Vec::new();
   if input.acceptance_policy == AcceptancePolicy::Performance
   {
      if median_speedup_pct < 5.0
      {
         reasons.push(String::from("median speedup is below 5%"));
      }
      if confidence_interval.bounds_pct[0] < 2.0
      {
         reasons.push(String::from("paired 95% confidence lower bound is below 2%"));
      }
      if pair_wins * 5 < valid_pairs.len() * 4
      {
         reasons.push(String::from("candidate wins fewer than 80% of valid pairs"));
      }
   }
   else if input.acceptance_policy == AcceptancePolicy::NoMaterialRegression
   {
      if median_speedup_pct < -3.0
      {
         reasons.push(String::from("candidate median regresses by more than 3%"));
      }
   }
   else if confidence_interval.bounds_pct[0] < -2.0 || confidence_interval.bounds_pct[1] > 2.0
   {
      reasons.push(String::from("noise-control exact interval leaves the -2%..2% range"));
   }
   if input.acceptance_policy == AcceptancePolicy::NoiseControl
   {
      for (label, baseline_value, candidate_value) in [
         ("p95", baseline.p95, candidate.p95),
         ("p99", baseline.p99, candidate.p99),
      ]
      {
         if changes_by_fraction(baseline_value, candidate_value, 0.03)
         {
            reasons.push(format!("noise-control {label} moves by more than 3%"));
         }
      }
      if changes_by_fraction(baseline.peak, candidate.peak, 0.05)
      {
         reasons.push(String::from("noise-control peak moves by more than 5%"));
      }
   }
   else
   {
      let (baseline_adverse_tails, candidate_adverse_tails, adverse_tail_labels) = if input.lower_is_better
      {
         (
            [baseline.p95, baseline.p99, baseline.peak],
            [candidate.p95, candidate.p99, candidate.peak],
            ["p95", "p99", "peak"],
         )
      }
      else
      {
         (
            [baseline.p05, baseline.p01, baseline.minimum],
            [candidate.p05, candidate.p01, candidate.minimum],
            ["p05", "p01", "minimum"],
         )
      };
      if regresses_by_fraction(
         baseline_adverse_tails[0],
         candidate_adverse_tails[0],
         input.lower_is_better,
         0.03,
      )
      {
         reasons.push(format!("candidate {} regresses by more than 3%", adverse_tail_labels[0]));
      }
      if regresses_by_fraction(
         baseline_adverse_tails[1],
         candidate_adverse_tails[1],
         input.lower_is_better,
         0.03,
      )
      {
         reasons.push(format!("candidate {} regresses by more than 3%", adverse_tail_labels[1]));
      }
      if regresses_by_fraction(
         baseline_adverse_tails[2],
         candidate_adverse_tails[2],
         input.lower_is_better,
         0.05,
      )
      {
         reasons.push(format!("candidate {} regresses by more than 5%", adverse_tail_labels[2]));
      }
   }

   Ok(PairedExperimentReport {
      schema_version: PAIRED_EXPERIMENT_SCHEMA_VERSION,
      experiment_id: input.experiment_id,
      workload: input.workload,
      metric: input.metric,
      lower_is_better: input.lower_is_better,
      acceptance_policy: input.acceptance_policy,
      seed: input.seed,
      identity: input.identity,
      baseline_sample_count: baseline_samples.len(),
      candidate_sample_count: candidate_samples.len(),
      baseline,
      candidate,
      decision: PairedDecision {
         accepted: reasons.is_empty(),
         median_speedup_pct,
         confidence_interval,
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
   if input.acceptance_policy == AcceptancePolicy::NoiseControl
   {
      ensure!(
         input.identity.baseline_binary_sha256 == input.identity.candidate_binary_sha256,
         "noise-control requires identical baseline and candidate binary hashes",
      );
   }

   let expected_orders = balanced_pair_order(input.seed, input.pairs.len());
   let mut valid_pairs = 0;
   let mut invalid_pairs = 0;
   let mut valid_ab_pairs: usize = 0;
   let mut valid_ba_pairs: usize = 0;
   let mut samples_a = 0;
   let mut samples_b = 0;
   let mut shared_environment: Option<&EnvironmentFingerprint> = None;
   for (expected_index, pair) in input.pairs.iter().enumerate()
   {
      ensure!(pair.index == expected_index, "pair index {} is not contiguous at position {}", pair.index, expected_index);
      ensure!(pair.order == expected_orders[expected_index], "pair {} order does not match the predeclared-seed balanced order", pair.index);
      validate_samples(&pair.warmup_samples_a, pair.index, "A warmup")?;
      validate_samples(&pair.warmup_samples_b, pair.index, "B warmup")?;
      validate_samples(&pair.samples_a, pair.index, "A")?;
      validate_samples(&pair.samples_b, pair.index, "B")?;
      validate_environment(&pair.environment_a, pair.index)?;
      validate_environment(&pair.environment_b, pair.index)?;
      if input.workload.requires_production_path()
      {
         ensure!(pair.environment_a.production_path && pair.environment_b.production_path, "pair {} does not exercise the production path", pair.index);
      }
      ensure!(!pair.artifact_hashes_a.is_empty() && !pair.artifact_hashes_b.is_empty(), "pair {} is missing artifact hashes", pair.index);
      validate_artifact_identity(pair, &input.identity)?;
      if let Some(reason) = pair.invalid_reason
      {
         ensure!(invalidation_condition_holds(reason, input.workload, pair), "pair {} invalidation reason {:?} does not match its evidence", pair.index, reason);
         invalid_pairs += 1;
         continue;
      }
      valid_pairs += 1;
      match pair.order
      {
         PairOrder::Ab => valid_ab_pairs += 1,
         PairOrder::Ba => valid_ba_pairs += 1,
      }
      samples_a += pair.samples_a.len();
      samples_b += pair.samples_b.len();
      if input.workload.requires_warmup()
      {
         ensure!(!pair.warmup_samples_a.is_empty() && !pair.warmup_samples_b.is_empty(), "pair {} is missing warmup samples", pair.index);
         ensure!(pair.warmup_samples_a.len() == pair.warmup_samples_b.len(), "pair {} has unequal warmup sample counts", pair.index);
      }
      ensure!(!pair.samples_a.is_empty() && !pair.samples_b.is_empty(), "pair {} is missing raw samples", pair.index);
      ensure!(pair.samples_a.len() == pair.samples_b.len(), "pair {} has unequal measured sample counts", pair.index);
      ensure!(pair.environment_a == pair.environment_b, "pair {} mixes environments or cache states", pair.index);
      if let Some(environment) = shared_environment
      {
         ensure!(pair.environment_a == *environment, "pair {} differs from the experiment environment or cache state", pair.index);
      }
      else
      {
         shared_environment = Some(&pair.environment_a);
      }
   }
   ensure!(invalid_pairs <= input.pairs.len() / 10, "{} invalid pairs exceed 10% of the {} scheduled pairs", invalid_pairs, input.pairs.len());
   ensure!(valid_ab_pairs.abs_diff(valid_ba_pairs) <= 1, "surviving AB/BA pair counts are imbalanced: {} AB and {} BA", valid_ab_pairs, valid_ba_pairs);
   ensure!(valid_pairs >= input.workload.minimum_pairs(), "{} valid pairs are below the {:?} minimum of {}", valid_pairs, input.workload, input.workload.minimum_pairs());
   ensure!(samples_a >= input.workload.minimum_samples_per_side(), "{} A samples are below the {:?} minimum of {}", samples_a, input.workload, input.workload.minimum_samples_per_side());
   ensure!(samples_b >= input.workload.minimum_samples_per_side(), "{} B samples are below the {:?} minimum of {}", samples_b, input.workload, input.workload.minimum_samples_per_side());
   Ok(())
}

fn invalidation_condition_holds(reason: PairInvalidationReason, workload: WorkloadKind, pair: &SamplePair) -> bool
{
   match reason
   {
      PairInvalidationReason::MissingWarmupSamples => workload.requires_warmup() && (pair.warmup_samples_a.is_empty() || pair.warmup_samples_b.is_empty()),
      PairInvalidationReason::MissingMeasuredSamples => pair.samples_a.is_empty() || pair.samples_b.is_empty(),
      PairInvalidationReason::UnequalMeasuredSampleCounts => pair.samples_a.len() != pair.samples_b.len(),
      PairInvalidationReason::EnvironmentMismatch => pair.environment_a != pair.environment_b,
   }
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
   if lower_is_better
   {
      (baseline - candidate) / baseline * 100.0
   }
   else
   {
      (candidate - baseline) / baseline * 100.0
   }
}

fn regresses_by_fraction(baseline: f64, candidate: f64, lower_is_better: bool, allowed_fraction: f64) -> bool
{
   let regression = if lower_is_better { candidate - baseline } else { baseline - candidate };
   regression / baseline > allowed_fraction
}

fn changes_by_fraction(baseline: f64, candidate: f64, allowed_fraction: f64) -> bool
{
   if baseline == 0.0
   {
      candidate != 0.0
   }
   else
   {
      ((candidate - baseline) / baseline).abs() > allowed_fraction
   }
}

fn exact_median_confidence_interval(speedups: &[f64]) -> Result<(f64, MedianConfidenceInterval)>
{
   ensure!(!speedups.is_empty(), "paired confidence interval requires at least one speedup");
   let (lower_rank, upper_rank, achieved_coverage) = exact_median_rank_bounds(
      speedups.len(),
      PAIRED_CONFIDENCE_TARGET_COVERAGE,
   ).context("paired population cannot form a finite exact 95% median interval")?;
   let mut sorted = speedups.to_vec();
   sorted.sort_unstable_by(f64::total_cmp);
   let median_speedup_pct = percentile_sorted(&sorted, 0.50);
   Ok((median_speedup_pct, MedianConfidenceInterval {
      method: ConfidenceIntervalMethod::ExactBinomialMedian,
      target_coverage: PAIRED_CONFIDENCE_TARGET_COVERAGE,
      achieved_coverage,
      lower_rank,
      upper_rank,
      bounds_pct: [sorted[lower_rank - 1], sorted[upper_rank - 1]],
   }))
}

fn exact_median_rank_bounds(sample_count: usize, target_coverage: f64) -> Option<(usize, usize, f64)>
{
   if sample_count == 0
   {
      return None;
   }
   let midpoint = sample_count / 2;
   let mut weights = vec![0.0; midpoint + 1];
   // Scale every binomial coefficient to the modal coefficient so tail coverage cannot overflow.
   weights[midpoint] = 1.0;
   for successes in (1..=midpoint).rev()
   {
      weights[successes - 1] = weights[successes]
         * successes as f64
         / (sample_count - successes + 1) as f64;
   }
   let lower_half_weight = weights.iter().sum::<f64>();
   let total_weight = if sample_count % 2 == 0
   {
      lower_half_weight * 2.0 - 1.0
   }
   else
   {
      lower_half_weight * 2.0
   };
   let mut omitted_tail = 0.0;
   let mut selected = None;
   for (index, weight) in weights.into_iter().enumerate()
   {
      omitted_tail += weight / total_weight;
      let achieved_coverage = 1.0 - omitted_tail * 2.0;
      if achieved_coverage < target_coverage
      {
         break;
      }
      selected = Some((index + 1, sample_count - index, achieved_coverage));
   }
   selected
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
      p05: percentile_sorted(&sorted, 0.05),
      p01: percentile_sorted(&sorted, 0.01),
      minimum: sorted.first().copied().unwrap_or(0.0),
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
