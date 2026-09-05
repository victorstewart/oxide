# xtask tests::apple_comparison_tests

## Intention and purpose

This integration suite protects the host-authoritative Apple fallback checkpoint boundary.

## Relation to the rest of the code

It exercises `finalize_per_app_transport_pair` using isolated pulled-artifact stand-ins.

## Entry points list

Rust's test harness invokes the stale-generation and valid-chain cases.

## Logic narrative

One test supplies a stale Oxide generation and asserts both an error and absence of `pair.complete.json`. The other supplies a fully linked artifact/ACK set and checks that only the completed checkpoint remains.

## Preconditions and postconditions

The tests require only writable temporary directories and leave no persistent files.

## Edge cases and failure modes

Coverage proves partial or untrusted evidence cannot create an authoritative checkpoint and a valid chain can.

## Concurrency and memory behavior

Temporary directories isolate the tests. Inputs are bounded JSON values.

## Performance notes

No device or timing acquisition runs.

## Feature flags and cfgs

None.

## Testing and benchmarks

Run `cargo test --locked -p xtask --test apple_comparison_tests`.

## Examples

See the fixture construction in `xtask/tests/apple_comparison_tests.rs`.

## Changelog

- 2026-07-17: added stale-generation and valid-chain coverage.
