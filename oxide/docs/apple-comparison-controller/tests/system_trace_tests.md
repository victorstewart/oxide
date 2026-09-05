# `oxide-apple-comparison-controller::tests::system_trace_tests`

## Intention and purpose

These deterministic tests freeze the Xcode 26 System Trace reducer contract without launching Instruments.

## Relation to the rest of the code

The tests call the public `reduce_macos_system_trace` entry point with checked-in XML fixtures for `os-signpost`, `thread-info`, `thread-state`, and `context-switch`. Controller source-contract tests separately cover recorder isolation and lifecycle wiring.

## Entry points list

This test target has no public entry points. Cargo discovers its three `#[test]` functions.

## Logic narrative

The happy path identifies PID 42's unique main thread, pairs one measured phase, clips six state intervals, excludes worker evidence, counts three context-switch timestamps, and recognizes two blocked-to-runnable wakeups. Error cases mutate one contract dimension at a time.

## Preconditions and postconditions

Fixtures use the exact Xcode 26 column order. Passing tests guarantee deterministic totals and rejection of malformed or incomplete evidence.

## Edge cases and failure modes

Coverage includes missing and duplicate boundaries, PID/TID mismatch, unsupported columns, empty context-switch evidence, and a gap in main-thread state coverage.

## Concurrency and memory behavior

Tests are synchronous and parse small immutable fixture strings.

## Performance notes

No performance claim is made; the fixtures validate attribution arithmetic and admission.

## Feature flags and cfgs

None.

## Testing and benchmarks

Run `cargo test --locked -p oxide-apple-comparison-controller --test system_trace_tests` from the Rust workspace.

## Examples

The happy-path assertion expects 110 ns running, 50 ns runnable wait, three context switches, and two wakeups inside the 100-300 ns measured interval.

## Changelog

- 2026-07-21: added Xcode 26 System Trace reducer fixtures and failure coverage.
