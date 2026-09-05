use std::sync::atomic::{AtomicU32, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use oxide_platform_api::network_status::{NetworkInterface, NetworkStatus};
use oxide_platform_api::{HapticPattern, Platform, PlatformError, StandardPath};
use oxide_platform_ios::IosPlatform;

static CLIPBOARD: Mutex<Vec<u8>> = Mutex::new(Vec::new());
static CLIPBOARD_READ_OK: AtomicU8 = AtomicU8::new(1);
static HAPTIC: AtomicU32 = AtomicU32::new(u32::MAX);
static REDRAWS: AtomicUsize = AtomicUsize::new(0);
static HIGH_REFRESH: AtomicU8 = AtomicU8::new(0);
static IDLE_DISABLED: AtomicU8 = AtomicU8::new(0);
static SETTINGS_OPENS: AtomicUsize = AtomicUsize::new(0);
static URL_OPENS: AtomicUsize = AtomicUsize::new(0);
static IME_SHOWS: AtomicUsize = AtomicUsize::new(0);
static IME_HIDES: AtomicUsize = AtomicUsize::new(0);
static PATH_KIND: AtomicU32 = AtomicU32::new(u32::MAX);
static NATIVE_SCALE_BITS: AtomicU32 = AtomicU32::new(3.0_f32.to_bits());
static REACHABILITY_STARTS: AtomicUsize = AtomicUsize::new(0);
static REACHABILITY_CALLBACK: Mutex<Option<extern "C" fn(u32, u32, u8)>> = Mutex::new(None);

#[no_mangle]
pub extern "C" fn oxide_host_clipboard_set(ptr: *const u8, len: usize)
{
   let value = if ptr.is_null() || len == 0
   {
      Vec::new()
   }
   else
   {
      unsafe
      {
         std::slice::from_raw_parts(ptr, len)
      }.to_vec()
   };
   *CLIPBOARD.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = value;
}

#[no_mangle]
pub extern "C" fn oxide_host_clipboard_get(out_ptr: *mut *mut u8, out_len: *mut usize) -> i32
{
   if CLIPBOARD_READ_OK.load(Ordering::SeqCst) == 0 || out_ptr.is_null() || out_len.is_null()
   {
      return 0;
   }
   let value = CLIPBOARD.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
   unsafe
   {
      copy_to_host(value.as_slice(), out_ptr, out_len)
   }
}

#[no_mangle]
pub extern "C" fn oxide_host_string_free(ptr: *mut u8)
{
   unsafe
   {
      libc::free(ptr.cast());
   }
}

#[no_mangle]
pub extern "C" fn oxide_host_haptics_play(pattern: u32)
{
   HAPTIC.store(pattern, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn oxide_host_request_redraw()
{
   REDRAWS.fetch_add(1, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn oxide_host_set_high_refresh(enable: u8)
{
   HIGH_REFRESH.store(enable, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn oxide_host_set_idle_timer_disabled(disabled: u8)
{
   IDLE_DISABLED.store(disabled, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn oxide_host_open_system_settings()
{
   SETTINGS_OPENS.fetch_add(1, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn oxide_host_open_external_url(ptr: *const u8, len: usize) -> i32
{
   if ptr.is_null() || len == 0
   {
      return 0;
   }
   let value = unsafe
   {
      std::slice::from_raw_parts(ptr, len)
   };
   if value.starts_with(b"https://")
   {
      URL_OPENS.fetch_add(1, Ordering::SeqCst);
      1
   }
   else
   {
      0
   }
}

#[no_mangle]
pub extern "C" fn oxide_host_ime_show()
{
   IME_SHOWS.fetch_add(1, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn oxide_host_ime_hide()
{
   IME_HIDES.fetch_add(1, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn oxide_host_max_framerate_hz() -> u32
{
   120
}

#[no_mangle]
pub extern "C" fn oxide_host_native_scale() -> f32
{
   f32::from_bits(NATIVE_SCALE_BITS.load(Ordering::SeqCst))
}

#[no_mangle]
pub extern "C" fn oxide_host_supports_edr() -> u8
{
   1
}

#[no_mangle]
pub extern "C" fn oxide_host_is_simulation() -> u8
{
   1
}

#[no_mangle]
pub extern "C" fn oxide_host_standard_path(kind: u32, out_ptr: *mut *mut u8, out_len: *mut usize) -> i32
{
   PATH_KIND.store(kind, Ordering::SeqCst);
   let value = match kind
   {
      0 => b"/app/Library/Application Support/Oxide".as_slice(),
      1 => b"/app/Library/Caches/Oxide".as_slice(),
      2 => b"/app/tmp/Oxide".as_slice(),
      _ => return 0,
   };
   unsafe
   {
      copy_to_host(value, out_ptr, out_len)
   }
}

#[no_mangle]
pub extern "C" fn oxide_host_net_set_reachability_callback(callback: Option<extern "C" fn(u32, u32, u8)>)
{
   *REACHABILITY_CALLBACK
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner) = callback;
}

#[no_mangle]
pub extern "C" fn oxide_host_net_start_reachability() -> i32
{
   REACHABILITY_STARTS.fetch_add(1, Ordering::SeqCst);
   emit_reachability(1, 0, 0);
   0
}

#[no_mangle]
pub extern "C" fn oxide_host_net_stop_reachability()
{
}

unsafe fn copy_to_host(value: &[u8], out_ptr: *mut *mut u8, out_len: *mut usize) -> i32
{
   if out_ptr.is_null() || out_len.is_null()
   {
      return 0;
   }
   if value.is_empty()
   {
      unsafe
      {
         *out_ptr = std::ptr::null_mut();
         *out_len = 0;
      }
      return 1;
   }
   let ptr = unsafe
   {
      libc::malloc(value.len())
   }.cast::<u8>();
   if ptr.is_null()
   {
      return 0;
   }
   unsafe
   {
      std::ptr::copy_nonoverlapping(value.as_ptr(), ptr, value.len());
      *out_ptr = ptr;
      *out_len = value.len();
   }
   1
}

fn emit_reachability(status: u32, interface: u32, expensive: u8)
{
   let callback = *REACHABILITY_CALLBACK
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner);
   if let Some(callback) = callback
   {
      callback(status, interface, expensive);
   }
}

#[test]
fn host_controls_and_device_caps_forward_without_policy_in_uikit()
{
   let platform = IosPlatform::new();
   platform.request_redraw();
   platform.set_high_refresh(true);
   platform.set_idle_timer_disabled(true);
   platform.open_system_settings();
   platform.ime_show();
   platform.ime_hide();
   assert_eq!(platform.open_external_url("https://oxide.test"), Ok(()));
   assert_eq!(
      platform.open_external_url("invalid"),
      Err(PlatformError::Unsupported("iOS rejected external URL"))
   );
   assert_eq!(
      platform.open_external_url(""),
      Err(PlatformError::Invalid("external URL is empty"))
   );

   assert_eq!(REDRAWS.load(Ordering::SeqCst), 1);
   assert_eq!(HIGH_REFRESH.load(Ordering::SeqCst), 1);
   assert_eq!(IDLE_DISABLED.load(Ordering::SeqCst), 1);
   assert_eq!(SETTINGS_OPENS.load(Ordering::SeqCst), 1);
   assert_eq!(URL_OPENS.load(Ordering::SeqCst), 1);
   assert_eq!(IME_SHOWS.load(Ordering::SeqCst), 1);
   assert_eq!(IME_HIDES.load(Ordering::SeqCst), 1);

   let caps = platform.device_caps();
   assert_eq!(caps.max_framerate_hz, 120);
   assert_eq!(caps.native_scale, 3.0);
   assert!(caps.supports_edr);
   assert!(caps.supports_msaa4x);
   assert!(platform.is_simulation());
   assert_eq!(
      platform.capabilities(),
      oxide_platform_api::Capabilities::CAMERA
         | oxide_platform_api::Capabilities::CAMERA_RECORDING
         | oxide_platform_api::Capabilities::BLUETOOTH
         | oxide_platform_api::Capabilities::PUSH
         | oxide_platform_api::Capabilities::MOTION
         | oxide_platform_api::Capabilities::LOCATION
   );
   NATIVE_SCALE_BITS.store(f32::NAN.to_bits(), Ordering::SeqCst);
   assert_eq!(platform.device_caps().native_scale, 1.0);
   NATIVE_SCALE_BITS.store(3.0_f32.to_bits(), Ordering::SeqCst);

   platform.haptics().play(HapticPattern::NotificationWarning);
   assert_eq!(HAPTIC.load(Ordering::SeqCst), 5);
}

#[test]
fn native_external_url_requires_synchronous_openability_check()
{
   let source = include_str!(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/src/ios/host_services.m"
   ));
   let implementation = source
      .split("int32_t oxide_host_open_external_url")
      .nth(1)
      .expect("external URL host implementation")
      .split("uint32_t oxide_host_max_framerate_hz")
      .next()
      .expect("external URL host implementation end");
   let sync = implementation
      .find("dispatch_main_sync")
      .expect("external URL check runs synchronously on the main queue");
   let can_open = implementation
      .find("[application canOpenURL:url]")
      .expect("external URL check requires UIApplication openability");
   let open = implementation
      .find("[application openURL:url")
      .expect("external URL host schedules the accepted URL");

   assert!(sync < can_open);
   assert!(can_open < open);
   assert!(implementation.contains("return opened;"));
}

#[test]
fn clipboard_preserves_empty_and_nonempty_utf8()
{
   let platform = IosPlatform::new();
   platform.clipboard_set("");
   assert_eq!(platform.clipboard_get(), Some(String::new()));
   platform.clipboard_set("Oxide λ");
   assert_eq!(platform.clipboard_get(), Some(String::from("Oxide λ")));
   CLIPBOARD_READ_OK.store(0, Ordering::SeqCst);
   assert_eq!(platform.clipboard_get(), None);
   CLIPBOARD_READ_OK.store(1, Ordering::SeqCst);
}

#[test]
fn standard_paths_use_host_owned_sandbox_directories()
{
   let platform = IosPlatform::new();
   assert_eq!(
      platform.paths().get(StandardPath::Documents),
      "/app/Library/Application Support/Oxide"
   );
   assert_eq!(PATH_KIND.load(Ordering::SeqCst), 0);
   assert_eq!(platform.paths().get(StandardPath::Cache), "/app/Library/Caches/Oxide");
   assert_eq!(PATH_KIND.load(Ordering::SeqCst), 1);
   assert_eq!(platform.paths().get(StandardPath::Temporary), "/app/tmp/Oxide");
   assert_eq!(PATH_KIND.load(Ordering::SeqCst), 2);
}

#[test]
fn native_standard_paths_append_the_oxide_directory_before_creation()
{
   let source = include_str!(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/src/ios/host_services.m"
   ));
   let implementation = source
      .split("int32_t oxide_host_standard_path")
      .nth(1)
      .expect("standard path host implementation")
      .split("static UIImpactFeedbackGenerator *")
      .next()
      .expect("standard path host implementation end");
   let append = implementation
      .find("URLByAppendingPathComponent:@\"Oxide\" isDirectory:YES")
      .expect("standard paths append the Oxide-owned directory");
   let create = implementation
      .find("createDirectoryAtURL:url")
      .expect("standard paths create their returned directory");

   assert!(append < create);
}

#[test]
fn telephony_reports_no_home_country_without_a_supported_ios_source()
{
   let platform = IosPlatform::new();
   assert_eq!(platform.telephony().home_country_iso_code(), None);
}

#[test]
fn network_status_tracks_reachability_and_notifies_subscribers()
{
   let platform = IosPlatform::new();
   let service = platform.network_status();
   let initial = service.current_status();
   assert!(initial.is_connected);
   assert_eq!(initial.interfaces, NetworkInterface::WIFI);
   assert_eq!(REACHABILITY_STARTS.load(Ordering::SeqCst), 1);

   let observed = Arc::new(Mutex::new(Vec::<NetworkStatus>::new()));
   let observed_for_callback = Arc::clone(&observed);
   service.subscribe(Box::new(move |status|
   {
      observed_for_callback
         .lock()
         .unwrap_or_else(std::sync::PoisonError::into_inner)
         .push(status);
   }));
   emit_reachability(1, 1, 1);
   emit_reachability(1, 3, 0);
   emit_reachability(0, 3, 0);

   let observed = observed.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
   assert_eq!(observed.len(), 4);
   assert_eq!(observed[0].interfaces, NetworkInterface::WIFI);
   assert_eq!(observed[1].interfaces, NetworkInterface::CELLULAR);
   assert!(observed[2].is_connected);
   assert!(observed[2].interfaces.is_empty());
   assert!(!observed[3].is_connected);
   assert!(observed[3].interfaces.is_empty());
}
