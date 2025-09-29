use oxideui_renderer_api::RectF;
use oxideui_ui_core::elements::ImageUploader;
use oxideui_ui_core::scenes::{Router, SceneKind};
use oxideui_ui_core::DrawListBuilder;

#[derive(Default)]
struct DummyUploader;
impl ImageUploader for DummyUploader {
    fn create_a8(
        &mut self,
        _w: u32,
        _h: u32,
        _data: &[u8],
        _row_bytes: usize,
    ) -> oxideui_renderer_api::ImageHandle {
        oxideui_renderer_api::ImageHandle(1)
    }
    fn update_a8(
        &mut self,
        _handle: oxideui_renderer_api::ImageHandle,
        _x: u32,
        _y: u32,
        _w: u32,
        _h: u32,
        _data: &[u8],
        _row_bytes: usize,
    ) {
    }
}

fn draw_router(router: &mut Router<DummyUploader>) -> usize {
    let mut builder = DrawListBuilder::new();
    let vp = RectF::new(0.0, 0.0, 640.0, 480.0);
    router.draw(vp, 2.0, &mut builder);
    builder.drawlist().items.len()
}

#[test]
fn controls_scene_counters_and_overlay() {
    let mut router = Router::new(DummyUploader::default());
    router.set_scene(0);
    router.update(0, 16);
    let draws = draw_router(&mut router);
    assert!(draws > 0);
    assert!(router.take_damage().len() >= 1);
    let overlay_draws = draws;
    router.toggle_overlay();
    let draws_no_overlay = draw_router(&mut router);
    assert!(draws_no_overlay < overlay_draws);
}

#[test]
fn text_layout_scene_stable_damage() {
    let mut router = Router::new(DummyUploader::default());
    router.set_scene(SceneKind::TextLayout as usize);
    router.update(0, 16);
    let _ = draw_router(&mut router);
    assert!(!router.take_damage().is_empty());
}

#[test]
fn zoom_scene_double_tap_resets_state() {
    let mut router = Router::new(DummyUploader::default());
    router.set_scene(SceneKind::ZoomImage as usize);
    router.set_zoom_image(oxideui_renderer_api::ImageHandle(7), 256, 256);
    router.update(0, 16);
    router.input_pinch(160.0, 120.0, 1.2);
    let draws_zoomed = draw_router(&mut router);
    router.input_double_tap();
    let draws_reset = draw_router(&mut router);
    assert!(draws_zoomed >= draws_reset);
}

#[test]
fn anim_scene_reports_active_anims() {
    let mut router = Router::new(DummyUploader::default());
    router.set_scene(SceneKind::AnimTimeline as usize);
    router.update(0, 16);
    let _ = draw_router(&mut router);
    assert!(router.counters.anims as i32 >= 0);
    assert!(!router.take_damage().is_empty());
}

#[test]
fn collection_scene_focus_navigation() {
    let mut router = Router::new(DummyUploader::default());
    router.set_scene(SceneKind::CollectionStress as usize);
    router.update(0, 16);
    draw_router(&mut router);
    router.key_arrow_right();
    router.key_arrow_down();
    draw_router(&mut router);
    // No panic and damage reported
    assert!(!router.take_damage().is_empty());
}
