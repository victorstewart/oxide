# oxide-text tests `owned_shape_replay_tests.rs`

## Intention and purpose
- Prove cached `OwnedShape` glyph replay avoids heap allocation after warmup on the normal LTR path.
- Preserve RTL owned-shape visual-order correctness while allowing the rare temporary reversed glyph buffer.

## Relation to the rest of the code
- Protects `oxide_ui_core::elements::TextCtx` cached label replay, which feeds WebGPU `GlyphRun` draw lists in the browser frame loop.
- Uses `oxide-wasm-alloc-counter` as a test-only global allocator so the native test can prove the same allocation invariant guarded by the browser WASM report.

## Entry points list
- `cached_ltr_owned_shape_replay_is_allocation_free_after_warmup`
  Warms an atlas and caller-owned draw buffers, then verifies a cached LTR owned shape replays with zero allocations and zero reallocations.
- `cached_rtl_owned_shape_replay_matches_direct_visual_order`
  Shapes Hebrew text when the macOS supplemental font is available and verifies owned-shape replay matches direct borrowed-shape visual output.

## Logic narrative
- The LTR test first pays shaping, rasterization, atlas insertion, and draw-buffer growth outside the measured section.
- The measured replay uses the same owned shape, already-resident atlas entries, and preallocated vertex/index buffers.
- The RTL test compares vertices and indices from `ShapeOutput::bake_into_with` against `OwnedShape::bake_into_with` so the optimized owned replay cannot silently change visual order.

## Preconditions and postconditions
- The Latin fixture font must remain available under `crates/text/tests/fixtures`.
- The RTL parity test skips when `/System/Library/Fonts/Supplemental/Arial Unicode.ttf` is unavailable.

## Edge cases and failure modes
- Spaces and other zero-bitmap glyphs are covered by the LTR string and must remain cached as no-geometry atlas entries.
- Allocation counters are serialized with a process-local mutex because the test binary uses one global allocator.

## Concurrency and memory behavior
- Tests use a global counting allocator and serialize allocation-sensitive sections.
- Passing LTR replay means no heap allocation or reallocation occurs between the measured allocation snapshots.

## Performance notes
- This test protects the hot path that reduced browser WebGPU frame-loop `router_draw` WASM allocations from 7 allocations/frame to 1 allocation/frame in the macOS Chrome/ANGLE Metal report.

## Feature flags and cfgs
- No feature-specific branches.

## Testing and benchmarks
- Run with `cargo test --locked -p oxide-text --test owned_shape_replay_tests -- --nocapture`.
- Browser companion: `node scripts/check_webgpu_browser_golden.mjs ... --json-report benchmarks/web/latest.json --markdown-report benchmarks/web/latest.md`.

## Examples
```rust
cargo test --locked -p oxide-text --test owned_shape_replay_tests -- --nocapture
```

## Changelog
- 2026-06-02: added allocation-free warm LTR owned-shape replay coverage and RTL owned-shape visual-order parity.
