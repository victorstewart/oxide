use oxide_platform_api as api;
use oxide_ui_core::anim::Animator;
use oxide_ui_core::NodeId;

#[test]
fn animator_emits_bounded_overrides() {
    let mut anim = Animator::new();
    // Opacity 0 -> 1 over 100ms
    let desc = api::AnimDesc {
        id: 42,
        prop: api::AnimProp::Opacity,
        from: api::AnimValue::F32(0.0),
        to: api::AnimValue::F32(1.0),
        curve: api::AnimCurve::Ease { ease: api::Ease { kind: api::EaseKind::CubicOut } },
        duration_ms: 100,
        delay_ms: 0,
        repeat: api::Repeat::Once,
    };
    let node = NodeId(7);
    let _id = anim.start(node, desc);

    let t0 = oxide_timing::now_ms();
    // Sample at 0, 50, 100ms
    let k0 = anim.step(t0);
    let v0 = k0.get(&node).and_then(|o| o.opacity).unwrap_or(0.0);
    assert!((0.0..=1.0).contains(&v0));

    let k1 = anim.step(t0 + 50);
    let v1 = k1.get(&node).and_then(|o| o.opacity).unwrap_or(v0);
    assert!(v1 >= v0 - 1e-3 && v1 <= 1.0);

    let k2 = anim.step(t0 + 100);
    let v2 = k2.get(&node).and_then(|o| o.opacity).unwrap_or(v1);
    assert!((v2 - 1.0).abs() <= 1e-3);
}

#[test]
fn animator_repeat_once_finishes() {
    let mut anim = Animator::new();
    let node = NodeId(10);
    let d = api::AnimDesc {
        id: 1,
        prop: api::AnimProp::Opacity,
        from: api::AnimValue::F32(0.0),
        to: api::AnimValue::F32(1.0),
        curve: api::AnimCurve::Ease { ease: api::Ease { kind: api::EaseKind::Linear } },
        duration_ms: 100,
        delay_ms: 0,
        repeat: api::Repeat::Once,
    };
    anim.start(node, d);
    let t0 = oxide_timing::now_ms();
    let _ = anim.step(t0 + 150);
    assert_eq!(anim.active_count(), 0);
}

#[test]
fn animator_repeat_count_decrements_and_stops() {
    let mut anim = Animator::new();
    let node = NodeId(11);
    let d = api::AnimDesc {
        id: 2,
        prop: api::AnimProp::Opacity,
        from: api::AnimValue::F32(0.0),
        to: api::AnimValue::F32(1.0),
        curve: api::AnimCurve::Ease { ease: api::Ease { kind: api::EaseKind::Linear } },
        duration_ms: 100,
        delay_ms: 0,
        repeat: api::Repeat::Count(2),
    };
    anim.start(node, d);
    let t0 = oxide_timing::now_ms();
    // First cycle completes and resets
    let _ = anim.step(t0 + 150);
    assert_eq!(anim.active_count(), 1);
    // Second cycle completes and removes
    let _ = anim.step(t0 + 300);
    assert_eq!(anim.active_count(), 0);
}

#[test]
fn animator_repeat_forever_loops() {
    let mut anim = Animator::new();
    let node = NodeId(12);
    let d = api::AnimDesc {
        id: 3,
        prop: api::AnimProp::Opacity,
        from: api::AnimValue::F32(0.0),
        to: api::AnimValue::F32(1.0),
        curve: api::AnimCurve::Ease { ease: api::Ease { kind: api::EaseKind::Linear } },
        duration_ms: 100,
        delay_ms: 0,
        repeat: api::Repeat::Forever,
    };
    anim.start(node, d);
    let t0 = oxide_timing::now_ms();
    let _ = anim.step(t0 + 100);
    assert_eq!(anim.active_count(), 1);
    let _ = anim.step(t0 + 250);
    assert_eq!(anim.active_count(), 1);
}

#[test]
fn animator_transform_and_color_are_finite() {
    let mut anim = Animator::new();
    let node = NodeId(13);
    // Transform
    let d_tx = api::AnimDesc {
        id: 4,
        prop: api::AnimProp::Transform2D,
        from: api::AnimValue::Xform2D(api::Transform2D {
            tx: 0.0,
            ty: 0.0,
            sx: 1.0,
            sy: 1.0,
            rot_rad: 0.0,
        }),
        to: api::AnimValue::Xform2D(api::Transform2D {
            tx: 10.0,
            ty: -5.0,
            sx: 1.2,
            sy: 0.8,
            rot_rad: 0.5,
        }),
        curve: api::AnimCurve::Ease { ease: api::Ease { kind: api::EaseKind::CubicInOut } },
        duration_ms: 120,
        delay_ms: 0,
        repeat: api::Repeat::Once,
    };
    anim.start(node, d_tx);
    let t0 = oxide_timing::now_ms();
    let over = anim.step(t0 + 60);
    let xf = over.get(&node).and_then(|o| o.transform).unwrap();
    assert!(
        xf.tx.is_finite()
            && xf.ty.is_finite()
            && xf.sx.is_finite()
            && xf.sy.is_finite()
            && xf.rot_rad.is_finite()
    );

    // Color
    let d_col = api::AnimDesc {
        id: 5,
        prop: api::AnimProp::ColorRGBA,
        from: api::AnimValue::Vec4([0.1, 0.2, 0.3, 0.4]),
        to: api::AnimValue::Vec4([0.9, 0.8, 0.7, 1.0]),
        curve: api::AnimCurve::Ease { ease: api::Ease { kind: api::EaseKind::QuadOut } },
        duration_ms: 200,
        delay_ms: 0,
        repeat: api::Repeat::Once,
    };
    anim.start(node, d_col);
    let over2 = anim.step(t0 + 100);
    let c = over2.get(&node).and_then(|o| o.color).unwrap();
    assert!(c.r.is_finite() && c.g.is_finite() && c.b.is_finite() && c.a.is_finite());
    assert!(
        c.r >= 0.0
            && c.r <= 1.0
            && c.g >= 0.0
            && c.g <= 1.0
            && c.b >= 0.0
            && c.b <= 1.0
            && c.a >= 0.0
            && c.a <= 1.0
    );
}
