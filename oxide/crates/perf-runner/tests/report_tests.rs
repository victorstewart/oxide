use oxide_perf_runner::{
    assert_case_metric_contract, assert_contract_coverage, assert_full_coverage,
    collect_suite_report, compare_reports, render_report_markdown, AuditFinding,
    ContractCoverageEntry, ContractCoverageReport, CoverageReport, PerfCaseResult, PerfReport,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
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

fn workspace_missing_case(report: &PerfReport, id: &str) -> bool {
    report.cases.iter().all(|case| case.id != id)
}

fn persisted_report_json(relative_path: &str) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative_path);
    let bytes = std::fs::read(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    serde_json::from_slice(&bytes).unwrap_or_else(|err| panic!("parse {}: {err}", path.display()))
}

fn json_key_set(value: &Value) -> BTreeSet<String> {
    value
        .as_object()
        .expect("json object")
        .keys()
        .cloned()
        .collect()
}

fn expected_key_set(keys: &[&str]) -> BTreeSet<String> {
    keys.iter().map(|key| String::from(*key)).collect()
}

fn assert_json_object_keys(value: &Value, expected: &[&str]) {
    assert_eq!(json_key_set(value), expected_key_set(expected));
}

fn string_key_set_digest(keys: &BTreeSet<String>) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for key in keys {
        for byte in key.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= u64::from(b'\n');
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn assert_json_object_key_digest(value: &Value, name: &str, expected_count: usize, expected_digest: u64) {
    let keys = json_key_set(value);
    assert_eq!(keys.len(), expected_count, "{name} key count changed: {keys:?}");
    assert_eq!(string_key_set_digest(&keys), expected_digest, "{name} key digest changed: {keys:?}");
}

fn assert_json_array_entry_key_digest(value: &Value, name: &str, expected_count: usize, expected_digest: u64)
{
   let rows = value.as_array().unwrap_or_else(|| panic!("{name} array"));
   assert!(!rows.is_empty(), "{name} has no rows");
   for row in rows
   {
      let keys = json_key_set(row);
      assert_eq!(keys.len(), expected_count, "{name} row key count changed: {keys:?}");
      assert_eq!(string_key_set_digest(&keys), expected_digest, "{name} row key digest changed: {keys:?}");
   }
}

fn metric_key_set(case: &PerfCaseResult) -> BTreeSet<String> {
    case.metrics.keys().cloned().collect()
}

fn assert_workspace_case_metric_key_digest(
    report: &PerfReport,
    id: &str,
    expected_count: usize,
    expected_digest: u64,
) {
    let case = workspace_case(report, id);
    let keys = metric_key_set(case);
    assert_eq!(keys.len(), expected_count, "{id} metric key count changed: {keys:?}");
    assert_eq!(string_key_set_digest(&keys), expected_digest, "{id} metric key digest changed: {keys:?}");
}

fn assert_json_object_has_keys(value: &Value, expected: &[&str]) {
    let keys = json_key_set(value);
    for key in expected {
        assert!(keys.contains(*key), "missing json key {key}");
    }
}

fn report_cases<'a>(report: &'a Value, name: &str) -> &'a [Value] {
    let cases = report["cases"].as_array().unwrap_or_else(|| panic!("{name} cases array"));
    assert!(!cases.is_empty(), "{name} report has no cases");
    cases
}

fn sorted_report_case_ids<'a>(report: &'a Value, name: &str) -> Vec<&'a str> {
    let mut ids: Vec<&str> = report_cases(report, name)
        .iter()
        .map(|case| case["id"].as_str().unwrap_or_else(|| panic!("{name} case has non-string id")))
        .collect();
    ids.sort_unstable();
    for pair in ids.windows(2) {
        assert_ne!(pair[0], pair[1], "{name} has duplicate case id {}", pair[0]);
    }
    ids
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

fn assert_report_case_id_set(report: &Value, name: &str, expected_count: usize, expected_digest: u64) {
    let ids = sorted_report_case_ids(report, name);
    assert_eq!(ids.len(), expected_count, "{name} case count changed");
    assert_eq!(case_id_digest(&ids), expected_digest, "{name} case id digest changed");
}

fn assert_report_case_key_class_digest(report: &Value, name: &str, expected_count: usize, expected_digest: u64)
{
   let mut classes = BTreeSet::new();
   for case in report_cases(report, name)
   {
      let id = case["id"].as_str().unwrap_or_else(|| panic!("{name} case has non-string id"));
      let keys = json_key_set(case);
      classes.insert(format!("{id}:{}:0x{:016x}", keys.len(), string_key_set_digest(&keys)));
   }
   assert_eq!(classes.len(), expected_count, "{name} case key class count changed: {classes:?}");
   assert_eq!(string_key_set_digest(&classes), expected_digest, "{name} case key class digest changed: {classes:?}");
}

fn assert_report_metric_key_class_digest(report: &Value, name: &str, expected_count: usize, expected_digest: u64)
{
   let mut classes = BTreeSet::new();
   for case in report_cases(report, name)
   {
      let id = case["id"].as_str().unwrap_or_else(|| panic!("{name} case has non-string id"));
      let keys = json_key_set(&case["metrics"]);
      classes.insert(format!("{id}:{}:0x{:016x}", keys.len(), string_key_set_digest(&keys)));
   }
   assert_eq!(classes.len(), expected_count, "{name} metric key class count changed: {classes:?}");
   assert_eq!(string_key_set_digest(&classes), expected_digest, "{name} metric key class digest changed: {classes:?}");
}

fn assert_all_report_cases_match_keys(report: &Value, name: &str, expected: &[&str]) {
    for case in report_cases(report, name) {
        let id = case["id"].as_str().unwrap_or("<missing id>");
        assert_eq!(json_key_set(case), expected_key_set(expected), "{name} case {id}");
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
fn persisted_report_root_and_case_schemas_are_frozen() {
    let perf_report_keys =
        ["cases", "contract", "coverage", "findings", "generated_label", "suite", "version"];
    let perf_case_keys = [
        "cache_state",
        "family",
        "gated",
        "id",
        "layer",
        "max",
        "mean",
        "median",
        "metrics",
        "min",
        "notes",
        "ops_per_sample",
        "p95",
        "p99",
        "refresh_mode",
        "samples",
        "scenario",
        "threshold_pct",
        "unit",
        "variant",
    ];
    let workspace = persisted_report_json("benchmarks/workspace/latest.json");
    assert_json_object_keys(&workspace, &perf_report_keys);
    assert_all_report_cases_match_keys(&workspace, "workspace latest", &perf_case_keys);

    let oxide_device = persisted_report_json("benchmarks/oxide-device/latest.json");
    assert_json_object_keys(&oxide_device, &perf_report_keys);
    assert_all_report_cases_match_keys(&oxide_device, "oxide device latest", &perf_case_keys);

    let uikit_report_keys = [
        "cases",
        "contract",
        "device_name",
        "energy_status",
        "generated_label",
        "notes",
        "suite",
        "version",
    ];
    let uikit_case_keys = [
        "benchmark_iterations",
        "cache_state",
        "canonical_signpost_source",
        "headline_metric",
        "id",
        "layer",
        "measure_iterations",
        "metrics",
        "notes",
        "oxide_case_id",
        "refresh_mode",
        "scenario",
        "style",
        "test_name",
        "threshold_pct",
    ];
    let uikit_device = persisted_report_json("benchmarks/uikit-device/latest.json");
    assert_json_object_keys(&uikit_device, &uikit_report_keys);
    assert_all_report_cases_match_keys(&uikit_device, "uikit device latest", &uikit_case_keys);

    let web_report_keys = [
        "backend_path_coverage",
        "backdrop_batch_summary",
        "benchmark_marks",
        "browser_environment",
        "browser_startup",
        "browser_target",
        "browser_trace",
        "capture_target",
        "cases",
        "clean_layer_summary",
        "command_family_summary",
        "direct_surface_summary",
        "effect_uniform_summary",
        "frame_loop_wasm_allocation_stages",
        "frame_loop_wasm_submit_allocation_stages",
        "generated_date",
        "glyph_run_summary",
        "gpu_stage_attribution",
        "gpu_timestamp_stage_breakdown",
        "id_mask_summary",
        "layer_effects_summary",
        "mixed_summary",
        "neon_marker_summary",
        "notes",
        "pixel_check",
        "scene3d_stress_summary",
        "scene3d_summary",
        "smoke",
        "status",
        "suite",
        "upload_summary",
        "url",
        "version",
        "warm_resource_churn",
        "wasm_allocation_audit",
        "wasm_allocation_invariance",
    ];
    let web_case_required_keys = [
        "avg_ms",
        "cache_state",
        "frames",
        "frames_per_sample",
        "id",
        "layer",
        "p50_ms",
        "p95_ms",
        "p99_ms",
        "peak_ms",
        "refresh_mode",
        "samples",
        "scenario",
        "unit",
        "variant",
    ];
    let web_cpu_submit_required_keys = [
        "cpu_submit_avg_ms",
        "cpu_submit_p50_ms",
        "cpu_submit_p95_ms",
        "cpu_submit_p99_ms",
        "cpu_submit_peak_ms",
        "frames",
        "frames_per_sample",
        "id",
        "layer",
        "refresh_mode",
        "samples",
        "scenario",
        "unit",
        "variant",
    ];
    let web_raf_required_keys = [
        "avg_ms",
        "cache_state",
        "canvas_css",
        "canvas_physical",
        "device_pixel_ratio",
        "frame_budget_120hz_ms",
        "frame_budget_60hz_ms",
        "frames",
        "gpu_ms_p50",
        "gpu_ms_p95",
        "gpu_ms_p99",
        "gpu_ms_peak",
        "hitch_frames_120hz",
        "hitch_frames_60hz",
        "hitch_ratio_120hz",
        "hitch_ratio_60hz",
        "id",
        "layer",
        "missed_frame_ratio_120hz",
        "missed_frame_ratio_60hz",
        "missed_frames_120hz",
        "missed_frames_60hz",
        "p50_ms",
        "p95_ms",
        "p99_ms",
        "peak_ms",
        "refresh_mode",
        "samples",
        "scenario",
        "submissions",
        "unit",
        "variant",
    ];
    let web = persisted_report_json("benchmarks/web/latest.json");
    assert_json_object_keys(&web, &web_report_keys);
    for case in report_cases(&web, "web latest")
    {
       if case["id"].as_str() == Some("web.wasm.webgpu.cpu_submit_throughput")
       {
          assert_json_object_has_keys(case, &web_cpu_submit_required_keys);
       }
       else if case["id"].as_str() == Some("web.wasm.webgpu.raf_frame_loop")
       {
          assert_json_object_has_keys(case, &web_raf_required_keys);
       }
       else
       {
          assert_json_object_has_keys(case, &web_case_required_keys);
       }
    }
}

#[test]
fn persisted_report_case_id_sets_are_frozen() {
    let workspace = persisted_report_json("benchmarks/workspace/latest.json");
    assert_report_case_id_set(&workspace, "workspace latest", 399, 0x0a3d9230959bfc6d);

    let oxide_device = persisted_report_json("benchmarks/oxide-device/latest.json");
    assert_report_case_id_set(&oxide_device, "oxide device latest", 23, 0x80168fb31ce042ff);

    let uikit_device = persisted_report_json("benchmarks/uikit-device/latest.json");
    assert_report_case_id_set(&uikit_device, "uikit device latest", 38, 0x753034922b773608);

    let web = persisted_report_json("benchmarks/web/latest.json");
    assert_report_case_id_set(&web, "web latest", 18, 0x9fc864e451bf9432);
}

#[test]
fn persisted_web_report_subobject_schemas_are_frozen() {
    let web = persisted_report_json("benchmarks/web/latest.json");
    let sections = [
        ("gpu_stage_attribution", 6, 0x4f8fd48b6e346353),
        ("browser_startup", 19, 0x443345197e367a7d),
        ("backend_path_coverage", 5, 0x158b99e985757594),
        ("command_family_summary", 15, 0x64b5032aacb66cbf),
        ("glyph_run_summary", 16, 0x2ddc23d0da721daa),
        ("layer_effects_summary", 20, 0x3e4dde1fe42c91d0),
        ("upload_summary", 11, 0x34860398c8ab5645),
        ("wasm_allocation_audit", 17, 0xeb32f3145ec864fc),
        ("warm_resource_churn", 45, 0x64e334f4ac8907c0),
        ("gpu_timestamp_stage_breakdown", 13, 0x65908b3e662b7aca),
    ];
    for (section, expected_count, expected_digest) in sections {
        assert_json_object_key_digest(&web[section], section, expected_count, expected_digest);
    }
}

#[test]
fn persisted_report_nested_key_sets_are_frozen()
{
   let workspace = persisted_report_json("benchmarks/workspace/latest.json");
   assert_json_object_key_digest(&workspace["coverage"], "workspace coverage", 32, 0x5ee0445752468f8d);
   assert_json_object_key_digest(&workspace["contract"], "workspace contract", 3, 0x0796508e10525921);
   assert_json_array_entry_key_digest(&workspace["contract"]["battery"], "workspace contract battery", 4, 0x0ab7b204885807d9);
   assert_json_array_entry_key_digest(&workspace["contract"]["layers"], "workspace contract layers", 4, 0x0ab7b204885807d9);
   assert_json_array_entry_key_digest(&workspace["findings"], "workspace findings", 2, 0x4c30c261b26d2ea9);

   let oxide_device = persisted_report_json("benchmarks/oxide-device/latest.json");
   assert_json_object_key_digest(&oxide_device["coverage"], "oxide device coverage", 32, 0x5ee0445752468f8d);
   assert_json_object_key_digest(&oxide_device["contract"], "oxide device contract", 3, 0x0796508e10525921);
   assert_json_array_entry_key_digest(&oxide_device["contract"]["battery"], "oxide device contract battery", 4, 0x0ab7b204885807d9);
   assert_json_array_entry_key_digest(&oxide_device["contract"]["layers"], "oxide device contract layers", 4, 0x0ab7b204885807d9);
   assert_json_array_entry_key_digest(&oxide_device["findings"], "oxide device findings", 2, 0x4c30c261b26d2ea9);
   assert_report_metric_key_class_digest(&oxide_device, "oxide device latest", 23, 0x6c582bb7208d4962);

   let uikit_device = persisted_report_json("benchmarks/uikit-device/latest.json");
   assert_json_object_key_digest(&uikit_device["contract"], "uikit device contract", 4, 0x92feb47c0d2e7b8b);
   assert_json_array_entry_key_digest(&uikit_device["contract"]["battery"], "uikit device contract battery", 4, 0x0ab7b204885807d9);
   assert_json_array_entry_key_digest(&uikit_device["contract"]["layers"], "uikit device contract layers", 4, 0x0ab7b204885807d9);
   assert_json_array_entry_key_digest(&uikit_device["contract"]["styles"], "uikit device contract styles", 4, 0x0ab7b204885807d9);
   assert_report_metric_key_class_digest(&uikit_device, "uikit device latest", 38, 0xebd1e83cc68ec4de);

   let web = persisted_report_json("benchmarks/web/latest.json");
   let web_sections = [
      ("backdrop_batch_summary", 9, 0x2b284758e777084d),
      ("benchmark_marks", 16, 0x83af02e7e29ee89d),
      ("browser_startup", 19, 0x443345197e367a7d),
      ("browser_trace", 19, 0x806c71bca480cfee),
      ("clean_layer_summary", 16, 0x3159d6b63d3db3f9),
      ("direct_surface_summary", 13, 0x7a7f8409acbfe0f0),
      ("effect_uniform_summary", 11, 0x6902916d38ce7681),
      ("frame_loop_wasm_allocation_stages", 15, 0x3a59951e69ac9007),
      ("frame_loop_wasm_submit_allocation_stages", 15, 0xd29637cda982c8f4),
      ("id_mask_summary", 9, 0x67e29d57f1316766),
      ("mixed_summary", 16, 0xbc967663981e0eb8),
      ("neon_marker_summary", 11, 0x7c34a247e845acd5),
      ("pixel_check", 7, 0x7654c15d62e216ec),
      ("scene3d_stress_summary", 23, 0x63a387ee9ca1bd4d),
      ("scene3d_summary", 22, 0x7bc41f1ab93ecf65),
      ("smoke", 21, 0x3c3ab7a93b727e37),
      ("wasm_allocation_invariance", 12, 0x25a587bf2e40a76f),
   ];
   for (section, expected_count, expected_digest) in web_sections
   {
      assert_json_object_key_digest(&web[section], section, expected_count, expected_digest);
   }
   assert_json_array_entry_key_digest(&web["browser_startup"]["files"], "web browser package files", 3, 0x16e32dc2a4132de1);
   assert_report_case_key_class_digest(&web, "web latest", 18, 0xd0842a5f22773fe7);
}

#[test]
fn persisted_workspace_native_renderer_metric_keys_are_frozen() {
    let report = workspace_latest_report();
    assert_workspace_case_metric_key_digest(
        &report,
        "gpu.system.id_mask_compositor.current",
        24,
        0x6d1f4edb402039fa,
    );
    assert_workspace_case_metric_key_digest(
        &report,
        "gpu.animation.effects.refresh_matrix",
        32,
        0x12223d95b0c97df8,
    );
    assert_workspace_case_metric_key_digest(
        &report,
        "gpu.journey.collection_navigation.frame_pacing",
        34,
        0x6c24fadfa02d6f63,
    );
    assert_workspace_case_metric_key_digest(
        &report,
        "gpu.authoring.scene3d.mixed_frame",
        22,
        0xf685d05bf68a0cc2,
    );
    assert_workspace_case_metric_key_digest(
        &report,
        "gpu.image_pipeline.png.first_visible",
        22,
        0x82cb16697b8606dd,
    );

    let scene_rows = [
        "gpu.scene.controls.frame",
        "gpu.scene.text_layout.frame",
        "gpu.scene.zoom_image.frame",
        "gpu.scene.anim_timeline.frame",
        "gpu.scene.collection.frame",
        "gpu.scene.damage_lab.frame",
        "gpu.scene.input_lab.frame",
        "gpu.scene.nine_slice.frame",
        "gpu.scene.sdf_text.frame",
        "gpu.scene.snapshot.frame",
        "gpu.scene.camera.frame",
        "gpu.scene.elements_extended.frame",
        "gpu.scene.animation_config.frame",
        "gpu.scene.orchestration.frame",
        "gpu.scene.permissions.frame",
        "gpu.scene.integration.frame",
        "gpu.scene.stress.frame",
    ];
    for id in scene_rows {
        assert_workspace_case_metric_key_digest(&report, id, 31, 0xf2dee20e9220b171);
    }
}

#[test]
fn perf_runner_docs_define_schema_versioning_rules()
{
   let docs = include_str!("../../../docs/perf-runner/lib.md");
   let section = docs
      .split("### Schema versioning rules")
      .nth(1)
      .unwrap_or_else(|| panic!("perf-runner docs must define schema versioning rules"));
   let next_section = section.find("\n## ").unwrap_or(section.len());
   let rules = &section[..next_section];

   for required in [
      "PerfReport.version",
      "browser WebGPU report `version`",
      "Oxide-device report `version`",
      "UIKit-device report `version`",
      "top-level JSON object adds, removes, renames",
      "Common case-row fields",
      "Benchmark IDs are semantic workload identifiers",
      "Reusing an existing ID for a different workload is forbidden",
      "Adding or retiring IDs requires refreshed persisted baselines",
      "Metric keys inside `metrics` may grow compatibly",
      "Renaming, deleting, reuniting, moving, or redefining a metric key requires a report version bump",
      "Browser WebGPU summary sections",
      "backend-path coverage rows",
      "allocation-stage objects",
      "timestamp attribution sections",
      "Oxide-device and UIKit-device rows",
      "Native ABI evidence layouts use ABI struct version/size fields",
      "layout, alignment, field order, ownership, or callback-payload semantic change",
      "Legacy-row retirement is a benchmark matrix hard cutover",
      "same-workload A/B evidence proves the retained path is faster",
      "Historical dated reports and CI snapshots may remain readable under their original version",
   ]
   {
      assert!(rules.contains(required), "schema versioning docs missing `{required}`");
   }
}

#[test]
fn perf_runner_docs_describe_markdown_render_bench()
{
   let docs = include_str!("../../../docs/perf-runner/lib.md");
   for required in [
      "--bench-markdown-render PATH",
      "--bench-markdown-write PATH",
      "--bench-markdown-compare PATH",
      "--bench-markdown-iters N",
      "loads an existing `PerfReport` JSON",
      "comparison baseline",
      "same-workload A/B proof",
      "latest-plus-dated Markdown output path",
      "report-generation changes",
   ]
   {
      assert!(docs.contains(required), "markdown render bench docs missing `{required}`");
   }
}

#[test]
fn perf_runner_docs_describe_json_render_bench()
{
   let docs = include_str!("../../../docs/perf-runner/lib.md");
   for required in [
      "--bench-json-render PATH",
      "--bench-json-string-render PATH",
      "--bench-json-iters N",
      "pretty JSON serialization",
      "shared pre-sized pretty JSON serializer",
      "String-return pretty JSON serializer",
      "host-facing JSON export changes",
      "persisted JSON write path",
      "capacity hint",
      "to_writer_pretty",
      "same-workload A/B proof",
      "artifact-density",
      "without rerunning the suite workloads",
   ]
   {
      assert!(docs.contains(required), "json render bench docs missing `{required}`");
   }
}

#[test]
fn perf_runner_docs_describe_sample_summary_bench()
{
   let docs = include_str!("../../../docs/perf-runner/lib.md");
   for required in [
      "--bench-sample-summary",
      "--bench-sample-summary-iters N",
      "representative 6-, 10-, 12-, and 24-sample slices",
      "sample-summary allocation changes",
      "summary-allocation and quantile changes",
      "summary counts plus a deterministic checksum",
      "fixed stack buffer",
      "sort_unstable_by",
      "fixed 6-, 10-, 12-, and 24-sample summaries use direct interpolation",
      "min/max/mean and p50/p95/p99 checksums",
   ]
   {
      assert!(docs.contains(required), "sample summary bench docs missing `{required}`");
   }
}

#[test]
fn perf_runner_docs_describe_case_filter_bench()
{
   let docs = include_str!("../../../docs/perf-runner/lib.md");
   for required in [
      "--bench-case-filter",
      "--bench-case-filter-iters N",
      "repeatedly checks representative case IDs and family prefixes",
      "case-selection changes",
   ]
   {
      assert!(docs.contains(required), "case filter bench docs missing `{required}`");
   }
}

#[test]
fn perf_runner_docs_describe_frame_pacing_metrics_bench()
{
   let docs = include_str!("../../../docs/perf-runner/lib.md");
   for required in [
      "--bench-frame-pacing-metrics",
      "--bench-frame-pacing-iters N",
      "repeatedly inserts frame-pacing metrics",
      "metric counts plus a deterministic checksum",
      "missed-frame and hitch metric insertion changes",
      "static metric key strings",
      "nonstandard refresh tiers keep the formatted fallback",
   ]
   {
      assert!(docs.contains(required), "frame pacing metrics bench docs missing `{required}`");
   }
}

#[test]
fn perf_runner_docs_describe_distribution_metrics_bench()
{
   let docs = include_str!("../../../docs/perf-runner/lib.md");
   for required in [
      "--bench-distribution-metrics",
      "--bench-distribution-iters N",
      "representative `frame_ms`, `event_to_visible_ms`, and `gpu_ms` distribution metrics",
      "metric counts plus a deterministic checksum",
      "distribution metric-key changes",
      "static metric key strings",
      "uncommon prefixes keep the formatted fallback",
      "distribution-only summary",
      "skips unused min and mean work",
   ]
   {
      assert!(docs.contains(required), "distribution metrics bench docs missing `{required}`");
   }
}

#[test]
fn perf_runner_docs_describe_case_metric_contract_bench()
{
   let docs = include_str!("../../../docs/perf-runner/lib.md");
   for required in [
      "--bench-case-metric-contract PATH",
      "--bench-case-metric-iters N",
      "repeatedly validates the case metric contract",
      "case/required counts plus a deterministic checksum",
      "required-key validation changes",
      "static required-key arrays",
      "validation-key formatting",
   ]
   {
      assert!(docs.contains(required), "case metric contract bench docs missing `{required}`");
   }
}

#[test]
fn perf_runner_docs_describe_contract_coverage_bench()
{
   let docs = include_str!("../../../docs/perf-runner/lib.md");
   for required in [
      "--bench-contract-coverage PATH",
      "--bench-contract-iters N",
      "repeatedly validates contract coverage",
      "layer/battery/note counts plus a deterministic checksum",
      "contract coverage validation changes",
      "allocation-free ASCII phrase scan",
      "gap-note validation",
      "first-byte ASCII fold",
      "per-byte uppercase branch",
      "tail-only ASCII phrase",
      "redundant first-byte comparison",
   ]
   {
      assert!(docs.contains(required), "contract coverage bench docs missing `{required}`");
   }
}

#[test]
fn perf_runner_docs_describe_compare_reports_bench()
{
   let docs = include_str!("../../../docs/perf-runner/lib.md");
   for required in [
      "--bench-compare-reports CURRENT BASELINE",
      "--bench-compare-iters N",
      "repeatedly runs `compare_reports`",
      "deterministic checksum",
      "lookup-table changes",
      "32 or fewer cases",
      "larger baselines",
      "same ordered case IDs",
      "missing-baseline vector",
   ]
   {
      assert!(docs.contains(required), "compare reports bench docs missing `{required}`");
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
    assert_workspace_cpu_row(wrapped, "system", "system");
    assert_eq!(workspace_metric(wrapped, "wrapped_label_variants"), 4096.0);
    assert!(workspace_metric(wrapped, "wrapped_label_glyph_runs") > 0.0);
    assert!(workspace_metric(wrapped, "wrapped_label_vertices") > 0.0);
    assert!(workspace_metric(wrapped, "dirty_to_full_upload_ratio") < 0.01);
    assert!(workspace_missing_case(&report, "cpu.system.wrapped_label_legacy_fit_shape"));

    let picker = workspace_case(&report, "cpu.system.picker_text_cached_encode");
    assert_workspace_cpu_row(picker, "system", "system");
    assert_eq!(workspace_metric(picker, "atlas_create_calls"), 1.0);
    assert_eq!(workspace_metric(picker, "atlas_update_calls"), 0.0);
    assert!(workspace_metric(picker, "picker_glyph_runs") > 0.0);
    assert!(workspace_metric(picker, "picker_vertices") > 0.0);
    assert!(workspace_metric(picker, "dirty_to_full_upload_ratio") < 0.01);
    assert!(workspace_missing_case(&report, "cpu.system.picker_text_legacy_shape_upload"));

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
fn web_latest_report_satisfies_webgpu_distribution_and_pacing_contract() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/web/latest.json");
    let bytes = std::fs::read(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    let report: Value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|err| panic!("parse {}: {err}", path.display()));

    assert_eq!(report["version"].as_u64(), Some(6));
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
    assert_eq!(
        report["browser_startup"]["id"].as_str(),
        Some("web.wasm.webgpu.browser_startup"),
    );
    assert_eq!(
        report["browser_startup"]["source"].as_str(),
        Some("performance.now+node.fs.stat"),
    );
    assert!(web_report_number(&report["browser_startup"], "wasm_init_ms") > 0.0);
    assert!(web_report_number(&report["browser_startup"], "app_init_ms") > 0.0);
    assert!(web_report_number(&report["browser_startup"], "report_ready_ms") > 0.0);
    assert!(web_report_number(&report["browser_startup"], "wasm_memory_bytes") > 0.0);
    assert_eq!(web_report_number(&report["browser_startup"], "package_file_count"), 4.0);
    assert!(web_report_number(&report["browser_startup"], "package_bytes") > 0.0);
    assert!(web_report_number(&report["browser_startup"], "wasm_bytes") > 0.0);
    assert!(web_report_number(&report["browser_startup"], "js_bytes") > 0.0);
    assert_eq!(
        report["browser_startup"]["files"]
            .as_array()
            .expect("browser startup package files")
            .len(),
        4,
    );

    let expected_benchmark_marks = [
        "cpu_submit_throughput",
        "raf_frame_loop",
        "id_mask_current",
        "upload_current",
        "effect_uniform_ab",
        "backdrop_batch_current",
        "scene3d_ab",
        "mixed_matrix",
        "layer_effects_matrix",
        "clean_layer_ab",
        "command_family_matrix",
        "glyph_run_current",
        "neon_marker_ab",
        "direct_surface_ab",
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
        "web.wasm.webgpu.id_mask_compositor.current",
        "web.wasm.webgpu.glyph_atlas_upload.current_dirty",
        "web.wasm.webgpu.image_upload.current_dirty",
        "web.wasm.webgpu.effect_uniform.current_batched",
        "web.wasm.webgpu.backdrop_batch.current_coalesced",
        "web.wasm.webgpu.scene3d.reused_mesh",
        "web.wasm.webgpu.scene3d.recreate_mesh",
        "web.wasm.webgpu.scene3d.stress_reused_mesh",
        "web.wasm.webgpu.scene3d.stress_recreate_mesh",
        "web.wasm.webgpu.mixed_text_image_effects",
        "web.wasm.webgpu.layer_damage_effects",
        "web.wasm.webgpu.clean_layer.clean_reuse",
        "web.wasm.webgpu.command_family_matrix",
        "web.wasm.webgpu.glyph_run.current",
        "web.wasm.webgpu.neon_marker.current",
        "web.wasm.webgpu.direct_surface.current",
    ];
    assert_eq!(
        report["cases"].as_array().expect("web cases").len(),
        expected_ids.len() + 2,
        "unexpected WebGPU browser report case count",
    );
    let cpu_submit = web_report_case(&report, "web.wasm.webgpu.cpu_submit_throughput");
    assert!(web_report_number(cpu_submit, "cpu_submit_p50_ms") > 0.0);
    assert!(web_report_number(cpu_submit, "cpu_submit_p95_ms") > 0.0);
    assert!(web_report_number(cpu_submit, "cpu_submit_p99_ms") > 0.0);
    assert!(web_report_number(cpu_submit, "cpu_submit_peak_ms") > 0.0);
    let raf = web_report_case(&report, "web.wasm.webgpu.raf_frame_loop");
    assert_eq!(raf["unit"].as_str(), Some("ms/displayed-frame"));
    assert_eq!(web_report_number(raf, "samples"), 2_000.0);
    assert_eq!(web_report_number(raf, "frames"), 2_000.0);
    assert_eq!(web_report_number(raf, "submissions"), 2_000.0);
    assert_eq!(
        raf["gpu_timestamp_samples"].as_array().expect("RAF GPU timestamp samples").len(),
        2_000,
    );
    assert_eq!(web_report_number(raf, "queue_pending_final"), 0.0);
    assert!(web_report_number(raf, "p50_ms") > 0.0);
    assert!(web_report_number(raf, "p95_ms") >= web_report_number(raf, "p50_ms"));
    assert!(web_report_number(raf, "p99_ms") >= web_report_number(raf, "p95_ms"));
    assert!(web_report_number(raf, "peak_ms") >= web_report_number(raf, "p99_ms"));
    for hz in ["60hz", "120hz"] {
        assert!(web_report_number(raf, &format!("frame_budget_{hz}_ms")) > 0.0);
        assert!(web_report_number(raf, &format!("missed_frames_{hz}")) >= 0.0);
        assert!(web_report_number(raf, &format!("hitch_frames_{hz}")) >= 0.0);
        let missed = web_report_number(raf, &format!("missed_frame_ratio_{hz}"));
        let hitch = web_report_number(raf, &format!("hitch_ratio_{hz}"));
        assert!((0.0..=1.0).contains(&missed), "missed ratio {hz} out of range: {missed}");
        assert!((0.0..=1.0).contains(&hitch), "hitch ratio {hz} out of range: {hitch}");
    }
    for id in expected_ids {
        assert_web_renderer_case_contract(web_report_case(&report, id));
    }

    let gpu_timestamp_stage_breakdown = &report["gpu_timestamp_stage_breakdown"];
    assert_eq!(
        gpu_timestamp_stage_breakdown["id"].as_str(),
        Some("web.wasm.webgpu.gpu_timestamp_stage_breakdown"),
    );
    assert_eq!(web_report_number(gpu_timestamp_stage_breakdown, "row_count"), 17.0);
    assert_eq!(web_report_number(gpu_timestamp_stage_breakdown, "collected_rows"), 17.0);
    assert_eq!(web_report_number(gpu_timestamp_stage_breakdown, "stage_count"), 9.0);
    assert_eq!(web_report_number(gpu_timestamp_stage_breakdown, "row_detail_count"), 17.0);
    assert_eq!(web_report_number(gpu_timestamp_stage_breakdown, "total_render_passes"), 82.0);
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
        .find(|row| row["id"].as_str() == Some("web.wasm.webgpu.cpu_submit_throughput"))
        .expect("CPU-submit gpu timestamp detail");
    assert_eq!(
        web_report_number(frame_loop_timestamp_row, "family_passes"),
        web_report_number(cpu_submit, "render_passes"),
    );
    assert_eq!(
        web_report_number(frame_loop_timestamp_row, "family_timestamp_ns"),
        web_report_number(
            cpu_submit,
            "gpu_timestamp_total_ns"
        ),
    );

    let warm_resource_churn = &report["warm_resource_churn"];
    assert_eq!(
        warm_resource_churn["id"].as_str(),
        Some("web.wasm.webgpu.warm_resource_churn.current_rows"),
    );
    assert_eq!(web_report_number(warm_resource_churn, "checked_rows"), 15.0);
    assert_eq!(web_report_number(warm_resource_churn, "excluded_rows"), 3.0);
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
        "web.wasm.webgpu.cpu_submit_throughput",
        "web.wasm.webgpu.id_mask_compositor.current",
        "web.wasm.webgpu.glyph_atlas_upload.current_dirty",
        "web.wasm.webgpu.image_upload.current_dirty",
        "web.wasm.webgpu.effect_uniform.current_batched",
        "web.wasm.webgpu.backdrop_batch.current_coalesced",
        "web.wasm.webgpu.scene3d.reused_mesh",
        "web.wasm.webgpu.scene3d.stress_reused_mesh",
        "web.wasm.webgpu.mixed_text_image_effects",
        "web.wasm.webgpu.layer_damage_effects",
        "web.wasm.webgpu.clean_layer.clean_reuse",
        "web.wasm.webgpu.command_family_matrix",
        "web.wasm.webgpu.glyph_run.current",
        "web.wasm.webgpu.neon_marker.current",
        "web.wasm.webgpu.direct_surface.current",
    ] {
        assert!(warm_rows.contains(&id), "warm resource churn missing checked row {id}");
   }
   for id in [
       "web.wasm.webgpu.raf_frame_loop",
       "web.wasm.webgpu.scene3d.recreate_mesh",
       "web.wasm.webgpu.scene3d.stress_recreate_mesh",
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
   assert_eq!(web_report_number(wasm_allocation_audit, "checked_count"), 15.0);
   assert_eq!(web_report_number(wasm_allocation_audit, "excluded_count"), 3.0);
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
    assert!(wasm_allocation_rows.contains(&"web.wasm.webgpu.cpu_submit_throughput"));
    assert!(wasm_allocation_rows.contains(&"web.wasm.webgpu.id_mask_compositor.current"));
    assert!(wasm_allocation_rows.contains(&"web.wasm.webgpu.glyph_run.current"));
    assert!(wasm_allocation_rows.contains(&"web.wasm.webgpu.neon_marker.current"));
    assert!(wasm_allocation_rows.contains(&"web.wasm.webgpu.direct_surface.current"));
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

    let frame_loop = web_report_case(&report, "web.wasm.webgpu.cpu_submit_throughput");
    let wasm_allocation_invariance = &report["wasm_allocation_invariance"];
    assert_eq!(
        wasm_allocation_invariance["id"].as_str(),
        Some("web.wasm.webgpu.wasm_allocation_invariance.current_rows"),
    );
    assert_eq!(
        wasm_allocation_invariance["status"].as_str(),
        Some("path-specific-allocations"),
    );
    assert_eq!(
        wasm_allocation_invariance["reference_row"].as_str(),
        Some("web.wasm.webgpu.cpu_submit_throughput"),
    );
    assert_eq!(
        web_report_number(wasm_allocation_invariance, "checked_count"),
        web_report_number(wasm_allocation_audit, "checked_count"),
    );
    assert_eq!(web_report_number(wasm_allocation_invariance, "unique_signature_count"), 6.0,);
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
        6,
    );

    let frame_stage_allocations = &report["frame_loop_wasm_allocation_stages"];
    assert_eq!(
        frame_stage_allocations["id"].as_str(),
        Some("web.wasm.webgpu.frame_loop_wasm_allocation_stages"),
    );
    assert_eq!(
        frame_stage_allocations["row_id"].as_str(),
        Some("web.wasm.webgpu.cpu_submit_throughput"),
    );
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
    assert_eq!(
        submit_stage_allocations["row_id"].as_str(),
        Some("web.wasm.webgpu.cpu_submit_throughput"),
    );
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
        "submit_timestamp_map_alloc_count",
    ] {
        assert_eq!(web_report_number(frame_loop, field), 0.0);
    }
    assert!(web_report_number(frame_loop, "submit_surface_alloc_count") > 0.0);
    assert!(web_report_number(frame_loop, "submit_finish_queue_alloc_count") > 0.0);
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
            "cpu_submit_throughput",
            &["web.wasm.webgpu.cpu_submit_throughput"],
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
            "raf_frame_loop",
            &["web.wasm.webgpu.raf_frame_loop"],
            &["frames", "submissions", "p50_ms", "p95_ms", "p99_ms", "peak_ms"],
        ),
        (
            "id_mask_compositor",
            &["web.wasm.webgpu.id_mask_compositor.current"],
            &[
                "id_mask_draws",
                "id_mask_uniform_writes",
                "id_mask_uniform_bytes",
                "id_mask_uniform_slots",
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
            &["web.wasm.webgpu.glyph_atlas_upload.current_dirty"],
            &["glyph_quads", "texture_upload_bytes", "buffer_upload_bytes", "gpu_timestamp_passes"],
        ),
        (
            "image_upload",
            &["web.wasm.webgpu.image_upload.current_dirty"],
            &["image_draws", "texture_upload_bytes", "buffer_upload_bytes", "gpu_timestamp_passes"],
        ),
        (
            "effect_uniform",
            &["web.wasm.webgpu.effect_uniform.current_batched"],
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
            &["web.wasm.webgpu.backdrop_batch.current_coalesced"],
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
                "scene3d_instances",
                "scene3d_instance_bytes",
                "scene3d_pipeline_binds",
                "scene3d_bind_group_binds",
                "scene3d_mesh_buffer_binds",
                "scene3d_viewport_sets",
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
                "scene3d_instances",
                "scene3d_instance_bytes",
                "scene3d_pipeline_binds",
                "scene3d_bind_group_binds",
                "scene3d_mesh_buffer_binds",
                "scene3d_viewport_sets",
                "mesh3d_creates",
                "buffer_grows",
                "cpu_scratch_grows",
                "gpu_timestamp_passes",
            ],
        ),
        (
            "mixed_text_image_effects",
            &["web.wasm.webgpu.mixed_text_image_effects"],
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
            &["web.wasm.webgpu.layer_damage_effects"],
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
            &["web.wasm.webgpu.clean_layer.clean_reuse"],
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
            &["web.wasm.webgpu.glyph_run.current"],
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
            &["web.wasm.webgpu.neon_marker.current"],
            &[
                "expected_markers",
                "expected_draw_items",
                "draw_items",
                "neon_marker_instances",
                "neon_marker_triangles",
                "neon_marker_instance_bytes",
                "draw_pipeline_binds",
                "draw_bind_group_binds",
                "draw_scissor_sets",
                "gpu_timestamp_passes",
            ],
        ),
        (
            "direct_surface",
            &["web.wasm.webgpu.direct_surface.current"],
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
                let source_field = if *row_id == "web.wasm.webgpu.cpu_submit_throughput"
                {
                   format!("cpu_submit_{field}")
                }
                else
                {
                   String::from(field)
                };
                assert_eq!(
                    web_report_number(detail, field),
                    web_report_number(source, &source_field),
                );
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

    let frame = web_report_case(&report, "web.wasm.webgpu.cpu_submit_throughput");
    let current = web_report_case(&report, "web.wasm.webgpu.id_mask_compositor.current");
    let glyph_current =
        web_report_case(&report, "web.wasm.webgpu.glyph_atlas_upload.current_dirty");
    let image_current = web_report_case(&report, "web.wasm.webgpu.image_upload.current_dirty");
    let effect_current = web_report_case(&report, "web.wasm.webgpu.effect_uniform.current_batched");
    assert!(web_report_case_optional(
        &report,
        "web.wasm.webgpu.effect_uniform.legacy_write_each",
    )
    .is_none());
    let backdrop_batch_current =
        web_report_case(&report, "web.wasm.webgpu.backdrop_batch.current_coalesced");
    let scene3d_reused = web_report_case(&report, "web.wasm.webgpu.scene3d.reused_mesh");
    let scene3d_recreate = web_report_case(&report, "web.wasm.webgpu.scene3d.recreate_mesh");
    let scene3d_stress_reused =
        web_report_case(&report, "web.wasm.webgpu.scene3d.stress_reused_mesh");
    let scene3d_stress_recreate =
        web_report_case(&report, "web.wasm.webgpu.scene3d.stress_recreate_mesh");
    let mixed = web_report_case(&report, "web.wasm.webgpu.mixed_text_image_effects");
    assert!(web_report_case_optional(
        &report,
        "web.wasm.webgpu.mixed_text_image_effects.legacy_rebind_unbatched",
    )
    .is_none());
    let layer_effects = web_report_case(&report, "web.wasm.webgpu.layer_damage_effects");
    let clean_layer = web_report_case(&report, "web.wasm.webgpu.clean_layer.clean_reuse");
    assert!(web_report_case_optional(
        &report,
        "web.wasm.webgpu.clean_layer.dirty_rerender",
    )
    .is_none());
    let command_family = web_report_case(&report, "web.wasm.webgpu.command_family_matrix");
    let glyph_run_current = web_report_case(&report, "web.wasm.webgpu.glyph_run.current");
    let neon_marker_current = web_report_case(&report, "web.wasm.webgpu.neon_marker.current");
    assert!(web_report_case_optional(
        &report,
        "web.wasm.webgpu.neon_marker.legacy_rebind",
    )
    .is_none());
    let direct_surface_current = web_report_case(&report, "web.wasm.webgpu.direct_surface.current");
    assert!(web_report_case_optional(
        &report,
        "web.wasm.webgpu.direct_surface.legacy_scene_present",
    )
    .is_none());

    for case in [
        frame,
        current,
        glyph_current,
        image_current,
        effect_current,
        backdrop_batch_current,
        scene3d_reused,
        scene3d_stress_reused,
        mixed,
        layer_effects,
        clean_layer,
        command_family,
        glyph_run_current,
        neon_marker_current,
        direct_surface_current,
    ] {
        assert_web_report_zero_resource_churn(case, false);
    }
    for case in [scene3d_recreate, scene3d_stress_recreate] {
        assert_web_report_zero_resource_churn(case, true);
        assert!(web_report_number(case, "mesh3d_creates") > 0.0);
    }

    for case in [current] {
        assert!(web_report_number(case, "vertices") > 0.0);
        assert!(web_report_number(case, "vertex_bytes") > 0.0);
        assert!(web_report_number(case, "id_mask_draws") > 0.0);
        assert!(web_report_number(case, "id_mask_uniform_writes") > 0.0);
        assert!(web_report_number(case, "id_mask_uniform_bytes") > 0.0);
        assert!(web_report_number(case, "id_mask_uniform_slots") > 0.0);
        assert_eq!(web_report_number(case, "id_mask_raster_passes"), 0.0);
        assert_eq!(web_report_number(case, "id_mask_field_seed_passes"), 0.0);
        assert_eq!(web_report_number(case, "id_mask_field_jump_passes"), 0.0);
        assert!(web_report_number(case, "id_mask_compositor_passes") > 0.0);
    }
    assert!(web_report_number(frame, "draw_items") > 0.0);
    assert!(web_report_number(frame, "draw_passes") > 0.0);
    assert!(web_report_number(frame, "glyph_quads") > 0.0);
    assert!(web_report_number(glyph_current, "glyph_quads") > 0.0);
    assert_eq!(
        web_report_number(glyph_current, "gpu_timestamp_passes"),
        web_report_number(glyph_current, "render_passes"),
    );
    assert!(web_report_number(glyph_current, "gpu_timestamp_total_ns") > 0.0);
    assert!(web_report_number(image_current, "image_draws") > 0.0);
    assert_eq!(
        web_report_number(image_current, "gpu_timestamp_passes"),
        web_report_number(image_current, "render_passes"),
    );
    assert!(web_report_number(image_current, "gpu_timestamp_total_ns") > 0.0);
    assert!(
        web_report_number(effect_current, "backdrop_draws")
            >= web_report_number(effect_current, "expected_backdrops")
    );
    assert_eq!(web_report_number(effect_current, "effect_uniform_writes"), 1.0);
    assert!(web_report_number(effect_current, "effect_uniform_bytes") > 0.0);
    assert_eq!(
        web_report_number(effect_current, "effect_uniform_slots"),
        web_report_number(effect_current, "expected_backdrops"),
    );
    assert_eq!(
        web_report_number(effect_current, "gpu_timestamp_passes"),
        web_report_number(effect_current, "render_passes"),
    );
    assert!(web_report_number(effect_current, "gpu_timestamp_total_ns") > 0.0);
    assert!(
        web_report_number(backdrop_batch_current, "backdrop_draws")
            >= web_report_number(backdrop_batch_current, "expected_backdrops")
    );
    assert_eq!(web_report_number(backdrop_batch_current, "effect_uniform_writes"), 1.0);
    assert_eq!(
        web_report_number(backdrop_batch_current, "effect_uniform_slots"),
        web_report_number(backdrop_batch_current, "expected_backdrops"),
    );
    assert_eq!(web_report_number(backdrop_batch_current, "texture_copies"), 1.0);
    assert_eq!(web_report_number(backdrop_batch_current, "render_passes"), 3.0);
    assert_eq!(
        web_report_number(backdrop_batch_current, "gpu_timestamp_passes"),
        web_report_number(backdrop_batch_current, "render_passes"),
    );
    assert_eq!(web_report_number(scene3d_reused, "mesh3d_creates"), 0.0);
    assert!(web_report_number(scene3d_reused, "scene3d_draws") > 0.0);
    assert!(web_report_number(scene3d_reused, "scene3d_passes") > 0.0);
    assert_eq!(web_report_number(scene3d_stress_reused, "mesh3d_creates"), 0.0);
    assert!(web_report_number(scene3d_stress_reused, "scene3d_draws") > 0.0);
    assert!(web_report_number(scene3d_stress_recreate, "scene3d_draws") > 0.0);
    assert!(web_report_number(scene3d_stress_reused, "scene3d_instances") >= 64.0);
    assert!(web_report_number(scene3d_stress_recreate, "scene3d_instances") >= 64.0);
    assert!(web_report_number(mixed, "backdrop_draws") > 0.0);
    assert!(web_report_number(mixed, "visual_effect_draws") > 0.0);
    assert!(web_report_number(mixed, "layer_draws") > 0.0);
    assert!(web_report_number(mixed, "clip_depth_peak") > 0.0);
    assert!(web_report_number(mixed, "damage_rects") > 0.0);
    assert!(web_report_number(mixed, "texture_copies") > 0.0);
    assert!(web_report_number(mixed, "image_draws") >= web_report_number(mixed, "image_tiles"));
    assert!(web_report_number(mixed, "draw_pipeline_binds") > 0.0);
    assert!(web_report_number(mixed, "draw_bind_group_binds") > 0.0);
    assert!(web_report_number(mixed, "draw_scissor_sets") > 0.0);
    assert!(web_report_number(mixed, "effect_uniform_writes") > 0.0);
    assert_eq!(
        web_report_number(mixed, "gpu_timestamp_passes"),
        web_report_number(mixed, "render_passes"),
    );
    assert_eq!(
        web_report_number(&report["mixed_summary"], "current_p50_ms"),
        web_report_number(mixed, "p50_ms"),
    );
    assert_eq!(
        web_report_number(&report["mixed_summary"], "current_draw_pipeline_binds"),
        web_report_number(mixed, "draw_pipeline_binds"),
    );
    assert_eq!(
        web_report_number(&report["mixed_summary"], "current_draw_bind_group_binds"),
        web_report_number(mixed, "draw_bind_group_binds"),
    );
    assert!(web_report_number(layer_effects, "image_draws") > 0.0);
    assert!(web_report_number(layer_effects, "glyph_quads") > 0.0);
    assert!(
        web_report_number(layer_effects, "layer_draws")
            >= web_report_number(layer_effects, "expected_layers")
    );
    assert!(
        web_report_number(layer_effects, "damage_rects")
            >= web_report_number(layer_effects, "expected_damage_rects")
    );
    assert!(web_report_number(layer_effects, "clip_depth_peak") > 0.0);
    assert!(web_report_number(layer_effects, "backdrop_draws") > 0.0);
    assert!(web_report_number(layer_effects, "visual_effect_draws") > 0.0);
    assert!(web_report_number(layer_effects, "spinner_draws") > 0.0);
    assert!(web_report_number(layer_effects, "texture_copies") > 0.0);
    assert!(
        web_report_number(layer_effects, "image_draws")
            >= web_report_number(layer_effects, "image_tiles")
    );
    assert!(web_report_number(layer_effects, "draw_pipeline_binds") > 0.0);
    assert!(web_report_number(layer_effects, "draw_bind_group_binds") > 0.0);
    assert!(web_report_number(layer_effects, "draw_scissor_sets") > 0.0);
    assert!(web_report_number(layer_effects, "effect_uniform_writes") > 0.0);
    assert!(web_report_number(layer_effects, "render_passes") > 0.0);
    assert_eq!(
        web_report_number(layer_effects, "gpu_timestamp_passes"),
        web_report_number(layer_effects, "render_passes"),
    );
    assert_eq!(
        web_report_number(&report["layer_effects_summary"], "current_p50_ms"),
        web_report_number(layer_effects, "p50_ms"),
    );
    assert_eq!(
        web_report_number(&report["layer_effects_summary"], "current_draw_pipeline_binds"),
        web_report_number(layer_effects, "draw_pipeline_binds"),
    );
    assert_eq!(web_report_number(clean_layer, "layer_cache_hits"), 1.0);
    assert_eq!(web_report_number(clean_layer, "layer_cache_misses"), 0.0);
    assert!(
        web_report_number(clean_layer, "layer_cache_skipped_draws")
            > web_report_number(clean_layer, "draw_items")
    );
    assert_eq!(web_report_number(clean_layer, "layer_passes"), 0.0);
    assert_eq!(
        web_report_number(clean_layer, "gpu_timestamp_passes"),
        web_report_number(clean_layer, "render_passes"),
    );
    assert_eq!(
        web_report_number(&report["clean_layer_summary"], "clean_p50_ms"),
        web_report_number(clean_layer, "p50_ms"),
    );
    assert_eq!(
        report["clean_layer_summary"]["id"].as_str(),
        Some("web.wasm.webgpu.clean_layer.clean_reuse"),
    );
    assert_eq!(
        web_report_number(&report["clean_layer_summary"], "clean_layer_cache_hits"),
        web_report_number(clean_layer, "layer_cache_hits"),
    );
    assert_eq!(
        web_report_number(&report["clean_layer_summary"], "clean_layer_cache_skipped_draws"),
        web_report_number(clean_layer, "layer_cache_skipped_draws"),
    );
    assert_eq!(
        web_report_number(&report["clean_layer_summary"], "clean_render_passes"),
        web_report_number(clean_layer, "render_passes"),
    );
    assert!(
        web_report_number(command_family, "image_mesh_draws")
            >= web_report_number(command_family, "expected_image_meshes")
    );
    assert!(
        web_report_number(command_family, "nine_slice_draws")
            >= web_report_number(command_family, "expected_nine_slices")
    );
    assert!(
        web_report_number(command_family, "sdf_glyph_quads")
            >= web_report_number(command_family, "expected_sdf_glyphs")
    );
    assert_eq!(web_report_number(command_family, "expected_camera_bg"), 0.0);
    assert_eq!(web_report_number(command_family, "camera_bg_draws"), 0.0);
    assert!(web_report_number(command_family, "image_draws") >= 10.0);
    assert_eq!(
        web_report_number(command_family, "gpu_timestamp_passes"),
        web_report_number(command_family, "render_passes"),
    );
    assert_eq!(
        web_report_number(&report["command_family_summary"], "current_p50_ms"),
        web_report_number(command_family, "p50_ms"),
    );
    assert_eq!(
        web_report_number(&report["command_family_summary"], "current_draw_items"),
        web_report_number(command_family, "draw_items"),
    );
    assert_eq!(
        web_report_number(&report["command_family_summary"], "current_draw_pipeline_binds"),
        web_report_number(command_family, "draw_pipeline_binds"),
    );
    assert_eq!(
        web_report_number(&report["command_family_summary"], "current_draw_bind_group_binds"),
        web_report_number(command_family, "draw_bind_group_binds"),
    );
    assert_eq!(
        web_report_number(&report["command_family_summary"], "current_draw_scissor_sets"),
        web_report_number(command_family, "draw_scissor_sets"),
    );
    assert_eq!(web_report_number(glyph_run_current, "expected_glyph_runs"), 64.0);
    assert_eq!(web_report_number(glyph_run_current, "expected_glyphs_per_run"), 8.0);
    assert_eq!(web_report_number(glyph_run_current, "expected_glyph_quads"), 512.0);
    assert_eq!(web_report_number(glyph_run_current, "expected_sdf_runs"), 32.0);
    assert_eq!(web_report_number(glyph_run_current, "expected_sdf_glyph_quads"), 256.0);
    assert_eq!(web_report_number(glyph_run_current, "expected_draw_items"), 3.0);
    assert_eq!(
        web_report_number(glyph_run_current, "draw_items"),
        web_report_number(glyph_run_current, "expected_draw_items"),
    );
    assert_eq!(
        web_report_number(glyph_run_current, "glyph_quads"),
        web_report_number(glyph_run_current, "expected_glyph_quads"),
    );
    assert_eq!(
        web_report_number(glyph_run_current, "sdf_glyph_quads"),
        web_report_number(glyph_run_current, "expected_sdf_glyph_quads"),
    );
    assert_eq!(
        web_report_number(glyph_run_current, "render_passes"),
        1.0,
    );
    assert_eq!(web_report_number(glyph_run_current, "draw_passes"), 1.0);
    assert!(web_report_number(glyph_run_current, "draw_pipeline_binds") > 0.0);
    assert!(web_report_number(glyph_run_current, "draw_bind_group_binds") > 0.0);
    assert!(web_report_number(glyph_run_current, "draw_scissor_sets") > 0.0);
    assert_eq!(
        web_report_number(glyph_run_current, "gpu_timestamp_passes"),
        web_report_number(glyph_run_current, "render_passes"),
    );
    assert_eq!(
        web_report_number(&report["glyph_run_summary"], "current_p50_ms"),
        web_report_number(glyph_run_current, "p50_ms"),
    );
    assert_eq!(
        web_report_number(&report["glyph_run_summary"], "current_draw_pipeline_binds"),
        web_report_number(glyph_run_current, "draw_pipeline_binds"),
    );
    assert_eq!(
        web_report_number(&report["glyph_run_summary"], "current_draw_items"),
        web_report_number(glyph_run_current, "draw_items"),
    );
    assert_eq!(web_report_number(neon_marker_current, "expected_markers"), 64.0);
    assert_eq!(web_report_number(neon_marker_current, "expected_draw_items"), 64.0);
    assert_eq!(
        web_report_number(neon_marker_current, "draw_items"),
        web_report_number(neon_marker_current, "expected_draw_items"),
    );
    assert_eq!(web_report_number(neon_marker_current, "solid_tris"), 0.0);
    assert_eq!(web_report_number(neon_marker_current, "neon_marker_instances"), 64.0);
    assert_eq!(web_report_number(neon_marker_current, "neon_marker_triangles"), 128.0);
    assert!(web_report_number(neon_marker_current, "neon_marker_instance_bytes") > 0.0);
    assert_eq!(web_report_number(neon_marker_current, "draw_pipeline_binds"), 1.0);
    assert_eq!(web_report_number(neon_marker_current, "draw_bind_group_binds"), 0.0);
    assert_eq!(web_report_number(neon_marker_current, "draw_scissor_sets"), 1.0);
    assert_eq!(
        web_report_number(neon_marker_current, "gpu_timestamp_passes"),
        web_report_number(neon_marker_current, "render_passes"),
    );
    assert_eq!(
        web_report_number(&report["neon_marker_summary"], "current_p50_ms"),
        web_report_number(neon_marker_current, "p50_ms"),
    );
    assert_eq!(
        report["neon_marker_summary"]["id"].as_str(),
        Some("web.wasm.webgpu.neon_marker.current"),
    );
    assert_eq!(
        web_report_number(&report["neon_marker_summary"], "current_draw_items"),
        web_report_number(neon_marker_current, "draw_items"),
    );
    assert_eq!(
        web_report_number(&report["neon_marker_summary"], "current_instances"),
        web_report_number(neon_marker_current, "neon_marker_instances"),
    );
    assert_eq!(
        web_report_number(&report["neon_marker_summary"], "current_triangles"),
        web_report_number(neon_marker_current, "neon_marker_triangles"),
    );
    assert_eq!(
        web_report_number(&report["neon_marker_summary"], "current_instance_bytes"),
        web_report_number(neon_marker_current, "neon_marker_instance_bytes"),
    );
    assert_eq!(
        web_report_number(&report["neon_marker_summary"], "current_draw_pipeline_binds"),
        web_report_number(neon_marker_current, "draw_pipeline_binds"),
    );
    assert_eq!(
        web_report_number(&report["neon_marker_summary"], "current_draw_bind_group_binds"),
        web_report_number(neon_marker_current, "draw_bind_group_binds"),
    );
    assert_eq!(
        web_report_number(&report["neon_marker_summary"], "current_draw_scissor_sets"),
        web_report_number(neon_marker_current, "draw_scissor_sets"),
    );
    assert_eq!(web_report_number(direct_surface_current, "expected_image_draws"), 384.0);
    assert_eq!(web_report_number(direct_surface_current, "expected_draw_items"), 385.0);
    assert_eq!(
        web_report_number(direct_surface_current, "draw_items"),
        web_report_number(direct_surface_current, "expected_draw_items"),
    );
    assert_eq!(
        web_report_number(direct_surface_current, "image_draws"),
        web_report_number(direct_surface_current, "expected_image_draws"),
    );
    assert_eq!(web_report_number(direct_surface_current, "draw_passes"), 1.0);
    assert_eq!(web_report_number(direct_surface_current, "clear_passes"), 0.0);
    assert_eq!(web_report_number(direct_surface_current, "present_passes"), 0.0);
    assert_eq!(web_report_number(direct_surface_current, "render_passes"), 1.0);
    assert_eq!(web_report_number(direct_surface_current, "texture_copies"), 0.0);
    assert!(web_report_number(direct_surface_current, "gpu_timestamp_total_ns") > 0.0);
    assert_eq!(
        web_report_number(direct_surface_current, "gpu_timestamp_passes"),
        web_report_number(direct_surface_current, "render_passes"),
    );
    assert_eq!(
        report["direct_surface_summary"]["id"].as_str(),
        Some("web.wasm.webgpu.direct_surface.current"),
    );
    assert_eq!(
        web_report_number(&report["direct_surface_summary"], "current_p50_ms"),
        web_report_number(direct_surface_current, "p50_ms"),
    );
    assert_eq!(
        web_report_number(&report["direct_surface_summary"], "current_draw_items"),
        web_report_number(direct_surface_current, "draw_items"),
    );
    assert_eq!(
        web_report_number(&report["direct_surface_summary"], "current_image_draws"),
        web_report_number(direct_surface_current, "image_draws"),
    );
    assert_eq!(
        web_report_number(&report["direct_surface_summary"], "current_render_passes"),
        web_report_number(direct_surface_current, "render_passes"),
    );
    assert_eq!(
        web_report_number(&report["direct_surface_summary"], "current_draw_passes"),
        web_report_number(direct_surface_current, "draw_passes"),
    );
    assert_eq!(
        web_report_number(&report["direct_surface_summary"], "current_texture_copies"),
        web_report_number(direct_surface_current, "texture_copies"),
    );
    assert_eq!(
        web_report_number(&report["direct_surface_summary"], "current_gpu_timestamp_total_ns"),
        web_report_number(direct_surface_current, "gpu_timestamp_total_ns"),
    );
    assert_eq!(
        web_report_number(&report["direct_surface_summary"], "current_gpu_timestamp_passes"),
        web_report_number(direct_surface_current, "gpu_timestamp_passes"),
    );
    assert_eq!(
        web_report_number(&report["direct_surface_summary"], "expected_draw_items"),
        web_report_number(direct_surface_current, "expected_draw_items"),
    );
    assert_eq!(
        web_report_number(&report["direct_surface_summary"], "expected_image_draws"),
        web_report_number(direct_surface_current, "expected_image_draws"),
    );

    let id_mask = &report["id_mask_summary"];
    assert_eq!(
        id_mask["id"].as_str(),
        Some("web.wasm.webgpu.id_mask_compositor.current"),
    );
    assert_eq!(web_report_number(id_mask, "current_p50_ms"), web_report_number(current, "p50_ms"));
    assert_eq!(
        web_report_number(id_mask, "current_render_passes"),
        web_report_number(current, "render_passes"),
    );
    assert_eq!(
        web_report_number(id_mask, "current_buffer_upload_bytes"),
        web_report_number(current, "buffer_upload_bytes"),
    );
    assert_eq!(web_report_number(id_mask, "vertices"), web_report_number(current, "vertices"));
    assert_eq!(web_report_number(id_mask, "vertex_bytes"), web_report_number(current, "vertex_bytes"));

    let upload = &report["upload_summary"];
    assert_eq!(upload["id"].as_str(), Some("web.wasm.webgpu.upload.current_dirty"));
    assert!(web_report_case_optional(&report, "web.wasm.webgpu.glyph_atlas_upload.legacy_full").is_none());
    assert!(web_report_case_optional(&report, "web.wasm.webgpu.image_upload.legacy_full").is_none());
    assert_eq!(
        web_report_number(upload, "glyph_current_texture_upload_bytes"),
        web_report_number(glyph_current, "texture_upload_bytes"),
    );
    assert_eq!(
        web_report_number(upload, "image_current_texture_upload_bytes"),
        web_report_number(image_current, "texture_upload_bytes"),
    );
    assert_eq!(
        web_report_number(upload, "glyph_current_gpu_timestamp_total_ns"),
        web_report_number(glyph_current, "gpu_timestamp_total_ns"),
    );
    assert_eq!(
        web_report_number(upload, "image_current_gpu_timestamp_total_ns"),
        web_report_number(image_current, "gpu_timestamp_total_ns"),
    );
    assert_eq!(
        web_report_number(upload, "atlas_dirty_width"),
        web_report_number(glyph_current, "dirty_width"),
    );
    assert_eq!(
        web_report_number(upload, "image_dirty_width"),
        web_report_number(image_current, "dirty_width"),
    );

    let effect_uniform = &report["effect_uniform_summary"];
    assert_eq!(
        effect_uniform["id"].as_str(),
        Some("web.wasm.webgpu.effect_uniform.current_batched"),
    );
    assert_eq!(web_report_number(effect_uniform, "current_effect_uniform_writes"), 1.0);
    assert!(web_report_number(effect_uniform, "current_effect_uniform_bytes") > 0.0);
    assert_eq!(
        web_report_number(effect_uniform, "current_effect_uniform_slots"),
        web_report_number(effect_uniform, "expected_backdrops"),
    );
    assert_eq!(
        web_report_number(effect_uniform, "current_texture_copies"),
        web_report_number(effect_current, "texture_copies"),
    );
    assert_eq!(
        web_report_number(effect_uniform, "current_gpu_timestamp_passes"),
        web_report_number(effect_current, "gpu_timestamp_passes"),
    );
    assert_eq!(
        web_report_number(effect_uniform, "current_gpu_timestamp_total_ns"),
        web_report_number(effect_current, "gpu_timestamp_total_ns"),
    );

    let backdrop_batch = &report["backdrop_batch_summary"];
    assert_eq!(
        backdrop_batch["id"].as_str(),
        Some("web.wasm.webgpu.backdrop_batch.current"),
    );
    assert_eq!(
        web_report_number(backdrop_batch, "current_effect_uniform_writes"),
        web_report_number(backdrop_batch_current, "effect_uniform_writes"),
    );
    assert_eq!(
        web_report_number(backdrop_batch, "current_effect_uniform_slots"),
        web_report_number(backdrop_batch_current, "effect_uniform_slots"),
    );
    assert_eq!(
        web_report_number(backdrop_batch, "current_texture_copies"),
        web_report_number(backdrop_batch_current, "texture_copies"),
    );
    assert_eq!(
        web_report_number(backdrop_batch, "current_render_passes"),
        web_report_number(backdrop_batch_current, "render_passes"),
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
fn markdown_out_writes_identical_latest_and_dated_reports() {
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
    let dated_body = std::fs::read(&dated).expect("read dated markdown");
    assert_eq!(latest_body, dated_body);
    assert!(!latest_body.is_empty());

    let _ = std::fs::remove_file(latest);
    let _ = std::fs::remove_file(dated);
    let _ = std::fs::remove_dir(dir);
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
fn filtered_run_suite_supports_paged_text_atlas_locality_case() {
    let mut json_out = std::env::temp_dir();
    json_out.push(format!("oxide-perf-runner-paged-text-atlas-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env("OXIDE_PERF_RUNNER_FILTER", "cpu.architecture.text.paged_atlas_locality")
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
    let row = report_case_slice(&report, "cpu.architecture.text.paged_atlas_locality");
    assert_eq!(report_f64(row, "atlas_pages"), 2.0);
    assert_eq!(report_f64(row, "atlas_evictions"), 1.0);
    assert_eq!(report_f64(row, "atlas_release_calls"), 1.0);
    assert_eq!(report_f64(row, "stable_unrelated_pages"), 1.0);
    assert_eq!(report_f64(row, "atlas_resident_bytes"), 1_152.0);
    assert!(report_f64(row, "atlas_fragmentation_bytes") > 0.0);
    let _ = std::fs::remove_file(json_out);
}

#[test]
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
fn filtered_run_suite_supports_rendering_architecture_contract() {
    let mut json_out = std::env::temp_dir();
    json_out.push(format!("oxide-perf-runner-architecture-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
        .env(
            "OXIDE_PERF_RUNNER_FILTER",
            "cpu.architecture.retained.depth_16.clean,cpu.architecture.retained.cache_pressure,cpu.architecture.animation.surface_300,cpu.architecture.idle.static_foreground",
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
    assert!(stdout.contains("cases=5"), "stdout: {stdout}");
    assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");
    let report = std::fs::read_to_string(&json_out).expect("read rendering architecture report");
    let retained = report_case_slice(&report, "cpu.architecture.retained.depth_16.clean");
    let hot = report_case_slice(&report, "cpu.architecture.retained.cache_pressure.hot_reuse");
    let churn = report_case_slice(&report, "cpu.architecture.retained.cache_pressure.one_use_churn");
    let animation = report_case_slice(&report, "cpu.architecture.animation.surface_300");
    let idle = report_case_slice(&report, "cpu.architecture.idle.static_foreground");
    for row in [retained, hot, churn, animation, idle] {
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
    assert_eq!(report_f64(animation, "animated_nodes"), 300.0);
    assert_eq!(report_f64(animation, "active_animations"), 600.0);
    assert_eq!(report_f64(animation, "chunks_rebuilt_avg"), 0.0);
    assert_eq!(report_f64(animation, "sequences_rebuilt_avg"), 0.0);
    assert_eq!(report_f64(animation, "command_bytes_copied_avg"), 0.0);
    assert_eq!(report_f64(animation, "vertex_bytes_copied_avg"), 0.0);
    assert_eq!(report_f64(animation, "index_bytes_copied_avg"), 0.0);
    assert!(report_f64(animation, "property_records_avg") >= 600.0);
    assert_eq!(report_f64(idle, "submissions"), 0.0);
    assert_eq!(report_f64(idle, "wakeups"), 0.0);
    let _ = std::fs::remove_file(json_out);
}

#[test]
fn dynamic_property_animation_has_a_public_authoring_contract()
{
   let mut json_out = std::env::temp_dir();
   json_out.push(format!("oxide-perf-runner-dynamic-authoring-{}.json", std::process::id()));
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .env("OXIDE_PERF_RUNNER_FILTER", "cpu.authoring.animation.dynamic_properties_300")
      .arg("--run-suite")
      .arg("--smoke")
      .arg("--json-out")
      .arg(&json_out)
      .output()
      .expect("run dynamic property authoring row");
   let stderr = String::from_utf8_lossy(&output.stderr);
   assert!(output.status.success(), "dynamic property authoring row failed: {stderr}");
   let report = std::fs::read_to_string(&json_out).expect("read dynamic property authoring report");
   let row = report_case_slice(&report, "cpu.authoring.animation.dynamic_properties_300");
   assert!(row.contains("\"family\": \"authoring\""));
   assert!(row.contains("\"scenario\": \"authoring\""));
   assert_eq!(report_f64(row, "animated_nodes"), 300.0);
   assert_eq!(report_f64(row, "chunks_rebuilt_avg"), 0.0);
   assert_eq!(report_f64(row, "sequences_rebuilt_avg"), 0.0);
   assert_eq!(report_f64(row, "command_bytes_copied_avg"), 0.0);
   assert!(report_f64(row, "property_records_avg") >= 600.0);
   let _ = std::fs::remove_file(json_out);
}

#[test]
fn retained_spatial_queries_have_engine_and_authoring_contracts()
{
   let mut json_out = std::env::temp_dir();
   json_out.push(format!("oxide-perf-runner-spatial-query-{}.json", std::process::id()));
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .env(
         "OXIDE_PERF_RUNNER_FILTER",
         "cpu.architecture.spatial_metadata.glyph_mesh_10000,cpu.authoring.retained_snapshot.spatial_query_10000",
      )
      .arg("--run-suite")
      .arg("--smoke")
      .arg("--json-out")
      .arg(&json_out)
      .output()
      .expect("run retained spatial-query rows");
   let stderr = String::from_utf8_lossy(&output.stderr);
   assert!(output.status.success(), "retained spatial-query rows failed: {stderr}");
   let report = std::fs::read_to_string(&json_out).expect("read retained spatial-query report");
   for id in [
      "cpu.architecture.spatial_metadata.glyph_mesh_10000",
      "cpu.authoring.retained_snapshot.spatial_query_10000",
   ]
   {
      let row = report_case_slice(&report, id);
      assert_eq!(report_f64(row, "instance_count"), 512.0);
      assert_eq!(report_f64(row, "damage_instances_visited"), 1.0);
      assert_eq!(report_f64(row, "damage_instances_matched"), 1.0);
      assert_eq!(report_f64(row, "damage_vertices_visited"), 0.0);
      assert!(report_f64(row, "snapshot_metadata_bytes") > 0.0);
   }
   let authoring = report_case_slice(
      &report,
      "cpu.authoring.retained_snapshot.spatial_query_10000",
   );
   assert!(authoring.contains("\"family\": \"authoring\""));
   let _ = std::fs::remove_file(json_out);
}

#[cfg(target_os = "macos")]
#[test]
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
fn metal_prepared_chunk_rows_freeze_clean_and_one_dirty_contracts()
{
   let mut json_out = std::env::temp_dir();
   json_out.push(format!("oxide-perf-runner-prepared-chunks-{}.json", std::process::id()));
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .env("OXIDE_PERF_RUNNER_FILTER", "gpu.architecture.prepared_chunks.")
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
   let clean = report_case_slice(&report, "gpu.architecture.prepared_chunks.clean_mixed");
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
   assert!(stdout.contains("cases=3"), "stdout: {stdout}");
   let report = std::fs::read_to_string(&json_out).expect("read prepared-layer report");
   let clean = report_case_slice(&report, "gpu.architecture.prepared_layers.clean_100x100");
   let dirty = report_case_slice(&report, "gpu.architecture.prepared_layers.one_dirty_100x100");
   let authoring = report_case_slice(
      &report,
      "gpu.authoring.retained_snapshot.prepared_layers_clean_100x100",
   );

   for row in [clean, authoring]
   {
      assert_eq!(report_f64(row, "layers"), 100.0);
      assert_eq!(report_f64(row, "draws_per_layer"), 100.0);
      assert_eq!(report_f64(row, "layer_body_commands_scanned_avg"), 0.0);
      assert_eq!(report_f64(row, "layer_body_commands_copied_avg"), 0.0);
      assert_eq!(report_f64(row, "geometry_bytes_copied_avg"), 0.0);
      assert_eq!(report_f64(row, "buffer_upload_bytes_avg"), 0.0);
      assert_eq!(report_f64(row, "layer_texture_creates_avg"), 0.0);
      assert_eq!(report_f64(row, "layer_cache_hits_avg"), 100.0);
      assert_eq!(report_f64(row, "layer_cache_misses_avg"), 0.0);
      assert_eq!(report_f64(row, "layer_offscreen_draws_avg"), 0.0);
      assert_eq!(report_f64(row, "render_passes_avg"), 1.0);
      assert_eq!(report_f64(row, "draws_avg"), 100.0);
      assert_eq!(report_f64(row, "chunks_prepared_avg"), 0.0);
      assert!(report_f64(row, "layer_cache_bytes_peak") > 0.0);
   }
   assert!(authoring.contains("\"family\": \"authoring\""));
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
fn metal_spatial_rows_freeze_small_and_full_damage_contracts()
{
   let mut json_out = std::env::temp_dir();
   json_out.push(format!("oxide-perf-runner-spatial-metal-{}.json", std::process::id()));
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .env("OXIDE_PERF_RUNNER_FILTER", "gpu.architecture.spatial_metadata.")
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
      "gpu.architecture.spatial_metadata.small_damage_glyph_mesh_10000",
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
