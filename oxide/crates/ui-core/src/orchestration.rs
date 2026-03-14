//! Animation orchestration for coordinated multi-node transitions
//!
//! Based on Nametag's Topper/Scatterer pattern for managing complex UI hierarchies.

use crate::NodeId;
use alloc::vec::Vec;
use oxide_platform_api as api;

/// Scatter state for a node (off → on or on → off)
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScatterState {
    Off, // Scale 0.05, opacity 0.0
    On,  // Scale 1.0, opacity 1.0
}

/// A scatterable node
pub trait Scatterer {
    fn node_id(&self) -> NodeId;
    fn scatter_state(&self) -> ScatterState;
    fn set_scatter_state(&mut self, state: ScatterState);
}

/// Result of orchestrated scatter animation
pub struct ScatterBatch {
    pub animations: Vec<api::AnimDesc>,
    pub duration_ms: u32,
}

/// Orchestration for managing groups of scatterers (Topper pattern)
pub struct ScatterOrchestrator {
    duration_ms: u32,
    interaction_depth: u32, // Track nested animations
}

impl Default for ScatterOrchestrator {
    fn default() -> Self {
        Self {
            duration_ms: 200, // 0.20s default (LayoutMaster::scatterAnimationTime)
            interaction_depth: 0,
        }
    }
}

impl ScatterOrchestrator {
    pub fn new(duration_ms: u32) -> Self {
        Self { duration_ms, interaction_depth: 0 }
    }

    /// Check if interactions should be blocked
    pub fn is_animating(&self) -> bool {
        self.interaction_depth > 0
    }

    /// Begin animation (blocks interactions)
    pub fn begin_transition(&mut self) {
        self.interaction_depth += 1;
    }

    /// End animation (may unblock interactions)
    pub fn end_transition(&mut self) {
        self.interaction_depth = self.interaction_depth.saturating_sub(1);
    }

    /// Create scatter ON animations for multiple nodes
    pub fn scatter_on(&self, node_ids: &[NodeId]) -> ScatterBatch {
        let mut animations = Vec::with_capacity(node_ids.len() * 2);

        for &id in node_ids {
            // Scale: 0.05 → 1.0
            animations.push(api::AnimDesc {
                id: id.0 as u64,
                prop: api::AnimProp::Transform2D,
                from: api::AnimValue::Xform2D(api::Transform2D {
                    tx: 0.0,
                    ty: 0.0,
                    sx: 0.05,
                    sy: 0.05,
                    rot_rad: 0.0,
                }),
                to: api::AnimValue::Xform2D(api::Transform2D {
                    tx: 0.0,
                    ty: 0.0,
                    sx: 1.0,
                    sy: 1.0,
                    rot_rad: 0.0,
                }),
                curve: api::AnimCurve::Ease { ease: api::Ease { kind: api::EaseKind::QuadOut } },
                duration_ms: self.duration_ms,
                delay_ms: 0,
                repeat: api::Repeat::Once,
            });

            // Opacity: 0.0 → 1.0
            animations.push(api::AnimDesc {
                id: id.0 as u64,
                prop: api::AnimProp::Opacity,
                from: api::AnimValue::F32(0.0),
                to: api::AnimValue::F32(1.0),
                curve: api::AnimCurve::Ease { ease: api::Ease { kind: api::EaseKind::QuadOut } },
                duration_ms: self.duration_ms,
                delay_ms: 0,
                repeat: api::Repeat::Once,
            });
        }

        ScatterBatch { animations, duration_ms: self.duration_ms }
    }

    /// Create scatter OFF animations for multiple nodes
    pub fn scatter_off(&self, node_ids: &[NodeId]) -> ScatterBatch {
        let mut animations = Vec::with_capacity(node_ids.len() * 2);

        for &id in node_ids {
            // Scale: 1.0 → 0.05
            animations.push(api::AnimDesc {
                id: id.0 as u64,
                prop: api::AnimProp::Transform2D,
                from: api::AnimValue::Xform2D(api::Transform2D {
                    tx: 0.0,
                    ty: 0.0,
                    sx: 1.0,
                    sy: 1.0,
                    rot_rad: 0.0,
                }),
                to: api::AnimValue::Xform2D(api::Transform2D {
                    tx: 0.0,
                    ty: 0.0,
                    sx: 0.05,
                    sy: 0.05,
                    rot_rad: 0.0,
                }),
                curve: api::AnimCurve::Ease { ease: api::Ease { kind: api::EaseKind::QuadIn } },
                duration_ms: self.duration_ms,
                delay_ms: 0,
                repeat: api::Repeat::Once,
            });

            // Opacity: 1.0 → 0.0
            animations.push(api::AnimDesc {
                id: id.0 as u64,
                prop: api::AnimProp::Opacity,
                from: api::AnimValue::F32(1.0),
                to: api::AnimValue::F32(0.0),
                curve: api::AnimCurve::Ease { ease: api::Ease { kind: api::EaseKind::QuadIn } },
                duration_ms: self.duration_ms,
                delay_ms: 0,
                repeat: api::Repeat::Once,
            });
        }

        ScatterBatch { animations, duration_ms: self.duration_ms }
    }

    /// Create transition: scatter OFF old nodes, then scatter ON new nodes
    pub fn transition(&self, old_nodes: &[NodeId], new_nodes: &[NodeId]) -> ScatterBatch {
        let mut animations = Vec::with_capacity((old_nodes.len() + new_nodes.len()) * 2);

        // Scatter OFF old nodes
        for &id in old_nodes {
            animations.push(api::AnimDesc {
                id: id.0 as u64,
                prop: api::AnimProp::Transform2D,
                from: api::AnimValue::Xform2D(api::Transform2D {
                    tx: 0.0,
                    ty: 0.0,
                    sx: 1.0,
                    sy: 1.0,
                    rot_rad: 0.0,
                }),
                to: api::AnimValue::Xform2D(api::Transform2D {
                    tx: 0.0,
                    ty: 0.0,
                    sx: 0.05,
                    sy: 0.05,
                    rot_rad: 0.0,
                }),
                curve: api::AnimCurve::Ease { ease: api::Ease { kind: api::EaseKind::QuadIn } },
                duration_ms: self.duration_ms,
                delay_ms: 0,
                repeat: api::Repeat::Once,
            });

            animations.push(api::AnimDesc {
                id: id.0 as u64,
                prop: api::AnimProp::Opacity,
                from: api::AnimValue::F32(1.0),
                to: api::AnimValue::F32(0.0),
                curve: api::AnimCurve::Ease { ease: api::Ease { kind: api::EaseKind::QuadIn } },
                duration_ms: self.duration_ms,
                delay_ms: 0,
                repeat: api::Repeat::Once,
            });
        }

        // Scatter ON new nodes (delayed by duration)
        for &id in new_nodes {
            animations.push(api::AnimDesc {
                id: id.0 as u64,
                prop: api::AnimProp::Transform2D,
                from: api::AnimValue::Xform2D(api::Transform2D {
                    tx: 0.0,
                    ty: 0.0,
                    sx: 0.05,
                    sy: 0.05,
                    rot_rad: 0.0,
                }),
                to: api::AnimValue::Xform2D(api::Transform2D {
                    tx: 0.0,
                    ty: 0.0,
                    sx: 1.0,
                    sy: 1.0,
                    rot_rad: 0.0,
                }),
                curve: api::AnimCurve::Ease { ease: api::Ease { kind: api::EaseKind::QuadOut } },
                duration_ms: self.duration_ms,
                delay_ms: self.duration_ms, // Wait for scatter off
                repeat: api::Repeat::Once,
            });

            animations.push(api::AnimDesc {
                id: id.0 as u64,
                prop: api::AnimProp::Opacity,
                from: api::AnimValue::F32(0.0),
                to: api::AnimValue::F32(1.0),
                curve: api::AnimCurve::Ease { ease: api::Ease { kind: api::EaseKind::QuadOut } },
                duration_ms: self.duration_ms,
                delay_ms: self.duration_ms,
                repeat: api::Repeat::Once,
            });
        }

        ScatterBatch {
            animations,
            duration_ms: self.duration_ms * 2, // Total duration
        }
    }

    /// Create staggered scatter ON for collection items
    pub fn scatter_on_staggered(&self, node_ids: &[NodeId], stagger_ms: u32) -> ScatterBatch {
        let mut animations = Vec::with_capacity(node_ids.len() * 2);
        let mut max_duration = self.duration_ms;

        for (i, &id) in node_ids.iter().enumerate() {
            let delay = i as u32 * stagger_ms;
            max_duration = max_duration.max(self.duration_ms + delay);

            animations.push(api::AnimDesc {
                id: id.0 as u64,
                prop: api::AnimProp::Transform2D,
                from: api::AnimValue::Xform2D(api::Transform2D {
                    tx: 0.0,
                    ty: 0.0,
                    sx: 0.05,
                    sy: 0.05,
                    rot_rad: 0.0,
                }),
                to: api::AnimValue::Xform2D(api::Transform2D {
                    tx: 0.0,
                    ty: 0.0,
                    sx: 1.0,
                    sy: 1.0,
                    rot_rad: 0.0,
                }),
                curve: api::AnimCurve::Ease { ease: api::Ease { kind: api::EaseKind::QuadOut } },
                duration_ms: self.duration_ms,
                delay_ms: delay,
                repeat: api::Repeat::Once,
            });

            animations.push(api::AnimDesc {
                id: id.0 as u64,
                prop: api::AnimProp::Opacity,
                from: api::AnimValue::F32(0.0),
                to: api::AnimValue::F32(1.0),
                curve: api::AnimCurve::Ease { ease: api::Ease { kind: api::EaseKind::QuadOut } },
                duration_ms: self.duration_ms,
                delay_ms: delay,
                repeat: api::Repeat::Once,
            });
        }

        ScatterBatch { animations, duration_ms: max_duration }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scatter_on_creates_animations() {
        let orchestrator = ScatterOrchestrator::default();
        let nodes = vec![NodeId(1), NodeId(2), NodeId(3)];

        let batch = orchestrator.scatter_on(&nodes);

        assert_eq!(batch.animations.len(), 6); // 2 anims per node
        assert_eq!(batch.duration_ms, 200);

        // Check first node has transform and opacity
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

        assert_eq!(batch.animations.len(), 4); // 2 anims per node
    }

    #[test]
    fn transition_sequences_animations() {
        let orchestrator = ScatterOrchestrator::default();
        let old = vec![NodeId(1)];
        let new = vec![NodeId(2)];

        let batch = orchestrator.transition(&old, &new);

        assert_eq!(batch.duration_ms, 400); // 200 * 2

        // Check delays: old nodes have 0 delay, new nodes have duration delay
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

        // Check delays increase by stagger amount
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
        assert!(orchestrator.is_animating()); // Still one level deep

        orchestrator.end_transition();
        assert!(!orchestrator.is_animating());
    }
}
