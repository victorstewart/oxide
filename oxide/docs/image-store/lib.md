# oxide-image-store::lib

## Intention and purpose

`oxide-image-store` owns generation-safe logical image variants between encoded assets and renderer texture handles. It decodes at requested display size, publishes only current completions, bounds decoded and GPU residency, pages suitable static images, and preserves unsuitable images as standalone textures.

## Relation to the rest of the code

- App and host code submits `ImageRequest`s and retains `ImageId`s rather than renderer handles.
- Native hosts may use `NativeDecodePool`; the browser host uses `decode_image_at_display_size_browser`.
- Metal and WebGPU implement `ImageResidencyBackend` for texture creation, append-only atlas publication, release, device generation, and exact prepared-chunk invalidation.
- `ui-core::ImageRegionView` consumes `ResolvedImage::source` without allowing contain/cover cropping to escape the owning atlas slot.

## Entry points list

- `ImageId`: opaque slot-plus-generation identity, with `INVALID`, `raw`, and `is_valid` helpers.
- `ImageVariant`: source/revision/display-size cache key.
- `ImageUsage`: selects static-atlas or standalone/mip policy.
- `ImageRequest`: one encoded source, variant, and usage request.
- `ImageStoreConfig`: decoded/GPU budgets and atlas geometry; `Default` uses 64 MiB decoded, 128 MiB GPU, and 512-square pages.
- `ImageStoreConfigError`: typed zero-budget, zero-dimension, and zero-gutter validation failures returned by `ImageStore::new`.
- `ImageStoreStats`: cumulative work plus exact current/peak residency and lifecycle counters.
- `ImageStatus`, `ImageDecodeError`, and `DecodedImage`: public state and decode result vocabulary.
- `DecodeJob` and `DecodeCompletion`: transferable decode ownership with cancellation and elapsed-time accounting.
- `ResolvedImage`: generation-checked texture, source rectangle, UV/layer, slot generation, and standalone marker.
- `ImageResidencyBackend`: portable GPU publication and invalidation boundary.
- `ImageStore`: request, supersede, cancel, decode dispatch/completion, upload, resolve, release, purge, device synchronization, stats, and invalidation drainage.
- `NativeDecodePool`: bounded native worker pool with nonblocking dispatch/collection.
- `decode_png_at_display_size`: native deterministic PNG decode and alpha-aware box resize.
- `decode_image_at_display_size_browser`: wasm `createImageBitmap` display-size decode and RGBA readback.

## Logic narrative

A request is deduplicated by `(ImageVariant, ImageUsage)`, assigned a generation-checked slot, and queued with a monotonically increasing request serial. Decode completion must match the live slot, generation, serial, and cancellation state before decoded bytes become visible. Uploads are queued separately so renderer work remains explicit, and request-to-first-publication latency is recorded only for the first residency publication of a request rather than device-loss or pressure reuploads. Display completion remains the host or benchmark's responsibility because the store does not own presentation.

Eligible small static images enter a size-classed page. Each cell includes repeated-edge gutters, has its own generation, and is populated through a subregion append without rewriting the page. Repeatedly minified, rapidly changing, video, incompatible, oversized, and explicitly standalone variants receive independent textures; only repeatedly minified textures request mip chains. Resolution records exact `RenderChunkId` references so eviction invalidates only work that sampled the retired placement.

Decoded and GPU objects have separate intrusive LRUs. Enforcement evicts the least-recently-used eligible entries until each hard budget is met. Empty atlas pages can be released, memory warnings purge decoded and GPU caches, and a backend generation change removes all stale handles, invalidates their referencing chunks, and queues decoded variants for reupload.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

- Display dimensions, atlas dimensions, and byte-size products must be nonzero and representable; invalid inputs fail without publication.
- An `ImageId` resolves only while both its slot and generation match.
- A completion publishes only when its request serial still owns the live entry.
- A page slot has at most one owner; slot reuse increments its generation.
- Current decoded/GPU counters equal store-owned live bytes and never exceed configured budgets after enforcement.
- Atlas source rectangles exclude gutters, while the uploaded patch repeats only the owning image's edge pixels.
- No unsafe code or external lifetime invariant is introduced.

## Edge cases and failure modes

Invalid store configuration returns `ImageStoreConfigError` instead of panicking. Canceled, superseded, released, stale-generation, stale-serial, wrong-dimension, and wrong-byte-length completions are rejected before GPU publication. A decoded item larger than the entire decoded budget reports `BudgetExceeded`. Failed backend allocation leaves the item decoded so a later upload can retry. Encoded source bytes are not retained after completion; if both decoded and GPU forms were purged, the caller must submit the source again. Unsupported or malformed PNG input becomes `ImageDecodeError` rather than a partially resident entry.

## Concurrency and memory behavior

Cancellation is an `Arc<AtomicBool>` observed before and during resize. Native workers receive owned jobs over channels and return owned completions; the store itself remains host-thread owned. Browser decode awaits `createImageBitmap` and rechecks cancellation before canvas readback. Intrusive LRUs, bounded upload/decode queues, reusable generation slots, and page free lists avoid per-frame recency-map reconstruction after warm-up.

## Performance notes

Display-size decode avoids uploading unused source pixels. GPU publication temporarily moves rather than clones the decoded buffer, and one retained patch scratch buffer serves atlas uploads. Atlas placement trades page-granularity residency for far fewer textures, binds, and draws at dense populations; sparse populations may consume more GPU bytes and should use measured policy rather than assuming a win. Empty-page creation lets Metal and WebGPU allocate atlas storage without uploading a zero-filled page. At a full same-class budget, the exact least-recently-used slot is overwritten in place instead of evicting a page or destroying/recreating its texture. Append-only cell publication keeps unrelated prepared chunks valid. Every counter is direct store accounting rather than a memory proxy.

## Feature flags and cfgs

The crate has no feature flags. `NativeDecodePool` and PNG decode compile outside wasm; `decode_image_at_display_size_browser` and its DOM helpers compile only for `wasm32`.

## Testing and benchmarks

`cargo test --locked -p oxide-image-store` covers typed configuration rejection, identity reuse, display-size decode, encoded-source release, cancellation/stale completion, gutters, exact/deduplicated invalidation, standalone/mip policy, memory/device purge, generation reuse, scrolling churn, native workers, and 10,000-request budget/page recycling.

C60 perf-runner rows cover 100/1,000/10,000 unique icons and the authoring scroll/release/reuse journey. Metal integration tests compare atlas and standalone pixels and prove one-slot eviction invalidates only its referencing prepared chunk. Browser and physical-iPhone matrix promotion remains owned by C61/C62.

## Examples

```rust
let mut store = ImageStore::new(ImageStoreConfig::default())?;
let id = store.request(ImageRequest { variant, encoded, usage: ImageUsage::Static });
pool.dispatch(&mut store, 8);
pool.collect(&mut store);
store.upload_ready(&mut renderer);
let image = store.resolve_for_chunk(id, chunk_id);
```

## Changelog

- 2026-07-15: introduced C60 generation-safe display-size decode, cancellation, bounded decoded/GPU residency, atlas gutters, standalone/mip fallback, exact prepared-chunk invalidation, and device-loss recovery.
