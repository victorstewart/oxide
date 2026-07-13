use metal::{
   ArgumentEncoderRef, Buffer, Device, DeviceRef, Library, MTLClearColor, MTLIndexType,
   MTLLoadAction, MTLPixelFormat, MTLPrimitiveType, MTLRenderStages, MTLResourceOptions,
   MTLResourceUsage, MTLStoreAction, MTLTexture, RenderCommandEncoderRef,
   RenderPassDescriptor, RenderPipelineDescriptor, RenderPipelineState, TextureRef,
};
use metal::foreign_types::ForeignTypeRef;
use oxide_renderer_api as api;
use std::collections::HashMap;

use super::{
   api_vertex_descriptor, append_remapped_indices_to_span, apply_scissor_dp,
   configure_source_alpha_blend, effective_scissor_dp, intersect_scissor_dp,
   pack_image_params, pack_rrect_params, pipeline_error, pipeline_function, pipeline_state,
   solid_primitive_for_index_count, solid_primitive_for_vertex_count,
   transparent_drawable_clear_enabled, MetalInitError, MetalRenderer,
};

pub const DEFAULT_PREPARED_CACHE_BUDGET_BYTES: u64 = 32 * 1024 * 1024;

pub(super) struct PreparedPipelines
{
   pub solid: RenderPipelineState,
   pub rrect: RenderPipelineState,
   pub rrect_opaque: RenderPipelineState,
   pub image: RenderPipelineState,
   pub image_opaque: RenderPipelineState,
   pub image_single: RenderPipelineState,
   pub image_single_opaque: RenderPipelineState,
   pub text: RenderPipelineState,
   pub text_opaque: RenderPipelineState,
   pub text_sdf: RenderPipelineState,
   pub text_sdf_opaque: RenderPipelineState,
}

impl PreparedPipelines
{
   pub fn new(device: &Device, library: &Library, format: MTLPixelFormat, sample_count: u32) -> Result<Self, MetalInitError>
   {
      Ok(Self {
         solid: prepared_pipeline(device, library, format, sample_count, "prepared.solid", "v_prepared_solid", "f_solid", true)?,
         rrect: prepared_pipeline(device, library, format, sample_count, "prepared.rrect", "v_prepared_inst_rect", "f_prepared_rrect", false)?,
         rrect_opaque: prepared_pipeline(device, library, format, sample_count, "prepared.rrect_opaque", "v_prepared_inst_rect", "f_rrect", false)?,
         image: prepared_pipeline(device, library, format, sample_count, "prepared.image", "v_prepared_inst_rect", "f_prepared_image", false)?,
         image_opaque: prepared_pipeline(device, library, format, sample_count, "prepared.image_opaque", "v_prepared_inst_rect", "f_image", false)?,
         image_single: prepared_pipeline(device, library, format, sample_count, "prepared.image_single", "v_prepared_inst_rect", "f_prepared_image_single", false)?,
         image_single_opaque: prepared_pipeline(device, library, format, sample_count, "prepared.image_single_opaque", "v_prepared_inst_rect", "f_image_single", false)?,
         text: prepared_pipeline(device, library, format, sample_count, "prepared.text", "v_prepared_text", "f_prepared_text", true)?,
         text_opaque: prepared_pipeline(device, library, format, sample_count, "prepared.text_opaque", "v_prepared_text", "f_text", true)?,
         text_sdf: prepared_pipeline(device, library, format, sample_count, "prepared.text_sdf", "v_prepared_text", "f_prepared_text_sdf", true)?,
         text_sdf_opaque: prepared_pipeline(device, library, format, sample_count, "prepared.text_sdf_opaque", "v_prepared_text", "f_text_sdf", true)?,
      })
   }
}

fn prepared_pipeline(device: &Device, library: &Library, format: MTLPixelFormat, sample_count: u32, stage: &str, vertex: &str, fragment: &str, vertex_descriptor: bool) -> Result<RenderPipelineState, MetalInitError>
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
   configure_source_alpha_blend(attachment);
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

#[derive(Clone, Copy)]
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
}

impl PreparedInstanceUniform
{
   fn dynamic(snapshot: &api::RenderSnapshot, property_slots: &[api::RenderPropertySlotId]) -> Option<PreparedDynamicUniform>
   {
      let mut matrix = [1.0_f32, 0.0, 0.0, 1.0];
      let mut translation = [0.0_f32, 0.0];
      let mut opacity = 1.0_f32;
      for id in property_slots.iter().copied()
      {
         let index = snapshot.properties().binary_search_by_key(&id.0, |property| property.id.0).ok()?;
         match snapshot.properties()[index].value
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
      Some(PreparedDynamicUniform { matrix, translation, opacity: opacity.clamp(0.0, 1.0) })
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
}

pub(super) struct PreparedChunk
{
   pub operations: Vec<PreparedOperation>,
   pub byte_size: u64,
   pub logical_byte_size: u64,
   pub buffer_count: u32,
   pub command_count: u64,
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
      let mut index = 0_usize;
      while index < list.items.len()
      {
         match &list.items[index]
         {
            api::DrawCmd::RRect { .. } =>
            {
               let start = index;
               while matches!(list.items.get(index), Some(api::DrawCmd::RRect { .. }))
               {
                  index += 1;
               }
               let (operation, bytes) = prepare_rrects(renderer.device.as_ref(), &list.items[start..index])?;
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
      let logical_byte_size = byte_size;
      byte_size = operations.iter().fold(0_u64, |bytes, operation| {
         bytes.saturating_add(operation_allocated_bytes(operation))
      });
      Some(Self {
         operations,
         byte_size,
         logical_byte_size,
         buffer_count,
         command_count: list.items.len() as u64,
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
}

pub(super) enum PreparedOperation
{
   RRects { params: Buffer, count: u64 },
   Images {
      params: Buffer,
      argument_buffer: Option<Buffer>,
      handles: Vec<api::ImageHandle>,
      instance_handles: Vec<api::ImageHandle>,
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
   Solid {
      vertices: Buffer,
      indices: Option<Buffer>,
      uniform: Buffer,
      vertex_count: u64,
      index_count: u64,
   },
   ClipPush(api::RectI),
   ClipPop,
}

pub(super) struct PreparedGlyphDraw
{
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
      PreparedOperation::Solid { vertices, indices, uniform, .. } => allocated(vertices)
         .saturating_add(indices.as_ref().map_or(0, allocated))
         .saturating_add(allocated(uniform)),
      PreparedOperation::ClipPush(_) | PreparedOperation::ClipPop => 0,
   }
}

fn prepare_rrects(device: &DeviceRef, commands: &[api::DrawCmd]) -> Option<(PreparedOperation, u64)>
{
   let mut params = Vec::with_capacity(commands.len());
   for command in commands
   {
      let api::DrawCmd::RRect { rect, radii, color } = command else { return None };
      params.push(pack_rrect_params(*rect, *radii, *color));
   }
   let params = buffer_from_slice(device, &params)?;
   let bytes = params.length();
   Some((PreparedOperation::RRects { params, count: commands.len() as u64 }, bytes))
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
      draws.push(PreparedGlyphDraw { vertex_offset, index_offset, uniform_offset, index_count });
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
      let mut cache = core::mem::take(&mut self.prepared_chunks);
      let mut plan = core::mem::take(&mut self.prepared_frame_plan);
      plan.clear();
      let mut supported = true;
      let mut hits = 0_u64;
      let mut misses = 0_u64;
      let mut commands_lowered = 0_u64;
      let mut upload_bytes = 0_u64;
      let mut resource_creates = 0_u32;
      let mut last_property_slots = None;
      let mut last_dynamic = None;
      snapshot.visit_instances(|instance| {
         if !supported || instance.layer.is_some()
         {
            supported = false;
            return;
         }
         let dynamic = if last_property_slots.as_deref() == Some(instance.property_slots.as_ref())
         {
            last_dynamic
         }
         else
         {
            let dynamic = PreparedInstanceUniform::dynamic(snapshot, &instance.property_slots);
            last_property_slots = Some(instance.property_slots.clone());
            last_dynamic = dynamic;
            dynamic
         };
         let Some(uniform) = dynamic.and_then(|dynamic| {
            PreparedInstanceUniform::from_dynamic(dynamic, instance.origin, viewport)
         }) else
         {
            supported = false;
            return;
         };
         let Some(lookup) = cache.get_or_prepare(self, &instance.chunk) else
         {
            supported = false;
            return;
         };
         if lookup.hit
         {
            hits = hits.saturating_add(1);
         }
         else
         {
            misses = misses.saturating_add(1);
            commands_lowered = commands_lowered.saturating_add(lookup.command_count);
            upload_bytes = upload_bytes.saturating_add(lookup.upload_bytes);
            resource_creates = resource_creates.saturating_add(lookup.buffer_count);
         }
         plan.push(PreparedFrameInstance {
            key: lookup.key,
            uniform,
            clip: instance.clip.map(|clip| transform_rect(clip, uniform)),
         });
      });
      if !supported
      {
         self.prepared_chunks = cache;
         self.prepared_frame_plan = plan;
         return self.encode_snapshot_flat(snapshot);
      }

      self.acc_backend_cache_hits = self.acc_backend_cache_hits.saturating_add(hits);
      self.acc_backend_cache_misses = self.acc_backend_cache_misses.saturating_add(misses);
      self.acc_chunks_reused = self.acc_chunks_reused.saturating_add(hits);
      self.acc_chunks_rebuilt = self.acc_chunks_rebuilt.saturating_add(misses);
      self.acc_chunks_prepared = self.acc_chunks_prepared.saturating_add(misses);
      self.acc_commands_traversed = self.acc_commands_traversed.saturating_add(commands_lowered);
      self.acc_geometry_bytes_copied = self.acc_geometry_bytes_copied.saturating_add(upload_bytes);
      self.acc_resource_creates = self.acc_resource_creates.saturating_add(resource_creates);

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
         self.prepared_frame_plan = plan;
         return self.encode_snapshot_flat(snapshot);
      };
      let slot = self.current_frame_slot();
      let dynamic_stride = core::mem::size_of::<PreparedInstanceUniform>();
      let dynamic_offset = super::align_up_usize(self.frames[slot].ub_used, core::mem::align_of::<PreparedInstanceUniform>());
      let Some(dynamic_bytes) = plan.len().checked_mul(dynamic_stride) else
      {
         self.prepared_chunks = cache;
         self.prepared_frame_plan = plan;
         return self.encode_snapshot_flat(snapshot);
      };
      let Some(dynamic_end) = dynamic_offset.checked_add(dynamic_bytes) else
      {
         self.prepared_chunks = cache;
         self.prepared_frame_plan = plan;
         return self.encode_snapshot_flat(snapshot);
      };
      if self.ub.ensure_capacity(&self.device, slot, dynamic_end)
      {
         self.acc_resource_grows = self.acc_resource_grows.saturating_add(1);
      }
      for (index, instance) in plan.iter().enumerate()
      {
         let offset = dynamic_offset + index * dynamic_stride;
         unsafe
         {
            core::ptr::copy_nonoverlapping(
               instance.uniform.values.as_ptr().cast::<u8>(),
               self.ub.contents_ptr(slot).as_ptr().add(offset),
               dynamic_stride,
            );
         }
      }
      self.frames[slot].ub_used = dynamic_end;
      let command_buffer = self.ensure_frame_command_buffer(slot);
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
      let global_clip = if use_damage { self.frame_scissor_dp } else { None };
      let mut clip_stack = self.clip_stack_pool.pop().unwrap_or_default();
      let mut last_applied = None;
      for (index, instance) in plan.iter().enumerate()
      {
         clip_stack.clear();
         if let Some(entry) = cache.get(instance.key)
         {
            encode_prepared_chunk(
               &encoder,
               self,
               entry,
               instance.uniform,
               slot,
               (dynamic_offset + index * dynamic_stride) as u64,
               instance.clip,
               global_clip,
               &mut clip_stack,
               &mut last_applied,
            );
         }
      }
      self.clip_stack_pool.push(clip_stack);
      encoder.end_encoding();

      self.prepared_chunks = cache;
      self.prepared_frame_plan = plan;
      let evictions = self.prepared_chunks.take_evictions();
      self.last_stats.vb_bytes = 0;
      self.last_stats.ib_bytes = 0;
      self.last_stats.ub_bytes = self.frames[slot].ub_used as u64;
      self.last_stats.draws = self.acc_draws;
      self.last_stats.instanced = self.acc_instanced;
      self.last_stats.icb_cmds = 0;
      self.last_stats.commands_traversed = self.acc_commands_traversed;
      self.last_stats.commands_copied = self.acc_commands_copied;
      self.last_stats.geometry_bytes_copied = self.acc_geometry_bytes_copied;
      self.last_stats.chunks_reused = self.acc_chunks_reused;
      self.last_stats.chunks_rebuilt = self.acc_chunks_rebuilt;
      self.last_stats.chunks_prepared = self.acc_chunks_prepared;
      self.last_stats.backend_cache_hits = self.acc_backend_cache_hits;
      self.last_stats.backend_cache_misses = self.acc_backend_cache_misses;
      self.last_stats.buffer_upload_bytes = upload_bytes;
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

fn encode_prepared_chunk(encoder: &RenderCommandEncoderRef, renderer: &mut MetalRenderer, chunk: &PreparedChunk, uniform: PreparedInstanceUniform, slot: usize, uniform_offset: u64, instance_clip: Option<api::RectI>, global_clip: Option<api::RectI>, clip_stack: &mut Vec<api::RectI>, last_applied: &mut Option<api::RectI>)
{
   let mut current_clip = instance_clip;
   apply_scissor_dp(
      encoder,
      renderer,
      effective_scissor_dp(current_clip, global_clip),
      last_applied,
   );
   encoder.set_vertex_buffer(2, Some(renderer.ub.buffer(slot)), uniform_offset);
   let opaque = uniform.values[8] == 1.0;
   if !opaque
   {
      encoder.set_fragment_buffer(3, Some(renderer.ub.buffer(slot)), uniform_offset);
   }
   for operation in &chunk.operations
   {
      match operation
      {
         PreparedOperation::RRects { params, count } =>
         {
            let Some(pipelines) = renderer.prepared_pipelines.as_ref() else { return };
            encoder.set_render_pipeline_state(if opaque { &pipelines.rrect_opaque } else { &pipelines.rrect });
            encoder.set_vertex_buffer(0, Some(params), 0);
            encoder.set_fragment_buffer(1, Some(params), 0);
            encoder.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, *count);
            renderer.acc_draws = renderer.acc_draws.saturating_add(1);
            renderer.acc_instanced = renderer.acc_instanced.saturating_add((*count).min(u64::from(u32::MAX)) as u32);
         }
         PreparedOperation::Images { params, argument_buffer, handles, instance_handles, count } =>
         {
            if let Some(argument_buffer) = argument_buffer.as_ref()
            {
               let Some(pipelines) = renderer.prepared_pipelines.as_ref() else { return };
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
               let Some(pipelines) = renderer.prepared_pipelines.as_ref() else { return };
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
            let Some(pipelines) = renderer.prepared_pipelines.as_ref() else { return };
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
         PreparedOperation::Solid { vertices, indices, uniform: color, vertex_count, index_count } =>
         {
            let Some(pipelines) = renderer.prepared_pipelines.as_ref() else { return };
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
   }
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
