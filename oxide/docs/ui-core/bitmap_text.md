# ui-core `bitmap_text.rs`

## Intention and purpose

`bitmap_text` provides deterministic fontdue-rasterized text for small overlays that do not use the primary shaped-text context. Production drawing is atlas-only: a caller owns one `BitmapTextAtlas`, uploads its A8 pixels, and encodes text as resolved `GlyphRun` commands.

## Rendering contract

- Create and retain one `BitmapTextAtlas` for the owning surface or renderer context.
- Upload `image()` and assign its `ImageHandle` with `set_handle()` before drawing.
- Call `draw_text`, `draw_text_aligned`, `draw_text_spans`, or `draw_multiline` with the retained atlas.
- After new glyphs are inserted, use `dirty_rect()` to upload the changed atlas region and call `clear_dirty()` only after publication succeeds.
- A `false` draw result is fail-closed. Production code must not reconstruct the retired per-alpha-run solid path.

The atlas owns its fontdue fonts, compact atlas-entry cache, reusable layout, and reusable vertex/index buffers. Raster bytes are copied directly into the A8 atlas and then released instead of being retained a second time. Warm rendering therefore requires neither the legacy global rendering mutex nor heap growth. Measurement-only helpers remain available for layout callers that do not own an atlas.

## Invariants

- Raster bytes and glyph placement retain the existing 3x-oversampled fontdue contract.
- Each non-empty text span emits at most one resolved glyph run.
- Atlas revisions advance when new raster bytes are inserted, and encoded glyph runs carry the current revision.
- The 1024 × 1024 A8 atlas has bounded storage and fails closed when it cannot place another glyph.
- The retired bitmap-solid implementation exists only as an independent test reference.

## Verification and performance

`crates/ui-core/tests/bitmap_text_tests.rs` independently rasterizes the Cut/Copy/Select All/Paste labels and compares every atlas byte and glyph quad with the retired fontdue behavior. `text_frame_allocation_tests.rs` proves the warmed option-popover path performs zero allocations. `cpu.architecture.text.bitmap_options` preserves the production command and atlas counters.

The final implementation-source 15-pair CPU probe measured parent/candidate p50 at 11.977/1.542 us per popover (87.170% paired median speedup, 95% CI 87.058%..87.200%, 15/15 wins). Total commands fell from 2,115 to 11, including 2,108 to zero label solids; render locks fell from four to zero and warm allocations fell from 16 to zero per operation.

The 15-pair tiled eight-popover Metal proof retained 2,100 completed timestamps per side. Parent/candidate p50 was 0.3355/0.0393 ms, p95 was 0.3401/0.0406 ms, p99 was 0.3420/0.0415 ms, and peak was 0.3468/0.0676 ms. The paired median speedup was 88.259% with 15/15 wins; commands/draws fell from 16,920/16,896 to 88/40. A one-popover GPU population was retained as rejected evidence because its 0.03 ms candidate workload was too short for stable tail attribution.

## Changelog

- 2026-07-14: removed production per-alpha-run solid rendering and consolidated deterministic bitmap text into the A8 atlas/`GlyphRun` pipeline.
