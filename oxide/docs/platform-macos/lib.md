# platform-macos `lib.rs`

## Intention and purpose
- Provide the macOS host implementation of Oxide's `Platform` trait for local host apps and smoke environments.

## Relation to the rest of the code
- Supplies a concrete `Platform` implementation to Oxide host code.
- Shares Apple secure-storage and network path decoding with iOS through `oxide-platform-apple`.
- Shares Apple HTTP, TCP/UDP networking, camera, location, motion, media-library, push, and WebView Rust service state through `oxide-platform-apple`.
- Uses explicit unsupported paths for individual platform features that are not production-ready yet while still satisfying the full `Platform` trait surface.

## Entry points list
- `MacPlatform`
  Concrete macOS `Platform` implementation.
- `MacHaptics`
  Haptics adapter used by `MacPlatform`, backed by `NSHapticFeedbackManager`.
- `MacNetworkStatus`
  Network.framework-backed status service that reports connectivity and active interface classes.
- `MacPermissions`
  Native macOS permission service for notifications, location, camera, contacts, Bluetooth, microphone, and media library status/request paths.
- `AppleHttpClient`
  Shared Apple HTTP client exposed through `MacPlatform::http()`.
- `AppleLocationService`
  Shared Apple location service exposed through `MacPlatform::location()` and backed by the AppKit host's CoreLocation shim.
- `AppleMotionService`
  Shared Apple motion service exposed through `MacPlatform::motion()`; macOS currently reports no native motion provider.
- `AppleMediaLibraryManager`
  Shared Apple media-library service exposed through `MacPlatform::media_library()` and backed by the AppKit host's Photos shim.
- `ApplePushManager`
  Shared Apple push manager exposed through `MacPlatform::push()` and backed by the AppKit host's APNs/UserNotifications shim.
- `AppleBluetooth`
  Shared Apple Bluetooth manager exposed through `MacPlatform::bluetooth()` and backed by the shared CoreBluetooth bridge.
- `AppleCameraManager`
  Shared Apple camera manager exposed through `MacPlatform::camera()` and backed by the AppKit host's AVFoundation capture shim.
- `AppleWebViewService`
  Shared Apple WebView service exposed through `MacPlatform::web_view_service()` and backed by the AppKit host's WebKit shim.
- `AppleSocketNetworking`
  Shared Apple raw TCP/UDP plus TCP keepalive service exposed through `MacPlatform::networking()`.
- `MacTelephony`
  macOS no-provider telephony implementation; returns `None` for home cellular country because macOS has no cellular-provider telephony API.

## Logic narrative
- `MacPlatform::run_app` parks the caller because the AppKit host owns the process run loop before Rust platform services are installed.
- `MacPlatform` forwards redraw, refresh-rate, idle-timer, clipboard, haptic, URL, settings, HTTP, secure-storage, Bluetooth, push, and network-status calls to Objective-C/FFI shims.
- Clipboard reads distinguish missing native string data from a successful empty string, because author code can intentionally set an empty clipboard payload.
- Capabilities advertise hover pointer plus the real Bluetooth and push services. Camera and recording bits are gated by AVFoundation device discovery, while location and motion bits remain gated by host availability.
- Network status starts one persistent `nw_path_monitor_t`, stores the latest status atomically in the host shim, and fans out Oxide `NetworkStatus` snapshots to Rust subscribers.
- Permissions forward through AppKit-hosted FFI to UserNotifications, CoreLocation, AVFoundation, Contacts, CoreBluetooth, and Photos where macOS exposes an API. Motion is reported denied because there is no macOS motion permission service behind the current Oxide motion API. UserNotifications access is guarded so non-bundled host-test processes report notifications as not-determined instead of aborting inside the framework.
- HTTP uses the shared Apple Rust wrapper and a macOS `NSURLSession` native bridge for GET requests.
- Location uses the shared Apple Rust service for callbacks, history, and region tracking, with native CoreLocation manager setup in the AppKit host.
- Motion uses the shared Apple Rust service shape, but the macOS host returns unavailable because the matching CoreMotion altimeter provider is explicitly unavailable on macOS.
- Media library uses the shared Apple Rust service for asset mapping, image/video result handling, buffer release, and host return-code mapping, with native PhotoKit query/load/export work in the AppKit host.
- Push uses the shared Apple Rust manager for token caching, subscriber fanout, badge calls, and delivered-notification clearing, with native APNs registration and UserNotifications delegate delivery in the AppKit host.
- Bluetooth uses the shared Apple Rust manager and shared CoreBluetooth source for powered-state, scan, connect, disconnect, GATT read/write/notify, advertising, restoration, cache updates, and permission event fanout.
- Camera uses the shared Apple Rust manager for stream subscribers, audio subscriber detection, capture-setting forwarding, recording/photo callback fanout, and NV12/audio sample conversion. The AppKit host owns the AVFoundation session, video/audio outputs, movie-file output, focus point mapping, and format enumeration. Visible preview rendering stays Oxide-owned through frame delivery.
- WebView uses the shared Apple Rust handle/callback wrapper, with native hidden `WKWebView` creation, navigation delegate events, JavaScript evaluation, structured return-code mapping, and teardown in the AppKit host.
- Networking uses the shared Apple socket implementation for raw TCP streams, Apple TCP keepalive socket options, and UDP sockets. QUIC, TLS-on-raw-TCP, and TCP Fast Open remain explicit unsupported paths until a real Network.framework or equivalent transport implementation lands, and those rejections are verified through the installed macOS platform.
- Telephony is modeled as a production no-provider service instead of an unsupported stub.
- The recent change keeps `device_caps`, `open_system_settings`, shared `Arc`-backed haptics, secure storage, and Network.framework status aligned with the expanded core `Platform` trait.

## Preconditions and postconditions
- Host FFI shims must be available for redraw, refresh-rate, clipboard, free, haptics, HTTP, secure storage, camera, location, motion, media-library, push, WebView, permissions, and network-status calls.

## Edge cases and failure modes
- Unsupported services return `PlatformError::Unsupported` rather than pretending to work.
- `run_app` never returns, but it parks instead of spinning so it does not consume a core when AppKit is already running.
- Clipboard reads return `None` only when native pasteboard string data is missing or invalid; a successful zero-length native result returns `Some("")`.
- Location no-prompt checks avoid starting CoreLocation by default. A strict opt-in `OXIDE_MACOS_LIVE_LOCATION=1` path validates start, request-once, update callback delivery, cached last reading, history, and stop only when Location permission is already authorized and Location Services are enabled.
- Motion start returns `PlatformError::Unsupported("motion unavailable")` on macOS because no matching native provider is exposed.
- Camera startup requires existing macOS camera authorization and an available AVFoundation capture device. Native return codes distinguish permission denied, missing device/session, busy, invalid input, and unsupported controls. Zoom is reported unsupported because `AVCaptureDevice.videoZoomFactor` is unavailable on macOS, and flash/torch only accept off because typical macOS capture devices do not expose those controls. These no-prompt unsupported/control paths, inactive photo/recording missing-session errors, and unauthorized startup permission mapping are host-verified through the installed platform. A strict opt-in host path covers live frame delivery, preview-photo capture, and audio-disabled recording when `OXIDE_MACOS_LIVE_CAMERA=1` is set on a pre-authorized camera host.
- Media-library calls require Photos authorization and return permission denied until the app has access. Invalid arguments, copy/export I/O failures, unsupported conversion paths, and missing assets are reported as distinct platform errors. No-prompt query/missing-asset behavior is host-verified through the installed platform, and authorized image-thumbnail extraction is exercised when Photos access and image assets are already available. A strict opt-in host path covers display image extraction and video file resolution/export when `OXIDE_MACOS_LIVE_MEDIA=1` is set on a pre-authorized Photos library with image and video assets.
- Push registration is asynchronous and requires app entitlement/provisioning for live APNs device-token delivery. Non-bundled host-test registration/token/badge/clear paths are host-verified and do not invent a token.
- Notification push/permission paths require an app-bundle identity before touching `UNUserNotificationCenter`; without one, registration becomes a no-op permission snapshot and notification status remains not-determined.
- WebView script execution requires an existing live view handle and returns not-found after close. Invalid input, unavailable WebKit creation, busy duplicate handles, JavaScript failures, and result-copy I/O failures are reported distinctly. The host harness covers local HTML navigation, concurrent hidden views, independent callbacks, JavaScript result/error variants, idempotent close, closed-handle errors, and sibling-view survival through live hidden `WKWebView` instances.
- Raw TCP/UDP networking uses background reader threads and is loopback-verified through the installed macOS platform; TCP keepalive is configured through Apple socket options and loopback-verified through the installed platform; QUIC, TLS-on-raw-TCP, and TCP Fast Open return `PlatformError::Unsupported` and are host-verified through the installed platform.

## Concurrency and memory behavior
- `run_app` blocks by parking the caller thread and performs no allocations in its idle path.
- Haptics are shared through one lazily initialized `Arc` and dispatch AppKit feedback requests onto the main queue.
- Clipboard result buffers are owned by the native host and released through `macos_free` after Rust copies them.
- Other stub services are static singletons.

## Performance notes
- The `run_app` fallback has no busy loop; it parks the thread while AppKit continues owning the live application loop.
- Clipboard and haptic calls are thin FFI wrappers and not on hot render paths.
- Network status callbacks are platform-service callbacks and do not touch renderer frame encode/present paths.
- Camera frames are copied at the native capture boundary into Oxide-owned NV12/audio sample payloads; renderer encode/present code remains unchanged by this backend wiring.
- Location and motion callbacks update bounded Rust service state and do not touch renderer frame encode/present paths.

## Feature flags and cfgs
- This crate is specific to macOS hosts.

## Testing and benchmarks
- Compiled by `cargo check -p oxide-platform-macos --locked`.
- Unit-style platform behavior is covered by `cargo test -p oxide-platform-macos --tests --locked`, including clipboard empty-string handling, clipboard text round-trip, device-cap fallback sanitization, and host-gated capability bits.
- Host linkage, lifecycle, redraw/refresh/idle-timer/IME controls, standard paths, monotonic time, device caps, capabilities, telephony no-provider, haptics dispatch, Bluetooth cache shape, native loopback HTTP GET, live Keychain secure-storage round-trip/delete, installed-platform TCP/UDP loopback networking, installed-platform TCP keepalive, installed-platform unsupported transport rejection, permission-status snapshots, network-status snapshots/subscriptions, no-prompt location/motion/camera/push/media-library behavior, opt-in live location update behavior, opt-in live camera frame/photo/recording behavior, opt-in live media image/video extraction behavior, and live hidden-WebView lifecycle/script behavior are covered by `cargo test -p oxide-host-macos --features host-testing --tests --locked`.
- Live macOS location validation is available through `OXIDE_MACOS_LIVE_LOCATION=1 cargo test -p oxide-host-macos --features host-testing --test web_view_harness --locked`, but live location permission prompts still require manual or UI-test validation.
- Shared camera manager behavior is covered by `cargo test -p oxide-platform-apple --locked`; live macOS camera frame/photo/recording validation is available through `OXIDE_MACOS_LIVE_CAMERA=1 cargo test -p oxide-host-macos --features host-testing --test web_view_harness --locked`, but live permission prompts still require manual or UI-test validation against real hardware.
- Live macOS media-library image/video validation is available through `OXIDE_MACOS_LIVE_MEDIA=1 cargo test -p oxide-host-macos --features host-testing --test web_view_harness --locked`, but live Photos permission prompts still require manual or UI-test validation.

## Examples
```rust
let platform = oxide_platform_macos::MacPlatform;
platform.request_redraw();
```

## Changelog
- 2026-05-19: added shared Apple TCP keepalive support and installed-platform loopback verification.
- 2026-05-19: added opt-in host-verified live CoreLocation update validation for pre-authorized macOS hosts.
- 2026-05-19: expanded host-verified WebView lifecycle/script validation through concurrent hidden `WKWebView` instances.
- 2026-05-19: added host-verified authorized media-library thumbnail extraction and opt-in live media image/video extraction validation.
- 2026-05-19: added host-verified no-prompt camera missing-session/startup permission mapping and opt-in live camera frame/photo/recording validation.
- 2026-05-19: verified installed-platform unsupported transport rejection for raw TCP TLS, TCP Fast Open, and QUIC.
- 2026-05-19: verified installed-platform safe host controls for redraw, refresh, idle timer, IME no-ops, and simulation reporting.
- 2026-05-19: verified installed-platform paths, monotonic time, device caps, capability coherence, telephony no-provider, haptics dispatch, and Bluetooth cache shape.
- 2026-05-19: verified no-prompt media-library query and missing-asset request behavior through the installed macOS platform.
- 2026-05-31: removed the public native-preview contract; macOS camera verification now focuses on Oxide-owned frame delivery and unsupported hardware controls.
- 2026-05-19: guarded UserNotifications access for non-bundled host-test processes and verified permission/network-status snapshots through the main-thread host harness.
- 2026-05-19: verified macOS WebView creation, local navigation, and JavaScript execution through a main-thread host harness and live hidden `WKWebView`.
- 2026-05-19: verified installed-platform raw TCP and UDP networking through macOS host loopback tests.
- 2026-05-19: verified macOS HTTP GET through the native `NSURLSession` host bridge and a loopback server.
- 2026-05-19: verified macOS secure storage against the live Keychain through the host harness.
- 2026-05-19: parked the macOS `run_app` fallback and added platform-macos integration coverage for clipboard/device-cap/capability behavior.
- 2026-05-19: preserved structured macOS WebView create/script error categories through the shared Apple WebView service.
- 2026-05-19: preserved structured macOS media-library error categories through the shared Apple media manager.
- 2026-05-19: gated macOS camera capability bits on AVFoundation camera discovery and preserved structured native camera error categories.
- 2026-05-19: exposed shared Apple camera manager from `MacPlatform`, backed by a macOS AVFoundation host ABI.
- 2026-05-19: exposed shared Apple Bluetooth from `MacPlatform`, advertised Bluetooth/push capabilities, and replaced the haptics no-op with `NSHapticFeedbackManager`.
- 2026-05-19: exposed shared Apple TCP/UDP socket networking from `MacPlatform`.
- 2026-05-19: exposed shared Apple WebView service from `MacPlatform`, backed by a macOS WebKit host ABI.
- 2026-05-19: exposed shared Apple push manager from `MacPlatform`, backed by a macOS APNs/UserNotifications host ABI.
- 2026-05-19: exposed shared Apple media-library service from `MacPlatform`, backed by a macOS PhotoKit host ABI.
- 2026-05-19: exposed shared Apple location/motion services from `MacPlatform`, backed location with CoreLocation, and kept motion as a macOS no-provider service.
- 2026-05-19: added shared Apple HTTP and secure storage, process-global platform installation, Network.framework-backed network status with interface classes, native macOS permission status/request plumbing, and a macOS no-provider telephony service.
- 2026-03-12: aligned the macOS host implementation with the expanded core `Platform` trait by providing `device_caps`, `open_system_settings`, and shared-`Arc` haptics accessors.
