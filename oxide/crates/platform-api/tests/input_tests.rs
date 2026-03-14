use oxide_platform_api::{
    AnimCurve, AnimDesc, AnimProp, AnimValue, AppEvent, ColorSpace, DeviceCaps, HapticPattern,
    KeyCode, KeyEvent, Lifecycle, Modifiers, PointerDevice, PointerEvent, TouchEvent, TouchPhase,
    WindowEvent,
};
use oxide_renderer_api::{Insets, RectF};

#[test]
fn modifiers_bit_operations() {
    let combo = Modifiers::SHIFT | Modifiers::CONTROL;
    assert!(combo.contains(Modifiers::SHIFT));
    assert!(combo.contains(Modifiers::CONTROL));
    assert!(!combo.contains(Modifiers::ALT));
    let lowered = combo & !Modifiers::SHIFT;
    assert!(!lowered.contains(Modifiers::SHIFT));
    assert!(lowered.contains(Modifiers::CONTROL));
}

#[test]
fn key_event_roundtrip_chars() {
    let event = KeyEvent {
        code: KeyCode::Letter('K'),
        chars: Some("k".into()),
        repeat: true,
        modifiers: Modifiers::SHIFT,
    };
    assert_eq!(event.code, KeyCode::Letter('K'));
    assert_eq!(event.chars.as_deref(), Some("k"));
    assert!(event.repeat);
    assert!(event.modifiers.contains(Modifiers::SHIFT));
}

#[test]
fn touch_event_equality_and_device() {
    let t0 = TouchEvent {
        id: oxide_platform_api::TouchId(7),
        phase: TouchPhase::Start,
        x: 12.0,
        y: 34.0,
        pressure: Some(0.5),
        tilt: Some((0.1, 0.2)),
        device: PointerDevice::Pencil,
    };
    let mut t1 = t0;
    assert_eq!(t0, t1);
    t1.phase = TouchPhase::Move;
    assert_ne!(t0, t1);
}

#[test]
fn app_event_holds_expected_variants() {
    let resize =
        WindowEvent::Resized { w: 800, h: 600, scale: 2.0, safe: Insets::new(1.0, 2.0, 3.0, 4.0) };
    let evt = AppEvent::Window(resize);
    match evt {
        AppEvent::Window(WindowEvent::Resized { w, h, scale, safe }) => {
            assert_eq!((w, h), (800, 600));
            assert!((scale - 2.0).abs() < 1e-6);
            assert_eq!(safe.left, 1.0);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn pointer_event_defaults() {
    let event = PointerEvent {
        x: 10.0,
        y: 20.0,
        dx: 1.0,
        dy: -1.0,
        buttons: Default::default(),
        modifiers: Modifiers::empty(),
    };
    assert_eq!(event.x, 10.0);
    assert_eq!(event.y, 20.0);
    assert_eq!(event.dx, 1.0);
    assert_eq!(event.dy, -1.0);
    assert!(!event.buttons.left);
}

#[test]
fn anim_desc_clone_roundtrip() {
    let desc = AnimDesc {
        id: 42,
        prop: AnimProp::ColorRGBA,
        from: AnimValue::Vec4([0.0, 0.0, 0.0, 1.0]),
        to: AnimValue::Vec4([1.0, 0.5, 0.25, 1.0]),
        curve: AnimCurve::Ease {
            ease: oxide_platform_api::Ease { kind: oxide_platform_api::EaseKind::CubicInOut },
        },
        duration_ms: 200,
        delay_ms: 10,
        repeat: oxide_platform_api::Repeat::Count(2),
    };
    let clone = desc.clone();
    assert_eq!(clone.id, 42);
    assert_eq!(clone.prop, AnimProp::ColorRGBA);
    assert_eq!(clone.duration_ms, 200);
}

#[test]
fn device_caps_copy() {
    let caps = DeviceCaps {
        max_framerate_hz: 120,
        supports_edr: true,
        supports_msaa4x: false,
        native_scale: 2.0,
        color_space: ColorSpace::DisplayP3Linear,
        a11y_reduce_motion: false,
    };
    let copy = caps;
    assert_eq!(copy.max_framerate_hz, 120);
    assert!(copy.supports_edr);
}

#[test]
fn text_event_variants() {
    let commit = AppEvent::Text(oxide_platform_api::TextEvent::Commit { text: "hello".into() });
    match commit {
        AppEvent::Text(oxide_platform_api::TextEvent::Commit { text }) => {
            assert_eq!(text, "hello")
        }
        _ => panic!("unexpected variant"),
    }

    let ime = oxide_platform_api::TextEvent::IMEShown(RectF::new(0.0, 0.0, 100.0, 50.0));
    match ime {
        oxide_platform_api::TextEvent::IMEShown(rect) => assert_eq!(rect.w, 100.0),
        _ => panic!("expected IMEShown"),
    }
}

#[test]
fn lifecycle_variants_and_haptics() {
    let lc = Lifecycle::DidEnterBackground;
    assert_eq!(format!("{:?}", lc), "DidEnterBackground");
    let hp = HapticPattern::NotificationSuccess;
    assert_eq!(format!("{:?}", hp), "NotificationSuccess");
}
