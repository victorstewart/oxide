use oxide_platform_api as api;
use oxide_ui_core::orchestration::ScatterOrchestrator;
use oxide_ui_core::NodeId;

#[test]
fn scatter_on_creates_animations() {
    let orchestrator = ScatterOrchestrator::default();
    let nodes = vec![NodeId(1), NodeId(2), NodeId(3)];

    let batch = orchestrator.scatter_on(&nodes);

    assert_eq!(batch.animations.len(), 6);
    assert_eq!(batch.duration_ms, 200);
    assert_eq!(batch.animations[0].id, 1);
    assert!(matches!(batch.animations[0].prop, api::AnimProp::Transform2D));
    assert_eq!(batch.animations[1].id, 1);
    assert!(matches!(batch.animations[1].prop, api::AnimProp::Opacity));
}

#[test]
fn scatter_off_creates_animations() {
    let orchestrator = ScatterOrchestrator::default();
    let nodes = vec![NodeId(1), NodeId(2)];

    let batch = orchestrator.scatter_off(&nodes);

    assert_eq!(batch.animations.len(), 4);
}

#[test]
fn transition_sequences_animations() {
    let orchestrator = ScatterOrchestrator::default();
    let old = vec![NodeId(1)];
    let new = vec![NodeId(2)];

    let batch = orchestrator.transition(&old, &new);

    assert_eq!(batch.duration_ms, 400);
    let old_anims: Vec<_> = batch.animations.iter().filter(|a| a.id == 1).collect();
    let new_anims: Vec<_> = batch.animations.iter().filter(|a| a.id == 2).collect();

    assert!(old_anims.iter().all(|a| a.delay_ms == 0));
    assert!(new_anims.iter().all(|a| a.delay_ms == 200));
}

#[test]
fn staggered_applies_delays() {
    let orchestrator = ScatterOrchestrator::default();
    let nodes = vec![NodeId(1), NodeId(2), NodeId(3)];

    let batch = orchestrator.scatter_on_staggered(&nodes, 50);

    let node1_anims: Vec<_> = batch.animations.iter().filter(|a| a.id == 1).collect();
    let node2_anims: Vec<_> = batch.animations.iter().filter(|a| a.id == 2).collect();
    let node3_anims: Vec<_> = batch.animations.iter().filter(|a| a.id == 3).collect();

    assert!(node1_anims.iter().all(|a| a.delay_ms == 0));
    assert!(node2_anims.iter().all(|a| a.delay_ms == 50));
    assert!(node3_anims.iter().all(|a| a.delay_ms == 100));
}

#[test]
fn interaction_depth_tracking() {
    let mut orchestrator = ScatterOrchestrator::default();

    assert!(!orchestrator.is_animating());

    orchestrator.begin_transition();
    assert!(orchestrator.is_animating());

    orchestrator.begin_transition();
    assert!(orchestrator.is_animating());

    orchestrator.end_transition();
    assert!(orchestrator.is_animating());

    orchestrator.end_transition();
    assert!(!orchestrator.is_animating());
}
