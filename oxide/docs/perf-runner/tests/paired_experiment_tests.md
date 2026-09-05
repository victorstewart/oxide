# oxide-perf-runner tests::paired_experiment_tests

## Intention and purpose

This integration suite freezes the paired source A/B evidence contract across analysis, serialization, source identities, and fresh-process workflow acquisition.

## Relation to the rest of the code

- Exercises public APIs from `oxide_perf_runner::paired`.
- Uses temporary Git repositories to verify baseline-commit and candidate-index identities.
- Verifies raw evidence written by the workflow runner can be reanalyzed deterministically.

Call flow:

- synthetic `PairedExperimentInput`
  - `balanced_pair_order`
  - `analyze_paired_experiment`
  - `report_json`
- temporary source workflow
  - `create_instrumentation_patch`
  - `run_paired_workflow`
  - persisted raw/report evidence assertions

## Entry points list

The file exports no library entry points. Rust's test harness reaches each `#[test]` function.

## Logic narrative

Helpers construct a fixed 15-pair workload with exact source, binary, instrumentation, environment, and artifact identities. Tests vary candidate values, metric direction, cache class, environments, and identities, then assert either a deterministic decision or fail-closed validation. The workflow test creates a real temporary Git repository and runs fresh shell processes in the seed-derived side order.

## Preconditions and postconditions

- The host provides `git` and `/bin/sh` for the workflow integration case.
- Temporary evidence roots are process-specific.
- Successful tests remove their temporary directories.
- Decisions are asserted only after minimum pair/sample cardinalities are satisfied.

## Edge cases and failure modes

Coverage includes ties, regressions, insufficient pairs, mixed environments, stale binary hashes, cold startup without warmups, higher-is-better tails, and deterministic repeated analysis.

## Concurrency and memory behavior

Tests are independent but use process-specific temporary roots so parallel test execution does not alias evidence. The analyzer's fixed bootstrap allocation is released after each test.

## Performance notes

The deterministic 100,000-resample bootstrap dominates test time. This is intentional coverage of the release analysis path rather than a benchmark.

## Feature flags and cfgs

The tests have no feature- or target-specific branches.

## Testing and benchmarks

Run with `cargo test --locked -p oxide-perf-runner --test paired_experiment_tests` from the `oxide` workspace.

## Examples

`higher_is_better_tail_direction_is_respected` demonstrates that larger throughput values pass p95/p99/peak guardrails while smaller values fail them.

## Changelog

- 2026-07-17: added explicit higher-is-better tail-direction regression coverage and documented the complete integration suite.
