# oxide-renderer-metal::lib

## Intention and purpose

`oxide-renderer-metal` is Oxide's Metal backend. It owns the retained GPU resources, frame command buffers, pipeline state, texture management, and encode path that turn Oxide draw data into Metal work on Apple platforms.

## Relation to the rest of the code

- Upstream callers:
  - `oxide_render_metal::MetalRenderer` is constructed by Oxide hosts and by renderer-focused tests and perf cases.
  - Standalone Rust apps can also depend on this crate directly when they need the same Oxide Metal backend without the full UI stack.
- Downstream dependencies:
  - `oxide-renderer-api` provides draw-list and renderer traits.
  - The in-crate Metal shaders under `oxide/crates/renderer-metal/shaders/` back the pipeline state objects created here.

## Entry points list

- `MetalRendererConfig::visible_host() -> MetalRendererConfig`
  - Selects the normal three-slot visible-host frame-resource mode; `Default` preserves the separately configured eight-slot offscreen/perf mode.
- `MetalRenderer::new_with_config(config) -> Result<MetalRenderer, MetalInitError>`
  - Builds the Metal device/queue, shader library, pipeline state, and the clamped one-to-eight-slot frame resources declared by `config.frame_resource_depth`.
- `MetalRenderer::mesh3d_create(data) -> Result<MeshHandle3d, RenderError>`
  - Uploads a static indexed 3D mesh into persistent Metal buffers for reuse across frames.
- `MetalRenderer::encode_scene3d(pass) -> Result<(), RenderError>`
  - Encodes one retained 3D pass into the current frame before `encode_pass`.
- `MetalRenderer::encode_id_mask_gpu_compositor(pass) -> Result<(), RenderError>`
  - Rasterizes semantic region/subregion ID triangles into renderer-owned R8 targets before running the compositor shader. Implementation lives in `id_mask_gpu.rs`.
- `MetalRenderer::readback_id_mask_snapshot()` (`snapshot-tests`)
  - Reads exact raster IDs and format-independent decoded final city/seam seeds for CPU-reference parity tests.
- `MetalRenderer::set_force_exact_blur_for_snapshot(force_exact)` (`snapshot-tests`)
  - Selects the exact Gaussian control only in snapshot-feature builds so paired-kernel image error can be measured against the identical renderer path.
- `MetalRenderer::set_memory_stats_enabled_for_benchmark(enabled)`
  - Enables or disables sampled resident-resource scans for explicit accounting-overhead controls; rendering behavior is unchanged.
- `MetalRenderer::set_accounting_stats_enabled_for_benchmark(enabled)`
  - Enables or disables the complete renderer-accounting path, including work snapshots and sampled resident-resource scans, for paired overhead controls; rendering behavior is unchanged.
- `MetalRenderer::encode_neon_markers(pass) -> Result<(), RenderError>`
  - Encodes bounded neon marker instances over the current color target before `encode_pass`. Implementation lives in `neon_marker_gpu.rs`.
- `MetalRenderer::encode_pass(list)`
  - Encodes the existing 2D Oxide draw list and reuses the same frame command buffer when a scene3d pass already ran.
- `MetalRenderer::encode_snapshot(snapshot) -> Result<(), RenderSnapshotError>`
  - Replays immutable supported chunks from persistent prepared buffers and retains the flat compatibility path for unsupported structures.
- `MetalRenderer::{prepared_cache_budget_bytes,set_prepared_cache_budget_bytes,prepared_cache_resident_bytes,prepared_cache_entry_count,purge_prepared_chunks}`
  - Configure, inspect, and release the byte-budgeted prepared cache.
- `MetalRenderer::{id_mask_cache_budget_bytes,set_id_mask_cache_budget_bytes,purge_id_mask_field_cache}`
  - Inspect, resize, or release the hard-budgeted cache of immutable ID-mask raster/JFA fields.
- `MetalRenderer::image_generation(handle) -> Option<u64>`
  - Returns the explicit generation required by image and glyph-atlas chunk dependencies.
- `MetalRenderer::image_create_rgba8_immutable(w, h, data, row_bytes, repeatedly_minified) -> ImageHandle`
  - Declares stable RGBA8 source bytes and opts repeatedly minified images into the measured complete-mip policy while preserving the direct Shared path for non-minified assets.
- `ImageResidencyStats`
  - Exposes current Shared/Private texture counts, mip levels, and retained bytes plus cumulative staging, Private-upload, mip-generation, and upload-command-buffer counters to tests and perf reports.
- `MetalRenderer::image_residency_stats() -> ImageResidencyStats`
  - Reports current Shared/Private texture counts and bytes plus cumulative staging, mip-generation, and upload-command-buffer work.
- `MetalRenderer::image_create_rgba8_immutable_for_benchmark(w, h, data, row_bytes, private, mipmapped) -> ImageHandle` (`doc(hidden)`)
  - Selects one isolated storage/mip combination for same-build evidence controls; production callers use the measured immutable entry point.

## Logic narrative

Solid draws keep their existing vertex and uniform ring uploads. The solid command color is now bound at vertex buffer index 1 so the shader can replace packed zero before interpolation; nonzero vertex colors pass through without another pipeline, draw, or resource.

The renderer keeps long-lived GPU resources resident and reuses them across frames. Static textures and scene3d meshes are uploaded once, while frame-local rings handle transient 2D geometry and uniforms. Visible hosts allocate three frame slots, matching their normal safe in-flight limit, while offscreen/perf construction explicitly retains eight slots for deeper stress. The ring's fixed direct-access cells avoid a branch on every bind; inactive cells alias the current slot-zero buffer, refresh that alias if slot zero grows, and are excluded from active-depth accounting, so they do not retain hidden Metal storage. Slot selection loads one bounded completion bitset and scans from the next slot without division; the completion handler clears only its submitted slot. If every configured bit remains set, selection skips nonblockingly. Drawable count is not used as a proxy for command-buffer lifetime. Metal retains committed command buffers until completion, so a frame slot does not take a second command-buffer reference solely for reuse tracking.

C59 makes image residency an explicit ownership decision. Dynamic RGBA8, glyph A8, video, and camera resources remain Shared so updates do not acquire a staging texture and copy submission. Non-minified immutable images also remain Shared because physical-iPhone large-static and small-one-use controls rejected staged Private storage. Repeatedly minified immutable images remain Shared and allocate a complete mip chain: isolated physical-iPhone Shared/Private-mip sampling tied at 0.3630/0.3636 ms GPU p50, while Private increased frame and encode p50 about 50%, first-visible time 15.0%, and creation peak 74.8%. The Mac cross-check also tied, so Shared won the simpler ownership and 42.8%-lower creation-peak tie-break. The renderer generates the chain once on its serial queue and samples with linear mip filtering plus clamp-to-edge addressing; standalone textures therefore need no atlas gutter. Partial updates regenerate an existing chain before later queue submissions can sample it. The hidden benchmark selector retains Shared/Private and mip/no-mip controls but does not alter the production policy.

Image ownership remains with the caller rather than a speculative renderer-side source cache. Memory-pressure purges release derived effect, layer, ID-mask, and prepared data but preserve app-owned image textures. Replacing the Metal renderer drops every old-device handle; the owning image layer must replay its source bytes into the new renderer. C59 verifies that replay exactly. C60 implements `ImageResidencyBackend` directly on `MetalRenderer`: atlas pages are empty Shared `RGBA8Unorm_sRGB` textures, validated cell publication is an append-only region upload, standalone minified variants use the existing mip path, the renderer has a unique backend generation, and store eviction invalidates only recorded prepared chunks and retained layers built from them.

Image call flow:

- caller bytes -> dynamic or immutable creation -> Shared upload or optional Shared staging/Private copy
- optional mip generation -> retained `ImageTexture` metadata -> ordinary argument-table/prepared sampling
- update/release -> resource-generation invalidation -> prepared chunk/layer refresh or residency decrement

Each slot starts with 512 KiB of vertex storage, 64 KiB of index storage, and 72 KiB of uniform storage. Those values cover both the measured 4,096-quad visible workload (327,680/49,152/16 bytes) and the existing 1,024-marker workload's 73,728 uniform bytes without growth. Larger stress frames grow only the active slot geometrically and retain that high-water capacity, replacing the previous unconditional 4/2/2 MiB allocation on all eight slots. Growth, prefix copying, and inactive-alias refresh live in one cold non-inlined path, leaving the ordinary capacity-hit check compact.

Mixed 2D/3D frames share the same frame command buffer and color target. `encode_scene3d` initializes color/depth when needed, then `encode_pass` loads the already-rendered target instead of clearing it again. The supported ordering is 3D first, then 2D overlay, which matches the intended Oxide use case of a 3D scene under author-driven 2D interface chrome.

The 2D encoder validates local or rebased `u16` index spans before upload, then writes normalized indices directly into the frame-local Metal index ring for Solid and GlyphRun draws. This avoids allocating a temporary index `Vec` in the shared renderer hot path while preserving the existing local-index and absolute-index contracts.

Consecutive compatible rounded rectangles, nine-slices, images, spinners, backdrop composites, and visual effects reserve aligned slices from the selected completion-protected frame ring. Their growing vertex and fragment arrays bind Metal buffers by offset and issue one draw per ordered run, so instance count is no longer constrained by the 4 KiB `set*Bytes` ceiling. Fixed viewport records remain inline. Image parameter construction retains warmed CPU scratch because the measured single-pass build plus one bulk ring copy beats traversing and resolving the draw list twice; every GPU parameter array is still uploaded once.

The renderer reports analytic instance bytes, buffer binds, and ring growth separately. Fragment shaders consume scalable arrays from the device address space, while clip, target, texture-table, alpha-order, and effect-prepass boundaries still terminate compatibility runs. Grouped glyph-run command metadata continues to reuse renderer-owned scratch independently.

Solid, image-mesh, text, and SDF text pipelines share the same API vertex descriptor because they all consume `oxide_renderer_api::Vertex` layout: position, UV, and normalized color packed at a 20-byte stride.

Inline layer fallbacks encode the original draw-list range directly. That keeps vertex and index spans valid without cloning the layer item slice or duplicating the full vertex/index arrays when a layer is rendered inline for prepass, unsupported commands, disabled layer caching, or a stale cache miss.

Damage prefiltering stays allocation-light. It now builds a compact temporary command list that borrows the original vertex and index backing arrays, so geometry-backed `Solid`, `GlyphRun`, and inline layer ranges can still be culled without cloning the full vertex/index payload just to discard off-scissor commands.

Layer caching builds one reusable `LayerPlan` table while walking a flat frame draw list. A valid clean plan composites its retained texture without copying the body or inspecting its geometry; a dirty, missing, or resized plan materializes the body once, renders it once offscreen, and composites it once. Unsupported bodies remain inline. A refreshed nested child marks its cached parent dirty, and same-size private textures are reused across refreshes. Clean pixel-aligned, same-scale composites use a pixel-coordinate nearest sample without nine-slice mapping, while fractional or just-refreshed composites retain linear sampling. Layer-target pipelines preserve source alpha in the transparent cache target, while both composite pipelines treat the cached RGB as premultiplied. The renderer reports structural body scans, copied commands, texture creation, hits/misses, offscreen/inline draws, and prevented duplicate body renders.

Layer texture sublists share one geometry-span offset/rebase helper for image meshes and glyph runs. That keeps local layer coordinates and rebased index spans consistent across the single cache refresh pass and the inline encode fallback path.

Renderer GPU timing is collected in-app instead of depending on Instruments hardware-counter availability. Completed frame command buffers update renderer stats from Metal's command-buffer GPU start/end timestamps, and iOS devices that expose the common timestamp counter set attach an `MTLCounterSampleBuffer` to the main 2D render pass for vertex/fragment/pass attribution. Those values are read after command-buffer completion and surfaced through `last_stats()` without waiting on the GPU from the frame hot path.

Renderer accounting keeps allocated GPU bytes and logical payload bytes separate. Metal's exposed `allocated_size` is deduplicated by resource identity into draw/MSAA, depth, effect, bloom, camera, layer, image, ID-mask, Scene3D mesh, frame-ring, and argument-buffer owners; logical texture extents and buffer lengths are reported independently. The identity sets are renderer-retained and cleared without releasing capacity, so scans allocate only while warming to a new peak resource count. The resident-resource walk is sampled once every 60 frames, while ordinary frames reuse the last snapshot. Work counters use saturating arithmetic for traversed/copied commands, copied geometry, ID-mask chunk reuse/rebuild, cache outcomes, encoders, copies, uploads, shaded pixels, submissions, and resource creation/growth. Explicit benchmark controls may disable the complete accounting snapshot path or only the resident scan without changing rendering.

Immutable render snapshots can bypass flat per-frame lowering through the focused [`prepared`](prepared.md) module. Its chunk key combines revisions, resource generations, device generation, target format, and sample count. Supported RRect/image/image-mesh/glyph/solid/clip payloads live in persistent shared Metal buffers under a 32 MiB default LRU budget, while origin, affine transform, opacity, viewport, clip, and damage remain dynamic. C27 queries precomputed world and chunk-local spatial metadata for small damage, records visited/matched instance/command/vertex work, and reuses a validated property-free full-frame plan.

C29 extends that prepared path to eligible retained snapshot layers. A complete layer key includes stable identity, chunk and content/nested/dynamic generations, bounds, diagonal scale, opacity, target scale/format/sample count, device generation, and effect outset; the cache entry also compares exact resource dependencies. Clean hits composite an existing layer texture without consulting or traversing the prepared body. Dirty or missing layers prepare if needed, render once offscreen, and composite once, with repeated same-key instances sharing the refresh. Resource update/release and prepared-cache purge invalidate layer keys while preserving compatible texture reuse. Translation remains dynamic, while rotation/shear, instance clips, effects/internal layers/spinners, and other unsupported cases retain the exact flat path. Prepared layers preserve the parent C05 blending/composite contract. Ordinary content and translucent RRects use the main layer format; opaque RRects use RGBA32Float to preserve antialiased edge bytes; mixed opaque/translucent RRect bodies fall back because one intermediate cannot reproduce both parent quantization paths.

Frame-level camera/effect metadata is gathered in one draw-list scan. Camera coverage, camera-blur sigma, backdrop presence, and the strongest visual-effect blur plan are reused by the later policy and prepass blocks instead of rediscovering the same facts with separate passes.

Effect target ownership follows that declared plan. A zero-blur backdrop allocates only the full-resolution prepass; ordinary blur adds half/quarter targets and one quarter ping-pong target; strong visual blur substitutes the declared eighth-resolution pair without retaining an unused full-resolution temporary. Compatible textures persist across warm frames. Resize invalidates incompatible targets, while the production memory-warning hook purges both effect and bloom targets and requests a replacement frame. `resource_creates` records first-use construction and the effect/bloom memory categories include every retained target.

The blur quality ladder keeps pass sigma below 2 on the exact per-tap Gaussian path. Canonical kernels at or above 2 whose sigma lands on the 1/16 bucket grid and whose radius remains exactly `ceil(3 * sigma)` use lazily generated, process-reused paired bilinear offsets and normalized weights inside the existing quarter/eighth render-graph chain. Horizontal and vertical records are prepacked so paired encoding retains one constant-data binding per pass. Other radii and non-finite or non-bucket sigma values fall back to exact evaluation. Separate persistent exact and paired pipeline states keep the runtime Gaussian loop out of paired-shader occupancy. The paired path removes runtime exponential evaluation and almost halves texture samples without changing pass count or target residency. `PerfStats` reports exact/paired passes, source and encoded samples, runtime exponential taps, and total process-resident kernel-table bytes after first use.

Camera preview rendering remains Oxide-owned. The renderer consumes `CameraBg` frame data and no longer accepts a native visible-preview draw marker in the product draw-list path.

Synthetic camera benchmark textures keep the BGRA reference and optimized NV12 shader on the same BT.709 full-range contract. The optimized shader uses normalized chroma offsets directly, while the legacy shader intentionally preserves its older divergent full-range conversion so the snapshot benchmark can detect regressions against the BGRA reference.

Scene3D bloom uses the same persistent-object discipline: additive bloom PSOs are created once, bloom textures are reused across frames at a bounded downsample size, and `encode_scene3d()` routes `Pass3d::bloom` through the common effect graph after the main 3D pass has initialized the target. The graph extracts emissive instances once at bloom resolution, serializes each separable blur and additive upsample composite, aliases compatible intermediates, and limits work to conservative viewport-plus-kernel regions. One layer needs two physical intermediates; multiple distinct layers need the minimum three so the common source survives while ping-pong blur output is composed. Scene depth remains private and stored because the public frame API permits later passes; it is not provably terminal and therefore cannot safely be memoryless.

ID-mask composition is GPU-owned. Semantic region/subregion triangles are rasterized into private R8 targets. Supported dimensions pack city-seed XY and seam-seed XY into two ping-ponged RGBA16Uint fields with `0xFFFF` invalid, reducing field logical ownership from 64 to 16 bytes per pixel; the compositor recovers semantic IDs from the original R8 masks. A dedicated four-RGBA32Float path preserves larger coordinates when the sentinel would collide. Both representations use the same strict-distance update order and final composition logic. A complete immutable-field key contains dimensions, exact mask-scale bits, aggregate vertex/chunk content generations and ranges, and every projection value consumed by rasterization. Final-only style, color, glow, polish, mode, opacity, and viewport placement are deliberately excluded. A hit therefore encodes only the compositor; a content, dimension, scale, or projection change reruns raster, seed, and every jump pass.

Cached field sets are immutable while admitted. The renderer retains at most four sets under an adaptive 64–512 MiB allocated-byte budget, evicts the least-recently used set, and may recycle a compatible evicted set for a later miss. Entries referenced earlier in the current command buffer are protected from recycling. Each configured frame slot stores only a small list of field-generation serials and allocated bytes; the large textures remain owned by the cache, the active command buffer, or the single snapshot readback target. Completion bits clear generation metadata before a slot is selected again, and eviction refuses to recycle a generation while any submitted slot still references it. Oversized transient requests evict unprotected retained dimensions before allocation. Snapshot/offscreen builds reuse their one completed same-size readback target and drop it immediately on a dimension mismatch instead of retaining one target set per eight-slot frame ring.

Cache telemetry reports hits, misses, entries, resident/budget bytes, evictions, stage counts, target generations created, unique in-flight generations/bytes, total cache-plus-in-flight storage, lifetime peak storage, and synchronization-blocked reuse. `PerfMemoryStats::id_mask_target_bytes` takes the greater of directly retained handles and generation-aware storage so an evicted target still owned by an in-flight Metal command buffer remains visible. Explicit purge, iOS memory pressure, and device loss release cache and snapshot ownership while retaining metadata for genuinely active submissions until their completion bits clear.

## Preconditions and postconditions

- Preconditions:
  - `MetalRenderer` must be resized before encode work.
  - `encode_scene3d()` currently requires `sample_count == 1`.
  - `encode_scene3d()` must run before `encode_pass()` within a frame.
  - ID-mask compositor dimensions must be non-zero, and GPU raster input must be a non-empty triangle list.
  - Stable ID-mask vertex revisions and chunk content hashes must change whenever their covered geometry changes.
  - Image dimensions, row stride, and source byte coverage must describe a valid Metal upload region.
- Postconditions:
  - Uploaded `MeshHandle3d` values stay valid until `mesh3d_release()` or renderer drop.
  - A mixed 3D/2D frame reuses one frame command buffer and one color target initialization path.
  - Image updates increment resource generations and invalidate prepared dependents; release removes current residency.

## Edge cases and failure modes

- Non-power-of-two immutable textures receive the complete floor-halved mip chain down to one texel.
- A non-minified immutable asset takes the same direct Shared path as a dynamic image, so small/one-use resources cannot accidentally acquire staging work.
- Partial updates to a mipmapped image regenerate the full chain; callers that update frequently should use the non-mip dynamic API.
- Unknown image handles remain no-ops under the pre-existing update/release contract.
- Device replacement invalidates every Metal handle; source replay belongs to the owning image layer rather than hidden renderer recovery state.

## Concurrency and memory behavior

`MetalRenderer` image mutation requires exclusive `&mut self`. Upload, copy, mip-generation, render, and readback command buffers share one serial Metal queue, so later sampling observes committed upload work without a CPU wait. Metal's default retained-reference command-buffer behavior owns transient staging textures through completion. Current residency counters decrement on release; cumulative staging/upload counters intentionally remain monotonic for attribution.

## Performance notes

Ordinary image sampling retains the existing hash lookup, texture bind, argument-table, and prepared-render paths; residency metadata is read only during creation, mutation, release, or reporting. A complete RGBA8 mip chain adds about one third over level-zero allocated bytes. Private controls add one Shared staging allocation and one copy submission, so they are accepted only when device sampling wins enough to repay startup and memory costs. The common sampler is created once and applies linear mip filtering; one-level textures do not create per-draw policy branches.

## Feature flags and cfgs

The backend is available on Apple Metal targets. Raw color readback and the focused image-residency integration suite require `snapshot-tests`; the production immutable APIs and counters do not.

## Testing and benchmarks

- `tests/image_residency_tests.rs` freezes storage, mip updates, quality, release, cache pressure, and renderer recreation.
- `oxide-perf-runner` C59 rows measure large static, minified-grid, small one-use, and public-`ImageView` paths on macOS and the physical iPhone.
- C60 image-store integration tests compare atlas and standalone readback pixels and prove exact one-chunk prepared invalidation; architecture/authoring rows run on macOS and the physical iPhone.

## Examples

Stable assets that are repeatedly sampled below source resolution call `image_create_rgba8_immutable(..., true)`. Dynamic atlases, camera/video surfaces, and frequently updated images continue to call `image_create_rgba8`.

## Changelog

- 2026-07-15: implemented the C60 portable image-residency backend with sRGB empty atlas pages, append-only subregion publication, unique device generations, and exact prepared-chunk invalidation.
- 2026-07-15: added C59 explicit immutable-image residency, complete mip generation and regeneration, storage/upload telemetry, memory-pressure ownership, and renderer-recreation semantics.
- 2026-07-14: added the C52 exact/paired blur quality ladder, dedicated persistent pipeline states, lazy 1/16-sigma kernels, sample/ALU/table telemetry, and snapshot-only exact control.
- 2026-07-14: replaced the eight-entry snapshot ID-mask target array with one completion-safe reusable target, tracked cache/transient generations by actual in-flight slots, blocked unsafe eviction reuse, evicted stale dimensions for oversized transient work, and exposed generation-aware target bytes and creation/reuse counters.
- 2026-07-14: packed Metal ID-mask city/seam coordinates into two RGBA16Uint fields with R8 ID recovery and an exact wide-coordinate fallback.
- 2026-07-14: cached complete Metal ID-mask raster and JFA fields under an allocated-byte LRU budget, skipped all field-building passes for final-only changes, added pass/cache telemetry, and wired memory-pressure/device-loss purge.
- 2026-07-14: bounded retained-layer storage by Metal `allocatedSize`, with adaptive/configurable budgets, protected-frame LRU eviction, compatible whole-texture pooling, absent-layer aging, pool shrink, and scale/device-loss/memory-warning purges. An over-budget frame renders layer bodies inline so budget enforcement cannot change pixels or omit work.
- 2026-07-15: integrated Scene3D bloom into the common effect graph with one emissive extraction, conservative regions, two/three-slot aliasing, plan reuse, and pass/bandwidth/storage telemetry.
- 2026-07-13: added C29 prepared snapshot-layer keys, body-free clean texture replay, single-owner dirty refresh, resource/purge invalidation, and adaptive exact parent-layer parity.
- 2026-07-13: added C27 prepared image meshes, indexed retained damage replay, spatial-query counters, and validated static snapshot-plan reuse.
- 2026-07-13: added persistent byte-budgeted prepared render chunks, dynamic transform/opacity records, resource-generation invalidation, prepared-cache accounting, and memory-pressure purge.
- 2026-07-13: matched visible-host frame resources to three completion-protected slots, consolidated completion state into one bounded bitset, removed variable-modulo scanning and redundant per-slot command-buffer retention, retained explicit eight-slot offscreen mode, and replaced unconditional multi-megabyte per-slot rings with measured initial capacities plus retained geometric growth.
- 2026-07-13: added C26 changed-record transform/opacity property uploads through a separate completion-protected ring and exposed logical property counters.
- 2026-07-14: streamed scalable flat analytic primitive arrays through completion-protected frame rings, removed 4 KiB draw chunking, and added exact instance-byte/bind/growth telemetry.
- 2026-07-12: made effect targets pass-plan-lazy, removed the unused full-size blur texture, and added production memory-pressure purging.
- 2026-07-12: replaced independent layer-cache prescan/lowering decisions with one generation-based plan per nesting range, single-owner body rendering, same-size texture reuse, nested invalidation propagation, and explicit ownership counters.
- 2026-07-12: added snapshot-feature raw color-target readback for exact BGRA8, 4x MSAA resolve, and packed BGRA10_XR correctness goldens.

- 2026-07-12: completed saturating logical/allocated resource accounting and frame-work counters, including previously omitted depth, bloom, ID-mask, Scene3D mesh, layer, argument-buffer, and frame-ring storage.
- 2026-07-12: moved solid uniform selection to the vertex stage and enabled interpolated packed vertex colors without changing draw or upload counts.

- 2026-06-01: added a renderer source-contract gate that keeps `wait_until_completed` confined to explicit readback helpers and out of frame hot paths.
- 2026-05-25: shared layer-sublist geometry offset/rebase handling between image meshes and glyph runs.
- 2026-05-25: reused the existing unindexed-vertex primitive selector for Metal image meshes so four-vertex quads encode as triangle strips instead of incomplete triangle lists.
- 2026-05-30: Aligned optimized full-range NV12 camera shader chroma handling with the BGRA benchmark reference.
- 2026-05-22: Shared the Metal API vertex descriptor across solid, image-mesh, text, and SDF text PSO setup.
- 2026-05-18: Compact ID-mask render-target reuse and shared the clear/store setup used by raster and field passes.
- 2026-05-31: removed the `NativeCameraPreview` draw marker from the product renderer path so visible camera preview composition remains Oxide-owned.
- 2026-05-15: Shared overlay color-target attachment setup between ID-mask and neon-marker encoders.
- 2026-05-14: Shared scene3d mesh validation, buffer upload, and handle insertion between position-only and colored mesh uploads.
- 2026-04-23: Added the reusable retained `scene3d` mesh pass with depth buffering and same-frame 2D overlay interop.
- 2026-04-25: Removed the temporary normalized-index allocation from Solid and GlyphRun Metal uploads.
- 2026-04-25: Wired scene3d bloom PSO initialization and documented the dedicated bloom composite path.
- 2026-04-25: Batched consecutive rounded rectangles through the existing instanced UI shader path.
- 2026-04-25: Replaced inline layer fallback sublist cloning with range-aware draw-list encoding.
- 2026-04-25: Collapsed repeated frame metadata scans for camera and visual-effect decisions.
- 2026-04-25: Reused renderer-retained scratch across remaining small batch encode paths and made damage prefiltering borrow geometry backing storage instead of cloning it.
- 2026-04-26: Generalized in-app Metal GPU timing from camera direct preview to normal renderer frame submissions.
- 2026-04-30: Routed Scene3D bloom payloads through the offscreen blur/composite encoder and fixed the Scene3D material shader padding ABI.
- 2026-05-13: Rejected empty ID-mask dimensions and invalid GPU raster input before Metal texture work.
- 2026-05-13: Reused the shared scene3d matrix multiply and collapsed repeated Metal alpha/additive blend-state setup plus duplicate ID-mask halo scans.
- 2026-05-13: Removed the legacy CPU ID-mask upload path and pooled GPU ID-mask render targets plus raster vertex uploads in the renderer frame ring.
- 2026-05-13: Moved ID-mask GPU compositor and neon-marker encode internals into focused modules without changing the public renderer API.
