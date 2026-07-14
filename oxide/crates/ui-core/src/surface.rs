//! Scene graph coordination utilities for Oxide surfaces.
//! Provides NodeTree management, asynchronous layout, interaction gating, and
//! scatter-style transition helpers inspired by AsyncDisplayKit flows.

use crate::{
    anim,
    capture::SurfaceCapture,
    elements::TextCtx,
    layout_async::AsyncLayoutCoordinator,
    overlay::{OverlayPointerResult, OverlayStack, PopupManager, RetainedOverlayStats},
    DrawListBuilder, LayoutRect, LayoutStats, NodeId, NodeStyle, NodeTree, RetainedCachePolicy,
    RetainedInvalidationReason, RetainedNodeStats,
};
use alloc::collections::BTreeSet;
use alloc::vec::Vec;
use core::fmt;
use std::collections::HashMap;

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

fn style_change_rebuilds_node_paint(before: &NodeStyle, after: &NodeStyle) -> bool
{
   before.background != after.background
      || before.corner_radii != after.corner_radii
      || before.shadow_alpha != after.shadow_alpha
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetainedDrawStatus {
    Rebuilt,
    Reused,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SurfaceRenderChunkStats
{
   pub status: RetainedDrawStatus,
   pub chunks_reused: u64,
   pub chunks_rebuilt: u64,
   pub sequences_reused: u64,
   pub sequences_rebuilt: u64,
   pub command_bytes_copied: u64,
   pub vertex_bytes_copied: u64,
   pub index_bytes_copied: u64,
   pub retained_bytes: u64,
   pub retained_sequence_bytes: u64,
}

pub struct SurfaceRenderSnapshot
{
   pub snapshot: gfx::RenderSnapshot,
   pub stats: SurfaceRenderChunkStats,
}

struct RetainedSnapshotCache
{
   namespace: u32,
   sequences: Vec<gfx::RenderChunkSequence>,
   snapshot: gfx::RenderSnapshot,
}

const MAX_SURFACE_DAMAGE_RECTS: usize = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SurfaceFrameDemand
{
   pub camera: bool,
   pub timer_due: bool,
   pub upload: bool,
   pub async_publication: bool,
}

impl SurfaceFrameDemand
{
   #[inline]
   #[must_use]
   pub const fn any(self) -> bool
   {
      self.camera || self.timer_due || self.upload || self.async_publication
   }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SurfaceDamageStats
{
   pub changed_paint_units: u32,
   pub layout_changes: u32,
   pub paint_changes: u32,
   pub property_only_changes: u32,
   pub resource_changes: u32,
   pub clip_changes: u32,
   pub descendant_invalidations: u32,
   pub order_changes: u32,
   pub effect_expansions: u32,
   pub damage_rects: u32,
   pub damage_pixels: u64,
   pub full_damage: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SurfacePaintKey
{
   id: gfx::RenderChunkId,
   occurrence: u32,
}

#[derive(Clone, Debug)]
struct SurfacePaintState
{
   bounds: gfx::RenderSpatialBounds,
   origin: [f32; 2],
   transform: [f32; 6],
   opacity: f32,
   resolved_clip: gfx::RenderSpatialBounds,
   source_revision: u64,
   revisions: gfx::RenderChunkRevisions,
   layer: Option<gfx::RenderLayerInstance>,
   chunk: gfx::RenderChunk,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SurfacePaintChanges
{
   changed: bool,
   layout: bool,
   paint: bool,
   property_only: bool,
   resource: bool,
   clip: bool,
   descendant: bool,
}

impl SurfacePaintChanges
{
   fn record(self, stats: &mut SurfaceDamageStats)
   {
      stats.changed_paint_units = stats.changed_paint_units.saturating_add(u32::from(self.changed));
      stats.layout_changes = stats.layout_changes.saturating_add(u32::from(self.layout));
      stats.paint_changes = stats.paint_changes.saturating_add(u32::from(self.paint));
      stats.property_only_changes = stats.property_only_changes.saturating_add(u32::from(self.property_only));
      stats.resource_changes = stats.resource_changes.saturating_add(u32::from(self.resource));
      stats.clip_changes = stats.clip_changes.saturating_add(u32::from(self.clip));
      stats.descendant_invalidations = stats.descendant_invalidations.saturating_add(u32::from(self.descendant));
   }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct SurfaceEffectDependency
{
   sample_bounds: gfx::RenderSpatialBounds,
   output_bounds: gfx::RenderSpatialBounds,
}

#[derive(Clone, Copy)]
struct SurfaceDamageRegion
{
   rects: [gfx::RectI; MAX_SURFACE_DAMAGE_RECTS],
   len: usize,
   full: bool,
   viewport: gfx::RectI,
}

impl Default for SurfaceDamageRegion
{
   fn default() -> Self
   {
      Self {
         rects: [gfx::RectI::new(0, 0, 0, 0); MAX_SURFACE_DAMAGE_RECTS],
         len: 0,
         full: false,
         viewport: gfx::RectI::new(0, 0, 0, 0),
      }
   }
}

impl SurfaceDamageRegion
{
   fn begin(&mut self, viewport: gfx::RectI)
   {
      self.len = 0;
      self.full = false;
      self.viewport = viewport;
   }

   fn is_empty(&self) -> bool
   {
      !self.full && self.len == 0
   }

   fn force_full(&mut self)
   {
      self.len = 0;
      self.full = true;
   }

   fn add_spatial(&mut self, bounds: gfx::RenderSpatialBounds)
   {
      match bounds
      {
         gfx::RenderSpatialBounds::Empty => {}
         gfx::RenderSpatialBounds::Finite(_) =>
         {
            if let Some(rect) = bounds.conservative_rect_i()
            {
               self.add(rect);
            }
         }
         gfx::RenderSpatialBounds::Unbounded => self.force_full(),
      }
   }

   fn add(&mut self, rect: gfx::RectI)
   {
      if self.full
      {
         return;
      }
      let Some(mut rect) = intersect_damage_rect(rect, self.viewport) else { return };
      let mut index = 0;
      while index < self.len
      {
         if damage_rects_touch(rect, self.rects[index])
         {
            rect = union_damage_rect(rect, self.rects[index]);
            self.len -= 1;
            self.rects[index] = self.rects[self.len];
            index = 0;
         }
         else
         {
            index += 1;
         }
      }
      if self.len == self.rects.len()
      {
         self.force_full();
         return;
      }
      self.rects[self.len] = rect;
      self.len += 1;
   }

   fn intersects(&self, bounds: gfx::RenderSpatialBounds) -> bool
   {
      if self.full
      {
         return !bounds.is_empty();
      }
      self.rects[..self.len].iter().copied().any(|rect| {
         rect_spatial_bounds(rect).intersects(bounds)
      })
   }

   fn write_into(&self, out: &mut Vec<gfx::RectI>)
   {
      out.clear();
      if self.full
      {
         if self.viewport.w > 0 && self.viewport.h > 0
         {
            out.push(self.viewport);
         }
      }
      else
      {
         out.extend_from_slice(&self.rects[..self.len]);
      }
   }

   fn finish_stats(&self, mut stats: SurfaceDamageStats, effect_expansions: u32) -> SurfaceDamageStats
   {
      stats.effect_expansions = effect_expansions;
      stats.damage_rects = if self.full { u32::from(self.viewport.w > 0 && self.viewport.h > 0) } else { self.len as u32 };
      stats.damage_pixels = if self.full
      {
         damage_rect_pixels(self.viewport)
      }
      else
      {
         self.rects[..self.len].iter().copied().fold(0_u64, |pixels, rect| {
            pixels.saturating_add(damage_rect_pixels(rect))
         })
      };
      stats.full_damage = self.full;
      stats
   }
}

fn surface_paint_changes(previous: &SurfacePaintState, next: &SurfacePaintState) -> SurfacePaintChanges
{
   let transform = previous.transform != next.transform;
   let opacity = previous.opacity != next.opacity;
   let clip = previous.resolved_clip != next.resolved_clip;
   let bounds = previous.bounds != next.bounds;
   let same_chunk = previous.chunk.ptr_eq(&next.chunk);
   let paint = previous.layer != next.layer
      || !same_chunk && previous.chunk.draw_list() != next.chunk.draw_list();
   let resource = !same_chunk
      && previous.chunk.resource_dependencies() != next.chunk.resource_dependencies();
   let layout = previous.origin != next.origin || bounds && !transform && !clip;
   let changed = transform || opacity || clip || bounds || paint || resource;
   SurfacePaintChanges {
      changed,
      layout,
      paint,
      property_only: (transform || opacity) && !layout && !paint && !resource && !clip,
      resource,
      clip,
      descendant: !changed && (!same_chunk
         || previous.revisions != next.revisions
         || previous.source_revision != next.source_revision),
   }
}

fn inserted_or_removed_paint_changes() -> SurfacePaintChanges
{
   SurfacePaintChanges {
      changed: true,
      layout: true,
      ..SurfacePaintChanges::default()
   }
}

fn surface_paint_state(resolved: &gfx::RenderResolvedInstance) -> SurfacePaintState
{
   SurfacePaintState {
      bounds: resolved.bounds,
      origin: resolved.instance.origin,
      transform: resolved.transform,
      opacity: resolved.opacity,
      resolved_clip: resolved.resolved_clip,
      source_revision: resolved.source_revision,
      revisions: resolved.instance.chunk.revisions(),
      layer: resolved.instance.layer,
      chunk: resolved.instance.chunk.clone(),
   }
}

fn surface_viewport(tree: &NodeTree) -> gfx::RectI
{
   tree.layout_rect(tree.root()).map_or(gfx::RectI::new(0, 0, 0, 0), |rect| {
      gfx::RenderSpatialBounds::Finite(gfx::RectF::new(rect.x, rect.y, rect.w, rect.h))
         .conservative_rect_i()
         .unwrap_or(gfx::RectI::new(0, 0, 0, 0))
   })
}

fn rect_spatial_bounds(rect: gfx::RectI) -> gfx::RenderSpatialBounds
{
   if rect.w <= 0 || rect.h <= 0
   {
      gfx::RenderSpatialBounds::Empty
   }
   else
   {
      gfx::RenderSpatialBounds::Finite(gfx::RectF::new(
         rect.x as f32,
         rect.y as f32,
         rect.w as f32,
         rect.h as f32,
      ))
   }
}

fn intersect_damage_rect(a: gfx::RectI, b: gfx::RectI) -> Option<gfx::RectI>
{
   let ax1 = i64::from(a.x).saturating_add(i64::from(a.w));
   let ay1 = i64::from(a.y).saturating_add(i64::from(a.h));
   let bx1 = i64::from(b.x).saturating_add(i64::from(b.w));
   let by1 = i64::from(b.y).saturating_add(i64::from(b.h));
   let x0 = i64::from(a.x).max(i64::from(b.x));
   let y0 = i64::from(a.y).max(i64::from(b.y));
   let x1 = ax1.min(bx1);
   let y1 = ay1.min(by1);
   if x1 <= x0 || y1 <= y0
   {
      return None;
   }
   Some(gfx::RectI::new(
      x0.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
      y0.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
      (x1 - x0).min(i64::from(i32::MAX)) as i32,
      (y1 - y0).min(i64::from(i32::MAX)) as i32,
   ))
}

fn damage_rects_touch(a: gfx::RectI, b: gfx::RectI) -> bool
{
   let ax1 = i64::from(a.x).saturating_add(i64::from(a.w)).saturating_add(1);
   let ay1 = i64::from(a.y).saturating_add(i64::from(a.h)).saturating_add(1);
   let bx1 = i64::from(b.x).saturating_add(i64::from(b.w)).saturating_add(1);
   let by1 = i64::from(b.y).saturating_add(i64::from(b.h)).saturating_add(1);
   i64::from(a.x) <= bx1 && i64::from(b.x) <= ax1
      && i64::from(a.y) <= by1 && i64::from(b.y) <= ay1
}

fn union_damage_rect(a: gfx::RectI, b: gfx::RectI) -> gfx::RectI
{
   let x0 = i64::from(a.x).min(i64::from(b.x));
   let y0 = i64::from(a.y).min(i64::from(b.y));
   let x1 = i64::from(a.x).saturating_add(i64::from(a.w))
      .max(i64::from(b.x).saturating_add(i64::from(b.w)));
   let y1 = i64::from(a.y).saturating_add(i64::from(a.h))
      .max(i64::from(b.y).saturating_add(i64::from(b.h)));
   gfx::RectI::new(
      x0.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
      y0.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
      (x1 - x0).min(i64::from(i32::MAX)) as i32,
      (y1 - y0).min(i64::from(i32::MAX)) as i32,
   )
}

fn damage_rect_pixels(rect: gfx::RectI) -> u64
{
   u64::try_from(rect.w.max(0)).unwrap_or(0)
      .saturating_mul(u64::try_from(rect.h.max(0)).unwrap_or(0))
}

fn surface_render_chunk_stats(status: RetainedDrawStatus, node: RetainedNodeStats) -> SurfaceRenderChunkStats
{
   SurfaceRenderChunkStats {
      status,
      chunks_reused: node.chunks_reused,
      chunks_rebuilt: node.chunks_rebuilt,
      sequences_reused: node.sequences_reused,
      sequences_rebuilt: node.sequences_rebuilt,
      command_bytes_copied: node.command_bytes_copied,
      vertex_bytes_copied: node.vertex_bytes_copied,
      index_bytes_copied: node.index_bytes_copied,
      retained_bytes: node.retained_chunk_bytes,
      retained_sequence_bytes: node.retained_sequence_bytes,
   }
}

#[derive(Debug)]
pub enum SurfaceRenderSnapshotError
{
   Chunk(gfx::RenderChunkError),
   Snapshot(gfx::RenderSnapshotError),
}

impl From<gfx::RenderChunkError> for SurfaceRenderSnapshotError
{
   fn from(error: gfx::RenderChunkError) -> Self
   {
      Self::Chunk(error)
   }
}

impl From<gfx::RenderSnapshotError> for SurfaceRenderSnapshotError
{
   fn from(error: gfx::RenderSnapshotError) -> Self
   {
      Self::Snapshot(error)
   }
}

impl fmt::Display for SurfaceRenderSnapshotError
{
   fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
   {
      match self {
         Self::Chunk(error) => write!(f, "retained surface chunk failed: {error}"),
         Self::Snapshot(error) => write!(f, "retained surface snapshot failed: {error}"),
      }
   }
}

impl std::error::Error for SurfaceRenderSnapshotError
{
   fn source(&self) -> Option<&(dyn std::error::Error + 'static)>
   {
      match self {
         Self::Chunk(error) => Some(error),
         Self::Snapshot(error) => Some(error),
      }
   }
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
    gate: InteractionGate,
    scatter: ScatterState,
    dirty: DirtySet,
    retained_snapshot: Option<RetainedSnapshotCache>,
    retained_node_stats: RetainedNodeStats,
    last_layout_stats: LayoutStats,
    frame_demand: SurfaceFrameDemand,
    paint_states: HashMap<SurfacePaintKey, SurfacePaintState>,
    paint_order: Vec<SurfacePaintKey>,
    next_paint_states: HashMap<SurfacePaintKey, SurfacePaintState>,
    next_paint_order: Vec<SurfacePaintKey>,
    paint_occurrences: HashMap<gfx::RenderChunkId, u32>,
    effect_dependencies: Vec<SurfaceEffectDependency>,
    damage_region: SurfaceDamageRegion,
    damage_stats: SurfaceDamageStats,
    force_full_damage: bool,
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
            gate: InteractionGate::default(),
            scatter: ScatterState::default(),
            dirty: DirtySet::all(),
            retained_snapshot: None,
            retained_node_stats: RetainedNodeStats::default(),
            last_layout_stats: LayoutStats::default(),
            frame_demand: SurfaceFrameDemand::default(),
            paint_states: HashMap::new(),
            paint_order: Vec::new(),
            next_paint_states: HashMap::new(),
            next_paint_order: Vec::new(),
            paint_occurrences: HashMap::new(),
            effect_dependencies: Vec::new(),
            damage_region: SurfaceDamageRegion::default(),
            damage_stats: SurfaceDamageStats::default(),
            force_full_damage: true,
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

   #[inline]
   pub fn retained_cache_policy(&self) -> RetainedCachePolicy
   {
      self.tree.retained_cache_policy()
   }

   pub fn set_retained_cache_policy(&mut self, policy: RetainedCachePolicy)
   {
      if self.tree.retained_cache_policy() == policy
      {
         return;
      }
      self.tree.set_retained_cache_policy(policy);
      self.retained_snapshot = None;
      self.retained_node_stats = RetainedNodeStats::default();
   }

   pub fn handle_memory_warning(&mut self)
   {
      let _ = self.tree.purge_retained_cache();
      self.retained_snapshot = None;
      self.retained_node_stats = RetainedNodeStats::default();
      self.paint_states.clear();
      self.paint_order.clear();
      self.next_paint_states.clear();
      self.next_paint_order.clear();
      self.paint_occurrences.clear();
      self.effect_dependencies.clear();
      self.dirty.mark(DirtyClass::Paint);
      self.force_full_damage = true;
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
   #[must_use]
   pub fn needs_frame(&self) -> bool
   {
      !self.damage_region.is_empty()
         || self.dirty.affects_draw()
         || self.animator.active_count() != 0
         || self.layout_worker.has_inflight()
         || self.frame_demand.any()
   }

   #[inline]
   #[must_use]
   pub const fn frame_demand(&self) -> SurfaceFrameDemand
   {
      self.frame_demand
   }

   #[inline]
   pub fn set_frame_demand(&mut self, demand: SurfaceFrameDemand)
   {
      self.frame_demand = demand;
   }

   #[inline]
   #[must_use]
   pub const fn damage_stats(&self) -> SurfaceDamageStats
   {
      self.damage_stats
   }

   pub fn take_damage_into(&mut self, out: &mut Vec<gfx::RectI>)
   {
      self.damage_region.write_into(out);
      self.damage_region.begin(self.damage_region.viewport);
   }

    #[inline]
    pub fn mark_dirty(&mut self, class: DirtyClass) {
        self.dirty.mark(class);
        if class == DirtyClass::Layout {
            self.tree.mark_layout_dirty(self.tree.root());
        }
        if class.bit() & DRAW_DIRTY_BITS != 0 {
            self.retained_node_stats = RetainedNodeStats::default();
            self.tree.mark_all_draw_dirty();
            self.force_full_damage = true;
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
            self.retained_node_stats = RetainedNodeStats::default();
        }
        true
    }

    pub fn edit_style<F: FnOnce(&mut NodeStyle)>(&mut self, id: NodeId, edit: F) -> bool {
        let Some((before, after)) = self.tree.edit_style_untracked(id, edit) else {
            return false;
        };
        if before == after {
            return false;
        }

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
            self.tree.mark_subtree_sequence_dirty(id);
        }
        if style_change_rebuilds_node_paint(&before, &after)
        {
            self.tree.mark_node_and_ancestors_draw_dirty(id);
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
        self.force_full_damage = true;
    }

    fn mark_scoped_tree_mutated(&mut self) {
        self.retained_node_stats = RetainedNodeStats::default();
        self.dirty.mark(DirtyClass::Style);
        self.dirty.mark(DirtyClass::Layout);
        self.dirty.mark(DirtyClass::Paint);
        self.dirty.mark(DirtyClass::Accessibility);
        self.dirty.mark(DirtyClass::HitTest);
    }

    #[inline]
    pub fn hit_test(&self, x: f32, y: f32) -> Option<(NodeId, [f32; 2])> {
        if self.animator.overrides().is_empty()
        {
           self.tree.hit_test(x, y)
        }
        else
        {
           self.tree.hit_test_with_anims(x, y, self.animator.overrides())
        }
    }

    #[inline]
    pub fn accessibility_frame(&self, id: NodeId) -> Option<gfx::RectF>
    {
       self.tree.accessibility_frame(
          id,
          (!self.animator.overrides().is_empty()).then_some(self.animator.overrides()),
       )
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
        if self.animator.overrides().is_empty() {
            self.tree.encode_draws(b);
        } else {
            self.tree.encode_draws_with_anims(b, self.animator.overrides());
        }
    }

   pub fn render_snapshot_retained(&mut self, id: gfx::RenderChunkId, content: &[gfx::RenderChunkSequence], mut properties: Vec<gfx::RenderPropertySlot>, mut damage: gfx::Damage) -> Result<SurfaceRenderSnapshot, SurfaceRenderSnapshotError>
   {
      let namespace = u32::try_from(id.0).map_err(|_| gfx::RenderChunkError::GeometryTooLarge)?;
      let (surface, node_stats) = if self.animator.overrides().is_empty() {
         self.tree.render_sequence(namespace)?
      } else {
         self.tree.render_sequence_with_anims(namespace, self.animator.overrides())?
      };
      properties.extend_from_slice(self.tree.dynamic_properties());
      properties.sort_unstable_by_key(|property| property.id.0);
      if !node_stats.cache_complete {
         self.retained_snapshot = None;
      }
      let cached = node_stats.cache_complete.then(|| self.retained_snapshot.as_ref()).flatten().filter(|cached| {
         cached.namespace == namespace
            && cached.sequences.len() == content.len().saturating_add(1)
            && cached.sequences[0].ptr_eq(&surface)
            && cached.sequences[1..].iter().zip(content).all(|(cached, current)| cached.ptr_eq(current))
            && cached.snapshot.properties() == properties
      }).map(|cached| cached.snapshot.clone());
      let base_snapshot = if let Some(snapshot) = cached {
         snapshot
      } else {
         let mut sequences = Vec::with_capacity(content.len().saturating_add(1));
         sequences.push(surface);
         sequences.extend(content.iter().cloned());
         let snapshot = gfx::RenderSnapshot::from_sequences(
            sequences.clone(),
            properties,
            gfx::Damage { rects: Vec::new() },
         )?;
         if node_stats.cache_complete {
            self.retained_snapshot = Some(RetainedSnapshotCache {
               namespace,
               sequences,
               snapshot: snapshot.clone(),
            });
         }
         snapshot
      };
      self.derive_damage(&base_snapshot, &damage.rects);
      self.damage_region.write_into(&mut damage.rects);
      let snapshot = base_snapshot.with_damage(damage);
      let status = if node_stats.chunks_rebuilt == 0 && node_stats.sequences_rebuilt == 0 {
         RetainedDrawStatus::Reused
      } else {
         RetainedDrawStatus::Rebuilt
      };
      let stats = surface_render_chunk_stats(status, node_stats);
      self.retained_node_stats = node_stats;
      self.dirty.clear_draw_affecting();
      Ok(SurfaceRenderSnapshot { snapshot, stats })
   }

   fn derive_damage(&mut self, snapshot: &gfx::RenderSnapshot, explicit: &[gfx::RectI])
   {
      let viewport = surface_viewport(&self.tree);
      self.damage_region.begin(viewport);
      for rect in explicit.iter().copied()
      {
         self.damage_region.add(rect);
      }
      if self.force_full_damage
      {
         self.damage_region.force_full();
         self.force_full_damage = false;
      }
      self.next_paint_states.clear();
      self.next_paint_order.clear();
      self.paint_occurrences.clear();
      self.effect_dependencies.clear();
      let mut unbounded_effect = false;
      snapshot.visit_resolved_instances(|resolved| {
         let id = resolved.instance.chunk.id();
         let first_key = SurfacePaintKey { id, occurrence: 0 };
         let mut state = Some(surface_paint_state(resolved));
         match self.next_paint_states.entry(first_key)
         {
            std::collections::hash_map::Entry::Vacant(entry) =>
            {
               entry.insert(state.take().expect("new paint state must exist"));
               self.next_paint_order.push(first_key);
            }
            std::collections::hash_map::Entry::Occupied(_) =>
            {
               let occurrence = self.paint_occurrences.entry(id).or_insert(1);
               let key = SurfacePaintKey { id, occurrence: *occurrence };
               *occurrence = occurrence.saturating_add(1);
               self.next_paint_states.insert(key, state.take().expect("duplicate paint state must exist"));
               self.next_paint_order.push(key);
            }
         }
         for effect in resolved.instance.chunk.effect_dependencies()
         {
            let sample_bounds = effect.sample_bounds
               .transformed(resolved.transform)
               .intersect(resolved.resolved_clip);
            let output_bounds = effect.output_bounds
               .transformed(resolved.transform)
               .intersect(resolved.resolved_clip);
            unbounded_effect |= sample_bounds.is_unbounded() || output_bounds.is_unbounded();
            self.effect_dependencies.push(SurfaceEffectDependency {
               sample_bounds,
               output_bounds,
            });
         }
      });
      let mut stats = SurfaceDamageStats::default();
      for key in self.next_paint_order.iter().copied()
      {
         let next = self.next_paint_states.get(&key).expect("ordered paint state must exist");
         match self.paint_states.get(&key)
         {
            Some(previous) =>
            {
               let changes = surface_paint_changes(previous, next);
               changes.record(&mut stats);
               if changes.changed
               {
                  self.damage_region.add_spatial(previous.bounds.union(next.bounds));
               }
            }
            None =>
            {
               inserted_or_removed_paint_changes().record(&mut stats);
               self.damage_region.add_spatial(next.bounds);
            }
         }
      }
      for key in self.paint_order.iter().copied()
      {
         if !self.next_paint_states.contains_key(&key)
         {
            let previous = self.paint_states.get(&key).expect("ordered paint state must exist");
            inserted_or_removed_paint_changes().record(&mut stats);
            self.damage_region.add_spatial(previous.bounds);
         }
      }
      let order_len = self.paint_order.len().max(self.next_paint_order.len());
      for index in 0..order_len
      {
         let previous = self.paint_order.get(index).copied();
         let next = self.next_paint_order.get(index).copied();
         if previous == next
         {
            continue;
         }
         stats.order_changes = stats.order_changes.saturating_add(1);
         if let Some(previous) = previous.and_then(|key| self.paint_states.get(&key))
         {
            self.damage_region.add_spatial(previous.bounds);
         }
         if let Some(next) = next.and_then(|key| self.next_paint_states.get(&key))
         {
            self.damage_region.add_spatial(next.bounds);
         }
      }
      let mut effect_expansions = 0_u32;
      if unbounded_effect && !self.damage_region.is_empty()
      {
         self.damage_region.force_full();
      }
      else
      {
         for effect in &self.effect_dependencies
         {
            if self.damage_region.intersects(effect.sample_bounds)
            {
               self.damage_region.add_spatial(effect.output_bounds);
               effect_expansions = effect_expansions.saturating_add(1);
            }
         }
      }
      core::mem::swap(&mut self.paint_states, &mut self.next_paint_states);
      core::mem::swap(&mut self.paint_order, &mut self.next_paint_order);
      self.next_paint_states.clear();
      self.next_paint_order.clear();
      self.damage_stats = self.damage_region.finish_stats(stats, effect_expansions);
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
        _text_atlases: Option<&[(gfx::ImageHandle, u64)]>,
    ) -> RetainedDrawStatus {
        let retained = if self.animator.overrides().is_empty() {
            self.tree.render_sequence(0)
        } else {
            self.tree.render_sequence_with_anims(0, self.animator.overrides())
        };
        let Ok((sequence, mut node_stats)) = retained else {
            self.encode(b);
            self.retained_node_stats = RetainedNodeStats {
                flat_fallback_uses: 1,
                ..RetainedNodeStats::default()
            };
            return RetainedDrawStatus::Rebuilt;
        };
        let status = if node_stats.chunks_rebuilt == 0 && node_stats.sequences_rebuilt == 0 {
            RetainedDrawStatus::Reused
        } else {
            RetainedDrawStatus::Rebuilt
        };
        let snapshot = gfx::RenderSnapshot::from_sequences(
            vec![sequence],
            self.tree.dynamic_properties().to_vec(),
            gfx::Damage { rects: Vec::new() },
        );
        if let Ok(snapshot) = snapshot {
            let _ = b.append_render_snapshot_flat(&snapshot);
            node_stats.flat_fallback_uses = node_stats.flat_fallback_uses.saturating_add(1);
        } else {
            self.encode(b);
            node_stats.flat_fallback_uses = node_stats.flat_fallback_uses.saturating_add(1);
        }
        self.dirty.clear_draw_affecting();
        self.retained_node_stats = node_stats;
        status
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
    pub fn overrides(&self) -> &anim::AnimOverrideSlots {
        self.animator.overrides()
    }

    #[inline]
    pub fn overrides_mut(&mut self) -> &mut anim::AnimOverrideSlots {
        self.tree.mark_all_draw_dirty_for(RetainedInvalidationReason::Animation);
        self.dirty.mark(DirtyClass::Transform);
        self.dirty.mark(DirtyClass::Opacity);
        self.dirty.mark(DirtyClass::Paint);
        self.animator.overrides_mut()
    }

    pub fn tick(&mut self) -> bool {
        let now = timing::now_ms();
        self.tick_at(now)
    }

    pub fn tick_at(&mut self, now_ms: u64) -> bool {
        self.animator.step(now_ms);
        let changed = !self.animator.overrides().changed_nodes().is_empty();
        if changed {
            self.dirty.mark(DirtyClass::Transform);
            self.dirty.mark(DirtyClass::Opacity);
            self.dirty.mark(DirtyClass::Accessibility);
            self.dirty.mark(DirtyClass::HitTest);
            for node in self.animator.overrides().paint_changed_nodes().iter().copied()
            {
               self.tree.mark_node_and_ancestors_draw_dirty_for(
                  node,
                  RetainedInvalidationReason::Animation,
               );
               self.dirty.mark(DirtyClass::Paint);
            }
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
