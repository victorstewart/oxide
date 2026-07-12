# oxide-platform-android — `tests/lib_tests.rs`

## Intention and purpose

Prove the Android shipping gate remains part of ordinary non-Android workspace checks.

## Relation to the rest of the code

The test imports the public marker from `oxide-platform-android`; Android compilation fails earlier
by design.

## Entry points

- `non_android_workspace_exposes_shipping_gate_marker()` verifies ordinary workspace registration.
- `android_shipping_fails_at_compile_time_until_a_real_http_host_exists()` freezes the unconditional target gate and forbids an unsupported runtime fallback.

## Logic narrative

The tests construct and compare the zero-sized marker, then inspect the shipping selection source to
prove Android reaches `compile_error!` and never selects `UnsupportedHttpClient`.

## Preconditions and postconditions

The target must not be Android. Passing proves the crate is wired into the workspace.

## Edge cases and failure modes

Android is deliberately handled by the compile-time error rather than this test.

## Concurrency and memory behavior

None.

## Performance notes

Constant time and allocation-free.

## Feature flags and cfgs

No features.

## Testing and benchmarks

Run with `cargo test -p oxide-platform-android`.

## Examples

Not applicable.

## Changelog

- 2026-07-11: Added workspace and explicit Android compile-failure coverage.
