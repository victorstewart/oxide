//! Raw-touch vertical scrolling with deterministic inertial decay.

use oxide_input::{TouchSurfaceEvent, TouchSurfaceRecognizer};
use oxide_platform_api::{TouchEvent, TouchPhase};

use crate::scroll_state::ScrollState;

const DRAG_SLOP_POINTS: f32 = 6.0;
const DECELERATION_PER_MS: f64 = 0.998;
const MAX_RELEASE_VELOCITY_POINTS_PER_S: f64 = 8_000.0;
const SETTLE_VELOCITY_POINTS_PER_S: f64 = 5.0;
const VELOCITY_SAMPLE_CAP: usize = 8;
const VELOCITY_WINDOW_NS: u64 = 100_000_000;
const MIN_VELOCITY_SPAN_NS: u64 = 8_000_000;

#[derive(Clone, Copy, Debug, Default)]
struct VelocitySample
{
   timestamp_ns: u64,
   y: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum DragPhase
{
   #[default]
   Idle,
   Pending { origin_y: f32 },
   Dragging,
}

/// Owns one vertical raw-touch interaction and its clamped inertial motion.
#[derive(Clone, Debug)]
pub struct VerticalScrollSurface
{
   scroll: ScrollState,
   position: f64,
   touches: TouchSurfaceRecognizer,
   drag: DragPhase,
   samples: [VelocitySample; VELOCITY_SAMPLE_CAP],
   sample_head: usize,
   sample_len: usize,
   velocity_points_per_s: f64,
   last_advance_ns: Option<u64>,
}

impl VerticalScrollSurface
{
   /// Creates a surface at offset zero with sanitized content and viewport extents.
   #[must_use]
   pub fn new(content_extent: f32, viewport_extent: f32) -> Self
   {
      Self {
         scroll: ScrollState::new(content_extent, viewport_extent),
         position: 0.0,
         touches: TouchSurfaceRecognizer::new(),
         drag: DragPhase::Idle,
         samples: [VelocitySample::default(); VELOCITY_SAMPLE_CAP],
         sample_head: 0,
         sample_len: 0,
         velocity_points_per_s: 0.0,
         last_advance_ns: None,
      }
   }

   /// Updates geometry, clamps the current offset, and reports a visible offset change.
   pub fn update_extents(&mut self, content_extent: f32, viewport_extent: f32) -> bool
   {
      let before = self.scroll.offset();
      self.scroll.update_extents(content_extent, viewport_extent);
      self.position = self.position.clamp(0.0, f64::from(self.scroll.max_offset()));
      self.scroll.set_offset(self.position as f32);
      self.stop_outward_motion_at_bound();
      self.scroll.offset() != before
   }

   /// Jumps to an offset and cancels any touch or inertial motion.
   pub fn set_offset(&mut self, offset: f32) -> f32
   {
      self.cancel_motion();
      let offset = self.scroll.set_offset(offset);
      self.position = f64::from(offset);
      offset
   }

   #[must_use]
   pub fn offset(&self) -> f32
   {
      self.scroll.offset()
   }

   #[must_use]
   pub fn max_offset(&self) -> f32
   {
      self.scroll.max_offset()
   }

   #[must_use]
   pub fn progress(&self) -> f32
   {
      self.scroll.progress()
   }

   /// Consumes one raw touch and reports whether the visible offset changed.
   pub fn input_touch(&mut self, event: &TouchEvent) -> bool
   {
      let offset_before = self.scroll.offset();
      let active_before = self.touches.active_count();
      for surface_event in self.touches.on_touch(event)
      {
         self.handle_surface_event(surface_event, event.timestamp_ns);
      }

      let active_after = self.touches.active_count();
      if active_before == 1 && active_after == 0
      {
         match event.phase
         {
            TouchPhase::End => self.finish_touch(event.y, event.timestamp_ns),
            TouchPhase::Cancel => self.cancel_drag(),
            TouchPhase::Start | TouchPhase::Move => {}
         }
      }
      self.scroll.offset() != offset_before
   }

   /// Advances inertia to an explicit monotonic timestamp and reports an offset change.
   pub fn advance_to(&mut self, timestamp_ns: u64) -> bool
   {
      if timestamp_ns == 0 || self.velocity_points_per_s == 0.0 || self.touches.active_count() != 0
      {
         return false;
      }
      if self.velocity_points_per_s.abs() <= SETTLE_VELOCITY_POINTS_PER_S
      {
         self.stop_inertia();
         return false;
      }
      let Some(last_ns) = self.last_advance_ns else
      {
         self.last_advance_ns = Some(timestamp_ns);
         return false;
      };
      if timestamp_ns <= last_ns
      {
         return false;
      }

      let offset_before = self.scroll.offset();
      let elapsed_ms = timestamp_ns.saturating_sub(last_ns) as f64 / 1_000_000.0;
      let velocity = self.velocity_points_per_s;
      let decay = DECELERATION_PER_MS.powf(elapsed_ms);
      let next_velocity = velocity * decay;
      let denominator = 1_000.0 * (1.0 - DECELERATION_PER_MS);
      let (distance, settled) = if next_velocity.abs() <= SETTLE_VELOCITY_POINTS_PER_S
      {
         let terminal_decay = SETTLE_VELOCITY_POINTS_PER_S / velocity.abs();
         (velocity * (1.0 - terminal_decay) / denominator, true)
      }
      else
      {
         (velocity * (1.0 - decay) / denominator, false)
      };

      if !distance.is_finite()
      {
         self.stop_inertia();
         return false;
      }
      let max_offset = f64::from(self.scroll.max_offset());
      self.position = (self.position + distance).clamp(0.0, max_offset);
      self.scroll.set_offset(self.position as f32);
      let reached_bound = (self.position <= 0.0 && velocity < 0.0)
         || (self.position >= max_offset && velocity > 0.0);
      if settled || reached_bound
      {
         self.stop_inertia();
      }
      else
      {
         self.velocity_points_per_s = next_velocity;
         self.last_advance_ns = Some(timestamp_ns);
      }
      self.scroll.offset() != offset_before
   }

   /// Returns true only while inertia requires another frame callback.
   #[must_use]
   pub fn wants_next_frame(&self) -> bool
   {
      self.velocity_points_per_s != 0.0
   }

   /// Returns true only after all contacts and inertial motion have ended.
   #[must_use]
   pub fn is_settled(&self) -> bool
   {
      self.touches.active_count() == 0
         && self.velocity_points_per_s == 0.0
         && matches!(self.drag, DragPhase::Idle)
   }

   /// Cancels contacts and inertia without changing the current offset or extents.
   pub fn cancel_motion(&mut self)
   {
      self.touches.reset();
      self.cancel_drag();
   }

   fn handle_surface_event(&mut self, event: TouchSurfaceEvent, timestamp_ns: u64)
   {
      match event
      {
         TouchSurfaceEvent::ActiveTouchesChanged { touch_count: 1, y, .. } =>
         {
            self.begin_touch(y, timestamp_ns);
         }
         TouchSurfaceEvent::ActiveTouchesChanged { touch_count, .. } if touch_count >= 2 =>
         {
            self.cancel_drag();
         }
         TouchSurfaceEvent::ActiveTouchesChanged { .. } => {}
         TouchSurfaceEvent::Pan { touch_count: 1, y, dy, .. } =>
         {
            self.drag_by(y, dy, timestamp_ns);
         }
         TouchSurfaceEvent::Pan { .. } | TouchSurfaceEvent::Pinch { .. } => {}
      }
   }

   fn begin_touch(&mut self, y: f32, timestamp_ns: u64)
   {
      self.stop_inertia();
      self.reset_samples();
      self.drag = DragPhase::Pending { origin_y: y };
      self.record_sample(y, timestamp_ns);
   }

   fn drag_by(&mut self, y: f32, dy: f32, timestamp_ns: u64)
   {
      if !y.is_finite() || !dy.is_finite()
      {
         return;
      }
      if dy != 0.0
      {
         self.record_sample(y, timestamp_ns);
      }
      let delta = match self.drag
      {
         DragPhase::Pending { origin_y } if (y - origin_y).abs() >= DRAG_SLOP_POINTS =>
         {
            self.drag = DragPhase::Dragging;
            -(y - origin_y)
         }
         DragPhase::Dragging => -dy,
         DragPhase::Idle | DragPhase::Pending { .. } => return,
      };
      self.scroll.scroll_by(delta);
      self.position = f64::from(self.scroll.offset());
   }

   fn finish_touch(&mut self, release_y: f32, timestamp_ns: u64)
   {
      let was_dragging = matches!(self.drag, DragPhase::Dragging);
      if was_dragging && release_y.is_finite()
      {
         if let Some(latest) = self.latest_sample()
         {
            self.scroll.scroll_by(-(release_y - latest.y));
            self.position = f64::from(self.scroll.offset());
         }
      }
      self.drag = DragPhase::Idle;
      self.velocity_points_per_s = if was_dragging
      {
         -self.release_velocity(release_y, timestamp_ns)
      }
      else
      {
         0.0
      };
      self.velocity_points_per_s = self.velocity_points_per_s.clamp(
         -MAX_RELEASE_VELOCITY_POINTS_PER_S,
         MAX_RELEASE_VELOCITY_POINTS_PER_S,
      );
      if self.velocity_points_per_s.abs() <= SETTLE_VELOCITY_POINTS_PER_S
      {
         self.velocity_points_per_s = 0.0;
      }
      self.last_advance_ns = if self.velocity_points_per_s == 0.0
      {
         None
      }
      else
      {
         self.release_timestamp(timestamp_ns)
      };
      self.reset_samples();
      self.stop_outward_motion_at_bound();
   }

   fn release_velocity(&self, release_y: f32, timestamp_ns: u64) -> f64
   {
      let Some(latest) = self.latest_sample() else
      {
         return 0.0;
      };
      let end_ns = if timestamp_ns == 0 { latest.timestamp_ns } else { timestamp_ns };
      let end_y = if release_y.is_finite() { release_y } else { latest.y };
      if end_ns == 0 || !end_y.is_finite()
      {
         return 0.0;
      }
      let cutoff_ns = end_ns.saturating_sub(VELOCITY_WINDOW_NS);
      let mut oldest = None;
      for offset in 0..self.sample_len
      {
         let index = (self.sample_head + VELOCITY_SAMPLE_CAP - self.sample_len + offset)
            % VELOCITY_SAMPLE_CAP;
         let sample = self.samples[index];
         if sample.timestamp_ns >= cutoff_ns && sample.timestamp_ns < end_ns
         {
            oldest = Some(sample);
            break;
         }
      }
      let Some(oldest) = oldest else
      {
         return 0.0;
      };
      let elapsed_ns = end_ns.saturating_sub(oldest.timestamp_ns);
      if elapsed_ns < MIN_VELOCITY_SPAN_NS
      {
         return 0.0;
      }
      f64::from(end_y - oldest.y) * 1_000_000_000.0 / elapsed_ns as f64
   }

   fn record_sample(&mut self, y: f32, timestamp_ns: u64)
   {
      if timestamp_ns == 0 || !y.is_finite()
      {
         return;
      }
      if let Some(latest) = self.latest_sample()
      {
         if timestamp_ns < latest.timestamp_ns
         {
            return;
         }
         if timestamp_ns == latest.timestamp_ns
         {
            let latest_index =
               (self.sample_head + VELOCITY_SAMPLE_CAP - 1) % VELOCITY_SAMPLE_CAP;
            self.samples[latest_index] = VelocitySample { timestamp_ns, y };
            return;
         }
      }
      self.samples[self.sample_head] = VelocitySample { timestamp_ns, y };
      self.sample_head = (self.sample_head + 1) % VELOCITY_SAMPLE_CAP;
      self.sample_len = (self.sample_len + 1).min(VELOCITY_SAMPLE_CAP);
   }

   fn latest_sample(&self) -> Option<VelocitySample>
   {
      if self.sample_len == 0
      {
         return None;
      }
      let index = (self.sample_head + VELOCITY_SAMPLE_CAP - 1) % VELOCITY_SAMPLE_CAP;
      Some(self.samples[index])
   }

   fn release_timestamp(&self, timestamp_ns: u64) -> Option<u64>
   {
      if timestamp_ns != 0
      {
         Some(timestamp_ns)
      }
      else
      {
         self.latest_sample().map(|sample| sample.timestamp_ns).filter(|value| *value != 0)
      }
   }

   fn stop_outward_motion_at_bound(&mut self)
   {
      let offset = self.scroll.offset();
      if (offset <= 0.0 && self.velocity_points_per_s < 0.0)
         || (offset >= self.scroll.max_offset() && self.velocity_points_per_s > 0.0)
      {
         self.stop_inertia();
      }
   }

   fn cancel_drag(&mut self)
   {
      self.drag = DragPhase::Idle;
      self.reset_samples();
      self.stop_inertia();
   }

   fn stop_inertia(&mut self)
   {
      self.velocity_points_per_s = 0.0;
      self.last_advance_ns = None;
   }

   fn reset_samples(&mut self)
   {
      self.sample_head = 0;
      self.sample_len = 0;
   }
}
