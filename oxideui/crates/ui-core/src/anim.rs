//! Animation system integration for ui-core nodes.
//! Maps per-node animated properties to draw-time overrides, using timing curves.

use crate::prelude::platform_api as api;
use crate::prelude::renderer_api as gfx;
use crate::NodeId;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use oxideui_timing as timing;

pub mod helpers {
    use super::*;

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
}

impl Default for Animator {
    fn default() -> Self {
        Self { reduce_motion: false, next_id: 1, active: alloc::vec::Vec::new() }
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

    pub fn step(&mut self, now_ms: u64) -> BTreeMap<NodeId, AnimOverrides> {
        let mut out: BTreeMap<NodeId, AnimOverrides> = BTreeMap::new();
        // Walk a copy to allow removal
        let mut i = 0;
        while i < self.active.len() {
            let a = &self.active[i];
            let d = &a.desc;
            let t_ms = now_ms.saturating_sub(a.start_ms);
            if t_ms < d.delay_ms as u64 {
                i += 1;
                continue;
            }
            let elapsed = (t_ms - d.delay_ms as u64) as u32;
            let val = timing::anim::value_at(d, elapsed);
            let e = out.entry(a.node).or_default();
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
            // Completion handling
            let done = elapsed >= d.duration_ms;
            if done {
                match d.repeat {
                    api::Repeat::Once => {
                        self.active.remove(i);
                        continue;
                    }
                    api::Repeat::Count(n) => {
                        if n <= 1 {
                            self.active.remove(i);
                            continue;
                        }
                        let mut nd = d.clone();
                        nd.repeat = api::Repeat::Count(n - 1);
                        self.active[i].desc = nd;
                        self.active[i].start_ms = now_ms;
                        i += 1;
                    }
                    api::Repeat::Forever => {
                        self.active[i].start_ms = now_ms;
                        i += 1;
                    }
                }
            } else {
                i += 1;
            }
        }
        out
    }
}
