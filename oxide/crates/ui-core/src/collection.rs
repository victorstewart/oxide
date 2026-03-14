//! Virtualized CollectionView: vertical grid and horizontal row.
//! Provides measurement, culling, diff-driven reusable cells, and focus/hover navigation.

use crate::{anim, DrawListBuilder};
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;
use oxide_renderer_api as gfx;
use oxide_timing as timing;

#[derive(Clone, Copy, Debug)]
pub enum CollectionMode {
    VerticalGrid { col_width: f32, spacing: f32 },
    HorizontalRow { row_height: f32, spacing: f32 },
}

/// Returns the length in the secondary axis under a constraint in the primary axis.
/// - VerticalGrid: returns item height for a given column width.
/// - HorizontalRow: returns item width for a given row height.
pub trait Measure {
    fn measure(&mut self, index: usize, constraint: f32) -> f32;
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
    last_columns: usize,
    // virtualization
    visible: Vec<VisibleItem>,
    cells: BTreeMap<usize, u32>, // index -> cell id
    pool: CellPool,
    transition: Option<CellTransition>,
    cell_anim: BTreeMap<usize, CellAnimState>,
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
            last_columns: 1,
            visible: Vec::new(),
            cells: BTreeMap::new(),
            pool: CellPool::default(),
            transition: None,
            cell_anim: BTreeMap::new(),
        }
    }

    pub fn set_mode(&mut self, mode: CollectionMode) {
        self.mode = mode;
    }
    pub fn mode(&self) -> &CollectionMode {
        &self.mode
    }
    pub fn set_count(&mut self, count: usize) {
        self.count = count;
        if self.focus.map(|f| f >= count).unwrap_or(false) {
            self.focus = None
        }
        self.cells.retain(|idx, _| *idx < count);
        self.cell_anim.retain(|idx, _| *idx < count);
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
        self.cells.get(&index).copied()
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
        // Diff-driven cell reuse
        let mut newset: BTreeSet<usize> = BTreeSet::new();
        for v in &self.visible {
            newset.insert(v.index);
        }
        let old_keys: Vec<usize> = self.cells.keys().copied().collect();
        for k in old_keys {
            if !newset.contains(&k) {
                if let Some(id) = self.cells.remove(&k) {
                    self.pool.free(id);
                }
            }
        }
        for v in &self.visible {
            self.cells.entry(v.index).or_insert_with(|| self.pool.alloc());
        }
        if self.transition.is_some() {
            self.cell_anim.retain(|idx, _| newset.contains(idx));
            for idx in newset.iter() {
                self.cell_anim
                    .entry(*idx)
                    .or_insert(CellAnimState { start_ms: now, cached_scale: None });
            }
        }
        // Render (collect data first to avoid borrow conflict)
        let visible_items: Vec<(usize, gfx::RectF, u32, bool, bool)> = self
            .visible
            .iter()
            .map(|v| {
                let cell_id = self.cells.get(&v.index).copied().unwrap_or(0);
                let focused = self.focus == Some(v.index);
                let hovered = self.hover == Some(v.index);
                (v.index, v.rect_screen, cell_id, focused, hovered)
            })
            .collect();

        for (index, rect_screen, cell_id, focused, hovered) in visible_items {
            let rect = self.transition_rect(index, rect_screen, now);
            renderer.render(cell_id, index, rect, focused, hovered, b);
        }
        self.content
    }

    fn transition_rect(&mut self, index: usize, rect: gfx::RectF, now: u64) -> gfx::RectF {
        let Some(transition) = self.transition else {
            return rect;
        };
        let Some(state) = self.cell_anim.get_mut(&index) else {
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
                    self.cell_anim.remove(&index);
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

    fn layout_grid<M: Measure>(
        &mut self,
        viewport: gfx::RectF,
        col_width: f32,
        spacing: f32,
        measure: &mut M,
    ) {
        let cols = ((viewport.w + spacing) / (col_width + spacing)).floor().max(1.0) as usize;
        self.last_columns = cols.max(1);
        let mut y = 0.0_f32;
        let mut i = 0_usize;
        let scroll = self.scroll;
        let vp_h = viewport.h;
        while i < self.count {
            let n = core::cmp::min(cols, self.count - i);
            // Measure row heights
            let mut heights: alloc::vec::Vec<f32> = alloc::vec::Vec::with_capacity(n);
            for k in 0..n {
                heights.push(measure.measure(i + k, col_width).max(1.0));
            }
            let row_h = heights.iter().cloned().fold(0.0f32, f32::max);
            let row_top = y;
            let row_bot = y + row_h;
            if row_bot >= scroll && row_top <= scroll + vp_h {
                for k in 0..n {
                    let x = viewport.x + (col_width + spacing) * (k as f32);
                    let screen_y = viewport.y + (row_top - scroll);
                    let rect = gfx::RectF::new(x, screen_y, col_width, heights[k]);
                    self.visible.push(VisibleItem { index: i + k, rect_screen: rect });
                }
            }
            y += row_h + spacing;
            i += n;
        }
        self.content = ContentMetrics { content_w: viewport.w, content_h: y.max(0.0) };
    }

    fn layout_row<M: Measure>(
        &mut self,
        viewport: gfx::RectF,
        row_h: f32,
        spacing: f32,
        measure: &mut M,
    ) {
        let mut x = 0.0_f32;
        let scroll = self.scroll;
        let vp_w = viewport.w;
        for i in 0..self.count {
            let w = measure.measure(i, row_h).max(1.0);
            let left = x;
            let right = x + w;
            if right >= scroll && left <= scroll + vp_w {
                let screen_x = viewport.x + (left - scroll);
                let rect = gfx::RectF::new(screen_x, viewport.y, w, row_h);
                self.visible.push(VisibleItem { index: i, rect_screen: rect });
            }
            x += w + spacing;
        }
        self.content = ContentMetrics { content_w: x.max(0.0), content_h: row_h };
    }

    // ---- Navigation and pointer semantics ----
    pub fn pointer_move(&mut self, x: f32, y: f32) {
        self.hover = None;
        for v in &self.visible {
            if pt_in(x, y, v.rect_screen) {
                self.hover = Some(v.index);
                break;
            }
        }
    }

    pub fn pointer_click(&mut self, x: f32, y: f32) -> Option<usize> {
        self.pointer_move(x, y);
        self.focus = self.hover;
        self.focus
    }

    pub fn focus_set(&mut self, idx: Option<usize>) {
        self.focus = idx.filter(|i| *i < self.count);
    }
    pub fn focus_move_left(&mut self) {
        match self.mode {
            CollectionMode::HorizontalRow { .. } => {
                if let Some(i) = self.focus {
                    if i > 0 {
                        self.focus = Some(i - 1);
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
                        self.focus = Some(i + 1);
                    }
                } else if self.count > 0 {
                    self.focus = Some(0);
                }
            }
            CollectionMode::VerticalGrid { .. } => self.focus_move_by(1),
        }
    }
    pub fn focus_move_up(&mut self) {
        if let CollectionMode::VerticalGrid { .. } = self.mode {
            if let Some(i) = self.focus {
                if i >= self.last_columns {
                    self.focus = Some(i - self.last_columns);
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
                }
            }
        }
    }
    fn focus_move_by(&mut self, delta: isize) {
        if let Some(i) = self.focus {
            let ni =
                (i as isize + delta).clamp(0, (self.count.saturating_sub(1)) as isize) as usize;
            self.focus = Some(ni);
        } else if self.count > 0 {
            self.focus = Some(0);
        }
    }
}

#[inline]
fn pt_in(x: f32, y: f32, r: gfx::RectF) -> bool {
    x >= r.x && y >= r.y && x <= r.x + r.w && y <= r.y + r.h
}

extern crate alloc;
