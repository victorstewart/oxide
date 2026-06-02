//! Scene graph coordination utilities for Oxide surfaces.
//! Provides NodeTree management, asynchronous layout, interaction gating, and
//! scatter-style transition helpers inspired by AsyncDisplayKit flows.

use crate::{
    anim, append_retained_drawlist,
    capture::SurfaceCapture,
    drawlist_retained_replay_safe_for,
    elements::TextCtx,
    layout_async::AsyncLayoutCoordinator,
    overlay::{OverlayPointerResult, OverlayStack, PopupManager, RetainedOverlayStats},
    DrawListBuilder, LayoutRect, LayoutStats, NodeId, NodeStyle, NodeTree, RetainedNodeStats,
};
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;
use oxide_renderer_api as gfx;
use oxide_timing as timing;

/// Safe-area and chrome metadata supplied by the host platform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChromeMetrics {
    pub safe_insets: gfx::Insets,
    pub status_bar_height: f32,
}

impl Default for ChromeMetrics {
    fn default() -> Self {
        Self { safe_insets: gfx::Insets::new(0.0, 0.0, 0.0, 0.0), status_bar_height: 0.0 }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DirtyClass {
    Style = 0,
    Layout = 1,
    Text = 2,
    Paint = 3,
    Transform = 4,
    Opacity = 5,
    Clip = 6,
    ImageContent = 7,
    CameraFrame = 8,
    Accessibility = 9,
    HitTest = 10,
}

impl DirtyClass {
    #[inline]
    const fn bit(self) -> u16 {
        1_u16 << (self as u8)
    }
}

const DRAW_DIRTY_BITS: u16 = DirtyClass::Style.bit()
    | DirtyClass::Layout.bit()
    | DirtyClass::Text.bit()
    | DirtyClass::Paint.bit()
    | DirtyClass::Transform.bit()
    | DirtyClass::Opacity.bit()
    | DirtyClass::Clip.bit()
    | DirtyClass::ImageContent.bit()
    | DirtyClass::CameraFrame.bit();

const ALL_DIRTY_BITS: u16 =
    DRAW_DIRTY_BITS | DirtyClass::Accessibility.bit() | DirtyClass::HitTest.bit();

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DirtySet {
    bits: u16,
}

impl DirtySet {
    #[inline]
    pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    #[inline]
    pub const fn all() -> Self {
        Self { bits: ALL_DIRTY_BITS }
    }

    #[inline]
    pub fn mark(&mut self, class: DirtyClass) {
        self.bits |= class.bit();
    }

    #[inline]
    pub fn clear(&mut self, class: DirtyClass) {
        self.bits &= !class.bit();
    }

    #[inline]
    pub const fn contains(self, class: DirtyClass) -> bool {
        self.bits & class.bit() != 0
    }

    #[inline]
    pub const fn is_empty(self) -> bool {
        self.bits == 0
    }

    #[inline]
    pub const fn affects_draw(self) -> bool {
        self.bits & DRAW_DIRTY_BITS != 0
    }

    #[inline]
    pub fn clear_draw_affecting(&mut self) {
        self.bits &= !DRAW_DIRTY_BITS;
    }
}

fn transform_changed(before: &NodeStyle, after: &NodeStyle) -> bool {
    before.transform.tx != after.transform.tx
        || before.transform.ty != after.transform.ty
        || before.transform.sx != after.transform.sx
        || before.transform.sy != after.transform.sy
        || before.transform.rot_rad != after.transform.rot_rad
}

fn style_change_affects_parent_layout(before: &NodeStyle, after: &NodeStyle) -> bool {
    before.size != after.size
        || before.min_size != after.min_size
        || before.max_size != after.max_size
        || before.margin != after.margin
        || before.flex_grow != after.flex_grow
        || before.overflow != after.overflow
}

fn style_change_affects_content_layout(before: &NodeStyle, after: &NodeStyle) -> bool {
    style_change_affects_parent_layout(before, after)
        || before.axis != after.axis
        || before.padding != after.padding
        || before.gap != after.gap
}

fn style_change_affects_paint(before: &NodeStyle, after: &NodeStyle) -> bool {
    style_change_affects_content_layout(before, after)
        || before.background != after.background
        || before.corner_radii != after.corner_radii
        || before.opacity != after.opacity
        || before.shadow_alpha != after.shadow_alpha
        || before.clip != after.clip
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetainedDrawStatus {
    Rebuilt,
    Reused,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RetainedCompositionStats {
    pub current_reused: usize,
    pub current_rebuilt: usize,
    pub overlay_reused: usize,
    pub overlay_rebuilt: usize,
    pub popup_reused: usize,
    pub popup_rebuilt: usize,
}

impl RetainedCompositionStats {
    fn record_current(&mut self, status: RetainedDrawStatus) {
        match status {
            RetainedDrawStatus::Rebuilt => {
                self.current_rebuilt = self.current_rebuilt.saturating_add(1);
            }
            RetainedDrawStatus::Reused => {
                self.current_reused = self.current_reused.saturating_add(1);
            }
        }
    }

    fn record_overlays(&mut self, stats: RetainedOverlayStats) {
        self.overlay_reused = self.overlay_reused.saturating_add(stats.reused_surfaces);
        self.overlay_rebuilt = self.overlay_rebuilt.saturating_add(stats.rebuilt_surfaces);
    }

    fn record_popups(&mut self, stats: RetainedOverlayStats) {
        self.popup_reused = self.popup_reused.saturating_add(stats.reused_surfaces);
        self.popup_rebuilt = self.popup_rebuilt.saturating_add(stats.rebuilt_surfaces);
    }
}

#[derive(Default, Debug)]
struct InteractionGate {
    depth: usize,
}

impl InteractionGate {
    fn begin(&mut self) {
        self.depth = self.depth.saturating_add(1);
    }

    fn end(&mut self) {
        if self.depth > 0 {
            self.depth -= 1;
        }
    }

    fn is_blocked(&self) -> bool {
        self.depth > 0
    }
}

/// RAII guard that keeps user interaction blocked while alive.
#[must_use]
pub struct InteractionBlockGuard<'a> {
    gate: &'a mut InteractionGate,
    active: bool,
}

impl<'a> InteractionBlockGuard<'a> {
    fn new(gate: &'a mut InteractionGate) -> Self {
        gate.begin();
        Self { gate, active: true }
    }

    pub fn release(&mut self) {
        if self.active {
            self.gate.end();
            self.active = false;
        }
    }
}

impl Drop for InteractionBlockGuard<'_> {
    fn drop(&mut self) {
        if self.active {
            self.gate.end();
            self.active = false;
        }
    }
}

/// Parameter set for scatter-style transitions on a specific node.
#[derive(Clone, Copy, Debug)]
pub struct ScatterSpec {
    pub node: NodeId,
    pub offset: [f32; 2],
    pub duration_ms: u32,
    pub fade_out: bool,
}

impl ScatterSpec {
    pub fn new(node: NodeId, offset: [f32; 2]) -> Self {
        Self { node, offset, duration_ms: 180, fade_out: true }
    }

    pub fn duration(mut self, ms: u32) -> Self {
        self.duration_ms = ms;
        self
    }

    pub fn fade_out(mut self, enable: bool) -> Self {
        self.fade_out = enable;
        self
    }
}

#[derive(Default)]
struct ScatterState {
    pending: BTreeSet<NodeId>,
}

/// High-level scene graph container that wraps [`NodeTree`] with animation and layout helpers.
pub struct UiSurface {
    tree: NodeTree,
    layout_worker: AsyncLayoutCoordinator<Vec<(NodeId, LayoutRect)>>,
    pending_layout: Option<u64>,
    last_layout_size: Option<(u32, u32)>,
    chrome: ChromeMetrics,
    animator: anim::Animator,
    overrides: BTreeMap<NodeId, anim::AnimOverrides>,
    gate: InteractionGate,
    scatter: ScatterState,
    dirty: DirtySet,
    retained_draws: Option<gfx::DrawList>,
    retained_node_stats: RetainedNodeStats,
    last_layout_stats: LayoutStats,
}

impl UiSurface {
    pub fn new(root_style: NodeStyle) -> Self {
        Self {
            tree: NodeTree::new_root(root_style),
            layout_worker: AsyncLayoutCoordinator::new(),
            pending_layout: None,
            last_layout_size: None,
            chrome: ChromeMetrics::default(),
            animator: anim::Animator::new(),
            overrides: BTreeMap::new(),
            gate: InteractionGate::default(),
            scatter: ScatterState::default(),
            dirty: DirtySet::all(),
            retained_draws: None,
            retained_node_stats: RetainedNodeStats::default(),
            last_layout_stats: LayoutStats::default(),
        }
    }

    #[inline]
    pub fn root(&self) -> NodeId {
        self.tree.root()
    }

    #[inline]
    pub fn tree(&self) -> &NodeTree {
        &self.tree
    }

    #[inline]
    pub fn tree_mut(&mut self) -> &mut NodeTree {
        self.mark_tree_mutated();
        &mut self.tree
    }

    pub fn add_node(&mut self, parent: NodeId, style: NodeStyle) -> Option<NodeId> {
        self.tree.style(parent)?;
        let id = self.tree.add_node(parent, style);
        self.mark_scoped_tree_mutated();
        Some(id)
    }

    pub fn remove_node(&mut self, id: NodeId) -> bool {
        if id == self.tree.root() || self.tree.style(id).is_none() {
            return false;
        }
        if !self.tree.remove_node(id) {
            return false;
        }
        self.mark_scoped_tree_mutated();
        true
    }

    #[inline]
    pub fn dirty(&self) -> DirtySet {
        self.dirty
    }

    #[inline]
    pub fn mark_dirty(&mut self, class: DirtyClass) {
        self.dirty.mark(class);
        if class == DirtyClass::Layout {
            self.tree.mark_layout_dirty(self.tree.root());
        }
        if class.bit() & DRAW_DIRTY_BITS != 0 {
            self.retained_draws = None;
            self.retained_node_stats = RetainedNodeStats::default();
            self.tree.mark_all_draw_dirty();
        }
    }

    pub fn mark_node_dirty(&mut self, id: NodeId, class: DirtyClass) -> bool {
        if self.tree.style(id).is_none() {
            return false;
        }

        self.dirty.mark(class);
        match class {
            DirtyClass::Layout => {
                self.tree.mark_layout_dirty(id);
                self.dirty.mark(DirtyClass::Accessibility);
                self.dirty.mark(DirtyClass::HitTest);
            }
            DirtyClass::Transform => {
                self.tree.mark_subtree_draw_dirty(id);
                self.dirty.mark(DirtyClass::Paint);
                self.dirty.mark(DirtyClass::Accessibility);
                self.dirty.mark(DirtyClass::HitTest);
            }
            DirtyClass::Clip => {
                self.tree.mark_node_and_ancestors_draw_dirty(id);
                self.dirty.mark(DirtyClass::HitTest);
            }
            DirtyClass::Text => {
                self.tree.mark_node_and_ancestors_draw_dirty(id);
                self.dirty.mark(DirtyClass::Accessibility);
            }
            DirtyClass::Style
            | DirtyClass::Paint
            | DirtyClass::Opacity
            | DirtyClass::ImageContent
            | DirtyClass::CameraFrame => {
                self.tree.mark_node_and_ancestors_draw_dirty(id);
            }
            DirtyClass::Accessibility | DirtyClass::HitTest => {}
        }
        if class.bit() & DRAW_DIRTY_BITS != 0 {
            self.retained_draws = None;
            self.retained_node_stats = RetainedNodeStats::default();
        }
        true
    }

    pub fn edit_style<F: FnOnce(&mut NodeStyle)>(&mut self, id: NodeId, edit: F) -> bool {
        let Some((before, after)) = self.tree.edit_style(id, edit) else {
            return false;
        };
        if before == after {
            return false;
        }

        self.retained_draws = None;
        self.retained_node_stats = RetainedNodeStats::default();
        self.dirty.mark(DirtyClass::Style);
        if style_change_affects_parent_layout(&before, &after) {
            self.tree.mark_layout_dirty(id);
            self.dirty.mark(DirtyClass::Layout);
            self.dirty.mark(DirtyClass::Accessibility);
            self.dirty.mark(DirtyClass::HitTest);
        } else if style_change_affects_content_layout(&before, &after) {
            self.tree.mark_node_layout_dirty(id);
            self.dirty.mark(DirtyClass::Layout);
            self.dirty.mark(DirtyClass::Accessibility);
            self.dirty.mark(DirtyClass::HitTest);
        }
        if transform_changed(&before, &after) {
            self.dirty.mark(DirtyClass::Transform);
            self.dirty.mark(DirtyClass::Accessibility);
            self.dirty.mark(DirtyClass::HitTest);
            self.tree.mark_subtree_draw_dirty(id);
        }
        if before.opacity != after.opacity {
            self.dirty.mark(DirtyClass::Opacity);
        }
        if before.clip != after.clip {
            self.dirty.mark(DirtyClass::Clip);
            self.dirty.mark(DirtyClass::HitTest);
        }
        if style_change_affects_paint(&before, &after) {
            self.dirty.mark(DirtyClass::Paint);
        }
        true
    }

    #[inline]
    pub fn retained_node_stats(&self) -> RetainedNodeStats {
        self.retained_node_stats
    }

    #[inline]
    pub fn last_layout_stats(&self) -> LayoutStats {
        self.last_layout_stats
    }

    fn mark_tree_mutated(&mut self) {
        self.dirty.mark(DirtyClass::Style);
        self.dirty.mark(DirtyClass::Layout);
        self.dirty.mark(DirtyClass::Paint);
        self.dirty.mark(DirtyClass::Accessibility);
        self.dirty.mark(DirtyClass::HitTest);
        self.tree.mark_layout_dirty(self.tree.root());
    }

    fn mark_scoped_tree_mutated(&mut self) {
        self.retained_draws = None;
        self.retained_node_stats = RetainedNodeStats::default();
        self.dirty.mark(DirtyClass::Style);
        self.dirty.mark(DirtyClass::Layout);
        self.dirty.mark(DirtyClass::Paint);
        self.dirty.mark(DirtyClass::Accessibility);
        self.dirty.mark(DirtyClass::HitTest);
    }

    #[inline]
    pub fn hit_test(&self, x: f32, y: f32) -> Option<(NodeId, [f32; 2])> {
        self.tree.hit_test(x, y)
    }

    #[inline]
    pub fn chrome_metrics(&self) -> ChromeMetrics {
        self.chrome
    }

    pub fn set_chrome_metrics(&mut self, metrics: ChromeMetrics) {
        if self.chrome == metrics {
            return;
        }
        self.chrome = metrics;
        self.dirty.mark(DirtyClass::Layout);
        self.dirty.mark(DirtyClass::Paint);
        self.dirty.mark(DirtyClass::Accessibility);
        self.dirty.mark(DirtyClass::HitTest);
    }

    /// Apply the chrome insets to the root node padding.
    pub fn apply_chrome_padding_to_root(&mut self) {
        let Some(current) = self.tree.style(self.tree.root()) else {
            return;
        };
        if current.padding.left == self.chrome.safe_insets.left
            && current.padding.top == self.chrome.safe_insets.top
            && current.padding.right == self.chrome.safe_insets.right
            && current.padding.bottom == self.chrome.safe_insets.bottom
        {
            return;
        }
        if let Some(style) = self.tree.root_style_mut() {
            style.padding.left = self.chrome.safe_insets.left;
            style.padding.top = self.chrome.safe_insets.top;
            style.padding.right = self.chrome.safe_insets.right;
            style.padding.bottom = self.chrome.safe_insets.bottom;
            self.dirty.mark(DirtyClass::Layout);
            self.dirty.mark(DirtyClass::Paint);
            self.dirty.mark(DirtyClass::Accessibility);
            self.dirty.mark(DirtyClass::HitTest);
        }
    }

    pub fn layout(&mut self, width: f32, height: f32) -> LayoutStats {
        let size = (width.to_bits(), height.to_bits());
        if self.last_layout_size == Some(size) && !self.dirty.contains(DirtyClass::Layout) {
            self.last_layout_stats = LayoutStats::default();
            return self.last_layout_stats;
        }
        let stats = self.tree.layout(width, height);
        self.last_layout_size = Some(size);
        self.dirty.clear(DirtyClass::Layout);
        self.dirty.mark(DirtyClass::Paint);
        self.dirty.mark(DirtyClass::HitTest);
        self.last_layout_stats = stats;
        stats
    }

    pub fn request_async_layout<F>(&mut self, job: F) -> u64
    where
        F: FnOnce() -> Vec<(NodeId, LayoutRect)> + Send + 'static,
    {
        let seq = self.layout_worker.request(job);
        self.pending_layout = Some(seq);
        seq
    }

    pub fn poll_async_layout(&mut self) -> bool {
        if let Some((seq, updates)) = self.layout_worker.poll_latest() {
            self.tree.apply_layouts(&updates);
            if self.pending_layout == Some(seq) {
                self.pending_layout = None;
            }
            self.last_layout_stats = LayoutStats::default();
            self.dirty.clear(DirtyClass::Layout);
            self.dirty.mark(DirtyClass::Paint);
            self.dirty.mark(DirtyClass::Accessibility);
            self.dirty.mark(DirtyClass::HitTest);
            true
        } else {
            false
        }
    }

    #[inline]
    pub fn has_inflight_layout(&self) -> bool {
        self.layout_worker.has_inflight()
    }

    #[inline]
    pub fn encode(&self, b: &mut DrawListBuilder) {
        if self.overrides.is_empty() {
            self.tree.encode_draws(b);
        } else {
            self.tree.encode_draws_with_anims(b, &self.overrides);
        }
    }

    pub fn encode_retained(&mut self, b: &mut DrawListBuilder) -> RetainedDrawStatus {
        self.encode_retained_impl(b, None)
    }

    pub fn encode_retained_with_text_atlas_revisions(
        &mut self,
        b: &mut DrawListBuilder,
        atlases: &[(gfx::ImageHandle, u64)],
    ) -> RetainedDrawStatus {
        self.encode_retained_impl(b, Some(atlases))
    }

    pub fn encode_retained_with_text_ctx(
        &mut self,
        b: &mut DrawListBuilder,
        text: &TextCtx,
    ) -> RetainedDrawStatus {
        if let Some(atlas) = text.retained_text_atlas_revision() {
            self.encode_retained_impl(b, Some(core::slice::from_ref(&atlas)))
        } else {
            self.encode_retained_impl(b, None)
        }
    }

    fn encode_retained_impl(
        &mut self,
        b: &mut DrawListBuilder,
        text_atlases: Option<&[(gfx::ImageHandle, u64)]>,
    ) -> RetainedDrawStatus {
        if !self.dirty.affects_draw() {
            if let Some(draws) = self.retained_draws.as_ref() {
                if append_retained_drawlist(b, draws, text_atlases) {
                    self.retained_node_stats =
                        RetainedNodeStats { reused_nodes: 1, rebuilt_nodes: 0 };
                    return RetainedDrawStatus::Reused;
                }
            }
        }

        let mut next = DrawListBuilder::new();
        let node_stats = if self.overrides.is_empty() {
            let retained = match text_atlases {
                Some(atlases) => {
                    self.tree.encode_draws_retained_with_text_atlas_revisions(&mut next, atlases)
                }
                None => self.tree.encode_draws_retained(&mut next),
            };
            if let Some(stats) = retained {
                stats
            } else {
                next.clear();
                self.encode(&mut next);
                RetainedNodeStats::default()
            }
        } else {
            self.encode(&mut next);
            RetainedNodeStats::default()
        };
        let draws = next.into_inner();
        self.retained_draws = None;
        let cache_safe = drawlist_retained_replay_safe_for(&draws, text_atlases);
        if b.append_drawlist(&draws) {
            if cache_safe {
                self.retained_draws = Some(draws);
            }
            self.dirty.clear_draw_affecting();
            self.retained_node_stats = node_stats;
        }
        RetainedDrawStatus::Rebuilt
    }

    pub fn capture(&self, viewport: gfx::RectF, device_scale: f32) -> SurfaceCapture {
        let mut builder = DrawListBuilder::new();
        let clip = gfx::RectI::new(
            viewport.x.floor() as i32,
            viewport.y.floor() as i32,
            viewport.w.ceil() as i32,
            viewport.h.ceil() as i32,
        );
        builder.clip_push(clip);
        self.encode(&mut builder);
        builder.clip_pop();
        SurfaceCapture::new(viewport, device_scale, builder.into_inner())
    }

    #[inline]
    pub fn animator(&mut self) -> &mut anim::Animator {
        &mut self.animator
    }

    #[inline]
    pub fn overrides(&self) -> &BTreeMap<NodeId, anim::AnimOverrides> {
        &self.overrides
    }

    #[inline]
    pub fn overrides_mut(&mut self) -> &mut BTreeMap<NodeId, anim::AnimOverrides> {
        self.dirty.mark(DirtyClass::Transform);
        self.dirty.mark(DirtyClass::Opacity);
        self.dirty.mark(DirtyClass::Paint);
        &mut self.overrides
    }

    pub fn tick(&mut self) -> bool {
        let now = timing::now_ms();
        self.tick_at(now)
    }

    pub fn tick_at(&mut self, now_ms: u64) -> bool {
        let new_overrides = self.animator.step(now_ms);
        let mut changed = false;
        if self.animator.active_count() == 0 && new_overrides.is_empty() {
            if !self.overrides.is_empty() {
                self.overrides.clear();
                changed = true;
            }
        } else if new_overrides != self.overrides {
            self.overrides = new_overrides;
            changed = true;
        }
        if changed {
            self.dirty.mark(DirtyClass::Transform);
            self.dirty.mark(DirtyClass::Opacity);
            self.dirty.mark(DirtyClass::Paint);
        }
        self.update_scatter_state();
        changed
    }

    pub fn run_scatter(&mut self, specs: &[ScatterSpec]) {
        if specs.is_empty() {
            return;
        }
        let mut engaged = false;
        for spec in specs {
            let Some(style) = self.tree.style(spec.node) else { continue };
            let seq = anim::helpers::scatter(
                style.transform,
                spec.offset,
                spec.duration_ms,
                spec.fade_out,
            );
            if seq.is_empty() {
                continue;
            }
            self.animator.start_sequence(spec.node, &seq);
            self.scatter.pending.insert(spec.node);
            engaged = true;
        }
        if engaged {
            self.dirty.mark(DirtyClass::Transform);
            self.dirty.mark(DirtyClass::Opacity);
            self.dirty.mark(DirtyClass::Paint);
            self.gate.begin();
        }
    }

    #[inline]
    pub fn is_interaction_blocked(&self) -> bool {
        self.gate.is_blocked()
    }

    #[inline]
    pub fn block_interactions(&mut self) -> InteractionBlockGuard<'_> {
        InteractionBlockGuard::new(&mut self.gate)
    }

    pub fn route_pointer<F: FnMut(NodeId, [f32; 2])>(&self, x: f32, y: f32, handler: F) {
        if self.is_interaction_blocked() {
            return;
        }
        self.tree.route_pointer(x, y, handler);
    }

    fn update_scatter_state(&mut self) {
        if self.scatter.pending.is_empty() {
            return;
        }
        let mut finished: Vec<NodeId> = Vec::new();
        for node in self.scatter.pending.iter().copied() {
            if !self.animator.is_active(node) {
                finished.push(node);
            }
        }
        if finished.is_empty() {
            return;
        }
        for node in finished {
            self.scatter.pending.remove(&node);
        }
        if self.scatter.pending.is_empty() {
            self.gate.end();
        }
    }
}

/// Coordinator for switching between multiple [`UiSurface`] instances with animated transitions.
pub struct SurfaceRouter {
    surfaces: Vec<UiSurface>,
    current: usize,
    overlays: OverlayStack,
    popups: PopupManager,
    viewport: gfx::RectF,
    device_scale: f32,
    retained_composition_stats: RetainedCompositionStats,
}

impl SurfaceRouter {
    pub fn new(surface: UiSurface) -> Self {
        Self {
            surfaces: vec![surface],
            current: 0,
            overlays: OverlayStack::new(),
            popups: PopupManager::new(),
            viewport: gfx::RectF::new(0.0, 0.0, 0.0, 0.0),
            device_scale: 1.0,
            retained_composition_stats: RetainedCompositionStats::default(),
        }
    }

    pub fn push(&mut self, surface: UiSurface) -> usize {
        self.surfaces.push(surface);
        self.surfaces.len() - 1
    }

    #[inline]
    pub fn current_index(&self) -> usize {
        self.current
    }

    pub fn surface(&self, index: usize) -> Option<&UiSurface> {
        self.surfaces.get(index)
    }

    pub fn surface_mut(&mut self, index: usize) -> Option<&mut UiSurface> {
        self.surfaces.get_mut(index)
    }

    pub fn current(&self) -> &UiSurface {
        &self.surfaces[self.current]
    }

    pub fn current_mut(&mut self) -> &mut UiSurface {
        &mut self.surfaces[self.current]
    }

    pub fn set_current(&mut self, index: usize) {
        if index < self.surfaces.len() {
            self.current = index;
        }
    }

    pub fn overlays(&self) -> &OverlayStack {
        &self.overlays
    }

    pub fn overlays_mut(&mut self) -> &mut OverlayStack {
        &mut self.overlays
    }

    pub fn popups(&self) -> &PopupManager {
        &self.popups
    }

    pub fn popups_mut(&mut self) -> &mut PopupManager {
        &mut self.popups
    }

    pub fn retained_composition_stats(&self) -> RetainedCompositionStats {
        self.retained_composition_stats
    }

    pub fn set_viewport(&mut self, viewport: gfx::RectF, device_scale: f32) {
        self.viewport = viewport;
        self.device_scale = device_scale.max(0.1);
        self.overlays.set_viewport(viewport, self.device_scale);
        self.popups.set_viewport(viewport, self.device_scale);
    }

    pub fn encode_with_overlays(
        &mut self,
        viewport: gfx::RectF,
        device_scale: f32,
        builder: &mut DrawListBuilder,
    ) {
        self.encode_with_overlays_impl(viewport, device_scale, builder, None);
    }

    pub fn encode_with_overlays_with_text_atlas_revisions(
        &mut self,
        viewport: gfx::RectF,
        device_scale: f32,
        builder: &mut DrawListBuilder,
        atlases: &[(gfx::ImageHandle, u64)],
    ) {
        self.encode_with_overlays_impl(viewport, device_scale, builder, Some(atlases));
    }

    pub fn encode_with_overlays_with_text_ctx(
        &mut self,
        viewport: gfx::RectF,
        device_scale: f32,
        builder: &mut DrawListBuilder,
        text: &TextCtx,
    ) {
        if let Some(atlas) = text.retained_text_atlas_revision() {
            self.encode_with_overlays_impl(
                viewport,
                device_scale,
                builder,
                Some(core::slice::from_ref(&atlas)),
            );
        } else {
            self.encode_with_overlays_impl(viewport, device_scale, builder, None);
        }
    }

    fn encode_with_overlays_impl(
        &mut self,
        viewport: gfx::RectF,
        device_scale: f32,
        builder: &mut DrawListBuilder,
        text_atlases: Option<&[(gfx::ImageHandle, u64)]>,
    ) {
        self.set_viewport(viewport, device_scale);
        let mut retained_stats = RetainedCompositionStats::default();
        if let Some(surface) = self.surfaces.get_mut(self.current) {
            retained_stats.record_current(surface.encode_retained_impl(builder, text_atlases));
        }
        retained_stats.record_overlays(self.overlays.encode_retained(builder, text_atlases));
        retained_stats.record_popups(self.popups.encode_retained(builder, text_atlases));
        self.retained_composition_stats = retained_stats;
    }

    pub fn capture(&mut self, viewport: gfx::RectF, device_scale: f32) -> SurfaceCapture {
        self.set_viewport(viewport, device_scale);
        let mut builder = DrawListBuilder::new();
        let clip = gfx::RectI::new(
            viewport.x.floor() as i32,
            viewport.y.floor() as i32,
            viewport.w.ceil() as i32,
            viewport.h.ceil() as i32,
        );
        builder.clip_push(clip);
        if let Some(surface) = self.surfaces.get(self.current) {
            surface.encode(&mut builder);
        }
        self.overlays.encode(&mut builder);
        self.popups.encode(&mut builder);
        builder.clip_pop();
        SurfaceCapture::new(viewport, device_scale, builder.into_inner())
    }

    pub fn transition_to(
        &mut self,
        index: usize,
        outgoing: &[ScatterSpec],
        incoming: &[ScatterSpec],
    ) {
        if index >= self.surfaces.len() || index == self.current {
            return;
        }
        {
            let current = &mut self.surfaces[self.current];
            current.run_scatter(outgoing);
        }
        {
            let next = &mut self.surfaces[index];
            next.run_scatter(incoming);
        }
        self.current = index;
    }

    pub fn tick_all(&mut self) -> bool {
        let now = timing::now_ms();
        self.tick_all_at(now)
    }

    pub fn tick_all_at(&mut self, now_ms: u64) -> bool {
        let mut changed = false;
        for surface in &mut self.surfaces {
            if surface.tick_at(now_ms) {
                changed = true;
            }
        }
        if self.overlays.tick_at(now_ms) {
            changed = true;
        }
        if self.popups.tick_at(now_ms) {
            changed = true;
        }
        changed
    }

    pub fn pointer_event<F: FnMut(NodeId, [f32; 2])>(
        &mut self,
        x: f32,
        y: f32,
        buttons: u32,
        mut handler: F,
    ) {
        match self.popups.pointer_event(x, y, buttons) {
            OverlayPointerResult::Consumed { node, .. } => {
                if let Some((id, pt)) = node {
                    handler(id, pt);
                }
                return;
            }
            OverlayPointerResult::Dismissed { .. } => return,
            OverlayPointerResult::Ignored => {}
        }
        match self.overlays.pointer_event(x, y, buttons) {
            OverlayPointerResult::Consumed { node, .. } => {
                if let Some((id, pt)) = node {
                    handler(id, pt);
                }
                return;
            }
            OverlayPointerResult::Dismissed { .. } => return,
            OverlayPointerResult::Ignored => {}
        }
        if let Some(surface) = self.surfaces.get(self.current) {
            surface.route_pointer(x, y, handler);
        }
    }

    pub fn route_pointer<F: FnMut(NodeId, [f32; 2])>(&mut self, x: f32, y: f32, handler: F) {
        self.pointer_event(x, y, 1, handler);
    }
}
