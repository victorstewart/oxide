use oxide_platform_api::{Capabilities, Platform};
use std::sync::{Mutex, OnceLock};

fn clipboard_bytes() -> &'static Mutex<Vec<u8>> {
    static CLIPBOARD: OnceLock<Mutex<Vec<u8>>> = OnceLock::new();
    CLIPBOARD.get_or_init(|| Mutex::new(Vec::new()))
}

#[no_mangle]
extern "C" fn macos_request_redraw() {}

#[no_mangle]
extern "C" fn macos_set_high_refresh(_enable: u8) {}

#[no_mangle]
extern "C" fn macos_set_idle_timer_disabled(_disabled: u8) {}

#[no_mangle]
extern "C" fn macos_open_system_settings() {}

#[no_mangle]
extern "C" fn macos_open_external_url(ptr: *const u8, len: usize) -> libc::c_int {
    if ptr.is_null() || len == 0 {
        return 0;
    }
    1
}

#[no_mangle]
extern "C" fn macos_max_framerate_hz() -> u32 {
    0
}

#[no_mangle]
extern "C" fn macos_native_scale() -> f32 {
    0.0
}

#[no_mangle]
extern "C" fn macos_supports_edr() -> u8 {
    0
}

#[no_mangle]
extern "C" fn macos_reduce_motion_enabled() -> u8 {
    1
}

#[no_mangle]
extern "C" fn macos_camera_available() -> u8 {
    0
}

#[no_mangle]
extern "C" fn macos_network_status(
    out_connected: *mut u8,
    out_interfaces: *mut u32,
) -> libc::c_int {
    if out_connected.is_null() || out_interfaces.is_null() {
        return 0;
    }
    unsafe {
        *out_connected = 1;
        *out_interfaces = 1;
    }
    1
}

#[no_mangle]
extern "C" fn macos_set_network_status_callback(_cb: Option<extern "C" fn(u8, u32)>) {}

#[no_mangle]
extern "C" fn macos_start_network_monitor() -> libc::c_int {
    1
}

#[no_mangle]
extern "C" fn macos_permission_status(_domain: u32) -> u32 {
    0
}

#[no_mangle]
extern "C" fn macos_permission_request(_domain: u32) {}

#[no_mangle]
extern "C" fn macos_set_permission_callback(_cb: Option<extern "C" fn(u32, u32)>) {}

#[no_mangle]
extern "C" fn macos_location_services_available() -> u8 {
    0
}

#[no_mangle]
extern "C" fn macos_motion_available() -> u8 {
    0
}

#[no_mangle]
extern "C" fn macos_clipboard_set(ptr: *const u8, len: usize) {
    let mut bytes = clipboard_bytes().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    bytes.clear();
    if !ptr.is_null() && len != 0 {
        bytes.extend_from_slice(unsafe { std::slice::from_raw_parts(ptr, len) });
    }
}

#[no_mangle]
extern "C" fn macos_clipboard_get(out_ptr: *mut *mut u8, out_len: *mut usize) -> libc::c_int {
    if out_ptr.is_null() || out_len.is_null() {
        return 0;
    }
    let bytes = clipboard_bytes().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    unsafe {
        *out_ptr = std::ptr::null_mut();
        *out_len = bytes.len();
    }
    if bytes.is_empty() {
        return 1;
    }
    let ptr = unsafe { libc::malloc(bytes.len()) };
    if ptr.is_null() {
        return 0;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.cast::<u8>(), bytes.len());
        *out_ptr = ptr.cast::<u8>();
    }
    1
}

#[no_mangle]
extern "C" fn macos_haptics_play(_pattern: u32) {}

#[no_mangle]
extern "C" fn macos_free(p: *mut libc::c_void) {
    if !p.is_null() {
        unsafe {
            libc::free(p);
        }
    }
}

#[test]
fn mac_platform_clipboard_preserves_empty_string() {
    let platform = oxide_platform_macos::platform();

    platform.clipboard_set("");

    assert_eq!(platform.clipboard_get(), Some(String::new()));
}

#[test]
fn mac_platform_clipboard_round_trips_text() {
    let platform = oxide_platform_macos::platform();

    platform.clipboard_set("oxide-macos");

    assert_eq!(platform.clipboard_get(), Some(String::from("oxide-macos")));
}

#[test]
fn mac_platform_device_caps_sanitize_host_values() {
    let platform = oxide_platform_macos::platform();
    let caps = platform.device_caps();

    assert_eq!(caps.max_framerate_hz, 60);
    assert_eq!(caps.native_scale, 1.0);
    assert!(caps.a11y_reduce_motion);
}

#[test]
fn mac_platform_capabilities_are_gated_by_host_availability() {
    let platform = oxide_platform_macos::platform();
    let caps = platform.capabilities();

    assert!(caps.contains(Capabilities::HOVER_POINTER));
    assert!(caps.contains(Capabilities::BLUETOOTH));
    assert!(caps.contains(Capabilities::PUSH));
    assert!(!caps.contains(Capabilities::CAMERA));
    assert!(!caps.contains(Capabilities::CAMERA_RECORDING));
    assert!(!caps.contains(Capabilities::LOCATION));
    assert!(!caps.contains(Capabilities::MOTION));
}
