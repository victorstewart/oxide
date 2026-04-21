use oxide_perf_runner::{
    assert_full_coverage, collect_suite_report, compare_reports, AuditFinding,
    ContractCoverageReport, CoverageReport, PerfCaseResult, PerfReport,
};
use std::collections::BTreeMap;

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
fn smoke_suite_keeps_popup_wheel_picker_case_id_stable() {
    let report = collect_suite_report(true).expect("collect smoke suite");
    let ids =
        report.cases.iter().map(|case| case.id.as_str()).collect::<std::collections::BTreeSet<_>>();

    assert!(ids.contains("cpu.authoring.popup_wheel_picker.interaction"));
    assert!(!ids.contains("cpu.authoring.popup_picker.interaction"));
}
