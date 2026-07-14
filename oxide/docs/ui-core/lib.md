# ui-core `lib.rs`

## Intention and purpose
- Define the framework-level UI primitives that higher-level apps consume.
- Re-export the generic text-input building blocks so apps can share one implementation for field-policy execution, shifting/caret behavior, and secure masking.
- Keep shared legacy modal chrome in `elements.rs` so apps do not duplicate fullscreen blur, popup blur, border, and inner-fill math per scene.
- Keep shared legacy badge overlay geometry in `elements.rs` so apps do not duplicate the old iOS quarter-size top-right badge placement or bounce defaults.
- Keep shared legacy spinner defaults in `elements.rs` so apps do not duplicate the old iOS large activity-indicator sizing and fallback animation rules.
- Keep shared legacy sliding-switch gesture semantics in `elements.rs` so apps do not duplicate the old iOS long-press gate, inactivity timeout, or outside-cancel behavior.

## Relation to the rest of the code
- `oxide-ui-core` sits above renderer/platform crates and below app crates such as Nametag.
- `text_fields.rs` now owns the generic policy-driven text-editing state machines.
- App crates can either consume the Oxide types directly or wrap them with app-local taxonomy adapters such as Nametag `FieldKind`.

## Entry points list
- `pub mod overlay`
  Exposes the shared overlay and popup-lifecycle infrastructure used by higher-level surface routers.
- `pub mod anim`
  Exposes shared animation helpers for reusable easing and keyframed offset sampling.
- `pub mod draw_replay`
  Exposes renderer-agnostic draw-list replay helpers for translated CPU composition paths.
- `pub mod collection`
  Exposes keyed, virtualized collection layout and rendering with fixed-extent and variable-extent measurement caches.
- `collection::Measure::item_index_for_key`
  Optional key-to-index lookup that lets keyed collections reconcile focus and hover after data reorder without scanning every item.
- `collection::Measure::collection_revision`
  Optional collection-level epoch that lets variable-size collection prefixes skip full key/revision signature scans when data is unchanged.
- `collection::Measure::changed_item_range`
  Optional dirty item range that lets epoch-backed variable-size collection prefixes repair only affected rows/items after small data changes.
- `DrawListBuilder::append_drawlist_with_text_atlas_revision`
  Replays a cached draw list only when cached glyph runs match the current text-atlas revision for the target atlas.
- `DrawListBuilder::append_drawlist_with_text_atlas_revisions`
  Replays a cached draw list only when every cached glyph run has an explicit matching atlas handle and revision.
- `DrawListBuilder::append_retained_drawlist`
  Replays a retained cached draw list only when it contains no text-atlas-dependent glyph runs or layer commands.
- `DrawListBuilder::append_retained_drawlist_with_text_atlas_revisions`
  Replays a retained cached draw list only when every glyph run has a current atlas revision and the command stream is retained-replay safe.
- `coalesce_adjacent_draws_reuse`
  Coalesces adjacent mergeable draw commands with caller-owned scratch storage for hot frame loops that prewarm allocation capacity.
- `elements::TextCtx::retained_text_atlas_revision`
  Exposes the live text atlas handle and revision only after dirty atlas bytes have been uploaded to the GPU.
- `elements::TextCtx::begin_frame` / `elements::TextCtx::finish_frame`
  Bracket visible label preparation, defer dirty atlas publication, patch provisional glyph handles in place, and return the completed frame's text counters.
- `elements::TextCtx::set_frame_stats_enabled` / `elements::TextCtx::last_frame_stats`
  Enable opt-in shaping, raster, cache, upload, eviction, and invalidation diagnostics and read the most recently completed frame.
- `elements::TextFrameStats`
  Carries the frame-scoped text preparation counters used by tests and performance reports.
- `elements::TextCtx::set_fallback_fonts`
  Configures fallback font ids used for text-input cursor-prefix metrics when the primary font does not cover a grapheme cluster.
- `elements::TextInputState::cursor_index`
  Exposes the current grapheme-cluster cursor index for host selection sync, tests, and performance diagnostics.
- `elements::ImageView::encode`
  Emits one semantic `DrawCmd::Image`: contain and stretch map the complete natural image, while cover/zoom/pan intersect the fitted destination with the view and map that visible interval back into source pixels.
- `UiSurface::encode_retained_with_text_ctx`
  Replays a retained surface with the live `TextCtx` atlas snapshot when it is safe, otherwise falls back to the no-context fail-closed replay path.
- `SurfaceRouter::encode_with_overlays_with_text_ctx`
  Routes current-surface retained replay through the live `TextCtx` atlas snapshot before adding overlays and popups.
- `SurfaceRouter::retained_composition_stats`
  Reports current-surface, overlay, and popup retained draw reuse for the most recent router composition encode.
- `UiSurface::edit_style`
  Applies a scoped node style edit and classifies paint-only changes separately from layout-affecting changes.
- `UiSurface::add_node`
  Adds a child through the surface-owned mutation path, preserving scoped layout dirtiness and retained sibling replay instead of conservatively dirtying the whole tree.
- `UiSurface::remove_node`
  Removes a non-root node through the surface-owned mutation path, detaching from the known parent and keeping clean sibling branches eligible for layout skip and retained replay.
- `UiSurface::mark_node_dirty`
  Marks one node with a dirty class so content-only text/image/camera updates can rebuild the affected retained path, while accessibility/hit-test metadata updates keep renderer-facing draw caches intact.
- `RetainedCachePolicy`
  Configures hard logical CPU and future prepared-GPU retained-byte budgets, recent-hit protection, and optional repeated-invalidation suppression.
- `NodeTree::retained_cache_policy` / `NodeTree::set_retained_cache_policy`
  Reads or replaces the tree-owned retained-cache policy; reducing the CPU budget evicts immediately through the same generation-aware LRU used after rendering.
- `UiSurface::retained_cache_policy` / `UiSurface::set_retained_cache_policy`
  Exposes the same policy at the public surface boundary and clears an incompatible whole-snapshot cache after a policy change.
- `UiSurface::tick_at` / `UiSurface::accessibility_frame`
  Advance animator-owned dense overrides and query the same fully composed affine geometry used for retained rendering and hit testing.
- `AnimOverrideSlots`
  Stores node-indexed transform, opacity, and paint overrides with retained capacity plus exact changed/paint-changed lists.
- `RetainedNodeStats`
  Reports chunk/sequence bytes, hits, misses, admissions, rejections, evictions, evicted bytes, build time, fallback use, cache completeness, and the latest invalidation reason.
- `LayoutStats::measured_children`
  Counts child entries scanned by row/column measurement passes, making parent-side layout work visible in tests and perf reports.
- `NodeStyle::transform`
  Applies translation during draw encoding and hit testing while keeping logical layout rectangles stable.
- `pub mod text_fields`
  Exposes the generic policy-driven text-input module.
- `pub mod picker_popup`
  Exposes the generic popup/legacy-picker interaction module.
- `pub mod emitter`
  Exposes the shared CAEmitter-style burst sampler used by downstream app particle effects.
- `pub use text_fields::{EditableText, FieldFailRestoreMode, HorizontalShiftingText, SecureText, TextFieldPolicy}`
  Makes the text-input primitives available from the crate root so app wrappers do not need to reach into module internals.
- `pub use picker_popup::{PanelPopupState, PickerColumnCommit, PickerColumnState, PopupPickerState, PopupTapRegion}`
  Makes the shared popup dismissal and legacy picker drag/snap/commit controllers available from the crate root.
- `pub use emitter::{BurstEmitter, BurstEmitterCellConfig, BurstEmitterConfig, BurstEmitterParticle, BurstEmitterShape}`
  Makes the shared CAEmitter-style burst API available from the crate root so app crates can reuse the same particle timing and source-shape logic.
- `pub use overlay::{PopupCallbacks, PopupManager, PopupSpec, PopupTouchRegion}`
  Makes the shared popup lifecycle contract available from the crate root so app crates can reuse key-popup lookup, dismissal approval, touch-exception routing, and content-size resync without rebuilding window semantics per scene.

## Logic narrative
- `lib.rs` remains the crate aggregation layer.
- The animation move adds shared bezier/keyframed-offset helpers so app crates no longer keep duplicate motion math for standard swap or recovery-shake profiles.
- The text-input move adds one more crate-root export surface so generic input primitives live beside the existing drawing, overlay, animation, and design-system utilities.
- `elements.rs` now also owns the old iOS modal popup chrome contract, so downstream apps can reuse one resolved blur-card treatment instead of hard-coding sigma, alpha, radius, and border math in scene code.
- `elements.rs` also owns the old iOS badge overlay contract, so downstream apps can reuse one image-first badge treatment instead of keeping count-pill logic or local placement math.
- `elements.rs` also owns the old iOS spinner contract, so downstream apps stop passing phase or stroke data and instead issue one atom-driven large-indicator request.
- `elements.rs` also owns the old iOS sliding-switch interaction contract, so downstream apps stop re-implementing the 0.3s press gate, one-shot inactivity callback semantics, and bounds cancellation around `SlidingSwitchState`.
- `collection.rs` owns stable item identity for virtualized grids and rows. Focus/hover state is keyed by `Measure::item_key`, while keyboard navigation can still move by current index and rematerialize the actual item key on the next layout pass.
- The popup-picker move follows the same boundary: Oxide owns the reusable multi-column legacy-picker interaction state, scroll-end commit result, and fixed medium-impact haptic intent, while apps keep their own anchored layouts, copy, and visual treatments.
- The emitter move follows that same pattern: Oxide owns the reusable burst timing, source-shape, and particle sampling math, while apps keep scene-specific asset choice and draw calls.
- The spinner move follows the same rule at runtime too: the iOS host can now promote spinner draws into native `UIActivityIndicatorViewStyleLarge` views while non-iOS fallbacks still share one Oxide-owned contract.
- Camera views encode the renderer-owned `CameraBg` path so visible preview composition stays inside Oxide.
- Image views reserve `NineSlice` for genuine nonzero slice insets. Axis-aligned source cropping expresses cover, zoom, and pan without a clip command, so renderer backends receive bounded image quads and explicit natural-pixel source rectangles.
- The popup lifecycle move follows that same rule: Oxide now owns the reusable key-popup, approval-gated dismissal, manual or content-root touch-exception, and content-size refresh contract, while apps keep scene-specific copy and mutation policy.
- This keeps ownership clear: Oxide owns reusable UI state machines; app crates own field naming, copy, and scene composition.

## Preconditions and postconditions
- Downstream code that imports the crate root now receives the same text-input types regardless of which app consumes Oxide.
- No public Nametag-specific concepts are exposed from this file.

## Edge cases and failure modes
- The crate root does not add new runtime failure paths; it only re-exports the new module.

## Concurrency and memory behavior
- No additional state is introduced at the crate root.
- Memory and synchronization behavior are defined by the exported modules themselves.

## Performance notes
- Crate-root re-exports are zero-cost.
- `draw_replay` translates only command-local geometry slices instead of cloning whole draw lists.
- Cached draw-list replay can reject stale or unknown text atlas revisions before appending glyph geometry, preventing retained text from pointing at an evicted atlas slot.
- Retained draw-list replay now fails closed when no text-atlas context is supplied, so cached glyph geometry cannot bypass atlas revision checks after atlas eviction, reset, or dirty upload state changes.
- `TextCtx::retained_text_atlas_revision` keeps surface/router retained text replay on a live atlas by refusing to expose a snapshot while atlas bytes are still dirty.
- `TextCtx` now defers frame-owned atlas publication until all visible labels are prepared, unions damage under an explicit 75% full-upload threshold, and patches provisional atlas handles without reordering commands.
- Disabled text diagnostics retain only one direct frame-active branch; boxed counter state is absent from cache-hit glyph baking, and the warm 1,000-label frame remains allocation-free.
- `TextCtx` builds cached shaped cursor maps from the cached unwrapped owned shape when available, avoiding duplicate shaping between label drawing and text-input cursor metrics.
- Text-input pointer picking uses the cached `oxide_text::ShapedCursorMap` directly instead of probing every cursor position through repeated cache lookups, including descending visual caret maps for pure RTL runs.
- When fallback fonts are configured, `TextCtx` builds prefix metrics through `oxide_text::TextShaper::cursor_map_with_fallback_fonts`, so unsupported grapheme clusters contribute the fallback font's shaped advance to caret geometry.
- Text-input filtering, secure masking, and legacy editable backspace now count grapheme clusters instead of Unicode scalar values.
- `UiSurface::encode_retained` now tracks bounded, replay-safe retained draw lists per `NodeTree` node so dirty leaf paint/style changes rebuild the leaf and ancestors while replaying clean sibling subtrees.
- `NodeTree` assigns every live node stable generation-checked transform and opacity slots, keeps chunks in node-local coordinates, and emits complete nested affine/cumulative-opacity values without invalidating geometry.
- Ancestor clip changes rebuild descendant instance metadata only; transform/opacity animation changes snapshot properties only. Hit testing and accessibility frames use the identical nested transform composition.
- Retained node chunks and persistent sequence metadata are governed by exact logical-byte accounting instead of an item-count cutoff. Eviction removes the selected chunk and every ancestor sequence that indirectly references it before the next render.
- The cache uses intrusive LRU links in existing nodes, generation windows, and cumulative hit counts to prefer cold eviction without allocating an auxiliary map or queue. Optional invalidation-streak suppression is explicit because enabling it by default regressed ordinary dirty-leaf rendering.
- A zero CPU budget takes a direct one-chunk UI rebuild path. It retains no node-cache bytes, leaves caller-owned text/image sequences untouched, and prevents one-use trees from constructing thousands of persistent node/path allocations.
- `UiSurface::edit_style` lets paint-only authoring changes dirty retained draw state without forcing a same-size layout pass.
- `UiSurface::mark_node_dirty` keeps text/image/camera content dirtiness node-scoped, avoiding full-surface retained invalidation when layout and hit-test geometry are unchanged.
- `UiSurface::mark_node_dirty` treats accessibility-only and hit-test-only dirtiness as non-draw metadata updates, preserving clean retained draw-list reuse.
- `ImageView::encode` uses aspect cross-products on the no-zoom contain/cover path, avoiding redundant scale divisions while emitting bounded source-cropped image draws. Transformed views use the general fitted-rectangle intersection only when zoom or pan requires it.
- `UiSurface::add_node` and `UiSurface::remove_node` cover common structural edits without falling back to `tree_mut()`'s whole-tree dirtiness. Existing direct `tree_mut()` access remains the conservative escape hatch.
- `SurfaceRouter::encode_with_overlays` reuses retained draw lists for the current surface, overlays, and popups while keeping capture paths as fresh non-retained encodes for diagnostics.
- `NodeTree::layout` returns `LayoutStats` and skips clean subtrees whose content rect is unchanged, keeping layout-affecting leaf edits from walking unrelated sibling branches.
- `NodeTree::layout` now detects unchanged clean child layout/content rects in the parent row/column loop, avoiding the extra child layout call that previously only discovered the same skip one level later.
- `NodeTree` now distinguishes descendant-only layout dirtiness from parent-geometry dirtiness. Fixed-outer padding, axis, and gap edits can traverse the dirty descendant path without remeasuring clean siblings at the parent.
- `NodeTree::layout_child_or_skip` refuses to skip a child that still has `descendant_layout_dirty`, so ancestor relayouts with stable child geometry cannot strand descendant-only layout updates.
- Transform-only style edits dirty retained draw and hit-test state without setting layout dirtiness; draw encoding and hit testing accumulate parent translation over stable logical layout rectangles.
- `CollectionView` caches variable item measurements by item key, constraint, and revision; keyed focus reconciliation preserves identity across visible reorders without invalidating warm measurement caches, and can use `Measure::item_index_for_key` to avoid full scans after far reorders.
- `CollectionView` bounds its variable measurement cache and prunes cold key/constraint/revision entries under large churn, so long-lived virtualized collections do not retain every historical measurement.
- `CollectionView` can reuse variable row/grid prefix offsets across scroll passes when `Measure::collection_revision` reports an unchanged epoch, and epoch-backed measures can provide `Measure::changed_item_range` to repair only affected prefix rows/items after small mutations. Legacy measures without a dirty range keep the existing full signature validation.
- `UICameraView` emits Oxide renderer camera commands only; host-native visible preview planes are diagnostic-only outside this authoring surface.
- Consolidating the text-input engines here removes duplicate app-side implementations without adding runtime indirection.
- `prepare_draws` preallocates the resolved clip stack for the common shallow nested-clip path, avoiding the first frame-loop stack growth on representative clipping workloads.
- `coalesce_adjacent_draws_reuse` lets host frame loops reuse a second command buffer for draw-command coalescing, avoiding the fresh output `Vec` allocation that the standalone convenience helper still pays.
- `elements::Label` keeps disabled watch logging off the allocation path, preallocates the common wrapped-line buffers, and lets internal non-wrapped label call sites encode borrowed text directly instead of cloning through temporary `Label` values.
- Wrapped `elements::Label` now reuses the shaped line outputs it created during width fitting and uploads the text atlas once after all line baking, avoiding the old second shape pass over every final wrapped line.
- Primary-font ASCII wrapped labels now shape once for break decisions on cache misses, then shape only the final emitted lines; fallback-font and non-ASCII wrapping keep the conservative legacy path for correctness.
- `PickerState::encode` reuses `TextCtx` cached shaped label lines and publishes only dirty glyph-atlas rectangles, so warm picker redraws do not reshape visible row labels or re-upload the full atlas. The former direct-shape/full-upload audit row was retired after same-workload A/B proof; `cpu.system.picker_text_cached_encode` remains the gated workspace perf signal.

## Feature flags and cfgs
- The text-fields export is always enabled.

## Testing and benchmarks
- `crates/ui-core/tests/elements_tests.rs` covers the shared overlay and popup chrome contract.
- `crates/ui-core/tests/overlay_tests.rs` covers the shared popup lifecycle contract.
- `crates/ui-core/tests/elements_tests.rs` also covers the shared legacy badge overlay contract.
- `crates/ui-core/tests/elements_tests.rs` also covers the shared legacy spinner defaults and atom-driven encoding contract.
- `crates/ui-core/tests/elements_tests.rs` also covers the shared legacy sliding-switch long-press, timeout, and bounds-cancel contract.
- `crates/ui-core/tests/layout_async.rs` covers native async layout worker ordering and blocked-job cleanup.
- `crates/ui-core/tests/draw_builder_tests.rs` covers image-mesh quad index synthesis.
- `crates/ui-core/tests/elements_tests.rs` covers contain, cover, stretch, zoom, pan, alpha, odd natural dimensions, bounded destinations, and fractional source-pixel crops for `ImageView`.
- `crates/ui-core/tests/draw_builder_tests.rs` covers atomic cached draw-list append plus local/absolute index normalization.
- `crates/ui-core/tests/draw_builder_tests.rs` also covers retained text draw replay rejection after missing, stale, and incomplete atlas revision contexts.
- `crates/ui-core/tests/elements_tests.rs` covers the live `TextCtx` retained atlas snapshot guard.
- `crates/ui-core/tests/elements_tests.rs` covers text-input cache and atlas upload paths that consume cached shaped cursor maps, batched visible fallback-font label encoding, plus pointer cursor picking across combining, ZWJ, pure RTL, and configured fallback-font grapheme-cluster boundaries.
- `crates/ui-core/tests/surface.rs` covers dirty leaf retained encoding, live `TextCtx` atlas context routing, clean sibling subtree replay through `RetainedNodeStats`, and retained current/overlay/popup router composition stats.
- `crates/ui-core/tests/surface.rs` also covers 300-node zero-geometry animation, nested affine clip/hit/accessibility synchronization, and generation-safe slot reuse; `anim_prop.rs` covers dense compaction and interruption/completion.
- `crates/ui-core/tests/surface.rs` also covers hard byte enforcement, exact output after eviction, hot-entry protection, explicit churn suppression/readmission, zero-budget direct fallback, and caller-owned text/image chunk identity.
- `crates/ui-core/tests/surface.rs` covers layout dirty-subtree skipping, descendant-only layout traversal, opacity/clip paint-only dirty-class edits, node-scoped content dirty-class edits, and validates `LayoutStats` visit/skip/measurement counters.
- `crates/ui-core/tests/surface.rs` also covers the mixed ancestor-layout plus descendant-dirty case where a stable child rect must not hide dirty grandchildren.
- `crates/ui-core/tests/surface.rs` covers scoped surface add/remove mutations that skip clean sibling layout and replay retained sibling draw lists.
- `crates/ui-core/tests/surface.rs` covers transform-only retained repositioning without layout work and validates translated hit testing.
- `crates/ui-core/tests/collection_transition.rs` covers fixed-extent measurement elision, variable measurement reuse by key/revision, bounded variable-measurement cache eviction, visible keyed cell identity, keyed focus preservation/navigation after reorder, and key-index reconciliation without broad scans.
- `crates/ui-core/tests/collection_transition.rs` covers epoch-stable variable grid/row prefix reuse, dirty-range prefix repair, and verifies warm scroll or small-revision passes avoid full signature scans when the measure provides the necessary epoch/range contract.
- `crates/ui-core/tests/draw_replay_tests.rs` covers translated replay geometry and clip restoration.
- `crates/ui-core/tests/anim_helpers.rs` covers the shared animation-helper surface.
- `crates/ui-core/tests/text_fields_tests.rs` covers the text-input surface.
- `crates/ui-core/tests/picker_popup_tests.rs` covers the popup-picker interaction surface.
- `crates/ui-core/tests/emitter_tests.rs` covers the CAEmitter-style burst surface.
- `crates/ui-core/tests/elements_tests.rs` covers multi-line ASCII wrapped-label cache reuse and clean warm atlas redraws, with `cpu.system.wrapped_label_cached_encode` retained as the gated workspace perf signal after the slower legacy fitting row was retired.
- `crates/ui-core/tests/elements_tests.rs` covers one-publication cold frames, zero-work warm frames, merged incremental damage, and provisional glyph-run order.
- `crates/ui-core/tests/text_frame_allocation_tests.rs` proves a warmed 1,000-label frame performs zero allocations, reallocations, shaping, rasterization, or atlas uploads.
- `crates/ui-core/tests/elements_tests.rs` covers picker label cache reuse and dirty glyph-atlas upload behavior, with `cpu.system.picker_text_cached_encode` retained as the gated workspace perf signal after the slower direct-shape/full-upload row was retired.
- `crates/ui-core/tests/coalesce_tests.rs` covers adjacency-preserving coalescing and caller-owned scratch reuse.

## Examples
```rust
use oxide_ui_core::{HorizontalShiftingText, TextFieldPolicy};
use oxide_ui_core::elements::CharFilter;

let policy = TextFieldPolicy::new(CharFilter::Alphabetic).with_max_length(Some(15));
let text = HorizontalShiftingText::new(policy, 32.0, 1_200);
assert_eq!(text.value(), "");
```

## Changelog
- 2026-07-14: added C43 frame-scoped text preparation, provisional glyph handles, merged atlas publication, opt-in text counters, and allocation coverage.
- 2026-07-13: added C26 node-local retained geometry, generation-checked dynamic slots, complete nested affine/opacity composition, and synchronized hit/accessibility geometry.
- 2026-07-13: Added hard retained CPU/prepared-GPU budgets, generation-aware LRU eviction, hot-entry protection, explicit churn suppression, zero-budget direct rebuild, and complete cache diagnostics for C23.

- 2026-07-13: changed `ImageView` to emit bounded `Image` commands with natural-pixel source crops; zero-inset `NineSlice` remains removed from the image-view path.
- 2026-06-02: Added `coalesce_adjacent_draws_reuse` so hot host frame loops can reuse draw-command coalescing scratch storage.
- 2026-06-01: fixed child layout skip logic so stable child geometry cannot bypass dirty descendants during an ancestor relayout.
- 2026-06-01: bounded `CollectionView` variable measurement cache entries and added cold-entry eviction coverage for large key/revision churn.
- 2026-06-01: ASCII wrapped-label cache misses now shape once for break decisions instead of reshaping every growing word candidate; `cpu.system.wrapped_label_cached_encode` remains the gated workspace perf row after the slower legacy audit row was retired.
- 2026-06-01: picker label encoding now reuses cached shaped label lines and dirty glyph-atlas uploads; `cpu.system.picker_text_cached_encode` remains the gated workspace perf row after the slower direct-shape/full-upload audit row was retired.
- 2026-05-31: added `UiSurface::add_node` and `UiSurface::remove_node` scoped structural mutations so common add/remove paths skip clean sibling layout and replay retained sibling draw lists.
- 2026-05-31: text inputs now consume `oxide_text::ShapedCursorMap` for cached caret widths and pointer hit testing, with grapheme-safe max-length filtering.
- 2026-05-31: `SurfaceRouter::encode_with_overlays` now retained-encodes overlay and popup surfaces and exposes `RetainedCompositionStats`.
- 2026-05-31: added optional `Measure::changed_item_range` so epoch-backed variable collection prefixes can repair small item changes without full item-revision scans.
- 2026-05-31: added optional `Measure::item_index_for_key` for keyed collection focus/hover reconciliation after far reorders, plus indexed-vs-scan authoring perf coverage.
- 2026-05-31: added `UiSurface::mark_node_dirty` so text/image/camera content dirty classes can skip layout and reuse clean retained sibling subtrees.
- 2026-05-31: transform translation now applies during draw and hit-test traversal instead of being baked into logical layout, enabling transform-only edits to skip layout.
- 2026-05-31: added optional `Measure::collection_revision` so variable collection row/grid prefix offsets can be reused across epoch-stable scroll passes without full signature scans.
- 2026-05-31: retained draw-list replay now fails closed without text-atlas context and exposes checked retained append helpers for no-text and multi-atlas replay.
- 2026-05-31: added descendant-only layout dirtiness and `LayoutStats::measured_children` so fixed-outer internal subtree edits avoid parent child-measure scans.
- 2026-05-31: text-input pointer picking now uses cached shaped-cluster prefix maps, and unwrapped label shapes are shared with prefix metrics.
- 2026-05-31: text-input pointer picking now handles pure RTL shaped cursor maps with descending visual caret positions.
- 2026-05-31: `TextCtx::set_fallback_fonts` now feeds fallback-font shaped runs into label/text-input glyph encoding and cursor-prefix metrics.
- 2026-05-31: `TextCtx` now derives text-input caret prefix metrics from one shaped run instead of reshaping every prefix boundary on cache miss.
- 2026-05-31: keyed collection focus and hover now reconcile through `Measure::item_key` during layout so focus survives data reorders and navigation materializes the actual new item key instead of an index-derived placeholder.
- 2026-05-31: added live `TextCtx` retained atlas snapshot helpers for `UiSurface` and `SurfaceRouter`, guarded so cached glyph replay only sees an uploaded atlas.
- 2026-05-31: direct-clean child layout skipping now avoids entering unchanged child subtrees during dirty relayout parent loops.
- 2026-05-31: added non-draw dirty-class coverage so accessibility/hit-test metadata updates preserve clean retained draw-list reuse.
- 2026-05-31: added opacity/clip dirty-class coverage so paint-only retained edits skip layout while reusing cached descendants and siblings.
- 2026-05-31: added per-node layout dirtiness and `LayoutStats` so clean sibling subtrees can be skipped during incremental relayout.
- 2026-05-31: added multi-atlas retained text replay checking so cached glyph drawlists require explicit revisions for every atlas they reference.
- 2026-05-25: synthesized standard indices for unindexed four-vertex image meshes so GPU triangle-list backends receive a complete quad.
- 2026-05-31: added scoped style edits and bounded per-node retained draw-list reuse for dirty leaf surface encodes, covered by `cpu.authoring.surface_retained.dirty_leaf_encode`.
- 2026-05-31: added text-atlas revision checking for cached draw-list replay so retained glyph geometry is rejected after atlas slot eviction or reset.
- 2026-05-22: documented async layout worker-ordering tests and their blocked-job cleanup guard.
- 2026-05-31: removed the native camera preview draw-list command and authoring flag from the product UI surface.
- 2026-05-14: documented `draw_replay` because glyph replay now resolves and translates vertex spans for CPU composition paths.
- 2026-05-10: reused the existing character-range byte mapping in `elements.rs` text insertion and removed the single-point byte helper.
- 2026-04-25: reused wrapped-label shaping results after release-mode A/B showed `cpu.component.label.encode` improving from p50 1155.122 us/op, p95 1165.781 to p50 1013.186 us/op, p95 1037.539 in focused runs, with the refreshed full workspace row at p50 987.312 us/op, p95 1004.876.
- 2026-04-25: preallocated the `prepare_draws` clip stack after release-mode A/B showed the representative clipping workload improving from p50 6.881 us/op, p95 10.977 to p50 5.368 us/op, p95 5.398.
- 2026-03-28: moved the legacy iOS modal overlay and popup blur-card contract into shared `elements.rs` popup primitives so downstream apps can draw one common fullscreen/panel blur treatment.
- 2026-03-28: moved the popup key-window, dismissal approval, touch-exception, and content-size refresh lifecycle contract into shared `overlay.rs` popup primitives so downstream apps stop rebuilding those window semantics locally.
- 2026-03-28: moved the legacy iOS `Badge` / `BadgeableButton` overlay contract into shared `elements.rs` badge primitives so downstream apps draw the image-backed quarter-size top-right badge instead of a numeric pill.
- 2026-03-28: moved the legacy iOS `Spinner` contract into shared `elements.rs` so downstream apps stop supplying phase/stroke data and the iOS host can promote spinner draws into native `UIActivityIndicatorViewStyleLarge` views.
- 2026-03-28: moved the legacy iOS `SlidingSwitch` long-press gate, inactivity timeout, and out-of-bounds cancellation contract into shared `SlidingSwitchState` so downstream apps stop rebuilding those gesture rules around the primitive.
- 2026-03-26: added and re-exported the shared `emitter` module so app crates can reuse deterministic CAEmitter-style burst sampling instead of keeping app-local particle math.
- 2026-03-13: centralized shared cubic-bezier easing and required-field shake helpers in `anim.rs` so app crates can drop duplicated motion math.
- 2026-03-13: re-exported the generic text-input primitives from the crate root so app crates can deduplicate their input state machines onto Oxide.
- 2026-03-28: re-exported the shared legacy multi-column picker controller and scroll-end commit types from the crate root so app crates can consume the old iOS picker contract directly.
- 2026-03-13: re-exported the generic popup/wheel-picker interaction primitives so app crates can share one drag/snap/dismiss controller instead of reimplementing them in scene code.
