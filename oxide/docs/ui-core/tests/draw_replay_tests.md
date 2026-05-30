# ui-core::tests::draw_replay_tests

## Intention and purpose

These integration tests verify `oxide_ui_core::draw_replay` behavior through the public renderer API. They exist to protect translated replay paths used by CPU composition and web/layer fallback renderers.

## Relation to the rest of the code

The tests construct synthetic `DrawList` values and replay them into a recording `RenderEncoder`. The recording encoder captures clips, rectangles, centers, vertex-backed solids, image meshes, and resolved glyph runs so assertions can compare the observable encoder calls rather than private helper state.

Call flow:

- test builds a draw list with mixed command types
- test calls `replay_drawlist`
- recording encoder stores each replayed command
- assertions verify translated geometry and clip restoration

## Entry points list

- `replay_translates_primitives_and_restores_fallback_clip()`: verifies translated primitive geometry, translated image-mesh vertices, translated glyph vertices, and fallback clip restoration.
- `replay_skips_invalid_solid_vertex_span()`: verifies invalid solid spans do not emit solid geometry.
- `replay_rebases_absolute_image_mesh_indices_to_translated_span()`: verifies absolute image-mesh indices are rebased after replay slices and translates the selected vertex span.
- `replay_recovers_from_unbalanced_clip_stack()`: verifies non-empty clip stacks are reset to fallback on return.

## Logic narrative

`RecordingEncoder` implements `RenderEncoder` and stores command arguments in vectors. The main replay test uses one shared vertex and index backing store for solid, image-mesh, and glyph commands. It replays with a non-zero origin, then checks that the vertex-backed commands moved by the same origin while glyph indices stayed unchanged. The image-mesh rebasing test builds an offset vertex span with absolute backing-store indices and verifies replay passes compact local indices to the encoder.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

The tests use only public API types and do not rely on renderer internals. They assert replay output through encoder calls. No unsafe code is used.

## Edge cases and failure modes

The invalid-span test protects the no-geometry solid path. The unbalanced-clip test protects cleanup after a missing `ClipPop`. The image-mesh rebasing assertion protects the edge case where replay translates a compact vertex slice but receives indices that were authored against the full draw-list backing store. The glyph assertion protects the edge case where resolved glyph geometry would otherwise bypass origin translation.

## Concurrency and memory behavior

The tests are single-threaded. The recording encoder clones small vectors to preserve replayed arguments for later assertions.

## Performance notes

The tests are narrow unit-style integration tests and do not benchmark. They intentionally use tiny geometry so they remain cheap in focused package test runs.

## Feature flags and cfgs

The tests have no feature flags or target-specific behavior.

## Testing and benchmarks

Run with:

```bash
cargo test -j$(sysctl -n hw.ncpu) -p oxide-ui-core --test draw_replay_tests
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

- 2026-05-19: Added absolute-index rebasing coverage for translated image-mesh replay.
- 2026-05-14: Added resolved glyph vertex assertions so translated replay keeps text aligned with other primitives.
