# oxide-renderer-metal::id_mask_gpu

## Intention and purpose

`id_mask_gpu` owns Metal rasterization, nearest-feature field generation, bounded immutable-field reuse, final semantic-mask composition, and snapshot-only field readback. It avoids repeating full-resolution raster, seed, and jump-flood work when geometry and projection are unchanged.

## Relation to the rest of the code

- `MetalRenderer` constructs the raster plus packed/wide seed, jump, and compositor pipeline states up front and calls this module from `encode_id_mask_gpu_compositor`.
- `id_mask_compositor` supplies generic pass, projection, chunk, style, and polish data.
- `id_mask_compositor.metal` consumes the raster, field, and final-compositor parameter blocks.
- `architecture_matrix` exercises the production begin/encode/submit path and persists cache, pass, timing, and residency evidence.

Call flow:

- host/perf/test -> `MetalRenderer::begin_frame`
- caller -> `MetalRenderer::encode_id_mask_gpu_compositor`
- cache hit -> final compositor
- cache miss -> raster -> field seed -> JFA ping-pong passes -> retain -> final compositor
- caller -> `MetalRenderer::submit`

## Entry points list

- `MetalRenderer::encode_id_mask_gpu_compositor(&mut self, pass: &IdMaskGpuCompositorPass<'_>) -> Result<(), RenderError>`
  - Validates input, resolves the immutable-field cache, encodes required Metal passes, and composites into the current color target.
- `MetalRenderer::readback_id_mask_snapshot(&self) -> Option<IdMaskSnapshotReadback>` with `snapshot-tests`
  - Reads exact R8 IDs and decodes the final packed or wide city/seam fields into exact seed vectors for verification only.
- `IdMaskSnapshotReadback`
  - Exposes mask dimensions and exact CPU-owned field bytes/vectors to snapshot tests.

## Logic narrative

The fixed cache key stores mask dimensions, the exact `mask_scale` bits, aggregate vertex revision and count, ordered chunk hashes/ranges, and exact bits for every projection matrix, camera, hemisphere, and normal input consumed by rasterization. Styles, colors, glow, polish, mode, opacity, and viewport placement are absent because they only affect the final compositor.

On a hit, the entry's LRU frame is refreshed, its serial is protected for the current command buffer, and only the final compositor is encoded. On a miss, admission evicts cold unprotected entries until the actual allocated-byte budget and four-entry bound permit the new set. A dimension-compatible evicted set can be rewritten instead of allocating new textures. Raster writes city and neighborhood R8 targets. When each maximum coordinate is below `0xFFFF`, seed and logarithmic JFA passes ping-pong two RGBA16Uint fields: city XY occupies `.xy`, seam XY occupies `.zw`, and `0xFFFF` is invalid. The final compositor recovers city and neighborhood IDs from the authoritative R8 masks at the selected coordinates. Dimensions needing the sentinel coordinate retain two city plus two seam RGBA32Float fields and dedicated prebuilt pipeline states; compile-time assertions pin the selector boundary at 65,535 accepted and 65,536 rejected.

The field set becomes immutable after its miss sequence. The current-frame serial list prevents a later map in the same command buffer from recycling fields already referenced by an earlier compositor. Cross-frame reuse and rewrites stay on the renderer's single command queue with default tracked hazards; command-buffer retention keeps submitted resources alive until Metal completes them.

## Preconditions and postconditions

- Dimensions are nonzero and sample count is one.
- Vertices are a complete nonempty triangle list covered exactly by valid chunks.
- Callers change `vertex_revision` and affected chunk hashes when geometry changes.
- A successful encode initializes or loads the current color target and records exact cache/stage counters.
- Cache residency never exceeds its configured allocated-byte budget after admission completes.

## Edge cases and failure modes

- Zero dimensions, empty/partial triangles, overflowed byte counts, unavailable target storage, or null shared-buffer mappings return `RenderError`.
- An entry larger than the budget renders transiently and is not admitted.
- When every resident entry is protected by the current frame, an additional miss uses transient targets rather than corrupting an earlier map.
- Exact floating-point bits make NaNs and signed zero conservative cache boundaries instead of approximate matches.
- Explicit purge clears both cache entries and frame-slot references; already committed command buffers retain only their required in-flight ownership.

## Concurrency and memory behavior

Renderer mutation is single-owner through `&mut MetalRenderer`. Cache entries clone Metal handles but not texture storage. CPU allocation occurs only on cache misses when an admitted entry copies its bounded ordered chunk key; hits allocate no chunk vector and create no Metal resources. The field budget defaults to one eighth of the device-recommended working set, clamped to 64–512 MiB, and can be overridden through `OXIDE_ID_MASK_CACHE_BUDGET_BYTES` or the public setter.

## Performance notes

A cache hit changes a 512-square field build from raster + seed + nine JFA passes + compositor to one compositor pass. Packed field ownership is 16 logical bytes per pixel across both ping-pong textures versus 64 for four wide fields, and each packed JFA candidate read fetches one eight-byte city/seam coordinate record instead of two 16-byte records. Counters expose cache hits/misses, entries, resident/budget bytes, evictions, and raster/seed/jump/compositor pass counts. Target byte accounting uses Metal allocated size, deduplicated by resource identity in renderer memory reports.

## Feature flags and cfgs

- The production encoder is available on Apple Metal targets.
- `snapshot-tests` adds synchronous readback and format-independent decoded field vectors; it does not affect the production path.

## Testing and benchmarks

- `id_mask_compositor_tests` covers complete keys, final-only hits, projection/content/scale/dimension misses, same-command-buffer two-map hits, hard-budget eviction, and purge.
- `snapshots` requires exact cached-versus-fresh fields and final pixels under Metal validation.
- `architecture_matrix` covers static, style, viewport, projection, content, and same-frame two-map workloads at representative sizes and chunk counts.
- `metal_id_mask_reference_tests` compares every decoded packed field pixel against CPU references at 256/512/1024/2048 and unusual aspect ratios.

## Examples

```rust
let token = renderer.begin_frame(&FrameTarget, None);
renderer.encode_id_mask_gpu_compositor(&pass)?;
renderer.submit(token)?;
```

## Changelog

- 2026-07-14: packed city/seam seed coordinates into two RGBA16Uint ping-pong fields, recovered semantic IDs from R8 masks in the compositor, and retained an exact wide-coordinate fallback.
- 2026-07-14: added complete-key, byte-budgeted immutable raster/JFA field caching with compositor-only hits, compatible target reuse, stage telemetry, and purge behavior.
