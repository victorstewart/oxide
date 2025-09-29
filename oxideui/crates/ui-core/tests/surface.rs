use oxideui_renderer_api as gfx;
use oxideui_timing as timing;
use oxideui_ui_core::{
    ChromeMetrics, LayoutRect, NodeStyle, ScatterSpec, Size2D, SurfaceRouter, UiSurface,
};

#[test]
fn chrome_padding_applies_to_root_style() {
    let mut surface = UiSurface::new(NodeStyle::default());
    let metrics = ChromeMetrics {
        safe_insets: gfx::Insets::new(8.0, 12.0, 4.0, 2.0),
        status_bar_height: 12.0,
    };
    surface.set_chrome_metrics(metrics);
    surface.apply_chrome_padding_to_root();
    let style = surface.tree().style(surface.root()).unwrap();
    assert!((style.padding.left - 8.0).abs() < f32::EPSILON);
    assert!((style.padding.top - 12.0).abs() < f32::EPSILON);
    assert!((style.padding.right - 4.0).abs() < f32::EPSILON);
    assert!((style.padding.bottom - 2.0).abs() < f32::EPSILON);
}

#[test]
fn async_layout_job_updates_tree() {
    let mut surface = UiSurface::new(NodeStyle {
        size: Size2D { w: oxideui_ui_core::Dim::Px(100.0), h: oxideui_ui_core::Dim::Px(100.0) },
        ..NodeStyle::default()
    });
    surface.layout(100.0, 100.0);
    let root = surface.root();
    let target = LayoutRect { x: 10.0, y: 20.0, w: 50.0, h: 60.0 };
    let _seq = surface.request_async_layout(move || vec![(root, target)]);
    // Poll until the worker finishes and applies the layout update.
    for _ in 0..20 {
        if surface.poll_async_layout() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let layout = surface.tree().layout_rect(root).unwrap();
    assert!((layout.x - target.x).abs() < f32::EPSILON);
    assert!((layout.y - target.y).abs() < f32::EPSILON);
    assert!((layout.w - target.w).abs() < f32::EPSILON);
    assert!((layout.h - target.h).abs() < f32::EPSILON);
}

#[test]
fn scatter_blocks_and_releases_gate() {
    timing::testing::reset();
    let mut surface = UiSurface::new(NodeStyle::default());
    let root = surface.root();
    let child = surface.tree_mut().add_node(
        root,
        NodeStyle {
            size: Size2D { w: oxideui_ui_core::Dim::Px(50.0), h: oxideui_ui_core::Dim::Px(50.0) },
            ..NodeStyle::default()
        },
    );
    surface.run_scatter(&[ScatterSpec::new(child, [24.0, 0.0]).duration(120)]);
    assert!(surface.is_interaction_blocked());
    let now = timing::now_ms();
    surface.tick_at(now + 260);
    surface.tick_at(now + 400);
    assert!(!surface.is_interaction_blocked());
    assert!(surface.overrides().is_empty());
}

#[test]
fn surface_router_transition_triggers_scatter() {
    timing::testing::reset();
    let mut surface_a = UiSurface::new(NodeStyle {
        size: Size2D { w: oxideui_ui_core::Dim::Px(120.0), h: oxideui_ui_core::Dim::Px(120.0) },
        ..NodeStyle::default()
    });
    let mut surface_b = UiSurface::new(NodeStyle {
        size: Size2D { w: oxideui_ui_core::Dim::Px(120.0), h: oxideui_ui_core::Dim::Px(120.0) },
        ..NodeStyle::default()
    });
    let child_style = NodeStyle {
        size: Size2D { w: oxideui_ui_core::Dim::Px(100.0), h: oxideui_ui_core::Dim::Px(100.0) },
        ..NodeStyle::default()
    };
    let root_a = surface_a.root();
    let node_a = surface_a.tree_mut().add_node(root_a, child_style.clone());
    let root_b = surface_b.root();
    let node_b = surface_b.tree_mut().add_node(root_b, child_style);
    surface_a.layout(120.0, 120.0);
    surface_b.layout(120.0, 120.0);

    let mut router = SurfaceRouter::new(surface_a);
    let idx_b = router.push(surface_b);
    router.transition_to(
        idx_b,
        &[ScatterSpec::new(node_a, [0.0, -32.0]).duration(90)],
        &[ScatterSpec::new(node_b, [0.0, 32.0]).duration(90)],
    );
    assert!(router.surface(idx_b).unwrap().is_interaction_blocked());
    let mut hit_before = None;
    router.surface(idx_b).unwrap().route_pointer(60.0, 60.0, |id, _| hit_before = Some(id));
    assert!(hit_before.is_none());

    let now = timing::now_ms();
    router.tick_all_at(now + 200);
    assert!(!router.surface(idx_b).unwrap().is_interaction_blocked());

    let mut hit_after = None;
    router.surface(idx_b).unwrap().route_pointer(60.0, 60.0, |id, _| hit_after = Some(id));
    assert_eq!(hit_after, Some(node_b));
}
