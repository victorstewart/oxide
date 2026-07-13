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
- `oxide_host_web::OxideWebApp::frame_at_timestamp_unprofiled(&self, timestamp_ms: f64) -> Result<(), JsValue>`: submits one normal frame without stage instrumentation for bounded harness-overhead controls.
- `oxide_host_web::OxideWebApp::frame_at_timestamp_profiled(&self, timestamp_ms: f64) -> Result<String, JsValue>`: submits one normal production frame at a supplied high-resolution RAF timestamp and returns bounded per-stage CPU timing for the displayed-frame harness.
- `oxide_host_web::OxideWebApp::begin_raf_gpu_timestamp_capture(&self)`: clears stale samples and enables one timestamp readback per measured RAF frame.
- `oxide_host_web::OxideWebApp::finish_raf_gpu_timestamp_capture(&self) -> Promise<String>`: drains completion-safe GPU samples, restores normal sampled cadence, and returns raw per-frame pass-family timings plus queue-drain duration, RAF waits, and pending counts.
- `oxide_host_web::OxideWebApp::render_webgpu_app_snapshot(&self) -> Result<String, JsValue>`: renders a fixed-timestamp app frame into the canvas for deterministic browser-backed golden verification.
- `oxide_host_web::OxideWebApp::render_webgpu_scene3d_snapshot(&self, width: u32, height: u32) -> Result<String, JsValue>`: renders a deterministic WebGPU Scene3D frame into the canvas for browser-backed golden verification.
- `oxide_host_web::OxideWebApp::bench_frames(&self, frames: u32) -> Result<String, JsValue>`: runs a bounded immediate frame loop and returns aggregate browser timing for quick ad hoc checks.
- `oxide_host_web::OxideWebApp::bench_cpu_submit_samples(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs sampled immediate frame loops as CPU-submit throughput and returns CPU p50/p95/p99/peak without displayed-frame, missed-frame, hitch, or event-to-visible claims.
- `oxide_host_web::OxideWebApp::bench_webgpu_id_mask_current(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs the current WebGPU ID-mask compositor as unpaced CPU-submit throughput with direct GPU counters.
- `oxide_host_web::OxideWebApp::bench_webgpu_id_mask_ab(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs current and legacy WebGPU ID-mask compositor throughput against the same scene contract.
- `oxide_host_web::OxideWebApp::bench_webgpu_upload_current(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs current WebGPU glyph-atlas A8 and RGBA dirty-subrect upload samples while drawing equivalent output and reporting direct timestamp totals for the rendered pass.
- `oxide_host_web::OxideWebApp::bench_webgpu_upload_scratch_ab(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs equivalent dirty A8/RGBA upload samples through the reusable upload-scratch path and a benchmark-only temporary-allocation path, returning temp-allocation, scratch, upload-byte, and p50/p95/p99/peak A/B metrics.
- `oxide_host_web::OxideWebApp::bench_webgpu_effect_uniform_ab(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs equivalent backdrop-effect samples through the shared/batched effect-uniform path, returning effect write/byte/slot, direct WebGPU timestamp totals, and p50/p95/p99/peak metrics after the slower default per-backdrop uniform-write row was retired.
- `oxide_host_web::OxideWebApp::bench_webgpu_backdrop_batch_current(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs separated consecutive backdrop samples through the current coalesced scene-copy path, returning texture-copy, render-pass, effect-slot, and p50/p95/p99/peak metrics after the slower default per-backdrop-copy row was retired.
- `oxide_host_web::OxideWebApp::bench_webgpu_scene3d_ab(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs WebGPU Scene3D samples comparing retained mesh buffers against a recreate-mesh-per-frame path for both a two-instance scene and a 96-instance stress scene while drawing equivalent output.
- `oxide_host_web::OxideWebApp::bench_webgpu_mixed_matrix(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs the same warm mixed command-encoding workload through current WebGPU state/effect batching, returning p50/p95/p99/peak plus draw-state, effect-write, pass, texture-copy, glyph, image, layer, clip, damage, and timestamp counters after the slower default legacy row was retired.
- `oxide_host_web::OxideWebApp::bench_webgpu_layer_effects_matrix(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs the warm layer/damage/effects workload through the current WebGPU state/effect batching path, returning p50/p95/p99/peak plus layer, damage, draw-state, effect-write, texture-copy, pass, timestamp, and resource-churn counters after the slower default legacy row was retired.
- `oxide_host_web::OxideWebApp::bench_webgpu_command_family_matrix(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs the warm generic draw-command workload through current WebGPU draw-state caching, covering `ImageMesh`, `NineSlice`, SDF glyphs, and zero web `CameraBg` work without adding product-specific renderer hooks.
- `oxide_host_web::OxideWebApp::bench_webgpu_glyph_run_current(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs the atlas-backed A8 and SDF `GlyphRun` workload through current WebGPU draw-state caching, preserving glyph quads and passes while reporting pipeline, bind-group, and scissor work after the slower default legacy rebind row was retired.
- `oxide_host_web::OxideWebApp::bench_webgpu_neon_marker_ab(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs the generic `neon_marker` overlay path through current WebGPU draw-state caching, preserving marker and solid-triangle work while reporting pipeline/scissor/bind counters after the slower default legacy rebind row was retired.
- `oxide_host_web::OxideWebApp::bench_webgpu_direct_surface_ab(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs the no-effect WebGPU image workload through current direct-surface submission, preserving draw work while reporting render/draw/clear/present pass counters and direct GPU timestamp totals after the slower default forced scene-texture plus present-pass row was retired.
- `oxide_host_web::OxideWebApp::bench_webgpu_draw_item_coalescing_ab(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs the same 1024-draw same-texture workload through current adjacent draw-item coalescing and a benchmark-only uncoalesced path while leaving draw-state caching enabled, preserving visible work while reporting encoded draw-item, coalesced-item, draw-call, state-bind, and p50/p95/p99/peak A/B metrics.
- `oxide_host_web::OxideWebApp::bench_webgpu_draw_state_cache_ab(&self, samples: u32, frames_per_sample: u32) -> Result<String, JsValue>`: runs a 1024-draw same-texture WebGPU workload through the current draw-state cache and a benchmark-only legacy rebind mode, preserving draw count and visual output while comparing redundant pipeline/bind-group/scissor work.
- `oxide_host_web::OxideWebApp::render_webgpu_id_mask_snapshot(&self) -> Result<String, JsValue>`: renders the deterministic WebGPU ID-mask compositor capture scene into the canvas for browser-backed golden verification.
- `oxide_host_web::OxideWebApp::read_webgpu_asymmetric_id_mask_fields(&self) -> Result<String, JsValue>`: asynchronously returns exact sparse-fixture R8 raster IDs and final RGBA16F city/seam fields for CPU-reference comparison.
- `oxide_host_web::OxideWebApp::set_scene(&self, scene_index: usize)`: switches the test-scene router scene.
- `oxide_host_web::OxideWebApp::last_draw_count(&self) -> u32`: returns the last renderer draw count.
- `oxide_host_web::OxideWebApp::renderer_backend(&self) -> String`: returns the active renderer backend name for smoke/perf logging.
- `oxide_host_web::start_oxide(canvas_id: &str) -> Result<OxideWebApp, JsValue>`: synchronous convenience export that returns `Unsupported` for the same reason as `OxideWebApp::new`.
- `oxide_host_web::start_oxide_async(canvas_id: &str) -> Promise<OxideWebApp>`: async convenience wasm export that constructs and starts the required WebGPU renderer.
- `oxide_host_web::bench_canvas_indexed_quads(samples: u32, frames_per_sample: u32, quads: u32) -> Result<String, JsValue>`: wasm-only non-default diagnostic export that renders an indexed Canvas2D `ImageMesh` workload into a hidden canvas for same-workload A/B proof before changing Canvas fallback quad walking.
- `oxide_host_web::platform_smoke_report() -> String`: wasm export used by the static page to verify browser-backed platform capabilities, network subscription installation, location permission status reads, and hidden iframe WebView create/close.
- `oxide_host_web::webgpu_smoke_report() -> Promise<String>`: wasm export that probes `navigator.gpu.requestAdapter()` and `adapter.requestDevice()` through dynamic JavaScript reflection without requiring unstable `web-sys` WebGPU bindings.
- `oxide_host_web::webgpu_timing_report() -> Promise<String>`: wasm export that probes `adapter.features.has("timestamp-query")`; direct collected samples are reported from renderer-owned timestamp writes in the benchmark rows when the adapter supports them.
- `oxide_host_web::host_web_requires_wasm32() -> &'static str`: native-build marker used when the crate is compiled outside wasm32.

## Logic narrative

The snapshot-only `id_mask_reference_only=1` report bypasses the benchmark battery, renders the shared 17x11 asymmetric fixture, waits for four mapped readback planes, and returns integer raster masks plus final half-float fields. `scripts/compare_id_mask_reference.mjs` supports an explicit parent-mismatch assertion for C03 and a strict match assertion for C04.

The clean-layer dirty rerender row is also retired from the default browser page; layer/damage/effects remains the dirty-layer coverage row.

In non-default `canvas_diag=1` mode, the static page skips WebGPU startup and runs `bench_canvas_indexed_quads` on a hidden Canvas2D renderer. That mode exists only to collect same-workload Canvas fallback A/B evidence before retaining renderer changes.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

The wasm exports require a browser canvas with the requested id. The canvas should have CSS dimensions before startup so the backing size can be computed. The host stores listener closures for the app lifetime to keep DOM callbacks alive. The crate forbids unsafe code.

## Edge cases and failure modes

If the canvas id is missing or not a canvas, construction returns a JavaScript error converted from `RenderError`. If WebGPU is unavailable, async construction returns `Unsupported` and no visual Canvas2D fallback is drawn. If browser timing is unavailable, manual frame calls use timestamp `0.0`. Pointer events with touch or pen pointer types route through Oxide touch recognition; mouse pointer events route as pointer deltas. Unsupported browser platform services remain handled by `oxide-platform-web`.

## Concurrency and memory behavior

The browser host is single-threaded and uses `Rc<RefCell<_>>` rather than cross-thread synchronization. The draw-list builder is reused between frames to preserve allocation capacity. Event listener closures and the hidden IME textarea intentionally live for the app lifetime.

## Performance notes

The static page keeps synchronous CPU-submit throughput separate from displayed-frame evidence. `runRafFrameHarness` awaits every high-resolution RAF timestamp, calls `frame_at_timestamp_profiled` exactly once for that callback, and persists one raw RAF delta, CPU-submit duration, exact separable CPU-stage sample, and GPU timestamp per frame. Its local server enables cross-origin isolation so sub-millisecond stages retain high-resolution timer precision. Layout and text preparation in the current scene router are marked as fused into draw extraction instead of receiving fabricated standalone durations. A bounded 200/200 ABBA control measures the cost of stage instrumentation against the unprofiled production call, and queue-drain diagnostics prove timestamp readbacks completed. DPR 1/2/3, 1080p/4K window sizes, scene selection, resize cadence, and frame-count/endurance are explicit command controls. Journeys without input events mark event-to-submit and event-to-visible as not collected rather than deriving them from batch averages.

The host avoids rebuilding the scene router and renderer per frame. The sampled benchmark exports are bounded async methods: frame production stays synchronous for timing stability, then the method yields to RAF only long enough to harvest nonblocking timestamp-query readbacks for the just-run case. The static benchmark page wraps each exported benchmark family in browser User Timing marks so the page report and duplicate Chrome trace can prove which workload phases ran, and the trace parser turns those marks into per-benchmark intervals with scoped event, GPU-event, and WebGPU/Dawn-event counts. The browser harness exposes Chrome GC and precise memory info, samples JS heap before and after each benchmark mark, and reports heap growth as attribution evidence without treating browser/reporting allocations as renderer hot-path failures. The wasm host installs a counting allocator for browser benchmark builds and reports Rust/WASM allocation deltas inside each post-warmup frame loop; these counters are separate from renderer-owned resource churn and currently gate bounded current-row allocation budgets, zero current-row reallocations, and a shared allocation signature across every checked current row so path-specific heap churn cannot hide behind the fixed submit-boundary profile. Browser startup/package metrics are reported separately from warm benchmark rows; they exist to measure page-init and artifact-size effects of future explicit diagnostic cleanup without treating startup instrumentation as a renderer-frame win. The recapture script also has a non-default `--startup-report PATH [--startup-repeats N]` mode that launches the same WebGPU page with `startup_only=1`, renders the first app frame, skips benchmark rows, and writes repeated startup distributions so package/export cleanup A/B decisions are not made from one noisy run. It also has a non-default `--canvas-report PATH [--canvas-repeats N]` mode that writes repeated Canvas indexed-quad distributions for renderer-internal Canvas fallback experiments without changing the default WebGPU report. The frame-loop benchmark also attributes those allocation deltas by host stage and uses caller-owned damage plus draw-coalescing storage so damage handoff and command coalescing stay allocation-free after warmup. WebGPU matrix benchmarks keep their static mixed-scene, layer/effects, and clean-layer lists in the reusable benchmark resource object instead of allocating damage or draw resource lists during measured frames. The exported perf strings include renderer-owned resource counters for draw families, draw items, draw pipeline binds, draw bind-group binds, draw scissor sets, image meshes, nine-slices, SDF glyphs, camera backgrounds, layer markers, layer cache hits/misses/skipped draws/passes, Scene3D draws, ID-mask draws, effect-family draws, render passes, direct timestamp-query family nanoseconds, command buffers, upload bytes, image-upload temp allocation bytes/counts, image-upload scratch capacity/growth, aggregate GPU resource creation plus family-level draw/image/target/layer/Scene3D/effect/ID-mask resource attribution, aggregate CPU scratch capacity growth, family-level CPU scratch capacity/growth for draw, Scene3D, effect, ID-mask, image-upload, and resource-table storage, mesh creation, sampler creation, and runtime pipeline-creation violations. The generated browser report now includes a GPU timestamp stage-breakdown summary that reconciles clear, draw, Scene3D, ID-mask, and present pass-family nanoseconds against every source row and the aggregate timestamp-query attribution totals. Clean-layer current report gates keep the same retained layer scene under clean body-skip reuse after same-workload A/B proof retired the slower default dirty rerender row; layer-cache counters and pass counts prove the retained texture path skips body work. Effect-uniform current report gates keep browser p50 and direct GPU timestamp totals separate after same-workload A/B proof retired the slower default per-backdrop uniform-write row. WebGPU startup is async and front-loaded; frame rendering uses the same host loop after construction.

## Feature flags and cfgs

Browser exports compile only for `target_arch = "wasm32"`. Native builds expose only `generate_checker_rgba` and `host_web_requires_wasm32` so workspace tests can run on macOS.

## Testing and benchmarks

The `glyph` browser capture target renders a deterministic 512x512 grid containing ordinary A8 and SDF runs and compares it with `goldens/snapshots/webgpu_glyph_atlas.png`. `OxideWebApp::bench_webgpu_atlas_c15` is a non-default diagnostic for paired padded-row cold 1024x1024 creation, full 1024x1024 update, and 64x64 dirty-update evidence; it does not expand the committed default browser battery. Run `node scripts/check_webgpu_browser_golden.mjs --target glyph --width 512 --height 512 --out /tmp/webgpu_glyph.png` after packaging the wasm host. Run one adapter sample with `CHROME_ARCH=arm64 node scripts/run_webgpu_atlas_c15.mjs host/web-app/www 1 24 cold /tmp/c15-cold.json`; replace `cold` with `full` or `dirty` for the other timing distribution.

`oxide/host/web-app/oxide-host-web/tests/lib_tests.rs` verifies the procedural checker texture, guards the static shell import path plus platform, WebGPU, timing capability, render, sampled perf, current ID-mask coverage, current upload coverage, effect-uniform current coverage, backdrop-batch current coverage, Scene3D A/B, mixed current coverage, layer/damage/effects current coverage, clean-layer current coverage, command-family current coverage, glyph-run current, neon-marker current, direct-surface current coverage, non-default Canvas indexed-quad diagnostic reporting, browser startup/package reporting, benchmark User Timing marks, benchmark JS heap sampling, wasm allocation-audit and frame-stage fields, hidden JSON report, capture target, deterministic app snapshot, deterministic Scene3D snapshot, startup-only repeat reporting, and browser IME bridge hooks, asserts the committed WebGPU app, Scene3D, and ID-mask browser goldens exist with expected PNG dimensions and rendered pixels, and keeps the browser recapture script wired to target-specific pixel diffing plus persisted report writes. Browser startup, platform smoke output, WebGPU device probing, timestamp-query capability probing, sampled frame timing, input, and pixel verification run through the static page after wasm-bindgen packaging. Browser results are persisted in `oxide/benchmarks/web/latest.json` and `oxide/benchmarks/web/latest.md`, including startup timing and static package bytes, frame distribution, missed-frame/hitch fields, GPU-stage attribution status, a Chrome browser trace summary captured from a duplicate benchmark-report run, per-benchmark page and trace User Timing labels, per-benchmark Chrome trace intervals with scoped event/GPU/WebGPU counts, zero WASM memory growth and sampled JS heap growth across benchmark marks after prewarm, current-row Rust/WASM allocation counts/bytes with bounded per-frame budgets and zero reallocations, frame-loop stage allocation attribution, report-level and per-row warm-resource-churn summaries proving current warm rows have zero post-warmup growth, family-level GPU resource attribution for draw, image, target, layer, Scene3D, effect, and ID-mask resources, family-level scratch growth attribution for draw, Scene3D, effect, ID-mask, image-upload, and resource-table storage, an explicit 15-path backend coverage matrix tying every important default WebGPU path family to distribution rows and explanatory counters, current upload summaries with direct glyph/RGBA timestamp totals, and WebGPU backend counters for draws, draw items, coalesced draw items, draw pipeline binds, draw bind-group binds, draw scissor sets, solid triangles, image draws, image-mesh draws, nine-slice draws, glyph quads, SDF glyph quads, clip depth, damage rectangles, layer markers, layer cache hits/misses/skipped draws/passes, Scene3D draws, ID-mask draws, effect-family draws, effect uniform writes/bytes/slots, camera-background draws, total render passes, pass-family counts, texture-copy count, command buffers, timestamp-query support, collected timestamp frame id/pass count/family nanoseconds/max pass nanoseconds/readback skips, upload bytes, image-upload temp allocation bytes/counts, image-upload scratch capacity/growth, aggregate and family-level GPU resource creation, aggregate and family-level CPU scratch growth, mesh creation, sampler creation, and runtime pipeline-creation violations for the 17-row browser WebGPU matrix: frame-loop row, ID-mask current row, current glyph-atlas upload row, current RGBA image upload row, effect-uniform current row, backdrop-batch current row, two-instance Scene3D rows, 96-instance Scene3D stress rows, mixed text/image/effect current row, layer/damage/effects current row, clean-layer clean row, command-family current row, glyph-run current row, neon-marker current row, and direct-surface current row. The recapture script retries the visual capture a bounded number of times when Chrome returns a blank or mismatched startup frame; the final successful capture still must pass the normal pixel/golden thresholds and the duplicate trace must contain all benchmark User Timing labels and trace intervals before any report is written. Use `node scripts/check_webgpu_browser_golden.mjs --virtual-time-budget 30000 --out /tmp/webgpu_browser.png --json-report benchmarks/web/latest.json --markdown-report benchmarks/web/latest.md --trace-json /tmp/oxide-webgpu-browser-trace.json` to recapture the 320x240 Chrome/WebGPU app canvas, compare it against `goldens/snapshots/webgpu_browser.png`, refresh the browser WebGPU frame-loop plus ID-mask/upload/effect-uniform/backdrop-batch/Scene3D/mixed/layer-effects/clean-layer/command-family/glyph-run/neon-marker/direct-surface baseline from an untraced run, persist startup/package evidence, and attach Chrome trace evidence with benchmark User Timing labels and per-benchmark intervals from a duplicate benchmark-report run. Use `node scripts/check_webgpu_browser_golden.mjs --startup-report /tmp/oxide-webgpu-startup.json --startup-repeats 7 --chrome-arch arm64` to collect repeated startup/package distributions without changing the committed browser report matrix. Use `node scripts/check_webgpu_browser_golden.mjs --canvas-report /tmp/oxide-canvas-indexed-quads.json --canvas-repeats 5 --canvas-samples 6 --canvas-frames 24 --canvas-quads 512 --chrome-arch arm64` to collect repeated Canvas indexed-quad distributions without changing the committed browser report matrix. Use `node scripts/check_webgpu_browser_golden.mjs --target scene3d --width 512 --height 512 --out /tmp/webgpu_scene3d.png` to recapture and compare the committed square WebGPU Scene3D golden; use `--width 640 --height 360 --golden goldens/snapshots/webgpu_scene3d_wide.png` and `--width 360 --height 640 --golden goldens/snapshots/webgpu_scene3d_portrait.png` for the aspect goldens. Use `node scripts/check_webgpu_browser_golden.mjs --target id-mask --width 512 --height 512 --out /tmp/webgpu_id_mask.png` to recapture and compare the committed 512x512 WebGPU ID-mask compositor golden. On Rosetta shells, add `--chrome-arch arm64` or `CHROME_ARCH=arm64` so universal Chrome starts in the native architecture.

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
window.oxideWebPerf = await window.oxideApp.bench_cpu_submit_samples(8, 30);
console.log("oxide-web-cpu-submit-throughput", window.oxideWebPerf);
window.oxideWebGpuIdMaskCurrent = window.oxideApp.bench_webgpu_id_mask_current(6, 24);
console.log("oxide-webgpu-id-mask-current", window.oxideWebGpuIdMaskCurrent);
window.oxideWebGpuScene3dAB = window.oxideApp.bench_webgpu_scene3d_ab(6, 24);
console.log("oxide-webgpu-scene3d-ab", window.oxideWebGpuScene3dAB);
```

## Changelog

- 2026-07-12: added the C15 single-channel atlas diagnostic and dedicated A8/SDF browser capture target.

- 2026-07-12: made the asymmetric WebGPU oracle encode a distractor and reference draw in one submission, persisted ID-mask warmup timing for paired analysis, and exposed exact uniform arena writes, bytes, and slots in browser reports.
- 2026-07-12: added the opt-in ten-row C01 WebGPU architecture primitive matrix with fixed scaling points, direct timestamps, queue completion, one-submission-per-RAF pacing, and zero-pass rejection; normal app execution does not invoke this matrix.
- 2026-06-22: retired the default browser WebGPU neon-marker legacy-rebind row after same-workload A/B proof while keeping current marker-overlay coverage.
- 2026-06-22: retired the default browser WebGPU effect-uniform per-backdrop uniform-write row after same-workload A/B proof while keeping current batched effect-uniform coverage.
- 2026-06-22: retired the default browser WebGPU backdrop-batch per-copy row after same-workload A/B proof while keeping current coalesced backdrop coverage.
- 2026-06-02: added browser WebGPU draw-item coalescing current-versus-uncoalesced A/B rows and report gates.
- 2026-06-01: added per-benchmark browser User Timing marks to the WebGPU report and duplicate Chrome trace contract.
- 2026-06-02: added per-benchmark Chrome trace interval attribution to the WebGPU report contract.
- 2026-06-02: added GPU timestamp stage-breakdown attribution to the WebGPU report contract.
- 2026-06-02: added explicit browser WebGPU backend-path coverage matrix checks.
- 2026-06-02: added browser WebGPU Rust/WASM frame allocation counters and current-row allocation budget gates.
- 2026-06-02: added frame-loop stage allocation attribution and reusable draw-coalescing storage to reduce warm app-frame allocations.
- 2026-06-22: retired the default browser WebGPU layer-effects legacy row after same-workload A/B proof, moving layer/damage/effects coverage to a current-only row.
- 2026-06-22: retired the default browser WebGPU command-family legacy row after same-workload A/B proof, moving the browser report to a 23-row current-only command-family matrix.
- 2026-06-22: retired the default browser WebGPU upload legacy rows and upload A/B export after same-workload A/B proof, moving upload coverage to current-only rows in report version 5.
- 2026-06-02: added direct timestamp-total fields to the browser WebGPU glyph/RGBA upload A/B summary.
- 2026-06-01: added direct GPU timestamp-total fields to the browser WebGPU effect-uniform A/B summary.
- 2026-06-01: added browser WebGPU effect-uniform A/B rows, effect uniform counters, and report gates.
- 2026-06-22: retired the default browser WebGPU mixed text/image/effects legacy rebind/unbatched row after same-workload A/B proof; the current row remains the default coverage gate.
- 2026-06-02: added browser WebGPU mixed text/image/effects current-versus-legacy-rebind/unbatched A/B rows and report gates.
- 2026-06-02: added browser WebGPU layer/damage/effects current-versus-legacy-rebind/unbatched A/B rows and report gates.
- 2026-06-22: retired the default browser WebGPU clean-layer dirty rerender row after same-workload A/B proof while keeping current retained-layer cache coverage.
- 2026-06-02: added browser WebGPU retained clean-layer comparison rows and report gates before the dirty row was later retired.
- 2026-06-02: moved mixed and layer/effects WebGPU matrix damage lists into reusable benchmark resources.
- 2026-06-02: added current-row WASM allocation-invariance report gates for the shared WebGPU submit-boundary profile.
- 2026-06-02: added browser WebGPU submit sub-stage WASM allocation attribution.
- 2026-06-02: added browser WebGPU glyph-run current-only rows and report gates.
- 2026-06-02: added browser WebGPU neon-marker current-versus-legacy-rebind A/B rows and report gates.
- 2026-06-02: added browser WebGPU direct-surface current-versus-forced-scene-present A/B rows and report gates.
- 2026-06-22: retired the default browser direct-surface forced-scene-present row after same-workload A/B proof showed current direct-surface submission used fewer passes and lower direct GPU time.
- 2026-06-22: retired default browser draw-item coalescing, draw-state cache, and clip-state cache standalone report rows after same-workload A/B proof showed current wins.
- 2026-06-22: retired the explicit clip-state diagnostic export after repeated startup/package A/B proof showed a smaller wasm-bindgen package and lower report-ready distribution.
- 2026-06-22: retired default browser upload-scratch standalone report rows after same-workload A/B proof showed current wins.
- 2026-06-22: retired the default browser glyph-run legacy-rebind row after same-workload A/B proof showed current draw-state caching wins.
- 2026-06-22: added browser startup and package-size report evidence for future same-workload diagnostic cleanup A/B tests.
- 2026-06-22: added a non-default repeated startup report mode for package/export cleanup A/B tests.
- 2026-06-22: added a non-default Canvas indexed-quad diagnostic report mode for same-workload Canvas fallback A/B tests.
- 2026-06-02: added browser WebGPU command-family current-versus-legacy-rebind A/B rows and report gates before the default legacy row was retired.
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
