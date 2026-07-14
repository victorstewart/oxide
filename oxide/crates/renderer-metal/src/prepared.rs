use metal::{
   ArgumentEncoderRef, Buffer, Device, DeviceRef, Library, MTLClearColor, MTLIndexType,
   MTLLoadAction, MTLPixelFormat, MTLPrimitiveType, MTLRenderStages, MTLResourceOptions,
   MTLResourceUsage, MTLStorageMode, MTLStoreAction, MTLTexture, MTLTextureType,
   MTLTextureUsage, RenderCommandEncoderRef, RenderPassDescriptor, RenderPipelineDescriptor,
   RenderPipelineState, TextureDescriptor, TextureRef,
};
use metal::foreign_types::ForeignTypeRef;
use oxide_renderer_api as api;
use std::collections::{HashMap, HashSet};

use super::{
   api_vertex_descriptor, append_remapped_indices_to_span, apply_scissor_dp,
   configure_layer_source_alpha_blend, configure_source_alpha_blend, effective_scissor_dp,
   intersect_scissor_dp, pack_image_params, pack_nine_slice_params, pack_rrect_params,
   pipeline_error, pipeline_function, pipeline_state, solid_primitive_for_index_count,
   solid_primitive_for_vertex_count, transparent_drawable_clear_enabled, MetalInitError,
   MetalRenderer, NineSliceGpuParams,
};

pub const DEFAULT_PREPARED_CACHE_BUDGET_BYTES: u64 = 32 * 1024 * 1024;
const STATIC_SOURCE_REVISION: u64 = 0xcbf2_9ce4_8422_2325;

pub(super) struct PreparedPipelines
{
   pub solid: RenderPipelineState,
   pub rrect: RenderPipelineState,
   pub rrect_opaque: RenderPipelineState,
   pub image: RenderPipelineState,
   pub image_opaque: RenderPipelineState,
   pub image_single: RenderPipelineState,
   pub image_single_opaque: RenderPipelineState,
   pub image_mesh: RenderPipelineState,
   pub image_mesh_opaque: RenderPipelineState,
   pub text: RenderPipelineState,
   pub text_opaque: RenderPipelineState,
   pub text_sdf: RenderPipelineState,
   pub text_sdf_opaque: RenderPipelineState,
}

impl PreparedPipelines
{
   pub fn new(device: &Device, library: &Library, format: MTLPixelFormat, sample_count: u32, layer: bool) -> Result<Self, MetalInitError>
   {
      Ok(Self {
         solid: prepared_pipeline(device, library, format, sample_count, layer, "prepared.solid", "v_prepared_solid", "f_solid", true)?,
         rrect: prepared_pipeline(device, library, format, sample_count, layer, "prepared.rrect", "v_prepared_inst_rect", "f_prepared_rrect", false)?,
         rrect_opaque: prepared_pipeline(device, library, format, sample_count, layer, "prepared.rrect_opaque", "v_prepared_inst_rect", "f_rrect", false)?,
         image: prepared_pipeline(device, library, format, sample_count, layer, "prepared.image", "v_prepared_inst_rect", "f_prepared_image", false)?,
         image_opaque: prepared_pipeline(device, library, format, sample_count, layer, "prepared.image_opaque", "v_prepared_inst_rect", "f_image", false)?,
         image_single: prepared_pipeline(device, library, format, sample_count, layer, "prepared.image_single", "v_prepared_inst_rect", "f_prepared_image_single", false)?,
         image_single_opaque: prepared_pipeline(device, library, format, sample_count, layer, "prepared.image_single_opaque", "v_prepared_inst_rect", "f_image_single", false)?,
         image_mesh: prepared_pipeline(device, library, format, sample_count, layer, "prepared.image_mesh", "v_prepared_text", "f_prepared_image_mesh", true)?,
         image_mesh_opaque: prepared_pipeline(device, library, format, sample_count, layer, "prepared.image_mesh_opaque", "v_prepared_text", "f_image_mesh", true)?,
         text: prepared_pipeline(device, library, format, sample_count, layer, "prepared.text", "v_prepared_text", "f_prepared_text", true)?,
         text_opaque: prepared_pipeline(device, library, format, sample_count, layer, "prepared.text_opaque", "v_prepared_text", "f_text", true)?,
         text_sdf: prepared_pipeline(device, library, format, sample_count, layer, "prepared.text_sdf", "v_prepared_text", "f_prepared_text_sdf", true)?,
         text_sdf_opaque: prepared_pipeline(device, library, format, sample_count, layer, "prepared.text_sdf_opaque", "v_prepared_text", "f_text_sdf", true)?,
      })
   }
}

fn prepared_pipeline(device: &Device, library: &Library, format: MTLPixelFormat, sample_count: u32, layer: bool, stage: &str, vertex: &str, fragment: &str, vertex_descriptor: bool) -> Result<RenderPipelineState, MetalInitError>
{
   let vertex = pipeline_function(library, stage, vertex)?;
   let fragment = pipeline_function(library, stage, fragment)?;
   let descriptor = RenderPipelineDescriptor::new();
   descriptor.set_vertex_function(Some(&vertex));
   descriptor.set_fragment_function(Some(&fragment));
   descriptor.set_sample_count(sample_count as u64);
   if vertex_descriptor
   {
      descriptor.set_vertex_descriptor(Some(api_vertex_descriptor()));
   }
   let attachment = descriptor.color_attachments().object_at(0)
      .ok_or_else(|| pipeline_error(stage, "missing color attachment zero"))?;
   attachment.set_pixel_format(format);
   if layer
   {
      configure_layer_source_alpha_blend(attachment);
   }
   else
   {
      configure_source_alpha_blend(attachment);
   }
   pipeline_state(device, stage, &descriptor)
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct PreparedChunkKey
{
   id: api::RenderChunkId,
   structural_revision: u64,
   geometry_revision: u64,
   resource_revision: u64,
   device_generation: u64,
   color_format: u64,
   sample_count: u32,
}

impl PreparedChunkKey
{
   fn new(renderer: &MetalRenderer, chunk: &api::RenderChunk) -> Self
   {
      let revisions = chunk.revisions();
      Self {
         id: chunk.id(),
         structural_revision: revisions.structural,
         geometry_revision: revisions.geometry,
         resource_revision: revisions.resource,
         device_generation: renderer.device_generation,
         color_format: renderer.color_format as u64,
         sample_count: renderer.sample_count,
      }
   }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct PreparedLayerKey
{
   id: u32,
   chunk: PreparedChunkKey,
   content_generation: u64,
   nested_generation: u64,
   dynamic_generation: u64,
   bounds: [u32; 4],
   scale: [u32; 2],
   opacity: u32,
   target_scale: u32,
   effect_outset: u32,
}

#[derive(Clone, Copy)]
pub(super) struct PreparedLayerFrame
{
   pub key: PreparedLayerKey,
   pub rect: api::RectF,
   pub local_uniform: PreparedInstanceUniform,
   pub width: u32,
   pub height: u32,
   pub refresh: bool,
   pub force_refresh: bool,
}

fn prepared_layer_frame(renderer: &MetalRenderer, layer: api::RenderLayerInstance, chunk: &api::RenderChunk, uniform: PreparedInstanceUniform, clip: Option<api::RectI>) -> Option<PreparedLayerFrame>
{
   if renderer.prepared_layer_pipelines.is_none() || clip.is_some()
   {
      return None;
   }
   let [scale_x, shear_y, shear_x, scale_y, translate_x, translate_y, ..] = uniform.values;
   if shear_x != 0.0 || shear_y != 0.0 || scale_x == 0.0 || scale_y == 0.0
   {
      return None;
   }
   let effect_outset = layer_effect_outset(chunk, layer.rect)?;
   let local_rect = api::RectF::new(
      layer.rect.x - effect_outset,
      layer.rect.y - effect_outset,
      layer.rect.w + effect_outset * 2.0,
      layer.rect.h + effect_outset * 2.0,
   );
   if ![
      local_rect.x, local_rect.y, local_rect.w, local_rect.h,
      scale_x, scale_y, translate_x, translate_y, renderer.target_scale,
   ].iter().all(|value| value.is_finite()) || local_rect.w <= 0.0 || local_rect.h <= 0.0
   {
      return None;
   }
   let transformed_x0 = scale_x * local_rect.x;
   let transformed_x1 = scale_x * (local_rect.x + local_rect.w);
   let transformed_y0 = scale_y * local_rect.y;
   let transformed_y1 = scale_y * (local_rect.y + local_rect.h);
   let min_x = transformed_x0.min(transformed_x1);
   let min_y = transformed_y0.min(transformed_y1);
   let width_dp = (transformed_x1 - transformed_x0).abs();
   let height_dp = (transformed_y1 - transformed_y0).abs();
   let target_scale = renderer.target_scale.max(1.0);
   let width = (width_dp * target_scale).ceil();
   let height = (height_dp * target_scale).ceil();
   if !width.is_finite() || !height.is_finite()
      || width < 1.0 || height < 1.0
      || width > u32::MAX as f32 || height > u32::MAX as f32
   {
      return None;
   }
   let rect = api::RectF::new(min_x + translate_x, min_y + translate_y, width_dp, height_dp);
   let opacity = uniform.values[8];
   let local_uniform = PreparedInstanceUniform {
      values: [
         scale_x, 0.0, 0.0, scale_y,
         -min_x, -min_y, width_dp, height_dp,
         opacity, if scale_x == 1.0 && scale_y == 1.0 { 1.0 } else { 0.0 }, 0.0, 0.0,
      ],
   };
   let revisions = chunk.revisions();
   Some(PreparedLayerFrame {
      key: PreparedLayerKey {
         id: layer.id,
         chunk: PreparedChunkKey::new(renderer, chunk),
         content_generation: revisions.geometry,
         nested_generation: revisions.structural,
         dynamic_generation: revisions.dynamic_properties,
         bounds: [
            layer.rect.x.to_bits(), layer.rect.y.to_bits(),
            layer.rect.w.to_bits(), layer.rect.h.to_bits(),
         ],
         scale: [scale_x.to_bits(), scale_y.to_bits()],
         opacity: opacity.to_bits(),
         target_scale: target_scale.to_bits(),
         effect_outset: effect_outset.to_bits(),
      },
      rect,
      local_uniform,
      width: width as u32,
      height: height as u32,
      refresh: false,
      force_refresh: layer.dirty,
   })
}

fn layer_effect_outset(chunk: &api::RenderChunk, rect: api::RectF) -> Option<f32>
{
   let mut outset = 0.0_f32;
   for effect in chunk.effect_dependencies()
   {
      let bounds = match effect.sample_bounds
      {
         api::RenderSpatialBounds::Empty => continue,
         api::RenderSpatialBounds::Finite(bounds) => bounds,
         api::RenderSpatialBounds::Unbounded => return None,
      };
      outset = outset
         .max(rect.x - bounds.x)
         .max(rect.y - bounds.y)
         .max(bounds.x + bounds.w - (rect.x + rect.w))
         .max(bounds.y + bounds.h - (rect.y + rect.h));
   }
   outset.is_finite().then_some(outset.max(0.0))
}

fn prepared_layer_matches(renderer: &MetalRenderer, layer: PreparedLayerFrame, chunk: &api::RenderChunk) -> bool
{
   renderer.layers.get(&layer.key.id).is_some_and(|entry| {
      entry.w == layer.width
         && entry.h == layer.height
         && entry.prepared_key == Some(layer.key)
         && entry.resources.as_slice() == chunk.resource_dependencies()
   })
}

fn prepared_layer_plan_matches(renderer: &MetalRenderer, layer: PreparedLayerFrame) -> bool
{
   !layer.force_refresh && renderer.layers.get(&layer.key.id).is_some_and(|entry| {
      entry.w == layer.width
         && entry.h == layer.height
         && entry.prepared_key == Some(layer.key)
   })
}

#[derive(Clone, Copy)]
#[repr(transparent)]
pub(super) struct PreparedInstanceUniform
{
   pub values: [f32; 12],
}

#[derive(Clone, Copy)]
struct PreparedDynamicUniform
{
   matrix: [f32; 4],
   translation: [f32; 2],
   opacity: f32,
   source_revision: u64,
}

impl PreparedInstanceUniform
{
   fn dynamic<const TRACK_REVISION: bool>(snapshot: &api::RenderSnapshot, property_slots: &[api::RenderPropertySlotId]) -> Option<PreparedDynamicUniform>
   {
      let mut matrix = [1.0_f32, 0.0, 0.0, 1.0];
      let mut translation = [0.0_f32, 0.0];
      let mut opacity = 1.0_f32;
      let mut source_revision = 0xcbf2_9ce4_8422_2325_u64;
      for id in property_slots.iter().copied()
      {
         let index = snapshot.properties().binary_search_by_key(&id.0, |property| property.id.0).ok()?;
         let property = snapshot.properties()[index];
         if TRACK_REVISION
         {
            source_revision = source_revision
               .rotate_left(17)
               ^ (u64::from(id.0) << 32)
               ^ property.revision;
         }
         match property.value
         {
            api::RenderPropertyValue::Transform(transform) =>
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
            api::RenderPropertyValue::Opacity(value) => opacity *= value,
         }
      }
      Some(PreparedDynamicUniform { matrix, translation, opacity: opacity.clamp(0.0, 1.0), source_revision })
   }

   fn from_dynamic(dynamic: PreparedDynamicUniform, origin: [f32; 2], viewport: [f32; 2]) -> Option<Self>
   {
      let translation = [
         dynamic.matrix[0] * origin[0] + dynamic.matrix[2] * origin[1] + dynamic.translation[0],
         dynamic.matrix[1] * origin[0] + dynamic.matrix[3] * origin[1] + dynamic.translation[1],
      ];
      let values = [
         dynamic.matrix[0], dynamic.matrix[1], dynamic.matrix[2], dynamic.matrix[3],
         translation[0], translation[1], viewport[0], viewport[1],
         dynamic.opacity, if dynamic.matrix == [1.0, 0.0, 0.0, 1.0] { 1.0 } else { 0.0 }, 0.0, 0.0,
      ];
      values.iter().all(|value| value.is_finite()).then_some(Self { values })
   }

   fn from_resolved(instance: &api::RenderResolvedInstance, viewport: [f32; 2]) -> Option<Self>
   {
      let transform = instance.transform;
      let values = [
         transform[0], transform[1], transform[2], transform[3],
         transform[4], transform[5], viewport[0], viewport[1],
         instance.opacity, if transform[..4] == [1.0, 0.0, 0.0, 1.0] { 1.0 } else { 0.0 }, 0.0, 0.0,
      ];
      values.iter().all(|value| value.is_finite()).then_some(Self { values })
   }
}

pub(super) struct PreparedChunkCache
{
   entries: HashMap<api::RenderChunkId, CachedPreparedChunk>,
   budget_bytes: u64,
   resident_bytes: u64,
   logical_resident_bytes: u64,
   generation: u64,
   evictions: u64,
}

struct CachedPreparedChunk
{
   key: PreparedChunkKey,
   chunk: PreparedChunk,
}

impl Default for PreparedChunkCache
{
   fn default() -> Self
   {
      Self {
         entries: HashMap::new(),
         budget_bytes: DEFAULT_PREPARED_CACHE_BUDGET_BYTES,
         resident_bytes: 0,
         logical_resident_bytes: 0,
         generation: 0,
         evictions: 0,
      }
   }
}

impl PreparedChunkCache
{
   pub fn budget_bytes(&self) -> u64
   {
      self.budget_bytes
   }

   pub fn resident_bytes(&self) -> u64
   {
      self.resident_bytes
   }

   pub fn logical_resident_bytes(&self) -> u64
   {
      self.logical_resident_bytes
   }

   pub fn len(&self) -> usize
   {
      self.entries.len()
   }

   pub fn take_evictions(&mut self) -> u64
   {
      core::mem::take(&mut self.evictions)
   }

   pub fn set_budget_bytes(&mut self, budget_bytes: u64)
   {
      self.budget_bytes = budget_bytes;
      self.enforce_budget(None);
   }

   pub fn clear(&mut self)
   {
      self.evictions = self.evictions.saturating_add(self.entries.len() as u64);
      self.entries.clear();
      self.resident_bytes = 0;
      self.logical_resident_bytes = 0;
   }

   pub fn invalidate_resource(&mut self, image: api::ImageHandle)
   {
      let mut removed_bytes = 0_u64;
      let mut removed_logical_bytes = 0_u64;
      let mut removed = 0_u64;
      self.entries.retain(|_, entry| {
         let keep = !entry.chunk.resources.iter().any(|dependency| dependency.image == image);
         if !keep
         {
            removed_bytes = removed_bytes.saturating_add(entry.chunk.byte_size);
            removed_logical_bytes = removed_logical_bytes.saturating_add(entry.chunk.logical_byte_size);
            removed = removed.saturating_add(1);
         }
         keep
      });
      self.resident_bytes = self.resident_bytes.saturating_sub(removed_bytes);
      self.logical_resident_bytes = self.logical_resident_bytes.saturating_sub(removed_logical_bytes);
      self.evictions = self.evictions.saturating_add(removed);
   }

   pub fn get_or_prepare(&mut self, renderer: &MetalRenderer, chunk: &api::RenderChunk) -> Option<PreparedLookup>
   {
      self.generation = self.generation.saturating_add(1);
      let generation = self.generation;
      let key = PreparedChunkKey::new(renderer, chunk);
      let compatible = self.entries.get(&key.id)
         .is_some_and(|entry| entry.key == key && entry.chunk.resources_compatible(renderer));
      if compatible
      {
         if let Some(entry) = self.entries.get_mut(&key.id)
         {
            entry.chunk.last_used_generation = generation;
         }
         return self.entries.get(&key.id).map(|entry| PreparedLookup {
            key,
            hit: true,
            upload_bytes: entry.chunk.logical_byte_size,
            buffer_count: entry.chunk.buffer_count,
            command_count: entry.chunk.command_count,
         });
      }
      self.remove_chunk_id(key.id);
      let mut entry = PreparedChunk::new(renderer, chunk)?;
      entry.last_used_generation = generation;
      let bytes = entry.byte_size;
      if bytes > self.budget_bytes
      {
         return None;
      }
      self.resident_bytes = self.resident_bytes.saturating_add(bytes);
      self.logical_resident_bytes = self.logical_resident_bytes.saturating_add(entry.logical_byte_size);
      self.entries.insert(key.id, CachedPreparedChunk { key, chunk: entry });
      self.enforce_budget(Some(key));
      self.entries.get(&key.id).map(|entry| PreparedLookup {
         key,
         hit: false,
         upload_bytes: entry.chunk.logical_byte_size,
         buffer_count: entry.chunk.buffer_count,
         command_count: entry.chunk.command_count,
      })
   }

   pub fn get(&self, key: PreparedChunkKey) -> Option<&PreparedChunk>
   {
      self.entries.get(&key.id)
         .filter(|entry| entry.key == key)
         .map(|entry| &entry.chunk)
   }

   fn remove_chunk_id(&mut self, id: api::RenderChunkId)
   {
      if let Some(entry) = self.entries.remove(&id)
      {
         self.resident_bytes = self.resident_bytes.saturating_sub(entry.chunk.byte_size);
         self.logical_resident_bytes = self.logical_resident_bytes.saturating_sub(entry.chunk.logical_byte_size);
         self.evictions = self.evictions.saturating_add(1);
      }
   }

   fn enforce_budget(&mut self, protected: Option<PreparedChunkKey>)
   {
      while self.resident_bytes > self.budget_bytes
      {
         let candidate = self.entries.iter()
            .filter(|(_, entry)| Some(entry.key) != protected)
            .min_by_key(|(_, entry)| entry.chunk.last_used_generation)
            .map(|(id, _)| *id);
         let Some(candidate) = candidate else { break };
         if let Some(entry) = self.entries.remove(&candidate)
         {
            self.resident_bytes = self.resident_bytes.saturating_sub(entry.chunk.byte_size);
            self.logical_resident_bytes = self.logical_resident_bytes.saturating_sub(entry.chunk.logical_byte_size);
            self.evictions = self.evictions.saturating_add(1);
         }
      }
   }
}

#[derive(Clone, Copy)]
pub(super) struct PreparedLookup
{
   pub key: PreparedChunkKey,
   pub hit: bool,
   pub upload_bytes: u64,
   pub buffer_count: u32,
   pub command_count: u64,
}

#[derive(Clone, Copy)]
pub(super) struct PreparedFrameInstance
{
   pub key: PreparedChunkKey,
   pub uniform: PreparedInstanceUniform,
   pub clip: Option<api::RectI>,
   pub local_damage: Option<api::RectF>,
   pub layer: Option<PreparedLayerFrame>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PreparedTarget
{
   Main,
   Layer,
   ExactLayer,
}

pub(super) struct PreparedPropertyCache
{
   entries: Vec<PreparedPropertyEntry>,
   pending_indices: Vec<usize>,
   pending_uniforms: Vec<PreparedInstanceUniform>,
   last_properties: Vec<(u32, u64)>,
   last_uniform_property_revision: Option<u64>,
}

struct PreparedPropertyEntry
{
   uniform: PreparedInstanceUniform,
   source_revision: u64,
   revision: u64,
   ring_revisions: [u64; super::MAX_FRAME_RESOURCE_DEPTH],
}

impl Default for PreparedPropertyCache
{
   fn default() -> Self
   {
      Self {
         entries: Vec::new(),
         pending_indices: Vec::new(),
         pending_uniforms: Vec::new(),
         last_properties: Vec::new(),
         last_uniform_property_revision: None,
      }
   }
}

impl PreparedPropertyCache
{
   fn begin_frame(&mut self, properties: &[api::RenderPropertySlot], uniform_revision: Option<u64>) -> bool
   {
      self.pending_indices.clear();
      self.pending_uniforms.clear();
      let write_all = matches!(
         (self.last_uniform_property_revision, uniform_revision),
         (Some(previous), Some(current)) if previous != current
      );
      self.last_uniform_property_revision = uniform_revision;
      if write_all
      {
         self.entries.clear();
         return true;
      }
      if self.last_properties.len() != properties.len()
      {
         self.last_properties.clear();
         self.last_properties.extend(properties.iter().map(|property| (property.id.0, property.revision)));
         self.entries.clear();
         return false;
      }
      let mut all_changed = !properties.is_empty();
      let mut layout_matches = true;
      for (last, property) in self.last_properties.iter_mut().zip(properties)
      {
         layout_matches &= last.0 == property.id.0;
         all_changed &= last.1 != property.revision;
         *last = (property.id.0, property.revision);
      }
      if !layout_matches || all_changed
      {
         self.entries.clear();
      }
      layout_matches && all_changed
   }

   fn resolve(&mut self, index: usize, uniform: PreparedInstanceUniform, source_revision: u64, slot: usize)
   {
      while self.entries.len() <= index
      {
         self.entries.push(PreparedPropertyEntry {
            uniform,
            source_revision,
            revision: 1,
            ring_revisions: [0; super::MAX_FRAME_RESOURCE_DEPTH],
         });
      }
      let entry = &mut self.entries[index];
      if entry.source_revision != source_revision || entry.uniform.values != uniform.values
      {
         entry.uniform = uniform;
         entry.source_revision = source_revision;
         entry.revision = entry.revision.wrapping_add(1).max(1);
      }
      if entry.ring_revisions[slot] != entry.revision
      {
         self.pending_indices.push(index);
         self.pending_uniforms.push(uniform);
      }
   }

   fn truncate(&mut self, len: usize)
   {
      self.entries.truncate(len);
   }
}

pub(super) struct PreparedChunk
{
   pub source: api::RenderChunk,
   pub operations: Vec<PreparedOperation>,
   command_operations: Vec<u32>,
   pub byte_size: u64,
   pub logical_byte_size: u64,
   pub buffer_count: u32,
   pub command_count: u64,
   has_opaque_rrect: bool,
   has_translucent_rrect: bool,
   resources: Vec<api::RenderResourceDependency>,
   last_used_generation: u64,
}

impl PreparedChunk
{
   fn new(renderer: &MetalRenderer, chunk: &api::RenderChunk) -> Option<Self>
   {
      if chunk.ordering().has_layer
      {
         return None;
      }
      if !chunk.resource_dependencies().iter().all(|dependency| {
         renderer.image_generations.get(&dependency.image.0).copied() == Some(dependency.generation)
      })
      {
         return None;
      }
      let list = chunk.draw_list();
      let mut operations = Vec::new();
      let mut byte_size = 0_u64;
      let mut buffer_count = 0_u32;
      let mut has_opaque_rrect = false;
      let mut has_translucent_rrect = false;
      let mut index = 0_usize;
      while index < list.items.len()
      {
         match &list.items[index]
         {
            api::DrawCmd::RRect { .. } =>
            {
               let start = index;
               while let Some(api::DrawCmd::RRect { color, .. }) = list.items.get(index)
               {
                  if color.a == 1.0
                  {
                     has_opaque_rrect = true;
                  }
                  else
                  {
                     has_translucent_rrect = true;
                  }
                  index += 1;
               }
               let (operation, bytes) = prepare_rrects(
                  renderer.device.as_ref(),
                  &list.items[start..index],
                  start as u32,
               )?;
               operations.push(operation);
               byte_size = byte_size.saturating_add(bytes);
               buffer_count = buffer_count.saturating_add(1);
            }
            api::DrawCmd::Image { .. } =>
            {
               let (operation, next, bytes) = prepare_images(renderer, list, index)?;
               operations.push(operation);
               byte_size = byte_size.saturating_add(bytes);
               buffer_count = buffer_count.saturating_add(1 + u32::from(renderer.use_image_arg_buffer));
               index = next;
            }
            api::DrawCmd::GlyphRun { .. } =>
            {
               let (operation, next, bytes) = prepare_glyphs(renderer.device.as_ref(), list, index)?;
               operations.push(operation);
               byte_size = byte_size.saturating_add(bytes);
               buffer_count = buffer_count.saturating_add(3);
               index = next;
            }
            api::DrawCmd::ImageMesh { .. } =>
            {
               let (operation, bytes) = prepare_image_mesh(renderer, list, index)?;
               let has_indices = matches!(operation, PreparedOperation::ImageMesh { indices: Some(_), .. });
               operations.push(operation);
               byte_size = byte_size.saturating_add(bytes);
               buffer_count = buffer_count.saturating_add(2 + u32::from(has_indices));
               index += 1;
            }
            api::DrawCmd::Solid { .. } =>
            {
               let (operation, bytes) = prepare_solid(renderer.device.as_ref(), list, index)?;
               let has_indices = matches!(operation, PreparedOperation::Solid { indices: Some(_), .. });
               operations.push(operation);
               byte_size = byte_size.saturating_add(bytes);
               buffer_count = buffer_count.saturating_add(2 + u32::from(has_indices));
               index += 1;
            }
            api::DrawCmd::ClipPush { rect } =>
            {
               operations.push(PreparedOperation::ClipPush(*rect));
               index += 1;
            }
            api::DrawCmd::ClipPop =>
            {
               operations.push(PreparedOperation::ClipPop);
               index += 1;
            }
            _ => return None,
         }
      }
      let mut command_operations = vec![u32::MAX; list.items.len()];
      for (operation_index, operation) in operations.iter().enumerate()
      {
         let operation_index = operation_index as u32;
         match operation
         {
            PreparedOperation::RRects { first_command, count, .. }
            | PreparedOperation::Images { first_command, count, .. } =>
            {
               let begin = *first_command as usize;
               let end = begin.saturating_add(*count as usize).min(command_operations.len());
               command_operations[begin..end].fill(operation_index);
            }
            PreparedOperation::Glyphs { draws, .. } =>
            {
               for draw in draws
               {
                  if let Some(entry) = command_operations.get_mut(draw.command as usize)
                  {
                     *entry = operation_index;
                  }
               }
            }
            PreparedOperation::ImageMesh { command, .. }
            | PreparedOperation::Solid { command, .. } =>
            {
               if let Some(entry) = command_operations.get_mut(*command as usize)
               {
                  *entry = operation_index;
               }
            }
            PreparedOperation::ClipPush(_) | PreparedOperation::ClipPop => {}
         }
      }
      let logical_byte_size = byte_size;
      byte_size = operations.iter().fold(0_u64, |bytes, operation| {
         bytes.saturating_add(operation_allocated_bytes(operation))
      }).saturating_add(
         (command_operations.capacity() as u64)
            .saturating_mul(core::mem::size_of::<u32>() as u64),
      );
      Some(Self {
         source: chunk.clone(),
         operations,
         command_operations,
         byte_size,
         logical_byte_size,
         buffer_count,
         command_count: list.items.len() as u64,
         has_opaque_rrect,
         has_translucent_rrect,
         resources: chunk.resource_dependencies().to_vec(),
         last_used_generation: 0,
      })
   }

   fn resources_compatible(&self, renderer: &MetalRenderer) -> bool
   {
      self.resources.iter().all(|dependency| {
         renderer.image_generations.get(&dependency.image.0).copied() == Some(dependency.generation)
      })
   }

   fn operation_for_command(&self, command: u32) -> Option<(u32, &PreparedOperation)>
   {
      let operation = *self.command_operations.get(command as usize)?;
      (operation != u32::MAX).then(|| {
         (operation, &self.operations[operation as usize])
      })
   }

   fn layer_target(&self, opacity: f32) -> Option<PreparedTarget>
   {
      if opacity != 1.0 || !self.has_opaque_rrect
      {
         return Some(PreparedTarget::Layer);
      }
      if self.has_translucent_rrect
      {
         return None;
      }
      Some(PreparedTarget::ExactLayer)
   }

}

pub(super) enum PreparedOperation
{
   RRects { params: Buffer, first_command: u32, count: u64 },
   Images {
      params: Buffer,
      argument_buffer: Option<Buffer>,
      handles: Vec<api::ImageHandle>,
      instance_handles: Vec<api::ImageHandle>,
      first_command: u32,
      count: u64,
   },
   Glyphs {
      vertices: Buffer,
      indices: Buffer,
      uniforms: Buffer,
      draws: Vec<PreparedGlyphDraw>,
      atlas: api::ImageHandle,
      sdf: bool,
   },
   ImageMesh {
      vertices: Buffer,
      indices: Option<Buffer>,
      uniform: Buffer,
      texture: api::ImageHandle,
      command: u32,
      vertex_count: u64,
      index_count: u64,
   },
   Solid {
      vertices: Buffer,
      indices: Option<Buffer>,
      uniform: Buffer,
      command: u32,
      vertex_count: u64,
      index_count: u64,
   },
   ClipPush(api::RectI),
   ClipPop,
}

pub(super) struct PreparedGlyphDraw
{
   pub command: u32,
   pub vertex_offset: u64,
   pub index_offset: u64,
   pub uniform_offset: u64,
   pub index_count: u64,
}

fn operation_allocated_bytes(operation: &PreparedOperation) -> u64
{
   let allocated = |buffer: &Buffer| buffer.allocated_size() as u64;
   match operation
   {
      PreparedOperation::RRects { params, .. } => allocated(params),
      PreparedOperation::Images { params, argument_buffer, .. } => allocated(params)
         .saturating_add(argument_buffer.as_ref().map_or(0, allocated)),
      PreparedOperation::Glyphs { vertices, indices, uniforms, .. } => allocated(vertices)
         .saturating_add(allocated(indices))
         .saturating_add(allocated(uniforms)),
      PreparedOperation::ImageMesh { vertices, indices, uniform, .. } => allocated(vertices)
         .saturating_add(indices.as_ref().map_or(0, allocated))
         .saturating_add(allocated(uniform)),
      PreparedOperation::Solid { vertices, indices, uniform, .. } => allocated(vertices)
         .saturating_add(indices.as_ref().map_or(0, allocated))
         .saturating_add(allocated(uniform)),
      PreparedOperation::ClipPush(_) | PreparedOperation::ClipPop => 0,
   }
}

fn prepare_rrects(device: &DeviceRef, commands: &[api::DrawCmd], first_command: u32) -> Option<(PreparedOperation, u64)>
{
   let mut params = Vec::with_capacity(commands.len());
   for command in commands
   {
      let api::DrawCmd::RRect { rect, radii, color } = command else { return None };
      params.push(pack_rrect_params(*rect, *radii, *color));
   }
   let params = buffer_from_slice(device, &params)?;
   let bytes = params.length();
   Some((PreparedOperation::RRects { params, first_command, count: commands.len() as u64 }, bytes))
}

fn prepare_images(renderer: &MetalRenderer, list: &api::DrawList, start: usize) -> Option<(PreparedOperation, usize, u64)>
{
   let mut slots = HashMap::<u32, u32>::new();
   let mut handles = Vec::new();
   let mut instance_handles = Vec::new();
   let mut params = Vec::new();
   let mut index = start;
   while let Some(api::DrawCmd::Image { tex, dst, src, alpha }) = list.items.get(index)
   {
      let texture = renderer.images.get(&tex.0)?;
      let slot = if let Some(slot) = slots.get(&tex.0).copied()
      {
         slot
      }
      else
      {
         if handles.len() as u32 == super::IMAGE_ARG_TEXTURE_SLOTS
         {
            break;
         }
         let slot = handles.len() as u32;
         slots.insert(tex.0, slot);
         handles.push(*tex);
         slot
      };
      params.push(pack_image_params(
         *dst,
         *src,
         [texture.width() as f32, texture.height() as f32],
         alpha.clamp(0.0, 1.0),
         slot,
      ));
      instance_handles.push(*tex);
      index += 1;
   }
   if params.is_empty()
   {
      return None;
   }
   let params = buffer_from_slice(renderer.device.as_ref(), &params)?;
   let argument_buffer = if renderer.use_image_arg_buffer
   {
      Some(prepare_image_argument_buffer(renderer, &handles)?)
   }
   else
   {
      None
   };
   let mut bytes = params.length();
   if let Some(buffer) = argument_buffer.as_ref()
   {
      bytes = bytes.saturating_add(buffer.length());
   }
   Some((PreparedOperation::Images {
      params,
      argument_buffer,
      handles,
      instance_handles,
      first_command: start as u32,
      count: (index - start) as u64,
   }, index, bytes))
}

fn prepare_image_argument_buffer(renderer: &MetalRenderer, handles: &[api::ImageHandle]) -> Option<Buffer>
{
   let encoder = renderer.img_arg.as_ref()?;
   let length = renderer.img_arg_stride.max(1);
   let buffer = renderer.device.new_buffer(length as u64, MTLResourceOptions::StorageModeShared);
   encode_image_argument_buffer(encoder.as_ref(), &buffer, renderer, handles);
   Some(buffer)
}

fn encode_image_argument_buffer(encoder: &ArgumentEncoderRef, buffer: &Buffer, renderer: &MetalRenderer, handles: &[api::ImageHandle])
{
   encoder.set_argument_buffer(buffer, 0);
   for (index, handle) in handles.iter().copied().enumerate()
   {
      if let Some(texture) = renderer.images.get(&handle.0)
      {
         encoder.set_texture(index as u64, texture);
      }
   }
}

fn prepare_glyphs(device: &DeviceRef, list: &api::DrawList, start: usize) -> Option<(PreparedOperation, usize, u64)>
{
   let first = match list.items.get(start)?
   {
      api::DrawCmd::GlyphRun { run } => *run,
      _ => return None,
   };
   let mut vertices = Vec::<api::Vertex>::new();
   let mut indices = Vec::<u16>::new();
   let mut uniforms = Vec::<[f32; 4]>::new();
   let mut draws = Vec::new();
   let mut index = start;
   while let Some(api::DrawCmd::GlyphRun { run }) = list.items.get(index)
   {
      if run.atlas != first.atlas || run.sdf != first.sdf
      {
         break;
      }
      let source_vertices = list.vertices.get(run.vb.offset as usize..run.vb.offset as usize + run.vb.len as usize)?;
      let source_indices = list.indices.get(run.ib.offset as usize..run.ib.offset as usize + run.ib.len as usize)?;
      let vertex_offset = vertices.len().saturating_mul(core::mem::size_of::<api::Vertex>()) as u64;
      let index_offset = indices.len().saturating_mul(core::mem::size_of::<u16>()) as u64;
      let uniform_offset = uniforms.len().saturating_mul(core::mem::size_of::<[f32; 4]>()) as u64;
      vertices.extend_from_slice(source_vertices);
      let index_count = append_remapped_indices_to_span(
         source_indices,
         run.vb.offset,
         run.vb.len,
         0,
         &mut indices,
      )? as u64;
      uniforms.push([run.color.r, run.color.g, run.color.b, run.color.a]);
      draws.push(PreparedGlyphDraw {
         command: index as u32,
         vertex_offset,
         index_offset,
         uniform_offset,
         index_count,
      });
      index += 1;
   }
   let vertices = buffer_from_slice(device, &vertices)?;
   let indices = buffer_from_slice(device, &indices)?;
   let uniforms = buffer_from_slice(device, &uniforms)?;
   let bytes = vertices.length().saturating_add(indices.length()).saturating_add(uniforms.length());
   Some((PreparedOperation::Glyphs {
      vertices,
      indices,
      uniforms,
      draws,
      atlas: first.atlas,
      sdf: first.sdf,
   }, index, bytes))
}

fn prepare_image_mesh(renderer: &MetalRenderer, list: &api::DrawList, index: usize) -> Option<(PreparedOperation, u64)>
{
   let api::DrawCmd::ImageMesh { tex, vb, ib, alpha } = list.items.get(index)? else { return None };
   let _ = renderer.images.get(&tex.0)?;
   let vertices = list.vertices.get(vb.offset as usize..vb.offset as usize + vb.len as usize)?;
   let source_indices = list.indices.get(ib.offset as usize..ib.offset as usize + ib.len as usize)?;
   let vertex_buffer = buffer_from_slice(renderer.device.as_ref(), vertices)?;
   let uniform = buffer_from_slice(renderer.device.as_ref(), &[[1.0_f32, 1.0, 1.0, alpha.clamp(0.0, 1.0)]])?;
   let index_buffer = if source_indices.is_empty()
   {
      None
   }
   else
   {
      let mut indices = Vec::with_capacity(source_indices.len());
      append_remapped_indices_to_span(source_indices, vb.offset, vb.len, 0, &mut indices)?;
      Some(buffer_from_slice(renderer.device.as_ref(), &indices)?)
   };
   let bytes = vertex_buffer.length()
      .saturating_add(uniform.length())
      .saturating_add(index_buffer.as_ref().map_or(0, |buffer| buffer.length()));
   Some((PreparedOperation::ImageMesh {
      vertices: vertex_buffer,
      indices: index_buffer,
      uniform,
      texture: *tex,
      command: index as u32,
      vertex_count: vb.len as u64,
      index_count: ib.len as u64,
   }, bytes))
}

fn prepare_solid(device: &DeviceRef, list: &api::DrawList, index: usize) -> Option<(PreparedOperation, u64)>
{
   let api::DrawCmd::Solid { vb, ib, color } = list.items.get(index)? else { return None };
   let vertices = list.vertices.get(vb.offset as usize..vb.offset as usize + vb.len as usize)?;
   let source_indices = list.indices.get(ib.offset as usize..ib.offset as usize + ib.len as usize)?;
   let vertex_buffer = buffer_from_slice(device, vertices)?;
   let uniform = buffer_from_slice(device, &[[color.r, color.g, color.b, color.a]])?;
   let index_buffer = if source_indices.is_empty()
   {
      None
   }
   else
   {
      let mut indices = Vec::with_capacity(source_indices.len());
      append_remapped_indices_to_span(source_indices, vb.offset, vb.len, 0, &mut indices)?;
      Some(buffer_from_slice(device, &indices)?)
   };
   let bytes = vertex_buffer.length()
      .saturating_add(uniform.length())
      .saturating_add(index_buffer.as_ref().map_or(0, |buffer| buffer.length()));
   Some((PreparedOperation::Solid {
      vertices: vertex_buffer,
      indices: index_buffer,
      uniform,
      command: index as u32,
      vertex_count: vb.len as u64,
      index_count: ib.len as u64,
   }, bytes))
}

fn buffer_from_slice<T>(device: &DeviceRef, values: &[T]) -> Option<Buffer>
{
   if values.is_empty()
   {
      return None;
   }
   let length = values.len().checked_mul(core::mem::size_of::<T>())?;
   Some(device.new_buffer_with_data(
      values.as_ptr().cast(),
      length as u64,
      MTLResourceOptions::StorageModeShared,
   ))
}

impl MetalRenderer
{
   /// Encodes an immutable render snapshot through persistent prepared buffers when supported.
   /// Unsupported snapshot structure uses the checked retained-capacity flat adapter.
   pub fn encode_snapshot(&mut self, snapshot: &api::RenderSnapshot) -> Result<(), api::RenderSnapshotError>
   {
      if self.prepared_pipelines.is_none()
      {
         return self.encode_snapshot_flat(snapshot);
      }
      if self.frame_backpressure_skipped || self.target_tex.is_none()
      {
         return Ok(());
      }
      if self.submit_error_flag.load(std::sync::atomic::Ordering::Acquire)
      {
         return Ok(());
      }

      let started_at = std::time::Instant::now();
      let viewport = [
         self.target_w as f32 / self.target_scale.max(1.0),
         self.target_h as f32 / self.target_scale.max(1.0),
      ];
      let damage_requested = self.sample_count == 1
         && self.damage_enabled
         && self.frame_scissor_dp.is_some()
         && self.frame_damage_pct < self.damage_use_thresh;
      if !self.frame_color_initialized && self.persistent_target_valid && self.persistent_target_policy != 0
      {
         self.persistent_target_valid = false;
      }
      let use_damage = damage_requested && self.persistent_target_valid;
      if damage_requested && !self.persistent_target_valid
      {
         self.acc_damage_forced_full_refreshes = self.acc_damage_forced_full_refreshes.saturating_add(1);
      }
      let static_instances = if use_damage { None } else { snapshot.precomputed_resolved_instances() };
      let mut cache = core::mem::take(&mut self.prepared_chunks);
      let mut property_cache = core::mem::take(&mut self.prepared_property_cache);
      let mut plan = core::mem::take(&mut self.prepared_frame_plan);
      let mut layer_frame_keys = core::mem::take(&mut self.prepared_layer_frame_keys);
      layer_frame_keys.clear();
      let mut damage_instances = core::mem::take(&mut self.prepared_damage_instances);
      let reuse_static_plan = !use_damage
         && static_instances.is_some()
         && self.prepared_frame_snapshot.as_ref().is_some_and(|cached| cached.ptr_eq(snapshot))
         && self.prepared_frame_viewport == viewport
         && plan.len() as u64 == snapshot.instance_count()
         && plan.iter().all(|instance| {
            instance.layer.map_or_else(
               || cache.get(instance.key).is_some(),
               |layer| prepared_layer_plan_matches(self, layer),
            )
         });
      if !reuse_static_plan
      {
         plan.clear();
         self.prepared_frame_snapshot = None;
         self.prepared_frame_keys.clear();
      }
      let write_all_properties = property_cache.begin_frame(
         snapshot.properties(),
         snapshot.uniform_property_revision(),
      );
      let slot = self.current_frame_slot();
      let mut supported = true;
      let mut hits = 0_u64;
      let mut misses = 0_u64;
      let mut commands_lowered = 0_u64;
      let mut upload_bytes = 0_u64;
      let mut property_upload_bytes = 0_u64;
      let mut property_records_updated = 0_u32;
      let mut resource_creates = 0_u32;
      let mut chunk_rebuilds = 0_u64;
      let mut layer_hits = 0_u32;
      let mut layer_misses = 0_u32;
      if reuse_static_plan
      {
         for instance in &mut plan
         {
            if let Some(layer) = instance.layer.as_mut()
            {
               layer.refresh = false;
            }
         }
         hits = plan.len() as u64;
         layer_hits = plan.iter().filter(|instance| instance.layer.is_some()).count()
            .min(u32::MAX as usize) as u32;
         self.acc_prepared_plan_reuses = self.acc_prepared_plan_reuses.saturating_add(1);
      }
      if use_damage
      {
         let query_started = std::time::Instant::now();
         let stats = snapshot.query_damage_instances(self.frame_scissor_dp.unwrap(), &mut damage_instances);
         self.acc_damage_query_ns = self.acc_damage_query_ns.saturating_add(
            query_started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64,
         );
         self.acc_damage_instances_visited = self.acc_damage_instances_visited.saturating_add(stats.entries_visited);
         self.acc_damage_instances_matched = self.acc_damage_instances_matched.saturating_add(stats.entries_matched);
      }
      let mut prepare_instance = |instance: &api::RenderChunkInstance, uniform: PreparedInstanceUniform, source_revision: u64, clip: Option<api::RectI>, local_damage: Option<api::RectF>| {
         if let Some(layer) = instance.layer
         {
            let Some(mut layer) = prepared_layer_frame(self, layer, &instance.chunk, uniform, clip) else
            {
               return false;
            };
            let duplicate = match layer_frame_keys.insert(layer.key.id, layer.key)
            {
               None => false,
               Some(key) if key == layer.key => true,
               Some(_) => return false,
            };
            let hit = duplicate
               || !layer.force_refresh && prepared_layer_matches(self, layer, &instance.chunk);
            layer.refresh = !hit;
            if hit
            {
               hits = hits.saturating_add(1);
               layer_hits = layer_hits.saturating_add(1);
            }
            else
            {
               let Some(lookup) = cache.get_or_prepare(self, &instance.chunk) else
               {
                  return false;
               };
               let Some(layer_target) = cache.get(lookup.key)
                  .and_then(|chunk| chunk.layer_target(layer.local_uniform.values[8]))
               else
               {
                  return false;
               };
               if layer_target == PreparedTarget::ExactLayer
                  && self.prepared_exact_layer_pipelines.is_none()
               {
                  return false;
               }
               misses = misses.saturating_add(1);
               layer_misses = layer_misses.saturating_add(1);
               if !lookup.hit
               {
                  chunk_rebuilds = chunk_rebuilds.saturating_add(1);
                  commands_lowered = commands_lowered.saturating_add(lookup.command_count);
                  upload_bytes = upload_bytes.saturating_add(lookup.upload_bytes);
                  resource_creates = resource_creates.saturating_add(lookup.buffer_count);
               }
            }
            if !write_all_properties
            {
               property_cache.resolve(
                  plan.len(),
                  uniform,
                  source_revision,
                  slot,
               );
            }
            plan.push(PreparedFrameInstance {
               key: layer.key.chunk,
               uniform,
               clip,
               local_damage,
               layer: Some(layer),
            });
            return true;
         }
         let Some(lookup) = cache.get_or_prepare(self, &instance.chunk) else
         {
            return false;
         };
         if lookup.hit
         {
            hits = hits.saturating_add(1);
         }
         else
         {
            misses = misses.saturating_add(1);
            chunk_rebuilds = chunk_rebuilds.saturating_add(1);
            commands_lowered = commands_lowered.saturating_add(lookup.command_count);
            upload_bytes = upload_bytes.saturating_add(lookup.upload_bytes);
            resource_creates = resource_creates.saturating_add(lookup.buffer_count);
         }
         if !write_all_properties
         {
            property_cache.resolve(
               plan.len(),
               uniform,
               source_revision,
               slot,
            );
         }
         plan.push(PreparedFrameInstance {
            key: lookup.key,
            uniform,
            clip,
            local_damage,
            layer: None,
         });
         true
      };
      if !reuse_static_plan && use_damage
      {
         for index in damage_instances.iter().copied()
         {
            let Some(resolved) = snapshot.resolved_instance(index) else { continue };
            if resolved.bounds.is_empty()
            {
               continue;
            }
            let Some(uniform) = PreparedInstanceUniform::from_resolved(&resolved, viewport) else
            {
               supported = false;
               break;
            };
            let clip = match resolved.resolved_clip
            {
               api::RenderSpatialBounds::Empty => continue,
               api::RenderSpatialBounds::Finite(_) => resolved.resolved_clip.conservative_rect_i(),
               api::RenderSpatialBounds::Unbounded => None,
            };
            let global = effective_scissor_dp(clip, self.frame_scissor_dp).unwrap();
            let local_damage = inverse_transform_rect(global, uniform);
            if !prepare_instance(&resolved.instance, uniform, resolved.source_revision, clip, local_damage)
            {
               supported = false;
               break;
            }
         }
      }
      else if !reuse_static_plan
      {
         if let Some(resolved_instances) = static_instances
         {
            for resolved in resolved_instances
            {
               let Some(uniform) = PreparedInstanceUniform::from_resolved(resolved, viewport) else
               {
                  supported = false;
                  break;
               };
               let clip = match resolved.resolved_clip
               {
                  api::RenderSpatialBounds::Empty => continue,
                  api::RenderSpatialBounds::Finite(_) => resolved.resolved_clip.conservative_rect_i(),
                  api::RenderSpatialBounds::Unbounded => None,
               };
               if !prepare_instance(&resolved.instance, uniform, resolved.source_revision, clip, None)
               {
                  supported = false;
                  break;
               }
            }
         }
         else
         {
            snapshot.visit_instances(|instance| {
               if !supported
               {
                  return;
               }
               let dynamic = if write_all_properties
               {
                  PreparedInstanceUniform::dynamic::<false>(snapshot, &instance.property_slots)
               }
               else
               {
                  PreparedInstanceUniform::dynamic::<true>(snapshot, &instance.property_slots)
               };
               let Some(dynamic) = dynamic else
               {
                  supported = false;
                  return;
               };
               let Some(uniform) = PreparedInstanceUniform::from_dynamic(dynamic, instance.origin, viewport) else
               {
                  supported = false;
                  return;
               };
               let mut clip = instance.clip.map(|clip| transform_rect(clip, uniform));
               for dynamic_clip in instance.dynamic_clips.iter().copied()
               {
                  let Some(clip_uniform) = PreparedInstanceUniform::dynamic::<false>(
                     snapshot,
                     core::slice::from_ref(&dynamic_clip.transform),
                  ).and_then(|dynamic| PreparedInstanceUniform::from_dynamic(dynamic, [0.0, 0.0], viewport)) else
                  {
                     supported = false;
                     return;
                  };
                  let transformed = transform_rect_f(dynamic_clip.rect, clip_uniform);
                  clip = Some(clip.map_or(transformed, |current| intersect_scissor_dp(current, transformed)));
               }
               supported = prepare_instance(instance, uniform, dynamic.source_revision, clip, None);
            });
         }
      }
      drop(prepare_instance);
      if reuse_static_plan
      {
         for (index, instance) in plan.iter().enumerate()
         {
            property_cache.resolve(
               index,
               instance.uniform,
               STATIC_SOURCE_REVISION,
               slot,
            );
         }
      }
      if !supported
      {
         self.prepared_chunks = cache;
         self.prepared_property_cache = property_cache;
         self.prepared_frame_plan = plan;
         self.prepared_layer_frame_keys = layer_frame_keys;
         self.prepared_damage_instances = damage_instances;
         return self.encode_snapshot_flat(snapshot);
      }
      self.prepared_layer_frame_keys = layer_frame_keys;
      if !reuse_static_plan && static_instances.is_some()
      {
         let mut unique = HashSet::with_capacity(cache.len());
         self.prepared_frame_keys.extend(plan.iter().filter_map(|instance| {
            (instance.layer.is_none() && unique.insert(instance.key)).then_some(instance.key)
         }));
         self.prepared_frame_snapshot = Some(snapshot.clone());
         self.prepared_frame_viewport = viewport;
      }
      self.prepared_damage_instances = damage_instances;
      self.acc_backend_cache_hits = self.acc_backend_cache_hits.saturating_add(hits);
      self.acc_backend_cache_misses = self.acc_backend_cache_misses.saturating_add(misses);
      self.acc_chunks_reused = self.acc_chunks_reused.saturating_add(hits);
      self.acc_chunks_rebuilt = self.acc_chunks_rebuilt.saturating_add(chunk_rebuilds);
      self.acc_chunks_prepared = self.acc_chunks_prepared.saturating_add(chunk_rebuilds);
      self.acc_commands_traversed = self.acc_commands_traversed.saturating_add(commands_lowered);
      self.acc_geometry_bytes_copied = self.acc_geometry_bytes_copied.saturating_add(upload_bytes);
      self.acc_resource_creates = self.acc_resource_creates.saturating_add(resource_creates);
      self.acc_layer_cache_hits = self.acc_layer_cache_hits.saturating_add(layer_hits);
      self.acc_layer_cache_misses = self.acc_layer_cache_misses.saturating_add(layer_misses);

      let pending_present_texture = self.pending_present_texture as *mut MTLTexture;
      let direct_present_texture = if self.sample_count == 1
         && !damage_requested
         && !self.frame_color_initialized
         && !pending_present_texture.is_null()
      {
         // SAFETY: the host retains the pending drawable texture until frame submission,
         // and this branch only borrows it while constructing this frame's render pass.
         let texture = unsafe { TextureRef::from_ptr(pending_present_texture) };
         (texture.width() as u32 == self.target_w
            && texture.height() as u32 == self.target_h
            && texture.pixel_format() == self.color_format)
            .then_some(texture)
      }
      else
      {
         None
      };
      self.frame_present_direct_to_drawable = direct_present_texture.is_some();
      if self.frame_present_direct_to_drawable
      {
         self.persistent_target_valid = false;
      }

      let descriptor = RenderPassDescriptor::new();
      let Some(attachment) = descriptor.color_attachments().object_at(0) else
      {
         self.prepared_chunks = cache;
         self.prepared_property_cache = property_cache;
         self.prepared_frame_plan = plan;
         return self.encode_snapshot_flat(snapshot);
      };
      let dynamic_stride = core::mem::size_of::<PreparedInstanceUniform>();
      let Some(dynamic_bytes) = plan.len().checked_mul(dynamic_stride) else
      {
         self.prepared_chunks = cache;
         self.prepared_property_cache = property_cache;
         self.prepared_frame_plan = plan;
         return self.encode_snapshot_flat(snapshot);
      };
      if self.property_ring.ensure_capacity(&self.device, slot, dynamic_bytes)
      {
         self.acc_resource_grows = self.acc_resource_grows.saturating_add(1);
      }
      if write_all_properties
      {
         for (index, instance) in plan.iter().enumerate()
         {
            let offset = index * dynamic_stride;
            unsafe
            {
               core::ptr::copy_nonoverlapping(
                  instance.uniform.values.as_ptr().cast::<u8>(),
                  self.property_ring.contents_ptr(slot).as_ptr().add(offset),
                  dynamic_stride,
               );
            }
         }
         property_upload_bytes = dynamic_bytes as u64;
         property_records_updated = plan.len().min(u32::MAX as usize) as u32;
      }
      else
      {
         let mut pending = 0;
         while pending < property_cache.pending_indices.len()
         {
            let run = pending;
            let first = property_cache.pending_indices[pending];
            let mut end = first + 1;
            pending += 1;
            while pending < property_cache.pending_indices.len()
               && property_cache.pending_indices[pending] == end
            {
               end += 1;
               pending += 1;
            }
            let offset = first * dynamic_stride;
            let bytes = (end - first) * dynamic_stride;
            unsafe
            {
               core::ptr::copy_nonoverlapping(
                  property_cache.pending_uniforms.as_ptr().add(run).cast::<u8>(),
                  self.property_ring.contents_ptr(slot).as_ptr().add(offset),
                  bytes,
               );
            }
            for index in first..end
            {
               let entry = &mut property_cache.entries[index];
               entry.ring_revisions[slot] = entry.revision;
            }
            property_upload_bytes = property_upload_bytes.saturating_add(bytes as u64);
            property_records_updated = property_records_updated.saturating_add(
               (end - first).min(u32::MAX as usize) as u32,
            );
         }
      }
      property_cache.truncate(plan.len());
      let command_buffer = self.ensure_frame_command_buffer(slot);
      let mut clip_stack = self.clip_stack_pool.pop().unwrap_or_default();
      let mut damage_commands = core::mem::take(&mut self.prepared_damage_commands);
      for instance in &plan
      {
         let Some(layer) = instance.layer.filter(|layer| layer.refresh) else { continue };
         let Some(chunk) = cache.get(instance.key) else { continue };
         let Some(layer_target) = chunk.layer_target(layer.local_uniform.values[8]) else { continue };
         let layer_format = if layer_target == PreparedTarget::ExactLayer
         {
            MTLPixelFormat::RGBA32Float
         }
         else
         {
            self.color_format
         };
         let texture = self.layers.get(&layer.key.id)
            .filter(|entry| {
               entry.w == layer.width
                  && entry.h == layer.height
                  && entry.tex.pixel_format() == layer_format
            })
            .map(|entry| entry.tex.to_owned())
            .unwrap_or_else(|| {
               let descriptor = TextureDescriptor::new();
               descriptor.set_pixel_format(layer_format);
               descriptor.set_texture_type(MTLTextureType::D2);
               descriptor.set_width(layer.width as u64);
               descriptor.set_height(layer.height as u64);
               descriptor.set_storage_mode(MTLStorageMode::Private);
               descriptor.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
               self.acc_resource_creates = self.acc_resource_creates.saturating_add(1);
               self.acc_layer_texture_creates = self.acc_layer_texture_creates.saturating_add(1);
               self.device.new_texture(&descriptor)
            });
         let layer_descriptor = RenderPassDescriptor::new();
         let Some(layer_attachment) = layer_descriptor.color_attachments().object_at(0) else { continue };
         layer_attachment.set_texture(Some(&texture));
         layer_attachment.set_load_action(MTLLoadAction::Clear);
         layer_attachment.set_clear_color(MTLClearColor {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
            alpha: 0.0,
         });
         layer_attachment.set_store_action(MTLStoreAction::Store);
         self.acc_render_passes = self.acc_render_passes.saturating_add(1);
         let layer_encoder = command_buffer.new_render_command_encoder(&layer_descriptor);
         layer_encoder.set_vertex_bytes(
            2,
            core::mem::size_of::<PreparedInstanceUniform>() as u64,
            layer.local_uniform.values.as_ptr().cast(),
         );
         layer_encoder.set_fragment_bytes(
            3,
            core::mem::size_of::<PreparedInstanceUniform>() as u64,
            layer.local_uniform.values.as_ptr().cast(),
         );
         clip_stack.clear();
         damage_commands.clear();
         let mut last_applied = None;
         let draws_before = u64::from(self.acc_draws);
         encode_prepared_chunk(
            &layer_encoder,
            self,
            chunk,
            layer.local_uniform,
            0,
            None,
            None,
            None,
            layer_target,
            &mut damage_commands,
            &mut clip_stack,
            &mut last_applied,
         );
         self.acc_layer_offscreen_draws = self.acc_layer_offscreen_draws
            .saturating_add(u64::from(self.acc_draws).saturating_sub(draws_before));
         layer_encoder.end_encoding();
         let generation = self.layers.get(&layer.key.id)
            .map_or(1, |entry| entry.generation.wrapping_add(1).max(1));
         if let Some(entry) = self.layers.get_mut(&layer.key.id)
            .filter(|entry| {
               entry.w == layer.width
                  && entry.h == layer.height
                  && entry.tex.pixel_format() == layer_format
            })
         {
            entry.generation = generation;
            entry.prepared_key = Some(layer.key);
            entry.resources.clear();
            entry.resources.extend_from_slice(chunk.source.resource_dependencies());
         }
         else
         {
            self.layers.insert(layer.key.id, super::LayerEntry {
               tex: texture,
               w: layer.width,
               h: layer.height,
               generation,
               prepared_key: Some(layer.key),
               resources: chunk.source.resource_dependencies().to_vec(),
            });
         }
         self.acc_layer_double_render_prevented = self.acc_layer_double_render_prevented.saturating_add(1);
      }
      if self.sample_count > 1
      {
         if let Some(texture) = self.target_msaa_tex.as_ref()
         {
            attachment.set_texture(Some(texture));
         }
         if let Some(texture) = self.target_tex.as_ref()
         {
            attachment.set_resolve_texture(Some(texture));
         }
         attachment.set_store_action(MTLStoreAction::MultisampleResolve);
      }
      else if let Some(texture) = direct_present_texture
      {
         attachment.set_texture(Some(texture));
         attachment.set_store_action(MTLStoreAction::Store);
      }
      else
      {
         attachment.set_texture(self.target_tex.as_ref().map(|texture| texture.as_ref()));
         attachment.set_store_action(MTLStoreAction::Store);
      }
      if self.frame_color_initialized && self.persistent_target_valid || use_damage
      {
         attachment.set_load_action(MTLLoadAction::Load);
      }
      else
      {
         attachment.set_load_action(MTLLoadAction::Clear);
      }
      let clear_alpha = if transparent_drawable_clear_enabled() { 0.0 } else { 1.0 };
      attachment.set_clear_color(MTLClearColor { red: 0.0, green: 0.0, blue: 0.0, alpha: clear_alpha });
      let frame_gpu_trace = self.gpu_stage_timing.as_ref()
         .and_then(|timing| timing.begin_submission(&self.device));
      if let Some(trace) = frame_gpu_trace.as_ref()
      {
         trace.configure_render_pass(&descriptor);
      }
      self.frame_gpu_trace = frame_gpu_trace;
      self.acc_render_passes = self.acc_render_passes.saturating_add(1);
      let encoder = command_buffer.new_render_command_encoder(&descriptor);
      encoder.set_vertex_buffer(2, Some(self.property_ring.buffer(slot)), 0);
      encoder.set_fragment_buffer(3, Some(self.property_ring.buffer(slot)), 0);
      let global_clip = if use_damage { self.frame_scissor_dp } else { None };
      let mut last_applied = None;
      for (index, instance) in plan.iter().enumerate()
      {
         clip_stack.clear();
         if let Some(layer) = instance.layer
         {
            apply_scissor_dp(&encoder, self, global_clip, &mut last_applied);
            encode_prepared_layer_composite(&encoder, self, layer);
            continue;
         }
         if let Some(entry) = cache.get(instance.key)
         {
            encode_prepared_chunk(
               &encoder,
               self,
               entry,
               instance.uniform,
               (index * dynamic_stride) as u64,
               instance.clip,
               global_clip,
               instance.local_damage,
               PreparedTarget::Main,
               &mut damage_commands,
               &mut clip_stack,
               &mut last_applied,
            );
         }
      }
      self.prepared_damage_commands = damage_commands;
      self.clip_stack_pool.push(clip_stack);
      encoder.end_encoding();

      self.prepared_chunks = cache;
      self.prepared_property_cache = property_cache;
      self.prepared_frame_plan = plan;
      let evictions = self.prepared_chunks.take_evictions();
      self.last_stats.vb_bytes = 0;
      self.last_stats.ib_bytes = 0;
      self.last_stats.ub_bytes = dynamic_bytes as u64;
      self.last_stats.draws = self.acc_draws;
      self.last_stats.instanced = self.acc_instanced;
      self.last_stats.icb_cmds = 0;
      self.last_stats.commands_traversed = self.acc_commands_traversed;
      self.last_stats.commands_copied = self.acc_commands_copied;
      self.last_stats.geometry_bytes_copied = self.acc_geometry_bytes_copied;
      self.last_stats.chunks_reused = self.acc_chunks_reused;
      self.last_stats.chunks_rebuilt = self.acc_chunks_rebuilt;
      self.last_stats.chunks_prepared = self.acc_chunks_prepared;
      self.last_stats.prepared_plan_reuses = self.acc_prepared_plan_reuses;
      self.last_stats.backend_cache_hits = self.acc_backend_cache_hits;
      self.last_stats.backend_cache_misses = self.acc_backend_cache_misses;
      self.last_stats.damage_instances_visited = self.acc_damage_instances_visited;
      self.last_stats.damage_instances_matched = self.acc_damage_instances_matched;
      self.last_stats.damage_commands_visited = self.acc_damage_commands_visited;
      self.last_stats.damage_commands_matched = self.acc_damage_commands_matched;
      self.last_stats.damage_vertices_visited = self.acc_damage_vertices_visited;
      self.last_stats.damage_query_ms = self.acc_damage_query_ns as f64 / 1_000_000.0;
      self.last_stats.layer_body_commands_scanned = self.acc_layer_body_commands_scanned;
      self.last_stats.layer_body_commands_copied = self.acc_layer_body_commands_copied;
      self.last_stats.layer_texture_creates = self.acc_layer_texture_creates;
      self.last_stats.layer_cache_hits = self.acc_layer_cache_hits;
      self.last_stats.layer_cache_misses = self.acc_layer_cache_misses;
      self.last_stats.layer_offscreen_draws = self.acc_layer_offscreen_draws;
      self.last_stats.layer_inline_draws = self.acc_layer_inline_draws;
      self.last_stats.layer_double_render_prevented = self.acc_layer_double_render_prevented;
      self.last_stats.buffer_upload_bytes = upload_bytes;
      self.last_stats.property_upload_bytes = property_upload_bytes;
      self.last_stats.property_records_updated = property_records_updated;
      self.last_stats.property_ring_bytes = self.property_ring.cap[..self.frames.len()].iter()
         .fold(0_u64, |bytes, capacity| bytes.saturating_add(*capacity as u64));
      self.last_stats.shaded_damage_px = if use_damage
      {
         self.frame_damage_px
      }
      else
      {
         u64::from(self.target_w).saturating_mul(u64::from(self.target_h))
      };
      self.last_stats.cache_evictions = evictions.min(u64::from(u32::MAX)) as u32;
      self.last_stats.resource_creates = self.acc_resource_creates;
      self.last_stats.render_passes = self.acc_render_passes;
      self.last_stats.command_buffers = 1;
      self.last_stats.damage_px = self.frame_damage_px;
      self.last_stats.damage_pct = self.frame_damage_pct;
      self.last_stats.damage_rects = self.frame_damage_rects;
      self.last_stats.damage_forced_full_refreshes = self.acc_damage_forced_full_refreshes;
      self.last_stats.persistent_target_valid = u32::from(!self.frame_present_direct_to_drawable);
      self.last_stats.encode_ms = started_at.elapsed().as_secs_f64() * 1000.0;
      self.frame_2d_encoded = true;
      self.frame_color_initialized = true;
      if self.frame_present_direct_to_drawable
      {
         self.persistent_target_valid = false;
      }
      else
      {
         self.persistent_target_valid = true;
         self.persistent_target_policy = 0;
      }
      Ok(())
   }

   fn encode_snapshot_flat(&mut self, snapshot: &api::RenderSnapshot) -> Result<(), api::RenderSnapshotError>
   {
      let mut fallback = core::mem::take(&mut self.prepared_fallback);
      fallback.items.clear();
      fallback.vertices.clear();
      fallback.indices.clear();
      let stats = snapshot.flatten_into(&mut fallback)?;
      <Self as api::Renderer>::encode_pass(self, &fallback);
      self.acc_commands_copied = self.acc_commands_copied.saturating_add(stats.commands_copied);
      self.acc_geometry_bytes_copied = self.acc_geometry_bytes_copied
         .saturating_add(stats.vertex_bytes_copied)
         .saturating_add(stats.index_bytes_copied);
      self.last_stats.commands_copied = self.acc_commands_copied;
      self.last_stats.geometry_bytes_copied = self.acc_geometry_bytes_copied;
      self.prepared_fallback = fallback;
      Ok(())
   }
}

fn encode_prepared_layer_composite(encoder: &RenderCommandEncoderRef, renderer: &mut MetalRenderer, layer: PreparedLayerFrame)
{
   let Some(entry) = renderer.layers.get(&layer.key.id)
      .filter(|entry| {
         entry.w == layer.width
            && entry.h == layer.height
            && entry.prepared_key == Some(layer.key)
      })
   else
   {
      debug_assert!(false, "prepared Metal layer key must exist before composition");
      return;
   };
   let texture = entry.tex.to_owned();
   let scale = renderer.target_scale.max(1.0);
   let pixel_aligned = [layer.rect.x, layer.rect.y, layer.rect.w, layer.rect.h]
      .into_iter()
      .all(|value| {
         let pixels = value * scale;
         (pixels - pixels.round()).abs() <= f32::EPSILON
      })
      && (layer.rect.w * scale).round() as u32 == entry.w
      && (layer.rect.h * scale).round() as u32 == entry.h;
   encoder.set_render_pipeline_state(if pixel_aligned {
      &renderer.pso_layer_composite_aligned
   } else {
      &renderer.pso_layer_composite
   });
   if !pixel_aligned
   {
      if let Some(sampler) = renderer.sampler.as_ref()
      {
         encoder.set_fragment_sampler_state(0, Some(sampler));
      }
   }
   encoder.set_fragment_texture(0, Some(&texture));
   let viewport = [
      renderer.target_w as f32 / scale,
      renderer.target_h as f32 / scale,
   ];
   encoder.set_vertex_bytes(
      1,
      core::mem::size_of_val(&viewport) as u64,
      viewport.as_ptr().cast(),
   );
   let vertex = [
      layer.rect.x, layer.rect.y, layer.rect.w, layer.rect.h,
      viewport[0], viewport[1],
   ];
   encoder.set_vertex_bytes(0, core::mem::size_of_val(&vertex) as u64, vertex.as_ptr().cast());
   let fragment = pack_nine_slice_params(
      layer.rect,
      entry.w as f32,
      entry.h as f32,
      api::Insets::new(0.0, 0.0, 0.0, 0.0),
      1.0,
   );
   encoder.set_fragment_bytes(
      1,
      core::mem::size_of_val(&fragment) as u64,
      (&fragment as *const NineSliceGpuParams).cast(),
   );
   encoder.draw_primitives(MTLPrimitiveType::Triangle, 0, 6);
   renderer.acc_draws = renderer.acc_draws.saturating_add(1);
}

fn prepared_pipelines_for_target(renderer: &MetalRenderer, target: PreparedTarget) -> &Option<PreparedPipelines>
{
   match target
   {
      PreparedTarget::Main => &renderer.prepared_pipelines,
      PreparedTarget::Layer => &renderer.prepared_layer_pipelines,
      PreparedTarget::ExactLayer => &renderer.prepared_exact_layer_pipelines,
   }
}

fn encode_prepared_chunk(encoder: &RenderCommandEncoderRef, renderer: &mut MetalRenderer, chunk: &PreparedChunk, uniform: PreparedInstanceUniform, uniform_offset: u64, instance_clip: Option<api::RectI>, global_clip: Option<api::RectI>, local_damage: Option<api::RectF>, target: PreparedTarget, damage_commands: &mut Vec<u32>, clip_stack: &mut Vec<api::RectI>, last_applied: &mut Option<api::RectI>)
{
   let filtered = if let Some(local_damage) = local_damage
   {
      let query_started = std::time::Instant::now();
      let stats = chunk.source.query_damage_commands(local_damage, damage_commands);
      renderer.acc_damage_query_ns = renderer.acc_damage_query_ns.saturating_add(
         query_started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64,
      );
      renderer.acc_damage_commands_visited = renderer.acc_damage_commands_visited.saturating_add(stats.entries_visited);
      renderer.acc_damage_commands_matched = renderer.acc_damage_commands_matched.saturating_add(stats.entries_matched);
      true
   }
   else
   {
      damage_commands.clear();
      false
   };
   let mut current_clip = instance_clip;
   apply_scissor_dp(
      encoder,
      renderer,
      effective_scissor_dp(current_clip, global_clip),
      last_applied,
   );
   if target == PreparedTarget::Main
   {
      encoder.set_vertex_buffer_offset(2, uniform_offset);
   }
   let opaque = uniform.values[8] == 1.0;
   if !opaque && target == PreparedTarget::Main
   {
      encoder.set_fragment_buffer_offset(3, uniform_offset);
   }
   macro_rules! encode_operation
   {
      ($operation:expr) =>
      {{
         match $operation
         {
         PreparedOperation::RRects { params, first_command, count } =>
         {
            if filtered && !selected_range(damage_commands, *first_command, *count)
            {
               continue;
            }
            let pipelines = prepared_pipelines_for_target(renderer, target);
            let Some(pipelines) = pipelines.as_ref() else { return };
            encoder.set_render_pipeline_state(if opaque { &pipelines.rrect_opaque } else { &pipelines.rrect });
            encoder.set_vertex_buffer(0, Some(params), 0);
            encoder.set_fragment_buffer(1, Some(params), 0);
            encoder.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, *count);
            renderer.acc_draws = renderer.acc_draws.saturating_add(1);
            renderer.acc_instanced = renderer.acc_instanced.saturating_add((*count).min(u64::from(u32::MAX)) as u32);
         }
         PreparedOperation::Images { params, argument_buffer, handles, instance_handles, first_command, count } =>
         {
            if filtered && !selected_range(damage_commands, *first_command, *count)
            {
               continue;
            }
            if let Some(argument_buffer) = argument_buffer.as_ref()
            {
               let pipelines = prepared_pipelines_for_target(renderer, target);
               let Some(pipelines) = pipelines.as_ref() else { return };
               encoder.set_render_pipeline_state(if opaque { &pipelines.image_opaque } else { &pipelines.image });
               encoder.set_vertex_buffer(0, Some(params), 0);
               encoder.set_fragment_buffer(1, Some(params), 0);
               encoder.set_fragment_buffer(2, Some(argument_buffer), 0);
               if let Some(sampler) = renderer.sampler.as_ref()
               {
                  encoder.set_fragment_sampler_state(0, Some(sampler));
               }
               for handle in handles
               {
                  if let Some(texture) = renderer.images.get(&handle.0)
                  {
                     encoder.use_resource_at(texture, MTLResourceUsage::Read, MTLRenderStages::Fragment);
                  }
               }
               encoder.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, *count);
               renderer.acc_draws = renderer.acc_draws.saturating_add(1);
               renderer.acc_instanced = renderer.acc_instanced.saturating_add((*count).min(u64::from(u32::MAX)) as u32);
               renderer.acc_image_argument_binds = renderer.acc_image_argument_binds.saturating_add(1);
            }
            else
            {
               let pipelines = prepared_pipelines_for_target(renderer, target);
               let Some(pipelines) = pipelines.as_ref() else { return };
               encoder.set_render_pipeline_state(if opaque { &pipelines.image_single_opaque } else { &pipelines.image_single });
               if let Some(sampler) = renderer.sampler.as_ref()
               {
                  encoder.set_fragment_sampler_state(0, Some(sampler));
               }
               for (index, handle) in instance_handles.iter().copied().enumerate()
               {
                  let Some(texture) = renderer.images.get(&handle.0) else { continue };
                  encoder.set_fragment_texture(0, Some(texture));
                  let offset = (index * core::mem::size_of::<super::ImageGpuParams>()) as u64;
                  encoder.set_vertex_buffer(0, Some(params), offset);
                  encoder.set_fragment_buffer(1, Some(params), offset);
                  encoder.draw_primitives(MTLPrimitiveType::Triangle, 0, 6);
                  renderer.acc_draws = renderer.acc_draws.saturating_add(1);
               }
            }
         }
         PreparedOperation::Glyphs { vertices, indices, uniforms, draws, atlas, sdf } =>
         {
            let Some(texture) = renderer.images.get(&atlas.0) else { continue };
            let pipelines = prepared_pipelines_for_target(renderer, target);
            let Some(pipelines) = pipelines.as_ref() else { return };
            let pipeline = match (*sdf, opaque)
            {
               (true, true) => &pipelines.text_sdf_opaque,
               (true, false) => &pipelines.text_sdf,
               (false, true) => &pipelines.text_opaque,
               (false, false) => &pipelines.text,
            };
            encoder.set_render_pipeline_state(pipeline);
            encoder.set_fragment_texture(0, Some(texture));
            if let Some(sampler) = renderer.sampler.as_ref()
            {
               encoder.set_fragment_sampler_state(0, Some(sampler));
            }
            for draw in draws
            {
               if filtered && damage_commands.binary_search(&draw.command).is_err()
               {
                  continue;
               }
               encoder.set_vertex_buffer(0, Some(vertices), draw.vertex_offset);
               encoder.set_fragment_buffer(0, Some(uniforms), draw.uniform_offset);
               encoder.draw_indexed_primitives(
                  MTLPrimitiveType::Triangle,
                  draw.index_count,
                  MTLIndexType::UInt16,
                  indices,
                  draw.index_offset,
               );
               renderer.acc_draws = renderer.acc_draws.saturating_add(1);
            }
         }
         PreparedOperation::ImageMesh { vertices, indices, uniform: color, texture, command, vertex_count, index_count } =>
         {
            if filtered && damage_commands.binary_search(command).is_err()
            {
               continue;
            }
            let Some(texture) = renderer.images.get(&texture.0) else { continue };
            let pipelines = prepared_pipelines_for_target(renderer, target);
            let Some(pipelines) = pipelines.as_ref() else { return };
            encoder.set_render_pipeline_state(if opaque { &pipelines.image_mesh_opaque } else { &pipelines.image_mesh });
            encoder.set_fragment_texture(0, Some(texture));
            if let Some(sampler) = renderer.sampler.as_ref()
            {
               encoder.set_fragment_sampler_state(0, Some(sampler));
            }
            encoder.set_vertex_buffer(0, Some(vertices), 0);
            encoder.set_fragment_buffer(0, Some(color), 0);
            if let Some(indices) = indices.as_ref()
            {
               encoder.draw_indexed_primitives(
                  MTLPrimitiveType::Triangle,
                  *index_count,
                  MTLIndexType::UInt16,
                  indices,
                  0,
               );
               renderer.acc_draws = renderer.acc_draws.saturating_add(1);
            }
            else if let Some(primitive) = solid_primitive_for_vertex_count(*vertex_count as usize)
            {
               encoder.draw_primitives(primitive, 0, *vertex_count);
               renderer.acc_draws = renderer.acc_draws.saturating_add(1);
            }
         }
         PreparedOperation::Solid { vertices, indices, uniform: color, command, vertex_count, index_count } =>
         {
            if filtered && damage_commands.binary_search(command).is_err()
            {
               continue;
            }
            let pipelines = prepared_pipelines_for_target(renderer, target);
            let Some(pipelines) = pipelines.as_ref() else { return };
            encoder.set_render_pipeline_state(&pipelines.solid);
            encoder.set_vertex_buffer(0, Some(vertices), 0);
            encoder.set_vertex_buffer(1, Some(color), 0);
            if let Some(indices) = indices.as_ref()
            {
               if let Some(primitive) = solid_primitive_for_index_count(*index_count as usize)
               {
                  encoder.draw_indexed_primitives(primitive, *index_count, MTLIndexType::UInt16, indices, 0);
                  renderer.acc_draws = renderer.acc_draws.saturating_add(1);
               }
            }
            else if let Some(primitive) = solid_primitive_for_vertex_count(*vertex_count as usize)
            {
               encoder.draw_primitives(primitive, 0, *vertex_count);
               renderer.acc_draws = renderer.acc_draws.saturating_add(1);
            }
         }
         PreparedOperation::ClipPush(rect) =>
         {
            let rect = transform_rect(*rect, uniform);
            let next = current_clip.map_or(rect, |current| intersect_scissor_dp(current, rect));
            clip_stack.push(next);
            current_clip = Some(next);
            apply_scissor_dp(
               encoder,
               renderer,
               effective_scissor_dp(current_clip, global_clip),
               last_applied,
            );
         }
         PreparedOperation::ClipPop =>
         {
            let _ = clip_stack.pop();
            current_clip = clip_stack.last().copied().or(instance_clip);
            apply_scissor_dp(
               encoder,
               renderer,
               effective_scissor_dp(current_clip, global_clip),
               last_applied,
            );
         }
         }
      }};
   }
   if filtered
   {
      let mut previous_operation = None;
      for command in damage_commands.iter().copied()
      {
         let Some((operation_index, operation)) = chunk.operation_for_command(command) else { continue };
         if previous_operation == Some(operation_index)
         {
            continue;
         }
         previous_operation = Some(operation_index);
         let Some(spatial) = chunk.source.command_spatial().get(command as usize) else { continue };
         current_clip = match spatial.resolved_clip
         {
            api::RenderSpatialBounds::Empty => continue,
            api::RenderSpatialBounds::Finite(rect) =>
            {
               let transformed = transform_rect_f(rect, uniform);
               Some(instance_clip.map_or(transformed, |clip| intersect_scissor_dp(clip, transformed)))
            }
            api::RenderSpatialBounds::Unbounded => instance_clip,
         };
         apply_scissor_dp(
            encoder,
            renderer,
            effective_scissor_dp(current_clip, global_clip),
            last_applied,
         );
         encode_operation!(operation);
      }
   }
   else
   {
      for operation in &chunk.operations
      {
         encode_operation!(operation);
      }
   }
}

fn selected_range(commands: &[u32], first: u32, count: u64) -> bool
{
   let end = u64::from(first).saturating_add(count).min(u64::from(u32::MAX) + 1);
   let index = commands.partition_point(|command| *command < first);
   commands.get(index).is_some_and(|command| u64::from(*command) < end)
}

fn inverse_transform_rect(rect: api::RectI, uniform: PreparedInstanceUniform) -> Option<api::RectF>
{
   let [m11, m12, m21, m22, tx, ty, ..] = uniform.values;
   let determinant = m11 * m22 - m12 * m21;
   if !determinant.is_finite() || determinant.abs() <= f32::EPSILON
   {
      return None;
   }
   let x0 = rect.x as f32;
   let y0 = rect.y as f32;
   let x1 = rect.x.saturating_add(rect.w) as f32;
   let y1 = rect.y.saturating_add(rect.h) as f32;
   let inverse = |x: f32, y: f32| {
      let x = x - tx;
      let y = y - ty;
      [
         (m22 * x - m21 * y) / determinant,
         (-m12 * x + m11 * y) / determinant,
      ]
   };
   let points = [
      inverse(x0, y0),
      inverse(x1, y0),
      inverse(x0, y1),
      inverse(x1, y1),
   ];
   if !points.iter().flatten().all(|value| value.is_finite())
   {
      return None;
   }
   let min_x = points.iter().map(|point| point[0]).fold(f32::INFINITY, f32::min);
   let min_y = points.iter().map(|point| point[1]).fold(f32::INFINITY, f32::min);
   let max_x = points.iter().map(|point| point[0]).fold(f32::NEG_INFINITY, f32::max);
   let max_y = points.iter().map(|point| point[1]).fold(f32::NEG_INFINITY, f32::max);
   Some(api::RectF::new(min_x, min_y, max_x - min_x, max_y - min_y))
}

pub(super) fn transform_rect(rect: api::RectI, uniform: PreparedInstanceUniform) -> api::RectI
{
   let [m11, m12, m21, m22, tx, ty, ..] = uniform.values;
   let x0 = rect.x as f32;
   let y0 = rect.y as f32;
   let x1 = rect.x.saturating_add(rect.w) as f32;
   let y1 = rect.y.saturating_add(rect.h) as f32;
   let points = [
      [m11 * x0 + m21 * y0 + tx, m12 * x0 + m22 * y0 + ty],
      [m11 * x1 + m21 * y0 + tx, m12 * x1 + m22 * y0 + ty],
      [m11 * x0 + m21 * y1 + tx, m12 * x0 + m22 * y1 + ty],
      [m11 * x1 + m21 * y1 + tx, m12 * x1 + m22 * y1 + ty],
   ];
   let min_x = points.iter().map(|point| point[0]).fold(f32::INFINITY, f32::min).floor();
   let min_y = points.iter().map(|point| point[1]).fold(f32::INFINITY, f32::min).floor();
   let max_x = points.iter().map(|point| point[0]).fold(f32::NEG_INFINITY, f32::max).ceil();
   let max_y = points.iter().map(|point| point[1]).fold(f32::NEG_INFINITY, f32::max).ceil();
   api::RectI::new(min_x as i32, min_y as i32, (max_x - min_x) as i32, (max_y - min_y) as i32)
}

fn transform_rect_f(rect: api::RectF, uniform: PreparedInstanceUniform) -> api::RectI
{
   let [m11, m12, m21, m22, tx, ty, ..] = uniform.values;
   let x0 = rect.x;
   let y0 = rect.y;
   let x1 = rect.x + rect.w;
   let y1 = rect.y + rect.h;
   let points = [
      [m11 * x0 + m21 * y0 + tx, m12 * x0 + m22 * y0 + ty],
      [m11 * x1 + m21 * y0 + tx, m12 * x1 + m22 * y0 + ty],
      [m11 * x0 + m21 * y1 + tx, m12 * x0 + m22 * y1 + ty],
      [m11 * x1 + m21 * y1 + tx, m12 * x1 + m22 * y1 + ty],
   ];
   let min_x = points.iter().map(|point| point[0]).fold(f32::INFINITY, f32::min).floor();
   let min_y = points.iter().map(|point| point[1]).fold(f32::INFINITY, f32::min).floor();
   let max_x = points.iter().map(|point| point[0]).fold(f32::NEG_INFINITY, f32::max).ceil();
   let max_y = points.iter().map(|point| point[1]).fold(f32::NEG_INFINITY, f32::max).ceil();
   api::RectI::new(min_x as i32, min_y as i32, (max_x - min_x) as i32, (max_y - min_y) as i32)
}
