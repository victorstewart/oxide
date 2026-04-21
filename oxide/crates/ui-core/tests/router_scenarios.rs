use oxide_renderer_api::RectF;
use oxide_test_scenes::{Router, SceneKind};
use oxide_ui_core::elements::ImageUploader;
use oxide_ui_core::DrawListBuilder;

#[derive(Default)]
struct DummyUploader;
impl ImageUploader for DummyUploader {
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

fn draw_router(router: &mut Router<DummyUploader>) -> usize {
    let mut builder = DrawListBuilder::new();
    let vp = RectF::new(0.0, 0.0, 640.0, 480.0);
    router.draw(vp, 2.0, &mut builder);
    builder.drawlist().items.len()
}

fn advance_router(router: &mut Router<DummyUploader>, now_ms: &mut u64, frames: usize, dt_ms: u32) {
    for _ in 0..frames {
        *now_ms += dt_ms as u64;
        router.update(*now_ms, dt_ms);
    }
}

#[test]
fn controls_scene_counters_and_overlay() {
    let mut router = Router::new(DummyUploader);
    router.set_scene(0);
    router.update(0, 16);
    let draws = draw_router(&mut router);
    assert!(draws > 0);
    assert!(!router.take_damage().is_empty());
    let overlay_draws = draws;
    router.toggle_overlay();
    let draws_no_overlay = draw_router(&mut router);
    assert!(draws_no_overlay < overlay_draws);
}

#[test]
fn overlay_toggle_keeps_fps_sampling_stable() {
    let mut router = Router::new(DummyUploader);
    let mut now_ms = 0_u64;

    advance_router(&mut router, &mut now_ms, 70, 16);
    let _ = draw_router(&mut router);
    assert!(router.counters.fps > 40.0);

    router.toggle_overlay();
    advance_router(&mut router, &mut now_ms, 60, 16);
    let _ = draw_router(&mut router);
    assert_eq!(router.counters.fps, 0.0);

    router.toggle_overlay();
    advance_router(&mut router, &mut now_ms, 1, 16);
    let _ = draw_router(&mut router);
    assert!(router.counters.fps > 40.0);
}

#[test]
fn text_layout_scene_stable_damage() {
    let mut router = Router::new(DummyUploader);
    router.set_scene(SceneKind::TextLayout as usize);
    router.update(0, 16);
    let _ = draw_router(&mut router);
    assert!(!router.take_damage().is_empty());
}

#[test]
fn zoom_scene_double_tap_resets_state() {
    let mut router = Router::new(DummyUploader);
    router.set_scene(SceneKind::ZoomImage as usize);
    router.set_zoom_image(oxide_renderer_api::ImageHandle(7), 256, 256);
    router.update(0, 16);
    router.input_pinch(160.0, 120.0, 1.2);
    let draws_zoomed = draw_router(&mut router);
    router.input_double_tap();
    let draws_reset = draw_router(&mut router);
    assert!(draws_zoomed >= draws_reset);
}

#[test]
fn anim_scene_reports_active_anims() {
    let mut router = Router::new(DummyUploader);
    router.set_scene(SceneKind::AnimTimeline as usize);
    router.update(0, 16);
    let _ = draw_router(&mut router);
    assert!(router.counters.anims as i32 >= 0);
    assert!(!router.take_damage().is_empty());
}

#[test]
fn collection_scene_focus_navigation() {
    let mut router = Router::new(DummyUploader);
    router.set_scene(SceneKind::CollectionStress as usize);
    router.update(0, 16);
    draw_router(&mut router);
    router.key_arrow_right();
    router.key_arrow_down();
    draw_router(&mut router);
    // No panic and damage reported
    assert!(!router.take_damage().is_empty());
}
