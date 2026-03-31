//! Test scenes and simple router for the Oxide UI test app.

#![no_std]
extern crate alloc;
extern crate std;

mod animation_config;
mod integration;
mod orchestration;
mod permissions;
mod stress_test;

use alloc::collections::{BTreeMap, VecDeque};
use alloc::format;
use oxide_permissions::{PermissionManager, PermissionState, SensorBridge};
use oxide_platform_api as api;
use oxide_renderer_api as gfx;
use oxide_telemetry::{TelemetryHub, TelemetrySnapshot};
use oxide_timing as timing;
use oxide_ui_core::{
    anim,
    camera::{
        recording_event_to_ui, CameraController, CameraEvent, CameraMode, CameraPreviewNode,
        CropperState, VolumeHudState,
    },
    collection, elements,
    layout_async::AsyncLayoutCoordinator,
    permissions::PermissionOverlayUi,
    Axis, Dim, DrawListBuilder, Edges, NodeId, NodeStyle, NodeTree, Size2D,
};
use std::sync::Arc;

pub use oxide_ui_core::camera::{CameraMetrics, CameraRecordingUiEvent};

const LEGACY_BADGE_IMAGE: gfx::ImageHandle = gfx::ImageHandle(1);

// ===== Utilities =====

pub struct Counters {
    pub fps: f32,
    pub draws: usize,
    pub anims: usize,
}

impl Default for Counters {
    fn default() -> Self {
        Self { fps: 0.0, draws: 0, anims: 0 }
    }
}

pub struct FpsCounter {
    last_ms: u64,
    acc_ms: u64,
    frames: u32,
    pub fps: f32,
}

impl Default for FpsCounter {
    fn default() -> Self {
        Self { last_ms: 0, acc_ms: 0, frames: 0, fps: 0.0 }
    }
}

impl FpsCounter {
    pub fn tick(&mut self, now_ms: u64) {
        if self.last_ms == 0 {
            self.last_ms = now_ms;
            return;
        }
        let dt = now_ms.saturating_sub(self.last_ms);
        self.last_ms = now_ms;
        self.acc_ms += dt;
        self.frames += 1;
        if self.acc_ms >= 1000 {
            self.fps = (self.frames as f32) * 1000.0 / (self.acc_ms as f32);
            self.acc_ms = 0;
            self.frames = 0;
        }
    }
}

// ===== Scenes =====

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneKind {
    Controls,
    TextLayout,
    ZoomImage,
    AnimTimeline,
    CollectionStress,
    DamageLab,
    InputLab,
    NineSlice,
    SdfText,
    Snapshot,
    Camera,
    ElementsExtended,
    AnimationConfig,
    Orchestration,
    Permissions,
    Integration,
    StressTest,
}

pub struct Router<U: elements::ImageUploader> {
    pub current: SceneKind,
    pub text: elements::TextCtx,
    pub uploader: U,
    pub counters: Counters,
    fps: FpsCounter,
    overlay_visible: bool,
    reduce_motion_on: bool,
    // Accumulated damage rects for the last draw (dp units)
    last_damage: alloc::vec::Vec<gfx::RectI>,
    // Scene states
    controls: Controls,
    text_layout: TextLayout,
    zoom_image: ZoomImage,
    anim_timeline: AnimTimeline,
    collection_stress: CollectionStress,
    nine_slice: NineSliceDemo,
    sdf_demo: SdfTextDemo,
    damage_lab: DamageLab,
    input_lab: InputLab,
    camera: CameraDemo,
    readback: ReadbackDemo,
    elements_extended: ElementsExtended,
    animation_config: animation_config::AnimationConfigScene,
    orchestration: orchestration::OrchestrationScene,
    permissions: permissions::PermissionsScene,
    integration: integration::IntegrationScene,
    stress_test: stress_test::StressTestScene,
    damage_stats_pct: f32,
    damage_stats_rects: u32,
    sensors: Option<oxide_ui_core::sensors::SensorView>,
    telemetry: Option<oxide_ui_core::telemetry::TelemetryView>,
}

impl<U: elements::ImageUploader> Router<U> {
    pub fn new(uploader: U) -> Self {
        Self {
            current: SceneKind::Controls,
            text: elements::TextCtx::default(),
            uploader,
            counters: Counters::default(),
            fps: FpsCounter::default(),
            overlay_visible: true,
            reduce_motion_on: false,
            last_damage: alloc::vec::Vec::new(),
            controls: Controls::default(),
            text_layout: TextLayout::default(),
            zoom_image: ZoomImage::default(),
            anim_timeline: AnimTimeline::default(),
            collection_stress: CollectionStress::default(),
            nine_slice: NineSliceDemo::default(),
            sdf_demo: SdfTextDemo::default(),
            damage_lab: DamageLab::default(),
            input_lab: InputLab::default(),
            camera: CameraDemo::default(),
            readback: ReadbackDemo::default(),
            elements_extended: ElementsExtended::default(),
            animation_config: animation_config::AnimationConfigScene::default(),
            orchestration: orchestration::OrchestrationScene::default(),
            permissions: permissions::PermissionsScene::default(),
            integration: integration::IntegrationScene::default(),
            stress_test: stress_test::StressTestScene::default(),
            damage_stats_pct: 0.0,
            damage_stats_rects: 0,
            sensors: None,
            telemetry: None,
        }
    }

    pub fn scene_names() -> &'static [&'static str] {
        &[
            "Controls",
            "Text Layout",
            "Zoom Image",
            "Animations",
            "Collection Stress",
            "Damage Lab",
            "Input & Haptics",
            "Nine Slice",
            "SDF Text",
            "Snapshot",
            "Camera",
            "Elements Extended",
            "Animation Timings",
            "UI Orchestration",
            "Permissions",
            "Integration",
            "Stress Test",
        ]
    }

    pub fn set_scene(&mut self, i: usize) {
        let previous = self.current;
        self.current = match i {
            0 => SceneKind::Controls,
            1 => SceneKind::TextLayout,
            2 => SceneKind::ZoomImage,
            3 => SceneKind::AnimTimeline,
            4 => SceneKind::CollectionStress,
            5 => SceneKind::DamageLab,
            6 => SceneKind::InputLab,
            7 => SceneKind::NineSlice,
            8 => SceneKind::SdfText,
            9 => SceneKind::Snapshot,
            10 => SceneKind::Camera,
            11 => SceneKind::ElementsExtended,
            12 => SceneKind::AnimationConfig,
            13 => SceneKind::Orchestration,
            14 => SceneKind::Permissions,
            15 => SceneKind::Integration,
            16 => SceneKind::StressTest,
            _ => self.current,
        };
        if self.current == SceneKind::Camera && previous != SceneKind::Camera {
            self.camera.set_active(true);
        } else if previous == SceneKind::Camera && self.current != SceneKind::Camera {
            self.camera.set_active(false);
        }
    }

    pub fn update(&mut self, now_ms: u64, dt_ms: u32) {
        if self.overlay_visible {
            self.fps.tick(now_ms);
        }
        self.camera.set_active(matches!(self.current, SceneKind::Camera));
        match self.current {
            SceneKind::Controls => self.controls.update(dt_ms),
            SceneKind::TextLayout => self.text_layout.update(dt_ms),
            SceneKind::ZoomImage => self.zoom_image.update(dt_ms),
            SceneKind::AnimTimeline => self.anim_timeline.update(dt_ms),
            SceneKind::CollectionStress => self.collection_stress.update(dt_ms),
            SceneKind::DamageLab => self.damage_lab.update(dt_ms),
            SceneKind::InputLab => self.input_lab.update(dt_ms),
            SceneKind::NineSlice => {}
            SceneKind::SdfText => {}
            SceneKind::Snapshot => self.readback.update(dt_ms),
            SceneKind::Camera => self.camera.update(dt_ms),
            SceneKind::ElementsExtended => self.elements_extended.update(dt_ms),
            SceneKind::AnimationConfig => self.animation_config.update(dt_ms),
            SceneKind::Orchestration => self.orchestration.update(dt_ms),
            SceneKind::Permissions => self.permissions.update(dt_ms),
            SceneKind::Integration => self.integration.update(dt_ms),
            SceneKind::StressTest => self.stress_test.update(dt_ms),
        }
    }

    // ===== Hotkey helpers =====
    pub fn key_scene_select(&mut self, idx0: usize) {
        self.set_scene(idx0);
    }

    pub fn key_space_down(&mut self) {
        if let SceneKind::Controls = self.current {
            self.controls.key_space_down();
        }
    }
    pub fn key_space_up(&mut self) {
        if let SceneKind::Controls = self.current {
            let _ = self.controls.key_space_up();
        }
    }

    pub fn key_arrow_left(&mut self) {
        match self.current {
            SceneKind::Controls => self.controls.key_arrow_left(),
            SceneKind::CollectionStress => self.collection_stress.view.focus_move_left(),
            _ => {}
        }
    }
    pub fn key_arrow_right(&mut self) {
        match self.current {
            SceneKind::Controls => self.controls.key_arrow_right(),
            SceneKind::CollectionStress => self.collection_stress.view.focus_move_right(),
            _ => {}
        }
    }
    pub fn key_arrow_up(&mut self) {
        if let SceneKind::CollectionStress = self.current {
            self.collection_stress.view.focus_move_up();
        }
    }
    pub fn key_arrow_down(&mut self) {
        if let SceneKind::CollectionStress = self.current {
            self.collection_stress.view.focus_move_down();
        }
    }

    pub fn key_zoom_reset(&mut self) {
        if let SceneKind::ZoomImage = self.current {
            self.zoom_image.double_tap();
        }
    }

    pub fn toggle_overlay(&mut self) {
        self.overlay_visible = !self.overlay_visible;
    }

    pub fn set_reduce_motion(&mut self, on: bool) {
        self.reduce_motion_on = on;
        self.anim_timeline.animator.set_reduce_motion(on);
    }

    // Set the image used by the Zoom Image scene.
    pub fn set_zoom_image(&mut self, tex: gfx::ImageHandle, w: u32, h: u32) {
        self.zoom_image.image.image = tex;
        self.zoom_image.image.natural_w = w;
        self.zoom_image.image.natural_h = h;
    }

    pub fn trim_memory(&mut self) {
        self.last_damage.clear();
        self.text.trim_memory();
    }

    pub fn permissions_bind(&mut self, manager: &Arc<PermissionManager>) {
        self.camera.bind_permission_manager(manager);
    }

    pub fn permissions_update(&mut self, states: &[PermissionState]) {
        self.camera.update_permissions(states);
    }

    pub fn sensors_bind(&mut self, bridge: &Arc<SensorBridge>) {
        self.sensors = Some(oxide_ui_core::sensors::SensorView::new(Arc::clone(bridge)));
    }

    pub fn sensors_snapshot(&self) -> Option<oxide_ui_core::sensors::SensorSnapshot> {
        self.sensors.as_ref().map(|view| view.snapshot())
    }

    pub fn telemetry_bind(&mut self, hub: &Arc<TelemetryHub>) {
        self.telemetry = Some(oxide_ui_core::telemetry::TelemetryView::new(Arc::clone(hub)));
    }

    pub fn telemetry_snapshot(&self) -> Option<TelemetrySnapshot> {
        self.telemetry.as_ref().map(|view| view.snapshot())
    }

    pub fn input_pointer(&mut self, x: f32, y: f32, dx: f32, dy: f32, buttons: u32) {
        match self.current {
            SceneKind::Controls => self.controls.input_pointer(x, y, dx, dy, buttons),
            SceneKind::TextLayout => {}
            SceneKind::ZoomImage => self.zoom_image.input_pointer(x, y, dx, dy, buttons),
            SceneKind::AnimTimeline => {}
            SceneKind::CollectionStress => {}
            SceneKind::DamageLab => {}
            SceneKind::InputLab => self.input_lab.pointer_event(x, y, dy, buttons),
            SceneKind::NineSlice => {}
            SceneKind::SdfText => {}
            SceneKind::Snapshot => {}
            SceneKind::Camera => self.camera.input_pointer(x, y, dx, dy, buttons),
            SceneKind::ElementsExtended => {
                self.elements_extended.input_pointer(x, y, dx, dy, buttons)
            }
            SceneKind::AnimationConfig => {
                self.animation_config.input_pointer(x, y, dx, dy, buttons)
            }
            SceneKind::Orchestration => self.orchestration.input_pointer(x, y, dx, dy, buttons),
            SceneKind::Permissions => self.permissions.input_pointer(x, y, dx, dy, buttons),
            SceneKind::Integration => self.integration.input_pointer(x, y, dx, dy, buttons),
            SceneKind::StressTest => self.stress_test.input_pointer(x, y, dx, dy, buttons),
        }
    }

    pub fn input_key(&mut self, key: &oxide_platform_api::KeyEvent) {
        if matches!(self.current, SceneKind::InputLab) {
            self.input_lab.key_event(key);
        }
    }

    pub fn input_pinch(&mut self, cx: f32, cy: f32, delta: f32) {
        match self.current {
            SceneKind::ZoomImage => self.zoom_image.pinch(cx, cy, delta),
            SceneKind::Camera => self.camera.pinch(delta),
            _ => {}
        }
    }

    pub fn input_double_tap(&mut self) {
        match self.current {
            SceneKind::ZoomImage => self.zoom_image.double_tap(),
            SceneKind::Camera => self.camera.double_tap(),
            _ => {}
        }
    }

    pub fn draw(&mut self, viewport: gfx::RectF, device_scale: f32, b: &mut DrawListBuilder) {
        // Reset damage for this frame
        self.last_damage.clear();
        b.clip_push(gfx::RectI::new(0, 0, viewport.w.ceil() as i32, viewport.h.ceil() as i32));
        match self.current {
            SceneKind::Controls => {
                self.controls.draw(viewport, device_scale, &mut self.text, &mut self.uploader, b);
                // Damage: spinner + progress bar panel area (coarse but correct)
                let panel = gfx::RectF::new(
                    viewport.x + 20.0,
                    viewport.y + 20.0,
                    viewport.w - 40.0,
                    viewport.h - 40.0,
                );
                // Spinner rect
                self.push_damage(rectf_to_recti(gfx::RectF::new(
                    panel.x + 16.0,
                    panel.y + 72.0,
                    24.0,
                    24.0,
                )));
                // Progress bar rect
                self.push_damage(rectf_to_recti(gfx::RectF::new(
                    panel.x + 16.0,
                    panel.y + 48.0,
                    panel.w - 32.0,
                    12.0,
                )));
            }
            SceneKind::TextLayout => {
                self.text_layout.draw(
                    viewport,
                    device_scale,
                    &mut self.text,
                    &mut self.uploader,
                    b,
                );
                // Static text: no damage unless scene toggled (overlay handled below)
            }
            SceneKind::ZoomImage => {
                self.zoom_image.draw(viewport, device_scale, &mut self.text, &mut self.uploader, b);
                // Coarse: the image view rect (content area)
                let rect = gfx::RectF::new(
                    viewport.x + 40.0,
                    viewport.y + 40.0,
                    viewport.w - 80.0,
                    viewport.h - 80.0,
                );
                self.push_damage(rectf_to_recti(rect));
            }
            SceneKind::AnimTimeline => {
                self.anim_timeline.draw(
                    viewport,
                    device_scale,
                    &mut self.text,
                    &mut self.uploader,
                    b,
                );
                let area = gfx::RectF::new(
                    viewport.x + 30.0,
                    viewport.y + 24.0,
                    (viewport.w - 60.0).max(0.0),
                    160.0,
                );
                self.push_damage(rectf_to_recti(area));
            }
            SceneKind::CollectionStress => {
                self.collection_stress.draw(
                    viewport,
                    device_scale,
                    &mut self.text,
                    &mut self.uploader,
                    b,
                );
                // Coarse: the viewport area where tiles render
                self.push_damage(rectf_to_recti(viewport));
            }
            SceneKind::DamageLab => {
                self.damage_lab.draw(
                    viewport,
                    device_scale,
                    &mut self.text,
                    &mut self.uploader,
                    b,
                    DamageFrameStats { pct: self.damage_stats_pct, rects: self.damage_stats_rects },
                );
            }
            SceneKind::InputLab => {
                self.input_lab.draw(viewport, device_scale, &mut self.text, &mut self.uploader, b);
            }
            SceneKind::NineSlice => {
                self.nine_slice.draw::<U>(viewport, b);
            }
            SceneKind::SdfText => {
                self.sdf_demo.draw(viewport, device_scale, &mut self.text, &mut self.uploader, b);
            }
            SceneKind::Snapshot => {
                self.readback.draw(viewport, device_scale, &mut self.text, &mut self.uploader, b);
            }
            SceneKind::Camera => {
                self.camera.draw(viewport, device_scale, &mut self.text, &mut self.uploader, b);
                self.push_damage(rectf_to_recti(viewport));
            }
            SceneKind::ElementsExtended => {
                self.elements_extended.draw(
                    viewport,
                    device_scale,
                    &mut self.text,
                    &mut self.uploader,
                    b,
                );
                self.push_damage(rectf_to_recti(viewport));
            }
            SceneKind::AnimationConfig => {
                self.animation_config.draw(
                    viewport,
                    device_scale,
                    &mut self.text,
                    &mut self.uploader,
                    b,
                );
                self.push_damage(rectf_to_recti(viewport));
            }
            SceneKind::Orchestration => {
                self.orchestration.draw(
                    viewport,
                    device_scale,
                    &mut self.text,
                    &mut self.uploader,
                    b,
                );
                self.push_damage(rectf_to_recti(viewport));
            }
            SceneKind::Permissions => {
                self.permissions.draw(
                    viewport,
                    device_scale,
                    &mut self.text,
                    &mut self.uploader,
                    b,
                );
                self.push_damage(rectf_to_recti(viewport));
            }
            SceneKind::Integration => {
                self.integration.draw(
                    viewport,
                    device_scale,
                    &mut self.text,
                    &mut self.uploader,
                    b,
                );
                self.push_damage(rectf_to_recti(viewport));
            }
            SceneKind::StressTest => {
                self.stress_test.draw(
                    viewport,
                    device_scale,
                    &mut self.text,
                    &mut self.uploader,
                    b,
                );
                self.push_damage(rectf_to_recti(viewport));
            }
        }
        b.clip_pop();
        self.counters.draws = b.drawlist().items.len();
        if self.overlay_visible {
            self.counters.fps = self.fps.fps;
            self.counters.anims = self.anim_timeline.animator.active_count();
        } else {
            self.counters.fps = 0.0;
            self.counters.anims = 0;
        }
        // Overlay (toggleable)
        if self.overlay_visible {
            let rm = if self.reduce_motion_on { "RM:on" } else { "RM:off" };
            let mut extra = alloc::string::String::new();
            match self.current {
                SceneKind::AnimTimeline => {
                    let play = if self.anim_timeline.playing() { "play" } else { "pause" };
                    extra =
                        alloc::format!(" anim:{} phase={:.2}", play, self.anim_timeline.progress());
                }
                SceneKind::DamageLab => {
                    let dmg = if self.damage_lab.enabled { "on" } else { "off" };
                    extra = alloc::format!(
                        " damage:{} use={:.2} pre={:.2} rects={} pct={:.0}%",
                        dmg,
                        self.damage_lab.use_thresh,
                        self.damage_lab.prefilter,
                        self.damage_stats_rects,
                        (self.damage_stats_pct * 100.0).round(),
                    );
                }
                SceneKind::InputLab => {
                    if let Some(summary) = self.input_lab.overlay_summary() {
                        extra = summary;
                    }
                }
                SceneKind::NineSlice => {
                    extra = alloc::format!(
                        " slice={:.1}px alpha={:.2}",
                        self.nine_slice.slice_px,
                        self.nine_slice.alpha
                    );
                }
                SceneKind::SdfText => {
                    extra = alloc::format!(" font_px={:.1}", self.sdf_demo.font_px);
                }
                SceneKind::Snapshot => {
                    extra = alloc::format!(" status: {}", self.readback.status());
                }
                SceneKind::Camera => {
                    extra = self.camera.overlay_line();
                }
                _ => {}
            }
            let base = if extra.is_empty() {
                alloc::format!(
                    "{} | {:.0} fps | draws={} | anims={} | {}",
                    Self::scene_names()[self.current as usize],
                    self.counters.fps,
                    self.counters.draws,
                    self.counters.anims,
                    rm
                )
            } else {
                alloc::format!(
                    "{} | {:.0} fps | draws={} | anims={} | {}{}",
                    Self::scene_names()[self.current as usize],
                    self.counters.fps,
                    self.counters.draws,
                    self.counters.anims,
                    rm,
                    extra
                )
            };
            let overlay = elements::Label {
                text: base,
                color: gfx::Color::rgba(0.1, 0.1, 0.1, 1.0),
                align: elements::Align::Left,
                wrap: false,
                font_id: 0,
                font_px: 12.0,
            };
            let bg = gfx::Color::rgba(1.0, 1.0, 1.0, 0.85);
            let rect = gfx::RectF::new(viewport.x + 8.0, viewport.y + 8.0, 440.0, 18.0);
            // Blur the backdrop behind overlay, then draw overlay panel
            b.backdrop(rect, 6.0, gfx::Color::rgba(1.0, 1.0, 1.0, 1.0), 0.9);
            b.rrect(rect, [4.0; 4], bg);
            // Overlay damage (panel rect)
            self.push_damage(rectf_to_recti(rect));
            overlay.encode(
                gfx::RectF::new(viewport.x + 12.0, viewport.y + 10.0, 352.0, 14.0),
                device_scale,
                &mut self.text,
                &mut self.uploader,
                b,
            );
        }
    }

    // Camera scene external controls via host (iOS)
    pub fn camera_set_options(&mut self, blur: bool, sigma: f32, grayscale: bool, animate: bool) {
        self.camera.set_options(blur, sigma, grayscale, animate);
    }

    pub fn camera_set_metrics(&mut self, metrics: CameraMetrics) {
        self.camera.set_metrics(metrics);
    }

    pub fn camera_attach_manager(&mut self, manager: Arc<dyn api::CameraManager + Send + Sync>) {
        self.camera.attach_manager(manager);
    }

    pub fn camera_detach_manager(&mut self) {
        self.camera.detach_manager();
    }

    pub fn camera_volume_level(&mut self, level: f32) {
        self.camera.show_volume(level);
    }

    pub fn camera_recording_started(&mut self) {
        self.camera.set_recording(true);
    }

    pub fn camera_recording_event(&mut self, event: CameraRecordingUiEvent) {
        self.camera.on_record_event(event);
    }

    pub fn input_commit(&mut self, text: &str) {
        if matches!(self.current, SceneKind::InputLab) {
            self.input_lab.commit(text);
        }
    }

    pub fn input_set_selection(&mut self, start: u32, end: u32) {
        if matches!(self.current, SceneKind::InputLab) {
            self.input_lab.set_selection(start, end);
        }
    }

    pub fn input_set_composition(&mut self, start: u32, end: u32, text: &str) {
        if matches!(self.current, SceneKind::InputLab) {
            self.input_lab.set_composition(start, end, text);
        }
    }

    pub fn input_set_ime_rect(&mut self, rect: gfx::RectF) {
        if matches!(self.current, SceneKind::InputLab) {
            self.input_lab.set_ime_rect(rect);
        }
    }

    pub fn input_hide_ime(&mut self) {
        if matches!(self.current, SceneKind::InputLab) {
            self.input_lab.hide_ime();
        }
    }

    pub fn input_log(&mut self, msg: &str) {
        self.input_lab.log(msg);
    }

    pub fn nine_slice_set_image(&mut self, tex: gfx::ImageHandle) {
        self.nine_slice.set_image(tex);
    }

    pub fn nine_slice_set_options(&mut self, slice: f32, alpha: f32) {
        self.nine_slice.set_options(slice, alpha);
    }

    pub fn sdf_set_font_px(&mut self, px: f32) {
        self.sdf_demo.set_font_px(px);
    }

    pub fn anim_set_play(&mut self, play: bool) {
        self.anim_timeline.set_playing(play);
    }

    pub fn anim_set_progress(&mut self, normalized: f32) {
        self.anim_timeline.set_progress(normalized);
    }

    pub fn anim_progress(&self) -> f32 {
        self.anim_timeline.progress()
    }

    pub fn anim_playing(&self) -> bool {
        self.anim_timeline.playing()
    }

    pub fn damage_set_options(&mut self, enabled: bool, use_thresh: f32, prefilter: f32) {
        self.damage_lab.set_options(enabled, use_thresh, prefilter);
    }

    pub fn damage_set_stats(&mut self, pct: f32, rects: u32) {
        self.damage_stats_pct = pct;
        self.damage_stats_rects = rects;
    }

    pub fn readback_set_status(&mut self, status: impl Into<alloc::string::String>) {
        self.readback.set_status(status);
    }

    pub fn readback_status(&self) -> &str {
        self.readback.status()
    }

    // Return last frame's damage rectangles (dp units). Caller can coalesce as needed.
    pub fn take_damage(&mut self) -> alloc::vec::Vec<gfx::RectI> {
        core::mem::take(&mut self.last_damage)
    }

    fn push_damage(&mut self, r: gfx::RectI) {
        if r.w > 0 && r.h > 0 {
            self.last_damage.push(r);
        }
    }
}

fn rectf_to_recti(r: gfx::RectF) -> gfx::RectI {
    let x = r.x.floor() as i32;
    let y = r.y.floor() as i32;
    let w = r.w.ceil() as i32;
    let h = r.h.ceil() as i32;
    gfx::RectI { x, y, w, h }
}

// ---- Controls scene ----

#[derive(Default)]
pub struct Controls {
    t: f32,
    progress: f32,
    button: elements::Button,
    button_state: elements::ButtonState,
    toggle: elements::Toggle,
    toggle_state: elements::ToggleState,
    slider: elements::Slider,
    slider_state: elements::SliderState,
}

impl Controls {
    pub fn update(&mut self, dt_ms: u32) {
        self.t += dt_ms as f32 / 1000.0;
        self.progress = (self.t * 0.25).sin() * 0.5 + 0.5;
    }
    pub fn input_pointer(&mut self, x: f32, y: f32, _dx: f32, _dy: f32, buttons: u32) {
        let r = gfx::RectF::new(40.0, 40.0, 140.0, 40.0);
        if buttons & 1 != 0 && x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h {
            self.button_state.on_pointer_down();
        } else if self.button_state.is_pressed() {
            let _ = self.button_state.on_pointer_up();
        }
    }
    pub fn key_space_down(&mut self) {
        self.button_state.on_pointer_down();
    }
    pub fn key_space_up(&mut self) -> bool {
        self.button_state.on_pointer_up()
    }
    pub fn key_arrow_left(&mut self) {
        let _ = self.slider_state.arrow_left(self.slider.step);
    }
    pub fn key_arrow_right(&mut self) {
        let _ = self.slider_state.arrow_right(self.slider.step);
    }
    #[allow(clippy::too_many_arguments)]
    pub fn draw<U: elements::ImageUploader>(
        &mut self,
        vp: gfx::RectF,
        ds: f32,
        text: &mut elements::TextCtx,
        up: &mut U,
        b: &mut DrawListBuilder,
    ) {
        // Layout a small panel
        let panel = gfx::RectF::new(vp.x + 20.0, vp.y + 20.0, vp.w - 40.0, vp.h - 40.0);
        b.rrect(panel, [8.0; 4], gfx::Color::rgba(0.96, 0.97, 0.99, 1.0));
        // Label
        let lbl = elements::Label {
            text: "Controls Showcase".into(),
            color: gfx::Color::rgba(0.1, 0.1, 0.1, 1.0),
            align: elements::Align::Left,
            wrap: false,
            font_id: 0,
            font_px: 18.0,
        };
        lbl.encode(gfx::RectF::new(panel.x + 16.0, panel.y + 12.0, 300.0, 24.0), ds, text, up, b);
        // Progress
        let pb = elements::ProgressBar {
            value: Some(self.progress),
            ..elements::ProgressBar::default()
        };
        pb.encode(gfx::RectF::new(panel.x + 16.0, panel.y + 48.0, panel.w - 32.0, 12.0), self.t, b);
        // Spinner
        let sp = elements::Spinner { alpha: 1.0 };
        sp.encode(gfx::RectF::new(panel.x + 16.0, panel.y + 72.0, 24.0, 24.0), b);
        // Button
        self.button.text = "Press Me".into();
        self.button.encode(
            gfx::RectF::new(panel.x + 16.0, panel.y + 108.0, 140.0, 40.0),
            ds,
            text,
            up,
            &self.button_state,
            b,
        );
        // Toggle
        self.toggle.encode(
            gfx::RectF::new(panel.x + 16.0, panel.y + 160.0, 60.0, 28.0),
            &self.toggle_state,
            b,
        );
        // Slider
        self.slider.encode(
            gfx::RectF::new(panel.x + 100.0, panel.y + 160.0, panel.w - 116.0, 28.0),
            &self.slider_state,
            b,
        );
    }
}

// ---- Text Layout ----

#[derive(Default)]
pub struct TextLayout {}

impl TextLayout {
    pub fn update(&mut self, _dt_ms: u32) {}
    pub fn draw<U: elements::ImageUploader>(
        &mut self,
        vp: gfx::RectF,
        ds: f32,
        text: &mut elements::TextCtx,
        up: &mut U,
        b: &mut DrawListBuilder,
    ) {
        b.rrect(vp, [0.0; 4], gfx::Color::rgba(1.0, 1.0, 1.0, 1.0));
        let samples = [
         ("Left align, wrapping paragraph in English. This tests wrapping and alignment across multiple lines.", elements::Align::Left),
         ("مرحبا بالعالم — نص عربي من اليمين إلى اليسار.", elements::Align::Center),
         ("これは日本語のテキストで、折り返しとレンダリングを確認します。", elements::Align::Right),
      ];
        let mut y = vp.y + 20.0;
        for (s, align) in samples {
            let lbl = elements::Label {
                text: s.into(),
                color: gfx::Color::rgba(0.1, 0.1, 0.1, 1.0),
                align,
                wrap: true,
                font_id: 0,
                font_px: 16.0,
            };
            lbl.encode(gfx::RectF::new(vp.x + 20.0, y, vp.w - 40.0, 80.0), ds, text, up, b);
            y += 90.0;
        }
    }
}

// ---- Zoomable Image ----

#[derive(Default)]
pub struct ZoomImage {
    pub image: elements::ImageView,
    pub zoom: elements::ImageZoomState,
}

impl ZoomImage {
    pub fn update(&mut self, _dt_ms: u32) {}
    pub fn input_pointer(&mut self, _x: f32, _y: f32, dx: f32, dy: f32, buttons: u32) {
        if buttons & 1 != 0 {
            self.zoom.pan(dx, dy);
        }
    }
    pub fn pinch(&mut self, cx: f32, cy: f32, delta: f32) {
        self.zoom.pinch(delta, [cx, cy]);
    }
    pub fn double_tap(&mut self) {
        self.zoom.double_tap_zoom_out();
    }
    pub fn draw<U: elements::ImageUploader>(
        &mut self,
        vp: gfx::RectF,
        _ds: f32,
        _text: &mut elements::TextCtx,
        _up: &mut U,
        b: &mut DrawListBuilder,
    ) {
        b.rrect(vp, [0.0; 4], gfx::Color::rgba(0.98, 0.98, 0.98, 1.0));
        let rect = gfx::RectF::new(vp.x + 40.0, vp.y + 40.0, vp.w - 80.0, vp.h - 80.0);
        self.image.encode(rect, Some(&self.zoom), b);
    }
}

// ---- Animations Timeline ----

pub struct AnimTimeline {
    pub animator: anim::Animator,
    phase: f32,
    playing: bool,
    overrides: BTreeMap<NodeId, anim::AnimOverrides>,
    shake_node: NodeId,
    wiggle_node: NodeId,
    scatter_node: NodeId,
}

impl Default for AnimTimeline {
    fn default() -> Self {
        Self {
            animator: anim::Animator::default(),
            phase: 0.0,
            playing: true,
            overrides: BTreeMap::new(),
            shake_node: NodeId(0x610),
            wiggle_node: NodeId(0x611),
            scatter_node: NodeId(0x612),
        }
    }
}

impl AnimTimeline {
    pub fn update(&mut self, dt_ms: u32) {
        if self.playing {
            self.phase += dt_ms as f32 / 1000.0;
        }
        let now = timing::now_ms();
        self.overrides = self.animator.step(now);
        if self.playing {
            self.ensure_sequences();
        }
    }
    pub fn draw<U: elements::ImageUploader>(
        &mut self,
        vp: gfx::RectF,
        ds: f32,
        text: &mut elements::TextCtx,
        up: &mut U,
        b: &mut DrawListBuilder,
    ) {
        b.rrect(vp, [10.0; 4], gfx::Color::rgba(0.96, 0.97, 1.0, 1.0));

        let shake_base = gfx::RectF::new(vp.x + 40.0, vp.y + 36.0, 120.0, 120.0);
        let wiggle_base = gfx::RectF::new(vp.x + 200.0, vp.y + 36.0, 120.0, 120.0);
        let scatter_base =
            gfx::RectF::new(vp.x + 360.0_f32.min(vp.x + vp.w - 160.0), vp.y + 36.0, 120.0, 120.0);

        let shake_rect = self.rect_for(self.shake_node, shake_base);
        let wiggle_rect = self.rect_for(self.wiggle_node, wiggle_base);
        let scatter_rect = self.rect_for(self.scatter_node, scatter_base);
        let scatter_alpha = self.opacity_for(self.scatter_node);

        b.rrect(shake_rect, [14.0; 4], gfx::Color::rgba(0.30, 0.55, 0.95, 0.95));
        b.rrect(wiggle_rect, [18.0; 4], gfx::Color::rgba(0.47, 0.76, 0.60, 0.92));
        b.rrect(
            scatter_rect,
            [60.0; 4],
            gfx::Color::rgba(0.98, 0.74, 0.40, scatter_alpha.max(0.05)),
        );

        let mut label = elements::Label {
            text: "Shake".into(),
            color: gfx::Color::rgba(1.0, 1.0, 1.0, 0.9),
            align: elements::Align::Center,
            wrap: false,
            font_id: 0,
            font_px: 16.0,
        };
        label.encode(shake_rect, ds, text, up, b);
        label.text = "Wiggle".into();
        label.encode(wiggle_rect, ds, text, up, b);
        label.text = "Scatter".into();
        label.encode(scatter_rect, ds, text, up, b);
    }

    fn ensure_sequences(&mut self) {
        if !self.animator.is_active(self.shake_node) {
            let seq = anim::helpers::shake(
                anim::helpers::identity_transform(),
                anim::helpers::Axis2D::Horizontal,
                16.0,
                3,
                480,
            );
            self.animator.start_sequence(self.shake_node, &seq);
        }
        if !self.animator.is_active(self.wiggle_node) {
            let seq = anim::helpers::wiggle(anim::helpers::identity_transform(), 0.15, 3, 540);
            self.animator.start_sequence(self.wiggle_node, &seq);
        }
        if !self.animator.is_active(self.scatter_node) {
            let seq = anim::helpers::scatter(
                anim::helpers::identity_transform(),
                [48.0, -28.0],
                560,
                true,
            );
            self.animator.start_sequence(self.scatter_node, &seq);
        }
    }

    fn rect_for(&self, node: NodeId, base: gfx::RectF) -> gfx::RectF {
        let Some(over) = self.overrides.get(&node) else {
            return base;
        };
        let Some(tr) = over.transform else {
            return base;
        };
        let sx = if tr.sx.is_finite() && tr.sx > 0.0 { tr.sx } else { 1.0 };
        let sy = if tr.sy.is_finite() && tr.sy > 0.0 { tr.sy } else { 1.0 };
        let cx = base.x + base.w * 0.5;
        let cy = base.y + base.h * 0.5;
        let w = base.w * sx;
        let h = base.h * sy;
        gfx::RectF::new(cx - w * 0.5 + tr.tx, cy - h * 0.5 + tr.ty, w, h)
    }

    fn opacity_for(&self, node: NodeId) -> f32 {
        if let Some(over) = self.overrides.get(&node) {
            if let Some(alpha) = over.opacity {
                return alpha.clamp(0.0, 1.0);
            }
        }
        1.0
    }

    pub fn set_playing(&mut self, play: bool) {
        self.playing = play;
    }

    pub fn set_progress(&mut self, normalized: f32) {
        let clamped = normalized.clamp(0.0, 1.0);
        self.phase = clamped * core::f32::consts::TAU;
    }

    pub fn progress(&self) -> f32 {
        (self.phase / core::f32::consts::TAU).rem_euclid(1.0)
    }

    pub fn playing(&self) -> bool {
        self.playing
    }
}

// ---- Camera demo ----

// ---- Collection Stress ----

pub struct CollectionStress {
    pub view: collection::CollectionView,
}

impl Default for CollectionStress {
    fn default() -> Self {
        let mut view = collection::CollectionView::new(collection::CollectionMode::VerticalGrid {
            col_width: 100.0,
            spacing: 8.0,
        });
        view.set_transition(Some(collection::CellTransition::shrink_grow(420, 0.82, 1.08)));
        Self { view }
    }
}

impl CollectionStress {
    pub fn update(&mut self, _dt_ms: u32) {
        if matches!(self.view.content_metrics().content_h, 0.0) {}
    }

    pub fn draw<U: elements::ImageUploader>(
        &mut self,
        vp: gfx::RectF,
        _ds: f32,
        _text: &mut elements::TextCtx,
        _up: &mut U,
        b: &mut DrawListBuilder,
    ) {
        if let collection::CollectionMode::VerticalGrid { .. } = self.view_mode() {
        } else {
            self.view = collection::CollectionView::new(collection::CollectionMode::VerticalGrid {
                col_width: 100.0,
                spacing: 8.0,
            });
            self.view
                .set_transition(Some(collection::CellTransition::shrink_grow(420, 0.82, 1.08)));
            self.view.set_count(5000);
        }
        let mut meas = GridMeasure;
        let mut rend = GridRenderer;
        let content = self.view.layout_and_render(vp, &mut meas, &mut rend, b);
        // Simple scrollbar indicator
        let ratio = (vp.h / content.content_h.max(1.0)).clamp(0.0, 1.0);
        let thumb_h = vp.h * ratio;
        b.rrect(
            gfx::RectF::new(vp.x + vp.w - 6.0, vp.y + 2.0, 4.0, thumb_h - 4.0),
            [2.0; 4],
            gfx::Color::rgba(0.0, 0.0, 0.0, 0.2),
        );
    }

    fn view_mode(&self) -> &collection::CollectionMode {
        self.view.mode()
    }
}

// ---- Damage Lab ----

pub struct DamageLab {
    pub enabled: bool,
    pub use_thresh: f32,
    pub prefilter: f32,
}

impl Default for DamageLab {
    fn default() -> Self {
        Self { enabled: false, use_thresh: 0.70, prefilter: 0.25 }
    }
}

#[derive(Clone, Copy)]
pub struct DamageFrameStats {
    pct: f32,
    rects: u32,
}

const INPUT_LOG_CAP: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FocusField {
    None,
    Username,
    Password,
}

struct FormNodes {
    header: NodeId,
    username: NodeId,
    password: NodeId,
    submit: NodeId,
    validation: NodeId,
    picker: NodeId,
    logs: NodeId,
}

struct FormLayout {
    tree: NodeTree,
    nodes: FormNodes,
    coordinator: AsyncLayoutCoordinator<alloc::vec::Vec<(NodeId, oxide_ui_core::LayoutRect)>>,
    last_viewport: Option<(f32, f32)>,
    layout_ready: bool,
}

impl FormLayout {
    fn new() -> Self {
        let mut tree = NodeTree::new_root(NodeStyle {
            axis: Axis::Column,
            padding: Edges { left: 18.0, top: 18.0, right: 18.0, bottom: 18.0 },
            gap: 18.0,
            background: gfx::Color::rgba(0.0, 0.0, 0.0, 0.0),
            ..Default::default()
        });
        let root = tree.root();
        let header = tree.add_node(
            root,
            NodeStyle {
                size: Size2D { w: Dim::Auto, h: Dim::Px(26.0) },
                margin: Edges { left: 0.0, top: 0.0, right: 0.0, bottom: 28.0 },
                background: gfx::Color::rgba(0.0, 0.0, 0.0, 0.0),
                ..Default::default()
            },
        );
        let content_row = tree.add_node(
            root,
            NodeStyle {
                axis: Axis::Row,
                gap: 24.0,
                flex_grow: 0.0,
                background: gfx::Color::rgba(0.0, 0.0, 0.0, 0.0),
                ..Default::default()
            },
        );
        let form_column = tree.add_node(
            content_row,
            NodeStyle {
                axis: Axis::Column,
                gap: 16.0,
                flex_grow: 1.0,
                background: gfx::Color::rgba(0.0, 0.0, 0.0, 0.0),
                ..Default::default()
            },
        );
        let username = tree.add_node(
            form_column,
            NodeStyle {
                size: Size2D { w: Dim::Auto, h: Dim::Px(56.0) },
                background: gfx::Color::rgba(0.0, 0.0, 0.0, 0.0),
                ..Default::default()
            },
        );
        let password = tree.add_node(
            form_column,
            NodeStyle {
                size: Size2D { w: Dim::Auto, h: Dim::Px(56.0) },
                background: gfx::Color::rgba(0.0, 0.0, 0.0, 0.0),
                ..Default::default()
            },
        );
        let submit = tree.add_node(
            form_column,
            NodeStyle {
                size: Size2D { w: Dim::Px(190.0), h: Dim::Px(46.0) },
                margin: Edges { left: 0.0, top: 8.0, right: 0.0, bottom: 8.0 },
                background: gfx::Color::rgba(0.0, 0.0, 0.0, 0.0),
                ..Default::default()
            },
        );
        let validation = tree.add_node(
            form_column,
            NodeStyle {
                size: Size2D { w: Dim::Auto, h: Dim::Px(20.0) },
                background: gfx::Color::rgba(0.0, 0.0, 0.0, 0.0),
                ..Default::default()
            },
        );
        let picker = tree.add_node(
            content_row,
            NodeStyle {
                size: Size2D { w: Dim::Px(220.0), h: Dim::Px(208.0) },
                margin: Edges { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 },
                background: gfx::Color::rgba(0.0, 0.0, 0.0, 0.0),
                ..Default::default()
            },
        );
        let logs = tree.add_node(
            root,
            NodeStyle {
                size: Size2D { w: Dim::Auto, h: Dim::Auto },
                flex_grow: 1.0,
                margin: Edges { left: 0.0, top: 24.0, right: 0.0, bottom: 0.0 },
                padding: Edges { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 },
                background: gfx::Color::rgba(0.0, 0.0, 0.0, 0.0),
                ..Default::default()
            },
        );

        let nodes = FormNodes { header, username, password, submit, validation, picker, logs };
        Self {
            tree,
            nodes,
            coordinator: AsyncLayoutCoordinator::new(),
            last_viewport: None,
            layout_ready: false,
        }
    }

    fn schedule(&mut self, size: (f32, f32)) {
        let mut snapshot = self.tree.clone();
        let _ = self.coordinator.request(move || {
            snapshot.layout(size.0, size.1);
            snapshot.collect_layouts()
        });
        self.layout_ready = false;
    }

    fn poll_results(&mut self) -> bool {
        let mut changed = false;
        while let Some((_seq, layouts)) = self.coordinator.poll_latest() {
            self.tree.apply_layouts(&layouts);
            self.layout_ready = true;
            changed = true;
        }
        changed
    }

    fn ensure_layout(&mut self, viewport: gfx::RectF) -> bool {
        let size = (viewport.w.max(0.0), viewport.h.max(0.0));
        let mut changed = self.poll_results();
        if self.last_viewport != Some(size) {
            self.last_viewport = Some(size);
            self.schedule(size);
        }
        if !self.layout_ready {
            let mut immediate = self.tree.clone();
            immediate.layout(size.0, size.1);
            let layouts = immediate.collect_layouts();
            self.tree.apply_layouts(&layouts);
            self.layout_ready = true;
            changed = true;
        }
        changed
    }

    fn rect(&self, id: NodeId) -> Option<gfx::RectF> {
        self.tree.layout_rect(id).map(|r| gfx::RectF::new(r.x, r.y, r.w, r.h))
    }
}

pub struct InputLab {
    username_input: elements::TextInputState,
    password_input: elements::TextInputState,
    username_widget: elements::TextInput,
    password_widget: elements::TextInput,
    submit_button: elements::Button,
    submit_state: elements::ButtonState,
    overlay: elements::OverlayState,
    overlay_view: elements::Overlay,
    popup: elements::PopupWindow,
    overlay_message: alloc::string::String,
    picker: elements::PickerState,
    picker_style: elements::PickerStyle,
    animator: anim::Animator,
    anim_overrides: BTreeMap<NodeId, anim::AnimOverrides>,
    username_node: NodeId,
    password_node: NodeId,
    submit_node: NodeId,
    picker_node: NodeId,
    focused: FocusField,
    last_username_rect: gfx::RectF,
    last_password_rect: gfx::RectF,
    last_submit_rect: gfx::RectF,
    last_picker_rect: gfx::RectF,
    logs: VecDeque<alloc::string::String>,
    form_layout: FormLayout,
}

impl Default for InputLab {
    fn default() -> Self {
        let mut username_input = elements::TextInputState::new("Display name");
        username_input.set_validator(|text| text.chars().count() >= 3);
        username_input.set_autocorrect(false);
        username_input.set_autocapitalization(api::AutoCapitalization::Words);
        username_input.set_return_key(api::ReturnKeyType::Next);
        let mut password_input =
            elements::TextInputState::with_secure("Password (8+ chars, number)", true);
        password_input.set_validator(|text| {
            text.chars().count() >= 8 && text.chars().any(|c| c.is_ascii_digit())
        });
        password_input.set_autocorrect(false);
        password_input.set_return_key(api::ReturnKeyType::Done);
        password_input.add_accessory_button("Paste Code");
        let overlay = elements::OverlayState::new();
        let picker_items = alloc::vec![
            alloc::string::String::from("Conference Pass"),
            alloc::string::String::from("Guest Access"),
            alloc::string::String::from("Team Member"),
            alloc::string::String::from("Partner"),
            alloc::string::String::from("Moderator"),
        ];
        let submit_button = elements::Button {
            text: alloc::string::String::from("Create Mission"),
            ..elements::Button::default()
        };
        let animator = anim::Animator::default();
        let form_layout = FormLayout::new();
        Self {
            username_input,
            password_input,
            username_widget: elements::TextInput::default(),
            password_widget: elements::TextInput::default(),
            submit_button,
            submit_state: elements::ButtonState::default(),
            overlay,
            overlay_view: elements::Overlay::default(),
            popup: elements::PopupWindow::default(),
            overlay_message: alloc::string::String::new(),
            picker: elements::PickerState::new(picker_items),
            picker_style: elements::PickerStyle::default(),
            animator,
            anim_overrides: BTreeMap::new(),
            username_node: NodeId(0x501),
            password_node: NodeId(0x502),
            submit_node: NodeId(0x503),
            picker_node: NodeId(0x504),
            focused: FocusField::None,
            last_username_rect: gfx::RectF::new(0.0, 0.0, 0.0, 0.0),
            last_password_rect: gfx::RectF::new(0.0, 0.0, 0.0, 0.0),
            last_submit_rect: gfx::RectF::new(0.0, 0.0, 0.0, 0.0),
            last_picker_rect: gfx::RectF::new(0.0, 0.0, 0.0, 0.0),
            logs: VecDeque::new(),
            form_layout,
        }
    }
}

impl InputLab {
    pub fn update(&mut self, dt_ms: u32) {
        if self.form_layout.poll_results() {
            self.refresh_rect_cache();
        }
        self.username_input.tick(dt_ms);
        self.password_input.tick(dt_ms);
        self.overlay.tick(dt_ms);
        self.picker.tick(dt_ms);
        let submitted = self.username_input.take_submit() || self.password_input.take_submit();
        if submitted {
            self.handle_submit();
        }
        let now = timing::now_ms();
        self.anim_overrides = self.animator.step(now);
    }

    pub fn pointer_event(&mut self, x: f32, y: f32, dy: f32, buttons: u32) {
        let point = [x, y];
        let pressed = buttons & 1 != 0;
        if pressed {
            if point_in_rect(point, self.last_username_rect) {
                self.focus(FocusField::Username);
            } else if point_in_rect(point, self.last_password_rect) {
                self.focus(FocusField::Password);
            } else if point_in_rect(point, self.last_submit_rect) {
                if !self.submit_state.is_pressed() {
                    self.submit_state.on_pointer_down();
                }
            } else {
                if self.submit_state.is_pressed() {
                    self.submit_state.on_pointer_cancel();
                }
                self.focus(FocusField::None);
            }
            if point_in_rect(point, self.last_picker_rect) {
                let row_height = self.picker_style.row_height(self.last_picker_rect);
                let scale = if row_height.abs() < f32::EPSILON { 1.0 } else { row_height };
                self.picker.scroll(dy / scale);
            }
        } else if self.submit_state.is_pressed() {
            let tapped =
                point_in_rect(point, self.last_submit_rect) && self.submit_state.on_pointer_up();
            if tapped {
                self.handle_submit();
            }
        }
    }

    pub fn key_event(&mut self, key: &oxide_platform_api::KeyEvent) {
        if let Some(input) = self.focused_mut() {
            input.handle_key(key);
        }
        if matches!(key.code, oxide_platform_api::KeyCode::Enter) {
            self.handle_submit();
        }
    }

    pub fn commit(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if let Some(input) = self.focused_mut() {
            input.handle_text_event(&oxide_platform_api::TextEvent::Commit { text: text.into() });
        }
    }

    pub fn set_selection(&mut self, start: u32, end: u32) {
        if let Some(input) = self.focused_mut() {
            input.handle_text_event(&oxide_platform_api::TextEvent::SelectionChanged {
                range: start..end,
            });
        }
    }

    pub fn set_composition(&mut self, start: u32, end: u32, text: &str) {
        if let Some(input) = self.focused_mut() {
            input.handle_text_event(&oxide_platform_api::TextEvent::Composition {
                range: start..end,
                text: text.into(),
            });
        }
    }

    pub fn set_ime_rect(&mut self, rect: gfx::RectF) {
        if let Some(input) = self.focused_mut() {
            input.handle_text_event(&oxide_platform_api::TextEvent::IMEShown(rect));
        }
    }

    pub fn hide_ime(&mut self) {
        self.username_input.handle_text_event(&oxide_platform_api::TextEvent::IMEHidden);
        self.password_input.handle_text_event(&oxide_platform_api::TextEvent::IMEHidden);
    }

    pub fn log(&mut self, msg: &str) {
        if msg.is_empty() {
            return;
        }
        if self.logs.len() == INPUT_LOG_CAP {
            self.logs.pop_front();
        }
        self.logs.push_back(msg.into());
    }

    pub fn overlay_summary(&self) -> Option<alloc::string::String> {
        let username_state = format!(
            "user={}, {:?}",
            Self::truncate(self.username_input.text(), 12),
            self.username_input.validation()
        );
        let password_state = format!(
            "pass_len={}, {:?}",
            self.password_input.text().chars().count(),
            self.password_input.validation()
        );
        let role = self.picker.selection_label().unwrap_or("none");
        Some(alloc::format!(" {} {} role={}", username_state, password_state, role))
    }

    pub fn draw<U: elements::ImageUploader>(
        &mut self,
        vp: gfx::RectF,
        ds: f32,
        text: &mut elements::TextCtx,
        up: &mut U,
        b: &mut DrawListBuilder,
    ) {
        b.rrect(vp, [0.0; 4], gfx::Color::rgba(0.95, 0.97, 1.0, 1.0));

        let header = elements::Label {
            text: "Input & Haptics".into(),
            color: gfx::Color::rgba(0.1, 0.1, 0.1, 1.0),
            align: elements::Align::Left,
            wrap: false,
            font_id: 0,
            font_px: 18.0,
        };
        if self.form_layout.ensure_layout(vp) {
            self.refresh_rect_cache();
        }

        let header_rect = self
            .form_layout
            .rect(self.form_layout.nodes.header)
            .unwrap_or(gfx::RectF::new(vp.x + 18.0, vp.y + 18.0, vp.w - 36.0, 26.0));
        header.encode(header_rect, ds, text, up, b);

        let username_rect = self.rect_for_node(self.last_username_rect, self.username_node);
        self.username_widget.encode(&self.username_input, username_rect, ds, text, up, b);

        let password_rect = self.rect_for_node(self.last_password_rect, self.password_node);
        self.password_widget.encode(&self.password_input, password_rect, ds, text, up, b);

        let submit_rect = self.rect_for_node(self.last_submit_rect, self.submit_node);
        self.submit_button.encode(submit_rect, ds, text, up, &self.submit_state, b);

        let validation_label = elements::Label {
            text: alloc::format!(
                "Username: {:?}  Password: {:?}",
                self.username_input.validation(),
                self.password_input.validation()
            ),
            color: gfx::Color::rgba(0.28, 0.32, 0.42, 1.0),
            align: elements::Align::Left,
            wrap: false,
            font_id: 0,
            font_px: 13.0,
        };
        let validation_rect = self.form_layout.rect(self.form_layout.nodes.validation).unwrap_or(
            gfx::RectF::new(submit_rect.x, submit_rect.y + 54.0, submit_rect.w.max(160.0), 20.0),
        );
        validation_label.encode(validation_rect, ds, text, up, b);

        let picker_alpha = self.node_opacity(self.picker_node);
        let picker_base = self.last_picker_rect;
        let picker_offset = self.offset_for_node(self.picker_node);
        let picker_panel = gfx::RectF::new(
            picker_base.x - 12.0 + picker_offset[0],
            picker_base.y - 16.0 + picker_offset[1],
            picker_base.w + 24.0,
            picker_base.h + 32.0,
        );
        b.rrect(picker_panel, [12.0; 4], gfx::Color::rgba(0.90, 0.94, 1.0, 0.55));
        let picker_rect = self.rect_for_node(picker_base, self.picker_node);
        let mut picker_style = self.picker_style;
        picker_style.highlight = gfx::Color::rgba(0.82, 0.91, 1.0, 0.45 * picker_alpha);
        picker_style.text_color = gfx::Color::rgba(
            picker_style.text_color.r,
            picker_style.text_color.g,
            picker_style.text_color.b,
            picker_alpha,
        );
        self.picker.encode(&picker_style, picker_rect, ds, text, up, b);

        let picker_title = elements::Label {
            text: alloc::string::String::from("Role Picker"),
            color: gfx::Color::rgba(0.18, 0.22, 0.32, picker_alpha.max(0.1)),
            align: elements::Align::Left,
            wrap: false,
            font_id: 0,
            font_px: 14.0,
        };
        let picker_title_rect = gfx::RectF::new(
            picker_panel.x + 12.0,
            picker_panel.y + 8.0,
            picker_panel.w - 24.0,
            20.0,
        );
        picker_title.encode(picker_title_rect, ds, text, up, b);

        let logs_rect =
            self.form_layout.rect(self.form_layout.nodes.logs).unwrap_or(gfx::RectF::new(
                vp.x + 18.0,
                submit_rect.y + 90.0,
                vp.w - 36.0,
                vp.h - (submit_rect.y + 110.0),
            ));
        b.rrect(logs_rect, [10.0; 4], gfx::Color::rgba(1.0, 1.0, 1.0, 0.65));
        let mut log_text = alloc::string::String::from("Event Log:");
        if self.logs.is_empty() {
            log_text.push_str("\n- none yet");
        } else {
            for entry in self.logs.iter().rev() {
                log_text.push_str("\n- ");
                log_text.push_str(entry);
            }
        }
        let log_label = elements::Label {
            text: log_text,
            color: gfx::Color::rgba(0.16, 0.18, 0.24, 1.0),
            align: elements::Align::Left,
            wrap: true,
            font_id: 0,
            font_px: 13.0,
        };
        let logs_text_rect = gfx::RectF::new(
            logs_rect.x + 12.0,
            logs_rect.y + 12.0,
            logs_rect.w - 24.0,
            logs_rect.h - 24.0,
        );
        log_label.encode(logs_text_rect, ds, text, up, b);

        if self.overlay_view.encode(&self.overlay, vp, ds, b) {
            let popup_w = vp.w.min(420.0);
            let popup_h = 160.0;
            let base_popup = gfx::RectF::new(
                vp.x + (vp.w - popup_w) * 0.5,
                vp.y + (vp.h - popup_h) * 0.5,
                popup_w,
                popup_h,
            );
            let scale = anim::helpers::shrink_grow_scale(self.overlay.progress(), 0.85, 1.08);
            let cx = base_popup.x + base_popup.w * 0.5;
            let cy = base_popup.y + base_popup.h * 0.5;
            let popup_rect = gfx::RectF::new(
                cx - base_popup.w * scale * 0.5,
                cy - base_popup.h * scale * 0.5,
                base_popup.w * scale,
                base_popup.h * scale,
            );
            self.popup.encode(popup_rect, ds, b);
            let overlay_label = elements::Label {
                text: self.overlay_message.clone(),
                color: gfx::Color::rgba(0.12, 0.14, 0.20, 1.0),
                align: elements::Align::Left,
                wrap: true,
                font_id: 0,
                font_px: 15.0,
            };
            overlay_label.encode(
                gfx::RectF::new(
                    popup_rect.x + 20.0,
                    popup_rect.y + 20.0,
                    popup_rect.w - 40.0,
                    popup_rect.h - 40.0,
                ),
                ds,
                text,
                up,
                b,
            );
        }
    }

    fn handle_submit(&mut self) {
        let user_valid =
            matches!(self.username_input.validation(), elements::TextValidation::Valid);
        let pass_valid =
            matches!(self.password_input.validation(), elements::TextValidation::Valid);
        let role = self.picker.selection_label().unwrap_or("None");
        if user_valid && pass_valid {
            self.overlay_message = alloc::format!(
                "Welcome {}! Assigned role: {}",
                Self::truncate(self.username_input.text(), 32),
                role
            );
            self.overlay.open();
            self.log("submission: success");
            if !self.animator.is_active_prop(self.submit_node, api::AnimProp::Transform2D) {
                let seq = anim::helpers::wiggle(anim::helpers::identity_transform(), 0.12, 2, 420);
                self.animator.start_sequence(self.submit_node, &seq);
            }
            if !self.animator.is_active_prop(self.picker_node, api::AnimProp::Transform2D) {
                let seq = anim::helpers::scatter(
                    anim::helpers::identity_transform(),
                    [46.0, -32.0],
                    480,
                    true,
                );
                self.animator.start_sequence(self.picker_node, &seq);
            }
        } else {
            let mut issues = alloc::vec![];
            if !user_valid {
                issues.push("username");
                if !self.animator.is_active_prop(self.username_node, api::AnimProp::Transform2D) {
                    let seq = anim::helpers::shake(
                        anim::helpers::identity_transform(),
                        anim::helpers::Axis2D::Horizontal,
                        9.0,
                        3,
                        420,
                    );
                    self.animator.start_sequence(self.username_node, &seq);
                }
            }
            if !pass_valid {
                issues.push("password");
                if !self.animator.is_active_prop(self.password_node, api::AnimProp::Transform2D) {
                    let seq = anim::helpers::shake(
                        anim::helpers::identity_transform(),
                        anim::helpers::Axis2D::Horizontal,
                        9.0,
                        3,
                        420,
                    );
                    self.animator.start_sequence(self.password_node, &seq);
                }
            }
            self.overlay_message = alloc::format!("Please fix: {}", issues.join(", "));
            self.overlay.open();
            self.log("submission: validation error");
        }
    }

    fn rect_for_node(&self, base: gfx::RectF, node: NodeId) -> gfx::RectF {
        let Some(over) = self.anim_overrides.get(&node) else {
            return base;
        };
        let Some(tr) = over.transform else {
            return base;
        };
        let sx = if tr.sx.is_finite() && tr.sx > 0.0 { tr.sx } else { 1.0 };
        let sy = if tr.sy.is_finite() && tr.sy > 0.0 { tr.sy } else { 1.0 };
        let cx = base.x + base.w * 0.5;
        let cy = base.y + base.h * 0.5;
        let w = base.w * sx;
        let h = base.h * sy;
        gfx::RectF::new(cx - w * 0.5 + tr.tx, cy - h * 0.5 + tr.ty, w, h)
    }

    fn offset_for_node(&self, node: NodeId) -> [f32; 2] {
        if let Some(over) = self.anim_overrides.get(&node) {
            if let Some(tr) = over.transform {
                return [tr.tx, tr.ty];
            }
        }
        [0.0, 0.0]
    }

    fn node_opacity(&self, node: NodeId) -> f32 {
        if let Some(over) = self.anim_overrides.get(&node) {
            if let Some(alpha) = over.opacity {
                return alpha.clamp(0.0, 1.0);
            }
        }
        1.0
    }

    fn focus(&mut self, field: FocusField) {
        self.focused = field;
        match field {
            FocusField::Username => {
                self.animator.cancel_prop(self.username_node, api::AnimProp::Transform2D);
                self.username_input.focus();
                self.password_input.blur();
                self.username_input.set_selection(0, 0);
                self.username_input.move_cursor_to_end();
            }
            FocusField::Password => {
                self.animator.cancel_prop(self.password_node, api::AnimProp::Transform2D);
                self.password_input.focus();
                self.username_input.blur();
                self.password_input.set_selection(0, 0);
                self.password_input.move_cursor_to_end();
            }
            FocusField::None => {
                self.username_input.blur();
                self.password_input.blur();
            }
        }
    }

    fn focused_mut(&mut self) -> Option<&mut elements::TextInputState> {
        match self.focused {
            FocusField::Username => Some(&mut self.username_input),
            FocusField::Password => Some(&mut self.password_input),
            FocusField::None => None,
        }
    }

    fn truncate<S: Into<alloc::string::String>>(value: S, max: usize) -> alloc::string::String {
        let input = value.into();
        let mut out = alloc::string::String::new();
        for (i, ch) in input.chars().enumerate() {
            if i >= max {
                out.push_str("...");
                break;
            }
            out.push(ch);
        }
        out
    }

    fn refresh_rect_cache(&mut self) {
        if let Some(rect) = self.form_layout.rect(self.form_layout.nodes.username) {
            self.last_username_rect = rect;
        }
        if let Some(rect) = self.form_layout.rect(self.form_layout.nodes.password) {
            self.last_password_rect = rect;
        }
        if let Some(rect) = self.form_layout.rect(self.form_layout.nodes.submit) {
            self.last_submit_rect = rect;
        }
        if let Some(rect) = self.form_layout.rect(self.form_layout.nodes.picker) {
            self.last_picker_rect = rect;
        }
    }
}

fn point_in_rect(pt: [f32; 2], rect: gfx::RectF) -> bool {
    pt[0] >= rect.x && pt[0] <= rect.x + rect.w && pt[1] >= rect.y && pt[1] <= rect.y + rect.h
}

// ---- Nine Slice Demo ----

pub struct NineSliceDemo {
    image: elements::NineSliceImage,
    slice_px: f32,
    alpha: f32,
    has_tex: bool,
}

impl Default for NineSliceDemo {
    fn default() -> Self {
        Self {
            image: elements::NineSliceImage {
                tex: gfx::ImageHandle(0),
                slice: gfx::Insets::new(16.0, 16.0, 16.0, 16.0),
                alpha: 1.0,
            },
            slice_px: 16.0,
            alpha: 1.0,
            has_tex: false,
        }
    }
}

impl NineSliceDemo {
    pub fn set_image(&mut self, tex: gfx::ImageHandle) {
        self.image.tex = tex;
        self.has_tex = tex.0 != 0;
    }

    pub fn set_options(&mut self, slice: f32, alpha: f32) {
        self.slice_px = slice.clamp(0.0, 64.0);
        self.alpha = alpha.clamp(0.0, 1.0);
        self.image.alpha = self.alpha;
        self.image.slice =
            gfx::Insets::new(self.slice_px, self.slice_px, self.slice_px, self.slice_px);
    }

    pub fn draw<U: elements::ImageUploader>(&self, vp: gfx::RectF, b: &mut DrawListBuilder) {
        if !self.has_tex {
            return;
        }
        let rect = gfx::RectF::new(vp.x + 40.0, vp.y + 40.0, vp.w - 80.0, vp.h - 80.0);
        self.image.encode(rect, b);
    }
}

// ---- SDF Text Demo ----

pub struct SdfTextDemo {
    pub font_px: f32,
    pub wrap: bool,
}

impl Default for SdfTextDemo {
    fn default() -> Self {
        Self { font_px: 28.0, wrap: true }
    }
}

impl SdfTextDemo {
    pub fn set_font_px(&mut self, px: f32) {
        self.font_px = px.clamp(10.0, 80.0);
    }

    pub fn draw<U: elements::ImageUploader>(
        &self,
        vp: gfx::RectF,
        ds: f32,
        text: &mut elements::TextCtx,
        up: &mut U,
        b: &mut DrawListBuilder,
    ) {
        let lbl = elements::Label {
            text: alloc::format!("SDF Demo\nFont Size: {:.1}", self.font_px),
            color: gfx::Color::rgba(0.1, 0.1, 0.1, 1.0),
            align: elements::Align::Left,
            wrap: self.wrap,
            font_id: 0,
            font_px: self.font_px,
        };
        lbl.encode(
            gfx::RectF::new(vp.x + 16.0, vp.y + 16.0, vp.w - 32.0, vp.h - 32.0),
            ds,
            text,
            up,
            b,
        );
    }
}

// ---- Readback Demo ----

#[derive(Default)]
pub struct ReadbackDemo {
    status: alloc::string::String,
}

impl ReadbackDemo {
    pub fn update(&mut self, _dt_ms: u32) {}

    pub fn set_status(&mut self, status: impl Into<alloc::string::String>) {
        self.status = status.into();
    }

    pub fn status(&self) -> &str {
        if self.status.is_empty() {
            "Tap snapshot to capture current frame"
        } else {
            &self.status
        }
    }

    pub fn draw<U: elements::ImageUploader>(
        &self,
        vp: gfx::RectF,
        ds: f32,
        text: &mut elements::TextCtx,
        up: &mut U,
        b: &mut DrawListBuilder,
    ) {
        let lbl = elements::Label {
            text: alloc::format!("Readback\n{}", self.status()),
            color: gfx::Color::rgba(0.1, 0.1, 0.1, 1.0),
            align: elements::Align::Left,
            wrap: true,
            font_id: 0,
            font_px: 16.0,
        };
        lbl.encode(
            gfx::RectF::new(vp.x + 16.0, vp.y + 16.0, vp.w - 32.0, vp.h - 32.0),
            ds,
            text,
            up,
            b,
        );
    }
}

impl DamageLab {
    pub fn update(&mut self, _dt_ms: u32) {}

    pub fn set_options(&mut self, enabled: bool, use_thresh: f32, prefilter: f32) {
        self.enabled = enabled;
        self.use_thresh = use_thresh.clamp(0.0, 1.0);
        self.prefilter = prefilter.clamp(0.0, 1.0);
    }

    pub fn draw<U: elements::ImageUploader>(
        &mut self,
        vp: gfx::RectF,
        ds: f32,
        text: &mut elements::TextCtx,
        up: &mut U,
        b: &mut DrawListBuilder,
        stats: DamageFrameStats,
    ) {
        b.rrect(vp, [0.0; 4], gfx::Color::rgba(0.96, 0.97, 0.99, 1.0));
        let info = alloc::format!(
            "Damage Lab\nEnabled: {}\nUse Threshold: {:.2}\nPrefilter: {:.2}\nLast Frame: {:.0}% damage, rects={}",
            if self.enabled { "On" } else { "Off" },
            self.use_thresh,
            self.prefilter,
            (stats.pct * 100.0).round(),
            stats.rects
        );
        let lbl = elements::Label {
            text: info,
            color: gfx::Color::rgba(0.1, 0.1, 0.1, 1.0),
            align: elements::Align::Left,
            wrap: true,
            font_id: 0,
            font_px: 16.0,
        };
        lbl.encode(
            gfx::RectF::new(vp.x + 16.0, vp.y + 16.0, vp.w - 32.0, vp.h - 32.0),
            ds,
            text,
            up,
            b,
        );
    }
}

// ---- Elements Extended ----

pub struct ElementsExtended {
    t: f32,
    // Badge
    badge: elements::Badge,
    badge_state: elements::BadgeState,
    badge_trigger_time: f32,
    // CountNode
    count_node: elements::CountNode,
    count_value: u32,
    // RecordButton
    record_button: elements::RecordButton,
    record_button_state: elements::RecordButtonState,
    // SlidingSwitch
    sliding_switch: elements::SlidingSwitch,
    sliding_switch_state: elements::SlidingSwitchState,
    // ShiftingTextInput
    shifting_input: elements::ShiftingTextInput,
    shifting_input_state: elements::ShiftingTextInputState,
}

impl Default for ElementsExtended {
    fn default() -> Self {
        // Create Badge with custom animation timing
        let badge_style = elements::BadgeStyle { bounce_duration_ms: 600, ..Default::default() };

        let badge = elements::Badge { image: LEGACY_BADGE_IMAGE, style: badge_style };

        // Create CountNode
        let count_node = elements::CountNode {
            count: 42,
            label: "items".into(),
            count_font_px: 24.0,
            label_font_px: 12.0,
            count_color: gfx::Color::rgba(0.2, 0.4, 0.8, 1.0),
            label_color: gfx::Color::rgba(0.5, 0.5, 0.5, 1.0),
        };

        // Create RecordButton with custom timings
        let record_style = elements::RecordButtonStyle {
            recording_timeout_ms: 5000,
            press_animation_ms: 150,
            ..Default::default()
        };

        let record_button = elements::RecordButton { style: record_style };

        // Create SlidingSwitch with custom timeout
        let switch_style =
            elements::SlidingSwitchStyle { inactive_timeout_ms: 3000, ..Default::default() };

        let sliding_switch = elements::SlidingSwitch { style: switch_style };
        let mut sliding_switch_state = elements::SlidingSwitchState::default();
        sliding_switch_state.start(&sliding_switch.style);

        // Create ShiftingTextInput
        let shifting_style = elements::ShiftingTextInputStyle::default();

        let shifting_input = elements::ShiftingTextInput {
            placeholder: "Enter text...".into(),
            prompt: Some("Type here:".into()),
            max_length: Some(50),
            filter: elements::CharFilter::None,
            style: shifting_style,
        };

        Self {
            t: 0.0,
            badge,
            badge_state: elements::BadgeState::default(),
            badge_trigger_time: 2.0, // Trigger bounce every 2 seconds
            count_node,
            count_value: 42,
            record_button,
            record_button_state: elements::RecordButtonState::default(),
            sliding_switch,
            sliding_switch_state,
            shifting_input,
            shifting_input_state: elements::ShiftingTextInputState::default(),
        }
    }
}

impl ElementsExtended {
    pub fn update(&mut self, dt_ms: u32) {
        self.t += dt_ms as f32 / 1000.0;

        // Trigger badge bounce periodically
        if (self.t % self.badge_trigger_time) < (dt_ms as f32 / 1000.0) {
            self.badge_state.bounce(&self.badge.style);
        }

        // Update count value with animation
        let new_count = (45.0 + (self.t * 0.5).sin() * 10.0) as u32;
        if new_count != self.count_value {
            self.count_value = new_count;
            self.count_node.count = new_count as u64;
        }

        // Update record button state - the timeout is handled internally
        // when we call on_pointer_up with recording active

        // Update sliding switch - the shared primitive emits the legacy one-shot timeout event.
        if self.sliding_switch_state.take_inactive() {
            self.sliding_switch_state.reset();
        }

        // Update shifting input animation
        self.shifting_input_state.tick();
    }

    pub fn input_pointer(&mut self, x: f32, y: f32, _dx: f32, _dy: f32, buttons: u32) {
        // Badge click area (triggers bounce)
        let badge_rect = gfx::RectF::new(50.0, 50.0, 80.0, 80.0);
        if buttons & 1 != 0 && point_in_rect([x, y], badge_rect) {
            self.badge_state.bounce(&self.badge.style);
        }

        // RecordButton interaction
        let record_rect = gfx::RectF::new(50.0, 150.0, 100.0, 100.0);
        let in_button = point_in_rect([x, y], record_rect);

        if buttons & 1 != 0 && in_button {
            self.record_button_state.on_pointer_down(&self.record_button.style);
        } else if buttons == 0 && in_button {
            // Button released inside
            if self.record_button_state.on_pointer_up(&self.record_button.style) {
                // Toggle recording
                if self.record_button_state.is_recording() {
                    self.record_button_state.stop_recording();
                } else {
                    self.record_button_state.start_recording(&self.record_button.style);
                }
            }
        } else if buttons == 0 {
            // Released outside - just release the press state
            let _ = self.record_button_state.on_pointer_up(&self.record_button.style);
        }

        // SlidingSwitch interaction - the shared primitive owns long-press gating and outside cancel.
        let switch_rect = gfx::RectF::new(50.0, 280.0, 200.0, 60.0);
        if buttons & 1 != 0 {
            if self.sliding_switch_state.mode == elements::SlidingSwitchMode::Idle {
                if self.sliding_switch_state.begin_drag([x, y], switch_rect) {
                    self.sliding_switch_state.start(&self.sliding_switch.style);
                }
            } else if self.sliding_switch_state.drag_to([x, y], switch_rect) {
                self.sliding_switch_state.reset();
            }
        } else {
            self.sliding_switch_state.end_drag();
        }

        // ShiftingTextInput focus
        let input_rect = gfx::RectF::new(50.0, 360.0, 300.0, 50.0);
        if buttons & 1 != 0 && point_in_rect([x, y], input_rect) {
            self.shifting_input_state.on_focus();
        } else if buttons & 1 != 0 {
            self.shifting_input_state.on_blur();
        }
    }

    pub fn draw<U: elements::ImageUploader>(
        &mut self,
        vp: gfx::RectF,
        ds: f32,
        text: &mut elements::TextCtx,
        up: &mut U,
        b: &mut DrawListBuilder,
    ) {
        // Background
        b.rrect(vp, [0.0; 4], gfx::Color::rgba(0.98, 0.98, 0.99, 1.0));

        // Title
        let title = elements::Label {
            text: "Extended Elements Showcase".into(),
            color: gfx::Color::rgba(0.1, 0.1, 0.1, 1.0),
            align: elements::Align::Center,
            wrap: false,
            font_id: 0,
            font_px: 20.0,
        };
        title.encode(gfx::RectF::new(vp.x, vp.y + 10.0, vp.w, 30.0), ds, text, up, b);

        // Badge
        let badge_rect = gfx::RectF::new(vp.x + 50.0, vp.y + 50.0, 80.0, 80.0);
        self.badge.encode(badge_rect, &self.badge_state, b);

        let badge_label = elements::Label {
            text: "Legacy Badge Overlay (click to bounce)".into(),
            color: gfx::Color::rgba(0.3, 0.3, 0.3, 1.0),
            align: elements::Align::Left,
            wrap: false,
            font_id: 0,
            font_px: 12.0,
        };
        badge_label.encode(
            gfx::RectF::new(vp.x + 140.0, vp.y + 75.0, 200.0, 20.0),
            ds,
            text,
            up,
            b,
        );

        // CountNode
        let count_rect = gfx::RectF::new(vp.x + 350.0, vp.y + 70.0, 200.0, 40.0);
        self.count_node.encode(count_rect, ds, text, up, b);

        // RecordButton
        let record_rect = gfx::RectF::new(vp.x + 50.0, vp.y + 150.0, 100.0, 100.0);
        self.record_button.encode(record_rect, &self.record_button_state, b);

        let record_label = elements::Label {
            text: if self.record_button_state.is_recording() {
                "Recording... (tap to stop)".into()
            } else {
                "Record Button (tap to start)".into()
            },
            color: gfx::Color::rgba(0.3, 0.3, 0.3, 1.0),
            align: elements::Align::Left,
            wrap: false,
            font_id: 0,
            font_px: 12.0,
        };
        record_label.encode(
            gfx::RectF::new(vp.x + 160.0, vp.y + 190.0, 250.0, 20.0),
            ds,
            text,
            up,
            b,
        );

        // SlidingSwitch
        let switch_rect = gfx::RectF::new(vp.x + 50.0, vp.y + 280.0, 200.0, 60.0);
        self.sliding_switch.encode(switch_rect, &self.sliding_switch_state, b);

        let switch_label = elements::Label {
            text: format!("Sliding Switch (state: {:?})", self.sliding_switch_state.mode),
            color: gfx::Color::rgba(0.3, 0.3, 0.3, 1.0),
            align: elements::Align::Left,
            wrap: false,
            font_id: 0,
            font_px: 12.0,
        };
        switch_label.encode(
            gfx::RectF::new(vp.x + 260.0, vp.y + 295.0, 300.0, 20.0),
            ds,
            text,
            up,
            b,
        );

        // ShiftingTextInput
        let input_rect = gfx::RectF::new(vp.x + 50.0, vp.y + 360.0, 300.0, 50.0);
        self.shifting_input.encode(&self.shifting_input_state, input_rect, ds, text, up, b);

        let input_label = elements::Label {
            text: "Shifting Text Input (tap to focus)".into(),
            color: gfx::Color::rgba(0.3, 0.3, 0.3, 1.0),
            align: elements::Align::Left,
            wrap: false,
            font_id: 0,
            font_px: 12.0,
        };
        input_label.encode(
            gfx::RectF::new(vp.x + 360.0, vp.y + 375.0, 250.0, 20.0),
            ds,
            text,
            up,
            b,
        );

        // Animation timing info
        let info_text = format!(
            "Custom Timings - Badge: {}ms, Record: {}ms/{}ms, Switch: {}ms",
            self.badge.style.bounce_duration_ms,
            self.record_button.style.press_animation_ms,
            self.record_button.style.recording_timeout_ms,
            self.sliding_switch.style.inactive_timeout_ms
        );
        let info_label = elements::Label {
            text: info_text,
            color: gfx::Color::rgba(0.5, 0.5, 0.5, 1.0),
            align: elements::Align::Center,
            wrap: false,
            font_id: 0,
            font_px: 11.0,
        };
        info_label.encode(gfx::RectF::new(vp.x, vp.y + vp.h - 30.0, vp.w, 20.0), ds, text, up, b);
    }
}

// Use the existing point_in_rect function defined earlier in the file

struct GridMeasure;
impl collection::Measure for GridMeasure {
    fn measure(&mut self, _i: usize, cw: f32) -> f32 {
        cw * 0.6
    }
}

struct GridRenderer;
impl collection::CellRenderer for GridRenderer {
    fn render(
        &mut self,
        _id: u32,
        idx: usize,
        rect: gfx::RectF,
        focused: bool,
        hovered: bool,
        b: &mut DrawListBuilder,
    ) {
        let base = 0.9 - ((idx % 5) as f32) * 0.05;
        let mut c = gfx::Color::rgba(base, base, base, 1.0);
        if focused {
            c = gfx::Color::rgba(0.2, 0.6, 1.0, 1.0);
        } else if hovered {
            c = gfx::Color::rgba(0.75, 0.8, 0.9, 1.0);
        }
        b.rrect(rect, [4.0; 4], c);
    }
}
pub struct CameraDemo {
    t: f32,
    blur: bool,
    sigma: f32,
    grayscale: bool,
    animate: bool,
    view: elements::UICameraView,
    metrics: CameraMetrics,
    recording: bool,
    last_message: Option<alloc::string::String>,
    message_timer: f32,
    manager: Option<Arc<dyn api::CameraManager + Send + Sync>>,
    session: Option<CameraController>,
    preview: CameraPreviewNode,
    cropper: Option<CropperState>,
    pending_frame_size: Option<(u32, u32)>,
    volume: VolumeHudState,
    playback_phase: f32,
    active: bool,
    preview_failed: bool,
    permission_ui: PermissionOverlayUi,
    permission_manager: Option<Arc<PermissionManager>>,
}

impl Default for CameraDemo {
    fn default() -> Self {
        Self {
            t: 0.0,
            blur: false,
            sigma: 0.0,
            grayscale: false,
            animate: false,
            view: elements::UICameraView::default(),
            metrics: CameraMetrics::default(),
            recording: false,
            last_message: None,
            message_timer: 0.0,
            manager: None,
            session: None,
            preview: CameraPreviewNode::new(),
            cropper: None,
            pending_frame_size: None,
            volume: VolumeHudState::new(900),
            playback_phase: 0.0,
            active: false,
            preview_failed: false,
            permission_ui: PermissionOverlayUi::default(),
            permission_manager: None,
        }
    }
}

impl CameraDemo {
    pub fn attach_manager(&mut self, manager: Arc<dyn api::CameraManager + Send + Sync>) {
        let session = CameraController::new(Arc::clone(&manager));
        self.manager = Some(manager);
        self.session = Some(session);
        self.preview_failed = false;
    }

    pub fn detach_manager(&mut self) {
        if let Some(session) = &self.session {
            session.stop_preview();
        }
        self.session = None;
        self.manager = None;
        self.preview_failed = false;
    }

    pub fn bind_permission_manager(&mut self, manager: &Arc<PermissionManager>) {
        self.permission_manager = Some(Arc::clone(manager));
    }

    pub fn update_permissions(&mut self, states: &[PermissionState]) {
        self.permission_ui.update(states);
    }

    pub fn set_active(&mut self, active: bool) {
        if self.active == active {
            return;
        }
        self.active = active;
        if active {
            self.preview_failed = false;
            self.pending_frame_size = None;
        } else if let Some(session) = &self.session {
            session.stop_preview();
        }
    }

    pub fn show_volume(&mut self, level: f32) {
        self.volume.show(level);
    }

    pub fn input_pointer(&mut self, x: f32, y: f32, dx: f32, dy: f32, buttons: u32) {
        if let Some(domain) = self.permission_ui.pointer_event(x, y, buttons) {
            if buttons == 0 {
                if let Some(manager) = &self.permission_manager {
                    manager.request(domain);
                }
            }
        }
        if buttons & 1 != 0 {
            if self.permission_ui.contains(x, y) {
                return;
            }
            if let Some(crop) = self.cropper.as_mut() {
                let inv_zoom = if crop.zoom() > 0.0 { 1.0 / crop.zoom() } else { 1.0 };
                crop.pan(-dx * inv_zoom, -dy * inv_zoom);
            }
        }
    }

    pub fn pinch(&mut self, delta: f32) {
        if let Some(crop) = self.cropper.as_mut() {
            let scale = 2.0_f32.powf(delta.clamp(-1.5, 1.5));
            crop.set_zoom(crop.zoom() * scale);
        }
    }

    pub fn double_tap(&mut self) {
        if let Some(crop) = self.cropper.as_mut() {
            crop.reset();
        }
    }

    pub fn update(&mut self, dt_ms: u32) {
        let dt = dt_ms as f32 / 1000.0;
        self.t += dt;
        if self.animate {
            let phase = (self.t * 1.5).sin() * 0.5 + 0.5;
            self.sigma = 2.0 + phase * 10.0;
        }
        if self.volume.is_visible() {
            self.volume.tick(dt_ms);
        }
        if self.last_message.is_some() {
            self.update_message_timer(dt);
        }

        if self.active {
            self.ensure_session();
            self.poll_session_events();
            if self.recording {
                self.playback_phase = (self.playback_phase + dt * 0.2).fract();
            } else {
                self.playback_phase = 0.0;
            }
        } else {
            self.playback_phase = 0.0;
        }

        self.view.blur = self.blur;
        self.view.sigma = self.sigma;
        self.view.grayscale = self.grayscale;
        self.view.alpha = 1.0;
        self.view.tint = gfx::Color::rgba(1.0, 1.0, 1.0, 1.0);
    }

    pub fn draw<U: elements::ImageUploader>(
        &mut self,
        vp: gfx::RectF,
        ds: f32,
        text: &mut elements::TextCtx,
        up: &mut U,
        b: &mut DrawListBuilder,
    ) {
        if self.is_plain_preview() {
            self.view.encode(vp, b);
            return;
        }
        b.rrect(vp, [0.0; 4], gfx::Color::rgba(0.98, 0.98, 0.98, 1.0));
        self.preview.layout(vp);
        let rect = self.preview.rect();
        self.sync_cropper(rect);
        self.view.encode(rect, b);
        self.draw_crop_overlay(rect, b);
        self.permission_ui.draw(vp, ds, text, up, b);
        self.draw_volume_hud(rect, b);
        self.draw_playback_timeline(rect, b);
        self.draw_message(rect, ds, text, up, b);
    }

    pub fn set_options(&mut self, blur: bool, sigma: f32, grayscale: bool, animate: bool) {
        self.blur = blur;
        self.sigma = sigma.max(0.0);
        self.grayscale = grayscale;
        self.animate = animate;
    }

    pub fn sigma(&self) -> f32 {
        self.sigma
    }

    pub fn blur(&self) -> bool {
        self.blur
    }

    pub fn grayscale(&self) -> bool {
        self.grayscale
    }

    pub fn set_metrics(&mut self, metrics: CameraMetrics) {
        self.metrics = metrics;
    }

    pub fn set_recording(&mut self, recording: bool) {
        self.recording = recording;
        if recording {
            self.last_message = None;
            self.message_timer = 0.0;
        }
    }

    pub fn on_record_event(&mut self, event: CameraRecordingUiEvent) {
        self.recording = false;
        self.message_timer = 5.0;
        self.last_message = Some(match event {
            CameraRecordingUiEvent::Completed { path, duration_ns, size_bytes, had_audio } => {
                let name = path.rsplit('/').next().unwrap_or(&path);
                let seconds = (duration_ns as f64 / 1_000_000_000f64).max(0.0);
                let size = Self::format_size(size_bytes);
                alloc::format!(
                    "saved {} ({:.1}s, {}, audio={})",
                    name,
                    seconds,
                    size,
                    Self::bool_str(had_audio)
                )
            }
            CameraRecordingUiEvent::Cancelled => alloc::string::String::from("capture cancelled"),
            CameraRecordingUiEvent::Failed { message } => alloc::format!("failed: {}", message),
        });
    }

    pub fn overlay_line(&self) -> alloc::string::String {
        let metrics = &self.metrics;
        let dims = if metrics.width > 0 && metrics.height > 0 {
            alloc::format!("{}x{}", metrics.width, metrics.height)
        } else {
            alloc::string::String::from("0x0")
        };
        let coverage = (metrics.coverage_pct * 100.0).clamp(0.0, 100.0);
        let paused = metrics.paused || !metrics.running;
        let rec_flag = if self.recording { "on" } else { "off" };
        let mut line = alloc::format!(
            " cam sigma={:.1} blur={} gray={} {} bd={} mx={} rng={} cov={:.0}% fps={:.1} paused={} lp={} th={} rec={}",
            self.sigma(),
            Self::bool_str(self.blur),
            Self::bool_str(self.grayscale),
            dims,
            metrics.bit_depth,
            Self::matrix_label(metrics.matrix),
            Self::range_label(metrics.video_range),
            coverage,
            metrics.fps,
            Self::bool_str(paused),
            Self::bool_str(metrics.low_power),
            metrics.thermal,
            rec_flag
        );
        if let Some(crop) = &self.cropper {
            line.push_str(" zoom=");
            line.push_str(&alloc::format!("{:.2}", crop.zoom()));
        }
        if let Some(msg) = &self.last_message {
            line.push_str(" | ");
            line.push_str(msg);
        }
        line
    }

    fn ensure_session(&mut self) {
        if self.manager.is_none() {
            return;
        }
        if self.session.is_none() {
            if let Some(manager) = &self.manager {
                self.session = Some(CameraController::new(Arc::clone(manager)));
            }
        }
        if self.preview_failed {
            return;
        }
        if let Some(session) = &self.session {
            match session.mode() {
                CameraMode::Idle => {
                    let cfg = session.config();
                    if let Err(err) = session.start_preview(cfg) {
                        self.preview_failed = true;
                        self.last_message = Some(alloc::format!("preview failed: {}", err));
                        self.message_timer = 4.0;
                    }
                }
                _ => self.preview_failed = false,
            }
        }
    }

    fn poll_session_events(&mut self) {
        if let Some(session) = &self.session {
            for event in session.poll_events() {
                match event {
                    CameraEvent::Frame(frame) => self.on_frame(frame),
                    CameraEvent::Audio(sample) => self.on_audio(sample),
                    CameraEvent::Recording(evt) => self.on_record_event(recording_event_to_ui(evt)),
                }
            }
        }
    }

    fn on_frame(&mut self, frame: api::CameraFrame) {
        self.metrics.width = frame.size.0;
        self.metrics.height = frame.size.1;
        self.metrics.running = true;
        self.metrics.paused = false;
        self.pending_frame_size = Some(frame.size);
    }

    fn on_audio(&mut self, sample: api::AudioSample) {
        let level = Self::audio_level(&sample);
        self.volume.show(level);
    }

    fn update_message_timer(&mut self, dt: f32) {
        if self.message_timer > 0.0 {
            self.message_timer = (self.message_timer - dt).max(0.0);
            if self.message_timer <= f32::EPSILON {
                self.last_message = None;
            }
        }
    }

    fn sync_cropper(&mut self, rect: gfx::RectF) {
        if rect.w <= 0.0 || rect.h <= 0.0 {
            return;
        }
        if let Some(crop) = self.cropper.as_mut() {
            if let Some(size) = self.pending_frame_size.take() {
                let content = (size.0 as f32, size.1 as f32);
                crop.set_content_size(content);
            }
            crop.set_view_size((rect.w, rect.h));
        } else {
            self.pending_frame_size = None;
        }
    }

    fn crop_rect(&self, rect: gfx::RectF) -> Option<gfx::RectF> {
        let crop = self.cropper.as_ref()?;
        let content = crop.content_size();
        if content.0 <= 0.0 || content.1 <= 0.0 {
            return None;
        }
        let visible = crop.visible_rect();
        let scale_x = rect.w / content.0;
        let scale_y = rect.h / content.1;
        Some(gfx::RectF::new(
            rect.x + visible.x * scale_x,
            rect.y + visible.y * scale_y,
            visible.w * scale_x,
            visible.h * scale_y,
        ))
    }

    fn draw_crop_overlay(&self, rect: gfx::RectF, b: &mut DrawListBuilder) {
        if let Some(crop_rect) = self.crop_rect(rect) {
            if crop_rect.w <= 0.0 || crop_rect.h <= 0.0 {
                return;
            }
            let overlay = gfx::Color::rgba(0.0, 0.0, 0.0, 0.32);
            if crop_rect.y > rect.y {
                b.rrect(
                    gfx::RectF::new(rect.x, rect.y, rect.w, crop_rect.y - rect.y),
                    [0.0; 4],
                    overlay,
                );
            }
            let bottom_y = crop_rect.y + crop_rect.h;
            let rect_bottom = rect.y + rect.h;
            if bottom_y < rect_bottom {
                b.rrect(
                    gfx::RectF::new(rect.x, bottom_y, rect.w, rect_bottom - bottom_y),
                    [0.0; 4],
                    overlay,
                );
            }
            if crop_rect.x > rect.x {
                b.rrect(
                    gfx::RectF::new(rect.x, crop_rect.y, crop_rect.x - rect.x, crop_rect.h),
                    [0.0; 4],
                    overlay,
                );
            }
            let right_x = crop_rect.x + crop_rect.w;
            let rect_right = rect.x + rect.w;
            if right_x < rect_right {
                b.rrect(
                    gfx::RectF::new(right_x, crop_rect.y, rect_right - right_x, crop_rect.h),
                    [0.0; 4],
                    overlay,
                );
            }
            let radius = self.preview.corner_radius();
            b.rrect(crop_rect, [radius; 4], gfx::Color::rgba(0.95, 0.82, 0.30, 0.65));
            let line_color = gfx::Color::rgba(1.0, 1.0, 1.0, 0.55);
            let thickness = 1.5;
            let cx = crop_rect.x + crop_rect.w * 0.5;
            let cy = crop_rect.y + crop_rect.h * 0.5;
            b.rrect(
                gfx::RectF::new(crop_rect.x, cy - thickness * 0.5, crop_rect.w, thickness),
                [0.0; 4],
                line_color,
            );
            b.rrect(
                gfx::RectF::new(cx - thickness * 0.5, crop_rect.y, thickness, crop_rect.h),
                [0.0; 4],
                line_color,
            );
        }
    }

    fn draw_playback_timeline(&self, rect: gfx::RectF, b: &mut DrawListBuilder) {
        if !self.recording {
            return;
        }
        let track = gfx::RectF::new(rect.x, rect.y + rect.h + 12.0, rect.w, 6.0);
        b.rrect(track, [3.0; 4], gfx::Color::rgba(0.20, 0.22, 0.26, 0.28));
        let knob_x = track.x + track.w * self.playback_phase;
        let knob = gfx::RectF::new(knob_x - 8.0, track.y - 4.0, 16.0, 14.0);
        b.rrect(knob, [6.0; 4], gfx::Color::rgba(0.95, 0.36, 0.45, 0.9));
    }

    fn is_plain_preview(&self) -> bool {
        !self.preview_failed
            && !self.permission_ui.is_visible()
            && self.cropper.is_none()
            && !self.volume.is_visible()
            && !self.recording
            && self.last_message.is_none()
    }

    fn draw_volume_hud(&self, rect: gfx::RectF, b: &mut DrawListBuilder) {
        if !self.volume.is_visible() {
            return;
        }
        let hud = gfx::RectF::new(rect.x + 24.0, rect.y + 24.0, 156.0, 36.0);
        b.backdrop(hud, 9.0, gfx::Color::rgba(0.15, 0.15, 0.18, 1.0), 0.55);
        b.rrect(hud, [12.0; 4], gfx::Color::rgba(0.08, 0.08, 0.10, 0.78));
        let bar = gfx::RectF::new(hud.x + 12.0, hud.y + 12.0, hud.w - 24.0, 12.0);
        b.rrect(bar, [6.0; 4], gfx::Color::rgba(1.0, 1.0, 1.0, 0.22));
        let fill = (bar.w * self.volume.level().clamp(0.0, 1.0)).max(0.0);
        if fill > 0.0 {
            b.rrect(
                gfx::RectF::new(bar.x, bar.y, fill, bar.h),
                [6.0; 4],
                gfx::Color::rgba(0.85, 0.32, 0.42, 0.95),
            );
        }
    }

    fn draw_message<U: elements::ImageUploader>(
        &self,
        rect: gfx::RectF,
        ds: f32,
        text: &mut elements::TextCtx,
        up: &mut U,
        b: &mut DrawListBuilder,
    ) {
        let Some(msg) = &self.last_message else { return };
        let panel = gfx::RectF::new(rect.x + 24.0, rect.y + rect.h - 52.0, rect.w - 48.0, 32.0);
        b.backdrop(panel, 8.0, gfx::Color::rgba(0.0, 0.0, 0.0, 1.0), 0.55);
        b.rrect(panel, [8.0; 4], gfx::Color::rgba(0.08, 0.08, 0.10, 0.82));
        let label = elements::Label {
            text: msg.clone(),
            color: gfx::Color::rgba(1.0, 1.0, 1.0, 0.88),
            align: elements::Align::Left,
            wrap: false,
            font_id: 0,
            font_px: 12.0,
        };
        label.encode(
            gfx::RectF::new(panel.x + 12.0, panel.y + 8.0, panel.w - 24.0, panel.h - 16.0),
            ds,
            text,
            up,
            b,
        );
    }

    fn audio_level(sample: &api::AudioSample) -> f32 {
        if sample.data.is_empty() {
            return 0.0;
        }
        let mut sum = 0.0f64;
        for &s in &sample.data {
            let v = (s as f64) / (i16::MAX as f64);
            sum += v * v;
        }
        let rms = (sum / (sample.data.len() as f64)).sqrt();
        (rms as f32).clamp(0.0, 1.0)
    }

    fn bool_str(value: bool) -> &'static str {
        if value {
            "yes"
        } else {
            "no"
        }
    }

    fn matrix_label(code: u8) -> &'static str {
        match code {
            1 => "601",
            2 => "2020",
            _ => "709",
        }
    }

    fn range_label(code: u8) -> &'static str {
        if code == 0 {
            "full"
        } else {
            "video"
        }
    }

    fn format_size(bytes: u64) -> alloc::string::String {
        const KB: f64 = 1024.0;
        const MB: f64 = KB * 1024.0;
        const GB: f64 = MB * 1024.0;
        let b = bytes as f64;
        if b >= GB {
            alloc::format!("{:.2} GB", b / GB)
        } else if b >= MB {
            alloc::format!("{:.1} MB", b / MB)
        } else if b >= KB {
            alloc::format!("{:.1} KB", b / KB)
        } else {
            alloc::format!("{} B", bytes)
        }
    }
}
