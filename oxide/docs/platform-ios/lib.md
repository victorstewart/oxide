# platform-ios `lib.rs`

## Intention and purpose
- Provide Oxide-owned Apple/iOS implementations for generic OS services.
- Keep app crates focused on app-specific policy while generic media-library, telephony, time, and secure-storage behavior lives in the framework layer.

## Relation to the rest of the code
- `oxide-platform-api` defines the traits this crate implements.
- App hosts export narrow `oxide_*` FFI entrypoints from Objective-C or Swift.
- App crates such as Nametag depend on these implementations through Oxide instead of duplicating iOS service code locally.

## Entry points list
- `IosMediaLibraryManager`
  Generic Photos asset query/load bridge, including raw BGRA image extraction for app-specific post-processing.
- `IosTelephonyService`
  Generic carrier/home-country ISO lookup service.
- `IosSecureStorage`
  Generic iOS Keychain-backed secure save/load/delete service.
- `IosTime`
  Monotonic clock bridge for animation/runtime timing.

## Logic narrative
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
- 2026-03-12: absorbed generic iOS media-library, telephony, and secure-storage ownership from Nametag host code, leaving only app-specific media staging in the app layer.
