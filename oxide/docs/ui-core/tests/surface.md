# ui-core `tests/surface.rs`

## Intention and purpose
This integration test suite verifies retained `UiSurface` behavior: dirty-class classification, incremental layout, retained draw-list replay, router composition, hit testing, and scoped tree mutation. It exists so performance fixes in `NodeTree` and `UiSurface` cannot silently degrade correctness or fall back to full-tree work.

## Relation to the rest of the code
- `oxide_ui_core::UiSurface` wraps `NodeTree`, animation overrides, retained draw caches, and layout stats.
- `oxide_ui_core::NodeTree` owns node identity, layout dirtiness, retained per-node draw lists, and hit testing.
- `oxide_renderer_api::DrawList` is the output boundary used to verify retained replay and draw-cache safety.

Call flow:

- test setup -> `UiSurface::add_node` or `UiSurface::tree_mut`
- mutation -> `UiSurface::edit_style`, `mark_dirty`, or `mark_node_dirty`
- layout -> `UiSurface::layout` -> `NodeTree::layout`
- encode -> `UiSurface::encode_retained` -> `NodeTree::encode_draws_retained`

## Entry points list
- `retained_encode_reuses_clean_drawlist_and_rebuilds_after_dirty()`
  Verifies whole-surface retained replay and rebuild after paint dirtiness.
- `retained_dirty_leaf_reuses_clean_sibling_subtree()`
  Verifies dirty leaf rebuilds do not force clean sibling subtree redraw.
- `text_ctx_retained_snapshot_requires_clean_uploaded_atlas()`
  Verifies retained text atlas snapshots are exposed only after dirty atlas uploads are cleared.
- `layout_dirty_subtree_skips_clean_sibling_subtree()`
  Verifies a layout-dirty leaf skips unrelated sibling branches.
- `descendant_only_layout_dirty_skips_parent_measurement()`
  Verifies descendant-only layout dirtiness avoids parent child-measure scans.
- `ancestor_relayout_does_not_skip_dirty_descendant_with_stable_child_rect()`
  Verifies an ancestor relayout cannot skip a stable child that has dirty descendants.
- `scoped_tree_add_remove_skips_clean_sibling_layout_and_reuses_retained_draws()`
  Verifies common structural mutations stay incremental.
- `retained_cache_enforces_hard_bytes_and_preserves_exact_output()`
  Verifies post-render logical bytes never exceed the configured budget and eviction does not change flattened output.
- `retained_cache_suppresses_churn_then_readmits_stable_nodes()`
  Verifies explicit invalidation-streak suppression rejects churn temporarily and readmits after the retry generation.
- `retained_cache_lru_protects_hot_root_while_evicting_cold_children()`
  Verifies recent hit history influences eviction without changing the in-flight immutable snapshot.
- `retained_cache_budget_never_evicts_caller_owned_text_or_image_chunks()`
  Verifies a zero node-cache budget preserves independent caller-owned chunk identity and exact mixed output.
- Additional tests in the file cover transform-only motion, opacity/clip dirty classes, content dirty classes, non-draw dirty classes, router retained composition, and hit-test identity.

## Logic narrative
The tests construct small retained trees with known geometry, run a cold layout or encode, mutate one scoped property, then assert both the resulting geometry and the diagnostic counters. The mixed ancestor/descendant test specifically marks an ancestor layout-dirty while a child keeps the same outer rect and a grandchild requires relayout; this locks the skip predicate so `descendant_layout_dirty` prevents an unsafe subtree skip.

## Preconditions and postconditions
- Tests use public `UiSurface` and crate-root types, not private node internals.
- Passing means clean sibling subtrees remain skippable, dirty descendants are reached, and retained draw-list reuse stays bounded by dirty classes and atlas safety.

## Edge cases and failure modes
- A paint-only mutation must rebuild draw caches without setting layout dirtiness.
- Accessibility and hit-test metadata dirtiness must not rebuild renderer-facing draws.
- A stable child rect is not enough to skip layout if `descendant_layout_dirty` is still set.
- Missing node ids must return false instead of dirtying the surface.
- Budget eviction must invalidate ancestor sequence references so no supposedly evicted descendant remains indirectly retained.
- A zero-byte policy must retain no node chunks or sequence metadata while still producing exact draw order and external resource dependencies.

## Concurrency and memory behavior
The tests are synchronous and allocate only local surfaces/builders. Production retained draw-list caches move `DrawList` values into nodes or surfaces and replay them by appending; the tests verify that clean replay does not require mutation of unrelated subtrees.

## Performance notes
The suite uses `LayoutStats` and `RetainedNodeStats` as explanatory counters. The target behavior is fewer visited nodes, fewer measured children, and retained subtree reuse when dirty classes do not require a full rebuild.

## Feature flags and cfgs
No special features or OS targets are required. The suite runs on the normal macOS Rust test host.

## Testing and benchmarks
Run with:

```sh
cargo test --locked -j$(sysctl -n hw.ncpu) -p oxide-ui-core --test surface
```

Related perf rows live in `oxide-perf-runner`: `cpu.layout.dirty_subtree.incremental_relayout`, `cpu.layout.descendant_only.incremental_relayout`, `cpu.layout.node_content_dirty.retained_replay`, `cpu.layout.non_draw_dirty.retained_reuse`, `cpu.architecture.retained.cache_pressure.hot_reuse`, and `cpu.architecture.retained.cache_pressure.one_use_churn`.

## Examples
```rust
let dirty = surface.layout(180.0, 90.0);
assert!(dirty.skipped_subtrees >= 1);
assert!(dirty.visited_nodes < cold.visited_nodes);
```

## Changelog
- 2026-07-13: Added C23 hard-budget, LRU/hot protection, churn suppression/readmission, external identity, and exact zero-budget fallback coverage.
- 2026-06-01: Added coverage that dirty text atlases are not retained-replay-safe until the dirty upload is cleared.
- 2026-06-01: Added coverage for ancestor relayout combined with dirty descendants under a stable child rect.
