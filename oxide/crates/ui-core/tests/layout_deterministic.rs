use oxide_platform_api::Transform2D;
use oxide_ui_core::{Axis, Dim, Edges, NodeStyle, NodeTree, Size2D};

fn root_style() -> NodeStyle {
    NodeStyle {
        axis: Axis::Row,
        size: Size2D { w: Dim::Px(200.0), h: Dim::Px(120.0) },
        padding: Edges { left: 10.0, top: 8.0, right: 10.0, bottom: 8.0 },
        ..NodeStyle::default()
    }
}

#[test]
fn hit_test_prefers_last_child() {
    let mut tree = NodeTree::new_root(root_style());
    let root = tree.root();
    let _child_a = tree.add_node(
        root,
        NodeStyle { size: Size2D { w: Dim::Px(80.0), h: Dim::Px(80.0) }, ..NodeStyle::default() },
    );
    let child_b = tree.add_node(
        root,
        NodeStyle {
            size: Size2D { w: Dim::Px(80.0), h: Dim::Px(80.0) },
            transform: Transform2D { tx: -60.0, ty: 0.0, sx: 1.0, sy: 1.0, rot_rad: 0.0 },
            ..NodeStyle::default()
        },
    );
    tree.layout(200.0, 120.0);

    // Point lies inside both children; last child should test first (top-most).
    let (hit_node, _) = tree.hit_test(40.0, 40.0).expect("expected hit");
    assert_eq!(hit_node, child_b);

    // Point outside children but inside root should hit root.
    let (hit_root, _) = tree.hit_test(190.0, 110.0).expect("hit root");
    assert_eq!(hit_root, root);
}

#[test]
fn layout_respects_margin_padding_and_transform() {
    let mut tree = NodeTree::new_root(root_style());
    let root = tree.root();
    let node = tree.add_node(
        root,
        NodeStyle {
            size: Size2D { w: Dim::Px(60.0), h: Dim::Px(30.0) },
            margin: Edges { left: 5.0, top: 7.0, right: 2.0, bottom: 3.0 },
            padding: Edges { left: 4.0, top: 2.0, right: 1.0, bottom: 1.0 },
            transform: Transform2D { tx: 3.0, ty: 4.0, sx: 1.0, sy: 1.0, rot_rad: 0.0 },
            ..NodeStyle::default()
        },
    );
    tree.layout(200.0, 120.0);

    // The node's top-left should factor in root padding, its margin, and transform.
    let expected_x = 10.0 + 5.0 + 3.0; // root padding + margin + transform
    let expected_y = 8.0 + 7.0 + 4.0;
    let (hit_node, offset) = tree.hit_test(expected_x + 1.0, expected_y + 1.0).expect("hit node");
    assert_eq!(hit_node, node);
    assert!((offset[0] - 1.0).abs() < 1e-3);
    assert!((offset[1] - 1.0).abs() < 1e-3);
}

#[test]
fn zero_sized_layout_has_no_hits() {
    let mut tree = NodeTree::new_root(NodeStyle {
        size: Size2D { w: Dim::Px(0.0), h: Dim::Px(0.0) },
        ..NodeStyle::default()
    });
    let root = tree.root();
    let _ = tree.add_node(
        root,
        NodeStyle { size: Size2D { w: Dim::Px(10.0), h: Dim::Px(10.0) }, ..NodeStyle::default() },
    );
    tree.layout(0.0, 0.0);
    assert!(tree.hit_test(0.0, 0.0).is_none());
}

#[test]
fn remove_node_removes_descendants_and_reuses_slots() {
    let mut tree = NodeTree::new_root(root_style());
    let root = tree.root();
    let parent = tree.add_node(
        root,
        NodeStyle {
            axis: Axis::Column,
            size: Size2D { w: Dim::Px(90.0), h: Dim::Px(90.0) },
            ..NodeStyle::default()
        },
    );
    let child = tree.add_node(
        parent,
        NodeStyle { size: Size2D { w: Dim::Px(40.0), h: Dim::Px(40.0) }, ..NodeStyle::default() },
    );
    let sibling = tree.add_node(
        root,
        NodeStyle { size: Size2D { w: Dim::Px(60.0), h: Dim::Px(60.0) }, ..NodeStyle::default() },
    );

    tree.remove_node(parent);

    assert!(tree.style(parent).is_none());
    assert!(tree.style(child).is_none());
    assert!(tree.style(sibling).is_some());
    let reused_a = tree.add_node(root, NodeStyle { ..NodeStyle::default() });
    let reused_b = tree.add_node(root, NodeStyle { ..NodeStyle::default() });
    assert!([reused_a, reused_b].contains(&parent));
    assert!([reused_a, reused_b].contains(&child));
}

#[test]
fn route_pointer_invokes_handler() {
    let mut tree = NodeTree::new_root(root_style());
    let target = tree.add_node(
        tree.root(),
        NodeStyle { size: Size2D { w: Dim::Px(40.0), h: Dim::Px(40.0) }, ..NodeStyle::default() },
    );
    tree.layout(200.0, 120.0);
    let mut called = None;
    tree.route_pointer(15.0, 12.0, |id, _pos| called = Some(id));
    assert_eq!(called, Some(target));
}

#[test]
fn row_layout_and_hit() {
    let mut tree = NodeTree::new_root(NodeStyle {
        axis: Axis::Row,
        size: Size2D { w: Dim::Px(300.0), h: Dim::Px(100.0) },
        gap: 10.0,
        padding: Edges { left: 10.0, top: 10.0, right: 10.0, bottom: 10.0 },
        ..NodeStyle::default()
    });
    let a = tree.add_node(
        tree.root(),
        NodeStyle { size: Size2D { w: Dim::Px(50.0), h: Dim::Px(50.0) }, ..NodeStyle::default() },
    );
    let b = tree.add_node(
        tree.root(),
        NodeStyle {
            size: Size2D { w: Dim::Px(0.0), h: Dim::Px(50.0) },
            flex_grow: 1.0,
            ..NodeStyle::default()
        },
    );

    tree.layout(300.0, 100.0);

    let a_layout = tree.layout_rect(a).expect("a layout");
    let b_layout = tree.layout_rect(b).expect("b layout");
    assert!((a_layout.x - 10.0).abs() < 0.5);
    assert!((b_layout.x - 70.0).abs() < 0.5);
    let hit = tree.hit_test(80.0, 20.0).expect("hit inside b");
    assert_eq!(hit.0, b);
}

#[test]
fn reuse_node_slots_and_preserve_root() {
    let mut tree = NodeTree::new_root(NodeStyle { ..NodeStyle::default() });
    let a = tree.add_node(tree.root(), NodeStyle { ..NodeStyle::default() });
    let b = tree.add_node(tree.root(), NodeStyle { ..NodeStyle::default() });
    assert_eq!(a.0, 2);
    assert_eq!(b.0, 3);

    tree.remove_node(a);
    let c = tree.add_node(tree.root(), NodeStyle { ..NodeStyle::default() });

    assert_eq!(c.0, a.0);
    tree.remove_node(tree.root());
    assert!(tree.style(tree.root()).is_some());
}
