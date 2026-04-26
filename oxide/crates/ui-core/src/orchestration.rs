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
