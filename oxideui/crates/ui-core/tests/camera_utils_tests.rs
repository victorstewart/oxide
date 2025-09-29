use oxideui_renderer_api::RectF;
use oxideui_ui_core::{CameraPreviewNode, CropperState, VolumeHudState};

#[test]
fn preview_node_layout_respects_padding() {
    let mut node = CameraPreviewNode::new();
    node.set_padding(12.0);
    node.layout(RectF::new(0.0, 0.0, 200.0, 100.0));
    let rect = node.rect();
    assert!((rect.x - 12.0).abs() < 1e-4);
    assert!((rect.y - 12.0).abs() < 1e-4);
    assert!((rect.w - 176.0).abs() < 1e-4);
    assert!((rect.h - 76.0).abs() < 1e-4);
}

#[test]
fn cropper_view_and_content_resets_offset() {
    let mut cropper = CropperState::new((400.0, 300.0), (120.0, 90.0));
    cropper.set_zoom(3.0);
    cropper.pan(80.0, 40.0);
    cropper.set_content_size((200.0, 150.0));
    let rect = cropper.visible_rect();
    assert!(rect.x >= 0.0);
    assert!(rect.y >= 0.0);
    assert!((rect.w - 120.0).abs() < 1e-4 || rect.w <= 200.0);
    cropper.set_view_size((160.0, 120.0));
    let rect2 = cropper.visible_rect();
    assert!(rect2.x >= 0.0);
    assert!(rect2.y >= 0.0);
    assert!(rect2.x + rect2.w <= 200.0 + 1.0);
    assert!(rect2.y + rect2.h <= 150.0 + 1.0);
}

#[test]
fn volume_hud_fade_and_clamp() {
    let mut hud = VolumeHudState::new(500);
    assert!(!hud.is_visible());
    hud.show(2.0);
    assert!(hud.is_visible());
    assert!((hud.level() - 1.0).abs() < 1e-4);
    hud.tick(300);
    assert!(hud.is_visible());
    hud.tick(300);
    assert!(!hud.is_visible());
}
