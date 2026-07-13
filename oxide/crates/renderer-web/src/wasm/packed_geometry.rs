use bytemuck::{Pod, Zeroable};

pub(crate) const PACKED_VERTEX_BYTES: usize = 20;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(crate) struct PackedVertex
{
   pub x: f32,
   pub y: f32,
   pub u: f32,
   pub v: f32,
   pub rgba: u32,
}

impl PackedVertex
{
   pub const fn new(x: f32, y: f32, u: f32, v: f32, rgba: u32) -> Self
   {
      Self { x, y, u, v, rgba }
   }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PackedIndexKind
{
   U16,
   U32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PackedIndexRange
{
   pub kind: PackedIndexKind,
   pub first_index: u32,
   pub index_count: u32,
   pub base_vertex: i32,
}

#[derive(Default)]
pub(crate) struct PackedGeometry
{
   pub vertices: Vec<PackedVertex>,
   pub indices_u16: Vec<u16>,
   pub indices_u32: Vec<u32>,
   vertex_count: u32,
   u16_segment_base: Option<u32>,
}

impl PackedGeometry
{
   pub fn clear(&mut self)
   {
      self.vertices.clear();
      self.indices_u16.clear();
      self.indices_u32.clear();
      self.vertex_count = 0;
      self.u16_segment_base = None;
   }

   #[cfg(test)]
   pub fn append(&mut self, vertices: &[PackedVertex], indices: &[u32]) -> Option<PackedIndexRange>
   {
      if vertices.is_empty() || indices.is_empty() || indices.iter().any(|index| *index as usize >= vertices.len())
      {
         return None;
      }
      self.append_validated(vertices, indices)
   }

   #[inline]
   pub fn append_validated(&mut self, vertices: &[PackedVertex], indices: &[u32]) -> Option<PackedIndexRange>
   {
      if vertices.is_empty() || indices.is_empty()
      {
         return None;
      }
      debug_assert!(indices.iter().all(|index| (*index as usize) < vertices.len()));
      let vertex_count = u32::try_from(vertices.len()).ok()?;
      let index_count = u32::try_from(indices.len()).ok()?;
      let global_vertex = self.vertex_count;
      let next_vertex = global_vertex.checked_add(vertex_count)?;
      let _ = i32::try_from(global_vertex).ok()?;
      self.vertices.extend_from_slice(vertices);
      self.vertex_count = next_vertex;

      if vertex_count <= u16::MAX as u32
      {
         let segment_base = self
            .u16_segment_base
            .filter(|base| next_vertex.saturating_sub(*base) <= u16::MAX as u32)
            .unwrap_or(global_vertex);
         self.u16_segment_base = Some(segment_base);
         let relative_vertex = global_vertex - segment_base;
         let first_index = u32::try_from(self.indices_u16.len()).ok()?;
         self.indices_u16.reserve(indices.len());
         for index in indices.iter().copied()
         {
            self.indices_u16.push((relative_vertex + index) as u16);
         }
         return Some(PackedIndexRange {
            kind: PackedIndexKind::U16,
            first_index,
            index_count,
            base_vertex: i32::try_from(segment_base).ok()?,
         });
      }

      self.u16_segment_base = None;
      let first_index = u32::try_from(self.indices_u32.len()).ok()?;
      self.indices_u32.extend_from_slice(indices);
      Some(PackedIndexRange {
         kind: PackedIndexKind::U32,
         first_index,
         index_count,
         base_vertex: i32::try_from(global_vertex).ok()?,
      })
   }

   #[inline]
   pub fn append_quad(&mut self, vertices: &[PackedVertex; 4]) -> Option<PackedIndexRange>
   {
      let global_vertex = self.vertex_count;
      let next_vertex = global_vertex.checked_add(4)?;
      let segment_base = self
         .u16_segment_base
         .filter(|base| next_vertex.saturating_sub(*base) <= u16::MAX as u32)
         .unwrap_or(global_vertex);
      let base = u16::try_from(global_vertex - segment_base).ok()?;
      let base_vertex = i32::try_from(segment_base).ok()?;
      let first_index = u32::try_from(self.indices_u16.len()).ok()?;
      self.vertices.extend_from_slice(vertices);
      self.indices_u16
         .extend_from_slice(&[base, base + 1, base + 2, base + 2, base + 1, base + 3]);
      self.vertex_count = next_vertex;
      self.u16_segment_base = Some(segment_base);
      Some(PackedIndexRange {
         kind: PackedIndexKind::U16,
         first_index,
         index_count: 6,
         base_vertex,
      })
   }

   pub fn align_uploads(&mut self)
   {
      if self.indices_u16.len() & 1 != 0
      {
         self.indices_u16.push(0);
      }
   }

   pub fn byte_len(&self) -> usize
   {
      self.vertices
         .len()
         .saturating_mul(PACKED_VERTEX_BYTES)
         .saturating_add(self.indices_u16.len().saturating_mul(2))
         .saturating_add(self.indices_u32.len().saturating_mul(4))
   }

   pub fn capacity_bytes(&self) -> usize
   {
      self.vertices
         .capacity()
         .saturating_mul(PACKED_VERTEX_BYTES)
         .saturating_add(self.indices_u16.capacity().saturating_mul(2))
      .saturating_add(self.indices_u32.capacity().saturating_mul(4))
   }
}

#[cfg(test)]
mod tests
{
   use super::{PackedGeometry, PackedIndexKind, PackedVertex, PACKED_VERTEX_BYTES};

   fn vertices(count: usize) -> Vec<PackedVertex>
   {
      (0..count)
         .map(|index| PackedVertex::new(index as f32, -(index as f32), 0.25, 0.75, 0xA1B2_C3D4))
         .collect()
   }

   #[test]
   fn packed_vertex_is_exactly_twenty_little_endian_bytes()
   {
      let mut geometry = PackedGeometry::default();
      let vertex = PackedVertex::new(1.0, -2.0, 0.25, 0.75, 0x4433_2211);
      geometry.append(&[vertex], &[0]).expect("packed vertex");
      let bytes = bytemuck::bytes_of(&geometry.vertices[0]);
      assert_eq!(core::mem::size_of::<PackedVertex>(), PACKED_VERTEX_BYTES);
      assert_eq!(&bytes[0..4], &1.0_f32.to_le_bytes());
      assert_eq!(&bytes[4..8], &(-2.0_f32).to_le_bytes());
      assert_eq!(&bytes[8..12], &0.25_f32.to_le_bytes());
      assert_eq!(&bytes[12..16], &0.75_f32.to_le_bytes());
      assert_eq!(&bytes[16..20], &[0x11, 0x22, 0x33, 0x44]);
   }

   #[test]
   fn u16_segments_rebase_before_the_vertex_span_overflows()
   {
      let mut geometry = PackedGeometry::default();
      let first = geometry.append(&vertices(40_000), &[0, 39_999]).expect("first segment");
      let second = geometry.append(&vertices(30_000), &[0, 29_999]).expect("second segment");
      assert_eq!(first.kind, PackedIndexKind::U16);
      assert_eq!(first.base_vertex, 0);
      assert_eq!(second.kind, PackedIndexKind::U16);
      assert_eq!(second.base_vertex, 40_000);
      assert_eq!(&geometry.indices_u16[2..4], &[0, 29_999]);
   }

   #[test]
   fn packed_quads_rebase_without_changing_triangle_order()
   {
      let mut geometry = PackedGeometry::default();
      let quad = [
         PackedVertex::new(0.0, 0.0, 0.0, 0.0, 1),
         PackedVertex::new(1.0, 0.0, 1.0, 0.0, 2),
         PackedVertex::new(0.0, 1.0, 0.0, 1.0, 3),
         PackedVertex::new(1.0, 1.0, 1.0, 1.0, 4),
      ];
      let first = geometry.append_quad(&quad).expect("first quad");
      for _ in 1..16_383
      {
         geometry.append_quad(&quad).expect("segment quad");
      }
      let rebased = geometry.append_quad(&quad).expect("rebased quad");
      assert_eq!(first.base_vertex, 0);
      assert_eq!(rebased.base_vertex, 65_532);
      assert_eq!(rebased.first_index, 98_298);
      assert_eq!(&geometry.indices_u16[0..6], &[0, 1, 2, 2, 1, 3]);
   }

   #[test]
   fn oversized_arbitrary_mesh_uses_u32_indices_and_resets_u16_segment()
   {
      let mut geometry = PackedGeometry::default();
      let first = geometry.append(&vertices(4), &[0, 1, 2]).expect("small mesh");
      let large = geometry
         .append(&vertices(65_536), &[0, 32_768, 65_535])
         .expect("large mesh");
      let trailing = geometry.append(&vertices(4), &[0, 2, 3]).expect("trailing mesh");
      assert_eq!(first.kind, PackedIndexKind::U16);
      assert_eq!(large.kind, PackedIndexKind::U32);
      assert_eq!(large.base_vertex, 4);
      assert_eq!(trailing.kind, PackedIndexKind::U16);
      assert_eq!(trailing.base_vertex, 65_540);
      assert_eq!(geometry.vertex_count, 65_544);
      assert_eq!(geometry.indices_u32.len(), 3);
   }

   #[test]
   fn invalid_indices_leave_streams_unchanged()
   {
      let mut geometry = PackedGeometry::default();
      assert!(geometry.append(&vertices(3), &[0, 3]).is_none());
      assert!(geometry.vertices.is_empty());
      assert!(geometry.indices_u16.is_empty());
      assert!(geometry.indices_u32.is_empty());
   }

   #[test]
   fn u16_upload_padding_does_not_change_index_ranges()
   {
      let mut geometry = PackedGeometry::default();
      let range = geometry.append(&vertices(3), &[0, 1, 2]).expect("triangle");
      geometry.align_uploads();
      assert_eq!(range.first_index, 0);
      assert_eq!(range.index_count, 3);
      assert_eq!(geometry.indices_u16, [0, 1, 2, 0]);
   }
}
