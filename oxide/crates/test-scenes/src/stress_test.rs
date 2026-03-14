//! Performance Stress Test Scene - Tests rendering and update performance under load
//!
//! This scene creates extreme conditions to test performance characteristics
//! and identify bottlenecks in the UI system.

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::cmp::Ordering;
use oxide_renderer_api as gfx;
use oxide_timing as timing;
use oxide_ui_core::{
    elements::{
        Align, Badge, BadgeState, BadgeStyle, Button, ButtonState, ButtonStyle, ImageUploader,
        Label, ProgressBar, RecordButton, RecordButtonState, RecordButtonStyle, TextCtx, Toggle,
        ToggleState, ToggleStyle,
    },
    DrawListBuilder,
};

/// Performance metrics tracker
struct PerformanceMetrics {
    frame_count: u64,
    total_time_ms: u64,
    last_frame_ms: u64,

    // Frame timing stats
    min_frame_ms: u32,
    max_frame_ms: u32,
    avg_frame_ms: f32,

    // Update timing
    draw_time_ms: u32,

    // Counts
    element_count: usize,
    animation_count: usize,
    draw_call_count: usize,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            frame_count: 0,
            total_time_ms: 0,
            last_frame_ms: 0,
            min_frame_ms: u32::MAX,
            max_frame_ms: 0,
            avg_frame_ms: 0.0,
            draw_time_ms: 0,
            element_count: 0,
            animation_count: 0,
            draw_call_count: 0,
        }
    }
}

impl PerformanceMetrics {
    fn update_frame(&mut self, dt_ms: u32) {
        self.frame_count += 1;
        self.total_time_ms += dt_ms as u64;

        self.min_frame_ms = self.min_frame_ms.min(dt_ms);
        self.max_frame_ms = self.max_frame_ms.max(dt_ms);

        if self.frame_count > 0 {
            self.avg_frame_ms = self.total_time_ms as f32 / self.frame_count as f32;
        }

        self.last_frame_ms = timing::now_ms();
    }

    fn fps(&self) -> f32 {
        if self.avg_frame_ms > 0.0 {
            1000.0 / self.avg_frame_ms
        } else {
            0.0
        }
    }
}

/// Different stress test modes
#[derive(Clone, Copy, Debug, PartialEq)]
enum StressTestMode {
    ManyButtons,
    ManyBadges,
    ManyAnimations,
    LargeText,
    ComplexLayouts,
    RapidUpdates,
    MixedChaos,
}

/// Animated element for stress testing
struct AnimatedElement {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: gfx::Color,
    velocity_x: f32,
    velocity_y: f32,
    rotation: f32,
    rotation_speed: f32,
    scale: f32,
    scale_direction: f32,
}

impl AnimatedElement {
    fn new(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            width: 20.0 + (x * 7.0).rem_euclid(40.0),
            height: 20.0 + (y * 13.0).rem_euclid(40.0),
            color: gfx::Color::rgba(
                (x * 0.001) % 1.0,
                (y * 0.001) % 1.0,
                ((x + y) * 0.001) % 1.0,
                0.8,
            ),
            velocity_x: ((x * 17.0).rem_euclid(200.0) - 100.0) * 0.01,
            velocity_y: ((y * 23.0).rem_euclid(200.0) - 100.0) * 0.01,
            rotation: 0.0,
            rotation_speed: ((x + y) * 0.1) % 5.0 - 2.5,
            scale: 1.0,
            scale_direction: 1.0,
        }
    }

    fn update(&mut self, dt_ms: u32, bounds: gfx::RectF) {
        // Update position
        self.x += self.velocity_x * dt_ms as f32;
        self.y += self.velocity_y * dt_ms as f32;

        // Bounce off walls
        if self.x < bounds.x || self.x + self.width > bounds.x + bounds.w {
            self.velocity_x = -self.velocity_x;
            self.x = self.x.max(bounds.x).min(bounds.x + bounds.w - self.width);
        }
        if self.y < bounds.y || self.y + self.height > bounds.y + bounds.h {
            self.velocity_y = -self.velocity_y;
            self.y = self.y.max(bounds.y).min(bounds.y + bounds.h - self.height);
        }

        // Update rotation
        self.rotation += self.rotation_speed * dt_ms as f32 * 0.001;

        // Update scale
        self.scale += self.scale_direction * dt_ms as f32 * 0.0005;
        if self.scale > 1.5 || self.scale < 0.5 {
            self.scale_direction = -self.scale_direction;
        }
    }

    fn draw(&self, builder: &mut DrawListBuilder) {
        let rect =
            gfx::RectF::new(self.x, self.y, self.width * self.scale, self.height * self.scale);
        builder.rrect(rect, [4.0; 4], self.color);
    }
}

/// Performance stress test scene
pub struct StressTestScene {
    // Test mode
    current_mode: StressTestMode,
    mode_buttons: Vec<(Button, ButtonState)>,

    // Performance metrics
    metrics: PerformanceMetrics,

    // UI elements for different stress tests
    buttons: Vec<(Button, ButtonState)>,
    badges: Vec<(Badge, BadgeState)>,
    toggles: Vec<(Toggle, ToggleState)>,
    progress_bars: Vec<ProgressBar>,
    record_buttons: Vec<(RecordButton, RecordButtonState)>,

    // Animated elements
    animated_elements: Vec<AnimatedElement>,

    // Text stress test
    large_text_lines: Vec<String>,

    // Controls
    element_count: usize,
    target_element_count: usize,
    auto_increase: bool,
    increase_rate: u32,
}

impl Default for StressTestScene {
    fn default() -> Self {
        let modes = vec![
            StressTestMode::ManyButtons,
            StressTestMode::ManyBadges,
            StressTestMode::ManyAnimations,
            StressTestMode::LargeText,
            StressTestMode::ComplexLayouts,
            StressTestMode::RapidUpdates,
            StressTestMode::MixedChaos,
        ];

        let mut mode_buttons = Vec::new();
        for mode in &modes {
            let text = match mode {
                StressTestMode::ManyButtons => "Many Buttons",
                StressTestMode::ManyBadges => "Many Badges",
                StressTestMode::ManyAnimations => "Animations",
                StressTestMode::LargeText => "Large Text",
                StressTestMode::ComplexLayouts => "Complex Layout",
                StressTestMode::RapidUpdates => "Rapid Updates",
                StressTestMode::MixedChaos => "Mixed Chaos",
            };

            mode_buttons.push((
                Button { text: text.into(), style: ButtonStyle::default() },
                ButtonState::default(),
            ));
        }

        Self {
            current_mode: StressTestMode::ManyButtons,
            mode_buttons,
            metrics: PerformanceMetrics::default(),
            buttons: Vec::new(),
            badges: Vec::new(),
            toggles: Vec::new(),
            progress_bars: Vec::new(),
            record_buttons: Vec::new(),
            animated_elements: Vec::new(),
            large_text_lines: Vec::new(),
            element_count: 100,
            target_element_count: 100,
            auto_increase: false,
            increase_rate: 10,
        }
    }
}

impl StressTestScene {
    pub fn update(&mut self, dt_ms: u32) {
        self.metrics.update_frame(dt_ms);

        // Auto-increase element count if enabled
        if self.auto_increase && self.element_count < 10000 && self.metrics.frame_count % 60 == 0 {
            self.target_element_count += self.increase_rate as usize;
        }

        match self.element_count.cmp(&self.target_element_count) {
            Ordering::Less => {
                self.element_count = (self.element_count + 10).min(self.target_element_count);
                self.regenerate_elements();
            }
            Ordering::Greater => {
                self.element_count = (self.element_count - 10).max(self.target_element_count);
                self.regenerate_elements();
            }
            Ordering::Equal => {}
        }

        // Mode-specific updates
        match self.current_mode {
            StressTestMode::ManyButtons => {
                // Randomly press/release buttons
                for (i, (_, state)) in self.buttons.iter_mut().enumerate() {
                    if i as u64 % 100 == self.metrics.frame_count % 100 {
                        // Toggle button press state
                        state.on_pointer_down();
                        state.on_pointer_up();
                    }
                }
            }

            StressTestMode::ManyBadges => {
                // Animate badges
                for (i, (badge, state)) in self.badges.iter_mut().enumerate() {
                    if i as u64 % 30 == self.metrics.frame_count % 30 {
                        badge.count = (badge.count % 99) + 1;
                        state.bounce(&badge.style);
                    }
                    // Badge animation handled internally
                }
            }

            StressTestMode::ManyAnimations => {
                // Update animated elements
                let bounds = gfx::RectF::new(100.0, 150.0, 600.0, 400.0);
                for element in &mut self.animated_elements {
                    element.update(dt_ms, bounds);
                }
            }

            StressTestMode::RapidUpdates => {
                // Toggle states rapidly
                for (i, (_, state)) in self.toggles.iter_mut().enumerate() {
                    if i as u64 % 5 == self.metrics.frame_count % 5 {
                        state.set_on(!state.on);
                    }
                    state.step(dt_ms);
                }

                // Update progress bars
                for (i, bar) in self.progress_bars.iter_mut().enumerate() {
                    if let Some(value) = bar.value {
                        let new_value = (value + 0.01 * (i as f32 + 1.0)) % 1.0;
                        bar.value = Some(new_value);
                    }
                }
            }

            StressTestMode::MixedChaos => {
                // Update everything at once
                for (i, (_, state)) in self.buttons.iter_mut().enumerate() {
                    if i as u64 % 50 == self.metrics.frame_count % 50 {
                        // Toggle button press state
                        state.on_pointer_down();
                        state.on_pointer_up();
                    }
                }

                for (i, (badge, state)) in self.badges.iter_mut().enumerate() {
                    if i as u64 % 20 == self.metrics.frame_count % 20 {
                        badge.count = (badge.count % 99) + 1;
                        state.bounce(&badge.style);
                    }
                    // Badge animation handled internally
                }

                let bounds = gfx::RectF::new(100.0, 150.0, 600.0, 400.0);
                for element in &mut self.animated_elements {
                    element.update(dt_ms, bounds);
                }
            }

            _ => {}
        }

        self.metrics.animation_count = self.animated_elements.len();
    }

    fn regenerate_elements(&mut self) {
        match self.current_mode {
            StressTestMode::ManyButtons => {
                self.buttons.clear();
                for i in 0..self.element_count {
                    self.buttons.push((
                        Button { text: format!("Btn {}", i), style: ButtonStyle::default() },
                        ButtonState::default(),
                    ));
                }
            }

            StressTestMode::ManyBadges => {
                self.badges.clear();
                for i in 0..self.element_count {
                    let style = BadgeStyle {
                        bounce_duration_ms: 100 + (i as u32 * 50) % 400,
                        ..BadgeStyle::default()
                    };

                    self.badges
                        .push((Badge { count: (i % 100) as u32, style }, BadgeState::default()));
                }
            }

            StressTestMode::ManyAnimations => {
                self.animated_elements.clear();
                let grid_size = (self.element_count as f32).sqrt() as usize;
                for i in 0..self.element_count {
                    let x = 100.0 + (i % grid_size) as f32 * 30.0;
                    let y = 150.0 + (i / grid_size) as f32 * 30.0;
                    self.animated_elements.push(AnimatedElement::new(x, y));
                }
            }

            StressTestMode::LargeText => {
                self.large_text_lines.clear();
                for i in 0..self.element_count {
                    self.large_text_lines.push(format!(
                        "Line {}: Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.",
                        i
                    ));
                }
            }

            StressTestMode::ComplexLayouts => {
                self.buttons.clear();
                self.badges.clear();
                self.toggles.clear();
                self.progress_bars.clear();

                let each_type = self.element_count / 4;

                for i in 0..each_type {
                    self.buttons.push((
                        Button { text: format!("B{}", i), style: ButtonStyle::default() },
                        ButtonState::default(),
                    ));

                    self.badges.push((
                        Badge { count: i as u32, style: BadgeStyle::default() },
                        BadgeState::default(),
                    ));

                    self.toggles
                        .push((Toggle { style: ToggleStyle::default() }, ToggleState::default()));

                    self.progress_bars.push(ProgressBar {
                        value: Some((i as f32 / each_type as f32).min(1.0)),
                        track: gfx::Color::rgba(0.9, 0.9, 0.9, 1.0),
                        fill: gfx::Color::rgba(0.2, 0.6, 0.9, 1.0),
                        corner: 2.0,
                    });
                }
            }

            StressTestMode::RapidUpdates => {
                self.toggles.clear();
                self.progress_bars.clear();

                for i in 0..self.element_count {
                    self.toggles
                        .push((Toggle { style: ToggleStyle::default() }, ToggleState::default()));

                    self.progress_bars.push(ProgressBar {
                        value: Some((i as f32 / self.element_count as f32).min(1.0)),
                        track: gfx::Color::rgba(0.8, 0.8, 0.8, 1.0),
                        fill: gfx::Color::rgba(0.3, 0.7, 0.3, 1.0),
                        corner: 3.0,
                    });
                }
            }

            StressTestMode::MixedChaos => {
                self.regenerate_all();
            }
        }

        self.metrics.element_count = self.buttons.len()
            + self.badges.len()
            + self.toggles.len()
            + self.animated_elements.len();
    }

    fn regenerate_all(&mut self) {
        let each = self.element_count / 5;

        self.buttons.clear();
        self.badges.clear();
        self.toggles.clear();
        self.animated_elements.clear();
        self.record_buttons.clear();

        for i in 0..each {
            self.buttons.push((
                Button { text: format!("{}", i), style: ButtonStyle::default() },
                ButtonState::default(),
            ));

            self.badges.push((
                Badge { count: i as u32, style: BadgeStyle::default() },
                BadgeState::default(),
            ));

            self.toggles.push((Toggle { style: ToggleStyle::default() }, ToggleState::default()));

            let x = 100.0 + (i % 20) as f32 * 30.0;
            let y = 150.0 + (i / 20) as f32 * 30.0;
            self.animated_elements.push(AnimatedElement::new(x, y));

            if i < each / 2 {
                self.record_buttons.push((
                    RecordButton { style: RecordButtonStyle::default() },
                    RecordButtonState::default(),
                ));
            }
        }
    }

    pub fn input_pointer(&mut self, x: f32, y: f32, _dx: f32, _dy: f32, buttons: u32) {
        // Mode selector buttons
        let mut mode_to_set = None;
        for (i, (_, state)) in self.mode_buttons.iter_mut().enumerate() {
            let rect = gfx::RectF::new(
                50.0 + (i % 4) as f32 * 120.0,
                50.0 + (i / 4) as f32 * 35.0,
                110.0,
                30.0,
            );

            if point_in_rect([x, y], rect) {
                if buttons & 1 != 0 {
                    state.on_pointer_down();
                } else if state.on_pointer_up() {
                    mode_to_set = Some(match i {
                        0 => StressTestMode::ManyButtons,
                        1 => StressTestMode::ManyBadges,
                        2 => StressTestMode::ManyAnimations,
                        3 => StressTestMode::LargeText,
                        4 => StressTestMode::ComplexLayouts,
                        5 => StressTestMode::RapidUpdates,
                        _ => StressTestMode::MixedChaos,
                    });
                }
            } else if buttons == 0 {
                state.on_pointer_cancel();
            }
        }

        if let Some(mode) = mode_to_set {
            self.current_mode = mode;
            self.regenerate_elements();
        }

        // Element count controls
        let increase_rect = gfx::RectF::new(550.0, 50.0, 50.0, 30.0);
        let decrease_rect = gfx::RectF::new(610.0, 50.0, 50.0, 30.0);
        let auto_rect = gfx::RectF::new(670.0, 50.0, 80.0, 30.0);

        if buttons & 1 != 0 {
            if point_in_rect([x, y], increase_rect) {
                self.target_element_count = (self.target_element_count + 50).min(10000);
            } else if point_in_rect([x, y], decrease_rect) {
                self.target_element_count = (self.target_element_count.saturating_sub(50)).max(10);
            } else if point_in_rect([x, y], auto_rect) {
                self.auto_increase = !self.auto_increase;
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
        let draw_start = timing::now_ms();

        // Background
        builder.rrect(viewport, [0.0; 4], gfx::Color::rgba(0.1, 0.1, 0.1, 1.0));

        // Title
        let title = Label {
            text: "Performance Stress Test".into(),
            color: gfx::Color::rgba(1.0, 1.0, 1.0, 1.0),
            align: Align::Center,
            wrap: false,
            font_id: 0,
            font_px: 20.0,
        };
        title.encode(
            gfx::RectF::new(viewport.x, viewport.y + 5.0, viewport.w, 30.0),
            device_scale,
            text,
            uploader,
            builder,
        );

        // Mode selector buttons
        for (i, ((button, state), mode)) in self
            .mode_buttons
            .iter_mut()
            .zip(
                [
                    StressTestMode::ManyButtons,
                    StressTestMode::ManyBadges,
                    StressTestMode::ManyAnimations,
                    StressTestMode::LargeText,
                    StressTestMode::ComplexLayouts,
                    StressTestMode::RapidUpdates,
                    StressTestMode::MixedChaos,
                ]
                .iter(),
            )
            .enumerate()
        {
            let is_active = self.current_mode == *mode;

            let rect = gfx::RectF::new(
                viewport.x + 50.0 + (i % 4) as f32 * 120.0,
                viewport.y + 50.0 + (i / 4) as f32 * 35.0,
                110.0,
                30.0,
            );

            if is_active {
                builder.rrect(
                    gfx::RectF::new(rect.x - 2.0, rect.y - 2.0, rect.w + 4.0, rect.h + 4.0),
                    [6.0; 4],
                    gfx::Color::rgba(0.3, 0.6, 0.9, 0.5),
                );
            }

            button.encode(rect, device_scale, text, uploader, state, builder);
        }

        // Control buttons
        let controls_y = viewport.y + 50.0;

        // Element count display
        let count_label = Label {
            text: format!("Elements: {}", self.element_count),
            color: gfx::Color::rgba(1.0, 1.0, 1.0, 1.0),
            align: Align::Right,
            wrap: false,
            font_id: 0,
            font_px: 12.0,
        };
        count_label.encode(
            gfx::RectF::new(viewport.x + 450.0, controls_y, 90.0, 30.0),
            device_scale,
            text,
            uploader,
            builder,
        );

        // Increase/Decrease buttons
        builder.rrect(
            gfx::RectF::new(viewport.x + 550.0, controls_y, 50.0, 30.0),
            [4.0; 4],
            gfx::Color::rgba(0.2, 0.6, 0.2, 1.0),
        );
        let plus_label = Label {
            text: "+50".into(),
            color: gfx::Color::rgba(1.0, 1.0, 1.0, 1.0),
            align: Align::Center,
            wrap: false,
            font_id: 0,
            font_px: 14.0,
        };
        plus_label.encode(
            gfx::RectF::new(viewport.x + 550.0, controls_y + 5.0, 50.0, 20.0),
            device_scale,
            text,
            uploader,
            builder,
        );

        builder.rrect(
            gfx::RectF::new(viewport.x + 610.0, controls_y, 50.0, 30.0),
            [4.0; 4],
            gfx::Color::rgba(0.6, 0.2, 0.2, 1.0),
        );
        let minus_label = Label {
            text: "-50".into(),
            color: gfx::Color::rgba(1.0, 1.0, 1.0, 1.0),
            align: Align::Center,
            wrap: false,
            font_id: 0,
            font_px: 14.0,
        };
        minus_label.encode(
            gfx::RectF::new(viewport.x + 610.0, controls_y + 5.0, 50.0, 20.0),
            device_scale,
            text,
            uploader,
            builder,
        );

        // Auto increase toggle
        builder.rrect(
            gfx::RectF::new(viewport.x + 670.0, controls_y, 80.0, 30.0),
            [4.0; 4],
            if self.auto_increase {
                gfx::Color::rgba(0.2, 0.6, 0.9, 1.0)
            } else {
                gfx::Color::rgba(0.3, 0.3, 0.3, 1.0)
            },
        );
        let auto_label = Label {
            text: if self.auto_increase { "Auto: ON" } else { "Auto: OFF" }.into(),
            color: gfx::Color::rgba(1.0, 1.0, 1.0, 1.0),
            align: Align::Center,
            wrap: false,
            font_id: 0,
            font_px: 12.0,
        };
        auto_label.encode(
            gfx::RectF::new(viewport.x + 670.0, controls_y + 5.0, 80.0, 20.0),
            device_scale,
            text,
            uploader,
            builder,
        );

        // Draw test content based on mode
        let content_area = gfx::RectF::new(
            viewport.x + 50.0,
            viewport.y + 130.0,
            viewport.w - 100.0,
            viewport.h - 200.0,
        );

        // Clip to content area
        builder.clip_push(rectf_to_recti(content_area));

        self.metrics.draw_call_count = 0;

        match self.current_mode {
            StressTestMode::ManyButtons => {
                let cols = 20;
                for (i, (button, state)) in self.buttons.iter_mut().enumerate() {
                    let col = i % cols;
                    let row = i / cols;
                    let rect = gfx::RectF::new(
                        content_area.x + col as f32 * 40.0,
                        content_area.y + row as f32 * 25.0,
                        38.0,
                        22.0,
                    );

                    if rect.y < content_area.y + content_area.h {
                        button.encode(rect, device_scale, text, uploader, state, builder);
                        self.metrics.draw_call_count += 1;
                    }
                }
            }

            StressTestMode::ManyBadges => {
                let cols = 15;
                for (i, (badge, state)) in self.badges.iter_mut().enumerate() {
                    let col = i % cols;
                    let row = i / cols;
                    let rect = gfx::RectF::new(
                        content_area.x + col as f32 * 50.0,
                        content_area.y + row as f32 * 50.0,
                        40.0,
                        40.0,
                    );

                    if rect.y < content_area.y + content_area.h {
                        badge.encode(rect, device_scale, text, uploader, state, builder);
                        self.metrics.draw_call_count += 1;
                    }
                }
            }

            StressTestMode::ManyAnimations => {
                for element in &self.animated_elements {
                    element.draw(builder);
                    self.metrics.draw_call_count += 1;
                }
            }

            StressTestMode::LargeText => {
                for (i, line) in self.large_text_lines.iter().enumerate() {
                    let y = content_area.y + i as f32 * 18.0;
                    if y < content_area.y + content_area.h {
                        let text_label = Label {
                            text: line.clone(),
                            color: gfx::Color::rgba(0.9, 0.9, 0.9, 1.0),
                            align: Align::Left,
                            wrap: true,
                            font_id: 0,
                            font_px: 11.0,
                        };
                        text_label.encode(
                            gfx::RectF::new(content_area.x, y, content_area.w, 16.0),
                            device_scale,
                            text,
                            uploader,
                            builder,
                        );
                        self.metrics.draw_call_count += 1;
                    }
                }
            }

            StressTestMode::ComplexLayouts => {
                let mut y_offset = 0.0;

                // Draw buttons row
                for (i, (button, state)) in self.buttons.iter_mut().enumerate() {
                    if i >= 20 {
                        break;
                    }
                    let rect = gfx::RectF::new(
                        content_area.x + (i % 10) as f32 * 70.0,
                        content_area.y + (i / 10) as f32 * 30.0,
                        65.0,
                        25.0,
                    );
                    button.encode(rect, device_scale, text, uploader, state, builder);
                    self.metrics.draw_call_count += 1;
                }
                y_offset += 70.0;

                // Draw badges row
                for (i, (badge, state)) in self.badges.iter_mut().enumerate() {
                    if i >= 15 {
                        break;
                    }
                    let rect = gfx::RectF::new(
                        content_area.x + i as f32 * 50.0,
                        content_area.y + y_offset,
                        40.0,
                        40.0,
                    );
                    badge.encode(rect, device_scale, text, uploader, state, builder);
                    self.metrics.draw_call_count += 1;
                }
                y_offset += 50.0;

                // Draw toggles row
                for (i, (toggle, state)) in self.toggles.iter_mut().enumerate() {
                    if i >= 20 {
                        break;
                    }
                    let rect = gfx::RectF::new(
                        content_area.x + i as f32 * 40.0,
                        content_area.y + y_offset,
                        35.0,
                        20.0,
                    );
                    toggle.encode(rect, state, builder);
                    self.metrics.draw_call_count += 1;
                }
                y_offset += 30.0;

                // Draw progress bars
                for (i, bar) in self.progress_bars.iter().enumerate() {
                    if i >= 10 {
                        break;
                    }
                    let rect = gfx::RectF::new(
                        content_area.x,
                        content_area.y + y_offset + i as f32 * 15.0,
                        content_area.w * 0.8,
                        10.0,
                    );
                    bar.encode(rect, i as f32 * 0.1, builder);
                    self.metrics.draw_call_count += 1;
                }
            }

            StressTestMode::RapidUpdates => {
                // Draw toggles in grid
                let cols = 20;
                for (i, (toggle, state)) in self.toggles.iter().enumerate() {
                    let col = i % cols;
                    let row = i / cols;
                    let rect = gfx::RectF::new(
                        content_area.x + col as f32 * 35.0,
                        content_area.y + row as f32 * 25.0,
                        30.0,
                        20.0,
                    );

                    if rect.y < content_area.y + content_area.h * 0.5 {
                        toggle.encode(rect, state, builder);
                        self.metrics.draw_call_count += 1;
                    }
                }

                // Draw progress bars
                let bar_start_y = content_area.y + content_area.h * 0.5;
                for (i, bar) in self.progress_bars.iter().enumerate() {
                    let y = bar_start_y + i as f32 * 12.0;
                    if y < content_area.y + content_area.h {
                        let rect = gfx::RectF::new(content_area.x, y, content_area.w, 10.0);
                        bar.encode(rect, 0.0, builder);
                        self.metrics.draw_call_count += 1;
                    }
                }
            }

            StressTestMode::MixedChaos => {
                // Draw everything mixed together
                let mut draw_count = 0;
                let max_per_type = 50;

                for (i, (button, state)) in self.buttons.iter_mut().enumerate() {
                    if i >= max_per_type {
                        break;
                    }
                    let rect = gfx::RectF::new(
                        content_area.x + (i % 15) as f32 * 50.0,
                        content_area.y + (i / 15) as f32 * 30.0,
                        45.0,
                        25.0,
                    );
                    button.encode(rect, device_scale, text, uploader, state, builder);
                    draw_count += 1;
                }

                for (i, (badge, state)) in self.badges.iter_mut().enumerate() {
                    if i >= max_per_type {
                        break;
                    }
                    let rect = gfx::RectF::new(
                        content_area.x + (i % 12) as f32 * 60.0,
                        content_area.y + 150.0 + (i / 12) as f32 * 45.0,
                        35.0,
                        35.0,
                    );
                    badge.encode(rect, device_scale, text, uploader, state, builder);
                    draw_count += 1;
                }

                for element in &self.animated_elements {
                    if draw_count >= max_per_type * 3 {
                        break;
                    }
                    element.draw(builder);
                    draw_count += 1;
                }

                self.metrics.draw_call_count = draw_count;
            }
        }

        builder.clip_pop();

        // Performance metrics panel
        let metrics_bg = gfx::RectF::new(
            viewport.x + viewport.w - 250.0,
            viewport.y + viewport.h - 120.0,
            240.0,
            110.0,
        );
        builder.rrect(metrics_bg, [8.0; 4], gfx::Color::rgba(0.0, 0.0, 0.0, 0.8));

        let metrics_text = [
            format!("FPS: {:.1}", self.metrics.fps()),
            format!("Frame: {:.1}ms (avg)", self.metrics.avg_frame_ms),
            format!("Min/Max: {}/{}ms", self.metrics.min_frame_ms, self.metrics.max_frame_ms),
            format!("Elements: {}", self.metrics.element_count),
            format!("Draw Calls: {}", self.metrics.draw_call_count),
            format!("Mode: {:?}", self.current_mode),
        ];

        for (i, line) in metrics_text.iter().enumerate() {
            let color = if i == 0 {
                // Color code FPS
                if self.metrics.fps() >= 60.0 {
                    gfx::Color::rgba(0.2, 0.9, 0.2, 1.0)
                } else if self.metrics.fps() >= 30.0 {
                    gfx::Color::rgba(0.9, 0.9, 0.2, 1.0)
                } else {
                    gfx::Color::rgba(0.9, 0.2, 0.2, 1.0)
                }
            } else {
                gfx::Color::rgba(0.9, 0.9, 0.9, 1.0)
            };

            let metric_label = Label {
                text: line.clone(),
                color,
                align: Align::Left,
                wrap: false,
                font_id: 0,
                font_px: 11.0,
            };
            metric_label.encode(
                gfx::RectF::new(
                    metrics_bg.x + 10.0,
                    metrics_bg.y + 10.0 + i as f32 * 16.0,
                    220.0,
                    14.0,
                ),
                device_scale,
                text,
                uploader,
                builder,
            );
        }

        self.metrics.draw_time_ms = (timing::now_ms() - draw_start) as u32;
    }
}

fn point_in_rect(point: [f32; 2], rect: gfx::RectF) -> bool {
    point[0] >= rect.x
        && point[0] <= rect.x + rect.w
        && point[1] >= rect.y
        && point[1] <= rect.y + rect.h
}

fn rectf_to_recti(r: gfx::RectF) -> gfx::RectI {
    let x = r.x.floor() as i32;
    let y = r.y.floor() as i32;
    let w = r.w.ceil() as i32;
    let h = r.h.ceil() as i32;
    gfx::RectI { x, y, w, h }
}
