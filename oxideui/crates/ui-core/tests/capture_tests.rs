use oxideui_renderer_api::{self as gfx, DrawCmd};
use oxideui_ui_core::overlay::{OverlayBehavior, OverlayVisual};
use oxideui_ui_core::{surface::SurfaceRouter, Dim, NodeStyle, Size2D, UiSurface};

fn colored_surface(color: gfx::Color) -> UiSurface {
    let mut surface = UiSurface::new(NodeStyle {
        size: Size2D { w: Dim::Px(200.0), h: Dim::Px(200.0) },
        background: color,
        ..NodeStyle::default()
    });
    surface.layout(200.0, 200.0);
    surface
}

#[test]
fn surface_capture_contains_draws() {
    let surface = colored_surface(gfx::Color::rgba(0.2, 0.3, 0.4, 1.0));
    let capture = surface.capture(gfx::RectF::new(0.0, 0.0, 200.0, 200.0), 2.0);
    assert_eq!(capture.viewport.w, 200.0);
    assert_eq!(capture.device_scale, 2.0);
    assert!(!capture.draw_list.items.is_empty());
}

#[test]
fn router_capture_includes_overlay_backdrop() {
    let base = colored_surface(gfx::Color::rgba(0.1, 0.1, 0.1, 1.0));
    let mut router = SurfaceRouter::new(base);
    let viewport = gfx::RectF::new(0.0, 0.0, 240.0, 240.0);
    router.set_viewport(viewport, 1.0);

    let mut overlay_surface = UiSurface::new(NodeStyle {
        size: Size2D { w: Dim::Px(240.0), h: Dim::Px(240.0) },
        background: gfx::Color::rgba(0.8, 0.2, 0.2, 0.9),
        ..NodeStyle::default()
    });
    overlay_surface.layout(240.0, 240.0);
    let behavior = OverlayBehavior {
        dismiss_on_background_tap: false,
        block_underlying_inputs: true,
        content_root: None,
        focus_root: None,
    };
    router.overlays_mut().push(overlay_surface, OverlayVisual::default(), behavior);

    let capture = router.capture(viewport, 1.0);
    assert!(capture.draw_list.items.iter().any(|cmd| matches!(cmd, DrawCmd::Backdrop { .. })));
}
