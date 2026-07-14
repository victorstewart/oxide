# oxide-renderer-metal::prepared

## Intention and purpose

`prepared` lowers immutable `RenderChunk` payloads once into persistent Metal buffers and a compact operation plan. C29 also retains eligible snapshot layers as Metal textures, so a clean layer composites without lowering, hashing, cloning, scanning, copying, or uploading its body.

## Relation to the rest of the code

- `oxide-renderer-api` supplies immutable chunk identity, structural/geometry/resource revisions, resource generations, ordered snapshot instances, and property slots.
- `MetalRenderer::encode_snapshot` owns admission, frame planning, render-pass encoding, accounting, and flat compatibility fallback.
- `ui.metal`, `text.metal`, and `solid.metal` consume the 48-byte dynamic instance record from the active completion-protected uniform ring while persistent buffers retain immutable RRect, image, glyph, and solid payloads.
- Image and atlas create/update/release paths in `renderer-metal::lib` maintain resource generations and invalidate dependent cache entries before stale textures can be replayed.

Call flow:

```text
RenderSnapshot instance
  -> PreparedChunkKey revisions/device/target identity
  -> optional PreparedLayerKey + exact resource generations
  -> clean layer-texture hit and one composite
     or one-time/dirty prepared-body render and one composite
     or exact flat fallback
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

On a miss, lowering accepts balanced clip operations plus RRects, images, image meshes, glyph runs, and solids. Consecutive compatible immutable commands become one prepared operation. Buffers use shared storage because the focused same-binary private-buffer comparison regressed one-dirty GPU p50 by 79.1% and every measured tail. Images retain a finalized immutable argument buffer when supported. Glyph/mesh vertices, normalized local indices, and draw-color records are persistent. No private staging or indirect command buffer is enabled.

Each snapshot visit resolves property slots into one affine matrix, translation, and opacity value. Those values, an identity-matrix flag, the instance origin, and the current viewport form a 48-byte record sent separately from cached geometry. All frame records are copied once into one contiguous slice of the active frame's completion-protected uniform ring, and draws bind offsets into that slice rather than asking Metal to stage one inline constant block per chunk. Equal adjacent property-slot lists reuse the resolved property value. Translation-only RRect/image vertices preserve flat-path world-coordinate rounding at fragment edges; general affine instances retain local fragment coordinates. Clip rectangles are transformed into conservative integer bounds; nested chunk clips remain ordered and intersect with the instance and damage scissors.

Small damage first queries the snapshot's stable world index, inverse-transforms the resulting scissor into each selected chunk, and queries its prepared paint spans. A prepared command-to-operation map jumps directly to the selected operations in paint order and applies their pre-resolved clips, so command filtering performs no full-list or vertex-span scan. Full damage skips both queries. Large unchanged property-free snapshots retain their backend frame plan and validate its unique prepared keys before linear replay, avoiding a second 10,000-instance planning walk without weakening resource-generation or viewport invalidation.

An eligible `RenderLayerInstance` gets a complete layer key containing the stable layer id; prepared chunk identity and revisions; content, structural/nested, and dynamic generations; local bounds; diagonal scale; opacity; target scale; target format/sample count; device generation; and conservative effect outset. The cache entry separately retains the exact image and glyph-atlas dependency list and compares it on every hit. Translation is deliberately outside the key because it only moves the final composite. Rotation, shear, instance clips, unbounded effects, internal layer commands, spinners, and other unsupported bodies use the exact flat path.

On a clean hit, the frame plan records only the existing texture composite. It does not consult the prepared-body cache, so body-buffer eviction does not turn clean replay into hidden body work. A miss or dirty key prepares the body if necessary, renders it once into a compatible private texture on the frame command buffer, then composites once into the main target. Repeated occurrences of the same stable layer key in one frame share that one refresh. Same-size compatible textures survive refresh and prepared-cache purge; resource update or release invalidates every dependent prepared-layer key before reuse.

Prepared layers preserve the parent C05 layer blending/composite contract. Translucent-RRect and image/glyph/image-mesh/Solid bodies use the main layer format. Opaque RRect antialiasing uses an RGBA32Float intermediate because main-format and RGBA16Float trials each changed eight edge-alpha bytes; RGBA32Float matches the parent pixels exactly. A body mixing opaque and translucent RRects uses the exact flat fallback because neither single intermediate precision reproduces both parent quantization paths. The exact pipeline family is optional at initialization, so devices that cannot build it fall back rather than changing pixels.

The cache owns one current prepared version per chunk id. Revision replacement or resource invalidation removes the old version. Admission rejects an entry larger than the hard budget; otherwise generation-aware LRU eviction removes the coldest unprotected entries. Resident accounting uses Metal's allocated buffer sizes, while logical accounting uses payload lengths.

Unsupported layers, effects, spinners, or malformed resource dependencies use `RenderSnapshot::flatten_into` and the established `encode_pass` path. Flat scratch capacity persists across frames, and fallback accounting reports copied commands and geometry exactly once.

## Preconditions and postconditions

- The renderer must be resized and inside a frame begun by `begin_frame`.
- Snapshot property slots must be finite, sorted, unique, and referenced completely.
- Snapshot resource dependencies must match renderer-owned image/atlas generations.
- A successful prepared clean layer replay performs zero body scans/copies, geometry copies, uploads, texture creates, prepared-body work, or offscreen draws; it issues one final composite per layer.
- A dirty prepared layer renders its body exactly once and composites it exactly once, even when the stable layer key appears repeatedly in the frame.
- A one-revision change rebuilds only that chunk; unchanged chunk identities remain cache hits.

## Edge cases and failure modes

- Internal nested layer commands are not yet lowered as independent prepared sublayers; the complete snapshot uses the exact flat compatibility path.
- Rotation, shear, instance clipping, unbounded effects, spinners, duplicate stable ids with different keys, or unavailable exact Solid-layer pipelines reject prepared-layer admission.
- An entry larger than the byte budget is not retained.
- Missing or stale textures prevent admission and cannot be silently sampled.
- Non-finite dynamic properties use the checked flat path, which returns the renderer-api error when equivalence cannot be preserved.
- Empty or invalid geometry is rejected by lowering and remains subject to the established flat validation behavior.

## Concurrency and memory behavior

`MetalRenderer` owns both caches and mutates them only on its render thread. The frame plan stores keys rather than raw pointers, preserving `MetalRenderer: Send` for native host ownership. Metal buffers retain immutable shared contents until eviction, invalidation, purge, or renderer destruction. Prepared layer textures are private and retained by the existing layer-cache accounting; the RGBA32Float opaque-RRect path accounts at 16 logical bytes per pixel. iOS critical-memory handling purges prepared keys and schedules a normal replacement frame.

## Performance notes

Clean replay allocates no new Metal buffers and copies no immutable geometry. C26 moves dynamic records into a separate completion-protected property ring and tracks the last value revision per physical frame slot, so unchanged records upload zero bytes and changed records copy exactly 48 bytes each. C27's property-free full-damage path reuses the unchanged frame plan after validating unique cache keys; its small-damage path visits only selected indexed entries and records instance/command/vertex query counts plus query CPU. The retained frame-plan vector, property cache, damage scratch, and clip-stack pool preserve capacity. Miss cost is proportional only to the changed chunk. The default hard budget is 32 MiB.

The C29 cases are `gpu.architecture.prepared_layers.{clean_100x100,one_dirty_100x100}` and `gpu.authoring.retained_snapshot.prepared_layers_clean_100x100`. Clean replay requires 100 texture hits and zero body/offscreen/upload work. The dirty row requires 99 clean hits, one miss, one offscreen body render, one main composite, and no new layer texture after warmup. All rows report frame/encode/GPU distributions, passes, draws, body scans/copies, geometry copies, uploads, texture creates, cache outcomes, prepared chunks, and layer residency.

C31 preflights every unique prepared layer through `heapTextureSizeAndAlign`. If the complete protected set fits, refreshes acquire exact-format compatible textures from the pool before allocating; clean and refreshed composites update their last-used frame. If the set cannot fit, the immutable snapshot takes the existing exact flat path before any layer pass is encoded. This keeps the hard budget authoritative without weakening prepared-layer or resource-generation keys.

## Feature flags and cfgs

Prepared pipelines are unavailable in the direct-camera-preview-only renderer configuration. Snapshot readback verification uses the existing `snapshot-tests` feature; the prepared production path itself has no feature flag.

## Testing and benchmarks

- `renderer-metal/tests/snapshots.rs` compares prepared and flat mixed/fractional pixels under Metal validation, changes dynamic properties without rebuilding, checks one-dirty behavior, enforces LRU bytes, validates resource generations, exercises purge, and freezes fallback accounting.
- `prepared_small_damage_queries_one_glyph_or_mesh_without_vertex_scans` freezes exact full/small pixels, one-entry selection, zero vertex/copy/upload work, and full static-plan reuse.
- `prepared_property_ring_uploads_only_changed_instance_records_after_warmup` warms every physical slot, proves an unchanged frame uploads zero property bytes, then proves one changed instance uploads one 48-byte record and zero geometry.
- `prepared_layer_clean_hit_composites_without_body_work_and_matches_flat_pixels` covers retained RRect, image, glyph, image-mesh, and Solid bodies and requires exact parent flat-layer pixels.
- `prepared_layer_main_format_matches_flat_translucent_rrect_pixels` freezes overlapping translucent RRect and alpha-channel parity with the parent layer path.
- `prepared_layer_main_format_image_text_mesh_and_solid_match_flat_pixels` proves the ordinary-format retained image, glyph, image-mesh, and Solid path.
- `prepared_layer_invalidates_once_for_dirty_nested_resource_scale_and_purge_changes` covers one-refresh deduplication, translation reuse, scale/resize/target-scale changes, nested generation, resource generation, and purge.
- `prepared_layer_effect_nested_and_unsupported_content_preserve_flat_fallback_pixels` freezes exact fallback for effects, internal layers, and spinner content.
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

- 2026-07-14: integrated prepared layers with allocated-byte admission, protected-set budgeting, compatible texture pooling, last-use tracking, and exact over-budget fallback.
- 2026-07-13: added C29 generation-keyed prepared snapshot layers with body-free clean composite, single-owner dirty refresh, exact resource invalidation, adaptive exact opaque-RRect intermediates, and exact unsupported fallback.
- 2026-07-13: added C27 prepared image meshes, indexed small-damage replay, zero-vertex-scan accounting, and validated static full-plan reuse.
- 2026-07-13: added a completion-protected changed-record property ring with separate logical upload/residency counters for C26.
- 2026-07-13: added byte-budgeted persistent Metal preparation for immutable RRect, image, glyph, solid, and clip chunks with dynamic properties, generation invalidation, exact fallback, memory-pressure purge, and permanent performance contracts.
