# oxide-host-web::lib

## Intention and purpose

`oxide-host-web` is the browser WebAssembly host for Oxide. It turns the new web platform and web renderer crates into a runnable browser app by wiring a canvas, requestAnimationFrame, resize handling, input events, image uploads, fonts, and the existing Oxide test-scene router.

## Relation to the rest of the code

The host depends on `oxide-platform-web`, `oxide-renderer-web`, `oxide-ui-core`, `oxide-test-scenes`, and `oxide-text`. It does not define a separate UI model; it uses the same scene router and draw-list builder as the iOS/macOS hosts.

Call flow:

- JavaScript calls `oxide_host_web::start_oxide_async`
- `oxide_platform_web::install_current_platform`
- `oxide_renderer_web::BrowserRenderer::from_canvas_id_webgpu`
- `oxide_test_scenes::Router::draw`
- `oxide_ui_core::coalesce_adjacent_draws`
- `oxide_renderer_web::BrowserRenderer::encode_pass`
- Browser WebGPU presents the frame

## Entry points list

- `oxide_host_web::generate_checker_rgba(width: u32, height: u32) -> Vec<u8>`: builds the procedural checkerboard texture used by the Zoom scene.
- `oxide_host_web::OxideWebApp::new(canvas_id: &str) -> Result<OxideWebApp, JsValue>`: wasm-only synchronous constructor that returns `Unsupported` because browser WebGPU device creation is asynchronous.
- `oxide_host_web::OxideWebApp::new_async(canvas_id: &str) -> Promise<OxideWebApp>`: wasm-only async constructor that requires WebGPU and returns `Unsupported` if the browser cannot provide it.
- `oxide_host_web::OxideWebApp::start(&self) -> Result<(), JsValue>`: starts the requestAnimationFrame loop.
- `oxide_host_web::OxideWebApp::frame(&self) -> Result<(), JsValue>`: draws one frame immediately.
- `oxide_host_web::OxideWebApp::bench_frames(&self, frames: u32) -> Result<String, JsValue>`: runs a bounded immediate frame loop and returns aggregate browser timing for quick ad hoc checks.
- `oxide_host_web::OxideWebApp::bench_frame_samples(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs sampled immediate frame loops and returns p50/p95/p99/peak timing for persisted browser baselines.
- `oxide_host_web::OxideWebApp::set_scene(&self, scene_index: usize)`: switches the test-scene router scene.
- `oxide_host_web::OxideWebApp::last_draw_count(&self) -> u32`: returns the last renderer draw count.
- `oxide_host_web::OxideWebApp::renderer_backend(&self) -> String`: returns the active renderer backend name for smoke/perf logging.
- `oxide_host_web::start_oxide(canvas_id: &str) -> Result<OxideWebApp, JsValue>`: synchronous convenience export that returns `Unsupported` for the same reason as `OxideWebApp::new`.
- `oxide_host_web::start_oxide_async(canvas_id: &str) -> Promise<OxideWebApp>`: async convenience wasm export that constructs and starts the required WebGPU renderer.
- `oxide_host_web::platform_smoke_report() -> String`: wasm export used by the static page to verify browser-backed platform capabilities, network subscription installation, location permission status reads, and hidden iframe WebView create/close.
- `oxide_host_web::webgpu_smoke_report() -> Promise<String>`: wasm export that probes `navigator.gpu.requestAdapter()` and `adapter.requestDevice()` through dynamic JavaScript reflection without requiring unstable `web-sys` WebGPU bindings.
- `oxide_host_web::host_web_requires_wasm32() -> &'static str`: native-build marker used when the crate is compiled outside wasm32.

## Logic narrative

The host creates a WebGPU-required `BrowserRenderer`, wraps it in `Rc<RefCell<_>>`, and gives the scene router an `ImageUploader` that forwards glyph atlas uploads into that renderer. The async static shell requires WebGPU; unsupported browsers fail startup instead of drawing through Canvas2D. Each frame resizes the canvas backing store from CSS size and `devicePixelRatio`, advances the router, draws into a reusable `DrawListBuilder`, takes damage rectangles, coalesces adjacent draws, and submits the draw list to WebGPU. Event listeners convert pointer, wheel, keyboard, hidden-textarea input, and browser composition events into the router's existing input methods. Custom `oxide-redraw` events dispatched by `oxide-platform-web` trigger an immediate frame; custom `oxide-ime-show` and `oxide-ime-hide` events focus/blur the hidden textarea and update Oxide IME geometry. The static page also calls `platform_smoke_report`, `webgpu_smoke_report`, and `bench_frame_samples` after wasm initialization and logs `oxide-platform-smoke`, `oxide-webgpu-smoke`, `oxide-renderer-backend`, `oxide-render-smoke`, and `oxide-web-perf` so browser tests can prove the platform, GPU-capability, renderer, and timing paths are active without showing extra UI.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

The wasm exports require a browser canvas with the requested id. The canvas should have CSS dimensions before startup so the backing size can be computed. The host stores listener closures for the app lifetime to keep DOM callbacks alive. The crate forbids unsafe code.

## Edge cases and failure modes

If the canvas id is missing or not a canvas, construction returns a JavaScript error converted from `RenderError`. If WebGPU is unavailable, async construction returns `Unsupported` and no visual Canvas2D fallback is drawn. If browser timing is unavailable, manual frame calls use timestamp `0.0`. Pointer events with touch or pen pointer types route through Oxide touch recognition; mouse pointer events route as pointer deltas. Unsupported browser platform services remain handled by `oxide-platform-web`.

## Concurrency and memory behavior

The browser host is single-threaded and uses `Rc<RefCell<_>>` rather than cross-thread synchronization. The draw-list builder is reused between frames to preserve allocation capacity. Event listener closures and the hidden IME textarea intentionally live for the app lifetime.

## Performance notes

The host avoids rebuilding the scene router and renderer per frame. The sampled benchmark export is intentionally bounded and synchronous so browser verification can capture stable p50/p95/p99/peak values without adding an always-on measurement loop. WebGPU startup is async and front-loaded; frame rendering uses the same host loop after construction.

## Feature flags and cfgs

Browser exports compile only for `target_arch = "wasm32"`. Native builds expose only `generate_checker_rgba` and `host_web_requires_wasm32` so workspace tests can run on macOS.

## Testing and benchmarks

`oxide/host/web-app/oxide-host-web/tests/lib_tests.rs` verifies the procedural checker texture and guards the static shell import path plus platform, WebGPU, render, sampled perf, and browser IME bridge hooks. Browser startup, platform smoke output, WebGPU device probing, sampled frame timing, input, and pixel verification run through the static page after wasm-bindgen packaging. Browser results are persisted in `oxide/benchmarks/web/latest.json` and `oxide/benchmarks/web/latest.md`.

## Examples

```javascript
import init, { platform_smoke_report, start_oxide_async, webgpu_smoke_report } from "./pkg/oxide_host_web.js";

await init();
window.oxidePlatformSmoke = platform_smoke_report();
console.log("oxide-platform-smoke", window.oxidePlatformSmoke);
window.oxideWebGpuSmoke = await webgpu_smoke_report();
console.log("oxide-webgpu-smoke", window.oxideWebGpuSmoke);
window.oxideApp = await start_oxide_async("oxide-canvas");
window.oxideApp.frame();
console.log("oxide-renderer-backend", window.oxideApp.renderer_backend());
console.log("oxide-render-smoke", `draws=${window.oxideApp.last_draw_count()}`);
window.oxideWebPerf = window.oxideApp.bench_frame_samples(8, 30);
console.log("oxide-web-perf", window.oxideWebPerf);
```

## Changelog

- Compacted DOM listener registration through a retained-listener helper while preserving the app-lifetime closure invariant.
- Hard-cut web visual startup to WebGPU only; synchronous startup and unsupported browsers now return `Unsupported` instead of drawing through Canvas2D.
- Added async WebGPU renderer selection and renderer-backend smoke logging.
- Added the WebGPU smoke export and sampled browser frame benchmark hook.
- Added hidden-textarea IME composition/input bridge wiring.
- Added the platform smoke export and static shell hook for browser backend verification.
