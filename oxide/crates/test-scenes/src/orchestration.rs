//! UI Orchestration Test Scene - Tests ScatterOrchestrator and overlay/modal systems
//!
//! This scene validates orchestrated animations and modal/overlay behaviors.

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use oxide_renderer_api as gfx;
use oxide_ui_core::{
    elements::{encode_label_text, Align, Button, ButtonState, ButtonStyle, ImageUploader, Label, TextCtx},
    orchestration::{ScatterOrchestrator, ScatterState, Scatterer},
    overlay::{OverlayBehavior, OverlayHandle, OverlayPointerResult, OverlayStack, OverlayVisual},
    DrawListBuilder, NodeId, NodeStyle, UiSurface,
};

/// Node that can be scattered
struct ScatterNode {
    id: NodeId,
    state: ScatterState,
    rect: gfx::RectF,
    color: gfx::Color,
    label: String,
}

impl Scatterer for ScatterNode {
    fn node_id(&self) -> NodeId {
        self.id
    }

    fn scatter_state(&self) -> ScatterState {
        self.state
    }

    fn set_scatter_state(&mut self, state: ScatterState) {
        self.state = state;
    }
}

/// Test scene for UI orchestration features
pub struct OrchestrationScene {
    /// Scatter orchestrator with configurable timing
    orchestrators: Vec<(ScatterOrchestrator, String, u32)>,

    /// Groups of scatter nodes for testing
    node_groups: Vec<Vec<ScatterNode>>,

    /// Current active orchestrator index
    active_orchestrator: usize,

    /// Current active node group
    active_group: usize,

    /// Overlay stack for modal testing
    overlay_stack: OverlayStack,

    /// Test overlays with different configurations
    test_overlays: Vec<(OverlayHandle, String)>,

    /// Buttons for triggering orchestration
    trigger_buttons: Vec<(Button, ButtonState, String)>,

    /// Animation state tracking
    animation_active: bool,
    animation_start_ms: u64,

    /// Time accumulator
    time_ms: u64,
}

impl Default for OrchestrationScene {
    fn default() -> Self {
        // Create orchestrators with different timings
        let orchestrators = vec![
            (ScatterOrchestrator::new(100), "Fast (100ms)".into(), 100),
            (ScatterOrchestrator::new(200), "Default (200ms)".into(), 200),
            (ScatterOrchestrator::new(400), "Slow (400ms)".into(), 400),
            (ScatterOrchestrator::new(800), "Very Slow (800ms)".into(), 800),
        ];

        // Create node groups for testing
        let mut node_groups = Vec::new();

        // Group 1: Simple 2x2 grid
        let mut group1 = Vec::new();
        for i in 0..4 {
            let col = i % 2;
            let row = i / 2;
            group1.push(ScatterNode {
                id: NodeId(100 + i),
                state: ScatterState::Off,
                rect: gfx::RectF::new(
                    100.0 + col as f32 * 120.0,
                    100.0 + row as f32 * 120.0,
                    100.0,
                    100.0,
                ),
                color: match i {
                    0 => gfx::Color::rgba(0.9, 0.3, 0.3, 1.0),
                    1 => gfx::Color::rgba(0.3, 0.9, 0.3, 1.0),
                    2 => gfx::Color::rgba(0.3, 0.3, 0.9, 1.0),
                    _ => gfx::Color::rgba(0.9, 0.9, 0.3, 1.0),
                },
                label: format!("Node {}", i + 1),
            });
        }
        node_groups.push(group1);

        // Group 2: Larger 3x3 grid
        let mut group2 = Vec::new();
        for i in 0..9 {
            let col = i % 3;
            let row = i / 3;
            group2.push(ScatterNode {
                id: NodeId(200 + i),
                state: ScatterState::Off,
                rect: gfx::RectF::new(
                    80.0 + col as f32 * 80.0,
                    80.0 + row as f32 * 80.0,
                    70.0,
                    70.0,
                ),
                color: {
                    let hue = i as f32 * 40.0;
                    let h = hue / 360.0;
                    let s = 0.7;
                    let v = 0.9;

                    // HSV to RGB conversion
                    let c = v * s;
                    let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
                    let m = v - c;

                    let (r, g, b) = match (h * 6.0) as u32 {
                        0 => (c, x, 0.0),
                        1 => (x, c, 0.0),
                        2 => (0.0, c, x),
                        3 => (0.0, x, c),
                        4 => (x, 0.0, c),
                        _ => (c, 0.0, x),
                    };

                    gfx::Color::rgba(r + m, g + m, b + m, 1.0)
                },
                label: format!("{}", i + 1),
            });
        }
        node_groups.push(group2);

        // Create trigger buttons
        let mut trigger_buttons = Vec::new();

        // Scatter ON button
        let scatter_on_button = Button { text: "Scatter ON".into(), style: ButtonStyle::default() };
        trigger_buttons.push((scatter_on_button, ButtonState::default(), "scatter_on".into()));

        // Scatter OFF button
        let scatter_off_button =
            Button { text: "Scatter OFF".into(), style: ButtonStyle::default() };
        trigger_buttons.push((scatter_off_button, ButtonState::default(), "scatter_off".into()));

        // Transition button
        let transition_button =
            Button { text: "Transition Groups".into(), style: ButtonStyle::default() };
        trigger_buttons.push((transition_button, ButtonState::default(), "transition".into()));

        // Modal buttons
        let modal_button = Button { text: "Show Modal".into(), style: ButtonStyle::default() };
        trigger_buttons.push((modal_button, ButtonState::default(), "modal".into()));

        let dismissable_modal_button =
            Button { text: "Dismissable Modal".into(), style: ButtonStyle::default() };
        trigger_buttons.push((
            dismissable_modal_button,
            ButtonState::default(),
            "dismissable_modal".into(),
        ));

        Self {
            orchestrators,
            node_groups,
            active_orchestrator: 1, // Start with default timing
            active_group: 0,
            overlay_stack: OverlayStack::new(),
            test_overlays: Vec::new(),
            trigger_buttons,
            animation_active: false,
            animation_start_ms: 0,
            time_ms: 0,
        }
    }
}

impl OrchestrationScene {
    pub fn update(&mut self, dt_ms: u32) {
        self.time_ms += dt_ms as u64;

        // Update animation state
        if self.animation_active {
            let (_, _, duration_ms) = &self.orchestrators[self.active_orchestrator];
            if self.time_ms - self.animation_start_ms >= *duration_ms as u64 {
                self.animation_active = false;
                self.orchestrators[self.active_orchestrator].0.end_transition();
            }
        }
    }

    pub fn input_pointer(&mut self, x: f32, y: f32, _dx: f32, _dy: f32, buttons: u32) {
        // Check overlay stack first
        if !self.overlay_stack.is_empty() {
            let result = self.overlay_stack.pointer_event(x, y, buttons);
            if let OverlayPointerResult::Dismissed { handle } = result {
                if let Some(pos) = self.test_overlays.iter().position(|(h, _)| *h == handle) {
                    self.test_overlays.remove(pos);
                }
            }
            return; // Overlays block underlying input
        }

        // Don't process input during animations
        if self.orchestrators[self.active_orchestrator].0.is_animating() {
            return;
        }

        // Process button interactions
        let mut action_to_handle = None;
        for (i, (_button, state, action)) in self.trigger_buttons.iter_mut().enumerate() {
            let rect = gfx::RectF::new(
                350.0 + (i % 2) as f32 * 150.0,
                100.0 + (i / 2) as f32 * 50.0,
                140.0,
                40.0,
            );

            if point_in_rect([x, y], rect) {
                if buttons & 1 != 0 {
                    state.on_pointer_down();
                } else if state.on_pointer_up() {
                    action_to_handle = Some(action.clone());
                }
            } else if buttons == 0 {
                state.on_pointer_cancel();
            }
        }

        if let Some(action) = action_to_handle {
            self.handle_action(&action);
        }

        // Orchestrator selection
        for i in 0..self.orchestrators.len() {
            let rect = gfx::RectF::new(350.0, 320.0 + i as f32 * 30.0, 150.0, 25.0);

            if buttons & 1 != 0 && point_in_rect([x, y], rect) {
                self.active_orchestrator = i;
            }
        }

        // Group selection
        for i in 0..self.node_groups.len() {
            let rect = gfx::RectF::new(520.0, 320.0 + i as f32 * 30.0, 100.0, 25.0);

            if buttons & 1 != 0 && point_in_rect([x, y], rect) {
                self.active_group = i;
            }
        }
    }

    pub fn benchmark_reset(&mut self) {
        *self = Self::default();
    }

    pub fn benchmark_transition_or_modal(&mut self, step: usize) {
        if step % 2 == 0 {
            self.handle_action("transition");
        } else {
            self.handle_action("modal");
        }
    }

    fn handle_action(&mut self, action: &str) {
        match action {
            "scatter_on" => {
                let node_ids: Vec<NodeId> =
                    self.node_groups[self.active_group].iter().map(|n| n.node_id()).collect();

                for node in &mut self.node_groups[self.active_group] {
                    node.set_scatter_state(ScatterState::On);
                }

                let orchestrator = &mut self.orchestrators[self.active_orchestrator].0;
                orchestrator.begin_transition();
                let _batch = orchestrator.scatter_on(&node_ids);

                self.animation_active = true;
                self.animation_start_ms = self.time_ms;
            }

            "scatter_off" => {
                let node_ids: Vec<NodeId> =
                    self.node_groups[self.active_group].iter().map(|n| n.node_id()).collect();

                for node in &mut self.node_groups[self.active_group] {
                    node.set_scatter_state(ScatterState::Off);
                }

                let orchestrator = &mut self.orchestrators[self.active_orchestrator].0;
                orchestrator.begin_transition();
                let _batch = orchestrator.scatter_off(&node_ids);

                self.animation_active = true;
                self.animation_start_ms = self.time_ms;
            }

            "transition" => {
                let next_group = (self.active_group + 1) % self.node_groups.len();

                let old_ids: Vec<NodeId> = self.node_groups[self.active_group]
                    .iter()
                    .filter(|n| n.scatter_state() == ScatterState::On)
                    .map(|n| n.node_id())
                    .collect();

                let new_ids: Vec<NodeId> =
                    self.node_groups[next_group].iter().map(|n| n.node_id()).collect();

                // Update states
                for node in &mut self.node_groups[self.active_group] {
                    node.set_scatter_state(ScatterState::Off);
                }
                for node in &mut self.node_groups[next_group] {
                    node.set_scatter_state(ScatterState::On);
                }

                let orchestrator = &mut self.orchestrators[self.active_orchestrator].0;
                orchestrator.begin_transition();
                let _batch = orchestrator.transition(&old_ids, &new_ids);

                self.active_group = next_group;
                self.animation_active = true;
                self.animation_start_ms = self.time_ms;
            }

            "modal" => {
                self.show_modal(false);
            }

            "dismissable_modal" => {
                self.show_modal(true);
            }

            _ => {}
        }
    }

    fn show_modal(&mut self, dismissable: bool) {
        // Create a simple modal surface
        let root_style = NodeStyle::default();
        let surface = UiSurface::new(root_style);

        // Modal would contain actual UI elements
        // For now, just track that it was created

        let visual = OverlayVisual {
            blur_sigma: if dismissable { 12.0 } else { 20.0 },
            tint: gfx::Color::rgba(0.0, 0.0, 0.0, 1.0),
            alpha: if dismissable { 0.3 } else { 0.5 },
            z_index: 1000,
        };

        let behavior = OverlayBehavior {
            dismiss_on_background_tap: dismissable,
            block_underlying_inputs: true,
            content_root: None,
            focus_root: None,
        };

        let handle = self.overlay_stack.push(surface, visual, behavior);
        let label =
            if dismissable { "Dismissable Modal".into() } else { "Modal (ESC to close)".into() };
        self.test_overlays.push((handle, label));
    }

    pub fn draw<U: ImageUploader>(
        &mut self,
        viewport: gfx::RectF,
        device_scale: f32,
        text: &mut TextCtx,
        uploader: &mut U,
        builder: &mut DrawListBuilder,
    ) {
        // Update overlay viewport
        self.overlay_stack.set_viewport(viewport, device_scale);

        // Background
        builder.rrect(viewport, [0.0; 4], gfx::Color::rgba(0.96, 0.96, 0.97, 1.0));

        // Title
        encode_label_text(
            "UI Orchestration Test",
            gfx::Color::rgba(0.1, 0.1, 0.1, 1.0),
            Align::Center,
            false,
            0,
            22.0,
            gfx::RectF::new(viewport.x, viewport.y + 5.0, viewport.w, 30.0),
            device_scale,
            text,
            uploader,
            builder,
        );

        // Draw scatter nodes
        for group in &self.node_groups {
            for node in group {
                let scale = match node.scatter_state() {
                    ScatterState::Off => 0.05,
                    ScatterState::On => 1.0,
                };

                let opacity = match node.scatter_state() {
                    ScatterState::Off => 0.0,
                    ScatterState::On => 1.0,
                };

                if opacity > 0.01 {
                    let rect = node.rect;
                    let center_x = rect.x + rect.w / 2.0;
                    let center_y = rect.y + rect.h / 2.0;
                    let scaled_w = rect.w * scale;
                    let scaled_h = rect.h * scale;

                    let scaled_rect = gfx::RectF::new(
                        viewport.x + center_x - scaled_w / 2.0,
                        viewport.y + center_y - scaled_h / 2.0,
                        scaled_w,
                        scaled_h,
                    );

                    let mut color = node.color;
                    color.a = opacity;

                    builder.rrect(scaled_rect, [8.0; 4], color);

                    if scale > 0.5 {
                        encode_label_text(
                            node.label.as_str(),
                            gfx::Color::rgba(1.0, 1.0, 1.0, opacity),
                            Align::Center,
                            false,
                            0,
                            14.0,
                            scaled_rect,
                            device_scale,
                            text,
                            uploader,
                            builder,
                        );
                    }
                }
            }
        }

        // Draw control buttons
        for (i, (button, state, _)) in self.trigger_buttons.iter_mut().enumerate() {
            button.encode(
                gfx::RectF::new(
                    viewport.x + 350.0 + (i % 2) as f32 * 150.0,
                    viewport.y + 100.0 + (i / 2) as f32 * 50.0,
                    140.0,
                    40.0,
                ),
                device_scale,
                text,
                uploader,
                state,
                builder,
            );
        }

        // Draw orchestrator selector
        encode_label_text(
            "Timing:",
            gfx::Color::rgba(0.3, 0.3, 0.3, 1.0),
            Align::Left,
            false,
            0,
            12.0,
            gfx::RectF::new(viewport.x + 350.0, viewport.y + 295.0, 100.0, 20.0),
            device_scale,
            text,
            uploader,
            builder,
        );

        for (i, (_, label, _)) in self.orchestrators.iter().enumerate() {
            let is_selected = i == self.active_orchestrator;
            let rect = gfx::RectF::new(
                viewport.x + 350.0,
                viewport.y + 320.0 + i as f32 * 30.0,
                150.0,
                25.0,
            );

            builder.rrect(
                rect,
                [4.0; 4],
                if is_selected {
                    gfx::Color::rgba(0.2, 0.5, 0.9, 1.0)
                } else {
                    gfx::Color::rgba(0.85, 0.85, 0.85, 1.0)
                },
            );

            let color = if is_selected {
                gfx::Color::rgba(1.0, 1.0, 1.0, 1.0)
            } else {
                gfx::Color::rgba(0.2, 0.2, 0.2, 1.0)
            };
            encode_label_text(
                label.as_str(),
                color,
                Align::Center,
                false,
                0,
                11.0,
                rect,
                device_scale,
                text,
                uploader,
                builder,
            );
        }

        // Draw group selector
        encode_label_text(
            "Group:",
            gfx::Color::rgba(0.3, 0.3, 0.3, 1.0),
            Align::Left,
            false,
            0,
            12.0,
            gfx::RectF::new(viewport.x + 520.0, viewport.y + 295.0, 100.0, 20.0),
            device_scale,
            text,
            uploader,
            builder,
        );

        for i in 0..self.node_groups.len() {
            let is_selected = i == self.active_group;
            let rect = gfx::RectF::new(
                viewport.x + 520.0,
                viewport.y + 320.0 + i as f32 * 30.0,
                100.0,
                25.0,
            );

            builder.rrect(
                rect,
                [4.0; 4],
                if is_selected {
                    gfx::Color::rgba(0.9, 0.5, 0.2, 1.0)
                } else {
                    gfx::Color::rgba(0.85, 0.85, 0.85, 1.0)
                },
            );

            let color = if is_selected {
                gfx::Color::rgba(1.0, 1.0, 1.0, 1.0)
            } else {
                gfx::Color::rgba(0.2, 0.2, 0.2, 1.0)
            };
            let group_fallback;
            let label = match i {
                0 => "Group 1",
                1 => "Group 2",
                _ => {
                    group_fallback = format!("Group {}", i + 1);
                    group_fallback.as_str()
                }
            };
            encode_label_text(
                label,
                color,
                Align::Center,
                false,
                0,
                11.0,
                rect,
                device_scale,
                text,
                uploader,
                builder,
            );
        }

        // Status info
        let status = Label {
            text: format!(
                "Animation: {} | Overlays: {} | Orchestrator: {} | Time: {:.1}s",
                if self.animation_active { "Active" } else { "Idle" },
                self.test_overlays.len(),
                if self.orchestrators[self.active_orchestrator].0.is_animating() {
                    "Blocking"
                } else {
                    "Ready"
                },
                self.time_ms as f32 / 1000.0
            ),
            color: gfx::Color::rgba(0.4, 0.4, 0.4, 1.0),
            align: Align::Center,
            wrap: false,
            font_id: 0,
            font_px: 11.0,
        };
        status.encode(
            gfx::RectF::new(viewport.x, viewport.y + viewport.h - 25.0, viewport.w, 20.0),
            device_scale,
            text,
            uploader,
            builder,
        );

        // Draw overlay indicators
        for (i, (_, label)) in self.test_overlays.iter().enumerate() {
            let overlay_info = Label {
                text: format!("Overlay {}: {}", i + 1, label),
                color: gfx::Color::rgba(0.7, 0.2, 0.2, 1.0),
                align: Align::Right,
                wrap: false,
                font_id: 0,
                font_px: 10.0,
            };
            overlay_info.encode(
                gfx::RectF::new(
                    viewport.x + viewport.w - 200.0,
                    viewport.y + 40.0 + i as f32 * 20.0,
                    190.0,
                    18.0,
                ),
                device_scale,
                text,
                uploader,
                builder,
            );
        }
    }
}

fn point_in_rect(point: [f32; 2], rect: gfx::RectF) -> bool {
    point[0] >= rect.x
        && point[0] <= rect.x + rect.w
        && point[1] >= rect.y
        && point[1] <= rect.y + rect.h
}
