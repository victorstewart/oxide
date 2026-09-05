# `oxide-text-compositing-diagnostic` / `production_probe`

## Intention and purpose

This module captures the real Oxide title atlas and lowered glyph-instance inputs for the offscreen Metal stage probe without exposing diagnostic hooks in production crates.

## Relation to the rest of the code

It calls public `oxide-text` and `ui-core` APIs exactly as the comparison scene does. A capture uploader records the first 1024² A8 page, while the draw list supplies the production glyph quads, UVs, linear color, baseline, and phase behavior. Production crates remain unchanged.

## Entry points

- `prepare(request, font_bytes, output_directory)`: prepares both frozen title cases and persists one A8 atlas plus one exact-float-bit manifest per case.

## Logic narrative

Each case creates a fresh production `TextCtx`, loads the pinned variable font at its declared default axes, computes the centered baseline from the comparison-only frozen Noto Sans metrics, and calls `elements::encode_label_text`. `finish_frame` drives the real paged-atlas publication boundary. The module validates canonical A8 glyph quads and serializes their destination, UV, and color floats by bit identity, matching the renderer's 48-byte glyph-instance lowering.

## Preconditions and postconditions

The request must contain the supported left or center alignment, a valid pinned font, finite geometry, and a title that lowers to non-SDF A8 quads. Success writes `CASE.production-atlas.a8` and `CASE.production-probe.json`.

## Edge cases and failure modes

Invalid variations, baseline failure, incomplete uploads, mismatched handles, out-of-bounds updates, noncanonical quads, empty output, serialization failure, or file errors fail the diagnostic.

## Concurrency and memory behavior

Preparation is sequential. Each case owns one bounded production text context and one captured 1024² A8 page.

## Performance notes

This is command-line diagnostic setup, not a production hot path or benchmark result.

## Feature flags and cfgs

The Apple raster path is selected by the macOS target. There are no crate features.

## Testing and benchmarks

The complete integration test validates the production atlas and manifest identities before accepting the Metal report.

## Examples

Use the crate CLI; this module is private.

## Changelog

- 2026-07-19: added real production A8-page and glyph-instance capture for the Metal stage probe.
