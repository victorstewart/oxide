use oxide_renderer_api as gfx;
use oxide_test_scenes::Router;
use oxide_ui_core as ui;
use oxide_ui_core::elements::ImageUploader;

#[derive(Default)]
struct DummyUploader;
impl ImageUploader for DummyUploader {
    fn create_a8(&mut self, _w: u32, _h: u32, _data: &[u8], _row_bytes: usize) -> gfx::ImageHandle {
        gfx::ImageHandle(1)
    }
    fn update_a8(
        &mut self,
        _h: gfx::ImageHandle,
        _x: u32,
        _y: u32,
        _w: u32,
        _h2: u32,
        _data: &[u8],
        _row_bytes: usize,
    ) {
    }
}

#[test]
fn router_basic_draw_and_counters() {
    let uploader = DummyUploader;
    let mut router = Router::new(uploader);
    router.set_scene(0); // Controls
    let mut b = ui::DrawListBuilder::new();
    let vp = gfx::RectF::new(0.0, 0.0, 640.0, 480.0);
    router.update(0, 16);
    router.draw(vp, 2.0, &mut b);
    assert!(router.counters.draws > 0);
}
