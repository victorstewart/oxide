#[test]
fn macos_display_links_suspend_without_polling_rust_while_idle()
{
   let rust = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"));
   let objc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/macos/app.m"));
   let plist = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../App/Info.plist"));
   let modern_tick = objc
      .split("- (void)handleDisplayLink:")
      .nth(1)
      .expect("modern display-link handler")
      .split("@end")
      .next()
      .expect("modern display-link handler end");
   let wake_bridge = objc
      .split("void macos_host_request_display_link_wake(uint64_t generation)\n{")
      .nth(1)
      .expect("display-link wake bridge")
      .split("// Forward declaration for display link callback used below")
      .next()
      .expect("display-link wake bridge end");

   assert!(rust.contains("static FRAME_WAKE_GENERATION: AtomicU64"));
   assert!(rust.contains("app.presented_wake_generation"));
   assert!(rust.contains("pub extern \"C\" fn macos_app_request_redraw()"));
   assert!(objc.contains("gDisplayLink.paused = YES;"));
   assert!(objc.contains("CVDisplayLinkStop(gDisplayLinkLegacy);"));
   assert!(objc.contains("macos_host_request_display_link_wake"));
   assert!(objc.contains("static _Atomic(CFRunLoopSourceRef) gDisplayLinkWakeSource"));
   assert!(objc.contains("CFRunLoopSourceSignal(source);"));
   assert!(objc.contains("CFRunLoopWakeUp(runLoop);"));
   assert!(objc.contains("static CFRunLoopTimerRef gDisplayLinkSettlementTimer"));
   assert!(objc.contains("ScheduleDisplayLinkSettlement();"));
   assert!(objc.contains("CFAbsoluteTimeGetCurrent() + 1.0 / refreshHz"));
   assert!(objc.contains("static void HandleDisplayLinkWake(void)"));
   assert!(objc.contains("if (gDisplayLinkSuspendedForIdle) {"));
   assert!(objc.contains("SchedulerCounterIncrement(&gDisplayLinkMissedWakeups);"));
   assert!(objc.contains("[(MetalView *)gMetalView renderOxideFrame];"));
   assert!(objc.contains("- (CALayer *)makeBackingLayer { return [CAMetalLayer layer]; }"));
   assert!(objc.contains("NSScreen *screen = view.window.screen ?: NSScreen.mainScreen;"));
   assert!(plist.contains("<key>NSPrincipalClass</key>"));
   assert!(plist.contains("<string>NSApplication</string>"));
   assert!(!modern_tick.contains("ShouldRenderFrame()"));
   assert!(modern_tick.contains("wakeGeneration <= gDisplayLinkObservedWakeGeneration"));
   assert!(modern_tick.contains("DispatchFrameTick();"));
   assert!(wake_bridge.contains("CFRunLoopSourceSignal(source);"));
   assert!(!wake_bridge.contains("dispatch_async"));
   assert!(!wake_bridge.contains("[NSThread isMainThread]"));
}

#[test]
fn macos_display_scheduler_wakes_all_host_owned_dirty_sources()
{
   let rust = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"));
   let objc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/macos/app.m"));

   for marker in [
      "fn mark_frame_dirty(app: &mut AppState)",
      "pub extern \"C\" fn macos_app_did_become_active()",
      "pub extern \"C\" fn macos_app_on_memory_pressure(level: u32)",
      "extern \"C\" fn touch_cb(",
      "extern \"C\" fn pointer_cb(",
      "extern \"C\" fn pinch_cb(",
      "extern \"C\" fn key_cb(",
   ]
   {
      assert!(rust.contains(marker), "missing wake source `{marker}`");
   }
   assert!(rust.contains("test_scenes::Router::wants_next_frame"));
   assert!(objc.contains("macos_app_request_redraw();"));
   assert!(objc.contains("- (void)onWindowResize:"));
}

#[test]
fn macos_scheduler_benchmark_measures_real_callbacks_and_wake_latency()
{
   let objc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/macos/app.m"));
   let build = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/build.rs"));
   let runner = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../app-runner/Cargo.toml"));
   let measurement = objc
      .split("static void BeginSchedulerBenchmarkMeasurement(void)\n{")
      .nth(1)
      .expect("benchmark measurement function")
      .split("static void StartSchedulerBenchmarkIfRequested")
      .next()
      .expect("benchmark measurement function end");

   assert!(objc.contains("OXIDE_MACOS_SCHEDULER_BENCH"));
   assert!(objc.contains("#if defined(OXIDE_HOST_TESTING)"));
   assert!(build.contains("b.define(\"OXIDE_HOST_TESTING\", None);"));
   assert!(runner.contains("host-testing = [\"oxide_host_macos/host-testing\"]"));
   assert!(objc.contains("OXIDE_MACOS_SCHEDULER_SUMMARY"));
   assert!(objc.contains("gSchedulerBenchmarkWakeLatencyMs[256]"));
   assert!(objc.contains("OXIDE_MACOS_SCHEDULER_WAKE_SAMPLES"));
   assert!(objc.contains("gSchedulerBenchmarkWakeSamples == gSchedulerBenchmarkWakeTarget"));
   assert!(objc.contains("macos_emit_pointer(400.0f, 300.0f, 1.0f, 0.0f"));
   assert!(objc.contains("gSchedulerBenchmarkIgnoresNativeInput = YES;"));
   assert!(objc.contains("SchedulerBenchmarkShouldIgnoreNativeInput()"));
   assert!(objc.contains("BeginSchedulerBenchmarkMeasurement();"));
   assert!(objc.contains("gSchedulerBenchmarkActivationAttempts < 50"));
   assert!(objc.contains("now - gSchedulerBenchmarkActiveSince >= 1.0"));
   assert!(objc.contains("[gMetalView.window orderFrontRegardless];"));
   assert!(!measurement.contains("startDisplayLink"));
   assert!(objc.contains("\\\"warmupDisplayCallbacks\\\""));
   assert!(objc.contains("\\\"wakeSamples\\\""));
   assert!(objc.contains("\\\"wakeTarget\\\""));
   assert!(objc.contains("\\\"externalInputEvents\\\""));
   assert!(objc.contains("gSchedulerBenchmarkForegroundValid"));
   assert!(objc.contains("\\\"validForeground\\\""));
   assert!(objc.contains("SchedulerBenchmarkCpuUs"));
   assert!(objc.contains("SchedulerBenchmarkResidentBytes"));
   assert!(objc.contains("static id<NSApplicationDelegate> gApplicationDelegate = nil;"));
   assert!(objc.contains("[NSApp setDelegate:gApplicationDelegate];"));
   assert!(objc.contains("[NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];"));
   assert!(objc.contains("if (self.window != nil) return;"));
   assert!(objc.contains("dispatch_async(dispatch_get_main_queue(), ^{"));
   assert!(objc.contains("[(AppDelegate *)gApplicationDelegate launchHost];"));
}
