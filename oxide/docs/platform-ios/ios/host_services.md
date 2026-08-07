# platform-ios `ios/host_services.m`

## Intention and purpose

- Own iOS platform-service ABI that is independent of a particular application shell.
- Keep production and legacy UIKit hosts from duplicating permission, clipboard, haptic, URL, device-capability, and sandbox-path code.

## Relation to the rest of the code

- `IosPlatform` calls the exported `oxide_host_*` functions through narrow FFI.
- Every UIKit shell links this source through `oxide-platform-ios`; the feature-gated legacy `app.m` therefore shares the same implementation.
- Optional legacy Nametag callbacks are compiled only when that bridge owns the corresponding symbols.

## Logic narrative

- Clipboard strings and sandbox paths cross the ABI as owned UTF-8 buffers reclaimed by `oxide_host_string_free`.
- Permission status queries and prompts stay domain-specific; Bluetooth and Photos remain lazy at bootstrap.
- UI-affecting operations dispatch to the main queue.
- External-URL requests synchronously ask `UIApplication` whether the URL can be opened before invoking `openURL`; the ABI reports failure when the application or handler is unavailable.
- Device capabilities resolve the active connected `UIWindowScene` instead of relying on a shell-global view or deprecated screen singleton.
- Standard paths append the app-owned `Oxide` directory to the selected Application Support, Cache, or temporary root, create that exact directory, and then return it.

## Preconditions and postconditions

- Output pointers must be non-null; successful owned-buffer calls return a matching pointer/length pair.
- Callers free returned strings exactly once through `oxide_host_string_free`.
- Host-independent symbols have exactly one owner in every iOS link.

## Edge cases and failure modes

- Invalid UTF-8 URLs, missing schemes, URLs without an installed handler, unavailable active scenes, and directory-creation failures fail closed.
- Maximum refresh and native scale fall back to `60 Hz` and `1.0` when no connected screen is available.
- Nametag-specific permission symbols are omitted when `OXIDE_PLATFORM_IOS_DISABLE_NAMETAG_BRIDGE` is defined.

## Concurrency and performance

- Main-queue synchronization is used only for infrequent service calls and screen discovery, never per frame.
- Haptic generators are initialized once and reused.
- Permission callbacks dispatch asynchronously and do not hold Rust locks.

## Testing

- `platform-ios/tests/platform_tests.rs` covers host-control forwarding, clipboard ownership, device caps, URL errors, and standard paths.
- `platform-ios/tests/media_library_tests.rs` covers lazy permission-cache and Nametag-bridge behavior only; no Photos data provider is implied.

## Changelog

- 2026-08-06: made every native standard path create and return its promised app-owned `Oxide` subdirectory.
- 2026-08-06: made external-URL success contingent on a synchronous main-thread `canOpenURL` check instead of reporting success after syntax validation alone.
- 2026-08-06: centralized shell-independent host services and made optional Nametag symbol ownership explicit.
