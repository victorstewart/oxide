use anyhow::{bail, Context, Result};
use base64::Engine;
use oxide_perf_runner::{
    compare_reports, render_report_markdown, AuditFinding, ContractCoverageEntry,
    ContractCoverageReport, CoverageReport, PerfCaseResult, PerfReport,
};
use plist::{Dictionary, Value as PlValue};
use roxmltree::Document;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_OXIDE_DEVICE_BASELINE_JSON: &str = "benchmarks/oxide-device/latest.json";
const DEFAULT_OXIDE_DEVICE_BASELINE_MARKDOWN: &str = "benchmarks/oxide-device/latest.md";
const DEFAULT_OXIDE_DEVICE_RESULT_ROOT: &str = "/tmp/oxide-device-perf";
const DEFAULT_UIKIT_DEVICE_BASELINE_JSON: &str = "benchmarks/uikit-device/latest.json";
const DEFAULT_UIKIT_DEVICE_BASELINE_MARKDOWN: &str = "benchmarks/uikit-device/latest.md";
const DEFAULT_UIKIT_DEVICE_RESULT_ROOT: &str = "/tmp/oxide-uikit-device-perf";
const DEFAULT_REACT_DEVICE_BASELINE_JSON: &str = "benchmarks/react-native-device/latest.json";
const DEFAULT_REACT_DEVICE_BASELINE_MARKDOWN: &str = "benchmarks/react-native-device/latest.md";
const DEFAULT_REACT_DEVICE_RESULT_ROOT: &str = "/tmp/react-native-device-perf";
const DEFAULT_UIKIT_SCHEME: &str = "OxideUIKitPerf";
const DEFAULT_UIKIT_TEST_TARGET: &str = "OxideHostPerfTests";
const DEFAULT_UIKIT_TEST_CLASS: &str = "OxideHostPerfTests";
const DEFAULT_UIKIT_UI_TEST_TARGET: &str = "OxideHostUITests";
const DEFAULT_UIKIT_UI_LAUNCH_TEST_CLASS: &str = "OxideUIKitLaunchPerfTests";
const DEFAULT_REACT_DEVICE_SCHEME: &str = "ReactNativeCameraBenchPerf";
const DEFAULT_REACT_DEVICE_TEST_TARGET: &str = "ReactNativeCameraBenchPerfTests";
const DEFAULT_REACT_DEVICE_TEST_CLASS: &str = "ReactNativeCameraBenchPerfTests";
const DEFAULT_REACT_DEVICE_TEST_NAME: &str = "testReactNativeVisionCameraLivePreview";
const DEFAULT_REACT_DEVICE_WORKSPACE_RELATIVE_PATH: &str =
    "host/react-native-camera-bench/ios/ReactNativeCameraBench.xcworkspace";
const REACT_NATIVE_CAMERA_CASE_ID: &str =
    "react_native.cross_platform.image_pipeline.camera_preview.vision_camera_live";
const PREFERRED_UIKIT_DEVICE_NAMES: &[&str] =
    &["iPhone 16", "iPhone 16 Pro", "iPhone 17", "iPhone 17 Pro"];
const UIKIT_SIM_GATED_METRICS: &[&str] = &["clock_s", "cpu_time_s", "cpu_cycles_kc"];
const UIKIT_DEVICE_GATED_METRICS: &[&str] =
    &["clock_s", "cpu_time_s", "cpu_cycles_kc", "memory_peak_kb", "gpu_time_s", "gpu_latency_s"];
const UIKIT_SIM_THRESHOLD_PCT: f64 = 0.20;
const UIKIT_DEVICE_THRESHOLD_PCT: f64 = 0.20;
const UIKIT_SIM_TINY_TIME_MAX_S: f64 = 0.002;
const UIKIT_SIM_TINY_TIME_NOISE_S: f64 = 0.00035;
const UIKIT_SIM_SMALL_TIME_MAX_S: f64 = 0.015;
const UIKIT_SIM_SMALL_TIME_NOISE_S: f64 = 0.0025;
const UIKIT_SIM_TINY_CPU_CYCLES_MAX_KC: f64 = 5_000.0;
const UIKIT_SIM_TINY_CPU_CYCLES_NOISE_KC: f64 = 1_000.0;
const UIKIT_SIM_SMALL_CPU_CYCLES_MAX_KC: f64 = 25_000.0;
const UIKIT_SIM_SMALL_CPU_CYCLES_NOISE_KC: f64 = 5_000.0;
const UIKIT_DEVICE_METRICS_BATCH_MAX_CASES: usize = 20;
const DEFAULT_UIKIT_DEVICE_TRACE_SECONDS: u64 = 5;
const UIKIT_PERF_SIGNPOST_SUBSYSTEM: &str = "com.oxide.perf";
const UIKIT_PERF_SIGNPOST_CATEGORY: &str = "PointsOfInterest";
const UIKIT_PERF_SIGNPOST_NAME: &str = "PerfWorkload";
const XCTRACE_EXPORT_RETRIES: usize = 4;
const XCTRACE_ATTACH_RETRIES: usize = 8;
const XCTRACE_ATTACH_RETRY_DELAY_MS: u64 = 250;
const XCTRACE_ATTACH_READY_DELAY_MS: u64 = 3000;
const XCTRACE_STARTED_TIMEOUT_MS: u64 = 5000;
const XCTRACE_EXPORT_RETRY_DELAY_MS: u64 = 250;
const XCTRACE_TRACE_SETTLE_TIMEOUT_MS: u64 = 4000;
const XCTRACE_TRACE_SETTLE_POLL_MS: u64 = 200;
const XCTRACE_STARTUP_DELAY_MS: u64 = 750;
const UIKIT_DEVICE_READY_NOTIFICATION: &str = "com.oxide.perf.ready";
const UIKIT_DEVICE_START_NOTIFICATION: &str = "com.oxide.perf.start";
const UIKIT_DEVICE_COMPLETE_NOTIFICATION: &str = "com.oxide.perf.complete";
const UIKIT_TRACE_STARTED_NOTIFICATION: &str = "com.oxide.perf.xctrace.started";
const OXIDE_DEVICE_REPORT_BEGIN_LINE: &str = "OXIDE_PERF_REPORT_BEGIN";
const OXIDE_DEVICE_REPORT_CHUNK_PREFIX: &str = "OXIDE_PERF_REPORT_CHUNK ";
const OXIDE_DEVICE_REPORT_END_LINE: &str = "OXIDE_PERF_REPORT_END";
const OXIDE_STAGE_SUMMARY_PREFIX: &str = "OXIDE_STAGE_SUMMARY ";
const OXIDE_MEMORY_SUMMARY_PREFIX: &str = "OXIDE_MEMORY_SUMMARY ";
const OXIDE_CAMERA_CONTRACT_SUMMARY_PREFIX: &str = "OXIDE_CAMERA_CONTRACT_SUMMARY ";
const UIKIT_DEVICE_NOTIFICATION_STARTUP_DELAY_MS: u64 = 250;
const UIKIT_DEVICE_READY_TIMEOUT_SECS: u64 = 30;
const UIKIT_DEVICE_COMPLETE_TIMEOUT_SECS: u64 = 30;
const OXIDE_DEVICE_READY_TIMEOUT_SECS: u64 = 30;
const OXIDE_DEVICE_COMPLETE_TIMEOUT_SECS: u64 = 900;
const UIKIT_DEVICE_READY_GRACE_MS: u64 = 2000;
const UIKIT_PERF_REFRESH_MODE_ENV: &str = "OXIDE_PERF_REFRESH_MODE";
const UIKIT_PERF_MEASURE_ITERATIONS_ENV: &str = "OXIDE_PERF_MEASURE_ITERATIONS";
const UIKIT_PERF_BENCHMARK_ITERATIONS_ENV: &str = "OXIDE_PERF_BENCHMARK_ITERATIONS";
const UIKIT_PERF_CAMERA_TRACE_PHASES_ENV: &str = "OXIDE_PERF_CAMERA_TRACE_PHASES";
const UIKIT_PERF_CAMERA_MAX_DRAWABLE_COUNT_ENV: &str = "OXIDE_PERF_CAMERA_MAX_DRAWABLE_COUNT";
const UIKIT_PERF_CAMERA_PREVIEW_SURFACE_SCALE_ENV: &str = "OXIDE_PERF_CAMERA_PREVIEW_SURFACE_SCALE";
const UIKIT_PERF_CAMERA_CAPTURE_CONTRACT_MODE_ENV: &str = "OXIDE_PERF_CAMERA_CAPTURE_CONTRACT_MODE";
const UIKIT_PERF_CAMERA_STAGE_MEASUREMENT_ENV: &str = "OXIDE_PERF_CAMERA_STAGE_MEASUREMENT";
const UIKIT_PERF_CAMERA_TINY_PREVIEW_RENDERER_ENV: &str = "OXIDE_PERF_CAMERA_TINY_PREVIEW_RENDERER";
const UIKIT_PERF_CAMERA_PREVIEW_BACKPRESSURE_ENV: &str = "OXIDE_PERF_CAMERA_PREVIEW_BACKPRESSURE";
const UIKIT_PERF_CAMERA_REAL_APP_HOST_ENV: &str = "OXIDE_PERF_CAMERA_REAL_APP_HOST";
const UIKIT_PERF_CAMERA_REAL_APP_HYBRID_VISIBLE_PREVIEW_ENV: &str =
    "OXIDE_PERF_CAMERA_REAL_APP_HYBRID_VISIBLE_PREVIEW";
const UIKIT_RENDER_IN_TEST_ENV: &str = "OXIDE_RENDER_IN_TEST";

struct UIKitCaseSpec {
    test_name: &'static str,
    case_id: &'static str,
    oxide_case_id: &'static str,
    note: &'static str,
}

const UIKIT_CASE_SPECS: &[UIKitCaseSpec] = &[
    UIKitCaseSpec {
        test_name: "testLabelEncode",
        case_id: "uikit.component.label.encode",
        oxide_case_id: "cpu.component.label.encode",
        note: "UILabel multiline layout parity.",
    },
    UIKitCaseSpec {
        test_name: "testProgressBarEncode",
        case_id: "uikit.component.progress_bar.encode",
        oxide_case_id: "cpu.component.progress_bar.encode",
        note: "UIView/CALayer progress fill parity.",
    },
    UIKitCaseSpec {
        test_name: "testSpinnerEncode",
        case_id: "uikit.component.spinner.encode",
        oxide_case_id: "cpu.component.spinner.encode",
        note: "CAShapeLayer spinner parity.",
    },
    UIKitCaseSpec {
        test_name: "testButtonEncode",
        case_id: "uikit.component.button.encode",
        oxide_case_id: "cpu.component.button.encode",
        note: "UIButton filled configuration parity.",
    },
    UIKitCaseSpec {
        test_name: "testToggleEncode",
        case_id: "uikit.component.toggle.encode",
        oxide_case_id: "cpu.component.toggle.encode",
        note: "Track/thumb custom toggle parity.",
    },
    UIKitCaseSpec {
        test_name: "testSliderEncode",
        case_id: "uikit.component.slider.encode",
        oxide_case_id: "cpu.component.slider.encode",
        note: "UISlider encode/layout parity.",
    },
    UIKitCaseSpec {
        test_name: "testImageViewEncode",
        case_id: "uikit.component.image_view.encode",
        oxide_case_id: "cpu.component.image_view.encode",
        note: "UIImageView bitmap bind parity.",
    },
    UIKitCaseSpec {
        test_name: "testNineSliceImageEncode",
        case_id: "uikit.component.nine_slice_image.encode",
        oxide_case_id: "cpu.component.nine_slice_image.encode",
        note: "Resizable cap-inset image parity.",
    },
    UIKitCaseSpec {
        test_name: "testCameraNV12OptimizedPreview",
        case_id: "uikit.optimized.image_pipeline.camera_preview.nv12_optimized",
        oxide_case_id: "gpu.scene.camera.frame",
        note: "OxideHost synthetic NV12 camera preview using the optimized Metal YUV to RGB conversion path.",
    },
    UIKitCaseSpec {
        test_name: "testCameraNV12LegacyPreview",
        case_id: "uikit.optimized.image_pipeline.camera_preview.nv12_legacy",
        oxide_case_id: "gpu.scene.camera.frame",
        note: "OxideHost synthetic NV12 camera preview using the legacy Metal YUV to RGB conversion path.",
    },
    UIKitCaseSpec {
        test_name: "testCameraBGRAPreview",
        case_id: "uikit.optimized.image_pipeline.camera_preview.bgra_benchmark",
        oxide_case_id: "gpu.scene.camera.frame",
        note: "OxideHost synthetic BGRA camera preview benchmark reference path.",
    },
    UIKitCaseSpec {
        test_name: "testCameraBGRALivePreview",
        case_id: "uikit.optimized.image_pipeline.camera_preview.bgra_live",
        oxide_case_id: "gpu.scene.camera.frame",
        note: "OxideHost live BGRA camera preview using the canonical raw-frame-to-Metal path.",
    },
    UIKitCaseSpec {
        test_name: "testCameraNV12OptimizedLivePreview",
        case_id: "uikit.optimized.image_pipeline.camera_preview.nv12_optimized_live",
        oxide_case_id: "gpu.scene.camera.frame",
        note: "OxideHost live NV12 camera preview using the optimized Metal YUV to RGB conversion path.",
    },
    UIKitCaseSpec {
        test_name: "testCameraNV12LegacyLivePreview",
        case_id: "uikit.optimized.image_pipeline.camera_preview.nv12_legacy_live",
        oxide_case_id: "gpu.scene.camera.frame",
        note: "OxideHost live NV12 camera preview using the legacy Metal YUV to RGB conversion path.",
    },
    UIKitCaseSpec {
        test_name: "testCameraNV12LegacyHybridPreviewLayerLivePreview",
        case_id: "uikit.optimized.image_pipeline.camera_preview.nv12_legacy_hybrid_preview_layer_live",
        oxide_case_id: "gpu.scene.camera.frame",
        note: "OxideHost live NV12 camera backend with AVCaptureVideoPreviewLayer handling the visible preview from the same running Oxide camera session.",
    },
    UIKitCaseSpec {
        test_name: "testCameraNV12LegacyRealAppLivePreview",
        case_id: "uikit.optimized.image_pipeline.camera_preview.nv12_legacy_real_app_live",
        oxide_case_id: "gpu.scene.camera.frame",
        note: "OxideHost live NV12 camera preview through the actual app-host display-link and MetalView path.",
    },
    UIKitCaseSpec {
        test_name: "testCameraNV12LegacyRealAppHybridPreviewLayerLivePreview",
        case_id: "uikit.optimized.image_pipeline.camera_preview.nv12_legacy_real_app_hybrid_preview_layer_live",
        oxide_case_id: "gpu.scene.camera.frame",
        note: "OxideHost live NV12 camera backend on the actual app-host path with AVCaptureVideoPreviewLayer handling the visible preview from the same running Oxide camera session.",
    },
    UIKitCaseSpec {
        test_name: "testCameraAVFoundationPreviewLayerLivePreview",
        case_id: "uikit.idiomatic.image_pipeline.camera_preview.avfoundation_preview_layer_live",
        oxide_case_id: "gpu.scene.camera.frame",
        note: "Stock AVFoundation live camera preview using AVCaptureSession and AVCaptureVideoPreviewLayer.",
    },
    UIKitCaseSpec {
        test_name: "testCameraAVFoundationPreviewLayerSidecarLivePreview",
        case_id: "uikit.idiomatic.image_pipeline.camera_preview.avfoundation_preview_layer_sidecar_live",
        oxide_case_id: "gpu.scene.camera.frame",
        note: "Hybrid live camera preview using AVCaptureVideoPreviewLayer for visible preview plus AVCaptureVideoDataOutput sidecar delivery.",
    },
    UIKitCaseSpec {
        test_name: "testCollectionViewEncode",
        case_id: "uikit.component.collection_view.encode",
        oxide_case_id: "cpu.component.collection_view.encode",
        note: "UICollectionView cell layout parity.",
    },
    UIKitCaseSpec {
        test_name: "testSimpleHomeColdLaunch",
        case_id: "uikit.idiomatic.launch.simple_home.cold_launch",
        oxide_case_id: "cpu.launch.simple_home.cold_launch",
        note: "Swift/UIKit simple-home cold launch parity through XCTApplicationLaunchMetric.",
    },
    UIKitCaseSpec {
        test_name: "testHeavyHomeColdLaunch",
        case_id: "uikit.idiomatic.launch.heavy_home.cold_launch",
        oxide_case_id: "cpu.launch.heavy_home.cold_launch",
        note: "Swift/UIKit heavy-home cold launch parity through XCTApplicationLaunchMetric.",
    },
    UIKitCaseSpec {
        test_name: "testDetailDeepLinkLaunch",
        case_id: "uikit.idiomatic.launch.detail.deep_link_launch",
        oxide_case_id: "cpu.launch.detail.deep_link_launch",
        note: "Swift/UIKit detail-route launch parity through XCTApplicationLaunchMetric.",
    },
    UIKitCaseSpec {
        test_name: "testSimpleHomeWarmResume",
        case_id: "uikit.idiomatic.launch.simple_home.warm_resume",
        oxide_case_id: "cpu.launch.simple_home.warm_resume",
        note: "Swift/UIKit simple-home warm-resume parity.",
    },
    UIKitCaseSpec {
        test_name: "testHeavyHomeForegroundAfterBackground",
        case_id: "uikit.idiomatic.launch.heavy_home.foreground_after_background",
        oxide_case_id: "cpu.launch.heavy_home.foreground_after_background",
        note: "Swift/UIKit heavy-home foreground-after-background parity.",
    },
    UIKitCaseSpec {
        test_name: "testLayoutFlatGridRelayout",
        case_id: "uikit.idiomatic.layout.flat_grid.rotation_relayout",
        oxide_case_id: "cpu.layout.flat_grid.rotation_relayout",
        note: "Flat grid relayout parity under alternating portrait and landscape widths.",
    },
    UIKitCaseSpec {
        test_name: "testLayoutDeepStackThemeSwap",
        case_id: "uikit.idiomatic.layout.deep_stack.theme_swap",
        oxide_case_id: "cpu.layout.deep_stack.theme_swap",
        note: "Deep stack theme swap relayout parity.",
    },
    UIKitCaseSpec {
        test_name: "testLayoutGridSafeAreaSwap",
        case_id: "uikit.idiomatic.layout.grid.safe_area_swap",
        oxide_case_id: "cpu.layout.grid.safe_area_swap",
        note: "Grid relayout parity under safe-area inset swaps.",
    },
    UIKitCaseSpec {
        test_name: "testLargeEditorKeystrokeBurst",
        case_id: "uikit.idiomatic.text_input.large_editor.keystroke_burst",
        oxide_case_id: "cpu.text_input.large_editor.keystroke_burst",
        note: "Large-editor typing burst parity.",
    },
    UIKitCaseSpec {
        test_name: "testLargeEditorPaste10KB",
        case_id: "uikit.idiomatic.text_input.large_editor.paste_10kb",
        oxide_case_id: "cpu.text_input.large_editor.paste_10kb",
        note: "Large-editor 10 KB paste parity.",
    },
    UIKitCaseSpec {
        test_name: "testLargeEditorSelectionReplace",
        case_id: "uikit.idiomatic.text_input.large_editor.selection_replace",
        oxide_case_id: "cpu.text_input.large_editor.selection_replace",
        note: "Large-editor selection replace parity.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedLargeEditorKeystrokeBurst",
        case_id: "uikit.optimized.text_input.large_editor.keystroke_burst",
        oxide_case_id: "cpu.text_input.large_editor.keystroke_burst",
        note: "Hand-tuned single-view large-editor typing burst parity.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedLargeEditorPaste10KB",
        case_id: "uikit.optimized.text_input.large_editor.paste_10kb",
        oxide_case_id: "cpu.text_input.large_editor.paste_10kb",
        note: "Hand-tuned single-view large-editor 10 KB paste parity.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedLargeEditorSelectionReplace",
        case_id: "uikit.optimized.text_input.large_editor.selection_replace",
        oxide_case_id: "cpu.text_input.large_editor.selection_replace",
        note: "Hand-tuned single-view large-editor selection replace parity.",
    },
    UIKitCaseSpec {
        test_name: "testImagePNGDecode",
        case_id: "uikit.idiomatic.image_pipeline.png.decode",
        oxide_case_id: "cpu.image_pipeline.png.decode",
        note: "PNG decode phase parity over the shared checker payload.",
    },
    UIKitCaseSpec {
        test_name: "testImageTextureUpload",
        case_id: "uikit.idiomatic.image_pipeline.png.upload",
        oxide_case_id: "gpu.image_pipeline.png.upload",
        note: "PNG upload phase parity over the shared checker payload.",
    },
    UIKitCaseSpec {
        test_name: "testImageFirstVisible",
        case_id: "uikit.idiomatic.image_pipeline.png.first_visible",
        oxide_case_id: "gpu.image_pipeline.png.first_visible",
        note: "PNG first-visible phase parity over the shared checker payload.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedImagePNGDecode",
        case_id: "uikit.optimized.image_pipeline.png.decode",
        oxide_case_id: "gpu.image_pipeline.png.decode",
        note: "Hand-tuned PNG decode parity using ImageIO eager decode.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedImageTextureUpload",
        case_id: "uikit.optimized.image_pipeline.png.upload",
        oxide_case_id: "gpu.image_pipeline.png.upload",
        note: "Hand-tuned PNG upload parity over a single-view image grid.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedImageFirstVisible",
        case_id: "uikit.optimized.image_pipeline.png.first_visible",
        oxide_case_id: "gpu.image_pipeline.png.first_visible",
        note: "Hand-tuned PNG first-visible parity over a single-view image grid.",
    },
    UIKitCaseSpec {
        test_name: "testButtonPressResponse",
        case_id: "uikit.idiomatic.navigation.button_press.response",
        oxide_case_id: "cpu.navigation.button_press.response",
        note: "Direct button response parity from event to first visible control-state update.",
    },
    UIKitCaseSpec {
        test_name: "testSliderScrubResponse",
        case_id: "uikit.idiomatic.navigation.slider_scrub.response",
        oxide_case_id: "cpu.navigation.slider_scrub.response",
        note: "Direct slider scrub response parity from event to first visible thumb/fill update.",
    },
    UIKitCaseSpec {
        test_name: "testTextFocusResponse",
        case_id: "uikit.idiomatic.navigation.text_focus.response",
        oxide_case_id: "cpu.navigation.text_focus.response",
        note: "Direct text focus response parity from event to first visible responder-state update.",
    },
    UIKitCaseSpec {
        test_name: "testSingleNodeReconcile",
        case_id: "uikit.idiomatic.reconcile.single_node_mutation",
        oxide_case_id: "cpu.reconcile.single_node_mutation",
        note: "Single-node reconcile parity over a retained 1000-node flat-rect tree.",
    },
    UIKitCaseSpec {
        test_name: "testTreeMutation1Pct",
        case_id: "uikit.idiomatic.reconcile.tree_mutation_1pct",
        oxide_case_id: "cpu.reconcile.tree_mutation_1pct",
        note: "1 percent tree-mutation reconcile parity over a retained 1000-node flat-rect tree.",
    },
    UIKitCaseSpec {
        test_name: "testTreeMutation10Pct",
        case_id: "uikit.idiomatic.reconcile.tree_mutation_10pct",
        oxide_case_id: "cpu.reconcile.tree_mutation_10pct",
        note: "10 percent tree-mutation reconcile parity over a retained 1000-node flat-rect tree.",
    },
    UIKitCaseSpec {
        test_name: "testThemeSwapFull",
        case_id: "uikit.idiomatic.reconcile.theme_swap_full",
        oxide_case_id: "cpu.reconcile.theme_swap_full",
        note: "Full retained-tree theme-swap parity over a retained 1000-node flat-rect tree.",
    },
    UIKitCaseSpec {
        test_name: "testEmptyRootMount",
        case_id: "uikit.idiomatic.primitive.empty_root.mount",
        oxide_case_id: "cpu.primitive.empty_root.mount",
        note: "Empty-root mount parity for a blank retained host.",
    },
    UIKitCaseSpec {
        test_name: "testFlatRects10Mount",
        case_id: "uikit.idiomatic.primitive.flat_rects.10.mount",
        oxide_case_id: "cpu.primitive.flat_rects.10.mount",
        note: "Retained flat-rect grid mount parity at 10 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testFlatRects100Mount",
        case_id: "uikit.idiomatic.primitive.flat_rects.100.mount",
        oxide_case_id: "cpu.primitive.flat_rects.100.mount",
        note: "Retained flat-rect grid mount parity at 100 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testFlatRects1000Mount",
        case_id: "uikit.idiomatic.primitive.flat_rects.1000.mount",
        oxide_case_id: "cpu.primitive.flat_rects.1000.mount",
        note: "Retained flat-rect grid mount parity at 1000 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testFlatRects10Mutate",
        case_id: "uikit.idiomatic.primitive.flat_rects.10.mutate_fill",
        oxide_case_id: "cpu.primitive.flat_rects.10.mutate_fill",
        note: "Retained flat-rect grid shared-fill mutation parity at 10 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testFlatRects100Mutate",
        case_id: "uikit.idiomatic.primitive.flat_rects.100.mutate_fill",
        oxide_case_id: "cpu.primitive.flat_rects.100.mutate_fill",
        note: "Retained flat-rect grid shared-fill mutation parity at 100 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testFlatRects1000Mutate",
        case_id: "uikit.idiomatic.primitive.flat_rects.1000.mutate_fill",
        oxide_case_id: "cpu.primitive.flat_rects.1000.mutate_fill",
        note: "Retained flat-rect grid shared-fill mutation parity at 1000 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testFlatRects100RemoveAll",
        case_id: "uikit.idiomatic.primitive.flat_rects.100.remove_all",
        oxide_case_id: "cpu.primitive.flat_rects.100.remove_all",
        note: "Retained flat-rect grid teardown parity at 100 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testFlatRects100Remount",
        case_id: "uikit.idiomatic.primitive.flat_rects.100.remount",
        oxide_case_id: "cpu.primitive.flat_rects.100.remount",
        note: "Retained flat-rect grid remount parity at 100 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedFlatRects10Mount",
        case_id: "uikit.optimized.primitive.flat_rects.10.mount",
        oxide_case_id: "cpu.primitive.flat_rects.10.mount",
        note: "Hand-tuned single-view flat-rect grid mount parity at 10 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedFlatRects100Mount",
        case_id: "uikit.optimized.primitive.flat_rects.100.mount",
        oxide_case_id: "cpu.primitive.flat_rects.100.mount",
        note: "Hand-tuned single-view flat-rect grid mount parity at 100 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedFlatRects1000Mount",
        case_id: "uikit.optimized.primitive.flat_rects.1000.mount",
        oxide_case_id: "cpu.primitive.flat_rects.1000.mount",
        note: "Hand-tuned single-view flat-rect grid mount parity at 1000 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedFlatRects10Mutate",
        case_id: "uikit.optimized.primitive.flat_rects.10.mutate_fill",
        oxide_case_id: "cpu.primitive.flat_rects.10.mutate_fill",
        note: "Hand-tuned single-view flat-rect grid shared-fill mutation parity at 10 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedFlatRects100Mutate",
        case_id: "uikit.optimized.primitive.flat_rects.100.mutate_fill",
        oxide_case_id: "cpu.primitive.flat_rects.100.mutate_fill",
        note: "Hand-tuned single-view flat-rect grid shared-fill mutation parity at 100 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedFlatRects1000Mutate",
        case_id: "uikit.optimized.primitive.flat_rects.1000.mutate_fill",
        oxide_case_id: "cpu.primitive.flat_rects.1000.mutate_fill",
        note: "Hand-tuned single-view flat-rect grid shared-fill mutation parity at 1000 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testLabels10Mount",
        case_id: "uikit.idiomatic.primitive.labels.10.mount",
        oxide_case_id: "cpu.primitive.labels.10.mount",
        note: "Retained multiline label mount parity at 10 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedLabels10Mount",
        case_id: "uikit.optimized.primitive.labels.10.mount",
        oxide_case_id: "cpu.primitive.labels.10.mount",
        note: "Hand-tuned single-view multiline label mount parity at 10 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testLabels100Mount",
        case_id: "uikit.idiomatic.primitive.labels.100.mount",
        oxide_case_id: "cpu.primitive.labels.100.mount",
        note: "Retained multiline label mount parity at 100 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedLabels100Mount",
        case_id: "uikit.optimized.primitive.labels.100.mount",
        oxide_case_id: "cpu.primitive.labels.100.mount",
        note: "Hand-tuned single-view multiline label mount parity at 100 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testLabels1000Mount",
        case_id: "uikit.idiomatic.primitive.labels.1000.mount",
        oxide_case_id: "cpu.primitive.labels.1000.mount",
        note: "Retained multiline label mount parity at 1000 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedLabels1000Mount",
        case_id: "uikit.optimized.primitive.labels.1000.mount",
        oxide_case_id: "cpu.primitive.labels.1000.mount",
        note: "Hand-tuned single-view multiline label mount parity at 1000 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testLabels10Mutate",
        case_id: "uikit.idiomatic.primitive.labels.10.mutate_text",
        oxide_case_id: "cpu.primitive.labels.10.mutate_text",
        note: "Retained multiline label shared-text mutation parity at 10 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedLabels10Mutate",
        case_id: "uikit.optimized.primitive.labels.10.mutate_text",
        oxide_case_id: "cpu.primitive.labels.10.mutate_text",
        note: "Hand-tuned single-view multiline label shared-text mutation parity at 10 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testLabels100Mutate",
        case_id: "uikit.idiomatic.primitive.labels.100.mutate_text",
        oxide_case_id: "cpu.primitive.labels.100.mutate_text",
        note: "Retained multiline label shared-text mutation parity at 100 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedLabels100Mutate",
        case_id: "uikit.optimized.primitive.labels.100.mutate_text",
        oxide_case_id: "cpu.primitive.labels.100.mutate_text",
        note: "Hand-tuned single-view multiline label shared-text mutation parity at 100 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testLabels1000Mutate",
        case_id: "uikit.idiomatic.primitive.labels.1000.mutate_text",
        oxide_case_id: "cpu.primitive.labels.1000.mutate_text",
        note: "Retained multiline label shared-text mutation parity at 1000 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedLabels1000Mutate",
        case_id: "uikit.optimized.primitive.labels.1000.mutate_text",
        oxide_case_id: "cpu.primitive.labels.1000.mutate_text",
        note: "Hand-tuned single-view multiline label shared-text mutation parity at 1000 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testCards10Mount",
        case_id: "uikit.idiomatic.primitive.cards.10.mount",
        oxide_case_id: "cpu.primitive.cards.10.mount",
        note: "Rounded card mount parity at 10 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedCards10Mount",
        case_id: "uikit.optimized.primitive.cards.10.mount",
        oxide_case_id: "cpu.primitive.cards.10.mount",
        note: "Hand-tuned single-view rounded card mount parity at 10 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testCards100Mount",
        case_id: "uikit.idiomatic.primitive.cards.100.mount",
        oxide_case_id: "cpu.primitive.cards.100.mount",
        note: "Rounded card mount parity at 100 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedCards100Mount",
        case_id: "uikit.optimized.primitive.cards.100.mount",
        oxide_case_id: "cpu.primitive.cards.100.mount",
        note: "Hand-tuned single-view rounded card mount parity at 100 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testCards10Mutate",
        case_id: "uikit.idiomatic.primitive.cards.10.mutate_palette",
        oxide_case_id: "cpu.primitive.cards.10.mutate_palette",
        note: "Rounded card shared-palette mutation parity at 10 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedCards10Mutate",
        case_id: "uikit.optimized.primitive.cards.10.mutate_palette",
        oxide_case_id: "cpu.primitive.cards.10.mutate_palette",
        note: "Hand-tuned single-view rounded card shared-palette mutation parity at 10 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testCards100Mutate",
        case_id: "uikit.idiomatic.primitive.cards.100.mutate_palette",
        oxide_case_id: "cpu.primitive.cards.100.mutate_palette",
        note: "Rounded card shared-palette mutation parity at 100 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedCards100Mutate",
        case_id: "uikit.optimized.primitive.cards.100.mutate_palette",
        oxide_case_id: "cpu.primitive.cards.100.mutate_palette",
        note: "Hand-tuned single-view rounded card shared-palette mutation parity at 100 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testImages10Mount",
        case_id: "uikit.idiomatic.primitive.images.10.mount",
        oxide_case_id: "cpu.primitive.images.10.mount",
        note: "UIImageView bitmap mount parity at 10 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedImages10Mount",
        case_id: "uikit.optimized.primitive.images.10.mount",
        oxide_case_id: "cpu.primitive.images.10.mount",
        note: "Hand-tuned single-view bitmap mount parity at 10 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testImages100Mount",
        case_id: "uikit.idiomatic.primitive.images.100.mount",
        oxide_case_id: "cpu.primitive.images.100.mount",
        note: "UIImageView bitmap mount parity at 100 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedImages100Mount",
        case_id: "uikit.optimized.primitive.images.100.mount",
        oxide_case_id: "cpu.primitive.images.100.mount",
        note: "Hand-tuned single-view bitmap mount parity at 100 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testImages10Mutate",
        case_id: "uikit.idiomatic.primitive.images.10.mutate_alpha",
        oxide_case_id: "cpu.primitive.images.10.mutate_alpha",
        note: "UIImageView shared-alpha mutation parity at 10 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedImages10Mutate",
        case_id: "uikit.optimized.primitive.images.10.mutate_alpha",
        oxide_case_id: "cpu.primitive.images.10.mutate_alpha",
        note: "Hand-tuned single-view bitmap shared-alpha mutation parity at 10 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testImages100Mutate",
        case_id: "uikit.idiomatic.primitive.images.100.mutate_alpha",
        oxide_case_id: "cpu.primitive.images.100.mutate_alpha",
        note: "UIImageView shared-alpha mutation parity at 100 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedImages100Mutate",
        case_id: "uikit.optimized.primitive.images.100.mutate_alpha",
        oxide_case_id: "cpu.primitive.images.100.mutate_alpha",
        note: "Hand-tuned single-view bitmap shared-alpha mutation parity at 100 nodes.",
    },
    UIKitCaseSpec {
        test_name: "testControlSetMount",
        case_id: "uikit.idiomatic.primitive.control_set.mount",
        oxide_case_id: "cpu.primitive.control_set.mount",
        note: "Shared control-set mount parity.",
    },
    UIKitCaseSpec {
        test_name: "testControlSetMutate",
        case_id: "uikit.idiomatic.primitive.control_set.mutate_state",
        oxide_case_id: "cpu.primitive.control_set.mutate_state",
        note: "Shared control-set state mutation parity.",
    },
    UIKitCaseSpec {
        test_name: "testSpinnerSpin",
        case_id: "uikit.animation.spinner_spin",
        oxide_case_id: "cpu.animation.spinner_spin",
        note: "Spinner phase animation parity.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedSpinnerSpin",
        case_id: "uikit.optimized.animation.spinner_spin",
        oxide_case_id: "cpu.animation.spinner_spin",
        note: "Hand-tuned single-view spinner phase animation parity.",
    },
    UIKitCaseSpec {
        test_name: "testProgressIndeterminate",
        case_id: "uikit.animation.progress_indeterminate",
        oxide_case_id: "cpu.animation.progress_indeterminate",
        note: "Indeterminate progress sweep parity.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedProgressIndeterminate",
        case_id: "uikit.optimized.animation.progress_indeterminate",
        oxide_case_id: "cpu.animation.progress_indeterminate",
        note: "Hand-tuned single-view indeterminate progress sweep parity.",
    },
    UIKitCaseSpec {
        test_name: "testButtonPressScale",
        case_id: "uikit.animation.button_press_scale",
        oxide_case_id: "cpu.animation.button_press_scale",
        note: "Button press transform parity.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedButtonPressScale",
        case_id: "uikit.optimized.animation.button_press_scale",
        oxide_case_id: "cpu.animation.button_press_scale",
        note: "Hand-tuned single-view button press transform parity.",
    },
    UIKitCaseSpec {
        test_name: "testToggleThumbSpring",
        case_id: "uikit.animation.toggle_thumb_spring",
        oxide_case_id: "cpu.animation.toggle_thumb_spring",
        note: "Toggle thumb spring parity.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedToggleThumbSpring",
        case_id: "uikit.optimized.animation.toggle_thumb_spring",
        oxide_case_id: "cpu.animation.toggle_thumb_spring",
        note: "Hand-tuned single-view toggle thumb spring parity.",
    },
    UIKitCaseSpec {
        test_name: "testSliderThumbMove",
        case_id: "uikit.animation.slider_thumb_move",
        oxide_case_id: "cpu.animation.slider_thumb_move",
        note: "Slider thumb movement parity.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedSliderThumbMove",
        case_id: "uikit.optimized.animation.slider_thumb_move",
        oxide_case_id: "cpu.animation.slider_thumb_move",
        note: "Hand-tuned single-view slider thumb movement parity.",
    },
    UIKitCaseSpec {
        test_name: "testImageZoomPan",
        case_id: "uikit.animation.image_zoom_pan",
        oxide_case_id: "cpu.animation.image_zoom_pan",
        note: "Image zoom/pan parity.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedImageZoomPan",
        case_id: "uikit.optimized.animation.image_zoom_pan",
        oxide_case_id: "cpu.animation.image_zoom_pan",
        note: "Hand-tuned single-view image zoom and pan parity.",
    },
    UIKitCaseSpec {
        test_name: "testAnimTimelineBars",
        case_id: "uikit.animation.anim_timeline_bars",
        oxide_case_id: "cpu.animation.anim_timeline_bars",
        note: "Animated timeline bars parity.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedAnimTimelineBars",
        case_id: "uikit.optimized.animation.anim_timeline_bars",
        oxide_case_id: "cpu.animation.anim_timeline_bars",
        note: "Hand-tuned single-view animated timeline bars parity.",
    },
    UIKitCaseSpec {
        test_name: "testInputFormJourney",
        case_id: "uikit.journey.input_form_submit",
        oxide_case_id: "cpu.journey.input_form_submit",
        note: "Text entry, picker selection, and submit journey parity.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedInputFormJourney",
        case_id: "uikit.optimized.journey.input_form_submit",
        oxide_case_id: "cpu.journey.input_form_submit",
        note: "Hand-tuned single-view text entry, picker selection, and submit journey parity.",
    },
    UIKitCaseSpec {
        test_name: "testCollectionNavigationJourney",
        case_id: "uikit.journey.collection_navigation",
        oxide_case_id: "cpu.journey.collection_navigation",
        note: "Collection focus-navigation journey parity.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedCollectionNavigationJourney",
        case_id: "uikit.optimized.journey.collection_navigation",
        oxide_case_id: "cpu.journey.collection_navigation",
        note: "Hand-tuned custom-draw collection focus-navigation journey parity.",
    },
    UIKitCaseSpec {
        test_name: "testFeedScrollJourney",
        case_id: "uikit.journey.feed_scroll_matrix",
        oxide_case_id: "cpu.journey.feed_scroll_matrix",
        note: "Feed scroll matrix parity.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedFeedScrollJourney",
        case_id: "uikit.optimized.journey.feed_scroll_matrix",
        oxide_case_id: "cpu.journey.feed_scroll_matrix",
        note: "Hand-tuned custom-draw feed scroll matrix parity.",
    },
    UIKitCaseSpec {
        test_name: "testThumbnailGridScrollJourney",
        case_id: "uikit.journey.thumbnail_grid_scroll_matrix",
        oxide_case_id: "cpu.journey.thumbnail_grid_scroll_matrix",
        note: "Thumbnail grid scroll matrix parity.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedThumbnailGridScrollJourney",
        case_id: "uikit.optimized.journey.thumbnail_grid_scroll_matrix",
        oxide_case_id: "cpu.journey.thumbnail_grid_scroll_matrix",
        note: "Hand-tuned custom-draw thumbnail grid scroll matrix parity.",
    },
    UIKitCaseSpec {
        test_name: "testChatThreadScrollJourney",
        case_id: "uikit.journey.chat_thread_scroll_matrix",
        oxide_case_id: "cpu.journey.chat_thread_scroll_matrix",
        note: "Chat thread scroll matrix parity.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedChatThreadScrollJourney",
        case_id: "uikit.optimized.journey.chat_thread_scroll_matrix",
        oxide_case_id: "cpu.journey.chat_thread_scroll_matrix",
        note: "Hand-tuned custom-draw chat thread scroll matrix parity.",
    },
    UIKitCaseSpec {
        test_name: "testZoomImageGestureJourney",
        case_id: "uikit.journey.zoom_image_gesture_cycle",
        oxide_case_id: "cpu.journey.zoom_image_gesture_cycle",
        note: "Zoom image pinch/pan/reset journey parity.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedZoomImageGestureJourney",
        case_id: "uikit.optimized.journey.zoom_image_gesture_cycle",
        oxide_case_id: "cpu.journey.zoom_image_gesture_cycle",
        note: "Hand-tuned single-view zoom image pinch/pan/reset journey parity.",
    },
    UIKitCaseSpec {
        test_name: "testOrchestrationJourney",
        case_id: "uikit.journey.orchestration_transition_modal",
        oxide_case_id: "cpu.journey.orchestration_transition_modal",
        note: "Transition plus modal overlay journey parity.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedOrchestrationJourney",
        case_id: "uikit.optimized.journey.orchestration_transition_modal",
        oxide_case_id: "cpu.journey.orchestration_transition_modal",
        note: "Hand-tuned single-view transition plus modal overlay journey parity.",
    },
    UIKitCaseSpec {
        test_name: "testTextFieldsEditCycle",
        case_id: "uikit.idiomatic.authoring.text_fields.edit_cycle",
        oxide_case_id: "cpu.authoring.text_fields.edit_cycle",
        note: "Author-facing text-field editing lifecycle parity.",
    },
    UIKitCaseSpec {
        test_name: "testPopupWheelPickerInteraction",
        case_id: "uikit.idiomatic.authoring.popup_wheel_picker.interaction",
        oxide_case_id: "cpu.authoring.popup_wheel_picker.interaction",
        note: "Author-facing popup and wheel-picker interaction parity.",
    },
    UIKitCaseSpec {
        test_name: "testBurstEmitterSample",
        case_id: "uikit.idiomatic.authoring.burst_emitter.sample",
        oxide_case_id: "cpu.authoring.burst_emitter.sample",
        note: "Author-facing burst-emitter configuration and sampling parity.",
    },
    UIKitCaseSpec {
        test_name: "testSurfaceRouterCompose",
        case_id: "uikit.idiomatic.authoring.surface_router.compose",
        oxide_case_id: "cpu.authoring.surface_router.compose",
        note: "Author-facing surface composition and overlay wiring parity.",
    },
    UIKitCaseSpec {
        test_name: "testOpenCloseHeavyScreen100x",
        case_id: "uikit.idiomatic.endurance.open_close_heavy_screen.100x",
        oxide_case_id: "cpu.endurance.open_close_heavy_screen.100x",
        note: "Heavy-screen open/close endurance parity.",
    },
    UIKitCaseSpec {
        test_name: "testTabSwitchHeavy500x",
        case_id: "uikit.idiomatic.endurance.tab_switch_heavy.500x",
        oxide_case_id: "cpu.endurance.tab_switch_heavy.500x",
        note: "Heavy tab-switch endurance parity.",
    },
    UIKitCaseSpec {
        test_name: "testIdleAnimation600Frames",
        case_id: "uikit.idiomatic.endurance.idle_animation.600_frames",
        oxide_case_id: "cpu.endurance.idle_animation.600_frames",
        note: "Idle animation endurance parity.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedOpenCloseHeavyScreen100x",
        case_id: "uikit.optimized.endurance.open_close_heavy_screen.100x",
        oxide_case_id: "cpu.endurance.open_close_heavy_screen.100x",
        note: "Hand-tuned heavy-screen open/close endurance parity over a custom-draw feed-style surface.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedTabSwitchHeavy500x",
        case_id: "uikit.optimized.endurance.tab_switch_heavy.500x",
        oxide_case_id: "cpu.endurance.tab_switch_heavy.500x",
        note: "Hand-tuned heavy tab-switch endurance parity over custom-draw feed, grid, and orchestration surfaces.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedIdleAnimation600Frames",
        case_id: "uikit.optimized.endurance.idle_animation.600_frames",
        oxide_case_id: "cpu.endurance.idle_animation.600_frames",
        note: "Hand-tuned single-view idle animation endurance parity.",
    },
    UIKitCaseSpec {
        test_name: "testFlatRects10000Mount",
        case_id: "uikit.idiomatic.stress.flat_rects.10000.mount",
        oxide_case_id: "cpu.stress.flat_rects.10000.mount",
        note: "10k-node flat-rect mount stress parity.",
    },
    UIKitCaseSpec {
        test_name: "testStress300Animations",
        case_id: "uikit.idiomatic.stress.simultaneous_animations.300",
        oxide_case_id: "cpu.stress.simultaneous_animations.300",
        note: "300 simultaneous animation stress parity.",
    },
    UIKitCaseSpec {
        test_name: "testTicker100Hz",
        case_id: "uikit.idiomatic.stress.ticker_100hz",
        oxide_case_id: "cpu.stress.ticker_100hz",
        note: "100 Hz ticker stress parity.",
    },
    UIKitCaseSpec {
        test_name: "testPermissionCallbackBridge",
        case_id: "uikit.bridge.permission_callback_fanout",
        oxide_case_id: "cpu.bridge.permission_callback_fanout",
        note: "Permission wrapper update and callback fanout parity.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedPermissionCallbackBridge",
        case_id: "uikit.optimized.bridge.permission_callback_fanout",
        oxide_case_id: "cpu.bridge.permission_callback_fanout",
        note: "Hand-tuned permission callback fanout parity over a single-domain bridge path.",
    },
    UIKitCaseSpec {
        test_name: "testSensorLocationBridge",
        case_id: "uikit.bridge.sensor_location_snapshot",
        oxide_case_id: "cpu.bridge.sensor_location_snapshot",
        note: "Location sensor cache bridge parity.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedSensorLocationBridge",
        case_id: "uikit.optimized.bridge.sensor_location_snapshot",
        oxide_case_id: "cpu.bridge.sensor_location_snapshot",
        note: "Hand-tuned location sensor cache bridge parity over a fixed-size ring buffer.",
    },
    UIKitCaseSpec {
        test_name: "testBluetoothCacheBridge",
        case_id: "uikit.bridge.bluetooth_cache_update",
        oxide_case_id: "cpu.bridge.bluetooth_cache_update",
        note: "Bluetooth discovery cache bridge parity.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedBluetoothCacheBridge",
        case_id: "uikit.optimized.bridge.bluetooth_cache_update",
        oxide_case_id: "cpu.bridge.bluetooth_cache_update",
        note: "Hand-tuned Bluetooth discovery cache bridge parity over a compact bounded cache.",
    },
    UIKitCaseSpec {
        test_name: "testPhotoImportThumbnailBridge",
        case_id: "uikit.bridge.photo_import_thumbnail",
        oxide_case_id: "cpu.bridge.photo_import_thumbnail",
        note: "Photo import bytes-to-first-thumbnail bridge parity, excluding system picker UI.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedPhotoImportThumbnailBridge",
        case_id: "uikit.optimized.bridge.photo_import_thumbnail",
        oxide_case_id: "cpu.bridge.photo_import_thumbnail",
        note: "Hand-tuned photo import bytes-to-first-thumbnail bridge parity, excluding system picker UI.",
    },
    UIKitCaseSpec {
        test_name: "testFileImportRenderBridge",
        case_id: "uikit.bridge.file_import_render",
        oxide_case_id: "cpu.bridge.file_import_render",
        note: "File import bytes-to-first-render bridge parity, excluding system document picker UI.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedFileImportRenderBridge",
        case_id: "uikit.optimized.bridge.file_import_render",
        oxide_case_id: "cpu.bridge.file_import_render",
        note: "Hand-tuned file import bytes-to-first-render bridge parity, excluding system document picker UI.",
    },
    UIKitCaseSpec {
        test_name: "testSharePayloadPrepareBridge",
        case_id: "uikit.bridge.share_payload_prepare",
        oxide_case_id: "cpu.bridge.share_payload_prepare",
        note: "Share payload preparation bridge parity, excluding system share sheet UI.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedSharePayloadPrepareBridge",
        case_id: "uikit.optimized.bridge.share_payload_prepare",
        oxide_case_id: "cpu.bridge.share_payload_prepare",
        note: "Hand-tuned share payload preparation bridge parity, excluding system share sheet UI.",
    },
    UIKitCaseSpec {
        test_name: "testLocalJSONTransportRenderBridge",
        case_id: "uikit.bridge.local_json_transport_render",
        oxide_case_id: "cpu.bridge.local_json_transport_render",
        note: "Local JSON transport-decode-render bridge parity.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedLocalJSONTransportRenderBridge",
        case_id: "uikit.optimized.bridge.local_json_transport_render",
        oxide_case_id: "cpu.bridge.local_json_transport_render",
        note: "Hand-tuned local JSON transport-decode-render bridge parity.",
    },
    UIKitCaseSpec {
        test_name: "testLocalImageTransportRenderBridge",
        case_id: "uikit.bridge.local_image_transport_render",
        oxide_case_id: "cpu.bridge.local_image_transport_render",
        note: "Local image transport-decode-render bridge parity.",
    },
    UIKitCaseSpec {
        test_name: "testOptimizedLocalImageTransportRenderBridge",
        case_id: "uikit.optimized.bridge.local_image_transport_render",
        oxide_case_id: "cpu.bridge.local_image_transport_render",
        note: "Hand-tuned local image transport-decode-render bridge parity.",
    },
];

#[derive(Debug, Deserialize)]
struct CapabilitiesToml {
    #[serde(default)]
    usage_strings: BTreeMap<String, String>,
    #[serde(default)]
    entitlements: Entitlements,
}

#[derive(Debug, Default, Deserialize)]
pub struct Entitlements {
    #[serde(default)]
    pub push_notifications: bool,
    #[serde(default)]
    pub bluetooth_central: bool,
    #[serde(default)]
    pub bluetooth_peripheral: bool,
    #[serde(default)]
    pub background_fetch: bool,
    #[serde(default)]
    pub background_remote_notification: bool,
    #[serde(default)]
    pub background_processing: bool,
    #[serde(default)]
    pub location: LocationMode,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum LocationMode {
    #[default]
    None,
    WhenInUse,
    Always,
}

#[derive(Debug, Default)]
struct IosPerfCli {
    compare: Option<PathBuf>,
    json_out: Option<PathBuf>,
    markdown_out: Option<PathBuf>,
    result_bundle: Option<PathBuf>,
    destination: Option<String>,
    write_baseline: bool,
}

#[derive(Debug, Default)]
struct IosDevicePerfCli {
    cases: Vec<String>,
    compare: Option<PathBuf>,
    device: Option<String>,
    json_out: Option<PathBuf>,
    markdown_out: Option<PathBuf>,
    power_trace: Option<PathBuf>,
    power_trace_root: Option<PathBuf>,
    refresh_modes: Vec<UIKitDeviceRefreshMode>,
    reuse_derived_data: Option<PathBuf>,
    result_root: Option<PathBuf>,
    team: Option<String>,
    trace_seconds: Option<u64>,
    write_baseline: bool,
}

#[derive(Debug, Default)]
struct IosOxideDevicePerfCli {
    compare: Option<PathBuf>,
    device: Option<String>,
    json_out: Option<PathBuf>,
    markdown_out: Option<PathBuf>,
    result_root: Option<PathBuf>,
    team: Option<String>,
    smoke: bool,
    write_baseline: bool,
}

#[derive(Debug, Default)]
struct IosReactDevicePerfCli {
    compare: Option<PathBuf>,
    device: Option<String>,
    json_out: Option<PathBuf>,
    markdown_out: Option<PathBuf>,
    result_root: Option<PathBuf>,
    reuse_derived_data: Option<PathBuf>,
    team: Option<String>,
    trace_seconds: Option<u64>,
    write_baseline: bool,
}

#[derive(Debug, Default)]
struct IosTimeProfilerSummaryCli {
    json_out: Option<PathBuf>,
    trace: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum UIKitDeviceRefreshMode {
    DeviceDefault,
    Hz60Capped,
    Native,
}

impl UIKitDeviceRefreshMode {
    fn parse_cli(value: &str) -> Result<Vec<Self>> {
        match value.trim().to_ascii_lowercase().as_str() {
            "default" | "device-default" => Ok(vec![Self::DeviceDefault]),
            "60" | "60hz" | "60hz-capped" => Ok(vec![Self::Hz60Capped]),
            "native" => Ok(vec![Self::Native]),
            "both" => Ok(vec![Self::Hz60Capped, Self::Native]),
            other => {
                bail!("unknown --refresh-mode `{}`; expected default, 60hz, native, or both", other)
            }
        }
    }

    fn report_value(self) -> &'static str {
        match self {
            Self::DeviceDefault => "device-default",
            Self::Hz60Capped => "60hz-capped",
            Self::Native => "native",
        }
    }

    fn dir_suffix(self) -> &'static str {
        match self {
            Self::DeviceDefault => "default",
            Self::Hz60Capped => "60hz",
            Self::Native => "native",
        }
    }

    fn env_value(self) -> Option<&'static str> {
        match self {
            Self::DeviceDefault => None,
            Self::Hz60Capped => Some("60hz"),
            Self::Native => Some("native"),
        }
    }
}

fn normalize_uikit_refresh_modes(modes: &mut Vec<UIKitDeviceRefreshMode>) {
    if modes.is_empty() {
        modes.push(UIKitDeviceRefreshMode::DeviceDefault);
        return;
    }
    modes.sort_unstable();
    modes.dedup();
    if modes.len() > 1 {
        modes.retain(|mode| *mode != UIKitDeviceRefreshMode::DeviceDefault);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UIKitPerfReport {
    pub version: u32,
    pub suite: String,
    pub generated_label: Option<String>,
    pub device_name: String,
    pub energy_status: String,
    #[serde(default)]
    pub contract: UIKitContractCoverageReport,
    pub cases: Vec<UIKitPerfCase>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UIKitPerfCase {
    pub id: String,
    pub oxide_case_id: String,
    pub test_name: String,
    pub layer: String,
    pub scenario: String,
    pub style: String,
    pub cache_state: String,
    pub refresh_mode: String,
    pub threshold_pct: f64,
    pub metrics: BTreeMap<String, UIKitMetricSummary>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UIKitMetricSummary {
    pub unit: String,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub median: f64,
    pub p95: f64,
    pub p99: f64,
    pub samples: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct UIKitContractCoverageReport {
    pub layers: Vec<UIKitContractCoverageEntry>,
    pub styles: Vec<UIKitContractCoverageEntry>,
    pub battery: Vec<UIKitContractCoverageEntry>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct UIKitContractCoverageEntry {
    pub id: String,
    pub label: String,
    pub status: String,
    pub notes: Vec<String>,
}

impl Default for UIKitPerfCase {
    fn default() -> Self {
        Self {
            id: String::new(),
            oxide_case_id: String::new(),
            test_name: String::new(),
            layer: String::new(),
            scenario: String::new(),
            style: String::new(),
            cache_state: String::new(),
            refresh_mode: String::new(),
            threshold_pct: 0.0,
            metrics: BTreeMap::new(),
            notes: Vec::new(),
        }
    }
}

impl Default for UIKitMetricSummary {
    fn default() -> Self {
        Self {
            unit: String::new(),
            min: 0.0,
            max: 0.0,
            mean: 0.0,
            median: 0.0,
            p95: 0.0,
            p99: 0.0,
            samples: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UIKitPerfComparison {
    pub matched: usize,
    pub missing_baseline: Vec<String>,
    pub regressions: Vec<UIKitPerfRegression>,
    pub improvements: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UIKitPerfRegression {
    pub case_id: String,
    pub metric: String,
    pub baseline_median: f64,
    pub current_median: f64,
    pub allowed_median: f64,
    pub delta_pct: f64,
}

#[derive(Debug, Deserialize)]
struct XCTestMetricBundle {
    #[serde(rename = "testIdentifier")]
    test_identifier: String,
    #[serde(rename = "testRuns")]
    test_runs: Vec<XCTestMetricRun>,
}

#[derive(Debug, Deserialize)]
struct XCTestMetricRun {
    device: XCTestDevice,
    metrics: Vec<XCTestMetric>,
}

#[derive(Debug, Deserialize)]
struct XCTestDevice {
    #[serde(rename = "deviceName")]
    device_name: String,
}

#[derive(Debug, Deserialize)]
struct XCTestMetric {
    identifier: String,
    #[serde(rename = "unitOfMeasurement")]
    unit_of_measurement: String,
    measurements: Vec<f64>,
}

#[derive(Debug, Deserialize)]
struct SimCtlList {
    devices: BTreeMap<String, Vec<SimCtlDevice>>,
}

#[derive(Debug, Deserialize)]
struct SimCtlDevice {
    udid: String,
    name: String,
    state: String,
}

#[derive(Debug, Deserialize)]
struct CoreDeviceListResponse {
    result: CoreDeviceListResult,
}

#[derive(Debug, Deserialize)]
struct CoreDeviceListResult {
    devices: Vec<CoreDevice>,
}

#[derive(Debug, Deserialize)]
struct CoreDevice {
    identifier: String,
    #[serde(rename = "connectionProperties")]
    connection_properties: CoreDeviceConnectionProperties,
    #[serde(rename = "deviceProperties")]
    device_properties: CoreDeviceProperties,
    #[serde(rename = "hardwareProperties")]
    hardware_properties: CoreDeviceHardwareProperties,
}

#[derive(Debug, Deserialize)]
struct CoreDeviceConnectionProperties {
    #[serde(rename = "pairingState")]
    pairing_state: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CoreDeviceProperties {
    #[serde(rename = "ddiServicesAvailable")]
    ddi_services_available: Option<bool>,
    #[serde(rename = "developerModeStatus")]
    developer_mode_status: Option<String>,
    #[serde(rename = "osBuildUpdate")]
    os_build_update: Option<String>,
    #[serde(rename = "osVersionNumber")]
    os_version_number: Option<String>,
    name: String,
}

#[derive(Debug, Deserialize)]
struct CoreDeviceHardwareProperties {
    platform: String,
    #[serde(rename = "productType")]
    product_type: Option<String>,
    reality: String,
    #[serde(default)]
    udid: String,
}

#[derive(Debug, Deserialize)]
struct DeviceCtlDetailsResponse {
    result: CoreDevice,
}

#[derive(Debug, Deserialize)]
struct DeviceCtlProcessesResponse {
    result: DeviceCtlProcessesResult,
}

#[derive(Debug, Deserialize)]
struct DeviceCtlProcessesResult {
    #[serde(rename = "runningProcesses")]
    running_processes: Vec<DeviceCtlRunningProcess>,
}

#[derive(Debug, Deserialize)]
struct DeviceCtlRunningProcess {
    executable: String,
    #[serde(rename = "processIdentifier")]
    process_identifier: u64,
}

#[derive(Debug, Deserialize)]
struct DeviceCtlInfoResponse {
    info: DeviceCtlInfo,
}

#[derive(Debug, Deserialize)]
struct DeviceCtlInfo {
    outcome: String,
    details: Option<String>,
}

#[derive(Debug, Clone)]
struct UIKitPhysicalDevice {
    name: String,
    os_build: String,
    os_version: String,
    product_type: String,
    udid: String,
}

#[derive(Debug, Clone)]
pub struct TraceWindow {
    pub start_ns: u64,
    pub end_ns: u64,
    pub process_name: String,
}

#[derive(Debug, Clone)]
pub struct XctraceColumn {
    pub mnemonic: String,
    pub name: String,
    pub engineering_type: String,
}

#[derive(Debug, Clone, Default)]
pub struct XctraceCell {
    pub raw: Option<String>,
    pub fmt: Option<String>,
}

#[derive(Debug, Clone)]
pub struct XctraceRow {
    pub values: BTreeMap<String, XctraceCell>,
}

#[derive(Debug, Clone)]
pub struct XctraceTable {
    pub columns: Vec<XctraceColumn>,
    pub rows: Vec<XctraceRow>,
}

#[derive(Debug, Clone)]
pub struct XctraceTocTable {
    pub schema: String,
    pub xpath: String,
    pub category: String,
    pub subsystem: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeProfilerThreadSummary {
    pub thread: String,
    pub thread_name: Option<String>,
    pub samples: usize,
    pub share_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeProfilerFrameSummary {
    pub frame: String,
    pub samples: usize,
    pub share_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeProfilerBucketSummary {
    pub bucket: String,
    pub samples: usize,
    pub share_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeProfilerWorkerThreadNaming {
    pub tokio_named_threads_visible_in_thread_info: bool,
    pub tokio_named_threads_visible_in_sampled_rows: bool,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeProfilerSummary {
    pub trace: String,
    pub source: String,
    pub sample_rows_with_backtraces: usize,
    pub top_threads: Vec<TimeProfilerThreadSummary>,
    pub top_frames: Vec<TimeProfilerFrameSummary>,
    pub bucket_counts: Vec<TimeProfilerBucketSummary>,
    pub worker_thread_naming: TimeProfilerWorkerThreadNaming,
    pub notes: Vec<String>,
}

pub fn run() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    run_cli(&args)
}

pub fn run_cli(args: &[String]) -> Result<()> {
    let first = args.first().map(String::as_str);
    let second = args.get(1).map(String::as_str);
    match (first, second) {
        (Some("ios"), Some("prepare")) => ios_prepare(),
        (Some("ios"), Some("perf")) => ios_perf(&args[2..]),
        (Some("ios"), Some("device-perf")) => ios_device_perf(&args[2..]),
        (Some("ios"), Some("react-device-perf")) => ios_react_device_perf(&args[2..]),
        (Some("ios"), Some("oxide-device-perf")) => ios_oxide_device_perf(&args[2..]),
        (Some("ios"), Some("time-profiler-summary")) => ios_time_profiler_summary(&args[2..]),
        (Some("test-all"), _) => test_all(),
        _ => {
            eprintln!(
                "Usage:\n  cargo xtask ios prepare\n  cargo xtask ios perf [disabled: use `ios device-perf`]\n  cargo xtask ios device-perf [--write-baseline] [--compare PATH] [--json-out PATH] [--markdown-out PATH] [--result-root PATH] [--device NAME|UDID] [--team TEAM_ID] [--case TEST_NAME]... [--reuse-derived-data PATH] [--trace-seconds N] [--refresh-mode default|60hz|native|both] [--power-trace PATH | --power-trace-root DIR]\n    note: `--trace-seconds 0` skips the attached Metal trace and collects only xcodebuild CPU metrics plus parked console summaries.\n  cargo xtask ios react-device-perf [--write-baseline] [--compare PATH] [--json-out PATH] [--markdown-out PATH] [--result-root PATH] [--device NAME|UDID] [--team TEAM_ID] [--reuse-derived-data PATH] [--trace-seconds N]\n  cargo xtask ios oxide-device-perf [--write-baseline] [--compare PATH] [--json-out PATH] [--markdown-out PATH] [--result-root PATH] [--device NAME|UDID] [--team TEAM_ID] [--smoke]\n  cargo xtask ios time-profiler-summary --trace PATH [--json-out PATH]\n  cargo xtask test-all"
            );
            Ok(())
        }
    }
}

fn ios_prepare() -> Result<()> {
    let root = locate_workspace_root()?;
    let app_dir = root.join("host/ios-app/App");
    let caps_toml = app_dir.join("capabilities.toml");
    let info_plist = app_dir.join("Info.plist");
    let entitlements_plist = app_dir.join("App.entitlements");

    let caps: CapabilitiesToml = {
        let text = fs::read_to_string(&caps_toml)
            .with_context(|| format!("reading {}", caps_toml.display()))?;
        toml::from_str(&text).with_context(|| "parsing capabilities.toml")?
    };

    validate_usage(&caps)?;

    // Generate entitlements
    let ent = build_entitlements_dict(&caps.entitlements);
    let ent_plist = PlValue::Dictionary(ent);
    plist::to_file_xml(&entitlements_plist, &ent_plist)
        .with_context(|| "writing App.entitlements")?;

    // Merge Info.plist
    let mut info = read_plist_dict(&info_plist).unwrap_or_default();
    merge_usage_strings(&mut info, &caps.usage_strings);
    merge_background_modes(&mut info, &caps.entitlements);
    plist::to_file_xml(&info_plist, &PlValue::Dictionary(info))
        .with_context(|| "writing Info.plist")?;

    // Build and bundle shaders (default.metallib)
    build_and_bundle_shaders(&root, &app_dir)?;

    println!("Prepared entitlements, Info.plist, and bundled shaders.");
    Ok(())
}

fn test_all() -> Result<()> {
    let root = locate_workspace_root()?;

    if clippy_available(&root) {
        run_command(
            &root,
            "cargo",
            &["clippy", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings"],
            false,
        )?;
    } else {
        println!("cargo clippy not installed; skipping lint step (install with `rustup component add clippy`).");
    }
    run_command(
        &root,
        "cargo",
        &["test", "--workspace", "--all-targets", "--all-features", "--quiet"],
        false,
    )?;
    run_command(
        &root,
        "cargo",
        &["test", "--workspace", "--no-default-features", "--quiet"],
        false,
    )?;
    run_command(&root, "cargo", &["hack", "check", "--each-feature", "--no-dev-deps"], true)?;
    run_command(&root, "cargo", &["run", "-p", "oxide-perf-runner", "--", "--smoke"], false)?;
    run_command(&root, "cargo", &["run", "-p", "oxide-snapshot-runner", "--", "--smoke"], false)?;
    run_xcui_smoke(&root)?;

    Ok(())
}

fn run_command(root: &Path, program: &str, args: &[&str], allow_fail: bool) -> Result<()> {
    println!("> {} {}", program, args.join(" "));
    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(root);
    let status = match cmd.status() {
        Ok(status) => status,
        Err(e) => {
            if allow_fail && e.kind() == ErrorKind::NotFound {
                println!("{} not found (skipping)", program);
                return Ok(());
            }
            return Err(e).with_context(|| format!("running {} {}", program, args.join(" ")));
        }
    };
    if status.success() {
        return Ok(());
    }
    if allow_fail {
        println!("{} {} failed (non-fatal)", program, args.join(" "));
        return Ok(());
    }
    bail!("{} {} failed with status {}", program, args.join(" "), status.code().unwrap_or(-1))
}

fn clippy_available(root: &Path) -> bool {
    Command::new("cargo")
        .arg("clippy")
        .arg("--version")
        .current_dir(root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn run_xcui_smoke(root: &Path) -> Result<()> {
    if matches!(std::env::var("OXIDE_SKIP_XCUI").as_deref(), Ok("1") | Ok("true") | Ok("yes")) {
        println!("OXIDE_SKIP_XCUI set; skipping XCUI smoke");
        return Ok(());
    }

    let direct = root.join("scripts/run_xcui_smoke.sh");
    let fallback = root.parent().map(|p| p.join("scripts/run_xcui_smoke.sh"));
    let script = if direct.exists() {
        Some(direct)
    } else {
        fallback.filter(|candidate| candidate.exists())
    };

    let Some(script) = script else {
        println!("scripts/run_xcui_smoke.sh not found; skipping XCUI smoke");
        return Ok(());
    };

    println!("> {}", script.display());
    let status = Command::new(&script)
        .current_dir(root)
        .status()
        .with_context(|| format!("running {}", script.display()))?;
    if status.success() {
        return Ok(());
    }

    let code = status.code().map(|value| value.to_string()).unwrap_or_else(|| "signal".to_owned());
    bail!("{} failed with status {}", script.display(), code)
}

fn ios_perf(args: &[String]) -> Result<()> {
    let _ = parse_ios_perf_cli(args)?;
    bail!(
        "`cargo xtask ios perf` is disabled by repo policy. Official UIKit perf baselines and comparisons are physical-device-only; use `cargo xtask ios device-perf --refresh-mode both ...`."
    )
}

fn ios_device_perf(args: &[String]) -> Result<()> {
    let cli = parse_ios_device_perf_cli(args)?;
    let root = locate_workspace_root()?;
    let spec = root.join("host/ios-app/App/project.yml");
    let project = root.join("host/ios-app/App/OxideHost.xcodeproj");
    let result_root =
        cli.result_root.clone().unwrap_or_else(|| PathBuf::from(DEFAULT_UIKIT_DEVICE_RESULT_ROOT));
    let selected_specs = selected_uikit_case_specs(&cli.cases)?;
    let device = resolve_uikit_physical_device(&root, cli.device.as_deref())?;
    let destination = format!("platform=iOS,id={}", device.udid);
    let trace_seconds = cli.trace_seconds.unwrap_or(DEFAULT_UIKIT_DEVICE_TRACE_SECONDS);
    let trace_enabled = uikit_device_trace_enabled(trace_seconds);
    let refresh_modes = cli.refresh_modes.clone();
    let has_same_device_refresh_matrix = refresh_modes
        .contains(&UIKitDeviceRefreshMode::Hz60Capped)
        && refresh_modes.contains(&UIKitDeviceRefreshMode::Native);
    let derived_data_path =
        cli.reuse_derived_data.clone().unwrap_or_else(|| result_root.join("derived-data"));

    if cli.reuse_derived_data.as_ref().map(|path| path.starts_with(&result_root)).unwrap_or(false) {
        fs::create_dir_all(&result_root)
            .with_context(|| format!("creating {}", result_root.display()))?;
    } else {
        remove_existing_path(&result_root)?;
        fs::create_dir_all(&result_root)
            .with_context(|| format!("creating {}", result_root.display()))?;
    }

    validate_uikit_power_trace_inputs(&cli, &selected_specs)?;
    if !trace_enabled && (cli.power_trace.is_some() || cli.power_trace_root.is_some()) {
        bail!("--trace-seconds 0 cannot be combined with --power-trace or --power-trace-root");
    }
    ensure_uikit_device_ready(&root, &device)?;
    ensure_uikit_device_support_available(&root, &device)?;
    let development_team =
        resolve_uikit_development_team(&root, cli.team.as_deref(), Some(device.udid.as_str()))?;
    run_command_owned(
        &root,
        "xcodegen",
        &[String::from("generate"), String::from("--spec"), spec.to_string_lossy().into_owned()],
        false,
    )?;
    if cli.reuse_derived_data.is_none() {
        run_uikit_device_build_for_testing(
            &root,
            &project,
            &destination,
            &development_team,
            &derived_data_path,
        )?;
    } else if !derived_data_path.exists() {
        bail!(
            "requested --reuse-derived-data path does not exist: {}",
            derived_data_path.display()
        );
    }
    let built_app = resolve_built_uikit_app(&derived_data_path)?;
    let xctestrun_path = resolve_built_xctestrun_path(&derived_data_path, DEFAULT_UIKIT_SCHEME)?;
    install_uikit_device_app(&root, &device, &built_app)?;

    let include_energy =
        trace_enabled && (cli.power_trace.is_some() || cli.power_trace_root.is_some());
    let mut metrics_json_by_refresh = BTreeMap::new();
    for refresh_mode in &refresh_modes {
        let metrics_json = run_uikit_device_metrics_batch(
            &root,
            &xctestrun_path,
            &destination,
            &selected_specs,
            *refresh_mode,
            &result_root,
        )?;
        metrics_json_by_refresh.insert(*refresh_mode, metrics_json);
    }
    let mut report_cases = Vec::new();
    for spec in selected_specs {
        for refresh_mode in &refresh_modes {
            let case_dir =
                result_root.join(format!("{}-{}", spec.test_name, refresh_mode.dir_suffix()));
            fs::create_dir_all(&case_dir)
                .with_context(|| format!("creating {}", case_dir.display()))?;
            let metrics_json = metrics_json_by_refresh.get(refresh_mode).with_context(|| {
                format!("missing batched metrics for `{}`", refresh_mode.report_value())
            })?;
            let gpu_run = if trace_enabled {
                run_uikit_device_case_trace(
                    &root,
                    &device,
                    &built_app,
                    spec,
                    *refresh_mode,
                    &case_dir,
                    trace_seconds,
                )?
            } else {
                run_uikit_device_case_console_capture(
                    &root,
                    &device,
                    &built_app,
                    spec,
                    *refresh_mode,
                    &case_dir,
                )?
            };
            let power_run = include_energy
                .then(|| load_uikit_device_case_power_trace(&root, &cli, spec, &case_dir));
            let power_run = match power_run {
                Some(run) => Some(run?),
                None => None,
            };
            report_cases.push(build_uikit_device_case(
                &root,
                &result_root,
                spec,
                &built_app.executable_name,
                *refresh_mode,
                metrics_json,
                &gpu_run,
                power_run.as_ref(),
            )?);
        }
    }

    let contract = build_uikit_contract_coverage(&report_cases, "device");

    let report = UIKitPerfReport {
        version: 1,
        suite: String::from("device"),
        generated_label: std::env::var("PERF_REPORT_DATE").ok(),
        device_name: device.name.clone(),
        energy_status: if !trace_enabled {
            String::from(
                "Direct device GPU time and energy were intentionally skipped for this run because `--trace-seconds 0` disabled the attached Metal trace; CPU metrics still come from xcodebuild test-without-building and camera summaries still come from the parked app console output.",
            )
        } else if include_energy {
            String::from(
                "Direct device GPU time comes from process-scoped Metal System Trace on real iPhone hardware. Direct energy is included only when manually imported per-case Power Profiler traces (.trace or raw exported .atrc) are supplied for the same OxideHost workload.",
            )
        } else {
            String::from(
                "Direct device GPU time comes from process-scoped Metal System Trace on real iPhone hardware. Direct energy is intentionally skipped in this run and remains manual-pending until per-case Power Profiler traces are imported.",
            )
        },
        contract,
        cases: report_cases,
        notes: if !trace_enabled {
            vec![
                String::from("Scheme: OxideUIKitPerf"),
                String::from(
                    "Device flow: build/install the host app once, collect CPU metrics through batched xcodebuild test-without-building runs per refresh mode, then prelaunch the benchmark app on the phone and drive the parked benchmark over Darwin notifications without attaching a Metal trace.",
                ),
                String::from(
                    "GPU trace: skipped for this run because `--trace-seconds 0` disabled the attached Metal trace. Camera contract, stage, and memory summaries still come from the parked app console log.",
                ),
                String::from(
                    "Energy trace: skipped because attached tracing was disabled for this run.",
                ),
                format!(
                    "Refresh modes: {}",
                    refresh_modes
                        .iter()
                        .map(|mode| mode.report_value())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                if has_same_device_refresh_matrix {
                    String::from(
                        "Refresh-matrix status: this report includes 60 Hz-capped and native rows on the same ProMotion iPhone, but the separate older 60 Hz hardware leg is still pending.",
                    )
                } else {
                    String::from(
                        "Refresh-matrix status: this run covers only the listed refresh mode(s); the broader 60 Hz versus native device matrix remains partial.",
                    )
                },
            ]
        } else if include_energy {
            vec![
                String::from("Scheme: OxideUIKitPerf"),
                String::from(
                    "Device flow: build/install the host app once, collect CPU metrics through batched xcodebuild test-without-building runs per refresh mode, then prelaunch the benchmark app on the phone and attach process-scoped Instruments traces by PID from the Mac for each case.",
                ),
                String::from(
                    "GPU trace: process-scoped Metal System Trace + Points of Interest, with Metal GPU Counters enabled when the device supports that counter profile.",
                ),
                String::from(
                    "Energy trace: manual per-case Power Profiler import from an exported .trace or raw .atrc captured for the same OxideHost workload.",
                ),
                format!(
                    "Refresh modes: {}",
                    refresh_modes
                        .iter()
                        .map(|mode| mode.report_value())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                if has_same_device_refresh_matrix {
                    String::from(
                        "Refresh-matrix status: this report includes 60 Hz-capped and native rows on the same ProMotion iPhone, but the separate older 60 Hz hardware leg is still pending.",
                    )
                } else {
                    String::from(
                        "Refresh-matrix status: this run covers only the listed refresh mode(s); the broader 60 Hz versus native device matrix remains partial.",
                    )
                },
            ]
        } else {
            vec![
                String::from("Scheme: OxideUIKitPerf"),
                String::from(
                    "Device flow: build/install the host app once, collect CPU metrics through batched xcodebuild test-without-building runs per refresh mode, then prelaunch the benchmark app on the phone and attach process-scoped Instruments traces by PID from the Mac for each case.",
                ),
                String::from(
                    "GPU trace: process-scoped Metal System Trace + Points of Interest, with Metal GPU Counters enabled when the device supports that counter profile.",
                ),
                String::from(
                    "Energy trace: manual per-case Power Profiler import from an exported .trace or raw .atrc captured for the same OxideHost workload.",
                ),
                format!(
                    "Refresh modes: {}",
                    refresh_modes
                        .iter()
                        .map(|mode| mode.report_value())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                if has_same_device_refresh_matrix {
                    String::from(
                        "Refresh-matrix status: this report includes 60 Hz-capped and native rows on the same ProMotion iPhone, but the separate older 60 Hz hardware leg is still pending.",
                    )
                } else {
                    String::from(
                        "Refresh-matrix status: this run covers only the listed refresh mode(s); the broader 60 Hz versus native device matrix remains partial.",
                    )
                },
            ]
        },
    };
    let comparison = if let Some(path) = cli.compare.as_ref() {
        let baseline = load_uikit_report(path)?;
        Some(compare_uikit_reports(&report, &baseline))
    } else {
        None
    };

    let json_out = if cli.write_baseline {
        Some(cli.json_out.unwrap_or_else(|| PathBuf::from(DEFAULT_UIKIT_DEVICE_BASELINE_JSON)))
    } else {
        cli.json_out
    };
    let markdown_out = if cli.write_baseline {
        Some(
            cli.markdown_out
                .unwrap_or_else(|| PathBuf::from(DEFAULT_UIKIT_DEVICE_BASELINE_MARKDOWN)),
        )
    } else {
        cli.markdown_out
    };

    if let Some(path) = json_out.as_ref() {
        write_uikit_report_json(path, &report)?;
    }
    if let Some(path) = markdown_out.as_ref() {
        write_uikit_markdown(path, &report, comparison.as_ref())?;
        write_uikit_dated_markdown(path, &report, comparison.as_ref())?;
    }

    print_uikit_summary(&report, comparison.as_ref());

    if let Some(comp) = comparison.as_ref() {
        if !comp.missing_baseline.is_empty() || !comp.regressions.is_empty() {
            bail!(
                "UIKit device performance comparison failed; inspect the generated report and update the committed baseline only with review"
            );
        }
    }

    Ok(())
}

fn ios_react_device_perf(args: &[String]) -> Result<()> {
    let cli = parse_ios_react_device_perf_cli(args)?;
    let root = locate_workspace_root()?;
    let workspace = root.join(DEFAULT_REACT_DEVICE_WORKSPACE_RELATIVE_PATH);
    let result_root =
        cli.result_root.clone().unwrap_or_else(|| PathBuf::from(DEFAULT_REACT_DEVICE_RESULT_ROOT));
    let derived_data_path =
        cli.reuse_derived_data.clone().unwrap_or_else(|| result_root.join("derived-data"));
    let trace_seconds = cli.trace_seconds.unwrap_or(DEFAULT_UIKIT_DEVICE_TRACE_SECONDS);
    let device = resolve_uikit_physical_device(&root, cli.device.as_deref())?;
    let destination = format!("platform=iOS,id={}", device.udid);

    if cli.reuse_derived_data.as_ref().map(|path| path.starts_with(&result_root)).unwrap_or(false) {
        fs::create_dir_all(&result_root)
            .with_context(|| format!("creating {}", result_root.display()))?;
    } else {
        remove_existing_path(&result_root)?;
        fs::create_dir_all(&result_root)
            .with_context(|| format!("creating {}", result_root.display()))?;
    }

    ensure_uikit_device_ready(&root, &device)?;
    ensure_uikit_device_support_available(&root, &device)?;
    let development_team =
        resolve_uikit_development_team(&root, cli.team.as_deref(), Some(device.udid.as_str()))?;

    if cli.reuse_derived_data.is_none() {
        run_react_device_build_for_testing(
            &root,
            &workspace,
            &development_team,
            &derived_data_path,
        )?;
    } else if !derived_data_path.exists() {
        bail!(
            "requested --reuse-derived-data path does not exist: {}",
            derived_data_path.display()
        );
    }

    let built_app = resolve_built_uikit_app(&derived_data_path)?;
    let xctestrun_path =
        resolve_built_xctestrun_path(&derived_data_path, DEFAULT_REACT_DEVICE_SCHEME)?;
    let react_run = run_react_device_perf_case(
        &root,
        &device,
        &built_app,
        &xctestrun_path,
        &destination,
        &result_root,
        trace_seconds,
    )?;
    let extracted_metrics = extract_xcresult_metrics_json(&root, &react_run.result_bundle)
        .with_context(|| {
            format!("extracting device metrics json from {}", react_run.result_bundle.display())
        });
    let metrics_json = match (react_run.xcodebuild_status.success(), extracted_metrics) {
        (true, Ok(metrics_json)) => metrics_json,
        (false, Ok(metrics_json)) => {
            eprintln!(
                "xcodebuild exited with an error after producing usable metrics for the React Native device benchmark; continuing with the extracted xcresult metrics: status={}",
                react_run.xcodebuild_status.code().unwrap_or(-1)
            );
            metrics_json
        }
        (_, Err(err)) => return Err(err),
    };

    let stdout = fs::read_to_string(&react_run.stdout_path)
        .with_context(|| format!("reading {}", react_run.stdout_path.display()))?;
    let report = parse_react_native_device_report_json(
        &metrics_json,
        &stdout,
        device.name.as_str(),
        built_app.executable_name.as_str(),
    )?;
    let mut report = report;
    if let Some(case) = report.cases.first_mut() {
        let (gpu_windows, used_summary_window) = extract_trace_windows_or_summary_window(
            &root,
            &react_run.trace_run.trace_path,
            built_app.executable_name.as_str(),
        )?;
        case.notes.extend(react_run.trace_run.notes.iter().cloned());
        if used_summary_window {
            case.notes.push(String::from(
                "GPU trace window status: this Metal trace did not expose the per-workload signposts, so GPU metrics were summarized over the full trace duration for the ReactNativeCameraBench process.",
            ));
        }
        case.notes.push(format!("GPU trace windows: {}", gpu_windows.len()));
        for (name, metric) in
            summarize_trace_signpost_metrics(&root, &react_run.trace_run.trace_path, &gpu_windows)?
        {
            case.metrics.insert(name, metric.median);
        }
        for (name, metric) in summarize_device_gpu_metrics(
            &root,
            &react_run.trace_run.trace_path,
            &gpu_windows,
            &mut case.notes,
        )? {
            case.metrics.insert(name, metric.median);
        }
    }
    report.contract.notes.push(format!(
        "GPU trace: all-processes Metal System Trace + Points of Interest, filtered back to the `{}` process with shared PerfWorkload windows.",
        built_app.executable_name
    ));
    let comparison = if let Some(path) = cli.compare.as_ref() {
        let baseline = load_oxide_device_report(path)?;
        Some(compare_reports(&report, &baseline))
    } else {
        None
    };

    let json_out = if cli.write_baseline {
        Some(cli.json_out.unwrap_or_else(|| PathBuf::from(DEFAULT_REACT_DEVICE_BASELINE_JSON)))
    } else {
        cli.json_out
    };
    let markdown_out = if cli.write_baseline {
        Some(
            cli.markdown_out
                .unwrap_or_else(|| PathBuf::from(DEFAULT_REACT_DEVICE_BASELINE_MARKDOWN)),
        )
    } else {
        cli.markdown_out
    };

    if let Some(path) = json_out.as_ref() {
        write_react_device_report_json(path, &report)?;
    }
    if let Some(path) = markdown_out.as_ref() {
        write_react_device_report_markdown(path, &report, comparison.as_ref())?;
        write_react_device_dated_markdown(path, &report, comparison.as_ref())?;
    }

    print_react_device_summary(&report, comparison.as_ref());

    if let Some(comp) = comparison.as_ref() {
        if !comp.missing_baseline.is_empty() || !comp.regressions.is_empty() {
            bail!(
                "React Native device performance comparison failed; inspect the generated report and update the committed baseline only with review"
            );
        }
    }

    Ok(())
}

fn ios_oxide_device_perf(args: &[String]) -> Result<()> {
    let cli = parse_ios_oxide_device_perf_cli(args)?;
    let root = locate_workspace_root()?;
    let spec = root.join("host/ios-app/App/project.yml");
    let project = root.join("host/ios-app/App/OxideHost.xcodeproj");
    let result_root =
        cli.result_root.clone().unwrap_or_else(|| PathBuf::from(DEFAULT_OXIDE_DEVICE_RESULT_ROOT));
    let device = resolve_uikit_physical_device(&root, cli.device.as_deref())?;
    let destination = format!("platform=iOS,id={}", device.udid);

    remove_existing_path(&result_root)?;
    fs::create_dir_all(&result_root)
        .with_context(|| format!("creating {}", result_root.display()))?;

    ensure_uikit_device_ready(&root, &device)?;
    let development_team =
        resolve_uikit_development_team(&root, cli.team.as_deref(), Some(device.udid.as_str()))?;
    run_command_owned(
        &root,
        "xcodegen",
        &[String::from("generate"), String::from("--spec"), spec.to_string_lossy().into_owned()],
        false,
    )?;
    let derived_data_path = result_root.join("derived-data");
    run_uikit_device_build_for_testing(
        &root,
        &project,
        &destination,
        &development_team,
        &derived_data_path,
    )?;
    let built_app = resolve_built_uikit_app(&derived_data_path)?;
    install_uikit_device_app(&root, &device, &built_app)?;

    let report =
        run_oxide_device_report_capture(&root, &device, &built_app, &result_root, cli.smoke)?;
    let comparison = if let Some(path) = cli.compare.as_ref() {
        let baseline = load_oxide_device_report(path)?;
        Some(compare_reports(&report, &baseline))
    } else {
        None
    };

    let json_out = if cli.write_baseline {
        Some(cli.json_out.unwrap_or_else(|| PathBuf::from(DEFAULT_OXIDE_DEVICE_BASELINE_JSON)))
    } else {
        cli.json_out
    };
    let markdown_out = if cli.write_baseline {
        Some(
            cli.markdown_out
                .unwrap_or_else(|| PathBuf::from(DEFAULT_OXIDE_DEVICE_BASELINE_MARKDOWN)),
        )
    } else {
        cli.markdown_out
    };

    if let Some(path) = json_out.as_ref() {
        write_oxide_device_report_json(path, &report)?;
    }
    if let Some(path) = markdown_out.as_ref() {
        write_oxide_device_report_markdown(path, &report, comparison.as_ref())?;
        write_oxide_device_dated_markdown(path, &report, comparison.as_ref())?;
    }

    print_oxide_device_summary(&report, comparison.as_ref());

    if let Some(comp) = comparison.as_ref() {
        if !comp.missing_baseline.is_empty() || !comp.regressions.is_empty() {
            bail!(
                "Oxide device performance comparison failed; inspect the generated report and update the committed baseline only with review"
            );
        }
    }

    Ok(())
}

fn ios_time_profiler_summary(args: &[String]) -> Result<()> {
    let cli = parse_ios_time_profiler_summary_cli(args)?;
    let root = locate_workspace_root()?;
    let trace_path = cli.trace.with_context(|| "--trace PATH is required")?;
    let summary = summarize_time_profiler_trace(&root, &trace_path)?;
    let json_out = cli.json_out.unwrap_or_else(|| {
        trace_path
            .parent()
            .map(|dir| dir.join("time-profiler-summary.json"))
            .unwrap_or_else(|| PathBuf::from("time-profiler-summary.json"))
    });
    let json = serde_json::to_string_pretty(&summary).with_context(|| {
        format!("serializing time profiler summary for {}", trace_path.display())
    })?;
    fs::write(&json_out, json).with_context(|| format!("writing {}", json_out.display()))?;
    println!("wrote {}", json_out.display());
    Ok(())
}

pub fn uikit_device_trace_enabled(trace_seconds: u64) -> bool {
    trace_seconds > 0
}

#[derive(Debug, Clone)]
struct DeviceTraceRun {
    trace_path: PathBuf,
    launch_stdout_path: PathBuf,
    notes: Vec<String>,
}

#[derive(Debug)]
struct ReactDevicePerfRun {
    result_bundle: PathBuf,
    stdout_path: PathBuf,
    trace_run: DeviceTraceRun,
    xcodebuild_status: std::process::ExitStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltUIKitApp {
    pub app_path: PathBuf,
    pub info_plist_path: PathBuf,
    pub bundle_identifier: String,
    pub executable_name: String,
}

fn build_uikit_device_case(
    root: &Path,
    result_root: &Path,
    spec: &UIKitCaseSpec,
    host_process_name: &str,
    refresh_mode: UIKitDeviceRefreshMode,
    metrics_json: &str,
    metal_run: &DeviceTraceRun,
    power_run: Option<&DeviceTraceRun>,
) -> Result<UIKitPerfCase> {
    let base_report = parse_uikit_report_json(metrics_json)?;
    let base_case = base_report
        .cases
        .into_iter()
        .find(|case| case.id == spec.case_id)
        .with_context(|| format!("missing base UIKit case `{}`", spec.case_id))?;
    let mut notes =
        vec![String::from(spec.note), format!("Refresh mode: {}", refresh_mode.report_value())];
    notes.extend(metal_run.notes.iter().cloned());

    let mut metrics = base_case.metrics;
    let (camera_summary_stdout_path, contract_source_note, stage_source_note, memory_source_note) =
        if uikit_case_uses_real_app_camera_host(spec) {
            (
                uikit_device_metrics_case_stdout_path(
                    result_root,
                    refresh_mode.dir_suffix(),
                    spec.test_name,
                ),
                "Capture contract source: actual app-host XCTest stdout emitted during xcodebuild test-without-building.",
                "Stage timing source: actual app-host XCTest stdout emitted during xcodebuild test-without-building.",
                "Memory breakdown source: actual app-host XCTest stdout emitted during xcodebuild test-without-building.",
            )
        } else {
            (
                metal_run.launch_stdout_path.clone(),
                "Capture contract source: app-owned parked benchmark summary emitted through the device console launch log.",
                "Stage timing source: app-owned parked benchmark summary emitted through the device console launch log.",
                "Memory breakdown source: app-owned parked benchmark summary emitted through the device console launch log.",
            )
        };
    if spec.test_name.contains("Camera") && camera_summary_stdout_path.is_file() {
        let stdout = fs::read_to_string(&camera_summary_stdout_path)
            .with_context(|| format!("reading {}", camera_summary_stdout_path.display()))?;
        match parse_oxide_camera_contract_summary(&stdout) {
            Ok(contract) => {
                validate_normalized_camera_contract(&contract, spec.test_name)?;
                notes.push(String::from(
                    "Capture contract validation: stable back-camera 1280x720@30 YUV-family negotiation confirmed before the report was accepted.",
                ));
                notes.push(String::from(contract_source_note));
                notes.push(render_oxide_camera_contract_note(&contract));
            }
            Err(err) => {
                notes.push(format!("Capture contract status: {}", err));
            }
        }
        match parse_oxide_stage_summary(&stdout) {
            Ok(stage_metrics) => {
                notes.push(String::from(stage_source_note));
                for (name, metric) in stage_metrics {
                    metrics.insert(name, metric);
                }
            }
            Err(err) => {
                notes.push(format!("Stage timing status: {}", err));
            }
        }
        match parse_oxide_memory_summary(&stdout) {
            Ok(memory_metrics) => {
                notes.push(String::from(memory_source_note));
                if let Some(note) = render_oxide_memory_breakdown_note(&memory_metrics) {
                    notes.push(note);
                }
                for (name, metric) in memory_metrics {
                    metrics.insert(name, metric);
                }
            }
            Err(err) => {
                notes.push(format!("Memory breakdown status: {}", err));
            }
        }
    }
    if uikit_device_trace_artifact_exists(&metal_run.trace_path) {
        let (gpu_windows, used_summary_window) = extract_trace_windows_or_summary_window(
            root,
            &metal_run.trace_path,
            host_process_name,
        )?;
        if used_summary_window {
            notes.push(String::from(
                "GPU trace window status: this Metal trace did not expose the per-workload signposts, so GPU metrics were summarized over the full trace duration for the OxideHost process.",
            ));
        }
        notes.push(format!("GPU trace windows: {}", gpu_windows.len()));
        for (name, metric) in
            summarize_trace_signpost_metrics(root, &metal_run.trace_path, &gpu_windows)?
        {
            metrics.insert(name, metric);
        }
        for (name, metric) in
            summarize_device_gpu_metrics(root, &metal_run.trace_path, &gpu_windows, &mut notes)?
        {
            if name.starts_with("gpu_counter.") {
                notes.push(format!("Direct counter: `{}`", name));
            }
            metrics.insert(name, metric);
        }
        if let Some(power_run) = power_run {
            let (power_windows, used_summary_window) = extract_trace_windows_or_summary_window(
                root,
                &power_run.trace_path,
                host_process_name,
            )?;
            notes.extend(power_run.notes.iter().cloned());
            if used_summary_window {
                notes.push(String::from(
                    "Energy trace window status: this power trace did not expose the per-workload signposts, so energy was integrated over the full trace duration for the OxideHost process.",
                ));
            }
            notes.push(format!("Power trace windows: {}", power_windows.len()));
            metrics.insert(
                String::from("energy_j"),
                summarize_device_energy_metric(root, &power_run.trace_path, &power_windows)?,
            );
        } else {
            notes.push(String::from(
                "Energy trace status: skipped for this run; import a per-case Power Profiler .trace or raw .atrc later to add direct device energy.",
            ));
        }
    }

    Ok(UIKitPerfCase {
        id: String::from(spec.case_id),
        oxide_case_id: String::from(spec.oxide_case_id),
        test_name: String::from(spec.test_name),
        layer: base_case.layer,
        scenario: base_case.scenario,
        style: base_case.style,
        cache_state: base_case.cache_state,
        refresh_mode: String::from(refresh_mode.report_value()),
        threshold_pct: UIKIT_DEVICE_THRESHOLD_PCT,
        metrics,
        notes,
    })
}

pub fn uikit_device_trace_artifact_exists(path: &Path) -> bool {
    path.is_file() || is_xctrace_trace_bundle(path)
}

fn selected_uikit_case_specs(requested: &[String]) -> Result<Vec<&'static UIKitCaseSpec>> {
    let mut selected = Vec::new();
    for spec in UIKIT_CASE_SPECS {
        if requested.is_empty()
            || requested.iter().any(|value| value == spec.test_name || value == spec.case_id)
        {
            selected.push(spec);
        }
    }
    if selected.is_empty() {
        bail!("unknown UIKit perf case(s) `{}`", requested.join(", "));
    }
    Ok(selected)
}

fn validate_uikit_power_trace_inputs(
    cli: &IosDevicePerfCli,
    selected_specs: &[&'static UIKitCaseSpec],
) -> Result<()> {
    if cli.power_trace.is_some() && cli.power_trace_root.is_some() {
        bail!("pass either --power-trace or --power-trace-root, not both");
    }
    if cli.power_trace.is_some() && selected_specs.len() != 1 {
        bail!("--power-trace requires exactly one selected UIKit device-perf case");
    }
    if cli.power_trace.is_some() || cli.power_trace_root.is_some() {
        for spec in selected_specs {
            let _ = resolve_uikit_power_trace_path(cli, spec)?;
        }
    }
    Ok(())
}

fn load_uikit_device_case_power_trace(
    root: &Path,
    cli: &IosDevicePerfCli,
    spec: &UIKitCaseSpec,
    case_dir: &Path,
) -> Result<DeviceTraceRun> {
    let source_path = resolve_uikit_power_trace_path(cli, spec)?;
    let mut notes = vec![format!("Energy trace source: {}", source_path.display())];
    let trace_path = materialize_uikit_power_trace(
        root,
        &source_path,
        &uikit_power_trace_import_path(case_dir),
        &mut notes,
    )?;
    notes.push(String::from(
        "Energy trace workflow: manual override from an imported .trace, with raw exported .atrc files auto-imported when provided.",
    ));
    Ok(DeviceTraceRun { trace_path, launch_stdout_path: PathBuf::new(), notes })
}

fn resolve_uikit_power_trace_path(cli: &IosDevicePerfCli, spec: &UIKitCaseSpec) -> Result<PathBuf> {
    if let Some(path) = cli.power_trace.as_ref() {
        if path.exists() {
            return Ok(path.clone());
        }
        bail!("power trace path does not exist: {}", path.display());
    }
    if let Some(root) = cli.power_trace_root.as_ref() {
        if let Some(path) = resolve_existing_uikit_power_trace(root, spec.test_name) {
            return Ok(path);
        }
        let candidates = uikit_power_trace_candidate_paths(root, spec.test_name);
        bail!(
            "missing imported power trace for `{}`; expected one of `{}`, `{}`, `{}`, `{}`, `{}`, or `{}`",
            spec.test_name,
            candidates[0].display(),
            candidates[1].display(),
            candidates[2].display(),
            candidates[3].display(),
            candidates[4].display(),
            candidates[5].display()
        );
    }
    bail!("no explicit power trace override was provided")
}

pub fn uikit_power_trace_candidate_paths(root: &Path, test_name: &str) -> Vec<PathBuf> {
    vec![
        root.join(test_name).join("power.trace"),
        root.join(test_name).join("power.atrc"),
        root.join(format!("{}.trace", test_name)),
        root.join(format!("{}.atrc", test_name)),
        root.join(format!("{}-power.trace", test_name)),
        root.join(format!("{}-power.atrc", test_name)),
    ]
}

pub fn resolve_existing_uikit_power_trace(root: &Path, test_name: &str) -> Option<PathBuf> {
    uikit_power_trace_candidate_paths(root, test_name)
        .into_iter()
        .find(|candidate| candidate.exists())
}

pub fn is_xctrace_trace_bundle(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("trace"))
        .unwrap_or(false)
}

fn uikit_power_trace_import_path(case_dir: &Path) -> PathBuf {
    case_dir.join("power.trace")
}

fn materialize_uikit_power_trace(
    root: &Path,
    source_path: &Path,
    imported_trace_path: &Path,
    notes: &mut Vec<String>,
) -> Result<PathBuf> {
    if is_xctrace_trace_bundle(source_path) {
        return Ok(source_path.to_path_buf());
    }

    remove_existing_path(imported_trace_path)?;
    run_command_owned(
        root,
        "xcrun",
        &[
            String::from("xctrace"),
            String::from("import"),
            String::from("--input"),
            source_path.to_string_lossy().into_owned(),
            String::from("--output"),
            imported_trace_path.to_string_lossy().into_owned(),
        ],
        false,
    )
    .with_context(|| format!("importing raw power trace `{}`", source_path.display()))?;
    notes.push(format!("Energy trace imported to: {}", imported_trace_path.display()));
    Ok(imported_trace_path.to_path_buf())
}

fn resolve_uikit_physical_device(
    root: &Path,
    requested: Option<&str>,
) -> Result<UIKitPhysicalDevice> {
    let json =
        run_devicectl_json(root, &[String::from("list"), String::from("devices")], "devices")?;
    let response: CoreDeviceListResponse =
        serde_json::from_str(&json).with_context(|| "parsing devicectl device list")?;
    let mut candidates = Vec::new();

    for device in response.result.devices {
        if device.hardware_properties.platform != "iOS"
            || device.hardware_properties.reality != "physical"
        {
            continue;
        }
        if device.connection_properties.pairing_state.as_deref() != Some("paired") {
            continue;
        }
        if device.device_properties.developer_mode_status.as_deref() != Some("enabled") {
            continue;
        }
        if device.hardware_properties.udid.is_empty() {
            continue;
        }
        if let Some(requested_device) = requested {
            let matches_requested = requested_device == device.identifier
                || requested_device == device.hardware_properties.udid
                || requested_device == device.device_properties.name;
            if !matches_requested {
                continue;
            }
        }
        let details = load_uikit_physical_device_details(root, &device.hardware_properties.udid)?;
        candidates.push(details);
    }

    candidates
        .into_iter()
        .next()
        .with_context(|| "no paired physical iOS device with developer mode enabled was found")
}

fn load_uikit_physical_device_details(root: &Path, udid: &str) -> Result<UIKitPhysicalDevice> {
    let json = run_devicectl_json(
        root,
        &[
            String::from("device"),
            String::from("info"),
            String::from("details"),
            String::from("--device"),
            udid.to_string(),
        ],
        "device-details",
    )?;
    let response: DeviceCtlDetailsResponse =
        serde_json::from_str(&json).with_context(|| "parsing devicectl device details")?;
    let device = response.result;
    Ok(UIKitPhysicalDevice {
        name: device.device_properties.name,
        os_build: device.device_properties.os_build_update.unwrap_or_default(),
        os_version: normalize_ios_version_for_device_support(
            device.device_properties.os_version_number.as_deref().unwrap_or_default(),
        ),
        product_type: device.hardware_properties.product_type.unwrap_or_default(),
        udid: device.hardware_properties.udid,
    })
}

fn ensure_uikit_device_ready(root: &Path, device: &UIKitPhysicalDevice) -> Result<()> {
    let list_json = run_devicectl_json(
        root,
        &[String::from("list"), String::from("devices")],
        "devices-ready",
    )?;
    let list: CoreDeviceListResponse =
        serde_json::from_str(&list_json).with_context(|| "parsing devicectl readiness list")?;
    let listed = list
        .result
        .devices
        .into_iter()
        .find(|candidate| candidate.hardware_properties.udid == device.udid)
        .with_context(|| format!("device `{}` disappeared before tracing", device.udid))?;
    if listed.device_properties.ddi_services_available == Some(true) {
        return Ok(());
    }

    let ddi_json = run_devicectl_json_allow_failure(
        root,
        &[
            String::from("device"),
            String::from("info"),
            String::from("ddiServices"),
            String::from("--device"),
            device.udid.clone(),
            String::from("--timeout"),
            String::from("20"),
        ],
        "ddi-services",
    )?;
    let info: DeviceCtlInfoResponse =
        serde_json::from_str(&ddi_json).with_context(|| "parsing devicectl ddiServices output")?;
    if info.info.outcome == "success" {
        return Ok(());
    }

    let details =
        info.info.details.unwrap_or_else(|| String::from("developer services unavailable"));
    bail!(
        "device `{}` is not ready for direct tracing: {}. Unlock the phone, trust this Mac, keep Developer Mode enabled, and wait for the developer disk image to mount before rerunning `cargo xtask ios device-perf`.",
        device.name,
        details
    )
}

fn ensure_uikit_device_support_available(root: &Path, device: &UIKitPhysicalDevice) -> Result<()> {
    if device.product_type.is_empty() || device.os_version.is_empty() {
        bail!(
            "device `{}` is missing product/version metadata required for DeviceSupport validation; rerun `cargo xtask ios device-perf` after reconnecting the phone.",
            device.name
        );
    }

    let candidate_dirs = list_uikit_device_support_dirs(root)?;
    if candidate_dirs.iter().any(|dir_name| {
        device_support_dir_matches(dir_name, &device.product_type, &device.os_version)
    }) {
        return Ok(());
    }

    let version_family = device.os_version.split('.').take(2).collect::<Vec<_>>().join(".");
    let mut nearby = candidate_dirs
        .into_iter()
        .filter(|dir_name| {
            dir_name.starts_with(device.product_type.as_str())
                || (!version_family.is_empty() && dir_name.contains(version_family.as_str()))
        })
        .collect::<Vec<_>>();
    nearby.sort();
    nearby.truncate(6);

    let found = if nearby.is_empty() { String::from("none") } else { nearby.join(", ") };

    bail!(
        "device `{}` is paired and DDI-ready, but this Mac is missing Xcode DeviceSupport for `{} {}`{}; direct Instruments GPU/energy traces will hang in symbolication until that support is installed. Local DeviceSupport candidates: {}.",
        device.name,
        device.product_type,
        device.os_version,
        if device.os_build.is_empty() {
            String::new()
        } else {
            format!(" ({})", device.os_build)
        },
        found
    )
}

fn list_uikit_process_ids(
    root: &Path,
    device: &UIKitPhysicalDevice,
    process_name: &str,
) -> Result<BTreeSet<u64>> {
    let json = run_devicectl_json(
        root,
        &[
            String::from("device"),
            String::from("info"),
            String::from("processes"),
            String::from("--device"),
            device.udid.clone(),
        ],
        "device-processes",
    )?;
    find_device_process_ids(&json, process_name)
}

fn wait_for_uikit_process_clear(
    root: &Path,
    device: &UIKitPhysicalDevice,
    process_name: &str,
    timeout: Duration,
) -> Result<()> {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        let pids = list_uikit_process_ids(root, device, process_name)?;
        if pids.is_empty() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(150));
    }

    let pids = list_uikit_process_ids(root, device, process_name)?
        .into_iter()
        .map(|pid| pid.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    bail!(
        "timed out waiting for process `{}` to exit on device `{}`; still running: {}",
        process_name,
        device.name,
        pids
    )
}

fn drain_uikit_processes(
    root: &Path,
    device: &UIKitPhysicalDevice,
    process_name: &str,
    timeout: Duration,
    context: &str,
) -> Result<()> {
    let pids = list_uikit_process_ids(root, device, process_name)?;
    if pids.is_empty() {
        return Ok(());
    }
    let listed = pids.into_iter().map(|pid| pid.to_string()).collect::<Vec<_>>().join(", ");
    eprintln!(
        "[xtask] terminating lingering `{}` processes before {}: {}",
        process_name, context, listed
    );
    terminate_all_uikit_processes_named(root, device, process_name)?;
    wait_for_uikit_process_clear(root, device, process_name, timeout)
}

fn wait_for_uikit_process_start(
    root: &Path,
    device: &UIKitPhysicalDevice,
    process_name: &str,
    timeout: Duration,
) -> Result<u64> {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        let pids = list_uikit_process_ids(root, device, process_name)?;
        match pids.len() {
            0 => {}
            1 => return Ok(*pids.iter().next().unwrap_or(&0)),
            _ => {
                let listed =
                    pids.into_iter().map(|pid| pid.to_string()).collect::<Vec<_>>().join(", ");
                bail!(
                    "expected one `{}` process on device `{}`, but found {}: {}",
                    process_name,
                    device.name,
                    listed.split(", ").count(),
                    listed
                );
            }
        }
        thread::sleep(Duration::from_millis(150));
    }

    bail!("timed out waiting for process `{}` to start on device `{}`", process_name, device.name)
}

fn list_uikit_device_support_dirs(root: &Path) -> Result<Vec<String>> {
    let mut roots = BTreeSet::new();
    if let Some(path) = std::env::var_os("DEVELOPER_DIR").filter(|path| !path.is_empty()) {
        roots.insert(PathBuf::from(path).join("Platforms/iPhoneOS.platform/DeviceSupport"));
    } else if let Ok(path) = run_command_capture_owned(root, "xcode-select", &[String::from("-p")])
    {
        roots.insert(PathBuf::from(path.trim()).join("Platforms/iPhoneOS.platform/DeviceSupport"));
    }
    if let Some(home) = std::env::var_os("HOME").filter(|path| !path.is_empty()) {
        roots.insert(PathBuf::from(home).join("Library/Developer/Xcode/iOS DeviceSupport"));
    }
    roots.insert(PathBuf::from(
        "/Applications/Xcode.app/Contents/Developer/Platforms/iPhoneOS.platform/DeviceSupport",
    ));

    let mut names = BTreeSet::new();
    for root in roots {
        let entries = match fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(err) if err.kind() == ErrorKind::NotFound => continue,
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("reading DeviceSupport directory {}", root.display()))
            }
        };
        for entry in entries {
            let entry = entry.with_context(|| format!("reading {}", root.display()))?;
            let file_type = entry
                .file_type()
                .with_context(|| format!("reading file type for {}", entry.path().display()))?;
            if !file_type.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().trim().to_string();
            if !name.is_empty() {
                names.insert(name);
            }
        }
    }

    Ok(names.into_iter().collect())
}

pub fn device_support_dir_matches(dir_name: &str, product_type: &str, os_version: &str) -> bool {
    if dir_name.is_empty() || os_version.is_empty() {
        return false;
    }
    dir_name.starts_with(format!("{} {}", product_type, os_version).as_str())
        || dir_name.starts_with(os_version)
}

pub fn find_device_process_ids(json: &str, process_name: &str) -> Result<BTreeSet<u64>> {
    let response: DeviceCtlProcessesResponse =
        serde_json::from_str(json).with_context(|| "parsing devicectl process list")?;
    Ok(response
        .result
        .running_processes
        .into_iter()
        .filter_map(|process| {
            (device_process_name(&process.executable) == process_name)
                .then_some(process.process_identifier)
        })
        .collect())
}

pub fn device_process_name(executable: &str) -> String {
    executable
        .trim()
        .trim_start_matches("file://")
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string()
}

pub fn is_expected_devicectl_console_termination(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    lowered.contains("waiting for the application to terminate")
        && lowered.contains("app terminated due to signal 9")
}

fn is_expected_devicectl_console_exit_with_app_output(stdout: &str, stderr: &str) -> bool {
    let stdout = stdout.trim();
    let stderr = stderr.trim();
    if !stderr.is_empty() || stdout.is_empty() {
        return false;
    }
    let lowered = stdout.to_ascii_lowercase();
    !lowered.contains("error:") && !lowered.contains("failed with status")
}

fn is_expected_devicectl_process_termination_failure(stdout: &str, stderr: &str) -> bool {
    let combined = format!("{}\n{}", stdout, stderr).to_ascii_lowercase();
    combined.contains("no such process") || combined.contains("nsposixerrordomain error 3")
}

pub fn devicectl_notification_observed(stdout: &str, notification_name: &str) -> bool {
    stdout.lines().any(|line| line.contains("Observed") && line.contains(notification_name))
}

pub fn console_output_contains_marker(stdout: &str, marker: &str) -> bool {
    stdout.lines().any(|line| line.trim() == marker)
}

pub fn notification_or_console_marker_observed(
    notification_stdout: &str,
    notification_name: &str,
    console_stdout: &str,
    console_marker: &str,
) -> bool {
    devicectl_notification_observed(notification_stdout, notification_name)
        || console_output_contains_marker(console_stdout, console_marker)
}

pub fn is_unsupported_gpu_counter_profile_error(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    lowered.contains("selected counter profile is not supported on target device")
        || (lowered.contains("metal gpu counters") && lowered.contains("failed with status 21"))
}

fn is_missing_xctrace_attach_process_error(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    lowered.contains("cannot find process matching name")
        || lowered.contains("cannot find process for provided pid")
}

fn xctrace_attach_ready_delay_ms() -> u64 {
    XCTRACE_ATTACH_READY_DELAY_MS
}

pub fn format_uikit_only_testing_identifier(
    test_target: &str,
    test_class: &str,
    test_name: &str,
) -> String {
    format!("{}/{}/{}", test_target, test_class, test_name)
}

fn uikit_launch_case_metadata(
    spec: &UIKitCaseSpec,
) -> Option<(&'static str, Option<&'static str>)> {
    match spec.test_name {
        "testSimpleHomeColdLaunch" | "testSimpleHomeWarmResume" => Some(("simple_home", None)),
        "testHeavyHomeColdLaunch" | "testHeavyHomeForegroundAfterBackground" => {
            Some(("heavy_home", None))
        }
        "testDetailDeepLinkLaunch" => {
            Some(("detail_route", Some("oxide://detail/integration?item=42")))
        }
        _ => None,
    }
}

fn uikit_only_testing_identifier_for_spec(spec: &UIKitCaseSpec) -> String {
    if uikit_launch_case_metadata(spec).is_some() {
        return format_uikit_only_testing_identifier(
            DEFAULT_UIKIT_UI_TEST_TARGET,
            DEFAULT_UIKIT_UI_LAUNCH_TEST_CLASS,
            spec.test_name,
        );
    }
    format_uikit_only_testing_identifier(
        DEFAULT_UIKIT_TEST_TARGET,
        DEFAULT_UIKIT_TEST_CLASS,
        spec.test_name,
    )
}

pub fn uikit_only_testing_identifier_for_test_name(test_name: &str) -> Result<String> {
    let requested = vec![String::from(test_name)];
    let spec = selected_uikit_case_specs(&requested)?
        .into_iter()
        .next()
        .with_context(|| format!("missing UIKit case `{}`", test_name))?;
    Ok(uikit_only_testing_identifier_for_spec(spec))
}

pub fn uikit_perf_environment_json_for_test_name(
    test_name: &str,
    refresh_mode: &str,
) -> Result<String> {
    let requested = vec![String::from(test_name)];
    let spec = selected_uikit_case_specs(&requested)?
        .into_iter()
        .next()
        .with_context(|| format!("missing UIKit case `{}`", test_name))?;
    let mode = UIKitDeviceRefreshMode::parse_cli(refresh_mode)?
        .into_iter()
        .next()
        .with_context(|| format!("missing refresh mode from `{}`", refresh_mode))?;
    uikit_perf_launch_environment_json(spec, mode)
}

pub fn normalize_ios_version_for_device_support(value: &str) -> String {
    value.trim().chars().take_while(|ch| ch.is_ascii_digit() || *ch == '.').collect()
}

fn host_parallel_job_count() -> String {
    std::thread::available_parallelism().map(|count| count.get()).unwrap_or(1).to_string()
}

fn uikit_case_uses_real_app_camera_host(spec: &UIKitCaseSpec) -> bool {
    matches!(
        spec.test_name,
        "testCameraNV12LegacyRealAppLivePreview"
            | "testCameraNV12LegacyRealAppHybridPreviewLayerLivePreview"
    )
}

fn uikit_case_uses_real_app_hybrid_visible_preview(spec: &UIKitCaseSpec) -> bool {
    spec.test_name == "testCameraNV12LegacyRealAppHybridPreviewLayerLivePreview"
}

pub fn uikit_device_metrics_case_stdout_path(
    result_root: &Path,
    refresh_dir_suffix: &str,
    test_name: &str,
) -> PathBuf {
    result_root.join(format!("metrics-{}-{}.stdout.log", refresh_dir_suffix, test_name))
}

fn uikit_device_metrics_case_stderr_path(
    result_root: &Path,
    refresh_dir_suffix: &str,
    test_name: &str,
) -> PathBuf {
    result_root.join(format!("metrics-{}-{}.stderr.log", refresh_dir_suffix, test_name))
}

fn append_uikit_case_specific_perf_environment(
    env: &mut BTreeMap<String, String>,
    spec: &UIKitCaseSpec,
) {
    if uikit_case_uses_real_app_camera_host(spec) {
        env.insert(String::from(UIKIT_RENDER_IN_TEST_ENV), String::from("1"));
        env.insert(String::from(UIKIT_PERF_CAMERA_REAL_APP_HOST_ENV), String::from("1"));
    }
    if uikit_case_uses_real_app_hybrid_visible_preview(spec) {
        env.insert(
            String::from(UIKIT_PERF_CAMERA_REAL_APP_HYBRID_VISIBLE_PREVIEW_ENV),
            String::from("1"),
        );
    }
}

fn uikit_perf_launch_environment_json(
    spec: &UIKitCaseSpec,
    refresh_mode: UIKitDeviceRefreshMode,
) -> Result<String> {
    uikit_perf_launch_environment_json_with_trace_phases(spec, refresh_mode, false)
}

fn uikit_perf_launch_environment_json_with_trace_phases(
    spec: &UIKitCaseSpec,
    refresh_mode: UIKitDeviceRefreshMode,
    camera_trace_phases: bool,
) -> Result<String> {
    let mut env = BTreeMap::new();
    if let Some((scenario, route)) = uikit_launch_case_metadata(spec) {
        env.insert(String::from("OXIDE_PERF_UIKIT_LAUNCH"), String::from("1"));
        env.insert(String::from("OXIDE_PERF_LAUNCH_SCENARIO"), String::from(scenario));
        env.insert(String::from("OXIDE_PERF_TRACE_HANDSHAKE"), String::from("1"));
        if let Some(route) = route {
            env.insert(String::from("OXIDE_PERF_LAUNCH_ROUTE"), String::from(route));
        }
    } else {
        env.insert(String::from("OXIDE_PERF_PARKED"), String::from("1"));
        env.insert(String::from("OXIDE_PERF_CASE"), String::from(spec.test_name));
    }
    if let Some(value) = refresh_mode.env_value() {
        env.insert(String::from(UIKIT_PERF_REFRESH_MODE_ENV), String::from(value));
    }
    append_uikit_case_specific_perf_environment(&mut env, spec);
    if camera_trace_phases {
        env.insert(String::from(UIKIT_PERF_CAMERA_TRACE_PHASES_ENV), String::from("1"));
    }
    append_forwarded_uikit_perf_environment(&mut env);
    serde_json::to_string(&env)
        .with_context(|| format!("encoding parked benchmark environment for `{}`", spec.test_name))
}

fn uikit_perf_launch_args(
    device: &UIKitPhysicalDevice,
    built_app: &BuiltUIKitApp,
    spec: &UIKitCaseSpec,
    refresh_mode: UIKitDeviceRefreshMode,
    camera_trace_phases: bool,
) -> Result<Vec<String>> {
    Ok(vec![
        String::from("devicectl"),
        String::from("device"),
        String::from("process"),
        String::from("launch"),
        String::from("--device"),
        device.udid.clone(),
        String::from("--console"),
        String::from("--terminate-existing"),
        String::from("--environment-variables"),
        uikit_perf_launch_environment_json_with_trace_phases(
            spec,
            refresh_mode,
            camera_trace_phases,
        )?,
        built_app.bundle_identifier.clone(),
    ])
}

fn append_forwarded_uikit_perf_environment(env: &mut BTreeMap<String, String>) {
    for key in [
        UIKIT_PERF_MEASURE_ITERATIONS_ENV,
        UIKIT_PERF_BENCHMARK_ITERATIONS_ENV,
        UIKIT_PERF_CAMERA_MAX_DRAWABLE_COUNT_ENV,
        UIKIT_PERF_CAMERA_PREVIEW_SURFACE_SCALE_ENV,
        UIKIT_PERF_CAMERA_CAPTURE_CONTRACT_MODE_ENV,
        UIKIT_PERF_CAMERA_STAGE_MEASUREMENT_ENV,
        UIKIT_PERF_CAMERA_TINY_PREVIEW_RENDERER_ENV,
        UIKIT_PERF_CAMERA_PREVIEW_BACKPRESSURE_ENV,
    ] {
        if let Ok(value) = std::env::var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                env.insert(String::from(key), String::from(trimmed));
            }
        }
    }
    if !env.contains_key(UIKIT_PERF_CAMERA_MAX_DRAWABLE_COUNT_ENV) {
        env.insert(String::from(UIKIT_PERF_CAMERA_MAX_DRAWABLE_COUNT_ENV), String::from("2"));
    }
}

fn uikit_device_notification_observe_args(
    device: &UIKitPhysicalDevice,
    name: &str,
    timeout_secs: u64,
) -> Vec<String> {
    vec![
        String::from("devicectl"),
        String::from("device"),
        String::from("notification"),
        String::from("observe"),
        String::from("--device"),
        device.udid.clone(),
        String::from("--name"),
        String::from(name),
        String::from("--session-timeout"),
        timeout_secs.to_string(),
        String::from("--timeout"),
        (timeout_secs + 5).to_string(),
    ]
}

fn post_uikit_device_notification(
    root: &Path,
    device: &UIKitPhysicalDevice,
    name: &str,
) -> Result<()> {
    run_command_owned(
        root,
        "xcrun",
        &[
            String::from("devicectl"),
            String::from("device"),
            String::from("notification"),
            String::from("post"),
            String::from("--device"),
            device.udid.clone(),
            String::from("--name"),
            String::from(name),
        ],
        false,
    )
}

fn terminate_uikit_device_process(
    root: &Path,
    device: &UIKitPhysicalDevice,
    process_pid: u64,
) -> Result<()> {
    let args = vec![
        String::from("devicectl"),
        String::from("device"),
        String::from("process"),
        String::from("terminate"),
        String::from("--device"),
        device.udid.clone(),
        String::from("--pid"),
        process_pid.to_string(),
        String::from("--kill"),
    ];
    println!("> xcrun {}", args.join(" "));
    let output = Command::new("xcrun")
        .args(&args)
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("running xcrun {}", args.join(" ")))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.success()
        || is_expected_devicectl_process_termination_failure(&stdout, &stderr)
    {
        if !stdout.trim().is_empty() {
            print!("{}", stdout);
        }
        if !stderr.trim().is_empty() {
            eprint!("{}", stderr);
        }
        return Ok(());
    }
    if stderr.trim().is_empty() {
        if stdout.trim().is_empty() {
            bail!(
                "xcrun {} failed with status {}",
                args.join(" "),
                output.status.code().unwrap_or(-1)
            );
        }
        bail!(
            "xcrun {} failed with status {}: {}",
            args.join(" "),
            output.status.code().unwrap_or(-1),
            stdout.trim()
        );
    }
    bail!(
        "xcrun {} failed with status {}: {}",
        args.join(" "),
        output.status.code().unwrap_or(-1),
        stderr.trim()
    )
}

fn terminate_all_uikit_processes_named(
    root: &Path,
    device: &UIKitPhysicalDevice,
    process_name: &str,
) -> Result<()> {
    for pid in list_uikit_process_ids(root, device, process_name)? {
        terminate_uikit_device_process(root, device, pid)?;
    }
    Ok(())
}

fn run_uikit_device_build_for_testing(
    root: &Path,
    project: &Path,
    destination: &str,
    development_team: &str,
    derived_data_path: &Path,
) -> Result<()> {
    remove_existing_path(derived_data_path)?;
    let mut args = vec![
        String::from("build-for-testing"),
        String::from("-project"),
        project.to_string_lossy().into_owned(),
        String::from("-scheme"),
        String::from(DEFAULT_UIKIT_SCHEME),
        String::from("-destination"),
        String::from(destination),
        String::from("-derivedDataPath"),
        derived_data_path.to_string_lossy().into_owned(),
        String::from("-jobs"),
        host_parallel_job_count(),
    ];
    append_uikit_device_signing_args(&mut args, development_team);
    run_command_owned(root, "xcodebuild", &args, false)
}

fn run_react_device_build_for_testing(
    root: &Path,
    workspace: &Path,
    development_team: &str,
    derived_data_path: &Path,
) -> Result<()> {
    remove_existing_path(derived_data_path)?;
    let mut args = vec![
        String::from("build-for-testing"),
        String::from("-workspace"),
        workspace.to_string_lossy().into_owned(),
        String::from("-scheme"),
        String::from(DEFAULT_REACT_DEVICE_SCHEME),
        String::from("-destination"),
        String::from("generic/platform=iOS"),
        String::from("-derivedDataPath"),
        derived_data_path.to_string_lossy().into_owned(),
        String::from("-jobs"),
        host_parallel_job_count(),
    ];
    append_uikit_device_signing_args(&mut args, development_team);
    run_command_owned(root, "xcodebuild", &args, false)
}

fn uikit_device_perf_environment(refresh_mode: UIKitDeviceRefreshMode) -> Vec<(String, String)> {
    let mut env = BTreeMap::new();
    if let Some(value) = refresh_mode.env_value() {
        env.insert(String::from(UIKIT_PERF_REFRESH_MODE_ENV), String::from(value));
    }
    append_forwarded_uikit_perf_environment(&mut env);
    env.into_iter().collect()
}

fn uikit_device_perf_environment_for_specs(
    refresh_mode: UIKitDeviceRefreshMode,
    specs: &[&UIKitCaseSpec],
) -> Vec<(String, String)> {
    let mut env: BTreeMap<String, String> =
        uikit_device_perf_environment(refresh_mode).into_iter().collect();
    for spec in specs {
        append_uikit_case_specific_perf_environment(&mut env, spec);
    }
    env.into_iter().collect()
}

pub fn uikit_device_perf_environment_for_test_name(
    test_name: &str,
    refresh_mode: &str,
) -> Result<Vec<(String, String)>> {
    let spec = UIKIT_CASE_SPECS
        .iter()
        .find(|spec| spec.test_name == test_name || spec.case_id == test_name)
        .with_context(|| format!("unknown UIKit perf case `{}`", test_name))?;
    let mode = UIKitDeviceRefreshMode::parse_cli(refresh_mode)?
        .into_iter()
        .next()
        .with_context(|| format!("missing refresh mode from `{}`", refresh_mode))?;
    Ok(uikit_device_perf_environment_for_specs(mode, &[spec]))
}

pub fn prepare_uikit_device_perf_xctestrun(
    source_path: &Path,
    environment: &[(String, String)],
) -> Result<PathBuf> {
    let stem = source_path
        .file_stem()
        .and_then(|value| value.to_str())
        .with_context(|| format!("missing xctestrun file stem for {}", source_path.display()))?;
    let output_path = source_path.with_file_name(format!("{}-perf.xctestrun", stem));
    let mut plist_value: PlValue = plist::from_file(source_path)
        .with_context(|| format!("reading {}", source_path.display()))?;
    let mut applied_targets = 0usize;
    if xctestrun_contains_target(&plist_value, DEFAULT_UIKIT_TEST_TARGET) {
        apply_xctestrun_environment_overrides(
            &mut plist_value,
            DEFAULT_UIKIT_TEST_TARGET,
            environment,
        )?;
        applied_targets += 1;
    }
    if xctestrun_contains_target(&plist_value, DEFAULT_UIKIT_UI_TEST_TARGET) {
        apply_xctestrun_environment_overrides(
            &mut plist_value,
            DEFAULT_UIKIT_UI_TEST_TARGET,
            environment,
        )?;
        applied_targets += 1;
    }
    if applied_targets == 0 {
        bail!(
            "xctestrun plist at {} did not contain `{}` or `{}` target entries",
            source_path.display(),
            DEFAULT_UIKIT_TEST_TARGET,
            DEFAULT_UIKIT_UI_TEST_TARGET
        );
    }
    plist::to_file_xml(&output_path, &plist_value)
        .with_context(|| format!("writing {}", output_path.display()))?;
    Ok(output_path)
}

fn react_device_only_testing_identifier() -> String {
    format!(
        "{}{}/{}/{}",
        "-only-testing:",
        DEFAULT_REACT_DEVICE_TEST_TARGET,
        DEFAULT_REACT_DEVICE_TEST_CLASS,
        DEFAULT_REACT_DEVICE_TEST_NAME
    )
}

fn react_device_perf_environment() -> Vec<(String, String)> {
    vec![
        (String::from(UIKIT_PERF_MEASURE_ITERATIONS_ENV), String::from("5")),
        (String::from(UIKIT_PERF_BENCHMARK_ITERATIONS_ENV), String::from("24")),
        (String::from("OXIDE_PERF_TRACE_HANDSHAKE"), String::from("1")),
        (String::from("MTL_HUD_ENABLED"), String::from("0")),
    ]
}

fn xctestrun_contains_target(xctestrun: &PlValue, test_target: &str) -> bool {
    xctestrun.as_dictionary().map(|root| root.contains_key(test_target)).unwrap_or(false)
}

fn react_trace_console_case_label() -> &'static str {
    DEFAULT_REACT_DEVICE_TEST_NAME
}

fn react_device_perf_xcodebuild_args(
    xctestrun_path: &Path,
    destination: &str,
    result_bundle: &Path,
) -> Vec<String> {
    vec![
        String::from("test-without-building"),
        String::from("-xctestrun"),
        xctestrun_path.to_string_lossy().into_owned(),
        String::from("-destination"),
        destination.to_string(),
        String::from("-parallel-testing-enabled"),
        String::from("NO"),
        String::from("-enablePerformanceTestsDiagnostics"),
        String::from("NO"),
        String::from("-collect-test-diagnostics"),
        String::from("never"),
        String::from("-resultBundlePath"),
        result_bundle.to_string_lossy().into_owned(),
        react_device_only_testing_identifier(),
    ]
}

fn run_react_device_perf_case(
    root: &Path,
    device: &UIKitPhysicalDevice,
    built_app: &BuiltUIKitApp,
    xctestrun_path: &Path,
    destination: &str,
    result_root: &Path,
    trace_seconds: u64,
) -> Result<ReactDevicePerfRun> {
    let result_bundle = result_root.join("react-native-camera-bench.xcresult");
    let stdout_path = result_root.join("xcodebuild.stdout.log");
    let stderr_path = result_root.join("xcodebuild.stderr.log");
    let ready_stdout_path = result_root.join("ready.stdout.log");
    let ready_stderr_path = result_root.join("ready.stderr.log");
    let complete_stdout_path = result_root.join("complete.stdout.log");
    let complete_stderr_path = result_root.join("complete.stderr.log");
    let trace_path = result_root.join("metal.trace");
    let trace_stdout_path = result_root.join("metal.stdout.log");
    let trace_stderr_path = result_root.join("metal.stderr.log");
    let trace_started_stdout_path = result_root.join("trace-started.stdout.log");
    let trace_started_stderr_path = result_root.join("trace-started.stderr.log");
    remove_existing_path(&result_bundle)?;
    remove_existing_path(&stdout_path)?;
    remove_existing_path(&stderr_path)?;
    remove_existing_path(&ready_stdout_path)?;
    remove_existing_path(&ready_stderr_path)?;
    remove_existing_path(&complete_stdout_path)?;
    remove_existing_path(&complete_stderr_path)?;
    remove_existing_path(&trace_path)?;
    remove_existing_path(&trace_stdout_path)?;
    remove_existing_path(&trace_stderr_path)?;
    remove_existing_path(&trace_started_stdout_path)?;
    remove_existing_path(&trace_started_stderr_path)?;
    let prepared_xctestrun_path = prepare_react_device_perf_xctestrun(xctestrun_path)?;

    drain_uikit_processes(
        root,
        device,
        &built_app.executable_name,
        Duration::from_secs(5),
        "pre-react trace launch cleanup",
    )?;

    let ready_args = uikit_device_notification_observe_args(
        device,
        UIKIT_DEVICE_READY_NOTIFICATION,
        UIKIT_DEVICE_READY_TIMEOUT_SECS,
    );
    let mut ready_child = spawn_command_owned_with_output_paths(
        root,
        "xcrun",
        &ready_args,
        &ready_stdout_path,
        &ready_stderr_path,
    )?;
    thread::sleep(Duration::from_millis(UIKIT_DEVICE_NOTIFICATION_STARTUP_DELAY_MS));

    let xcodebuild_args =
        react_device_perf_xcodebuild_args(&prepared_xctestrun_path, destination, &result_bundle);
    let mut xcodebuild_child = spawn_command_owned_with_env_and_output_paths(
        root,
        "xcodebuild",
        &xcodebuild_args,
        &react_device_perf_environment(),
        &stdout_path,
        &stderr_path,
    )?;

    let ready_console_marker = format!("OXIDE_READY {}", react_trace_console_case_label());
    wait_for_device_notification_or_console_marker(
        "xcrun",
        &ready_args,
        &mut ready_child,
        &ready_stdout_path,
        &ready_stderr_path,
        UIKIT_DEVICE_READY_NOTIFICATION,
        &stdout_path,
        &ready_console_marker,
        Duration::from_secs(UIKIT_DEVICE_READY_TIMEOUT_SECS),
    )?;

    let trace_started_args =
        vec![String::from("-1"), String::from(UIKIT_TRACE_STARTED_NOTIFICATION)];
    let mut trace_started_child = spawn_command_owned_with_output_paths(
        root,
        "notifyutil",
        &trace_started_args,
        &trace_started_stdout_path,
        &trace_started_stderr_path,
    )?;
    thread::sleep(Duration::from_millis(UIKIT_DEVICE_NOTIFICATION_STARTUP_DELAY_MS));

    let complete_args = uikit_device_notification_observe_args(
        device,
        UIKIT_DEVICE_COMPLETE_NOTIFICATION,
        UIKIT_DEVICE_COMPLETE_TIMEOUT_SECS,
    );
    let mut complete_child = spawn_command_owned_with_output_paths(
        root,
        "xcrun",
        &complete_args,
        &complete_stdout_path,
        &complete_stderr_path,
    )?;
    thread::sleep(Duration::from_millis(UIKIT_DEVICE_NOTIFICATION_STARTUP_DELAY_MS));

    let trace_args = vec![
        String::from("xctrace"),
        String::from("record"),
        String::from("--template"),
        String::from("Metal System Trace"),
        String::from("--device"),
        device.udid.clone(),
        String::from("--all-processes"),
        String::from("--time-limit"),
        format!("{}s", trace_seconds),
        String::from("--output"),
        trace_path.to_string_lossy().into_owned(),
        String::from("--notify-tracing-started"),
        String::from(UIKIT_TRACE_STARTED_NOTIFICATION),
        String::from("--no-prompt"),
        String::from("--instrument"),
        String::from("Points of Interest"),
    ];
    let mut trace_child = spawn_command_owned_with_output_paths(
        root,
        "xcrun",
        &trace_args,
        &trace_stdout_path,
        &trace_stderr_path,
    )?;
    thread::sleep(Duration::from_millis(XCTRACE_STARTUP_DELAY_MS));
    wait_for_trace_started_or_trace_exit(
        "xcrun",
        &trace_args,
        &mut trace_child,
        &trace_stdout_path,
        &trace_stderr_path,
        &mut trace_started_child,
        &trace_started_stdout_path,
        &trace_started_stderr_path,
    )?;

    post_uikit_device_notification(root, device, UIKIT_DEVICE_START_NOTIFICATION)?;
    let complete_console_marker = format!("OXIDE_COMPLETE {}", react_trace_console_case_label());
    wait_for_device_notification_or_console_marker(
        "xcrun",
        &complete_args,
        &mut complete_child,
        &complete_stdout_path,
        &complete_stderr_path,
        UIKIT_DEVICE_COMPLETE_NOTIFICATION,
        &stdout_path,
        &complete_console_marker,
        Duration::from_secs(UIKIT_DEVICE_COMPLETE_TIMEOUT_SECS),
    )?;

    let xcodebuild_status = xcodebuild_child
        .wait()
        .with_context(|| format!("waiting for xcodebuild {}", xcodebuild_args.join(" ")))?;
    wait_for_child_with_output_paths(
        root,
        "xcrun",
        &trace_args,
        &mut trace_child,
        &trace_stdout_path,
        &trace_stderr_path,
    )?;
    drain_uikit_processes(
        root,
        device,
        &built_app.executable_name,
        Duration::from_secs(5),
        "react trace cleanup",
    )?;
    wait_for_xctrace_bundle_settle(&trace_path)?;

    Ok(ReactDevicePerfRun {
        result_bundle,
        stdout_path: stdout_path.clone(),
        trace_run: DeviceTraceRun {
            trace_path,
            launch_stdout_path: stdout_path,
            notes: vec![String::from(
                "GPU trace workflow: all-processes Metal System Trace + Points of Interest, with React workload windows bounded by the shared PerfWorkload signpost emitted from the app-hosted XCTest bundle.",
            )],
        },
        xcodebuild_status,
    })
}

fn install_uikit_device_app(
    root: &Path,
    device: &UIKitPhysicalDevice,
    built_app: &BuiltUIKitApp,
) -> Result<()> {
    run_command_owned(
        root,
        "xcrun",
        &[
            String::from("devicectl"),
            String::from("device"),
            String::from("install"),
            String::from("app"),
            String::from("--device"),
            device.udid.clone(),
            built_app.app_path.to_string_lossy().into_owned(),
        ],
        false,
    )
}

fn run_uikit_device_metrics_batch(
    root: &Path,
    xctestrun_path: &Path,
    destination: &str,
    specs: &[&'static UIKitCaseSpec],
    refresh_mode: UIKitDeviceRefreshMode,
    result_root: &Path,
) -> Result<String> {
    let mut metrics_json_fragments = Vec::new();
    let mut metric_shards: Vec<Vec<&'static UIKitCaseSpec>> = Vec::new();
    let mut current_shard: Vec<&'static UIKitCaseSpec> = Vec::new();
    for spec in specs {
        if spec.test_name.starts_with("testCamera") {
            if !current_shard.is_empty() {
                metric_shards.push(core::mem::take(&mut current_shard));
            }
            metric_shards.push(vec![*spec]);
            continue;
        }
        current_shard.push(*spec);
        if current_shard.len() == UIKIT_DEVICE_METRICS_BATCH_MAX_CASES {
            metric_shards.push(core::mem::take(&mut current_shard));
        }
    }
    if !current_shard.is_empty() {
        metric_shards.push(current_shard);
    }
    if metric_shards.is_empty() {
        metric_shards.push(Vec::new());
    }
    let shard_count = metric_shards.len();

    for (shard_index, shard_specs) in metric_shards.iter().enumerate() {
        let environment = uikit_device_perf_environment_for_specs(refresh_mode, shard_specs);
        let prepared_xctestrun_path =
            prepare_uikit_device_perf_xctestrun(xctestrun_path, &environment)?;
        let result_bundle = if shard_count == 1 {
            result_root.join(format!("metrics-{}.xcresult", refresh_mode.dir_suffix()))
        } else {
            result_root.join(format!(
                "metrics-{}-part{:02}.xcresult",
                refresh_mode.dir_suffix(),
                shard_index + 1
            ))
        };
        let stdout_path = if shard_specs.len() == 1 {
            uikit_device_metrics_case_stdout_path(
                result_root,
                refresh_mode.dir_suffix(),
                shard_specs[0].test_name,
            )
        } else {
            result_root.join(format!(
                "metrics-{}-part{:02}.stdout.log",
                refresh_mode.dir_suffix(),
                shard_index + 1
            ))
        };
        let stderr_path = if shard_specs.len() == 1 {
            uikit_device_metrics_case_stderr_path(
                result_root,
                refresh_mode.dir_suffix(),
                shard_specs[0].test_name,
            )
        } else {
            result_root.join(format!(
                "metrics-{}-part{:02}.stderr.log",
                refresh_mode.dir_suffix(),
                shard_index + 1
            ))
        };
        remove_existing_path(&result_bundle)?;
        remove_existing_path(&stdout_path)?;
        remove_existing_path(&stderr_path)?;

        let mut args = vec![
            String::from("test-without-building"),
            String::from("-xctestrun"),
            prepared_xctestrun_path.to_string_lossy().into_owned(),
            String::from("-destination"),
            String::from(destination),
            String::from("-parallel-testing-enabled"),
            String::from("NO"),
            String::from("-enablePerformanceTestsDiagnostics"),
            String::from("NO"),
            String::from("-collect-test-diagnostics"),
            String::from("never"),
            String::from("-resultBundlePath"),
            result_bundle.to_string_lossy().into_owned(),
        ];
        for spec in shard_specs {
            args.push(format!("-only-testing:{}", uikit_only_testing_identifier_for_spec(spec)));
        }
        let mut child = spawn_command_owned_with_env_and_output_paths(
            root,
            "xcodebuild",
            &args,
            &environment,
            &stdout_path,
            &stderr_path,
        )?;
        let run_result = wait_for_child_with_output_paths(
            root,
            "xcodebuild",
            &args,
            &mut child,
            &stdout_path,
            &stderr_path,
        );
        let extracted_metrics =
            extract_xcresult_metrics_json(root, &result_bundle).with_context(|| {
                format!(
                    "extracting sharded device metrics json for {} part {}",
                    refresh_mode.report_value(),
                    shard_index + 1
                )
            });
        let metrics_json = match (run_result, extracted_metrics) {
            (Ok(()), Ok(metrics_json)) => metrics_json,
            (Err(err), Ok(metrics_json)) => {
                eprintln!(
                    "xcodebuild exited with an error after producing usable metrics for {} part {}; continuing with the extracted xcresult metrics: {}",
                    refresh_mode.report_value(),
                    shard_index + 1,
                    err
                );
                metrics_json
            }
            (Ok(()), Err(err)) | (Err(_), Err(err)) => return Err(err),
        };
        metrics_json_fragments.push(metrics_json);
    }

    merge_xcresult_metrics_json_fragments(&metrics_json_fragments)
}

fn oxide_device_launch_environment_json(smoke: bool) -> Result<String> {
    let mut env = BTreeMap::new();
    env.insert(String::from("OXIDE_PERF_PARKED"), String::from("1"));
    env.insert(String::from("OXIDE_PERF_RUNNER"), String::from("1"));
    if smoke {
        env.insert(String::from("OXIDE_PERF_RUNNER_SMOKE"), String::from("1"));
    }
    if let Ok(label) = std::env::var("PERF_REPORT_DATE") {
        if !label.trim().is_empty() {
            env.insert(String::from("PERF_REPORT_DATE"), label);
        }
    }
    serde_json::to_string(&env)
        .with_context(|| "encoding parked Oxide device benchmark environment")
}

fn oxide_device_launch_args(
    device: &UIKitPhysicalDevice,
    built_app: &BuiltUIKitApp,
    smoke: bool,
) -> Result<Vec<String>> {
    Ok(vec![
        String::from("devicectl"),
        String::from("device"),
        String::from("process"),
        String::from("launch"),
        String::from("--device"),
        device.udid.clone(),
        String::from("--console"),
        String::from("--terminate-existing"),
        String::from("--environment-variables"),
        oxide_device_launch_environment_json(smoke)?,
        built_app.bundle_identifier.clone(),
    ])
}

fn run_oxide_device_report_capture(
    root: &Path,
    device: &UIKitPhysicalDevice,
    built_app: &BuiltUIKitApp,
    result_root: &Path,
    smoke: bool,
) -> Result<PerfReport> {
    let launch_stdout_path = result_root.join("launch.stdout.log");
    let launch_stderr_path = result_root.join("launch.stderr.log");
    let ready_stdout_path = result_root.join("ready.stdout.log");
    let ready_stderr_path = result_root.join("ready.stderr.log");
    let complete_stdout_path = result_root.join("complete.stdout.log");
    let complete_stderr_path = result_root.join("complete.stderr.log");
    remove_existing_path(&launch_stdout_path)?;
    remove_existing_path(&launch_stderr_path)?;
    remove_existing_path(&ready_stdout_path)?;
    remove_existing_path(&ready_stderr_path)?;
    remove_existing_path(&complete_stdout_path)?;
    remove_existing_path(&complete_stderr_path)?;

    drain_uikit_processes(
        root,
        device,
        &built_app.executable_name,
        Duration::from_secs(5),
        "pre-trace launch cleanup",
    )?;

    let ready_args = uikit_device_notification_observe_args(
        device,
        UIKIT_DEVICE_READY_NOTIFICATION,
        OXIDE_DEVICE_READY_TIMEOUT_SECS,
    );
    let mut ready_child = spawn_command_owned_with_output_paths(
        root,
        "xcrun",
        &ready_args,
        &ready_stdout_path,
        &ready_stderr_path,
    )?;
    thread::sleep(Duration::from_millis(UIKIT_DEVICE_NOTIFICATION_STARTUP_DELAY_MS));

    let complete_args = uikit_device_notification_observe_args(
        device,
        UIKIT_DEVICE_COMPLETE_NOTIFICATION,
        OXIDE_DEVICE_COMPLETE_TIMEOUT_SECS,
    );
    let mut complete_child = spawn_command_owned_with_output_paths(
        root,
        "xcrun",
        &complete_args,
        &complete_stdout_path,
        &complete_stderr_path,
    )?;
    thread::sleep(Duration::from_millis(UIKIT_DEVICE_NOTIFICATION_STARTUP_DELAY_MS));

    let launch_args = oxide_device_launch_args(device, built_app, smoke)?;
    let mut launch_child = spawn_command_owned_with_output_paths(
        root,
        "xcrun",
        &launch_args,
        &launch_stdout_path,
        &launch_stderr_path,
    )?;

    let process_pid = match wait_for_uikit_process_start(
        root,
        device,
        &built_app.executable_name,
        Duration::from_secs(15),
    ) {
        Ok(pid) => pid,
        Err(err) => {
            let _ = ready_child.kill();
            let _ = ready_child.wait();
            let _ = complete_child.kill();
            let _ = complete_child.wait();
            let _ = launch_child.kill();
            let _ = launch_child.wait();
            let _ = wait_for_uikit_process_clear(
                root,
                device,
                &built_app.executable_name,
                Duration::from_secs(5),
            );
            return Err(err);
        }
    };

    let _ = wait_for_ready_notification_or_assume_ready(
        &mut ready_child,
        &ready_stdout_path,
        &ready_stderr_path,
    )?;

    let start_result =
        post_uikit_device_notification(root, device, UIKIT_DEVICE_START_NOTIFICATION);
    let complete_result = if start_result.is_ok() {
        wait_for_child_with_output_paths(
            root,
            "xcrun",
            &complete_args,
            &mut complete_child,
            &complete_stdout_path,
            &complete_stderr_path,
        )
    } else {
        start_result.map(|_| ())
    };
    let launch_result = wait_for_console_launch_with_output_paths(
        root,
        "xcrun",
        &launch_args,
        &mut launch_child,
        &launch_stdout_path,
        &launch_stderr_path,
    );
    let clear_result = wait_for_uikit_process_clear(
        root,
        device,
        &built_app.executable_name,
        Duration::from_secs(15),
    );

    complete_result?;
    launch_result?;
    clear_result?;

    let stdout = fs::read_to_string(&launch_stdout_path)
        .with_context(|| format!("reading {}", launch_stdout_path.display()))?;
    let mut report = parse_oxide_device_report_json(&stdout).with_context(|| {
        format!("parsing Oxide device perf report from {}", launch_stdout_path.display())
    })?;
    report.suite = String::from("oxide-device");
    if report.generated_label.is_none() {
        report.generated_label = std::env::var("PERF_REPORT_DATE").ok();
    }
    report.contract.notes.push(format!(
        "Device flow: build/install the host app, launch the parked Oxide app on the physical iPhone with the in-process Rust perf runner enabled, trigger it over Darwin notifications, and exfiltrate the JSON report through the process-scoped devicectl console channel."
    ));
    report.contract.notes.push(format!("Device: `{}`", device.name));
    report.contract.notes.push(format!("Executable: `{}`", built_app.executable_name));
    if smoke {
        report.contract.notes.push(String::from("Capture mode: smoke-only device run."));
    } else {
        report.contract.notes.push(String::from("Capture mode: full device perf suite."));
    }
    let _ = process_pid;
    Ok(report)
}

fn run_uikit_device_case_trace(
    root: &Path,
    device: &UIKitPhysicalDevice,
    built_app: &BuiltUIKitApp,
    spec: &UIKitCaseSpec,
    refresh_mode: UIKitDeviceRefreshMode,
    case_dir: &Path,
    trace_seconds: u64,
) -> Result<DeviceTraceRun> {
    match run_uikit_device_case_trace_attempt(
        root,
        device,
        built_app,
        spec,
        refresh_mode,
        case_dir,
        trace_seconds,
        true,
    ) {
        Ok(run) => Ok(run),
        Err(err) if is_unsupported_gpu_counter_profile_error(&err.to_string()) => {
            println!(
                "Metal GPU Counters unsupported on {}; retrying `{}` without the counter profile.",
                device.name, spec.test_name
            );
            let mut run = run_uikit_device_case_trace_attempt(
                root,
                device,
                built_app,
                spec,
                refresh_mode,
                case_dir,
                trace_seconds,
                false,
            )?;
            run.notes.push(String::from(
                "GPU counter status: the attached device rejected the Metal GPU Counters profile, so this case was retried with direct GPU time and GPU latency only.",
            ));
            Ok(run)
        }
        Err(err) => Err(err),
    }
}

fn run_uikit_device_case_console_capture(
    root: &Path,
    device: &UIKitPhysicalDevice,
    built_app: &BuiltUIKitApp,
    spec: &UIKitCaseSpec,
    refresh_mode: UIKitDeviceRefreshMode,
    case_dir: &Path,
) -> Result<DeviceTraceRun> {
    let launch_stdout_path = case_dir.join("launch.stdout.log");
    let launch_stderr_path = case_dir.join("launch.stderr.log");
    let ready_stdout_path = case_dir.join("ready.stdout.log");
    let ready_stderr_path = case_dir.join("ready.stderr.log");
    let complete_stdout_path = case_dir.join("complete.stdout.log");
    let complete_stderr_path = case_dir.join("complete.stderr.log");
    remove_existing_path(&launch_stdout_path)?;
    remove_existing_path(&launch_stderr_path)?;
    remove_existing_path(&ready_stdout_path)?;
    remove_existing_path(&ready_stderr_path)?;
    remove_existing_path(&complete_stdout_path)?;
    remove_existing_path(&complete_stderr_path)?;

    drain_uikit_processes(
        root,
        device,
        &built_app.executable_name,
        Duration::from_secs(5),
        "pre-console launch cleanup",
    )?;

    let ready_args = uikit_device_notification_observe_args(
        device,
        UIKIT_DEVICE_READY_NOTIFICATION,
        UIKIT_DEVICE_READY_TIMEOUT_SECS,
    );
    let mut ready_child = spawn_command_owned_with_output_paths(
        root,
        "xcrun",
        &ready_args,
        &ready_stdout_path,
        &ready_stderr_path,
    )?;
    thread::sleep(Duration::from_millis(UIKIT_DEVICE_NOTIFICATION_STARTUP_DELAY_MS));

    let complete_args = uikit_device_notification_observe_args(
        device,
        UIKIT_DEVICE_COMPLETE_NOTIFICATION,
        UIKIT_DEVICE_COMPLETE_TIMEOUT_SECS,
    );
    let mut complete_child = spawn_command_owned_with_output_paths(
        root,
        "xcrun",
        &complete_args,
        &complete_stdout_path,
        &complete_stderr_path,
    )?;
    thread::sleep(Duration::from_millis(UIKIT_DEVICE_NOTIFICATION_STARTUP_DELAY_MS));

    let launch_args = uikit_perf_launch_args(device, built_app, spec, refresh_mode, false)?;
    let mut launch_child = spawn_command_owned_with_output_paths(
        root,
        "xcrun",
        &launch_args,
        &launch_stdout_path,
        &launch_stderr_path,
    )?;

    let process_pid = match wait_for_uikit_process_start(
        root,
        device,
        &built_app.executable_name,
        Duration::from_secs(15),
    ) {
        Ok(pid) => pid,
        Err(err) => {
            let _ = ready_child.kill();
            let _ = ready_child.wait();
            let _ = complete_child.kill();
            let _ = complete_child.wait();
            let _ = launch_child.kill();
            let _ = launch_child.wait();
            let _ = wait_for_uikit_process_clear(
                root,
                device,
                &built_app.executable_name,
                Duration::from_secs(5),
            );
            return Err(err);
        }
    };

    let _ = wait_for_ready_notification_or_assume_ready(
        &mut ready_child,
        &ready_stdout_path,
        &ready_stderr_path,
    )?;
    let console_case_label = uikit_trace_console_case_label(spec);
    let ready_console_marker = format!("OXIDE_READY {}", console_case_label);
    wait_for_device_notification_or_console_marker(
        "xcrun",
        &ready_args,
        &mut ready_child,
        &ready_stdout_path,
        &ready_stderr_path,
        UIKIT_DEVICE_READY_NOTIFICATION,
        &launch_stdout_path,
        &ready_console_marker,
        Duration::from_secs(UIKIT_DEVICE_READY_TIMEOUT_SECS),
    )?;

    let start_result =
        post_uikit_device_notification(root, device, UIKIT_DEVICE_START_NOTIFICATION);
    let complete_console_marker = format!("OXIDE_COMPLETE {}", console_case_label);
    let complete_result = if start_result.is_ok() {
        wait_for_device_notification_or_console_marker(
            "xcrun",
            &complete_args,
            &mut complete_child,
            &complete_stdout_path,
            &complete_stderr_path,
            UIKIT_DEVICE_COMPLETE_NOTIFICATION,
            &launch_stdout_path,
            &complete_console_marker,
            Duration::from_secs(UIKIT_DEVICE_COMPLETE_TIMEOUT_SECS),
        )
    } else {
        start_result.map(|_| ())
    };
    let launch_result = wait_for_console_launch_with_output_paths(
        root,
        "xcrun",
        &launch_args,
        &mut launch_child,
        &launch_stdout_path,
        &launch_stderr_path,
    );
    let terminate_result = terminate_uikit_device_process(root, device, process_pid);
    let clear_result = wait_for_uikit_process_clear(
        root,
        device,
        &built_app.executable_name,
        Duration::from_secs(15),
    );

    complete_result?;
    terminate_result?;
    launch_result?;
    clear_result?;

    Ok(DeviceTraceRun {
        trace_path: PathBuf::new(),
        launch_stdout_path,
        notes: vec![String::from(
            "GPU trace status: skipped for this case because `--trace-seconds 0` disabled the attached Metal trace.",
        )],
    })
}

fn run_uikit_device_case_trace_attempt(
    root: &Path,
    device: &UIKitPhysicalDevice,
    built_app: &BuiltUIKitApp,
    spec: &UIKitCaseSpec,
    refresh_mode: UIKitDeviceRefreshMode,
    case_dir: &Path,
    trace_seconds: u64,
    include_gpu_counters: bool,
) -> Result<DeviceTraceRun> {
    let trace_label = "metal";
    let template_name = "Metal System Trace";
    let mut extra_instruments = vec![String::from("Points of Interest")];
    if include_gpu_counters {
        extra_instruments.push(String::from("Metal GPU Counters"));
    }
    let (trace_path, launch_stdout_path, stderr_path) = run_uikit_device_attached_trace(
        root,
        device,
        built_app,
        spec,
        refresh_mode,
        case_dir,
        trace_label,
        template_name,
        &extra_instruments,
        trace_seconds,
    )?;
    let mut notes = Vec::new();
    let stderr = fs::read_to_string(&stderr_path).unwrap_or_default();
    if include_gpu_counters && is_unsupported_gpu_counter_profile_error(&stderr) {
        notes.push(String::from(
            "GPU counter status: the attached device rejected the Metal GPU Counters profile, so this case includes direct GPU time and GPU latency only.",
        ));
    }
    Ok(DeviceTraceRun { trace_path, launch_stdout_path, notes })
}

fn run_uikit_device_attached_trace(
    root: &Path,
    device: &UIKitPhysicalDevice,
    built_app: &BuiltUIKitApp,
    spec: &UIKitCaseSpec,
    refresh_mode: UIKitDeviceRefreshMode,
    case_dir: &Path,
    trace_label: &str,
    template_name: &str,
    extra_instruments: &[String],
    trace_seconds: u64,
) -> Result<(PathBuf, PathBuf, PathBuf)> {
    let trace_path = case_dir.join(format!("{}.trace", trace_label));
    let stdout_path = case_dir.join(format!("{}.stdout.log", trace_label));
    let stderr_path = case_dir.join(format!("{}.stderr.log", trace_label));
    let launch_stdout_path = case_dir.join("launch.stdout.log");
    let launch_stderr_path = case_dir.join("launch.stderr.log");
    let ready_stdout_path = case_dir.join("ready.stdout.log");
    let ready_stderr_path = case_dir.join("ready.stderr.log");
    let complete_stdout_path = case_dir.join("complete.stdout.log");
    let complete_stderr_path = case_dir.join("complete.stderr.log");
    let trace_started_stdout_path = case_dir.join("trace-started.stdout.log");
    let trace_started_stderr_path = case_dir.join("trace-started.stderr.log");
    remove_existing_path(&trace_path)?;
    remove_existing_path(&stdout_path)?;
    remove_existing_path(&stderr_path)?;
    remove_existing_path(&launch_stdout_path)?;
    remove_existing_path(&launch_stderr_path)?;
    remove_existing_path(&ready_stdout_path)?;
    remove_existing_path(&ready_stderr_path)?;
    remove_existing_path(&complete_stdout_path)?;
    remove_existing_path(&complete_stderr_path)?;
    remove_existing_path(&trace_started_stdout_path)?;
    remove_existing_path(&trace_started_stderr_path)?;

    drain_uikit_processes(
        root,
        device,
        &built_app.executable_name,
        Duration::from_secs(5),
        "pre-trace launch cleanup",
    )?;

    let ready_args = uikit_device_notification_observe_args(
        device,
        UIKIT_DEVICE_READY_NOTIFICATION,
        UIKIT_DEVICE_READY_TIMEOUT_SECS,
    );
    let mut ready_child = spawn_command_owned_with_output_paths(
        root,
        "xcrun",
        &ready_args,
        &ready_stdout_path,
        &ready_stderr_path,
    )?;
    thread::sleep(Duration::from_millis(UIKIT_DEVICE_NOTIFICATION_STARTUP_DELAY_MS));

    let launch_args = uikit_perf_launch_args(device, built_app, spec, refresh_mode, true)?;
    let mut launch_child = spawn_command_owned_with_output_paths(
        root,
        "xcrun",
        &launch_args,
        &launch_stdout_path,
        &launch_stderr_path,
    )?;
    let process_pid = match wait_for_uikit_process_start(
        root,
        device,
        &built_app.executable_name,
        Duration::from_secs(15),
    ) {
        Ok(pid) => pid,
        Err(err) => {
            let _ = ready_child.kill();
            let _ = ready_child.wait();
            let _ = launch_child.kill();
            let _ = launch_child.wait();
            let _ = wait_for_uikit_process_clear(
                root,
                device,
                &built_app.executable_name,
                Duration::from_secs(5),
            );
            return Err(err);
        }
    };
    let _ = wait_for_ready_notification_or_assume_ready(
        &mut ready_child,
        &ready_stdout_path,
        &ready_stderr_path,
    )?;
    let console_case_label = uikit_trace_console_case_label(spec);
    let ready_console_marker = format!("OXIDE_READY {}", console_case_label);
    wait_for_device_notification_or_console_marker(
        "xcrun",
        &ready_args,
        &mut ready_child,
        &ready_stdout_path,
        &ready_stderr_path,
        UIKIT_DEVICE_READY_NOTIFICATION,
        &launch_stdout_path,
        &ready_console_marker,
        Duration::from_secs(UIKIT_DEVICE_READY_TIMEOUT_SECS),
    )?;
    let attach_ready_delay_ms = xctrace_attach_ready_delay_ms();
    if attach_ready_delay_ms > 0 {
        thread::sleep(Duration::from_millis(attach_ready_delay_ms));
    }

    let mut trace_child = None;
    let mut complete_child = None;
    let mut active_trace_args = Vec::new();
    let mut attach_error = None;
    for attempt in 0..XCTRACE_ATTACH_RETRIES {
        let trace_started_args =
            vec![String::from("-1"), String::from(UIKIT_TRACE_STARTED_NOTIFICATION)];
        remove_existing_path(&trace_started_stdout_path)?;
        remove_existing_path(&trace_started_stderr_path)?;
        let mut trace_started_child = spawn_command_owned_with_output_paths(
            root,
            "notifyutil",
            &trace_started_args,
            &trace_started_stdout_path,
            &trace_started_stderr_path,
        )?;
        thread::sleep(Duration::from_millis(UIKIT_DEVICE_NOTIFICATION_STARTUP_DELAY_MS));

        let complete_args = uikit_device_notification_observe_args(
            device,
            UIKIT_DEVICE_COMPLETE_NOTIFICATION,
            UIKIT_DEVICE_COMPLETE_TIMEOUT_SECS,
        );
        remove_existing_path(&complete_stdout_path)?;
        remove_existing_path(&complete_stderr_path)?;
        let mut complete_child_attempt = spawn_command_owned_with_output_paths(
            root,
            "xcrun",
            &complete_args,
            &complete_stdout_path,
            &complete_stderr_path,
        )?;
        thread::sleep(Duration::from_millis(UIKIT_DEVICE_NOTIFICATION_STARTUP_DELAY_MS));

        let mut trace_args = vec![
            String::from("xctrace"),
            String::from("record"),
            String::from("--template"),
            String::from(template_name),
            String::from("--device"),
            device.udid.clone(),
            String::from("--attach"),
            process_pid.to_string(),
            String::from("--time-limit"),
            format!("{}s", trace_seconds),
            String::from("--output"),
            trace_path.to_string_lossy().into_owned(),
            String::from("--notify-tracing-started"),
            String::from(UIKIT_TRACE_STARTED_NOTIFICATION),
            String::from("--no-prompt"),
        ];
        for instrument in extra_instruments {
            trace_args.push(String::from("--instrument"));
            trace_args.push(instrument.clone());
        }
        remove_existing_path(&trace_path)?;
        remove_existing_path(&stdout_path)?;
        remove_existing_path(&stderr_path)?;
        let mut child = spawn_command_owned_with_output_paths(
            root,
            "xcrun",
            &trace_args,
            &stdout_path,
            &stderr_path,
        )?;
        thread::sleep(Duration::from_millis(XCTRACE_STARTUP_DELAY_MS));
        let started_result = wait_for_trace_started_or_trace_exit(
            "xcrun",
            &trace_args,
            &mut child,
            &stdout_path,
            &stderr_path,
            &mut trace_started_child,
            &trace_started_stdout_path,
            &trace_started_stderr_path,
        );
        match started_result {
            Ok(()) => {
                active_trace_args = trace_args;
                trace_child = Some(child);
                complete_child = Some((complete_args, complete_child_attempt));
                break;
            }
            Err(err) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = trace_started_child.kill();
                let _ = trace_started_child.wait();
                let _ = complete_child_attempt.kill();
                let _ = complete_child_attempt.wait();
                let can_retry = is_missing_xctrace_attach_process_error(&err.to_string())
                    && attempt + 1 < XCTRACE_ATTACH_RETRIES;
                attach_error = Some(err);
                if can_retry {
                    thread::sleep(Duration::from_millis(XCTRACE_ATTACH_RETRY_DELAY_MS));
                    continue;
                }
                break;
            }
        }
    }
    let mut trace_child = match trace_child {
        Some(child) => child,
        None => {
            let terminate_result = terminate_uikit_device_process(root, device, process_pid);
            let launch_result = wait_for_console_launch_with_output_paths(
                root,
                "xcrun",
                &launch_args,
                &mut launch_child,
                &launch_stdout_path,
                &launch_stderr_path,
            );
            let clear_result = drain_uikit_processes(
                root,
                device,
                &built_app.executable_name,
                Duration::from_secs(5),
                "failed attached trace cleanup",
            );
            let _ = terminate_result;
            let _ = launch_result;
            let _ = clear_result;
            return Err(attach_error.unwrap_or_else(|| {
                anyhow::anyhow!(
                    "xcrun xctrace record never attached to pid `{}` after {} attempts",
                    process_pid,
                    XCTRACE_ATTACH_RETRIES
                )
            }));
        }
    };
    let (complete_args, mut complete_child) = complete_child.with_context(|| {
        format!(
            "xcrun {} started without a completion observer for pid `{}`",
            active_trace_args.join(" "),
            process_pid
        )
    })?;
    let start_result =
        post_uikit_device_notification(root, device, UIKIT_DEVICE_START_NOTIFICATION);
    let complete_console_marker = format!("OXIDE_COMPLETE {}", console_case_label);
    let complete_result = if start_result.is_ok() {
        wait_for_device_notification_or_console_marker(
            "xcrun",
            &complete_args,
            &mut complete_child,
            &complete_stdout_path,
            &complete_stderr_path,
            UIKIT_DEVICE_COMPLETE_NOTIFICATION,
            &launch_stdout_path,
            &complete_console_marker,
            Duration::from_secs(UIKIT_DEVICE_COMPLETE_TIMEOUT_SECS),
        )
    } else {
        start_result.map(|_| ())
    };
    let trace_result = wait_for_child_with_output_paths(
        root,
        "xcrun",
        &active_trace_args,
        &mut trace_child,
        &stdout_path,
        &stderr_path,
    );
    let terminate_result = terminate_uikit_device_process(root, device, process_pid);
    let launch_result = wait_for_console_launch_with_output_paths(
        root,
        "xcrun",
        &launch_args,
        &mut launch_child,
        &launch_stdout_path,
        &launch_stderr_path,
    );
    let clear_result = drain_uikit_processes(
        root,
        device,
        &built_app.executable_name,
        Duration::from_secs(5),
        "attached trace cleanup",
    );
    complete_result?;
    trace_result?;
    terminate_result?;
    if let Err(err) = launch_result {
        eprintln!("[xtask] non-fatal console launch teardown error for attached trace: {err}");
    }
    clear_result?;
    wait_for_xctrace_bundle_settle(&trace_path)?;

    Ok((trace_path, launch_stdout_path, stderr_path))
}

fn wait_for_trace_started_or_trace_exit(
    program: &str,
    args: &[String],
    trace_child: &mut Child,
    trace_stdout_path: &Path,
    trace_stderr_path: &Path,
    started_child: &mut Child,
    started_stdout_path: &Path,
    started_stderr_path: &Path,
) -> Result<()> {
    let deadline = Instant::now() + Duration::from_millis(XCTRACE_STARTED_TIMEOUT_MS);
    loop {
        if let Some(status) = started_child
            .try_wait()
            .with_context(|| format!("probing notifyutil {}", UIKIT_TRACE_STARTED_NOTIFICATION))?
        {
            let stdout = fs::read_to_string(started_stdout_path).unwrap_or_default();
            let stderr = fs::read_to_string(started_stderr_path).unwrap_or_default();
            if status.success() {
                return Ok(());
            }
            let stdout = stdout.trim();
            let stderr = stderr.trim();
            if stderr.is_empty() {
                if stdout.is_empty() {
                    bail!(
                        "notifyutil -1 {} failed with status {}",
                        UIKIT_TRACE_STARTED_NOTIFICATION,
                        status.code().unwrap_or(-1)
                    );
                }
                bail!(
                    "notifyutil -1 {} failed with status {}: {}",
                    UIKIT_TRACE_STARTED_NOTIFICATION,
                    status.code().unwrap_or(-1),
                    stdout
                );
            }
            bail!(
                "notifyutil -1 {} failed with status {}: {}",
                UIKIT_TRACE_STARTED_NOTIFICATION,
                status.code().unwrap_or(-1),
                stderr
            );
        }
        if let Some(status) = trace_child
            .try_wait()
            .with_context(|| format!("probing {} {}", program, args.join(" ")))?
        {
            let stdout = fs::read_to_string(trace_stdout_path).unwrap_or_default();
            let stderr = fs::read_to_string(trace_stderr_path).unwrap_or_default();
            let stdout = stdout.trim();
            let stderr = stderr.trim();
            if status.success() {
                bail!(
                    "{} {} exited before sending `{}`",
                    program,
                    args.join(" "),
                    UIKIT_TRACE_STARTED_NOTIFICATION
                );
            }
            if stderr.is_empty() {
                if stdout.is_empty() {
                    bail!(
                        "{} {} failed with status {}",
                        program,
                        args.join(" "),
                        status.code().unwrap_or(-1)
                    );
                }
                bail!(
                    "{} {} failed with status {}: {}",
                    program,
                    args.join(" "),
                    status.code().unwrap_or(-1),
                    stdout
                );
            }
            bail!(
                "{} {} failed with status {}: {}",
                program,
                args.join(" "),
                status.code().unwrap_or(-1),
                stderr
            );
        }
        if Instant::now() >= deadline {
            bail!(
                "{} {} did not emit `{}` within {} ms",
                program,
                args.join(" "),
                UIKIT_TRACE_STARTED_NOTIFICATION,
                XCTRACE_STARTED_TIMEOUT_MS
            );
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn wait_for_ready_notification_or_assume_ready(
    ready_child: &mut Child,
    ready_stdout_path: &Path,
    ready_stderr_path: &Path,
) -> Result<bool> {
    let deadline = Instant::now() + Duration::from_millis(UIKIT_DEVICE_READY_GRACE_MS);
    loop {
        if let Some(status) = ready_child
            .try_wait()
            .with_context(|| format!("probing xcrun {}", UIKIT_DEVICE_READY_NOTIFICATION))?
        {
            let stdout = fs::read_to_string(ready_stdout_path).unwrap_or_default();
            let stderr = fs::read_to_string(ready_stderr_path).unwrap_or_default();
            if status.success() {
                return Ok(true);
            }
            let stdout = stdout.trim();
            let stderr = stderr.trim();
            if stderr.is_empty() {
                if stdout.is_empty() {
                    bail!(
                        "xcrun devicectl device notification observe --name {} failed with status {}",
                        UIKIT_DEVICE_READY_NOTIFICATION,
                        status.code().unwrap_or(-1)
                    );
                }
                bail!(
                    "xcrun devicectl device notification observe --name {} failed with status {}: {}",
                    UIKIT_DEVICE_READY_NOTIFICATION,
                    status.code().unwrap_or(-1),
                    stdout
                );
            }
            bail!(
                "xcrun devicectl device notification observe --name {} failed with status {}: {}",
                UIKIT_DEVICE_READY_NOTIFICATION,
                status.code().unwrap_or(-1),
                stderr
            );
        }
        if Instant::now() >= deadline {
            let _ = ready_child.kill();
            let _ = ready_child.wait();
            return Ok(false);
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn wait_for_device_notification_or_console_marker(
    program: &str,
    args: &[String],
    child: &mut Child,
    stdout_path: &Path,
    stderr_path: &Path,
    notification_name: &str,
    console_stdout_path: &Path,
    console_marker: &str,
    timeout: Duration,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    let mut observer_result: Option<(std::process::ExitStatus, String, String)> = None;
    loop {
        let console_stdout = fs::read_to_string(console_stdout_path).unwrap_or_default();
        if console_output_contains_marker(&console_stdout, console_marker) {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(());
        }
        if observer_result.is_none() {
            if let Some(status) = child
                .try_wait()
                .with_context(|| format!("probing {} {}", program, args.join(" ")))?
            {
                let stdout = fs::read_to_string(stdout_path).unwrap_or_default();
                let stderr = fs::read_to_string(stderr_path).unwrap_or_default();
                if notification_or_console_marker_observed(
                    &stdout,
                    notification_name,
                    &console_stdout,
                    console_marker,
                ) {
                    return Ok(());
                }
                observer_result = Some((status, stdout, stderr));
            }
        }
        if Instant::now() >= deadline {
            if observer_result.is_none() {
                let _ = child.kill();
                let _ = child.wait();
                bail!(
                    "timed out waiting for `{}` or console marker `{}` from {} {}",
                    notification_name,
                    console_marker,
                    program,
                    args.join(" ")
                );
            }
            let (status, stdout, stderr) =
                observer_result.take().expect("observer result present after deadline");
            let stdout = stdout.trim();
            let stderr = stderr.trim();
            if status.success() {
                bail!(
                    "{} {} exited without observing `{}` and console marker `{}` never appeared before the timeout",
                    program,
                    args.join(" "),
                    notification_name,
                    console_marker
                );
            }
            if stderr.is_empty() {
                if stdout.is_empty() {
                    bail!(
                        "{} {} failed with status {} and console marker `{}` never appeared before the timeout",
                        program,
                        args.join(" "),
                        status.code().unwrap_or(-1),
                        console_marker
                    );
                }
                bail!(
                    "{} {} failed with status {}: {}",
                    program,
                    args.join(" "),
                    status.code().unwrap_or(-1),
                    stdout
                );
            }
            bail!(
                "{} {} failed with status {}: {}",
                program,
                args.join(" "),
                status.code().unwrap_or(-1),
                stderr
            );
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn uikit_trace_console_case_label(spec: &UIKitCaseSpec) -> String {
    if let Some((scenario, _route)) = uikit_launch_case_metadata(spec) {
        return String::from(scenario);
    }
    String::from(spec.test_name)
}

pub fn parse_available_ios_sim_destination(json: &str) -> Result<Option<String>> {
    let list: SimCtlList =
        serde_json::from_str(json).with_context(|| "parsing simctl device list")?;
    let mut candidates = Vec::new();

    for (runtime, devices) in list.devices {
        if !runtime.contains(".iOS-") {
            continue;
        }
        let version = parse_sim_runtime_version(&runtime);
        for device in devices {
            if !device.name.starts_with("iPhone") {
                continue;
            }
            candidates.push((
                preferred_uikit_device_rank(&device.name),
                device.state == "Booted",
                version,
                device.name,
                device.udid,
            ));
        }
    }

    candidates.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| right.2.cmp(&left.2))
            .then_with(|| left.3.cmp(&right.3))
            .then_with(|| left.4.cmp(&right.4))
    });
    Ok(candidates
        .into_iter()
        .next()
        .map(|(_, _, _, _, udid)| format!("platform=iOS Simulator,id={}", udid)))
}

fn parse_sim_runtime_version(runtime: &str) -> (u32, u32, u32) {
    let tail = runtime.rsplit('.').next().unwrap_or(runtime);
    let mut parts =
        tail.trim_start_matches("iOS-").split('-').filter_map(|value| value.parse::<u32>().ok());
    (parts.next().unwrap_or(0), parts.next().unwrap_or(0), parts.next().unwrap_or(0))
}

fn preferred_uikit_device_rank(name: &str) -> usize {
    PREFERRED_UIKIT_DEVICE_NAMES
        .iter()
        .position(|candidate| *candidate == name)
        .unwrap_or(PREFERRED_UIKIT_DEVICE_NAMES.len())
}

fn parse_ios_perf_cli(args: &[String]) -> Result<IosPerfCli> {
    let mut cli = IosPerfCli::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--compare" => {
                let path = it.next().context("missing value for --compare")?;
                cli.compare = Some(PathBuf::from(path));
            }
            "--json-out" => {
                let path = it.next().context("missing value for --json-out")?;
                cli.json_out = Some(PathBuf::from(path));
            }
            "--markdown-out" => {
                let path = it.next().context("missing value for --markdown-out")?;
                cli.markdown_out = Some(PathBuf::from(path));
            }
            "--result-bundle" => {
                let path = it.next().context("missing value for --result-bundle")?;
                cli.result_bundle = Some(PathBuf::from(path));
            }
            "--destination" => {
                let value = it.next().context("missing value for --destination")?;
                cli.destination = Some(value.clone());
            }
            "--write-baseline" => {
                cli.write_baseline = true;
            }
            other => bail!("unknown ios perf argument `{}`", other),
        }
    }
    Ok(cli)
}

fn parse_ios_device_perf_cli(args: &[String]) -> Result<IosDevicePerfCli> {
    let mut cli = IosDevicePerfCli::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--case" => {
                let value = it.next().context("missing value for --case")?;
                cli.cases.push(value.clone());
            }
            "--compare" => {
                let path = it.next().context("missing value for --compare")?;
                cli.compare = Some(PathBuf::from(path));
            }
            "--device" => {
                let value = it.next().context("missing value for --device")?;
                cli.device = Some(value.clone());
            }
            "--json-out" => {
                let path = it.next().context("missing value for --json-out")?;
                cli.json_out = Some(PathBuf::from(path));
            }
            "--markdown-out" => {
                let path = it.next().context("missing value for --markdown-out")?;
                cli.markdown_out = Some(PathBuf::from(path));
            }
            "--power-trace" => {
                let path = it.next().context("missing value for --power-trace")?;
                cli.power_trace = Some(PathBuf::from(path));
            }
            "--power-trace-root" => {
                let path = it.next().context("missing value for --power-trace-root")?;
                cli.power_trace_root = Some(PathBuf::from(path));
            }
            "--refresh-mode" => {
                let value = it.next().context("missing value for --refresh-mode")?;
                cli.refresh_modes.extend(UIKitDeviceRefreshMode::parse_cli(value)?);
            }
            "--reuse-derived-data" => {
                let path = it.next().context("missing value for --reuse-derived-data")?;
                cli.reuse_derived_data = Some(PathBuf::from(path));
            }
            "--result-root" => {
                let path = it.next().context("missing value for --result-root")?;
                cli.result_root = Some(PathBuf::from(path));
            }
            "--team" => {
                let value = it.next().context("missing value for --team")?;
                cli.team = Some(value.clone());
            }
            "--trace-seconds" => {
                let value = it.next().context("missing value for --trace-seconds")?;
                let seconds = value
                    .parse::<u64>()
                    .with_context(|| format!("parsing trace seconds from `{}`", value))?;
                cli.trace_seconds = Some(seconds);
            }
            "--write-baseline" => {
                cli.write_baseline = true;
            }
            other => bail!("unknown ios device-perf argument `{}`", other),
        }
    }
    normalize_uikit_refresh_modes(&mut cli.refresh_modes);
    Ok(cli)
}

fn parse_ios_react_device_perf_cli(args: &[String]) -> Result<IosReactDevicePerfCli> {
    let mut cli = IosReactDevicePerfCli::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--compare" => {
                let path = it.next().context("missing value for --compare")?;
                cli.compare = Some(PathBuf::from(path));
            }
            "--device" => {
                let value = it.next().context("missing value for --device")?;
                cli.device = Some(value.clone());
            }
            "--json-out" => {
                let path = it.next().context("missing value for --json-out")?;
                cli.json_out = Some(PathBuf::from(path));
            }
            "--markdown-out" => {
                let path = it.next().context("missing value for --markdown-out")?;
                cli.markdown_out = Some(PathBuf::from(path));
            }
            "--result-root" => {
                let path = it.next().context("missing value for --result-root")?;
                cli.result_root = Some(PathBuf::from(path));
            }
            "--reuse-derived-data" => {
                let path = it.next().context("missing value for --reuse-derived-data")?;
                cli.reuse_derived_data = Some(PathBuf::from(path));
            }
            "--team" => {
                let value = it.next().context("missing value for --team")?;
                cli.team = Some(value.clone());
            }
            "--trace-seconds" => {
                let value = it.next().context("missing value for --trace-seconds")?;
                let seconds = value
                    .parse::<u64>()
                    .with_context(|| format!("parsing trace seconds from `{}`", value))?;
                cli.trace_seconds = Some(seconds);
            }
            "--write-baseline" => {
                cli.write_baseline = true;
            }
            other => bail!("unknown ios react-device-perf argument `{}`", other),
        }
    }
    Ok(cli)
}

fn parse_ios_oxide_device_perf_cli(args: &[String]) -> Result<IosOxideDevicePerfCli> {
    let mut cli = IosOxideDevicePerfCli::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--compare" => {
                let path = it.next().context("missing value for --compare")?;
                cli.compare = Some(PathBuf::from(path));
            }
            "--device" => {
                let value = it.next().context("missing value for --device")?;
                cli.device = Some(value.clone());
            }
            "--json-out" => {
                let path = it.next().context("missing value for --json-out")?;
                cli.json_out = Some(PathBuf::from(path));
            }
            "--markdown-out" => {
                let path = it.next().context("missing value for --markdown-out")?;
                cli.markdown_out = Some(PathBuf::from(path));
            }
            "--result-root" => {
                let path = it.next().context("missing value for --result-root")?;
                cli.result_root = Some(PathBuf::from(path));
            }
            "--team" => {
                let value = it.next().context("missing value for --team")?;
                cli.team = Some(value.clone());
            }
            "--smoke" => {
                cli.smoke = true;
            }
            "--write-baseline" => {
                cli.write_baseline = true;
            }
            other => bail!("unknown ios oxide-device-perf argument `{}`", other),
        }
    }
    Ok(cli)
}

fn parse_ios_time_profiler_summary_cli(args: &[String]) -> Result<IosTimeProfilerSummaryCli> {
    let mut cli = IosTimeProfilerSummaryCli::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--json-out" => {
                let path = it.next().context("missing value for --json-out")?;
                cli.json_out = Some(PathBuf::from(path));
            }
            "--trace" => {
                let path = it.next().context("missing value for --trace")?;
                cli.trace = Some(PathBuf::from(path));
            }
            other => bail!("unknown ios time-profiler-summary argument `{}`", other),
        }
    }
    Ok(cli)
}

fn run_command_owned(root: &Path, program: &str, args: &[String], allow_fail: bool) -> Result<()> {
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    run_command(root, program, &arg_refs, allow_fail)
}

fn run_command_capture_owned(root: &Path, program: &str, args: &[String]) -> Result<String> {
    println!("> {} {}", program, args.join(" "));
    let output = Command::new(program)
        .args(args)
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("running {} {}", program, args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "{} {} failed with status {}",
            program,
            args.join(" "),
            output.status.code().unwrap_or(-1)
        );
    }
    String::from_utf8(output.stdout).with_context(|| format!("decoding stdout from {}", program))
}

fn spawn_command_owned_with_env_and_output_paths(
    root: &Path,
    program: &str,
    args: &[String],
    envs: &[(String, String)],
    stdout_path: &Path,
    stderr_path: &Path,
) -> Result<Child> {
    println!("> {} {}", program, args.join(" "));
    let stdout_file = fs::File::create(stdout_path)
        .with_context(|| format!("creating {}", stdout_path.display()))?;
    let stderr_file = fs::File::create(stderr_path)
        .with_context(|| format!("creating {}", stderr_path.display()))?;
    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(root)
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file));
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.spawn().with_context(|| format!("running {} {}", program, args.join(" ")))
}

fn append_uikit_device_signing_args(args: &mut Vec<String>, development_team: &str) {
    args.push(String::from("-allowProvisioningDeviceRegistration"));
    args.push(String::from("-allowProvisioningUpdates"));
    args.push(format!("DEVELOPMENT_TEAM={}", development_team));
    args.push(String::from("CODE_SIGN_STYLE=Automatic"));
    args.push(String::from("CODE_SIGN_IDENTITY=Apple Development"));
}

fn resolve_uikit_development_team(
    root: &Path,
    requested: Option<&str>,
    device_udid: Option<&str>,
) -> Result<String> {
    if let Some(team) = requested.map(str::trim).filter(|team| !team.is_empty()) {
        return Ok(team.to_string());
    }

    if let Some(team) = std::env::var("OXIDE_IOS_DEVELOPMENT_TEAM")
        .ok()
        .map(|team| team.trim().to_string())
        .filter(|team| !team.is_empty())
    {
        return Ok(team);
    }

    if let Some(team) = std::env::var("DEVELOPMENT_TEAM")
        .ok()
        .map(|team| team.trim().to_string())
        .filter(|team| !team.is_empty())
    {
        return Ok(team);
    }

    if let Some(team) = resolve_local_provisioning_profile_team(root, device_udid)? {
        return Ok(team);
    }

    let output = run_command_capture_owned(
        root,
        "security",
        &[
            String::from("find-identity"),
            String::from("-p"),
            String::from("codesigning"),
            String::from("-v"),
        ],
    )?;
    if let Some(team) = parse_apple_development_team_from_security_output(&output) {
        return Ok(team);
    }

    bail!(
        "unable to resolve an iOS development team; pass --team TEAM_ID or set OXIDE_IOS_DEVELOPMENT_TEAM"
    )
}

#[derive(Debug)]
struct ProvisioningProfileSummary {
    team_identifier: String,
    provisioned_devices: Vec<String>,
}

fn resolve_local_provisioning_profile_team(
    root: &Path,
    device_udid: Option<&str>,
) -> Result<Option<String>> {
    let Some(home) = std::env::var_os("HOME") else {
        return Ok(None);
    };
    let profiles_dir = PathBuf::from(home).join("Library/MobileDevice/Provisioning Profiles");
    if !profiles_dir.is_dir() {
        return Ok(None);
    }

    let mut matched_teams = BTreeSet::new();
    let mut fallback_teams = BTreeSet::new();
    for entry in fs::read_dir(&profiles_dir)
        .with_context(|| format!("reading {}", profiles_dir.display()))?
    {
        let entry = entry.with_context(|| format!("reading {}", profiles_dir.display()))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("mobileprovision") {
            continue;
        }
        let output = match run_command_capture_owned(
            root,
            "security",
            &[
                String::from("cms"),
                String::from("-D"),
                String::from("-i"),
                path.to_string_lossy().into_owned(),
            ],
        ) {
            Ok(output) => output,
            Err(_) => continue,
        };
        let Some(summary) = parse_provisioning_profile_summary(&output) else {
            continue;
        };
        fallback_teams.insert(summary.team_identifier.clone());
        if let Some(udid) = device_udid {
            if summary.provisioned_devices.iter().any(|device| device == udid) {
                matched_teams.insert(summary.team_identifier);
            }
        }
    }

    if matched_teams.len() == 1 {
        return Ok(matched_teams.into_iter().next());
    }
    if fallback_teams.len() == 1 {
        return Ok(fallback_teams.into_iter().next());
    }
    Ok(None)
}

fn parse_provisioning_profile_summary(text: &str) -> Option<ProvisioningProfileSummary> {
    let plist = PlValue::from_reader_xml(text.as_bytes()).ok()?;
    let dict = plist.as_dictionary()?;
    let team_identifier = dict.get("TeamIdentifier")?.as_array()?.first()?.as_string()?.to_string();
    let provisioned_devices = dict
        .get("ProvisionedDevices")
        .and_then(PlValue::as_array)
        .map(|devices| {
            devices.iter().filter_map(PlValue::as_string).map(str::to_string).collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Some(ProvisioningProfileSummary { team_identifier, provisioned_devices })
}

pub fn parse_provisioning_profile_team_identifier(text: &str) -> Option<String> {
    parse_provisioning_profile_summary(text).map(|summary| summary.team_identifier)
}

pub fn parse_apple_development_team_from_security_output(output: &str) -> Option<String> {
    for line in output.lines() {
        if !line.contains("Apple Development:") {
            continue;
        }
        let Some(open) = line.rfind('(') else {
            continue;
        };
        let Some(close_rel) = line[(open + 1)..].find(')') else {
            continue;
        };
        let close = open + 1 + close_rel;
        let team = line[(open + 1)..close].trim();
        if !team.is_empty() && team.chars().all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
        {
            return Some(team.to_string());
        }
    }
    None
}

fn spawn_command_owned_with_output_paths(
    root: &Path,
    program: &str,
    args: &[String],
    stdout_path: &Path,
    stderr_path: &Path,
) -> Result<Child> {
    println!("> {} {}", program, args.join(" "));
    let stdout_file = fs::File::create(stdout_path)
        .with_context(|| format!("creating {}", stdout_path.display()))?;
    let stderr_file = fs::File::create(stderr_path)
        .with_context(|| format!("creating {}", stderr_path.display()))?;
    Command::new(program)
        .args(args)
        .current_dir(root)
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .with_context(|| format!("running {} {}", program, args.join(" ")))
}

fn wait_for_child_with_output_paths(
    _root: &Path,
    program: &str,
    args: &[String],
    child: &mut Child,
    stdout_path: &Path,
    stderr_path: &Path,
) -> Result<()> {
    let status =
        child.wait().with_context(|| format!("waiting for {} {}", program, args.join(" ")))?;
    let stdout = fs::read_to_string(stdout_path).unwrap_or_default();
    let stderr = fs::read_to_string(stderr_path).unwrap_or_default();
    if status.success() {
        return Ok(());
    }
    let stdout = stdout.trim();
    let stderr = stderr.trim();
    if stderr.is_empty() {
        if stdout.is_empty() {
            bail!(
                "{} {} failed with status {}",
                program,
                args.join(" "),
                status.code().unwrap_or(-1)
            );
        }
        bail!(
            "{} {} failed with status {}: {}",
            program,
            args.join(" "),
            status.code().unwrap_or(-1),
            stdout
        );
    }
    bail!(
        "{} {} failed with status {}: {}",
        program,
        args.join(" "),
        status.code().unwrap_or(-1),
        stderr
    )
}

fn wait_for_console_launch_with_output_paths(
    _root: &Path,
    program: &str,
    args: &[String],
    child: &mut Child,
    stdout_path: &Path,
    stderr_path: &Path,
) -> Result<()> {
    let status =
        child.wait().with_context(|| format!("waiting for {} {}", program, args.join(" ")))?;
    let stdout = fs::read_to_string(stdout_path).unwrap_or_default();
    let stderr = fs::read_to_string(stderr_path).unwrap_or_default();
    if status.success()
        || is_expected_devicectl_console_termination(&stdout)
        || is_expected_devicectl_console_exit_with_app_output(&stdout, &stderr)
    {
        return Ok(());
    }
    let stdout = stdout.trim();
    let stderr = stderr.trim();
    if stderr.is_empty() {
        if stdout.is_empty() {
            bail!(
                "{} {} failed with status {}",
                program,
                args.join(" "),
                status.code().unwrap_or(-1)
            );
        }
        bail!(
            "{} {} failed with status {}: {}",
            program,
            args.join(" "),
            status.code().unwrap_or(-1),
            stdout
        );
    }
    bail!(
        "{} {} failed with status {}: {}",
        program,
        args.join(" "),
        status.code().unwrap_or(-1),
        stderr
    )
}

fn run_devicectl_json(root: &Path, args: &[String], label: &str) -> Result<String> {
    let (json, status) = run_devicectl_json_inner(root, args, label)?;
    if status == 0 {
        return Ok(json);
    }
    bail!("devicectl {} failed with status {}", args.join(" "), status)
}

fn run_devicectl_json_allow_failure(root: &Path, args: &[String], label: &str) -> Result<String> {
    let (json, _) = run_devicectl_json_inner(root, args, label)?;
    Ok(json)
}

fn run_devicectl_json_inner(root: &Path, args: &[String], label: &str) -> Result<(String, i32)> {
    let json_path =
        std::env::temp_dir().join(format!("oxide-xtask-{}-{}.json", label, std::process::id()));
    remove_existing_path(&json_path)?;
    let mut full_args = vec![String::from("devicectl")];
    full_args.extend_from_slice(args);
    full_args.push(String::from("-j"));
    full_args.push(json_path.to_string_lossy().into_owned());
    println!("> xcrun {}", full_args.join(" "));
    let status = Command::new("xcrun")
        .args(&full_args)
        .current_dir(root)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("running xcrun {}", full_args.join(" ")))?;
    let json = fs::read_to_string(&json_path)
        .with_context(|| format!("reading {}", json_path.display()))?;
    Ok((json, status.code().unwrap_or(-1)))
}

fn extract_xcresult_metrics_json(root: &Path, result_bundle: &Path) -> Result<String> {
    run_command_capture_owned(
        root,
        "xcrun",
        &[
            String::from("xcresulttool"),
            String::from("get"),
            String::from("test-results"),
            String::from("metrics"),
            String::from("--path"),
            result_bundle.to_string_lossy().into_owned(),
            String::from("--format"),
            String::from("json"),
        ],
    )
}

pub fn merge_xcresult_metrics_json_fragments(fragments: &[String]) -> Result<String> {
    let mut merged_bundles = Vec::new();
    for (index, fragment) in fragments.iter().enumerate() {
        let mut shard_bundles: Vec<serde_json::Value> = serde_json::from_str(fragment)
            .with_context(|| {
                format!("parsing sharded device metrics json fragment {}", index + 1)
            })?;
        merged_bundles.append(&mut shard_bundles);
    }
    serde_json::to_string(&merged_bundles).with_context(|| "serializing merged device metrics json")
}

fn remove_existing_path(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    if path.is_dir() {
        fs::remove_dir_all(path).with_context(|| format!("removing {}", path.display()))?;
    } else {
        fs::remove_file(path).with_context(|| format!("removing {}", path.display()))?;
    }
    Ok(())
}

fn wait_for_xctrace_bundle_settle(trace_path: &Path) -> Result<()> {
    let deadline = Instant::now() + Duration::from_millis(XCTRACE_TRACE_SETTLE_TIMEOUT_MS);
    let mut last_snapshot = None;
    loop {
        let snapshot = xctrace_bundle_snapshot(trace_path)?;
        if last_snapshot.as_ref() == Some(&snapshot) {
            return Ok(());
        }
        last_snapshot = Some(snapshot);
        if Instant::now() >= deadline {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(XCTRACE_TRACE_SETTLE_POLL_MS));
    }
}

fn xctrace_bundle_snapshot(trace_path: &Path) -> Result<(u64, u64, usize)> {
    let mut total_size = 0u64;
    let mut newest_mtime_ns = 0u64;
    let mut entries = 0usize;
    accumulate_xctrace_bundle_snapshot(
        trace_path,
        &mut total_size,
        &mut newest_mtime_ns,
        &mut entries,
    )?;
    if entries == 0 {
        bail!("xctrace trace bundle `{}` is empty", trace_path.display());
    }
    Ok((total_size, newest_mtime_ns, entries))
}

fn accumulate_xctrace_bundle_snapshot(
    path: &Path,
    total_size: &mut u64,
    newest_mtime_ns: &mut u64,
    entries: &mut usize,
) -> Result<()> {
    let metadata = fs::metadata(path).with_context(|| format!("reading {}", path.display()))?;
    *entries += 1;
    *total_size = total_size.saturating_add(metadata.len());
    let modified = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_nanos().min(u64::MAX as u128) as u64)
        .unwrap_or(0);
    *newest_mtime_ns = (*newest_mtime_ns).max(modified);
    if metadata.is_dir() {
        for entry in fs::read_dir(path).with_context(|| format!("reading {}", path.display()))? {
            let entry = entry.with_context(|| format!("reading {}", path.display()))?;
            accumulate_xctrace_bundle_snapshot(
                &entry.path(),
                total_size,
                newest_mtime_ns,
                entries,
            )?;
        }
    }
    Ok(())
}

fn export_xctrace_toc(root: &Path, trace_path: &Path) -> Result<Vec<XctraceTocTable>> {
    let text = export_xctrace_toc_text(root, trace_path)?;
    parse_xctrace_toc_tables(&text)
}

fn export_xctrace_toc_text(root: &Path, trace_path: &Path) -> Result<String> {
    wait_for_xctrace_bundle_settle(trace_path)?;
    run_xctrace_export(
        root,
        &[
            String::from("xctrace"),
            String::from("export"),
            String::from("--input"),
            trace_path.to_string_lossy().into_owned(),
            String::from("--toc"),
        ],
    )
}

fn export_xctrace_tables(
    root: &Path,
    trace_path: &Path,
    schema: &str,
) -> Result<Vec<XctraceTable>> {
    let combined = run_xctrace_export(
        root,
        &[
            String::from("xctrace"),
            String::from("export"),
            String::from("--input"),
            trace_path.to_string_lossy().into_owned(),
            String::from("--xpath"),
            format!("/trace-toc/run[1]/data[1]/table[@schema=\"{}\"]", schema),
        ],
    )?;
    let parsed = parse_xctrace_tables(&combined)?;
    if !parsed.is_empty() {
        return Ok(parsed);
    }

    let toc = export_xctrace_toc(root, trace_path)?;
    let mut out = Vec::new();
    for table in preferred_xctrace_toc_tables(&toc, schema, None) {
        let text = run_xctrace_export(
            root,
            &[
                String::from("xctrace"),
                String::from("export"),
                String::from("--input"),
                trace_path.to_string_lossy().into_owned(),
                String::from("--xpath"),
                table.xpath,
            ],
        )?;
        out.extend(parse_xctrace_tables(&text)?);
    }
    Ok(out)
}

fn export_xctrace_preferred_table(
    root: &Path,
    trace_path: &Path,
    schema: &str,
    preferred_category: Option<&str>,
) -> Result<XctraceTable> {
    let toc = export_xctrace_toc(root, trace_path)?;
    let candidates = preferred_xctrace_toc_tables(&toc, schema, preferred_category);
    let mut fallback = None;
    for table in candidates {
        let text = run_xctrace_export(
            root,
            &[
                String::from("xctrace"),
                String::from("export"),
                String::from("--input"),
                trace_path.to_string_lossy().into_owned(),
                String::from("--xpath"),
                table.xpath,
            ],
        )?;
        let parsed = parse_xctrace_tables(&text)?;
        if let Some(non_empty) = parsed.iter().find(|candidate| !candidate.rows.is_empty()) {
            return Ok(non_empty.clone());
        }
        if fallback.is_none() {
            fallback = parsed.into_iter().next();
        }
    }
    fallback.with_context(|| format!("missing `{}` table in {}", schema, trace_path.display()))
}

pub fn preferred_xctrace_toc_tables(
    toc: &[XctraceTocTable],
    schema: &str,
    preferred_category: Option<&str>,
) -> Vec<XctraceTocTable> {
    let mut filtered =
        toc.iter().filter(|table| table.schema == schema).cloned().collect::<Vec<_>>();
    filtered.sort_by(|left, right| {
        preferred_category
            .map(|category| {
                (!xctrace_category_matches_preferred(&left.category, category))
                    .cmp(&(!xctrace_category_matches_preferred(&right.category, category)))
                    .then_with(|| left.xpath.cmp(&right.xpath))
            })
            .unwrap_or_else(|| left.xpath.cmp(&right.xpath))
    });
    filtered
}

fn xctrace_category_matches_preferred(category: &str, preferred_category: &str) -> bool {
    if category == preferred_category || category.eq_ignore_ascii_case(preferred_category) {
        return true;
    }
    preferred_category == UIKIT_PERF_SIGNPOST_CATEGORY
        && category.eq_ignore_ascii_case("pointsOfInterest")
}

fn run_xctrace_export(root: &Path, args: &[String]) -> Result<String> {
    let cache_key = args.to_vec();
    if let Some(cached) = xctrace_export_cache().lock().unwrap().get(&cache_key).cloned() {
        return Ok(cached);
    }
    let mut last_error = None;
    for attempt in 0..XCTRACE_EXPORT_RETRIES {
        match run_command_capture_owned(root, "xcrun", args) {
            Ok(text) => {
                xctrace_export_cache().lock().unwrap().insert(cache_key.clone(), text.clone());
                return Ok(text);
            }
            Err(err) => {
                last_error = Some(err);
                if attempt + 1 < XCTRACE_EXPORT_RETRIES {
                    thread::sleep(Duration::from_millis(XCTRACE_EXPORT_RETRY_DELAY_MS));
                }
            }
        }
    }
    Err(last_error.with_context(|| "xctrace export failed without an error payload")?)
}

fn xctrace_export_cache() -> &'static Mutex<BTreeMap<Vec<String>, String>> {
    static CACHE: OnceLock<Mutex<BTreeMap<Vec<String>, String>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

pub fn parse_xctrace_toc_tables(text: &str) -> Result<Vec<XctraceTocTable>> {
    let doc = Document::parse(text.trim()).with_context(|| "parsing xctrace toc xml")?;
    let mut tables = Vec::new();
    for (index, node) in doc.descendants().filter(|node| node.has_tag_name("table")).enumerate() {
        let Some(schema) = node.attribute("schema") else {
            continue;
        };
        tables.push(XctraceTocTable {
            schema: schema.to_string(),
            xpath: format!("/trace-toc/run[1]/data[1]/table[{}]", index + 1),
            category: normalize_xctrace_toc_attr(node.attribute("category")),
            subsystem: normalize_xctrace_toc_attr(node.attribute("subsystem")),
        });
    }
    Ok(tables)
}

fn normalize_xctrace_toc_attr(value: Option<&str>) -> String {
    value.unwrap_or_default().trim().trim_matches('"').to_string()
}

pub fn parse_xctrace_tables(text: &str) -> Result<Vec<XctraceTable>> {
    let doc = Document::parse(text.trim()).with_context(|| "parsing xctrace query xml")?;
    let mut tables = Vec::new();
    let mut last_columns = None::<Vec<XctraceColumn>>;
    for node in doc.descendants().filter(|node| node.has_tag_name("node")) {
        let columns = if let Some(schema_node) =
            node.children().find(|child| child.is_element() && child.has_tag_name("schema"))
        {
            let columns = schema_node
                .children()
                .filter(|child| child.is_element() && child.has_tag_name("col"))
                .map(|column| XctraceColumn {
                    mnemonic: column_text(&column, "mnemonic"),
                    name: column_text(&column, "name"),
                    engineering_type: column_text(&column, "engineering-type"),
                })
                .collect::<Vec<_>>();
            last_columns = Some(columns.clone());
            columns
        } else if let Some(columns) = last_columns.clone() {
            columns
        } else {
            continue;
        };
        let mut rows = Vec::new();
        let mut cell_refs = BTreeMap::<String, XctraceCell>::new();
        for row_node in
            node.children().filter(|child| child.is_element() && child.has_tag_name("row"))
        {
            let mut values = BTreeMap::new();
            let mut col_index = 0usize;
            for value_node in row_node.children().filter(|child| child.is_element()) {
                if col_index >= columns.len() {
                    break;
                }
                let key = columns[col_index].mnemonic.clone();
                if !value_node.has_tag_name("sentinel") {
                    let mut cell = XctraceCell {
                        raw: value_node
                            .text()
                            .map(|text| text.trim().to_string())
                            .filter(|text| !text.is_empty()),
                        fmt: value_node.attribute("fmt").map(ToOwned::to_owned),
                    };
                    if let Some(reference_id) = value_node.attribute("ref") {
                        if let Some(reference_cell) = cell_refs.get(reference_id) {
                            if cell.raw.is_none() {
                                cell.raw = reference_cell.raw.clone();
                            }
                            if cell.fmt.is_none() {
                                cell.fmt = reference_cell.fmt.clone();
                            }
                        }
                    }
                    if let Some(cell_id) = value_node.attribute("id") {
                        cell_refs.insert(cell_id.to_string(), cell.clone());
                    }
                    values.insert(key, cell);
                }
                col_index += 1;
            }
            rows.push(XctraceRow { values });
        }
        tables.push(XctraceTable { columns, rows });
    }
    Ok(tables)
}

fn export_xctrace_preferred_table_xml(
    root: &Path,
    trace_path: &Path,
    schema: &str,
    preferred_category: Option<&str>,
) -> Result<String> {
    let toc = export_xctrace_toc(root, trace_path)?;
    let candidates = preferred_xctrace_toc_tables(&toc, schema, preferred_category);
    let mut fallback = None;
    for table in candidates {
        let text = run_xctrace_export(
            root,
            &[
                String::from("xctrace"),
                String::from("export"),
                String::from("--input"),
                trace_path.to_string_lossy().into_owned(),
                String::from("--xpath"),
                table.xpath,
            ],
        )?;
        let parsed = parse_xctrace_tables(&text)?;
        if parsed.iter().any(|candidate| !candidate.rows.is_empty()) {
            return Ok(text);
        }
        if fallback.is_none() {
            fallback = Some(text);
        }
    }
    fallback.with_context(|| format!("missing `{}` table in {}", schema, trace_path.display()))
}

#[derive(Debug, Clone)]
struct TimeProfileSampleRow {
    thread: String,
    thread_name: Option<String>,
    frames: Vec<String>,
}

fn parse_thread_info_names(text: &str) -> Result<BTreeMap<String, Option<String>>> {
    let doc = Document::parse(text.trim()).with_context(|| "parsing xctrace thread-info xml")?;
    let Some(node) = doc.descendants().find(|candidate| candidate.has_tag_name("node")) else {
        bail!("missing thread-info node");
    };
    let mut names = BTreeMap::new();
    let mut thread_fmt_by_id = BTreeMap::<String, String>::new();
    let mut thread_name_fmt_by_id = BTreeMap::<String, Option<String>>::new();
    for row in node.children().filter(|child| child.is_element() && child.has_tag_name("row")) {
        for child in row.children().filter(|candidate| candidate.is_element()) {
            if child.has_tag_name("thread") {
                if let Some(id) = child.attribute("id") {
                    if let Some(fmt) = child.attribute("fmt") {
                        thread_fmt_by_id.insert(id.to_string(), fmt.to_string());
                    }
                }
            } else if child.has_tag_name("thread-name") {
                let resolved =
                    child.attribute("fmt").map(ToOwned::to_owned).filter(|text| !text.is_empty());
                if let Some(id) = child.attribute("id") {
                    thread_name_fmt_by_id.insert(id.to_string(), resolved.clone());
                }
            }
        }
        let mut thread_fmt = None;
        let mut thread_name_fmt = None;
        for child in row.children().filter(|candidate| candidate.is_element()) {
            if child.has_tag_name("thread") {
                thread_fmt = child.attribute("fmt").map(ToOwned::to_owned).or_else(|| {
                    child
                        .attribute("ref")
                        .and_then(|reference| thread_fmt_by_id.get(reference).cloned())
                });
            } else if child.has_tag_name("thread-name") {
                thread_name_fmt = child
                    .attribute("fmt")
                    .map(ToOwned::to_owned)
                    .filter(|text| !text.is_empty())
                    .or_else(|| {
                        child.attribute("ref").and_then(|reference| {
                            thread_name_fmt_by_id.get(reference).cloned().flatten()
                        })
                    });
            }
        }
        if let Some(thread_fmt) = thread_fmt {
            names.insert(thread_fmt, thread_name_fmt);
        }
    }
    Ok(names)
}

fn parse_time_profile_rows(
    profile_xml: &str,
    thread_names: &BTreeMap<String, Option<String>>,
    windows: &[TraceWindow],
) -> Result<Vec<TimeProfileSampleRow>> {
    let doc =
        Document::parse(profile_xml.trim()).with_context(|| "parsing xctrace time-profile xml")?;
    let Some(node) = doc.descendants().find(|candidate| candidate.has_tag_name("node")) else {
        bail!("missing time-profile node");
    };
    let mut frame_name_by_id = BTreeMap::<String, String>::new();
    let mut thread_fmt_by_id = BTreeMap::<String, String>::new();
    let mut rows = Vec::new();
    for row in node.children().filter(|child| child.is_element() && child.has_tag_name("row")) {
        for frame in row
            .descendants()
            .filter(|candidate| candidate.is_element() && candidate.has_tag_name("frame"))
        {
            if let (Some(id), Some(name)) = (frame.attribute("id"), frame.attribute("name")) {
                frame_name_by_id.insert(id.to_string(), name.to_string());
            }
        }
        for thread in row
            .children()
            .filter(|candidate| candidate.is_element() && candidate.has_tag_name("thread"))
        {
            if let (Some(id), Some(fmt)) = (thread.attribute("id"), thread.attribute("fmt")) {
                thread_fmt_by_id.insert(id.to_string(), fmt.to_string());
            }
        }
        let time_ns = row
            .children()
            .find(|candidate| candidate.is_element() && candidate.has_tag_name("sample-time"))
            .and_then(|candidate| candidate.text())
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .and_then(|text| text.parse::<u64>().ok());
        let Some(time_ns) = time_ns else {
            continue;
        };
        if !windows.iter().any(|window| time_ns >= window.start_ns && time_ns <= window.end_ns) {
            continue;
        }
        let thread = row
            .children()
            .find(|candidate| candidate.is_element() && candidate.has_tag_name("thread"))
            .and_then(|candidate| {
                candidate.attribute("fmt").map(ToOwned::to_owned).or_else(|| {
                    candidate
                        .attribute("ref")
                        .and_then(|reference| thread_fmt_by_id.get(reference).cloned())
                })
            })
            .unwrap_or_else(|| String::from("<unknown-thread>"));
        let mut frames = Vec::new();
        if let Some(backtrace) = row
            .descendants()
            .find(|candidate| candidate.is_element() && candidate.has_tag_name("backtrace"))
        {
            for frame in backtrace
                .children()
                .filter(|candidate| candidate.is_element() && candidate.has_tag_name("frame"))
            {
                let name = frame.attribute("name").map(ToOwned::to_owned).or_else(|| {
                    frame
                        .attribute("ref")
                        .and_then(|reference| frame_name_by_id.get(reference).cloned())
                });
                if let Some(name) = name {
                    frames.push(name);
                }
            }
        }
        if frames.is_empty() {
            continue;
        }
        rows.push(TimeProfileSampleRow {
            thread: thread.clone(),
            thread_name: thread_names.get(&thread).cloned().flatten(),
            frames,
        });
    }
    Ok(rows)
}

fn time_profiler_bucket_name(frames: &[String]) -> &'static str {
    let joined = frames.join(" | ");
    let has_any = |needles: &[&str]| needles.iter().any(|needle| joined.contains(needle));
    if has_any(&["summarizeStageSamples"]) {
        return "benchmark_summary";
    }
    if has_any(&[
        "NametagCameraStream",
        "publishPreviewOnlyTextures",
        "updateLatestTextures",
        "copy_nv12",
        "CVMetalTextureCacheCreateTextureFromImage",
        "videoOutput",
    ]) {
        return "oxide_camera_bridge";
    }
    if has_any(&[
        "roDeserializeSampleBuffer",
        "FigRemote",
        "sbufAtom_",
        "CMBlockBuffer",
        "CVPixelBufferCreateWithIOSurface",
    ]) {
        return "sample_delivery";
    }
    if has_any(&[
        "nextDrawable",
        "IOSurface",
        "FPInFlightDrawable",
        "AGX::RenderContext",
        "_MTLCommandBuffer",
        "IOGPUMetalCommandBuffer",
        "AGXG18PFamilyCommandBuffer",
        "layer_presented",
        "presentDrawable",
        "present_drawable",
        "commit",
    ]) {
        return "renderer_driver_present";
    }
    "other"
}

fn time_profiler_share_pct(samples: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        (((samples as f64) / (total as f64)) * 10_000.0).round() / 100.0
    }
}

pub fn summarize_time_profile_from_xml(
    trace_path: &Path,
    profile_xml: &str,
    thread_info_xml: &str,
    windows: &[TraceWindow],
) -> Result<TimeProfilerSummary> {
    let thread_names = parse_thread_info_names(thread_info_xml)?;
    let rows = parse_time_profile_rows(profile_xml, &thread_names, windows)?;
    let total = rows.len();
    let mut top_threads = BTreeMap::<String, usize>::new();
    let mut thread_name_map = BTreeMap::<String, Option<String>>::new();
    let mut top_frames = BTreeMap::<String, usize>::new();
    let mut buckets = BTreeMap::<String, usize>::new();
    let tokio_named_threads_visible_in_thread_info = thread_names.iter().any(|(thread, name)| {
        thread.contains("oxide-tokio")
            || name.as_deref().unwrap_or_default().contains("oxide-tokio")
    });
    let tokio_named_threads_visible_in_sampled_rows = rows.iter().any(|row| {
        row.thread.contains("oxide-tokio")
            || row.thread_name.as_deref().unwrap_or_default().contains("oxide-tokio")
    });
    for row in &rows {
        *top_threads.entry(row.thread.clone()).or_insert(0) += 1;
        thread_name_map.entry(row.thread.clone()).or_insert_with(|| row.thread_name.clone());
        if let Some(frame) = row.frames.first() {
            *top_frames.entry(frame.clone()).or_insert(0) += 1;
        }
        *buckets.entry(time_profiler_bucket_name(&row.frames).to_string()).or_insert(0) += 1;
    }
    let mut top_threads = top_threads
        .into_iter()
        .map(|(thread, samples)| TimeProfilerThreadSummary {
            thread: thread.clone(),
            thread_name: thread_name_map.get(&thread).cloned().flatten(),
            samples,
            share_pct: time_profiler_share_pct(samples, total),
        })
        .collect::<Vec<_>>();
    top_threads.sort_by(|left, right| {
        right.samples.cmp(&left.samples).then_with(|| left.thread.cmp(&right.thread))
    });

    let mut top_frames = top_frames
        .into_iter()
        .map(|(frame, samples)| TimeProfilerFrameSummary {
            frame,
            samples,
            share_pct: time_profiler_share_pct(samples, total),
        })
        .collect::<Vec<_>>();
    top_frames.sort_by(|left, right| {
        right.samples.cmp(&left.samples).then_with(|| left.frame.cmp(&right.frame))
    });

    let mut bucket_counts = buckets
        .into_iter()
        .map(|(bucket, samples)| TimeProfilerBucketSummary {
            bucket,
            samples,
            share_pct: time_profiler_share_pct(samples, total),
        })
        .collect::<Vec<_>>();
    bucket_counts.sort_by(|left, right| {
        right.samples.cmp(&left.samples).then_with(|| left.bucket.cmp(&right.bucket))
    });

    let mut notes = Vec::new();
    match bucket_counts.first().map(|entry| entry.bucket.as_str()) {
        Some("renderer_driver_present") => notes.push(String::from(
            "The bounded workload is still dominated by drawable/render/driver work rather than camera publication.",
        )),
        Some("sample_delivery") => notes.push(String::from(
            "The bounded workload is now dominated by sample delivery and remote sample-buffer transport rather than camera publication or drawable/present churn.",
        )),
        Some("oxide_camera_bridge") => notes.push(String::from(
            "The bounded workload is still spending a material share of CPU inside the Oxide camera bridge/publication path.",
        )),
        Some("other") => notes.push(String::from(
            "No single named hot path dominated this bounded workload window; the remaining cost is concentrated in uncategorized runtime, framework, or driver work.",
        )),
        Some("benchmark_summary") => notes.push(String::from(
            "Benchmark bookkeeping dominated this bounded workload window, so the sample mix is not representative of shipping preview cost.",
        )),
        _ => {}
    }
    if bucket_counts.iter().any(|entry| entry.bucket == "sample_delivery") {
        notes.push(String::from(
            "Sample-delivery / remote sample-buffer deserialize work is still visible in the bounded window.",
        ));
    }
    if bucket_counts.iter().any(|entry| entry.bucket == "oxide_camera_bridge") {
        notes.push(String::from(
            "The camera bridge/publication path remains measurable, but it is no longer the only place to look for wins.",
        ));
    }
    if bucket_counts.iter().any(|entry| entry.bucket == "renderer_driver_present") {
        notes.push(String::from(
            "Drawable/present/driver stacks are still present in the bounded window and remain worth re-checking after any visible-preview surface change.",
        ));
    }
    if bucket_counts.iter().any(|entry| entry.bucket == "benchmark_summary") {
        notes.push(String::from(
            "Benchmark summary helper work appears in the sampled window because the parked benchmark emits stage summaries before exit; do not treat that bucket as shipping preview cost.",
        ));
    }

    Ok(TimeProfilerSummary {
        trace: trace_path.display().to_string(),
        source: String::from("bounded Time Profiler trace"),
        sample_rows_with_backtraces: total,
        top_threads,
        top_frames,
        bucket_counts,
        worker_thread_naming: TimeProfilerWorkerThreadNaming {
            tokio_named_threads_visible_in_thread_info,
            tokio_named_threads_visible_in_sampled_rows,
            note: String::from(
                "Time Profiler still surfaced sampled Oxide worker threads as generic OxideHost tids instead of oxide-tokio-* names in this export.",
            ),
        },
        notes,
    })
}

pub fn summarize_time_profiler_trace(
    root: &Path,
    trace_path: &Path,
) -> Result<TimeProfilerSummary> {
    let (windows, used_summary_window) =
        extract_trace_windows_or_summary_window(root, trace_path, "OxideHost")?;
    let profile_xml = export_xctrace_preferred_table_xml(root, trace_path, "time-profile", None)?;
    let thread_info_xml =
        export_xctrace_preferred_table_xml(root, trace_path, "thread-info", None)?;
    let mut summary =
        summarize_time_profile_from_xml(trace_path, &profile_xml, &thread_info_xml, &windows)?;
    if used_summary_window {
        summary.notes.push(String::from(
            "Time Profiler window status: workload signposts were unavailable, so the summary fell back to the full trace duration.",
        ));
    }
    Ok(summary)
}

fn column_text(node: &roxmltree::Node<'_, '_>, tag_name: &str) -> String {
    node.children()
        .find(|child| child.is_element() && child.has_tag_name(tag_name))
        .and_then(|child| child.text())
        .unwrap_or_default()
        .to_string()
}

impl XctraceRow {
    fn cell(&self, name: &str) -> Option<&XctraceCell> {
        self.values.get(name)
    }
}

impl XctraceCell {
    fn display(&self) -> Option<&str> {
        self.fmt.as_deref().or(self.raw.as_deref())
    }

    fn raw_f64(&self) -> Option<f64> {
        self.raw.as_deref()?.parse::<f64>().ok()
    }

    fn raw_u64(&self) -> Option<u64> {
        self.raw.as_deref()?.parse::<u64>().ok()
    }
}

fn extract_trace_windows(root: &Path, trace_path: &Path) -> Result<Vec<TraceWindow>> {
    let mut tables = export_xctrace_tables(root, trace_path, "region-of-interest")?;
    tables.extend(export_xctrace_tables(root, trace_path, "os-signpost")?);
    extract_trace_windows_from_tables(&tables)
}

fn extract_trace_windows_or_summary_window(
    root: &Path,
    trace_path: &Path,
    process_name: &str,
) -> Result<(Vec<TraceWindow>, bool)> {
    match extract_trace_windows(root, trace_path) {
        Ok(windows) => Ok((windows, false)),
        Err(_) => Ok((vec![extract_trace_summary_window(root, trace_path, process_name)?], true)),
    }
}

fn summarize_trace_signpost_metrics(
    root: &Path,
    trace_path: &Path,
    windows: &[TraceWindow],
) -> Result<BTreeMap<String, UIKitMetricSummary>> {
    let mut tables = export_xctrace_tables(root, trace_path, "region-of-interest")?;
    tables.extend(export_xctrace_tables(root, trace_path, "os-signpost")?);
    summarize_trace_signpost_metrics_from_tables(&tables, windows)
}

fn extract_trace_summary_window(
    root: &Path,
    trace_path: &Path,
    process_name: &str,
) -> Result<TraceWindow> {
    let text = export_xctrace_toc_text(root, trace_path)?;
    parse_xctrace_summary_window(&text, process_name)
}

pub fn parse_xctrace_summary_window(text: &str, process_name: &str) -> Result<TraceWindow> {
    let doc = Document::parse(text.trim()).with_context(|| "parsing xctrace toc xml")?;
    let duration_text = doc
        .descendants()
        .find(|node| node.has_tag_name("duration"))
        .and_then(|node| node.text())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .with_context(|| "missing xctrace summary duration")?;
    let duration_s = duration_text
        .parse::<f64>()
        .with_context(|| format!("parsing xctrace duration `{}`", duration_text))?;
    if duration_s <= 0.0 {
        bail!("xctrace summary duration must be positive");
    }
    Ok(TraceWindow {
        start_ns: 0,
        end_ns: (duration_s * 1_000_000_000.0).round() as u64,
        process_name: process_name.to_string(),
    })
}

pub fn extract_trace_windows_from_tables(tables: &[XctraceTable]) -> Result<Vec<TraceWindow>> {
    let mut roi_windows = Vec::new();
    for table in tables {
        for row in &table.rows {
            let name = row.cell("name").and_then(XctraceCell::display).unwrap_or_default();
            let subsystem =
                row.cell("subsystem").and_then(XctraceCell::display).unwrap_or_default();
            if name != UIKIT_PERF_SIGNPOST_NAME || subsystem != UIKIT_PERF_SIGNPOST_SUBSYSTEM {
                continue;
            }
            let Some(start_ns) = row.cell("start").and_then(XctraceCell::raw_u64) else {
                continue;
            };
            let Some(duration_ns) = row.cell("duration").and_then(XctraceCell::raw_u64) else {
                continue;
            };
            if duration_ns == 0 {
                continue;
            }
            let process_name = normalize_process_name(
                row.cell("process")
                    .or_else(|| row.cell("start-process"))
                    .and_then(XctraceCell::display)
                    .unwrap_or_default(),
            );
            roi_windows.push(TraceWindow {
                start_ns,
                end_ns: start_ns.saturating_add(duration_ns),
                process_name,
            });
        }
    }
    if !roi_windows.is_empty() {
        roi_windows.sort_by(|left, right| {
            left.start_ns
                .cmp(&right.start_ns)
                .then_with(|| left.end_ns.cmp(&right.end_ns))
                .then_with(|| left.process_name.cmp(&right.process_name))
        });
        return Ok(roi_windows);
    }

    struct SignpostEvent {
        time_ns: u64,
        process_name: String,
        identifier: String,
        is_begin: bool,
    }

    let mut windows = Vec::new();
    let mut events = Vec::new();
    let mut open = BTreeMap::<(String, String), Vec<u64>>::new();
    for table in tables {
        for row in &table.rows {
            let subsystem =
                row.cell("subsystem").and_then(XctraceCell::display).unwrap_or_default();
            let category = row.cell("category").and_then(XctraceCell::display).unwrap_or_default();
            let name = row.cell("name").and_then(XctraceCell::display).unwrap_or_default();
            if subsystem != UIKIT_PERF_SIGNPOST_SUBSYSTEM
                || !xctrace_category_matches_preferred(category, UIKIT_PERF_SIGNPOST_CATEGORY)
                || name != UIKIT_PERF_SIGNPOST_NAME
            {
                continue;
            }
            let time_ns = row
                .cell("time")
                .and_then(XctraceCell::raw_u64)
                .with_context(|| "missing signpost timestamp")?;
            let event_type =
                row.cell("event-type").and_then(XctraceCell::display).unwrap_or_default();
            let is_begin = event_type.contains("Begin");
            let is_end = event_type.contains("End");
            if !is_begin && !is_end {
                continue;
            }
            let process_name = normalize_process_name(
                row.cell("process").and_then(XctraceCell::display).unwrap_or_default(),
            );
            let identifier = row
                .cell("identifier")
                .and_then(XctraceCell::display)
                .unwrap_or("default")
                .to_string();
            events.push(SignpostEvent { time_ns, process_name, identifier, is_begin });
        }
    }
    if events.is_empty() {
        bail!("missing UIKit device trace signposts");
    }

    events.sort_by(|left, right| {
        left.time_ns
            .cmp(&right.time_ns)
            .then_with(|| right.is_begin.cmp(&left.is_begin))
            .then_with(|| left.process_name.cmp(&right.process_name))
            .then_with(|| left.identifier.cmp(&right.identifier))
    });

    for event in events {
        let key = (event.process_name.clone(), event.identifier);
        if event.is_begin {
            open.entry(key).or_default().push(event.time_ns);
        } else {
            let Some(starts) = open.get_mut(&key) else {
                continue;
            };
            let Some(start_ns) = starts.pop() else {
                continue;
            };
            windows.push(TraceWindow {
                start_ns,
                end_ns: event.time_ns,
                process_name: event.process_name,
            });
        }
    }

    if windows.is_empty() {
        bail!("missing complete UIKit device trace signpost windows");
    }
    windows.sort_by(|left, right| left.start_ns.cmp(&right.start_ns));
    Ok(windows)
}

pub fn summarize_trace_signpost_metrics_from_tables(
    tables: &[XctraceTable],
    windows: &[TraceWindow],
) -> Result<BTreeMap<String, UIKitMetricSummary>> {
    let roi_metrics = summarize_trace_signpost_metrics_from_roi_tables(tables, windows)?;
    if !roi_metrics.is_empty() {
        return Ok(roi_metrics);
    }

    struct SignpostEvent {
        time_ns: u64,
        process_name: String,
        identifier: String,
        name: String,
        is_begin: bool,
    }

    let mut events = Vec::new();
    for table in tables {
        for row in &table.rows {
            let subsystem =
                row.cell("subsystem").and_then(XctraceCell::display).unwrap_or_default();
            let category = row.cell("category").and_then(XctraceCell::display).unwrap_or_default();
            if subsystem != UIKIT_PERF_SIGNPOST_SUBSYSTEM
                || !xctrace_category_matches_preferred(category, UIKIT_PERF_SIGNPOST_CATEGORY)
            {
                continue;
            }
            let name = row.cell("name").and_then(XctraceCell::display).unwrap_or_default();
            if name.is_empty() || name == UIKIT_PERF_SIGNPOST_NAME {
                continue;
            }
            let time_ns = row
                .cell("time")
                .and_then(XctraceCell::raw_u64)
                .with_context(|| "missing signpost metric timestamp")?;
            let event_type =
                row.cell("event-type").and_then(XctraceCell::display).unwrap_or_default();
            let is_begin = event_type.contains("Begin");
            let is_end = event_type.contains("End");
            if !is_begin && !is_end {
                continue;
            }
            let process_name = normalize_process_name(
                row.cell("process").and_then(XctraceCell::display).unwrap_or_default(),
            );
            let identifier = row
                .cell("identifier")
                .and_then(XctraceCell::display)
                .unwrap_or("default")
                .to_string();
            events.push(SignpostEvent {
                time_ns,
                process_name,
                identifier,
                name: name.to_string(),
                is_begin,
            });
        }
    }

    if events.is_empty() {
        return Ok(BTreeMap::new());
    }

    events.sort_by(|left, right| {
        left.time_ns
            .cmp(&right.time_ns)
            .then_with(|| right.is_begin.cmp(&left.is_begin))
            .then_with(|| left.process_name.cmp(&right.process_name))
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.identifier.cmp(&right.identifier))
    });

    let mut open = BTreeMap::<(String, String, String), Vec<u64>>::new();
    let mut samples = BTreeMap::<String, Vec<f64>>::new();
    for event in events {
        let key = (event.process_name.clone(), event.name.clone(), event.identifier);
        if event.is_begin {
            open.entry(key).or_default().push(event.time_ns);
            continue;
        }
        let Some(starts) = open.get_mut(&key) else {
            continue;
        };
        let Some(start_ns) = starts.pop() else {
            continue;
        };
        let overlapped_ns = windows
            .iter()
            .filter(|window| window.process_name == event.process_name)
            .map(|window| {
                overlapping_duration_ns(start_ns, event.time_ns, window.start_ns, window.end_ns)
            })
            .sum::<u64>();
        if overlapped_ns == 0 {
            continue;
        }
        let metric_name = format!("signpost_{}_s", sanitize_metric_name(&event.name));
        samples.entry(metric_name).or_default().push((overlapped_ns as f64) / 1_000_000_000.0);
    }

    let mut metrics = BTreeMap::new();
    for (name, values) in samples {
        metrics.insert(name, metric_summary_from_samples("s", &values)?);
    }
    Ok(metrics)
}

fn summarize_trace_signpost_metrics_from_roi_tables(
    tables: &[XctraceTable],
    windows: &[TraceWindow],
) -> Result<BTreeMap<String, UIKitMetricSummary>> {
    let mut samples = BTreeMap::<String, Vec<f64>>::new();
    for table in tables {
        let has_interval_columns = table.columns.iter().any(|column| column.mnemonic == "start")
            && table.columns.iter().any(|column| column.mnemonic == "duration")
            && table.columns.iter().any(|column| column.mnemonic == "name");
        if !has_interval_columns {
            continue;
        }
        for row in &table.rows {
            let subsystem =
                row.cell("subsystem").and_then(XctraceCell::display).unwrap_or_default();
            if subsystem != UIKIT_PERF_SIGNPOST_SUBSYSTEM {
                continue;
            }
            let name = row.cell("name").and_then(XctraceCell::display).unwrap_or_default();
            if name.is_empty() || name == UIKIT_PERF_SIGNPOST_NAME {
                continue;
            }
            let Some(start_ns) = row.cell("start").and_then(XctraceCell::raw_u64) else {
                continue;
            };
            let Some(duration_ns) = row.cell("duration").and_then(XctraceCell::raw_u64) else {
                continue;
            };
            if duration_ns == 0 {
                continue;
            }
            let process_name = normalize_process_name(
                row.cell("process")
                    .or_else(|| row.cell("start-process"))
                    .and_then(XctraceCell::display)
                    .unwrap_or_default(),
            );
            let end_ns = start_ns.saturating_add(duration_ns);
            let overlapped_ns = windows
                .iter()
                .filter(|window| window.process_name == process_name)
                .map(|window| {
                    overlapping_duration_ns(start_ns, end_ns, window.start_ns, window.end_ns)
                })
                .sum::<u64>();
            if overlapped_ns == 0 {
                continue;
            }
            let metric_name = format!("signpost_{}_s", sanitize_metric_name(name));
            samples.entry(metric_name).or_default().push((overlapped_ns as f64) / 1_000_000_000.0);
        }
    }

    let mut metrics = BTreeMap::new();
    for (name, values) in samples {
        metrics.insert(name, metric_summary_from_samples("s", &values)?);
    }
    Ok(metrics)
}

fn summarize_device_gpu_metrics(
    root: &Path,
    trace_path: &Path,
    windows: &[TraceWindow],
    notes: &mut Vec<String>,
) -> Result<BTreeMap<String, UIKitMetricSummary>> {
    let table = export_xctrace_preferred_table(root, trace_path, "metal-gpu-intervals", None)?;
    let mut metrics = summarize_device_gpu_metrics_from_tables(&table, windows, notes)?;
    let counter_info = export_xctrace_preferred_table(root, trace_path, "gpu-counter-info", None);
    let counter_values =
        export_xctrace_preferred_table(root, trace_path, "gpu-counter-value", None);
    let (Ok(counter_info), Ok(counter_values)) = (counter_info, counter_values) else {
        notes.push(String::from(
            "GPU counter status: direct GPU counters were unavailable in this device trace; GPU time and GPU latency remained available from Metal System Trace.",
        ));
        return Ok(metrics);
    };
    let mut counter_names = BTreeMap::new();
    for row in &counter_info.rows {
        let Some(counter_id) = row.cell("counter-id").and_then(XctraceCell::raw_u64) else {
            continue;
        };
        let Some(name) = row.cell("name").and_then(XctraceCell::display) else {
            continue;
        };
        counter_names.insert(counter_id, sanitize_metric_name(name));
    }

    let mut counter_samples = BTreeMap::<String, Vec<f64>>::new();
    for window in windows {
        let mut interval_samples = BTreeMap::<String, Vec<f64>>::new();
        for row in &counter_values.rows {
            let Some(timestamp) = row.cell("timestamp").and_then(XctraceCell::raw_u64) else {
                continue;
            };
            if timestamp < window.start_ns || timestamp > window.end_ns {
                continue;
            }
            let Some(counter_id) = row.cell("counter-id").and_then(XctraceCell::raw_u64) else {
                continue;
            };
            let Some(counter_name) = counter_names.get(&counter_id) else {
                continue;
            };
            let Some(value) = row.cell("value").and_then(XctraceCell::raw_f64) else {
                continue;
            };
            interval_samples.entry(counter_name.clone()).or_default().push(value);
        }
        for (counter_name, values) in interval_samples {
            let mean = values.iter().sum::<f64>() / values.len() as f64;
            counter_samples.entry(counter_name).or_default().push(mean);
        }
    }
    if counter_samples.is_empty() {
        notes.push(String::from(
            "GPU counter status: the trace exposed GPU counter tables, but there were no direct counter samples inside the bounded workload window; GPU time and GPU latency remained available from Metal System Trace.",
        ));
        return Ok(metrics);
    }
    for (counter_name, values) in counter_samples {
        metrics.insert(
            format!("gpu_counter.{}", counter_name),
            metric_summary_from_samples("count", &values)?,
        );
    }
    Ok(metrics)
}

pub fn summarize_device_gpu_metrics_from_tables(
    table: &XctraceTable,
    windows: &[TraceWindow],
    notes: &mut Vec<String>,
) -> Result<BTreeMap<String, UIKitMetricSummary>> {
    let mut gpu_samples = Vec::with_capacity(windows.len());
    let mut latency_samples = Vec::with_capacity(windows.len());
    let mut used_compositor_fallback = false;
    let mut compositor_process_totals = BTreeMap::<String, (f64, usize)>::new();
    let mut compositor_window_count = 0usize;

    let summarize_window = |window: &TraceWindow,
                            filter_process: Option<&str>|
     -> (f64, f64, usize, BTreeMap<String, (f64, usize)>) {
        let mut total_ns = 0.0;
        let mut latency_ns = 0.0;
        let mut matched_rows = 0;
        let mut process_totals = BTreeMap::<String, (f64, usize)>::new();
        for row in &table.rows {
            let process_name = normalize_process_name(
                row.cell("process").and_then(XctraceCell::display).unwrap_or_default(),
            );
            if let Some(expected_process) = filter_process {
                if process_name != expected_process {
                    continue;
                }
            }
            let start_ns = row.cell("start").and_then(XctraceCell::raw_u64).unwrap_or(0);
            let duration_ns = row.cell("duration").and_then(XctraceCell::raw_u64).unwrap_or(0);
            let overlap_ns = overlapping_duration_ns(
                start_ns,
                start_ns.saturating_add(duration_ns),
                window.start_ns,
                window.end_ns,
            );
            if overlap_ns == 0 {
                continue;
            }
            matched_rows += 1;
            total_ns += overlap_ns as f64;
            latency_ns +=
                row.cell("start-latency").and_then(XctraceCell::raw_u64).unwrap_or(0) as f64;
            if filter_process.is_none() {
                let entry = process_totals.entry(process_name).or_insert((0.0, 0usize));
                entry.0 += overlap_ns as f64;
                entry.1 += 1;
            }
        }
        (total_ns, latency_ns, matched_rows, process_totals)
    };

    for window in windows {
        let (mut total_ns, mut latency_total_ns, mut matched_rows, _) =
            summarize_window(window, Some(window.process_name.as_str()));
        if matched_rows == 0 {
            let compositor_total = summarize_window(window, None);
            if compositor_total.2 > 0 {
                total_ns = compositor_total.0;
                latency_total_ns = compositor_total.1;
                matched_rows = compositor_total.2;
                used_compositor_fallback = true;
                compositor_window_count += 1;
                for (process_name, (duration_ns, row_count)) in compositor_total.3 {
                    let entry =
                        compositor_process_totals.entry(process_name).or_insert((0.0, 0usize));
                    entry.0 += duration_ns;
                    entry.1 += row_count;
                }
            }
        }
        gpu_samples.push(total_ns / 1_000_000_000.0);
        latency_samples.push(if matched_rows == 0 {
            0.0
        } else {
            (latency_total_ns / matched_rows as f64) / 1_000_000_000.0
        });
    }

    let mut metrics = BTreeMap::new();
    metrics.insert(String::from("gpu_time_s"), metric_summary_from_samples("s", &gpu_samples)?);
    metrics
        .insert(String::from("gpu_latency_s"), metric_summary_from_samples("s", &latency_samples)?);
    if used_compositor_fallback {
        notes.push(String::from(
            "GPU interval attribution status: this trace exposed no direct target-process GPU intervals, so GPU time and latency were summarized from compositor-inclusive Metal GPU intervals within the same trace window.",
        ));
        if let Some((process_name, (duration_ns, row_count))) =
            compositor_process_totals.iter().max_by(|left, right| left.1 .0.total_cmp(&right.1 .0))
        {
            notes.push(format!(
                "GPU compositor detail: the compositor-inclusive overlap was dominated by `{}` ({} rows, {:.3} ms total across {} workload window(s)).",
                process_name,
                row_count,
                duration_ns / 1_000_000.0,
                compositor_window_count,
            ));
        }
    }
    notes.push(String::from(
        "GPU metric semantics: `gpu_time_s` is the total overlapping Metal GPU execution duration inside each bounded workload window. `gpu_latency_s` is the mean CPU-to-GPU latency across overlapping Metal GPU intervals inside that same window.",
    ));
    Ok(metrics)
}

fn summarize_device_energy_metric(
    root: &Path,
    trace_path: &Path,
    windows: &[TraceWindow],
) -> Result<UIKitMetricSummary> {
    let toc = export_xctrace_toc(root, trace_path)?;
    let mut candidates = toc
        .into_iter()
        .map(|table| table.schema)
        .filter(|schema| schema.contains("power") || schema.contains("energy"))
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.dedup();

    let mut best = None::<(usize, UIKitMetricSummary)>;
    for schema in candidates {
        for table in export_xctrace_tables(root, trace_path, &schema)? {
            if table.rows.is_empty() {
                continue;
            }
            let Some((score, metric)) = summarize_energy_from_table(&table, windows) else {
                continue;
            };
            if best.as_ref().map(|(best_score, _)| score > *best_score).unwrap_or(true) {
                best = Some((score, metric));
            }
        }
    }

    best.map(|(_, metric)| metric).with_context(|| {
        format!("could not find a direct device energy table in {}", trace_path.display())
    })
}

fn summarize_energy_from_table(
    table: &XctraceTable,
    windows: &[TraceWindow],
) -> Option<(usize, UIKitMetricSummary)> {
    let time_column = find_time_column(table)?;
    let duration_column = find_duration_column(table);
    let process_column = find_process_column(table);
    let energy_column = find_energy_column(table);
    let power_column = find_power_column(table);
    if energy_column.is_none() && power_column.is_none() {
        return None;
    }

    let mut samples = Vec::with_capacity(windows.len());
    for window in windows {
        let mut joules = 0.0;
        for row in &table.rows {
            if let Some(process_column) = process_column.as_deref() {
                let process_name = normalize_process_name(
                    row.cell(process_column).and_then(XctraceCell::display).unwrap_or_default(),
                );
                if process_name != window.process_name {
                    continue;
                }
            }
            let start_ns = row.cell(&time_column).and_then(XctraceCell::raw_u64).unwrap_or(0);
            let duration_ns = duration_column
                .as_deref()
                .and_then(|column| row.cell(column))
                .and_then(XctraceCell::raw_u64)
                .unwrap_or(0);
            let overlap_ns = if duration_ns > 0 {
                overlapping_duration_ns(
                    start_ns,
                    start_ns.saturating_add(duration_ns),
                    window.start_ns,
                    window.end_ns,
                )
            } else if start_ns >= window.start_ns && start_ns <= window.end_ns {
                1
            } else {
                0
            };
            if overlap_ns == 0 {
                continue;
            }
            if let Some(energy_column) = energy_column.as_deref() {
                if let Some(energy_j) =
                    row.cell(energy_column).and_then(|cell| display_value_to_base(cell, "J"))
                {
                    let scale =
                        if duration_ns > 0 { overlap_ns as f64 / duration_ns as f64 } else { 1.0 };
                    joules += energy_j * scale;
                    continue;
                }
            }
            if let Some(power_column) = power_column.as_deref() {
                if let Some(power_w) =
                    row.cell(power_column).and_then(|cell| display_value_to_base(cell, "W"))
                {
                    joules += power_w * (overlap_ns as f64 / 1_000_000_000.0);
                }
            }
        }
        samples.push(joules);
    }
    if samples.iter().all(|value| *value == 0.0) {
        return None;
    }
    let mut score = 0usize;
    if process_column.is_some() {
        score += 2;
    }
    if energy_column.is_some() {
        score += 2;
    }
    if duration_column.is_some() {
        score += 1;
    }
    Some((score, metric_summary_from_samples("J", &samples).ok()?))
}

pub fn summarize_energy_table(
    table: &XctraceTable,
    windows: &[TraceWindow],
) -> Result<UIKitMetricSummary> {
    summarize_energy_from_table(table, windows)
        .map(|(_, metric)| metric)
        .with_context(|| "could not summarize direct energy samples from xctrace table")
}

fn find_time_column(table: &XctraceTable) -> Option<String> {
    table
        .columns
        .iter()
        .find(|column| {
            column.engineering_type.contains("time")
                || column.mnemonic == "start"
                || column.mnemonic == "timestamp"
                || column.mnemonic == "time"
        })
        .map(|column| column.mnemonic.clone())
}

fn find_duration_column(table: &XctraceTable) -> Option<String> {
    table
        .columns
        .iter()
        .find(|column| {
            column.engineering_type.contains("duration") || column.mnemonic == "duration"
        })
        .map(|column| column.mnemonic.clone())
}

fn find_process_column(table: &XctraceTable) -> Option<String> {
    table
        .columns
        .iter()
        .find(|column| column.engineering_type.contains("process") || column.mnemonic == "process")
        .map(|column| column.mnemonic.clone())
}

fn find_energy_column(table: &XctraceTable) -> Option<String> {
    table
        .columns
        .iter()
        .find(|column| {
            let lowered = format!("{} {}", column.mnemonic, column.name).to_lowercase();
            lowered.contains("energy")
                || table.rows.iter().any(|row| {
                    row.cell(&column.mnemonic)
                        .and_then(|cell| cell.display())
                        .map(|display| display.contains(" J") || display.ends_with("J"))
                        .unwrap_or(false)
                })
        })
        .map(|column| column.mnemonic.clone())
}

fn find_power_column(table: &XctraceTable) -> Option<String> {
    table
        .columns
        .iter()
        .find(|column| {
            let lowered = format!("{} {}", column.mnemonic, column.name).to_lowercase();
            lowered.contains("power")
                || table.rows.iter().any(|row| {
                    row.cell(&column.mnemonic)
                        .and_then(|cell| cell.display())
                        .map(|display| display.contains(" W") || display.ends_with("W"))
                        .unwrap_or(false)
                })
        })
        .map(|column| column.mnemonic.clone())
}

fn metric_summary_from_samples(unit: &str, values: &[f64]) -> Result<UIKitMetricSummary> {
    if values.is_empty() {
        bail!("metric `{}` had no measurements", unit);
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let samples = sorted.len();
    let mean = sorted.iter().sum::<f64>() / samples as f64;
    let median = percentile(&sorted, 0.50);
    let p95 = percentile(&sorted, 0.95);
    let p99 = percentile(&sorted, 0.99);
    Ok(UIKitMetricSummary {
        unit: unit.to_string(),
        min: *sorted.first().unwrap_or(&0.0),
        max: *sorted.last().unwrap_or(&0.0),
        mean,
        median,
        p95,
        p99,
        samples,
    })
}

fn overlapping_duration_ns(start_a: u64, end_a: u64, start_b: u64, end_b: u64) -> u64 {
    let start = start_a.max(start_b);
    let end = end_a.min(end_b);
    end.saturating_sub(start)
}

fn normalize_process_name(display: &str) -> String {
    display
        .rsplit_once(" (")
        .map(|(name, _)| name.to_string())
        .unwrap_or_else(|| display.to_string())
}

fn uikit_case_contract_metadata(
    case_id: &str,
) -> (&'static str, &'static str, &'static str, &'static str) {
    let style = if case_id.contains(".idiomatic.") {
        "idiomatic"
    } else if case_id.contains(".optimized.") {
        "optimized"
    } else {
        "idiomatic"
    };
    if case_id.contains(".launch.") {
        let cache_state =
            if case_id.contains(".cold_launch") || case_id.contains(".deep_link_launch") {
                "cold"
            } else {
                "warm"
            };
        return ("flow", "launch-lifecycle", style, cache_state);
    }
    if case_id.contains(".journey.") {
        return ("flow", "screen-flow", style, "warm");
    }
    if case_id.contains(".bridge.") {
        return ("bridge", "os-bridge", style, "warm");
    }
    if case_id.contains(".layout.") {
        return ("engine", "layout-invalidation", style, "warm");
    }
    if case_id.contains(".text_input.") {
        return ("engine", "text-input", style, "warm");
    }
    if case_id.contains(".image_pipeline.") {
        let cache_state = if case_id.contains(".decode") { "cold" } else { "warm" };
        return ("engine", "image-pipeline", style, cache_state);
    }
    if case_id.contains(".navigation.") {
        return ("flow", "navigation-input", style, "warm");
    }
    if case_id.contains(".reconcile.") {
        return ("engine", "state-reconcile", style, "warm");
    }
    if case_id.contains(".endurance.") {
        return ("flow", "endurance-thermal", style, "warm");
    }
    if case_id.contains(".stress.") {
        return ("engine", "stress-pathological", style, "warm");
    }
    if case_id.contains(".primitive.") {
        return ("engine", "primitive-lifecycle", style, "warm");
    }
    if case_id.contains(".authoring.") {
        return ("engine", "authoring", style, "warm");
    }
    if case_id.contains(".animation.") {
        return ("engine", "animation-effect", style, "warm");
    }
    if case_id.contains(".component.") {
        return ("engine", "primitive-view", style, "warm");
    }
    ("engine", "uncategorized", style, "warm")
}

fn uikit_refresh_mode_for_suite(suite: &str) -> &'static str {
    match suite {
        "device" => "device-default",
        _ => "simulator-default",
    }
}

fn build_uikit_contract_coverage(
    cases: &[UIKitPerfCase],
    suite: &str,
) -> UIKitContractCoverageReport {
    let has = |needle: &str| cases.iter().any(|case| case.id.contains(needle));
    let has_case = |id: &str| cases.iter().any(|case| case.id == id);
    let has_style = |style: &str| cases.iter().any(|case| case.style == style);
    let layers = vec![
        uikit_contract_entry(
            "engine",
            "Engine Microbenchmarks",
            if has(".component.") || has(".animation.") || has(".primitive.") {
                "implemented"
            } else {
                "missing"
            },
            vec![String::from(
                "UIKit engine coverage currently spans primitive views, animation effects, and primitive lifecycle slices.",
            )],
        ),
        uikit_contract_entry(
            "flow",
            "Representative Screen Flows",
            if has(".journey.") { "implemented" } else { "missing" },
            vec![String::from(
                "Flow coverage now spans launch/lifecycle and user-journey cases, but hitch and refresh-mode matrices are still incomplete.",
            )],
        ),
        uikit_contract_entry(
            "os-bridge",
            "OS-Bridge Benchmarks",
            if has(".bridge.") { "implemented" } else { "missing" },
            vec![String::from(
                "Bridge coverage measures app-owned wrapper overhead separately from system-owned UI surfaces.",
            )],
        ),
    ];
    let styles = vec![
        uikit_contract_entry(
            "idiomatic",
            "Idiomatic UIKit",
            if has_style("idiomatic") { "implemented" } else { "missing" },
            vec![String::from(
                "Idiomatic retained-view parity is the default UIKit baseline in this suite.",
            )],
        ),
        uikit_contract_entry(
            "optimized",
            "Hand-Optimized UIKit",
            if has_style("optimized") { "partial" } else { "missing" },
            vec![String::from(
                "The optimized UIKit slice now covers the full currently implemented journey, bridge, and endurance families, plus primitive-lifecycle, animation-effect, image-pipeline, and large-editor text-input peers; launch/lifecycle, layout/invalidation, authoring, component microbenchmarks, and stress/pathological traps still need tuned peers.",
            )],
        ),
    ];
    let battery = vec![
        uikit_contract_entry(
            "launch-lifecycle",
            "Launch & Lifecycle",
            if has_case("uikit.idiomatic.launch.simple_home.cold_launch")
                && has_case("uikit.idiomatic.launch.heavy_home.cold_launch")
                && has_case("uikit.idiomatic.launch.detail.deep_link_launch")
                && has_case("uikit.idiomatic.launch.simple_home.warm_resume")
                && has_case("uikit.idiomatic.launch.heavy_home.foreground_after_background")
            {
                "implemented"
            } else {
                "partial"
            },
            vec![if has_case("uikit.idiomatic.launch.simple_home.cold_launch")
                && has_case("uikit.idiomatic.launch.heavy_home.cold_launch")
                && has_case("uikit.idiomatic.launch.detail.deep_link_launch")
                && has_case("uikit.idiomatic.launch.simple_home.warm_resume")
                && has_case("uikit.idiomatic.launch.heavy_home.foreground_after_background")
            {
                String::from(
                    "The XCTest harness now runs simple-home and heavy-home cold launch, detail-route launch, warm resume, and foreground-after-background batteries, using XCTApplicationLaunchMetric on the cold launch cases.",
                )
            } else {
                String::from(
                    "The current XCTest harness does not yet run a dedicated launch/resume/deep-link battery with XCTApplicationLaunchMetric.",
                )
            }],
        ),
        uikit_contract_entry(
            "primitive-lifecycle",
            "Primitive Mount / Update / Destroy",
            if has_case("uikit.idiomatic.primitive.empty_root.mount")
                && has_case("uikit.idiomatic.primitive.control_set.mount")
                && has_case("uikit.idiomatic.primitive.control_set.mutate_state")
                && has_case("uikit.idiomatic.primitive.flat_rects.100.remove_all")
                && has_case("uikit.idiomatic.primitive.flat_rects.100.remount")
            {
                "implemented"
            } else {
                "partial"
            },
            vec![if has_case("uikit.idiomatic.primitive.empty_root.mount")
                && has_case("uikit.idiomatic.primitive.control_set.mount")
                && has_case("uikit.idiomatic.primitive.control_set.mutate_state")
                && has_case("uikit.idiomatic.primitive.flat_rects.100.remove_all")
                && has_case("uikit.idiomatic.primitive.flat_rects.100.remount")
            {
                String::from(
                    "Flat rects, labels, cards, images, an empty-root slice, a shared control-set slice, and retained-view remove-all/remount slices are all covered.",
                )
            } else {
                String::from(
                    "Flat rects, labels, cards, and images cover mount plus mutate; the empty-root, shared control-set, and retained-view remove-all/remount slices are still incomplete.",
                )
            }],
        ),
        uikit_contract_entry(
            "layout-invalidation",
            "Layout & Invalidation",
            if has_case("uikit.idiomatic.layout.flat_grid.rotation_relayout")
                && has_case("uikit.idiomatic.layout.deep_stack.theme_swap")
                && has_case("uikit.idiomatic.layout.grid.safe_area_swap")
            {
                "implemented"
            } else {
                "partial"
            },
            vec![if has_case("uikit.idiomatic.layout.flat_grid.rotation_relayout")
                && has_case("uikit.idiomatic.layout.deep_stack.theme_swap")
                && has_case("uikit.idiomatic.layout.grid.safe_area_swap")
            {
                String::from(
                    "Flat-grid rotation, deep-stack theme swap, and safe-area inset relayout batteries are all implemented.",
                )
            } else {
                String::from(
                    "Dedicated relayout batteries now exist, but not every required flat/deep/grid invalidation slice is present yet.",
                )
            }],
        ),
        uikit_contract_entry(
            "text-input",
            "Text & Text Input",
            if has_case("uikit.idiomatic.text_input.large_editor.keystroke_burst")
                && has_case("uikit.idiomatic.text_input.large_editor.paste_10kb")
                && has_case("uikit.idiomatic.text_input.large_editor.selection_replace")
            {
                "implemented"
            } else {
                "partial"
            },
            vec![if has_case("uikit.idiomatic.text_input.large_editor.keystroke_burst")
                && has_case("uikit.idiomatic.text_input.large_editor.paste_10kb")
                && has_case("uikit.idiomatic.text_input.large_editor.selection_replace")
            {
                String::from(
                    "Large-editor keystroke, paste, and selection-replace workloads now complement the existing UILabel and form-journey coverage.",
                )
            } else {
                String::from(
                    "UILabel parity and the input-form journey exist, but the full large-editor typing, paste, and selection battery is still incomplete.",
                )
            }],
        ),
        uikit_contract_entry(
            "image-pipeline",
            "Image Pipeline",
            if has_case("uikit.idiomatic.image_pipeline.png.decode")
                && has_case("uikit.idiomatic.image_pipeline.png.upload")
                && has_case("uikit.idiomatic.image_pipeline.png.first_visible")
            {
                "implemented"
            } else {
                "partial"
            },
            vec![if has_case("uikit.idiomatic.image_pipeline.png.decode")
                && has_case("uikit.idiomatic.image_pipeline.png.upload")
                && has_case("uikit.idiomatic.image_pipeline.png.first_visible")
            {
                String::from(
                    "The committed UIKit image battery now splits PNG decode, upload/attach, and first-visible phases into separate persisted workloads.",
                )
            } else {
                String::from(
                    "UIImageView and zoom workloads exist, but bytes-ready, decode, upload, and first-visible phases are not yet split into separate metrics.",
                )
            }],
        ),
        uikit_contract_entry(
            "lists-grids-chat",
            "Lists, Grids, & Chat",
            if has_case("uikit.journey.feed_scroll_matrix")
                && has_case("uikit.journey.thumbnail_grid_scroll_matrix")
                && has_case("uikit.journey.chat_thread_scroll_matrix")
            {
                "implemented"
            } else {
                "partial"
            },
            vec![if has_case("uikit.journey.feed_scroll_matrix")
                && has_case("uikit.journey.thumbnail_grid_scroll_matrix")
                && has_case("uikit.journey.chat_thread_scroll_matrix")
            {
                String::from(
                    "Feed, thumbnail-grid, and chat-thread scroll matrices now exist alongside collection encode and navigation slices.",
                )
            } else {
                String::from(
                    "Collection-view encode and collection-navigation journey coverage exist, but the full feed/grid/chat scroll matrices are still incomplete.",
                )
            }],
        ),
        uikit_contract_entry(
            "navigation-input",
            "Navigation & Input Latency",
            if has_case("uikit.idiomatic.navigation.button_press.response")
                && has_case("uikit.idiomatic.navigation.slider_scrub.response")
                && has_case("uikit.idiomatic.navigation.text_focus.response")
            {
                "implemented"
            } else {
                "partial"
            },
            vec![if has_case("uikit.idiomatic.navigation.button_press.response")
                && has_case("uikit.idiomatic.navigation.slider_scrub.response")
                && has_case("uikit.idiomatic.navigation.text_focus.response")
            {
                String::from(
                    "Direct button-press, slider-scrub, and text-focus response batteries now complement the higher-level journey cases.",
                )
            } else {
                String::from(
                    "Navigation, orchestration, and zoom journeys exist, but direct input-event-to-response batteries are still missing.",
                )
            }],
        ),
        uikit_contract_entry(
            "animation-effects",
            "Animation & Visual Effects",
            "partial",
            vec![String::from(
                "Idiomatic and hand-tuned animation-effect cases now exist, but the suite still lacks full hitch-ratio and refresh-mode matrices on real 60 Hz and ProMotion hardware.",
            )],
        ),
        uikit_contract_entry(
            "state-reconcile",
            "State Mutation & Reconciliation",
            if has_case("uikit.idiomatic.reconcile.single_node_mutation")
                && has_case("uikit.idiomatic.reconcile.tree_mutation_1pct")
                && has_case("uikit.idiomatic.reconcile.tree_mutation_10pct")
                && has_case("uikit.idiomatic.reconcile.theme_swap_full")
            {
                "implemented"
            } else {
                "partial"
            },
            vec![if has_case("uikit.idiomatic.reconcile.single_node_mutation")
                && has_case("uikit.idiomatic.reconcile.tree_mutation_1pct")
                && has_case("uikit.idiomatic.reconcile.tree_mutation_10pct")
                && has_case("uikit.idiomatic.reconcile.theme_swap_full")
            {
                String::from(
                    "Single-node, 1 percent, 10 percent, and full-theme tree mutation batteries now expose diff/apply cost directly.",
                )
            } else {
                String::from(
                    "Primitive mutate and orchestration workloads exist, but explicit diff/apply batteries for tree mutation rates and theme swaps are still missing.",
                )
            }],
        ),
        uikit_contract_entry(
            "os-bridge",
            "OS Bridge Overhead",
            if has_case("uikit.bridge.permission_callback_fanout")
                && has_case("uikit.bridge.sensor_location_snapshot")
                && has_case("uikit.bridge.bluetooth_cache_update")
                && has_case("uikit.bridge.photo_import_thumbnail")
                && has_case("uikit.bridge.file_import_render")
                && has_case("uikit.bridge.share_payload_prepare")
                && has_case("uikit.bridge.local_json_transport_render")
                && has_case("uikit.bridge.local_image_transport_render")
            {
                "implemented"
            } else {
                "partial"
            },
            vec![if has_case("uikit.bridge.permission_callback_fanout")
                && has_case("uikit.bridge.sensor_location_snapshot")
                && has_case("uikit.bridge.bluetooth_cache_update")
                && has_case("uikit.bridge.photo_import_thumbnail")
                && has_case("uikit.bridge.file_import_render")
                && has_case("uikit.bridge.share_payload_prepare")
                && has_case("uikit.bridge.local_json_transport_render")
                && has_case("uikit.bridge.local_image_transport_render")
            {
                String::from(
                    "Permission, sensor, photo import, file import, share payload, and localhost transport/render bridge workloads are all covered without claiming system-owned UI as a renderer win.",
                )
            } else {
                String::from(
                    "Permission, location, and Bluetooth wrapper overhead is covered, but photo import, file import, share sheet, and transport/decode/render bridge batteries remain missing.",
                )
            }],
        ),
        uikit_contract_entry(
            "endurance-thermal",
            "Endurance, Memory, & Thermal Drift",
            if has_case("uikit.idiomatic.endurance.open_close_heavy_screen.100x")
                && has_case("uikit.idiomatic.endurance.tab_switch_heavy.500x")
                && has_case("uikit.idiomatic.endurance.idle_animation.600_frames")
            {
                "implemented"
            } else {
                "partial"
            },
            vec![if has_case("uikit.idiomatic.endurance.open_close_heavy_screen.100x")
                && has_case("uikit.idiomatic.endurance.tab_switch_heavy.500x")
                && has_case("uikit.idiomatic.endurance.idle_animation.600_frames")
            {
                String::from(
                    "Open/close, tab-switch, and idle-animation endurance loops are now part of the committed UIKit battery.",
                )
            } else {
                String::from(
                    "There is still not a complete long-run open/close, tab-switch, and idle-animation endurance battery in the current UIKit suite.",
                )
            }],
        ),
        uikit_contract_entry(
            "stress-pathological",
            "Stress & Pathological Regressions",
            if has_case("uikit.idiomatic.stress.flat_rects.10000.mount")
                && has_case("uikit.idiomatic.stress.simultaneous_animations.300")
                && has_case("uikit.idiomatic.stress.ticker_100hz")
            {
                "implemented"
            } else {
                "partial"
            },
            vec![if has_case("uikit.idiomatic.stress.flat_rects.10000.mount")
                && has_case("uikit.idiomatic.stress.simultaneous_animations.300")
                && has_case("uikit.idiomatic.stress.ticker_100hz")
            {
                String::from(
                    "Dedicated 10k-node, 300-animation, and 100 Hz ticker traps now complement the rest of the UIKit suite.",
                )
            } else {
                String::from(
                    "The explicit 10k-node, 300-animation, and 100 Hz ticker traps are still incomplete in the UIKit suite.",
                )
            }],
        ),
    ];
    let mut notes = vec![String::from(
        "The UIKit reports now persist explicit contract coverage so the suite does not over-claim comprehensiveness.",
    )];
    if suite == "device" {
        notes.push(String::from(
            "The device report is the authoritative GPU source. Manual per-case Power Profiler traces still gate true energy coverage.",
        ));
    } else {
        notes.push(String::from(
            "The simulator report remains proxy-only for CPU, memory, and storage. Phase signposts stay instrumented in the shared harness, but Xcode 26 simulator app-hosted XCTest runs are currently not collecting them through XCTOSSignpostMetric because that path is crashing in Apple metric teardown. Device GPU and energy numbers live in the device report.",
        ));
    }
    UIKitContractCoverageReport { layers, styles, battery, notes }
}

fn uikit_contract_entry(
    id: &str,
    label: &str,
    status: &str,
    notes: Vec<String>,
) -> UIKitContractCoverageEntry {
    UIKitContractCoverageEntry {
        id: id.to_string(),
        label: label.to_string(),
        status: status.to_string(),
        notes,
    }
}

fn sanitize_metric_name(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

pub fn display_value_to_base(cell: &XctraceCell, base_unit: &str) -> Option<f64> {
    let display = cell.display()?;
    let cleaned = display.replace(',', "");
    let mut parts = cleaned.split_whitespace();
    let number = parts.next()?.parse::<f64>().ok()?;
    let unit = parts.next().unwrap_or(base_unit);
    match (base_unit, unit) {
        ("J", "J") => Some(number),
        ("J", "mJ") => Some(number / 1_000.0),
        ("J", "µJ") | ("J", "uJ") => Some(number / 1_000_000.0),
        ("W", "W") => Some(number),
        ("W", "mW") => Some(number / 1_000.0),
        ("W", "µW") | ("W", "uW") => Some(number / 1_000_000.0),
        _ => None,
    }
}

pub fn parse_uikit_report_json(text: &str) -> Result<UIKitPerfReport> {
    let bundles: Vec<XCTestMetricBundle> =
        serde_json::from_str(text).with_context(|| "parsing xcresult metrics json")?;
    let mut device_name = String::new();
    let mut cases = Vec::with_capacity(bundles.len());

    for bundle in bundles {
        let run = bundle
            .test_runs
            .first()
            .with_context(|| format!("missing test run for {}", bundle.test_identifier))?;
        if device_name.is_empty() {
            device_name = run.device.device_name.clone();
        }
        let test_name = bundle
            .test_identifier
            .split('/')
            .last()
            .unwrap_or(bundle.test_identifier.as_str())
            .trim_end_matches("()");
        let (case_id, oxide_case_id, note) = map_uikit_case(test_name)?;
        let (layer, scenario, style, cache_state) = uikit_case_contract_metadata(case_id);
        let mut metrics = BTreeMap::new();
        for metric in &run.metrics {
            let Some(metric_key) = map_uikit_metric(&metric.identifier) else {
                continue;
            };
            metrics.insert(metric_key, summarize_uikit_metric(metric)?);
        }
        for required in [
            "clock_s",
            "cpu_time_s",
            "cpu_cycles_kc",
            "cpu_instructions_ki",
            "memory_physical_kb",
            "memory_peak_kb",
        ] {
            if !metrics.contains_key(required) {
                bail!("missing `{}` metric for {}", required, test_name);
            }
        }
        cases.push(UIKitPerfCase {
            id: String::from(case_id),
            oxide_case_id: String::from(oxide_case_id),
            test_name: test_name.to_string(),
            layer: String::from(layer),
            scenario: String::from(scenario),
            style: String::from(style),
            cache_state: String::from(cache_state),
            refresh_mode: String::from(uikit_refresh_mode_for_suite("simulator")),
            threshold_pct: UIKIT_SIM_THRESHOLD_PCT,
            metrics,
            notes: vec![String::from(note)],
        });
    }

    cases.sort_by(|left, right| left.id.cmp(&right.id));

    let contract = build_uikit_contract_coverage(&cases, "simulator");

    Ok(UIKitPerfReport {
        version: 1,
        suite: String::from("simulator"),
        generated_label: std::env::var("PERF_REPORT_DATE").ok(),
        device_name,
        energy_status: String::from(
            "True energy metrics are unavailable on iOS Simulator; Apple Power Profiler is unsupported there. CPU cycles are retained as the stable on-simulator energy proxy while direct device GPU and energy reports live under benchmarks/uikit-device/.",
        ),
        contract,
        cases,
        notes: vec![
            String::from("Scheme: OxideUIKitPerf"),
            String::from("Harness: standalone iOS simulator XCTest bundle running UIKit parity views."),
        ],
    })
}

pub fn compare_uikit_reports(
    current: &UIKitPerfReport,
    baseline: &UIKitPerfReport,
) -> UIKitPerfComparison {
    let mut baseline_cases = BTreeMap::new();
    for case in &baseline.cases {
        baseline_cases.insert(uikit_case_match_key(case), case);
    }

    let mut comparison = UIKitPerfComparison::default();
    for case in &current.cases {
        let case_key = uikit_case_match_key(case);
        let Some(base_case) = baseline_cases.get(case_key.as_str()) else {
            comparison.missing_baseline.push(case_key.clone());
            continue;
        };
        comparison.matched += 1;
        for metric_name in gated_metric_names_for_case(current, case, base_case) {
            let Some(current_metric) = case.metrics.get(metric_name.as_str()) else {
                comparison.missing_baseline.push(format!("{}::{}", case_key, metric_name));
                continue;
            };
            let Some(base_metric) = base_case.metrics.get(metric_name.as_str()) else {
                comparison.missing_baseline.push(format!("{}::{}", case_key, metric_name));
                continue;
            };
            let allowed = allowed_uikit_metric_median(
                current.suite.as_str(),
                metric_name.as_str(),
                base_metric.median,
                case.threshold_pct,
            );
            if current_metric.median > allowed {
                comparison.regressions.push(UIKitPerfRegression {
                    case_id: case_key.clone(),
                    metric: metric_name.clone(),
                    baseline_median: base_metric.median,
                    current_median: current_metric.median,
                    allowed_median: allowed,
                    delta_pct: delta_pct(current_metric.median, base_metric.median),
                });
            } else if current_metric.median < base_metric.median {
                comparison.improvements.push(format!("{}::{}", case_key, metric_name));
            }
        }
    }

    comparison
}

fn uikit_case_match_key(case: &UIKitPerfCase) -> String {
    format!("{}::{}", case.id, case.refresh_mode)
}

fn allowed_uikit_metric_median(
    suite: &str,
    metric_name: &str,
    baseline_median: f64,
    threshold_pct: f64,
) -> f64 {
    let percent_limit = baseline_median * (1.0 + threshold_pct);
    let absolute_limit =
        baseline_median + uikit_metric_noise_floor(suite, metric_name, baseline_median);
    percent_limit.max(absolute_limit)
}

fn uikit_metric_noise_floor(suite: &str, metric_name: &str, baseline_median: f64) -> f64 {
    if suite != "simulator" {
        return 0.0;
    }
    match metric_name {
        "clock_s" | "cpu_time_s" if baseline_median <= UIKIT_SIM_TINY_TIME_MAX_S => {
            UIKIT_SIM_TINY_TIME_NOISE_S
        }
        "clock_s" | "cpu_time_s" if baseline_median <= UIKIT_SIM_SMALL_TIME_MAX_S => {
            UIKIT_SIM_SMALL_TIME_NOISE_S
        }
        "cpu_cycles_kc" if baseline_median <= UIKIT_SIM_TINY_CPU_CYCLES_MAX_KC => {
            UIKIT_SIM_TINY_CPU_CYCLES_NOISE_KC
        }
        "cpu_cycles_kc" if baseline_median <= UIKIT_SIM_SMALL_CPU_CYCLES_MAX_KC => {
            UIKIT_SIM_SMALL_CPU_CYCLES_NOISE_KC
        }
        _ => 0.0,
    }
}

fn gated_metric_names_for_case(
    report: &UIKitPerfReport,
    current_case: &UIKitPerfCase,
    base_case: &UIKitPerfCase,
) -> Vec<String> {
    let mut names = match report.suite.as_str() {
        "device" => {
            UIKIT_DEVICE_GATED_METRICS.iter().map(|name| (*name).to_string()).collect::<Vec<_>>()
        }
        _ => UIKIT_SIM_GATED_METRICS.iter().map(|name| (*name).to_string()).collect::<Vec<_>>(),
    };
    if current_case.metrics.contains_key("app_launch_s")
        && base_case.metrics.contains_key("app_launch_s")
        && !names.contains(&String::from("app_launch_s"))
    {
        names.push(String::from("app_launch_s"));
    }
    if report.suite == "device" {
        if current_case.metrics.contains_key("energy_j")
            && base_case.metrics.contains_key("energy_j")
        {
            names.push(String::from("energy_j"));
        }
        for metric_name in base_case.metrics.keys() {
            if metric_name.starts_with("gpu_counter.") && !names.contains(metric_name) {
                names.push(metric_name.clone());
            }
        }
    }
    if report.suite == "simulator"
        && current_case.id == base_case.id
        && is_simulator_clock_proxy_case(&current_case.id)
    {
        // These simulator cases absorb scheduler/event-loop delay in wall-clock
        // that does not show up in CPU time or cycles. Keep gating the CPU-
        // backed metrics, but do not fail on simulator clock jitter alone.
        names.retain(|name| name != "clock_s");
    }
    names
}

fn is_simulator_clock_proxy_case(case_id: &str) -> bool {
    matches!(
        case_id,
        "uikit.component.spinner.encode" | "uikit.idiomatic.navigation.button_press.response"
    )
}

fn load_uikit_report(path: &Path) -> Result<UIKitPerfReport> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

fn load_oxide_device_report(path: &Path) -> Result<PerfReport> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

fn write_oxide_device_report_json(path: &Path, report: &PerfReport) -> Result<()> {
    ensure_parent_dir(path)?;
    let json =
        serde_json::to_string_pretty(report).with_context(|| "serializing Oxide device report")?;
    fs::write(path, json).with_context(|| format!("writing {}", path.display()))
}

pub fn extract_oxide_device_report_json(stdout: &str) -> Result<String> {
    let mut in_payload = false;
    let mut saw_begin = false;
    let mut saw_end = false;
    let mut payload = String::new();

    for line in stdout.lines() {
        if !saw_begin {
            if line.contains(OXIDE_DEVICE_REPORT_BEGIN_LINE) {
                saw_begin = true;
                in_payload = true;
            }
            continue;
        }
        if in_payload && line.contains(OXIDE_DEVICE_REPORT_END_LINE) {
            saw_end = true;
            break;
        }
        if let Some(index) = line.find(OXIDE_DEVICE_REPORT_CHUNK_PREFIX) {
            payload.push_str(line[(index + OXIDE_DEVICE_REPORT_CHUNK_PREFIX.len())..].trim());
        }
    }

    if !saw_begin {
        bail!("missing `{}` marker in device console output", OXIDE_DEVICE_REPORT_BEGIN_LINE);
    }
    if !saw_end {
        bail!("missing `{}` marker in device console output", OXIDE_DEVICE_REPORT_END_LINE);
    }
    if payload.is_empty() {
        bail!("no Oxide device report payload was emitted between console markers");
    }

    let decoded = base64::engine::general_purpose::STANDARD
        .decode(payload.as_bytes())
        .with_context(|| "decoding base64 Oxide device report payload")?;
    String::from_utf8(decoded).with_context(|| "decoding Oxide device report payload as UTF-8")
}

#[derive(Debug, Clone, Deserialize)]
struct OxideStageSummaryPayload {
    stages: BTreeMap<String, UIKitMetricSummary>,
}

#[derive(Debug, Clone, Deserialize)]
struct OxideMemorySummaryPayload {
    categories: BTreeMap<String, UIKitMetricSummary>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OxideCameraContractSummaryPayload {
    pub source: String,
    pub transport: String,
    pub device_position: String,
    pub session_preset: String,
    pub requested_pixel_format: String,
    pub active_pixel_format: String,
    pub requested_width: i32,
    pub requested_height: i32,
    pub requested_fps: i32,
    pub active_width: i32,
    pub active_height: i32,
    pub active_fps: f64,
    pub video_range: String,
    pub color_space: String,
    pub wide_color_auto: bool,
    pub mirrored: bool,
}

pub fn parse_oxide_stage_summary(stdout: &str) -> Result<BTreeMap<String, UIKitMetricSummary>> {
    let mut payload_json = None;
    for line in stdout.lines() {
        if let Some(index) = line.find(OXIDE_STAGE_SUMMARY_PREFIX) {
            payload_json =
                Some(line[(index + OXIDE_STAGE_SUMMARY_PREFIX.len())..].trim().to_string());
        }
    }
    let payload_json = payload_json.with_context(|| {
        format!("missing `{}` marker in device console output", OXIDE_STAGE_SUMMARY_PREFIX)
    })?;
    let payload: OxideStageSummaryPayload =
        serde_json::from_str(&payload_json).with_context(|| "parsing Oxide stage summary json")?;
    Ok(payload
        .stages
        .into_iter()
        .map(|(stage_name, summary)| (format!("stage.{}", stage_name), summary))
        .collect())
}

pub fn parse_oxide_memory_summary(stdout: &str) -> Result<BTreeMap<String, UIKitMetricSummary>> {
    let mut payload_json = None;
    for line in stdout.lines() {
        if let Some(index) = line.find(OXIDE_MEMORY_SUMMARY_PREFIX) {
            payload_json =
                Some(line[(index + OXIDE_MEMORY_SUMMARY_PREFIX.len())..].trim().to_string());
        }
    }
    let payload_json = payload_json.with_context(|| {
        format!("missing `{}` marker in device console output", OXIDE_MEMORY_SUMMARY_PREFIX)
    })?;
    let payload: OxideMemorySummaryPayload =
        serde_json::from_str(&payload_json).with_context(|| "parsing Oxide memory summary json")?;
    Ok(payload
        .categories
        .into_iter()
        .map(|(name, summary)| (format!("memory.{}", name), summary))
        .collect())
}

fn render_oxide_memory_breakdown_note(
    memory_metrics: &BTreeMap<String, UIKitMetricSummary>,
) -> Option<String> {
    let top_bytes = [
        ("memory.camera.sample_delivery_pool_bytes_est", "camera.samplePoolEst"),
        ("memory.camera.active_sample_surface_bytes_est", "camera.activeSampleEst"),
        ("memory.camera.peak_active_sample_surface_bytes_est", "camera.peakActiveSampleEst"),
        ("memory.camera.retained_sample_surface_bytes_est", "camera.retainedSampleEst"),
        (
            "memory.camera.retained_published_slot_surface_bytes_est",
            "camera.retainedPublishedSlots",
        ),
        (
            "memory.camera.retained_latest_pixel_buffer_surface_bytes_est",
            "camera.retainedLatestPixelBuffer",
        ),
        ("memory.renderer.total_bytes", "renderer.total"),
        ("memory.view.drawable_pool_bytes_est", "view.drawablePoolEst"),
        ("memory.known.total_bytes_est", "known.totalEst"),
        ("memory.renderer.draw_targets_bytes", "renderer.drawTargets"),
        ("memory.renderer.draw_target_main_bytes", "renderer.drawTargetMain"),
        ("memory.renderer.draw_target_msaa_bytes", "renderer.drawTargetMsaa"),
        ("memory.renderer.effect_targets_bytes", "renderer.effectTargets"),
        ("memory.renderer.effect_prepass_bytes", "renderer.effectPrepass"),
        ("memory.renderer.effect_blur_chain_bytes", "renderer.effectBlurChain"),
        ("memory.renderer.live_camera_bytes", "renderer.liveCamera"),
        ("memory.renderer.camera_cache_bytes", "renderer.cameraCache"),
        ("memory.renderer.camera_blur_cache_bytes", "renderer.cameraBlurCache"),
        ("memory.renderer.camera_transition_cache_bytes", "renderer.cameraTransitionCache"),
        ("memory.renderer.layer_cache_bytes", "renderer.layerCache"),
        ("memory.renderer.image_cache_bytes", "renderer.imageCache"),
        ("memory.renderer.buffer_bytes", "renderer.buffers"),
        ("memory.renderer.benchmark_camera_bytes", "renderer.syntheticCamera"),
    ]
    .into_iter()
    .filter_map(|(key, label)| {
        memory_metrics
            .get(key)
            .map(|summary| (label, summary.max / (1024.0 * 1024.0)))
            .filter(|(_, mb)| *mb > 0.0)
    })
    .collect::<Vec<_>>();
    let mut parts = top_bytes
        .into_iter()
        .take(6)
        .map(|(label, mb)| format!("{}={:.2}MB", label, mb))
        .collect::<Vec<_>>();
    if let Some(summary) = memory_metrics.get("memory.renderer.pending_command_buffers") {
        parts.push(format!("renderer.pendingCmdBuffers={:.0}", summary.max));
    }
    if let Some(summary) = memory_metrics.get("memory.renderer.pending_present_drawables") {
        parts.push(format!("renderer.pendingPresentDrawables={:.0}", summary.max));
    }
    if let Some(summary) = memory_metrics.get("memory.renderer.pending_present_textures") {
        parts.push(format!("renderer.pendingPresentTextures={:.0}", summary.max));
    }
    if let Some(summary) = memory_metrics.get("memory.renderer.preview_submission_depth") {
        parts.push(format!("renderer.previewDepthMax={:.0}", summary.max));
    }
    let depth0_share = memory_metrics
        .get("memory.renderer.preview_submission_depth_is_0")
        .map(|summary| summary.mean * 100.0);
    let depth1_share = memory_metrics
        .get("memory.renderer.preview_submission_depth_is_1")
        .map(|summary| summary.mean * 100.0);
    let depth2_share = memory_metrics
        .get("memory.renderer.preview_submission_depth_is_2_or_more")
        .map(|summary| summary.mean * 100.0);
    if let (Some(depth0), Some(depth1), Some(depth2)) = (depth0_share, depth1_share, depth2_share) {
        parts.push(format!(
            "renderer.previewDepthShare=0:{:.0}%/1:{:.0}%/2+:{:.0}%",
            depth0, depth1, depth2
        ));
    }
    if let Some(summary) = memory_metrics.get("memory.renderer.preview_submission_skipped") {
        parts.push(format!("renderer.previewSkipRate={:.1}%", summary.mean * 100.0));
    }
    if let Some(summary) = memory_metrics.get("memory.renderer.preview_submission_frame_age_ms") {
        parts.push(format!("renderer.previewFrameAgeP95={:.2}ms", summary.p95));
        parts.push(format!("renderer.previewFrameAgeMax={:.2}ms", summary.max));
    }
    if let Some(summary) = memory_metrics.get("memory.camera.sample_delivery_pool_surfaces") {
        parts.push(format!("camera.samplePoolSurfaces={:.0}", summary.max));
    }
    if let Some(summary) = memory_metrics.get("memory.camera.active_sample_surface_surfaces") {
        parts.push(format!("camera.activeSampleSurfaces={:.0}", summary.max));
    }
    if let Some(summary) = memory_metrics.get("memory.camera.active_sample_buffers") {
        parts.push(format!("camera.activeSampleBuffers={:.0}", summary.max));
    }
    if let Some(summary) = memory_metrics.get("memory.camera.peak_active_sample_surface_surfaces") {
        parts.push(format!("camera.peakActiveSampleSurfaces={:.0}", summary.max));
    }
    if let Some(summary) = memory_metrics.get("memory.camera.peak_active_sample_buffers") {
        parts.push(format!("camera.peakActiveSampleBuffers={:.0}", summary.max));
    }
    let sample_total = memory_metrics
        .get("memory.camera.sample_delivery_total_samples")
        .map(|summary| summary.max);
    let sample_reused_frames = memory_metrics
        .get("memory.camera.sample_delivery_reused_frames")
        .map(|summary| summary.max);
    if let (Some(reused), Some(total)) = (sample_reused_frames, sample_total) {
        parts.push(format!("camera.sampleReuseFrames={:.0}/{:.0}", reused, total));
    }
    if let Some(summary) = memory_metrics.get("memory.camera.sample_delivery_reuse_fraction") {
        parts.push(format!("camera.sampleReuseRate={:.1}%", summary.mean * 100.0));
    }
    if let Some(summary) = memory_metrics.get("memory.camera.sample_delivery_reused_surfaces") {
        parts.push(format!("camera.sampleReusedSurfaces={:.0}", summary.max));
    }
    if let Some(summary) = memory_metrics.get("memory.camera.sample_delivery_max_reuse_gap_frames")
    {
        parts.push(format!("camera.sampleMaxReuseGapFrames={:.0}", summary.max));
    }
    if let Some(summary) = memory_metrics.get("memory.camera.retained_sample_surface_surfaces") {
        parts.push(format!("camera.retainedSampleSurfaces={:.0}", summary.max));
    }
    if let Some(summary) = memory_metrics.get("memory.camera.retained_published_slot_surfaces") {
        parts.push(format!("camera.retainedPublishedSlotSurfaces={:.0}", summary.max));
    }
    if let Some(summary) =
        memory_metrics.get("memory.camera.retained_latest_pixel_buffer_surface_surfaces")
    {
        parts.push(format!("camera.retainedLatestPixelBufferSurfaces={:.0}", summary.max));
    }
    if parts.is_empty() {
        None
    } else {
        Some(format!("Observed preview memory breakdown (max observed): {}", parts.join(", ")))
    }
}

pub fn parse_oxide_camera_contract_summary(
    stdout: &str,
) -> Result<OxideCameraContractSummaryPayload> {
    let mut payload_json = None;
    for line in stdout.lines() {
        if let Some(index) = line.find(OXIDE_CAMERA_CONTRACT_SUMMARY_PREFIX) {
            payload_json = Some(
                line[(index + OXIDE_CAMERA_CONTRACT_SUMMARY_PREFIX.len())..].trim().to_string(),
            );
        }
    }
    let payload_json = payload_json.with_context(|| {
        format!(
            "missing `{}` marker in device console output",
            OXIDE_CAMERA_CONTRACT_SUMMARY_PREFIX
        )
    })?;
    serde_json::from_str(&payload_json)
        .with_context(|| "parsing Oxide camera contract summary json")
}

fn render_oxide_camera_contract_note(payload: &OxideCameraContractSummaryPayload) -> String {
    format!(
        "Capture contract: source={} transport={} device={} preset={} request={}x{}@{} {} active={}x{}@{:.2} {} range={} color={} mirrored={} wideColorAuto={}",
        payload.source,
        payload.transport,
        payload.device_position,
        payload.session_preset,
        payload.requested_width,
        payload.requested_height,
        payload.requested_fps,
        payload.requested_pixel_format,
        payload.active_width,
        payload.active_height,
        payload.active_fps,
        payload.active_pixel_format,
        payload.video_range,
        payload.color_space,
        payload.mirrored,
        payload.wide_color_auto
    )
}

fn is_normalized_yuv_pixel_format(pixel_format: &str) -> bool {
    matches!(pixel_format, "yuv" | "420f" | "420v")
}

pub fn validate_normalized_camera_contract(
    payload: &OxideCameraContractSummaryPayload,
    label: &str,
) -> Result<()> {
    if payload.device_position != "back" {
        bail!(
            "{} camera contract expected `back` camera, found `{}`",
            label,
            payload.device_position
        );
    }
    if payload.requested_width != 1280
        || payload.requested_height != 720
        || payload.active_width != 1280
        || payload.active_height != 720
    {
        bail!(
            "{} camera contract expected stable 1280x720 negotiation, found request={}x{} active={}x{}",
            label,
            payload.requested_width,
            payload.requested_height,
            payload.active_width,
            payload.active_height
        );
    }
    if payload.requested_fps != 30 || (payload.active_fps - 30.0).abs() > 0.01 {
        bail!(
            "{} camera contract expected stable 30 fps negotiation, found request={} active={:.2}",
            label,
            payload.requested_fps,
            payload.active_fps
        );
    }
    if !is_normalized_yuv_pixel_format(&payload.requested_pixel_format)
        || !is_normalized_yuv_pixel_format(&payload.active_pixel_format)
    {
        bail!(
            "{} camera contract expected stable YUV-family negotiation, found request=`{}` active=`{}`",
            label,
            payload.requested_pixel_format,
            payload.active_pixel_format
        );
    }
    Ok(())
}

fn parse_oxide_device_report_json(stdout: &str) -> Result<PerfReport> {
    let json = extract_oxide_device_report_json(stdout)?;
    serde_json::from_str(&json).with_context(|| "parsing Oxide device perf report json")
}

pub fn parse_react_native_device_report_json(
    text: &str,
    stdout: &str,
    device_name: &str,
    executable_name: &str,
) -> Result<PerfReport> {
    let bundles: Vec<XCTestMetricBundle> =
        serde_json::from_str(text).with_context(|| "parsing React Native xcresult metrics json")?;
    let bundle = bundles
        .iter()
        .find(|bundle| bundle.test_identifier.contains(DEFAULT_REACT_DEVICE_TEST_NAME))
        .with_context(|| {
            format!(
                "missing `{}` metrics bundle in React Native xcresult json",
                DEFAULT_REACT_DEVICE_TEST_NAME
            )
        })?;
    let run = bundle
        .test_runs
        .first()
        .with_context(|| format!("missing test run for {}", bundle.test_identifier))?;

    let mut metrics = BTreeMap::new();
    let mut metric_summaries = BTreeMap::new();
    for metric in &run.metrics {
        let Some(metric_key) = map_uikit_metric(&metric.identifier) else {
            continue;
        };
        let summary = summarize_uikit_metric(metric)?;
        metrics.insert(metric_key.clone(), summary.median);
        metric_summaries.insert(metric_key, summary);
    }
    for required in [
        "clock_s",
        "cpu_time_s",
        "cpu_cycles_kc",
        "cpu_instructions_ki",
        "memory_physical_kb",
        "memory_peak_kb",
    ] {
        if !metric_summaries.contains_key(required) {
            bail!("missing `{}` metric for {}", required, DEFAULT_REACT_DEVICE_TEST_NAME);
        }
    }

    let contract = parse_oxide_camera_contract_summary(stdout)?;
    validate_normalized_camera_contract(&contract, "React Native")?;
    let clock = metric_summaries.get("clock_s").expect("required clock metric already validated");
    let case = PerfCaseResult {
        id: String::from(REACT_NATIVE_CAMERA_CASE_ID),
        family: String::from("image_pipeline"),
        layer: String::from("cross_platform"),
        scenario: String::from("camera_preview"),
        variant: String::from("react_native_vision_camera"),
        cache_state: String::from("warm"),
        refresh_mode: String::from("native"),
        unit: String::from("s"),
        gated: false,
        threshold_pct: UIKIT_DEVICE_THRESHOLD_PCT,
        median: clock.median,
        p95: clock.p95,
        p99: clock.p99,
        min: clock.min,
        max: clock.max,
        mean: clock.mean,
        samples: clock.samples,
        ops_per_sample: 1,
        notes: vec![
            String::from(
                "React Native VisionCamera live preview using the mainstream native preview-view path.",
            ),
            String::from(
                "On iOS this React arm stays on the library's system-managed native preview-view transport, not an app-owned raw-frame Metal renderer.",
            ),
            String::from(
                "Capture contract validation: stable back-camera 1280x720@30 YUV-family negotiation confirmed before the report was accepted.",
            ),
            render_oxide_camera_contract_note(&contract),
        ],
        metrics,
    };

    Ok(PerfReport {
        version: 1,
        suite: String::from("react-native-device"),
        generated_label: std::env::var("PERF_REPORT_DATE").ok(),
        cases: vec![case],
        coverage: CoverageReport {
            image_pipeline_total: 1,
            image_pipeline_covered: vec![String::from("react_native.vision_camera.live_preview")],
            ..CoverageReport::default()
        },
        contract: ContractCoverageReport {
            layers: vec![ContractCoverageEntry {
                id: String::from("react-native-cross-platform"),
                label: String::from("React Native Cross-Platform Camera Preview"),
                status: String::from("implemented"),
                notes: vec![String::from(
                    "This report measures a physical-iPhone React Native + VisionCamera fullscreen back-camera preview using the same normalized 1280x720@30 contract we use for the UIKit and Oxide camera comparisons.",
                )],
            }],
            battery: vec![ContractCoverageEntry {
                id: String::from("camera-preview"),
                label: String::from("Camera Preview"),
                status: String::from("implemented"),
                notes: vec![String::from(
                    "The current React battery covers one release-style live preview case on the plugged-in iPhone with the same 24-step signposted preview workload used by the AVFoundation and Oxide camera harnesses.",
                )],
            }],
            notes: vec![
                String::from("Scheme: ReactNativeCameraBenchPerf"),
                format!("Device: `{}`", device_name),
                format!("Executable: `{}`", executable_name),
                format!("Reported device from xcresult: `{}`", run.device.device_name),
                String::from(
                    "Device flow: generic iOS build-for-testing, then xcodebuild test-without-building on the physical iPhone using the app-hosted React Native XCTest bundle, with the measured preview workload gated by the same Darwin ready/start/complete handshake used by the other camera device harnesses.",
                ),
                String::from(
                    "Metric scope: XCTest clock/CPU/memory/storage plus direct physical-device Metal System Trace GPU time and GPU latency bounded by shared PerfWorkload signposts.",
                ),
            ],
        },
        findings: vec![AuditFinding {
            status: String::from("info"),
            summary: String::from(
                "This React Native baseline uses the mainstream VisionCamera native preview path rather than a raw-frame custom renderer, so it is the cross-platform-framework reference arm in the three-way camera comparison.",
            ),
        }],
    })
}

fn write_oxide_device_report_markdown(
    path: &Path,
    report: &PerfReport,
    comparison: Option<&oxide_perf_runner::PerfComparison>,
) -> Result<()> {
    ensure_parent_dir(path)?;
    let mut markdown = render_report_markdown(report, comparison);
    markdown =
        markdown.replacen("# Oxide Performance Report", "# Oxide Device Performance Report", 1);
    markdown = markdown.replace(
        "PERF_REPORT_DATE=$(date +%F) cargo run --release -j$(sysctl -n hw.ncpu) -p oxide-perf-runner -- --run-suite --write-baseline",
        "PERF_REPORT_DATE=$(date +%F) cargo run --locked -j$(sysctl -n hw.ncpu) -p xtask -- ios oxide-device-perf --write-baseline",
    );
    markdown =
        markdown.replace("benchmarks/workspace/latest.json", DEFAULT_OXIDE_DEVICE_BASELINE_JSON);
    markdown =
        markdown.replace("benchmarks/workspace/latest.md", DEFAULT_OXIDE_DEVICE_BASELINE_MARKDOWN);
    fs::write(path, markdown).with_context(|| format!("writing {}", path.display()))
}

fn write_react_device_report_json(path: &Path, report: &PerfReport) -> Result<()> {
    ensure_parent_dir(path)?;
    let json = serde_json::to_string_pretty(report)
        .with_context(|| "serializing React Native perf report")?;
    fs::write(path, json).with_context(|| format!("writing {}", path.display()))
}

fn write_react_device_report_markdown(
    path: &Path,
    report: &PerfReport,
    comparison: Option<&oxide_perf_runner::PerfComparison>,
) -> Result<()> {
    ensure_parent_dir(path)?;
    let mut markdown = render_report_markdown(report, comparison);
    markdown = markdown.replacen(
        "# Oxide Performance Report",
        "# React Native Device Performance Report",
        1,
    );
    markdown = markdown.replace(
        "PERF_REPORT_DATE=$(date +%F) cargo run --release -j$(sysctl -n hw.ncpu) -p oxide-perf-runner -- --run-suite --write-baseline",
        "PERF_REPORT_DATE=$(date +%F) cargo run --locked -j$(sysctl -n hw.ncpu) -p xtask -- ios react-device-perf --write-baseline",
    );
    markdown =
        markdown.replace("benchmarks/workspace/latest.json", DEFAULT_REACT_DEVICE_BASELINE_JSON);
    markdown =
        markdown.replace("benchmarks/workspace/latest.md", DEFAULT_REACT_DEVICE_BASELINE_MARKDOWN);
    fs::write(path, markdown).with_context(|| format!("writing {}", path.display()))
}

fn write_react_device_dated_markdown(
    latest_path: &Path,
    report: &PerfReport,
    comparison: Option<&oxide_perf_runner::PerfComparison>,
) -> Result<()> {
    let Some(label) = report.generated_label.as_ref() else {
        return Ok(());
    };
    let dated_path = latest_path.with_file_name(format!("{}.md", label));
    if dated_path == latest_path {
        return Ok(());
    }
    write_react_device_report_markdown(&dated_path, report, comparison)
}

fn write_oxide_device_dated_markdown(
    latest_path: &Path,
    report: &PerfReport,
    comparison: Option<&oxide_perf_runner::PerfComparison>,
) -> Result<()> {
    let Some(label) = report.generated_label.as_ref() else {
        return Ok(());
    };
    let dated_path = latest_path.with_file_name(format!("{}.md", label));
    if dated_path == latest_path {
        return Ok(());
    }
    write_oxide_device_report_markdown(&dated_path, report, comparison)
}

fn print_oxide_device_summary(
    report: &PerfReport,
    comparison: Option<&oxide_perf_runner::PerfComparison>,
) {
    println!(
        "Oxide device perf report: suite={} cases={} components={}/{} animations={}/{} launch={}/{} primitive_lifecycle={}/{} scenes_cpu={}/{} scenes_gpu={}/{} journeys={}/{} authoring={}/{} image_pipeline={}/{} navigation={}/{} reconcile={}/{} bridges={}/{}",
        report.suite,
        report.cases.len(),
        report.coverage.components_covered.len(),
        report.coverage.components_total,
        report.coverage.animations_covered.len(),
        report.coverage.animations_total,
        report.coverage.launch_covered.len(),
        report.coverage.launch_total,
        report.coverage.primitive_lifecycle_covered.len(),
        report.coverage.primitive_lifecycle_total,
        report.coverage.scenes_cpu_covered.len(),
        report.coverage.scenes_cpu_total,
        report.coverage.scenes_gpu_covered.len(),
        report.coverage.scenes_gpu_total,
        report.coverage.journeys_covered.len(),
        report.coverage.journeys_total,
        report.coverage.authoring_covered.len(),
        report.coverage.authoring_total,
        report.coverage.image_pipeline_covered.len(),
        report.coverage.image_pipeline_total,
        report.coverage.navigation_covered.len(),
        report.coverage.navigation_total,
        report.coverage.reconcile_covered.len(),
        report.coverage.reconcile_total,
        report.coverage.bridges_covered.len(),
        report.coverage.bridges_total
    );
    if let Some(comp) = comparison {
        println!(
            "Oxide device compare: matched={} missing={} regressions={} improvements={}",
            comp.matched,
            comp.missing_baseline.len(),
            comp.regressions.len(),
            comp.improvements.len()
        );
    }
}

fn print_react_device_summary(
    report: &PerfReport,
    comparison: Option<&oxide_perf_runner::PerfComparison>,
) {
    println!(
        "React Native device perf report: suite={} cases={}",
        report.suite,
        report.cases.len()
    );
    if let Some(comp) = comparison {
        println!(
            "React Native device compare: matched={} missing={} regressions={} improvements={}",
            comp.matched,
            comp.missing_baseline.len(),
            comp.regressions.len(),
            comp.improvements.len()
        );
    }
}

fn write_uikit_report_json(path: &Path, report: &UIKitPerfReport) -> Result<()> {
    ensure_parent_dir(path)?;
    let json =
        serde_json::to_string_pretty(report).with_context(|| "serializing UIKit perf report")?;
    fs::write(path, json).with_context(|| format!("writing {}", path.display()))
}

fn push_uikit_contract_markdown(out: &mut String, report: &UIKitPerfReport) {
    out.push_str("\n## Contract Coverage\n\n");
    out.push_str("| Section | Status | Notes |\n");
    out.push_str("| --- | --- | --- |\n");
    for entry in &report.contract.layers {
        out.push_str(&format!(
            "| `{}` | `{}` | {} |\n",
            entry.label,
            entry.status,
            entry.notes.join(" ")
        ));
    }
    for entry in &report.contract.styles {
        out.push_str(&format!(
            "| `{}` | `{}` | {} |\n",
            entry.label,
            entry.status,
            entry.notes.join(" ")
        ));
    }
    for entry in &report.contract.battery {
        out.push_str(&format!(
            "| `{}` | `{}` | {} |\n",
            entry.label,
            entry.status,
            entry.notes.join(" ")
        ));
    }
    if !report.contract.notes.is_empty() {
        out.push_str("\n");
        for note in &report.contract.notes {
            out.push_str(&format!("- {}\n", note));
        }
    }
}

fn write_uikit_markdown(
    path: &Path,
    report: &UIKitPerfReport,
    comparison: Option<&UIKitPerfComparison>,
) -> Result<()> {
    if report.suite == "device" {
        return write_uikit_device_markdown(path, report, comparison);
    }
    ensure_parent_dir(path)?;
    let mut out = String::new();
    out.push_str("# UIKit Perf Report\n\n");
    out.push_str(&format!("- Suite: `{}`\n", report.suite));
    out.push_str(&format!("- Device: `{}`\n", report.device_name));
    out.push_str(&format!("- Energy: {}\n", report.energy_status));
    out.push_str("- CPU columns measure UIKit-side orchestration cost (layout, animation stepping, layer updates, command submission) around a GPU-backed rendering pipeline; they do not imply final rasterization happened on the CPU.\n");
    out.push_str("- Metrics reflect 10 XCTest iterations per case on the same simulator target used for CI. Stable XCTest clock/CPU/memory/storage metrics are always collected; phase columns are filled only when the runner can export them safely.\n");
    if let Some(label) = report.generated_label.as_ref() {
        out.push_str(&format!("- Label: `{}`\n", label));
    }
    if let Some(comp) = comparison {
        out.push_str(&format!("- Baseline matches: `{}`\n", comp.matched));
        out.push_str(&format!("- Missing baseline cases: `{}`\n", comp.missing_baseline.len()));
        out.push_str(&format!("- Regressions: `{}`\n", comp.regressions.len()));
    }

    push_uikit_contract_markdown(&mut out, report);

    out.push_str("\n## Case Table\n\n");
    out.push_str("| UIKit Case | Layer | Scenario | Style | Cache | Refresh | P50 ms | P95 ms | P99 ms | Peak ms | CPU ms | CPU cycles kC | Writes kB | RSS kB | Peak kB | Launch/Mount ms | Layout ms | Text ms | Diff ms | Draw ms | Present ms | Scroll ms | Transition ms | Bridge ms |\n");
    out.push_str("| --- | --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for case in &report.cases {
        out.push_str(&format!(
            "| `{}` | `{}` | `{}` | `{}` | `{}` | `{}` | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            case.id,
            case.layer,
            case.scenario,
            case.style,
            case.cache_state,
            case.refresh_mode,
            metric_median_ms(case, "clock_s"),
            metric_percentile_ms(case, "clock_s", PercentileKey::P95),
            metric_percentile_ms(case, "clock_s", PercentileKey::P99),
            metric_peak_ms(case, "clock_s"),
            metric_median_ms(case, "cpu_time_s"),
            metric_median(case, "cpu_cycles_kc"),
            metric_median(case, "logical_writes_kb"),
            metric_median(case, "memory_physical_kb"),
            metric_median(case, "memory_peak_kb"),
            launch_or_mount_display_ms(case),
            metric_display_ms(case, "signpost_layout_s"),
            metric_display_ms(case, "signpost_text_measure_s"),
            metric_display_ms(case, "signpost_diff_apply_s"),
            metric_display_ms(case, "signpost_draw_encode_s"),
            metric_display_ms(case, "signpost_frame_present_s"),
            metric_display_ms(case, "signpost_scroll_s"),
            metric_display_ms(case, "signpost_transition_s"),
            metric_display_ms(case, "signpost_native_bridge_s"),
        ));
    }

    if let Some(comp) = comparison {
        out.push_str("\n## Comparison\n\n");
        if comp.regressions.is_empty() && comp.missing_baseline.is_empty() {
            out.push_str("- No UIKit perf regressions against the committed baseline.\n");
        } else {
            for missing in &comp.missing_baseline {
                out.push_str(&format!("- Missing baseline: `{}`\n", missing));
            }
            for reg in &comp.regressions {
                out.push_str(&format!(
                    "- Regression: `{}` `{}` {:.3} -> {:.3} (allowed {:.3}, delta {:+.2}%)\n",
                    reg.case_id,
                    reg.metric,
                    reg.baseline_median,
                    reg.current_median,
                    reg.allowed_median,
                    reg.delta_pct
                ));
            }
        }
    }

    out.push_str("\n## Notes\n\n");
    for note in &report.notes {
        out.push_str(&format!("- {}\n", note));
    }
    out.push_str("- True iOS energy capture remains device-only. The simulator report persists CPU cycles as the stable energy proxy; direct GPU and energy baselines live under `benchmarks/uikit-device/`.\n");
    out.push_str("- The simulator UIKit suite now carries idiomatic parity across components, primitive lifecycle, authoring APIs, journeys, bridge overhead, endurance loops, and launch/lifecycle, plus hand-optimized UIKit peers across primitive lifecycle, animation/effects, image pipeline, text input, journeys, bridges, and endurance.\n");

    fs::write(path, out).with_context(|| format!("writing {}", path.display()))
}

fn write_uikit_device_markdown(
    path: &Path,
    report: &UIKitPerfReport,
    comparison: Option<&UIKitPerfComparison>,
) -> Result<()> {
    let includes_energy = report_includes_metric(report, "energy_j");
    ensure_parent_dir(path)?;
    let mut out = String::new();
    out.push_str("# UIKit Device Perf Report\n\n");
    out.push_str(&format!("- Suite: `{}`\n", report.suite));
    out.push_str(&format!("- Device: `{}`\n", report.device_name));
    out.push_str(&format!("- Energy: {}\n", report.energy_status));
    out.push_str("- CPU columns measure UIKit-side orchestration cost around a GPU-backed rendering pipeline; GPU columns come from direct physical-device Instruments traces.\n");
    if includes_energy {
        out.push_str("- Metrics reflect 10 XCTest iterations per case plus automated per-case process-scoped Metal System Trace captures attached only to the single launched OxideHost process on the same physical iPhone. Direct energy values in this report come from manually imported per-case Power Profiler traces for the same workload. Shared workload/phase signposts still bound the device traces even when the XCTest result bundle is carrying only the stable core metrics.\n");
    } else {
        out.push_str("- Metrics reflect 10 XCTest iterations per case plus automated per-case process-scoped Metal System Trace captures attached only to the single launched OxideHost process on the same physical iPhone. Energy is manual-pending and is intentionally omitted from this run. Shared workload/phase signposts still bound the device traces even when the XCTest result bundle is carrying only the stable core metrics.\n");
    }
    if let Some(label) = report.generated_label.as_ref() {
        out.push_str(&format!("- Label: `{}`\n", label));
    }
    if let Some(comp) = comparison {
        out.push_str(&format!("- Baseline matches: `{}`\n", comp.matched));
        out.push_str(&format!("- Missing baseline cases: `{}`\n", comp.missing_baseline.len()));
        out.push_str(&format!("- Regressions: `{}`\n", comp.regressions.len()));
    }

    push_uikit_contract_markdown(&mut out, report);

    out.push_str("\n## Case Table\n\n");
    out.push_str("| UIKit Case | Layer | Scenario | Style | Cache | Refresh | P50 ms | P95 ms | P99 ms | Peak ms | CPU ms | Peak kB | GPU time ms | GPU latency ms | Energy J | Launch/Mount ms | Layout ms | Text ms | Diff ms | Draw ms | Present ms | Scroll ms | Transition ms | Bridge ms | GPU counters |\n");
    out.push_str("| --- | --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |\n");
    for case in &report.cases {
        let counter_count =
            case.metrics.keys().filter(|name| name.starts_with("gpu_counter.")).count();
        let energy_display = case
            .metrics
            .get("energy_j")
            .map(|metric| format!("{:.6}", metric.median))
            .unwrap_or_else(|| String::from("manual pending"));
        out.push_str(&format!(
            "| `{}` | `{}` | `{}` | `{}` | `{}` | `{}` | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | `{}` |\n",
            case.id,
            case.layer,
            case.scenario,
            case.style,
            case.cache_state,
            case.refresh_mode,
            metric_median_ms(case, "clock_s"),
            metric_percentile_ms(case, "clock_s", PercentileKey::P95),
            metric_percentile_ms(case, "clock_s", PercentileKey::P99),
            metric_peak_ms(case, "clock_s"),
            metric_median_ms(case, "cpu_time_s"),
            metric_median(case, "memory_peak_kb"),
            metric_median_ms(case, "gpu_time_s"),
            metric_median_ms(case, "gpu_latency_s"),
            energy_display,
            launch_or_mount_display_ms(case),
            metric_display_ms(case, "signpost_layout_s"),
            metric_display_ms(case, "signpost_text_measure_s"),
            metric_display_ms(case, "signpost_diff_apply_s"),
            metric_display_ms(case, "signpost_draw_encode_s"),
            metric_display_ms(case, "signpost_frame_present_s"),
            metric_display_ms(case, "signpost_scroll_s"),
            metric_display_ms(case, "signpost_transition_s"),
            metric_display_ms(case, "signpost_native_bridge_s"),
            format!("{} direct", counter_count),
        ));
    }

    if let Some(comp) = comparison {
        out.push_str("\n## Comparison\n\n");
        if comp.regressions.is_empty() && comp.missing_baseline.is_empty() {
            out.push_str("- No UIKit device perf regressions against the committed baseline.\n");
        } else {
            for missing in &comp.missing_baseline {
                out.push_str(&format!("- Missing baseline: `{}`\n", missing));
            }
            for reg in &comp.regressions {
                out.push_str(&format!(
                    "- Regression: `{}` `{}` {:.6} -> {:.6} (allowed {:.6}, delta {:+.2}%)\n",
                    reg.case_id,
                    reg.metric,
                    reg.baseline_median,
                    reg.current_median,
                    reg.allowed_median,
                    reg.delta_pct
                ));
            }
        }
    }

    out.push_str("\n## Notes\n\n");
    for note in &report.notes {
        out.push_str(&format!("- {}\n", note));
    }
    for case in &report.cases {
        for note in &case.notes {
            if note.starts_with("Direct counter:") || note.starts_with("GPU counter status:") {
                out.push_str(&format!("- `{}`: {}\n", case.id, note));
            }
        }
    }

    fs::write(path, out).with_context(|| format!("writing {}", path.display()))
}

fn write_uikit_dated_markdown(
    latest_path: &Path,
    report: &UIKitPerfReport,
    comparison: Option<&UIKitPerfComparison>,
) -> Result<()> {
    let Some(label) = report.generated_label.as_ref() else {
        return Ok(());
    };
    let dated_path = latest_path.with_file_name(format!("{}.md", label));
    if dated_path == latest_path {
        return Ok(());
    }
    write_uikit_markdown(&dated_path, report, comparison)
}

fn print_uikit_summary(report: &UIKitPerfReport, comparison: Option<&UIKitPerfComparison>) {
    println!("UIKit perf report: {} cases on {}", report.cases.len(), report.device_name);
    println!("Energy: {}", report.energy_status);
    if let Some(comp) = comparison {
        println!(
            "UIKit compare: matched={} missing={} regressions={} improvements={}",
            comp.matched,
            comp.missing_baseline.len(),
            comp.regressions.len(),
            comp.improvements.len()
        );
    }
}

fn ensure_parent_dir(path: &Path) -> Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))
}

pub fn map_uikit_case(test_name: &str) -> Result<(&'static str, &'static str, &'static str)> {
    UIKIT_CASE_SPECS
        .iter()
        .find(|spec| spec.test_name == test_name)
        .map(|spec| (spec.case_id, spec.oxide_case_id, spec.note))
        .with_context(|| format!("unmapped UIKit perf test `{}`", test_name))
}

fn map_uikit_metric(identifier: &str) -> Option<String> {
    let lowered = identifier.to_ascii_lowercase();
    if lowered.contains("application") && lowered.contains("launch") {
        return Some(String::from("app_launch_s"));
    }
    if lowered.contains("clock.time.monotonic") {
        return Some(String::from("clock_s"));
    }
    if lowered.contains("cpu.time") {
        return Some(String::from("cpu_time_s"));
    }
    if lowered.contains("cpu.cycles") {
        return Some(String::from("cpu_cycles_kc"));
    }
    if lowered.contains("cpu.instructions_retired") {
        return Some(String::from("cpu_instructions_ki"));
    }
    if lowered.contains("memory.physical_peak") {
        return Some(String::from("memory_peak_kb"));
    }
    if lowered.contains("memory.physical") {
        return Some(String::from("memory_physical_kb"));
    }
    if lowered.contains("storage") && (lowered.contains("write") || lowered.contains("logical")) {
        return Some(String::from("logical_writes_kb"));
    }
    if lowered.contains("hitch") && lowered.contains("ratio") {
        return Some(String::from("hitch_ms_per_s"));
    }
    if let Some((_, rest)) = lowered.split_once("ossignpost-") {
        if let Some(name) = rest.strip_suffix(".duration") {
            if name == "perfworkload" {
                return Some(String::from("workload_s"));
            }
            let sanitized = sanitize_metric_name(name);
            return Some(format!("signpost_{}_s", sanitized));
        }
    }
    None
}

fn summarize_uikit_metric(metric: &XCTestMetric) -> Result<UIKitMetricSummary> {
    metric_summary_from_samples(&metric.unit_of_measurement, &metric.measurements)
}

enum PercentileKey {
    P95,
    P99,
}

fn percentile(sorted_values: &[f64], pct: f64) -> f64 {
    if sorted_values.is_empty() {
        return 0.0;
    }
    let last = sorted_values.len() - 1;
    let index = ((last as f64) * pct).round() as usize;
    sorted_values[index.min(last)]
}

fn metric_median(case: &UIKitPerfCase, name: &str) -> f64 {
    case.metrics.get(name).map(|metric| metric.median).unwrap_or_default()
}

fn metric_percentile(case: &UIKitPerfCase, name: &str, which: PercentileKey) -> f64 {
    case.metrics
        .get(name)
        .map(|metric| match which {
            PercentileKey::P95 => metric.p95,
            PercentileKey::P99 => metric.p99,
        })
        .unwrap_or_default()
}

fn metric_peak(case: &UIKitPerfCase, name: &str) -> f64 {
    case.metrics.get(name).map(|metric| metric.max).unwrap_or_default()
}

fn report_includes_metric(report: &UIKitPerfReport, name: &str) -> bool {
    !report.cases.is_empty() && report.cases.iter().all(|case| case.metrics.contains_key(name))
}

fn metric_median_ms(case: &UIKitPerfCase, name: &str) -> f64 {
    metric_median(case, name) * 1000.0
}

fn metric_percentile_ms(case: &UIKitPerfCase, name: &str, which: PercentileKey) -> f64 {
    metric_percentile(case, name, which) * 1000.0
}

fn metric_peak_ms(case: &UIKitPerfCase, name: &str) -> f64 {
    metric_peak(case, name) * 1000.0
}

fn launch_or_mount_display_ms(case: &UIKitPerfCase) -> String {
    if case.scenario == "launch-lifecycle" && case.metrics.contains_key("app_launch_s") {
        return metric_display_ms(case, "app_launch_s");
    }
    metric_display_ms(case, "signpost_screen_mount_s")
}

fn metric_display_ms(case: &UIKitPerfCase, name: &str) -> String {
    case.metrics
        .get(name)
        .map(|metric| format!("{:.3}", metric.median * 1000.0))
        .unwrap_or_else(|| String::from("`-`"))
}

fn delta_pct(current: f64, baseline: f64) -> f64 {
    if baseline == 0.0 {
        return 0.0;
    }
    ((current - baseline) / baseline) * 100.0
}

fn locate_workspace_root() -> Result<PathBuf> {
    // xtask is at <root>/xtask. Walk up until we find Cargo.toml containing [workspace]
    let mut p = std::env::current_dir()?;
    for _ in 0..5 {
        let ct = p.join("Cargo.toml");
        if ct.exists() {
            let s = fs::read_to_string(&ct)?;
            if s.contains("[workspace]") {
                return Ok(p);
            }
        }
        if !p.pop() {
            break;
        }
    }
    bail!("workspace root not found")
}

pub fn resolve_built_uikit_app(derived_data_path: &Path) -> Result<BuiltUIKitApp> {
    let products_root = derived_data_path.join("Build/Products");
    let mut app_paths = Vec::new();
    collect_app_bundles(&products_root, &mut app_paths)?;
    if app_paths.is_empty() {
        bail!(
            "no built .app bundle was found under {}; run `cargo xtask ios device-perf` again after a successful build-for-testing pass",
            products_root.display()
        );
    }

    let mut matches = Vec::new();
    for app_path in app_paths {
        let info_plist_path = app_path.join("Info.plist");
        let Some(dict) = read_plist_dict(&info_plist_path) else {
            continue;
        };
        let Some(bundle_identifier) = plist_string(&dict, "CFBundleIdentifier") else {
            continue;
        };
        let Some(executable_name) = plist_string(&dict, "CFBundleExecutable") else {
            continue;
        };
        if bundle_identifier.ends_with(".UITests") || bundle_identifier.ends_with(".xctrunner") {
            continue;
        }
        matches.push(BuiltUIKitApp {
            app_path,
            info_plist_path,
            bundle_identifier,
            executable_name,
        });
    }

    match matches.len() {
        0 => bail!(
            "found built app bundles under {}, but none exposed a usable CFBundleIdentifier/CFBundleExecutable pair",
            products_root.display()
        ),
        1 => Ok(matches.remove(0)),
        _ => {
            let listed = matches
                .into_iter()
                .map(|app| format!("{} ({})", app.bundle_identifier, app.app_path.display()))
                .collect::<Vec<_>>()
                .join(", ");
            bail!(
                "multiple built app bundles matched under {}; tighten the selection logic before tracing: {}",
                products_root.display(),
                listed
            )
        }
    }
}

fn resolve_built_xctestrun_path(derived_data_path: &Path, scheme_name: &str) -> Result<PathBuf> {
    let products_root = derived_data_path.join("Build/Products");
    let entries = fs::read_dir(&products_root)
        .with_context(|| format!("reading {}", products_root.display()))?;
    let mut matches = Vec::new();
    for entry in entries {
        let entry = entry.with_context(|| format!("reading {}", products_root.display()))?;
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();
        if !is_primary_built_xctestrun_file(&file_name, scheme_name) {
            continue;
        }
        matches.push(path);
    }

    match matches.len() {
        0 => bail!(
            "no built .xctestrun bundle matching `{}` was found under {}; rerun build-for-testing first",
            scheme_name,
            products_root.display()
        ),
        1 => Ok(matches.remove(0)),
        _ => {
            matches.sort();
            let listed = matches
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            bail!(
                "multiple built .xctestrun files matched `{}` under {}; tighten the selection logic before running device tests: {}",
                scheme_name,
                products_root.display(),
                listed
            )
        }
    }
}

pub fn is_primary_built_xctestrun_file(file_name: &str, scheme_name: &str) -> bool {
    file_name.starts_with(scheme_name)
        && file_name.ends_with(".xctestrun")
        && !file_name.ends_with("-perf.xctestrun")
}

fn prepare_react_device_perf_xctestrun(source_path: &Path) -> Result<PathBuf> {
    let stem = source_path
        .file_stem()
        .and_then(|value| value.to_str())
        .with_context(|| format!("missing xctestrun file stem for {}", source_path.display()))?;
    let output_path = source_path.with_file_name(format!("{}-perf.xctestrun", stem));
    let mut plist_value: PlValue = plist::from_file(source_path)
        .with_context(|| format!("reading {}", source_path.display()))?;
    apply_xctestrun_environment_overrides(
        &mut plist_value,
        DEFAULT_REACT_DEVICE_TEST_TARGET,
        &react_device_perf_environment(),
    )?;
    plist::to_file_xml(&output_path, &plist_value)
        .with_context(|| format!("writing {}", output_path.display()))?;
    Ok(output_path)
}

pub fn apply_xctestrun_environment_overrides(
    xctestrun: &mut PlValue,
    test_target: &str,
    environment: &[(String, String)],
) -> Result<()> {
    let root = xctestrun
        .as_dictionary_mut()
        .with_context(|| "xctestrun plist root must be a dictionary")?;
    let target = root
        .get_mut(test_target)
        .and_then(PlValue::as_dictionary_mut)
        .with_context(|| format!("missing `{}` target entry in xctestrun plist", test_target))?;
    for section_name in ["EnvironmentVariables", "TestingEnvironmentVariables"] {
        if !target.contains_key(section_name) {
            target.insert(String::from(section_name), PlValue::Dictionary(Dictionary::new()));
        }
        let section =
            target.get_mut(section_name).and_then(PlValue::as_dictionary_mut).with_context(
                || format!("`{}` must be a dictionary in xctestrun plist", section_name),
            )?;
        for (key, value) in environment {
            section.insert(key.clone(), PlValue::String(value.clone()));
        }
    }
    Ok(())
}

fn collect_app_bundles(root: &Path, app_paths: &mut Vec<PathBuf>) -> Result<()> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            return Err(err)
                .with_context(|| format!("reading build products under {}", root.display()))
        }
    };

    for entry in entries {
        let entry = entry.with_context(|| format!("reading {}", root.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("reading file type for {}", path.display()))?;
        if !file_type.is_dir() {
            continue;
        }
        if path.extension().and_then(|value| value.to_str()) == Some("app") {
            app_paths.push(path);
            continue;
        }
        collect_app_bundles(&path, app_paths)?;
    }

    Ok(())
}

fn read_plist_dict(path: &Path) -> Option<Dictionary> {
    let v: PlValue = plist::from_file(path).ok()?;
    match v {
        PlValue::Dictionary(d) => Some(d),
        _ => None,
    }
}

fn plist_string(dict: &Dictionary, key: &str) -> Option<String> {
    dict.get(key).and_then(PlValue::as_string).map(str::to_string)
}

pub fn merge_usage_strings(info: &mut Dictionary, usage: &BTreeMap<String, String>) {
    for (k, v) in usage {
        info.insert(k.clone(), PlValue::String(v.clone()));
    }
}

pub fn merge_background_modes(info: &mut Dictionary, ent: &Entitlements) {
    let mut modes: Vec<String> = Vec::new();
    if ent.background_remote_notification {
        modes.push("remote-notification".into());
    }
    if ent.background_fetch {
        modes.push("fetch".into());
    }
    if ent.background_processing {
        modes.push("processing".into());
    }
    if ent.bluetooth_central {
        modes.push("bluetooth-central".into());
    }
    if ent.bluetooth_peripheral {
        modes.push("bluetooth-peripheral".into());
    }
    if !modes.is_empty() {
        let arr = PlValue::Array(modes.into_iter().map(PlValue::String).collect());
        info.insert("UIBackgroundModes".into(), arr);
    }
}

pub fn build_entitlements_dict(e: &Entitlements) -> Dictionary {
    let mut d = Dictionary::new();
    if e.push_notifications {
        d.insert("aps-environment".into(), PlValue::String("development".into()));
    }
    // Spec requests Bluetooth roles under entitlements (engine will gate APIs regardless)
    if e.bluetooth_central || e.bluetooth_peripheral {
        let mut roles: Vec<PlValue> = Vec::new();
        if e.bluetooth_central {
            roles.push(PlValue::String("central".into()));
        }
        if e.bluetooth_peripheral {
            roles.push(PlValue::String("peripheral".into()));
        }
        d.insert("com.apple.developer.bluetooth".into(), PlValue::Array(roles));
    }
    d
}

fn validate_usage(c: &CapabilitiesToml) -> Result<()> {
    let u = &c.usage_strings;
    // Required keys for chosen capabilities
    if c.entitlements.bluetooth_central && !u.contains_key("NSBluetoothAlwaysUsageDescription") {
        bail!("Missing NSBluetoothAlwaysUsageDescription for bluetooth_central=true");
    }
    if c.entitlements.bluetooth_peripheral
        && !u.contains_key("NSBluetoothPeripheralUsageDescription")
    {
        bail!("Missing NSBluetoothPeripheralUsageDescription for bluetooth_peripheral=true");
    }
    match c.entitlements.location {
        LocationMode::None => {}
        LocationMode::WhenInUse => {
            if !u.contains_key("NSLocationWhenInUseUsageDescription") {
                bail!("Missing NSLocationWhenInUseUsageDescription for location=when_in_use");
            }
        }
        LocationMode::Always => {
            if !u.contains_key("NSLocationAlwaysAndWhenInUseUsageDescription") {
                bail!("Missing NSLocationAlwaysAndWhenInUseUsageDescription for location=always");
            }
        }
    }
    Ok(())
}

pub fn build_and_bundle_shaders(root: &Path, app_dir: &Path) -> Result<()> {
    let shaders = root.join("crates/renderer-metal/shaders");
    if !shaders.exists() {
        return Ok(());
    }
    // Ensure resources dir
    let res_dir = app_dir.join("Resources");
    fs::create_dir_all(&res_dir).with_context(|| format!("creating {}", res_dir.display()))?;

    // Determine SDK
    let target = std::env::var("TARGET").unwrap_or_default();
    let sdk = if target.contains("apple-ios") {
        if target.contains("sim") {
            "iphonesimulator"
        } else {
            "iphoneos"
        }
    } else {
        "iphoneos"
    };

    // Compile all .metal to .air
    let mut airs: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(&shaders).with_context(|| "reading shaders dir")? {
        let entry = entry?;
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) == Some("metal") {
            let stem = p.file_stem().unwrap().to_string_lossy().to_string();
            let air = res_dir.join(format!("{stem}.air"));
            let status = std::process::Command::new("xcrun")
                .args(["-sdk", sdk, "metal", "-c"])
                .arg(&p)
                .args(["-o"])
                .arg(&air)
                .status()?;
            if !status.success() {
                bail!("metal compile failed for {}", p.display());
            }
            airs.push(air);
        }
    }
    if airs.is_empty() {
        return Ok(());
    }
    // Link metallib
    let metallib = res_dir.join("default.metallib");
    let mut cmd = std::process::Command::new("xcrun");
    cmd.args(["-sdk", sdk, "metallib"]).args(airs.iter().map(|p| p.as_os_str()));
    cmd.arg("-o").arg(&metallib);
    let status = cmd.status()?;
    if !status.success() {
        bail!("metallib link failed");
    }
    // Cleanup .air files
    for a in airs {
        let _ = fs::remove_file(a);
    }
    Ok(())
}
