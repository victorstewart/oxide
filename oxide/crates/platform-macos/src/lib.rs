//! Oxide macOS platform crate
#![allow(clippy::all, clippy::pedantic)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions
)]

use once_cell::sync::Lazy;
use oxide_platform_api as api;

extern "C" {
    fn macos_request_redraw();
    fn macos_set_high_refresh(enable: u8);
    fn macos_set_idle_timer_disabled(disabled: u8);
    fn macos_clipboard_set(ptr: *const u8, len: usize);
    fn macos_clipboard_get(out_ptr: *mut *mut u8, out_len: *mut usize) -> ::libc::c_int;
    fn macos_haptics_play(pattern: u32);
    fn macos_free(p: *mut ::libc::c_void);
}

pub struct MacHaptics;

impl api::Haptics for MacHaptics {
    fn play(&self, p: api::HapticPattern) {
        let code = match p {
            api::HapticPattern::ImpactLight => 0,
            api::HapticPattern::ImpactMedium => 1,
            api::HapticPattern::ImpactHeavy => 2,
            api::HapticPattern::Selection => 3,
            api::HapticPattern::NotificationSuccess => 4,
            api::HapticPattern::NotificationWarning => 5,
            api::HapticPattern::NotificationError => 6,
        };
        unsafe { macos_haptics_play(code) };
    }
}

fn clipboard_get() -> Option<String> {
    let mut ptr: *mut u8 = std::ptr::null_mut();
    let mut len: usize = 0;
    let ok = unsafe { macos_clipboard_get(&mut ptr, &mut len) };
    if ok == 0 || ptr.is_null() || len == 0 { return None; }
    let s = unsafe { std::slice::from_raw_parts(ptr, len) };
    let out = String::from_utf8_lossy(s).into_owned();
    unsafe { macos_free(ptr.cast()) };
    Some(out)
}

fn clipboard_set(s: &str) { unsafe { macos_clipboard_set(s.as_ptr(), s.len()) } }

static HAPTICS: Lazy<std::sync::Arc<MacHaptics>> =
    Lazy::new(|| std::sync::Arc::new(MacHaptics));

pub struct MacPlatform;

impl api::Platform for MacPlatform {
    fn run_app(&self, _app: alloc::boxed::Box<dyn api::App>) -> ! {
        // For macOS host app, NSApplicationMain is already running in Obj-C.
        // In this test host, run() will park the current thread indefinitely,
        // since the main loop is already active.
        #[allow(clippy::empty_loop)]
        loop {}
    }
    fn request_redraw(&self) { unsafe { macos_request_redraw() } }
    fn set_high_refresh(&self, enable: bool) { unsafe { macos_set_high_refresh(if enable {1} else {0}) } }
    fn set_idle_timer_disabled(&self, disabled: bool) { unsafe { macos_set_idle_timer_disabled(if disabled {1} else {0}) } }
    fn open_system_settings(&self) {}
    fn clipboard_get(&self) -> Option<String> { clipboard_get() }
    fn clipboard_set(&self, s: &str) { clipboard_set(s) }
    fn ime_show(&self) { /* not applicable on macOS; input method handled by responder */ }
    fn ime_hide(&self) { /* not applicable on macOS */ }
    fn device_caps(&self) -> api::DeviceCaps {
        api::DeviceCaps {
            max_framerate_hz: 120,
            supports_edr: false,
            supports_msaa4x: true,
            native_scale: 1.0,
            color_space: api::ColorSpace::Srgb,
            a11y_reduce_motion: false,
        }
    }
    fn haptics(&self) -> std::sync::Arc<dyn api::Haptics + Send + Sync> { HAPTICS.clone() }
    fn permissions(&self) -> &dyn api::Permissions { &NOP_PERMS }
    fn camera(&self) -> &dyn api::CameraManager { &NOP_CAMERA }
    fn bluetooth(&self) -> &dyn api::Bluetooth { &NOP_BLE }
    fn location(&self) -> &dyn api::LocationService { &NOP_LOCATION }
    fn motion(&self) -> &dyn api::MotionService { &NOP_MOTION }
    fn push(&self) -> &dyn api::PushManager { &NOP_PUSH }
    fn capabilities(&self) -> api::Capabilities { api::Capabilities::empty() }
    fn bluetooth_with_restoration(&self, _restore_id: &str) -> alloc::boxed::Box<dyn api::Bluetooth> {
        alloc::boxed::Box::new(NOP_BLE)
    }
    fn networking(&self) -> &dyn api::Networking { &NOP_NETWORKING }
    fn paths(&self) -> &dyn api::PathService { &NOP_PATHS }
    fn secure_storage(&self) -> &dyn api::secure_storage::SecureStorage { &NOP_SECURE_STORAGE }
    fn time(&self) -> &dyn api::TimeService { &NOP_TIME }
    fn web_view_service(&self) -> &dyn api::web_view::WebViewService { &NOP_WEB_VIEW }
    fn telephony(&self) -> &dyn api::telephony::TelephonyService { &NOP_TELEPHONY }
    fn media_library(&self) -> &dyn api::media_library::MediaLibrary { &NOP_MEDIA_LIBRARY }
    fn network_status(&self) -> &dyn api::network_status::NetworkStatusService { &NOP_NETWORK_STATUS }
}

static NOP_PERMS: NopPermissions = NopPermissions;
struct NopPermissions;
impl api::Permissions for NopPermissions {
    fn request(&self, _domain: api::PermissionDomain) {}
    fn status(&self, _domain: api::PermissionDomain) -> api::PermissionStatus { api::PermissionStatus { kind: api::PermissionStatusKind::Denied, detail: None } }
    fn subscribe(&self, _f: alloc::boxed::Box<dyn Fn(api::PermissionDomain, api::PermissionStatus) + Send>) {}
}

static NOP_CAMERA: NopCamera = NopCamera;
struct NopCamera;
impl api::CameraManager for NopCamera {
    fn start_stream(
        &self,
        _cfg: api::CameraConfig,
        _on_frame: alloc::boxed::Box<dyn Fn(api::CameraFrame) + Send>,
        _on_audio: Option<alloc::boxed::Box<dyn Fn(api::AudioSample) + Send>>,
    ) -> Result<Box<dyn api::CameraStream>, api::PlatformError> {
        Err(api::PlatformError::Unsupported("macOS test app"))
    }
    fn start_recording(
        &self,
        _options: api::RecordingOptions,
        _on_event: alloc::boxed::Box<dyn Fn(api::RecordingEvent) + Send>,
    ) -> Result<alloc::boxed::Box<dyn api::CameraRecording + Send>, api::PlatformError> {
        Err(api::PlatformError::Unsupported("macOS test app"))
    }
    fn select_device(&self, _device: api::CameraDevice) {}
    fn set_fps(&self, _fps: u32) {}
    fn set_resolution(&self, _width: u32, _height: u32) {}
    fn set_mode(&self, _mode: api::CaptureMode) {}
    fn set_focus_point(&self, _x: f32, _y: f32) -> Result<(), api::PlatformError> {
        Err(api::PlatformError::Unsupported("macOS test app"))
    }
    fn set_zoom_factor(&self, _factor: f32) -> Result<(), api::PlatformError> {
        Err(api::PlatformError::Unsupported("macOS test app"))
    }
    fn set_flash_mode(&self, _mode: api::FlashMode) -> Result<(), api::PlatformError> {
        Err(api::PlatformError::Unsupported("macOS test app"))
    }
    fn set_torch_mode(&self, _mode: api::TorchMode) -> Result<(), api::PlatformError> {
        Err(api::PlatformError::Unsupported("macOS test app"))
    }
    fn capture_photo(
        &self,
        _options: api::PhotoOptions,
        _on_event: alloc::boxed::Box<dyn Fn(api::PhotoEvent) + Send>,
    ) -> Result<(), api::PlatformError> {
        Err(api::PlatformError::Unsupported("macOS test app"))
    }
}

static NOP_BLE: NopBle = NopBle;
struct NopBle;
impl api::Bluetooth for NopBle {
    fn powered_on(&self) -> bool { false }
    fn subscribe_events(&self, _f: alloc::boxed::Box<dyn Fn(api::BluetoothEvent) + Send>) {}
    fn start_scan(&self, _opts: &api::ScanOptions) {}
    fn stop_scan(&self) {}
    fn connect(&self, _id: api::PeripheralId) {}
    fn disconnect(&self, _id: api::PeripheralId) {}
    fn read(&self, _id: api::PeripheralId, _chr: api::GattChar) -> Result<Vec<u8>, api::PlatformError> { Err(api::PlatformError::Unsupported("macOS test app")) }
    fn write(&self, _id: api::PeripheralId, _chr: api::GattChar, _data: &[u8], _with_response: bool) -> Result<(), api::PlatformError> { Err(api::PlatformError::Unsupported("macOS test app")) }
    fn notify(&self, _id: api::PeripheralId, _chr: api::GattChar, _enable: bool) -> Result<(), api::PlatformError> { Err(api::PlatformError::Unsupported("macOS test app")) }
    fn advertise_start(&self, _name: &str, _services: &[api::BleUuid]) {}
    fn advertise_stop(&self) {}
    fn cached_peripherals(&self) -> Vec<api::BleCacheEntry> { Vec::new() }
}

static NOP_LOCATION: NopLocation = NopLocation;
struct NopLocation;
impl api::LocationService for NopLocation {
    fn start(&self, _opts: api::LocationOptions) -> Result<(), api::PlatformError> {
        Err(api::PlatformError::Unsupported("macOS test app"))
    }
    fn stop(&self) {}
    fn request_once(&self) {}
    fn last(&self) -> Option<api::LocationReading> { None }
    fn subscribe(&self, _f: alloc::boxed::Box<dyn Fn(api::LocationEvent) + Send>) {}
    fn history(&self) -> alloc::vec::Vec<api::LocationReading> { alloc::vec::Vec::new() }
    fn region_tracker(&self) -> Option<alloc::boxed::Box<dyn api::GeoRegionTracker>> { None }
    fn set_accuracy(&self, _accuracy: api::LocationAccuracy) -> Result<(), api::PlatformError> {
        Err(api::PlatformError::Unsupported("macOS test app"))
    }
}

static NOP_MOTION: NopMotion = NopMotion;
struct NopMotion;
impl api::MotionService for NopMotion {
    fn start(&self) -> Result<(), api::PlatformError> {
        Err(api::PlatformError::Unsupported("macOS test app"))
    }
    fn stop(&self) {}
    fn is_running(&self) -> bool { false }
    fn subscribe(&self, _f: alloc::boxed::Box<dyn Fn(api::MotionSample) + Send>) {}
    fn pressure_history(&self) -> alloc::vec::Vec<api::MotionSample> { alloc::vec::Vec::new() }
}

static NOP_PUSH: NopPush = NopPush;
struct NopPush;
impl api::PushManager for NopPush {
    fn register(&self) {}
    fn device_token(&self) -> Option<api::PushToken> { None }
    fn subscribe(&self, _f: alloc::boxed::Box<dyn Fn(api::PushNotification) + Send>) {}
    fn set_badge(&self, _count: i32) {}
    fn clear_badge(&self) {}
    fn clear_all_delivered(&self) {}
}

static NOP_NETWORKING: NopNetworking = NopNetworking;
struct NopNetworking;
impl api::Networking for NopNetworking {
    fn connect_tcp(
        &self,
        _options: api::ConnectionOptions,
        _on_event: alloc::boxed::Box<dyn Fn(api::ConnectionEvent) + Send>,
    ) -> Result<alloc::boxed::Box<dyn api::Connection + Send>, api::PlatformError> {
        Err(api::PlatformError::Unsupported("macOS test app"))
    }
    fn connect_quic(
        &self,
        _options: api::ConnectionOptions,
        _on_event: alloc::boxed::Box<dyn Fn(api::ConnectionEvent) + Send>,
    ) -> Result<alloc::boxed::Box<dyn api::ConnectionGroup + Send>, api::PlatformError> {
        Err(api::PlatformError::Unsupported("macOS test app"))
    }
    fn bind_udp(
        &self,
        _local_port: u16,
        _on_event: alloc::boxed::Box<dyn Fn(api::UdpEvent) + Send>,
    ) -> Result<alloc::boxed::Box<dyn api::UdpSocket + Send>, api::PlatformError> {
        Err(api::PlatformError::Unsupported("macOS test app"))
    }
}

static NOP_PATHS: NopPaths = NopPaths;
struct NopPaths;
impl api::PathService for NopPaths {
    fn get(&self, _path: api::StandardPath) -> alloc::string::String {
        alloc::string::String::from("/tmp")
    }
}

static NOP_SECURE_STORAGE: NopSecureStorage = NopSecureStorage;
struct NopSecureStorage;
impl api::secure_storage::SecureStorage for NopSecureStorage {
    fn save(&self, _key: &str, _data: &[u8]) -> impl core::future::Future<Output = Result<(), api::PlatformError>> + Send {
        async { Err(api::PlatformError::Unsupported("macOS test app")) }
    }
    fn load(&self, _key: &str) -> impl core::future::Future<Output = Result<Option<alloc::vec::Vec<u8>>, api::PlatformError>> + Send {
        async { Ok(None) }
    }
    fn delete(&self, _key: &str) -> impl core::future::Future<Output = Result<(), api::PlatformError>> + Send {
        async { Err(api::PlatformError::Unsupported("macOS test app")) }
    }
}

static NOP_TIME: NopTime = NopTime;
struct NopTime;
impl api::TimeService for NopTime {
    fn monotonic_now(&self) -> core::time::Duration {
        core::time::Duration::from_nanos(0)
    }
}

static NOP_WEB_VIEW: NopWebView = NopWebView;
struct NopWebView;
impl api::web_view::WebViewService for NopWebView {
    fn create_web_view(&self, _url: &str, _config: api::web_view::WebViewConfig) -> Result<alloc::boxed::Box<dyn api::web_view::WebView + Send>, api::PlatformError> {
        Err(api::PlatformError::Unsupported("macOS test app"))
    }
}

static NOP_TELEPHONY: NopTelephony = NopTelephony;
struct NopTelephony;
impl api::telephony::TelephonyService for NopTelephony {
    fn make_call(&self, _phone_number: &str) -> Result<(), api::PlatformError> {
        Err(api::PlatformError::Unsupported("macOS test app"))
    }
}

static NOP_MEDIA_LIBRARY: NopMediaLibrary = NopMediaLibrary;
struct NopMediaLibrary;
impl api::media_library::MediaLibrary for NopMediaLibrary {
    fn manager(&self) -> &dyn api::media_library::MediaLibraryManager {
        &NOP_MEDIA_LIBRARY_MANAGER
    }
}

static NOP_MEDIA_LIBRARY_MANAGER: NopMediaLibraryManager = NopMediaLibraryManager;
struct NopMediaLibraryManager;
impl api::media_library::MediaLibraryManager for NopMediaLibraryManager {
    fn fetch_assets(&mut self, _options: api::media_library::FetchOptions) -> api::media_library::MediaFetchResult {
        api::media_library::MediaFetchResult::Error("macOS test app".into())
    }
    fn load_thumbnail(&mut self, _identifier: &str, _size: api::media_library::ThumbnailSize) -> api::media_library::ImageLoadResult {
        api::media_library::ImageLoadResult::Error("macOS test app".into())
    }
    fn load_full_image(&mut self, _identifier: &str) -> api::media_library::ImageLoadResult {
        api::media_library::ImageLoadResult::Error("macOS test app".into())
    }
    fn subscribe_to_changes<F>(&mut self, _callback: F) -> u32
    where
        F: Fn() + Send + 'static,
    {
        0
    }
    fn unsubscribe(&mut self, _subscription_id: u32) {}
}

static NOP_NETWORK_STATUS: NopNetworkStatus = NopNetworkStatus;
struct NopNetworkStatus;
impl api::network_status::NetworkStatusService for NopNetworkStatus {
    fn current_status(&self) -> api::network_status::NetworkStatus {
        api::network_status::NetworkStatus::Unknown
    }
    fn subscribe(&self, _f: alloc::boxed::Box<dyn Fn(api::network_status::NetworkStatus) + Send>) {}
}

pub fn platform() -> MacPlatform { MacPlatform }
