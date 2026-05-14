use oxide_renderer_api::{Insets, RectF};
use oxide_ui_core::{
    build_visual_tree_action_graph, build_visual_tree_action_graph_manifest,
    compare_visual_tree_action_graphs, compare_visual_tree_sequences,
    compare_visual_tree_snapshots, default_visual_tree_action_animation_trace,
    visual_tree_action_observation_for_path, visual_tree_node_by_path, VisualTreeActionDescriptor,
    VisualTreeActionObservation, VisualTreeInsets, VisualTreeNode, VisualTreeRect,
    VisualTreeSequence, VisualTreeSequenceStep, VisualTreeSnapshot, VisualTreeViewport,
    VISUAL_TREE_ACTION_GRAPH_MANIFEST_SCHEMA_VERSION, VISUAL_TREE_SCHEMA_VERSION,
    VISUAL_TREE_SEQUENCE_SCHEMA_VERSION,
};
use std::collections::BTreeMap;

fn snapshot(child_frame: RectF, label: &str) -> VisualTreeSnapshot {
    let mut root = VisualTreeNode::new("root", "Root", RectF::new(0.0, 0.0, 100.0, 200.0));
    root.push_child(
        VisualTreeNode::new("root/button", "Button", child_frame)
            .with_role("button")
            .with_data("label", label),
    );
    VisualTreeSnapshot {
        schema: VISUAL_TREE_SCHEMA_VERSION.to_owned(),
        producer: "test".to_owned(),
        scene: "scene".to_owned(),
        route: "route".to_owned(),
        preset: Some("fixture".to_owned()),
        viewport: VisualTreeViewport {
            frame: VisualTreeRect::from_rect(RectF::new(0.0, 0.0, 100.0, 200.0)),
            safe: VisualTreeInsets::from_insets(Insets::new(0.0, 10.0, 0.0, 5.0)),
            points_scale: 1.0,
        },
        root,
    }
}

fn snapshot_with_button_opacity(
    child_frame: RectF,
    label: &str,
    opacity: f32,
) -> VisualTreeSnapshot {
    let mut snapshot = snapshot(child_frame, label);
    snapshot.root.children[0].opacity = opacity;
    snapshot
}

fn nested_opacity_snapshot(container_opacity: f32, child_opacity: f32) -> VisualTreeSnapshot {
    let mut root = VisualTreeNode::new("root", "Root", RectF::new(0.0, 0.0, 100.0, 200.0));
    let mut container =
        VisualTreeNode::new("root/container", "Container", RectF::new(10.0, 20.0, 60.0, 80.0))
            .with_opacity(container_opacity);
    container.push_child(
        VisualTreeNode::new("root/container/icon", "Icon", RectF::new(16.0, 24.0, 20.0, 20.0))
            .with_opacity(child_opacity),
    );
    root.push_child(container);

    VisualTreeSnapshot {
        schema: VISUAL_TREE_SCHEMA_VERSION.to_owned(),
        producer: "test".to_owned(),
        scene: "scene".to_owned(),
        route: "route".to_owned(),
        preset: Some("fixture".to_owned()),
        viewport: VisualTreeViewport {
            frame: VisualTreeRect::from_rect(RectF::new(0.0, 0.0, 100.0, 200.0)),
            safe: VisualTreeInsets::from_insets(Insets::new(0.0, 10.0, 0.0, 5.0)),
            points_scale: 1.0,
        },
        root,
    }
}

fn nested_action_snapshot(container_opacity: f32, button_opacity: f32) -> VisualTreeSnapshot {
    let mut root = VisualTreeNode::new("root", "Root", RectF::new(0.0, 0.0, 100.0, 200.0));
    let mut container =
        VisualTreeNode::new("root/container", "Container", RectF::new(10.0, 20.0, 60.0, 80.0))
            .with_opacity(container_opacity);
    container.push_child(
        VisualTreeNode::new("root/container/button", "Button", RectF::new(16.0, 24.0, 20.0, 20.0))
            .with_role("button")
            .with_opacity(button_opacity)
            .with_data("target", "people"),
    );
    root.push_child(container);

    VisualTreeSnapshot {
        schema: VISUAL_TREE_SCHEMA_VERSION.to_owned(),
        producer: "test".to_owned(),
        scene: "scene".to_owned(),
        route: "route".to_owned(),
        preset: Some("fixture".to_owned()),
        viewport: VisualTreeViewport {
            frame: VisualTreeRect::from_rect(RectF::new(0.0, 0.0, 100.0, 200.0)),
            safe: VisualTreeInsets::from_insets(Insets::new(0.0, 10.0, 0.0, 5.0)),
            points_scale: 1.0,
        },
        root,
    }
}

fn observation(
    step_index: usize,
    frame: VisualTreeRect,
    opacity: f32,
) -> VisualTreeActionObservation {
    VisualTreeActionObservation {
        step_index,
        scene: "scene".to_owned(),
        route: "route".to_owned(),
        preset: Some("fixture".to_owned()),
        frame,
        visible: true,
        opacity,
        data: BTreeMap::new(),
    }
}

fn snapshot_with_visual_metrics(
    content_x: f64,
    content_y: f64,
    font_px: f64,
    line_height_px: f64,
) -> VisualTreeSnapshot {
    let mut snapshot = snapshot(RectF::new(10.0, 20.0, 30.0, 40.0), "demo");
    snapshot.root.children[0].data.insert(
        "visual".to_owned(),
        serde_json::json!({
            "content_rect": {
                "x": content_x,
                "y": content_y,
                "w": 24.0,
                "h": 12.0
            },
            "font_px": font_px,
            "line_height_px": line_height_px,
            "stroke_width_px": 0.0,
            "text_align": "center"
        }),
    );
    snapshot
}

fn snapshot_with_nested_non_visual_metric(score: f64) -> VisualTreeSnapshot {
    let mut snapshot = snapshot(RectF::new(10.0, 20.0, 30.0, 40.0), "demo");
    snapshot.root.children[0].data.insert(
        "stats".to_owned(),
        serde_json::json!({
            "score": score
        }),
    );
    snapshot
}

fn numeric_encoding_snapshot(root: VisualTreeNode) -> VisualTreeSnapshot {
    VisualTreeSnapshot {
        schema: VISUAL_TREE_SCHEMA_VERSION.to_owned(),
        producer: "test".to_owned(),
        scene: "mission_control".to_owned(),
        route: "mission_control::radar".to_owned(),
        preset: Some("legacy_preview".to_owned()),
        viewport: VisualTreeViewport {
            frame: VisualTreeRect { x: 0.0, y: 0.0, w: 100.0, h: 100.0 },
            safe: VisualTreeInsets { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 },
            points_scale: 1.0,
        },
        root,
    }
}

fn numeric_encoding_node() -> VisualTreeNode {
    VisualTreeNode::new("root/card", "BigFaceNode", RectF::new(0.0, 0.0, 10.0, 10.0))
}

#[test]
fn visual_tree_data_compare_accepts_equivalent_numeric_encodings() {
    let reference = numeric_encoding_snapshot(
        numeric_encoding_node()
            .with_data("tunnel_alpha", serde_json::json!(1))
            .with_data("tunnel_scale", serde_json::json!(0.9706518054008483)),
    );
    let candidate = numeric_encoding_snapshot(
        numeric_encoding_node()
            .with_data("tunnel_alpha", serde_json::json!(1.0))
            .with_data("tunnel_scale", serde_json::json!(0.9706518054008484)),
    );

    let diff = compare_visual_tree_snapshots(&reference, &candidate, 0.75);

    assert!(diff.passed, "{:?}", diff.mismatches);
}

#[test]
fn visual_tree_data_compare_rejects_meaningful_numeric_deltas() {
    let reference = numeric_encoding_snapshot(
        numeric_encoding_node().with_data("tunnel_alpha", serde_json::json!(0.5)),
    );
    let candidate = numeric_encoding_snapshot(
        numeric_encoding_node().with_data("tunnel_alpha", serde_json::json!(0.51)),
    );

    let diff = compare_visual_tree_snapshots(&reference, &candidate, 0.75);

    assert!(!diff.passed);
    assert_eq!(diff.mismatches.len(), 1);
    assert_eq!(diff.mismatches[0].field, "data.tunnel_alpha");
    assert!(diff.mismatches[0].delta.is_some());
}

#[test]
fn visual_tree_diff_accepts_frame_deltas_inside_tolerance() {
    let expected = snapshot(RectF::new(10.0, 20.0, 30.0, 40.0), "demo");
    let actual = snapshot(RectF::new(10.2, 19.9, 30.0, 40.1), "demo");

    let diff = compare_visual_tree_snapshots(&expected, &actual, 0.25);

    assert!(diff.passed);
    assert!(diff.mismatches.is_empty());
}

#[test]
fn visual_tree_diff_reports_geometry_and_semantic_mismatches() {
    let expected = snapshot(RectF::new(10.0, 20.0, 30.0, 40.0), "demo");
    let actual = snapshot(RectF::new(12.0, 20.0, 30.0, 40.0), "other");

    let diff = compare_visual_tree_snapshots(&expected, &actual, 0.25);

    assert!(!diff.passed);
    assert!(diff
        .mismatches
        .iter()
        .any(|mismatch| mismatch.path == "root/button" && mismatch.field == "frame.x"));
    assert!(diff
        .mismatches
        .iter()
        .any(|mismatch| mismatch.path == "root/button" && mismatch.field == "data.label"));
}

#[test]
fn visual_tree_diff_reports_viewport_safe_area_mismatch() {
    let expected = snapshot(RectF::new(10.0, 20.0, 30.0, 40.0), "demo");
    let mut actual = snapshot(RectF::new(10.0, 20.0, 30.0, 40.0), "demo");
    actual.viewport.safe.top = 0.0;

    let diff = compare_visual_tree_snapshots(&expected, &actual, 0.25);

    assert!(!diff.passed);
    assert!(diff
        .mismatches
        .iter()
        .any(|mismatch| mismatch.path == "$.viewport.safe" && mismatch.field == "top"));
}

#[test]
fn visual_tree_diff_reports_points_scale_mismatch() {
    let expected = snapshot(RectF::new(10.0, 20.0, 30.0, 40.0), "demo");
    let mut actual = snapshot(RectF::new(10.0, 20.0, 30.0, 40.0), "demo");
    actual.viewport.points_scale = 2.0;

    let diff = compare_visual_tree_snapshots(&expected, &actual, 0.25);

    assert!(!diff.passed);
    assert!(diff
        .mismatches
        .iter()
        .any(|mismatch| mismatch.path == "$.viewport" && mismatch.field == "points_scale"));
}

#[test]
fn visual_tree_diff_reports_opacity_mismatch_with_large_geometry_tolerance() {
    let expected = snapshot_with_button_opacity(RectF::new(10.0, 20.0, 30.0, 40.0), "demo", 0.20);
    let actual = snapshot_with_button_opacity(RectF::new(10.0, 20.0, 30.0, 40.0), "demo", 0.65);

    let diff = compare_visual_tree_snapshots(&expected, &actual, 0.75);

    assert!(!diff.passed);
    assert!(diff
        .mismatches
        .iter()
        .any(|mismatch| mismatch.path == "root/button" && mismatch.field == "opacity"));
}

#[test]
fn visual_tree_diff_reports_effective_opacity_mismatch_for_nested_nodes() {
    let expected = nested_opacity_snapshot(0.35, 1.0);
    let actual = nested_opacity_snapshot(0.75, 1.0);

    let diff = compare_visual_tree_snapshots(&expected, &actual, 0.75);

    assert!(!diff.passed);
    assert!(diff
        .mismatches
        .iter()
        .any(|mismatch| mismatch.path == "root/container/icon"
            && mismatch.field == "effective_opacity"));
}

#[test]
fn visual_tree_diff_accepts_nested_visual_metric_deltas_inside_tolerance() {
    let expected =
        snapshot_with_visual_metrics(114.0, 115.0, 21.84782600402832, 25.037608600616455);
    let actual = snapshot_with_visual_metrics(
        113.66666412353516,
        115.0,
        21.84782600402832,
        25.0383243560791,
    );

    let diff = compare_visual_tree_snapshots(&expected, &actual, 0.75);

    assert!(diff.passed, "{:?}", diff.mismatches);
}

#[test]
fn visual_tree_diff_reports_nested_visual_metric_delta_outside_tolerance() {
    let expected =
        snapshot_with_visual_metrics(114.0, 115.0, 21.84782600402832, 25.037608600616455);
    let actual = snapshot_with_visual_metrics(112.9, 115.0, 21.84782600402832, 25.037608600616455);

    let diff = compare_visual_tree_snapshots(&expected, &actual, 0.75);

    assert!(!diff.passed);
    assert!(diff.mismatches.iter().any(|mismatch| mismatch.path == "root/button"
        && mismatch.field == "data.visual.content_rect.x"
        && mismatch.delta.is_some()));
}

#[test]
fn visual_tree_diff_keeps_nested_non_visual_numbers_exact() {
    let expected = snapshot_with_nested_non_visual_metric(1.0);
    let actual = snapshot_with_nested_non_visual_metric(1.01);

    let diff = compare_visual_tree_snapshots(&expected, &actual, 0.75);

    assert!(!diff.passed);
    assert!(diff.mismatches.iter().any(|mismatch| mismatch.path == "root/button"
        && mismatch.field == "data.stats.score"
        && mismatch.delta.is_some()));
}

#[test]
fn visual_tree_diff_allows_candidate_data_to_extend_reference_data() {
    let expected = snapshot(RectF::new(10.0, 20.0, 30.0, 40.0), "demo");
    let mut actual = snapshot(RectF::new(10.0, 20.0, 30.0, 40.0), "demo");
    actual.root.children[0].data.insert("implementation".to_owned(), serde_json::json!("rust"));

    let diff = compare_visual_tree_snapshots(&expected, &actual, 0.25);

    assert!(diff.passed);
    assert!(diff.mismatches.is_empty());
}

#[test]
fn visual_tree_diff_skips_parity_ignored_diagnostic_subtrees() {
    let mut expected = snapshot(RectF::new(10.0, 20.0, 30.0, 40.0), "demo");
    expected.root.push_child(
        VisualTreeNode::new("root/source_tree", "Diagnostics", RectF::new(0.0, 0.0, 1.0, 1.0))
            .with_data("parity_ignore", true),
    );
    let actual = snapshot(RectF::new(10.0, 20.0, 30.0, 40.0), "demo");

    let diff = compare_visual_tree_snapshots(&expected, &actual, 0.25);

    assert!(diff.passed);
    assert!(diff.mismatches.is_empty());
}

fn action_descriptor(node: &VisualTreeNode) -> Option<VisualTreeActionDescriptor> {
    if node.role.as_deref() != Some("button") {
        return None;
    }
    let mut data = BTreeMap::new();
    if let Some(target) = node.data.get("target").cloned() {
        data.insert("target".to_owned(), target);
    }
    Some(VisualTreeActionDescriptor {
        action_type: "navigate".to_owned(),
        action_key: node.data.get("target").and_then(|value| value.as_str()).map(ToOwned::to_owned),
        data,
    })
}

#[test]
fn visual_tree_action_graph_extracts_actions_and_skips_ignored_nodes() {
    let mut snap = snapshot(RectF::new(10.0, 20.0, 30.0, 40.0), "demo");
    snap.root.children[0].data.insert("target".to_owned(), serde_json::json!("people"));
    let mut ignored =
        VisualTreeNode::new("root/source", "Diagnostics", RectF::new(0.0, 0.0, 1.0, 1.0))
            .with_data("parity_ignore", true);
    ignored.push_child(
        VisualTreeNode::new("root/source/button", "Button", RectF::new(0.0, 0.0, 1.0, 1.0))
            .with_role("button")
            .with_data("target", "ignored"),
    );
    snap.root.push_child(ignored);

    let graph = build_visual_tree_action_graph(
        snap.producer.clone(),
        snap.scene.clone(),
        snap.route.clone(),
        snap.preset.clone(),
        "snapshot.json",
        [(0, &snap)],
        action_descriptor,
    );

    assert_eq!(graph.actions.len(), 1);
    assert_eq!(graph.actions[0].path, "root/button");
    assert_eq!(graph.actions[0].action_type, "navigate");
    assert_eq!(graph.actions[0].action_key.as_deref(), Some("people"));
}

#[test]
fn visual_tree_action_graph_diff_reports_missing_candidate_action() {
    let mut expected = snapshot(RectF::new(10.0, 20.0, 30.0, 40.0), "demo");
    expected.root.children[0].data.insert("target".to_owned(), serde_json::json!("people"));
    let mut actual = snapshot(RectF::new(10.0, 20.0, 30.0, 40.0), "demo");
    actual.root.children.clear();

    let expected_graph = build_visual_tree_action_graph(
        expected.producer.clone(),
        expected.scene.clone(),
        expected.route.clone(),
        expected.preset.clone(),
        "expected.json",
        [(0, &expected)],
        action_descriptor,
    );
    let actual_graph = build_visual_tree_action_graph(
        actual.producer.clone(),
        actual.scene.clone(),
        actual.route.clone(),
        actual.preset.clone(),
        "actual.json",
        [(0, &actual)],
        action_descriptor,
    );

    let diff = compare_visual_tree_action_graphs(&expected_graph, &actual_graph, 0.25);

    assert!(!diff.passed);
    assert!(diff.mismatches.iter().any(|mismatch| mismatch.contains("missing candidate action")));
}

#[test]
fn visual_tree_action_graph_diff_allows_candidate_to_add_action_key() {
    let expected = snapshot(RectF::new(10.0, 20.0, 30.0, 40.0), "demo");
    let mut actual = snapshot(RectF::new(10.0, 20.0, 30.0, 40.0), "demo");
    actual.root.children[0].data.insert("target".to_owned(), serde_json::json!("people"));

    let expected_graph = build_visual_tree_action_graph(
        expected.producer.clone(),
        expected.scene.clone(),
        expected.route.clone(),
        expected.preset.clone(),
        "expected.json",
        [(0, &expected)],
        action_descriptor,
    );
    let actual_graph = build_visual_tree_action_graph(
        actual.producer.clone(),
        actual.scene.clone(),
        actual.route.clone(),
        actual.preset.clone(),
        "actual.json",
        [(0, &actual)],
        action_descriptor,
    );

    let diff = compare_visual_tree_action_graphs(&expected_graph, &actual_graph, 0.25);

    assert!(diff.passed, "{:?}", diff.mismatches);
}

#[test]
fn visual_tree_action_observation_lookup_selects_requested_step() {
    let mut step_0 = snapshot(RectF::new(1.0, 2.0, 3.0, 4.0), "demo");
    step_0.root.children[0].data.insert("target".to_owned(), serde_json::json!("people"));
    let mut step_2 = snapshot(RectF::new(5.0, 6.0, 7.0, 8.0), "demo");
    step_2.root.children[0].data.insert("target".to_owned(), serde_json::json!("people"));
    let graph = build_visual_tree_action_graph(
        step_0.producer.clone(),
        step_0.scene.clone(),
        step_0.route.clone(),
        step_0.preset.clone(),
        "sequence.json",
        [(0, &step_0), (2, &step_2)],
        action_descriptor,
    );

    let observation = visual_tree_action_observation_for_path(&graph, "root/button", Some(2))
        .expect("step 2 observation");
    assert_eq!(observation.center(), (8.5, 10.0));
    assert!(visual_tree_action_observation_for_path(&graph, "root/button", Some(1)).is_none());
}

#[test]
fn visual_tree_action_observation_lookup_defaults_to_earliest_step() {
    let mut step_0 = snapshot(RectF::new(1.0, 2.0, 3.0, 4.0), "demo");
    step_0.root.children[0].data.insert("target".to_owned(), serde_json::json!("people"));
    let mut step_2 = snapshot(RectF::new(5.0, 6.0, 7.0, 8.0), "demo");
    step_2.root.children[0].data.insert("target".to_owned(), serde_json::json!("people"));
    let graph = build_visual_tree_action_graph(
        step_0.producer.clone(),
        step_0.scene.clone(),
        step_0.route.clone(),
        step_0.preset.clone(),
        "sequence.json",
        [(2, &step_2), (0, &step_0)],
        action_descriptor,
    );

    let observation =
        visual_tree_action_observation_for_path(&graph, "root/button", None).expect("observation");
    assert_eq!(observation.step_index, 0);
    assert_eq!(observation.center(), (2.5, 4.0));
}

#[test]
fn visual_tree_node_by_path_finds_nested_nodes() {
    let mut snap = snapshot(RectF::new(10.0, 20.0, 30.0, 40.0), "demo");
    snap.root.children[0].push_child(VisualTreeNode::new(
        "root/button/icon",
        "Icon",
        RectF::new(12.0, 22.0, 8.0, 8.0),
    ));

    let node = visual_tree_node_by_path(&snap.root, "root/button/icon").expect("nested node");
    assert_eq!(node.kind, "Icon");
    assert!(visual_tree_node_by_path(&snap.root, "root/missing").is_none());
}

#[test]
fn visual_tree_action_observation_marks_transition_nodes_unactionable_for_replay() {
    let observation = observation(0, VisualTreeRect { x: 0.0, y: 0.0, w: 5.0, h: 9.0 }, 0.0);

    assert_eq!(observation.replay_unactionable_reason(), Some("opacity_below_replay_threshold"));
    assert!(!observation.is_replay_actionable());
}

#[test]
fn visual_tree_action_observation_allows_zero_extent_targets_when_center_is_finite() {
    let observation =
        observation(0, VisualTreeRect { x: 187.66667, y: 303.0, w: 0.0, h: 0.0 }, 1.0);

    assert_eq!(observation.center(), (187.66667, 303.0));
    assert_eq!(observation.replay_unactionable_reason(), None);
    assert!(observation.is_replay_actionable());
}

#[test]
fn visual_tree_action_observation_allows_stable_visible_replay_targets() {
    let observation = observation(0, VisualTreeRect { x: 10.0, y: 20.0, w: 24.0, h: 24.0 }, 0.50);

    assert_eq!(observation.replay_unactionable_reason(), None);
    assert!(observation.is_replay_actionable());
}

#[test]
fn visual_tree_action_observation_allows_nametag_skipped_manifest_targets() {
    let replay_targets = [
        (
            "root/account/login/forgot",
            observation(0, VisualTreeRect { x: 187.66667, y: 303.0, w: 0.0, h: 0.0 }, 1.0),
        ),
        (
            "root/mission_control/panel/radar/content/card_0",
            observation(
                3,
                VisualTreeRect { x: 178.20905, y: 117.0, w: 145.45198, h: 271.0 },
                0.00027457738,
            ),
        ),
        (
            "root/mission_control/panel/radar/content/card_2",
            observation(
                2,
                VisualTreeRect { x: 90.768364, y: 703.0, w: 5.0273438, h: 9.0 },
                0.001780273,
            ),
        ),
        (
            "root/mission_control/panel/radar/content/card_3",
            observation(
                2,
                VisualTreeRect { x: 289.31967, y: 752.0, w: 5.0273438, h: 9.0 },
                0.00000814721,
            ),
        ),
    ];

    for (path, observation) in replay_targets {
        assert_eq!(
            observation.replay_unactionable_reason(),
            None,
            "{} should stay replayable",
            path
        );
        assert!(observation.is_replay_actionable(), "{} should stay actionable", path);
        assert!(observation.center().0.is_finite());
        assert!(observation.center().1.is_finite());
    }
}

#[test]
fn visual_tree_action_graph_uses_effective_opacity_for_replay_targets() {
    let snapshot = nested_action_snapshot(0.01, 1.0);
    let graph = build_visual_tree_action_graph(
        snapshot.producer.clone(),
        snapshot.scene.clone(),
        snapshot.route.clone(),
        snapshot.preset.clone(),
        "snapshot.json",
        [(0, &snapshot)],
        action_descriptor,
    );

    let observation =
        visual_tree_action_observation_for_path(&graph, "root/container/button", None)
            .expect("nested observation");
    assert_eq!(observation.opacity, 0.01);
    assert_eq!(observation.replay_unactionable_reason(), None);
}

#[test]
fn visual_tree_action_graph_manifest_collects_replay_plan_and_coverage() {
    let mut front_page = snapshot(RectF::new(10.0, 20.0, 30.0, 40.0), "demo");
    front_page.route = "account::front_page".to_owned();
    front_page.root.children[0]
        .data
        .insert("target".to_owned(), serde_json::json!("account::login"));
    let front_page_graph = build_visual_tree_action_graph(
        front_page.producer.clone(),
        front_page.scene.clone(),
        front_page.route.clone(),
        Some("demo_on".to_owned()),
        "front-page.json",
        [(0, &front_page)],
        action_descriptor,
    );

    let mut login = snapshot(RectF::new(1.0, 2.0, 24.0, 24.0), "demo");
    login.route = "account::login".to_owned();
    login.root.children[0]
        .data
        .insert("target".to_owned(), serde_json::json!("mission_control::radar"));
    login.root.children[0].visible = false;
    let login_graph = build_visual_tree_action_graph(
        login.producer.clone(),
        login.scene.clone(),
        login.route.clone(),
        Some("demo_on".to_owned()),
        "login.json",
        [(0, &login)],
        action_descriptor,
    );

    let manifest = build_visual_tree_action_graph_manifest(
        "old-ios",
        "old-ios",
        "account::front_page",
        true,
        ["account::front_page", "account::login", "mission_control::radar"],
        ["navigate"],
        vec![front_page_graph, login_graph],
    );

    assert_eq!(manifest.schema, VISUAL_TREE_ACTION_GRAPH_MANIFEST_SCHEMA_VERSION);
    assert!(!manifest.pass);
    assert_eq!(
        manifest.route_order,
        vec!["account::front_page".to_owned(), "account::login".to_owned()]
    );
    assert_eq!(manifest.missing_routes, vec!["mission_control::radar".to_owned()]);
    assert_eq!(manifest.observed_action_count, 2);
    assert_eq!(manifest.replay_plan.len(), 2);
    assert_eq!(manifest.replay_plan[0].path, "root/button");
    assert_eq!(
        manifest.replay_plan[0].animation_trace,
        default_visual_tree_action_animation_trace("root/button", "navigate")
    );
    assert_eq!(manifest.replay_plan[1].replay_unactionable_reason.as_deref(), Some("not_visible"));
    assert_eq!(manifest.route_action_counts.get("account::login"), Some(&1_usize));
}

#[test]
fn default_visual_tree_action_animation_trace_marks_mission_control_chrome() {
    let plan = default_visual_tree_action_animation_trace(
        "root/mission_control/top_chrome/people",
        "navigate",
    )
    .expect("animation trace plan");

    assert_eq!(plan.sample_offsets_ms, vec![0, 83, 166, 249, 332, 415, 700]);
    assert!(default_visual_tree_action_animation_trace(
        "root/account/front_page/login",
        "submit_account"
    )
    .is_none());
}

#[test]
fn visual_tree_sequence_diff_reports_time_and_snapshot_mismatches() {
    let reference = VisualTreeSequence {
        schema: VISUAL_TREE_SEQUENCE_SCHEMA_VERSION.to_owned(),
        producer: "old-ios".to_owned(),
        scene: "mission_control".to_owned(),
        route: "mission_control::people".to_owned(),
        preset: Some("demo_on".to_owned()),
        kind: Some("action_animation_trace".to_owned()),
        steps: vec![VisualTreeSequenceStep {
            index: 0,
            requested_scroll_y: None,
            effective_scroll_y: None,
            time_ms: Some(83),
            data: BTreeMap::new(),
            snapshot: snapshot(RectF::new(10.0, 20.0, 30.0, 40.0), "demo"),
        }],
    };
    let candidate = VisualTreeSequence {
        schema: VISUAL_TREE_SEQUENCE_SCHEMA_VERSION.to_owned(),
        producer: "rust".to_owned(),
        scene: "mission_control".to_owned(),
        route: "mission_control::people".to_owned(),
        preset: Some("demo_on".to_owned()),
        kind: Some("action_animation_trace".to_owned()),
        steps: vec![VisualTreeSequenceStep {
            index: 0,
            requested_scroll_y: None,
            effective_scroll_y: None,
            time_ms: Some(100),
            data: BTreeMap::new(),
            snapshot: snapshot(RectF::new(12.0, 20.0, 30.0, 40.0), "demo"),
        }],
    };

    let diff = compare_visual_tree_sequences(&reference, &candidate, 0.25);

    assert!(!diff.passed);
    assert!(diff.mismatches.iter().any(|mismatch| mismatch.contains("time_ms mismatch")));
    assert!(diff.steps.first().is_some_and(|step| step
        .mismatches
        .iter()
        .any(|mismatch| mismatch.contains("snapshot mismatch count"))));
}

#[test]
fn visual_tree_sequence_diff_reports_sampled_opacity_mismatch() {
    let reference = VisualTreeSequence {
        schema: VISUAL_TREE_SEQUENCE_SCHEMA_VERSION.to_owned(),
        producer: "old-ios".to_owned(),
        scene: "mission_control".to_owned(),
        route: "mission_control::people".to_owned(),
        preset: Some("demo_on".to_owned()),
        kind: Some("action_animation_trace".to_owned()),
        steps: vec![VisualTreeSequenceStep {
            index: 0,
            requested_scroll_y: None,
            effective_scroll_y: None,
            time_ms: Some(83),
            data: BTreeMap::new(),
            snapshot: snapshot_with_button_opacity(
                RectF::new(10.0, 20.0, 30.0, 40.0),
                "demo",
                0.25,
            ),
        }],
    };
    let candidate = VisualTreeSequence {
        schema: VISUAL_TREE_SEQUENCE_SCHEMA_VERSION.to_owned(),
        producer: "rust".to_owned(),
        scene: "mission_control".to_owned(),
        route: "mission_control::people".to_owned(),
        preset: Some("demo_on".to_owned()),
        kind: Some("action_animation_trace".to_owned()),
        steps: vec![VisualTreeSequenceStep {
            index: 0,
            requested_scroll_y: None,
            effective_scroll_y: None,
            time_ms: Some(83),
            data: BTreeMap::new(),
            snapshot: snapshot_with_button_opacity(
                RectF::new(10.0, 20.0, 30.0, 40.0),
                "demo",
                0.55,
            ),
        }],
    };

    let diff = compare_visual_tree_sequences(&reference, &candidate, 0.75);

    assert!(!diff.passed);
    assert!(diff.steps.first().is_some_and(|step| step
        .diff
        .mismatches
        .iter()
        .any(|mismatch| mismatch.path == "root/button" && mismatch.field == "opacity")));
}
