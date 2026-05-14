# platform-web::tests::lib_tests

## Intention and purpose

These tests verify that the browser platform adapter reports honest capabilities and keeps deterministic native fallbacks. They exist because most browser APIs cannot be exercised by native Rust tests.

## Relation to the rest of the code

The tests instantiate `WebPlatform`, call `oxide-platform-api` trait methods, and verify unsupported behavior for OS-only services. They also cover the localStorage hex codec used by the wasm secure-storage adapter.

Call flow:

- cargo test
- `platform-web/tests/lib_tests.rs`
- `oxide_platform_web::WebPlatform`
- `oxide_platform_api::Platform` and service traits

## Entry points list

- `hex_round_trips_secret_bytes()`: verifies secure-storage byte encoding.
- `hex_decode_rejects_invalid_input()`: verifies malformed hex errors.
- `platform_reports_browser_caps_and_unsupported_os_services()`: verifies caps, empty native fallback capability flags, denied contacts permission, and unsupported camera stream.
- `native_fallback_keeps_browser_only_services_explicit()`: verifies native fallback behavior for browser-only location, network subscription, permission subscription, and hidden WebView surfaces.
- `platform_clipboard_provider_caches_written_text()`: verifies synchronous clipboard cache behavior.
- `haptics_service_accepts_all_patterns()`: verifies haptics calls are no-op safe on native.

## Logic narrative

The tests use the same public API a host would use. The capability test ensures browser-only work does not accidentally claim native OS features. The browser-only fallback test protects the explicit unsupported boundaries for location, network listener installation, permission requests, and WebViews on native test builds. The clipboard test documents the synchronous cache used because browser clipboard reads are asynchronous and permission-gated.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

The tests require no browser. They maintain the invariant that unsupported browser/OS services return explicit unsupported or denied states instead of panicking, while browser-native wasm branches are compile-checked and exercised by the host page.

## Edge cases and failure modes

Odd-length and non-hex secure-storage strings fail. Camera stream startup fails cleanly. Empty capabilities are expected on native fallback builds. Browser capability reporting is wasm-only.

## Concurrency and memory behavior

The clipboard cache uses a process-global lock; tests write and read a single value.

## Performance notes

These are platform contract tests, not benchmark cases.

## Feature flags and cfgs

Native tests exercise fallback behavior. Wasm browser behavior is verified by the host website check.

## Testing and benchmarks

Run with `cargo test -p oxide-platform-web --tests`.

## Examples

```rust
pub fn encoded() -> String
{
   oxide_platform_web::hex_encode(&[1, 2, 3])
}
```

## Changelog

- Added native fallback coverage for browser-only platform services.
