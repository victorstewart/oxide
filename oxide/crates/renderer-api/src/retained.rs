use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;
use core::mem;

use super::{
   Color, Damage, DrawCmd, DrawList, GlyphRun, ImageHandle, IndexSpan, RectF, RectI, Vertex,
   VertexSpan, VisualEffect,
};

/// Stable caller-owned identity for one independently retained render unit.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenderChunkId(pub u64);

/// Independent change domains for one retained chunk.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenderChunkRevisions
{
   pub structural: u64,
   pub geometry: u64,
   pub resource: u64,
   pub dynamic_properties: u64,
}

/// Declares how every index in the source draw list addresses its command's vertex span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChunkIndexMode
{
   Local,
   Absolute,
}

/// Resource generation captured when a chunk was built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenderResourceDependency
{
   pub image: ImageHandle,
   pub generation: u64,
}

/// Ordering facts proven while the chunk is created.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenderChunkOrdering
{
   pub max_clip_depth: u32,
   pub max_layer_depth: u32,
   pub has_clip: bool,
   pub has_layer: bool,
}

/// Conservative spatial state for retained paint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RenderSpatialBounds
{
   Empty,
   Finite(RectF),
   Unbounded,
}

impl RenderSpatialBounds
{
   #[inline]
   #[must_use]
   pub const fn rect(self) -> Option<RectF>
   {
      match self
      {
         Self::Finite(rect) => Some(rect),
         Self::Empty | Self::Unbounded => None,
      }
   }

   #[inline]
   #[must_use]
   pub const fn is_empty(self) -> bool
   {
      matches!(self, Self::Empty)
   }

   #[inline]
   #[must_use]
   pub const fn is_unbounded(self) -> bool
   {
      matches!(self, Self::Unbounded)
   }

   #[must_use]
   pub fn conservative_rect_i(self) -> Option<RectI>
   {
      let Self::Finite(rect) = self else { return None };
      let x0 = rect.x.floor();
      let y0 = rect.y.floor();
      let x1 = (rect.x + rect.w).ceil();
      let y1 = (rect.y + rect.h).ceil();
      Some(RectI::new(
         x0 as i32,
         y0 as i32,
         (x1 - x0) as i32,
         (y1 - y0) as i32,
      ))
   }
}

/// Immutable spatial facts for one canonical chunk command.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderCommandSpatial
{
   pub bounds: RenderSpatialBounds,
   pub resolved_clip: RenderSpatialBounds,
   pub matching_scope: Option<u32>,
   pub vertex_count: u32,
}

/// One top-level paint unit. A layer span includes its matching `LayerEnd`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderPaintSpan
{
   pub begin: u32,
   pub end: u32,
   pub bounds: RenderSpatialBounds,
   pub vertex_count: u32,
}

/// Work performed by one compact spatial-index query.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RenderSpatialQueryStats
{
   pub entries_visited: u64,
   pub entries_matched: u64,
}

#[derive(Debug, Clone, Copy)]
struct SpatialIndexEntry
{
   order: u32,
   bounds: RectF,
}

#[derive(Debug, Default)]
struct SpatialIndex
{
   by_min_x: Arc<[SpatialIndexEntry]>,
   prefix_max_x: Arc<[f32]>,
   unbounded: Arc<[u32]>,
}

impl SpatialIndex
{
   fn new(bounds: impl IntoIterator<Item = (u32, RenderSpatialBounds)>) -> Self
   {
      let mut by_min_x = Vec::new();
      let mut unbounded = Vec::new();
      for (order, bounds) in bounds
      {
         match bounds
         {
            RenderSpatialBounds::Finite(rect) if rect.w > 0.0 && rect.h > 0.0 =>
            {
               by_min_x.push(SpatialIndexEntry { order, bounds: rect });
            }
            RenderSpatialBounds::Unbounded => unbounded.push(order),
            RenderSpatialBounds::Empty | RenderSpatialBounds::Finite(_) => {}
         }
      }
      by_min_x.sort_unstable_by(|a, b| {
         a.bounds.x.total_cmp(&b.bounds.x).then_with(|| a.order.cmp(&b.order))
      });
      let mut max_x = f32::NEG_INFINITY;
      let prefix_max_x = by_min_x.iter().map(|entry| {
         max_x = max_x.max(entry.bounds.x + entry.bounds.w);
         max_x
      }).collect::<Vec<_>>();
      Self {
         by_min_x: by_min_x.into(),
         prefix_max_x: prefix_max_x.into(),
         unbounded: unbounded.into(),
      }
   }

   fn byte_size(&self) -> u64
   {
      (self.by_min_x.len() as u64)
         .saturating_mul(mem::size_of::<SpatialIndexEntry>() as u64)
         .saturating_add(
            (self.prefix_max_x.len() as u64).saturating_mul(mem::size_of::<f32>() as u64),
         )
         .saturating_add(
            (self.unbounded.len() as u64).saturating_mul(mem::size_of::<u32>() as u64),
         )
   }

   fn query(&self, rect: RectF, out: &mut Vec<u32>) -> RenderSpatialQueryStats
   {
      out.clear();
      let Some(rect) = finite_rect(rect) else
      {
         out.extend(self.by_min_x.iter().map(|entry| entry.order));
         out.extend_from_slice(&self.unbounded);
         out.sort_unstable();
         out.dedup();
         return RenderSpatialQueryStats {
            entries_visited: (self.by_min_x.len() + self.unbounded.len()) as u64,
            entries_matched: out.len() as u64,
         };
      };
      if rect.w <= 0.0 || rect.h <= 0.0
      {
         return RenderSpatialQueryStats::default();
      }
      let x1 = rect.x + rect.w;
      let upper = self.by_min_x.partition_point(|entry| entry.bounds.x < x1);
      let lower = self.prefix_max_x[..upper].partition_point(|max_x| *max_x <= rect.x);
      let mut visited = 0_u64;
      for entry in &self.by_min_x[lower..upper]
      {
         visited = visited.saturating_add(1);
         if rects_intersect(entry.bounds, rect)
         {
            out.push(entry.order);
         }
      }
      visited = visited.saturating_add(self.unbounded.len() as u64);
      out.extend_from_slice(&self.unbounded);
      out.sort_unstable();
      out.dedup();
      RenderSpatialQueryStats {
         entries_visited: visited,
         entries_matched: out.len() as u64,
      }
   }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderChunkError
{
   VertexSpanOutOfBounds { command: usize },
   IndexSpanOutOfBounds { command: usize },
   IndexOutsideVertexSpan { command: usize, index: u16 },
   ClipUnderflow { command: usize },
   LayerUnderflow { command: usize },
   OrderingMismatch { command: usize },
   UnbalancedClipStack,
   UnbalancedLayerStack,
   MissingResourceGeneration(ImageHandle),
   ConflictingResourceGeneration(ImageHandle),
   UnusedResourceGeneration(ImageHandle),
   GeometryTooLarge,
}

impl fmt::Display for RenderChunkError
{
   fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
   {
      match self {
         Self::VertexSpanOutOfBounds { command } => write!(f, "vertex span is out of bounds at command {command}"),
         Self::IndexSpanOutOfBounds { command } => write!(f, "index span is out of bounds at command {command}"),
         Self::IndexOutsideVertexSpan { command, index } => write!(f, "index {index} is outside the vertex span at command {command}"),
         Self::ClipUnderflow { command } => write!(f, "clip stack underflow at command {command}"),
         Self::LayerUnderflow { command } => write!(f, "layer stack underflow at command {command}"),
         Self::OrderingMismatch { command } => write!(f, "clip and layer scopes cross at command {command}"),
         Self::UnbalancedClipStack => write!(f, "unbalanced clip stack"),
         Self::UnbalancedLayerStack => write!(f, "unbalanced layer stack"),
         Self::MissingResourceGeneration(image) => write!(f, "missing generation for image {}", image.0),
         Self::ConflictingResourceGeneration(image) => write!(f, "conflicting generations for image {}", image.0),
         Self::UnusedResourceGeneration(image) => write!(f, "unused generation for image {}", image.0),
         Self::GeometryTooLarge => write!(f, "chunk geometry exceeds renderer span limits"),
      }
   }
}

impl std::error::Error for RenderChunkError {}

#[derive(Debug)]
struct RenderChunkData
{
   id: RenderChunkId,
   revisions: RenderChunkRevisions,
   list: DrawList,
   bounds: RenderSpatialBounds,
   commands: Arc<[RenderCommandSpatial]>,
   paint_spans: Arc<[RenderPaintSpan]>,
   spatial_index: SpatialIndex,
   resources: Arc<[RenderResourceDependency]>,
   ordering: RenderChunkOrdering,
   byte_size: u64,
}

/// Immutable packed commands and canonical local indices for one retained render unit.
#[derive(Debug, Clone)]
pub struct RenderChunk
{
   inner: Arc<RenderChunkData>,
}

impl RenderChunk
{
   pub fn new(id: RenderChunkId, revisions: RenderChunkRevisions, source: DrawList, index_mode: ChunkIndexMode, resource_dependencies: &[RenderResourceDependency]) -> Result<Self, RenderChunkError>
   {
      let (list, ordering) = canonicalize_draw_list(&source, index_mode)?;
      let resources = validate_resource_dependencies(&list, resource_dependencies)?;
      let (bounds, commands, paint_spans, spatial_index) = prepare_spatial_metadata(&list)?;
      let byte_size = retained_byte_size(
         &list,
         resources.len(),
         commands.len(),
         paint_spans.len(),
         spatial_index.byte_size(),
      )?;
      Ok(Self {
         inner: Arc::new(RenderChunkData {
            id,
            revisions,
            list,
            bounds,
            commands: commands.into(),
            paint_spans: paint_spans.into(),
            spatial_index,
            resources: resources.into(),
            ordering,
            byte_size,
         }),
      })
   }

   #[inline]
   #[must_use]
   pub fn id(&self) -> RenderChunkId
   {
      self.inner.id
   }

   #[inline]
   #[must_use]
   pub fn revisions(&self) -> RenderChunkRevisions
   {
      self.inner.revisions
   }

   #[inline]
   #[must_use]
   pub fn draw_list(&self) -> &DrawList
   {
      &self.inner.list
   }

   #[inline]
   #[must_use]
   pub fn bounds(&self) -> Option<RectF>
   {
      self.inner.bounds.rect()
   }

   #[inline]
   #[must_use]
   pub fn spatial_bounds(&self) -> RenderSpatialBounds
   {
      self.inner.bounds
   }

   #[inline]
   #[must_use]
   pub fn command_spatial(&self) -> &[RenderCommandSpatial]
   {
      &self.inner.commands
   }

   #[inline]
   #[must_use]
   pub fn paint_spans(&self) -> &[RenderPaintSpan]
   {
      &self.inner.paint_spans
   }

   pub fn query_damage_commands(&self, rect: RectF, out: &mut Vec<u32>) -> RenderSpatialQueryStats
   {
      let stats = self.inner.spatial_index.query(rect, out);
      for command in out.iter_mut()
      {
         *command = self.inner.paint_spans[*command as usize].begin;
      }
      stats
   }

   #[inline]
   #[must_use]
   pub fn resource_dependencies(&self) -> &[RenderResourceDependency]
   {
      &self.inner.resources
   }

   #[inline]
   #[must_use]
   pub fn ordering(&self) -> RenderChunkOrdering
   {
      self.inner.ordering
   }

   #[inline]
   #[must_use]
   pub fn byte_size(&self) -> u64
   {
      self.inner.byte_size
   }

   #[must_use]
   pub fn resources_compatible(&self, resources: &[(ImageHandle, u64)]) -> bool
   {
      self.inner.resources.iter().all(|dependency| {
         resources.iter().any(|(image, generation)| {
            *image == dependency.image && *generation == dependency.generation
         })
      })
   }

   #[inline]
   #[must_use]
   pub fn ptr_eq(&self, other: &Self) -> bool
   {
      Arc::ptr_eq(&self.inner, &other.inner)
   }

}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenderPropertySlotId(pub u32);

impl RenderPropertySlotId
{
   const DYNAMIC_BIT: u32 = 1 << 31;
   const GENERATION_BITS: u32 = 11;
   const GENERATION_MASK: u32 = (1 << Self::GENERATION_BITS) - 1;
   const INDEX_MASK: u32 = (1 << (31 - Self::GENERATION_BITS)) - 1;

   #[must_use]
   pub fn dynamic(index: u32, generation: u32) -> Option<Self>
   {
      if index == 0 || index > Self::INDEX_MASK || generation == 0 || generation > Self::GENERATION_MASK
      {
         return None;
      }
      Some(Self(Self::DYNAMIC_BIT | index << Self::GENERATION_BITS | generation))
   }

   #[inline]
   #[must_use]
   pub const fn is_dynamic(self) -> bool
   {
      self.0 & Self::DYNAMIC_BIT != 0
   }

   #[inline]
   #[must_use]
   pub const fn dynamic_index(self) -> Option<u32>
   {
      if self.is_dynamic()
      {
         Some((self.0 >> Self::GENERATION_BITS) & Self::INDEX_MASK)
      }
      else
      {
         None
      }
   }

   #[inline]
   #[must_use]
   pub const fn dynamic_generation(self) -> Option<u32>
   {
      if self.is_dynamic()
      {
         Some(self.0 & Self::GENERATION_MASK)
      }
      else
      {
         None
      }
   }
}

/// Dynamic data referenced by chunk instances without modifying immutable commands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RenderPropertyValue
{
   /// Column-major 2D affine transform: `[m11, m12, m21, m22, tx, ty]`.
   Transform([f32; 6]),
   Opacity(f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderPropertySlot
{
   pub id: RenderPropertySlotId,
   pub revision: u64,
   pub value: RenderPropertyValue,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderLayerInstance
{
   pub id: u32,
   pub rect: RectF,
   pub dirty: bool,
}

/// One local clip whose affine transform is supplied by a dynamic property slot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderDynamicClip
{
   pub rect: RectF,
   pub transform: RenderPropertySlotId,
}

/// One ordered use of an immutable chunk in a render snapshot.
#[derive(Debug, Clone)]
pub struct RenderChunkInstance
{
   pub chunk: RenderChunk,
   pub origin: [f32; 2],
   pub property_slots: Arc<[RenderPropertySlotId]>,
   pub dynamic_clips: Arc<[RenderDynamicClip]>,
   pub clip: Option<RectI>,
   pub layer: Option<RenderLayerInstance>,
}

impl RenderChunkInstance
{
   #[must_use]
   pub fn new(chunk: RenderChunk, origin: [f32; 2]) -> Self
   {
      Self {
         chunk,
         origin,
         property_slots: Arc::from([]),
         dynamic_clips: Arc::from([]),
         clip: None,
         layer: None,
      }
   }
}

/// Immutable ordered lightweight chunk references that can be shared by snapshots.
#[derive(Debug)]
struct RenderChunkSequenceData
{
   instances: Arc<[RenderChunkInstance]>,
   child_blocks: Arc<[Arc<[RenderChunkSequenceChild]>]>,
   child_count: usize,
   property_slots: Arc<[RenderPropertySlotId]>,
   invalid_origin: Option<RenderChunkId>,
   origin_bounds: Option<[f32; 4]>,
   instance_count: u64,
}

#[derive(Debug, Clone)]
struct RenderChunkSequenceChild
{
   sequence: RenderChunkSequence,
   translation: [f32; 2],
   clip: Option<RectI>,
}

const RENDER_SEQUENCE_CHILD_BLOCK: usize = 8;

#[derive(Debug, Clone)]
pub struct RenderChunkSequence
{
   inner: Arc<RenderChunkSequenceData>,
}

impl RenderChunkSequence
{
   #[inline]
   #[must_use]
   pub fn new(instances: Vec<RenderChunkInstance>) -> Self
   {
      Self::compose(instances, Vec::new())
   }

   #[must_use]
   pub fn compose(instances: Vec<RenderChunkInstance>, children: Vec<(Self, [f32; 2], Option<RectI>)>) -> Self
   {
      let mut property_slots = Vec::new();
      let mut invalid_origin = None;
      let mut origin_bounds = None;
      for instance in &instances
      {
         if invalid_origin.is_none() && !instance.origin.iter().all(|value| value.is_finite())
         {
            invalid_origin = Some(instance.chunk.id());
         }
         if invalid_origin.is_none()
         {
            extend_origin_bounds(&mut origin_bounds, instance.origin);
         }
         for slot in instance.property_slots.iter().copied()
         {
            if !property_slots.contains(&slot)
            {
               property_slots.push(slot);
            }
         }
         for clip in instance.dynamic_clips.iter()
         {
            if !property_slots.contains(&clip.transform)
            {
               property_slots.push(clip.transform);
            }
         }
      }
      let children = children.into_iter().map(|(sequence, translation, clip)| {
         if invalid_origin.is_none() && !translation.iter().all(|value| value.is_finite())
         {
            invalid_origin = sequence.first_chunk_id();
         }
         if invalid_origin.is_none()
         {
            invalid_origin = sequence.inner.invalid_origin;
         }
         if invalid_origin.is_none()
         {
            if let Some(bounds) = sequence.inner.origin_bounds
            {
               let min = [bounds[0] + translation[0], bounds[1] + translation[1]];
               let max = [bounds[2] + translation[0], bounds[3] + translation[1]];
               if min.iter().chain(max.iter()).all(|value| value.is_finite())
               {
                  extend_origin_bounds(&mut origin_bounds, min);
                  extend_origin_bounds(&mut origin_bounds, max);
               }
               else
               {
                  invalid_origin = sequence.first_chunk_id();
               }
            }
         }
         for slot in sequence.inner.property_slots.iter().copied()
         {
            if !property_slots.contains(&slot)
            {
               property_slots.push(slot);
            }
         }
         RenderChunkSequenceChild { sequence, translation, clip }
      }).collect::<Vec<_>>();
      let instance_count = children.iter().fold(instances.len() as u64, |count, child| {
         count.saturating_add(child.sequence.instance_count())
      });
      let child_count = children.len();
      let child_blocks = children.chunks(RENDER_SEQUENCE_CHILD_BLOCK)
         .map(Arc::<[RenderChunkSequenceChild]>::from)
         .collect::<Vec<_>>();
      Self {
         inner: Arc::new(RenderChunkSequenceData {
            instances: instances.into(),
            child_blocks: child_blocks.into(),
            child_count,
            property_slots: property_slots.into(),
            invalid_origin,
            origin_bounds,
            instance_count,
         }),
      }
   }

   #[inline]
   #[must_use]
   pub fn direct_instances(&self) -> &[RenderChunkInstance]
   {
      &self.inner.instances
   }

   #[inline]
   #[must_use]
   pub fn instance_count(&self) -> u64
   {
      self.inner.instance_count
   }

   #[inline]
   #[must_use]
   pub fn metadata_byte_size(&self) -> u64
   {
      (self.inner.instances.len() as u64)
         .saturating_mul(mem::size_of::<RenderChunkInstance>() as u64)
         .saturating_add(
            self.inner.instances.iter().fold(0_u64, |bytes, instance| {
               bytes.saturating_add(
                  (instance.dynamic_clips.len() as u64)
                     .saturating_mul(mem::size_of::<RenderDynamicClip>() as u64),
               )
            }),
         )
         .saturating_add(
            (self.inner.property_slots.len() as u64)
               .saturating_mul(mem::size_of::<RenderPropertySlotId>() as u64),
         )
         .saturating_add(
            (self.inner.child_count as u64)
               .saturating_mul(mem::size_of::<RenderChunkSequenceChild>() as u64),
         )
         .saturating_add(
            (self.inner.child_blocks.len() as u64)
               .saturating_mul(mem::size_of::<Arc<[RenderChunkSequenceChild]>>() as u64),
         )
   }

   #[inline]
   #[must_use]
   pub fn ptr_eq(&self, other: &Self) -> bool
   {
      Arc::ptr_eq(&self.inner, &other.inner)
   }

   #[inline]
   #[must_use]
   pub fn child_count(&self) -> usize
   {
      self.inner.child_count
   }

   #[must_use]
   pub fn replacing_direct_instance(&self, index: usize, instance: RenderChunkInstance) -> Option<Self>
   {
      let current = self.inner.instances.get(index)?;
      if current.origin == instance.origin
         && current.property_slots == instance.property_slots
         && current.dynamic_clips == instance.dynamic_clips
         && instance.origin.iter().all(|value| value.is_finite())
      {
         let mut instances = self.inner.instances.to_vec();
         instances[index] = instance;
         return Some(Self {
            inner: Arc::new(RenderChunkSequenceData {
               instances: instances.into(),
               child_blocks: self.inner.child_blocks.clone(),
               child_count: self.inner.child_count,
               property_slots: self.inner.property_slots.clone(),
               invalid_origin: self.inner.invalid_origin,
               origin_bounds: self.inner.origin_bounds,
               instance_count: self.inner.instance_count,
            }),
         });
      }
      let mut instances = self.inner.instances.to_vec();
      *instances.get_mut(index)? = instance;
      let children = self.inner.child_blocks.iter().flat_map(|block| block.iter()).map(|child| (
         child.sequence.clone(),
         child.translation,
         child.clip,
      )).collect();
      Some(Self::compose(instances, children))
   }

   #[must_use]
   pub fn replacing_child(&self, index: usize, sequence: Self) -> Option<Self>
   {
      let block_index = index / RENDER_SEQUENCE_CHILD_BLOCK;
      let child_index = index % RENDER_SEQUENCE_CHILD_BLOCK;
      let child = self.inner.child_blocks.get(block_index)?.get(child_index)?;
      let old_count = child.sequence.instance_count();
      let mut child_blocks = self.inner.child_blocks.to_vec();
      let mut block = child_blocks[block_index].to_vec();
      block[child_index].sequence = sequence.clone();
      child_blocks[block_index] = block.into();
      let property_slots_unchanged = sequence.inner.property_slots == child.sequence.inner.property_slots;
      if property_slots_unchanged
         && self.inner.invalid_origin.is_none()
         && sequence.inner.invalid_origin.is_none()
         && sequence.inner.origin_bounds == child.sequence.inner.origin_bounds
      {
         return Some(Self {
            inner: Arc::new(RenderChunkSequenceData {
               instances: self.inner.instances.clone(),
               child_blocks: child_blocks.into(),
               child_count: self.inner.child_count,
               property_slots: self.inner.property_slots.clone(),
               invalid_origin: None,
               origin_bounds: self.inner.origin_bounds,
               instance_count: self.inner.instance_count
                  .saturating_sub(old_count)
                  .saturating_add(sequence.instance_count()),
            }),
         });
      }
      let children = child_blocks.into_iter().flat_map(|block| {
         block.iter().cloned().collect::<Vec<_>>()
      }).map(|child| {
         (child.sequence, child.translation, child.clip)
      }).collect();
      Some(Self::compose(self.inner.instances.to_vec(), children))
   }

   pub fn visit_instances<F: FnMut(&RenderChunkInstance)>(&self, mut visit: F)
   {
      visit_sequence_instances(self, [0.0, 0.0], None, &mut visit);
   }

   #[must_use]
   pub fn instance(&self, index: u64) -> Option<RenderChunkInstance>
   {
      sequence_instance(self, index, [0.0, 0.0], None)
   }

   fn first_chunk_id(&self) -> Option<RenderChunkId>
   {
      self.inner.instances.first().map(|instance| instance.chunk.id()).or_else(|| {
         self.inner.child_blocks.first().and_then(|block| block.first())
            .and_then(|child| child.sequence.first_chunk_id())
      })
   }
}

fn extend_origin_bounds(bounds: &mut Option<[f32; 4]>, origin: [f32; 2])
{
   if let Some(bounds) = bounds.as_mut()
   {
      bounds[0] = bounds[0].min(origin[0]);
      bounds[1] = bounds[1].min(origin[1]);
      bounds[2] = bounds[2].max(origin[0]);
      bounds[3] = bounds[3].max(origin[1]);
   }
   else
   {
      *bounds = Some([origin[0], origin[1], origin[0], origin[1]]);
   }
}

fn visit_sequence_instances<F: FnMut(&RenderChunkInstance)>(sequence: &RenderChunkSequence, translation: [f32; 2], clip: Option<RectI>, visit: &mut F)
{
   for instance in sequence.inner.instances.iter()
   {
      if translation == [0.0, 0.0] && clip.is_none()
      {
         visit(instance);
      }
      else
      {
         let transformed = transformed_instance(instance, translation, clip);
         visit(&transformed);
      }
   }
   for block in sequence.inner.child_blocks.iter()
   {
      for child in block.iter()
      {
         let child_clip = child.clip.map(|rect| translate_rect_i(rect, translation));
         let next_clip = intersect_optional_clip(clip, child_clip);
         let next_translation = [
            translation[0] + child.translation[0],
            translation[1] + child.translation[1],
         ];
         visit_sequence_instances(&child.sequence, next_translation, next_clip, visit);
      }
   }
}

fn sequence_instance(sequence: &RenderChunkSequence, mut index: u64, translation: [f32; 2], clip: Option<RectI>) -> Option<RenderChunkInstance>
{
   if index < sequence.inner.instances.len() as u64
   {
      return sequence.inner.instances.get(index as usize)
         .map(|instance| transformed_instance(instance, translation, clip));
   }
   index -= sequence.inner.instances.len() as u64;
   for block in sequence.inner.child_blocks.iter()
   {
      for child in block.iter()
      {
         if index < child.sequence.instance_count()
         {
            let child_clip = child.clip.map(|rect| translate_rect_i(rect, translation));
            let next_clip = intersect_optional_clip(clip, child_clip);
            let next_translation = [
               translation[0] + child.translation[0],
               translation[1] + child.translation[1],
            ];
            return sequence_instance(&child.sequence, index, next_translation, next_clip);
         }
         index -= child.sequence.instance_count();
      }
   }
   None
}

fn transformed_instance(instance: &RenderChunkInstance, translation: [f32; 2], clip: Option<RectI>) -> RenderChunkInstance
{
   let mut transformed = instance.clone();
   transformed.origin[0] += translation[0];
   transformed.origin[1] += translation[1];
   let instance_clip = instance.clip.map(|rect| translate_rect_i(rect, translation));
   transformed.clip = intersect_optional_clip(clip, instance_clip);
   if let Some(layer) = transformed.layer.as_mut()
   {
      layer.rect = translate_rect(layer.rect, translation);
   }
   transformed
}

fn intersect_optional_clip(a: Option<RectI>, b: Option<RectI>) -> Option<RectI>
{
   match (a, b) {
      (Some(a), Some(b)) => Some(intersect_rect_i(a, b)),
      (Some(a), None) => Some(a),
      (None, Some(b)) => Some(b),
      (None, None) => None,
   }
}

fn intersect_rect_i(a: RectI, b: RectI) -> RectI
{
   let x0 = i64::from(a.x).max(i64::from(b.x));
   let y0 = i64::from(a.y).max(i64::from(b.y));
   let x1 = (i64::from(a.x) + i64::from(a.w)).min(i64::from(b.x) + i64::from(b.w));
   let y1 = (i64::from(a.y) + i64::from(a.h)).min(i64::from(b.y) + i64::from(b.h));
   RectI::new(
      x0.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
      y0.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
      (x1 - x0).clamp(0, i64::from(i32::MAX)) as i32,
      (y1 - y0).clamp(0, i64::from(i32::MAX)) as i32,
   )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderSnapshotError
{
   DuplicatePropertySlot(RenderPropertySlotId),
   ConflictingPropertyGeneration { index: u32 },
   MissingPropertySlot(RenderPropertySlotId),
   NonFiniteProperty(RenderPropertySlotId),
   InvalidOpacity(RenderPropertySlotId),
   InvalidClipProperty(RenderPropertySlotId),
   NonFiniteOrigin(RenderChunkId),
   UnsupportedFlatTransform(RenderPropertySlotId),
   GeometryTooLarge,
}

impl fmt::Display for RenderSnapshotError
{
   fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
   {
      match self {
         Self::DuplicatePropertySlot(id) => write!(f, "duplicate render property slot {}", id.0),
         Self::ConflictingPropertyGeneration { index } => write!(f, "dynamic render property index {index} has multiple live generations"),
         Self::MissingPropertySlot(id) => write!(f, "missing render property slot {}", id.0),
         Self::NonFiniteProperty(id) => write!(f, "non-finite render property slot {}", id.0),
         Self::InvalidOpacity(id) => write!(f, "opacity slot {} is outside 0...1", id.0),
         Self::InvalidClipProperty(id) => write!(f, "dynamic clip slot {} is not a transform", id.0),
         Self::NonFiniteOrigin(id) => write!(f, "chunk {} has a non-finite origin", id.0),
         Self::UnsupportedFlatTransform(id) => write!(f, "flat fallback cannot preserve transform slot {}", id.0),
         Self::GeometryTooLarge => write!(f, "flattened snapshot exceeds renderer span limits"),
      }
   }
}

impl std::error::Error for RenderSnapshotError {}

/// One ordered snapshot instance with frame properties and clips resolved once.
#[derive(Debug, Clone)]
pub struct RenderResolvedInstance
{
   pub instance: RenderChunkInstance,
   /// Column-major local-to-world affine transform.
   pub transform: [f32; 6],
   pub opacity: f32,
   pub source_revision: u64,
   pub resolved_clip: RenderSpatialBounds,
   pub bounds: RenderSpatialBounds,
}

#[derive(Debug)]
struct RenderSnapshotData
{
   sequences: Arc<[RenderChunkSequence]>,
   resolved_spatial: Option<Arc<RenderResolvedSpatialData>>,
   properties: Arc<[RenderPropertySlot]>,
   damage: Damage,
   instance_count: u64,
   uniform_property_revision: Option<u64>,
   metadata_byte_size: u64,
}

#[derive(Debug)]
struct RenderResolvedSpatialData
{
   instances: Arc<[RenderResolvedInstance]>,
   spatial_index: SpatialIndex,
   byte_size: u64,
}

const SNAPSHOT_SPATIAL_INDEX_MIN_INSTANCES: u64 = 32;

/// Ordered immutable chunk references plus frame-local property and damage metadata.
#[derive(Debug, Clone)]
pub struct RenderSnapshot
{
   inner: Arc<RenderSnapshotData>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RenderFallbackStats
{
   pub fallback_count: u64,
   pub chunks_flattened: u64,
   pub commands_copied: u64,
   pub vertices_copied: u64,
   pub indices_copied: u64,
   pub command_bytes_copied: u64,
   pub vertex_bytes_copied: u64,
   pub index_bytes_copied: u64,
}

impl RenderSnapshot
{
   pub fn new(instances: Vec<RenderChunkInstance>, properties: Vec<RenderPropertySlot>, damage: Damage) -> Result<Self, RenderSnapshotError>
   {
      Self::from_sequences(vec![RenderChunkSequence::new(instances)], properties, damage)
   }

   pub fn from_sequences(sequences: Vec<RenderChunkSequence>, mut properties: Vec<RenderPropertySlot>, damage: Damage) -> Result<Self, RenderSnapshotError>
   {
      properties.sort_unstable_by_key(|property| property.id.0);
      validate_properties(&properties)?;
      let mut all_instances_have_transform_opacity = true;
      for sequence in &sequences {
         if let Some(id) = sequence.inner.invalid_origin {
            return Err(RenderSnapshotError::NonFiniteOrigin(id));
         }
         for slot in sequence.inner.property_slots.iter().copied() {
            if property(&properties, slot).is_none() {
               return Err(RenderSnapshotError::MissingPropertySlot(slot));
            }
         }
         let mut clip_validation = Ok(());
         sequence.visit_instances(|instance| {
            all_instances_have_transform_opacity &= instance.property_slots.len() == 2;
            if clip_validation.is_err()
            {
               return;
            }
            for clip in instance.dynamic_clips.iter()
            {
               if !matches!(property(&properties, clip.transform).map(|property| property.value), Some(RenderPropertyValue::Transform(_)))
               {
                  clip_validation = Err(RenderSnapshotError::InvalidClipProperty(clip.transform));
                  return;
               }
            }
         });
         clip_validation?;
      }
      let instance_count = sequences.iter().fold(0_u64, |count, sequence| {
         count.saturating_add(sequence.instance_count())
      });
      if instance_count > u64::from(u32::MAX)
      {
         return Err(RenderSnapshotError::GeometryTooLarge);
      }
      let uniform_property_revision = if all_instances_have_transform_opacity
         && instance_count > 0
         && instance_count.checked_mul(2) == Some(properties.len() as u64)
      {
         properties.first()
            .map(|property| property.revision)
            .filter(|revision| properties.iter().all(|property| property.revision == *revision))
      }
      else
      {
         None
      };
      let resolved_spatial = if properties.is_empty() && instance_count >= SNAPSHOT_SPATIAL_INDEX_MIN_INSTANCES
      {
         Some(Arc::new(build_resolved_spatial(&sequences, &properties, instance_count)))
      }
      else
      {
         None
      };
      let metadata_byte_size = sequences.iter().fold(0_u64, |bytes, sequence| {
         bytes.saturating_add(sequence.metadata_byte_size())
      })
      .saturating_add(
         (sequences.len() as u64).saturating_mul(mem::size_of::<RenderChunkSequence>() as u64),
      )
      .saturating_add(
         (properties.len() as u64).saturating_mul(mem::size_of::<RenderPropertySlot>() as u64),
      )
      .saturating_add(resolved_spatial.as_ref().map_or(0, |spatial| spatial.byte_size));
      Ok(Self {
         inner: Arc::new(RenderSnapshotData {
            sequences: sequences.into(),
            resolved_spatial,
            properties: properties.into(),
            damage,
            instance_count,
            uniform_property_revision,
            metadata_byte_size,
         }),
      })
   }

   #[inline]
   #[must_use]
   pub fn sequences(&self) -> &[RenderChunkSequence]
   {
      &self.inner.sequences
   }

   #[inline]
   #[must_use]
   pub fn instance_count(&self) -> u64
   {
      self.inner.instance_count
   }

   /// Returns the shared revision when every instance has one transform and one
   /// opacity property and every property advanced in the same revision epoch.
   #[inline]
   #[must_use]
   pub fn uniform_property_revision(&self) -> Option<u64>
   {
      self.inner.uniform_property_revision
   }

   #[inline]
   #[must_use]
   pub fn metadata_byte_size(&self) -> u64
   {
      self.inner.metadata_byte_size
   }

   pub fn visit_instances<F: FnMut(&RenderChunkInstance)>(&self, mut visit: F)
   {
      for sequence in self.inner.sequences.iter() {
         sequence.visit_instances(&mut visit);
      }
   }

   #[must_use]
   pub fn instance(&self, mut index: u64) -> Option<RenderChunkInstance>
   {
      for sequence in self.inner.sequences.iter() {
         if index < sequence.instance_count() {
            return sequence.instance(index);
         }
         index -= sequence.instance_count();
      }
      None
   }

   pub fn query_damage_instances(&self, rect: RectI, out: &mut Vec<u32>) -> RenderSpatialQueryStats
   {
      let query = RectF::new(rect.x as f32, rect.y as f32, rect.w as f32, rect.h as f32);
      if let Some(spatial) = self.inner.resolved_spatial.as_ref()
      {
         return spatial.spatial_index.query(query, out);
      }
      out.clear();
      let mut visited = 0_u64;
      let mut order = 0_u32;
      self.visit_instances(|instance| {
         visited = visited.saturating_add(1);
         let resolved = resolve_instance(instance, &self.inner.properties)
            .expect("validated render instance must resolve");
         if spatial_intersects_query(resolved.bounds, query)
         {
            out.push(order);
         }
         order = order.saturating_add(1);
      });
      RenderSpatialQueryStats {
         entries_visited: visited,
         entries_matched: out.len() as u64,
      }
   }

   #[must_use]
   pub fn resolved_instance(&self, index: u32) -> Option<RenderResolvedInstance>
   {
      if let Some(spatial) = self.inner.resolved_spatial.as_ref()
      {
         return spatial.instances.get(index as usize).cloned();
      }
      self.instance(u64::from(index)).and_then(|instance| {
         resolve_instance(&instance, &self.inner.properties).ok()
      })
   }

   #[inline]
   #[must_use]
   pub fn precomputed_resolved_instances(&self) -> Option<&[RenderResolvedInstance]>
   {
      self.inner.resolved_spatial.as_ref().map(|spatial| spatial.instances.as_ref())
   }

   #[inline]
   #[must_use]
   pub fn properties(&self) -> &[RenderPropertySlot]
   {
      &self.inner.properties
   }

   #[inline]
   #[must_use]
   pub fn damage(&self) -> &Damage
   {
      &self.inner.damage
   }

   #[inline]
   #[must_use]
   pub fn ptr_eq(&self, other: &Self) -> bool
   {
      Arc::ptr_eq(&self.inner, &other.inner)
   }

   pub fn collect_incompatible_chunk_ids(&self, resources: &[(ImageHandle, u64)], out: &mut Vec<RenderChunkId>)
   {
      self.visit_instances(|instance| {
         let id = instance.chunk.id();
         if !instance.chunk.resources_compatible(resources) && !out.contains(&id) {
            out.push(id);
         }
      });
   }

   #[must_use]
   pub fn incompatible_chunk_ids(&self, resources: &[RenderResourceDependency]) -> Vec<RenderChunkId>
   {
      let mut ids = Vec::new();
      self.visit_instances(|instance| {
         let compatible = instance.chunk.resource_dependencies().iter().all(|required| {
            resources.iter().any(|available| available == required)
         });
         let id = instance.chunk.id();
         if !compatible && !ids.contains(&id) {
            ids.push(id);
         }
      });
      ids
   }

   /// Appends a compatibility flat draw list and reports every copied command and geometry byte.
   /// Non-translation affine properties are rejected because `DrawList` cannot preserve them.
   pub fn flatten_into(&self, out: &mut DrawList) -> Result<RenderFallbackStats, RenderSnapshotError>
   {
      let mut validation = Ok(());
      self.visit_instances(|instance| {
         if validation.is_ok() {
            validation = flat_instance_properties(instance, &self.inner.properties).map(|_| ());
         }
      });
      validation?;

      let mut command_count = out.items.len();
      let mut vertex_count = out.vertices.len();
      let mut index_count = out.indices.len();
      let mut capacity = Ok(());
      self.visit_instances(|instance| {
         if capacity.is_err() {
            return;
         }
         let list = instance.chunk.draw_list();
         let wrappers = usize::from(instance.layer.is_some()) * 2
            + usize::from(instance.clip.is_some()) * 2
            + instance.dynamic_clips.len().saturating_mul(2);
         command_count = match command_count.checked_add(list.items.len()).and_then(|count| count.checked_add(wrappers)) {
            Some(count) => count,
            None => {
               capacity = Err(RenderSnapshotError::GeometryTooLarge);
               return;
            }
         };
         vertex_count = match vertex_count.checked_add(list.vertices.len()) {
            Some(count) if count <= u32::MAX as usize => count,
            _ => {
               capacity = Err(RenderSnapshotError::GeometryTooLarge);
               return;
            }
         };
         index_count = match index_count.checked_add(list.indices.len()) {
            Some(count) if count <= u32::MAX as usize => count,
            _ => {
               capacity = Err(RenderSnapshotError::GeometryTooLarge);
               return;
            }
         };
      });
      capacity?;
      out.items.reserve(command_count.saturating_sub(out.items.len()));
      out.vertices.reserve(vertex_count.saturating_sub(out.vertices.len()));
      out.indices.reserve(index_count.saturating_sub(out.indices.len()));

      let mut stats = RenderFallbackStats { fallback_count: 1, ..RenderFallbackStats::default() };
      let mut flattened = Ok(());
      self.visit_instances(|instance| {
         if flattened.is_err() {
            return;
         }
         flattened = flat_instance_properties(instance, &self.inner.properties).and_then(
            |(translation, opacity)| {
               append_flat_instance(instance, translation, opacity, &self.inner.properties, out, &mut stats)
            },
         );
      });
      flattened?;
      Ok(stats)
   }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope
{
   Clip,
   Layer,
}

fn canonicalize_draw_list(source: &DrawList, index_mode: ChunkIndexMode) -> Result<(DrawList, RenderChunkOrdering), RenderChunkError>
{
   let mut list = DrawList {
      items: Vec::with_capacity(source.items.len()),
      vertices: Vec::new(),
      indices: Vec::new(),
   };
   let mut scopes = Vec::new();
   let mut ordering = RenderChunkOrdering::default();
   let mut clip_depth = 0_u32;
   let mut layer_depth = 0_u32;
   for (command, item) in source.items.iter().enumerate() {
      let canonical = match item {
         DrawCmd::Solid { vb, ib, color } => {
            let (vb, ib) = canonical_geometry(source, *vb, *ib, index_mode, command, &mut list)?;
            DrawCmd::Solid { vb, ib, color: *color }
         }
         DrawCmd::ImageMesh { tex, vb, ib, alpha } => {
            let (vb, ib) = canonical_geometry(source, *vb, *ib, index_mode, command, &mut list)?;
            DrawCmd::ImageMesh { tex: *tex, vb, ib, alpha: *alpha }
         }
         DrawCmd::GlyphRun { run } => {
            let (vb, ib) = canonical_geometry(source, run.vb, run.ib, index_mode, command, &mut list)?;
            DrawCmd::GlyphRun { run: GlyphRun { vb, ib, ..*run } }
         }
         DrawCmd::ClipPush { rect } => {
            clip_depth = clip_depth.checked_add(1).ok_or(RenderChunkError::GeometryTooLarge)?;
            ordering.max_clip_depth = ordering.max_clip_depth.max(clip_depth);
            ordering.has_clip = true;
            scopes.push(Scope::Clip);
            DrawCmd::ClipPush { rect: *rect }
         }
         DrawCmd::ClipPop => {
            if clip_depth == 0 {
               return Err(RenderChunkError::ClipUnderflow { command });
            }
            if scopes.pop() != Some(Scope::Clip) {
               return Err(RenderChunkError::OrderingMismatch { command });
            }
            clip_depth -= 1;
            DrawCmd::ClipPop
         }
         DrawCmd::LayerBegin { id, rect, dirty } => {
            layer_depth = layer_depth.checked_add(1).ok_or(RenderChunkError::GeometryTooLarge)?;
            ordering.max_layer_depth = ordering.max_layer_depth.max(layer_depth);
            ordering.has_layer = true;
            scopes.push(Scope::Layer);
            DrawCmd::LayerBegin { id: *id, rect: *rect, dirty: *dirty }
         }
         DrawCmd::LayerEnd => {
            if layer_depth == 0 {
               return Err(RenderChunkError::LayerUnderflow { command });
            }
            if scopes.pop() != Some(Scope::Layer) {
               return Err(RenderChunkError::OrderingMismatch { command });
            }
            layer_depth -= 1;
            DrawCmd::LayerEnd
         }
         _ => item.clone(),
      };
      list.items.push(canonical);
   }
   if clip_depth != 0 {
      return Err(RenderChunkError::UnbalancedClipStack);
   }
   if layer_depth != 0 {
      return Err(RenderChunkError::UnbalancedLayerStack);
   }
   Ok((list, ordering))
}

fn canonical_geometry(source: &DrawList, vb: VertexSpan, ib: IndexSpan, index_mode: ChunkIndexMode, command: usize, out: &mut DrawList) -> Result<(VertexSpan, IndexSpan), RenderChunkError>
{
   let vertex_start = vb.offset as usize;
   let vertex_end = vertex_start.checked_add(vb.len as usize).ok_or(RenderChunkError::VertexSpanOutOfBounds { command })?;
   let vertices = source.vertices.get(vertex_start..vertex_end).ok_or(RenderChunkError::VertexSpanOutOfBounds { command })?;
   let index_start = ib.offset as usize;
   let index_end = index_start.checked_add(ib.len as usize).ok_or(RenderChunkError::IndexSpanOutOfBounds { command })?;
   let indices = source.indices.get(index_start..index_end).ok_or(RenderChunkError::IndexSpanOutOfBounds { command })?;
   let vertex_offset = u32::try_from(out.vertices.len()).map_err(|_| RenderChunkError::GeometryTooLarge)?;
   let index_offset = u32::try_from(out.indices.len()).map_err(|_| RenderChunkError::GeometryTooLarge)?;
   out.vertices.extend_from_slice(vertices);
   out.indices.reserve(indices.len());
   let vertex_end = vb.offset.checked_add(vb.len).ok_or(RenderChunkError::VertexSpanOutOfBounds { command })?;
   for index in indices.iter().copied() {
      let local = match index_mode {
         ChunkIndexMode::Local => index as u32,
         ChunkIndexMode::Absolute => {
            let absolute = index as u32;
            if absolute < vb.offset || absolute >= vertex_end {
               return Err(RenderChunkError::IndexOutsideVertexSpan { command, index });
            }
            absolute - vb.offset
         }
      };
      if local >= vb.len || local > u16::MAX as u32 {
         return Err(RenderChunkError::IndexOutsideVertexSpan { command, index });
      }
      out.indices.push(local as u16);
   }
   Ok((
      VertexSpan { offset: vertex_offset, len: vb.len },
      IndexSpan { offset: index_offset, len: ib.len },
   ))
}

fn validate_resource_dependencies(list: &DrawList, supplied: &[RenderResourceDependency]) -> Result<Vec<RenderResourceDependency>, RenderChunkError>
{
   let mut supplied_unique = Vec::<RenderResourceDependency>::with_capacity(supplied.len());
   for dependency in supplied.iter().copied() {
      if let Some(existing) = supplied_unique.iter().find(|existing| existing.image == dependency.image) {
         if existing.generation != dependency.generation {
            return Err(RenderChunkError::ConflictingResourceGeneration(dependency.image));
         }
      } else {
         supplied_unique.push(dependency);
      }
   }

   let mut resources = Vec::<RenderResourceDependency>::new();
   for command in &list.items {
      let dependency = match command {
         DrawCmd::Image { tex, .. } | DrawCmd::ImageMesh { tex, .. } | DrawCmd::NineSlice { tex, .. } => {
            supplied_unique.iter().find(|dependency| dependency.image == *tex).copied().ok_or(RenderChunkError::MissingResourceGeneration(*tex))?
         }
         DrawCmd::GlyphRun { run } => RenderResourceDependency { image: run.atlas, generation: run.atlas_revision },
         _ => continue,
      };
      if let Some(existing) = resources.iter().find(|existing| existing.image == dependency.image) {
         if existing.generation != dependency.generation {
            return Err(RenderChunkError::ConflictingResourceGeneration(dependency.image));
         }
      } else {
         resources.push(dependency);
      }
   }
   for dependency in supplied_unique {
      if !resources.iter().any(|used| used == &dependency) {
         return Err(RenderChunkError::UnusedResourceGeneration(dependency.image));
      }
   }
   resources.sort_unstable_by_key(|dependency| dependency.image.0);
   Ok(resources)
}

fn retained_byte_size(list: &DrawList, resources: usize, commands: usize, spans: usize, spatial_bytes: u64) -> Result<u64, RenderChunkError>
{
   let draw_commands = list.items.len().checked_mul(mem::size_of::<DrawCmd>()).ok_or(RenderChunkError::GeometryTooLarge)?;
   let vertices = list.vertices.len().checked_mul(mem::size_of::<Vertex>()).ok_or(RenderChunkError::GeometryTooLarge)?;
   let indices = list.indices.len().checked_mul(mem::size_of::<u16>()).ok_or(RenderChunkError::GeometryTooLarge)?;
   let dependencies = resources.checked_mul(mem::size_of::<RenderResourceDependency>()).ok_or(RenderChunkError::GeometryTooLarge)?;
   let command_spatial = commands.checked_mul(mem::size_of::<RenderCommandSpatial>()).ok_or(RenderChunkError::GeometryTooLarge)?;
   let paint_spans = spans.checked_mul(mem::size_of::<RenderPaintSpan>()).ok_or(RenderChunkError::GeometryTooLarge)?;
   let fixed = draw_commands.checked_add(vertices)
      .and_then(|size| size.checked_add(indices))
      .and_then(|size| size.checked_add(dependencies))
      .and_then(|size| size.checked_add(command_spatial))
      .and_then(|size| size.checked_add(paint_spans))
      .ok_or(RenderChunkError::GeometryTooLarge)?;
   u64::try_from(fixed).map_err(|_| RenderChunkError::GeometryTooLarge)?.checked_add(spatial_bytes)
      .ok_or(RenderChunkError::GeometryTooLarge)
}

const RASTER_AA_OUTSET_DP: f32 = 1.0;
const EFFECT_MAX_BLUR_SIGMA_DP: f32 = 72.0;

fn prepare_spatial_metadata(list: &DrawList) -> Result<(RenderSpatialBounds, Vec<RenderCommandSpatial>, Vec<RenderPaintSpan>, SpatialIndex), RenderChunkError>
{
   let empty = RenderCommandSpatial {
      bounds: RenderSpatialBounds::Empty,
      resolved_clip: RenderSpatialBounds::Unbounded,
      matching_scope: None,
      vertex_count: 0,
   };
   let mut commands = vec![empty; list.items.len()];
   let mut spans = Vec::new();
   let mut scopes = Vec::<(Scope, usize, RenderSpatialBounds)>::new();
   let mut current_clip = RenderSpatialBounds::Unbounded;
   let mut layer_depth = 0_u32;
   for (index, command) in list.items.iter().enumerate()
   {
      let command_index = u32::try_from(index).map_err(|_| RenderChunkError::GeometryTooLarge)?;
      let vertex_count = command_vertex_count(command);
      match command
      {
         DrawCmd::ClipPush { rect } =>
         {
            let previous = current_clip;
            current_clip = intersect_spatial(current_clip, rect_i_bounds(*rect));
            commands[index] = RenderCommandSpatial {
               resolved_clip: current_clip,
               ..empty
            };
            scopes.push((Scope::Clip, index, previous));
         }
         DrawCmd::ClipPop =>
         {
            let Some((Scope::Clip, begin, previous)) = scopes.pop() else
            {
               return Err(RenderChunkError::OrderingMismatch { command: index });
            };
            commands[index].resolved_clip = current_clip;
            commands[index].matching_scope = Some(u32::try_from(begin).map_err(|_| RenderChunkError::GeometryTooLarge)?);
            commands[begin].matching_scope = Some(command_index);
            current_clip = previous;
         }
         DrawCmd::LayerBegin { rect, .. } =>
         {
            let bounds = intersect_spatial(command_spatial_bounds(list, command), current_clip);
            commands[index] = RenderCommandSpatial {
               bounds,
               resolved_clip: current_clip,
               matching_scope: None,
               vertex_count,
            };
            let previous = current_clip;
            current_clip = intersect_spatial(current_clip, rect_f_bounds(*rect));
            scopes.push((Scope::Layer, index, previous));
            layer_depth = layer_depth.saturating_add(1);
         }
         DrawCmd::LayerEnd =>
         {
            let Some((Scope::Layer, begin, previous)) = scopes.pop() else
            {
               return Err(RenderChunkError::OrderingMismatch { command: index });
            };
            commands[index].resolved_clip = current_clip;
            commands[index].matching_scope = Some(u32::try_from(begin).map_err(|_| RenderChunkError::GeometryTooLarge)?);
            commands[begin].matching_scope = Some(command_index);
            current_clip = previous;
            layer_depth = layer_depth.saturating_sub(1);
            if layer_depth == 0
            {
               spans.push(RenderPaintSpan {
                  begin: u32::try_from(begin).map_err(|_| RenderChunkError::GeometryTooLarge)?,
                  end: command_index.saturating_add(1),
                  bounds: commands[begin].bounds,
                  vertex_count: 0,
               });
            }
         }
         _ =>
         {
            let bounds = intersect_spatial(command_spatial_bounds(list, command), current_clip);
            commands[index] = RenderCommandSpatial {
               bounds,
               resolved_clip: current_clip,
               matching_scope: None,
               vertex_count,
            };
            if layer_depth == 0 && !bounds.is_empty()
            {
               spans.push(RenderPaintSpan {
                  begin: command_index,
                  end: command_index.saturating_add(1),
                  bounds,
                  vertex_count,
               });
            }
         }
      }
   }
   let mut vertex_prefix = Vec::with_capacity(commands.len().saturating_add(1));
   vertex_prefix.push(0_u64);
   for command in &commands
   {
      vertex_prefix.push(vertex_prefix.last().copied().unwrap_or(0).saturating_add(u64::from(command.vertex_count)));
   }
   for span in &mut spans
   {
      let begin = span.begin as usize;
      let end = span.end as usize;
      span.vertex_count = vertex_prefix.get(end).copied().unwrap_or(u64::MAX)
         .saturating_sub(vertex_prefix.get(begin).copied().unwrap_or(0))
         .min(u64::from(u32::MAX)) as u32;
   }
   spans.sort_unstable_by_key(|span| span.begin);
   let bounds = spans.iter().fold(RenderSpatialBounds::Empty, |bounds, span| {
      union_spatial(bounds, span.bounds)
   });
   let spatial_index = SpatialIndex::new(spans.iter().enumerate().map(|(index, span)| {
      (index as u32, span.bounds)
   }));
   Ok((bounds, commands, spans, spatial_index))
}

fn command_vertex_count(command: &DrawCmd) -> u32
{
   match command
   {
      DrawCmd::Solid { vb, .. } | DrawCmd::ImageMesh { vb, .. } => vb.len,
      DrawCmd::GlyphRun { run } => run.vb.len,
      _ => 0,
   }
}

fn command_spatial_bounds(list: &DrawList, command: &DrawCmd) -> RenderSpatialBounds
{
   match command
   {
      DrawCmd::Solid { vb, .. } | DrawCmd::ImageMesh { vb, .. } =>
         outset_spatial(span_spatial_bounds(&list.vertices, *vb), RASTER_AA_OUTSET_DP),
      DrawCmd::GlyphRun { run } =>
         outset_spatial(span_spatial_bounds(&list.vertices, run.vb), RASTER_AA_OUTSET_DP),
      DrawCmd::LayerBegin { rect, .. }
      | DrawCmd::Image { dst: rect, .. }
      | DrawCmd::RRect { rect, .. }
      | DrawCmd::NineSlice { rect, .. } =>
         outset_spatial(rect_f_bounds(*rect), RASTER_AA_OUTSET_DP),
      DrawCmd::Backdrop { rect, sigma, .. } =>
         effect_bounds(*rect, *sigma),
      DrawCmd::VisualEffect { rect, effect } =>
         effect_bounds(*rect, effect.blur_intensity() * EFFECT_MAX_BLUR_SIGMA_DP),
      DrawCmd::CameraBg { rect, blur, sigma, .. } =>
      {
         effect_bounds(*rect, if *blur { *sigma } else { 0.0 })
      }
      DrawCmd::Spinner { center, atom, .. } =>
      {
         outset_spatial(
            rect_f_bounds(RectF::new(center[0] - atom, center[1] - atom, atom * 2.0, atom * 2.0)),
            RASTER_AA_OUTSET_DP,
         )
      }
      DrawCmd::LayerEnd | DrawCmd::ClipPush { .. } | DrawCmd::ClipPop => RenderSpatialBounds::Empty,
   }
}

fn effect_bounds(rect: RectF, sigma: f32) -> RenderSpatialBounds
{
   if !sigma.is_finite()
   {
      return RenderSpatialBounds::Unbounded;
   }
   outset_spatial(rect_f_bounds(rect), RASTER_AA_OUTSET_DP + sigma.abs() * 3.0)
}

fn span_spatial_bounds(vertices: &[Vertex], span: VertexSpan) -> RenderSpatialBounds
{
   let start = span.offset as usize;
   let Some(end) = start.checked_add(span.len as usize) else { return RenderSpatialBounds::Unbounded };
   let Some(vertices) = vertices.get(start..end) else { return RenderSpatialBounds::Unbounded };
   let Some(first) = vertices.first() else { return RenderSpatialBounds::Empty };
   if !vertices.iter().all(|vertex| vertex.x.is_finite() && vertex.y.is_finite())
   {
      return RenderSpatialBounds::Unbounded;
   }
   let mut x0 = first.x;
   let mut y0 = first.y;
   let mut x1 = first.x;
   let mut y1 = first.y;
   for vertex in &vertices[1..] {
      x0 = x0.min(vertex.x);
      y0 = y0.min(vertex.y);
      x1 = x1.max(vertex.x);
      y1 = y1.max(vertex.y);
   }
   RenderSpatialBounds::Finite(RectF::new(x0, y0, x1 - x0, y1 - y0))
}

fn finite_rect(rect: RectF) -> Option<RectF>
{
   if ![rect.x, rect.y, rect.w, rect.h].iter().all(|value| value.is_finite()) {
      return None;
   }
   let x1 = rect.x + rect.w;
   let y1 = rect.y + rect.h;
   if !x1.is_finite() || !y1.is_finite() {
      return None;
   }
   Some(RectF::new(rect.x.min(x1), rect.y.min(y1), (rect.x - x1).abs(), (rect.y - y1).abs()))
}

fn rect_f_bounds(rect: RectF) -> RenderSpatialBounds
{
   finite_rect(rect).map_or(RenderSpatialBounds::Unbounded, |rect| {
      if rect.w <= 0.0 || rect.h <= 0.0
      {
         RenderSpatialBounds::Empty
      }
      else
      {
         RenderSpatialBounds::Finite(rect)
      }
   })
}

fn rect_i_bounds(rect: RectI) -> RenderSpatialBounds
{
   rect_f_bounds(RectF::new(rect.x as f32, rect.y as f32, rect.w as f32, rect.h as f32))
}

fn outset_spatial(bounds: RenderSpatialBounds, outset: f32) -> RenderSpatialBounds
{
   match bounds
   {
      RenderSpatialBounds::Empty => RenderSpatialBounds::Empty,
      RenderSpatialBounds::Unbounded => RenderSpatialBounds::Unbounded,
      RenderSpatialBounds::Finite(rect) if outset.is_finite() =>
      {
         rect_f_bounds(RectF::new(
            rect.x - outset,
            rect.y - outset,
            rect.w + outset * 2.0,
            rect.h + outset * 2.0,
         ))
      }
      RenderSpatialBounds::Finite(_) => RenderSpatialBounds::Unbounded,
   }
}

fn intersect_spatial(a: RenderSpatialBounds, b: RenderSpatialBounds) -> RenderSpatialBounds
{
   match (a, b)
   {
      (RenderSpatialBounds::Empty, _) | (_, RenderSpatialBounds::Empty) => RenderSpatialBounds::Empty,
      (RenderSpatialBounds::Unbounded, bounds) | (bounds, RenderSpatialBounds::Unbounded) => bounds,
      (RenderSpatialBounds::Finite(a), RenderSpatialBounds::Finite(b)) =>
      {
         let x0 = a.x.max(b.x);
         let y0 = a.y.max(b.y);
         let x1 = (a.x + a.w).min(b.x + b.w);
         let y1 = (a.y + a.h).min(b.y + b.h);
         if x1 <= x0 || y1 <= y0
         {
            RenderSpatialBounds::Empty
         }
         else
         {
            RenderSpatialBounds::Finite(RectF::new(x0, y0, x1 - x0, y1 - y0))
         }
      }
   }
}

fn union_spatial(a: RenderSpatialBounds, b: RenderSpatialBounds) -> RenderSpatialBounds
{
   match (a, b)
   {
      (RenderSpatialBounds::Unbounded, _) | (_, RenderSpatialBounds::Unbounded) => RenderSpatialBounds::Unbounded,
      (RenderSpatialBounds::Empty, bounds) | (bounds, RenderSpatialBounds::Empty) => bounds,
      (RenderSpatialBounds::Finite(a), RenderSpatialBounds::Finite(b)) =>
      {
         let x0 = a.x.min(b.x);
         let y0 = a.y.min(b.y);
         let x1 = (a.x + a.w).max(b.x + b.w);
         let y1 = (a.y + a.h).max(b.y + b.h);
         RenderSpatialBounds::Finite(RectF::new(x0, y0, x1 - x0, y1 - y0))
      }
   }
}

fn rects_intersect(a: RectF, b: RectF) -> bool
{
   a.x + a.w > b.x
      && a.x < b.x + b.w
      && a.y + a.h > b.y
      && a.y < b.y + b.h
}

fn spatial_intersects_query(bounds: RenderSpatialBounds, query: RectF) -> bool
{
   let Some(query) = finite_rect(query) else { return !bounds.is_empty() };
   match bounds
   {
      RenderSpatialBounds::Empty => false,
      RenderSpatialBounds::Unbounded => query.w > 0.0 && query.h > 0.0,
      RenderSpatialBounds::Finite(bounds) => rects_intersect(bounds, query),
   }
}

fn validate_properties(properties: &[RenderPropertySlot]) -> Result<(), RenderSnapshotError>
{
   for pair in properties.windows(2) {
      if pair[0].id == pair[1].id {
         return Err(RenderSnapshotError::DuplicatePropertySlot(pair[0].id));
      }
   }
   for pair in properties.windows(2)
   {
      let Some(dynamic_index) = pair[0].id.dynamic_index() else { continue };
      if pair[1].id.dynamic_index() == Some(dynamic_index) && pair[0].id != pair[1].id
      {
         return Err(RenderSnapshotError::ConflictingPropertyGeneration { index: dynamic_index });
      }
   }
   for property in properties {
      match property.value {
         RenderPropertyValue::Transform(transform) if !transform.iter().all(|value| value.is_finite()) => {
            return Err(RenderSnapshotError::NonFiniteProperty(property.id));
         }
         RenderPropertyValue::Opacity(opacity) if !opacity.is_finite() => {
            return Err(RenderSnapshotError::NonFiniteProperty(property.id));
         }
         RenderPropertyValue::Opacity(opacity) if !(0.0..=1.0).contains(&opacity) => {
            return Err(RenderSnapshotError::InvalidOpacity(property.id));
         }
         RenderPropertyValue::Transform(_) | RenderPropertyValue::Opacity(_) => {}
      }
   }
   Ok(())
}

fn property(properties: &[RenderPropertySlot], id: RenderPropertySlotId) -> Option<&RenderPropertySlot>
{
   let index = properties.binary_search_by_key(&id.0, |property| property.id.0).ok()?;
   properties.get(index)
}

fn resolve_instance(instance: &RenderChunkInstance, properties: &[RenderPropertySlot]) -> Result<RenderResolvedInstance, RenderSnapshotError>
{
   let mut matrix = [1.0_f32, 0.0, 0.0, 1.0];
   let mut translation = [0.0_f32, 0.0];
   let mut opacity = 1.0_f32;
   let mut source_revision = 0xcbf2_9ce4_8422_2325_u64;
   for id in instance.property_slots.iter().copied()
   {
      let property = property(properties, id).ok_or(RenderSnapshotError::MissingPropertySlot(id))?;
      source_revision = source_revision.rotate_left(17) ^ (u64::from(id.0) << 32) ^ property.revision;
      match property.value
      {
         RenderPropertyValue::Transform(transform) =>
         {
            let next = [
               transform[0] * matrix[0] + transform[2] * matrix[1],
               transform[1] * matrix[0] + transform[3] * matrix[1],
               transform[0] * matrix[2] + transform[2] * matrix[3],
               transform[1] * matrix[2] + transform[3] * matrix[3],
            ];
            translation = [
               transform[0] * translation[0] + transform[2] * translation[1] + transform[4],
               transform[1] * translation[0] + transform[3] * translation[1] + transform[5],
            ];
            matrix = next;
         }
         RenderPropertyValue::Opacity(value) => opacity *= value,
      }
   }
   translation = [
      matrix[0] * instance.origin[0] + matrix[2] * instance.origin[1] + translation[0],
      matrix[1] * instance.origin[0] + matrix[3] * instance.origin[1] + translation[1],
   ];
   let transform = [matrix[0], matrix[1], matrix[2], matrix[3], translation[0], translation[1]];
   let mut resolved_clip = instance.clip.map_or(RenderSpatialBounds::Unbounded, |clip| {
      transform_spatial(rect_i_bounds(clip), transform)
   });
   for clip in instance.dynamic_clips.iter().copied()
   {
      let property = property(properties, clip.transform)
         .ok_or(RenderSnapshotError::MissingPropertySlot(clip.transform))?;
      let RenderPropertyValue::Transform(transform) = property.value else
      {
         return Err(RenderSnapshotError::InvalidClipProperty(clip.transform));
      };
      resolved_clip = intersect_spatial(
         resolved_clip,
         transform_spatial(rect_f_bounds(clip.rect), transform),
      );
   }
   let local_bounds = instance.layer.map_or(instance.chunk.spatial_bounds(), |layer| {
      outset_spatial(rect_f_bounds(layer.rect), RASTER_AA_OUTSET_DP)
   });
   let bounds = intersect_spatial(transform_spatial(local_bounds, transform), resolved_clip);
   Ok(RenderResolvedInstance {
      instance: instance.clone(),
      transform,
      opacity: opacity.clamp(0.0, 1.0),
      source_revision,
      resolved_clip,
      bounds,
   })
}

fn build_resolved_spatial(sequences: &[RenderChunkSequence], properties: &[RenderPropertySlot], instance_count: u64) -> RenderResolvedSpatialData
{
   let mut instances = Vec::with_capacity(instance_count as usize);
   for sequence in sequences
   {
      sequence.visit_instances(|instance| {
         instances.push(resolve_instance(instance, properties)
            .expect("validated render instance must resolve"));
      });
   }
   let spatial_index = SpatialIndex::new(instances.iter().enumerate().map(|(order, instance)| {
      (order as u32, instance.bounds)
   }));
   let byte_size = (instances.len() as u64)
      .saturating_mul(mem::size_of::<RenderResolvedInstance>() as u64)
      .saturating_add(spatial_index.byte_size());
   RenderResolvedSpatialData {
      instances: instances.into(),
      spatial_index,
      byte_size,
   }
}

fn transform_spatial(bounds: RenderSpatialBounds, transform: [f32; 6]) -> RenderSpatialBounds
{
   match bounds
   {
      RenderSpatialBounds::Empty => RenderSpatialBounds::Empty,
      RenderSpatialBounds::Unbounded => RenderSpatialBounds::Unbounded,
      RenderSpatialBounds::Finite(rect) =>
      {
         let x1 = rect.x + rect.w;
         let y1 = rect.y + rect.h;
         let points = [
            transform_point([rect.x, rect.y], transform),
            transform_point([x1, rect.y], transform),
            transform_point([rect.x, y1], transform),
            transform_point([x1, y1], transform),
         ];
         if !points.iter().flatten().all(|value| value.is_finite())
         {
            return RenderSpatialBounds::Unbounded;
         }
         let x0 = points.iter().map(|point| point[0]).fold(f32::INFINITY, f32::min);
         let y0 = points.iter().map(|point| point[1]).fold(f32::INFINITY, f32::min);
         let x1 = points.iter().map(|point| point[0]).fold(f32::NEG_INFINITY, f32::max);
         let y1 = points.iter().map(|point| point[1]).fold(f32::NEG_INFINITY, f32::max);
         if x1 <= x0 || y1 <= y0
         {
            RenderSpatialBounds::Empty
         }
         else
         {
            RenderSpatialBounds::Finite(RectF::new(x0, y0, x1 - x0, y1 - y0))
         }
      }
   }
}

fn transform_point(point: [f32; 2], transform: [f32; 6]) -> [f32; 2]
{
   [
      transform[0] * point[0] + transform[2] * point[1] + transform[4],
      transform[1] * point[0] + transform[3] * point[1] + transform[5],
   ]
}

fn flat_instance_properties(instance: &RenderChunkInstance, properties: &[RenderPropertySlot]) -> Result<([f32; 2], f32), RenderSnapshotError>
{
   let mut translation = instance.origin;
   let mut opacity = 1.0;
   for id in instance.property_slots.iter().copied() {
      let property = property(properties, id).ok_or(RenderSnapshotError::MissingPropertySlot(id))?;
      match property.value {
         RenderPropertyValue::Transform(transform) => {
            if transform[0] != 1.0 || transform[1] != 0.0 || transform[2] != 0.0 || transform[3] != 1.0 {
               return Err(RenderSnapshotError::UnsupportedFlatTransform(id));
            }
            translation[0] += transform[4];
            translation[1] += transform[5];
         }
         RenderPropertyValue::Opacity(value) => opacity *= value,
      }
   }
   for clip in instance.dynamic_clips.iter()
   {
      let _ = flat_clip_rect(*clip, properties)?;
   }
   Ok((translation, opacity))
}

fn append_flat_instance(instance: &RenderChunkInstance, translation: [f32; 2], opacity: f32, properties: &[RenderPropertySlot], out: &mut DrawList, stats: &mut RenderFallbackStats) -> Result<(), RenderSnapshotError>
{
   let list = instance.chunk.draw_list();
   let vertex_base = u32::try_from(out.vertices.len()).map_err(|_| RenderSnapshotError::GeometryTooLarge)?;
   let index_base = u32::try_from(out.indices.len()).map_err(|_| RenderSnapshotError::GeometryTooLarge)?;
   let wrapper_commands = u64::from(instance.layer.is_some()) * 2
      + u64::from(instance.clip.is_some()) * 2
      + (instance.dynamic_clips.len() as u64).saturating_mul(2);
   out.items.reserve(list.items.len() + wrapper_commands as usize);
   out.vertices.reserve(list.vertices.len());
   out.indices.reserve(list.indices.len());
   if let Some(layer) = instance.layer {
      out.items.push(DrawCmd::LayerBegin {
         id: layer.id,
         rect: translate_rect(layer.rect, translation),
         dirty: layer.dirty,
      });
   }
   if let Some(clip) = instance.clip {
      out.items.push(DrawCmd::ClipPush { rect: translate_rect_i(clip, translation) });
   }
   for clip in instance.dynamic_clips.iter().copied()
   {
      out.items.push(DrawCmd::ClipPush { rect: flat_clip_rect(clip, properties)? });
   }
   for command in &list.items {
      out.items.push(flat_command(command, vertex_base, index_base, translation, opacity)?);
   }
   for _ in instance.dynamic_clips.iter()
   {
      out.items.push(DrawCmd::ClipPop);
   }
   if instance.clip.is_some() {
      out.items.push(DrawCmd::ClipPop);
   }
   if instance.layer.is_some() {
      out.items.push(DrawCmd::LayerEnd);
   }
   out.vertices.extend(list.vertices.iter().copied().map(|vertex| Vertex {
      x: vertex.x + translation[0],
      y: vertex.y + translation[1],
      rgba: rgba_with_opacity(vertex.rgba, opacity),
      ..vertex
   }));
   out.indices.extend_from_slice(&list.indices);

   let commands = u64::try_from(list.items.len()).map_err(|_| RenderSnapshotError::GeometryTooLarge)? + wrapper_commands;
   let vertices = u64::try_from(list.vertices.len()).map_err(|_| RenderSnapshotError::GeometryTooLarge)?;
   let indices = u64::try_from(list.indices.len()).map_err(|_| RenderSnapshotError::GeometryTooLarge)?;
   stats.chunks_flattened += 1;
   stats.commands_copied += commands;
   stats.vertices_copied += vertices;
   stats.indices_copied += indices;
   stats.command_bytes_copied += commands * mem::size_of::<DrawCmd>() as u64;
   stats.vertex_bytes_copied += vertices * mem::size_of::<Vertex>() as u64;
   stats.index_bytes_copied += indices * mem::size_of::<u16>() as u64;
   Ok(())
}

fn flat_clip_rect(clip: RenderDynamicClip, properties: &[RenderPropertySlot]) -> Result<RectI, RenderSnapshotError>
{
   let property = property(properties, clip.transform)
      .ok_or(RenderSnapshotError::MissingPropertySlot(clip.transform))?;
   let RenderPropertyValue::Transform(transform) = property.value else
   {
      return Err(RenderSnapshotError::UnsupportedFlatTransform(clip.transform));
   };
   if transform[0] != 1.0 || transform[1] != 0.0 || transform[2] != 0.0 || transform[3] != 1.0
   {
      return Err(RenderSnapshotError::UnsupportedFlatTransform(clip.transform));
   }
   let x0 = clip.rect.x + transform[4];
   let y0 = clip.rect.y + transform[5];
   let x1 = x0 + clip.rect.w;
   let y1 = y0 + clip.rect.h;
   Ok(RectI::new(
      x0.min(x1).floor() as i32,
      y0.min(y1).floor() as i32,
      (x0 - x1).abs().ceil() as i32,
      (y0 - y1).abs().ceil() as i32,
   ))
}

fn flat_command(command: &DrawCmd, vertex_base: u32, index_base: u32, translation: [f32; 2], opacity: f32) -> Result<DrawCmd, RenderSnapshotError>
{
   let translated = |rect| translate_rect(rect, translation);
   let offset_vb = |span: VertexSpan| -> Result<VertexSpan, RenderSnapshotError> {
      Ok(VertexSpan { offset: span.offset.checked_add(vertex_base).ok_or(RenderSnapshotError::GeometryTooLarge)?, len: span.len })
   };
   let offset_ib = |span: IndexSpan| -> Result<IndexSpan, RenderSnapshotError> {
      Ok(IndexSpan { offset: span.offset.checked_add(index_base).ok_or(RenderSnapshotError::GeometryTooLarge)?, len: span.len })
   };
   Ok(match command {
      DrawCmd::LayerBegin { id, rect, dirty } => DrawCmd::LayerBegin { id: *id, rect: translated(*rect), dirty: *dirty },
      DrawCmd::LayerEnd => DrawCmd::LayerEnd,
      DrawCmd::Solid { vb, ib, color } => DrawCmd::Solid { vb: offset_vb(*vb)?, ib: offset_ib(*ib)?, color: color_with_opacity(*color, opacity) },
      DrawCmd::Image { tex, dst, src, alpha } => DrawCmd::Image { tex: *tex, dst: translated(*dst), src: *src, alpha: alpha * opacity },
      DrawCmd::ImageMesh { tex, vb, ib, alpha } => DrawCmd::ImageMesh { tex: *tex, vb: offset_vb(*vb)?, ib: offset_ib(*ib)?, alpha: alpha * opacity },
      DrawCmd::GlyphRun { run } => DrawCmd::GlyphRun { run: GlyphRun { vb: offset_vb(run.vb)?, ib: offset_ib(run.ib)?, color: color_with_opacity(run.color, opacity), ..*run } },
      DrawCmd::RRect { rect, radii, color } => DrawCmd::RRect { rect: translated(*rect), radii: *radii, color: color_with_opacity(*color, opacity) },
      DrawCmd::NineSlice { tex, rect, slice, alpha } => DrawCmd::NineSlice { tex: *tex, rect: translated(*rect), slice: *slice, alpha: alpha * opacity },
      DrawCmd::Backdrop { rect, sigma, tint, alpha } => DrawCmd::Backdrop { rect: translated(*rect), sigma: *sigma, tint: *tint, alpha: alpha * opacity },
      DrawCmd::VisualEffect { rect, effect } => DrawCmd::VisualEffect { rect: translated(*rect), effect: effect_with_opacity(*effect, opacity) },
      DrawCmd::CameraBg { rect, tint, alpha, grayscale, blur, sigma } => DrawCmd::CameraBg { rect: translated(*rect), tint: *tint, alpha: alpha * opacity, grayscale: *grayscale, blur: *blur, sigma: *sigma },
      DrawCmd::Spinner { center, atom, alpha } => DrawCmd::Spinner { center: [center[0] + translation[0], center[1] + translation[1]], atom: *atom, alpha: alpha * opacity },
      DrawCmd::ClipPush { rect } => DrawCmd::ClipPush { rect: translate_rect_i(*rect, translation) },
      DrawCmd::ClipPop => DrawCmd::ClipPop,
   })
}

#[inline]
fn translate_rect(rect: RectF, translation: [f32; 2]) -> RectF
{
   RectF::new(rect.x + translation[0], rect.y + translation[1], rect.w, rect.h)
}

fn translate_rect_i(rect: RectI, translation: [f32; 2]) -> RectI
{
   let x0 = (rect.x as f32 + translation[0]).floor();
   let y0 = (rect.y as f32 + translation[1]).floor();
   let x1 = (rect.x.saturating_add(rect.w) as f32 + translation[0]).ceil();
   let y1 = (rect.y.saturating_add(rect.h) as f32 + translation[1]).ceil();
   RectI::new(saturating_i32(x0), saturating_i32(y0), saturating_i32(x1 - x0), saturating_i32(y1 - y0))
}

#[inline]
fn saturating_i32(value: f32) -> i32
{
   if value <= i32::MIN as f32 {
      i32::MIN
   } else if value >= i32::MAX as f32 {
      i32::MAX
   } else {
      value as i32
   }
}

#[inline]
fn color_with_opacity(color: Color, opacity: f32) -> Color
{
   Color { a: color.a * opacity, ..color }
}

#[inline]
fn rgba_with_opacity(rgba: u32, opacity: f32) -> u32
{
   if rgba == 0
   {
      return 0;
   }
   let alpha = ((rgba >> 24) & 0xff) as f32;
   let alpha = (alpha * opacity).round().clamp(0.0, 255.0) as u32;
   (rgba & 0x00ff_ffff) | (alpha << 24)
}

#[inline]
fn effect_with_opacity(effect: VisualEffect, opacity: f32) -> VisualEffect
{
   match effect {
      VisualEffect::UIKitDark => VisualEffect::DarkPopup {
         blur_intensity: 1.0,
         tint: Color::rgba(0.0, 0.0, 0.0, 0.90 * opacity),
      },
      VisualEffect::DarkPopup { blur_intensity, tint } => VisualEffect::DarkPopup {
         blur_intensity,
         tint: color_with_opacity(tint, opacity),
      },
   }
}
