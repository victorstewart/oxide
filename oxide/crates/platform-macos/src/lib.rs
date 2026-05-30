//! Oxide macOS platform crate
#![allow(clippy::all, clippy::pedantic)]
#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc, clippy::module_name_repetitions)]

extern crate alloc;

use std::sync::{Arc, Mutex, Once};

use once_cell::sync::Lazy;
use oxide_platform_apple::{
    network_status_from_apple_interface_mask, permission_domain_from_apple_code,
    permission_domain_to_apple_code, permission_status_from_apple_code, AppleHttpClient,
    AppleBluetooth, AppleCameraManager, AppleLocationService, AppleMediaLibraryManager, AppleMotionService,
    AppleSecureStorage, ApplePushManager, AppleSocketNetworking, AppleWebViewService,
    apple_bluetooth_with_restoration,
};
use oxide_platform_api as api;

extern "C" {
    fn macos_request_redraw();
    fn macos_set_high_refresh(enable: u8);
    fn macos_set_idle_timer_disabled(disabled: u8);
    fn macos_open_system_settings();
    fn macos_open_external_url(ptr: *const u8, len: usize) -> ::libc::c_int;
    fn macos_max_framerate_hz() -> u32;
    fn macos_native_scale() -> f32;
    fn macos_supports_edr() -> u8;
    fn macos_reduce_motion_enabled() -> u8;
    fn macos_camera_available() -> u8;
    fn macos_network_status(out_connected: *mut u8, out_interfaces: *mut u32) -> ::libc::c_int;
    fn macos_set_network_status_callback(cb: Option<extern "C" fn(u8, u32)>);
    fn macos_start_network_monitor() -> ::libc::c_int;
    fn macos_permission_status(domain: u32) -> u32;
    fn macos_permission_request(domain: u32);
    fn macos_set_permission_callback(cb: Option<extern "C" fn(u32, u32)>);
    fn macos_location_services_available() -> u8;
    fn macos_motion_available() -> u8;
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
    if ok == 0 {
        return None;
    }
    if len == 0 {
        if !ptr.is_null() {
            unsafe { macos_free(ptr.cast()) };
        }
        return Some(String::new());
    }
    if ptr.is_null() {
        return None;
    }
    let s = unsafe { std::slice::from_raw_parts(ptr, len) };
    let out = String::from_utf8_lossy(s).into_owned();
    unsafe { macos_free(ptr.cast()) };
    Some(out)
}

fn clipboard_set(s: &str) {
    unsafe { macos_clipboard_set(s.as_ptr(), s.len()) }
}

static HAPTICS: Lazy<std::sync::Arc<MacHaptics>> = Lazy::new(|| std::sync::Arc::new(MacHaptics));
type NetworkStatusCallback = Arc<Mutex<alloc::boxed::Box<dyn Fn(api::network_status::NetworkStatus) + Send>>>;
static NETWORK_STATUS_CALLBACKS: Lazy<Mutex<alloc::vec::Vec<NetworkStatusCallback>>> =
    Lazy::new(|| Mutex::new(alloc::vec::Vec::new()));
static NETWORK_STATUS_INIT: Once = Once::new();
type PermissionStatusCallback = Arc<Mutex<alloc::boxed::Box<dyn Fn(api::PermissionDomain, api::PermissionStatus) + Send>>>;
static PERMISSION_CALLBACKS: Lazy<Mutex<alloc::vec::Vec<PermissionStatusCallback>>> =
    Lazy::new(|| Mutex::new(alloc::vec::Vec::new()));
static PERMISSION_INIT: Once = Once::new();
static MAC_HTTP: AppleHttpClient = AppleHttpClient::new();

pub struct MacPlatform;

impl MacPlatform {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl api::Platform for MacPlatform {
    fn run_app(&self, _app: alloc::boxed::Box<dyn api::App>) -> ! {
        // For macOS host app, NSApplicationMain is already running in Obj-C.
        // Park this caller indefinitely without burning a CPU core.
        loop {
            std::thread::park();
        }
    }
    fn request_redraw(&self) {
        unsafe { macos_request_redraw() }
    }
    fn set_high_refresh(&self, enable: bool) {
        unsafe { macos_set_high_refresh(if enable { 1 } else { 0 }) }
    }
    fn set_idle_timer_disabled(&self, disabled: bool) {
        unsafe { macos_set_idle_timer_disabled(if disabled { 1 } else { 0 }) }
    }
    fn open_system_settings(&self) {
        unsafe { macos_open_system_settings() }
    }
    fn open_external_url(&self, url: &str) -> Result<(), api::PlatformError> {
        let ok = unsafe { macos_open_external_url(url.as_ptr(), url.len()) };
        if ok == 0 {
            Err(api::PlatformError::Unsupported("macOS rejected external url"))
        } else {
            Ok(())
        }
    }
    fn clipboard_get(&self) -> Option<String> {
        clipboard_get()
    }
    fn clipboard_set(&self, s: &str) {
        clipboard_set(s)
    }
    fn ime_show(&self) { /* not applicable on macOS; input method handled by responder */
    }
    fn ime_hide(&self) { /* not applicable on macOS */
    }
    fn device_caps(&self) -> api::DeviceCaps {
        let max_framerate_hz = unsafe { macos_max_framerate_hz() }.max(60);
        let native_scale = {
            let scale = unsafe { macos_native_scale() };
            if scale.is_finite() && scale > 0.0 { scale } else { 1.0 }
        };
        api::DeviceCaps {
            max_framerate_hz,
            supports_edr: unsafe { macos_supports_edr() != 0 },
            supports_msaa4x: true,
            native_scale,
            color_space: api::ColorSpace::Srgb,
            a11y_reduce_motion: unsafe { macos_reduce_motion_enabled() != 0 },
        }
    }
    fn haptics(&self) -> std::sync::Arc<dyn api::Haptics + Send + Sync> {
        HAPTICS.clone()
    }
    fn permissions(&self) -> &dyn api::Permissions {
        &MAC_PERMS
    }
    fn camera(&self) -> &dyn api::CameraManager {
        &MAC_CAMERA
    }
    fn bluetooth(&self) -> &dyn api::Bluetooth {
        &MAC_BLUETOOTH
    }
    fn location(&self) -> &dyn api::LocationService {
        &MAC_LOCATION
    }
    fn motion(&self) -> &dyn api::MotionService {
        &MAC_MOTION
    }
    fn push(&self) -> &dyn api::PushManager {
        &MAC_PUSH
    }
    fn capabilities(&self) -> api::Capabilities {
        let mut caps = api::Capabilities::HOVER_POINTER
            | api::Capabilities::BLUETOOTH
            | api::Capabilities::PUSH;
        if unsafe { macos_camera_available() } != 0 {
            caps |= api::Capabilities::CAMERA | api::Capabilities::CAMERA_RECORDING;
        }
        if unsafe { macos_location_services_available() } != 0 {
            caps |= api::Capabilities::LOCATION;
        }
        if unsafe { macos_motion_available() } != 0 {
            caps |= api::Capabilities::MOTION;
        }
        caps
    }
    fn bluetooth_with_restoration(
        &self,
        restore_id: &str,
    ) -> alloc::boxed::Box<dyn api::Bluetooth> {
        alloc::boxed::Box::new(apple_bluetooth_with_restoration(restore_id))
    }
    fn networking(&self) -> &dyn api::Networking {
        &MAC_NETWORKING
    }
    fn http(&self) -> &dyn api::HttpClient {
        &MAC_HTTP
    }
    fn paths(&self) -> &dyn api::PathService {
        &MAC_PATHS
    }
    fn secure_storage(&self) -> &dyn api::SecureStorage {
        &MAC_SECURE_STORAGE
    }
    fn time(&self) -> &dyn api::TimeService {
        &MAC_TIME
    }
    fn web_view_service(&self) -> &dyn api::web_view::WebViewService {
        &MAC_WEB_VIEW
    }
    fn telephony(&self) -> &dyn api::telephony::TelephonyService {
        &MAC_TELEPHONY
    }
    fn media_library(&self) -> &dyn api::media_library::MediaLibrary {
        &MAC_MEDIA_LIBRARY
    }
    fn network_status(&self) -> &dyn api::network_status::NetworkStatusService {
        &MAC_NETWORK_STATUS
    }
}

static MAC_PERMS: MacPermissions = MacPermissions;
struct MacPermissions;
impl api::Permissions for MacPermissions {
    fn request(&self, domain: api::PermissionDomain) {
        start_permission_bridge();
        unsafe {
            macos_permission_request(permission_domain_to_apple_code(domain));
        }
    }
    fn status(&self, domain: api::PermissionDomain) -> api::PermissionStatus {
        let status = unsafe { macos_permission_status(permission_domain_to_apple_code(domain)) };
        permission_status_from_apple_code(status)
    }
    fn subscribe(
        &self,
        f: alloc::boxed::Box<dyn Fn(api::PermissionDomain, api::PermissionStatus) + Send>,
    ) {
        start_permission_bridge();
        permission_callbacks().push(Arc::new(Mutex::new(f)));
    }
}

static MAC_CAMERA: AppleCameraManager = AppleCameraManager;

static MAC_BLUETOOTH: AppleBluetooth = AppleBluetooth::new();

static MAC_LOCATION: AppleLocationService = AppleLocationService::new();

static MAC_MOTION: AppleMotionService = AppleMotionService::new();

static MAC_PUSH: ApplePushManager = ApplePushManager::new();

static MAC_NETWORKING: AppleSocketNetworking = AppleSocketNetworking::new();

static MAC_PATHS: MacPaths = MacPaths;
struct MacPaths;
impl api::PathService for MacPaths {
    fn get(&self, path: api::StandardPath) -> alloc::string::String {
        standard_path(path)
    }
}

static MAC_SECURE_STORAGE: AppleSecureStorage = AppleSecureStorage::new();

static MAC_TIME: MacTime = MacTime;
struct MacTime;
impl api::TimeService for MacTime {
    fn monotonic_now(&self) -> core::time::Duration {
        static START: Lazy<std::time::Instant> = Lazy::new(std::time::Instant::now);
        START.elapsed()
    }
}

static MAC_WEB_VIEW: AppleWebViewService = AppleWebViewService::new();

static MAC_TELEPHONY: MacTelephony = MacTelephony;
struct MacTelephony;
impl api::telephony::TelephonyService for MacTelephony {
    fn home_country_iso_code(&self) -> Option<alloc::string::String> {
        None
    }
}

static MAC_MEDIA_LIBRARY: AppleMediaLibraryManager = AppleMediaLibraryManager;

static MAC_NETWORK_STATUS: MacNetworkStatus = MacNetworkStatus;
struct MacNetworkStatus;
impl api::network_status::NetworkStatusService for MacNetworkStatus {
    fn current_status(&self) -> api::network_status::NetworkStatus {
        current_network_status()
    }
    fn subscribe(&self, f: alloc::boxed::Box<dyn Fn(api::network_status::NetworkStatus) + Send>) {
        start_network_status_monitor();
        let callback = Arc::new(Mutex::new(f));
        network_status_callbacks().push(Arc::clone(&callback));
        let status = current_network_status();
        let callback = callback.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        callback(status);
    }
}

fn permission_callbacks() -> std::sync::MutexGuard<'static, alloc::vec::Vec<PermissionStatusCallback>> {
    PERMISSION_CALLBACKS.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

extern "C" fn permission_cb(domain: u32, status: u32) {
    let Some(domain) = permission_domain_from_apple_code(domain) else {
        return;
    };
    let status = permission_status_from_apple_code(status);
    let callbacks: alloc::vec::Vec<PermissionStatusCallback> =
        permission_callbacks().iter().map(Arc::clone).collect();
    for callback in callbacks {
        let callback = callback.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        callback(domain, status);
    }
}

fn start_permission_bridge() {
    PERMISSION_INIT.call_once(|| unsafe {
        macos_set_permission_callback(Some(permission_cb));
    });
}

fn network_status_callbacks() -> std::sync::MutexGuard<'static, alloc::vec::Vec<NetworkStatusCallback>> {
    NETWORK_STATUS_CALLBACKS.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

extern "C" fn network_status_cb(connected: u8, interfaces: u32) {
    let status = network_status_from_apple_interface_mask(connected != 0, interfaces);
    let callbacks: alloc::vec::Vec<NetworkStatusCallback> =
        network_status_callbacks().iter().map(Arc::clone).collect();
    for callback in callbacks {
        let callback = callback.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        callback(status);
    }
}

fn start_network_status_monitor() {
    NETWORK_STATUS_INIT.call_once(|| unsafe {
        macos_set_network_status_callback(Some(network_status_cb));
        let _ = macos_start_network_monitor();
    });
}

fn current_network_status() -> api::network_status::NetworkStatus {
    start_network_status_monitor();
    let mut connected = 0;
    let mut interfaces = 0;
    let ok = unsafe { macos_network_status(&mut connected, &mut interfaces) };
    if ok == 0 {
        return network_status_from_apple_interface_mask(false, 0);
    }
    network_status_from_apple_interface_mask(connected != 0, interfaces)
}

fn standard_path(path: api::StandardPath) -> alloc::string::String {
    let mut dir = match path {
        api::StandardPath::Documents => {
            let mut dir = home_dir();
            dir.push("Library");
            dir.push("Application Support");
            dir.push("Oxide");
            dir
        }
        api::StandardPath::Cache => {
            let mut dir = home_dir();
            dir.push("Library");
            dir.push("Caches");
            dir.push("Oxide");
            dir
        }
        api::StandardPath::Temporary => {
            let mut dir = std::env::temp_dir();
            dir.push("Oxide");
            dir
        }
    };
    if std::fs::create_dir_all(&dir).is_err() {
        dir = std::env::temp_dir();
    }
    dir.to_string_lossy().into_owned()
}

fn home_dir() -> std::path::PathBuf {
    std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir)
}

pub fn install_current_platform() -> Arc<MacPlatform> {
    let platform = Arc::new(MacPlatform::new());
    let shared: Arc<dyn api::Platform + Send + Sync> = platform.clone();
    api::set_current_platform(shared);
    platform
}

#[must_use]
pub const fn platform() -> MacPlatform {
    MacPlatform::new()
}
