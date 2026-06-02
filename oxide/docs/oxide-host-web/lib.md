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
- `oxide_host_web::OxideWebApp::render_webgpu_app_snapshot(&self) -> Result<String, JsValue>`: renders a fixed-timestamp app frame into the canvas for deterministic browser-backed golden verification.
- `oxide_host_web::OxideWebApp::render_webgpu_scene3d_snapshot(&self, width: u32, height: u32) -> Result<String, JsValue>`: renders a deterministic WebGPU Scene3D frame into the canvas for browser-backed golden verification.
- `oxide_host_web::OxideWebApp::bench_frames(&self, frames: u32) -> Result<String, JsValue>`: runs a bounded immediate frame loop and returns aggregate browser timing for quick ad hoc checks.
- `oxide_host_web::OxideWebApp::bench_frame_samples(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs sampled immediate frame loops and returns p50/p95/p99/peak plus 60 Hz and 120 Hz missed-frame/hitch timing for persisted browser baselines.
- `oxide_host_web::OxideWebApp::bench_webgpu_id_mask_ab(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs current and legacy WebGPU ID-mask compositor samples against the same scene contract and returns p50/p95/p99/peak plus missed-frame/hitch A/B timing.
- `oxide_host_web::OxideWebApp::bench_webgpu_upload_ab(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs WebGPU glyph-atlas A8 and RGBA image upload samples comparing dirty-subrect updates against full-texture updates while drawing equivalent output and reporting direct timestamp totals for the rendered pass.
- `oxide_host_web::OxideWebApp::bench_webgpu_upload_scratch_ab(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs equivalent dirty A8/RGBA upload samples through the reusable upload-scratch path and a benchmark-only temporary-allocation path, returning temp-allocation, scratch, upload-byte, and p50/p95/p99/peak A/B metrics.
- `oxide_host_web::OxideWebApp::bench_webgpu_effect_uniform_ab(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs equivalent backdrop-effect samples through the shared/batched effect-uniform path and a benchmark-only per-backdrop uniform-write path, returning effect write/byte/slot, direct WebGPU timestamp totals, and p50/p95/p99/peak A/B metrics.
- `oxide_host_web::OxideWebApp::bench_webgpu_backdrop_batch_ab(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs separated consecutive backdrop samples through the coalesced scene-copy path and a benchmark-only per-backdrop copy path, returning texture-copy, render-pass, effect-slot, and p50/p95/p99/peak A/B metrics.
- `oxide_host_web::OxideWebApp::bench_webgpu_scene3d_ab(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs WebGPU Scene3D samples comparing retained mesh buffers against a recreate-mesh-per-frame path for both a two-instance scene and a 96-instance stress scene while drawing equivalent output.
- `oxide_host_web::OxideWebApp::bench_webgpu_mixed_matrix(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs the same warm mixed command-encoding workload through current WebGPU state/effect batching and a benchmark-only legacy rebind/unbatched path, returning p50/p95/p99/peak plus draw-state, effect-write, pass, and texture-copy A/B counters.
- `oxide_host_web::OxideWebApp::bench_webgpu_layer_effects_matrix(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs the same warm layer/damage/effects workload through current WebGPU state/effect batching and a benchmark-only legacy rebind/unbatched path, returning p50/p95/p99/peak plus layer, damage, draw-state, effect-write, texture-copy, pass, timestamp, and resource-churn A/B counters.
- `oxide_host_web::OxideWebApp::bench_webgpu_command_family_matrix(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs the same warm generic draw-command workload through current WebGPU draw-state caching and a benchmark-only legacy rebind path, covering `ImageMesh`, `NineSlice`, SDF glyphs, and zero web `CameraBg` work without adding product-specific renderer hooks.
- `oxide_host_web::OxideWebApp::bench_webgpu_draw_state_cache_ab(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs a 1024-draw same-texture WebGPU workload through the current draw-state cache and a benchmark-only legacy rebind mode, preserving draw count and visual output while comparing redundant pipeline/bind-group/scissor work.
- `oxide_host_web::OxideWebApp::bench_webgpu_clip_state_ab(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs a 512-draw same-texture WebGPU workload through nested `ClipPush`/`ClipPop` scissor runs, preserving draw count and clip depth while comparing current state caching against benchmark-only legacy per-draw rebinding.
- `oxide_host_web::OxideWebApp::render_webgpu_id_mask_snapshot(&self) -> Result<String, JsValue>`: renders the deterministic WebGPU ID-mask compositor capture scene into the canvas for browser-backed golden verification.
- `oxide_host_web::OxideWebApp::set_scene(&self, scene_index: usize)`: switches the test-scene router scene.
- `oxide_host_web::OxideWebApp::last_draw_count(&self) -> u32`: returns the last renderer draw count.
- `oxide_host_web::OxideWebApp::renderer_backend(&self) -> String`: returns the active renderer backend name for smoke/perf logging.
- `oxide_host_web::start_oxide(canvas_id: &str) -> Result<OxideWebApp, JsValue>`: synchronous convenience export that returns `Unsupported` for the same reason as `OxideWebApp::new`.
- `oxide_host_web::start_oxide_async(canvas_id: &str) -> Promise<OxideWebApp>`: async convenience wasm export that constructs and starts the required WebGPU renderer.
- `oxide_host_web::platform_smoke_report() -> String`: wasm export used by the static page to verify browser-backed platform capabilities, network subscription installation, location permission status reads, and hidden iframe WebView create/close.
- `oxide_host_web::webgpu_smoke_report() -> Promise<String>`: wasm export that probes `navigator.gpu.requestAdapter()` and `adapter.requestDevice()` through dynamic JavaScript reflection without requiring unstable `web-sys` WebGPU bindings.
- `oxide_host_web::webgpu_timing_report() -> Promise<String>`: wasm export that probes `adapter.features.has("timestamp-query")`; direct collected samples are reported from renderer-owned timestamp writes in the benchmark rows when the adapter supports them.
- `oxide_host_web::host_web_requires_wasm32() -> &'static str`: native-build marker used when the crate is compiled outside wasm32.

## Logic narrative

The host creates a WebGPU-required `BrowserRenderer`, wraps it in `Rc<RefCell<_>>`, and gives the scene router an `ImageUploader` that forwards glyph atlas uploads into that renderer. The async static shell requires WebGPU; unsupported browsers fail startup instead of drawing through Canvas2D. Each frame resizes the canvas backing store from CSS size and `devicePixelRatio`, advances the router, draws into a reusable `DrawListBuilder`, takes damage rectangles, coalesces adjacent draws, and submits the draw list to WebGPU. Event listeners convert pointer, wheel, keyboard, hidden-textarea input, and browser composition events into the router's existing input methods. Touch and pen pointer events preserve `PointerEvent.timeStamp` as `TouchEvent::timestamp_ns` before entering Oxide input code. Custom `oxide-redraw` events dispatched by `oxide-platform-web` trigger an immediate frame; custom `oxide-ime-show` and `oxide-ime-hide` events focus/blur the hidden textarea and update Oxide IME geometry. The static page also calls `platform_smoke_report`, `webgpu_smoke_report`, `webgpu_timing_report`, `bench_frame_samples`, `bench_webgpu_id_mask_ab`, `bench_webgpu_upload_ab`, `bench_webgpu_upload_scratch_ab`, `bench_webgpu_effect_uniform_ab`, `bench_webgpu_backdrop_batch_ab`, `bench_webgpu_scene3d_ab`, `bench_webgpu_mixed_matrix`, `bench_webgpu_layer_effects_matrix`, `bench_webgpu_command_family_matrix`, `bench_webgpu_draw_state_cache_ab`, and `bench_webgpu_clip_state_ab` after wasm initialization and logs `oxide-platform-smoke`, `oxide-webgpu-smoke`, `oxide-webgpu-timing`, `oxide-renderer-backend`, `oxide-render-smoke`, `oxide-web-perf`, `oxide-webgpu-id-mask-ab`, `oxide-webgpu-upload-ab`, `oxide-webgpu-upload-scratch-ab`, `oxide-webgpu-effect-uniform-ab`, `oxide-webgpu-backdrop-batch-ab`, `oxide-webgpu-scene3d-ab`, `oxide-webgpu-mixed-matrix`, `oxide-webgpu-layer-effects-matrix`, `oxide-webgpu-command-family-matrix`, `oxide-webgpu-draw-state-cache-ab`, and `oxide-webgpu-clip-state-ab` so browser tests can prove the platform, GPU-capability, timestamp-query capability, renderer, timing, upload A/B, upload-scratch A/B, effect-uniform A/B, backdrop copy/pass A/B, Scene3D resource-lifetime A/B, mixed command A/B, layer/damage/effects A/B, generic command-family A/B, draw-state cache A/B, clip-state cache A/B, and WebGPU A/B paths are active without showing extra UI. The report endpoint constructs the app without starting RAF so async timestamp readback waits cannot repaint over the just-measured benchmark row, and timestamp settling rejects stale samples whose frame id predates the benchmark row or whose timestamp pass count does not match the row's render-pass count. For visual capture, `capture_target=app&capture_only=1` constructs the renderer without starting RAF, calls `render_webgpu_app_snapshot` at a fixed timestamp, and waits a few animation frames before Chrome captures the canvas; `capture_target=scene3d` calls `render_webgpu_scene3d_snapshot` with the script-provided capture dimensions so Chrome captures a real WebGPU Scene3D pass, and `capture_target=id-mask` calls `render_webgpu_id_mask_snapshot` so the final screenshot is the WebGPU compositor output. The direct capture paths hold a guard after the app, compositor, or Scene3D render so resize/redraw events cannot repaint the normal app scene over the WebGPU snapshot before Chrome takes the screenshot. The committed `goldens/snapshots/webgpu_browser.png` captures the 320x240 browser-rendered app canvas, `goldens/snapshots/webgpu_scene3d*.png` captures square, wide, and portrait browser-rendered Scene3D passes, and `goldens/snapshots/webgpu_id_mask_compositor.png` captures the 512x512 browser-rendered WebGPU ID-mask compositor.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

The wasm exports require a browser canvas with the requested id. The canvas should have CSS dimensions before startup so the backing size can be computed. The host stores listener closures for the app lifetime to keep DOM callbacks alive. The crate forbids unsafe code.

## Edge cases and failure modes

If the canvas id is missing or not a canvas, construction returns a JavaScript error converted from `RenderError`. If WebGPU is unavailable, async construction returns `Unsupported` and no visual Canvas2D fallback is drawn. If browser timing is unavailable, manual frame calls use timestamp `0.0`. Pointer events with touch or pen pointer types route through Oxide touch recognition; mouse pointer events route as pointer deltas. Unsupported browser platform services remain handled by `oxide-platform-web`.

## Concurrency and memory behavior

The browser host is single-threaded and uses `Rc<RefCell<_>>` rather than cross-thread synchronization. The draw-list builder is reused between frames to preserve allocation capacity. Event listener closures and the hidden IME textarea intentionally live for the app lifetime.

## Performance notes

The host avoids rebuilding the scene router and renderer per frame. The sampled benchmark exports are bounded async methods: frame production stays synchronous for timing stability, then the method yields to RAF only long enough to harvest nonblocking timestamp-query readbacks for the just-run case. The static benchmark page wraps each exported benchmark family in browser User Timing marks so the page report and duplicate Chrome trace can prove which workload phases ran, and the trace parser turns those marks into per-benchmark intervals with scoped event, GPU-event, and WebGPU/Dawn-event counts. The browser harness exposes Chrome GC and precise memory info, samples JS heap before and after each benchmark mark, and reports heap growth as attribution evidence without treating browser/reporting allocations as renderer hot-path failures. The wasm host installs a counting allocator for browser benchmark builds and reports Rust/WASM allocation deltas inside each post-warmup frame loop; these counters are separate from renderer-owned resource churn and currently gate bounded current-row allocation budgets plus zero current-row reallocations. The frame-loop benchmark also attributes those allocation deltas by host stage and uses caller-owned damage plus draw-coalescing storage so damage handoff and command coalescing stay allocation-free after warmup. The exported perf strings include renderer-owned resource counters for draw families, draw items, draw pipeline binds, draw bind-group binds, draw scissor sets, image meshes, nine-slices, SDF glyphs, camera backgrounds, layer markers, Scene3D draws, ID-mask draws, effect-family draws, render passes, direct timestamp-query family nanoseconds, command buffers, upload bytes, image-upload temp allocation bytes/counts, image-upload scratch capacity/growth, aggregate GPU resource creation plus family-level draw/image/target/Scene3D/effect/ID-mask resource attribution, aggregate CPU scratch capacity growth, family-level CPU scratch capacity/growth for draw, Scene3D, effect, ID-mask, image-upload, and resource-table storage, mesh creation, sampler creation, and runtime pipeline-creation violations. Effect-uniform A/B report gates keep browser p50 and direct GPU timestamp totals separate, so a GPU-stage win is not conflated with browser timing noise. WebGPU startup is async and front-loaded; frame rendering uses the same host loop after construction.

## Feature flags and cfgs

Browser exports compile only for `target_arch = "wasm32"`. Native builds expose only `generate_checker_rgba` and `host_web_requires_wasm32` so workspace tests can run on macOS.

## Testing and benchmarks

`oxide/host/web-app/oxide-host-web/tests/lib_tests.rs` verifies the procedural checker texture, guards the static shell import path plus platform, WebGPU, timing capability, render, sampled perf, upload A/B, upload-scratch A/B, effect-uniform A/B, backdrop-batch A/B, Scene3D A/B, mixed command A/B, layer/damage/effects A/B, command-family A/B, draw-state cache A/B, clip-state cache A/B, benchmark User Timing marks, benchmark JS heap sampling, wasm allocation-audit and frame-stage fields, hidden JSON report, capture target, deterministic app snapshot, deterministic Scene3D snapshot, and browser IME bridge hooks, asserts the committed WebGPU app, Scene3D, and ID-mask browser goldens exist with expected PNG dimensions and rendered pixels, and keeps the browser recapture script wired to target-specific pixel diffing plus persisted A/B report writes. Browser startup, platform smoke output, WebGPU device probing, timestamp-query capability probing, sampled frame timing, input, and pixel verification run through the static page after wasm-bindgen packaging. Browser results are persisted in `oxide/benchmarks/web/latest.json` and `oxide/benchmarks/web/latest.md`, including frame distribution, missed-frame/hitch fields, GPU-stage attribution status, a Chrome browser trace summary captured from a duplicate benchmark-report run, per-benchmark page and trace User Timing labels, per-benchmark Chrome trace intervals with scoped event/GPU/WebGPU counts, zero WASM memory growth and sampled JS heap growth across benchmark marks after prewarm, current-row Rust/WASM allocation counts/bytes with bounded per-frame budgets and zero reallocations, frame-loop stage allocation attribution, report-level and per-row warm-resource-churn summaries proving current warm rows have zero post-warmup growth, family-level GPU resource attribution for draw, image, target, Scene3D, effect, and ID-mask resources, family-level scratch growth attribution for draw, Scene3D, effect, ID-mask, image-upload, and resource-table storage, an explicit backend-path coverage matrix tying every important WebGPU path family to distribution rows and explanatory counters, upload A/B summaries with direct glyph/RGBA timestamp totals, and WebGPU backend counters for draws, draw items, draw pipeline binds, draw bind-group binds, draw scissor sets, solid triangles, image draws, image-mesh draws, nine-slice draws, glyph quads, SDF glyph quads, clip depth, damage rectangles, layer markers, Scene3D draws, ID-mask draws, effect-family draws, effect uniform writes/bytes/slots, camera-background draws, total render passes, pass-family counts, texture-copy count, command buffers, timestamp-query support, collected timestamp frame id/pass count/family nanoseconds/max pass nanoseconds/readback skips, upload bytes, image-upload temp allocation bytes/counts, image-upload scratch capacity/growth, aggregate and family-level GPU resource creation, aggregate and family-level CPU scratch growth, mesh creation, sampler creation, and runtime pipeline-creation violations for the 27-row browser WebGPU matrix: frame-loop row, ID-mask A/B rows, glyph-atlas upload rows, RGBA image upload rows, upload-scratch current/legacy rows, effect-uniform current/legacy rows, backdrop-batch current/legacy rows, two-instance Scene3D rows, 96-instance Scene3D stress rows, mixed text/image/effect current/legacy rows, layer/damage/effects current/legacy rows, command-family current/legacy rows, draw-state current/legacy rows, and clip-state current/legacy rows. The recapture script retries the visual capture a bounded number of times when Chrome returns a blank or mismatched startup frame; the final successful capture still must pass the normal pixel/golden thresholds and the duplicate trace must contain all benchmark User Timing labels and trace intervals before any report is written. Use `node scripts/check_webgpu_browser_golden.mjs --virtual-time-budget 30000 --out /tmp/webgpu_browser.png --json-report benchmarks/web/latest.json --markdown-report benchmarks/web/latest.md --trace-json /tmp/oxide-webgpu-browser-trace.json` to recapture the 320x240 Chrome/WebGPU app canvas, compare it against `goldens/snapshots/webgpu_browser.png`, refresh the browser WebGPU frame-loop plus ID-mask/upload/upload-scratch/effect-uniform/backdrop-batch/Scene3D/mixed/layer-effects/command-family/draw-state/clip-state baseline from an untraced run, and attach Chrome trace evidence with benchmark User Timing labels and per-benchmark intervals from a duplicate benchmark-report run. Use `node scripts/check_webgpu_browser_golden.mjs --target scene3d --width 512 --height 512 --out /tmp/webgpu_scene3d.png` to recapture and compare the committed square WebGPU Scene3D golden; use `--width 640 --height 360 --golden goldens/snapshots/webgpu_scene3d_wide.png` and `--width 360 --height 640 --golden goldens/snapshots/webgpu_scene3d_portrait.png` for the aspect goldens. Use `node scripts/check_webgpu_browser_golden.mjs --target id-mask --width 512 --height 512 --out /tmp/webgpu_id_mask.png` to recapture and compare the committed 512x512 WebGPU ID-mask compositor golden. On Rosetta shells, add `--chrome-arch arm64` or `CHROME_ARCH=arm64` so universal Chrome starts in the native architecture.

## Examples

```javascript
import init, { platform_smoke_report, start_oxide_async, webgpu_smoke_report, webgpu_timing_report } from "./pkg/oxide_host_web.js";

await init();
window.oxidePlatformSmoke = platform_smoke_report();
console.log("oxide-platform-smoke", window.oxidePlatformSmoke);
window.oxideWebGpuSmoke = await webgpu_smoke_report();
console.log("oxide-webgpu-smoke", window.oxideWebGpuSmoke);
window.oxideWebGpuTiming = await webgpu_timing_report();
console.log("oxide-webgpu-timing", window.oxideWebGpuTiming);
window.oxideApp = await start_oxide_async("oxide-canvas");
window.oxideApp.frame();
console.log("oxide-renderer-backend", window.oxideApp.renderer_backend());
console.log("oxide-render-smoke", `draws=${window.oxideApp.last_draw_count()}`);
window.oxideWebPerf = window.oxideApp.bench_frame_samples(8, 30);
console.log("oxide-web-perf", window.oxideWebPerf);
window.oxideWebGpuIdMaskAB = window.oxideApp.bench_webgpu_id_mask_ab(6, 24);
console.log("oxide-webgpu-id-mask-ab", window.oxideWebGpuIdMaskAB);
window.oxideWebGpuScene3dAB = window.oxideApp.bench_webgpu_scene3d_ab(6, 24);
console.log("oxide-webgpu-scene3d-ab", window.oxideWebGpuScene3dAB);
```

## Changelog

- 2026-06-01: added per-benchmark browser User Timing marks to the WebGPU report and duplicate Chrome trace contract.
- 2026-06-02: added per-benchmark Chrome trace interval attribution to the WebGPU report contract.
- 2026-06-02: added explicit browser WebGPU backend-path coverage matrix checks.
- 2026-06-02: added browser WebGPU Rust/WASM frame allocation counters and current-row allocation budget gates.
- 2026-06-02: added frame-loop stage allocation attribution and reusable draw-coalescing storage to reduce warm app-frame allocations.
- 2026-06-02: added direct timestamp-total fields to the browser WebGPU glyph/RGBA upload A/B summary.
- 2026-06-01: added direct GPU timestamp-total fields to the browser WebGPU effect-uniform A/B summary.
- 2026-06-01: added browser WebGPU effect-uniform A/B rows, effect uniform counters, and report gates.
- 2026-06-02: added browser WebGPU mixed text/image/effects current-versus-legacy-rebind/unbatched A/B rows and report gates.
- 2026-06-02: added browser WebGPU layer/damage/effects current-versus-legacy-rebind/unbatched A/B rows and report gates.
- 2026-06-02: added browser WebGPU command-family current-versus-legacy-rebind A/B rows and report gates.
- 2026-06-01: added browser WebGPU backdrop-batch A/B rows, texture-copy/render-pass counters, and report gates.
- 2026-06-01: added browser WebGPU upload-scratch A/B rows, image-upload temp/scratch counters, and a configurable browser report timeout.
- 2026-06-01: added browser WebGPU draw-state cache A/B rows and report gates.
- 2026-06-01: added browser WebGPU clip-state cache A/B rows and report gates.
- 2026-06-01: added a dedicated browser WebGPU command-family matrix row for generic `ImageMesh`, `NineSlice`, SDF glyph, and zero web `CameraBg` work without product-specific globe hooks.
- 2026-06-01: hardened browser WebGPU timestamp settling against stale prior-row readbacks and added an app-capture animation-frame settle before screenshot.
- 2026-06-01: added a dedicated browser WebGPU layer/damage/effects matrix row and report gate.
- 2026-06-01: persisted nonblocking WebGPU timestamp-query row metrics and kept report benchmarks isolated from the normal RAF loop during readback waits.
- 2026-06-01: added 96-instance browser WebGPU Scene3D stress rows to the persisted report contract.
- 2026-06-01: persisted and gated WebGPU sampler creation counters in the browser report contract.
- 2026-06-01: persisted and gated WebGPU CPU scratch growth counters in the browser report contract.
- 2026-06-02: added family-level WebGPU warm scratch attribution to host metrics and browser report gates.
- 2026-06-02: added family-level WebGPU GPU resource attribution to host metrics and browser report gates.
- 2026-06-01: added browser WebGPU Scene3D reused-mesh versus recreate-mesh A/B rows with direct resource lifetime counters.
- 2026-06-01: added a deterministic browser WebGPU Scene3D capture target and committed golden coverage.
- 2026-06-01: made browser WebGPU Scene3D capture dimension-aware and added wide/portrait golden coverage.
- 2026-06-01: persisted WebGPU frame-loop backend counters beside browser timing distributions.
- 2026-06-01: persisted WebGPU resource counters beside frame-loop and ID-mask A/B browser distributions.
- 2026-06-01: added browser WebGPU glyph-atlas upload, RGBA image upload, and mixed text/image/effect workload rows.
- 2026-06-01: added layer-marker counters to the browser WebGPU report contract.
- 2026-06-01: guarded direct WebGPU ID-mask capture from host resize/redraw events and added a final animation-frame settle so the browser golden script captures the compositor instead of the app scene.
- 2026-06-01: made the browser app golden deterministic by rendering a no-RAF fixed-timestamp WebGPU app snapshot for capture-only screenshots.
- 2026-06-01: added browser capture-target support and a committed 512x512 WebGPU ID-mask compositor golden.
- 2026-06-01: corrected the browser WebGPU ID-mask A/B geometry to rasterize the full 512 px mask instead of a corner-only screen-space grid.
- 2026-05-31: added the WebGPU ID-mask compositor A/B benchmark hook to the static browser page.
- 2026-05-31: added 60 Hz and 120 Hz missed-frame/hitch fields to the WebGPU browser frame-loop and ID-mask A/B baseline rows.
- 2026-05-31: added hidden browser JSON report output and WebGPU baseline writing to the recapture script so frame-loop and ID-mask A/B rows are persisted instead of only logged.
- 2026-05-31: added a committed 320x240 WebGPU browser canvas golden under `goldens/snapshots/webgpu_browser.png`.
- 2026-05-31: added `scripts/check_webgpu_browser_golden.mjs` to recapture the Chrome/WebGPU canvas and pixel-diff it against the committed browser golden.
- 2026-05-31: added `CHROME_ARCH` / `--chrome-arch` support to the WebGPU golden script for Rosetta-hosted macOS shells.
- 2026-05-31: preserved browser touch/pen sample timestamps in `TouchEvent::timestamp_ns`.
- Compacted DOM listener registration through a retained-listener helper while preserving the app-lifetime closure invariant.
- Hard-cut web visual startup to WebGPU only; synchronous startup and unsupported browsers now return `Unsupported` instead of drawing through Canvas2D.
- Added async WebGPU renderer selection and renderer-backend smoke logging.
- Added the WebGPU smoke export and sampled browser frame benchmark hook.
- Added hidden-textarea IME composition/input bridge wiring.
- Added the platform smoke export and static shell hook for browser backend verification.
