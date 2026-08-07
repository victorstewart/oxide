use super::*;

#[test]
fn legacy_encoder_borrow_and_draw_storage_survive_frame_reuse()
{
   let mut encoder = LegacyDrawEncoder::new();
   encoder.begin_frame();
   let vertices = [
      gfx_api::Vertex { x: 0.0, y: 0.0, u: 0.0, v: 0.0, rgba: u32::MAX },
      gfx_api::Vertex { x: 10.0, y: 0.0, u: 1.0, v: 0.0, rgba: u32::MAX },
      gfx_api::Vertex { x: 0.0, y: 10.0, u: 0.0, v: 1.0, rgba: u32::MAX },
      gfx_api::Vertex { x: 10.0, y: 10.0, u: 1.0, v: 1.0, rgba: u32::MAX },
   ];
   {
      let context = gfx_api::RenderContext { frame_id: 1, encoder: &mut encoder };
      context.encoder.draw_solid(&vertices, gfx_api::Color::rgba(1.0, 0.0, 0.0, 1.0));
   }
   encoder.finish_frame();

   assert_eq!(encoder.draw_list().items.len(), 1);
   assert_eq!(encoder.draw_list().vertices.len(), 4);
   let capacity = encoder.draw_list().vertices.capacity();
   encoder.begin_frame();
   assert_eq!(encoder.draw_list().vertices.capacity(), capacity);
}
