# platform-ios `lib.rs`

## Intention and purpose
- Provide Oxide-owned Apple/iOS implementations for generic OS services.
- Keep app crates focused on app-specific policy while generic media-library, telephony, time, and secure-storage behavior lives in the framework layer.

## Relation to the rest of the code
- `oxide-platform-api` defines the traits this crate implements.
- App hosts export narrow `oxide_*` FFI entrypoints from Objective-C or Swift.
- App crates such as Nametag depend on these implementations through Oxide instead of duplicating iOS service code locally.

## Entry points list
- `IosPlatform`
  Production iOS implementation of `oxide_platform_api::Platform`; it exposes shared Apple services and forwards host controls through the UIKit shell.
- `install_current_platform() -> Arc<IosPlatform>`
  Installs the iOS aggregate in the process-global platform registry and installs the same object as the clipboard provider.
- `platform() -> IosPlatform`
  Returns an unregistered standalone aggregate for explicit embedding and tests.
- `IosCameraManager`
  Alias for the shared `AppleCameraManager`, covering app-visible frame streams, audio samples, photo capture, recording, and camera-scene texture export. When the optional `native-camera-bridge` feature is enabled, the crate also compiles the iOS Objective-C AVFoundation bridge it binds to.
- Shared native bridges
  `../platform-apple/src/apple/bluetooth.m`, `../platform-apple/src/apple/http.m`, `../platform-apple/src/apple/secure_storage.m`, `src/ios/host_services.m`, `src/ios/network.h`, `src/ios/network.m`, and `src/ios/push.m` provide shared Objective-C ownership for generic BLE, HTTP, secure storage, clipboard/haptics/permissions/open-URL helpers, QUIC/reachability, and APNs plumbing.
- `IosMediaLibraryManager`
  Alias for the shared `AppleMediaLibraryManager` for embedders that provide its native ABI. The production aggregate currently returns an explicit unsupported service because no device-verified Photos provider is installed.
- `IosTelephonyService`
  Returns no home-country ISO because current iOS has no supported carrier-country source for this contract.
- `IosSecureStorage`
  Alias for the shared `AppleSecureStorage` wrapper; the shared Apple native Keychain source owns the save/load/delete ABI.
- `IosHttpClient`
  Alias for the shared `AppleHttpClient` wrapper; the shared Apple native HTTP source owns the URLSession bridge.
- `IosNetworking`
  Alias for the shared `AppleSocketNetworking` raw TCP/UDP service.
- `IosLocation`
  Alias for the shared `AppleLocationService`; the iOS native CoreLocation file still owns delegate delivery.
- `IosMotion`
  Alias for the shared `AppleMotionService`; the iOS native motion file still owns sample delivery.
- `IosPushManager`
  Alias for the shared `ApplePushManager`; the iOS native push file still owns APNs registration and UserNotifications delegate delivery.
- `IosBluetooth`
  Alias for the shared `AppleBluetooth`; the shared Apple native CoreBluetooth file owns manager/delegate delivery.
- `IosTime`
  Monotonic clock bridge for animation/runtime timing.
- Standalone Metal app host
  `src/OxideMetalAppHost.m` is the minimal UIKit/CAMetalLayer shell for direct iOS Metal apps. Its app callback ABI is split into prepare, submit, and cancel phases so Rust frame work happens before drawable acquisition and prepared frames can be canceled when `nextDrawable` times out.

## Logic narrative
- `IosPlatform` is the production service aggregate intended for installation before app initialization. UIKit retains lifecycle and presentation ownership while Rust reaches OS services through narrow host calls.
- `IosPlatform::run_app` retains its app argument and parks because UIKit hosts own `UIApplicationMain`; it performs no polling. A host that manages app lifecycle externally installs the aggregate before initializing its Rust app.
- Standard paths come from the native sandbox using Application Support, Caches, and temporary-directory APIs. The host creates each returned directory before Rust copies and frees the UTF-8 result.
- Network status lazily starts one native path monitor, feeds its path updates through `ReachabilityManager`, and retains Rust subscriptions for the lifetime of the process.
- Clipboard reads now preserve the distinction between a failed native read and a successful empty UTF-8 string.
- `IosCameraManager` is an alias for the shared Apple camera Rust manager; iOS keeps the native Objective-C AVFoundation bridge behind the shared ABI while visible preview rendering remains Oxide-owned.
- Diagnostic PreviewLayer comparisons live in the host perf harness, not in the public platform API or UI authoring surface.
- `host_services.m` now owns the shared clipboard, haptics, and permission/update plumbing so host apps can keep only app-shell behavior locally instead of carrying parallel Objective-C utility bridges.
- Media-library permission status remains lazy until an explicit Photos request, and the cached Oxide status is kept separate from the legacy Nametag status because limited Photos access has different status codes across those bridges.
- The native CoreLocation bridge keeps delegate-owned cached location state on the main queue; synchronous reads of the last sample also cross that queue so `LocationService: Send + Sync` callers do not race the delegate callback.
- Rust-side location callback fanout, bounded history, last-sample caching, and geofence-region tracking live in `oxide-platform-apple` and are shared with macOS.
- Rust-side motion callback fanout and bounded pressure history live in `oxide-platform-apple` and are shared with macOS.
- The Network.framework transport bridge honors forced TCP/TLS selection across every retry, because retrying the QUIC parameter set after a forced TCP/TLS failure would silently change the caller-requested transport.
- The native network header owns the complete Oxide-named retained-session and reachability ABI, including frame/idle/terminal polling; the build script watches that header so ABI changes always rebuild the Objective-C bridge.
- Reachability path decoding is shared with macOS through `oxide-platform-apple` so Wi-Fi, cellular, wired, and other path kinds map consistently across Apple hosts.
- Permission domain/status raw-code mapping is shared with macOS through `oxide-platform-apple`; native permission prompts remain platform-specific.
- The HTTP bridge is compiled from `oxide-platform-apple/src/apple/http.m` so iOS and macOS use the same native byte-copy path for response bodies and UTF-8 metadata, while iOS-specific multipath behavior stays platform-gated.
- Each service owns the platform-specific FFI contract and converts host return codes into `PlatformError`.
- `IosSecureStorage` is an alias for the shared `AppleSecureStorage` wrapper and uses the shared Apple native Keychain host ABI behind it.
- `IosHttpClient` is an alias for the shared `AppleHttpClient` wrapper and uses the shared Apple native HTTP host ABI behind it.
- `IosLocation` and `IosMotion` are aliases for shared Apple Rust services; iOS keeps only the native CoreLocation/CoreMotion host ABI behind them.
- `IosMediaLibraryManager` remains an alias for the shared Apple media-library Rust service, but the generic iOS aggregate does not advertise a provider or bind unresolved/stub Photos symbols. A separately verified embed can supply that ABI without changing the platform service boundary.
- `IosPushManager` is an alias for the shared Apple push manager; iOS keeps only the native APNs/UserNotifications host ABI behind it.
- `IosBluetooth` is an alias for the shared Apple Bluetooth manager; iOS compiles the shared CoreBluetooth bridge from `oxide-platform-apple` and keeps the legacy Nametag permission callback only on iOS.
- Camera stream subscriber state, audio subscriber detection, recording/photo callback fanout, and format recommendation helpers live in `oxide-platform-apple` and are shared with macOS.
- The standalone Metal app host keeps `CAMetalLayer` drawable acquisition late and timeout-capable. It calls the app's prepare callback before `nextDrawable`, submits immediately after a drawable is acquired, and calls cancel if no drawable is returned.

## Preconditions and postconditions
- Host `oxide_*` FFI exports must be installed and ABI-compatible with the Rust declarations in this crate.
- Successful calls return Oxide trait types or platform errors without leaking host-allocated buffers.
- A UIKit host embedding an Oxide app must install `IosPlatform` before app initialization so early service lookups and redraw requests resolve to that host.

## Edge cases and failure modes
- Negative host return codes are converted into structured `PlatformError` values.
- Failed or malformed host buffers are rejected instead of producing invalid slices; a successful zero-length clipboard buffer remains `Some("")`.
- A rejected external URL returns `PlatformError::Unsupported`; an empty URL returns `PlatformError::Invalid` without crossing FFI.
- Invalid device scale values fall back to `1.0`, and an unavailable maximum refresh value is clamped to the 60 Hz baseline.
- If native standard-path discovery fails, the service creates and returns an Oxide directory below the process temporary directory.
- Camera stream stop is lease-based: dropping one stream stops capture only when there are no remaining app-visible frame subscribers.
- Media requests fail with `PlatformError::Unsupported` until a production Photos provider is installed and verified independently.
- Telephony home-country lookup returns `None`; it does not bind deprecated carrier APIs or invent a locale-based substitute.

## Testing and benchmarks
- Validated indirectly through downstream workspace builds/tests that consume these services.
- `tests/platform_tests.rs` directly verifies host-control forwarding, device caps, clipboard empty-string behavior, sandbox path mapping, haptic mapping, URL errors, and Reachability-backed status subscription on the native build host.
- `tests/network_bridge_tests.rs` checks Objective-C bridge invariants that are difficult to exercise without a real Network.framework endpoint, including forced TCP/TLS retry routing.
- `tests/camera_capture_mode_tests.rs` checks the shared Apple camera start-mode invariant from `crates/platform-apple/src/lib.rs`.
- `tests/abi_layout_tests.rs` freezes iOS camera, location, motion, and contact ABI layouts plus native camera/location/motion static-assert retention.

## Changelog
- 2026-08-06: made the production aggregate fail closed for media-library data instead of linking benchmark stubs or an unverified Photos provider, and made telephony's unsupported home-country behavior explicit.
- 2026-08-06: added the production `IosPlatform` aggregate, process-global installation, native sandbox paths, host controls/device caps, and Reachability-backed network status; preserved successful empty clipboard reads.
- 2026-07-11: hard-cut the iOS transport/reachability C ABI to Oxide-owned names and explicit caller-owned connection/retry policy.
- 2026-07-10: published the tri-state retained-session polling header and added it to native build dependency tracking.
- 2026-06-22: added iOS platform ABI layout freeze coverage for camera, location, motion, and contact bridge payloads.
- 2026-06-01: split the standalone Metal app host frame ABI into prepare/submit/cancel phases and enabled timeout-capable drawable acquisition so it matches the late-drawable performance contract.
- 2026-06-01: removed stale native visible-preview wording from the platform API docs; diagnostic `AVCaptureVideoPreviewLayer` comparisons remain host-perf-only.
- 2026-05-19: moved the iOS native Keychain secure-storage bridge into shared `oxide-platform-apple`.
- 2026-05-19: moved the iOS native HTTP URLSession bridge into shared `oxide-platform-apple`.
- 2026-05-19: replaced the local Rust camera manager with the shared `AppleCameraManager` alias while keeping the iOS native camera bridge platform-specific.
- 2026-05-19: moved iOS Bluetooth Rust manager and native CoreBluetooth source into shared `oxide-platform-apple`.
- 2026-05-19: moved iOS push Rust manager behavior into shared `oxide-platform-apple`.
- 2026-05-19: moved iOS media-library Rust service behavior into shared `oxide-platform-apple`.
- 2026-05-19: moved iOS location/motion Rust service state into shared `oxide-platform-apple` aliases.
- 2026-05-19: moved secure-storage and HTTP Rust ABI handling plus Apple reachability and permission-code decoding into `oxide-platform-apple`.
- 2026-05-17: compacted the platform-iOS build source selection, HTTP bridge response-copy paths, and TLS/ALPN option setup.
- 2026-05-17: kept forced TCP/TLS Network.framework retries on TLS parameters instead of falling through to the generic QUIC retry path.
- 2026-05-15: compacted empty camera-frame stub output clearing in the standalone Metal app host.
- 2026-05-01: split lazy media-library permission caching into Oxide and legacy Nametag status slots so limited Photos access preserves each bridge's ABI mapping.
- 2026-04-20: synchronized `oxide_host_location_last` through the main queue to match CoreLocation delegate ownership and prevent cross-thread reads of the cached native sample.
- 2026-03-20: absorbed shared iOS clipboard, haptics, permissions, and open-URL/settings Objective-C helpers into `src/ios/host_services.m`, deleting the parallel host-local copies from both Oxide host-app and downstream Nametag host code.
- 2026-03-20: absorbed shared iOS Bluetooth, push, and QUIC/reachability Objective-C shims so Oxide and downstream apps stop carrying parallel host-local copies of the same platform bridges.
- 2026-03-20: absorbed the shared iOS camera bridge from Nametag behind the optional `native-camera-bridge` feature so downstream apps can delete duplicated camera host code and use Oxide ownership directly.
- 2026-03-12: absorbed generic iOS media-library, telephony, and secure-storage ownership from Nametag host code, leaving only app-specific media staging in the app layer.
