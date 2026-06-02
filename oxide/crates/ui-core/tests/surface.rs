use oxide_platform_api::Transform2D;
use oxide_renderer_api as gfx;
use oxide_timing as timing;
use oxide_ui_core::{
    elements::TextCtx, Axis, ChromeMetrics, Dim, DirtyClass, DrawListBuilder, Edges, LayoutRect,
    NodeId, NodeStyle, OverlayBehavior, OverlayVisual, PopupSpec, RetainedDrawStatus, ScatterSpec,
    Size2D, SurfaceRouter, UiSurface,
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
        size: Size2D { w: oxide_ui_core::Dim::Px(100.0), h: oxide_ui_core::Dim::Px(100.0) },
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
fn retained_encode_reuses_clean_drawlist_and_rebuilds_after_dirty() {
    let mut surface = UiSurface::new(NodeStyle {
        size: Size2D { w: oxide_ui_core::Dim::Px(100.0), h: oxide_ui_core::Dim::Px(100.0) },
        background: gfx::Color::rgba(0.2, 0.3, 0.4, 1.0),
        ..NodeStyle::default()
    });
    surface.layout(100.0, 100.0);

    let mut first = DrawListBuilder::new();
    assert_eq!(surface.encode_retained(&mut first), RetainedDrawStatus::Rebuilt);
    assert!(!surface.dirty().affects_draw());
    let first_items = first.drawlist().items.clone();

    let mut second = DrawListBuilder::new();
    assert_eq!(surface.encode_retained(&mut second), RetainedDrawStatus::Reused);
    assert_eq!(second.drawlist().items, first_items);

    surface.mark_dirty(DirtyClass::Paint);
    let mut third = DrawListBuilder::new();
    assert_eq!(surface.encode_retained(&mut third), RetainedDrawStatus::Rebuilt);
    assert_eq!(third.drawlist().items, first_items);
    assert_eq!(surface.retained_node_stats().rebuilt_nodes, 1);
    assert_eq!(surface.retained_node_stats().reused_nodes, 0);
}

#[test]
fn retained_encode_text_atlas_context_overload_reuses_clean_surface() {
    let mut surface = UiSurface::new(NodeStyle {
        size: Size2D { w: oxide_ui_core::Dim::Px(100.0), h: oxide_ui_core::Dim::Px(100.0) },
        background: gfx::Color::rgba(0.2, 0.3, 0.4, 1.0),
        ..NodeStyle::default()
    });
    let atlases = [(gfx::ImageHandle(4), 3)];
    surface.layout(100.0, 100.0);

    let mut first = DrawListBuilder::new();
    assert_eq!(
        surface.encode_retained_with_text_atlas_revisions(&mut first, &atlases),
        RetainedDrawStatus::Rebuilt,
    );
    let first_items = first.drawlist().items.clone();

    let mut second = DrawListBuilder::new();
    assert_eq!(
        surface.encode_retained_with_text_atlas_revisions(&mut second, &atlases),
        RetainedDrawStatus::Reused,
    );
    assert_eq!(second.drawlist().items, first_items);
}

#[test]
fn text_ctx_retained_snapshot_requires_clean_uploaded_atlas() {
    let mut text = TextCtx::default();
    text.atlas_handle = Some(gfx::ImageHandle(4));

    assert_eq!(text.retained_text_atlas_revision(), Some((gfx::ImageHandle(4), 0)));

    text.atlas.reset();
    assert_eq!(text.retained_text_atlas_revision(), None);

    text.atlas.clear_dirty();
    assert_eq!(text.retained_text_atlas_revision(), Some((gfx::ImageHandle(4), 1)));
}

#[test]
fn retained_dirty_leaf_reuses_clean_sibling_subtree() {
    let mut surface = UiSurface::new(NodeStyle {
        axis: oxide_ui_core::Axis::Row,
        size: Size2D { w: oxide_ui_core::Dim::Px(120.0), h: oxide_ui_core::Dim::Px(80.0) },
        gap: 4.0,
        ..NodeStyle::default()
    });
    let root = surface.root();
    let left = surface.tree_mut().add_node(
        root,
        NodeStyle {
            size: Size2D { w: oxide_ui_core::Dim::Px(40.0), h: oxide_ui_core::Dim::Px(40.0) },
            background: gfx::Color::rgba(0.2, 0.3, 0.4, 1.0),
            ..NodeStyle::default()
        },
    );
    let right = surface.tree_mut().add_node(
        root,
        NodeStyle {
            size: Size2D { w: oxide_ui_core::Dim::Px(40.0), h: oxide_ui_core::Dim::Px(40.0) },
            background: gfx::Color::rgba(0.4, 0.3, 0.2, 1.0),
            ..NodeStyle::default()
        },
    );
    surface.layout(120.0, 80.0);

    let mut warm = DrawListBuilder::new();
    assert_eq!(surface.encode_retained(&mut warm), RetainedDrawStatus::Rebuilt);
    assert_eq!(surface.retained_node_stats().rebuilt_nodes, 3);

    assert!(surface.edit_style(left, |style| {
        style.background = gfx::Color::rgba(0.9, 0.1, 0.1, 1.0);
    }));
    assert!(!surface.dirty().contains(DirtyClass::Layout));
    assert!(surface.dirty().contains(DirtyClass::Paint));
    surface.layout(120.0, 80.0);
    let mut dirty = DrawListBuilder::new();
    assert_eq!(surface.encode_retained(&mut dirty), RetainedDrawStatus::Rebuilt);
    let stats = surface.retained_node_stats();
    assert!(stats.rebuilt_nodes >= 2, "dirty leaf and root should rebuild, got {stats:?}");
    assert!(stats.reused_nodes >= 1, "clean sibling should replay from cache, got {stats:?}");

    let layout = surface.tree().layout_rect(right).unwrap();
    assert_eq!(layout.w, 40.0);
}

#[test]
fn transform_only_edit_skips_layout_and_updates_hit_testing() {
    let mut surface = UiSurface::new(NodeStyle {
        axis: Axis::Row,
        size: Size2D { w: Dim::Px(120.0), h: Dim::Px(80.0) },
        padding: Edges { left: 8.0, top: 6.0, right: 0.0, bottom: 0.0 },
        ..NodeStyle::default()
    });
    let root = surface.root();
    let child = surface.tree_mut().add_node(
        root,
        NodeStyle {
            axis: Axis::Column,
            size: Size2D { w: Dim::Px(40.0), h: Dim::Px(40.0) },
            background: gfx::Color::rgba(0.2, 0.3, 0.4, 1.0),
            ..NodeStyle::default()
        },
    );
    let leaf = surface.tree_mut().add_node(
        child,
        NodeStyle {
            size: Size2D { w: Dim::Px(12.0), h: Dim::Px(12.0) },
            background: gfx::Color::rgba(0.8, 0.3, 0.2, 1.0),
            ..NodeStyle::default()
        },
    );
    let cold = surface.layout(120.0, 80.0);
    assert!(cold.measured_children > 0);
    let child_layout = surface.tree().layout_rect(child).unwrap();
    let leaf_layout = surface.tree().layout_rect(leaf).unwrap();

    let mut warm = DrawListBuilder::new();
    assert_eq!(surface.encode_retained(&mut warm), RetainedDrawStatus::Rebuilt);
    assert!(surface.edit_style(child, |style| {
        style.transform = Transform2D { tx: 12.0, ty: 5.0, sx: 1.0, sy: 1.0, rot_rad: 0.0 };
    }));
    assert!(surface.dirty().contains(DirtyClass::Transform));
    assert!(surface.dirty().contains(DirtyClass::HitTest));
    assert!(!surface.dirty().contains(DirtyClass::Layout));

    let skipped = surface.layout(120.0, 80.0);
    assert_eq!(skipped, oxide_ui_core::LayoutStats::default());
    assert_eq!(surface.tree().layout_rect(child), Some(child_layout));
    assert_eq!(surface.tree().layout_rect(leaf), Some(leaf_layout));

    let (hit_leaf, offset) =
        surface.hit_test(leaf_layout.x + 13.0, leaf_layout.y + 6.0).expect("translated leaf hit");
    assert_eq!(hit_leaf, leaf);
    assert!((offset[0] - 1.0).abs() < 1e-3);
    assert!((offset[1] - 1.0).abs() < 1e-3);

    let mut moved = DrawListBuilder::new();
    assert_eq!(surface.encode_retained(&mut moved), RetainedDrawStatus::Rebuilt);
    assert!(!surface.dirty().affects_draw());
}

#[test]
fn opacity_and_clip_edits_skip_layout_and_reuse_retained_subtrees() {
    let mut surface = UiSurface::new(NodeStyle {
        axis: Axis::Row,
        size: Size2D { w: Dim::Px(160.0), h: Dim::Px(80.0) },
        gap: 4.0,
        ..NodeStyle::default()
    });
    let root = surface.root();
    let target = surface.tree_mut().add_node(
        root,
        NodeStyle {
            axis: Axis::Column,
            size: Size2D { w: Dim::Px(56.0), h: Dim::Px(60.0) },
            background: gfx::Color::rgba(0.2, 0.3, 0.4, 1.0),
            ..NodeStyle::default()
        },
    );
    let leaf = surface.tree_mut().add_node(
        target,
        NodeStyle {
            size: Size2D { w: Dim::Px(24.0), h: Dim::Px(24.0) },
            background: gfx::Color::rgba(0.7, 0.2, 0.2, 1.0),
            ..NodeStyle::default()
        },
    );
    let sibling = surface.tree_mut().add_node(
        root,
        NodeStyle {
            size: Size2D { w: Dim::Px(56.0), h: Dim::Px(60.0) },
            background: gfx::Color::rgba(0.2, 0.6, 0.3, 1.0),
            ..NodeStyle::default()
        },
    );
    let cold = surface.layout(160.0, 80.0);
    assert!(cold.measured_children > 0);
    let leaf_layout = surface.tree().layout_rect(leaf).unwrap();
    let sibling_layout = surface.tree().layout_rect(sibling).unwrap();

    let mut warm = DrawListBuilder::new();
    assert_eq!(surface.encode_retained(&mut warm), RetainedDrawStatus::Rebuilt);

    assert!(surface.edit_style(target, |style| {
        style.opacity = 0.42;
    }));
    assert!(surface.dirty().contains(DirtyClass::Opacity));
    assert!(surface.dirty().contains(DirtyClass::Paint));
    assert!(!surface.dirty().contains(DirtyClass::Layout));
    assert_eq!(surface.layout(160.0, 80.0), oxide_ui_core::LayoutStats::default());
    let mut opacity_draws = DrawListBuilder::new();
    assert_eq!(surface.encode_retained(&mut opacity_draws), RetainedDrawStatus::Rebuilt);
    let opacity_stats = surface.retained_node_stats();
    assert!(
        opacity_stats.rebuilt_nodes >= 2,
        "root and target should rebuild, got {opacity_stats:?}"
    );
    assert!(
        opacity_stats.reused_nodes >= 2,
        "leaf and sibling should replay, got {opacity_stats:?}"
    );

    assert!(surface.edit_style(target, |style| {
        style.clip = true;
    }));
    assert!(surface.dirty().contains(DirtyClass::Clip));
    assert!(surface.dirty().contains(DirtyClass::HitTest));
    assert!(!surface.dirty().contains(DirtyClass::Layout));
    assert_eq!(surface.layout(160.0, 80.0), oxide_ui_core::LayoutStats::default());
    assert_eq!(surface.tree().layout_rect(leaf), Some(leaf_layout));
    assert_eq!(surface.tree().layout_rect(sibling), Some(sibling_layout));
    let mut clip_draws = DrawListBuilder::new();
    assert_eq!(surface.encode_retained(&mut clip_draws), RetainedDrawStatus::Rebuilt);
    let clip_stats = surface.retained_node_stats();
    assert!(
        clip_stats.rebuilt_nodes >= 2,
        "root and clipped node should rebuild, got {clip_stats:?}"
    );
    assert!(
        clip_stats.reused_nodes >= 2,
        "clipped child and sibling should replay, got {clip_stats:?}"
    );
    assert!(
        clip_draws.drawlist().items.iter().any(|cmd| matches!(cmd, gfx::DrawCmd::ClipPush { .. })),
        "clip edit should encode a clip command"
    );
}

#[test]
fn node_content_dirty_classes_skip_layout_and_reuse_retained_subtrees() {
    let mut surface = UiSurface::new(NodeStyle {
        axis: Axis::Row,
        size: Size2D { w: Dim::Px(180.0), h: Dim::Px(80.0) },
        gap: 4.0,
        ..NodeStyle::default()
    });
    let root = surface.root();
    let target = surface.tree_mut().add_node(
        root,
        NodeStyle {
            axis: Axis::Column,
            size: Size2D { w: Dim::Px(56.0), h: Dim::Px(60.0) },
            background: gfx::Color::rgba(0.2, 0.3, 0.4, 1.0),
            ..NodeStyle::default()
        },
    );
    let leaf = surface.tree_mut().add_node(
        target,
        NodeStyle {
            size: Size2D { w: Dim::Px(24.0), h: Dim::Px(24.0) },
            background: gfx::Color::rgba(0.7, 0.2, 0.2, 1.0),
            ..NodeStyle::default()
        },
    );
    let sibling = surface.tree_mut().add_node(
        root,
        NodeStyle {
            size: Size2D { w: Dim::Px(56.0), h: Dim::Px(60.0) },
            background: gfx::Color::rgba(0.2, 0.6, 0.3, 1.0),
            ..NodeStyle::default()
        },
    );
    let cold = surface.layout(180.0, 80.0);
    assert!(cold.measured_children > 0);
    let leaf_layout = surface.tree().layout_rect(leaf).unwrap();
    let sibling_layout = surface.tree().layout_rect(sibling).unwrap();

    let mut warm = DrawListBuilder::new();
    assert_eq!(surface.encode_retained(&mut warm), RetainedDrawStatus::Rebuilt);
    for class in [DirtyClass::Text, DirtyClass::ImageContent, DirtyClass::CameraFrame] {
        assert!(surface.mark_node_dirty(leaf, class));
        assert!(surface.dirty().contains(class));
        assert!(!surface.dirty().contains(DirtyClass::Layout));
        assert_eq!(surface.layout(180.0, 80.0), oxide_ui_core::LayoutStats::default());
        assert_eq!(surface.tree().layout_rect(leaf), Some(leaf_layout));
        assert_eq!(surface.tree().layout_rect(sibling), Some(sibling_layout));

        let mut dirty = DrawListBuilder::new();
        assert_eq!(surface.encode_retained(&mut dirty), RetainedDrawStatus::Rebuilt);
        let stats = surface.retained_node_stats();
        assert!(stats.rebuilt_nodes >= 3, "root, target, and leaf should rebuild, got {stats:?}");
        assert!(stats.reused_nodes >= 1, "clean sibling should replay, got {stats:?}");
    }
    assert!(!surface.mark_node_dirty(NodeId(99), DirtyClass::Text));
}

#[test]
fn non_draw_dirty_classes_skip_layout_and_reuse_retained_drawlist() {
    let mut surface = UiSurface::new(NodeStyle {
        axis: Axis::Row,
        size: Size2D { w: Dim::Px(180.0), h: Dim::Px(80.0) },
        gap: 4.0,
        ..NodeStyle::default()
    });
    let root = surface.root();
    let target = surface.tree_mut().add_node(
        root,
        NodeStyle {
            axis: Axis::Column,
            size: Size2D { w: Dim::Px(56.0), h: Dim::Px(60.0) },
            background: gfx::Color::rgba(0.2, 0.3, 0.4, 1.0),
            ..NodeStyle::default()
        },
    );
    let leaf = surface.tree_mut().add_node(
        target,
        NodeStyle {
            size: Size2D { w: Dim::Px(24.0), h: Dim::Px(24.0) },
            background: gfx::Color::rgba(0.7, 0.2, 0.2, 1.0),
            ..NodeStyle::default()
        },
    );
    let sibling = surface.tree_mut().add_node(
        root,
        NodeStyle {
            size: Size2D { w: Dim::Px(56.0), h: Dim::Px(60.0) },
            background: gfx::Color::rgba(0.2, 0.6, 0.3, 1.0),
            ..NodeStyle::default()
        },
    );
    let cold = surface.layout(180.0, 80.0);
    assert!(cold.measured_children > 0);
    let leaf_layout = surface.tree().layout_rect(leaf).unwrap();
    let sibling_layout = surface.tree().layout_rect(sibling).unwrap();

    let mut warm = DrawListBuilder::new();
    assert_eq!(surface.encode_retained(&mut warm), RetainedDrawStatus::Rebuilt);
    let warm_draws = warm.drawlist().items.clone();
    for class in [DirtyClass::Accessibility, DirtyClass::HitTest] {
        assert!(surface.mark_node_dirty(leaf, class));
        assert!(surface.dirty().contains(class));
        assert!(!surface.dirty().contains(DirtyClass::Layout));
        assert!(!surface.dirty().affects_draw());
        assert_eq!(surface.layout(180.0, 80.0), oxide_ui_core::LayoutStats::default());
        assert_eq!(surface.tree().layout_rect(leaf), Some(leaf_layout));
        assert_eq!(surface.tree().layout_rect(sibling), Some(sibling_layout));

        let mut dirty = DrawListBuilder::new();
        assert_eq!(surface.encode_retained(&mut dirty), RetainedDrawStatus::Reused);
        assert_eq!(dirty.drawlist().items, warm_draws);
        let stats = surface.retained_node_stats();
        assert_eq!(
            stats.rebuilt_nodes, 0,
            "non-draw dirty class should not rebuild, got {stats:?}"
        );
        assert_eq!(stats.reused_nodes, 1, "non-draw dirty class should reuse cached draw list");
    }
    assert!(!surface.mark_node_dirty(NodeId(99), DirtyClass::Accessibility));
}

#[test]
fn layout_dirty_subtree_skips_clean_sibling_subtree() {
    let mut surface = UiSurface::new(NodeStyle {
        axis: Axis::Row,
        size: Size2D { w: Dim::Px(140.0), h: Dim::Px(80.0) },
        gap: 4.0,
        ..NodeStyle::default()
    });
    let root = surface.root();
    let left = surface.tree_mut().add_node(
        root,
        NodeStyle {
            axis: Axis::Column,
            size: Size2D { w: Dim::Px(48.0), h: Dim::Px(60.0) },
            ..NodeStyle::default()
        },
    );
    let left_leaf = surface.tree_mut().add_node(
        left,
        NodeStyle { size: Size2D { w: Dim::Px(20.0), h: Dim::Px(20.0) }, ..NodeStyle::default() },
    );
    let right = surface.tree_mut().add_node(
        root,
        NodeStyle {
            axis: Axis::Column,
            size: Size2D { w: Dim::Px(48.0), h: Dim::Px(60.0) },
            ..NodeStyle::default()
        },
    );
    let right_leaf = surface.tree_mut().add_node(
        right,
        NodeStyle { size: Size2D { w: Dim::Px(20.0), h: Dim::Px(20.0) }, ..NodeStyle::default() },
    );

    let cold = surface.layout(140.0, 80.0);
    assert_eq!(cold.skipped_subtrees, 0);
    assert!(cold.visited_nodes >= 5, "cold layout should visit full tree, got {cold:?}");

    assert!(surface.edit_style(right_leaf, |style| {
        style.size.w = Dim::Px(24.0);
    }));
    let dirty = surface.layout(140.0, 80.0);
    assert!(dirty.skipped_subtrees >= 1, "clean sibling subtree should skip, got {dirty:?}");
    assert!(
        dirty.visited_nodes < cold.visited_nodes,
        "dirty relayout should visit fewer nodes than cold layout, cold={cold:?} dirty={dirty:?}",
    );
    assert_eq!(dirty.visited_nodes, 3);
    assert!(dirty.measured_children < cold.measured_children);

    let left_layout = surface.tree().layout_rect(left_leaf).unwrap();
    let right_layout = surface.tree().layout_rect(right_leaf).unwrap();
    assert_eq!(left_layout.w, 20.0);
    assert_eq!(right_layout.w, 24.0);
}

#[test]
fn descendant_only_layout_dirty_skips_parent_measurement() {
    let mut surface = UiSurface::new(NodeStyle {
        axis: Axis::Row,
        size: Size2D { w: Dim::Px(140.0), h: Dim::Px(80.0) },
        gap: 4.0,
        ..NodeStyle::default()
    });
    let root = surface.root();
    let left = surface.tree_mut().add_node(
        root,
        NodeStyle {
            axis: Axis::Column,
            size: Size2D { w: Dim::Px(48.0), h: Dim::Px(60.0) },
            ..NodeStyle::default()
        },
    );
    let left_leaf = surface.tree_mut().add_node(
        left,
        NodeStyle { size: Size2D { w: Dim::Px(20.0), h: Dim::Px(20.0) }, ..NodeStyle::default() },
    );
    let right = surface.tree_mut().add_node(
        root,
        NodeStyle {
            axis: Axis::Column,
            size: Size2D { w: Dim::Px(48.0), h: Dim::Px(60.0) },
            ..NodeStyle::default()
        },
    );
    let right_leaf = surface.tree_mut().add_node(
        right,
        NodeStyle { size: Size2D { w: Dim::Px(20.0), h: Dim::Px(20.0) }, ..NodeStyle::default() },
    );

    let cold = surface.layout(140.0, 80.0);
    let left_before = surface.tree().layout_rect(left_leaf).unwrap();
    let right_before = surface.tree().layout_rect(right).unwrap();
    let right_leaf_before = surface.tree().layout_rect(right_leaf).unwrap();

    assert!(surface.edit_style(right, |style| {
        style.padding = Edges { left: 8.0, top: 0.0, right: 0.0, bottom: 0.0 };
    }));
    let dirty = surface.layout(140.0, 80.0);

    assert_eq!(surface.tree().layout_rect(left_leaf).unwrap(), left_before);
    assert_eq!(surface.tree().layout_rect(right).unwrap(), right_before);
    assert_eq!(surface.tree().layout_rect(right_leaf).unwrap().x, right_leaf_before.x + 8.0);
    assert_eq!(dirty.visited_nodes, 3);
    assert_eq!(dirty.measured_children, 1);
    assert!(
        dirty.measured_children < cold.measured_children,
        "descendant-only relayout should avoid parent child-measure scans, cold={cold:?} dirty={dirty:?}",
    );
    assert!(dirty.skipped_subtrees >= 1, "clean sibling should skip, got {dirty:?}");
}

#[test]
fn ancestor_relayout_does_not_skip_dirty_descendant_with_stable_child_rect() {
    let mut surface = UiSurface::new(NodeStyle {
        axis: Axis::Row,
        size: Size2D { w: Dim::Px(180.0), h: Dim::Px(90.0) },
        gap: 4.0,
        ..NodeStyle::default()
    });
    let root = surface.root();
    let left = surface.tree_mut().add_node(
        root,
        NodeStyle {
            axis: Axis::Column,
            size: Size2D { w: Dim::Px(48.0), h: Dim::Px(60.0) },
            ..NodeStyle::default()
        },
    );
    let _left_leaf = surface.tree_mut().add_node(
        left,
        NodeStyle { size: Size2D { w: Dim::Px(20.0), h: Dim::Px(20.0) }, ..NodeStyle::default() },
    );
    let right = surface.tree_mut().add_node(
        root,
        NodeStyle {
            axis: Axis::Column,
            size: Size2D { w: Dim::Px(64.0), h: Dim::Px(60.0) },
            ..NodeStyle::default()
        },
    );
    let mid = surface.tree_mut().add_node(
        right,
        NodeStyle {
            axis: Axis::Column,
            size: Size2D { w: Dim::Px(44.0), h: Dim::Px(44.0) },
            ..NodeStyle::default()
        },
    );
    let grandchild = surface.tree_mut().add_node(
        mid,
        NodeStyle { size: Size2D { w: Dim::Px(12.0), h: Dim::Px(12.0) }, ..NodeStyle::default() },
    );

    let cold = surface.layout(180.0, 90.0);
    let right_before = surface.tree().layout_rect(right).unwrap();
    let grandchild_before = surface.tree().layout_rect(grandchild).unwrap();

    assert!(surface.edit_style(mid, |style| {
        style.padding.left = 7.0;
    }));
    surface.mark_dirty(DirtyClass::Layout);
    let dirty = surface.layout(180.0, 90.0);

    assert_eq!(surface.tree().layout_rect(right).unwrap(), right_before);
    assert_eq!(surface.tree().layout_rect(grandchild).unwrap().x, grandchild_before.x + 7.0);
    assert!(
        dirty.visited_nodes < cold.visited_nodes,
        "mixed ancestor/descendant relayout should still skip clean siblings, cold={cold:?} dirty={dirty:?}",
    );
    assert!(dirty.skipped_subtrees >= 1, "clean sibling should skip, got {dirty:?}");
}

#[test]
fn scoped_tree_add_remove_skips_clean_sibling_layout_and_reuses_retained_draws() {
    let mut surface = UiSurface::new(NodeStyle {
        axis: Axis::Row,
        size: Size2D { w: Dim::Px(220.0), h: Dim::Px(100.0) },
        gap: 4.0,
        ..NodeStyle::default()
    });
    let root = surface.root();
    let left = surface
        .add_node(
            root,
            NodeStyle {
                axis: Axis::Column,
                size: Size2D { w: Dim::Px(80.0), h: Dim::Px(80.0) },
                background: gfx::Color::rgba(0.2, 0.3, 0.4, 1.0),
                ..NodeStyle::default()
            },
        )
        .expect("add left branch");
    let left_leaf = surface
        .add_node(
            left,
            NodeStyle {
                size: Size2D { w: Dim::Px(36.0), h: Dim::Px(20.0) },
                background: gfx::Color::rgba(0.7, 0.2, 0.2, 1.0),
                ..NodeStyle::default()
            },
        )
        .expect("add left leaf");
    let right = surface
        .add_node(
            root,
            NodeStyle {
                axis: Axis::Column,
                size: Size2D { w: Dim::Px(80.0), h: Dim::Px(80.0) },
                background: gfx::Color::rgba(0.2, 0.5, 0.3, 1.0),
                ..NodeStyle::default()
            },
        )
        .expect("add right branch");
    let right_leaf = surface
        .add_node(
            right,
            NodeStyle {
                size: Size2D { w: Dim::Px(36.0), h: Dim::Px(20.0) },
                background: gfx::Color::rgba(0.2, 0.2, 0.8, 1.0),
                ..NodeStyle::default()
            },
        )
        .expect("add right leaf");

    let cold = surface.layout(220.0, 100.0);
    let left_before = surface.tree().layout_rect(left_leaf).unwrap();
    let right_before = surface.tree().layout_rect(right_leaf).unwrap();
    let mut warm = DrawListBuilder::new();
    assert_eq!(surface.encode_retained(&mut warm), RetainedDrawStatus::Rebuilt);

    let inserted = surface
        .add_node(
            right,
            NodeStyle {
                size: Size2D { w: Dim::Px(36.0), h: Dim::Px(20.0) },
                background: gfx::Color::rgba(0.8, 0.7, 0.2, 1.0),
                ..NodeStyle::default()
            },
        )
        .expect("scoped add");
    assert!(surface.dirty().contains(DirtyClass::Layout));
    let add_stats = surface.layout(220.0, 100.0);
    assert_eq!(surface.tree().layout_rect(left_leaf), Some(left_before));
    assert_eq!(surface.tree().layout_rect(right_leaf), Some(right_before));
    assert!(
        add_stats.visited_nodes < cold.visited_nodes,
        "add should be incremental: cold={cold:?} add={add_stats:?}"
    );
    assert!(add_stats.skipped_subtrees >= 1, "clean left branch should skip: {add_stats:?}");

    let mut added_draws = DrawListBuilder::new();
    assert_eq!(surface.encode_retained(&mut added_draws), RetainedDrawStatus::Rebuilt);
    assert!(surface.retained_node_stats().reused_nodes >= 1);

    assert!(surface.remove_node(inserted));
    assert!(surface.tree().style(inserted).is_none());
    let remove_stats = surface.layout(220.0, 100.0);
    assert_eq!(surface.tree().layout_rect(left_leaf), Some(left_before));
    assert_eq!(surface.tree().layout_rect(right_leaf), Some(right_before));
    assert!(
        remove_stats.visited_nodes < cold.visited_nodes,
        "remove should be incremental: cold={cold:?} remove={remove_stats:?}"
    );
    assert!(remove_stats.skipped_subtrees >= 1, "clean left branch should skip: {remove_stats:?}");

    let mut removed_draws = DrawListBuilder::new();
    assert_eq!(surface.encode_retained(&mut removed_draws), RetainedDrawStatus::Rebuilt);
    assert!(surface.retained_node_stats().reused_nodes >= 1);
    assert!(!surface.remove_node(NodeId(99)));
    assert!(!surface.remove_node(surface.root()));
    assert!(surface.add_node(NodeId(99), NodeStyle::default()).is_none());
}

#[test]
fn tree_mut_marks_retained_surface_dirty_until_layout_and_encode() {
    let mut surface = UiSurface::new(NodeStyle {
        size: Size2D { w: oxide_ui_core::Dim::Px(120.0), h: oxide_ui_core::Dim::Px(120.0) },
        ..NodeStyle::default()
    });
    surface.layout(120.0, 120.0);
    let mut first = DrawListBuilder::new();
    assert_eq!(surface.encode_retained(&mut first), RetainedDrawStatus::Rebuilt);
    assert!(!surface.dirty().affects_draw());

    let root = surface.root();
    surface.tree_mut().style_mut(root).unwrap().background = gfx::Color::rgba(0.8, 0.1, 0.1, 1.0);
    assert!(surface.dirty().contains(DirtyClass::Layout));
    assert!(surface.dirty().contains(DirtyClass::Paint));

    surface.layout(120.0, 120.0);
    assert!(!surface.dirty().contains(DirtyClass::Layout));
    assert!(surface.dirty().contains(DirtyClass::Paint));
    let mut second = DrawListBuilder::new();
    assert_eq!(surface.encode_retained(&mut second), RetainedDrawStatus::Rebuilt);
    assert!(!surface.dirty().affects_draw());
}

#[test]
fn router_encode_with_overlays_uses_retained_current_surface() {
    let mut surface = UiSurface::new(NodeStyle {
        size: Size2D { w: oxide_ui_core::Dim::Px(120.0), h: oxide_ui_core::Dim::Px(120.0) },
        background: gfx::Color::rgba(0.16, 0.20, 0.26, 1.0),
        ..NodeStyle::default()
    });
    surface.layout(120.0, 120.0);
    let viewport = gfx::RectF::new(0.0, 0.0, 120.0, 120.0);
    let mut router = SurfaceRouter::new(surface);

    let mut first = DrawListBuilder::new();
    router.encode_with_overlays(viewport, 1.0, &mut first);
    assert!(!router.current().dirty().affects_draw());
    let first_items = first.drawlist().items.clone();

    let mut second = DrawListBuilder::new();
    router.encode_with_overlays(viewport, 1.0, &mut second);
    assert_eq!(second.drawlist().items, first_items);

    router.current_mut().mark_dirty(DirtyClass::Paint);
    let mut third = DrawListBuilder::new();
    router.encode_with_overlays(viewport, 1.0, &mut third);
    assert_eq!(third.drawlist().items, first_items);
    assert!(!router.current().dirty().affects_draw());
}

#[test]
fn router_encode_with_overlays_reuses_clean_overlay_and_popup_surfaces() {
    let mut surface = UiSurface::new(NodeStyle {
        size: Size2D { w: oxide_ui_core::Dim::Px(120.0), h: oxide_ui_core::Dim::Px(120.0) },
        background: gfx::Color::rgba(0.16, 0.20, 0.26, 1.0),
        ..NodeStyle::default()
    });
    surface.layout(120.0, 120.0);
    let mut overlay_surface = UiSurface::new(NodeStyle {
        size: Size2D { w: oxide_ui_core::Dim::Px(120.0), h: oxide_ui_core::Dim::Px(120.0) },
        background: gfx::Color::rgba(0.42, 0.16, 0.22, 0.88),
        ..NodeStyle::default()
    });
    overlay_surface.layout(120.0, 120.0);
    let mut popup_surface = UiSurface::new(NodeStyle {
        size: Size2D { w: oxide_ui_core::Dim::Px(80.0), h: oxide_ui_core::Dim::Px(80.0) },
        background: gfx::Color::rgba(0.18, 0.36, 0.62, 0.94),
        ..NodeStyle::default()
    });
    popup_surface.layout(120.0, 120.0);
    let viewport = gfx::RectF::new(0.0, 0.0, 120.0, 120.0);
    let mut router = SurfaceRouter::new(surface);
    let overlay = router.overlays_mut().push(
        overlay_surface,
        OverlayVisual::default(),
        OverlayBehavior::default(),
    );
    router.popups_mut().push(
        popup_surface,
        PopupSpec {
            visual: OverlayVisual { z_index: 1, ..OverlayVisual::default() },
            ..PopupSpec::default()
        },
    );

    let mut first = DrawListBuilder::new();
    router.encode_with_overlays(viewport, 1.0, &mut first);
    let stats = router.retained_composition_stats();
    assert_eq!(stats.current_rebuilt, 1);
    assert_eq!(stats.overlay_rebuilt, 1);
    assert_eq!(stats.popup_rebuilt, 1);

    let mut second = DrawListBuilder::new();
    router.encode_with_overlays(viewport, 1.0, &mut second);
    let stats = router.retained_composition_stats();
    assert_eq!(stats.current_reused, 1);
    assert_eq!(stats.overlay_reused, 1);
    assert_eq!(stats.popup_reused, 1);
    assert_eq!(second.drawlist().items, first.drawlist().items);

    router.overlays_mut().surface_mut(overlay).unwrap().mark_dirty(DirtyClass::Paint);
    let mut third = DrawListBuilder::new();
    router.encode_with_overlays(viewport, 1.0, &mut third);
    let stats = router.retained_composition_stats();
    assert_eq!(stats.current_reused, 1);
    assert_eq!(stats.overlay_rebuilt, 1);
    assert_eq!(stats.popup_reused, 1);
}

#[test]
fn router_encode_with_overlays_accepts_text_atlas_context_path() {
    let mut surface = UiSurface::new(NodeStyle {
        size: Size2D { w: oxide_ui_core::Dim::Px(120.0), h: oxide_ui_core::Dim::Px(120.0) },
        background: gfx::Color::rgba(0.16, 0.20, 0.26, 1.0),
        ..NodeStyle::default()
    });
    surface.layout(120.0, 120.0);
    let viewport = gfx::RectF::new(0.0, 0.0, 120.0, 120.0);
    let atlases = [(gfx::ImageHandle(4), 3)];
    let mut router = SurfaceRouter::new(surface);

    let mut first = DrawListBuilder::new();
    router.encode_with_overlays_with_text_atlas_revisions(viewport, 1.0, &mut first, &atlases);
    assert!(!router.current().dirty().affects_draw());
    let first_items = first.drawlist().items.clone();

    let mut second = DrawListBuilder::new();
    router.encode_with_overlays_with_text_atlas_revisions(viewport, 1.0, &mut second, &atlases);
    assert_eq!(second.drawlist().items, first_items);
}

#[test]
fn router_encode_with_overlays_uses_clean_text_ctx_atlas_snapshot() {
    let mut surface = UiSurface::new(NodeStyle {
        size: Size2D { w: oxide_ui_core::Dim::Px(120.0), h: oxide_ui_core::Dim::Px(120.0) },
        background: gfx::Color::rgba(0.16, 0.20, 0.26, 1.0),
        ..NodeStyle::default()
    });
    surface.layout(120.0, 120.0);
    let viewport = gfx::RectF::new(0.0, 0.0, 120.0, 120.0);
    let mut text = TextCtx::default();
    text.atlas_handle = Some(gfx::ImageHandle(4));
    let mut router = SurfaceRouter::new(surface);

    let mut first = DrawListBuilder::new();
    router.encode_with_overlays_with_text_ctx(viewport, 1.0, &mut first, &text);
    assert!(!router.current().dirty().affects_draw());
    let first_items = first.drawlist().items.clone();

    let mut second = DrawListBuilder::new();
    router.encode_with_overlays_with_text_ctx(viewport, 1.0, &mut second, &text);
    assert_eq!(second.drawlist().items, first_items);
}

#[test]
fn scatter_blocks_and_releases_gate() {
    timing::testing::reset();
    let mut surface = UiSurface::new(NodeStyle::default());
    let root = surface.root();
    let child = surface.tree_mut().add_node(
        root,
        NodeStyle {
            size: Size2D { w: oxide_ui_core::Dim::Px(50.0), h: oxide_ui_core::Dim::Px(50.0) },
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
        size: Size2D { w: oxide_ui_core::Dim::Px(120.0), h: oxide_ui_core::Dim::Px(120.0) },
        ..NodeStyle::default()
    });
    let mut surface_b = UiSurface::new(NodeStyle {
        size: Size2D { w: oxide_ui_core::Dim::Px(120.0), h: oxide_ui_core::Dim::Px(120.0) },
        ..NodeStyle::default()
    });
    let child_style = NodeStyle {
        size: Size2D { w: oxide_ui_core::Dim::Px(100.0), h: oxide_ui_core::Dim::Px(100.0) },
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
