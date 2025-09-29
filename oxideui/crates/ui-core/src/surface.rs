//! Scene graph coordination utilities for OxideUI surfaces.
//! Provides NodeTree management, asynchronous layout, interaction gating, and
//! scatter-style transition helpers inspired by AsyncDisplayKit flows.

use crate::{
    anim,
    capture::SurfaceCapture,
    layout_async::AsyncLayoutCoordinator,
    overlay::{OverlayPointerResult, OverlayStack, PopupManager},
    DrawListBuilder, LayoutRect, NodeId, NodeStyle, NodeTree,
};
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;
use oxideui_renderer_api as gfx;
use oxideui_timing as timing;

/// Safe-area and chrome metadata supplied by the host platform.
#[derive(Clone, Copy, Debug)]
pub struct ChromeMetrics {
    pub safe_insets: gfx::Insets,
    pub status_bar_height: f32,
}

impl Default for ChromeMetrics {
    fn default() -> Self {
        Self { safe_insets: gfx::Insets::new(0.0, 0.0, 0.0, 0.0), status_bar_height: 0.0 }
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
    chrome: ChromeMetrics,
    animator: anim::Animator,
    overrides: BTreeMap<NodeId, anim::AnimOverrides>,
    gate: InteractionGate,
    scatter: ScatterState,
}

impl UiSurface {
    pub fn new(root_style: NodeStyle) -> Self {
        Self {
            tree: NodeTree::new_root(root_style),
            layout_worker: AsyncLayoutCoordinator::new(),
            pending_layout: None,
            chrome: ChromeMetrics::default(),
            animator: anim::Animator::new(),
            overrides: BTreeMap::new(),
            gate: InteractionGate::default(),
            scatter: ScatterState::default(),
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
        &mut self.tree
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
        self.chrome = metrics;
    }

    /// Apply the chrome insets to the root node padding.
    pub fn apply_chrome_padding_to_root(&mut self) {
        if let Some(style) = self.tree.root_style_mut() {
            style.padding.left = self.chrome.safe_insets.left;
            style.padding.top = self.chrome.safe_insets.top;
            style.padding.right = self.chrome.safe_insets.right;
            style.padding.bottom = self.chrome.safe_insets.bottom;
        }
    }

    pub fn layout(&mut self, width: f32, height: f32) {
        self.tree.layout(width, height);
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
        self.set_viewport(viewport, device_scale);
        if let Some(surface) = self.surfaces.get(self.current) {
            surface.encode(builder);
        }
        self.overlays.encode(builder);
        self.popups.encode(builder);
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
