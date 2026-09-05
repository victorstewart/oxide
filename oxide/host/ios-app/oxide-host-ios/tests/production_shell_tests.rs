#[test]
fn build_selects_exactly_one_objective_c_host_by_feature()
{
   let build = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/build.rs"));

   assert!(build.contains("CARGO_FEATURE_TEST_SCENES_ENTRYPOINT"));
   assert!(build.contains("let app_source = if legacy_host"));
   assert!(build.contains("\"src/ios/app.m\""));
   assert!(build.contains("\"src/ios/product_app.m\""));
   assert!(build.contains(".file(app_source)"));
   assert!(build.contains("if legacy_host && std::env::var_os(\"CARGO_FEATURE_PERF_HOST_STUBS\")"));
   assert!(!build.contains(".file(\"src/ios/app.m\")"));
}

#[test]
fn product_host_is_a_bounded_injection_shell()
{
   let source = include_str!(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/src/ios/product_app.m"
   ));
   let lowercase = source.to_ascii_lowercase();

   assert!(source.lines().count() <= 1_000);
   for forbidden in [
      "AVCapture",
      "UISegmentedControl",
      "UIStackView",
      "oxide_host_perf",
      "oxide_host_scene_",
      "oxide_host_set_camera_",
      "OxidePerf",
      "UITest",
   ]
   {
      assert!(!source.contains(forbidden), "production host contains {forbidden}");
   }
   assert!(!lowercase.contains("accessibility"));
   assert!(!lowercase.contains("benchmark"));
}

#[test]
fn product_host_retains_only_required_os_boundaries()
{
   let source = include_str!(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/src/ios/product_app.m"
   ));

   for required in [
      "@interface OxideTouchWindow : UIWindow",
      "event.allTouches",
      "((UIPressesEvent *)event).allPresses",
      "UITextView <UITextViewDelegate>",
      "oxide_host_emit_pointer",
      "oxide_host_emit_key",
      "oxide_host_emit_text_composition",
      "oxide_host_emit_ime_shown",
      "oxide_host_app_did_enter_background",
      "oxide_host_app_will_enter_foreground",
      "oxide_host_app_will_terminate",
      "oxide_host_request_display_link_wake",
      "oxide_cam_set_preview_publish_callback",
      "camera_preview_did_advance",
      "oxide_host_request_redraw",
      "oxide_host_set_high_refresh",
      "oxide_host_display_link_frame_rate_range",
      "oxide_host_environment_transition_counts",
   ]
   {
      assert!(source.contains(required), "production host is missing {required}");
   }
   assert!(!source.contains("oxide_host_resource_read"));

   let services = include_str!(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/../../../crates/platform-ios/src/ios/host_services.m"
   ));
   for required in [
      "oxide_host_set_idle_timer_disabled",
      "oxide_host_open_system_settings",
      "oxide_host_open_external_url",
      "oxide_host_max_framerate_hz",
      "oxide_host_native_scale",
      "oxide_host_supports_edr",
      "oxide_host_is_simulation",
      "oxide_host_standard_path",
   ]
   {
      assert!(services.contains(required), "shared host services are missing {required}");
   }
}

#[test]
fn product_key_bridge_emits_only_key_down_and_repeat_events()
{
   let source = include_str!(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/src/ios/product_app.m"
   ));
   let key_bridge = source
      .split("- (void)emitKeyPress:(UIPress *)press {")
      .nth(1)
      .expect("product key bridge")
      .split("\n- (void)sendEvent:")
      .next()
      .expect("product key bridge terminator");

   assert!(key_bridge.contains("case UIPressPhaseEnded:\n  case UIPressPhaseCancelled:\n    return;"));
   assert!(!key_bridge.contains("repeat = 2"));

   let rust = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"));
   assert!(!rust.contains("from_utf8_unchecked"));
}

#[test]
fn product_camera_publication_wake_is_bound_to_foreground_lifecycle()
{
   let source = include_str!(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/src/ios/product_app.m"
   ));

   assert!(source.contains(
      "oxide_cam_set_preview_publish_callback(camera_preview_did_advance);"
   ));
   for lifecycle in [
      "- (void)sceneWillResignActive:(UIScene *)scene {",
      "- (void)sceneDidEnterBackground:(UIScene *)scene {",
      "- (void)sceneDidDisconnect:(UIScene *)scene {",
      "- (void)applicationWillTerminate:(UIApplication *)application {",
   ]
   {
      let body = source
         .split(lifecycle)
         .nth(1)
         .unwrap_or_else(|| panic!("missing lifecycle callback {lifecycle}"))
         .split("\n- (void)")
         .next()
         .expect("lifecycle callback terminator");
      assert!(body.contains("oxide_cam_set_preview_publish_callback(NULL);"));
   }
}

#[test]
fn legacy_host_contains_no_production_injection_branch()
{
   let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ios/app.m"));
   for forbidden in [
      "oxide_host_is_injected_app",
      "oxide_host_request_redraw(void)",
      "OxideProductSceneDelegate",
      "OxideProductAppDelegate",
   ]
   {
      assert!(!source.contains(forbidden), "legacy host contains {forbidden}");
   }
}

#[test]
fn product_host_counts_transient_environment_changes()
{
   let source = include_str!(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/src/ios/product_app.m"
   ));

   for required in [
      "NSProcessInfoThermalStateDidChangeNotification",
      "NSProcessInfoPowerStateDidChangeNotification",
      "atomic_fetch_add_explicit(&gThermalStateChanges",
      "atomic_fetch_add_explicit(&gLowPowerModeChanges",
   ]
   {
      assert!(source.contains(required), "production host is missing {required}");
   }
}

#[test]
fn display_link_observation_is_unavailable_off_ios()
{
   assert_eq!(oxide_host_ios::display_link_frame_rate_range(), None);
}

#[test]
fn display_link_range_uses_a_thread_safe_native_snapshot()
{
   for path in ["/src/ios/product_app.m", "/src/ios/app.m"]
   {
      let (source, getter_end) = match path
      {
         "/src/ios/product_app.m" => (
            include_str!(concat!(
               env!("CARGO_MANIFEST_DIR"),
               "/src/ios/product_app.m"
            )),
            "void oxide_host_environment_transition_counts",
         ),
         _ => (
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ios/app.m")),
            "static void OxideCameraPreviewPublishDidAdvance",
         ),
      };
      assert!(source.contains("static _Atomic(uint32_t) gDisplayLinkRangeHz = 0;"));
      let getter = source
         .split("int32_t oxide_host_display_link_frame_rate_range")
         .nth(1)
         .expect("display-link range getter")
         .split(getter_end)
         .next()
         .expect("display-link range getter end");
      assert!(getter.contains("atomic_load_explicit(&gDisplayLinkRangeHz"));
      assert!(!getter.contains("gActive"));
      assert!(!getter.contains(".displayLink"));
      assert!(!getter.contains("preferredFrameRateRange"));
      assert!(!getter.contains("preferredFramesPerSecond"));
      let updater = source
         .split("- (void)updateDisplayLinkRange {")
         .nth(1)
         .expect("display-link range updater")
         .lines()
         .take(25)
         .collect::<String>();
      assert!(updater.contains("atomic_store_explicit(&gDisplayLinkRangeHz"));
   }
}

#[test]
fn product_host_owns_exactly_one_window_scene()
{
   let source = include_str!(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/src/ios/product_app.m"
   ));
   let connect = source
      .split("willConnectToSession:(UISceneSession *)session")
      .nth(1)
      .expect("scene connection");
   let claim = connect.find("gOwnedSceneSession = session;").expect("scene claim");
   let window = connect.find("OxideTouchWindow *window").expect("owned window");

   assert!(source.contains("static __weak UISceneSession *gOwnedSceneSession = nil;"));
   assert!(connect.contains("gOwnedSceneSession != nil && gOwnedSceneSession != session"));
   assert!(connect.contains("requestSceneSessionDestruction:session"));
   assert!(claim < window);
   let disconnect = source
      .split("- (void)sceneDidDisconnect:(UIScene *)scene {")
      .nth(1)
      .expect("scene disconnect")
      .split("\n}")
      .next()
      .expect("scene disconnect end");
   assert!(disconnect.contains("if (gOwnedSceneSession == scene.session)"));
   assert!(disconnect.contains("gOwnedSceneSession = nil;"));
}

#[test]
fn product_init_waits_for_actual_window_metrics()
{
   let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"));
   let init = source
      .split("fn init_injected_app(app_state:")
      .nth(1)
      .expect("injected app init")
      .split("#[cfg(not(target_os = \"ios\"))]")
      .next()
      .expect("injected app init end");

   assert!(!init.contains("WindowEvent::Resized"));
   assert!(source.contains("extern \"C\" fn window_resized_cb("));
}

#[test]
fn tokio_spawn_api_and_runtime_installation_share_one_host_feature()
{
   let manifest = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));
   let platform_dependency = manifest
      .lines()
      .find(|line| line.starts_with("oxide-platform-ios ="))
      .expect("oxide-platform-ios dependency");
   assert!(platform_dependency.contains("default-features = false"));
   assert!(!platform_dependency.contains("tokio-runtime"));
   assert!(manifest.contains("default = []"));
   assert!(manifest.contains(
      "tokio-runtime = [\"oxide-platform-ios/tokio-runtime\"]"
   ));

   let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"));
   let run_app = source
      .split("pub unsafe fn run_app(")
      .nth(1)
      .expect("run_app")
      .split("pub extern \"C\" fn rust_entry")
      .next()
      .expect("run_app end");
   assert!(run_app.contains("#[cfg(feature = \"tokio-runtime\")]"));
   assert!(run_app.contains("oxide_platform_ios::init_tokio_spawn();"));
   assert!(source.contains("/// # Safety"));
   let off_ios = run_app
      .find("#[cfg(not(target_os = \"ios\"))]")
      .expect("off-iOS run_app branch");
   let install = run_app.find("if install_app(app).is_err()").expect("iOS app install");
   assert!(off_ios < install);

   let rust_entry = source
      .split("pub extern \"C\" fn rust_entry")
      .nth(1)
      .expect("rust_entry")
      .split("pub extern \"C\" fn oxide_host_is_injected_app")
      .next()
      .expect("rust_entry end");
   assert!(rust_entry.contains("#[cfg(feature = \"tokio-runtime\")]"));
   assert!(rust_entry.contains("oxide_platform_ios::init_tokio_spawn();"));
}

#[test]
fn legacy_rust_exports_and_state_are_feature_owned()
{
   let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"));
   for name in [
      "oxide_host_app_stats",
      "oxide_host_run_perf_suite",
      "oxide_host_perf_report_json_len",
      "oxide_host_scene_count",
      "oxide_host_set_benchmark_mode",
      "oxide_host_set_camera_render_mode",
      "oxide_host_prepare_onscreen_benchmark",
      "oxide_host_set_overlay_visible",
   ]
   {
      let marker = format!("pub extern \"C\" fn {name}");
      let offset = source.find(&marker).unwrap_or_else(|| panic!("missing {name}"));
      let prefix = &source[offset.saturating_sub(100)..offset];
      assert!(
         prefix.contains("#[cfg(feature = \"test-scenes-entrypoint\")]"),
         "{name} is not feature-owned"
      );
   }

   let state = source
      .split("struct AppState")
      .nth(1)
      .expect("AppState")
      .split("impl Default for AppState")
      .next()
      .expect("AppState end");
   for field in [
      "benchmark_scene_index",
      "benchmark_mode",
      "pending_frame_stats",
      "snapshot_status",
      "settle_frames_remaining",
   ]
   {
      let offset = state.find(field).unwrap_or_else(|| panic!("missing {field}"));
      let prefix = &state[offset.saturating_sub(80)..offset];
      assert!(
         prefix.contains("#[cfg(feature = \"test-scenes-entrypoint\")]"),
         "AppState::{field} is not feature-owned"
      );
   }
}

#[test]
fn injected_submit_feedback_and_retry_are_success_owned()
{
   let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"));
   let submit = source
      .split("pub extern \"C\" fn oxide_host_app_submit_prepared_frame_with_drawable")
      .nth(1)
      .expect("injected submit")
      .split("pub extern \"C\" fn oxide_host_app_cancel_prepared_frame")
      .next()
      .expect("submit end");
   let success = submit.find("Ok(stats) =>").expect("submit success arm");
   let feedback = submit
      .find("observe_injected_renderer_stats(&mut app, renderer_stats)")
      .expect("post-submit renderer feedback");
   let failure = submit
      .find("request_injected_frame_retry(&mut app)")
      .expect("submit failure retry");
   assert!(feedback > success);
   assert!(failure > feedback);

   let cancel = source
      .split("pub extern \"C\" fn oxide_host_app_cancel_prepared_frame")
      .nth(1)
      .expect("cancel")
      .split("fn oxide_host_app_frame_inner")
      .next()
      .expect("cancel end");
   assert!(cancel.contains("request_injected_frame_retry(&mut app)"));

   let observation_helper = source
      .split("fn observe_injected_renderer_stats")
      .nth(1)
      .expect("renderer observation helper")
      .split("fn dispatch_injected_event")
      .next()
      .expect("renderer observation helper end");
   assert!(observation_helper.contains("AppEvent::RendererStats(stats)"));
   assert!(!observation_helper.contains("request_frame_wake"));
   assert!(!observation_helper.contains("prepared_frame = false"));
}

#[test]
fn dormant_lifecycle_and_shutdown_do_not_retain_frame_work()
{
   let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"));
   let background = source
      .split("pub extern \"C\" fn oxide_host_app_did_enter_background")
      .nth(1)
      .expect("background lifecycle")
      .split("pub extern \"C\" fn oxide_host_app_will_enter_foreground")
      .next()
      .expect("background lifecycle end");
   let terminate = source
      .split("pub extern \"C\" fn oxide_host_app_will_terminate")
      .nth(1)
      .expect("termination lifecycle")
      .split("pub extern \"C\" fn oxide_host_on_memory_warning")
      .next()
      .expect("termination lifecycle end");
   for lifecycle in [background, terminate]
   {
      assert!(lifecycle.contains("dispatch_injected_event_without_wake"));
      assert!(!lifecycle.contains("request_frame_wake"));
   }

   let shutdown = source
      .split("pub extern \"C\" fn oxide_host_app_shutdown")
      .nth(1)
      .expect("app shutdown")
      .split("pub extern \"C\" fn oxide_host_set_benchmark_mode")
      .next()
      .expect("app shutdown end");
   for clear in [
      "app.pending_damage_rects.clear();",
      "app.prepared_frame = false;",
      "app.prepared_surface = None;",
      "app.prepared_retry_generation = None;",
      "POSTED_TASKS.get()",
      "lock_or_recover(tasks).clear();",
   ]
   {
      assert!(shutdown.contains(clear), "shutdown is missing {clear}");
   }
}
