//! Generation-safe asynchronous image decoding and bounded GPU residency.

use oxide_renderer_api::{ImageHandle, RectF, RenderChunkId};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const RGBA_BYTES_PER_PIXEL: u64 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// Opaque logical image identity containing a reusable slot and generation.
pub struct ImageId(u64);

impl ImageId
{
   /// Sentinel returned when a request cannot form a valid variant.
   pub const INVALID: Self = Self(0);

   /// Returns the packed identity for persistence in host-owned value structures.
   pub const fn raw(self) -> u64
   {
      self.0
   }

   /// Returns whether this is a non-sentinel identity.
   pub const fn is_valid(self) -> bool
   {
      self.0 != 0
   }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// Cache key for one source revision decoded at one display size.
pub struct ImageVariant
{
   /// Caller-stable source identity.
   pub source: u64,
   /// Caller-owned content revision for the source identity.
   pub revision: u64,
   /// Requested decoded width in pixels.
   pub display_width: u32,
   /// Requested decoded height in pixels.
   pub display_height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// Residency policy selected from the image's update and sampling behavior.
pub enum ImageUsage
{
   /// Small immutable image eligible for atlas placement.
   Static,
   /// Immutable image that needs a complete standalone mip chain.
   RepeatedlyMinified,
   /// Frequently replaced image that must stay standalone.
   RapidlyChanging,
   /// Video frame or stream-owned image that must stay standalone.
   Video,
   /// Source whose compressed representation cannot enter the RGBA atlas.
   CompressedIncompatible,
   /// Explicit standalone override.
   Standalone,
}

impl ImageUsage
{
   const fn atlas_eligible(self) -> bool
   {
      matches!(self, Self::Static)
   }

   const fn needs_mips(self) -> bool
   {
      matches!(self, Self::RepeatedlyMinified)
   }
}

#[derive(Clone)]
/// One logical image request and its encoded source ownership.
pub struct ImageRequest
{
   /// Variant identity and requested display size.
   pub variant: ImageVariant,
   /// Encoded source retained only until decode completes.
   pub encoded: Arc<[u8]>,
   /// Sampling/update policy.
   pub usage: ImageUsage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Hard decoded/GPU budgets and atlas geometry.
pub struct ImageStoreConfig
{
   /// Maximum live decoded RGBA bytes.
   pub decoded_budget_bytes: u64,
   /// Maximum logical live GPU texture bytes.
   pub gpu_budget_bytes: u64,
   /// Atlas page width in pixels.
   pub atlas_width: u32,
   /// Atlas page height in pixels.
   pub atlas_height: u32,
   /// Largest image dimension eligible for paging.
   pub max_atlas_image_dimension: u32,
   /// Repeated-edge padding on every side of a paged image.
   pub gutter: u32,
}

impl Default for ImageStoreConfig
{
   fn default() -> Self
   {
      Self {
         decoded_budget_bytes: 64 * 1024 * 1024,
         gpu_budget_bytes: 128 * 1024 * 1024,
         atlas_width: 512,
         atlas_height: 512,
         max_atlas_image_dimension: 128,
         gutter: 2,
      }
   }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Invalid hard-budget or atlas geometry supplied to `ImageStore::new`.
pub enum ImageStoreConfigError
{
   /// The decoded-byte budget was zero.
   ZeroDecodedBudget,
   /// The GPU-byte budget was zero.
   ZeroGpuBudget,
   /// At least one atlas dimension was zero.
   ZeroAtlasDimension,
   /// The repeated-edge gutter was zero.
   ZeroGutter,
}

impl fmt::Display for ImageStoreConfigError
{
   fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
   {
      f.write_str(match self
      {
         Self::ZeroDecodedBudget => "decoded image budget must be nonzero",
         Self::ZeroGpuBudget => "GPU image budget must be nonzero",
         Self::ZeroAtlasDimension => "atlas dimensions must be nonzero",
         Self::ZeroGutter => "atlas gutter must be nonzero",
      })
   }
}

impl std::error::Error for ImageStoreConfigError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
/// Exact cumulative work and current/peak residency counters.
pub struct ImageStoreStats
{
   /// Total request calls.
   pub requests: u64,
   /// Requests satisfied by an existing logical variant.
   pub variant_cache_hits: u64,
   /// Decode jobs created.
   pub decode_jobs: u64,
   /// Current completions accepted for accounting.
   pub decode_completions: u64,
   /// Current decode completions that failed.
   pub decode_failures: u64,
   /// Requests canceled before GPU publication.
   pub canceled_jobs: u64,
   /// Completion messages rejected by identity, serial, or cancellation.
   pub stale_completions: u64,
   /// Encoded bytes consumed by accepted completions.
   pub encoded_input_bytes: u64,
   /// Decoded RGBA bytes produced cumulatively.
   pub decoded_output_bytes: u64,
   /// Decoded RGBA bytes currently retained.
   pub decoded_resident_bytes: u64,
   /// Maximum decoded RGBA residency observed.
   pub decoded_peak_bytes: u64,
   /// Aggregate accepted decode latency in nanoseconds.
   pub decode_time_ns: u64,
   /// Logical bytes published into texture storage.
   pub upload_bytes: u64,
   /// Zero-fill bytes uploaded while creating atlas pages; production backends keep this zero.
   pub atlas_page_clear_bytes: u64,
   /// Logical GPU texture bytes currently retained.
   pub gpu_resident_bytes: u64,
   /// Maximum logical GPU texture residency observed.
   pub gpu_peak_bytes: u64,
   /// Texture creation calls.
   pub texture_creates: u64,
   /// Texture release calls.
   pub texture_releases: u64,
   /// Live atlas page count.
   pub atlas_pages: u64,
   /// Live occupied atlas slot count.
   pub atlas_slots: u64,
   /// Live standalone image count.
   pub standalone_images: u64,
   /// Residency removals, including explicit release and pressure eviction.
   pub gpu_evictions: u64,
   /// Decoded-buffer pressure evictions.
   pub decoded_evictions: u64,
   /// Unique prepared-chunk invalidation emissions accumulated across drain batches.
   pub invalidated_chunks: u64,
   /// Completed memory-warning purges.
   pub memory_warning_purges: u64,
   /// Completed backend-generation purges.
   pub device_loss_purges: u64,
   /// Request identities published to GPU residency for the first time.
   pub first_publication_count: u64,
   /// Aggregate request-to-first-publication latency in nanoseconds.
   pub request_to_first_publication_ns: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Observable lifecycle state for a generation-checked image.
pub enum ImageStatus
{
   /// Identity is stale or has no retained representation.
   Missing,
   /// Decode is queued or in flight.
   Pending,
   /// Decoded bytes are available but no GPU placement is live.
   Decoded,
   /// A generation-checked GPU placement is live.
   Resident,
   /// The current request failed.
   Failed,
   /// The current request was canceled before publication.
   Canceled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Decode or admission failure for one request serial.
pub enum ImageDecodeError
{
   /// Cancellation was observed before completion.
   Canceled,
   /// A source or target dimension was zero or overflowed.
   InvalidDimensions,
   /// The decoded output format cannot be normalized to RGBA8.
   UnsupportedFormat,
   /// Decoder- or browser-provided failure detail.
   Decode(String),
   /// One decoded image exceeds the complete decoded-byte budget.
   BudgetExceeded,
}

impl fmt::Display for ImageDecodeError
{
   fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
   {
      match self
      {
         Self::Canceled => f.write_str("image decode canceled"),
         Self::InvalidDimensions => f.write_str("image dimensions are invalid"),
         Self::UnsupportedFormat => f.write_str("decoded image format is unsupported"),
         Self::Decode(message) => write!(f, "image decode failed: {message}"),
         Self::BudgetExceeded => f.write_str("decoded image exceeds the configured byte budget"),
      }
   }
}

impl std::error::Error for ImageDecodeError {}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Display-sized straight-alpha RGBA8 output.
pub struct DecodedImage
{
   /// Pixel width.
   pub width: u32,
   /// Pixel height.
   pub height: u32,
   /// Tightly packed RGBA8 pixels.
   pub rgba: Vec<u8>,
}

#[derive(Clone)]
/// Owned, transferable decode work for one request serial.
pub struct DecodeJob
{
   id: ImageId,
   serial: u64,
   encoded: Arc<[u8]>,
   display_width: u32,
   display_height: u32,
   canceled: Arc<AtomicBool>,
   started: Instant,
}

impl DecodeJob
{
   /// Returns the generation-checked logical image identity.
   pub const fn image_id(&self) -> ImageId
   {
      self.id
   }

   /// Returns the requested display dimensions.
   pub const fn display_size(&self) -> (u32, u32)
   {
      (self.display_width, self.display_height)
   }

   /// Borrows the encoded source.
   pub fn encoded(&self) -> &[u8]
   {
      &self.encoded
   }

   /// Returns whether the owner canceled this serial.
   pub fn is_canceled(&self) -> bool
   {
      self.canceled.load(Ordering::Acquire)
   }

   /// Resets decode timing at the actual execution boundary.
   pub fn mark_started(&mut self)
   {
      self.started = Instant::now();
   }

   /// Consumes the job and packages its result for store-thread publication.
   pub fn complete(self, result: Result<DecodedImage, ImageDecodeError>) -> DecodeCompletion
   {
      DecodeCompletion {
         id: self.id,
         serial: self.serial,
         encoded_bytes: self.encoded.len() as u64,
         elapsed: self.started.elapsed(),
         result,
      }
   }
}

/// Owned decode result returned to the store thread.
pub struct DecodeCompletion
{
   id: ImageId,
   serial: u64,
   encoded_bytes: u64,
   elapsed: Duration,
   result: Result<DecodedImage, ImageDecodeError>,
}

impl DecodeCompletion
{
   /// Returns the logical image identity carried by this completion.
   pub const fn image_id(&self) -> ImageId
   {
      self.id
   }

   /// Borrows the decode result before store publication.
   pub fn result(&self) -> &Result<DecodedImage, ImageDecodeError>
   {
      &self.result
   }
}

#[derive(Clone, Copy, Debug, PartialEq)]
/// Generation-checked GPU placement ready for draw-command construction.
pub struct ResolvedImage
{
   /// Renderer texture handle.
   pub texture: ImageHandle,
   /// Pixel source rectangle inside the texture.
   pub source: RectF,
   /// Logical image width.
   pub width: u32,
   /// Logical image height.
   pub height: u32,
   /// Normalized source coordinates.
   pub uv: [f32; 4],
   /// Array layer, reserved as zero for current 2D backends.
   pub layer: u16,
   /// Atlas-slot generation, or zero for standalone placement.
   pub slot_generation: u32,
   /// Whether the placement owns a standalone texture.
   pub standalone: bool,
}

/// Minimal renderer boundary for image-store residency and exact invalidation.
pub trait ImageResidencyBackend
{
   /// Returns a nonzero generation unique to the live renderer device.
   fn image_device_generation(&self) -> u64;
   /// Creates and initializes one RGBA8 texture, optionally with complete mips.
   fn image_create_rgba8(&mut self, width: u32, height: u32, data: &[u8], row_bytes: usize, mipmapped: bool) -> ImageHandle;
   /// Creates an uninitialized RGBA8 atlas page without uploading a clear buffer.
   fn image_create_rgba8_empty(&mut self, width: u32, height: u32) -> ImageHandle;
   /// Publishes one previously unsampled RGBA8 subregion without broad dependency invalidation.
   fn image_append_rgba8(&mut self, handle: ImageHandle, x: u32, y: u32, width: u32, height: u32, data: &[u8], row_bytes: usize);
   /// Releases one live texture handle.
   fn image_release(&mut self, handle: ImageHandle);
   /// Invalidates only prepared chunks that referenced retired placements.
   fn image_invalidate_chunks(&mut self, _chunks: &[RenderChunkId]) {}
}

#[derive(Clone, Copy)]
struct AtlasPlacement
{
   page: usize,
   slot: usize,
   generation: u32,
   source: RectF,
}

#[derive(Clone, Copy)]
enum GpuPlacement
{
   Atlas(AtlasPlacement),
   Standalone {
      handle: ImageHandle,
      resident_bytes: u64,
   },
}

#[derive(Clone, Copy, Default)]
struct LruLinks
{
   previous: Option<ImageId>,
   next: Option<ImageId>,
   linked: bool,
}

struct Entry
{
   variant: ImageVariant,
   usage: ImageUsage,
   request_serial: u64,
   requested_at: Instant,
   cancel: Arc<AtomicBool>,
   pending: bool,
   canceled: bool,
   upload_queued: bool,
   published_for_request: bool,
   decoded: Option<DecodedImage>,
   placement: Option<GpuPlacement>,
   error: Option<ImageDecodeError>,
   chunk_refs: Vec<RenderChunkId>,
   last_used: u64,
   decoded_lru: LruLinks,
   gpu_lru: LruLinks,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct RequestKey
{
   variant: ImageVariant,
   usage: ImageUsage,
}

struct EntrySlot
{
   generation: u32,
   entry: Option<Entry>,
}

struct AtlasPage
{
   handle: ImageHandle,
   cell: u32,
   columns: u32,
   owners: Vec<Option<ImageId>>,
   generations: Vec<u32>,
   free: Vec<usize>,
   resident_bytes: u64,
   last_used: u64,
}

impl AtlasPage
{
   fn new(handle: ImageHandle, width: u32, height: u32, cell: u32, resident_bytes: u64, tick: u64) -> Self
   {
      let columns = width / cell;
      let rows = height / cell;
      let count = columns.saturating_mul(rows) as usize;
      Self {
         handle,
         cell,
         columns,
         owners: vec![None; count],
         generations: vec![1; count],
         free: (0..count).rev().collect(),
         resident_bytes,
         last_used: tick,
      }
   }

   fn is_empty(&self) -> bool
   {
      self.free.len() == self.owners.len()
   }
}

/// Host-thread-owned logical image cache and bounded residency manager.
pub struct ImageStore
{
   config: ImageStoreConfig,
   slots: Vec<EntrySlot>,
   free_slots: Vec<u32>,
   variants: HashMap<RequestKey, ImageId>,
   decode_queue: VecDeque<DecodeJob>,
   upload_queue: VecDeque<ImageId>,
   decoded_lru_head: Option<ImageId>,
   decoded_lru_tail: Option<ImageId>,
   gpu_lru_head: Option<ImageId>,
   gpu_lru_tail: Option<ImageId>,
   pages: Vec<Option<AtlasPage>>,
   free_pages: Vec<usize>,
   atlas_patch_scratch: Vec<u8>,
   invalidated_chunks: Vec<RenderChunkId>,
   invalidated_chunk_set: HashSet<RenderChunkId>,
   tick: u64,
   next_request_serial: u64,
   device_generation: u64,
   stats: ImageStoreStats,
}

impl ImageStore
{
   /// Creates an empty store after validating all hard-budget and atlas invariants.
   pub fn new(config: ImageStoreConfig) -> Result<Self, ImageStoreConfigError>
   {
      if config.decoded_budget_bytes == 0
      {
         return Err(ImageStoreConfigError::ZeroDecodedBudget);
      }
      if config.gpu_budget_bytes == 0
      {
         return Err(ImageStoreConfigError::ZeroGpuBudget);
      }
      if config.atlas_width == 0 || config.atlas_height == 0
      {
         return Err(ImageStoreConfigError::ZeroAtlasDimension);
      }
      if config.gutter == 0
      {
         return Err(ImageStoreConfigError::ZeroGutter);
      }
      Ok(Self {
         config,
         slots: Vec::new(),
         free_slots: Vec::new(),
         variants: HashMap::new(),
         decode_queue: VecDeque::new(),
         upload_queue: VecDeque::new(),
         decoded_lru_head: None,
         decoded_lru_tail: None,
         gpu_lru_head: None,
         gpu_lru_tail: None,
         pages: Vec::new(),
         free_pages: Vec::new(),
         atlas_patch_scratch: Vec::new(),
         invalidated_chunks: Vec::new(),
         invalidated_chunk_set: HashSet::new(),
         tick: 0,
         next_request_serial: 0,
         device_generation: 0,
         stats: ImageStoreStats::default(),
      })
   }

   /// Returns the immutable store configuration.
   pub const fn config(&self) -> ImageStoreConfig
   {
      self.config
   }

   /// Returns an exact snapshot of cumulative and current counters.
   pub const fn stats(&self) -> ImageStoreStats
   {
      self.stats
   }

   /// Returns an existing variant identity or queues a new display-size decode.
   pub fn request(&mut self, request: ImageRequest) -> ImageId
   {
      self.stats.requests = self.stats.requests.saturating_add(1);
      if request.variant.display_width == 0 || request.variant.display_height == 0
      {
         return ImageId::INVALID;
      }
      let key = RequestKey { variant: request.variant, usage: request.usage };
      if let Some(id) = self.variants.get(&key).copied()
      {
         self.stats.variant_cache_hits = self.stats.variant_cache_hits.saturating_add(1);
         self.touch(id);
         let restart = self.entry(id).is_some_and(|entry| {
            entry.placement.is_none() && entry.decoded.is_none() && !entry.pending
         });
         if restart
         {
            self.restart_decode(id, request.encoded);
         }
         else
         {
            self.queue_upload(id);
         }
         return id;
      }

      let slot_index = if let Some(index) = self.free_slots.pop()
      {
         index as usize
      }
      else
      {
         self.slots.push(EntrySlot { generation: 1, entry: None });
         self.slots.len() - 1
      };
      let generation = self.slots[slot_index].generation.max(1);
      let id = encode_id(slot_index, generation);
      self.tick = self.tick.wrapping_add(1).max(1);
      let serial = self.next_serial();
      let cancel = Arc::new(AtomicBool::new(false));
      let requested_at = Instant::now();
      self.slots[slot_index].entry = Some(Entry {
         variant: request.variant,
         usage: request.usage,
         request_serial: serial,
         requested_at,
         cancel: cancel.clone(),
         pending: true,
         canceled: false,
         upload_queued: false,
         published_for_request: false,
         decoded: None,
         placement: None,
         error: None,
         chunk_refs: Vec::new(),
         last_used: self.tick,
         decoded_lru: LruLinks::default(),
         gpu_lru: LruLinks::default(),
      });
      self.variants.insert(key, id);
      self.queue_decode_job(id, serial, request.encoded, cancel, requested_at);
      id
   }

   /// Cancels an unpublished identity and requests its replacement.
   pub fn supersede(&mut self, old: ImageId, request: ImageRequest) -> ImageId
   {
      self.cancel(old);
      self.request(request)
   }

   /// Cancels pending or decoded-before-upload work.
   pub fn cancel(&mut self, id: ImageId) -> bool
   {
      let Some(cancellable) = self.entry(id).map(|entry| {
         entry.placement.is_none() && (entry.pending || entry.decoded.is_some())
      }) else
      {
         return false;
      };
      if !cancellable
      {
         return false;
      }
      self.unlink_decoded_lru(id);
      let Some(entry) = self.entry_mut(id) else
      {
         return false;
      };
      entry.cancel.store(true, Ordering::Release);
      entry.pending = false;
      entry.canceled = true;
      entry.upload_queued = false;
      entry.error = Some(ImageDecodeError::Canceled);
      let decoded_bytes = entry.decoded.take().map_or(0, |decoded| decoded.rgba.len() as u64);
      self.stats.decoded_resident_bytes = self.stats.decoded_resident_bytes.saturating_sub(decoded_bytes);
      self.stats.canceled_jobs = self.stats.canceled_jobs.saturating_add(1);
      true
   }

   /// Removes the next noncanceled job for external asynchronous execution.
   pub fn take_decode_job(&mut self) -> Option<DecodeJob>
   {
      while let Some(job) = self.decode_queue.pop_front()
      {
         if !job.is_canceled()
         {
            return Some(job);
         }
      }
      None
   }

   /// Restores an undispatched job at the front of the queue.
   pub fn requeue_decode_job(&mut self, job: DecodeJob)
   {
      if !job.is_canceled()
      {
         self.decode_queue.push_front(job);
      }
   }

   /// Publishes a completion only when identity, serial, and cancellation still match.
   pub fn complete_decode(&mut self, completion: DecodeCompletion) -> bool
   {
      let id = completion.id;
      let Some((request_serial, canceled, display_width, display_height)) = self.entry(id).map(|entry| {
         (
            entry.request_serial,
            entry.canceled || entry.cancel.load(Ordering::Acquire),
            entry.variant.display_width,
            entry.variant.display_height,
         )
      }) else
      {
         self.stats.stale_completions = self.stats.stale_completions.saturating_add(1);
         return false;
      };
      if request_serial != completion.serial || canceled
      {
         self.stats.stale_completions = self.stats.stale_completions.saturating_add(1);
         return false;
      }
      self.stats.decode_completions = self.stats.decode_completions.saturating_add(1);
      self.stats.encoded_input_bytes = self.stats.encoded_input_bytes.saturating_add(completion.encoded_bytes);
      self.stats.decode_time_ns = self.stats.decode_time_ns.saturating_add(duration_ns(completion.elapsed));
      match completion.result
      {
         Ok(decoded) =>
         {
            let expected_bytes = display_width
               .checked_mul(display_height)
               .and_then(|pixels| pixels.checked_mul(RGBA_BYTES_PER_PIXEL as u32))
               .map(|bytes| bytes as usize);
            if decoded.width != display_width
               || decoded.height != display_height
               || expected_bytes != Some(decoded.rgba.len())
            {
               let Some(entry) = self.entry_mut(id) else
               {
                  return false;
               };
               entry.pending = false;
               entry.error = Some(ImageDecodeError::InvalidDimensions);
               self.stats.decode_failures = self.stats.decode_failures.saturating_add(1);
               return false;
            }
            let bytes = decoded.rgba.len() as u64;
            if bytes > self.config.decoded_budget_bytes
            {
               let Some(entry) = self.entry_mut(id) else
               {
                  return false;
               };
               entry.pending = false;
               entry.error = Some(ImageDecodeError::BudgetExceeded);
               self.stats.decode_failures = self.stats.decode_failures.saturating_add(1);
               return false;
            }
            self.make_decoded_room(bytes, id);
            let Some(entry) = self.entry_mut(id) else
            {
               return false;
            };
            entry.pending = false;
            entry.canceled = false;
            entry.error = None;
            entry.decoded = Some(decoded);
            self.stats.decoded_output_bytes = self.stats.decoded_output_bytes.saturating_add(bytes);
            self.stats.decoded_resident_bytes = self.stats.decoded_resident_bytes.saturating_add(bytes);
            self.stats.decoded_peak_bytes = self.stats.decoded_peak_bytes.max(self.stats.decoded_resident_bytes);
            self.touch_decoded_lru(id);
            self.queue_upload(id);
            true
         }
         Err(error) =>
         {
            let Some(entry) = self.entry_mut(id) else
            {
               return false;
            };
            entry.pending = false;
            entry.error = Some(error);
            self.stats.decode_failures = self.stats.decode_failures.saturating_add(1);
            false
         }
      }
   }

   /// Executes at most `limit` queued native PNG jobs synchronously.
   pub fn process_decode_jobs_inline(&mut self, limit: usize) -> usize
   {
      let mut completed = 0;
      while completed < limit
      {
         let Some(job) = self.take_decode_job() else
         {
            break;
         };
         let completion = decode_png_at_display_size(job);
         self.complete_decode(completion);
         completed += 1;
      }
      completed
   }

   /// Publishes every ready queued variant through the renderer backend.
   pub fn upload_ready<B: ImageResidencyBackend>(&mut self, backend: &mut B) -> usize
   {
      self.synchronize_device(backend);
      let mut uploaded = 0;
      let queued = self.upload_queue.len();
      for _ in 0..queued
      {
         let Some(id) = self.upload_queue.pop_front() else
         {
            break;
         };
         let Some(entry) = self.entry_mut(id) else
         {
            continue;
         };
         entry.upload_queued = false;
         if entry.decoded.is_none() || entry.placement.is_some() || entry.canceled
         {
            continue;
         }
         if self.upload_one(id, backend)
         {
            uploaded += 1;
         }
      }
      self.trim_decoded_to_budget(None);
      uploaded
   }

   /// Resolves a live placement without recording a prepared-chunk dependency.
   pub fn resolve(&mut self, id: ImageId) -> Option<ResolvedImage>
   {
      self.resolve_inner(id, None)
   }

   /// Resolves a live placement and records one exact prepared-chunk dependency.
   pub fn resolve_for_chunk(&mut self, id: ImageId, chunk: RenderChunkId) -> Option<ResolvedImage>
   {
      self.resolve_inner(id, Some(chunk))
   }

   /// Returns the current lifecycle state of an identity.
   pub fn status(&self, id: ImageId) -> ImageStatus
   {
      let Some(entry) = self.entry(id) else
      {
         return ImageStatus::Missing;
      };
      if entry.placement.is_some()
      {
         ImageStatus::Resident
      }
      else if entry.canceled
      {
         ImageStatus::Canceled
      }
      else if entry.pending
      {
         ImageStatus::Pending
      }
      else if entry.decoded.is_some()
      {
         ImageStatus::Decoded
      }
      else if entry.error.is_some()
      {
         ImageStatus::Failed
      }
      else
      {
         ImageStatus::Missing
      }
   }

   /// Drains unique chunk invalidations accumulated since the prior drain.
   pub fn drain_invalidated_chunks(&mut self) -> impl Iterator<Item = RenderChunkId> + '_
   {
      self.invalidated_chunk_set.clear();
      self.invalidated_chunks.drain(..)
   }

   /// Releases every decoded/GPU representation and recycles the logical slot.
   pub fn release<B: ImageResidencyBackend>(&mut self, id: ImageId, backend: &mut B) -> bool
   {
      let Some((index, generation)) = decode_id(id) else
      {
         return false;
      };
      let Some(slot) = self.slots.get(index) else
      {
         return false;
      };
      if slot.generation != generation || slot.entry.is_none()
      {
         return false;
      }
      self.evict_gpu(id, backend);
      self.unlink_decoded_lru(id);
      let mut entry = {
         let slot = &mut self.slots[index];
         let Some(entry) = slot.entry.take() else
         {
            return false;
         };
         slot.generation = slot.generation.wrapping_add(1).max(1);
         entry
      };
      entry.cancel.store(true, Ordering::Release);
      if let Some(decoded) = entry.decoded.take()
      {
         self.stats.decoded_resident_bytes = self.stats.decoded_resident_bytes.saturating_sub(decoded.rgba.len() as u64);
      }
      self.variants.remove(&RequestKey { variant: entry.variant, usage: entry.usage });
      self.invalidate_refs(&mut entry.chunk_refs, backend);
      self.free_slots.push(index as u32);
      true
   }

   /// Purges all decoded/GPU cache storage while preserving logical requests.
   pub fn purge_for_memory_warning<B: ImageResidencyBackend>(&mut self, backend: &mut B)
   {
      let ids = self.live_ids();
      for id in ids
      {
         self.evict_gpu(id, backend);
         self.unlink_decoded_lru(id);
         let (decoded_bytes, canceled) = self.entry_mut(id).map_or((0, false), |entry| {
            let canceled = entry.pending;
            if canceled
            {
               entry.cancel.store(true, Ordering::Release);
               entry.pending = false;
               entry.canceled = true;
               entry.error = Some(ImageDecodeError::Canceled);
            }
            entry.upload_queued = false;
            (
               entry.decoded.take().map_or(0, |decoded| decoded.rgba.len() as u64),
               canceled,
            )
         });
         self.stats.decoded_resident_bytes = self.stats.decoded_resident_bytes.saturating_sub(decoded_bytes);
         self.stats.canceled_jobs = self.stats.canceled_jobs.saturating_add(u64::from(canceled));
      }
      self.decode_queue.clear();
      self.upload_queue.clear();
      self.release_empty_pages(backend, u64::MAX);
      self.stats.memory_warning_purges = self.stats.memory_warning_purges.saturating_add(1);
   }

   /// Detects backend replacement, rejects old handles, and queues retained decoded variants.
   pub fn synchronize_device<B: ImageResidencyBackend>(&mut self, backend: &mut B)
   {
      let generation = backend.image_device_generation();
      if self.device_generation == 0
      {
         self.device_generation = generation;
         return;
      }
      if self.device_generation == generation
      {
         return;
      }
      self.device_generation = generation;
      for page in &mut self.pages
      {
         *page = None;
      }
      self.free_pages = (0..self.pages.len()).rev().collect();
      self.stats.gpu_resident_bytes = 0;
      self.stats.atlas_pages = 0;
      self.stats.atlas_slots = 0;
      self.stats.standalone_images = 0;
      self.gpu_lru_head = None;
      self.gpu_lru_tail = None;
      let ids = self.live_ids();
      let mut reupload = Vec::new();
      for id in ids
      {
         let Some(entry) = self.entry_mut(id) else
         {
            continue;
         };
         let had_placement = entry.placement.take().is_some();
         entry.upload_queued = false;
         entry.gpu_lru = LruLinks::default();
         let mut refs = had_placement.then(|| std::mem::take(&mut entry.chunk_refs));
         let should_reupload = entry.decoded.is_some() && !entry.canceled;
         if let Some(refs) = refs.as_mut()
         {
            self.invalidate_refs(refs, backend);
         }
         if should_reupload
         {
            reupload.push(id);
         }
      }
      for id in reupload
      {
         self.queue_upload(id);
      }
      self.stats.device_loss_purges = self.stats.device_loss_purges.saturating_add(1);
   }

   fn queue_decode_job(&mut self, id: ImageId, serial: u64, encoded: Arc<[u8]>, canceled: Arc<AtomicBool>, started: Instant)
   {
      let Some(entry) = self.entry(id) else
      {
         return;
      };
      self.decode_queue.push_back(DecodeJob {
         id,
         serial,
         encoded,
         display_width: entry.variant.display_width,
         display_height: entry.variant.display_height,
         canceled,
         started,
      });
      self.stats.decode_jobs = self.stats.decode_jobs.saturating_add(1);
   }

   fn restart_decode(&mut self, id: ImageId, encoded: Arc<[u8]>)
   {
      let serial = self.next_serial();
      let cancel = Arc::new(AtomicBool::new(false));
      let started = Instant::now();
      let Some(entry) = self.entry_mut(id) else
      {
         return;
      };
      entry.request_serial = serial;
      entry.requested_at = started;
      entry.cancel = cancel.clone();
      entry.pending = true;
      entry.canceled = false;
      entry.upload_queued = false;
      entry.published_for_request = false;
      entry.error = None;
      self.queue_decode_job(id, serial, encoded, cancel, started);
   }

   fn upload_one<B: ImageResidencyBackend>(&mut self, id: ImageId, backend: &mut B) -> bool
   {
      let Some((decoded, usage)) = self.entry_mut(id).and_then(|entry| {
         entry.decoded.take().map(|decoded| (decoded, entry.usage))
      }) else
      {
         return false;
      };
      let placement = if self.atlas_cell(decoded.width, decoded.height, usage).is_some()
      {
         self
            .upload_atlas(id, &decoded, backend)
            .map(GpuPlacement::Atlas)
            .or_else(|| self.upload_standalone(id, &decoded, ImageUsage::Standalone, backend))
      }
      else
      {
         self.upload_standalone(id, &decoded, usage, backend)
      };
      let now = Instant::now();
      let first_publication = {
         let Some(entry) = self.entry_mut(id) else
         {
            return false;
         };
         entry.decoded = Some(decoded);
         let Some(placement) = placement else
         {
            return false;
         };
         entry.placement = Some(placement);
         if entry.published_for_request
         {
            None
         }
         else
         {
            entry.published_for_request = true;
            Some(entry.requested_at)
         }
      };
      self.touch_gpu_lru(id);
      if let Some(requested_at) = first_publication
      {
         self.stats.first_publication_count = self.stats.first_publication_count.saturating_add(1);
         self.stats.request_to_first_publication_ns = self
            .stats
            .request_to_first_publication_ns
            .saturating_add(duration_ns(now.saturating_duration_since(requested_at)));
      }
      true
   }

   fn upload_atlas<B: ImageResidencyBackend>(&mut self, id: ImageId, decoded: &DecodedImage, backend: &mut B) -> Option<AtlasPlacement>
   {
      let cell = self.atlas_cell(decoded.width, decoded.height, ImageUsage::Static)?;
      let page_index = self.page_with_space(cell).or_else(|| self.create_page(cell, id, backend))?;
      let gutter = self.config.gutter;
      write_atlas_patch(&mut self.atlas_patch_scratch, decoded, cell, gutter);
      let last_used = self.entry(id)?.last_used;
      let page = self.pages.get_mut(page_index)?.as_mut()?;
      let slot = page.free.pop()?;
      page.owners[slot] = Some(id);
      page.last_used = page.last_used.max(last_used);
      let x = (slot as u32 % page.columns).saturating_mul(cell);
      let y = (slot as u32 / page.columns).saturating_mul(cell);
      backend.image_append_rgba8(page.handle, x, y, cell, cell, &self.atlas_patch_scratch, cell as usize * 4);
      self.stats.upload_bytes = self.stats.upload_bytes.saturating_add(self.atlas_patch_scratch.len() as u64);
      self.stats.atlas_slots = self.stats.atlas_slots.saturating_add(1);
      Some(AtlasPlacement {
         page: page_index,
         slot,
         generation: page.generations[slot],
         source: RectF::new(
            (x + gutter) as f32,
            (y + gutter) as f32,
            decoded.width as f32,
            decoded.height as f32,
         ),
      })
   }

   fn upload_standalone<B: ImageResidencyBackend>(&mut self, id: ImageId, decoded: &DecodedImage, usage: ImageUsage, backend: &mut B) -> Option<GpuPlacement>
   {
      let resident_bytes = image_resident_bytes(decoded.width, decoded.height, usage.needs_mips());
      if resident_bytes > self.config.gpu_budget_bytes
      {
         return None;
      }
      self.make_gpu_room(resident_bytes, id, backend);
      if self.stats.gpu_resident_bytes.saturating_add(resident_bytes) > self.config.gpu_budget_bytes
      {
         return None;
      }
      let handle = backend.image_create_rgba8(
         decoded.width,
         decoded.height,
         &decoded.rgba,
         decoded.width as usize * 4,
         usage.needs_mips(),
      );
      if handle.0 == 0
      {
         return None;
      }
      self.stats.gpu_resident_bytes = self.stats.gpu_resident_bytes.saturating_add(resident_bytes);
      self.stats.gpu_peak_bytes = self.stats.gpu_peak_bytes.max(self.stats.gpu_resident_bytes);
      self.stats.upload_bytes = self.stats.upload_bytes.saturating_add(resident_bytes);
      self.stats.texture_creates = self.stats.texture_creates.saturating_add(1);
      self.stats.standalone_images = self.stats.standalone_images.saturating_add(1);
      Some(GpuPlacement::Standalone { handle, resident_bytes })
   }

   fn create_page<B: ImageResidencyBackend>(&mut self, cell: u32, exclude: ImageId, backend: &mut B) -> Option<usize>
   {
      let bytes = image_resident_bytes(self.config.atlas_width, self.config.atlas_height, false);
      if bytes > self.config.gpu_budget_bytes
      {
         return None;
      }
      if self.stats.gpu_resident_bytes.saturating_add(bytes) > self.config.gpu_budget_bytes
      {
         if let Some(index) = self.recycle_atlas_slot(cell, exclude, backend)
         {
            return Some(index);
         }
      }
      self.make_gpu_room(bytes, exclude, backend);
      if self.stats.gpu_resident_bytes.saturating_add(bytes) > self.config.gpu_budget_bytes
      {
         return None;
      }
      let handle = backend.image_create_rgba8_empty(self.config.atlas_width, self.config.atlas_height);
      if handle.0 == 0
      {
         return None;
      }
      let page = AtlasPage::new(
         handle,
         self.config.atlas_width,
         self.config.atlas_height,
         cell,
         bytes,
         self.tick,
      );
      let index = if let Some(index) = self.free_pages.pop()
      {
         self.pages[index] = Some(page);
         index
      }
      else
      {
         self.pages.push(Some(page));
         self.pages.len() - 1
      };
      self.stats.gpu_resident_bytes = self.stats.gpu_resident_bytes.saturating_add(bytes);
      self.stats.gpu_peak_bytes = self.stats.gpu_peak_bytes.max(self.stats.gpu_resident_bytes);
      self.stats.texture_creates = self.stats.texture_creates.saturating_add(1);
      self.stats.atlas_pages = self.stats.atlas_pages.saturating_add(1);
      Some(index)
   }

   fn make_gpu_room<B: ImageResidencyBackend>(&mut self, needed: u64, exclude: ImageId, backend: &mut B)
   {
      self.release_empty_pages(backend, needed);
      while self.stats.gpu_resident_bytes.saturating_add(needed) > self.config.gpu_budget_bytes
      {
         let victim = self.gpu_victim(exclude);
         let Some(victim) = victim else
         {
            break;
         };
         let atlas_page = self.entry(victim).and_then(|entry| match entry.placement {
            Some(GpuPlacement::Atlas(placement)) => Some(placement.page),
            _ => None,
         });
         if let Some(page) = atlas_page
         {
            self.evict_atlas_page(page, exclude, backend);
         }
         else
         {
            self.evict_gpu(victim, backend);
         }
         self.release_empty_pages(backend, needed);
      }
   }

   fn recycle_atlas_slot<B: ImageResidencyBackend>(&mut self, cell: u32, exclude: ImageId, backend: &mut B) -> Option<usize>
   {
      let mut candidate = self.gpu_lru_head;
      while let Some(id) = candidate
      {
         let entry = self.entry(id)?;
         let next = entry.gpu_lru.next;
         if id != exclude
         {
            if let Some(GpuPlacement::Atlas(placement)) = entry.placement
            {
               if self
                  .pages
                  .get(placement.page)
                  .and_then(Option::as_ref)
                  .is_some_and(|page| page.cell == cell)
               {
                  self.evict_gpu(id, backend);
                  return Some(placement.page);
               }
            }
         }
         candidate = next;
      }
      None
   }

   fn evict_atlas_page<B: ImageResidencyBackend>(&mut self, page_index: usize, exclude: ImageId, backend: &mut B)
   {
      let slots = self
         .pages
         .get(page_index)
         .and_then(Option::as_ref)
         .map_or(0, |page| page.owners.len());
      for slot in 0..slots
      {
         let owner = self
            .pages
            .get(page_index)
            .and_then(Option::as_ref)
            .and_then(|page| page.owners.get(slot))
            .copied()
            .flatten();
         if let Some(owner) = owner.filter(|owner| *owner != exclude)
         {
            self.evict_gpu(owner, backend);
         }
      }
   }

   fn evict_gpu<B: ImageResidencyBackend>(&mut self, id: ImageId, backend: &mut B)
   {
      self.unlink_gpu_lru(id);
      let Some(placement) = self.entry_mut(id).and_then(|entry| entry.placement.take()) else
      {
         return;
      };
      match placement
      {
         GpuPlacement::Standalone { handle, resident_bytes } =>
         {
            backend.image_release(handle);
            self.stats.gpu_resident_bytes = self.stats.gpu_resident_bytes.saturating_sub(resident_bytes);
            self.stats.texture_releases = self.stats.texture_releases.saturating_add(1);
            self.stats.standalone_images = self.stats.standalone_images.saturating_sub(1);
         }
         GpuPlacement::Atlas(placement) =>
         {
            if let Some(page) = self.pages.get_mut(placement.page).and_then(Option::as_mut)
            {
               if page.owners.get(placement.slot).copied().flatten() == Some(id)
                  && page.generations[placement.slot] == placement.generation
               {
                  page.owners[placement.slot] = None;
                  page.generations[placement.slot] = page.generations[placement.slot].wrapping_add(1).max(1);
                  page.free.push(placement.slot);
                  self.stats.atlas_slots = self.stats.atlas_slots.saturating_sub(1);
               }
            }
         }
      }
      if let Some(entry) = self.entry_mut(id)
      {
         entry.upload_queued = false;
         let mut refs = std::mem::take(&mut entry.chunk_refs);
         self.invalidate_refs(&mut refs, backend);
      }
      self.stats.gpu_evictions = self.stats.gpu_evictions.saturating_add(1);
   }

   fn release_empty_pages<B: ImageResidencyBackend>(&mut self, backend: &mut B, needed: u64)
   {
      loop
      {
         if needed != u64::MAX
            && self.stats.gpu_resident_bytes.saturating_add(needed) <= self.config.gpu_budget_bytes
         {
            break;
         }
         let Some(index) = self
            .pages
            .iter()
            .enumerate()
            .filter_map(|(index, page)| page.as_ref().filter(|page| page.is_empty()).map(|page| (index, page.last_used)))
            .min_by_key(|(_, last_used)| *last_used)
            .map(|(index, _)| index) else
         {
            break;
         };
         let Some(page) = self.pages[index].take() else
         {
            break;
         };
         backend.image_release(page.handle);
         self.stats.gpu_resident_bytes = self.stats.gpu_resident_bytes.saturating_sub(page.resident_bytes);
         self.stats.texture_releases = self.stats.texture_releases.saturating_add(1);
         self.stats.atlas_pages = self.stats.atlas_pages.saturating_sub(1);
         self.free_pages.push(index);
      }
   }

   fn resolve_inner(&mut self, id: ImageId, chunk: Option<RenderChunkId>) -> Option<ResolvedImage>
   {
      self.tick = self.tick.wrapping_add(1).max(1);
      let tick = self.tick;
      let mut needs_upload = false;
      {
         let entry = self.entry_mut(id)?;
         entry.last_used = tick;
         if entry.placement.is_none()
         {
            needs_upload = entry.decoded.is_some() && !entry.canceled;
         }
      }
      self.touch_decoded_lru(id);
      if needs_upload
      {
         self.queue_upload(id);
         return None;
      }
      if let Some(chunk) = chunk
      {
         let entry = self.entry_mut(id)?;
         if !entry.chunk_refs.contains(&chunk)
         {
            entry.chunk_refs.push(chunk);
         }
      }
      self.touch_gpu_lru(id);
      let entry = self.entry(id)?;
      let placement = entry.placement?;
      let width = entry.variant.display_width;
      let height = entry.variant.display_height;
      match placement
      {
         GpuPlacement::Standalone { handle, .. } => Some(ResolvedImage {
            texture: handle,
            source: RectF::new(0.0, 0.0, width as f32, height as f32),
            width,
            height,
            uv: [0.0, 0.0, 1.0, 1.0],
            layer: 0,
            slot_generation: 0,
            standalone: true,
         }),
         GpuPlacement::Atlas(placement) =>
         {
            let page = self.pages.get_mut(placement.page)?.as_mut()?;
            if page.owners.get(placement.slot).copied().flatten() != Some(id)
               || page.generations.get(placement.slot).copied() != Some(placement.generation)
            {
               return None;
            }
            page.last_used = tick;
            Some(ResolvedImage {
               texture: page.handle,
               source: placement.source,
               width,
               height,
               uv: [
                  placement.source.x / self.config.atlas_width as f32,
                  placement.source.y / self.config.atlas_height as f32,
                  (placement.source.x + placement.source.w) / self.config.atlas_width as f32,
                  (placement.source.y + placement.source.h) / self.config.atlas_height as f32,
               ],
               layer: 0,
               slot_generation: placement.generation,
               standalone: false,
            })
         }
      }
   }

   fn atlas_cell(&self, width: u32, height: u32, usage: ImageUsage) -> Option<u32>
   {
      if !usage.atlas_eligible()
         || width > self.config.max_atlas_image_dimension
         || height > self.config.max_atlas_image_dimension
      {
         return None;
      }
      let outer = width.max(height).checked_add(self.config.gutter.checked_mul(2)?)?;
      let cell = outer.checked_add(3)? & !3;
      (cell <= self.config.atlas_width && cell <= self.config.atlas_height).then_some(cell)
   }

   fn page_with_space(&self, cell: u32) -> Option<usize>
   {
      self.pages.iter().position(|page| page.as_ref().is_some_and(|page| page.cell == cell && !page.free.is_empty()))
   }

   fn make_decoded_room(&mut self, needed: u64, exclude: ImageId)
   {
      while self.stats.decoded_resident_bytes.saturating_add(needed) > self.config.decoded_budget_bytes
      {
         let victim = self.decoded_victim(Some(exclude));
         let Some(victim) = victim else
         {
            break;
         };
         self.evict_decoded(victim);
      }
   }

   fn trim_decoded_to_budget(&mut self, exclude: Option<ImageId>)
   {
      while self.stats.decoded_resident_bytes > self.config.decoded_budget_bytes
      {
         let victim = self.decoded_victim(exclude);
         let Some(victim) = victim else
         {
            break;
         };
         self.evict_decoded(victim);
      }
   }

   fn evict_decoded(&mut self, id: ImageId)
   {
      self.unlink_decoded_lru(id);
      let bytes = self.entry_mut(id).map_or(0, |entry| {
         entry.upload_queued = false;
         entry.decoded.take().map_or(0, |decoded| decoded.rgba.len() as u64)
      });
      if bytes > 0
      {
         self.stats.decoded_resident_bytes = self.stats.decoded_resident_bytes.saturating_sub(bytes);
         self.stats.decoded_evictions = self.stats.decoded_evictions.saturating_add(1);
      }
   }

   fn touch(&mut self, id: ImageId)
   {
      self.tick = self.tick.wrapping_add(1).max(1);
      let tick = self.tick;
      if let Some(entry) = self.entry_mut(id)
      {
         entry.last_used = tick;
      }
      let resources = self.entry(id).map(|entry| (entry.decoded.is_some(), entry.placement.is_some()));
      if let Some((decoded, placed)) = resources
      {
         if decoded
         {
            self.touch_decoded_lru(id);
         }
         if placed
         {
            self.touch_gpu_lru(id);
         }
      }
   }

   fn gpu_victim(&self, exclude: ImageId) -> Option<ImageId>
   {
      let mut candidate = self.gpu_lru_head;
      while let Some(id) = candidate
      {
         let entry = self.entry(id)?;
         if id != exclude && entry.placement.is_some()
         {
            return Some(id);
         }
         candidate = entry.gpu_lru.next;
      }
      None
   }

   fn decoded_victim(&self, exclude: Option<ImageId>) -> Option<ImageId>
   {
      let mut candidate = self.decoded_lru_head;
      while let Some(id) = candidate
      {
         let entry = self.entry(id)?;
         if Some(id) != exclude && entry.decoded.is_some()
         {
            return Some(id);
         }
         candidate = entry.decoded_lru.next;
      }
      None
   }

   fn touch_decoded_lru(&mut self, id: ImageId)
   {
      if !self.entry(id).is_some_and(|entry| entry.decoded.is_some())
      {
         return;
      }
      self.unlink_decoded_lru(id);
      let previous = self.decoded_lru_tail;
      let Some(entry) = self.entry_mut(id) else
      {
         return;
      };
      entry.decoded_lru = LruLinks { previous, next: None, linked: true };
      if let Some(previous) = previous
      {
         if let Some(entry) = self.entry_mut(previous)
         {
            entry.decoded_lru.next = Some(id);
         }
      }
      else
      {
         self.decoded_lru_head = Some(id);
      }
      self.decoded_lru_tail = Some(id);
   }

   fn unlink_decoded_lru(&mut self, id: ImageId)
   {
      let Some(links) = self.entry(id).map(|entry| entry.decoded_lru) else
      {
         return;
      };
      if !links.linked
      {
         return;
      }
      if let Some(previous) = links.previous
      {
         if let Some(entry) = self.entry_mut(previous)
         {
            entry.decoded_lru.next = links.next;
         }
      }
      else
      {
         self.decoded_lru_head = links.next;
      }
      if let Some(next) = links.next
      {
         if let Some(entry) = self.entry_mut(next)
         {
            entry.decoded_lru.previous = links.previous;
         }
      }
      else
      {
         self.decoded_lru_tail = links.previous;
      }
      if let Some(entry) = self.entry_mut(id)
      {
         entry.decoded_lru = LruLinks::default();
      }
   }

   fn touch_gpu_lru(&mut self, id: ImageId)
   {
      if !self.entry(id).is_some_and(|entry| entry.placement.is_some())
      {
         return;
      }
      self.unlink_gpu_lru(id);
      let previous = self.gpu_lru_tail;
      let Some(entry) = self.entry_mut(id) else
      {
         return;
      };
      entry.gpu_lru = LruLinks { previous, next: None, linked: true };
      if let Some(previous) = previous
      {
         if let Some(entry) = self.entry_mut(previous)
         {
            entry.gpu_lru.next = Some(id);
         }
      }
      else
      {
         self.gpu_lru_head = Some(id);
      }
      self.gpu_lru_tail = Some(id);
   }

   fn unlink_gpu_lru(&mut self, id: ImageId)
   {
      let Some(links) = self.entry(id).map(|entry| entry.gpu_lru) else
      {
         return;
      };
      if !links.linked
      {
         return;
      }
      if let Some(previous) = links.previous
      {
         if let Some(entry) = self.entry_mut(previous)
         {
            entry.gpu_lru.next = links.next;
         }
      }
      else
      {
         self.gpu_lru_head = links.next;
      }
      if let Some(next) = links.next
      {
         if let Some(entry) = self.entry_mut(next)
         {
            entry.gpu_lru.previous = links.previous;
         }
      }
      else
      {
         self.gpu_lru_tail = links.previous;
      }
      if let Some(entry) = self.entry_mut(id)
      {
         entry.gpu_lru = LruLinks::default();
      }
   }

   fn queue_upload(&mut self, id: ImageId)
   {
      let Some(entry) = self.entry_mut(id) else
      {
         return;
      };
      if entry.upload_queued || entry.decoded.is_none() || entry.placement.is_some() || entry.canceled
      {
         return;
      }
      entry.upload_queued = true;
      self.upload_queue.push_back(id);
   }

   fn invalidate_refs<B: ImageResidencyBackend>(&mut self, refs: &mut Vec<RenderChunkId>, backend: &mut B)
   {
      let first_new = self.invalidated_chunks.len();
      for chunk in refs.drain(..)
      {
         if self.invalidated_chunk_set.insert(chunk)
         {
            self.invalidated_chunks.push(chunk);
            self.stats.invalidated_chunks = self.stats.invalidated_chunks.saturating_add(1);
         }
      }
      backend.image_invalidate_chunks(&self.invalidated_chunks[first_new..]);
   }

   fn live_ids(&self) -> Vec<ImageId>
   {
      self.slots
         .iter()
         .enumerate()
         .filter_map(|(index, slot)| slot.entry.as_ref().map(|_| encode_id(index, slot.generation)))
         .collect()
   }

   fn next_serial(&mut self) -> u64
   {
      self.next_request_serial = self.next_request_serial.wrapping_add(1).max(1);
      self.next_request_serial
   }

   fn entry(&self, id: ImageId) -> Option<&Entry>
   {
      let (index, generation) = decode_id(id)?;
      self.slots.get(index).filter(|slot| slot.generation == generation)?.entry.as_ref()
   }

   fn entry_mut(&mut self, id: ImageId) -> Option<&mut Entry>
   {
      let (index, generation) = decode_id(id)?;
      self.slots.get_mut(index).filter(|slot| slot.generation == generation)?.entry.as_mut()
   }
}

#[cfg(not(target_arch = "wasm32"))]
/// Native worker pool for off-request-thread PNG decode.
pub struct NativeDecodePool
{
   sender: Option<std::sync::mpsc::Sender<DecodeJob>>,
   receiver: std::sync::mpsc::Receiver<DecodeCompletion>,
   workers: Vec<std::thread::JoinHandle<()>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl NativeDecodePool
{
   /// Starts at least one decode worker.
   pub fn new(worker_count: usize) -> Self
   {
      let (job_sender, job_receiver) = std::sync::mpsc::channel::<DecodeJob>();
      let (completion_sender, completion_receiver) = std::sync::mpsc::channel();
      let shared_receiver = Arc::new(std::sync::Mutex::new(job_receiver));
      let mut workers = Vec::with_capacity(worker_count.max(1));
      for _ in 0..worker_count.max(1)
      {
         let jobs = shared_receiver.clone();
         let completions = completion_sender.clone();
         workers.push(std::thread::spawn(move || loop {
            let job = match jobs.lock()
            {
               Ok(receiver) => receiver.recv(),
               Err(_) => break,
            };
            let Ok(job) = job else
            {
               break;
            };
            if completions.send(decode_png_at_display_size(job)).is_err()
            {
               break;
            }
         }));
      }
      drop(completion_sender);
      Self {
         sender: Some(job_sender),
         receiver: completion_receiver,
         workers,
      }
   }

   /// Transfers at most `limit` queued jobs from the store into the worker pool.
   pub fn dispatch(&self, store: &mut ImageStore, limit: usize) -> usize
   {
      let mut dispatched = 0;
      while dispatched < limit
      {
         let Some(job) = store.take_decode_job() else
         {
            break;
         };
         let Some(sender) = self.sender.as_ref() else
         {
            store.requeue_decode_job(job);
            break;
         };
         if let Err(error) = sender.send(job)
         {
            store.requeue_decode_job(error.0);
            break;
         }
         dispatched += 1;
      }
      dispatched
   }

   /// Publishes every currently available worker completion into the store.
   pub fn collect(&self, store: &mut ImageStore) -> usize
   {
      let mut completed = 0;
      while let Ok(completion) = self.receiver.try_recv()
      {
         store.complete_decode(completion);
         completed += 1;
      }
      completed
   }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for NativeDecodePool
{
   fn drop(&mut self)
   {
      self.sender.take();
      for worker in self.workers.drain(..)
      {
         let _ = worker.join();
      }
   }
}

/// Decodes one PNG job and resizes it to its requested display dimensions.
pub fn decode_png_at_display_size(mut job: DecodeJob) -> DecodeCompletion
{
   job.mark_started();
   let result = if job.is_canceled()
   {
      Err(ImageDecodeError::Canceled)
   }
   else
   {
      decode_png(&job.encoded, job.display_width, job.display_height, &job.canceled)
   };
   job.complete(result)
}

#[cfg(target_arch = "wasm32")]
/// Decodes one browser image through `createImageBitmap` at display size.
pub async fn decode_image_at_display_size_browser(mut job: DecodeJob) -> DecodeCompletion
{
   use js_sys::{Array, Uint8Array};
   use wasm_bindgen::JsCast;
   use wasm_bindgen_futures::JsFuture;
   use web_sys::{
      Blob, CanvasRenderingContext2d, HtmlCanvasElement, ImageBitmap, ImageBitmapOptions,
      ResizeQuality,
   };

   job.mark_started();
   let result = async {
      if job.is_canceled()
      {
         return Err(ImageDecodeError::Canceled);
      }
      let window = web_sys::window().ok_or_else(|| browser_decode_error("window unavailable"))?;
      let document = window.document().ok_or_else(|| browser_decode_error("document unavailable"))?;
      let encoded = Uint8Array::from(job.encoded());
      let parts = Array::new();
      parts.push(&encoded);
      let blob = Blob::new_with_u8_array_sequence(&parts)
         .map_err(|error| browser_js_decode_error("creating image blob", error))?;
      let options = ImageBitmapOptions::new();
      options.set_resize_width(job.display_width);
      options.set_resize_height(job.display_height);
      options.set_resize_quality(ResizeQuality::High);
      let promise = window
         .create_image_bitmap_with_blob_and_image_bitmap_options(&blob, &options)
         .map_err(|error| browser_js_decode_error("starting image bitmap decode", error))?;
      let bitmap = JsFuture::from(promise)
         .await
         .map_err(|error| browser_js_decode_error("awaiting image bitmap decode", error))?
         .dyn_into::<ImageBitmap>()
         .map_err(|error| browser_js_decode_error("reading decoded image bitmap", error))?;
      if job.is_canceled()
      {
         bitmap.close();
         return Err(ImageDecodeError::Canceled);
      }
      let canvas = document
         .create_element("canvas")
         .map_err(|error| browser_js_decode_error("creating decode canvas", error))?
         .dyn_into::<HtmlCanvasElement>()
         .map_err(|error| browser_js_decode_error("reading decode canvas", error.into()))?;
      canvas.set_width(job.display_width);
      canvas.set_height(job.display_height);
      let context = canvas
         .get_context("2d")
         .map_err(|error| browser_js_decode_error("creating decode context", error))?
         .ok_or_else(|| browser_decode_error("2D decode context unavailable"))?
         .dyn_into::<CanvasRenderingContext2d>()
         .map_err(|error| browser_js_decode_error("reading decode context", error.into()))?;
      context
         .draw_image_with_image_bitmap(&bitmap, 0.0, 0.0)
         .map_err(|error| browser_js_decode_error("drawing decoded image", error))?;
      bitmap.close();
      let image = context
         .get_image_data(0.0, 0.0, f64::from(job.display_width), f64::from(job.display_height))
         .map_err(|error| browser_js_decode_error("reading decoded RGBA pixels", error))?;
      Ok(DecodedImage {
         width: job.display_width,
         height: job.display_height,
         rgba: image.data().0,
      })
   }
   .await;
   job.complete(result)
}

#[cfg(target_arch = "wasm32")]
fn browser_decode_error(message: &str) -> ImageDecodeError
{
   ImageDecodeError::Decode(String::from(message))
}

#[cfg(target_arch = "wasm32")]
fn browser_js_decode_error(message: &str, error: wasm_bindgen::JsValue) -> ImageDecodeError
{
   ImageDecodeError::Decode(format!("{message}: {error:?}"))
}

fn decode_png(encoded: &[u8], target_width: u32, target_height: u32, canceled: &AtomicBool) -> Result<DecodedImage, ImageDecodeError>
{
   if target_width == 0 || target_height == 0
   {
      return Err(ImageDecodeError::InvalidDimensions);
   }
   let mut decoder = png::Decoder::new(Cursor::new(encoded));
   decoder.set_transformations(png::Transformations::normalize_to_color8());
   let mut reader = decoder.read_info().map_err(|error| ImageDecodeError::Decode(error.to_string()))?;
   let mut bytes = vec![0; reader.output_buffer_size()];
   let info = reader.next_frame(&mut bytes).map_err(|error| ImageDecodeError::Decode(error.to_string()))?;
   if canceled.load(Ordering::Acquire)
   {
      return Err(ImageDecodeError::Canceled);
   }
   bytes.truncate(info.buffer_size());
   let rgba = normalize_rgba(&bytes, info.color_type)?;
   let rgba = if info.width == target_width && info.height == target_height
   {
      rgba
   }
   else
   {
      resize_rgba_box(&rgba, info.width, info.height, target_width, target_height, canceled)?
   };
   Ok(DecodedImage { width: target_width, height: target_height, rgba })
}

fn normalize_rgba(bytes: &[u8], color: png::ColorType) -> Result<Vec<u8>, ImageDecodeError>
{
   match color
   {
      png::ColorType::Rgba => Ok(bytes.to_vec()),
      png::ColorType::Rgb =>
      {
         let mut rgba = Vec::with_capacity(bytes.len() / 3 * 4);
         for rgb in bytes.chunks_exact(3)
         {
            rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
         }
         Ok(rgba)
      }
      png::ColorType::Grayscale =>
      {
         let mut rgba = Vec::with_capacity(bytes.len() * 4);
         for gray in bytes
         {
            rgba.extend_from_slice(&[*gray, *gray, *gray, 255]);
         }
         Ok(rgba)
      }
      png::ColorType::GrayscaleAlpha =>
      {
         let mut rgba = Vec::with_capacity(bytes.len() / 2 * 4);
         for pixel in bytes.chunks_exact(2)
         {
            rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]);
         }
         Ok(rgba)
      }
      png::ColorType::Indexed => Err(ImageDecodeError::UnsupportedFormat),
   }
}

fn resize_rgba_box(source: &[u8], source_width: u32, source_height: u32, target_width: u32, target_height: u32, canceled: &AtomicBool) -> Result<Vec<u8>, ImageDecodeError>
{
   let target_len = target_width
      .checked_mul(target_height)
      .and_then(|pixels| pixels.checked_mul(4))
      .ok_or(ImageDecodeError::InvalidDimensions)? as usize;
   let mut target = vec![0; target_len];
   for y in 0..target_height
   {
      if canceled.load(Ordering::Acquire)
      {
         return Err(ImageDecodeError::Canceled);
      }
      let source_y0 = (u64::from(y) * u64::from(source_height) / u64::from(target_height)) as u32;
      let source_y1 = (((u64::from(y + 1) * u64::from(source_height) + u64::from(target_height) - 1) / u64::from(target_height)) as u32).max(source_y0 + 1).min(source_height);
      for x in 0..target_width
      {
         let source_x0 = (u64::from(x) * u64::from(source_width) / u64::from(target_width)) as u32;
         let source_x1 = (((u64::from(x + 1) * u64::from(source_width) + u64::from(target_width) - 1) / u64::from(target_width)) as u32).max(source_x0 + 1).min(source_width);
         let mut alpha = 0_u64;
         let mut red_alpha = 0_u64;
         let mut green_alpha = 0_u64;
         let mut blue_alpha = 0_u64;
         let mut samples = 0_u64;
         for source_y in source_y0..source_y1
         {
            for source_x in source_x0..source_x1
            {
               let index = (u64::from(source_y) * u64::from(source_width) + u64::from(source_x)) as usize * 4;
               let a = u64::from(source[index + 3]);
               alpha += a;
               red_alpha += u64::from(source[index]) * a;
               green_alpha += u64::from(source[index + 1]) * a;
               blue_alpha += u64::from(source[index + 2]) * a;
               samples += 1;
            }
         }
         let target_index = (u64::from(y) * u64::from(target_width) + u64::from(x)) as usize * 4;
         if alpha > 0
         {
            target[target_index] = ((red_alpha + alpha / 2) / alpha) as u8;
            target[target_index + 1] = ((green_alpha + alpha / 2) / alpha) as u8;
            target[target_index + 2] = ((blue_alpha + alpha / 2) / alpha) as u8;
         }
         target[target_index + 3] = ((alpha + samples / 2) / samples) as u8;
      }
   }
   Ok(target)
}

fn write_atlas_patch(patch: &mut Vec<u8>, image: &DecodedImage, cell: u32, gutter: u32)
{
   patch.resize(cell as usize * cell as usize * RGBA_BYTES_PER_PIXEL as usize, 0);
   for y in 0..cell
   {
      let source_y = y.saturating_sub(gutter).min(image.height - 1);
      for x in 0..cell
      {
         let source_x = x.saturating_sub(gutter).min(image.width - 1);
         let source_index = (source_y as usize * image.width as usize + source_x as usize) * 4;
         let target_index = (y as usize * cell as usize + x as usize) * 4;
         patch[target_index..target_index + 4].copy_from_slice(&image.rgba[source_index..source_index + 4]);
      }
   }
}

fn image_resident_bytes(width: u32, height: u32, mipmapped: bool) -> u64
{
   let mut width = u64::from(width);
   let mut height = u64::from(height);
   let mut bytes = 0_u64;
   loop
   {
      bytes = bytes.saturating_add(width.saturating_mul(height).saturating_mul(RGBA_BYTES_PER_PIXEL));
      if !mipmapped || (width == 1 && height == 1)
      {
         return bytes;
      }
      width = (width / 2).max(1);
      height = (height / 2).max(1);
   }
}

fn duration_ns(duration: Duration) -> u64
{
   duration.as_nanos().min(u128::from(u64::MAX)) as u64
}

fn encode_id(index: usize, generation: u32) -> ImageId
{
   ImageId((u64::from(generation) << 32) | (index as u64 + 1))
}

fn decode_id(id: ImageId) -> Option<(usize, u32)>
{
   let slot = id.0 as u32;
   let generation = (id.0 >> 32) as u32;
   (slot != 0 && generation != 0).then_some(((slot - 1) as usize, generation))
}
