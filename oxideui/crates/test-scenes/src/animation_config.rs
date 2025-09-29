//! Animation Config Test Scene - Tests all configurable animation timings
//!
//! This scene validates that per-component animation timings work correctly
//! by displaying multiple instances of the same component with different timings.

use oxideui_ui_core::{
    elements::{
        Badge, BadgeState, BadgeStyle,
        Button, ButtonState, ButtonStyle,
        Toggle, ToggleState, ToggleStyle,
        RecordButton, RecordButtonState, RecordButtonStyle,
        SlidingSwitch, SlidingSwitchState, SlidingSwitchStyle,
        Label, Align, ImageUploader, TextCtx,
    },
    DrawListBuilder,
};
use oxideui_renderer_api as gfx;
use alloc::vec::Vec;
use alloc::vec;
use alloc::string::String;
use alloc::format;

/// Test scene for animation timing configurations
pub struct AnimationConfigScene {
    /// Multiple badges with different bounce timings (100ms, 450ms, 1000ms, 2000ms)
    badges: Vec<(Badge, BadgeState, String)>,

    /// Multiple buttons with different press timings (50ms, 100ms, 200ms, 500ms)
    buttons: Vec<(Button, ButtonState, String)>,

    /// Multiple toggles with different animation timings (100ms, 200ms, 400ms, 800ms)
    toggles: Vec<(Toggle, ToggleState, String)>,

    /// Multiple record buttons with different timings
    record_buttons: Vec<(RecordButton, RecordButtonState, String)>,

    /// Sliding switches with different timeout timings
    sliding_switches: Vec<(SlidingSwitch, SlidingSwitchState, String)>,

    /// Time accumulator for animations
    time_ms: u64,

    /// Last trigger time for periodic events
    last_trigger_ms: u64,

    /// Trigger interval for badge bounces
    trigger_interval_ms: u64,
}

impl Default for AnimationConfigScene {
    fn default() -> Self {
        // Create badges with different timings
        let badge_timings = vec![100, 450, 1000, 2000];
        let mut badges = Vec::new();
        for &timing_ms in &badge_timings {
            let mut style = BadgeStyle::default();
            style.bounce_duration_ms = timing_ms;

            let badge = Badge {
                count: 5,
                style,
            };

            let label = format!("{}ms", timing_ms);
            badges.push((badge, BadgeState::default(), label));
        }

        // Create buttons with different press timings
        let button_timings = vec![50, 100, 200, 500];
        let mut buttons = Vec::new();
        for &timing_ms in &button_timings {
            let mut style = ButtonStyle::default();
            style.press_animation_ms = timing_ms;

            let button = Button {
                text: format!("Press {}ms", timing_ms),
                style,
            };

            let label = format!("{}ms press", timing_ms);
            buttons.push((button, ButtonState::default(), label));
        }

        // Create toggles with different animation timings
        let toggle_timings = vec![100, 200, 400, 800];
        let mut toggles = Vec::new();
        for &timing_ms in &toggle_timings {
            let mut style = ToggleStyle::default();
            style.animation_ms = timing_ms;

            let toggle = Toggle {
                style,
            };

            let label = format!("{}ms", timing_ms);
            toggles.push((toggle, ToggleState::default(), label));
        }

        // Create record buttons with different combinations
        let record_configs = vec![
            (50, 3000),   // Fast press, short recording
            (100, 5000),  // Normal press, medium recording
            (200, 9000),  // Slow press, long recording
            (500, 15000), // Very slow press, extra long recording
        ];
        let mut record_buttons = Vec::new();
        for &(press_ms, timeout_ms) in &record_configs {
            let mut style = RecordButtonStyle::default();
            style.press_animation_ms = press_ms;
            style.recording_timeout_ms = timeout_ms;

            let button = RecordButton {
                style,
            };

            let label = format!("P:{}ms T:{}s", press_ms, timeout_ms / 1000);
            record_buttons.push((button, RecordButtonState::default(), label));
        }

        // Create sliding switches with different timeouts
        let switch_timeouts = vec![1000, 2000, 5000, 10000];
        let mut sliding_switches = Vec::new();
        for &timeout_ms in &switch_timeouts {
            let mut style = SlidingSwitchStyle::default();
            style.inactive_timeout_ms = timeout_ms as u64;

            let switch = SlidingSwitch {
                style,
            };

            let label = format!("{}s timeout", timeout_ms / 1000);
            sliding_switches.push((switch, SlidingSwitchState::default(), label));
        }

        Self {
            badges,
            buttons,
            toggles,
            record_buttons,
            sliding_switches,
            time_ms: 0,
            last_trigger_ms: 0,
            trigger_interval_ms: 3000, // Trigger every 3 seconds
        }
    }
}

impl AnimationConfigScene {
    pub fn update(&mut self, dt_ms: u32) {
        self.time_ms += dt_ms as u64;

        // Trigger all badge bounces periodically
        if self.time_ms - self.last_trigger_ms >= self.trigger_interval_ms {
            self.last_trigger_ms = self.time_ms;
            for (badge, state, _) in &mut self.badges {
                state.bounce(&badge.style);
            }
        }

        // Auto-toggle the toggles periodically (staggered)
        for (i, (_toggle, state, _)) in self.toggles.iter_mut().enumerate() {
            let phase_offset = i as u64 * 500; // Stagger by 500ms each
            let should_be_on = ((self.time_ms + phase_offset) / 2000) % 2 == 0;
            state.set_on(should_be_on);
        }

        // Update toggle animations
        for (_toggle, state, _) in &mut self.toggles {
            state.step(dt_ms);
        }
    }

    pub fn input_pointer(&mut self, x: f32, y: f32, _dx: f32, _dy: f32, buttons: u32) {
        // Button interaction zones (4 columns)
        for (i, (button, state, _)) in self.buttons.iter_mut().enumerate() {
            let col = i % 2;
            let row = i / 2;
            let rect = gfx::RectF::new(
                50.0 + col as f32 * 180.0,
                120.0 + row as f32 * 50.0,
                160.0,
                40.0,
            );

            if point_in_rect([x, y], rect) {
                if buttons & 1 != 0 {
                    state.on_pointer_down();
                } else {
                    state.on_pointer_up();
                }
            } else if buttons == 0 {
                state.on_pointer_cancel();
            }
        }

        // Badge interaction zones
        for (i, (badge, state, _)) in self.badges.iter_mut().enumerate() {
            let col = i % 4;
            let rect = gfx::RectF::new(
                50.0 + col as f32 * 100.0,
                40.0,
                60.0,
                60.0,
            );

            if buttons & 1 != 0 && point_in_rect([x, y], rect) {
                state.bounce(&badge.style);
                badge.count = (badge.count % 99) + 1;
            }
        }

        // Toggle interaction zones
        for (i, (toggle, state, _)) in self.toggles.iter_mut().enumerate() {
            let col = i % 4;
            let rect = gfx::RectF::new(
                50.0 + col as f32 * 100.0,
                240.0,
                60.0,
                30.0,
            );

            if buttons & 1 != 0 && point_in_rect([x, y], rect) {
                let current_on = state.on;
                state.set_on(!current_on);
            }
        }

        // Record button interaction zones
        for (i, (button, state, _)) in self.record_buttons.iter_mut().enumerate() {
            let col = i % 4;
            let rect = gfx::RectF::new(
                50.0 + col as f32 * 100.0,
                320.0,
                80.0,
                80.0,
            );

            if point_in_rect([x, y], rect) {
                if buttons & 1 != 0 {
                    state.on_pointer_down(&button.style);
                } else {
                    let was_pressed = state.on_pointer_up(&button.style);
                    if was_pressed {
                        // Toggle recording
                        if state.is_recording() {
                            state.stop_recording();
                        } else {
                            state.start_recording(&button.style);
                        }
                    }
                }
            }
        }
    }

    pub fn draw<U: ImageUploader>(
        &mut self,
        viewport: gfx::RectF,
        device_scale: f32,
        text: &mut TextCtx,
        uploader: &mut U,
        builder: &mut DrawListBuilder,
    ) {
        // Background
        builder.rrect(viewport, [0.0; 4], gfx::Color::rgba(0.98, 0.98, 0.99, 1.0));

        // Title
        let title = Label {
            text: "Animation Timing Test".into(),
            color: gfx::Color::rgba(0.1, 0.1, 0.1, 1.0),
            align: Align::Center,
            wrap: false,
            font_id: 0,
            font_px: 22.0,
        };
        title.encode(
            gfx::RectF::new(viewport.x, viewport.y + 5.0, viewport.w, 30.0),
            device_scale,
            text,
            uploader,
            builder,
        );

        // Draw badges row
        let badge_label = Label {
            text: "Badges (click to bounce):".into(),
            color: gfx::Color::rgba(0.3, 0.3, 0.3, 1.0),
            align: Align::Left,
            wrap: false,
            font_id: 0,
            font_px: 11.0,
        };
        badge_label.encode(
            gfx::RectF::new(viewport.x + 450.0, viewport.y + 55.0, 200.0, 20.0),
            device_scale,
            text,
            uploader,
            builder,
        );

        for (i, (badge, state, label)) in self.badges.iter_mut().enumerate() {
            let col = i % 4;
            let x = viewport.x + 50.0 + col as f32 * 100.0;
            let y = viewport.y + 40.0;

            badge.encode(
                gfx::RectF::new(x, y, 60.0, 60.0),
                device_scale,
                text,
                uploader,
                state,
                builder,
            );

            // Timing label
            let timing_label = Label {
                text: label.clone(),
                color: gfx::Color::rgba(0.5, 0.5, 0.5, 1.0),
                align: Align::Center,
                wrap: false,
                font_id: 0,
                font_px: 10.0,
            };
            timing_label.encode(
                gfx::RectF::new(x - 10.0, y + 65.0, 80.0, 15.0),
                device_scale,
                text,
                uploader,
                builder,
            );
        }

        // Draw buttons row
        let button_label = Label {
            text: "Buttons (press to test):".into(),
            color: gfx::Color::rgba(0.3, 0.3, 0.3, 1.0),
            align: Align::Left,
            wrap: false,
            font_id: 0,
            font_px: 11.0,
        };
        button_label.encode(
            gfx::RectF::new(viewport.x + 450.0, viewport.y + 140.0, 200.0, 20.0),
            device_scale,
            text,
            uploader,
            builder,
        );

        for (i, (button, state, _)) in self.buttons.iter_mut().enumerate() {
            let col = i % 2;
            let row = i / 2;

            button.encode(
                gfx::RectF::new(
                    viewport.x + 50.0 + col as f32 * 180.0,
                    viewport.y + 120.0 + row as f32 * 50.0,
                    160.0,
                    40.0,
                ),
                device_scale,
                text,
                uploader,
                state,
                builder,
            );
        }

        // Draw toggles row
        let toggle_label = Label {
            text: "Toggles (auto-toggling):".into(),
            color: gfx::Color::rgba(0.3, 0.3, 0.3, 1.0),
            align: Align::Left,
            wrap: false,
            font_id: 0,
            font_px: 11.0,
        };
        toggle_label.encode(
            gfx::RectF::new(viewport.x + 450.0, viewport.y + 245.0, 200.0, 20.0),
            device_scale,
            text,
            uploader,
            builder,
        );

        for (i, (toggle, state, label)) in self.toggles.iter().enumerate() {
            let col = i % 4;
            let x = viewport.x + 50.0 + col as f32 * 100.0;
            let y = viewport.y + 240.0;

            toggle.encode(
                gfx::RectF::new(x, y, 60.0, 30.0),
                state,
                builder,
            );

            // Timing label
            let timing_label = Label {
                text: label.clone(),
                color: gfx::Color::rgba(0.5, 0.5, 0.5, 1.0),
                align: Align::Center,
                wrap: false,
                font_id: 0,
                font_px: 10.0,
            };
            timing_label.encode(
                gfx::RectF::new(x - 10.0, y + 32.0, 80.0, 15.0),
                device_scale,
                text,
                uploader,
                builder,
            );
        }

        // Draw record buttons row
        let record_label = Label {
            text: "Record Buttons:".into(),
            color: gfx::Color::rgba(0.3, 0.3, 0.3, 1.0),
            align: Align::Left,
            wrap: false,
            font_id: 0,
            font_px: 11.0,
        };
        record_label.encode(
            gfx::RectF::new(viewport.x + 450.0, viewport.y + 350.0, 200.0, 20.0),
            device_scale,
            text,
            uploader,
            builder,
        );

        for (i, (button, state, label)) in self.record_buttons.iter().enumerate() {
            let col = i % 4;
            let x = viewport.x + 50.0 + col as f32 * 100.0;
            let y = viewport.y + 320.0;

            button.encode(
                gfx::RectF::new(x, y, 80.0, 80.0),
                state,
                builder,
            );

            // Config label
            let config_label = Label {
                text: label.clone(),
                color: gfx::Color::rgba(0.5, 0.5, 0.5, 1.0),
                align: Align::Center,
                wrap: false,
                font_id: 0,
                font_px: 9.0,
            };
            config_label.encode(
                gfx::RectF::new(x - 10.0, y + 82.0, 100.0, 15.0),
                device_scale,
                text,
                uploader,
                builder,
            );
        }

        // Info footer
        let info = Label {
            text: format!(
                "Testing {} timing variations | Time: {:.1}s | All animations using per-component config",
                self.badges.len() + self.buttons.len() + self.toggles.len() + self.record_buttons.len(),
                self.time_ms as f32 / 1000.0
            ),
            color: gfx::Color::rgba(0.4, 0.4, 0.4, 1.0),
            align: Align::Center,
            wrap: false,
            font_id: 0,
            font_px: 11.0,
        };
        info.encode(
            gfx::RectF::new(viewport.x, viewport.y + viewport.h - 25.0, viewport.w, 20.0),
            device_scale,
            text,
            uploader,
            builder,
        );
    }
}

fn point_in_rect(point: [f32; 2], rect: gfx::RectF) -> bool {
    point[0] >= rect.x && point[0] <= rect.x + rect.w &&
    point[1] >= rect.y && point[1] <= rect.y + rect.h
}