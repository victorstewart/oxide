use oxide_renderer_api::{DrawCmd, RectF};
use oxide_test_scenes::{Router, SceneKind};
use oxide_text::Font;
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

#[test]
fn controls_scene_emits_glyph_runs_once_a_font_is_loaded() {
    let mut router = Router::new(DummyUploader);
    let font_bytes = include_bytes!("../../../../crates/ui-core/assets/Asap-Regular.ttf").to_vec();
    let _font_id = router.text.fonts.add_font(Font::from_bytes(font_bytes));
    router.set_scene(SceneKind::Controls as usize);
    router.update(0, 16);

    let mut builder = DrawListBuilder::new();
    router.draw(RectF::new(0.0, 0.0, 390.0, 844.0), 3.0, &mut builder);
    let drawlist = builder.drawlist();

    assert!(
        drawlist.items.iter().any(|item| matches!(item, DrawCmd::GlyphRun { .. })),
        "expected Controls scene to emit glyph runs once a font is loaded"
    );
}
