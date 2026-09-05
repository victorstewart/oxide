# oxide-benchmark-spec/tests/nightly_scenario_tests.rs

## Intention and purpose

This integration test freezes the non-PR nightly idle and endurance contracts before they are admitted to an acquisition plan. It prevents either workload from silently substituting less work or a shorter observation window.

## Relation to the rest of the code

- Loads committed scenario JSON through `oxide_benchmark_spec::load_scenario`.
- Applies the shared structural and artifact validators.
- Protects the macOS, iOS, and Web adapters that consume the same framework-neutral scenario.

## Entry points

This test file exports no library API. The Rust test harness reaches the idle and endurance contract tests.

## Logic narrative

The idle test requires one trace-free sixty-second measured phase over the full dashboard. The endurance test requires exactly 300,000 measured milliseconds, 100 heavy-screen cycles, 500 tab switches, 600 animation frames, four final-state checkpoints, and byte-identical dashboard-derived screenshots without waiting in real time.

## Preconditions and postconditions

The comparative specification root and pinned dashboard assets must exist. Success proves the committed idle contract is structurally valid, canonical, and artifact-complete.

## Edge cases and failure modes

Changed hashes, path traversal, duplicate or malformed contract fields, additional measured phases, a trace-driven idle phase, or a changed duration fail the test.

## Concurrency and memory behavior

The test is single-threaded apart from the Rust test runner. It reads bounded committed files and performs no acquisition.

## Performance notes

This is an unmeasured correctness test. It protects the sixty-second runtime contract without waiting in real time.

## Feature flags and cfgs

None.

## Testing and benchmarks

Run `cargo test --locked -p oxide-benchmark-spec --test nightly_scenario_tests`.

## Examples

The committed `benchmarks/comparative/specs/v1/scenarios/idle.steady.json` file is the executable example.

## Changelog

- 2026-07-21: Added canonical five-minute endurance scenario coverage.
- 2026-07-21: Added canonical nightly idle scenario coverage.
