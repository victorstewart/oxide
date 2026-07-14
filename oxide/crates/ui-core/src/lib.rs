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
    RetainedDrawStatus, ScatterSpec, SurfaceDamageStats, SurfaceFrameDemand,
    SurfaceRenderChunkStats, SurfaceRenderSnapshot, SurfaceRenderSnapshotError, SurfaceRouter,
    UiSurface,
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
use oxide_timing as timing;

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
    fn draw_visible(&self) -> bool {
        self.clip_stack.last().map_or(true, rect_i_visible)
    }

    fn push_draw_cmd(&mut self, cmd: gfx::DrawCmd) {
        match cmd {
            gfx::DrawCmd::ClipPush { rect } => {
                let effective = self
                    .clip_stack
                    .last()
                    .copied()
                    .map_or(rect, |current| intersect_rect_i(current, rect));
                self.clip_stack.push(effective);
                self.list.items.push(gfx::DrawCmd::ClipPush { rect });
            }
            gfx::DrawCmd::ClipPop => {
                let _ = self.clip_stack.pop();
                self.list.items.push(gfx::DrawCmd::ClipPop);
            }
            gfx::DrawCmd::LayerBegin { .. } | gfx::DrawCmd::LayerEnd => {
                self.list.items.push(cmd);
            }
            _ if self.draw_visible() && draw_cmd_visible(&cmd) => self.list.items.push(cmd),
            _ => {}
        }
    }

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
            if !draw_cmd_visible(item) {
                continue;
            }
            let Some(cmd) = offset_draw_cmd(item, list, vertex_offset, index_offset, &mut indices)
            else {
                return false;
            };
            adjusted.push(cmd);
        }
        self.list.vertices.extend_from_slice(&list.vertices);
        self.list.indices.extend_from_slice(&indices);
        for cmd in adjusted {
            self.push_draw_cmd(cmd);
        }
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

   pub fn append_render_snapshot_flat(&mut self, snapshot: &gfx::RenderSnapshot) -> Result<gfx::RenderFallbackStats, gfx::RenderSnapshotError>
   {
      snapshot.flatten_into(&mut self.list)
   }

    #[inline]
    pub fn layer_begin(&mut self, id: u32, rect: gfx::RectF, dirty: bool) {
        self.push_draw_cmd(gfx::DrawCmd::LayerBegin { id, rect, dirty });
    }

    #[inline]
    pub fn layer_end(&mut self) {
        self.push_draw_cmd(gfx::DrawCmd::LayerEnd);
    }

    #[inline]
    pub fn clip_push(&mut self, rect: gfx::RectI) {
        self.push_draw_cmd(gfx::DrawCmd::ClipPush { rect });
    }

    #[inline]
    pub fn clip_pop(&mut self) {
        self.push_draw_cmd(gfx::DrawCmd::ClipPop);
    }

    pub fn solid(&mut self, vb: gfx::VertexSpan, ib: gfx::IndexSpan, color: gfx::Color) {
        self.push_draw_cmd(gfx::DrawCmd::Solid { vb, ib, color });
    }

    pub fn image(&mut self, tex: gfx::ImageHandle, dst: gfx::RectF, src: gfx::RectF, alpha: f32) {
        self.push_draw_cmd(gfx::DrawCmd::Image { tex, dst, src, alpha });
    }

    pub(crate) fn image_prevalidated(&mut self, tex: gfx::ImageHandle, dst: gfx::RectF, src: gfx::RectF, alpha: f32) {
        // ImageView proves intrinsic visibility once while deriving its crop; clip visibility remains builder-owned.
        debug_assert!(draw_cmd_visible(&gfx::DrawCmd::Image { tex, dst, src, alpha }));
        if self.draw_visible() {
            self.list.items.push(gfx::DrawCmd::Image { tex, dst, src, alpha });
        }
    }

    pub fn image_mesh(
        &mut self,
        tex: gfx::ImageHandle,
        vertices: &[gfx::Vertex],
        indices: &[u16],
        alpha: f32,
    ) {
        if tex.0 == 0 || !alpha_visible(alpha) || !vertices_visible(vertices) {
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
        self.push_draw_cmd(gfx::DrawCmd::ImageMesh {
            tex,
            vb: gfx::VertexSpan { offset: vb_offset, len: vb_len },
            ib: gfx::IndexSpan { offset: ib_offset, len: ib_len },
            alpha,
        });
    }

    pub fn glyph_run(&mut self, run: gfx::GlyphRun) {
        self.push_draw_cmd(gfx::DrawCmd::GlyphRun { run });
    }

    pub(crate) fn glyph_run_provisional(&mut self, run: gfx::GlyphRun) {
        if self.draw_visible()
            && run.vb.len != 0
            && run.ib.len != 0
            && color_visible(run.color)
        {
            self.list.items.push(gfx::DrawCmd::GlyphRun { run });
        }
    }

    pub fn glyph_run_resolved(
        &mut self,
        run: gfx::GlyphRun,
        vertices: &[gfx::Vertex],
        indices: &[u16],
    ) {
        if run.atlas.0 == 0
            || !color_visible(run.color)
            || !vertices_visible(vertices)
            || indices.is_empty()
        {
            return;
        }
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
        self.glyph_run(gfx::GlyphRun {
            vb: gfx::VertexSpan { offset: vb_offset, len: vb_len },
            ib: gfx::IndexSpan { offset: ib_offset, len: ib_len },
            ..run
        });
    }

    pub fn rrect(&mut self, rect: gfx::RectF, radii: [f32; 4], color: gfx::Color) {
        self.push_draw_cmd(gfx::DrawCmd::RRect { rect, radii, color });
    }

    pub fn nine_slice(
        &mut self,
        tex: gfx::ImageHandle,
        rect: gfx::RectF,
        slice: gfx::Insets,
        alpha: f32,
    ) {
        self.push_draw_cmd(gfx::DrawCmd::NineSlice { tex, rect, slice, alpha });
    }

    pub fn backdrop(&mut self, rect: gfx::RectF, sigma: f32, tint: gfx::Color, alpha: f32) {
        self.push_draw_cmd(gfx::DrawCmd::Backdrop { rect, sigma, tint, alpha });
    }

    pub fn visual_effect(&mut self, rect: gfx::RectF, effect: gfx::VisualEffect) {
        self.push_draw_cmd(gfx::DrawCmd::VisualEffect { rect, effect });
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
        self.push_draw_cmd(gfx::DrawCmd::CameraBg {
            rect,
            tint,
            alpha,
            grayscale,
            blur,
            sigma,
        });
    }

    pub fn spinner(&mut self, center: [f32; 2], atom: f32, alpha: f32) {
        self.push_draw_cmd(gfx::DrawCmd::Spinner { center, atom, alpha });
    }
}

#[inline]
fn alpha_visible(alpha: f32) -> bool {
    alpha.is_finite() && alpha > 0.0
}

#[inline]
fn color_finite(color: gfx::Color) -> bool {
    color.r.is_finite() && color.g.is_finite() && color.b.is_finite() && color.a.is_finite()
}

#[inline]
fn color_visible(color: gfx::Color) -> bool {
    color_finite(color) && color.a > 0.0
}

#[inline]
fn rect_f_visible(rect: gfx::RectF) -> bool {
    rect.x.is_finite()
        && rect.y.is_finite()
        && rect.w.is_finite()
        && rect.h.is_finite()
        && rect.w > 0.0
        && rect.h > 0.0
}

#[inline]
fn rect_i_visible(rect: &gfx::RectI) -> bool {
    rect.w > 0 && rect.h > 0
}

fn intersect_rect_i(a: gfx::RectI, b: gfx::RectI) -> gfx::RectI {
    let x1 = i64::from(a.x).max(i64::from(b.x));
    let y1 = i64::from(a.y).max(i64::from(b.y));
    let x2 = (i64::from(a.x) + i64::from(a.w)).min(i64::from(b.x) + i64::from(b.w));
    let y2 = (i64::from(a.y) + i64::from(a.h)).min(i64::from(b.y) + i64::from(b.h));
    let w = (x2 - x1).clamp(0, i64::from(i32::MAX)) as i32;
    let h = (y2 - y1).clamp(0, i64::from(i32::MAX)) as i32;
    gfx::RectI {
        x: x1.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        y: y1.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        w,
        h,
    }
}

fn vertices_visible(vertices: &[gfx::Vertex]) -> bool {
    let Some(first) = vertices.first() else {
        return false;
    };
    if !vertex_finite(first) {
        return false;
    }
    let (mut min_x, mut max_x) = (first.x, first.x);
    let (mut min_y, mut max_y) = (first.y, first.y);
    for vertex in &vertices[1..] {
        if !vertex_finite(vertex) {
            return false;
        }
        min_x = min_x.min(vertex.x);
        max_x = max_x.max(vertex.x);
        min_y = min_y.min(vertex.y);
        max_y = max_y.max(vertex.y);
    }
    max_x > min_x && max_y > min_y
}

#[inline]
fn vertex_finite(vertex: &gfx::Vertex) -> bool {
    vertex.x.is_finite()
        && vertex.y.is_finite()
        && vertex.u.is_finite()
        && vertex.v.is_finite()
}

fn visual_effect_visible(effect: gfx::VisualEffect) -> bool {
    match effect {
        gfx::VisualEffect::UIKitDark => true,
        gfx::VisualEffect::DarkPopup { blur_intensity, tint } => {
            blur_intensity.is_finite()
                && color_finite(tint)
                && (blur_intensity > 0.0 || tint.a > 0.0)
        }
    }
}

fn draw_cmd_visible(cmd: &gfx::DrawCmd) -> bool {
    match cmd {
        gfx::DrawCmd::LayerBegin { .. }
        | gfx::DrawCmd::LayerEnd
        | gfx::DrawCmd::ClipPush { .. }
        | gfx::DrawCmd::ClipPop => true,
        gfx::DrawCmd::Solid { vb, color, .. } => vb.len > 0 && color_visible(*color),
        gfx::DrawCmd::Image { tex, dst, src, alpha } => {
            tex.0 != 0 && rect_f_visible(*dst) && rect_f_visible(*src) && alpha_visible(*alpha)
        }
        gfx::DrawCmd::ImageMesh { tex, vb, alpha, .. } => {
            tex.0 != 0 && vb.len > 0 && alpha_visible(*alpha)
        }
        gfx::DrawCmd::GlyphRun { run } => {
            run.atlas.0 != 0 && run.vb.len > 0 && color_visible(run.color)
        }
        gfx::DrawCmd::RRect { rect, radii, color } => {
            rect_f_visible(*rect)
                && radii.iter().all(|radius| radius.is_finite())
                && color_visible(*color)
        }
        gfx::DrawCmd::NineSlice { tex, rect, slice, alpha } => {
            tex.0 != 0
                && rect_f_visible(*rect)
                && [slice.left, slice.top, slice.right, slice.bottom]
                    .iter()
                    .all(|value| value.is_finite())
                && alpha_visible(*alpha)
        }
        gfx::DrawCmd::Backdrop { rect, sigma, tint, alpha } => {
            rect_f_visible(*rect)
                && sigma.is_finite()
                && color_finite(*tint)
                && alpha.is_finite()
                && (*sigma > 0.0 || tint.a * *alpha > 0.0)
        }
        gfx::DrawCmd::VisualEffect { rect, effect } => {
            rect_f_visible(*rect) && visual_effect_visible(*effect)
        }
        gfx::DrawCmd::CameraBg { rect, tint, alpha, sigma, .. } => {
            rect_f_visible(*rect)
                && color_finite(*tint)
                && alpha_visible(*alpha)
                && sigma.is_finite()
        }
        gfx::DrawCmd::Spinner { center, atom, alpha } => {
            center.iter().all(|value| value.is_finite())
                && atom.is_finite()
                && *atom > 0.0
                && alpha_visible(*alpha)
        }
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
    single_line_text_selection_rect, text_caret_visible, text_char_slice,
    text_floating_placeholder_elapsed_progress, text_floating_placeholder_layout,
    text_floating_placeholder_target, text_floating_placeholder_tick, text_input_option_at,
    text_input_options_layout, text_selection_drag_anchor_at, text_word_range_at_char_index,
    EditableText, FieldFailRestoreMode, HorizontalShiftingText, SecureText, TextCaretDragState,
    TextFieldPolicy, TextFloatingPlaceholderLayout, TextInputOption, TextInputOptionsConfig,
    TextInputOptionsLayout, TextInputOptionsPopoverState, TextInputOptionsPopoverStyle,
    TextSelectionDragAnchor, TextSelectionDragState, TextSelectionHighlightLayout,
    TextSelectionHighlightStyle, TextSelectionState, TextTapMemory,
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

#[derive(Clone, Copy, Debug)]
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
    index_in_parent: usize,
    style: NodeStyle,
    children: alloc::vec::Vec<NodeId>,
    layout: LayoutRect,
    last_content: Option<LayoutRect>,
    layout_dirty: bool,
    descendant_layout_dirty: bool,
    chunk_dirty: bool,
    sequence_dirty: bool,
    dirty_child: Option<usize>,
    all_children_dirty: bool,
    retained_chunk: Option<gfx::RenderChunk>,
    retained_sequence: Option<gfx::RenderChunkSequence>,
    retained_composition: Option<NodeCompositionState>,
    chunk_revisions: gfx::RenderChunkRevisions,
    cache_last_used_generation: u64,
    cache_hits: u32,
    cache_hit_since_build: bool,
    cache_invalidation_streak: u8,
    cache_suppressed_until: u64,
    cache_lru_prev: Option<NodeId>,
    cache_lru_next: Option<NodeId>,
    cache_lru_listed: bool,
    transform_slot: gfx::RenderPropertySlotId,
    opacity_slot: gfx::RenderPropertySlotId,
    transform_revision: u64,
    opacity_revision: u64,
    world_transform: [f32; 6],
    world_opacity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct NodeCompositionState {
    layout: LayoutRect,
    clip: bool,
}

impl Node {
    fn new(id: NodeId, parent: Option<NodeId>, style: NodeStyle, transform_slot: gfx::RenderPropertySlotId, opacity_slot: gfx::RenderPropertySlotId) -> Self {
        Self {
            id,
            parent,
            index_in_parent: 0,
            style,
            children: alloc::vec::Vec::new(),
            layout: LayoutRect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 },
            last_content: None,
            layout_dirty: true,
            descendant_layout_dirty: false,
            chunk_dirty: true,
            sequence_dirty: true,
            dirty_child: None,
            all_children_dirty: true,
            retained_chunk: None,
            retained_sequence: None,
            retained_composition: None,
            chunk_revisions: gfx::RenderChunkRevisions::default(),
            cache_last_used_generation: 0,
            cache_hits: 0,
            cache_hit_since_build: false,
            cache_invalidation_streak: 0,
            cache_suppressed_until: 0,
            cache_lru_prev: None,
            cache_lru_next: None,
            cache_lru_listed: false,
            transform_slot,
            opacity_slot,
            transform_revision: 1,
            opacity_revision: 1,
            world_transform: affine_identity(),
            world_opacity: 1.0,
        }
    }
}

#[derive(Clone, Default)]
struct DynamicPropertySlotArena
{
   generations: Vec<u32>,
   free: Vec<u32>,
}

impl DynamicPropertySlotArena
{
   fn allocate(&mut self) -> gfx::RenderPropertySlotId
   {
      if let Some(index) = self.free.pop()
      {
         let generation = self.generations[index as usize];
         return gfx::RenderPropertySlotId::dynamic(index, generation)
            .expect("recycled dynamic property slot is valid");
      }
      let index = self.generations.len() as u32;
      let index = index.max(1);
      if self.generations.is_empty()
      {
         self.generations.push(0);
      }
      self.generations.push(1);
      gfx::RenderPropertySlotId::dynamic(index, 1)
         .expect("dynamic property slot capacity exceeded")
   }

   fn release(&mut self, id: gfx::RenderPropertySlotId)
   {
      let (Some(index), Some(generation)) = (id.dynamic_index(), id.dynamic_generation()) else { return };
      let Some(current) = self.generations.get_mut(index as usize) else { return };
      if *current != generation
      {
         return;
      }
      *current = current.wrapping_add(1) & 0x7ff;
      if *current == 0
      {
         *current = 1;
      }
      self.free.push(index);
   }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RetainedInvalidationReason
{
   #[default]
   None,
   Namespace,
   Style,
   Layout,
   Animation,
   Budget,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetainedCachePolicy
{
   pub cpu_budget_bytes: u64,
   pub prepared_gpu_budget_bytes: u64,
   pub hot_hit_threshold: u32,
   pub hot_generation_window: u64,
   pub churn_invalidation_threshold: u8,
   pub churn_retry_generations: u64,
}

impl Default for RetainedCachePolicy
{
   fn default() -> Self
   {
      Self {
         cpu_budget_bytes: 8 * 1024 * 1024,
         prepared_gpu_budget_bytes: 8 * 1024 * 1024,
         hot_hit_threshold: 2,
         hot_generation_window: 8,
         churn_invalidation_threshold: 0,
         churn_retry_generations: 8,
      }
   }
}

pub struct NodeTree {
    nodes: alloc::vec::Vec<Option<Node>>, // dense slot map
    root: NodeId,
    retained_namespace: Option<u32>,
    retained_chunk_bytes: u64,
    retained_sequence_bytes: u64,
    retained_cache_policy: RetainedCachePolicy,
    retained_cache_generation: u64,
    pending_cache_evictions: u64,
    pending_cache_evicted_bytes: u64,
    last_invalidation_reason: RetainedInvalidationReason,
    retained_cache_lru_head: Option<NodeId>,
    retained_cache_lru_tail: Option<NodeId>,
    dynamic_slots: DynamicPropertySlotArena,
    dynamic_properties: Vec<gfx::RenderPropertySlot>,
}

fn mark_dirty_child(node: &mut Node, index: Option<usize>) {
    let Some(index) = index else {
        return;
    };
    if node.all_children_dirty {
        return;
    }
    match node.dirty_child {
        Some(current) if current != index => node.all_children_dirty = true,
        Some(_) => {}
        None => node.dirty_child = Some(index),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RetainedNodeStats {
    pub reused_nodes: u32,
    pub rebuilt_nodes: u32,
    pub chunks_reused: u64,
    pub chunks_rebuilt: u64,
    pub sequences_reused: u64,
    pub sequences_rebuilt: u64,
    pub command_bytes_copied: u64,
    pub vertex_bytes_copied: u64,
    pub index_bytes_copied: u64,
    pub retained_chunk_bytes: u64,
    pub retained_sequence_bytes: u64,
    pub prepared_gpu_bytes: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_admissions: u64,
    pub cache_admission_rejections: u64,
    pub cache_evictions: u64,
    pub cache_evicted_bytes: u64,
    pub cache_build_time_ns: u64,
    pub flat_fallback_uses: u64,
    pub cache_complete: bool,
    pub last_invalidation_reason: RetainedInvalidationReason,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutStats {
    pub visited_nodes: u32,
    pub skipped_subtrees: u32,
    pub layout_updates: u32,
    pub measured_children: u32,
}

const INLINE_LAYOUT_CHILDREN: usize = 16;

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

impl NodeTree {
    pub fn new_root(style: NodeStyle) -> Self {
        let root = NodeId(1);
        let mut dynamic_slots = DynamicPropertySlotArena::default();
        let transform_slot = dynamic_slots.allocate();
        let opacity_slot = dynamic_slots.allocate();
        let mut nodes = alloc::vec::Vec::with_capacity(8);
        nodes.push(None); // 0 unused
        nodes.push(Some(Node::new(root, None, style, transform_slot, opacity_slot)));
        Self {
            nodes,
            root,
            retained_namespace: None,
            retained_chunk_bytes: 0,
            retained_sequence_bytes: 0,
            retained_cache_policy: RetainedCachePolicy::default(),
            retained_cache_generation: 0,
            pending_cache_evictions: 0,
            pending_cache_evicted_bytes: 0,
            last_invalidation_reason: RetainedInvalidationReason::None,
            retained_cache_lru_head: None,
            retained_cache_lru_tail: None,
            dynamic_slots,
            dynamic_properties: Vec::with_capacity(16),
        }
    }

    pub fn root(&self) -> NodeId {
        self.root
    }

   #[inline]
   pub fn retained_cache_policy(&self) -> RetainedCachePolicy
   {
      self.retained_cache_policy
   }

   pub fn set_retained_cache_policy(&mut self, policy: RetainedCachePolicy)
   {
      if self.retained_cache_policy == policy
      {
         return;
      }
      self.retained_cache_policy = policy;
      let mut stats = RetainedNodeStats { cache_complete: true, ..RetainedNodeStats::default() };
      self.enforce_retained_cache_budget(&mut stats);
      self.pending_cache_evictions = self.pending_cache_evictions.saturating_add(stats.cache_evictions);
      self.pending_cache_evicted_bytes = self.pending_cache_evicted_bytes
         .saturating_add(stats.cache_evicted_bytes);
   }

    pub fn add_node(&mut self, parent: NodeId, style: NodeStyle) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        let child_index = self.get(parent).map_or(0, |node| node.children.len());
        let transform_slot = self.dynamic_slots.allocate();
        let opacity_slot = self.dynamic_slots.allocate();
        let mut node = Node::new(id, Some(parent), style, transform_slot, opacity_slot);
        node.index_in_parent = child_index;
        self.nodes.push(Some(node));
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

   fn record_cache_invalidation(&mut self, id: NodeId, reason: RetainedInvalidationReason)
   {
      self.last_invalidation_reason = reason;
      let generation = self.retained_cache_generation;
      let threshold = self.retained_cache_policy.churn_invalidation_threshold;
      let retry = self.retained_cache_policy.churn_retry_generations;
      let Some(node) = self.get_mut(id) else { return };
      if node.retained_chunk.is_some()
      {
         if node.cache_hit_since_build
         {
            node.cache_invalidation_streak = 0;
         }
         else
         {
            node.cache_invalidation_streak = node.cache_invalidation_streak.saturating_add(1);
         }
         if threshold > 0 && node.cache_invalidation_streak >= threshold
         {
            node.cache_suppressed_until = generation.saturating_add(retry.max(1));
         }
      }
      node.cache_hit_since_build = false;
   }

    pub(crate) fn mark_node_and_ancestors_draw_dirty(&mut self, id: NodeId) {
        self.mark_node_and_ancestors_draw_dirty_for(id, RetainedInvalidationReason::Style);
    }

    pub(crate) fn mark_node_and_ancestors_draw_dirty_for(&mut self, id: NodeId, reason: RetainedInvalidationReason) {
        self.record_cache_invalidation(id, reason);
        let mut current = Some(id);
        let mut target = true;
        let mut child_index = None;
        while let Some(node_id) = current {
            let index = node_id.0 as usize;
            let parent =
                self.nodes.get(index).and_then(|slot| slot.as_ref()).and_then(|node| node.parent);
            if let Some(Some(node)) = self.nodes.get_mut(index) {
                node.sequence_dirty = true;
                if target {
                    node.chunk_dirty = true;
                }
                mark_dirty_child(node, child_index);
            }
            target = false;
            child_index = self.nodes.get(index).and_then(|slot| slot.as_ref())
                .map(|node| node.index_in_parent);
            current = parent;
        }
    }

    fn mark_node_and_ancestors_sequence_dirty(&mut self, id: NodeId) {
        let mut current = Some(id);
        let mut child_index = None;
        while let Some(node_id) = current {
            let index = node_id.0 as usize;
            let parent =
                self.nodes.get(index).and_then(|slot| slot.as_ref()).and_then(|node| node.parent);
            if let Some(Some(node)) = self.nodes.get_mut(index) {
                node.sequence_dirty = true;
                mark_dirty_child(node, child_index);
            }
            child_index = self.nodes.get(index).and_then(|slot| slot.as_ref())
                .map(|node| node.index_in_parent);
            current = parent;
        }
    }

    pub fn mark_subtree_draw_dirty(&mut self, id: NodeId) {
        self.mark_node_and_ancestors_draw_dirty(id);
    }

    pub(crate) fn mark_subtree_sequence_dirty(&mut self, id: NodeId)
    {
       self.mark_node_and_ancestors_sequence_dirty(id);
       self.mark_descendant_sequences_dirty(id);
    }

    fn mark_descendant_sequences_dirty(&mut self, id: NodeId)
    {
       let child_count = self.get(id).map_or(0, |node| node.children.len());
       if let Some(node) = self.get_mut(id)
       {
          node.sequence_dirty = true;
          node.all_children_dirty = true;
       }
       for index in 0..child_count
       {
          let Some(child) = self.get(id).and_then(|node| node.children.get(index).copied()) else { continue };
          self.mark_descendant_sequences_dirty(child);
       }
    }

    pub fn mark_layout_dirty(&mut self, id: NodeId) {
        self.record_cache_invalidation(id, RetainedInvalidationReason::Layout);
        let mut current = Some(id);
        while let Some(node_id) = current {
            let index = node_id.0 as usize;
            let parent =
                self.nodes.get(index).and_then(|slot| slot.as_ref()).and_then(|node| node.parent);
            if let Some(Some(node)) = self.nodes.get_mut(index) {
                node.layout_dirty = true;
                node.descendant_layout_dirty = false;
                node.chunk_dirty = true;
                node.sequence_dirty = true;
                node.all_children_dirty = true;
            }
            current = parent;
        }
    }

    pub fn mark_node_layout_dirty(&mut self, id: NodeId) {
        self.record_cache_invalidation(id, RetainedInvalidationReason::Layout);
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
                node.chunk_dirty = true;
                node.sequence_dirty = true;
                node.all_children_dirty = true;
            }
            first = false;
            current = parent;
        }
    }

    fn set_layout(&mut self, id: NodeId, rect: LayoutRect) -> bool {
        let change = if let Some(node) = self.get_mut(id) {
            if node.layout == rect {
                None
            } else {
                let resized = node.layout.w != rect.w || node.layout.h != rect.h;
                node.layout = rect;
                Some(resized)
            }
        } else {
            None
        };
        if let Some(resized) = change {
            self.mark_node_and_ancestors_sequence_dirty(id);
            if resized {
                if let Some(node) = self.get_mut(id) {
                    node.chunk_dirty = true;
                }
            }
        }
        change.is_some()
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
        self.hit_test_node(self.root, x, y, affine_identity(), LayoutRect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 }, None)
    }

    pub fn hit_test_with_anims(&self, x: f32, y: f32, over: &crate::anim::AnimOverrideSlots) -> Option<(NodeId, [f32; 2])> {
        self.hit_test_node(
            self.root,
            x,
            y,
            affine_identity(),
            LayoutRect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 },
            Some(over),
        )
    }

    fn hit_test_node(
        &self,
        id: NodeId,
        x: f32,
        y: f32,
        parent_world: [f32; 6],
        parent_layout: LayoutRect,
        over: Option<&crate::anim::AnimOverrideSlots>,
    ) -> Option<(NodeId, [f32; 2])> {
        let n = self.get(id)?;
        let transform = over.and_then(|overrides| overrides.get(&id))
            .and_then(|override_| override_.transform)
            .unwrap_or(n.style.transform);
        let world = affine_mul(
            parent_world,
            affine_from_transform(
                n.layout.x - parent_layout.x,
                n.layout.y - parent_layout.y,
                transform,
            ),
        );
        let local = affine_inverse_point(world, [x, y])?;
        if local[0] < 0.0 || local[1] < 0.0 || local[0] >= n.layout.w || local[1] >= n.layout.h {
            return None;
        }
        // Children painted in order; top-most is last child. Search reverse.
        for &kid in n.children.iter().rev() {
            if let Some(hit) = self.hit_test_node(kid, x, y, world, n.layout, over) {
                return Some(hit);
            }
        }
        Some((id, local))
    }

    pub fn accessibility_frame(&self, id: NodeId, over: Option<&crate::anim::AnimOverrideSlots>) -> Option<gfx::RectF>
    {
       let (world, layout) = self.node_world_transform(id, over)?;
       let points = [
          affine_point(world, [0.0, 0.0]),
          affine_point(world, [layout.w, 0.0]),
          affine_point(world, [0.0, layout.h]),
          affine_point(world, [layout.w, layout.h]),
       ];
       let mut x0 = points[0][0];
       let mut y0 = points[0][1];
       let mut x1 = x0;
       let mut y1 = y0;
       for point in points.iter().skip(1)
       {
          x0 = x0.min(point[0]);
          y0 = y0.min(point[1]);
          x1 = x1.max(point[0]);
          y1 = y1.max(point[1]);
       }
       Some(gfx::RectF::new(x0, y0, x1 - x0, y1 - y0))
    }

    fn node_world_transform(&self, id: NodeId, over: Option<&crate::anim::AnimOverrideSlots>) -> Option<([f32; 6], LayoutRect)>
    {
       let node = self.get(id)?;
       let mut lineage = Vec::new();
       let mut current = Some(id);
       while let Some(node) = current
       {
          lineage.push(node);
          current = self.get(node).and_then(|node| node.parent);
       }
       let mut world = affine_identity();
       let mut parent_layout = LayoutRect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
       for node in lineage.iter().rev().copied()
       {
          let current = self.get(node)?;
          let transform = over.and_then(|overrides| overrides.get(&node))
             .and_then(|override_| override_.transform)
             .unwrap_or(current.style.transform);
          world = affine_mul(
             world,
             affine_from_transform(
                current.layout.x - parent_layout.x,
                current.layout.y - parent_layout.y,
                transform,
             ),
          );
          parent_layout = current.layout;
       }
       Some((world, node.layout))
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

    pub fn render_sequence(&mut self, namespace: u32) -> Result<(gfx::RenderChunkSequence, RetainedNodeStats), gfx::RenderChunkError> {
        self.render_sequence_impl(namespace, None)
    }

    pub fn render_sequence_with_anims(&mut self, namespace: u32, over: &crate::anim::AnimOverrideSlots) -> Result<(gfx::RenderChunkSequence, RetainedNodeStats), gfx::RenderChunkError> {
        self.render_sequence_impl(namespace, Some(over))
    }

   fn render_sequence_impl(&mut self, namespace: u32, over: Option<&crate::anim::AnimOverrideSlots>) -> Result<(gfx::RenderChunkSequence, RetainedNodeStats), gfx::RenderChunkError>
   {
      self.prepare_dynamic_properties(over);
      self.retained_cache_generation = self.retained_cache_generation.saturating_add(1);
      if self.retained_namespace != Some(namespace)
      {
         self.mark_all_draw_dirty_for(RetainedInvalidationReason::Namespace);
      }
      let mut stats = RetainedNodeStats {
         cache_evictions: core::mem::take(&mut self.pending_cache_evictions),
         cache_evicted_bytes: core::mem::take(&mut self.pending_cache_evicted_bytes),
         last_invalidation_reason: self.last_invalidation_reason,
         ..RetainedNodeStats::default()
      };
      if self.retained_cache_policy.cpu_budget_bytes == 0
      {
         let sequence = self.render_uncached_sequence(namespace, over, &mut stats)?;
         self.retained_namespace = Some(namespace);
         stats.cache_misses = stats.chunks_rebuilt;
         stats.last_invalidation_reason = RetainedInvalidationReason::Budget;
         self.last_invalidation_reason = RetainedInvalidationReason::None;
         return Ok((sequence, stats));
      }
      let build_started = self.get(self.root)
         .is_some_and(|node| node.sequence_dirty)
         .then(timing::now_ns);
      let (sequence, fully_cached) = self.render_node_sequence(self.root, namespace, over, &mut stats)?;
      if let Some(build_started) = build_started
      {
         stats.cache_build_time_ns = timing::now_ns().saturating_sub(build_started);
      }
      stats.cache_complete = fully_cached;
      self.enforce_retained_cache_budget(&mut stats);
      self.retained_namespace = Some(namespace);
      stats.chunks_reused = sequence.instance_count().saturating_sub(stats.chunks_rebuilt);
      stats.cache_hits = stats.chunks_reused;
      stats.cache_misses = stats.chunks_rebuilt;
      stats.retained_chunk_bytes = self.retained_chunk_bytes;
      stats.retained_sequence_bytes = self.retained_sequence_bytes;
      self.last_invalidation_reason = RetainedInvalidationReason::None;
      Ok((sequence, stats))
   }

   #[inline]
   #[must_use]
   pub fn dynamic_properties(&self) -> &[gfx::RenderPropertySlot]
   {
      &self.dynamic_properties
   }

   fn prepare_dynamic_properties(&mut self, over: Option<&crate::anim::AnimOverrideSlots>)
   {
      self.dynamic_properties.clear();
      self.prepare_dynamic_node(
         self.root,
         affine_identity(),
         LayoutRect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 },
         1.0,
         over,
      );
   }

   fn prepare_dynamic_node(&mut self, id: NodeId, parent_world: [f32; 6], parent_layout: LayoutRect, parent_opacity: f32, over: Option<&crate::anim::AnimOverrideSlots>)
   {
      let Some((style, layout, child_count, transform_slot, opacity_slot, old_transform, old_opacity, transform_revision, opacity_revision)) = self.get(id).map(|node| {
         (
            node.style,
            node.layout,
            node.children.len(),
            node.transform_slot,
            node.opacity_slot,
            node.world_transform,
            node.world_opacity,
            node.transform_revision,
            node.opacity_revision,
         )
      }) else { return };
      let override_ = over.and_then(|overrides| overrides.get(&id));
      let transform = override_.and_then(|override_| override_.transform).unwrap_or(style.transform);
      let local = affine_from_transform(
         layout.x - parent_layout.x,
         layout.y - parent_layout.y,
         transform,
      );
      let world = affine_mul(parent_world, local);
      let property_transform = affine_mul(world, affine_translate(-layout.x, -layout.y));
      let opacity = parent_opacity
         * override_.and_then(|override_| override_.opacity).unwrap_or(style.opacity).clamp(0.0, 1.0);
      let transform_revision = if property_transform == old_transform
      {
         transform_revision
      }
      else
      {
         transform_revision.wrapping_add(1).max(1)
      };
      let opacity_revision = if opacity == old_opacity
      {
         opacity_revision
      }
      else
      {
         opacity_revision.wrapping_add(1).max(1)
      };
      if let Some(node) = self.get_mut(id)
      {
         node.world_transform = property_transform;
         node.world_opacity = opacity;
         node.transform_revision = transform_revision;
         node.opacity_revision = opacity_revision;
      }
      self.dynamic_properties.push(gfx::RenderPropertySlot {
         id: transform_slot,
         revision: transform_revision,
         value: gfx::RenderPropertyValue::Transform(property_transform),
      });
      self.dynamic_properties.push(gfx::RenderPropertySlot {
         id: opacity_slot,
         revision: opacity_revision,
         value: gfx::RenderPropertyValue::Opacity(opacity),
      });
      for index in 0..child_count
      {
         let Some(child) = self.get(id).and_then(|node| node.children.get(index).copied()) else { continue };
         self.prepare_dynamic_node(child, world, layout, opacity, over);
      }
   }

   fn render_uncached_sequence(&self, namespace: u32, over: Option<&crate::anim::AnimOverrideSlots>, stats: &mut RetainedNodeStats) -> Result<gfx::RenderChunkSequence, gfx::RenderChunkError>
   {
      let started = timing::now_ns();
      let mut builder = DrawListBuilder::new();
      if let Some(overrides) = over
      {
         self.encode_draws_with_anims(&mut builder, overrides);
      }
      else
      {
         self.encode_draws(&mut builder);
      }
      let draws = builder.into_inner();
      stats.command_bytes_copied = (draws.items.len() as u64)
         .saturating_mul(core::mem::size_of::<gfx::DrawCmd>() as u64);
      stats.vertex_bytes_copied = (draws.vertices.len() as u64)
         .saturating_mul(core::mem::size_of::<gfx::Vertex>() as u64);
      stats.index_bytes_copied = (draws.indices.len() as u64)
         .saturating_mul(core::mem::size_of::<u16>() as u64);
      let chunk = gfx::RenderChunk::new(
         gfx::RenderChunkId((u64::from(namespace) << 32) | u64::from(self.root.0)),
         gfx::RenderChunkRevisions { structural: self.retained_cache_generation, ..gfx::RenderChunkRevisions::default() },
         draws,
         gfx::ChunkIndexMode::Local,
         &[],
      )?;
      stats.chunks_rebuilt = 1;
      stats.sequences_rebuilt = 1;
      stats.rebuilt_nodes = 1;
      stats.flat_fallback_uses = 1;
      stats.cache_complete = false;
      stats.cache_build_time_ns = timing::now_ns().saturating_sub(started);
      Ok(gfx::RenderChunkSequence::new(vec![gfx::RenderChunkInstance::new(chunk, [0.0, 0.0])]))
   }

    /// Encode with optional animation overrides per node.
    pub fn encode_draws_with_anims(
        &self,
        b: &mut DrawListBuilder,
        over: &crate::anim::AnimOverrideSlots,
    ) {
        self.encode_node(self.root, b, Some(over), 0.0, 0.0);
    }

    fn encode_node(
        &self,
        id: NodeId,
        b: &mut DrawListBuilder,
        over: Option<&crate::anim::AnimOverrideSlots>,
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

    fn render_node_sequence(&mut self, id: NodeId, namespace: u32, over: Option<&crate::anim::AnimOverrideSlots>, stats: &mut RetainedNodeStats) -> Result<(gfx::RenderChunkSequence, bool), gfx::RenderChunkError> {
        if let Some(sequence) = self.get(id).and_then(|node| {
            if node.sequence_dirty { None } else { node.retained_sequence.clone() }
        }) {
            self.touch_retained_cache(id);
            stats.reused_nodes = stats.reused_nodes.saturating_add(1);
            stats.sequences_reused = stats.sequences_reused.saturating_add(1);
            stats.chunks_reused = stats.chunks_reused.saturating_add(sequence.instance_count());
            return Ok((sequence, true));
        }

        let Some((style, layout, chunk_dirty, retained_chunk, revisions, retained_sequence, retained_composition, child_count, dirty_child, all_children_dirty)) = self.get(id).map(|node| {
            (
                effective_node_style(id, node.style, over),
                node.layout,
                node.chunk_dirty,
                node.retained_chunk.clone(),
                node.chunk_revisions,
                node.retained_sequence.clone(),
                node.retained_composition,
                node.children.len(),
                node.dirty_child,
                node.all_children_dirty,
            )
        }) else {
            return Ok((gfx::RenderChunkSequence::new(Vec::new()), false));
        };
        let composition = NodeCompositionState {
            layout,
            clip: style.clip,
        };
        if let Some(mut sequence) = retained_sequence.filter(|sequence| {
            retained_composition == Some(composition) && sequence.child_count() == child_count
        }) {
            let mut children_cached = true;
            let chunk = if chunk_dirty {
                self.build_node_chunk(id, namespace, style, layout, revisions, stats)?
            } else {
                self.touch_retained_cache(id);
                retained_chunk.ok_or(gfx::RenderChunkError::GeometryTooLarge)?
            };
            let instance = self.render_chunk_instance(id, chunk.clone(), layout);
            sequence = sequence.replacing_direct_instance(0, instance)
                .ok_or(gfx::RenderChunkError::GeometryTooLarge)?;
            let child_range = if all_children_dirty {
                0..child_count
            } else if let Some(index) = dirty_child {
                let clean_children = child_count.saturating_sub(1) as u32;
                stats.reused_nodes = stats.reused_nodes.saturating_add(clean_children);
                stats.sequences_reused = stats.sequences_reused.saturating_add(u64::from(clean_children));
                index..index.saturating_add(1).min(child_count)
            } else {
                stats.reused_nodes = stats.reused_nodes.saturating_add(child_count as u32);
                stats.sequences_reused = stats.sequences_reused.saturating_add(child_count as u64);
                0..0
            };
            for index in child_range {
                let Some(child) = self.get(id).and_then(|node| node.children.get(index).copied()) else {
                    continue;
                };
                if self.get(child).is_some_and(|node| node.sequence_dirty) {
                    let (child_sequence, child_cached) = self.render_node_sequence(child, namespace, over, stats)?;
                    children_cached &= child_cached;
                    sequence = sequence.replacing_child(index, child_sequence)
                        .ok_or(gfx::RenderChunkError::GeometryTooLarge)?;
                } else {
                    stats.sequences_reused = stats.sequences_reused.saturating_add(1);
                }
            }
            let cached = self.store_node_sequence(id, chunk, sequence.clone(), composition, children_cached, stats);
            stats.rebuilt_nodes = stats.rebuilt_nodes.saturating_add(1);
            stats.sequences_rebuilt = stats.sequences_rebuilt.saturating_add(1);
            return Ok((sequence, cached));
        }

        let mut child_sequences = Vec::with_capacity(child_count);
        let mut children_cached = true;
        for index in 0..child_count {
            let Some(child) = self.get(id).and_then(|node| node.children.get(index).copied()) else {
                continue;
            };
            let (sequence, cached) = self.render_node_sequence(child, namespace, over, stats)?;
            children_cached &= cached;
            child_sequences.push(sequence);
        }

        let chunk = if !chunk_dirty {
            if let Some(chunk) = retained_chunk {
                self.touch_retained_cache(id);
                stats.chunks_reused = stats.chunks_reused.saturating_add(1);
                chunk
            } else {
                self.build_node_chunk(id, namespace, style, layout, revisions, stats)?
            }
        } else {
            self.build_node_chunk(id, namespace, style, layout, revisions, stats)?
        };

        let instance = self.render_chunk_instance(id, chunk.clone(), layout);
        let children = child_sequences.into_iter().map(|sequence| {
            (sequence, [0.0, 0.0], None)
        }).collect();
        let sequence = gfx::RenderChunkSequence::compose(vec![instance], children);
        let cached = self.store_node_sequence(id, chunk, sequence.clone(), composition, children_cached, stats);
        stats.rebuilt_nodes = stats.rebuilt_nodes.saturating_add(1);
        stats.sequences_rebuilt = stats.sequences_rebuilt.saturating_add(1);
        Ok((sequence, cached))
    }

   fn render_chunk_instance(&self, id: NodeId, chunk: gfx::RenderChunk, layout: LayoutRect) -> gfx::RenderChunkInstance
   {
      let mut instance = gfx::RenderChunkInstance::new(chunk, [layout.x, layout.y]);
      if let Some(node) = self.get(id)
      {
         instance.property_slots = alloc::sync::Arc::from([node.transform_slot, node.opacity_slot]);
      }
      let mut clips = Vec::new();
      let mut current = self.get(id).and_then(|node| node.parent);
      while let Some(ancestor) = current
      {
         let Some(node) = self.get(ancestor) else { break };
         if node.style.clip
         {
            clips.push(gfx::RenderDynamicClip {
               rect: gfx::RectF::new(node.layout.x, node.layout.y, node.layout.w, node.layout.h),
               transform: node.transform_slot,
            });
         }
         current = node.parent;
      }
      clips.reverse();
      instance.dynamic_clips = clips.into();
      instance
   }

   fn store_node_sequence(&mut self, id: NodeId, chunk: gfx::RenderChunk, sequence: gfx::RenderChunkSequence, composition: NodeCompositionState, children_cached: bool, stats: &mut RetainedNodeStats) -> bool
   {
      let generation = self.retained_cache_generation;
      let suppressed = self.get(id).is_some_and(|node| {
         generation < node.cache_suppressed_until
      });
      let old_chunk_bytes = self.get(id).and_then(|node| node.retained_chunk.as_ref())
         .map_or(0, gfx::RenderChunk::byte_size);
      let old_sequence_bytes = self.get(id).and_then(|node| node.retained_sequence.as_ref())
         .map_or(0, gfx::RenderChunkSequence::metadata_byte_size);
      let old_chunk_matches = self.get(id).and_then(|node| node.retained_chunk.as_ref())
         .is_some_and(|old| old.ptr_eq(&chunk));
      let sequence_bytes = if children_cached && !suppressed
      {
         sequence.metadata_byte_size()
      }
      else
      {
         0
      };
      let chunk_bytes = if suppressed { 0 } else { chunk.byte_size() };
      self.retained_chunk_bytes = self.retained_chunk_bytes
         .saturating_sub(old_chunk_bytes)
         .saturating_add(chunk_bytes);
      self.retained_sequence_bytes = self.retained_sequence_bytes
         .saturating_sub(old_sequence_bytes)
         .saturating_add(sequence_bytes);
      if let Some(node) = self.get_mut(id)
      {
         node.retained_chunk = (!suppressed).then_some(chunk);
         node.retained_sequence = (children_cached && !suppressed).then_some(sequence);
         node.retained_composition = (children_cached && !suppressed).then_some(composition);
         node.chunk_dirty = suppressed;
         node.sequence_dirty = !children_cached || suppressed;
         node.dirty_child = None;
         node.all_children_dirty = !children_cached || suppressed;
         if !suppressed
         {
            node.cache_last_used_generation = generation;
            node.cache_hit_since_build = false;
         }
      }
      if suppressed
      {
         self.retained_cache_lru_remove(id);
         stats.cache_admission_rejections = stats.cache_admission_rejections.saturating_add(1);
         false
      }
      else
      {
         self.retained_cache_lru_touch(id);
         if !old_chunk_matches
         {
            stats.cache_admissions = stats.cache_admissions.saturating_add(1);
         }
         children_cached
      }
   }

   fn touch_retained_cache(&mut self, id: NodeId)
   {
      let generation = self.retained_cache_generation;
      if let Some(node) = self.get_mut(id)
      {
         node.cache_last_used_generation = generation;
         node.cache_hits = node.cache_hits.saturating_add(1);
         node.cache_hit_since_build = true;
         node.cache_invalidation_streak = 0;
      }
      self.retained_cache_lru_touch(id);
   }

   fn retained_cache_lru_remove(&mut self, id: NodeId)
   {
      let Some((listed, previous, next)) = self.get(id).map(|node| {
         (node.cache_lru_listed, node.cache_lru_prev, node.cache_lru_next)
      }) else {
         return;
      };
      if !listed
      {
         return;
      }
      if let Some(previous) = previous
      {
         if let Some(node) = self.get_mut(previous)
         {
            node.cache_lru_next = next;
         }
      }
      else
      {
         self.retained_cache_lru_head = next;
      }
      if let Some(next) = next
      {
         if let Some(node) = self.get_mut(next)
         {
            node.cache_lru_prev = previous;
         }
      }
      else
      {
         self.retained_cache_lru_tail = previous;
      }
      if let Some(node) = self.get_mut(id)
      {
         node.cache_lru_prev = None;
         node.cache_lru_next = None;
         node.cache_lru_listed = false;
      }
   }

   fn retained_cache_lru_touch(&mut self, id: NodeId)
   {
      if self.get(id).is_none() || self.retained_cache_lru_tail == Some(id)
      {
         return;
      }
      self.retained_cache_lru_remove(id);
      let previous = self.retained_cache_lru_tail;
      if let Some(previous) = previous
      {
         if let Some(node) = self.get_mut(previous)
         {
            node.cache_lru_next = Some(id);
         }
      }
      else
      {
         self.retained_cache_lru_head = Some(id);
      }
      if let Some(node) = self.get_mut(id)
      {
         node.cache_lru_prev = previous;
         node.cache_lru_next = None;
         node.cache_lru_listed = true;
      }
      self.retained_cache_lru_tail = Some(id);
   }

   #[inline]
   fn retained_cache_bytes(&self) -> u64
   {
      self.retained_chunk_bytes.saturating_add(self.retained_sequence_bytes)
   }

   fn retained_cache_eviction_candidate(&self) -> Option<NodeId>
   {
      let generation = self.retained_cache_generation;
      let policy = self.retained_cache_policy;
      let fallback = self.retained_cache_lru_head;
      let mut current = fallback;
      while let Some(id) = current
      {
         let Some(node) = self.get(id) else { break };
         let hot = node.cache_hits >= policy.hot_hit_threshold
            && generation.saturating_sub(node.cache_last_used_generation) <= policy.hot_generation_window;
         if !hot
         {
            return Some(id);
         }
         current = node.cache_lru_next;
      }
      fallback
   }

   fn evict_retained_cache_entry(&mut self, id: NodeId) -> u64
   {
      self.retained_cache_lru_remove(id);
      let retry_until = self.retained_cache_generation
         .saturating_add(self.retained_cache_policy.churn_retry_generations.max(1));
      let Some(mut current) = self.get(id).map(|_| Some(id)) else { return 0 };
      let mut child_index = None;
      let mut freed = 0_u64;
      let mut first = true;
      while let Some(node_id) = current
      {
         let Some((parent, next_index)) = self.get(node_id).map(|node| {
            (node.parent, node.index_in_parent)
         }) else {
            break;
         };
         let (chunk_bytes, sequence_bytes) = if let Some(node) = self.get_mut(node_id)
         {
            let mut chunk_bytes = 0;
            if first
            {
               if let Some(chunk) = node.retained_chunk.take()
               {
                  chunk_bytes = chunk.byte_size();
               }
               node.chunk_dirty = true;
               node.cache_suppressed_until = retry_until;
               node.cache_hits = 0;
               node.cache_hit_since_build = false;
            }
            let sequence_bytes = node.retained_sequence.take()
               .map_or(0, |sequence| sequence.metadata_byte_size());
            node.retained_composition = None;
            node.sequence_dirty = true;
            mark_dirty_child(node, child_index);
            (chunk_bytes, sequence_bytes)
         }
         else
         {
            (0, 0)
         };
         self.retained_chunk_bytes = self.retained_chunk_bytes.saturating_sub(chunk_bytes);
         self.retained_sequence_bytes = self.retained_sequence_bytes.saturating_sub(sequence_bytes);
         freed = freed.saturating_add(chunk_bytes).saturating_add(sequence_bytes);
         first = false;
         child_index = Some(next_index);
         current = parent;
      }
      self.last_invalidation_reason = RetainedInvalidationReason::Budget;
      freed
   }

   fn enforce_retained_cache_budget(&mut self, stats: &mut RetainedNodeStats)
   {
      let budget = self.retained_cache_policy.cpu_budget_bytes;
      while self.retained_cache_bytes() > budget
      {
         let Some(candidate) = self.retained_cache_eviction_candidate() else { break };
         let freed = self.evict_retained_cache_entry(candidate);
         if freed == 0
         {
            break;
         }
         stats.cache_evictions = stats.cache_evictions.saturating_add(1);
         stats.cache_evicted_bytes = stats.cache_evicted_bytes.saturating_add(freed);
         stats.cache_complete = false;
         stats.last_invalidation_reason = RetainedInvalidationReason::Budget;
      }
   }

   pub fn purge_retained_cache(&mut self) -> (u64, u64)
   {
      let mut entries = 0_u64;
      let mut bytes = 0_u64;
      for index in 1..self.nodes.len()
      {
         let Some(node) = self.nodes[index].as_mut() else { continue };
         if let Some(chunk) = node.retained_chunk.take()
         {
            entries = entries.saturating_add(1);
            bytes = bytes.saturating_add(chunk.byte_size());
         }
         if let Some(sequence) = node.retained_sequence.take()
         {
            bytes = bytes.saturating_add(sequence.metadata_byte_size());
         }
         node.retained_composition = None;
         node.chunk_dirty = true;
         node.sequence_dirty = true;
         node.all_children_dirty = true;
         node.dirty_child = None;
         node.cache_lru_prev = None;
         node.cache_lru_next = None;
         node.cache_lru_listed = false;
         node.cache_hits = 0;
         node.cache_hit_since_build = false;
         node.cache_suppressed_until = 0;
      }
      self.retained_chunk_bytes = 0;
      self.retained_sequence_bytes = 0;
      self.retained_cache_lru_head = None;
      self.retained_cache_lru_tail = None;
      self.pending_cache_evictions = self.pending_cache_evictions.saturating_add(entries);
      self.pending_cache_evicted_bytes = self.pending_cache_evicted_bytes.saturating_add(bytes);
      self.last_invalidation_reason = RetainedInvalidationReason::Budget;
      (entries, bytes)
   }

    fn build_node_chunk(&mut self, id: NodeId, namespace: u32, style: NodeStyle, layout: LayoutRect, revisions: gfx::RenderChunkRevisions, stats: &mut RetainedNodeStats) -> Result<gfx::RenderChunk, gfx::RenderChunkError> {
        let mut builder = DrawListBuilder::new();
        encode_node_local_frame(&style, layout, &mut builder);
        let draws = builder.into_inner();
        stats.command_bytes_copied = stats.command_bytes_copied.saturating_add(
            (draws.items.len() as u64).saturating_mul(core::mem::size_of::<gfx::DrawCmd>() as u64),
        );
        stats.vertex_bytes_copied = stats.vertex_bytes_copied.saturating_add(
            (draws.vertices.len() as u64).saturating_mul(core::mem::size_of::<gfx::Vertex>() as u64),
        );
        stats.index_bytes_copied = stats.index_bytes_copied.saturating_add(
            (draws.indices.len() as u64).saturating_mul(core::mem::size_of::<u16>() as u64),
        );
        let next_revisions = gfx::RenderChunkRevisions {
            geometry: revisions.geometry.saturating_add(1),
            dynamic_properties: revisions.dynamic_properties.saturating_add(1),
            ..revisions
        };
        let chunk = gfx::RenderChunk::new(
            gfx::RenderChunkId((u64::from(namespace) << 32) | u64::from(id.0)),
            next_revisions,
            draws,
            gfx::ChunkIndexMode::Local,
            &[],
        )?;
        if let Some(node) = self.get_mut(id) {
            node.chunk_revisions = next_revisions;
        }
        stats.chunks_rebuilt = stats.chunks_rebuilt.saturating_add(1);
        Ok(chunk)
    }

   pub fn mark_all_draw_dirty(&mut self)
   {
      self.mark_all_draw_dirty_for(RetainedInvalidationReason::Style);
   }

   pub(crate) fn mark_all_draw_dirty_for(&mut self, reason: RetainedInvalidationReason)
   {
      for index in 1..self.nodes.len()
      {
         let id = NodeId(index as u32);
         self.record_cache_invalidation(id, reason);
         if let Some(node) = self.get_mut(id)
         {
            node.chunk_dirty = true;
            node.sequence_dirty = true;
            node.all_children_dirty = true;
         }
      }
   }
}

#[inline]
const fn affine_identity() -> [f32; 6]
{
   [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
}

#[inline]
const fn affine_translate(x: f32, y: f32) -> [f32; 6]
{
   [1.0, 0.0, 0.0, 1.0, x, y]
}

#[inline]
fn affine_mul(a: [f32; 6], b: [f32; 6]) -> [f32; 6]
{
   [
      a[0] * b[0] + a[2] * b[1],
      a[1] * b[0] + a[3] * b[1],
      a[0] * b[2] + a[2] * b[3],
      a[1] * b[2] + a[3] * b[3],
      a[0] * b[4] + a[2] * b[5] + a[4],
      a[1] * b[4] + a[3] * b[5] + a[5],
   ]
}

#[inline]
fn affine_from_transform(layout_x: f32, layout_y: f32, transform: plat::Transform2D) -> [f32; 6]
{
   let (sin, cos) = transform.rot_rad.sin_cos();
   [
      cos * transform.sx,
      sin * transform.sx,
      -sin * transform.sy,
      cos * transform.sy,
      layout_x + transform.tx,
      layout_y + transform.ty,
   ]
}

#[inline]
fn affine_point(transform: [f32; 6], point: [f32; 2]) -> [f32; 2]
{
   [
      transform[0] * point[0] + transform[2] * point[1] + transform[4],
      transform[1] * point[0] + transform[3] * point[1] + transform[5],
   ]
}

#[inline]
fn affine_inverse_point(transform: [f32; 6], point: [f32; 2]) -> Option<[f32; 2]>
{
   let determinant = transform[0] * transform[3] - transform[1] * transform[2];
   if !determinant.is_finite() || determinant.abs() <= f32::EPSILON
   {
      return None;
   }
   let x = point[0] - transform[4];
   let y = point[1] - transform[5];
   Some([
      (transform[3] * x - transform[2] * y) / determinant,
      (-transform[1] * x + transform[0] * y) / determinant,
   ])
}

fn encode_node_local_frame(style: &NodeStyle, layout: LayoutRect, b: &mut DrawListBuilder) {
    if style.shadow_alpha > 0.0 {
        b.rrect(
            gfx::RectF::new(0.0, 2.0, layout.w, layout.h),
            style.corner_radii,
            gfx::Color::rgba(0.0, 0.0, 0.0, style.shadow_alpha.clamp(0.0, 1.0)),
        );
    }
    b.rrect(gfx::RectF::new(0.0, 0.0, layout.w, layout.h), style.corner_radii, style.background);
}

fn effective_node_style(id: NodeId, mut style: NodeStyle, over: Option<&crate::anim::AnimOverrideSlots>) -> NodeStyle {
    let Some(override_) = over.and_then(|overrides| overrides.get(&id)) else {
        return style;
    };
    if let Some(radii) = override_.corner_radii {
        style.corner_radii = radii;
    }
    if let Some(color) = override_.color {
        style.background = color;
    }
    if let Some(alpha) = override_.shadow_alpha {
        style.shadow_alpha = alpha;
    }
    style
}

fn encode_node_frame(
    id: NodeId,
    style: &NodeStyle,
    layout: LayoutRect,
    b: &mut DrawListBuilder,
    over: Option<&crate::anim::AnimOverrideSlots>,
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
        let parent_and_index = self.get(id).map(|node| (node.parent, node.index_in_parent));
        let (parent, removed_index) = parent_and_index.unwrap_or((None, 0));
        if let Some(parent_id) = parent {
            if let Some(parent) = self.get_mut(parent_id) {
                if parent.children.get(removed_index) == Some(&id) {
                    parent.children.remove(removed_index);
                } else {
                    parent.children.retain(|child| *child != id);
                }
            }
            let child_count = self.get(parent_id).map_or(0, |node| node.children.len());
            for index in removed_index..child_count {
                let child = self.get(parent_id).and_then(|node| node.children.get(index).copied());
                if let Some(child) = child.and_then(|child| self.get_mut(child)) {
                    child.index_in_parent = index;
                }
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
        self.retained_cache_lru_remove(id);
        let retained_bytes = self.get(id).map(|node| (
            node.retained_chunk.as_ref().map_or(0, gfx::RenderChunk::byte_size),
            node.retained_sequence.as_ref().map_or(0, gfx::RenderChunkSequence::metadata_byte_size),
        ));
        if let Some((chunk_bytes, sequence_bytes)) = retained_bytes {
            self.retained_chunk_bytes = self.retained_chunk_bytes.saturating_sub(chunk_bytes);
            self.retained_sequence_bytes = self.retained_sequence_bytes.saturating_sub(sequence_bytes);
        }
        if let Some(slot) = self.nodes.get_mut(id.0 as usize) {
            if let Some(node) = slot.take() {
                self.dynamic_slots.release(node.transform_slot);
                self.dynamic_slots.release(node.opacity_slot);
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

    pub(crate) fn edit_style_untracked<F: FnOnce(&mut NodeStyle)>(&mut self, id: NodeId, edit: F) -> Option<(NodeStyle, NodeStyle)>
    {
       let before = self.get(id)?.style;
       edit(&mut self.get_mut(id)?.style);
       Some((before, self.get(id)?.style))
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
                    next.chunk_dirty = true;
                    next.sequence_dirty = true;
                    next.layout_dirty = true;
                    next.descendant_layout_dirty = false;
                    next.last_content = None;
                    next.retained_chunk = None;
                    next.retained_sequence = None;
                    next.retained_composition = None;
                    next.cache_lru_prev = None;
                    next.cache_lru_next = None;
                    next.cache_lru_listed = false;
                    next
                })
            })
            .collect();
        Self {
            nodes,
            root: self.root,
            retained_namespace: None,
            retained_chunk_bytes: 0,
            retained_sequence_bytes: 0,
            retained_cache_policy: self.retained_cache_policy,
            retained_cache_generation: 0,
            pending_cache_evictions: 0,
            pending_cache_evicted_bytes: 0,
            last_invalidation_reason: RetainedInvalidationReason::None,
            retained_cache_lru_head: None,
            retained_cache_lru_tail: None,
            dynamic_slots: self.dynamic_slots.clone(),
            dynamic_properties: Vec::with_capacity(self.dynamic_properties.capacity()),
        }
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
