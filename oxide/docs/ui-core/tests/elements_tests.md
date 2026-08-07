# oxide-ui-core::tests::elements_tests

## Intention and purpose

These integration tests freeze element state and emitted renderer commands. C60 adds explicit coverage that `ImageRegionView` cover fitting cannot sample outside its atlas slot.

## Relation to the rest of the code

The tests construct public UI elements, encode into `DrawListBuilder`, and inspect renderer-api commands. Text and interaction cases also exercise the production UI helpers used by authoring surfaces.

## Entry points list

The file covers image fitting/zoom/clipping, overlays/popups, badges, controls, pickers, text caching/layout/input, camera command emission, and pointer/keyboard state. `image_region_view_cover_keeps_crop_inside_atlas_slot` is the C60 regression case. `device_scale_change_invalidates_retained_text_without_republishing_pages` freezes the Retina retained-text boundary.

## Logic narrative

The image-region helper extracts the single emitted image command. The regression fixture supplies a nonzero atlas offset, a destination with a different aspect ratio, and cover mode, then verifies the crop is calculated relative to the slot while the emitted source stays within its exact bounds.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

Each image helper expects exactly one image draw for valid geometry. Rectangle comparisons use a small floating-point tolerance. No unsafe code is used.

## Edge cases and failure modes

Neighbor bleed can recur if cover math accidentally uses texture-global dimensions or fails to add the slot origin. Existing cases also cover empty clips, nonfinite bounds, alpha rejection, odd dimensions, zoom/pan, each fit mode, and stale retained glyph replay after a device-scale transition.

## Concurrency and memory behavior

Tests are synchronous and operate on owned command lists. Allocation behavior is covered separately by the UI text-frame allocation contract.

## Performance notes

The image regression asserts command shape, not timing. It protects the one-command atlas path so performance work cannot require a temporary crop texture or extra clip pass. The retained-text regression proves a 1x/3x scale change rejects stale geometry, appends only the new A8 glyph pixels, preserves the GPU page handle, reuses the resident 1x entry without another upload after switchback, and keeps the released no-argument frame API scale-correct after its first encode.

## Feature flags and cfgs

No C60-specific cfg is used.

## Testing and benchmarks

Run `cargo test --locked -p oxide-ui-core --test elements_tests`. The full `oxide-ui-core` package suite covers neighboring element behavior.

## Examples

See `image_region_draw` for extracting destination/source/alpha from the encoded draw list.

## Changelog

- 2026-08-06: added 1x to 3x retained-text invalidation and warm 1x switchback coverage without atlas-page recreation.
- 2026-07-15: added atlas-offset cover-crop coverage for `ImageRegionView`.
