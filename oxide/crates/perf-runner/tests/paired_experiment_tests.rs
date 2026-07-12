use oxide_perf_runner::paired::{
   analyze_paired_experiment, balanced_pair_order, report_json, AcceptancePolicy, EnvironmentFingerprint,
   ExperimentIdentity, PairOrder, PairedExperimentInput, PairedWorkflowPlan, SamplePair,
   SourceIdentityKind, WorkflowAdapter, WorkflowCommand, WorkloadKind,
   PAIRED_EXPERIMENT_SCHEMA_VERSION,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;

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
      experiment_id: String::from("c00-synthetic"),
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

fn git_output(root: &Path, args: &[&str]) -> String
{
   let output = Command::new("git").arg("-C").arg(root).args(args).output().expect("run git");
   assert!(output.status.success(), "git {:?} failed", args);
   String::from_utf8(output.stdout).expect("git UTF-8").trim().to_owned()
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
fn decisive_improvement_passes_statistical_gates()
{
   let report = analyze_paired_experiment(input(0.90)).expect("analyze decisive improvement");
   assert!(report.decision.accepted, "{:?}", report.decision.reasons);
   assert_eq!(report.decision.pair_wins, 15);
   assert!(report.decision.median_speedup_pct > 9.9);
   assert!(report.decision.confidence_interval_95_pct[0] > 9.9);
   assert_eq!(report.baseline_sample_count, 45);
   assert_eq!(report.candidate_sample_count, 45);
   assert!((report.baseline.p50 - 11.07).abs() < f64::EPSILON);
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
   assert!(report.contains("\"accepted\": true"));
   assert!(report.contains("\"warmup_samples_a\""));
   fs::remove_dir_all(root).expect("remove temp root");
}

#[test]
fn shared_workflow_runner_executes_fresh_balanced_process_pairs()
{
   let root = std::env::temp_dir().join(format!("oxide-paired-workflow-{}", std::process::id()));
   let source = root.join("source");
   let evidence = root.join("evidence");
   let patch = root.join("instrumentation.patch");
   fs::create_dir_all(&source).expect("create source root");
   assert!(Command::new("git").arg("init").arg("-q").arg(&source).status().expect("git init").success());
   assert!(Command::new("git").arg("-C").arg(&source).args(["config", "user.email", "paired@example.invalid"]).status().expect("git email").success());
   assert!(Command::new("git").arg("-C").arg(&source).args(["config", "user.name", "Paired Test"]).status().expect("git name").success());
   fs::write(source.join("identity.txt"), "baseline\n").expect("write baseline identity");
   assert!(Command::new("git").arg("-C").arg(&source).args(["add", "identity.txt"]).status().expect("git add baseline").success());
   assert!(Command::new("git").arg("-C").arg(&source).args(["commit", "-qm", "baseline"]).status().expect("git commit").success());
   let baseline_sha = git_output(&source, &["rev-parse", "HEAD"]);
   fs::write(source.join("identity.txt"), "candidate\n").expect("write candidate identity");
   assert!(Command::new("git").arg("-C").arg(&source).args(["add", "identity.txt"]).status().expect("git add candidate").success());
   let candidate_tree_sha = git_output(&source, &["write-tree"]);
   let patch_sha = oxide_perf_runner::paired::create_instrumentation_patch(
      &source,
      &[Path::new("identity.txt").to_path_buf()],
      &patch,
   )
   .expect("create instrumentation patch");
   assert_eq!(patch_sha.len(), 64);

   let command = |sample: f64, identity| WorkflowCommand {
      adapter: WorkflowAdapter::WorkspaceCpu,
      program: String::from("/bin/sh"),
      args: vec![
         String::from("-c"),
         format!("printf '{{\"warmups\":[10.0],\"samples\":[{sample}]}}\\n' > \"$1\""),
         String::from("paired-test"),
         String::from("{result}"),
      ],
      current_dir: source.clone(),
      env: BTreeMap::new(),
      source_root: source.clone(),
      source_identity_kind: identity,
      artifact_path: Path::new("/bin/sh").to_path_buf(),
      samples_json_pointer: String::from("/samples"),
      warmups_json_pointer: String::from("/warmups"),
   };
   let plan = PairedWorkflowPlan {
      schema_version: PAIRED_EXPERIMENT_SCHEMA_VERSION,
      experiment_id: String::from("c00-workflow-self-test"),
      workload: WorkloadKind::WorkspaceCpu,
      metric: String::from("us/op"),
      lower_is_better: true,
      acceptance_policy: AcceptancePolicy::Performance,
      seed: 99,
      pair_count: 15,
      baseline_sha,
      candidate_tree_sha,
      instrumentation_patch: patch,
      environment: environment(),
      baseline: command(10.0, SourceIdentityKind::BaselineCommit),
      candidate: command(9.0, SourceIdentityKind::CandidateIndex),
      evidence_root: evidence.clone(),
   };
   let report = oxide_perf_runner::paired::run_paired_workflow(plan).expect("run paired workflow");
   assert!(report.decision.accepted, "{:?}", report.decision.reasons);
   assert_eq!(report.decision.valid_pairs, 15);
   assert!(evidence.join("input.json").is_file());
   assert_eq!(fs::read_dir(&evidence).expect("read evidence").count(), 91);
   fs::remove_dir_all(root).expect("remove workflow root");
}
