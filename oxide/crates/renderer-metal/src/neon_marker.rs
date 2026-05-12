use oxide_renderer_api as api;

pub const NEON_MARKER_MAX_INSTANCES: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NeonMarker {
   pub center: [f32; 2],
   pub core_radius_px: f32,
   pub ring_radius_px: f32,
   pub ring_width_px: f32,
   pub halo_radius_px: f32,
   pub halo_sigma_px: f32,
   pub core_color: api::Color,
   pub ring_color: api::Color,
   pub halo_alpha_max: f32,
   pub ring_alpha_max: f32,
}

impl NeonMarker {
   #[must_use]
   pub fn bounds(self) -> api::RectF
   {
      let radius = self.halo_radius_px.max(self.ring_radius_px + self.ring_width_px).max(self.core_radius_px).max(0.0);
      api::RectF::new(self.center[0] - radius, self.center[1] - radius, radius * 2.0, radius * 2.0)
   }
}

#[derive(Clone, Copy, Debug)]
pub struct NeonMarkerPass<'a> {
   pub viewport: api::RectF,
   pub markers: &'a [NeonMarker],
}

impl<'a> NeonMarkerPass<'a> {
   #[must_use]
   pub fn clamped_len(self) -> usize
   {
      self.markers.len().min(NEON_MARKER_MAX_INSTANCES)
   }
}
