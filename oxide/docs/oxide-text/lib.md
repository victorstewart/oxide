# oxide-text `lib.rs`

## Intention and purpose
- Own text shaping, glyph rasterization, A8 atlas packing, and glyph-run quad generation for Oxide renderers.
- Keep text atlas pressure inside the text subsystem instead of letting callers silently lose glyphs when the atlas fills.

## Relation to the rest of the code
- `oxide-ui-core::elements::TextCtx` owns an `oxide_text::Atlas`, `TextShaper`, and `RasterCtx`.
- UI elements request shaped and baked glyph runs; renderers only consume `oxide_renderer_api::GlyphRun` geometry and the uploaded atlas image.

## Entry points list
- `oxide_text::Font::from_bytes(data: Vec<u8>) -> Font`
  Creates an owned font payload for shaping and rasterization.
- `oxide_text::FontDb::add_font(&mut self, f: Font) -> usize`
  Stores a font and returns its stable font id.
- `oxide_text::FontDb::font(&self, id: usize) -> Option<&Font>`
  Looks up a font by id.
- `oxide_text::Atlas::new(width: u32, height: u32) -> Atlas`
  Creates an A8 glyph atlas with monotonic packing and LRU slot eviction.
- `oxide_text::Atlas::image(&self) -> (&[u8], u32, u32)`
  Exposes the atlas bytes and dimensions for upload.
- `oxide_text::Atlas::dirty_rect(&self) -> Option<AtlasDirtyRect>`
  Returns the union of dirty atlas pixels since the last upload.
- `oxide_text::Atlas::clear_dirty(&mut self)`
  Marks the current dirty region as uploaded.
- `oxide_text::Atlas::reset(&mut self)`
  Clears atlas pixels, entries, dirty state, and eviction stats for explicit memory trimming.
- `oxide_text::Atlas::glyph_count(&self) -> usize`
  Reports resident glyph entries for tests and diagnostics.
- `oxide_text::Atlas::eviction_count(&self) -> u64`
  Reports LRU slot evictions for tests and perf diagnostics.
- `oxide_text::Atlas::revision(&self) -> u64`
  Reports the atlas slot-generation counter used to reject retained glyph geometry after eviction or reset.
- `oxide_text::TextShaper::shape(&mut self, font: &Font, font_id: usize, text: &str, px: f32) -> anyhow::Result<ShapeOutput<'_>>`
  Shapes UTF-8 text into glyph ids and advances.
- `oxide_text::TextShaper::cursor_map_with_fallback_fonts(&mut self, fonts: &FontDb, primary_id: usize, fallback_ids: &[usize], text: &str, px: f32) -> Option<ShapedCursorMap>`
  Builds a grapheme cursor map by shaping unsupported grapheme clusters with the first configured fallback font that covers them.
- `oxide_text::ShapeOutput::width(&self) -> f32`
  Returns shaped advance width.
- `oxide_text::ShapeOutput::prefix_widths_for_boundaries(&self, boundaries: &[usize]) -> Vec<f32>`
  Derives cumulative cursor/prefix widths for caller-provided UTF-8 boundary offsets from one shaped run.
- `oxide_text::ShapeOutput::cursor_map_for_text(&self, text: &str) -> ShapedCursorMap`
  Builds a grapheme-boundary cursor map from a borrowed shaped run.
- `oxide_text::ShapeOutput::cursor_map_for_boundaries(&self, byte_boundaries: Vec<usize>) -> ShapedCursorMap`
  Builds a cursor map from caller-provided UTF-8 boundaries and shaped visual caret positions.
- `oxide_text::ShapeOutput::to_owned_shape(&self) -> OwnedShape`
  Converts a borrowed shaping result into a reusable owned shaped run.
- `oxide_text::ShapeOutput::bake_into(...) -> oxide_renderer_api::GlyphRun`
  Rasterizes and appends glyph geometry using a temporary raster context.
- `oxide_text::ShapeOutput::bake_into_with(...) -> oxide_renderer_api::GlyphRun`
  Rasterizes and appends glyph geometry with caller-owned raster state.
- `oxide_text::OwnedShape::width(&self) -> f32`
  Returns cached shaped width.
- `oxide_text::OwnedShape::prefix_widths_for_boundaries(&self, boundaries: &[usize]) -> Vec<f32>`
  Derives the same shaped-cluster prefix width map from an owned cached run.
- `oxide_text::OwnedShape::cursor_map_for_text(&self, text: &str) -> ShapedCursorMap`
  Builds the same grapheme-safe cursor map from a cached owned shaped run.
- `oxide_text::ShapedCursorMap`
  Owns grapheme byte boundaries and shaped visual caret positions for caret, selection, and pointer-to-cursor mapping.
- `oxide_text::ShapedCursorMap::width_at_with_affinity(&self, cursor: usize, affinity: CaretAffinity) -> f32`
  Returns the upstream or downstream visual x position for a cursor at mixed-direction run boundaries.
- `oxide_text::ShapedCursorMap::cursor_for_x_with_affinity(&self, x: f32, affinity: CaretAffinity) -> usize`
  Resolves ambiguous mixed-direction hit tests using an explicit caret affinity.
- `oxide_text::OwnedShape::bake_into_with(...) -> oxide_renderer_api::GlyphRun`
  Reuses owned shaped glyphs and caller-owned raster state to append glyph geometry.

## Logic narrative
- `TextShaper` uses `rustybuzz` to shape text and keeps glyph ids plus advances separate from rasterization.
- `RasterCtx` wraps `swash` scaling state so repeated baking can reuse raster infrastructure.
- `Atlas` first attempts monotonic row packing. If there is no free tail space and the new glyph can fit in the atlas, it reuses the least-recently-used resident slot that is large enough for the glyph.
- Slot-level eviction avoids clearing the whole atlas, which would invalidate unrelated glyph UVs already held by retained draw lists.
- Eviction is generation-tracked: retained glyph geometry records the atlas revision it was baked against, and the revision changes on eviction or reset.
- While baking a single run, eviction is limited to glyphs older than the run's starting atlas clock so pressure cannot overwrite a glyph whose vertices were already emitted earlier in the same run.
- Evicting into a larger stale slot clears and dirties the full old slot before writing the replacement glyph, so dirty-rect uploads cannot leave stale glyph pixels around smaller replacements.
- New glyph pixels are unioned into the dirty slot region, so `TextCtx::ensure_gpu` can upload a dirty rectangle instead of the full atlas.
- `ShapeOutput::prefix_widths_for_boundaries` and `OwnedShape::prefix_widths_for_boundaries` map glyph-cluster advances onto caller-provided boundaries, then prefix-sum those advances so UI text inputs can build caret and selection positions without reshaping every prefix.
- `ShapedCursorMap` combines Unicode grapheme byte boundaries with shaped caret positions, so callers use one artifact for byte ranges, caret positions, and O(log n) x-to-cursor lookup for both ascending LTR and descending pure RTL runs. Mixed LTR/RTL maps keep upstream and downstream x positions at ambiguous run boundaries while preserving the single-position fast path for ordinary text.
- `TextShaper::cursor_map_with_fallback_fonts` walks grapheme clusters, groups adjacent clusters by the selected font, shapes each run once, and stitches the run prefix widths into one global `ShapedCursorMap`.

## Preconditions and postconditions
- Font ids passed to shaping and baking must identify the font used to shape the glyphs.
- Returned `GlyphRun` spans reference vertices and indices appended to the caller-provided buffers.
- Atlas eviction preserves atlas dimensions and increments `eviction_count`.
- Returned `GlyphRun` values carry the atlas revision current at the end of baking.

## Edge cases and failure modes
- Zero-sized glyph bitmaps are skipped.
- Glyphs larger than the atlas are skipped without entering an eviction loop.
- If no resident slot is large enough during pressure, the glyph is skipped rather than clearing the atlas and invalidating existing UVs.
- If a constrained atlas cannot fit all unique glyphs in one run without evicting earlier glyphs from that same run, later glyphs are skipped instead of corrupting already-emitted vertices.

## Concurrency and memory behavior
- `Atlas`, `TextShaper`, and `RasterCtx` are caller-owned mutable state and do not synchronize internally.
- The atlas maintains a `HashMap` of resident glyph keys and a contiguous A8 pixel buffer.
- Baking a borrowed `ShapeOutput` materializes a compact glyph vector before rasterization; cached UI paths prefer `OwnedShape`.

## Performance notes
- Cache hits update glyph LRU state without rasterizing or uploading.
- LRU slot eviction clears and replaces a stale glyph rectangle in place, avoiding full-atlas uploads, stale global atlas resets, and stale edge pixels after smaller replacement glyphs reuse a larger slot.
- The dirty rectangle is unioned across new or overwritten glyph slots until the caller uploads it.
- Cursor maps reuse the same shaped glyph buffer or owned cached run, replacing O(boundary count) shaping calls on a cache miss with one shape plus a glyph-to-boundary pass and binary-search hit testing.
- Fallback shaping emits font-contiguous grapheme runs with global x offsets so visible glyph encoding can batch one mixed-font line into one glyph draw when the combined index span fits.
- Fallback cursor maps still use the cached `ShapedCursorMap` for hot pointer picking and do not probe fallback fonts per pointer event.
- Retained draw caches should compare cached glyph-run revisions with the current atlas revision before replaying cached text geometry.

## Feature flags and cfgs
- No feature-specific atlas behavior.

## Testing and benchmarks
- `crates/text/tests/shaping_tests.rs` covers shaping, reset behavior, atlas revisions, dirty rectangles, full-slot dirtying/clearing after atlas eviction, atlas pressure eviction, same-run eviction protection, and oversize glyph skipping.
- `crates/text/tests/shaping_tests.rs` also covers shaped-run prefix width maps and cursor maps for ASCII prefixes, combining-grapheme boundaries, ZWJ clusters, pure RTL visual order, mixed-bidi caret affinity, configured fallback-font cursor widths and shape runs, and owned-run reuse parity.
- `cpu.system.text_atlas_pressure` exercises constrained atlas pressure in the workspace perf runner.
- `cpu.system.text_prefix_width_map` exercises the one-shaped-run cursor prefix map used by text input caches.

## Examples
```rust
let mut atlas = oxide_text::Atlas::new(1024, 1024);
assert_eq!(atlas.eviction_count(), 0);
```

## Changelog
- 2026-06-01: atlas slot eviction now clears and dirties the full reused slot before writing a smaller replacement glyph, preventing stale dirty-rect-upload pixels around the new glyph.
- 2026-05-31: added mixed-bidi upstream/downstream caret affinity positions to `ShapedCursorMap`.
- 2026-05-31: added `ShapedCursorMap` so grapheme byte boundaries, shaped prefix widths, and x-to-cursor lookup share one reusable text artifact.
- 2026-05-31: added fallback-font shape runs so mixed-font visible glyph encoding can share the same fallback segmentation as cursor maps.
- 2026-05-31: added fallback-font cursor map construction so mixed Latin/CJK text can measure unsupported grapheme clusters with configured fallback fonts.
- 2026-05-31: added descending visual caret positions to `ShapedCursorMap` so pure RTL text-input picking can use the same cached shaped map path as LTR text.
- 2026-05-31: added owned-shape prefix width maps so UI text caches can share one unwrapped shaped run between label drawing and cursor metrics.
- 2026-05-31: added shaped-run prefix width maps so text inputs can cache caret positions without reshaping every prefix.
- 2026-05-31: added LRU glyph-slot eviction and atlas pressure diagnostics so full atlases stop silently dropping glyphs that can fit in stale resident slots.
- 2026-05-31: added atlas revision tracking so retained text draw caches can reject glyph geometry after atlas slot eviction or reset.
