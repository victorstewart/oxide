//! Virtualized CollectionView: vertical grid and horizontal row.
//! Provides measurement, culling, diff-driven reusable cells, and focus/hover navigation.

use crate::{anim, DrawListBuilder};
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use oxide_renderer_api as gfx;
use oxide_timing as timing;

const MEASURE_CACHE_CAP: usize = 16_384;
const MEASURE_CACHE_PRUNE_TARGET: usize = MEASURE_CACHE_CAP * 3 / 4;

#[derive(Clone, Copy, Debug)]
pub enum CollectionMode {
    VerticalGrid { col_width: f32, spacing: f32 },
    HorizontalRow { row_height: f32, spacing: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ItemKey(pub u64);

/// Returns the length in the secondary axis under a constraint in the primary axis.
/// - VerticalGrid: returns item height for a given column width.
/// - HorizontalRow: returns item width for a given row height.
pub trait Measure {
    fn measure(&mut self, index: usize, constraint: f32) -> f32;

    fn item_key(&self, index: usize) -> ItemKey {
        ItemKey(index as u64)
    }

    fn item_index_for_key(&self, _key: ItemKey) -> Option<usize> {
        None
    }

    fn item_revision(&self, _index: usize) -> u64 {
        0
    }

    fn collection_revision(&self) -> Option<u64> {
        None
    }

    fn changed_item_range(&self) -> Option<core::ops::Range<usize>> {
        None
    }

    fn fixed_extent(&self, _constraint: f32) -> Option<f32> {
        None
    }
}

/// Renders a cell at a rect (screen coords). Focus/hover semantics provided.
pub trait CellRenderer {
    fn render(
        &mut self,
        cell_id: u32,
        index: usize,
        rect: gfx::RectF,
        focused: bool,
        hovered: bool,
        b: &mut DrawListBuilder,
    );
}

#[derive(Clone, Copy, Debug)]
pub struct ContentMetrics {
    pub content_w: f32,
    pub content_h: f32,
}

#[derive(Clone, Debug)]
struct VisibleItem {
    index: usize,
    key: ItemKey,
    rect_screen: gfx::RectF,
}

/// Animation applied to cells as they enter the viewport.
#[derive(Clone, Copy, Debug)]
pub enum CellTransition {
    ShrinkGrow { duration_ms: u32, min_scale: f32, overshoot: f32 },
}

impl CellTransition {
    pub fn shrink_grow(duration_ms: u32, min_scale: f32, overshoot: f32) -> Self {
        Self::ShrinkGrow { duration_ms, min_scale, overshoot }
    }
}

#[derive(Clone, Copy, Debug)]
struct CellAnimState {
    start_ms: u64,
    cached_scale: Option<(u64, f32)>, // (timestamp, scale) for caching
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct MeasureCacheKey {
    key: ItemKey,
    constraint_bits: u32,
    revision: u64,
}

struct CachedMeasure {
    extent: f32,
    last_used: u64,
}

#[derive(Default)]
struct VariableGridPrefixCache {
    cols: usize,
    col_width_bits: u32,
    spacing_bits: u32,
    count: usize,
    row_heights: Vec<f32>,
    row_offsets: Vec<f32>,
    row_signatures: Vec<u64>,
    content_h: f32,
    collection_revision: Option<u64>,
}

impl VariableGridPrefixCache {
    fn clear(&mut self) {
        self.cols = 0;
        self.col_width_bits = 0;
        self.spacing_bits = 0;
        self.count = 0;
        self.row_heights.clear();
        self.row_offsets.clear();
        self.row_signatures.clear();
        self.content_h = 0.0;
        self.collection_revision = None;
    }
}

#[derive(Default)]
struct VariableRowPrefixCache {
    row_height_bits: u32,
    spacing_bits: u32,
    count: usize,
    widths: Vec<f32>,
    offsets: Vec<f32>,
    signatures: Vec<u64>,
    content_w: f32,
    collection_revision: Option<u64>,
}

impl VariableRowPrefixCache {
    fn clear(&mut self) {
        self.row_height_bits = 0;
        self.spacing_bits = 0;
        self.count = 0;
        self.widths.clear();
        self.offsets.clear();
        self.signatures.clear();
        self.content_w = 0.0;
        self.collection_revision = None;
    }
}

#[derive(Default)]
struct CellPool {
    next: u32,
    free: Vec<u32>,
}
impl CellPool {
    fn alloc(&mut self) -> u32 {
        if let Some(id) = self.free.pop() {
            id
        } else {
            self.next = self.next.saturating_add(1).max(1);
            self.next
        }
    }
    fn free(&mut self, id: u32) {
        self.free.push(id);
    }
}

/// Virtualized collection view state.
pub struct CollectionView {
    mode: CollectionMode,
    count: usize,
    scroll: f32, // vertical for grid, horizontal for row
    content: ContentMetrics,
    // navigation
    focus: Option<usize>,
    hover: Option<usize>,
    focus_key: Option<ItemKey>,
    hover_key: Option<ItemKey>,
    last_columns: usize,
    // virtualization
    visible: Vec<VisibleItem>,
    cells: BTreeMap<ItemKey, u32>, // item key -> cell id
    pool: CellPool,
    transition: Option<CellTransition>,
    cell_anim: BTreeMap<ItemKey, CellAnimState>,
    measure_cache: BTreeMap<MeasureCacheKey, CachedMeasure>,
    measure_cache_clock: u64,
    grid_prefix: VariableGridPrefixCache,
    row_prefix: VariableRowPrefixCache,
    row_heights: Vec<f32>,
    row_keys: Vec<ItemKey>,
}

impl CollectionView {
    pub fn new(mode: CollectionMode) -> Self {
        Self {
            mode,
            count: 0,
            scroll: 0.0,
            content: ContentMetrics { content_w: 0.0, content_h: 0.0 },
            focus: None,
            hover: None,
            focus_key: None,
            hover_key: None,
            last_columns: 1,
            visible: Vec::new(),
            cells: BTreeMap::new(),
            pool: CellPool::default(),
            transition: None,
            cell_anim: BTreeMap::new(),
            measure_cache: BTreeMap::new(),
            measure_cache_clock: 0,
            grid_prefix: VariableGridPrefixCache::default(),
            row_prefix: VariableRowPrefixCache::default(),
            row_heights: Vec::new(),
            row_keys: Vec::new(),
        }
    }

    pub fn set_mode(&mut self, mode: CollectionMode) {
        self.mode = mode;
        self.measure_cache.clear();
        self.measure_cache_clock = 0;
        self.grid_prefix.clear();
        self.row_prefix.clear();
    }
    pub fn mode(&self) -> &CollectionMode {
        &self.mode
    }
    pub fn set_count(&mut self, count: usize) {
        if self.count != count {
            self.grid_prefix.clear();
            self.row_prefix.clear();
        }
        self.count = count;
        if self.focus.map(|f| f >= count).unwrap_or(false) {
            self.focus = None;
            self.focus_key = None;
        }
        if self.hover.map(|h| h >= count).unwrap_or(false) {
            self.hover = None;
            self.hover_key = None;
        }
    }
    pub fn set_scroll(&mut self, value: f32) {
        self.scroll = value.max(0.0);
    }
    pub fn scroll(&self) -> f32 {
        self.scroll
    }
    pub fn focus(&self) -> Option<usize> {
        self.focus
    }
    pub fn hover(&self) -> Option<usize> {
        self.hover
    }
    pub fn content_metrics(&self) -> ContentMetrics {
        self.content
    }
    pub fn cell_id_for(&self, index: usize) -> Option<u32> {
        self.cells.get(&ItemKey(index as u64)).copied()
    }
    pub fn cell_id_for_key(&self, key: ItemKey) -> Option<u32> {
        self.cells.get(&key).copied()
    }

    /// Configure the transition effect applied to cells as they appear.
    pub fn set_transition(&mut self, transition: Option<CellTransition>) {
        self.transition = transition;
        self.cell_anim.clear();
    }

    /// Compute layout and render visible cells. Returns content metrics for scroll bars.
    pub fn layout_and_render<M: Measure, R: CellRenderer>(
        &mut self,
        viewport: gfx::RectF,
        measure: &mut M,
        renderer: &mut R,
        b: &mut DrawListBuilder,
    ) -> ContentMetrics {
        self.reconcile_navigation_keys(measure);
        self.visible.clear();
        match self.mode {
            CollectionMode::VerticalGrid { col_width, spacing } => {
                self.layout_grid(viewport, col_width, spacing, measure)
            }
            CollectionMode::HorizontalRow { row_height, spacing } => {
                self.layout_row(viewport, row_height, spacing, measure)
            }
        }
        let now = timing::now_ms();
        let visible = &self.visible;
        let pool = &mut self.pool;
        self.cells.retain(|key, cell_id| {
            let keep = collection_visible_contains_key(visible, *key);
            if !keep {
                pool.free(*cell_id);
            }
            keep
        });
        for v in &self.visible {
            self.cells.entry(v.key).or_insert_with(|| self.pool.alloc());
        }
        if self.transition.is_some() {
            let visible = &self.visible;
            self.cell_anim.retain(|key, _| collection_visible_contains_key(visible, *key));
            for v in &self.visible {
                self.cell_anim
                    .entry(v.key)
                    .or_insert(CellAnimState { start_ms: now, cached_scale: None });
            }
        }
        for slot in 0..self.visible.len() {
            let visible = self.visible[slot].clone();
            let cell_id = self.cells.get(&visible.key).copied().unwrap_or(0);
            let focused =
                self.focus_key.map_or(self.focus == Some(visible.index), |key| key == visible.key);
            let hovered =
                self.hover_key.map_or(self.hover == Some(visible.index), |key| key == visible.key);
            let rect = self.transition_rect(visible.key, visible.rect_screen, now);
            renderer.render(cell_id, visible.index, rect, focused, hovered, b);
        }
        self.content
    }

    fn reconcile_navigation_keys<M: Measure>(&mut self, measure: &M) {
        self.focus =
            collection_reconciled_index_for_key(measure, self.count, self.focus, self.focus_key);
        if self.focus.is_none() {
            self.focus_key = None;
        } else if self.focus_key.is_none() {
            self.focus_key = self.focus.map(|index| measure.item_key(index));
        }
        self.hover =
            collection_reconciled_index_for_key(measure, self.count, self.hover, self.hover_key);
        if self.hover.is_none() {
            self.hover_key = None;
        } else if self.hover_key.is_none() {
            self.hover_key = self.hover.map(|index| measure.item_key(index));
        }
    }

    fn transition_rect(&mut self, key: ItemKey, rect: gfx::RectF, now: u64) -> gfx::RectF {
        let Some(transition) = self.transition else {
            return rect;
        };
        let Some(state) = self.cell_anim.get_mut(&key) else {
            return rect;
        };
        match transition {
            CellTransition::ShrinkGrow { duration_ms, min_scale, overshoot } => {
                if duration_ms == 0 {
                    return rect;
                }

                // Check cache first (valid for same timestamp)
                let scale = if let Some((cached_ts, cached_scale)) = state.cached_scale {
                    if cached_ts == now {
                        cached_scale
                    } else {
                        // Recalculate and cache
                        let elapsed = now.saturating_sub(state.start_ms) as f32;
                        let pct = (elapsed / duration_ms as f32).clamp(0.0, 1.0);
                        let new_scale = anim::helpers::shrink_grow_scale(pct, min_scale, overshoot);
                        state.cached_scale = Some((now, new_scale));
                        new_scale
                    }
                } else {
                    // First calculation
                    let elapsed = now.saturating_sub(state.start_ms) as f32;
                    let pct = (elapsed / duration_ms as f32).clamp(0.0, 1.0);
                    let new_scale = anim::helpers::shrink_grow_scale(pct, min_scale, overshoot);
                    state.cached_scale = Some((now, new_scale));
                    new_scale
                };

                if (scale - 1.0).abs() < f32::EPSILON {
                    // Animation complete, remove from tracking
                    self.cell_anim.remove(&key);
                    return rect;
                }
                let cx = rect.x + rect.w * 0.5;
                let cy = rect.y + rect.h * 0.5;
                let w = rect.w * scale;
                let h = rect.h * scale;
                gfx::RectF::new(cx - w * 0.5, cy - h * 0.5, w, h)
            }
        }
    }

    fn rebuild_grid_prefix<M: Measure>(
        &mut self,
        cols: usize,
        col_width: f32,
        spacing: f32,
        measure: &mut M,
    ) {
        let row_count = self.count.saturating_add(cols.saturating_sub(1)) / cols.max(1);
        let col_width_bits = col_width.to_bits();
        let spacing_bits = spacing.to_bits();
        let collection_revision = measure.collection_revision();
        let prefix_compatible = self.grid_prefix.cols == cols
            && self.grid_prefix.col_width_bits == col_width_bits
            && self.grid_prefix.spacing_bits == spacing_bits
            && self.grid_prefix.count == self.count
            && self.grid_prefix.row_heights.len() == row_count
            && self.grid_prefix.row_offsets.len() == row_count
            && self.grid_prefix.row_signatures.len() == row_count;
        if collection_revision.is_some()
            && self.grid_prefix.collection_revision == collection_revision
            && prefix_compatible
        {
            return;
        }
        if collection_revision.is_some()
            && self.grid_prefix.collection_revision.is_some()
            && prefix_compatible
        {
            if let Some(range) = measure
                .changed_item_range()
                .and_then(|range| collection_normalized_range(range, self.count))
            {
                let first_row = range.start / cols.max(1);
                let end_row = range.end.saturating_add(cols.saturating_sub(1)) / cols.max(1);
                let end_row = end_row.min(row_count);
                for row in first_row..end_row {
                    let start = row * cols;
                    let n = core::cmp::min(cols, self.count - start);
                    let signature = collection_grid_row_signature(measure, start, n);
                    if self.grid_prefix.row_signatures[row] != signature
                        || self.grid_prefix.row_heights[row] <= 0.0
                    {
                        let mut row_h = 0.0_f32;
                        for k in 0..n {
                            let index = start + k;
                            let key = measure.item_key(index);
                            let height = collection_cached_measure(
                                &mut self.measure_cache,
                                &mut self.measure_cache_clock,
                                measure,
                                index,
                                key,
                                col_width,
                            );
                            row_h = row_h.max(height);
                        }
                        self.grid_prefix.row_heights[row] = row_h;
                        self.grid_prefix.row_signatures[row] = signature;
                    }
                }
                let mut y =
                    if first_row == 0 { 0.0 } else { self.grid_prefix.row_offsets[first_row] };
                for row in first_row..row_count {
                    self.grid_prefix.row_offsets[row] = y;
                    y += self.grid_prefix.row_heights[row] + spacing;
                }
                self.grid_prefix.content_h = y.max(0.0);
                self.grid_prefix.collection_revision = collection_revision;
                return;
            }
        }
        if self.grid_prefix.cols != cols
            || self.grid_prefix.col_width_bits != col_width_bits
            || self.grid_prefix.spacing_bits != spacing_bits
            || self.grid_prefix.count != self.count
        {
            self.grid_prefix.cols = cols;
            self.grid_prefix.col_width_bits = col_width_bits;
            self.grid_prefix.spacing_bits = spacing_bits;
            self.grid_prefix.count = self.count;
            self.grid_prefix.row_heights.clear();
            self.grid_prefix.row_offsets.clear();
            self.grid_prefix.row_signatures.clear();
            self.grid_prefix.collection_revision = None;
        }
        self.grid_prefix.row_heights.resize(row_count, 0.0);
        self.grid_prefix.row_offsets.resize(row_count, 0.0);
        self.grid_prefix.row_signatures.resize(row_count, 0);

        let mut y = 0.0_f32;
        for row in 0..row_count {
            let start = row * cols;
            let n = core::cmp::min(cols, self.count - start);
            let signature = collection_grid_row_signature(measure, start, n);
            if self.grid_prefix.row_signatures[row] != signature
                || self.grid_prefix.row_heights[row] <= 0.0
            {
                let mut row_h = 0.0_f32;
                for k in 0..n {
                    let index = start + k;
                    let key = measure.item_key(index);
                    let height = collection_cached_measure(
                        &mut self.measure_cache,
                        &mut self.measure_cache_clock,
                        measure,
                        index,
                        key,
                        col_width,
                    );
                    row_h = row_h.max(height);
                }
                self.grid_prefix.row_heights[row] = row_h;
                self.grid_prefix.row_signatures[row] = signature;
            }
            self.grid_prefix.row_offsets[row] = y;
            y += self.grid_prefix.row_heights[row] + spacing;
        }
        self.grid_prefix.content_h = y.max(0.0);
        self.grid_prefix.collection_revision = collection_revision;
    }

    fn rebuild_row_prefix<M: Measure>(&mut self, row_h: f32, spacing: f32, measure: &mut M) {
        let row_height_bits = row_h.to_bits();
        let spacing_bits = spacing.to_bits();
        let collection_revision = measure.collection_revision();
        let prefix_compatible = self.row_prefix.row_height_bits == row_height_bits
            && self.row_prefix.spacing_bits == spacing_bits
            && self.row_prefix.count == self.count
            && self.row_prefix.widths.len() == self.count
            && self.row_prefix.offsets.len() == self.count
            && self.row_prefix.signatures.len() == self.count;
        if collection_revision.is_some()
            && self.row_prefix.collection_revision == collection_revision
            && prefix_compatible
        {
            return;
        }
        if collection_revision.is_some()
            && self.row_prefix.collection_revision.is_some()
            && prefix_compatible
        {
            if let Some(range) = measure
                .changed_item_range()
                .and_then(|range| collection_normalized_range(range, self.count))
            {
                for index in range.clone() {
                    let key = measure.item_key(index);
                    let signature = collection_item_signature(key, measure.item_revision(index));
                    if self.row_prefix.signatures[index] != signature
                        || self.row_prefix.widths[index] <= 0.0
                    {
                        self.row_prefix.widths[index] = collection_cached_measure(
                            &mut self.measure_cache,
                            &mut self.measure_cache_clock,
                            measure,
                            index,
                            key,
                            row_h,
                        );
                        self.row_prefix.signatures[index] = signature;
                    }
                }
                let mut x =
                    if range.start == 0 { 0.0 } else { self.row_prefix.offsets[range.start] };
                for index in range.start..self.count {
                    self.row_prefix.offsets[index] = x;
                    x += self.row_prefix.widths[index] + spacing;
                }
                self.row_prefix.content_w = x.max(0.0);
                self.row_prefix.collection_revision = collection_revision;
                return;
            }
        }
        if self.row_prefix.row_height_bits != row_height_bits
            || self.row_prefix.spacing_bits != spacing_bits
            || self.row_prefix.count != self.count
        {
            self.row_prefix.row_height_bits = row_height_bits;
            self.row_prefix.spacing_bits = spacing_bits;
            self.row_prefix.count = self.count;
            self.row_prefix.widths.clear();
            self.row_prefix.offsets.clear();
            self.row_prefix.signatures.clear();
            self.row_prefix.collection_revision = None;
        }
        self.row_prefix.widths.resize(self.count, 0.0);
        self.row_prefix.offsets.resize(self.count, 0.0);
        self.row_prefix.signatures.resize(self.count, 0);

        let mut x = 0.0_f32;
        for index in 0..self.count {
            let key = measure.item_key(index);
            let signature = collection_item_signature(key, measure.item_revision(index));
            if self.row_prefix.signatures[index] != signature
                || self.row_prefix.widths[index] <= 0.0
            {
                self.row_prefix.widths[index] = collection_cached_measure(
                    &mut self.measure_cache,
                    &mut self.measure_cache_clock,
                    measure,
                    index,
                    key,
                    row_h,
                );
                self.row_prefix.signatures[index] = signature;
            }
            self.row_prefix.offsets[index] = x;
            x += self.row_prefix.widths[index] + spacing;
        }
        self.row_prefix.content_w = x.max(0.0);
        self.row_prefix.collection_revision = collection_revision;
    }

    fn layout_grid<M: Measure>(
        &mut self,
        viewport: gfx::RectF,
        col_width: f32,
        spacing: f32,
        measure: &mut M,
    ) {
        let cols = ((viewport.w + spacing) / (col_width + spacing)).floor().max(1.0) as usize;
        self.last_columns = cols.max(1);
        if let Some(row_h) = measure.fixed_extent(col_width).map(|extent| extent.max(1.0)) {
            let row_stride = row_h + spacing;
            let row_count = (self.count + cols - 1) / cols;
            let first_row = (self.scroll / row_stride).floor().max(0.0) as usize;
            let last_row = ((self.scroll + viewport.h) / row_stride).ceil().max(0.0) as usize;
            let last_row = last_row.min(row_count.saturating_sub(1));
            for row in first_row..=last_row {
                let base = row * cols;
                if base >= self.count {
                    break;
                }
                let row_top = row as f32 * row_stride;
                let n = core::cmp::min(cols, self.count - base);
                for k in 0..n {
                    let x = viewport.x + (col_width + spacing) * (k as f32);
                    let screen_y = viewport.y + (row_top - self.scroll);
                    let rect = gfx::RectF::new(x, screen_y, col_width, row_h);
                    let index = base + k;
                    self.visible.push(VisibleItem {
                        index,
                        key: measure.item_key(index),
                        rect_screen: rect,
                    });
                }
            }
            let content_h = if row_count == 0 {
                0.0
            } else {
                row_count as f32 * row_h + row_count.saturating_sub(1) as f32 * spacing
            };
            self.content = ContentMetrics { content_w: viewport.w, content_h };
            return;
        }
        let scroll = self.scroll;
        let vp_h = viewport.h;
        self.rebuild_grid_prefix(cols, col_width, spacing, measure);
        let row_count = self.grid_prefix.row_heights.len();
        if row_count == 0 {
            self.content = ContentMetrics { content_w: viewport.w, content_h: 0.0 };
            return;
        }
        let first_row = collection_first_visible_span(
            &self.grid_prefix.row_offsets,
            &self.grid_prefix.row_heights,
            scroll,
        );
        let end_row =
            collection_visible_end(&self.grid_prefix.row_offsets, scroll + vp_h).min(row_count);
        for row in first_row..end_row {
            let i = row * cols;
            if i >= self.count {
                break;
            }
            let n = core::cmp::min(cols, self.count - i);
            let row_top = self.grid_prefix.row_offsets[row];
            self.row_heights.clear();
            self.row_keys.clear();
            for k in 0..n {
                let index = i + k;
                let key = measure.item_key(index);
                let height = collection_cached_measure(
                    &mut self.measure_cache,
                    &mut self.measure_cache_clock,
                    measure,
                    index,
                    key,
                    col_width,
                );
                self.row_keys.push(key);
                self.row_heights.push(height);
            }
            for k in 0..n {
                let x = viewport.x + (col_width + spacing) * (k as f32);
                let screen_y = viewport.y + (row_top - scroll);
                let rect = gfx::RectF::new(x, screen_y, col_width, self.row_heights[k]);
                self.visible.push(VisibleItem {
                    index: i + k,
                    key: self.row_keys[k],
                    rect_screen: rect,
                });
            }
        }
        self.content =
            ContentMetrics { content_w: viewport.w, content_h: self.grid_prefix.content_h };
    }

    fn layout_row<M: Measure>(
        &mut self,
        viewport: gfx::RectF,
        row_h: f32,
        spacing: f32,
        measure: &mut M,
    ) {
        if let Some(w) = measure.fixed_extent(row_h).map(|extent| extent.max(1.0)) {
            if self.count == 0 {
                self.content = ContentMetrics { content_w: 0.0, content_h: row_h };
                return;
            }
            let stride = w + spacing;
            let first = (self.scroll / stride).floor().max(0.0) as usize;
            let last = ((self.scroll + viewport.w) / stride).ceil().max(0.0) as usize;
            let last = last.min(self.count.saturating_sub(1));
            for i in first..=last {
                let left = i as f32 * stride;
                let screen_x = viewport.x + (left - self.scroll);
                let rect = gfx::RectF::new(screen_x, viewport.y, w, row_h);
                self.visible.push(VisibleItem {
                    index: i,
                    key: measure.item_key(i),
                    rect_screen: rect,
                });
            }
            let content_w = if self.count == 0 {
                0.0
            } else {
                self.count as f32 * w + self.count.saturating_sub(1) as f32 * spacing
            };
            self.content = ContentMetrics { content_w, content_h: row_h };
            return;
        }
        let scroll = self.scroll;
        let vp_w = viewport.w;
        self.rebuild_row_prefix(row_h, spacing, measure);
        let first = collection_first_visible_span(
            &self.row_prefix.offsets,
            &self.row_prefix.widths,
            scroll,
        );
        let end = collection_visible_end(&self.row_prefix.offsets, scroll + vp_w).min(self.count);
        for i in first..end {
            let key = measure.item_key(i);
            let w = self.row_prefix.widths[i];
            let left = self.row_prefix.offsets[i];
            let screen_x = viewport.x + (left - scroll);
            let rect = gfx::RectF::new(screen_x, viewport.y, w, row_h);
            self.visible.push(VisibleItem { index: i, key, rect_screen: rect });
        }
        self.content = ContentMetrics { content_w: self.row_prefix.content_w, content_h: row_h };
    }

    // ---- Navigation and pointer semantics ----
    pub fn pointer_move(&mut self, x: f32, y: f32) {
        self.hover = None;
        self.hover_key = None;
        for v in &self.visible {
            if pt_in(x, y, v.rect_screen) {
                self.hover = Some(v.index);
                self.hover_key = Some(v.key);
                break;
            }
        }
    }

    pub fn pointer_click(&mut self, x: f32, y: f32) -> Option<usize> {
        self.pointer_move(x, y);
        self.focus = self.hover;
        self.focus_key = self.hover_key;
        self.focus
    }

    pub fn focus_set(&mut self, idx: Option<usize>) {
        self.focus = idx.filter(|i| *i < self.count);
        self.focus_key = None;
    }
    pub fn focus_set_key(&mut self, idx: Option<usize>, key: Option<ItemKey>) {
        self.focus = idx.filter(|i| *i < self.count);
        self.focus_key = key;
    }
    pub fn focus_move_left(&mut self) {
        match self.mode {
            CollectionMode::HorizontalRow { .. } => {
                if let Some(i) = self.focus {
                    if i > 0 {
                        let next = i - 1;
                        self.focus = Some(next);
                        self.focus_key = None;
                    }
                }
            }
            CollectionMode::VerticalGrid { .. } => self.focus_move_by(-1),
        }
    }
    pub fn focus_move_right(&mut self) {
        match self.mode {
            CollectionMode::HorizontalRow { .. } => {
                if let Some(i) = self.focus {
                    if i + 1 < self.count {
                        let next = i + 1;
                        self.focus = Some(next);
                        self.focus_key = None;
                    }
                } else if self.count > 0 {
                    self.focus = Some(0);
                    self.focus_key = None;
                }
            }
            CollectionMode::VerticalGrid { .. } => self.focus_move_by(1),
        }
    }
    pub fn focus_move_up(&mut self) {
        if let CollectionMode::VerticalGrid { .. } = self.mode {
            if let Some(i) = self.focus {
                if i >= self.last_columns {
                    let next = i - self.last_columns;
                    self.focus = Some(next);
                    self.focus_key = None;
                }
            }
        }
    }
    pub fn focus_move_down(&mut self) {
        if let CollectionMode::VerticalGrid { .. } = self.mode {
            if let Some(i) = self.focus {
                let j = i + self.last_columns;
                if j < self.count {
                    self.focus = Some(j);
                    self.focus_key = None;
                }
            }
        }
    }
    fn focus_move_by(&mut self, delta: isize) {
        if let Some(i) = self.focus {
            let ni =
                (i as isize + delta).clamp(0, (self.count.saturating_sub(1)) as isize) as usize;
            self.focus = Some(ni);
            self.focus_key = None;
        } else if self.count > 0 {
            self.focus = Some(0);
            self.focus_key = None;
        }
    }
}

#[inline]
fn pt_in(x: f32, y: f32, r: gfx::RectF) -> bool {
    x >= r.x && y >= r.y && x <= r.x + r.w && y <= r.y + r.h
}

#[inline]
fn collection_visible_contains_key(visible: &[VisibleItem], key: ItemKey) -> bool {
    visible.iter().any(|item| item.key == key)
}

fn collection_item_signature(key: ItemKey, revision: u64) -> u64 {
    key.0.wrapping_mul(0x9E37_79B1_85EB_CA87).rotate_left(7)
        ^ revision.wrapping_mul(0xC2B2_AE3D_27D4_EB4F).rotate_left(19)
}

fn collection_grid_row_signature<M: Measure>(measure: &M, start: usize, len: usize) -> u64 {
    let mut signature = (len as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
    for offset in 0..len {
        let index = start + offset;
        let key = measure.item_key(index);
        let item = collection_item_signature(key, measure.item_revision(index));
        signature = signature.rotate_left(11) ^ item.wrapping_add(offset as u64);
    }
    signature
}

fn collection_reconciled_index_for_key<M: Measure>(
    measure: &M,
    count: usize,
    index: Option<usize>,
    key: Option<ItemKey>,
) -> Option<usize> {
    let Some(key) = key else {
        return index.filter(|i| *i < count);
    };
    if let Some(index) = index {
        if index < count && measure.item_key(index) == key {
            return Some(index);
        }
    }
    if let Some(index) = measure.item_index_for_key(key) {
        if index < count && measure.item_key(index) == key {
            return Some(index);
        }
    }
    (0..count).find(|candidate| measure.item_key(*candidate) == key)
}

fn collection_normalized_range(
    range: core::ops::Range<usize>,
    count: usize,
) -> Option<core::ops::Range<usize>> {
    let start = range.start.min(count);
    let end = range.end.min(count);
    if start < end {
        Some(start..end)
    } else {
        None
    }
}

fn collection_first_visible_span(offsets: &[f32], spans: &[f32], scroll: f32) -> usize {
    let mut lo = 0usize;
    let mut hi = spans.len().min(offsets.len());
    while lo < hi {
        let mid = lo + ((hi - lo) / 2);
        if offsets[mid] + spans[mid] < scroll {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo
}

fn collection_visible_end(offsets: &[f32], end: f32) -> usize {
    let mut lo = 0usize;
    let mut hi = offsets.len();
    while lo < hi {
        let mid = lo + ((hi - lo) / 2);
        if offsets[mid] <= end {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo
}

fn collection_cached_measure<M: Measure>(
    cache: &mut BTreeMap<MeasureCacheKey, CachedMeasure>,
    clock: &mut u64,
    measure: &mut M,
    index: usize,
    key: ItemKey,
    constraint: f32,
) -> f32 {
    *clock = clock.wrapping_add(1);
    let cache_key = MeasureCacheKey {
        key,
        constraint_bits: constraint.to_bits(),
        revision: measure.item_revision(index),
    };
    if let Some(entry) = cache.get_mut(&cache_key) {
        entry.last_used = *clock;
        return entry.extent;
    }
    let extent = measure.measure(index, constraint).max(1.0);
    if cache.len() >= MEASURE_CACHE_CAP {
        collection_prune_measure_cache(cache);
    }
    cache.insert(cache_key, CachedMeasure { extent, last_used: *clock });
    extent
}

fn collection_prune_measure_cache(cache: &mut BTreeMap<MeasureCacheKey, CachedMeasure>) {
    if cache.len() < MEASURE_CACHE_CAP {
        return;
    }
    let remove_count = cache.len().saturating_sub(MEASURE_CACHE_PRUNE_TARGET).saturating_add(1);
    let mut cold: Vec<(u64, MeasureCacheKey)> =
        cache.iter().map(|(key, entry)| (entry.last_used, *key)).collect();
    cold.sort_unstable_by_key(|(last_used, _)| *last_used);
    for (_, key) in cold.into_iter().take(remove_count) {
        cache.remove(&key);
    }
}

extern crate alloc;
