use super::*;

#[test]
fn apply_camera_render_mode_updates_state_after_success() {
    let mut app = AppState::default();
    assert_eq!(apply_camera_render_mode(&mut app, metal::CameraRenderMode::BgraBenchmark, 0), 0);
    assert_eq!(app.camera_render_mode, metal::CameraRenderMode::BgraBenchmark);
}

#[test]
fn apply_camera_render_mode_preserves_existing_mode_after_preview_pixel_format_failure() {
    let mut app = AppState::default();
    app.camera_render_mode = metal::CameraRenderMode::Nv12Legacy;
    assert_eq!(apply_camera_render_mode(&mut app, metal::CameraRenderMode::BgraBenchmark, -1), -1);
    assert_eq!(app.camera_render_mode, metal::CameraRenderMode::Nv12Legacy);
}
