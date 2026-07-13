# renderer-web::lib

## Intention and purpose

`oxide-renderer-web` provides the WebAssembly renderer backend for Oxide. It exists so the existing renderer-agnostic `DrawList` contract can run inside a browser without introducing a new scene API or coupling UI code to DOM details.

## Relation to the rest of the code

The crate depends on `oxide-renderer-api` for draw commands and renderer traits. Production browser hosts create a `BrowserRenderer` asynchronously, which requires `WebGpuRenderer` and returns `RenderError::Unsupported` when browser WebGPU is unavailable. Hosts upload images through renderer-owned image methods, then feed Oxide draw lists into `Renderer::encode_pass`.

Call flow:

- Web host frame loop
- `oxide_test_scenes::Router::draw`
- `oxide_ui_core::DrawListBuilder`
- `oxide_renderer_web::BrowserRenderer::from_canvas_id_webgpu`
- `oxide_renderer_web::BrowserRenderer::begin_frame`
- `oxide_renderer_web::BrowserRenderer::encode_pass`
- WebGPU pipelines
- `oxide_renderer_web::BrowserRenderer::submit`

## Entry points list

- `oxide_renderer_web::BrowserRenderer::from_canvas_id_webgpu(id: &str) -> Future<Result<Self, RenderError>>`: async production constructor that initializes WebGPU and returns `Unsupported` if the browser cannot provide it.
- `oxide_renderer_web::BrowserRenderer::backend_name(&self) -> &'static str`: returns `webgpu` for browser smoke/perf reports.
- `oxide_renderer_web::WebGpuRenderer::from_canvas_id(id: &str) -> Future<Result<Self, RenderError>>`: wasm-only WebGPU constructor.
- `oxide_renderer_web::BrowserRenderer::canvas(&self) -> HtmlCanvasElement`: returns the backing canvas wrapper for host integration.
- `oxide_renderer_web::BrowserRenderer::last_stats(&self) -> WebRendererStats`: exposes the most recent frame counters.
- `oxide_renderer_web::WebGpuTimestampSample`: stores one completed frame's pass count and total/per-family WebGPU timestamp durations.
- `oxide_renderer_web::WebGpuCpuSubmitTimingSample`: stores optional high-resolution upload, surface acquisition, encoder creation, command encoding, timestamp-readback, scratch-accounting, queue-submit, present, and timestamp-map CPU durations for one explicitly profiled WebGPU submit.
- `oxide_renderer_web::BrowserRenderer::set_timestamp_readback_interval_for_benchmark(&mut self, frames: u64)`: selects bounded timestamp sampling cadence for explicit measurement; normal production cadence remains every eight frames.
- `oxide_renderer_web::BrowserRenderer::set_memory_stats_interval_for_benchmark(&mut self, frames: u64)`: selects the bounded resident-resource sampling cadence for explicit measurement; normal production cadence remains every 60 frames.
- `oxide_renderer_web::BrowserRenderer::set_memory_stats_enabled_for_benchmark(&mut self, enabled: bool)`: enables or disables resident-resource scans for an accounting-overhead control without changing rendering.
- `oxide_renderer_web::BrowserRenderer::clear_completed_timestamp_samples(&mut self)`: clears completed measurement samples before a declared workload.
- `oxide_renderer_web::BrowserRenderer::drain_completed_timestamp_samples_into(&mut self, output: &mut Vec<WebGpuTimestampSample>)`: harvests and drains completed samples into caller-owned reusable storage.
- `oxide_renderer_web::BrowserRenderer::set_cpu_submit_timing_enabled_for_benchmark(&mut self, enabled: bool)`: enables bounded submit-stage timing only around an explicit profiled frame; normal production submission leaves it disabled.
- `oxide_renderer_web::BrowserRenderer::last_cpu_submit_timing(&self) -> WebGpuCpuSubmitTimingSample`: returns the most recently collected submit-stage sample.
- `oxide_renderer_web::BrowserRenderer::image_create_rgba8(&mut self, width: u32, height: u32, data: &[u8], row_bytes: usize) -> ImageHandle`: uploads an RGBA texture to WebGPU.
- `oxide_renderer_web::BrowserRenderer::image_update_rgba8(&mut self, handle: ImageHandle, x: u32, y: u32, width: u32, height: u32, data: &[u8], row_bytes: usize) -> Result<(), RenderError>`: updates an RGBA texture subrectangle on WebGPU.
- `oxide_renderer_web::BrowserRenderer::image_release(&mut self, handle: ImageHandle) -> bool`: invalidates an uploaded image handle and releases its renderer-owned texture and bind group; returns whether the handle was live.
- `oxide_renderer_web::BrowserRenderer::image_create_a8(&mut self, width: u32, height: u32, data: &[u8], row_bytes: usize) -> ImageHandle`: uploads a glyph atlas as a single-channel `R8Unorm` WebGPU texture.
- `oxide_renderer_web::BrowserRenderer::image_update_a8(&mut self, handle: ImageHandle, x: u32, y: u32, width: u32, height: u32, data: &[u8], row_bytes: usize)`: updates an `R8Unorm` atlas subrectangle with one byte per texel.
- `oxide_renderer_web::BrowserRenderer::encode_neon_markers(&mut self, pass: &neon_marker::NeonMarkerPass<'_>) -> Result<(), RenderError>`: encodes the generic neon marker overlay path into the current WebGPU frame by lowering bounded markers to rounded-rect draw work.
- `impl oxide_renderer_api::Renderer for BrowserRenderer`: provides device caps, frame begin, WebGPU draw-list encoding, submit, and resize.
- `oxide_renderer_web::color_to_css(color: Color) -> String`: converts Oxide colors to Canvas2D CSS strings.
- `oxide_renderer_web::packed_rgba_to_css(rgba: u32) -> String`: converts packed vertex color to CSS.
- `oxide_renderer_web::color_cache_key(color: Color) -> u32`: returns the glyph tint cache key.
- `oxide_renderer_web::sanitize_scale(scale: f32) -> f32`: normalizes invalid device scales.

## Logic narrative

Solid WebGPU lowering preserves packed vertex color only on solid command paths; image and text tint behavior remains unchanged. The Canvas fallback leaves all-zero spans on its existing triangle walker, while nonzero spans accept only unindexed six-vertex axis-aligned quads with flat or opposing-edge colors and render them with one fill or one linear gradient. Unsupported colored topology emits no draw rather than a false uniform result.

The legacy Canvas2D renderer stores one visible canvas context plus offscreen canvases for uploaded images, layers, and sampled backdrops. It remains in the crate for diagnostics and native test coverage, but it is not re-exported as a wasm public API and is not reachable from `BrowserRenderer` or the web host production startup path.

The WebGPU renderer owns the browser surface, prebuilt persistent pipelines, one persistent sampler, persistent present buffers, reusable frame vertex/index buffers, reusable per-primitive scratch buffers, reusable image-upload scratch storage, renderable scene/scratch textures, retained layer textures, and texture bind groups for uploaded images. It initializes the browser surface with premultiplied alpha so transparent clears can composite with DOM content behind the canvas. It initializes all static WGSL render pipelines for solid geometry, RGBA images, A8 glyph masks, SDF glyph masks, Scene3D, ID-mask compositing, and sampled backdrop effects during WebGPU construction instead of first draw use. Generic 2D lowering appends directly into retained 20-byte POD vertices, segmented u16 indices, and a u32 large-mesh fallback; checked `bytemuck` views upload those streams without frame-level byte reserialization. Frames without backdrop sampling draw directly to the WebGPU surface in one render pass; a benchmark-only switch can force the older scene-texture plus present-pass route for A/B proof. Frames with backdrop/effect commands render through the scene texture, copy to scratch only at effect boundaries, then present the scene texture to the surface. WebGPU layer markers support retained clean-layer reuse by layer id, rect, surface size, and scale: dirty layers render into explicit offscreen textures, while clean cache hits skip the body and composite the retained texture. The current texture allocation is full-surface to preserve existing global-coordinate, clip, and effect parity; rect-sized retained textures remain a future optimization. When the browser adapter supports `TIMESTAMP_QUERY`, the renderer enables it up front, writes begin/end timestamps on render passes, resolves them after frame encoding, maps timestamp readback buffers asynchronously every 8 submitted frames, and harvests completed samples without blocking the frame loop. Draw-pass encoding merges adjacent compatible same-target/same-clip draw items over contiguous same-format, same-base index ranges, then caches the currently bound pipeline, image/effect bind group, scissor rectangle, and index format so compatible work keeps visual order while avoiding redundant WebGPU calls. Effect uniforms are recorded while backdrop draws are appended; identical effect parameters share one 16-byte upload and mixed parameters use dynamic uniform offsets so the renderer preserves correctness without issuing a queue write per backdrop. A8 and RGBA subresource updates reuse renderer-owned upload scratch and write textures directly from that buffer; the benchmark-only legacy mode keeps the old per-update temporary allocation path for A/B proof. `WebRendererStats` records frame shape, draw families, draw-item count, coalesced draw-item count, draw pipeline binds, draw bind-group binds, draw scissor sets, image-mesh draws, nine-slice draws, SDF glyph quads, layer cache hits/misses/skipped draws/passes, Scene3D/ID-mask/effect/camera family counts, effect uniform writes/bytes/slots, total render-pass count, pass-family counts for clear, draw, Scene3D, Scene3D overlay, ID-mask raster/field/compositor, present, texture-copy count, command-buffer count, timestamp-query support, collected timestamp frame id/pass count/family nanoseconds/max pass nanoseconds/readback skips/readback interval, upload bytes, image-upload temp allocation bytes/counts, image-upload scratch capacity/growth, aggregate GPU resource creation, GPU resource creation by draw/image/target/layer/Scene3D/effect/ID-mask family, aggregate CPU scratch capacity growth, draw/Scene3D/effect/ID-mask/image-upload/resource-table CPU scratch capacity growth, mesh creation, sampler creation, and runtime pipeline-creation violations so browser reports expose resource churn and pass attribution rather than only latency. Text rendering is supported through the normal `DrawCmd::GlyphRun` path because that command is replayed with the owning draw list's vertices and indices. The legacy standalone `RenderEncoder::draw_glyph_run(&GlyphRun)` callback is still insufficient by itself; renderer-agnostic replay now calls `draw_glyph_run_resolved` with resolved geometry. The diagnostic Canvas2D path shares one quad walker for indexed and unindexed image-mesh/glyph fallback drawing so both paths keep identical quad materialization rules.

Uploaded images occupy generation-checked renderer slots. The low 16 handle bits identify one of at most 65,535 slots and the high 16 bits identify that slot's generation. `image_release` removes the live value, advances the generation before recycling the slot, and drops the WebGPU texture and bind group. Stale handles therefore remain invalid after slot reuse, repeated release is harmless, and resource-table memory follows peak live image pressure instead of historical create count. A slot whose generation is exhausted is retired rather than permitting handle ABA.

WebGPU A8 and SDF resources use `R8Unorm`, upload one byte per texel, and sample `.r`; ordinary image resources remain `Rgba8Unorm`. Tight A8 rows pass directly to `Queue::write_texture`. Genuinely strided rows are repacked into single-channel storage because direct padded-row submission regressed Dawn cold-upload time; updates reuse renderer-owned scratch and cold creation allocates only the packed A8 payload. Residency and upload counters report destination texel bytes rather than source padding, so a 1024x1024 atlas accounts for 1,048,576 bytes and a 64x64 dirty update accounts for 4,096 bytes.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

Canvas constructors require a browser `window`, `document`, and 2D canvas context. WebGPU construction also requires `navigator.gpu`, a compatible adapter, a device, and a canvas WebGPU surface. Image uploads require enough row-strided bytes for the requested dimensions. Frame submission requires the token returned by the matching `begin_frame`. Image handle `0` is reserved as the invalid handle. A released handle is invalid immediately and must not be used for future updates or draws. The WebGPU resource table admits at most 65,535 simultaneous images and fails before creating a GPU resource when no reusable slot remains. The crate forbids unsafe code.

## Edge cases and failure modes

Colored Canvas solids reject indexed, malformed, skewed, non-finite, inconsistent-duplicate, and independently colored-corner shapes. Canvas color-stop failure also emits no draw.

Invalid scales collapse to `1.0`. Invalid image rows return `RenderError::InvalidOperation` on fallible APIs and `ImageHandle(0)` on host-uploader convenience APIs. Unknown and released images are skipped during drawing. Releasing handle `0`, an out-of-range handle, or an already released handle returns `false` without changing state. `CameraBg` is unavailable on web and is ignored by the web renderers. WebGPU initialization failure is reported as unsupported; the production browser renderer does not draw through Canvas2D.

## Concurrency and memory behavior

Browser renderers are intended for the browser main thread and store DOM/GPU handles only on wasm32. Native builds expose a small stub so macOS workspace checks can compile. The production WebGPU renderer reuses prebuilt pipelines, its sampler, texture bind groups, present buffers, scene/scratch textures, timestamp query/readback resources, ID-mask vertex-cache slots, generation-checked image slots, and grow-only frame vertex/index buffers after warmup. Releasing an uploaded image drops its texture and bind group without waiting on the CPU; wgpu retains any internal ownership needed by already submitted GPU work. Free-slot lookup and draw-time handle validation are constant-time vector operations without locks or hashing.

Resident accounting reports declared WebGPU texture extents and buffer sizes as logical bytes. Allocated bytes stay explicitly unavailable and zero because wgpu does not expose driver allocation, heap padding, or residency. Vertex/index/uniform buffers, present assets, transient color/depth targets, retained layers, ID-mask fields, atlas/image textures, Scene3D meshes, and timestamp staging buffers reconcile into one saturating logical total. Resource-table scans are sampled once every 60 frames and the cached snapshot is copied into ordinary frame stats in constant time; benchmark controls can select a one-frame cadence or disable the scan. Frame-work counters separately expose command traversal/copying, copied geometry, ID-mask chunk reuse/rebuild, cache outcomes, pass/encoder counts, texture-copy pixels/bytes, uploads, shaded pixels, submissions, evictions, and resource creation/growth.

The `snapshot-tests` feature adds asynchronous COPY_SRC readback for exact R8 city/neighborhood masks and final RGBA16F city/seam fields. Readback buffers use 256-byte padded rows and are absent from normal renderer builds.

## Performance notes

Metal-independent web draw counts, pipeline state, and upload sizes remain unchanged. WebGPU adds bounded packed-byte decoding per solid vertex; the Canvas colored path classifies six vertices and creates one gradient only when its endpoints differ.

Draw complexity is linear in draw-list commands plus emitted solid triangles, image meshes, nine-slices, glyph quads, dirty layer bodies, Scene3D instances, and ID-mask passes. WebGPU reduces browser draw-call overhead by batching geometry into frame buffers, coalescing adjacent compatible draw items, caching draw-pass state across adjacent compatible items, drawing no-effect frames directly to the surface, skipping clean retained layer bodies, and using shader paths for A8/SDF glyphs, Scene3D, ID-mask compositing, and sampled backdrop effects. Solid spans, image meshes, nine-slices, glyph runs, rounded rectangles, ID-mask vertex serialization, ID-mask uniform serialization, and browser image/glyph subresource uploads reuse renderer-owned scratch storage during lowering so warm frames avoid temporary per-draw or per-upload vectors. Scene3D render passes iterate retained draw arrays by index instead of cloning draw lists before command encoding, and the browser report includes both a two-instance and a 96-instance reused-mesh versus recreate-mesh A/B row to guard mesh-buffer lifetime at small and stressed draw counts. Backdrop/effect commands normally pay a scene-texture copy before the effect pass; consecutive backdrops whose shader sampling regions do not overlap can share one scratch copy and draw pass, while overlapping backdrops keep per-backdrop copies for visual parity. The browser report includes a current ID-mask row for default coverage, current glyph/RGBA dirty-subrect upload rows after the slower default full-upload rows were retired, a current effect-uniform row that keeps the batched/shared uniform write, byte, slot, backdrop, texture-copy, pass, and timestamp counters after the slower default per-backdrop uniform-write row was retired, a current backdrop-batch row that keeps the coalesced copy/pass counters after the slower default per-backdrop-copy row was retired, a mixed text/image/effects current row that keeps the same image, glyph, layer, clip, damage, backdrop, visual-effect, spinner, state-bind, texture-copy, pass, and timestamp counters after the slower default legacy rebind/unbatched row was retired, a layer/damage/effects current row that keeps the nested layer, damage, image, glyph, backdrop, visual-effect, spinner, state-bind, texture-copy, pass, and timestamp counters after the slower default legacy rebind/unbatched row was retired, a clean-layer current row that draws the same retained layer workload under clean body-skip reuse after the slower default dirty rerender row was retired, a command-family current row that draws the generic `ImageMesh`, `NineSlice`, and SDF glyph workload while asserting zero web `CameraBg` work after the slower default legacy rebind row was retired, a glyph-run current row that draws the atlas-backed A8 and SDF `GlyphRun` workload after the slower default legacy rebind row was retired, a neon-marker current row that draws the same bounded generic marker overlay and gates marker-derived solid draw count plus cached pipeline/scissor counters after the slower default legacy rebind row was retired, and a direct-surface current row that draws the same no-effect image workload while preserving the one-pass no-scene-present route after the forced scene-texture plus present-pass row was retired. The ID-mask A/B, upload-scratch, draw-item coalescing, and draw-state cache A/B exports remain available for explicit diagnostics, but their default browser report rows were retired after same-workload A/B evidence showed current wins; the explicit clip-state and upload A/B exports were later retired after repeated startup/package A/B proof while renderer clip-depth and dirty-upload counters remain covered by broader/current rows. The report also persists browser startup timing and static wasm-bindgen package bytes, and the host script can write a non-default repeated startup/package report, so deleting non-default diagnostics later can be judged against page-init and artifact-size measurements instead of source-size intuition. The current glyph/RGBA upload rows persist dirty subrect texture writes and direct timestamp-query totals for the rendered pass, the current effect-uniform row gates one batched/shared uniform upload with direct WebGPU timestamp totals separately from browser p50, the current backdrop-batch row reports the shared scene-copy pass that beat the legacy per-backdrop copy path on separated equivalent backdrops, and the current direct-surface row reports one draw pass, zero clear/present passes, zero texture copies, and direct timestamp totals for the retained no-effect surface route. Resource counters expose whether a timing change came with extra uploads, effect uniform writes/bytes/slots, image-upload temp allocations, image-upload scratch growth, image meshes, nine-slices, glyph runs, SDF glyphs, layer markers, layer cache hits/misses/skipped draws/passes, Scene3D draws, ID-mask draws, web camera-background draw violations, effect draws, draw items coalesced, draw-state binds, clip depth, scissor sets, passes, texture copies, direct timestamp-query nanoseconds, command buffers, GPU buffer growth, CPU scratch growth, mesh creation, texture creation, bind-group creation, sampler creation, or runtime pipeline-creation violations. Persisted browser numbers are collected from release wasm builds, and the report now includes a 15-entry backend-path coverage matrix that ties every important default WebGPU path family to its distribution rows and explanatory counters, a GPU timestamp stage-breakdown summary that reconciles clear/draw/Scene3D/ID-mask/present pass-family nanoseconds with every source row, a Rust/WASM allocation-audit summary for current warm rows, a current-row allocation-invariance summary that fails if any checked backend path allocates beyond the shared submit-boundary profile, a frame-loop allocation stage summary for host/router/renderer attribution, and a renderer submit sub-stage allocation summary that reconciles surface/view acquisition, command-buffer finish/queue submit, timestamp mapping, and zero-allocation renderer-side upload/render/present stages with the parent submit allocation total. The traced duplicate report run must include browser User Timing labels plus per-benchmark trace intervals with positive scoped event, GPU-event, and WebGPU/Dawn-event counts for every default benchmark family so trace evidence is tied to the same workload phases as the persisted rows.

The host script can also write a non-default Canvas indexed-quad diagnostic report for same-workload A/B proof around the shared Canvas fallback quad walker before changing `draw_vertex_quads`.

## Feature flags and cfgs

The real backend is compiled only on `target_arch = "wasm32"`. Non-wasm builds expose `WebRenderer::new_for_tests` and a `Renderer` implementation that returns `RenderError::Unsupported` from `submit` so native tests can inspect shared helpers without exposing a browser Canvas2D visual path. The native stub mirrors the web `CameraBg` boundary by treating that command as zero draw work.

The typed Canvas path enables the `web-sys/CanvasGradient` feature.

## Testing and benchmarks

`oxide/crates/renderer-web/tests/lib_tests.rs` covers color conversion, scale normalization, layer sizing, native stub behavior including zero-work `CameraBg`, WebGPU-only public wasm exports, premultiplied-alpha surface configuration, generation-checked image-handle reclamation without append-only tombstones, direct-surface submission and its benchmark-only forced scene-present toggle, present-buffer caching, eager static pipeline initialization, revision-keyed and slot-reused ID-mask vertex caching, draw-item coalescing wiring and counters, draw-state and clip-state cache wiring and counters, scratch-storage reuse for hot draw lowering, direct image-upload scratch writes, effect-uniform batching/dynamic-offset wiring, backdrop batch planning, retained clean-layer counters and metric exposure, ID-mask uniforms, RGBA subresource update exposure, resource-counter wiring for image meshes, nine-slices, SDF glyphs, layer markers/cache, Scene3D, ID-mask, backdrop/effect/spinner/camera families, uploads, passes, timestamp-query resources/readback collection, aggregate and family-level CPU scratch growth, aggregate and family-level GPU resource creation, sampler lifetime, mesh creation, runtime object counters, and no-clone Scene3D render iteration. Browser pixel tests run through `oxide-host-web` after wasm-bindgen packaging, and the current browser WebGPU baseline is persisted in `oxide/benchmarks/web/latest.json` plus `oxide/benchmarks/web/latest.md` with browser startup/package evidence, mixed-scene current coverage, layer/damage/effects current coverage, clean-layer current reuse, command-family current coverage, glyph-run current, neon-marker current coverage, direct-surface current coverage, current upload rows with direct glyph/RGBA timestamp totals, effect-uniform, current backdrop-batch, report-level and per-row warm-resource-churn zero-growth rows including GPU resource and CPU scratch family attribution, explicit backend-path coverage rows, current-row Rust/WASM allocation counters with bounded per-frame budgets, zero reallocations, shared allocation-signature invariance across every checked current row, frame-loop stage allocation attribution, zero WASM memory growth plus Chrome JS heap sampling across benchmark marks after prewarm, in-app WebGPU timestamp attribution, and Chrome browser trace summaries plus per-benchmark User Timing labels and scoped trace-event attribution captured from a duplicate benchmark-report run while timing rows remain from the untraced baseline run.

The non-default Canvas indexed-quad report path is intentionally separate from the committed WebGPU baseline and exists only to prove Canvas fallback changes on the exact indexed `ImageMesh` workload.

## Examples

```rust
pub async fn build_renderer() -> Result<oxide_renderer_web::BrowserRenderer, oxide_renderer_api::RenderError>
{
   oxide_renderer_web::BrowserRenderer::from_canvas_id_webgpu("oxide").await
}
```

## Changelog

- 2026-07-12: compacted generic WebGPU vertices from 32 to 20 bytes, added segmented u16 plus u32 fallback indices, and directly uploaded retained POD streams.
- 2026-07-12: stored WebGPU A8/SDF atlases as `R8Unorm`, removed A8-to-RGBA upload conversion, sampled `.r`, and added padded-row, byte-accounting, browser glyph-golden, and cold/full/dirty diagnostic coverage.

- 2026-07-12: replaced shared mutable ID-mask uniforms with one reusable aligned frame arena, immutable dynamic offsets per raster/seed/jump/compositor pass, one queue upload, and uniform write/byte/slot counters.
- 2026-07-12: added sampled, saturating WebGPU resident-memory snapshots and complete frame-work/report counters with explicit logical-versus-allocated semantics.
- 2026-07-12: added snapshot-only asynchronous ID-mask raster/final-field readback for CPU-reference parity.
- 2026-07-12: added packed solid-color WebGPU lowering and the narrow six-vertex Canvas flat/opposing-edge gradient path.
- 2026-07-10: replaced append-only WebGPU image tombstones with a constant-time generation-checked slot arena that reclaims metadata without stale-handle ABA.
- 2026-07-09: added explicit, idempotent browser image release so Rust-owned runtime asset lifetimes reclaim WebGPU textures and bind groups.
- 2026-06-22: retired the default browser WebGPU neon-marker legacy-rebind row after same-workload A/B proof while keeping current marker-overlay coverage and counters.
- 2026-06-22: retired the default browser WebGPU effect-uniform per-backdrop uniform-write row after same-workload A/B proof, keeping current batched uniform coverage and direct GPU timestamp totals.
- 2026-06-22: retired the default browser WebGPU backdrop-batch per-copy row after same-workload A/B proof while keeping current coalesced copy/pass coverage.
- 2026-06-22: configured browser WebGPU surfaces with premultiplied alpha so transparent clears can reveal DOM content behind embedded canvases.
- 2026-06-02: added WebGPU adjacent draw-item coalescing, counters, and browser current-versus-uncoalesced A/B coverage.
- 2026-06-02: made the native web test stub mirror zero-work `CameraBg` behavior and documented web camera background as unavailable.
- 2026-06-22: retired the default browser WebGPU clean-layer dirty rerender row after same-workload A/B proof while keeping current retained-layer cache coverage.
- 2026-06-02: added retained clean-layer reuse in WebGPU plus initial browser comparison coverage before the dirty row was later retired.
- 2026-06-22: retired the default browser WebGPU mixed text/image/effects legacy rebind/unbatched row after same-workload A/B proof; current mixed coverage remains.
- 2026-06-02: added browser WebGPU mixed text/image/effects current-versus-legacy-rebind/unbatched A/B coverage.
- 2026-06-02: moved all static WebGPU render pipelines to construction-time initialization and added a source gate against lazy frame-path pipeline creation.
- 2026-06-02: added family-level WebGPU GPU resource counters and warm-resource report gates for draw, image, target, Scene3D, effect, and ID-mask resource churn.
- 2026-06-02: added family-level WebGPU CPU scratch capacity/growth counters and warm-resource report gates for draw, Scene3D, effect, ID-mask, image-upload, and resource-table storage.
- 2026-06-02: added browser WebGPU layer/damage/effects current-versus-legacy-rebind/unbatched A/B coverage.
- 2026-06-22: retired the default browser WebGPU command-family legacy-rebind row after same-workload A/B proof, keeping current generic `ImageMesh`, `NineSlice`, SDF glyph, and zero web `CameraBg` coverage.
- 2026-06-02: added browser WebGPU command-family current-versus-legacy-rebind A/B coverage before the default legacy row was retired.
- 2026-06-22: retired the default browser WebGPU glyph-run legacy-rebind row after current-row A/B proof, keeping current atlas-backed A8 and SDF text draw coverage.
- 2026-06-02: added browser WebGPU neon-marker current-versus-legacy-rebind A/B coverage for the generic marker overlay path.
- 2026-06-02: added browser WebGPU direct-surface current-versus-forced-scene-present A/B coverage.
- 2026-06-22: retired the default browser WebGPU direct-surface forced-scene-present row after current direct-surface submission proved lower-pass and lower-GPU-time on the same workload.
- 2026-06-02: added per-benchmark Chrome trace interval attribution to the browser WebGPU report.
- 2026-06-02: added browser WebGPU backend-path coverage matrix enforcement.
- 2026-06-02: added browser WebGPU timestamp stage-breakdown report enforcement.
- 2026-06-02: added browser WebGPU Rust/WASM frame allocation audit counters and current-row allocation budget gates.
- 2026-06-02: added browser WebGPU submit sub-stage WASM allocation attribution.
- 2026-06-02: added browser WebGPU frame-loop WASM allocation stage attribution.
- 2026-06-02: sampled WebGPU timestamp readbacks every 8 frames while keeping pass timestamp writes and report interval coverage.
- 2026-06-22: retired the default browser WebGPU upload legacy rows and upload A/B export after same-workload A/B proof; the version 5 browser report keeps current glyph/RGBA upload rows with direct timestamp totals.
- 2026-06-02: added direct timestamp-total fields to the browser WebGPU glyph/RGBA upload A/B summary.
- 2026-06-01: added WebGPU effect-uniform batching, shared same-parameter uploads, dynamic-offset mixed-effect support, counters, and browser A/B coverage.
- 2026-06-01: added direct GPU timestamp-total fields to the WebGPU effect-uniform A/B report.
- 2026-06-01: added per-benchmark browser User Timing marks to the WebGPU report and Chrome trace contract.
- 2026-06-01: added conservative WebGPU backdrop copy/pass coalescing plus browser A/B coverage for coalesced versus per-backdrop copies.
- 2026-06-01: added reusable WebGPU image-upload scratch storage, direct scratch texture writes, temp-allocation counters, and current-versus-legacy upload-scratch A/B coverage.
- 2026-06-22: retired default browser upload-scratch report rows after same-workload A/B proof; kept the explicit diagnostic export and scratch counters.
- 2026-06-22: added browser startup and package-size report evidence for future explicit diagnostic cleanup A/B tests.
- 2026-06-22: added non-default repeated startup/package report support for explicit diagnostic cleanup A/B tests.
- 2026-06-22: added non-default Canvas indexed-quad report support for same-workload Canvas fallback A/B tests.
- 2026-06-01: added browser WebGPU draw-state cache counters and current-versus-legacy-rebind A/B coverage.
- 2026-06-01: added browser WebGPU clip-state cache A/B coverage for nested `ClipPush`/`ClipPop` scissor runs.
- 2026-06-22: retired standalone draw-item coalescing, draw-state cache, and clip-state cache rows from the default browser report after same-workload A/B proof showed current wins.
- 2026-06-22: retired the explicit clip-state diagnostic export after repeated startup/package A/B proof while keeping renderer clip-depth counters covered by broader rows.
- 2026-06-22: retired the default browser WebGPU layer-effects legacy row after same-workload A/B proof while keeping current layer/damage/effects coverage and counters.
- 2026-06-22: retired the default browser ID-mask legacy row after same-workload A/B proof while keeping current ID-mask coverage and the explicit diagnostic export.
- 2026-06-01: added browser WebGPU command-family report coverage and counters for generic `ImageMesh`, `NineSlice`, SDF glyph, and zero web `CameraBg` work.
- 2026-06-01: added dedicated browser WebGPU layer/damage/effects report coverage for nested layer markers, multiple damage rects, backdrop copies, and timestamped passes.
- 2026-06-01: added a 96-instance browser WebGPU Scene3D stress A/B row for retained-mesh resource proof.
- 2026-06-01: added nonblocking browser WebGPU timestamp-query collection and persisted pass-family nanosecond buckets.
- 2026-06-01: added browser-gated WebGPU sampler creation counters and a static startup-only sampler guard.
- 2026-06-01: added browser-gated WebGPU CPU scratch growth counters and reused ID-mask vertex-cache slots across revision churn.
- 2026-06-01: added direct WebGPU counters for Scene3D, ID-mask, backdrop, visual-effect, spinner, zero web camera-background work, and mesh creation paths.
- 2026-06-01: added browser WebGPU Scene3D reused-mesh versus recreate-mesh A/B coverage for resource lifetime proof.
- 2026-06-01: moved WebGPU solid/image-mesh/glyph/rounded-rect lowering onto reusable scratch buffers and removed Scene3D draw-list clones during render passes.
- 2026-06-01: moved WebGPU ID-mask raster/compositor uniform serialization onto reusable scratch buffers.
- 2026-06-01: added WebGPU resource counters for render passes, command buffers, upload bytes, buffer growth, texture creation, bind-group creation, and runtime pipeline-creation detection.
- 2026-06-01: exposed WebGPU RGBA texture subresource updates for browser image-upload A/B coverage.
- 2026-06-01: made WebGPU inline layer-marker handling explicit through `layer_draws` counters.
- 2026-06-01: split WebGPU render-pass reporting into pass-family counters and texture-copy attribution.
- 2026-05-25: shared Canvas2D fallback quad walking between image meshes and glyph runs.
- 2026-05-25: expanded unindexed four-vertex WebGPU draw geometry into six triangle-list indices so image meshes and glyph quads render as complete quads.
- Compacted repeated WebGPU non-indexed vertex expansion across solid, image-mesh, and glyph encoding.
- Compacted WebGPU render-target, depth-target, and ID-mask texture creation through one 2D texture descriptor helper.
- Compacted row-strided image copying and Canvas2D fallback camera/backdrop helper branches without changing the public WebGPU startup contract.
- Added browser pixel verification and persisted Canvas2D wasm baseline coverage through `oxide-host-web`.
- Hard-cut production browser rendering to WebGPU only; unsupported browsers now fail construction instead of drawing through Canvas2D.
- Added async `BrowserRenderer` WebGPU selection, `WebGpuRenderer`, shader-backed A8/SDF/effect paths, and geometry-aware glyph replay.
- Added offscreen layer compositing and sampled backdrop blur.
- Added the initial Canvas2D WebAssembly renderer backend.
