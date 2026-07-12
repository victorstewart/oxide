# oxide-platform-api — `tests/http_tests.rs`

## Intention and purpose

Freeze the object-safe asynchronous HTTP request/event/cancellation contract independently of an OS adapter.

## Relation to the rest of the code

Apple and simulation adapters implement the tested API; generic Rust consumers build requests and classify terminal events through it.

## Entry points

- `request_owns_remaining_timeout_headers_and_response_selection()` verifies owned request policy and builders.
- `terminal_event_classification_and_unsupported_admission_are_exact()` verifies terminal classification and explicit unsupported admission.

## Logic narrative

Tests construct requests with remaining timeouts, byte caps, request headers, and selected response headers, then classify every event variant and invoke the nonshipping unsupported client.

## Preconditions and postconditions

No platform or network is installed. Passing proves the generic contract is deterministic and object-safe.

## Edge cases and failure modes

Success, failure, and cancellation are terminal; response and body events are not. Unsupported hosts fail before accepting work.

## Concurrency and memory behavior

Tests are synchronous and allocation-bounded; concurrency behavior belongs to adapter tests.

## Performance notes

No benchmark is required for value construction and enum classification.

## Feature flags and cfgs

None.

## Testing and benchmarks

Run `cargo test -p oxide-platform-api --test http_tests --locked`.

## Examples

The test request builders are minimal API examples.

## Changelog

- 2026-07-11: added asynchronous HTTP contract coverage.
