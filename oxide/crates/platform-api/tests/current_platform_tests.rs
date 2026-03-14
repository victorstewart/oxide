use oxide_platform_api::{
    clear_current_platform_for_tests, current_platform, current_platform_if_registered,
    request_redraw_if_registered, set_current_platform, App, AudioSample, BleCacheEntry, BleUuid,
    Bluetooth, BluetoothEvent, CameraConfig, CameraDevice, CameraFrame, CameraManager,
    CameraRecording, CameraStream, Capabilities, CaptureMode, ColorSpace, DeviceCaps, GattChar,
    HapticPattern, Haptics, LocationEvent, LocationOptions, LocationReading, LocationService,
    MotionSample, MotionService, PeripheralId, PermissionDomain, PermissionStatus, Permissions,
    Platform, PlatformError, PushManager, PushNotification, PushToken, RecordingEvent,
    RecordingOptions, ScanOptions, SharedPlatform, TimeService,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[derive(Default)]
struct RecordingHaptics;

impl Haptics for RecordingHaptics {
    fn play(&self, _p: HapticPattern) {}
}

#[derive(Default)]
struct StubPermissions;

impl Permissions for StubPermissions {
    fn request(&self, _domain: PermissionDomain) {}

    fn status(&self, _domain: PermissionDomain) -> PermissionStatus {
        PermissionStatus::Authorized
    }

    fn subscribe(&self, _f: Box<dyn Fn(PermissionDomain, PermissionStatus) + Send>) {}
}

#[derive(Default)]
struct StubCamera;

impl CameraManager for StubCamera {
    fn start_stream(
        &self,
        _cfg: CameraConfig,
        _on_frame: Box<dyn Fn(CameraFrame) + Send>,
        _on_audio: Option<Box<dyn Fn(AudioSample) + Send>>,
    ) -> Result<Box<dyn CameraStream + Send>, PlatformError> {
        Err(PlatformError::Unsupported("camera unavailable in tests"))
    }

    fn start_recording(
        &self,
        _options: RecordingOptions,
        _on_event: Box<dyn Fn(RecordingEvent) + Send>,
    ) -> Result<Box<dyn CameraRecording + Send>, PlatformError> {
        Err(PlatformError::Unsupported("recording unavailable in tests"))
    }

    fn select_device(&self, _device: CameraDevice) {}
    fn set_fps(&self, _fps: u32) {}
    fn set_resolution(&self, _width: u32, _height: u32) {}
    fn set_mode(&self, _mode: CaptureMode) {}
}

#[derive(Default)]
struct StubBluetooth;

impl Bluetooth for StubBluetooth {
    fn powered_on(&self) -> bool {
        true
    }

    fn subscribe_events(&self, _f: Box<dyn Fn(BluetoothEvent) + Send>) {}
    fn start_scan(&self, _opts: &ScanOptions) {}
    fn stop_scan(&self) {}
    fn connect(&self, _id: PeripheralId) {}
    fn disconnect(&self, _id: PeripheralId) {}

    fn read(&self, _id: PeripheralId, _chr: GattChar) -> Result<Vec<u8>, PlatformError> {
        Err(PlatformError::Unsupported("bluetooth unavailable in tests"))
    }

    fn write(
        &self,
        _id: PeripheralId,
        _chr: GattChar,
        _data: &[u8],
        _with_response: bool,
    ) -> Result<(), PlatformError> {
        Ok(())
    }

    fn notify(
        &self,
        _id: PeripheralId,
        _chr: GattChar,
        _enable: bool,
    ) -> Result<(), PlatformError> {
        Ok(())
    }

    fn advertise_start(&self, _name: &str, _services: &[BleUuid]) {}
    fn advertise_stop(&self) {}

    fn cached_peripherals(&self) -> Vec<BleCacheEntry> {
        Vec::new()
    }
}

#[derive(Default)]
struct StubLocation;

impl LocationService for StubLocation {
    fn start(&self, _opts: LocationOptions) -> Result<(), PlatformError> {
        Ok(())
    }

    fn stop(&self) {}
    fn request_once(&self) {}

    fn last(&self) -> Option<LocationReading> {
        None
    }

    fn subscribe(&self, _f: Box<dyn Fn(LocationEvent) + Send>) {}

    fn history(&self) -> Vec<LocationReading> {
        Vec::new()
    }

    fn region_tracker(&self) -> Option<Box<dyn oxide_platform_api::GeoRegionTracker>> {
        None
    }
}

#[derive(Default)]
struct StubMotion;

impl MotionService for StubMotion {
    fn start(&self) -> Result<(), PlatformError> {
        Ok(())
    }

    fn stop(&self) {}

    fn is_running(&self) -> bool {
        false
    }

    fn subscribe(&self, _f: Box<dyn Fn(MotionSample) + Send>) {}

    fn pressure_history(&self) -> Vec<MotionSample> {
        Vec::new()
    }
}

#[derive(Default)]
struct StubPush;

impl PushManager for StubPush {
    fn register(&self) {}
    fn device_token(&self) -> Option<PushToken> {
        None
    }
    fn subscribe(&self, _f: Box<dyn Fn(PushNotification) + Send>) {}
    fn set_badge(&self, _count: i32) {}
    fn clear_badge(&self) {}
}

#[derive(Default)]
struct StubTime;

impl TimeService for StubTime {
    fn monotonic_now(&self) -> Duration {
        Duration::from_millis(42)
    }
}

struct RecordingPlatform {
    redraws: AtomicUsize,
    haptics: Arc<RecordingHaptics>,
    permissions: StubPermissions,
    camera: StubCamera,
    bluetooth: StubBluetooth,
    location: StubLocation,
    motion: StubMotion,
    push: StubPush,
    time: StubTime,
}

impl RecordingPlatform {
    fn new() -> Self {
        Self {
            redraws: AtomicUsize::new(0),
            haptics: Arc::new(RecordingHaptics),
            permissions: StubPermissions,
            camera: StubCamera,
            bluetooth: StubBluetooth,
            location: StubLocation,
            motion: StubMotion,
            push: StubPush,
            time: StubTime,
        }
    }
}

impl Platform for RecordingPlatform {
    fn run_app(&self, _app: Box<dyn App>) -> ! {
        panic!("run_app should not be called in tests");
    }

    fn request_redraw(&self) {
        self.redraws.fetch_add(1, Ordering::SeqCst);
    }

    fn set_high_refresh(&self, _enable: bool) {}
    fn set_idle_timer_disabled(&self, _disabled: bool) {}
    fn open_system_settings(&self) {}
    fn clipboard_get(&self) -> Option<String> {
        None
    }
    fn clipboard_set(&self, _s: &str) {}
    fn ime_show(&self) {}
    fn ime_hide(&self) {}

    fn device_caps(&self) -> DeviceCaps {
        DeviceCaps {
            max_framerate_hz: 120,
            supports_edr: false,
            supports_msaa4x: true,
            native_scale: 2.0,
            color_space: ColorSpace::Srgb,
            a11y_reduce_motion: false,
        }
    }

    fn haptics(&self) -> Arc<dyn Haptics + Send + Sync> {
        self.haptics.clone()
    }

    fn permissions(&self) -> &dyn Permissions {
        &self.permissions
    }
    fn camera(&self) -> &dyn CameraManager {
        &self.camera
    }
    fn bluetooth(&self) -> &dyn Bluetooth {
        &self.bluetooth
    }

    fn bluetooth_with_restoration(&self, _restore_id: &str) -> Box<dyn Bluetooth> {
        Box::new(StubBluetooth)
    }

    fn location(&self) -> &dyn LocationService {
        &self.location
    }
    fn motion(&self) -> &dyn MotionService {
        &self.motion
    }
    fn push(&self) -> &dyn PushManager {
        &self.push
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities::empty()
    }
    fn time(&self) -> &dyn TimeService {
        &self.time
    }
}

#[test]
fn current_platform_registry_tracks_shared_platform_instance() {
    clear_current_platform_for_tests();
    assert!(current_platform_if_registered().is_none());
    assert!(!request_redraw_if_registered());

    let platform: Arc<dyn Platform + Send + Sync> = Arc::new(RecordingPlatform::new());
    set_current_platform(platform.clone());

    let installed = current_platform();
    assert_eq!(installed.device_caps().native_scale, 2.0);
    assert!(request_redraw_if_registered());

    let shared = SharedPlatform::new(platform);
    assert_eq!(shared.time().monotonic_now(), Duration::from_millis(42));
    assert!(shared.bluetooth().powered_on());

    clear_current_platform_for_tests();
    assert!(current_platform_if_registered().is_none());
}
