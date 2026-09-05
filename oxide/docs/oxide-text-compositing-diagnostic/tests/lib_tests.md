# `oxide-text-compositing-diagnostic` / tests / `lib_tests`

## Intention and purpose

These tests freeze the request, font, helper boundary, output identities, and complete 35-signature report.

## Relation to the rest of the code

The tests call the public request/hash/run APIs. The macOS integration test uses a unique bounded temporary directory and removes it after validating the helper output.

## Entry points

- `request_freezes_two_titles_and_all_known_signatures()`: checks geometry, colors, case counts, and 16+19 signature partition.
- `pinned_font_identity_is_frozen()`: checks the Noto Sans artifact SHA-256.
- `swift_helper_keeps_all_three_paths_offscreen_and_bounded()`: guards required CoreText/A8/direct/mask/CPU paths and rejects app/window ownership.
- `metal_helper_freezes_the_production_atlas_pipeline_stages()`: freezes `R8Unorm` upload, point/linear `R32Float` coverage, `BGRA8Unorm_sRGB`, exact source-alpha factors, both shader stages, and the offscreen boundary.
- `helper_persists_complete_identity_checked_report()`: runs both macOS helpers and validates two cases, 35 signatures, the CoreText 30/30/5 counts, the Metal 5/0/0/30 attribution, zero unexplained signatures, and every frozen artifact hash.

## Logic narrative

Static tests detect contract drift before invoking Apple frameworks. The integration test exercises real production atlas preparation, both Swift helpers, CoreText and Metal rendering, output persistence, report parsing, exact stage attribution, and Rust-side SHA/length validation as one bounded workflow.

## Preconditions and postconditions

The pinned font must exist under comparative specs. The complete test requires macOS command-line Swift.

## Edge cases and failure modes

Any missing path, changed hash, incomplete signature, helper-source regression, nonzero sampler-stage difference, incomplete full-Metal reproduction, report error, or cleanup failure fails the test.

## Concurrency and memory behavior

Tests use one helper process and one unique temporary directory. No app or window is created.

## Performance notes

No performance result is asserted.

## Feature flags and cfgs

The full helper test is compiled only on macOS.

## Testing and benchmarks

Run `cargo test --locked -p oxide-text-compositing-diagnostic`.

## Examples

The integration test is the executable example of the complete contract.

## Changelog

- 2026-07-19: added frozen request, identity, boundary, and complete-helper coverage.
