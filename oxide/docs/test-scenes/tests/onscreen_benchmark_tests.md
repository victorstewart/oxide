# oxide-test-scenes::tests::onscreen_benchmark_tests

## Intention and purpose

These tests keep the headline on-screen benchmark key list honest. They verify that every new UI object and common animation case can be prepared and stepped through the Rust scene router.

## Relation to the rest of the code

- Upstream callers:
  - `cargo test -p oxide-test-scenes --test onscreen_benchmark_tests`.
- Downstream dependencies:
  - `oxide_test_scenes::Router` supplies the benchmark prepare/step API.
  - `oxide_ui_core::elements::ImageUploader` is implemented by a null uploader because these tests validate routing, not GPU upload behavior.

## Entry points list

- `headline_component_onscreen_benchmarks_prepare_and_step()`
  - Checks label, progress bar, spinner, button, toggle, slider, image view, nine-slice image, and collection-view benchmark keys.
- `headline_animation_onscreen_benchmarks_prepare_and_step()`
  - Checks indeterminate progress, button press scale, toggle thumb spring, and slider thumb move benchmark keys.

## Logic narrative

Each test creates a fresh router per case, calls `prepare_onscreen_benchmark`, asserts that the expected `SceneKind` was selected, and then calls `step_onscreen_benchmark`. A fresh router per row prevents hidden state from one benchmark key from making another key pass.

## Preconditions and postconditions

- Preconditions:
  - The test-scenes crate must compile with the UI-core element API.
- Postconditions:
  - All headline benchmark keys are routable by the Rust scene router.
- Invariants maintained:
  - Test-only image upload behavior remains isolated in the test file.

## Edge cases and failure modes

The tests intentionally do not validate pixels or performance values. Device visual validation and timing are owned by the iOS host perf harness.

## Concurrency and memory behavior

The tests run single-threaded router mutations and use no real GPU resources.

## Performance notes

The null uploader avoids texture work so these tests stay focused and cheap; the actual device benchmarks still exercise real host rendering.

## Testing and benchmarks

These tests are the local correctness gate for benchmark key coverage. Device performance statistics are generated separately by `cargo run --locked -j$(sysctl -n hw.ncpu) -p xtask -- ios compare-device-perf`.

## Examples

```rust
let mut router = Router::new(NullUploader);
assert!(router.prepare_onscreen_benchmark("animation_slider_thumb_move"));
assert!(router.step_onscreen_benchmark("animation_slider_thumb_move", 1));
```

## Changelog

- 2026-04-26: Added coverage for headline on-screen UI object and animation benchmark keys.
