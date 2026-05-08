use oxide_permissions::PermissionState;
use oxide_platform_api::{PermissionDomain, PermissionStatus};
use oxide_renderer_api as gfx;
use oxide_test_scenes::CameraDemo;
use oxide_ui_core::{elements::TextCtx, DrawListBuilder};

mod helpers;

use helpers::NullUploader;

#[test]
fn default_camera_preview_draws_only_fullscreen_camera_background() {
    let mut camera = CameraDemo::default();
    let viewport = gfx::RectF::new(0.0, 0.0, 390.0, 844.0);
    let mut text = TextCtx::default();
    let mut uploader = NullUploader;
    let mut builder = DrawListBuilder::new();

    camera.draw(viewport, 3.0, &mut text, &mut uploader, &mut builder);

    assert_eq!(builder.drawlist().items.len(), 1);
    match &builder.drawlist().items[0] {
        gfx::DrawCmd::CameraBg { rect, .. } => assert_eq!(*rect, viewport),
        other => panic!("expected a single fullscreen camera draw, got {:?}", other),
    }
}

#[test]
fn permission_overlay_disables_plain_preview_fast_path() {
    let mut camera = CameraDemo::default();
    let viewport = gfx::RectF::new(0.0, 0.0, 390.0, 844.0);
    let mut text = TextCtx::default();
    let mut uploader = NullUploader;
    let mut builder = DrawListBuilder::new();

    camera.update_permissions(&[PermissionState::new(
        PermissionDomain::Camera,
        PermissionStatus::NotDetermined,
        0,
    )]);
    camera.draw(viewport, 3.0, &mut text, &mut uploader, &mut builder);

    assert!(builder.drawlist().items.len() > 1);
}
