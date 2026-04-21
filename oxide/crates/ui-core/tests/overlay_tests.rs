use oxide_renderer_api as gfx;
use oxide_ui_core::overlay::{
    OverlayBehavior, OverlayPointerResult, OverlayStack, OverlayVisual, PopupCallbacks,
    PopupManager, PopupSpec, PopupTouchRegion,
};
use oxide_ui_core::{surface::UiSurface, Dim, NodeId, NodeStyle, Size2D};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

fn basic_surface(w: f32, h: f32) -> (UiSurface, oxide_ui_core::NodeId) {
    let mut surface = UiSurface::new(NodeStyle {
        size: Size2D { w: Dim::Px(w), h: Dim::Px(h) },
        ..NodeStyle::default()
    });
    let root = surface.root();
    let content = surface.tree_mut().add_node(
        root,
        NodeStyle {
            size: Size2D { w: Dim::Px(w * 0.5), h: Dim::Px(h * 0.5) },
            ..NodeStyle::default()
        },
    );
    surface.layout(w, h);
    (surface, content)
}

fn popup_manager() -> PopupManager {
    let mut manager = PopupManager::new();
    manager.set_viewport(gfx::RectF::new(0.0, 0.0, 220.0, 220.0), 1.0);
    manager
}

fn focused_popup_behavior(content: NodeId) -> OverlayBehavior {
    OverlayBehavior {
        content_root: Some(content),
        focus_root: Some(content),
        ..OverlayBehavior::default()
    }
}

fn noop_popup_callbacks() -> PopupCallbacks {
    PopupCallbacks {
        approve_dismissal: None,
        dismissal: Some(Box::new(|_| {})),
        approve_touch: None,
    }
}

#[test]
fn overlay_background_tap_dismisses() {
    let (surface, content) = basic_surface(200.0, 200.0);
    let mut stack = OverlayStack::new();
    stack.set_viewport(gfx::RectF::new(0.0, 0.0, 200.0, 200.0), 2.0);
    let behavior = OverlayBehavior {
        dismiss_on_background_tap: true,
        block_underlying_inputs: true,
        content_root: Some(content),
        focus_root: None,
    };
    let handle = stack.push(surface, OverlayVisual::default(), behavior);
    let inside = stack.pointer_event(40.0, 40.0, 1);
    match inside {
        OverlayPointerResult::Consumed { node, .. } => assert!(node.is_some()),
        _ => panic!("expected overlay to consume tap"),
    }
    let dismiss = stack.pointer_event(180.0, 180.0, 0);
    match dismiss {
        OverlayPointerResult::Dismissed { handle: dismissed } => assert_eq!(dismissed.0, handle.0),
        _ => panic!("expected overlay to dismiss on background tap"),
    }
    assert!(stack.is_empty());
}

#[test]
fn popup_z_order_prefers_topmost() {
    let (popup_a, _) = basic_surface(160.0, 160.0);
    let (popup_b, _) = basic_surface(160.0, 160.0);
    let mut manager = popup_manager();
    let high = OverlayVisual { z_index: 10, ..OverlayVisual::default() };
    let low = OverlayVisual::default();
    let handle_low = manager.push(
        popup_a,
        PopupSpec { visual: low, behavior: OverlayBehavior::default(), ..PopupSpec::default() },
    );
    let handle_high = manager.push(
        popup_b,
        PopupSpec { visual: high, behavior: OverlayBehavior::default(), ..PopupSpec::default() },
    );
    let result = manager.pointer_event(20.0, 20.0, 1);
    match result {
        OverlayPointerResult::Consumed { handle, .. } => assert_eq!(handle.0, handle_high.0),
        _ => panic!("topmost popup should consume input"),
    }
    let _ = manager.remove(handle_high);
    let result_low = manager.pointer_event(40.0, 40.0, 1);
    match result_low {
        OverlayPointerResult::Consumed { handle, .. } => assert_eq!(handle.0, handle_low.0),
        _ => panic!("lower popup should now consume input"),
    }
    let _ = manager.remove(handle_low);
}

#[test]
fn popup_key_window_tracks_topmost_popup() {
    let (popup_a, popup_content_a) = basic_surface(160.0, 160.0);
    let (popup_b, popup_content_b) = basic_surface(160.0, 160.0);
    let mut manager = popup_manager();
    let handle_low = manager.push(
        popup_a,
        PopupSpec { behavior: focused_popup_behavior(popup_content_a), ..PopupSpec::default() },
    );
    let handle_high = manager.push(
        popup_b,
        PopupSpec {
            visual: OverlayVisual { z_index: 5, ..OverlayVisual::default() },
            behavior: focused_popup_behavior(popup_content_b),
            ..PopupSpec::default()
        },
    );

    assert!(manager.popup_is_key_window());
    assert_eq!(manager.key_popup(), Some(handle_high));
    assert_eq!(manager.focus_target(), Some(popup_content_b));

    let _ = manager.remove(handle_high);
    assert_eq!(manager.key_popup(), Some(handle_low));

    let _ = manager.remove(handle_low);
    assert!(!manager.popup_is_key_window());
    assert_eq!(manager.key_popup(), None);
}

#[test]
fn popup_dismissal_obeys_approve_dismissal_and_runs_once() {
    let (popup, popup_content) = basic_surface(160.0, 160.0);
    let mut manager = popup_manager();

    let approvals = Arc::new(AtomicUsize::new(0));
    let dismissals = Arc::new(AtomicUsize::new(0));
    let allow_dismissal = Arc::new(AtomicBool::new(false));
    let handle = manager.push(
        popup,
        PopupSpec {
            behavior: focused_popup_behavior(popup_content),
            callbacks: PopupCallbacks {
                approve_dismissal: Some(Box::new({
                    let approvals = Arc::clone(&approvals);
                    let allow_dismissal = Arc::clone(&allow_dismissal);
                    move |_| {
                        approvals.fetch_add(1, Ordering::SeqCst);
                        allow_dismissal.load(Ordering::SeqCst)
                    }
                })),
                dismissal: Some(Box::new({
                    let dismissals = Arc::clone(&dismissals);
                    move |_| {
                        dismissals.fetch_add(1, Ordering::SeqCst);
                    }
                })),
                approve_touch: None,
            },
            ..PopupSpec::default()
        },
    );

    assert!(!manager.dismiss(handle));
    assert_eq!(approvals.load(Ordering::SeqCst), 1);
    assert_eq!(dismissals.load(Ordering::SeqCst), 0);
    assert!(manager.popup_is_key_window());

    allow_dismissal.store(true, Ordering::SeqCst);
    assert!(manager.dismiss(handle));
    assert_eq!(approvals.load(Ordering::SeqCst), 2);
    assert_eq!(dismissals.load(Ordering::SeqCst), 1);
    assert!(!manager.popup_is_key_window());
    assert!(!manager.dismiss(handle));
    assert_eq!(dismissals.load(Ordering::SeqCst), 1);
}

#[test]
fn popup_pointer_dismisses_outside_touch_region_on_press() {
    let (popup, popup_content) = basic_surface(160.0, 160.0);
    let mut manager = popup_manager();
    let dismissals = Arc::new(AtomicUsize::new(0));
    manager.push(
        popup,
        PopupSpec {
            behavior: focused_popup_behavior(popup_content),
            callbacks: PopupCallbacks {
                approve_dismissal: None,
                dismissal: Some(Box::new({
                    let dismissals = Arc::clone(&dismissals);
                    move |_| {
                        dismissals.fetch_add(1, Ordering::SeqCst);
                    }
                })),
                approve_touch: None,
            },
            ..PopupSpec::default()
        },
    );

    let dismiss = manager.pointer_event(180.0, 180.0, 1);
    match dismiss {
        OverlayPointerResult::Dismissed { .. } => {}
        _ => panic!("outside touch should dismiss popup immediately"),
    }
    assert_eq!(dismissals.load(Ordering::SeqCst), 1);
    assert!(manager.is_empty());
}

#[test]
fn popup_approve_touch_can_veto_inside_touches() {
    let (popup, popup_content) = basic_surface(160.0, 160.0);
    let mut manager = popup_manager();
    let dismissals = Arc::new(AtomicUsize::new(0));
    manager.push(
        popup,
        PopupSpec {
            behavior: focused_popup_behavior(popup_content),
            callbacks: PopupCallbacks {
                approve_dismissal: None,
                dismissal: Some(Box::new({
                    let dismissals = Arc::clone(&dismissals);
                    move |_| {
                        dismissals.fetch_add(1, Ordering::SeqCst);
                    }
                })),
                approve_touch: Some(Box::new(|_, point| point[0] < 20.0)),
            },
            ..PopupSpec::default()
        },
    );

    let dismiss = manager.pointer_event(40.0, 40.0, 1);
    match dismiss {
        OverlayPointerResult::Dismissed { .. } => {}
        _ => panic!("approve_touch should dismiss rejected touches"),
    }
    assert_eq!(dismissals.load(Ordering::SeqCst), 1);
}

#[test]
fn popup_content_size_changed_refreshes_content_touch_region() {
    let (popup, popup_content) = basic_surface(160.0, 160.0);
    let mut manager = popup_manager();
    let handle = manager.push(
        popup,
        PopupSpec {
            behavior: focused_popup_behavior(popup_content),
            callbacks: noop_popup_callbacks(),
            ..PopupSpec::default()
        },
    );

    {
        let surface = manager.surface_mut(handle).expect("popup surface");
        let style = surface.tree_mut().style_mut(popup_content).expect("popup content style");
        style.size = Size2D { w: Dim::Px(180.0), h: Dim::Px(180.0) };
    }
    assert!(manager.content_size_changed(handle));

    let hit = manager.pointer_event(170.0, 170.0, 1);
    match hit {
        OverlayPointerResult::Consumed { handle: hit_handle, .. } => assert_eq!(hit_handle, handle),
        _ => panic!("content_size_changed should refresh the default touch region"),
    }
}

#[test]
fn popup_manual_touch_region_overrides_content_root_resync() {
    let (popup, popup_content) = basic_surface(160.0, 160.0);
    let mut manager = popup_manager();
    let handle = manager.push(
        popup,
        PopupSpec {
            behavior: focused_popup_behavior(popup_content),
            callbacks: noop_popup_callbacks(),
            ..PopupSpec::default()
        },
    );

    assert!(manager
        .set_touch_region(handle, PopupTouchRegion::Rect(gfx::RectF::new(0.0, 0.0, 20.0, 20.0)),));
    {
        let surface = manager.surface_mut(handle).expect("popup surface");
        let style = surface.tree_mut().style_mut(popup_content).expect("popup content style");
        style.size = Size2D { w: Dim::Px(200.0), h: Dim::Px(200.0) };
    }
    assert!(manager.content_size_changed(handle));
    assert_eq!(
        manager.touch_region(handle),
        Some(PopupTouchRegion::Rect(gfx::RectF::new(0.0, 0.0, 20.0, 20.0))),
    );

    let dismiss = manager.pointer_event(30.0, 30.0, 1);
    match dismiss {
        OverlayPointerResult::Dismissed { handle: dismissed } => assert_eq!(dismissed, handle),
        _ => panic!("manual touch region should override content-root resync"),
    }
}
