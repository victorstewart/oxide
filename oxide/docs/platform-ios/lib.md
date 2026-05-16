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
  Generic iOS camera manager for native preview planes, app-visible frame streams, photo capture, recording, and camera-scene texture export. When the optional `native-camera-bridge` feature is enabled, the crate also compiles the Objective-C camera bridge it binds to.
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
- Native preview streams increment a separate preview lease count and start the host bridge with frame delivery disabled. If an app-visible stream starts while a native preview is active, the bridge restarts with frame callbacks; when the last app-visible subscriber leaves, the bridge downgrades back to native-preview-only capture when preview leases remain.
- `host_services.m` now owns the shared clipboard, haptics, and permission/update plumbing so host apps can keep only app-shell behavior locally instead of carrying parallel Objective-C utility bridges.
- Media-library permission status remains lazy until an explicit Photos request, and the cached Oxide status is kept separate from the legacy Nametag status because limited Photos access has different status codes across those bridges.
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
- Native preview stop is lease-based: dropping the last preview stream stops capture only when there are no app-visible frame subscribers.

## Testing and benchmarks
- Validated indirectly through downstream workspace builds/tests that consume these services.

## Changelog
- 2026-05-15: added the native camera preview stream path that can run `AVCaptureVideoPreviewLayer` presentation without app-visible camera frame callbacks, while preserving full frame streams for capture and pixel-processing callers.
- 2026-05-15: compacted empty camera-frame stub output clearing in the standalone Metal app host.
- 2026-05-01: split lazy media-library permission caching into Oxide and legacy Nametag status slots so limited Photos access preserves each bridge's ABI mapping.
- 2026-04-20: synchronized `oxide_host_location_last` through the main queue to match CoreLocation delegate ownership and prevent cross-thread reads of the cached native sample.
- 2026-03-20: absorbed shared iOS clipboard, haptics, permissions, and open-URL/settings Objective-C helpers into `src/ios/host_services.m`, deleting the parallel host-local copies from both Oxide host-app and downstream Nametag host code.
- 2026-03-20: absorbed shared iOS Bluetooth, push, and QUIC/reachability Objective-C shims so Oxide and downstream apps stop carrying parallel host-local copies of the same platform bridges.
- 2026-03-20: absorbed the shared iOS camera bridge from Nametag behind the optional `native-camera-bridge` feature so downstream apps can delete duplicated camera host code and use Oxide ownership directly.
- 2026-03-12: absorbed generic iOS media-library, telephony, and secure-storage ownership from Nametag host code, leaving only app-specific media staging in the app layer.
