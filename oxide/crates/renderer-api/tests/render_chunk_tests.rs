use std::sync::Arc;

use oxide_renderer_api::{
   ChunkIndexMode, Color, Damage, DrawCmd, DrawList, GlyphRun, ImageHandle, IndexSpan, RectF,
   RectI, RenderChunk, RenderChunkError, RenderChunkId, RenderChunkInstance,
   RenderChunkRevisions, RenderLayerInstance, RenderPropertySlot, RenderPropertySlotId,
   RenderPropertyValue, RenderResourceDependency, RenderSnapshot, RenderSnapshotError, Vertex,
   VertexSpan,
};

fn vertex(x: f32, y: f32) -> Vertex
{
   Vertex { x, y, u: 0.0, v: 0.0, rgba: 0 }
}

fn mesh_list(vertex_offset: u32, indices: &[u16]) -> DrawList
{
   let mut list = DrawList::default();
   list.vertices.extend([
      vertex(-100.0, -100.0),
      vertex(-90.0, -90.0),
      vertex(-80.0, -80.0),
      vertex(-70.0, -70.0),
      vertex(10.0, 20.0),
      vertex(30.0, 20.0),
      vertex(30.0, 50.0),
   ]);
   list.indices.extend_from_slice(indices);
   list.items.push(DrawCmd::Solid {
      vb: VertexSpan { offset: vertex_offset, len: 3 },
      ib: IndexSpan { offset: 0, len: indices.len() as u32 },
      color: Color::rgba(1.0, 0.5, 0.25, 1.0),
   });
   list
}

fn shape_chunk(id: u64) -> RenderChunk
{
   let mut list = DrawList::default();
   list.items.push(DrawCmd::RRect {
      rect: RectF::new(0.0, 0.0, 20.0, 10.0),
      radii: [2.0; 4],
      color: Color::rgba(0.1, 0.2, 0.3, 0.8),
   });
   RenderChunk::new(
      RenderChunkId(id),
      RenderChunkRevisions::default(),
      list,
      ChunkIndexMode::Local,
      &[],
   )
   .unwrap_or_else(|error| panic!("shape chunk failed: {error}"))
}

#[test]
fn chunk_canonicalizes_absolute_indices_and_packs_only_referenced_vertices()
{
   let chunk = RenderChunk::new(
      RenderChunkId(7),
      RenderChunkRevisions { structural: 1, geometry: 2, resource: 3, dynamic_properties: 4 },
      mesh_list(4, &[4, 5, 6]),
      ChunkIndexMode::Absolute,
      &[],
   )
   .unwrap_or_else(|error| panic!("chunk failed: {error}"));

   assert_eq!(chunk.id(), RenderChunkId(7));
   assert_eq!(chunk.draw_list().vertices, vec![vertex(10.0, 20.0), vertex(30.0, 20.0), vertex(30.0, 50.0)]);
   assert_eq!(chunk.draw_list().indices, vec![0, 1, 2]);
   assert_eq!(chunk.bounds(), Some(RectF::new(10.0, 20.0, 20.0, 30.0)));
   match &chunk.draw_list().items[0] {
      DrawCmd::Solid { vb, ib, .. } => {
         assert_eq!(*vb, VertexSpan { offset: 0, len: 3 });
         assert_eq!(*ib, IndexSpan { offset: 0, len: 3 });
      }
      command => panic!("unexpected canonical command: {command:?}"),
   }
}

#[test]
fn explicit_index_mode_rejects_ambiguous_or_out_of_span_indices()
{
   let error = RenderChunk::new(
      RenderChunkId(1),
      RenderChunkRevisions::default(),
      mesh_list(4, &[4, 5, 6]),
      ChunkIndexMode::Local,
      &[],
   )
   .expect_err("absolute indices must not be guessed as local");
   assert_eq!(error, RenderChunkError::IndexOutsideVertexSpan { command: 0, index: 4 });

   let error = RenderChunk::new(
      RenderChunkId(2),
      RenderChunkRevisions::default(),
      mesh_list(4, &[4, 5, 7]),
      ChunkIndexMode::Absolute,
      &[],
   )
   .expect_err("absolute index outside the declared span must fail");
   assert_eq!(error, RenderChunkError::IndexOutsideVertexSpan { command: 0, index: 7 });
}

#[test]
fn chunk_proves_nested_ordering_and_rejects_crossed_scopes()
{
   let mut valid = DrawList::default();
   valid.items.extend([
      DrawCmd::ClipPush { rect: RectI::new(0, 0, 100, 100) },
      DrawCmd::LayerBegin { id: 9, rect: RectF::new(0.0, 0.0, 40.0, 30.0), dirty: true },
      DrawCmd::LayerEnd,
      DrawCmd::ClipPop,
   ]);
   let chunk = RenderChunk::new(
      RenderChunkId(3),
      RenderChunkRevisions::default(),
      valid,
      ChunkIndexMode::Local,
      &[],
   )
   .unwrap_or_else(|error| panic!("valid ordering failed: {error}"));
   assert_eq!(chunk.ordering().max_clip_depth, 1);
   assert_eq!(chunk.ordering().max_layer_depth, 1);
   assert!(chunk.ordering().has_clip && chunk.ordering().has_layer);

   let mut crossed = DrawList::default();
   crossed.items.extend([
      DrawCmd::ClipPush { rect: RectI::new(0, 0, 100, 100) },
      DrawCmd::LayerBegin { id: 9, rect: RectF::new(0.0, 0.0, 40.0, 30.0), dirty: true },
      DrawCmd::ClipPop,
      DrawCmd::LayerEnd,
   ]);
   let error = RenderChunk::new(
      RenderChunkId(4),
      RenderChunkRevisions::default(),
      crossed,
      ChunkIndexMode::Local,
      &[],
   )
   .expect_err("crossed scopes must fail once at chunk creation");
   assert_eq!(error, RenderChunkError::OrderingMismatch { command: 2 });
}

#[test]
fn resource_generations_invalidate_exact_dependent_chunks()
{
   let image = ImageHandle(11);
   let atlas = ImageHandle(12);
   let mut image_list = DrawList::default();
   image_list.items.push(DrawCmd::Image {
      tex: image,
      dst: RectF::new(0.0, 0.0, 20.0, 10.0),
      src: RectF::new(0.0, 0.0, 1.0, 1.0),
      alpha: 1.0,
   });
   let image_chunk = RenderChunk::new(
      RenderChunkId(10),
      RenderChunkRevisions::default(),
      image_list,
      ChunkIndexMode::Local,
      &[RenderResourceDependency { image, generation: 5 }],
   )
   .unwrap_or_else(|error| panic!("image chunk failed: {error}"));

   let mut glyph_list = mesh_list(4, &[0, 1, 2]);
   glyph_list.items.clear();
   glyph_list.items.push(DrawCmd::GlyphRun {
      run: GlyphRun {
         atlas,
         atlas_revision: 8,
         vb: VertexSpan { offset: 4, len: 3 },
         ib: IndexSpan { offset: 0, len: 3 },
         sdf: true,
         color: Color::rgba(1.0, 1.0, 1.0, 1.0),
      },
   });
   let glyph_chunk = RenderChunk::new(
      RenderChunkId(20),
      RenderChunkRevisions::default(),
      glyph_list,
      ChunkIndexMode::Local,
      &[],
   )
   .unwrap_or_else(|error| panic!("glyph chunk failed: {error}"));

   let snapshot = RenderSnapshot::new(
      vec![
         RenderChunkInstance::new(image_chunk, [0.0, 0.0]),
         RenderChunkInstance::new(glyph_chunk, [0.0, 0.0]),
      ],
      vec![],
      Damage { rects: vec![] },
   )
   .unwrap_or_else(|error| panic!("snapshot failed: {error}"));
   assert_eq!(
      snapshot.incompatible_chunk_ids(&[
         RenderResourceDependency { image, generation: 6 },
         RenderResourceDependency { image: atlas, generation: 8 },
      ]),
      vec![RenderChunkId(10)]
   );
   assert_eq!(
      snapshot.incompatible_chunk_ids(&[
         RenderResourceDependency { image, generation: 5 },
         RenderResourceDependency { image: atlas, generation: 9 },
      ]),
      vec![RenderChunkId(20)]
   );
}

#[test]
fn flat_fallback_applies_instance_metadata_and_reports_every_copy()
{
   let chunk = shape_chunk(30);
   let mut instance = RenderChunkInstance::new(chunk, [10.0, 20.0]);
   instance.property_slots = Arc::from([
      RenderPropertySlotId(1),
      RenderPropertySlotId(2),
   ]);
   instance.clip = Some(RectI::new(0, 0, 100, 100));
   instance.layer = Some(RenderLayerInstance {
      id: 7,
      rect: RectF::new(0.0, 0.0, 100.0, 100.0),
      dirty: false,
   });
   let snapshot = RenderSnapshot::new(
      vec![instance],
      vec![
         RenderPropertySlot {
            id: RenderPropertySlotId(2),
            revision: 4,
            value: RenderPropertyValue::Opacity(0.5),
         },
         RenderPropertySlot {
            id: RenderPropertySlotId(1),
            revision: 3,
            value: RenderPropertyValue::Transform([1.0, 0.0, 0.0, 1.0, 2.0, 3.0]),
         },
      ],
      Damage { rects: vec![RectI::new(1, 2, 3, 4)] },
   )
   .unwrap_or_else(|error| panic!("snapshot failed: {error}"));

   let mut flat = DrawList::default();
   let stats = snapshot.flatten_into(&mut flat).unwrap_or_else(|error| panic!("flatten failed: {error}"));
   assert_eq!(stats.fallback_count, 1);
   assert_eq!(stats.chunks_flattened, 1);
   assert_eq!(stats.commands_copied, 5);
   assert_eq!(stats.vertices_copied, 0);
   assert_eq!(stats.indices_copied, 0);
   assert_eq!(flat.items.len(), 5);
   assert!(matches!(
      flat.items[0],
      DrawCmd::LayerBegin { rect, .. } if rect == RectF::new(12.0, 23.0, 100.0, 100.0)
   ));
   assert_eq!(flat.items[1], DrawCmd::ClipPush { rect: RectI::new(12, 23, 100, 100) });
   match &flat.items[2] {
      DrawCmd::RRect { rect, color, .. } => {
         assert_eq!(*rect, RectF::new(12.0, 23.0, 20.0, 10.0));
         assert_eq!(color.a, 0.4);
      }
      command => panic!("unexpected flattened command: {command:?}"),
   }
}

#[test]
fn flat_fallback_applies_opacity_to_packed_vertex_colors()
{
   let list = DrawList {
      items: vec![DrawCmd::Solid {
         vb: VertexSpan { offset: 0, len: 3 },
         ib: IndexSpan { offset: 0, len: 3 },
         color: Color::rgba(1.0, 1.0, 1.0, 1.0),
      }],
      vertices: vec![
         Vertex { rgba: 0x8000_00ff, ..vertex(0.0, 0.0) },
         Vertex { rgba: 0x8000_ff00, ..vertex(1.0, 0.0) },
         Vertex { rgba: 0x80ff_0000, ..vertex(0.0, 1.0) },
      ],
      indices: vec![0, 1, 2],
   };
   let chunk = RenderChunk::new(
      RenderChunkId(31),
      RenderChunkRevisions::default(),
      list,
      ChunkIndexMode::Local,
      &[],
   ).unwrap_or_else(|error| panic!("chunk failed: {error}"));
   let mut instance = RenderChunkInstance::new(chunk, [0.0, 0.0]);
   instance.property_slots = Arc::from([RenderPropertySlotId(3)]);
   let snapshot = RenderSnapshot::new(
      vec![instance],
      vec![RenderPropertySlot {
         id: RenderPropertySlotId(3),
         revision: 1,
         value: RenderPropertyValue::Opacity(0.5),
      }],
      Damage { rects: vec![] },
   ).unwrap_or_else(|error| panic!("snapshot failed: {error}"));
   let mut flat = DrawList::default();
   snapshot.flatten_into(&mut flat).unwrap_or_else(|error| panic!("flatten failed: {error}"));
   assert_eq!(flat.vertices[0].rgba, 0x4000_00ff);
   assert_eq!(flat.vertices[1].rgba, 0x4000_ff00);
   assert_eq!(flat.vertices[2].rgba, 0x40ff_0000);
}

#[test]
fn snapshot_rejects_missing_properties_and_flatten_rejects_lossy_transform()
{
   let mut missing = RenderChunkInstance::new(shape_chunk(40), [0.0, 0.0]);
   missing.property_slots = Arc::from([RenderPropertySlotId(9)]);
   let error = RenderSnapshot::new(
      vec![missing],
      vec![],
      Damage { rects: vec![] },
   )
   .expect_err("missing slot must fail at snapshot creation");
   assert_eq!(error, RenderSnapshotError::MissingPropertySlot(RenderPropertySlotId(9)));

   let mut rotated = RenderChunkInstance::new(shape_chunk(41), [0.0, 0.0]);
   rotated.property_slots = Arc::from([RenderPropertySlotId(10)]);
   let snapshot = RenderSnapshot::new(
      vec![rotated],
      vec![RenderPropertySlot {
         id: RenderPropertySlotId(10),
         revision: 1,
         value: RenderPropertyValue::Transform([0.0, 1.0, -1.0, 0.0, 0.0, 0.0]),
      }],
      Damage { rects: vec![] },
   )
   .unwrap_or_else(|error| panic!("rotated snapshot failed: {error}"));
   let error = snapshot.flatten_into(&mut DrawList::default()).expect_err("flat DrawList cannot preserve rotation");
   assert_eq!(error, RenderSnapshotError::UnsupportedFlatTransform(RenderPropertySlotId(10)));
}

#[test]
fn revisions_identity_byte_size_order_and_damage_are_retained()
{
   let revisions = RenderChunkRevisions {
      structural: 11,
      geometry: 12,
      resource: 13,
      dynamic_properties: 14,
   };
   let mut list = DrawList::default();
   list.items.push(DrawCmd::RRect {
      rect: RectF::new(1.0, 2.0, 3.0, 4.0),
      radii: [1.0; 4],
      color: Color::rgba(0.1, 0.2, 0.3, 1.0),
   });
   let chunk = RenderChunk::new(
      RenderChunkId(50),
      revisions,
      list,
      ChunkIndexMode::Local,
      &[],
   ).unwrap_or_else(|error| panic!("chunk failed: {error}"));
   let clone = chunk.clone();
   assert_eq!(chunk.revisions(), revisions);
   assert!(chunk.ptr_eq(&clone));
   assert_eq!(chunk.byte_size(), core::mem::size_of::<DrawCmd>() as u64);

   let snapshot = RenderSnapshot::new(
      vec![
         RenderChunkInstance::new(chunk, [0.0, 0.0]),
         RenderChunkInstance::new(shape_chunk(51), [10.0, 20.0]),
      ],
      vec![],
      Damage { rects: vec![RectI::new(1, 2, 3, 4)] },
   ).unwrap_or_else(|error| panic!("snapshot failed: {error}"));
   assert_eq!(snapshot.instances()[0].chunk.id(), RenderChunkId(50));
   assert_eq!(snapshot.instances()[1].chunk.id(), RenderChunkId(51));
   assert_eq!(snapshot.damage().rects, [RectI::new(1, 2, 3, 4)]);
}
