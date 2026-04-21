# platform-ios `lib.rs`

## Intention and purpose
- Provide Oxide-owned Apple/iOS implementations for generic OS services.
- Keep app crates focused on app-specific policy while generic media-library, telephony, time, and secure-storage behavior lives in the framework layer.

## Relation to the rest of the code
- `oxide-platform-api` defines the traits this crate implements.
- App hosts export narrow `oxide_*` FFI entrypoints from Objective-C or Swift.
- App crates such as Nametag depend on these implementations through Oxide instead of duplicating iOS service code locally.

## Entry points list
- `IosCameraManager`
  Generic iOS camera manager for preview, photo capture, recording, and camera-scene texture export. When the optional `native-camera-bridge` feature is enabled, the crate also compiles the Objective-C camera bridge it binds to.
- Shared native bridges
  `src/ios/bluetooth.m`, `src/ios/host_services.m`, `src/ios/network.m`, and `src/ios/push.m` now provide the shared Objective-C ownership for generic BLE, clipboard/haptics/permissions/open-URL helpers, QUIC/reachability, and APNs plumbing used by both Oxide host apps and downstream apps such as Nametag.
- `IosMediaLibraryManager`
  Generic Photos asset query/load bridge, including raw BGRA image extraction for app-specific post-processing.
- `IosTelephonyService`
  Generic carrier/home-country ISO lookup service.
- `IosSecureStorage`
  Generic iOS Keychain-backed secure save/load/delete service.
- `IosTime`
  Monotonic clock bridge for animation/runtime timing.

## Logic narrative
- `IosCameraManager` now owns the shared iOS camera bridge that downstream apps such as Nametag consume, so app crates should only keep app-specific review/policy hooks above this layer instead of duplicating AVFoundation camera control code locally.
- `host_services.m` now owns the shared clipboard, haptics, and permission/update plumbing so host apps can keep only app-shell behavior locally instead of carrying parallel Objective-C utility bridges.
- The native CoreLocation bridge keeps delegate-owned cached location state on the main queue; synchronous reads of the last sample also cross that queue so `LocationService: Send + Sync` callers do not race the delegate callback.
- Each service owns the platform-specific FFI contract and converts host return codes into `PlatformError`.
- `IosSecureStorage` exposes sync helper methods plus the async `SecureStorage` trait adapter so both direct platform usage and callback-registry compatibility shims share one implementation.
- `IosMediaLibraryManager` keeps the generic asset query/load path in Oxide, while app crates can still layer app-specific crop/runtime-image logic above the raw BGRA helper.

## Preconditions and postconditions
- Host `oxide_*` FFI exports must be installed and ABI-compatible with the Rust declarations in this crate.
- Successful calls return Oxide trait types or platform errors without leaking host-allocated buffers.

## Edge cases and failure modes
- Negative host return codes are converted into structured `PlatformError` values.
- Missing/empty returned buffers are treated as `NotFound` instead of producing invalid slices.

## Testing and benchmarks
- Validated indirectly through downstream workspace builds/tests that consume these services.

## Changelog
- 2026-04-20: synchronized `oxide_host_location_last` through the main queue to match CoreLocation delegate ownership and prevent cross-thread reads of the cached native sample.
- 2026-03-20: absorbed shared iOS clipboard, haptics, permissions, and open-URL/settings Objective-C helpers into `src/ios/host_services.m`, deleting the parallel host-local copies from both Oxide host-app and downstream Nametag host code.
- 2026-03-20: absorbed shared iOS Bluetooth, push, and QUIC/reachability Objective-C shims so Oxide and downstream apps stop carrying parallel host-local copies of the same platform bridges.
- 2026-03-20: absorbed the shared iOS camera bridge from Nametag behind the optional `native-camera-bridge` feature so downstream apps can delete duplicated camera host code and use Oxide ownership directly.
- 2026-03-12: absorbed generic iOS media-library, telephony, and secure-storage ownership from Nametag host code, leaving only app-specific media staging in the app layer.
