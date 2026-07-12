#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use oxide_renderer_api as api;

pub(crate) fn resolve_vertex_color(rgba: u32, uniform: api::Color) -> api::Color
{
   if rgba == 0
   {
      return uniform;
   }
   api::Color::rgba(
      (rgba & 0xFF) as f32 / 255.0,
      ((rgba >> 8) & 0xFF) as f32 / 255.0,
      ((rgba >> 16) & 0xFF) as f32 / 255.0,
      ((rgba >> 24) & 0xFF) as f32 / 255.0,
   )
}

pub(crate) fn colored_quad(
   vertices: &[api::Vertex],
   uniform: api::Color,
) -> Option<(api::RectF, [f32; 2], [f32; 2], u32, u32)>
{
   if vertices.len() != 6
      || vertices.iter().any(|vertex| !vertex.x.is_finite() || !vertex.y.is_finite())
   {
      return None;
   }

   let min_x = vertices.iter().map(|vertex| vertex.x).fold(f32::INFINITY, f32::min);
   let max_x = vertices.iter().map(|vertex| vertex.x).fold(f32::NEG_INFINITY, f32::max);
   let min_y = vertices.iter().map(|vertex| vertex.y).fold(f32::INFINITY, f32::min);
   let max_y = vertices.iter().map(|vertex| vertex.y).fold(f32::NEG_INFINITY, f32::max);
   if min_x >= max_x || min_y >= max_y
   {
      return None;
   }

   let inherited = uniform.pack_rgba8();
   let mut colors = [None; 4];
   let mut triangles = [0_u8; 2];
   for (index, vertex) in vertices.iter().enumerate()
   {
      let corner = match (vertex.x == min_x, vertex.x == max_x, vertex.y == min_y, vertex.y == max_y)
      {
         (true, false, true, false) => 0,
         (false, true, true, false) => 1,
         (true, false, false, true) => 2,
         (false, true, false, true) => 3,
         _ => return None,
      };
      let rgba = if vertex.rgba == 0 { inherited } else { vertex.rgba };
      if colors[corner].is_some_and(|color| color != rgba)
      {
         return None;
      }
      colors[corner] = Some(rgba);
      triangles[index / 3] |= 1 << corner;
   }
   if !matches!(
      (triangles[0], triangles[1]),
      (0b0111, 0b1110) | (0b1110, 0b0111) | (0b1011, 0b1101) | (0b1101, 0b1011)
   )
   {
      return None;
   }
   let [Some(top_left), Some(top_right), Some(bottom_left), Some(bottom_right)] = colors else
   {
      return None;
   };
   let rect = api::RectF::new(min_x, min_y, max_x - min_x, max_y - min_y);
   if top_left == top_right && top_left == bottom_left && top_left == bottom_right
   {
      return Some((rect, [min_x, min_y], [max_x, min_y], top_left, top_left));
   }
   if top_left == bottom_left && top_right == bottom_right
   {
      return Some((rect, [min_x, min_y], [max_x, min_y], top_left, top_right));
   }
   if top_left == top_right && bottom_left == bottom_right
   {
      return Some((rect, [min_x, min_y], [min_x, max_y], top_left, bottom_left));
   }
   None
}
