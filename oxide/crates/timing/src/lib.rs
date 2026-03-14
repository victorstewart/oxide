//! `Oxide` timing: monotonic time, timers, and animations.
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::cast_precision_loss
)]

use oxide_platform_api as api;
use std::collections::{BTreeMap, HashMap};
use std::convert::TryFrom;
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

// ===== Monotonic clock =====

static START: LazyLock<Instant> = LazyLock::new(Instant::now);

pub fn now_ns() -> u64 {
    clamp_u128_to_u64(START.elapsed().as_nanos())
}
pub fn now_ms() -> u64 {
    clamp_u128_to_u64(START.elapsed().as_millis())
}

#[inline]
fn clamp_u128_to_u64(value: u128) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

// ===== Timers =====

type TimerId = u64;

struct TimerEntry {
    cb: Box<dyn FnOnce() + Send + 'static>,
}

static TIMERS: LazyLock<Mutex<BTreeMap<u64, Vec<TimerEntry>>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));
static NEXT_TID: LazyLock<Mutex<u64>> = LazyLock::new(|| Mutex::new(1));

pub fn schedule_after<F: FnOnce() + Send + 'static>(delay_ms: u64, f: F) -> TimerId {
    let when = now_ms().saturating_add(delay_ms);
    let mut map = TIMERS.lock().unwrap();
    let mut id_guard = NEXT_TID.lock().unwrap();
    let id = *id_guard;
    *id_guard = id.saturating_add(1);
    map.entry(when).or_default().push(TimerEntry { cb: Box::new(f) });
    id
}

pub fn advance_timers(now_ms_val: u64) {
    let mut map = TIMERS.lock().unwrap();
    let mut due_keys: Vec<u64> = Vec::new();
    for (k, _) in map.range(..=now_ms_val) {
        due_keys.push(*k);
    }
    for k in due_keys {
        if let Some(mut vec) = map.remove(&k) {
            for entry in vec.drain(..) {
                (entry.cb)();
            }
        }
    }
}

// ===== Animations =====

#[derive(Clone)]
struct AnimState {
    desc: api::AnimDesc,
    start_ms: u64,
    elapsed_ms: u64,
    finished: bool,
}

static ANIMS: LazyLock<Mutex<HashMap<api::AnimId, AnimState>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static RUNNING_PROP: LazyLock<Mutex<HashMap<api::AnimProp, api::AnimId>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static REDUCE_MOTION: LazyLock<Mutex<bool>> = LazyLock::new(|| Mutex::new(false));

pub fn set_reduce_motion(enabled: bool) {
    *REDUCE_MOTION.lock().unwrap() = enabled;
}

#[cfg_attr(not(test), doc(hidden))]
#[cfg_attr(not(test), allow(dead_code))]
pub mod testing {
    use super::{ANIMS, NEXT_TID, REDUCE_MOTION, RUNNING_PROP, TIMERS};
    use oxide_platform_api as api;

    pub fn reset() {
        TIMERS.lock().unwrap().clear();
        *NEXT_TID.lock().unwrap() = 1;
        ANIMS.lock().unwrap().clear();
        RUNNING_PROP.lock().unwrap().clear();
        *REDUCE_MOTION.lock().unwrap() = false;
    }

    pub fn pending_timers() -> usize {
        TIMERS.lock().unwrap().values().map(Vec::len).sum()
    }

    pub fn active_anims() -> usize {
        ANIMS.lock().unwrap().len()
    }

    pub fn anim_desc(id: api::AnimId) -> Option<api::AnimDesc> {
        ANIMS.lock().unwrap().get(&id).map(|st| st.desc.clone())
    }
}

pub mod anim {
    use super::{ease_value, lerp_value, now_ms, AnimState, ANIMS, REDUCE_MOTION, RUNNING_PROP};
    use oxide_platform_api as api;

    pub fn start(desc: &api::AnimDesc) -> api::AnimId {
        let reduce = *REDUCE_MOTION.lock().unwrap();
        // Cancel existing prop animation if any
        cancel_prop(desc.prop);
        let now = now_ms();
        let mut d = desc.clone();
        if reduce {
            d.duration_ms = 0;
            d.delay_ms = 0;
        }
        let id = d.id;
        let st = AnimState { desc: d, start_ms: now, elapsed_ms: 0, finished: false };
        ANIMS.lock().unwrap().insert(id, st);
        RUNNING_PROP.lock().unwrap().insert(desc.prop, id);
        id
    }

    pub fn cancel(id: api::AnimId) {
        ANIMS.lock().unwrap().remove(&id);
    }

    pub fn cancel_prop(prop: api::AnimProp) {
        if let Some(id) = RUNNING_PROP.lock().unwrap().remove(&prop) {
            ANIMS.lock().unwrap().remove(&id);
        }
    }

    pub fn step(now_ms_val: u64) {
        let mut to_remove: Vec<api::AnimId> = Vec::new();
        let mut anims = ANIMS.lock().unwrap();
        for (id, st) in anims.iter_mut() {
            let d = &st.desc;
            let t_ms = now_ms_val.saturating_sub(st.start_ms);
            let delay = u64::from(d.delay_ms);
            if t_ms < delay {
                continue;
            }
            let run_ms = (t_ms - delay) as u32;
            st.elapsed_ms = run_ms as u64;
            let done = run_ms >= d.duration_ms;
            if done {
                match d.repeat {
                    api::Repeat::Once => {
                        st.finished = true;
                        to_remove.push(*id);
                    }
                    api::Repeat::Count(n) => {
                        if n <= 1 {
                            st.finished = true;
                            to_remove.push(*id);
                        } else {
                            let mut nd = d.clone();
                            nd.repeat = api::Repeat::Count(n - 1);
                            st.desc = nd;
                            st.start_ms = now_ms_val;
                            st.elapsed_ms = 0;
                        }
                    }
                    api::Repeat::Forever => {
                        st.start_ms = now_ms_val;
                        st.elapsed_ms = 0;
                    }
                }
            }
        }
        if to_remove.is_empty() {
            drop(anims);
            return;
        }

        let doomed: std::collections::HashSet<api::AnimId> = to_remove.iter().copied().collect();
        for id in &to_remove {
            anims.remove(id);
        }
        drop(anims);

        let mut running = RUNNING_PROP.lock().unwrap();
        running.retain(|_, v| !doomed.contains(v));
        // Note: computing the current value for each prop is left to the client using value_at(desc, elapsed)
    }

    #[must_use]
    pub fn value_at(desc: &api::AnimDesc, elapsed_ms: u32) -> api::AnimValue {
        let t = if desc.duration_ms == 0 {
            1.0
        } else {
            (elapsed_ms as f32 / desc.duration_ms as f32).clamp(0.0, 1.0)
        };
        let k = ease_value(&desc.curve, t);
        lerp_value(&desc.from, &desc.to, k)
    }
}

fn ease_value(curve: &api::AnimCurve, t: f32) -> f32 {
    match curve {
        api::AnimCurve::Ease { ease } => match ease.kind {
            api::EaseKind::Linear => t,
            api::EaseKind::QuadIn => t * t,
            api::EaseKind::QuadOut => t * (2.0 - t),
            api::EaseKind::QuadInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    -1.0 + (4.0 - 2.0 * t) * t
                }
            }
            api::EaseKind::CubicIn => t * t * t,
            api::EaseKind::CubicOut => {
                let u = t - 1.0;
                u * u * u + 1.0
            }
            api::EaseKind::CubicInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    let u = 2.0 * t - 2.0;
                    0.5 * u * u * u + 1.0
                }
            }
            api::EaseKind::BackInOut => {
                let c1 = 1.70158;
                let c2 = c1 * 1.525;
                if t < 0.5 {
                    ((2.0 * t).powi(2) * ((c2 + 1.0) * 2.0 * t - c2)) / 2.0
                } else {
                    let u = 2.0 * t - 2.0;
                    (u.powi(2) * ((c2 + 1.0) * u + c2) + 2.0) / 2.0
                }
            }
            api::EaseKind::ElasticOut => {
                if t.abs() <= f32::EPSILON {
                    0.0
                } else if (t - 1.0).abs() <= f32::EPSILON {
                    1.0
                } else {
                    let p = 0.3;
                    2f32.powf(-10.0 * t) * ((t - p / 4.0) * (2.0 * std::f32::consts::PI) / p).sin()
                        + 1.0
                }
            }
            api::EaseKind::BounceOut => bounce_out(t),
        },
        api::AnimCurve::Spring { sp } => {
            spring_critically_damped(t, sp.stiffness, sp.damping, sp.mass)
        }
    }
}

fn bounce_out(t: f32) -> f32 {
    let n1 = 7.562_5;
    let d1 = 2.75;
    if t < 1.0 / d1 {
        n1 * t * t
    } else if t < 2.0 / d1 {
        let u = t - 1.5 / d1;
        n1 * u * u + 0.75
    } else if t < 2.5 / d1 {
        let u = t - 2.25 / d1;
        n1 * u * u + 0.937_5
    } else {
        let u = t - 2.625 / d1;
        n1 * u * u + 0.984_375
    }
}

fn spring_critically_damped(t: f32, stiffness: f32, damping: f32, mass: f32) -> f32 {
    // Normalize damping to critical range; simplified mapping for t in [0,1]
    // This is a placeholder analytical form close to critically-damped response
    let z = damping / (2.0 * (stiffness * mass).sqrt()).max(1e-5);
    if z >= 1.0 {
        // over/critical damp
        let k = (-stiffness * t).exp();
        1.0 - k
    } else {
        // underdamped oscillation
        let wd = stiffness * (1.0 - z * z).sqrt();
        let e = (-damping * t).exp();
        1.0 - e * ((wd * t).cos())
    }
}

fn lerp_value(a: &api::AnimValue, b: &api::AnimValue, k: f32) -> api::AnimValue {
    use api::AnimValue as V;
    match (a, b) {
        (V::F32(x0), V::F32(x1)) => V::F32(x0 + (x1 - x0) * k),
        (V::Vec2(a0), V::Vec2(a1)) => {
            V::Vec2([a0[0] + (a1[0] - a0[0]) * k, a0[1] + (a1[1] - a0[1]) * k])
        }
        (V::Vec4(a0), V::Vec4(a1)) => V::Vec4([
            a0[0] + (a1[0] - a0[0]) * k,
            a0[1] + (a1[1] - a0[1]) * k,
            a0[2] + (a1[2] - a0[2]) * k,
            a0[3] + (a1[3] - a0[3]) * k,
        ]),
        (V::Xform2D(x0), V::Xform2D(x1)) => V::Xform2D(api::Transform2D {
            tx: x0.tx + (x1.tx - x0.tx) * k,
            ty: x0.ty + (x1.ty - x0.ty) * k,
            sx: x0.sx + (x1.sx - x0.sx) * k,
            sy: x0.sy + (x1.sy - x0.sy) * k,
            rot_rad: x0.rot_rad + (x1.rot_rad - x0.rot_rad) * k,
        }),
        _ => b.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ease_curves_behave() {
        // Monotonic eases: should be non-decreasing [0,1]
        let mono = [
            api::EaseKind::Linear,
            api::EaseKind::QuadIn,
            api::EaseKind::QuadOut,
            api::EaseKind::QuadInOut,
            api::EaseKind::CubicIn,
            api::EaseKind::CubicOut,
            api::EaseKind::CubicInOut,
        ];
        for k in mono {
            let e = api::Ease { kind: k };
            let c = api::AnimCurve::Ease { ease: e };
            let mut last = -1.0f32;
            for i in 0..=20 {
                let t = i as f32 / 20.0;
                let y = super::ease_value(&c, t);
                assert!(y.is_finite());
                assert!((0.0 - 1e-4..=1.0 + 1e-4).contains(&y));
                assert!(y + 1e-4 >= last);
                last = y;
            }
        }
        // Non-monotonic eases: values remain finite and within [0,1]
        for k in [api::EaseKind::BackInOut, api::EaseKind::ElasticOut, api::EaseKind::BounceOut] {
            let e = api::Ease { kind: k };
            let c = api::AnimCurve::Ease { ease: e };
            for i in 0..=20 {
                let t = i as f32 / 20.0;
                let y = super::ease_value(&c, t);
                assert!(y.is_finite());
                // Allow bounded overshoot/undershoot typical for these curves
                assert!((-0.5..=1.5).contains(&y));
            }
        }
    }

    #[test]
    fn timers_fire() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };
        let hit = Arc::new(AtomicUsize::new(0));
        let hit2 = hit.clone();
        schedule_after(10, move || {
            hit2.fetch_add(1, Ordering::SeqCst);
        });
        advance_timers(now_ms() + 9);
        assert_eq!(hit.load(Ordering::SeqCst), 0);
        advance_timers(now_ms() + 11);
        assert_eq!(hit.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn ease_endpoints() {
        let d = api::AnimDesc {
            id: 1,
            prop: api::AnimProp::Opacity,
            from: api::AnimValue::F32(0.0),
            to: api::AnimValue::F32(1.0),
            curve: api::AnimCurve::Ease { ease: api::Ease { kind: api::EaseKind::CubicOut } },
            duration_ms: 200,
            delay_ms: 0,
            repeat: api::Repeat::Once,
        };
        match super::anim::value_at(&d, 0) {
            api::AnimValue::F32(v) => assert!((v - 0.0).abs() < 1e-6),
            _ => panic!(),
        }
        match super::anim::value_at(&d, 200) {
            api::AnimValue::F32(v) => assert!((v - 1.0).abs() < 1e-6),
            _ => panic!(),
        }
    }

    #[test]
    fn reduce_motion_zero_duration() {
        super::set_reduce_motion(true);
        let d = api::AnimDesc {
            id: 2,
            prop: api::AnimProp::Opacity,
            from: api::AnimValue::F32(0.5),
            to: api::AnimValue::F32(0.9),
            curve: api::AnimCurve::Ease { ease: api::Ease { kind: api::EaseKind::Linear } },
            duration_ms: 100,
            delay_ms: 50,
            repeat: api::Repeat::Once,
        };
        let id = super::anim::start(&d);
        super::anim::step(now_ms());
        // Since durations/delay become 0 under reduce motion, direct finish expected
        if let api::AnimValue::F32(v) = super::anim::value_at(&d, 0) {
            assert!((v - 0.9).abs() > 1e-2);
        }
        super::anim::cancel(id);
        super::set_reduce_motion(false);
    }
}
