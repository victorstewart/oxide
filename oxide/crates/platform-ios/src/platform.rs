//! Production iOS `Platform` aggregate.

use std::sync::{Arc, Mutex, Once};

use once_cell::sync::Lazy;
use oxide_networking::{NetworkPathKind, ReachabilityManager, ReachabilitySnapshot, ReachabilityState, ReachabilitySubscription};
use oxide_platform_api as api;
use oxide_platform_apple::{
   apple_bluetooth_with_restoration, AppleBluetooth, AppleCameraManager, AppleHttpClient,
   AppleLocationService, AppleMotionService, ApplePushManager, AppleSecureStorage,
   AppleSocketNetworking,
};

use crate::{clipboard, IosHaptics, IosPermissions, IosReachability, IosTelephonyService, IosTime};

extern "C"
{
   fn oxide_host_request_redraw();
   fn oxide_host_set_high_refresh(enable: u8);
   fn oxide_host_set_idle_timer_disabled(disabled: u8);
   fn oxide_host_open_system_settings();
   fn oxide_host_open_external_url(url_ptr: *const u8, url_len: usize) -> ::libc::c_int;
   fn oxide_host_ime_show();
   fn oxide_host_ime_hide();
   fn oxide_host_max_framerate_hz() -> u32;
   fn oxide_host_native_scale() -> f32;
   fn oxide_host_supports_edr() -> u8;
   fn oxide_host_is_simulation() -> u8;
   fn oxide_host_standard_path(kind: u32, out_ptr: *mut *mut u8, out_len: *mut usize) -> ::libc::c_int;
   fn oxide_host_string_free(ptr: *mut u8);
}

static IOS_HAPTICS: Lazy<Arc<IosHaptics>> = Lazy::new(|| Arc::new(IosHaptics));
static IOS_PERMISSIONS: IosPermissions = IosPermissions;
static IOS_CAMERA: AppleCameraManager = AppleCameraManager;
static IOS_BLUETOOTH: AppleBluetooth = AppleBluetooth::new();
static IOS_LOCATION: AppleLocationService = AppleLocationService::new();
static IOS_MOTION: AppleMotionService = AppleMotionService::new();
static IOS_PUSH: ApplePushManager = ApplePushManager::new();
static IOS_NETWORKING: AppleSocketNetworking = AppleSocketNetworking::new();
static IOS_HTTP: AppleHttpClient = AppleHttpClient::new();
static IOS_PATHS: IosPaths = IosPaths;
static IOS_SECURE_STORAGE: AppleSecureStorage = AppleSecureStorage::new();
static IOS_TIME: IosTime = IosTime;
static IOS_TELEPHONY: IosTelephonyService = IosTelephonyService;
static IOS_MEDIA_LIBRARY: UnsupportedIosMediaLibrary = UnsupportedIosMediaLibrary;
static IOS_NETWORK_STATUS: Lazy<IosNetworkStatus> = Lazy::new(IosNetworkStatus::new);

/// iOS implementation of the cross-platform Oxide service aggregate.
#[derive(Debug, Default, Clone, Copy)]
pub struct IosPlatform;

impl IosPlatform
{
   /// Creates a standalone aggregate. Process-global service registration is explicit.
   #[must_use]
   pub const fn new() -> Self
   {
      Self
   }
}

impl api::clipboard::ClipboardProvider for IosPlatform
{
   fn read_string(&self) -> Option<String>
   {
      clipboard::get()
   }

   fn write_string(&self, value: &str)
   {
      clipboard::set(value);
   }
}

impl api::Platform for IosPlatform
{
   fn run_app(&self, app: Box<dyn api::App>) -> !
   {
      // UIApplication is already running and the host owns event delivery. Keep
      // the app alive without polling or consuming a frame-time budget.
      let _owned_app = app;
      loop
      {
         std::thread::park();
      }
   }

   fn request_redraw(&self)
   {
      unsafe
      {
         oxide_host_request_redraw();
      }
   }

   fn set_high_refresh(&self, enable: bool)
   {
      unsafe
      {
         oxide_host_set_high_refresh(u8::from(enable));
      }
   }

   fn set_idle_timer_disabled(&self, disabled: bool)
   {
      unsafe
      {
         oxide_host_set_idle_timer_disabled(u8::from(disabled));
      }
   }

   fn open_system_settings(&self)
   {
      unsafe
      {
         oxide_host_open_system_settings();
      }
   }

   fn open_external_url(&self, url: &str) -> Result<(), api::PlatformError>
   {
      if url.is_empty()
      {
         return Err(api::PlatformError::Invalid("external URL is empty"));
      }
      let opened = unsafe
      {
         oxide_host_open_external_url(url.as_ptr(), url.len())
      };
      if opened == 0
      {
         Err(api::PlatformError::Unsupported("iOS rejected external URL"))
      }
      else
      {
         Ok(())
      }
   }

   fn clipboard_get(&self) -> Option<String>
   {
      clipboard::get()
   }

   fn clipboard_set(&self, value: &str)
   {
      clipboard::set(value);
   }

   fn ime_show(&self)
   {
      unsafe
      {
         oxide_host_ime_show();
      }
   }

   fn ime_hide(&self)
   {
      unsafe
      {
         oxide_host_ime_hide();
      }
   }

   fn device_caps(&self) -> api::DeviceCaps
   {
      let native_scale = unsafe
      {
         oxide_host_native_scale()
      };
      api::DeviceCaps
      {
         max_framerate_hz: unsafe
         {
            oxide_host_max_framerate_hz()
         }.max(60),
         supports_edr: unsafe
         {
            oxide_host_supports_edr() != 0
         },
         supports_msaa4x: true,
         native_scale: if native_scale.is_finite() && native_scale > 0.0
         {
            native_scale
         }
         else
         {
            1.0
         },
         color_space: api::ColorSpace::Srgb,
         a11y_reduce_motion: false,
      }
   }

   fn haptics(&self) -> Arc<dyn api::Haptics + Send + Sync>
   {
      IOS_HAPTICS.clone()
   }

   fn is_simulation(&self) -> bool
   {
      unsafe
      {
         oxide_host_is_simulation() != 0
      }
   }

   fn permissions(&self) -> &dyn api::Permissions
   {
      &IOS_PERMISSIONS
   }

   fn camera(&self) -> &dyn api::CameraManager
   {
      &IOS_CAMERA
   }

   fn bluetooth(&self) -> &dyn api::Bluetooth
   {
      &IOS_BLUETOOTH
   }

   fn bluetooth_with_restoration(&self, restore_id: &str) -> Box<dyn api::Bluetooth>
   {
      Box::new(apple_bluetooth_with_restoration(restore_id))
   }

   fn location(&self) -> &dyn api::LocationService
   {
      &IOS_LOCATION
   }

   fn motion(&self) -> &dyn api::MotionService
   {
      &IOS_MOTION
   }

   fn push(&self) -> &dyn api::PushManager
   {
      &IOS_PUSH
   }

   fn capabilities(&self) -> api::Capabilities
   {
      api::Capabilities::CAMERA
         | api::Capabilities::CAMERA_RECORDING
         | api::Capabilities::BLUETOOTH
         | api::Capabilities::PUSH
         | api::Capabilities::MOTION
         | api::Capabilities::LOCATION
   }

   fn networking(&self) -> &dyn api::Networking
   {
      &IOS_NETWORKING
   }

   fn http(&self) -> &dyn api::HttpClient
   {
      &IOS_HTTP
   }

   fn paths(&self) -> &dyn api::PathService
   {
      &IOS_PATHS
   }

   fn secure_storage(&self) -> &dyn api::SecureStorage
   {
      &IOS_SECURE_STORAGE
   }

   fn time(&self) -> &dyn api::TimeService
   {
      &IOS_TIME
   }

   fn telephony(&self) -> &dyn api::telephony::TelephonyService
   {
      &IOS_TELEPHONY
   }

   fn media_library(&self) -> &dyn api::media_library::MediaLibrary
   {
      &IOS_MEDIA_LIBRARY
   }

   fn network_status(&self) -> &dyn api::network_status::NetworkStatusService
   {
      &*IOS_NETWORK_STATUS
   }
}

struct IosPaths;

struct UnsupportedIosMediaLibrary;
type UnsupportedMediaFuture<'a, T> = std::pin::Pin<
   Box<dyn std::future::Future<Output = Result<T, api::PlatformError>> + Send + 'a>,
>;

fn unsupported_media<T>() -> UnsupportedMediaFuture<'static, T>
{
   Box::pin(async {
      Err(api::PlatformError::Unsupported(
         "iOS media library provider is not installed",
      ))
   })
}

impl api::media_library::MediaLibrary for UnsupportedIosMediaLibrary
{
   fn query_assets(
      &self,
      _asset_type: api::media_library::AssetType,
      _limit: u32,
      _offset: u32,
   ) -> UnsupportedMediaFuture<'_, Vec<api::media_library::MediaAsset>>
   {
      unsupported_media()
   }

   fn request_image_data(
      &self,
      _id: &api::media_library::AssetId,
      _quality: api::media_library::ImageQuality,
   ) -> UnsupportedMediaFuture<'_, api::media_library::AssetData>
   {
      unsupported_media()
   }

   fn request_video_data(
      &self,
      _id: &api::media_library::AssetId,
   ) -> UnsupportedMediaFuture<'_, api::media_library::AssetData>
   {
      unsupported_media()
   }
}

impl api::PathService for IosPaths
{
   fn get(&self, path: api::StandardPath) -> String
   {
      let kind = match path
      {
         api::StandardPath::Documents => 0,
         api::StandardPath::Cache => 1,
         api::StandardPath::Temporary => 2,
      };
      host_standard_path(kind).unwrap_or_else(fallback_temporary_path)
   }
}

fn host_standard_path(kind: u32) -> Option<String>
{
   let mut ptr = std::ptr::null_mut();
   let mut len = 0;
   let ok = unsafe
   {
      oxide_host_standard_path(kind, &mut ptr, &mut len)
   };
   if ok == 0 || ptr.is_null() || len == 0
   {
      if !ptr.is_null()
      {
         unsafe
         {
            oxide_host_string_free(ptr);
         }
      }
      return None;
   }
   let value = unsafe
   {
      std::slice::from_raw_parts(ptr, len)
   };
   let path = String::from_utf8(value.to_vec()).ok();
   unsafe
   {
      oxide_host_string_free(ptr);
   }
   path
}

fn fallback_temporary_path() -> String
{
   let mut path = std::env::temp_dir();
   path.push("Oxide");
   let _ = std::fs::create_dir_all(&path);
   path.to_string_lossy().into_owned()
}

type NetworkStatusCallback = Arc<Mutex<Box<dyn Fn(api::network_status::NetworkStatus) + Send>>>;

struct IosNetworkStatus
{
   manager: Arc<ReachabilityManager>,
   reachability: IosReachability,
   started: Once,
   subscriptions: Mutex<Vec<ReachabilitySubscription>>,
}

impl IosNetworkStatus
{
   fn new() -> Self
   {
      let manager = Arc::new(ReachabilityManager::with_default_clock());
      let reachability = IosReachability::new(Arc::clone(&manager));
      Self
      {
         manager,
         reachability,
         started: Once::new(),
         subscriptions: Mutex::new(Vec::new()),
      }
   }

   fn ensure_started(&self)
   {
      self.started.call_once(||
      {
         let _ = self.reachability.start();
      });
   }
}

impl api::network_status::NetworkStatusService for IosNetworkStatus
{
   fn current_status(&self) -> api::network_status::NetworkStatus
   {
      self.ensure_started();
      network_status(self.manager.snapshot())
   }

   fn subscribe(&self, callback: Box<dyn Fn(api::network_status::NetworkStatus) + Send>)
   {
      self.ensure_started();
      let callback: NetworkStatusCallback = Arc::new(Mutex::new(callback));
      let callback_for_subscription = Arc::clone(&callback);
      let subscription = self.manager.subscribe(move |snapshot|
      {
         let callback = callback_for_subscription
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
         callback(network_status(snapshot));
      });
      self.subscriptions
         .lock()
         .unwrap_or_else(std::sync::PoisonError::into_inner)
         .push(subscription);
   }
}

fn network_status(snapshot: ReachabilitySnapshot) -> api::network_status::NetworkStatus
{
   match snapshot.state
   {
      ReachabilityState::Offline => api::network_status::NetworkStatus
      {
         is_connected: false,
         interfaces: api::network_status::NetworkInterface::empty(),
      },
      ReachabilityState::Online { path } =>
      {
         let interfaces = match path.kind
         {
            NetworkPathKind::Wifi => api::network_status::NetworkInterface::WIFI,
            NetworkPathKind::Cellular => api::network_status::NetworkInterface::CELLULAR,
            NetworkPathKind::Wired => api::network_status::NetworkInterface::WIRED,
            NetworkPathKind::Other => api::network_status::NetworkInterface::empty(),
         };
         api::network_status::NetworkStatus
         {
            is_connected: true,
            interfaces,
         }
      }
   }
}

/// Installs iOS services into the process-global Oxide registries.
pub fn install_current_platform() -> Arc<IosPlatform>
{
   let platform = Arc::new(IosPlatform::new());
   let shared: Arc<dyn api::Platform + Send + Sync> = platform.clone();
   api::set_current_platform(shared);
   let clipboard: Arc<dyn api::clipboard::ClipboardProvider> = platform.clone();
   api::clipboard::set_clipboard_provider(clipboard);
   platform
}

/// Returns a standalone iOS platform aggregate without installing it globally.
#[must_use]
pub const fn platform() -> IosPlatform
{
   IosPlatform::new()
}
