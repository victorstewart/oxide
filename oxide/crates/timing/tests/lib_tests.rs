use oxide_platform_api as api;
use oxide_timing::testing;
use oxide_timing::{self, advance_timers, anim, now_ms, now_ns, schedule_after};
use std::sync::{Arc, LazyLock, Mutex};

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
