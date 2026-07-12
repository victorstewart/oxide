# oxide-platform-apple `tests/http_native_harness_tests.rs`

## Intention and purpose

Compile and execute the native Apple HTTP limit harness against the real Objective-C bridge during macOS Cargo tests.

## Relation to the rest of the code

The test invokes Clang on `src/apple/http.m` and `tests/http_native_tests.m`, runs the resulting external executable, and removes its temporary output before asserting results.

## Entry points list

- `native_http_limits_hold_through_local_url_protocol()`: builds and runs the local-URL-protocol matrix.

## Logic narrative

The driver enables the compile-time-only test protocol hook, compiles with warnings as errors, executes the matrix, captures diagnostics, and unconditionally cleans its temporary directory.

## Preconditions and postconditions; invariants maintained

It runs only on macOS and requires `xcrun`, Clang, and Foundation. No test hook is compiled into production builds.

## Edge cases and failure modes

Compile and runtime failures preserve stderr in the assertion. Cleanup occurs before either status is asserted.

## Concurrency and memory behavior

The child process owns all native session state. The Rust driver waits synchronously and retains no FFI pointers.

## Performance notes

This is correctness coverage, not a benchmark.

## Feature flags and cfgs

The entire integration test is gated to `target_os = "macos"`.

## Testing and benchmarks

Run `cargo test --locked -p oxide-platform-apple --test http_native_harness_tests`.

## Examples

The Cargo command above is the supported entry point.

## Changelog

- 2026-07-12: added executable native HTTP bounds coverage.
