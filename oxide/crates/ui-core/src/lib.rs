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
mod text_boundary;
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
pub use surface::{
    ChromeMetrics, DirtyClass, DirtySet, InteractionBlockGuard, RetainedCompositionStats,
    RetainedDrawStatus, ScatterSpec, SurfaceRouter, UiSurface,
};
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

    pub fn append_drawlist(&mut self, list: &gfx::DrawList) -> bool {
        let Ok(vertex_offset) = u32::try_from(self.list.vertices.len()) else {
            return false;
        };
        let Ok(index_offset) = u32::try_from(self.list.indices.len()) else {
            return false;
        };
        let mut adjusted = alloc::vec::Vec::with_capacity(list.items.len());
        let mut indices = alloc::vec::Vec::with_capacity(list.indices.len());
        for item in &list.items {
            let Some(cmd) = offset_draw_cmd(item, list, vertex_offset, index_offset, &mut indices)
            else {
                return false;
            };
            adjusted.push(cmd);
        }
        self.list.vertices.extend_from_slice(&list.vertices);
        self.list.indices.extend_from_slice(&indices);
        self.list.items.extend(adjusted);
        true
    }

    pub fn append_drawlist_with_text_atlas_revision(
        &mut self,
        list: &gfx::DrawList,
        atlas: gfx::ImageHandle,
        revision: u64,
    ) -> bool {
        self.append_drawlist_with_text_atlas_revisions(list, &[(atlas, revision)])
    }

    pub fn append_drawlist_with_text_atlas_revisions(
        &mut self,
        list: &gfx::DrawList,
        atlases: &[(gfx::ImageHandle, u64)],
    ) -> bool {
        if !list.text_atlas_revisions_compatible(atlases) {
            return false;
        }
        self.append_drawlist(list)
    }

    pub fn append_retained_drawlist(&mut self, list: &gfx::DrawList) -> bool {
        append_retained_drawlist(self, list, None)
    }

    pub fn append_retained_drawlist_with_text_atlas_revisions(
        &mut self,
        list: &gfx::DrawList,
        atlases: &[(gfx::ImageHandle, u64)],
    ) -> bool {
        append_retained_drawlist(self, list, Some(atlases))
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
        let indices =
            if indices.is_empty() && vertices.len() == 4 { &quad_indices[..] } else { indices };
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

    pub fn spinner(&mut self, center: [f32; 2], atom: f32, alpha: f32) {
        self.list.items.push(gfx::DrawCmd::Spinner { center, atom, alpha });
    }
}

fn offset_vertex_span(span: gfx::VertexSpan, base: u32) -> Option<gfx::VertexSpan> {
    Some(gfx::VertexSpan { offset: span.offset.checked_add(base)?, len: span.len })
}

#[derive(Clone, Copy)]
enum IndexMode {
    Local,
    Absolute { vertex_base: u32 },
}

fn validate_vertex_span(list: &gfx::DrawList, span: gfx::VertexSpan) -> Option<()> {
    let end = span.offset.checked_add(span.len)?;
    if end as usize <= list.vertices.len() {
        Some(())
    } else {
        None
    }
}

fn index_mode(source: &[u16], vertex_base: u32, vertex_count: u32) -> Option<IndexMode> {
    if source.is_empty() {
        return Some(IndexMode::Local);
    }
    if vertex_count == 0 {
        return None;
    }
    if vertex_count <= u16::MAX as u32 {
        let local_limit = vertex_count as u16;
        if source.iter().all(|index| *index < local_limit) {
            return Some(IndexMode::Local);
        }
    }

    let vertex_end = vertex_base.checked_add(vertex_count)?;
    for index in source.iter().copied() {
        let absolute = index as u32;
        if absolute < vertex_base || absolute >= vertex_end {
            return None;
        }
    }
    Some(IndexMode::Absolute { vertex_base })
}

fn append_localized_indices(
    list: &gfx::DrawList,
    vb: gfx::VertexSpan,
    ib: gfx::IndexSpan,
    index_base: u32,
    out: &mut alloc::vec::Vec<u16>,
) -> Option<gfx::IndexSpan> {
    let offset = index_base.checked_add(u32::try_from(out.len()).ok()?)?;
    let count = ib.len as usize;
    if count == 0 {
        return Some(gfx::IndexSpan { offset, len: 0 });
    }
    let start = ib.offset as usize;
    let end = start.checked_add(count)?;
    let source = list.indices.get(start..end)?;
    let mode = index_mode(source, vb.offset, vb.len)?;
    out.reserve(source.len());
    for index in source.iter().copied() {
        let local = match mode {
            IndexMode::Local => index as u32,
            IndexMode::Absolute { vertex_base } => (index as u32).checked_sub(vertex_base)?,
        };
        if local > u16::MAX as u32 {
            return None;
        }
        out.push(local as u16);
    }
    Some(gfx::IndexSpan { offset, len: ib.len })
}

fn offset_draw_cmd(
    cmd: &gfx::DrawCmd,
    list: &gfx::DrawList,
    vertex_base: u32,
    index_base: u32,
    indices: &mut alloc::vec::Vec<u16>,
) -> Option<gfx::DrawCmd> {
    use gfx::DrawCmd as C;

    match cmd {
        C::LayerBegin { id, rect, dirty } => {
            Some(C::LayerBegin { id: *id, rect: *rect, dirty: *dirty })
        }
        C::LayerEnd => Some(C::LayerEnd),
        C::Solid { vb, ib, color } => {
            validate_vertex_span(list, *vb)?;
            Some(C::Solid {
                vb: offset_vertex_span(*vb, vertex_base)?,
                ib: append_localized_indices(list, *vb, *ib, index_base, indices)?,
                color: *color,
            })
        }
        C::Image { tex, dst, src, alpha } => {
            Some(C::Image { tex: *tex, dst: *dst, src: *src, alpha: *alpha })
        }
        C::ImageMesh { tex, vb, ib, alpha } => {
            validate_vertex_span(list, *vb)?;
            Some(C::ImageMesh {
                tex: *tex,
                vb: offset_vertex_span(*vb, vertex_base)?,
                ib: append_localized_indices(list, *vb, *ib, index_base, indices)?,
                alpha: *alpha,
            })
        }
        C::GlyphRun { run } => {
            let mut next = *run;
            validate_vertex_span(list, next.vb)?;
            next.ib = append_localized_indices(list, next.vb, next.ib, index_base, indices)?;
            next.vb = offset_vertex_span(next.vb, vertex_base)?;
            Some(C::GlyphRun { run: next })
        }
        C::RRect { rect, radii, color } => {
            Some(C::RRect { rect: *rect, radii: *radii, color: *color })
        }
        C::NineSlice { tex, rect, slice, alpha } => {
            Some(C::NineSlice { tex: *tex, rect: *rect, slice: *slice, alpha: *alpha })
        }
        C::Backdrop { rect, sigma, tint, alpha } => {
            Some(C::Backdrop { rect: *rect, sigma: *sigma, tint: *tint, alpha: *alpha })
        }
        C::VisualEffect { rect, effect } => Some(C::VisualEffect { rect: *rect, effect: *effect }),
        C::CameraBg { rect, tint, alpha, grayscale, blur, sigma } => Some(C::CameraBg {
            rect: *rect,
            tint: *tint,
            alpha: *alpha,
            grayscale: *grayscale,
            blur: *blur,
            sigma: *sigma,
        }),
        C::Spinner { center, atom, alpha } => {
            Some(C::Spinner { center: *center, atom: *atom, alpha: *alpha })
        }
        C::ClipPush { rect } => Some(C::ClipPush { rect: *rect }),
        C::ClipPop => Some(C::ClipPop),
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
    let mut scratch = alloc::vec::Vec::with_capacity(list.items.len());
    coalesce_adjacent_draws_reuse(list, &mut scratch);
}

/// Coalesce adjacent draw commands using caller-owned scratch storage.
///
/// This preserves the public `DrawList` contents while allowing hot frame loops to
/// prewarm and reuse both the input and output command buffers.
pub fn coalesce_adjacent_draws_reuse(
    list: &mut gfx::DrawList,
    scratch: &mut alloc::vec::Vec<gfx::DrawCmd>,
) {
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

    let mut items = core::mem::take(&mut list.items);
    scratch.clear();
    scratch.reserve(items.len());
    {
        let mut iter = items.drain(..);
        let Some(mut current) = iter.next() else {
            drop(iter);
            list.items = items;
            return;
        };

        for next in iter {
            if can_merge(&current, &next) {
                merge_into(&mut current, next);
            } else {
                scratch.push(current);
                current = next;
            }
        }

        scratch.push(current);
    }

    list.items = core::mem::take(scratch);
    *scratch = items;
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

impl PartialEq for NodeStyle {
    fn eq(&self, other: &Self) -> bool {
        self.axis == other.axis
            && self.size == other.size
            && self.min_size == other.min_size
            && self.max_size == other.max_size
            && self.margin == other.margin
            && self.padding == other.padding
            && self.gap == other.gap
            && self.flex_grow == other.flex_grow
            && self.background == other.background
            && self.corner_radii == other.corner_radii
            && self.opacity == other.opacity
            && self.transform.tx == other.transform.tx
            && self.transform.ty == other.transform.ty
            && self.transform.sx == other.transform.sx
            && self.transform.sy == other.transform.sy
            && self.transform.rot_rad == other.transform.rot_rad
            && self.shadow_alpha == other.shadow_alpha
            && self.clip == other.clip
            && self.overflow == other.overflow
    }
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
    parent: Option<NodeId>,
    style: NodeStyle,
    children: alloc::vec::Vec<NodeId>,
    layout: LayoutRect,
    last_content: Option<LayoutRect>,
    layout_dirty: bool,
    descendant_layout_dirty: bool,
    draw_dirty: bool,
    retained_draws: Option<gfx::DrawList>,
}

impl Node {
    fn new(id: NodeId, parent: Option<NodeId>, style: NodeStyle) -> Self {
        Self {
            id,
            parent,
            style,
            children: alloc::vec::Vec::new(),
            layout: LayoutRect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 },
            last_content: None,
            layout_dirty: true,
            descendant_layout_dirty: false,
            draw_dirty: true,
            retained_draws: None,
        }
    }
}

pub struct NodeTree {
    nodes: alloc::vec::Vec<Option<Node>>, // dense slot map
    root: NodeId,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RetainedNodeStats {
    pub reused_nodes: u32,
    pub rebuilt_nodes: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutStats {
    pub visited_nodes: u32,
    pub skipped_subtrees: u32,
    pub layout_updates: u32,
    pub measured_children: u32,
}

const INLINE_LAYOUT_CHILDREN: usize = 16;
const MAX_RETAINED_NODE_DRAW_ITEMS: usize = 128;

#[must_use]
pub fn drawlist_retained_replay_safe_with_text_atlas_revisions(
    list: &gfx::DrawList,
    atlases: &[(gfx::ImageHandle, u64)],
) -> bool {
    drawlist_retained_replay_safe_for(list, Some(atlases))
}

pub(crate) fn drawlist_retained_replay_safe_for(
    list: &gfx::DrawList,
    text_atlases: Option<&[(gfx::ImageHandle, u64)]>,
) -> bool {
    list.items.iter().all(|cmd| match cmd {
        gfx::DrawCmd::LayerBegin { .. } | gfx::DrawCmd::LayerEnd => false,
        gfx::DrawCmd::GlyphRun { run } => match text_atlases {
            Some(atlases) => atlases
                .iter()
                .any(|(atlas, revision)| run.atlas == *atlas && run.atlas_revision == *revision),
            None => false,
        },
        _ => true,
    })
}

pub(crate) fn append_retained_drawlist(
    b: &mut DrawListBuilder,
    list: &gfx::DrawList,
    text_atlases: Option<&[(gfx::ImageHandle, u64)]>,
) -> bool {
    if !drawlist_retained_replay_safe_for(list, text_atlases) {
        return false;
    }
    b.append_drawlist(list)
}

fn should_cache_retained_node_draws(
    list: &gfx::DrawList,
    text_atlases: Option<&[(gfx::ImageHandle, u64)]>,
) -> bool {
    list.items.len() <= MAX_RETAINED_NODE_DRAW_ITEMS
        && drawlist_retained_replay_safe_for(list, text_atlases)
}

impl NodeTree {
    pub fn new_root(style: NodeStyle) -> Self {
        let root = NodeId(1);
        let mut nodes = alloc::vec::Vec::with_capacity(8);
        nodes.push(None); // 0 unused
        nodes.push(Some(Node::new(root, None, style)));
        Self { nodes, root }
    }

    pub fn root(&self) -> NodeId {
        self.root
    }

    pub fn add_node(&mut self, parent: NodeId, style: NodeStyle) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(Some(Node::new(id, Some(parent), style)));
        if let Some(p) = self.get_mut(parent) {
            p.children.push(id);
        }
        self.mark_layout_dirty(parent);
        self.mark_node_and_ancestors_draw_dirty(parent);
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

    pub(crate) fn mark_node_and_ancestors_draw_dirty(&mut self, id: NodeId) {
        let mut current = Some(id);
        while let Some(node_id) = current {
            let index = node_id.0 as usize;
            let parent =
                self.nodes.get(index).and_then(|slot| slot.as_ref()).and_then(|node| node.parent);
            if let Some(Some(node)) = self.nodes.get_mut(index) {
                node.draw_dirty = true;
            }
            current = parent;
        }
    }

    pub fn mark_subtree_draw_dirty(&mut self, id: NodeId) {
        self.mark_node_and_ancestors_draw_dirty(id);
        self.mark_descendants_draw_dirty(id);
    }

    fn mark_descendants_draw_dirty(&mut self, id: NodeId) {
        let Some(children) = self.get(id).map(|node| node.children.clone()) else {
            return;
        };
        for child in children {
            if let Some(node) = self.get_mut(child) {
                node.draw_dirty = true;
            }
            self.mark_descendants_draw_dirty(child);
        }
    }

    pub fn mark_layout_dirty(&mut self, id: NodeId) {
        let mut current = Some(id);
        while let Some(node_id) = current {
            let index = node_id.0 as usize;
            let parent =
                self.nodes.get(index).and_then(|slot| slot.as_ref()).and_then(|node| node.parent);
            if let Some(Some(node)) = self.nodes.get_mut(index) {
                node.layout_dirty = true;
                node.descendant_layout_dirty = false;
                node.draw_dirty = true;
            }
            current = parent;
        }
    }

    pub fn mark_node_layout_dirty(&mut self, id: NodeId) {
        let mut current = Some(id);
        let mut first = true;
        while let Some(node_id) = current {
            let index = node_id.0 as usize;
            let parent =
                self.nodes.get(index).and_then(|slot| slot.as_ref()).and_then(|node| node.parent);
            if let Some(Some(node)) = self.nodes.get_mut(index) {
                if first {
                    node.layout_dirty = true;
                } else if !node.layout_dirty {
                    node.descendant_layout_dirty = true;
                }
                node.draw_dirty = true;
            }
            first = false;
            current = parent;
        }
    }

    fn set_layout(&mut self, id: NodeId, rect: LayoutRect) -> bool {
        let changed = if let Some(node) = self.get_mut(id) {
            if node.layout == rect {
                false
            } else {
                node.layout = rect;
                true
            }
        } else {
            false
        };
        if changed {
            self.mark_node_and_ancestors_draw_dirty(id);
        }
        changed
    }

    // ---- Layout ----

    pub fn layout(&mut self, root_w: f32, root_h: f32) -> LayoutStats {
        let mut stats = LayoutStats::default();
        let px = LayoutRect { x: 0.0, y: 0.0, w: root_w.max(0.0), h: root_h.max(0.0) };
        let Some(rect) = self.get(self.root).map(|root| content_rect(&root.style, &px)) else {
            return stats;
        };
        if self.set_layout(self.root, px) {
            stats.layout_updates = stats.layout_updates.saturating_add(1);
        }
        self.layout_children(self.root, rect, &mut stats);
        stats
    }

    fn layout_children(&mut self, id: NodeId, content: LayoutRect, stats: &mut LayoutStats) {
        let (axis, gap, child_count, can_skip, descendant_only) = if let Some(node) = self.get(id) {
            (
                node.style.axis,
                node.style.gap,
                node.children.len(),
                !node.layout_dirty
                    && !node.descendant_layout_dirty
                    && node.last_content == Some(content),
                !node.layout_dirty
                    && node.descendant_layout_dirty
                    && node.last_content == Some(content),
            )
        } else {
            return;
        };
        stats.visited_nodes = stats.visited_nodes.saturating_add(1);
        if can_skip {
            stats.skipped_subtrees = stats.skipped_subtrees.saturating_add(1);
            return;
        }
        if descendant_only {
            self.layout_dirty_descendants(id, stats);
            if let Some(node) = self.get_mut(id) {
                node.descendant_layout_dirty = false;
            }
            return;
        }
        if let Some(node) = self.get_mut(id) {
            node.last_content = Some(content);
            node.layout_dirty = false;
            node.descendant_layout_dirty = false;
        }
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
                Axis::Row => self.layout_row(id, content, gap, &kids[..child_count], stats),
                Axis::Column => self.layout_col(id, content, gap, &kids[..child_count], stats),
            }
        } else {
            let kids: alloc::vec::Vec<NodeId> =
                if let Some(n) = self.get(id) { n.children.clone() } else { return };
            match axis {
                Axis::Row => self.layout_row(id, content, gap, &kids, stats),
                Axis::Column => self.layout_col(id, content, gap, &kids, stats),
            }
        }
    }

    fn layout_child_or_skip(
        &mut self,
        kid: NodeId,
        rect: LayoutRect,
        inner: LayoutRect,
        stats: &mut LayoutStats,
    ) {
        let can_skip = self
            .get(kid)
            .map(|node| {
                !node.layout_dirty
                    && !node.descendant_layout_dirty
                    && node.layout == rect
                    && node.last_content == Some(inner)
            })
            .unwrap_or(false);
        if can_skip {
            stats.skipped_subtrees = stats.skipped_subtrees.saturating_add(1);
            return;
        }
        if self.set_layout(kid, rect) {
            stats.layout_updates = stats.layout_updates.saturating_add(1);
        }
        self.layout_children(kid, inner, stats);
    }

    fn layout_dirty_descendants(&mut self, id: NodeId, stats: &mut LayoutStats) {
        let child_count = if let Some(node) = self.get(id) { node.children.len() } else { return };
        for index in 0..child_count {
            let Some(kid) = self.get(id).and_then(|node| node.children.get(index).copied()) else {
                continue;
            };
            let Some((needs_layout, rect, inner)) = self.get(kid).map(|node| {
                let needs_layout = node.layout_dirty || node.descendant_layout_dirty;
                let rect = node.layout;
                let inner = content_rect(&node.style, &rect);
                (needs_layout, rect, inner)
            }) else {
                continue;
            };
            if !needs_layout {
                stats.skipped_subtrees = stats.skipped_subtrees.saturating_add(1);
                continue;
            }
            if self.set_layout(kid, rect) {
                stats.layout_updates = stats.layout_updates.saturating_add(1);
            }
            self.layout_children(kid, inner, stats);
        }
    }

    fn layout_row(
        &mut self,
        id: NodeId,
        content: LayoutRect,
        gap: f32,
        kids: &[NodeId],
        stats: &mut LayoutStats,
    ) {
        let mut fixed = 0.0;
        let mut flex_sum = 0.0;
        let gap_total = if kids.len() > 1 { gap * ((kids.len() - 1) as f32) } else { 0.0 };
        // Measure pass: collect fixed widths and flex
        for &kid in kids {
            if let Some(n) = self.get(kid) {
                stats.measured_children = stats.measured_children.saturating_add(1);
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
                let nx = x + margin_l;
                let ny = y + margin_t;
                let rect = LayoutRect { x: nx, y: ny, w, h };
                // Compute inner content rect from snapped padding
                let inner = LayoutRect {
                    x: nx + pad_l,
                    y: ny + pad_t,
                    w: (w - pad_l - pad_r).max(0.0),
                    h: (h - pad_t - pad_b).max(0.0),
                };
                self.layout_child_or_skip(kid, rect, inner, stats);
                x += margin_l + w + margin_r;
                if i + 1 < kids.len() {
                    x += gap;
                }
            }
        }
        // Update own content height usage (optional)
        let _ = id;
    }

    fn layout_col(
        &mut self,
        id: NodeId,
        content: LayoutRect,
        gap: f32,
        kids: &[NodeId],
        stats: &mut LayoutStats,
    ) {
        let mut fixed = 0.0;
        let mut flex_sum = 0.0;
        let gap_total = if kids.len() > 1 { gap * ((kids.len() - 1) as f32) } else { 0.0 };
        for &kid in kids {
            if let Some(n) = self.get(kid) {
                stats.measured_children = stats.measured_children.saturating_add(1);
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
                let nx = x + margin_l;
                let ny = y + margin_t;
                let rect = LayoutRect { x: nx, y: ny, w, h };
                let inner = LayoutRect {
                    x: nx + pad_l,
                    y: ny + pad_t,
                    w: (w - pad_l - pad_r).max(0.0),
                    h: (h - pad_t - pad_b).max(0.0),
                };
                self.layout_child_or_skip(kid, rect, inner, stats);
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
        self.hit_test_node(self.root, x, y, 0.0, 0.0)
    }

    fn hit_test_node(
        &self,
        id: NodeId,
        x: f32,
        y: f32,
        parent_tx: f32,
        parent_ty: f32,
    ) -> Option<(NodeId, [f32; 2])> {
        let n = self.get(id)?;
        let tx = parent_tx + n.style.transform.tx;
        let ty = parent_ty + n.style.transform.ty;
        let rect = translated_layout_rect(n.layout, tx, ty);
        if !point_in_rect(x, y, rect) {
            return None;
        }
        // Children painted in order; top-most is last child. Search reverse.
        for &kid in n.children.iter().rev() {
            if let Some(hit) = self.hit_test_node(kid, x, y, tx, ty) {
                return Some(hit);
            }
        }
        Some((id, [x - rect.x, y - rect.y]))
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
        self.encode_node(self.root, b, None, 0.0, 0.0);
    }

    pub fn encode_draws_retained(&mut self, b: &mut DrawListBuilder) -> Option<RetainedNodeStats> {
        self.encode_draws_retained_impl(b, None)
    }

    pub fn encode_draws_retained_with_text_atlas_revisions(
        &mut self,
        b: &mut DrawListBuilder,
        atlases: &[(gfx::ImageHandle, u64)],
    ) -> Option<RetainedNodeStats> {
        self.encode_draws_retained_impl(b, Some(atlases))
    }

    fn encode_draws_retained_impl(
        &mut self,
        b: &mut DrawListBuilder,
        text_atlases: Option<&[(gfx::ImageHandle, u64)]>,
    ) -> Option<RetainedNodeStats> {
        let mut stats = RetainedNodeStats::default();
        if self.encode_node_retained(self.root, b, &mut stats, text_atlases, 0.0, 0.0) {
            Some(stats)
        } else {
            None
        }
    }

    /// Encode with optional animation overrides per node.
    pub fn encode_draws_with_anims(
        &self,
        b: &mut DrawListBuilder,
        over: &alloc::collections::BTreeMap<NodeId, crate::anim::AnimOverrides>,
    ) {
        self.encode_node(self.root, b, Some(over), 0.0, 0.0);
    }

    fn encode_node(
        &self,
        id: NodeId,
        b: &mut DrawListBuilder,
        over: Option<&alloc::collections::BTreeMap<NodeId, crate::anim::AnimOverrides>>,
        parent_tx: f32,
        parent_ty: f32,
    ) {
        let Some(n) = self.get(id) else { return };
        let (tx, ty) = encode_node_frame(n.id, &n.style, n.layout, b, over, parent_tx, parent_ty);
        for &kid in &n.children {
            self.encode_node(kid, b, over, tx, ty);
        }
        if n.style.clip {
            b.clip_pop();
        }
    }

    fn encode_node_retained(
        &mut self,
        id: NodeId,
        b: &mut DrawListBuilder,
        stats: &mut RetainedNodeStats,
        text_atlases: Option<&[(gfx::ImageHandle, u64)]>,
        parent_tx: f32,
        parent_ty: f32,
    ) -> bool {
        if let Some(node) = self.get(id) {
            if !node.draw_dirty {
                if let Some(draws) = node.retained_draws.as_ref() {
                    if append_retained_drawlist(b, draws, text_atlases) {
                        stats.reused_nodes = stats.reused_nodes.saturating_add(1);
                        return true;
                    } else {
                        return false;
                    }
                }
            }
        }

        let Some((style, layout, children)) =
            self.get(id).map(|node| (node.style.clone(), node.layout, node.children.clone()))
        else {
            return true;
        };
        let mut next = DrawListBuilder::new();
        let (tx, ty) = encode_node_frame(id, &style, layout, &mut next, None, parent_tx, parent_ty);
        for child in children {
            if !self.encode_node_retained(child, &mut next, stats, text_atlases, tx, ty) {
                return false;
            }
        }
        if style.clip {
            next.clip_pop();
        }
        let draws = next.into_inner();
        if !b.append_drawlist(&draws) {
            return false;
        }
        if let Some(node) = self.get_mut(id) {
            node.retained_draws = if should_cache_retained_node_draws(&draws, text_atlases) {
                Some(draws)
            } else {
                None
            };
            node.draw_dirty = false;
        }
        stats.rebuilt_nodes = stats.rebuilt_nodes.saturating_add(1);
        true
    }

    pub fn mark_all_draw_dirty(&mut self) {
        for slot in &mut self.nodes {
            if let Some(node) = slot {
                node.draw_dirty = true;
            }
        }
    }
}

fn encode_node_frame(
    id: NodeId,
    style: &NodeStyle,
    layout: LayoutRect,
    b: &mut DrawListBuilder,
    over: Option<&alloc::collections::BTreeMap<NodeId, crate::anim::AnimOverrides>>,
    parent_tx: f32,
    parent_ty: f32,
) -> (f32, f32) {
    let mut tx = parent_tx + style.transform.tx;
    let mut ty = parent_ty + style.transform.ty;
    let mut radii = style.corner_radii;
    let mut color = style.background;
    let mut opacity = style.opacity;
    let mut shadow_a = style.shadow_alpha;
    if let Some(map) = over {
        if let Some(o) = map.get(&id) {
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

    if shadow_a > 0.0 {
        let rect = gfx::RectF::new(layout.x + tx, layout.y + ty + 2.0, layout.w, layout.h);
        let a = shadow_a.clamp(0.0, 1.0);
        b.rrect(rect, radii, gfx::Color::rgba(0.0, 0.0, 0.0, a));
    }
    {
        let rect = gfx::RectF::new(layout.x + tx, layout.y + ty, layout.w, layout.h);
        let mut c = color;
        c.a *= opacity.clamp(0.0, 1.0);
        b.rrect(rect, radii, c);
    }
    if style.clip {
        let ri = gfx::RectI::new(
            (layout.x + tx).floor() as i32,
            (layout.y + ty).floor() as i32,
            layout.w.ceil() as i32,
            layout.h.ceil() as i32,
        );
        b.clip_push(ri);
    }
    (tx, ty)
}

impl NodeTree {
    pub fn remove_node(&mut self, id: NodeId) -> bool {
        if id == self.root {
            return false;
        }
        if self.get(id).is_none() {
            return false;
        }
        let parent = self.get(id).and_then(|node| node.parent);
        if let Some(parent_id) = parent {
            if let Some(parent) = self.get_mut(parent_id) {
                parent.children.retain(|c| *c != id);
            }
        }
        self.remove_subtree(id);
        if let Some(parent) = parent {
            self.mark_layout_dirty(parent);
            self.mark_node_and_ancestors_draw_dirty(parent);
        }
        true
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
        self.get(id)?;
        self.mark_layout_dirty(id);
        self.mark_node_and_ancestors_draw_dirty(id);
        self.get_mut(id).map(|node| &mut node.style)
    }

    pub fn edit_style<F: FnOnce(&mut NodeStyle)>(
        &mut self,
        id: NodeId,
        edit: F,
    ) -> Option<(NodeStyle, NodeStyle)> {
        let before = self.get(id)?.style.clone();
        {
            let node = self.get_mut(id)?;
            edit(&mut node.style);
        }
        let after = self.get(id)?.style.clone();
        if before != after {
            self.mark_node_and_ancestors_draw_dirty(id);
        }
        Some((before, after))
    }

    pub fn root_style_mut(&mut self) -> Option<&mut NodeStyle> {
        self.style_mut(self.root)
    }

    pub fn apply_layouts(&mut self, updates: &[(NodeId, LayoutRect)]) {
        for (id, rect) in updates {
            self.set_layout(*id, *rect);
            if let Some(node) = self.get_mut(*id) {
                node.layout_dirty = false;
            }
        }
    }

    pub fn collect_layouts(&self) -> alloc::vec::Vec<(NodeId, LayoutRect)> {
        self.nodes.iter().filter_map(|slot| slot.as_ref().map(|n| (n.id, n.layout))).collect()
    }
}

impl Clone for NodeTree {
    fn clone(&self) -> Self {
        let nodes = self
            .nodes
            .iter()
            .map(|slot| {
                slot.as_ref().map(|node| {
                    let mut next = node.clone();
                    next.draw_dirty = true;
                    next.layout_dirty = true;
                    next.descendant_layout_dirty = false;
                    next.last_content = None;
                    next.retained_draws = None;
                    next
                })
            })
            .collect();
        Self { nodes, root: self.root }
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

#[inline]
fn translated_layout_rect(rect: LayoutRect, tx: f32, ty: f32) -> LayoutRect {
    LayoutRect { x: rect.x + tx, y: rect.y + ty, w: rect.w, h: rect.h }
}

fn point_in_rect(x: f32, y: f32, r: LayoutRect) -> bool {
    x >= r.x && y >= r.y && x < r.x + r.w && y < r.y + r.h
}
