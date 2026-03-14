use oxide_platform_api::{KeyboardEvent, KeyboardGeometry, KeyboardTransition};
use oxide_ui_core::keyboard::{KeyboardEventExt, KeyboardTracker};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[test]
fn tracker_updates_geometry_and_notifies() {
    let mut tracker = KeyboardTracker::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let captured = Arc::clone(&calls);
    let _ = tracker.add_listener(move |_| {
        captured.fetch_add(1, Ordering::Relaxed);
    });
    let transition = KeyboardTransition {
        geometry: KeyboardGeometry {
            visible: true,
            frame: oxide_renderer_api::RectF::new(0.0, 400.0, 320.0, 260.0),
            overlap_insets: oxide_renderer_api::Insets::new(0.0, 0.0, 0.0, 240.0),
        },
        animation_ms: 250,
    };
    tracker.on_event(KeyboardEvent::WillChange(transition));
    tracker.on_event(KeyboardEvent::DidChange(transition));
    assert_eq!(tracker.geometry(), transition.geometry);
    assert_eq!(calls.load(Ordering::Relaxed), 2);
}

#[test]
fn keyboard_event_extension() {
    let transition =
        KeyboardTransition { geometry: KeyboardGeometry::default(), animation_ms: 180 };
    let event = KeyboardEvent::DidChange(transition);
    assert_eq!(event.transition(), Some(transition));
}
