# `lib_tests.rs`

## Intention and purpose

These tests freeze the comparison-only diagnostic's source identity and bounded case matrix.

## Relation to the rest of the code

The tests exercise only public APIs from `oxide-image-sampling-diagnostic`; they do not invoke AppKit, Metal, a window, or an application bundle.

## Entry points

- `canonical_decoder_matches_the_frozen_source_identity()`: verifies encoded and canonical RGBA hashes, dimensions, byte count, and opacity.
- `sampling_matrix_is_complete_and_bounded()`: verifies all ratios, phases, pair count, and ROI ceiling.
- `canonical_decoder_rejects_invalid_png_bytes()`: verifies malformed input fails closed.

## Logic narrative

The frozen source test guards against decoder or fixture drift. The matrix test prevents accidental expansion or omission. The malformed-input test covers the decoder boundary.

## Preconditions and postconditions

The committed comparative fixture must exist relative to the crate. Passing tests prove deterministic CPU-side setup, not AppKit/Metal pixel equality.

## Edge cases and failure modes

Missing fixtures, altered hashes, nonopaque pixels, changed matrix values, an oversized ROI, or accepted malformed data fail assertions.

## Concurrency and memory behavior

Tests run independently and allocate one decoded source only in the source-identity case.

## Performance notes

The ROI ceiling test keeps the GPU helper population bounded even though it does not execute GPU work.

## Feature flags and cfgs

None.

## Testing and benchmarks

Run `cargo test --locked -p oxide-image-sampling-diagnostic`.

## Examples

No additional examples.

## Changelog

- 2026-07-19: added fixture, matrix, and malformed-input coverage.
