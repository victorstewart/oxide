#![cfg(all(
   feature = "snapshot-tests",
   any(target_os = "macos", all(target_os = "ios", not(target_abi = "sim")))
))]

use oxide_renderer_api::{self as api, Renderer};
use oxide_renderer_metal::MetalRenderer;

fn render_image(renderer: &mut MetalRenderer, texture: api::ImageHandle, width: u32, height: u32, source_width: u32, source_height: u32) -> Vec<u8>
{
   renderer.resize(width, height, 1.0).expect("resize image renderer");
   let mut list = api::DrawList::default();
   list.items.push(api::DrawCmd::Image {
      tex: texture,
      dst: api::RectF::new(0.0, 0.0, width as f32, height as f32),
      src: api::RectF::new(0.0, 0.0, source_width as f32, source_height as f32),
      alpha: 1.0,
   });
   let frame = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_pass(&list);
   renderer.submit(frame).expect("submit image frame");
   renderer.readback_bgra8().expect("read image frame").2
}

fn patterned_pixels(width: u32, height: u32) -> Vec<u8>
{
   let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
   for y in 0..height
   {
      for x in 0..width
      {
         pixels.extend_from_slice(&[
            (x.wrapping_mul(17) ^ y.wrapping_mul(3)) as u8,
            (x.wrapping_mul(5) ^ y.wrapping_mul(29)) as u8,
            (x.wrapping_mul(11) ^ y.wrapping_mul(7)) as u8,
            255,
         ]);
      }
   }
   pixels
}

#[test]
fn immutable_policy_keeps_nonminified_images_shared_and_allows_explicit_private_staging()
{
   let mut renderer = MetalRenderer::new_default().expect("metal");
   let small = patterned_pixels(64, 64);
   let small_handle = renderer.image_create_rgba8_immutable(64, 64, &small, 64 * 4, false);
   let small_stats = renderer.image_residency_stats();
   assert_eq!(small_stats.shared_textures, 1);
   assert_eq!(small_stats.private_textures, 0);
   assert_eq!(small_stats.upload_command_buffers, 0);

   let large = patterned_pixels(512, 512);
   let large_handle = renderer.image_create_rgba8_immutable(512, 512, &large, 512 * 4, false);
   let large_stats = renderer.image_residency_stats();
   assert_eq!(large_stats.shared_textures, 2);
   assert_eq!(large_stats.private_textures, 0);
   assert_eq!(large_stats.private_uploads, 0);
   assert_eq!(large_stats.upload_command_buffers, 0);

   let minified = patterned_pixels(256, 256);
   let minified_handle = renderer.image_create_rgba8_immutable(
      256,
      256,
      &minified,
      256 * 4,
      true,
   );
   let minified_stats = renderer.image_residency_stats();
   assert_eq!(minified_stats.shared_textures, 3);
   assert_eq!(minified_stats.private_textures, 0);
   assert_eq!(minified_stats.mipmapped_textures, 1);
   assert_eq!(minified_stats.mip_levels, 11);
   assert_eq!(minified_stats.mipmap_generations, 1);
   assert_eq!(minified_stats.upload_command_buffers, 1);

   let private_handle = renderer.image_create_rgba8_immutable_for_benchmark(
      512,
      512,
      &large,
      512 * 4,
      true,
      false,
   );
   let private_stats = renderer.image_residency_stats();
   assert_eq!(private_stats.shared_textures, 3);
   assert_eq!(private_stats.private_textures, 1);
   assert_eq!(private_stats.private_uploads, 1);
   assert_eq!(private_stats.mipmap_generations, 1);
   assert_eq!(private_stats.upload_command_buffers, 2);
   assert!(private_stats.private_bytes >= 512 * 512 * 4);
   assert!(private_stats.staging_upload_bytes >= 512 * 512 * 4);

   renderer.image_release(small_handle);
   renderer.image_release(large_handle);
   renderer.image_release(minified_handle);
   renderer.image_release(private_handle);
   let released = renderer.image_residency_stats();
   assert_eq!(released.shared_textures, 0);
   assert_eq!(released.private_textures, 0);
   assert_eq!(released.shared_bytes, 0);
   assert_eq!(released.private_bytes, 0);
   assert_eq!(released.mip_levels, 0);
}

#[test]
fn dynamic_rgba_images_remain_shared_at_large_sizes()
{
   let mut renderer = MetalRenderer::new_default().expect("metal");
   let pixels = patterned_pixels(512, 512);
   let handle = renderer.image_create_rgba8(512, 512, &pixels, 512 * 4);
   let stats = renderer.image_residency_stats();
   assert_eq!(stats.shared_textures, 1);
   assert_eq!(stats.private_textures, 0);
   assert_eq!(stats.mipmap_generations, 0);
   assert_eq!(stats.upload_command_buffers, 0);
   renderer.image_release(handle);
}

#[test]
fn mipmapped_immutable_upload_and_partial_update_match_dynamic_pixels()
{
   let width = 64_u32;
   let height = 64_u32;
   let pixels = patterned_pixels(width, height);
   let mut dynamic = MetalRenderer::new_default().expect("dynamic metal");
   let dynamic_image = dynamic.image_create_rgba8(width, height, &pixels, width as usize * 4);
   let mut shared = MetalRenderer::new_default().expect("shared-mip metal");
   let shared_image = shared.image_create_rgba8_immutable_for_benchmark(
      width,
      height,
      &pixels,
      width as usize * 4,
      false,
      true,
   );
   let mut private = MetalRenderer::new_default().expect("private metal");
   let private_image = private.image_create_rgba8_immutable_for_benchmark(
      width,
      height,
      &pixels,
      width as usize * 4,
      true,
      true,
   );

   let patch = vec![37_u8; 13 * 9 * 4];
   let mut patched_pixels = pixels.clone();
   for row in 0..9_usize
   {
      let offset = ((11 + row) * width as usize + 7) * 4;
      patched_pixels[offset..offset + 13 * 4]
         .copy_from_slice(&patch[row * 13 * 4..(row + 1) * 13 * 4]);
   }
   dynamic.image_update_rgba8(dynamic_image, 7, 11, 13, 9, &patch, 13 * 4);
   shared.image_update_rgba8(shared_image, 7, 11, 13, 9, &patch, 13 * 4);
   private.image_update_rgba8(private_image, 7, 11, 13, 9, &patch, 13 * 4);
   let dynamic_pixels = render_image(&mut dynamic, dynamic_image, width, height, width, height);
   let shared_pixels = render_image(&mut shared, shared_image, width, height, width, height);
   let private_pixels = render_image(&mut private, private_image, width, height, width, height);
   assert_eq!(shared_pixels, dynamic_pixels);
   assert_eq!(private_pixels, dynamic_pixels);

   let mut rebuilt = MetalRenderer::new_default().expect("rebuilt mip metal");
   let rebuilt_image = rebuilt.image_create_rgba8_immutable_for_benchmark(
      width,
      height,
      &patched_pixels,
      width as usize * 4,
      false,
      true,
   );
   let rebuilt_minified = render_image(&mut rebuilt, rebuilt_image, 17, 17, width, height);
   let shared_minified = render_image(&mut shared, shared_image, 17, 17, width, height);
   let private_minified = render_image(&mut private, private_image, 17, 17, width, height);
   assert_eq!(shared_minified, rebuilt_minified);
   assert_eq!(private_minified, rebuilt_minified);

   let stats = shared.image_residency_stats();
   assert_eq!(stats.private_uploads, 0);
   assert_eq!(stats.mipmap_generations, 2);
   assert_eq!(stats.upload_command_buffers, 2);
   let stats = private.image_residency_stats();
   assert_eq!(stats.private_uploads, 2);
   assert_eq!(stats.mipmap_generations, 2);
   assert_eq!(stats.upload_command_buffers, 2);
}

#[test]
fn mipmapped_minification_reduces_checkerboard_aliasing()
{
   let source_size = 256_u32;
   let mut checker = Vec::with_capacity(source_size as usize * source_size as usize * 4);
   for y in 0..source_size
   {
      for x in 0..source_size
      {
         let value = if (x + y) & 1 == 0 { 0 } else { 255 };
         checker.extend_from_slice(&[value, value, value, 255]);
      }
   }

   let mut base = MetalRenderer::new_default().expect("base metal");
   let base_image = base.image_create_rgba8_immutable_for_benchmark(
      source_size,
      source_size,
      &checker,
      source_size as usize * 4,
      false,
      false,
   );
   let base_pixels = render_image(&mut base, base_image, 31, 31, source_size, source_size);
   let mut shared_mipmapped = MetalRenderer::new_default().expect("shared-mip metal");
   let shared_mipmapped_image = shared_mipmapped.image_create_rgba8_immutable_for_benchmark(
      source_size,
      source_size,
      &checker,
      source_size as usize * 4,
      false,
      true,
   );
   let shared_mipmapped_pixels = render_image(
      &mut shared_mipmapped,
      shared_mipmapped_image,
      31,
      31,
      source_size,
      source_size,
   );
   let mut mipmapped = MetalRenderer::new_default().expect("mipmapped metal");
   let mipmapped_image = mipmapped.image_create_rgba8_immutable_for_benchmark(
      source_size,
      source_size,
      &checker,
      source_size as usize * 4,
      true,
      true,
   );
   let mipmapped_pixels = render_image(
      &mut mipmapped,
      mipmapped_image,
      31,
      31,
      source_size,
      source_size,
   );
   assert_eq!(shared_mipmapped_pixels, mipmapped_pixels);

   let variance = |pixels: &[u8]| -> f64 {
      let samples = pixels.chunks_exact(4).map(|pixel| f64::from(pixel[0])).collect::<Vec<_>>();
      let mean = samples.iter().sum::<f64>() / samples.len() as f64;
      samples.iter().map(|sample| (sample - mean) * (sample - mean)).sum::<f64>()
         / samples.len() as f64
   };
   let base_variance = variance(&base_pixels);
   let mipmapped_variance = variance(&mipmapped_pixels);
   assert!(
      mipmapped_variance * 4.0 < base_variance,
      "mipmapped variance {mipmapped_variance} did not materially reduce base variance {base_variance}",
   );
   let stats = mipmapped.image_residency_stats();
   assert_eq!(stats.mipmapped_textures, 1);
   assert_eq!(stats.mip_levels, 9);
   assert_eq!(stats.mipmap_generations, 1);
}

#[test]
fn immutable_images_survive_cache_pressure_and_recreate_with_a_new_renderer()
{
   let size = 128_u32;
   let pixels = patterned_pixels(size, size);
   let mut renderer = MetalRenderer::new_default().expect("metal");
   let image = renderer.image_create_rgba8_immutable(
      size,
      size,
      &pixels,
      size as usize * 4,
      true,
   );
   renderer.purge_effect_targets();
   renderer.purge_layer_cache_for_memory_warning();
   renderer.purge_id_mask_field_cache();
   renderer.purge_prepared_chunks();
   let before_loss = render_image(&mut renderer, image, 43, 43, size, size);
   drop(renderer);

   let mut recreated = MetalRenderer::new_default().expect("recreated metal");
   let recreated_image = recreated.image_create_rgba8_immutable(
      size,
      size,
      &pixels,
      size as usize * 4,
      true,
   );
   let after_loss = render_image(&mut recreated, recreated_image, 43, 43, size, size);
   assert_eq!(after_loss, before_loss);
}
