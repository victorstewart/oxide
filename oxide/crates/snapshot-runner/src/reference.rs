#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdMaskFieldSeed
{
   pub x: i16,
   pub y: i16,
   pub city: u8,
   pub neighborhood: u8,
}

impl IdMaskFieldSeed
{
   pub const INVALID: Self = Self { x: -1, y: -1, city: 0, neighborhood: 0 };

   #[must_use]
   pub fn valid(self) -> bool
   {
      self.x >= 0 && self.y >= 0 && self.city != 0
   }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdMaskFields
{
   pub width: usize,
   pub height: usize,
   pub city: Vec<IdMaskFieldSeed>,
   pub seam: Vec<IdMaskFieldSeed>,
}

#[must_use]
pub fn asymmetric_id_mask_fixture() -> (usize, usize, Vec<u8>, Vec<u8>)
{
   let width = 17;
   let height = 11;
   let mut city = vec![0_u8; width * height];
   let mut neighborhood = vec![0_u8; city.len()];
   for (x, y, city_id, neighborhood_id) in [
      (0, 5, 1, 3),
      (1, 5, 1, 7),
      (5, 1, 2, 11),
      (13, 8, 3, 19),
   ]
   {
      let index = y * width + x;
      city[index] = city_id;
      neighborhood[index] = neighborhood_id;
   }
   (width, height, city, neighborhood)
}

#[must_use]
pub fn id_mask_seed_fields(width: usize, height: usize, city: &[u8], neighborhood: &[u8]) -> IdMaskFields
{
   assert_eq!(city.len(), width.saturating_mul(height));
   assert_eq!(neighborhood.len(), city.len());
   let mut city_field = vec![IdMaskFieldSeed::INVALID; city.len()];
   let mut seam_field = vec![IdMaskFieldSeed::INVALID; city.len()];
   for y in 0..height
   {
      for x in 0..width
      {
         let index = y * width + x;
         let city_id = city[index];
         let neighborhood_id = neighborhood[index];
         if city_id == 0
         {
            continue;
         }
         city_field[index] = IdMaskFieldSeed {
            x: x as i16,
            y: y as i16,
            city: city_id,
            neighborhood: neighborhood_id,
         };
         if neighborhood_id != 0 && has_other_neighborhood(width, height, city, neighborhood, x, y)
         {
            seam_field[index] = IdMaskFieldSeed {
               x: x as i16,
               y: y as i16,
               city: city_id,
               neighborhood: 1,
            };
         }
      }
   }
   IdMaskFields { width, height, city: city_field, seam: seam_field }
}

#[must_use]
pub fn id_mask_jump_schedule(width: usize, height: usize) -> Vec<usize>
{
   let mut jump = width.max(height).max(1).next_power_of_two() / 2;
   let mut jumps = Vec::new();
   while jump >= 1
   {
      jumps.push(jump);
      jump /= 2;
   }
   jumps
}

#[must_use]
pub fn id_mask_jump_fields(fields: &IdMaskFields, jump: usize) -> IdMaskFields
{
   assert!(jump > 0);
   IdMaskFields {
      width: fields.width,
      height: fields.height,
      city: jump_field(fields.width, fields.height, &fields.city, jump),
      seam: jump_field(fields.width, fields.height, &fields.seam, jump),
   }
}

#[must_use]
pub fn id_mask_field_rgba(width: usize, height: usize, field: &[IdMaskFieldSeed]) -> Vec<u8>
{
   assert_eq!(field.len(), width.saturating_mul(height));
   let x_scale = 255.0 / width.saturating_sub(1).max(1) as f32;
   let y_scale = 255.0 / height.saturating_sub(1).max(1) as f32;
   let mut rgba = Vec::with_capacity(field.len().saturating_mul(4));
   for seed in field
   {
      if seed.valid()
      {
         rgba.extend_from_slice(&[
            (seed.x as f32 * x_scale).round() as u8,
            (seed.y as f32 * y_scale).round() as u8,
            seed.city,
            seed.neighborhood.max(1),
         ]);
      }
      else
      {
         rgba.extend_from_slice(&[0, 0, 0, 0]);
      }
   }
   rgba
}

#[must_use]
pub fn id_mask_fields_rgba(fields: &IdMaskFields) -> Vec<u8>
{
   let mut rgba = id_mask_field_rgba(fields.width, fields.height, &fields.city);
   rgba.extend_from_slice(&id_mask_field_rgba(fields.width, fields.height, &fields.seam));
   rgba
}

fn has_other_neighborhood(width: usize, height: usize, city: &[u8], neighborhood: &[u8], x: usize, y: usize) -> bool
{
   let index = y * width + x;
   let city_id = city[index];
   let neighborhood_id = neighborhood[index];
   for oy in -1_i32..=1
   {
      for ox in -1_i32..=1
      {
         if ox == 0 && oy == 0
         {
            continue;
         }
         let qx = x as i32 + ox;
         let qy = y as i32 + oy;
         if qx < 0 || qy < 0 || qx >= width as i32 || qy >= height as i32
         {
            continue;
         }
         let other = qy as usize * width + qx as usize;
         if city[other] == city_id
            && neighborhood[other] != 0
            && neighborhood[other] != neighborhood_id
         {
            return true;
         }
      }
   }
   false
}

fn jump_field(width: usize, height: usize, source: &[IdMaskFieldSeed], jump: usize) -> Vec<IdMaskFieldSeed>
{
   let mut output = vec![IdMaskFieldSeed::INVALID; source.len()];
   for y in 0..height
   {
      for x in 0..width
      {
         let mut best = source[y * width + x];
         let mut best_distance = seed_distance_squared(best, x, y);
         for oy in -1_i32..=1
         {
            for ox in -1_i32..=1
            {
               if ox == 0 && oy == 0
               {
                  continue;
               }
               let qx = x as i32 + ox * jump as i32;
               let qy = y as i32 + oy * jump as i32;
               if qx < 0 || qy < 0 || qx >= width as i32 || qy >= height as i32
               {
                  continue;
               }
               let candidate = source[qy as usize * width + qx as usize];
               let distance = seed_distance_squared(candidate, x, y);
               if distance < best_distance
               {
                  best = candidate;
                  best_distance = distance;
               }
            }
         }
         output[y * width + x] = best;
      }
   }
   output
}

fn seed_distance_squared(seed: IdMaskFieldSeed, x: usize, y: usize) -> u64
{
   if !seed.valid()
   {
      return u64::MAX;
   }
   let dx = i64::from(seed.x) - x as i64;
   let dy = i64::from(seed.y) - y as i64;
   (dx * dx + dy * dy) as u64
}
