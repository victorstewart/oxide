use oxide_permissions::PermissionState;
use oxide_platform_api::{PermissionDomain, PermissionStatus};
use oxide_renderer_api::RectF;
use oxide_ui_core::{
    elements::{ImageUploader, TextCtx},
    permissions::PermissionOverlayUi,
    DrawListBuilder,
};

struct TestUploader;
impl ImageUploader for TestUploader {
    fn create_a8(
        &mut self,
        _w: u32,
        _h: u32,
        _data: &[u8],
        _row_bytes: usize,
    ) -> oxide_renderer_api::ImageHandle {
        oxide_renderer_api::ImageHandle(1)
    }

    fn update_a8(
        &mut self,
        _handle: oxide_renderer_api::ImageHandle,
        _x: u32,
        _y: u32,
        _w: u32,
        _h: u32,
        _data: &[u8],
        _row_bytes: usize,
    ) {
    }
}

#[test]
fn overlay_tracks_permission_status() {
    let mut overlay = PermissionOverlayUi::default();
    let states = [
        PermissionState::new(PermissionDomain::Camera, PermissionStatus::NotDetermined, 0),
        PermissionState::new(PermissionDomain::Microphone, PermissionStatus::Authorized, 0),
    ];
    overlay.update(&states);
    assert!(overlay.is_visible());

    let mut builder = DrawListBuilder::new();
    let mut text = TextCtx::default();
    let mut uploader = TestUploader;
    overlay.draw(RectF::new(0.0, 0.0, 320.0, 240.0), 2.0, &mut text, &mut uploader, &mut builder);

    let triggered = overlay.pointer_event(160.0, 180.0, 1);
    assert!(triggered.is_none());
    let triggered = overlay.pointer_event(160.0, 180.0, 0);
    assert_eq!(triggered, Some(PermissionDomain::Camera));
}
