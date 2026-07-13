# oxide-renderer-metal::prepared

## Intention and purpose

`prepared` lowers immutable `RenderChunk` payloads once into persistent Metal buffers and a compact operation plan. Clean `RenderSnapshot` frames reuse those buffers without copying geometry or traversing the chunk's commands again, while transform, opacity, origin, clip, and viewport values remain frame-dynamic.

## Relation to the rest of the code

- `oxide-renderer-api` supplies immutable chunk identity, structural/geometry/resource revisions, resource generations, ordered snapshot instances, and property slots.
- `MetalRenderer::encode_snapshot` owns admission, frame planning, render-pass encoding, accounting, and flat compatibility fallback.
- `ui.metal`, `text.metal`, and `solid.metal` consume the 48-byte dynamic instance record from the active completion-protected uniform ring while persistent buffers retain immutable RRect, image, glyph, and solid payloads.
- Image and atlas create/update/release paths in `renderer-metal::lib` maintain resource generations and invalidate dependent cache entries before stale textures can be replayed.

Call flow:

```text
RenderSnapshot instance
  -> PreparedChunkKey revisions/device/target identity
  -> prepared-cache hit or one-time lowering
  -> frame-dynamic transform/opacity/clip record
  -> persistent-buffer Metal draws
  -> frame stats and byte-budgeted LRU
```

## Entry points list

- `MetalRenderer::encode_snapshot(&mut self, snapshot: &RenderSnapshot) -> Result<(), RenderSnapshotError>`
  - Replays supported immutable chunks directly and uses the retained flat adapter for unsupported snapshot structures.
- `MetalRenderer::prepared_cache_budget_bytes(&self) -> u64`
  - Returns the hard allocated-byte budget.
- `MetalRenderer::set_prepared_cache_budget_bytes(&mut self, budget_bytes: u64)`
  - Applies a new hard budget immediately and evicts least-recently-used entries until resident bytes fit.
- `MetalRenderer::prepared_cache_resident_bytes(&self) -> u64`
  - Returns API-exposed allocated bytes owned by prepared buffers.
- `MetalRenderer::prepared_cache_entry_count(&self) -> usize`
  - Returns the number of currently admitted chunk versions.
- `MetalRenderer::purge_prepared_chunks(&mut self)`
  - Releases every prepared entry for memory pressure or explicit lifecycle control.
- `MetalRenderer::image_generation(&self, handle: ImageHandle) -> Option<u64>`
  - Exposes the renderer-owned generation used to construct compatible snapshot dependencies.

## Logic narrative

The cache key contains chunk id, structural revision, geometry revision, resource revision, renderer device generation, target color format, and sample count. A hit also verifies every referenced image or glyph-atlas generation against the renderer's current generation table. An update or release invalidates all dependent entries immediately; a mismatched snapshot dependency falls back rather than sampling stale content.

On a miss, lowering accepts balanced clip operations plus RRects, images, glyph runs, and solids. Consecutive compatible immutable commands become one prepared operation. Buffers use shared storage because the focused same-binary private-buffer comparison regressed one-dirty GPU p50 by 79.1% and every measured tail. Images retain a finalized immutable argument buffer when supported. Glyph vertices, normalized local indices, and draw-color records are persistent. No private staging or indirect command buffer is enabled.

Each snapshot visit resolves property slots into one affine matrix, translation, and opacity value. Those values, an identity-matrix flag, the instance origin, and the current viewport form a 48-byte record sent separately from cached geometry. All frame records are copied once into one contiguous slice of the active frame's completion-protected uniform ring, and draws bind offsets into that slice rather than asking Metal to stage one inline constant block per chunk. Equal adjacent property-slot lists reuse the resolved property value. Translation-only RRect/image vertices preserve flat-path world-coordinate rounding at fragment edges; general affine instances retain local fragment coordinates. Clip rectangles are transformed into conservative integer bounds; nested chunk clips remain ordered and intersect with the instance and damage scissors.

The cache owns one current prepared version per chunk id. Revision replacement or resource invalidation removes the old version. Admission rejects an entry larger than the hard budget; otherwise generation-aware LRU eviction removes the coldest unprotected entries. Resident accounting uses Metal's allocated buffer sizes, while logical accounting uses payload lengths.

Unsupported layers, effects, spinners, meshes, or malformed resource dependencies use `RenderSnapshot::flatten_into` and the established `encode_pass` path. Flat scratch capacity persists across frames, and fallback accounting reports copied commands and geometry exactly once.

## Preconditions and postconditions

- The renderer must be resized and inside a frame begun by `begin_frame`.
- Snapshot property slots must be finite, sorted, unique, and referenced completely.
- Snapshot resource dependencies must match renderer-owned image/atlas generations.
- A successful prepared clean replay uploads zero geometry bytes and traverses zero cached commands.
- A one-revision change rebuilds only that chunk; unchanged chunk identities remain cache hits.

## Edge cases and failure modes

- A chunk containing layer structure is not admitted because prepared layer-generation ownership is not yet represented; the complete snapshot uses the flat compatibility path.
- An entry larger than the byte budget is not retained.
- Missing or stale textures prevent admission and cannot be silently sampled.
- Non-finite dynamic properties use the checked flat path, which returns the renderer-api error when equivalence cannot be preserved.
- Empty or invalid geometry is rejected by lowering and remains subject to the established flat validation behavior.

## Concurrency and memory behavior

`MetalRenderer` owns the cache and mutates it only on its render thread. The frame plan stores keys rather than raw pointers, preserving `MetalRenderer: Send` for native host ownership. No cache mutation occurs during prepared encoding. Metal buffers retain immutable shared contents until eviction, invalidation, purge, or renderer destruction. iOS critical-memory handling purges the cache and schedules a normal replacement frame.

## Performance notes

Clean replay performs one bounded cache lookup and one dynamic record per instance but allocates no new Metal buffers and copies no immutable geometry. Dynamic records use one existing frame-ring slice; the 256-chunk contract reports exactly 12,288 dynamic uniform bytes separately from zero clean immutable upload. The retained frame-plan vector and clip-stack pool preserve capacity. Miss cost is proportional only to the changed chunk. The default hard budget is 32 MiB.

The permanent cases are `gpu.architecture.prepared_chunks.clean_mixed`, `gpu.architecture.prepared_chunks.one_dirty`, and `gpu.authoring.retained_snapshot.clean_mixed`. They report encode/GPU/frame distributions, upload and geometry-copy bytes, command traversal, draws, image-table binds, hit/miss/rebuild counts, eviction count, and resident prepared/renderer bytes.

## Feature flags and cfgs

Prepared pipelines are unavailable in the direct-camera-preview-only renderer configuration. Snapshot readback verification uses the existing `snapshot-tests` feature; the prepared production path itself has no feature flag.

## Testing and benchmarks

- `renderer-metal/tests/snapshots.rs` compares prepared and flat mixed/fractional pixels under Metal validation, changes dynamic properties without rebuilding, checks one-dirty behavior, enforces LRU bytes, validates resource generations, exercises purge, and freezes fallback accounting.
- `perf-runner/tests/report_tests.rs` freezes clean zero-upload/zero-traversal and one-dirty exact-work counters.
- Run `MTL_DEBUG_LAYER=1 cargo test --locked -p oxide-renderer-metal --test snapshots prepared_snapshot --features snapshot-tests`.
- Run `OXIDE_PERF_RUNNER_FILTER=gpu.architecture.prepared_chunks. cargo run --release --locked -p oxide-perf-runner -- --run-suite`.

## Examples

```rust
let token = renderer.begin_frame(&FrameTarget, Some(snapshot.damage()));
renderer.encode_snapshot(&snapshot)?;
renderer.submit(token)?;
```

## Changelog

- 2026-07-13: added byte-budgeted persistent Metal preparation for immutable RRect, image, glyph, solid, and clip chunks with dynamic properties, generation invalidation, exact fallback, memory-pressure purge, and permanent performance contracts.
