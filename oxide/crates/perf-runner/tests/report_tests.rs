use oxide_perf_runner::{
    assert_case_metric_contract, assert_contract_coverage, assert_full_coverage,
    collect_suite_report, compare_reports, render_report_markdown, AuditFinding,
    ContractCoverageEntry, ContractCoverageReport, CoverageReport, PerfCaseResult, PerfReport,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::process::Command;

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

fn workspace_latest_report() -> PerfReport {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../benchmarks/workspace/latest.json");
    let bytes = std::fs::read(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    serde_json::from_slice(&bytes).unwrap_or_else(|err| panic!("parse {}: {err}", path.display()))
}

fn workspace_case<'a>(report: &'a PerfReport, id: &str) -> &'a PerfCaseResult {
    report
        .cases
        .iter()
        .find(|case| case.id == id)
        .unwrap_or_else(|| panic!("missing workspace case {id}"))
}

fn workspace_metric(case: &PerfCaseResult, key: &str) -> f64 {
    *case.metrics.get(key).unwrap_or_else(|| panic!("{} missing metric {key}", case.id))
}

fn sample_report(cases: Vec<PerfCaseResult>) -> PerfReport {
    PerfReport {
        version: 1,
        suite: String::from("test"),
        generated_label: None,
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
fn full_coverage_check_accepts_complete_registry_counts() {
    let coverage = CoverageReport {
        components_total: 2,
        components_covered: vec![String::from("Button"), String::from("Label")],
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
    };

    assert!(assert_full_coverage(&coverage).is_ok());
}

#[test]
fn full_coverage_check_rejects_missing_journey_coverage() {
    let coverage = CoverageReport {
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
        journeys_total: 2,
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
    };

    assert!(assert_full_coverage(&coverage).is_err());
}

#[test]
fn full_coverage_check_rejects_missing_authoring_coverage() {
    let coverage = CoverageReport {
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
        authoring_total: 2,
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
    };

    assert!(assert_full_coverage(&coverage).is_err());
}

#[test]
fn full_coverage_check_rejects_missing_bridge_coverage() {
    let coverage = CoverageReport {
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
        bridges_total: 2,
        bridges_covered: vec![String::from("Permission Callback Fanout")],
    };

    assert!(assert_full_coverage(&coverage).is_err());
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

#[test]
fn workspace_latest_frame_rows_satisfy_metric_contract() {
    let report = workspace_latest_report();

    assert!(report.cases.iter().any(|case| case.unit == "ms/frame"));
    assert_case_metric_contract(&report.cases)
        .unwrap_or_else(|err| panic!("workspace latest frame metric contract failed: {err}"));
}

fn assert_workspace_metal_pacing_row(case: &PerfCaseResult, family: &str, scenario: &str) {
    assert_eq!(case.layer, "flow");
    assert_eq!(case.family, family);
    assert_eq!(case.scenario, scenario);
    assert_eq!(case.variant, "oxide-metal");
    assert_eq!(case.cache_state, "warm");
    assert_eq!(case.refresh_mode, "60hz-and-120hz-budget");
    assert_eq!(case.unit, "ms/frame");
    assert!(case.gated);
    assert!(case.samples > 0);
    assert_eq!(case.ops_per_sample, 1);

    assert_eq!(workspace_metric(case, "frame_ms_p50"), case.median);
    assert_eq!(workspace_metric(case, "frame_ms_p95"), case.p95);
    assert_eq!(workspace_metric(case, "frame_ms_p99"), case.p99);
    assert_eq!(workspace_metric(case, "frame_ms_peak"), case.max);
    assert!(case.median > 0.0);
    assert!(case.p95 >= case.median);
    assert!(case.p99 >= case.p95);
    assert!(case.max >= case.p99);

    assert!(workspace_metric(case, "gpu_ms_p50") > 0.0);
    assert!(workspace_metric(case, "gpu_ms_p95") >= workspace_metric(case, "gpu_ms_p50"));
    assert!(workspace_metric(case, "gpu_ms_p99") >= workspace_metric(case, "gpu_ms_p95"));
    assert!(workspace_metric(case, "gpu_ms_peak") >= workspace_metric(case, "gpu_ms_p99"));

    assert!((workspace_metric(case, "frame_budget_60hz_ms") - (1000.0 / 60.0)).abs() < 0.0001);
    assert!((workspace_metric(case, "frame_budget_120hz_ms") - (1000.0 / 120.0)).abs() < 0.0001);
    assert_eq!(workspace_metric(case, "missed_frames_60hz"), 0.0);
    assert_eq!(workspace_metric(case, "missed_frames_120hz"), 0.0);
    assert_eq!(workspace_metric(case, "missed_frame_ratio_60hz"), 0.0);
    assert_eq!(workspace_metric(case, "missed_frame_ratio_120hz"), 0.0);
    assert_eq!(workspace_metric(case, "hitch_frames_60hz"), 0.0);
    assert_eq!(workspace_metric(case, "hitch_frames_120hz"), 0.0);
    assert_eq!(workspace_metric(case, "hitch_ratio_60hz"), 0.0);
    assert_eq!(workspace_metric(case, "hitch_ratio_120hz"), 0.0);
}

#[test]
fn workspace_latest_gates_mac_metal_animation_and_navigation_pacing_rows() {
    let report = workspace_latest_report();
    let animation = workspace_case(&report, "gpu.animation.effects.refresh_matrix");
    assert_workspace_metal_pacing_row(animation, "animation-effects", "animation-effects");
    assert_eq!(workspace_metric(animation, "refresh_matrix_rows"), 2.0);
    assert!(workspace_metric(animation, "draw_ms_median") > 0.0);
    assert!(workspace_metric(animation, "encode_ms_median") > 0.0);
    assert!(workspace_metric(animation, "draws_avg") > 0.0);
    assert!(workspace_metric(animation, "instanced_avg") > 0.0);
    assert!(workspace_metric(animation, "damage_pct_avg") > 0.0);

    let navigation = workspace_case(&report, "gpu.journey.collection_navigation.frame_pacing");
    assert_workspace_metal_pacing_row(navigation, "journey-gpu", "screen-flow");
    assert!(workspace_metric(navigation, "event_to_visible_ms_p50") > 0.0);
    assert!(
        workspace_metric(navigation, "event_to_visible_ms_p95")
            >= workspace_metric(navigation, "event_to_visible_ms_p50")
    );
    assert_eq!(workspace_metric(navigation, "frame_backpressure_skips"), 0.0);
    assert!(workspace_metric(navigation, "navigation_events") > 0.0);
    assert!(workspace_metric(navigation, "damage_rects_avg") > 0.0);
}

fn assert_workspace_cpu_row(case: &PerfCaseResult, family: &str, scenario: &str) {
    assert_eq!(case.layer, "engine");
    assert_eq!(case.family, family);
    assert_eq!(case.scenario, scenario);
    assert_eq!(case.variant, "oxide");
    assert_eq!(case.cache_state, "warm");
    assert_eq!(case.unit, "us/op");
    assert!(case.gated);
    assert!(case.samples > 0);
    assert!(case.ops_per_sample > 0);
    assert!(case.median > 0.0);
    assert!(case.p95 >= case.median);
    assert!(case.p99 >= case.p95);
    assert!(case.max >= case.p99);
}

fn assert_workspace_audit_row(case: &PerfCaseResult) {
    assert_eq!(case.layer, "engine");
    assert_eq!(case.family, "audit-baseline");
    assert_eq!(case.scenario, "audit-baseline");
    assert_eq!(case.variant, "legacy-baseline");
    assert_eq!(case.cache_state, "warm");
    assert_eq!(case.unit, "us/op");
    assert!(!case.gated);
    assert!(case.samples > 0);
    assert!(case.ops_per_sample > 0);
    assert!(case.median > 0.0);
    assert!(case.p95 >= case.median);
    assert!(case.p99 >= case.p95);
    assert!(case.max >= case.p99);
}

fn assert_workspace_zero_layout_dirty_row(case: &PerfCaseResult) {
    assert_workspace_cpu_row(case, "layout", "layout-invalidation");
    assert_eq!(workspace_metric(case, "dirty_nodes"), 1.0);
    assert_eq!(workspace_metric(case, "layout_passes"), 0.0);
    assert_eq!(workspace_metric(case, "layout_visited_nodes_per_op"), 0.0);
    assert_eq!(workspace_metric(case, "layout_measured_children_per_op"), 0.0);
    assert_eq!(workspace_metric(case, "layout_updates_per_op"), 0.0);
    assert!(workspace_metric(case, "layout_ops_sampled") > 0.0);
}

#[test]
fn workspace_latest_gates_retained_layout_dirty_class_rows() {
    let report = workspace_latest_report();

    let clean = workspace_case(&report, "cpu.authoring.surface_retained.clean_encode");
    assert_workspace_cpu_row(clean, "authoring", "authoring");
    assert_eq!(workspace_metric(clean, "retained_reuse_ratio"), 1.0);
    assert_eq!(workspace_metric(clean, "retained_rebuilt_ops"), 0.0);
    assert!(workspace_metric(clean, "retained_reused_ops") > 0.0);
    assert!(workspace_metric(clean, "draw_items") > 0.0);

    let dirty_leaf = workspace_case(&report, "cpu.authoring.surface_retained.dirty_leaf_encode");
    assert_workspace_cpu_row(dirty_leaf, "authoring", "authoring");
    assert_eq!(workspace_metric(dirty_leaf, "dirty_nodes"), 1.0);
    assert!(workspace_metric(dirty_leaf, "retained_node_reuse_ratio") > 0.9);
    assert!(
        workspace_metric(dirty_leaf, "retained_reused_nodes_per_op")
            > workspace_metric(dirty_leaf, "retained_rebuilt_nodes_per_op")
    );
    assert!(workspace_metric(dirty_leaf, "tracked_nodes") >= 1000.0);

    let text_atlas = workspace_case(&report, "cpu.authoring.surface_retained.text_atlas_context");
    assert_workspace_cpu_row(text_atlas, "authoring", "authoring");
    assert_eq!(workspace_metric(text_atlas, "retained_reuse_ratio"), 1.0);
    assert_eq!(workspace_metric(text_atlas, "retained_rebuilt_ops"), 0.0);
    assert!(workspace_metric(text_atlas, "retained_reused_ops") > 0.0);
    assert!(workspace_metric(text_atlas, "text_atlases_checked") >= 1.0);

    let transform = workspace_case(&report, "cpu.layout.transform_only.reposition");
    assert_workspace_zero_layout_dirty_row(transform);
    assert!(workspace_metric(transform, "retained_reused_nodes_per_op") > 0.0);
    assert!(workspace_metric(transform, "retained_rebuilt_nodes_per_op") > 0.0);

    let paint = workspace_case(&report, "cpu.layout.paint_only.opacity_clip");
    assert_workspace_zero_layout_dirty_row(paint);
    assert!(workspace_metric(paint, "opacity_ops") > 0.0);
    assert!(workspace_metric(paint, "clip_ops") > 0.0);
    assert!(workspace_metric(paint, "retained_reused_nodes_per_op") > 0.0);
    assert!(workspace_metric(paint, "retained_rebuilt_nodes_per_op") > 0.0);

    let content = workspace_case(&report, "cpu.layout.node_content_dirty.retained_replay");
    assert_workspace_zero_layout_dirty_row(content);
    assert!(workspace_metric(content, "text_dirty_ops") > 0.0);
    assert!(workspace_metric(content, "image_dirty_ops") > 0.0);
    assert!(workspace_metric(content, "camera_dirty_ops") > 0.0);
    assert!(workspace_metric(content, "retained_reused_nodes_per_op") > 0.0);
    assert!(workspace_metric(content, "retained_rebuilt_nodes_per_op") > 0.0);

    let non_draw = workspace_case(&report, "cpu.layout.non_draw_dirty.retained_reuse");
    assert_workspace_zero_layout_dirty_row(non_draw);
    assert_eq!(workspace_metric(non_draw, "retained_rebuilt_nodes_per_op"), 0.0);
    assert_eq!(workspace_metric(non_draw, "retained_rebuilt_ops"), 0.0);
    assert!(workspace_metric(non_draw, "retained_reused_nodes_per_op") > 0.0);
    assert!(workspace_metric(non_draw, "retained_reused_ops") > 0.0);
    assert!(workspace_metric(non_draw, "accessibility_dirty_ops") > 0.0);
    assert!(workspace_metric(non_draw, "hit_test_dirty_ops") > 0.0);
}

#[test]
fn workspace_latest_gates_collection_identity_and_prefix_ab_rows() {
    let report = workspace_latest_report();
    let indexed = workspace_case(&report, "cpu.authoring.collection_key_reconcile.indexed");
    let scan = workspace_case(&report, "cpu.authoring.collection_key_reconcile.scan");
    assert_workspace_cpu_row(indexed, "authoring", "authoring");
    assert_workspace_cpu_row(scan, "authoring", "authoring");
    assert!(indexed.median < scan.median);
    assert_eq!(workspace_metric(indexed, "collection_key_index_enabled"), 1.0);
    assert_eq!(workspace_metric(scan, "collection_key_index_enabled"), 0.0);
    assert!(workspace_metric(indexed, "collection_key_index_hits_total") > 0.0);
    assert_eq!(workspace_metric(scan, "collection_key_index_hits_total"), 0.0);
    assert_eq!(
        workspace_metric(indexed, "collection_key_index_queries_total"),
        workspace_metric(indexed, "collection_key_index_hits_total"),
    );
    assert!(
        workspace_metric(indexed, "collection_item_key_queries_per_lookup")
            < workspace_metric(scan, "collection_item_key_queries_per_lookup")
    );
    assert_eq!(
        workspace_metric(indexed, "collection_reconciled_index"),
        workspace_metric(scan, "collection_reconciled_index"),
    );

    let bounded_cache =
        workspace_case(&report, "cpu.authoring.collection_measure_cache.bounded_churn");
    assert_workspace_cpu_row(bounded_cache, "authoring", "authoring");
    assert!(workspace_metric(bounded_cache, "collection_count") >= 20_000.0);
    assert!(
        workspace_metric(bounded_cache, "collection_initial_measure_calls_per_op")
            >= workspace_metric(bounded_cache, "collection_count")
    );
    assert!(workspace_metric(bounded_cache, "collection_repair_measure_calls_per_op") > 0.0);
    assert!(workspace_metric(bounded_cache, "collection_repair_measure_calls_per_op") < 32.0);
    assert!(workspace_metric(bounded_cache, "collection_repair_to_initial_measure_ratio") < 0.01);
    assert!(workspace_metric(bounded_cache, "collection_repair_draw_items_per_op") > 0.0);

    let incremental = workspace_case(&report, "cpu.authoring.collection_prefix_update.incremental");
    let full_scan = workspace_case(&report, "cpu.authoring.collection_prefix_update.full_scan");
    assert_workspace_cpu_row(incremental, "authoring", "authoring");
    assert_workspace_cpu_row(full_scan, "authoring", "authoring");
    assert!(incremental.median < full_scan.median);
    assert_eq!(workspace_metric(incremental, "collection_changed_range_enabled"), 1.0);
    assert_eq!(workspace_metric(full_scan, "collection_changed_range_enabled"), 0.0);
    assert_eq!(
        workspace_metric(incremental, "collection_changed_index"),
        workspace_metric(full_scan, "collection_changed_index"),
    );
    assert!(
        workspace_metric(incremental, "collection_item_revision_queries_per_op")
            < workspace_metric(full_scan, "collection_item_revision_queries_per_op")
    );
    assert_eq!(
        workspace_metric(incremental, "collection_measure_calls_total"),
        workspace_metric(full_scan, "collection_measure_calls_total"),
    );
}

#[test]
fn workspace_latest_gates_text_cache_atlas_and_cursor_rows() {
    let report = workspace_latest_report();

    let prefix = workspace_case(&report, "cpu.system.text_prefix_width_map");
    assert_workspace_cpu_row(prefix, "system", "system");
    assert!(workspace_metric(prefix, "text_bytes") > 0.0);
    assert!(workspace_metric(prefix, "prefix_boundaries") > 0.0);
    assert_eq!(
        workspace_metric(prefix, "prefix_boundaries"),
        workspace_metric(prefix, "width_entries"),
    );
    assert_eq!(workspace_metric(prefix, "shaped_runs"), 1.0);

    let atlas_pressure = workspace_case(&report, "cpu.system.text_atlas_pressure");
    assert_workspace_cpu_row(atlas_pressure, "system", "system");
    assert!(workspace_metric(atlas_pressure, "atlas_shape_count") > 0.0);
    assert!(workspace_metric(atlas_pressure, "atlas_rendered_glyph_runs") > 0.0);
    assert!(workspace_metric(atlas_pressure, "atlas_evictions") > 0.0);
    assert_eq!(
        workspace_metric(atlas_pressure, "atlas_revision"),
        workspace_metric(atlas_pressure, "atlas_evictions"),
    );
    assert!(workspace_metric(atlas_pressure, "atlas_resident_glyphs") > 0.0);
    assert!(workspace_metric(atlas_pressure, "atlas_dirty_rects") > 0.0);
    assert!(workspace_metric(atlas_pressure, "atlas_dirty_pixels") > 0.0);
    assert!(workspace_metric(atlas_pressure, "atlas_max_dirty_pixels") > 0.0);
    assert!(workspace_metric(atlas_pressure, "atlas_pressure_vertices") > 0.0);
    assert!(workspace_metric(atlas_pressure, "atlas_pressure_indices") > 0.0);

    let dirty_upload = workspace_case(&report, "cpu.system.text_atlas_dirty_rect_upload");
    assert_workspace_cpu_row(dirty_upload, "system", "system");
    assert_eq!(workspace_metric(dirty_upload, "atlas_create_calls"), 1.0);
    assert!(workspace_metric(dirty_upload, "atlas_update_calls") >= 2.0);
    assert!(workspace_metric(dirty_upload, "dirty_upload_pixels") > 0.0);
    assert!(workspace_metric(dirty_upload, "max_dirty_update_pixels") > 0.0);
    assert!(workspace_metric(dirty_upload, "dirty_to_full_upload_ratio") < 0.01);

    let wrapped = workspace_case(&report, "cpu.system.wrapped_label_cached_encode");
    let wrapped_legacy = workspace_case(&report, "cpu.system.wrapped_label_legacy_fit_shape");
    assert_workspace_cpu_row(wrapped, "system", "system");
    assert_workspace_audit_row(wrapped_legacy);
    assert!(wrapped.median < wrapped_legacy.median);
    assert_eq!(workspace_metric(wrapped, "wrapped_label_variants"), 4096.0);
    assert_eq!(
        workspace_metric(wrapped, "wrapped_label_vertices"),
        workspace_metric(wrapped_legacy, "wrapped_label_vertices"),
    );
    assert!(
        workspace_metric(wrapped_legacy, "legacy_shape_calls")
            > workspace_metric(wrapped, "wrapped_label_glyph_runs")
    );
    assert!(workspace_metric(wrapped, "dirty_to_full_upload_ratio") < 0.01);

    let picker = workspace_case(&report, "cpu.system.picker_text_cached_encode");
    let picker_legacy = workspace_case(&report, "cpu.system.picker_text_legacy_shape_upload");
    assert_workspace_cpu_row(picker, "system", "system");
    assert_workspace_audit_row(picker_legacy);
    assert!(picker.median < picker_legacy.median);
    assert_eq!(workspace_metric(picker, "atlas_create_calls"), 1.0);
    assert_eq!(workspace_metric(picker, "atlas_update_calls"), 1.0);
    assert!(workspace_metric(picker, "dirty_to_full_upload_ratio") < 0.01);
    assert!(
        workspace_metric(picker_legacy, "atlas_update_pixels")
            >= workspace_metric(picker_legacy, "full_upload_pixels")
    );

    let cluster = workspace_case(&report, "cpu.text_input.cursor_pick.cluster_map");
    let rtl = workspace_case(&report, "cpu.text_input.cursor_pick.rtl_cluster_map");
    let fallback = workspace_case(&report, "cpu.text_input.cursor_pick.fallback_cluster_map");
    let mixed = workspace_case(&report, "cpu.text_input.cursor_pick.mixed_bidi_affinity");
    for case in [cluster, rtl, fallback, mixed] {
        assert_workspace_cpu_row(case, "text-input", "text-input");
        assert_eq!(workspace_metric(case, "cursor_pick_positions"), 6.0);
        assert!(workspace_metric(case, "text_bytes") > 0.0);
    }
    assert_text_cursor_map_workspace_metrics(cluster, "cursor_map");
    assert_text_cursor_map_workspace_metrics(rtl, "rtl_cursor_map");
    assert_text_cursor_map_workspace_metrics(fallback, "fallback_cursor_map");
    assert_text_cursor_map_workspace_metrics(mixed, "mixed_bidi_cursor_map");
    assert!(workspace_metric(cluster, "cursor_checksum") > 0.0);
    assert!(workspace_metric(rtl, "rtl_cursor_checksum") > 0.0);
    assert!(workspace_metric(fallback, "fallback_cursor_checksum") > 0.0);
    assert!(workspace_metric(fallback, "fallback_fonts") >= 1.0);
    assert!(workspace_metric(fallback, "fallback_shape_runs") >= 3.0);
    assert!(workspace_metric(mixed, "mixed_bidi_cursor_checksum") > 0.0);
    assert_eq!(workspace_metric(mixed, "mixed_bidi_boundary_positions"), 2.0);
    assert!(workspace_metric(mixed, "mixed_bidi_cursor_map_affinity_splits") >= 2.0);
    assert!(workspace_metric(mixed, "rtl_font_loaded") >= 1.0);
}

fn assert_text_cursor_map_workspace_metrics(case: &PerfCaseResult, prefix: &str) {
    let cursor_count = workspace_metric(case, &format!("{prefix}_cursor_count"));
    let byte_boundaries = workspace_metric(case, &format!("{prefix}_byte_boundaries"));
    assert!(cursor_count > 0.0);
    assert_eq!(byte_boundaries, cursor_count + 1.0);
    assert!(workspace_metric(case, &format!("{prefix}_boundary_checksum")) > 0.0);
    assert!(workspace_metric(case, &format!("{prefix}_width_span")) > 0.0);
}

fn web_report_case<'a>(report: &'a Value, id: &str) -> &'a Value {
    let cases = report["cases"].as_array().expect("web report cases array");
    cases
        .iter()
        .find(|case| case["id"].as_str() == Some(id))
        .unwrap_or_else(|| panic!("missing web report case {id}"))
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

fn assert_web_frame_case_contract(case: &Value) {
    assert_eq!(case["unit"].as_str(), Some("ms/frame"));
    assert_eq!(case["cache_state"].as_str(), Some("warm"));
    assert_eq!(case["refresh_mode"].as_str(), Some("browser-main-thread"));
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
    for hz in ["60hz", "120hz"] {
        assert!(web_report_number(case, &format!("frame_budget_{hz}_ms")) > 0.0);
        assert!(web_report_number(case, &format!("missed_frames_{hz}")) >= 0.0);
        assert!(web_report_number(case, &format!("hitch_frames_{hz}")) >= 0.0);
        let missed = web_report_number(case, &format!("missed_frame_ratio_{hz}"));
        let hitch = web_report_number(case, &format!("hitch_ratio_{hz}"));
        assert!((0.0..=1.0).contains(&missed), "missed ratio {hz} out of range: {missed}");
        assert!((0.0..=1.0).contains(&hitch), "hitch ratio {hz} out of range: {hitch}");
    }
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
fn web_latest_report_satisfies_webgpu_distribution_and_pacing_contract() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/web/latest.json");
    let bytes = std::fs::read(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    let report: Value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|err| panic!("parse {}: {err}", path.display()));

    assert_eq!(report["version"].as_u64(), Some(2));
    assert_eq!(report["suite"].as_str(), Some("web-wasm"));
    assert_eq!(report["status"].as_str(), Some("browser-baseline"));
    assert_eq!(report["smoke"]["webgpu"].as_str(), Some("webgpu=device-ok"));
    assert_eq!(report["smoke"]["backend"].as_str(), Some("webgpu"));
    assert!(report["smoke"]["webgpu_timing"]
        .as_str()
        .expect("webgpu timing smoke")
        .contains("gpu_stage_attribution="));
    assert_eq!(
        report["gpu_stage_attribution"]["source"].as_str(),
        Some("adapter.features+renderer.timestamp_writes"),
    );
    assert_eq!(
        report["gpu_stage_attribution"]["status"].as_str(),
        Some("timestamp-query-collected"),
    );
    assert!(web_report_number(&report["gpu_stage_attribution"], "collected_rows") > 0.0);
    assert!(web_report_number(&report["gpu_stage_attribution"], "collected_passes") > 0.0);
    assert_eq!(report["browser_trace"]["status"].as_str(), Some("collected"));
    assert_eq!(report["browser_trace"]["capture_phase"].as_str(), Some("benchmark-report"));
    assert_eq!(report["browser_trace"]["timing_source"].as_str(), Some("untraced-baseline-report"),);
    assert!(web_report_number(&report["browser_trace"], "events") > 0.0);
    assert!(web_report_number(&report["browser_trace"], "gpu_related_events") > 0.0);
    assert!(web_report_number(&report["browser_trace"], "duration_us") > 0.0);
    assert!(web_report_number(&report["browser_trace"], "category_count") > 0.0);
    assert!(!report["browser_trace"]["sample_categories"]
        .as_array()
        .expect("browser trace sample categories")
        .is_empty());
    assert_eq!(report["browser_trace"]["benchmark_trace_mark_status"].as_str(), Some("collected"),);
    assert!(web_report_number(&report["browser_trace"], "benchmark_trace_mark_count") > 0.0);
    assert!(report["browser_trace"]["benchmark_trace_mark_labels"].as_array().is_some());
    assert!(report["browser_trace"]["benchmark_trace_marks"].as_array().is_some());

    let expected_benchmark_marks = [
        "frame_loop",
        "id_mask_ab",
        "upload_ab",
        "upload_scratch_ab",
        "effect_uniform_ab",
        "backdrop_batch_ab",
        "scene3d_ab",
        "mixed_matrix",
        "layer_effects_matrix",
        "clean_layer_ab",
        "command_family_matrix",
        "glyph_run_ab",
        "neon_marker_ab",
        "direct_surface_ab",
        "draw_item_coalescing_ab",
        "draw_state_cache_ab",
        "clip_state_cache_ab",
    ];
    let benchmark_marks = &report["benchmark_marks"];
    assert_eq!(benchmark_marks["id"].as_str(), Some("web.wasm.webgpu.benchmark_mark_coverage"),);
    assert_eq!(
        web_report_number(benchmark_marks, "expected_count"),
        expected_benchmark_marks.len() as f64,
    );
    assert!(
        web_report_number(benchmark_marks, "page_mark_count")
            >= expected_benchmark_marks.len() as f64
    );
    let page_labels: Vec<&str> = benchmark_marks["page_labels"]
        .as_array()
        .expect("benchmark page labels")
        .iter()
        .map(|value| value.as_str().expect("benchmark page label"))
        .collect();
    for id in expected_benchmark_marks {
        assert!(page_labels.contains(&id), "benchmark marks missing page label {id}");
    }
    assert_eq!(
        web_report_number(benchmark_marks, "traced_mark_count"),
        expected_benchmark_marks.len() as f64,
    );
    assert_eq!(web_report_number(benchmark_marks, "wasm_memory_total_growth_bytes"), 0.0);
    assert_eq!(web_report_number(benchmark_marks, "wasm_memory_max_growth_bytes"), 0.0);
    assert!(benchmark_marks["wasm_memory_growth_labels"]
        .as_array()
        .expect("wasm memory growth labels")
        .is_empty());
    assert!(
        web_report_number(benchmark_marks, "js_heap_sample_supported_count")
            >= expected_benchmark_marks.len() as f64
    );
    assert!(
        web_report_number(benchmark_marks, "js_heap_gc_available_count")
            >= expected_benchmark_marks.len() as f64
    );
    assert!(web_report_number(benchmark_marks, "js_heap_total_growth_bytes") >= 0.0);
    assert!(web_report_number(benchmark_marks, "js_heap_max_growth_bytes") >= 0.0);
    assert!(benchmark_marks["js_heap_growth_labels"]
        .as_array()
        .expect("js heap growth labels")
        .iter()
        .all(|value| value.as_str().is_some()));
    let traced_labels: Vec<&str> = benchmark_marks["traced_labels"]
        .as_array()
        .expect("benchmark traced labels")
        .iter()
        .map(|value| value.as_str().expect("benchmark traced label"))
        .collect();
    let trace_labels: Vec<&str> = report["browser_trace"]["benchmark_trace_mark_labels"]
        .as_array()
        .expect("browser trace benchmark mark labels")
        .iter()
        .map(|value| value.as_str().expect("browser trace benchmark mark label"))
        .collect();
    assert_eq!(
        web_report_number(&report["browser_trace"], "benchmark_trace_interval_count"),
        expected_benchmark_marks.len() as f64,
    );
    let trace_interval_labels: Vec<&str> = report["browser_trace"]
        ["benchmark_trace_interval_labels"]
        .as_array()
        .expect("browser trace benchmark interval labels")
        .iter()
        .map(|value| value.as_str().expect("browser trace benchmark interval label"))
        .collect();
    for id in expected_benchmark_marks {
        assert!(traced_labels.contains(&id), "benchmark marks missing traced label {id}");
        assert!(trace_labels.contains(&id), "browser trace missing traced benchmark label {id}");
        assert!(
            trace_interval_labels.contains(&id),
            "browser trace missing benchmark interval {id}"
        );
    }
    let trace_intervals = report["browser_trace"]["benchmark_trace_intervals"]
        .as_array()
        .expect("browser trace benchmark intervals");
    assert_eq!(trace_intervals.len(), expected_benchmark_marks.len());
    for interval in trace_intervals {
        let id = interval["id"].as_str().expect("browser trace benchmark interval id");
        assert!(
            expected_benchmark_marks.contains(&id),
            "unexpected browser trace benchmark interval {id}"
        );
        assert!(web_report_number(interval, "duration_us") > 0.0);
        assert!(web_report_number(interval, "event_count") > 0.0);
        assert!(web_report_number(interval, "gpu_related_events") > 0.0);
        assert!(web_report_number(interval, "webgpu_related_events") > 0.0);
        assert!(web_report_number(interval, "event_duration_us") > 0.0);
        assert!(web_report_number(interval, "angle_related_events") >= 0.0);
        assert!(web_report_number(interval, "renderer_related_events") >= 0.0);
    }
    for mark in benchmark_marks["marks"].as_array().expect("benchmark marks") {
        assert!(web_report_number(mark, "start_ms") >= 0.0);
        assert!(web_report_number(mark, "duration_ms") > 0.0);
        assert!(web_report_number(mark, "wasm_memory_before_bytes") > 0.0);
        assert!(
            web_report_number(mark, "wasm_memory_after_bytes")
                >= web_report_number(mark, "wasm_memory_before_bytes")
        );
        assert_eq!(
            web_report_number(mark, "wasm_memory_after_bytes"),
            web_report_number(mark, "wasm_memory_before_bytes"),
        );
        assert_eq!(web_report_number(mark, "wasm_memory_growth_bytes"), 0.0);
        assert!(web_report_number(mark, "js_heap_sample_supported") > 0.0);
        assert!(web_report_number(mark, "js_heap_gc_available") > 0.0);
        assert!(web_report_number(mark, "js_heap_before_bytes") > 0.0);
        assert!(web_report_number(mark, "js_heap_after_bytes") > 0.0);
        assert!(web_report_number(mark, "js_heap_growth_bytes") >= 0.0);
    }

    let expected_ids = [
        "web.wasm.webgpu.frame_loop",
        "web.wasm.webgpu.id_mask_compositor.current",
        "web.wasm.webgpu.id_mask_compositor.legacy_upload",
        "web.wasm.webgpu.glyph_atlas_upload.current_dirty",
        "web.wasm.webgpu.glyph_atlas_upload.legacy_full",
        "web.wasm.webgpu.image_upload.current_dirty",
        "web.wasm.webgpu.image_upload.legacy_full",
        "web.wasm.webgpu.upload_scratch.current_reuse",
        "web.wasm.webgpu.upload_scratch.legacy_temp_alloc",
        "web.wasm.webgpu.effect_uniform.current_batched",
        "web.wasm.webgpu.effect_uniform.legacy_write_each",
        "web.wasm.webgpu.backdrop_batch.current_coalesced",
        "web.wasm.webgpu.backdrop_batch.legacy_per_backdrop_copy",
        "web.wasm.webgpu.scene3d.reused_mesh",
        "web.wasm.webgpu.scene3d.recreate_mesh",
        "web.wasm.webgpu.scene3d.stress_reused_mesh",
        "web.wasm.webgpu.scene3d.stress_recreate_mesh",
        "web.wasm.webgpu.mixed_text_image_effects",
        "web.wasm.webgpu.mixed_text_image_effects.legacy_rebind_unbatched",
        "web.wasm.webgpu.layer_damage_effects",
        "web.wasm.webgpu.layer_damage_effects.legacy_rebind_unbatched",
        "web.wasm.webgpu.clean_layer.clean_reuse",
        "web.wasm.webgpu.clean_layer.dirty_rerender",
        "web.wasm.webgpu.command_family_matrix",
        "web.wasm.webgpu.command_family_matrix.legacy_rebind",
        "web.wasm.webgpu.glyph_run.current",
        "web.wasm.webgpu.glyph_run.legacy_rebind",
        "web.wasm.webgpu.neon_marker.current",
        "web.wasm.webgpu.neon_marker.legacy_rebind",
        "web.wasm.webgpu.direct_surface.current",
        "web.wasm.webgpu.direct_surface.legacy_scene_present",
        "web.wasm.webgpu.draw_item_coalescing.current",
        "web.wasm.webgpu.draw_item_coalescing.legacy_uncoalesced",
        "web.wasm.webgpu.draw_state_cache.current",
        "web.wasm.webgpu.draw_state_cache.legacy_rebind",
        "web.wasm.webgpu.clip_state_cache.current",
        "web.wasm.webgpu.clip_state_cache.legacy_rebind",
    ];
    assert_eq!(
        report["cases"].as_array().expect("web cases").len(),
        expected_ids.len(),
        "unexpected WebGPU browser report case count",
    );
    for id in expected_ids {
        assert_web_frame_case_contract(web_report_case(&report, id));
    }

    let gpu_timestamp_stage_breakdown = &report["gpu_timestamp_stage_breakdown"];
    assert_eq!(
        gpu_timestamp_stage_breakdown["id"].as_str(),
        Some("web.wasm.webgpu.gpu_timestamp_stage_breakdown"),
    );
    assert_eq!(web_report_number(gpu_timestamp_stage_breakdown, "row_count"), 37.0);
    assert_eq!(web_report_number(gpu_timestamp_stage_breakdown, "collected_rows"), 37.0);
    assert_eq!(web_report_number(gpu_timestamp_stage_breakdown, "stage_count"), 9.0);
    assert_eq!(web_report_number(gpu_timestamp_stage_breakdown, "row_detail_count"), 37.0);
    assert_eq!(
        web_report_number(gpu_timestamp_stage_breakdown, "total_render_passes"),
        web_report_number(gpu_timestamp_stage_breakdown, "total_timestamp_passes"),
    );
    assert_eq!(
        web_report_number(gpu_timestamp_stage_breakdown, "total_render_passes"),
        web_report_number(gpu_timestamp_stage_breakdown, "total_family_passes"),
    );
    assert_eq!(
        web_report_number(gpu_timestamp_stage_breakdown, "total_timestamp_ns"),
        web_report_number(gpu_timestamp_stage_breakdown, "total_family_timestamp_ns"),
    );
    assert_eq!(
        web_report_number(gpu_timestamp_stage_breakdown, "total_timestamp_passes"),
        web_report_number(&report["gpu_stage_attribution"], "collected_passes"),
    );
    assert_eq!(
        web_report_number(gpu_timestamp_stage_breakdown, "total_timestamp_ns"),
        web_report_number(&report["gpu_stage_attribution"], "total_ns"),
    );
    let gpu_timestamp_stages: Vec<&str> = gpu_timestamp_stage_breakdown["stages"]
        .as_array()
        .expect("gpu timestamp stage details")
        .iter()
        .map(|stage| stage["stage"].as_str().expect("gpu timestamp stage name"))
        .collect();
    for stage in ["draw", "scene3d", "id_mask_field_jump", "present"] {
        assert!(gpu_timestamp_stages.contains(&stage), "missing gpu timestamp stage {stage}");
    }
    let gpu_timestamp_rows =
        gpu_timestamp_stage_breakdown["row_details"].as_array().expect("gpu timestamp row details");
    let frame_loop_timestamp_row = gpu_timestamp_rows
        .iter()
        .find(|row| row["id"].as_str() == Some("web.wasm.webgpu.frame_loop"))
        .expect("frame-loop gpu timestamp detail");
    assert_eq!(
        web_report_number(frame_loop_timestamp_row, "family_passes"),
        web_report_number(web_report_case(&report, "web.wasm.webgpu.frame_loop"), "render_passes"),
    );
    assert_eq!(
        web_report_number(frame_loop_timestamp_row, "family_timestamp_ns"),
        web_report_number(
            web_report_case(&report, "web.wasm.webgpu.frame_loop"),
            "gpu_timestamp_total_ns"
        ),
    );

    let warm_resource_churn = &report["warm_resource_churn"];
    assert_eq!(
        warm_resource_churn["id"].as_str(),
        Some("web.wasm.webgpu.warm_resource_churn.current_rows"),
    );
    assert_eq!(web_report_number(warm_resource_churn, "checked_rows"), 19.0);
    assert_eq!(web_report_number(warm_resource_churn, "excluded_rows"), 18.0);
    let warm_rows: Vec<&str> = warm_resource_churn["rows"]
        .as_array()
        .expect("warm resource churn rows")
        .iter()
        .map(|value| value.as_str().expect("warm resource churn row id"))
        .collect();
    let warm_excluded: Vec<&str> = warm_resource_churn["excluded"]
        .as_array()
        .expect("warm resource churn excluded rows")
        .iter()
        .map(|value| value.as_str().expect("warm resource churn excluded row id"))
        .collect();
    let warm_row_details: Vec<&Value> = warm_resource_churn["row_details"]
        .as_array()
        .expect("warm resource churn row details")
        .iter()
        .collect();
    assert_eq!(web_report_number(warm_resource_churn, "row_detail_count"), warm_rows.len() as f64,);
    assert_eq!(warm_row_details.len(), warm_rows.len());
    for id in [
        "web.wasm.webgpu.frame_loop",
        "web.wasm.webgpu.id_mask_compositor.current",
        "web.wasm.webgpu.glyph_atlas_upload.current_dirty",
        "web.wasm.webgpu.image_upload.current_dirty",
        "web.wasm.webgpu.upload_scratch.current_reuse",
        "web.wasm.webgpu.effect_uniform.current_batched",
        "web.wasm.webgpu.backdrop_batch.current_coalesced",
        "web.wasm.webgpu.scene3d.reused_mesh",
        "web.wasm.webgpu.scene3d.stress_reused_mesh",
        "web.wasm.webgpu.mixed_text_image_effects",
        "web.wasm.webgpu.layer_damage_effects",
        "web.wasm.webgpu.clean_layer.clean_reuse",
        "web.wasm.webgpu.command_family_matrix",
        "web.wasm.webgpu.neon_marker.current",
        "web.wasm.webgpu.direct_surface.current",
        "web.wasm.webgpu.draw_item_coalescing.current",
        "web.wasm.webgpu.draw_state_cache.current",
        "web.wasm.webgpu.clip_state_cache.current",
    ] {
        assert!(warm_rows.contains(&id), "warm resource churn missing checked row {id}");
    }
    for id in [
        "web.wasm.webgpu.id_mask_compositor.legacy_upload",
        "web.wasm.webgpu.glyph_atlas_upload.legacy_full",
        "web.wasm.webgpu.image_upload.legacy_full",
        "web.wasm.webgpu.upload_scratch.legacy_temp_alloc",
        "web.wasm.webgpu.effect_uniform.legacy_write_each",
        "web.wasm.webgpu.backdrop_batch.legacy_per_backdrop_copy",
        "web.wasm.webgpu.scene3d.recreate_mesh",
        "web.wasm.webgpu.scene3d.stress_recreate_mesh",
        "web.wasm.webgpu.mixed_text_image_effects.legacy_rebind_unbatched",
        "web.wasm.webgpu.layer_damage_effects.legacy_rebind_unbatched",
        "web.wasm.webgpu.clean_layer.dirty_rerender",
        "web.wasm.webgpu.command_family_matrix.legacy_rebind",
        "web.wasm.webgpu.neon_marker.legacy_rebind",
        "web.wasm.webgpu.direct_surface.legacy_scene_present",
        "web.wasm.webgpu.draw_item_coalescing.legacy_uncoalesced",
        "web.wasm.webgpu.draw_state_cache.legacy_rebind",
        "web.wasm.webgpu.clip_state_cache.legacy_rebind",
    ] {
        assert!(warm_excluded.contains(&id), "warm resource churn missing excluded row {id}");
    }
    for detail in &warm_row_details {
        let id = detail["id"].as_str().expect("warm resource churn row detail id");
        assert!(warm_rows.contains(&id), "warm resource churn has unexpected row detail {id}");
    }
    for field in [
        "buffer_grows",
        "texture_creates",
        "bind_group_creates",
        "pipeline_creates",
        "sampler_creates",
        "mesh3d_creates",
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
        "image_upload_scratch_grows",
        "cpu_scratch_grows",
        "cpu_scratch_growth_bytes",
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
        assert_eq!(web_report_number(warm_resource_churn, &format!("total_{field}")), 0.0);
        for detail in &warm_row_details {
            let id = detail["id"].as_str().expect("warm resource churn row detail id");
            let source = web_report_case(&report, id);
            assert_eq!(
                web_report_number(detail, field),
                web_report_number(source, field),
                "warm resource churn row detail mismatch for {id}.{field}",
            );
            assert_eq!(
                web_report_number(detail, field),
                0.0,
                "warm resource churn row {id}.{field} was nonzero",
            );
        }
    }

    let wasm_allocation_audit = &report["wasm_allocation_audit"];
    assert_eq!(
        wasm_allocation_audit["id"].as_str(),
        Some("web.wasm.webgpu.wasm_allocation_audit.current_rows"),
    );
    assert_eq!(wasm_allocation_audit["status"].as_str(), Some("measured"));
    assert_eq!(web_report_number(wasm_allocation_audit, "checked_count"), 19.0);
    assert_eq!(web_report_number(wasm_allocation_audit, "excluded_count"), 18.0);
    assert_eq!(
        web_report_number(wasm_allocation_audit, "row_detail_count"),
        web_report_number(wasm_allocation_audit, "checked_count"),
    );
    assert!(web_report_number(wasm_allocation_audit, "total_wasm_alloc_count") > 0.0);
    assert!(web_report_number(wasm_allocation_audit, "total_wasm_alloc_bytes") > 0.0);
    assert_eq!(web_report_number(wasm_allocation_audit, "total_wasm_realloc_count"), 0.0);
    assert_eq!(web_report_number(wasm_allocation_audit, "total_wasm_realloc_grow_bytes"), 0.0,);
    assert!(web_report_number(wasm_allocation_audit, "budget_wasm_allocs_per_frame") <= 7.0);
    assert!(web_report_number(wasm_allocation_audit, "budget_wasm_alloc_bytes_per_frame") <= 144.0);
    assert!(
        web_report_number(wasm_allocation_audit, "max_wasm_allocs_per_frame")
            <= web_report_number(wasm_allocation_audit, "budget_wasm_allocs_per_frame")
    );
    assert!(
        web_report_number(wasm_allocation_audit, "max_wasm_alloc_bytes_per_frame")
            <= web_report_number(wasm_allocation_audit, "budget_wasm_alloc_bytes_per_frame")
    );
    let wasm_allocation_rows: Vec<&str> = wasm_allocation_audit["rows"]
        .as_array()
        .expect("wasm allocation rows")
        .iter()
        .map(|value| value.as_str().expect("wasm allocation row id"))
        .collect();
    let wasm_allocation_details =
        wasm_allocation_audit["row_details"].as_array().expect("wasm allocation row details");
    assert!(wasm_allocation_rows.contains(&"web.wasm.webgpu.frame_loop"));
    assert!(wasm_allocation_rows.contains(&"web.wasm.webgpu.id_mask_compositor.current"));
    assert!(wasm_allocation_rows.contains(&"web.wasm.webgpu.glyph_run.current"));
    assert!(wasm_allocation_rows.contains(&"web.wasm.webgpu.neon_marker.current"));
    assert!(wasm_allocation_rows.contains(&"web.wasm.webgpu.direct_surface.current"));
    assert!(wasm_allocation_rows.contains(&"web.wasm.webgpu.draw_item_coalescing.current"));
    for detail in wasm_allocation_details {
        let id = detail["id"].as_str().expect("wasm allocation row detail id");
        assert!(wasm_allocation_rows.contains(&id));
        let source = web_report_case(&report, id);
        assert_eq!(
            web_report_number(detail, "wasm_alloc_count"),
            web_report_number(source, "wasm_alloc_count"),
        );
        assert_eq!(
            web_report_number(detail, "wasm_alloc_bytes"),
            web_report_number(source, "wasm_alloc_bytes"),
        );
        assert_eq!(web_report_number(detail, "wasm_realloc_count"), 0.0);
        assert_eq!(web_report_number(detail, "wasm_realloc_grow_bytes"), 0.0);
        assert!(
            web_report_number(detail, "wasm_allocs_per_frame")
                <= web_report_number(wasm_allocation_audit, "budget_wasm_allocs_per_frame")
        );
        assert!(
            web_report_number(detail, "wasm_alloc_bytes_per_frame")
                <= web_report_number(wasm_allocation_audit, "budget_wasm_alloc_bytes_per_frame")
        );
    }

    let frame_loop = web_report_case(&report, "web.wasm.webgpu.frame_loop");
    let wasm_allocation_invariance = &report["wasm_allocation_invariance"];
    assert_eq!(
        wasm_allocation_invariance["id"].as_str(),
        Some("web.wasm.webgpu.wasm_allocation_invariance.current_rows"),
    );
    assert_eq!(
        wasm_allocation_invariance["status"].as_str(),
        Some("shared-submit-boundary-profile"),
    );
    assert_eq!(
        wasm_allocation_invariance["reference_row"].as_str(),
        Some("web.wasm.webgpu.frame_loop"),
    );
    assert_eq!(
        web_report_number(wasm_allocation_invariance, "checked_count"),
        web_report_number(wasm_allocation_audit, "checked_count"),
    );
    assert_eq!(web_report_number(wasm_allocation_invariance, "unique_signature_count"), 1.0,);
    assert_eq!(
        web_report_number(wasm_allocation_invariance, "shared_wasm_alloc_count"),
        web_report_number(frame_loop, "wasm_alloc_count"),
    );
    assert_eq!(
        web_report_number(wasm_allocation_invariance, "shared_wasm_alloc_bytes"),
        web_report_number(frame_loop, "wasm_alloc_bytes"),
    );
    assert_eq!(web_report_number(wasm_allocation_invariance, "shared_wasm_realloc_count"), 0.0,);
    assert_eq!(
        web_report_number(wasm_allocation_invariance, "shared_wasm_realloc_grow_bytes"),
        0.0,
    );
    assert_eq!(
        wasm_allocation_invariance["signature_rows"]
            .as_array()
            .expect("wasm allocation invariance signature rows")
            .len(),
        1,
    );

    let frame_stage_allocations = &report["frame_loop_wasm_allocation_stages"];
    assert_eq!(
        frame_stage_allocations["id"].as_str(),
        Some("web.wasm.webgpu.frame_loop_wasm_allocation_stages"),
    );
    assert_eq!(frame_stage_allocations["row_id"].as_str(), Some("web.wasm.webgpu.frame_loop"),);
    assert_eq!(web_report_number(frame_stage_allocations, "stage_count"), 11.0);
    assert_eq!(
        web_report_number(frame_stage_allocations, "total_stage_wasm_alloc_count"),
        web_report_number(frame_loop, "wasm_alloc_count"),
    );
    assert_eq!(
        web_report_number(frame_stage_allocations, "total_stage_wasm_alloc_bytes"),
        web_report_number(frame_loop, "wasm_alloc_bytes"),
    );
    assert_eq!(web_report_number(frame_stage_allocations, "total_stage_wasm_realloc_count"), 0.0,);
    assert_eq!(
        web_report_number(frame_stage_allocations, "total_stage_wasm_realloc_grow_bytes"),
        0.0,
    );
    let stage_details = frame_stage_allocations["stages"]
        .as_array()
        .expect("frame-loop wasm allocation stage details");
    let stage_names: Vec<&str> =
        stage_details.iter().map(|stage| stage["stage"].as_str().expect("stage name")).collect();
    for name in [
        "canvas_resize",
        "frame_timing",
        "builder_clear",
        "router_update",
        "router_draw",
        "damage_handoff",
        "draw_coalesce",
        "begin_frame",
        "encode_pass",
        "submit",
        "post_submit",
    ] {
        assert!(stage_names.contains(&name), "missing frame-loop allocation stage {name}");
    }
    for stage in stage_details {
        assert!(web_report_number(stage, "wasm_alloc_count") >= 0.0);
        assert!(web_report_number(stage, "wasm_alloc_bytes") >= 0.0);
        assert_eq!(web_report_number(stage, "wasm_realloc_count"), 0.0);
        assert_eq!(web_report_number(stage, "wasm_realloc_grow_bytes"), 0.0);
    }

    let submit_stage_allocations = &report["frame_loop_wasm_submit_allocation_stages"];
    assert_eq!(
        submit_stage_allocations["id"].as_str(),
        Some("web.wasm.webgpu.frame_loop_wasm_submit_allocation_stages"),
    );
    assert_eq!(submit_stage_allocations["row_id"].as_str(), Some("web.wasm.webgpu.frame_loop"),);
    assert_eq!(web_report_number(submit_stage_allocations, "stage_count"), 9.0);
    assert_eq!(
        web_report_number(submit_stage_allocations, "total_stage_wasm_alloc_count"),
        web_report_number(frame_loop, "submit_total_alloc_count"),
    );
    assert_eq!(
        web_report_number(submit_stage_allocations, "total_stage_wasm_alloc_bytes"),
        web_report_number(frame_loop, "submit_total_alloc_bytes"),
    );
    assert_eq!(
        web_report_number(submit_stage_allocations, "frame_stage_submit_wasm_alloc_count"),
        web_report_number(frame_loop, "wasm_stage_submit_alloc_count"),
    );
    assert_eq!(
        web_report_number(frame_loop, "submit_total_alloc_count"),
        web_report_number(frame_loop, "wasm_stage_submit_alloc_count"),
    );
    assert_eq!(
        web_report_number(frame_loop, "submit_total_alloc_bytes"),
        web_report_number(frame_loop, "wasm_stage_submit_alloc_bytes"),
    );
    assert_eq!(web_report_number(frame_loop, "submit_total_realloc_count"), 0.0);
    assert_eq!(web_report_number(frame_loop, "submit_total_realloc_grow_bytes"), 0.0);
    for field in [
        "submit_upload_alloc_count",
        "submit_encoder_alloc_count",
        "submit_render_alloc_count",
        "submit_timestamp_alloc_count",
        "submit_scratch_stats_alloc_count",
        "submit_present_alloc_count",
    ] {
        assert_eq!(web_report_number(frame_loop, field), 0.0);
    }
    assert!(web_report_number(frame_loop, "submit_surface_alloc_count") > 0.0);
    assert!(web_report_number(frame_loop, "submit_finish_queue_alloc_count") > 0.0);
    assert!(web_report_number(frame_loop, "submit_timestamp_map_alloc_count") > 0.0);
    assert_eq!(submit_stage_allocations["dominant_stage"].as_str(), Some("surface"));
    let submit_stage_details = submit_stage_allocations["stages"]
        .as_array()
        .expect("frame-loop wasm submit allocation stage details");
    let submit_stage_names: Vec<&str> = submit_stage_details
        .iter()
        .map(|stage| stage["stage"].as_str().expect("submit stage name"))
        .collect();
    for name in [
        "upload",
        "surface",
        "encoder",
        "render",
        "timestamp",
        "scratch_stats",
        "finish_queue",
        "present",
        "timestamp_map",
    ] {
        assert!(
            submit_stage_names.contains(&name),
            "missing frame-loop submit allocation stage {name}"
        );
    }

    let backend_path_coverage = &report["backend_path_coverage"];
    assert_eq!(backend_path_coverage["id"].as_str(), Some("web.wasm.webgpu.backend_path_coverage"),);
    let expected_backend_paths: &[(&str, &[&str], &[&str])] = &[
        (
            "frame_loop",
            &["web.wasm.webgpu.frame_loop"],
            &[
                "draws",
                "draw_items",
                "draw_passes",
                "command_buffers",
                "buffer_upload_bytes",
                "gpu_timestamp_passes",
            ],
        ),
        (
            "id_mask_compositor",
            &[
                "web.wasm.webgpu.id_mask_compositor.current",
                "web.wasm.webgpu.id_mask_compositor.legacy_upload",
            ],
            &[
                "id_mask_draws",
                "id_mask_raster_passes",
                "id_mask_field_seed_passes",
                "id_mask_field_jump_passes",
                "id_mask_compositor_passes",
                "buffer_upload_bytes",
                "vertices",
                "gpu_timestamp_passes",
            ],
        ),
        (
            "glyph_atlas_upload",
            &[
                "web.wasm.webgpu.glyph_atlas_upload.current_dirty",
                "web.wasm.webgpu.glyph_atlas_upload.legacy_full",
            ],
            &["glyph_quads", "texture_upload_bytes", "buffer_upload_bytes", "gpu_timestamp_passes"],
        ),
        (
            "image_upload",
            &[
                "web.wasm.webgpu.image_upload.current_dirty",
                "web.wasm.webgpu.image_upload.legacy_full",
            ],
            &["image_draws", "texture_upload_bytes", "buffer_upload_bytes", "gpu_timestamp_passes"],
        ),
        (
            "upload_scratch",
            &[
                "web.wasm.webgpu.upload_scratch.current_reuse",
                "web.wasm.webgpu.upload_scratch.legacy_temp_alloc",
            ],
            &[
                "image_upload_temp_allocs",
                "image_upload_temp_bytes",
                "image_upload_scratch_bytes",
                "texture_upload_bytes",
                "gpu_timestamp_passes",
            ],
        ),
        (
            "effect_uniform",
            &[
                "web.wasm.webgpu.effect_uniform.current_batched",
                "web.wasm.webgpu.effect_uniform.legacy_write_each",
            ],
            &[
                "backdrop_draws",
                "visual_effect_draws",
                "effect_uniform_writes",
                "effect_uniform_slots",
                "texture_copies",
                "render_passes",
                "gpu_timestamp_total_ns",
            ],
        ),
        (
            "backdrop_batch",
            &[
                "web.wasm.webgpu.backdrop_batch.current_coalesced",
                "web.wasm.webgpu.backdrop_batch.legacy_per_backdrop_copy",
            ],
            &[
                "backdrop_draws",
                "effect_uniform_slots",
                "texture_copies",
                "render_passes",
                "gpu_timestamp_passes",
            ],
        ),
        (
            "scene3d_mesh_reuse",
            &["web.wasm.webgpu.scene3d.reused_mesh", "web.wasm.webgpu.scene3d.recreate_mesh"],
            &[
                "scene3d_draws",
                "mesh3d_creates",
                "buffer_grows",
                "cpu_scratch_grows",
                "gpu_timestamp_passes",
            ],
        ),
        (
            "scene3d_stress_mesh_reuse",
            &[
                "web.wasm.webgpu.scene3d.stress_reused_mesh",
                "web.wasm.webgpu.scene3d.stress_recreate_mesh",
            ],
            &[
                "scene3d_draws",
                "mesh3d_creates",
                "buffer_grows",
                "cpu_scratch_grows",
                "gpu_timestamp_passes",
            ],
        ),
        (
            "mixed_text_image_effects",
            &[
                "web.wasm.webgpu.mixed_text_image_effects",
                "web.wasm.webgpu.mixed_text_image_effects.legacy_rebind_unbatched",
            ],
            &[
                "glyph_quads",
                "image_draws",
                "image_tiles",
                "backdrop_draws",
                "visual_effect_draws",
                "spinner_draws",
                "layer_draws",
                "damage_rects",
                "draw_pipeline_binds",
                "draw_bind_group_binds",
                "draw_scissor_sets",
                "effect_uniform_writes",
                "texture_copies",
                "render_passes",
                "gpu_timestamp_passes",
            ],
        ),
        (
            "layer_damage_effects",
            &[
                "web.wasm.webgpu.layer_damage_effects",
                "web.wasm.webgpu.layer_damage_effects.legacy_rebind_unbatched",
            ],
            &[
                "glyph_quads",
                "image_draws",
                "image_tiles",
                "layer_draws",
                "damage_rects",
                "clip_depth_peak",
                "backdrop_draws",
                "visual_effect_draws",
                "spinner_draws",
                "draw_pipeline_binds",
                "draw_bind_group_binds",
                "draw_scissor_sets",
                "effect_uniform_writes",
                "texture_copies",
                "render_passes",
                "gpu_timestamp_passes",
            ],
        ),
        (
            "clean_layer_reuse",
            &[
                "web.wasm.webgpu.clean_layer.clean_reuse",
                "web.wasm.webgpu.clean_layer.dirty_rerender",
            ],
            &[
                "layer_draws",
                "layer_cache_hits",
                "layer_cache_misses",
                "layer_cache_skipped_draws",
                "layer_passes",
                "draw_items",
                "draw_passes",
                "gpu_timestamp_passes",
            ],
        ),
        (
            "command_family_matrix",
            &[
                "web.wasm.webgpu.command_family_matrix",
                "web.wasm.webgpu.command_family_matrix.legacy_rebind",
            ],
            &[
                "image_mesh_draws",
                "nine_slice_draws",
                "sdf_glyph_quads",
                "camera_bg_draws",
                "expected_camera_bg",
                "draw_items",
                "draw_pipeline_binds",
                "draw_bind_group_binds",
                "draw_scissor_sets",
                "gpu_timestamp_passes",
            ],
        ),
        (
            "glyph_run",
            &["web.wasm.webgpu.glyph_run.current", "web.wasm.webgpu.glyph_run.legacy_rebind"],
            &[
                "expected_glyph_runs",
                "expected_glyph_quads",
                "expected_sdf_glyph_quads",
                "expected_draw_items",
                "draw_items",
                "glyph_quads",
                "sdf_glyph_quads",
                "draw_pipeline_binds",
                "draw_bind_group_binds",
                "draw_scissor_sets",
                "render_passes",
                "gpu_timestamp_passes",
            ],
        ),
        (
            "neon_marker",
            &["web.wasm.webgpu.neon_marker.current", "web.wasm.webgpu.neon_marker.legacy_rebind"],
            &[
                "expected_markers",
                "expected_draw_items",
                "draw_items",
                "solid_tris",
                "draw_pipeline_binds",
                "draw_bind_group_binds",
                "draw_scissor_sets",
                "gpu_timestamp_passes",
            ],
        ),
        (
            "direct_surface",
            &[
                "web.wasm.webgpu.direct_surface.current",
                "web.wasm.webgpu.direct_surface.legacy_scene_present",
            ],
            &[
                "expected_draw_items",
                "expected_image_draws",
                "draw_items",
                "image_draws",
                "render_passes",
                "draw_passes",
                "clear_passes",
                "present_passes",
                "texture_copies",
                "gpu_timestamp_passes",
            ],
        ),
        (
            "draw_item_coalescing",
            &[
                "web.wasm.webgpu.draw_item_coalescing.current",
                "web.wasm.webgpu.draw_item_coalescing.legacy_uncoalesced",
            ],
            &[
                "expected_source_draw_items",
                "expected_current_draw_items",
                "draw_items",
                "draw_items_coalesced",
                "draws",
                "draw_pipeline_binds",
                "draw_bind_group_binds",
                "draw_scissor_sets",
                "gpu_timestamp_passes",
            ],
        ),
        (
            "draw_state_cache",
            &[
                "web.wasm.webgpu.draw_state_cache.current",
                "web.wasm.webgpu.draw_state_cache.legacy_rebind",
            ],
            &[
                "draw_items",
                "draw_pipeline_binds",
                "draw_bind_group_binds",
                "draw_scissor_sets",
                "gpu_timestamp_passes",
            ],
        ),
        (
            "clip_state_cache",
            &[
                "web.wasm.webgpu.clip_state_cache.current",
                "web.wasm.webgpu.clip_state_cache.legacy_rebind",
            ],
            &[
                "clip_depth_peak",
                "draw_scissor_sets",
                "draw_pipeline_binds",
                "draw_bind_group_binds",
                "gpu_timestamp_passes",
            ],
        ),
    ];
    assert_eq!(
        web_report_number(backend_path_coverage, "expected_path_count"),
        expected_backend_paths.len() as f64,
    );
    assert_eq!(
        web_report_number(backend_path_coverage, "covered_path_count"),
        expected_backend_paths.len() as f64,
    );
    assert_eq!(web_report_number(backend_path_coverage, "missing_path_count"), 0.0);
    let backend_paths =
        backend_path_coverage["paths"].as_array().expect("backend path coverage paths");
    assert_eq!(backend_paths.len(), expected_backend_paths.len());
    for (path_id, row_ids, counters) in expected_backend_paths {
        let path = backend_paths
            .iter()
            .find(|value| value["id"].as_str() == Some(*path_id))
            .unwrap_or_else(|| panic!("missing backend path coverage {path_id}"));
        assert_eq!(path["status"].as_str(), Some("covered"));
        assert_eq!(web_report_number(path, "row_count"), row_ids.len() as f64);
        assert_eq!(web_report_number(path, "counter_count"), counters.len() as f64);
        assert!(path["missing_rows"].as_array().expect("backend missing rows").is_empty());
        assert!(path["missing_counters"].as_array().expect("backend missing counters").is_empty());
        let detail_rows = path["row_details"].as_array().expect("backend row details");
        assert_eq!(detail_rows.len(), row_ids.len());
        for row_id in *row_ids {
            let source = web_report_case(&report, row_id);
            let detail = detail_rows
                .iter()
                .find(|value| value["id"].as_str() == Some(*row_id))
                .unwrap_or_else(|| panic!("missing backend path row detail {path_id}.{row_id}"));
            for field in ["p50_ms", "p95_ms", "p99_ms", "peak_ms"] {
                assert_eq!(web_report_number(detail, field), web_report_number(source, field));
            }
            for field in *counters {
                assert_eq!(
                    web_report_number(&detail["counters"], field),
                    web_report_number(source, field),
                    "backend path counter mismatch {path_id}.{row_id}.{field}",
                );
            }
        }
    }

    let frame = web_report_case(&report, "web.wasm.webgpu.frame_loop");
    let current = web_report_case(&report, "web.wasm.webgpu.id_mask_compositor.current");
    let legacy = web_report_case(&report, "web.wasm.webgpu.id_mask_compositor.legacy_upload");
    let glyph_current =
        web_report_case(&report, "web.wasm.webgpu.glyph_atlas_upload.current_dirty");
    let glyph_legacy = web_report_case(&report, "web.wasm.webgpu.glyph_atlas_upload.legacy_full");
    let image_current = web_report_case(&report, "web.wasm.webgpu.image_upload.current_dirty");
    let image_legacy = web_report_case(&report, "web.wasm.webgpu.image_upload.legacy_full");
    let upload_scratch_current =
        web_report_case(&report, "web.wasm.webgpu.upload_scratch.current_reuse");
    let upload_scratch_legacy =
        web_report_case(&report, "web.wasm.webgpu.upload_scratch.legacy_temp_alloc");
    let effect_current = web_report_case(&report, "web.wasm.webgpu.effect_uniform.current_batched");
    let effect_legacy =
        web_report_case(&report, "web.wasm.webgpu.effect_uniform.legacy_write_each");
    let backdrop_batch_current =
        web_report_case(&report, "web.wasm.webgpu.backdrop_batch.current_coalesced");
    let backdrop_batch_legacy =
        web_report_case(&report, "web.wasm.webgpu.backdrop_batch.legacy_per_backdrop_copy");
    let scene3d_reused = web_report_case(&report, "web.wasm.webgpu.scene3d.reused_mesh");
    let scene3d_recreate = web_report_case(&report, "web.wasm.webgpu.scene3d.recreate_mesh");
    let scene3d_stress_reused =
        web_report_case(&report, "web.wasm.webgpu.scene3d.stress_reused_mesh");
    let scene3d_stress_recreate =
        web_report_case(&report, "web.wasm.webgpu.scene3d.stress_recreate_mesh");
    let mixed = web_report_case(&report, "web.wasm.webgpu.mixed_text_image_effects");
    let mixed_legacy = web_report_case(
        &report,
        "web.wasm.webgpu.mixed_text_image_effects.legacy_rebind_unbatched",
    );
    let layer_effects = web_report_case(&report, "web.wasm.webgpu.layer_damage_effects");
    let layer_effects_legacy =
        web_report_case(&report, "web.wasm.webgpu.layer_damage_effects.legacy_rebind_unbatched");
    let clean_layer = web_report_case(&report, "web.wasm.webgpu.clean_layer.clean_reuse");
    let dirty_layer = web_report_case(&report, "web.wasm.webgpu.clean_layer.dirty_rerender");
    let command_family = web_report_case(&report, "web.wasm.webgpu.command_family_matrix");
    let command_family_legacy =
        web_report_case(&report, "web.wasm.webgpu.command_family_matrix.legacy_rebind");
    let glyph_run_current = web_report_case(&report, "web.wasm.webgpu.glyph_run.current");
    let glyph_run_legacy = web_report_case(&report, "web.wasm.webgpu.glyph_run.legacy_rebind");
    let neon_marker_current = web_report_case(&report, "web.wasm.webgpu.neon_marker.current");
    let neon_marker_legacy = web_report_case(&report, "web.wasm.webgpu.neon_marker.legacy_rebind");
    let direct_surface_current = web_report_case(&report, "web.wasm.webgpu.direct_surface.current");
    let direct_surface_legacy =
        web_report_case(&report, "web.wasm.webgpu.direct_surface.legacy_scene_present");
    let draw_item_coalescing_current =
        web_report_case(&report, "web.wasm.webgpu.draw_item_coalescing.current");
    let draw_item_coalescing_legacy =
        web_report_case(&report, "web.wasm.webgpu.draw_item_coalescing.legacy_uncoalesced");
    let draw_state_current = web_report_case(&report, "web.wasm.webgpu.draw_state_cache.current");
    let draw_state_legacy =
        web_report_case(&report, "web.wasm.webgpu.draw_state_cache.legacy_rebind");
    let clip_state_current = web_report_case(&report, "web.wasm.webgpu.clip_state_cache.current");
    let clip_state_legacy =
        web_report_case(&report, "web.wasm.webgpu.clip_state_cache.legacy_rebind");

    for case in [
        frame,
        current,
        legacy,
        glyph_current,
        glyph_legacy,
        image_current,
        image_legacy,
        upload_scratch_current,
        upload_scratch_legacy,
        effect_current,
        effect_legacy,
        backdrop_batch_current,
        backdrop_batch_legacy,
        scene3d_reused,
        scene3d_stress_reused,
        mixed,
        mixed_legacy,
        layer_effects,
        layer_effects_legacy,
        clean_layer,
        dirty_layer,
        command_family,
        command_family_legacy,
        glyph_run_current,
        glyph_run_legacy,
        neon_marker_current,
        neon_marker_legacy,
        direct_surface_current,
        direct_surface_legacy,
        draw_item_coalescing_current,
        draw_item_coalescing_legacy,
        draw_state_current,
        draw_state_legacy,
        clip_state_current,
        clip_state_legacy,
    ] {
        assert_web_report_zero_resource_churn(case, false);
    }
    for case in [scene3d_recreate, scene3d_stress_recreate] {
        assert_web_report_zero_resource_churn(case, true);
        assert!(web_report_number(case, "mesh3d_creates") > 0.0);
    }

    for case in [current, legacy] {
        assert!(web_report_number(case, "vertices") > 0.0);
        assert!(web_report_number(case, "vertex_bytes") > 0.0);
        assert!(web_report_number(case, "id_mask_draws") > 0.0);
        assert!(web_report_number(case, "id_mask_raster_passes") > 0.0);
        assert!(web_report_number(case, "id_mask_field_jump_passes") > 0.0);
        assert!(web_report_number(case, "id_mask_compositor_passes") > 0.0);
    }
    assert!(web_report_number(frame, "solid_tris") > 0.0);
    assert!(web_report_number(frame, "draw_passes") > 0.0);
    assert!(web_report_number(frame, "glyph_quads") > 0.0);
    assert!(web_report_number(glyph_current, "glyph_quads") > 0.0);
    assert!(web_report_number(glyph_legacy, "glyph_quads") > 0.0);
    assert_eq!(
        web_report_number(glyph_current, "gpu_timestamp_passes"),
        web_report_number(glyph_current, "render_passes"),
    );
    assert_eq!(
        web_report_number(glyph_legacy, "gpu_timestamp_passes"),
        web_report_number(glyph_legacy, "render_passes"),
    );
    assert!(web_report_number(glyph_current, "gpu_timestamp_total_ns") > 0.0);
    assert!(web_report_number(glyph_legacy, "gpu_timestamp_total_ns") > 0.0);
    assert!(web_report_number(image_current, "image_draws") > 0.0);
    assert!(web_report_number(image_legacy, "image_draws") > 0.0);
    assert_eq!(
        web_report_number(image_current, "gpu_timestamp_passes"),
        web_report_number(image_current, "render_passes"),
    );
    assert_eq!(
        web_report_number(image_legacy, "gpu_timestamp_passes"),
        web_report_number(image_legacy, "render_passes"),
    );
    assert!(web_report_number(image_current, "gpu_timestamp_total_ns") > 0.0);
    assert!(web_report_number(image_legacy, "gpu_timestamp_total_ns") > 0.0);
    assert_eq!(web_report_number(upload_scratch_current, "image_upload_temp_allocs"), 0.0);
    assert_eq!(web_report_number(upload_scratch_current, "image_upload_temp_bytes"), 0.0);
    assert!(web_report_number(upload_scratch_current, "image_upload_scratch_bytes") > 0.0);
    assert!(web_report_number(upload_scratch_legacy, "image_upload_temp_allocs") > 0.0);
    assert!(web_report_number(upload_scratch_legacy, "image_upload_temp_bytes") > 0.0);
    assert_eq!(
        web_report_number(upload_scratch_current, "texture_upload_bytes"),
        web_report_number(upload_scratch_legacy, "texture_upload_bytes"),
    );
    assert!(web_report_number(upload_scratch_current, "image_draws") > 0.0);
    assert!(web_report_number(upload_scratch_current, "glyph_quads") > 0.0);
    assert_eq!(
        web_report_number(upload_scratch_current, "gpu_timestamp_passes"),
        web_report_number(upload_scratch_current, "render_passes"),
    );
    assert_eq!(
        web_report_number(upload_scratch_legacy, "gpu_timestamp_passes"),
        web_report_number(upload_scratch_legacy, "render_passes"),
    );
    assert!(
        web_report_number(effect_current, "backdrop_draws")
            >= web_report_number(effect_current, "expected_backdrops")
    );
    assert!(
        web_report_number(effect_legacy, "backdrop_draws")
            >= web_report_number(effect_legacy, "expected_backdrops")
    );
    assert_eq!(web_report_number(effect_current, "effect_uniform_writes"), 1.0);
    assert!(
        web_report_number(effect_legacy, "effect_uniform_writes")
            > web_report_number(effect_current, "effect_uniform_writes")
    );
    assert_eq!(
        web_report_number(effect_current, "effect_uniform_slots"),
        web_report_number(effect_current, "expected_backdrops"),
    );
    assert_eq!(
        web_report_number(effect_legacy, "effect_uniform_slots"),
        web_report_number(effect_legacy, "expected_backdrops"),
    );
    assert_eq!(
        web_report_number(effect_current, "texture_copies"),
        web_report_number(effect_legacy, "texture_copies"),
    );
    assert_eq!(
        web_report_number(effect_current, "render_passes"),
        web_report_number(effect_legacy, "render_passes"),
    );
    assert_eq!(
        web_report_number(effect_current, "gpu_timestamp_passes"),
        web_report_number(effect_current, "render_passes"),
    );
    assert_eq!(
        web_report_number(effect_legacy, "gpu_timestamp_passes"),
        web_report_number(effect_legacy, "render_passes"),
    );
    assert!(web_report_number(effect_current, "gpu_timestamp_total_ns") >= 0.0);
    assert!(web_report_number(effect_legacy, "gpu_timestamp_total_ns") >= 0.0);
    assert!(
        web_report_number(backdrop_batch_current, "backdrop_draws")
            >= web_report_number(backdrop_batch_current, "expected_backdrops")
    );
    assert!(
        web_report_number(backdrop_batch_legacy, "backdrop_draws")
            >= web_report_number(backdrop_batch_legacy, "expected_backdrops")
    );
    assert_eq!(
        web_report_number(backdrop_batch_current, "effect_uniform_writes"),
        web_report_number(backdrop_batch_legacy, "effect_uniform_writes"),
    );
    assert_eq!(
        web_report_number(backdrop_batch_current, "effect_uniform_slots"),
        web_report_number(backdrop_batch_legacy, "effect_uniform_slots"),
    );
    assert!(
        web_report_number(backdrop_batch_current, "texture_copies")
            < web_report_number(backdrop_batch_legacy, "texture_copies")
    );
    assert!(
        web_report_number(backdrop_batch_current, "render_passes")
            < web_report_number(backdrop_batch_legacy, "render_passes")
    );
    assert_eq!(
        web_report_number(backdrop_batch_current, "gpu_timestamp_passes"),
        web_report_number(backdrop_batch_current, "render_passes"),
    );
    assert_eq!(
        web_report_number(backdrop_batch_legacy, "gpu_timestamp_passes"),
        web_report_number(backdrop_batch_legacy, "render_passes"),
    );
    assert_eq!(web_report_number(scene3d_reused, "mesh3d_creates"), 0.0);
    assert!(web_report_number(scene3d_reused, "scene3d_draws") > 0.0);
    assert!(web_report_number(scene3d_reused, "scene3d_passes") > 0.0);
    assert_eq!(web_report_number(scene3d_stress_reused, "mesh3d_creates"), 0.0);
    assert!(web_report_number(scene3d_stress_reused, "scene3d_draws") >= 64.0);
    assert!(web_report_number(scene3d_stress_recreate, "scene3d_draws") >= 64.0);
    assert!(web_report_number(mixed, "backdrop_draws") > 0.0);
    assert!(web_report_number(mixed_legacy, "backdrop_draws") > 0.0);
    assert!(web_report_number(mixed, "visual_effect_draws") > 0.0);
    assert!(web_report_number(mixed_legacy, "visual_effect_draws") > 0.0);
    assert!(web_report_number(mixed, "layer_draws") > 0.0);
    assert!(web_report_number(mixed_legacy, "layer_draws") > 0.0);
    assert!(web_report_number(mixed, "clip_depth_peak") > 0.0);
    assert!(web_report_number(mixed_legacy, "clip_depth_peak") > 0.0);
    assert!(web_report_number(mixed, "damage_rects") > 0.0);
    assert!(web_report_number(mixed_legacy, "damage_rects") > 0.0);
    assert!(web_report_number(mixed, "texture_copies") > 0.0);
    assert_eq!(
        web_report_number(mixed, "draw_items"),
        web_report_number(mixed_legacy, "draw_items"),
    );
    assert_eq!(
        web_report_number(mixed, "glyph_quads"),
        web_report_number(mixed_legacy, "glyph_quads"),
    );
    assert_eq!(
        web_report_number(mixed, "image_draws"),
        web_report_number(mixed_legacy, "image_draws"),
    );
    assert!(web_report_number(mixed, "image_draws") >= web_report_number(mixed, "image_tiles"));
    assert!(
        web_report_number(mixed_legacy, "image_draws")
            >= web_report_number(mixed_legacy, "image_tiles")
    );
    assert_eq!(
        web_report_number(mixed, "backdrop_draws"),
        web_report_number(mixed_legacy, "backdrop_draws"),
    );
    assert_eq!(
        web_report_number(mixed, "visual_effect_draws"),
        web_report_number(mixed_legacy, "visual_effect_draws"),
    );
    assert_eq!(
        web_report_number(mixed, "layer_draws"),
        web_report_number(mixed_legacy, "layer_draws"),
    );
    assert_eq!(
        web_report_number(mixed, "damage_rects"),
        web_report_number(mixed_legacy, "damage_rects"),
    );
    assert!(
        web_report_number(mixed, "draw_pipeline_binds")
            < web_report_number(mixed_legacy, "draw_pipeline_binds")
    );
    assert!(
        web_report_number(mixed, "draw_bind_group_binds")
            <= web_report_number(mixed_legacy, "draw_bind_group_binds")
    );
    assert!(
        web_report_number(mixed, "draw_scissor_sets")
            < web_report_number(mixed_legacy, "draw_scissor_sets")
    );
    assert!(
        web_report_number(mixed, "effect_uniform_writes")
            < web_report_number(mixed_legacy, "effect_uniform_writes")
    );
    assert!(
        web_report_number(mixed, "texture_copies")
            <= web_report_number(mixed_legacy, "texture_copies")
    );
    assert!(
        web_report_number(mixed, "render_passes")
            <= web_report_number(mixed_legacy, "render_passes")
    );
    assert_eq!(
        web_report_number(mixed, "gpu_timestamp_passes"),
        web_report_number(mixed, "render_passes"),
    );
    assert_eq!(
        web_report_number(mixed_legacy, "gpu_timestamp_passes"),
        web_report_number(mixed_legacy, "render_passes"),
    );
    assert_eq!(
        web_report_number(&report["mixed_summary"], "current_p50_ms"),
        web_report_number(mixed, "p50_ms"),
    );
    assert_eq!(
        web_report_number(&report["mixed_summary"], "legacy_p50_ms"),
        web_report_number(mixed_legacy, "p50_ms"),
    );
    assert_eq!(
        web_report_number(&report["mixed_summary"], "current_draw_pipeline_binds"),
        web_report_number(mixed, "draw_pipeline_binds"),
    );
    assert_eq!(
        web_report_number(&report["mixed_summary"], "legacy_draw_pipeline_binds"),
        web_report_number(mixed_legacy, "draw_pipeline_binds"),
    );
    assert!(web_report_number(layer_effects, "image_draws") > 0.0);
    assert!(web_report_number(layer_effects_legacy, "image_draws") > 0.0);
    assert!(web_report_number(layer_effects, "glyph_quads") > 0.0);
    assert!(web_report_number(layer_effects_legacy, "glyph_quads") > 0.0);
    assert!(
        web_report_number(layer_effects, "layer_draws")
            >= web_report_number(layer_effects, "expected_layers")
    );
    assert!(
        web_report_number(layer_effects_legacy, "layer_draws")
            >= web_report_number(layer_effects_legacy, "expected_layers")
    );
    assert!(
        web_report_number(layer_effects, "damage_rects")
            >= web_report_number(layer_effects, "expected_damage_rects")
    );
    assert!(
        web_report_number(layer_effects_legacy, "damage_rects")
            >= web_report_number(layer_effects_legacy, "expected_damage_rects")
    );
    assert!(web_report_number(layer_effects, "clip_depth_peak") > 0.0);
    assert!(web_report_number(layer_effects_legacy, "clip_depth_peak") > 0.0);
    assert!(web_report_number(layer_effects, "backdrop_draws") > 0.0);
    assert!(web_report_number(layer_effects_legacy, "backdrop_draws") > 0.0);
    assert!(web_report_number(layer_effects, "visual_effect_draws") > 0.0);
    assert!(web_report_number(layer_effects_legacy, "visual_effect_draws") > 0.0);
    assert!(web_report_number(layer_effects, "spinner_draws") > 0.0);
    assert!(web_report_number(layer_effects_legacy, "spinner_draws") > 0.0);
    assert!(web_report_number(layer_effects, "texture_copies") > 0.0);
    assert!(web_report_number(layer_effects_legacy, "texture_copies") > 0.0);
    assert_eq!(
        web_report_number(layer_effects, "draw_items"),
        web_report_number(layer_effects_legacy, "draw_items"),
    );
    assert_eq!(
        web_report_number(layer_effects, "glyph_quads"),
        web_report_number(layer_effects_legacy, "glyph_quads"),
    );
    assert_eq!(
        web_report_number(layer_effects, "image_draws"),
        web_report_number(layer_effects_legacy, "image_draws"),
    );
    assert!(
        web_report_number(layer_effects, "image_draws")
            >= web_report_number(layer_effects, "image_tiles")
    );
    assert!(
        web_report_number(layer_effects_legacy, "image_draws")
            >= web_report_number(layer_effects_legacy, "image_tiles")
    );
    assert_eq!(
        web_report_number(layer_effects, "layer_draws"),
        web_report_number(layer_effects_legacy, "layer_draws"),
    );
    assert_eq!(
        web_report_number(layer_effects, "damage_rects"),
        web_report_number(layer_effects_legacy, "damage_rects"),
    );
    assert_eq!(
        web_report_number(layer_effects, "backdrop_draws"),
        web_report_number(layer_effects_legacy, "backdrop_draws"),
    );
    assert_eq!(
        web_report_number(layer_effects, "visual_effect_draws"),
        web_report_number(layer_effects_legacy, "visual_effect_draws"),
    );
    assert_eq!(
        web_report_number(layer_effects, "spinner_draws"),
        web_report_number(layer_effects_legacy, "spinner_draws"),
    );
    assert!(
        web_report_number(layer_effects, "draw_pipeline_binds")
            < web_report_number(layer_effects_legacy, "draw_pipeline_binds")
    );
    assert!(
        web_report_number(layer_effects, "draw_bind_group_binds")
            < web_report_number(layer_effects_legacy, "draw_bind_group_binds")
    );
    assert!(
        web_report_number(layer_effects, "draw_scissor_sets")
            < web_report_number(layer_effects_legacy, "draw_scissor_sets")
    );
    assert!(
        web_report_number(layer_effects, "effect_uniform_writes")
            < web_report_number(layer_effects_legacy, "effect_uniform_writes")
    );
    assert!(
        web_report_number(layer_effects, "texture_copies")
            < web_report_number(layer_effects_legacy, "texture_copies")
    );
    assert!(
        web_report_number(layer_effects, "render_passes")
            < web_report_number(layer_effects_legacy, "render_passes")
    );
    assert_eq!(
        web_report_number(layer_effects, "gpu_timestamp_passes"),
        web_report_number(layer_effects, "render_passes"),
    );
    assert_eq!(
        web_report_number(layer_effects_legacy, "gpu_timestamp_passes"),
        web_report_number(layer_effects_legacy, "render_passes"),
    );
    assert_eq!(
        web_report_number(&report["layer_effects_summary"], "current_p50_ms"),
        web_report_number(layer_effects, "p50_ms"),
    );
    assert_eq!(
        web_report_number(&report["layer_effects_summary"], "legacy_p50_ms"),
        web_report_number(layer_effects_legacy, "p50_ms"),
    );
    assert!(web_report_number(&report["layer_effects_summary"], "legacy_over_current") > 1.0);
    assert_eq!(
        web_report_number(&report["layer_effects_summary"], "current_draw_pipeline_binds"),
        web_report_number(layer_effects, "draw_pipeline_binds"),
    );
    assert_eq!(
        web_report_number(&report["layer_effects_summary"], "legacy_draw_pipeline_binds"),
        web_report_number(layer_effects_legacy, "draw_pipeline_binds"),
    );
    assert_eq!(web_report_number(clean_layer, "layer_cache_hits"), 1.0);
    assert_eq!(web_report_number(clean_layer, "layer_cache_misses"), 0.0);
    assert!(
        web_report_number(clean_layer, "layer_cache_skipped_draws")
            > web_report_number(clean_layer, "draw_items")
    );
    assert_eq!(web_report_number(clean_layer, "layer_passes"), 0.0);
    assert_eq!(web_report_number(dirty_layer, "layer_cache_hits"), 0.0);
    assert_eq!(web_report_number(dirty_layer, "layer_cache_misses"), 1.0);
    assert_eq!(web_report_number(dirty_layer, "layer_cache_skipped_draws"), 0.0);
    assert_eq!(web_report_number(dirty_layer, "layer_passes"), 1.0);
    assert!(
        web_report_number(clean_layer, "draw_items") < web_report_number(dirty_layer, "draw_items")
    );
    assert!(
        web_report_number(clean_layer, "render_passes")
            < web_report_number(dirty_layer, "render_passes")
    );
    assert_eq!(
        web_report_number(clean_layer, "gpu_timestamp_passes"),
        web_report_number(clean_layer, "render_passes"),
    );
    assert_eq!(
        web_report_number(dirty_layer, "gpu_timestamp_passes"),
        web_report_number(dirty_layer, "render_passes"),
    );
    assert_eq!(
        web_report_number(&report["clean_layer_summary"], "clean_p50_ms"),
        web_report_number(clean_layer, "p50_ms"),
    );
    assert_eq!(
        web_report_number(&report["clean_layer_summary"], "dirty_p50_ms"),
        web_report_number(dirty_layer, "p50_ms"),
    );
    assert_eq!(
        web_report_number(&report["clean_layer_summary"], "clean_layer_cache_hits"),
        web_report_number(clean_layer, "layer_cache_hits"),
    );
    assert!(
        web_report_number(command_family, "image_mesh_draws")
            >= web_report_number(command_family, "expected_image_meshes")
    );
    assert!(
        web_report_number(command_family_legacy, "image_mesh_draws")
            >= web_report_number(command_family_legacy, "expected_image_meshes")
    );
    assert!(
        web_report_number(command_family, "nine_slice_draws")
            >= web_report_number(command_family, "expected_nine_slices")
    );
    assert!(
        web_report_number(command_family_legacy, "nine_slice_draws")
            >= web_report_number(command_family_legacy, "expected_nine_slices")
    );
    assert!(
        web_report_number(command_family, "sdf_glyph_quads")
            >= web_report_number(command_family, "expected_sdf_glyphs")
    );
    assert!(
        web_report_number(command_family_legacy, "sdf_glyph_quads")
            >= web_report_number(command_family_legacy, "expected_sdf_glyphs")
    );
    assert_eq!(web_report_number(command_family, "expected_camera_bg"), 0.0);
    assert_eq!(web_report_number(command_family_legacy, "expected_camera_bg"), 0.0);
    assert_eq!(web_report_number(command_family, "camera_bg_draws"), 0.0);
    assert_eq!(web_report_number(command_family_legacy, "camera_bg_draws"), 0.0);
    assert!(web_report_number(command_family, "image_draws") >= 10.0);
    assert_eq!(
        web_report_number(command_family, "draw_items"),
        web_report_number(command_family_legacy, "draw_items"),
    );
    assert_eq!(
        web_report_number(command_family, "image_mesh_draws"),
        web_report_number(command_family_legacy, "image_mesh_draws"),
    );
    assert_eq!(
        web_report_number(command_family, "nine_slice_draws"),
        web_report_number(command_family_legacy, "nine_slice_draws"),
    );
    assert_eq!(
        web_report_number(command_family, "sdf_glyph_quads"),
        web_report_number(command_family_legacy, "sdf_glyph_quads"),
    );
    assert!(
        web_report_number(command_family, "draw_pipeline_binds")
            < web_report_number(command_family_legacy, "draw_pipeline_binds")
    );
    assert!(
        web_report_number(command_family, "draw_bind_group_binds")
            < web_report_number(command_family_legacy, "draw_bind_group_binds")
    );
    assert!(
        web_report_number(command_family, "draw_scissor_sets")
            < web_report_number(command_family_legacy, "draw_scissor_sets")
    );
    assert_eq!(
        web_report_number(command_family, "gpu_timestamp_passes"),
        web_report_number(command_family, "render_passes"),
    );
    assert_eq!(
        web_report_number(command_family_legacy, "gpu_timestamp_passes"),
        web_report_number(command_family_legacy, "render_passes"),
    );
    assert_eq!(
        web_report_number(&report["command_family_summary"], "current_p50_ms"),
        web_report_number(command_family, "p50_ms"),
    );
    assert_eq!(
        web_report_number(&report["command_family_summary"], "legacy_p50_ms"),
        web_report_number(command_family_legacy, "p50_ms"),
    );
    assert!(web_report_number(&report["command_family_summary"], "legacy_over_current") > 1.0);
    assert_eq!(
        web_report_number(&report["command_family_summary"], "current_draw_pipeline_binds"),
        web_report_number(command_family, "draw_pipeline_binds"),
    );
    assert_eq!(
        web_report_number(&report["command_family_summary"], "legacy_draw_pipeline_binds"),
        web_report_number(command_family_legacy, "draw_pipeline_binds"),
    );
    assert_eq!(web_report_number(glyph_run_current, "expected_glyph_runs"), 64.0);
    assert_eq!(web_report_number(glyph_run_legacy, "expected_glyph_runs"), 64.0);
    assert_eq!(web_report_number(glyph_run_current, "expected_glyphs_per_run"), 8.0);
    assert_eq!(web_report_number(glyph_run_legacy, "expected_glyphs_per_run"), 8.0);
    assert_eq!(web_report_number(glyph_run_current, "expected_glyph_quads"), 512.0);
    assert_eq!(web_report_number(glyph_run_legacy, "expected_glyph_quads"), 512.0);
    assert_eq!(web_report_number(glyph_run_current, "expected_sdf_runs"), 32.0);
    assert_eq!(web_report_number(glyph_run_legacy, "expected_sdf_runs"), 32.0);
    assert_eq!(web_report_number(glyph_run_current, "expected_sdf_glyph_quads"), 256.0);
    assert_eq!(web_report_number(glyph_run_legacy, "expected_sdf_glyph_quads"), 256.0);
    assert_eq!(web_report_number(glyph_run_current, "expected_draw_items"), 65.0);
    assert_eq!(web_report_number(glyph_run_legacy, "expected_draw_items"), 65.0);
    assert_eq!(
        web_report_number(glyph_run_current, "draw_items"),
        web_report_number(glyph_run_current, "expected_draw_items"),
    );
    assert_eq!(
        web_report_number(glyph_run_legacy, "draw_items"),
        web_report_number(glyph_run_legacy, "expected_draw_items"),
    );
    assert_eq!(
        web_report_number(glyph_run_current, "glyph_quads"),
        web_report_number(glyph_run_current, "expected_glyph_quads"),
    );
    assert_eq!(
        web_report_number(glyph_run_legacy, "glyph_quads"),
        web_report_number(glyph_run_legacy, "expected_glyph_quads"),
    );
    assert_eq!(
        web_report_number(glyph_run_current, "sdf_glyph_quads"),
        web_report_number(glyph_run_current, "expected_sdf_glyph_quads"),
    );
    assert_eq!(
        web_report_number(glyph_run_legacy, "sdf_glyph_quads"),
        web_report_number(glyph_run_legacy, "expected_sdf_glyph_quads"),
    );
    assert_eq!(
        web_report_number(glyph_run_current, "render_passes"),
        web_report_number(glyph_run_legacy, "render_passes"),
    );
    assert_eq!(
        web_report_number(glyph_run_current, "draw_passes"),
        web_report_number(glyph_run_legacy, "draw_passes"),
    );
    assert!(
        web_report_number(glyph_run_current, "draw_pipeline_binds")
            < web_report_number(glyph_run_legacy, "draw_pipeline_binds")
    );
    assert!(
        web_report_number(glyph_run_current, "draw_bind_group_binds")
            < web_report_number(glyph_run_legacy, "draw_bind_group_binds")
    );
    assert!(
        web_report_number(glyph_run_current, "draw_scissor_sets")
            < web_report_number(glyph_run_legacy, "draw_scissor_sets")
    );
    assert_eq!(
        web_report_number(glyph_run_current, "gpu_timestamp_passes"),
        web_report_number(glyph_run_current, "render_passes"),
    );
    assert_eq!(
        web_report_number(glyph_run_legacy, "gpu_timestamp_passes"),
        web_report_number(glyph_run_legacy, "render_passes"),
    );
    assert_eq!(
        web_report_number(&report["glyph_run_summary"], "current_p50_ms"),
        web_report_number(glyph_run_current, "p50_ms"),
    );
    assert_eq!(
        web_report_number(&report["glyph_run_summary"], "legacy_p50_ms"),
        web_report_number(glyph_run_legacy, "p50_ms"),
    );
    assert!(web_report_number(&report["glyph_run_summary"], "legacy_over_current") > 1.0);
    assert_eq!(
        web_report_number(&report["glyph_run_summary"], "current_draw_pipeline_binds"),
        web_report_number(glyph_run_current, "draw_pipeline_binds"),
    );
    assert_eq!(
        web_report_number(&report["glyph_run_summary"], "legacy_draw_pipeline_binds"),
        web_report_number(glyph_run_legacy, "draw_pipeline_binds"),
    );
    assert_eq!(web_report_number(neon_marker_current, "expected_markers"), 64.0);
    assert_eq!(web_report_number(neon_marker_legacy, "expected_markers"), 64.0);
    assert_eq!(web_report_number(neon_marker_current, "expected_draw_items"), 192.0);
    assert_eq!(web_report_number(neon_marker_legacy, "expected_draw_items"), 192.0);
    assert_eq!(
        web_report_number(neon_marker_current, "draw_items"),
        web_report_number(neon_marker_current, "expected_draw_items"),
    );
    assert_eq!(
        web_report_number(neon_marker_legacy, "draw_items"),
        web_report_number(neon_marker_legacy, "expected_draw_items"),
    );
    assert_eq!(
        web_report_number(neon_marker_current, "solid_tris"),
        web_report_number(neon_marker_legacy, "solid_tris"),
    );
    assert!(web_report_number(neon_marker_current, "solid_tris") > 0.0);
    assert!(
        web_report_number(neon_marker_current, "draw_pipeline_binds")
            < web_report_number(neon_marker_legacy, "draw_pipeline_binds")
    );
    assert_eq!(
        web_report_number(neon_marker_current, "draw_bind_group_binds"),
        web_report_number(neon_marker_legacy, "draw_bind_group_binds"),
    );
    assert!(
        web_report_number(neon_marker_current, "draw_scissor_sets")
            < web_report_number(neon_marker_legacy, "draw_scissor_sets")
    );
    assert_eq!(
        web_report_number(neon_marker_current, "gpu_timestamp_passes"),
        web_report_number(neon_marker_current, "render_passes"),
    );
    assert_eq!(
        web_report_number(neon_marker_legacy, "gpu_timestamp_passes"),
        web_report_number(neon_marker_legacy, "render_passes"),
    );
    assert_eq!(
        web_report_number(&report["neon_marker_summary"], "current_p50_ms"),
        web_report_number(neon_marker_current, "p50_ms"),
    );
    assert_eq!(
        web_report_number(&report["neon_marker_summary"], "legacy_p50_ms"),
        web_report_number(neon_marker_legacy, "p50_ms"),
    );
    assert_eq!(
        web_report_number(&report["neon_marker_summary"], "current_draw_pipeline_binds"),
        web_report_number(neon_marker_current, "draw_pipeline_binds"),
    );
    assert_eq!(
        web_report_number(&report["neon_marker_summary"], "legacy_draw_pipeline_binds"),
        web_report_number(neon_marker_legacy, "draw_pipeline_binds"),
    );
    assert_eq!(web_report_number(direct_surface_current, "expected_image_draws"), 384.0);
    assert_eq!(web_report_number(direct_surface_legacy, "expected_image_draws"), 384.0);
    assert_eq!(web_report_number(direct_surface_current, "expected_draw_items"), 385.0);
    assert_eq!(web_report_number(direct_surface_legacy, "expected_draw_items"), 385.0);
    assert_eq!(
        web_report_number(direct_surface_current, "draw_items"),
        web_report_number(direct_surface_current, "expected_draw_items"),
    );
    assert_eq!(
        web_report_number(direct_surface_legacy, "draw_items"),
        web_report_number(direct_surface_legacy, "expected_draw_items"),
    );
    assert_eq!(
        web_report_number(direct_surface_current, "image_draws"),
        web_report_number(direct_surface_current, "expected_image_draws"),
    );
    assert_eq!(
        web_report_number(direct_surface_legacy, "image_draws"),
        web_report_number(direct_surface_legacy, "expected_image_draws"),
    );
    assert_eq!(
        web_report_number(direct_surface_current, "draw_passes"),
        web_report_number(direct_surface_legacy, "draw_passes"),
    );
    assert_eq!(web_report_number(direct_surface_current, "clear_passes"), 0.0);
    assert!(web_report_number(direct_surface_legacy, "clear_passes") > 0.0);
    assert_eq!(web_report_number(direct_surface_current, "present_passes"), 0.0);
    assert!(web_report_number(direct_surface_legacy, "present_passes") > 0.0);
    assert!(
        web_report_number(direct_surface_current, "render_passes")
            < web_report_number(direct_surface_legacy, "render_passes")
    );
    assert!(
        web_report_number(direct_surface_current, "gpu_timestamp_total_ns")
            < web_report_number(direct_surface_legacy, "gpu_timestamp_total_ns")
    );
    assert_eq!(
        web_report_number(direct_surface_current, "texture_copies"),
        web_report_number(direct_surface_legacy, "texture_copies"),
    );
    assert_eq!(
        web_report_number(direct_surface_current, "gpu_timestamp_passes"),
        web_report_number(direct_surface_current, "render_passes"),
    );
    assert_eq!(
        web_report_number(direct_surface_legacy, "gpu_timestamp_passes"),
        web_report_number(direct_surface_legacy, "render_passes"),
    );
    assert_eq!(
        web_report_number(&report["direct_surface_summary"], "current_p50_ms"),
        web_report_number(direct_surface_current, "p50_ms"),
    );
    assert_eq!(
        web_report_number(&report["direct_surface_summary"], "legacy_p50_ms"),
        web_report_number(direct_surface_legacy, "p50_ms"),
    );
    assert!(
        web_report_number(&report["direct_surface_summary"], "current_gpu_timestamp_total_ns")
            < web_report_number(&report["direct_surface_summary"], "legacy_gpu_timestamp_total_ns"),
        "direct surface GPU timestamp total should beat forced scene-present"
    );
    assert_eq!(
        web_report_number(draw_item_coalescing_current, "expected_source_draw_items"),
        1024.0,
    );
    assert_eq!(
        web_report_number(draw_item_coalescing_legacy, "expected_source_draw_items"),
        1024.0,
    );
    assert_eq!(web_report_number(draw_item_coalescing_current, "expected_current_draw_items"), 1.0,);
    assert_eq!(web_report_number(draw_item_coalescing_legacy, "expected_current_draw_items"), 1.0,);
    assert_eq!(
        web_report_number(draw_item_coalescing_current, "draw_items"),
        web_report_number(draw_item_coalescing_current, "expected_current_draw_items"),
    );
    assert_eq!(
        web_report_number(draw_item_coalescing_legacy, "draw_items"),
        web_report_number(draw_item_coalescing_legacy, "expected_source_draw_items"),
    );
    assert_eq!(
        web_report_number(draw_item_coalescing_current, "draw_items_coalesced"),
        web_report_number(draw_item_coalescing_current, "expected_source_draw_items")
            - web_report_number(draw_item_coalescing_current, "expected_current_draw_items"),
    );
    assert_eq!(web_report_number(draw_item_coalescing_legacy, "draw_items_coalesced"), 0.0,);
    assert_eq!(
        web_report_number(draw_item_coalescing_current, "draws"),
        web_report_number(draw_item_coalescing_current, "draw_items"),
    );
    assert_eq!(
        web_report_number(draw_item_coalescing_legacy, "draws"),
        web_report_number(draw_item_coalescing_legacy, "draw_items"),
    );
    assert!(
        web_report_number(draw_item_coalescing_current, "draw_items")
            < web_report_number(draw_item_coalescing_legacy, "draw_items"),
        "draw-item coalescing should reduce encoded draw items"
    );
    assert!(
        web_report_number(draw_item_coalescing_current, "draw_pipeline_binds")
            <= web_report_number(draw_item_coalescing_legacy, "draw_pipeline_binds")
    );
    assert!(
        web_report_number(draw_item_coalescing_current, "draw_bind_group_binds")
            <= web_report_number(draw_item_coalescing_legacy, "draw_bind_group_binds")
    );
    assert!(
        web_report_number(draw_item_coalescing_current, "draw_scissor_sets")
            <= web_report_number(draw_item_coalescing_legacy, "draw_scissor_sets")
    );
    assert!(
        web_report_number(&report["draw_item_coalescing_summary"], "legacy_over_current") > 1.0,
        "draw-item coalescing p50 should beat uncoalesced legacy"
    );
    assert_eq!(
        web_report_number(&report["draw_item_coalescing_summary"], "current_draw_items"),
        web_report_number(draw_item_coalescing_current, "draw_items"),
    );
    assert_eq!(
        web_report_number(&report["draw_item_coalescing_summary"], "legacy_draw_items"),
        web_report_number(draw_item_coalescing_legacy, "draw_items"),
    );
    assert_eq!(
        web_report_number(draw_item_coalescing_current, "gpu_timestamp_passes"),
        web_report_number(draw_item_coalescing_current, "render_passes"),
    );
    assert_eq!(
        web_report_number(draw_item_coalescing_legacy, "gpu_timestamp_passes"),
        web_report_number(draw_item_coalescing_legacy, "render_passes"),
    );
    assert!(
        web_report_number(draw_state_current, "draw_items")
            >= web_report_number(draw_state_current, "expected_draw_items")
    );
    assert!(
        web_report_number(draw_state_legacy, "draw_items")
            >= web_report_number(draw_state_legacy, "expected_draw_items")
    );
    assert_eq!(
        web_report_number(draw_state_current, "draws"),
        web_report_number(draw_state_current, "draw_items"),
    );
    assert_eq!(
        web_report_number(draw_state_legacy, "draws"),
        web_report_number(draw_state_legacy, "draw_items"),
    );
    assert!(
        web_report_number(draw_state_current, "draw_pipeline_binds")
            < web_report_number(draw_state_legacy, "draw_pipeline_binds")
    );
    assert!(
        web_report_number(draw_state_current, "draw_bind_group_binds")
            < web_report_number(draw_state_legacy, "draw_bind_group_binds")
    );
    assert!(
        web_report_number(draw_state_current, "draw_scissor_sets")
            < web_report_number(draw_state_legacy, "draw_scissor_sets")
    );
    assert!(
        web_report_number(&report["draw_state_summary"], "legacy_over_current") > 1.0,
        "draw-state cache p50 should beat legacy rebind"
    );
    assert_eq!(
        web_report_number(draw_state_current, "gpu_timestamp_passes"),
        web_report_number(draw_state_current, "render_passes"),
    );
    assert_eq!(
        web_report_number(draw_state_legacy, "gpu_timestamp_passes"),
        web_report_number(draw_state_legacy, "render_passes"),
    );
    assert!(
        web_report_number(clip_state_current, "draw_items")
            >= web_report_number(clip_state_current, "expected_draw_items")
    );
    assert!(
        web_report_number(clip_state_legacy, "draw_items")
            >= web_report_number(clip_state_legacy, "expected_draw_items")
    );
    assert_eq!(
        web_report_number(clip_state_current, "draws"),
        web_report_number(clip_state_current, "draw_items"),
    );
    assert_eq!(
        web_report_number(clip_state_legacy, "draws"),
        web_report_number(clip_state_legacy, "draw_items"),
    );
    assert!(
        web_report_number(clip_state_current, "clip_depth_peak")
            >= web_report_number(clip_state_current, "expected_clip_depth")
    );
    assert!(
        web_report_number(clip_state_legacy, "clip_depth_peak")
            >= web_report_number(clip_state_legacy, "expected_clip_depth")
    );
    assert!(
        web_report_number(clip_state_current, "draw_pipeline_binds")
            < web_report_number(clip_state_legacy, "draw_pipeline_binds")
    );
    assert!(
        web_report_number(clip_state_current, "draw_bind_group_binds")
            < web_report_number(clip_state_legacy, "draw_bind_group_binds")
    );
    assert!(
        web_report_number(clip_state_current, "draw_scissor_sets")
            <= web_report_number(clip_state_current, "expected_clip_runs")
    );
    assert!(
        web_report_number(clip_state_current, "draw_scissor_sets")
            < web_report_number(clip_state_legacy, "draw_scissor_sets")
    );
    assert!(
        web_report_number(&report["clip_state_summary"], "legacy_over_current") > 1.0,
        "clip-state cache p50 should beat legacy rebind"
    );
    assert_eq!(
        web_report_number(clip_state_current, "gpu_timestamp_passes"),
        web_report_number(clip_state_current, "render_passes"),
    );
    assert_eq!(
        web_report_number(clip_state_legacy, "gpu_timestamp_passes"),
        web_report_number(clip_state_legacy, "render_passes"),
    );

    let ab = &report["ab_summary"];
    assert_eq!(
        ab["id"].as_str(),
        Some("web.wasm.webgpu.id_mask_compositor.current_vs_legacy_upload"),
    );
    assert_eq!(web_report_number(ab, "current_p50_ms"), web_report_number(current, "p50_ms"));
    assert_eq!(web_report_number(ab, "legacy_p50_ms"), web_report_number(legacy, "p50_ms"));
    assert!(web_report_number(ab, "legacy_over_current") > 1.0);
    assert_eq!(web_report_number(ab, "vertices"), web_report_number(current, "vertices"));
    assert_eq!(web_report_number(ab, "vertex_bytes"), web_report_number(current, "vertex_bytes"));

    let upload = &report["upload_summary"];
    assert_eq!(upload["id"].as_str(), Some("web.wasm.webgpu.upload.current_dirty_vs_legacy_full"));
    assert!(web_report_number(upload, "glyph_legacy_over_current") > 1.0);
    assert!(web_report_number(upload, "image_legacy_over_current") > 1.0);
    assert!(
        web_report_number(upload, "glyph_current_texture_upload_bytes")
            < web_report_number(upload, "glyph_legacy_texture_upload_bytes")
    );
    assert!(
        web_report_number(upload, "image_current_texture_upload_bytes")
            < web_report_number(upload, "image_legacy_texture_upload_bytes")
    );
    assert_eq!(
        web_report_number(upload, "glyph_current_gpu_timestamp_total_ns"),
        web_report_number(glyph_current, "gpu_timestamp_total_ns"),
    );
    assert_eq!(
        web_report_number(upload, "glyph_legacy_gpu_timestamp_total_ns"),
        web_report_number(glyph_legacy, "gpu_timestamp_total_ns"),
    );
    assert_eq!(
        web_report_number(upload, "image_current_gpu_timestamp_total_ns"),
        web_report_number(image_current, "gpu_timestamp_total_ns"),
    );
    assert_eq!(
        web_report_number(upload, "image_legacy_gpu_timestamp_total_ns"),
        web_report_number(image_legacy, "gpu_timestamp_total_ns"),
    );
    assert!(web_report_number(upload, "glyph_legacy_gpu_over_current") > 0.0);
    assert!(web_report_number(upload, "image_legacy_gpu_over_current") > 0.0);

    let upload_scratch = &report["upload_scratch_summary"];
    assert_eq!(
        upload_scratch["id"].as_str(),
        Some("web.wasm.webgpu.upload_scratch.current_reuse_vs_legacy_temp_alloc"),
    );
    assert!(web_report_number(upload_scratch, "legacy_over_current") > 1.0);
    assert_eq!(
        web_report_number(upload_scratch, "current_p50_ms"),
        web_report_number(upload_scratch_current, "p50_ms"),
    );
    assert_eq!(
        web_report_number(upload_scratch, "legacy_p50_ms"),
        web_report_number(upload_scratch_legacy, "p50_ms"),
    );
    assert_eq!(web_report_number(upload_scratch, "current_temp_allocs"), 0.0);
    assert!(web_report_number(upload_scratch, "legacy_temp_allocs") > 0.0);
    assert_eq!(web_report_number(upload_scratch, "current_temp_bytes"), 0.0);
    assert!(web_report_number(upload_scratch, "legacy_temp_bytes") > 0.0);
    assert_eq!(
        web_report_number(upload_scratch, "current_texture_upload_bytes"),
        web_report_number(upload_scratch, "legacy_texture_upload_bytes"),
    );

    let effect_uniform = &report["effect_uniform_summary"];
    assert_eq!(
        effect_uniform["id"].as_str(),
        Some("web.wasm.webgpu.effect_uniform.batched_vs_legacy_write_each"),
    );
    assert!(web_report_number(effect_uniform, "legacy_over_current") >= 0.0);
    assert_eq!(web_report_number(effect_uniform, "current_effect_uniform_writes"), 1.0);
    assert!(
        web_report_number(effect_uniform, "legacy_effect_uniform_writes")
            > web_report_number(effect_uniform, "current_effect_uniform_writes")
    );
    assert_eq!(
        web_report_number(effect_uniform, "current_effect_uniform_slots"),
        web_report_number(effect_uniform, "expected_backdrops"),
    );
    assert_eq!(
        web_report_number(effect_uniform, "legacy_effect_uniform_slots"),
        web_report_number(effect_uniform, "expected_backdrops"),
    );
    assert_eq!(
        web_report_number(effect_uniform, "current_texture_copies"),
        web_report_number(effect_uniform, "legacy_texture_copies"),
    );
    assert!(web_report_number(effect_uniform, "current_gpu_timestamp_total_ns") >= 0.0);
    assert!(web_report_number(effect_uniform, "legacy_gpu_timestamp_total_ns") >= 0.0);
    assert!(web_report_number(effect_uniform, "legacy_gpu_over_current") >= 0.0);

    let backdrop_batch = &report["backdrop_batch_summary"];
    assert_eq!(
        backdrop_batch["id"].as_str(),
        Some("web.wasm.webgpu.backdrop_batch.coalesced_vs_per_backdrop_copy"),
    );
    assert!(web_report_number(backdrop_batch, "legacy_over_current") > 1.0);
    assert_eq!(
        web_report_number(backdrop_batch, "current_effect_uniform_writes"),
        web_report_number(backdrop_batch, "legacy_effect_uniform_writes"),
    );
    assert_eq!(
        web_report_number(backdrop_batch, "current_effect_uniform_slots"),
        web_report_number(backdrop_batch, "legacy_effect_uniform_slots"),
    );
    assert!(
        web_report_number(backdrop_batch, "current_texture_copies")
            < web_report_number(backdrop_batch, "legacy_texture_copies")
    );
    assert!(
        web_report_number(backdrop_batch, "current_render_passes")
            < web_report_number(backdrop_batch, "legacy_render_passes")
    );

    let clip_state = &report["clip_state_summary"];
    assert_eq!(
        clip_state["id"].as_str(),
        Some("web.wasm.webgpu.clip_state_cache.current_vs_legacy_rebind"),
    );
    assert!(web_report_number(clip_state, "legacy_over_current") > 1.0);
    assert_eq!(
        web_report_number(clip_state, "current_draw_items"),
        web_report_number(clip_state, "expected_draw_items"),
    );
    assert_eq!(
        web_report_number(clip_state, "legacy_draw_items"),
        web_report_number(clip_state, "expected_draw_items"),
    );
    assert!(
        web_report_number(clip_state, "current_clip_depth_peak")
            >= web_report_number(clip_state, "expected_clip_depth")
    );
    assert!(
        web_report_number(clip_state, "legacy_clip_depth_peak")
            >= web_report_number(clip_state, "expected_clip_depth")
    );
    assert!(
        web_report_number(clip_state, "current_draw_scissor_sets")
            <= web_report_number(clip_state, "expected_clip_runs")
    );
    assert!(
        web_report_number(clip_state, "current_draw_scissor_sets")
            < web_report_number(clip_state, "legacy_draw_scissor_sets")
    );

    let scene3d = &report["scene3d_summary"];
    assert_eq!(
        scene3d["id"].as_str(),
        Some("web.wasm.webgpu.scene3d.reused_mesh_vs_recreate_mesh"),
    );
    assert_eq!(web_report_number(scene3d, "reused_mesh3d_creates"), 0.0);
    assert!(web_report_number(scene3d, "recreate_mesh3d_creates") > 0.0);
    assert_eq!(web_report_number(scene3d, "reused_buffer_grows"), 0.0);
    assert!(web_report_number(scene3d, "recreate_buffer_grows") > 0.0);
    assert_eq!(web_report_number(scene3d, "reused_cpu_scratch_grows"), 0.0);

    let scene3d_stress = &report["scene3d_stress_summary"];
    assert_eq!(
        scene3d_stress["id"].as_str(),
        Some("web.wasm.webgpu.scene3d.stress_reused_mesh_vs_stress_recreate_mesh"),
    );
    assert_eq!(web_report_number(scene3d_stress, "reused_mesh3d_creates"), 0.0);
    assert!(web_report_number(scene3d_stress, "recreate_mesh3d_creates") > 0.0);
    assert_eq!(web_report_number(scene3d_stress, "reused_buffer_grows"), 0.0);
    assert!(web_report_number(scene3d_stress, "recreate_buffer_grows") > 0.0);
    assert_eq!(web_report_number(scene3d_stress, "reused_cpu_scratch_grows"), 0.0);
    assert!(web_report_number(scene3d_stress, "instances") >= 64.0);

    let pixel = &report["pixel_check"];
    assert_eq!(web_report_number(pixel, "pixdiff"), 0.0);
    assert_eq!(web_report_number(pixel, "max_err"), 0.0);
    assert_eq!(web_report_number(pixel, "mse"), 0.0);
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
fn smoke_suite_keeps_popup_wheel_picker_case_id_stable() {
    let report = collect_suite_report(true).expect("collect smoke suite");
    let ids =
        report.cases.iter().map(|case| case.id.as_str()).collect::<std::collections::BTreeSet<_>>();

    assert!(ids.contains("cpu.authoring.popup_wheel_picker.interaction"));
    assert!(!ids.contains("cpu.authoring.popup_picker.interaction"));
}

#[test]
fn filtered_run_suite_skips_full_coverage_gate() {
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "cpu.system.prepare_draws.current")
        .arg("--run-suite")
        .arg("--smoke")
        .output()
        .expect("run filtered smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=1"), "stdout: {stdout}");
    assert!(stdout.contains("layout="), "stdout: {stdout}");
    assert!(stdout.contains("text_input="), "stdout: {stdout}");
    assert!(stdout.contains("endurance="), "stdout: {stdout}");
    assert!(stdout.contains("stress="), "stdout: {stdout}");
    assert!(stdout.contains("case=cpu.system.prepare_draws.current"), "stdout: {stdout}");
    assert!(!stdout.contains("case=cpu.system.prepare_draws.legacy"), "stdout: {stdout}");
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");
}

#[test]
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
fn filtered_run_suite_supports_wrapped_label_ab_cases() {
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
    assert!(stdout.contains("cases=2"), "stdout: {stdout}");
    assert!(stdout.contains("case=cpu.system.wrapped_label_cached_encode"), "stdout: {stdout}");
    assert!(stdout.contains("case=cpu.system.wrapped_label_legacy_fit_shape"), "stdout: {stdout}");
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");

    let report = std::fs::read_to_string(&json_out).expect("read filtered wrapped label report");
    let row = report_case_slice(&report, "cpu.system.wrapped_label_cached_encode");
    assert_eq!(report_f64(row, "wrapped_label_variants"), 4096.0);
    assert_eq!(report_f64(row, "atlas_create_calls"), 1.0);
    assert!(report_f64(row, "atlas_update_calls") >= 1.0);
    assert!(report_f64(row, "wrapped_label_glyph_runs") > 1.0);
    assert!(report_f64(row, "wrapped_label_vertices") > 0.0);
    let legacy = report_case_slice(&report, "cpu.system.wrapped_label_legacy_fit_shape");
    assert!(report_f64(legacy, "legacy_shape_calls") > report_f64(row, "wrapped_label_glyph_runs"));
    assert!(report_f64(legacy, "wrapped_label_vertices") > 0.0);
    let _ = std::fs::remove_file(json_out);
}

#[test]
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
    assert!(stdout.contains("cases=2"), "stdout: {stdout}");
    assert!(stdout.contains("case=cpu.system.picker_text_cached_encode"), "stdout: {stdout}");
    assert!(stdout.contains("case=cpu.system.picker_text_legacy_shape_upload"), "stdout: {stdout}");
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");

    let report =
        std::fs::read_to_string(&json_out).expect("read filtered picker text cached report");
    let row = report_case_slice(&report, "cpu.system.picker_text_cached_encode");
    assert_eq!(report_f64(row, "atlas_create_calls"), 1.0);
    assert_eq!(report_f64(row, "atlas_update_calls"), 1.0);
    assert!(report_f64(row, "picker_glyph_runs") > 0.0);
    assert!(report_f64(row, "picker_vertices") > 0.0);
    assert!(report_f64(row, "dirty_to_full_upload_ratio") < 1.0);
    let legacy = report_case_slice(&report, "cpu.system.picker_text_legacy_shape_upload");
    assert_eq!(report_f64(legacy, "atlas_create_calls"), 1.0);
    assert!(report_f64(legacy, "atlas_update_calls") >= 2.0);
    assert!(report_f64(legacy, "atlas_update_pixels") >= report_f64(legacy, "full_upload_pixels"));
    assert!(report_f64(legacy, "picker_glyph_runs") > 0.0);
    let _ = std::fs::remove_file(json_out);
}

#[test]
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
fn filtered_run_suite_supports_surface_text_atlas_context_authoring_case() {
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "cpu.authoring.surface_retained.text_atlas_context")
        .arg("--run-suite")
        .arg("--smoke")
        .output()
        .expect("run filtered surface text atlas context authoring smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=1"), "stdout: {stdout}");
    assert!(
        stdout.contains("case=cpu.authoring.surface_retained.text_atlas_context"),
        "stdout: {stdout}",
    );
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");
}

#[test]
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
fn filtered_run_suite_supports_non_draw_dirty_layout_case() {
    let mut json_out = std::env::temp_dir();
    json_out.push(format!("oxide-perf-runner-non-draw-dirty-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "cpu.layout.non_draw_dirty.retained_reuse")
        .arg("--run-suite")
        .arg("--smoke")
        .arg("--json-out")
        .arg(&json_out)
        .output()
        .expect("run filtered non-draw dirty layout smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=1"), "stdout: {stdout}");
    assert!(stdout.contains("case=cpu.layout.non_draw_dirty.retained_reuse"), "stdout: {stdout}",);
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");

    let report = std::fs::read_to_string(&json_out).expect("read filtered non-draw report");
    let row = report_case_slice(&report, "cpu.layout.non_draw_dirty.retained_reuse");
    assert_eq!(report_f64(row, "layout_visited_nodes_per_op"), 0.0);
    assert_eq!(report_f64(row, "layout_measured_children_per_op"), 0.0);
    assert_eq!(report_f64(row, "retained_rebuilt_nodes_per_op"), 0.0);
    assert_eq!(report_f64(row, "retained_rebuilt_ops"), 0.0);
    assert!(report_f64(row, "accessibility_dirty_ops") > 0.0);
    assert!(report_f64(row, "hit_test_dirty_ops") > 0.0);
    assert!(report_f64(row, "retained_reused_nodes_per_op") > 0.0);
    assert!(report_f64(row, "retained_reused_ops") > 0.0);
    let _ = std::fs::remove_file(json_out);
}

#[test]
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
fn filtered_run_suite_supports_metal_id_mask_ab_cases() {
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "gpu.system.id_mask_compositor")
        .arg("--run-suite")
        .arg("--smoke")
        .output()
        .expect("run filtered gpu system smoke suite");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "filtered suite failed: {stderr}");
    assert!(stdout.contains("cases=2"), "stdout: {stdout}");
    assert!(stdout.contains("case=gpu.system.id_mask_compositor.current"), "stdout: {stdout}");
    assert!(
        stdout.contains("case=gpu.system.id_mask_compositor.legacy_upload"),
        "stdout: {stdout}"
    );
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");
}

#[test]
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
    let _ = std::fs::remove_file(json_out);
}
