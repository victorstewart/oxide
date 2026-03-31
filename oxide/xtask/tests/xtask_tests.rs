use base64::Engine;
use plist::{Dictionary, Value as PlValue};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use tempfile::tempdir;
use xtask::{
    apply_xctestrun_environment_overrides, build_entitlements_dict, compare_uikit_reports,
    console_output_contains_marker, device_process_name, device_support_dir_matches,
    devicectl_notification_observed, display_value_to_base, extract_oxide_device_report_json,
    extract_trace_windows_from_tables, find_device_process_ids,
    format_uikit_only_testing_identifier, is_expected_devicectl_console_termination,
    is_primary_built_xctestrun_file, is_unsupported_gpu_counter_profile_error,
    is_xctrace_trace_bundle, map_uikit_case, merge_background_modes, merge_usage_strings,
    merge_xcresult_metrics_json_fragments, normalize_ios_version_for_device_support,
    notification_or_console_marker_observed, parse_apple_development_team_from_security_output,
    parse_available_ios_sim_destination, parse_oxide_camera_contract_summary,
    parse_oxide_memory_summary, parse_oxide_stage_summary,
    parse_provisioning_profile_team_identifier, parse_react_native_device_report_json,
    parse_uikit_report_json, parse_xctrace_summary_window, parse_xctrace_tables,
    parse_xctrace_toc_tables, preferred_xctrace_toc_tables, prepare_uikit_device_perf_xctestrun,
    resolve_existing_uikit_power_trace, summarize_device_gpu_metrics_from_tables,
    summarize_energy_table, summarize_time_profile_from_xml,
    summarize_trace_signpost_metrics_from_tables, uikit_device_metrics_case_stdout_path,
    uikit_device_perf_environment_for_test_name, uikit_device_trace_artifact_exists,
    uikit_device_trace_enabled, uikit_only_testing_identifier_for_test_name,
    uikit_perf_environment_json_for_test_name, uikit_power_trace_candidate_paths,
    validate_normalized_camera_contract, Entitlements, LocationMode, TraceWindow,
    UIKitContractCoverageReport, UIKitMetricSummary, UIKitPerfCase, UIKitPerfReport, XctraceCell,
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
fn uikit_device_metrics_case_stdout_path_uses_case_name_suffix() {
    let root = tempdir().expect("tempdir");
    let path = uikit_device_metrics_case_stdout_path(
        root.path(),
        "60hz",
        "testCameraNV12LegacyRealAppLivePreview",
    );
    assert_eq!(
        path,
        root.path().join("metrics-60hz-testCameraNV12LegacyRealAppLivePreview.stdout.log")
    );
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
    assert_eq!(report.contract.styles[0].status, "implemented");
    assert_eq!(report.cases[0].metrics["clock_s"].median, 0.002);
    assert_eq!(report.cases[0].metrics["memory_peak_kb"].p95, 21.0);
    assert_eq!(report.cases[0].metrics["workload_s"].median, 0.008);
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
fn uikit_device_perf_environment_for_real_app_camera_cases_includes_case_specific_flags() {
    let custom_env = uikit_device_perf_environment_for_test_name(
        "testCameraNV12LegacyRealAppLivePreview",
        "60hz",
    )
    .expect("custom real-app xctestrun environment");
    let custom_map: BTreeMap<String, String> = custom_env.into_iter().collect();
    assert_eq!(custom_map.get("OXIDE_PERF_REFRESH_MODE").map(String::as_str), Some("60hz"));
    assert_eq!(custom_map.get("OXIDE_RENDER_IN_TEST").map(String::as_str), Some("1"));
    assert_eq!(custom_map.get("OXIDE_PERF_CAMERA_REAL_APP_HOST").map(String::as_str), Some("1"));
    assert!(!custom_map.contains_key("OXIDE_PERF_CAMERA_REAL_APP_HYBRID_VISIBLE_PREVIEW"));

    let hybrid_env = uikit_device_perf_environment_for_test_name(
        "testCameraNV12LegacyRealAppHybridPreviewLayerLivePreview",
        "60hz",
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
fn uikit_perf_environment_forwards_camera_benchmark_overrides() {
    with_env_vars(
        &[
            ("OXIDE_PERF_CAMERA_PREVIEW_SURFACE_SCALE", Some("0.5")),
            ("OXIDE_PERF_CAMERA_CAPTURE_CONTRACT_MODE", Some("preset-720p")),
            ("OXIDE_PERF_CAMERA_STAGE_MEASUREMENT", Some("0")),
            ("OXIDE_PERF_CAMERA_TINY_PREVIEW_RENDERER", Some("1")),
            ("OXIDE_PERF_CAMERA_PREVIEW_BACKPRESSURE", Some("1")),
        ],
        || {
            let json = uikit_perf_environment_json_for_test_name(
                "testCameraNV12LegacyLivePreview",
                "60hz",
            )
            .expect("camera environment json");
            assert!(json.contains("\"OXIDE_PERF_CAMERA_PREVIEW_SURFACE_SCALE\":\"0.5\""));
            assert!(json.contains("\"OXIDE_PERF_CAMERA_CAPTURE_CONTRACT_MODE\":\"preset-720p\""));
            assert!(json.contains("\"OXIDE_PERF_CAMERA_STAGE_MEASUREMENT\":\"0\""));
            assert!(json.contains("\"OXIDE_PERF_CAMERA_TINY_PREVIEW_RENDERER\":\"1\""));
            assert!(json.contains("\"OXIDE_PERF_CAMERA_PREVIEW_BACKPRESSURE\":\"1\""));
        },
    );
}

#[test]
fn uikit_perf_environment_enables_real_app_camera_host_for_real_app_cases() {
    let custom_json =
        uikit_perf_environment_json_for_test_name("testCameraNV12LegacyRealAppLivePreview", "60hz")
            .expect("real app custom environment json");
    assert!(custom_json.contains("\"OXIDE_RENDER_IN_TEST\":\"1\""));
    assert!(custom_json.contains("\"OXIDE_PERF_CAMERA_REAL_APP_HOST\":\"1\""));
    assert!(!custom_json.contains("\"OXIDE_PERF_CAMERA_REAL_APP_HYBRID_VISIBLE_PREVIEW\""));

    let hybrid_json = uikit_perf_environment_json_for_test_name(
        "testCameraNV12LegacyRealAppHybridPreviewLayerLivePreview",
        "60hz",
    )
    .expect("real app hybrid environment json");
    assert!(hybrid_json.contains("\"OXIDE_RENDER_IN_TEST\":\"1\""));
    assert!(hybrid_json.contains("\"OXIDE_PERF_CAMERA_REAL_APP_HOST\":\"1\""));
    assert!(hybrid_json.contains("\"OXIDE_PERF_CAMERA_REAL_APP_HYBRID_VISIBLE_PREVIEW\":\"1\""));
}

#[test]
fn selected_uikit_case_specs_includes_oxide_hybrid_preview_layer_live_case() {
    let selected =
        map_uikit_case("testCameraNV12LegacyHybridPreviewLayerLivePreview").expect("map case");
    assert_eq!(
        selected.0,
        "uikit.optimized.image_pipeline.camera_preview.nv12_legacy_hybrid_preview_layer_live"
    );
    assert_eq!(selected.1, "gpu.scene.camera.frame");
}

#[test]
fn selected_uikit_case_specs_include_real_app_camera_cases() {
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
fn unsupported_gpu_counter_profile_detection_matches_xctrace_error_text() {
    let text = "xcrun xctrace record --template Metal System Trace --instrument Metal GPU Counters failed with status 21: GPU Service reported error: Selected counter profile is not supported on target device";
    assert!(is_unsupported_gpu_counter_profile_error(text));
    assert!(!is_unsupported_gpu_counter_profile_error(
        "xcrun xctrace record failed with status 19: Cannot find process matching name: OxideHost"
    ));
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
            refresh_mode: String::from("device-default"),
            threshold_pct: 0.10,
            notes: Vec::new(),
            metrics: BTreeMap::from([
                (String::from("clock_s"), sample_metric(1.0)),
                (String::from("cpu_time_s"), sample_metric(1.0)),
                (String::from("cpu_cycles_kc"), sample_metric(1.0)),
                (String::from("memory_peak_kb"), sample_metric(1.0)),
                (String::from("gpu_time_s"), sample_metric(1.0)),
                (String::from("gpu_latency_s"), sample_metric(1.0)),
                (String::from("gpu_counter.shader_cycles"), sample_metric(1.30)),
            ]),
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
            refresh_mode: String::from("device-default"),
            threshold_pct: 0.10,
            notes: Vec::new(),
            metrics: BTreeMap::from([
                (String::from("clock_s"), sample_metric(1.0)),
                (String::from("cpu_time_s"), sample_metric(1.0)),
                (String::from("cpu_cycles_kc"), sample_metric(1.0)),
                (String::from("memory_peak_kb"), sample_metric(1.0)),
                (String::from("gpu_time_s"), sample_metric(1.0)),
                (String::from("gpu_latency_s"), sample_metric(1.0)),
                (String::from("energy_j"), sample_metric(1.0)),
                (String::from("gpu_counter.shader_cycles"), sample_metric(1.0)),
            ]),
        }],
    };

    let comparison = compare_uikit_reports(&current, &baseline);
    assert!(comparison.missing_baseline.is_empty());
    assert_eq!(comparison.regressions.len(), 1);
    assert_eq!(comparison.regressions[0].metric, "gpu_counter.shader_cycles");
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
            refresh_mode: String::from("device-default"),
            threshold_pct: 0.10,
            notes: Vec::new(),
            metrics: BTreeMap::from([
                (String::from("clock_s"), sample_metric(1.0)),
                (String::from("cpu_time_s"), sample_metric(1.0)),
                (String::from("cpu_cycles_kc"), sample_metric(1.0)),
                (String::from("memory_peak_kb"), sample_metric(1.0)),
                (String::from("gpu_time_s"), sample_metric(1.0)),
                (String::from("gpu_latency_s"), sample_metric(1.0)),
                (String::from("energy_j"), sample_metric(1.30)),
            ]),
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
            refresh_mode: String::from("device-default"),
            threshold_pct: 0.10,
            notes: Vec::new(),
            metrics: BTreeMap::from([
                (String::from("clock_s"), sample_metric(1.0)),
                (String::from("cpu_time_s"), sample_metric(1.0)),
                (String::from("cpu_cycles_kc"), sample_metric(1.0)),
                (String::from("memory_peak_kb"), sample_metric(1.0)),
                (String::from("gpu_time_s"), sample_metric(1.0)),
                (String::from("gpu_latency_s"), sample_metric(1.0)),
                (String::from("energy_j"), sample_metric(1.0)),
            ]),
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
            refresh_mode: String::from("60hz-capped"),
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
    let metrics = summarize_trace_signpost_metrics_from_tables(&tables, &windows)
        .expect("summarize signpost metrics");

    assert!(!metrics.contains_key("signpost_perfworkload_s"));
    let drawable =
        metrics.get("signpost_camera_drawable_acquire_s").expect("drawable acquire metric");
    assert_eq!(drawable.samples, 1);
    assert!((drawable.median - 0.1).abs() < 1e-9);

    let publish = metrics.get("signpost_camera_capture_publish_s").expect("publish metric");
    assert_eq!(publish.samples, 2);
    assert!((publish.min - 0.2).abs() < 1e-9);
    assert!((publish.max - 0.3).abs() < 1e-9);
    assert!((publish.median - 0.3).abs() < 1e-9);

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
    let metrics = summarize_trace_signpost_metrics_from_tables(&tables, &windows)
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
    let metrics = summarize_trace_signpost_metrics_from_tables(&tables, &windows)
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
    let metrics = summarize_trace_signpost_metrics_from_tables(&tables, &windows)
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
    let metrics = summarize_device_gpu_metrics_from_tables(&table, &windows, &mut notes)
        .expect("summarize gpu metrics");

    let gpu_time = metrics.get("gpu_time_s").expect("gpu time metric");
    assert_eq!(gpu_time.samples, 1);
    assert!((gpu_time.median - 60e-9).abs() < 1e-18);

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
    }
}
