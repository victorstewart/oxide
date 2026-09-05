# `oxide-apple-comparison-controller::tests::energy_tests`

## Intention and purpose

These deterministic fake-meter tests freeze the macOS direct-energy contract without claiming or using real hardware measurements.

## Relation to the rest of the code

The tests exercise the standalone energy reducer and protocol validators. Benchmark-spec tests separately freeze the release-only 120-second stabilization and 120-second measurement overlay.

## Entry points list

This integration test has no public entry points. Cargo discovers its four `#[test]` functions.

## Logic narrative

A one-hertz fake meter supplies 10 watts across a 240-second window. Integrity-protected comparator telemetry divides the measured half into four 30-second phases. The expected result is 1,200 joules raw and 960 joules after the declared two-watt baseline.

## Preconditions and postconditions

Fixtures use one shared mach timebase and complete calibration metadata. Passing tests prove deterministic integration and strict protocol validation, not physical accuracy.

## Edge cases and failure modes

Coverage rejects a 119-second stabilization window, a 119-second measurement window, a sample gap larger than two configured periods, calibration drift, adapter-byte drift, display/topology disagreement, and any request that permits invasive tools.

## Concurrency and memory behavior

Tests are synchronous and use small in-memory samples plus a temporary fake executable.

## Performance notes

No real meter, profiler, screen recorder, app, or GUI session is started.

## Feature flags and cfgs

None.

## Testing and benchmarks

Run `cargo test --locked -p oxide-apple-comparison-controller --test energy_tests` from the Rust workspace.

## Examples

The happy path asserts exactly 300 joules for each 30-second phase at 10 watts.

## Changelog

- 2026-07-21: added deterministic fake-meter reduction, calibration, isolation, and boundary-failure coverage.
