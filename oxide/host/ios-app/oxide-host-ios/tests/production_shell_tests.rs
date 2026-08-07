fn assert_only_accessibility_opt_outs(source: &str)
{
   let element_opt_out = "self.isAccessibilityElement = NO;";
   let subtree_opt_out = "self.accessibilityElementsHidden = YES;";
   assert_eq!(source.matches(element_opt_out).count(), 2);
   assert_eq!(source.matches(subtree_opt_out).count(), 2);
   for line in source
      .lines()
      .filter(|line| line.to_ascii_lowercase().contains("accessibility"))
   {
      let line = line.trim();
      assert!(
         line == element_opt_out || line == subtree_opt_out,
         "unexpected production accessibility surface: {line}",
      );
   }
}

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
   assert_only_accessibility_opt_outs(source);
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
