# oxide-host-web::tests::lib_tests

## Intention and purpose

These tests verify native-testable support code in the WebAssembly host. They exist so the procedural image used by the browser demo has deterministic dimensions and alpha, and so the static browser shell keeps its WebGPU smoke, benchmark, report, and golden-capture hooks wired.

## Relation to the rest of the code

The tests call `oxide_host_web::generate_checker_rgba`, which is used by the wasm host to seed the Zoom scene when no external image bundle exists.

Call flow:

- cargo test
- `oxide-host-web/tests/lib_tests.rs`
- `oxide_host_web::generate_checker_rgba`
- web host image upload during browser startup

## Entry points list

- `checker_texture_has_expected_size_and_alpha()`: verifies byte count and full opacity.
- `checker_texture_alternates_tiles()`: verifies the generated image is not a flat color.
- `static_shell_imports_generated_pkg_and_platform_smoke_hook()`: verifies the static page imports `www/pkg`, exposes browser platform/WebGPU smoke reports, runs sampled frame, current ID-mask, current upload, effect-uniform A/B, backdrop-batch current, Scene3D A/B, mixed matrix, layer/effects matrix, clean-layer A/B, command-family matrix, glyph-run current, neon-marker A/B, and direct-surface A/B benchmarks, wraps benchmark families in browser User Timing marks, supports the non-default Canvas indexed-quad diagnostic report path, supports the `capture_target` / `capture_only` browser capture path and startup-only report path, invokes deterministic app and ID-mask snapshot hooks, waits an animation frame after ID-mask capture, and writes the hidden JSON report hook for script capture.
- `host_exposes_webgpu_id_mask_ab_benchmark()`: verifies the wasm host keeps the current-only default WebGPU ID-mask benchmark, the explicit current-vs-legacy ID-mask diagnostic, upload, upload-scratch, effect-uniform, current backdrop-batch, Scene3D, mixed, layer/effects, clean-layer, command-family, glyph-run, neon-marker, direct-surface, and explicit diagnostic draw-item coalescing and draw-state benchmark exports, verifies the retired clip-state diagnostic export stays deleted, exports p50/p95/p99/peak/avg plus missed-frame/hitch, Rust/WASM allocation fields, and frame-stage allocation fields, exposes the explicit app and ID-mask browser snapshot render hooks, and keeps the direct-capture guard that prevents resize/redraw events from repainting the app over the ID-mask capture.
- `committed_webgpu_id_mask_golden_is_present_and_sized()`: verifies the committed browser WebGPU ID-mask compositor golden is present at 512x512.
- `committed_webgpu_id_mask_golden_contains_rendered_pixels()`: decodes the committed browser WebGPU ID-mask compositor golden and checks that it contains a colorful full-mask compositor output instead of the normal app canvas or an untouched surface.
- `committed_webgpu_glyph_golden_is_present_and_sized()`: verifies the dedicated A8/SDF atlas golden is present at 512x512.
- `committed_webgpu_glyph_golden_contains_a8_and_sdf_pixels()`: decodes the glyph golden and requires ordinary bright A8 rows, cyan SDF rows, and the dark background.
- `webgpu_browser_capture_script_compares_pixels_against_golden()`: verifies the browser recapture script still compares pixels, supports app, Scene3D, and ID-mask capture targets, retries transient blank/mismatched visual captures before report writes, can write startup-only repeat reports, can write non-default Canvas indexed-quad reports, and can write JSON/Markdown WebGPU baseline reports with startup/package evidence, pacing, pass-family, timestamp-attribution, GPU timestamp stage breakdown, duplicate benchmark-report Chrome trace, per-benchmark User Timing marks and trace intervals, resource-lifetime, report-level and per-row warm-resource-churn, Rust/WASM allocation-audit, frame-loop and submit-substage allocation attribution, backend-path coverage, current upload direct timestamp totals, effect-uniform direct GPU timestamp totals, current backdrop-batch, Scene3D, and pixel-check fields.
- `c16_geometry_adapter_covers_compact_and_fallback_streams()`: requires the real-Chrome adapter and host export to retain 10,000-glyph, 10,000-image, and 70,002-vertex workloads plus selected warmup/sample persistence.
- `committed_webgpu_browser_baseline_persists_nonzero_id_mask_current_row()`: parses `benchmarks/web/latest.json` and verifies the 23-row browser WebGPU matrix is present with report version 5, browser startup/package fields, nonzero current ID-mask timing, frame-pacing fields, pass-family counters, GPU timestamp stage totals reconciled to source rows, Chrome trace event counts, benchmark User Timing labels, and per-benchmark trace intervals from the duplicate benchmark-report run, current-row Rust/WASM allocation counters with bounded per-frame budgets and zero reallocations, frame-loop allocation stage totals, submit-substage allocation totals, zero WASM memory growth across benchmark marks after prewarm, zero warm-frame sampler creation, report-level and per-row current-row warm-resource-churn zero-growth summaries, backend-path coverage rows tying important WebGPU path families to distributions and explanatory counters, current glyph/RGBA upload rows with direct timestamp totals and retired legacy upload rows absent, effect-uniform A/B with direct GPU timestamp totals, current backdrop-batch coverage, mixed text/image/effects A/B, layer/damage/effects A/B, clean-layer A/B, command-family current coverage with the legacy row absent, glyph-run current, neon-marker A/B, direct-surface A/B, the Scene3D stress rows, and current-path wins.

## Logic narrative

The first test checks RGBA buffer shape. The second test samples different tile positions and confirms they differ, which catches accidental one-color placeholder output. The static shell test catches regressions where the HTML page points at the wrong wasm-bindgen output path, stops invoking the backend smoke and perf hooks, stops probing timestamp-query capability, stops marking default benchmark families with browser User Timing, stops publishing the hidden report JSON, stops honoring capture-target query parameters, stops waiting after ID-mask capture, stops using the no-RAF deterministic app snapshot path for app captures, stops supporting startup-only repeat reports, or stops logging the browser-test markers. The source-inspection tests keep the browser-only WebGPU A/B exports that remain after upload retirement, explicit app/Scene3D/ID-mask snapshot render hooks, direct-capture guard, bounded visual-capture retry, startup/package report evidence, repeated startup/package measurement output, upload and effect-uniform GPU timestamp fields, timestamp stage-breakdown reporting, current backdrop-batch coverage, mixed-scene A/B, clean-layer A/B, command-family current coverage, glyph-run current, neon-marker A/B, direct-surface A/B, diagnostic draw-item coalescing A/B, Chrome trace, benchmark mark, trace interval, zero WASM memory-growth, Rust/WASM allocation counters, frame-stage and submit-substage allocation counters, warm-resource-churn report contracts, and backend-path coverage visible to native CI without launching Chrome. The same source test also asserts the retired clip-state diagnostic export, upload A/B export, default backdrop/upload legacy rows, and command-family legacy row stay absent after their A/B wins. The committed-golden tests decode browser PNGs so missing files, wrong dimensions, blank captures, and app-vs-compositor target mixups fail in native tests. The persisted-report test prevents committed browser baselines from silently dropping report version 5, startup/package metrics, current ID-mask default coverage, frame-pacing fields, pass-family counters, timestamp-attribution status, GPU timestamp stage totals, duplicate benchmark-report Chrome trace event counts, benchmark labels, per-benchmark trace intervals, current-row Rust/WASM allocation counters with zero reallocations, frame-loop allocation stage totals, submit-substage allocation totals, zero WASM memory growth after prewarm, resource-lifetime counters, current glyph/RGBA upload and effect-uniform counters with direct GPU timestamp totals, current backdrop-batch counters, mixed-scene state/effect counters, clean-layer cache counters, command-family current counters, glyph-run current counters, neon-marker counters, direct-surface pass/GPU timestamp counters, the report-level and per-row current-row warm-resource-churn zero-growth summaries, backend-path coverage rows, capture target, or from regressing back to virtual-time zero measurements.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

The tests require no wasm runtime. Generated images are always RGBA8 and fully opaque.

## Edge cases and failure modes

Small dimensions still allocate a correctly sized buffer. Tile alternation is checked with a width crossing the tile boundary. The static HTML test uses `include_str!` so it fails at compile time if the shell is moved without updating the test.

## Concurrency and memory behavior

The function allocates one vector sized to `width * height * 4`.

## Performance notes

Generation is linear in pixel count and is used only during host startup.

## Feature flags and cfgs

These tests run on native targets. The wasm host entry points are compile-checked with the wasm target and verified through the browser page.

## Testing and benchmarks

Run with `cargo test --locked -p oxide-host-web --test lib_tests`.

## Examples

```rust
pub fn texture() -> Vec<u8>
{
   oxide_host_web::generate_checker_rgba(16, 16)
}
```

## Changelog

- 2026-07-12: added static C16 adapter coverage for compact u16 quad streams and the large-mesh u32 fallback.
- 2026-07-12: added static C15 atlas diagnostic/capture coverage and decoded A8/SDF glyph-golden assertions.

- 2026-07-12: added static coverage for the two-draw asymmetric ID-mask oracle and uniform arena counters in host and browser report schemas.
- 2026-06-22: updated static and committed-report checks after retiring the default backdrop-batch per-copy row with same-workload A/B proof.
- 2026-06-02: added static and committed-report checks for WebGPU draw-item coalescing A/B rows and counters.
- 2026-06-22: retired default committed-report checks for draw-item coalescing, draw-state cache, and clip-state cache standalone rows after same-workload A/B proof showed current wins.
- 2026-06-22: updated source-inspection checks after retiring the explicit clip-state diagnostic export with repeated startup/package A/B proof.
- 2026-06-22: added static and committed-report checks for browser startup timing and package-size evidence in WebGPU report version 3.
- 2026-06-22: updated committed-report checks for browser WebGPU report version 5 after retiring the default upload legacy rows and upload A/B export with same-workload A/B proof.
- 2026-06-22: updated static and committed-report checks after retiring the default glyph-run legacy row with same-workload A/B proof.
- 2026-06-22: added static checks for the non-default repeated startup/package report mode.
- 2026-06-22: added static checks for the non-default Canvas indexed-quad diagnostic report mode.
- 2026-06-02: added static and committed-report checks for browser WebGPU Rust/WASM allocation audit fields and summary gates.
- 2026-06-02: added static and committed-report checks for WebGPU timestamp stage-breakdown attribution.
- 2026-06-02: added static and committed-report checks for browser WebGPU frame-loop allocation stage attribution.
- 2026-06-01: added static and committed-report checks for WebGPU benchmark User Timing marks and trace labels.
- 2026-06-02: added static and committed-report checks for per-benchmark Chrome trace interval attribution.
- 2026-06-02: added static and committed-report checks for per-row WebGPU warm-resource-churn zero-growth details.
- 2026-06-02: added static and committed-report checks for the WebGPU backend-path coverage matrix.
- 2026-06-02: added static and committed-report checks for WebGPU submit sub-stage WASM allocation attribution.
- 2026-06-02: added static and committed-report checks for WebGPU glyph/RGBA upload direct timestamp totals.
- 2026-06-02: added static and committed-report checks for WebGPU mixed-scene current-versus-legacy A/B rows.
- 2026-06-02: added static and committed-report checks for WebGPU layer/damage/effects current-versus-legacy A/B rows.
- 2026-06-02: added static and committed-report checks for WebGPU retained clean-layer clean-versus-dirty A/B rows.
- 2026-06-02: added static and committed-report checks for WebGPU neon-marker current-versus-legacy A/B rows.
- 2026-06-02: added static and committed-report checks for WebGPU direct-surface current-versus-forced-scene-present A/B rows.
- 2026-06-02: added static and committed-report checks for WebGPU glyph-run current-only rows.
- 2026-06-22: updated static and committed-report checks after retiring the default command-family legacy row with same-workload A/B proof.
- 2026-06-02: added static and committed-report checks for WebGPU command-family current-versus-legacy A/B rows before the default legacy row was retired.
- 2026-06-01: added static and committed-report checks for WebGPU effect-uniform direct GPU timestamp A/B totals.
- 2026-06-01: added static coverage for bounded WebGPU browser capture retries.
- 2026-06-01: added static and committed-report checks for WebGPU backdrop-batch A/B rows and counters.
- 2026-06-01: added static and committed-report checks for duplicate benchmark-report Chrome trace summary fields in the WebGPU browser baseline.
- 2026-06-01: added static and committed-report checks for the browser WebGPU warm-resource-churn summary.
- 2026-06-01: added static and committed-report checks for WebGPU effect-uniform A/B rows and counters.
- 2026-06-01: added committed-report checks for the 96-instance Scene3D stress rows.
- 2026-06-01: added committed-report checks for WebGPU pass-family attribution counters and timestamp-query capability status.
- 2026-06-01: added committed-report checks for WebGPU sampler lifetime counters.
- 2026-06-01: added static coverage for the direct-capture guard and post-render animation-frame settle used by WebGPU ID-mask browser golden capture.
- 2026-06-01: added static coverage for the deterministic fixed-timestamp app snapshot hook used by browser app golden capture.
- 2026-06-01: added native enforcement for the committed browser WebGPU ID-mask compositor golden and capture-target script hooks.
- 2026-05-31: expanded static-shell and script tests for hidden browser report output plus persisted WebGPU frame-loop and ID-mask A/B baseline writes.
- 2026-05-31: added committed web-baseline parsing coverage for nonzero WebGPU frame-loop and ID-mask current/legacy A/B rows.
- Added static shell coverage for the generated package import and platform smoke hook.
