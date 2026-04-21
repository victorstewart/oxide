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
