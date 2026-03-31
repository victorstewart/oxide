//! Integration Test Scene - Real-world workflows combining multiple UI systems
//!
//! This scene validates complex interactions between different UI components
//! in realistic application scenarios.

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
        Align, Badge, BadgeState, BadgeStyle, Button, ButtonState, ButtonStyle, ImageUploader,
        Label, ProgressBar, RecordButton, RecordButtonState, RecordButtonStyle, TextCtx,
    },
    orchestration::ScatterOrchestrator,
    permissions::PermissionOverlayUi,
    DrawListBuilder,
};

const LEGACY_BADGE_IMAGE: gfx::ImageHandle = gfx::ImageHandle(1);

/// Workflow states for the integration demo
#[derive(Clone, Copy, Debug, PartialEq)]
enum WorkflowState {
    Idle,
    RequestingPermissions,
    RecordingMedia,
    ProcessingData,
    ShowingResults,
}

/// Media capture workflow component
struct MediaCaptureWorkflow {
    state: WorkflowState,

    // Permission states
    camera_permission: PermissionState,
    microphone_permission: PermissionState,

    // Recording controls
    record_button: RecordButton,
    record_state: RecordButtonState,
    recording_start_ms: u64,

    // Progress tracking
    progress: f32,
    progress_bar: ProgressBar,

    // Results
    captured_items: Vec<String>,

    // UI elements
    status_label: String,
    action_button: Button,
    action_button_state: ButtonState,
}

impl MediaCaptureWorkflow {
    fn new() -> Self {
        Self {
            state: WorkflowState::Idle,
            camera_permission: PermissionState {
                domain: PermissionDomain::Camera,
                status: PermissionStatus::NotDetermined,
                last_changed_ms: 0,
            },
            microphone_permission: PermissionState {
                domain: PermissionDomain::Microphone,
                status: PermissionStatus::NotDetermined,
                last_changed_ms: 0,
            },
            record_button: RecordButton {
                style: RecordButtonStyle {
                    recording_timeout_ms: 30000,
                    ..RecordButtonStyle::default()
                },
            },
            record_state: RecordButtonState::default(),
            recording_start_ms: 0,
            progress: 0.0,
            progress_bar: ProgressBar {
                value: Some(0.0),
                track: gfx::Color::rgba(0.9, 0.9, 0.9, 1.0),
                fill: gfx::Color::rgba(0.2, 0.6, 0.9, 1.0),
                corner: 4.0,
            },
            captured_items: Vec::new(),
            status_label: "Ready to capture".into(),
            action_button: Button { text: "Start Workflow".into(), style: ButtonStyle::default() },
            action_button_state: ButtonState::default(),
        }
    }

    fn update(&mut self, dt_ms: u32) {
        match self.state {
            WorkflowState::RequestingPermissions => {
                // Simulate permission request completion
                if timing::now_ms() - self.camera_permission.last_changed_ms > 2000 {
                    self.camera_permission.status = PermissionStatus::Authorized;
                    self.microphone_permission.status = PermissionStatus::Authorized;
                    self.state = WorkflowState::Idle;
                    self.status_label = "Permissions granted - Ready to record".into();
                    self.action_button.text = "Start Recording".into();
                }
            }
            WorkflowState::RecordingMedia => {
                if self.record_state.is_recording() {
                    let elapsed = timing::now_ms() - self.recording_start_ms;
                    self.progress = (elapsed as f32 / 30000.0).min(1.0);
                    self.progress_bar.value = Some(self.progress);

                    if elapsed >= 30000 {
                        self.stop_recording();
                    }
                }
            }
            WorkflowState::ProcessingData => {
                self.progress = (self.progress + dt_ms as f32 / 3000.0).min(1.0);
                self.progress_bar.value = Some(self.progress);

                if self.progress >= 1.0 {
                    self.state = WorkflowState::ShowingResults;
                    self.captured_items
                        .push(format!("Recording_{}", self.captured_items.len() + 1));
                    self.status_label = format!(
                        "Processing complete - {} items captured",
                        self.captured_items.len()
                    );
                    self.action_button.text = "Capture Another".into();
                }
            }
            _ => {}
        }
    }

    fn start_workflow(&mut self) {
        match self.state {
            WorkflowState::Idle => {
                if self.camera_permission.status != PermissionStatus::Authorized
                    || self.microphone_permission.status != PermissionStatus::Authorized
                {
                    self.state = WorkflowState::RequestingPermissions;
                    self.status_label = "Requesting permissions...".into();
                    self.camera_permission.last_changed_ms = timing::now_ms();
                    self.microphone_permission.last_changed_ms = timing::now_ms();
                } else {
                    self.start_recording();
                }
            }
            WorkflowState::ShowingResults => {
                self.state = WorkflowState::Idle;
                self.progress = 0.0;
                self.progress_bar.value = Some(0.0);
                self.start_recording();
            }
            _ => {}
        }
    }

    fn start_recording(&mut self) {
        self.state = WorkflowState::RecordingMedia;
        self.record_state.start_recording(&self.record_button.style);
        self.recording_start_ms = timing::now_ms();
        self.status_label = "Recording...".into();
        self.action_button.text = "Stop Recording".into();
    }

    fn stop_recording(&mut self) {
        self.record_state.stop_recording();
        self.state = WorkflowState::ProcessingData;
        self.progress = 0.0;
        self.status_label = "Processing recording...".into();
        self.action_button.text = "Processing...".into();
    }
}

/// Data collection workflow with form validation
struct DataCollectionWorkflow {
    // Simulated form data
    name_text: String,
    email_text: String,
    notes_text: String,

    // Validation states
    name_valid: bool,
    email_valid: bool,

    // Submission
    submit_button: Button,
    submit_state: ButtonState,
    submission_count: u32,

    // Collection of submissions
    submissions: Vec<(String, String, String)>,

    // Status
    status_message: String,
    show_success_badge: bool,
    success_badge: Badge,
    success_badge_state: BadgeState,
}

impl DataCollectionWorkflow {
    fn new() -> Self {
        Self {
            name_text: String::new(),
            email_text: String::new(),
            notes_text: String::new(),

            name_valid: false,
            email_valid: false,

            submit_button: Button { text: "Submit".into(), style: ButtonStyle::default() },
            submit_state: ButtonState::default(),
            submission_count: 0,

            submissions: Vec::new(),
            status_message: "Fill out the form".into(),
            show_success_badge: false,
            success_badge: Badge { image: LEGACY_BADGE_IMAGE, style: BadgeStyle::default() },
            success_badge_state: BadgeState::default(),
        }
    }

    fn validate(&mut self) {
        self.name_valid = !self.name_text.trim().is_empty();
        self.email_valid = self.email_text.contains('@') && self.email_text.contains('.');

        if !self.name_valid {
            self.status_message = "Name is required".into();
        } else if !self.email_valid {
            self.status_message = "Valid email is required".into();
        } else {
            self.status_message = "Ready to submit".into();
        }
    }

    fn submit(&mut self) {
        if self.name_valid && self.email_valid {
            self.submissions.push((
                self.name_text.clone(),
                self.email_text.clone(),
                self.notes_text.clone(),
            ));

            self.submission_count += 1;
            self.success_badge_state.bounce(&self.success_badge.style);
            self.show_success_badge = true;

            // Clear form
            self.name_text.clear();
            self.email_text.clear();
            self.notes_text.clear();

            self.status_message = format!("Submission #{} successful!", self.submission_count);

            // Reset validation
            self.name_valid = false;
            self.email_valid = false;
        }
    }

    fn update(&mut self, _dt_ms: u32) {
        // Badge animation is handled internally
        // Hide success badge after 3 seconds
        if self.show_success_badge {
            // Simple timer logic instead
            self.show_success_badge = self.submission_count == 0;
        }
    }
}

/// Multi-step onboarding workflow
struct OnboardingWorkflow {
    current_step: usize,
    total_steps: usize,
    steps_completed: Vec<bool>,

    // Navigation
    next_button: Button,
    next_button_state: ButtonState,
    prev_button: Button,
    prev_button_state: ButtonState,

    // Progress
    progress: f32,

    // Orchestration for transitions
    orchestrator: ScatterOrchestrator,
    animating: bool,

    // Content for each step
    step_titles: Vec<String>,
    step_descriptions: Vec<String>,

    // Completion
    completed: bool,
    completion_time_ms: u64,
}

impl OnboardingWorkflow {
    fn new() -> Self {
        let step_titles = vec![
            "Welcome".into(),
            "Setup Permissions".into(),
            "Configure Profile".into(),
            "Choose Preferences".into(),
            "Get Started".into(),
        ];

        let step_descriptions = vec![
            "Welcome to the integration demo. This workflow will guide you through the setup process.".into(),
            "Grant necessary permissions for the best experience.".into(),
            "Set up your profile information.".into(),
            "Customize your preferences and settings.".into(),
            "You're all set! Start using the application.".into(),
        ];

        Self {
            current_step: 0,
            total_steps: 5,
            steps_completed: vec![false; 5],

            next_button: Button { text: "Next".into(), style: ButtonStyle::default() },
            next_button_state: ButtonState::default(),

            prev_button: Button { text: "Previous".into(), style: ButtonStyle::default() },
            prev_button_state: ButtonState::default(),

            progress: 0.0,

            orchestrator: ScatterOrchestrator::new(300),
            animating: false,

            step_titles,
            step_descriptions,

            completed: false,
            completion_time_ms: 0,
        }
    }

    fn next_step(&mut self) {
        if self.current_step < self.total_steps - 1 && !self.animating {
            self.steps_completed[self.current_step] = true;
            self.current_step += 1;
            self.progress = (self.current_step as f32) / (self.total_steps as f32 - 1.0);

            self.orchestrator.begin_transition();
            self.animating = true;

            if self.current_step == self.total_steps - 1 {
                self.next_button.text = "Complete".into();
            }
        } else if self.current_step == self.total_steps - 1 && !self.completed {
            self.steps_completed[self.current_step] = true;
            self.completed = true;
            self.completion_time_ms = timing::now_ms();
        }
    }

    fn prev_step(&mut self) {
        if self.current_step > 0 && !self.animating {
            self.current_step -= 1;
            self.progress = (self.current_step as f32) / (self.total_steps as f32 - 1.0);

            self.orchestrator.begin_transition();
            self.animating = true;

            if self.next_button.text == "Complete" {
                self.next_button.text = "Next".into();
            }
        }
    }

    fn update(&mut self, _dt_ms: u32) {
        if self.animating && !self.orchestrator.is_animating() {
            self.animating = false;
            self.orchestrator.end_transition();
        }
    }
}

/// Main integration test scene
pub struct IntegrationScene {
    // Workflows
    media_workflow: MediaCaptureWorkflow,
    data_workflow: DataCollectionWorkflow,
    onboarding_workflow: OnboardingWorkflow,

    // Active workflow
    active_workflow: usize,
    workflow_names: Vec<String>,

    // Workflow selector buttons
    workflow_buttons: Vec<(Button, ButtonState)>,

    // Global state
    time_ms: u64,

    // Permission overlay
    permission_overlay: PermissionOverlayUi,
    permission_states: Vec<PermissionState>,
}

impl Default for IntegrationScene {
    fn default() -> Self {
        let workflow_names: Vec<String> =
            vec!["Media Capture".into(), "Data Collection".into(), "Onboarding Flow".into()];

        let mut workflow_buttons = Vec::new();
        for name in &workflow_names {
            workflow_buttons.push((
                Button { text: name.clone(), style: ButtonStyle::default() },
                ButtonState::default(),
            ));
        }

        Self {
            media_workflow: MediaCaptureWorkflow::new(),
            data_workflow: DataCollectionWorkflow::new(),
            onboarding_workflow: OnboardingWorkflow::new(),

            active_workflow: 0,
            workflow_names,
            workflow_buttons,

            time_ms: 0,

            permission_overlay: PermissionOverlayUi::default(),
            permission_states: Vec::new(),
        }
    }
}

struct TextFieldView<'a> {
    rect: gfx::RectF,
    label: &'a str,
    text: &'a str,
    placeholder: Option<&'a str>,
    is_valid: bool,
}

impl IntegrationScene {
    pub fn update(&mut self, dt_ms: u32) {
        self.time_ms += dt_ms as u64;

        // Update active workflow
        match self.active_workflow {
            0 => self.media_workflow.update(dt_ms),
            1 => self.data_workflow.update(dt_ms),
            2 => self.onboarding_workflow.update(dt_ms),
            _ => {}
        }

        // Update permission states for overlay
        self.permission_states =
            vec![self.media_workflow.camera_permission, self.media_workflow.microphone_permission];
        self.permission_overlay.update(&self.permission_states);
    }

    pub fn input_pointer(&mut self, x: f32, y: f32, _dx: f32, _dy: f32, buttons: u32) {
        // Workflow selector buttons
        for (i, (_button, state)) in self.workflow_buttons.iter_mut().enumerate() {
            let rect = gfx::RectF::new(50.0 + i as f32 * 180.0, 50.0, 170.0, 40.0);

            if point_in_rect([x, y], rect) {
                if buttons & 1 != 0 {
                    state.on_pointer_down();
                } else if state.on_pointer_up() {
                    self.active_workflow = i;
                }
            } else if buttons == 0 {
                state.on_pointer_cancel();
            }
        }

        // Route input to active workflow
        match self.active_workflow {
            0 => self.handle_media_workflow_input(x, y, buttons),
            1 => self.handle_data_workflow_input(x, y, buttons),
            2 => self.handle_onboarding_workflow_input(x, y, buttons),
            _ => {}
        }
    }

    fn handle_media_workflow_input(&mut self, x: f32, y: f32, buttons: u32) {
        let workflow = &mut self.media_workflow;

        // Action button
        let action_rect = gfx::RectF::new(100.0, 200.0, 150.0, 40.0);
        if point_in_rect([x, y], action_rect) {
            if buttons & 1 != 0 {
                workflow.action_button_state.on_pointer_down();
            } else if workflow.action_button_state.on_pointer_up() {
                if workflow.state == WorkflowState::RecordingMedia {
                    workflow.stop_recording();
                } else {
                    workflow.start_workflow();
                }
            }
        } else if buttons == 0 {
            workflow.action_button_state.on_pointer_cancel();
        }

        // Record button (if visible)
        if workflow.state == WorkflowState::RecordingMedia {
            let record_rect = gfx::RectF::new(300.0, 180.0, 80.0, 80.0);
            if point_in_rect([x, y], record_rect) {
                if buttons & 1 != 0 {
                    workflow.record_state.on_pointer_down(&workflow.record_button.style);
                } else if workflow.record_state.on_pointer_up(&workflow.record_button.style) {
                    workflow.stop_recording();
                }
            }
        }
    }

    fn handle_data_workflow_input(&mut self, x: f32, y: f32, buttons: u32) {
        let workflow = &mut self.data_workflow;

        // Text field interactions would go here
        // For demo, just handle submit button
        let submit_rect = gfx::RectF::new(100.0, 400.0, 120.0, 40.0);
        if point_in_rect([x, y], submit_rect) {
            if buttons & 1 != 0 {
                workflow.submit_state.on_pointer_down();
            } else if workflow.submit_state.on_pointer_up() {
                workflow.validate();
                workflow.submit();
            }
        } else if buttons == 0 {
            workflow.submit_state.on_pointer_cancel();
        }

        // Simulate text input for demo
        if buttons & 1 != 0 {
            let name_rect = gfx::RectF::new(100.0, 200.0, 300.0, 40.0);
            let email_rect = gfx::RectF::new(100.0, 260.0, 300.0, 40.0);

            if point_in_rect([x, y], name_rect) && workflow.name_text.is_empty() {
                workflow.name_text = "John Doe".into();
                workflow.validate();
            } else if point_in_rect([x, y], email_rect) && workflow.email_text.is_empty() {
                workflow.email_text = "john@example.com".into();
                workflow.validate();
            }
        }
    }

    fn handle_onboarding_workflow_input(&mut self, x: f32, y: f32, buttons: u32) {
        let workflow = &mut self.onboarding_workflow;

        // Next button
        let next_rect = gfx::RectF::new(400.0, 400.0, 100.0, 40.0);
        if point_in_rect([x, y], next_rect) {
            if buttons & 1 != 0 {
                workflow.next_button_state.on_pointer_down();
            } else if workflow.next_button_state.on_pointer_up() {
                workflow.next_step();
            }
        } else if buttons == 0 {
            workflow.next_button_state.on_pointer_cancel();
        }

        // Previous button
        let prev_rect = gfx::RectF::new(100.0, 400.0, 100.0, 40.0);
        if point_in_rect([x, y], prev_rect) && workflow.current_step > 0 {
            if buttons & 1 != 0 {
                workflow.prev_button_state.on_pointer_down();
            } else if workflow.prev_button_state.on_pointer_up() {
                workflow.prev_step();
            }
        } else if buttons == 0 {
            workflow.prev_button_state.on_pointer_cancel();
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
        builder.rrect(viewport, [0.0; 4], gfx::Color::rgba(0.95, 0.95, 0.96, 1.0));

        // Title
        let title = Label {
            text: "Integration Test - Real-World Workflows".into(),
            color: gfx::Color::rgba(0.1, 0.1, 0.1, 1.0),
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

        // Workflow selector tabs
        for (i, ((button, state), _name)) in
            self.workflow_buttons.iter_mut().zip(self.workflow_names.iter()).enumerate()
        {
            let is_active = i == self.active_workflow;
            let rect = gfx::RectF::new(
                viewport.x + 50.0 + i as f32 * 180.0,
                viewport.y + 50.0,
                170.0,
                40.0,
            );

            // Highlight active tab
            if is_active {
                let highlight_rect =
                    gfx::RectF::new(rect.x - 5.0, rect.y - 5.0, rect.w + 10.0, rect.h + 10.0);
                builder.rrect(highlight_rect, [8.0; 4], gfx::Color::rgba(0.2, 0.5, 0.9, 0.2));
            }

            button.encode(rect, device_scale, text, uploader, state, builder);
        }

        // Draw active workflow
        match self.active_workflow {
            0 => self.draw_media_workflow(viewport, device_scale, text, uploader, builder),
            1 => self.draw_data_workflow(viewport, device_scale, text, uploader, builder),
            2 => self.draw_onboarding_workflow(viewport, device_scale, text, uploader, builder),
            _ => {}
        }

        // Draw permission overlay if needed
        if self.permission_overlay.is_visible() {
            builder.rrect(viewport, [0.0; 4], gfx::Color::rgba(0.0, 0.0, 0.0, 0.4));
            self.permission_overlay.draw(viewport, device_scale, text, uploader, builder);
        }
    }

    fn draw_media_workflow<U: ImageUploader>(
        &mut self,
        viewport: gfx::RectF,
        device_scale: f32,
        text: &mut TextCtx,
        uploader: &mut U,
        builder: &mut DrawListBuilder,
    ) {
        // Extract permission statuses before mutable borrow
        let camera_status = self.media_workflow.camera_permission.status;
        let microphone_status = self.media_workflow.microphone_permission.status;

        let workflow = &mut self.media_workflow;

        // Workflow title
        let title = Label {
            text: "Media Capture Workflow".into(),
            color: gfx::Color::rgba(0.2, 0.2, 0.2, 1.0),
            align: Align::Left,
            wrap: false,
            font_id: 0,
            font_px: 18.0,
        };
        title.encode(
            gfx::RectF::new(viewport.x + 100.0, viewport.y + 120.0, 300.0, 25.0),
            device_scale,
            text,
            uploader,
            builder,
        );

        // Status
        let status = Label {
            text: workflow.status_label.clone(),
            color: gfx::Color::rgba(0.4, 0.4, 0.4, 1.0),
            align: Align::Left,
            wrap: false,
            font_id: 0,
            font_px: 14.0,
        };
        status.encode(
            gfx::RectF::new(viewport.x + 100.0, viewport.y + 150.0, 400.0, 20.0),
            device_scale,
            text,
            uploader,
            builder,
        );

        // Action button
        workflow.action_button.encode(
            gfx::RectF::new(viewport.x + 100.0, viewport.y + 200.0, 150.0, 40.0),
            device_scale,
            text,
            uploader,
            &workflow.action_button_state,
            builder,
        );

        // Record button (if recording)
        if workflow.state == WorkflowState::RecordingMedia {
            workflow.record_button.encode(
                gfx::RectF::new(viewport.x + 300.0, viewport.y + 180.0, 80.0, 80.0),
                &workflow.record_state,
                builder,
            );
        }

        // Progress bar
        if workflow.state == WorkflowState::RecordingMedia
            || workflow.state == WorkflowState::ProcessingData
        {
            workflow.progress_bar.encode(
                gfx::RectF::new(viewport.x + 100.0, viewport.y + 260.0, 300.0, 20.0),
                0.0, // phase parameter
                builder,
            );
        }

        // Captured items list
        if !workflow.captured_items.is_empty() {
            let list_title = Label {
                text: "Captured Items:".into(),
                color: gfx::Color::rgba(0.3, 0.3, 0.3, 1.0),
                align: Align::Left,
                wrap: false,
                font_id: 0,
                font_px: 14.0,
            };
            list_title.encode(
                gfx::RectF::new(viewport.x + 100.0, viewport.y + 320.0, 200.0, 20.0),
                device_scale,
                text,
                uploader,
                builder,
            );

            for (i, item) in workflow.captured_items.iter().enumerate() {
                let item_label = Label {
                    text: format!("• {}", item),
                    color: gfx::Color::rgba(0.2, 0.6, 0.3, 1.0),
                    align: Align::Left,
                    wrap: false,
                    font_id: 0,
                    font_px: 12.0,
                };
                item_label.encode(
                    gfx::RectF::new(
                        viewport.x + 120.0,
                        viewport.y + 345.0 + i as f32 * 20.0,
                        200.0,
                        18.0,
                    ),
                    device_scale,
                    text,
                    uploader,
                    builder,
                );
            }
        }

        // Permission status indicators
        self.draw_permission_indicator(
            viewport.x + 500.0,
            viewport.y + 200.0,
            "Camera",
            camera_status,
            builder,
        );

        self.draw_permission_indicator(
            viewport.x + 500.0,
            viewport.y + 230.0,
            "Microphone",
            microphone_status,
            builder,
        );
    }

    fn draw_data_workflow<U: ImageUploader>(
        &mut self,
        viewport: gfx::RectF,
        device_scale: f32,
        text: &mut TextCtx,
        uploader: &mut U,
        builder: &mut DrawListBuilder,
    ) {
        // Extract text values before mutable borrow
        let name_text = self.data_workflow.name_text.clone();
        let email_text = self.data_workflow.email_text.clone();
        let notes_text = self.data_workflow.notes_text.clone();
        let name_valid = self.data_workflow.name_valid;
        let email_valid = self.data_workflow.email_valid;

        let workflow = &mut self.data_workflow;

        // Workflow title
        let title = Label {
            text: "Data Collection Form".into(),
            color: gfx::Color::rgba(0.2, 0.2, 0.2, 1.0),
            align: Align::Left,
            wrap: false,
            font_id: 0,
            font_px: 18.0,
        };
        title.encode(
            gfx::RectF::new(viewport.x + 100.0, viewport.y + 120.0, 300.0, 25.0),
            device_scale,
            text,
            uploader,
            builder,
        );

        // Form fields
        Self::draw_text_field(
            TextFieldView {
                rect: gfx::RectF::new(viewport.x + 100.0, viewport.y + 200.0, 300.0, 40.0),
                label: "Name:",
                text: &name_text,
                placeholder: Some("Enter name..."),
                is_valid: name_valid,
            },
            text,
            device_scale,
            uploader,
            builder,
        );

        Self::draw_text_field(
            TextFieldView {
                rect: gfx::RectF::new(viewport.x + 100.0, viewport.y + 260.0, 300.0, 40.0),
                label: "Email:",
                text: &email_text,
                placeholder: Some("Enter email..."),
                is_valid: email_valid,
            },
            text,
            device_scale,
            uploader,
            builder,
        );

        Self::draw_text_field(
            TextFieldView {
                rect: gfx::RectF::new(viewport.x + 100.0, viewport.y + 320.0, 300.0, 60.0),
                label: "Notes:",
                text: &notes_text,
                placeholder: Some("Add notes (optional)..."),
                is_valid: true,
            },
            text,
            device_scale,
            uploader,
            builder,
        );

        // Submit button
        workflow.submit_button.encode(
            gfx::RectF::new(viewport.x + 100.0, viewport.y + 400.0, 120.0, 40.0),
            device_scale,
            text,
            uploader,
            &workflow.submit_state,
            builder,
        );

        // Status message
        let status = Label {
            text: workflow.status_message.clone(),
            color: if workflow.name_valid && workflow.email_valid {
                gfx::Color::rgba(0.2, 0.7, 0.3, 1.0)
            } else {
                gfx::Color::rgba(0.7, 0.3, 0.3, 1.0)
            },
            align: Align::Left,
            wrap: false,
            font_id: 0,
            font_px: 12.0,
        };
        status.encode(
            gfx::RectF::new(viewport.x + 240.0, viewport.y + 408.0, 200.0, 20.0),
            device_scale,
            text,
            uploader,
            builder,
        );

        // Success badge
        if workflow.show_success_badge {
            workflow.success_badge.encode(
                gfx::RectF::new(viewport.x + 350.0, viewport.y + 395.0, 50.0, 50.0),
                &workflow.success_badge_state,
                builder,
            );
        }

        // Submissions count
        if workflow.submission_count > 0 {
            let count_label = Label {
                text: format!("Total Submissions: {}", workflow.submission_count),
                color: gfx::Color::rgba(0.3, 0.3, 0.3, 1.0),
                align: Align::Left,
                wrap: false,
                font_id: 0,
                font_px: 14.0,
            };
            count_label.encode(
                gfx::RectF::new(viewport.x + 100.0, viewport.y + 460.0, 300.0, 20.0),
                device_scale,
                text,
                uploader,
                builder,
            );
        }
    }

    fn draw_onboarding_workflow<U: ImageUploader>(
        &mut self,
        viewport: gfx::RectF,
        device_scale: f32,
        text: &mut TextCtx,
        uploader: &mut U,
        builder: &mut DrawListBuilder,
    ) {
        let workflow = &mut self.onboarding_workflow;

        // Progress indicator
        let progress_width = 400.0;
        let progress_x = viewport.x + (viewport.w - progress_width) / 2.0;
        let progress_y = viewport.y + 120.0;

        // Progress bar background
        builder.rrect(
            gfx::RectF::new(progress_x, progress_y, progress_width, 8.0),
            [4.0; 4],
            gfx::Color::rgba(0.9, 0.9, 0.9, 1.0),
        );

        // Progress bar fill
        builder.rrect(
            gfx::RectF::new(progress_x, progress_y, progress_width * workflow.progress, 8.0),
            [4.0; 4],
            gfx::Color::rgba(0.2, 0.7, 0.4, 1.0),
        );

        // Step indicators
        for i in 0..workflow.total_steps {
            let step_x =
                progress_x + (i as f32 / (workflow.total_steps - 1) as f32) * progress_width;
            let is_completed = workflow.steps_completed[i];
            let is_current = i == workflow.current_step;

            let color = if is_completed {
                gfx::Color::rgba(0.2, 0.7, 0.4, 1.0)
            } else if is_current {
                gfx::Color::rgba(0.3, 0.5, 0.9, 1.0)
            } else {
                gfx::Color::rgba(0.8, 0.8, 0.8, 1.0)
            };

            let radius = if is_current { 12.0 } else { 10.0 };

            builder.rrect(
                gfx::RectF::new(
                    step_x - radius,
                    progress_y - radius + 4.0,
                    radius * 2.0,
                    radius * 2.0,
                ),
                [radius; 4],
                color,
            );

            // Step number
            let step_num = Label {
                text: format!("{}", i + 1),
                color: gfx::Color::rgba(1.0, 1.0, 1.0, 1.0),
                align: Align::Center,
                wrap: false,
                font_id: 0,
                font_px: 10.0,
            };
            step_num.encode(
                gfx::RectF::new(
                    step_x - radius,
                    progress_y - radius + 4.0,
                    radius * 2.0,
                    radius * 2.0,
                ),
                device_scale,
                text,
                uploader,
                builder,
            );
        }

        // Current step content
        let content_y = viewport.y + 180.0;

        // Step title
        let title = Label {
            text: workflow.step_titles[workflow.current_step].clone(),
            color: gfx::Color::rgba(0.1, 0.1, 0.1, 1.0),
            align: Align::Center,
            wrap: false,
            font_id: 0,
            font_px: 24.0,
        };
        title.encode(
            gfx::RectF::new(viewport.x, content_y, viewport.w, 35.0),
            device_scale,
            text,
            uploader,
            builder,
        );

        // Step description
        let desc = Label {
            text: workflow.step_descriptions[workflow.current_step].clone(),
            color: gfx::Color::rgba(0.4, 0.4, 0.4, 1.0),
            align: Align::Center,
            wrap: true,
            font_id: 0,
            font_px: 14.0,
        };
        desc.encode(
            gfx::RectF::new(viewport.x + 100.0, content_y + 50.0, viewport.w - 200.0, 100.0),
            device_scale,
            text,
            uploader,
            builder,
        );

        // Navigation buttons
        if workflow.current_step > 0 {
            workflow.prev_button.encode(
                gfx::RectF::new(viewport.x + 100.0, viewport.y + 400.0, 100.0, 40.0),
                device_scale,
                text,
                uploader,
                &workflow.prev_button_state,
                builder,
            );
        }

        if !workflow.completed {
            workflow.next_button.encode(
                gfx::RectF::new(viewport.x + 400.0, viewport.y + 400.0, 100.0, 40.0),
                device_scale,
                text,
                uploader,
                &workflow.next_button_state,
                builder,
            );
        }

        // Completion message
        if workflow.completed {
            let complete_msg = Label {
                text: "✓ Onboarding Complete!".into(),
                color: gfx::Color::rgba(0.2, 0.7, 0.3, 1.0),
                align: Align::Center,
                wrap: false,
                font_id: 0,
                font_px: 20.0,
            };
            complete_msg.encode(
                gfx::RectF::new(viewport.x, viewport.y + 400.0, viewport.w, 40.0),
                device_scale,
                text,
                uploader,
                builder,
            );
        }
    }

    fn draw_text_field<U: ImageUploader>(
        view: TextFieldView<'_>,
        text_ctx: &mut TextCtx,
        device_scale: f32,
        uploader: &mut U,
        builder: &mut DrawListBuilder,
    ) {
        // Label
        let field_label = Label {
            text: view.label.into(),
            color: gfx::Color::rgba(0.3, 0.3, 0.3, 1.0),
            align: Align::Left,
            wrap: false,
            font_id: 0,
            font_px: 12.0,
        };
        field_label.encode(
            gfx::RectF::new(view.rect.x, view.rect.y - 20.0, 100.0, 18.0),
            device_scale,
            text_ctx,
            uploader,
            builder,
        );

        // Field background
        let border_color = if view.is_valid {
            gfx::Color::rgba(0.7, 0.7, 0.7, 1.0)
        } else {
            gfx::Color::rgba(0.9, 0.3, 0.3, 1.0)
        };

        builder.rrect(view.rect, [4.0; 4], border_color);

        builder.rrect(
            gfx::RectF::new(
                view.rect.x + 1.0,
                view.rect.y + 1.0,
                view.rect.w - 2.0,
                view.rect.h - 2.0,
            ),
            [3.0; 4],
            gfx::Color::rgba(1.0, 1.0, 1.0, 1.0),
        );

        // Field text or placeholder
        let display_text = if view.text.is_empty() {
            view.placeholder.unwrap_or("").into()
        } else {
            view.text.into()
        };

        let text_color = if view.text.is_empty() {
            gfx::Color::rgba(0.6, 0.6, 0.6, 1.0)
        } else {
            gfx::Color::rgba(0.1, 0.1, 0.1, 1.0)
        };

        let field_text = Label {
            text: display_text,
            color: text_color,
            align: Align::Left,
            wrap: false,
            font_id: 0,
            font_px: 14.0,
        };
        field_text.encode(
            gfx::RectF::new(
                view.rect.x + 10.0,
                view.rect.y + (view.rect.h - 16.0) / 2.0,
                view.rect.w - 20.0,
                20.0,
            ),
            device_scale,
            text_ctx,
            uploader,
            builder,
        );
    }

    fn draw_permission_indicator(
        &self,
        x: f32,
        y: f32,
        _name: &str,
        status: PermissionStatus,
        builder: &mut DrawListBuilder,
    ) {
        let color = match status {
            PermissionStatus::Authorized => gfx::Color::rgba(0.2, 0.8, 0.3, 1.0),
            PermissionStatus::Denied => gfx::Color::rgba(0.9, 0.3, 0.3, 1.0),
            PermissionStatus::NotDetermined => gfx::Color::rgba(0.5, 0.5, 0.5, 1.0),
            PermissionStatus::Limited => gfx::Color::rgba(0.8, 0.5, 0.2, 1.0),
        };

        // Status dot
        builder.rrect(gfx::RectF::new(x, y, 12.0, 12.0), [6.0; 4], color);
    }
}

fn point_in_rect(point: [f32; 2], rect: gfx::RectF) -> bool {
    point[0] >= rect.x
        && point[0] <= rect.x + rect.w
        && point[1] >= rect.y
        && point[1] <= rect.y + rect.h
}
