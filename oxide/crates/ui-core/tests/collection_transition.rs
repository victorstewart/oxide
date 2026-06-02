use oxide_renderer_api as gfx;
use oxide_ui_core::{collection, DrawListBuilder};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::ops::Range;

struct DummyMeasure;

impl collection::Measure for DummyMeasure {
    fn measure(&mut self, _index: usize, constraint: f32) -> f32 {
        constraint.min(48.0)
    }
}

struct FixedMeasure {
    calls: usize,
    extent: f32,
}

impl collection::Measure for FixedMeasure {
    fn measure(&mut self, _index: usize, _constraint: f32) -> f32 {
        self.calls += 1;
        self.extent
    }

    fn fixed_extent(&self, _constraint: f32) -> Option<f32> {
        Some(self.extent)
    }
}

struct CaptureRenderer {
    rects: Vec<gfx::RectF>,
}

impl collection::CellRenderer for CaptureRenderer {
    fn render(
        &mut self,
        _cell_id: u32,
        _index: usize,
        rect: gfx::RectF,
        _focused: bool,
        _hovered: bool,
        _b: &mut DrawListBuilder,
    ) {
        self.rects.push(rect);
    }
}

struct VariableMeasure {
    calls: usize,
    order: Vec<u64>,
    revisions: Vec<u64>,
}

impl VariableMeasure {
    fn new(count: usize) -> Self {
        Self { calls: 0, order: (0..count as u64).collect(), revisions: vec![0; count] }
    }

    fn height_for_key(key: u64) -> f32 {
        28.0 + (key % 7) as f32 * 3.0
    }
}

impl collection::Measure for VariableMeasure {
    fn measure(&mut self, index: usize, _constraint: f32) -> f32 {
        self.calls += 1;
        Self::height_for_key(self.order[index])
    }

    fn item_key(&self, index: usize) -> collection::ItemKey {
        collection::ItemKey(self.order[index])
    }

    fn item_revision(&self, index: usize) -> u64 {
        self.revisions[index]
    }
}

struct EpochVariableMeasure {
    calls: usize,
    order: Vec<u64>,
    revisions: Vec<u64>,
    epoch: u64,
    changed: Option<Range<usize>>,
    revision_queries: Cell<usize>,
}

impl EpochVariableMeasure {
    fn new(count: usize) -> Self {
        Self {
            calls: 0,
            order: (0..count as u64).collect(),
            revisions: vec![0; count],
            epoch: 1,
            changed: None,
            revision_queries: Cell::new(0),
        }
    }
}

impl collection::Measure for EpochVariableMeasure {
    fn measure(&mut self, index: usize, _constraint: f32) -> f32 {
        self.calls += 1;
        VariableMeasure::height_for_key(self.order[index])
    }

    fn item_key(&self, index: usize) -> collection::ItemKey {
        collection::ItemKey(self.order[index])
    }

    fn item_revision(&self, index: usize) -> u64 {
        self.revision_queries.set(self.revision_queries.get().saturating_add(1));
        self.revisions[index]
    }

    fn collection_revision(&self) -> Option<u64> {
        Some(self.epoch)
    }

    fn changed_item_range(&self) -> Option<Range<usize>> {
        self.changed.clone()
    }
}

struct IndexedKeyMeasure {
    order: Vec<u64>,
    index_by_key: BTreeMap<u64, usize>,
    item_key_queries: Cell<usize>,
    index_queries: Cell<usize>,
}

impl IndexedKeyMeasure {
    fn new(count: usize) -> Self {
        let mut measure = Self {
            order: (0..count as u64).collect(),
            index_by_key: BTreeMap::new(),
            item_key_queries: Cell::new(0),
            index_queries: Cell::new(0),
        };
        measure.rebuild_index();
        measure
    }

    fn move_key_to(&mut self, key: u64, target: usize) {
        let Some(source) = self.order.iter().position(|candidate| *candidate == key) else {
            return;
        };
        let key = self.order.remove(source);
        self.order.insert(target.min(self.order.len()), key);
        self.rebuild_index();
    }

    fn rebuild_index(&mut self) {
        self.index_by_key.clear();
        for (index, key) in self.order.iter().enumerate() {
            self.index_by_key.insert(*key, index);
        }
    }

    fn reset_queries(&self) {
        self.item_key_queries.set(0);
        self.index_queries.set(0);
    }
}

impl collection::Measure for IndexedKeyMeasure {
    fn measure(&mut self, _index: usize, _constraint: f32) -> f32 {
        40.0
    }

    fn item_key(&self, index: usize) -> collection::ItemKey {
        self.item_key_queries.set(self.item_key_queries.get().saturating_add(1));
        collection::ItemKey(self.order[index])
    }

    fn item_index_for_key(&self, key: collection::ItemKey) -> Option<usize> {
        self.index_queries.set(self.index_queries.get().saturating_add(1));
        self.index_by_key.get(&key.0).copied()
    }

    fn fixed_extent(&self, _constraint: f32) -> Option<f32> {
        Some(40.0)
    }
}

struct IdentityRenderer {
    records: Vec<(usize, u32, bool)>,
}

impl collection::CellRenderer for IdentityRenderer {
    fn render(
        &mut self,
        cell_id: u32,
        index: usize,
        _rect: gfx::RectF,
        focused: bool,
        _hovered: bool,
        _b: &mut DrawListBuilder,
    ) {
        self.records.push((index, cell_id, focused));
    }
}

#[test]
fn shrink_grow_transition_scales_on_entry() {
    let mut view = collection::CollectionView::new(collection::CollectionMode::VerticalGrid {
        col_width: 60.0,
        spacing: 6.0,
    });
    view.set_transition(Some(collection::CellTransition::shrink_grow(360, 0.82, 1.08)));
    view.set_count(1);
    let mut measure = DummyMeasure;
    let mut renderer = CaptureRenderer { rects: Vec::new() };
    let viewport = gfx::RectF::new(0.0, 0.0, 120.0, 120.0);
    let mut builder = DrawListBuilder::new();
    view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);
    assert_eq!(renderer.rects.len(), 1);
    let rect = renderer.rects[0];
    assert!(rect.w < 60.0 + f32::EPSILON);
    assert!(rect.h < 60.0 + f32::EPSILON);
}

#[test]
fn fixed_extent_grid_skips_full_collection_measurement() {
    let mut view = collection::CollectionView::new(collection::CollectionMode::VerticalGrid {
        col_width: 50.0,
        spacing: 5.0,
    });
    view.set_count(10_000);
    view.set_scroll(3_000.0);
    let mut measure = FixedMeasure { calls: 0, extent: 40.0 };
    let mut renderer = CaptureRenderer { rects: Vec::new() };
    let viewport = gfx::RectF::new(0.0, 0.0, 220.0, 160.0);
    let mut builder = DrawListBuilder::new();

    let content = view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);

    assert_eq!(measure.calls, 0);
    assert!(renderer.rects.len() <= 24);
    assert!(content.content_h > viewport.h);
}

#[test]
fn fixed_extent_row_skips_full_collection_measurement() {
    let mut view = collection::CollectionView::new(collection::CollectionMode::HorizontalRow {
        row_height: 44.0,
        spacing: 4.0,
    });
    view.set_count(10_000);
    view.set_scroll(2_400.0);
    let mut measure = FixedMeasure { calls: 0, extent: 64.0 };
    let mut renderer = CaptureRenderer { rects: Vec::new() };
    let viewport = gfx::RectF::new(0.0, 0.0, 300.0, 80.0);
    let mut builder = DrawListBuilder::new();

    let content = view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);

    assert_eq!(measure.calls, 0);
    assert!(renderer.rects.len() <= 7);
    assert!(content.content_w > viewport.w);
}

#[test]
fn variable_grid_reuses_measurements_by_item_key_and_revision() {
    let mut view = collection::CollectionView::new(collection::CollectionMode::VerticalGrid {
        col_width: 50.0,
        spacing: 4.0,
    });
    view.set_count(256);
    view.set_scroll(1_600.0);
    let mut measure = VariableMeasure::new(256);
    let mut renderer = CaptureRenderer { rects: Vec::new() };
    let viewport = gfx::RectF::new(0.0, 0.0, 120.0, 140.0);
    let mut builder = DrawListBuilder::new();

    view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);
    let first_calls = measure.calls;
    assert!(first_calls > 0);

    measure.calls = 0;
    renderer.rects.clear();
    view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);
    assert_eq!(measure.calls, 0, "warm variable layout should reuse cached item measurements");

    measure.revisions[0] = 1;
    view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);
    assert_eq!(measure.calls, 1, "only the revised item should be remeasured");
}

#[test]
fn variable_grid_measure_cache_evicts_cold_entries_under_large_key_churn() {
    let mut view = collection::CollectionView::new(collection::CollectionMode::VerticalGrid {
        col_width: 50.0,
        spacing: 4.0,
    });
    view.set_count(20_000);
    let mut measure = VariableMeasure::new(20_000);
    let mut renderer = CaptureRenderer { rects: Vec::new() };
    let viewport = gfx::RectF::new(0.0, 0.0, 120.0, 140.0);
    let mut builder = DrawListBuilder::new();

    view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);
    assert!(measure.calls >= 20_000);

    measure.calls = 0;
    renderer.rects.clear();
    builder.clear();
    view.set_scroll(42_000.0);
    view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);

    assert!(
        measure.calls > 0,
        "large variable-grid churn should evict cold measurements instead of retaining every key",
    );
    assert!(
        measure.calls < 32,
        "cold-cache repair should remeasure only newly visible cells, not rebuild the full prefix",
    );
    assert!(!renderer.rects.is_empty());
}

#[test]
fn variable_row_reuses_prefix_offsets_and_remeasures_revised_item() {
    let mut view = collection::CollectionView::new(collection::CollectionMode::HorizontalRow {
        row_height: 44.0,
        spacing: 4.0,
    });
    view.set_count(256);
    view.set_scroll(1_500.0);
    let mut measure = VariableMeasure::new(256);
    let mut renderer = CaptureRenderer { rects: Vec::new() };
    let viewport = gfx::RectF::new(0.0, 0.0, 180.0, 80.0);
    let mut builder = DrawListBuilder::new();

    view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);
    let first_calls = measure.calls;
    assert!(first_calls > 0);
    assert!(!renderer.rects.is_empty());

    measure.calls = 0;
    renderer.rects.clear();
    view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);
    assert_eq!(measure.calls, 0, "warm variable row should reuse cached prefix measurements");
    assert!(!renderer.rects.is_empty());

    measure.revisions[0] = 1;
    renderer.rects.clear();
    view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);
    assert_eq!(measure.calls, 1, "only the revised variable-width item should be remeasured");
}

#[test]
fn variable_grid_epoch_reuses_prefix_offsets_without_full_signature_scan() {
    let mut view = collection::CollectionView::new(collection::CollectionMode::VerticalGrid {
        col_width: 50.0,
        spacing: 4.0,
    });
    view.set_count(512);
    view.set_scroll(1_600.0);
    let mut measure = EpochVariableMeasure::new(512);
    let mut renderer = CaptureRenderer { rects: Vec::new() };
    let viewport = gfx::RectF::new(0.0, 0.0, 120.0, 140.0);
    let mut builder = DrawListBuilder::new();

    view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);
    assert!(measure.revision_queries.get() >= 512);
    assert!(measure.calls > 0);

    measure.calls = 0;
    measure.revision_queries.set(0);
    renderer.rects.clear();
    view.set_scroll(1_700.0);
    view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);
    assert_eq!(measure.calls, 0, "warm epoch-stable grid should reuse measured items");
    assert!(
        measure.revision_queries.get() < 32,
        "warm epoch-stable grid should skip full signature scan",
    );
    assert!(!renderer.rects.is_empty());

    measure.calls = 0;
    measure.revision_queries.set(0);
    measure.revisions[0] = 1;
    measure.epoch = measure.epoch.wrapping_add(1);
    renderer.rects.clear();
    view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);
    assert_eq!(measure.calls, 1, "epoch change should remeasure only the revised item");
    assert!(
        measure.revision_queries.get() >= 512,
        "epoch change should rescan signatures before rebuilding offsets",
    );
}

#[test]
fn variable_row_epoch_reuses_prefix_offsets_without_full_signature_scan() {
    let mut view = collection::CollectionView::new(collection::CollectionMode::HorizontalRow {
        row_height: 44.0,
        spacing: 4.0,
    });
    view.set_count(512);
    view.set_scroll(1_500.0);
    let mut measure = EpochVariableMeasure::new(512);
    let mut renderer = CaptureRenderer { rects: Vec::new() };
    let viewport = gfx::RectF::new(0.0, 0.0, 180.0, 80.0);
    let mut builder = DrawListBuilder::new();

    view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);
    assert!(measure.revision_queries.get() >= 512);
    assert!(measure.calls > 0);

    measure.calls = 0;
    measure.revision_queries.set(0);
    renderer.rects.clear();
    view.set_scroll(1_620.0);
    view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);
    assert_eq!(measure.calls, 0, "warm epoch-stable row should reuse measured items");
    assert!(
        measure.revision_queries.get() < 16,
        "warm epoch-stable row should skip full signature scan",
    );
    assert!(!renderer.rects.is_empty());

    measure.calls = 0;
    measure.revision_queries.set(0);
    measure.revisions[0] = 1;
    measure.epoch = measure.epoch.wrapping_add(1);
    renderer.rects.clear();
    view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);
    assert_eq!(measure.calls, 1, "epoch change should remeasure only the revised row item");
    assert!(
        measure.revision_queries.get() >= 512,
        "epoch change should rescan signatures before rebuilding offsets",
    );
}

#[test]
fn variable_grid_dirty_range_repairs_prefix_without_full_signature_scan() {
    let mut view = collection::CollectionView::new(collection::CollectionMode::VerticalGrid {
        col_width: 50.0,
        spacing: 4.0,
    });
    view.set_count(512);
    view.set_scroll(1_600.0);
    let mut measure = EpochVariableMeasure::new(512);
    let mut renderer = CaptureRenderer { rects: Vec::new() };
    let viewport = gfx::RectF::new(0.0, 0.0, 120.0, 140.0);
    let mut builder = DrawListBuilder::new();

    let before = view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);
    assert!(measure.revision_queries.get() >= 512);
    assert!(measure.calls > 0);

    measure.calls = 0;
    measure.revision_queries.set(0);
    measure.revisions[507] = 1;
    measure.changed = Some(507..508);
    measure.epoch = measure.epoch.wrapping_add(1);
    renderer.rects.clear();
    builder.clear();
    let after = view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);

    assert_eq!(measure.calls, 1, "dirty-range grid should remeasure only the revised item");
    assert!(
        measure.revision_queries.get() < 32,
        "dirty-range grid should avoid a full collection signature scan",
    );
    assert_eq!(after.content_h, before.content_h);
    assert!(!renderer.rects.is_empty());
}

#[test]
fn variable_row_dirty_range_repairs_prefix_without_full_signature_scan() {
    let mut view = collection::CollectionView::new(collection::CollectionMode::HorizontalRow {
        row_height: 44.0,
        spacing: 4.0,
    });
    view.set_count(512);
    view.set_scroll(1_500.0);
    let mut measure = EpochVariableMeasure::new(512);
    let mut renderer = CaptureRenderer { rects: Vec::new() };
    let viewport = gfx::RectF::new(0.0, 0.0, 180.0, 80.0);
    let mut builder = DrawListBuilder::new();

    let before = view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);
    assert!(measure.revision_queries.get() >= 512);
    assert!(measure.calls > 0);

    measure.calls = 0;
    measure.revision_queries.set(0);
    measure.revisions[508] = 1;
    measure.changed = Some(508..509);
    measure.epoch = measure.epoch.wrapping_add(1);
    renderer.rects.clear();
    builder.clear();
    let after = view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);

    assert_eq!(measure.calls, 1, "dirty-range row should remeasure only the revised item");
    assert!(
        measure.revision_queries.get() < 32,
        "dirty-range row should avoid a full collection signature scan",
    );
    assert_eq!(after.content_w, before.content_w);
    assert!(!renderer.rects.is_empty());
}

#[test]
fn keyed_collection_identity_survives_visible_reorder() {
    let mut view = collection::CollectionView::new(collection::CollectionMode::VerticalGrid {
        col_width: 80.0,
        spacing: 4.0,
    });
    view.set_count(3);
    let mut measure =
        VariableMeasure { calls: 0, order: vec![10, 20, 30], revisions: vec![0, 0, 0] };
    let mut renderer = IdentityRenderer { records: Vec::new() };
    let viewport = gfx::RectF::new(0.0, 0.0, 100.0, 180.0);
    let mut builder = DrawListBuilder::new();

    view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);
    let key_20_cell = renderer
        .records
        .iter()
        .find_map(|(index, cell_id, _)| (measure.order[*index] == 20).then_some(*cell_id))
        .expect("initial key 20 cell");
    assert_eq!(view.pointer_click(10.0, 45.0), Some(1));

    measure.order = vec![30, 10, 20];
    measure.calls = 0;
    renderer.records.clear();
    view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);

    let mut focused_keys = Vec::new();
    let key_20_cell_after = renderer
        .records
        .iter()
        .find_map(|(index, cell_id, focused)| {
            if *focused {
                focused_keys.push(measure.order[*index]);
            }
            (measure.order[*index] == 20).then_some(*cell_id)
        })
        .expect("reordered key 20 cell");
    assert_eq!(measure.calls, 0, "reordered visible items should reuse key-based measurements");
    assert_eq!(key_20_cell_after, key_20_cell);
    assert_eq!(focused_keys, vec![20]);
}

#[test]
fn keyed_collection_focus_set_and_navigation_use_item_keys_after_reorder() {
    let mut view = collection::CollectionView::new(collection::CollectionMode::VerticalGrid {
        col_width: 80.0,
        spacing: 4.0,
    });
    view.set_count(3);
    let mut measure =
        VariableMeasure { calls: 0, order: vec![10, 20, 30], revisions: vec![0, 0, 0] };
    let mut renderer = IdentityRenderer { records: Vec::new() };
    let viewport = gfx::RectF::new(0.0, 0.0, 100.0, 180.0);
    let mut builder = DrawListBuilder::new();

    view.focus_set(Some(1));
    view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);
    assert_eq!(view.focus(), Some(1));

    measure.order = vec![30, 10, 20];
    renderer.records.clear();
    view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);

    let focused_after_reorder: Vec<u64> = renderer
        .records
        .iter()
        .filter_map(|(index, _, focused)| focused.then_some(measure.order[*index]))
        .collect();
    assert_eq!(view.focus(), Some(2));
    assert_eq!(focused_after_reorder, vec![20]);

    view.focus_move_left();
    renderer.records.clear();
    view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);
    let focused_after_move: Vec<u64> = renderer
        .records
        .iter()
        .filter_map(|(index, _, focused)| focused.then_some(measure.order[*index]))
        .collect();
    assert_eq!(view.focus(), Some(1));
    assert_eq!(focused_after_move, vec![10]);
}

#[test]
fn keyed_collection_focus_reconcile_uses_item_index_lookup_after_far_reorder() {
    let mut view = collection::CollectionView::new(collection::CollectionMode::VerticalGrid {
        col_width: 80.0,
        spacing: 4.0,
    });
    view.set_count(256);
    view.focus_set_key(Some(200), Some(collection::ItemKey(200)));
    let mut measure = IndexedKeyMeasure::new(256);
    let mut renderer = IdentityRenderer { records: Vec::new() };
    let viewport = gfx::RectF::new(0.0, 0.0, 100.0, 180.0);
    let mut builder = DrawListBuilder::new();

    view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);
    assert_eq!(view.focus(), Some(200));

    measure.move_key_to(200, 220);
    measure.reset_queries();
    renderer.records.clear();
    builder.clear();
    view.layout_and_render(viewport, &mut measure, &mut renderer, &mut builder);

    assert_eq!(view.focus(), Some(220));
    assert_eq!(measure.index_queries.get(), 1);
    assert!(
        measure.item_key_queries.get() < 16,
        "focus reconciliation should not scan the collection after a keyed reorder",
    );
}
