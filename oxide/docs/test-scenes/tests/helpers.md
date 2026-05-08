# oxide-test-scenes::tests::helpers

## Intention and purpose

`helpers` holds shared integration-test scaffolding for `oxide-test-scenes`. It exists so routing, damage, and camera-preview tests do not each carry their own copy of the same null image uploader.

## Relation to the rest of the code

- Upstream callers:
  - `oxide/crates/test-scenes/tests/camera_preview_tests.rs`
  - `oxide/crates/test-scenes/tests/damage_rect_tests.rs`
  - `oxide/crates/test-scenes/tests/onscreen_benchmark_tests.rs`
- Downstream dependencies:
  - `oxide_ui_core::elements::ImageUploader` defines the uploader trait.
  - `oxide_renderer_api::ImageHandle` supplies the inert test handle.

## Entry points list

- `helpers::NullUploader`
  - Implements `ImageUploader` by returning `ImageHandle(0)` and ignoring updates.

## Logic narrative

The helper gives tests an uploader that satisfies scene construction without allocating textures or invoking a renderer. Tests that only inspect routing, draw-list shape, damage rectangles, or scene state can therefore share one tiny uploader.

## Preconditions and postconditions

- Preconditions:
  - Tests must not rely on uploaded image contents when using `NullUploader`.
- Postconditions:
  - `create_a8` returns a stable inert handle.
  - `update_a8` has no side effects.

## Edge cases and failure modes

Pixel-content tests should use a real uploader or renderer fixture instead of this helper. The helper intentionally cannot distinguish individual image uploads.

## Concurrency and memory behavior

`NullUploader` owns no state, allocates no memory, and is used by single-threaded integration tests.

## Performance notes

The helper keeps local correctness tests cheap by avoiding texture allocation and upload work.

## Testing and benchmarks

The helper is exercised indirectly by the camera preview, damage-rect, and on-screen benchmark integration tests.

## Examples

```rust
let mut router = oxide_test_scenes::Router::new(NullUploader);
assert!(router.prepare_onscreen_benchmark("component_button_encode"));
```

## Changelog

- 2026-05-05: Extracted the repeated null uploader into shared test helpers.
