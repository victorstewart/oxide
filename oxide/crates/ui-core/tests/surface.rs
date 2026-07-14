use oxide_platform_api::Transform2D;
use oxide_renderer_api as gfx;
use oxide_timing as timing;
use oxide_ui_core::{
    elements::TextCtx, Axis, ChromeMetrics, Dim, DirtyClass, DrawListBuilder, Edges, LayoutRect,
    NodeId, NodeStyle, OverlayBehavior, OverlayVisual, PopupSpec, RetainedCachePolicy,
    RetainedDrawStatus, RetainedInvalidationReason, ScatterSpec, Size2D, SurfaceFrameDemand,
    SurfaceRouter, UiSurface,
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
    assert_eq!(surface.encode_retained(&mut opacity_draws), RetainedDrawStatus::Reused);
    let opacity_stats = surface.retained_node_stats();
    assert_eq!(opacity_stats.chunks_rebuilt, 0, "opacity is a dynamic property: {opacity_stats:?}");
    assert_eq!(opacity_stats.sequences_rebuilt, 0, "opacity keeps retained metadata: {opacity_stats:?}");
    assert_eq!(opacity_stats.chunks_reused, 4, "every node chunk should replay: {opacity_stats:?}");

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
    assert_eq!(clip_stats.chunks_rebuilt, 0, "clip metadata must not rebake geometry: {clip_stats:?}");
    assert_eq!(clip_stats.chunks_reused, 4, "all clipped geometry should replay: {clip_stats:?}");
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

#[test]
fn retained_render_sequence_rebuilds_only_dirty_leaf_and_matches_flat_output()
{
   let mut surface = UiSurface::new(NodeStyle {
      size: Size2D { w: Dim::Px(120.0), h: Dim::Px(120.0) },
      background: gfx::Color::rgba(0.2, 0.3, 0.4, 1.0),
      ..NodeStyle::default()
   });
   let root = surface.root();
   let leaf = surface.tree_mut().add_node(root, NodeStyle {
      size: Size2D { w: Dim::Px(40.0), h: Dim::Px(30.0) },
      background: gfx::Color::rgba(0.8, 0.2, 0.1, 1.0),
      ..NodeStyle::default()
   });
   surface.layout(120.0, 120.0);

   let first = surface.render_snapshot_retained(
      gfx::RenderChunkId(11),
      &[],
      Vec::new(),
      gfx::Damage { rects: Vec::new() },
   ).unwrap();
   assert_eq!(first.stats.status, RetainedDrawStatus::Rebuilt);
   assert_eq!(first.stats.chunks_rebuilt, 2);
   let root_chunk = first.snapshot.instance(0).unwrap().chunk.clone();
   let leaf_chunk = first.snapshot.instance(1).unwrap().chunk.clone();

   let clean = surface.render_snapshot_retained(
      gfx::RenderChunkId(11),
      &[],
      Vec::new(),
      gfx::Damage { rects: Vec::new() },
   ).unwrap();
   assert_eq!(clean.stats.status, RetainedDrawStatus::Reused);
   assert_eq!(clean.stats.chunks_reused, 2);
   assert_eq!(clean.stats.chunks_rebuilt, 0);
   assert_eq!(clean.stats.sequences_rebuilt, 0);
   assert_eq!(clean.stats.command_bytes_copied, 0);
   assert_eq!(clean.stats.vertex_bytes_copied, 0);
   assert_eq!(clean.stats.index_bytes_copied, 0);
   assert!(root_chunk.ptr_eq(&clean.snapshot.instance(0).unwrap().chunk));
   assert!(leaf_chunk.ptr_eq(&clean.snapshot.instance(1).unwrap().chunk));

   surface.edit_style(leaf, |style| {
      style.background = gfx::Color::rgba(0.1, 0.7, 0.3, 1.0);
   });
   let dirty = surface.render_snapshot_retained(
      gfx::RenderChunkId(11),
      &[],
      Vec::new(),
      gfx::Damage { rects: Vec::new() },
   ).unwrap();
   assert_eq!(dirty.stats.status, RetainedDrawStatus::Rebuilt);
   assert_eq!(dirty.stats.chunks_rebuilt, 1);
   assert!(root_chunk.ptr_eq(&dirty.snapshot.instance(0).unwrap().chunk));
   assert!(!leaf_chunk.ptr_eq(&dirty.snapshot.instance(1).unwrap().chunk));
   assert_eq!(dirty.stats.vertex_bytes_copied, 0);
   assert_eq!(dirty.stats.index_bytes_copied, 0);

   let mut compatibility = DrawListBuilder::new();
   let fallback = compatibility.append_render_snapshot_flat(&dirty.snapshot).unwrap();
   let mut direct = DrawListBuilder::new();
   surface.encode(&mut direct);
   assert_eq!(compatibility.drawlist(), direct.drawlist());
   assert_eq!(fallback.fallback_count, 1);
   assert_eq!(fallback.commands_copied, direct.drawlist().items.len() as u64);

   let namespaced = surface.render_snapshot_retained(
      gfx::RenderChunkId(13),
      &[],
      Vec::new(),
      gfx::Damage { rects: Vec::new() },
   ).unwrap();
   assert_eq!(namespaced.stats.chunks_rebuilt, 2);
   assert_eq!(
      namespaced.snapshot.instance(0).unwrap().chunk.id(),
      gfx::RenderChunkId((13_u64 << 32) | u64::from(root.0)),
   );
}

#[test]
fn retained_cache_enforces_hard_bytes_and_preserves_exact_output()
{
   let mut surface = UiSurface::new(NodeStyle {
      axis: Axis::Row,
      size: Size2D { w: Dim::Px(640.0), h: Dim::Px(80.0) },
      ..NodeStyle::default()
   });
   let root = surface.root();
   for index in 0..32
   {
      surface.tree_mut().add_node(root, NodeStyle {
         size: Size2D { w: Dim::Px(18.0), h: Dim::Px(18.0) },
         background: gfx::Color::rgba(0.1 + index as f32 * 0.01, 0.4, 0.7, 1.0),
         ..NodeStyle::default()
      });
   }
   surface.layout(640.0, 80.0);
   let policy = RetainedCachePolicy {
      cpu_budget_bytes: 512,
      churn_retry_generations: 4,
      ..RetainedCachePolicy::default()
   };
   surface.set_retained_cache_policy(policy);
   let rendered = surface.render_snapshot_retained(
      gfx::RenderChunkId(70),
      &[],
      Vec::new(),
      gfx::Damage { rects: Vec::new() },
   ).unwrap();
   let stats = surface.retained_node_stats();
   assert!(stats.cache_evictions > 0);
   assert!(!stats.cache_complete);
   assert_eq!(stats.last_invalidation_reason, RetainedInvalidationReason::Budget);
   assert!(
      rendered.stats.retained_bytes.saturating_add(rendered.stats.retained_sequence_bytes)
         <= policy.cpu_budget_bytes,
   );
   assert_eq!(stats.prepared_gpu_bytes, 0);

   let mut retained = DrawListBuilder::new();
   retained.append_render_snapshot_flat(&rendered.snapshot).unwrap();
   let mut direct = DrawListBuilder::new();
   surface.encode(&mut direct);
   assert_eq!(retained.drawlist(), direct.drawlist());
}

#[test]
fn retained_cache_suppresses_churn_then_readmits_stable_nodes()
{
   let mut surface = UiSurface::new(NodeStyle {
      size: Size2D { w: Dim::Px(100.0), h: Dim::Px(100.0) },
      ..NodeStyle::default()
   });
   let root = surface.root();
   let leaf = surface.tree_mut().add_node(root, NodeStyle {
      size: Size2D { w: Dim::Px(40.0), h: Dim::Px(40.0) },
      ..NodeStyle::default()
   });
   surface.layout(100.0, 100.0);
   surface.set_retained_cache_policy(RetainedCachePolicy {
      churn_invalidation_threshold: 2,
      churn_retry_generations: 4,
      ..RetainedCachePolicy::default()
   });
   let render = |surface: &mut UiSurface| {
      surface.render_snapshot_retained(
         gfx::RenderChunkId(71),
         &[],
         Vec::new(),
         gfx::Damage { rects: Vec::new() },
      ).unwrap()
   };
   let _ = render(&mut surface);
   let mut rejected = false;
   for phase in 0..3
   {
      surface.edit_style(leaf, |style| {
         style.background = gfx::Color::rgba(0.2 + phase as f32 * 0.1, 0.3, 0.4, 1.0);
      });
      let _ = render(&mut surface);
      rejected |= surface.retained_node_stats().cache_admission_rejections > 0;
   }
   assert!(rejected);

   let mut stable = None;
   for _ in 0..8
   {
      stable = Some(render(&mut surface));
   }
   let stable = stable.unwrap();
   let stable_stats = surface.retained_node_stats();
   assert!(stable_stats.cache_complete);
   assert_eq!(stable.stats.chunks_rebuilt, 0);
   assert!(stable_stats.cache_hits >= 2);
}

#[test]
fn retained_cache_lru_protects_hot_root_while_evicting_cold_children()
{
   let mut surface = UiSurface::new(NodeStyle {
      axis: Axis::Row,
      size: Size2D { w: Dim::Px(200.0), h: Dim::Px(60.0) },
      ..NodeStyle::default()
   });
   let root = surface.root();
   for _ in 0..6
   {
      surface.tree_mut().add_node(root, NodeStyle {
         size: Size2D { w: Dim::Px(24.0), h: Dim::Px(24.0) },
         ..NodeStyle::default()
      });
   }
   surface.layout(200.0, 60.0);
   let render = |surface: &mut UiSurface| {
      surface.render_snapshot_retained(
         gfx::RenderChunkId(75),
         &[],
         Vec::new(),
         gfx::Damage { rects: Vec::new() },
      ).unwrap()
   };
   let first = render(&mut surface);
   let root_chunk = first.snapshot.instance(0).unwrap().chunk.clone();
   let _ = render(&mut surface);
   let hot = render(&mut surface);
   let retained = hot.stats.retained_bytes.saturating_add(hot.stats.retained_sequence_bytes);
   surface.set_retained_cache_policy(RetainedCachePolicy {
      cpu_budget_bytes: retained / 2,
      hot_hit_threshold: 2,
      hot_generation_window: 8,
      ..RetainedCachePolicy::default()
   });
   let pressured = render(&mut surface);
   assert!(surface.retained_node_stats().cache_evictions > 0);
   assert!(root_chunk.ptr_eq(&pressured.snapshot.instance(0).unwrap().chunk));
   assert!(pressured.stats.chunks_rebuilt > 0);
}

#[test]
fn retained_cache_budget_never_evicts_caller_owned_text_or_image_chunks()
{
   let mut surface = UiSurface::new(NodeStyle {
      size: Size2D { w: Dim::Px(80.0), h: Dim::Px(80.0) },
      ..NodeStyle::default()
   });
   surface.layout(80.0, 80.0);
   surface.set_retained_cache_policy(RetainedCachePolicy {
      cpu_budget_bytes: 0,
      ..RetainedCachePolicy::default()
   });
   let text = gfx::RenderChunk::new(
      gfx::RenderChunkId(72),
      gfx::RenderChunkRevisions::default(),
      gfx::DrawList {
         items: vec![gfx::DrawCmd::RRect {
            rect: gfx::RectF::new(0.0, 0.0, 12.0, 12.0),
            radii: [1.0; 4],
            color: gfx::Color::rgba(0.9, 0.9, 0.9, 1.0),
         }],
         vertices: Vec::new(),
         indices: Vec::new(),
      },
      gfx::ChunkIndexMode::Local,
      &[],
   ).unwrap();
   let image = gfx::RenderChunk::new(
      gfx::RenderChunkId(73),
      gfx::RenderChunkRevisions::default(),
      gfx::DrawList {
         items: vec![gfx::DrawCmd::Image {
            tex: gfx::ImageHandle(9),
            dst: gfx::RectF::new(0.0, 0.0, 16.0, 16.0),
            src: gfx::RectF::new(0.0, 0.0, 1.0, 1.0),
            alpha: 1.0,
         }],
         vertices: Vec::new(),
         indices: Vec::new(),
      },
      gfx::ChunkIndexMode::Local,
      &[gfx::RenderResourceDependency { image: gfx::ImageHandle(9), generation: 4 }],
   ).unwrap();
   let content = vec![
      gfx::RenderChunkSequence::new(vec![gfx::RenderChunkInstance::new(text.clone(), [0.0, 0.0])]),
      gfx::RenderChunkSequence::new(vec![gfx::RenderChunkInstance::new(image.clone(), [16.0, 0.0])]),
   ];
   let rendered = surface.render_snapshot_retained(
      gfx::RenderChunkId(74),
      &content,
      Vec::new(),
      gfx::Damage { rects: Vec::new() },
   ).unwrap();
   let stats = surface.retained_node_stats();
   assert_eq!(rendered.stats.retained_bytes, 0);
   assert_eq!(rendered.stats.retained_sequence_bytes, 0);
   assert_eq!(stats.flat_fallback_uses, 1);
   assert!(text.ptr_eq(&rendered.snapshot.instance(1).unwrap().chunk));
   assert!(image.ptr_eq(&rendered.snapshot.instance(2).unwrap().chunk));

   let mut retained = DrawListBuilder::new();
   retained.append_render_snapshot_flat(&rendered.snapshot).unwrap();
   let mut direct = DrawListBuilder::new();
   surface.encode(&mut direct);
   let external = gfx::RenderSnapshot::from_sequences(
      content,
      Vec::new(),
      gfx::Damage { rects: Vec::new() },
   ).unwrap();
   direct.append_render_snapshot_flat(&external).unwrap();
   assert_eq!(retained.drawlist(), direct.drawlist());
}

#[test]
fn retained_snapshot_applies_animation_overrides_and_matches_direct_output()
{
   let mut surface = UiSurface::new(NodeStyle {
      size: Size2D { w: Dim::Px(100.0), h: Dim::Px(100.0) },
      ..NodeStyle::default()
   });
   let root = surface.root();
   let leaf = surface.tree_mut().add_node(root, NodeStyle {
      size: Size2D { w: Dim::Px(30.0), h: Dim::Px(20.0) },
      background: gfx::Color::rgba(0.8, 0.2, 0.1, 1.0),
      ..NodeStyle::default()
   });
   surface.layout(100.0, 100.0);
   surface.overrides_mut().insert(leaf, oxide_ui_core::anim::AnimOverrides {
      opacity: Some(0.5),
      transform: Some(Transform2D { tx: 7.0, ty: 9.0, sx: 1.0, sy: 1.0, rot_rad: 0.0 }),
      ..oxide_ui_core::anim::AnimOverrides::default()
   });
   let rendered = surface.render_snapshot_retained(
      gfx::RenderChunkId(12),
      &[],
      Vec::new(),
      gfx::Damage { rects: Vec::new() },
   ).unwrap();
   let mut retained = DrawListBuilder::new();
   retained.append_render_snapshot_flat(&rendered.snapshot).unwrap();
   let mut direct = DrawListBuilder::new();
   surface.encode(&mut direct);
   assert_eq!(retained.drawlist(), direct.drawlist());
}

#[test]
fn transform_and_opacity_animation_reuses_all_warm_geometry()
{
   let mut surface = UiSurface::new(NodeStyle {
      axis: Axis::Row,
      size: Size2D { w: Dim::Px(600.0), h: Dim::Px(600.0) },
      ..NodeStyle::default()
   });
   let root = surface.root();
   let mut nodes = Vec::new();
   for index in 0..300
   {
      nodes.push(surface.tree_mut().add_node(root, NodeStyle {
         size: Size2D { w: Dim::Px(20.0), h: Dim::Px(20.0) },
         background: gfx::Color::rgba(0.1 + index as f32 / 2_000.0, 0.3, 0.8, 1.0),
         ..NodeStyle::default()
      }));
   }
   surface.layout(600.0, 600.0);
   let warm = surface.render_snapshot_retained(
      gfx::RenderChunkId(21),
      &[],
      Vec::new(),
      gfx::Damage { rects: Vec::new() },
   ).unwrap();
   assert_eq!(warm.stats.chunks_rebuilt, 301);
   for (index, node) in nodes.iter().copied().enumerate()
   {
      surface.animator().start(node, oxide_platform_api::AnimDesc {
         id: 0,
         prop: oxide_platform_api::AnimProp::Transform2D,
         from: oxide_platform_api::AnimValue::Xform2D(Transform2D {
            tx: 0.0,
            ty: 0.0,
            sx: 1.0,
            sy: 1.0,
            rot_rad: 0.0,
         }),
         to: oxide_platform_api::AnimValue::Xform2D(Transform2D {
            tx: (index % 7) as f32,
            ty: (index % 5) as f32,
            sx: 1.1,
            sy: 0.9,
            rot_rad: 0.2,
         }),
         curve: oxide_platform_api::AnimCurve::Ease {
            ease: oxide_platform_api::Ease { kind: oxide_platform_api::EaseKind::Linear },
         },
         duration_ms: 1_000,
         delay_ms: 0,
         repeat: oxide_platform_api::Repeat::Forever,
      });
      surface.animator().start(node, oxide_platform_api::AnimDesc {
         id: 0,
         prop: oxide_platform_api::AnimProp::Opacity,
         from: oxide_platform_api::AnimValue::F32(0.4),
         to: oxide_platform_api::AnimValue::F32(1.0),
         curve: oxide_platform_api::AnimCurve::Ease {
            ease: oxide_platform_api::Ease { kind: oxide_platform_api::EaseKind::Linear },
         },
         duration_ms: 1_000,
         delay_ms: 0,
         repeat: oxide_platform_api::Repeat::Forever,
      });
   }
   let now = oxide_timing::now_ms();
   assert!(surface.tick_at(now.saturating_add(500)));
   let animated = surface.render_snapshot_retained(
      gfx::RenderChunkId(21),
      &[],
      Vec::new(),
      gfx::Damage { rects: Vec::new() },
   ).unwrap();
   assert_eq!(animated.stats.chunks_rebuilt, 0);
   assert_eq!(animated.stats.sequences_rebuilt, 0);
   assert_eq!(animated.stats.command_bytes_copied, 0);
   assert_eq!(animated.stats.vertex_bytes_copied, 0);
   assert_eq!(animated.stats.index_bytes_copied, 0);
   assert_eq!(animated.snapshot.properties().len(), 602);
}

#[test]
fn nested_animation_keeps_clip_hit_test_and_accessibility_geometry_synchronized()
{
   let mut surface = UiSurface::new(NodeStyle {
      size: Size2D { w: Dim::Px(240.0), h: Dim::Px(240.0) },
      ..NodeStyle::default()
   });
   let root = surface.root();
   let parent = surface.tree_mut().add_node(root, NodeStyle {
      size: Size2D { w: Dim::Px(120.0), h: Dim::Px(120.0) },
      padding: oxide_ui_core::Edges { left: 20.0, top: 20.0, right: 0.0, bottom: 0.0 },
      clip: true,
      ..NodeStyle::default()
   });
   let leaf = surface.tree_mut().add_node(parent, NodeStyle {
      size: Size2D { w: Dim::Px(40.0), h: Dim::Px(30.0) },
      background: gfx::Color::rgba(0.9, 0.2, 0.1, 1.0),
      ..NodeStyle::default()
   });
   surface.layout(240.0, 240.0);
   surface.overrides_mut().insert(parent, oxide_ui_core::anim::AnimOverrides {
      opacity: Some(0.5),
      transform: Some(Transform2D { tx: 31.0, ty: 17.0, sx: 1.25, sy: 0.8, rot_rad: 0.3 }),
      ..oxide_ui_core::anim::AnimOverrides::default()
   });
   surface.overrides_mut().insert(leaf, oxide_ui_core::anim::AnimOverrides {
      opacity: Some(0.6),
      transform: Some(Transform2D { tx: 5.0, ty: 7.0, sx: 0.9, sy: 1.1, rot_rad: -0.2 }),
      ..oxide_ui_core::anim::AnimOverrides::default()
   });
   let rendered = surface.render_snapshot_retained(
      gfx::RenderChunkId(22),
      &[],
      Vec::new(),
      gfx::Damage { rects: Vec::new() },
   ).unwrap();
   let leaf_instance = rendered.snapshot.instance(2).unwrap();
   assert_eq!(leaf_instance.dynamic_clips.len(), 1);
   let frame = surface.accessibility_frame(leaf).unwrap();
   let center = [frame.x + frame.w * 0.5, frame.y + frame.h * 0.5];
   let hit = surface.hit_test(center[0], center[1]).unwrap();
   assert_eq!(hit.0, leaf);
   assert!(hit.1[0] >= 0.0 && hit.1[0] < 40.0);
   assert!(hit.1[1] >= 0.0 && hit.1[1] < 30.0);
   let leaf_opacity = leaf_instance.property_slots.iter().find_map(|id| {
      rendered.snapshot.properties().iter().find(|property| property.id == *id).and_then(|property| {
         match property.value {
            gfx::RenderPropertyValue::Opacity(value) => Some(value),
            gfx::RenderPropertyValue::Transform(_) => None,
         }
      })
   }).unwrap();
   assert!((leaf_opacity - 0.3).abs() < 1.0e-5);
}

#[test]
fn removed_node_property_slots_reuse_dense_indices_with_new_generations()
{
   let mut tree = oxide_ui_core::NodeTree::new_root(NodeStyle::default());
   let root = tree.root();
   let first = tree.add_node(root, NodeStyle::default());
   tree.layout(100.0, 100.0);
   let (sequence, _) = tree.render_sequence(31).unwrap();
   let first_slots = sequence.instance(1).unwrap().property_slots;
   assert!(tree.remove_node(first));
   let second = tree.add_node(root, NodeStyle::default());
   tree.layout(100.0, 100.0);
   let (sequence, _) = tree.render_sequence(31).unwrap();
   let second_slots = sequence.instance(1).unwrap().property_slots;
   assert_eq!(first_slots[0].dynamic_index(), second_slots[1].dynamic_index());
   assert_eq!(first_slots[1].dynamic_index(), second_slots[0].dynamic_index());
   assert_ne!(first_slots[0].dynamic_generation(), second_slots[1].dynamic_generation());
   assert_ne!(first_slots[1].dynamic_generation(), second_slots[0].dynamic_generation());
   assert!(second.0 > first.0);
}

#[test]
fn mixed_surface_snapshot_invalidates_only_dependent_chunks()
{
   let mut surface = UiSurface::new(NodeStyle {
      size: Size2D { w: Dim::Px(80.0), h: Dim::Px(80.0) },
      background: gfx::Color::rgba(0.2, 0.3, 0.4, 1.0),
      ..NodeStyle::default()
   });
   let root = surface.root();
   surface.layout(80.0, 80.0);

   let image_list = gfx::DrawList {
      items: vec![gfx::DrawCmd::Image {
         tex: gfx::ImageHandle(7),
         dst: gfx::RectF::new(4.0, 4.0, 24.0, 24.0),
         src: gfx::RectF::new(0.0, 0.0, 1.0, 1.0),
         alpha: 1.0,
      }],
      vertices: Vec::new(),
      indices: Vec::new(),
   };
   let image_chunk = gfx::RenderChunk::new(
      gfx::RenderChunkId(22),
      gfx::RenderChunkRevisions { resource: 1, ..gfx::RenderChunkRevisions::default() },
      image_list,
      gfx::ChunkIndexMode::Local,
      &[gfx::RenderResourceDependency { image: gfx::ImageHandle(7), generation: 3 }],
   ).unwrap();
   let expected_image = image_chunk.draw_list().clone();
   let glyph_list = gfx::DrawList {
      items: vec![gfx::DrawCmd::GlyphRun { run: gfx::GlyphRun {
         atlas: gfx::ImageHandle(8),
         atlas_revision: 5,
         vb: gfx::VertexSpan { offset: 0, len: 4 },
         ib: gfx::IndexSpan { offset: 0, len: 6 },
         sdf: false,
         color: gfx::Color::rgba(1.0, 1.0, 1.0, 1.0),
      }}],
      vertices: vec![
         gfx::Vertex { x: 0.0, y: 0.0, u: 0.0, v: 0.0, rgba: 0 },
         gfx::Vertex { x: 12.0, y: 0.0, u: 1.0, v: 0.0, rgba: 0 },
         gfx::Vertex { x: 12.0, y: 16.0, u: 1.0, v: 1.0, rgba: 0 },
         gfx::Vertex { x: 0.0, y: 16.0, u: 0.0, v: 1.0, rgba: 0 },
      ],
      indices: vec![0, 1, 2, 0, 2, 3],
   };
   let glyph_chunk = gfx::RenderChunk::new(
      gfx::RenderChunkId(23),
      gfx::RenderChunkRevisions { resource: 1, ..gfx::RenderChunkRevisions::default() },
      glyph_list,
      gfx::ChunkIndexMode::Local,
      &[],
   ).unwrap();
   let expected_glyph = glyph_chunk.draw_list().clone();
   let image_identity = image_chunk.clone();
   let glyph_identity = glyph_chunk.clone();
   let content = vec![
      gfx::RenderChunkSequence::new(vec![gfx::RenderChunkInstance::new(image_chunk, [0.0, 0.0])]),
      gfx::RenderChunkSequence::new(vec![gfx::RenderChunkInstance::new(glyph_chunk, [0.0, 0.0])]),
   ];
   let rendered = surface.render_snapshot_retained(
      gfx::RenderChunkId(21),
      &content,
      Vec::new(),
      gfx::Damage { rects: vec![gfx::RectI::new(0, 0, 80, 80)] },
   ).unwrap();
   let snapshot = rendered.snapshot;
   assert_eq!(rendered.stats.status, RetainedDrawStatus::Rebuilt);
   assert_eq!(
      {
         let mut ids = Vec::new();
         snapshot.visit_instances(|instance| ids.push(instance.chunk.id()));
         ids
      },
      vec![
         gfx::RenderChunkId((21_u64 << 32) | u64::from(root.0)),
         gfx::RenderChunkId(22),
         gfx::RenderChunkId(23),
      ],
   );
   assert_eq!(snapshot.damage().rects, vec![gfx::RectI::new(0, 0, 80, 80)]);
   assert_eq!(
      snapshot.incompatible_chunk_ids(&[
         gfx::RenderResourceDependency { image: gfx::ImageHandle(7), generation: 4 },
         gfx::RenderResourceDependency { image: gfx::ImageHandle(8), generation: 5 },
      ]),
      vec![gfx::RenderChunkId(22)],
   );

   let mut compatibility = DrawListBuilder::new();
   let fallback = compatibility.append_render_snapshot_flat(&snapshot).unwrap();
   let mut expected = DrawListBuilder::new();
   surface.encode(&mut expected);
   assert!(expected.append_drawlist(&expected_image));
   assert!(expected.append_drawlist(&expected_glyph));
   assert_eq!(compatibility.drawlist(), expected.drawlist());
   assert_eq!(fallback.fallback_count, 1);
   assert_eq!(fallback.chunks_flattened, 3);
   assert_eq!(fallback.commands_copied, expected.drawlist().items.len() as u64);

   let surface_identity = snapshot.instance(0).unwrap().chunk.clone();
   let clean = surface.render_snapshot_retained(
      gfx::RenderChunkId(21),
      &content,
      Vec::new(),
      gfx::Damage { rects: Vec::new() },
   ).unwrap();
   assert_eq!(clean.stats.status, RetainedDrawStatus::Reused);
   assert_eq!(clean.stats.command_bytes_copied, 0);
   assert_eq!(clean.stats.vertex_bytes_copied, 0);
   assert_eq!(clean.stats.index_bytes_copied, 0);
   assert!(surface_identity.ptr_eq(&clean.snapshot.instance(0).unwrap().chunk));
   assert!(image_identity.ptr_eq(&clean.snapshot.instance(1).unwrap().chunk));
   assert!(glyph_identity.ptr_eq(&clean.snapshot.instance(2).unwrap().chunk));

   surface.edit_style(root, |style| {
      style.background = gfx::Color::rgba(0.8, 0.1, 0.2, 1.0);
   });
   let dirty = surface.render_snapshot_retained(
      gfx::RenderChunkId(21),
      &content,
      Vec::new(),
      gfx::Damage { rects: Vec::new() },
   ).unwrap();
   assert_eq!(dirty.stats.status, RetainedDrawStatus::Rebuilt);
   assert!(!surface_identity.ptr_eq(&dirty.snapshot.instance(0).unwrap().chunk));
   assert!(image_identity.ptr_eq(&dirty.snapshot.instance(1).unwrap().chunk));
   assert!(glyph_identity.ptr_eq(&dirty.snapshot.instance(2).unwrap().chunk));
}

fn c28_render(surface: &mut UiSurface, content: &[gfx::RenderChunkSequence]) -> oxide_ui_core::SurfaceRenderSnapshot
{
   surface.render_snapshot_retained(
      gfx::RenderChunkId(280),
      content,
      Vec::new(),
      gfx::Damage { rects: Vec::new() },
   ).unwrap()
}

fn c28_rect_chunk(id: u64, revision: u64, rect: gfx::RectF, color: gfx::Color) -> gfx::RenderChunk
{
   gfx::RenderChunk::new(
      gfx::RenderChunkId(id),
      gfx::RenderChunkRevisions { geometry: revision, ..gfx::RenderChunkRevisions::default() },
      gfx::DrawList {
         items: vec![gfx::DrawCmd::RRect { rect, radii: [0.0; 4], color }],
         vertices: Vec::new(),
         indices: Vec::new(),
      },
      gfx::ChunkIndexMode::Local,
      &[],
   ).unwrap()
}

#[test]
fn retained_damage_skips_static_idle_and_tracks_caret_property_damage()
{
   let mut surface = UiSurface::new(NodeStyle {
      axis: Axis::Column,
      size: Size2D { w: Dim::Px(100.0), h: Dim::Px(60.0) },
      padding: Edges { left: 10.0, top: 10.0, right: 0.0, bottom: 0.0 },
      background: gfx::Color::rgba(0.1, 0.2, 0.3, 1.0),
      ..NodeStyle::default()
   });
   let root = surface.root();
   let caret = surface.add_node(root, NodeStyle {
      size: Size2D { w: Dim::Px(2.0), h: Dim::Px(16.0) },
      background: gfx::Color::rgba(0.9, 0.9, 0.9, 1.0),
      ..NodeStyle::default()
   }).unwrap();
   surface.layout(100.0, 60.0);

   assert!(surface.needs_frame());
   let cold = c28_render(&mut surface, &[]);
   assert_eq!(cold.snapshot.damage().rects, [gfx::RectI::new(0, 0, 100, 60)]);
   assert!(surface.needs_frame());

   let mut damage = Vec::with_capacity(8);
   let capacity = damage.capacity();
   surface.take_damage_into(&mut damage);
   assert_eq!(damage, [gfx::RectI::new(0, 0, 100, 60)]);
   assert!(!surface.needs_frame());
   surface.take_damage_into(&mut damage);
   assert!(damage.is_empty());
   assert_eq!(damage.capacity(), capacity);

   let idle = c28_render(&mut surface, &[]);
   assert!(idle.snapshot.damage().rects.is_empty());
   assert_eq!(surface.damage_stats().changed_paint_units, 0);

   surface.set_frame_demand(SurfaceFrameDemand {
      camera: true,
      timer_due: true,
      upload: true,
      async_publication: true,
   });
   assert!(surface.needs_frame());
   surface.set_frame_demand(SurfaceFrameDemand::default());
   assert!(!surface.needs_frame());

   assert!(surface.edit_style(caret, |style| style.opacity = 0.0));
   assert!(surface.needs_frame());
   let blink = c28_render(&mut surface, &[]);
   assert_eq!(blink.snapshot.damage().rects, [gfx::RectI::new(9, 9, 4, 18)]);
   assert_eq!(surface.damage_stats().changed_paint_units, 1);
   assert_eq!(surface.damage_stats().property_only_changes, 1);
   assert_eq!(surface.damage_stats().damage_pixels, 72);
   surface.take_damage_into(&mut damage);
   assert_eq!(damage, blink.snapshot.damage().rects);
   assert_eq!(damage.capacity(), capacity);
}

#[test]
fn retained_damage_unions_move_remove_resize_and_one_dirty_leaf_bounds()
{
   let mut surface = UiSurface::new(NodeStyle {
      axis: Axis::Row,
      size: Size2D { w: Dim::Px(120.0), h: Dim::Px(60.0) },
      gap: 10.0,
      ..NodeStyle::default()
   });
   let root = surface.root();
   let moving = surface.add_node(root, NodeStyle {
      size: Size2D { w: Dim::Px(20.0), h: Dim::Px(20.0) },
      background: gfx::Color::rgba(0.8, 0.2, 0.1, 1.0),
      ..NodeStyle::default()
   }).unwrap();
   let sibling = surface.add_node(root, NodeStyle {
      size: Size2D { w: Dim::Px(20.0), h: Dim::Px(20.0) },
      background: gfx::Color::rgba(0.1, 0.6, 0.3, 1.0),
      ..NodeStyle::default()
   }).unwrap();
   surface.layout(120.0, 60.0);
   let _ = c28_render(&mut surface, &[]);

   assert!(surface.edit_style(moving, |style| style.transform.tx = 30.0));
   let moved = c28_render(&mut surface, &[]);
   assert_eq!(moved.snapshot.damage().rects, [gfx::RectI::new(0, 0, 51, 21)]);
   assert_eq!(surface.damage_stats().changed_paint_units, 1);
   assert_eq!(surface.damage_stats().property_only_changes, 1);

   assert!(surface.remove_node(moving));
   surface.layout(120.0, 60.0);
   let removed = c28_render(&mut surface, &[]);
   assert_eq!(removed.snapshot.damage().rects, [gfx::RectI::new(0, 0, 51, 21)]);
   assert_eq!(surface.damage_stats().changed_paint_units, 2);
   assert_eq!(surface.damage_stats().layout_changes, 2);
   assert!(surface.damage_stats().descendant_invalidations >= 1);

   assert!(surface.edit_style(sibling, |style| {
      style.background = gfx::Color::rgba(0.2, 0.3, 0.9, 1.0);
   }));
   let painted = c28_render(&mut surface, &[]);
   assert_eq!(painted.snapshot.damage().rects, [gfx::RectI::new(0, 0, 21, 21)]);
   assert_eq!(painted.stats.chunks_rebuilt, 1);
   assert_eq!(surface.damage_stats().changed_paint_units, 1);
   assert_eq!(surface.damage_stats().paint_changes, 1);

   assert!(surface.edit_style(sibling, |style| style.size.w = Dim::Px(30.0)));
   surface.layout(120.0, 60.0);
   let resized = c28_render(&mut surface, &[]);
   assert_eq!(resized.snapshot.damage().rects, [gfx::RectI::new(0, 0, 31, 21)]);
   assert_eq!(surface.damage_stats().changed_paint_units, 1);
   assert_eq!(surface.damage_stats().layout_changes, 1);
}

#[test]
fn retained_damage_propagates_underlay_mutation_through_backdrop_dependency()
{
   let mut surface = UiSurface::new(NodeStyle {
      size: Size2D { w: Dim::Px(100.0), h: Dim::Px(100.0) },
      background: gfx::Color::rgba(0.05, 0.06, 0.07, 1.0),
      ..NodeStyle::default()
   });
   surface.layout(100.0, 100.0);
   let underlay = c28_rect_chunk(
      281,
      1,
      gfx::RectF::new(20.0, 20.0, 10.0, 10.0),
      gfx::Color::rgba(0.8, 0.2, 0.1, 1.0),
   );
   let effect = gfx::RenderChunk::new(
      gfx::RenderChunkId(282),
      gfx::RenderChunkRevisions::default(),
      gfx::DrawList {
         items: vec![gfx::DrawCmd::Backdrop {
            rect: gfx::RectF::new(10.0, 10.0, 40.0, 40.0),
            sigma: 4.0,
            tint: gfx::Color::rgba(0.2, 0.3, 0.4, 0.5),
            alpha: 0.8,
         }],
         vertices: Vec::new(),
         indices: Vec::new(),
      },
      gfx::ChunkIndexMode::Local,
      &[],
   ).unwrap();
   let content = [gfx::RenderChunkSequence::new(vec![
      gfx::RenderChunkInstance::new(underlay, [0.0, 0.0]),
      gfx::RenderChunkInstance::new(effect.clone(), [0.0, 0.0]),
   ])];
   let warm = c28_render(&mut surface, &content);
   let mut warm_flat = gfx::DrawList::default();
   warm.snapshot.flatten_into(&mut warm_flat).unwrap();

   let changed_underlay = c28_rect_chunk(
      281,
      2,
      gfx::RectF::new(20.0, 20.0, 10.0, 10.0),
      gfx::Color::rgba(0.1, 0.7, 0.3, 1.0),
   );
   let changed_content = [gfx::RenderChunkSequence::new(vec![
      gfx::RenderChunkInstance::new(changed_underlay, [0.0, 0.0]),
      gfx::RenderChunkInstance::new(effect, [0.0, 0.0]),
   ])];
   let changed = c28_render(&mut surface, &changed_content);
   assert_eq!(changed.snapshot.damage().rects, [gfx::RectI::new(9, 9, 42, 42)]);
   assert_eq!(surface.damage_stats().changed_paint_units, 1);
   assert_eq!(surface.damage_stats().effect_expansions, 1);

   let mut changed_flat = gfx::DrawList::default();
   changed.snapshot.flatten_into(&mut changed_flat).unwrap();
   assert_eq!(changed_flat.items.len(), 3);
   assert_eq!(changed_flat.items[0], warm_flat.items[0]);
   assert_ne!(changed_flat.items[1], warm_flat.items[1]);
   assert_eq!(changed_flat.items[2], warm_flat.items[2]);
}

#[test]
fn retained_damage_falls_back_to_full_after_bounded_region_capacity()
{
   let mut surface = UiSurface::new(NodeStyle {
      size: Size2D { w: Dim::Px(200.0), h: Dim::Px(20.0) },
      ..NodeStyle::default()
   });
   surface.layout(200.0, 20.0);
   let make_content = |revision: u64, color: gfx::Color| {
      gfx::RenderChunkSequence::new((0..9_u64).map(|index| {
         gfx::RenderChunkInstance::new(
            c28_rect_chunk(300 + index, revision, gfx::RectF::new(0.0, 0.0, 4.0, 4.0), color),
            [5.0 + index as f32 * 20.0, 5.0],
         )
      }).collect())
   };
   let warm_content = [make_content(1, gfx::Color::rgba(0.8, 0.2, 0.1, 1.0))];
   let _ = c28_render(&mut surface, &warm_content);
   let changed_content = [make_content(2, gfx::Color::rgba(0.1, 0.7, 0.3, 1.0))];
   let changed = c28_render(&mut surface, &changed_content);

   assert_eq!(changed.snapshot.damage().rects, [gfx::RectI::new(0, 0, 200, 20)]);
   assert_eq!(surface.damage_stats().changed_paint_units, 9);
   assert!(surface.damage_stats().full_damage);
   assert_eq!(surface.damage_stats().damage_pixels, 4_000);
}

#[test]
fn retained_damage_falls_back_to_full_for_unbounded_effect_dependency()
{
   let mut surface = UiSurface::new(NodeStyle {
      size: Size2D { w: Dim::Px(80.0), h: Dim::Px(60.0) },
      ..NodeStyle::default()
   });
   surface.layout(80.0, 60.0);
   let effect = gfx::RenderChunk::new(
      gfx::RenderChunkId(370),
      gfx::RenderChunkRevisions::default(),
      gfx::DrawList {
         items: vec![gfx::DrawCmd::Backdrop {
            rect: gfx::RectF::new(10.0, 10.0, 40.0, 30.0),
            sigma: f32::NAN,
            tint: gfx::Color::rgba(0.2, 0.3, 0.4, 0.5),
            alpha: 0.8,
         }],
         vertices: Vec::new(),
         indices: Vec::new(),
      },
      gfx::ChunkIndexMode::Local,
      &[],
   ).unwrap();
   let make_content = |revision, color| {
      [gfx::RenderChunkSequence::new(vec![
         gfx::RenderChunkInstance::new(
            c28_rect_chunk(
               371,
               revision,
               gfx::RectF::new(20.0, 20.0, 4.0, 4.0),
               color,
            ),
            [0.0, 0.0],
         ),
         gfx::RenderChunkInstance::new(effect.clone(), [0.0, 0.0]),
      ])]
   };
   let warm = make_content(1, gfx::Color::rgba(0.8, 0.2, 0.1, 1.0));
   let _ = c28_render(&mut surface, &warm);
   let changed = make_content(2, gfx::Color::rgba(0.1, 0.7, 0.3, 1.0));
   let rendered = c28_render(&mut surface, &changed);

   assert_eq!(rendered.snapshot.damage().rects, [gfx::RectI::new(0, 0, 80, 60)]);
   assert!(surface.damage_stats().full_damage);
   assert_eq!(surface.damage_stats().damage_pixels, 4_800);
}

#[test]
fn retained_damage_classifies_resource_and_clip_invalidation()
{
   let mut surface = UiSurface::new(NodeStyle {
      size: Size2D { w: Dim::Px(80.0), h: Dim::Px(60.0) },
      ..NodeStyle::default()
   });
   surface.layout(80.0, 60.0);
   let image_chunk = |generation| {
      gfx::RenderChunk::new(
         gfx::RenderChunkId(380),
         gfx::RenderChunkRevisions { resource: generation, ..gfx::RenderChunkRevisions::default() },
         gfx::DrawList {
            items: vec![gfx::DrawCmd::Image {
               tex: gfx::ImageHandle(7),
               dst: gfx::RectF::new(10.0, 10.0, 20.0, 20.0),
               src: gfx::RectF::new(0.0, 0.0, 1.0, 1.0),
               alpha: 1.0,
            }],
            vertices: Vec::new(),
            indices: Vec::new(),
         },
         gfx::ChunkIndexMode::Local,
         &[gfx::RenderResourceDependency { image: gfx::ImageHandle(7), generation }],
      ).unwrap()
   };
   let sequence = |generation, clip| {
      let mut instance = gfx::RenderChunkInstance::new(image_chunk(generation), [0.0, 0.0]);
      instance.clip = clip;
      [gfx::RenderChunkSequence::new(vec![instance])]
   };
   let warm = sequence(1, Some(gfx::RectI::new(0, 0, 40, 40)));
   let _ = c28_render(&mut surface, &warm);

   let resource = sequence(2, Some(gfx::RectI::new(0, 0, 40, 40)));
   let resource_changed = c28_render(&mut surface, &resource);
   assert_eq!(resource_changed.snapshot.damage().rects, [gfx::RectI::new(9, 9, 22, 22)]);
   assert_eq!(surface.damage_stats().resource_changes, 1);
   assert_eq!(surface.damage_stats().paint_changes, 0);

   let clipped = sequence(2, Some(gfx::RectI::new(0, 0, 20, 40)));
   let clip_changed = c28_render(&mut surface, &clipped);
   assert_eq!(clip_changed.snapshot.damage().rects, [gfx::RectI::new(9, 9, 22, 22)]);
   assert_eq!(surface.damage_stats().clip_changes, 1);
   assert_eq!(surface.damage_stats().layout_changes, 0);
}

#[test]
fn retained_damage_covers_reordered_overlapping_chunks()
{
   let mut surface = UiSurface::new(NodeStyle {
      size: Size2D { w: Dim::Px(80.0), h: Dim::Px(60.0) },
      ..NodeStyle::default()
   });
   surface.layout(80.0, 60.0);
   let first = c28_rect_chunk(
      390,
      1,
      gfx::RectF::new(10.0, 10.0, 30.0, 30.0),
      gfx::Color::rgba(0.8, 0.2, 0.1, 1.0),
   );
   let second = c28_rect_chunk(
      391,
      1,
      gfx::RectF::new(20.0, 20.0, 30.0, 30.0),
      gfx::Color::rgba(0.1, 0.7, 0.3, 0.8),
   );
   let warm_content = [gfx::RenderChunkSequence::new(vec![
      gfx::RenderChunkInstance::new(first.clone(), [0.0, 0.0]),
      gfx::RenderChunkInstance::new(second.clone(), [0.0, 0.0]),
   ])];
   let warm = c28_render(&mut surface, &warm_content);
   let mut warm_flat = gfx::DrawList::default();
   warm.snapshot.flatten_into(&mut warm_flat).unwrap();

   let reordered_content = [gfx::RenderChunkSequence::new(vec![
      gfx::RenderChunkInstance::new(second, [0.0, 0.0]),
      gfx::RenderChunkInstance::new(first, [0.0, 0.0]),
   ])];
   let reordered = c28_render(&mut surface, &reordered_content);
   assert_eq!(reordered.snapshot.damage().rects, [gfx::RectI::new(9, 9, 42, 42)]);
   assert_eq!(surface.damage_stats().changed_paint_units, 0);
   assert_eq!(surface.damage_stats().order_changes, 2);
   let mut reordered_flat = gfx::DrawList::default();
   reordered.snapshot.flatten_into(&mut reordered_flat).unwrap();
   assert_eq!(reordered_flat.items[0], warm_flat.items[0]);
   assert_eq!(reordered_flat.items[1], warm_flat.items[2]);
   assert_eq!(reordered_flat.items[2], warm_flat.items[1]);
}

#[test]
fn retained_damage_memory_warning_purges_cache_and_forces_exact_full_redraw()
{
   let mut surface = UiSurface::new(NodeStyle {
      size: Size2D { w: Dim::Px(80.0), h: Dim::Px(60.0) },
      background: gfx::Color::rgba(0.2, 0.3, 0.4, 1.0),
      ..NodeStyle::default()
   });
   let root = surface.root();
   surface.add_node(root, NodeStyle {
      size: Size2D { w: Dim::Px(20.0), h: Dim::Px(20.0) },
      background: gfx::Color::rgba(0.8, 0.2, 0.1, 1.0),
      ..NodeStyle::default()
   }).unwrap();
   surface.layout(80.0, 60.0);
   let warm = c28_render(&mut surface, &[]);
   let mut warm_flat = gfx::DrawList::default();
   warm.snapshot.flatten_into(&mut warm_flat).unwrap();

   surface.handle_memory_warning();
   assert!(surface.needs_frame());
   let rebuilt = c28_render(&mut surface, &[]);
   assert_eq!(rebuilt.snapshot.damage().rects, [gfx::RectI::new(0, 0, 80, 60)]);
   assert!(surface.damage_stats().full_damage);
   assert!(surface.retained_node_stats().cache_evictions >= 2);
   let mut rebuilt_flat = gfx::DrawList::default();
   rebuilt.snapshot.flatten_into(&mut rebuilt_flat).unwrap();
   assert_eq!(rebuilt_flat, warm_flat);
   let mut damage = Vec::with_capacity(8);
   surface.take_damage_into(&mut damage);
   assert!(!surface.needs_frame());
}
