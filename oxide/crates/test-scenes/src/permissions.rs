//! Permissions UI Test Scene - Tests all 8 permission domains
//!
//! This scene validates permission UI overlays, state management, and user flows.

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use oxide_permissions::PermissionState;
use oxide_platform_api::{PermissionDomain, PermissionStatus};
use oxide_renderer_api as gfx;
use oxide_timing as timing;
use oxide_ui_core::{
    elements::{
        Align, Button, ButtonState, ButtonStyle, ImageUploader, Label, TextCtx, Toggle,
        ToggleState, ToggleStyle,
    },
    permissions::PermissionOverlayUi,
    DrawListBuilder,
};

/// Visual representation of a permission domain
struct PermissionCard {
    domain: PermissionDomain,
    state: PermissionState,
    button: Button,
    button_state: ButtonState,
    toggle: Toggle,
    toggle_state: ToggleState,
    last_request_ms: u64,
    request_count: u32,
}

impl PermissionCard {
    fn new(domain: PermissionDomain) -> Self {
        let state =
            PermissionState { domain, status: PermissionStatus::NotDetermined, last_changed_ms: 0 };

        let button_text = match domain {
            PermissionDomain::Camera => "Request Camera",
            PermissionDomain::Microphone => "Request Microphone",
            PermissionDomain::Location => "Request Location",
            PermissionDomain::Bluetooth => "Request Bluetooth",
            PermissionDomain::Motion => "Request Motion",
            PermissionDomain::Notifications => "Request Notifications",
            PermissionDomain::Contacts => "Request Contacts",
            PermissionDomain::MediaLibrary => "Request Media Library",
        };

        Self {
            domain,
            state,
            button: Button { text: button_text.into(), style: ButtonStyle::default() },
            button_state: ButtonState::default(),
            toggle: Toggle { style: ToggleStyle::default() },
            toggle_state: ToggleState::default(),
            last_request_ms: 0,
            request_count: 0,
        }
    }

    fn simulate_status_change(&mut self) {
        // Simulate permission state transitions
        self.state.status = match self.state.status {
            PermissionStatus::NotDetermined => {
                if self.toggle_state.on {
                    PermissionStatus::Authorized
                } else {
                    PermissionStatus::Denied
                }
            }
            PermissionStatus::Denied => {
                if self.toggle_state.on {
                    PermissionStatus::Authorized
                } else {
                    PermissionStatus::Denied
                }
            }
            PermissionStatus::Authorized => {
                if !self.toggle_state.on {
                    PermissionStatus::Denied
                } else {
                    PermissionStatus::Authorized
                }
            }
            PermissionStatus::Limited => PermissionStatus::Limited,
        };
        self.state.last_changed_ms = timing::now_ms();
    }

    fn get_status_color(&self) -> gfx::Color {
        match self.state.status {
            PermissionStatus::NotDetermined => gfx::Color::rgba(0.5, 0.5, 0.5, 1.0),
            PermissionStatus::Authorized => gfx::Color::rgba(0.2, 0.8, 0.3, 1.0),
            PermissionStatus::Denied => gfx::Color::rgba(0.9, 0.3, 0.3, 1.0),
            PermissionStatus::Limited => gfx::Color::rgba(0.8, 0.5, 0.2, 1.0),
        }
    }

    fn get_status_text(&self) -> &'static str {
        match self.state.status {
            PermissionStatus::NotDetermined => "Not Determined",
            PermissionStatus::Authorized => "Authorized",
            PermissionStatus::Denied => "Denied",
            PermissionStatus::Limited => "Limited",
        }
    }
}

/// Test scene for permissions UI
pub struct PermissionsScene {
    /// All 8 permission cards
    cards: Vec<PermissionCard>,

    /// Permission manager for testing
    permission_states: Vec<PermissionState>,

    /// Overlay UI for rendering permission prompts
    overlay_ui: PermissionOverlayUi,

    /// Control buttons
    request_all_button: (Button, ButtonState),
    reset_all_button: (Button, ButtonState),
    deny_all_button: (Button, ButtonState),
    authorize_all_button: (Button, ButtonState),

    /// Test scenarios
    test_scenarios: Vec<(String, Vec<(PermissionDomain, PermissionStatus)>)>,
    current_scenario: usize,

    /// Stats
    total_requests: u32,
    last_update_ms: u64,

    /// Show overlay toggle
    show_overlay: bool,
    show_overlay_toggle: (Toggle, ToggleState),
}

impl Default for PermissionsScene {
    fn default() -> Self {
        // Create cards for all 8 domains
        let domains = vec![
            PermissionDomain::Camera,
            PermissionDomain::Microphone,
            PermissionDomain::Location,
            PermissionDomain::Bluetooth,
            PermissionDomain::Motion,
            PermissionDomain::Notifications,
            PermissionDomain::Contacts,
            PermissionDomain::MediaLibrary,
        ];

        let cards: Vec<PermissionCard> = domains.into_iter().map(PermissionCard::new).collect();

        // Initialize permission states
        let permission_states = cards.iter().map(|card| card.state).collect();

        // Create control buttons
        let request_all_button = (
            Button { text: "Request All".into(), style: ButtonStyle::default() },
            ButtonState::default(),
        );

        let reset_all_button = (
            Button { text: "Reset All".into(), style: ButtonStyle::default() },
            ButtonState::default(),
        );

        let deny_all_button = (
            Button { text: "Deny All".into(), style: ButtonStyle::default() },
            ButtonState::default(),
        );

        let authorize_all_button = (
            Button { text: "Authorize All".into(), style: ButtonStyle::default() },
            ButtonState::default(),
        );

        // Define test scenarios
        let test_scenarios = vec![
            (
                "Camera + Mic Only".into(),
                vec![
                    (PermissionDomain::Camera, PermissionStatus::NotDetermined),
                    (PermissionDomain::Microphone, PermissionStatus::NotDetermined),
                ],
            ),
            (
                "Location + Bluetooth".into(),
                vec![
                    (PermissionDomain::Location, PermissionStatus::NotDetermined),
                    (PermissionDomain::Bluetooth, PermissionStatus::NotDetermined),
                ],
            ),
            (
                "Mixed States".into(),
                vec![
                    (PermissionDomain::Camera, PermissionStatus::Authorized),
                    (PermissionDomain::Microphone, PermissionStatus::Denied),
                    (PermissionDomain::Location, PermissionStatus::NotDetermined),
                    (PermissionDomain::Notifications, PermissionStatus::Limited),
                ],
            ),
            (
                "All Denied".into(),
                vec![
                    (PermissionDomain::Camera, PermissionStatus::Denied),
                    (PermissionDomain::Microphone, PermissionStatus::Denied),
                    (PermissionDomain::Location, PermissionStatus::Denied),
                    (PermissionDomain::Bluetooth, PermissionStatus::Denied),
                    (PermissionDomain::Motion, PermissionStatus::Denied),
                    (PermissionDomain::Notifications, PermissionStatus::Denied),
                    (PermissionDomain::Contacts, PermissionStatus::Denied),
                    (PermissionDomain::MediaLibrary, PermissionStatus::Denied),
                ],
            ),
        ];

        let show_overlay_toggle =
            (Toggle { style: ToggleStyle::default() }, ToggleState::default());

        Self {
            cards,
            permission_states,
            overlay_ui: PermissionOverlayUi::default(),
            request_all_button,
            reset_all_button,
            deny_all_button,
            authorize_all_button,
            test_scenarios,
            current_scenario: 0,
            total_requests: 0,
            last_update_ms: 0,
            show_overlay: false,
            show_overlay_toggle,
        }
    }
}

impl PermissionsScene {
    pub fn update(&mut self, dt_ms: u32) {
        // Update toggle animations
        for card in &mut self.cards {
            card.toggle_state.step(dt_ms);
        }
        self.show_overlay_toggle.1.step(dt_ms);

        // Sync permission states
        self.permission_states = self.cards.iter().map(|card| card.state).collect();

        // Update overlay UI with current states
        if self.show_overlay {
            self.overlay_ui.update(&self.permission_states);
        }

        self.last_update_ms = timing::now_ms();
    }

    pub fn input_pointer(&mut self, x: f32, y: f32, _dx: f32, _dy: f32, buttons: u32) {
        // Permission cards (2x4 grid)
        for (i, card) in self.cards.iter_mut().enumerate() {
            let col = i % 2;
            let row = i / 2;

            // Request button
            let button_rect =
                gfx::RectF::new(50.0 + col as f32 * 300.0, 100.0 + row as f32 * 90.0, 120.0, 35.0);

            if point_in_rect([x, y], button_rect) {
                if buttons & 1 != 0 {
                    card.button_state.on_pointer_down();
                } else if card.button_state.on_pointer_up() {
                    // Request permission
                    card.request_count += 1;
                    card.last_request_ms = timing::now_ms();
                    self.total_requests += 1;
                }
            } else if buttons == 0 {
                card.button_state.on_pointer_cancel();
            }

            // Toggle for authorized state
            let toggle_rect =
                gfx::RectF::new(180.0 + col as f32 * 300.0, 105.0 + row as f32 * 90.0, 50.0, 25.0);

            if buttons & 1 != 0 && point_in_rect([x, y], toggle_rect) {
                card.toggle_state.set_on(!card.toggle_state.on);
                card.simulate_status_change();
            }
        }

        // Control buttons
        let mut action_to_handle = None;

        let control_buttons = [
            (&mut self.request_all_button, 650.0, 100.0, "request_all"),
            (&mut self.reset_all_button, 650.0, 150.0, "reset_all"),
            (&mut self.deny_all_button, 650.0, 200.0, "deny_all"),
            (&mut self.authorize_all_button, 650.0, 250.0, "authorize_all"),
        ];

        for (button_pair, btn_x, btn_y, action) in control_buttons {
            let rect = gfx::RectF::new(btn_x, btn_y, 120.0, 35.0);

            if point_in_rect([x, y], rect) {
                if buttons & 1 != 0 {
                    button_pair.1.on_pointer_down();
                } else if button_pair.1.on_pointer_up() {
                    action_to_handle = Some(action);
                }
            } else if buttons == 0 {
                button_pair.1.on_pointer_cancel();
            }
        }

        if let Some(action) = action_to_handle {
            self.handle_control_action(action);
        }

        // Scenario buttons
        let mut scenario_to_apply = None;
        for (i, (_name, _)) in self.test_scenarios.iter().enumerate() {
            let rect = gfx::RectF::new(650.0, 320.0 + i as f32 * 30.0, 150.0, 25.0);

            if buttons & 1 != 0 && point_in_rect([x, y], rect) {
                scenario_to_apply = Some(i);
            }
        }

        if let Some(idx) = scenario_to_apply {
            self.apply_scenario(idx);
        }

        // Show overlay toggle
        let overlay_toggle_rect = gfx::RectF::new(650.0, 480.0, 50.0, 25.0);
        if buttons & 1 != 0 && point_in_rect([x, y], overlay_toggle_rect) {
            self.show_overlay_toggle.1.set_on(!self.show_overlay_toggle.1.on);
            self.show_overlay = self.show_overlay_toggle.1.on;
        }
    }

    fn handle_control_action(&mut self, action: &str) {
        match action {
            "request_all" => {
                for card in &mut self.cards {
                    card.request_count += 1;
                    card.last_request_ms = timing::now_ms();
                }
                self.total_requests += self.cards.len() as u32;
            }
            "reset_all" => {
                for card in &mut self.cards {
                    card.state.status = PermissionStatus::NotDetermined;
                    card.state.last_changed_ms = timing::now_ms();
                    card.toggle_state.set_on(false);
                }
            }
            "deny_all" => {
                for card in &mut self.cards {
                    card.state.status = PermissionStatus::Denied;
                    card.state.last_changed_ms = timing::now_ms();
                    card.toggle_state.set_on(false);
                }
            }
            "authorize_all" => {
                for card in &mut self.cards {
                    card.state.status = PermissionStatus::Authorized;
                    card.state.last_changed_ms = timing::now_ms();
                    card.toggle_state.set_on(true);
                }
            }
            _ => {}
        }
    }

    fn apply_scenario(&mut self, index: usize) {
        if index >= self.test_scenarios.len() {
            return;
        }

        self.current_scenario = index;

        // Reset all to NotDetermined first
        for card in &mut self.cards {
            card.state.status = PermissionStatus::NotDetermined;
            card.toggle_state.set_on(false);
        }

        // Apply scenario
        let scenario = &self.test_scenarios[index];
        for (domain, status) in &scenario.1 {
            if let Some(card) = self.cards.iter_mut().find(|c| c.domain == *domain) {
                card.state.status = *status;
                card.toggle_state.set_on(*status == PermissionStatus::Authorized);
                card.state.last_changed_ms = timing::now_ms();
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
        builder.rrect(viewport, [0.0; 4], gfx::Color::rgba(0.97, 0.97, 0.98, 1.0));

        // Title
        let title = Label {
            text: "Permissions UI Test - All 8 Domains".into(),
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

        // Draw permission cards (2x4 grid)
        for (i, card) in self.cards.iter_mut().enumerate() {
            let col = i % 2;
            let row = i / 2;
            let x = viewport.x + 50.0 + col as f32 * 300.0;
            let y = viewport.y + 80.0 + row as f32 * 90.0;

            // Card background with border
            let card_rect = gfx::RectF::new(x - 10.0, y - 5.0, 280.0, 75.0);
            // Draw border first (larger rect)
            builder.rrect(card_rect, [8.0; 4], card.get_status_color());
            // Draw inner white background (smaller rect)
            let inner_rect = gfx::RectF::new(x - 8.0, y - 3.0, 276.0, 71.0);
            builder.rrect(inner_rect, [7.0; 4], gfx::Color::rgba(1.0, 1.0, 1.0, 1.0));

            // Domain name
            let domain_label = Label {
                text: format!("{:?}", card.domain),
                color: gfx::Color::rgba(0.2, 0.2, 0.2, 1.0),
                align: Align::Left,
                wrap: false,
                font_id: 0,
                font_px: 14.0,
            };
            domain_label.encode(
                gfx::RectF::new(x, y, 200.0, 20.0),
                device_scale,
                text,
                uploader,
                builder,
            );

            // Status text
            let status_label = Label {
                text: card.get_status_text().into(),
                color: card.get_status_color(),
                align: Align::Left,
                wrap: false,
                font_id: 0,
                font_px: 11.0,
            };
            status_label.encode(
                gfx::RectF::new(x, y + 45.0, 150.0, 15.0),
                device_scale,
                text,
                uploader,
                builder,
            );

            // Request button
            card.button.encode(
                gfx::RectF::new(x, y + 20.0, 120.0, 35.0),
                device_scale,
                text,
                uploader,
                &card.button_state,
                builder,
            );

            // Toggle
            card.toggle.encode(
                gfx::RectF::new(x + 130.0, y + 25.0, 50.0, 25.0),
                &card.toggle_state,
                builder,
            );

            // Request count
            if card.request_count > 0 {
                let count_label = Label {
                    text: format!("Reqs: {}", card.request_count),
                    color: gfx::Color::rgba(0.5, 0.5, 0.5, 1.0),
                    align: Align::Right,
                    wrap: false,
                    font_id: 0,
                    font_px: 10.0,
                };
                count_label.encode(
                    gfx::RectF::new(x + 190.0, y + 25.0, 70.0, 15.0),
                    device_scale,
                    text,
                    uploader,
                    builder,
                );
            }
        }

        // Control buttons section
        let controls_label = Label {
            text: "Controls:".into(),
            color: gfx::Color::rgba(0.3, 0.3, 0.3, 1.0),
            align: Align::Left,
            wrap: false,
            font_id: 0,
            font_px: 14.0,
        };
        controls_label.encode(
            gfx::RectF::new(viewport.x + 650.0, viewport.y + 70.0, 100.0, 20.0),
            device_scale,
            text,
            uploader,
            builder,
        );

        // Draw control buttons
        self.request_all_button.0.encode(
            gfx::RectF::new(viewport.x + 650.0, viewport.y + 100.0, 120.0, 35.0),
            device_scale,
            text,
            uploader,
            &self.request_all_button.1,
            builder,
        );

        self.reset_all_button.0.encode(
            gfx::RectF::new(viewport.x + 650.0, viewport.y + 150.0, 120.0, 35.0),
            device_scale,
            text,
            uploader,
            &self.reset_all_button.1,
            builder,
        );

        self.deny_all_button.0.encode(
            gfx::RectF::new(viewport.x + 650.0, viewport.y + 200.0, 120.0, 35.0),
            device_scale,
            text,
            uploader,
            &self.deny_all_button.1,
            builder,
        );

        self.authorize_all_button.0.encode(
            gfx::RectF::new(viewport.x + 650.0, viewport.y + 250.0, 120.0, 35.0),
            device_scale,
            text,
            uploader,
            &self.authorize_all_button.1,
            builder,
        );

        // Test scenarios section
        let scenarios_label = Label {
            text: "Test Scenarios:".into(),
            color: gfx::Color::rgba(0.3, 0.3, 0.3, 1.0),
            align: Align::Left,
            wrap: false,
            font_id: 0,
            font_px: 14.0,
        };
        scenarios_label.encode(
            gfx::RectF::new(viewport.x + 650.0, viewport.y + 290.0, 150.0, 20.0),
            device_scale,
            text,
            uploader,
            builder,
        );

        // Draw scenario buttons
        for (i, (name, _)) in self.test_scenarios.iter().enumerate() {
            let is_selected = i == self.current_scenario;
            let rect = gfx::RectF::new(
                viewport.x + 650.0,
                viewport.y + 320.0 + i as f32 * 30.0,
                150.0,
                25.0,
            );

            builder.rrect(
                rect,
                [4.0; 4],
                if is_selected {
                    gfx::Color::rgba(0.3, 0.6, 0.9, 1.0)
                } else {
                    gfx::Color::rgba(0.9, 0.9, 0.9, 1.0)
                },
            );

            let scenario_label = Label {
                text: name.clone(),
                color: if is_selected {
                    gfx::Color::rgba(1.0, 1.0, 1.0, 1.0)
                } else {
                    gfx::Color::rgba(0.2, 0.2, 0.2, 1.0)
                },
                align: Align::Center,
                wrap: false,
                font_id: 0,
                font_px: 11.0,
            };
            scenario_label.encode(rect, device_scale, text, uploader, builder);
        }

        // Show overlay toggle
        let overlay_label = Label {
            text: "Show Overlay:".into(),
            color: gfx::Color::rgba(0.3, 0.3, 0.3, 1.0),
            align: Align::Left,
            wrap: false,
            font_id: 0,
            font_px: 12.0,
        };
        overlay_label.encode(
            gfx::RectF::new(viewport.x + 650.0, viewport.y + 455.0, 100.0, 20.0),
            device_scale,
            text,
            uploader,
            builder,
        );

        self.show_overlay_toggle.0.encode(
            gfx::RectF::new(viewport.x + 650.0, viewport.y + 480.0, 50.0, 25.0),
            &self.show_overlay_toggle.1,
            builder,
        );

        // Draw overlay if enabled
        if self.show_overlay && self.overlay_ui.is_visible() {
            // Darken background
            builder.rrect(viewport, [0.0; 4], gfx::Color::rgba(0.0, 0.0, 0.0, 0.5));

            // Draw permission overlay
            self.overlay_ui.draw(viewport, device_scale, text, uploader, builder);
        }

        // Stats footer
        let stats = Label {
            text: format!(
                "Total Requests: {} | States: A:{} D:{} N:{} L:{} | Overlay: {}",
                self.total_requests,
                self.cards
                    .iter()
                    .filter(|c| c.state.status == PermissionStatus::Authorized)
                    .count(),
                self.cards.iter().filter(|c| c.state.status == PermissionStatus::Denied).count(),
                self.cards
                    .iter()
                    .filter(|c| c.state.status == PermissionStatus::NotDetermined)
                    .count(),
                self.cards.iter().filter(|c| c.state.status == PermissionStatus::Limited).count(),
                if self.show_overlay && self.overlay_ui.is_visible() {
                    "Visible"
                } else {
                    "Hidden"
                }
            ),
            color: gfx::Color::rgba(0.4, 0.4, 0.4, 1.0),
            align: Align::Center,
            wrap: false,
            font_id: 0,
            font_px: 11.0,
        };
        stats.encode(
            gfx::RectF::new(viewport.x, viewport.y + viewport.h - 25.0, viewport.w, 20.0),
            device_scale,
            text,
            uploader,
            builder,
        );
    }
}

fn point_in_rect(point: [f32; 2], rect: gfx::RectF) -> bool {
    point[0] >= rect.x
        && point[0] <= rect.x + rect.w
        && point[1] >= rect.y
        && point[1] <= rect.y + rect.h
}
