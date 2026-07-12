use oxide_networking::{NetworkPathKind, ReachabilityState};
use oxide_platform_api::media_library::{
    AssetData, AssetId, AssetType, ImageQuality, MediaLibrary,
};
use oxide_platform_api::network_status::NetworkInterface;
#[cfg(feature = "web-view-macos")]
use oxide_platform_api::web_view::{WebViewEvent, WebViewService};
use oxide_platform_api::{
    AudioSample, BleUuid, Bluetooth, BluetoothEvent, CameraFrame, CameraImage, CameraManager,
    ConnectionEvent, ConnectionOptions, GattChar, GeoHash, GeoRegion, HttpClient, HttpEvent, HttpRequest,
    LocationEvent, LocationService, MotionService, Networking, PermissionDomain, PermissionStatus,
    PhotoEvent, PlatformError, ProtocolOptions, PushManager, QuicOptions, RecordingEvent,
    ScanOptions, TcpOptions, UdpEvent, UdpPacket,
};
use oxide_platform_apple::{
    network_status_from_apple_interface_mask, network_status_from_apple_path,
    oxide_location_error_trampoline, oxide_location_update_trampoline, oxide_motion_trampoline,
    oxide_push_token_trampoline, permission_domain_from_apple_code,
    permission_domain_to_apple_code, permission_status_from_apple_code,
    permission_status_to_apple_code, reachability_state_from_apple_path, APPLE_INTERFACE_CELLULAR,
    APPLE_INTERFACE_WIFI, APPLE_INTERFACE_WIRED, APPLE_PATH_KIND_CELLULAR, APPLE_PATH_KIND_OTHER,
    APPLE_PATH_KIND_WIFI, APPLE_PATH_KIND_WIRED, APPLE_PERMISSION_AUTHORIZED,
    APPLE_PERMISSION_CAMERA, APPLE_PERMISSION_DENIED, APPLE_PERMISSION_NOT_DETERMINED,
};
#[cfg(feature = "web-view-macos")]
use oxide_platform_apple::{oxide_web_view_event_trampoline, AppleWebViewService};
use oxide_platform_apple::{
    AppleBleScanConfig, AppleBleScanInfo, AppleBluetooth, AppleCamAudio, AppleCamFrame,
    AppleCamPhotoEvent, AppleCamRecordEvent, AppleCameraManager, AppleHttpClient, AppleHttpEvent,
    AppleHttpHeader, AppleLocationConfig, AppleLocationSample, AppleLocationService,
    AppleMediaAsset, AppleMediaImageData, AppleMediaLibraryManager, AppleMotionSample,
    AppleMotionService, ApplePushManager, AppleSecureStorage, AppleSocketNetworking,
};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

fn store() -> &'static Mutex<HashMap<Vec<u8>, Vec<u8>>> {
    static STORE: OnceLock<Mutex<HashMap<Vec<u8>, Vec<u8>>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn heap_bytes(bytes: &[u8]) -> (*mut u8, usize) {
    if bytes.is_empty() {
        return (std::ptr::null_mut(), 0);
    }
    let mut copy = bytes.to_vec();
    let ptr = copy.as_mut_ptr();
    let len = copy.len();
    std::mem::forget(copy);
    (ptr, len)
}

fn block_on_ready<F: Future>(future: F) -> F::Output {
    fn clone(_: *const ()) -> RawWaker {
        RawWaker::new(core::ptr::null(), &VTABLE)
    }
    fn wake(_: *const ()) {}
    fn wake_by_ref(_: *const ()) {}
    fn drop(_: *const ()) {}
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);

    let waker = unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) };
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    match Pin::new(&mut future).poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("test future unexpectedly pending"),
    }
}

fn location_callback_cell(
) -> &'static Mutex<Option<unsafe extern "C" fn(*const AppleLocationSample)>> {
    static CALLBACK: OnceLock<Mutex<Option<unsafe extern "C" fn(*const AppleLocationSample)>>> =
        OnceLock::new();
    CALLBACK.get_or_init(|| Mutex::new(None))
}

fn location_error_callback_cell() -> &'static Mutex<Option<unsafe extern "C" fn(*const u8, usize)>>
{
    static CALLBACK: OnceLock<Mutex<Option<unsafe extern "C" fn(*const u8, usize)>>> =
        OnceLock::new();
    CALLBACK.get_or_init(|| Mutex::new(None))
}

fn native_last_location_cell() -> &'static Mutex<Option<AppleLocationSample>> {
    static LAST: OnceLock<Mutex<Option<AppleLocationSample>>> = OnceLock::new();
    LAST.get_or_init(|| Mutex::new(None))
}

fn motion_callback_cell() -> &'static Mutex<Option<unsafe extern "C" fn(*const AppleMotionSample)>>
{
    static CALLBACK: OnceLock<Mutex<Option<unsafe extern "C" fn(*const AppleMotionSample)>>> =
        OnceLock::new();
    CALLBACK.get_or_init(|| Mutex::new(None))
}

fn apple_service_test_mutex() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

static NATIVE_MOTION_RUNNING: AtomicBool = AtomicBool::new(false);
static PUSH_REGISTER_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static PUSH_BADGE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
static PUSH_CLEAR_DELIVERED_CALLS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
static BLE_INIT_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static BLE_SCAN_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static BLE_STOP_SCAN_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static BLE_CONNECT_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static BLE_DISCONNECT_CALLS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
static BLE_WRITE_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static BLE_NOTIFY_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static BLE_ADVERTISE_START_CALLS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
static BLE_ADVERTISE_STOP_CALLS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
static CAMERA_START_DEFAULT_CALLS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
static CAMERA_START_PREVIEW_ONLY_CALLS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
static CAMERA_STOP_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static CAMERA_RECORD_START_CALLS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
static CAMERA_PHOTO_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static CAMERA_START_DEFAULT_RC: AtomicI32 = AtomicI32::new(0);
static CAMERA_START_PREVIEW_ONLY_RC: AtomicI32 = AtomicI32::new(0);
static CAMERA_FOCUS_RC: AtomicI32 = AtomicI32::new(0);
static CAMERA_ZOOM_RC: AtomicI32 = AtomicI32::new(0);
static CAMERA_FLASH_RC: AtomicI32 = AtomicI32::new(0);
static CAMERA_TORCH_RC: AtomicI32 = AtomicI32::new(0);
static CAMERA_RECORD_START_RC: AtomicI32 = AtomicI32::new(0);
static CAMERA_PHOTO_RC: AtomicI32 = AtomicI32::new(0);
#[cfg(feature = "web-view-macos")]
static WEB_VIEW_LAST_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn push_token_callback_cell() -> &'static Mutex<Option<unsafe extern "C" fn(u32, *const u8, usize)>>
{
    static CALLBACK: OnceLock<Mutex<Option<unsafe extern "C" fn(u32, *const u8, usize)>>> =
        OnceLock::new();
    CALLBACK.get_or_init(|| Mutex::new(None))
}

fn push_notify_callback_cell() -> &'static Mutex<Option<unsafe extern "C" fn(*const u8, usize)>> {
    static CALLBACK: OnceLock<Mutex<Option<unsafe extern "C" fn(*const u8, usize)>>> =
        OnceLock::new();
    CALLBACK.get_or_init(|| Mutex::new(None))
}

fn push_string_allocations() -> &'static Mutex<HashMap<usize, usize>> {
    static ALLOCS: OnceLock<Mutex<HashMap<usize, usize>>> = OnceLock::new();
    ALLOCS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn camera_frame_callback_cell() -> &'static Mutex<Option<unsafe extern "C" fn(*const AppleCamFrame)>>
{
    static CALLBACK: OnceLock<Mutex<Option<unsafe extern "C" fn(*const AppleCamFrame)>>> =
        OnceLock::new();
    CALLBACK.get_or_init(|| Mutex::new(None))
}

fn camera_audio_callback_cell() -> &'static Mutex<Option<unsafe extern "C" fn(*const AppleCamAudio)>>
{
    static CALLBACK: OnceLock<Mutex<Option<unsafe extern "C" fn(*const AppleCamAudio)>>> =
        OnceLock::new();
    CALLBACK.get_or_init(|| Mutex::new(None))
}

fn camera_record_callback_cell(
) -> &'static Mutex<Option<unsafe extern "C" fn(*const AppleCamRecordEvent)>> {
    static CALLBACK: OnceLock<Mutex<Option<unsafe extern "C" fn(*const AppleCamRecordEvent)>>> =
        OnceLock::new();
    CALLBACK.get_or_init(|| Mutex::new(None))
}

fn camera_photo_callback_cell(
) -> &'static Mutex<Option<unsafe extern "C" fn(*const AppleCamPhotoEvent)>> {
    static CALLBACK: OnceLock<Mutex<Option<unsafe extern "C" fn(*const AppleCamPhotoEvent)>>> =
        OnceLock::new();
    CALLBACK.get_or_init(|| Mutex::new(None))
}

#[cfg(feature = "web-view-macos")]
fn web_view_callback_cell(
) -> &'static Mutex<Option<unsafe extern "C" fn(u64, u32, *const u8, usize)>> {
    static CALLBACK: OnceLock<Mutex<Option<unsafe extern "C" fn(u64, u32, *const u8, usize)>>> =
        OnceLock::new();
    CALLBACK.get_or_init(|| Mutex::new(None))
}

#[cfg(feature = "web-view-macos")]
fn web_view_string_allocations() -> &'static Mutex<HashMap<usize, usize>> {
    static ALLOCS: OnceLock<Mutex<HashMap<usize, usize>>> = OnceLock::new();
    ALLOCS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(feature = "web-view-macos")]
fn web_view_closed_ids() -> &'static Mutex<Vec<u64>> {
    static CLOSED: OnceLock<Mutex<Vec<u64>>> = OnceLock::new();
    CLOSED.get_or_init(|| Mutex::new(Vec::new()))
}

#[no_mangle]
pub extern "C" fn oxide_host_http_start(
    url_ptr: *const u8,
    url_len: usize,
    _timeout_ms: u32,
    max_response_bytes: usize,
    _request_headers: *const AppleHttpHeader,
    _request_header_count: usize,
    _response_headers: *const AppleHttpHeader,
    _response_header_count: usize,
    callback: Option<unsafe extern "C" fn(*mut core::ffi::c_void, *const AppleHttpEvent)>,
    context: *mut core::ffi::c_void,
    out_request_id: *mut u64,
) -> i32 {
    if url_ptr.is_null() || url_len == 0 || callback.is_none() || context.is_null() || out_request_id.is_null() {
        return -1;
    }
    let url = unsafe { std::slice::from_raw_parts(url_ptr, url_len) };
    if max_response_bytes < 2 {
        return -4;
    }
    unsafe {
        *out_request_id = 1;
        let callback = callback.unwrap_unchecked();
        if url.ends_with(b"/ffi-null-count") {
            callback(context, &AppleHttpEvent {
                kind: 1,
                error: 0,
                status: 200,
                reserved: 0,
                content_length: 0,
                data_ptr: std::ptr::null(),
                data_len: 0,
                final_url_ptr: url.as_ptr(),
                final_url_len: url.len(),
                headers_ptr: std::ptr::null(),
                header_count: 1,
            });
            return 0;
        }
        if url.ends_with(b"/ffi-count-over") {
            callback(context, &AppleHttpEvent {
                kind: 1,
                error: 0,
                status: 200,
                reserved: 0,
                content_length: 0,
                data_ptr: std::ptr::null(),
                data_len: 0,
                final_url_ptr: url.as_ptr(),
                final_url_len: url.len(),
                headers_ptr: std::ptr::null(),
                header_count: 65,
            });
            return 0;
        }
        if url.ends_with(b"/ffi-url-over") {
            let oversized_url = vec![b'a'; 16 * 1024 + 1];
            callback(context, &AppleHttpEvent {
                kind: 1,
                error: 0,
                status: 200,
                reserved: 0,
                content_length: 0,
                data_ptr: std::ptr::null(),
                data_len: 0,
                final_url_ptr: oversized_url.as_ptr(),
                final_url_len: oversized_url.len(),
                headers_ptr: std::ptr::null(),
                header_count: 0,
            });
            return 0;
        }
        if url.ends_with(b"/ffi-metadata-over") {
            let oversized_value = vec![b'a'; 32 * 1024];
            let header = AppleHttpHeader {
                name_ptr: b"x".as_ptr(),
                name_len: 1,
                value_ptr: oversized_value.as_ptr(),
                value_len: oversized_value.len(),
            };
            callback(context, &AppleHttpEvent {
                kind: 1,
                error: 0,
                status: 200,
                reserved: 0,
                content_length: 0,
                data_ptr: std::ptr::null(),
                data_len: 0,
                final_url_ptr: url.as_ptr(),
                final_url_len: url.len(),
                headers_ptr: &header,
                header_count: 1,
            });
            return 0;
        }
        callback(context, &AppleHttpEvent {
            kind: 1,
            error: 0,
            status: 200,
            reserved: 0,
            content_length: 2,
            data_ptr: std::ptr::null(),
            data_len: 0,
            final_url_ptr: url.as_ptr(),
            final_url_len: url.len(),
            headers_ptr: std::ptr::null(),
            header_count: 0,
        });
        callback(context, &AppleHttpEvent {
            kind: 2,
            error: 0,
            status: 0,
            reserved: 0,
            content_length: -1,
            data_ptr: b"ok".as_ptr(),
            data_len: 2,
            final_url_ptr: std::ptr::null(),
            final_url_len: 0,
            headers_ptr: std::ptr::null(),
            header_count: 0,
        });
        callback(context, &AppleHttpEvent {
            kind: 3,
            error: 0,
            status: 0,
            reserved: 0,
            content_length: -1,
            data_ptr: std::ptr::null(),
            data_len: 0,
            final_url_ptr: std::ptr::null(),
            final_url_len: 0,
            headers_ptr: std::ptr::null(),
            header_count: 0,
        });
    }
    0
}

#[no_mangle]
pub extern "C" fn oxide_host_http_cancel(_request_id: u64) {}

#[no_mangle]
pub extern "C" fn oxide_ble_init() {
    BLE_INIT_CALLS.fetch_add(1, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn oxide_ble_init_with_restoration(_restore_id: *const core::ffi::c_char) {
    BLE_INIT_CALLS.fetch_add(1, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn oxide_ble_powered_on() -> u8 {
    1
}

#[no_mangle]
pub extern "C" fn oxide_ble_start_scan(cfg: *const AppleBleScanConfig) {
    assert!(!cfg.is_null());
    BLE_SCAN_CALLS.fetch_add(1, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn oxide_ble_stop_scan() {
    BLE_STOP_SCAN_CALLS.fetch_add(1, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn oxide_ble_connect(_id16: *const u8) {
    BLE_CONNECT_CALLS.fetch_add(1, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn oxide_ble_disconnect(_id16: *const u8) {
    BLE_DISCONNECT_CALLS.fetch_add(1, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn oxide_ble_read(
    _id16: *const u8,
    _svc16: *const u8,
    _chr16: *const u8,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
    _timeout_ms: u32,
) -> i32 {
    if out_ptr.is_null() || out_len.is_null() {
        return 0;
    }
    let (ptr, len) = heap_bytes(b"ble-read");
    push_string_allocations().lock().expect("push string allocations").insert(ptr as usize, len);
    unsafe {
        *out_ptr = ptr;
        *out_len = len;
    }
    1
}

#[no_mangle]
pub extern "C" fn oxide_ble_write(
    _id16: *const u8,
    _svc16: *const u8,
    _chr16: *const u8,
    _data: *const u8,
    _len: usize,
    _with_response: u8,
    _timeout_ms: u32,
) -> i32 {
    BLE_WRITE_CALLS.fetch_add(1, Ordering::SeqCst);
    1
}

#[no_mangle]
pub extern "C" fn oxide_ble_notify(
    _id16: *const u8,
    _svc16: *const u8,
    _chr16: *const u8,
    _enable: u8,
    _timeout_ms: u32,
) -> i32 {
    BLE_NOTIFY_CALLS.fetch_add(1, Ordering::SeqCst);
    1
}

#[no_mangle]
pub extern "C" fn oxide_ble_advertise_start(
    _name: *const core::ffi::c_char,
    _service_uuid: *const u8,
) {
    BLE_ADVERTISE_START_CALLS.fetch_add(1, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn oxide_ble_advertise_stop() {
    BLE_ADVERTISE_STOP_CALLS.fetch_add(1, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn oxide_host_set_camera_callback(
    cb: Option<unsafe extern "C" fn(*const AppleCamFrame)>,
) {
    *camera_frame_callback_cell().lock().expect("camera frame callback cell") = cb;
}

#[no_mangle]
pub extern "C" fn oxide_host_set_camera_audio_callback(
    cb: Option<unsafe extern "C" fn(*const AppleCamAudio)>,
) {
    *camera_audio_callback_cell().lock().expect("camera audio callback cell") = cb;
}

#[no_mangle]
pub extern "C" fn oxide_host_set_camera_record_callback(
    cb: Option<unsafe extern "C" fn(*const AppleCamRecordEvent)>,
) {
    *camera_record_callback_cell().lock().expect("camera record callback cell") = cb;
}

#[no_mangle]
pub extern "C" fn oxide_host_set_camera_photo_callback(
    cb: Option<unsafe extern "C" fn(*const AppleCamPhotoEvent)>,
) {
    *camera_photo_callback_cell().lock().expect("camera photo callback cell") = cb;
}

#[no_mangle]
pub extern "C" fn oxide_cam_start_default() -> i32 {
    CAMERA_START_DEFAULT_CALLS.fetch_add(1, Ordering::SeqCst);
    CAMERA_START_DEFAULT_RC.load(Ordering::SeqCst)
}

#[no_mangle]
pub extern "C" fn oxide_cam_start_default_preview_only() -> i32 {
    CAMERA_START_PREVIEW_ONLY_CALLS.fetch_add(1, Ordering::SeqCst);
    CAMERA_START_PREVIEW_ONLY_RC.load(Ordering::SeqCst)
}

#[no_mangle]
pub extern "C" fn oxide_cam_stop() {
    CAMERA_STOP_CALLS.fetch_add(1, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn oxide_cam_set_fps(_fps: i32) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn oxide_cam_set_resolution_height(_height: i32) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn oxide_cam_set_bit_depth(_bits: i32) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn oxide_cam_set_color_space(_id: i32) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn oxide_cam_set_position(_pos: i32) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn oxide_cam_set_mode(_mode: i32) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn oxide_cam_set_focus_point(_x: f32, _y: f32) -> i32 {
    CAMERA_FOCUS_RC.load(Ordering::SeqCst)
}

#[no_mangle]
pub extern "C" fn oxide_cam_set_zoom_factor(_factor: f32) -> i32 {
    CAMERA_ZOOM_RC.load(Ordering::SeqCst)
}

#[no_mangle]
pub extern "C" fn oxide_cam_set_flash_mode(_mode: i32) -> i32 {
    CAMERA_FLASH_RC.load(Ordering::SeqCst)
}

#[no_mangle]
pub extern "C" fn oxide_cam_set_torch_mode(_mode: i32, _level: f32) -> i32 {
    CAMERA_TORCH_RC.load(Ordering::SeqCst)
}

#[no_mangle]
pub extern "C" fn oxide_cam_capture_photo(_high_speed_from_preview: u8, _flash_mode: i32) -> i32 {
    CAMERA_PHOTO_CALLS.fetch_add(1, Ordering::SeqCst);
    CAMERA_PHOTO_RC.load(Ordering::SeqCst)
}

#[no_mangle]
pub extern "C" fn oxide_cam_set_audio_session_mode(_mode: i32) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn oxide_cam_record_start(
    _dest_ptr: *const u8,
    _dest_len: usize,
    _container: i32,
    _include_audio: u8,
) -> i32 {
    CAMERA_RECORD_START_CALLS.fetch_add(1, Ordering::SeqCst);
    CAMERA_RECORD_START_RC.load(Ordering::SeqCst)
}

#[no_mangle]
pub extern "C" fn oxide_cam_record_stop() -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn oxide_cam_record_cancel() -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn oxide_host_set_camera_running(_on: u8) -> i32 {
    0
}

fn reset_camera_return_codes() {
    CAMERA_START_DEFAULT_RC.store(0, Ordering::SeqCst);
    CAMERA_START_PREVIEW_ONLY_RC.store(0, Ordering::SeqCst);
    CAMERA_FOCUS_RC.store(0, Ordering::SeqCst);
    CAMERA_ZOOM_RC.store(0, Ordering::SeqCst);
    CAMERA_FLASH_RC.store(0, Ordering::SeqCst);
    CAMERA_TORCH_RC.store(0, Ordering::SeqCst);
    CAMERA_RECORD_START_RC.store(0, Ordering::SeqCst);
    CAMERA_PHOTO_RC.store(0, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn oxide_secure_storage_save(
    key_ptr: *const u8,
    key_len: usize,
    data_ptr: *const u8,
    data_len: usize,
) -> i32 {
    if key_ptr.is_null() || key_len == 0 || (data_ptr.is_null() && data_len > 0) {
        return -1;
    }
    let key = unsafe { std::slice::from_raw_parts(key_ptr, key_len).to_vec() };
    let data = if data_len == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(data_ptr, data_len).to_vec() }
    };
    store().lock().expect("secure-storage test store").insert(key, data);
    0
}

#[no_mangle]
pub extern "C" fn oxide_secure_storage_load(
    key_ptr: *const u8,
    key_len: usize,
    out_data_ptr: *mut *const u8,
    out_data_len: *mut usize,
) -> i32 {
    if key_ptr.is_null() || key_len == 0 || out_data_ptr.is_null() || out_data_len.is_null() {
        return -1;
    }
    unsafe {
        *out_data_ptr = std::ptr::null();
        *out_data_len = 0;
    }
    let key = unsafe { std::slice::from_raw_parts(key_ptr, key_len).to_vec() };
    let Some(data) = store().lock().expect("secure-storage test store").get(&key).cloned() else {
        return 1;
    };
    if data.is_empty() {
        return 0;
    }
    let mut copy = data;
    let ptr = copy.as_mut_ptr();
    let len = copy.len();
    std::mem::forget(copy);
    unsafe {
        *out_data_ptr = ptr;
        *out_data_len = len;
    }
    0
}

#[no_mangle]
pub extern "C" fn oxide_secure_storage_delete(key_ptr: *const u8, key_len: usize) -> i32 {
    if key_ptr.is_null() || key_len == 0 {
        return -1;
    }
    let key = unsafe { std::slice::from_raw_parts(key_ptr, key_len).to_vec() };
    if store().lock().expect("secure-storage test store").remove(&key).is_some() {
        0
    } else {
        1
    }
}

#[no_mangle]
pub extern "C" fn oxide_secure_storage_free_data(data_ptr: *const u8, data_len: usize) {
    if !data_ptr.is_null() {
        unsafe {
            drop(Vec::from_raw_parts(data_ptr.cast_mut(), data_len, data_len));
        }
    }
}

#[no_mangle]
pub extern "C" fn oxide_host_set_location_callback(
    cb: Option<unsafe extern "C" fn(*const AppleLocationSample)>,
) {
    *location_callback_cell().lock().expect("location callback cell") = cb;
}

#[no_mangle]
pub extern "C" fn oxide_host_set_location_error_callback(
    cb: Option<unsafe extern "C" fn(*const u8, usize)>,
) {
    *location_error_callback_cell().lock().expect("location error callback cell") = cb;
}

#[no_mangle]
pub extern "C" fn oxide_host_location_start(_cfg: AppleLocationConfig) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn oxide_host_location_stop() {}

#[no_mangle]
pub extern "C" fn oxide_host_location_request_once() {}

#[no_mangle]
pub extern "C" fn oxide_host_location_last(out: *mut AppleLocationSample) -> u8 {
    if out.is_null() {
        return 0;
    }
    let Some(sample) = *native_last_location_cell().lock().expect("native last location") else {
        return 0;
    };
    unsafe {
        *out = sample;
    }
    1
}

#[no_mangle]
pub extern "C" fn oxide_host_location_set_accuracy(_accuracy_kind: u32) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn oxide_host_set_motion_callback(
    cb: Option<unsafe extern "C" fn(*const AppleMotionSample)>,
) {
    *motion_callback_cell().lock().expect("motion callback cell") = cb;
}

#[no_mangle]
pub extern "C" fn oxide_host_motion_start() -> i32 {
    NATIVE_MOTION_RUNNING.store(true, Ordering::SeqCst);
    0
}

#[no_mangle]
pub extern "C" fn oxide_host_motion_stop() {
    NATIVE_MOTION_RUNNING.store(false, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn oxide_host_motion_is_active() -> u8 {
    if NATIVE_MOTION_RUNNING.load(Ordering::SeqCst) {
        1
    } else {
        0
    }
}

#[no_mangle]
pub extern "C" fn oxide_host_push_register() {
    PUSH_REGISTER_CALLS.fetch_add(1, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn oxide_host_push_get_device_token(
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    if out_ptr.is_null() || out_len.is_null() {
        return 0;
    }
    let (ptr, len) = heap_bytes(b"native-token");
    push_string_allocations().lock().expect("push string allocations").insert(ptr as usize, len);
    unsafe {
        *out_ptr = ptr;
        *out_len = len;
    }
    1
}

#[no_mangle]
pub extern "C" fn oxide_host_push_set_badge(count: i32) {
    PUSH_BADGE.store(count, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn oxide_host_push_clear_badge() {
    PUSH_BADGE.store(0, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn oxide_host_push_clear_all_delivered() {
    PUSH_CLEAR_DELIVERED_CALLS.fetch_add(1, Ordering::SeqCst);
    PUSH_BADGE.store(0, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn oxide_host_set_push_token_callback(
    cb: Option<unsafe extern "C" fn(u32, *const u8, usize)>,
) {
    *push_token_callback_cell().lock().expect("push token callback cell") = cb;
}

#[no_mangle]
pub extern "C" fn oxide_host_set_push_notify_callback(
    cb: Option<unsafe extern "C" fn(*const u8, usize)>,
) {
    *push_notify_callback_cell().lock().expect("push notify callback cell") = cb;
}

#[no_mangle]
pub extern "C" fn oxide_host_string_free(p: *mut u8) {
    if !p.is_null() {
        let len = push_string_allocations()
            .lock()
            .expect("push string allocations")
            .remove(&(p as usize));
        if let Some(len) = len {
            unsafe {
                drop(Vec::from_raw_parts(p, len, len));
            }
        }
    }
}

#[cfg(feature = "web-view-macos")]
#[no_mangle]
pub extern "C" fn oxide_web_view_set_event_callback(
    cb: Option<unsafe extern "C" fn(u64, u32, *const u8, usize)>,
) {
    *web_view_callback_cell().lock().expect("web view callback cell") = cb;
}

#[cfg(feature = "web-view-macos")]
#[no_mangle]
pub extern "C" fn oxide_web_view_create(url_ptr: *const u8, url_len: usize, id: u64) -> i32 {
    if url_ptr.is_null() || url_len == 0 || id == 0 {
        return -1;
    }
    let url = unsafe { std::slice::from_raw_parts(url_ptr, url_len) };
    if url == b"webview-unavailable" {
        return -2;
    }
    if url == b"webview-busy" {
        return -3;
    }
    WEB_VIEW_LAST_ID.store(id, Ordering::SeqCst);
    0
}

#[cfg(feature = "web-view-macos")]
#[no_mangle]
pub extern "C" fn oxide_web_view_execute_script(
    id: u64,
    script_ptr: *const u8,
    script_len: usize,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    if id == 0 || script_ptr.is_null() || script_len == 0 || out_ptr.is_null() || out_len.is_null()
    {
        return -1;
    }
    unsafe {
        *out_ptr = std::ptr::null_mut();
        *out_len = 0;
    }
    let script = unsafe { std::slice::from_raw_parts(script_ptr, script_len) };
    if script == b"undefined" {
        return 0;
    }
    if script == b"empty" {
        return 1;
    }
    if script == b"missing" {
        return -4;
    }
    if script == b"fail" {
        return -5;
    }
    if script == b"copy-fail" {
        return -6;
    }
    let mut result = b"script:".to_vec();
    result.extend_from_slice(script);
    let (ptr, len) = heap_bytes(&result);
    web_view_string_allocations()
        .lock()
        .expect("web view string allocations")
        .insert(ptr as usize, len);
    unsafe {
        *out_ptr = ptr;
        *out_len = len;
    }
    1
}

#[cfg(feature = "web-view-macos")]
#[no_mangle]
pub extern "C" fn oxide_web_view_close(id: u64) {
    web_view_closed_ids().lock().expect("web view closed ids").push(id);
}

#[cfg(feature = "web-view-macos")]
#[no_mangle]
pub extern "C" fn oxide_web_view_free_string(p: *mut u8) {
    if p.is_null() {
        return;
    }
    let len = web_view_string_allocations()
        .lock()
        .expect("web view string allocations")
        .remove(&(p as usize));
    if let Some(len) = len {
        unsafe {
            drop(Vec::from_raw_parts(p, len, len));
        }
    }
}

#[no_mangle]
pub extern "C" fn oxide_media_fetch_assets(
    media_type_mask: u8,
    limit: i32,
    _ascending: u8,
    out_assets: *mut *const AppleMediaAsset,
    out_count: *mut usize,
) -> i32 {
    if out_assets.is_null() || out_count.is_null() || limit < 0 {
        return -3;
    }

    let mut rows = Vec::new();
    if media_type_mask & 1 != 0 {
        rows.push(("image-a", 0_u8, 0.0, 640_u32, 480_u32));
        rows.push(("image-b", 0_u8, 0.0, 320_u32, 240_u32));
    }
    if media_type_mask & 2 != 0 {
        rows.push(("video-a", 1_u8, 1.5, 1920_u32, 1080_u32));
    }

    let keep = if limit == 0 { rows.len() } else { rows.len().min(limit as usize) };
    let mut assets = Vec::with_capacity(keep);
    for (identifier, media_type, duration_sec, width, height) in rows.into_iter().take(keep) {
        let (identifier_ptr, identifier_len) = heap_bytes(identifier.as_bytes());
        assets.push(AppleMediaAsset {
            identifier_ptr,
            identifier_len,
            media_type,
            creation_date: 0,
            duration_sec,
            width,
            height,
            file_size: 0,
        });
    }

    let count = assets.len();
    let ptr = assets.as_ptr();
    std::mem::forget(assets);
    unsafe {
        *out_assets = ptr;
        *out_count = count;
    }
    0
}

#[no_mangle]
pub extern "C" fn oxide_media_free_assets(assets: *const AppleMediaAsset, count: usize) {
    if assets.is_null() {
        return;
    }
    unsafe {
        for idx in 0..count {
            let asset = &*assets.add(idx);
            if !asset.identifier_ptr.is_null() {
                drop(Vec::from_raw_parts(
                    asset.identifier_ptr.cast_mut(),
                    asset.identifier_len,
                    asset.identifier_len,
                ));
            }
        }
        drop(Vec::from_raw_parts(assets.cast_mut(), count, count));
    }
}

fn fill_media_image(
    out_image: *mut AppleMediaImageData,
    bytes: &[u8],
    width: u32,
    height: u32,
) -> i32 {
    if out_image.is_null() {
        return -3;
    }
    let (data_ptr, data_len) = heap_bytes(bytes);
    unsafe {
        (*out_image).data_ptr = data_ptr;
        (*out_image).data_len = data_len;
        (*out_image).width = width;
        (*out_image).height = height;
        (*out_image).row_bytes = if height == 0 { 0 } else { data_len / height as usize };
    }
    0
}

fn media_identifier(identifier_ptr: *const u8, identifier_len: usize) -> String {
    if identifier_ptr.is_null() || identifier_len == 0 {
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(identifier_ptr, identifier_len) };
    String::from_utf8_lossy(bytes).into_owned()
}

#[no_mangle]
pub extern "C" fn oxide_media_load_thumbnail(
    identifier_ptr: *const u8,
    identifier_len: usize,
    _size: u8,
    out_image: *mut AppleMediaImageData,
) -> i32 {
    match media_identifier(identifier_ptr, identifier_len).as_str() {
        "missing-image" => return 1,
        "invalid-image" => return -3,
        "io-image" => return -2,
        _ => {}
    }
    fill_media_image(out_image, b"thumb-jpeg", 2, 2)
}

#[no_mangle]
pub extern "C" fn oxide_media_load_thumbnail_rgba(
    identifier_ptr: *const u8,
    identifier_len: usize,
    _size: u8,
    out_image: *mut AppleMediaImageData,
) -> i32 {
    match media_identifier(identifier_ptr, identifier_len).as_str() {
        "missing-image" => return 1,
        "invalid-image" => return -3,
        "io-image" => return -2,
        _ => {}
    }
    fill_media_image(out_image, &[1, 2, 3, 4], 1, 1)
}

#[no_mangle]
pub extern "C" fn oxide_media_load_full_image(
    identifier_ptr: *const u8,
    identifier_len: usize,
    out_image: *mut AppleMediaImageData,
) -> i32 {
    match media_identifier(identifier_ptr, identifier_len).as_str() {
        "missing-image" => return 1,
        "invalid-image" => return -3,
        "io-image" => return -2,
        _ => {}
    }
    fill_media_image(out_image, b"full-jpeg", 4, 4)
}

#[no_mangle]
pub extern "C" fn oxide_media_load_full_image_rgba(
    identifier_ptr: *const u8,
    identifier_len: usize,
    out_image: *mut AppleMediaImageData,
) -> i32 {
    match media_identifier(identifier_ptr, identifier_len).as_str() {
        "missing-image" => return 1,
        "invalid-image" => return -3,
        "io-image" => return -2,
        _ => {}
    }
    fill_media_image(out_image, &[5, 6, 7, 8], 1, 1)
}

#[no_mangle]
pub extern "C" fn oxide_media_free_image_data(data_ptr: *const u8, data_len: usize) {
    if !data_ptr.is_null() {
        unsafe {
            drop(Vec::from_raw_parts(data_ptr.cast_mut(), data_len, data_len));
        }
    }
}

#[no_mangle]
pub extern "C" fn oxide_media_load_video_file(
    identifier_ptr: *const u8,
    identifier_len: usize,
    out_path_ptr: *mut *const u8,
    out_path_len: *mut usize,
) -> i32 {
    if out_path_ptr.is_null() || out_path_len.is_null() {
        return -3;
    }
    match media_identifier(identifier_ptr, identifier_len).as_str() {
        "missing-video" => return 1,
        "invalid-video" => return -3,
        "io-video" => return -2,
        "unsupported-video" => return -4,
        _ => {}
    }
    let (path_ptr, path_len) = heap_bytes(b"/tmp/oxide-video.mov");
    unsafe {
        *out_path_ptr = path_ptr;
        *out_path_len = path_len;
    }
    0
}

#[no_mangle]
pub extern "C" fn oxide_media_free_string(data_ptr: *const u8, data_len: usize) {
    if !data_ptr.is_null() {
        unsafe {
            drop(Vec::from_raw_parts(data_ptr.cast_mut(), data_len, data_len));
        }
    }
}

#[test]
fn apple_secure_storage_round_trips_c_abi() {
    store().lock().expect("secure-storage test store").clear();
    let storage = AppleSecureStorage::new();

    assert_eq!(storage.load_sync("token").expect("load missing"), None);
    storage.save_sync("token", b"abc").expect("save token");
    assert_eq!(storage.load_sync("token").expect("load token"), Some(b"abc".to_vec()));

    storage.save_sync("empty", b"").expect("save empty");
    assert_eq!(storage.load_sync("empty").expect("load empty"), Some(Vec::new()));

    storage.delete_sync("token").expect("delete token");
    assert_eq!(storage.load_sync("token").expect("load deleted"), None);
    storage.delete_sync("token").expect("delete missing is idempotent");
}

#[test]
fn apple_http_client_streams_through_c_abi() {
    let client = AppleHttpClient::new();
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let _operation = client.start(
        HttpRequest::get("https://oxide.test/health"),
        Box::new(move |event| sink.lock().expect("HTTP events").push(event)),
    ).expect("HTTP start through test ABI");
    let events = events.lock().expect("HTTP events");
    assert_eq!(events.len(), 3);
    assert!(matches!(&events[0], HttpEvent::Response(response)
        if response.status == 200 && response.final_url == "https://oxide.test/health"));
    assert_eq!(events[1], HttpEvent::Body(b"ok".to_vec()));
    assert_eq!(events[2], HttpEvent::Complete);
}

#[test]
fn apple_http_request_metadata_accepts_exact_bounds_and_rejects_overflow() {
    let client = AppleHttpClient::new();
    let prefix = "https://oxide.test/";
    let exact_url = format!("{prefix}{}", "a".repeat(16 * 1024 - prefix.len()));
    let exact = HttpRequest::get(exact_url).with_header("x", "a".repeat(32 * 1024 - 1));
    assert!(client.start(exact, Box::new(|_| {})).is_ok());

    let oversized_url = format!("{prefix}{}", "a".repeat(16 * 1024 + 1 - prefix.len()));
    assert!(matches!(
        client.start(HttpRequest::get(oversized_url), Box::new(|_| {})),
        Err(PlatformError::Invalid("HTTP URL exceeds limit"))
    ));
    let oversized_metadata = HttpRequest::get("https://oxide.test/metadata")
        .with_header("x", "a".repeat(32 * 1024));
    assert!(matches!(
        client.start(oversized_metadata, Box::new(|_| {})),
        Err(PlatformError::Invalid("HTTP metadata bytes exceed limit"))
    ));
}

#[test]
fn apple_http_request_header_count_accepts_exact_limit_and_rejects_limit_plus_one() {
    let client = AppleHttpClient::new();
    let mut exact = HttpRequest::get("https://oxide.test/count");
    exact.response_headers = (0..64).map(|index| format!("x-{index}")).collect();
    assert!(client.start(exact, Box::new(|_| {})).is_ok());

    let mut oversized = HttpRequest::get("https://oxide.test/count");
    oversized.response_headers = (0..65).map(|index| format!("x-{index}")).collect();
    assert!(matches!(
        client.start(oversized, Box::new(|_| {})),
        Err(PlatformError::Invalid("HTTP header count exceeds limit"))
    ));
}

#[test]
fn apple_http_response_ffi_rejects_inconsistent_and_oversized_metadata() {
    let client = AppleHttpClient::new();
    for path in ["ffi-null-count", "ffi-count-over", "ffi-url-over", "ffi-metadata-over"] {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        let _operation = client.start(
            HttpRequest::get(format!("https://oxide.test/{path}")),
            Box::new(move |event| sink.lock().expect("HTTP events").push(event)),
        ).expect("HTTP start through malformed test ABI");
        let events = events.lock().expect("HTTP events");
        assert_eq!(events.len(), 1, "unexpected event count for {path}");
        assert!(matches!(events[0], HttpEvent::Failed(PlatformError::Invalid(
            "native HTTP event bounds are invalid"
        ))), "malformed FFI event was accepted for {path}");
    }
}

#[test]
fn apple_media_library_queries_assets_with_paging() {
    let library = AppleMediaLibraryManager;
    let assets = block_on_ready(library.query_assets(AssetType::Image, 1, 1))
        .expect("media query through test ABI");

    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0].id.0, "image-b");
    assert_eq!(assets[0].asset_type, AssetType::Image);
    assert_eq!(assets[0].width, 320);
    assert_eq!(assets[0].height, 240);
}

#[test]
fn apple_media_library_loads_image_and_video_data() {
    let library = AppleMediaLibraryManager;
    let image = block_on_ready(
        library.request_image_data(&AssetId(String::from("image-a")), ImageQuality::Display),
    )
    .expect("image data through test ABI");
    assert!(matches!(image, AssetData::Image { data, .. } if data == b"full-jpeg"));

    let raw = library
        .load_image_bgra_data(&AssetId(String::from("image-a")), ImageQuality::Thumbnail)
        .expect("raw thumbnail through test ABI");
    assert_eq!(raw.bgra, vec![1, 2, 3, 4]);

    let video = block_on_ready(library.request_video_data(&AssetId(String::from("video-a"))))
        .expect("video data through test ABI");
    assert!(matches!(video, AssetData::Video { file_path } if file_path == "/tmp/oxide-video.mov"));
}

#[test]
fn apple_media_library_maps_host_return_codes_to_platform_errors() {
    let library = AppleMediaLibraryManager;

    let missing_image = block_on_ready(
        library.request_image_data(&AssetId(String::from("missing-image")), ImageQuality::Display),
    )
    .expect_err("missing image should fail");
    assert!(matches!(missing_image, PlatformError::NotFound("media image data")));

    let invalid_image = block_on_ready(
        library.request_image_data(&AssetId(String::from("invalid-image")), ImageQuality::Display),
    )
    .expect_err("invalid image request should fail");
    assert!(matches!(invalid_image, PlatformError::Invalid("media image request failed")));

    let io_image = library
        .load_image_bgra_data(&AssetId(String::from("io-image")), ImageQuality::Thumbnail)
        .expect_err("image I/O should fail");
    assert!(
        matches!(io_image, PlatformError::Io(message) if message == "media image rgba request failed")
    );

    let missing_video =
        block_on_ready(library.request_video_data(&AssetId(String::from("missing-video"))))
            .expect_err("missing video should fail");
    assert!(matches!(missing_video, PlatformError::NotFound("media video file")));

    let invalid_video =
        block_on_ready(library.request_video_data(&AssetId(String::from("invalid-video"))))
            .expect_err("invalid video request should fail");
    assert!(matches!(invalid_video, PlatformError::Invalid("media video request failed")));

    let unsupported_video =
        block_on_ready(library.request_video_data(&AssetId(String::from("unsupported-video"))))
            .expect_err("unsupported video request should fail");
    assert!(matches!(unsupported_video, PlatformError::Unsupported("media video request failed")));
}

#[test]
fn apple_push_manager_registers_caches_token_and_fans_out_notifications() {
    PUSH_REGISTER_CALLS.store(0, Ordering::SeqCst);
    let manager = ApplePushManager::new();
    manager.register();

    assert_eq!(PUSH_REGISTER_CALLS.load(Ordering::SeqCst), 1);

    let token_cb = *push_token_callback_cell().lock().expect("push token callback cell");
    let token_cb = token_cb.expect("push token callback installed");
    unsafe {
        token_cb(0, b"callback-token".as_ptr(), b"callback-token".len());
    }
    assert_eq!(manager.device_token().expect("push token").value, "callback-token");

    let notifications = Arc::new(Mutex::new(Vec::new()));
    let sink = notifications.clone();
    manager.subscribe(Box::new(move |notification| {
        sink.lock().expect("push notifications").push(notification);
    }));

    let notify_cb = *push_notify_callback_cell().lock().expect("push notify callback cell");
    let notify_cb = notify_cb.expect("push notify callback installed");
    unsafe {
        notify_cb(
            br#"{"message":"hello","count":3}"#.as_ptr(),
            br#"{"message":"hello","count":3}"#.len(),
        );
    }

    let notifications = notifications.lock().expect("push notifications");
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].user_info.get("message").map(String::as_str), Some("hello"));
    assert_eq!(notifications[0].user_info.get("count").map(String::as_str), Some("3"));
}

#[test]
fn apple_push_manager_uses_host_token_and_badge_abi() {
    let manager = ApplePushManager::new();
    unsafe {
        oxide_push_token_trampoline(0, core::ptr::null(), 0);
    }
    assert_eq!(manager.device_token().expect("native push token").value, "native-token");

    manager.set_badge(7);
    assert_eq!(PUSH_BADGE.load(Ordering::SeqCst), 7);
    manager.clear_badge();
    assert_eq!(PUSH_BADGE.load(Ordering::SeqCst), 0);
    manager.set_badge(4);
    manager.clear_all_delivered();
    assert_eq!(PUSH_CLEAR_DELIVERED_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(PUSH_BADGE.load(Ordering::SeqCst), 0);
}

#[test]
fn apple_socket_networking_connects_tcp_and_reads() {
    assert_apple_socket_tcp_echo(TcpOptions::default());
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
#[test]
fn apple_socket_networking_configures_tcp_keepalive_and_reads() {
    let mut tcp = TcpOptions::default();
    tcp.keepalive = true;
    tcp.keepalive_idle_time_secs = 30;
    assert_apple_socket_tcp_echo(tcp);
}

fn assert_apple_socket_tcp_echo(tcp: TcpOptions) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("TCP listener");
    let port = listener.local_addr().expect("TCP listener addr").port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("TCP accept");
        let mut buf = [0_u8; 4];
        std::io::Read::read_exact(&mut stream, &mut buf).expect("TCP read");
        assert_eq!(&buf, b"ping");
        std::io::Write::write_all(&mut stream, b"pong").expect("TCP write");
    });

    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let networking = AppleSocketNetworking::new();
    let connection = networking
        .connect_tcp(
            ConnectionOptions {
                host: String::from("127.0.0.1"),
                port,
                protocol: ProtocolOptions::Tcp(tcp),
                tls_options: None,
            },
            Box::new(move |event| {
                sink.lock().expect("TCP events").push(event);
            }),
        )
        .expect("TCP connect");

    block_on_ready(connection.write(b"ping")).expect("TCP write through connection");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let has_pong = events
            .lock()
            .expect("TCP events")
            .iter()
            .any(|event| matches!(event, ConnectionEvent::Read(bytes) if bytes == b"pong"));
        if has_pong {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "timed out waiting for TCP read");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    connection.close();
    server.join().expect("TCP server");
}

#[test]
fn apple_socket_networking_binds_udp_sends_and_reads() {
    let echo = std::net::UdpSocket::bind("127.0.0.1:0").expect("UDP echo bind");
    let echo_port = echo.local_addr().expect("UDP echo addr").port();
    let server = std::thread::spawn(move || {
        let mut buf = [0_u8; 64];
        let (n, peer) = echo.recv_from(&mut buf).expect("UDP echo read");
        assert_eq!(&buf[..n], b"ping");
        echo.send_to(b"pong", peer).expect("UDP echo write");
    });

    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let socket = AppleSocketNetworking::new()
        .bind_udp(
            0,
            Box::new(move |event| {
                sink.lock().expect("UDP events").push(event);
            }),
        )
        .expect("UDP bind");
    socket
        .send(&UdpPacket {
            host: String::from("127.0.0.1"),
            port: echo_port,
            data: b"ping".to_vec(),
        })
        .expect("UDP send");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let has_pong = events
            .lock()
            .expect("UDP events")
            .iter()
            .any(|event| matches!(event, UdpEvent::Read(packet) if packet.data == b"pong"));
        if has_pong {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "timed out waiting for UDP read");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    socket.close();
    server.join().expect("UDP server");
}

#[test]
fn apple_socket_networking_rejects_unsupported_transport_options() {
    let networking = AppleSocketNetworking::new();
    let tls_result = networking.connect_tcp(
        ConnectionOptions {
            host: String::from("127.0.0.1"),
            port: 443,
            protocol: ProtocolOptions::Tcp(TcpOptions::default()),
            tls_options: Some(oxide_platform_api::TlsOptions {
                client_identity: None,
                pinned_public_keys: Vec::new(),
            }),
        },
        Box::new(|_| {}),
    );
    assert!(matches!(tls_result, Err(PlatformError::Unsupported(_))));

    let quic_result = networking.connect_quic(
        ConnectionOptions {
            host: String::from("127.0.0.1"),
            port: 443,
            protocol: ProtocolOptions::Quic(QuicOptions { alpn: String::from("h3") }),
            tls_options: None,
        },
        Box::new(|_| {}),
    );
    assert!(matches!(quic_result, Err(PlatformError::Unsupported(_))));
}

#[test]
fn apple_bluetooth_forwards_controls_and_read_write_notify() {
    let _guard = apple_service_test_mutex().lock().expect("Apple service test lock");
    BLE_SCAN_CALLS.store(0, Ordering::SeqCst);
    BLE_STOP_SCAN_CALLS.store(0, Ordering::SeqCst);
    BLE_CONNECT_CALLS.store(0, Ordering::SeqCst);
    BLE_DISCONNECT_CALLS.store(0, Ordering::SeqCst);
    BLE_WRITE_CALLS.store(0, Ordering::SeqCst);
    BLE_NOTIFY_CALLS.store(0, Ordering::SeqCst);
    BLE_ADVERTISE_START_CALLS.store(0, Ordering::SeqCst);
    BLE_ADVERTISE_STOP_CALLS.store(0, Ordering::SeqCst);

    let bluetooth = AppleBluetooth::new();
    assert!(bluetooth.powered_on());

    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    bluetooth.subscribe_events(Box::new(move |event| {
        sink.lock().expect("BLE events").push(event);
    }));

    oxide_platform_apple::oxide_host_ble_emit_state(1);
    assert!(events
        .lock()
        .expect("BLE events")
        .iter()
        .any(|event| { matches!(event, BluetoothEvent::StateChanged { powered_on: true }) }));

    let id = 0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10_u128;
    let service = BleUuid([0x21; 16]);
    let characteristic = BleUuid([0x22; 16]);
    let chr = GattChar { service, characteristic };

    bluetooth.start_scan(&ScanOptions { services: vec![service], allow_duplicates: true });
    bluetooth.stop_scan();
    bluetooth.connect(id);
    bluetooth.disconnect(id);

    assert_eq!(bluetooth.read(id, chr.clone()).expect("BLE read"), b"ble-read");
    bluetooth.write(id, chr.clone(), b"ble-write", true).expect("BLE write");
    bluetooth.notify(id, chr.clone(), true).expect("BLE notify");
    bluetooth.advertise_start("oxide-ble-test", &[service]);
    bluetooth.advertise_stop();

    assert_eq!(BLE_SCAN_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(BLE_STOP_SCAN_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(BLE_CONNECT_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(BLE_DISCONNECT_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(BLE_WRITE_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(BLE_NOTIFY_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(BLE_ADVERTISE_START_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(BLE_ADVERTISE_STOP_CALLS.load(Ordering::SeqCst), 1);
}

#[test]
fn apple_bluetooth_emits_discovery_cache_and_notifications() {
    let _guard = apple_service_test_mutex().lock().expect("Apple service test lock");
    let bluetooth = AppleBluetooth::new();
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    bluetooth.subscribe_events(Box::new(move |event| {
        sink.lock().expect("BLE events").push(event);
    }));

    let id = 0x201f_1e1d_1c1b_1a19_1817_1615_1413_1211_u128;
    let id_bytes = id.to_le_bytes();
    let service = BleUuid([0x31; 16]);
    let characteristic = BleUuid([0x32; 16]);
    let manufacturer = b"oxide-mfg";
    let name = b"oxide-ble";
    let info = AppleBleScanInfo {
        id: id_bytes,
        name_ptr: name.as_ptr(),
        name_len: name.len(),
        rssi_dbm: -42,
        services_ptr: service.0.as_ptr(),
        service_count: 1,
        manufacturer_ptr: manufacturer.as_ptr(),
        manufacturer_len: manufacturer.len(),
        connectable: 1,
    };

    unsafe {
        oxide_platform_apple::oxide_host_ble_emit_discovered(&info);
        oxide_platform_apple::oxide_host_ble_emit_connected(id_bytes.as_ptr());
        oxide_platform_apple::oxide_host_ble_emit_notified(
            id_bytes.as_ptr(),
            service.0.as_ptr(),
            characteristic.0.as_ptr(),
            b"notify".as_ptr(),
            b"notify".len(),
        );
    }

    let chr = GattChar { service, characteristic };
    let events = events.lock().expect("BLE events").clone();
    assert!(events.iter().any(|event| {
        matches!(
           event,
           BluetoothEvent::Discovered(info)
              if info.id == id
                 && info.name.as_deref() == Some("oxide-ble")
                 && info.rssi_dbm == -42
                 && info.advertisement.services == vec![service]
                 && info.advertisement.manufacturer_data.as_deref() == Some(manufacturer.as_slice())
                 && info.advertisement.connectable
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(event, BluetoothEvent::CacheUpdated(entry) if entry.peripheral.id == id)
    }));
    assert!(events
        .iter()
        .any(|event| { matches!(event, BluetoothEvent::Connected(event_id) if *event_id == id) }));
    assert!(events.iter().any(|event| {
        match event {
            BluetoothEvent::Notified { id: event_id, chr: event_chr, data } => {
                *event_id == id && *event_chr == chr && data.as_slice() == b"notify"
            }
            _ => false,
        }
    }));

    let cache = bluetooth.cached_peripherals();
    assert!(cache.iter().any(|entry| {
        entry.peripheral.id == id && entry.peripheral.name.as_deref() == Some("oxide-ble")
    }));
}

#[test]
fn apple_camera_manager_forwards_stream_controls_and_trampolines() {
    let _guard = apple_service_test_mutex().lock().expect("Apple service test lock");
    CAMERA_START_DEFAULT_CALLS.store(0, Ordering::SeqCst);
    CAMERA_START_PREVIEW_ONLY_CALLS.store(0, Ordering::SeqCst);
    CAMERA_STOP_CALLS.store(0, Ordering::SeqCst);
    reset_camera_return_codes();

    let camera = AppleCameraManager;
    let frames = Arc::new(Mutex::new(Vec::<CameraFrame>::new()));
    let audio = Arc::new(Mutex::new(Vec::<AudioSample>::new()));
    let frame_sink = frames.clone();
    let audio_sink = audio.clone();
    let stream = camera
        .start_stream(
            oxide_platform_api::CameraConfig::default(),
            Box::new(move |frame| {
                frame_sink.lock().expect("camera frames").push(frame);
            }),
            Some(Box::new(move |sample| {
                audio_sink.lock().expect("camera audio").push(sample);
            })),
        )
        .expect("camera stream");

    assert_eq!(CAMERA_START_DEFAULT_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(CAMERA_START_PREVIEW_ONLY_CALLS.load(Ordering::SeqCst), 0);
    assert!(camera_frame_callback_cell().lock().expect("camera frame callback").is_some());
    assert!(camera_audio_callback_cell().lock().expect("camera audio callback").is_some());

    let y = [1_u8, 2, 3, 4];
    let uv = [5_u8, 6];
    let frame = AppleCamFrame {
        y_ptr: y.as_ptr(),
        y_len: y.len(),
        y_stride: 2,
        uv_ptr: uv.as_ptr(),
        uv_len: uv.len(),
        uv_stride: 2,
        width: 2,
        height: 2,
        timestamp_ns: 42,
        rotation_deg: 90,
        bit_depth: 8,
        matrix: 1,
        video_range: 1,
    };
    unsafe {
        oxide_platform_apple::oxide_cam_frame_trampoline(&frame);
    }

    let samples = [7_i16, 8, 9, 10];
    let audio_sample = AppleCamAudio {
        audio_ptr: samples.as_ptr(),
        sample_count: samples.len(),
        channels: 2,
        sample_rate_hz: 48_000,
        timestamp_ns: 99,
    };
    unsafe {
        oxide_platform_apple::oxide_cam_audio_trampoline(&audio_sample);
    }

    let frames = frames.lock().expect("camera frames");
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].size, (2, 2));
    assert_eq!(frames[0].timestamp_ns, 42);
    assert_eq!(frames[0].rotation_deg, 90);
    assert!(matches!(
       &frames[0].image,
       CameraImage::Nv12 { y_plane, uv_plane, stride_y: 2, stride_uv: 2, .. }
          if y_plane == &y && uv_plane == &uv
    ));
    drop(frames);

    let audio = audio.lock().expect("camera audio");
    assert_eq!(audio.len(), 1);
    assert_eq!(audio[0].channels, 2);
    assert_eq!(audio[0].sample_rate_hz, 48_000);
    assert_eq!(audio[0].data, samples);
    drop(audio);

    stream.stop();
    assert_eq!(CAMERA_STOP_CALLS.load(Ordering::SeqCst), 1);
}

#[test]
fn apple_camera_manager_uses_preview_only_without_audio_and_handles_record_photo() {
    let _guard = apple_service_test_mutex().lock().expect("Apple service test lock");
    CAMERA_START_DEFAULT_CALLS.store(0, Ordering::SeqCst);
    CAMERA_START_PREVIEW_ONLY_CALLS.store(0, Ordering::SeqCst);
    CAMERA_RECORD_START_CALLS.store(0, Ordering::SeqCst);
    CAMERA_PHOTO_CALLS.store(0, Ordering::SeqCst);
    reset_camera_return_codes();

    let camera = AppleCameraManager;
    let stream = camera
        .start_stream(oxide_platform_api::CameraConfig::default(), Box::new(|_| {}), None)
        .expect("preview-only stream");
    assert_eq!(CAMERA_START_PREVIEW_ONLY_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(CAMERA_START_DEFAULT_CALLS.load(Ordering::SeqCst), 0);

    let records = Arc::new(Mutex::new(Vec::<RecordingEvent>::new()));
    let record_sink = records.clone();
    let recording = camera
        .start_recording(
            oxide_platform_api::RecordingOptions::default(),
            Box::new(move |event| {
                record_sink.lock().expect("record events").push(event);
            }),
        )
        .expect("recording start");
    assert_eq!(CAMERA_RECORD_START_CALLS.load(Ordering::SeqCst), 1);
    assert!(camera_record_callback_cell().lock().expect("camera record callback").is_some());

    let path = b"/tmp/oxide-camera.mov";
    let record_event = AppleCamRecordEvent {
        kind: 0,
        path_ptr: path.as_ptr(),
        path_len: path.len(),
        duration_ns: 1_000,
        size_bytes: 2_000,
        had_audio: 1,
        error_code: 0,
        error_msg_ptr: core::ptr::null(),
        error_msg_len: 0,
    };
    unsafe {
        oxide_platform_apple::oxide_cam_record_trampoline(&record_event);
    }
    assert!(matches!(
       records.lock().expect("record events").as_slice(),
       [RecordingEvent::Completed(result)]
          if result.path == "/tmp/oxide-camera.mov"
             && result.duration_ns == 1_000
             && result.size_bytes == 2_000
             && result.had_audio
    ));
    recording.stop();

    let photos = Arc::new(Mutex::new(Vec::<PhotoEvent>::new()));
    let photo_sink = photos.clone();
    camera
        .capture_photo(
            oxide_platform_api::PhotoOptions::default(),
            Box::new(move |event| {
                photo_sink.lock().expect("photo events").push(event);
            }),
        )
        .expect("photo capture");
    assert_eq!(CAMERA_PHOTO_CALLS.load(Ordering::SeqCst), 1);
    assert!(camera_photo_callback_cell().lock().expect("camera photo callback").is_some());

    let y = [11_u8, 12, 13, 14];
    let uv = [15_u8, 16];
    let frame = AppleCamFrame {
        y_ptr: y.as_ptr(),
        y_len: y.len(),
        y_stride: 2,
        uv_ptr: uv.as_ptr(),
        uv_len: uv.len(),
        uv_stride: 2,
        width: 2,
        height: 2,
        timestamp_ns: 77,
        rotation_deg: 0,
        bit_depth: 8,
        matrix: 1,
        video_range: 1,
    };
    let photo_event = AppleCamPhotoEvent {
        kind: 0,
        frame,
        error_code: 0,
        error_msg_ptr: core::ptr::null(),
        error_msg_len: 0,
    };
    unsafe {
        oxide_platform_apple::oxide_cam_photo_trampoline(&photo_event);
    }
    assert!(matches!(
       photos.lock().expect("photo events").as_slice(),
       [PhotoEvent::Completed(frame)] if frame.size == (2, 2)
    ));

    stream.stop();
}

#[test]
fn apple_camera_manager_maps_host_return_codes_to_platform_errors() {
    let _guard = apple_service_test_mutex().lock().expect("Apple service test lock");
    reset_camera_return_codes();

    let camera = AppleCameraManager;
    CAMERA_START_PREVIEW_ONLY_RC.store(-2, Ordering::SeqCst);
    let err = match camera.start_stream(
        oxide_platform_api::CameraConfig::default(),
        Box::new(|_| {}),
        None,
    ) {
        Ok(stream) => {
            stream.stop();
            panic!("camera stream unexpectedly started");
        }
        Err(err) => err,
    };
    assert!(matches!(err, PlatformError::PermissionDenied("camera start failed")));
    reset_camera_return_codes();

    CAMERA_RECORD_START_RC.store(-3, Ordering::SeqCst);
    let err = match camera
        .start_recording(oxide_platform_api::RecordingOptions::default(), Box::new(|_| {}))
    {
        Ok(recording) => {
            recording.cancel();
            panic!("camera recording unexpectedly started");
        }
        Err(err) => err,
    };
    assert!(matches!(err, PlatformError::NotFound("camera recording unavailable")));
    reset_camera_return_codes();

    CAMERA_FOCUS_RC.store(-5, Ordering::SeqCst);
    let err = camera.set_focus_point(0.5, 0.5).expect_err("focus should fail");
    assert!(matches!(err, PlatformError::Invalid("set_focus_point failed")));
    reset_camera_return_codes();

    CAMERA_ZOOM_RC.store(-1, Ordering::SeqCst);
    let err = camera.set_zoom_factor(2.0).expect_err("zoom should fail");
    assert!(matches!(err, PlatformError::Unsupported("set_zoom_factor failed")));
    reset_camera_return_codes();

    CAMERA_PHOTO_RC.store(-4, Ordering::SeqCst);
    let err = camera
        .capture_photo(oxide_platform_api::PhotoOptions::default(), Box::new(|_| {}))
        .expect_err("photo should fail");
    assert!(matches!(err, PlatformError::Busy));
    reset_camera_return_codes();

    camera
        .capture_photo(oxide_platform_api::PhotoOptions::default(), Box::new(|_| {}))
        .expect("photo callback slot should be cleared after host failure");
    let photo_event = AppleCamPhotoEvent {
        kind: 1,
        frame: AppleCamFrame {
            y_ptr: core::ptr::null(),
            y_len: 0,
            y_stride: 0,
            uv_ptr: core::ptr::null(),
            uv_len: 0,
            uv_stride: 0,
            width: 0,
            height: 0,
            timestamp_ns: 0,
            rotation_deg: 0,
            bit_depth: 0,
            matrix: 0,
            video_range: 0,
        },
        error_code: 5,
        error_msg_ptr: core::ptr::null(),
        error_msg_len: 0,
    };
    unsafe {
        oxide_platform_apple::oxide_cam_photo_trampoline(&photo_event);
    }
}

#[cfg(feature = "web-view-macos")]
#[test]
fn apple_web_view_service_creates_executes_emits_and_closes() {
    WEB_VIEW_LAST_ID.store(0, Ordering::SeqCst);
    web_view_closed_ids().lock().expect("web view closed ids").clear();

    let service = AppleWebViewService::new();
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let view = service
        .create_view(
            "https://example.invalid",
            Box::new(move |event| {
                sink.lock().expect("web view events").push(event);
            }),
        )
        .expect("web view create");

    let id = WEB_VIEW_LAST_ID.load(Ordering::SeqCst);
    assert_ne!(id, 0);
    assert!(web_view_callback_cell().lock().expect("web view callback cell").is_some());

    unsafe {
        oxide_web_view_event_trampoline(id, 0, core::ptr::null(), 0);
        oxide_web_view_event_trampoline(id, 1, b"load failed".as_ptr(), b"load failed".len());
    }

    let result = block_on_ready(view.execute_script("1 + 1")).expect("script result");
    assert_eq!(result.as_deref(), Some("script:1 + 1"));
    let empty = block_on_ready(view.execute_script("empty")).expect("empty script result");
    assert_eq!(empty.as_deref(), Some(""));
    let undefined =
        block_on_ready(view.execute_script("undefined")).expect("undefined script result");
    assert_eq!(undefined, None);
    let missing = block_on_ready(view.execute_script("missing")).expect_err("missing script view");
    assert!(matches!(missing, PlatformError::NotFound("web view handle not found")));
    let failed = block_on_ready(view.execute_script("fail")).expect_err("failed script");
    assert!(
        matches!(failed, PlatformError::Unknown(message) if message == "web view script failed")
    );
    let copy_failed =
        block_on_ready(view.execute_script("copy-fail")).expect_err("copy failed script");
    assert!(
        matches!(copy_failed, PlatformError::Io(message) if message == "web view result copy failed")
    );

    let events = events.lock().expect("web view events");
    assert_eq!(events.len(), 2);
    assert!(matches!(events[0], WebViewEvent::LoadFinished));
    assert!(matches!(events[1], WebViewEvent::LoadFailed(_)));
    drop(events);

    view.close();
    assert_eq!(web_view_closed_ids().lock().expect("web view closed ids").as_slice(), &[id]);
}

#[cfg(feature = "web-view-macos")]
#[test]
fn apple_web_view_service_maps_host_create_errors() {
    let service = AppleWebViewService::new();

    let unavailable = match service.create_view("webview-unavailable", Box::new(|_| {})) {
        Ok(view) => {
            view.close();
            panic!("unavailable web view unexpectedly created");
        }
        Err(err) => err,
    };
    assert!(matches!(unavailable, PlatformError::Unsupported("web view unavailable")));

    let busy = match service.create_view("webview-busy", Box::new(|_| {})) {
        Ok(view) => {
            view.close();
            panic!("busy web view unexpectedly created");
        }
        Err(err) => err,
    };
    assert!(matches!(busy, PlatformError::Busy));
}

#[test]
fn apple_path_kinds_decode_to_reachability() {
    assert_eq!(
        reachability_state_from_apple_path(0, APPLE_PATH_KIND_WIFI, false),
        ReachabilityState::Offline
    );

    let wifi = reachability_state_from_apple_path(1, APPLE_PATH_KIND_WIFI, false);
    assert!(
        matches!(wifi, ReachabilityState::Online { path } if path.kind == NetworkPathKind::Wifi && !path.expensive)
    );

    let cellular = reachability_state_from_apple_path(1, APPLE_PATH_KIND_CELLULAR, true);
    assert!(
        matches!(cellular, ReachabilityState::Online { path } if path.kind == NetworkPathKind::Cellular && path.expensive)
    );

    let wired = reachability_state_from_apple_path(1, APPLE_PATH_KIND_WIRED, false);
    assert!(
        matches!(wired, ReachabilityState::Online { path } if path.kind == NetworkPathKind::Wired)
    );

    let other = reachability_state_from_apple_path(1, APPLE_PATH_KIND_OTHER, false);
    assert!(
        matches!(other, ReachabilityState::Online { path } if path.kind == NetworkPathKind::Other)
    );
}

#[test]
fn apple_network_status_reports_interface_bits() {
    let offline = network_status_from_apple_interface_mask(false, APPLE_INTERFACE_WIFI);
    assert!(!offline.is_connected);
    assert!(offline.interfaces.is_empty());

    let status = network_status_from_apple_interface_mask(
        true,
        APPLE_INTERFACE_WIFI | APPLE_INTERFACE_CELLULAR | APPLE_INTERFACE_WIRED,
    );
    assert!(status.is_connected);
    assert!(status.interfaces.contains(NetworkInterface::WIFI));
    assert!(status.interfaces.contains(NetworkInterface::CELLULAR));
    assert!(status.interfaces.contains(NetworkInterface::WIRED));

    let single_path = network_status_from_apple_path(1, APPLE_PATH_KIND_WIFI);
    assert_eq!(single_path.interfaces, NetworkInterface::WIFI);
}

#[test]
fn apple_permission_codes_round_trip_known_values() {
    assert_eq!(permission_domain_to_apple_code(PermissionDomain::Camera), APPLE_PERMISSION_CAMERA);
    assert_eq!(
        permission_domain_from_apple_code(APPLE_PERMISSION_CAMERA),
        Some(PermissionDomain::Camera)
    );
    assert_eq!(permission_domain_from_apple_code(u32::MAX), None);

    assert_eq!(
        permission_status_from_apple_code(APPLE_PERMISSION_NOT_DETERMINED),
        PermissionStatus::NotDetermined
    );
    assert_eq!(
        permission_status_from_apple_code(APPLE_PERMISSION_AUTHORIZED),
        PermissionStatus::Authorized
    );
    assert_eq!(permission_status_to_apple_code(PermissionStatus::Denied), APPLE_PERMISSION_DENIED);
    assert_eq!(permission_status_from_apple_code(u32::MAX), PermissionStatus::Denied);
}

fn location_sample(latitude: f64, longitude: f64, timestamp_ms: u64) -> AppleLocationSample {
    AppleLocationSample {
        latitude,
        longitude,
        altitude: 14.0,
        horizontal_accuracy: 6.0,
        vertical_accuracy: 8.0,
        speed: 2.5,
        course: 90.0,
        timestamp_ms,
    }
}

#[test]
fn apple_location_update_trampoline_caches_last_and_history() {
    let _guard = apple_service_test_mutex().lock().expect("Apple service test lock");
    let service = AppleLocationService::new();
    let sample = location_sample(37.3349, -122.0090, 42);

    unsafe {
        oxide_location_update_trampoline(&sample);
    }

    let last = service.last().expect("last location");
    assert_eq!(last.latitude_deg, sample.latitude);
    assert_eq!(last.longitude_deg, sample.longitude);
    assert_eq!(last.timestamp_ms, sample.timestamp_ms);
    assert_eq!(service.history().last(), Some(&last));
}

#[test]
fn apple_location_region_tracker_emits_enter_and_exit_events() {
    let _guard = apple_service_test_mutex().lock().expect("Apple service test lock");
    let service = AppleLocationService::new();
    unsafe {
        oxide_location_update_trampoline(&location_sample(0.0, 0.0, 1));
    }

    let events = Arc::new(Mutex::new(Vec::new()));
    let tracker = service.region_tracker().expect("region tracker");
    tracker
        .set_regions(&[GeoRegion {
            hash: GeoHash(0),
            center: (37.3349, -122.0090),
            radius_m: 50.0,
        }])
        .expect("set regions");

    let events_sink = events.clone();
    service.subscribe(Box::new(move |event| {
        events_sink.lock().expect("location events").push(event);
    }));

    unsafe {
        oxide_location_update_trampoline(&location_sample(37.3349, -122.0090, 2));
        oxide_location_update_trampoline(&location_sample(37.3400, -122.0150, 3));
    }

    let events = events.lock().expect("location events").clone();
    assert!(events.iter().any(|event| matches!(event, LocationEvent::EnteredRegion(_))));
    assert!(events.iter().any(|event| matches!(event, LocationEvent::ExitedRegion(_))));
}

#[test]
fn apple_location_error_trampoline_emits_error_events() {
    let _guard = apple_service_test_mutex().lock().expect("Apple service test lock");
    let service = AppleLocationService::new();
    let events = Arc::new(Mutex::new(Vec::new()));
    let events_sink = events.clone();
    service.subscribe(Box::new(move |event| {
        events_sink.lock().expect("location events").push(event);
    }));

    let msg = b"gps offline";
    unsafe {
        oxide_location_error_trampoline(msg.as_ptr(), msg.len());
    }

    let events = events.lock().expect("location events").clone();
    assert!(events.iter().any(|event| {
      matches!(event, LocationEvent::Error(PlatformError::Unknown(message)) if message == "gps offline")
   }));
}

#[test]
fn apple_motion_trampoline_caches_history_and_notifies_subscribers() {
    let _guard = apple_service_test_mutex().lock().expect("Apple service test lock");
    let service = AppleMotionService::new();
    let events = Arc::new(Mutex::new(Vec::new()));
    let events_sink = events.clone();
    service.subscribe(Box::new(move |event| {
        events_sink.lock().expect("motion events").push(event);
    }));

    let sample = AppleMotionSample {
        pressure_pa: 101_325.0,
        relative_altitude_m: 12.5,
        timestamp_ms: 9,
        has_pressure: 1,
        has_relative_altitude: 1,
    };
    unsafe {
        oxide_motion_trampoline(&sample);
    }

    let history = service.pressure_history();
    let last = history.last().expect("motion history");
    assert_eq!(last.pressure_pa, Some(101_325.0));
    assert_eq!(last.relative_altitude_m, Some(12.5));
    assert_eq!(last.timestamp_ms, 9);
    assert_eq!(events.lock().expect("motion events").last(), Some(last));
}
