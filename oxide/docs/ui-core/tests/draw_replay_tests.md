# ui-core::tests::draw_replay_tests

## Intention and purpose

These integration tests verify `oxide_ui_core::draw_replay` behavior through the public renderer API. They exist to protect translated replay paths used by CPU composition and web/layer fallback renderers.

## Relation to the rest of the code

The tests construct synthetic `DrawList` values and replay them into a recording `RenderEncoder`. The recording encoder captures clips, rectangles, centers, vertex-backed solids, and resolved glyph runs so assertions can compare the observable encoder calls rather than private helper state.

Call flow:

- test builds a draw list with mixed command types
- test calls `replay_drawlist`
- recording encoder stores each replayed command
- assertions verify translated geometry and clip restoration

## Entry points list

- `replay_translates_primitives_and_restores_fallback_clip()`: verifies translated primitive geometry, translated glyph vertices, and fallback clip restoration.
- `replay_skips_invalid_solid_vertex_span()`: verifies invalid solid spans do not emit solid geometry.
- `replay_recovers_from_unbalanced_clip_stack()`: verifies non-empty clip stacks are reset to fallback on return.

## Logic narrative

`RecordingEncoder` implements `RenderEncoder` and stores command arguments in vectors. The main replay test uses one shared vertex and index backing store for both a solid and a glyph run. It replays with a non-zero origin, then checks that the solid vertices and glyph vertices both moved by the same origin while glyph indices stayed unchanged.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

The tests use only public API types and do not rely on renderer internals. They assert replay output through encoder calls. No unsafe code is used.

## Edge cases and failure modes

The invalid-span test protects the no-geometry solid path. The unbalanced-clip test protects cleanup after a missing `ClipPop`. The glyph assertion protects the edge case where resolved glyph geometry would otherwise bypass origin translation.

## Concurrency and memory behavior

The tests are single-threaded. The recording encoder clones small vectors to preserve replayed arguments for later assertions.

## Performance notes

The tests are narrow unit-style integration tests and do not benchmark. They intentionally use tiny geometry so they remain cheap in focused package test runs.

## Feature flags and cfgs

The tests have no feature flags or target-specific behavior.

## Testing and benchmarks

Run with:

```bash
cargo test --locked -j$(sysctl -n hw.ncpu) -p oxide-ui-core --test draw_replay_tests
```

## Examples

```rust
pub fn smoke()
{
   let list = oxide_renderer_api::DrawList::default();
   let _ = oxide_ui_core::draw_replay::viewport_clip(oxide_renderer_api::RectF::new(0.0, 0.0, 1.0, 1.0));
   assert!(list.items.is_empty());
}
```

## Changelog

- 2026-05-14: Added resolved glyph vertex assertions so translated replay keeps text aligned with other primitives.
