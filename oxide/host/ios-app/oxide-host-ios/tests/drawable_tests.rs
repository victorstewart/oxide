use oxide_host_ios::{
    oxide_host_app_frame_with_drawable, oxide_host_app_request_redraw,
    oxide_host_app_wake_generation,
};

#[test]
fn frame_with_drawable_stub() {
    assert_eq!(oxide_host_app_frame_with_drawable(1, 1, 1.0, core::ptr::null_mut()), -1);
}

#[test]
fn explicit_redraw_advances_the_lock_free_wake_generation() {
    let before = oxide_host_app_wake_generation();
    oxide_host_app_request_redraw();
    assert!(oxide_host_app_wake_generation() > before);
}

#[test]
fn rapid_redraw_requests_are_not_lost_before_the_host_can_coalesce_them() {
    let before = oxide_host_app_wake_generation();
    for _ in 0..256 {
        oxide_host_app_request_redraw();
    }
    let advanced = oxide_host_app_wake_generation().wrapping_sub(before);
    assert!(advanced >= 256, "only {advanced} of 256 redraw generations were retained");
}

#[test]
fn ios_frame_wake_matrix_covers_host_owned_publication_paths() {
    let rust = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"));
    let objc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ios/app.m"));
    for marker in [
        "hub.update_reachability(snapshot);",
        "telemetry.update_permissions(app.permission_states.clone());",
        "pub extern \"C\" fn oxide_host_app_request_redraw()",
        "pub extern \"C\" fn oxide_host_app_will_enter_foreground()",
        "pub extern \"C\" fn oxide_host_on_memory_warning()",
        "pub extern \"C\" fn oxide_host_set_anim_play(play: u8)",
        "fn window_resized_cb(",
        "fn pointer_cb(",
        "fn touch_cb(",
    ] {
        assert!(rust.contains(marker), "missing wake source `{marker}`");
    }
    assert!(rust.contains("test_scenes::Router::wants_next_frame"));
    assert!(objc.contains("OxideCameraPreviewPublishDidAdvance"));
    assert!(objc.contains("[delegate updateDisplayLinkDemandState];"));
}

#[test]
fn ios_display_link_has_explicit_idle_pause_and_wake_ownership() {
    let objc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ios/app.m"));
    let rust = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"));
    let tick = objc.split("- (void)onTick:").nth(1).expect("display-link tick");
    let wake = objc
        .rsplit("- (void)requestDisplayLinkWake:")
        .next()
        .expect("display-link wake method")
        .split("- (void)pauseDisplayLinkForIdle")
        .next()
        .expect("wake method end");

    assert!(rust.contains("static FRAME_WAKE_GENERATION: AtomicU64"));
    assert!(rust.contains("fn request_frame_wake()"));
    assert!(rust.contains("oxide_host_request_display_link_wake(generation)"));
    assert!(rust.contains("fn mark_frame_dirty(app: &mut AppState)"));
    assert!(rust.contains("app.presented_wake_generation"));
    assert!(rust.contains("let wake_pending = FRAME_WAKE_GENERATION.load(Ordering::Acquire)"));
    assert!(objc.contains("gDisplayLinkWakeDispatchPending"));
    assert!(objc.contains("[self pauseDisplayLinkForIdle];"));
    assert!(tick.contains("[self updateDisplayLinkDemandState];"));
    assert!(wake.contains("self.displayLink.paused = NO;"));
    assert!(!wake.contains("oxide_host_app_should_render"));
}

#[test]
fn ios_display_link_observes_missed_wakes_and_preserves_lifecycle_pauses() {
    let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ios/app.m"));
    let tick = source.split("- (void)onTick:").nth(1).expect("display-link tick");
    let resign = source
        .split("- (void)sceneWillResignActive:")
        .nth(1)
        .expect("resign handler")
        .split("- (void)sceneDidEnterBackground:")
        .next()
        .expect("resign handler end");
    let background = source
        .split("- (void)sceneDidEnterBackground:")
        .nth(1)
        .expect("background handler")
        .split("- (void)sceneWillEnterForeground:")
        .next()
        .expect("background handler end");

    assert!(tick.contains("display_link_missed_wakeups += 1"));
    assert!(tick.contains("wakeGeneration > self.displayLinkWakeGeneration"));
    assert!(source.contains("@\"displayLinkMissedWakeups\""));
    assert!(resign.contains("self.displayLinkForegroundActive = NO;"));
    assert!(resign.contains("self.displayLink.paused = YES;"));
    assert!(background.contains("self.displayLinkForegroundActive = NO;"));
    assert!(background.contains("oxide_host_app_did_enter_background();"));
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
fn native_frame_coalescing_reuses_app_storage() {
    let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"));
    assert!(source.contains("coalesce_items: Vec<gfx_api::DrawCmd>"));
    assert!(source.contains("coalesce_adjacent_draws_reuse(dl, &mut app.coalesce_items)"));
    assert!(!source.contains("oxide_ui_core::coalesce_adjacent_draws(dl)"));
}

#[test]
fn native_damage_handoff_reuses_router_and_submit_storage() {
    let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"));
    assert!(source.contains("router.take_damage_into(&mut damage_rects)"));
    assert!(source.contains("damage_rects = damage.rects"));
    assert!(source.contains("app.pending_damage_rects = damage_rects"));
    assert!(!source.contains("router.take_damage()"));
}

#[test]
fn memory_warnings_purge_effect_targets_and_request_a_frame() {
    let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"));
    let warning = source
        .split("pub extern \"C\" fn oxide_host_on_memory_warning()")
        .nth(1)
        .expect("memory-warning handler");
    let warning = warning.split("// ===== Camera options control").next().expect("handler end");
    assert!(warning.contains("renderer.purge_effect_targets();"));
    assert!(warning.contains("renderer.purge_layer_cache_for_memory_warning();"));
    assert!(warning.contains("renderer.purge_prepared_chunks();"));
    assert!(warning.contains("renderer.purge_id_mask_field_cache();"));
    assert!(warning.contains("mark_frame_dirty(app);"));
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
