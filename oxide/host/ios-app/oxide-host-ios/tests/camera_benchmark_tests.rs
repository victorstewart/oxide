use oxide_host_ios::{
    merge_camera_contract_fields, oxide_host_app_frame, oxide_host_app_init,
    oxide_host_app_shutdown, oxide_host_app_stats, oxide_host_camera_preview_plan,
    oxide_host_current_scene, oxide_host_set_benchmark_mode, oxide_host_set_camera_render_mode,
    oxide_host_set_camera_texture_source, oxide_host_set_scene, OxideHostStats,
};
use std::sync::{Mutex, OnceLock};

#[unsafe(no_mangle)]
extern "C" fn oxide_host_resource_read(
    _name: *const core::ffi::c_char,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    unsafe {
        if !out_ptr.is_null() {
            *out_ptr = core::ptr::null_mut();
        }
        if !out_len.is_null() {
            *out_len = 0;
        }
    }
    0
}

#[unsafe(no_mangle)]
extern "C" fn oxide_host_string_free(_ptr: *mut u8) {}

fn zeroed_host_stats() -> OxideHostStats {
    // `OxideHostStats` is a repr(C) aggregate of numeric fields, so a zeroed
    // value is a valid baseline for out-parameter tests.
    unsafe { core::mem::zeroed() }
}

fn test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn lock_tests() -> std::sync::MutexGuard<'static, ()> {
    test_lock().lock().expect("test lock")
}

fn source_between<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split(start)
        .nth(1)
        .unwrap_or_else(|| panic!("missing source marker `{}`", start))
        .split(end)
        .next()
        .unwrap_or_else(|| panic!("missing source end marker `{}`", end))
}

fn init_benchmark_camera_scene() {
    oxide_host_app_shutdown();
    assert_eq!(oxide_host_set_benchmark_mode(1), 0);
    assert_eq!(oxide_host_set_camera_render_mode(1), 0);
    assert_eq!(oxide_host_set_camera_texture_source(1), 0);
    assert_eq!(oxide_host_app_init(390, 844, 3.0), 0);
    assert_eq!(oxide_host_set_scene(10), 0);
}

fn shutdown_benchmark_camera_scene() {
    assert_eq!(oxide_host_set_camera_texture_source(0), 0);
    assert_eq!(oxide_host_set_camera_render_mode(0), 0);
    oxide_host_app_shutdown();
}

#[test]
fn avfoundation_preview_layer_transport_stays_benchmark_diagnostic_only() {
    let source = include_str!("../src/ios/app.m");

    assert!(
        !source.contains("NativeCameraPreview") && !source.contains("draw_native_camera_preview"),
        "product camera preview must stay on Oxide-owned draw commands, not a native visible-preview API"
    );
    assert!(source.contains("@interface OxidePerfCameraPreviewView"));
    assert!(!source.contains("@interface OxideCameraPreviewView"));

    let preview_view = source_between(
        source,
        "@interface OxidePerfCameraPreviewView",
        "@interface RustSceneDelegate",
    );
    assert!(
        preview_view.contains("AVCaptureVideoPreviewLayer")
            && preview_view.contains("@implementation OxidePerfCameraPreviewView"),
        "AVCaptureVideoPreviewLayer must remain isolated to the explicitly perf-named view"
    );

    let avfoundation_enabled = source_between(
        source,
        "static BOOL OxidePerfActualAppAVFoundationCameraBenchmarkEnabled(void)",
        "static BOOL OxidePerfActualAppCameraBenchmarkEnabled(void)",
    );
    assert!(avfoundation_enabled.contains("OxidePerfCameraRealAppHostEnabled()"));
    assert!(avfoundation_enabled.contains("testCameraAVFoundationPreviewLayerRealAppLivePreview"));

    let scene_install =
        source_between(source, "BOOL useAVFoundationVisiblePreview =", "vc.view = container;");
    assert!(scene_install.contains("OxidePerfActualAppAVFoundationCameraBenchmarkEnabled();"));
    assert!(scene_install.contains("if (!useAVFoundationVisiblePreview)"));
    assert!(scene_install.contains("mv = [MetalView new];"));
    assert!(
        scene_install.contains(
            "OxidePerfCameraRealAppHybridVisiblePreviewEnabled() ||\n        useAVFoundationVisiblePreview"
        ),
        "preview-layer view installation must require an explicit AVFoundation or hybrid perf mode"
    );
    assert!(scene_install.contains("[OxidePerfCameraPreviewView new]"));

    let configure = source_between(
        source,
        "- (void)configureActualAppCameraBenchmarkIfNeeded {",
        "int32_t sceneIndex = OxideResolveSceneIndexNamed(\"Camera\");",
    );
    assert!(configure
        .contains("if (!OxidePerfActualAppBenchmarkEnabled() || self.perfBenchmarkConfigured)"));
    assert!(configure.contains("if (OxidePerfActualAppAVFoundationCameraBenchmarkEnabled())"));
    assert!(configure.contains("configureActualAppAVFoundationSessionIfNeeded"));

    let custom = source_between(
        source,
        "int32_t sceneIndex = OxideResolveSceneIndexNamed(\"Camera\");",
        "- (void)handleActualAppBenchmarkStart {",
    );
    assert!(custom.contains("oxide_host_set_benchmark_mode(1);"));
    assert!(custom.contains("oxide_host_set_camera_texture_source(0);"));
    assert!(custom.contains("oxide_host_set_camera_running_mode(1, 1);"));
    assert!(
        !custom.contains("previewLayer.session = session"),
        "shipping-oriented custom app-host camera path must not bind AVCaptureVideoPreviewLayer"
    );

    let hybrid_tick = source_between(
        source,
        "if (OxidePerfCameraRealAppHybridVisiblePreviewEnabled()) {",
        "if (rc_plan == 0) {",
    );
    assert!(hybrid_tick.contains("[self bindPerfCameraPreviewLayerIfNeeded];"));
    assert!(hybrid_tick.contains("return;"));
}

#[test]
fn uikit_preview_layer_cases_are_labeled_as_baseline_or_diagnostic() {
    let source = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../App/PerfShared/OxideUIKitBenchmarkRuntime.swift"
    ));
    let catalog =
        source_between(source, "switch normalizedTestName", "case \"testCollectionViewEncode\":");

    assert!(catalog.contains("case \"testCameraNV12LegacyLivePreview\":"));
    assert!(catalog.contains("case \"testCameraNV12LegacyHybridPreviewLayerLivePreview\":"));
    assert!(catalog.contains("case \"testCameraNV12LegacyRealAppHybridPreviewLayerLivePreview\":"));
    assert!(catalog.contains("case \"testCameraAVFoundationPreviewLayerLivePreview\":"));
    assert!(catalog.contains("case \"testCameraAVFoundationPreviewLayerSidecarLivePreview\":"));

    let parked_hybrid = source_between(
        catalog,
        "case \"testCameraNV12LegacyHybridPreviewLayerLivePreview\":",
        "case \"testCameraNV12LegacyRealAppLivePreview\":",
    );
    assert!(parked_hybrid.contains("visibleTransport: .avFoundationPreviewLayer"));

    let real_app_hybrid = source_between(
        catalog,
        "case \"testCameraNV12LegacyRealAppHybridPreviewLayerLivePreview\":",
        "case \"testCameraAVFoundationPreviewLayerLivePreview\":",
    );
    assert!(real_app_hybrid.contains("visibleTransport: .avFoundationPreviewLayer"));

    let avfoundation_baseline = source_between(
        catalog,
        "case \"testCameraAVFoundationPreviewLayerLivePreview\":",
        "case \"testCameraAVFoundationPreviewLayerSidecarLivePreview\":",
    );
    assert!(avfoundation_baseline.contains("makeAVFoundationPreviewBenchmark"));

    let avfoundation_sidecar = source_between(
        source,
        "case \"testCameraAVFoundationPreviewLayerSidecarLivePreview\":",
        "case \"testCollectionViewEncode\":",
    );
    assert!(avfoundation_sidecar.contains("includeVideoDataOutputSidecar: true"));

    let xtask = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../xtask/src/lib.rs"));
    assert!(
        xtask.contains("Official parked microscope AVFoundation baseline")
            && xtask.contains("Diagnostic-only hybrid live camera preview")
            && xtask.contains("Diagnostic-only actual app-host hybrid camera preview"),
        "UIKit preview-layer cases must stay explicitly labeled as baseline or diagnostic-only"
    );
}

#[test]
fn actual_app_frame_driven_scheduling_installs_callback_before_camera_start() {
    let source = include_str!("../src/ios/app.m");
    let perf_branch = source
        .split("if (IsRunningPerfBenchmarkHost()) {")
        .nth(1)
        .expect("perf host scene branch")
        .split("gAppDebugPerf.normal_scene_branch_calls += 1;")
        .next()
        .expect("normal scene branch marker");
    let install_pos = perf_branch
        .find("[self installCameraDrivenSchedulingCallbackIfNeeded];")
        .expect("perf host installs frame-driven callback");
    let configure_pos = perf_branch
        .find("[self configureActualAppCameraBenchmarkIfNeeded];")
        .expect("perf host configures actual app camera benchmark");
    assert!(
        install_pos < configure_pos,
        "frame-driven camera scheduling must be armed before the perf camera starts"
    );

    let helper = source
        .split("- (void)installCameraDrivenSchedulingCallbackIfNeeded {")
        .nth(1)
        .expect("camera-driven scheduling helper")
        .split("\n}")
        .next()
        .expect("helper terminator");
    assert!(helper.contains("gActiveRustSceneDelegate = self;"));
    assert!(helper
        .contains("oxide_cam_set_preview_publish_callback(OxideCameraPreviewPublishDidAdvance"));
}

#[test]
fn benchmark_camera_scene_uses_minimal_preview_draw_list() {
    let _guard = lock_tests();
    init_benchmark_camera_scene();
    assert_eq!(oxide_host_current_scene(), 10);
    assert_eq!(oxide_host_app_frame(390, 844, 3.0), 0);
    assert_eq!(oxide_host_app_frame(390, 844, 3.0), 0);

    let mut stats = zeroed_host_stats();
    assert_eq!(oxide_host_app_stats(&mut stats), 0);
    assert_eq!(stats.draws, 1);
    assert_eq!(stats.anims, 0);
    assert_eq!(stats.damage_rects, 0);
    assert_eq!(stats.cam_blur_updates, 0);
    assert_eq!(stats.cam_update_period_ms, 0);
    assert_eq!(stats.cam_paused, 0);
    assert!(stats.cam_width > 0);
    assert!(stats.cam_height > 0);
    assert!(stats.cam_coverage_pct > 0.0);
    assert!(stats.cam_fetch_ms >= 0.0);
    assert!(stats.cam_setup_ms >= 0.0);
    assert!(stats.cam_encode_quad_ms >= 0.0);
    assert!(stats.cam_command_buffer_ms >= 0.0);
    assert!(stats.cam_encoder_ms >= 0.0);
    assert!(stats.cam_encode_bind_ms >= 0.0);
    assert!(stats.cam_encode_draw_ms >= 0.0);
    assert!(stats.cam_end_encoding_ms >= 0.0);
    assert!(stats.cam_commit_ms >= 0.0);
    assert!(stats.cam_present_ms >= 0.0);
    assert!(stats.cam_gpu_ms >= 0.0);
    assert!(stats.cam_gpu_render_ms >= 0.0);
    assert!(stats.cam_gpu_vertex_ms >= 0.0);
    assert!(stats.cam_gpu_fragment_ms >= 0.0);
    assert!(stats.renderer_gpu_ms >= 0.0);
    assert!(stats.renderer_gpu_render_ms >= 0.0);
    assert!(stats.renderer_gpu_vertex_ms >= 0.0);
    assert!(stats.renderer_gpu_fragment_ms >= 0.0);
    assert!(stats.cam_capture_sample_setup_ms >= 0.0);
    assert!(stats.cam_capture_frame_delivery_ms >= 0.0);
    assert!(stats.renderer_memory_total_bytes > 0);
    assert!(stats.renderer_memory_buffer_bytes > 0);
    assert!(stats.renderer_memory_benchmark_camera_bytes > 0);
    assert!(stats.renderer_memory_total_bytes >= stats.renderer_memory_buffer_bytes);
    assert!(stats.renderer_memory_total_bytes >= stats.renderer_memory_benchmark_camera_bytes);

    shutdown_benchmark_camera_scene();
}

#[test]
fn ios_manual_touch_path_uses_raw_events_and_recognizer_fallback() {
    let source = include_str!("../src/ios/app.m");
    assert!(
        source.contains("self.multipleTouchEnabled = YES;")
            && source.contains("for (UITouch *t in touches)")
            && source.contains("[self emitTouch:t phase:"),
        "UIKit should only enable multi-touch and forward raw UITouch events into Oxide"
    );
    assert!(
        source.contains("@interface OxideTouchWindow : UIWindow")
            && source.contains("- (void)sendEvent:(UIEvent *)event")
            && source.contains("event.allTouches")
            && source.contains("window touch emit")
            && source.contains("case UITouchPhaseStationary")
            && source.contains("[[OxideTouchWindow alloc] initWithWindowScene:ws]"),
        "every iOS Oxide app should capture UIEvent.allTouches at the window boundary before view hit-testing can lose samples"
    );
    assert!(
        source.contains("touchesBegan skipped window touch capture active")
            && source.contains("touchesMoved skipped window touch capture active"),
        "view-level touch handlers should not double-emit when Oxide window capture is active"
    );
    assert!(
        source.contains("OxideEventHasOnlyDirectTouches(event)")
            && source.contains("touch recognizer shouldReceiveEvent blocked direct touches")
            && source.contains("touch recognizer shouldReceiveTouch blocked direct touch"),
        "recognizer fallback should not double-emit direct touches already captured by the Oxide window"
    );
    assert!(
        source.contains("@interface OxideApplication : UIApplication")
            && source.contains("touch-debug application sendEvent")
            && source.contains("touch-debug window sendEvent")
            && source.contains("NSStringFromClass([OxideApplication class])"),
        "manual touch debugging must log app-level and window-level event dispatch before recognizers or views can drop input"
    );
    assert!(
        source.contains(
            "- (BOOL)gestureRecognizerShouldBegin:(UIGestureRecognizer *)gestureRecognizer"
        ) && source.contains("return YES;"),
        "manual Simulator gestures need recognizer fallback enabled"
    );
    assert!(
        !source.contains("finishPinchWithActiveTouches")
            && !source.contains("pinchDistanceForActiveTouches")
            && !source.contains("lastPinchDistance"),
        "drag and pinch state must live in Rust, not Objective-C"
    );
    assert!(
        source.contains("oxide_host_emit_pan_gesture(p.x, p.y, d.x, d.y, 1);")
            && source.contains("oxide_host_emit_pinch(c.x, c.y, delta);"),
        "recognizers may only forward OS deltas into Rust-owned gestures"
    );
    assert!(
        source.contains("@(UITouchTypeIndirectPointer)")
            && source.contains("pan.allowedScrollTypesMask = UIScrollTypeMaskAll;"),
        "manual Simulator mouse/trackpad gestures must accept indirect pointer and scroll input"
    );
    assert!(
        source.contains("shouldReceiveEvent:(UIEvent *)event")
            && source.contains("touch recognizer shouldReceiveEvent")
            && source.contains("touch emit generic"),
        "manual touch debugging must log recognizer receipt and raw Objective-C touch emits"
    );
    assert!(
        source.contains("oxide-touch.log")
            && source.contains("OxideTouchFileLog")
            && source.contains("rust %@"),
        "manual touch debugging must persist Objective-C and Rust boundary logs into the Simulator app container"
    );
    assert!(
        source.contains("OxideTouchScreenLogEnabled")
            && source.contains("OxideUiScreenLogEnabled")
            && source.contains("if (!OxideUiScreenLogEnabled())"),
        "touch diagnostics should write to file without covering the app screen"
    );
}

#[test]
fn benchmark_camera_preview_plan_requires_first_drawable() {
    let _guard = lock_tests();
    init_benchmark_camera_scene();

    assert_eq!(oxide_host_camera_preview_plan(390, 844, 3.0), 1);

    shutdown_benchmark_camera_scene();
}

#[test]
fn merge_camera_contract_fields_prefers_backend_contract_over_rotated_preview_stats() {
    let _guard = lock_tests();
    let (width, height, fps, video_range, color_space) =
        merge_camera_contract_fields(720, 1280, 0.0, 0, 0, 1280, 720, 30.0, 0, 0);

    assert_eq!(width, 1280);
    assert_eq!(height, 720);
    assert_eq!(fps, 30.0);
    assert_eq!(video_range, 0);
    assert_eq!(color_space, 0);
}
