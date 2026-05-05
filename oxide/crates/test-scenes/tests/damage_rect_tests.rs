use oxide_renderer_api as gfx;
use oxide_test_scenes::Router;
use oxide_ui_core::elements::ImageUploader;
use oxide_ui_core::DrawListBuilder;

struct NullUploader;

impl ImageUploader for NullUploader {
   fn create_a8(&mut self, _w: u32, _h: u32, _data: &[u8], _row_bytes: usize) -> gfx::ImageHandle
   {
      gfx::ImageHandle(0)
   }

   fn update_a8(
      &mut self,
      _handle: gfx::ImageHandle,
      _x: u32,
      _y: u32,
      _w: u32,
      _h: u32,
      _data: &[u8],
      _row_bytes: usize,
   )
   {
   }
}

fn viewport() -> gfx::RectF
{
   gfx::RectF::new(0.0, 0.0, 390.0, 844.0)
}

fn viewport_damage() -> gfx::RectI
{
   gfx::RectI::new(0, 0, 390, 844)
}

#[test]
fn damage_lab_scene_switch_forces_one_full_redraw_before_partial_damage()
{
   let mut router = Router::new(NullUploader);
   router.toggle_overlay();

   assert!(router.prepare_onscreen_benchmark("damage_lab_frame"));

   let mut builder = DrawListBuilder::new();
   router.draw(viewport(), 1.0, &mut builder);
   assert_eq!(router.take_damage(), vec![viewport_damage()]);

   assert!(router.step_onscreen_benchmark("damage_lab_frame", 0));

   let mut builder = DrawListBuilder::new();
   router.draw(viewport(), 1.0, &mut builder);
   let damage = router.take_damage();

   assert!(!damage.iter().any(|rect| *rect == viewport_damage()));
   assert_eq!(damage, vec![gfx::RectI::new(8, 8, 374, 128)]);
}
