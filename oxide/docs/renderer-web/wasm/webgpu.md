# renderer-web `wasm/webgpu.rs`

## Intention and purpose

Lower Oxide draw lists into persistent wgpu/WebGPU buffers, pipelines, passes, and surface submissions.

## Relation to the rest of the code

Consumes renderer-api values and lowers generic 2D geometry through `packed_geometry`; embedded WGSL interpolates the normalized packed vertex color.

## Entry points list

- `BrowserRenderer::prewarm_auxiliary_targets` lets an app move the allocation of only its declared backdrop and/or Scene3D targets outside a latency-sensitive first frame.
- `BrowserRenderer::set_timestamp_readback_interval_for_benchmark`, `clear_completed_timestamp_samples`, and `drain_completed_timestamp_samples_into` control and collect bounded C00 GPU timestamp distributions without changing the normal eight-frame production sampling cadence.
- `BrowserRenderer::queue_completion_flag_for_benchmark` registers a benchmark-only completion fence used to serialize C01 primitive submissions before the next presented drawable.
- `BrowserRenderer::set_cpu_submit_timing_enabled_for_benchmark` and `last_cpu_submit_timing` expose bounded, opt-in CPU attribution for upload, surface, command encoding, queue submit, present, and readback bookkeeping; the normal renderer path retains only a disabled branch.
- `BrowserRenderer::encode_snapshot(&mut self, snapshot: &RenderSnapshot) -> Result<(), RenderSnapshotError>` prepares or replays immutable retained chunks and falls back to exact flattening when an instance or command is not supported by the prepared path.
- `BrowserRenderer::prepared_cache_resident_bytes(&self) -> u64`, `set_prepared_cache_budget_bytes`, and `purge_prepared_chunks` expose logical residency plus explicit cache policy and invalidation.
- `BrowserRenderer::set_prepared_bundle_min_draws_for_benchmark` and `advance_prepared_device_generation_for_benchmark` isolate C25 threshold and device-lifecycle guardrails; production keeps the measured threshold and renderer-owned device lifetime.
- Retained snapshot layer instances use the internal `PreparedLayerKey` path: clean generation/resource matches composite a persistent local-sized texture without entering the chunk body, while dirty or missing instances refresh that texture once.
- `encode_solid`, `gpu_vertex`, and the three `append_*gpu_vertices` helpers implement this boundary.

## Logic narrative

Solid lowering passes `preserve_vertex_color = true` for local-indexed, rebased-indexed, and unindexed spans. Image and glyph paths pass false to retain existing tint semantics. Nonzero API `AABBGGRR` bits are copied unchanged; zero inherits one quantized uniform color. Generic frame geometry is retained as 20-byte POD vertices plus segmented u16 and fallback u32 indices, exposed to `Queue::write_buffer` by checked `bytemuck` slice views without a second serialization vector. Each draw packet records its index format and base vertex, so adjacent compatible ranges coalesce only inside the same segment.

The surface is constructed at the canvas's already-selected physical backing size. Scene color, backdrop scratch, and Scene3D depth targets begin absent, are created by the first declared feature or explicit app prewarm, and are dropped when a physical resize invalidates their dimensions. Direct 2D surface rendering therefore owns none of those full-size targets. The viewport uniform is written at construction and when size or scale changes, not on every submission.

Explicit benchmark capture lazily allocates a 4,096-entry completed-sample FIFO, samples every frame, clears stale completed samples, and drains results into host-owned reusable storage. Normal production timestamp sampling does not allocate or populate that history. When an active capture reaches the bound, the oldest completed sample is discarded; pending GPU readbacks retain their existing completion-safe slot ownership.

Immutable zero-origin snapshot chunks are keyed by chunk id, structural/geometry/resource revisions, device generation, surface format, and bundle policy. A miss lowers only that chunk into persistent vertex/index buffers and an ordered prepared plan; capacity-compatible buffers are queue-updated in place. Full-surface static ranges record bundles, while clipped or otherwise bundle-incompatible ranges remain ordered direct segments over the same buffers. A wholly compatible snapshot additionally retains one aggregate bundle keyed by each chunk's buffer/plan generation, so clean frames issue one replay and one execute call without command traversal, geometry packing, or upload. Effects, camera input, unsupported mixed snapshots, missing resources, and zero cache budget use the checked flat path.

Retained snapshot layers add stable layer identity, full chunk revisions, content/nested/dynamic generations, bounds, transform scale, opacity, target scale, format/device generation, effect outset, physical pixel phase, and precise resource dependencies to that contract. A validated immutable snapshot retains its resolved layer frames and chunk references, so subsequent frames with the same snapshot identity do not recompute transforms, bounds, phases, duplicate IDs, or keys. Axis-aligned targets snap transformed minimum bounds down and maximum bounds up on the canvas physical-pixel grid, then allocate only that span, bounded by the device limit. The snapping preserves the parent raster sample phase at fractional origins while adding at most one edge pixel per axis. Each target owns a persistent viewport uniform whose origin maps existing coordinates into local pixels; the final composite uses normalized `[0, 1]` UVs. Nested targets inherit their ancestor matrix and translation but not opacity, derive an inverse composite rectangle, and therefore apply the transform and opacity exactly once. Local scissor conversion conservatively transforms internal clips before applying the established negative-origin extent rule, and effect copies move only the visible local target region into the sampling texture. A clean key/resource hit records the skipped body length immediately and emits only its composite. Dirty or missing content clears and redraws once; compatible dimensions reuse the texture and uniform binding. Canvas-size changes preserve compatible local layers, while scale, resource, purge, or device-generation changes invalidate them; plan ownership is also cleared when a lifecycle event can change its key space.

## Preconditions and postconditions

Indexed paths validate or rebase indices first. Generic shader locations remain unchanged; the color location is now `Unorm8x4` at byte 16 with a 20-byte stride. u16 writes are four-byte aligned at the stream tail, and large geometry retains a u32 fallback.

## Edge cases and failure modes

Invalid spans or indices clear scratch output and emit no draw. Packed zero exactly inherits the uniform.

An absent or released image dependency rejects preparation before encoding. Resize, scale change, device-generation change, explicit purge, budget eviction, and resource release invalidate affected prepared ownership. A positive budget protects the current plan while evicting least-recently-used unprotected chunks; if that plan cannot fit, the frame falls back instead of replaying a partial snapshot.

Non-finite, empty, rotated/sheared, dynamically clipped, unbounded-effect, or device-limit-exceeding retained layers use exact flattening/inline rendering. Oversized layers are never silently downscaled and never issue an invalid WebGPU texture request.

## Concurrency and memory behavior

Frame scratch vectors and typed packed streams retain capacity across frames. The change adds no resource or synchronization work after warmup and contains no handwritten unsafe cast.

Optional auxiliary texture handles retain wgpu's completion-safe internal ownership when the renderer drops or explicitly destroys its current resize-invalidated handle.

Prepared entries own their wgpu buffers, render bundles, lowered draw vectors, resource handles, and logical-byte accounting. The cache is browser-main-thread state with no locks; clean lookup is hash-table access per instance, while budget enforcement scans only when residency exceeds the configured limit. Bundle-referenced resources remain alive through cache or aggregate-bundle ownership until explicit invalidation.

C26 adds a three-slice dynamic-uniform property ring. Queue writes and render submissions share the WebGPU queue timeline, so reusing a physical slice remains ordered without a CPU wait. Each plan ordinal retains its last value revision per slice; adjacent changed records coalesce into one `queue.write_buffer` range. Dynamic instances keep persistent chunk buffers but use ordered direct draws because bundle commands cannot change dynamic offsets per replay.

## Performance notes

Draw count is unchanged. Generic vertex uploads fall from 32 to 20 bytes each, u16-eligible index uploads fall from four to two bytes each, and frame-level vertex/index reserialization is deleted. The C16 browser workload separately measures 10,000 glyph quads, 10,000 image quads, and a 70,002-vertex u32-fallback solid mesh while retaining direct GPU timestamp and visual evidence.

C19 measures construction resource count, direct/backdrop/Scene3D logical target bytes, resize creation work, explicit prewarm cost, first-feature submission, queue completion, and GPU time across fresh Chrome processes. A simple direct app leaves prewarm disabled and retains zero auxiliary-target bytes.

C25 measures 256 chunks and 7,680 mixed solid/image/A8/SDF draws. The retained eight-draw threshold plus 64-draw scene floor gives clean frames 256 hits, zero lowering/upload work, and one aggregate bundle execute. One dirty chunk leaves 255 hits and updates only 684 geometry bytes. Persistent residency is bounded by a 32 MiB logical-byte LRU; higher thresholds and recurring bundle/buffer recreation were rejected by the recorded tail gates.

C26 measures 300 retained text/image instances with alternating transform and opacity. After all ring slices warm, the candidate records 300 cache hits, zero command traversal/copy and geometry upload, 300 changed property records, and 14,400 logical property bytes per alternating frame. Full affine snapshots use the same prepared path; dynamic clip metadata resolves against its transform slot before scissor intersection.

C30 measures 100 retained 72×40-point cards at physical 1080p and 4K with DPR2. The expected local residency is 4,608,000 texture bytes at either canvas size, rather than 829,440,000 bytes at 1080p or 3,317,760,000 bytes at 4K for one full-canvas texture per card. Clean frames require 100 hits and zero body traversal; one-dirty frames require 99 hits, one miss, one local clear/draw pass, and no recurring texture creation. The fractional-edge capture combines transformed outer bounds, nested layers, clips, a backdrop effect, and a clean replay for exact parent/candidate pixel proof.

C31 bounds those local textures with an adaptive logical-byte budget derived from canvas area and a configurable override. Active entries record logical color bytes and last-used frame; compatible cold entries are recycled before allocation, while LRU entries not referenced by the current frame may be evicted. The pool is capped at one quarter of the budget and releases old entries after 60 frames; active layers absent for 120 frames move through the pool. Memory pressure, scale changes, and device loss purge storage and invalidate prepared plans. WebGPU exposes no driver allocation size, so allocated-byte availability remains false while C31 reports logical GPU bytes and exact renderer-owned CPU payload capacity.

C33 retains complete ID-mask raster and RGBA16F jump-flood fields under an adaptive 64–512 MiB logical-byte budget with at most four entries. Its exact key covers mask dimensions/scale, vertex revision/count, ordered chunk hashes/ranges, and every projection value; style, color, glow, polish, mode, opacity, and viewport placement remain compositor-only. Hits write one compositor uniform record and encode one pass. Projection/content changes rebuild all twelve 512-square passes. Queue submission order makes cross-frame target reuse safe, while uniform-arena growth rebuilds every target-specific bind group before reuse. Memory pressure, device loss, and explicit purge release cache ownership.

## Feature flags and cfgs

Compiled only for `wasm32` with the existing WebGPU and WGSL features.

## Testing and benchmarks

Native contract tests exercise decoding/source paths and freeze prepared-cache invalidation, aggregate/hybrid bundle ownership, dynamic property-ring ownership, generation/resource-complete local layers, device-bounded target dimensions, flat boundaries, hard admission, pooling, aging, purge paths, complete ID-mask field keys, compositor-only hits, compact hit uniforms, and counters; wasm compilation verifies the implementation. The C25 adapter retains static bundle proof, `scripts/run_webgpu_dynamic_c26.mjs` supplies C26 backend CPU/GPU/property and real-RAF samples, `scripts/run_webgpu_local_layers_c30.mjs` preserves C30 evidence plus its C31 navigation-churn mode, and `check_webgpu_browser_golden.mjs --id-mask-cache-only` runs the C33 static/invalidation/one-entry/LRU matrix with direct timestamp samples.

## Examples

Packed `0xFFFF_0000` uploads as opaque blue; packed zero uploads the draw uniform.

## Changelog

- 2026-07-14: added C33 complete WebGPU ID-mask field keys, compact compositor-only hits, bounded compatible LRU storage, direct stage timing, and explicit pressure/device purge behavior.
- 2026-07-14: added C31 adaptive/configurable logical-byte budgets, protected LRU admission, compatible layer pooling, absent/pool aging, exact over-budget fallback, purge reasons, and storage telemetry.
- 2026-07-14: added generation-correct retained WebGPU layers backed by physical-grid-snapped local textures, transform-inheriting nested viewports/scissors, local effect copies, exact resource invalidation, compatible resize reuse, and the C30 100-card proof path.
- 2026-07-13: added full-affine/opacity prepared instances, transform-linked dynamic clips, and a changed-record WebGPU property ring for C26.
- 2026-07-13: added revision/device-aware persistent prepared chunks, ordered bundle/direct segments, an aggregate static snapshot bundle, logical-byte LRU eviction, lifecycle invalidation, and C25 counters.
- 2026-07-13: made scene, scratch, and depth targets feature-driven; initialized the viewport only at construction/resize; and added selective app-controlled backdrop/Scene3D prewarm.
- 2026-07-12: replaced generic frame reserialization with directly uploaded 20-byte POD vertices, segmented u16 indices, and a correct u32 large-mesh fallback.
- 2026-07-12: exposed a benchmark-only queue completion flag for the opt-in C01 one-submit-per-RAF primitive matrix without changing normal submission behavior.
- 2026-07-12: added bounded per-frame timestamp history and caller-owned draining for C00 GPU distributions.
- 2026-07-12: added opt-in high-resolution WebGPU submit-stage CPU timing for the C00 one-submit-per-RAF harness.
- 2026-07-12: preserved packed colors on every solid lowering topology.
