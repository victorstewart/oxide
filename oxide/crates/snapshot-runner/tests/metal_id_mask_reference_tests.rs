#![cfg(target_os = "macos")]

use oxide_renderer_api::{self as api, Renderer};
use oxide_renderer_metal::id_mask_compositor::{
   IdMaskCityStyle, IdMaskCompositorMode, IdMaskGpuCompositorPass, IdMaskGpuRasterPass,
   IdMaskPolishConfig, IdMaskRasterChunk, IdMaskRasterProjection, IdMaskRasterVertex,
   ID_MASK_MAX_NEIGHBORHOOD_COLORS,
};
use oxide_renderer_metal::MetalRenderer;
use oxide_snapshot_runner::reference::{
   asymmetric_id_mask_fixture, id_mask_jump_fields, id_mask_jump_schedule, id_mask_seed_fields,
   IdMaskFieldSeed,
};

fn mask_vertices(width: usize, height: usize, city: &[u8], neighborhood: &[u8]) -> Vec<IdMaskRasterVertex>
{
   let mut vertices = Vec::new();
   for y in 0..height
   {
      for x in 0..width
      {
         let index = y * width + x;
         if city[index] == 0
         {
            continue;
         }
         let x0 = x as f32;
         let y0 = y as f32;
         let x1 = x0 + 1.0;
         let y1 = y0 + 1.0;
         let vertex = |position| IdMaskRasterVertex::new(position, city[index], neighborhood[index]);
         vertices.extend_from_slice(&[
            vertex([x0, y0]),
            vertex([x1, y0]),
            vertex([x0, y1]),
            vertex([x1, y0]),
            vertex([x1, y1]),
            vertex([x0, y1]),
         ]);
      }
   }
   vertices
}

fn field_seed(pixel: [f32; 4]) -> IdMaskFieldSeed
{
   if pixel[0] < -0.5 || pixel[1] < -0.5 || pixel[2] < 0.5
   {
      IdMaskFieldSeed::INVALID
   }
   else
   {
      IdMaskFieldSeed {
         x: pixel[0].round() as i16,
         y: pixel[1].round() as i16,
         city: pixel[2].round() as u8,
         neighborhood: pixel[3].round() as u8,
      }
   }
}

#[test]
fn metal_asymmetric_id_mask_raster_and_final_fields_match_cpu_reference()
{
   let (width, height, city, neighborhood) = asymmetric_id_mask_fixture();
   let vertices = mask_vertices(width, height, &city, &neighborhood);
   let chunks = [IdMaskRasterChunk {
      content_hash: 0xC03,
      first_vertex: 0,
      vertex_count: vertices.len(),
   }];
   let pass = IdMaskGpuCompositorPass {
      raster: IdMaskGpuRasterPass {
         viewport: api::RectF::new(0.0, 0.0, width as f32, height as f32),
         mask_width: width,
         mask_height: height,
         mask_scale: 1.0,
         vertex_revision: 1,
         vertices: &vertices,
         chunks: &chunks,
         projection: IdMaskRasterProjection::screen_px(),
      },
      city_styles: [IdMaskCityStyle::default(); 4],
      neighborhood_colors: [[1.0; 3]; ID_MASK_MAX_NEIGHBORHOOD_COLORS],
      mode: IdMaskCompositorMode::Beauty,
      glow_enabled: false,
      darken_background_alpha: 0.0,
      polish: IdMaskPolishConfig::default(),
   };
   let mut renderer = MetalRenderer::new_default().expect("create Metal renderer");
   renderer.resize(width as u32, height as u32, 1.0).expect("resize Metal renderer");
   let preferred_slot = renderer.mark_next_preferred_frame_slot_busy_for_snapshot();
   let token = renderer.begin_frame(&api::FrameTarget, None);
   let selected_slot = renderer.current_frame_slot_for_snapshot();
   assert_ne!(selected_slot, preferred_slot, "busy preferred slot was selected");
   assert_eq!(renderer.last_stats().frame_backpressure_skipped, 0, "one busy slot caused backpressure");
   renderer.release_frame_slot_for_snapshot(preferred_slot);
   renderer.encode_id_mask_gpu_compositor(&pass).expect("encode ID mask");
   assert_eq!(
      renderer.current_frame_command_buffer_slot_for_snapshot(),
      Some(selected_slot),
      "ID-mask compositor encoded outside the selected frame slot",
   );
   assert!(
      !renderer.frame_slot_has_command_buffer_for_snapshot(preferred_slot),
      "ID-mask compositor created a second command buffer on the busy preferred slot",
   );
   renderer.submit(token).expect("submit ID mask");
   let readback = renderer.readback_id_mask_snapshot().expect("read ID-mask fields");

   assert_eq!((readback.width, readback.height), (width, height));
   assert_eq!(readback.city, city);
   assert_eq!(readback.neighborhood, neighborhood);
   let mut reference = id_mask_seed_fields(width, height, &city, &neighborhood);
   for jump in id_mask_jump_schedule(width, height)
   {
      reference = id_mask_jump_fields(&reference, jump);
   }
   assert_eq!(readback.city_field.into_iter().map(field_seed).collect::<Vec<_>>(), reference.city);
   assert_eq!(readback.seam_field.into_iter().map(field_seed).collect::<Vec<_>>(), reference.seam);
}
