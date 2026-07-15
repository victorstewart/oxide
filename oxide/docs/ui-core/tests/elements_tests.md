# oxide-ui-core::tests::elements_tests

## Intention and purpose

These integration tests freeze element state and emitted renderer commands. C60 adds explicit coverage that `ImageRegionView` cover fitting cannot sample outside its atlas slot.

## Relation to the rest of the code

The tests construct public UI elements, encode into `DrawListBuilder`, and inspect renderer-api commands. Text and interaction cases also exercise the production UI helpers used by authoring surfaces.

## Entry points list

The file covers image fitting/zoom/clipping, overlays/popups, badges, controls, pickers, text caching/layout/input, camera command emission, and pointer/keyboard state. `image_region_view_cover_keeps_crop_inside_atlas_slot` is the C60 regression case.

## Logic narrative

The image-region helper extracts the single emitted image command. The regression fixture supplies a nonzero atlas offset, a destination with a different aspect ratio, and cover mode, then verifies the crop is calculated relative to the slot while the emitted source stays within its exact bounds.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

Each image helper expects exactly one image draw for valid geometry. Rectangle comparisons use a small floating-point tolerance. No unsafe code is used.

## Edge cases and failure modes

Neighbor bleed can recur if cover math accidentally uses texture-global dimensions or fails to add the slot origin. Existing cases also cover empty clips, nonfinite bounds, alpha rejection, odd dimensions, zoom/pan, and each fit mode.

## Concurrency and memory behavior

Tests are synchronous and operate on owned command lists. Allocation behavior is covered separately by the UI text-frame allocation contract.

## Performance notes

The regression asserts command shape, not timing. It protects the one-command atlas path so performance work cannot require a temporary crop texture or extra clip pass.

## Feature flags and cfgs

No C60-specific cfg is used.

## Testing and benchmarks

Run `cargo test --locked -p oxide-ui-core --test elements_tests`. The full `oxide-ui-core` package suite covers neighboring element behavior.

## Examples

See `image_region_draw` for extracting destination/source/alpha from the encoded draw list.

## Changelog

- 2026-07-15: added atlas-offset cover-crop coverage for `ImageRegionView`.
