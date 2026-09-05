# Release candidate scenario tests

## Intention and purpose

`crates/benchmark-spec/tests/release_candidate_scenario_tests.rs` freezes the five release-only workload contracts while canonical adapter screenshots are not yet available.

## Relation to the rest of the code

- The candidates reuse the v1 neutral asset manifest, pinned font pack, and style tokens.
- Typed trace validation comes from `oxide-benchmark-spec`.
- The files stay outside `scenarios/`, so existing loaders and runnable scenario sets are unchanged.

## Entry points list

Cargo's test harness invokes `release_candidates_are_frozen_hash_bound_and_fail_closed_on_screenshots` and `release_fixture_cardinalities_and_phase_contracts_match_the_frozen_matrix`.

## Logic narrative

The first test requires canonical compact candidate JSON, verifies every referenced SHA-256, validates every typed trace against its phase duration, and proves state and accessibility role counts agree with the frozen checkpoint contract. It also requires the corresponding loadable scenario to remain absent while screenshot materialization is blocked. The second test freezes the workload cardinalities, viewport column counts, mutation classes, multilingual categories and wraps, and ten alternating resize/theme changes.

## Preconditions and postconditions

The benchmark-spec v1 artifact tree must contain the shared assets and five candidate families. Success proves deterministic contract closure except for canonical PNGs; it does not claim visual parity, adapter readiness, or performance results.

## Edge cases and failure modes

Missing artifacts, stale hashes, malformed or overlong traces, workload-size drift, mismatched semantic counts, noncanonical candidate bytes, invented screenshot fields, or premature loadable manifests fail the tests.

## Concurrency and memory behavior

Tests use synchronous read-only host filesystem access. Data is bounded by the small JSON contract files and released with each test process.

## Performance notes

The tests run outside benchmark acquisition and cannot affect production frame paths. SHA-256 validation is linear in the referenced artifact bytes.

## Feature flags and cfgs

No feature flag or target cfg changes these tests.

## Testing and benchmarks

Run `cargo test --locked -p oxide-benchmark-spec --test release_candidate_scenario_tests`. Runtime comparisons remain blocked until adapters generate and cross-validate canonical checkpoint PNGs.

## Examples

`release-candidates/grid.large-scroll.candidate.json` shows the complete non-loadable contract shape and explicit screenshot promotion gate.

## Changelog

- 2026-07-21: added release candidate contract validation for grid, effects, mutation, multilingual text, and resize/theme workloads.
