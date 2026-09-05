# oxide-apple-comparison-controller tests: deep_attribution_tests

## Intention and purpose

These tests verify the pure deep-attribution reducer without launching Instruments.

## Relation to the rest of the code

The tests materialize a benchmark-spec replay and feed representative xctrace XML through the public controller reducer.

## Entry points list

- `deep_attribution_reduction_uses_exact_pid_measured_windows_and_retains_raw_metric_identity()` covers successful reduction.
- `deep_attribution_reduction_rejects_missing_exact_pid_windows_and_overlapping_phases()` covers fail-closed boundaries.

## Logic narrative

Synthetic exact-PID signposts bound a measured phase. Allocation rows from the target and an unrelated PID prove filtering, while unit metadata proves raw metric identity survives. Mutated PID and overlapping phase streams must fail.

## Preconditions and postconditions

The fixtures are self-contained UTF-8 XML and require no GUI session.

## Edge cases and failure modes

Missing exact-PID windows and overlapping measured phases are rejected.

## Concurrency and memory behavior

Tests are synchronous and use small in-memory XML fixtures.

## Performance notes

No xctrace process or production application is started.

## Feature flags and cfgs

None.

## Testing and benchmarks

Run `cargo test --locked -p oxide-apple-comparison-controller --test deep_attribution_tests` with a bounded external Cargo target.

## Examples

Not applicable.

## Changelog

- 2026-07-21: added deep-attribution reducer coverage.
