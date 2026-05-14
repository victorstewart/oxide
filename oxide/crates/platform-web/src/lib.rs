//! Oxide browser platform adapter for WebAssembly hosts.

#![forbid(unsafe_code)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use oxide_platform_api as api;
#[cfg(target_arch = "wasm32")]
use std::cell::{Cell, RefCell};
#[cfg(target_arch = "wasm32")]
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::{spawn_local, JsFuture};

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const SECURE_STORAGE_PREFIX: &str = "oxide.secure.";

/// Browser platform implementation.
#[derive(Debug, Default)]
pub struct WebPlatform;

impl WebPlatform {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[cfg(target_arch = "wasm32")]
pub struct BrowserMediaStream {
    stream: web_sys::MediaStream,
}

#[cfg(target_arch = "wasm32")]
impl BrowserMediaStream {
    #[must_use]
    pub fn id(&self) -> String {
        self.stream.id()
    }

    #[must_use]
    pub fn is_active(&self) -> bool {
        self.stream.active()
    }

    #[must_use]
    pub fn track_count(&self) -> u32 {
        self.stream.get_tracks().length()
    }

    #[must_use]
    pub fn stream(&self) -> web_sys::MediaStream {
        self.stream.clone()
    }

    pub fn stop(&self) {
        stop_media_stream(&self.stream);
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for BrowserMediaStream {
    fn drop(&mut self) {
        stop_media_stream(&self.stream);
    }
}

#[cfg(target_arch = "wasm32")]
pub async fn start_browser_media_stream(
    video: bool,
    audio: bool,
) -> Result<BrowserMediaStream, api::PlatformError> {
    if !video && !audio {
        return Err(api::PlatformError::Invalid("media stream requires audio or video"));
    }
    let Some(window) = web_sys::window() else {
        return Err(unsupported("window is unavailable"));
    };
    let media_devices = window
        .navigator()
        .media_devices()
        .map_err(|value| js_unknown("mediaDevices unavailable", value))?;
    let constraints = web_sys::MediaStreamConstraints::new();
    constraints.set_video(&JsValue::from_bool(video));
    constraints.set_audio(&JsValue::from_bool(audio));
    let promise = media_devices
        .get_user_media_with_constraints(&constraints)
        .map_err(|value| js_unknown("getUserMedia failed", value))?;
    let stream = JsFuture::from(promise)
        .await
        .map_err(|value| js_unknown("getUserMedia rejected", value))?
        .dyn_into::<web_sys::MediaStream>()
        .map_err(|_| {
            api::PlatformError::Unknown(String::from("getUserMedia did not return MediaStream"))
        })?;
    Ok(BrowserMediaStream { stream })
}

/// Installs the browser platform into the process-global platform registry.
pub fn install_current_platform() -> Arc<WebPlatform> {
    refresh_rate_probe_start();
    let platform = Arc::new(WebPlatform::new());
    let shared: Arc<dyn api::Platform + Send + Sync> = platform.clone();
    api::set_current_platform(shared);
    let clipboard: Arc<dyn api::clipboard::ClipboardProvider> = platform.clone();
    api::clipboard::set_clipboard_provider(clipboard);
    platform
}

/// Returns a standalone browser platform value.
#[must_use]
pub const fn platform() -> WebPlatform {
    WebPlatform::new()
}

impl api::clipboard::ClipboardProvider for WebPlatform {
    fn read_string(&self) -> Option<String> {
        clipboard_cached_get()
    }

    fn write_string(&self, value: &str) {
        clipboard_cached_set(value);
        clipboard_write_browser(value);
    }
}

impl api::Platform for WebPlatform {
    fn run_app(&self, _app: Box<dyn api::App>) -> ! {
        panic!("WebPlatform::run_app is handled by oxide-host-web requestAnimationFrame runtime")
    }

    fn request_redraw(&self) {
        dispatch_window_event("oxide-redraw");
    }

    fn set_high_refresh(&self, enable: bool) {
        if enable {
            refresh_rate_probe_start();
        }
    }

    fn set_idle_timer_disabled(&self, disabled: bool) {
        wake_lock_set(disabled);
    }

    fn open_system_settings(&self) {}

    fn open_external_url(&self, url: &str) -> Result<(), api::PlatformError> {
        open_external_url_browser(url)
    }

    fn clipboard_get(&self) -> Option<String> {
        clipboard_cached_get()
    }

    fn clipboard_set(&self, value: &str) {
        clipboard_cached_set(value);
        clipboard_write_browser(value);
    }

    fn ime_show(&self) {
        dispatch_window_event("oxide-ime-show");
    }

    fn ime_hide(&self) {
        dispatch_window_event("oxide-ime-hide");
    }

    fn device_caps(&self) -> api::DeviceCaps {
        api::DeviceCaps {
            max_framerate_hz: browser_refresh_rate_hz(),
            supports_edr: false,
            supports_msaa4x: false,
            native_scale: browser_device_scale(),
            color_space: api::ColorSpace::Srgb,
            a11y_reduce_motion: false,
        }
    }

    fn haptics(&self) -> Arc<dyn api::Haptics + Send + Sync> {
        web_haptics()
    }

    fn is_simulation(&self) -> bool {
        false
    }

    fn permissions(&self) -> &dyn api::Permissions {
        &WEB_PERMISSIONS
    }

    fn camera(&self) -> &dyn api::CameraManager {
        &WEB_CAMERA
    }

    fn bluetooth(&self) -> &dyn api::Bluetooth {
        &WEB_BLUETOOTH
    }

    fn bluetooth_with_restoration(&self, _restore_id: &str) -> Box<dyn api::Bluetooth> {
        Box::new(WebBluetooth)
    }

    fn location(&self) -> &dyn api::LocationService {
        &WEB_LOCATION
    }

    fn motion(&self) -> &dyn api::MotionService {
        &WEB_MOTION
    }

    fn push(&self) -> &dyn api::PushManager {
        &WEB_PUSH
    }

    fn capabilities(&self) -> api::Capabilities {
        browser_capabilities()
    }

    fn networking(&self) -> &dyn api::Networking {
        &WEB_NETWORKING
    }

    fn paths(&self) -> &dyn api::PathService {
        &WEB_PATHS
    }

    fn secure_storage(&self) -> &dyn api::SecureStorage {
        &WEB_SECURE_STORAGE
    }

    fn time(&self) -> &dyn api::TimeService {
        &WEB_TIME
    }

    fn web_view_service(&self) -> &dyn api::web_view::WebViewService {
        &WEB_VIEW_SERVICE
    }

    fn telephony(&self) -> &dyn api::telephony::TelephonyService {
        &WEB_TELEPHONY
    }

    fn media_library(&self) -> &dyn api::media_library::MediaLibrary {
        &WEB_MEDIA_LIBRARY
    }

    fn network_status(&self) -> &dyn api::network_status::NetworkStatusService {
        &WEB_NETWORK_STATUS
    }
}

static WEB_PERMISSIONS: WebPermissions = WebPermissions;
struct WebPermissions;

impl api::Permissions for WebPermissions {
    fn request(&self, domain: api::PermissionDomain) {
        if domain == api::PermissionDomain::Location {
            api::LocationService::request_once(&WEB_LOCATION);
        }
    }

    fn status(&self, domain: api::PermissionDomain) -> api::PermissionStatus {
        match domain {
            api::PermissionDomain::Location => location_permission_status(),
            _ => api::PermissionStatus::Denied,
        }
    }

    fn subscribe(&self, f: Box<dyn Fn(api::PermissionDomain, api::PermissionStatus) + Send>) {
        permission_subscribe(f);
    }
}

static WEB_CAMERA: WebCamera = WebCamera;
struct WebCamera;

impl api::CameraManager for WebCamera {
    fn start_stream(
        &self,
        _cfg: api::CameraConfig,
        _on_frame: Box<dyn Fn(api::CameraFrame) + Send>,
        _on_audio: Option<Box<dyn Fn(api::AudioSample) + Send>>,
    ) -> Result<Box<dyn api::CameraStream + Send>, api::PlatformError> {
        Err(unsupported("web camera stream is not supported by this backend"))
    }

    fn start_recording(
        &self,
        _options: api::RecordingOptions,
        _on_event: Box<dyn Fn(api::RecordingEvent) + Send>,
    ) -> Result<Box<dyn api::CameraRecording + Send>, api::PlatformError> {
        Err(unsupported("web camera recording is not supported by this backend"))
    }

    fn select_device(&self, _device: api::CameraDevice) {}

    fn set_fps(&self, _fps: u32) {}

    fn set_resolution(&self, _width: u32, _height: u32) {}

    fn set_mode(&self, _mode: api::CaptureMode) {}
}

static WEB_BLUETOOTH: WebBluetooth = WebBluetooth;
struct WebBluetooth;

impl api::Bluetooth for WebBluetooth {
    fn powered_on(&self) -> bool {
        false
    }

    fn subscribe_events(&self, _f: Box<dyn Fn(api::BluetoothEvent) + Send>) {}

    fn start_scan(&self, _opts: &api::ScanOptions) {}

    fn stop_scan(&self) {}

    fn connect(&self, _id: api::PeripheralId) {}

    fn disconnect(&self, _id: api::PeripheralId) {}

    fn read(
        &self,
        _id: api::PeripheralId,
        _chr: api::GattChar,
    ) -> Result<Vec<u8>, api::PlatformError> {
        Err(unsupported("web bluetooth read is not supported by this backend"))
    }

    fn write(
        &self,
        _id: api::PeripheralId,
        _chr: api::GattChar,
        _data: &[u8],
        _with_response: bool,
    ) -> Result<(), api::PlatformError> {
        Err(unsupported("web bluetooth write is not supported by this backend"))
    }

    fn notify(
        &self,
        _id: api::PeripheralId,
        _chr: api::GattChar,
        _enable: bool,
    ) -> Result<(), api::PlatformError> {
        Err(unsupported("web bluetooth notify is not supported by this backend"))
    }

    fn advertise_start(&self, _name: &str, _services: &[api::BleUuid]) {}

    fn advertise_stop(&self) {}

    fn cached_peripherals(&self) -> Vec<api::BleCacheEntry> {
        Vec::new()
    }
}

static WEB_LOCATION: WebLocation = WebLocation;
struct WebLocation;

impl api::LocationService for WebLocation {
    fn start(&self, opts: api::LocationOptions) -> Result<(), api::PlatformError> {
        location_start(opts)
    }

    fn stop(&self) {
        location_stop();
    }

    fn request_once(&self) {
        location_request_once();
    }

    fn last(&self) -> Option<api::LocationReading> {
        location_last()
    }

    fn subscribe(&self, f: Box<dyn Fn(api::LocationEvent) + Send>) {
        location_subscribe(f);
    }

    fn history(&self) -> Vec<api::LocationReading> {
        location_history()
    }

    fn region_tracker(&self) -> Option<Box<dyn api::GeoRegionTracker>> {
        None
    }

    fn set_accuracy(&self, accuracy: api::LocationAccuracy) -> Result<(), api::PlatformError> {
        location_set_accuracy(accuracy)
    }
}

static WEB_MOTION: WebMotion = WebMotion;
struct WebMotion;

impl api::MotionService for WebMotion {
    fn start(&self) -> Result<(), api::PlatformError> {
        Err(unsupported("web motion is not supported by this backend"))
    }

    fn stop(&self) {}

    fn is_running(&self) -> bool {
        false
    }

    fn subscribe(&self, _f: Box<dyn Fn(api::MotionSample) + Send>) {}

    fn pressure_history(&self) -> Vec<api::MotionSample> {
        Vec::new()
    }
}

static WEB_PUSH: WebPush = WebPush;
struct WebPush;

impl api::PushManager for WebPush {
    fn register(&self) {}

    fn device_token(&self) -> Option<api::PushToken> {
        None
    }

    fn subscribe(&self, _f: Box<dyn Fn(api::PushNotification) + Send>) {}

    fn set_badge(&self, _count: i32) {}

    fn clear_badge(&self) {}
}

static WEB_NETWORKING: WebNetworking = WebNetworking;
struct WebNetworking;

impl api::Networking for WebNetworking {
    fn connect_tcp(
        &self,
        _options: api::ConnectionOptions,
        _on_event: Box<dyn Fn(api::ConnectionEvent) + Send>,
    ) -> Result<Box<dyn api::Connection + Send>, api::PlatformError> {
        Err(unsupported("raw TCP is unavailable in browser WebAssembly"))
    }

    fn connect_quic(
        &self,
        _options: api::ConnectionOptions,
        _on_event: Box<dyn Fn(api::ConnectionEvent) + Send>,
    ) -> Result<Box<dyn api::ConnectionGroup + Send>, api::PlatformError> {
        Err(unsupported("raw QUIC is unavailable in browser WebAssembly"))
    }

    fn bind_udp(
        &self,
        _local_port: u16,
        _on_event: Box<dyn Fn(api::UdpEvent) + Send>,
    ) -> Result<Box<dyn api::UdpSocket + Send>, api::PlatformError> {
        Err(unsupported("raw UDP is unavailable in browser WebAssembly"))
    }
}

static WEB_PATHS: WebPaths = WebPaths;
struct WebPaths;

impl api::PathService for WebPaths {
    fn get(&self, path: api::StandardPath) -> String {
        match path {
            api::StandardPath::Documents => String::from("oxide://documents"),
            api::StandardPath::Cache => String::from("oxide://cache"),
            api::StandardPath::Temporary => String::from("oxide://temporary"),
        }
    }
}

static WEB_SECURE_STORAGE: WebSecureStorage = WebSecureStorage;
struct WebSecureStorage;

impl api::SecureStorage for WebSecureStorage {
    fn save<'a>(
        &'a self,
        key: &'a str,
        data: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<(), api::PlatformError>> + Send + 'a>> {
        let result = local_storage_save(key, data);
        Box::pin(async move { result })
    }

    fn load<'a>(
        &'a self,
        key: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Vec<u8>>, api::PlatformError>> + Send + 'a>>
    {
        let result = local_storage_load(key);
        Box::pin(async move { result })
    }

    fn delete<'a>(
        &'a self,
        key: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), api::PlatformError>> + Send + 'a>> {
        let result = local_storage_delete(key);
        Box::pin(async move { result })
    }
}

static WEB_TIME: WebTime = WebTime;
struct WebTime;

impl api::TimeService for WebTime {
    fn monotonic_now(&self) -> Duration {
        browser_monotonic_now()
    }
}

static WEB_VIEW_SERVICE: WebViewService = WebViewService;
struct WebViewService;

#[derive(Debug)]
struct WebFrameView {
    id: u32,
}

impl Drop for WebFrameView {
    fn drop(&mut self) {
        web_view_close(self.id);
    }
}

impl api::web_view::WebView for WebFrameView {
    fn execute_script(
        &self,
        script: &str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<String>, api::PlatformError>> + Send + '_>> {
        let result = web_view_execute_script(self.id, script);
        Box::pin(async move { result })
    }

    fn close(&self) {
        web_view_close(self.id);
    }
}

impl api::web_view::WebViewService for WebViewService {
    fn create_view(
        &self,
        url: &str,
        on_event: Box<dyn Fn(api::web_view::WebViewEvent) + Send>,
    ) -> Result<Box<dyn api::web_view::WebView + Send>, api::PlatformError> {
        let id = web_view_create(url, on_event)?;
        Ok(Box::new(WebFrameView { id }))
    }
}

static WEB_TELEPHONY: WebTelephony = WebTelephony;
struct WebTelephony;

impl api::telephony::TelephonyService for WebTelephony {
    fn home_country_iso_code(&self) -> Option<String> {
        None
    }
}

static WEB_MEDIA_LIBRARY: WebMediaLibrary = WebMediaLibrary;
struct WebMediaLibrary;

impl api::media_library::MediaLibrary for WebMediaLibrary {
    fn query_assets(
        &self,
        _asset_type: api::media_library::AssetType,
        _limit: u32,
        _offset: u32,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Vec<api::media_library::MediaAsset>, api::PlatformError>>
                + Send
                + '_,
        >,
    > {
        Box::pin(async { Err(unsupported("media library is not supported by this backend")) })
    }

    fn request_image_data(
        &self,
        _id: &api::media_library::AssetId,
        _quality: api::media_library::ImageQuality,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<api::media_library::AssetData, api::PlatformError>>
                + Send
                + '_,
        >,
    > {
        Box::pin(async {
            Err(unsupported("media library images are not supported by this backend"))
        })
    }

    fn request_video_data(
        &self,
        _id: &api::media_library::AssetId,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<api::media_library::AssetData, api::PlatformError>>
                + Send
                + '_,
        >,
    > {
        Box::pin(async {
            Err(unsupported("media library videos are not supported by this backend"))
        })
    }
}

static WEB_NETWORK_STATUS: WebNetworkStatus = WebNetworkStatus;
struct WebNetworkStatus;

impl api::network_status::NetworkStatusService for WebNetworkStatus {
    fn current_status(&self) -> api::network_status::NetworkStatus {
        api::network_status::NetworkStatus {
            is_connected: browser_online(),
            interfaces: api::network_status::NetworkInterface::empty(),
        }
    }

    fn subscribe(&self, f: Box<dyn Fn(api::network_status::NetworkStatus) + Send>) {
        network_subscribe(f);
    }
}

struct WebHaptics;

impl api::Haptics for WebHaptics {
    fn play(&self, pattern: api::HapticPattern) {
        vibrate(pattern_vibration_ms(pattern));
    }
}

fn web_haptics() -> Arc<dyn api::Haptics + Send + Sync> {
    static HAPTICS: OnceLock<Arc<WebHaptics>> = OnceLock::new();
    HAPTICS.get_or_init(|| Arc::new(WebHaptics)).clone()
}

fn pattern_vibration_ms(pattern: api::HapticPattern) -> u32 {
    match pattern {
        api::HapticPattern::ImpactLight | api::HapticPattern::Selection => 10,
        api::HapticPattern::ImpactMedium | api::HapticPattern::NotificationSuccess => 20,
        api::HapticPattern::ImpactHeavy | api::HapticPattern::NotificationWarning => 35,
        api::HapticPattern::NotificationError => 45,
    }
}

fn unsupported(message: &'static str) -> api::PlatformError {
    api::PlatformError::Unsupported(message)
}

fn clipboard_cell() -> &'static RwLock<Option<String>> {
    static CELL: OnceLock<RwLock<Option<String>>> = OnceLock::new();
    CELL.get_or_init(|| RwLock::new(None))
}

fn clipboard_cached_get() -> Option<String> {
    clipboard_cell().read().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
}

fn clipboard_cached_set(value: &str) {
    let mut guard = clipboard_cell().write().unwrap_or_else(std::sync::PoisonError::into_inner);
    *guard = Some(value.to_owned());
}

#[must_use]
pub fn hex_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes.iter().copied() {
        out.push(TABLE[(byte >> 4) as usize] as char);
        out.push(TABLE[(byte & 0x0F) as usize] as char);
    }
    out
}

pub fn hex_decode(input: &str) -> Result<Vec<u8>, api::PlatformError> {
    let bytes = input.as_bytes();
    if bytes.len() % 2 != 0 {
        return Err(api::PlatformError::Invalid("hex string has odd length"));
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let high = hex_nibble(pair[0])?;
        let low = hex_nibble(pair[1])?;
        out.push((high << 4) | low);
    }
    Ok(out)
}

#[must_use]
pub fn refresh_rate_hz_from_frame_deltas(deltas_ms: &[f64]) -> u32 {
    let mut deltas = deltas_ms
        .iter()
        .copied()
        .filter(|delta| delta.is_finite() && *delta > 0.0)
        .collect::<Vec<_>>();
    if deltas.is_empty() {
        return 60;
    }
    deltas.sort_by(|a, b| a.total_cmp(b));
    let median = deltas[deltas.len() / 2];
    if median <= 0.0 {
        60
    } else {
        (1000.0 / median).round().clamp(30.0, 240.0) as u32
    }
}

pub async fn clipboard_read_string_async() -> Result<Option<String>, api::PlatformError> {
    clipboard_read_browser_async().await
}

pub async fn clipboard_write_string_async(value: &str) -> Result<(), api::PlatformError> {
    clipboard_cached_set(value);
    clipboard_write_browser_async(value).await
}

fn hex_nibble(byte: u8) -> Result<u8, api::PlatformError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(api::PlatformError::Invalid("hex string contains non-hex digit")),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn storage_key(key: &str) -> String {
    let mut out = String::with_capacity(SECURE_STORAGE_PREFIX.len().saturating_add(key.len()));
    out.push_str(SECURE_STORAGE_PREFIX);
    out.push_str(key);
    out
}

#[cfg(target_arch = "wasm32")]
const LOCATION_HISTORY_LIMIT: usize = 256;

#[cfg(target_arch = "wasm32")]
type LocationCallback = Box<dyn Fn(api::LocationEvent) + Send>;
#[cfg(target_arch = "wasm32")]
type NetworkCallback = Box<dyn Fn(api::network_status::NetworkStatus) + Send>;
#[cfg(target_arch = "wasm32")]
type PermissionCallback = Box<dyn Fn(api::PermissionDomain, api::PermissionStatus) + Send>;

#[cfg(target_arch = "wasm32")]
thread_local! {
   static LOCATION_STATE: RefCell<LocationState> = RefCell::new(LocationState::default());
   static LOCATION_SUBSCRIBERS: RefCell<Vec<LocationCallback>> = RefCell::new(Vec::new());
   static LOCATION_ONESHOT: RefCell<Option<LocationOneShot>> = RefCell::new(None);
   static LOCATION_ONESHOT_NEXT_ID: Cell<u32> = Cell::new(1);
   static NETWORK_SUBSCRIBERS: RefCell<Vec<NetworkCallback>> = RefCell::new(Vec::new());
   static NETWORK_LISTENERS: RefCell<Option<NetworkListeners>> = RefCell::new(None);
   static PERMISSION_SUBSCRIBERS: RefCell<Vec<PermissionCallback>> = RefCell::new(Vec::new());
   static WAKE_LOCK_SENTINEL: RefCell<Option<JsValue>> = RefCell::new(None);
   static REFRESH_RATE_HZ: Cell<u32> = Cell::new(60);
   static REFRESH_LAST_MS: Cell<f64> = Cell::new(0.0);
   static REFRESH_SAMPLES: RefCell<Vec<f64>> = RefCell::new(Vec::new());
   static REFRESH_PROBE: RefCell<Option<Closure<dyn FnMut(f64)>>> = RefCell::new(None);
   static WEB_VIEW_NEXT_ID: Cell<u32> = Cell::new(1);
   static WEB_VIEWS: RefCell<HashMap<u32, WebFrameEntry>> = RefCell::new(HashMap::new());
}

#[cfg(target_arch = "wasm32")]
struct LocationState {
    watch_id: Option<i32>,
    success: Option<Closure<dyn FnMut(web_sys::Position)>>,
    error: Option<Closure<dyn FnMut(web_sys::PositionError)>>,
    last: Option<api::LocationReading>,
    history: Vec<api::LocationReading>,
    permission: api::PermissionStatus,
    options: api::LocationOptions,
}

#[cfg(target_arch = "wasm32")]
impl Default for LocationState {
    fn default() -> Self {
        Self {
            watch_id: None,
            success: None,
            error: None,
            last: None,
            history: Vec::new(),
            permission: api::PermissionStatus::NotDetermined,
            options: api::LocationOptions::default(),
        }
    }
}

#[cfg(target_arch = "wasm32")]
struct LocationOneShot {
    id: u32,
    _success: Closure<dyn FnMut(web_sys::Position)>,
    _error: Closure<dyn FnMut(web_sys::PositionError)>,
}

#[cfg(target_arch = "wasm32")]
struct NetworkListeners {
    _online: Closure<dyn FnMut(web_sys::Event)>,
    _offline: Closure<dyn FnMut(web_sys::Event)>,
}

#[cfg(target_arch = "wasm32")]
struct WebFrameEntry {
    iframe: web_sys::HtmlIFrameElement,
    _load: Closure<dyn FnMut(web_sys::Event)>,
    _error: Closure<dyn FnMut(web_sys::Event)>,
    on_event: Option<Box<dyn Fn(api::web_view::WebViewEvent) + Send>>,
}

#[cfg(target_arch = "wasm32")]
fn browser_capabilities() -> api::Capabilities {
    let mut caps = api::Capabilities::empty();
    if browser_geolocation().is_ok() {
        caps |= api::Capabilities::LOCATION;
    }
    if browser_supports_hover_pointer() {
        caps |= api::Capabilities::HOVER_POINTER;
    }
    caps
}

#[cfg(not(target_arch = "wasm32"))]
fn browser_capabilities() -> api::Capabilities {
    api::Capabilities::empty()
}

#[cfg(target_arch = "wasm32")]
fn browser_supports_hover_pointer() -> bool {
    web_sys::window()
        .and_then(|window| window.match_media("(hover: hover)").ok().flatten())
        .map(|query| query.matches())
        .unwrap_or(false)
}

#[cfg(target_arch = "wasm32")]
fn browser_refresh_rate_hz() -> u32 {
    refresh_rate_probe_start();
    REFRESH_RATE_HZ.with(Cell::get)
}

#[cfg(not(target_arch = "wasm32"))]
fn browser_refresh_rate_hz() -> u32 {
    60
}

#[cfg(target_arch = "wasm32")]
fn refresh_rate_probe_start() {
    let already_running = REFRESH_PROBE.with(|probe| probe.borrow().is_some());
    if already_running {
        return;
    }
    REFRESH_LAST_MS.with(|last| last.set(0.0));
    REFRESH_SAMPLES.with(|samples| samples.borrow_mut().clear());

    let closure = Closure::wrap(Box::new(move |timestamp_ms: f64| {
        refresh_rate_probe_sample(timestamp_ms);
    }) as Box<dyn FnMut(f64)>);
    REFRESH_PROBE.with(|probe| {
        *probe.borrow_mut() = Some(closure);
    });
    refresh_rate_probe_request_next();
}

#[cfg(not(target_arch = "wasm32"))]
fn refresh_rate_probe_start() {}

#[cfg(target_arch = "wasm32")]
fn refresh_rate_probe_sample(timestamp_ms: f64) {
    let last = REFRESH_LAST_MS.with(Cell::get);
    if last > 0.0 && timestamp_ms > last {
        REFRESH_SAMPLES.with(|samples| samples.borrow_mut().push(timestamp_ms - last));
    }
    REFRESH_LAST_MS.with(|cell| cell.set(timestamp_ms));

    let done = REFRESH_SAMPLES.with(|samples| samples.borrow().len() >= 12);
    if done {
        let hz =
            REFRESH_SAMPLES.with(|samples| refresh_rate_hz_from_frame_deltas(&samples.borrow()));
        REFRESH_RATE_HZ.with(|rate| rate.set(hz));
        REFRESH_PROBE.with(|probe| {
            *probe.borrow_mut() = None;
        });
    } else {
        refresh_rate_probe_request_next();
    }
}

#[cfg(target_arch = "wasm32")]
fn refresh_rate_probe_request_next() {
    let Some(window) = web_sys::window() else {
        return;
    };
    REFRESH_PROBE.with(|probe| {
        if let Some(closure) = probe.borrow().as_ref() {
            let _ = window.request_animation_frame(closure.as_ref().unchecked_ref());
        }
    });
}

#[cfg(target_arch = "wasm32")]
fn permission_subscribe(f: Box<dyn Fn(api::PermissionDomain, api::PermissionStatus) + Send>) {
    PERMISSION_SUBSCRIBERS.with(|subscribers| subscribers.borrow_mut().push(f));
}

#[cfg(not(target_arch = "wasm32"))]
fn permission_subscribe(_f: Box<dyn Fn(api::PermissionDomain, api::PermissionStatus) + Send>) {}

#[cfg(target_arch = "wasm32")]
fn permission_notify(domain: api::PermissionDomain, status: api::PermissionStatus) {
    let callbacks =
        PERMISSION_SUBSCRIBERS.with(|subscribers| std::mem::take(&mut *subscribers.borrow_mut()));
    for callback in callbacks.iter() {
        callback(domain, status);
    }
    PERMISSION_SUBSCRIBERS.with(|subscribers| subscribers.borrow_mut().extend(callbacks));
}

#[cfg(target_arch = "wasm32")]
fn location_permission_status() -> api::PermissionStatus {
    LOCATION_STATE.with(|state| state.borrow().permission)
}

#[cfg(not(target_arch = "wasm32"))]
fn location_permission_status() -> api::PermissionStatus {
    api::PermissionStatus::Denied
}

#[cfg(target_arch = "wasm32")]
fn location_set_permission(status: api::PermissionStatus) {
    let changed = LOCATION_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.permission == status {
            return false;
        }
        state.permission = status;
        true
    });
    if changed {
        permission_notify(api::PermissionDomain::Location, status);
    }
}

#[cfg(target_arch = "wasm32")]
fn location_start(opts: api::LocationOptions) -> Result<(), api::PlatformError> {
    let restart = LOCATION_STATE.with(|state| {
        let state = state.borrow();
        state.watch_id.is_some() && state.options != opts
    });
    if restart {
        location_stop();
    }
    let already_running = LOCATION_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.watch_id.is_some() {
            state.options = opts;
            true
        } else {
            false
        }
    });
    if already_running {
        return Ok(());
    }

    let geolocation = browser_geolocation()?;
    let position_options = location_position_options(opts);
    let success = Closure::wrap(Box::new(move |position: web_sys::Position| {
        location_handle_position(position);
    }) as Box<dyn FnMut(web_sys::Position)>);
    let error = Closure::wrap(Box::new(move |error: web_sys::PositionError| {
        location_handle_error(error);
    }) as Box<dyn FnMut(web_sys::PositionError)>);
    let watch_id = geolocation
        .watch_position_with_error_callback_and_options(
            success.as_ref().unchecked_ref(),
            Some(error.as_ref().unchecked_ref()),
            &position_options,
        )
        .map_err(|value| js_unknown("geolocation watchPosition failed", value))?;

    LOCATION_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.watch_id = Some(watch_id);
        state.success = Some(success);
        state.error = Some(error);
        state.options = opts;
    });
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn location_start(_opts: api::LocationOptions) -> Result<(), api::PlatformError> {
    Err(unsupported("web location is unavailable on non-wasm targets"))
}

#[cfg(target_arch = "wasm32")]
fn location_stop() {
    let (watch_id, geolocation) = LOCATION_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let watch_id = state.watch_id.take();
        state.success = None;
        state.error = None;
        (watch_id, browser_geolocation().ok())
    });
    if let (Some(watch_id), Some(geolocation)) = (watch_id, geolocation) {
        geolocation.clear_watch(watch_id);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn location_stop() {}

#[cfg(target_arch = "wasm32")]
fn location_request_once() {
    if let Err(error) = location_request_once_inner() {
        location_notify(api::LocationEvent::Error(error));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn location_request_once() {}

#[cfg(target_arch = "wasm32")]
fn location_request_once_inner() -> Result<(), api::PlatformError> {
    let geolocation = browser_geolocation()?;
    let opts = LOCATION_STATE.with(|state| state.borrow().options);
    let position_options = location_position_options(opts);
    let id = LOCATION_ONESHOT_NEXT_ID.with(|next| {
        let id = next.get();
        next.set(id.wrapping_add(1).max(1));
        id
    });
    let success = Closure::wrap(Box::new(move |position: web_sys::Position| {
        location_handle_position(position);
        location_clear_oneshot(id);
    }) as Box<dyn FnMut(web_sys::Position)>);
    let error = Closure::wrap(Box::new(move |error: web_sys::PositionError| {
        location_handle_error(error);
        location_clear_oneshot(id);
    }) as Box<dyn FnMut(web_sys::PositionError)>);

    geolocation
        .get_current_position_with_error_callback_and_options(
            success.as_ref().unchecked_ref(),
            Some(error.as_ref().unchecked_ref()),
            &position_options,
        )
        .map_err(|value| js_unknown("geolocation getCurrentPosition failed", value))?;
    LOCATION_ONESHOT.with(|slot| {
        *slot.borrow_mut() = Some(LocationOneShot { id, _success: success, _error: error });
    });
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn location_clear_oneshot(id: u32) {
    LOCATION_ONESHOT.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.as_ref().map(|one_shot| one_shot.id) == Some(id) {
            *slot = None;
        }
    });
}

#[cfg(target_arch = "wasm32")]
fn location_last() -> Option<api::LocationReading> {
    LOCATION_STATE.with(|state| state.borrow().last)
}

#[cfg(not(target_arch = "wasm32"))]
fn location_last() -> Option<api::LocationReading> {
    None
}

#[cfg(target_arch = "wasm32")]
fn location_subscribe(f: Box<dyn Fn(api::LocationEvent) + Send>) {
    LOCATION_SUBSCRIBERS.with(|subscribers| subscribers.borrow_mut().push(f));
}

#[cfg(not(target_arch = "wasm32"))]
fn location_subscribe(_f: Box<dyn Fn(api::LocationEvent) + Send>) {}

#[cfg(target_arch = "wasm32")]
fn location_history() -> Vec<api::LocationReading> {
    LOCATION_STATE.with(|state| state.borrow().history.clone())
}

#[cfg(not(target_arch = "wasm32"))]
fn location_history() -> Vec<api::LocationReading> {
    Vec::new()
}

#[cfg(target_arch = "wasm32")]
fn location_set_accuracy(accuracy: api::LocationAccuracy) -> Result<(), api::PlatformError> {
    let restart_opts = LOCATION_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.options.accuracy = accuracy;
        state.options.precise = accuracy == api::LocationAccuracy::Precise;
        if state.watch_id.is_some() {
            Some(state.options)
        } else {
            None
        }
    });
    if let Some(opts) = restart_opts {
        location_stop();
        location_start(opts)?;
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn location_set_accuracy(_accuracy: api::LocationAccuracy) -> Result<(), api::PlatformError> {
    Err(unsupported("location accuracy control unavailable on non-wasm targets"))
}

#[cfg(target_arch = "wasm32")]
fn location_position_options(opts: api::LocationOptions) -> web_sys::PositionOptions {
    let position_options = web_sys::PositionOptions::new();
    position_options
        .set_enable_high_accuracy(opts.precise || opts.accuracy == api::LocationAccuracy::Precise);
    position_options.set_maximum_age(match opts.accuracy {
        api::LocationAccuracy::Reduced | api::LocationAccuracy::LowPower => 60_000,
        api::LocationAccuracy::Balanced => 10_000,
        api::LocationAccuracy::Precise => 0,
    });
    position_options.set_timeout(30_000);
    position_options
}

#[cfg(target_arch = "wasm32")]
fn location_handle_position(position: web_sys::Position) {
    let reading = location_reading_from_position(&position);
    LOCATION_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.last = Some(reading);
        if state.history.len() >= LOCATION_HISTORY_LIMIT {
            let remove =
                state.history.len().saturating_add(1).saturating_sub(LOCATION_HISTORY_LIMIT);
            state.history.drain(0..remove);
        }
        state.history.push(reading);
    });
    location_set_permission(api::PermissionStatus::Authorized);
    location_notify(api::LocationEvent::Update(reading));
}

#[cfg(target_arch = "wasm32")]
fn location_handle_error(error: web_sys::PositionError) {
    let platform_error = location_error_from_position(&error);
    if error.code() == web_sys::PositionError::PERMISSION_DENIED {
        location_set_permission(api::PermissionStatus::Denied);
    }
    location_notify(api::LocationEvent::Error(platform_error));
}

#[cfg(target_arch = "wasm32")]
fn location_reading_from_position(position: &web_sys::Position) -> api::LocationReading {
    let coords = position.coords();
    api::LocationReading {
        latitude_deg: coords.latitude(),
        longitude_deg: coords.longitude(),
        altitude_m: finite_f64_to_f32(coords.altitude().unwrap_or(0.0)),
        horizontal_accuracy_m: finite_f64_to_f32(coords.accuracy()),
        vertical_accuracy_m: finite_f64_to_f32(coords.altitude_accuracy().unwrap_or(0.0)),
        speed_mps: finite_f64_to_f32(coords.speed().unwrap_or(0.0)),
        course_deg: finite_f64_to_f32(coords.heading().unwrap_or(0.0)),
        timestamp_ms: finite_f64_to_u64(position.timestamp()),
    }
}

#[cfg(target_arch = "wasm32")]
fn location_error_from_position(error: &web_sys::PositionError) -> api::PlatformError {
    match error.code() {
        web_sys::PositionError::PERMISSION_DENIED => {
            api::PlatformError::PermissionDenied("browser geolocation denied")
        }
        web_sys::PositionError::POSITION_UNAVAILABLE => {
            api::PlatformError::NotFound("browser geolocation unavailable")
        }
        web_sys::PositionError::TIMEOUT => api::PlatformError::Unknown(error.message()),
        _ => api::PlatformError::Unknown(error.message()),
    }
}

#[cfg(target_arch = "wasm32")]
fn location_notify(event: api::LocationEvent) {
    let callbacks =
        LOCATION_SUBSCRIBERS.with(|subscribers| std::mem::take(&mut *subscribers.borrow_mut()));
    for callback in callbacks.iter() {
        callback(event.clone());
    }
    LOCATION_SUBSCRIBERS.with(|subscribers| subscribers.borrow_mut().extend(callbacks));
}

#[cfg(target_arch = "wasm32")]
fn browser_geolocation() -> Result<web_sys::Geolocation, api::PlatformError> {
    web_sys::window()
        .ok_or(unsupported("window is unavailable"))?
        .navigator()
        .geolocation()
        .map_err(|_| unsupported("browser geolocation is unavailable"))
}

#[cfg(target_arch = "wasm32")]
fn finite_f64_to_f32(value: f64) -> f32 {
    if value.is_finite() {
        value as f32
    } else {
        0.0
    }
}

#[cfg(target_arch = "wasm32")]
fn finite_f64_to_u64(value: f64) -> u64 {
    if !value.is_finite() || value <= 0.0 {
        0
    } else if value >= u64::MAX as f64 {
        u64::MAX
    } else {
        value.round() as u64
    }
}

#[cfg(target_arch = "wasm32")]
fn network_subscribe(f: Box<dyn Fn(api::network_status::NetworkStatus) + Send>) {
    NETWORK_SUBSCRIBERS.with(|subscribers| subscribers.borrow_mut().push(f));
    network_install_listeners();
}

#[cfg(not(target_arch = "wasm32"))]
fn network_subscribe(_f: Box<dyn Fn(api::network_status::NetworkStatus) + Send>) {}

#[cfg(target_arch = "wasm32")]
fn network_install_listeners() {
    let installed = NETWORK_LISTENERS.with(|listeners| listeners.borrow().is_some());
    if installed {
        return;
    }
    let Some(window) = web_sys::window() else {
        return;
    };
    let target: &web_sys::EventTarget = window.unchecked_ref();
    let online = Closure::wrap(Box::new(move |_event: web_sys::Event| {
        network_notify();
    }) as Box<dyn FnMut(web_sys::Event)>);
    let offline = Closure::wrap(Box::new(move |_event: web_sys::Event| {
        network_notify();
    }) as Box<dyn FnMut(web_sys::Event)>);
    let _ = target.add_event_listener_with_callback("online", online.as_ref().unchecked_ref());
    let _ = target.add_event_listener_with_callback("offline", offline.as_ref().unchecked_ref());
    NETWORK_LISTENERS.with(|listeners| {
        *listeners.borrow_mut() = Some(NetworkListeners { _online: online, _offline: offline });
    });
}

#[cfg(target_arch = "wasm32")]
fn network_notify() {
    let status = api::network_status::NetworkStatus {
        is_connected: browser_online(),
        interfaces: api::network_status::NetworkInterface::empty(),
    };
    let callbacks =
        NETWORK_SUBSCRIBERS.with(|subscribers| std::mem::take(&mut *subscribers.borrow_mut()));
    for callback in callbacks.iter() {
        callback(status);
    }
    NETWORK_SUBSCRIBERS.with(|subscribers| subscribers.borrow_mut().extend(callbacks));
}

#[cfg(target_arch = "wasm32")]
fn web_view_create(
    url: &str,
    on_event: Box<dyn Fn(api::web_view::WebViewEvent) + Send>,
) -> Result<u32, api::PlatformError> {
    let window = web_sys::window().ok_or(unsupported("window is unavailable"))?;
    let href = web_view_same_origin_url(&window, url)?;
    let document = window.document().ok_or(unsupported("document is unavailable"))?;
    let body = document.body().ok_or(unsupported("document body is unavailable"))?;
    let iframe = document
        .create_element("iframe")
        .map_err(|value| js_unknown("iframe creation failed", value))?
        .dyn_into::<web_sys::HtmlIFrameElement>()
        .map_err(|_| {
            api::PlatformError::Unknown(String::from("created element was not an iframe"))
        })?;
    let id = web_view_next_id();

    iframe.set_name(&format!("oxide-web-view-{id}"));
    let element: &web_sys::Element = iframe.unchecked_ref();
    element
        .set_attribute("aria-hidden", "true")
        .map_err(|value| js_unknown("iframe attribute set failed", value))?;
    let style = iframe.style();
    let _ = style.set_property("position", "absolute");
    let _ = style.set_property("width", "1px");
    let _ = style.set_property("height", "1px");
    let _ = style.set_property("left", "-10000px");
    let _ = style.set_property("top", "-10000px");
    let _ = style.set_property("border", "0");
    let _ = style.set_property("visibility", "hidden");

    let load = Closure::wrap(Box::new(move |_event: web_sys::Event| {
        web_view_emit(id, api::web_view::WebViewEvent::LoadFinished);
    }) as Box<dyn FnMut(web_sys::Event)>);
    let error = Closure::wrap(Box::new(move |_event: web_sys::Event| {
        web_view_emit(
            id,
            api::web_view::WebViewEvent::LoadFailed(unsupported("iframe load failed")),
        );
    }) as Box<dyn FnMut(web_sys::Event)>);
    let target: &web_sys::EventTarget = iframe.unchecked_ref();
    target
        .add_event_listener_with_callback("load", load.as_ref().unchecked_ref())
        .map_err(|value| js_unknown("iframe load listener failed", value))?;
    target
        .add_event_listener_with_callback("error", error.as_ref().unchecked_ref())
        .map_err(|value| js_unknown("iframe error listener failed", value))?;

    let body_node: &web_sys::Node = body.unchecked_ref();
    let iframe_node: &web_sys::Node = iframe.unchecked_ref();
    body_node
        .append_child(iframe_node)
        .map_err(|value| js_unknown("iframe append failed", value))?;
    iframe.set_src(&href);

    WEB_VIEWS.with(|views| {
        views.borrow_mut().insert(
            id,
            WebFrameEntry { iframe, _load: load, _error: error, on_event: Some(on_event) },
        );
    });
    Ok(id)
}

#[cfg(not(target_arch = "wasm32"))]
fn web_view_create(
    _url: &str,
    _on_event: Box<dyn Fn(api::web_view::WebViewEvent) + Send>,
) -> Result<u32, api::PlatformError> {
    Err(unsupported("embedded browser web views are unavailable on non-wasm targets"))
}

#[cfg(target_arch = "wasm32")]
fn web_view_execute_script(id: u32, script: &str) -> Result<Option<String>, api::PlatformError> {
    let iframe = WEB_VIEWS.with(|views| views.borrow().get(&id).map(|entry| entry.iframe.clone()));
    let iframe = iframe.ok_or(api::PlatformError::NotFound("web view handle not found"))?;
    let content_window = iframe
        .content_window()
        .ok_or(api::PlatformError::PermissionDenied("iframe content window is inaccessible"))?;
    let eval_value = js_sys::Reflect::get(content_window.as_ref(), &JsValue::from_str("eval"))
        .map_err(|value| js_unknown("iframe eval lookup failed", value))?;
    let eval = eval_value
        .dyn_into::<js_sys::Function>()
        .map_err(|_| api::PlatformError::PermissionDenied("iframe eval is inaccessible"))?;
    let value = eval
        .call1(content_window.as_ref(), &JsValue::from_str(script))
        .map_err(|value| js_unknown("iframe script execution failed", value))?;
    Ok(js_value_to_optional_string(&value))
}

#[cfg(not(target_arch = "wasm32"))]
fn web_view_execute_script(_id: u32, _script: &str) -> Result<Option<String>, api::PlatformError> {
    Err(unsupported("embedded browser web views are unavailable on non-wasm targets"))
}

#[cfg(target_arch = "wasm32")]
fn web_view_close(id: u32) {
    let entry = WEB_VIEWS.with(|views| views.borrow_mut().remove(&id));
    if let Some(entry) = entry {
        let element: &web_sys::Element = entry.iframe.unchecked_ref();
        element.remove();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn web_view_close(_id: u32) {}

#[cfg(target_arch = "wasm32")]
fn web_view_emit(id: u32, event: api::web_view::WebViewEvent) {
    let callback = WEB_VIEWS
        .with(|views| views.borrow_mut().get_mut(&id).and_then(|entry| entry.on_event.take()));
    if let Some(callback) = callback {
        callback(event);
        WEB_VIEWS.with(|views| {
            if let Some(entry) = views.borrow_mut().get_mut(&id) {
                if entry.on_event.is_none() {
                    entry.on_event = Some(callback);
                }
            }
        });
    }
}

#[cfg(target_arch = "wasm32")]
fn web_view_next_id() -> u32 {
    WEB_VIEW_NEXT_ID.with(|next| {
        let id = next.get();
        next.set(id.wrapping_add(1).max(1));
        id
    })
}

#[cfg(target_arch = "wasm32")]
fn web_view_same_origin_url(
    window: &web_sys::Window,
    url: &str,
) -> Result<String, api::PlatformError> {
    if url == "about:blank" {
        return Ok(String::from("about:blank"));
    }
    let base = window
        .location()
        .href()
        .map_err(|value| js_unknown("window location lookup failed", value))?;
    let parsed = web_sys::Url::new_with_base(url, &base)
        .map_err(|_| api::PlatformError::Invalid("invalid web view URL"))?;
    let origin = window
        .location()
        .origin()
        .map_err(|value| js_unknown("window origin lookup failed", value))?;
    if parsed.origin() != origin {
        return Err(api::PlatformError::PermissionDenied(
            "browser iframe web views require same-origin URLs",
        ));
    }
    Ok(parsed.href())
}

#[cfg(target_arch = "wasm32")]
fn js_value_to_optional_string(value: &JsValue) -> Option<String> {
    if value.is_null() || value.is_undefined() {
        return None;
    }
    value
        .as_string()
        .or_else(|| js_sys::JSON::stringify(value).ok().and_then(|text| text.as_string()))
}

#[cfg(target_arch = "wasm32")]
fn js_unknown(context: &'static str, value: JsValue) -> api::PlatformError {
    match value.as_string() {
        Some(message) if !message.is_empty() => api::PlatformError::Unknown(message),
        _ => api::PlatformError::Unknown(String::from(context)),
    }
}

#[cfg(target_arch = "wasm32")]
fn browser_device_scale() -> f32 {
    web_sys::window().map(|window| window.device_pixel_ratio() as f32).unwrap_or(1.0).max(1.0)
}

#[cfg(not(target_arch = "wasm32"))]
fn browser_device_scale() -> f32 {
    1.0
}

#[cfg(target_arch = "wasm32")]
fn dispatch_window_event(name: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    if let Ok(event) = web_sys::CustomEvent::new(name) {
        let _ = window.dispatch_event(&event);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn dispatch_window_event(_name: &str) {}

#[cfg(target_arch = "wasm32")]
fn open_external_url_browser(url: &str) -> Result<(), api::PlatformError> {
    let Some(window) = web_sys::window() else {
        return Err(unsupported("window is unavailable"));
    };
    window
        .open_with_url_and_target_and_features(url, "_blank", "noopener,noreferrer")
        .map(|_| ())
        .map_err(|_| api::PlatformError::Unknown(String::from("window.open failed")))
}

#[cfg(not(target_arch = "wasm32"))]
fn open_external_url_browser(_url: &str) -> Result<(), api::PlatformError> {
    Err(unsupported("browser window is unavailable on non-wasm targets"))
}

#[cfg(target_arch = "wasm32")]
fn clipboard_write_browser(value: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let clipboard = window.navigator().clipboard();
    let _ = clipboard.write_text(value);
}

#[cfg(not(target_arch = "wasm32"))]
fn clipboard_write_browser(_value: &str) {}

#[cfg(target_arch = "wasm32")]
async fn clipboard_read_browser_async() -> Result<Option<String>, api::PlatformError> {
    let Some(window) = web_sys::window() else {
        return Err(unsupported("window is unavailable"));
    };
    let value = JsFuture::from(window.navigator().clipboard().read_text())
        .await
        .map_err(|value| js_unknown("clipboard readText failed", value))?;
    let text = value.as_string().unwrap_or_default();
    clipboard_cached_set(&text);
    Ok(Some(text))
}

#[cfg(not(target_arch = "wasm32"))]
async fn clipboard_read_browser_async() -> Result<Option<String>, api::PlatformError> {
    Ok(clipboard_cached_get())
}

#[cfg(target_arch = "wasm32")]
async fn clipboard_write_browser_async(value: &str) -> Result<(), api::PlatformError> {
    let Some(window) = web_sys::window() else {
        return Err(unsupported("window is unavailable"));
    };
    JsFuture::from(window.navigator().clipboard().write_text(value))
        .await
        .map(|_| ())
        .map_err(|value| js_unknown("clipboard writeText failed", value))
}

#[cfg(not(target_arch = "wasm32"))]
async fn clipboard_write_browser_async(_value: &str) -> Result<(), api::PlatformError> {
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn stop_media_stream(stream: &web_sys::MediaStream) {
    let tracks = stream.get_tracks();
    for idx in 0..tracks.length() {
        if let Ok(track) = tracks.get(idx).dyn_into::<web_sys::MediaStreamTrack>() {
            track.stop();
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn wake_lock_set(disabled: bool) {
    if disabled {
        wake_lock_request();
    } else {
        wake_lock_release();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn wake_lock_set(_disabled: bool) {}

#[cfg(target_arch = "wasm32")]
fn wake_lock_request() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let navigator = window.navigator();
    let Ok(wake_lock) = js_sys::Reflect::get(navigator.as_ref(), &JsValue::from_str("wakeLock"))
    else {
        return;
    };
    if wake_lock.is_null() || wake_lock.is_undefined() {
        return;
    }
    let Ok(request_value) = js_sys::Reflect::get(&wake_lock, &JsValue::from_str("request")) else {
        return;
    };
    let Some(request) = request_value.dyn_ref::<js_sys::Function>() else {
        return;
    };
    let Ok(promise_value) = request.call1(&wake_lock, &JsValue::from_str("screen")) else {
        return;
    };
    let Ok(promise) = promise_value.dyn_into::<js_sys::Promise>() else {
        return;
    };
    spawn_local(async move {
        if let Ok(sentinel) = JsFuture::from(promise).await {
            WAKE_LOCK_SENTINEL.with(|slot| {
                *slot.borrow_mut() = Some(sentinel);
            });
        }
    });
}

#[cfg(target_arch = "wasm32")]
fn wake_lock_release() {
    let sentinel = WAKE_LOCK_SENTINEL.with(|slot| slot.borrow_mut().take());
    let Some(sentinel) = sentinel else {
        return;
    };
    let Ok(release_value) = js_sys::Reflect::get(&sentinel, &JsValue::from_str("release")) else {
        return;
    };
    let Some(release) = release_value.dyn_ref::<js_sys::Function>() else {
        return;
    };
    let _ = release.call0(&sentinel);
}

#[cfg(target_arch = "wasm32")]
fn vibrate(ms: u32) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let _ = window.navigator().vibrate_with_duration(ms);
}

#[cfg(not(target_arch = "wasm32"))]
fn vibrate(_ms: u32) {}

#[cfg(target_arch = "wasm32")]
fn browser_online() -> bool {
    web_sys::window().map(|window| window.navigator().on_line()).unwrap_or(false)
}

#[cfg(not(target_arch = "wasm32"))]
fn browser_online() -> bool {
    false
}

#[cfg(target_arch = "wasm32")]
fn browser_monotonic_now() -> Duration {
    let millis = web_sys::window()
        .and_then(|window| window.performance())
        .map(|perf| perf.now())
        .unwrap_or(0.0);
    Duration::from_nanos((millis.max(0.0) * 1_000_000.0).round() as u64)
}

#[cfg(not(target_arch = "wasm32"))]
fn browser_monotonic_now() -> Duration {
    Duration::from_nanos(0)
}

#[cfg(target_arch = "wasm32")]
fn local_storage_save(key: &str, data: &[u8]) -> Result<(), api::PlatformError> {
    let storage = local_storage()?;
    storage
        .set_item(&storage_key(key), &hex_encode(data))
        .map_err(|_| api::PlatformError::Unknown(String::from("localStorage set_item failed")))
}

#[cfg(not(target_arch = "wasm32"))]
fn local_storage_save(_key: &str, _data: &[u8]) -> Result<(), api::PlatformError> {
    Err(unsupported("localStorage is unavailable on non-wasm targets"))
}

#[cfg(target_arch = "wasm32")]
fn local_storage_load(key: &str) -> Result<Option<Vec<u8>>, api::PlatformError> {
    let storage = local_storage()?;
    match storage
        .get_item(&storage_key(key))
        .map_err(|_| api::PlatformError::Unknown(String::from("localStorage get_item failed")))?
    {
        Some(value) => hex_decode(&value).map(Some),
        None => Ok(None),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn local_storage_load(_key: &str) -> Result<Option<Vec<u8>>, api::PlatformError> {
    Err(unsupported("localStorage is unavailable on non-wasm targets"))
}

#[cfg(target_arch = "wasm32")]
fn local_storage_delete(key: &str) -> Result<(), api::PlatformError> {
    let storage = local_storage()?;
    storage
        .remove_item(&storage_key(key))
        .map_err(|_| api::PlatformError::Unknown(String::from("localStorage remove_item failed")))
}

#[cfg(not(target_arch = "wasm32"))]
fn local_storage_delete(_key: &str) -> Result<(), api::PlatformError> {
    Err(unsupported("localStorage is unavailable on non-wasm targets"))
}

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Result<web_sys::Storage, api::PlatformError> {
    web_sys::window()
        .ok_or(unsupported("window is unavailable"))?
        .local_storage()
        .map_err(|_| api::PlatformError::Unknown(String::from("localStorage access failed")))?
        .ok_or(unsupported("localStorage is unavailable"))
}
