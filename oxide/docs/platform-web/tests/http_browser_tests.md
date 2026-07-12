# platform-web `tests/http_browser_tests.rs`

## Intention and purpose

Exercise the browser HTTP adapter in a wasm browser runner, including streaming, cancellation, and response bounds.

## Relation to the rest of the code

The tests call `BrowserHttpClient` through `HttpClient` and collect the same `HttpEvent` sequence consumed by Oxide applications.

## Entry points list

- `fetch_streams_response_and_delivers_exactly_one_terminal_event`: checks ordinary response delivery.
- `cancel_aborts_fetch_and_ignores_every_late_browser_callback`: checks cancellation ownership.
- `streamed_response_cap_fails_closed`: checks that a limit-plus-one response emits no body event.
- `streamed_response_exact_cap_succeeds`: checks exact-limit body delivery.
- `selected_response_header_values_share_the_fixed_header_byte_budget`: checks selected response metadata.

## Logic narrative

Each case starts a browser fetch, pumps zero-delay browser tasks until a terminal event, and then inspects response, body, and terminal ordering. Data URLs keep the coverage self-contained.

## Preconditions and postconditions; invariants maintained

The suite requires wasm32 and a browser runner. Every admitted request must emit exactly one terminal event.

## Edge cases and failure modes

Cancellation suppresses late callbacks. Limit-plus-one body data fails without a body event; the exact body limit completes with all bytes.

## Concurrency and memory behavior

Events are collected behind an `Arc<Mutex<Vec<HttpEvent>>>`; browser operations remain single-threaded wasm tasks.

## Performance notes

The over-limit assertion protects the admission-before-copy path. This is correctness coverage rather than a benchmark.

## Feature flags and cfgs

The complete file is gated to `target_arch = "wasm32"`.

## Testing and benchmarks

Run with `wasm-pack test --headless --chrome --test http_browser_tests` using a browser-compatible WebDriver.

## Examples

The wasm-pack command above is the canonical invocation.

## Changelog

- 2026-07-12: added exact-limit success and no-body-event limit-plus-one coverage.
