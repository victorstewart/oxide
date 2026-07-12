# platform-apple `lib.rs`

## Intention and purpose
- Hold small Apple-family adapters that are identical across iOS and macOS.
- Keep shared Rust ownership in one crate while native UIKit/AppKit or framework calls remain in the platform hosts.

## Relation to the rest of the code
- `oxide-platform-ios` re-exports `AppleSecureStorage` as `IosSecureStorage`.
- `oxide-platform-macos` uses `AppleSecureStorage` for its `Platform::secure_storage()` implementation.
- iOS and macOS secure storage use the same Rust wrapper and shared native Keychain bridge in `src/apple/secure_storage.m`.
- iOS reachability and macOS network status both use the Apple path/interface decoding helpers so raw Network.framework values map to Oxide types consistently.
- iOS and macOS permission bridges use the same raw domain/status conversion helpers so native permission callbacks have one Rust-side ABI mapping.
- iOS and macOS location services use the same Rust-side callback, history, and geofence-region state machine while each host owns native CoreLocation delivery.
- iOS and macOS motion services use the same Rust-side callback/history service; hosts can provide real samples where the OS exposes a matching provider.
- iOS and macOS HTTP GET use the same Rust wrapper and shared native `NSURLSession` bridge in `src/apple/http.m`.
- iOS and macOS media-library services use the same Rust-side `MediaLibrary` implementation over host-owned Photos/native media ABI functions.
- iOS and macOS push services use the same Rust-side token cache, subscriber fanout, badge, and delivered-notification clearing manager over host-owned APNs/UserNotifications ABI functions.
- iOS and macOS Bluetooth services use the same Rust-side CoreBluetooth manager state, callback fanout, cache, scan/connect/GATT/advertising ABI, and restoration entrypoint.
- iOS and macOS camera services use the same Rust-side camera manager, stream subscriber state, recording/photo callbacks, and format recommendation helpers over host-owned AVFoundation ABI functions.
- macOS WebView uses the shared Apple Rust handle/callback wrapper over a host-owned WebKit ABI and is host-verified against a live hidden `WKWebView`.
- iOS and macOS TCP/UDP raw transport can use the shared Apple Rust socket networking implementation where the platform backend exposes it.

## Entry points list
- `oxide_platform_apple::AppleSecureStorage`
  Shared wrapper over the `oxide_secure_storage_*` C ABI.
- `oxide_platform_apple::AppleHttpClient`
  Shared GET-only HTTP client wrapper over the `oxide_host_http_*` C ABI.
- `oxide_platform_apple::AppleLocationService`
  Shared `LocationService` implementation over the `oxide_host_location_*` C ABI.
- `oxide_platform_apple::AppleMotionService`
  Shared `MotionService` implementation over the `oxide_host_motion_*` C ABI.
- `oxide_platform_apple::AppleMediaLibraryManager`
  Shared `MediaLibrary` implementation over the `oxide_media_*` C ABI.
- `oxide_platform_apple::ApplePushManager`
  Shared `PushManager` implementation over the `oxide_host_push_*` C ABI.
- `oxide_platform_apple::AppleBluetooth`
  Shared `Bluetooth` implementation over the `oxide_ble_*` C ABI and `oxide_host_ble_emit_*` callbacks.
- `oxide_platform_apple::AppleCameraManager`
  Shared `CameraManager` implementation over the `oxide_cam_*` and `oxide_host_set_camera_*` C ABI.
- `oxide_platform_apple::AppleWebViewService`
  Feature-gated shared `WebViewService` implementation over the `oxide_web_view_*` C ABI.
- `oxide_platform_apple::AppleSocketNetworking`
  Shared raw TCP stream, TCP keepalive setup, and UDP socket implementation for Apple-family hosts.
- `oxide_platform_apple::reachability_state_from_apple_path(status, iface, expensive)`
  Converts an Apple path snapshot into `oxide_networking::ReachabilityState`.
- `oxide_platform_apple::network_status_from_apple_path(status, iface)`
  Converts a single Apple path kind into `oxide_platform_api::network_status::NetworkStatus`.
- `oxide_platform_apple::network_status_from_apple_interface_mask(connected, interface_mask)`
  Converts a macOS/iOS interface bitmask into Oxide network status.
- `oxide_platform_apple::permission_domain_to_apple_code(domain)` and `permission_domain_from_apple_code(code)`
  Convert Oxide permission domains to and from the shared Apple host ABI.
- `oxide_platform_apple::permission_status_to_apple_code(status)` and `permission_status_from_apple_code(code)`
  Convert Oxide permission status values to and from the shared Apple host ABI.

## Logic narrative
- The secure-storage adapter owns Rust-side status-code handling. The shared native Keychain bridge owns generic-password save/load/delete, host-buffer allocation, and host-buffer release for both iOS and macOS.
- The HTTP adapter owns Rust-side request validation and event mapping. One shared native `NSURLSession` delegate owns streaming response delivery and cancellation for both iOS and macOS.
- The location adapter owns Rust-side last-sample caching, bounded history, callback fanout, and geofence-region enter/exit detection; native hosts own CoreLocation manager configuration and permission prompts.
- The motion adapter owns Rust-side bounded history and callback fanout; unavailable host providers return `PlatformError::Unsupported` through the shared service.
- The media-library adapter owns Rust-side paging, asset mapping, image/video result conversion, optional BGRA helper loading, host-buffer release, and host return-code mapping; native hosts own Photos authorization and data extraction.
- The push adapter owns Rust-side token caching, notification JSON-to-user-info conversion, subscriber fanout, badge calls, and delivered-notification clearing; native hosts own APNs registration and UserNotifications delegate delivery.
- The Bluetooth adapter owns Rust-side initialization, subscriber fanout, discovered-peripheral cache, scan option marshaling, GATT read/write/notify calls, advertising calls, and state/restoration callbacks; the shared native CoreBluetooth source owns OS manager/delegate behavior.
- The camera adapter owns Rust-side stream subscriber lists, audio subscriber detection, capture-setting forwarding, recording/photo callback fanout, NV12/audio sample conversion, host return-code mapping, and camera format recommendation helpers; native hosts own AVFoundation sessions, sample delivery, and platform-specific hardware controls.
- The WebView adapter owns Rust-side handle lifetime, callback fanout, script-result copying, close idempotence, and host return-code mapping; native hosts own WebKit view creation, navigation delegates, and JavaScript evaluation.
- The socket networking adapter owns TCP connect/read/write/close, Apple TCP keepalive socket option setup, and UDP bind/send/read/close with background reader threads; QUIC and unsupported TCP options fail explicitly.
- Network decoding uses shared constants for Wi-Fi, cellular, wired, and other path kinds; macOS can report a bitmask when multiple interfaces are active.
- Permission decoding uses shared constants for notifications, location, camera, contacts, Bluetooth, motion, microphone, and media library domains.

## Preconditions and postconditions
- Apple hosts must compile `src/apple/secure_storage.m` or export ABI-compatible `oxide_secure_storage_save`, `oxide_secure_storage_load`, `oxide_secure_storage_delete`, and `oxide_secure_storage_free_data` symbols.
- Apple builds of this crate compile `src/apple/http.m` and export `oxide_host_http_start` and `oxide_host_http_cancel` for `AppleHttpClient` automatically.
- Apple hosts must export ABI-compatible `oxide_host_location_*` symbols to use `AppleLocationService`.
- Apple hosts must export ABI-compatible `oxide_host_motion_*` symbols to use `AppleMotionService`.
- Apple hosts must export ABI-compatible `oxide_media_*` symbols to use `AppleMediaLibraryManager`.
- Apple hosts must export ABI-compatible `oxide_host_push_*` and push callback-registration symbols to use `ApplePushManager`.
- Apple hosts must compile the shared CoreBluetooth bridge or export ABI-compatible `oxide_ble_*` symbols plus `oxide_host_ble_emit_*` callback delivery to use `AppleBluetooth`.
- Apple hosts must export ABI-compatible `oxide_cam_*` and camera callback-registration symbols to use `AppleCameraManager`.
- Apple hosts must export ABI-compatible `oxide_web_view_*` symbols when the `web-view-macos` feature is enabled.
- Network decoding helpers require raw values that follow the Apple bridge constants in this crate.
- Permission helpers require raw values that follow the Apple bridge constants in this crate.

## Edge cases and failure modes
- Missing secure-storage keys map to `Ok(None)`.
- Empty secure-storage values map to `Ok(Some(Vec::new()))`.
- HTTP is intentionally GET-only and returns `PlatformError::Unsupported` for other methods.
- Native HTTP admission returns immediately with an operation handle or a typed validation/admission error.
- Native location start failures map to `PlatformError::Unsupported("location start failed")`.
- Native motion start failures map to `PlatformError::Unsupported("motion unavailable")`.
- Native media permission failures map to `PlatformError::PermissionDenied("media_library")`.
- Native media invalid input, copy/export I/O failures, unsupported conversion paths, and missing assets map to distinct `PlatformError` variants.
- Native media missing assets map to `PlatformError::NotFound`.
- Push registration is asynchronous; `device_token()` returns `None` until the host callback or native token query supplies a token.
- Push notification payloads that are not UTF-8 or JSON objects are ignored before subscriber fanout.
- Bluetooth initialization is process-global. The first plain or restoration initializer wins, matching CoreBluetooth's singleton-manager model.
- Bluetooth cached peripherals are bounded and updated on discovery, connection, disconnection, and notification events.
- Camera startup/control failures preserve host return-code categories for permission denied, missing device, busy, invalid, I/O, and unsupported cases. Recording/photo callback failures preserve their event status categories, and visible preview paths require app-visible frame subscribers so Oxide owns composition and presentation.
- WebView create rejects empty URLs, script execution rejects empty scripts, and closed handles return not-found.
- WebView native failures preserve invalid input, unavailable service, busy duplicate handles, missing handles, script failures, and result-copy I/O failures.
- Socket networking configures TCP keepalive through Apple socket options and rejects TLS, TCP Fast Open, and QUIC requests instead of silently ignoring unsupported transport semantics.
- Disconnected network status always reports an empty interface set.

## Testing and benchmarks
- Covered by `crates/platform-apple/tests/secure_storage_tests.rs`.
- Shared Apple ABI layout and native guard retention are covered by `crates/platform-apple/tests/abi_layout_tests.rs`.
- macOS host-backed HTTP, secure-storage, and raw TCP/UDP networking behavior is additionally exercised by `host/macos-app/oxide-host-macos/tests/headless_harness.rs`, which verifies the shared Apple wrappers through the shared native `NSURLSession` bridge, Keychain, installed-platform loopback socket paths, installed-platform TCP keepalive, and installed-platform unsupported TLS/QUIC/TFO rejection.
- macOS host-backed WebView behavior is additionally exercised by `host/macos-app/oxide-host-macos/tests/web_view_harness.rs`, which verifies live hidden `WKWebView` load callbacks, concurrent view isolation, JavaScript result/error behavior, and teardown through the shared Apple wrapper. The same harness verifies macOS camera missing-session and unauthorized-start error mapping by default, validates authorized media-thumbnail extraction when available, and has opt-in live location/camera/media paths for pre-authorized Location Services, frame/photo/recording, and image/video extraction validation.

## Changelog
- 2026-07-11: made the asynchronous HTTP native bridge self-contained in this crate and removed duplicate host compilation.
- 2026-06-22: added shared Apple ABI layout freeze coverage for HTTP, Bluetooth, camera, location, motion, media, and camera-format structs.
- 2026-05-19: moved the native Apple Keychain secure-storage bridge into `src/apple/secure_storage.m` and compiled it into both iOS and macOS hosts.
- 2026-05-19: moved the native Apple HTTP `NSURLSession` bridge into `src/apple/http.m` and compiled it into both iOS and macOS hosts.
- 2026-05-19: added shared Apple TCP keepalive socket-option support and installed macOS loopback verification.
- 2026-05-19: added opt-in macOS host CoreLocation update validation through the installed platform.
- 2026-05-19: expanded macOS host WebView lifecycle/script coverage through the installed platform.
- 2026-05-19: added macOS host media-library thumbnail validation and opt-in live image/video extraction coverage through the installed platform.
- 2026-05-19: added macOS host camera missing-session/startup validation and opt-in live camera frame/photo/recording coverage through the installed platform.
- 2026-05-19: verified shared Apple socket unsupported transport rejection through the installed macOS platform.
- 2026-05-19: verified the shared Apple WebView wrapper through installed macOS platform local navigation and script execution.
- 2026-05-19: verified the shared Apple TCP/UDP socket backend through installed macOS platform loopback tests.
- 2026-05-19: verified shared Apple HTTP and secure-storage wrappers through macOS host-backed loopback/Keychain tests.
- 2026-05-19: preserved native WebView return-code categories for busy duplicate handles and script-result copy failures.
- 2026-05-19: preserved native media-library return-code categories for invalid input, I/O/export failure, unsupported conversion, permission denied, and missing assets.
- 2026-05-19: preserved native camera return-code categories in the shared camera manager instead of collapsing every host failure to unsupported.
- 2026-05-19: moved shared Apple camera manager state, callbacks, and format recommendation helpers into this crate.
- 2026-05-19: moved shared Apple Bluetooth Rust manager and CoreBluetooth bridge ownership into this crate.
- 2026-05-19: added shared Apple TCP/UDP socket networking.
- 2026-05-19: added feature-gated shared Apple WebView Rust handle/callback behavior.
- 2026-05-19: moved shared Apple push manager behavior into this crate.
- 2026-05-19: moved shared Apple media-library Rust service behavior into this crate.
- 2026-05-19: moved shared Apple location/motion Rust service state into this crate.
- 2026-05-19: added shared secure-storage and HTTP ABI handling plus shared Apple network path/status decoding.
- 2026-05-19: added shared Apple permission domain/status decoding.
