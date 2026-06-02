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
- `oxide_renderer_web::BrowserRenderer::image_create_rgba8(&mut self, width: u32, height: u32, data: &[u8], row_bytes: usize) -> ImageHandle`: uploads an RGBA texture to WebGPU.
- `oxide_renderer_web::BrowserRenderer::image_update_rgba8(&mut self, handle: ImageHandle, x: u32, y: u32, width: u32, height: u32, data: &[u8], row_bytes: usize) -> Result<(), RenderError>`: updates an RGBA texture subrectangle on WebGPU.
- `oxide_renderer_web::BrowserRenderer::image_create_a8(&mut self, width: u32, height: u32, data: &[u8], row_bytes: usize) -> ImageHandle`: uploads a glyph atlas as a WebGPU alpha-mask texture.
- `oxide_renderer_web::BrowserRenderer::image_update_a8(&mut self, handle: ImageHandle, x: u32, y: u32, width: u32, height: u32, data: &[u8], row_bytes: usize)`: updates an atlas subrectangle on the WebGPU texture.
- `impl oxide_renderer_api::Renderer for BrowserRenderer`: provides device caps, frame begin, WebGPU draw-list encoding, submit, and resize.
- `oxide_renderer_web::color_to_css(color: Color) -> String`: converts Oxide colors to Canvas2D CSS strings.
- `oxide_renderer_web::packed_rgba_to_css(rgba: u32) -> String`: converts packed vertex color to CSS.
- `oxide_renderer_web::color_cache_key(color: Color) -> u32`: returns the glyph tint cache key.
- `oxide_renderer_web::sanitize_scale(scale: f32) -> f32`: normalizes invalid device scales.

## Logic narrative

The legacy Canvas2D renderer stores one visible canvas context plus offscreen canvases for uploaded images, layers, and sampled backdrops. It remains in the crate for diagnostics and native test coverage, but it is not re-exported as a wasm public API and is not reachable from `BrowserRenderer` or the web host production startup path.

The WebGPU renderer owns the browser surface, prebuilt persistent pipelines, one persistent sampler, persistent present buffers, reusable frame vertex/index buffers, reusable per-primitive scratch buffers, reusable image-upload scratch storage, renderable scene/scratch textures, and texture bind groups for uploaded images. It initializes all static WGSL render pipelines for solid geometry, RGBA images, A8 glyph masks, SDF glyph masks, Scene3D, ID-mask compositing, and sampled backdrop effects during WebGPU construction instead of first draw use. It encodes the draw list into a compact frame command stream and uploads contiguous vertex/index buffers. Frames without backdrop sampling draw directly to the WebGPU surface in one render pass. Frames with backdrop/effect commands render through the scene texture, copy to scratch only at effect boundaries, then present the scene texture to the surface. WebGPU layer markers are currently correctness-first inline groups: the backend renders layer bodies in order and records `layer_draws`, but does not claim clean-layer texture reuse yet. When the browser adapter supports `TIMESTAMP_QUERY`, the renderer enables it up front, writes begin/end timestamps on render passes, resolves them after frame encoding, maps readback buffers asynchronously after submit, and harvests completed samples without blocking the frame loop. Draw-pass encoding caches the currently bound pipeline, image/effect bind group, and scissor rectangle so adjacent compatible draw items keep visual order and draw count while avoiding redundant WebGPU state calls. Effect uniforms are recorded while backdrop draws are appended; identical effect parameters share one 16-byte upload and mixed parameters use dynamic uniform offsets so the renderer preserves correctness without issuing a queue write per backdrop. A8 and RGBA subresource updates reuse renderer-owned upload scratch and write textures directly from that buffer; the benchmark-only legacy mode keeps the old per-update temporary allocation path for A/B proof. `WebRendererStats` records frame shape, draw families, draw-item count, draw pipeline binds, draw bind-group binds, draw scissor sets, image-mesh draws, nine-slice draws, SDF glyph quads, Scene3D/ID-mask/effect/camera family counts, effect uniform writes/bytes/slots, total render-pass count, pass-family counts for clear, draw, Scene3D, Scene3D overlay, ID-mask raster/field/compositor, present, texture-copy count, command-buffer count, timestamp-query support, collected timestamp frame id/pass count/family nanoseconds/max pass nanoseconds/readback skips, upload bytes, image-upload temp allocation bytes/counts, image-upload scratch capacity/growth, aggregate GPU resource creation, GPU resource creation by draw/image/target/Scene3D/effect/ID-mask family, aggregate CPU scratch capacity growth, draw/Scene3D/effect/ID-mask/image-upload/resource-table CPU scratch capacity growth, mesh creation, sampler creation, and runtime pipeline-creation violations so browser reports expose resource churn and pass attribution rather than only latency. Text rendering is supported through the normal `DrawCmd::GlyphRun` path because that command is replayed with the owning draw list's vertices and indices. The legacy standalone `RenderEncoder::draw_glyph_run(&GlyphRun)` callback is still insufficient by itself; renderer-agnostic replay now calls `draw_glyph_run_resolved` with resolved geometry. The diagnostic Canvas2D path shares one quad walker for indexed and unindexed image-mesh/glyph fallback drawing so both paths keep identical quad materialization rules.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

Canvas constructors require a browser `window`, `document`, and 2D canvas context. WebGPU construction also requires `navigator.gpu`, a compatible adapter, a device, and a canvas WebGPU surface. Image uploads require enough row-strided bytes for the requested dimensions. Frame submission requires the token returned by the matching `begin_frame`. Image handle `0` is reserved as the invalid handle. The crate forbids unsafe code.

## Edge cases and failure modes

Invalid scales collapse to `1.0`. Invalid image rows return `RenderError::InvalidOperation` on fallible APIs and `ImageHandle(0)` on host-uploader convenience APIs. Unknown images are skipped during drawing. `CameraBg` is unavailable on web and is ignored by the web renderers. WebGPU initialization failure is reported as unsupported; the production browser renderer does not draw through Canvas2D.

## Concurrency and memory behavior

Browser renderers are intended for the browser main thread and store DOM/GPU handles only on wasm32. Native builds expose a small stub so macOS workspace checks can compile. The production WebGPU renderer reuses prebuilt pipelines, its sampler, texture bind groups, present buffers, scene/scratch textures, timestamp query/readback resources, ID-mask vertex-cache slots, and grow-only frame vertex/index buffers after warmup.

## Performance notes

Draw complexity is linear in draw-list commands plus emitted solid triangles, image meshes, nine-slices, glyph quads, Scene3D instances, and ID-mask passes. WebGPU reduces browser draw-call overhead by batching geometry into frame buffers, caching draw-pass state across adjacent compatible items, drawing no-effect frames directly to the surface, and using shader paths for A8/SDF glyphs, Scene3D, ID-mask compositing, and sampled backdrop effects. Solid spans, image meshes, nine-slices, glyph runs, rounded rectangles, ID-mask vertex serialization, ID-mask uniform serialization, and browser image/glyph subresource uploads reuse renderer-owned scratch storage during lowering so warm frames avoid temporary per-draw or per-upload vectors. Scene3D render passes iterate retained draw arrays by index instead of cloning draw lists before command encoding, and the browser report includes both a two-instance and a 96-instance reused-mesh versus recreate-mesh A/B row to guard mesh-buffer lifetime at small and stressed draw counts. Backdrop/effect commands normally pay a scene-texture copy before the effect pass; consecutive backdrops whose shader sampling regions do not overlap can share one scratch copy and draw pass, while overlapping backdrops keep per-backdrop copies for visual parity. The browser report includes a mixed text/image/effects A/B row pair that draws the same image, glyph, layer, clip, damage, backdrop, visual-effect, and spinner workload while comparing current state/effect batching against legacy rebinding/unbatched toggles, a layer/damage/effects A/B row pair that draws the same nested layer, damage, image, glyph, backdrop, visual-effect, and spinner workload while gating current state/effect/pass reductions against the legacy rebind/unbatched path, a command-family A/B row pair that draws the same generic `ImageMesh`, `NineSlice`, and SDF glyph workload while asserting zero web `CameraBg` work and gating current draw-state reductions against the legacy rebind path, the draw-state cache A/B rows compare the current state-cache path against a legacy rebind path on the same 1024-draw image workload, the clip-state cache A/B rows compare the same state-cache path on real nested `ClipPush`/`ClipPop` scissor runs, the glyph/RGBA upload A/B rows compare dirty subrect texture writes against full-texture writes and persist direct timestamp-query totals for the rendered pass, the effect-uniform A/B rows compare one batched/shared uniform upload against the legacy per-backdrop write path on equivalent backdrop work and now gate the direct WebGPU timestamp total separately from browser p50, the backdrop-batch A/B rows compare one shared scene-copy pass against a legacy per-backdrop copy path on separated equivalent backdrops, and the upload-scratch A/B rows compare reusable image-upload scratch against a benchmark-only temporary-allocation path on equivalent A8/RGBA dirty updates. Resource counters expose whether a timing change came with extra uploads, effect uniform writes/bytes/slots, image-upload temp allocations, image-upload scratch growth, image meshes, nine-slices, SDF glyphs, layer markers, Scene3D draws, ID-mask draws, web camera-background draw violations, effect draws, draw-state binds, clip depth, scissor sets, passes, texture copies, direct timestamp-query nanoseconds, command buffers, GPU buffer growth, CPU scratch growth, mesh creation, texture creation, bind-group creation, sampler creation, or runtime pipeline-creation violations. Persisted browser numbers are collected from release wasm builds, and the report now includes a backend-path coverage matrix that ties every important WebGPU path family to its distribution rows and explanatory counters. The traced duplicate report run must include browser User Timing labels plus per-benchmark trace intervals with positive scoped event, GPU-event, and WebGPU/Dawn-event counts for every benchmark family so trace evidence is tied to the same workload phases as the persisted rows.

## Feature flags and cfgs

The real backend is compiled only on `target_arch = "wasm32"`. Non-wasm builds expose `WebRenderer::new_for_tests` and a `Renderer` implementation that returns `RenderError::Unsupported` from `submit` so native tests can inspect shared helpers without exposing a browser Canvas2D visual path. The native stub mirrors the web `CameraBg` boundary by treating that command as zero draw work.

## Testing and benchmarks

`oxide/crates/renderer-web/tests/lib_tests.rs` covers color conversion, scale normalization, layer sizing, native stub behavior including zero-work `CameraBg`, WebGPU-only public wasm exports, direct-surface submission, present-buffer caching, eager static pipeline initialization, revision-keyed and slot-reused ID-mask vertex caching, draw-state and clip-state cache wiring and counters, scratch-storage reuse for hot draw lowering, direct image-upload scratch writes, effect-uniform batching/dynamic-offset wiring, backdrop batch planning, ID-mask uniforms, RGBA subresource update exposure, resource-counter wiring for image meshes, nine-slices, SDF glyphs, layer markers, Scene3D, ID-mask, backdrop/effect/spinner/camera families, uploads, passes, timestamp-query resources/readback collection, aggregate and family-level CPU scratch growth, aggregate and family-level GPU resource creation, sampler lifetime, mesh creation, runtime object counters, and no-clone Scene3D render iteration. Browser pixel tests run through `oxide-host-web` after wasm-bindgen packaging, and the current browser WebGPU baseline is persisted in `oxide/benchmarks/web/latest.json` plus `oxide/benchmarks/web/latest.md` with mixed-scene A/B, layer/damage/effects A/B, command-family A/B, draw-state cache, clip-state cache, upload A/B with direct glyph/RGBA timestamp totals, effect-uniform, backdrop-batch, upload-scratch, report-level and per-row warm-resource-churn zero-growth rows including GPU resource and CPU scratch family attribution, explicit backend-path coverage rows, zero WASM memory growth plus Chrome JS heap sampling across benchmark marks after prewarm, in-app WebGPU timestamp attribution, and Chrome browser trace summaries plus per-benchmark User Timing labels and scoped trace-event attribution captured from a duplicate benchmark-report run while timing rows remain from the untraced baseline run.

## Examples

```rust
pub async fn build_renderer() -> Result<oxide_renderer_web::BrowserRenderer, oxide_renderer_api::RenderError>
{
   oxide_renderer_web::BrowserRenderer::from_canvas_id_webgpu("oxide").await
}
```

## Changelog

- 2026-06-02: made the native web test stub mirror zero-work `CameraBg` behavior and documented web camera background as unavailable.
- 2026-06-02: added browser WebGPU mixed text/image/effects current-versus-legacy-rebind/unbatched A/B coverage.
- 2026-06-02: moved all static WebGPU render pipelines to construction-time initialization and added a source gate against lazy frame-path pipeline creation.
- 2026-06-02: added family-level WebGPU GPU resource counters and warm-resource report gates for draw, image, target, Scene3D, effect, and ID-mask resource churn.
- 2026-06-02: added family-level WebGPU CPU scratch capacity/growth counters and warm-resource report gates for draw, Scene3D, effect, ID-mask, image-upload, and resource-table storage.
- 2026-06-02: added browser WebGPU layer/damage/effects current-versus-legacy-rebind/unbatched A/B coverage.
- 2026-06-02: added browser WebGPU command-family current-versus-legacy-rebind A/B coverage.
- 2026-06-02: added per-benchmark Chrome trace interval attribution to the browser WebGPU report.
- 2026-06-02: added browser WebGPU backend-path coverage matrix enforcement.
- 2026-06-02: added direct timestamp-total fields to the browser WebGPU glyph/RGBA upload A/B summary.
- 2026-06-01: added WebGPU effect-uniform batching, shared same-parameter uploads, dynamic-offset mixed-effect support, counters, and browser A/B coverage.
- 2026-06-01: added direct GPU timestamp-total fields to the WebGPU effect-uniform A/B report.
- 2026-06-01: added per-benchmark browser User Timing marks to the WebGPU report and Chrome trace contract.
- 2026-06-01: added conservative WebGPU backdrop copy/pass coalescing plus browser A/B coverage for coalesced versus per-backdrop copies.
- 2026-06-01: added reusable WebGPU image-upload scratch storage, direct scratch texture writes, temp-allocation counters, and current-versus-legacy upload-scratch A/B coverage.
- 2026-06-01: added browser WebGPU draw-state cache counters and current-versus-legacy-rebind A/B coverage.
- 2026-06-01: added browser WebGPU clip-state cache A/B coverage for nested `ClipPush`/`ClipPop` scissor runs.
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
