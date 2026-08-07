use std::collections::BTreeMap;
use std::fs;

use oxide_perf_runner::paired::{
   analyze_paired_experiment, balanced_pair_order, report_json, AcceptancePolicy,
   ConfidenceIntervalMethod,
   EnvironmentFingerprint, ExperimentIdentity, PairInvalidationReason, PairOrder,
   PairedExperimentInput, SamplePair, WorkloadKind, PAIRED_EXPERIMENT_SCHEMA_VERSION,
};

const BASE_SHA: &str = "1111111111111111111111111111111111111111";
const TREE_SHA: &str = "2222222222222222222222222222222222222222";
const INSTRUMENTATION_SHA: &str = "3333333333333333333333333333333333333333333333333333333333333333";
const BINARY_A_SHA: &str = "4444444444444444444444444444444444444444444444444444444444444444";
const BINARY_B_SHA: &str = "5555555555555555555555555555555555555555555555555555555555555555";

fn environment() -> EnvironmentFingerprint
{
   EnvironmentFingerprint {
      hardware: String::from("test-host"),
      os: String::from("test-os"),
      toolchain: String::from("rustc-test"),
      browser_or_device: String::from("none"),
      viewport: String::from("offscreen"),
      scale: String::from("1"),
      refresh_mode: String::from("offscreen"),
      cache_state: String::from("warm"),
      build_flags: String::from("--release --locked"),
      instrumentation_enabled: true,
      production_path: false,
   }
}

fn input(candidate_factor: f64) -> PairedExperimentInput
{
   let seed = 0x5eed_u64;
   let orders = balanced_pair_order(seed, 15);
   let pairs = orders
      .into_iter()
      .enumerate()
      .map(|(index, order)| SamplePair {
         index,
         order,
         warmup_samples_a: vec![10.0],
         warmup_samples_b: vec![10.0 * candidate_factor],
         samples_a: vec![
            10.0 + index as f64 * 0.01,
            11.0 + index as f64 * 0.01,
            12.0 + index as f64 * 0.01,
         ],
         samples_b: vec![
            (10.0 + index as f64 * 0.01) * candidate_factor,
            (11.0 + index as f64 * 0.01) * candidate_factor,
            (12.0 + index as f64 * 0.01) * candidate_factor,
         ],
         invalid_reason: None,
         environment_a: environment(),
         environment_b: environment(),
         artifact_hashes_a: BTreeMap::from([
            (String::from("binary"), String::from(BINARY_A_SHA)),
            (String::from("instrumentation"), String::from(INSTRUMENTATION_SHA)),
            (String::from("raw"), format!("{index:064x}")),
         ]),
         artifact_hashes_b: BTreeMap::from([
            (String::from("binary"), String::from(BINARY_B_SHA)),
            (String::from("instrumentation"), String::from(INSTRUMENTATION_SHA)),
            (String::from("raw"), format!("{:064x}", index + 100)),
         ]),
      })
      .collect();
   PairedExperimentInput {
      schema_version: PAIRED_EXPERIMENT_SCHEMA_VERSION,
      experiment_id: String::from("paired-synthetic"),
      workload: WorkloadKind::WorkspaceCpu,
      metric: String::from("us/op"),
      lower_is_better: true,
      acceptance_policy: AcceptancePolicy::Performance,
      seed,
      identity: ExperimentIdentity {
         baseline_sha: String::from(BASE_SHA),
         candidate_tree_sha: String::from(TREE_SHA),
         instrumentation_sha256: String::from(INSTRUMENTATION_SHA),
         baseline_binary_sha256: String::from(BINARY_A_SHA),
         candidate_binary_sha256: String::from(BINARY_B_SHA),
      },
      pairs,
   }
}

fn constant_input(baseline: f64, candidate: f64, lower_is_better: bool) -> PairedExperimentInput
{
   let mut input = input(1.0);
   input.lower_is_better = lower_is_better;
   input.acceptance_policy = AcceptancePolicy::NoMaterialRegression;
   for pair in &mut input.pairs
   {
      pair.warmup_samples_a = vec![baseline];
      pair.warmup_samples_b = vec![candidate];
      pair.samples_a = vec![baseline; 3];
      pair.samples_b = vec![candidate; 3];
   }
   input
}

fn input_with_pair_count(candidate_factor: f64, pair_count: usize) -> PairedExperimentInput
{
   let mut expanded = input(candidate_factor);
   let source_pairs = expanded.pairs.clone();
   expanded.pairs = balanced_pair_order(expanded.seed, pair_count)
      .into_iter()
      .enumerate()
      .map(|(index, order)| {
         let mut pair = source_pairs[index % source_pairs.len()].clone();
         pair.index = index;
         pair.order = order;
         pair.artifact_hashes_a.insert(String::from("raw"), format!("{index:064x}"));
         pair.artifact_hashes_b.insert(String::from("raw"), format!("{:064x}", index + 100));
         pair
      })
      .collect();
   expanded
}

fn ranked_speedup_input(pair_count: usize, workload: WorkloadKind, samples_per_pair: usize) -> PairedExperimentInput
{
   let mut ranked = input_with_pair_count(1.0, pair_count);
   ranked.workload = workload;
   for (index, pair) in ranked.pairs.iter_mut().enumerate()
   {
      let baseline = 100.0;
      let candidate = baseline - (index + 1) as f64;
      pair.warmup_samples_a = vec![baseline];
      pair.warmup_samples_b = vec![candidate];
      pair.samples_a = vec![baseline; samples_per_pair];
      pair.samples_b = vec![candidate; samples_per_pair];
      if workload == WorkloadKind::PhysicalDeviceFrames
      {
         pair.environment_a.production_path = true;
         pair.environment_b.production_path = true;
      }
   }
   ranked
}

#[test]
fn balanced_order_is_deterministic_and_balanced()
{
   let first = balanced_pair_order(9, 16);
   let second = balanced_pair_order(9, 16);
   assert_eq!(first, second);
   assert_eq!(first.iter().filter(|order| **order == PairOrder::Ab).count(), 8);
   assert_eq!(first.iter().filter(|order| **order == PairOrder::Ba).count(), 8);
}

#[test]
fn invalidation_cannot_select_only_ba_pairs_from_a_balanced_schedule()
{
   let mut exploit = input_with_pair_count(0.90, 30);
   for pair in &mut exploit.pairs
   {
      if pair.order == PairOrder::Ab
      {
         pair.samples_a.clear();
         pair.samples_b.clear();
         pair.invalid_reason = Some(PairInvalidationReason::MissingMeasuredSamples);
      }
   }
   let error = analyze_paired_experiment(exploit).expect_err("reject all-BA survivor selection");
   assert!(error.to_string().contains("invalid pairs exceed 10%"));
}

#[test]
fn fabricated_and_excessive_invalidations_are_rejected()
{
   let mut fabricated = input(0.90);
   fabricated.pairs[0].invalid_reason = Some(PairInvalidationReason::MissingMeasuredSamples);
   let error = analyze_paired_experiment(fabricated).expect_err("reject unsupported invalidation claim");
   assert!(error.to_string().contains("does not match its evidence"));

   let mut excessive = input_with_pair_count(0.90, 30);
   for pair in excessive.pairs.iter_mut().take(4)
   {
      pair.samples_a.clear();
      pair.samples_b.clear();
      pair.invalid_reason = Some(PairInvalidationReason::MissingMeasuredSamples);
   }
   let error = analyze_paired_experiment(excessive).expect_err("reject excessive invalidation rate");
   assert_eq!(error.to_string(), "4 invalid pairs exceed 10% of the 30 scheduled pairs");
}

#[test]
fn survivor_order_balance_is_enforced_at_the_invalidation_cap()
{
   let mut selected = input_with_pair_count(0.90, 20);
   let mut invalidated = 0;
   for pair in &mut selected.pairs
   {
      if pair.order == PairOrder::Ab && invalidated < 2
      {
         pair.samples_a.clear();
         pair.samples_b.clear();
         pair.invalid_reason = Some(PairInvalidationReason::MissingMeasuredSamples);
         invalidated += 1;
      }
   }
   assert_eq!(invalidated, 2);
   let error = analyze_paired_experiment(selected).expect_err("reject imbalanced survivors at exact cap");
   assert_eq!(error.to_string(), "surviving AB/BA pair counts are imbalanced: 8 AB and 10 BA");
}

#[test]
fn valid_pairs_require_equal_sample_counts()
{
   let mut measured = input(0.90);
   measured.pairs[0].samples_b.pop();
   let error = analyze_paired_experiment(measured).expect_err("reject unequal measured sample counts");
   assert_eq!(error.to_string(), "pair 0 has unequal measured sample counts");

   let mut warmup = input(0.90);
   warmup.pairs[0].warmup_samples_b.push(9.0);
   let error = analyze_paired_experiment(warmup).expect_err("reject unequal required warmup sample counts");
   assert_eq!(error.to_string(), "pair 0 has unequal warmup sample counts");
}

#[test]
fn capped_diagnostic_invalidation_preserves_balanced_publication()
{
   let mut diagnostic = input_with_pair_count(0.90, 16);
   diagnostic.pairs[0].environment_b.viewport = String::from("other-offscreen");
   diagnostic.pairs[0].invalid_reason = Some(PairInvalidationReason::EnvironmentMismatch);
   let report = analyze_paired_experiment(diagnostic).expect("analyze one evidenced diagnostic invalidation");
   assert!(report.decision.accepted, "{:?}", report.decision.reasons);
   assert_eq!(report.decision.valid_pairs, 15);
   assert_eq!(report.pairs.len(), 16);
   let serialized = serde_json::to_value(&report).expect("serialize diagnostic invalidation");
   assert_eq!(serialized["pairs"][0]["invalid_reason"].as_str(), Some("environment-mismatch"));
}

#[test]
fn invalid_pairs_do_not_bypass_evidence_validation()
{
   let mut invalid_sample = input_with_pair_count(0.90, 16);
   invalid_sample.pairs[0].samples_a.clear();
   invalid_sample.pairs[0].samples_b.clear();
   invalid_sample.pairs[0].warmup_samples_a[0] = f64::NAN;
   invalid_sample.pairs[0].invalid_reason = Some(PairInvalidationReason::MissingMeasuredSamples);
   let error = analyze_paired_experiment(invalid_sample).expect_err("reject malformed invalid-pair sample");
   assert!(error.to_string().contains("A warmup contains invalid sample"));

   let mut invalid_environment = input_with_pair_count(0.90, 16);
   invalid_environment.pairs[0].samples_a.clear();
   invalid_environment.pairs[0].samples_b.clear();
   invalid_environment.pairs[0].environment_a.hardware.clear();
   invalid_environment.pairs[0].invalid_reason = Some(PairInvalidationReason::MissingMeasuredSamples);
   let error = analyze_paired_experiment(invalid_environment).expect_err("reject malformed invalid-pair environment");
   assert!(error.to_string().contains("environment field hardware is empty"));

   let mut invalid_artifact = input_with_pair_count(0.90, 16);
   invalid_artifact.pairs[0].samples_a.clear();
   invalid_artifact.pairs[0].samples_b.clear();
   invalid_artifact.pairs[0]
      .artifact_hashes_a
      .insert(String::from("binary"), String::from("not-a-sha256"));
   invalid_artifact.pairs[0].invalid_reason = Some(PairInvalidationReason::MissingMeasuredSamples);
   let error = analyze_paired_experiment(invalid_artifact).expect_err("reject malformed invalid-pair artifact");
   assert!(error.to_string().contains("artifact SHA-256"));
}

#[test]
fn invalidation_schema_is_closed_and_null_compatible()
{
   let mut all_valid = serde_json::to_value(input(0.90)).expect("serialize all-valid input");
   assert!(all_valid["pairs"][0]["invalid_reason"].is_null());
   let parsed: PairedExperimentInput = serde_json::from_value(all_valid.clone())
      .expect("parse null invalidation reason");
   assert_eq!(parsed.pairs[0].invalid_reason, None);

   all_valid["pairs"][0]["invalid_reason"] = serde_json::Value::String(String::from("operator-choice"));
   assert!(serde_json::from_value::<PairedExperimentInput>(all_valid).is_err());

   let persisted: serde_json::Value = serde_json::from_str(include_str!(
      "../../../benchmarks/experiments/c55-macos-demand-display-link/accepted-wake-cpu-report.json"
   ))
   .expect("parse persisted all-valid report JSON");
   let persisted_pair: SamplePair = serde_json::from_value(persisted["pairs"][0].clone())
      .expect("parse persisted null invalidation reason");
   assert_eq!(persisted_pair.invalid_reason, None);
   let round_trip = serde_json::to_value(persisted_pair).expect("reserialize persisted pair");
   assert!(round_trip["invalid_reason"].is_null());
}

#[test]
fn decisive_improvement_passes_statistical_gates()
{
   let report = analyze_paired_experiment(input(0.90)).expect("analyze decisive improvement");
   assert!(report.decision.accepted, "{:?}", report.decision.reasons);
   assert!(report.lower_is_better);
   assert_eq!(report.decision.pair_wins, 15);
   assert!(report.decision.median_speedup_pct > 9.9);
   assert!(report.decision.confidence_interval.bounds_pct[0] > 9.9);
   assert_eq!(report.baseline_sample_count, 45);
   assert_eq!(report.candidate_sample_count, 45);
   assert!((report.baseline.p50 - 11.07).abs() < f64::EPSILON);
}

#[test]
fn exact_median_interval_reports_conservative_rank_coverage()
{
   let report = analyze_paired_experiment(
      ranked_speedup_input(15, WorkloadKind::WorkspaceCpu, 1),
   ).expect("analyze exact 15-pair confidence interval");
   let interval = report.decision.confidence_interval;
   assert_eq!(interval.method, ConfidenceIntervalMethod::ExactBinomialMedian);
   assert_eq!(interval.target_coverage, 0.95);
   assert_eq!(interval.achieved_coverage, 0.964_843_75);
   assert_eq!([interval.lower_rank, interval.upper_rank], [4, 12]);
   assert!((interval.bounds_pct[0] - 4.0).abs() < 1e-12);
   assert!((interval.bounds_pct[1] - 12.0).abs() < 1e-12);
}

#[test]
fn physical_device_minimum_supports_a_finite_exact_interval()
{
   let too_short = ranked_speedup_input(5, WorkloadKind::PhysicalDeviceFrames, 400);
   let error = analyze_paired_experiment(too_short).expect_err("reject five-pair physical-device interval");
   assert_eq!(error.to_string(), "5 valid pairs are below the PhysicalDeviceFrames minimum of 6");

   let report = analyze_paired_experiment(
      ranked_speedup_input(6, WorkloadKind::PhysicalDeviceFrames, 334),
   ).expect("analyze minimum physical-device interval");
   let interval = report.decision.confidence_interval;
   assert_eq!(interval.achieved_coverage, 0.968_75);
   assert_eq!([interval.lower_rank, interval.upper_rank], [1, 6]);
   assert!((interval.bounds_pct[0] - 1.0).abs() < 1e-12);
   assert!((interval.bounds_pct[1] - 6.0).abs() < 1e-12);
}

#[test]
fn ties_and_regressions_are_rejected()
{
   let tie = analyze_paired_experiment(input(1.0)).expect("analyze tie");
   assert!(!tie.decision.accepted);
   assert_eq!(tie.decision.pair_wins, 0);

   let regression = analyze_paired_experiment(input(1.10)).expect("analyze regression");
   assert!(!regression.decision.accepted);
   assert!(regression.decision.median_speedup_pct < 0.0);
}

#[test]
fn insufficient_and_mixed_inputs_are_rejected()
{
   let mut too_short = input(0.90);
   too_short.pairs.pop();
   assert!(analyze_paired_experiment(too_short).is_err());

   let mut mixed = input(0.90);
   mixed.pairs[3].environment_b.cache_state = String::from("cold");
   assert!(analyze_paired_experiment(mixed).is_err());

   let mut mixed_sessions = input(0.90);
   mixed_sessions.pairs[3].environment_a.hardware = String::from("other-host");
   mixed_sessions.pairs[3].environment_b.hardware = String::from("other-host");
   assert!(analyze_paired_experiment(mixed_sessions).is_err());

   let mut stale_binary = input(0.90);
   stale_binary.pairs[2]
      .artifact_hashes_a
      .insert(String::from("binary"), String::from(BINARY_B_SHA));
   assert!(analyze_paired_experiment(stale_binary).is_err());
}

#[test]
fn no_material_regression_policy_accepts_ties_but_not_tail_regressions()
{
   let mut tie = input(1.0);
   tie.acceptance_policy = AcceptancePolicy::NoMaterialRegression;
   assert!(analyze_paired_experiment(tie).expect("analyze parity").decision.accepted);

   let mut regression = input(1.0);
   regression.acceptance_policy = AcceptancePolicy::NoMaterialRegression;
   regression.pairs[14].samples_b[2] *= 2.0;
   let report = analyze_paired_experiment(regression).expect("analyze tail regression");
   assert!(!report.decision.accepted);
   assert!(report.decision.reasons.iter().any(|reason| reason.contains("p99") || reason.contains("peak")));
}

#[test]
fn higher_is_better_tail_direction_is_respected()
{
   let mut improvement = input(1.10);
   improvement.lower_is_better = false;
   let improved = analyze_paired_experiment(improvement).expect("analyze higher-is-better improvement");
   assert!(improved.decision.accepted, "{:?}", improved.decision.reasons);
   assert!(improved.decision.median_speedup_pct > 9.9);

   let mut regression = input(0.90);
   regression.lower_is_better = false;
   let regressed = analyze_paired_experiment(regression).expect("analyze higher-is-better regression");
   assert!(!regressed.decision.accepted);
   assert!(regressed.decision.reasons.iter().any(|reason| reason.contains("p05")));
   assert!(regressed.decision.reasons.iter().any(|reason| reason.contains("p01")));
   assert!(regressed.decision.reasons.iter().any(|reason| reason.contains("minimum")));
}

#[test]
fn higher_is_better_low_tail_regression_blocks_publication()
{
   let mut parity = input(1.0);
   parity.lower_is_better = false;
   parity.acceptance_policy = AcceptancePolicy::NoMaterialRegression;
   let admitted = analyze_paired_experiment(parity).expect("analyze higher-is-better parity");
   assert!(admitted.decision.accepted, "{:?}", admitted.decision.reasons);

   let mut regression = input(1.0);
   regression.lower_is_better = false;
   regression.acceptance_policy = AcceptancePolicy::NoMaterialRegression;
   for pair in regression.pairs.iter_mut().take(3)
   {
      pair.samples_b[0] *= 0.80;
   }
   let report = analyze_paired_experiment(regression).expect("analyze isolated lower-tail regression");
   assert!(!report.lower_is_better);
   assert_eq!(report.decision.median_speedup_pct, 0.0);
   assert_eq!(report.baseline.p50, report.candidate.p50);
   assert_eq!(report.baseline.p95, report.candidate.p95);
   assert_eq!(report.baseline.p99, report.candidate.p99);
   assert_eq!(report.baseline.peak, report.candidate.peak);
   assert!(report.candidate.p05 < report.baseline.p05);
   assert!(report.candidate.p01 < report.baseline.p01);
   assert!(report.candidate.minimum < report.baseline.minimum);
   assert!(!report.decision.accepted);
   assert!(report.decision.reasons.iter().any(|reason| reason.contains("p05")));
   assert!(report.decision.reasons.iter().any(|reason| reason.contains("p01")));
   assert!(report.decision.reasons.iter().any(|reason| reason.contains("minimum")));

   let report_bytes = report_json(&report).expect("serialize lower-tail decision inputs");
   let serialized: serde_json::Value = serde_json::from_slice(&report_bytes)
      .expect("parse serialized lower-tail decision inputs");
   assert_eq!(serialized["schema_version"].as_u64(), Some(PAIRED_EXPERIMENT_SCHEMA_VERSION as u64));
   assert_eq!(serialized["lower_is_better"].as_bool(), Some(false));
   for (side, summary) in [("baseline", &report.baseline), ("candidate", &report.candidate)]
   {
      assert_eq!(serialized[side]["p05"].as_f64(), Some(summary.p05));
      assert_eq!(serialized[side]["p01"].as_f64(), Some(summary.p01));
      assert_eq!(serialized[side]["minimum"].as_f64(), Some(summary.minimum));
   }
}

#[test]
fn zero_baseline_median_is_rejected_before_report_serialization()
{
   let error = analyze_paired_experiment(constant_input(0.0, 1.0, false))
      .expect_err("reject undefined zero-baseline speedup");
   assert_eq!(error.to_string(), "pair 0 baseline median is zero; relative speedup is undefined");
}

#[test]
fn relative_tail_regression_boundaries_are_symmetric()
{
   for (lower_is_better, boundary, beyond) in [
      (true, 103.0, 103.01),
      (false, 97.0, 96.99),
   ]
   {
      let tail_labels = if lower_is_better { ["p95", "p99"] } else { ["p05", "p01"] };
      let at_boundary = analyze_paired_experiment(constant_input(100.0, boundary, lower_is_better))
         .expect("analyze exact three-percent boundary");
      assert!(!at_boundary.decision.reasons.iter().any(|reason| reason.contains(tail_labels[0])));
      assert!(!at_boundary.decision.reasons.iter().any(|reason| reason.contains(tail_labels[1])));

      let past_boundary = analyze_paired_experiment(constant_input(100.0, beyond, lower_is_better))
         .expect("analyze beyond three-percent boundary");
      assert!(past_boundary.decision.reasons.iter().any(|reason| reason.contains(tail_labels[0])));
      assert!(past_boundary.decision.reasons.iter().any(|reason| reason.contains(tail_labels[1])));
   }

   for (lower_is_better, boundary, beyond) in [
      (true, 105.0, 105.01),
      (false, 95.0, 94.99),
   ]
   {
      let worst_label = if lower_is_better { "peak" } else { "minimum" };
      let at_boundary = analyze_paired_experiment(constant_input(100.0, boundary, lower_is_better))
         .expect("analyze exact five-percent boundary");
      assert!(!at_boundary.decision.reasons.iter().any(|reason| reason.contains(worst_label)));

      let past_boundary = analyze_paired_experiment(constant_input(100.0, beyond, lower_is_better))
         .expect("analyze beyond five-percent boundary");
      assert!(past_boundary.decision.reasons.iter().any(|reason| reason.contains(worst_label)));
   }
}

#[test]
fn cold_browser_startup_persists_empty_warmups()
{
   let mut startup = input(0.90);
   startup.workload = WorkloadKind::BrowserStartup;
   startup.pairs = balanced_pair_order(startup.seed, 25)
      .into_iter()
      .enumerate()
      .map(|(index, order)| {
         let mut pair = startup.pairs[index % startup.pairs.len()].clone();
         pair.index = index;
         pair.order = order;
         pair.warmup_samples_a.clear();
         pair.warmup_samples_b.clear();
         pair.environment_a.cache_state = String::from("cold");
         pair.environment_b.cache_state = String::from("cold");
         pair
      })
      .collect();
   assert!(analyze_paired_experiment(startup).expect("analyze cold startup").decision.accepted);
}

#[test]
fn analysis_and_json_are_byte_deterministic()
{
   let first = analyze_paired_experiment(input(0.90)).expect("first analysis");
   let second = analyze_paired_experiment(input(0.90)).expect("second analysis");
   assert_eq!(first, second);
   assert_eq!(report_json(&first).expect("first JSON"), report_json(&second).expect("second JSON"));
}

#[test]
fn shared_cli_analyzes_and_persists_raw_evidence()
{
   let root = std::env::temp_dir().join(format!("oxide-paired-analysis-{}", std::process::id()));
   let input_path = root.join("input.json");
   let output_path = root.join("nested/report.json");
   fs::create_dir_all(&root).expect("create temp root");
   fs::write(&input_path, serde_json::to_vec_pretty(&input(0.90)).expect("serialize input"))
      .expect("write paired input");
   oxide_perf_runner::run_cli(&[
      String::from("--paired-analyze"),
      input_path.display().to_string(),
      String::from("--paired-json-out"),
      output_path.display().to_string(),
   ])
   .expect("run paired analyzer CLI");
   let report = fs::read_to_string(&output_path).expect("read paired report");
   let report: serde_json::Value = serde_json::from_str(&report).expect("parse paired report");
   assert_eq!(report["schema_version"].as_u64(), Some(PAIRED_EXPERIMENT_SCHEMA_VERSION as u64));
   assert_eq!(report["decision"]["accepted"].as_bool(), Some(true));
   assert!(report["pairs"][0]["warmup_samples_a"].is_array());
   assert_eq!(report["decision"]["confidence_interval"]["method"], "exact-binomial-median");
   assert_eq!(report["decision"]["confidence_interval"]["achieved_coverage"], 0.964_843_75);
   assert!(report.get("bootstrap_resamples").is_none());
   fs::remove_dir_all(root).expect("remove temp root");
}

#[test]
fn paired_cli_rejects_incomplete_output_arguments()
{
   let missing_output = oxide_perf_runner::run_cli(&[
      String::from("--paired-analyze"),
      String::from("input.json"),
   ]);
   assert!(missing_output.is_err());

   let orphan_output = oxide_perf_runner::run_cli(&[
      String::from("--paired-json-out"),
      String::from("output.json"),
   ]);
   assert!(orphan_output.is_err());
}
