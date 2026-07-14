# oxide-text tests `shaping_tests.rs`

## Intention and purpose
- Prove the text shaper, atlas, and glyph baking paths preserve visible glyph output and atlas upload invariants.
- Cover atlas pressure behavior so constrained atlases evict stale glyph slots instead of silently dropping renderable current glyphs.

## Relation to the rest of the code
- Exercises public `oxide-text` APIs directly with fixture fonts.
- Protects `oxide-ui-core::elements::TextCtx`, which depends on dirty atlas rectangles and stable glyph-run output.

## Entry points list
- `latin_text_shapes_into_atlas`
  Shapes Latin text and verifies glyph vertices, indices, and atlas pixels.
- `shaped_prefix_widths_match_ascii_prefix_shapes`
  Verifies one shaped-run prefix widths match repeated prefix shaping for simple ASCII text.
- `shaped_prefix_widths_follow_combining_grapheme_boundaries`
  Verifies shaped cluster advances land on grapheme boundaries for combining-mark text.
- `shaped_cursor_map_tracks_combining_grapheme_boundaries`
  Verifies cursor-map byte ranges and x-picks keep a combining-mark cluster atomic.
- `owned_cursor_map_keeps_zwj_cluster_as_one_cursor_step`
  Verifies an owned shaped cursor map treats a ZWJ family sequence as one cursor step.
- `owned_shape_prefix_widths_match_shaped_output`
  Verifies owned cached shaped runs produce the same prefix width map as borrowed shaping output.
- `atlas_reset_preserves_image_contract`
  Verifies explicit reset keeps image dimensions and clears eviction stats.
- `atlas_dirty_rect_tracks_new_glyph_pixels_only`
  Verifies cached glyph reuse does not create new dirty atlas regions.
- `atlas_eviction_clears_full_reused_slot_for_dirty_upload`
  Verifies a smaller glyph replacing a larger evicted slot clears stale slot pixels and dirties the full reused slot for upload.
- `atlas_pressure_evicts_and_rebakes_current_run`
  Verifies constrained atlas pressure evicts a stale slot and renders the current glyph.
- `atlas_pressure_does_not_evict_glyphs_used_earlier_in_same_run`
  Verifies constrained atlas pressure skips later glyphs instead of evicting glyphs already emitted into the current run.
- `frame_pin_protects_preexisting_visible_glyphs_from_later_runs`
  Verifies a frame lock protects glyphs resident before the frame from eviction by later visible runs, then permits deterministic eviction after frame completion.
- `atlas_too_small_for_glyph_skips_without_eviction_loop`
  Verifies oversize glyphs do not spin through eviction attempts.
- `cjk_text_shapes_into_atlas`
  Shapes CJK text and verifies atlas output.
- `missing_glyph_is_skipped`
  Verifies whitespace or missing visible glyphs do not create geometry.
- `fallback_decisions_invalidate_for_font_database_and_chain_changes`
  Verifies a newly added fallback font and a changed fallback chain cannot reuse stale cached coverage or font decisions.

## Logic narrative
- Tests load fixed Latin and CJK fixture fonts to avoid platform font differences.
- The library's test-only SDF oracle compares the exact EDT with the retired 17x17 search at a predeclared zero-byte tolerance for synthetic holes/thin strokes and the Latin/CJK 2x/3x by 48/96 px glyph matrix.
- Prefix-width tests derive caret positions from one shaped run, compare the result against repeated prefix shaping where that is a valid ASCII oracle, and verify owned-run cache reuse does not change the cursor map.
- Cursor-map tests validate both the shaped width table and UTF-8 byte ranges, so text input code cannot split combining or ZWJ clusters while mapping pointer x positions.
- Atlas-pressure coverage uses a deliberately small atlas and feeds unique glyphs until a stale slot must be reused.
- The pressure tests check the current glyph-run spans, atlas revision, resident dirty rectangle, and eviction counter rather than depending on private atlas coordinates.
- The full-slot reuse test probes glyph bounds first, then constrains the atlas so the replacement glyph must reuse a larger old slot and verifies CPU pixels outside the smaller replacement are cleared.

## Preconditions and postconditions
- Fixture fonts must remain available under `crates/text/tests/fixtures`.
- Passing tests mean renderable glyphs are either resident, rasterized, or explicitly skipped when they cannot fit.

## Edge cases and failure modes
- Empty and oversize glyph output is covered.
- Repeated glyph baking with cached atlas entries is covered.
- Atlas pressure is covered without requiring a full atlas reset.
- Smaller replacement glyphs in larger evicted slots are covered so dirty-rect uploads cannot preserve stale edge pixels.
- Same-run pressure is covered so a tiny atlas cannot corrupt vertices emitted earlier in the same `GlyphRun`.
- Whole-frame pressure is covered so later labels cannot overwrite atlas slots referenced by earlier visible labels.
- Combining-mark prefix boundaries are covered so cursor maps do not split UTF-8 inside a grapheme cluster.
- ZWJ grapheme boundaries are covered so emoji-style clusters remain one cursor step even when the fixture font lacks a color glyph.
- Owned-shape prefix parity is covered so UI caches can share shaped label runs with text-input cursor metrics.

## Concurrency and memory behavior
- Tests are single-threaded.
- The atlas pressure test accumulates caller-owned vertex and index buffers to verify returned spans remain relative to existing buffer contents.

## Performance notes
- The pressure test protects the allocation-avoidance policy that reuses stale atlas slots instead of rebuilding the entire atlas.
- The full-slot reuse test protects dirty-rect upload correctness after slot-level eviction.
- Revision assertions protect retained draw-list invalidation after atlas eviction or reset.

## Feature flags and cfgs
- No feature-specific branches.

## Testing and benchmarks
- Run with `cargo test --locked -p oxide-text --test shaping_tests`.
- Perf companion: `OXIDE_PERF_RUNNER_FILTER=cpu.system.text_atlas_pressure cargo run --release --locked -p oxide-perf-runner -- --run-suite --smoke`.
- Prefix-map perf companion: `OXIDE_PERF_RUNNER_FILTER=cpu.system.text_prefix_width_map cargo run --release --locked -p oxide-perf-runner -- --run-suite --smoke`.

## Examples
```rust
cargo test --locked -p oxide-text --test shaping_tests
```

## Changelog
- 2026-07-14: added fallback cache invalidation coverage and documented the exact zero-tolerance SDF reference matrix.
- 2026-07-14: added whole-frame pin coverage for pre-existing visible glyph slots.
- 2026-06-01: added full-slot clear/dirty coverage for smaller replacement glyphs reusing larger evicted atlas slots.
- 2026-05-31: added shaped cursor-map tests for combining-grapheme byte ranges and ZWJ cluster atomicity.
- 2026-05-31: added owned-shape prefix-map parity coverage for shared label/cursor shaped-run caches.
- 2026-05-31: added shaped-prefix tests for one-run cursor width maps across ASCII prefixes and combining-grapheme boundaries.
- 2026-05-31: added atlas-pressure, same-run protection, revision, and oversize-glyph tests for the LRU slot eviction path.
