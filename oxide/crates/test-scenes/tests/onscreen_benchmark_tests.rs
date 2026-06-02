use oxide_platform_api as api;
use oxide_renderer_api as gfx;
use oxide_test_scenes::{Router, SceneKind};
use oxide_ui_core::DrawListBuilder;

mod helpers;

use helpers::NullUploader;

#[test]
fn headline_onscreen_benchmarks_prepare_and_step() {
    let cases = [
        ("component_label_encode", SceneKind::TextLayout),
        ("component_progress_bar_encode", SceneKind::Controls),
        ("component_spinner_encode", SceneKind::Controls),
        ("component_button_encode", SceneKind::Controls),
        ("component_toggle_encode", SceneKind::Controls),
        ("component_slider_encode", SceneKind::Controls),
        ("component_image_view_encode", SceneKind::ZoomImage),
        ("component_nine_slice_image_encode", SceneKind::NineSlice),
        ("component_collection_view_encode", SceneKind::CollectionStress),
        ("animation_progress_indeterminate", SceneKind::Controls),
        ("animation_button_press_scale", SceneKind::Controls),
        ("animation_toggle_thumb_spring", SceneKind::Controls),
        ("animation_slider_thumb_move", SceneKind::Controls),
    ];

    for (name, scene) in cases {
        let mut router = Router::new(NullUploader);

        assert!(router.prepare_onscreen_benchmark(name), "{name} did not prepare");
        assert_eq!(router.current, scene, "{name} prepared the wrong scene");
        assert!(router.step_onscreen_benchmark(name, 1), "{name} did not step");
    }
}

fn touch(id: u64, phase: api::TouchPhase, x: f32, y: f32) -> api::TouchEvent {
    api::TouchEvent {
        id: api::TouchId(id),
        phase,
        timestamp_ns: 0,
        x,
        y,
        pressure: None,
        tilt: None,
        device: api::PointerDevice::Finger,
    }
}

fn image_rect(router: &mut Router<NullUploader>) -> gfx::RectF {
    let mut builder = DrawListBuilder::new();
    router.draw(gfx::RectF::new(0.0, 0.0, 390.0, 844.0), 1.0, &mut builder);
    builder
        .drawlist()
        .items
        .iter()
        .find_map(|cmd| match cmd {
            gfx::DrawCmd::NineSlice { rect, .. } => Some(*rect),
            _ => None,
        })
        .expect("zoom image draw command")
}

#[test]
fn raw_touch_pinch_reaches_zoom_image_scene() {
    let mut router = Router::new(NullUploader);
    assert!(router.prepare_onscreen_benchmark("component_image_view_encode"));
    router.set_zoom_image(gfx::ImageHandle(7), 100, 100);

    let before = image_rect(&mut router);
    router.input_touch(&touch(1, api::TouchPhase::Start, 180.0, 400.0));
    router.input_touch(&touch(2, api::TouchPhase::Start, 220.0, 400.0));
    router.input_touch(&touch(2, api::TouchPhase::Move, 260.0, 400.0));
    let after = image_rect(&mut router);

    assert!(
        after.w > before.w * 1.5,
        "pinch should increase zoom width: before={before:?} after={after:?}"
    );
}

#[test]
fn raw_touch_pinch_does_not_apply_two_touch_pan_to_zoom_image_scene() {
    let mut router = Router::new(NullUploader);
    assert!(router.prepare_onscreen_benchmark("component_image_view_encode"));
    router.set_zoom_image(gfx::ImageHandle(7), 100, 100);

    router.input_touch(&touch(1, api::TouchPhase::Start, 180.0, 400.0));
    router.input_touch(&touch(2, api::TouchPhase::Start, 220.0, 400.0));
    router.input_touch(&touch(2, api::TouchPhase::Move, 260.0, 400.0));
    let after = image_rect(&mut router);

    assert!(
      (after.x + 115.0).abs() < 0.001,
      "pinch should scale around the image layout without applying two-touch pan: after={after:?}"
   );
}

#[test]
fn raw_touch_pan_reaches_zoom_image_scene() {
    let mut router = Router::new(NullUploader);
    assert!(router.prepare_onscreen_benchmark("component_image_view_encode"));
    router.set_zoom_image(gfx::ImageHandle(7), 100, 100);

    let before = image_rect(&mut router);
    router.input_touch(&touch(1, api::TouchPhase::Start, 180.0, 400.0));
    router.input_touch(&touch(1, api::TouchPhase::Move, 210.0, 416.0));
    let after = image_rect(&mut router);

    assert!(
        after.x > before.x + 20.0 && after.y > before.y + 10.0,
        "pan should move zoom image rect: before={before:?} after={after:?}"
    );
}
