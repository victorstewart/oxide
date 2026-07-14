//! Animation system integration for ui-core nodes.
//! Maps per-node animated properties to draw-time overrides, using timing curves.

use crate::prelude::platform_api as api;
use crate::prelude::renderer_api as gfx;
use crate::NodeId;
use alloc::vec::Vec;
use oxide_timing as timing;

pub mod helpers {
    use super::*;

    pub const REQUIRED_FIELD_SHAKE_PHASE_DURATION_MS: u32 = 35;
    const REQUIRED_FIELD_SHAKE_PHASE_TARGETS: [f32; 12] =
        [-2.0, 2.0, -2.0, 2.0, -2.0, 2.0, -2.0, 2.0, -2.0, 2.0, -2.0, 0.0];
    pub const REQUIRED_FIELD_SHAKE_DURATION_MS: u32 =
        REQUIRED_FIELD_SHAKE_PHASE_DURATION_MS * REQUIRED_FIELD_SHAKE_PHASE_TARGETS.len() as u32;
    pub const DELETE_WIGGLE_BOUNCE_Y: f32 = 2.0;
    pub const DELETE_WIGGLE_BOUNCE_DURATION_MS: u32 = 130;
    pub const DELETE_WIGGLE_BOUNCE_VARIANCE_MS: u32 = 25;
    pub const DELETE_WIGGLE_ROTATE_ANGLE_RAD: f32 = 0.06;
    pub const DELETE_WIGGLE_ROTATE_DURATION_MS: u32 = 130;
    pub const DELETE_WIGGLE_ROTATE_VARIANCE_MS: u32 = 25;

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct DeleteWiggleSample {
        pub rotation_rad: f32,
        pub translation_y: f32,
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum Axis2D {
        Horizontal,
        Vertical,
        Both,
    }

    #[inline]
    pub const fn identity_transform() -> api::Transform2D {
        api::Transform2D { tx: 0.0, ty: 0.0, sx: 1.0, sy: 1.0, rot_rad: 0.0 }
    }

    #[inline]
    pub fn shrink_grow_scale(progress: f32, min_scale: f32, overshoot: f32) -> f32 {
        let t = progress.clamp(0.0, 1.0);
        let (first_span, second_span) = (0.45, 0.75);
        if t <= first_span {
            let local = (t / first_span).clamp(0.0, 1.0);
            lerp(min_scale, overshoot, ease_out_back(local))
        } else if t <= second_span {
            let local = ((t - first_span) / (second_span - first_span)).clamp(0.0, 1.0);
            lerp(overshoot, 1.0, ease_out_cubic(local))
        } else {
            let local = ((t - second_span) / (1.0 - second_span)).clamp(0.0, 1.0);
            lerp(1.0, 1.0, local)
        }
    }

    pub fn shake(
        base: api::Transform2D,
        axis: Axis2D,
        amplitude: f32,
        cycles: u32,
        duration_ms: u32,
    ) -> Vec<api::AnimDesc> {
        if cycles == 0 || duration_ms == 0 {
            return Vec::new();
        }
        let segments = cycles.saturating_mul(2).saturating_add(1);
        let step_ms = ((duration_ms as f32) / segments as f32).ceil() as u32;
        let mut seq: Vec<api::AnimDesc> = Vec::with_capacity(segments as usize + 1);
        let mut delay = 0;
        let mut direction = 1.0_f32;
        let mut from = base;
        for _ in 0..cycles.saturating_mul(2) {
            let to = apply_axis(base, axis, amplitude * direction);
            seq.push(build_transform(from, to, delay, step_ms, api::EaseKind::CubicInOut));
            delay = delay.saturating_add(step_ms);
            direction = -direction;
            from = to;
        }
        seq.push(build_transform(from, base, delay, step_ms, api::EaseKind::CubicOut));
        seq
    }

    pub fn wiggle(
        base: api::Transform2D,
        squish: f32,
        cycles: u32,
        duration_ms: u32,
    ) -> Vec<api::AnimDesc> {
        if cycles == 0 || duration_ms == 0 {
            return Vec::new();
        }
        let segments = cycles.saturating_mul(2).saturating_add(1);
        let step_ms = ((duration_ms as f32) / segments as f32).ceil() as u32;
        let mut seq: Vec<api::AnimDesc> = Vec::with_capacity(segments as usize + 1);
        let mut delay = 0;
        let mut direction = 1.0_f32;
        let mut from = base;
        for _ in 0..cycles.saturating_mul(2) {
            let mut to = base;
            let delta = squish * direction;
            to.sx = (base.sx * (1.0 - delta)).max(0.5);
            to.sy = (base.sy * (1.0 + delta)).max(0.5);
            seq.push(build_transform(from, to, delay, step_ms, api::EaseKind::CubicInOut));
            delay = delay.saturating_add(step_ms);
            direction = -direction;
            from = to;
        }
        seq.push(build_transform(from, base, delay, step_ms, api::EaseKind::CubicOut));
        seq
    }

    /// Samples the old iOS `Wiggler` delete-mode motion.
    ///
    /// The legacy app attached two independent autoreversing CA keyframe
    /// animations: `transform.rotation.z` from `-0.06` to `+0.06` radians and
    /// `transform.translation.y` from `2pt` to `0pt`. Each animation used a
    /// `0.13s` duration with `0.025s` random variance. Oxide makes the variance
    /// deterministic per element seed so retained redraws remain stable while
    /// preserving the old per-view desynchronization.
    #[inline]
    pub fn delete_wiggle_sample(elapsed_ms: u32, seed: u32) -> DeleteWiggleSample {
        let rotate_duration = deterministic_duration_ms(
            DELETE_WIGGLE_ROTATE_DURATION_MS,
            DELETE_WIGGLE_ROTATE_VARIANCE_MS,
            seed,
            0x4f1b_3c29,
        );
        let bounce_duration = deterministic_duration_ms(
            DELETE_WIGGLE_BOUNCE_DURATION_MS,
            DELETE_WIGGLE_BOUNCE_VARIANCE_MS,
            seed,
            0x9d2c_7a51,
        );
        DeleteWiggleSample {
            rotation_rad: sample_autoreversing(
                elapsed_ms,
                rotate_duration,
                -DELETE_WIGGLE_ROTATE_ANGLE_RAD,
                DELETE_WIGGLE_ROTATE_ANGLE_RAD,
            ),
            translation_y: sample_autoreversing(
                elapsed_ms,
                bounce_duration,
                DELETE_WIGGLE_BOUNCE_Y,
                0.0,
            ),
        }
    }

    pub fn scatter(
        base: api::Transform2D,
        offset: [f32; 2],
        duration_ms: u32,
        fade_out: bool,
    ) -> Vec<api::AnimDesc> {
        if duration_ms == 0 {
            return Vec::new();
        }
        let mut seq: Vec<api::AnimDesc> = Vec::new();
        let mut to = base;
        to.tx += offset[0];
        to.ty += offset[1];
        seq.push(build_transform(base, to, 0, duration_ms, api::EaseKind::QuadOut));
        seq.push(build_transform(to, base, duration_ms, duration_ms, api::EaseKind::QuadIn));
        if fade_out {
            seq.push(api::AnimDesc {
                id: 0,
                prop: api::AnimProp::Opacity,
                from: api::AnimValue::F32(1.0),
                to: api::AnimValue::F32(0.0),
                curve: api::AnimCurve::Ease { ease: api::Ease { kind: api::EaseKind::QuadOut } },
                duration_ms,
                delay_ms: 0,
                repeat: api::Repeat::Once,
            });
            seq.push(api::AnimDesc {
                id: 0,
                prop: api::AnimProp::Opacity,
                from: api::AnimValue::F32(0.0),
                to: api::AnimValue::F32(1.0),
                curve: api::AnimCurve::Ease { ease: api::Ease { kind: api::EaseKind::QuadIn } },
                duration_ms,
                delay_ms: duration_ms,
                repeat: api::Repeat::Once,
            });
        }
        seq
    }

    /// Sample a cubic-bezier easing curve by solving the x curve for `progress`
    /// and returning the matching y value.
    pub fn cubic_bezier_ease(progress: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
        let clamped = progress.clamp(0.0, 1.0);
        if clamped <= 0.0 || clamped >= 1.0 {
            return clamped;
        }

        let mut parameter = clamped;
        for _ in 0..6 {
            let x = sample_cubic_curve(parameter, x1, x2) - clamped;
            let dx = sample_cubic_derivative(parameter, x1, x2);
            if dx.abs() < 1.0e-5 {
                break;
            }
            parameter = (parameter - x / dx).clamp(0.0, 1.0);
        }
        sample_cubic_curve(parameter, y1, y2)
    }

    /// Shared ease-in-out profile for legacy swap animations, implemented
    /// entirely in Rust so app crates do not keep local solvers.
    #[inline]
    pub fn cubic_bezier_ease_in_out(progress: f32) -> f32 {
        cubic_bezier_ease(progress, 0.42, 0.0, 0.58, 1.0)
    }

    /// Sample a phase-keyframed offset list where each entry is the target for
    /// one fixed-duration phase. The offset starts at `0.0` and returns to
    /// `0.0` once the sampled duration has elapsed.
    pub fn sample_keyframed_offset(
        elapsed_ms: u32,
        phase_duration_ms: u32,
        phase_targets: &[f32],
    ) -> f32 {
        if phase_targets.is_empty() {
            return 0.0;
        }

        let phase_duration = phase_duration_ms.max(1);
        let duration_ms = phase_duration.saturating_mul(phase_targets.len() as u32);
        if elapsed_ms >= duration_ms {
            return 0.0;
        }

        let phase_index = (elapsed_ms / phase_duration) as usize;
        let phase_progress = (elapsed_ms % phase_duration) as f32 / phase_duration as f32;
        let phase_start = if phase_index == 0 { 0.0 } else { phase_targets[phase_index - 1] };
        let phase_end = phase_targets[phase_index];
        lerp(phase_start, phase_end, phase_progress)
    }

    /// Shared required-field recovery shake profile used by app crates that
    /// need the legacy 35 ms / 12-phase shake curve.
    #[inline]
    pub fn required_field_shake_offset(elapsed_ms: u32, scale: f32) -> f32 {
        sample_keyframed_offset(
            elapsed_ms,
            REQUIRED_FIELD_SHAKE_PHASE_DURATION_MS,
            &REQUIRED_FIELD_SHAKE_PHASE_TARGETS,
        ) * scale.max(0.0)
    }

    #[inline]
    fn deterministic_duration_ms(base_ms: u32, variance_ms: u32, seed: u32, salt: u32) -> u32 {
        let mixed = stable_hash(seed ^ salt);
        let unit = (mixed as f32 / u32::MAX as f32).clamp(0.0, 1.0);
        let offset = ((unit * 2.0 - 1.0) * variance_ms as f32).round() as i32;
        base_ms.saturating_add_signed(offset).max(1)
    }

    #[inline]
    fn stable_hash(mut value: u32) -> u32 {
        value ^= value >> 16;
        value = value.wrapping_mul(0x7feb_352d);
        value ^= value >> 15;
        value = value.wrapping_mul(0x846c_a68b);
        value ^ (value >> 16)
    }

    #[inline]
    fn sample_autoreversing(elapsed_ms: u32, duration_ms: u32, start: f32, end: f32) -> f32 {
        let duration = duration_ms.max(1);
        let period = duration.saturating_mul(2).max(1);
        let local = elapsed_ms % period;
        let progress = if local <= duration {
            local as f32 / duration as f32
        } else {
            1.0 - (local - duration) as f32 / duration as f32
        };
        lerp(start, end, progress)
    }

    #[inline]
    fn apply_axis(base: api::Transform2D, axis: Axis2D, delta: f32) -> api::Transform2D {
        let mut out = base;
        match axis {
            Axis2D::Horizontal => out.tx = base.tx + delta,
            Axis2D::Vertical => out.ty = base.ty + delta,
            Axis2D::Both => {
                out.tx = base.tx + delta;
                out.ty = base.ty + delta * 0.6;
            }
        }
        out
    }

    #[inline]
    fn build_transform(
        from: api::Transform2D,
        to: api::Transform2D,
        delay_ms: u32,
        duration_ms: u32,
        ease: api::EaseKind,
    ) -> api::AnimDesc {
        api::AnimDesc {
            id: 0,
            prop: api::AnimProp::Transform2D,
            from: api::AnimValue::Xform2D(from),
            to: api::AnimValue::Xform2D(to),
            curve: api::AnimCurve::Ease { ease: api::Ease { kind: ease } },
            duration_ms,
            delay_ms,
            repeat: api::Repeat::Once,
        }
    }

    #[inline]
    fn lerp(a: f32, b: f32, t: f32) -> f32 {
        a + (b - a) * t
    }

    #[inline]
    fn sample_cubic_curve(parameter: f32, a1: f32, a2: f32) -> f32 {
        let one_minus_t = 1.0 - parameter;
        3.0 * one_minus_t * one_minus_t * parameter * a1
            + 3.0 * one_minus_t * parameter * parameter * a2
            + parameter * parameter * parameter
    }

    #[inline]
    fn sample_cubic_derivative(parameter: f32, a1: f32, a2: f32) -> f32 {
        let one_minus_t = 1.0 - parameter;
        3.0 * one_minus_t * one_minus_t * a1
            + 6.0 * one_minus_t * parameter * (a2 - a1)
            + 3.0 * parameter * parameter * (1.0 - a2)
    }

    #[inline]
    fn ease_out_back(t: f32) -> f32 {
        let c1 = 1.70158;
        let c3 = c1 + 1.0;
        let u = t - 1.0;
        1.0 + c3 * u * u * u + c1 * u * u
    }

    #[inline]
    fn ease_out_cubic(t: f32) -> f32 {
        let u = t - 1.0;
        u * u * u + 1.0
    }
}

#[derive(Clone, Debug, Default)]
pub struct AnimOverrides {
    pub opacity: Option<f32>,
    pub transform: Option<api::Transform2D>,
    pub color: Option<gfx::Color>,
    pub corner_radii: Option<[f32; 4]>,
    pub shadow_alpha: Option<f32>,
}

#[derive(Clone, Debug, Default)]
struct AnimOverrideEntry
{
   current: AnimOverrides,
   previous: AnimOverrides,
   present: bool,
   was_present: bool,
}

/// Dense node-indexed animation overrides with storage retained across frames.
#[derive(Clone, Debug, Default)]
pub struct AnimOverrideSlots
{
   entries: Vec<AnimOverrideEntry>,
   active: Vec<NodeId>,
   previous: Vec<NodeId>,
   changed: Vec<NodeId>,
   paint_changed: Vec<NodeId>,
}

impl AnimOverrideSlots
{
   fn begin_frame(&mut self)
   {
      core::mem::swap(&mut self.active, &mut self.previous);
      self.active.clear();
      self.changed.clear();
      self.paint_changed.clear();
      for node in self.previous.iter().copied()
      {
         let entry = &mut self.entries[node.0 as usize];
         entry.previous = core::mem::take(&mut entry.current);
         entry.present = false;
         entry.was_present = true;
      }
   }

   fn entry_mut(&mut self, node: NodeId) -> &mut AnimOverrides
   {
      let index = node.0 as usize;
      if self.entries.len() <= index
      {
         self.entries.resize_with(index + 1, AnimOverrideEntry::default);
      }
      if !self.entries[index].present
      {
         self.entries[index].present = true;
         self.active.push(node);
      }
      &mut self.entries[index].current
   }

   fn finish_frame(&mut self)
   {
      for node in self.previous.iter().copied()
      {
         let entry = &mut self.entries[node.0 as usize];
         if !entry.present || entry.current != entry.previous
         {
            self.changed.push(node);
            if !paint_overrides_equal(&entry.current, &entry.previous)
            {
               self.paint_changed.push(node);
            }
         }
         if !entry.present
         {
            entry.previous = AnimOverrides::default();
            entry.was_present = false;
         }
      }
      for node in self.active.iter().copied()
      {
         let entry = &mut self.entries[node.0 as usize];
         if !entry.was_present
         {
            self.changed.push(node);
            if !paint_overrides_equal(&entry.current, &AnimOverrides::default())
            {
               self.paint_changed.push(node);
            }
         }
         entry.previous = AnimOverrides::default();
         entry.was_present = false;
      }
      self.previous.clear();
   }

   #[inline]
   #[must_use]
   pub fn get(&self, node: &NodeId) -> Option<&AnimOverrides>
   {
      self.entries.get(node.0 as usize).filter(|entry| entry.present).map(|entry| &entry.current)
   }

   #[inline]
   #[must_use]
   pub fn len(&self) -> usize
   {
      self.active.len()
   }

   #[inline]
   #[must_use]
   pub fn is_empty(&self) -> bool
   {
      self.active.is_empty()
   }

   #[inline]
   #[must_use]
   pub fn changed_nodes(&self) -> &[NodeId]
   {
      &self.changed
   }

   #[inline]
   #[must_use]
   pub fn storage_capacity(&self) -> usize
   {
      self.entries.capacity()
   }

   #[inline]
   #[must_use]
   pub fn paint_changed_nodes(&self) -> &[NodeId]
   {
      &self.paint_changed
   }

   pub fn insert(&mut self, node: NodeId, overrides: AnimOverrides) -> Option<AnimOverrides>
   {
      let previous = self.get(&node).cloned();
      *self.entry_mut(node) = overrides;
      if previous.as_ref() != self.get(&node)
      {
         self.changed.push(node);
      }
      previous
   }

   pub fn clear(&mut self)
   {
      self.begin_frame();
      self.finish_frame();
   }

   pub fn iter(&self) -> impl Iterator<Item = (NodeId, &AnimOverrides)>
   {
      self.active.iter().copied().filter_map(|node| self.get(&node).map(|overrides| (node, overrides)))
   }
}

#[inline]
fn paint_overrides_equal(a: &AnimOverrides, b: &AnimOverrides) -> bool
{
   a.color == b.color
      && a.corner_radii == b.corner_radii
      && a.shadow_alpha == b.shadow_alpha
}

impl PartialEq for AnimOverrides {
    fn eq(&self, other: &Self) -> bool {
        self.opacity == other.opacity
            && transforms_equal(self.transform.as_ref(), other.transform.as_ref())
            && self.color == other.color
            && self.corner_radii == other.corner_radii
            && self.shadow_alpha == other.shadow_alpha
    }
}

fn transforms_equal(a: Option<&api::Transform2D>, b: Option<&api::Transform2D>) -> bool {
    match (a, b) {
        (Some(lhs), Some(rhs)) => {
            lhs.tx == rhs.tx
                && lhs.ty == rhs.ty
                && lhs.sx == rhs.sx
                && lhs.sy == rhs.sy
                && lhs.rot_rad == rhs.rot_rad
        }
        (None, None) => true,
        _ => false,
    }
}

#[derive(Clone, Debug)]
struct Active {
    node: NodeId,
    desc: api::AnimDesc,
    start_ms: u64,
}

/// Animator manages many node property animations and produces per-frame overrides.
pub struct Animator {
    reduce_motion: bool,
    next_id: api::AnimId,
    active: alloc::vec::Vec<Active>,
    overrides: AnimOverrideSlots,
}

impl Default for Animator {
    fn default() -> Self {
        Self {
            reduce_motion: false,
            next_id: 1,
            active: alloc::vec::Vec::new(),
            overrides: AnimOverrideSlots::default(),
        }
    }
}

impl Animator {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set_reduce_motion(&mut self, on: bool) {
        self.reduce_motion = on;
        timing::set_reduce_motion(on);
    }

    fn alloc_id(&mut self) -> api::AnimId {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        id
    }

    pub fn start(&mut self, node: NodeId, mut desc: api::AnimDesc) -> api::AnimId {
        // Honor reduce motion by collapsing duration/delay unless essential
        if self.reduce_motion {
            desc.duration_ms = 0;
            desc.delay_ms = 0;
        }
        let id = self.alloc_id();
        desc.id = id;
        let st = Active { node, desc: desc.clone(), start_ms: timing::now_ms() };
        self.active.push(st);
        id
    }

    pub fn cancel_prop(&mut self, node: NodeId, prop: api::AnimProp) {
        self.active.retain(|a| !(a.node == node && a.desc.prop == prop));
    }

    pub fn is_active(&self, node: NodeId) -> bool {
        self.active.iter().any(|a| a.node == node)
    }

    pub fn is_active_prop(&self, node: NodeId, prop: api::AnimProp) -> bool {
        self.active.iter().any(|a| a.node == node && a.desc.prop == prop)
    }

    pub fn start_sequence(&mut self, node: NodeId, seq: &[api::AnimDesc]) {
        for desc in seq.iter().cloned() {
            self.start(node, desc);
        }
    }

    #[inline]
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    #[inline]
    #[must_use]
    pub fn active_storage_capacity(&self) -> usize {
        self.active.capacity()
    }

    pub fn step(&mut self, now_ms: u64) -> &AnimOverrideSlots {
        self.overrides.begin_frame();
        let overrides = &mut self.overrides;
        self.active.retain_mut(|a| {
            let d = &a.desc;
            let t_ms = now_ms.saturating_sub(a.start_ms);
            if t_ms < d.delay_ms as u64 {
                return true;
            }
            let elapsed = (t_ms - d.delay_ms as u64) as u32;
            let val = timing::anim::value_at(d, elapsed);
            let e = overrides.entry_mut(a.node);
            match (d.prop, val) {
                (api::AnimProp::Opacity, api::AnimValue::F32(v)) => e.opacity = Some(v),
                (api::AnimProp::Transform2D, api::AnimValue::Xform2D(xf)) => e.transform = Some(xf),
                (api::AnimProp::ColorRGBA, api::AnimValue::Vec4(c)) => {
                    e.color = Some(gfx::Color::rgba(c[0], c[1], c[2], c[3]))
                }
                (api::AnimProp::CornerRadius, api::AnimValue::Vec4(r)) => {
                    e.corner_radii = Some([r[0], r[1], r[2], r[3]])
                }
                (api::AnimProp::ShadowAlpha, api::AnimValue::F32(v)) => e.shadow_alpha = Some(v),
                _ => {}
            }
            let done = elapsed >= d.duration_ms;
            if !done {
                return true;
            }
            match d.repeat {
                    api::Repeat::Once => false,
                    api::Repeat::Count(n) => {
                        if n <= 1 {
                            return false;
                        }
                        a.desc.repeat = api::Repeat::Count(n - 1);
                        a.start_ms = now_ms;
                        true
                    }
                    api::Repeat::Forever => {
                        a.start_ms = now_ms;
                        true
                    }
                }
        });
        self.overrides.finish_frame();
        &self.overrides
    }

    #[inline]
    #[must_use]
    pub fn overrides(&self) -> &AnimOverrideSlots {
        &self.overrides
    }

    #[inline]
    pub fn overrides_mut(&mut self) -> &mut AnimOverrideSlots {
        &mut self.overrides
    }
}
