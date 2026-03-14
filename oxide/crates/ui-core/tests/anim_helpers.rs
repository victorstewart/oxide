use oxide_platform_api as api;
use oxide_ui_core::anim;

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

#[test]
fn cubic_bezier_ease_in_out_uses_non_linear_legacy_curve() {
    assert_eq!(anim::helpers::cubic_bezier_ease_in_out(0.0), 0.0);
    assert_eq!(anim::helpers::cubic_bezier_ease_in_out(1.0), 1.0);
    assert!(anim::helpers::cubic_bezier_ease_in_out(0.25) < 0.25);
    assert!(anim::helpers::cubic_bezier_ease_in_out(0.75) > 0.75);
}

#[test]
fn sample_keyframed_offset_interpolates_and_clamps_to_zero_after_duration() {
    let targets = [8.0, -4.0, 0.0];
    assert_eq!(anim::helpers::sample_keyframed_offset(0, 20, &targets), 0.0);
    assert_eq!(anim::helpers::sample_keyframed_offset(10, 20, &targets), 4.0);
    assert_eq!(anim::helpers::sample_keyframed_offset(20, 20, &targets), 8.0);
    assert_eq!(anim::helpers::sample_keyframed_offset(30, 20, &targets), 2.0);
    assert_eq!(anim::helpers::sample_keyframed_offset(60, 20, &targets), 0.0);
}

#[test]
fn required_field_shake_offset_matches_shared_profile_and_clamps_negative_scale() {
    assert_eq!(anim::helpers::required_field_shake_offset(0, 1.0), 0.0);
    assert!(anim::helpers::required_field_shake_offset(
        anim::helpers::REQUIRED_FIELD_SHAKE_PHASE_DURATION_MS / 2,
        1.0,
    ) < 0.0);
    assert_eq!(anim::helpers::required_field_shake_offset(0, -1.0), 0.0);
    assert_eq!(
        anim::helpers::required_field_shake_offset(
            anim::helpers::REQUIRED_FIELD_SHAKE_DURATION_MS,
            1.0,
        ),
        0.0,
    );
}
