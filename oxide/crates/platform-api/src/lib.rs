//! `Oxide` Platform API
//!
//! Provides platform-agnostic traits and types used across the engine.

#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc, clippy::module_name_repetitions)]

use core::fmt;
use core::future::Future;
use core::pin::Pin;
use oxide_renderer_api as rend;
use std::sync::{Arc, OnceLock, RwLock};

pub mod clipboard;
pub mod contacts;
pub mod media_library;
pub mod network_status;
pub mod secure_storage;
pub mod telephony;
pub mod url_scheme;
pub mod web_view;

type PostTaskFn = dyn Fn(alloc::boxed::Box<dyn FnOnce() + Send>) + Send + Sync;

// ===== Public app interface =====

pub trait App: Send + Sync {
    fn init(&mut self, ctx: &mut InitContext);
    fn event(&mut self, e: AppEvent, ctx: &mut UpdateContext);
    fn draw(&mut self, r: &mut rend::RenderContext);

    fn upload_runtime_images(&mut self, _uploader: &mut dyn rend::RuntimeImageUploader) {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MobileTarget {
    Ios,
    Android, // future
}

// ===== Contexts =====

#[derive(Debug, Default)]
pub struct FontLoader;

pub struct InitContext {
    pub fonts: FontLoader,
    pub device: DeviceCaps,
    pub platform: alloc::boxed::Box<dyn Platform>,
}

#[derive(Debug, Default)]
pub struct Timers; // engine-provided in later phases

pub trait Haptics: Send + Sync {
    fn play(&self, p: HapticPattern);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HapticPattern {
    ImpactLight,
    ImpactMedium,
    ImpactHeavy,
    Selection,
    NotificationSuccess,
    NotificationWarning,
    NotificationError,
}

pub struct UpdateContext {
    pub post_task: alloc::boxed::Box<PostTaskFn>,
    pub timers: Timers,
    pub haptics: alloc::boxed::Box<dyn Haptics>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoCapitalization {
    None,
    Sentences,
    Words,
    AllCharacters,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardAppearance {
    Default,
    Dark,
    Light,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReturnKeyType {
    Default,
    Done,
    Go,
    Next,
    Search,
    Send,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextContentType {
    Plain,
    Username,
    Password,
    Email,
    OneTimeCode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextInputConfig {
    pub autocorrect: bool,
    pub autocapitalization: AutoCapitalization,
    pub keyboard: KeyboardAppearance,
    pub return_key: ReturnKeyType,
    pub content_type: TextContentType,
}

impl Default for TextInputConfig {
    fn default() -> Self {
        Self {
            autocorrect: true,
            autocapitalization: AutoCapitalization::Sentences,
            keyboard: KeyboardAppearance::Default,
            return_key: ReturnKeyType::Default,
            content_type: TextContentType::Plain,
        }
    }
}

impl UpdateContext {
    #[cfg(feature = "tokio-runtime")]
    pub fn spawn<F>(&self, fut: F)
    where
        F: core::future::Future<Output = ()> + Send + 'static,
    {
        runtime::spawn(fut);
    }
}

impl Timers {}

// ===== Device caps and color space =====

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSpace {
    Srgb,
    DisplayP3Linear,
    ScRGBLinear,
    Bt2020PQ,
}

// ===== Animation system types =====

pub type AnimId = u64;

#[derive(Clone, Copy, Debug)]
pub enum EaseKind {
    Linear,
    QuadIn,
    QuadOut,
    QuadInOut,
    CubicIn,
    CubicOut,
    CubicInOut,
    BackInOut,
    ElasticOut,
    BounceOut,
}

#[derive(Clone, Copy, Debug)]
pub struct Ease {
    pub kind: EaseKind,
}

#[derive(Clone, Copy, Debug)]
pub struct SpringParams {
    pub stiffness: f32,
    pub damping: f32,
    pub mass: f32,
    pub eps: f32,
}

#[derive(Clone, Copy, Debug)]
pub enum AnimCurve {
    Ease { ease: Ease },
    Spring { sp: SpringParams },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnimProp {
    Opacity,
    Transform2D,
    ColorRGBA,
    CornerRadius,
    ShadowAlpha,
}

#[derive(Clone, Copy, Debug)]
pub struct Transform2D {
    pub tx: f32,
    pub ty: f32,
    pub sx: f32,
    pub sy: f32,
    pub rot_rad: f32,
}

#[derive(Clone, Debug)]
pub enum AnimValue {
    F32(f32),
    Vec2([f32; 2]),
    Vec4([f32; 4]),
    Mat3([f32; 9]),
    Xform2D(Transform2D),
}

#[derive(Clone, Copy, Debug)]
pub enum Repeat {
    Once,
    Count(u32),
    Forever,
}

#[derive(Clone, Debug)]
pub struct AnimDesc {
    pub id: AnimId,
    pub prop: AnimProp,
    pub from: AnimValue,
    pub to: AnimValue,
    pub curve: AnimCurve,
    pub duration_ms: u32,
    pub delay_ms: u32,
    pub repeat: Repeat,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeviceCaps {
    pub max_framerate_hz: u32,
    pub supports_edr: bool,
    pub supports_msaa4x: bool,
    pub native_scale: f32,
    pub color_space: ColorSpace,
    pub a11y_reduce_motion: bool,
}

// ===== Events =====

#[derive(Debug, Clone)]
pub enum AppEvent {
    Lifecycle(Lifecycle),
    Window(WindowEvent),
    Input(InputEvent),
    Text(TextEvent),
    Keyboard(KeyboardEvent),
    RendererStats(RendererStats),
    UrlScheme(alloc::string::String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle {
    WillEnterForeground,
    DidEnterBackground,
    WillTerminate,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InsetsF {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowEvent {
    Resized { w: u32, h: u32, scale: f32, safe: rend::Insets },
    OrientationChanged { portrait: bool },
}

#[derive(Debug, Clone)]
pub enum InputEvent {
    Touch(TouchEvent),
    Pointer(PointerEvent),
    Key(KeyEvent),
}

#[derive(Debug, Clone)]
pub enum TextEvent {
    Commit { text: alloc::string::String },
    Composition { range: core::ops::Range<u32>, text: alloc::string::String },
    SelectionChanged { range: core::ops::Range<u32> },
    IMEShown(rend::RectF),
    IMEHidden,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyboardGeometry {
    pub visible: bool,
    pub frame: rend::RectF,
    pub overlap_insets: rend::Insets,
}

impl Default for KeyboardGeometry {
    fn default() -> Self {
        Self {
            visible: false,
            frame: rend::RectF::new(0.0, 0.0, 0.0, 0.0),
            overlap_insets: rend::Insets::new(0.0, 0.0, 0.0, 0.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyboardTransition {
    pub geometry: KeyboardGeometry,
    pub animation_ms: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RendererStats {
    pub frame_id: u64,
    pub encode_ms: f32,
    pub damage_pct: f32,
    pub damage_rects: u32,
    pub draws: u32,
    pub sample_count: u32,
    pub hdr: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum KeyboardEvent {
    WillChange(KeyboardTransition),
    DidChange(KeyboardTransition),
}

// Touch & pointer

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TouchId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchPhase {
    Start,
    Move,
    End,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerDevice {
    Finger,
    Pencil,
    Mouse,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TouchEvent {
    pub id: TouchId,
    pub phase: TouchPhase,
    pub timestamp_ns: u64,
    pub x: f32,
    pub y: f32,
    pub pressure: Option<f32>,
    pub tilt: Option<(f32, f32)>, // altitude, azimuth in radians
    pub device: PointerDevice,
}

pub use bitflags::bitflags;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Modifiers: u32 {
        const SHIFT   = 1 << 0;
        const CONTROL = 1 << 1;
        const ALT     = 1 << 2;
        const META    = 1 << 3; // Command/Windows
        const CAPS    = 1 << 4;
        const FN      = 1 << 5;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct MouseButtons {
    pub left: bool,
    pub middle: bool,
    pub right: bool,
    pub back: bool,
    pub forward: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointerEvent {
    pub x: f32,
    pub y: f32,
    pub dx: f32,
    pub dy: f32,
    pub buttons: MouseButtons,
    pub modifiers: Modifiers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    Unknown,
    Escape,
    Enter,
    Tab,
    Backspace,
    Space,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    F(u8),        // F1..F24
    Digit(u8),    // 0..9
    Letter(char), // A..Z
}

#[derive(Debug, Clone, PartialEq)]
pub struct KeyEvent {
    pub code: KeyCode,
    pub chars: Option<alloc::string::String>,
    pub repeat: bool,
    pub modifiers: Modifiers,
}

// ===== Capabilities =====

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Capabilities: u64 {
        const CAMERA        = 1 << 0;
        const BLUETOOTH     = 1 << 1;
        const PUSH          = 1 << 2;
        const HOVER_POINTER = 1 << 3;
        const MOTION        = 1 << 4;
        const LOCATION      = 1 << 5;
        const CAMERA_RECORDING = 1 << 6;
    }
}

// ===== Permissions =====

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PermissionDomain {
    Notifications,
    Location,
    Camera,
    Contacts,
    Bluetooth,
    Motion,
    Microphone,
    MediaLibrary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PermissionStatus {
    NotDetermined,
    Denied,
    Limited,
    Authorized,
}

pub trait Permissions: Send + Sync {
    fn request(&self, domain: PermissionDomain);
    fn status(&self, domain: PermissionDomain) -> PermissionStatus;
    fn subscribe(&self, f: alloc::boxed::Box<dyn Fn(PermissionDomain, PermissionStatus) + Send>);
}

// ===== Location & motion =====

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocationReading {
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub altitude_m: f32,
    pub horizontal_accuracy_m: f32,
    pub vertical_accuracy_m: f32,
    pub speed_mps: f32,
    pub course_deg: f32,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GeoHash(pub u64);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoRegion {
    pub hash: GeoHash,
    pub center: (f64, f64),
    pub radius_m: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocationAccuracy {
    Reduced,
    Balanced,
    LowPower,
    Precise,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocationOptions {
    pub accuracy: LocationAccuracy,
    pub distance_filter_m: f32,
    pub allow_background_updates: bool,
    pub precise: bool,
}

impl Default for LocationOptions {
    fn default() -> Self {
        Self {
            accuracy: LocationAccuracy::Balanced,
            distance_filter_m: 0.0,
            allow_background_updates: false,
            precise: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum LocationEvent {
    Update(LocationReading),
    EnteredRegion(GeoRegion),
    ExitedRegion(GeoRegion),
    Error(PlatformError),
}

pub trait GeoRegionTracker {
    fn monitored_regions(&self) -> alloc::vec::Vec<GeoRegion>;
    fn set_regions(&self, regions: &[GeoRegion]) -> Result<(), PlatformError>;
}

pub trait LocationService: Send + Sync {
    fn start(&self, opts: LocationOptions) -> Result<(), PlatformError>;
    fn stop(&self);
    fn request_once(&self);
    fn last(&self) -> Option<LocationReading>;
    fn subscribe(&self, f: alloc::boxed::Box<dyn Fn(LocationEvent) + Send>);
    fn history(&self) -> alloc::vec::Vec<LocationReading>;
    fn region_tracker(&self) -> Option<alloc::boxed::Box<dyn GeoRegionTracker>>;
    fn set_accuracy(&self, _accuracy: LocationAccuracy) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("location accuracy control unavailable"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionSample {
    pub pressure_pa: Option<f32>,
    pub relative_altitude_m: Option<f32>,
    pub timestamp_ms: u64,
}

pub trait MotionService: Send + Sync {
    fn start(&self) -> Result<(), PlatformError>;
    fn stop(&self);
    fn is_running(&self) -> bool;
    fn subscribe(&self, f: alloc::boxed::Box<dyn Fn(MotionSample) + Send>);
    fn pressure_history(&self) -> alloc::vec::Vec<MotionSample>;
}

// ===== Bluetooth =====

pub type PeripheralId = u128;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BleUuid(pub [u8; 16]);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeripheralInfo {
    pub id: PeripheralId,
    pub name: Option<alloc::string::String>,
    pub rssi_dbm: i16,
    pub advertisement: AdvertisementData,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GattChar {
    pub service: BleUuid,
    pub characteristic: BleUuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestorationInfo {
    pub peripherals: alloc::vec::Vec<PeripheralInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BluetoothEvent {
    StateChanged { powered_on: bool },
    Discovered(PeripheralInfo),
    Connected(PeripheralId),
    Disconnected(PeripheralId),
    Notified { id: PeripheralId, chr: GattChar, data: alloc::vec::Vec<u8> },
    CacheUpdated(BleCacheEntry),
    Restored(RestorationInfo),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvertisementData {
    pub services: alloc::vec::Vec<BleUuid>,
    pub manufacturer_data: Option<alloc::vec::Vec<u8>>,
    pub connectable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BleCacheEntry {
    pub peripheral: PeripheralInfo,
    pub last_seen_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScanOptions {
    pub services: alloc::vec::Vec<BleUuid>,
    pub allow_duplicates: bool,
}

pub trait Bluetooth: Send + Sync {
    // Central role
    fn powered_on(&self) -> bool;
    fn subscribe_events(&self, f: alloc::boxed::Box<dyn Fn(BluetoothEvent) + Send>);
    fn start_scan(&self, opts: &ScanOptions);
    fn stop_scan(&self);
    fn connect(&self, id: PeripheralId);
    fn disconnect(&self, id: PeripheralId);
    fn read(&self, id: PeripheralId, chr: GattChar) -> Result<alloc::vec::Vec<u8>, PlatformError>;
    fn write(
        &self,
        id: PeripheralId,
        chr: GattChar,
        data: &[u8],
        with_response: bool,
    ) -> Result<(), PlatformError>;
    fn notify(&self, id: PeripheralId, chr: GattChar, enable: bool) -> Result<(), PlatformError>;

    // Peripheral role
    fn advertise_start(&self, name: &str, services: &[BleUuid]);
    fn advertise_stop(&self);

    fn cached_peripherals(&self) -> alloc::vec::Vec<BleCacheEntry>;
}

// ===== Push notifications =====

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushPresentation {
    /// The notification was tapped by the user to launch or foreground the app.
    Opened,
    /// The notification arrived while the app was already in the foreground.
    Foreground,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushNotification {
    pub user_info: alloc::collections::BTreeMap<alloc::string::String, alloc::string::String>,
    pub badge: Option<i32>,
    pub sound: Option<alloc::string::String>,
    pub presentation: PushPresentation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushProvider {
    Apns,
    Fcm,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushToken {
    pub provider: PushProvider,
    pub value: alloc::string::String,
}

pub trait PushManager: Send + Sync {
    fn register(&self);
    fn device_token(&self) -> Option<PushToken>;
    fn subscribe(&self, f: alloc::boxed::Box<dyn Fn(PushNotification) + Send>);
    fn set_badge(&self, count: i32);
    fn clear_badge(&self);
    fn clear_all_delivered(&self) {}
}

// ===== Camera =====

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraDevice {
    Front,
    Back,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CameraConfig {
    pub device: CameraDevice,
    pub fps: u32,
    pub resolution: (u32, u32),
    pub capture: CaptureMode,
    // New Field:
    pub preferred_color_space: Option<ColorSpace>,
}

impl Default for CameraConfig {
    fn default() -> Self {
        Self {
            device: CameraDevice::Back,
            fps: 30,
            resolution: (1920, 1080),
            capture: CaptureMode::Preview,
            preferred_color_space: Some(ColorSpace::DisplayP3Linear), // Default to wide color
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureMode {
    Preview,
    Photo,
    Video,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraFrame {
    pub image: CameraImage,
    pub size: (u32, u32),
    pub timestamp_ns: u64,
    pub rotation_deg: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CameraImage {
    Gpu {
        tex: rend::ImageHandle,
    },
    Nv12 {
        y_plane: alloc::vec::Vec<u8>,
        uv_plane: alloc::vec::Vec<u8>,
        stride_y: u32,
        stride_uv: u32,
        bit_depth: u8,
        matrix: u8,
        video_range: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashMode {
    Off,
    On,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TorchMode {
    Off,
    On { level: f32 }, // level from 0.0 to 1.0
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoOptions {
    /// If true, attempts a fast capture from the preview stream if possible.
    /// If false or if flash is enabled, performs a high-quality capture.
    pub high_speed_from_preview: bool,
    pub flash_mode: FlashMode,
}

impl Default for PhotoOptions {
    fn default() -> Self {
        Self { high_speed_from_preview: false, flash_mode: FlashMode::Off }
    }
}

#[derive(Debug, Clone)]
pub enum PhotoEvent {
    Completed(CameraFrame),
    Failed(PlatformError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSample {
    pub channels: u32,
    pub sample_rate_hz: u32,
    pub data: alloc::vec::Vec<i16>,
    pub timestamp_ns: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingContainer {
    Mp4,
    Mov,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordingDestination {
    Temporary,
    File { path: alloc::string::String },
}

impl RecordingDestination {
    pub fn file<P: Into<alloc::string::String>>(path: P) -> Self {
        Self::File { path: path.into() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioSessionMode {
    /// The app takes full control of the audio session, potentially interrupting other apps.
    Exclusive,
    /// The app attempts to mix its audio with other apps.
    MixWithOthers,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingOptions {
    pub destination: RecordingDestination,
    pub container: RecordingContainer,
    pub include_audio: bool,
    // New Field:
    pub audio_session_mode: AudioSessionMode,
}

impl Default for RecordingOptions {
    fn default() -> Self {
        Self {
            destination: RecordingDestination::Temporary,
            container: RecordingContainer::Mp4,
            include_audio: true,
            audio_session_mode: AudioSessionMode::Exclusive,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingResult {
    pub path: alloc::string::String,
    pub duration_ns: u64,
    pub size_bytes: u64,
    pub had_audio: bool,
}

#[derive(Debug, Clone)]
pub enum RecordingEvent {
    Completed(RecordingResult),
    Cancelled,
    Failed(PlatformError),
}

pub trait CameraRecording {
    fn stop(&self);
    fn cancel(&self);
}

pub trait CameraStream {
    fn stop(&self);
}

pub trait CameraManager: Send + Sync {
    fn start_stream(
        &self,
        cfg: CameraConfig,
        on_frame: alloc::boxed::Box<dyn Fn(CameraFrame) + Send>,
        on_audio: Option<alloc::boxed::Box<dyn Fn(AudioSample) + Send>>,
    ) -> Result<alloc::boxed::Box<dyn CameraStream + Send>, PlatformError>;

    fn start_recording(
        &self,
        options: RecordingOptions,
        on_event: alloc::boxed::Box<dyn Fn(RecordingEvent) + Send>,
    ) -> Result<alloc::boxed::Box<dyn CameraRecording + Send>, PlatformError>;

    fn select_device(&self, device: CameraDevice);
    fn set_fps(&self, fps: u32);
    fn set_resolution(&self, width: u32, height: u32);
    fn set_mode(&self, mode: CaptureMode);

    /// Sets the camera's focus point.
    /// The point should be in a normalized coordinate system (e.g., 0.0 to 1.0).
    fn set_focus_point(&self, _x: f32, _y: f32) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("camera focus control unavailable"))
    }

    /// Sets the camera's zoom factor.
    /// A value of 1.0 is no zoom.
    fn set_zoom_factor(&self, _factor: f32) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("camera zoom control unavailable"))
    }

    /// Sets the flash mode for photo capture.
    fn set_flash_mode(&self, _mode: FlashMode) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("camera flash control unavailable"))
    }

    /// Sets the torch (flashlight) mode.
    fn set_torch_mode(&self, _mode: TorchMode) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("camera torch control unavailable"))
    }

    /// Captures a single photo.
    fn capture_photo(
        &self,
        _options: PhotoOptions,
        _on_event: alloc::boxed::Box<dyn Fn(PhotoEvent) + Send>,
    ) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("camera photo capture unavailable"))
    }
}

// ===== Networking =====

/// Represents a client identity for mutual TLS (mTLS).
/// The format is platform-specific (e.g., a PKCS#12 bundle).
pub type ClientIdentity = alloc::vec::Vec<u8>;

/// Represents a public key to use for certificate pinning.
pub type PinnedPublicKey = alloc::vec::Vec<u8>;

#[derive(Debug, Clone)]
pub struct TlsOptions {
    /// The identity of the client to be presented to the server for mTLS.
    pub client_identity: Option<ClientIdentity>,
    /// A list of public keys to use for certificate pinning. If the server's
    /// certificate chain does not contain one of these public keys, the
    /// connection will fail.
    pub pinned_public_keys: alloc::vec::Vec<PinnedPublicKey>,
}

#[derive(Debug, Clone, Default)]
pub struct TcpOptions {
    /// Enable TCP keepalives.
    pub keepalive: bool,
    /// The time in seconds of inactivity before sending a keepalive probe.
    pub keepalive_idle_time_secs: u32,
    /// Enable TCP Fast Open.
    pub fast_open: bool,
}

#[derive(Debug, Clone)]
pub struct QuicOptions {
    /// The Application-Layer Protocol Negotiation (ALPN) protocol name.
    pub alpn: alloc::string::String,
    // Other QUIC-specific parameters like flow control can be added here.
}

#[derive(Debug, Clone)]
pub enum ProtocolOptions {
    Tcp(TcpOptions),
    Quic(QuicOptions),
}

#[derive(Debug, Clone)]
pub struct ConnectionOptions {
    pub host: alloc::string::String,
    pub port: u16,
    /// The underlying transport protocol to use.
    pub protocol: ProtocolOptions,
    /// If Some, the connection will be secured using the specified TLS options.
    pub tls_options: Option<TlsOptions>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NetworkErrorDomain {
    Tls,
    Dns,
    Posix,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct NetworkError {
    pub domain: NetworkErrorDomain,
    pub code: i32,
    pub reason: alloc::string::String,
}

#[derive(Debug, Clone)]
pub enum ConnectionEvent {
    Connected,
    Disconnected {
        error: Option<NetworkError>,
    },
    /// Provides a block of data read from the connection.
    Read(alloc::vec::Vec<u8>),
}

/// Represents a single, low-level, stream-oriented connection.
pub trait Connection: Send + Sync {
    /// Asynchronously writes a block of data to the connection.
    fn write<'a>(
        &'a self,
        data: &'a [u8],
    ) -> Pin<alloc::boxed::Box<dyn Future<Output = Result<(), PlatformError>> + Send + 'a>>;

    /// Closes the connection.
    fn close(&self);
}

/// Represents a multiplexed connection (like QUIC) from which multiple
/// logical streams (`Connection` objects) can be created.
pub trait ConnectionGroup: Send + Sync {
    /// Extracts a new bidirectional stream from the group.
    fn extract_stream(&self) -> Result<alloc::boxed::Box<dyn Connection + Send>, PlatformError>;
    /// Closes the entire connection group.
    fn close(&self);
}

#[derive(Debug, Clone)]
pub struct UdpPacket {
    pub host: alloc::string::String,
    pub port: u16,
    pub data: alloc::vec::Vec<u8>,
}

#[derive(Debug, Clone)]
pub enum UdpEvent {
    /// A packet was received from a remote peer.
    Read(UdpPacket),
    /// An asynchronous write operation failed..
    WriteError(PlatformError),
}

pub trait UdpSocket: Send + Sync {
    /// Sends a UDP packet to a specified destination. This is a "fire-and-forget"
    /// operation. Errors are generally not reported for individual sends unless
    /// a system-level error occurs, which would be reported via the `UdpEvent::WriteError`
    /// event on the callback.
    fn send(&self, packet: &UdpPacket) -> Result<(), PlatformError>;

    /// Closes the socket.
    fn close(&self);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
   Get,
   Post,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpCredentials {
   Omit,
   SameOrigin,
}

/// One caller-supplied request header or caller-selected response header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpHeader {
   pub name: alloc::string::String,
   pub value: alloc::string::String,
}

/// A single manual-redirect HTTP hop.
///
/// Platform adapters must not follow redirects. Callers preserve one absolute budget across
/// redirect hops and pass only the remaining timeout to each operation. Ambient credentials are
/// omitted unless the caller explicitly selects `HttpCredentials::SameOrigin`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
   pub method: HttpMethod,
   pub url: alloc::string::String,
   pub timeout: std::time::Duration,
   pub max_response_bytes: usize,
   pub headers: alloc::vec::Vec<HttpHeader>,
   pub response_headers: alloc::vec::Vec<alloc::string::String>,
   pub body: alloc::vec::Vec<u8>,
   pub credentials: HttpCredentials,
}

impl HttpRequest {
   pub const DEFAULT_TIMEOUT_MS: u32 = 10_000;
   pub const DEFAULT_MAX_RESPONSE_BYTES: usize = 1_048_576;

   #[must_use]
   pub fn get(url: impl Into<alloc::string::String>) -> Self {
      Self {
         method: HttpMethod::Get,
         url: url.into(),
         timeout: std::time::Duration::from_millis(u64::from(Self::DEFAULT_TIMEOUT_MS)),
         max_response_bytes: Self::DEFAULT_MAX_RESPONSE_BYTES,
         headers: alloc::vec::Vec::new(),
         response_headers: alloc::vec::Vec::new(),
         body: alloc::vec::Vec::new(),
         credentials: HttpCredentials::Omit,
      }
   }

   #[must_use]
   pub fn post(url: impl Into<alloc::string::String>, body: alloc::vec::Vec<u8>) -> Self {
      let mut request = Self::get(url);
      request.method = HttpMethod::Post;
      request.body = body;
      request
   }

   #[must_use]
   pub fn with_timeout_ms(mut self, timeout_ms: u32) -> Self {
      self.timeout = std::time::Duration::from_millis(u64::from(timeout_ms));
      self
   }

   #[must_use]
   pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
      self.timeout = timeout;
      self
   }

   #[must_use]
   pub fn with_max_response_bytes(mut self, max_response_bytes: usize) -> Self {
      self.max_response_bytes = max_response_bytes;
      self
   }

   #[must_use]
   pub fn with_header(mut self, name: impl Into<alloc::string::String>, value: impl Into<alloc::string::String>) -> Self {
      self.headers.push(HttpHeader { name: name.into(), value: value.into() });
      self
   }

   #[must_use]
   pub fn select_response_header(mut self, name: impl Into<alloc::string::String>) -> Self {
      self.response_headers.push(name.into());
      self
   }

   #[must_use]
   pub fn with_credentials(mut self, credentials: HttpCredentials) -> Self {
      self.credentials = credentials;
      self
   }
}

/// Response metadata for one HTTP hop. Bodies arrive incrementally as `HttpEvent::Body`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
   pub final_url: alloc::string::String,
   pub status: u16,
   pub content_length: Option<u64>,
   pub headers: alloc::vec::Vec<HttpHeader>,
}

/// Serialized events for one accepted HTTP operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpEvent {
   Response(HttpResponse),
   Body(alloc::vec::Vec<u8>),
   Complete,
   Failed(PlatformError),
   Cancelled,
}

impl HttpEvent {
   #[must_use]
   pub const fn terminal(&self) -> bool {
      matches!(self, Self::Complete | Self::Failed(_) | Self::Cancelled)
   }
}

/// Cancellation ownership for one accepted HTTP operation.
pub trait HttpOperation: Send + Sync {
   /// Requests cancellation. Implementations deliver at most one terminal callback overall.
   fn cancel(&self);
}

/// Object-safe, nonblocking HTTP service.
pub trait HttpClient: Send + Sync {
   /// Starts one hop and returns before network progress. Events are serialized per operation.
   fn start(&self, request: HttpRequest, on_event: alloc::boxed::Box<dyn Fn(HttpEvent) + Send + Sync>) -> Result<alloc::boxed::Box<dyn HttpOperation + Send + Sync>, PlatformError>;
}

pub struct UnsupportedHttpClient;

impl HttpClient for UnsupportedHttpClient {
   fn start(&self, _request: HttpRequest, _on_event: alloc::boxed::Box<dyn Fn(HttpEvent) + Send + Sync>) -> Result<alloc::boxed::Box<dyn HttpOperation + Send + Sync>, PlatformError> {
      Err(PlatformError::Unsupported("platform HTTP service not implemented"))
   }
}

static UNSUPPORTED_HTTP_CLIENT: UnsupportedHttpClient = UnsupportedHttpClient;

/// A factory for creating network connections.
pub trait Networking: Send + Sync {
    /// Establishes a new TCP connection to a remote host.
    fn connect_tcp(
        &self,
        options: ConnectionOptions, // `protocol` field should be ProtocolOptions::Tcp
        on_event: alloc::boxed::Box<dyn Fn(ConnectionEvent) + Send>,
    ) -> Result<alloc::boxed::Box<dyn Connection + Send>, PlatformError>;

    /// Establishes a new QUIC connection group to a remote host.
    /// Events related to the group itself (e.g., Connected, Disconnected)
    /// are delivered via the callback. Individual streams extracted from the
    /// group will have their own event handlers.
    fn connect_quic(
        &self,
        options: ConnectionOptions, // `protocol` field should be ProtocolOptions::Quic
        on_event: alloc::boxed::Box<dyn Fn(ConnectionEvent) + Send>,
    ) -> Result<alloc::boxed::Box<dyn ConnectionGroup + Send>, PlatformError>;

    /// Creates a UDP socket bound to a local port.
    ///
    /// The socket will be ready to send and receive packets immediately.
    /// Received packets will be delivered via the provided callback.
    fn bind_udp(
        &self,
        // The local port to bind to. If 0, the OS will choose a random ephemeral port.
        local_port: u16,
        on_event: alloc::boxed::Box<dyn Fn(UdpEvent) + Send>,
    ) -> Result<alloc::boxed::Box<dyn UdpSocket + Send>, PlatformError>;
}

// ===== Storage Paths =====

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StandardPath {
    /// A directory for storing critical user data files that should be backed up.
    /// On iOS, this maps to the Application Support directory.
    /// On Android, this maps to the directory returned by `getFilesDir()`.
    Documents,

    /// A directory for storing cached data that can be regenerated.
    /// The OS may delete files in this directory to free up space.
    /// On iOS, this maps to the Caches directory.
    /// On Android, this maps to the directory returned by `getCacheDir()`.
    Cache,

    /// A directory for temporary files that do not need to persist across
    /// app launches.
    Temporary,
}

pub trait PathService: Send + Sync {
    /// Gets the absolute path string for a given standard directory.
    /// The host guarantees that this directory exists.
    fn get(&self, path: StandardPath) -> alloc::string::String;
}

// ===== Secure Storage =====

pub trait SecureStorage: Send + Sync {
    /// Saves a block of data under a given key.
    /// This will overwrite any existing data for the key.
    fn save<'a>(
        &'a self,
        key: &'a str,
        data: &'a [u8],
    ) -> Pin<alloc::boxed::Box<dyn Future<Output = Result<(), PlatformError>> + Send + 'a>>;

    /// Loads a block of data for a given key.
    /// Returns `Ok(None)` if the key does not exist.
    fn load<'a>(
        &'a self,
        key: &'a str,
    ) -> Pin<
        alloc::boxed::Box<
            dyn Future<Output = Result<Option<alloc::vec::Vec<u8>>, PlatformError>> + Send + 'a,
        >,
    >;

    /// Deletes the data associated with a given key.
    fn delete<'a>(
        &'a self,
        key: &'a str,
    ) -> Pin<alloc::boxed::Box<dyn Future<Output = Result<(), PlatformError>> + Send + 'a>>;
}

// ===== Time Service =====
use core::time::Duration;

pub trait TimeService: Send + Sync {
    /// Returns the amount of time that has elapsed since an arbitrary,
    /// fixed point in the past. This clock is guaranteed to be monotonically
    /// non-decreasing and is not affected by changes to the system's wall clock.
    ///
    /// This is the cross-platform equivalent of `mach_absolute_time()` on macOS/iOS
    /// or `System.nanoTime()` on Android.
    fn monotonic_now(&self) -> Duration;
}

// ===== Platform and errors =====

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformError {
    CapabilityDisabled(&'static str),
    PermissionDenied(&'static str),
    Busy,
    NotFound(&'static str),
    Io(String),
    Invalid(&'static str),
    Unsupported(&'static str),
    Unknown(String),
}
impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CapabilityDisabled(s) => write!(f, "capability disabled: {s}"),
            Self::PermissionDenied(s) => write!(f, "permission denied: {s}"),
            Self::Busy => write!(f, "busy"),
            Self::NotFound(s) => write!(f, "not found: {s}"),
            Self::Io(s) => write!(f, "io error: {s}"),
            Self::Invalid(s) => write!(f, "invalid: {s}"),
            Self::Unsupported(s) => write!(f, "unsupported: {s}"),
            Self::Unknown(s) => write!(f, "unknown: {s}"),
        }
    }
}
impl std::error::Error for PlatformError {}

pub trait Platform: Send + Sync {
    fn run_app(&self, app: alloc::boxed::Box<dyn App>) -> !;
    fn request_redraw(&self);
    fn set_high_refresh(&self, enable: bool);
    fn set_idle_timer_disabled(&self, disabled: bool);
    fn open_system_settings(&self) {
        panic!("platform open_system_settings not implemented")
    }
    fn open_external_url(&self, _url: &str) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("platform open_external_url not implemented"))
    }
    fn clipboard_get(&self) -> Option<alloc::string::String>;
    fn clipboard_set(&self, s: &str);
    fn ime_show(&self);
    fn ime_hide(&self);
    fn device_caps(&self) -> DeviceCaps;
    fn haptics(&self) -> Arc<dyn Haptics + Send + Sync>;
    fn is_simulation(&self) -> bool {
        false
    }
    fn permissions(&self) -> &dyn Permissions;
    fn camera(&self) -> &dyn CameraManager;
    fn bluetooth(&self) -> &dyn Bluetooth;
    fn bluetooth_with_restoration(&self, restore_id: &str) -> alloc::boxed::Box<dyn Bluetooth>;
    fn location(&self) -> &dyn LocationService;
    fn motion(&self) -> &dyn MotionService;
    fn push(&self) -> &dyn PushManager;
    fn capabilities(&self) -> Capabilities;
    fn networking(&self) -> &dyn Networking {
        panic!("platform networking service not implemented")
    }
    fn http(&self) -> &dyn HttpClient {
        &UNSUPPORTED_HTTP_CLIENT
    }
    fn paths(&self) -> &dyn PathService {
        panic!("platform path service not implemented")
    }
    fn secure_storage(&self) -> &dyn SecureStorage {
        panic!("platform secure storage service not implemented")
    }
    fn time(&self) -> &dyn TimeService;
    fn web_view_service(&self) -> &dyn web_view::WebViewService {
        panic!("platform web view service not implemented")
    }
    fn telephony(&self) -> &dyn telephony::TelephonyService {
        panic!("platform telephony service not implemented")
    }
    fn media_library(&self) -> &dyn media_library::MediaLibrary {
        panic!("platform media library service not implemented")
    }
    fn network_status(&self) -> &dyn network_status::NetworkStatusService {
        panic!("platform network status service not implemented")
    }
}

type SharedPlatformRef = Arc<dyn Platform + Send + Sync>;

fn current_platform_cell() -> &'static RwLock<Option<SharedPlatformRef>> {
    static CURRENT_PLATFORM: OnceLock<RwLock<Option<SharedPlatformRef>>> = OnceLock::new();
    CURRENT_PLATFORM.get_or_init(|| RwLock::new(None))
}

fn current_platform_write_guard() -> std::sync::RwLockWriteGuard<'static, Option<SharedPlatformRef>>
{
    match current_platform_cell().write() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn current_platform_read_guard() -> std::sync::RwLockReadGuard<'static, Option<SharedPlatformRef>> {
    match current_platform_cell().read() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Installs the process-global platform instance used by apps that need host services
/// before they receive their `InitContext`.
pub fn set_current_platform(platform: SharedPlatformRef) {
    let mut guard = current_platform_write_guard();
    *guard = Some(platform);
}

/// Clears the process-global platform instance.
///
/// This is intended for deterministic test teardown.
pub fn clear_current_platform_for_tests() {
    let mut guard = current_platform_write_guard();
    *guard = None;
}

/// Returns the installed platform if one has been registered for the process.
#[must_use]
pub fn current_platform_if_registered() -> Option<SharedPlatformRef> {
    current_platform_read_guard().as_ref().map(Arc::clone)
}

/// Returns the installed platform or aborts when no host has registered one.
#[must_use]
pub fn current_platform() -> SharedPlatformRef {
    match current_platform_if_registered() {
        Some(platform) => platform,
        None => {
            eprintln!("oxide-platform-api: current platform not installed");
            std::process::abort();
        }
    }
}

/// Requests a redraw when a host platform has already been installed.
#[must_use]
pub fn request_redraw_if_registered() -> bool {
    match current_platform_if_registered() {
        Some(platform) => {
            platform.request_redraw();
            true
        }
        None => false,
    }
}

/// Thin adapter that reuses a shared platform instance anywhere a boxed `Platform`
/// is still required by the engine APIs.
pub struct SharedPlatform {
    inner: SharedPlatformRef,
}

impl SharedPlatform {
    #[must_use]
    pub fn new(inner: SharedPlatformRef) -> Self {
        Self { inner }
    }
}

impl Platform for SharedPlatform {
    fn run_app(&self, app: alloc::boxed::Box<dyn App>) -> ! {
        self.inner.run_app(app)
    }

    fn request_redraw(&self) {
        self.inner.request_redraw();
    }

    fn set_high_refresh(&self, enable: bool) {
        self.inner.set_high_refresh(enable);
    }

    fn set_idle_timer_disabled(&self, disabled: bool) {
        self.inner.set_idle_timer_disabled(disabled);
    }

    fn open_system_settings(&self) {
        self.inner.open_system_settings();
    }

    fn open_external_url(&self, url: &str) -> Result<(), PlatformError> {
        self.inner.open_external_url(url)
    }

    fn clipboard_get(&self) -> Option<alloc::string::String> {
        self.inner.clipboard_get()
    }

    fn clipboard_set(&self, s: &str) {
        self.inner.clipboard_set(s);
    }

    fn ime_show(&self) {
        self.inner.ime_show();
    }

    fn ime_hide(&self) {
        self.inner.ime_hide();
    }

    fn device_caps(&self) -> DeviceCaps {
        self.inner.device_caps()
    }

    fn haptics(&self) -> Arc<dyn Haptics + Send + Sync> {
        self.inner.haptics()
    }

    fn is_simulation(&self) -> bool {
        self.inner.is_simulation()
    }

    fn permissions(&self) -> &dyn Permissions {
        self.inner.permissions()
    }

    fn camera(&self) -> &dyn CameraManager {
        self.inner.camera()
    }

    fn bluetooth(&self) -> &dyn Bluetooth {
        self.inner.bluetooth()
    }

    fn bluetooth_with_restoration(&self, restore_id: &str) -> alloc::boxed::Box<dyn Bluetooth> {
        self.inner.bluetooth_with_restoration(restore_id)
    }

    fn location(&self) -> &dyn LocationService {
        self.inner.location()
    }

    fn motion(&self) -> &dyn MotionService {
        self.inner.motion()
    }

    fn push(&self) -> &dyn PushManager {
        self.inner.push()
    }

    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }

    fn networking(&self) -> &dyn Networking {
        self.inner.networking()
    }

    fn http(&self) -> &dyn HttpClient {
        self.inner.http()
    }

    fn paths(&self) -> &dyn PathService {
        self.inner.paths()
    }

    fn secure_storage(&self) -> &dyn SecureStorage {
        self.inner.secure_storage()
    }

    fn time(&self) -> &dyn TimeService {
        self.inner.time()
    }

    fn web_view_service(&self) -> &dyn web_view::WebViewService {
        self.inner.web_view_service()
    }

    fn telephony(&self) -> &dyn telephony::TelephonyService {
        self.inner.telephony()
    }

    fn media_library(&self) -> &dyn media_library::MediaLibrary {
        self.inner.media_library()
    }

    fn network_status(&self) -> &dyn network_status::NetworkStatusService {
        self.inner.network_status()
    }
}

extern crate alloc;

// ===== Tokio runtime hook (engine-provided) =====
#[cfg(feature = "tokio-runtime")]
pub mod runtime {
    use core::{future::Future, pin::Pin};
    use std::sync::OnceLock;

    pub type SpawnFn = fn(Pin<Box<dyn Future<Output = ()> + Send>>) -> ();
    static SPAWN: OnceLock<SpawnFn> = OnceLock::new();

    pub fn set_spawn(f: SpawnFn) {
        let _ = SPAWN.set(f);
    }

    pub fn spawn<F>(fut: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        if let Some(sp) = SPAWN.get() {
            sp(Box::pin(fut));
        } else {
            // no runtime installed; drop task
        }
    }
}
