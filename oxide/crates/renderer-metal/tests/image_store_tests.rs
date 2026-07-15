#![cfg(all(
   feature = "snapshot-tests",
   any(target_os = "macos", all(target_os = "ios", not(target_abi = "sim")))
))]

use oxide_image_store::{
   ImageRequest, ImageResidencyBackend, ImageStore, ImageStoreConfig, ImageUsage, ImageVariant,
};
use oxide_renderer_api::{self as api, Renderer};
use oxide_renderer_metal::MetalRenderer;
use std::sync::Arc;

fn solid_png(rgba: [u8; 4]) -> Arc<[u8]>
{
   let pixels = [rgba; 64].concat();
   let mut encoded = Vec::new();
   {
      let mut encoder = png::Encoder::new(&mut encoded, 8, 8);
      encoder.set_color(png::ColorType::Rgba);
      encoder.set_depth(png::BitDepth::Eight);
      encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
   }
   encoded.into()
}

fn image_request(source: u64, encoded: Arc<[u8]>, usage: ImageUsage) -> ImageRequest
{
   ImageRequest {
      variant: ImageVariant {
         source,
         revision: 1,
         display_width: 8,
         display_height: 8,
      },
      encoded,
      usage,
   }
}

fn image_store() -> ImageStore
{
   ImageStore::new(ImageStoreConfig {
      decoded_budget_bytes: 64 * 1024,
      gpu_budget_bytes: 64 * 1024,
      atlas_width: 64,
      atlas_height: 64,
      max_atlas_image_dimension: 16,
      gutter: 2,
   })
   .unwrap()
}

fn read_pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4]
{
   let index = (y as usize * width as usize + x as usize) * 4;
   [pixels[index], pixels[index + 1], pixels[index + 2], pixels[index + 3]]
}

fn image_chunk(id: u64, image: oxide_image_store::ResolvedImage, generation: u64, x: f32) -> api::RenderChunk
{
   api::RenderChunk::new(
      api::RenderChunkId(id),
      api::RenderChunkRevisions { resource: generation, ..api::RenderChunkRevisions::default() },
      api::DrawList {
         items: vec![api::DrawCmd::Image {
            tex: image.texture,
            dst: api::RectF::new(x, 0.0, 32.0, 32.0),
            src: image.source,
            alpha: 1.0,
         }],
         ..api::DrawList::default()
      },
      api::ChunkIndexMode::Local,
      &[api::RenderResourceDependency { image: image.texture, generation }],
   )
   .expect("image store prepared chunk")
}

#[test]
fn image_store_atlas_matches_standalone_rgba_and_has_no_neighbor_bleed()
{
   let red = solid_png([255, 0, 0, 255]);
   let green = solid_png([0, 255, 0, 255]);
   let mut atlas_renderer = MetalRenderer::new_default().expect("atlas metal");
   atlas_renderer.resize(64, 32, 1.0).expect("resize atlas");
   let mut atlas_store = image_store();
   let red_id = atlas_store.request(image_request(1, red.clone(), ImageUsage::Static));
   let green_id = atlas_store.request(image_request(2, green.clone(), ImageUsage::Static));
   assert_eq!(atlas_store.process_decode_jobs_inline(usize::MAX), 2);
   assert_eq!(atlas_store.upload_ready(&mut atlas_renderer), 2);
   let red_atlas = atlas_store.resolve(red_id).expect("red atlas image");
   let green_atlas = atlas_store.resolve(green_id).expect("green atlas image");
   assert_eq!(red_atlas.texture, green_atlas.texture);
   ImageResidencyBackend::image_append_rgba8(
      &mut atlas_renderer,
      red_atlas.texture,
      0,
      0,
      2,
      2,
      &[0; 3],
      8,
   );
   ImageResidencyBackend::image_append_rgba8(
      &mut atlas_renderer,
      red_atlas.texture,
      63,
      63,
      2,
      2,
      &[0; 16],
      8,
   );
   let mut atlas_draws = api::DrawList::default();
   atlas_draws.items.push(api::DrawCmd::Image {
      tex: red_atlas.texture,
      dst: api::RectF::new(0.0, 0.0, 32.0, 32.0),
      src: red_atlas.source,
      alpha: 1.0,
   });
   atlas_draws.items.push(api::DrawCmd::Image {
      tex: green_atlas.texture,
      dst: api::RectF::new(32.0, 0.0, 32.0, 32.0),
      src: green_atlas.source,
      alpha: 1.0,
   });
   let frame = atlas_renderer.begin_frame(&api::FrameTarget, None);
   atlas_renderer.encode_pass(&atlas_draws);
   atlas_renderer.submit(frame).expect("submit atlas frame");
   let atlas_pixels = atlas_renderer.readback_bgra8().expect("read atlas frame").2;

   let mut standalone_renderer = MetalRenderer::new_default().expect("standalone metal");
   standalone_renderer.resize(64, 32, 1.0).expect("resize standalone");
   let mut standalone_store = image_store();
   let red_id = standalone_store.request(image_request(1, red, ImageUsage::Standalone));
   let green_id = standalone_store.request(image_request(2, green, ImageUsage::Standalone));
   assert_eq!(standalone_store.process_decode_jobs_inline(usize::MAX), 2);
   assert_eq!(standalone_store.upload_ready(&mut standalone_renderer), 2);
   let red_image = standalone_store.resolve(red_id).expect("red standalone image");
   let green_image = standalone_store.resolve(green_id).expect("green standalone image");
   let mut standalone_draws = api::DrawList::default();
   standalone_draws.items.push(api::DrawCmd::Image {
      tex: red_image.texture,
      dst: api::RectF::new(0.0, 0.0, 32.0, 32.0),
      src: red_image.source,
      alpha: 1.0,
   });
   standalone_draws.items.push(api::DrawCmd::Image {
      tex: green_image.texture,
      dst: api::RectF::new(32.0, 0.0, 32.0, 32.0),
      src: green_image.source,
      alpha: 1.0,
   });
   let frame = standalone_renderer.begin_frame(&api::FrameTarget, None);
   standalone_renderer.encode_pass(&standalone_draws);
   standalone_renderer.submit(frame).expect("submit standalone frame");
   let standalone_pixels = standalone_renderer.readback_bgra8().expect("read standalone frame").2;

   assert_eq!(atlas_pixels, standalone_pixels);
   for (x, expected) in [
      (0, [0, 0, 255, 255]),
      (15, [0, 0, 255, 255]),
      (31, [0, 0, 255, 255]),
      (32, [0, 255, 0, 255]),
      (48, [0, 255, 0, 255]),
      (63, [0, 255, 0, 255]),
   ]
   {
      assert_eq!(read_pixel(&atlas_pixels, 64, x, 16), expected, "pixel at x={x}");
   }
}

#[test]
fn image_store_slot_eviction_invalidates_only_its_prepared_chunk()
{
   let encoded = solid_png([90, 120, 220, 255]);
   let mut renderer = MetalRenderer::new_default().expect("metal");
   renderer.resize(64, 32, 1.0).expect("resize");
   let mut store = image_store();
   let first_id = store.request(image_request(1, encoded.clone(), ImageUsage::Static));
   let second_id = store.request(image_request(2, encoded, ImageUsage::Static));
   assert_eq!(store.process_decode_jobs_inline(usize::MAX), 2);
   assert_eq!(store.upload_ready(&mut renderer), 2);
   let first = store.resolve_for_chunk(first_id, api::RenderChunkId(101)).expect("first image");
   let second = store.resolve_for_chunk(second_id, api::RenderChunkId(202)).expect("second image");
   assert_eq!(first.texture, second.texture);
   let generation = renderer.image_generation(first.texture).expect("atlas generation");
   let snapshot = api::RenderSnapshot::new(
      vec![
         api::RenderChunkInstance::new(image_chunk(101, first, generation, 0.0), [0.0, 0.0]),
         api::RenderChunkInstance::new(image_chunk(202, second, generation, 32.0), [0.0, 0.0]),
      ],
      Vec::new(),
      api::Damage { rects: Vec::new() },
   )
   .expect("image store snapshot");
   let frame = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&snapshot).expect("prepare image chunks");
   renderer.submit(frame).expect("submit image chunks");
   let _ = renderer.readback_bgra8().expect("drain image chunks");
   assert_eq!(renderer.prepared_cache_entry_count(), 2);

   assert!(store.release(first_id, &mut renderer));
   assert_eq!(renderer.prepared_cache_entry_count(), 1);
   assert!(store.release(second_id, &mut renderer));
   assert_eq!(renderer.prepared_cache_entry_count(), 0);
   assert_eq!(store.drain_invalidated_chunks().collect::<Vec<_>>(), [
      api::RenderChunkId(101),
      api::RenderChunkId(202),
   ]);
}

#[test]
fn image_store_slot_eviction_invalidates_only_its_retained_layer()
{
   let encoded = solid_png([90, 120, 220, 255]);
   let mut renderer = MetalRenderer::new_default().expect("metal");
   renderer.resize(64, 32, 1.0).expect("resize");
   let mut store = image_store();
   let first_id = store.request(image_request(1, encoded.clone(), ImageUsage::Static));
   let second_id = store.request(image_request(2, encoded, ImageUsage::Static));
   assert_eq!(store.process_decode_jobs_inline(usize::MAX), 2);
   assert_eq!(store.upload_ready(&mut renderer), 2);
   let first = store.resolve_for_chunk(first_id, api::RenderChunkId(101)).expect("first image");
   let second = store.resolve_for_chunk(second_id, api::RenderChunkId(202)).expect("second image");
   let generation = renderer.image_generation(first.texture).expect("atlas generation");
   let mut first_instance = api::RenderChunkInstance::new(
      image_chunk(101, first, generation, 0.0),
      [0.0, 0.0],
   );
   first_instance.layer = Some(api::RenderLayerInstance {
      id: 1,
      rect: api::RectF::new(0.0, 0.0, 32.0, 32.0),
      dirty: false,
   });
   let mut second_instance = api::RenderChunkInstance::new(
      image_chunk(202, second, generation, 0.0),
      [32.0, 0.0],
   );
   second_instance.layer = Some(api::RenderLayerInstance {
      id: 2,
      rect: api::RectF::new(0.0, 0.0, 32.0, 32.0),
      dirty: false,
   });
   let snapshot = api::RenderSnapshot::new(
      vec![first_instance, second_instance],
      Vec::new(),
      api::Damage { rects: Vec::new() },
   )
   .expect("image store layer snapshot");
   for _ in 0..2
   {
      let frame = renderer.begin_frame(&api::FrameTarget, None);
      renderer.encode_snapshot(&snapshot).expect("encode image layers");
      renderer.submit(frame).expect("submit image layers");
      let _ = renderer.readback_bgra8().expect("drain image layers");
   }
   assert_eq!(renderer.last_stats().layer_cache_hits, 2);

   assert!(store.release(first_id, &mut renderer));
   let frame = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&snapshot).expect("encode invalidated image layer");
   renderer.submit(frame).expect("submit invalidated image layer");
   let stats = renderer.last_stats();
   let _ = renderer.readback_bgra8().expect("drain invalidated image layer");
   assert_eq!(stats.layer_cache_misses, 1);
   assert_eq!(stats.layer_cache_hits, 1);
}
