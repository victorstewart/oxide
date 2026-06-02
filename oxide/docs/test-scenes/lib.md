# oxide-test-scenes::lib

## Intention and purpose

`oxide-test-scenes` provides the reusable scene router used by the host app, snapshot tooling, and device performance harnesses. It keeps visual workloads in Rust/Oxide so iOS shells only deliver host surfaces and raw events.

## Relation to the rest of the code

- Upstream callers:
  - `oxide/host/ios-app/oxide-host-ios/src/lib.rs` drives the router through FFI for app frames and on-screen benchmarks.
  - `oxide/crates/perf-runner` uses the scenes for offscreen workspace performance cases.
  - Snapshot and host diagnostics use the same router to render canonical scenes.
- Downstream dependencies:
  - `oxide-ui-core` provides controls, text, collection, animation, camera, and draw-list primitives.
  - `oxide-renderer-api` supplies draw commands, rectangles, colors, and image handles.
  - `oxide-input` derives surface pinch deltas from raw host touch contacts.

## Entry points list

- `oxide_test_scenes::Router<U>::new(uploader: U) -> Self`
  - Builds the scene router and all scene state.
  - Main callers: host app and tests.
- `oxide_test_scenes::Router<U>::prepare_onscreen_benchmark(&mut self, benchmark: &str) -> bool`
  - Selects and resets the scene used by a named on-screen device benchmark.
  - Main callers: iOS host FFI.
- `oxide_test_scenes::Router<U>::step_onscreen_benchmark(&mut self, benchmark: &str, step: usize) -> bool`
  - Advances one named benchmark step before the host renders the next visible frame.
  - Main callers: iOS host FFI.
- `oxide_test_scenes::Router<U>::draw(&mut self, viewport: RectF, device_scale: f32, b: &mut DrawListBuilder)`
  - Encodes the active scene into a draw list.
  - Main callers: host app frame path and snapshot/perf tools.
- `oxide_test_scenes::Router<U>::take_damage_into(&mut self, out: &mut Vec<RectI>)`
  - Moves the last frame's damage rectangles into caller-owned reusable storage.
  - Main callers: allocation-audited host frame loops.
- `oxide_test_scenes::Router<U>::input_touch(&mut self, event: &TouchEvent)`
  - Feeds raw touch contacts into the Oxide-owned surface recognizer and forwards one-finger pans plus pinch deltas to scenes that support them.
  - Main callers: iOS host raw touch callback.

## Logic narrative

The router owns one state object per scene and switches between them by `SceneKind`. On-screen benchmarks reuse those scenes instead of introducing benchmark-only renderers. Headline component cases map to the smallest existing scene that renders the target object: text layout for labels, controls for progress/spinner/button/toggle/slider, zoom image for image views, nine-slice for resizable imagery, and collection stress for collection views. Headline animation cases use the controls scene for indeterminate progress, button scale, toggle spring, and slider thumb motion, plus the existing zoom-image and animation-timeline scenes.

`prepare_onscreen_benchmark` resets state so every measurement pass starts from a known scene. `step_onscreen_benchmark` performs one deterministic mutation, sharing the common button and collection-focus step mechanics while preserving case-specific action labels, then the host renders a real MetalView frame. This keeps product behavior and gesture/control state in Rust while UIKit remains only the host shell.

Raw touch input follows the same ownership rule. The host forwards each `TouchEvent`, the router updates a `TouchSurfaceRecognizer`, one-finger pan events are replayed through the existing pointer-drag entry point, and recognized pinch ratios are applied through the existing scene-level pinch entry points for Zoom Image and Camera. Two-touch center pan events emitted by the recognizer are not replayed as one-finger drags because pinch surfaces cancel drag ownership while two touches are active. Scene switches reset the recognizer so stale contacts cannot leak across benchmark or product scene boundaries.

## Preconditions and postconditions

- Preconditions:
  - The router must have a valid image uploader.
  - Text benchmarks need loaded fonts to produce glyph runs.
  - Host-side on-screen benchmarks must call prepare before step.
- Postconditions:
  - A successful prepare selects the expected scene and resets its benchmark state.
  - A successful step mutates only the active benchmark's scene state.
- Invariants maintained:
  - UIKit does not own scene-specific benchmark state.
  - Component and animation cases share production scene code.
  - Test code remains in `tests/`, not inside source modules.

## Edge cases and failure modes

Unknown benchmark names return `false` so the FFI layer can fail the benchmark explicitly. Missing fonts can reduce label draw output, but the benchmark still runs through the same scene path and host validation catches blank output on device.

Unknown or unsupported touch gestures are ignored. Invalid coordinates are filtered by `oxide-input`, two-touch pan is ignored at the router boundary, and scene changes clear active touch state before the next scene receives input.

## Concurrency and memory behavior

The router is single-threaded scene state. Benchmark stepping reuses existing scene allocations after prepare; per-frame allocation behavior is governed by the underlying UI-core scene primitives and renderer. Host frame loops can use `take_damage_into` to keep damage handoff storage caller-owned after warmup.

Raw touch recognition stores a bounded inline set of active contacts with overflow only for unusually high touch counts. The router consumes the recognizer synchronously on the host input thread.

## Performance notes

The headline cases deliberately avoid new benchmark-only abstractions. Reusing existing scenes keeps code surface small and ensures the measured cost includes the same draw-list paths app authors use. Damage handoff supports caller-owned vector reuse so browser/host allocation audits can distinguish scene damage content from per-frame storage churn. The default Controls scene keeps static label/button text out of the per-frame allocation path.

## Testing and benchmarks

- `oxide/crates/test-scenes/tests/onscreen_benchmark_tests.rs` verifies the new headline benchmark keys prepare the expected scenes and accept a step.
- `oxide/crates/test-scenes/tests/onscreen_benchmark_tests.rs` verifies raw two-touch pinch events change the Zoom Image scene through the router without applying two-touch pan as a drag.
- `oxide/crates/test-scenes/tests/damage_rect_tests.rs` verifies damage scene switching, partial damage, and caller-owned damage storage reuse.
- Device benchmark rows are selected by `oxide/xtask/src/lib.rs` and persisted under `oxide/benchmarks/oxide-device/`.

## Examples

```rust
let mut router = oxide_test_scenes::Router::new(uploader);
assert!(router.prepare_onscreen_benchmark("component_button_encode"));
assert!(router.step_onscreen_benchmark("component_button_encode", 1));
```

## Changelog

- 2026-06-02: Added `take_damage_into` so allocation-audited hosts can reuse caller-owned damage storage.
- 2026-06-02: Removed per-frame static label/button string allocations from the Controls scene draw path.
- 2026-05-16: Merged duplicate Controls-scene benchmark prepare arms into one grouped reset.
- 2026-05-13: Collapsed duplicate spinner, slider, and nine-slice on-screen benchmark step bodies while preserving their case-specific action labels.
- 2026-05-11: Removed the redundant component-benchmark reset assignment for `Controls::progress_indeterminate`; `Controls::default()` already starts determinate progress.
- 2026-05-09: Filtered router touch-pan forwarding to one-finger pans so pinch does not also drag Zoom Image or Camera state.
- 2026-05-05: Shared repeated button and collection-focus benchmark step mechanics inside the router.
- 2026-04-26: Added headline on-screen component and animation benchmark keys for matched Oxide/UIKit device statistics, with identical prepare resets grouped by scene state.
- 2026-04-29: Routed raw touch pinch events through the scene router instead of dropping them at the new hook.
