# oxide-benchmark-spec::tests::visual_tests

## Intention and purpose

These integration tests freeze the calibrated static cross-framework admission
contract, the retained legacy diagnostic reducer, and exact-RGB localization
before controllers rely on their report shapes.

## Relation to the rest of the code

- Exercises only public `oxide_benchmark_spec` visual APIs.
- Creates bounded in-memory RGB/RGBA PNG fixtures with the production PNG codec.
- Guards the equivalence validator required by Apple and Web comparison runners.

Call flow:

- deterministic pixels and runtime geometry
  - PNG encoding
  - `compare_calibrated_static_pngs`
  - global, stable-interior, voxel, and acceptance assertions
- legacy diagnostic fixtures
  - `reduce_normalized_png_visual_parity`
  - report field, text-geometry, acceptance, or error assertion
- exact matched pixels
  - `compare_exact_static_pngs`
  - zero-difference or changed-channel assertion

## Entry points list

- `calibrated_thresholds_freeze_rapid_visual_contract()` freezes the five v5
  threshold fields.
- `calibrated_static_comparison_accepts_sparse_raster_noise_but_keeps_exact_diagnostics()`
  proves bounded noise can pass while exact RGB still localizes it.
- `calibrated_static_report_is_byte_deterministic()` repeats v5 reduction and
  requires byte-identical JSON.
- `calibrated_static_voxel_averaging_cancels_within_voxel_raster_redistribution()`
  proves equal per-cell color survives harmless pixel redistribution.
- `calibrated_static_comparison_rejects_invalid_thresholds()` covers fail-closed
  threshold validation.
- `calibrated_static_text_regions_cannot_hide_a_wrong_component_interior()`
  proves a text-bearing node cannot exclude its complete bounds.
- `calibrated_static_text_regions_do_not_hide_missing_text_pixels()` proves the
  unmasked voxel guard still sees text pixels.
- `calibrated_static_rounded_surface_pixels_remain_inside_every_acceptance_guard()`
  proves edge pixels are compared rather than broadly masked.
- `calibrated_static_voxel_guard_rejects_a_half_sized_visual_defect_hidden_by_global_averages()`
  proves a localized structural defect cannot hide in global averages.
- The remaining tests preserve legacy diagnostic masking, text-geometry,
  opacity, sRGB, canonical-dimension, determinism, Swift coordinate-space, and
  exact-static behavior.

## Logic narrative

The v5 fixtures distinguish the three admission populations deliberately.
Full-frame SSIM and 16×16 voxels always include every pixel. Stable-interior
differing-pixel accounting excludes only declared runtime text-line rectangles
expanded by one physical pixel. Hostile fixtures put wrong component interiors,
missing glyph pixels, rounded edges, and half-sized visuals where a broad mask
would previously have hidden them.

## Preconditions and postconditions

Inputs must declare sRGB, be opaque, share dimensions, and match
`layout root × canonical scale`. Passing proves deterministic behavior for the
covered fixtures; it does not replace live framework screenshot acquisition.

## Edge cases and failure modes

Coverage includes dimension mismatch, canonical-root mismatch, nonopaque or
untagged PNGs, absent legacy text evidence, invalid calibrated thresholds, broad
text-node masking, rounded-edge masking, and localized structural loss.
Malformed PNG fuzzing and ICC transformation remain outside this bounded suite.

## Concurrency and memory behavior

Tests are synchronous and use small in-memory buffers. They have no clocks,
sleeps, network access, filesystem mutation, global mutation, or concurrent
tasks.

## Performance notes

Fixtures validate contract behavior, not throughput. The v5 calibrated path
decodes each input once, builds one narrow text-region bitmap, and performs
linear raster and voxel passes without Delta-E conversion.

## Feature flags and cfgs

No feature or target cfg changes these tests.

## Testing and benchmarks

Run `cargo test --locked -j12 -p oxide-benchmark-spec --test visual_tests` from
the `oxide` workspace.

## Examples

The sparse-noise test is the minimal calibrated example: encode equal canonical
frames, declare empty runtime text lines, invoke
`compare_calibrated_static_pngs`, and inspect both admission metrics and exact
diagnostics.

## Changelog

- 2026-07-26: froze v5 true voxel averaging, rapid thresholds, and adversarial acceptance/rejection coverage.
- 2026-07-26: added v4 calibrated global, narrow-region, and unmasked local
  adversarial coverage.
- 2026-07-18: added exact and legacy diagnostic visual coverage.
