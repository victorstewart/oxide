# platform-apple `src/apple/http.m`

## Intention and purpose
- Provide the shared native Apple HTTP GET bridge consumed by `AppleHttpClient`.
- Remove duplicated iOS/macOS `NSURLSession` response-copy and release code while keeping platform-specific session options behind compile-time Apple target checks.

## Relation to the rest of the code
- `oxide-platform-apple/src/lib.rs` declares the `oxide_host_http_get` and `oxide_host_http_response_free` ABI that this file exports.
- `oxide-platform-ios/build.rs` compiles this source for iOS platform builds.
- `oxide-host-macos/build.rs` compiles this source into the AppKit host so `MacPlatform::http()` reaches the same native bridge.

## Entry points list
- `oxide_host_http_get(url_ptr, url_len, timeout_ms, max_response_bytes, out_response)`
  Performs a synchronous GET from a non-main thread using an ephemeral `NSURLSession`, validates `http`/`https` URLs, enforces the response byte cap, and copies response body, final URL, and content type into native-owned buffers.
- `oxide_host_http_response_free(response)`
  Frees all native-owned response buffers and clears the ABI struct.

## Logic narrative
- The bridge rejects null/empty URLs, zero byte caps, invalid schemes, main-thread calls, transport failures, non-HTTP responses, oversized responses, and response-copy allocation failures with the negative return codes already mapped by `AppleHttpClient`.
- The session uses no URL cache and reloads from origin so loopback tests and callers do not observe stale cached responses.
- iOS enables cellular access and `NSURLSessionMultipathServiceTypeHandover`; macOS builds omit those iOS-only session policies.
- Empty response bodies, final URLs, and content types are represented as null pointers with zero lengths, matching the Rust wrapper's native-buffer handoff contract.

## Preconditions and postconditions
- Foundation and dispatch must be linked by the consuming Apple host.
- Callers must invoke `oxide_host_http_response_free` on successful responses after Rust copies the data.
- The function must not be called from the main thread because it waits synchronously for the URLSession completion.

## Edge cases and failure modes
- Main-thread calls return the shared busy code so UI or run-loop owners are not blocked.
- `timeout_ms == 0` uses the existing 10-second default timeout.
- `max_response_bytes` is enforced before native buffers are copied.
- Any allocation failure after a partial response copy frees already-copied fields before returning an I/O error code.

## Testing and benchmarks
- Rust-side ABI mapping is covered by `cargo test -p oxide-platform-apple --locked`.
- macOS installed-platform behavior is covered by `cargo test -p oxide-host-macos --features host-testing --tests --locked`, which fetches a loopback HTTP response through `MacPlatform::http()`.
- iOS target linkage is covered by `cargo check -p oxide-platform-ios --target aarch64-apple-ios --locked`.

## Changelog
- 2026-05-19: added shared native Apple HTTP bridge and removed duplicated iOS/macOS implementations.
