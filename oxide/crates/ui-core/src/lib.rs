//! Oxide UI Core (minimal CPU utilities)
#![forbid(unsafe_code)]
#![allow(clippy::all, clippy::pedantic)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

pub mod prelude {
    pub use oxide_platform_api as platform_api;
    pub use oxide_renderer_api as renderer_api;
    pub use oxide_utils as utils;
}

pub mod bitmap_text;
pub mod camera;
pub mod capture;
pub mod draw_replay;
pub mod emitter;
pub mod keyboard;
pub mod layout_async;
pub mod overlay;
pub mod permissions;
pub mod picker_popup;
pub mod scroll_state;
pub mod sensors;
pub mod surface;
pub mod telemetry;
pub mod visual_tree;

pub use camera::{
    recording_event_to_ui, CameraController, CameraEvent, CameraMetrics, CameraMode,
    CameraPreviewNode, CameraRecordingUiEvent, CameraSession, CropperState, VolumeHudState,
};
pub use capture::SurfaceCapture;
pub use emitter::{
    BurstEmitter, BurstEmitterCellConfig, BurstEmitterConfig, BurstEmitterParticle,
    BurstEmitterShape,
};
pub use keyboard::{KeyboardEventExt, KeyboardTracker};
pub use layout_async::AsyncLayoutCoordinator;
pub use overlay::{
    OverlayBehavior, OverlayHandle, OverlayPointerResult, OverlayStack, OverlayVisual,
    PopupCallbacks, PopupHandle, PopupManager, PopupSpec, PopupTouchRegion,
};
pub use permissions::{PermissionOverlayUi, PermissionPrompt};
pub use picker_popup::{
    PanelPopupState, PickerColumnCommit, PickerColumnState, PopupPickerState, PopupTapRegion,
};
pub use scroll_state::ScrollState;
pub use sensors::{
    BluetoothSnapshot, LocationSnapshot, MotionSnapshot, PushSnapshot, SensorBridgeConfig,
    SensorPermissionBinding, SensorSnapshot, SensorView,
};
pub use surface::{ChromeMetrics, InteractionBlockGuard, ScatterSpec, SurfaceRouter, UiSurface};
pub use telemetry::TelemetryView;
pub use visual_tree::{
    build_visual_tree_action_graph, build_visual_tree_action_graph_manifest,
    compare_visual_tree_action_graphs, compare_visual_tree_sequences,
    compare_visual_tree_snapshots, default_visual_tree_action_animation_trace,
    visual_tree_action_observation_for_path, visual_tree_node_by_path,
    VisualTreeActionAnimationTracePlan, VisualTreeActionDescriptor, VisualTreeActionGraph,
    VisualTreeActionGraphDiff, VisualTreeActionGraphManifest, VisualTreeActionNode,
    VisualTreeActionObservation, VisualTreeActionReplayPlanStep, VisualTreeDiff, VisualTreeInsets,
    VisualTreeMismatch, VisualTreeNode, VisualTreeRect, VisualTreeSequence, VisualTreeSequenceDiff,
    VisualTreeSequenceStep, VisualTreeSequenceStepDiff, VisualTreeSnapshot, VisualTreeViewport,
    VISUAL_TREE_ACTION_GRAPH_MANIFEST_SCHEMA_VERSION, VISUAL_TREE_ACTION_GRAPH_SCHEMA_VERSION,
    VISUAL_TREE_SCHEMA_VERSION, VISUAL_TREE_SEQUENCE_SCHEMA_VERSION,
};

use oxide_renderer_api as gfx;

/// Builder for renderer-agnostic draw lists with a managed clip stack.
pub struct DrawListBuilder {
    list: gfx::DrawList,
    clip_stack: alloc::vec::Vec<gfx::RectI>,
}

impl Default for DrawListBuilder {
    fn default() -> Self {
        Self { list: gfx::DrawList::default(), clip_stack: alloc::vec::Vec::new() }
    }
}

impl DrawListBuilder {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn into_inner(self) -> gfx::DrawList {
        self.list
    }

    #[inline]
    pub fn drawlist(&self) -> &gfx::DrawList {
        &self.list
    }

    #[inline]
    pub fn drawlist_mut(&mut self) -> &mut gfx::DrawList {
        &mut self.list
    }

    #[inline]
    pub fn clear(&mut self) {
        self.list.items.clear();
        self.list.vertices.clear();
        self.list.indices.clear();
        self.clip_stack.clear();
    }

    #[inline]
    pub fn layer_begin(&mut self, id: u32, rect: gfx::RectF, dirty: bool) {
        self.list.items.push(gfx::DrawCmd::LayerBegin { id, rect, dirty });
    }

    #[inline]
    pub fn layer_end(&mut self) {
        self.list.items.push(gfx::DrawCmd::LayerEnd);
    }

    #[inline]
    pub fn clip_push(&mut self, rect: gfx::RectI) {
        self.clip_stack.push(rect);
        self.list.items.push(gfx::DrawCmd::ClipPush { rect });
    }

    #[inline]
    pub fn clip_pop(&mut self) {
        let _ = self.clip_stack.pop();
        self.list.items.push(gfx::DrawCmd::ClipPop);
    }

    pub fn solid(&mut self, vb: gfx::VertexSpan, ib: gfx::IndexSpan, color: gfx::Color) {
        self.list.items.push(gfx::DrawCmd::Solid { vb, ib, color });
    }

    pub fn image(&mut self, tex: gfx::ImageHandle, dst: gfx::RectF, src: gfx::RectF, alpha: f32) {
        self.list.items.push(gfx::DrawCmd::Image { tex, dst, src, alpha });
    }

    pub fn image_mesh(
        &mut self,
        tex: gfx::ImageHandle,
        vertices: &[gfx::Vertex],
        indices: &[u16],
        alpha: f32,
    ) {
        if vertices.is_empty() {
            return;
        }
        let quad_indices = [0_u16, 1, 2, 2, 1, 3];
        let indices = if indices.is_empty() && vertices.len() == 4 {
            &quad_indices[..]
        } else {
            indices
        };
        let Ok(vb_len) = u32::try_from(vertices.len()) else {
            return;
        };
        let Ok(ib_len) = u32::try_from(indices.len()) else {
            return;
        };
        let Ok(vb_offset) = u32::try_from(self.list.vertices.len()) else {
            return;
        };
        let Ok(ib_offset) = u32::try_from(self.list.indices.len()) else {
            return;
        };
        self.list.vertices.extend_from_slice(vertices);
        self.list.indices.extend_from_slice(indices);
        self.list.items.push(gfx::DrawCmd::ImageMesh {
            tex,
            vb: gfx::VertexSpan { offset: vb_offset, len: vb_len },
            ib: gfx::IndexSpan { offset: ib_offset, len: ib_len },
            alpha,
        });
    }

    pub fn glyph_run(&mut self, run: gfx::GlyphRun) {
        self.list.items.push(gfx::DrawCmd::GlyphRun { run });
    }

    pub fn rrect(&mut self, rect: gfx::RectF, radii: [f32; 4], color: gfx::Color) {
        self.list.items.push(gfx::DrawCmd::RRect { rect, radii, color });
    }

    pub fn nine_slice(
        &mut self,
        tex: gfx::ImageHandle,
        rect: gfx::RectF,
        slice: gfx::Insets,
        alpha: f32,
    ) {
        self.list.items.push(gfx::DrawCmd::NineSlice { tex, rect, slice, alpha });
    }

    pub fn backdrop(&mut self, rect: gfx::RectF, sigma: f32, tint: gfx::Color, alpha: f32) {
        self.list.items.push(gfx::DrawCmd::Backdrop { rect, sigma, tint, alpha });
    }

    pub fn visual_effect(&mut self, rect: gfx::RectF, effect: gfx::VisualEffect) {
        self.list.items.push(gfx::DrawCmd::VisualEffect { rect, effect });
    }

    pub fn camera_bg(
        &mut self,
        rect: gfx::RectF,
        tint: gfx::Color,
        alpha: f32,
        grayscale: bool,
        blur: bool,
        sigma: f32,
    ) {
        self.list.items.push(gfx::DrawCmd::CameraBg { rect, tint, alpha, grayscale, blur, sigma });
    }

    pub fn native_camera_preview(&mut self, rect: gfx::RectF) {
        self.list.items.push(gfx::DrawCmd::NativeCameraPreview { rect });
    }

    pub fn topomap_globe(&mut self, rect: gfx::RectF) {
        self.list.items.push(gfx::DrawCmd::TopomapGlobe { rect });
    }

    pub fn spinner(&mut self, center: [f32; 2], atom: f32, alpha: f32) {
        self.list.items.push(gfx::DrawCmd::Spinner { center, atom, alpha });
    }
}

/// Prepared draw with a resolved clip rectangle.
#[derive(Debug, Clone)]
pub struct PreparedDraw {
    pub cmd: gfx::DrawCmd,
    pub clip: Option<gfx::RectI>,
}

#[inline]
fn intersect(a: gfx::RectI, b: gfx::RectI) -> Option<gfx::RectI> {
    let x1 = a.x.max(b.x);
    let y1 = a.y.max(b.y);
    let x2 = (a.x + a.w).min(b.x + b.w);
    let y2 = (a.y + a.h).min(b.y + b.h);
    let w = x2 - x1;
    let h = y2 - y1;
    if w > 0 && h > 0 {
        Some(gfx::RectI { x: x1, y: y1, w, h })
    } else {
        None
    }
}

/// Lower ClipPush/ClipPop into a resolved scissor for each draw item; clip commands are removed.
#[must_use]
pub fn prepare_draws(list: &gfx::DrawList) -> alloc::vec::Vec<PreparedDraw> {
    use gfx::DrawCmd as C;
    let mut out = alloc::vec::Vec::with_capacity(list.items.len());
    let mut stack: alloc::vec::Vec<gfx::RectI> = alloc::vec::Vec::with_capacity(8);
    for item in &list.items {
        match *item {
            C::ClipPush { rect } => {
                let next = if let Some(cur) = stack.last().copied() {
                    intersect(cur, rect).unwrap_or(gfx::RectI { x: 0, y: 0, w: 0, h: 0 })
                } else {
                    rect
                };
                stack.push(next);
            }
            C::ClipPop => {
                let _ = stack.pop();
            }
            _ => {
                out.push(PreparedDraw {
                    cmd: item.clone(),
                    clip: stack.last().copied().filter(|r| r.w > 0 && r.h > 0),
                });
            }
        }
    }
    out
}

/// Batch key for stable sorting: pipeline kind, texture id (if any), clip hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct BatchKey(u8, u32, u64);

fn key_for(pd: &PreparedDraw) -> BatchKey {
    use gfx::DrawCmd as C;
    let clip_hash: u64 = if let Some(c) = pd.clip {
        // Simple mixing of fields; sufficient for a stable grouping key
        let mut h = 0_u64;
        h ^= (c.x as i64 as u64).wrapping_mul(0x9E3779B185EBCA87);
        h ^= (c.y as i64 as u64).rotate_left(13);
        h ^= (c.w as i64 as u64).rotate_left(27);
        h ^ (c.h as i64 as u64).rotate_left(41)
    } else {
        0
    };

    match &pd.cmd {
        C::LayerBegin { .. } | C::LayerEnd => BatchKey(7, 0, clip_hash),
        C::Solid { .. } => BatchKey(0, 0, clip_hash),
        C::Image { tex, .. } => BatchKey(1, tex.0, clip_hash),
        C::ImageMesh { tex, .. } => BatchKey(1, tex.0, clip_hash),
        C::GlyphRun { .. } => BatchKey(2, 0, clip_hash),
        C::RRect { .. } => BatchKey(3, 0, clip_hash),
        C::NineSlice { tex, .. } => BatchKey(4, tex.0, clip_hash),
        C::Backdrop { .. } => BatchKey(5, 0, clip_hash),
        C::Spinner { .. } => BatchKey(6, 0, clip_hash),
        C::VisualEffect { .. } => BatchKey(9, 0, clip_hash),
        C::CameraBg { .. } => BatchKey(8, 0, clip_hash),
        C::NativeCameraPreview { .. } => BatchKey(10, 0, clip_hash),
        C::TopomapGlobe { .. } => BatchKey(11, 0, clip_hash),
        C::ClipPush { .. } | C::ClipPop => {
            unreachable!("clip commands are removed by prepare_draws")
        }
    }
}

/// Stable sort for batching: PSO → texture → clip.
#[must_use]
pub fn sort_for_batching(
    mut prepared: alloc::vec::Vec<PreparedDraw>,
) -> alloc::vec::Vec<PreparedDraw> {
    prepared.sort_by_key(|pd| key_for(pd));
    prepared
}

/// Coalesce adjacent draws that are trivially mergeable without changing order semantics.
///
/// Rules:
/// - Solid: same color, adjacent, and both vertex/index spans are contiguous -> extend.
///
/// Glyph runs are intentionally excluded here because modern text paths may encode indices
/// local to each run. Blindly concatenating spans without rebasing indices can corrupt glyph
/// geometry (stretched triangles/garbled text), especially at high cumulative vertex offsets.
///
/// This does not reorder anything and is safe under blending and z-order.
pub fn coalesce_adjacent_draws(list: &mut gfx::DrawList) {
    use gfx::DrawCmd as C;

    #[inline]
    fn contiguous(a_off: u32, a_len: u32, b_off: u32) -> bool {
        a_off.saturating_add(a_len) == b_off
    }

    #[inline]
    fn mergeable_nonindexed_solid(vb: gfx::VertexSpan) -> bool {
        // Non-indexed solids are rendered as a raw primitive stream by the backend.
        // Merging only preserves topology when each span is an explicit triangle list.
        vb.len >= 3 && vb.len % 3 == 0
    }

    #[inline]
    fn can_merge(a: &C, b: &C) -> bool {
        match (a, b) {
            (C::GlyphRun { .. }, C::GlyphRun { .. }) => false,
            (C::Solid { vb: av, ib: ai, color: ac }, C::Solid { vb: bv, ib: bi, color: bc }) => {
                if ac != bc
                    || !contiguous(av.offset, av.len, bv.offset)
                    || !contiguous(ai.offset, ai.len, bi.offset)
                {
                    false
                } else if ai.len == 0 && bi.len == 0 {
                    mergeable_nonindexed_solid(*av) && mergeable_nonindexed_solid(*bv)
                } else {
                    ai.len > 0 && bi.len > 0
                }
            }
            _ => false,
        }
    }

    #[inline]
    fn merge_into(dst: &mut C, src: C) {
        match (dst, src) {
            (C::Solid { vb: av, ib: ai, .. }, C::Solid { vb: bv, ib: bi, .. }) => {
                av.len += bv.len;
                ai.len += bi.len;
            }
            _ => {}
        }
    }

    if list.items.len() < 2 {
        return;
    }

    let items = core::mem::take(&mut list.items);
    let mut out = alloc::vec::Vec::with_capacity(items.len());
    let mut iter = items.into_iter();
    let Some(mut current) = iter.next() else {
        return;
    };

    for next in iter {
        if can_merge(&current, &next) {
            merge_into(&mut current, next);
        } else {
            out.push(current);
            current = next;
        }
    }

    out.push(current);
    list.items = out;
}

extern crate alloc;

pub mod anim;
pub mod collection;
pub mod design_system;
pub mod elements;
pub mod orchestration;
pub mod text_fields;

pub use text_fields::{
    draw_text_input_options_popover, draw_text_selection_highlight,
    single_line_text_selection_highlight_layout, single_line_text_selection_index_for_x,
    single_line_text_selection_rect, text_char_slice, text_input_option_at,
    text_input_options_layout, text_selection_drag_anchor_at, text_word_range_at_char_index,
    EditableText, FieldFailRestoreMode, HorizontalShiftingText, SecureText, TextCaretDragState,
    TextFieldPolicy, TextInputOption, TextInputOptionsConfig, TextInputOptionsLayout,
    TextInputOptionsPopoverState, TextInputOptionsPopoverStyle, TextSelectionDragAnchor,
    TextSelectionDragState, TextSelectionHighlightLayout, TextSelectionHighlightStyle,
    TextSelectionState, TextTapMemory,
};

// ===== UI Node Tree (layout + routing) =====

use crate::prelude::platform_api as plat;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Row,
    Column,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dim {
    Auto,
    Px(f32),
    Percent(f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size2D {
    pub w: Dim,
    pub h: Dim,
}
impl Default for Size2D {
    fn default() -> Self {
        Self { w: Dim::Auto, h: Dim::Auto }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Edges {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}
impl Default for Edges {
    fn default() -> Self {
        Self { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overflow {
    /// Content is clipped to the node's bounds.
    Hidden,
    /// Content can overflow the node's bounds, and if enabled, can be scrolled.
    Scroll,
}

#[derive(Clone, Debug)]
pub struct NodeStyle {
    pub axis: Axis,
    pub size: Size2D,
    pub min_size: Size2D,
    pub max_size: Size2D,
    pub margin: Edges,
    pub padding: Edges,
    pub gap: f32,
    pub flex_grow: f32,
    pub background: gfx::Color,
    pub corner_radii: [f32; 4],
    pub opacity: f32,
    pub transform: plat::Transform2D, // applied as translation only here
    pub shadow_alpha: f32,            // rendered as simple offset quad
    pub clip: bool,
    pub overflow: Overflow,
}

impl Default for NodeStyle {
    fn default() -> Self {
        Self {
            axis: Axis::Column,
            size: Size2D::default(),
            min_size: Size2D::default(),
            max_size: Size2D { w: Dim::Auto, h: Dim::Auto },
            margin: Edges::default(),
            padding: Edges::default(),
            gap: 0.0,
            flex_grow: 0.0,
            background: gfx::Color::rgba(0.95, 0.95, 0.95, 1.0),
            corner_radii: [0.0, 0.0, 0.0, 0.0],
            opacity: 1.0,
            transform: plat::Transform2D { tx: 0.0, ty: 0.0, sx: 1.0, sy: 1.0, rot_rad: 0.0 },
            shadow_alpha: 0.0,
            clip: false,
            overflow: Overflow::Hidden,
        }
    }
}

#[derive(Clone, Debug)]
struct Node {
    id: NodeId,
    style: NodeStyle,
    children: alloc::vec::Vec<NodeId>,
    layout: LayoutRect,
}

impl Node {
    fn new(id: NodeId, style: NodeStyle) -> Self {
        Self {
            id,
            style,
            children: alloc::vec::Vec::new(),
            layout: LayoutRect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 },
        }
    }
}

pub struct NodeTree {
    nodes: alloc::vec::Vec<Option<Node>>, // dense slot map
    root: NodeId,
    free_list: alloc::vec::Vec<NodeId>,
}

const INLINE_LAYOUT_CHILDREN: usize = 16;

impl NodeTree {
    pub fn new_root(style: NodeStyle) -> Self {
        let root = NodeId(1);
        let mut nodes = alloc::vec::Vec::with_capacity(8);
        nodes.push(None); // 0 unused
        nodes.push(Some(Node::new(root, style)));
        Self { nodes, root, free_list: alloc::vec::Vec::new() }
    }

    pub fn root(&self) -> NodeId {
        self.root
    }

    pub fn add_node(&mut self, parent: NodeId, style: NodeStyle) -> NodeId {
        let id = if let Some(reuse) = self.free_list.pop() {
            if let Some(slot) = self.nodes.get_mut(reuse.0 as usize) {
                *slot = Some(Node::new(reuse, style));
            }
            reuse
        } else {
            let id = NodeId(self.nodes.len() as u32);
            self.nodes.push(Some(Node::new(id, style)));
            id
        };
        if let Some(p) = self.get_mut(parent) {
            p.children.push(id);
        }
        id
    }

    #[inline]
    fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.0 as usize).and_then(|o| o.as_ref())
    }
    #[inline]
    fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(id.0 as usize).and_then(|o| o.as_mut())
    }

    // ---- Layout ----

    pub fn layout(&mut self, root_w: f32, root_h: f32) {
        let px = LayoutRect { x: 0.0, y: 0.0, w: root_w.max(0.0), h: root_h.max(0.0) };
        if let Some(root) = self.get_mut(self.root) {
            root.layout = px;
            let rect = content_rect(&root.style, &px);
            self.layout_children(self.root, rect);
        }
    }

    fn layout_children(&mut self, id: NodeId, content: LayoutRect) {
        let (axis, gap, child_count) = if let Some(node) = self.get(id) {
            (node.style.axis, node.style.gap, node.children.len())
        } else {
            return;
        };
        if child_count == 0 {
            return;
        }

        if child_count <= INLINE_LAYOUT_CHILDREN {
            let mut kids = [NodeId(0); INLINE_LAYOUT_CHILDREN];
            if let Some(node) = self.get(id) {
                kids[..child_count].copy_from_slice(&node.children);
            } else {
                return;
            }
            match axis {
                Axis::Row => self.layout_row(id, content, gap, &kids[..child_count]),
                Axis::Column => self.layout_col(id, content, gap, &kids[..child_count]),
            }
        } else {
            let kids: alloc::vec::Vec<NodeId> =
                if let Some(n) = self.get(id) { n.children.clone() } else { return };
            match axis {
                Axis::Row => self.layout_row(id, content, gap, &kids),
                Axis::Column => self.layout_col(id, content, gap, &kids),
            }
        }
    }

    fn layout_row(&mut self, id: NodeId, content: LayoutRect, gap: f32, kids: &[NodeId]) {
        let mut fixed = 0.0;
        let mut flex_sum = 0.0;
        let gap_total = if kids.len() > 1 { gap * ((kids.len() - 1) as f32) } else { 0.0 };
        // Measure pass: collect fixed widths and flex
        for &kid in kids {
            if let Some(n) = self.get(kid) {
                let mw = match n.style.size.w {
                    Dim::Px(px) => px,
                    _ => 0.0,
                } + n.style.margin.left
                    + n.style.margin.right;
                fixed += mw;
                flex_sum += n.style.flex_grow.max(0.0);
            }
        }
        // `content` is already the inner rect excluding this node's padding; do not add padding again.
        let mut x = content.x;
        let y = content.y;
        let leftover = (content.w - fixed - gap_total).max(0.0);
        for (i, &kid) in kids.iter().enumerate() {
            if let Some(n) = self.get(kid) {
                // Snapshot immutable fields needed after mutation
                let style = &n.style;
                let margin_l = style.margin.left;
                let margin_r = style.margin.right;
                let margin_t = style.margin.top;
                let margin_b = style.margin.bottom;
                let flex = style.flex_grow.max(0.0);
                let tx = style.transform.tx;
                let ty = style.transform.ty;
                let pad_l = style.padding.left;
                let pad_t = style.padding.top;
                let pad_r = style.padding.right;
                let pad_b = style.padding.bottom;

                let cw = match style.size.w {
                    Dim::Px(px) if px > 0.0 => px,
                    _ => {
                        if flex_sum > 0.0 {
                            leftover * (flex / flex_sum)
                        } else {
                            0.0
                        }
                    }
                };
                let ch = match style.size.h {
                    Dim::Px(px) if px > 0.0 => px,
                    _ => content.h - margin_t - margin_b,
                };
                let w = cw.max(0.0);
                let h = ch.max(0.0);
                let nx = x + margin_l + tx; // apply translation
                let ny = y + margin_t + ty;
                if let Some(m) = self.get_mut(kid) {
                    m.layout = LayoutRect { x: nx, y: ny, w, h }
                }
                // Compute inner content rect from snapped padding
                let inner = LayoutRect {
                    x: nx + pad_l,
                    y: ny + pad_t,
                    w: (w - pad_l - pad_r).max(0.0),
                    h: (h - pad_t - pad_b).max(0.0),
                };
                self.layout_children(kid, inner);
                x += margin_l + w + margin_r;
                if i + 1 < kids.len() {
                    x += gap;
                }
            }
        }
        // Update own content height usage (optional)
        let _ = id;
    }

    fn layout_col(&mut self, id: NodeId, content: LayoutRect, gap: f32, kids: &[NodeId]) {
        let mut fixed = 0.0;
        let mut flex_sum = 0.0;
        let gap_total = if kids.len() > 1 { gap * ((kids.len() - 1) as f32) } else { 0.0 };
        for &kid in kids {
            if let Some(n) = self.get(kid) {
                let mh = match n.style.size.h {
                    Dim::Px(px) => px,
                    _ => 0.0,
                } + n.style.margin.top
                    + n.style.margin.bottom;
                fixed += mh;
                flex_sum += n.style.flex_grow.max(0.0);
            }
        }
        let x = content.x;
        let mut y = content.y;
        let leftover = (content.h - fixed - gap_total).max(0.0);
        for (i, &kid) in kids.iter().enumerate() {
            if let Some(n) = self.get(kid) {
                let style = &n.style;
                let margin_l = style.margin.left;
                let margin_r = style.margin.right;
                let margin_t = style.margin.top;
                let margin_b = style.margin.bottom;
                let flex = style.flex_grow.max(0.0);
                let tx = style.transform.tx;
                let ty = style.transform.ty;
                let pad_l = style.padding.left;
                let pad_t = style.padding.top;
                let pad_r = style.padding.right;
                let pad_b = style.padding.bottom;

                let ch = match style.size.h {
                    Dim::Px(px) if px > 0.0 => px,
                    _ => {
                        if flex_sum > 0.0 {
                            leftover * (flex / flex_sum)
                        } else {
                            0.0
                        }
                    }
                };
                let cw = match style.size.w {
                    Dim::Px(px) if px > 0.0 => px,
                    _ => content.w - margin_l - margin_r,
                };
                let w = cw.max(0.0);
                let h = ch.max(0.0);
                let nx = x + margin_l + tx;
                let ny = y + margin_t + ty;
                if let Some(m) = self.get_mut(kid) {
                    m.layout = LayoutRect { x: nx, y: ny, w, h }
                }
                let inner = LayoutRect {
                    x: nx + pad_l,
                    y: ny + pad_t,
                    w: (w - pad_l - pad_r).max(0.0),
                    h: (h - pad_t - pad_b).max(0.0),
                };
                self.layout_children(kid, inner);
                y += margin_t + h + margin_b;
                if i + 1 < kids.len() {
                    y += gap;
                }
            }
        }
        let _ = id;
    }

    // ---- Hit-testing ----

    pub fn hit_test(&self, x: f32, y: f32) -> Option<(NodeId, [f32; 2])> {
        self.hit_test_node(self.root, x, y)
    }

    fn hit_test_node(&self, id: NodeId, x: f32, y: f32) -> Option<(NodeId, [f32; 2])> {
        let n = self.get(id)?;
        if !point_in_rect(x, y, n.layout) {
            return None;
        }
        // Children painted in order; top-most is last child. Search reverse.
        for &kid in n.children.iter().rev() {
            if let Some(hit) = self.hit_test_node(kid, x, y) {
                return Some(hit);
            }
        }
        Some((id, [x - n.layout.x, y - n.layout.y]))
    }

    pub fn route_pointer<F: FnMut(NodeId, [f32; 2])>(&self, x: f32, y: f32, mut handler: F) {
        if let Some((id, p)) = self.hit_test(x, y) {
            handler(id, p);
        }
    }

    pub fn is_descendant(&self, node: NodeId, ancestor: NodeId) -> bool {
        if ancestor == node {
            return true;
        }
        self.is_descendant_impl(ancestor, node)
    }

    fn is_descendant_impl(&self, current: NodeId, target: NodeId) -> bool {
        let Some(node) = self.get(current) else { return false };
        for &child in &node.children {
            if child == target || self.is_descendant_impl(child, target) {
                return true;
            }
        }
        false
    }

    // ---- Draw encoding ----

    pub fn encode_draws(&self, b: &mut DrawListBuilder) {
        self.encode_node(self.root, b, None);
    }

    /// Encode with optional animation overrides per node.
    pub fn encode_draws_with_anims(
        &self,
        b: &mut DrawListBuilder,
        over: &alloc::collections::BTreeMap<NodeId, crate::anim::AnimOverrides>,
    ) {
        self.encode_node(self.root, b, Some(over));
    }

    fn encode_node(
        &self,
        id: NodeId,
        b: &mut DrawListBuilder,
        over: Option<&alloc::collections::BTreeMap<NodeId, crate::anim::AnimOverrides>>,
    ) {
        let Some(n) = self.get(id) else { return };
        // Effective properties with animation overrides
        let mut tx = 0.0_f32;
        let mut ty = 0.0_f32;
        let mut radii = n.style.corner_radii;
        let mut color = n.style.background;
        let mut opacity = n.style.opacity;
        let mut shadow_a = n.style.shadow_alpha;
        if let Some(map) = over {
            if let Some(o) = map.get(&n.id) {
                if let Some(xf) = o.transform {
                    tx += xf.tx;
                    ty += xf.ty;
                }
                if let Some(r) = o.corner_radii {
                    radii = r;
                }
                if let Some(c) = o.color {
                    color = c;
                }
                if let Some(a) = o.opacity {
                    opacity = a;
                }
                if let Some(sa) = o.shadow_alpha {
                    shadow_a = sa;
                }
            }
        }

        // Shadow as simple offset rounded rect behind
        if shadow_a > 0.0 {
            let r = n.layout;
            let rect = gfx::RectF::new(r.x + tx, r.y + ty + 2.0, r.w, r.h);
            let a = shadow_a.clamp(0.0, 1.0);
            b.rrect(rect, radii, gfx::Color::rgba(0.0, 0.0, 0.0, a));
        }
        // Background
        {
            let r = n.layout;
            let rect = gfx::RectF::new(r.x + tx, r.y + ty, r.w, r.h);
            let mut c = color;
            c.a *= opacity.clamp(0.0, 1.0);
            b.rrect(rect, radii, c);
        }
        // Clip children if requested
        if n.style.clip {
            let ri = gfx::RectI::new(
                (n.layout.x + tx).floor() as i32,
                (n.layout.y + ty).floor() as i32,
                n.layout.w.ceil() as i32,
                n.layout.h.ceil() as i32,
            );
            b.clip_push(ri);
        }
        for &kid in &n.children {
            self.encode_node(kid, b, over);
        }
        if n.style.clip {
            b.clip_pop();
        }
    }
}

impl NodeTree {
    pub fn remove_node(&mut self, id: NodeId) {
        if id == self.root {
            return;
        }
        if self.get(id).is_none() {
            return;
        }
        for slot in self.nodes.iter_mut() {
            if let Some(parent) = slot {
                parent.children.retain(|c| *c != id);
            }
        }
        self.remove_subtree(id);
    }

    fn remove_subtree(&mut self, id: NodeId) {
        let children = if let Some(node) = self.get_mut(id) {
            core::mem::take(&mut node.children)
        } else {
            return;
        };
        for child in children {
            self.remove_subtree(child);
        }
        if let Some(slot) = self.nodes.get_mut(id.0 as usize) {
            if slot.is_some() {
                *slot = None;
                self.free_list.push(id);
            }
        }
    }

    pub fn layout_rect(&self, id: NodeId) -> Option<LayoutRect> {
        self.get(id).map(|n| n.layout)
    }

    pub fn style(&self, id: NodeId) -> Option<&NodeStyle> {
        self.get(id).map(|n| &n.style)
    }

    pub fn style_mut(&mut self, id: NodeId) -> Option<&mut NodeStyle> {
        if let Some(node) = self.get_mut(id) {
            Some(&mut node.style)
        } else {
            None
        }
    }

    pub fn root_style_mut(&mut self) -> Option<&mut NodeStyle> {
        self.style_mut(self.root)
    }

    pub fn apply_layouts(&mut self, updates: &[(NodeId, LayoutRect)]) {
        for (id, rect) in updates {
            if let Some(node) = self.get_mut(*id) {
                node.layout = *rect;
            }
        }
    }

    pub fn collect_layouts(&self) -> alloc::vec::Vec<(NodeId, LayoutRect)> {
        self.nodes.iter().filter_map(|slot| slot.as_ref().map(|n| (n.id, n.layout))).collect()
    }
}

impl Clone for NodeTree {
    fn clone(&self) -> Self {
        Self { nodes: self.nodes.clone(), root: self.root, free_list: self.free_list.clone() }
    }
}

fn content_rect(style: &NodeStyle, layout: &LayoutRect) -> LayoutRect {
    LayoutRect {
        x: layout.x + style.padding.left,
        y: layout.y + style.padding.top,
        w: (layout.w - style.padding.left - style.padding.right).max(0.0),
        h: (layout.h - style.padding.top - style.padding.bottom).max(0.0),
    }
}

fn point_in_rect(x: f32, y: f32, r: LayoutRect) -> bool {
    x >= r.x && y >= r.y && x < r.x + r.w && y < r.y + r.h
}
