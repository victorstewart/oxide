use oxide_host_ios::oxide_host_app_frame_with_drawable;

#[test]
fn frame_with_drawable_stub() {
    assert_eq!(oxide_host_app_frame_with_drawable(1, 1, 1.0, core::ptr::null_mut()), -1);
}

#[test]
fn ios_tick_prepares_frame_before_acquiring_drawable() {
    let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ios/app.m"));
    let tick = source.split("- (void)onTick:").nth(1).expect("MetalView onTick implementation");
    let prepare = tick.find("oxide_host_app_prepare_frame").expect("prepare frame call");
    let acquire = tick.find("nextDrawable").expect("drawable acquisition");
    let submit = tick
        .find("oxide_host_app_submit_prepared_frame_with_drawable")
        .expect("prepared frame submit call");

    assert!(prepare < acquire, "iOS app host must prepare CPU frame work before nextDrawable");
    assert!(acquire < submit, "iOS app host must acquire the drawable immediately before submit");
    assert!(tick.contains("oxide_host_app_cancel_prepared_frame"));
}

#[test]
fn ios_metal_layer_uses_timeout_capable_drawable_acquisition() {
    let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ios/app.m"));

    assert!(
        source.contains("layer.allowsNextDrawableTimeout = YES;"),
        "iOS host must allow nextDrawable timeout so a prepared frame can be canceled instead of blocking indefinitely"
    );
}

#[test]
fn ios_perf_runtime_prepares_frame_before_acquiring_drawable() {
    let source = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../App/PerfShared/OxideUIKitBenchmarkRuntime.swift"
    ));
    let render = source
        .split("func renderFrame(signpost:")
        .nth(1)
        .expect("camera benchmark renderFrame implementation");
    let prepare = render.find("oxideHostAppPrepareFrame").expect("prepare frame call");
    let acquire = render.find("layer.nextDrawable").expect("drawable acquisition");
    let submit = render
        .find("oxideHostAppSubmitPreparedFrameWithDrawable")
        .expect("prepared frame submit call");

    assert!(prepare < acquire, "iOS perf runtime must prepare CPU frame work before nextDrawable");
    assert!(
        acquire < submit,
        "iOS perf runtime must acquire the drawable immediately before submit"
    );
    assert!(render.contains("oxideHostAppCancelPreparedFrame"));
}

#[test]
fn app_main_uses_declared_c_environment_apis_for_parked_perf_launch() {
    let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../App/main.m"));

    assert!(source.contains("#include <stdlib.h>"), "getenv/setenv/unsetenv must be declared");
    assert!(source.contains("NSProcessInfo.processInfo.environment"));
    assert!(source.contains("@\"OXIDE_PERF_PARKED\""));
    assert!(source.contains("@\"OxidePerfParkedAppDelegate\""));
}

#[test]
fn app_main_routes_restored_perf_scenes_through_mux_delegate() {
    let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../App/main.m"));
    let plist = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../App/Info.plist"));
    let rust_host = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ios/app.m"));

    assert!(source.contains("@interface OxideSceneMuxDelegate"));
    assert!(source.contains("OxideShouldUsePerfSceneDelegate()"));
    assert!(source.contains("gOxideLaunchPerfApp = launchPerfApp"));
    assert!(source.contains("@\"OxidePerfParkedSceneDelegate\""));
    assert!(source.contains("@\"RustSceneDelegate\""));
    assert!(plist.contains("<string>OxideSceneMuxDelegate</string>"));
    assert!(rust_host.contains("OxidePerfParkedBenchmarkLaunchEnabled"));
    assert!(rust_host.contains("parkedPerfSceneDelegateIfNeeded"));
    assert!(rust_host.contains("@\"OxidePerfParkedSceneDelegate\""));
}

#[test]
fn parked_perf_scene_holds_foreground_execution_for_device_gpu_runs() {
    let source =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../App/OxidePerfParkedApp.swift"));

    assert!(source.contains("previousIdleTimerDisabled"));
    assert!(source.contains("didFinishBenchmark"));
    assert!(source.contains("UIApplication.shared.isIdleTimerDisabled = true"));
    assert!(source.contains("restoreForegroundExecution()"));
    assert!(source.contains("publishReadyWhenForegroundActive"));
    assert!(source.contains("pendingReadyRetryScheduled"));
    assert!(source.contains("pendingReadyRetryCount < 300"));
    assert!(source.contains(
        "postDarwinNotification(readyNotificationName)\n        schedulePendingTraceAutostartIfNeeded()",
    ));
    assert!(source.contains("DispatchQueue.main.asyncAfter(deadline: .now() + 0.1)"));
    assert!(source.contains("window?.windowScene?.activationState == .foregroundActive"));
    assert!(source.contains("markForegroundFailure"));
    assert!(source.contains("requiresForegroundHandshake"));
    assert!(source.contains("sceneWillResignActive"));
    assert!(source.contains("didRunBenchmark || requiresForegroundHandshake()"));
    assert!(source.contains("DispatchQueue.main.asyncAfter(deadline: .now() + 0.25)"));
    assert!(source.contains("failed - parked benchmark lost active foreground state"));
    assert!(source.contains("failed - parked benchmark entered background"));
}
