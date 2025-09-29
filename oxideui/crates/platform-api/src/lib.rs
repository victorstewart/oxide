//! `OxideUI` Platform API
//!
//! Provides platform-agnostic traits and types used across the engine.

#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc, clippy::module_name_repetitions)]

use core::fmt;
use oxideui_renderer_api as rend;

pub mod clipboard;
pub mod contacts;
pub mod media_library;
pub mod url_scheme;

type PostTaskFn = dyn Fn(alloc::boxed::Box<dyn FnOnce() + Send>) + Send + Sync;

// ===== Public app interface =====

pub trait App: Send + Sync {
    fn init(&mut self, ctx: &mut InitContext);
    fn event(&mut self, e: AppEvent, ctx: &mut UpdateContext);
    fn draw(&mut self, r: &mut rend::RenderContext);
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

#[derive(Debug, Clone, Copy, PartialEq)]
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
    pub x: f32,
    pub y: f32,
    pub pressure: Option<f32>,
    pub tilt: Option<(f32, f32)>, // altitude, azimuth in radians
    pub device: PointerDevice,
}

bitflags::bitflags! {
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

bitflags::bitflags! {
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
pub enum BluetoothEvent {
    StateChanged { powered_on: bool },
    Discovered(PeripheralInfo),
    Connected(PeripheralId),
    Disconnected(PeripheralId),
    Notified { id: PeripheralId, chr: GattChar, data: alloc::vec::Vec<u8> },
    CacheUpdated(BleCacheEntry),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushNotification {
    pub user_info: alloc::collections::BTreeMap<alloc::string::String, alloc::string::String>,
    pub badge: Option<i32>,
    pub sound: Option<alloc::string::String>,
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
}

impl Default for CameraConfig {
    fn default() -> Self {
        Self {
            device: CameraDevice::Back,
            fps: 30,
            resolution: (1920, 1080),
            capture: CaptureMode::Preview,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingOptions {
    pub destination: RecordingDestination,
    pub container: RecordingContainer,
    pub include_audio: bool,
}

impl Default for RecordingOptions {
    fn default() -> Self {
        Self {
            destination: RecordingDestination::Temporary,
            container: RecordingContainer::Mp4,
            include_audio: true,
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
}

// ===== Platform and errors =====

#[derive(Debug, Clone)]
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
    fn clipboard_get(&self) -> Option<alloc::string::String>;
    fn clipboard_set(&self, s: &str);
    fn ime_show(&self);
    fn ime_hide(&self);
    fn permissions(&self) -> &dyn Permissions;
    fn camera(&self) -> &dyn CameraManager;
    fn bluetooth(&self) -> &dyn Bluetooth;
    fn location(&self) -> &dyn LocationService;
    fn motion(&self) -> &dyn MotionService;
    fn push(&self) -> &dyn PushManager;
    fn capabilities(&self) -> Capabilities;
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
