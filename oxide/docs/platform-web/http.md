# platform-web `src/http.rs`

## Intention and purpose

Provide the bounded browser `fetch` adapter behind `BrowserHttpClient` without buffering complete responses.

## Relation to the rest of the code

`WebPlatform::http` returns this client. Requests flow through validation, browser `fetch`, selected response metadata, streamed body admission, and one terminal callback.

## Entry points list

- `BrowserHttpClient::start`: validates and starts one browser request through `HttpClient`.
- `BrowserHttpOperation::cancel`: aborts one admitted operation idempotently through `HttpOperation`.

## Logic narrative

For each stream value the adapter creates a JavaScript `Uint8Array` view, reads `byteLength`, admits that length against the remaining budget, and only then calls `to_vec`. Response metadata is emitted only after the final URL passes the 16 KiB bound.

## Preconditions and postconditions; invariants maintained

Browser execution requires wasm32 and a live `window`. Manual redirects preserve caller policy. Every admitted operation emits at most one terminal event.

## Edge cases and failure modes

Declared or streamed body overruns, selected metadata above 64 headers or 32 KiB, final URLs above 16 KiB, timeout, cancellation, and malformed stream values fail closed.

## Concurrency and memory behavior

DOM handles remain in a thread-local registry. Exact-limit chunks allocate one Rust body vector; limit-plus-one chunks allocate no Rust body vector and emit no body event.

## Performance notes

Admission is constant time per chunk and precedes its linear Rust copy. No frame-loop work or dependency was added.

## Feature flags and cfgs

Browser execution compiles only for wasm32; native builds keep the explicit unsupported fallback.

## Testing and benchmarks

`tests/http_browser_tests.rs` covers exact and over-limit streams. `tests/lib_tests.rs` also checks admission-before-copy source order for native CI.

## Examples

Use `BrowserHttpClient` through `HttpClient` with a caller-selected response-byte cap.

## Changelog

- 2026-07-12: admitted chunk byte lengths before copying and capped final response URLs at 16 KiB.
