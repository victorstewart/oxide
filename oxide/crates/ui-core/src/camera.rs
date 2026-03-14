//! Camera coordination utilities: preview/recording controller, cropper state, volume HUD logic.

use alloc::{string::String, vec::Vec};
use oxide_platform_api::{
    AudioSample, CameraConfig, CameraFrame, CameraManager, CameraRecording, CameraStream,
    CaptureMode, PlatformError, RecordingEvent, RecordingOptions,
};
use oxide_renderer_api as gfx;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CameraMode {
    Idle,
    Previewing,
    Recording,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraMetrics {
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub matrix: u8,
    pub video_range: u8,
    pub color_space: u8,
    pub coverage_pct: f32,
    pub blur_ms: f32,
    pub blur_updates: u32,
    pub update_period_ms: u32,
    pub paused: bool,
    pub running: bool,
    pub low_power: bool,
    pub thermal: u8,
    pub fps: f32,
}

impl Default for CameraMetrics {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
            bit_depth: 0,
            matrix: 0,
            video_range: 0,
            color_space: 0,
            coverage_pct: 0.0,
            blur_ms: 0.0,
            blur_updates: 0,
            update_period_ms: 0,
            paused: false,
            running: false,
            low_power: false,
            thermal: 0,
            fps: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CameraRecordingUiEvent {
    Completed { path: String, duration_ns: u64, size_bytes: u64, had_audio: bool },
    Cancelled,
    Failed { message: String },
}

#[derive(Clone, Debug)]
pub enum CameraEvent {
    Frame(CameraFrame),
    Audio(AudioSample),
    Recording(RecordingEvent),
}

struct CameraInner {
    config: CameraConfig,
    mode: CameraMode,
    stream: Option<Box<dyn CameraStream + Send>>,
    recording: Option<Box<dyn CameraRecording + Send>>,
    events: Vec<CameraEvent>,
    metrics: CameraMetrics,
    last_frame_ts_ns: Option<u64>,
    frame_counter: u64,
}

impl CameraInner {
    fn new(config: CameraConfig) -> Self {
        Self {
            config,
            mode: CameraMode::Idle,
            stream: None,
            recording: None,
            events: Vec::new(),
            metrics: CameraMetrics::default(),
            last_frame_ts_ns: None,
            frame_counter: 0,
        }
    }
}

pub struct CameraController {
    manager: Arc<dyn CameraManager + Send + Sync>,
    inner: Arc<Mutex<CameraInner>>,
}

pub trait CameraSession {
    fn config(&self) -> CameraConfig;
    fn mode(&self) -> CameraMode;
    fn update_config(&self, cfg: CameraConfig);
    fn start_preview(&self, cfg: CameraConfig) -> Result<(), PlatformError>;
    fn stop_preview(&self);
    fn start_recording(&self, options: RecordingOptions) -> Result<(), PlatformError>;
    fn stop_recording(&self);
    fn cancel_recording(&self);
    fn poll_events(&self) -> Vec<CameraEvent>;
    fn metrics(&self) -> CameraMetrics;
}

impl CameraController {
    pub fn new(manager: Arc<dyn CameraManager + Send + Sync>) -> Self {
        let inner = CameraInner::new(CameraConfig::default());
        Self { manager, inner: Arc::new(Mutex::new(inner)) }
    }

    pub fn config(&self) -> CameraConfig {
        self.inner.lock().unwrap().config
    }

    pub fn mode(&self) -> CameraMode {
        self.inner.lock().unwrap().mode
    }

    pub fn update_config(&self, cfg: CameraConfig) {
        let mut guard = self.inner.lock().unwrap();
        guard.config = cfg;
    }

    pub fn start_preview(&self, cfg: CameraConfig) -> Result<(), PlatformError> {
        let mut guard = self.inner.lock().unwrap();
        if matches!(guard.mode, CameraMode::Previewing | CameraMode::Recording) {
            guard.config = cfg;
            drop(guard);
            self.manager.set_mode(CaptureMode::Preview);
            self.manager.set_resolution(cfg.resolution.0, cfg.resolution.1);
            self.manager.set_fps(cfg.fps);
            self.manager.select_device(cfg.device);
            return Ok(());
        }

        let shared = self.inner.clone();
        let frame_cb = move |frame: CameraFrame| {
            let mut inner = shared.lock().unwrap();
            inner.frame_counter = inner.frame_counter.saturating_add(1);
            inner.metrics.width = frame.size.0;
            inner.metrics.height = frame.size.1;
            inner.metrics.bit_depth = match frame.image {
                oxide_platform_api::CameraImage::Nv12 {
                    bit_depth, matrix, video_range, ..
                } => {
                    inner.metrics.matrix = matrix;
                    inner.metrics.video_range = video_range;
                    bit_depth
                }
                oxide_platform_api::CameraImage::Gpu { .. } => inner.metrics.bit_depth,
            };
            inner.metrics.color_space = 0;
            inner.metrics.running = true;
            inner.metrics.coverage_pct = 1.0;
            if let Some(last) = inner.last_frame_ts_ns {
                if frame.timestamp_ns > last {
                    let delta = frame.timestamp_ns - last;
                    if delta > 0 {
                        inner.metrics.fps = 1_000_000_000f32 / (delta as f32);
                        inner.metrics.update_period_ms = (delta / 1_000_000) as u32;
                    }
                }
            }
            inner.last_frame_ts_ns = Some(frame.timestamp_ns);
            inner.events.push(CameraEvent::Frame(frame));
        };

        let audio_shared = self.inner.clone();
        let audio_cb = move |sample: AudioSample| {
            let mut inner = audio_shared.lock().unwrap();
            inner.events.push(CameraEvent::Audio(sample));
        };

        let filter_audio = matches!(cfg.capture, CaptureMode::Video);
        let stream = self.manager.start_stream(
            cfg,
            Box::new(frame_cb),
            if filter_audio { Some(Box::new(audio_cb)) } else { None },
        )?;

        guard.config = cfg;
        guard.stream = Some(stream);
        guard.mode = CameraMode::Previewing;
        guard.metrics.running = true;
        guard.metrics.paused = false;
        Ok(())
    }

    pub fn stop_preview(&self) {
        let mut guard = self.inner.lock().unwrap();
        if let Some(stream) = guard.stream.take() {
            stream.stop();
        }
        guard.mode = CameraMode::Idle;
        guard.metrics.running = false;
    }

    pub fn start_recording(&self, options: RecordingOptions) -> Result<(), PlatformError> {
        let shared = self.inner.clone();
        let record_cb = move |event: RecordingEvent| {
            let mut inner = shared.lock().unwrap();
            inner.events.push(CameraEvent::Recording(event));
        };

        let recording = self.manager.start_recording(options.clone(), Box::new(record_cb))?;
        let mut guard = self.inner.lock().unwrap();
        guard.recording = Some(recording);
        guard.mode = CameraMode::Recording;
        guard.metrics.running = true;
        guard.metrics.paused = false;
        guard.metrics.blur_updates = guard.metrics.blur_updates.saturating_add(1);
        guard.metrics.blur_ms = options.include_audio.then_some(0.0).unwrap_or(0.0);
        Ok(())
    }

    pub fn stop_recording(&self) {
        let mut guard = self.inner.lock().unwrap();
        if let Some(rec) = guard.recording.take() {
            rec.stop();
        }
        guard.mode = CameraMode::Previewing;
    }

    pub fn cancel_recording(&self) {
        let mut guard = self.inner.lock().unwrap();
        if let Some(rec) = guard.recording.take() {
            rec.cancel();
        }
        guard.mode = CameraMode::Previewing;
    }

    pub fn poll_events(&self) -> Vec<CameraEvent> {
        let mut guard = self.inner.lock().unwrap();
        let events = core::mem::take(&mut guard.events);
        events
    }

    pub fn metrics(&self) -> CameraMetrics {
        self.inner.lock().unwrap().metrics
    }
}

impl CameraSession for CameraController {
    fn config(&self) -> CameraConfig {
        CameraController::config(self)
    }

    fn mode(&self) -> CameraMode {
        CameraController::mode(self)
    }

    fn update_config(&self, cfg: CameraConfig) {
        CameraController::update_config(self, cfg);
    }

    fn start_preview(&self, cfg: CameraConfig) -> Result<(), PlatformError> {
        CameraController::start_preview(self, cfg)
    }

    fn stop_preview(&self) {
        CameraController::stop_preview(self);
    }

    fn start_recording(&self, options: RecordingOptions) -> Result<(), PlatformError> {
        CameraController::start_recording(self, options)
    }

    fn stop_recording(&self) {
        CameraController::stop_recording(self);
    }

    fn cancel_recording(&self) {
        CameraController::cancel_recording(self);
    }

    fn poll_events(&self) -> Vec<CameraEvent> {
        CameraController::poll_events(self)
    }

    fn metrics(&self) -> CameraMetrics {
        CameraController::metrics(self)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CameraPreviewNode {
    rect: gfx::RectF,
    padding: f32,
    corner_radius: f32,
}

impl CameraPreviewNode {
    pub fn new() -> Self {
        Self { rect: gfx::RectF::new(0.0, 0.0, 0.0, 0.0), padding: 16.0, corner_radius: 18.0 }
    }

    pub fn layout(&mut self, viewport: gfx::RectF) {
        let pad = self.padding.max(0.0);
        let w = (viewport.w - pad * 2.0).max(0.0);
        let h = (viewport.h - pad * 2.0).max(0.0);
        self.rect = gfx::RectF::new(viewport.x + pad, viewport.y + pad, w, h);
    }

    pub fn rect(&self) -> gfx::RectF {
        self.rect
    }

    pub fn corner_radius(&self) -> f32 {
        self.corner_radius
    }

    pub fn set_padding(&mut self, padding: f32) {
        self.padding = padding.max(0.0);
    }

    pub fn set_corner_radius(&mut self, radius: f32) {
        self.corner_radius = radius.max(0.0);
    }
}

#[derive(Clone, Copy, Debug)]
pub struct VolumeHudState {
    level: f32,
    fade_ms: u32,
    remaining_ms: u32,
}

impl VolumeHudState {
    pub fn new(fade_ms: u32) -> Self {
        Self { level: 0.0, fade_ms, remaining_ms: 0 }
    }

    pub fn show(&mut self, level: f32) {
        self.level = level.clamp(0.0, 1.0);
        self.remaining_ms = self.fade_ms;
    }

    pub fn tick(&mut self, dt_ms: u32) {
        if self.remaining_ms > 0 {
            self.remaining_ms = self.remaining_ms.saturating_sub(dt_ms);
        }
    }

    pub fn is_visible(&self) -> bool {
        self.remaining_ms > 0
    }

    pub fn level(&self) -> f32 {
        self.level
    }
}

#[derive(Clone, Debug)]
pub struct CropperState {
    content_size: (f32, f32),
    view_size: (f32, f32),
    zoom: f32,
    min_zoom: f32,
    max_zoom: f32,
    offset: [f32; 2],
}

impl CropperState {
    pub fn new(content_size: (f32, f32), view_size: (f32, f32)) -> Self {
        let min_zoom = Self::compute_min_zoom(content_size, view_size);
        Self {
            content_size,
            view_size,
            zoom: min_zoom,
            min_zoom,
            max_zoom: 4.0,
            offset: [0.0, 0.0],
        }
    }

    pub fn set_zoom_limits(&mut self, min_zoom: f32, max_zoom: f32) {
        self.min_zoom = min_zoom;
        self.max_zoom = max_zoom.max(min_zoom);
        self.zoom = self.zoom.clamp(self.min_zoom, self.max_zoom);
        self.clamp_offset();
    }

    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom.clamp(self.min_zoom, self.max_zoom);
        self.clamp_offset();
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.offset[0] += dx;
        self.offset[1] += dy;
        self.clamp_offset();
    }

    pub fn set_content_size(&mut self, content_size: (f32, f32)) {
        self.content_size = content_size;
        let min_zoom = Self::compute_min_zoom(self.content_size, self.view_size);
        self.min_zoom = min_zoom;
        self.max_zoom = self.max_zoom.max(min_zoom);
        self.zoom = self.zoom.clamp(self.min_zoom, self.max_zoom);
        self.clamp_offset();
    }

    pub fn set_view_size(&mut self, view_size: (f32, f32)) {
        self.view_size = view_size;
        let min_zoom = Self::compute_min_zoom(self.content_size, self.view_size);
        self.min_zoom = min_zoom;
        self.max_zoom = self.max_zoom.max(min_zoom);
        self.zoom = self.zoom.clamp(self.min_zoom, self.max_zoom);
        self.clamp_offset();
    }

    pub fn content_size(&self) -> (f32, f32) {
        self.content_size
    }

    pub fn view_size(&self) -> (f32, f32) {
        self.view_size
    }

    pub fn reset(&mut self) {
        self.zoom = self.min_zoom;
        self.offset = [0.0, 0.0];
    }

    pub fn visible_rect(&self) -> gfx::RectF {
        let inv_zoom = 1.0 / self.zoom;
        let view_w = self.view_size.0 * inv_zoom;
        let view_h = self.view_size.1 * inv_zoom;
        let cx = self.content_size.0 * 0.5 + self.offset[0];
        let cy = self.content_size.1 * 0.5 + self.offset[1];
        gfx::RectF::new(
            (cx - view_w * 0.5).clamp(0.0, self.content_size.0 - view_w),
            (cy - view_h * 0.5).clamp(0.0, self.content_size.1 - view_h),
            view_w.min(self.content_size.0),
            view_h.min(self.content_size.1),
        )
    }

    fn compute_min_zoom(content: (f32, f32), view: (f32, f32)) -> f32 {
        if content.0 <= 0.0 || content.1 <= 0.0 || view.0 <= 0.0 || view.1 <= 0.0 {
            return 1.0;
        }
        let sx = view.0 / content.0;
        let sy = view.1 / content.1;
        sx.max(sy)
    }

    fn clamp_offset(&mut self) {
        let inv_zoom = 1.0 / self.zoom;
        let view_w = self.view_size.0 * inv_zoom;
        let view_h = self.view_size.1 * inv_zoom;
        let max_x = (self.content_size.0 - view_w) * 0.5;
        let max_y = (self.content_size.1 - view_h) * 0.5;
        self.offset[0] = self.offset[0].clamp(-max_x, max_x);
        self.offset[1] = self.offset[1].clamp(-max_y, max_y);
    }
}

pub fn recording_event_to_ui(event: RecordingEvent) -> CameraRecordingUiEvent {
    match event {
        RecordingEvent::Completed(result) => CameraRecordingUiEvent::Completed {
            path: result.path,
            duration_ns: result.duration_ns,
            size_bytes: result.size_bytes,
            had_audio: result.had_audio,
        },
        RecordingEvent::Cancelled => CameraRecordingUiEvent::Cancelled,
        RecordingEvent::Failed(err) => {
            CameraRecordingUiEvent::Failed { message: format_platform_error(err) }
        }
    }
}

fn format_platform_error(err: PlatformError) -> String {
    use core::fmt::Write;
    let mut out = String::new();
    let _ = write!(out, "{}", err);
    out
}
