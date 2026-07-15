use oxide_image_store::{
   decode_png_at_display_size, DecodedImage, ImageId, ImageRequest, ImageResidencyBackend,
   ImageStatus, ImageStore, ImageStoreConfig, ImageStoreConfigError, ImageUsage,
   ImageVariant, NativeDecodePool,
};
use oxide_renderer_api::{ImageHandle, RenderChunkId};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone)]
struct Texture
{
   width: u32,
   height: u32,
   rgba: Vec<u8>,
   mipmapped: bool,
}

struct MockBackend
{
   generation: u64,
   next: u32,
   textures: HashMap<u32, Texture>,
   releases: u64,
   invalidated_chunks: Vec<RenderChunkId>,
}

impl MockBackend
{
   fn new() -> Self
   {
      Self {
         generation: 1,
         next: 1,
         textures: HashMap::new(),
         releases: 0,
         invalidated_chunks: Vec::new(),
      }
   }
}

impl ImageResidencyBackend for MockBackend
{
   fn image_device_generation(&self) -> u64
   {
      self.generation
   }

   fn image_create_rgba8(&mut self, width: u32, height: u32, data: &[u8], row_bytes: usize, mipmapped: bool) -> ImageHandle
   {
      let handle = ImageHandle(self.next);
      self.next += 1;
      let mut rgba = vec![0; width as usize * height as usize * 4];
      for row in 0..height as usize
      {
         let source = row * row_bytes;
         let target = row * width as usize * 4;
         rgba[target..target + width as usize * 4]
            .copy_from_slice(&data[source..source + width as usize * 4]);
      }
      self.textures.insert(handle.0, Texture { width, height, rgba, mipmapped });
      handle
   }

   fn image_create_rgba8_empty(&mut self, width: u32, height: u32) -> ImageHandle
   {
      let handle = ImageHandle(self.next);
      self.next += 1;
      self.textures.insert(handle.0, Texture {
         width,
         height,
         rgba: vec![0; width as usize * height as usize * 4],
         mipmapped: false,
      });
      handle
   }

   fn image_append_rgba8(&mut self, handle: ImageHandle, x: u32, y: u32, width: u32, height: u32, data: &[u8], row_bytes: usize)
   {
      let texture = self.textures.get_mut(&handle.0).expect("atlas texture");
      assert!(x + width <= texture.width && y + height <= texture.height);
      for row in 0..height as usize
      {
         let source = row * row_bytes;
         let target = ((y as usize + row) * texture.width as usize + x as usize) * 4;
         texture.rgba[target..target + width as usize * 4]
            .copy_from_slice(&data[source..source + width as usize * 4]);
      }
   }

   fn image_release(&mut self, handle: ImageHandle)
   {
      if self.textures.remove(&handle.0).is_some()
      {
         self.releases += 1;
      }
   }

   fn image_invalidate_chunks(&mut self, chunks: &[RenderChunkId])
   {
      self.invalidated_chunks.extend_from_slice(chunks);
   }
}

fn config() -> ImageStoreConfig
{
   ImageStoreConfig {
      decoded_budget_bytes: 4 * 1024 * 1024,
      gpu_budget_bytes: 4 * 1024 * 1024,
      atlas_width: 64,
      atlas_height: 64,
      max_atlas_image_dimension: 16,
      gutter: 2,
   }
}

fn store() -> ImageStore
{
   ImageStore::new(config()).unwrap()
}

fn png(width: u32, height: u32, pixels: impl Fn(u32, u32) -> [u8; 4]) -> Arc<[u8]>
{
   let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
   for y in 0..height
   {
      for x in 0..width
      {
         rgba.extend_from_slice(&pixels(x, y));
      }
   }
   let mut encoded = Vec::new();
   {
      let mut encoder = png::Encoder::new(&mut encoded, width, height);
      encoder.set_color(png::ColorType::Rgba);
      encoder.set_depth(png::BitDepth::Eight);
      encoder.write_header().unwrap().write_image_data(&rgba).unwrap();
   }
   encoded.into()
}

fn request(source: u64, width: u32, height: u32, usage: ImageUsage, encoded: Arc<[u8]>) -> ImageRequest
{
   ImageRequest {
      variant: ImageVariant {
         source,
         revision: 1,
         display_width: width,
         display_height: height,
      },
      encoded,
      usage,
   }
}

fn populate(store: &mut ImageStore, backend: &mut MockBackend, requests: impl IntoIterator<Item = ImageRequest>) -> Vec<ImageId>
{
   let ids: Vec<_> = requests.into_iter().map(|request| store.request(request)).collect();
   store.process_decode_jobs_inline(usize::MAX);
   store.upload_ready(backend);
   ids
}

#[test]
fn invalid_store_configuration_returns_typed_errors()
{
   let mut invalid = config();
   invalid.decoded_budget_bytes = 0;
   assert_eq!(ImageStore::new(invalid).err(), Some(ImageStoreConfigError::ZeroDecodedBudget));

   invalid = config();
   invalid.gpu_budget_bytes = 0;
   assert_eq!(ImageStore::new(invalid).err(), Some(ImageStoreConfigError::ZeroGpuBudget));

   invalid = config();
   invalid.atlas_width = 0;
   assert_eq!(ImageStore::new(invalid).err(), Some(ImageStoreConfigError::ZeroAtlasDimension));

   invalid = config();
   invalid.gutter = 0;
   assert_eq!(ImageStore::new(invalid).err(), Some(ImageStoreConfigError::ZeroGutter));
}

#[test]
fn decode_is_display_sized_and_variant_hits_keep_identity()
{
   let encoded = png(16, 16, |x, y| [x as u8, y as u8, 90, 255]);
   let mut store = store();
   let first = store.request(request(1, 8, 6, ImageUsage::Static, encoded.clone()));
   let second = store.request(request(1, 8, 6, ImageUsage::Static, encoded.clone()));
   let minified = store.request(request(1, 8, 6, ImageUsage::RepeatedlyMinified, encoded));
   assert_eq!(first, second);
   assert_ne!(first, minified);
   assert_eq!(store.process_decode_jobs_inline(usize::MAX), 2);
   let mut backend = MockBackend::new();
   assert_eq!(store.upload_ready(&mut backend), 2);
   let resolved = store.resolve(first).unwrap();
   assert_eq!((resolved.width, resolved.height), (8, 6));
   assert!(!resolved.standalone);
   let stats = store.stats();
   assert_eq!(stats.requests, 3);
   assert_eq!(stats.variant_cache_hits, 1);
   assert_eq!(stats.decoded_output_bytes, 8 * 6 * 4 * 2);
}

#[test]
fn completed_variants_do_not_retain_the_encoded_source_allocation()
{
   let encoded = png(16, 16, |x, y| [x as u8, y as u8, 90, 255]);
   let encoded_weak = Arc::downgrade(&encoded);
   let mut store = store();
   store.request(request(1, 8, 8, ImageUsage::Static, encoded.clone()));
   drop(encoded);
   assert!(encoded_weak.upgrade().is_some());
   assert_eq!(store.process_decode_jobs_inline(usize::MAX), 1);
   assert!(encoded_weak.upgrade().is_none());
}

#[test]
fn canceled_and_stale_completions_never_publish()
{
   let encoded = png(8, 8, |_, _| [12, 34, 56, 255]);
   let mut store = store();
   let id = store.request(request(1, 8, 8, ImageUsage::Static, encoded.clone()));
   let job = store.take_decode_job().unwrap();
   assert!(store.cancel(id));
   assert!(!store.complete_decode(decode_png_at_display_size(job)));
   assert_eq!(store.status(id), ImageStatus::Canceled);
   assert_eq!(store.stats().stale_completions, 1);

   let decoded = store.request(request(2, 8, 8, ImageUsage::Static, encoded));
   assert_eq!(store.process_decode_jobs_inline(usize::MAX), 1);
   assert_eq!(store.status(decoded), ImageStatus::Decoded);
   assert!(store.cancel(decoded));
   let mut backend = MockBackend::new();
   assert_eq!(store.upload_ready(&mut backend), 0);
   assert_eq!(store.status(decoded), ImageStatus::Canceled);
   assert_eq!(store.stats().decoded_resident_bytes, 0);
   assert_eq!(store.stats().canceled_jobs, 2);
}

#[test]
fn malformed_decode_completion_never_reaches_gpu_publication()
{
   let encoded = png(8, 8, |_, _| [12, 34, 56, 255]);
   let mut store = store();
   let id = store.request(request(1, 8, 8, ImageUsage::Static, encoded));
   let job = store.take_decode_job().unwrap();
   let malformed = DecodedImage { width: 8, height: 8, rgba: vec![0; 8 * 8 * 4 - 1] };
   assert!(!store.complete_decode(job.complete(Ok(malformed))));
   assert_eq!(store.status(id), ImageStatus::Failed);
   assert_eq!(store.stats().decode_failures, 1);
   let mut backend = MockBackend::new();
   assert_eq!(store.upload_ready(&mut backend), 0);
   assert!(backend.textures.is_empty());
}

#[test]
fn atlas_gutters_repeat_only_the_owning_image_edges()
{
   let encoded = png(2, 2, |x, y| match (x, y) {
      (0, 0) => [255, 0, 0, 255],
      (1, 0) => [0, 255, 0, 255],
      (0, 1) => [0, 0, 255, 255],
      _ => [255, 255, 255, 255],
   });
   let mut store = store();
   let mut backend = MockBackend::new();
   let id = populate(&mut store, &mut backend, [request(1, 2, 2, ImageUsage::Static, encoded)])[0];
   let resolved = store.resolve(id).unwrap();
   let texture = backend.textures.get(&resolved.texture.0).unwrap();
   let pixel = |x: u32, y: u32| {
      let index = (y as usize * texture.width as usize + x as usize) * 4;
      &texture.rgba[index..index + 4]
   };
   let x = resolved.source.x as u32;
   let y = resolved.source.y as u32;
   assert_eq!(pixel(x, y), [255, 0, 0, 255]);
   assert_eq!(pixel(x - 1, y), [255, 0, 0, 255]);
   assert_eq!(pixel(x + 2, y), [0, 255, 0, 255]);
   assert_eq!(pixel(x, y + 2), [0, 0, 255, 255]);
   assert_eq!(store.stats().atlas_page_clear_bytes, 0);
}

#[test]
fn atlas_reuse_invalidates_only_referencing_chunks_and_checks_generations()
{
   let encoded = png(8, 8, |_, _| [20, 40, 60, 255]);
   let mut store = store();
   let mut backend = MockBackend::new();
   let ids = populate(
      &mut store,
      &mut backend,
      [
         request(1, 8, 8, ImageUsage::Static, encoded.clone()),
         request(2, 8, 8, ImageUsage::Static, encoded.clone()),
      ],
   );
   let first = store.resolve_for_chunk(ids[0], RenderChunkId(101)).unwrap();
   let second = store.resolve_for_chunk(ids[1], RenderChunkId(202)).unwrap();
   assert_eq!(first.texture, second.texture);
   assert!(store.release(ids[0], &mut backend));
   assert_eq!(backend.invalidated_chunks, [RenderChunkId(101)]);
   assert_eq!(store.drain_invalidated_chunks().collect::<Vec<_>>(), [RenderChunkId(101)]);
   assert!(store.resolve(ids[0]).is_none());
   let replacement = populate(
      &mut store,
      &mut backend,
      [request(3, 8, 8, ImageUsage::Static, encoded)],
   )[0];
   let replacement = store.resolve(replacement).unwrap();
   assert_ne!(first.slot_generation, replacement.slot_generation);
   assert_eq!(store.stats().invalidated_chunks, 1);
}

#[test]
fn unsuitable_images_stay_standalone_and_minified_images_request_mips()
{
   let encoded = png(32, 32, |_, _| [90, 80, 70, 255]);
   let mut store = store();
   let mut backend = MockBackend::new();
   let ids = populate(
      &mut store,
      &mut backend,
      [
         request(1, 32, 32, ImageUsage::Static, encoded.clone()),
         request(2, 8, 8, ImageUsage::RapidlyChanging, encoded.clone()),
         request(3, 8, 8, ImageUsage::Video, encoded.clone()),
         request(4, 8, 8, ImageUsage::CompressedIncompatible, encoded.clone()),
         request(5, 8, 8, ImageUsage::RepeatedlyMinified, encoded),
      ],
   );
   for id in &ids
   {
      assert!(store.resolve(*id).unwrap().standalone);
   }
   let minified = store.resolve(ids[4]).unwrap();
   assert!(backend.textures.get(&minified.texture.0).unwrap().mipmapped);
   assert_eq!(store.stats().standalone_images, 5);
}

#[test]
fn memory_pressure_and_device_loss_purge_exact_residency()
{
   let encoded = png(8, 8, |_, _| [1, 2, 3, 255]);
   let mut store = store();
   let mut backend = MockBackend::new();
   let ids = populate(
      &mut store,
      &mut backend,
      [
         request(1, 8, 8, ImageUsage::Static, encoded.clone()),
         request(2, 8, 8, ImageUsage::Standalone, encoded),
      ],
   );
   assert!(store.stats().gpu_resident_bytes > 0);
   backend.generation += 1;
   store.synchronize_device(&mut backend);
   assert_eq!(store.stats().gpu_resident_bytes, 0);
   assert_eq!(store.stats().device_loss_purges, 1);
   assert_eq!(store.upload_ready(&mut backend), 2);
   assert_eq!(store.stats().first_publication_count, 2);
   store.resolve_for_chunk(ids[0], RenderChunkId(9)).unwrap();
   store.purge_for_memory_warning(&mut backend);
   assert_eq!(store.stats().gpu_resident_bytes, 0);
   assert_eq!(store.stats().decoded_resident_bytes, 0);
   assert_eq!(store.stats().memory_warning_purges, 1);
   assert_eq!(store.drain_invalidated_chunks().collect::<Vec<_>>(), [RenderChunkId(9)]);
}

#[test]
fn memory_warning_cancels_queued_decode_and_allows_explicit_restart()
{
   let encoded = png(8, 8, |_, _| [1, 2, 3, 255]);
   let mut store = store();
   let mut backend = MockBackend::new();
   let id = store.request(request(1, 8, 8, ImageUsage::Static, encoded.clone()));
   store.purge_for_memory_warning(&mut backend);
   assert_eq!(store.status(id), ImageStatus::Canceled);
   assert!(store.take_decode_job().is_none());
   assert_eq!(store.stats().canceled_jobs, 1);

   assert_eq!(store.request(request(1, 8, 8, ImageUsage::Static, encoded)), id);
   assert_eq!(store.process_decode_jobs_inline(usize::MAX), 1);
   assert_eq!(store.upload_ready(&mut backend), 1);
   assert_eq!(store.status(id), ImageStatus::Resident);
}

#[test]
fn invalidation_deduplicates_a_chunk_shared_by_multiple_images()
{
   let encoded = png(8, 8, |_, _| [1, 2, 3, 255]);
   let mut store = store();
   let mut backend = MockBackend::new();
   let ids = populate(
      &mut store,
      &mut backend,
      [
         request(1, 8, 8, ImageUsage::Static, encoded.clone()),
         request(2, 8, 8, ImageUsage::Static, encoded),
      ],
   );
   let chunk = RenderChunkId(77);
   store.resolve_for_chunk(ids[0], chunk).unwrap();
   store.resolve_for_chunk(ids[1], chunk).unwrap();
   store.purge_for_memory_warning(&mut backend);
   assert_eq!(backend.invalidated_chunks, [chunk]);
   assert_eq!(store.drain_invalidated_chunks().collect::<Vec<_>>(), [chunk]);
}

#[test]
fn release_and_reuse_changes_the_logical_generation()
{
   let encoded = png(4, 4, |_, _| [9, 8, 7, 255]);
   let mut store = store();
   let mut backend = MockBackend::new();
   let first = populate(
      &mut store,
      &mut backend,
      [request(1, 4, 4, ImageUsage::Static, encoded.clone())],
   )[0];
   assert!(store.release(first, &mut backend));
   let second = store.request(request(1, 4, 4, ImageUsage::Static, encoded));
   assert_ne!(first, second);
   assert_eq!(first.raw() as u32, second.raw() as u32);
   assert_eq!(store.status(first), ImageStatus::Missing);
}

#[test]
fn scrolling_release_and_reuse_invalidates_only_visible_chunk_owners()
{
   let encoded = png(4, 4, |x, y| [x as u8 * 40, y as u8 * 40, 90, 255]);
   let mut store = store();
   let mut backend = MockBackend::new();
   let ids = populate(
      &mut store,
      &mut backend,
      (0..512_u64).map(|source| request(source + 1, 4, 4, ImageUsage::Static, encoded.clone())),
   );
   let mut released = Vec::new();
   let mut expected_chunks = Vec::new();
   for step in 0..64_usize
   {
      let index = step * 73 % ids.len();
      let chunk = RenderChunkId(10_000 + step as u64);
      assert!(store.resolve_for_chunk(ids[index], chunk).is_some());
      released.push(ids[index]);
      expected_chunks.push(chunk);
   }
   for id in &released
   {
      assert!(store.release(*id, &mut backend));
   }
   assert_eq!(backend.invalidated_chunks, expected_chunks);
   assert_eq!(store.drain_invalidated_chunks().collect::<Vec<_>>(), expected_chunks);

   let replacements: Vec<_> = (0..released.len())
      .map(|index| store.request(request(20_000 + index as u64, 4, 4, ImageUsage::Static, encoded.clone())))
      .collect();
   store.process_decode_jobs_inline(usize::MAX);
   assert_eq!(store.upload_ready(&mut backend), replacements.len());
   assert!(replacements.iter().all(|id| store.resolve(*id).is_some()));
   assert!(store.stats().decoded_resident_bytes <= store.config().decoded_budget_bytes);
   assert!(store.stats().gpu_resident_bytes <= store.config().gpu_budget_bytes);
}

#[test]
fn native_pool_decodes_off_the_requesting_thread()
{
   let encoded = png(64, 64, |x, y| [x as u8, y as u8, 4, 255]);
   let mut store = store();
   let id = store.request(request(1, 16, 16, ImageUsage::Static, encoded));
   let pool = NativeDecodePool::new(2);
   assert_eq!(pool.dispatch(&mut store, 1), 1);
   let deadline = Instant::now() + Duration::from_secs(2);
   while pool.collect(&mut store) == 0 && Instant::now() < deadline
   {
      std::thread::yield_now();
   }
   assert_eq!(store.status(id), ImageStatus::Decoded);
}

#[test]
fn ten_thousand_requests_remain_within_hard_cpu_and_gpu_budgets()
{
   let encoded = png(4, 4, |x, y| [x as u8, y as u8, 1, 255]);
   let mut bounded = config();
   bounded.decoded_budget_bytes = 512 * 1024;
   bounded.gpu_budget_bytes = 256 * 1024;
   let mut store = ImageStore::new(bounded).unwrap();
   let mut backend = MockBackend::new();
   let mut first = ImageId::INVALID;
   let mut recent = ImageId::INVALID;
   let mut last = ImageId::INVALID;
   for base in (0..10_000_u64).step_by(250)
   {
      for index in base..base + 250
      {
         let id = store.request(request(index + 1, 4, 4, ImageUsage::Static, encoded.clone()));
         if index == 0
         {
            first = id;
         }
         if index == 9_000
         {
            recent = id;
         }
         last = id;
      }
      store.process_decode_jobs_inline(usize::MAX);
      store.upload_ready(&mut backend);
   }
   let stats = store.stats();
   assert!(stats.decoded_resident_bytes <= bounded.decoded_budget_bytes);
   assert!(stats.gpu_resident_bytes <= bounded.gpu_budget_bytes);
   assert_eq!(stats.texture_creates, bounded.gpu_budget_bytes / (64 * 64 * 4));
   assert_eq!(stats.texture_releases, 0);
   assert!(stats.atlas_pages <= bounded.gpu_budget_bytes / (64 * 64 * 4));
   assert_ne!(store.status(first), ImageStatus::Resident);
   assert_eq!(store.status(recent), ImageStatus::Resident);
   assert_eq!(store.status(last), ImageStatus::Resident);
}
