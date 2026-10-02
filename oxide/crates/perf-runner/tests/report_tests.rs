use oxide_perf_runner::{
    assert_case_metric_contract, assert_contract_coverage, assert_report_repository_provenance,
    collect_suite_report,
    compare_reports, render_report_markdown, AuditFinding, ContractCoverageEntry,
    ContractCoverageReport, CoverageReport, PerfCaseResult, PerfReport, RepositoryProvenance,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn perf_runner_explicitly_enables_renderer_diagnostics()
{
   let manifest = include_str!("../Cargo.toml");
   assert!(manifest.contains(
      "oxide-renderer-web = { path = \"../renderer-web\", features = [\"diagnostic-instrumentation\"] }",
   ));
}

fn git(root: &Path, args: &[&str]) -> String
{
   let output = Command::new("git")
      .arg("-C")
      .arg(root)
      .args(args)
      .output()
      .unwrap_or_else(|error| panic!("run git {}: {error}", args.join(" ")));
   assert!(
      output.status.success(),
      "git {} failed: {}",
      args.join(" "),
      String::from_utf8_lossy(&output.stderr)
   );
   String::from_utf8(output.stdout)
      .unwrap_or_else(|error| panic!("decode git {} output: {error}", args.join(" ")))
      .trim()
      .to_string()
}

fn initialized_git_repository(label: &str) -> PathBuf
{
   let nonce = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .expect("system clock before epoch")
      .as_nanos();
   let root = std::env::temp_dir().join(format!(
      "oxide-perf-report-{label}-{}-{nonce}",
      std::process::id(),
   ));
   fs::create_dir_all(&root).expect("create temporary Git repository");
   git(&root, &["init", "-b", "main"]);
   git(&root, &["config", "user.name", "Oxide Perf Test"]);
   git(&root, &["config", "user.email", "oxide-perf@example.invalid"]);
   fs::create_dir_all(root.join("oxide")).expect("create nested workspace directory");
   fs::write(root.join("source.txt"), b"first\n").expect("write initial source");
   fs::write(root.join("oxide/nested.txt"), b"nested\n").expect("write nested source");
   git(&root, &["add", "."]);
   git(&root, &["commit", "-m", "initial source"]);
   root
}

fn sample_repository_provenance() -> RepositoryProvenance
{
   RepositoryProvenance {
      repository_ref: Some(String::from("refs/heads/main")),
      repository_head_commit: Some(String::from("1111111111111111111111111111111111111111")),
      repository_tree: Some(String::from("2222222222222222222222222222222222222222")),
   }
}

#[test]
fn repository_provenance_requires_clean_named_stable_source()
{
   let root = initialized_git_repository("source-revision");
   let nested = root.join("oxide");
   let repository_root = RepositoryProvenance::resolve_root(&nested).expect("resolve Git top level");
   let provenance = RepositoryProvenance::capture(&nested).expect("capture clean named source");

   assert_eq!(repository_root, fs::canonicalize(&root).expect("canonical repository root"));
   provenance.validate().expect("validate captured source");
   assert_eq!(provenance.repository_ref.as_deref(), Some("refs/heads/main"));
   provenance.ensure_unchanged(&nested).expect("unchanged source remains valid");

   fs::write(root.join("source.txt"), b"dirty\n").expect("dirty tracked source");
   let dirty = RepositoryProvenance::capture(&root).expect_err("dirty source must fail");
   assert!(dirty.to_string().contains("not clean"), "{dirty:#}");

   git(&root, &["add", "source.txt"]);
   git(&root, &["commit", "-m", "changed source"]);
   let drift = provenance.ensure_unchanged(&nested).expect_err("source drift must fail");
   assert!(drift.to_string().contains("changed during evidence capture"), "{drift:#}");

   git(&root, &["switch", "--detach"]);
   let detached = RepositoryProvenance::capture(&root).expect_err("detached source must fail");
   assert!(detached.to_string().contains("symbolic-ref"), "{detached:#}");

   fs::remove_dir_all(root).expect("remove temporary Git repository");
}

#[test]
fn repository_provenance_rejects_partial_or_malformed_triples()
{
   let partial = RepositoryProvenance {
      repository_ref: Some(String::from("refs/heads/main")),
      repository_head_commit: None,
      repository_tree: Some(String::from("2222222222222222222222222222222222222222")),
   };
   assert!(partial.validate().expect_err("partial source must fail").to_string().contains("HEAD"));

   let malformed = RepositoryProvenance {
      repository_ref: Some(String::from("main")),
      repository_head_commit: Some(String::from("not-a-commit")),
      repository_tree: Some(String::from("not-a-tree")),
   };
   assert!(malformed.validate().expect_err("malformed source must fail").to_string().contains("named branch"));
}

fn sample_case(id: &str, median: f64, threshold_pct: f64, gated: bool) -> PerfCaseResult {
    PerfCaseResult {
        id: id.to_string(),
        family: String::from("test"),
        layer: String::from("engine"),
        scenario: String::from("test"),
        variant: String::from("oxide"),
        cache_state: String::from("warm"),
        refresh_mode: String::from("offscreen"),
        unit: String::from("us/op"),
        gated,
        threshold_pct,
        median,
        p95: median,
        p99: median,
        min: median,
        max: median,
        mean: median,
        samples: 3,
        ops_per_sample: 1,
        notes: Vec::new(),
        metrics: BTreeMap::new(),
    }
}

fn sample_case_with_distribution(
    id: &str,
    median: f64,
    p95: f64,
    p99: f64,
    threshold_pct: f64,
    gated: bool,
) -> PerfCaseResult {
    PerfCaseResult {
        id: id.to_string(),
        family: String::from("test"),
        layer: String::from("engine"),
        scenario: String::from("test"),
        variant: String::from("oxide"),
        cache_state: String::from("warm"),
        refresh_mode: String::from("offscreen"),
        unit: String::from("us/op"),
        gated,
        threshold_pct,
        median,
        p95,
        p99,
        min: median,
        max: p99,
        mean: median,
        samples: 3,
        ops_per_sample: 1,
        notes: Vec::new(),
        metrics: BTreeMap::new(),
    }
}

fn sample_gpu_frame_case(id: &str) -> PerfCaseResult {
    let mut case = sample_case(id, 8.0, 0.10, true);
    case.family = String::from("scene-gpu");
    case.layer = String::from("flow");
    case.unit = String::from("ms/frame");
    case.metrics = sample_gpu_frame_metrics();
    case
}

fn sample_gpu_frame_metrics() -> BTreeMap<String, f64> {
    let mut metrics = BTreeMap::new();
    for prefix in ["frame_ms", "gpu_ms"] {
        metrics.insert(format!("{}_p50", prefix), 8.0);
        metrics.insert(format!("{}_p95", prefix), 9.0);
        metrics.insert(format!("{}_p99", prefix), 10.0);
        metrics.insert(format!("{}_peak", prefix), 11.0);
    }
    for refresh_hz in [60, 120] {
        let label = format!("{}hz", refresh_hz);
        metrics.insert(format!("frame_budget_{}_ms", label), 1000.0 / refresh_hz as f64);
        metrics.insert(format!("missed_frames_{}", label), 0.0);
        metrics.insert(format!("missed_frame_ratio_{}", label), 0.0);
        metrics.insert(format!("hitch_frames_{}", label), 0.0);
        metrics.insert(format!("hitch_ratio_{}", label), 0.0);
    }
    metrics
}


fn report_case_slice<'a>(report: &'a str, id: &str) -> &'a str {
    let marker = format!("\"id\": \"{id}\"");
    let start = report.find(&marker).unwrap_or_else(|| panic!("missing report case {id}"));
    let tail = &report[start..];
    let end = tail.find("\n    }").unwrap_or(tail.len());
    &tail[..end]
}

fn report_f64(section: &str, key: &str) -> f64 {
    let marker = format!("\"{key}\": ");
    let start =
        section.find(&marker).unwrap_or_else(|| panic!("missing numeric report field {key}"))
            + marker.len();
    let rest = &section[start..];
    let end = rest.find(|ch: char| ch == ',' || ch == '\n' || ch == '}').unwrap_or(rest.len());
    rest[..end]
        .trim()
        .parse::<f64>()
        .unwrap_or_else(|_| panic!("invalid numeric report field {key}"))
}

fn workspace_case<'a>(report: &'a PerfReport, id: &str) -> &'a PerfCaseResult {
    report
        .cases
        .iter()
        .find(|case| case.id == id)
        .unwrap_or_else(|| panic!("missing workspace case {id}"))
}

fn case_id_digest(ids: &[&str]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for id in ids {
        for byte in id.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= u64::from(b'\n');
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}





fn sample_report(cases: Vec<PerfCaseResult>) -> PerfReport {
    PerfReport {
        version: 1,
        suite: String::from("test"),
        generated_label: None,
        repository: RepositoryProvenance::default(),
        cases,
        coverage: CoverageReport {
            components_total: 1,
            components_covered: vec![String::from("Button")],
            animations_total: 1,
            animations_covered: vec![String::from("SpinnerSpin")],
            launch_total: 1,
            launch_covered: vec![String::from("Simple Home Cold Launch")],
            primitive_lifecycle_total: 1,
            primitive_lifecycle_covered: vec![String::from("Flat Rects Mount x10")],
            scenes_cpu_total: 1,
            scenes_cpu_covered: vec![String::from("Controls")],
            scenes_gpu_total: 1,
            scenes_gpu_covered: vec![String::from("Controls")],
            journeys_total: 1,
            journeys_covered: vec![String::from("Input Form Submit")],
            authoring_total: 1,
            authoring_covered: vec![String::from("Text Fields")],
            layout_total: 1,
            layout_covered: vec![String::from("Flat Grid Rotation Relayout")],
            text_input_total: 1,
            text_input_covered: vec![String::from("Large Editor Keystroke Burst")],
            image_pipeline_total: 1,
            image_pipeline_covered: vec![String::from("PNG Decode")],
            navigation_total: 1,
            navigation_covered: vec![String::from("Button Press Response")],
            reconcile_total: 1,
            reconcile_covered: vec![String::from("Single Node Mutation")],
            endurance_total: 1,
            endurance_covered: vec![String::from("Open Close Heavy Screen 100x")],
            stress_total: 1,
            stress_covered: vec![String::from("Flat Rects 10k Mount")],
            bridges_total: 1,
            bridges_covered: vec![String::from("Permission Callback Fanout")],
        },
        contract: ContractCoverageReport::default(),
        findings: vec![AuditFinding { status: String::from("fixed"), summary: String::from("ok") }],
    }
}


#[test]
fn markdown_metric_summary_preserves_priority_order_and_limit()
{
   let mut case = sample_case("cpu.report.metric_summary", 1.0, 0.10, true);
   for (name, value) in [
      ("zz_overflow", 9.0),
      ("gpu_ms_p99", 3.0),
      ("alpha_extra", 7.0),
      ("frame_ms_p50", 5.0),
      ("gpu_ms_p50", 1.0),
      ("hitch_ms_per_s", 4.0),
      ("z_extra", 8.0),
   ]
   {
      case.metrics.insert(String::from(name), value);
   }
   let report = sample_report(vec![case]);
   let markdown = render_report_markdown(&report, None);
   let expected = concat!(
      "`gpu_ms_p50=1.000; gpu_ms_p99=3.000; hitch_ms_per_s=4.000; ",
      "frame_ms_p50=5.000; alpha_extra=7.000; z_extra=8.000`"
   );

   assert!(markdown.contains(expected), "{markdown}");
   assert!(!markdown.contains("zz_overflow=9.000"), "{markdown}");
}

#[test]
fn markdown_reports_selected_case_count_without_catalog_fractions()
{
   let report = sample_report(vec![sample_case("cpu.report.selected", 1.0, 0.10, true)]);
   let markdown = render_report_markdown(&report, None);

   assert!(markdown.contains("- Cases: `1`"), "{markdown}");
   assert!(!markdown.contains("- Coverage:"), "{markdown}");
}

#[test]
fn comparison_rejection_precedes_report_output_resolution_and_writes()
{
   let source = include_str!("../src/lib.rs");
   let body = source
      .split_once("fn run_suite(cli: Cli)")
      .and_then(|(_, tail)| tail.split_once("pub fn collect_suite_report"))
      .map(|(body, _)| body)
      .expect("run-suite source body");
   let rejection = body
      .find("performance comparison failed; existing report outputs were preserved")
      .expect("comparison rejection gate");

   for output in [
      "let json_out =",
      "let markdown_out =",
      "workspace_baseline_outputs(",
      "promote_files_atomically(&outputs)",
      "write_report_json(path, &report)",
      "write_markdown_outputs(path, &report",
   ]
   {
      let output = body.find(output).unwrap_or_else(|| panic!("missing output path `{output}`"));
      assert!(rejection < output, "comparison rejection follows `{output}`");
   }
}

#[test]
fn canonical_workspace_reports_use_one_atomic_promotion()
{
   let source = include_str!("../src/lib.rs");
   let body = source
      .split_once("fn run_suite(cli: Cli)")
      .and_then(|(_, tail)| tail.split_once("pub fn collect_suite_report"))
      .map(|(body, _)| body)
      .expect("run-suite source body");
   let baseline_branch = body
      .split_once("if cli.write_baseline")
      .map(|(_, tail)| tail)
      .expect("baseline publication branch");
   let prepare = baseline_branch
      .find("workspace_baseline_outputs(")
      .expect("workspace baseline preparation");
   let promote = baseline_branch
      .find("promote_files_atomically(&outputs)")
      .expect("workspace atomic promotion");

   assert!(prepare < promote);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn rejected_comparison_preserves_existing_report_outputs()
{
   let nonce = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .expect("system clock before epoch")
      .as_nanos();
   let root = std::env::temp_dir().join(format!(
      "oxide-perf-rejected-output-{}-{nonce}",
      std::process::id(),
   ));
   fs::create_dir_all(&root).expect("create rejected-output fixture");
   let baseline = root.join("baseline.json");
   let json_out = root.join("current.json");
   let markdown_out = root.join("current.md");
   let json_sentinel = b"preserve-json\n";
   let markdown_sentinel = b"preserve-markdown\n";
   fs::write(
      &baseline,
      serde_json::to_vec(&sample_report(Vec::new())).expect("serialize empty baseline"),
   )
   .expect("write empty baseline");
   fs::write(&json_out, json_sentinel).expect("write JSON sentinel");
   fs::write(&markdown_out, markdown_sentinel).expect("write Markdown sentinel");

   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .env("OXIDE_PERF_RUNNER_FILTER", "cpu.component.label.encode")
      .env_remove("PERF_REPORT_DATE")
      .arg("--run-suite")
      .arg("--smoke")
      .arg("--compare")
      .arg(&baseline)
      .arg("--json-out")
      .arg(&json_out)
      .arg("--markdown-out")
      .arg(&markdown_out)
      .output()
      .expect("run rejected comparison");
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(!output.status.success(), "comparison unexpectedly succeeded");
   assert!(stderr.contains("existing report outputs were preserved"), "stderr: {stderr}");
   assert_eq!(fs::read(&json_out).expect("read JSON sentinel"), json_sentinel);
   assert_eq!(fs::read(&markdown_out).expect("read Markdown sentinel"), markdown_sentinel);

   fs::remove_dir_all(root).expect("remove rejected-output fixture");
}

#[test]
fn source_bound_report_serializes_and_renders_repository_revision()
{
   let mut report = sample_report(vec![sample_case("cpu.report.source", 1.0, 0.10, true)]);
   report.version = 2;
   report.repository = sample_repository_provenance();
   let json = serde_json::to_value(&report).expect("serialize source-bound report");
   let markdown = render_report_markdown(&report, None);

   assert_eq!(json["repository_ref"], "refs/heads/main");
   assert_eq!(json["repository_head_commit"], "1111111111111111111111111111111111111111");
   assert_eq!(json["repository_tree"], "2222222222222222222222222222222222222222");
   assert!(markdown.contains("- Repository ref: `refs/heads/main`"), "{markdown}");
   assert!(markdown.contains("- Repository HEAD: `1111111111111111111111111111111111111111`"), "{markdown}");
   assert!(markdown.contains("- Repository tree: `2222222222222222222222222222222222222222`"), "{markdown}");
}

#[test]
fn version_two_report_rejects_missing_repository_revision()
{
   let mut report = sample_report(Vec::new());
   report.version = 2;

   let validation = assert_report_repository_provenance(report.version, &report.repository)
      .expect_err("version 2 validation without source must fail");
   assert!(format!("{validation:#}").contains("missing repository ref"), "{validation:#}");
   let error = serde_json::to_value(&report).expect_err("version 2 without source must fail");
   assert!(error.to_string().contains("validating version 2"), "{error:#}");
}

#[test]
fn historical_report_omits_and_defaults_repository_revision()
{
   let report = sample_report(Vec::new());
   let json = serde_json::to_value(&report).expect("serialize historical report");
   let decoded: PerfReport = serde_json::from_value(json.clone()).expect("decode historical report");

   assert!(json.get("repository_ref").is_none());
   assert!(json.get("repository_head_commit").is_none());
   assert!(json.get("repository_tree").is_none());
   assert_eq!(decoded.repository, RepositoryProvenance::default());
}

#[test]
fn compare_reports_flags_regressions_and_missing_baselines() {
    let current = sample_report(vec![
        sample_case("cpu.component.button.encode", 12.5, 0.10, true),
        sample_case("cpu.component.label.encode", 8.0, 0.10, true),
    ]);
    let baseline =
        sample_report(vec![sample_case("cpu.component.button.encode", 10.0, 0.10, true)]);

    let comparison = compare_reports(&current, &baseline);

    assert_eq!(comparison.matched, 1);
    assert_eq!(comparison.regressions.len(), 1);
    assert_eq!(comparison.regressions[0].id, "cpu.component.button.encode");
    assert_eq!(comparison.missing_baseline, vec![String::from("cpu.component.label.encode")]);
}

#[test]
fn compare_reports_uses_high_variance_baseline_envelope() {
    let current = sample_report(vec![sample_case("gpu.scene.damage_lab.frame", 18.0, 0.10, true)]);
    let baseline = sample_report(vec![sample_case_with_distribution(
        "gpu.scene.damage_lab.frame",
        10.0,
        20.0,
        21.0,
        0.10,
        true,
    )]);

    let comparison = compare_reports(&current, &baseline);

    assert_eq!(comparison.matched, 1);
    assert!(comparison.regressions.is_empty());
}

#[test]
fn compare_reports_still_flags_regressions_above_high_variance_envelope() {
    let current = sample_report(vec![sample_case("gpu.scene.damage_lab.frame", 22.5, 0.10, true)]);
    let baseline = sample_report(vec![sample_case_with_distribution(
        "gpu.scene.damage_lab.frame",
        10.0,
        20.0,
        21.0,
        0.10,
        true,
    )]);

    let comparison = compare_reports(&current, &baseline);

    assert_eq!(comparison.matched, 1);
    assert_eq!(comparison.regressions.len(), 1);
    assert_eq!(comparison.regressions[0].id, "gpu.scene.damage_lab.frame");
}

#[test]
fn compare_reports_large_baseline_keeps_regression_semantics()
{
   let mut baseline_cases = Vec::new();
   for index in 0..40
   {
      baseline_cases.push(sample_case(&format!("cpu.compare.baseline.{}", index), 10.0, 0.10, true));
   }
   let current = sample_report(vec![
      sample_case("cpu.compare.baseline.39", 12.0, 0.10, true),
      sample_case("cpu.compare.missing", 1.0, 0.10, true),
   ]);
   let baseline = sample_report(baseline_cases);

   let comparison = compare_reports(&current, &baseline);

   assert_eq!(comparison.matched, 1);
   assert_eq!(comparison.regressions.len(), 1);
   assert_eq!(comparison.regressions[0].id, "cpu.compare.baseline.39");
   assert_eq!(comparison.missing_baseline, vec![String::from("cpu.compare.missing")]);
}

#[test]
fn compare_reports_large_same_order_baseline_keeps_regression_semantics()
{
   let mut baseline_cases = Vec::new();
   let mut current_cases = Vec::new();
   for index in 0..40
   {
      let id = format!("cpu.compare.same_order.{}", index);
      baseline_cases.push(sample_case(&id, 10.0, 0.10, index != 7));
      let median = match index {
         3 => 8.0,
         39 => 12.0,
         _ => 10.0,
      };
      current_cases.push(sample_case(&id, median, 0.10, index != 7));
   }
   let current = sample_report(current_cases);
   let baseline = sample_report(baseline_cases);

   let comparison = compare_reports(&current, &baseline);

   assert_eq!(comparison.matched, 39);
   assert_eq!(comparison.regressions.len(), 1);
   assert_eq!(comparison.regressions[0].id, "cpu.compare.same_order.39");
   assert!(comparison.missing_baseline.is_empty());
   assert_eq!(comparison.improvements, vec![String::from("cpu.compare.same_order.3")]);
}

#[test]
fn compare_reports_large_reordered_same_length_baseline_keeps_lookup_semantics()
{
   let mut baseline_cases = Vec::new();
   let mut current_cases = Vec::new();
   for index in 0..40
   {
      let id = format!("cpu.compare.reordered.{}", index);
      let baseline_median = if index == 5 {
         100.0
      } else {
         10.0
      };
      let current_median = match index {
         5 => 80.0,
         13 => 12.0,
         _ => 10.0,
      };
      baseline_cases.push(sample_case(&id, baseline_median, 0.10, true));
      current_cases.push(sample_case(&id, current_median, 0.10, true));
   }
   baseline_cases.swap(5, 13);
   let current = sample_report(current_cases);
   let baseline = sample_report(baseline_cases);

   let comparison = compare_reports(&current, &baseline);

   assert_eq!(comparison.matched, 40);
   assert_eq!(comparison.regressions.len(), 1);
   assert_eq!(comparison.regressions[0].id, "cpu.compare.reordered.13");
   assert!(comparison.missing_baseline.is_empty());
   assert_eq!(comparison.improvements, vec![String::from("cpu.compare.reordered.5")]);
}

#[test]
fn contract_coverage_rejects_implemented_rows_with_gap_notes() {
    let contract = ContractCoverageReport {
        layers: vec![ContractCoverageEntry {
            id: String::from("flow"),
            label: String::from("Representative Screen Flows"),
            status: String::from("implemented"),
            notes: vec![String::from("Flow coverage is still incomplete for hitch metrics.")],
        }],
        battery: Vec::new(),
        notes: Vec::new(),
    };

    assert!(assert_contract_coverage(&contract).is_err());
}

#[test]
fn contract_coverage_rejects_case_insensitive_gap_notes() {
    let contract = ContractCoverageReport {
        layers: vec![ContractCoverageEntry {
            id: String::from("flow"),
            label: String::from("Representative Screen Flows"),
            status: String::from("implemented"),
            notes: vec![String::from("Flow coverage has No Dedicated hitch row.")],
        }],
        battery: Vec::new(),
        notes: Vec::new(),
    };

    assert!(assert_contract_coverage(&contract).is_err());
}

#[test]
fn contract_coverage_allows_explicit_partial_gap_rows() {
    let contract = ContractCoverageReport {
        layers: vec![ContractCoverageEntry {
            id: String::from("flow"),
            label: String::from("Representative Screen Flows"),
            status: String::from("partial"),
            notes: vec![String::from("Flow coverage is still incomplete for hitch metrics.")],
        }],
        battery: Vec::new(),
        notes: Vec::new(),
    };

    assert!(assert_contract_coverage(&contract).is_ok());
}

#[test]
fn case_metric_contract_rejects_gpu_frame_rows_without_pacing_metrics() {
    let mut case = sample_gpu_frame_case("gpu.scene.controls.frame");
    case.metrics.remove("missed_frame_ratio_120hz");

    let result = assert_case_metric_contract(&[case]);

    assert!(result.is_err());
}

#[test]
fn case_metric_contract_rejects_frame_rows_without_frame_distribution() {
    let mut case = sample_gpu_frame_case("cpu.journey.feed_scroll.frame");
    case.id = String::from("cpu.journey.feed_scroll.frame");
    case.family = String::from("journey");
    case.metrics.remove("frame_ms_p99");

    let result = assert_case_metric_contract(&[case]);

    assert!(result.is_err());
}

#[test]
fn case_metric_contract_rejects_gpu_frame_rows_without_gpu_distribution() {
    let mut case = sample_gpu_frame_case("gpu.scene.controls.frame");
    case.metrics.remove("gpu_ms_p99");

    let result = assert_case_metric_contract(&[case]);

    assert!(result.is_err());
}

#[test]
fn case_metric_contract_accepts_gpu_frame_rows_with_gpu_and_pacing_metrics() {
    let cases = vec![
        sample_gpu_frame_case("gpu.scene.controls.frame"),
        sample_gpu_frame_case("gpu.animation.effects.refresh_matrix"),
        sample_gpu_frame_case("gpu.authoring.scene3d.mixed_frame"),
        sample_case("cpu.component.button.encode", 12.0, 0.10, true),
    ];

    assert!(assert_case_metric_contract(&cases).is_ok());
}

fn web_report_case<'a>(report: &'a Value, id: &str) -> &'a Value {
    let cases = report["cases"].as_array().expect("web report cases array");
    cases
        .iter()
        .find(|case| case["id"].as_str() == Some(id))
        .unwrap_or_else(|| panic!("missing web report case {id}"))
}

fn web_report_case_optional<'a>(report: &'a Value, id: &str) -> Option<&'a Value> {
    report["cases"]
        .as_array()
        .expect("web report cases array")
        .iter()
        .find(|case| case["id"].as_str() == Some(id))
}

fn web_report_number(value: &Value, key: &str) -> f64 {
    let number = value[key].as_f64().unwrap_or_else(|| panic!("missing numeric web field {key}"));
    assert!(number.is_finite(), "non-finite web field {key}: {number}");
    number
}

fn assert_web_report_zero_resource_churn(case: &Value, allow_buffer_grows: bool) {
    assert_eq!(web_report_number(case, "pipeline_creates"), 0.0);
    assert_eq!(web_report_number(case, "bind_group_creates"), 0.0);
    assert_eq!(web_report_number(case, "texture_creates"), 0.0);
    assert_eq!(web_report_number(case, "sampler_creates"), 0.0);
    for field in [
        "draw_buffer_grows",
        "image_texture_creates",
        "image_bind_group_creates",
        "target_texture_creates",
        "target_bind_group_creates",
        "layer_texture_creates",
        "layer_bind_group_creates",
        "scene3d_bind_group_creates",
        "effect_buffer_grows",
        "effect_bind_group_creates",
        "id_mask_texture_creates",
        "id_mask_buffer_grows",
        "id_mask_bind_group_creates",
    ] {
        assert_eq!(web_report_number(case, field), 0.0);
    }
    assert_eq!(web_report_number(case, "cpu_scratch_grows"), 0.0);
    assert_eq!(web_report_number(case, "cpu_scratch_growth_bytes"), 0.0);
    for field in [
        "cpu_draw_scratch_grows",
        "cpu_draw_scratch_growth_bytes",
        "cpu_scene3d_scratch_grows",
        "cpu_scene3d_scratch_growth_bytes",
        "cpu_effect_scratch_grows",
        "cpu_effect_scratch_growth_bytes",
        "cpu_id_mask_scratch_grows",
        "cpu_id_mask_scratch_growth_bytes",
        "cpu_image_upload_scratch_grows",
        "cpu_image_upload_scratch_growth_bytes",
        "cpu_resource_table_scratch_grows",
        "cpu_resource_table_scratch_growth_bytes",
    ] {
        assert_eq!(web_report_number(case, field), 0.0);
    }
    if allow_buffer_grows {
        assert!(web_report_number(case, "buffer_grows") > 0.0);
    } else {
        assert_eq!(web_report_number(case, "buffer_grows"), 0.0);
        assert_eq!(web_report_number(case, "scene3d_buffer_grows"), 0.0);
        assert_eq!(web_report_number(case, "id_mask_buffer_grows"), 0.0);
    }
}

fn assert_web_renderer_case_contract(case: &Value) {
    assert_eq!(case["unit"].as_str(), Some("ms/cpu-submit"));
    assert_eq!(case["cache_state"].as_str(), Some("warm"));
    assert_eq!(case["refresh_mode"].as_str(), Some("unpaced-tight-loop"));
    assert!(web_report_number(case, "samples") > 0.0);
    assert!(web_report_number(case, "frames_per_sample") > 0.0);
    assert!(web_report_number(case, "frames") > 0.0);
    for key in ["p50_ms", "p95_ms", "p99_ms", "peak_ms", "avg_ms"] {
        assert!(web_report_number(case, key) >= 0.0);
    }
    assert!(web_report_number(case, "p50_ms") > 0.0);
    assert!(web_report_number(case, "p95_ms") >= web_report_number(case, "p50_ms"));
    assert!(web_report_number(case, "p99_ms") >= web_report_number(case, "p95_ms"));
    assert!(web_report_number(case, "peak_ms") >= web_report_number(case, "p99_ms"));
    for key in [
        "draws",
        "draw_items",
        "draw_items_coalesced",
        "draw_pipeline_binds",
        "draw_bind_group_binds",
        "draw_scissor_sets",
        "solid_tris",
        "image_draws",
        "image_mesh_draws",
        "nine_slice_draws",
        "glyph_quads",
        "sdf_glyph_quads",
        "clip_depth_peak",
        "damage_rects",
        "layer_draws",
        "layer_cache_hits",
        "layer_cache_misses",
        "layer_cache_skipped_draws",
        "layer_passes",
        "scene3d_draws",
        "id_mask_draws",
        "backdrop_draws",
        "visual_effect_draws",
        "effect_uniform_writes",
        "effect_uniform_bytes",
        "effect_uniform_slots",
        "spinner_draws",
        "camera_bg_draws",
        "render_passes",
        "clear_passes",
        "draw_passes",
        "scene3d_passes",
        "scene3d_overlay_passes",
        "id_mask_raster_passes",
        "id_mask_field_seed_passes",
        "id_mask_field_jump_passes",
        "id_mask_compositor_passes",
        "present_passes",
        "texture_copies",
        "command_buffers",
        "gpu_timestamp_query_supported",
        "gpu_timestamp_frame_id",
        "gpu_timestamp_passes",
        "gpu_timestamp_total_ns",
        "gpu_timestamp_clear_ns",
        "gpu_timestamp_draw_ns",
        "gpu_timestamp_scene3d_ns",
        "gpu_timestamp_scene3d_overlay_ns",
        "gpu_timestamp_id_mask_raster_ns",
        "gpu_timestamp_id_mask_field_seed_ns",
        "gpu_timestamp_id_mask_field_jump_ns",
        "gpu_timestamp_id_mask_compositor_ns",
        "gpu_timestamp_present_ns",
        "gpu_timestamp_max_pass_ns",
        "gpu_timestamp_readback_skips",
        "gpu_timestamp_readback_interval",
        "buffer_upload_bytes",
        "texture_upload_bytes",
        "buffer_grows",
        "texture_creates",
        "bind_group_creates",
        "pipeline_creates",
        "sampler_creates",
        "mesh3d_creates",
        "wasm_alloc_count",
        "wasm_alloc_bytes",
        "wasm_dealloc_count",
        "wasm_dealloc_bytes",
        "wasm_realloc_count",
        "wasm_realloc_grow_bytes",
        "wasm_realloc_shrink_bytes",
        "wasm_allocating_frames",
        "wasm_peak_frame_alloc_bytes",
        "draw_buffer_grows",
        "image_texture_creates",
        "image_bind_group_creates",
        "target_texture_creates",
        "target_bind_group_creates",
        "layer_texture_creates",
        "layer_bind_group_creates",
        "scene3d_buffer_grows",
        "scene3d_bind_group_creates",
        "effect_buffer_grows",
        "effect_bind_group_creates",
        "id_mask_texture_creates",
        "id_mask_buffer_grows",
        "id_mask_bind_group_creates",
        "image_upload_temp_allocs",
        "image_upload_temp_bytes",
        "image_upload_scratch_bytes",
        "image_upload_scratch_grows",
        "cpu_scratch_bytes",
        "cpu_scratch_grows",
        "cpu_scratch_growth_bytes",
        "cpu_draw_scratch_bytes",
        "cpu_draw_scratch_grows",
        "cpu_draw_scratch_growth_bytes",
        "cpu_scene3d_scratch_bytes",
        "cpu_scene3d_scratch_grows",
        "cpu_scene3d_scratch_growth_bytes",
        "cpu_effect_scratch_bytes",
        "cpu_effect_scratch_grows",
        "cpu_effect_scratch_growth_bytes",
        "cpu_id_mask_scratch_bytes",
        "cpu_id_mask_scratch_grows",
        "cpu_id_mask_scratch_growth_bytes",
        "cpu_image_upload_scratch_bytes",
        "cpu_image_upload_scratch_grows",
        "cpu_image_upload_scratch_growth_bytes",
        "cpu_resource_table_scratch_bytes",
        "cpu_resource_table_scratch_grows",
        "cpu_resource_table_scratch_growth_bytes",
    ] {
        assert!(web_report_number(case, key) >= 0.0, "{} missing or negative", key);
    }
    assert!(web_report_number(case, "command_buffers") > 0.0);
    assert!(web_report_number(case, "render_passes") > 0.0);
    let pass_family_total = web_report_number(case, "clear_passes")
        + web_report_number(case, "draw_passes")
        + web_report_number(case, "scene3d_passes")
        + web_report_number(case, "scene3d_overlay_passes")
        + web_report_number(case, "id_mask_raster_passes")
        + web_report_number(case, "id_mask_field_seed_passes")
        + web_report_number(case, "id_mask_field_jump_passes")
        + web_report_number(case, "id_mask_compositor_passes")
        + web_report_number(case, "present_passes");
    assert_eq!(pass_family_total, web_report_number(case, "render_passes"));
    if web_report_number(case, "gpu_timestamp_query_supported") > 0.0
        && web_report_number(case, "gpu_timestamp_passes") > 0.0
    {
        assert_eq!(
            web_report_number(case, "gpu_timestamp_passes"),
            web_report_number(case, "render_passes"),
        );
        assert!(web_report_number(case, "gpu_timestamp_readback_interval") >= 1.0);
    }
    assert!(web_report_number(case, "cpu_scratch_bytes") > 0.0);
    assert!(web_report_number(case, "cpu_draw_scratch_bytes") > 0.0);
    assert!(web_report_number(case, "cpu_resource_table_scratch_bytes") > 0.0);
    assert_eq!(
        web_report_number(case, "bind_group_creates"),
        web_report_number(case, "image_bind_group_creates")
            + web_report_number(case, "target_bind_group_creates")
            + web_report_number(case, "layer_bind_group_creates")
            + web_report_number(case, "scene3d_bind_group_creates")
            + web_report_number(case, "effect_bind_group_creates")
            + web_report_number(case, "id_mask_bind_group_creates"),
    );
    assert_eq!(
        web_report_number(case, "texture_creates"),
        web_report_number(case, "image_texture_creates")
            + web_report_number(case, "target_texture_creates")
            + web_report_number(case, "layer_texture_creates")
            + web_report_number(case, "id_mask_texture_creates"),
    );
    assert_eq!(
        web_report_number(case, "buffer_grows"),
        web_report_number(case, "draw_buffer_grows")
            + web_report_number(case, "scene3d_buffer_grows")
            + web_report_number(case, "effect_buffer_grows")
            + web_report_number(case, "id_mask_buffer_grows"),
    );
}

#[test]
fn markdown_prioritizes_gpu_distribution_and_frame_pacing_metrics() {
    let report = sample_report(vec![sample_gpu_frame_case("gpu.scene.controls.frame")]);
    let markdown = render_report_markdown(&report, None);

    assert!(markdown.contains("gpu_ms_p50=8.000"), "{markdown}");
    assert!(markdown.contains("gpu_ms_p95=9.000"), "{markdown}");
    assert!(markdown.contains("missed_frame_ratio_120hz=0.000"), "{markdown}");
    assert!(markdown.contains("hitch_ratio_120hz=0.000"), "{markdown}");
}

#[test]
fn markdown_render_bench_cli_loads_report_without_running_suite() {
    let mut json_out = std::env::temp_dir();
    json_out.push(format!("oxide-perf-runner-markdown-bench-{}.json", std::process::id()));
    let report = sample_report(vec![sample_gpu_frame_case("gpu.scene.controls.frame")]);
    let bytes = serde_json::to_vec(&report).expect("serialize sample report");
    std::fs::write(&json_out, bytes).expect("write sample report");

    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .arg("--bench-markdown-render")
        .arg(&json_out)
        .arg("--bench-markdown-iters")
        .arg("2")
        .output()
        .expect("run markdown render bench");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "markdown render bench failed: {stderr}");
    assert!(stdout.contains("markdown_render_bench"), "stdout: {stdout}");
    assert!(stdout.contains("iterations=2"), "stdout: {stdout}");
    assert!(stdout.contains("us_per_iter="), "stdout: {stdout}");
    assert!(stdout.contains("bytes_per_iter="), "stdout: {stdout}");
    assert!(!stdout.contains("suite="), "stdout: {stdout}");
    let _ = std::fs::remove_file(json_out);
}

#[test]
fn markdown_render_bench_cli_loads_comparison_baseline() {
    let mut current_out = std::env::temp_dir();
    current_out.push(format!("oxide-perf-runner-markdown-current-{}.json", std::process::id()));
    let mut baseline_out = std::env::temp_dir();
    baseline_out.push(format!("oxide-perf-runner-markdown-baseline-{}.json", std::process::id()));

    let current = sample_report(vec![
        sample_gpu_frame_case("gpu.scene.controls.frame"),
        sample_case("cpu.component.label.encode", 4.0, 0.10, true),
    ]);
    let baseline = sample_report(vec![sample_gpu_frame_case("gpu.scene.controls.frame")]);
    std::fs::write(&current_out, serde_json::to_vec(&current).expect("serialize current"))
        .expect("write current report");
    std::fs::write(&baseline_out, serde_json::to_vec(&baseline).expect("serialize baseline"))
        .expect("write baseline report");

    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .arg("--bench-markdown-render")
        .arg(&current_out)
        .arg("--bench-markdown-compare")
        .arg(&baseline_out)
        .arg("--bench-markdown-iters")
        .arg("2")
        .output()
        .expect("run markdown render bench with comparison");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "markdown comparison render bench failed: {stderr}");
    assert!(stdout.contains("markdown_render_bench"), "stdout: {stdout}");
    assert!(stdout.contains("matched=1"), "stdout: {stdout}");
    assert!(stdout.contains("missing_baseline=1"), "stdout: {stdout}");
    assert!(stdout.contains("iterations=2"), "stdout: {stdout}");
    assert!(!stdout.contains("suite="), "stdout: {stdout}");
    let _ = std::fs::remove_file(current_out);
    let _ = std::fs::remove_file(baseline_out);
}

#[test]
fn json_render_bench_cli_loads_report_without_running_suite()
{
   let mut json_out = std::env::temp_dir();
   json_out.push(format!("oxide-perf-runner-json-bench-{}.json", std::process::id()));
   let report = sample_report(vec![sample_gpu_frame_case("gpu.scene.controls.frame")]);
   let bytes = serde_json::to_vec(&report).expect("serialize sample report");
   std::fs::write(&json_out, bytes).expect("write sample report");

   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .arg("--bench-json-render")
      .arg(&json_out)
      .arg("--bench-json-iters")
      .arg("2")
      .output()
      .expect("run json render bench");
   let stdout = String::from_utf8_lossy(&output.stdout);
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(output.status.success(), "json render bench failed: {stderr}");
   assert!(stdout.contains("json_render_bench"), "stdout: {stdout}");
   assert!(stdout.contains("iterations=2"), "stdout: {stdout}");
   assert!(stdout.contains("us_per_iter="), "stdout: {stdout}");
   assert!(stdout.contains("bytes_per_iter="), "stdout: {stdout}");
   assert!(stdout.contains("total_bytes="), "stdout: {stdout}");
   assert!(!stdout.contains("suite="), "stdout: {stdout}");
   let _ = std::fs::remove_file(json_out);
}

#[test]
fn json_string_render_bench_cli_loads_report_without_running_suite()
{
   let mut json_out = std::env::temp_dir();
   json_out.push(format!("oxide-perf-runner-json-string-bench-{}.json", std::process::id()));
   let report = sample_report(vec![sample_gpu_frame_case("gpu.scene.controls.frame")]);
   let bytes = serde_json::to_vec(&report).expect("serialize sample report");
   std::fs::write(&json_out, bytes).expect("write sample report");

   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .arg("--bench-json-string-render")
      .arg(&json_out)
      .arg("--bench-json-iters")
      .arg("2")
      .output()
      .expect("run json string render bench");
   let stdout = String::from_utf8_lossy(&output.stdout);
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(output.status.success(), "json string render bench failed: {stderr}");
   assert!(stdout.contains("json_string_render_bench"), "stdout: {stdout}");
   assert!(stdout.contains("iterations=2"), "stdout: {stdout}");
   assert!(stdout.contains("us_per_iter="), "stdout: {stdout}");
   assert!(stdout.contains("bytes_per_iter="), "stdout: {stdout}");
   assert!(stdout.contains("total_bytes="), "stdout: {stdout}");
   assert!(!stdout.contains("suite="), "stdout: {stdout}");
   let _ = std::fs::remove_file(json_out);
}

#[test]
fn markdown_write_bench_cli_loads_comparison_baseline() {
    let mut current_out = std::env::temp_dir();
    current_out.push(format!("oxide-perf-runner-markdown-write-current-{}.json", std::process::id()));
    let mut baseline_out = std::env::temp_dir();
    baseline_out.push(format!("oxide-perf-runner-markdown-write-baseline-{}.json", std::process::id()));

    let current = sample_report(vec![
        sample_gpu_frame_case("gpu.scene.controls.frame"),
        sample_case("cpu.component.label.encode", 4.0, 0.10, true),
    ]);
    let baseline = sample_report(vec![sample_gpu_frame_case("gpu.scene.controls.frame")]);
    std::fs::write(&current_out, serde_json::to_vec(&current).expect("serialize current"))
        .expect("write current report");
    std::fs::write(&baseline_out, serde_json::to_vec(&baseline).expect("serialize baseline"))
        .expect("write baseline report");

    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .arg("--bench-markdown-write")
        .arg(&current_out)
        .arg("--bench-markdown-compare")
        .arg(&baseline_out)
        .arg("--bench-markdown-iters")
        .arg("2")
        .output()
        .expect("run markdown write bench with comparison");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "markdown write bench failed: {stderr}");
    assert!(stdout.contains("markdown_write_bench"), "stdout: {stdout}");
    assert!(stdout.contains("matched=1"), "stdout: {stdout}");
    assert!(stdout.contains("missing_baseline=1"), "stdout: {stdout}");
    assert!(stdout.contains("iterations=2"), "stdout: {stdout}");
    assert!(stdout.contains("bytes_per_iter="), "stdout: {stdout}");
    assert!(!stdout.contains("suite="), "stdout: {stdout}");
    let _ = std::fs::remove_file(current_out);
    let _ = std::fs::remove_file(baseline_out);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn markdown_out_writes_only_the_requested_report() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "oxide-perf-runner-markdown-out-{}-{nonce}",
        std::process::id(),
    ));
    std::fs::create_dir_all(&dir).expect("create markdown output dir");
    let latest = dir.join("latest.md");
    let dated = dir.join("2099-01-02.md");

    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "cpu.component.label.encode")
        .env("PERF_REPORT_DATE", "2099-01-02")
        .arg("--run-suite")
        .arg("--smoke")
        .arg("--markdown-out")
        .arg(&latest)
        .output()
        .expect("run filtered markdown output suite");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered markdown output suite failed: {stderr}");
    let latest_body = std::fs::read(&latest).expect("read latest markdown");
    assert!(!dated.exists(), "report output must not create a dated duplicate");
    assert!(!latest_body.is_empty());

    let _ = std::fs::remove_file(latest);
    let _ = std::fs::remove_dir(dir);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_registry_cases_do_not_expand_siblings()
{
   for (filter, expected, sibling) in [
      (
         "cpu.component.label.encode",
         "case=cpu.component.label.encode",
         "case=cpu.component.button.encode",
      ),
      (
         "cpu.animation.image_zoom_pan",
         "case=cpu.animation.image_zoom_pan",
         "case=cpu.animation.spinner_spin",
      ),
   ]
   {
      let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
         .env("OXIDE_PERF_RUNNER_FILTER", filter)
         .arg("--smoke")
         .output()
         .expect("run filtered registry case");
      let stdout = String::from_utf8_lossy(&output.stdout);
      let stderr = String::from_utf8_lossy(&output.stderr);

      assert!(output.status.success(), "filtered registry case failed: {stderr}");
      assert!(stdout.contains("cases=1"), "stdout: {stdout}");
      assert!(stdout.contains(expected), "stdout: {stdout}");
      assert!(!stdout.contains(sibling), "stdout: {stdout}");
   }
}

#[test]
fn markdown_render_bench_iters_requires_report_path() {
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .arg("--bench-markdown-iters")
        .arg("2")
        .output()
        .expect("run markdown render bench without report path");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "markdown bench unexpectedly succeeded");
    assert!(
        stderr.contains("--bench-markdown-iters requires --bench-markdown-render or --bench-markdown-write"),
        "stderr: {stderr}"
    );
}

#[test]
fn markdown_render_bench_compare_requires_report_path() {
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .arg("--bench-markdown-compare")
        .arg("baseline.json")
        .output()
        .expect("run markdown comparison bench without report path");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "markdown comparison bench unexpectedly succeeded");
    assert!(
        stderr.contains("--bench-markdown-compare requires --bench-markdown-render or --bench-markdown-write"),
        "stderr: {stderr}"
    );
}

#[test]
fn json_render_bench_iters_requires_report_path()
{
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .arg("--bench-json-iters")
      .arg("2")
      .output()
      .expect("run json render bench without report path");
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(!output.status.success(), "json bench unexpectedly succeeded");
   assert!(
      stderr.contains("--bench-json-iters requires --bench-json-render or --bench-json-string-render"),
      "stderr: {stderr}"
   );
}

#[test]
fn sample_summary_bench_cli_reports_summary_counts()
{
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .arg("--bench-sample-summary")
      .arg("--bench-sample-summary-iters")
      .arg("2")
      .output()
      .expect("run sample summary bench");
   let stdout = String::from_utf8_lossy(&output.stdout);
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(output.status.success(), "sample summary bench failed: {stderr}");
   assert!(stdout.contains("sample_summary_bench"), "stdout: {stdout}");
   assert!(stdout.contains("iterations=2"), "stdout: {stdout}");
   assert!(stdout.contains("groups=4"), "stdout: {stdout}");
   assert!(stdout.contains("summaries_per_iter=4"), "stdout: {stdout}");
   assert!(stdout.contains("checksum="), "stdout: {stdout}");
   assert!(!stdout.contains("suite="), "stdout: {stdout}");
}

#[test]
fn sample_summary_bench_iters_requires_bench_flag()
{
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .arg("--bench-sample-summary-iters")
      .arg("2")
      .output()
      .expect("run sample summary bench without bench flag");
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(!output.status.success(), "sample summary bench unexpectedly succeeded");
   assert!(
      stderr.contains("--bench-sample-summary-iters requires --bench-sample-summary"),
      "stderr: {stderr}"
   );
}

#[test]
fn case_filter_bench_cli_reports_filter_counts() {
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env(
            "OXIDE_PERF_RUNNER_FILTER",
            "cpu.system.,gpu.scene.damage_lab.frame,cpu.authoring.collection_",
        )
        .arg("--bench-case-filter")
        .arg("--bench-case-filter-iters")
        .arg("2")
        .output()
        .expect("run case filter bench");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "case filter bench failed: {stderr}");
    assert!(stdout.contains("case_filter_bench"), "stdout: {stdout}");
    assert!(stdout.contains("iterations=2"), "stdout: {stdout}");
    assert!(stdout.contains("allowed_per_iter=5"), "stdout: {stdout}");
    assert!(stdout.contains("prefix_allowed_per_iter=3"), "stdout: {stdout}");
    assert!(stdout.contains("checksum="), "stdout: {stdout}");
    assert!(!stdout.contains("suite="), "stdout: {stdout}");
}

#[test]
fn case_filter_bench_iters_requires_bench_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .arg("--bench-case-filter-iters")
        .arg("2")
        .output()
        .expect("run case filter bench without bench flag");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "case filter bench unexpectedly succeeded");
    assert!(
        stderr.contains("--bench-case-filter-iters requires --bench-case-filter"),
        "stderr: {stderr}"
    );
}

#[test]
fn frame_pacing_metrics_bench_cli_reports_metric_counts()
{
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .arg("--bench-frame-pacing-metrics")
      .arg("--bench-frame-pacing-iters")
      .arg("2")
      .output()
      .expect("run frame pacing metrics bench");
   let stdout = String::from_utf8_lossy(&output.stdout);
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(output.status.success(), "frame pacing metrics bench failed: {stderr}");
   assert!(stdout.contains("frame_pacing_metrics_bench"), "stdout: {stdout}");
   assert!(stdout.contains("iterations=2"), "stdout: {stdout}");
   assert!(stdout.contains("samples=1024"), "stdout: {stdout}");
   assert!(stdout.contains("metrics_per_iter=10"), "stdout: {stdout}");
   assert!(stdout.contains("checksum="), "stdout: {stdout}");
   assert!(!stdout.contains("suite="), "stdout: {stdout}");
}

#[test]
fn frame_pacing_metrics_bench_iters_requires_bench_flag()
{
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .arg("--bench-frame-pacing-iters")
      .arg("2")
      .output()
      .expect("run frame pacing metrics bench without bench flag");
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(!output.status.success(), "frame pacing metrics bench unexpectedly succeeded");
   assert!(
      stderr.contains("--bench-frame-pacing-iters requires --bench-frame-pacing-metrics"),
      "stderr: {stderr}"
   );
}

#[test]
fn distribution_metrics_bench_cli_reports_metric_counts()
{
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .arg("--bench-distribution-metrics")
      .arg("--bench-distribution-iters")
      .arg("2")
      .output()
      .expect("run distribution metrics bench");
   let stdout = String::from_utf8_lossy(&output.stdout);
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(output.status.success(), "distribution metrics bench failed: {stderr}");
   assert!(stdout.contains("distribution_metrics_bench"), "stdout: {stdout}");
   assert!(stdout.contains("iterations=2"), "stdout: {stdout}");
   assert!(stdout.contains("samples_per_distribution=24"), "stdout: {stdout}");
   assert!(stdout.contains("distributions_per_iter=3"), "stdout: {stdout}");
   assert!(stdout.contains("metrics_per_iter=12"), "stdout: {stdout}");
   assert!(stdout.contains("checksum="), "stdout: {stdout}");
   assert!(!stdout.contains("suite="), "stdout: {stdout}");
}

#[test]
fn distribution_metrics_bench_iters_requires_bench_flag()
{
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .arg("--bench-distribution-iters")
      .arg("2")
      .output()
      .expect("run distribution metrics bench without bench flag");
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(!output.status.success(), "distribution metrics bench unexpectedly succeeded");
   assert!(
      stderr.contains("--bench-distribution-iters requires --bench-distribution-metrics"),
      "stderr: {stderr}"
   );
}

#[test]
fn case_metric_contract_bench_cli_reports_counts()
{
   let mut report_out = std::env::temp_dir();
   report_out.push(format!("oxide-perf-runner-case-metric-contract-{}.json", std::process::id()));
   let report = sample_report(vec![sample_gpu_frame_case("gpu.scene.contract.frame")]);
   fs::write(&report_out, serde_json::to_vec(&report).expect("serialize case metric report"))
      .expect("write case metric report");

   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .arg("--bench-case-metric-contract")
      .arg(&report_out)
      .arg("--bench-case-metric-iters")
      .arg("2")
      .output()
      .expect("run case metric contract bench");
   let stdout = String::from_utf8_lossy(&output.stdout);
   let stderr = String::from_utf8_lossy(&output.stderr);

   let _ = fs::remove_file(&report_out);

   assert!(output.status.success(), "case metric contract bench failed: {stderr}");
   assert!(stdout.contains("case_metric_contract_bench"), "stdout: {stdout}");
   assert!(stdout.contains("iterations=2"), "stdout: {stdout}");
   assert!(stdout.contains("cases=1"), "stdout: {stdout}");
   assert!(stdout.contains("frame_required=1"), "stdout: {stdout}");
   assert!(stdout.contains("gpu_required=1"), "stdout: {stdout}");
   assert!(stdout.contains("checksum="), "stdout: {stdout}");
   assert!(!stdout.contains("suite="), "stdout: {stdout}");
}

#[test]
fn case_metric_contract_bench_iters_requires_bench_flag()
{
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .arg("--bench-case-metric-iters")
      .arg("2")
      .output()
      .expect("run case metric contract bench without bench flag");
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(!output.status.success(), "case metric contract bench unexpectedly succeeded");
   assert!(
      stderr.contains("--bench-case-metric-iters requires --bench-case-metric-contract"),
      "stderr: {stderr}"
   );
}

#[test]
fn contract_coverage_bench_cli_reports_counts()
{
   let mut report_out = std::env::temp_dir();
   report_out.push(format!("oxide-perf-runner-contract-coverage-{}.json", std::process::id()));
   let mut report = sample_report(vec![sample_case("cpu.component.button.encode", 10.0, 0.10, true)]);
   report.contract = ContractCoverageReport {
      layers: vec![ContractCoverageEntry {
         id: String::from("runtime"),
         label: String::from("Runtime"),
         status: String::from("implemented"),
         notes: vec![String::from("Runtime coverage is implemented.")],
      }],
      battery: vec![ContractCoverageEntry {
         id: String::from("battery"),
         label: String::from("Battery"),
         status: String::from("partial"),
         notes: vec![String::from("Diagnostic row is separate.")],
      }],
      notes: Vec::new(),
   };
   fs::write(&report_out, serde_json::to_vec(&report).expect("serialize contract coverage report"))
      .expect("write contract coverage report");

   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .arg("--bench-contract-coverage")
      .arg(&report_out)
      .arg("--bench-contract-iters")
      .arg("2")
      .output()
      .expect("run contract coverage bench");
   let stdout = String::from_utf8_lossy(&output.stdout);
   let stderr = String::from_utf8_lossy(&output.stderr);

   let _ = fs::remove_file(&report_out);

   assert!(output.status.success(), "contract coverage bench failed: {stderr}");
   assert!(stdout.contains("contract_coverage_bench"), "stdout: {stdout}");
   assert!(stdout.contains("iterations=2"), "stdout: {stdout}");
   assert!(stdout.contains("layers=1"), "stdout: {stdout}");
   assert!(stdout.contains("battery=1"), "stdout: {stdout}");
   assert!(stdout.contains("notes=2"), "stdout: {stdout}");
   assert!(stdout.contains("checksum="), "stdout: {stdout}");
   assert!(!stdout.contains("suite="), "stdout: {stdout}");
}

#[test]
fn contract_coverage_bench_iters_requires_bench_flag()
{
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .arg("--bench-contract-iters")
      .arg("2")
      .output()
      .expect("run contract coverage bench without bench flag");
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(!output.status.success(), "contract coverage bench unexpectedly succeeded");
   assert!(
      stderr.contains("--bench-contract-iters requires --bench-contract-coverage"),
      "stderr: {stderr}"
   );
}

#[test]
fn compare_reports_bench_cli_reports_counts()
{
   let mut current_out = std::env::temp_dir();
   current_out.push(format!("oxide-perf-runner-compare-current-{}.json", std::process::id()));
   let mut baseline_out = std::env::temp_dir();
   baseline_out.push(format!("oxide-perf-runner-compare-baseline-{}.json", std::process::id()));

   let current = sample_report(vec![
      sample_case("cpu.component.button.encode", 10.0, 0.10, true),
      sample_case("cpu.component.label.encode", 4.0, 0.10, true),
   ]);
   let baseline = sample_report(vec![sample_case("cpu.component.button.encode", 9.0, 0.10, true)]);
   std::fs::write(&current_out, serde_json::to_vec(&current).expect("serialize current"))
      .expect("write current report");
   std::fs::write(&baseline_out, serde_json::to_vec(&baseline).expect("serialize baseline"))
      .expect("write baseline report");

   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .arg("--bench-compare-reports")
      .arg(&current_out)
      .arg(&baseline_out)
      .arg("--bench-compare-iters")
      .arg("2")
      .output()
      .expect("run compare reports bench");
   let stdout = String::from_utf8_lossy(&output.stdout);
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(output.status.success(), "compare reports bench failed: {stderr}");
   assert!(stdout.contains("compare_reports_bench"), "stdout: {stdout}");
   assert!(stdout.contains("iterations=2"), "stdout: {stdout}");
   assert!(stdout.contains("matched=1"), "stdout: {stdout}");
   assert!(stdout.contains("regressions=1"), "stdout: {stdout}");
   assert!(stdout.contains("missing_baseline=1"), "stdout: {stdout}");
   assert!(stdout.contains("improvements=0"), "stdout: {stdout}");
   assert!(stdout.contains("checksum="), "stdout: {stdout}");
   assert!(!stdout.contains("suite="), "stdout: {stdout}");
   let _ = std::fs::remove_file(current_out);
   let _ = std::fs::remove_file(baseline_out);
}

#[test]
fn compare_reports_bench_iters_requires_bench_flag()
{
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .arg("--bench-compare-iters")
      .arg("2")
      .output()
      .expect("run compare reports bench without bench flag");
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(!output.status.success(), "compare reports bench unexpectedly succeeded");
   assert!(
      stderr.contains("--bench-compare-iters requires --bench-compare-reports"),
      "stderr: {stderr}"
   );
}

#[test]
fn child_run_suite_tests_keep_everyday_tiering()
{
   let source = include_str!("report_tests.rs");
   let run_suite_arg = concat!("--run", "-suite");
   let ignore_marker = "#[ignore = \"explicit touched-case perf contract\"]";
   let active_tests = [
      "fn filtered_run_suite_runs_only_the_touched_case()",
      "fn retired_exact_aliases_are_not_registered()",
   ];
   let mut total = 0usize;
   let mut ignored = 0usize;
   let mut active = 0usize;

   for test in source.split("#[test]").skip(1)
   {
      let launches = test.matches(run_suite_arg).count();
      if launches == 0
      {
         continue;
      }
      total += launches;
      if test.contains(ignore_marker)
      {
         ignored += launches;
      }
      else
      {
         active += launches;
         assert!(
            active_tests.iter().any(|name| test.contains(name)),
            "unexpected active child suite test"
         );
      }
   }

   assert_eq!(total, 56);
   assert_eq!(ignored, 54);
   assert_eq!(active, 2);
}

#[test]
fn canonical_smoke_suite_keeps_exact_inventory()
{
   let report = collect_suite_report(true).expect("collect smoke suite");
   let mut ids = report.cases.iter().map(|case| case.id.as_str()).collect::<Vec<_>>();
   ids.sort_unstable();

   assert_eq!(report.version, 1);
   assert_eq!(report.suite, "canonical-smoke");
   assert_eq!(ids.len(), 23);
   assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
   assert_eq!(case_id_digest(&ids), 0x025eafdefc5ce69b);
}

#[test]
fn baseline_write_rejects_smoke_sampling()
{
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .arg("--write-baseline")
      .arg("--smoke")
      .output()
      .expect("reject smoke baseline write");
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(!output.status.success(), "smoke baseline write unexpectedly succeeded");
   assert!(stderr.contains("--write-baseline cannot be combined with --smoke"));
}

#[test]
fn baseline_write_rejects_touched_filter()
{
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .env("OXIDE_PERF_RUNNER_FILTER", "cpu.system.prepare_draws.current")
      .arg("--write-baseline")
      .output()
      .expect("reject filtered baseline write");
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(!output.status.success(), "filtered baseline write unexpectedly succeeded");
   assert!(stderr.contains("--write-baseline cannot be combined with OXIDE_PERF_RUNNER_FILTER"));
}

#[test]
fn filtered_run_suite_runs_only_the_touched_case()
{
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .env("OXIDE_PERF_RUNNER_FILTER", "cpu.system.prepare_draws.current")
      .arg("--run-suite")
      .arg("--smoke")
      .output()
      .expect("run filtered smoke suite");
   let stdout = String::from_utf8_lossy(&output.stdout);
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(output.status.success(), "filtered suite failed: {stderr}");
   assert!(stdout.contains("suite=touched-smoke cases=1"), "stdout: {stdout}");
   assert!(stdout.contains("case=cpu.system.prepare_draws.current"), "stdout: {stdout}");
   assert!(!stdout.contains("case=cpu.system.prepare_draws.legacy"), "stdout: {stdout}");
   assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_text_prefix_width_map_case() {
    let mut json_out = std::env::temp_dir();
    json_out.push(format!("oxide-perf-runner-text-prefix-width-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "cpu.system.text_prefix_width_map")
        .arg("--run-suite")
        .arg("--smoke")
        .arg("--json-out")
        .arg(&json_out)
        .output()
        .expect("run filtered text prefix smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=1"), "stdout: {stdout}");
    assert!(stdout.contains("case=cpu.system.text_prefix_width_map"), "stdout: {stdout}");
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");

    let report = std::fs::read_to_string(&json_out).expect("read filtered text prefix report");
    let row = report_case_slice(&report, "cpu.system.text_prefix_width_map");
    assert!(report_f64(row, "text_bytes") > 0.0);
    assert_eq!(report_f64(row, "prefix_boundaries"), report_f64(row, "width_entries"));
    assert_eq!(report_f64(row, "shaped_runs"), 1.0);
    let _ = std::fs::remove_file(json_out);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_text_atlas_pressure_metrics() {
    let mut json_out = std::env::temp_dir();
    json_out.push(format!("oxide-perf-runner-text-atlas-pressure-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "cpu.system.text_atlas_pressure")
        .arg("--run-suite")
        .arg("--smoke")
        .arg("--json-out")
        .arg(&json_out)
        .output()
        .expect("run filtered text atlas pressure smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=1"), "stdout: {stdout}");
    assert!(stdout.contains("case=cpu.system.text_atlas_pressure"), "stdout: {stdout}");
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");

    let report = std::fs::read_to_string(&json_out).expect("read filtered text atlas report");
    let row = report_case_slice(&report, "cpu.system.text_atlas_pressure");
    assert!(report_f64(row, "atlas_shape_count") > 0.0);
    assert!(report_f64(row, "atlas_rendered_glyph_runs") > 0.0);
    assert!(report_f64(row, "atlas_evictions") > 0.0);
    assert_eq!(report_f64(row, "atlas_revision"), report_f64(row, "atlas_evictions"));
    assert!(report_f64(row, "atlas_dirty_rects") > 0.0);
    assert!(report_f64(row, "atlas_dirty_pixels") > 0.0);
    let _ = std::fs::remove_file(json_out);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_text_sdf_bake_metrics()
{
   let mut json_out = std::env::temp_dir();
   json_out.push(format!("oxide-perf-runner-text-sdf-bake-{}.json", std::process::id()));
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .env("OXIDE_PERF_RUNNER_FILTER", "cpu.system.text_sdf_bake")
      .arg("--run-suite")
      .arg("--smoke")
      .arg("--json-out")
      .arg(&json_out)
      .output()
      .expect("run filtered text SDF bake smoke suite");
   let stdout = String::from_utf8_lossy(&output.stdout);
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(output.status.success(), "filtered suite failed: {stderr}");
   assert!(stdout.contains("cases=1"), "stdout: {stdout}");
   assert!(stdout.contains("case=cpu.system.text_sdf_bake"), "stdout: {stdout}");
   assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");

   let report = std::fs::read_to_string(&json_out).expect("read filtered text SDF report");
   let row = report_case_slice(&report, "cpu.system.text_sdf_bake");
   let parsed: PerfReport = serde_json::from_str(&report).expect("parse filtered text SDF report");
   assert_eq!(workspace_case(&parsed, "cpu.system.text_sdf_bake").cache_state, "cold");
   assert_eq!(report_f64(row, "sdf_glyph_runs"), 2.0);
   assert!(report_f64(row, "sdf_vertices") > 0.0);
   assert!(report_f64(row, "sdf_indices") > 0.0);
   assert!(report_f64(row, "sdf_dirty_pixels") > 0.0);
   let _ = std::fs::remove_file(json_out);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_text_fallback_label_encode_case() {
    let mut json_out = std::env::temp_dir();
    json_out.push(format!("oxide-perf-runner-text-fallback-label-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "cpu.system.text_fallback_label_encode")
        .arg("--run-suite")
        .arg("--smoke")
        .arg("--json-out")
        .arg(&json_out)
        .output()
        .expect("run filtered text fallback label smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=1"), "stdout: {stdout}");
    assert!(stdout.contains("case=cpu.system.text_fallback_label_encode"), "stdout: {stdout}");
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");

    let report =
        std::fs::read_to_string(&json_out).expect("read filtered text fallback label report");
    let row = report_case_slice(&report, "cpu.system.text_fallback_label_encode");
    assert!(report_f64(row, "fallback_fonts") >= 1.0);
    assert!(report_f64(row, "fallback_label_glyph_runs") >= 1.0);
    assert!(report_f64(row, "fallback_label_vertices") > 0.0);
    let _ = std::fs::remove_file(json_out);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_text_atlas_dirty_rect_upload_case() {
    let mut json_out = std::env::temp_dir();
    json_out.push(format!("oxide-perf-runner-text-atlas-dirty-upload-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "cpu.system.text_atlas_dirty_rect_upload")
        .arg("--run-suite")
        .arg("--smoke")
        .arg("--json-out")
        .arg(&json_out)
        .output()
        .expect("run filtered text atlas dirty upload smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=1"), "stdout: {stdout}");
    assert!(stdout.contains("case=cpu.system.text_atlas_dirty_rect_upload"), "stdout: {stdout}");
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");

    let report =
        std::fs::read_to_string(&json_out).expect("read filtered text atlas dirty upload report");
    let row = report_case_slice(&report, "cpu.system.text_atlas_dirty_rect_upload");
    assert!(report_f64(row, "atlas_create_calls") >= 1.0);
    assert!(report_f64(row, "atlas_update_calls") >= 2.0);
    assert!(report_f64(row, "dirty_upload_pixels") > 0.0);
    assert!(report_f64(row, "dirty_to_full_upload_ratio") < 1.0);
    let _ = std::fs::remove_file(json_out);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_wrapped_label_cached_encode_case() {
    let mut json_out = std::env::temp_dir();
    json_out.push(format!("oxide-perf-runner-wrapped-label-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "cpu.system.wrapped_label_")
        .arg("--run-suite")
        .arg("--smoke")
        .arg("--json-out")
        .arg(&json_out)
        .output()
        .expect("run filtered wrapped label smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=1"), "stdout: {stdout}");
    assert!(stdout.contains("case=cpu.system.wrapped_label_cached_encode"), "stdout: {stdout}");
    assert!(!stdout.contains("case=cpu.system.wrapped_label_legacy_fit_shape"), "stdout: {stdout}");
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");

    let report = std::fs::read_to_string(&json_out).expect("read filtered wrapped label report");
    let row = report_case_slice(&report, "cpu.system.wrapped_label_cached_encode");
    assert_eq!(report_f64(row, "wrapped_label_variants"), 4096.0);
    assert_eq!(report_f64(row, "atlas_create_calls"), 1.0);
    assert_eq!(report_f64(row, "atlas_update_calls"), 0.0);
    assert!(report_f64(row, "wrapped_label_glyph_runs") > 1.0);
    assert!(report_f64(row, "wrapped_label_vertices") > 0.0);
    assert!(!report.contains("cpu.system.wrapped_label_legacy_fit_shape"));
    let _ = std::fs::remove_file(json_out);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_picker_text_cached_encode_case() {
    let mut json_out = std::env::temp_dir();
    json_out.push(format!("oxide-perf-runner-picker-text-cached-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "cpu.system.picker_text_")
        .arg("--run-suite")
        .arg("--smoke")
        .arg("--json-out")
        .arg(&json_out)
        .output()
        .expect("run filtered picker text cached smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=1"), "stdout: {stdout}");
    assert!(stdout.contains("case=cpu.system.picker_text_cached_encode"), "stdout: {stdout}");
    assert!(!stdout.contains("case=cpu.system.picker_text_legacy_shape_upload"), "stdout: {stdout}");
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");

    let report =
        std::fs::read_to_string(&json_out).expect("read filtered picker text cached report");
    let row = report_case_slice(&report, "cpu.system.picker_text_cached_encode");
    assert_eq!(report_f64(row, "atlas_create_calls"), 1.0);
    assert_eq!(report_f64(row, "atlas_update_calls"), 0.0);
    assert!(report_f64(row, "picker_glyph_runs") > 0.0);
    assert!(report_f64(row, "picker_vertices") > 0.0);
    assert!(report_f64(row, "dirty_to_full_upload_ratio") < 1.0);
    assert!(!report.contains("cpu.system.picker_text_legacy_shape_upload"));
    let _ = std::fs::remove_file(json_out);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_paged_text_atlas_locality_case() {
    let mut json_out = std::env::temp_dir();
    json_out.push(format!("oxide-perf-runner-paged-text-atlas-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env(
            "OXIDE_PERF_RUNNER_FILTER",
            "cpu.architecture.text.paged_atlas_locality.single_scale",
        )
        .arg("--run-suite")
        .arg("--smoke")
        .arg("--json-out")
        .arg(&json_out)
        .output()
        .expect("run filtered paged text atlas smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=1"), "stdout: {stdout}");
    let report = std::fs::read_to_string(&json_out).expect("read paged text atlas report");
    let row = report_case_slice(
        &report,
        "cpu.architecture.text.paged_atlas_locality.single_scale",
    );
    assert_eq!(report_f64(row, "atlas_pages"), 2.0);
    assert_eq!(report_f64(row, "atlas_evictions"), 1.0);
    assert_eq!(report_f64(row, "atlas_release_calls"), 1.0);
    assert_eq!(report_f64(row, "stable_unrelated_pages"), 1.0);
    assert_eq!(report_f64(row, "atlas_resident_bytes"), 1_152.0);
    assert!(report_f64(row, "atlas_fragmentation_bytes") > 0.0);
    let _ = std::fs::remove_file(json_out);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_bitmap_text_options_case() {
    let mut json_out = std::env::temp_dir();
    json_out.push(format!("oxide-perf-runner-bitmap-options-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "cpu.architecture.text.bitmap_options")
        .arg("--run-suite")
        .arg("--smoke")
        .arg("--json-out")
        .arg(&json_out)
        .output()
        .expect("run filtered bitmap options smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=1"), "stdout: {stdout}");
    assert!(stdout.contains("case=cpu.architecture.text.bitmap_options"), "stdout: {stdout}");
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");

    let report = std::fs::read_to_string(&json_out).expect("read bitmap options report");
    let row = report_case_slice(&report, "cpu.architecture.text.bitmap_options");
    assert_eq!(report_f64(row, "option_labels"), 4.0);
    assert_eq!(report_f64(row, "glyph_run_draws"), 4.0);
    assert_eq!(report_f64(row, "label_solid_draws"), 0.0);
    assert_eq!(report_f64(row, "non_label_solid_draws"), 2.0);
    assert_eq!(report_f64(row, "global_render_mutex_locks"), 0.0);
    assert_eq!(report_f64(row, "warm_atlas_upload_calls"), 0.0);
    assert_eq!(report_f64(row, "warm_atlas_upload_bytes"), 0.0);
    let _ = std::fs::remove_file(json_out);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_metal_paged_text_atlas_locality_case() {
    let mut json_out = std::env::temp_dir();
    json_out.push(format!("oxide-perf-runner-metal-paged-text-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "gpu.architecture.text.paged_atlas_locality")
        .arg("--run-suite")
        .arg("--smoke")
        .arg("--json-out")
        .arg(&json_out)
        .output()
        .expect("run filtered Metal paged text atlas smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=1"), "stdout: {stdout}");
    let report = std::fs::read_to_string(&json_out).expect("read Metal paged text report");
    let row = report_case_slice(&report, "gpu.architecture.text.paged_atlas_locality");
    assert_eq!(report_f64(row, "paged_atlas"), 1.0);
    assert_eq!(report_f64(row, "invalidated_chunks_avg"), 1.0);
    assert_eq!(report_f64(row, "prepared_cache_hits_avg"), 1.0);
    assert_eq!(report_f64(row, "chunks_prepared_avg"), 1.0);
    assert_eq!(report_f64(row, "draws_avg"), 2.0);
    assert_eq!(report_f64(row, "atlas_resident_bytes"), 8_192.0);
    assert_eq!(report_f64(row, "atlas_upload_bytes_avg"), 4_096.0);
    let _ = std::fs::remove_file(json_out);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_gpu_authoring_cases() {
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "gpu.authoring.scene3d.mixed_frame")
        .arg("--run-suite")
        .arg("--smoke")
        .output()
        .expect("run filtered gpu authoring smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=1"), "stdout: {stdout}");
    assert!(stdout.contains("case=gpu.authoring.scene3d.mixed_frame"), "stdout: {stdout}");
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_retained_snapshot_authoring_case() {
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "gpu.authoring.retained_snapshot.clean_mixed")
        .arg("--run-suite")
        .arg("--smoke")
        .output()
        .expect("run filtered retained-snapshot authoring smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=1"), "stdout: {stdout}");
    assert!(stdout.contains("case=gpu.authoring.retained_snapshot.clean_mixed"), "stdout: {stdout}");
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_gpu_animation_effects_case() {
    let mut json_out = std::env::temp_dir();
    json_out.push(format!("oxide-perf-runner-gpu-animation-effects-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "gpu.animation.effects.refresh_matrix")
        .arg("--run-suite")
        .arg("--smoke")
        .arg("--json-out")
        .arg(&json_out)
        .output()
        .expect("run filtered gpu animation-effects smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=1"), "stdout: {stdout}");
    assert!(stdout.contains("case=gpu.animation.effects.refresh_matrix"), "stdout: {stdout}");
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");

    let report =
        std::fs::read_to_string(&json_out).expect("read filtered gpu animation-effects report");
    assert!(report.contains("\"id\": \"gpu.animation.effects.refresh_matrix\""));
    assert!(report.contains("\"gpu_ms_p99\""));
    assert!(report.contains("\"missed_frame_ratio_120hz\""));
    assert!(report.contains("\"hitch_ratio_120hz\""));
    let _ = std::fs::remove_file(json_out);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_dirty_leaf_retained_authoring_case() {
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "cpu.authoring.surface_retained.dirty_leaf_encode")
        .arg("--run-suite")
        .arg("--smoke")
        .output()
        .expect("run filtered dirty-leaf retained authoring smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=1"), "stdout: {stdout}");
    assert!(
        stdout.contains("case=cpu.authoring.surface_retained.dirty_leaf_encode"),
        "stdout: {stdout}",
    );
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_retained_cache_policy_authoring_case()
{
   let mut json_out = std::env::temp_dir();
   json_out.push(format!("oxide-perf-runner-retained-cache-policy-{}.json", std::process::id()));
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .env("OXIDE_PERF_RUNNER_FILTER", "cpu.authoring.surface_retained.cache_policy")
      .arg("--run-suite")
      .arg("--smoke")
      .arg("--json-out")
      .arg(&json_out)
      .output()
      .expect("run filtered retained cache-policy authoring smoke suite");
   let stdout = String::from_utf8_lossy(&output.stdout);
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(output.status.success(), "filtered suite failed: {stderr}");
   assert!(stdout.contains("cases=1"), "stdout: {stdout}");
   let report = std::fs::read_to_string(&json_out).expect("read retained cache-policy report");
   let row = report_case_slice(&report, "cpu.authoring.surface_retained.cache_policy");
   assert_eq!(report_f64(row, "cpu_budget_bytes"), 1_048_576.0);
   assert_eq!(report_f64(row, "prepared_gpu_budget_bytes"), 2_097_152.0);
   assert_eq!(report_f64(row, "cache_complete"), 1.0);
   assert!(report_f64(row, "cache_hits") > 0.0);
   let _ = std::fs::remove_file(json_out);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_surface_router_retained_overlay_metrics() {
    let mut json_out = std::env::temp_dir();
    json_out.push(format!("oxide-perf-runner-surface-router-compose-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "cpu.authoring.surface_router.compose")
        .arg("--run-suite")
        .arg("--smoke")
        .arg("--json-out")
        .arg(&json_out)
        .output()
        .expect("run filtered surface router compose smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=1"), "stdout: {stdout}");
    assert!(stdout.contains("case=cpu.authoring.surface_router.compose"), "stdout: {stdout}");
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");

    let report = std::fs::read_to_string(&json_out).expect("read surface router report");
    let report: PerfReport = serde_json::from_str(&report).expect("parse surface router report");
    let case = report
        .cases
        .iter()
        .find(|case| case.id == "cpu.authoring.surface_router.compose")
        .expect("surface router case");
    assert!(case.metrics["router_current_reused_total"] > 0.0);
    assert!(case.metrics["router_overlay_reused_total"] > 0.0);
    assert!(case.metrics["router_popup_reused_total"] > 0.0);
    let _ = std::fs::remove_file(json_out);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_collection_key_reconcile_ab_cases() {
    let mut json_out = std::env::temp_dir();
    json_out
        .push(format!("oxide-perf-runner-collection-key-reconcile-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "cpu.authoring.collection_key_reconcile")
        .arg("--run-suite")
        .arg("--smoke")
        .arg("--json-out")
        .arg(&json_out)
        .output()
        .expect("run filtered collection key reconcile smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=2"), "stdout: {stdout}");
    assert!(
        stdout.contains("case=cpu.authoring.collection_key_reconcile.indexed"),
        "stdout: {stdout}",
    );
    assert!(
        stdout.contains("case=cpu.authoring.collection_key_reconcile.scan"),
        "stdout: {stdout}",
    );
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");

    let report = std::fs::read_to_string(&json_out).expect("read collection key reconcile report");
    let report: PerfReport = serde_json::from_str(&report).expect("parse collection report");
    let indexed = report
        .cases
        .iter()
        .find(|case| case.id == "cpu.authoring.collection_key_reconcile.indexed")
        .expect("indexed case");
    let scan = report
        .cases
        .iter()
        .find(|case| case.id == "cpu.authoring.collection_key_reconcile.scan")
        .expect("scan case");
    assert_eq!(indexed.metrics["collection_key_index_enabled"], 1.0);
    assert_eq!(scan.metrics["collection_key_index_enabled"], 0.0);
    assert!(
        scan.metrics["collection_item_key_queries_per_lookup"]
            > indexed.metrics["collection_item_key_queries_per_lookup"] * 10.0,
        "scan metrics {:?}; indexed metrics {:?}",
        scan.metrics,
        indexed.metrics,
    );
    let _ = std::fs::remove_file(json_out);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_collection_prefix_update_ab_cases() {
    let mut json_out = std::env::temp_dir();
    json_out
        .push(format!("oxide-perf-runner-collection-prefix-update-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "cpu.authoring.collection_prefix_update")
        .arg("--run-suite")
        .arg("--smoke")
        .arg("--json-out")
        .arg(&json_out)
        .output()
        .expect("run filtered collection prefix update smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=2"), "stdout: {stdout}");
    assert!(
        stdout.contains("case=cpu.authoring.collection_prefix_update.incremental"),
        "stdout: {stdout}",
    );
    assert!(
        stdout.contains("case=cpu.authoring.collection_prefix_update.full_scan"),
        "stdout: {stdout}",
    );
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");

    let report = std::fs::read_to_string(&json_out).expect("read collection prefix update report");
    let report: PerfReport = serde_json::from_str(&report).expect("parse collection report");
    let incremental = report
        .cases
        .iter()
        .find(|case| case.id == "cpu.authoring.collection_prefix_update.incremental")
        .expect("incremental case");
    let full_scan = report
        .cases
        .iter()
        .find(|case| case.id == "cpu.authoring.collection_prefix_update.full_scan")
        .expect("full-scan case");
    assert_eq!(incremental.metrics["collection_changed_range_enabled"], 1.0);
    assert_eq!(full_scan.metrics["collection_changed_range_enabled"], 0.0);
    assert!(
        full_scan.metrics["collection_item_revision_queries_per_op"]
            > incremental.metrics["collection_item_revision_queries_per_op"] * 50.0,
        "full-scan metrics {:?}; incremental metrics {:?}",
        full_scan.metrics,
        incremental.metrics,
    );
    let _ = std::fs::remove_file(json_out);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_collection_measure_cache_bounded_churn_case() {
    let mut json_out = std::env::temp_dir();
    json_out.push(format!("oxide-perf-runner-collection-cache-churn-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "cpu.authoring.collection_measure_cache.bounded_churn")
        .arg("--run-suite")
        .arg("--smoke")
        .arg("--json-out")
        .arg(&json_out)
        .output()
        .expect("run filtered collection measurement-cache churn smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=1"), "stdout: {stdout}");
    assert!(
        stdout.contains("case=cpu.authoring.collection_measure_cache.bounded_churn"),
        "stdout: {stdout}",
    );
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");

    let report = std::fs::read_to_string(&json_out).expect("read collection cache churn report");
    let report: PerfReport = serde_json::from_str(&report).expect("parse collection cache report");
    let row = report
        .cases
        .iter()
        .find(|case| case.id == "cpu.authoring.collection_measure_cache.bounded_churn")
        .expect("bounded churn case");
    assert!(
        row.metrics["collection_initial_measure_calls_per_op"] >= row.metrics["collection_count"]
    );
    assert!(row.metrics["collection_repair_measure_calls_per_op"] > 0.0);
    assert!(row.metrics["collection_repair_measure_calls_per_op"] < 32.0);
    assert!(row.metrics["collection_repair_to_initial_measure_ratio"] < 0.01);
    assert!(row.metrics["collection_repair_draw_items_per_op"] > 0.0);
    let _ = std::fs::remove_file(json_out);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_drawlist_text_replay_authoring_case() {
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "cpu.authoring.drawlist_text_replay.multi_atlas")
        .arg("--run-suite")
        .arg("--smoke")
        .output()
        .expect("run filtered drawlist text replay authoring smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=1"), "stdout: {stdout}");
    assert!(
        stdout.contains("case=cpu.authoring.drawlist_text_replay.multi_atlas"),
        "stdout: {stdout}",
    );
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_dirty_subtree_layout_case() {
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "cpu.layout.dirty_subtree.incremental_relayout")
        .arg("--run-suite")
        .arg("--smoke")
        .output()
        .expect("run filtered dirty-subtree layout smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=1"), "stdout: {stdout}");
    assert!(
        stdout.contains("case=cpu.layout.dirty_subtree.incremental_relayout"),
        "stdout: {stdout}",
    );
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_descendant_only_layout_case() {
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "cpu.layout.descendant_only.incremental_relayout")
        .arg("--run-suite")
        .arg("--smoke")
        .output()
        .expect("run filtered descendant-only layout smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=1"), "stdout: {stdout}");
    assert!(
        stdout.contains("case=cpu.layout.descendant_only.incremental_relayout"),
        "stdout: {stdout}",
    );
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_transform_only_layout_case() {
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "cpu.layout.transform_only.reposition")
        .arg("--run-suite")
        .arg("--smoke")
        .output()
        .expect("run filtered transform-only layout smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=1"), "stdout: {stdout}");
    assert!(stdout.contains("case=cpu.layout.transform_only.reposition"), "stdout: {stdout}",);
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_paint_only_opacity_clip_layout_case() {
    let mut json_out = std::env::temp_dir();
    json_out.push(format!("oxide-perf-runner-paint-only-layout-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "cpu.layout.paint_only.opacity_clip")
        .arg("--run-suite")
        .arg("--smoke")
        .arg("--json-out")
        .arg(&json_out)
        .output()
        .expect("run filtered paint-only layout smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=1"), "stdout: {stdout}");
    assert!(stdout.contains("case=cpu.layout.paint_only.opacity_clip"), "stdout: {stdout}",);
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");

    let report = std::fs::read_to_string(&json_out).expect("read filtered paint-only report");
    let row = report_case_slice(&report, "cpu.layout.paint_only.opacity_clip");
    assert_eq!(report_f64(row, "layout_visited_nodes_per_op"), 0.0);
    assert_eq!(report_f64(row, "layout_measured_children_per_op"), 0.0);
    assert!(report_f64(row, "retained_reused_nodes_per_op") > 0.0);
    assert!(report_f64(row, "retained_rebuilt_nodes_per_op") > 0.0);
    let _ = std::fs::remove_file(json_out);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_node_content_dirty_layout_case() {
    let mut json_out = std::env::temp_dir();
    json_out.push(format!("oxide-perf-runner-node-content-dirty-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "cpu.layout.node_content_dirty.retained_replay")
        .arg("--run-suite")
        .arg("--smoke")
        .arg("--json-out")
        .arg(&json_out)
        .output()
        .expect("run filtered node-content dirty layout smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=1"), "stdout: {stdout}");
    assert!(
        stdout.contains("case=cpu.layout.node_content_dirty.retained_replay"),
        "stdout: {stdout}",
    );
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");

    let report = std::fs::read_to_string(&json_out).expect("read filtered node-content report");
    let row = report_case_slice(&report, "cpu.layout.node_content_dirty.retained_replay");
    assert_eq!(report_f64(row, "layout_visited_nodes_per_op"), 0.0);
    assert_eq!(report_f64(row, "layout_measured_children_per_op"), 0.0);
    assert!(report_f64(row, "text_dirty_ops") > 0.0);
    assert!(report_f64(row, "image_dirty_ops") > 0.0);
    assert!(report_f64(row, "camera_dirty_ops") > 0.0);
    assert!(report_f64(row, "retained_reused_nodes_per_op") > 0.0);
    assert!(report_f64(row, "retained_rebuilt_nodes_per_op") > 0.0);
    let _ = std::fs::remove_file(json_out);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_hit_test_dirty_layout_case() {
    let mut json_out = std::env::temp_dir();
    json_out.push(format!("oxide-perf-runner-hit-test-dirty-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "cpu.layout.hit_test_dirty.retained_reuse")
        .arg("--run-suite")
        .arg("--smoke")
        .arg("--json-out")
        .arg(&json_out)
        .output()
        .expect("run filtered hit-test dirty layout smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=1"), "stdout: {stdout}");
    assert!(stdout.contains("case=cpu.layout.hit_test_dirty.retained_reuse"), "stdout: {stdout}",);
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");

    let report = std::fs::read_to_string(&json_out).expect("read filtered hit-test report");
    let row = report_case_slice(&report, "cpu.layout.hit_test_dirty.retained_reuse");
    assert_eq!(report_f64(row, "layout_visited_nodes_per_op"), 0.0);
    assert_eq!(report_f64(row, "layout_measured_children_per_op"), 0.0);
    assert_eq!(report_f64(row, "retained_rebuilt_nodes_per_op"), 0.0);
    assert_eq!(report_f64(row, "retained_rebuilt_ops"), 0.0);
    assert!(report_f64(row, "hit_test_dirty_ops") > 0.0);
    assert!(report_f64(row, "retained_reused_nodes_per_op") > 0.0);
    assert!(report_f64(row, "retained_reused_ops") > 0.0);
    let _ = std::fs::remove_file(json_out);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_scoped_tree_mutation_layout_case() {
    let mut json_out = std::env::temp_dir();
    json_out.push(format!("oxide-perf-runner-scoped-tree-mutation-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "cpu.layout.scoped_tree_mutation.add_remove")
        .arg("--run-suite")
        .arg("--smoke")
        .arg("--json-out")
        .arg(&json_out)
        .output()
        .expect("run filtered scoped tree mutation layout smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=1"), "stdout: {stdout}");
    assert!(stdout.contains("case=cpu.layout.scoped_tree_mutation.add_remove"), "stdout: {stdout}",);
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");

    let report = std::fs::read_to_string(&json_out).expect("read scoped tree mutation report");
    let row = report_case_slice(&report, "cpu.layout.scoped_tree_mutation.add_remove");
    assert!(report_f64(row, "scoped_add_ops") > 0.0);
    assert!(report_f64(row, "scoped_remove_ops") > 0.0);
    assert!(report_f64(row, "layout_skipped_subtrees_per_op") > 0.0);
    assert!(report_f64(row, "retained_reused_nodes_per_op") > 0.0);
    assert!(report_f64(row, "retained_rebuilt_nodes_per_op") > 0.0);
    let _ = std::fs::remove_file(json_out);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_state_reconcile_battery() {
    let mut json_out = std::env::temp_dir();
    json_out.push(format!("oxide-perf-runner-state-reconcile-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "cpu.reconcile.")
        .arg("--run-suite")
        .arg("--smoke")
        .arg("--json-out")
        .arg(&json_out)
        .output()
        .expect("run filtered state reconcile smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=4"), "stdout: {stdout}");
    for id in [
        "cpu.reconcile.single_node_mutation",
        "cpu.reconcile.tree_mutation_1pct",
        "cpu.reconcile.tree_mutation_10pct",
        "cpu.reconcile.theme_swap_full",
    ] {
        assert!(stdout.contains(&format!("case={id}")), "stdout: {stdout}");
    }
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");

    let report = std::fs::read_to_string(&json_out).expect("read filtered reconcile report");
    let single = report_case_slice(&report, "cpu.reconcile.single_node_mutation");
    let one_pct = report_case_slice(&report, "cpu.reconcile.tree_mutation_1pct");
    let ten_pct = report_case_slice(&report, "cpu.reconcile.tree_mutation_10pct");
    let full = report_case_slice(&report, "cpu.reconcile.theme_swap_full");
    assert_eq!(report_f64(single, "dirty_nodes"), 1.0);
    assert_eq!(report_f64(one_pct, "dirty_nodes"), 10.0);
    assert_eq!(report_f64(ten_pct, "dirty_nodes"), 100.0);
    assert_eq!(report_f64(full, "dirty_nodes"), 1000.0);
    assert_eq!(report_f64(full, "layout_passes"), 2.0);
    let _ = std::fs::remove_file(json_out);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_text_ime_journey_and_state_cases() {
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env(
            "OXIDE_PERF_RUNNER_FILTER",
            "cpu.journey.text_ime_composition_cycle,cpu.text_input.ime.composition_commit_cycle",
        )
        .arg("--run-suite")
        .arg("--smoke")
        .output()
        .expect("run filtered text ime smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=2"), "stdout: {stdout}");
    assert!(stdout.contains("case=cpu.journey.text_ime_composition_cycle"), "stdout: {stdout}");
    assert!(
        stdout.contains("case=cpu.text_input.ime.composition_commit_cycle"),
        "stdout: {stdout}",
    );
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_text_cursor_pick_cluster_map_case() {
    let mut json_out = std::env::temp_dir();
    json_out.push(format!("oxide-perf-runner-text-cursor-map-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env(
            "OXIDE_PERF_RUNNER_FILTER",
            "cpu.text_input.cursor_pick.cluster_map,cpu.text_input.cursor_pick.rtl_cluster_map,cpu.text_input.cursor_pick.fallback_cluster_map,cpu.text_input.cursor_pick.mixed_bidi_affinity",
        )
        .arg("--run-suite")
        .arg("--smoke")
        .arg("--json-out")
        .arg(&json_out)
        .output()
        .expect("run filtered text cursor-pick smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=4"), "stdout: {stdout}");
    assert!(stdout.contains("case=cpu.text_input.cursor_pick.cluster_map"), "stdout: {stdout}",);
    assert!(stdout.contains("case=cpu.text_input.cursor_pick.rtl_cluster_map"), "stdout: {stdout}",);
    assert!(
        stdout.contains("case=cpu.text_input.cursor_pick.fallback_cluster_map"),
        "stdout: {stdout}",
    );
    assert!(
        stdout.contains("case=cpu.text_input.cursor_pick.mixed_bidi_affinity"),
        "stdout: {stdout}",
    );
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");

    let report = std::fs::read_to_string(&json_out).expect("read filtered text cursor report");
    let cluster = report_case_slice(&report, "cpu.text_input.cursor_pick.cluster_map");
    let rtl = report_case_slice(&report, "cpu.text_input.cursor_pick.rtl_cluster_map");
    let fallback = report_case_slice(&report, "cpu.text_input.cursor_pick.fallback_cluster_map");
    let mixed = report_case_slice(&report, "cpu.text_input.cursor_pick.mixed_bidi_affinity");
    assert_text_cursor_map_report_metrics(cluster, "cursor_map");
    assert_text_cursor_map_report_metrics(rtl, "rtl_cursor_map");
    assert_text_cursor_map_report_metrics(fallback, "fallback_cursor_map");
    assert_text_cursor_map_report_metrics(mixed, "mixed_bidi_cursor_map");
    assert!(report_f64(fallback, "fallback_shape_runs") >= 3.0);
    assert!(report_f64(mixed, "mixed_bidi_cursor_map_affinity_splits") >= 2.0);
    let _ = std::fs::remove_file(json_out);
}

fn assert_text_cursor_map_report_metrics(row: &str, prefix: &str) {
    let cursor_count = report_f64(row, &format!("{prefix}_cursor_count"));
    let byte_boundaries = report_f64(row, &format!("{prefix}_byte_boundaries"));
    assert!(cursor_count > 0.0);
    assert_eq!(byte_boundaries, cursor_count + 1.0);
    assert!(report_f64(row, &format!("{prefix}_boundary_checksum")) > 0.0);
    assert!(report_f64(row, &format!("{prefix}_width_span")) > 0.0);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_metal_id_mask_current_case() {
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "gpu.system.id_mask_compositor")
        .arg("--run-suite")
        .arg("--smoke")
        .output()
        .expect("run filtered gpu system smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=1"), "stdout: {stdout}");
    assert!(stdout.contains("case=gpu.system.id_mask_compositor.current"), "stdout: {stdout}");
    assert!(
        !stdout.contains("case=gpu.system.id_mask_compositor.legacy_upload"),
        "stdout: {stdout}"
    );
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_metal_neon_marker_ring_cases()
{
    let mut json_out = std::env::temp_dir();
    json_out.push(format!("oxide-perf-runner-neon-marker-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env(
            "OXIDE_PERF_RUNNER_FILTER",
            "gpu.architecture.neon_markers.count_128,gpu.architecture.neon_markers.count_1024",
        )
        .arg("--run-suite")
        .arg("--smoke")
        .arg("--json-out")
        .arg(&json_out)
        .output()
        .expect("run filtered Metal neon-marker smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=2"), "stdout: {stdout}");
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");

    let report = std::fs::read_to_string(&json_out).expect("read Metal neon-marker report");
    let dense = report_case_slice(&report, "gpu.architecture.neon_markers.count_128");
    let multi = report_case_slice(&report, "gpu.architecture.neon_markers.count_1024");
    assert_eq!(report_f64(dense, "marker_count"), 128.0);
    assert_eq!(report_f64(dense, "marker_batches"), 1.0);
    assert_eq!(report_f64(dense, "draws_avg"), 1.0);
    assert_eq!(report_f64(dense, "instances_avg"), 128.0);
    assert_eq!(report_f64(dense, "uniform_upload_bytes_avg"), 9_216.0);
    assert_eq!(report_f64(multi, "marker_count"), 1_024.0);
    assert_eq!(report_f64(multi, "marker_batches"), 8.0);
    assert_eq!(report_f64(multi, "draws_avg"), 8.0);
    assert_eq!(report_f64(multi, "instances_avg"), 1_024.0);
    assert_eq!(report_f64(multi, "uniform_upload_bytes_avg"), 73_728.0);
    assert_eq!(report_f64(multi, "resource_grows_avg"), 0.0);
    let _ = std::fs::remove_file(json_out);
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_central_noop_rejection_cases()
{
    let mut json_out = std::env::temp_dir();
    json_out.push(format!("oxide-perf-runner-noop-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env(
            "OXIDE_PERF_RUNNER_FILTER",
            "cpu.architecture.noop.,gpu.architecture.noop.",
        )
        .arg("--run-suite")
        .arg("--smoke")
        .arg("--json-out")
        .arg(&json_out)
        .output()
        .expect("run filtered no-op rejection smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=4"), "stdout: {stdout}");

    let report = std::fs::read_to_string(&json_out).expect("read no-op rejection report");
    for id in [
        "cpu.architecture.noop.transparent_containers",
        "cpu.architecture.noop.zero_area",
        "gpu.architecture.noop.transparent_containers",
        "gpu.architecture.noop.zero_area",
    ] {
        let row = report_case_slice(&report, id);
        assert_eq!(report_f64(row, "input_noop_commands"), 4_096.0);
        assert_eq!(report_f64(row, "visible_commands"), 64.0);
        assert_eq!(report_f64(row, "emitted_commands"), 64.0);
        if id.starts_with("gpu.") {
            assert_eq!(report_f64(row, "commands_traversed_avg"), 64.0);
            assert_eq!(report_f64(row, "instances_avg"), 64.0);
            assert_eq!(report_f64(row, "instanced_draw_calls_avg"), 1.0);
            assert_eq!(report_f64(row, "parameter_upload_bytes_avg"), 4_104.0);
        }
    }
    let _ = std::fs::remove_file(json_out);
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_classifies_first_visible_images_as_cold()
{
   let mut json_out = std::env::temp_dir();
   json_out.push(format!("oxide-perf-runner-image-first-visible-{}.json", std::process::id()));
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .env(
         "OXIDE_PERF_RUNNER_FILTER",
         "gpu.image_pipeline.png.first_visible,gpu.image_pipeline.rgba.nearest_first_visible",
      )
      .arg("--run-suite")
      .arg("--smoke")
      .arg("--json-out")
      .arg(&json_out)
      .output()
      .expect("run filtered first-visible image smoke suite");
   let stdout = String::from_utf8_lossy(&output.stdout);
   let stderr = String::from_utf8_lossy(&output.stderr);
   assert!(output.status.success(), "filtered suite failed: {stderr}");
   assert!(stdout.contains("cases=2"), "stdout: {stdout}");

   let report: PerfReport = serde_json::from_slice(
      &std::fs::read(&json_out).expect("read first-visible image report"),
   ).expect("parse first-visible image report");
   for (id, sampling) in [
      ("gpu.image_pipeline.png.first_visible", "linear-sampled"),
      ("gpu.image_pipeline.rgba.nearest_first_visible", "nearest-sampled"),
   ]
   {
      let case = report.cases.iter().find(|case| case.id == id).expect("first-visible case");
      assert_eq!(case.cache_state, "cold");
      assert!(case.notes.iter().any(|note| {
         note.contains(sampling) && note.contains("prebuilt ImageView draw list")
      }));
   }
   let _ = std::fs::remove_file(json_out);
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_image_view_crop_authoring_cases()
{
    let mut json_out = std::env::temp_dir();
    json_out.push(format!("oxide-perf-runner-image-view-crop-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env(
            "OXIDE_PERF_RUNNER_FILTER",
            "cpu.authoring.image_view_grid.cover_,gpu.authoring.image_view_grid.cover_",
        )
        .arg("--run-suite")
        .arg("--smoke")
        .arg("--json-out")
        .arg(&json_out)
        .output()
        .expect("run filtered image-view crop smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=4"), "stdout: {stdout}");

    let report = std::fs::read_to_string(&json_out).expect("read image-view crop report");
    for count in [100_usize, 1_000]
    {
        for prefix in ["cpu", "gpu"]
        {
            let id = format!("{prefix}.authoring.image_view_grid.cover_{count}");
            let row = report_case_slice(&report, &id);
            assert_eq!(report_f64(row, "image_draws"), count as f64);
            assert_eq!(report_f64(row, "nine_slice_draws"), 0.0);
            assert_eq!(report_f64(row, "source_crop_commands"), count as f64);
            assert_eq!(report_f64(row, "quads"), count as f64);
            assert_eq!(report_f64(row, "logical_shaded_pixels"), (count * 288) as f64);
            if prefix == "gpu"
            {
                assert_eq!(report_f64(row, "instanced_draw_calls_avg"), if count == 100 { 2.0 } else { 16.0 });
                assert_eq!(report_f64(row, "total_parameter_bytes_avg"), if count == 100 { 7_432.0 } else { 72_256.0 });
            }
        }
    }
    let _ = std::fs::remove_file(json_out);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_rendering_architecture_contract() {
    let mut json_out = std::env::temp_dir();
    json_out.push(format!("oxide-perf-runner-architecture-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env(
            "OXIDE_PERF_RUNNER_FILTER",
            "cpu.architecture.retained.depth_16.clean,cpu.architecture.retained.cache_pressure,cpu.architecture.idle.static_foreground",
        )
        .arg("--run-suite")
        .arg("--smoke")
        .arg("--json-out")
        .arg(&json_out)
        .output()
        .expect("run filtered rendering architecture smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=4"), "stdout: {stdout}");
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");
    let report = std::fs::read_to_string(&json_out).expect("read rendering architecture report");
    let retained = report_case_slice(&report, "cpu.architecture.retained.depth_16.clean");
    let hot = report_case_slice(&report, "cpu.architecture.retained.cache_pressure.hot_reuse");
    let churn = report_case_slice(&report, "cpu.architecture.retained.cache_pressure.one_use_churn");
    let idle = report_case_slice(&report, "cpu.architecture.idle.static_foreground");
    for row in [retained, hot, churn, idle] {
        assert!(row.contains("\"family\": \"architecture\""));
        assert!(row.contains("\"scenario\": \"rendering-architecture\""));
    }
    assert_eq!(report_f64(retained, "tree_depth"), 16.0);
    assert_eq!(report_f64(retained, "label_nodes"), 1_000.0);
    assert_eq!(report_f64(retained, "image_nodes"), 500.0);
    assert_eq!(report_f64(hot, "cache_hit_rate"), 1.0);
    assert_eq!(report_f64(hot, "cache_complete"), 1.0);
    assert!(
        report_f64(hot, "retained_chunk_bytes")
            + report_f64(hot, "retained_sequence_bytes")
            <= report_f64(hot, "hard_budget_bytes"),
    );
    assert_eq!(report_f64(churn, "cache_hit_rate"), 0.0);
    assert_eq!(report_f64(churn, "retained_chunk_bytes"), 0.0);
    assert_eq!(report_f64(churn, "retained_sequence_bytes"), 0.0);
    assert_eq!(report_f64(churn, "flat_fallback_uses"), 1.0);
    assert_eq!(report_f64(idle, "submissions"), 0.0);
    assert_eq!(report_f64(idle, "wakeups"), 0.0);
    let _ = std::fs::remove_file(json_out);
}

#[test]
fn retired_exact_aliases_are_not_registered()
{
   let aliases = [
      "cpu.architecture.animation.surface_hit_test_300",
      "cpu.architecture.damage.retained_surface_dirty_leaf_10000",
      "cpu.architecture.spatial_metadata.glyph_mesh_10000",
      "gpu.architecture.images.immutable_minified_auto",
      "gpu.architecture.prepared_chunks.clean_mixed",
      "gpu.architecture.prepared_layers.clean_100x100",
      "gpu.architecture.spatial_metadata.small_damage_glyph_mesh_10000",
      "gpu.scene.anim_timeline.frame",
   ];
   let mut json_out = std::env::temp_dir();
   json_out.push(format!("oxide-perf-runner-retired-aliases-{}.json", std::process::id()));
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .env("OXIDE_PERF_RUNNER_FILTER", aliases.join(","))
      .arg("--run-suite")
      .arg("--smoke")
      .arg("--json-out")
      .arg(&json_out)
      .output()
      .expect("run retired exact-alias inventory");
   let stdout = String::from_utf8_lossy(&output.stdout);
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(output.status.success(), "retired exact-alias inventory failed: {stderr}");
   assert!(stdout.contains("cases=0"), "stdout: {stdout}");
   let report = std::fs::read_to_string(&json_out).expect("read retired exact-alias inventory");
   for alias in aliases
   {
      assert!(!report.contains(alias), "retired exact alias remains registered: {alias}");
   }
   let _ = std::fs::remove_file(json_out);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn gpu_scene_inventory_defers_timeline_work_to_animation_battery()
{
   let mut json_out = std::env::temp_dir();
   json_out.push(format!("oxide-perf-runner-gpu-scene-inventory-{}.json", std::process::id()));
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .env("OXIDE_PERF_RUNNER_FILTER", "gpu.scene.")
      .arg("--run-suite")
      .arg("--smoke")
      .arg("--json-out")
      .arg(&json_out)
      .output()
      .expect("run GPU scene inventory");
   let stdout = String::from_utf8_lossy(&output.stdout);
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(output.status.success(), "GPU scene inventory failed: {stderr}");
   assert!(stdout.contains("suite=touched-smoke cases=16"), "stdout: {stdout}");
   let report: PerfReport = serde_json::from_slice(
      &std::fs::read(&json_out).expect("read GPU scene inventory"),
   ).expect("parse GPU scene inventory");
   assert_eq!(report.coverage.scenes_gpu_total, 16);
   assert_eq!(report.coverage.scenes_gpu_covered.len(), 16);
   assert!(!report.cases.iter().any(|case| case.id == "gpu.scene.anim_timeline.frame"));
   let _ = std::fs::remove_file(json_out);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn webgpu_pipeline_profiles_have_a_public_authoring_contract()
{
   let mut json_out = std::env::temp_dir();
   json_out.push(format!("oxide-perf-runner-webgpu-profile-{}.json", std::process::id()));
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .env("OXIDE_PERF_RUNNER_FILTER", "cpu.authoring.webgpu_pipeline_profile.compose")
      .arg("--run-suite")
      .arg("--smoke")
      .arg("--json-out")
      .arg(&json_out)
      .output()
      .expect("run WebGPU pipeline-profile authoring row");
   let stderr = String::from_utf8_lossy(&output.stderr);
   assert!(output.status.success(), "WebGPU pipeline-profile authoring row failed: {stderr}");
   let report = std::fs::read_to_string(&json_out)
      .expect("read WebGPU pipeline-profile authoring report");
   let row = report_case_slice(&report, "cpu.authoring.webgpu_pipeline_profile.compose");
   assert!(row.contains("\"family\": \"authoring\""));
   assert!(row.contains("\"scenario\": \"authoring\""));
   assert_eq!(report_f64(row, "full_declared_pipelines"), 43.0);
   assert_eq!(report_f64(row, "minimal_declared_pipelines"), 2.0);
   assert_eq!(report_f64(row, "mixed_declared_pipelines"), 9.0);
   assert_eq!(report_f64(row, "minimal_pipelines_avoided"), 41.0);
   assert_eq!(report_f64(row, "mixed_pipelines_avoided"), 34.0);
   let _ = std::fs::remove_file(json_out);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn dynamic_property_animation_has_a_public_authoring_contract()
{
   let mut json_out = std::env::temp_dir();
   json_out.push(format!("oxide-perf-runner-dynamic-authoring-{}.json", std::process::id()));
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .env("OXIDE_PERF_RUNNER_FILTER", "cpu.authoring.animation.dynamic_properties_hit_test_300")
      .arg("--run-suite")
      .arg("--smoke")
      .arg("--json-out")
      .arg(&json_out)
      .output()
      .expect("run dynamic property authoring row");
   let stderr = String::from_utf8_lossy(&output.stderr);
   assert!(output.status.success(), "dynamic property authoring row failed: {stderr}");
   let report = std::fs::read_to_string(&json_out).expect("read dynamic property authoring report");
   let row = report_case_slice(&report, "cpu.authoring.animation.dynamic_properties_hit_test_300");
   assert!(row.contains("\"family\": \"authoring\""));
   assert!(row.contains("\"scenario\": \"authoring\""));
   assert_eq!(report_f64(row, "animated_nodes"), 300.0);
   assert_eq!(report_f64(row, "hit_test_geometry_nodes"), 300.0);
   assert_eq!(report_f64(row, "chunks_rebuilt_avg"), 0.0);
   assert_eq!(report_f64(row, "sequences_rebuilt_avg"), 0.0);
   assert_eq!(report_f64(row, "command_bytes_copied_avg"), 0.0);
   assert!(report_f64(row, "property_records_avg") >= 600.0);
   let _ = std::fs::remove_file(json_out);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn retained_spatial_query_has_a_public_authoring_contract()
{
   let mut json_out = std::env::temp_dir();
   json_out.push(format!("oxide-perf-runner-spatial-query-{}.json", std::process::id()));
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .env("OXIDE_PERF_RUNNER_FILTER", "cpu.authoring.retained_snapshot.spatial_query_10000")
      .arg("--run-suite")
      .arg("--smoke")
      .arg("--json-out")
      .arg(&json_out)
      .output()
      .expect("run retained spatial-query authoring row");
   let stderr = String::from_utf8_lossy(&output.stderr);
   assert!(output.status.success(), "retained spatial-query authoring row failed: {stderr}");
   let report = std::fs::read_to_string(&json_out).expect("read retained spatial-query report");
   let authoring = report_case_slice(
      &report,
      "cpu.authoring.retained_snapshot.spatial_query_10000",
   );
   assert!(authoring.contains("\"family\": \"authoring\""));
   assert_eq!(report_f64(authoring, "instance_count"), 512.0);
   assert_eq!(report_f64(authoring, "damage_instances_visited"), 1.0);
   assert_eq!(report_f64(authoring, "damage_instances_matched"), 1.0);
   assert_eq!(report_f64(authoring, "damage_vertices_visited"), 0.0);
   assert!(report_f64(authoring, "snapshot_metadata_bytes") > 0.0);
   let _ = std::fs::remove_file(json_out);
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "explicit touched-case perf contract"]
fn metal_architecture_reports_reconciled_renderer_resource_families()
{
   let mut json_out = std::env::temp_dir();
   json_out.push(format!("oxide-perf-runner-accounting-{}.json", std::process::id()));
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .env(
         "OXIDE_PERF_RUNNER_FILTER",
         "gpu.architecture.layers.clean_100x100,gpu.architecture.id_mask.static.size_512.chunks_1,gpu.architecture.scene3d.instances_96.compatible,gpu.architecture.scene3d.instances_96.bloom_1,gpu.architecture.scene3d.instances_96.bloom_3,gpu.architecture.scene3d.instances_96.bloom_viewport_25pct,gpu.architecture.scene3d.instances_96.bloom_overlay",
      )
      .env("OXIDE_ARCHITECTURE_METAL_FRAMES", "4")
      .env("OXIDE_ARCHITECTURE_METAL_WARMUPS", "2")
      .env("OXIDE_ARCHITECTURE_METAL_RAW_SAMPLES", "1")
      .env("OXIDE_C58_METAL_FRAMES", "4")
      .env("OXIDE_C58_METAL_WARMUPS", "2")
      .env("OXIDE_C58_RAW_SAMPLES", "1")
      .arg("--run-suite")
      .arg("--smoke")
      .arg("--json-out")
      .arg(&json_out)
      .output()
      .expect("run Metal renderer accounting smoke suite");
   let stdout = String::from_utf8_lossy(&output.stdout);
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(output.status.success(), "Metal accounting suite failed: {stderr}");
   assert!(stdout.contains("cases=8"), "stdout: {stdout}");
   let report = std::fs::read_to_string(&json_out).expect("read Metal accounting report");
   let layer = report_case_slice(&report, "gpu.architecture.layers.clean_100x100");
   let id_mask =
      report_case_slice(&report, "gpu.architecture.id_mask.static.size_512.chunks_1");
   let scene3d =
      report_case_slice(&report, "gpu.architecture.scene3d.instances_96.bloom_1");
   let scene3d_three =
      report_case_slice(&report, "gpu.architecture.scene3d.instances_96.bloom_3");
   let scene3d_viewport = report_case_slice(
      &report,
      "gpu.architecture.scene3d.instances_96.bloom_viewport_25pct",
   );
   let scene3d_overlay =
      report_case_slice(&report, "gpu.architecture.scene3d.instances_96.bloom_overlay");
   let scene3d_guard =
      report_case_slice(&report, "gpu.architecture.scene3d.instances_96.compatible");
   assert!(report_f64(layer, "layer_cache_bytes_peak") > 0.0);
   assert!(report_f64(layer, "layer_body_commands_scanned_avg") > 0.0);
   assert_eq!(report_f64(layer, "layer_body_commands_copied_avg"), 0.0);
   assert_eq!(report_f64(layer, "layer_texture_creates_avg"), 0.0);
   assert_eq!(report_f64(layer, "layer_cache_hits_avg"), 100.0);
   assert_eq!(report_f64(layer, "layer_cache_misses_avg"), 0.0);
   assert_eq!(report_f64(layer, "layer_offscreen_draws_avg"), 0.0);
   assert_eq!(report_f64(layer, "layer_inline_draws_avg"), 0.0);
   assert_eq!(report_f64(layer, "layer_double_render_prevented_avg"), 0.0);
   assert!(report_f64(layer, "raw_frame_ms_0000") > 0.0);
   assert!(report_f64(layer, "raw_frame_ms_0003") > 0.0);
   assert!(report_f64(layer, "raw_encode_ms_0003") > 0.0);
   assert!(report_f64(layer, "raw_gpu_ms_0003") > 0.0);
   assert!(report_f64(layer, "warmup_frame_ms_0000") > 0.0);
   assert!(report_f64(layer, "warmup_encode_ms_0000") > 0.0);
   assert!(report_f64(layer, "warmup_gpu_ms_0000") > 0.0);
   assert!(report_f64(layer, "warmup_gpu_ms_0001") > 0.0);
   assert!(report_f64(id_mask, "id_mask_target_bytes_peak") > 0.0);
   assert!(report_f64(id_mask, "id_mask_vertex_bytes_peak") > 0.0);
   assert_eq!(report_f64(id_mask, "chunks_prepared_avg"), 0.0);
   assert_eq!(report_f64(id_mask, "id_mask_cache_hits_avg"), 1.0);
   assert_eq!(report_f64(id_mask, "id_mask_cache_misses_avg"), 0.0);
   assert_eq!(report_f64(id_mask, "id_mask_raster_passes_avg"), 0.0);
   assert_eq!(report_f64(id_mask, "id_mask_field_seed_passes_avg"), 0.0);
   assert_eq!(report_f64(id_mask, "id_mask_field_jump_passes_avg"), 0.0);
   assert_eq!(report_f64(id_mask, "id_mask_compositor_passes_avg"), 1.0);
   assert_eq!(report_f64(id_mask, "render_passes_avg"), 1.0);
   assert_eq!(report_f64(id_mask, "id_mask_cache_entries_peak"), 1.0);
   assert_eq!(report_f64(id_mask, "id_mask_target_creates_avg"), 0.0);
   assert_eq!(report_f64(id_mask, "id_mask_in_flight_generations_peak"), 1.0);
   assert!(report_f64(id_mask, "id_mask_in_flight_target_bytes_peak") > 0.0);
   assert!(report_f64(id_mask, "id_mask_target_storage_bytes_peak") > 0.0);
   assert!(report_f64(id_mask, "id_mask_generation_peak_bytes") > 0.0);
   assert_eq!(report_f64(id_mask, "id_mask_target_reuse_blocked"), 0.0);
   assert!(report_f64(id_mask, "id_mask_cache_resident_bytes_peak") > 0.0);
   assert!(report_f64(id_mask, "id_mask_cache_resident_bytes_peak")
      <= report_f64(id_mask, "id_mask_cache_budget_bytes"));
   assert!(report_f64(scene3d, "depth_target_bytes_peak") > 0.0);
   assert!(report_f64(scene3d, "bloom_target_bytes_peak") > 0.0);
   assert!(report_f64(scene3d, "mesh_buffer_bytes_peak") > 0.0);
   assert_eq!(report_f64(scene3d, "render_passes_avg"), 5.0);
   assert_eq!(report_f64(scene3d, "scene3d_bloom_source_passes_avg"), 1.0);
   assert_eq!(report_f64(scene3d, "scene3d_bloom_source_draws_avg"), 96.0);
   assert_eq!(report_f64(scene3d, "scene3d_bloom_graph_resources_avg"), 3.0);
   assert_eq!(report_f64(scene3d, "scene3d_bloom_graph_alias_slots_avg"), 2.0);
   assert_eq!(report_f64(scene3d, "scene3d_bloom_graph_plan_builds_avg"), 0.0);
   assert_eq!(report_f64(scene3d, "scene3d_bloom_graph_plan_reuses_avg"), 1.0);
   assert!(report_f64(scene3d, "c58_frame_ms_0003") > 0.0);
   assert!(report_f64(scene3d, "c58_encode_ms_0003") > 0.0);
   assert!(report_f64(scene3d, "c58_gpu_ms_0003") > 0.0);
   assert_eq!(report_f64(scene3d_three, "render_passes_avg"), 11.0);
   assert_eq!(report_f64(scene3d_three, "scene3d_bloom_source_passes_avg"), 1.0);
   assert_eq!(report_f64(scene3d_three, "scene3d_bloom_source_draws_avg"), 96.0);
   assert_eq!(report_f64(scene3d_three, "scene3d_bloom_extract_passes_avg"), 1.0);
   assert_eq!(report_f64(scene3d_three, "scene3d_bloom_downsample_passes_avg"), 1.0);
   assert_eq!(report_f64(scene3d_three, "scene3d_bloom_blur_horizontal_passes_avg"), 3.0);
   assert_eq!(report_f64(scene3d_three, "scene3d_bloom_blur_vertical_passes_avg"), 3.0);
   assert_eq!(report_f64(scene3d_three, "scene3d_bloom_upsample_passes_avg"), 3.0);
   assert_eq!(report_f64(scene3d_three, "scene3d_bloom_composite_passes_avg"), 3.0);
   assert_eq!(report_f64(scene3d_three, "scene3d_bloom_graph_resources_avg"), 7.0);
   assert_eq!(report_f64(scene3d_three, "scene3d_bloom_graph_alias_slots_avg"), 3.0);
   assert!(report_f64(scene3d_three, "scene3d_bloom_graph_aliased_bytes_avg") > 0.0);
   assert!(report_f64(scene3d_three, "scene3d_bloom_bandwidth_bytes_avg") > 0.0);
   assert!(report_f64(scene3d_three, "scene3d_bloom_region_pixels_avg") > 0.0);
   assert!(report_f64(scene3d_viewport, "scene3d_bloom_bandwidth_bytes_avg")
      < report_f64(scene3d_three, "scene3d_bloom_bandwidth_bytes_avg"));
   assert!(report_f64(scene3d_viewport, "scene3d_bloom_region_pixels_avg")
      < report_f64(scene3d_three, "scene3d_bloom_region_pixels_avg"));
   assert_eq!(report_f64(scene3d_overlay, "overlay_control"), 1.0);
   assert_eq!(report_f64(scene3d_overlay, "render_passes_avg"), 12.0);
   assert_eq!(report_f64(scene3d_guard, "render_passes_avg"), 1.0);
   assert_eq!(report_f64(scene3d_guard, "scene3d_bloom_source_passes_avg"), 0.0);
   assert_eq!(report_f64(scene3d_guard, "scene3d_bloom_graph_resources_avg"), 0.0);
   let _ = std::fs::remove_file(json_out);
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "explicit touched-case perf contract"]
fn metal_effect_target_plan_reports_first_use_and_exact_residency()
{
   let mut json_out = std::env::temp_dir();
   json_out.push(format!("oxide-perf-runner-effect-targets-{}.json", std::process::id()));
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .env("OXIDE_PERF_RUNNER_FILTER", "gpu.architecture.effects.target_plan_")
      .env("OXIDE_ARCHITECTURE_METAL_FRAMES", "2")
      .env("OXIDE_ARCHITECTURE_METAL_WARMUPS", "1")
      .arg("--run-suite")
      .arg("--smoke")
      .arg("--json-out")
      .arg(&json_out)
      .output()
      .expect("run Metal effect-target plan smoke suite");
   let stdout = String::from_utf8_lossy(&output.stdout);
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(output.status.success(), "Metal effect-target suite failed: {stderr}");
   assert!(stdout.contains("cases=4"), "stdout: {stdout}");
   let report = std::fs::read_to_string(&json_out).expect("read effect-target report");
   let direct = report_case_slice(&report, "gpu.architecture.effects.target_plan_direct");
   let prepass = report_case_slice(&report, "gpu.architecture.effects.target_plan_prepass");
   let quarter = report_case_slice(&report, "gpu.architecture.effects.target_plan_quarter");
   let eighth = report_case_slice(&report, "gpu.architecture.effects.target_plan_eighth");

   assert_eq!(report_f64(direct, "first_resource_creates"), 1.0);
   assert_eq!(report_f64(direct, "resource_creates_total"), 1.0);
   assert_eq!(report_f64(direct, "effect_targets_bytes_peak"), 0.0);
   assert_eq!(report_f64(direct, "bloom_targets_bytes_peak"), 0.0);
   assert_eq!(report_f64(prepass, "first_resource_creates"), 2.0);
   assert_eq!(report_f64(prepass, "resource_creates_total"), 2.0);
   assert_eq!(report_f64(prepass, "effect_blur_chain_bytes_peak"), 0.0);
   assert_eq!(
      report_f64(prepass, "effect_targets_bytes_peak"),
      report_f64(prepass, "effect_prepass_bytes_peak"),
   );
   assert_eq!(report_f64(quarter, "first_resource_creates"), 5.0);
   assert_eq!(report_f64(quarter, "resource_creates_total"), 5.0);
   assert_eq!(report_f64(eighth, "first_resource_creates"), 6.0);
   assert_eq!(report_f64(eighth, "resource_creates_total"), 6.0);
   assert_eq!(
      report_f64(quarter, "effect_prepass_bytes_peak"),
      report_f64(eighth, "effect_prepass_bytes_peak"),
   );
   assert!(
      report_f64(eighth, "effect_targets_bytes_peak")
         < report_f64(quarter, "effect_targets_bytes_peak"),
   );
   for row in [direct, prepass, quarter, eighth]
   {
      assert!(report_f64(row, "first_frame_ms") > 0.0);
      assert!(report_f64(row, "first_encode_ms") > 0.0);
      assert!(report_f64(row, "first_gpu_ms") > 0.0);
   }
   let _ = std::fs::remove_file(json_out);
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "explicit touched-case perf contract"]
fn metal_blur_sigma_sweep_freezes_quality_ladder_work()
{
   let mut json_out = std::env::temp_dir();
   json_out.push(format!("oxide-perf-runner-blur-sweep-{}.json", std::process::id()));
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .env("OXIDE_PERF_RUNNER_FILTER", "gpu.architecture.effects.blur_sigma_")
      .env("OXIDE_ARCHITECTURE_METAL_FRAMES", "2")
      .env("OXIDE_ARCHITECTURE_METAL_WARMUPS", "1")
      .arg("--run-suite")
      .arg("--smoke")
      .arg("--json-out")
      .arg(&json_out)
      .output()
      .expect("run Metal blur sigma sweep");
   let stdout = String::from_utf8_lossy(&output.stdout);
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(output.status.success(), "Metal blur sigma sweep failed: {stderr}");
   assert!(stdout.contains("cases=5"), "stdout: {stdout}");
   let report = std::fs::read_to_string(&json_out).expect("read blur sigma report");
   let sigma2 = report_case_slice(&report, "gpu.architecture.effects.blur_sigma_2_local");
   let sigma8 = report_case_slice(&report, "gpu.architecture.effects.blur_sigma_8_local");
   let sigma16 = report_case_slice(&report, "gpu.architecture.effects.blur_sigma_16_fullscreen");
   let sigma32 = report_case_slice(&report, "gpu.architecture.effects.blur_sigma_32_fullscreen");
   let sigma64 = report_case_slice(&report, "gpu.architecture.effects.blur_sigma_64_fullscreen");

   for (row, sigma, radius, source_samples, encoded_samples, exp_taps, paired, exact) in [
      (sigma2, 2.0, 2.0, 10.0, 10.0, 4.0, 0.0, 2.0),
      (sigma8, 8.0, 6.0, 26.0, 14.0, 0.0, 2.0, 0.0),
      (sigma16, 16.0, 12.0, 50.0, 26.0, 0.0, 2.0, 0.0),
      (sigma32, 32.0, 24.0, 98.0, 50.0, 0.0, 2.0, 0.0),
      (sigma64, 64.0, 48.0, 194.0, 98.0, 0.0, 2.0, 0.0),
   ]
   {
      assert_eq!(report_f64(row, "blur_source_sigma_dp"), sigma);
      assert_eq!(report_f64(row, "blur_pass_radius_px"), radius);
      assert_eq!(report_f64(row, "blur_kernel_source_samples_avg"), source_samples);
      assert_eq!(report_f64(row, "blur_kernel_encoded_samples_avg"), encoded_samples);
      assert_eq!(report_f64(row, "blur_kernel_runtime_exp_taps_avg"), exp_taps);
      assert_eq!(report_f64(row, "blur_kernel_paired_passes_avg"), paired);
      assert_eq!(report_f64(row, "blur_kernel_exact_passes_avg"), exact);
   }
   assert_eq!(report_f64(sigma2, "blur_kernel_sample_reduction_pct"), 0.0);
   assert!(report_f64(sigma8, "blur_kernel_sample_reduction_pct") >= 46.0);
   assert!(report_f64(sigma16, "blur_kernel_sample_reduction_pct") >= 48.0);
   assert!(report_f64(sigma64, "blur_kernel_sample_reduction_pct") >= 49.0);
   assert!(report_f64(sigma8, "blur_kernel_table_bytes_peak") > 0.0);
   assert!(report_f64(sigma16, "blur_kernel_table_bytes_peak") > 0.0);
   assert!(report_f64(sigma64, "blur_kernel_table_bytes_peak")
      > report_f64(sigma16, "blur_kernel_table_bytes_peak"));
   let _ = std::fs::remove_file(json_out);
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "explicit touched-case perf contract"]
fn metal_final_target_rows_freeze_direct_and_persistent_paths()
{
   let mut json_out = std::env::temp_dir();
   json_out.push(format!("oxide-perf-runner-final-target-{}.json", std::process::id()));
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .env("OXIDE_PERF_RUNNER_FILTER", "gpu.architecture.final_target.")
      .env("OXIDE_ARCHITECTURE_METAL_FRAMES", "2")
      .env("OXIDE_ARCHITECTURE_METAL_WARMUPS", "1")
      .arg("--run-suite")
      .arg("--smoke")
      .arg("--json-out")
      .arg(&json_out)
      .output()
      .expect("run Metal final-target smoke suite");
   let stdout = String::from_utf8_lossy(&output.stdout);
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(output.status.success(), "Metal final-target suite failed: {stderr}");
   assert!(stdout.contains("cases=2"), "stdout: {stdout}");
   let report = std::fs::read_to_string(&json_out).expect("read final-target report");
   let direct = report_case_slice(
      &report,
      "gpu.architecture.final_target.auxiliary_direct",
   );
   let partial = report_case_slice(
      &report,
      "gpu.architecture.final_target.partial_damage",
   );

   assert!(direct.contains("\"refresh_mode\": \"drawable-unthrottled\""));
   assert_eq!(report_f64(direct, "blit_passes_avg"), 0.0);
   assert_eq!(report_f64(direct, "texture_copies_avg"), 0.0);
   assert_eq!(report_f64(direct, "texture_copy_bytes_avg"), 0.0);
   assert_eq!(report_f64(direct, "persistent_target_frames"), 0.0);
   assert_eq!(report_f64(direct, "draw_target_main_bytes_peak"), 0.0);
   assert_eq!(report_f64(partial, "blit_passes_avg"), 1.0);
   assert_eq!(report_f64(partial, "texture_copies_avg"), 1.0);
   assert_eq!(report_f64(partial, "texture_copy_bytes_avg"), 3_840_000.0);
   assert_eq!(report_f64(partial, "persistent_target_frames"), 2.0);
   assert!(report_f64(partial, "draw_target_main_bytes_peak") >= 3_840_000.0);
   let _ = std::fs::remove_file(json_out);
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "explicit touched-case perf contract"]
fn metal_frame_resource_rows_freeze_visible_and_offscreen_depth_contracts()
{
   let mut json_out = std::env::temp_dir();
   json_out.push(format!("oxide-perf-runner-frame-resources-{}.json", std::process::id()));
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .env("OXIDE_PERF_RUNNER_FILTER", "gpu.architecture.frame_resources.")
      .arg("--run-suite")
      .arg("--smoke")
      .arg("--json-out")
      .arg(&json_out)
      .output()
      .expect("run Metal frame-resource smoke suite");
   let stdout = String::from_utf8_lossy(&output.stdout);
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(output.status.success(), "Metal frame-resource suite failed: {stderr}");
   assert!(stdout.contains("cases=2"), "stdout: {stdout}");
   let report = std::fs::read_to_string(&json_out).expect("read frame-resource report");
   let visible = report_case_slice(
      &report,
      "gpu.architecture.frame_resources.visible_high_water",
   );
   let offscreen = report_case_slice(
      &report,
      "gpu.architecture.frame_resources.offscreen_growth_stress",
   );

   assert_eq!(report_f64(visible, "frame_resource_depth"), 3.0);
   assert_eq!(report_f64(visible, "frame_ring_buffer_bytes_peak"), 2_064_384.0);
   assert_eq!(report_f64(visible, "cold_resource_grows"), 0.0);
   assert_eq!(report_f64(visible, "warm_resource_grows"), 0.0);
   assert_eq!(report_f64(visible, "vertex_upload_bytes"), 327_680.0);
   assert_eq!(report_f64(visible, "index_upload_bytes"), 49_152.0);
   assert_eq!(report_f64(visible, "uniform_upload_bytes"), 16.0);
   assert!(report_f64(visible, "gpu_ms_p50") > 0.0);
   assert!(report_f64(visible, "gpu_ms_p95") > 0.0);
   assert!(report_f64(visible, "gpu_ms_p99") > 0.0);
   assert!(report_f64(visible, "gpu_ms_peak") > 0.0);
   assert_eq!(report_f64(offscreen, "frame_resource_depth"), 8.0);
   assert_eq!(report_f64(offscreen, "frame_ring_buffer_bytes_peak"), 7_864_320.0);
   assert_eq!(report_f64(offscreen, "cold_resource_grows"), 16.0);
   assert_eq!(report_f64(offscreen, "warm_resource_grows"), 0.0);
   assert_eq!(report_f64(offscreen, "frame_backpressure_skips"), 0.0);
   assert_eq!(report_f64(offscreen, "vertex_upload_bytes"), 655_360.0);
   assert_eq!(report_f64(offscreen, "index_upload_bytes"), 98_304.0);
   assert_eq!(report_f64(offscreen, "uniform_upload_bytes"), 16.0);
   assert!(report_f64(offscreen, "gpu_ms_p50") > 0.0);
   assert!(report_f64(offscreen, "gpu_ms_p95") > 0.0);
   assert!(report_f64(offscreen, "gpu_ms_p99") > 0.0);
   assert!(report_f64(offscreen, "gpu_ms_peak") > 0.0);
   let _ = std::fs::remove_file(json_out);
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "explicit touched-case perf contract"]
fn metal_prepared_chunk_rows_freeze_clean_and_one_dirty_contracts()
{
   let mut json_out = std::env::temp_dir();
   json_out.push(format!("oxide-perf-runner-prepared-chunks-{}.json", std::process::id()));
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .env(
         "OXIDE_PERF_RUNNER_FILTER",
         "gpu.architecture.prepared_chunks.one_dirty,gpu.authoring.retained_snapshot.clean_mixed",
      )
      .arg("--run-suite")
      .arg("--smoke")
      .arg("--json-out")
      .arg(&json_out)
      .output()
      .expect("run Metal prepared-chunk smoke suite");
   let stdout = String::from_utf8_lossy(&output.stdout);
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(output.status.success(), "Metal prepared-chunk suite failed: {stderr}");
   assert!(stdout.contains("cases=2"), "stdout: {stdout}");
   let report = std::fs::read_to_string(&json_out).expect("read prepared-chunk report");
   let clean = report_case_slice(&report, "gpu.authoring.retained_snapshot.clean_mixed");
   let dirty = report_case_slice(&report, "gpu.architecture.prepared_chunks.one_dirty");

   assert_eq!(report_f64(clean, "chunk_count"), 256.0);
   assert_eq!(report_f64(clean, "backend_cache_hits_avg"), 256.0);
   assert_eq!(report_f64(clean, "backend_cache_misses_avg"), 0.0);
   assert_eq!(report_f64(clean, "chunks_prepared_avg"), 0.0);
   assert_eq!(report_f64(clean, "commands_traversed_avg"), 0.0);
   assert_eq!(report_f64(clean, "geometry_bytes_copied_avg"), 0.0);
   assert_eq!(report_f64(clean, "buffer_upload_bytes_avg"), 0.0);
   assert_eq!(report_f64(clean, "dynamic_uniform_upload_bytes_avg"), 256.0 * 48.0);
   assert_eq!(report_f64(dirty, "backend_cache_hits_avg"), 255.0);
   assert_eq!(report_f64(dirty, "backend_cache_misses_avg"), 1.0);
   assert_eq!(report_f64(dirty, "chunks_prepared_avg"), 1.0);
   assert_eq!(report_f64(dirty, "commands_traversed_avg"), 64.0);
   assert_eq!(report_f64(dirty, "geometry_bytes_copied_avg"), 3_072.0);
   assert_eq!(report_f64(dirty, "buffer_upload_bytes_avg"), 3_072.0);
   assert_eq!(report_f64(dirty, "dynamic_uniform_upload_bytes_avg"), 256.0 * 48.0);
   assert!(report_f64(clean, "prepared_cache_bytes_peak") > 0.0);
   assert_eq!(
      report_f64(clean, "prepared_cache_bytes_peak"),
      report_f64(dirty, "prepared_cache_bytes_peak"),
   );
   let _ = std::fs::remove_file(json_out);
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "explicit touched-case perf contract"]
fn metal_prepared_layer_rows_freeze_body_free_clean_and_single_dirty_contracts()
{
   let mut json_out = std::env::temp_dir();
   json_out.push(format!("oxide-perf-runner-prepared-layers-{}.json", std::process::id()));
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .env(
         "OXIDE_PERF_RUNNER_FILTER",
         "gpu.architecture.prepared_layers.,gpu.authoring.retained_snapshot.prepared_layers_",
      )
      .env("OXIDE_ARCHITECTURE_METAL_WARMUPS", "2")
      .env("OXIDE_ARCHITECTURE_METAL_FRAMES", "4")
      .arg("--run-suite")
      .arg("--smoke")
      .arg("--json-out")
      .arg(&json_out)
      .output()
      .expect("run Metal prepared-layer smoke suite");
   let stdout = String::from_utf8_lossy(&output.stdout);
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(output.status.success(), "Metal prepared-layer suite failed: {stderr}");
   assert!(stdout.contains("cases=2"), "stdout: {stdout}");
   let report = std::fs::read_to_string(&json_out).expect("read prepared-layer report");
   let clean = report_case_slice(
      &report,
      "gpu.authoring.retained_snapshot.prepared_layers_clean_100x100",
   );
   let dirty = report_case_slice(&report, "gpu.architecture.prepared_layers.one_dirty_100x100");

   assert!(clean.contains("\"family\": \"authoring\""));
   assert_eq!(report_f64(clean, "layers"), 100.0);
   assert_eq!(report_f64(clean, "draws_per_layer"), 100.0);
   assert_eq!(report_f64(clean, "layer_body_commands_scanned_avg"), 0.0);
   assert_eq!(report_f64(clean, "layer_body_commands_copied_avg"), 0.0);
   assert_eq!(report_f64(clean, "geometry_bytes_copied_avg"), 0.0);
   assert_eq!(report_f64(clean, "buffer_upload_bytes_avg"), 0.0);
   assert_eq!(report_f64(clean, "layer_texture_creates_avg"), 0.0);
   assert_eq!(report_f64(clean, "layer_cache_hits_avg"), 100.0);
   assert_eq!(report_f64(clean, "layer_cache_misses_avg"), 0.0);
   assert_eq!(report_f64(clean, "layer_offscreen_draws_avg"), 0.0);
   assert_eq!(report_f64(clean, "render_passes_avg"), 1.0);
   assert_eq!(report_f64(clean, "draws_avg"), 100.0);
   assert_eq!(report_f64(clean, "chunks_prepared_avg"), 0.0);
   assert!(report_f64(clean, "layer_cache_bytes_peak") > 0.0);
   assert_eq!(report_f64(dirty, "dirty_layers_per_frame"), 1.0);
   assert_eq!(report_f64(dirty, "layer_body_commands_scanned_avg"), 0.0);
   assert_eq!(report_f64(dirty, "layer_body_commands_copied_avg"), 0.0);
   assert_eq!(report_f64(dirty, "geometry_bytes_copied_avg"), 0.0);
   assert_eq!(report_f64(dirty, "buffer_upload_bytes_avg"), 0.0);
   assert_eq!(report_f64(dirty, "layer_texture_creates_avg"), 0.0);
   assert_eq!(report_f64(dirty, "layer_cache_hits_avg"), 99.0);
   assert_eq!(report_f64(dirty, "layer_cache_misses_avg"), 1.0);
   assert_eq!(report_f64(dirty, "layer_offscreen_draws_avg"), 1.0);
   assert_eq!(report_f64(dirty, "render_passes_avg"), 2.0);
   assert_eq!(report_f64(dirty, "draws_avg"), 101.0);
   assert_eq!(report_f64(dirty, "chunks_prepared_avg"), 0.0);
   let _ = std::fs::remove_file(json_out);
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "explicit touched-case perf contract"]
fn metal_dynamic_property_row_freezes_zero_geometry_upload_contract()
{
   let mut json_out = std::env::temp_dir();
   json_out.push(format!("oxide-perf-runner-dynamic-properties-{}.json", std::process::id()));
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .env("OXIDE_PERF_RUNNER_FILTER", "gpu.architecture.animation.dynamic_properties_300")
      .arg("--run-suite")
      .arg("--smoke")
      .arg("--json-out")
      .arg(&json_out)
      .output()
      .expect("run Metal dynamic-property smoke row");
   let stderr = String::from_utf8_lossy(&output.stderr);
   assert!(output.status.success(), "Metal dynamic-property row failed: {stderr}");
   let report = std::fs::read_to_string(&json_out).expect("read dynamic-property report");
   let row = report_case_slice(&report, "gpu.architecture.animation.dynamic_properties_300");

   assert_eq!(report_f64(row, "animated_nodes"), 300.0);
   assert_eq!(report_f64(row, "text_nodes"), 200.0);
   assert_eq!(report_f64(row, "image_nodes"), 100.0);
   assert_eq!(report_f64(row, "property_records"), 300.0);
   assert_eq!(report_f64(row, "property_records_updated_avg"), 300.0);
   assert_eq!(report_f64(row, "property_upload_bytes_avg"), 300.0 * 48.0);
   assert_eq!(report_f64(row, "buffer_upload_bytes_avg"), 0.0);
   assert_eq!(report_f64(row, "geometry_bytes_copied_avg"), 0.0);
   assert_eq!(report_f64(row, "commands_traversed_avg"), 0.0);
   assert_eq!(report_f64(row, "backend_cache_hits_avg"), 300.0);
   assert_eq!(report_f64(row, "backend_cache_misses_avg"), 0.0);
   assert_eq!(report_f64(row, "missed_frames_120hz"), 0.0);
   assert!(report_f64(row, "property_ring_bytes_peak") >= 300.0 * 48.0 * 3.0);
   let _ = std::fs::remove_file(json_out);
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "explicit touched-case perf contract"]
fn metal_spatial_rows_freeze_small_and_full_damage_contracts()
{
   let mut json_out = std::env::temp_dir();
   json_out.push(format!("oxide-perf-runner-spatial-metal-{}.json", std::process::id()));
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .env(
         "OXIDE_PERF_RUNNER_FILTER",
         "gpu.architecture.spatial_metadata.full_damage_glyph_mesh_10000,gpu.authoring.retained_snapshot.spatial_damage_10000",
      )
      .arg("--run-suite")
      .arg("--smoke")
      .arg("--json-out")
      .arg(&json_out)
      .output()
      .expect("run Metal spatial rows");
   let stderr = String::from_utf8_lossy(&output.stderr);
   assert!(output.status.success(), "Metal spatial rows failed: {stderr}");
   let report = std::fs::read_to_string(&json_out).expect("read Metal spatial report");
   let small = report_case_slice(
      &report,
      "gpu.authoring.retained_snapshot.spatial_damage_10000",
   );
   let full = report_case_slice(
      &report,
      "gpu.architecture.spatial_metadata.full_damage_glyph_mesh_10000",
   );

   assert_eq!(report_f64(small, "instance_count"), 512.0);
   assert_eq!(report_f64(small, "damage_instances_visited_avg"), 1.0);
   assert_eq!(report_f64(small, "damage_instances_matched_avg"), 1.0);
   assert_eq!(report_f64(small, "damage_commands_visited_avg"), 1.0);
   assert_eq!(report_f64(small, "damage_commands_matched_avg"), 1.0);
   assert_eq!(report_f64(small, "damage_vertices_visited_avg"), 0.0);
   assert_eq!(report_f64(small, "prepared_plan_reuses_avg"), 0.0);
   assert_eq!(report_f64(small, "draws_avg"), 1.0);
   assert_eq!(report_f64(small, "geometry_bytes_copied_avg"), 0.0);
   assert_eq!(report_f64(small, "buffer_upload_bytes_avg"), 0.0);
   assert_eq!(report_f64(small, "shaded_damage_pixels_avg"), 4.0);
   assert_eq!(report_f64(full, "damage_instances_visited_avg"), 0.0);
   assert_eq!(report_f64(full, "damage_commands_visited_avg"), 0.0);
   assert_eq!(report_f64(full, "damage_vertices_visited_avg"), 0.0);
   assert_eq!(report_f64(full, "prepared_plan_reuses_avg"), 1.0);
   assert_eq!(report_f64(full, "draws_avg"), 512.0);
   assert_eq!(report_f64(full, "geometry_bytes_copied_avg"), 0.0);
   assert_eq!(report_f64(full, "buffer_upload_bytes_avg"), 0.0);
   assert_eq!(report_f64(full, "shaded_damage_pixels_avg"), 1_200.0 * 800.0);
   let _ = std::fs::remove_file(json_out);
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "explicit touched-case perf contract"]
fn metal_immutable_image_rows_freeze_residency_mip_and_quality_contracts()
{
   let mut json_out = std::env::temp_dir();
   json_out.push(format!("oxide-perf-runner-immutable-images-{}.json", std::process::id()));
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .env(
         "OXIDE_PERF_RUNNER_FILTER",
         "gpu.architecture.images.immutable_large_auto,gpu.architecture.images.immutable_minified_shared,gpu.architecture.images.immutable_minified_shared_mipmapped,gpu.architecture.images.immutable_minified_mipmapped,gpu.architecture.images.immutable_small_one_use_auto,gpu.authoring.image_view_grid.immutable_minified",
      )
      .env("OXIDE_C59_METAL_WARMUPS", "1")
      .env("OXIDE_C59_METAL_FRAMES", "2")
      .env("OXIDE_C59_RAW_SAMPLES", "1")
      .arg("--run-suite")
      .arg("--smoke")
      .arg("--json-out")
      .arg(&json_out)
      .output()
      .expect("run immutable-image Metal rows");
   let stderr = String::from_utf8_lossy(&output.stderr);
   assert!(output.status.success(), "immutable-image rows failed: {stderr}");
   let report = std::fs::read_to_string(&json_out).expect("read immutable-image report");
   let large = report_case_slice(&report, "gpu.architecture.images.immutable_large_auto");
   let shared = report_case_slice(&report, "gpu.architecture.images.immutable_minified_shared");
   let mipmapped = report_case_slice(
      &report,
      "gpu.architecture.images.immutable_minified_mipmapped",
   );
   let shared_mipmapped = report_case_slice(
      &report,
      "gpu.architecture.images.immutable_minified_shared_mipmapped",
   );
   let small = report_case_slice(
      &report,
      "gpu.architecture.images.immutable_small_one_use_auto",
   );
   let authoring = report_case_slice(
      &report,
      "gpu.authoring.image_view_grid.immutable_minified",
   );

   assert_eq!(report_f64(large, "shared_textures"), 1.0);
   assert_eq!(report_f64(large, "private_textures"), 0.0);
   assert_eq!(report_f64(large, "mipmapped_textures"), 0.0);
   assert_eq!(report_f64(large, "upload_command_buffers"), 0.0);
   assert_eq!(
      report_f64(large, "creation_peak_texture_bytes"),
      report_f64(large, "shared_bytes"),
   );
   assert!(report_f64(large, "first_visible_ms") > 0.0);
   assert_eq!(report_f64(shared, "shared_textures"), 1.0);
   assert_eq!(report_f64(shared, "mip_levels"), 1.0);
   assert_eq!(report_f64(shared_mipmapped, "shared_textures"), 1.0);
   assert_eq!(report_f64(shared_mipmapped, "private_textures"), 0.0);
   assert_eq!(report_f64(shared_mipmapped, "mip_levels"), 11.0);
   assert_eq!(report_f64(shared_mipmapped, "mipmap_generations"), 1.0);
   assert_eq!(report_f64(mipmapped, "private_textures"), 1.0);
   assert_eq!(report_f64(mipmapped, "mip_levels"), 11.0);
   assert_eq!(report_f64(mipmapped, "mipmap_generations"), 1.0);
   assert!(
      report_f64(mipmapped, "first_visible_spatial_variance") * 4.0
         < report_f64(shared, "first_visible_spatial_variance"),
   );
   assert!(
      report_f64(shared_mipmapped, "first_visible_spatial_variance") * 4.0
         < report_f64(shared, "first_visible_spatial_variance"),
   );
   assert_eq!(
      report_f64(shared_mipmapped, "first_visible_spatial_variance"),
      report_f64(mipmapped, "first_visible_spatial_variance"),
   );
   assert_eq!(report_f64(small, "private_uploads"), 0.0);
   assert_eq!(report_f64(small, "resident_shared_textures_after_release"), 0.0);
   assert_eq!(report_f64(small, "resident_private_textures_after_release"), 0.0);
   assert_eq!(report_f64(small, "staging_upload_bytes_per_create"), 0.0);
   assert_eq!(
      report_f64(small, "creation_peak_texture_bytes"),
      report_f64(small, "sampled_resident_bytes_peak"),
   );
   assert!(report_f64(small, "first_visible_ms_p50") > 0.0);
   assert!(report_f64(mipmapped, "c59_frame_ms_0001") > 0.0);
   assert!(report_f64(mipmapped, "c59_gpu_ms_0001") > 0.0);
   assert!(authoring.contains("\"family\": \"authoring\""));
   assert_eq!(report_f64(authoring, "image_view_encodes"), 1_089.0);
   assert_eq!(report_f64(authoring, "shared_textures"), 1.0);
   assert_eq!(report_f64(authoring, "private_textures"), 0.0);
   assert_eq!(report_f64(authoring, "mip_levels"), 11.0);
   let _ = std::fs::remove_file(json_out);
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "explicit touched-case perf contract"]
fn metal_image_store_rows_freeze_scaling_completion_and_reuse_contracts()
{
   let mut json_out = std::env::temp_dir();
   json_out.push(format!("oxide-perf-runner-image-store-{}.json", std::process::id()));
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .env(
         "OXIDE_PERF_RUNNER_FILTER",
         "gpu.architecture.images.icons_100,gpu.architecture.images.icons_1000,gpu.architecture.images.icons_10000,gpu.authoring.image_store.atlas_grid_1000",
      )
      .arg("--run-suite")
      .arg("--smoke")
      .arg("--json-out")
      .arg(&json_out)
      .output()
      .expect("run image-store Metal rows");
   let stderr = String::from_utf8_lossy(&output.stderr);
   assert!(output.status.success(), "image-store rows failed: {stderr}");
   let report = std::fs::read_to_string(&json_out).expect("read image-store report");
   for (id, count, pages) in [
      ("gpu.architecture.images.icons_100", 100.0, 1.0),
      ("gpu.architecture.images.icons_1000", 1_000.0, 4.0),
      ("gpu.architecture.images.icons_10000", 10_000.0, 40.0),
   ]
   {
      let row = report_case_slice(&report, id);
      assert_eq!(report_f64(row, "unique_images"), count);
      assert_eq!(report_f64(row, "display_decode_bytes"), count * 28.0 * 28.0 * 4.0);
      assert_eq!(report_f64(row, "first_publications"), count);
      assert_eq!(report_f64(row, "uploaded_images"), count);
      assert_eq!(report_f64(row, "atlas_pages"), pages);
      assert_eq!(report_f64(row, "texture_creates"), pages);
      assert_eq!(report_f64(row, "gpu_resident_bytes"), pages * 512.0 * 512.0 * 4.0);
      assert_eq!(report_f64(row, "atlas_slots"), count);
      assert_eq!(report_f64(row, "standalone_images"), 0.0);
      assert_eq!(report_f64(row, "atlas_page_clear_bytes"), 0.0);
      assert_eq!(report_f64(row, "draws_avg"), 1.0);
      assert!(report_f64(row, "request_to_first_completed_frame_ms") > 0.0);
      assert!(report_f64(row, "store_request_to_first_publication_ms_avg") > 0.0);
      assert!(report_f64(row, "first_visible_gpu_ms") > 0.0);
      assert!(report_f64(row, "first_completed_frame_spatial_variance") > 0.0);
      assert!(report_f64(row, "decoded_peak_bytes") <= 64.0 * 1024.0 * 1024.0);
      assert!(report_f64(row, "gpu_peak_bytes") <= 64.0 * 1024.0 * 1024.0);
      assert!(!row.contains("event_to_first_visible_ms"));
   }

   let authoring = report_case_slice(
      &report,
      "gpu.authoring.image_store.atlas_grid_1000",
   );
   assert!(authoring.contains("\"family\": \"authoring\""));
   assert_eq!(report_f64(authoring, "release_reuse_uploaded"), 64.0);
   assert_eq!(report_f64(authoring, "prepared_chunk_invalidations"), 64.0);
   assert_eq!(report_f64(authoring, "slot_generation_changes"), 64.0);
   let _ = std::fs::remove_file(json_out);
}

#[test]
#[ignore = "explicit touched-case perf contract"]
fn filtered_run_suite_supports_gpu_journey_frame_pacing_case() {
    let mut json_out = std::env::temp_dir();
    json_out.push(format!("oxide-perf-runner-gpu-journey-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "gpu.journey.collection_navigation.frame_pacing")
        .arg("--run-suite")
        .arg("--smoke")
        .arg("--json-out")
        .arg(&json_out)
        .output()
        .expect("run filtered gpu journey smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=1"), "stdout: {stdout}");
    assert!(
        stdout.contains("case=gpu.journey.collection_navigation.frame_pacing"),
        "stdout: {stdout}",
    );
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");

    let report = std::fs::read_to_string(&json_out).expect("read gpu journey report");
    let row = report_case_slice(&report, "gpu.journey.collection_navigation.frame_pacing");
    assert!(report_f64(row, "frame_ms_p50") > 0.0);
    assert!(report_f64(row, "event_to_visible_ms_p50") > 0.0);
    assert!(report_f64(row, "gpu_ms_p50") > 0.0);
    assert_eq!(report_f64(row, "missed_frame_ratio_120hz"), 0.0);
    assert_eq!(report_f64(row, "hitch_ratio_120hz"), 0.0);
    assert!(report_f64(row, "navigation_events") > 0.0);
    assert_eq!(report_f64(row, "frame_resource_depth"), 3.0);
    assert_eq!(report_f64(row, "frame_ring_buffer_bytes_peak"), 2_064_384.0);
    assert_eq!(report_f64(row, "resource_grows_total"), 0.0);
    assert_eq!(report_f64(row, "frame_backpressure_skips"), 0.0);
    let _ = std::fs::remove_file(json_out);
}
