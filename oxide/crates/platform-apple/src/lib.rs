//! Shared Apple platform adapters.

#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc, clippy::module_name_repetitions)]

extern crate alloc;

use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::cmp::Reverse;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::ffi::CString;
use std::io::{ErrorKind as IoErrorKind, Read, Write};
use std::net::{Shutdown, TcpStream, UdpSocket};
#[cfg(any(target_os = "macos", target_os = "ios"))]
use std::os::fd::AsRawFd;
use std::sync::{Arc, Mutex, Once};
use std::time::Duration;

use once_cell::sync::Lazy;
use oxide_networking::{NetworkPath, NetworkPathKind, ReachabilityState};
use oxide_platform_api::media_library::{
    AssetData, AssetId, AssetType, ImageFormat, ImageQuality, MediaAsset, MediaLibrary,
};
use oxide_platform_api::{
    secure_storage, AdvertisementData, AudioSample, AudioSessionMode, BleCacheEntry, BleUuid,
    Bluetooth, BluetoothEvent, CameraConfig, CameraDevice, CameraFrame, CameraImage, CameraManager,
    CameraRecording, CameraStream, CaptureMode, ColorSpace, Connection, ConnectionEvent,
    ConnectionGroup, ConnectionOptions, FlashMode, GattChar, GeoHash, GeoRegion, GeoRegionTracker,
    HttpClient, HttpEvent, HttpHeader, HttpMethod, HttpOperation, HttpRequest, HttpResponse,
    LocationAccuracy, LocationEvent,
    LocationOptions, LocationReading, LocationService, MotionSample, MotionService, NetworkError,
    NetworkErrorDomain, Networking, PeripheralId, PeripheralInfo, PhotoEvent, PhotoOptions,
    PlatformError, ProtocolOptions, PushManager, PushNotification, PushPresentation, PushProvider,
    PushToken, RecordingContainer, RecordingDestination, RecordingEvent, RecordingOptions,
    RecordingResult, RestorationInfo, ScanOptions, SecureStorage, TcpOptions, TorchMode, UdpEvent,
    UdpPacket, UdpSocket as OxideUdpSocket,
};

pub const APPLE_PATH_KIND_WIFI: u32 = 0;
pub const APPLE_PATH_KIND_CELLULAR: u32 = 1;
pub const APPLE_PATH_KIND_WIRED: u32 = 2;
pub const APPLE_PATH_KIND_OTHER: u32 = 3;
pub const APPLE_INTERFACE_WIFI: u32 = 1 << 0;
pub const APPLE_INTERFACE_CELLULAR: u32 = 1 << 1;
pub const APPLE_INTERFACE_WIRED: u32 = 1 << 2;
pub const APPLE_PERMISSION_NOT_DETERMINED: u32 = 0;
pub const APPLE_PERMISSION_DENIED: u32 = 1;
pub const APPLE_PERMISSION_LIMITED: u32 = 2;
pub const APPLE_PERMISSION_AUTHORIZED: u32 = 3;
pub const APPLE_PERMISSION_NOTIFICATIONS: u32 = 0;
pub const APPLE_PERMISSION_LOCATION: u32 = 1;
pub const APPLE_PERMISSION_CAMERA: u32 = 2;
pub const APPLE_PERMISSION_CONTACTS: u32 = 3;
pub const APPLE_PERMISSION_BLUETOOTH: u32 = 4;
pub const APPLE_PERMISSION_MOTION: u32 = 5;
pub const APPLE_PERMISSION_MICROPHONE: u32 = 6;
pub const APPLE_PERMISSION_MEDIA_LIBRARY: u32 = 7;

extern "C" {
    fn oxide_host_http_start(
        url_ptr: *const u8,
        url_len: usize,
        timeout_ms: u32,
        max_response_bytes: usize,
        request_headers: *const AppleHttpHeader,
        request_header_count: usize,
        response_headers: *const AppleHttpHeader,
        response_header_count: usize,
        callback: Option<unsafe extern "C" fn(*mut core::ffi::c_void, *const AppleHttpEvent)>,
        context: *mut core::ffi::c_void,
        out_request_id: *mut u64,
    ) -> i32;

    fn oxide_host_http_cancel(request_id: u64);

    fn oxide_ble_init();

    fn oxide_ble_init_with_restoration(restore_id: *const core::ffi::c_char);

    fn oxide_ble_powered_on() -> u8;

    fn oxide_ble_start_scan(cfg: *const AppleBleScanConfig);

    fn oxide_ble_stop_scan();

    fn oxide_ble_connect(id16: *const u8);

    fn oxide_ble_disconnect(id16: *const u8);

    fn oxide_ble_read(
        id16: *const u8,
        svc16: *const u8,
        chr16: *const u8,
        out_ptr: *mut *mut u8,
        out_len: *mut usize,
        timeout_ms: u32,
    ) -> i32;

    fn oxide_ble_write(
        id16: *const u8,
        svc16: *const u8,
        chr16: *const u8,
        data: *const u8,
        len: usize,
        with_response: u8,
        timeout_ms: u32,
    ) -> i32;

    fn oxide_ble_notify(
        id16: *const u8,
        svc16: *const u8,
        chr16: *const u8,
        enable: u8,
        timeout_ms: u32,
    ) -> i32;

    fn oxide_ble_advertise_start(name: *const core::ffi::c_char, service_uuid: *const u8);

    fn oxide_ble_advertise_stop();

    fn oxide_secure_storage_save(
        key_ptr: *const u8,
        key_len: usize,
        data_ptr: *const u8,
        data_len: usize,
    ) -> i32;

    fn oxide_secure_storage_load(
        key_ptr: *const u8,
        key_len: usize,
        out_data_ptr: *mut *const u8,
        out_data_len: *mut usize,
    ) -> i32;

    fn oxide_secure_storage_delete(key_ptr: *const u8, key_len: usize) -> i32;

    fn oxide_secure_storage_free_data(data_ptr: *const u8, data_len: usize);

    fn oxide_host_set_location_callback(
        cb: Option<unsafe extern "C" fn(*const AppleLocationSample)>,
    );

    fn oxide_host_set_location_error_callback(cb: Option<unsafe extern "C" fn(*const u8, usize)>);

    fn oxide_host_location_start(cfg: AppleLocationConfig) -> i32;

    fn oxide_host_location_stop();

    fn oxide_host_location_request_once();

    fn oxide_host_location_last(out: *mut AppleLocationSample) -> u8;

    fn oxide_host_location_set_accuracy(accuracy_kind: u32) -> i32;

    fn oxide_host_set_motion_callback(cb: Option<unsafe extern "C" fn(*const AppleMotionSample)>);

    fn oxide_host_motion_start() -> i32;

    fn oxide_host_motion_stop();

    fn oxide_host_motion_is_active() -> u8;

    fn oxide_host_push_register();

    fn oxide_host_push_get_device_token(out_ptr: *mut *mut u8, out_len: *mut usize) -> i32;

    fn oxide_host_push_set_badge(count: i32);

    fn oxide_host_push_clear_badge();

    fn oxide_host_push_clear_all_delivered();

    fn oxide_host_set_push_token_callback(cb: Option<unsafe extern "C" fn(u32, *const u8, usize)>);

    fn oxide_host_set_push_notify_callback(cb: Option<unsafe extern "C" fn(*const u8, usize)>);

    fn oxide_host_string_free(p: *mut u8);

    fn oxide_media_fetch_assets(
        media_type_mask: u8,
        limit: i32,
        ascending: u8,
        out_assets: *mut *const AppleMediaAsset,
        out_count: *mut usize,
    ) -> i32;

    fn oxide_media_free_assets(assets: *const AppleMediaAsset, count: usize);

    fn oxide_media_load_thumbnail(
        identifier_ptr: *const u8,
        identifier_len: usize,
        size: u8,
        out_image: *mut AppleMediaImageData,
    ) -> i32;

    fn oxide_media_load_thumbnail_rgba(
        identifier_ptr: *const u8,
        identifier_len: usize,
        size: u8,
        out_image: *mut AppleMediaImageData,
    ) -> i32;

    fn oxide_media_load_full_image(
        identifier_ptr: *const u8,
        identifier_len: usize,
        out_image: *mut AppleMediaImageData,
    ) -> i32;

    fn oxide_media_load_full_image_rgba(
        identifier_ptr: *const u8,
        identifier_len: usize,
        out_image: *mut AppleMediaImageData,
    ) -> i32;

    fn oxide_media_free_image_data(data_ptr: *const u8, data_len: usize);

    fn oxide_media_load_video_file(
        identifier_ptr: *const u8,
        identifier_len: usize,
        out_path_ptr: *mut *const u8,
        out_path_len: *mut usize,
    ) -> i32;

    fn oxide_media_free_string(data_ptr: *const u8, data_len: usize);

    #[cfg(any(test, all(feature = "web-view-macos", target_os = "macos")))]
    fn oxide_web_view_set_event_callback(
        cb: Option<unsafe extern "C" fn(u64, u32, *const u8, usize)>,
    );

    #[cfg(any(test, all(feature = "web-view-macos", target_os = "macos")))]
    fn oxide_web_view_create(url_ptr: *const u8, url_len: usize, id: u64) -> i32;

    #[cfg(any(test, all(feature = "web-view-macos", target_os = "macos")))]
    fn oxide_web_view_execute_script(
        id: u64,
        script_ptr: *const u8,
        script_len: usize,
        out_ptr: *mut *mut u8,
        out_len: *mut usize,
    ) -> i32;

    #[cfg(any(test, all(feature = "web-view-macos", target_os = "macos")))]
    fn oxide_web_view_close(id: u64);

    #[cfg(any(test, all(feature = "web-view-macos", target_os = "macos")))]
    fn oxide_web_view_free_string(data_ptr: *mut u8);
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AppleHttpHeader {
   pub name_ptr: *const u8,
   pub name_len: usize,
   pub value_ptr: *const u8,
   pub value_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AppleHttpEvent {
   pub kind: u32,
   pub error: i32,
   pub status: u16,
   pub reserved: u16,
   pub content_length: i64,
   pub data_ptr: *const u8,
   pub data_len: usize,
   pub final_url_ptr: *const u8,
   pub final_url_len: usize,
   pub headers_ptr: *const AppleHttpHeader,
   pub header_count: usize,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct AppleHttpClient;

impl AppleHttpClient {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

struct AppleHttpCallbackState {
   terminal: AtomicBool,
   callback: alloc::boxed::Box<dyn Fn(HttpEvent) + Send + Sync>,
}

struct AppleHttpOperation {
   request_id: u64,
   cancelled: AtomicBool,
}

impl HttpOperation for AppleHttpOperation {
   fn cancel(&self) {
      if !self.cancelled.swap(true, Ordering::AcqRel) {
         unsafe { oxide_host_http_cancel(self.request_id) };
      }
   }
}

impl Drop for AppleHttpOperation {
   fn drop(&mut self) {
      self.cancel();
   }
}

impl HttpClient for AppleHttpClient {
   fn start(&self, request: HttpRequest, on_event: alloc::boxed::Box<dyn Fn(HttpEvent) + Send + Sync>) -> Result<alloc::boxed::Box<dyn HttpOperation + Send + Sync>, PlatformError> {
      validate_http_request(&request)?;
      let remaining = request.timeout;
      if remaining.is_zero() {
         return Err(PlatformError::Io(alloc::string::String::from("HTTP deadline exceeded")));
      }
      let timeout_ms = remaining.as_millis().clamp(1, u128::from(u32::MAX)) as u32;
      let request_headers = request.headers.iter().map(raw_http_header).collect::<alloc::vec::Vec<_>>();
      let response_headers = request.response_headers.iter().map(|name| AppleHttpHeader {
         name_ptr: name.as_ptr(),
         name_len: name.len(),
         value_ptr: core::ptr::null(),
         value_len: 0,
      }).collect::<alloc::vec::Vec<_>>();
      let callback = Arc::new(AppleHttpCallbackState {
         terminal: AtomicBool::new(false),
         callback: on_event,
      });
      let context = Arc::into_raw(callback).cast_mut().cast::<core::ffi::c_void>();
      let mut request_id = 0;
      let result = unsafe {
         oxide_host_http_start(
            request.url.as_ptr(),
            request.url.len(),
            timeout_ms,
            request.max_response_bytes,
            request_headers.as_ptr(),
            request_headers.len(),
            response_headers.as_ptr(),
            response_headers.len(),
            Some(apple_http_event),
            context,
            &mut request_id,
         )
      };
      if result != 0 || request_id == 0 {
         unsafe { drop(Arc::from_raw(context.cast::<AppleHttpCallbackState>())) };
         return Err(http_error(if result == 0 { -1 } else { result }));
      }
      Ok(alloc::boxed::Box::new(AppleHttpOperation {
         request_id,
         cancelled: AtomicBool::new(false),
      }))
   }
}

fn raw_http_header(header: &HttpHeader) -> AppleHttpHeader {
   AppleHttpHeader {
      name_ptr: header.name.as_ptr(),
      name_len: header.name.len(),
      value_ptr: header.value.as_ptr(),
      value_len: header.value.len(),
   }
}

fn valid_http_header_name(name: &str) -> bool {
   !name.is_empty() && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
}

const MAXIMUM_HTTP_HEADER_COUNT: usize = 64;
const MAXIMUM_HTTP_METADATA_BYTES: usize = 32 * 1024;
const MAXIMUM_HTTP_URL_BYTES: usize = 16 * 1024;

fn validate_http_request(request: &HttpRequest) -> Result<(), PlatformError> {
   if request.method != HttpMethod::Get {
      return Err(PlatformError::Unsupported("Apple HTTP bridge only supports GET"));
   }
   if !request.body.is_empty() || request.credentials != oxide_platform_api::HttpCredentials::Omit {
      return Err(PlatformError::Unsupported("Apple HTTP bridge does not accept request bodies or ambient credentials"));
   }
   if request.url.trim().is_empty() || request.max_response_bytes == 0 {
      return Err(PlatformError::Invalid("HTTP URL or response limit is invalid"));
   }
   if request.url.len() > MAXIMUM_HTTP_URL_BYTES {
      return Err(PlatformError::Invalid("HTTP URL exceeds limit"));
   }
   let Some(header_count) = request.headers.len().checked_add(request.response_headers.len()) else {
      return Err(PlatformError::Invalid("HTTP header count exceeds limit"));
   };
   if header_count > MAXIMUM_HTTP_HEADER_COUNT {
      return Err(PlatformError::Invalid("HTTP header count exceeds limit"));
   }
   let mut metadata_bytes = 0_usize;
   for header in &request.headers {
      let Some(next_metadata_bytes) = metadata_bytes.checked_add(header.name.len()).and_then(|total| total.checked_add(header.value.len())) else {
         return Err(PlatformError::Invalid("HTTP metadata bytes exceed limit"));
      };
      metadata_bytes = next_metadata_bytes;
      if !valid_http_header_name(header.name.as_str())
         || header.value.bytes().any(|byte| byte == b'\r' || byte == b'\n')
         || header.name.eq_ignore_ascii_case("cookie")
         || header.name.eq_ignore_ascii_case("authorization")
         || header.name.eq_ignore_ascii_case("proxy-authorization")
      {
         return Err(PlatformError::Invalid("HTTP request header is invalid"));
      }
   }
   for name in &request.response_headers {
      let Some(next_metadata_bytes) = metadata_bytes.checked_add(name.len()) else {
         return Err(PlatformError::Invalid("HTTP metadata bytes exceed limit"));
      };
      metadata_bytes = next_metadata_bytes;
      if !valid_http_header_name(name.as_str()) {
         return Err(PlatformError::Invalid("selected HTTP response header is invalid"));
      }
   }
   if metadata_bytes > MAXIMUM_HTTP_METADATA_BYTES {
      return Err(PlatformError::Invalid("HTTP metadata bytes exceed limit"));
   }
   Ok(())
}

unsafe extern "C" fn apple_http_event(context: *mut core::ffi::c_void, raw: *const AppleHttpEvent) {
   if context.is_null() || raw.is_null() {
      return;
   }
   let state = unsafe { &*context.cast::<AppleHttpCallbackState>() };
   if state.terminal.load(Ordering::Acquire) {
      return;
   }
   let event = unsafe { decode_apple_http_event(&*raw) };
   let terminal = event.terminal();
   if terminal && state.terminal.swap(true, Ordering::AcqRel) {
      return;
   }
   if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (state.callback)(event))).is_err() {
      std::process::abort();
   }
   if terminal {
      unsafe { drop(Arc::from_raw(context.cast::<AppleHttpCallbackState>())) };
   }
}

unsafe fn decode_apple_http_event(raw: &AppleHttpEvent) -> HttpEvent {
   if !unsafe { valid_apple_http_event(raw) } {
      return HttpEvent::Failed(PlatformError::Invalid("native HTTP event bounds are invalid"));
   }
   match raw.kind {
      1 => {
         let headers = if raw.headers_ptr.is_null() || raw.header_count == 0 {
            alloc::vec::Vec::new()
         } else {
            unsafe { core::slice::from_raw_parts(raw.headers_ptr, raw.header_count) }.iter().filter_map(|header| {
               Some(HttpHeader {
                  name: copy_string(header.name_ptr, header.name_len)?,
                  value: copy_string(header.value_ptr, header.value_len).unwrap_or_default(),
               })
            }).collect()
         };
         HttpEvent::Response(HttpResponse {
            final_url: copy_string(raw.final_url_ptr, raw.final_url_len).unwrap_or_default(),
            status: raw.status,
            content_length: u64::try_from(raw.content_length).ok(),
            headers,
         })
      }
      2 => HttpEvent::Body(copy_bytes(raw.data_ptr, raw.data_len)),
      3 => HttpEvent::Complete,
      4 => HttpEvent::Cancelled,
      5 => HttpEvent::Failed(http_error(raw.error)),
      _ => HttpEvent::Failed(PlatformError::Unknown(alloc::string::String::from("native HTTP event was invalid"))),
   }
}

unsafe fn valid_apple_http_event(raw: &AppleHttpEvent) -> bool {
   match raw.kind {
      1 => {
         if raw.data_ptr.is_null() != (raw.data_len == 0)
            || raw.data_len != 0
            || raw.final_url_len == 0
            || raw.final_url_len > MAXIMUM_HTTP_URL_BYTES
            || raw.final_url_ptr.is_null()
            || raw.header_count > MAXIMUM_HTTP_HEADER_COUNT
            || raw.headers_ptr.is_null() != (raw.header_count == 0)
            || (!raw.headers_ptr.is_null()
               && (raw.headers_ptr as usize) % core::mem::align_of::<AppleHttpHeader>() != 0)
         {
            return false;
         }
         if raw.header_count == 0 {
            return true;
         }
         let headers = unsafe { core::slice::from_raw_parts(raw.headers_ptr, raw.header_count) };
         let mut metadata_bytes = 0_usize;
         for header in headers {
            if header.name_len == 0
               || header.name_ptr.is_null()
               || (header.value_len != 0 && header.value_ptr.is_null())
            {
               return false;
            }
            let Some(next_metadata_bytes) = metadata_bytes.checked_add(header.name_len).and_then(|total| total.checked_add(header.value_len)) else {
               return false;
            };
            metadata_bytes = next_metadata_bytes;
         }
         metadata_bytes <= MAXIMUM_HTTP_METADATA_BYTES
      }
      2 => raw.data_ptr.is_null() == (raw.data_len == 0)
         && raw.final_url_ptr.is_null()
         && raw.final_url_len == 0
         && raw.headers_ptr.is_null()
         && raw.header_count == 0,
      3..=5 => raw.data_ptr.is_null()
         && raw.data_len == 0
         && raw.final_url_ptr.is_null()
         && raw.final_url_len == 0
         && raw.headers_ptr.is_null()
         && raw.header_count == 0,
      _ => false,
   }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AppleBleScanConfig {
    pub services_ptr: *const u8,
    pub service_count: usize,
    pub allow_duplicates: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AppleBleScanInfo {
    pub id: [u8; 16],
    pub name_ptr: *const u8,
    pub name_len: usize,
    pub rssi_dbm: i16,
    pub services_ptr: *const u8,
    pub service_count: usize,
    pub manufacturer_ptr: *const u8,
    pub manufacturer_len: usize,
    pub connectable: u8,
}

type BleCallback = alloc::boxed::Box<dyn Fn(BluetoothEvent) + Send + 'static>;
type RecordingCallback = alloc::boxed::Box<dyn Fn(RecordingEvent) + Send>;
type PhotoCallback = alloc::boxed::Box<dyn Fn(PhotoEvent) + Send>;

static APPLE_BLE_SUBS: Lazy<Mutex<alloc::vec::Vec<BleCallback>>> =
    Lazy::new(|| Mutex::new(alloc::vec::Vec::new()));
static APPLE_BLE_CACHE: Lazy<Mutex<HashMap<PeripheralId, BleCacheEntry>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static APPLE_BLE_INIT: Once = Once::new();
const BLE_CACHE_MAX: usize = 128;

#[derive(Debug, Default, Clone, Copy)]
pub struct AppleBluetooth;

impl AppleBluetooth {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[no_mangle]
pub extern "C" fn oxide_host_ble_emit_state(powered_on: u32) {
    let ev = BluetoothEvent::StateChanged { powered_on: powered_on != 0 };
    let subs = APPLE_BLE_SUBS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    for callback in subs.iter() {
        callback(ev.clone());
    }
}

#[no_mangle]
pub unsafe extern "C" fn oxide_host_ble_emit_discovered(info: *const AppleBleScanInfo) {
    if info.is_null() {
        return;
    }
    let raw = unsafe { &*info };
    let info = peripheral_info_from_raw(raw);
    let cache_entry = store_ble_cache(&info);
    let discovered = BluetoothEvent::Discovered(info);
    let cache_evt = BluetoothEvent::CacheUpdated(cache_entry);
    let subs = APPLE_BLE_SUBS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    for callback in subs.iter() {
        callback(discovered.clone());
        callback(cache_evt.clone());
    }
}

#[no_mangle]
pub unsafe extern "C" fn oxide_host_ble_emit_restored(
    infos: *const AppleBleScanInfo,
    count: usize,
) {
    if infos.is_null() || count == 0 {
        return;
    }
    let raw_infos = unsafe { core::slice::from_raw_parts(infos, count) };
    let peripherals = raw_infos.iter().map(peripheral_info_from_raw).collect();
    let ev = BluetoothEvent::Restored(RestorationInfo { peripherals });
    let subs = APPLE_BLE_SUBS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    for callback in subs.iter() {
        callback(ev.clone());
    }
}

#[no_mangle]
pub unsafe extern "C" fn oxide_host_ble_emit_connected(id: *const u8) {
    if id.is_null() {
        return;
    }
    let pid = id_from_ptr(id);
    let connected = BluetoothEvent::Connected(pid);
    let cache_evt = touch_ble_cache(pid).map(BluetoothEvent::CacheUpdated);
    let subs = APPLE_BLE_SUBS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    for callback in subs.iter() {
        callback(connected.clone());
        if let Some(ref ev) = cache_evt {
            callback(ev.clone());
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn oxide_host_ble_emit_disconnected(id: *const u8) {
    if id.is_null() {
        return;
    }
    let pid = id_from_ptr(id);
    let disconnected = BluetoothEvent::Disconnected(pid);
    let cache_evt = touch_ble_cache(pid).map(BluetoothEvent::CacheUpdated);
    let subs = APPLE_BLE_SUBS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    for callback in subs.iter() {
        callback(disconnected.clone());
        if let Some(ref ev) = cache_evt {
            callback(ev.clone());
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn oxide_host_ble_emit_notified(
    id: *const u8,
    svc: *const u8,
    chr: *const u8,
    data: *const u8,
    len: usize,
) {
    if id.is_null() || svc.is_null() || chr.is_null() || (data.is_null() && len > 0) {
        return;
    }
    let pid = id_from_ptr(id);
    let service = BleUuid(copy16(svc));
    let characteristic = BleUuid(copy16(chr));
    let bytes = if len == 0 {
        alloc::vec::Vec::new()
    } else {
        unsafe { core::slice::from_raw_parts(data, len) }.to_vec()
    };
    let notify = BluetoothEvent::Notified {
        id: pid,
        chr: GattChar { service, characteristic },
        data: bytes,
    };
    let cache_evt = touch_ble_cache(pid).map(BluetoothEvent::CacheUpdated);
    let subs = APPLE_BLE_SUBS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    for callback in subs.iter() {
        callback(notify.clone());
        if let Some(ref ev) = cache_evt {
            callback(ev.clone());
        }
    }
}

pub fn apple_bluetooth_with_restoration(restore_id: &str) -> AppleBluetooth {
    if let Ok(c_id) = CString::new(restore_id) {
        APPLE_BLE_INIT.call_once(|| unsafe {
            oxide_ble_init_with_restoration(c_id.as_ptr());
        });
    } else {
        ensure_ble_initialized();
    }
    AppleBluetooth
}

impl Bluetooth for AppleBluetooth {
    fn powered_on(&self) -> bool {
        ensure_ble_initialized();
        unsafe { oxide_ble_powered_on() != 0 }
    }

    fn subscribe_events(&self, f: alloc::boxed::Box<dyn Fn(BluetoothEvent) + Send>) {
        ensure_ble_initialized();
        APPLE_BLE_SUBS.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(f);
    }

    fn start_scan(&self, opts: &ScanOptions) {
        ensure_ble_initialized();
        let mut bytes = alloc::vec::Vec::with_capacity(opts.services.len() * 16);
        for service in &opts.services {
            bytes.extend_from_slice(&service.0);
        }
        let cfg = AppleBleScanConfig {
            services_ptr: if bytes.is_empty() { core::ptr::null() } else { bytes.as_ptr() },
            service_count: opts.services.len(),
            allow_duplicates: if opts.allow_duplicates { 1 } else { 0 },
        };
        unsafe {
            oxide_ble_start_scan(&cfg);
        }
    }

    fn stop_scan(&self) {
        unsafe {
            oxide_ble_stop_scan();
        }
    }

    fn connect(&self, id: PeripheralId) {
        let bytes = id.to_le_bytes();
        unsafe {
            oxide_ble_connect(bytes.as_ptr());
        }
    }

    fn disconnect(&self, id: PeripheralId) {
        let bytes = id.to_le_bytes();
        unsafe {
            oxide_ble_disconnect(bytes.as_ptr());
        }
    }

    fn read(&self, id: PeripheralId, chr: GattChar) -> Result<alloc::vec::Vec<u8>, PlatformError> {
        let bytes = id.to_le_bytes();
        let mut out: *mut u8 = core::ptr::null_mut();
        let mut len: usize = 0;
        let ok = unsafe {
            oxide_ble_read(
                bytes.as_ptr(),
                chr.service.0.as_ptr(),
                chr.characteristic.0.as_ptr(),
                &mut out,
                &mut len,
                5000,
            )
        };
        if ok == 0 {
            return Err(PlatformError::Busy);
        }
        let data = if out.is_null() || len == 0 {
            alloc::vec::Vec::new()
        } else {
            unsafe { core::slice::from_raw_parts(out, len) }.to_vec()
        };
        unsafe {
            oxide_host_string_free(out);
        }
        Ok(data)
    }

    fn write(
        &self,
        id: PeripheralId,
        chr: GattChar,
        data: &[u8],
        with_response: bool,
    ) -> Result<(), PlatformError> {
        let bytes = id.to_le_bytes();
        let ok = unsafe {
            oxide_ble_write(
                bytes.as_ptr(),
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
        let bytes = id.to_le_bytes();
        let ok = unsafe {
            oxide_ble_notify(
                bytes.as_ptr(),
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

    fn advertise_start(&self, name: &str, services: &[BleUuid]) {
        let c_name = CString::new(name).unwrap_or_default();
        let uuid_ptr = services.first().map_or(core::ptr::null(), |uuid| uuid.0.as_ptr());
        unsafe {
            oxide_ble_advertise_start(c_name.as_ptr(), uuid_ptr);
        }
    }

    fn advertise_stop(&self) {
        unsafe {
            oxide_ble_advertise_stop();
        }
    }

    fn cached_peripherals(&self) -> alloc::vec::Vec<BleCacheEntry> {
        current_ble_cache()
    }
}

fn ensure_ble_initialized() {
    APPLE_BLE_INIT.call_once(|| unsafe {
        oxide_ble_init();
    });
}

fn peripheral_info_from_raw(raw: &AppleBleScanInfo) -> PeripheralInfo {
    let id = PeripheralId::from_le_bytes(raw.id);
    let name = if raw.name_len > 0 && !raw.name_ptr.is_null() {
        let bytes = unsafe { core::slice::from_raw_parts(raw.name_ptr, raw.name_len) };
        Some(alloc::string::String::from_utf8_lossy(bytes).into_owned())
    } else {
        None
    };
    let mut services = alloc::vec::Vec::new();
    if raw.service_count > 0 && !raw.services_ptr.is_null() {
        let slice =
            unsafe { core::slice::from_raw_parts(raw.services_ptr, raw.service_count * 16) };
        for chunk in slice.chunks_exact(16) {
            let mut uuid = [0_u8; 16];
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
    let advertisement =
        AdvertisementData { services, manufacturer_data, connectable: raw.connectable != 0 };
    PeripheralInfo { id, name, rssi_dbm: raw.rssi_dbm, advertisement }
}

fn id_from_ptr(ptr: *const u8) -> PeripheralId {
    u128::from_le_bytes(copy16(ptr))
}

fn copy16(ptr: *const u8) -> [u8; 16] {
    if ptr.is_null() {
        return [0; 16];
    }
    unsafe { *(ptr as *const [u8; 16]) }
}

fn store_ble_cache(info: &PeripheralInfo) -> BleCacheEntry {
    let cached = BleCacheEntry { peripheral: info.clone(), last_seen_ms: now_ms() };
    let mut cache = APPLE_BLE_CACHE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    cache.insert(cached.peripheral.id, cached.clone());
    if cache.len() > BLE_CACHE_MAX {
        if let Some(oldest) = cache.iter().min_by_key(|(_, e)| e.last_seen_ms).map(|(k, _)| *k) {
            cache.remove(&oldest);
        }
    }
    cached
}

fn touch_ble_cache(id: PeripheralId) -> Option<BleCacheEntry> {
    let mut cache = APPLE_BLE_CACHE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(entry) = cache.get_mut(&id) {
        entry.last_seen_ms = now_ms();
        Some(entry.clone())
    } else {
        None
    }
}

fn current_ble_cache() -> alloc::vec::Vec<BleCacheEntry> {
    let mut entries: alloc::vec::Vec<_> = APPLE_BLE_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .values()
        .cloned()
        .collect();
    entries.sort_by_key(|entry| Reverse(entry.last_seen_ms));
    entries
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |duration| duration.as_millis() as u64)
}

// ===== Camera manager =====

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AppleCamFrame {
    pub y_ptr: *const u8,
    pub y_len: usize,
    pub y_stride: usize,
    pub uv_ptr: *const u8,
    pub uv_len: usize,
    pub uv_stride: usize,
    pub width: i32,
    pub height: i32,
    pub timestamp_ns: u64,
    pub rotation_deg: u16,
    pub bit_depth: u8,
    pub matrix: u8,
    pub video_range: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AppleCamAudio {
    pub audio_ptr: *const i16,
    pub sample_count: usize,
    pub channels: u32,
    pub sample_rate_hz: u32,
    pub timestamp_ns: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AppleCamRecordEvent {
    pub kind: u32,
    pub path_ptr: *const u8,
    pub path_len: usize,
    pub duration_ns: u64,
    pub size_bytes: u64,
    pub had_audio: u8,
    pub error_code: i32,
    pub error_msg_ptr: *const u8,
    pub error_msg_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AppleCamPhotoEvent {
    pub kind: u32,
    pub frame: AppleCamFrame,
    pub error_code: i32,
    pub error_msg_ptr: *const u8,
    pub error_msg_len: usize,
}

extern "C" {
    fn oxide_cam_start_default() -> i32;
    fn oxide_cam_start_default_preview_only() -> i32;
    fn oxide_cam_stop();
    fn oxide_cam_set_fps(fps: i32) -> i32;
    fn oxide_cam_set_resolution_height(h: i32) -> i32;
    fn oxide_cam_set_bit_depth(bits: i32) -> i32;
    fn oxide_cam_set_color_space(id: i32) -> i32;
    fn oxide_cam_set_position(pos: i32) -> i32;
    fn oxide_cam_set_mode(mode: i32) -> i32;
    fn oxide_cam_set_focus_point(x: f32, y: f32) -> i32;
    fn oxide_cam_set_zoom_factor(factor: f32) -> i32;
    fn oxide_cam_set_flash_mode(mode: i32) -> i32;
    fn oxide_cam_set_torch_mode(mode: i32, level: f32) -> i32;
    fn oxide_cam_capture_photo(high_speed_from_preview: u8, flash_mode: i32) -> i32;
    fn oxide_cam_set_audio_session_mode(mode: i32) -> i32;
    fn oxide_host_set_camera_callback(cb: Option<unsafe extern "C" fn(*const AppleCamFrame)>);
    fn oxide_host_set_camera_audio_callback(cb: Option<unsafe extern "C" fn(*const AppleCamAudio)>);
    fn oxide_host_set_camera_record_callback(
        cb: Option<unsafe extern "C" fn(*const AppleCamRecordEvent)>,
    );
    fn oxide_host_set_camera_photo_callback(
        cb: Option<unsafe extern "C" fn(*const AppleCamPhotoEvent)>,
    );
    fn oxide_cam_record_start(
        dest_ptr: *const u8,
        dest_len: usize,
        container: i32,
        include_audio: u8,
    ) -> i32;
    fn oxide_cam_record_stop() -> i32;
    fn oxide_cam_record_cancel() -> i32;
    fn oxide_host_set_camera_running(on: u8) -> i32;
}

pub struct AppleCameraManager;

static APPLE_CAMERA_MANAGER: AppleCameraManager = AppleCameraManager;

pub fn camera_manager() -> &'static AppleCameraManager {
    &APPLE_CAMERA_MANAGER
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
    preferred_color_space: Option<ColorSpace>,
}

impl Default for CameraSettings {
    fn default() -> Self {
        Self {
            device: CameraDevice::Back,
            fps: 30,
            width: 1920,
            height: 1080,
            mode: CaptureMode::Preview,
            preferred_color_space: Some(ColorSpace::DisplayP3Linear),
        }
    }
}

struct CamState {
    subs: Mutex<alloc::vec::Vec<CameraSubscriber>>,
    next_id: AtomicU64,
    settings: Mutex<CameraSettings>,
    callback_once: std::sync::Once,
    record_once: std::sync::Once,
    photo_once: std::sync::Once,
    record_cb: Mutex<Option<RecordingCallback>>,
    photo_cb: Mutex<Option<PhotoCallback>>,
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
            photo_once: std::sync::Once::new(),
            record_cb: Mutex::new(None),
            photo_cb: Mutex::new(None),
            recording: AtomicBool::new(false),
        }
    }

    fn has_audio_subscribers(&self) -> bool {
        self.subs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .any(|s| s.audio_cb.is_some())
    }

    fn ensure_callback(&self) {
        self.callback_once.call_once(|| unsafe {
            oxide_host_set_camera_callback(Some(oxide_cam_frame_trampoline));
            oxide_host_set_camera_audio_callback(Some(oxide_cam_audio_trampoline));
        });
    }

    fn ensure_record_callback(&self) {
        self.record_once.call_once(|| unsafe {
            oxide_host_set_camera_record_callback(Some(oxide_cam_record_trampoline));
        });
    }

    fn ensure_photo_callback(&self) {
        self.photo_once.call_once(|| unsafe {
            oxide_host_set_camera_photo_callback(Some(oxide_cam_photo_trampoline));
        });
    }

    fn try_begin_recording(&self, cb: RecordingCallback) -> Result<(), PlatformError> {
        if self.recording.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err()
        {
            return Err(PlatformError::Busy);
        }
        let mut slot = self.record_cb.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        *slot = Some(cb);
        Ok(())
    }

    fn finish_recording(&self) -> Option<RecordingCallback> {
        self.recording.store(false, Ordering::SeqCst);
        let mut slot = self.record_cb.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        slot.take()
    }

    fn apply_settings(&self, cfg: CameraConfig) {
        let mut settings = self.settings.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        settings.device = cfg.device;
        settings.fps = cfg.fps;
        settings.width = cfg.resolution.0;
        settings.height = cfg.resolution.1;
        settings.mode = cfg.capture;
        settings.preferred_color_space = cfg.preferred_color_space;
        drop(settings);
        Self::apply_device(cfg.device);
        Self::apply_fps(cfg.fps);
        Self::apply_resolution(cfg.resolution.1);
        Self::apply_mode(cfg.capture);
        Self::apply_color_space(cfg.preferred_color_space);
    }

    fn apply_device(device: CameraDevice) {
        let pos = match device {
            CameraDevice::Front => 1,
            CameraDevice::Back => 0,
        };
        unsafe {
            let _ = oxide_cam_set_position(pos);
        }
    }

    fn apply_fps(fps: u32) {
        unsafe {
            let _ = oxide_cam_set_fps(fps as i32);
        }
    }

    fn apply_resolution(height: u32) {
        unsafe {
            let _ = oxide_cam_set_resolution_height(height as i32);
        }
    }

    fn apply_mode(mode: CaptureMode) {
        let mode_code = match mode {
            CaptureMode::Preview => 0,
            CaptureMode::Photo => 1,
            CaptureMode::Video => 2,
        };
        unsafe {
            let _ = oxide_cam_set_mode(mode_code);
        }
    }

    fn apply_color_space(color_space: Option<ColorSpace>) {
        let color_space_code = match color_space {
            Some(ColorSpace::Srgb) => 0,
            Some(ColorSpace::DisplayP3Linear) => 1,
            _ => -1, // Let the host decide or use default
        };
        if color_space_code != -1 {
            unsafe {
                let _ = oxide_cam_set_color_space(color_space_code);
            }
        }
    }
}

static CAM_STATE: Lazy<CamState> = Lazy::new(CamState::new);

struct AppleCameraStream {
    id: u64,
}

struct AppleCameraRecording {
    active: AtomicBool,
}

impl CameraStream for AppleCameraStream {
    fn stop(&self) {
        remove_camera_subscriber(self.id);
    }
}

impl Drop for AppleCameraStream {
    fn drop(&mut self) {
        self.stop();
    }
}

impl AppleCameraRecording {
    fn new() -> Self {
        Self { active: AtomicBool::new(true) }
    }
}

impl CameraRecording for AppleCameraRecording {
    fn stop(&self) {
        if self.active.swap(false, Ordering::SeqCst) {
            unsafe {
                let _ = oxide_cam_record_stop();
            }
        }
    }

    fn cancel(&self) {
        if self.active.swap(false, Ordering::SeqCst) {
            unsafe {
                let _ = oxide_cam_record_cancel();
            }
        }
    }
}

impl AppleCameraManager {
    fn start_capture(&self) -> Result<(), PlatformError> {
        let rc = unsafe {
            match camera_capture_start_mode(CAM_STATE.has_audio_subscribers()) {
                CameraCaptureStartMode::Default => oxide_cam_start_default(),
                CameraCaptureStartMode::PreviewOnly => oxide_cam_start_default_preview_only(),
            }
        };
        if rc != 0 {
            return Err(camera_error_from_rc(rc, "camera start failed"));
        }
        unsafe {
            let _ = oxide_host_set_camera_running(1);
        }
        Ok(())
    }

    fn stop_capture(&self) {
        unsafe {
            oxide_cam_stop();
            let _ = oxide_host_set_camera_running(0);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CameraCaptureStartMode {
    Default,
    PreviewOnly,
}

fn camera_error_from_rc(rc: i32, context: &'static str) -> PlatformError {
    match rc {
        -2 => PlatformError::PermissionDenied(context),
        -3 => PlatformError::NotFound(context),
        -4 => PlatformError::Busy,
        -5 => PlatformError::Invalid(context),
        -6 => PlatformError::Io(alloc::string::String::from(context)),
        _ => PlatformError::Unsupported(context),
    }
}

fn camera_capture_start_mode(has_audio_subscribers: bool) -> CameraCaptureStartMode {
    if has_audio_subscribers {
        CameraCaptureStartMode::Default
    } else {
        CameraCaptureStartMode::PreviewOnly
    }
}

fn remove_camera_subscriber(id: u64) {
    let mut subs = CAM_STATE.subs.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(pos) = subs.iter().position(|s| s.id == id) {
        subs.remove(pos);
    }
    let should_stop = subs.is_empty();
    drop(subs);
    if should_stop {
        APPLE_CAMERA_MANAGER.stop_capture();
    }
}

impl CameraManager for AppleCameraManager {
    fn start_stream(
        &self,
        cfg: CameraConfig,
        on_frame: alloc::boxed::Box<dyn Fn(CameraFrame) + Send>,
        on_audio: Option<alloc::boxed::Box<dyn Fn(AudioSample) + Send>>,
    ) -> Result<alloc::boxed::Box<dyn CameraStream + Send>, PlatformError> {
        CAM_STATE.ensure_callback();
        CAM_STATE.apply_settings(cfg);
        let mut subs = CAM_STATE.subs.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
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
        Ok(Box::new(AppleCameraStream { id }))
    }

    fn start_recording(
        &self,
        options: RecordingOptions,
        on_event: RecordingCallback,
    ) -> Result<alloc::boxed::Box<dyn CameraRecording + Send>, PlatformError> {
        CAM_STATE.ensure_record_callback();
        CAM_STATE.try_begin_recording(on_event)?;

        let RecordingOptions { destination, container, include_audio, audio_session_mode } =
            options;
        let container_code = match container {
            RecordingContainer::Mp4 => 0,
            RecordingContainer::Mov => 1,
        };

        let audio_session_mode_code = match audio_session_mode {
            AudioSessionMode::Exclusive => 0,
            AudioSessionMode::MixWithOthers => 1,
        };

        unsafe {
            let _ = oxide_cam_set_audio_session_mode(audio_session_mode_code);
        }

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
        let rc = unsafe { oxide_cam_record_start(dest_ptr, dest_len, container_code, audio_flag) };
        drop(path_buf);
        if rc != 0 {
            let _ = CAM_STATE.finish_recording();
            return Err(camera_error_from_rc(rc, "camera recording unavailable"));
        }
        Ok(Box::new(AppleCameraRecording::new()))
    }

    fn select_device(&self, device: CameraDevice) {
        let mut settings =
            CAM_STATE.settings.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        settings.device = device;
        drop(settings);
        CamState::apply_device(device);
    }

    fn set_fps(&self, fps: u32) {
        let mut settings =
            CAM_STATE.settings.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        settings.fps = fps;
        drop(settings);
        CamState::apply_fps(fps);
    }

    fn set_resolution(&self, width: u32, height: u32) {
        let mut settings =
            CAM_STATE.settings.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        settings.width = width;
        settings.height = height;
        drop(settings);
        CamState::apply_resolution(height);
    }

    fn set_mode(&self, mode: CaptureMode) {
        let mut settings =
            CAM_STATE.settings.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        settings.mode = mode;
        drop(settings);
        CamState::apply_mode(mode);
    }

    fn set_focus_point(&self, x: f32, y: f32) -> Result<(), PlatformError> {
        let rc = unsafe { oxide_cam_set_focus_point(x, y) };
        if rc != 0 {
            return Err(camera_error_from_rc(rc, "set_focus_point failed"));
        }
        Ok(())
    }

    fn set_zoom_factor(&self, factor: f32) -> Result<(), PlatformError> {
        let rc = unsafe { oxide_cam_set_zoom_factor(factor) };
        if rc != 0 {
            return Err(camera_error_from_rc(rc, "set_zoom_factor failed"));
        }
        Ok(())
    }

    fn set_flash_mode(&self, mode: FlashMode) -> Result<(), PlatformError> {
        let mode_code = match mode {
            FlashMode::Off => 0,
            FlashMode::On => 1,
            FlashMode::Auto => 2,
        };
        let rc = unsafe { oxide_cam_set_flash_mode(mode_code) };
        if rc != 0 {
            return Err(camera_error_from_rc(rc, "set_flash_mode failed"));
        }
        Ok(())
    }

    fn set_torch_mode(&self, mode: TorchMode) -> Result<(), PlatformError> {
        let (mode_code, level) = match mode {
            TorchMode::Off => (0, 0.0),
            TorchMode::On { level } => (1, level),
        };
        let rc = unsafe { oxide_cam_set_torch_mode(mode_code, level) };
        if rc != 0 {
            return Err(camera_error_from_rc(rc, "set_torch_mode failed"));
        }
        Ok(())
    }

    fn capture_photo(
        &self,
        options: PhotoOptions,
        on_event: PhotoCallback,
    ) -> Result<(), PlatformError> {
        CAM_STATE.ensure_photo_callback();
        let mut slot = CAM_STATE.photo_cb.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if slot.is_some() {
            return Err(PlatformError::Busy);
        }
        *slot = Some(on_event);

        let flash_mode_code = match options.flash_mode {
            FlashMode::Off => 0,
            FlashMode::On => 1,
            FlashMode::Auto => 2,
        };
        let high_speed_flag = if options.high_speed_from_preview { 1 } else { 0 };

        let rc = unsafe { oxide_cam_capture_photo(high_speed_flag, flash_mode_code) };
        if rc != 0 {
            let _ = slot.take(); // Clear callback on failure
            return Err(camera_error_from_rc(rc, "capture_photo failed"));
        }
        Ok(())
    }
}

fn dispatch_camera_frame(frame: CameraFrame) {
    let subs = CAM_STATE.subs.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    for sub in subs.iter() {
        (sub.frame_cb)(frame.clone());
    }
}

fn dispatch_camera_audio(sample: AudioSample) {
    let subs = CAM_STATE.subs.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    for sub in subs.iter() {
        if let Some(cb) = sub.audio_cb.as_ref() {
            cb(sample.clone());
        }
    }
}

const RECORD_EVENT_COMPLETED: u32 = 0;
const RECORD_EVENT_CANCELLED: u32 = 1;
const RECORD_EVENT_FAILED: u32 = 2;

const PHOTO_EVENT_COMPLETED: u32 = 0;
const PHOTO_EVENT_FAILED: u32 = 1;

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

fn photo_error_from(code: i32, msg: alloc::string::String) -> PlatformError {
    match code {
        1 => PlatformError::PermissionDenied("camera photo permission denied"),
        2 => PlatformError::CapabilityDisabled("camera photo"),
        3 => PlatformError::Busy,
        4 => PlatformError::Invalid("camera photo options invalid"),
        5 => PlatformError::Unsupported("camera photo unsupported"),
        6 => PlatformError::Io(msg),
        _ => PlatformError::Unknown(msg),
    }
}

fn dispatch_camera_record(event: RecordingEvent) {
    if let Some(cb) = CAM_STATE.finish_recording() {
        cb(event);
    }
}

fn dispatch_camera_photo(event: PhotoEvent) {
    if let Some(cb) =
        CAM_STATE.photo_cb.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take()
    {
        cb(event);
    }
}

#[no_mangle]
pub unsafe extern "C" fn oxide_cam_audio_trampoline(audio: *const AppleCamAudio) {
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
pub unsafe extern "C" fn oxide_cam_record_trampoline(event: *const AppleCamRecordEvent) {
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
pub unsafe extern "C" fn oxide_cam_photo_trampoline(event: *const AppleCamPhotoEvent) {
    if event.is_null() {
        dispatch_camera_photo(PhotoEvent::Failed(PlatformError::Unknown(
            "unknown photo capture error".into(),
        )));
        return;
    }
    let raw = unsafe { &*event };
    let photo_event = match raw.kind {
        PHOTO_EVENT_COMPLETED => {
            let frame = camera_frame_from_oxide_cam_frame(&raw.frame);
            PhotoEvent::Completed(frame)
        }
        PHOTO_EVENT_FAILED => {
            let msg = if raw.error_msg_ptr.is_null() || raw.error_msg_len == 0 {
                alloc::string::String::new()
            } else {
                let bytes =
                    unsafe { core::slice::from_raw_parts(raw.error_msg_ptr, raw.error_msg_len) };
                alloc::string::String::from_utf8_lossy(bytes).into_owned()
            };
            PhotoEvent::Failed(photo_error_from(raw.error_code, msg))
        }
        _ => PhotoEvent::Failed(photo_error_from(
            -1,
            alloc::string::String::from("unknown camera photo event"),
        )),
    };
    dispatch_camera_photo(photo_event);
}

fn camera_frame_from_oxide_cam_frame(raw_frame: &AppleCamFrame) -> CameraFrame {
    let width = raw_frame.width.max(0) as u32;
    let height = raw_frame.height.max(0) as u32;
    let y_slice = if !raw_frame.y_ptr.is_null() && raw_frame.y_len > 0 {
        unsafe { core::slice::from_raw_parts(raw_frame.y_ptr, raw_frame.y_len) }
    } else {
        &[]
    };
    let uv_slice = if !raw_frame.uv_ptr.is_null() && raw_frame.uv_len > 0 {
        unsafe { core::slice::from_raw_parts(raw_frame.uv_ptr, raw_frame.uv_len) }
    } else {
        &[]
    };
    let image = CameraImage::Nv12 {
        y_plane: y_slice.to_vec(),
        uv_plane: uv_slice.to_vec(),
        stride_y: raw_frame.y_stride as u32,
        stride_uv: raw_frame.uv_stride as u32,
        bit_depth: raw_frame.bit_depth,
        matrix: raw_frame.matrix,
        video_range: raw_frame.video_range,
    };
    CameraFrame {
        image,
        size: (width, height),
        timestamp_ns: raw_frame.timestamp_ns,
        rotation_deg: raw_frame.rotation_deg,
    }
}

#[no_mangle]
pub unsafe extern "C" fn oxide_cam_frame_trampoline(frame: *const AppleCamFrame) {
    if frame.is_null() {
        return;
    }
    let frame = camera_frame_from_oxide_cam_frame(&*frame);
    dispatch_camera_frame(frame);
}

// ===== Camera (start/stop) =====
pub mod camera {
    pub fn start_default() -> i32 {
        unsafe { super::oxide_cam_start_default() }
    }

    pub fn stop() {
        unsafe { super::oxide_cam_stop() }
    }

    pub fn set_fps(fps: i32) -> i32 {
        unsafe { super::oxide_cam_set_fps(fps) }
    }

    pub fn set_resolution_height(h: i32) -> i32 {
        unsafe { super::oxide_cam_set_resolution_height(h) }
    }

    pub fn set_bit_depth(bits: i32) -> i32 {
        unsafe { super::oxide_cam_set_bit_depth(bits) }
    }

    pub fn set_color_space(id: i32) -> i32 {
        unsafe { super::oxide_cam_set_color_space(id) }
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
        fn oxide_cam_query_formats(
            out_ptr: *mut *mut ::core::ffi::c_void,
            out_count: *mut usize,
        ) -> i32;
        fn oxide_cam_query_pixfmts(
            out_ptr: *mut *mut ::core::ffi::c_void,
            out_count: *mut usize,
        ) -> i32;
        fn oxide_cam_caps_free(p: *mut ::core::ffi::c_void);
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
        let ok = unsafe { oxide_cam_query_formats(&mut p, &mut n) };
        if ok == 0 || p.is_null() || n == 0 {
            return alloc::vec::Vec::new();
        }
        let slice = unsafe { core::slice::from_raw_parts(p as *const CamFormat, n) };
        let out = slice.to_vec();
        unsafe { oxide_cam_caps_free(p) };
        out
    }

    pub fn query_pixel_formats() -> alloc::vec::Vec<CamPixFmt> {
        let mut p: *mut ::core::ffi::c_void = core::ptr::null_mut();
        let mut n: usize = 0;
        let ok = unsafe { oxide_cam_query_pixfmts(&mut p, &mut n) };
        if ok == 0 || p.is_null() || n == 0 {
            return alloc::vec::Vec::new();
        }
        let slice = unsafe { core::slice::from_raw_parts(p as *const CamPixFmt, n) };
        let out = slice.to_vec();
        unsafe { oxide_cam_caps_free(p) };
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

#[derive(Debug, Default, Clone, Copy)]
pub struct AppleSocketNetworking;

impl AppleSocketNetworking {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[derive(Debug)]
struct AppleTcpConnection {
    stream: Arc<Mutex<TcpStream>>,
    closed: Arc<AtomicBool>,
}

impl AppleTcpConnection {
    fn new(
        stream: TcpStream,
        on_event: alloc::boxed::Box<dyn Fn(ConnectionEvent) + Send>,
    ) -> Result<Self, PlatformError> {
        let reader = stream.try_clone().map_err(platform_io_error)?;
        let stream = Arc::new(Mutex::new(stream));
        let closed = Arc::new(AtomicBool::new(false));
        let closed_reader = Arc::clone(&closed);
        let _reader = std::thread::Builder::new()
            .name(alloc::string::String::from("oxide-apple-tcp-reader"))
            .spawn(move || {
                on_event(ConnectionEvent::Connected);
                tcp_read_loop(reader, closed_reader, on_event);
            })
            .map_err(platform_io_error)?;
        Ok(Self { stream, closed })
    }
}

impl Connection for AppleTcpConnection {
    fn write<'a>(
        &'a self,
        data: &'a [u8],
    ) -> Pin<alloc::boxed::Box<dyn Future<Output = Result<(), PlatformError>> + Send + 'a>> {
        let result = if self.closed.load(Ordering::Acquire) {
            Err(PlatformError::NotFound("TCP connection is closed"))
        } else {
            let mut stream = self.stream.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            stream.write_all(data).map_err(platform_io_error)
        };
        alloc::boxed::Box::pin(async move { result })
    }

    fn close(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        let stream = self.stream.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = stream.shutdown(Shutdown::Both);
    }
}

#[derive(Debug)]
struct AppleUdpSocket {
    socket: UdpSocket,
    closed: Arc<AtomicBool>,
}

impl AppleUdpSocket {
    fn new(
        socket: UdpSocket,
        on_event: alloc::boxed::Box<dyn Fn(UdpEvent) + Send>,
    ) -> Result<Self, PlatformError> {
        socket.set_read_timeout(Some(Duration::from_millis(200))).map_err(platform_io_error)?;
        let reader = socket.try_clone().map_err(platform_io_error)?;
        let closed = Arc::new(AtomicBool::new(false));
        let closed_reader = Arc::clone(&closed);
        let _reader = std::thread::Builder::new()
            .name(alloc::string::String::from("oxide-apple-udp-reader"))
            .spawn(move || udp_read_loop(reader, closed_reader, on_event))
            .map_err(platform_io_error)?;
        Ok(Self { socket, closed })
    }
}

impl OxideUdpSocket for AppleUdpSocket {
    fn send(&self, packet: &UdpPacket) -> Result<(), PlatformError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(PlatformError::NotFound("UDP socket is closed"));
        }
        if packet.host.trim().is_empty() || packet.port == 0 {
            return Err(PlatformError::Invalid("UDP destination is invalid"));
        }
        self.socket
            .send_to(&packet.data, (packet.host.as_str(), packet.port))
            .map(|_| ())
            .map_err(platform_io_error)
    }

    fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }
}

impl Networking for AppleSocketNetworking {
    fn connect_tcp(
        &self,
        options: ConnectionOptions,
        on_event: alloc::boxed::Box<dyn Fn(ConnectionEvent) + Send>,
    ) -> Result<alloc::boxed::Box<dyn Connection + Send>, PlatformError> {
        let tcp = match options.protocol {
            ProtocolOptions::Tcp(ref tcp) => tcp,
            ProtocolOptions::Quic(_) => {
                return Err(PlatformError::Invalid("connect_tcp requires TCP options"))
            }
        };
        validate_tcp_options(&options, tcp)?;
        let stream =
            TcpStream::connect((options.host.as_str(), options.port)).map_err(platform_io_error)?;
        configure_tcp_stream(&stream, tcp)?;
        let connection = AppleTcpConnection::new(stream, on_event)?;
        Ok(alloc::boxed::Box::new(connection))
    }

    fn connect_quic(
        &self,
        _options: ConnectionOptions,
        _on_event: alloc::boxed::Box<dyn Fn(ConnectionEvent) + Send>,
    ) -> Result<alloc::boxed::Box<dyn ConnectionGroup + Send>, PlatformError> {
        Err(PlatformError::Unsupported("raw QUIC is not implemented for the Apple socket backend"))
    }

    fn bind_udp(
        &self,
        local_port: u16,
        on_event: alloc::boxed::Box<dyn Fn(UdpEvent) + Send>,
    ) -> Result<alloc::boxed::Box<dyn OxideUdpSocket + Send>, PlatformError> {
        let socket = bind_udp_socket(local_port)?;
        let socket = AppleUdpSocket::new(socket, on_event)?;
        Ok(alloc::boxed::Box::new(socket))
    }
}

fn validate_tcp_options(
    options: &ConnectionOptions,
    tcp: &TcpOptions,
) -> Result<(), PlatformError> {
    if options.host.trim().is_empty() || options.port == 0 {
        return Err(PlatformError::Invalid("TCP destination is invalid"));
    }
    if options.tls_options.is_some() {
        return Err(PlatformError::Unsupported(
            "raw TCP TLS is not implemented for the Apple socket backend",
        ));
    }
    if tcp.fast_open {
        return Err(PlatformError::Unsupported(
            "TCP Fast Open is not implemented for the Apple socket backend",
        ));
    }
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn configure_tcp_stream(stream: &TcpStream, tcp: &TcpOptions) -> Result<(), PlatformError> {
    stream.set_nodelay(true).map_err(platform_io_error)?;
    if !tcp.keepalive {
        return Ok(());
    }

    let fd = stream.as_raw_fd();
    set_socket_int(fd, libc::SOL_SOCKET, libc::SO_KEEPALIVE, 1)?;
    if tcp.keepalive_idle_time_secs > 0 {
        let idle = tcp.keepalive_idle_time_secs.min(i32::MAX as u32) as libc::c_int;
        set_socket_int(fd, libc::IPPROTO_TCP, libc::TCP_KEEPALIVE, idle)?;
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "ios")))]
fn configure_tcp_stream(stream: &TcpStream, tcp: &TcpOptions) -> Result<(), PlatformError> {
    stream.set_nodelay(true).map_err(platform_io_error)?;
    if tcp.keepalive {
        return Err(PlatformError::Unsupported(
            "TCP keepalive configuration requires an Apple socket backend",
        ));
    }
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn set_socket_int(
    fd: libc::c_int,
    level: libc::c_int,
    name: libc::c_int,
    value: libc::c_int,
) -> Result<(), PlatformError> {
    let rc = unsafe {
        // SAFETY: `fd` is owned by a live `TcpStream`, and the option payload is a valid `c_int`.
        libc::setsockopt(
            fd,
            level,
            name,
            (&value as *const libc::c_int).cast(),
            core::mem::size_of_val(&value) as libc::socklen_t,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(platform_io_error(std::io::Error::last_os_error()))
    }
}

fn bind_udp_socket(local_port: u16) -> Result<UdpSocket, PlatformError> {
    let port = local_port.to_string();
    let mut last_err = None;
    for host in ["0.0.0.0", "[::]"] {
        let addr = alloc::format!("{host}:{port}");
        match UdpSocket::bind(addr.as_str()) {
            Ok(socket) => return Ok(socket),
            Err(err) => last_err = Some(err),
        }
    }
    Err(platform_io_error(
        last_err.unwrap_or_else(|| std::io::Error::from(IoErrorKind::AddrNotAvailable)),
    ))
}

fn tcp_read_loop(
    mut stream: TcpStream,
    closed: Arc<AtomicBool>,
    on_event: alloc::boxed::Box<dyn Fn(ConnectionEvent) + Send>,
) {
    let mut buf = [0_u8; 16 * 1024];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => {
                on_event(ConnectionEvent::Disconnected { error: None });
                return;
            }
            Ok(n) => on_event(ConnectionEvent::Read(buf[..n].to_vec())),
            Err(err) => {
                let error = if closed.load(Ordering::Acquire) {
                    None
                } else {
                    Some(network_error_from_io(&err))
                };
                on_event(ConnectionEvent::Disconnected { error });
                return;
            }
        }
    }
}

fn udp_read_loop(
    socket: UdpSocket,
    closed: Arc<AtomicBool>,
    on_event: alloc::boxed::Box<dyn Fn(UdpEvent) + Send>,
) {
    let mut buf = [0_u8; 64 * 1024];
    while !closed.load(Ordering::Acquire) {
        match socket.recv_from(&mut buf) {
            Ok((n, addr)) => {
                on_event(UdpEvent::Read(UdpPacket {
                    host: addr.ip().to_string(),
                    port: addr.port(),
                    data: buf[..n].to_vec(),
                }));
            }
            Err(err) if matches!(err.kind(), IoErrorKind::WouldBlock | IoErrorKind::TimedOut) => {}
            Err(err) => {
                if !closed.load(Ordering::Acquire) {
                    on_event(UdpEvent::WriteError(platform_io_error(err)));
                }
                return;
            }
        }
    }
}

fn platform_io_error(err: std::io::Error) -> PlatformError {
    PlatformError::Io(err.to_string())
}

fn network_error_from_io(err: &std::io::Error) -> NetworkError {
    let domain = match err.kind() {
        IoErrorKind::NotFound => NetworkErrorDomain::Dns,
        IoErrorKind::ConnectionRefused
        | IoErrorKind::ConnectionReset
        | IoErrorKind::ConnectionAborted => NetworkErrorDomain::Posix,
        _ => NetworkErrorDomain::Unknown,
    };
    NetworkError { domain, code: err.raw_os_error().unwrap_or(-1), reason: err.to_string() }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AppleLocationSample {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: f64,
    pub horizontal_accuracy: f64,
    pub vertical_accuracy: f64,
    pub speed: f64,
    pub course: f64,
    pub timestamp_ms: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AppleLocationConfig {
    pub accuracy_kind: u32,
    pub distance_filter_m: f64,
    pub allow_background: u8,
    pub precise: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AppleMotionSample {
    pub pressure_pa: f64,
    pub relative_altitude_m: f64,
    pub timestamp_ms: u64,
    pub has_pressure: u8,
    pub has_relative_altitude: u8,
}

type LocationCallback = Arc<Mutex<alloc::boxed::Box<dyn Fn(LocationEvent) + Send + 'static>>>;
type MotionCallback = Arc<Mutex<alloc::boxed::Box<dyn Fn(MotionSample) + Send + 'static>>>;

static APPLE_LOCATION_INIT: Once = Once::new();
static APPLE_LOCATION_SUBS: Lazy<Mutex<alloc::vec::Vec<LocationCallback>>> =
    Lazy::new(|| Mutex::new(alloc::vec::Vec::new()));
static APPLE_LOCATION_LAST: Lazy<Mutex<Option<LocationReading>>> = Lazy::new(|| Mutex::new(None));
static APPLE_LOCATION_HISTORY: Lazy<Mutex<VecDeque<LocationReading>>> =
    Lazy::new(|| Mutex::new(VecDeque::with_capacity(LOCATION_HISTORY_MAX)));
static APPLE_LOCATION_REGIONS: Lazy<Arc<Mutex<RegionState>>> =
    Lazy::new(|| Arc::new(Mutex::new(RegionState::default())));
static APPLE_LOCATION_RUNNING: AtomicBool = AtomicBool::new(false);
const LOCATION_HISTORY_MAX: usize = 128;

#[derive(Default)]
struct RegionState {
    entries: alloc::vec::Vec<RegionEntry>,
}

#[derive(Clone, Copy)]
struct RegionEntry {
    region: GeoRegion,
    inside: bool,
}

struct AppleGeoRegionTracker {
    state: Arc<Mutex<RegionState>>,
}

impl GeoRegionTracker for AppleGeoRegionTracker {
    fn monitored_regions(&self) -> alloc::vec::Vec<GeoRegion> {
        let state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.entries.iter().map(|e| e.region).collect()
    }

    fn set_regions(&self, regions: &[GeoRegion]) -> Result<(), PlatformError> {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let last = *APPLE_LOCATION_LAST.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.entries = regions
            .iter()
            .map(|r| canonical_region(*r))
            .map(|region| {
                let inside = last.is_some_and(|loc| region_contains(region, loc));
                RegionEntry { region, inside }
            })
            .collect();
        Ok(())
    }
}

impl AppleGeoRegionTracker {
    fn new() -> Self {
        Self { state: APPLE_LOCATION_REGIONS.clone() }
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
    let mut state =
        APPLE_LOCATION_REGIONS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
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

fn reading_from_apple_sample(raw: &AppleLocationSample) -> LocationReading {
    LocationReading {
        latitude_deg: raw.latitude,
        longitude_deg: raw.longitude,
        altitude_m: raw.altitude as f32,
        horizontal_accuracy_m: raw.horizontal_accuracy.max(0.0) as f32,
        vertical_accuracy_m: raw.vertical_accuracy.max(0.0) as f32,
        speed_mps: raw.speed.max(0.0) as f32,
        course_deg: if raw.course.is_sign_negative() { 0.0 } else { raw.course as f32 },
        timestamp_ms: raw.timestamp_ms,
    }
}

fn location_accuracy_to_apple_kind(accuracy: LocationAccuracy) -> u32 {
    match accuracy {
        LocationAccuracy::Reduced => 0,
        LocationAccuracy::Balanced => 1,
        LocationAccuracy::LowPower => 2,
        LocationAccuracy::Precise => 3,
    }
}

fn location_callbacks() -> alloc::vec::Vec<LocationCallback> {
    APPLE_LOCATION_SUBS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .map(Arc::clone)
        .collect()
}

fn ensure_location_trampolines() {
    APPLE_LOCATION_INIT.call_once(|| unsafe {
        oxide_host_set_location_callback(Some(oxide_location_update_trampoline));
        oxide_host_set_location_error_callback(Some(oxide_location_error_trampoline));
    });
}

#[no_mangle]
pub unsafe extern "C" fn oxide_location_update_trampoline(sample: *const AppleLocationSample) {
    if sample.is_null() {
        return;
    }
    let raw = unsafe { &*sample };
    let reading = reading_from_apple_sample(raw);
    {
        let mut hist =
            APPLE_LOCATION_HISTORY.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if hist.len() >= LOCATION_HISTORY_MAX {
            hist.pop_front();
        }
        hist.push_back(reading);
    }
    *APPLE_LOCATION_LAST.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(reading);

    let region_events = update_region_events(reading);
    let callbacks = location_callbacks();
    for callback in callbacks {
        let callback = callback.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        callback(LocationEvent::Update(reading));
        for event in &region_events {
            callback(event.clone());
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn oxide_location_error_trampoline(msg_ptr: *const u8, len: usize) {
    if msg_ptr.is_null() || len == 0 {
        return;
    }
    let bytes = unsafe { std::slice::from_raw_parts(msg_ptr, len) };
    let msg = alloc::string::String::from_utf8_lossy(bytes).into_owned();
    let err = PlatformError::Unknown(msg);
    let callbacks = location_callbacks();
    for callback in callbacks {
        let callback = callback.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        callback(LocationEvent::Error(err.clone()));
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct AppleLocationService;

impl AppleLocationService {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl LocationService for AppleLocationService {
    fn start(&self, opts: LocationOptions) -> Result<(), PlatformError> {
        ensure_location_trampolines();
        let cfg = AppleLocationConfig {
            accuracy_kind: location_accuracy_to_apple_kind(opts.accuracy),
            distance_filter_m: f64::from(opts.distance_filter_m),
            allow_background: if opts.allow_background_updates { 1 } else { 0 },
            precise: if opts.precise { 1 } else { 0 },
        };
        let rc = unsafe { oxide_host_location_start(cfg) };
        if rc != 0 {
            return Err(PlatformError::Unsupported("location start failed"));
        }
        APPLE_LOCATION_RUNNING.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn stop(&self) {
        APPLE_LOCATION_RUNNING.store(false, Ordering::SeqCst);
        unsafe {
            oxide_host_location_stop();
        }
    }

    fn request_once(&self) {
        ensure_location_trampolines();
        unsafe {
            oxide_host_location_request_once();
        }
    }

    fn last(&self) -> Option<LocationReading> {
        if let Some(cached) =
            *APPLE_LOCATION_LAST.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
        {
            return Some(cached);
        }
        let mut raw = AppleLocationSample {
            latitude: 0.0,
            longitude: 0.0,
            altitude: 0.0,
            horizontal_accuracy: 0.0,
            vertical_accuracy: 0.0,
            speed: 0.0,
            course: 0.0,
            timestamp_ms: 0,
        };
        let ok = unsafe { oxide_host_location_last(&mut raw) } != 0;
        if ok {
            let reading = reading_from_apple_sample(&raw);
            *APPLE_LOCATION_LAST.lock().unwrap_or_else(std::sync::PoisonError::into_inner) =
                Some(reading);
            Some(reading)
        } else {
            None
        }
    }

    fn subscribe(&self, f: alloc::boxed::Box<dyn Fn(LocationEvent) + Send>) {
        ensure_location_trampolines();
        APPLE_LOCATION_SUBS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(Arc::new(Mutex::new(f)));
    }

    fn history(&self) -> alloc::vec::Vec<LocationReading> {
        APPLE_LOCATION_HISTORY
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .cloned()
            .collect()
    }

    fn region_tracker(&self) -> Option<alloc::boxed::Box<dyn GeoRegionTracker>> {
        Some(alloc::boxed::Box::new(AppleGeoRegionTracker::new()))
    }

    fn set_accuracy(&self, accuracy: LocationAccuracy) -> Result<(), PlatformError> {
        let rc =
            unsafe { oxide_host_location_set_accuracy(location_accuracy_to_apple_kind(accuracy)) };
        if rc != 0 {
            return Err(PlatformError::Unsupported("location accuracy update failed"));
        }
        Ok(())
    }
}

static APPLE_MOTION_INIT: Once = Once::new();
static APPLE_MOTION_SUBS: Lazy<Mutex<alloc::vec::Vec<MotionCallback>>> =
    Lazy::new(|| Mutex::new(alloc::vec::Vec::new()));
static APPLE_MOTION_HISTORY: Lazy<Mutex<VecDeque<MotionSample>>> =
    Lazy::new(|| Mutex::new(VecDeque::with_capacity(MOTION_HISTORY_MAX)));
static APPLE_MOTION_RUNNING: AtomicBool = AtomicBool::new(false);
const MOTION_HISTORY_MAX: usize = 128;

fn ensure_motion_trampolines() {
    APPLE_MOTION_INIT.call_once(|| unsafe {
        oxide_host_set_motion_callback(Some(oxide_motion_trampoline));
    });
}

fn motion_callbacks() -> alloc::vec::Vec<MotionCallback> {
    APPLE_MOTION_SUBS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .map(Arc::clone)
        .collect()
}

#[no_mangle]
pub unsafe extern "C" fn oxide_motion_trampoline(sample: *const AppleMotionSample) {
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
        let mut hist =
            APPLE_MOTION_HISTORY.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if hist.len() >= MOTION_HISTORY_MAX {
            hist.pop_front();
        }
        hist.push_back(reading);
    }
    let callbacks = motion_callbacks();
    for callback in callbacks {
        let callback = callback.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        callback(reading);
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct AppleMotionService;

impl AppleMotionService {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl MotionService for AppleMotionService {
    fn start(&self) -> Result<(), PlatformError> {
        ensure_motion_trampolines();
        let rc = unsafe { oxide_host_motion_start() };
        if rc != 0 {
            return Err(PlatformError::Unsupported("motion unavailable"));
        }
        APPLE_MOTION_RUNNING.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn stop(&self) {
        APPLE_MOTION_RUNNING.store(false, Ordering::SeqCst);
        unsafe {
            oxide_host_motion_stop();
        }
    }

    fn is_running(&self) -> bool {
        APPLE_MOTION_RUNNING.load(Ordering::SeqCst) || unsafe { oxide_host_motion_is_active() != 0 }
    }

    fn subscribe(&self, f: alloc::boxed::Box<dyn Fn(MotionSample) + Send>) {
        ensure_motion_trampolines();
        APPLE_MOTION_SUBS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(Arc::new(Mutex::new(f)));
    }

    fn pressure_history(&self) -> alloc::vec::Vec<MotionSample> {
        APPLE_MOTION_HISTORY
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .cloned()
            .collect()
    }
}

type PushCallback = alloc::boxed::Box<dyn Fn(PushNotification) + Send + 'static>;

static APPLE_PUSH_TOKEN: Lazy<Mutex<Option<PushToken>>> = Lazy::new(|| Mutex::new(None));
static APPLE_PUSH_SUBS: Lazy<Mutex<alloc::vec::Vec<PushCallback>>> =
    Lazy::new(|| Mutex::new(alloc::vec::Vec::new()));
static APPLE_PUSH_INIT: Once = Once::new();

fn push_token_from_provider(provider: u32, value: alloc::string::String) -> PushToken {
    let provider = match provider {
        0 => PushProvider::Apns,
        1 => PushProvider::Fcm,
        _ => PushProvider::Apns,
    };
    PushToken { provider, value }
}

#[no_mangle]
pub unsafe extern "C" fn oxide_push_token_trampoline(provider: u32, ptr: *const u8, len: usize) {
    if ptr.is_null() || len == 0 {
        *APPLE_PUSH_TOKEN.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        return;
    }
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
    if let Ok(token) = core::str::from_utf8(bytes) {
        let token = push_token_from_provider(provider, token.to_owned());
        *APPLE_PUSH_TOKEN.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(token);
    }
}

#[no_mangle]
pub unsafe extern "C" fn oxide_push_notify_trampoline(ptr: *const u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
    let Ok(json) = core::str::from_utf8(bytes) else {
        return;
    };

    let mut notification = PushNotification {
        user_info: alloc::collections::BTreeMap::new(),
        badge: None,
        sound: None,
        presentation: PushPresentation::Foreground,
    };
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(json) {
        if let Some(object) = value.as_object() {
            for (key, value) in object {
                let mapped = value
                    .as_str()
                    .map(alloc::string::ToString::to_string)
                    .unwrap_or_else(|| value.to_string());
                notification.user_info.insert(key.clone(), mapped);
            }
        }
    }

    let callbacks = APPLE_PUSH_SUBS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    for callback in callbacks.iter() {
        callback(notification.clone());
    }
}

fn init_push_trampolines() {
    APPLE_PUSH_INIT.call_once(|| unsafe {
        oxide_host_set_push_token_callback(Some(oxide_push_token_trampoline));
        oxide_host_set_push_notify_callback(Some(oxide_push_notify_trampoline));
    });
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ApplePushManager;

impl ApplePushManager {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl PushManager for ApplePushManager {
    fn register(&self) {
        init_push_trampolines();
        unsafe {
            oxide_host_push_register();
        }
    }

    fn device_token(&self) -> Option<PushToken> {
        if let Some(token) =
            APPLE_PUSH_TOKEN.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
        {
            return Some(token);
        }

        let mut ptr: *mut u8 = core::ptr::null_mut();
        let mut len: usize = 0;
        let ok = unsafe { oxide_host_push_get_device_token(&mut ptr, &mut len) };
        if ok != 0 && !ptr.is_null() && len > 0 {
            let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
            let token = alloc::string::String::from_utf8_lossy(bytes).into_owned();
            unsafe {
                oxide_host_string_free(ptr);
            }
            *APPLE_PUSH_TOKEN.lock().unwrap_or_else(std::sync::PoisonError::into_inner) =
                Some(PushToken { provider: PushProvider::Apns, value: token.clone() });
            return Some(PushToken { provider: PushProvider::Apns, value: token });
        }
        None
    }

    fn subscribe(&self, f: alloc::boxed::Box<dyn Fn(PushNotification) + Send>) {
        init_push_trampolines();
        APPLE_PUSH_SUBS.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(f);
    }

    fn set_badge(&self, count: i32) {
        unsafe {
            oxide_host_push_set_badge(count);
        }
    }

    fn clear_badge(&self) {
        unsafe {
            oxide_host_push_clear_badge();
        }
    }

    fn clear_all_delivered(&self) {
        unsafe {
            oxide_host_push_clear_all_delivered();
        }
    }
}

#[cfg(any(test, all(feature = "web-view-macos", target_os = "macos")))]
type WebViewCallback =
    Arc<Mutex<alloc::boxed::Box<dyn Fn(oxide_platform_api::web_view::WebViewEvent) + Send>>>;

#[cfg(any(test, all(feature = "web-view-macos", target_os = "macos")))]
static APPLE_WEB_VIEW_INIT: Once = Once::new();
#[cfg(any(test, all(feature = "web-view-macos", target_os = "macos")))]
static APPLE_WEB_VIEW_CALLBACKS: Lazy<Mutex<HashMap<u64, WebViewCallback>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
#[cfg(any(test, all(feature = "web-view-macos", target_os = "macos")))]
static APPLE_WEB_VIEW_NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[cfg(any(test, all(feature = "web-view-macos", target_os = "macos")))]
fn init_web_view_trampoline() {
    APPLE_WEB_VIEW_INIT.call_once(|| unsafe {
        oxide_web_view_set_event_callback(Some(oxide_web_view_event_trampoline));
    });
}

#[cfg(any(test, all(feature = "web-view-macos", target_os = "macos")))]
fn web_view_error(code: i32) -> PlatformError {
    match code {
        -1 => PlatformError::Invalid("web view input invalid"),
        -2 => PlatformError::Unsupported("web view unavailable"),
        -3 => PlatformError::Busy,
        -4 => PlatformError::NotFound("web view handle not found"),
        -5 => PlatformError::Unknown(alloc::string::String::from("web view script failed")),
        -6 => PlatformError::Io(alloc::string::String::from("web view result copy failed")),
        _ => PlatformError::Unknown(alloc::format!("web view host error {code}")),
    }
}

#[cfg(any(test, all(feature = "web-view-macos", target_os = "macos")))]
fn web_view_callbacks() -> std::sync::MutexGuard<'static, HashMap<u64, WebViewCallback>> {
    APPLE_WEB_VIEW_CALLBACKS.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(any(test, all(feature = "web-view-macos", target_os = "macos")))]
#[no_mangle]
pub unsafe extern "C" fn oxide_web_view_event_trampoline(
    id: u64,
    kind: u32,
    msg_ptr: *const u8,
    msg_len: usize,
) {
    let event = match kind {
        0 => oxide_platform_api::web_view::WebViewEvent::LoadFinished,
        1 => {
            let message = if msg_ptr.is_null() || msg_len == 0 {
                alloc::string::String::from("web view load failed")
            } else {
                let bytes = unsafe { core::slice::from_raw_parts(msg_ptr, msg_len) };
                alloc::string::String::from_utf8_lossy(bytes).into_owned()
            };
            oxide_platform_api::web_view::WebViewEvent::LoadFailed(PlatformError::Unknown(message))
        }
        _ => return,
    };
    let callback = web_view_callbacks().get(&id).map(Arc::clone);
    if let Some(callback) = callback {
        let callback = callback.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        callback(event);
    }
}

#[cfg(any(test, all(feature = "web-view-macos", target_os = "macos")))]
#[derive(Debug, Default, Clone, Copy)]
pub struct AppleWebViewService;

#[cfg(any(test, all(feature = "web-view-macos", target_os = "macos")))]
impl AppleWebViewService {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[cfg(any(test, all(feature = "web-view-macos", target_os = "macos")))]
#[derive(Debug)]
struct AppleWebView {
    id: u64,
    closed: AtomicBool,
}

#[cfg(any(test, all(feature = "web-view-macos", target_os = "macos")))]
impl AppleWebView {
    fn new(id: u64) -> Self {
        Self { id, closed: AtomicBool::new(false) }
    }

    fn close_once(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        web_view_callbacks().remove(&self.id);
        unsafe {
            oxide_web_view_close(self.id);
        }
    }
}

#[cfg(any(test, all(feature = "web-view-macos", target_os = "macos")))]
impl Drop for AppleWebView {
    fn drop(&mut self) {
        self.close_once();
    }
}

#[cfg(any(test, all(feature = "web-view-macos", target_os = "macos")))]
impl oxide_platform_api::web_view::WebView for AppleWebView {
    fn execute_script(
        &self,
        script: &str,
    ) -> Pin<
        alloc::boxed::Box<
            dyn Future<Output = Result<Option<alloc::string::String>, PlatformError>> + Send + '_,
        >,
    > {
        let result = if script.trim().is_empty() {
            Err(PlatformError::Invalid("web view script is empty"))
        } else if self.closed.load(Ordering::Acquire) {
            Err(PlatformError::NotFound("web view handle not found"))
        } else {
            let mut ptr: *mut u8 = core::ptr::null_mut();
            let mut len: usize = 0;
            let code = unsafe {
                oxide_web_view_execute_script(
                    self.id,
                    script.as_ptr(),
                    script.len(),
                    &mut ptr,
                    &mut len,
                )
            };
            if code < 0 {
                Err(web_view_error(code))
            } else if code == 0 {
                Ok(None)
            } else if len == 0 {
                Ok(Some(alloc::string::String::new()))
            } else if ptr.is_null() {
                Err(PlatformError::Unknown(alloc::string::String::from(
                    "web view host returned null script result",
                )))
            } else {
                let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
                let value = alloc::string::String::from_utf8_lossy(bytes).into_owned();
                unsafe {
                    oxide_web_view_free_string(ptr);
                }
                Ok(Some(value))
            }
        };
        alloc::boxed::Box::pin(async move { result })
    }

    fn close(&self) {
        self.close_once();
    }
}

#[cfg(any(test, all(feature = "web-view-macos", target_os = "macos")))]
impl oxide_platform_api::web_view::WebViewService for AppleWebViewService {
    fn create_view(
        &self,
        url: &str,
        on_event: alloc::boxed::Box<dyn Fn(oxide_platform_api::web_view::WebViewEvent) + Send>,
    ) -> Result<alloc::boxed::Box<dyn oxide_platform_api::web_view::WebView + Send>, PlatformError>
    {
        if url.trim().is_empty() {
            return Err(PlatformError::Invalid("web view URL is empty"));
        }
        init_web_view_trampoline();
        let id = APPLE_WEB_VIEW_NEXT_ID.fetch_add(1, Ordering::AcqRel);
        web_view_callbacks().insert(id, Arc::new(Mutex::new(on_event)));
        let code = unsafe { oxide_web_view_create(url.as_ptr(), url.len(), id) };
        if code != 0 {
            web_view_callbacks().remove(&id);
            return Err(web_view_error(code));
        }
        Ok(alloc::boxed::Box::new(AppleWebView::new(id)))
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AppleMediaAsset {
    pub identifier_ptr: *const u8,
    pub identifier_len: usize,
    pub media_type: u8,
    pub creation_date: u64,
    pub duration_sec: f64,
    pub width: u32,
    pub height: u32,
    pub file_size: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AppleMediaImageData {
    pub data_ptr: *const u8,
    pub data_len: usize,
    pub width: u32,
    pub height: u32,
    pub row_bytes: usize,
}

#[derive(Default)]
pub struct AppleMediaLibraryManager;

#[derive(Clone, Debug)]
pub struct AppleRawImageData {
    pub width: u32,
    pub height: u32,
    pub row_bytes: usize,
    pub bgra: alloc::vec::Vec<u8>,
}

struct OwnedImageData {
    width: u32,
    height: u32,
    row_bytes: usize,
    bytes: alloc::vec::Vec<u8>,
}

impl OwnedImageData {
    fn into_raw_image_data(self) -> AppleRawImageData {
        AppleRawImageData {
            width: self.width,
            height: self.height,
            row_bytes: self.row_bytes,
            bgra: self.bytes,
        }
    }
}

impl AppleMediaLibraryManager {
    fn media_error_from_result(result: i32, failure_label: &'static str) -> PlatformError {
        match result {
            -1 => PlatformError::PermissionDenied("media_library"),
            -2 => PlatformError::Io(alloc::string::String::from(failure_label)),
            -3 => PlatformError::Invalid(failure_label),
            -4 => PlatformError::Unsupported(failure_label),
            _ => PlatformError::Unknown(alloc::format!("{failure_label}: {result}")),
        }
    }

    fn load_owned_image_data<F>(
        &self,
        id: &AssetId,
        missing_is_ok: bool,
        failure_label: &'static str,
        missing_label: &'static str,
        load: F,
    ) -> Result<Option<OwnedImageData>, PlatformError>
    where
        F: FnOnce(*const u8, usize, &mut AppleMediaImageData) -> i32,
    {
        let identifier = id.0.as_bytes();
        let mut image_data = AppleMediaImageData {
            data_ptr: core::ptr::null(),
            data_len: 0,
            width: 0,
            height: 0,
            row_bytes: 0,
        };
        let result = load(identifier.as_ptr(), identifier.len(), &mut image_data);

        if result < 0 {
            return Err(Self::media_error_from_result(result, failure_label));
        }
        if image_data.data_ptr.is_null() || image_data.data_len == 0 {
            if missing_is_ok {
                return Ok(None);
            }
            return Err(PlatformError::NotFound(missing_label));
        }

        let bytes = unsafe {
            core::slice::from_raw_parts(image_data.data_ptr, image_data.data_len).to_vec()
        };
        unsafe {
            oxide_media_free_image_data(image_data.data_ptr, image_data.data_len);
        }
        Ok(Some(OwnedImageData {
            width: image_data.width,
            height: image_data.height,
            row_bytes: image_data.row_bytes,
            bytes,
        }))
    }

    fn load_image_data(
        &self,
        id: &AssetId,
        quality: ImageQuality,
    ) -> Result<AssetData, PlatformError> {
        let image = self
            .load_owned_image_data(
                id,
                false,
                "media image request failed",
                "media image data",
                |identifier_ptr, identifier_len, image_data| match quality {
                    ImageQuality::Thumbnail => unsafe {
                        oxide_media_load_thumbnail(identifier_ptr, identifier_len, 1, image_data)
                    },
                    ImageQuality::Display => unsafe {
                        oxide_media_load_full_image(identifier_ptr, identifier_len, image_data)
                    },
                },
            )?
            .ok_or(PlatformError::NotFound("media image data"))?;

        Ok(AssetData::Image { data: image.bytes, format: ImageFormat::Jpeg })
    }

    pub fn load_image_bgra_data(
        &self,
        id: &AssetId,
        quality: ImageQuality,
    ) -> Result<AppleRawImageData, PlatformError> {
        self.load_owned_image_data(
            id,
            false,
            "media image rgba request failed",
            "media image rgba data",
            |identifier_ptr, identifier_len, image_data| match quality {
                ImageQuality::Thumbnail => unsafe {
                    oxide_media_load_thumbnail_rgba(identifier_ptr, identifier_len, 0, image_data)
                },
                ImageQuality::Display => unsafe {
                    oxide_media_load_full_image_rgba(identifier_ptr, identifier_len, image_data)
                },
            },
        )?
        .map(OwnedImageData::into_raw_image_data)
        .ok_or(PlatformError::NotFound("media image rgba data"))
    }

    pub fn load_display_image_bgra_data_if_available(
        &self,
        id: &AssetId,
    ) -> Result<Option<AppleRawImageData>, PlatformError> {
        self.load_owned_image_data(
            id,
            true,
            "media display image rgba request failed",
            "media image cached rgba data",
            |identifier_ptr, identifier_len, image_data| unsafe {
                oxide_media_load_full_image_rgba(identifier_ptr, identifier_len, image_data)
            },
        )
        .map(|image| image.map(OwnedImageData::into_raw_image_data))
    }
}

impl MediaLibrary for AppleMediaLibraryManager {
    fn query_assets(
        &self,
        asset_type: AssetType,
        limit: u32,
        offset: u32,
    ) -> core::pin::Pin<
        alloc::boxed::Box<
            dyn core::future::Future<Output = Result<alloc::vec::Vec<MediaAsset>, PlatformError>>
                + Send
                + '_,
        >,
    > {
        alloc::boxed::Box::pin(async move {
            if limit == 0 {
                return Ok(alloc::vec::Vec::new());
            }

            let type_mask = match asset_type {
                AssetType::Image => 1,
                AssetType::Video => 2,
            };
            let fetch_limit = limit.saturating_add(offset).min(i32::MAX as u32) as i32;

            let mut assets_ptr: *const AppleMediaAsset = core::ptr::null();
            let mut count: usize = 0;
            let result = unsafe {
                oxide_media_fetch_assets(type_mask, fetch_limit, 0, &mut assets_ptr, &mut count)
            };

            if result < 0 {
                return Err(Self::media_error_from_result(result, "media query failed"));
            }
            if assets_ptr.is_null() || count == 0 {
                return Ok(alloc::vec::Vec::new());
            }

            let mut assets = alloc::vec::Vec::with_capacity(count);
            for idx in 0..count {
                let Some(raw) = (unsafe { assets_ptr.add(idx).as_ref() }) else {
                    continue;
                };

                let mapped_type = match raw.media_type {
                    0 => AssetType::Image,
                    1 => AssetType::Video,
                    _ => continue,
                };
                if mapped_type != asset_type {
                    continue;
                }

                let duration_ms = if raw.duration_sec > 0.0 {
                    Some((raw.duration_sec * 1000.0).round() as u64)
                } else {
                    None
                };
                assets.push(MediaAsset {
                    id: AssetId(
                        copy_string(raw.identifier_ptr, raw.identifier_len).unwrap_or_default(),
                    ),
                    asset_type: mapped_type,
                    width: raw.width,
                    height: raw.height,
                    duration_ms,
                });
            }

            unsafe {
                oxide_media_free_assets(assets_ptr, count);
            }

            let start = offset as usize;
            if start >= assets.len() {
                return Ok(alloc::vec::Vec::new());
            }
            let mut paged = assets.split_off(start);
            let max_len = limit as usize;
            if paged.len() > max_len {
                paged.truncate(max_len);
            }
            Ok(paged)
        })
    }

    fn request_image_data(
        &self,
        id: &AssetId,
        quality: ImageQuality,
    ) -> core::pin::Pin<
        alloc::boxed::Box<
            dyn core::future::Future<Output = Result<AssetData, PlatformError>> + Send + '_,
        >,
    > {
        let owned_id = id.clone();
        alloc::boxed::Box::pin(async move { self.load_image_data(&owned_id, quality) })
    }

    fn request_video_data(
        &self,
        id: &AssetId,
    ) -> core::pin::Pin<
        alloc::boxed::Box<
            dyn core::future::Future<Output = Result<AssetData, PlatformError>> + Send + '_,
        >,
    > {
        let owned_id = id.clone();
        alloc::boxed::Box::pin(async move {
            let identifier = owned_id.0.as_bytes();
            let mut path_ptr: *const u8 = core::ptr::null();
            let mut path_len: usize = 0;
            let result = unsafe {
                oxide_media_load_video_file(
                    identifier.as_ptr(),
                    identifier.len(),
                    &mut path_ptr,
                    &mut path_len,
                )
            };
            if result < 0 {
                return Err(Self::media_error_from_result(result, "media video request failed"));
            }
            let Some(file_path) = copy_string(path_ptr, path_len) else {
                return Err(PlatformError::NotFound("media video file"));
            };
            unsafe {
                oxide_media_free_string(path_ptr, path_len);
            }
            Ok(AssetData::Video { file_path })
        })
    }
}

fn copy_bytes(ptr: *const u8, len: usize) -> alloc::vec::Vec<u8> {
    if ptr.is_null() || len == 0 {
        return alloc::vec::Vec::new();
    }
    unsafe { core::slice::from_raw_parts(ptr, len).to_vec() }
}

fn copy_string(ptr: *const u8, len: usize) -> Option<alloc::string::String> {
    if ptr.is_null() || len == 0 {
        return None;
    }
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
    Some(alloc::string::String::from_utf8_lossy(bytes).into_owned())
}

fn http_error(rc: i32) -> PlatformError {
    match rc {
        -1 => PlatformError::Invalid("invalid HTTP request"),
        -2 => PlatformError::Io(alloc::string::String::from("native HTTP request failed")),
        -3 => PlatformError::Invalid("native HTTP response was not HTTP"),
        -4 => PlatformError::Invalid("native HTTP response exceeded limit"),
        -5 => PlatformError::Io(alloc::string::String::from("native HTTP allocation failed")),
        -6 => PlatformError::Busy,
        _ => PlatformError::Unknown(alloc::format!("native HTTP request failed: {rc}")),
    }
}

#[must_use]
pub fn reachability_state_from_apple_path(
    status: u32,
    iface: u32,
    expensive: bool,
) -> ReachabilityState {
    if status == 0 {
        ReachabilityState::Offline
    } else {
        let kind = match iface {
            APPLE_PATH_KIND_WIFI => NetworkPathKind::Wifi,
            APPLE_PATH_KIND_CELLULAR => NetworkPathKind::Cellular,
            APPLE_PATH_KIND_WIRED => NetworkPathKind::Wired,
            _ => NetworkPathKind::Other,
        };
        ReachabilityState::Online { path: NetworkPath { kind, expensive } }
    }
}

#[must_use]
pub fn network_interface_from_apple_path_kind(
    iface: u32,
) -> oxide_platform_api::network_status::NetworkInterface {
    match iface {
        APPLE_PATH_KIND_WIFI => oxide_platform_api::network_status::NetworkInterface::WIFI,
        APPLE_PATH_KIND_CELLULAR => oxide_platform_api::network_status::NetworkInterface::CELLULAR,
        APPLE_PATH_KIND_WIRED => oxide_platform_api::network_status::NetworkInterface::WIRED,
        _ => oxide_platform_api::network_status::NetworkInterface::empty(),
    }
}

#[must_use]
pub fn network_status_from_apple_path(
    status: u32,
    iface: u32,
) -> oxide_platform_api::network_status::NetworkStatus {
    oxide_platform_api::network_status::NetworkStatus {
        is_connected: status != 0,
        interfaces: if status == 0 {
            oxide_platform_api::network_status::NetworkInterface::empty()
        } else {
            network_interface_from_apple_path_kind(iface)
        },
    }
}

#[must_use]
pub fn network_status_from_apple_interface_mask(
    connected: bool,
    interface_mask: u32,
) -> oxide_platform_api::network_status::NetworkStatus {
    let mut interfaces = oxide_platform_api::network_status::NetworkInterface::empty();
    if connected {
        if interface_mask & APPLE_INTERFACE_WIFI != 0 {
            interfaces |= oxide_platform_api::network_status::NetworkInterface::WIFI;
        }
        if interface_mask & APPLE_INTERFACE_CELLULAR != 0 {
            interfaces |= oxide_platform_api::network_status::NetworkInterface::CELLULAR;
        }
        if interface_mask & APPLE_INTERFACE_WIRED != 0 {
            interfaces |= oxide_platform_api::network_status::NetworkInterface::WIRED;
        }
    }
    oxide_platform_api::network_status::NetworkStatus { is_connected: connected, interfaces }
}

#[must_use]
pub fn permission_domain_to_apple_code(domain: oxide_platform_api::PermissionDomain) -> u32 {
    match domain {
        oxide_platform_api::PermissionDomain::Notifications => APPLE_PERMISSION_NOTIFICATIONS,
        oxide_platform_api::PermissionDomain::Location => APPLE_PERMISSION_LOCATION,
        oxide_platform_api::PermissionDomain::Camera => APPLE_PERMISSION_CAMERA,
        oxide_platform_api::PermissionDomain::Contacts => APPLE_PERMISSION_CONTACTS,
        oxide_platform_api::PermissionDomain::Bluetooth => APPLE_PERMISSION_BLUETOOTH,
        oxide_platform_api::PermissionDomain::Motion => APPLE_PERMISSION_MOTION,
        oxide_platform_api::PermissionDomain::Microphone => APPLE_PERMISSION_MICROPHONE,
        oxide_platform_api::PermissionDomain::MediaLibrary => APPLE_PERMISSION_MEDIA_LIBRARY,
    }
}

#[must_use]
pub fn permission_domain_from_apple_code(
    code: u32,
) -> Option<oxide_platform_api::PermissionDomain> {
    match code {
        APPLE_PERMISSION_NOTIFICATIONS => Some(oxide_platform_api::PermissionDomain::Notifications),
        APPLE_PERMISSION_LOCATION => Some(oxide_platform_api::PermissionDomain::Location),
        APPLE_PERMISSION_CAMERA => Some(oxide_platform_api::PermissionDomain::Camera),
        APPLE_PERMISSION_CONTACTS => Some(oxide_platform_api::PermissionDomain::Contacts),
        APPLE_PERMISSION_BLUETOOTH => Some(oxide_platform_api::PermissionDomain::Bluetooth),
        APPLE_PERMISSION_MOTION => Some(oxide_platform_api::PermissionDomain::Motion),
        APPLE_PERMISSION_MICROPHONE => Some(oxide_platform_api::PermissionDomain::Microphone),
        APPLE_PERMISSION_MEDIA_LIBRARY => Some(oxide_platform_api::PermissionDomain::MediaLibrary),
        _ => None,
    }
}

#[must_use]
pub fn permission_status_to_apple_code(status: oxide_platform_api::PermissionStatus) -> u32 {
    match status {
        oxide_platform_api::PermissionStatus::NotDetermined => APPLE_PERMISSION_NOT_DETERMINED,
        oxide_platform_api::PermissionStatus::Denied => APPLE_PERMISSION_DENIED,
        oxide_platform_api::PermissionStatus::Limited => APPLE_PERMISSION_LIMITED,
        oxide_platform_api::PermissionStatus::Authorized => APPLE_PERMISSION_AUTHORIZED,
    }
}

#[must_use]
pub fn permission_status_from_apple_code(code: u32) -> oxide_platform_api::PermissionStatus {
    match code {
        APPLE_PERMISSION_NOT_DETERMINED => oxide_platform_api::PermissionStatus::NotDetermined,
        APPLE_PERMISSION_LIMITED => oxide_platform_api::PermissionStatus::Limited,
        APPLE_PERMISSION_AUTHORIZED => oxide_platform_api::PermissionStatus::Authorized,
        _ => oxide_platform_api::PermissionStatus::Denied,
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct AppleSecureStorage;

impl AppleSecureStorage {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    pub fn save_sync(&self, key: &str, data: &[u8]) -> Result<(), PlatformError> {
        let result = unsafe {
            oxide_secure_storage_save(key.as_ptr(), key.len(), data.as_ptr(), data.len())
        };
        match result {
            0 => Ok(()),
            code => {
                Err(PlatformError::Unknown(alloc::format!("secure storage save failed: {code}")))
            }
        }
    }

    pub fn load_sync(&self, key: &str) -> Result<Option<alloc::vec::Vec<u8>>, PlatformError> {
        let mut data_ptr: *const u8 = core::ptr::null();
        let mut data_len: usize = 0;
        let result = unsafe {
            oxide_secure_storage_load(key.as_ptr(), key.len(), &mut data_ptr, &mut data_len)
        };
        match result {
            0 => {
                if data_ptr.is_null() || data_len == 0 {
                    return Ok(Some(alloc::vec::Vec::new()));
                }
                let data = unsafe { core::slice::from_raw_parts(data_ptr, data_len).to_vec() };
                unsafe {
                    oxide_secure_storage_free_data(data_ptr, data_len);
                }
                Ok(Some(data))
            }
            1 => Ok(None),
            code => {
                Err(PlatformError::Unknown(alloc::format!("secure storage load failed: {code}")))
            }
        }
    }

    pub fn delete_sync(&self, key: &str) -> Result<(), PlatformError> {
        let result = unsafe { oxide_secure_storage_delete(key.as_ptr(), key.len()) };
        match result {
            0 | 1 => Ok(()),
            code => {
                Err(PlatformError::Unknown(alloc::format!("secure storage delete failed: {code}")))
            }
        }
    }
}

impl SecureStorage for AppleSecureStorage {
    fn save<'a>(
        &'a self,
        key: &'a str,
        data: &'a [u8],
    ) -> core::pin::Pin<
        alloc::boxed::Box<dyn core::future::Future<Output = Result<(), PlatformError>> + Send + 'a>,
    > {
        alloc::boxed::Box::pin(async move { self.save_sync(key, data) })
    }

    fn load<'a>(
        &'a self,
        key: &'a str,
    ) -> core::pin::Pin<
        alloc::boxed::Box<
            dyn core::future::Future<Output = Result<Option<alloc::vec::Vec<u8>>, PlatformError>>
                + Send
                + 'a,
        >,
    > {
        alloc::boxed::Box::pin(async move { self.load_sync(key) })
    }

    fn delete<'a>(
        &'a self,
        key: &'a str,
    ) -> core::pin::Pin<
        alloc::boxed::Box<dyn core::future::Future<Output = Result<(), PlatformError>> + Send + 'a>,
    > {
        alloc::boxed::Box::pin(async move { self.delete_sync(key) })
    }
}

impl secure_storage::SecureStorage for AppleSecureStorage {
    fn save<'a>(
        &'a self,
        key: &'a str,
        data: &'a [u8],
    ) -> core::pin::Pin<
        alloc::boxed::Box<dyn core::future::Future<Output = Result<(), PlatformError>> + Send + 'a>,
    > {
        alloc::boxed::Box::pin(async move { self.save_sync(key, data) })
    }

    fn load<'a>(
        &'a self,
        key: &'a str,
    ) -> core::pin::Pin<
        alloc::boxed::Box<
            dyn core::future::Future<Output = Result<Option<alloc::vec::Vec<u8>>, PlatformError>>
                + Send
                + 'a,
        >,
    > {
        alloc::boxed::Box::pin(async move { self.load_sync(key) })
    }

    fn delete<'a>(
        &'a self,
        key: &'a str,
    ) -> core::pin::Pin<
        alloc::boxed::Box<dyn core::future::Future<Output = Result<(), PlatformError>> + Send + 'a>,
    > {
        alloc::boxed::Box::pin(async move { self.delete_sync(key) })
    }
}
