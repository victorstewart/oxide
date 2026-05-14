# ui-core::draw_replay

## Intention and purpose

`draw_replay` replays an `oxide_renderer_api::DrawList` through a generic `RenderEncoder` while applying a fixed origin translation. It exists for CPU-side composition paths, layer fallback rendering, and web renderer paths that need to draw an already-built list into a translated target.

## Relation to the rest of the code

The module sits between `DrawListBuilder` output and renderer-specific encoders. Renderers or host adapters pass a draw list, fallback clip, and origin into `replay_drawlist`, and the helper translates geometry before calling the encoder methods. It is especially relevant to web and layer-like paths because glyphs are now resolved from shared draw-list vertex and index spans before being replayed.

Call flow:

- caller builds a `DrawList`
- caller chooses a fallback clip and origin
- `replay_drawlist` translates clips, rectangles, centers, and vertex-backed geometry
- the supplied `RenderEncoder` receives commands in list order

## Entry points list

- `oxide_ui_core::draw_replay::viewport_clip(rect: RectF) -> RectI`: converts a floating viewport to an integer clip rectangle.
- `oxide_ui_core::draw_replay::replay_drawlist(list: &DrawList, encoder: &mut dyn RenderEncoder, fallback_clip: RectI, origin: [f32; 2])`: replays commands through a generic encoder with origin translation and fallback clip restoration.

## Logic narrative

`replay_drawlist` rounds the origin for integer clip translation and keeps the floating origin for geometry. It sets the translated fallback clip first, then walks commands in order. Solid and glyph commands resolve their vertex spans from the draw-list backing storage; solid vertices and glyph vertices are translated before they are passed to the encoder. Rect-based commands translate their rectangles, spinner commands translate their center, and clip pushes are translated on the integer clip stack. Layer markers are ignored because this helper replays already-selected command bodies rather than managing retained layer caches.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

The draw list may contain invalid spans; invalid vertex or index spans are treated as empty slices and the downstream encoder decides whether it can draw. The fallback clip is restored when clip nesting unwinds, and an unbalanced non-empty clip stack is reset to the translated fallback before return. The module uses no unsafe code.

## Edge cases and failure modes

Empty draw lists still set the fallback clip. Invalid solid vertex spans are skipped because there is no geometry to draw. Invalid glyph spans replay as empty resolved geometry, matching the generic encoder fallback while preventing web backends from reading outside the list storage. Non-finite coordinates are not sanitized here because draw-list producers and renderer backends own those validation contracts.

## Concurrency and memory behavior

The helper is single-threaded and borrows the caller-provided draw list and encoder. Solid and glyph replay allocate translated vertex buffers proportional to the resolved vertex span length. Other command types do not allocate.

## Performance notes

The function is linear in command count plus translated vertex count. It avoids copying full draw lists and translates only the vertex slices needed by vertex-backed commands. Glyph replay keeps indices unchanged because glyph indices are relative to the resolved run vertex span.

## Feature flags and cfgs

The module has no feature flags or target-specific code.

## Testing and benchmarks

`oxide/crates/ui-core/tests/draw_replay_tests.rs` covers primitive translation, clip restoration, invalid solid spans, unbalanced clips, and glyph resolved-vertex translation.

## Examples

```rust
pub fn replay_into_encoder(
   list: &oxide_renderer_api::DrawList,
   encoder: &mut dyn oxide_renderer_api::RenderEncoder,
)
{
   let clip = oxide_ui_core::draw_replay::viewport_clip(oxide_renderer_api::RectF::new(0.0, 0.0, 320.0, 240.0));
   oxide_ui_core::draw_replay::replay_drawlist(list, encoder, clip, [8.0, 12.0]);
}
```

## Changelog

- 2026-05-14: Translated resolved glyph vertices during replay so web and layer fallback paths place text at the same origin as every other primitive.
