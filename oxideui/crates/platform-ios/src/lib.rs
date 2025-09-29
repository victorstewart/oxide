//! OxideUI iOS platform crate
//!
//! This module exposes safe wrappers for clipboard and haptics on iOS,
//! backed by Objective‑C bridges compiled in the host static library.
#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc, clippy::module_name_repetitions)]

use oxideui_networking::{NetworkPath, NetworkPathKind, ReachabilityManager, ReachabilityState};
#[cfg(feature = "tokio-runtime")]
use oxideui_platform_api::runtime;
use oxideui_platform_api::{
    AdvertisementData, AudioSample, BleCacheEntry, BleUuid, Bluetooth, BluetoothEvent,
    CameraConfig, CameraDevice, CameraFrame, CameraImage, CameraManager, CameraRecording,
    CameraStream, CaptureMode, GattChar, PeripheralId, PeripheralInfo, PlatformError,
    RecordingContainer, RecordingDestination, RecordingEvent, RecordingOptions, RecordingResult,
    ScanOptions,
};
use oxideui_platform_api::{
    GeoHash, GeoRegion, GeoRegionTracker, LocationAccuracy, LocationEvent, LocationOptions,
    LocationReading, LocationService, MotionSample, MotionService,
};
use oxideui_platform_api::{HapticPattern, Haptics as HapticsTrait};
use oxideui_platform_api::{PermissionDomain, PermissionStatus, Permissions};
use oxideui_platform_api::{PushManager, PushNotification, PushProvider, PushToken};

use once_cell::sync::Lazy;
use std::cmp::Reverse;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, Weak};

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct OxideLocationSample {
    latitude: f64,
    longitude: f64,
    altitude: f64,
    horizontal_accuracy: f64,
    vertical_accuracy: f64,
    speed: f64,
    course: f64,
    timestamp_ms: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct OxideLocationConfig {
    accuracy_kind: u32,
    distance_filter_m: f64,
    allow_background: u8,
    precise: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct OxideMotionSample {
    pressure_pa: f64,
    relative_altitude_m: f64,
    timestamp_ms: u64,
    has_pressure: u8,
    has_relative_altitude: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct OxideBleScanConfig {
    services_ptr: *const u8,
    service_count: usize,
    allow_duplicates: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct OxideBleScanInfo {
    id: [u8; 16],
    name_ptr: *const u8,
    name_len: usize,
    rssi_dbm: i16,
    services_ptr: *const u8,
    service_count: usize,
    manufacturer_ptr: *const u8,
    manufacturer_len: usize,
    connectable: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct OxideCamFrame {
    y_ptr: *const u8,
    y_len: usize,
    y_stride: usize,
    uv_ptr: *const u8,
    uv_len: usize,
    uv_stride: usize,
    width: i32,
    height: i32,
    timestamp_ns: u64,
    rotation_deg: u16,
    bit_depth: u8,
    matrix: u8,
    video_range: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct OxideCamAudio {
    audio_ptr: *const i16,
    sample_count: usize,
    channels: u32,
    sample_rate_hz: u32,
    timestamp_ns: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct OxideCamRecordEvent {
    kind: u32,
    path_ptr: *const u8,
    path_len: usize,
    duration_ns: u64,
    size_bytes: u64,
    had_audio: u8,
    error_code: i32,
    error_msg_ptr: *const u8,
    error_msg_len: usize,
}

extern "C" {
    fn oxideui_host_clipboard_set(utf8: *const u8, len: usize);
    fn oxideui_host_clipboard_get(out_ptr: *mut *mut u8, out_len: *mut usize) -> ::libc::c_int;
    fn oxideui_host_string_free(p: *mut u8);
    fn oxideui_host_haptics_play(pattern: u32);
    fn oxideui_host_perm_status(domain: u32) -> u32;
    fn oxideui_host_perm_request(domain: u32);
    fn oxideui_host_set_perm_callback(cb: Option<extern "C" fn(u32, u32)>);
    // Push
    fn oxideui_host_push_register();
    fn oxideui_host_push_get_device_token(
        out_ptr: *mut *mut u8,
        out_len: *mut usize,
    ) -> ::libc::c_int;
    fn oxideui_host_push_set_badge(count: i32);
    fn oxideui_host_push_clear_badge();
    fn oxideui_host_set_push_token_callback(cb: Option<extern "C" fn(u32, *const u8, usize)>);
    fn oxideui_host_set_push_notify_callback(cb: Option<extern "C" fn(*const u8, usize)>);
    // Networking
    fn oxideui_host_net_set_reachability_callback(cb: Option<extern "C" fn(u32, u32, u8)>);
    fn oxideui_host_net_start_reachability() -> ::libc::c_int;
    fn oxideui_host_net_stop_reachability();
    // BLE controls
    fn oxideui_ble_init();
    fn oxideui_ble_powered_on() -> u8;
    fn oxideui_ble_start_scan(cfg: *const OxideBleScanConfig);
    fn oxideui_ble_stop_scan();
    fn oxideui_ble_connect(id16: *const u8);
    fn oxideui_ble_disconnect(id16: *const u8);
    fn oxideui_ble_read(
        id16: *const u8,
        svc16: *const u8,
        chr16: *const u8,
        out_ptr: *mut *mut u8,
        out_len: *mut usize,
        timeout_ms: u32,
    ) -> ::libc::c_int;
    fn oxideui_ble_write(
        id16: *const u8,
        svc16: *const u8,
        chr16: *const u8,
        data: *const u8,
        len: usize,
        with_response: u8,
        timeout_ms: u32,
    ) -> ::libc::c_int;
    fn oxideui_ble_notify(
        id16: *const u8,
        svc16: *const u8,
        chr16: *const u8,
        enable: u8,
        timeout_ms: u32,
    ) -> ::libc::c_int;
    // BLE events
    fn oxideui_host_ble_set_state_cb(cb: Option<extern "C" fn(u8)>);
    fn oxideui_host_ble_set_discovered_cb(cb: Option<extern "C" fn(*const OxideBleScanInfo)>);
    fn oxideui_host_ble_set_connected_cb(cb: Option<extern "C" fn(*const u8)>);
    fn oxideui_host_ble_set_disconnected_cb(cb: Option<extern "C" fn(*const u8)>);
    fn oxideui_host_ble_set_notify_cb(
        cb: Option<extern "C" fn(*const u8, *const u8, *const u8, *const u8, usize)>,
    );
    // Camera (NV12 → Metal) controls
    fn oxideui_cam_start_default() -> ::libc::c_int;
    fn oxideui_cam_stop();
    fn oxideui_cam_set_fps(fps: i32) -> i32;
    fn oxideui_cam_set_resolution_height(h: i32) -> i32;
    fn oxideui_cam_set_bit_depth(bits: i32) -> i32;
    fn oxideui_cam_set_color_space(id: i32) -> i32;
    fn oxideui_cam_set_position(pos: i32) -> i32;
    fn oxideui_cam_set_mode(mode: i32) -> i32;
    fn oxideui_host_set_camera_callback(cb: Option<extern "C" fn(*const OxideCamFrame)>);
    fn oxideui_host_set_camera_audio_callback(cb: Option<extern "C" fn(*const OxideCamAudio)>);
    fn oxideui_host_set_camera_record_callback(
        cb: Option<extern "C" fn(*const OxideCamRecordEvent)>,
    );
    fn oxideui_cam_record_start(
        dest_ptr: *const u8,
        dest_len: usize,
        container: i32,
        include_audio: u8,
    ) -> i32;
    fn oxideui_cam_record_stop() -> i32;
    fn oxideui_cam_record_cancel() -> i32;
    fn oxideui_host_set_camera_running(on: u8) -> ::libc::c_int;
    fn oxideui_host_set_location_callback(cb: Option<extern "C" fn(*const OxideLocationSample)>);
    fn oxideui_host_set_location_error_callback(cb: Option<extern "C" fn(*const u8, usize)>);
    fn oxideui_host_location_start(cfg: OxideLocationConfig) -> i32;
    fn oxideui_host_location_stop();
    fn oxideui_host_location_request_once();
    fn oxideui_host_location_last(out: *mut OxideLocationSample) -> u8;
    fn oxideui_host_set_motion_callback(cb: Option<extern "C" fn(*const OxideMotionSample)>);
    fn oxideui_host_motion_start() -> i32;
    fn oxideui_host_motion_stop();
    fn oxideui_host_motion_is_active() -> u8;
}

pub mod clipboard {
    use super::*;

    pub fn set(s: &str) {
        unsafe { oxideui_host_clipboard_set(s.as_ptr(), s.len()) };
    }

    pub fn get() -> Option<String> {
        let mut ptr: *mut u8 = core::ptr::null_mut();
        let mut len: usize = 0;
        let ok = unsafe { oxideui_host_clipboard_get(&mut ptr, &mut len) };
        if ok == 0 || ptr.is_null() || len == 0 {
            return None;
        }
        let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
        let out = String::from_utf8_lossy(slice).into_owned();
        unsafe { oxideui_host_string_free(ptr) };
        Some(out)
    }
}

pub struct IosHaptics;

impl HapticsTrait for IosHaptics {
    fn play(&self, p: HapticPattern) {
        let code = match p {
            HapticPattern::ImpactLight => 0,
            HapticPattern::ImpactMedium => 1,
            HapticPattern::ImpactHeavy => 2,
            HapticPattern::Selection => 3,
            HapticPattern::NotificationSuccess => 4,
            HapticPattern::NotificationWarning => 5,
            HapticPattern::NotificationError => 6,
        };
        unsafe { oxideui_host_haptics_play(code) };
    }
}

extern crate alloc;

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_millis(0))
        .as_millis() as u64
}

// ===== Permissions =====

static PERM_INIT: std::sync::Once = std::sync::Once::new();
static SUBS: Lazy<
    std::sync::Mutex<Vec<Box<dyn Fn(PermissionDomain, PermissionStatus) + Send + 'static>>>,
> = Lazy::new(|| std::sync::Mutex::new(Vec::new()));

#[no_mangle]
pub extern "C" fn oxideui_perm_trampoline(domain: u32, status: u32) {
    let d = from_domain(domain);
    let s = from_status(status);
    let subs = SUBS.lock().unwrap();
    for f in subs.iter() {
        f(d, s);
    }
}

fn to_domain(d: PermissionDomain) -> u32 {
    match d {
        PermissionDomain::Notifications => 0,
        PermissionDomain::Location => 1,
        PermissionDomain::Camera => 2,
        PermissionDomain::Contacts => 3,
        PermissionDomain::Bluetooth => 4,
        PermissionDomain::Motion => 5,
        PermissionDomain::Microphone => 6,
        PermissionDomain::MediaLibrary => 7,
    }
}
fn from_domain(v: u32) -> PermissionDomain {
    match v {
        0 => PermissionDomain::Notifications,
        1 => PermissionDomain::Location,
        2 => PermissionDomain::Camera,
        3 => PermissionDomain::Contacts,
        4 => PermissionDomain::Bluetooth,
        5 => PermissionDomain::Motion,
        6 => PermissionDomain::Microphone,
        7 => PermissionDomain::MediaLibrary,
        _ => PermissionDomain::Notifications,
    }
}
fn from_status(v: u32) -> PermissionStatus {
    match v {
        0 => PermissionStatus::NotDetermined,
        1 => PermissionStatus::Denied,
        2 => PermissionStatus::Limited,
        3 => PermissionStatus::Authorized,
        _ => PermissionStatus::Denied,
    }
}

pub struct IosPermissions;

impl IosPermissions {
    fn ensure_cb() {
        PERM_INIT.call_once(|| unsafe {
            oxideui_host_set_perm_callback(Some(oxideui_perm_trampoline));
        });
    }
}

impl Permissions for IosPermissions {
    fn request(&self, domain: PermissionDomain) {
        unsafe {
            oxideui_host_perm_request(to_domain(domain));
        }
    }
    fn status(&self, domain: PermissionDomain) -> PermissionStatus {
        let s = unsafe { oxideui_host_perm_status(to_domain(domain)) };
        from_status(s)
    }
    fn subscribe(&self, f: Box<dyn Fn(PermissionDomain, PermissionStatus) + Send>) {
        Self::ensure_cb();
        SUBS.lock().unwrap().push(f);
    }
}

// ===== Location service =====

static LOCATION_INIT: std::sync::Once = std::sync::Once::new();
static LOCATION_SUBS: Lazy<std::sync::Mutex<Vec<Box<dyn Fn(LocationEvent) + Send + 'static>>>> =
    Lazy::new(|| std::sync::Mutex::new(Vec::new()));
static LOCATION_LAST: Lazy<std::sync::Mutex<Option<LocationReading>>> =
    Lazy::new(|| std::sync::Mutex::new(None));
static LOCATION_HISTORY: Lazy<Mutex<VecDeque<LocationReading>>> =
    Lazy::new(|| Mutex::new(VecDeque::with_capacity(LOCATION_HISTORY_MAX)));
static LOCATION_REGIONS: Lazy<Arc<Mutex<RegionState>>> =
    Lazy::new(|| Arc::new(Mutex::new(RegionState::default())));
const LOCATION_HISTORY_MAX: usize = 128;
static LOCATION_RUNNING: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
struct RegionState {
    entries: Vec<RegionEntry>,
}

#[derive(Clone, Copy)]
struct RegionEntry {
    region: GeoRegion,
    inside: bool,
}

struct IosGeoRegionTracker {
    state: Arc<Mutex<RegionState>>,
}

impl GeoRegionTracker for IosGeoRegionTracker {
    fn monitored_regions(&self) -> alloc::vec::Vec<GeoRegion> {
        let state = self.state.lock().unwrap();
        state.entries.iter().map(|e| e.region).collect()
    }

    fn set_regions(&self, regions: &[GeoRegion]) -> Result<(), PlatformError> {
        let mut state = self.state.lock().unwrap();
        let last = LOCATION_LAST.lock().unwrap().clone();
        state.entries = regions
            .iter()
            .map(|r| canonical_region(*r))
            .map(|region| {
                let inside = last.map_or(false, |loc| region_contains(region, loc));
                RegionEntry { region, inside }
            })
            .collect();
        Ok(())
    }
}

impl IosGeoRegionTracker {
    fn new() -> Self {
        Self { state: LOCATION_REGIONS.clone() }
    }
}

fn canonical_region(mut region: GeoRegion) -> GeoRegion {
    if region.hash.0 == 0 {
        region.hash = encode_geohash(region.center.0, region.center.1);
    }
    region
}

fn region_contains(region: GeoRegion, reading: LocationReading) -> bool {
    distance_m(region.center, (reading.latitude_deg, reading.longitude_deg)) <= region.radius_m
}

fn update_region_events(reading: LocationReading) -> alloc::vec::Vec<LocationEvent> {
    let mut events = alloc::vec::Vec::new();
    let mut state = LOCATION_REGIONS.lock().unwrap();
    for entry in state.entries.iter_mut() {
        let inside = region_contains(entry.region, reading);
        if inside && !entry.inside {
            events.push(LocationEvent::EnteredRegion(entry.region));
        } else if !inside && entry.inside {
            events.push(LocationEvent::ExitedRegion(entry.region));
        }
        entry.inside = inside;
    }
    events
}

fn encode_geohash(lat: f64, lon: f64) -> GeoHash {
    fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
        v.max(lo).min(hi)
    }
    let lat_norm = clamp((lat + 90.0) / 180.0, 0.0, 1.0);
    let lon_norm = clamp((lon + 180.0) / 360.0, 0.0, 1.0);
    let lat_i = (lat_norm * ((1u64 << 32) - 1) as f64).round() as u64;
    let lon_i = (lon_norm * ((1u64 << 32) - 1) as f64).round() as u64;
    GeoHash(interleave_bits(lat_i, lon_i))
}

fn interleave_bits(x: u64, y: u64) -> u64 {
    fn spread(mut v: u64) -> u64 {
        v &= 0x0000_0000_FFFF_FFFF;
        v = (v | (v << 16)) & 0x0000_FFFF_0000_FFFF;
        v = (v | (v << 8)) & 0x00FF_00FF_00FF_00FF;
        v = (v | (v << 4)) & 0x0F0F_0F0F_0F0F_0F0F;
        v = (v | (v << 2)) & 0x3333_3333_3333_3333;
        v = (v | (v << 1)) & 0x5555_5555_5555_5555;
        v
    }
    spread(x) | (spread(y) << 1)
}

fn distance_m(a: (f64, f64), b: (f64, f64)) -> f32 {
    const EARTH_RADIUS_M: f64 = 6_371_000.0;
    let (lat1, lon1) = (a.0.to_radians(), a.1.to_radians());
    let (lat2, lon2) = (b.0.to_radians(), b.1.to_radians());
    let dlat = lat2 - lat1;
    let dlon = lon2 - lon1;
    let sin_dlat = (dlat / 2.0).sin();
    let sin_dlon = (dlon / 2.0).sin();
    let h = sin_dlat * sin_dlat + lat1.cos() * lat2.cos() * sin_dlon * sin_dlon;
    let c = 2.0 * h.sqrt().atan2((1.0 - h).sqrt());
    (EARTH_RADIUS_M * c) as f32
}

#[no_mangle]
pub extern "C" fn oxideui_location_update_trampoline(sample: *const OxideLocationSample) {
    if sample.is_null() {
        return;
    }
    let raw = unsafe { &*sample };
    let reading = LocationReading {
        latitude_deg: raw.latitude,
        longitude_deg: raw.longitude,
        altitude_m: raw.altitude as f32,
        horizontal_accuracy_m: raw.horizontal_accuracy.max(0.0) as f32,
        vertical_accuracy_m: raw.vertical_accuracy.max(0.0) as f32,
        speed_mps: raw.speed.max(0.0) as f32,
        course_deg: if raw.course.is_sign_negative() { 0.0 } else { raw.course as f32 },
        timestamp_ms: raw.timestamp_ms,
    };
    {
        let mut hist = LOCATION_HISTORY.lock().unwrap();
        if hist.len() >= LOCATION_HISTORY_MAX {
            hist.pop_front();
        }
        hist.push_back(reading);
    }
    *LOCATION_LAST.lock().unwrap() = Some(reading);
    let region_events = update_region_events(reading);
    let subs = LOCATION_SUBS.lock().unwrap();
    for cb in subs.iter() {
        cb(LocationEvent::Update(reading));
        for ev in &region_events {
            cb(ev.clone());
        }
    }
}

#[no_mangle]
pub extern "C" fn oxideui_location_error_trampoline(msg_ptr: *const u8, len: usize) {
    if msg_ptr.is_null() || len == 0 {
        return;
    }
    let bytes = unsafe { std::slice::from_raw_parts(msg_ptr, len) };
    let msg = String::from_utf8_lossy(bytes).into_owned();
    let err = PlatformError::Unknown(msg);
    for cb in LOCATION_SUBS.lock().unwrap().iter() {
        cb(LocationEvent::Error(err.clone()));
    }
}

fn ensure_location_trampolines() {
    LOCATION_INIT.call_once(|| unsafe {
        oxideui_host_set_location_callback(Some(oxideui_location_update_trampoline));
        oxideui_host_set_location_error_callback(Some(oxideui_location_error_trampoline));
    });
}

pub struct IosLocation;

impl LocationService for IosLocation {
    fn start(&self, opts: LocationOptions) -> Result<(), PlatformError> {
        ensure_location_trampolines();
        let cfg = OxideLocationConfig {
            accuracy_kind: match opts.accuracy {
                LocationAccuracy::Reduced => 0,
                LocationAccuracy::Balanced => 1,
                LocationAccuracy::Precise => 2,
            },
            distance_filter_m: f64::from(opts.distance_filter_m),
            allow_background: if opts.allow_background_updates { 1 } else { 0 },
            precise: if opts.precise { 1 } else { 0 },
        };
        let rc = unsafe { oxideui_host_location_start(cfg) };
        if rc != 0 {
            return Err(PlatformError::Unsupported("location start failed"));
        }
        LOCATION_RUNNING.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn stop(&self) {
        LOCATION_RUNNING.store(false, Ordering::SeqCst);
        unsafe { oxideui_host_location_stop() };
    }

    fn request_once(&self) {
        ensure_location_trampolines();
        unsafe { oxideui_host_location_request_once() }
    }

    fn last(&self) -> Option<LocationReading> {
        if let Some(cached) = LOCATION_LAST.lock().unwrap().clone() {
            return Some(cached);
        }
        let mut raw = OxideLocationSample {
            latitude: 0.0,
            longitude: 0.0,
            altitude: 0.0,
            horizontal_accuracy: 0.0,
            vertical_accuracy: 0.0,
            speed: 0.0,
            course: 0.0,
            timestamp_ms: 0,
        };
        let ok = unsafe { oxideui_host_location_last(&mut raw) } != 0;
        if ok {
            let reading = LocationReading {
                latitude_deg: raw.latitude,
                longitude_deg: raw.longitude,
                altitude_m: raw.altitude as f32,
                horizontal_accuracy_m: raw.horizontal_accuracy.max(0.0) as f32,
                vertical_accuracy_m: raw.vertical_accuracy.max(0.0) as f32,
                speed_mps: raw.speed.max(0.0) as f32,
                course_deg: if raw.course.is_sign_negative() { 0.0 } else { raw.course as f32 },
                timestamp_ms: raw.timestamp_ms,
            };
            *LOCATION_LAST.lock().unwrap() = Some(reading);
            Some(reading)
        } else {
            None
        }
    }

    fn subscribe(&self, f: Box<dyn Fn(LocationEvent) + Send>) {
        ensure_location_trampolines();
        LOCATION_SUBS.lock().unwrap().push(f);
    }
    fn history(&self) -> alloc::vec::Vec<LocationReading> {
        LOCATION_HISTORY.lock().unwrap().iter().cloned().collect()
    }
    fn region_tracker(&self) -> Option<Box<dyn GeoRegionTracker>> {
        Some(Box::new(IosGeoRegionTracker::new()))
    }
}

// ===== Motion service =====

static MOTION_INIT: std::sync::Once = std::sync::Once::new();
static MOTION_SUBS: Lazy<std::sync::Mutex<Vec<Box<dyn Fn(MotionSample) + Send + 'static>>>> =
    Lazy::new(|| std::sync::Mutex::new(Vec::new()));
static MOTION_LAST: Lazy<std::sync::Mutex<Option<MotionSample>>> =
    Lazy::new(|| std::sync::Mutex::new(None));
static MOTION_HISTORY: Lazy<Mutex<VecDeque<MotionSample>>> =
    Lazy::new(|| Mutex::new(VecDeque::with_capacity(MOTION_HISTORY_MAX)));
const MOTION_HISTORY_MAX: usize = 128;
static MOTION_RUNNING: AtomicBool = AtomicBool::new(false);

#[no_mangle]
pub extern "C" fn oxideui_motion_trampoline(sample: *const OxideMotionSample) {
    if sample.is_null() {
        return;
    }
    let raw = unsafe { &*sample };
    let reading = MotionSample {
        pressure_pa: if raw.has_pressure != 0 { Some(raw.pressure_pa as f32) } else { None },
        relative_altitude_m: if raw.has_relative_altitude != 0 {
            Some(raw.relative_altitude_m as f32)
        } else {
            None
        },
        timestamp_ms: raw.timestamp_ms,
    };
    {
        let mut hist = MOTION_HISTORY.lock().unwrap();
        if hist.len() >= MOTION_HISTORY_MAX {
            hist.pop_front();
        }
        hist.push_back(reading);
    }
    *MOTION_LAST.lock().unwrap() = Some(reading);
    for cb in MOTION_SUBS.lock().unwrap().iter() {
        cb(reading);
    }
}

fn ensure_motion_trampolines() {
    MOTION_INIT.call_once(|| unsafe {
        oxideui_host_set_motion_callback(Some(oxideui_motion_trampoline));
    });
}

pub struct IosMotion;

impl MotionService for IosMotion {
    fn start(&self) -> Result<(), PlatformError> {
        ensure_motion_trampolines();
        let rc = unsafe { oxideui_host_motion_start() };
        if rc != 0 {
            return Err(PlatformError::Unsupported("motion unavailable"));
        }
        MOTION_RUNNING.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn stop(&self) {
        MOTION_RUNNING.store(false, Ordering::SeqCst);
        unsafe { oxideui_host_motion_stop() };
    }

    fn is_running(&self) -> bool {
        if MOTION_RUNNING.load(Ordering::SeqCst) {
            true
        } else {
            unsafe { oxideui_host_motion_is_active() != 0 }
        }
    }

    fn subscribe(&self, f: Box<dyn Fn(MotionSample) + Send>) {
        ensure_motion_trampolines();
        MOTION_SUBS.lock().unwrap().push(f);
    }
    fn pressure_history(&self) -> alloc::vec::Vec<MotionSample> {
        MOTION_HISTORY.lock().unwrap().iter().cloned().collect()
    }
}

// ===== Reachability =====

static REACHABILITY_TARGET: Lazy<Mutex<Weak<ReachabilityManager>>> =
    Lazy::new(|| Mutex::new(Weak::new()));
static REACHABILITY_INIT: std::sync::Once = std::sync::Once::new();

#[no_mangle]
pub extern "C" fn oxideui_reachability_trampoline(status: u32, iface: u32, expensive: u8) {
    let manager = REACHABILITY_TARGET.lock().ok().and_then(|guard| guard.clone().upgrade());
    if let Some(manager) = manager {
        let state = decode_reachability(status, iface, expensive != 0);
        manager.update(state);
    }
}

fn decode_reachability(status: u32, iface: u32, expensive: bool) -> ReachabilityState {
    if status == 0 {
        ReachabilityState::Offline
    } else {
        let kind = match iface {
            0 => NetworkPathKind::Wifi,
            1 => NetworkPathKind::Cellular,
            2 => NetworkPathKind::Wired,
            _ => NetworkPathKind::Other,
        };
        ReachabilityState::Online { path: NetworkPath { kind, expensive } }
    }
}

fn ensure_reachability_callback() {
    REACHABILITY_INIT.call_once(|| unsafe {
        oxideui_host_net_set_reachability_callback(Some(oxideui_reachability_trampoline));
    });
}

fn store_reachability_target(manager: &Arc<ReachabilityManager>) {
    if let Ok(mut slot) = REACHABILITY_TARGET.lock() {
        *slot = Arc::downgrade(manager);
    }
}

pub struct IosReachability {
    manager: Arc<ReachabilityManager>,
}

impl IosReachability {
    pub fn new(manager: Arc<ReachabilityManager>) -> Self {
        ensure_reachability_callback();
        store_reachability_target(&manager);
        Self { manager }
    }

    pub fn start(&self) -> Result<(), PlatformError> {
        let rc = unsafe { oxideui_host_net_start_reachability() };
        if rc == 0 {
            Ok(())
        } else {
            Err(PlatformError::Unsupported("reachability unavailable"))
        }
    }

    pub fn stop(&self) {
        unsafe { oxideui_host_net_stop_reachability() };
        if let Ok(mut slot) = REACHABILITY_TARGET.lock() {
            let should_clear =
                slot.upgrade().map_or(false, |current| Arc::ptr_eq(&current, &self.manager));
            if should_clear {
                *slot = Weak::new();
            }
        }
    }

    pub fn manager(&self) -> &Arc<ReachabilityManager> {
        &self.manager
    }
}

impl Drop for IosReachability {
    fn drop(&mut self) {
        self.stop();
    }
}

// ===== Push Manager =====

static PUSH_TOKEN: Lazy<std::sync::Mutex<Option<String>>> =
    Lazy::new(|| std::sync::Mutex::new(None));
static PUSH_SUBS: Lazy<std::sync::Mutex<Vec<Box<dyn Fn(PushNotification) + Send + 'static>>>> =
    Lazy::new(|| std::sync::Mutex::new(Vec::new()));
static PUSH_INIT: std::sync::Once = std::sync::Once::new();

#[no_mangle]
pub extern "C" fn oxideui_push_token_trampoline(_provider: u32, ptr: *const u8, len: usize) {
    let s = unsafe { std::slice::from_raw_parts(ptr, len) };
    if let Ok(tok) = std::str::from_utf8(s) {
        *PUSH_TOKEN.lock().unwrap() = Some(tok.to_string());
    }
}

#[no_mangle]
pub extern "C" fn oxideui_push_notify_trampoline(ptr: *const u8, len: usize) {
    let s = unsafe { std::slice::from_raw_parts(ptr, len) };
    if let Ok(json) = std::str::from_utf8(s) {
        let mut n = PushNotification {
            user_info: std::collections::BTreeMap::new(),
            badge: None,
            sound: None,
        };
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(json) {
            if let Some(obj) = v.as_object() {
                for (k, val) in obj.iter() {
                    n.user_info.insert(k.clone(), val.as_str().unwrap_or("").to_string());
                }
            }
        }
        let subs = PUSH_SUBS.lock().unwrap();
        for f in subs.iter() {
            f(n.clone());
        }
    }
}

fn init_push_trampolines() {
    PUSH_INIT.call_once(|| unsafe {
        oxideui_host_set_push_token_callback(Some(oxideui_push_token_trampoline));
        oxideui_host_set_push_notify_callback(Some(oxideui_push_notify_trampoline));
    });
}

pub struct IosPushManager;

impl PushManager for IosPushManager {
    fn register(&self) {
        init_push_trampolines();
        unsafe {
            oxideui_host_push_register();
        }
    }
    fn device_token(&self) -> Option<PushToken> {
        // Prefer cached token; otherwise query host
        if let Some(t) = PUSH_TOKEN.lock().unwrap().clone() {
            return Some(PushToken { provider: PushProvider::Apns, value: t });
        }
        let mut ptr: *mut u8 = std::ptr::null_mut();
        let mut len: usize = 0;
        let ok = unsafe { oxideui_host_push_get_device_token(&mut ptr, &mut len) };
        if ok != 0 && !ptr.is_null() && len > 0 {
            let s = unsafe { std::slice::from_raw_parts(ptr, len) };
            let tok = String::from_utf8_lossy(s).into_owned();
            unsafe { oxideui_host_string_free(ptr) };
            *PUSH_TOKEN.lock().unwrap() = Some(tok.clone());
            return Some(PushToken { provider: PushProvider::Apns, value: tok });
        }
        None
    }
    fn subscribe(&self, f: Box<dyn Fn(PushNotification) + Send>) {
        init_push_trampolines();
        PUSH_SUBS.lock().unwrap().push(f);
    }
    fn set_badge(&self, count: i32) {
        unsafe { oxideui_host_push_set_badge(count) }
    }
    fn clear_badge(&self) {
        unsafe { oxideui_host_push_clear_badge() }
    }
}

// ===== Bluetooth =====

static BLE_SUBS: Lazy<std::sync::Mutex<Vec<Box<dyn Fn(BluetoothEvent) + Send + 'static>>>> =
    Lazy::new(|| std::sync::Mutex::new(Vec::new()));
static BLE_CACHE: Lazy<Mutex<HashMap<PeripheralId, BleCacheEntry>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
const BLE_CACHE_MAX: usize = 128;
static BLE_INIT_ONCE: std::sync::Once = std::sync::Once::new();

#[no_mangle]
pub extern "C" fn oxideui_ble_state_tramp(on: u8) {
    let ev = BluetoothEvent::StateChanged { powered_on: on != 0 };
    for f in BLE_SUBS.lock().unwrap().iter() {
        f(ev.clone());
    }
}
#[no_mangle]
pub extern "C" fn oxideui_ble_disc_tramp(info: *const OxideBleScanInfo) {
    if info.is_null() {
        return;
    }
    let raw = unsafe { &*info };
    let pid = PeripheralId::from_le_bytes(raw.id);
    let name = if raw.name_len > 0 && !raw.name_ptr.is_null() {
        let bytes = unsafe { core::slice::from_raw_parts(raw.name_ptr, raw.name_len) };
        Some(alloc::string::String::from_utf8_lossy(bytes).into_owned())
    } else {
        None
    };
    let mut services: alloc::vec::Vec<BleUuid> = alloc::vec::Vec::new();
    if raw.service_count > 0 && !raw.services_ptr.is_null() {
        let slice =
            unsafe { core::slice::from_raw_parts(raw.services_ptr, raw.service_count * 16) };
        for chunk in slice.chunks(16) {
            let mut uuid = [0u8; 16];
            uuid.copy_from_slice(chunk);
            services.push(BleUuid(uuid));
        }
    }
    let manufacturer_data = if raw.manufacturer_len > 0 && !raw.manufacturer_ptr.is_null() {
        let bytes =
            unsafe { core::slice::from_raw_parts(raw.manufacturer_ptr, raw.manufacturer_len) };
        Some(bytes.to_vec())
    } else {
        None
    };
    let adv = AdvertisementData { services, manufacturer_data, connectable: raw.connectable != 0 };
    let info = PeripheralInfo { id: pid, name, rssi_dbm: raw.rssi_dbm, advertisement: adv };
    let cache_entry = store_ble_cache(&info);
    let discovered = BluetoothEvent::Discovered(info.clone());
    let cache_evt = BluetoothEvent::CacheUpdated(cache_entry);
    let subs = BLE_SUBS.lock().unwrap();
    for f in subs.iter() {
        f(discovered.clone());
        f(cache_evt.clone());
    }
}
#[no_mangle]
pub extern "C" fn oxideui_ble_conn_tramp(id: *const u8) {
    let pid = id_from_ptr(id);
    let connected = BluetoothEvent::Connected(pid);
    let cache_evt = touch_ble_cache(pid).map(BluetoothEvent::CacheUpdated);
    let subs = BLE_SUBS.lock().unwrap();
    for f in subs.iter() {
        f(connected.clone());
        if let Some(ref evt) = cache_evt {
            f(evt.clone());
        }
    }
}
#[no_mangle]
pub extern "C" fn oxideui_ble_discconn_tramp(id: *const u8) {
    let pid = id_from_ptr(id);
    let disconnected = BluetoothEvent::Disconnected(pid);
    let cache_evt = touch_ble_cache(pid).map(BluetoothEvent::CacheUpdated);
    let subs = BLE_SUBS.lock().unwrap();
    for f in subs.iter() {
        f(disconnected.clone());
        if let Some(ref evt) = cache_evt {
            f(evt.clone());
        }
    }
}
#[no_mangle]
pub extern "C" fn oxideui_ble_notify_tramp(
    id: *const u8,
    svc: *const u8,
    chr: *const u8,
    data: *const u8,
    len: usize,
) {
    let pid = id_from_ptr(id);
    let service = BleUuid(copy16(svc));
    let characteristic = BleUuid(copy16(chr));
    let chr_key = GattChar { service, characteristic };
    let bytes = unsafe { std::slice::from_raw_parts(data as *const u8, len) }.to_vec();
    let notify = BluetoothEvent::Notified { id: pid, chr: chr_key, data: bytes };
    let cache_evt = touch_ble_cache(pid).map(BluetoothEvent::CacheUpdated);
    let subs = BLE_SUBS.lock().unwrap();
    for f in subs.iter() {
        f(notify.clone());
        if let Some(ref evt) = cache_evt {
            f(evt.clone());
        }
    }
}

fn ensure_ble_trampolines() {
    BLE_INIT_ONCE.call_once(|| unsafe {
        oxideui_host_ble_set_state_cb(Some(oxideui_ble_state_tramp));
        oxideui_host_ble_set_discovered_cb(Some(oxideui_ble_disc_tramp));
        oxideui_host_ble_set_connected_cb(Some(oxideui_ble_conn_tramp));
        oxideui_host_ble_set_disconnected_cb(Some(oxideui_ble_discconn_tramp));
        oxideui_host_ble_set_notify_cb(Some(oxideui_ble_notify_tramp));
        oxideui_ble_init();
    });
}

fn id_from_ptr(p: *const u8) -> PeripheralId {
    let b = copy16(p);
    u128::from_le_bytes(b)
}
fn copy16(p: *const u8) -> [u8; 16] {
    unsafe { *(p as *const [u8; 16]) }
}

fn store_ble_cache(entry: &PeripheralInfo) -> BleCacheEntry {
    let cached = BleCacheEntry { peripheral: entry.clone(), last_seen_ms: now_ms() };
    let mut cache = BLE_CACHE.lock().unwrap();
    cache.insert(cached.peripheral.id, cached.clone());
    if cache.len() > BLE_CACHE_MAX {
        if let Some((oldest, _)) =
            cache.iter().min_by_key(|(_, e)| e.last_seen_ms).map(|(k, v)| (*k, v.last_seen_ms))
        {
            cache.remove(&oldest);
        }
    }
    cached
}

fn touch_ble_cache(id: PeripheralId) -> Option<BleCacheEntry> {
    let mut cache = BLE_CACHE.lock().unwrap();
    if let Some(entry) = cache.get_mut(&id) {
        entry.last_seen_ms = now_ms();
        return Some(entry.clone());
    }
    None
}

fn current_ble_cache() -> alloc::vec::Vec<BleCacheEntry> {
    let mut items: alloc::vec::Vec<_> = BLE_CACHE.lock().unwrap().values().cloned().collect();
    items.sort_by_key(|entry| Reverse(entry.last_seen_ms));
    items
}

pub struct IosBluetooth;

impl Bluetooth for IosBluetooth {
    fn powered_on(&self) -> bool {
        unsafe { oxideui_ble_powered_on() != 0 }
    }
    fn subscribe_events(&self, f: Box<dyn Fn(BluetoothEvent) + Send>) {
        ensure_ble_trampolines();
        BLE_SUBS.lock().unwrap().push(f);
    }
    fn start_scan(&self, opts: &ScanOptions) {
        let mut bytes: Vec<u8> = Vec::with_capacity(opts.services.len() * 16);
        for s in &opts.services {
            bytes.extend_from_slice(&s.0);
        }
        let cfg = OxideBleScanConfig {
            services_ptr: if bytes.is_empty() { core::ptr::null() } else { bytes.as_ptr() },
            service_count: opts.services.len(),
            allow_duplicates: if opts.allow_duplicates { 1 } else { 0 },
        };
        unsafe {
            oxideui_ble_start_scan(&cfg);
        }
    }
    fn stop_scan(&self) {
        unsafe { oxideui_ble_stop_scan() }
    }
    fn connect(&self, id: PeripheralId) {
        let b = id.to_le_bytes();
        unsafe { oxideui_ble_connect(b.as_ptr()) }
    }
    fn disconnect(&self, id: PeripheralId) {
        let b = id.to_le_bytes();
        unsafe { oxideui_ble_disconnect(b.as_ptr()) }
    }
    fn read(&self, id: PeripheralId, chr: GattChar) -> Result<Vec<u8>, PlatformError> {
        let b = id.to_le_bytes();
        let mut out: *mut u8 = std::ptr::null_mut();
        let mut len: usize = 0;
        let ok = unsafe {
            oxideui_ble_read(
                b.as_ptr(),
                chr.service.0.as_ptr(),
                chr.characteristic.0.as_ptr(),
                &mut out,
                &mut len,
                5000,
            )
        };
        if ok == 0 {
            Err(PlatformError::Busy)
        } else {
            let v = unsafe { std::slice::from_raw_parts(out, len) }.to_vec();
            unsafe { oxideui_host_string_free(out) };
            Ok(v)
        }
    }
    fn write(
        &self,
        id: PeripheralId,
        chr: GattChar,
        data: &[u8],
        with_response: bool,
    ) -> Result<(), PlatformError> {
        let b = id.to_le_bytes();
        let ok = unsafe {
            oxideui_ble_write(
                b.as_ptr(),
                chr.service.0.as_ptr(),
                chr.characteristic.0.as_ptr(),
                data.as_ptr(),
                data.len(),
                if with_response { 1 } else { 0 },
                5000,
            )
        };
        if ok == 0 {
            Err(PlatformError::Busy)
        } else {
            Ok(())
        }
    }
    fn notify(&self, id: PeripheralId, chr: GattChar, enable: bool) -> Result<(), PlatformError> {
        let b = id.to_le_bytes();
        let ok = unsafe {
            oxideui_ble_notify(
                b.as_ptr(),
                chr.service.0.as_ptr(),
                chr.characteristic.0.as_ptr(),
                if enable { 1 } else { 0 },
                2000,
            )
        };
        if ok == 0 {
            Err(PlatformError::Busy)
        } else {
            Ok(())
        }
    }
    fn advertise_start(&self, _name: &str, _services: &[BleUuid]) { /* not implemented */
    }
    fn advertise_stop(&self) { /* not implemented */
    }

    fn cached_peripherals(&self) -> Vec<BleCacheEntry> {
        current_ble_cache()
    }
}

// ===== Camera manager =====

pub struct IosCameraManager;

static IOS_CAMERA_MANAGER: IosCameraManager = IosCameraManager;

pub fn camera_manager() -> &'static IosCameraManager {
    &IOS_CAMERA_MANAGER
}

struct CameraSubscriber {
    id: u64,
    frame_cb: alloc::boxed::Box<dyn Fn(CameraFrame) + Send>,
    audio_cb: Option<alloc::boxed::Box<dyn Fn(AudioSample) + Send>>,
}

#[derive(Debug, Clone, Copy)]
struct CameraSettings {
    device: CameraDevice,
    fps: u32,
    width: u32,
    height: u32,
    mode: CaptureMode,
}

impl Default for CameraSettings {
    fn default() -> Self {
        Self {
            device: CameraDevice::Back,
            fps: 30,
            width: 1920,
            height: 1080,
            mode: CaptureMode::Preview,
        }
    }
}

struct CamState {
    subs: Mutex<alloc::vec::Vec<CameraSubscriber>>,
    next_id: AtomicU64,
    settings: Mutex<CameraSettings>,
    callback_once: std::sync::Once,
    record_once: std::sync::Once,
    record_cb: Mutex<Option<alloc::boxed::Box<dyn Fn(RecordingEvent) + Send>>>,
    recording: AtomicBool,
}

impl CamState {
    fn new() -> Self {
        Self {
            subs: Mutex::new(alloc::vec::Vec::new()),
            next_id: AtomicU64::new(1),
            settings: Mutex::new(CameraSettings::default()),
            callback_once: std::sync::Once::new(),
            record_once: std::sync::Once::new(),
            record_cb: Mutex::new(None),
            recording: AtomicBool::new(false),
        }
    }

    fn has_audio_subscribers(&self) -> bool {
        self.subs.lock().unwrap().iter().any(|s| s.audio_cb.is_some())
    }

    fn ensure_callback(&self) {
        self.callback_once.call_once(|| unsafe {
            oxideui_host_set_camera_callback(Some(oxideui_cam_frame_trampoline));
            oxideui_host_set_camera_audio_callback(Some(oxideui_cam_audio_trampoline));
        });
    }

    fn ensure_record_callback(&self) {
        self.record_once.call_once(|| unsafe {
            oxideui_host_set_camera_record_callback(Some(oxideui_cam_record_trampoline));
        });
    }

    fn try_begin_recording(
        &self,
        cb: alloc::boxed::Box<dyn Fn(RecordingEvent) + Send>,
    ) -> Result<(), PlatformError> {
        if self.recording.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err()
        {
            return Err(PlatformError::Busy);
        }
        let mut slot = self.record_cb.lock().unwrap();
        *slot = Some(cb);
        Ok(())
    }

    fn finish_recording(&self) -> Option<alloc::boxed::Box<dyn Fn(RecordingEvent) + Send>> {
        self.recording.store(false, Ordering::SeqCst);
        let mut slot = self.record_cb.lock().unwrap();
        slot.take()
    }

    fn apply_settings(&self, cfg: CameraConfig) {
        let mut settings = self.settings.lock().unwrap();
        settings.device = cfg.device;
        settings.fps = cfg.fps;
        settings.width = cfg.resolution.0;
        settings.height = cfg.resolution.1;
        settings.mode = cfg.capture;
        drop(settings);
        Self::apply_device(cfg.device);
        Self::apply_fps(cfg.fps);
        Self::apply_resolution(cfg.resolution.1);
        Self::apply_mode(cfg.capture);
    }

    fn apply_device(device: CameraDevice) {
        let pos = match device {
            CameraDevice::Front => 1,
            CameraDevice::Back => 0,
        };
        unsafe {
            let _ = oxideui_cam_set_position(pos);
        }
    }

    fn apply_fps(fps: u32) {
        unsafe {
            let _ = oxideui_cam_set_fps(fps as i32);
        }
    }

    fn apply_resolution(height: u32) {
        unsafe {
            let _ = oxideui_cam_set_resolution_height(height as i32);
        }
    }

    fn apply_mode(mode: CaptureMode) {
        let mode_code = match mode {
            CaptureMode::Preview => 0,
            CaptureMode::Photo => 1,
            CaptureMode::Video => 2,
        };
        unsafe {
            let _ = oxideui_cam_set_mode(mode_code);
        }
    }
}

static CAM_STATE: Lazy<CamState> = Lazy::new(CamState::new);

struct IosCameraStream {
    id: u64,
}

struct IosCameraRecording {
    active: AtomicBool,
}

impl CameraStream for IosCameraStream {
    fn stop(&self) {
        remove_camera_subscriber(self.id);
    }
}

impl Drop for IosCameraStream {
    fn drop(&mut self) {
        self.stop();
    }
}

impl IosCameraRecording {
    fn new() -> Self {
        Self { active: AtomicBool::new(true) }
    }
}

impl CameraRecording for IosCameraRecording {
    fn stop(&self) {
        if self.active.swap(false, Ordering::SeqCst) {
            unsafe {
                let _ = oxideui_cam_record_stop();
            }
        }
    }

    fn cancel(&self) {
        if self.active.swap(false, Ordering::SeqCst) {
            unsafe {
                let _ = oxideui_cam_record_cancel();
            }
        }
    }
}

impl IosCameraManager {
    fn start_capture(&self) -> Result<(), PlatformError> {
        let rc = unsafe { oxideui_cam_start_default() };
        if rc != 0 {
            return Err(PlatformError::Unsupported("camera start failed"));
        }
        unsafe {
            let _ = oxideui_host_set_camera_running(1);
        }
        Ok(())
    }

    fn stop_capture(&self) {
        unsafe {
            oxideui_cam_stop();
            let _ = oxideui_host_set_camera_running(0);
        }
    }
}

fn remove_camera_subscriber(id: u64) {
    let mut subs = CAM_STATE.subs.lock().unwrap();
    if let Some(pos) = subs.iter().position(|s| s.id == id) {
        subs.remove(pos);
    }
    let should_stop = subs.is_empty();
    drop(subs);
    if should_stop {
        IOS_CAMERA_MANAGER.stop_capture();
    }
}

impl CameraManager for IosCameraManager {
    fn start_stream(
        &self,
        cfg: CameraConfig,
        on_frame: alloc::boxed::Box<dyn Fn(CameraFrame) + Send>,
        on_audio: Option<alloc::boxed::Box<dyn Fn(AudioSample) + Send>>,
    ) -> Result<alloc::boxed::Box<dyn CameraStream + Send>, PlatformError> {
        CAM_STATE.ensure_callback();
        CAM_STATE.apply_settings(cfg);
        let mut subs = CAM_STATE.subs.lock().unwrap();
        let id = CAM_STATE.next_id.fetch_add(1, Ordering::Relaxed);
        let is_first = subs.is_empty();
        subs.push(CameraSubscriber { id, frame_cb: on_frame, audio_cb: on_audio });
        drop(subs);
        if is_first {
            if let Err(e) = self.start_capture() {
                remove_camera_subscriber(id);
                return Err(e);
            }
        }
        Ok(Box::new(IosCameraStream { id }))
    }

    fn start_recording(
        &self,
        options: RecordingOptions,
        on_event: alloc::boxed::Box<dyn Fn(RecordingEvent) + Send>,
    ) -> Result<alloc::boxed::Box<dyn CameraRecording + Send>, PlatformError> {
        CAM_STATE.ensure_record_callback();
        CAM_STATE.try_begin_recording(on_event)?;

        let RecordingOptions { destination, container, include_audio } = options;
        let container_code = match container {
            RecordingContainer::Mp4 => 0,
            RecordingContainer::Mov => 1,
        };

        let mut path_buf: Option<alloc::vec::Vec<u8>> = None;
        let (dest_ptr, dest_len) = match destination {
            RecordingDestination::Temporary => (core::ptr::null(), 0),
            RecordingDestination::File { path } => {
                let bytes = path.into_bytes();
                let ptr = bytes.as_ptr();
                let len = bytes.len();
                path_buf = Some(bytes);
                (ptr, len)
            }
        };

        let audio_flag = if include_audio { 1 } else { 0 };
        let rc =
            unsafe { oxideui_cam_record_start(dest_ptr, dest_len, container_code, audio_flag) };
        drop(path_buf);
        if rc != 0 {
            let _ = CAM_STATE.finish_recording();
            return Err(PlatformError::Unsupported("camera recording unavailable"));
        }
        Ok(Box::new(IosCameraRecording::new()))
    }

    fn select_device(&self, device: CameraDevice) {
        let mut settings = CAM_STATE.settings.lock().unwrap();
        settings.device = device;
        drop(settings);
        CamState::apply_device(device);
    }

    fn set_fps(&self, fps: u32) {
        let mut settings = CAM_STATE.settings.lock().unwrap();
        settings.fps = fps;
        drop(settings);
        CamState::apply_fps(fps);
    }

    fn set_resolution(&self, width: u32, height: u32) {
        let mut settings = CAM_STATE.settings.lock().unwrap();
        settings.width = width;
        settings.height = height;
        drop(settings);
        CamState::apply_resolution(height);
    }

    fn set_mode(&self, mode: CaptureMode) {
        let mut settings = CAM_STATE.settings.lock().unwrap();
        settings.mode = mode;
        drop(settings);
        CamState::apply_mode(mode);
    }
}

fn dispatch_camera_frame(frame: CameraFrame) {
    let subs = CAM_STATE.subs.lock().unwrap();
    for sub in subs.iter() {
        (sub.frame_cb)(frame.clone());
    }
}

fn dispatch_camera_audio(sample: AudioSample) {
    let subs = CAM_STATE.subs.lock().unwrap();
    for sub in subs.iter() {
        if let Some(cb) = sub.audio_cb.as_ref() {
            cb(sample.clone());
        }
    }
}

const RECORD_EVENT_COMPLETED: u32 = 0;
const RECORD_EVENT_CANCELLED: u32 = 1;
const RECORD_EVENT_FAILED: u32 = 2;

fn record_error_from(code: i32, msg: alloc::string::String) -> PlatformError {
    match code {
        1 => PlatformError::PermissionDenied("camera recording permission denied"),
        2 => PlatformError::CapabilityDisabled("camera recording"),
        3 => PlatformError::NotFound("camera recording destination"),
        4 => PlatformError::Busy,
        5 => PlatformError::Invalid("camera recording options invalid"),
        6 => PlatformError::Unsupported("camera recording unsupported"),
        7 => PlatformError::Io(msg),
        _ => PlatformError::Unknown(msg),
    }
}

fn dispatch_camera_record(event: RecordingEvent) {
    if let Some(cb) = CAM_STATE.finish_recording() {
        cb(event);
    }
}

#[no_mangle]
pub extern "C" fn oxideui_cam_audio_trampoline(audio: *const OxideCamAudio) {
    if audio.is_null() {
        return;
    }
    if !CAM_STATE.has_audio_subscribers() {
        return;
    }
    let raw = unsafe { &*audio };
    if raw.audio_ptr.is_null()
        || raw.sample_count == 0
        || raw.channels == 0
        || raw.sample_rate_hz == 0
    {
        return;
    }
    let samples = unsafe { std::slice::from_raw_parts(raw.audio_ptr, raw.sample_count) }.to_vec();
    let sample = AudioSample {
        channels: raw.channels,
        sample_rate_hz: raw.sample_rate_hz,
        data: samples,
        timestamp_ns: raw.timestamp_ns,
    };
    dispatch_camera_audio(sample);
}

#[no_mangle]
pub extern "C" fn oxideui_cam_record_trampoline(event: *const OxideCamRecordEvent) {
    if event.is_null() {
        CAM_STATE.finish_recording();
        return;
    }
    let raw = unsafe { &*event };
    let rec_event = match raw.kind {
        RECORD_EVENT_COMPLETED => {
            let path = if raw.path_ptr.is_null() || raw.path_len == 0 {
                alloc::string::String::new()
            } else {
                let bytes = unsafe { core::slice::from_raw_parts(raw.path_ptr, raw.path_len) };
                alloc::string::String::from_utf8_lossy(bytes).into_owned()
            };
            RecordingEvent::Completed(RecordingResult {
                path,
                duration_ns: raw.duration_ns,
                size_bytes: raw.size_bytes,
                had_audio: raw.had_audio != 0,
            })
        }
        RECORD_EVENT_CANCELLED => RecordingEvent::Cancelled,
        RECORD_EVENT_FAILED => {
            let msg = if raw.error_msg_ptr.is_null() || raw.error_msg_len == 0 {
                alloc::string::String::new()
            } else {
                let bytes =
                    unsafe { core::slice::from_raw_parts(raw.error_msg_ptr, raw.error_msg_len) };
                alloc::string::String::from_utf8_lossy(bytes).into_owned()
            };
            RecordingEvent::Failed(record_error_from(raw.error_code, msg))
        }
        _ => RecordingEvent::Failed(record_error_from(
            -1,
            alloc::string::String::from("unknown camera recording event"),
        )),
    };
    dispatch_camera_record(rec_event);
}

#[no_mangle]
pub extern "C" fn oxideui_cam_frame_trampoline(frame: *const OxideCamFrame) {
    if frame.is_null() {
        return;
    }
    let raw = unsafe { &*frame };
    let width = raw.width.max(0) as u32;
    let height = raw.height.max(0) as u32;
    let y_slice = if !raw.y_ptr.is_null() && raw.y_len > 0 {
        unsafe { core::slice::from_raw_parts(raw.y_ptr, raw.y_len) }
    } else {
        &[]
    };
    let uv_slice = if !raw.uv_ptr.is_null() && raw.uv_len > 0 {
        unsafe { core::slice::from_raw_parts(raw.uv_ptr, raw.uv_len) }
    } else {
        &[]
    };
    let image = CameraImage::Nv12 {
        y_plane: y_slice.to_vec(),
        uv_plane: uv_slice.to_vec(),
        stride_y: raw.y_stride as u32,
        stride_uv: raw.uv_stride as u32,
        bit_depth: raw.bit_depth,
        matrix: raw.matrix,
        video_range: raw.video_range,
    };
    let frame = CameraFrame {
        image,
        size: (width, height),
        timestamp_ns: raw.timestamp_ns,
        rotation_deg: raw.rotation_deg,
    };
    dispatch_camera_frame(frame);
}

// ===== Camera (start/stop) =====
pub mod camera {
    pub fn start_default() -> i32 {
        unsafe { super::oxideui_cam_start_default() }
    }

    pub fn stop() {
        unsafe { super::oxideui_cam_stop() }
    }

    pub fn set_fps(fps: i32) -> i32 {
        unsafe { super::oxideui_cam_set_fps(fps) }
    }

    pub fn set_resolution_height(h: i32) -> i32 {
        unsafe { super::oxideui_cam_set_resolution_height(h) }
    }

    pub fn set_bit_depth(bits: i32) -> i32 {
        unsafe { super::oxideui_cam_set_bit_depth(bits) }
    }

    pub fn set_color_space(id: i32) -> i32 {
        unsafe { super::oxideui_cam_set_color_space(id) }
    }

    // Convenience profiles (best-effort; device may clamp)
    pub fn enter_background_mode() {
        // Prefer lower power: 8-bit, sRGB, ~720p, 24 fps
        let _ = set_bit_depth(8);
        let _ = set_color_space(0); // sRGB
        let _ = set_resolution_height(720);
        let _ = set_fps(24);
    }

    pub fn enter_camera_mode() {
        // Prefer quality: 10-bit when possible, P3, ~1080p, 30 fps
        let _ = set_bit_depth(10);
        let _ = set_color_space(1); // P3 (best-effort)
        let _ = set_resolution_height(1080);
        let _ = set_fps(30);
    }

    // ---- Capability queries (fast C-ABI arrays) ----
    extern "C" {
        fn oxideui_cam_query_formats(
            out_ptr: *mut *mut ::core::ffi::c_void,
            out_count: *mut usize,
        ) -> ::libc::c_int;
        fn oxideui_cam_query_pixfmts(
            out_ptr: *mut *mut ::core::ffi::c_void,
            out_count: *mut usize,
        ) -> ::libc::c_int;
        fn oxideui_cam_caps_free(p: *mut ::core::ffi::c_void);
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    pub struct CamFormat {
        pub width: i32,
        pub height: i32,
        pub fps_min: f32,
        pub fps_max: f32,
        pub color_spaces_mask: u32, // bit 0: sRGB, bit 1: P3
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    pub struct CamPixFmt {
        pub code: u32,      // CVPixelFormatType
        pub bit_depth: i32, // 8 or 10
        pub range: i32,     // 0 full, 1 video
    }

    pub fn query_formats() -> alloc::vec::Vec<CamFormat> {
        let mut p: *mut ::core::ffi::c_void = core::ptr::null_mut();
        let mut n: usize = 0;
        let ok = unsafe { oxideui_cam_query_formats(&mut p, &mut n) };
        if ok == 0 || p.is_null() || n == 0 {
            return alloc::vec::Vec::new();
        }
        let slice = unsafe { core::slice::from_raw_parts(p as *const CamFormat, n) };
        let out = slice.to_vec();
        unsafe { oxideui_cam_caps_free(p) };
        out
    }

    pub fn query_pixel_formats() -> alloc::vec::Vec<CamPixFmt> {
        let mut p: *mut ::core::ffi::c_void = core::ptr::null_mut();
        let mut n: usize = 0;
        let ok = unsafe { oxideui_cam_query_pixfmts(&mut p, &mut n) };
        if ok == 0 || p.is_null() || n == 0 {
            return alloc::vec::Vec::new();
        }
        let slice = unsafe { core::slice::from_raw_parts(p as *const CamPixFmt, n) };
        let out = slice.to_vec();
        unsafe { oxideui_cam_caps_free(p) };
        out
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum CameraPolicy {
        Background,
        Camera,
    }

    #[derive(Debug, Clone, Copy)]
    pub struct ResolutionCaps {
        pub width: i32,
        pub height: i32,
        pub fps_min: f32,
        pub fps_max: f32,
        pub color_spaces_mask: u32,
    }

    pub fn resolution_catalog() -> alloc::vec::Vec<ResolutionCaps> {
        resolution_catalog_from_formats(&query_formats())
    }

    #[doc(hidden)]
    pub fn resolution_catalog_from_formats(
        formats: &[CamFormat],
    ) -> alloc::vec::Vec<ResolutionCaps> {
        let mut map: std::collections::BTreeMap<(i32, i32), ResolutionCaps> =
            std::collections::BTreeMap::new();
        for f in formats.iter().copied() {
            let key = (f.width, f.height);
            map.entry(key)
                .and_modify(|e| {
                    e.fps_min = e.fps_min.min(f.fps_min);
                    e.fps_max = e.fps_max.max(f.fps_max);
                    e.color_spaces_mask |= f.color_spaces_mask;
                })
                .or_insert(ResolutionCaps {
                    width: f.width,
                    height: f.height,
                    fps_min: f.fps_min,
                    fps_max: f.fps_max,
                    color_spaces_mask: f.color_spaces_mask,
                });
        }
        map.into_values().collect()
    }

    #[derive(Debug, Clone, Copy)]
    pub struct RecommendedProfile {
        pub bit_depth: i32,   // 8 or 10
        pub color_space: i32, // 0=sRGB, 1=P3
        pub height: i32,      // desired capture height
        pub fps: i32,         // desired fps
    }

    pub fn recommend(policy: CameraPolicy) -> Option<RecommendedProfile> {
        let pix = query_pixel_formats();
        let caps = resolution_catalog();
        recommend_from(policy, &pix, &caps)
    }

    #[doc(hidden)]
    pub fn recommend_from(
        policy: CameraPolicy,
        pixel_formats: &[CamPixFmt],
        caps: &[ResolutionCaps],
    ) -> Option<RecommendedProfile> {
        if caps.is_empty() {
            return None;
        }

        let has_10 = pixel_formats.iter().any(|p| p.bit_depth == 10);
        let bit_depth = match policy {
            CameraPolicy::Camera if has_10 => 10,
            _ => 8,
        };

        let (target_h, target_fps) = match policy {
            CameraPolicy::Background => (720, 24),
            CameraPolicy::Camera => (1080, 30),
        };

        let mut best: Option<ResolutionCaps> = None;
        let mut best_score = i64::MAX;
        for r in caps.iter().copied() {
            let ok_fps = (r.fps_min <= target_fps as f32 + 0.001)
                && (r.fps_max + 0.001 >= target_fps as f32);
            let fps_penalty = if ok_fps { 0 } else { 10_000 };
            let dh = (r.height - target_h).abs() as i64;
            let score = dh * 100 + fps_penalty as i64;
            if score < best_score {
                best_score = score;
                best = Some(r);
            }
        }
        let chosen = best?;

        let color_space = match policy {
            CameraPolicy::Camera => {
                if (chosen.color_spaces_mask & (1 << 1)) != 0 {
                    1
                } else {
                    0
                }
            }
            CameraPolicy::Background => 0,
        };

        let fps = crate::clamp_fps_to_caps(target_fps, target_fps, chosen.fps_min, chosen.fps_max);

        Some(RecommendedProfile { bit_depth, color_space, height: chosen.height, fps })
    }

    // ----- Preset-style catalog and selection -----
    #[derive(Debug, Clone, Copy)]
    pub struct PresetCaps {
        pub preset_height: i32, // 480, 540, 720, 1080, 1440, 2160
        pub fps_min: f32,
        pub fps_max: f32,
        pub color_spaces_mask: u32,
    }

    fn nearest_preset(h: i32) -> i32 {
        const PRESETS: [i32; 6] = [480, 540, 720, 1080, 1440, 2160];
        let mut best = PRESETS[0];
        let mut best_d = (h - best).abs();
        for p in PRESETS.iter().copied() {
            let d = (h - p).abs();
            if d < best_d {
                best_d = d;
                best = p;
            }
        }
        best
    }

    pub fn preset_catalog() -> alloc::vec::Vec<PresetCaps> {
        let caps = resolution_catalog();
        preset_catalog_from_caps(&caps)
    }

    #[doc(hidden)]
    pub fn preset_catalog_from_caps(caps: &[ResolutionCaps]) -> alloc::vec::Vec<PresetCaps> {
        use std::collections::BTreeMap;
        let mut agg: BTreeMap<i32, PresetCaps> = BTreeMap::new();
        for r in caps.iter().copied() {
            let p = nearest_preset(r.height);
            agg.entry(p)
                .and_modify(|e| {
                    e.fps_min = e.fps_min.min(r.fps_min);
                    e.fps_max = e.fps_max.max(r.fps_max);
                    e.color_spaces_mask |= r.color_spaces_mask;
                })
                .or_insert(PresetCaps {
                    preset_height: p,
                    fps_min: r.fps_min,
                    fps_max: r.fps_max,
                    color_spaces_mask: r.color_spaces_mask,
                });
        }
        agg.into_values().collect()
    }

    pub fn recommend_for_preset(
        preset_height: i32,
        target_fps: i32,
        prefer_p3: bool,
        prefer_10bit: bool,
    ) -> Option<RecommendedProfile> {
        let pix = query_pixel_formats();
        let presets = preset_catalog();
        recommend_for_preset_from(
            preset_height,
            target_fps,
            prefer_p3,
            prefer_10bit,
            &pix,
            &presets,
        )
    }

    #[doc(hidden)]
    pub fn recommend_for_preset_from(
        preset_height: i32,
        target_fps: i32,
        prefer_p3: bool,
        prefer_10bit: bool,
        pixel_formats: &[CamPixFmt],
        presets: &[PresetCaps],
    ) -> Option<RecommendedProfile> {
        let has_10 = prefer_10bit && pixel_formats.iter().any(|p| p.bit_depth == 10);
        let bit_depth = if has_10 { 10 } else { 8 };

        let p = nearest_preset(preset_height);
        let caps = presets.iter().find(|c| c.preset_height == p).copied()?;

        let color_space = if prefer_p3 && (caps.color_spaces_mask & (1 << 1)) != 0 { 1 } else { 0 };

        let fps = crate::clamp_fps_to_caps(target_fps, target_fps, caps.fps_min, caps.fps_max);

        Some(RecommendedProfile { bit_depth, color_space, height: p, fps })
    }
}

#[allow(clippy::cast_possible_truncation)]
fn clamp_fps_to_caps(default_fps: i32, current: i32, min: f32, max: f32) -> i32 {
    let max_i = if max.is_finite() { max.floor() as i32 } else { default_fps };
    let min_i = if min.is_finite() { min.ceil() as i32 } else { default_fps };
    let (lo, hi) = if min_i > max_i { (max_i, max_i) } else { (min_i, max_i) };
    let mut fps = current;
    if fps > hi {
        fps = hi;
    }
    if fps < lo {
        fps = lo;
    }
    if fps <= 0 {
        default_fps.max(1)
    } else {
        fps
    }
}

// ===== Contacts =====

use oxideui_platform_api::contacts::{
    Contact, ContactChange, ContactEmail, ContactPhone, ContactsFetchResult, ContactsManager,
};

/// Contacts state for incremental updates
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct OxideContactsState {
    waypoint_ptr: *const u8,
    waypoint_len: usize,
    carrier_region_ptr: *const u8,
    carrier_region_len: usize,
}

/// FFI contact structure
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct OxideContact {
    identifier_ptr: *const u8,
    identifier_len: usize,
    given_name_ptr: *const u8,
    given_name_len: usize,
    family_name_ptr: *const u8,
    family_name_len: usize,
    phones_ptr: *const OxideContactPhone,
    phones_count: usize,
    emails_ptr: *const OxideContactEmail,
    emails_count: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct OxideContactPhone {
    number_ptr: *const u8,
    number_len: usize,
    region_ptr: *const u8,
    region_len: usize,
    normalized_ptr: *const u8,
    normalized_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct OxideContactEmail {
    address_ptr: *const u8,
    address_len: usize,
    is_valid: u8,
}

extern "C" {
    fn oxide_contacts_fetch(
        waypoint_ptr: *const u8,
        waypoint_len: usize,
        out_contacts: *mut *const OxideContact,
        out_count: *mut usize,
        out_state: *mut OxideContactsState,
    ) -> i32;

    fn oxide_contacts_free(contacts: *const OxideContact, count: usize);

    fn oxide_contacts_get_carrier_region(out_ptr: *mut *const u8, out_len: *mut usize) -> i32;
}

pub struct IosContactsManager {
    next_subscription_id: u32,
    // Subscriptions would be stored here in a production impl
}

impl Default for IosContactsManager {
    fn default() -> Self {
        Self { next_subscription_id: 1 }
    }
}

impl ContactsManager for IosContactsManager {
    fn fetch_contacts(&mut self, waypoint: Option<String>) -> ContactsFetchResult {
        let waypoint_bytes = waypoint.as_ref().map(|s| s.as_bytes());
        let waypoint_ptr = waypoint_bytes.map_or(std::ptr::null(), |b| b.as_ptr());
        let waypoint_len = waypoint_bytes.map_or(0, |b| b.len());

        let mut contacts_ptr: *const OxideContact = std::ptr::null();
        let mut count: usize = 0;
        let mut state = OxideContactsState {
            waypoint_ptr: std::ptr::null(),
            waypoint_len: 0,
            carrier_region_ptr: std::ptr::null(),
            carrier_region_len: 0,
        };

        let result = unsafe {
            oxide_contacts_fetch(
                waypoint_ptr,
                waypoint_len,
                &mut contacts_ptr,
                &mut count,
                &mut state,
            )
        };

        if result == -1 {
            return ContactsFetchResult::Denied;
        }

        if result < 0 {
            return ContactsFetchResult::Error(format!("Failed to fetch contacts: {}", result));
        }

        // Convert C contacts to Rust
        let contacts: Vec<Contact> = (0..count)
            .filter_map(|i| unsafe {
                let c = contacts_ptr.add(i).as_ref()?;
                Some(Contact {
                    identifier: c_str_to_string(c.identifier_ptr, c.identifier_len),
                    given_name: if c.given_name_len > 0 {
                        Some(c_str_to_string(c.given_name_ptr, c.given_name_len))
                    } else {
                        None
                    },
                    family_name: if c.family_name_len > 0 {
                        Some(c_str_to_string(c.family_name_ptr, c.family_name_len))
                    } else {
                        None
                    },
                    phones: (0..c.phones_count)
                        .filter_map(|j| {
                            let p = c.phones_ptr.add(j).as_ref()?;
                            Some(ContactPhone {
                                number: c_str_to_string(p.number_ptr, p.number_len),
                                region_code: if p.region_len > 0 {
                                    Some(c_str_to_string(p.region_ptr, p.region_len))
                                } else {
                                    None
                                },
                                normalized: if p.normalized_len > 0 {
                                    Some(c_str_to_string(p.normalized_ptr, p.normalized_len))
                                } else {
                                    None
                                },
                            })
                        })
                        .collect(),
                    emails: (0..c.emails_count)
                        .filter_map(|j| {
                            let e = c.emails_ptr.add(j).as_ref()?;
                            Some(ContactEmail {
                                address: c_str_to_string(e.address_ptr, e.address_len),
                                is_valid: e.is_valid != 0,
                            })
                        })
                        .collect(),
                })
            })
            .collect();

        let new_waypoint = if state.waypoint_len > 0 {
            Some(unsafe { c_str_to_string(state.waypoint_ptr, state.waypoint_len) })
        } else {
            None
        };

        // Free C memory
        unsafe {
            oxide_contacts_free(contacts_ptr, count);
        }

        ContactsFetchResult::Success { contacts, waypoint: new_waypoint }
    }

    fn subscribe_to_changes<F>(&mut self, _callback: F) -> u32
    where
        F: Fn(ContactChange) + Send + 'static,
    {
        // Stub for now - would need NSNotificationCenter bridge
        let id = self.next_subscription_id;
        self.next_subscription_id += 1;
        id
    }

    fn unsubscribe(&mut self, _subscription_id: u32) {
        // Stub for now
    }

    fn carrier_region_code(&self) -> Option<String> {
        let mut ptr: *const u8 = std::ptr::null();
        let mut len: usize = 0;

        let result = unsafe { oxide_contacts_get_carrier_region(&mut ptr, &mut len) };

        if result == 0 && len > 0 {
            Some(unsafe { c_str_to_string(ptr, len) })
        } else {
            None
        }
    }
}

unsafe fn c_str_to_string(ptr: *const u8, len: usize) -> String {
    if ptr.is_null() || len == 0 {
        return String::new();
    }
    let slice = std::slice::from_raw_parts(ptr, len);
    String::from_utf8_lossy(slice).into_owned()
}

// ===== Media Library =====

use oxideui_platform_api::media_library::{
    FetchOptions, ImageData, ImageLoadResult, MediaAsset, MediaFetchResult,
    MediaLibraryManager, MediaType, ThumbnailSize,
};

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct OxideMediaAsset {
    identifier_ptr: *const u8,
    identifier_len: usize,
    media_type: u8, // 0=Image, 1=Video, 2=Audio
    creation_date: u64,
    duration_sec: f64,
    width: u32,
    height: u32,
    file_size: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct OxideImageData {
    data_ptr: *const u8,
    data_len: usize,
    width: u32,
    height: u32,
    row_bytes: usize,
}

extern "C" {
    fn oxide_media_fetch_assets(
        media_type_mask: u8,
        limit: i32,
        ascending: u8,
        out_assets: *mut *const OxideMediaAsset,
        out_count: *mut usize,
    ) -> i32;

    fn oxide_media_free_assets(assets: *const OxideMediaAsset, count: usize);

    fn oxide_media_load_thumbnail(
        identifier_ptr: *const u8,
        identifier_len: usize,
        size: u8, // 0=Small, 1=Medium, 2=Large
        out_image: *mut OxideImageData,
    ) -> i32;

    fn oxide_media_load_full_image(
        identifier_ptr: *const u8,
        identifier_len: usize,
        out_image: *mut OxideImageData,
    ) -> i32;

    fn oxide_media_free_image_data(data_ptr: *const u8, data_len: usize);
}

pub struct IosMediaLibraryManager {
    next_subscription_id: u32,
}

impl Default for IosMediaLibraryManager {
    fn default() -> Self {
        Self { next_subscription_id: 1 }
    }
}

impl MediaLibraryManager for IosMediaLibraryManager {
    fn fetch_assets(&mut self, options: FetchOptions) -> MediaFetchResult {
        let mut type_mask: u8 = 0;
        for mt in &options.media_types {
            type_mask |= match mt {
                MediaType::Image => 1,
                MediaType::Video => 2,
                MediaType::Audio => 4,
            };
        }

        let limit = options.limit.map_or(-1, |l| l as i32);
        let ascending = if options.ascending { 1 } else { 0 };

        let mut assets_ptr: *const OxideMediaAsset = std::ptr::null();
        let mut count: usize = 0;

        let result = unsafe {
            oxide_media_fetch_assets(type_mask, limit, ascending, &mut assets_ptr, &mut count)
        };

        if result == -1 {
            return MediaFetchResult::Denied;
        }

        if result < 0 {
            return MediaFetchResult::Error(format!("Failed to fetch media: {}", result));
        }

        let assets: Vec<MediaAsset> = (0..count)
            .filter_map(|i| unsafe {
                let a = assets_ptr.add(i).as_ref()?;
                Some(MediaAsset {
                    identifier: c_str_to_string(a.identifier_ptr, a.identifier_len),
                    media_type: match a.media_type {
                        0 => MediaType::Image,
                        1 => MediaType::Video,
                        _ => MediaType::Audio,
                    },
                    creation_date: if a.creation_date > 0 {
                        Some(a.creation_date)
                    } else {
                        None
                    },
                    duration_sec: if a.duration_sec > 0.0 {
                        Some(a.duration_sec)
                    } else {
                        None
                    },
                    width: a.width,
                    height: a.height,
                    file_size: a.file_size,
                })
            })
            .collect();

        unsafe {
            oxide_media_free_assets(assets_ptr, count);
        }

        MediaFetchResult::Success(assets)
    }

    fn load_thumbnail(&mut self, identifier: &str, size: ThumbnailSize) -> ImageLoadResult {
        let id_bytes = identifier.as_bytes();
        let size_code = match size {
            ThumbnailSize::Small => 0,
            ThumbnailSize::Medium => 1,
            ThumbnailSize::Large => 2,
        };

        let mut image_data = OxideImageData {
            data_ptr: std::ptr::null(),
            data_len: 0,
            width: 0,
            height: 0,
            row_bytes: 0,
        };

        let result = unsafe {
            oxide_media_load_thumbnail(
                id_bytes.as_ptr(),
                id_bytes.len(),
                size_code,
                &mut image_data,
            )
        };

        if result < 0 {
            return ImageLoadResult::Error(format!("Failed to load thumbnail: {}", result));
        }

        if image_data.data_ptr.is_null() {
            return ImageLoadResult::Error(String::from("No image data returned"));
        }

        let data = unsafe {
            std::slice::from_raw_parts(image_data.data_ptr, image_data.data_len).to_vec()
        };

        // Free C memory
        unsafe {
            oxide_media_free_image_data(image_data.data_ptr, image_data.data_len);
        }

        ImageLoadResult::Success(ImageData {
            width: image_data.width,
            height: image_data.height,
            data,
            row_bytes: image_data.row_bytes,
        })
    }

    fn load_full_image(&mut self, identifier: &str) -> ImageLoadResult {
        let id_bytes = identifier.as_bytes();

        let mut image_data = OxideImageData {
            data_ptr: std::ptr::null(),
            data_len: 0,
            width: 0,
            height: 0,
            row_bytes: 0,
        };

        let result = unsafe {
            oxide_media_load_full_image(id_bytes.as_ptr(), id_bytes.len(), &mut image_data)
        };

        if result < 0 {
            return ImageLoadResult::Error(format!("Failed to load image: {}", result));
        }

        if image_data.data_ptr.is_null() {
            return ImageLoadResult::Error(String::from("No image data returned"));
        }

        let data = unsafe {
            std::slice::from_raw_parts(image_data.data_ptr, image_data.data_len).to_vec()
        };

        // Free C memory
        unsafe {
            oxide_media_free_image_data(image_data.data_ptr, image_data.data_len);
        }

        ImageLoadResult::Success(ImageData {
            width: image_data.width,
            height: image_data.height,
            data,
            row_bytes: image_data.row_bytes,
        })
    }

    fn subscribe_to_changes<F>(&mut self, _callback: F) -> u32
    where
        F: Fn() + Send + 'static,
    {
        // Stub for now - would need PHPhotoLibraryChangeObserver bridge
        let id = self.next_subscription_id;
        self.next_subscription_id += 1;
        id
    }

    fn unsubscribe(&mut self, _subscription_id: u32) {
        // Stub for now
    }
}

// ===== URL Scheme Handling =====

use oxideui_platform_api::url_scheme::{UrlComponents, UrlOpenResult, UrlSchemeHandler};

extern "C" {
    fn oxide_url_can_open(url_ptr: *const u8, url_len: usize) -> i32;
    fn oxide_url_open(url_ptr: *const u8, url_len: usize) -> i32;
    fn oxide_url_register_handler(callback: extern "C" fn(*const u8, usize));
}

pub struct IosUrlSchemeHandler {
    // Callback storage would go here
}

impl Default for IosUrlSchemeHandler {
    fn default() -> Self {
        Self {}
    }
}

impl UrlSchemeHandler for IosUrlSchemeHandler {
    fn can_open(&self, url: &str) -> bool {
        let url_bytes = url.as_bytes();
        let result = unsafe { oxide_url_can_open(url_bytes.as_ptr(), url_bytes.len()) };
        result > 0
    }

    fn open(&mut self, url: &str) -> UrlOpenResult {
        let url_bytes = url.as_bytes();
        let result = unsafe { oxide_url_open(url_bytes.as_ptr(), url_bytes.len()) };

        match result {
            1 => UrlOpenResult::Opened,
            0 => UrlOpenResult::NotSupported,
            _ => UrlOpenResult::Error(format!("Failed to open URL: {}", result)),
        }
    }

    // TODO: Implement these methods when UrlSchemeSecurity is properly exported
    fn security(&self) -> &oxideui_platform_api::url_scheme::UrlSchemeSecurity {
        unimplemented!("URL scheme security not yet implemented for iOS")
    }

    fn set_security(&mut self, _security: oxideui_platform_api::url_scheme::UrlSchemeSecurity) {
        // TODO: Store security settings
    }

    fn open_unchecked(&mut self, url: &str) -> UrlOpenResult {
        // For now, just call regular open
        self.open(url)
    }

    fn register_handler<F>(&mut self, _callback: F)
    where
        F: Fn(UrlComponents) + Send + 'static,
    {
        // Stub for now - would need to store callback and bridge from C
        // In production: Store callback in Arc<Mutex<Option<Box<dyn Fn...>>>>
        // Then call from extern "C" fn trampoline
    }
}

// ===== Tokio runtime integration =====
#[cfg(feature = "tokio-runtime")]
pub fn init_tokio_spawn() {
    use once_cell::sync::OnceCell;
    static RT: OnceCell<tokio::runtime::Runtime> = OnceCell::new();
    runtime::set_spawn(|fut| {
        let rt = RT.get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(std::cmp::min(4, num_cpus::get()))
                .enable_all()
                .build()
                .expect("tokio runtime")
        });
        drop(rt.spawn(fut));
    });
}
