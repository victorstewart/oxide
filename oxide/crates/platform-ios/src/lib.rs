//! Oxide iOS platform crate
//!
//! This module exposes safe wrappers for clipboard and haptics on iOS,
//! backed by Objective‑C bridges compiled in the host static library.
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::missing_safety_doc
)]

use oxide_networking::ReachabilityManager;
#[cfg(feature = "tokio-runtime")]
use oxide_platform_api::runtime;
use oxide_platform_api::telephony::{normalize_country_iso, TelephonyService};
use oxide_platform_api::{HapticPattern, Haptics as HapticsTrait};
use oxide_platform_api::{PermissionDomain, PermissionStatus, Permissions};
use oxide_platform_api::{PlatformError, TimeService};

use core::time::Duration;
use once_cell::sync::Lazy;
use std::sync::{Arc, Mutex, Weak};

type PermissionStatusCallback = Box<dyn Fn(PermissionDomain, PermissionStatus) + Send + 'static>;
extern "C" {
    fn oxide_host_clipboard_set(utf8: *const u8, len: usize);
    fn oxide_host_clipboard_get(out_ptr: *mut *mut u8, out_len: *mut usize) -> ::libc::c_int;
    fn oxide_host_string_free(p: *mut u8);
    fn oxide_host_haptics_play(pattern: u32);
    fn oxide_host_perm_status(domain: u32) -> u32;
    fn oxide_host_perm_request(domain: u32);
    fn oxide_host_set_perm_callback(cb: Option<extern "C" fn(u32, u32)>);
    // Networking
    fn oxide_host_net_set_reachability_callback(cb: Option<extern "C" fn(u32, u32, u8)>);
    fn oxide_host_net_start_reachability() -> ::libc::c_int;
    fn oxide_host_net_stop_reachability();
}

pub mod clipboard {
    use super::*;

    pub fn set(s: &str) {
        unsafe { oxide_host_clipboard_set(s.as_ptr(), s.len()) };
    }

    pub fn get() -> Option<String> {
        let mut ptr: *mut u8 = core::ptr::null_mut();
        let mut len: usize = 0;
        let ok = unsafe { oxide_host_clipboard_get(&mut ptr, &mut len) };
        if ok == 0 || ptr.is_null() || len == 0 {
            return None;
        }
        let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
        let out = String::from_utf8_lossy(slice).into_owned();
        unsafe { oxide_host_string_free(ptr) };
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
        unsafe { oxide_host_haptics_play(code) };
    }
}

pub struct IosTime;

impl TimeService for IosTime {
    fn monotonic_now(&self) -> Duration {
        let mut info = mach2::mach_time::mach_timebase_info_data_t { numer: 0, denom: 0 };
        let status = unsafe { mach2::mach_time::mach_timebase_info(&mut info) };
        if status != mach2::kern_return::KERN_SUCCESS || info.denom == 0 {
            return Duration::from_nanos(0);
        }
        let time = unsafe { mach2::mach_time::mach_absolute_time() };
        let nanos = time.saturating_mul(u64::from(info.numer)) / u64::from(info.denom);
        Duration::from_nanos(nanos)
    }
}

extern crate alloc;

// ===== Permissions =====

static PERM_INIT: std::sync::Once = std::sync::Once::new();
static SUBS: Lazy<std::sync::Mutex<Vec<PermissionStatusCallback>>> =
    Lazy::new(|| std::sync::Mutex::new(Vec::new()));

#[no_mangle]
pub extern "C" fn oxide_perm_trampoline(domain: u32, status: u32) {
    let d = from_domain(domain);
    let s = from_status(status);
    let subs = SUBS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    for f in subs.iter() {
        f(d, s);
    }
}

fn to_domain(d: PermissionDomain) -> u32 {
    oxide_platform_apple::permission_domain_to_apple_code(d)
}
fn from_domain(v: u32) -> PermissionDomain {
    oxide_platform_apple::permission_domain_from_apple_code(v)
        .unwrap_or(PermissionDomain::Notifications)
}
fn from_status(v: u32) -> PermissionStatus {
    oxide_platform_apple::permission_status_from_apple_code(v)
}

pub struct IosPermissions;

impl IosPermissions {
    fn ensure_cb() {
        PERM_INIT.call_once(|| unsafe {
            oxide_host_set_perm_callback(Some(oxide_perm_trampoline));
        });
    }
}

impl Permissions for IosPermissions {
    fn request(&self, domain: PermissionDomain) {
        unsafe {
            oxide_host_perm_request(to_domain(domain));
        }
    }
    fn status(&self, domain: PermissionDomain) -> PermissionStatus {
        let s = unsafe { oxide_host_perm_status(to_domain(domain)) };
        from_status(s)
    }
    fn subscribe(&self, f: PermissionStatusCallback) {
        Self::ensure_cb();
        SUBS.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(f);
    }
}

// ===== Location and motion =====

pub use oxide_platform_apple::{
    AppleLocationConfig as OxideLocationConfig, AppleLocationSample as OxideLocationSample,
    AppleLocationService as IosLocation, AppleMotionSample as OxideMotionSample,
    AppleMotionService as IosMotion,
};

// ===== HTTP =====

pub use oxide_platform_apple::AppleHttpClient as IosHttpClient;

// ===== Reachability =====

static REACHABILITY_TARGET: Lazy<Mutex<Weak<ReachabilityManager>>> =
    Lazy::new(|| Mutex::new(Weak::new()));
static REACHABILITY_INIT: std::sync::Once = std::sync::Once::new();

#[no_mangle]
pub extern "C" fn oxide_reachability_trampoline(status: u32, iface: u32, expensive: u8) {
    let manager = REACHABILITY_TARGET.lock().ok().and_then(|guard| guard.clone().upgrade());
    if let Some(manager) = manager {
        let state = decode_reachability(status, iface, expensive != 0);
        manager.update(state);
    }
}

fn decode_reachability(
    status: u32,
    iface: u32,
    expensive: bool,
) -> oxide_networking::ReachabilityState {
    oxide_platform_apple::reachability_state_from_apple_path(status, iface, expensive)
}

fn ensure_reachability_callback() {
    REACHABILITY_INIT.call_once(|| unsafe {
        oxide_host_net_set_reachability_callback(Some(oxide_reachability_trampoline));
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
        let rc = unsafe { oxide_host_net_start_reachability() };
        if rc == 0 {
            Ok(())
        } else {
            Err(PlatformError::Unsupported("reachability unavailable"))
        }
    }

    pub fn stop(&self) {
        unsafe { oxide_host_net_stop_reachability() };
        if let Ok(mut slot) = REACHABILITY_TARGET.lock() {
            let should_clear =
                slot.upgrade().is_some_and(|current| Arc::ptr_eq(&current, &self.manager));
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

pub use oxide_platform_apple::{
    oxide_push_notify_trampoline, oxide_push_token_trampoline, ApplePushManager as IosPushManager,
};

// ===== Bluetooth =====

pub use oxide_platform_apple::{
    apple_bluetooth_with_restoration as bluetooth_with_restoration,
    AppleBleScanConfig as OxideBleScanConfig, AppleBleScanInfo as OxideBleScanInfo,
    AppleBluetooth as IosBluetooth,
};

// ===== Camera manager =====

pub use oxide_platform_apple::{
    camera, camera_manager, AppleCamAudio as OxideCamAudio, AppleCamFrame as OxideCamFrame,
    AppleCamPhotoEvent as OxideCamPhotoEvent, AppleCamRecordEvent as OxideCamRecordEvent,
    AppleCameraManager as IosCameraManager,
};

// ===== Contacts =====

use oxide_platform_api::contacts::{
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

pub use oxide_platform_apple::{
    AppleMediaAsset as OxideMediaAsset, AppleMediaImageData as OxideImageData,
    AppleMediaLibraryManager as IosMediaLibraryManager, AppleRawImageData as IosRawImageData,
};

// ===== Telephony =====

extern "C" {
    fn oxide_telephony_home_country_iso(
        buffer: *mut std::os::raw::c_char,
        buffer_len: usize,
    ) -> bool;
}

pub struct IosTelephonyService;

impl TelephonyService for IosTelephonyService {
    fn home_country_iso_code(&self) -> Option<String> {
        let mut buffer = [0 as std::os::raw::c_char; 8];
        let ok = unsafe { oxide_telephony_home_country_iso(buffer.as_mut_ptr(), buffer.len()) };
        if !ok {
            return None;
        }
        let value = unsafe { std::ffi::CStr::from_ptr(buffer.as_ptr()) };
        let as_str = value.to_str().ok()?;
        normalize_country_iso(as_str)
    }
}

// ===== Secure Storage =====

pub use oxide_platform_apple::AppleSecureStorage as IosSecureStorage;

// ===== URL Scheme Handling =====

use oxide_platform_api::url_scheme::{
    UrlComponents, UrlOpenResult, UrlSchemeHandler, UrlSchemeSecurity,
};

extern "C" {
    fn oxide_url_can_open(url_ptr: *const u8, url_len: usize) -> i32;
    fn oxide_url_open(url_ptr: *const u8, url_len: usize) -> i32;
    #[allow(dead_code)]
    fn oxide_url_register_handler(callback: extern "C" fn(*const u8, usize));
}

pub struct IosUrlSchemeHandler {
    security: UrlSchemeSecurity,
}

impl Default for IosUrlSchemeHandler {
    fn default() -> Self {
        Self { security: UrlSchemeSecurity::default() }
    }
}

impl UrlSchemeHandler for IosUrlSchemeHandler {
    fn security(&self) -> &UrlSchemeSecurity {
        &self.security
    }

    fn set_security(&mut self, security: UrlSchemeSecurity) {
        self.security = security;
    }

    fn can_open(&self, url: &str) -> bool {
        let url_bytes = url.as_bytes();
        let result = unsafe { oxide_url_can_open(url_bytes.as_ptr(), url_bytes.len()) };
        result > 0
    }

    fn open_unchecked(&mut self, url: &str) -> UrlOpenResult {
        let url_bytes = url.as_bytes();
        let result = unsafe { oxide_url_open(url_bytes.as_ptr(), url_bytes.len()) };

        match result {
            1 => UrlOpenResult::Opened,
            0 => UrlOpenResult::NotSupported,
            _ => UrlOpenResult::Error(format!("Failed to open URL: {}", result)),
        }
    }

    fn register_handler<F>(&mut self, _callback: F)
    where
        F: Fn(UrlComponents) + Send + 'static,
    {
        // Stub: callback bridge to host URL dispatch remains pending.
    }
}

// ===== Tokio runtime integration =====
#[cfg(feature = "tokio-runtime")]
pub fn init_tokio_spawn() {
    use once_cell::sync::OnceCell;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static RT: OnceCell<Option<tokio::runtime::Runtime>> = OnceCell::new();
    static NEXT_WORKER_INDEX: AtomicUsize = AtomicUsize::new(0);
    runtime::set_spawn(|fut| {
        let rt = RT.get_or_init(|| {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(std::cmp::min(4, num_cpus::get()))
                .thread_name_fn(|| {
                    format!("oxide-tokio-{}", NEXT_WORKER_INDEX.fetch_add(1, Ordering::Relaxed))
                })
                .on_thread_start(|| {
                    #[cfg(any(target_os = "ios", target_os = "macos"))]
                    {
                        if let Some(name) = std::thread::current().name() {
                            let mut bytes = name.as_bytes().to_vec();
                            if bytes.len() > 63 {
                                bytes.truncate(63);
                            }
                            if let Ok(c_name) = std::ffi::CString::new(bytes) {
                                unsafe {
                                    libc::pthread_setname_np(c_name.as_ptr());
                                }
                            }
                        }
                    }
                })
                .enable_all()
                .build();
            runtime.ok()
        });
        if let Some(rt) = rt.as_ref() {
            drop(rt.spawn(fut));
        }
    });
}
