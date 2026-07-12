# oxide-platform-apple — `tests/http_async_contract_tests.rs`

## Intention and purpose

Freeze security, streaming, cancellation, and serialization properties in the native Apple HTTP source.

## Relation to the rest of the code

The checked Objective-C source implements the C ABI consumed by `AppleHttpClient`; live macOS loopback and iOS compile checks complement these static guards.

## Entry points

- `apple_http_is_shared_streaming_manual_and_credential_free()` rejects per-request sessions, buffering, automatic redirects, cookies, credentials, and semaphores.
- `apple_http_serializes_cancel_timeout_and_delegate_callbacks()` verifies one delegate queue owns cancellation, timeout, and callback terminal arbitration.

## Logic narrative

Tests inspect the actual compiled source for the required singleton delegate, ephemeral credential-free configuration, incremental callbacks, size checks, manual redirect completion, and serialized state removal.

## Preconditions and postconditions

The source path must remain owned by `oxide-platform-apple`. Passing proves structural invariants; live harnesses prove runtime behavior.

## Edge cases and failure modes

The guards fail on reintroduced semaphores, automatic following, ambient credential stores, or per-request sessions.

## Concurrency and memory behavior

One serial delegate queue arbitrates state and terminal removal. Body data is borrowed only during the callback and copied by Rust consumers as needed.

## Performance notes

The shared session avoids per-operation session and thread creation; streamed caps prevent unbounded full-body retention.

## Feature flags and cfgs

The source is compiled only for iOS and macOS.

## Testing and benchmarks

Run `cargo test -p oxide-platform-apple --test http_async_contract_tests --locked` and the macOS live loopback harness.

The executable local-`NSURLProtocol` limit matrix is driven by `http_native_harness_tests.rs`; this source-contract test remains focused on delegate ownership and serialization.

## Examples

Not applicable; the C ABI is consumed through `AppleHttpClient`.

## Changelog

- 2026-07-11: added asynchronous native-source contract guards.
