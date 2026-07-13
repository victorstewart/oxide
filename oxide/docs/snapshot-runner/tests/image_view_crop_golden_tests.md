# oxide-snapshot-runner tests `image_view_crop_golden_tests.rs`

## Intention and purpose

Freeze `ImageView` contain, cover, stretch, zoom, pan, alpha, and odd-source sampling at DPR 1, 2, and 3 while proving the cropped `Image` path matches the clipped zero-inset `NineSlice` reference it replaces.

## Relation to the rest of the code

The test encodes through `oxide_ui_core::elements::ImageView`, submits through the production macOS `MetalRenderer`, and compares committed PNGs under `goldens/images`.

## Entry points list

- `image_view_contain_cover_stretch_zoom_and_pan_match_dpr_goldens`: renders all five mapping modes at three device scales, rejects image-view clip or nine-slice commands, and compares candidate pixels with the clipped parent reference and committed golden.

## Logic narrative

A 7x5 asymmetric RGBA texture makes horizontal and vertical source cropping observable. Each DPR uses physical target dimensions equal to logical dimensions times scale. The candidate scene emits five `Image` commands; the reference scene repeats the former fitted destination and zero-slice nine-slice mapping under the minimum view clip. Exact equality proves that moving clipping into the source rectangle preserves intended pixels.

## Preconditions and postconditions

The test requires macOS Metal. Every readback must be exact RGBA at the declared physical dimensions, and no tolerance is applied.

## Edge cases and failure modes

Odd source dimensions, fractional crop boundaries, cover overflow, zoom magnification, pan offset, contain letterboxing, stretch, alpha, command-family regression, or DPR drift fail the test.

## Concurrency and memory behavior

Each scale owns two renderers and two image textures. Blocking readback is test-only; all resources are released before the next scale.

## Performance notes

This is correctness evidence. Timed 100/1,000-image authoring and Metal rows live in `oxide-perf-runner`.

## Feature flags and cfgs

The integration test is macOS-only and uses renderer-metal's snapshot support through the snapshot-runner dependency.

## Testing and benchmarks

Run `cargo test --locked -p oxide-snapshot-runner --test image_view_crop_golden_tests`.

## Examples

Set `UPDATE_GOLDENS=1` only when intentionally reviewing a source-mapping change at all three DPRs.

## Changelog

- 2026-07-13: added C14 clipped-parent parity and DPR 1/2/3 image-view crop goldens.
