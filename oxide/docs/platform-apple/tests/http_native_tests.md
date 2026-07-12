# oxide-platform-apple `tests/http_native_tests.m`

## Intention and purpose

Exercise the real `NSURLSession` bridge with a deterministic local `NSURLProtocol` and no external network.

## Relation to the rest of the code

The harness links `src/apple/http.m` directly and calls `oxide_host_http_start`, `oxide_host_http_cancel`, and a test-build-only delegate-queue barrier.

## Entry points list

- `main`: runs declared-body, streamed-body, selected-metadata, final-URL, cancellation, exact-bound, FFI-count, and terminal-uniqueness cases.

## Logic narrative

Each URL path selects a local response shape. The exact case emits a 16 KiB final URL, 32 KiB selected metadata, and two protocol writes totaling five bytes. Foundation may coalesce those writes into one delegate body event; the over-limit stream must still emit no body event, and every case must emit exactly one terminal callback. A delegate-queue barrier makes terminal-count inspection deterministic without sleeps.

## Preconditions and postconditions; invariants maintained

The harness is compiled only with `OXIDE_HTTP_TESTING`; production bridge builds do not install the test protocol or export the barrier.

## Edge cases and failure modes

The matrix rejects declared and streamed overruns, header value/aggregate/count overruns, and a final URL overrun. Exact bounds and cancellation succeed with their expected terminal kinds.

## Concurrency and memory behavior

Callback counters are atomic. Heap callback states remain alive until the final delegate barrier, then are released.

## Performance notes

This is a correctness harness. All traffic remains process-local.

## Feature flags and cfgs

`OXIDE_HTTP_TESTING` adds only compile-time test configuration.

## Testing and benchmarks

Run through `http_native_harness_tests.rs` or compile with Clang and Foundation directly.

## Examples

The Cargo integration test is the canonical invocation.

## Changelog

- 2026-07-12: added the native HTTP boundary matrix.
