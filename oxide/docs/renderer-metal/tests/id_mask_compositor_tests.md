# oxide-renderer-metal::tests::id_mask_compositor_tests

## Intention and purpose

These tests freeze ID-mask input validity, chunk-keyed vertex uploads, complete immutable-field cache invalidation, same-frame multi-map safety, completion-safe target reuse, bounded eviction, and explicit purge behavior.

## Relation to the rest of the code

- Public data-contract tests exercise `id_mask_compositor` pass types without a GPU.
- macOS runtime coverage constructs `MetalRenderer`, uses the production begin/encode/submit path, and reads public `PerfStats` counters.
- Exact field and final-pixel parity remains in `tests/snapshots.rs`.

Call flow:

- test data -> `IdMaskGpuRasterPass::valid_triangle_vertex_count`
- runtime pass -> `MetalRenderer::encode_id_mask_gpu_compositor`
- submit -> `MetalRenderer::last_stats`
- pressure -> budget setter or explicit purge -> public cache telemetry

## Entry points list

- `id_mask_gpu_raster_rejects_empty_or_non_triangle_vertices()` validates malformed input boundaries.
- `id_mask_gpu_raster_accepts_triangle_vertices_and_generic_style_alias()` validates the generic semantic-mask contract.
- `id_mask_gpu_upload_cache_is_content_hash_chunk_keyed()` freezes persistent chunk-upload ownership.
- `id_mask_field_cache_hits_final_only_changes_and_evicts_complete_keys()` validates runtime cache and pass behavior on macOS.

## Logic narrative

The runtime test first populates one 64-square field set and requires one raster, one seed, six JFA, and one compositor pass. A frame changing viewport, styles, colors, polish, mode, glow, and background alpha must hit and encode only the compositor. Projection, vertex revision, ordered chunk hash, mask scale, and dimensions each force a miss. Two already-warm map revisions are then encoded in one command buffer and must produce two hits and two compositor passes without rebuilding fields.

After four equal-size entries are resident, the test lowers the budget to one entry, requires immediate LRU eviction, exercises further misses and compatible storage reuse, and checks residency never exceeds the hard budget. A zero-budget snapshot path must reuse one completed same-size transient target without another Metal allocation. A deterministic busy-slot fixture then forces eviction of an active generation: the renderer must allocate a second target, report two unique generations and twice one-generation storage, preserve pixels, and avoid backpressure. Releasing that slot permits the following same-size miss to recycle the completed target with zero creation. Explicit purge must report zero entries and zero resident bytes.

## Preconditions and postconditions

- Runtime coverage requires macOS Metal.
- Passing proves observable cache classifications and stage counts through public APIs.
- The source-contract upload test complements runtime behavior but does not replace it.

## Edge cases and failure modes

Empty vertices, partial triangles, out-of-contract chunk coverage, incomplete keys, unintended final-only invalidation, missing misses, excess residency, or absent purge telemetry fail deterministically.

## Concurrency and memory behavior

Tests use one renderer and bounded textures. Same-command-buffer dual-map encoding exercises current-frame cache protection. Snapshot hooks set and release one completion bit deterministically so the target-reuse test does not depend on timing. Metal validation is enabled by the verification command.

## Performance notes

The warm hit contract is zero raster, seed, and jump passes. Budget tests validate allocated residency rather than inferring memory from logical dimensions. Target telemetry must distinguish cache residency, unique in-flight bytes, total cache-plus-in-flight bytes, lifetime peak, target creation, and synchronization-blocked reuse.

## Feature flags and cfgs

The runtime cache test is guarded by `target_os = "macos"`; pure data tests are portable.

## Testing and benchmarks

Run `MTL_DEBUG_LAYER=1 cargo test --locked -p oxide-renderer-metal --features snapshot-tests --test id_mask_compositor_tests`.

## Examples

The public budget control used by the test is:

```rust
renderer.set_id_mask_cache_budget_bytes(one_entry_budget);
renderer.purge_id_mask_field_cache();
```

## Changelog

- 2026-07-14: added C36 zero-budget same-size pooling plus deterministic busy-generation rejection, exact storage/create telemetry, completed reuse, pixel parity, and backpressure coverage.
- 2026-07-14: added C32 complete-key, final-only, same-frame multi-map, budget/eviction, and purge coverage.
