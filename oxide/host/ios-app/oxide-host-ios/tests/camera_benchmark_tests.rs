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
    OxideHostStats {
        fps: 0.0,
        draws: 0,
        anims: 0,
        memory_warnings: 0,
        damage_pct: 0.0,
        damage_rects: 0,
        cam_coverage_pct: 0.0,
        cam_blur_ms: 0.0,
        cam_blur_updates: 0,
        cam_update_period_ms: 0,
        cam_paused: 0,
        cam_low_power: 0,
        cam_thermal: 0,
        cam_width: 0,
        cam_height: 0,
        cam_bit_depth: 0,
        cam_matrix: 0,
        cam_video_range: 0,
        cam_color_space: 0,
        cam_running: 0,
        cam_fps: 0.0,
        cam_poll_submissions_ms: 0.0,
        cam_fetch_ms: 0.0,
        cam_setup_ms: 0.0,
        cam_encode_quad_ms: 0.0,
        cam_command_buffer_ms: 0.0,
        cam_encoder_ms: 0.0,
        cam_encode_bind_ms: 0.0,
        cam_encode_draw_ms: 0.0,
        cam_end_encoding_ms: 0.0,
        cam_present_ms: 0.0,
        cam_commit_ms: 0.0,
        cam_gpu_ms: 0.0,
        cam_capture_total_ms: 0.0,
        cam_capture_sample_setup_ms: 0.0,
        cam_capture_lock_ms: 0.0,
        cam_capture_texture_bridge_ms: 0.0,
        cam_capture_publish_ms: 0.0,
        cam_capture_publish_lock_ms: 0.0,
        cam_capture_publish_texture_refs_ms: 0.0,
        cam_capture_publish_pixel_buffer_ms: 0.0,
        cam_capture_frame_delivery_ms: 0.0,
        cam_sample_delivery_pool_bytes: 0,
        cam_sample_delivery_pool_surfaces: 0,
        cam_active_sample_surface_bytes: 0,
        cam_active_sample_surface_surfaces: 0,
        cam_active_sample_buffers: 0,
        cam_peak_active_sample_surface_bytes: 0,
        cam_peak_active_sample_surface_surfaces: 0,
        cam_peak_active_sample_buffers: 0,
        cam_sample_delivery_total_samples: 0,
        cam_sample_delivery_reused_frames: 0,
        cam_sample_delivery_reused_surfaces: 0,
        cam_sample_delivery_max_reuse_gap_frames: 0,
        cam_retained_sample_surface_bytes: 0,
        cam_retained_sample_surface_surfaces: 0,
        cam_retained_published_slot_surface_bytes: 0,
        cam_retained_published_slot_surfaces: 0,
        cam_retained_latest_pixel_buffer_surface_bytes: 0,
        cam_retained_latest_pixel_buffer_surface_surfaces: 0,
        cam_latest_published_generation: 0,
        cam_latest_published_timestamp_ns: 0,
        renderer_memory_total_bytes: 0,
        renderer_memory_draw_targets_bytes: 0,
        renderer_memory_draw_target_main_bytes: 0,
        renderer_memory_draw_target_msaa_bytes: 0,
        renderer_memory_effect_targets_bytes: 0,
        renderer_memory_effect_prepass_bytes: 0,
        renderer_memory_effect_blur_chain_bytes: 0,
        renderer_memory_live_camera_bytes: 0,
        renderer_memory_camera_cache_bytes: 0,
        renderer_memory_camera_blur_cache_bytes: 0,
        renderer_memory_camera_transition_cache_bytes: 0,
        renderer_memory_benchmark_camera_bytes: 0,
        renderer_memory_layer_cache_bytes: 0,
        renderer_memory_image_cache_bytes: 0,
        renderer_memory_buffer_bytes: 0,
        renderer_pending_command_buffers: 0,
        renderer_pending_present_drawables: 0,
        renderer_pending_present_textures: 0,
        renderer_preview_submission_depth: 0,
        renderer_preview_submission_skipped: 0,
        renderer_preview_submission_frame_age_ms: 0.0,
    }
}

fn test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn benchmark_camera_scene_uses_minimal_preview_draw_list() {
    let _guard = test_lock().lock().expect("test lock");
    oxide_host_app_shutdown();
    assert_eq!(oxide_host_set_benchmark_mode(1), 0);
    assert_eq!(oxide_host_set_camera_render_mode(1), 0);
    assert_eq!(oxide_host_set_camera_texture_source(1), 0);
    assert_eq!(oxide_host_app_init(390, 844, 3.0), 0);
    assert_eq!(oxide_host_set_scene(10), 0);
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
    assert!(stats.cam_capture_sample_setup_ms >= 0.0);
    assert!(stats.cam_capture_frame_delivery_ms >= 0.0);
    assert!(stats.renderer_memory_total_bytes > 0);
    assert!(stats.renderer_memory_buffer_bytes > 0);
    assert!(stats.renderer_memory_benchmark_camera_bytes > 0);
    assert!(stats.renderer_memory_total_bytes >= stats.renderer_memory_buffer_bytes);
    assert!(stats.renderer_memory_total_bytes >= stats.renderer_memory_benchmark_camera_bytes);

    assert_eq!(oxide_host_set_camera_texture_source(0), 0);
    assert_eq!(oxide_host_set_camera_render_mode(0), 0);
    oxide_host_app_shutdown();
}

#[test]
fn benchmark_camera_preview_plan_requires_first_drawable() {
    let _guard = test_lock().lock().expect("test lock");
    oxide_host_app_shutdown();
    assert_eq!(oxide_host_set_benchmark_mode(1), 0);
    assert_eq!(oxide_host_set_camera_render_mode(1), 0);
    assert_eq!(oxide_host_set_camera_texture_source(1), 0);
    assert_eq!(oxide_host_app_init(390, 844, 3.0), 0);
    assert_eq!(oxide_host_set_scene(10), 0);

    assert_eq!(oxide_host_camera_preview_plan(390, 844, 3.0), 1);

    assert_eq!(oxide_host_set_camera_texture_source(0), 0);
    assert_eq!(oxide_host_set_camera_render_mode(0), 0);
    oxide_host_app_shutdown();
}

#[test]
fn merge_camera_contract_fields_prefers_backend_contract_over_rotated_preview_stats() {
    let _guard = test_lock().lock().expect("test lock");
    let (width, height, fps, video_range, color_space) =
        merge_camera_contract_fields(720, 1280, 0.0, 0, 0, 1280, 720, 30.0, 0, 0);

    assert_eq!(width, 1280);
    assert_eq!(height, 720);
    assert_eq!(fps, 30.0);
    assert_eq!(video_range, 0);
    assert_eq!(color_space, 0);
}
