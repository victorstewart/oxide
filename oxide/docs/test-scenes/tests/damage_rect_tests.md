# oxide-test-scenes::tests::damage_rect_tests

## Intention and purpose

These tests protect router damage behavior and warmed draw allocation invariants that browser and host frame loops rely on.

## Relation to the rest of the code

- Upstream callers:
  - `cargo test -p oxide-test-scenes --test damage_rect_tests`.
- Downstream dependencies:
  - `oxide_test_scenes::Router` supplies scene drawing and damage handoff.
  - `tests/helpers.rs` supplies `NullUploader`.
  - `oxide-wasm-alloc-counter` supplies allocation counters for warmed draw assertions.

## Entry points list

- `damage_lab_scene_switch_forces_one_full_redraw_before_partial_damage`
  Verifies that entering the damage lab emits one full redraw before steady partial damage.
- `damage_handoff_can_reuse_caller_storage`
  Verifies `take_damage_into` preserves caller-owned storage across frames.
- `warmed_overlay_draw_reuses_text_scratch_without_allocating`
  Warms the default router draw path with overlay visible, then verifies a second draw performs no heap allocation or reallocation when using caller-owned damage storage.

## Logic narrative

The allocation-sensitive test warms text caches, atlas contents, draw-list capacity, overlay string capacity, and damage storage before measuring. The measured draw mirrors the browser frame-loop shape: draw into a reused builder, keep overlay visible, and use `take_damage_into` rather than the capacity-dropping `take_damage` helper.

## Preconditions and postconditions

- Preconditions:
  - Tests must run in a native test process where `oxide-wasm-alloc-counter` can wrap the system allocator.
- Postconditions:
  - Warm overlay draw remains allocation-free for the default Controls scene.
  - Caller-owned damage storage stays reusable after handoff.

## Edge cases and failure modes

The allocation test intentionally targets the default overlay path used by the browser frame-loop benchmark. Other scene-specific overlay extras may still allocate until they receive dedicated scratch paths.

## Concurrency and memory behavior

The allocation-sensitive test serializes through a process-local mutex because the test binary uses one global allocator.

## Performance notes

This test protects the hot path that removes the remaining `router_draw` allocation from the browser WebGPU frame-loop allocation stage.

## Testing and benchmarks

Run with `cargo test --locked -p oxide-test-scenes --test damage_rect_tests -- --nocapture`.

## Examples

```rust
let mut router = oxide_test_scenes::Router::new(NullUploader);
let mut builder = oxide_ui_core::DrawListBuilder::new();
router.draw(viewport, 1.0, &mut builder);
```

## Changelog

- 2026-06-02: Added warmed overlay draw allocation coverage for router-owned overlay text scratch reuse.
