use std::sync::{Arc, Mutex};

use oxideui_platform_api::{
    AudioSample, CameraConfig, CameraDevice, CameraFrame, CameraImage, CameraManager,
    CameraRecording, CameraStream, CaptureMode, PlatformError, RecordingContainer,
    RecordingDestination, RecordingEvent, RecordingOptions, RecordingResult,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CamStateSnapshot {
    started: bool,
    stop_calls: u32,
    last_cfg: Option<CameraConfig>,
    device: CameraDevice,
    fps: u32,
    width: u32,
    height: u32,
    mode: CaptureMode,
}

impl Default for CamStateSnapshot {
    fn default() -> Self {
        Self {
            started: false,
            stop_calls: 0,
            last_cfg: None,
            device: CameraDevice::Back,
            fps: 0,
            width: 0,
            height: 0,
            mode: CaptureMode::Preview,
        }
    }
}

struct CamState {
    snapshot: CamStateSnapshot,
}

impl Default for CamState {
    fn default() -> Self {
        Self { snapshot: CamStateSnapshot::default() }
    }
}

struct CamInner {
    state: Mutex<CamState>,
    frames: Mutex<Vec<CameraFrame>>,
    record_active: Mutex<bool>,
    record_cb: Mutex<Option<Box<dyn Fn(RecordingEvent) + Send>>>,
    record_events: Mutex<Vec<RecordingEvent>>,
    record_opts: Mutex<Option<RecordingOptions>>,
}

impl Default for CamInner {
    fn default() -> Self {
        Self {
            state: Mutex::new(CamState::default()),
            frames: Mutex::new(Vec::new()),
            record_active: Mutex::new(false),
            record_cb: Mutex::new(None),
            record_events: Mutex::new(Vec::new()),
            record_opts: Mutex::new(None),
        }
    }
}

#[derive(Clone, Default)]
struct FakeCamera {
    inner: Arc<CamInner>,
}

struct FakeStream {
    inner: Arc<CamInner>,
}

struct FakeRecording {
    inner: Arc<CamInner>,
    had_audio: bool,
}

impl CamInner {
    fn snapshot(&self) -> CamStateSnapshot {
        self.state.lock().expect("state mutex").snapshot
    }

    fn begin_recording(
        &self,
        opts: RecordingOptions,
        cb: Box<dyn Fn(RecordingEvent) + Send>,
    ) -> Result<(), PlatformError> {
        let mut active = self.record_active.lock().expect("record active mutex");
        if *active {
            return Err(PlatformError::Busy);
        }
        *active = true;
        drop(active);
        {
            let mut store = self.record_opts.lock().expect("record opts mutex");
            *store = Some(opts);
        }
        let mut slot = self.record_cb.lock().expect("record cb mutex");
        *slot = Some(cb);
        Ok(())
    }

    fn finish_recording(&self, event: RecordingEvent) {
        {
            let mut events = self.record_events.lock().expect("record events mutex");
            events.push(event.clone());
        }
        let cb = {
            let mut slot = self.record_cb.lock().expect("record cb mutex");
            slot.take()
        };
        if let Some(cb) = cb {
            cb(event);
        }
        let mut active = self.record_active.lock().expect("record active mutex");
        *active = false;
        self.record_opts.lock().expect("record opts mutex").take();
    }

    fn last_record_options(&self) -> Option<RecordingOptions> {
        self.record_opts.lock().expect("record opts mutex").clone()
    }
}

impl FakeCamera {
    fn new() -> Self {
        Self { inner: Arc::new(CamInner::default()) }
    }

    fn snapshot(&self) -> CamStateSnapshot {
        self.inner.snapshot()
    }

    fn last_record_options(&self) -> Option<RecordingOptions> {
        self.inner.last_record_options()
    }
}

impl CameraStream for FakeStream {
    fn stop(&self) {
        let mut guard = self.inner.state.lock().expect("state mutex");
        let snap = &mut guard.snapshot;
        if snap.started {
            snap.started = false;
        }
        snap.stop_calls = snap.stop_calls.saturating_add(1);
    }
}

impl CameraRecording for FakeRecording {
    fn stop(&self) {
        self.inner.finish_recording(RecordingEvent::Completed(RecordingResult {
            path: "video.mp4".into(),
            duration_ns: 2_000_000_000,
            size_bytes: 2048,
            had_audio: self.had_audio,
        }));
    }

    fn cancel(&self) {
        self.inner.finish_recording(RecordingEvent::Cancelled);
    }
}

impl CameraManager for FakeCamera {
    fn start_stream(
        &self,
        cfg: CameraConfig,
        on_frame: Box<dyn Fn(CameraFrame) + Send>,
        on_audio: Option<Box<dyn Fn(AudioSample) + Send>>,
    ) -> Result<Box<dyn CameraStream + Send>, PlatformError> {
        let mut guard = self.inner.state.lock().expect("state mutex");
        let snap = &mut guard.snapshot;
        snap.started = true;
        snap.last_cfg = Some(cfg);
        snap.device = cfg.device;
        snap.fps = cfg.fps;
        snap.width = cfg.resolution.0;
        snap.height = cfg.resolution.1;
        snap.mode = cfg.capture;
        drop(guard);

        let frame = CameraFrame {
            image: CameraImage::Nv12 {
                y_plane: vec![0u8; 4],
                uv_plane: vec![128u8; 4],
                stride_y: 2,
                stride_uv: 2,
                bit_depth: 8,
                matrix: 0,
                video_range: 0,
            },
            size: (cfg.resolution.0, cfg.resolution.1),
            timestamp_ns: 1_000,
            rotation_deg: 0,
        };
        self.inner.frames.lock().expect("frames mutex").push(frame.clone());
        on_frame(frame);
        if let Some(cb) = on_audio {
            cb(AudioSample {
                channels: 1,
                sample_rate_hz: 48_000,
                data: vec![0, 0],
                timestamp_ns: 2_000,
            });
        }
        Ok(Box::new(FakeStream { inner: Arc::clone(&self.inner) }))
    }

    fn start_recording(
        &self,
        options: RecordingOptions,
        on_event: Box<dyn Fn(RecordingEvent) + Send>,
    ) -> Result<Box<dyn CameraRecording + Send>, PlatformError> {
        self.inner.begin_recording(options.clone(), on_event)?;
        Ok(Box::new(FakeRecording {
            inner: Arc::clone(&self.inner),
            had_audio: options.include_audio,
        }))
    }

    fn select_device(&self, device: CameraDevice) {
        let mut guard = self.inner.state.lock().expect("state mutex");
        let snap = &mut guard.snapshot;
        snap.device = device;
        snap.last_cfg = snap.last_cfg.map(|mut cfg| {
            cfg.device = device;
            cfg
        });
    }

    fn set_fps(&self, fps: u32) {
        let mut guard = self.inner.state.lock().expect("state mutex");
        let snap = &mut guard.snapshot;
        snap.fps = fps;
        snap.last_cfg = snap.last_cfg.map(|mut cfg| {
            cfg.fps = fps;
            cfg
        });
    }

    fn set_resolution(&self, width: u32, height: u32) {
        let mut guard = self.inner.state.lock().expect("state mutex");
        let snap = &mut guard.snapshot;
        snap.width = width;
        snap.height = height;
        snap.last_cfg = snap.last_cfg.map(|mut cfg| {
            cfg.resolution = (width, height);
            cfg
        });
    }

    fn set_mode(&self, mode: CaptureMode) {
        let mut guard = self.inner.state.lock().expect("state mutex");
        let snap = &mut guard.snapshot;
        snap.mode = mode;
        snap.last_cfg = snap.last_cfg.map(|mut cfg| {
            cfg.capture = mode;
            cfg
        });
    }
}

#[test]
fn default_camera_config_matches_expected() {
    let cfg = CameraConfig::default();
    assert_eq!(cfg.device, CameraDevice::Back);
    assert_eq!(cfg.fps, 30);
    assert_eq!(cfg.resolution, (1920, 1080));
    assert_eq!(cfg.capture, CaptureMode::Preview);
}

#[test]
fn camera_stream_invokes_callback_and_tracks_state() {
    let manager = FakeCamera::new();
    let frames = Arc::new(Mutex::new(Vec::new()));
    let capture_frames = Arc::clone(&frames);
    let audio = Arc::new(Mutex::new(Vec::new()));
    let capture_audio = Arc::clone(&audio);
    let cfg = CameraConfig {
        device: CameraDevice::Front,
        fps: 60,
        resolution: (1280, 720),
        capture: CaptureMode::Video,
    };
    let stream = manager
        .start_stream(
            cfg,
            Box::new(move |frame| {
                capture_frames.lock().expect("frames mutex").push(frame);
            }),
            Some(Box::new(move |sample| {
                capture_audio.lock().expect("audio mutex").push(sample.data.len());
            })),
        )
        .expect("start_stream ok");

    let seen = frames.lock().expect("frames mutex");
    assert_eq!(seen.len(), 1);
    let audio_seen = audio.lock().expect("audio mutex");
    assert_eq!(audio_seen.len(), 1);
    assert_eq!(seen[0].size, (1280, 720));
    assert_eq!(seen[0].timestamp_ns, 1_000);
    drop(seen);
    drop(audio_seen);

    let snap = manager.snapshot();
    assert!(snap.started);
    assert_eq!(snap.last_cfg, Some(cfg));
    assert_eq!(snap.device, CameraDevice::Front);
    assert_eq!(snap.fps, 60);
    assert_eq!(snap.width, 1280);
    assert_eq!(snap.height, 720);
    assert_eq!(snap.mode, CaptureMode::Video);

    manager.select_device(CameraDevice::Back);
    manager.set_fps(24);
    manager.set_resolution(640, 480);
    manager.set_mode(CaptureMode::Photo);

    let snap = manager.snapshot();
    assert_eq!(snap.device, CameraDevice::Back);
    assert_eq!(snap.fps, 24);
    assert_eq!(snap.width, 640);
    assert_eq!(snap.height, 480);
    assert_eq!(snap.mode, CaptureMode::Photo);
    assert_eq!(snap.last_cfg.unwrap().resolution, (640, 480));

    stream.stop();
    let snap = manager.snapshot();
    assert!(!snap.started);
    assert_eq!(snap.stop_calls, 1);
}

#[test]
fn camera_frames_are_equatable() {
    let frame_a = CameraFrame {
        image: CameraImage::Nv12 {
            y_plane: vec![1, 2, 3],
            uv_plane: vec![4, 5, 6],
            stride_y: 3,
            stride_uv: 3,
            bit_depth: 10,
            matrix: 2,
            video_range: 1,
        },
        size: (320, 240),
        timestamp_ns: 42,
        rotation_deg: 0,
    };
    let frame_b = frame_a.clone();
    assert_eq!(frame_a, frame_b);
}

#[test]
fn camera_start_recording_tracks_options_and_events() {
    let manager = FakeCamera::new();
    let events = Arc::new(Mutex::new(Vec::new()));
    let capture = Arc::clone(&events);
    let options = RecordingOptions {
        destination: RecordingDestination::file("/tmp/demo.mp4"),
        container: RecordingContainer::Mov,
        include_audio: false,
    };
    let rec = manager
        .start_recording(
            options.clone(),
            Box::new(move |evt| {
                capture.lock().expect("capture mutex").push(evt);
            }),
        )
        .expect("record start");
    rec.stop();
    let seen_opts = manager.last_record_options();
    // Recording options should be cleared after completion.
    assert!(seen_opts.is_none());
    let events = events.lock().expect("events mutex");
    assert_eq!(events.len(), 1);
    match &events[0] {
        RecordingEvent::Completed(result) => {
            assert_eq!(result.path, "video.mp4");
            assert_eq!(result.size_bytes, 2048);
            assert!(!result.had_audio);
        }
        other => panic!("unexpected event {other:?}"),
    }
}

#[test]
fn camera_recording_busy_errors_on_second_start() {
    let manager = FakeCamera::new();
    let rec =
        manager.start_recording(RecordingOptions::default(), Box::new(|_| {})).expect("record ok");
    let err = manager.start_recording(RecordingOptions::default(), Box::new(|_| {}));
    assert!(matches!(err, Err(PlatformError::Busy)));
    rec.cancel();
}
