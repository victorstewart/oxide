use oxideui_platform_api as api;
use oxideui_ui_core::anim;

#[test]
fn shake_sequence_has_segments() {
    let seq = anim::helpers::shake(
        anim::helpers::identity_transform(),
        anim::helpers::Axis2D::Horizontal,
        10.0,
        2,
        400,
    );
    assert_eq!(seq.len(), 5);
    assert_eq!(seq[0].delay_ms, 0);
    assert!(matches!(seq[0].prop, api::AnimProp::Transform2D));
    let first = match &seq[0].to {
        api::AnimValue::Xform2D(t) => *t,
        _ => panic!("expected transform"),
    };
    assert!(first.tx > 0.0);
    let last = match &seq.last().unwrap().to {
        api::AnimValue::Xform2D(t) => *t,
        _ => panic!("expected transform"),
    };
    assert!((last.tx).abs() < 1e-3);
}

#[test]
fn shrink_grow_scale_profile() {
    let start = anim::helpers::shrink_grow_scale(0.0, 0.82, 1.08);
    let mid = anim::helpers::shrink_grow_scale(0.5, 0.82, 1.08);
    let end = anim::helpers::shrink_grow_scale(1.0, 0.82, 1.08);
    assert!(start < 0.9);
    assert!(mid > 1.0);
    assert!((end - 1.0).abs() < 1e-3);
}

#[test]
fn scatter_sequence_returns_to_base() {
    let seq = anim::helpers::scatter(anim::helpers::identity_transform(), [32.0, -24.0], 300, true);
    assert_eq!(seq.len(), 4);
    let last = seq.last().unwrap();
    assert_eq!(last.delay_ms, 300);
    assert!(matches!(last.prop, api::AnimProp::Opacity));
}
