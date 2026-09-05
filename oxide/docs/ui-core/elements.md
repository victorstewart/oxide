# oxide-ui-core::elements

## Intention and purpose

`elements` provides Oxide's author-facing UI element state and encoding primitives. C60 extends its image family with `ImageRegionView`, which draws a resolved subregion of a larger texture without losing contain, cover, stretch, alpha, or crop semantics.

## Relation to the rest of the code

Elements encode renderer-api draw commands through `DrawListBuilder`; they do not own renderer resources. `ImageView` uses a complete texture, while `ImageRegionView` accepts the source rectangle produced by `oxide-image-store::ResolvedImage` and keeps every derived crop inside that rectangle.

## Entry points list

The source unit groups buttons, labels/text input, images/camera, overlays/popups, pickers, sliders/switches, badges, and related state helpers. The C60 entry point is `ImageRegionView::encode` with public `image`, `source`, `fit`, and `alpha` fields. Existing `ImageFit` selects contain, cover, or stretch behavior.

## Logic narrative

`ImageRegionView::encode` and the unzoomed `ImageView` path share one inlined region-fit encoder. It derives width and height from the supplied texture region and emits one image draw whose source crop is offset into that region. Cover therefore crops the logical image, not the full atlas page, without duplicating fit semantics between standalone and atlas-backed views.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

Invalid or empty destination/source geometry and noncontributing alpha emit no draw. Emitted source coordinates remain within the supplied region, alpha is clamped, ordering follows the caller's builder sequence, and no unsafe code is introduced.

## Edge cases and failure modes

Zero-sized source regions, nonfinite bounds, empty clips, and zero alpha are ignored. Fractional crop coordinates are retained so odd image sizes do not gain integer-rounding seams.

## Concurrency and memory behavior

Element encoding is caller-thread owned and allocation-free for the image-region path. The view borrows no decoded bytes and stores only a renderer handle plus value geometry.

## Performance notes

One atlas-backed region emits the same single image command as a standalone image. No clip command or temporary texture is required for cover cropping, and resolved atlas identity remains available to the prepared renderer through the caller's chunk revision.

## Feature flags and cfgs

No C60-specific feature flag or target cfg is used.

## Testing and benchmarks

`tests/elements_tests.rs` freezes atlas-region cover cropping. C60's `gpu.authoring.image_store.atlas_grid_1000` case exercises the public resolved-region path with scrolling, release/reuse, and exact invalidation.

## Examples

```rust
ImageRegionView {
   image: resolved.texture,
   source: resolved.source,
   fit: ImageFit::Cover,
   alpha: 1.0,
}.encode(bounds, &mut builder);
```

## Changelog

- 2026-07-15: added `ImageRegionView` for contain/cover/stretch rendering inside generation-checked atlas regions.
