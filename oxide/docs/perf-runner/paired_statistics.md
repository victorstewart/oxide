# oxide-perf-runner::paired_statistics

## Intention and purpose

`paired_statistics` owns direction-aware deterministic statistics shared by the existing source A/B analyzer and the comparative framework analyzer. Centralizing these operations prevents throughput metrics from reusing latency-only tail rules and lets the comparative path reuse ordering, distribution, exact-decision, and multiplicity behavior without changing paired schema v1.

## Relation to the rest of the code

- `paired` publicly re-exports `PairOrder` and `balanced_pair_order`, preserving their established API paths.
- `paired::analyze_paired_experiment` uses the shared speedup, ratio-regression, percentile, median, and bootstrap helpers.
- `comparative` re-exports exact sign-test and Holm types while delegating their implementation to this module.

Call flow:

- `paired::analyze_paired_experiment`
  - `median`
  - `relative_speedup_pct`
  - `paired_bootstrap_ci`
  - `percentile`
  - `regresses_by_ratio`
- `comparative`
  - `exact_sign_test`
  - `holm_adjust`
  - `exact_sign_test_resolution_floor`

## Entry points list

This is a crate-private module. `PairOrder` and `balanced_pair_order(seed: u64, pair_count: usize) -> Vec<PairOrder>` remain externally reachable only through `oxide_perf_runner::paired`; exact-decision types and functions are reachable through `oxide_perf_runner::comparative`.

## Logic narrative

Balanced ordering advances one fixed xorshift64 state per four-position block and emits either ABBA or BAAB. Direction-aware speedup and guardrail helpers interpret smaller values as better for latency/cost metrics and larger values as better for throughput metrics. The bootstrap resamples whole paired estimators, computes one median per resample, sorts the results, and reads the fixed percentile interval. Exact sign tests remove boundary ties and sum an exact binomial tail; Holm adjustment uses raw p-value plus lexical identifiers for stable ordering and a running maximum for monotonic adjusted values.

## Preconditions and postconditions

- Statistical inputs are validated as finite, nonnegative samples by the owning analyzer.
- `allowed_ratio` is a multiplicative worse-than boundary greater than or equal to one.
- Identical seed, values, and resample count produce identical ordering and confidence bounds.
- Exact sign-test populations contain at most 63 non-tied pairs; decision families and identifiers are nonempty.
- Public paired schema names and serialized enum values remain unchanged.

## Edge cases and failure modes

- An empty percentile input returns zero defensively; validated analysis never supplies one.
- A zero baseline and zero candidate produce zero speedup.
- Against a zero baseline, a positive candidate is an infinite loss for lower-is-better and an infinite win for higher-is-better.
- Nonfinite inputs are rejected before these helpers run.
- Exact ties reduce effective sample size, while empty effective populations produce p-value one for later sufficiency rejection.

## Concurrency and memory behavior

All functions are single-threaded and use only caller-local state. Ordering allocates the returned vector. Bootstrap allocates one resample buffer sized to the pair population and one vector sized to the requested resample count.

## Performance notes

These functions execute outside production renderer hot paths. Median and percentile helpers copy and sort inputs for deterministic ownership. The fixed release analyzer intentionally spends more CPU on reproducible evidence than an interactive approximate quantile would.

## Feature flags and cfgs

The module has no feature- or target-specific behavior.

## Testing and benchmarks

`tests/paired_experiment_tests.rs` covers deterministic balance, lower-is-better and higher-is-better improvements/regressions, tail direction, deterministic bootstrap output, and workflow integration. `tests/comparative_analysis_tests.rs` covers exact sign-test and Holm decision vectors.

## Examples

For a throughput metric, call `regresses_by_ratio(reference, contender, false, 1.03)` so a contender more than three percent below the reference is a regression.

## Changelog

- 2026-07-17: added exact paired sign tests, deterministic Holm adjustment, and decision-family resolution floors for comparative analysis.
- 2026-07-17: extracted deterministic ordering and distribution helpers from `paired`, added direction-aware p95/p99/peak guardrails, and preserved the paired schema-v1 public surface.
