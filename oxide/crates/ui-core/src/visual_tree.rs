use oxide_renderer_api::{Insets, RectF};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const VISUAL_TREE_SCHEMA_VERSION: &str = "oxide.visual_tree.v1";
pub const VISUAL_TREE_SEQUENCE_SCHEMA_VERSION: &str = "oxide.visual_tree.sequence.v1";
pub const VISUAL_TREE_ACTION_GRAPH_SCHEMA_VERSION: &str = "oxide.visual_tree.action_graph.v1";
pub const VISUAL_TREE_ACTION_GRAPH_MANIFEST_SCHEMA_VERSION: &str =
    "oxide.visual_tree.action_graph_manifest.v1";
pub const VISUAL_TREE_REPLAY_MIN_ACTION_EXTENT: f32 = 8.0;
pub const VISUAL_TREE_REPLAY_MIN_ACTION_OPACITY: f32 = 0.05;
const VISUAL_TREE_DATA_NUMBER_TOLERANCE: f64 = 1.0e-6;
const VISUAL_TREE_OPACITY_TOLERANCE: f32 = 0.01;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct VisualTreeRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl VisualTreeRect {
    #[must_use]
    pub fn from_rect(rect: RectF) -> Self {
        Self { x: rect.x, y: rect.y, w: rect.w, h: rect.h }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct VisualTreeInsets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl VisualTreeInsets {
    #[must_use]
    pub fn from_insets(insets: Insets) -> Self {
        Self { left: insets.left, top: insets.top, right: insets.right, bottom: insets.bottom }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualTreeViewport {
    pub frame: VisualTreeRect,
    pub safe: VisualTreeInsets,
    pub points_scale: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualTreeSnapshot {
    pub schema: String,
    pub producer: String,
    pub scene: String,
    pub route: String,
    pub preset: Option<String>,
    pub viewport: VisualTreeViewport,
    pub root: VisualTreeNode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualTreeNode {
    pub path: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    pub frame: VisualTreeRect,
    pub visible: bool,
    pub opacity: f32,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub data: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<VisualTreeNode>,
}

impl VisualTreeNode {
    #[must_use]
    pub fn new(path: impl Into<String>, kind: impl Into<String>, frame: RectF) -> Self {
        Self {
            path: path.into(),
            kind: kind.into(),
            role: None,
            frame: VisualTreeRect::from_rect(frame),
            visible: true,
            opacity: 1.0,
            data: BTreeMap::new(),
            children: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        self.role = Some(role.into());
        self
    }

    #[must_use]
    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    #[must_use]
    pub fn with_data(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.data.insert(key.into(), value.into());
        self
    }

    pub fn push_child(&mut self, child: VisualTreeNode) {
        self.children.push(child);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualTreeDiff {
    pub passed: bool,
    pub tolerance_points: f32,
    pub mismatches: Vec<VisualTreeMismatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualTreeMismatch {
    pub path: String,
    pub field: String,
    pub expected: Value,
    pub actual: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualTreeActionObservation {
    pub step_index: usize,
    pub scene: String,
    pub route: String,
    pub preset: Option<String>,
    pub frame: VisualTreeRect,
    pub visible: bool,
    pub opacity: f32,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub data: BTreeMap<String, Value>,
}

impl VisualTreeActionObservation {
    #[must_use]
    pub fn center(&self) -> (f32, f32) {
        (self.frame.x + self.frame.w * 0.50, self.frame.y + self.frame.h * 0.50)
    }

    #[must_use]
    pub fn replay_unactionable_reason(&self) -> Option<&'static str> {
        if !self.visible {
            return Some("not_visible");
        }
        if !self.opacity.is_finite() || self.opacity <= 0.0 {
            return Some("opacity_below_replay_threshold");
        }
        if !self.frame.w.is_finite() || self.frame.w < 0.0 {
            return Some("width_below_replay_threshold");
        }
        if !self.frame.h.is_finite() || self.frame.h < 0.0 {
            return Some("height_below_replay_threshold");
        }
        None
    }

    #[must_use]
    pub fn is_replay_actionable(&self) -> bool {
        self.replay_unactionable_reason().is_none()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualTreeActionNode {
    pub path: String,
    pub kind: String,
    pub role: Option<String>,
    pub action_type: String,
    pub action_key: Option<String>,
    pub observations: Vec<VisualTreeActionObservation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualTreeActionGraph {
    pub schema: String,
    pub producer: String,
    pub scene: String,
    pub route: String,
    pub preset: Option<String>,
    pub source: String,
    pub actions: Vec<VisualTreeActionNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualTreeActionReplayPlanStep {
    pub route: String,
    pub path: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    pub action_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_key: Option<String>,
    pub observation_step: usize,
    pub scene: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    pub frame: VisualTreeRect,
    pub visible: bool,
    pub opacity: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_unactionable_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub animation_trace: Option<VisualTreeActionAnimationTracePlan>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualTreeActionAnimationTracePlan {
    pub sample_offsets_ms: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualTreeActionGraphManifest {
    pub schema: String,
    pub producer: String,
    pub source_truth: String,
    pub root_route: String,
    pub demo_mode_required: bool,
    pub pass: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_routes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observed_routes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_routes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_action_types: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observed_action_types: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_action_types: Vec<String>,
    pub observed_action_count: usize,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub route_action_counts: BTreeMap<String, usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub route_order: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replay_plan: Vec<VisualTreeActionReplayPlanStep>,
    pub graphs: Vec<VisualTreeActionGraph>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualTreeActionGraphDiff {
    pub passed: bool,
    pub tolerance_points: f32,
    pub mismatches: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualTreeSequenceStep {
    pub index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_scroll_y: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_scroll_y: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub data: BTreeMap<String, Value>,
    pub snapshot: VisualTreeSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualTreeSequence {
    pub schema: String,
    pub producer: String,
    pub scene: String,
    pub route: String,
    pub preset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub steps: Vec<VisualTreeSequenceStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualTreeSequenceStepDiff {
    pub index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_requested_scroll_y: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_requested_scroll_y: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_effective_scroll_y: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_effective_scroll_y: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_time_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_time_ms: Option<u64>,
    pub reference_node_count: usize,
    pub candidate_node_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mismatches: Vec<String>,
    pub diff: VisualTreeDiff,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualTreeSequenceDiff {
    pub passed: bool,
    pub tolerance_points: f32,
    pub mismatches: Vec<String>,
    pub steps: Vec<VisualTreeSequenceStepDiff>,
}

struct ComparableVisualTreeNode<'a> {
    node: &'a VisualTreeNode,
    effective_opacity: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisualTreeActionDescriptor {
    pub action_type: String,
    pub action_key: Option<String>,
    pub data: BTreeMap<String, Value>,
}

impl VisualTreeActionDescriptor {
    #[must_use]
    pub fn new(action_type: impl Into<String>) -> Self {
        Self { action_type: action_type.into(), action_key: None, data: BTreeMap::new() }
    }

    #[must_use]
    pub fn with_action_key(mut self, action_key: impl Into<String>) -> Self {
        self.action_key = Some(action_key.into());
        self
    }

    #[must_use]
    pub fn with_data(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.data.insert(key.into(), value.into());
        self
    }
}

#[must_use]
pub fn default_visual_tree_action_animation_trace(
    path: &str,
    action_type: &str,
) -> Option<VisualTreeActionAnimationTracePlan> {
    let sample_offsets_ms = if path.starts_with("root/mission_control/top_chrome/")
        || path.starts_with("root/mission_control/radar_chrome/")
        || path.starts_with("root/mission_control/panel/foundation/carousel/")
        || path == "root/simulation_preview_toggle"
        || action_type == "navigate"
    {
        vec![0, 83, 166, 249, 332, 415, 700]
    } else {
        Vec::new()
    };
    (!sample_offsets_ms.is_empty())
        .then_some(VisualTreeActionAnimationTracePlan { sample_offsets_ms })
}

#[must_use]
pub fn build_visual_tree_action_graph<'a, I, F>(
    producer: impl Into<String>,
    scene: impl Into<String>,
    route: impl Into<String>,
    preset: Option<String>,
    source: impl Into<String>,
    snapshots: I,
    mut classify: F,
) -> VisualTreeActionGraph
where
    I: IntoIterator<Item = (usize, &'a VisualTreeSnapshot)>,
    F: FnMut(&VisualTreeNode) -> Option<VisualTreeActionDescriptor>,
{
    let mut actions = BTreeMap::<String, VisualTreeActionNode>::new();
    for (step_index, snapshot) in snapshots {
        collect_visual_tree_action_nodes(
            &snapshot.root,
            step_index,
            snapshot.scene.as_str(),
            snapshot.route.as_str(),
            snapshot.preset.as_ref(),
            1.0,
            &mut actions,
            &mut classify,
        );
    }
    VisualTreeActionGraph {
        schema: VISUAL_TREE_ACTION_GRAPH_SCHEMA_VERSION.to_owned(),
        producer: producer.into(),
        scene: scene.into(),
        route: route.into(),
        preset,
        source: source.into(),
        actions: actions.into_values().collect(),
    }
}

#[must_use]
pub fn build_visual_tree_action_graph_manifest<I, R, A>(
    producer: impl Into<String>,
    source_truth: impl Into<String>,
    root_route: impl Into<String>,
    demo_mode_required: bool,
    required_routes: impl IntoIterator<Item = R>,
    required_action_types: impl IntoIterator<Item = A>,
    graphs: I,
) -> VisualTreeActionGraphManifest
where
    I: IntoIterator<Item = VisualTreeActionGraph>,
    R: Into<String>,
    A: Into<String>,
{
    let graphs: Vec<_> = graphs.into_iter().collect();
    let required_routes: Vec<_> = required_routes.into_iter().map(Into::into).collect();
    let required_action_types: Vec<_> = required_action_types.into_iter().map(Into::into).collect();
    let mut observed_routes = BTreeSet::<String>::new();
    let mut observed_action_types = BTreeSet::<String>::new();
    let mut observed_actions = BTreeSet::<(String, String)>::new();
    let mut route_action_counts = BTreeMap::<String, usize>::new();
    let mut route_order = Vec::with_capacity(graphs.len());
    let mut replay_plan = Vec::new();

    for graph in graphs.iter() {
        route_order.push(graph.route.clone());
        observed_routes.insert(graph.route.clone());
        for action in graph.actions.iter() {
            observed_action_types.insert(action.action_type.clone());
            observed_actions.insert((graph.route.clone(), action.path.clone()));
            for observation in action.observations.iter() {
                observed_routes.insert(observation.route.clone());
                *route_action_counts.entry(observation.route.clone()).or_insert(0) += 1;
                replay_plan.push(VisualTreeActionReplayPlanStep {
                    route: observation.route.clone(),
                    path: action.path.clone(),
                    kind: action.kind.clone(),
                    role: action.role.clone(),
                    action_type: action.action_type.clone(),
                    action_key: action.action_key.clone(),
                    observation_step: observation.step_index,
                    scene: observation.scene.clone(),
                    preset: observation.preset.clone(),
                    frame: observation.frame,
                    visible: observation.visible,
                    opacity: observation.opacity,
                    replay_unactionable_reason: observation
                        .replay_unactionable_reason()
                        .map(str::to_owned),
                    animation_trace: default_visual_tree_action_animation_trace(
                        action.path.as_str(),
                        action.action_type.as_str(),
                    ),
                });
            }
        }
    }

    let missing_routes: Vec<_> = required_routes
        .iter()
        .filter(|route| !observed_routes.contains(route.as_str()))
        .cloned()
        .collect();
    let missing_action_types: Vec<_> = required_action_types
        .iter()
        .filter(|action_type| !observed_action_types.contains(action_type.as_str()))
        .cloned()
        .collect();

    VisualTreeActionGraphManifest {
        schema: VISUAL_TREE_ACTION_GRAPH_MANIFEST_SCHEMA_VERSION.to_owned(),
        producer: producer.into(),
        source_truth: source_truth.into(),
        root_route: root_route.into(),
        demo_mode_required,
        pass: missing_routes.is_empty() && missing_action_types.is_empty(),
        required_routes,
        observed_routes: observed_routes.into_iter().collect(),
        missing_routes,
        required_action_types,
        observed_action_types: observed_action_types.into_iter().collect(),
        missing_action_types,
        observed_action_count: observed_actions.len(),
        route_action_counts,
        route_order,
        replay_plan,
        graphs,
    }
}

#[must_use]
pub fn compare_visual_tree_action_graphs(
    reference: &VisualTreeActionGraph,
    candidate: &VisualTreeActionGraph,
    tolerance_points: f32,
) -> VisualTreeActionGraphDiff {
    let mut mismatches = Vec::new();
    if reference.scene != candidate.scene {
        mismatches.push(format!(
            "scene mismatch: reference={} candidate={}",
            reference.scene, candidate.scene
        ));
    }
    if reference.route != candidate.route {
        mismatches.push(format!(
            "route mismatch: reference={} candidate={}",
            reference.route, candidate.route
        ));
    }
    if reference.preset != candidate.preset {
        mismatches.push(format!(
            "preset mismatch: reference={:?} candidate={:?}",
            reference.preset, candidate.preset
        ));
    }

    let reference_actions: BTreeMap<&str, &VisualTreeActionNode> =
        reference.actions.iter().map(|action| (action.path.as_str(), action)).collect();
    let candidate_actions: BTreeMap<&str, &VisualTreeActionNode> =
        candidate.actions.iter().map(|action| (action.path.as_str(), action)).collect();
    for path in reference_actions.keys() {
        if !candidate_actions.contains_key(path) {
            mismatches.push(format!("missing candidate action: {}", path));
        }
    }
    for path in candidate_actions.keys() {
        if !reference_actions.contains_key(path) {
            mismatches.push(format!("extra candidate action: {}", path));
        }
    }
    for (path, reference_action) in reference_actions.iter() {
        let Some(candidate_action) = candidate_actions.get(path) else {
            continue;
        };
        compare_visual_tree_action_node(
            path,
            reference_action,
            candidate_action,
            tolerance_points,
            &mut mismatches,
        );
    }

    VisualTreeActionGraphDiff { passed: mismatches.is_empty(), tolerance_points, mismatches }
}

#[must_use]
pub fn visual_tree_action_observation_for_path<'a>(
    graph: &'a VisualTreeActionGraph,
    action_path: &str,
    observation_step: Option<usize>,
) -> Option<&'a VisualTreeActionObservation> {
    let action = graph.actions.iter().find(|action| action.path == action_path)?;
    if let Some(step) = observation_step {
        return action.observations.iter().find(|observation| observation.step_index == step);
    }
    action.observations.iter().min_by_key(|observation| observation.step_index)
}

#[must_use]
pub fn visual_tree_node_by_path<'a>(
    root: &'a VisualTreeNode,
    path: &str,
) -> Option<&'a VisualTreeNode> {
    if root.path == path {
        return Some(root);
    }
    root.children.iter().find_map(|child| visual_tree_node_by_path(child, path))
}

fn collect_visual_tree_action_nodes<F>(
    node: &VisualTreeNode,
    step_index: usize,
    scene: &str,
    route: &str,
    preset: Option<&String>,
    inherited_opacity: f32,
    actions: &mut BTreeMap<String, VisualTreeActionNode>,
    classify: &mut F,
) where
    F: FnMut(&VisualTreeNode) -> Option<VisualTreeActionDescriptor>,
{
    if parity_ignored(node) {
        return;
    }
    let effective_opacity = inherited_opacity * node.opacity;
    if let Some(descriptor) = classify(node) {
        let entry = actions.entry(node.path.clone()).or_insert_with(|| VisualTreeActionNode {
            path: node.path.clone(),
            kind: node.kind.clone(),
            role: node.role.clone(),
            action_type: descriptor.action_type.clone(),
            action_key: descriptor.action_key.clone(),
            observations: Vec::new(),
        });
        entry.observations.push(VisualTreeActionObservation {
            step_index,
            scene: scene.to_owned(),
            route: route.to_owned(),
            preset: preset.cloned(),
            frame: node.frame,
            visible: node.visible,
            opacity: effective_opacity,
            data: descriptor.data,
        });
    }
    for child in node.children.iter() {
        collect_visual_tree_action_nodes(
            child,
            step_index,
            scene,
            route,
            preset,
            effective_opacity,
            actions,
            classify,
        );
    }
}

fn compare_visual_tree_action_node(
    path: &str,
    reference: &VisualTreeActionNode,
    candidate: &VisualTreeActionNode,
    tolerance_points: f32,
    mismatches: &mut Vec<String>,
) {
    if reference.kind != candidate.kind {
        mismatches.push(format!(
            "{} kind mismatch: reference={} candidate={}",
            path, reference.kind, candidate.kind
        ));
    }
    if reference.role != candidate.role {
        mismatches.push(format!(
            "{} role mismatch: reference={:?} candidate={:?}",
            path, reference.role, candidate.role
        ));
    }
    if reference.action_type != candidate.action_type {
        mismatches.push(format!(
            "{} action_type mismatch: reference={} candidate={}",
            path, reference.action_type, candidate.action_type
        ));
    }
    if reference.action_key.is_some() && reference.action_key != candidate.action_key {
        mismatches.push(format!(
            "{} action_key mismatch: reference={:?} candidate={:?}",
            path, reference.action_key, candidate.action_key
        ));
    }
    compare_visual_tree_action_observations(
        path,
        reference,
        candidate,
        tolerance_points,
        mismatches,
    );
}

fn compare_visual_tree_action_observations(
    path: &str,
    reference: &VisualTreeActionNode,
    candidate: &VisualTreeActionNode,
    tolerance_points: f32,
    mismatches: &mut Vec<String>,
) {
    let reference_steps: BTreeMap<usize, &VisualTreeActionObservation> = reference
        .observations
        .iter()
        .map(|observation| (observation.step_index, observation))
        .collect();
    let candidate_steps: BTreeMap<usize, &VisualTreeActionObservation> = candidate
        .observations
        .iter()
        .map(|observation| (observation.step_index, observation))
        .collect();
    for step in reference_steps.keys() {
        if !candidate_steps.contains_key(step) {
            mismatches.push(format!("{} missing candidate observation step {}", path, step));
        }
    }
    for step in candidate_steps.keys() {
        if !reference_steps.contains_key(step) {
            mismatches.push(format!("{} extra candidate observation step {}", path, step));
        }
    }
    for (step, reference_observation) in reference_steps.iter() {
        let Some(candidate_observation) = candidate_steps.get(step) else {
            continue;
        };
        compare_visual_tree_action_observation(
            path,
            *step,
            reference_observation,
            candidate_observation,
            tolerance_points,
            mismatches,
        );
    }
}

fn compare_visual_tree_action_observation(
    path: &str,
    step: usize,
    reference: &VisualTreeActionObservation,
    candidate: &VisualTreeActionObservation,
    tolerance_points: f32,
    mismatches: &mut Vec<String>,
) {
    let fields = [
        ("x", reference.frame.x, candidate.frame.x),
        ("y", reference.frame.y, candidate.frame.y),
        ("w", reference.frame.w, candidate.frame.w),
        ("h", reference.frame.h, candidate.frame.h),
    ];
    if reference.scene != candidate.scene {
        mismatches.push(format!(
            "{} step {} scene mismatch: reference={} candidate={}",
            path, step, reference.scene, candidate.scene
        ));
    }
    if reference.route != candidate.route {
        mismatches.push(format!(
            "{} step {} route mismatch: reference={} candidate={}",
            path, step, reference.route, candidate.route
        ));
    }
    if reference.preset != candidate.preset {
        mismatches.push(format!(
            "{} step {} preset mismatch: reference={:?} candidate={:?}",
            path, step, reference.preset, candidate.preset
        ));
    }
    for (field, reference_value, candidate_value) in fields {
        if (reference_value - candidate_value).abs() > tolerance_points {
            mismatches.push(format!(
                "{} step {} {} mismatch: reference={} candidate={}",
                path, step, field, reference_value, candidate_value
            ));
        }
    }
    if (reference.opacity - candidate.opacity).abs() > 0.01 {
        mismatches.push(format!(
            "{} step {} opacity mismatch: reference={} candidate={}",
            path, step, reference.opacity, candidate.opacity
        ));
    }
    if reference.visible != candidate.visible {
        mismatches.push(format!(
            "{} step {} visible mismatch: reference={} candidate={}",
            path, step, reference.visible, candidate.visible
        ));
    }
    for (key, reference_value) in reference.data.iter() {
        match candidate.data.get(key) {
            Some(candidate_value)
                if visual_tree_data_values_match(
                    format!("data.{key}").as_str(),
                    reference_value,
                    candidate_value,
                    tolerance_points,
                ) => {}
            Some(candidate_value) => mismatches.push(format!(
                "{} step {} data.{} mismatch: reference={} candidate={}",
                path, step, key, reference_value, candidate_value
            )),
            None => mismatches.push(format!("{} step {} missing data.{}", path, step, key)),
        }
    }
}

#[must_use]
pub fn compare_visual_tree_snapshots(
    reference: &VisualTreeSnapshot,
    candidate: &VisualTreeSnapshot,
    tolerance_points: f32,
) -> VisualTreeDiff {
    let mut mismatches = Vec::new();
    compare_snapshot_field(
        &mut mismatches,
        "$",
        "schema",
        Value::String(reference.schema.clone()),
        Value::String(candidate.schema.clone()),
    );
    compare_snapshot_field(
        &mut mismatches,
        "$",
        "scene",
        Value::String(reference.scene.clone()),
        Value::String(candidate.scene.clone()),
    );
    compare_snapshot_field(
        &mut mismatches,
        "$",
        "route",
        Value::String(reference.route.clone()),
        Value::String(candidate.route.clone()),
    );
    compare_snapshot_field(
        &mut mismatches,
        "$",
        "preset",
        serde_json::to_value(reference.preset.as_ref()).unwrap_or(Value::Null),
        serde_json::to_value(candidate.preset.as_ref()).unwrap_or(Value::Null),
    );
    compare_rect(
        &mut mismatches,
        "$.viewport",
        reference.viewport.frame,
        candidate.viewport.frame,
        tolerance_points,
    );
    compare_insets(
        &mut mismatches,
        "$.viewport.safe",
        reference.viewport.safe,
        candidate.viewport.safe,
        tolerance_points,
    );
    compare_f32(
        &mut mismatches,
        "$.viewport",
        "points_scale",
        reference.viewport.points_scale,
        candidate.viewport.points_scale,
        tolerance_points,
    );

    let reference_nodes = flatten_comparable_nodes(&reference.root);
    let candidate_nodes = flatten_comparable_nodes(&candidate.root);
    let reference_paths = reference_nodes.keys().cloned().collect::<BTreeSet<_>>();
    let candidate_paths = candidate_nodes.keys().cloned().collect::<BTreeSet<_>>();

    for path in reference_paths.difference(&candidate_paths) {
        mismatches.push(VisualTreeMismatch {
            path: path.clone(),
            field: "node".to_owned(),
            expected: Value::String("present".to_owned()),
            actual: Value::String("missing".to_owned()),
            delta: None,
        });
    }
    for path in candidate_paths.difference(&reference_paths) {
        mismatches.push(VisualTreeMismatch {
            path: path.clone(),
            field: "node".to_owned(),
            expected: Value::String("absent".to_owned()),
            actual: Value::String("present".to_owned()),
            delta: None,
        });
    }
    for path in reference_paths.intersection(&candidate_paths) {
        let reference_node =
            reference_nodes.get(path).expect("intersection path exists in reference");
        let candidate_node =
            candidate_nodes.get(path).expect("intersection path exists in candidate");
        compare_node(&mut mismatches, reference_node, candidate_node, tolerance_points);
    }

    VisualTreeDiff { passed: mismatches.is_empty(), tolerance_points, mismatches }
}

#[must_use]
pub fn compare_visual_tree_sequences(
    reference: &VisualTreeSequence,
    candidate: &VisualTreeSequence,
    tolerance_points: f32,
) -> VisualTreeSequenceDiff {
    let mut mismatches = Vec::new();
    if reference.schema != candidate.schema {
        mismatches.push(format!(
            "sequence schema mismatch: reference={} candidate={}",
            reference.schema, candidate.schema
        ));
    }
    if reference.scene != candidate.scene {
        mismatches.push(format!(
            "sequence scene mismatch: reference={} candidate={}",
            reference.scene, candidate.scene
        ));
    }
    if reference.route != candidate.route {
        mismatches.push(format!(
            "sequence route mismatch: reference={} candidate={}",
            reference.route, candidate.route
        ));
    }
    if reference.preset != candidate.preset {
        mismatches.push(format!(
            "sequence preset mismatch: reference={:?} candidate={:?}",
            reference.preset, candidate.preset
        ));
    }
    if reference.kind != candidate.kind {
        mismatches.push(format!(
            "sequence kind mismatch: reference={:?} candidate={:?}",
            reference.kind, candidate.kind
        ));
    }
    if reference.steps.len() != candidate.steps.len() {
        mismatches.push(format!(
            "sequence step count mismatch: reference={} candidate={}",
            reference.steps.len(),
            candidate.steps.len()
        ));
    }

    let mut steps = Vec::new();
    for (reference_step, candidate_step) in reference.steps.iter().zip(candidate.steps.iter()) {
        let mut step_mismatches = Vec::new();
        if reference_step.index != candidate_step.index {
            step_mismatches.push(format!(
                "index mismatch: reference={} candidate={}",
                reference_step.index, candidate_step.index
            ));
        }
        if let Some(reference_time_ms) = reference_step.time_ms {
            match candidate_step.time_ms {
                Some(candidate_time_ms) if candidate_time_ms == reference_time_ms => {}
                Some(candidate_time_ms) => step_mismatches.push(format!(
                    "time_ms mismatch: reference={} candidate={}",
                    reference_time_ms, candidate_time_ms
                )),
                None => step_mismatches.push("missing candidate time_ms".to_owned()),
            }
        }
        if let Some(reference_requested_scroll_y) = reference_step.requested_scroll_y {
            match candidate_step.requested_scroll_y {
                Some(candidate_requested_scroll_y)
                    if (reference_requested_scroll_y - candidate_requested_scroll_y).abs()
                        <= 0.001 => {}
                Some(candidate_requested_scroll_y) => step_mismatches.push(format!(
                    "requested_scroll_y mismatch: reference={} candidate={}",
                    reference_requested_scroll_y, candidate_requested_scroll_y
                )),
                None => step_mismatches.push("missing candidate requested_scroll_y".to_owned()),
            }
        }
        if let Some(reference_effective_scroll_y) = reference_step.effective_scroll_y {
            match candidate_step.effective_scroll_y {
                Some(candidate_effective_scroll_y)
                    if (reference_effective_scroll_y - candidate_effective_scroll_y).abs()
                        <= 0.01 => {}
                Some(candidate_effective_scroll_y) => step_mismatches.push(format!(
                    "effective_scroll_y mismatch: reference={} candidate={}",
                    reference_effective_scroll_y, candidate_effective_scroll_y
                )),
                None => step_mismatches.push("missing candidate effective_scroll_y".to_owned()),
            }
        }
        for (key, reference_value) in reference_step.data.iter() {
            match candidate_step.data.get(key) {
                Some(candidate_value)
                    if visual_tree_data_values_match(
                        format!("data.{key}").as_str(),
                        reference_value,
                        candidate_value,
                        tolerance_points,
                    ) => {}
                Some(candidate_value) => step_mismatches.push(format!(
                    "data.{} mismatch: reference={} candidate={}",
                    key, reference_value, candidate_value
                )),
                None => step_mismatches.push(format!("missing data.{}", key)),
            }
        }
        let diff = compare_visual_tree_snapshots(
            &reference_step.snapshot,
            &candidate_step.snapshot,
            tolerance_points,
        );
        if !diff.mismatches.is_empty() {
            step_mismatches.push(format!("snapshot mismatch count={}", diff.mismatches.len()));
        }
        for mismatch in step_mismatches.iter() {
            mismatches.push(format!("step {}: {}", reference_step.index, mismatch));
        }
        steps.push(VisualTreeSequenceStepDiff {
            index: reference_step.index,
            reference_requested_scroll_y: reference_step.requested_scroll_y,
            candidate_requested_scroll_y: candidate_step.requested_scroll_y,
            reference_effective_scroll_y: reference_step.effective_scroll_y,
            candidate_effective_scroll_y: candidate_step.effective_scroll_y,
            reference_time_ms: reference_step.time_ms,
            candidate_time_ms: candidate_step.time_ms,
            reference_node_count: count_visual_tree_nodes(&reference_step.snapshot.root),
            candidate_node_count: count_visual_tree_nodes(&candidate_step.snapshot.root),
            mismatches: step_mismatches,
            diff,
        });
    }

    VisualTreeSequenceDiff { passed: mismatches.is_empty(), tolerance_points, mismatches, steps }
}

fn compare_snapshot_field(
    mismatches: &mut Vec<VisualTreeMismatch>,
    path: &str,
    field: &str,
    expected: Value,
    actual: Value,
) {
    if expected == actual {
        return;
    }
    mismatches.push(VisualTreeMismatch {
        path: path.to_owned(),
        field: field.to_owned(),
        expected,
        actual,
        delta: None,
    });
}

fn compare_node(
    mismatches: &mut Vec<VisualTreeMismatch>,
    reference: &ComparableVisualTreeNode<'_>,
    candidate: &ComparableVisualTreeNode<'_>,
    tolerance_points: f32,
) {
    let reference_node = reference.node;
    let candidate_node = candidate.node;

    compare_snapshot_field(
        mismatches,
        reference_node.path.as_str(),
        "kind",
        Value::String(reference_node.kind.clone()),
        Value::String(candidate_node.kind.clone()),
    );
    compare_snapshot_field(
        mismatches,
        reference_node.path.as_str(),
        "role",
        serde_json::to_value(reference_node.role.as_ref()).unwrap_or(Value::Null),
        serde_json::to_value(candidate_node.role.as_ref()).unwrap_or(Value::Null),
    );
    compare_snapshot_field(
        mismatches,
        reference_node.path.as_str(),
        "visible",
        Value::Bool(reference_node.visible),
        Value::Bool(candidate_node.visible),
    );
    compare_opacity(
        mismatches,
        reference_node.path.as_str(),
        "opacity",
        reference_node.opacity,
        candidate_node.opacity,
    );
    compare_opacity(
        mismatches,
        reference_node.path.as_str(),
        "effective_opacity",
        reference.effective_opacity,
        candidate.effective_opacity,
    );
    compare_rect(
        mismatches,
        reference_node.path.as_str(),
        reference_node.frame,
        candidate_node.frame,
        tolerance_points,
    );
    compare_node_data(mismatches, reference_node, candidate_node, tolerance_points);
    let reference_children = comparable_child_paths(reference_node);
    let candidate_children = comparable_child_paths(candidate_node);
    if reference_children != candidate_children {
        mismatches.push(VisualTreeMismatch {
            path: reference_node.path.clone(),
            field: "children".to_owned(),
            expected: serde_json::json!(reference_children),
            actual: serde_json::json!(candidate_children),
            delta: None,
        });
    }
}

fn compare_node_data(
    mismatches: &mut Vec<VisualTreeMismatch>,
    reference: &VisualTreeNode,
    candidate: &VisualTreeNode,
    tolerance_points: f32,
) {
    for (key, expected) in reference.data.iter() {
        if key == "parity_ignore" {
            continue;
        }
        match candidate.data.get(key) {
            Some(actual) => compare_visual_tree_data_value(
                mismatches,
                reference.path.as_str(),
                format!("data.{key}"),
                expected,
                actual,
                tolerance_points,
            ),
            None => mismatches.push(VisualTreeMismatch {
                path: reference.path.clone(),
                field: format!("data.{key}"),
                expected: expected.clone(),
                actual: Value::Null,
                delta: None,
            }),
        }
    }
}

fn compare_visual_tree_data_value(
    mismatches: &mut Vec<VisualTreeMismatch>,
    path: &str,
    field: String,
    expected: &Value,
    actual: &Value,
    tolerance_points: f32,
) {
    if visual_tree_data_values_match(field.as_str(), expected, actual, tolerance_points) {
        return;
    }

    match (expected, actual) {
        (Value::Object(expected_object), Value::Object(actual_object)) => {
            for (key, expected_value) in expected_object.iter() {
                match actual_object.get(key) {
                    Some(actual_value) => compare_visual_tree_data_value(
                        mismatches,
                        path,
                        format!("{field}.{key}"),
                        expected_value,
                        actual_value,
                        tolerance_points,
                    ),
                    None => mismatches.push(VisualTreeMismatch {
                        path: path.to_owned(),
                        field: format!("{field}.{key}"),
                        expected: expected_value.clone(),
                        actual: Value::Null,
                        delta: None,
                    }),
                }
            }
        }
        (Value::Array(expected_array), Value::Array(actual_array)) => {
            if expected_array.len() != actual_array.len() {
                mismatches.push(VisualTreeMismatch {
                    path: path.to_owned(),
                    field,
                    expected: expected.clone(),
                    actual: actual.clone(),
                    delta: None,
                });
                return;
            }

            for (index, (expected_value, actual_value)) in
                expected_array.iter().zip(actual_array.iter()).enumerate()
            {
                compare_visual_tree_data_value(
                    mismatches,
                    path,
                    format!("{field}[{index}]"),
                    expected_value,
                    actual_value,
                    tolerance_points,
                );
            }
        }
        _ => mismatches.push(VisualTreeMismatch {
            path: path.to_owned(),
            field,
            expected: expected.clone(),
            actual: actual.clone(),
            delta: visual_tree_data_number_delta(expected, actual),
        }),
    }
}

fn visual_tree_data_values_match(
    field: &str,
    expected: &Value,
    actual: &Value,
    tolerance_points: f32,
) -> bool {
    if expected == actual {
        return true;
    }
    match (expected.as_f64(), actual.as_f64()) {
        (Some(expected), Some(actual)) => {
            (expected - actual).abs() <= visual_tree_data_numeric_tolerance(field, tolerance_points)
        }
        _ => false,
    }
}

fn visual_tree_data_number_delta(expected: &Value, actual: &Value) -> Option<f32> {
    match (expected.as_f64(), actual.as_f64()) {
        (Some(expected), Some(actual)) => Some((expected - actual).abs() as f32),
        _ => None,
    }
}

fn visual_tree_data_numeric_tolerance(field: &str, tolerance_points: f32) -> f64 {
    if field.starts_with("data.visual.") {
        if field.ends_with(".opacity") || field.ends_with(".effective_opacity") {
            return f64::from(VISUAL_TREE_OPACITY_TOLERANCE);
        }
        return f64::from(tolerance_points);
    }
    VISUAL_TREE_DATA_NUMBER_TOLERANCE
}

fn compare_rect(
    mismatches: &mut Vec<VisualTreeMismatch>,
    path: &str,
    expected: VisualTreeRect,
    actual: VisualTreeRect,
    tolerance_points: f32,
) {
    compare_f32(mismatches, path, "frame.x", expected.x, actual.x, tolerance_points);
    compare_f32(mismatches, path, "frame.y", expected.y, actual.y, tolerance_points);
    compare_f32(mismatches, path, "frame.w", expected.w, actual.w, tolerance_points);
    compare_f32(mismatches, path, "frame.h", expected.h, actual.h, tolerance_points);
}

fn compare_insets(
    mismatches: &mut Vec<VisualTreeMismatch>,
    path: &str,
    expected: VisualTreeInsets,
    actual: VisualTreeInsets,
    tolerance_points: f32,
) {
    compare_f32(mismatches, path, "left", expected.left, actual.left, tolerance_points);
    compare_f32(mismatches, path, "top", expected.top, actual.top, tolerance_points);
    compare_f32(mismatches, path, "right", expected.right, actual.right, tolerance_points);
    compare_f32(mismatches, path, "bottom", expected.bottom, actual.bottom, tolerance_points);
}

fn compare_f32(
    mismatches: &mut Vec<VisualTreeMismatch>,
    path: &str,
    field: &str,
    expected: f32,
    actual: f32,
    tolerance_points: f32,
) {
    compare_f32_with_tolerance(mismatches, path, field, expected, actual, tolerance_points);
}

fn compare_opacity(
    mismatches: &mut Vec<VisualTreeMismatch>,
    path: &str,
    field: &str,
    expected: f32,
    actual: f32,
) {
    compare_f32_with_tolerance(
        mismatches,
        path,
        field,
        expected,
        actual,
        VISUAL_TREE_OPACITY_TOLERANCE,
    );
}

fn compare_f32_with_tolerance(
    mismatches: &mut Vec<VisualTreeMismatch>,
    path: &str,
    field: &str,
    expected: f32,
    actual: f32,
    tolerance: f32,
) {
    let delta = (expected - actual).abs();
    if delta <= tolerance {
        return;
    }
    mismatches.push(VisualTreeMismatch {
        path: path.to_owned(),
        field: field.to_owned(),
        expected: serde_json::json!(expected),
        actual: serde_json::json!(actual),
        delta: Some(delta),
    });
}

fn comparable_child_paths(node: &VisualTreeNode) -> Vec<&str> {
    node.children
        .iter()
        .filter(|child| !parity_ignored(child))
        .map(|child| child.path.as_str())
        .collect()
}

fn flatten_comparable_nodes(
    root: &VisualTreeNode,
) -> BTreeMap<String, ComparableVisualTreeNode<'_>> {
    let mut nodes = BTreeMap::new();
    flatten_comparable_node(root, 1.0, &mut nodes);
    nodes
}

fn flatten_comparable_node<'a>(
    node: &'a VisualTreeNode,
    inherited_opacity: f32,
    nodes: &mut BTreeMap<String, ComparableVisualTreeNode<'a>>,
) {
    if parity_ignored(node) {
        return;
    }
    let effective_opacity = inherited_opacity * node.opacity;
    nodes.insert(node.path.clone(), ComparableVisualTreeNode { node, effective_opacity });
    for child in node.children.iter() {
        flatten_comparable_node(child, effective_opacity, nodes);
    }
}

fn parity_ignored(node: &VisualTreeNode) -> bool {
    node.data.get("parity_ignore").and_then(Value::as_bool).unwrap_or(false)
}

fn count_visual_tree_nodes(node: &VisualTreeNode) -> usize {
    1 + node.children.iter().map(count_visual_tree_nodes).sum::<usize>()
}
