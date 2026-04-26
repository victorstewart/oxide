use oxide_platform_api as api;
use oxide_timing::testing;
use oxide_timing::{self, advance_timers, anim, now_ms, now_ns, schedule_after};
use std::sync::{Arc, LazyLock, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[test]
fn monotonic_clock() {
    let _guard = TEST_LOCK.lock().unwrap();
    testing::reset();
    let a_ms = now_ms();
    let b_ms = now_ms();
    assert!(b_ms >= a_ms);
    let a_ns = now_ns();
    let b_ns = now_ns();
    assert!(b_ns >= a_ns);
}

#[test]
fn timers_fire_in_order() {
    let _guard = TEST_LOCK.lock().unwrap();
    testing::reset();
    let hits: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));
    let rec = hits.clone();
    let start = now_ms();
    schedule_after(50, move || {
        rec.lock().unwrap().push(1);
    });
    let rec2 = hits.clone();
    schedule_after(80, move || {
        rec2.lock().unwrap().push(2);
    });

    advance_timers(start + 40);
    assert!(hits.lock().unwrap().is_empty());

    advance_timers(start + 60);
    assert_eq!(hits.lock().unwrap().as_slice(), &[1]);

    advance_timers(start + 200);
    assert_eq!(hits.lock().unwrap().as_slice(), &[1, 2]);
    assert_eq!(testing::pending_timers(), 0);
}

#[test]
fn timer_callback_fires_after_delay() {
    let _guard = TEST_LOCK.lock().unwrap();
    testing::reset();
    let hit = Arc::new(AtomicUsize::new(0));
    let hit2 = hit.clone();
    let start = now_ms();
    schedule_after(10, move || {
        hit2.fetch_add(1, Ordering::SeqCst);
    });
    advance_timers(start + 9);
    assert_eq!(hit.load(Ordering::SeqCst), 0);
    advance_timers(start + 11);
    assert_eq!(hit.load(Ordering::SeqCst), 1);
}

#[test]
fn timer_callbacks_can_schedule_follow_up_timers() {
    let _guard = TEST_LOCK.lock().unwrap();
    testing::reset();
    let hits: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));
    let rec = hits.clone();
    let start = now_ms();
    schedule_after(10, move || {
        rec.lock().unwrap().push(1);
        let rec2 = rec.clone();
        schedule_after(10, move || {
            rec2.lock().unwrap().push(2);
        });
    });

    advance_timers(start + 20);
    assert_eq!(hits.lock().unwrap().as_slice(), &[1]);
    assert_eq!(testing::pending_timers(), 1);

    advance_timers(start + 200);
    assert_eq!(hits.lock().unwrap().as_slice(), &[1, 2]);
    assert_eq!(testing::pending_timers(), 0);
}

#[test]
fn timer_ids_are_monotonic() {
    let _guard = TEST_LOCK.lock().unwrap();
    testing::reset();
    let id1 = schedule_after(10, || {});
    let id2 = schedule_after(10, || {});
    assert!(id2 > id1);
    advance_timers(now_ms() + 100);
    assert_eq!(testing::pending_timers(), 0);
}

fn sample_anim(id: api::AnimId) -> api::AnimDesc {
    api::AnimDesc {
        id,
        prop: api::AnimProp::Opacity,
        from: api::AnimValue::F32(0.0),
        to: api::AnimValue::F32(1.0),
        curve: api::AnimCurve::Ease { ease: api::Ease { kind: api::EaseKind::Linear } },
        duration_ms: 120,
        delay_ms: 30,
        repeat: api::Repeat::Once,
    }
}

fn sample_anim_with_ease(kind: api::EaseKind) -> api::AnimDesc {
    api::AnimDesc {
        id: 10,
        prop: api::AnimProp::Opacity,
        from: api::AnimValue::F32(0.0),
        to: api::AnimValue::F32(1.0),
        curve: api::AnimCurve::Ease { ease: api::Ease { kind } },
        duration_ms: 20,
        delay_ms: 0,
        repeat: api::Repeat::Once,
    }
}

fn anim_value_as_f32(value: api::AnimValue) -> f32 {
    match value {
        api::AnimValue::F32(v) => v,
        _ => panic!("expected f32 animation value"),
    }
}

#[test]
fn ease_curves_behave() {
    let monotonic = [
        api::EaseKind::Linear,
        api::EaseKind::QuadIn,
        api::EaseKind::QuadOut,
        api::EaseKind::QuadInOut,
        api::EaseKind::CubicIn,
        api::EaseKind::CubicOut,
        api::EaseKind::CubicInOut,
    ];
    for kind in monotonic {
        let desc = sample_anim_with_ease(kind);
        let mut last = -1.0f32;
        for elapsed in 0..=20 {
            let value = anim_value_as_f32(anim::value_at(&desc, elapsed));
            assert!(value.is_finite());
            assert!((0.0 - 1e-4..=1.0 + 1e-4).contains(&value));
            assert!(value + 1e-4 >= last);
            last = value;
        }
    }

    for kind in [api::EaseKind::BackInOut, api::EaseKind::ElasticOut, api::EaseKind::BounceOut] {
        let desc = sample_anim_with_ease(kind);
        for elapsed in 0..=20 {
            let value = anim_value_as_f32(anim::value_at(&desc, elapsed));
            assert!(value.is_finite());
            assert!((-0.5..=1.5).contains(&value));
        }
    }
}

#[test]
fn ease_endpoints() {
    let desc = sample_anim_with_ease(api::EaseKind::CubicOut);
    assert!((anim_value_as_f32(anim::value_at(&desc, 0)) - 0.0).abs() < 1e-6);
    assert!((anim_value_as_f32(anim::value_at(&desc, 20)) - 1.0).abs() < 1e-6);
}

#[test]
fn starting_animation_replaces_previous_prop() {
    let _guard = TEST_LOCK.lock().unwrap();
    testing::reset();
    let d1 = sample_anim(1);
    let id1 = anim::start(&d1);
    assert_eq!(testing::active_anims(), 1);
    assert!(testing::anim_desc(id1).is_some());

    let id2 = anim::start(&sample_anim(2));
    assert!(testing::anim_desc(id1).is_none());
    assert_eq!(testing::active_anims(), 1);
    anim::cancel(id2);
    assert_eq!(testing::active_anims(), 0);
}

#[test]
fn reduce_motion_zeroes_duration() {
    let _guard = TEST_LOCK.lock().unwrap();
    testing::reset();
    oxide_timing::set_reduce_motion(true);
    let id = anim::start(&sample_anim(3));
    let stored = testing::anim_desc(id).expect("anim stored");
    assert_eq!(stored.duration_ms, 0);
    assert_eq!(stored.delay_ms, 0);
    oxide_timing::set_reduce_motion(false);
    anim::cancel(id);
    assert_eq!(testing::active_anims(), 0);
}
