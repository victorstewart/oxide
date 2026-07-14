use base64::Engine;
use oxide_perf_runner::{
    ContractCoverageReport, CoverageReport, PerfCaseResult, PerfComparison, PerfReport,
};
use plist::{Dictionary, Value as PlValue};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use tempfile::tempdir;
use xtask::{
    apply_xctestrun_environment_overrides, build_entitlements_dict,
    check_experiment_manifest_text,
    compare_device_comparisons_pass, compare_device_missing_promotion_families,
    compare_device_official_families, compare_uikit_reports, console_output_contains_marker,
    device_console_failure_line, device_process_name, device_support_dir_matches,
    devicectl_notification_observed, display_value_to_base, extract_oxide_device_report_json,
    extract_trace_windows_from_tables, find_device_process_ids,
    format_uikit_only_testing_identifier, is_expected_devicectl_console_termination,
    is_primary_built_xctestrun_file, is_retryable_devicectl_install_error,
    is_retryable_devicectl_json_error, is_retryable_uikit_trace_handshake_error,
    is_retryable_xctrace_record_timeout_error, is_unsupported_gpu_counter_profile_error,
    is_xctrace_trace_bundle, latest_benchmark_build_failure, map_uikit_case,
    merge_background_modes, merge_usage_strings, merge_xcresult_metrics_json_fragments,
    missing_uikit_metrics_case_ids, normalize_ios_version_for_device_support,
    notification_or_console_marker_observed, oxide_device_launch_environment_json,
    parse_apple_development_team_from_security_output, parse_available_ios_sim_destination,
    parse_devicectl_display_backlight_active, parse_devicectl_lock_state_text,
    parse_oxide_app_host_debug_summary, parse_oxide_benchmark_metadata,
    parse_oxide_camera_contract_summary, parse_oxide_frame_cadence_summary,
    parse_oxide_memory_summary, parse_oxide_stage_summary, parse_oxide_static_idle_summary,
    parse_oxide_tick_ring, parse_provisioning_profile_team_identifier,
    parse_react_native_device_report_json, parse_uikit_report_json, parse_xctrace_summary_window,
    parse_xctrace_tables, parse_xctrace_toc_tables,
    perf_frame_capture_relative_source_for_test_name, perf_report_matches_case_ids,
    preferred_xctrace_toc_tables, prepare_resumable_uikit_device_result_root,
    prepare_uikit_device_perf_xctestrun, render_oxide_app_host_debug_summary_note,
    render_oxide_tick_ring_note, resolve_existing_uikit_power_trace,
    start_console_marker_or_completion_observed, summarize_device_gpu_metrics_from_tables,
    summarize_energy_table, summarize_time_profile_from_xml,
    summarize_trace_signpost_metrics_from_tables, uikit_case_in_compare_device_family,
    uikit_case_in_compare_device_watchable_smoke, uikit_case_in_official_device_battery,
    uikit_case_requires_normalized_camera_contract, uikit_device_metrics_case_stdout_path,
    uikit_device_perf_environment_for_test_name, uikit_device_support_required,
    uikit_device_trace_artifact_exists, uikit_device_trace_enabled,
    uikit_only_testing_identifier_for_test_name, uikit_perf_environment_json_for_test_name,
    uikit_perf_environment_json_for_test_name_with_watch_capture,
    uikit_power_trace_candidate_paths, uikit_report_matches_case_ids,
    validate_normalized_camera_contract, validate_oxide_device_report_metric_contract,
    validate_uikit_device_report_metric_contract, xctrace_export_input_path_for_args,
    CompareDeviceProofFamilyStatus, CompareDeviceProofStatus, Entitlements, LocationMode,
    ExperimentCheckSummary, TraceWindow, UIKitCanonicalSignpostSource,
    UIKitContractCoverageReport, UIKitHostBuildStamp, UIKitMetricFallbackMode, UIKitMetricSource,
    UIKitMetricSummary, UIKitPerfCase, UIKitPerfComparison, UIKitPerfReport, XctraceCell,
    XctraceTocTable,
};

fn env_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn with_env_vars(vars: &[(&str, Option<&str>)], body: impl FnOnce()) {
    let _guard = env_test_lock().lock().expect("env test lock");
    let saved: Vec<(String, Option<String>)> =
        vars.iter().map(|(key, _)| (String::from(*key), std::env::var(key).ok())).collect();
    for (key, value) in vars {
        match value {
            Some(value) => unsafe { std::env::set_var(key, value) },
            None => unsafe { std::env::remove_var(key) },
        }
    }
    body();
    for (key, value) in saved {
        match value {
            Some(value) => unsafe { std::env::set_var(&key, value) },
            None => unsafe { std::env::remove_var(&key) },
        }
    }
}

fn sample_perf_report(case_ids: &[&str]) -> PerfReport {
    PerfReport {
        version: 1,
        suite: String::from("oxide-device"),
        generated_label: None,
        cases: case_ids
            .iter()
            .map(|id| PerfCaseResult {
                id: String::from(*id),
                refresh_mode: String::from("native"),
                gated: true,
                metrics: sample_oxide_device_metrics(),
                ..PerfCaseResult::default()
            })
            .collect(),
        coverage: CoverageReport::default(),
        contract: ContractCoverageReport::default(),
        findings: Vec::new(),
    }
}

#[test]
fn oxide_device_contract_source_lists_canonical_families() {
    let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"));
    for id in [
        "launch-lifecycle",
        "primitive-lifecycle",
        "layout-invalidation",
        "text-input",
        "image-pipeline",
        "lists-grids-chat",
        "navigation-input-latency",
        "animation-effects",
        "state-reconciliation",
        "os-bridge-overhead",
        "endurance-memory-thermal",
        "stress-pathological",
    ] {
        assert!(source.contains(id), "missing canonical Oxide device contract family `{id}`");
    }
}

#[test]
fn experiment_manifest_checker_accepts_current_manifest() {
    let text = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../perf-experiments.toml"));
    for id in [
        "native-audit-row-retirement",
        "webgpu-default-standalone-cache-row-retirement",
        "webgpu-default-upload-scratch-row-retirement",
        "webgpu-default-id-mask-legacy-row-retirement",
        "webgpu-default-upload-legacy-row-retirement",
        "webgpu-default-glyph-run-legacy-row-retirement",
        "webgpu-default-backdrop-batch-legacy-row-retirement",
        "webgpu-default-command-family-legacy-row-retirement",
        "webgpu-clip-state-diagnostic-export-retirement",
        "canvas-indexed-quad-stack-array",
        "perf-runner-markdown-metric-summary-direct-streaming",
        "perf-runner-markdown-results-row-direct-streaming",
        "perf-runner-markdown-inline-metric-summary-streaming",
        "perf-runner-markdown-contract-row-direct-streaming",
        "perf-runner-markdown-summary-direct-streaming",
        "perf-runner-markdown-tail-line-direct-streaming",
        "perf-runner-markdown-metric-priority-index-lookup",
        "perf-runner-case-filter-cache",
        "perf-runner-case-filter-starts-with-match",
        "perf-runner-case-filter-state-fast-paths-rejected",
        "perf-runner-markdown-comparison-line-direct-streaming",
        "perf-runner-compare-reports-hash-map-rejected",
        "perf-runner-compare-reports-small-baseline-linear-scan",
        "perf-runner-compare-reports-output-vector-capacity",
        "perf-runner-compare-reports-missing-capacity-ceiling",
        "perf-runner-compare-reports-lookup-improvement-before-envelope",
        "perf-runner-compare-reports-same-order-fast-path",
        "perf-runner-compare-reports-same-order-single-pass",
        "perf-runner-compare-reports-same-order-lazy-improvements-rejected",
        "perf-runner-compare-reports-same-order-equal-median-fast-path",
        "perf-runner-compare-reports-same-order-before-small-baseline",
        "perf-runner-compare-reports-regression-vector-capacity-rejected",
        "perf-runner-compare-reports-specialized-small-baseline-loop-rejected",
        "perf-runner-markdown-latest-dated-single-render",
        "perf-runner-json-pre-sized-serialization",
        "perf-runner-json-capacity-hint-tightening",
        "perf-runner-json-string-pre-sized-serialization-rejected",
        "webgpu-mixed-legacy-row-retirement",
        "webgpu-effect-uniform-legacy-row-retirement",
        "webgpu-direct-surface-legacy-row-retirement",
        "webgpu-neon-marker-legacy-row-retirement",
        "webgpu-clean-layer-dirty-row-retirement",
        "webgpu-id-mask-diagnostic-export-retirement-rejected",
        "ui-core-coalesce-in-place-compaction-rejected",
        "perf-runner-markdown-audit-branch-retirement-rejected",
        "webgpu-upload-scratch-diagnostic-export-retirement-rejected",
        "webgpu-upload-scratch-export-only-retirement-rejected",
        "webgpu-standalone-cache-diagnostic-exports-retirement-rejected",
        "webgpu-draw-item-diagnostic-export-retirement-rejected",
        "webgpu-draw-state-diagnostic-export-retirement-rejected",
        "perf-runner-markdown-preallocation-rejected",
        "perf-runner-metric-summary-vector-capacity-rejected",
        "perf-runner-markdown-metric-priority-table-hoist-rejected",
        "perf-runner-markdown-metric-priority-bitmask-rejected",
        "perf-runner-markdown-literal-string-line-writes-rejected",
        "perf-runner-markdown-metric-summary-cap-early-return-rejected",
        "perf-runner-markdown-empty-priority-fast-path-rejected",
        "perf-runner-contract-coverage-allocation-free-gap-scan",
        "perf-runner-contract-coverage-first-byte-ascii-fold",
        "perf-runner-contract-coverage-tail-only-phrase-match",
        "perf-runner-distribution-summary-unused-fields",
        "perf-runner-distribution-stack-summary-buffer",
        "perf-runner-distribution-fixed-24-summary",
        "perf-runner-sample-summary-stack-buffer",
        "perf-runner-sample-summary-fixed-count-quantiles",
        "perf-runner-sample-summary-unstable-sort",
        "permissions-bluetooth-discovery-cache-move",
        "permissions-sensor-permission-cache-fixed-slots",
        "permissions-manager-state-fixed-slots",
    ] {
        assert!(text.contains(id), "manifest missing `{id}`");
    }
    let summary =
        check_experiment_manifest_text(text, "2026-06-22").expect("current manifest should pass");
    assert_eq!(
        summary,
        ExperimentCheckSummary { total: 148, undecided: 0, accepted: 73, rejected: 75 }
    );
}

#[test]
fn experiment_manifest_checker_rejects_expired_undecided_entries() {
    let text = r#"
[[experiments]]
id = "renderer-packed-stream"
introduced_commit = "abc123"
introduced_date = "2026-06-01"
expires = "2026-06-21"
required_backends = ["renderer-metal"]
required_devices = ["macOS host"]
correctness_gate = "snapshot parity"
performance_gate = "same-workload A/B"
decision_state = "undecided"
perf_ab_gate = "perf-ab:renderer-packed-stream"
"#;
    let err = check_experiment_manifest_text(text, "2026-06-22")
        .expect_err("expired undecided experiment should fail");
    assert!(err.to_string().contains("expired undecided experiment"));
}

#[test]
fn experiment_manifest_checker_requires_perf_ab_gate_for_undecided_entries() {
    let text = r#"
[[experiments]]
id = "renderer-packed-stream"
introduced_commit = "abc123"
introduced_date = "2026-06-01"
expires = "2026-06-23"
required_backends = ["renderer-metal"]
required_devices = ["macOS host"]
correctness_gate = "snapshot parity"
performance_gate = "same-workload A/B"
decision_state = "undecided"
"#;
    let err = check_experiment_manifest_text(text, "2026-06-22")
        .expect_err("undecided experiment without perf-ab gate should fail");
    assert!(err.to_string().contains("perf_ab_gate"));
}

#[test]
fn experiment_manifest_checker_requires_proof_for_decided_entries() {
    let text = r#"
[[experiments]]
id = "renderer-packed-stream"
introduced_commit = "abc123"
introduced_date = "2026-06-01"
expires = "2026-06-23"
required_backends = ["renderer-metal"]
required_devices = ["macOS host"]
correctness_gate = "snapshot parity"
performance_gate = "same-workload A/B"
decision_state = "accepted"
decision = "accepted after proof"
cleanup = ["deleted loser path"]
"#;
    let err = check_experiment_manifest_text(text, "2026-06-22")
        .expect_err("decided experiment without proof should fail");
    assert!(err.to_string().contains("proof"));
}

#[test]
fn xtask_docs_describe_experiment_manifest_check() {
    let docs = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/xtask/lib.md"));
    for needle in [
        "cargo xtask experiments check",
        "perf-experiments.toml",
        "perf-ab",
        "check_experiment_manifest_text",
    ] {
        assert!(docs.contains(needle), "xtask docs missing `{needle}`");
    }
}

fn sample_uikit_report(case_ids: &[&str]) -> UIKitPerfReport {
    UIKitPerfReport {
        version: 1,
        suite: String::from("uikit-device"),
        generated_label: None,
        device_name: String::from("iPhone"),
        energy_status: String::from("manual-pending"),
        contract: UIKitContractCoverageReport::default(),
        cases: case_ids
            .iter()
            .map(|id| UIKitPerfCase { id: String::from(*id), ..UIKitPerfCase::default() })
            .collect(),
        notes: Vec::new(),
    }
}

#[test]
fn entitlements_gen() {
    let entitlements = Entitlements {
        push_notifications: true,
        bluetooth_central: true,
        bluetooth_peripheral: false,
        background_fetch: true,
        background_remote_notification: true,
        background_processing: false,
        location: LocationMode::WhenInUse,
    };

    let dict = build_entitlements_dict(&entitlements);
    assert_eq!(
        dict.get("aps-environment").and_then(|value| value.as_string()),
        Some("development")
    );

    let mut info = Dictionary::new();
    let mut usage = BTreeMap::new();
    usage.insert(String::from("NSBluetoothAlwaysUsageDescription"), String::from("Needed"));
    usage.insert(String::from("NSLocationWhenInUseUsageDescription"), String::from("Needed"));
    merge_usage_strings(&mut info, &usage);
    merge_background_modes(&mut info, &entitlements);
    assert!(info.contains_key("UIBackgroundModes"));
}

#[test]
fn uikit_device_trace_enabled_treats_zero_as_console_only_mode() {
    assert!(!uikit_device_trace_enabled(0));
    assert!(uikit_device_trace_enabled(1));
    assert!(uikit_device_trace_enabled(3));
}

#[test]
fn xctrace_export_input_path_is_extracted_for_retry_settle() {
    let args = vec![
        String::from("xctrace"),
        String::from("export"),
        String::from("--input"),
        String::from("/tmp/case/metal.trace"),
        String::from("--xpath"),
        String::from("/trace-toc/run[1]/data[1]/table[107]"),
    ];

    assert_eq!(
        xctrace_export_input_path_for_args(&args),
        Some(PathBuf::from("/tmp/case/metal.trace"))
    );
    assert_eq!(xctrace_export_input_path_for_args(&args[0..2]), None);
}

#[test]
fn uikit_device_metrics_case_stdout_path_uses_case_name_suffix() {
    let root = tempdir().expect("tempdir");
    let path = uikit_device_metrics_case_stdout_path(
        root.path(),
        "native",
        "testCameraNV12LegacyRealAppLivePreview",
    );
    assert_eq!(
        path,
        root.path().join("metrics-native-testCameraNV12LegacyRealAppLivePreview.stdout.log")
    );
}

#[test]
fn uikit_device_refresh_mode_rejects_non_native_values() {
    let err = uikit_perf_environment_json_for_test_name("testLabelEncode", "60hz")
        .expect_err("60hz should be rejected");
    assert!(err.to_string().contains("native-only"));
}

#[test]
fn parse_uikit_report_json_maps_cases_and_metrics() {
    let json = r#"
[
  {
    "testIdentifier": "OxideHostPerfTests/testLabelEncode()",
    "testRuns": [
      {
        "device": {
          "deviceName": "iPhone 16"
        },
        "metrics": [
          {
            "identifier": "com.apple.dt.XCTMetric_Clock.time.monotonic",
            "unitOfMeasurement": "s",
            "measurements": [0.001, 0.002, 0.003]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.time",
            "unitOfMeasurement": "s",
            "measurements": [0.004, 0.005, 0.006]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.cycles",
            "unitOfMeasurement": "kC",
            "measurements": [10.0, 11.0, 12.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.instructions_retired",
            "unitOfMeasurement": "kI",
            "measurements": [13.0, 14.0, 15.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_Memory.physical",
            "unitOfMeasurement": "kB",
            "measurements": [16.0, 17.0, 18.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_Memory.physical_peak",
            "unitOfMeasurement": "kB",
            "measurements": [19.0, 20.0, 21.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_OSSignpost-PerfWorkload.duration",
            "unitOfMeasurement": "s",
            "measurements": [0.007, 0.008, 0.009]
          }
        ]
      }
    ]
  }
]
"#;

    let report = parse_uikit_report_json(json).expect("parse UIKit perf report");
    assert_eq!(report.device_name, "iPhone 16");
    assert_eq!(report.cases.len(), 1);
    assert_eq!(report.cases[0].id, "uikit.component.label.encode");
    assert_eq!(report.cases[0].oxide_case_id, "cpu.component.label.encode");
    assert_eq!(report.cases[0].layer, "engine");
    assert_eq!(report.cases[0].scenario, "primitive-view");
    assert_eq!(report.cases[0].style, "idiomatic");
    assert_eq!(report.cases[0].refresh_mode, "simulator-default");
    assert_eq!(report.cases[0].measure_iterations, 3);
    assert_eq!(report.cases[0].benchmark_iterations, 0);
    assert_eq!(report.cases[0].canonical_signpost_source, UIKitCanonicalSignpostSource::XCTest);
    assert_eq!(report.contract.styles[0].status, "implemented");
    assert_eq!(report.cases[0].metrics["clock_s"].median, 0.002);
    assert!((report.cases[0].metrics["memory_peak_kb"].p95 - 20.9).abs() < 1e-9);
    assert_eq!(report.cases[0].metrics["workload_s"].median, 0.008);
    assert_eq!(report.cases[0].metrics["clock_s"].source, UIKitMetricSource::XCTest);
    assert_eq!(report.cases[0].metrics["workload_s"].source, UIKitMetricSource::XCTestSignpost);
    assert!(report.cases[0].metrics["workload_s"].fallback_modes.is_empty());
}

#[test]
fn parse_uikit_report_json_classifies_optimized_cases() {
    let json = r#"
[
  {
    "testIdentifier": "OxideHostPerfTests/testOptimizedFlatRects10Mount()",
    "testRuns": [
      {
        "device": {
          "deviceName": "iPhone 16"
        },
        "metrics": [
          {
            "identifier": "com.apple.dt.XCTMetric_Clock.time.monotonic",
            "unitOfMeasurement": "s",
            "measurements": [0.001, 0.002, 0.003]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.time",
            "unitOfMeasurement": "s",
            "measurements": [0.004, 0.005, 0.006]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.cycles",
            "unitOfMeasurement": "kC",
            "measurements": [10.0, 11.0, 12.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.instructions_retired",
            "unitOfMeasurement": "kI",
            "measurements": [13.0, 14.0, 15.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_Memory.physical",
            "unitOfMeasurement": "kB",
            "measurements": [16.0, 17.0, 18.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_Memory.physical_peak",
            "unitOfMeasurement": "kB",
            "measurements": [19.0, 20.0, 21.0]
          }
        ]
      }
    ]
  }
]
"#;

    let report = parse_uikit_report_json(json).expect("parse optimized UIKit perf report");
    assert_eq!(report.cases[0].id, "uikit.optimized.primitive.flat_rects.10.mount");
    assert_eq!(report.cases[0].style, "optimized");
    assert_eq!(report.contract.styles[1].status, "partial");
}

#[test]
fn parse_uikit_report_json_classifies_authoring_cases() {
    let json = r#"
[
  {
    "testIdentifier": "OxideHostPerfTests/testTextFieldsEditCycle()",
    "testRuns": [
      {
        "device": {
          "deviceName": "iPhone 16"
        },
        "metrics": [
          {
            "identifier": "com.apple.dt.XCTMetric_Clock.time.monotonic",
            "unitOfMeasurement": "s",
            "measurements": [0.010, 0.012, 0.014]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.time",
            "unitOfMeasurement": "s",
            "measurements": [0.020, 0.021, 0.022]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.cycles",
            "unitOfMeasurement": "kC",
            "measurements": [10.0, 11.0, 12.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.instructions_retired",
            "unitOfMeasurement": "kI",
            "measurements": [13.0, 14.0, 15.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_Memory.physical",
            "unitOfMeasurement": "kB",
            "measurements": [16.0, 17.0, 18.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_Memory.physical_peak",
            "unitOfMeasurement": "kB",
            "measurements": [19.0, 20.0, 21.0]
          }
        ]
      }
    ]
  }
]
"#;

    let report = parse_uikit_report_json(json).expect("parse UIKit authoring report");
    assert_eq!(report.cases[0].id, "uikit.idiomatic.authoring.text_fields.edit_cycle");
    assert_eq!(report.cases[0].oxide_case_id, "cpu.authoring.text_fields.edit_cycle");
    assert_eq!(report.cases[0].layer, "engine");
    assert_eq!(report.cases[0].scenario, "authoring");
    assert_eq!(report.cases[0].style, "idiomatic");
    assert_eq!(report.cases[0].refresh_mode, "simulator-default");
}

#[test]
fn missing_uikit_metrics_case_ids_returns_missing_expected_rows() {
    let json = r#"
[
  {
    "testIdentifier": "OxideHostPerfTests/testCameraNV12LegacyLivePreview()",
    "testRuns": [
      {
        "device": {
          "deviceName": "iPhone 16 Pro"
        },
        "metrics": [
          {
            "identifier": "com.apple.dt.XCTMetric_Clock.time.monotonic",
            "unitOfMeasurement": "s",
            "measurements": [0.1, 0.2, 0.3]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.time",
            "unitOfMeasurement": "s",
            "measurements": [0.4, 0.5, 0.6]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.cycles",
            "unitOfMeasurement": "kC",
            "measurements": [10.0, 11.0, 12.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.instructions_retired",
            "unitOfMeasurement": "kI",
            "measurements": [13.0, 14.0, 15.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_Memory.physical",
            "unitOfMeasurement": "kB",
            "measurements": [16.0, 17.0, 18.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_Memory.physical_peak",
            "unitOfMeasurement": "kB",
            "measurements": [19.0, 20.0, 21.0]
          }
        ]
      }
    ]
  }
]
"#;

    let missing = missing_uikit_metrics_case_ids(
        json,
        &[
            "uikit.optimized.image_pipeline.camera_preview.nv12_legacy_live",
            "uikit.idiomatic.image_pipeline.camera_preview.avfoundation_preview_layer_live",
        ],
    )
    .expect("missing case ids");
    assert_eq!(
        missing,
        vec![String::from(
            "uikit.idiomatic.image_pipeline.camera_preview.avfoundation_preview_layer_live"
        )]
    );
}

#[test]
fn missing_uikit_metrics_case_ids_accepts_complete_shard() {
    let json = r#"
[
  {
    "testIdentifier": "OxideHostPerfTests/testCameraNV12LegacyLivePreview()",
    "testRuns": [
      {
        "device": {
          "deviceName": "iPhone 16 Pro"
        },
        "metrics": [
          {
            "identifier": "com.apple.dt.XCTMetric_Clock.time.monotonic",
            "unitOfMeasurement": "s",
            "measurements": [0.1, 0.2, 0.3]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.time",
            "unitOfMeasurement": "s",
            "measurements": [0.4, 0.5, 0.6]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.cycles",
            "unitOfMeasurement": "kC",
            "measurements": [10.0, 11.0, 12.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.instructions_retired",
            "unitOfMeasurement": "kI",
            "measurements": [13.0, 14.0, 15.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_Memory.physical",
            "unitOfMeasurement": "kB",
            "measurements": [16.0, 17.0, 18.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_Memory.physical_peak",
            "unitOfMeasurement": "kB",
            "measurements": [19.0, 20.0, 21.0]
          }
        ]
      }
    ]
  },
  {
    "testIdentifier": "OxideHostPerfTests/testCameraAVFoundationPreviewLayerLivePreview()",
    "testRuns": [
      {
        "device": {
          "deviceName": "iPhone 16 Pro"
        },
        "metrics": [
          {
            "identifier": "com.apple.dt.XCTMetric_Clock.time.monotonic",
            "unitOfMeasurement": "s",
            "measurements": [0.1, 0.2, 0.3]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.time",
            "unitOfMeasurement": "s",
            "measurements": [0.4, 0.5, 0.6]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.cycles",
            "unitOfMeasurement": "kC",
            "measurements": [10.0, 11.0, 12.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.instructions_retired",
            "unitOfMeasurement": "kI",
            "measurements": [13.0, 14.0, 15.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_Memory.physical",
            "unitOfMeasurement": "kB",
            "measurements": [16.0, 17.0, 18.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_Memory.physical_peak",
            "unitOfMeasurement": "kB",
            "measurements": [19.0, 20.0, 21.0]
          }
        ]
      }
    ]
  }
]
"#;

    let missing = missing_uikit_metrics_case_ids(
        json,
        &[
            "uikit.optimized.image_pipeline.camera_preview.nv12_legacy_live",
            "uikit.idiomatic.image_pipeline.camera_preview.avfoundation_preview_layer_live",
        ],
    )
    .expect("complete shard");
    assert!(missing.is_empty());
}

#[test]
fn merge_xcresult_metrics_json_fragments_combines_shards() {
    let merged = merge_xcresult_metrics_json_fragments(&[
        String::from(
            r#"[{"testIdentifier":"OxideHostPerfTests/testLabelEncode()","testRuns":[{"device":{"deviceName":"iPhone 17 Pro"},"metrics":[]}]}]"#
        ),
        String::from(
            r#"[{"testIdentifier":"OxideHostPerfTests/testButtonEncode()","testRuns":[{"device":{"deviceName":"iPhone 17 Pro"},"metrics":[]}]}]"#
        ),
    ])
    .expect("merge shard metrics");
    let bundles: Vec<serde_json::Value> =
        serde_json::from_str(&merged).expect("parse merged shard metrics");
    assert_eq!(bundles.len(), 2);
    assert_eq!(bundles[0]["testIdentifier"].as_str(), Some("OxideHostPerfTests/testLabelEncode()"));
    assert_eq!(
        bundles[1]["testIdentifier"].as_str(),
        Some("OxideHostPerfTests/testButtonEncode()")
    );
}

#[test]
fn extract_oxide_device_report_json_decodes_console_chunks() {
    let expected = r#"{"suite":"workspace","cases":[{"id":"cpu.component.label.encode"}]}"#;
    let payload = base64::engine::general_purpose::STANDARD.encode(expected.as_bytes());
    let split = payload.len() / 2;
    let stdout = format!(
        "prefix\n{} \nnoise {}\n{}{}\n{}{}\n{}\nsuffix\n",
        "OXIDE_READY oxide-perf-runner",
        "ignored",
        "device-log ",
        "OXIDE_PERF_REPORT_BEGIN",
        "device-log ",
        format!("OXIDE_PERF_REPORT_CHUNK {}", &payload[..split]),
        format!(
            "device-log OXIDE_PERF_REPORT_CHUNK {}\ndevice-log OXIDE_PERF_REPORT_END",
            &payload[split..]
        ),
    );

    let json = extract_oxide_device_report_json(&stdout).expect("decode Oxide device payload");
    assert_eq!(json, expected);
}

#[test]
fn parse_oxide_stage_summary_maps_stage_metrics() {
    let stdout = concat!(
        "OXIDE_READY testCameraNV12LegacyLivePreview\n",
        "OXIDE_STAGE_SUMMARY {\"stages\":{\"camera.capture.frame_delivery\":{\"unit\":\"ms\",\"min\":0.0,\"max\":0.4,\"mean\":0.1,\"median\":0.0,\"p95\":0.4,\"p99\":0.4,\"samples\":4},\"camera.capture.sample_setup\":{\"unit\":\"ms\",\"min\":0.1,\"max\":0.3,\"mean\":0.2,\"median\":0.2,\"p95\":0.3,\"p99\":0.3,\"samples\":4},\"camera.host.frame\":{\"unit\":\"ms\",\"min\":1.0,\"max\":3.0,\"mean\":2.0,\"median\":2.0,\"p95\":3.0,\"p99\":3.0,\"samples\":4},\"camera.renderer.direct.fetch\":{\"unit\":\"ms\",\"min\":0.5,\"max\":1.5,\"mean\":1.0,\"median\":1.0,\"p95\":1.5,\"p99\":1.5,\"samples\":4}}}\n",
        "OXIDE_COMPLETE testCameraNV12LegacyLivePreview\n"
    );

    let stages = parse_oxide_stage_summary(stdout).expect("parse stage summary");
    assert_eq!(stages["stage.camera.host.frame"].unit, "ms");
    assert_eq!(stages["stage.camera.host.frame"].median, 2.0);
    assert_eq!(stages["stage.camera.renderer.direct.fetch"].p95, 1.5);
    assert_eq!(stages["stage.camera.capture.sample_setup"].mean, 0.2);
    assert_eq!(stages["stage.camera.capture.frame_delivery"].median, 0.0);
    assert_eq!(stages["stage.camera.renderer.direct.fetch"].samples, 4);
}

#[test]
fn parse_oxide_memory_summary_maps_memory_metrics() {
    let stdout = concat!(
        "OXIDE_READY testCameraNV12LegacyLivePreview\n",
        "OXIDE_MEMORY_SUMMARY {\"categories\":{\"camera.active_sample_buffers\":{\"unit\":\"count\",\"min\":0.0,\"max\":3.0,\"mean\":1.25,\"median\":1.0,\"p95\":3.0,\"p99\":3.0,\"samples\":4},\"camera.active_sample_surface_bytes_est\":{\"unit\":\"bytes\",\"min\":0.0,\"max\":2097152.0,\"mean\":1048576.0,\"median\":1048576.0,\"p95\":2097152.0,\"p99\":2097152.0,\"samples\":4},\"camera.active_sample_surface_surfaces\":{\"unit\":\"count\",\"min\":0.0,\"max\":2.0,\"mean\":1.0,\"median\":1.0,\"p95\":2.0,\"p99\":2.0,\"samples\":4},\"camera.peak_active_sample_buffers\":{\"unit\":\"count\",\"min\":1.0,\"max\":4.0,\"mean\":2.5,\"median\":2.0,\"p95\":4.0,\"p99\":4.0,\"samples\":4},\"camera.peak_active_sample_surface_bytes_est\":{\"unit\":\"bytes\",\"min\":1048576.0,\"max\":3145728.0,\"mean\":2097152.0,\"median\":2097152.0,\"p95\":3145728.0,\"p99\":3145728.0,\"samples\":4},\"camera.peak_active_sample_surface_surfaces\":{\"unit\":\"count\",\"min\":1.0,\"max\":3.0,\"mean\":2.0,\"median\":2.0,\"p95\":3.0,\"p99\":3.0,\"samples\":4},\"known.total_bytes_est\":{\"unit\":\"bytes\",\"min\":7340032.0,\"max\":9437184.0,\"mean\":8388608.0,\"median\":8388608.0,\"p95\":9437184.0,\"p99\":9437184.0,\"samples\":4},\"renderer.buffer_bytes\":{\"unit\":\"bytes\",\"min\":262144.0,\"max\":524288.0,\"mean\":393216.0,\"median\":393216.0,\"p95\":524288.0,\"p99\":524288.0,\"samples\":4},\"renderer.total_bytes\":{\"unit\":\"bytes\",\"min\":4194304.0,\"max\":6291456.0,\"mean\":5242880.0,\"median\":5242880.0,\"p95\":6291456.0,\"p99\":6291456.0,\"samples\":4},\"renderer.pending_command_buffers\":{\"unit\":\"count\",\"min\":0.0,\"max\":2.0,\"mean\":1.0,\"median\":1.0,\"p95\":2.0,\"p99\":2.0,\"samples\":4},\"view.drawable_pool_bytes_est\":{\"unit\":\"bytes\",\"min\":3145728.0,\"max\":3145728.0,\"mean\":3145728.0,\"median\":3145728.0,\"p95\":3145728.0,\"p99\":3145728.0,\"samples\":4}}}\n",
        "OXIDE_COMPLETE testCameraNV12LegacyLivePreview\n"
    );

    let memory = parse_oxide_memory_summary(stdout).expect("parse memory summary");
    assert_eq!(memory["memory.renderer.total_bytes"].unit, "bytes");
    assert_eq!(memory["memory.renderer.total_bytes"].median, 5_242_880.0);
    assert_eq!(memory["memory.renderer.buffer_bytes"].p95, 524_288.0);
    assert_eq!(memory["memory.view.drawable_pool_bytes_est"].max, 3_145_728.0);
    assert_eq!(memory["memory.renderer.pending_command_buffers"].unit, "count");
    assert_eq!(memory["memory.renderer.pending_command_buffers"].max, 2.0);
    assert_eq!(memory["memory.camera.active_sample_surface_bytes_est"].median, 1_048_576.0);
    assert_eq!(memory["memory.camera.active_sample_surface_surfaces"].max, 2.0);
    assert_eq!(memory["memory.camera.active_sample_buffers"].max, 3.0);
    assert_eq!(memory["memory.camera.peak_active_sample_surface_bytes_est"].median, 2_097_152.0);
    assert_eq!(memory["memory.camera.peak_active_sample_surface_surfaces"].max, 3.0);
    assert_eq!(memory["memory.camera.peak_active_sample_buffers"].max, 4.0);
    assert_eq!(memory["memory.known.total_bytes_est"].samples, 4);
}

#[test]
fn parse_oxide_frame_cadence_summary_maps_hitch_and_missed_frames() {
    let stdout = concat!(
        "OXIDE_READY testSpinnerSpin\n",
        "OXIDE_FRAME_CADENCE_SUMMARY {\"metrics\":{\"frame_interval_ms\":{\"unit\":\"ms\",\"min\":8.0,\"max\":20.0,\"mean\":10.0,\"median\":8.3,\"p95\":18.0,\"p99\":20.0,\"samples\":12},\"frame_budget_ms\":{\"unit\":\"ms\",\"min\":8.3,\"max\":8.3,\"mean\":8.3,\"median\":8.3,\"p95\":8.3,\"p99\":8.3,\"samples\":13},\"hitch_ms_per_s\":{\"unit\":\"ms/s\",\"min\":3.5,\"max\":3.5,\"mean\":3.5,\"median\":3.5,\"p95\":3.5,\"p99\":3.5,\"samples\":1},\"missed_frames\":{\"unit\":\"frames\",\"min\":2.0,\"max\":2.0,\"mean\":2.0,\"median\":2.0,\"p95\":2.0,\"p99\":2.0,\"samples\":1},\"missed_frames_per_s\":{\"unit\":\"frames/s\",\"min\":4.0,\"max\":4.0,\"mean\":4.0,\"median\":4.0,\"p95\":4.0,\"p99\":4.0,\"samples\":1}}}\n",
        "OXIDE_COMPLETE testSpinnerSpin\n"
    );

    let cadence = parse_oxide_frame_cadence_summary(stdout).expect("parse cadence summary");

    assert_eq!(cadence["hitch_ms_per_s"].median, 3.5);
    assert_eq!(cadence["missed_frames"].median, 2.0);
    assert_eq!(cadence["missed_frames_per_s"].unit, "frames/s");
    assert_eq!(cadence["frame_interval_ms"].p99, 20.0);
    assert_eq!(cadence["frame_budget_ms"].samples, 13);
    assert_eq!(cadence["hitch_ms_per_s"].source, UIKitMetricSource::DeviceConsoleFrameCadence);
}

#[test]
fn parse_oxide_tick_ring_summarizes_submission_depth_and_frame_age() {
    let stdout = concat!(
        "OXIDE_READY testCameraNV12LegacyLivePreview\n",
        "OXIDE_TICK_RING {\"ticks\":[",
        "{\"serial\":1,\"drawableWidth\":1170,\"drawableHeight\":2532,\"drawableScale\":3.0,\"planReason\":384,\"planMs\":0.01,\"drawableAcquireMs\":0.02,\"frameCallMs\":0.30,\"tickTotalMs\":0.33,\"skipped\":false,\"drawableAcquired\":true,\"frameSubmitted\":true,\"previewSubmissionDepth\":1,\"previewSubmissionSkipped\":false,\"previewFrameAgeMs\":1.25},",
        "{\"serial\":2,\"drawableWidth\":1170,\"drawableHeight\":2532,\"drawableScale\":3.0,\"planReason\":0,\"planMs\":0.01,\"drawableAcquireMs\":0.0,\"frameCallMs\":0.0,\"tickTotalMs\":0.01,\"skipped\":true,\"drawableAcquired\":false,\"frameSubmitted\":false,\"previewSubmissionDepth\":0,\"previewSubmissionSkipped\":false,\"previewFrameAgeMs\":0.0},",
        "{\"serial\":3,\"drawableWidth\":1170,\"drawableHeight\":2532,\"drawableScale\":3.0,\"planReason\":512,\"planMs\":0.02,\"drawableAcquireMs\":0.03,\"frameCallMs\":0.31,\"tickTotalMs\":0.36,\"skipped\":false,\"drawableAcquired\":true,\"frameSubmitted\":true,\"previewSubmissionDepth\":2,\"previewSubmissionSkipped\":true,\"previewFrameAgeMs\":2.50}",
        "]}\n",
        "OXIDE_COMPLETE testCameraNV12LegacyLivePreview\n"
    );

    let payload = parse_oxide_tick_ring(stdout).expect("parse tick ring");
    assert_eq!(payload.ticks.len(), 3);
    assert_eq!(payload.ticks[0].preview_submission_depth, 1);
    assert!(payload.ticks[2].preview_submission_skipped);
    let note = render_oxide_tick_ring_note(&payload).expect("render tick ring note");
    assert!(note.contains("ticks=3"));
    assert!(note.contains("previewDepth p50/p95/p99/max=1/2/2/2"));
    assert!(note.contains("previewFrameAge p50/p95/p99/max=1.88/2.44/2.49/2.50ms"));
}

#[test]
fn parse_oxide_app_host_debug_summary_surfaces_actual_app_counters() {
    let stdout = concat!(
        "OXIDE_READY testCameraNV12LegacyRealAppLivePreview\n",
        "OXIDE_APP_HOST_DEBUG_SUMMARY {\"cameraFrameTriggeredRenders\":15,\"cameraGenerationAdvances\":16,\"commandBuffersCommitted\":14,\"displayLinkCallbacks\":18,\"displayLinkCreateCalls\":1,\"drawablesAcquired\":14,\"ensureHostInitializedCalls\":1,\"hostReady\":true,\"hostReadyTransitions\":1,\"metalViewInstalls\":1,\"normalSceneBranchCalls\":0,\"onTickCalls\":18,\"perfSceneBranchCalls\":1,\"planSkips\":4,\"presentedFrameAgeMs\":1.75,\"previewSubmissionDepth\":1,\"runningPerfBenchmarkHost\":true,\"runningUiTest\":true,\"samplesBridged\":16,\"samplesDroppedPrebridge\":3,\"samplesPresented\":14,\"samplesPublished\":16,\"samplesReceived\":19,\"samplesSupersededBeforePresent\":2,\"sceneDidBecomeActiveCalls\":1,\"sceneWillConnectCalls\":1,\"sceneWillEnterForegroundCalls\":0,\"shouldRender\":true}\n",
        "OXIDE_COMPLETE testCameraNV12LegacyRealAppLivePreview\n"
    );

    let payload = parse_oxide_app_host_debug_summary(stdout).expect("parse app-host debug summary");
    assert_eq!(payload.display_link_callbacks, 18);
    assert_eq!(payload.camera_generation_advances, 16);
    assert_eq!(payload.samples_dropped_prebridge, 3);
    let note = render_oxide_app_host_debug_summary_note(&payload);
    assert!(note.contains("displayLinkCallbacks=18"));
    assert!(note.contains("cameraTriggeredRenders=15"));
    assert!(note.contains(
        "samples received/droppedPrebridge/bridged/published/presented/superseded=19/3/16/16/14/2"
    ));
}

#[test]
fn parse_oxide_static_idle_summary_surfaces_no_redraw_contract() {
    let stdout = concat!(
        "OXIDE_READY testOxideStaticIdleNoRedraw\n",
        "OXIDE_STATIC_IDLE_SUMMARY {\"contractPassed\":true,\"deltaCommandBuffersCommitted\":0,\"deltaDisplayLinkCallbacks\":12,\"deltaDrawablesAcquired\":0,\"deltaHostIdleSkippedFrames\":12,\"deltaHostSubmittedFrames\":0,\"deltaPlanSkips\":12,\"endCommandBuffersCommitted\":2,\"endDisplayLinkCallbacks\":18,\"endDrawablesAcquired\":2,\"endHostFrameDirty\":0,\"endHostIdleSkippedFrames\":14,\"endHostSettleFramesRemaining\":0,\"endHostSubmittedFrames\":2,\"endPlanSkips\":14,\"startCommandBuffersCommitted\":2,\"startDisplayLinkCallbacks\":6,\"startDrawablesAcquired\":2,\"startHostIdleSkippedFrames\":2,\"startHostSubmittedFrames\":2,\"startPlanSkips\":2,\"windowMs\":116.7}\n",
        "OXIDE_COMPLETE testOxideStaticIdleNoRedraw\n"
    );

    let payload = parse_oxide_static_idle_summary(stdout).expect("parse static-idle summary");
    assert!(payload.contract_passed);
    assert_eq!(payload.delta_drawables_acquired, 0);
    assert_eq!(payload.delta_command_buffers_committed, 0);
    assert_eq!(payload.delta_host_submitted_frames, 0);
    assert_eq!(payload.end_host_frame_dirty, 0);
    assert_eq!(payload.end_host_settle_frames_remaining, 0);
    assert_eq!(payload.delta_host_idle_skipped_frames, 12);
    assert_eq!(payload.window_ms, 116.7);
}

#[test]
fn parse_oxide_camera_contract_summary_maps_contract_fields() {
    let stdout = concat!(
        "OXIDE_READY testCameraNV12LegacyLivePreview\n",
        "OXIDE_CAMERA_CONTRACT_SUMMARY {\"activeFps\":30.0,\"activeHeight\":720,\"activePixelFormat\":\"420f\",\"activeWidth\":1280,\"colorSpace\":\"srgb\",\"devicePosition\":\"back\",\"mirrored\":false,\"requestedFps\":30,\"requestedHeight\":720,\"requestedPixelFormat\":\"420f\",\"requestedWidth\":1280,\"sessionPreset\":\"inputPriority\",\"source\":\"oxide-live\",\"transport\":\"AVCaptureVideoDataOutput+CVMetalTexture(NV12)\",\"videoRange\":\"full\",\"wideColorAuto\":false}\n",
        "OXIDE_COMPLETE testCameraNV12LegacyLivePreview\n"
    );

    let contract =
        parse_oxide_camera_contract_summary(stdout).expect("parse camera contract summary");
    assert_eq!(contract.source, "oxide-live");
    assert_eq!(contract.transport, "AVCaptureVideoDataOutput+CVMetalTexture(NV12)");
    assert_eq!(contract.device_position, "back");
    assert_eq!(contract.session_preset, "inputPriority");
    assert_eq!(contract.requested_pixel_format, "420f");
    assert_eq!(contract.active_pixel_format, "420f");
    assert_eq!(contract.requested_width, 1280);
    assert_eq!(contract.requested_height, 720);
    assert_eq!(contract.requested_fps, 30);
    assert_eq!(contract.active_width, 1280);
    assert_eq!(contract.active_height, 720);
    assert_eq!(contract.active_fps, 30.0);
    assert_eq!(contract.video_range, "full");
    assert_eq!(contract.color_space, "srgb");
    assert!(!contract.wide_color_auto);
    assert!(!contract.mirrored);
}

#[test]
fn parse_oxide_benchmark_metadata_collects_per_test_iteration_counts() {
    let stdout = concat!(
        "OXIDE_BENCHMARK_METADATA ",
        "{\"testName\":\"testLabelEncode\",\"measureIterations\":10,\"benchmarkIterations\":96}\n",
        "noise\n",
        "OXIDE_BENCHMARK_METADATA ",
        "{\"testName\":\"testSimpleHomeColdLaunch\",\"measureIterations\":10,\"benchmarkIterations\":1}\n",
    );

    let metadata = parse_oxide_benchmark_metadata(stdout).expect("parse benchmark metadata");
    assert_eq!(metadata["testLabelEncode"].measure_iterations, 10);
    assert_eq!(metadata["testLabelEncode"].benchmark_iterations, 96);
    assert_eq!(metadata["testSimpleHomeColdLaunch"].benchmark_iterations, 1);
}

#[test]
fn validate_normalized_camera_contract_accepts_yuv_family_formats() {
    let oxide_contract = parse_oxide_camera_contract_summary(concat!(
        "OXIDE_CAMERA_CONTRACT_SUMMARY ",
        "{\"activeFps\":30.0,\"activeHeight\":720,\"activePixelFormat\":\"420f\",\"activeWidth\":1280,\"colorSpace\":\"srgb\",\"devicePosition\":\"back\",\"mirrored\":false,\"requestedFps\":30,\"requestedHeight\":720,\"requestedPixelFormat\":\"420v\",\"requestedWidth\":1280,\"sessionPreset\":\"inputPriority\",\"source\":\"oxide-live\",\"transport\":\"AVCaptureVideoDataOutput+CVMetalTexture(NV12)\",\"videoRange\":\"full\",\"wideColorAuto\":false}\n"
    ))
    .expect("parse Oxide camera contract summary");
    validate_normalized_camera_contract(&oxide_contract, "Oxide")
        .expect("420f/420v should be accepted as YUV-family formats");

    let react_contract = parse_oxide_camera_contract_summary(concat!(
        "OXIDE_CAMERA_CONTRACT_SUMMARY ",
        "{\"activeFps\":30.0,\"activeHeight\":720,\"activePixelFormat\":\"yuv\",\"activeWidth\":1280,\"colorSpace\":\"unknown\",\"devicePosition\":\"back\",\"mirrored\":false,\"requestedFps\":30,\"requestedHeight\":720,\"requestedPixelFormat\":\"yuv\",\"requestedWidth\":1280,\"sessionPreset\":\"format:1280x720\",\"source\":\"react-native-vision-camera\",\"transport\":\"native-preview-view\",\"videoRange\":\"unknown\",\"wideColorAuto\":false}\n"
    ))
    .expect("parse React camera contract summary");
    validate_normalized_camera_contract(&react_contract, "React Native")
        .expect("React yuv contract should be accepted");
}

#[test]
fn uikit_case_requires_normalized_camera_contract_only_for_live_camera_cases() {
    assert!(uikit_case_requires_normalized_camera_contract("testCameraNV12LegacyLivePreview"));
    assert!(uikit_case_requires_normalized_camera_contract(
        "testCameraAVFoundationPreviewLayerLivePreview"
    ));
    assert!(!uikit_case_requires_normalized_camera_contract("testCameraNV12LegacyPreview"));
}

#[test]
fn parse_react_native_device_report_json_maps_metrics_and_contract() {
    let metrics_json = r#"
[
  {
    "testIdentifier": "ReactNativeCameraBenchPerfTests/ReactNativeCameraBenchPerfTests/testReactNativeVisionCameraLivePreview()",
    "testRuns": [
      {
        "device": {
          "deviceName": "Victor's iPhone"
        },
        "metrics": [
          {
            "identifier": "com.apple.dt.XCTMetric_Clock.time.monotonic",
            "unitOfMeasurement": "s",
            "measurements": [0.70, 0.74, 0.82]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.time",
            "unitOfMeasurement": "s",
            "measurements": [0.002, 0.003, 0.004]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.cycles",
            "unitOfMeasurement": "kC",
            "measurements": [3000.0, 3100.0, 3200.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.instructions_retired",
            "unitOfMeasurement": "kI",
            "measurements": [4800.0, 4900.0, 5000.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_Memory.physical",
            "unitOfMeasurement": "kB",
            "measurements": [18000.0, 18100.0, 18200.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_Memory.physical_peak",
            "unitOfMeasurement": "kB",
            "measurements": [19100.0, 19200.0, 19300.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_Storage.logical_writes",
            "unitOfMeasurement": "kB",
            "measurements": [0.0, 0.0, 0.0]
          }
        ]
      }
    ]
  }
]
"#;
    let stdout = concat!(
        "2026-03-30 08:22:25.517 ReactNativeCameraBench[123:456] OXIDE_CAMERA_CONTRACT_SUMMARY ",
        "{\"requestedHeight\":720,\"source\":\"react-native-vision-camera\",\"devicePosition\":\"back\",\"requestedWidth\":1280,\"transport\":\"native-preview-view\",\"sessionPreset\":\"format:1280x720\",\"activeWidth\":1280,\"activeHeight\":720,\"requestedFps\":30,\"activeFps\":30.0,\"requestedPixelFormat\":\"yuv\",\"activePixelFormat\":\"yuv\",\"videoRange\":\"unknown\",\"colorSpace\":\"unknown\",\"wideColorAuto\":false,\"mirrored\":false}\n",
        "2026-03-30 08:22:25.517 ReactNativeCameraBench[123:456] OXIDE_READY\n"
    );

    let report = parse_react_native_device_report_json(
        metrics_json,
        stdout,
        "Victor's iPhone",
        "ReactNativeCameraBench",
    )
    .expect("parse React Native device report");
    assert_eq!(report.suite, "react-native-device");
    assert_eq!(report.cases.len(), 1);
    assert_eq!(
        report.cases[0].id,
        "react_native.cross_platform.image_pipeline.camera_preview.vision_camera_live"
    );
    assert_eq!(report.cases[0].layer, "cross_platform");
    assert_eq!(report.cases[0].scenario, "camera_preview");
    assert_eq!(report.cases[0].variant, "react_native_vision_camera");
    assert_eq!(report.cases[0].refresh_mode, "native");
    assert_eq!(report.cases[0].median, 0.74);
    assert_eq!(report.cases[0].metrics["cpu_time_s"], 0.003);
    assert_eq!(report.cases[0].metrics["memory_peak_kb"], 19200.0);
    assert!(report.cases[0]
        .notes
        .iter()
        .any(|note| note.contains("source=react-native-vision-camera")));
    assert!(report.cases[0]
        .notes
        .iter()
        .any(|note| note.contains("system-managed native preview-view transport")));
    assert!(report.cases[0].notes.iter().any(|note| note.contains("1280x720@30 YUV-family")));
    assert_eq!(report.coverage.image_pipeline_total, 1);
    assert_eq!(report.contract.notes[0], "Scheme: ReactNativeCameraBenchPerf");
    assert!(report.contract.notes.iter().any(|note| note.contains("PerfWorkload signposts")));
}

#[test]
fn apply_xctestrun_environment_overrides_updates_both_environment_sections() {
    let mut xctestrun = PlValue::Dictionary(Dictionary::from_iter([(
        String::from("ReactNativeCameraBenchPerfTests"),
        PlValue::Dictionary(Dictionary::from_iter([
            (
                String::from("EnvironmentVariables"),
                PlValue::Dictionary(Dictionary::from_iter([(
                    String::from("TERM"),
                    PlValue::String(String::from("dumb")),
                )])),
            ),
            (
                String::from("TestingEnvironmentVariables"),
                PlValue::Dictionary(Dictionary::from_iter([(
                    String::from("XCODE_SCHEME_NAME"),
                    PlValue::String(String::from("ReactNativeCameraBenchPerf")),
                )])),
            ),
        ])),
    )]));

    apply_xctestrun_environment_overrides(
        &mut xctestrun,
        "ReactNativeCameraBenchPerfTests",
        &[
            (String::from("OXIDE_PERF_TRACE_HANDSHAKE"), String::from("1")),
            (String::from("OXIDE_PERF_BENCHMARK_ITERATIONS"), String::from("24")),
            (String::from("MTL_HUD_ENABLED"), String::from("0")),
        ],
    )
    .expect("apply xctestrun environment overrides");

    let root = xctestrun.as_dictionary().expect("root dictionary");
    let target = root
        .get("ReactNativeCameraBenchPerfTests")
        .and_then(PlValue::as_dictionary)
        .expect("target dictionary");
    for section_name in ["EnvironmentVariables", "TestingEnvironmentVariables"] {
        let section = target
            .get(section_name)
            .and_then(PlValue::as_dictionary)
            .expect("environment section dictionary");
        assert_eq!(
            section.get("OXIDE_PERF_TRACE_HANDSHAKE").and_then(PlValue::as_string),
            Some("1")
        );
        assert_eq!(
            section.get("OXIDE_PERF_BENCHMARK_ITERATIONS").and_then(PlValue::as_string),
            Some("24")
        );
        assert_eq!(section.get("MTL_HUD_ENABLED").and_then(PlValue::as_string), Some("0"));
    }
}

#[test]
fn prepare_uikit_device_perf_xctestrun_updates_perf_and_ui_test_targets() {
    let dir = tempdir().expect("tempdir");
    let source_path = dir.path().join("OxideUIKitPerf_iphoneos26.4-arm64.xctestrun");
    let xctestrun = PlValue::Dictionary(Dictionary::from_iter([
        (
            String::from("OxideHostPerfTests"),
            PlValue::Dictionary(Dictionary::from_iter([
                (String::from("EnvironmentVariables"), PlValue::Dictionary(Dictionary::new())),
                (
                    String::from("TestingEnvironmentVariables"),
                    PlValue::Dictionary(Dictionary::new()),
                ),
            ])),
        ),
        (
            String::from("OxideHostUITests"),
            PlValue::Dictionary(Dictionary::from_iter([
                (String::from("EnvironmentVariables"), PlValue::Dictionary(Dictionary::new())),
                (
                    String::from("TestingEnvironmentVariables"),
                    PlValue::Dictionary(Dictionary::new()),
                ),
            ])),
        ),
    ]));
    plist::to_file_xml(&source_path, &xctestrun).expect("write xctestrun");

    let prepared_path = prepare_uikit_device_perf_xctestrun(
        &source_path,
        &[
            (String::from("OXIDE_PERF_REFRESH_MODE"), String::from("native")),
            (String::from("OXIDE_PERF_CAMERA_MAX_DRAWABLE_COUNT"), String::from("2")),
        ],
    )
    .expect("prepare UIKit xctestrun");
    assert!(prepared_path
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| value.contains("-perf-"))
        .unwrap_or(false));

    let prepared: PlValue = plist::from_file(&prepared_path).expect("read prepared xctestrun");
    let root = prepared.as_dictionary().expect("root dictionary");
    for target_name in ["OxideHostPerfTests", "OxideHostUITests"] {
        let target =
            root.get(target_name).and_then(PlValue::as_dictionary).expect("target dictionary");
        for section_name in ["EnvironmentVariables", "TestingEnvironmentVariables"] {
            let section = target
                .get(section_name)
                .and_then(PlValue::as_dictionary)
                .expect("environment section dictionary");
            assert_eq!(
                section.get("OXIDE_PERF_REFRESH_MODE").and_then(PlValue::as_string),
                Some("native")
            );
            assert_eq!(
                section.get("OXIDE_PERF_CAMERA_MAX_DRAWABLE_COUNT").and_then(PlValue::as_string),
                Some("2")
            );
        }
    }
}

#[test]
fn prepare_uikit_device_perf_xctestrun_reuses_same_hashed_path_for_same_environment() {
    let dir = tempdir().expect("tempdir");
    let source_path = dir.path().join("OxideUIKitPerf_iphoneos26.4-arm64.xctestrun");
    let xctestrun = PlValue::Dictionary(Dictionary::from_iter([(
        String::from("OxideHostPerfTests"),
        PlValue::Dictionary(Dictionary::from_iter([
            (String::from("EnvironmentVariables"), PlValue::Dictionary(Dictionary::new())),
            (String::from("TestingEnvironmentVariables"), PlValue::Dictionary(Dictionary::new())),
        ])),
    )]));
    plist::to_file_xml(&source_path, &xctestrun).expect("write xctestrun");

    let environment = vec![(String::from("OXIDE_PERF_REFRESH_MODE"), String::from("native"))];
    let first = prepare_uikit_device_perf_xctestrun(&source_path, &environment)
        .expect("first prepared xctestrun");
    let second = prepare_uikit_device_perf_xctestrun(&source_path, &environment)
        .expect("second prepared xctestrun");

    assert_eq!(first, second);
}

#[test]
fn prepare_resumable_uikit_device_result_root_keeps_matching_checkpoints() {
    let dir = tempdir().expect("tempdir");
    let result_root = dir.path().join("result-root");
    let derived_data = result_root.join("derived-data");
    fs::create_dir_all(&derived_data).expect("create derived-data");
    let stamp = UIKitHostBuildStamp {
        destination: String::from("platform=iOS,id=device"),
        development_team: String::from("TEAM123456"),
        source_fingerprint: 1,
    };

    prepare_resumable_uikit_device_result_root(
        &result_root,
        &[derived_data.as_path()],
        &stamp,
        "UIKit device",
    )
    .expect("seed result root");

    let case_dir = result_root.join("uikit").join("testLabelEncode-native");
    fs::create_dir_all(&case_dir).expect("create case dir");
    fs::write(case_dir.join("case.json"), "{}").expect("write case checkpoint");

    prepare_resumable_uikit_device_result_root(
        &result_root,
        &[derived_data.as_path()],
        &stamp,
        "UIKit device",
    )
    .expect("reuse matching result root");

    assert!(case_dir.join("case.json").is_file());
    assert!(derived_data.exists());
}

#[test]
fn prepare_resumable_uikit_device_result_root_clears_stale_checkpoints_on_stamp_change() {
    let dir = tempdir().expect("tempdir");
    let result_root = dir.path().join("result-root");
    let derived_data = result_root.join("derived-data");
    fs::create_dir_all(&derived_data).expect("create derived-data");
    let old_stamp = UIKitHostBuildStamp {
        destination: String::from("platform=iOS,id=device"),
        development_team: String::from("TEAM123456"),
        source_fingerprint: 1,
    };
    let new_stamp = UIKitHostBuildStamp {
        destination: String::from("platform=iOS,id=device"),
        development_team: String::from("TEAM123456"),
        source_fingerprint: 2,
    };

    prepare_resumable_uikit_device_result_root(
        &result_root,
        &[derived_data.as_path()],
        &old_stamp,
        "UIKit device",
    )
    .expect("seed result root");

    let case_dir = result_root.join("uikit").join("testAnimTimelineBars-native");
    fs::create_dir_all(&case_dir).expect("create case dir");
    fs::write(case_dir.join("case.json"), "{}").expect("write stale case checkpoint");

    prepare_resumable_uikit_device_result_root(
        &result_root,
        &[derived_data.as_path()],
        &new_stamp,
        "UIKit device",
    )
    .expect("clear stale result root");

    assert!(!case_dir.exists());
    assert!(derived_data.exists());
}

#[test]
fn uikit_device_perf_environment_for_real_app_camera_cases_includes_case_specific_flags() {
    let custom_env = uikit_device_perf_environment_for_test_name(
        "testCameraNV12LegacyRealAppLivePreview",
        "native",
    )
    .expect("custom real-app xctestrun environment");
    let custom_map: BTreeMap<String, String> = custom_env.into_iter().collect();
    assert_eq!(custom_map.get("OXIDE_PERF_REFRESH_MODE").map(String::as_str), Some("native"));
    assert_eq!(custom_map.get("OXIDE_RENDER_IN_TEST").map(String::as_str), Some("1"));
    assert_eq!(custom_map.get("OXIDE_PERF_CAMERA_REAL_APP_HOST").map(String::as_str), Some("1"));
    assert!(!custom_map.contains_key("OXIDE_PERF_CAMERA_REAL_APP_HYBRID_VISIBLE_PREVIEW"));

    let hybrid_env = uikit_device_perf_environment_for_test_name(
        "testCameraNV12LegacyRealAppHybridPreviewLayerLivePreview",
        "native",
    )
    .expect("hybrid real-app xctestrun environment");
    let hybrid_map: BTreeMap<String, String> = hybrid_env.into_iter().collect();
    assert_eq!(hybrid_map.get("OXIDE_RENDER_IN_TEST").map(String::as_str), Some("1"));
    assert_eq!(hybrid_map.get("OXIDE_PERF_CAMERA_REAL_APP_HOST").map(String::as_str), Some("1"));
    assert_eq!(
        hybrid_map.get("OXIDE_PERF_CAMERA_REAL_APP_HYBRID_VISIBLE_PREVIEW").map(String::as_str),
        Some("1")
    );
}

#[test]
fn is_primary_built_xctestrun_file_ignores_generated_perf_copies() {
    assert!(is_primary_built_xctestrun_file(
        "ReactNativeCameraBenchPerf_iphoneos26.4-arm64.xctestrun",
        "ReactNativeCameraBenchPerf"
    ));
    assert!(!is_primary_built_xctestrun_file(
        "ReactNativeCameraBenchPerf_iphoneos26.4-arm64-perf.xctestrun",
        "ReactNativeCameraBenchPerf"
    ));
    assert!(!is_primary_built_xctestrun_file(
        "ReactNativeCameraBenchPerf_iphoneos26.4-arm64-perf-deadbeefcafebabe.xctestrun",
        "ReactNativeCameraBenchPerf"
    ));
    assert!(!is_primary_built_xctestrun_file(
        "OtherScheme_iphoneos26.4-arm64.xctestrun",
        "ReactNativeCameraBenchPerf"
    ));
}

#[test]
fn devicectl_notification_observed_requires_observed_line() {
    let observed = "Darwin notification observation started.\n• Mar 29, 2026 at 15:27:42 : Observed 'com.oxide.perf.complete'\n";
    let timed_out = "Darwin notification observation started. 30 seconds remaining:\n";

    assert!(devicectl_notification_observed(observed, "com.oxide.perf.complete"));
    assert!(!devicectl_notification_observed(timed_out, "com.oxide.perf.complete"));
}

#[test]
fn parse_devicectl_lock_state_text_reads_passcode_required() {
    let unlocked = r#"
Current device lock state:
• deviceIdentifier: 1DEDF2A3-EC8E-5FCC-A437-8BD3A6F3D659
• passcodeRequired: false
• unlockedSinceBoot: true
"#;
    assert!(!parse_devicectl_lock_state_text(unlocked).expect("unlocked parse"));

    let locked = unlocked.replace("false", "true");
    assert!(parse_devicectl_lock_state_text(&locked).expect("locked parse"));
}

#[test]
fn parse_devicectl_display_backlight_active_reads_backlight_state() {
    let active = r#"
Current Displays:
▿ 1: LCD (primary):
Main display backlight state: backlight is on and active
"#;
    assert!(parse_devicectl_display_backlight_active(active).expect("active parse"));

    let inactive = active.replace("backlight is on and active", "backlight is off");
    assert!(!parse_devicectl_display_backlight_active(&inactive).expect("inactive parse"));
}

#[test]
fn console_output_contains_marker_matches_exact_line() {
    let stdout = "OXIDE_READY testCameraNV12LegacyLivePreview\nnoise\nOXIDE_COMPLETE testCameraNV12LegacyLivePreview\n";

    assert!(console_output_contains_marker(stdout, "OXIDE_READY testCameraNV12LegacyLivePreview"));
    assert!(console_output_contains_marker(
        stdout,
        "OXIDE_COMPLETE testCameraNV12LegacyLivePreview"
    ));
    assert!(!console_output_contains_marker(stdout, "OXIDE_STAGE_SUMMARY"));
}

#[test]
fn notification_or_console_marker_observed_accepts_console_fallback() {
    let notification_stdout = "Darwin notification observation started. 30 seconds remaining:\n";
    let console_stdout =
        "OXIDE_READY testCameraNV12LegacyLivePreview\nOXIDE_COMPLETE testCameraNV12LegacyLivePreview\n";

    assert!(notification_or_console_marker_observed(
        notification_stdout,
        "com.oxide.perf.ready",
        console_stdout,
        "OXIDE_READY testCameraNV12LegacyLivePreview"
    ));
    assert!(!notification_or_console_marker_observed(
        notification_stdout,
        "com.oxide.perf.ready",
        "banner\n",
        "OXIDE_READY testCameraNV12LegacyLivePreview"
    ));
}

#[test]
fn latest_benchmark_build_failure_returns_last_failure_line() {
    let stdout = "\
OXIDE_STAGE one\n\
OXIDE_BENCHMARK_BUILD_FAIL failed - first\n\
OXIDE_STAGE two\n\
OXIDE_BENCHMARK_BUILD_FAIL failed - second\n";

    assert_eq!(latest_benchmark_build_failure(stdout).as_deref(), Some("failed - second"));
}

#[test]
fn device_console_failure_line_returns_last_parked_or_runner_failure() {
    let stdout = "\
OXIDE_STAGE parked.fail.foreground failed - parked benchmark lost active foreground state\n\
noise\n\
OXIDE_COMPLETE oxide-perf-runner failed\n";

    assert_eq!(
        device_console_failure_line(stdout).as_deref(),
        Some("OXIDE_COMPLETE oxide-perf-runner failed")
    );
    assert_eq!(device_console_failure_line("OXIDE_COMPLETE testCamera\n"), None);
}

#[test]
fn latest_benchmark_build_failure_ignores_unrelated_output() {
    let stdout = "OXIDE_READY testLabelEncode\nOXIDE_COMPLETE testLabelEncode\n";

    assert_eq!(latest_benchmark_build_failure(stdout), None);
}

#[test]
fn start_console_marker_or_completion_observed_accepts_start_marker() {
    let console_stdout = "OXIDE_READY testCameraNV12LegacyLivePreview\nOXIDE_START testCameraNV12LegacyLivePreview\n";

    assert!(start_console_marker_or_completion_observed(
        "",
        "com.oxide.perf.complete",
        console_stdout,
        "OXIDE_START testCameraNV12LegacyLivePreview",
        "OXIDE_COMPLETE testCameraNV12LegacyLivePreview"
    ));
}

#[test]
fn start_console_marker_or_completion_observed_accepts_completion_fallback() {
    let completion_stdout =
        "Darwin notification observation started.\n• Apr 2, 2026 at 02:26:54 : Observed 'com.oxide.perf.complete'\n";
    let console_stdout = "OXIDE_READY testCameraNV12LegacyLivePreview\n";

    assert!(start_console_marker_or_completion_observed(
        completion_stdout,
        "com.oxide.perf.complete",
        console_stdout,
        "OXIDE_START testCameraNV12LegacyLivePreview",
        "OXIDE_COMPLETE testCameraNV12LegacyLivePreview"
    ));
}

#[test]
fn extract_oxide_device_report_json_rejects_missing_markers() {
    let err = extract_oxide_device_report_json("OXIDE_PERF_REPORT_CHUNK abc")
        .expect_err("missing markers should fail");
    assert!(err.to_string().contains("OXIDE_PERF_REPORT_BEGIN"));
}

#[test]
fn parse_uikit_report_json_classifies_new_primitive_lifecycle_cases() {
    let json = r#"
[
  {
    "testIdentifier": "OxideHostPerfTests/testControlSetMount()",
    "testRuns": [
      {
        "device": {
          "deviceName": "iPhone 16"
        },
        "metrics": [
          {
            "identifier": "com.apple.dt.XCTMetric_Clock.time.monotonic",
            "unitOfMeasurement": "s",
            "measurements": [0.010, 0.011, 0.012]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.time",
            "unitOfMeasurement": "s",
            "measurements": [0.020, 0.021, 0.022]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.cycles",
            "unitOfMeasurement": "kC",
            "measurements": [10.0, 11.0, 12.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.instructions_retired",
            "unitOfMeasurement": "kI",
            "measurements": [13.0, 14.0, 15.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_Memory.physical",
            "unitOfMeasurement": "kB",
            "measurements": [16.0, 17.0, 18.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_Memory.physical_peak",
            "unitOfMeasurement": "kB",
            "measurements": [19.0, 20.0, 21.0]
          }
        ]
      }
    ]
  }
]
"#;

    let report = parse_uikit_report_json(json).expect("parse UIKit primitive lifecycle report");
    assert_eq!(report.cases[0].id, "uikit.idiomatic.primitive.control_set.mount");
    assert_eq!(report.cases[0].oxide_case_id, "cpu.primitive.control_set.mount");
    assert_eq!(report.cases[0].layer, "engine");
    assert_eq!(report.cases[0].scenario, "primitive-lifecycle");
    assert_eq!(report.cases[0].style, "idiomatic");
}

#[test]
fn parse_uikit_report_json_classifies_navigation_and_reconcile_cases() {
    let json = r#"
[
  {
    "testIdentifier": "OxideHostPerfTests/testButtonPressResponse()",
    "testRuns": [
      {
        "device": {
          "deviceName": "iPhone 16"
        },
        "metrics": [
          {
            "identifier": "com.apple.dt.XCTMetric_Clock.time.monotonic",
            "unitOfMeasurement": "s",
            "measurements": [0.010, 0.011, 0.012]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.time",
            "unitOfMeasurement": "s",
            "measurements": [0.020, 0.021, 0.022]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.cycles",
            "unitOfMeasurement": "kC",
            "measurements": [10.0, 11.0, 12.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.instructions_retired",
            "unitOfMeasurement": "kI",
            "measurements": [13.0, 14.0, 15.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_Memory.physical",
            "unitOfMeasurement": "kB",
            "measurements": [16.0, 17.0, 18.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_Memory.physical_peak",
            "unitOfMeasurement": "kB",
            "measurements": [19.0, 20.0, 21.0]
          }
        ]
      }
    ]
  },
  {
    "testIdentifier": "OxideHostPerfTests/testThemeSwapFull()",
    "testRuns": [
      {
        "device": {
          "deviceName": "iPhone 16"
        },
        "metrics": [
          {
            "identifier": "com.apple.dt.XCTMetric_Clock.time.monotonic",
            "unitOfMeasurement": "s",
            "measurements": [0.030, 0.031, 0.032]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.time",
            "unitOfMeasurement": "s",
            "measurements": [0.040, 0.041, 0.042]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.cycles",
            "unitOfMeasurement": "kC",
            "measurements": [13.0, 14.0, 15.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_CPU.instructions_retired",
            "unitOfMeasurement": "kI",
            "measurements": [16.0, 17.0, 18.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_Memory.physical",
            "unitOfMeasurement": "kB",
            "measurements": [26.0, 27.0, 28.0]
          },
          {
            "identifier": "com.apple.dt.XCTMetric_Memory.physical_peak",
            "unitOfMeasurement": "kB",
            "measurements": [29.0, 30.0, 31.0]
          }
        ]
      }
    ]
  }
]
"#;

    let report = parse_uikit_report_json(json).expect("parse UIKit navigation/reconcile report");
    assert_eq!(report.cases.len(), 2);
    assert_eq!(report.cases[0].id, "uikit.idiomatic.navigation.button_press.response");
    assert_eq!(report.cases[0].oxide_case_id, "cpu.navigation.button_press.response");
    assert_eq!(report.cases[0].layer, "flow");
    assert_eq!(report.cases[0].scenario, "navigation-input");
    assert_eq!(report.cases[1].id, "uikit.idiomatic.reconcile.theme_swap_full");
    assert_eq!(report.cases[1].oxide_case_id, "cpu.reconcile.theme_swap_full");
    assert_eq!(report.cases[1].layer, "engine");
    assert_eq!(report.cases[1].scenario, "state-reconcile");
}

#[test]
fn compare_uikit_reports_flags_regressions() {
    let current = UIKitPerfReport {
        version: 1,
        suite: String::from("simulator"),
        generated_label: None,
        device_name: String::from("iPhone 16"),
        energy_status: String::from("simulator proxy"),
        contract: UIKitContractCoverageReport::default(),
        notes: Vec::new(),
        cases: vec![UIKitPerfCase {
            id: String::from("uikit.component.label.encode"),
            oxide_case_id: String::from("cpu.component.label.encode"),
            test_name: String::from("testLabelEncode"),
            layer: String::from("engine"),
            scenario: String::from("primitive-view"),
            style: String::from("idiomatic"),
            cache_state: String::from("warm"),
            refresh_mode: String::from("simulator-default"),
            threshold_pct: 0.10,
            notes: Vec::new(),
            metrics: BTreeMap::from([
                (String::from("clock_s"), sample_metric(1.2)),
                (String::from("cpu_time_s"), sample_metric(1.1)),
                (String::from("cpu_cycles_kc"), sample_metric(1.05)),
                (String::from("memory_peak_kb"), sample_metric(1.0)),
            ]),
            ..UIKitPerfCase::default()
        }],
    };

    let baseline = UIKitPerfReport {
        version: 1,
        suite: String::from("simulator"),
        generated_label: None,
        device_name: String::from("iPhone 16"),
        energy_status: String::from("simulator proxy"),
        contract: UIKitContractCoverageReport::default(),
        notes: Vec::new(),
        cases: vec![UIKitPerfCase {
            id: String::from("uikit.component.label.encode"),
            oxide_case_id: String::from("cpu.component.label.encode"),
            test_name: String::from("testLabelEncode"),
            layer: String::from("engine"),
            scenario: String::from("primitive-view"),
            style: String::from("idiomatic"),
            cache_state: String::from("warm"),
            refresh_mode: String::from("simulator-default"),
            threshold_pct: 0.10,
            notes: Vec::new(),
            metrics: BTreeMap::from([
                (String::from("clock_s"), sample_metric(1.0)),
                (String::from("cpu_time_s"), sample_metric(1.0)),
                (String::from("cpu_cycles_kc"), sample_metric(1.0)),
                (String::from("memory_peak_kb"), sample_metric(1.0)),
            ]),
            ..UIKitPerfCase::default()
        }],
    };

    let comparison = compare_uikit_reports(&current, &baseline);
    assert_eq!(comparison.matched, 1);
    assert_eq!(comparison.regressions.len(), 1);
    assert_eq!(comparison.regressions[0].metric, "clock_s");
}

#[test]
fn compare_uikit_reports_allows_tiny_simulator_bridge_jitter() {
    let current = UIKitPerfReport {
        version: 1,
        suite: String::from("simulator"),
        generated_label: None,
        device_name: String::from("iPhone 16"),
        energy_status: String::from("simulator proxy"),
        contract: UIKitContractCoverageReport::default(),
        notes: Vec::new(),
        cases: vec![UIKitPerfCase {
            id: String::from("uikit.bridge.sensor_location_snapshot"),
            oxide_case_id: String::from("cpu.bridge.sensor_location_snapshot"),
            test_name: String::from("testSensorLocationSnapshot"),
            layer: String::from("bridge"),
            scenario: String::from("os-bridge"),
            style: String::from("idiomatic"),
            cache_state: String::from("warm"),
            refresh_mode: String::from("simulator-default"),
            threshold_pct: 0.20,
            notes: Vec::new(),
            metrics: BTreeMap::from([
                (String::from("clock_s"), sample_metric(0.000719)),
                (String::from("cpu_time_s"), sample_metric(0.000719)),
                (String::from("cpu_cycles_kc"), sample_metric(2386.376)),
                (String::from("memory_peak_kb"), sample_metric(72125.224)),
            ]),
            ..UIKitPerfCase::default()
        }],
    };

    let baseline = UIKitPerfReport {
        version: 1,
        suite: String::from("simulator"),
        generated_label: None,
        device_name: String::from("iPhone 16"),
        energy_status: String::from("simulator proxy"),
        contract: UIKitContractCoverageReport::default(),
        notes: Vec::new(),
        cases: vec![UIKitPerfCase {
            id: String::from("uikit.bridge.sensor_location_snapshot"),
            oxide_case_id: String::from("cpu.bridge.sensor_location_snapshot"),
            test_name: String::from("testSensorLocationSnapshot"),
            layer: String::from("bridge"),
            scenario: String::from("os-bridge"),
            style: String::from("idiomatic"),
            cache_state: String::from("warm"),
            refresh_mode: String::from("simulator-default"),
            threshold_pct: 0.20,
            notes: Vec::new(),
            metrics: BTreeMap::from([
                (String::from("clock_s"), sample_metric(0.000595)),
                (String::from("cpu_time_s"), sample_metric(0.000595)),
                (String::from("cpu_cycles_kc"), sample_metric(1975.153)),
                (String::from("memory_peak_kb"), sample_metric(72059.688)),
            ]),
            ..UIKitPerfCase::default()
        }],
    };

    let comparison = compare_uikit_reports(&current, &baseline);
    assert_eq!(comparison.matched, 1);
    assert!(comparison.regressions.is_empty());
}

#[test]
fn compare_uikit_reports_ignores_simulator_peak_memory_drift() {
    let current = UIKitPerfReport {
        version: 1,
        suite: String::from("simulator"),
        generated_label: None,
        device_name: String::from("iPhone 16"),
        energy_status: String::from("simulator proxy"),
        contract: UIKitContractCoverageReport::default(),
        notes: Vec::new(),
        cases: vec![UIKitPerfCase {
            id: String::from("uikit.bridge.permission_callback_fanout"),
            oxide_case_id: String::from("cpu.bridge.permission_callback_fanout"),
            test_name: String::from("testPermissionCallbackBridge"),
            layer: String::from("bridge"),
            scenario: String::from("os-bridge"),
            style: String::from("idiomatic"),
            cache_state: String::from("warm"),
            refresh_mode: String::from("simulator-default"),
            threshold_pct: 0.20,
            notes: Vec::new(),
            metrics: BTreeMap::from([
                (String::from("clock_s"), sample_metric(0.000128)),
                (String::from("cpu_time_s"), sample_metric(0.000703)),
                (String::from("cpu_cycles_kc"), sample_metric(2321.346)),
                (String::from("memory_peak_kb"), sample_metric(108809.216)),
            ]),
            ..UIKitPerfCase::default()
        }],
    };

    let baseline = UIKitPerfReport {
        version: 1,
        suite: String::from("simulator"),
        generated_label: None,
        device_name: String::from("iPhone 16"),
        energy_status: String::from("simulator proxy"),
        contract: UIKitContractCoverageReport::default(),
        notes: Vec::new(),
        cases: vec![UIKitPerfCase {
            id: String::from("uikit.bridge.permission_callback_fanout"),
            oxide_case_id: String::from("cpu.bridge.permission_callback_fanout"),
            test_name: String::from("testPermissionCallbackBridge"),
            layer: String::from("bridge"),
            scenario: String::from("os-bridge"),
            style: String::from("idiomatic"),
            cache_state: String::from("warm"),
            refresh_mode: String::from("simulator-default"),
            threshold_pct: 0.20,
            notes: Vec::new(),
            metrics: BTreeMap::from([
                (String::from("clock_s"), sample_metric(0.000115)),
                (String::from("cpu_time_s"), sample_metric(0.000640)),
                (String::from("cpu_cycles_kc"), sample_metric(2012.978)),
                (String::from("memory_peak_kb"), sample_metric(73583.640)),
            ]),
            ..UIKitPerfCase::default()
        }],
    };

    let comparison = compare_uikit_reports(&current, &baseline);
    assert_eq!(comparison.matched, 1);
    assert!(comparison.regressions.is_empty());
}

#[test]
fn compare_uikit_reports_allows_tiny_simulator_component_jitter() {
    let current = UIKitPerfReport {
        version: 1,
        suite: String::from("simulator"),
        generated_label: None,
        device_name: String::from("iPhone 16"),
        energy_status: String::from("simulator proxy"),
        contract: UIKitContractCoverageReport::default(),
        notes: Vec::new(),
        cases: vec![UIKitPerfCase {
            id: String::from("uikit.component.image_view.encode"),
            oxide_case_id: String::from("cpu.component.image_view.encode"),
            test_name: String::from("testImageViewEncode"),
            layer: String::from("engine"),
            scenario: String::from("primitive-view"),
            style: String::from("idiomatic"),
            cache_state: String::from("warm"),
            refresh_mode: String::from("simulator-default"),
            threshold_pct: 0.20,
            notes: Vec::new(),
            metrics: BTreeMap::from([
                (String::from("clock_s"), sample_metric(0.001314)),
                (String::from("cpu_time_s"), sample_metric(0.001620)),
                (String::from("cpu_cycles_kc"), sample_metric(4870.673)),
                (String::from("memory_peak_kb"), sample_metric(108809.216)),
            ]),
            ..UIKitPerfCase::default()
        }],
    };

    let baseline = UIKitPerfReport {
        version: 1,
        suite: String::from("simulator"),
        generated_label: None,
        device_name: String::from("iPhone 16"),
        energy_status: String::from("simulator proxy"),
        contract: UIKitContractCoverageReport::default(),
        notes: Vec::new(),
        cases: vec![UIKitPerfCase {
            id: String::from("uikit.component.image_view.encode"),
            oxide_case_id: String::from("cpu.component.image_view.encode"),
            test_name: String::from("testImageViewEncode"),
            layer: String::from("engine"),
            scenario: String::from("primitive-view"),
            style: String::from("idiomatic"),
            cache_state: String::from("warm"),
            refresh_mode: String::from("simulator-default"),
            threshold_pct: 0.20,
            notes: Vec::new(),
            metrics: BTreeMap::from([
                (String::from("clock_s"), sample_metric(0.001057)),
                (String::from("cpu_time_s"), sample_metric(0.001307)),
                (String::from("cpu_cycles_kc"), sample_metric(3950.752)),
                (String::from("memory_peak_kb"), sample_metric(73665.560)),
            ]),
            ..UIKitPerfCase::default()
        }],
    };

    let comparison = compare_uikit_reports(&current, &baseline);
    assert_eq!(comparison.matched, 1);
    assert!(comparison.regressions.is_empty());
}

#[test]
fn compare_uikit_reports_allows_small_simulator_microbench_jitter() {
    let current = UIKitPerfReport {
        version: 1,
        suite: String::from("simulator"),
        generated_label: None,
        device_name: String::from("iPhone 16"),
        energy_status: String::from("simulator proxy"),
        contract: UIKitContractCoverageReport::default(),
        notes: Vec::new(),
        cases: vec![UIKitPerfCase {
            id: String::from("uikit.idiomatic.primitive.empty_root.mount"),
            oxide_case_id: String::from("cpu.primitive.empty_root.mount"),
            test_name: String::from("testEmptyRootMount"),
            layer: String::from("engine"),
            scenario: String::from("primitive-lifecycle"),
            style: String::from("idiomatic"),
            cache_state: String::from("warm"),
            refresh_mode: String::from("simulator-default"),
            threshold_pct: 0.20,
            notes: Vec::new(),
            metrics: BTreeMap::from([
                (String::from("clock_s"), sample_metric(0.0072)),
                (String::from("cpu_time_s"), sample_metric(0.0072)),
                (String::from("cpu_cycles_kc"), sample_metric(23668.251)),
                (String::from("memory_peak_kb"), sample_metric(85806.152)),
            ]),
            ..UIKitPerfCase::default()
        }],
    };

    let baseline = UIKitPerfReport {
        version: 1,
        suite: String::from("simulator"),
        generated_label: None,
        device_name: String::from("iPhone 16"),
        energy_status: String::from("simulator proxy"),
        contract: UIKitContractCoverageReport::default(),
        notes: Vec::new(),
        cases: vec![UIKitPerfCase {
            id: String::from("uikit.idiomatic.primitive.empty_root.mount"),
            oxide_case_id: String::from("cpu.primitive.empty_root.mount"),
            test_name: String::from("testEmptyRootMount"),
            layer: String::from("engine"),
            scenario: String::from("primitive-lifecycle"),
            style: String::from("idiomatic"),
            cache_state: String::from("warm"),
            refresh_mode: String::from("simulator-default"),
            threshold_pct: 0.20,
            notes: Vec::new(),
            metrics: BTreeMap::from([
                (String::from("clock_s"), sample_metric(0.00595)),
                (String::from("cpu_time_s"), sample_metric(0.00600)),
                (String::from("cpu_cycles_kc"), sample_metric(19177.811)),
                (String::from("memory_peak_kb"), sample_metric(85806.152)),
            ]),
            ..UIKitPerfCase::default()
        }],
    };

    let comparison = compare_uikit_reports(&current, &baseline);
    assert_eq!(comparison.matched, 1);
    assert!(comparison.regressions.is_empty());
}

#[test]
fn compare_uikit_reports_still_flags_small_simulator_microbench_regressions() {
    let current = UIKitPerfReport {
        version: 1,
        suite: String::from("simulator"),
        generated_label: None,
        device_name: String::from("iPhone 16"),
        energy_status: String::from("simulator proxy"),
        contract: UIKitContractCoverageReport::default(),
        notes: Vec::new(),
        cases: vec![UIKitPerfCase {
            id: String::from("uikit.optimized.primitive.labels.10.mutate_text"),
            oxide_case_id: String::from("cpu.primitive.labels.10.mutate_text"),
            test_name: String::from("testOptimizedLabels10Mutate"),
            layer: String::from("engine"),
            scenario: String::from("primitive-lifecycle"),
            style: String::from("optimized"),
            cache_state: String::from("warm"),
            refresh_mode: String::from("simulator-default"),
            threshold_pct: 0.20,
            notes: Vec::new(),
            metrics: BTreeMap::from([
                (String::from("clock_s"), sample_metric(0.0138)),
                (String::from("cpu_time_s"), sample_metric(0.0140)),
                (String::from("cpu_cycles_kc"), sample_metric(28000.0)),
                (String::from("memory_peak_kb"), sample_metric(143445.064)),
            ]),
            ..UIKitPerfCase::default()
        }],
    };

    let baseline = UIKitPerfReport {
        version: 1,
        suite: String::from("simulator"),
        generated_label: None,
        device_name: String::from("iPhone 16"),
        energy_status: String::from("simulator proxy"),
        contract: UIKitContractCoverageReport::default(),
        notes: Vec::new(),
        cases: vec![UIKitPerfCase {
            id: String::from("uikit.optimized.primitive.labels.10.mutate_text"),
            oxide_case_id: String::from("cpu.primitive.labels.10.mutate_text"),
            test_name: String::from("testOptimizedLabels10Mutate"),
            layer: String::from("engine"),
            scenario: String::from("primitive-lifecycle"),
            style: String::from("optimized"),
            cache_state: String::from("warm"),
            refresh_mode: String::from("simulator-default"),
            threshold_pct: 0.20,
            notes: Vec::new(),
            metrics: BTreeMap::from([
                (String::from("clock_s"), sample_metric(0.0101)),
                (String::from("cpu_time_s"), sample_metric(0.0108)),
                (String::from("cpu_cycles_kc"), sample_metric(21000.0)),
                (String::from("memory_peak_kb"), sample_metric(143445.064)),
            ]),
            ..UIKitPerfCase::default()
        }],
    };

    let comparison = compare_uikit_reports(&current, &baseline);
    assert_eq!(comparison.matched, 1);
    assert_eq!(comparison.regressions.len(), 3);
}

#[test]
fn compare_uikit_reports_ignores_spinner_encode_simulator_clock_jitter() {
    let current = UIKitPerfReport {
        version: 1,
        suite: String::from("simulator"),
        generated_label: None,
        device_name: String::from("iPhone 16"),
        energy_status: String::from("simulator proxy"),
        contract: UIKitContractCoverageReport::default(),
        notes: Vec::new(),
        cases: vec![UIKitPerfCase {
            id: String::from("uikit.component.spinner.encode"),
            oxide_case_id: String::from("cpu.component.spinner.encode"),
            test_name: String::from("testSpinnerEncode"),
            layer: String::from("engine"),
            scenario: String::from("primitive-view"),
            style: String::from("idiomatic"),
            cache_state: String::from("warm"),
            refresh_mode: String::from("simulator-default"),
            threshold_pct: 0.20,
            notes: Vec::new(),
            metrics: BTreeMap::from([
                (String::from("clock_s"), sample_metric(0.0129)),
                (String::from("cpu_time_s"), sample_metric(0.00282)),
                (String::from("cpu_cycles_kc"), sample_metric(8622.283)),
                (String::from("memory_peak_kb"), sample_metric(74353.688)),
            ]),
            ..UIKitPerfCase::default()
        }],
    };

    let baseline = UIKitPerfReport {
        version: 1,
        suite: String::from("simulator"),
        generated_label: None,
        device_name: String::from("iPhone 16"),
        energy_status: String::from("simulator proxy"),
        contract: UIKitContractCoverageReport::default(),
        notes: Vec::new(),
        cases: vec![UIKitPerfCase {
            id: String::from("uikit.component.spinner.encode"),
            oxide_case_id: String::from("cpu.component.spinner.encode"),
            test_name: String::from("testSpinnerEncode"),
            layer: String::from("engine"),
            scenario: String::from("primitive-view"),
            style: String::from("idiomatic"),
            cache_state: String::from("warm"),
            refresh_mode: String::from("simulator-default"),
            threshold_pct: 0.20,
            notes: Vec::new(),
            metrics: BTreeMap::from([
                (String::from("clock_s"), sample_metric(0.003421163)),
                (String::from("cpu_time_s"), sample_metric(0.00202868)),
                (String::from("cpu_cycles_kc"), sample_metric(6329.844)),
                (String::from("memory_peak_kb"), sample_metric(83758.2)),
            ]),
            ..UIKitPerfCase::default()
        }],
    };

    let comparison = compare_uikit_reports(&current, &baseline);
    assert_eq!(comparison.matched, 1);
    assert!(comparison.regressions.is_empty());
}

#[test]
fn compare_uikit_reports_ignores_button_press_response_simulator_clock_jitter() {
    let current = UIKitPerfReport {
        version: 1,
        suite: String::from("simulator"),
        generated_label: None,
        device_name: String::from("iPhone 16"),
        energy_status: String::from("simulator proxy"),
        contract: UIKitContractCoverageReport::default(),
        notes: Vec::new(),
        cases: vec![UIKitPerfCase {
            id: String::from("uikit.idiomatic.navigation.button_press.response"),
            oxide_case_id: String::from("cpu.navigation.button_press.response"),
            test_name: String::from("testButtonPressResponse"),
            layer: String::from("flow"),
            scenario: String::from("navigation-input"),
            style: String::from("idiomatic"),
            cache_state: String::from("warm"),
            refresh_mode: String::from("simulator-default"),
            threshold_pct: 0.20,
            notes: Vec::new(),
            metrics: BTreeMap::from([
                (String::from("clock_s"), sample_metric(0.051545487)),
                (String::from("cpu_time_s"), sample_metric(0.015012068)),
                (String::from("cpu_cycles_kc"), sample_metric(48298.675)),
                (String::from("memory_peak_kb"), sample_metric(38619.824)),
            ]),
            ..UIKitPerfCase::default()
        }],
    };

    let baseline = UIKitPerfReport {
        version: 1,
        suite: String::from("simulator"),
        generated_label: None,
        device_name: String::from("iPhone 16"),
        energy_status: String::from("simulator proxy"),
        contract: UIKitContractCoverageReport::default(),
        notes: Vec::new(),
        cases: vec![UIKitPerfCase {
            id: String::from("uikit.idiomatic.navigation.button_press.response"),
            oxide_case_id: String::from("cpu.navigation.button_press.response"),
            test_name: String::from("testButtonPressResponse"),
            layer: String::from("flow"),
            scenario: String::from("navigation-input"),
            style: String::from("idiomatic"),
            cache_state: String::from("warm"),
            refresh_mode: String::from("simulator-default"),
            threshold_pct: 0.20,
            notes: Vec::new(),
            metrics: BTreeMap::from([
                (String::from("clock_s"), sample_metric(0.03983519)),
                (String::from("cpu_time_s"), sample_metric(0.012528083)),
                (String::from("cpu_cycles_kc"), sample_metric(40348.289)),
                (String::from("memory_peak_kb"), sample_metric(41454.16)),
            ]),
            ..UIKitPerfCase::default()
        }],
    };

    let comparison = compare_uikit_reports(&current, &baseline);
    assert_eq!(comparison.matched, 1);
    assert!(comparison.regressions.is_empty());
}

#[test]
fn parse_available_ios_sim_destination_prefers_pinned_iphone_when_available() {
    let json = r#"
{
  "devices": {
    "com.apple.CoreSimulator.SimRuntime.iOS-18-6": [
      {
        "udid": "PINNED-IPHONE",
        "name": "iPhone 16",
        "state": "Shutdown"
      }
    ],
    "com.apple.CoreSimulator.SimRuntime.iOS-26-0": [
      {
        "udid": "BOOTED-IPHONE",
        "name": "iPhone 17",
        "state": "Booted"
      }
    ]
  }
}
"#;

    let destination = parse_available_ios_sim_destination(json).expect("parse simctl JSON");
    assert_eq!(destination.as_deref(), Some("platform=iOS Simulator,id=PINNED-IPHONE"));
}

#[test]
fn parse_available_ios_sim_destination_prefers_booted_latest_iphone_when_fallback_needed() {
    let json = r#"
{
  "devices": {
    "com.apple.CoreSimulator.SimRuntime.iOS-18-6": [
      {
        "udid": "OLDER-IPHONE",
        "name": "iPhone 15",
        "state": "Shutdown"
      }
    ],
    "com.apple.CoreSimulator.SimRuntime.iOS-26-0": [
      {
        "udid": "LATEST-IPHONE",
        "name": "iPhone 17 Pro",
        "state": "Shutdown"
      },
      {
        "udid": "BOOTED-IPHONE",
        "name": "iPhone 17",
        "state": "Booted"
      },
      {
        "udid": "IPAD",
        "name": "iPad Pro 13-inch (M5)",
        "state": "Shutdown"
      }
    ]
  }
}
"#;

    let destination = parse_available_ios_sim_destination(json).expect("parse simctl JSON");
    assert_eq!(destination.as_deref(), Some("platform=iOS Simulator,id=BOOTED-IPHONE"));
}

#[test]
fn format_uikit_only_testing_identifier_includes_target_class_and_method() {
    let identifier = format_uikit_only_testing_identifier(
        "OxideHostPerfTests",
        "OxideHostPerfTests",
        "testInputFormJourney",
    );
    assert_eq!(identifier, "OxideHostPerfTests/OxideHostPerfTests/testInputFormJourney");
}

#[test]
fn uikit_only_testing_identifier_maps_launch_cases_to_ui_test_target() {
    let identifier = uikit_only_testing_identifier_for_test_name("testSimpleHomeColdLaunch")
        .expect("launch identifier");
    assert_eq!(identifier, "OxideHostUITests/OxideUIKitLaunchPerfTests/testSimpleHomeColdLaunch");
}

#[test]
fn uikit_only_testing_identifier_maps_optimized_launch_cases_to_ui_test_target() {
    let identifier =
        uikit_only_testing_identifier_for_test_name("testOptimizedSimpleHomeColdLaunch")
            .expect("optimized launch identifier");
    assert_eq!(
        identifier,
        "OxideHostUITests/OxideUIKitLaunchPerfTests/testOptimizedSimpleHomeColdLaunch"
    );
}

#[test]
fn uikit_only_testing_identifier_maps_real_app_camera_cases_to_ui_test_target() {
    let custom =
        uikit_only_testing_identifier_for_test_name("testCameraNV12LegacyRealAppLivePreview")
            .expect("real-app custom identifier");
    assert_eq!(
        custom,
        "OxideHostUITests/OxideUIKitLaunchPerfTests/testCameraNV12LegacyRealAppLivePreview"
    );

    let avfoundation = uikit_only_testing_identifier_for_test_name(
        "testCameraAVFoundationPreviewLayerRealAppLivePreview",
    )
    .expect("real-app AVFoundation identifier");
    assert_eq!(
        avfoundation,
        "OxideHostUITests/OxideUIKitLaunchPerfTests/testCameraAVFoundationPreviewLayerRealAppLivePreview"
    );
}

#[test]
fn uikit_perf_environment_uses_launch_env_for_launch_cases() {
    let json = uikit_perf_environment_json_for_test_name("testDetailDeepLinkLaunch", "native")
        .expect("launch environment json");
    assert!(json.contains("\"OXIDE_PERF_UIKIT_LAUNCH\":\"1\""));
    assert!(json.contains("\"OXIDE_PERF_LAUNCH_SCENARIO\":\"detail_route\""));
    assert!(json.contains("\"OXIDE_PERF_LAUNCH_ROUTE\":\"oxide://detail/integration?item=42\""));
    assert!(json.contains("\"OXIDE_PERF_REFRESH_MODE\":\"native\""));
    assert!(json.contains("\"OXIDE_PERF_CAMERA_MAX_DRAWABLE_COUNT\":\"2\""));
    assert!(!json.contains("\"OXIDE_PERF_CAMERA_TRACE_PHASES\""));
    assert!(!json.contains("\"OXIDE_PERF_CASE\""));
}

#[test]
fn uikit_perf_environment_uses_launch_style_for_optimized_launch_cases() {
    let json =
        uikit_perf_environment_json_for_test_name("testOptimizedDetailDeepLinkLaunch", "native")
            .expect("optimized launch environment json");
    assert!(json.contains("\"OXIDE_PERF_UIKIT_LAUNCH\":\"1\""));
    assert!(json.contains("\"OXIDE_PERF_LAUNCH_SCENARIO\":\"detail_route\""));
    assert!(json.contains("\"OXIDE_PERF_LAUNCH_STYLE\":\"optimized\""));
    assert!(json.contains("\"OXIDE_PERF_LAUNCH_ROUTE\":\"oxide://detail/integration?item=42\""));
    assert!(json.contains("\"OXIDE_PERF_REFRESH_MODE\":\"native\""));
}

#[test]
fn uikit_perf_environment_enables_watch_frame_capture_for_watchable_runs() {
    with_env_vars(
        &[
            ("OXIDE_PERF_WATCH_MODE", None),
            ("OXIDE_PERF_FRAME_CAPTURE", None),
            ("OXIDE_PERF_FRAME_CAPTURE_EVERY", None),
            ("OXIDE_PERF_FRAME_CAPTURE_MAX", None),
        ],
        || {
            let json = uikit_perf_environment_json_for_test_name_with_watch_capture(
                "testButtonPressResponse",
                "native",
                true,
            )
            .expect("watchable environment json");
            assert!(json.contains("\"OXIDE_PERF_WATCH_MODE\":\"1\""));
            assert!(json.contains("\"OXIDE_PERF_FRAME_CAPTURE\":\"1\""));
            assert!(json.contains("\"OXIDE_PERF_FRAME_CAPTURE_EVERY\":\"1\""));
            assert!(json.contains("\"OXIDE_PERF_FRAME_CAPTURE_MAX\":\"12\""));
            assert!(json.contains("\"OXIDE_PERF_CASE\":\"testButtonPressResponse\""));
        },
    );
}

#[test]
fn uikit_perf_environment_respects_watch_frame_capture_overrides() {
    with_env_vars(
        &[
            ("OXIDE_PERF_FRAME_CAPTURE_EVERY", Some("2")),
            ("OXIDE_PERF_FRAME_CAPTURE_MAX", Some("5")),
        ],
        || {
            let json = uikit_perf_environment_json_for_test_name_with_watch_capture(
                "testButtonPressResponse",
                "native",
                true,
            )
            .expect("watchable environment json");
            assert!(json.contains("\"OXIDE_PERF_FRAME_CAPTURE_EVERY\":\"2\""));
            assert!(json.contains("\"OXIDE_PERF_FRAME_CAPTURE_MAX\":\"5\""));
        },
    );
}

#[test]
fn perf_frame_capture_relative_source_uses_stable_case_component() {
    assert_eq!(
        perf_frame_capture_relative_source_for_test_name("testButtonPressResponse"),
        "Library/Caches/oxide-watch-captures/testButtonPressResponse"
    );
    assert_eq!(
        perf_frame_capture_relative_source_for_test_name("test odd/case name"),
        "Library/Caches/oxide-watch-captures/test_odd_case_name"
    );
}

#[test]
fn uikit_perf_environment_forwards_camera_benchmark_overrides() {
    with_env_vars(
        &[
            ("OXIDE_PERF_CAMERA_PREVIEW_SURFACE_SCALE", Some("0.5")),
            ("OXIDE_PERF_CAMERA_CAPTURE_CONTRACT_MODE", Some("preset-720p")),
            ("OXIDE_PERF_CAMERA_STAGE_MEASUREMENT", Some("0")),
            ("OXIDE_PERF_CAMERA_TINY_PREVIEW_RENDERER", Some("1")),
            ("OXIDE_PERF_CAMERA_PREVIEW_BACKPRESSURE", Some("1")),
            ("OXIDE_PERF_CAMERA_PREVIEW_DONT_CARE_LOAD", Some("1")),
            ("OXIDE_PERF_CAMERA_PREVIEW_SUBMISSION_CAP", Some("1")),
            ("OXIDE_PERF_CAMERA_PREVIEW_PUBLISHED_SLOT_COUNT", Some("2")),
            ("OXIDE_PERF_CAMERA_NO_VISIBLE_PRESENT", Some("1")),
            ("OXIDE_PERF_CAMERA_FRAME_DRIVEN_SCHEDULING", Some("1")),
            ("OXIDE_PERF_CAMERA_PREBRIDGE_DROP", Some("1")),
        ],
        || {
            let json = uikit_perf_environment_json_for_test_name(
                "testCameraNV12LegacyLivePreview",
                "native",
            )
            .expect("camera environment json");
            assert!(json.contains("\"OXIDE_PERF_CAMERA_PREVIEW_SURFACE_SCALE\":\"0.5\""));
            assert!(json.contains("\"OXIDE_PERF_CAMERA_CAPTURE_CONTRACT_MODE\":\"preset-720p\""));
            assert!(json.contains("\"OXIDE_PERF_CAMERA_STAGE_MEASUREMENT\":\"0\""));
            assert!(json.contains("\"OXIDE_PERF_CAMERA_TINY_PREVIEW_RENDERER\":\"1\""));
            assert!(json.contains("\"OXIDE_PERF_CAMERA_PREVIEW_BACKPRESSURE\":\"1\""));
            assert!(json.contains("\"OXIDE_PERF_CAMERA_PREVIEW_DONT_CARE_LOAD\":\"1\""));
            assert!(json.contains("\"OXIDE_PERF_CAMERA_PREVIEW_SUBMISSION_CAP\":\"1\""));
            assert!(json.contains("\"OXIDE_PERF_CAMERA_PREVIEW_PUBLISHED_SLOT_COUNT\":\"2\""));
            assert!(json.contains("\"OXIDE_PERF_CAMERA_NO_VISIBLE_PRESENT\":\"1\""));
            assert!(json.contains("\"OXIDE_PERF_CAMERA_FRAME_DRIVEN_SCHEDULING\":\"1\""));
            assert!(json.contains("\"OXIDE_PERF_CAMERA_PREBRIDGE_DROP\":\"1\""));
        },
    );
}

#[test]
fn uikit_perf_environment_enables_real_app_camera_host_for_real_app_cases() {
    let custom_json = uikit_perf_environment_json_for_test_name(
        "testCameraNV12LegacyRealAppLivePreview",
        "native",
    )
    .expect("real app custom environment json");
    assert!(custom_json.contains("\"OXIDE_PERF_CASE\":\"testCameraNV12LegacyRealAppLivePreview\""));
    assert!(custom_json.contains("\"OXIDE_RENDER_IN_TEST\":\"1\""));
    assert!(custom_json.contains("\"OXIDE_PERF_CAMERA_REAL_APP_HOST\":\"1\""));
    assert!(!custom_json.contains("\"OXIDE_PERF_PARKED\":\"1\""));
    assert!(!custom_json.contains("\"OXIDE_PERF_CAMERA_REAL_APP_HYBRID_VISIBLE_PREVIEW\""));

    let hybrid_json = uikit_perf_environment_json_for_test_name(
        "testCameraNV12LegacyRealAppHybridPreviewLayerLivePreview",
        "native",
    )
    .expect("real app hybrid environment json");
    assert!(hybrid_json.contains(
        "\"OXIDE_PERF_CASE\":\"testCameraNV12LegacyRealAppHybridPreviewLayerLivePreview\""
    ));
    assert!(hybrid_json.contains("\"OXIDE_RENDER_IN_TEST\":\"1\""));
    assert!(hybrid_json.contains("\"OXIDE_PERF_CAMERA_REAL_APP_HOST\":\"1\""));
    assert!(!hybrid_json.contains("\"OXIDE_PERF_PARKED\":\"1\""));
    assert!(hybrid_json.contains("\"OXIDE_PERF_CAMERA_REAL_APP_HYBRID_VISIBLE_PREVIEW\":\"1\""));
}

#[test]
fn oxide_device_launch_environment_json_forwards_runner_debug_env() {
    with_env_vars(
        &[
            ("OXIDE_PERF_RUNNER_FILTER", Some("gpu.scene.zoom_image.frame")),
            ("OXIDE_DEBUG_ENCODE_EVERY", Some("1")),
            ("OXIDE_ENABLE_IMAGE_ARG_BUFFER", Some("0")),
            ("OXIDE_ENABLE_DAMAGE", Some("0")),
        ],
        || {
            let json = oxide_device_launch_environment_json(true)
                .expect("oxide device launch environment json");
            assert!(json.contains("\"OXIDE_PERF_RUNNER\":\"1\""));
            assert!(json.contains("\"OXIDE_PERF_RUNNER_SMOKE\":\"1\""));
            assert!(json.contains("\"OXIDE_PERF_RUNNER_FILTER\":\"gpu.scene.zoom_image.frame\""));
            assert!(json.contains("\"OXIDE_DEBUG_ENCODE_EVERY\":\"1\""));
            assert!(json.contains("\"OXIDE_ENABLE_IMAGE_ARG_BUFFER\":\"0\""));
            assert!(json.contains("\"OXIDE_ENABLE_DAMAGE\":\"0\""));
        },
    );
}

#[test]
fn map_uikit_case_includes_oxide_hybrid_preview_layer_live_case() {
    let selected =
        map_uikit_case("testCameraNV12LegacyHybridPreviewLayerLivePreview").expect("map case");
    assert_eq!(
        selected.0,
        "uikit.optimized.image_pipeline.camera_preview.nv12_legacy_hybrid_preview_layer_live"
    );
    assert_eq!(selected.1, "gpu.scene.camera.frame");
}

#[test]
fn map_uikit_case_includes_real_app_camera_cases() {
    let custom = map_uikit_case("testCameraNV12LegacyRealAppLivePreview").expect("map custom case");
    assert_eq!(custom.0, "uikit.optimized.image_pipeline.camera_preview.nv12_legacy_real_app_live");
    assert_eq!(custom.1, "gpu.scene.camera.frame");

    let hybrid = map_uikit_case("testCameraNV12LegacyRealAppHybridPreviewLayerLivePreview")
        .expect("map hybrid case");
    assert_eq!(
        hybrid.0,
        "uikit.optimized.image_pipeline.camera_preview.nv12_legacy_real_app_hybrid_preview_layer_live"
    );
    assert_eq!(hybrid.1, "gpu.scene.camera.frame");
}

#[test]
fn official_device_battery_keeps_only_the_official_camera_pair() {
    assert!(uikit_case_in_official_device_battery("testCameraNV12LegacyLivePreview")
        .expect("parked pure custom case"));
    assert!(uikit_case_in_official_device_battery("testCameraAVFoundationPreviewLayerLivePreview")
        .expect("parked AVFoundation case"));
    assert!(!uikit_case_in_official_device_battery(
        "testCameraNV12LegacyHybridPreviewLayerLivePreview"
    )
    .expect("parked hybrid case"));
    assert!(!uikit_case_in_official_device_battery("testCameraNV12LegacyRealAppLivePreview")
        .expect("real app custom case"));
    assert!(!uikit_case_in_official_device_battery(
        "testCameraAVFoundationPreviewLayerRealAppLivePreview"
    )
    .expect("real app baseline case"));
    assert!(!uikit_case_in_official_device_battery(
        "testCameraNV12LegacyRealAppHybridPreviewLayerLivePreview",
    )
    .expect("real app hybrid case"));
    assert!(!uikit_case_in_official_device_battery(
        "testCameraAVFoundationPreviewLayerSidecarLivePreview"
    )
    .expect("sidecar diagnostic case"));
}

#[test]
fn official_device_battery_keeps_representative_signal_cases_and_tiers_out_repetitive_matrix_rows()
{
    assert!(
        uikit_case_in_official_device_battery("testSpinnerSpin").expect("matched animation case")
    );
    assert!(uikit_case_in_official_device_battery("testOptimizedSpinnerSpin")
        .expect("matched optimized animation case"));
    assert!(uikit_case_in_official_device_battery("testLabelEncode")
        .expect("matched label component case"));
    assert!(uikit_case_in_official_device_battery("testButtonEncode")
        .expect("matched button component case"));
    assert!(uikit_case_in_official_device_battery("testCollectionViewEncode")
        .expect("matched collection component case"));
    assert!(uikit_case_in_official_device_battery("testProgressIndeterminate")
        .expect("matched progress animation case"));
    assert!(uikit_case_in_official_device_battery("testButtonPressScale")
        .expect("matched button animation case"));
    assert!(uikit_case_in_official_device_battery("testToggleThumbSpring")
        .expect("matched toggle animation case"));
    assert!(uikit_case_in_official_device_battery("testSliderThumbMove")
        .expect("matched slider animation case"));
    assert!(uikit_case_in_official_device_battery("testImageZoomPan")
        .expect("representative animation case"));
    assert!(uikit_case_in_official_device_battery("testAnimTimelineBars")
        .expect("matched timeline animation case"));
    assert!(uikit_case_in_official_device_battery("testInputFormJourney")
        .expect("matched journey case"));
    assert!(uikit_case_in_official_device_battery("testCollectionNavigationJourney")
        .expect("matched collection journey case"));
    assert!(uikit_case_in_official_device_battery("testZoomImageGestureJourney")
        .expect("matched zoom journey case"));
    assert!(uikit_case_in_official_device_battery("testOrchestrationJourney")
        .expect("matched orchestration journey case"));
    assert!(uikit_case_in_official_device_battery("testButtonPressResponse")
        .expect("matched navigation case"));
    assert!(uikit_case_in_official_device_battery("testTextFocusResponse")
        .expect("matched text focus case"));
    assert!(uikit_case_in_official_device_battery("testCameraNV12LegacyLivePreview")
        .expect("matched custom camera case"));
    assert!(uikit_case_in_official_device_battery("testCameraAVFoundationPreviewLayerLivePreview")
        .expect("matched avfoundation camera case"));
    assert!(!uikit_case_in_official_device_battery("testSimpleHomeColdLaunch")
        .expect("trimmed launch case"));
    assert!(!uikit_case_in_official_device_battery("testLabels1000Mount")
        .expect("trimmed primitive case"));
    assert!(!uikit_case_in_official_device_battery("testPhotoImportThumbnailBridge")
        .expect("trimmed bridge case"));
    assert!(!uikit_case_in_official_device_battery("testFeedScrollJourney")
        .expect("trimmed unmatched journey case"));
}

#[test]
fn compare_device_watchable_smoke_keeps_one_watchable_pair_per_family() {
    assert!(uikit_case_in_compare_device_watchable_smoke("testButtonEncode")
        .expect("component smoke case"));
    assert!(uikit_case_in_compare_device_watchable_smoke("testSpinnerSpin")
        .expect("animation smoke case"));
    assert!(uikit_case_in_compare_device_watchable_smoke("testOptimizedButtonPressResponse")
        .expect("navigation smoke case"));
    assert!(uikit_case_in_compare_device_watchable_smoke("testCollectionNavigationJourney")
        .expect("journey smoke case"));
    assert!(uikit_case_in_compare_device_watchable_smoke("testCameraNV12LegacyLivePreview")
        .expect("camera smoke case"));
    assert!(!uikit_case_in_compare_device_watchable_smoke("testImageZoomPan")
        .expect("non-smoke animation case"));
    assert!(!uikit_case_in_compare_device_watchable_smoke("testTextFocusResponse")
        .expect("non-smoke navigation case"));
}

#[test]
fn compare_device_family_classification_matches_staged_proof_buckets() {
    assert!(uikit_case_in_compare_device_family("testButtonEncode", "component")
        .expect("component family"));
    assert!(uikit_case_in_compare_device_family("testSpinnerSpin", "animation")
        .expect("animation family"));
    assert!(uikit_case_in_compare_device_family("testButtonPressResponse", "navigation")
        .expect("navigation family"));
    assert!(uikit_case_in_compare_device_family("testOrchestrationJourney", "journey")
        .expect("journey family"));
    assert!(uikit_case_in_compare_device_family(
        "testCameraAVFoundationPreviewLayerLivePreview",
        "camera"
    )
    .expect("camera family"));
    assert!(uikit_case_in_compare_device_family(
        "testCameraAVFoundationPreviewLayerLivePreview",
        "image_pipeline"
    )
    .expect("camera alias family"));
    assert!(!uikit_case_in_compare_device_family("testCollectionNavigationJourney", "animation")
        .expect("wrong family"));
}

#[test]
fn compare_device_promotion_missing_families_requires_green_proofs_for_current_build() {
    let expected_stamp = UIKitHostBuildStamp {
        destination: String::from("platform=iOS,id=device"),
        development_team: String::from("TEAM"),
        source_fingerprint: 42,
    };
    let mut families = BTreeMap::new();
    families.insert(
        String::from("animation"),
        CompareDeviceProofFamilyStatus { watchable_smoke_passed: true, family_proof_passed: true },
    );
    families.insert(
        String::from("navigation"),
        CompareDeviceProofFamilyStatus { watchable_smoke_passed: true, family_proof_passed: true },
    );
    let status = CompareDeviceProofStatus { build_stamp: expected_stamp.clone(), families };

    let missing = compare_device_missing_promotion_families(Some(&status), &expected_stamp);
    assert_eq!(
        missing,
        vec![String::from("camera"), String::from("component"), String::from("journey"),]
    );

    let stale_stamp = UIKitHostBuildStamp { source_fingerprint: 99, ..expected_stamp };
    assert_eq!(
        compare_device_missing_promotion_families(Some(&status), &stale_stamp),
        compare_device_official_families()
    );
}

#[test]
fn perf_report_case_set_must_match_selected_cases_before_reuse() {
    let report = sample_perf_report(&["cpu.animation.spinner_spin", "gpu.scene.damage_lab.frame"]);

    assert!(perf_report_matches_case_ids(
        &report,
        &["gpu.scene.damage_lab.frame", "cpu.animation.spinner_spin"]
    ));
    assert!(!perf_report_matches_case_ids(&report, &["cpu.animation.spinner_spin"]));
    assert!(!perf_report_matches_case_ids(
        &sample_perf_report(&["cpu.animation.spinner_spin", "cpu.animation.spinner_spin"]),
        &["cpu.animation.spinner_spin", "cpu.animation.spinner_spin"]
    ));
}

#[test]
fn uikit_report_case_set_must_match_selected_cases_before_reuse() {
    let report =
        sample_uikit_report(&["uikit.animation.spinner_spin", "uikit.component.button.encode"]);

    assert!(uikit_report_matches_case_ids(
        &report,
        &["uikit.component.button.encode", "uikit.animation.spinner_spin"]
    ));
    assert!(!uikit_report_matches_case_ids(&report, &["uikit.animation.spinner_spin"]));
    assert!(!uikit_report_matches_case_ids(
        &sample_uikit_report(&["uikit.animation.spinner_spin", "uikit.animation.spinner_spin"]),
        &["uikit.animation.spinner_spin", "uikit.animation.spinner_spin"]
    ));
}

#[test]
fn compare_device_comparisons_must_pass_before_proof_status_updates() {
    assert!(compare_device_comparisons_pass(None, None));

    let uikit_failed = UIKitPerfComparison {
        missing_baseline: vec![String::from("uikit.case")],
        ..Default::default()
    };
    assert!(!compare_device_comparisons_pass(Some(&uikit_failed), None));

    let oxide_failed =
        PerfComparison { missing_baseline: vec![String::from("oxide.case")], ..Default::default() };
    assert!(!compare_device_comparisons_pass(None, Some(&oxide_failed)));
}

#[test]
fn normalize_ios_version_for_device_support_strips_suffix() {
    assert_eq!(normalize_ios_version_for_device_support("26.3.1 (a)"), "26.3.1");
    assert_eq!(normalize_ios_version_for_device_support("18.6"), "18.6");
}

#[test]
fn device_support_dir_matches_supported_directory_layouts() {
    assert!(device_support_dir_matches("iPhone18,2 26.3.1 (23D771330a)", "iPhone18,2", "26.3.1"));
    assert!(device_support_dir_matches("26.3.1", "iPhone18,2", "26.3.1"));
    assert!(!device_support_dir_matches("iPhone18,2 26.2.1 (23C71)", "iPhone18,2", "26.3.1"));
}

#[test]
fn uikit_device_support_required_tracks_attached_trace_collection() {
    assert!(!uikit_device_support_required(0));
    assert!(uikit_device_support_required(1));
}

#[test]
fn unsupported_gpu_counter_profile_detection_matches_xctrace_error_text() {
    let text = "xcrun xctrace record --template Metal System Trace --instrument Metal GPU Counters failed with status 21: GPU Service reported error: Selected counter profile is not supported on target device";
    assert!(is_unsupported_gpu_counter_profile_error(text));
    assert!(!is_unsupported_gpu_counter_profile_error(
        "xcrun xctrace record failed with status 19: Cannot find process matching name: OxideHost"
    ));
}

#[test]
fn retryable_uikit_trace_handshake_error_matches_completion_timeout_text() {
    assert!(is_retryable_uikit_trace_handshake_error(
        "Error: xcrun devicectl device notification observe --device 00008150-001529C434F8401C --name com.oxide.perf.complete --session-timeout 30 --timeout 35 exited without observing `com.oxide.perf.complete` and console marker `OXIDE_COMPLETE testOptimizedCollectionViewEncode` never appeared before the timeout"
    ));
    assert!(is_retryable_uikit_trace_handshake_error(
        "Error: xcrun devicectl device notification observe --device 00008150-001529C434F8401C --name com.oxide.perf.ready --session-timeout 30 --timeout 35 exited without observing `com.oxide.perf.ready` and console marker `OXIDE_READY testLabelEncode` never appeared before the timeout"
    ));
    assert!(is_retryable_uikit_trace_handshake_error(
        "posted `com.oxide.perf.start` 3 times but `OXIDE_START testCameraNV12LegacyLivePreview` or `OXIDE_COMPLETE testCameraNV12LegacyLivePreview` never appeared before the acknowledgment timeout"
    ));
    assert!(!is_retryable_uikit_trace_handshake_error(
        "xcrun xctrace record failed with status 19: Cannot find process matching name: OxideHost"
    ));
}

#[test]
fn retryable_xctrace_record_timeout_error_matches_watchdog_text() {
    assert!(is_retryable_xctrace_record_timeout_error(
        "Error: xcrun xctrace record --template Metal System Trace --device 00008150-001529C434F8401C --time-limit 6s --output /tmp/test.trace --no-prompt --instrument Points of Interest --launch -- com.oxide.host exceeded wall-time timeout of 21.0s before xctrace finished. stdout: Starting recording with the Metal System Trace template and Points of Interest Instruments. Launching process: com.oxide.host. Time limit: 6.0 s stderr: "
    ));
    assert!(!is_retryable_xctrace_record_timeout_error(
        "xcrun xctrace record failed with status 19: Cannot find process matching name: OxideHost"
    ));
}

#[test]
fn retryable_devicectl_json_error_matches_streaming_device_failures() {
    assert!(is_retryable_devicectl_json_error(
        "ERROR: The operation couldn’t be completed. (CoreDevice.ActionError error 3.)\nNSDebugDescription = This operation could not be performed due to an error in StreamingAction: Couldn't get the message from the device."
    ));
    assert!(is_retryable_devicectl_json_error(
        "StreamingAction: Couldn't get the message from the device."
    ));
    assert!(is_retryable_devicectl_json_error(
        "ERROR: Failed to allocate RSD device. (com.apple.mobiledevice error -402653181 (0xE8000003))"
    ));
    assert!(!is_retryable_devicectl_json_error(
        "devicectl device info processes --device 00008150 failed with status 1"
    ));
}

#[test]
fn retryable_devicectl_install_error_matches_installcoordination_failures() {
    assert!(is_retryable_devicectl_install_error(
        "ERROR: Failed to install the app on the device. Could not get service com.apple.remote.installcoordination_proxy (IXRemoteErrorDomain error 5)"
    ));
    assert!(!is_retryable_devicectl_install_error("codesign failed before install"));
}

#[test]
fn expected_devicectl_console_termination_accepts_intentional_sigkill() {
    let text = "Launched application with com.oxide.host bundle identifier.\nWaiting for the application to terminate…\nApp terminated due to signal 9.\n";
    assert!(is_expected_devicectl_console_termination(text));
    assert!(!is_expected_devicectl_console_termination(
        "Launched application with com.oxide.host bundle identifier.\nApp terminated due to signal 6.\n"
    ));
}

#[test]
fn device_process_helpers_extract_name_and_pid() {
    let json = r#"
{
  "result": {
    "runningProcesses": [
      {
        "executable": "file:///System/Library/CoreServices/SpringBoard.app/SpringBoard",
        "processIdentifier": 35
      },
      {
        "processIdentifier": 41
      },
      {
        "executable": "file:///var/containers/Bundle/Application/ABC/OxideHost.app/OxideHost",
        "processIdentifier": 14165
      }
    ]
  }
}
"#;

    assert_eq!(
        device_process_name(
            "file:///var/containers/Bundle/Application/ABC/OxideHost.app/OxideHost"
        ),
        "OxideHost"
    );
    assert_eq!(
        find_device_process_ids(json, "OxideHost").expect("parse process list"),
        [14165_u64].into_iter().collect()
    );
}

#[test]
fn device_process_helpers_collect_all_matching_pids() {
    let json = r#"
{
  "result": {
    "runningProcesses": [
      {
        "executable": "file:///var/containers/Bundle/Application/ABC/OxideHost.app/OxideHost",
        "processIdentifier": 14165
      },
      {
        "executable": "file:///var/containers/Bundle/Application/XYZ/OxideHost.app/OxideHost",
        "processIdentifier": 14188
      }
    ]
  }
}
"#;

    assert_eq!(
        find_device_process_ids(json, "OxideHost").expect("parse process list"),
        [14165_u64, 14188_u64].into_iter().collect()
    );
}

#[test]
fn compare_uikit_reports_device_suite_gates_direct_counters() {
    let current = UIKitPerfReport {
        version: 1,
        suite: String::from("device"),
        generated_label: None,
        device_name: String::from("Victor’s iPhone"),
        energy_status: String::from("direct device"),
        contract: UIKitContractCoverageReport::default(),
        notes: Vec::new(),
        cases: vec![UIKitPerfCase {
            id: String::from("uikit.component.label.encode"),
            oxide_case_id: String::from("cpu.component.label.encode"),
            test_name: String::from("testLabelEncode"),
            layer: String::from("engine"),
            scenario: String::from("primitive-view"),
            style: String::from("idiomatic"),
            cache_state: String::from("warm"),
            refresh_mode: String::from("native"),
            threshold_pct: 0.10,
            notes: Vec::new(),
            metrics: BTreeMap::from([
                (String::from("clock_s"), sample_metric(1.0)),
                (String::from("cpu_time_s"), sample_metric(1.0)),
                (String::from("cpu_cycles_kc"), sample_metric(1.0)),
                (String::from("memory_peak_kb"), sample_metric(1.0)),
                (String::from("gpu_time_s"), sample_metric(1.0)),
                (String::from("gpu_latency_s"), sample_metric(1.0)),
                (String::from("hitch_ms_per_s"), sample_metric(0.0)),
                (String::from("missed_frames"), sample_metric(0.0)),
                (String::from("missed_frames_per_s"), sample_metric(0.0)),
                (String::from("gpu_counter.shader_cycles"), sample_metric(1.30)),
            ]),
            ..UIKitPerfCase::default()
        }],
    };

    let baseline = UIKitPerfReport {
        version: 1,
        suite: String::from("device"),
        generated_label: None,
        device_name: String::from("Victor’s iPhone"),
        energy_status: String::from("direct device"),
        contract: UIKitContractCoverageReport::default(),
        notes: Vec::new(),
        cases: vec![UIKitPerfCase {
            id: String::from("uikit.component.label.encode"),
            oxide_case_id: String::from("cpu.component.label.encode"),
            test_name: String::from("testLabelEncode"),
            layer: String::from("engine"),
            scenario: String::from("primitive-view"),
            style: String::from("idiomatic"),
            cache_state: String::from("warm"),
            refresh_mode: String::from("native"),
            threshold_pct: 0.10,
            notes: Vec::new(),
            metrics: BTreeMap::from([
                (String::from("clock_s"), sample_metric(1.0)),
                (String::from("cpu_time_s"), sample_metric(1.0)),
                (String::from("cpu_cycles_kc"), sample_metric(1.0)),
                (String::from("memory_peak_kb"), sample_metric(1.0)),
                (String::from("gpu_time_s"), sample_metric(1.0)),
                (String::from("gpu_latency_s"), sample_metric(1.0)),
                (String::from("hitch_ms_per_s"), sample_metric(0.0)),
                (String::from("missed_frames"), sample_metric(0.0)),
                (String::from("missed_frames_per_s"), sample_metric(0.0)),
                (String::from("energy_j"), sample_metric(1.0)),
                (String::from("gpu_counter.shader_cycles"), sample_metric(1.0)),
            ]),
            ..UIKitPerfCase::default()
        }],
    };

    let comparison = compare_uikit_reports(&current, &baseline);
    assert!(comparison.missing_baseline.is_empty());
    assert_eq!(comparison.regressions.len(), 1);
    assert_eq!(comparison.regressions[0].metric, "gpu_counter.shader_cycles");
}

#[test]
fn compare_uikit_reports_device_suite_requires_direct_gpu_and_frame_cadence_metrics() {
    for metric_name in
        ["gpu_time_s", "gpu_latency_s", "hitch_ms_per_s", "missed_frames", "missed_frames_per_s"]
    {
        let mut metrics = sample_uikit_device_metrics();
        metrics.remove(metric_name);
        let current = sample_uikit_device_report(metrics);
        let baseline = sample_uikit_device_report(sample_uikit_device_metrics());

        let comparison = compare_uikit_reports(&current, &baseline);

        assert_eq!(comparison.matched, 1);
        assert_eq!(
            comparison.missing_baseline,
            vec![format!("uikit.component.label.encode::native::{}", metric_name)]
        );
    }
}

#[test]
fn validate_uikit_device_report_metric_contract_rejects_missing_gpu_and_cadence_metrics() {
    assert!(validate_uikit_device_report_metric_contract(&sample_uikit_device_report(
        sample_uikit_device_metrics()
    ))
    .is_ok());

    for metric_name in
        ["gpu_time_s", "gpu_latency_s", "hitch_ms_per_s", "missed_frames", "missed_frames_per_s"]
    {
        let mut metrics = sample_uikit_device_metrics();
        metrics.remove(metric_name);
        let report = sample_uikit_device_report(metrics);
        let err =
            validate_uikit_device_report_metric_contract(&report).expect_err("missing metric");

        assert!(err.to_string().contains(metric_name), "{err}");
        assert!(!uikit_report_matches_case_ids(&report, &["uikit.component.label.encode"]));
    }
}

#[test]
fn validate_uikit_device_report_metric_contract_rejects_invalid_distribution_fields() {
    let mut metrics = sample_uikit_device_metrics();
    metrics.get_mut("gpu_time_s").expect("gpu metric").p95 = f64::NAN;
    let report = sample_uikit_device_report(metrics);
    let err =
        validate_uikit_device_report_metric_contract(&report).expect_err("invalid distribution");

    assert!(err.to_string().contains("gpu_time_s"), "{err}");
    assert!(!uikit_report_matches_case_ids(&report, &["uikit.component.label.encode"]));

    let mut metrics = sample_uikit_device_metrics();
    metrics.get_mut("missed_frames").expect("cadence metric").samples = 0;
    let report = sample_uikit_device_report(metrics);
    let err = validate_uikit_device_report_metric_contract(&report)
        .expect_err("invalid distribution samples");

    assert!(err.to_string().contains("missed_frames"), "{err}");
    assert!(!uikit_report_matches_case_ids(&report, &["uikit.component.label.encode"]));
}

#[test]
fn validate_oxide_device_report_metric_contract_rejects_missing_gpu_memory_and_cadence_metrics() {
    assert!(validate_oxide_device_report_metric_contract(&sample_oxide_device_report(
        sample_oxide_device_metrics()
    ))
    .is_ok());

    for metric_name in [
        "memory_peak_kb",
        "gpu_time_s",
        "gpu_latency_s",
        "hitch_ms_per_s",
        "missed_frames",
        "missed_frames_per_s",
    ] {
        let mut metrics = sample_oxide_device_metrics();
        metrics.remove(metric_name);
        let report = sample_oxide_device_report(metrics);
        let err =
            validate_oxide_device_report_metric_contract(&report).expect_err("missing metric");

        assert!(err.to_string().contains(metric_name), "{err}");
        assert!(!perf_report_matches_case_ids(&report, &["cpu.animation.spinner_spin"]));
    }
}

#[test]
fn validate_oxide_device_report_metric_contract_rejects_missing_gpu_and_cadence_distributions() {
    for metric_key in [
        "gpu_time_s_p95",
        "gpu_latency_s_p99",
        "hitch_ms_per_s_peak",
        "missed_frames_p50",
        "missed_frames_per_s_samples",
    ] {
        let mut metrics = sample_oxide_device_metrics();
        metrics.remove(metric_key);
        let report = sample_oxide_device_report(metrics);
        let err = validate_oxide_device_report_metric_contract(&report)
            .expect_err("missing distribution metric");

        assert!(err.to_string().contains(metric_key), "{err}");
        assert!(!perf_report_matches_case_ids(&report, &["cpu.animation.spinner_spin"]));
    }
}

#[test]
fn validate_oxide_device_report_metric_contract_rejects_invalid_distribution_values() {
    let mut metrics = sample_oxide_device_metrics();
    metrics.insert(String::from("gpu_time_s_p95"), f64::NAN);
    let report = sample_oxide_device_report(metrics);
    let err = validate_oxide_device_report_metric_contract(&report)
        .expect_err("invalid distribution metric");

    assert!(err.to_string().contains("gpu_time_s_p95"), "{err}");
    assert!(!perf_report_matches_case_ids(&report, &["cpu.animation.spinner_spin"]));

    let mut metrics = sample_oxide_device_metrics();
    metrics.insert(String::from("hitch_ms_per_s_samples"), 0.0);
    let report = sample_oxide_device_report(metrics);
    let err = validate_oxide_device_report_metric_contract(&report)
        .expect_err("invalid distribution samples");

    assert!(err.to_string().contains("hitch_ms_per_s_samples"), "{err}");
    assert!(!perf_report_matches_case_ids(&report, &["cpu.animation.spinner_spin"]));
}

#[test]
fn committed_oxide_device_latest_is_strict_or_explicitly_stale() {
    let report: PerfReport = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../benchmarks/oxide-device/latest.json"
    )))
    .expect("committed Oxide device latest JSON");

    let Err(err) = validate_oxide_device_report_metric_contract(&report) else {
        return;
    };
    let err = err.to_string();

    assert!(err.contains("memory_peak_kb"), "{err}");
    assert!(err.contains("hitch_ms_per_s"), "{err}");
    assert!(
        notes_contain(&report.contract.notes, "metric contract status: stale partial"),
        "stale Oxide device baseline must explicitly mark its metric contract gap: {err}"
    );
}

#[test]
fn committed_uikit_device_latest_is_strict_or_explicitly_stale() {
    let report: UIKitPerfReport = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../benchmarks/uikit-device/latest.json"
    )))
    .expect("committed UIKit device latest JSON");

    let Err(err) = validate_uikit_device_report_metric_contract(&report) else {
        return;
    };
    let err = err.to_string();

    assert!(err.contains("hitch_ms_per_s"), "{err}");
    assert!(err.contains("missed_frames"), "{err}");
    assert!(
        notes_contain(&report.contract.notes, "metric contract status: stale partial"),
        "stale UIKit device baseline must explicitly mark its metric contract gap: {err}"
    );
}

#[test]
fn compare_uikit_reports_device_suite_gates_energy_when_present() {
    let current = UIKitPerfReport {
        version: 1,
        suite: String::from("device"),
        generated_label: None,
        device_name: String::from("Victor’s iPhone"),
        energy_status: String::from("direct device"),
        contract: UIKitContractCoverageReport::default(),
        notes: Vec::new(),
        cases: vec![UIKitPerfCase {
            id: String::from("uikit.component.label.encode"),
            oxide_case_id: String::from("cpu.component.label.encode"),
            test_name: String::from("testLabelEncode"),
            layer: String::from("engine"),
            scenario: String::from("primitive-view"),
            style: String::from("idiomatic"),
            cache_state: String::from("warm"),
            refresh_mode: String::from("native"),
            threshold_pct: 0.10,
            notes: Vec::new(),
            metrics: BTreeMap::from([
                (String::from("clock_s"), sample_metric(1.0)),
                (String::from("cpu_time_s"), sample_metric(1.0)),
                (String::from("cpu_cycles_kc"), sample_metric(1.0)),
                (String::from("memory_peak_kb"), sample_metric(1.0)),
                (String::from("gpu_time_s"), sample_metric(1.0)),
                (String::from("gpu_latency_s"), sample_metric(1.0)),
                (String::from("hitch_ms_per_s"), sample_metric(0.0)),
                (String::from("missed_frames"), sample_metric(0.0)),
                (String::from("missed_frames_per_s"), sample_metric(0.0)),
                (String::from("energy_j"), sample_metric(1.30)),
            ]),
            ..UIKitPerfCase::default()
        }],
    };

    let baseline = UIKitPerfReport {
        version: 1,
        suite: String::from("device"),
        generated_label: None,
        device_name: String::from("Victor’s iPhone"),
        energy_status: String::from("direct device"),
        contract: UIKitContractCoverageReport::default(),
        notes: Vec::new(),
        cases: vec![UIKitPerfCase {
            id: String::from("uikit.component.label.encode"),
            oxide_case_id: String::from("cpu.component.label.encode"),
            test_name: String::from("testLabelEncode"),
            layer: String::from("engine"),
            scenario: String::from("primitive-view"),
            style: String::from("idiomatic"),
            cache_state: String::from("warm"),
            refresh_mode: String::from("native"),
            threshold_pct: 0.10,
            notes: Vec::new(),
            metrics: BTreeMap::from([
                (String::from("clock_s"), sample_metric(1.0)),
                (String::from("cpu_time_s"), sample_metric(1.0)),
                (String::from("cpu_cycles_kc"), sample_metric(1.0)),
                (String::from("memory_peak_kb"), sample_metric(1.0)),
                (String::from("gpu_time_s"), sample_metric(1.0)),
                (String::from("gpu_latency_s"), sample_metric(1.0)),
                (String::from("hitch_ms_per_s"), sample_metric(0.0)),
                (String::from("missed_frames"), sample_metric(0.0)),
                (String::from("missed_frames_per_s"), sample_metric(0.0)),
                (String::from("energy_j"), sample_metric(1.0)),
            ]),
            ..UIKitPerfCase::default()
        }],
    };

    let comparison = compare_uikit_reports(&current, &baseline);
    assert_eq!(comparison.regressions.len(), 1);
    assert_eq!(comparison.regressions[0].metric, "energy_j");
}

#[test]
fn compare_uikit_reports_keys_device_rows_by_refresh_mode() {
    let current = UIKitPerfReport {
        version: 1,
        suite: String::from("device"),
        generated_label: None,
        device_name: String::from("Victor’s iPhone"),
        energy_status: String::from("direct device"),
        contract: UIKitContractCoverageReport::default(),
        notes: Vec::new(),
        cases: vec![UIKitPerfCase {
            id: String::from("uikit.animation.spinner_spin"),
            oxide_case_id: String::from("cpu.animation.spinner_spin"),
            test_name: String::from("testSpinnerSpin"),
            layer: String::from("engine"),
            scenario: String::from("animation-effect"),
            style: String::from("idiomatic"),
            cache_state: String::from("warm"),
            refresh_mode: String::from("native"),
            threshold_pct: 0.10,
            notes: Vec::new(),
            metrics: BTreeMap::from([
                (String::from("clock_s"), sample_metric(1.0)),
                (String::from("cpu_time_s"), sample_metric(1.0)),
                (String::from("cpu_cycles_kc"), sample_metric(1.0)),
                (String::from("memory_peak_kb"), sample_metric(1.0)),
                (String::from("gpu_time_s"), sample_metric(1.0)),
                (String::from("gpu_latency_s"), sample_metric(1.0)),
            ]),
            ..UIKitPerfCase::default()
        }],
    };

    let baseline = UIKitPerfReport {
        version: 1,
        suite: String::from("device"),
        generated_label: None,
        device_name: String::from("Victor’s iPhone"),
        energy_status: String::from("direct device"),
        contract: UIKitContractCoverageReport::default(),
        notes: Vec::new(),
        cases: vec![UIKitPerfCase {
            id: String::from("uikit.animation.spinner_spin"),
            oxide_case_id: String::from("cpu.animation.spinner_spin"),
            test_name: String::from("testSpinnerSpin"),
            layer: String::from("engine"),
            scenario: String::from("animation-effect"),
            style: String::from("idiomatic"),
            cache_state: String::from("warm"),
            refresh_mode: String::from("legacy-refresh"),
            threshold_pct: 0.10,
            notes: Vec::new(),
            metrics: BTreeMap::from([
                (String::from("clock_s"), sample_metric(1.0)),
                (String::from("cpu_time_s"), sample_metric(1.0)),
                (String::from("cpu_cycles_kc"), sample_metric(1.0)),
                (String::from("memory_peak_kb"), sample_metric(1.0)),
                (String::from("gpu_time_s"), sample_metric(1.0)),
                (String::from("gpu_latency_s"), sample_metric(1.0)),
            ]),
            ..UIKitPerfCase::default()
        }],
    };

    let comparison = compare_uikit_reports(&current, &baseline);
    assert_eq!(comparison.matched, 0);
    assert_eq!(
        comparison.missing_baseline,
        vec![String::from("uikit.animation.spinner_spin::native")]
    );
}

#[test]
fn parse_xctrace_toc_tables_collects_schemas() {
    let xml = r#"
<?xml version="1.0"?>
<trace-toc>
  <run number="1">
    <data>
      <table schema="power-profile"/>
      <table schema="os-signpost" category="PointsOfInterest" subsystem="&quot;com.oxide.perf&quot;"/>
    </data>
  </run>
</trace-toc>
"#;

    let tables = parse_xctrace_toc_tables(xml).expect("parse xctrace toc");
    assert_eq!(tables.len(), 2);
    assert_eq!(tables[0].schema, "power-profile");
    assert_eq!(tables[0].xpath, "/trace-toc/run[1]/data[1]/table[1]");
    assert_eq!(tables[0].category, "");
    assert_eq!(tables[1].schema, "os-signpost");
    assert_eq!(tables[1].xpath, "/trace-toc/run[1]/data[1]/table[2]");
    assert_eq!(tables[1].category, "PointsOfInterest");
    assert_eq!(tables[1].subsystem, "com.oxide.perf");
}

#[test]
fn preferred_xctrace_toc_tables_prioritizes_requested_category() {
    let toc = vec![
        XctraceTocTable {
            schema: String::from("os-signpost"),
            xpath: String::from("/trace-toc/run[1]/data[1]/table[4]"),
            category: String::from("CAMetalLayer"),
            subsystem: String::from("com.apple.coreanimation"),
        },
        XctraceTocTable {
            schema: String::from("os-signpost"),
            xpath: String::from("/trace-toc/run[1]/data[1]/table[5]"),
            category: String::from("PointsOfInterest"),
            subsystem: String::from("com.oxide.perf"),
        },
    ];

    let ordered = preferred_xctrace_toc_tables(&toc, "os-signpost", Some("PointsOfInterest"));
    assert_eq!(ordered.len(), 2);
    assert_eq!(ordered[0].xpath, "/trace-toc/run[1]/data[1]/table[5]");
    assert_eq!(ordered[1].xpath, "/trace-toc/run[1]/data[1]/table[4]");
}

#[test]
fn parse_xctrace_summary_window_uses_trace_duration() {
    let xml = r#"
<?xml version="1.0"?>
<trace-toc>
  <run number="1">
    <info>
      <summary>
        <duration>16.443459</duration>
      </summary>
    </info>
  </run>
</trace-toc>
"#;

    let window = parse_xctrace_summary_window(xml, "OxideHost").expect("parse xctrace summary");
    assert_eq!(window.start_ns, 0);
    assert_eq!(window.end_ns, 16_443_459_000);
    assert_eq!(window.process_name, "OxideHost");
}

#[test]
fn uikit_power_trace_candidate_paths_cover_trace_and_atrc_layouts() {
    let root = PathBuf::from("/tmp/power-traces");
    let candidates = uikit_power_trace_candidate_paths(&root, "testInputFormJourney");
    let expected = vec![
        root.join("testInputFormJourney").join("power.trace"),
        root.join("testInputFormJourney").join("power.atrc"),
        root.join("testInputFormJourney.trace"),
        root.join("testInputFormJourney.atrc"),
        root.join("testInputFormJourney-power.trace"),
        root.join("testInputFormJourney-power.atrc"),
    ];
    assert_eq!(candidates, expected);
}

#[test]
fn resolve_existing_uikit_power_trace_prefers_existing_trace_bundle_before_raw_export() {
    let temp = tempdir().expect("tempdir");
    let trace_path = temp.path().join("testInputFormJourney.trace");
    let atrc_path = temp.path().join("testInputFormJourney.atrc");
    fs::create_dir_all(&trace_path).expect("trace dir");
    fs::write(&atrc_path, b"raw atrc").expect("atrc file");

    let resolved = resolve_existing_uikit_power_trace(temp.path(), "testInputFormJourney");
    assert_eq!(resolved.as_deref(), Some(trace_path.as_path()));
}

#[test]
fn resolve_existing_uikit_power_trace_falls_back_to_raw_atrc_export() {
    let temp = tempdir().expect("tempdir");
    let atrc_path = temp.path().join("testInputFormJourney-power.atrc");
    fs::write(&atrc_path, b"raw atrc").expect("atrc file");

    let resolved = resolve_existing_uikit_power_trace(temp.path(), "testInputFormJourney");
    assert_eq!(resolved.as_deref(), Some(atrc_path.as_path()));
}

#[test]
fn is_xctrace_trace_bundle_matches_trace_extension_only() {
    assert!(is_xctrace_trace_bundle(&PathBuf::from("/tmp/demo.trace")));
    assert!(is_xctrace_trace_bundle(&PathBuf::from("/tmp/DEMO.TRACE")));
    assert!(!is_xctrace_trace_bundle(&PathBuf::from("/tmp/demo.atrc")));
    assert!(!is_xctrace_trace_bundle(&PathBuf::from("/tmp/demo")));
}

#[test]
fn uikit_device_trace_artifact_exists_accepts_trace_bundles() {
    assert!(uikit_device_trace_artifact_exists(&PathBuf::from("/tmp/demo.trace")));
    assert!(uikit_device_trace_artifact_exists(&PathBuf::from("/tmp/DEMO.TRACE")));
    assert!(!uikit_device_trace_artifact_exists(&PathBuf::from("/tmp/demo.atrc")));
}

#[test]
fn parse_xctrace_tables_reads_rows_and_formats() {
    let xml = r#"
<?xml version="1.0"?>
<trace-query-result>
  <node xpath='//trace-toc[1]/run[1]/data[1]/table[1]'>
    <schema name="gpu-counter-value">
      <col><mnemonic>timestamp</mnemonic><name>Timestamp</name><engineering-type>event-time</engineering-type></col>
      <col><mnemonic>value</mnemonic><name>Value</name><engineering-type>fixed-decimal</engineering-type></col>
    </schema>
    <row>
      <event-time fmt="00:00.000.100">100</event-time>
      <fixed-decimal fmt="1.25">1.25</fixed-decimal>
    </row>
  </node>
</trace-query-result>
"#;

    let tables = parse_xctrace_tables(xml).expect("parse xctrace query");
    assert_eq!(tables.len(), 1);
    assert_eq!(tables[0].columns[0].mnemonic, "timestamp");
    assert_eq!(tables[0].rows[0].values["timestamp"].fmt.as_deref(), Some("00:00.000.100"));
    assert_eq!(tables[0].rows[0].values["value"].raw.as_deref(), Some("1.25"));
}

#[test]
fn parse_xctrace_tables_inherits_schema_for_follow_on_nodes() {
    let xml = r#"
<?xml version="1.0"?>
<trace-query-result>
  <node xpath='//trace-toc[1]/run[1]/data[1]/table[1]'>
    <schema name="os-signpost">
      <col><mnemonic>time</mnemonic><name>Timestamp</name><engineering-type>event-time</engineering-type></col>
      <col><mnemonic>process</mnemonic><name>Process</name><engineering-type>process</engineering-type></col>
      <col><mnemonic>event-type</mnemonic><name>Event Type</name><engineering-type>event-type</engineering-type></col>
      <col><mnemonic>identifier</mnemonic><name>Identifier</name><engineering-type>os-signpost-identifier</engineering-type></col>
      <col><mnemonic>name</mnemonic><name>Name</name><engineering-type>signpost-name</engineering-type></col>
    </schema>
    <row>
      <event-time fmt="00:01.000.000">1000000000</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="Begin Interval">1</event-type>
      <os-signpost-identifier fmt="W">W</os-signpost-identifier>
      <signpost-name fmt="PerfWorkload">PerfWorkload</signpost-name>
    </row>
  </node>
  <node xpath='//trace-toc[1]/run[1]/data[1]/table[2]'>
    <row>
      <event-time fmt="00:02.000.000">2000000000</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="Begin Interval">1</event-type>
      <os-signpost-identifier fmt="C">C</os-signpost-identifier>
      <signpost-name fmt="camera.renderer.direct.commit">camera.renderer.direct.commit</signpost-name>
    </row>
  </node>
</trace-query-result>
"#;

    let tables = parse_xctrace_tables(xml).expect("parse xctrace query");
    assert_eq!(tables.len(), 2);
    assert_eq!(tables[1].columns[0].mnemonic, "time");
    assert_eq!(
        tables[1].rows[0].values["name"].fmt.as_deref(),
        Some("camera.renderer.direct.commit")
    );
}

#[test]
fn summarize_time_profile_from_xml_bounds_rows_and_buckets_hotspots() {
    let profile_xml = r#"
<?xml version="1.0"?>
<trace-query-result>
  <node xpath='//trace-toc[1]/run[1]/data[1]/table[1]'>
    <schema name="time-profile">
      <col><mnemonic>time</mnemonic><name>Sample Time</name><engineering-type>sample-time</engineering-type></col>
      <col><mnemonic>thread</mnemonic><name>Thread</name><engineering-type>thread</engineering-type></col>
      <col><mnemonic>process</mnemonic><name>Process</name><engineering-type>process</engineering-type></col>
      <col><mnemonic>core</mnemonic><name>Core</name><engineering-type>core</engineering-type></col>
      <col><mnemonic>thread-state</mnemonic><name>State</name><engineering-type>thread-state</engineering-type></col>
      <col><mnemonic>weight</mnemonic><name>Weight</name><engineering-type>weight</engineering-type></col>
      <col><mnemonic>stack</mnemonic><name>Backtrace</name><engineering-type>tagged-backtrace</engineering-type></col>
    </schema>
    <row>
      <sample-time id="1" fmt="00:00.000.100">100</sample-time>
      <thread id="2" fmt="Main Thread (0x1) (OxideHost, pid: 42)"><tid id="3" fmt="0x1">1</tid></thread>
      <process id="4" fmt="OxideHost (42)">42</process>
      <core id="5" fmt="CPU 0">0</core>
      <thread-state id="6" fmt="Running">Running</thread-state>
      <weight id="7" fmt="1.00 ms">1000000</weight>
      <tagged-backtrace id="8" fmt="AGX::RenderContext::encodeAndEmitRenderState ← (1 other frame)">
        <backtrace id="9">
          <frame id="10" name="AGX::RenderContext::encodeAndEmitRenderState"/>
          <frame id="11" name="-[FPInFlightDrawableLifetime dealloc]"/>
        </backtrace>
      </tagged-backtrace>
    </row>
    <row>
      <sample-time fmt="00:00.000.150">150</sample-time>
      <thread id="12" fmt="OxideHost (0x2) (OxideHost, pid: 42)"><tid id="13" fmt="0x2">2</tid></thread>
      <process ref="4"/>
      <core ref="5"/>
      <thread-state ref="6"/>
      <weight ref="7"/>
      <tagged-backtrace fmt="roDeserializeSampleBuffer ← (1 other frame)">
        <backtrace id="14">
          <frame id="15" name="roDeserializeSampleBuffer"/>
          <frame id="16" name="CVPixelBufferCreateWithIOSurface"/>
        </backtrace>
      </tagged-backtrace>
    </row>
    <row>
      <sample-time fmt="00:00.000.350">350</sample-time>
      <thread ref="12"/>
      <process ref="4"/>
      <core ref="5"/>
      <thread-state ref="6"/>
      <weight ref="7"/>
      <tagged-backtrace fmt="summarizeStageSamples ← (1 other frame)">
        <backtrace id="17">
          <frame id="18" name="summarizeStageSamples"/>
        </backtrace>
      </tagged-backtrace>
    </row>
  </node>
</trace-query-result>
"#;
    let thread_info_xml = r#"
<?xml version="1.0"?>
<trace-query-result>
  <node xpath='//trace-toc[1]/run[1]/data[1]/table[2]'>
    <schema name="thread-info">
      <col><mnemonic>time</mnemonic><name>Timestamp</name><engineering-type>event-time</engineering-type></col>
      <col><mnemonic>pid</mnemonic><name>Process ID</name><engineering-type>pid</engineering-type></col>
      <col><mnemonic>tid</mnemonic><name>Thread ID</name><engineering-type>tid</engineering-type></col>
      <col><mnemonic>process</mnemonic><name>Process</name><engineering-type>process</engineering-type></col>
      <col><mnemonic>thread</mnemonic><name>Thread</name><engineering-type>thread</engineering-type></col>
      <col><mnemonic>name</mnemonic><name>Thread Name</name><engineering-type>thread-name</engineering-type></col>
      <col><mnemonic>main-thread</mnemonic><name>Main Thread</name><engineering-type>boolean</engineering-type></col>
    </schema>
    <row>
      <event-time id="1" fmt="00:00.000.000">0</event-time>
      <pid id="2" fmt="42">42</pid>
      <tid id="3" fmt="0x1">1</tid>
      <process id="4" fmt="OxideHost (42)">42</process>
      <thread id="5" fmt="Main Thread (0x1) (OxideHost, pid: 42)"><tid ref="3"/><process ref="4"/></thread>
      <thread-name id="6" fmt="main  0x1"><string id="7" fmt="main">main</string><thread ref="5"/></thread-name>
      <boolean id="8" fmt="Yes">1</boolean>
    </row>
    <row>
      <event-time ref="1"/>
      <pid ref="2"/>
      <tid id="9" fmt="0x2">2</tid>
      <process ref="4"/>
      <thread id="10" fmt="OxideHost (0x2) (OxideHost, pid: 42)"><tid ref="9"/><process ref="4"/></thread>
      <thread-name id="11" fmt="oxide-tokio-0  0x2"><string id="12" fmt="oxide-tokio-0">oxide-tokio-0</string><thread ref="10"/></thread-name>
      <boolean id="13" fmt="No">0</boolean>
    </row>
  </node>
</trace-query-result>
"#;
    let windows =
        vec![TraceWindow { start_ns: 90, end_ns: 250, process_name: String::from("OxideHost") }];

    let summary = summarize_time_profile_from_xml(
        &PathBuf::from("/tmp/demo.trace"),
        profile_xml,
        thread_info_xml,
        &windows,
    )
    .expect("summarize time profiler xml");

    assert_eq!(summary.sample_rows_with_backtraces, 2);
    assert_eq!(summary.top_threads[0].samples, 1);
    assert!(summary
        .bucket_counts
        .iter()
        .any(|entry| entry.bucket == "renderer_driver_present" && entry.samples == 1));
    assert!(summary
        .bucket_counts
        .iter()
        .any(|entry| entry.bucket == "sample_delivery" && entry.samples == 1));
    assert!(!summary.bucket_counts.iter().any(|entry| entry.bucket == "benchmark_summary"));
    assert!(summary.worker_thread_naming.tokio_named_threads_visible_in_thread_info);
    assert!(summary.worker_thread_naming.tokio_named_threads_visible_in_sampled_rows);
}

#[test]
fn extract_trace_windows_from_tables_pairs_signposts() {
    let xml = r#"
<?xml version="1.0"?>
<trace-query-result>
  <node xpath='//trace-toc[1]/run[1]/data[1]/table[1]'>
    <schema name="os-signpost">
      <col><mnemonic>time</mnemonic><name>Timestamp</name><engineering-type>event-time</engineering-type></col>
      <col><mnemonic>process</mnemonic><name>Process</name><engineering-type>process</engineering-type></col>
      <col><mnemonic>event-type</mnemonic><name>Event Type</name><engineering-type>event-type</engineering-type></col>
      <col><mnemonic>identifier</mnemonic><name>Identifier</name><engineering-type>os-signpost-identifier</engineering-type></col>
      <col><mnemonic>name</mnemonic><name>Name</name><engineering-type>signpost-name</engineering-type></col>
      <col><mnemonic>subsystem</mnemonic><name>Subsystem</name><engineering-type>subsystem</engineering-type></col>
      <col><mnemonic>category</mnemonic><name>Category</name><engineering-type>category</engineering-type></col>
    </schema>
    <row>
      <event-time id="1" fmt="00:00.000.100">100</event-time>
      <process id="2" fmt="PerfTests (42)">42</process>
      <event-type id="3" fmt="Begin Interval">1</event-type>
      <os-signpost-identifier id="4" fmt="A">A</os-signpost-identifier>
      <signpost-name id="5" fmt="PerfWorkload">PerfWorkload</signpost-name>
      <subsystem id="6" fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category id="7" fmt="PointsOfInterest">PointsOfInterest</category>
    </row>
    <row>
      <event-time fmt="00:00.000.250">250</event-time>
      <process ref="2"/>
      <event-type fmt="End Interval">2</event-type>
      <os-signpost-identifier ref="4"/>
      <signpost-name ref="5"/>
      <subsystem ref="6"/>
      <category ref="7"/>
    </row>
  </node>
</trace-query-result>
"#;

    let tables = parse_xctrace_tables(xml).expect("parse signpost table");
    let windows = extract_trace_windows_from_tables(&tables).expect("extract trace windows");
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].start_ns, 100);
    assert_eq!(windows[0].end_ns, 250);
    assert_eq!(windows[0].process_name, "PerfTests");
}

#[test]
fn extract_trace_windows_from_tables_sorts_backdated_signposts() {
    let xml = r#"
<?xml version="1.0"?>
<trace-query-result>
  <node xpath='//trace-toc[1]/run[1]/data[1]/table[1]'>
    <schema name="os-signpost">
      <col><mnemonic>time</mnemonic><name>Timestamp</name><engineering-type>event-time</engineering-type></col>
      <col><mnemonic>process</mnemonic><name>Process</name><engineering-type>process</engineering-type></col>
      <col><mnemonic>event-type</mnemonic><name>Event Type</name><engineering-type>event-type</engineering-type></col>
      <col><mnemonic>identifier</mnemonic><name>Identifier</name><engineering-type>os-signpost-identifier</engineering-type></col>
      <col><mnemonic>name</mnemonic><name>Name</name><engineering-type>signpost-name</engineering-type></col>
      <col><mnemonic>subsystem</mnemonic><name>Subsystem</name><engineering-type>subsystem</engineering-type></col>
      <col><mnemonic>category</mnemonic><name>Category</name><engineering-type>category</engineering-type></col>
    </schema>
    <row>
      <event-time fmt="00:00.000.250">250</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="End Interval">2</event-type>
      <os-signpost-identifier fmt="A">A</os-signpost-identifier>
      <signpost-name fmt="PerfWorkload">PerfWorkload</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="PointsOfInterest">PointsOfInterest</category>
    </row>
    <row>
      <event-time fmt="00:00.000.100">100</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="Begin Interval">1</event-type>
      <os-signpost-identifier fmt="A">A</os-signpost-identifier>
      <signpost-name fmt="PerfWorkload">PerfWorkload</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="PointsOfInterest">PointsOfInterest</category>
    </row>
  </node>
</trace-query-result>
"#;

    let tables = parse_xctrace_tables(xml).expect("parse signpost table");
    let windows = extract_trace_windows_from_tables(&tables).expect("extract trace windows");
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].start_ns, 100);
    assert_eq!(windows[0].end_ns, 250);
}

#[test]
fn extract_trace_windows_from_tables_ignores_clipped_edge_events() {
    let xml = r#"
<?xml version="1.0"?>
<trace-query-result>
  <node xpath='//trace-toc[1]/run[1]/data[1]/table[1]'>
    <schema name="os-signpost">
      <col><mnemonic>time</mnemonic><name>Timestamp</name><engineering-type>event-time</engineering-type></col>
      <col><mnemonic>process</mnemonic><name>Process</name><engineering-type>process</engineering-type></col>
      <col><mnemonic>event-type</mnemonic><name>Event Type</name><engineering-type>event-type</engineering-type></col>
      <col><mnemonic>identifier</mnemonic><name>Identifier</name><engineering-type>os-signpost-identifier</engineering-type></col>
      <col><mnemonic>name</mnemonic><name>Name</name><engineering-type>signpost-name</engineering-type></col>
      <col><mnemonic>subsystem</mnemonic><name>Subsystem</name><engineering-type>subsystem</engineering-type></col>
      <col><mnemonic>category</mnemonic><name>Category</name><engineering-type>category</engineering-type></col>
    </schema>
    <row>
      <event-time fmt="00:00.000.050">50</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="End Interval">2</event-type>
      <os-signpost-identifier fmt="A">A</os-signpost-identifier>
      <signpost-name fmt="PerfWorkload">PerfWorkload</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="PointsOfInterest">PointsOfInterest</category>
    </row>
    <row>
      <event-time fmt="00:00.000.100">100</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="Begin Interval">1</event-type>
      <os-signpost-identifier fmt="B">B</os-signpost-identifier>
      <signpost-name fmt="PerfWorkload">PerfWorkload</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="PointsOfInterest">PointsOfInterest</category>
    </row>
    <row>
      <event-time fmt="00:00.000.250">250</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="End Interval">2</event-type>
      <os-signpost-identifier fmt="B">B</os-signpost-identifier>
      <signpost-name fmt="PerfWorkload">PerfWorkload</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="PointsOfInterest">PointsOfInterest</category>
    </row>
    <row>
      <event-time fmt="00:00.000.300">300</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="Begin Interval">1</event-type>
      <os-signpost-identifier fmt="C">C</os-signpost-identifier>
      <signpost-name fmt="PerfWorkload">PerfWorkload</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="PointsOfInterest">PointsOfInterest</category>
    </row>
  </node>
</trace-query-result>
"#;

    let tables = parse_xctrace_tables(xml).expect("parse signpost table");
    let windows = extract_trace_windows_from_tables(&tables).expect("extract trace windows");
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].start_ns, 100);
    assert_eq!(windows[0].end_ns, 250);
}

#[test]
fn extract_trace_windows_from_tables_uses_region_of_interest_perfworkload() {
    let xml = r#"
<?xml version="1.0"?>
<trace-query-result>
  <node xpath='//trace-toc[1]/run[1]/data[1]/table[1]'>
    <schema name="region-of-interest">
      <col><mnemonic>start</mnemonic><name>Start</name><engineering-type>start-time</engineering-type></col>
      <col><mnemonic>name</mnemonic><name>Name</name><engineering-type>signpost-name</engineering-type></col>
      <col><mnemonic>duration</mnemonic><name>Duration</name><engineering-type>duration</engineering-type></col>
      <col><mnemonic>subsystem</mnemonic><name>Subsystem</name><engineering-type>subsystem</engineering-type></col>
      <col><mnemonic>process</mnemonic><name>Start Process</name><engineering-type>process</engineering-type></col>
    </schema>
    <row>
      <start-time fmt="00:10.808.576">10808576916</start-time>
      <signpost-name fmt="PerfWorkload">PerfWorkload</signpost-name>
      <duration fmt="100.87 ms">100871230</duration>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <process fmt="OxideHost (18296)">18296</process>
    </row>
  </node>
</trace-query-result>
"#;

    let tables = parse_xctrace_tables(xml).expect("parse roi table");
    let windows = extract_trace_windows_from_tables(&tables).expect("extract trace windows");
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].start_ns, 10_808_576_916);
    assert_eq!(windows[0].end_ns, 10_909_448_146);
    assert_eq!(windows[0].process_name, "OxideHost");
}

#[test]
fn extract_trace_windows_from_tables_uses_phase_roi_when_perfworkload_is_missing() {
    let xml = r#"
<?xml version="1.0"?>
<trace-query-result>
  <node xpath='//trace-toc[1]/run[1]/data[1]/table[1]'>
    <schema name="region-of-interest">
      <col><mnemonic>start</mnemonic><name>Start</name><engineering-type>start-time</engineering-type></col>
      <col><mnemonic>name</mnemonic><name>Name</name><engineering-type>signpost-name</engineering-type></col>
      <col><mnemonic>duration</mnemonic><name>Duration</name><engineering-type>duration</engineering-type></col>
      <col><mnemonic>subsystem</mnemonic><name>Subsystem</name><engineering-type>subsystem</engineering-type></col>
      <col><mnemonic>process</mnemonic><name>Start Process</name><engineering-type>process</engineering-type></col>
    </schema>
    <row>
      <start-time fmt="00:00.884.761">884761541</start-time>
      <signpost-name fmt="screen.mount">screen.mount</signpost-name>
      <duration fmt="20.00 ms">20000000</duration>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <process fmt="Oxide Demo (1013)">1013</process>
    </row>
    <row>
      <start-time fmt="00:00.885.177">885177250</start-time>
      <signpost-name fmt="draw.encode">draw.encode</signpost-name>
      <duration fmt="291 ns">291</duration>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <process fmt="Oxide Demo (1013)">1013</process>
    </row>
    <row>
      <start-time fmt="00:00.885.180">885180208</start-time>
      <signpost-name fmt="frame.present">frame.present</signpost-name>
      <duration fmt="43.33 µs">43333</duration>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <process fmt="Oxide Demo (1013)">1013</process>
    </row>
  </node>
</trace-query-result>
"#;

    let tables = parse_xctrace_tables(xml).expect("parse roi table");
    let windows = extract_trace_windows_from_tables(&tables).expect("extract trace windows");
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].start_ns, 884_761_541);
    assert_eq!(windows[0].end_ns, 904_761_541);
    assert_eq!(windows[0].process_name, "Oxide Demo");
}

#[test]
fn summarize_trace_signpost_metrics_from_tables_collects_stage_durations() {
    let xml = r#"
<?xml version="1.0"?>
<trace-query-result>
  <node xpath='//trace-toc[1]/run[1]/data[1]/table[1]'>
    <schema name="os-signpost">
      <col><mnemonic>time</mnemonic><name>Timestamp</name><engineering-type>event-time</engineering-type></col>
      <col><mnemonic>process</mnemonic><name>Process</name><engineering-type>process</engineering-type></col>
      <col><mnemonic>event-type</mnemonic><name>Event Type</name><engineering-type>event-type</engineering-type></col>
      <col><mnemonic>identifier</mnemonic><name>Identifier</name><engineering-type>os-signpost-identifier</engineering-type></col>
      <col><mnemonic>name</mnemonic><name>Name</name><engineering-type>signpost-name</engineering-type></col>
      <col><mnemonic>subsystem</mnemonic><name>Subsystem</name><engineering-type>subsystem</engineering-type></col>
      <col><mnemonic>category</mnemonic><name>Category</name><engineering-type>category</engineering-type></col>
    </schema>
    <row>
      <event-time fmt="00:01.000.000">1000000000</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="Begin Interval">1</event-type>
      <os-signpost-identifier fmt="W">W</os-signpost-identifier>
      <signpost-name fmt="PerfWorkload">PerfWorkload</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="PointsOfInterest">PointsOfInterest</category>
    </row>
    <row>
      <event-time fmt="00:05.000.000">5000000000</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="End Interval">2</event-type>
      <os-signpost-identifier fmt="W">W</os-signpost-identifier>
      <signpost-name fmt="PerfWorkload">PerfWorkload</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="PointsOfInterest">PointsOfInterest</category>
    </row>
    <row>
      <event-time fmt="00:00.900.000">900000000</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="Begin Interval">1</event-type>
      <os-signpost-identifier fmt="D">D</os-signpost-identifier>
      <signpost-name fmt="camera.drawable.acquire">camera.drawable.acquire</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="PointsOfInterest">PointsOfInterest</category>
    </row>
    <row>
      <event-time fmt="00:01.100.000">1100000000</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="End Interval">2</event-type>
      <os-signpost-identifier fmt="D">D</os-signpost-identifier>
      <signpost-name fmt="camera.drawable.acquire">camera.drawable.acquire</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="PointsOfInterest">PointsOfInterest</category>
    </row>
    <row>
      <event-time fmt="00:01.200.000">1200000000</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="Begin Interval">1</event-type>
      <os-signpost-identifier fmt="P1">P1</os-signpost-identifier>
      <signpost-name fmt="camera.capture.publish">camera.capture.publish</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="PointsOfInterest">PointsOfInterest</category>
    </row>
    <row>
      <event-time fmt="00:01.500.000">1500000000</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="End Interval">2</event-type>
      <os-signpost-identifier fmt="P1">P1</os-signpost-identifier>
      <signpost-name fmt="camera.capture.publish">camera.capture.publish</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="PointsOfInterest">PointsOfInterest</category>
    </row>
    <row>
      <event-time fmt="00:02.000.000">2000000000</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="Begin Interval">1</event-type>
      <os-signpost-identifier fmt="P2">P2</os-signpost-identifier>
      <signpost-name fmt="camera.capture.publish">camera.capture.publish</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="PointsOfInterest">PointsOfInterest</category>
    </row>
    <row>
      <event-time fmt="00:02.200.000">2200000000</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="End Interval">2</event-type>
      <os-signpost-identifier fmt="P2">P2</os-signpost-identifier>
      <signpost-name fmt="camera.capture.publish">camera.capture.publish</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="PointsOfInterest">PointsOfInterest</category>
    </row>
    <row>
      <event-time fmt="00:02.500.000">2500000000</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="Begin Interval">1</event-type>
      <os-signpost-identifier fmt="C">C</os-signpost-identifier>
      <signpost-name fmt="camera.renderer.direct.commit">camera.renderer.direct.commit</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="PointsOfInterest">PointsOfInterest</category>
    </row>
    <row>
      <event-time fmt="00:02.700.000">2700000000</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="End Interval">2</event-type>
      <os-signpost-identifier fmt="C">C</os-signpost-identifier>
      <signpost-name fmt="camera.renderer.direct.commit">camera.renderer.direct.commit</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="PointsOfInterest">PointsOfInterest</category>
    </row>
    <row>
      <event-time fmt="00:06.000.000">6000000000</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="Begin Interval">1</event-type>
      <os-signpost-identifier fmt="OUT">OUT</os-signpost-identifier>
      <signpost-name fmt="camera.capture.publish">camera.capture.publish</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="PointsOfInterest">PointsOfInterest</category>
    </row>
    <row>
      <event-time fmt="00:06.500.000">6500000000</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="End Interval">2</event-type>
      <os-signpost-identifier fmt="OUT">OUT</os-signpost-identifier>
      <signpost-name fmt="camera.capture.publish">camera.capture.publish</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="PointsOfInterest">PointsOfInterest</category>
    </row>
  </node>
</trace-query-result>
"#;

    let tables = parse_xctrace_tables(xml).expect("parse signpost table");
    let windows = extract_trace_windows_from_tables(&tables).expect("extract trace windows");
    let metrics = summarize_trace_signpost_metrics_from_tables(&tables, &windows, &[])
        .expect("summarize signpost metrics");

    assert!(!metrics.contains_key("signpost_perfworkload_s"));
    let drawable =
        metrics.get("signpost_camera_drawable_acquire_s").expect("drawable acquire metric");
    assert_eq!(drawable.samples, 1);
    assert!((drawable.median - 0.1).abs() < 1e-9);
    assert_eq!(drawable.source, UIKitMetricSource::XctraceSignpost);
    assert!(drawable.fallback_modes.is_empty());

    let publish = metrics.get("signpost_camera_capture_publish_s").expect("publish metric");
    assert_eq!(publish.samples, 2);
    assert!((publish.min - 0.2).abs() < 1e-9);
    assert!((publish.max - 0.3).abs() < 1e-9);
    assert!((publish.median - 0.25).abs() < 1e-9);

    let commit = metrics.get("signpost_camera_renderer_direct_commit_s").expect("commit metric");
    assert_eq!(commit.samples, 1);
    assert!((commit.median - 0.2).abs() < 1e-9);
}

#[test]
fn summarize_trace_signpost_metrics_from_tables_accepts_lowercase_points_of_interest_category() {
    let xml = r#"
<?xml version="1.0"?>
<trace-query-result>
  <node xpath='//trace-toc[1]/run[1]/data[1]/table[1]'>
    <schema name="os-signpost">
      <col><mnemonic>time</mnemonic><name>Timestamp</name><engineering-type>event-time</engineering-type></col>
      <col><mnemonic>process</mnemonic><name>Process</name><engineering-type>process</engineering-type></col>
      <col><mnemonic>event-type</mnemonic><name>Event Type</name><engineering-type>event-type</engineering-type></col>
      <col><mnemonic>identifier</mnemonic><name>Identifier</name><engineering-type>os-signpost-identifier</engineering-type></col>
      <col><mnemonic>name</mnemonic><name>Name</name><engineering-type>signpost-name</engineering-type></col>
      <col><mnemonic>subsystem</mnemonic><name>Subsystem</name><engineering-type>subsystem</engineering-type></col>
      <col><mnemonic>category</mnemonic><name>Category</name><engineering-type>category</engineering-type></col>
    </schema>
    <row>
      <event-time fmt="00:01.000.000">1000000000</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="Begin Interval">1</event-type>
      <os-signpost-identifier fmt="W">W</os-signpost-identifier>
      <signpost-name fmt="PerfWorkload">PerfWorkload</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="pointsOfInterest">pointsOfInterest</category>
    </row>
    <row>
      <event-time fmt="00:05.000.000">5000000000</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="End Interval">2</event-type>
      <os-signpost-identifier fmt="W">W</os-signpost-identifier>
      <signpost-name fmt="PerfWorkload">PerfWorkload</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="pointsOfInterest">pointsOfInterest</category>
    </row>
    <row>
      <event-time fmt="00:02.500.000">2500000000</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="Begin Interval">1</event-type>
      <os-signpost-identifier fmt="C">C</os-signpost-identifier>
      <signpost-name fmt="camera.renderer.direct.commit">camera.renderer.direct.commit</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="pointsOfInterest">pointsOfInterest</category>
    </row>
    <row>
      <event-time fmt="00:02.700.000">2700000000</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="End Interval">2</event-type>
      <os-signpost-identifier fmt="C">C</os-signpost-identifier>
      <signpost-name fmt="camera.renderer.direct.commit">camera.renderer.direct.commit</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="pointsOfInterest">pointsOfInterest</category>
    </row>
  </node>
</trace-query-result>
"#;

    let tables = parse_xctrace_tables(xml).expect("parse signpost table");
    let windows = extract_trace_windows_from_tables(&tables).expect("extract trace windows");
    let metrics = summarize_trace_signpost_metrics_from_tables(&tables, &windows, &[])
        .expect("summarize signpost metrics");

    let commit = metrics.get("signpost_camera_renderer_direct_commit_s").expect("commit metric");
    assert_eq!(commit.samples, 1);
    assert!((commit.median - 0.2).abs() < 1e-9);
}

#[test]
fn summarize_trace_signpost_metrics_from_tables_prefers_region_of_interest_intervals() {
    let xml = r#"
<?xml version="1.0"?>
<trace-query-result>
  <node xpath='//trace-toc[1]/run[1]/data[1]/table[1]'>
    <schema name="region-of-interest">
      <col><mnemonic>start</mnemonic><name>Start</name><engineering-type>start-time</engineering-type></col>
      <col><mnemonic>name</mnemonic><name>Name</name><engineering-type>signpost-name</engineering-type></col>
      <col><mnemonic>duration</mnemonic><name>Duration</name><engineering-type>duration</engineering-type></col>
      <col><mnemonic>subsystem</mnemonic><name>Subsystem</name><engineering-type>subsystem</engineering-type></col>
      <col><mnemonic>process</mnemonic><name>Start Process</name><engineering-type>process</engineering-type></col>
    </schema>
    <row>
      <start-time fmt="00:01.000.000">1000000000</start-time>
      <signpost-name fmt="PerfWorkload">PerfWorkload</signpost-name>
      <duration fmt="4.0 s">4000000000</duration>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <process fmt="PerfTests (42)">42</process>
    </row>
    <row>
      <start-time fmt="00:02.500.000">2500000000</start-time>
      <signpost-name fmt="camera.renderer.direct.commit">camera.renderer.direct.commit</signpost-name>
      <duration fmt="40.0 µs">40000</duration>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <process fmt="PerfTests (42)">42</process>
    </row>
  </node>
  <node xpath='//trace-toc[1]/run[1]/data[1]/table[2]'>
    <schema name="os-signpost">
      <col><mnemonic>time</mnemonic><name>Timestamp</name><engineering-type>event-time</engineering-type></col>
      <col><mnemonic>process</mnemonic><name>Process</name><engineering-type>process</engineering-type></col>
      <col><mnemonic>event-type</mnemonic><name>Event Type</name><engineering-type>event-type</engineering-type></col>
      <col><mnemonic>identifier</mnemonic><name>Identifier</name><engineering-type>os-signpost-identifier</engineering-type></col>
      <col><mnemonic>name</mnemonic><name>Name</name><engineering-type>signpost-name</engineering-type></col>
      <col><mnemonic>subsystem</mnemonic><name>Subsystem</name><engineering-type>subsystem</engineering-type></col>
      <col><mnemonic>category</mnemonic><name>Category</name><engineering-type>category</engineering-type></col>
    </schema>
    <row>
      <event-time fmt="00:01.000.000">1000000000</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="Begin Interval">1</event-type>
      <os-signpost-identifier fmt="W">W</os-signpost-identifier>
      <signpost-name fmt="PerfWorkload">PerfWorkload</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="PointsOfInterest">PointsOfInterest</category>
    </row>
    <row>
      <event-time fmt="00:05.000.000">5000000000</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="End Interval">2</event-type>
      <os-signpost-identifier fmt="W">W</os-signpost-identifier>
      <signpost-name fmt="PerfWorkload">PerfWorkload</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="PointsOfInterest">PointsOfInterest</category>
    </row>
    <row>
      <event-time fmt="00:02.500.000">2500000000</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="Begin Interval">1</event-type>
      <os-signpost-identifier fmt="C">C</os-signpost-identifier>
      <signpost-name fmt="camera.renderer.direct.commit">camera.renderer.direct.commit</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="PointsOfInterest">PointsOfInterest</category>
    </row>
    <row>
      <event-time fmt="00:02.700.000">2700000000</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="End Interval">2</event-type>
      <os-signpost-identifier fmt="C">C</os-signpost-identifier>
      <signpost-name fmt="camera.renderer.direct.commit">camera.renderer.direct.commit</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="PointsOfInterest">PointsOfInterest</category>
    </row>
  </node>
</trace-query-result>
"#;

    let tables = parse_xctrace_tables(xml).expect("parse roi + signpost tables");
    let windows = extract_trace_windows_from_tables(&tables).expect("extract trace windows");
    let metrics = summarize_trace_signpost_metrics_from_tables(&tables, &windows, &[])
        .expect("summarize signpost metrics");

    let commit = metrics.get("signpost_camera_renderer_direct_commit_s").expect("commit metric");
    assert_eq!(commit.samples, 1);
    assert!((commit.median - 0.00004).abs() < 1e-12);
}

#[test]
fn summarize_trace_signpost_metrics_from_tables_ignores_non_perf_signpost_tables() {
    let xml = r#"
<?xml version="1.0"?>
<trace-query-result>
  <node xpath='//trace-toc[1]/run[1]/data[1]/table[1]'>
    <schema name="os-signpost">
      <col><mnemonic>time</mnemonic><name>Timestamp</name><engineering-type>event-time</engineering-type></col>
      <col><mnemonic>process</mnemonic><name>Process</name><engineering-type>process</engineering-type></col>
      <col><mnemonic>event-type</mnemonic><name>Event Type</name><engineering-type>event-type</engineering-type></col>
      <col><mnemonic>identifier</mnemonic><name>Identifier</name><engineering-type>os-signpost-identifier</engineering-type></col>
      <col><mnemonic>name</mnemonic><name>Name</name><engineering-type>signpost-name</engineering-type></col>
      <col><mnemonic>subsystem</mnemonic><name>Subsystem</name><engineering-type>subsystem</engineering-type></col>
      <col><mnemonic>category</mnemonic><name>Category</name><engineering-type>category</engineering-type></col>
    </schema>
    <row>
      <event-time fmt="00:01.000.000">1000000000</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="Begin Interval">1</event-type>
      <os-signpost-identifier fmt="CA">CA</os-signpost-identifier>
      <signpost-name fmt="ClientDrawable">ClientDrawable</signpost-name>
      <subsystem fmt="com.apple.coreanimation">com.apple.coreanimation</subsystem>
      <category fmt="CAMetalLayer">CAMetalLayer</category>
    </row>
    <row>
      <event-time fmt="00:01.500.000">1500000000</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="End Interval">2</event-type>
      <os-signpost-identifier fmt="CA">CA</os-signpost-identifier>
      <signpost-name fmt="ClientDrawable">ClientDrawable</signpost-name>
      <subsystem fmt="com.apple.coreanimation">com.apple.coreanimation</subsystem>
      <category fmt="CAMetalLayer">CAMetalLayer</category>
    </row>
  </node>
  <node xpath='//trace-toc[1]/run[1]/data[1]/table[2]'>
    <schema name="os-signpost">
      <col><mnemonic>time</mnemonic><name>Timestamp</name><engineering-type>event-time</engineering-type></col>
      <col><mnemonic>process</mnemonic><name>Process</name><engineering-type>process</engineering-type></col>
      <col><mnemonic>event-type</mnemonic><name>Event Type</name><engineering-type>event-type</engineering-type></col>
      <col><mnemonic>identifier</mnemonic><name>Identifier</name><engineering-type>os-signpost-identifier</engineering-type></col>
      <col><mnemonic>name</mnemonic><name>Name</name><engineering-type>signpost-name</engineering-type></col>
      <col><mnemonic>subsystem</mnemonic><name>Subsystem</name><engineering-type>subsystem</engineering-type></col>
      <col><mnemonic>category</mnemonic><name>Category</name><engineering-type>category</engineering-type></col>
    </schema>
    <row>
      <event-time fmt="00:01.000.000">1000000000</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="Begin Interval">1</event-type>
      <os-signpost-identifier fmt="W">W</os-signpost-identifier>
      <signpost-name fmt="PerfWorkload">PerfWorkload</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="PointsOfInterest">PointsOfInterest</category>
    </row>
    <row>
      <event-time fmt="00:05.000.000">5000000000</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="End Interval">2</event-type>
      <os-signpost-identifier fmt="W">W</os-signpost-identifier>
      <signpost-name fmt="PerfWorkload">PerfWorkload</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="PointsOfInterest">PointsOfInterest</category>
    </row>
    <row>
      <event-time fmt="00:02.500.000">2500000000</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="Begin Interval">1</event-type>
      <os-signpost-identifier fmt="C">C</os-signpost-identifier>
      <signpost-name fmt="camera.renderer.direct.commit">camera.renderer.direct.commit</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="PointsOfInterest">PointsOfInterest</category>
    </row>
    <row>
      <event-time fmt="00:02.700.000">2700000000</event-time>
      <process fmt="PerfTests (42)">42</process>
      <event-type fmt="End Interval">2</event-type>
      <os-signpost-identifier fmt="C">C</os-signpost-identifier>
      <signpost-name fmt="camera.renderer.direct.commit">camera.renderer.direct.commit</signpost-name>
      <subsystem fmt="com.oxide.perf">com.oxide.perf</subsystem>
      <category fmt="PointsOfInterest">PointsOfInterest</category>
    </row>
  </node>
</trace-query-result>
"#;

    let tables = parse_xctrace_tables(xml).expect("parse signpost tables");
    let windows = extract_trace_windows_from_tables(&tables).expect("extract trace windows");
    let metrics = summarize_trace_signpost_metrics_from_tables(&tables, &windows, &[])
        .expect("summarize signpost metrics");

    let commit = metrics.get("signpost_camera_renderer_direct_commit_s").expect("commit metric");
    assert_eq!(commit.samples, 1);
    assert!((commit.median - 0.2).abs() < 1e-9);
    assert!(!metrics.contains_key("signpost_clientdrawable_s"));
}

#[test]
fn summarize_energy_table_converts_millijoules() {
    let xml = r#"
<?xml version="1.0"?>
<trace-query-result>
  <node xpath='//trace-toc[1]/run[1]/data[1]/table[1]'>
    <schema name="power-energy">
      <col><mnemonic>start</mnemonic><name>Start</name><engineering-type>start-time</engineering-type></col>
      <col><mnemonic>duration</mnemonic><name>Duration</name><engineering-type>duration</engineering-type></col>
      <col><mnemonic>process</mnemonic><name>Process</name><engineering-type>process</engineering-type></col>
      <col><mnemonic>energy</mnemonic><name>Energy</name><engineering-type>fixed-decimal</engineering-type></col>
    </schema>
    <row>
      <start-time fmt="00:00.000.100">100</start-time>
      <duration fmt="100 ns">100</duration>
      <process fmt="PerfTests (42)">42</process>
      <fixed-decimal fmt="2.00 mJ">2.00</fixed-decimal>
    </row>
    <row>
      <start-time fmt="00:00.000.300">300</start-time>
      <duration fmt="100 ns">100</duration>
      <process fmt="PerfTests (42)">42</process>
      <fixed-decimal fmt="4.00 mJ">4.00</fixed-decimal>
    </row>
  </node>
</trace-query-result>
"#;

    let table = parse_xctrace_tables(xml).expect("parse energy table").remove(0);
    let windows = vec![
        TraceWindow { start_ns: 100, end_ns: 200, process_name: String::from("PerfTests") },
        TraceWindow { start_ns: 300, end_ns: 400, process_name: String::from("PerfTests") },
    ];
    let summary = summarize_energy_table(&table, &windows).expect("summarize energy");
    assert_eq!(summary.samples, 2);
    assert!((summary.mean - 0.003).abs() < 1e-9);
    assert_eq!(summary.source, UIKitMetricSource::XctraceEnergy);
}

#[test]
fn summarize_device_gpu_metrics_from_tables_reports_compositor_fallback_detail() {
    let xml = r#"
<?xml version="1.0"?>
<trace-query-result>
  <node xpath='//trace-toc[1]/run[1]/data[1]/table[1]'>
    <schema name="metal-gpu-intervals">
      <col><mnemonic>start</mnemonic><name>Creation</name><engineering-type>start-time</engineering-type></col>
      <col><mnemonic>duration</mnemonic><name>Duration</name><engineering-type>duration</engineering-type></col>
      <col><mnemonic>start-latency</mnemonic><name>CPU to GPU Latency</name><engineering-type>duration</engineering-type></col>
      <col><mnemonic>process</mnemonic><name>Process</name><engineering-type>process</engineering-type></col>
    </schema>
    <row>
      <start-time fmt="00:00.000.110">110</start-time>
      <duration fmt="40 ns">40</duration>
      <duration fmt="700 ns">700</duration>
      <process fmt="backboardd (71)">71</process>
    </row>
    <row>
      <start-time fmt="00:00.000.150">150</start-time>
      <duration fmt="20 ns">20</duration>
      <duration fmt="900 ns">900</duration>
      <process fmt="backboardd (71)">71</process>
    </row>
  </node>
</trace-query-result>
"#;

    let table = parse_xctrace_tables(xml).expect("parse gpu table").remove(0);
    let windows = vec![TraceWindow {
        start_ns: 100,
        end_ns: 200,
        process_name: String::from("ReactNativeCameraBench"),
    }];
    let mut notes = Vec::new();
    let metrics = summarize_device_gpu_metrics_from_tables(&table, &windows, &mut notes, &[])
        .expect("summarize gpu metrics");

    let gpu_time = metrics.get("gpu_time_s").expect("gpu time metric");
    assert_eq!(gpu_time.samples, 1);
    assert!((gpu_time.median - 60e-9).abs() < 1e-18);
    assert_eq!(gpu_time.source, UIKitMetricSource::XctraceGpuInterval);
    assert!(gpu_time
        .fallback_modes
        .contains(&UIKitMetricFallbackMode::CompositorInclusiveGpuIntervals));

    let gpu_latency = metrics.get("gpu_latency_s").expect("gpu latency metric");
    assert_eq!(gpu_latency.samples, 1);
    assert!((gpu_latency.median - 800e-9).abs() < 1e-18);

    assert!(notes.iter().any(|note| note.contains("no direct target-process GPU intervals")));
    assert!(notes.iter().any(|note| note.contains("dominated by `backboardd`")));
    assert!(notes
        .iter()
        .any(|note| note
            .contains("`gpu_time_s` is the total overlapping Metal GPU execution duration")));
}

#[test]
fn display_value_to_base_converts_microunits() {
    let cell = XctraceCell { raw: Some(String::from("1200")), fmt: Some(String::from("1200 µW")) };
    let watts = display_value_to_base(&cell, "W").expect("convert microwatts");
    assert!((watts - 0.0012).abs() < 1e-9);
}

#[test]
fn parse_apple_development_team_from_security_output_prefers_development_identity() {
    let output = r#"
  1) 1234567890ABCDEF1234567890ABCDEF12345678 "Apple Distribution: Victor Stewart (6GQ7T2VDQ5)"
  2) ABCDEF1234567890ABCDEF1234567890ABCDEF12 "Apple Development: Victor Stewart (V9NL9Y9HC3)"
     2 valid identities found
"#;

    let team = parse_apple_development_team_from_security_output(output);
    assert_eq!(team.as_deref(), Some("V9NL9Y9HC3"));
}

#[test]
fn parse_provisioning_profile_team_identifier_reads_team_identifier() {
    let plist = r#"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>ProvisionedDevices</key>
  <array>
    <string>00008150-001529C434F8401C</string>
  </array>
  <key>TeamIdentifier</key>
  <array>
    <string>6GQ7T2VDQ5</string>
  </array>
</dict>
</plist>
"#;

    let team = parse_provisioning_profile_team_identifier(plist);
    assert_eq!(team.as_deref(), Some("6GQ7T2VDQ5"));
}

fn sample_uikit_device_metrics() -> BTreeMap<String, UIKitMetricSummary> {
    BTreeMap::from([
        (String::from("clock_s"), sample_metric(1.0)),
        (String::from("cpu_time_s"), sample_metric(1.0)),
        (String::from("cpu_cycles_kc"), sample_metric(1.0)),
        (String::from("memory_peak_kb"), sample_metric(1.0)),
        (String::from("gpu_time_s"), sample_metric(1.0)),
        (String::from("gpu_latency_s"), sample_metric(1.0)),
        (String::from("hitch_ms_per_s"), sample_metric(0.0)),
        (String::from("missed_frames"), sample_metric(0.0)),
        (String::from("missed_frames_per_s"), sample_metric(0.0)),
    ])
}

fn sample_uikit_device_report(metrics: BTreeMap<String, UIKitMetricSummary>) -> UIKitPerfReport {
    UIKitPerfReport {
        version: 1,
        suite: String::from("device"),
        generated_label: None,
        device_name: String::from("Victor’s iPhone"),
        energy_status: String::from("direct device"),
        contract: UIKitContractCoverageReport::default(),
        notes: Vec::new(),
        cases: vec![UIKitPerfCase {
            id: String::from("uikit.component.label.encode"),
            oxide_case_id: String::from("cpu.component.label.encode"),
            test_name: String::from("testLabelEncode"),
            layer: String::from("engine"),
            scenario: String::from("primitive-view"),
            style: String::from("idiomatic"),
            cache_state: String::from("warm"),
            refresh_mode: String::from("native"),
            threshold_pct: 0.10,
            notes: Vec::new(),
            metrics,
            ..UIKitPerfCase::default()
        }],
    }
}

fn sample_oxide_device_metrics() -> BTreeMap<String, f64> {
    let mut metrics = BTreeMap::from([
        (String::from("clock_s"), 1.0),
        (String::from("memory_peak_kb"), 42.0),
        (String::from("gpu_time_s"), 0.1),
        (String::from("gpu_latency_s"), 0.001),
        (String::from("hitch_ms_per_s"), 0.0),
        (String::from("missed_frames"), 0.0),
        (String::from("missed_frames_per_s"), 0.0),
    ]);
    for (metric_name, value) in [
        ("gpu_time_s", 0.1),
        ("gpu_latency_s", 0.001),
        ("hitch_ms_per_s", 0.0),
        ("missed_frames", 0.0),
        ("missed_frames_per_s", 0.0),
    ] {
        insert_sample_oxide_distribution(&mut metrics, metric_name, value);
    }
    metrics
}

fn insert_sample_oxide_distribution(
    metrics: &mut BTreeMap<String, f64>,
    metric_name: &str,
    value: f64,
) {
    metrics.insert(format!("{}_p50", metric_name), value);
    metrics.insert(format!("{}_p95", metric_name), value);
    metrics.insert(format!("{}_p99", metric_name), value);
    metrics.insert(format!("{}_peak", metric_name), value);
    metrics.insert(format!("{}_samples", metric_name), 3.0);
}

fn sample_oxide_device_report(metrics: BTreeMap<String, f64>) -> PerfReport {
    PerfReport {
        version: 1,
        suite: String::from("oxide-device"),
        generated_label: None,
        cases: vec![PerfCaseResult {
            id: String::from("cpu.animation.spinner_spin"),
            refresh_mode: String::from("native"),
            gated: true,
            metrics,
            ..PerfCaseResult::default()
        }],
        coverage: CoverageReport::default(),
        contract: ContractCoverageReport::default(),
        findings: Vec::new(),
    }
}

fn notes_contain(notes: &[String], needle: &str) -> bool {
    let needle = needle.to_ascii_lowercase();
    notes.iter().any(|note| note.to_ascii_lowercase().contains(&needle))
}

fn sample_metric(median: f64) -> UIKitMetricSummary {
    UIKitMetricSummary {
        unit: String::from("test"),
        min: median,
        max: median,
        mean: median,
        median,
        p95: median,
        p99: median,
        samples: 3,
        source: UIKitMetricSource::Unknown,
        fallback_modes: Vec::new(),
    }
}
