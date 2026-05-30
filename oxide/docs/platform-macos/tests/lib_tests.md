# platform-macos `tests/lib_tests.rs`

## Intention and purpose
- Verify small `MacPlatform` Rust-side behaviors without launching AppKit or touching user-visible macOS state.
- Exercise the Rust/native clipboard ABI contract with test-local C symbols so empty-string behavior is deterministic.

## Relation to the rest of the code
- The tests link `oxide-platform-macos` against local `macos_*` C ABI stubs instead of the AppKit host.
- The clipboard tests cover the Rust wrapper logic in `crates/platform-macos/src/lib.rs`.
- Device-cap and capability tests cover the host-value sanitization and availability gating used by `MacPlatform::device_caps()` and `MacPlatform::capabilities()`.

## Entry points list
- `mac_platform_clipboard_preserves_empty_string()`
  Calls `MacPlatform::clipboard_set("")` and verifies `clipboard_get()` returns `Some("")`.
- `mac_platform_clipboard_round_trips_text()`
  Calls the same public platform clipboard methods with non-empty UTF-8 text.
- `mac_platform_device_caps_sanitize_host_values()`
  Verifies invalid host refresh-rate and scale values are clamped to safe defaults.
- `mac_platform_capabilities_are_gated_by_host_availability()`
  Verifies camera, recording, location, and motion capability bits depend on host availability while hover pointer, Bluetooth, and push remain advertised.

## Logic narrative
- The test file provides `#[no_mangle]` stubs for the macOS C ABI functions required by the platform crate.
- Clipboard stubs store bytes in a process-local mutex and allocate returned non-empty buffers with `libc::malloc`, matching the ownership contract of `macos_free`.
- The empty clipboard path returns success with a zero length and null pointer, which proves Rust distinguishes a valid empty string from a missing pasteboard string.
- Device-cap stubs intentionally return unusable refresh-rate and scale values so the Rust wrapper must apply defaults.

## Preconditions and postconditions
- Tests require no AppKit run loop and no real pasteboard.
- Every non-empty clipboard buffer allocated by the test ABI is freed through the platform wrapper's `macos_free` path.

## Edge cases and failure modes
- Null output parameters in the test clipboard getter return failure, matching the native ABI contract.
- Empty clipboard data is successful data, not absence.
- Host-unavailable camera, location, and motion values clear their capability bits.

## Concurrency and memory behavior
- Clipboard bytes are guarded by one process-local mutex.
- Tests do not spawn threads and do not depend on timing.
- The allocation/free boundary mirrors the native host ABI so leak-prone zero-length and non-empty cases are both covered.

## Performance notes
- These are deterministic unit-style tests and do not benchmark platform-service latency.
- The behavior under test is outside renderer and input hot paths.

## Feature flags and cfgs
- The tests run as normal `oxide-platform-macos` integration tests on the host toolchain.

## Testing and benchmarks
- Run with `cargo test -p oxide-platform-macos --tests --locked`.

## Examples
```rust
let platform = oxide_platform_macos::platform();
platform.clipboard_set("");
assert_eq!(platform.clipboard_get(), Some(String::new()));
```

## Changelog
- 2026-05-19: added platform-macos integration tests for clipboard empty strings, clipboard text, device-cap fallback sanitization, and capability gating.
