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
- `oxide_renderer_web::BrowserRenderer::set_camera_background_rgba8(&mut self, width: u32, height: u32, data: &[u8], row_bytes: usize) -> Result<(), RenderError>`: publishes the latest browser/app-owned camera frame for `DrawCmd::CameraBg`.
- `oxide_renderer_web::BrowserRenderer::image_create_a8(&mut self, width: u32, height: u32, data: &[u8], row_bytes: usize) -> ImageHandle`: uploads a glyph atlas as a WebGPU alpha-mask texture.
- `oxide_renderer_web::BrowserRenderer::image_update_a8(&mut self, handle: ImageHandle, x: u32, y: u32, width: u32, height: u32, data: &[u8], row_bytes: usize)`: updates an atlas subrectangle on the WebGPU texture.
- `impl oxide_renderer_api::Renderer for BrowserRenderer`: provides device caps, frame begin, WebGPU draw-list encoding, submit, and resize.
- `oxide_renderer_web::color_to_css(color: Color) -> String`: converts Oxide colors to Canvas2D CSS strings.
- `oxide_renderer_web::packed_rgba_to_css(rgba: u32) -> String`: converts packed vertex color to CSS.
- `oxide_renderer_web::color_cache_key(color: Color) -> u32`: returns the glyph tint cache key.
- `oxide_renderer_web::sanitize_scale(scale: f32) -> f32`: normalizes invalid device scales.

## Logic narrative

The legacy Canvas2D renderer stores one visible canvas context plus offscreen canvases for uploaded images, layers, and sampled backdrops. It remains in the crate for diagnostics and native test coverage, but it is not re-exported as a wasm public API and is not reachable from `BrowserRenderer` or the web host production startup path.

The WebGPU renderer owns the browser surface, persistent pipelines, persistent present buffers, reusable frame vertex/index buffers, renderable scene/scratch textures, and texture bind groups for uploaded images. It compiles WGSL pipelines for solid geometry, RGBA images, A8 glyph masks, SDF glyph masks, and sampled backdrop effects. It encodes the draw list into a compact frame command stream and uploads contiguous vertex/index buffers. Frames without backdrop sampling draw directly to the WebGPU surface in one render pass. Frames with backdrop/effect commands render through the scene texture, copy to scratch only at effect boundaries, then present the scene texture to the surface. Text rendering is supported through the normal `DrawCmd::GlyphRun` path because that command is replayed with the owning draw list's vertices and indices. The legacy standalone `RenderEncoder::draw_glyph_run(&GlyphRun)` callback is still insufficient by itself; renderer-agnostic replay now calls `draw_glyph_run_resolved` with resolved geometry.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

Canvas constructors require a browser `window`, `document`, and 2D canvas context. WebGPU construction also requires `navigator.gpu`, a compatible adapter, a device, and a canvas WebGPU surface. Image uploads require enough row-strided bytes for the requested dimensions. Frame submission requires the token returned by the matching `begin_frame`. Image handle `0` is reserved as the invalid handle. The crate forbids unsafe code.

## Edge cases and failure modes

Invalid scales collapse to `1.0`. Invalid image rows return `RenderError::InvalidOperation` on fallible APIs and `ImageHandle(0)` on host-uploader convenience APIs. Unknown images are skipped during drawing. `CameraBg` is skipped until a camera frame has been published. WebGPU initialization failure is reported as unsupported; the production browser renderer does not draw through Canvas2D.

## Concurrency and memory behavior

Browser renderers are intended for the browser main thread and store DOM/GPU handles only on wasm32. Native builds expose a small stub so macOS workspace checks can compile. The production WebGPU renderer reuses pipelines, texture bind groups, present buffers, scene/scratch textures, and grow-only frame vertex/index buffers after warmup.

## Performance notes

Draw complexity is linear in draw-list commands plus emitted solid triangles and glyph quads. WebGPU reduces browser draw-call overhead by batching geometry into frame buffers, drawing no-effect frames directly to the surface, and using shader paths for A8/SDF glyphs and sampled backdrop effects. Backdrop/effect commands pay a scene-texture copy before the effect pass. Persisted browser numbers are collected from release wasm builds.

## Feature flags and cfgs

The real backend is compiled only on `target_arch = "wasm32"`. Non-wasm builds expose `WebRenderer::new_for_tests` and a `Renderer` implementation that returns `RenderError::Unsupported` from `submit` so native tests can inspect shared helpers without exposing a browser Canvas2D visual path.

## Testing and benchmarks

`oxide/crates/renderer-web/tests/lib_tests.rs` covers color conversion, scale normalization, layer sizing, and native stub behavior. Browser pixel tests run through `oxide-host-web` after wasm-bindgen packaging, and the current browser WebGPU baseline is persisted in `oxide/benchmarks/web/latest.json` plus `oxide/benchmarks/web/latest.md`.

## Examples

```rust
pub async fn build_renderer() -> Result<oxide_renderer_web::BrowserRenderer, oxide_renderer_api::RenderError>
{
   oxide_renderer_web::BrowserRenderer::from_canvas_id_webgpu("oxide").await
}
```

## Changelog

- Compacted row-strided image copying and Canvas2D fallback camera/backdrop helper branches without changing the public WebGPU startup contract.
- Added browser pixel verification and persisted Canvas2D wasm baseline coverage through `oxide-host-web`.
- Hard-cut production browser rendering to WebGPU only; unsupported browsers now fail construction instead of drawing through Canvas2D.
- Added async `BrowserRenderer` WebGPU selection, `WebGpuRenderer`, shader-backed A8/SDF/effect paths, and geometry-aware glyph replay.
- Added offscreen layer compositing, sampled backdrop blur, and camera-background frame publication.
- Added the initial Canvas2D WebAssembly renderer backend.
