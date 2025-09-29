use oxideui_input::{GestureConfig, GestureEvent, GestureOutcome, GestureRecognizer};
use oxideui_platform_api as api;

fn touch(id: api::TouchId, phase: api::TouchPhase, x: f32, y: f32) -> api::TouchEvent {
    api::TouchEvent {
        id,
        phase,
        x,
        y,
        pressure: None,
        tilt: None,
        device: api::PointerDevice::Finger,
    }
}

#[test]
fn tap_emitted_within_thresholds() {
    let mut gr = GestureRecognizer::with_defaults();
    let start = touch(api::TouchId(1), api::TouchPhase::Start, 10.0, 20.0);
    assert!(gr.on_touch(&start, 1000).is_empty());

    let end = touch(api::TouchId(1), api::TouchPhase::End, 10.5, 19.5);
    let evs = gr.on_touch(&end, 1100);
    assert_eq!(evs.len(), 1);
    assert_eq!(evs[0], GestureEvent::Tap { id: api::TouchId(1), x: 10.5, y: 19.5 });
}

#[test]
fn tap_suppressed_when_moved_too_far() {
    let mut gr = GestureRecognizer::with_defaults();
    let start = touch(api::TouchId(2), api::TouchPhase::Start, 0.0, 0.0);
    gr.on_touch(&start, 0);
    // move beyond tap_max_move
    let end = touch(api::TouchId(2), api::TouchPhase::End, 20.0, 0.0);
    let evs = gr.on_touch(&end, 100);
    assert!(evs.is_empty());
}

#[test]
fn long_press_triggers_and_pan_after_threshold() {
    let mut gr = GestureRecognizer::with_defaults();
    let start = touch(api::TouchId(3), api::TouchPhase::Start, 5.0, 5.0);
    gr.on_touch(&start, 0);
    // hold past long_ms without moving beyond pan threshold
    let hold = touch(api::TouchId(3), api::TouchPhase::Move, 5.5, 5.5);
    let evs = gr.on_touch(&hold, 500);
    assert!(matches!(evs.first(), Some(GestureEvent::LongPress { .. })));

    // subsequent move exceeding pan threshold should start pan
    let pan_begin = touch(api::TouchId(3), api::TouchPhase::Move, 20.0, 5.5);
    let evs = gr.on_touch(&pan_begin, 520);
    assert!(matches!(evs.first(), Some(GestureEvent::PanStart { .. })));

    // final end should generate PanEnd
    let pan_end = touch(api::TouchId(3), api::TouchPhase::End, 25.0, 5.5);
    let evs = gr.on_touch(&pan_end, 540);
    assert!(matches!(evs.first(), Some(GestureEvent::PanEnd { .. })));
}

#[test]
fn cancel_clears_track() {
    let mut gr = GestureRecognizer::with_defaults();
    let start = touch(api::TouchId(4), api::TouchPhase::Start, 0.0, 0.0);
    gr.on_touch(&start, 0);
    let cancel = touch(api::TouchId(4), api::TouchPhase::Cancel, 0.0, 0.0);
    let evs = gr.on_touch(&cancel, 10);
    assert_eq!(evs.len(), 1);
    assert_eq!(evs[0], GestureEvent::Cancel { id: api::TouchId(4) });
    // Ending afterwards should do nothing (track removed)
    let end = touch(api::TouchId(4), api::TouchPhase::End, 0.0, 0.0);
    assert!(gr.on_touch(&end, 20).is_empty());
}

#[test]
fn pan_velocity_computation() {
    let mut cfg = GestureConfig::default();
    cfg.pan_min_move = 1.0;
    let mut gr = GestureRecognizer::new(cfg);
    let start = touch(api::TouchId(5), api::TouchPhase::Start, 0.0, 0.0);
    gr.on_touch(&start, 0);

    // initiate pan
    let move1 = touch(api::TouchId(5), api::TouchPhase::Move, 5.0, 0.0);
    let evs = gr.on_touch(&move1, 10);
    assert!(matches!(evs.first(), Some(GestureEvent::PanStart { .. })));

    let move2 = touch(api::TouchId(5), api::TouchPhase::Move, 8.0, 0.0);
    let evs = gr.on_touch(&move2, 20);
    assert!(matches!(evs.first(), Some(GestureEvent::PanMove { .. })));

    let end = touch(api::TouchId(5), api::TouchPhase::End, 10.0, 0.0);
    let evs = gr.on_touch(&end, 30);
    if let [GestureEvent::PanEnd { vx, vy, .. }] = evs.as_slice() {
        assert!(*vx > 0.0 && vy.abs() < 1e-3);
    } else {
        panic!("expected pan end event");
    }
}

#[test]
fn double_tap_detected() {
    let mut gr = GestureRecognizer::with_defaults();
    let start1 = touch(api::TouchId(10), api::TouchPhase::Start, 0.0, 0.0);
    gr.on_touch(&start1, 0);
    let end1 = touch(api::TouchId(10), api::TouchPhase::End, 0.5, 0.2);
    let tap = gr.on_touch(&end1, 80);
    assert!(matches!(tap.first(), Some(GestureEvent::Tap { .. })));

    let start2 = touch(api::TouchId(11), api::TouchPhase::Start, 0.2, 0.1);
    gr.on_touch(&start2, 130);
    let end2 = touch(api::TouchId(11), api::TouchPhase::End, 0.4, 0.0);
    let events = gr.on_touch(&end2, 200);
    assert_eq!(events.len(), 1);
    assert!(matches!(events.first(), Some(GestureEvent::DoubleTap { .. })));
}

#[test]
fn gesture_outcome_haptics() {
    let mut gr = GestureRecognizer::with_defaults();
    let start = touch(api::TouchId(20), api::TouchPhase::Start, 1.0, 1.0);
    gr.on_touch(&start, 0);
    let end = touch(api::TouchId(20), api::TouchPhase::End, 1.0, 1.0);
    let outcomes = gr.on_touch_with_feedback(&end, 40);
    assert_eq!(outcomes.len(), 1);
    assert_eq!(
        outcomes[0],
        GestureOutcome {
            event: GestureEvent::Tap { id: api::TouchId(20), x: 1.0, y: 1.0 },
            haptic: Some(api::HapticPattern::Selection)
        }
    );
}
