# platform-apple `src/apple/http.m`

## Intention and purpose
- Provide the shared asynchronous Apple HTTP bridge consumed by `AppleHttpClient` on iOS and macOS.
- Stream bounded response events without semaphores, full-response buffering, automatic redirects, or per-request sessions.

## Relation to the rest of the code
- `oxide-platform-apple/src/lib.rs` maps the `oxide_host_http_start` and `oxide_host_http_cancel` ABI to `HttpEvent` and `HttpOperation`.
- `oxide-platform-apple` compiles this source once for every Apple target, so every direct consumer
  receives the native symbols without host-specific source duplication.

## Entry points list
- `oxide_host_http_start(...)` validates and admits one request, then emits response metadata, body chunks, and exactly one terminal event.
- `oxide_host_http_cancel(operation_id)` requests idempotent cancellation on the delegate queue.

## Logic narrative
- The session disables cookies, credential storage, and caching. Requests reject embedded URL credentials and credential-bearing headers.
- Redirect responses are delivered to Rust and automatic following is disabled, preserving caller-owned redirect and downgrade policy.
- Declared and streamed body sizes are checked against the request cap. The caller's remaining Rust budget becomes the native request and delegate-queue timeout.
- All terminal paths remove the operation before invoking the callback, preventing duplicate terminal delivery and callback-after-free.

## Preconditions and postconditions
- `oxide-platform-apple` links Foundation; the consuming target must be an Apple target.
- The callback context remains owned until a terminal event; cancellation is safe and idempotent.

## Edge cases and failure modes
- Invalid methods, URLs, headers, deadlines, byte caps, authentication challenges, oversized bodies, and transport failures fail closed.
- Redirects complete as ordinary 3xx responses and are never followed inside the native bridge.

## Testing and benchmarks
- Rust ABI/event mapping: `cargo test -p oxide-platform-apple --locked`.
- iOS linkage: `cargo check -p oxide-platform-ios --target aarch64-apple-ios --locked`.
- Static contract tests reject semaphore use, automatic redirects, and per-request sessions in this source.

## Changelog
- 2026-07-11: moved native-source compilation into `oxide-platform-apple` so iOS, macOS, and direct consumers share one self-contained adapter.
- 2026-07-11: replaced the synchronous response-copy bridge with one shared streaming URLSession delegate and explicit cancellation.
