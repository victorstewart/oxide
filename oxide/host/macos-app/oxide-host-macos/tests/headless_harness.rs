#[cfg(all(target_os = "macos", feature = "host-testing"))]
mod harness {
    use oxide_host_macos::{
        host_harness_reset, host_harness_snapshot, macos_app_frame, macos_app_frame_with_drawable,
        macos_app_init,
    };
    use oxide_telemetry::TelemetryLifecycleState;
    use oxide_test_scenes::SceneKind;

    #[test]
    fn headless_host_smoke() {
        const W: u32 = 1280;
        const H: u32 = 720;
        const SCALE: f32 = 2.0;

        host_harness_reset();

        let init = macos_app_init(W, H, SCALE);
        assert_eq!(init, 0, "macos_app_init failed");

        let baseline = host_harness_snapshot();
        assert!(baseline.inited, "app should be initialised");
        assert_eq!(baseline.router_scene, Some(SceneKind::Controls));
        assert_eq!(baseline.draws.unwrap_or_default(), 0, "no draws before frames");
        assert_eq!(
            baseline.telemetry_lifecycle,
            Some(TelemetryLifecycleState::Foreground),
            "telemetry lifecycle should be foreground after init"
        );

        for _ in 0..5 {
            let frame = macos_app_frame(W, H, SCALE);
            assert_eq!(frame, 0, "macos_app_frame should succeed");
        }
        assert_eq!(
            macos_app_frame_with_drawable(W, H, SCALE, core::ptr::null_mut()),
            0,
            "macos_app_frame_with_drawable should preserve the headless path",
        );

        let after = host_harness_snapshot();
        assert!(after.inited, "app should remain initialised");
        let draws = after.draws.unwrap_or_default();
        let instanced = after.instanced.unwrap_or_default();
        assert!(
            draws > 0 || instanced > 0,
            "renderer should record draw commands; draws={} instanced={}",
            draws,
            instanced
        );
        assert!(after.last_ms >= baseline.last_ms, "frame clock should advance");
        assert_eq!(
            after.telemetry_lifecycle,
            Some(TelemetryLifecycleState::Foreground),
            "telemetry lifecycle should remain foreground"
        );

        host_harness_reset();
    }
}

#[cfg(not(all(target_os = "macos", feature = "host-testing")))]
#[test]
fn headless_host_smoke() {
    // Harness requires macOS + host-testing feature; nothing to assert here.
}
