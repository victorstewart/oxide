use oxideui_renderer_api as gfx;
use oxideui_ui_core::overlay::{
    OverlayBehavior, OverlayPointerResult, OverlayStack, OverlayVisual, PopupManager, PopupSpec,
};
use oxideui_ui_core::{surface::UiSurface, Dim, NodeStyle, Size2D};

fn basic_surface(w: f32, h: f32) -> (UiSurface, oxideui_ui_core::NodeId) {
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
    let mut manager = PopupManager::new();
    let viewport = gfx::RectF::new(0.0, 0.0, 220.0, 220.0);
    manager.set_viewport(viewport, 1.0);
    let mut high = OverlayVisual::default();
    high.z_index = 10;
    let low = OverlayVisual::default();
    let handle_low =
        manager.push(popup_a, PopupSpec { visual: low, behavior: OverlayBehavior::default() });
    let handle_high =
        manager.push(popup_b, PopupSpec { visual: high, behavior: OverlayBehavior::default() });
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
