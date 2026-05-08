use oxide_renderer_api as gfx;
use oxide_ui_core::elements::ImageUploader;

pub struct NullUploader;

impl ImageUploader for NullUploader
{
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
