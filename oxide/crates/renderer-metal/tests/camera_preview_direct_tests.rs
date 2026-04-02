use oxide_renderer_metal::{
    direct_live_preview_needs_render, direct_preview_can_reuse_resize_targets,
    direct_preview_reason_requires_drawable, direct_preview_submission_backpressure_applies,
    direct_preview_tiny_renderer_active, direct_preview_uses_dontcare_load_action,
    direct_preview_uses_fast_yuv_pipeline, CameraRenderMode, CameraTextureSource, MetalInitError,
    MetalRenderer, MetalRendererConfig,
    CAMERA_PREVIEW_REASON_BACKPRESSURE, CAMERA_PREVIEW_REASON_NEW_GENERATION,
    CAMERA_PREVIEW_REASON_NEW_TIMESTAMP, CAMERA_PREVIEW_REASON_NO_CURRENT_FRAME,
    CAMERA_PREVIEW_REASON_RESIZE,
};
use std::sync::{Mutex, OnceLock};

fn env_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn with_env_var(key: &str, value: Option<&str>, body: impl FnOnce()) {
    let _guard = env_test_lock().lock().expect("env test lock");
    let saved = std::env::var(key).ok();
    match value {
        Some(value) => unsafe { std::env::set_var(key, value) },
        None => unsafe { std::env::remove_var(key) },
    }
    body();
    match saved {
        Some(value) => unsafe { std::env::set_var(key, value) },
        None => unsafe { std::env::remove_var(key) },
    }
}

#[test]
fn direct_camera_preview_path_draws_single_synthetic_camera_frame() {
    let mut renderer = match MetalRenderer::new_with_config(MetalRendererConfig {
        wants_hdr: false,
        sample_count: 1,
        camera_render_mode: CameraRenderMode::Nv12Legacy,
        camera_texture_source: CameraTextureSource::SyntheticBenchmark,
        direct_preview_only: true,
    }) {
        Ok(renderer) => renderer,
        Err(MetalInitError::NoDevice) => return,
        Err(err) => panic!("unexpected renderer init error: {err}"),
    };

    unsafe {
        renderer
            .render_camera_preview_direct(core::ptr::null_mut(), 390, 844, 3.0)
            .expect("render direct synthetic camera preview");
    }

    let stats = renderer.last_stats();
    assert_eq!(stats.draws, 1);
    assert_eq!(stats.damage_rects, 0);
    assert_eq!(stats.blur_updates, 0);
    assert_eq!(stats.blur_period_ms, 0);
    assert_eq!(stats.cam_paused, 0);
    assert_eq!(stats.cam_width, 1920);
    assert_eq!(stats.cam_height, 1080);
    assert_eq!(stats.cam_bit_depth, 8);
    assert!(stats.cam_coverage_pct > 0.99);
}

#[test]
fn direct_camera_preview_path_reuses_same_surface_size_without_regressing_output() {
    let mut renderer = match MetalRenderer::new_with_config(MetalRendererConfig {
        wants_hdr: false,
        sample_count: 1,
        camera_render_mode: CameraRenderMode::Nv12Legacy,
        camera_texture_source: CameraTextureSource::SyntheticBenchmark,
        direct_preview_only: true,
    }) {
        Ok(renderer) => renderer,
        Err(MetalInitError::NoDevice) => return,
        Err(err) => panic!("unexpected renderer init error: {err}"),
    };

    unsafe {
        renderer
            .render_camera_preview_direct(core::ptr::null_mut(), 390, 844, 3.0)
            .expect("first direct synthetic camera preview");
        renderer
            .render_camera_preview_direct(core::ptr::null_mut(), 390, 844, 3.0)
            .expect("second direct synthetic camera preview");
    }

    let stats = renderer.last_stats();
    assert_eq!(stats.draws, 1);
    assert_eq!(stats.damage_rects, 0);
    assert_eq!(stats.cam_paused, 0);
    assert_eq!(stats.cam_width, 1920);
    assert_eq!(stats.cam_height, 1080);
    assert!(stats.cam_fetch_ms >= 0.0);
}

#[test]
fn direct_camera_preview_path_reuses_same_size_fast_path() {
    let mut renderer = match MetalRenderer::new_with_config(MetalRendererConfig {
        wants_hdr: false,
        sample_count: 1,
        camera_render_mode: CameraRenderMode::Nv12Legacy,
        camera_texture_source: CameraTextureSource::SyntheticBenchmark,
        direct_preview_only: true,
    }) {
        Ok(renderer) => renderer,
        Err(MetalInitError::NoDevice) => return,
        Err(err) => panic!("unexpected renderer init error: {err}"),
    };

    unsafe {
        renderer
            .render_camera_preview_direct(core::ptr::null_mut(), 390, 844, 3.0)
            .expect("render first direct synthetic camera preview");
        renderer
            .render_camera_preview_direct(core::ptr::null_mut(), 390, 844, 3.0)
            .expect("render second direct synthetic camera preview");
    }

    let stats = renderer.last_stats();
    assert_eq!(stats.draws, 1);
    assert_eq!(stats.damage_rects, 0);
    assert_eq!(stats.cam_width, 1920);
    assert_eq!(stats.cam_height, 1080);
    assert!(stats.cam_fetch_ms >= 0.0);
    assert!(stats.cam_setup_ms >= 0.0);
    assert!(stats.cam_gpu_render_ms >= 0.0);
    assert!(stats.cam_gpu_vertex_ms >= 0.0);
    assert!(stats.cam_gpu_fragment_ms >= 0.0);
}

#[test]
fn direct_camera_preview_resize_reuses_same_size_targets() {
    assert!(direct_preview_can_reuse_resize_targets(390, 844, 3.0, 390, 844, 3.0, 1));
    assert!(!direct_preview_can_reuse_resize_targets(390, 844, 3.0, 391, 844, 3.0, 1));
    assert!(!direct_preview_can_reuse_resize_targets(390, 844, 3.0, 390, 844, 2.0, 1));
    assert!(!direct_preview_can_reuse_resize_targets(390, 844, 3.0, 390, 844, 3.0, 4));
}

#[test]
fn direct_camera_preview_fast_pipeline_only_targets_8bit_bt709_yuv() {
    assert!(direct_preview_uses_fast_yuv_pipeline(8, 0, 0));
    assert!(direct_preview_uses_fast_yuv_pipeline(8, 0, 1));
    assert!(!direct_preview_uses_fast_yuv_pipeline(10, 0, 0));
    assert!(!direct_preview_uses_fast_yuv_pipeline(8, 1, 0));
    assert!(!direct_preview_uses_fast_yuv_pipeline(8, 2, 1));
}

#[test]
fn direct_live_preview_requires_drawable_only_for_resize_or_new_frame_identity() {
    assert_eq!(
        direct_live_preview_needs_render(false, true, 7, 100, 7, 100),
        CAMERA_PREVIEW_REASON_RESIZE
    );
    assert_eq!(
        direct_live_preview_needs_render(true, false, 0, 0, 0, 0),
        CAMERA_PREVIEW_REASON_NO_CURRENT_FRAME
    );
    assert_eq!(
        direct_live_preview_needs_render(true, true, 7, 100, 8, 101),
        CAMERA_PREVIEW_REASON_NEW_TIMESTAMP | CAMERA_PREVIEW_REASON_NEW_GENERATION
    );
    assert_eq!(
        direct_live_preview_needs_render(true, true, 7, 100, 8, 100),
        CAMERA_PREVIEW_REASON_NEW_GENERATION
    );
    assert_eq!(direct_live_preview_needs_render(true, true, 7, 100, 7, 100), 0);
    assert_eq!(
        direct_live_preview_needs_render(true, true, 7, 100, 7, 101),
        CAMERA_PREVIEW_REASON_NEW_TIMESTAMP
    );
    assert_eq!(
        direct_live_preview_needs_render(true, true, 7, 0, 8, 0),
        CAMERA_PREVIEW_REASON_NEW_GENERATION
    );
    assert_eq!(direct_live_preview_needs_render(true, true, 7, 0, 7, 0), 0);
}

#[test]
fn direct_tiny_preview_renderer_only_targets_live_single_sample_nv12_modes() {
    assert!(direct_preview_tiny_renderer_active(
        true,
        1,
        CameraTextureSource::Live,
        CameraRenderMode::Nv12Optimized
    ));
    assert!(direct_preview_tiny_renderer_active(
        true,
        1,
        CameraTextureSource::Live,
        CameraRenderMode::Nv12Legacy
    ));
    assert!(!direct_preview_tiny_renderer_active(
        false,
        1,
        CameraTextureSource::Live,
        CameraRenderMode::Nv12Optimized
    ));
    assert!(!direct_preview_tiny_renderer_active(
        true,
        4,
        CameraTextureSource::Live,
        CameraRenderMode::Nv12Optimized
    ));
    assert!(!direct_preview_tiny_renderer_active(
        true,
        1,
        CameraTextureSource::SyntheticBenchmark,
        CameraRenderMode::Nv12Optimized
    ));
    assert!(!direct_preview_tiny_renderer_active(
        true,
        1,
        CameraTextureSource::Live,
        CameraRenderMode::BgraBenchmark
    ));
}

#[test]
fn direct_preview_backpressure_only_applies_at_two_in_flight_submissions() {
    assert!(!direct_preview_submission_backpressure_applies(None, 2));
    assert!(!direct_preview_submission_backpressure_applies(Some(2), 1));
    assert!(direct_preview_submission_backpressure_applies(Some(2), 2));
    assert!(direct_preview_submission_backpressure_applies(Some(2), 3));
    assert!(!direct_preview_submission_backpressure_applies(Some(1), 0));
    assert!(direct_preview_submission_backpressure_applies(Some(1), 1));
    assert!(!direct_preview_reason_requires_drawable(CAMERA_PREVIEW_REASON_BACKPRESSURE));
    assert!(direct_preview_reason_requires_drawable(CAMERA_PREVIEW_REASON_NEW_GENERATION));
}

#[test]
fn direct_preview_dontcare_load_action_is_env_gated() {
    with_env_var("OXIDE_PERF_CAMERA_PREVIEW_DONT_CARE_LOAD", None, || {
        assert!(!direct_preview_uses_dontcare_load_action());
    });
    with_env_var("OXIDE_PERF_CAMERA_PREVIEW_DONT_CARE_LOAD", Some("1"), || {
        assert!(direct_preview_uses_dontcare_load_action());
    });
}
