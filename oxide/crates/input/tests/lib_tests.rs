use oxide_input::{
    GestureConfig, GestureEvent, GestureOutcome, GestureRecognizer, ScrollAccumulator,
    ScrollPhase, TouchSurfaceEvent, TouchSurfaceRecognizer,
};
use oxide_platform_api as api;

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

fn start(id: u64, x: f32, y: f32) -> api::TouchEvent {
    touch(api::TouchId(id), api::TouchPhase::Start, x, y)
}

fn mv(id: u64, x: f32, y: f32) -> api::TouchEvent {
    touch(api::TouchId(id), api::TouchPhase::Move, x, y)
}

fn end(id: u64, x: f32, y: f32) -> api::TouchEvent {
    touch(api::TouchId(id), api::TouchPhase::End, x, y)
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
fn tap_recognized_after_small_move() {
    let mut gr = GestureRecognizer::with_defaults();
    assert!(gr.on_touch(&start(1, 10.0, 10.0), 0).is_empty());
    assert!(gr.on_touch(&mv(1, 12.0, 10.0), 50).is_empty());
    let events = gr.on_touch(&end(1, 12.0, 10.0), 150);
    assert_eq!(events, vec![GestureEvent::Tap { id: api::TouchId(1), x: 12.0, y: 10.0 }]);
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
fn long_press_then_pan_move_and_end_are_ordered() {
    let mut gr = GestureRecognizer::with_defaults();
    gr.on_touch(&start(1, 0.0, 0.0), 0);
    let long = gr.on_touch(&mv(1, 1.0, 0.0), 500);
    assert_eq!(long, vec![GestureEvent::LongPress { id: api::TouchId(1), x: 1.0, y: 0.0 }]);
    let begin = gr.on_touch(&mv(1, 20.0, 0.0), 520);
    assert_eq!(begin, vec![GestureEvent::PanStart { id: api::TouchId(1), x: 20.0, y: 0.0 }]);
    let move_events = gr.on_touch(&mv(1, 25.0, 5.0), 540);
    assert_eq!(
        move_events,
        vec![GestureEvent::PanMove {
            id: api::TouchId(1),
            x: 25.0,
            y: 5.0,
            dx: 5.0,
            dy: 5.0,
        }]
    );
    let end_events = gr.on_touch(&end(1, 30.0, 10.0), 560);
    assert!(matches!(end_events.as_slice(), [GestureEvent::PanEnd { id, .. }] if *id == api::TouchId(1)));
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
fn cancel_breaks_gesture_without_late_end_event() {
    let mut gr = GestureRecognizer::with_defaults();
    gr.on_touch(&start(1, 0.0, 0.0), 0);
    let _ = gr.on_touch(&mv(1, 2.0, 2.0), 50);
    let cancel = touch(api::TouchId(1), api::TouchPhase::Cancel, 2.0, 2.0);
    assert_eq!(gr.on_touch(&cancel, 60), vec![GestureEvent::Cancel { id: api::TouchId(1) }]);
    assert!(gr.on_touch(&end(1, 2.0, 2.0), 80).is_empty());
}

#[test]
fn deterministic_move_order_within_tick() {
    let mut gr = GestureRecognizer::with_defaults();
    gr.on_touch(&start(1, 0.0, 0.0), 0);
    let _ = gr.on_touch(&mv(1, 10.0, 0.0), 1);
    let first = gr.on_touch(&mv(1, 15.0, 0.0), 2);
    let second = gr.on_touch(&mv(1, 20.0, 0.0), 2);
    match (&first[..], &second[..]) {
        ([GestureEvent::PanMove { x: x1, .. }], [GestureEvent::PanMove { x: x2, .. }]) => {
            assert!(*x1 < *x2);
        }
        _ => panic!("expected PanMove events"),
    }
}

#[test]
fn scroll_accumulates_resets_and_tracks_momentum() {
    let mut acc = ScrollAccumulator::default();
    acc.push(1.0, 2.0, ScrollPhase::Began);
    acc.push(0.5, 1.0, ScrollPhase::Changed);
    let (dx, dy) = acc.take();
    assert!((dx - 1.5).abs() < 1e-6 && (dy - 3.0).abs() < 1e-6);
    let (dx2, dy2) = acc.take();
    assert!(dx2.abs() < 1e-6 && dy2.abs() < 1e-6);

    acc.push(0.0, -1.0, ScrollPhase::Momentum);
    assert!(acc.momentum());
    let _ = acc.take();
    assert!(acc.momentum());
    acc.push(0.0, 0.0, ScrollPhase::Began);
    assert!(!acc.momentum());
}

#[test]
fn gesture_track_overflow_replaces_duplicate_start() {
    let mut gr = GestureRecognizer::with_defaults();
    for id in 1..=4 {
        gr.on_touch(&touch(api::TouchId(id), api::TouchPhase::Start, id as f32, 0.0), id);
    }
    gr.on_touch(&touch(api::TouchId(5), api::TouchPhase::Start, 0.0, 0.0), 10);
    gr.on_touch(&touch(api::TouchId(5), api::TouchPhase::Start, 100.0, 100.0), 20);

    let events = gr.on_touch(&touch(api::TouchId(5), api::TouchPhase::End, 100.0, 100.0), 60);

    assert_eq!(events.len(), 1);
    assert_eq!(events[0], GestureEvent::Tap { id: api::TouchId(5), x: 100.0, y: 100.0 });
}

#[test]
fn pan_velocity_computation() {
    let cfg = GestureConfig { pan_min_move: 1.0, ..GestureConfig::default() };
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

#[test]
fn touch_surface_recognizer_owns_two_touch_pinch_semantics() {
    let mut surface = TouchSurfaceRecognizer::new();
    surface.on_touch(&touch(api::TouchId(1), api::TouchPhase::Start, 180.0, 400.0));
    surface.on_touch(&touch(api::TouchId(2), api::TouchPhase::Start, 220.0, 400.0));

    let out = surface.on_touch(&touch(api::TouchId(2), api::TouchPhase::Move, 260.0, 400.0));

    assert!(out.iter().any(|event| matches!(
        event,
        TouchSurfaceEvent::Pinch { scale_delta, log2_scale_delta, .. }
            if (*scale_delta - 2.0).abs() < 0.001 && (*log2_scale_delta - 1.0).abs() < 0.001
    )));
    assert!(out.iter().any(|event| matches!(
        event,
        TouchSurfaceEvent::Pan { touch_count: 2, dx, dy, .. }
            if (*dx - 20.0).abs() < 0.001 && dy.abs() < 0.001
    )));
}

#[test]
fn touch_surface_single_touch_pan_from_raw_events() {
    let mut surface = TouchSurfaceRecognizer::new();
    assert_eq!(
        surface.on_touch(&start(1, 100.0, 200.0)),
        vec![TouchSurfaceEvent::ActiveTouchesChanged { touch_count: 1, x: 100.0, y: 200.0 }]
    );

    assert_eq!(
        surface.on_touch(&mv(1, 124.0, 190.0)),
        vec![TouchSurfaceEvent::Pan {
            touch_count: 1,
            x: 124.0,
            y: 190.0,
            dx: 24.0,
            dy: -10.0,
        }]
    );
}

#[test]
fn touch_surface_two_touch_pinch_and_center_pan_from_raw_events() {
    let mut surface = TouchSurfaceRecognizer::new();
    let _ = surface.on_touch(&start(2, 220.0, 400.0));
    let _ = surface.on_touch(&start(1, 180.0, 400.0));

    assert_eq!(
        surface.on_touch(&mv(1, 160.0, 390.0)),
        vec![
            TouchSurfaceEvent::Pinch {
                x: 190.0,
                y: 395.0,
                scale_delta: ((60.0_f32 * 60.0) + (10.0_f32 * 10.0)).sqrt() / 40.0,
                log2_scale_delta: (((60.0_f32 * 60.0) + (10.0_f32 * 10.0)).sqrt() / 40.0).log2(),
            },
            TouchSurfaceEvent::Pan { touch_count: 2, x: 190.0, y: 395.0, dx: -10.0, dy: -5.0 },
        ]
    );
}

#[test]
fn touch_surface_frame_uses_lowest_two_touch_ids() {
    let mut surface = TouchSurfaceRecognizer::new();
    surface.on_touch(&touch(api::TouchId(7), api::TouchPhase::Start, 700.0, 700.0));
    surface.on_touch(&touch(api::TouchId(5), api::TouchPhase::Start, 100.0, 100.0));
    surface.on_touch(&touch(api::TouchId(6), api::TouchPhase::Start, 300.0, 100.0));

    let out = surface.on_touch(&touch(api::TouchId(6), api::TouchPhase::Move, 340.0, 120.0));

    assert!(out.iter().any(|event| matches!(
        event,
        TouchSurfaceEvent::Pinch { x, y, scale_delta, .. }
            if (*x - 220.0).abs() < 0.001
                && (*y - 110.0).abs() < 0.001
                && *scale_delta > 1.20
                && *scale_delta < 1.21
    )));
    assert!(out.iter().any(|event| matches!(
        event,
        TouchSurfaceEvent::Pan { touch_count: 2, x, y, dx, dy }
            if (*x - 220.0).abs() < 0.001
                && (*y - 110.0).abs() < 0.001
                && (*dx - 20.0).abs() < 0.001
                && (*dy - 10.0).abs() < 0.001
    )));
}

#[test]
fn touch_surface_overflow_preserves_active_count_and_lowest_pair() {
    let mut surface = TouchSurfaceRecognizer::new();
    surface.on_touch(&touch(api::TouchId(20), api::TouchPhase::Start, 500.0, 500.0));
    surface.on_touch(&touch(api::TouchId(21), api::TouchPhase::Start, 600.0, 500.0));
    surface.on_touch(&touch(api::TouchId(22), api::TouchPhase::Start, 700.0, 500.0));
    surface.on_touch(&touch(api::TouchId(23), api::TouchPhase::Start, 800.0, 500.0));
    surface.on_touch(&touch(api::TouchId(19), api::TouchPhase::Start, 100.0, 100.0));

    assert_eq!(surface.active_count(), 5);
    let out = surface.on_touch(&touch(api::TouchId(20), api::TouchPhase::Move, 540.0, 520.0));

    assert!(out.iter().any(|event| matches!(
        event,
        TouchSurfaceEvent::Pan { touch_count: 2, x, y, dx, dy }
            if (*x - 320.0).abs() < 0.001
                && (*y - 310.0).abs() < 0.001
                && (*dx - 20.0).abs() < 0.001
                && (*dy - 10.0).abs() < 0.001
    )));
}

#[test]
fn touch_surface_cancel_removes_active_touch() {
    let mut surface = TouchSurfaceRecognizer::new();
    let _ = surface.on_touch(&start(1, 0.0, 0.0));

    assert_eq!(surface.active_count(), 1);
    let cancel = touch(api::TouchId(1), api::TouchPhase::Cancel, 0.0, 0.0);
    let out = surface.on_touch(&cancel);

    assert_eq!(surface.active_count(), 0);
    assert_eq!(
        out,
        vec![TouchSurfaceEvent::ActiveTouchesChanged { touch_count: 0, x: 0.0, y: 0.0 }]
    );
}
