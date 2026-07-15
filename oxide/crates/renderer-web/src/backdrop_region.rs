use oxide_renderer_api as api;

const LINEAR_FILTER_OUTSET_PIXELS: f64 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PhysicalCopyRegion
{
   pub source_x: u32,
   pub source_y: u32,
   pub destination_x: u32,
   pub destination_y: u32,
   pub width: u32,
   pub height: u32,
}

#[cfg(target_arch = "wasm32")]
impl PhysicalCopyRegion
{
   pub const fn pixels(self) -> u64
   {
      (self.width as u64).saturating_mul(self.height as u64)
   }
}

pub(crate) fn backdrop_sample_bounds(rect: api::RectF, clip: api::RectI, sigma: f32, scale: f32) -> Option<api::RectF>
{
   let values = [rect.x, rect.y, rect.w, rect.h, sigma, scale];
   if !values.iter().all(|value| value.is_finite()) || rect.w <= 0.0 || rect.h <= 0.0
      || clip.w <= 0 || clip.h <= 0 || scale <= 0.0
   {
      return None;
   }
   let clip_x1 = clip.x.saturating_add(clip.w);
   let clip_y1 = clip.y.saturating_add(clip.h);
   let x0 = rect.x.max(clip.x as f32);
   let y0 = rect.y.max(clip.y as f32);
   let x1 = (rect.x + rect.w).min(clip_x1 as f32);
   let y1 = (rect.y + rect.h).min(clip_y1 as f32);
   if x1 <= x0 || y1 <= y0
   {
      return None;
   }
   let radius = f64::from(sigma.clamp(0.0, 96.0));
   let outset = ((radius * 0.35).max(1.0) + LINEAR_FILTER_OUTSET_PIXELS)
      / f64::from(scale);
   Some(api::RectF::new(
      (f64::from(x0) - outset) as f32,
      (f64::from(y0) - outset) as f32,
      (f64::from(x1 - x0) + outset * 2.0) as f32,
      (f64::from(y1 - y0) + outset * 2.0) as f32,
   ))
}

pub(crate) fn physical_copy_region(
   sample: api::RectF,
   scale: f32,
   canvas_width: u32,
   canvas_height: u32,
   target_x: i64,
   target_y: i64,
   target_width: u32,
   target_height: u32,
) -> Option<PhysicalCopyRegion>
{
   if ![sample.x, sample.y, sample.w, sample.h, scale]
      .iter().all(|value| value.is_finite())
      || sample.w <= 0.0 || sample.h <= 0.0 || scale <= 0.0
   {
      return None;
   }
   let scale = f64::from(scale);
   let sample_x0 = (f64::from(sample.x) * scale).floor() as i64;
   let sample_y0 = (f64::from(sample.y) * scale).floor() as i64;
   let sample_x1 = (f64::from(sample.x + sample.w) * scale).ceil() as i64;
   let sample_y1 = (f64::from(sample.y + sample.h) * scale).ceil() as i64;
   let target_x1 = target_x.saturating_add(i64::from(target_width));
   let target_y1 = target_y.saturating_add(i64::from(target_height));
   let x0 = sample_x0.max(0).max(target_x);
   let y0 = sample_y0.max(0).max(target_y);
   let x1 = sample_x1.min(i64::from(canvas_width)).min(target_x1);
   let y1 = sample_y1.min(i64::from(canvas_height)).min(target_y1);
   if x1 <= x0 || y1 <= y0
   {
      return None;
   }
   Some(PhysicalCopyRegion {
      source_x: x0.saturating_sub(target_x) as u32,
      source_y: y0.saturating_sub(target_y) as u32,
      destination_x: x0 as u32,
      destination_y: y0 as u32,
      width: x1.saturating_sub(x0) as u32,
      height: y1.saturating_sub(y0) as u32,
   })
}

pub(crate) fn copy_regions_overlap(a: PhysicalCopyRegion, b: PhysicalCopyRegion) -> bool
{
   let ax1 = a.destination_x.saturating_add(a.width);
   let ay1 = a.destination_y.saturating_add(a.height);
   let bx1 = b.destination_x.saturating_add(b.width);
   let by1 = b.destination_y.saturating_add(b.height);
   a.destination_x < bx1 && ax1 > b.destination_x
      && a.destination_y < by1 && ay1 > b.destination_y
}

pub(crate) fn coalesce_copy_regions_within(
   regions: &mut Vec<PhysicalCopyRegion>,
   minimum_regions: usize,
   maximum_pixels: u64,
)
{
   if regions.len() < minimum_regions || regions.is_empty()
   {
      return;
   }
   let first = regions[0];
   let delta_x = i64::from(first.destination_x) - i64::from(first.source_x);
   let delta_y = i64::from(first.destination_y) - i64::from(first.source_y);
   let mut x0 = first.destination_x;
   let mut y0 = first.destination_y;
   let mut x1 = first.destination_x.saturating_add(first.width);
   let mut y1 = first.destination_y.saturating_add(first.height);
   for region in &regions[1..]
   {
      if i64::from(region.destination_x) - i64::from(region.source_x) != delta_x
         || i64::from(region.destination_y) - i64::from(region.source_y) != delta_y
      {
         return;
      }
      x0 = x0.min(region.destination_x);
      y0 = y0.min(region.destination_y);
      x1 = x1.max(region.destination_x.saturating_add(region.width));
      y1 = y1.max(region.destination_y.saturating_add(region.height));
   }
   let width = x1.saturating_sub(x0);
   let height = y1.saturating_sub(y0);
   let union_pixels = u64::from(width).saturating_mul(u64::from(height));
   if union_pixels >= maximum_pixels
   {
      return;
   }
   regions.clear();
   regions.push(PhysicalCopyRegion {
      source_x: (i64::from(x0) - delta_x) as u32,
      source_y: (i64::from(y0) - delta_y) as u32,
      destination_x: x0,
      destination_y: y0,
      width,
      height,
   });
}

#[cfg(test)]
mod tests
{
   use super::*;

   #[test]
   fn sample_bounds_clip_before_adding_shader_and_filter_outset()
   {
      let bounds = backdrop_sample_bounds(
         api::RectF::new(-10.0, 10.0, 40.0, 30.0),
         api::RectI::new(0, 0, 20, 20),
         20.0,
         2.0,
      ).expect("visible backdrop");
      assert_eq!(bounds, api::RectF::new(-4.0, 6.0, 28.0, 18.0));
   }

   #[test]
   fn physical_region_preserves_layer_local_source_and_global_destination()
   {
      let region = physical_copy_region(
         api::RectF::new(8.0, 12.0, 30.0, 20.0),
         2.0,
         200,
         160,
         20,
         10,
         80,
         70,
      ).expect("layer intersection");
      assert_eq!(region, PhysicalCopyRegion {
         source_x: 0,
         source_y: 14,
         destination_x: 20,
         destination_y: 24,
         width: 56,
         height: 40,
      });
   }

   #[test]
   fn physical_region_clips_edges_and_skips_empty_samples()
   {
      assert_eq!(
         physical_copy_region(
            api::RectF::new(-3.0, -2.0, 8.0, 7.0),
            2.0,
            100,
            80,
            0,
            0,
            100,
            80,
         ),
         Some(PhysicalCopyRegion {
            source_x: 0,
            source_y: 0,
            destination_x: 0,
            destination_y: 0,
            width: 10,
            height: 10,
         }),
      );
      assert_eq!(
         physical_copy_region(
            api::RectF::new(80.0, 80.0, 10.0, 10.0),
            1.0,
            64,
            64,
            0,
            0,
            64,
            64,
         ),
         None,
      );
   }

   #[test]
   fn physical_overlap_is_strict_and_uses_global_destination_space()
   {
      let a = PhysicalCopyRegion {
         source_x: 0,
         source_y: 0,
         destination_x: 20,
         destination_y: 10,
         width: 20,
         height: 20,
      };
      assert!(!copy_regions_overlap(a, PhysicalCopyRegion {
         destination_x: 40,
         ..a
      }));
      assert!(copy_regions_overlap(a, PhysicalCopyRegion {
         source_x: 19,
         destination_x: 39,
         ..a
      }));
   }

   #[test]
   fn epoch_union_requires_enough_regions_and_stays_smaller_than_the_full_copy()
   {
      let region = |x| PhysicalCopyRegion {
         source_x: x,
         source_y: 10,
         destination_x: x,
         destination_y: 10,
         width: 10,
         height: 10,
      };
      let mut regions = vec![region(10), region(30), region(50), region(70)];
      coalesce_copy_regions_within(&mut regions, 4, 1_000);
      assert_eq!(regions, vec![PhysicalCopyRegion {
         source_x: 10,
         source_y: 10,
         destination_x: 10,
         destination_y: 10,
         width: 70,
         height: 10,
      }]);

      let mut regions = vec![region(10), region(30), region(50), region(70)];
      coalesce_copy_regions_within(&mut regions, 4, 700);
      assert_eq!(regions.len(), 4);
   }
}
