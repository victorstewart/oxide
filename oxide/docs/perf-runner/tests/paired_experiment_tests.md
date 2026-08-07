# oxide-perf-runner tests `paired_experiment_tests.rs`

## Intention and purpose

This integration suite freezes the offline paired reducer's evidence admission,
direction-aware publication decisions, deterministic statistics, and CLI
serialization. It specifically prevents throughput-style metrics from being
published when only their lower service tail regresses, and prevents invalid
pairs from becoming a post-collection order-selection mechanism.

## Relation to the rest of the code

- Exercises only the public paired API documented in [`../paired.md`](../paired.md).
- Supplies synthetic A/B evidence with exact binary, instrumentation,
  environment, warmup, pair-order, and raw-sample identities.
- Calls the shared perf-runner CLI for the persisted reanalysis boundary.

Call graph:

- fixture helpers -> `balanced_pair_order` -> `PairedExperimentInput`
- decision tests -> `analyze_paired_experiment` -> validation -> paired medians/bootstrap -> adverse-tail gates
- serialization test -> `report_json`
- CLI test -> `oxide_perf_runner::run_cli` -> paired analyzer -> atomic report output

## Entry points list

- `balanced_order_is_deterministic_and_balanced` freezes deterministic AB/BA blocks.
- `invalidation_cannot_select_only_ba_pairs_from_a_balanced_schedule` reproduces
  and rejects a 30-pair exploit that discards every AB result.
- `fabricated_and_excessive_invalidations_are_rejected` requires a typed reason
  to match persisted evidence and caps exclusions at 10%.
- `survivor_order_balance_is_enforced_at_the_invalidation_cap` rejects an exact
  10% exclusion that removes two AB pairs and leaves an 8/10 survivor split.
- `valid_pairs_require_equal_sample_counts` requires symmetric A/B measured and
  required-warmup populations within each valid pair.
- `capped_diagnostic_invalidation_preserves_balanced_publication` admits one
  genuine environment-mismatch diagnostic while retaining the minimum
  population and balanced survivors.
- `invalid_pairs_do_not_bypass_evidence_validation` rejects malformed samples,
  environments, and artifact identities even when the pair has a genuine
  invalidation condition.
- `invalidation_schema_is_closed_and_null_compatible` rejects arbitrary
  free-text reasons and parses a persisted all-valid v1 pair whose reason is
  null.
- `decisive_improvement_passes_statistical_gates` verifies all performance-policy gates and serialized direction for a clear lower-is-better win.
- `ties_and_regressions_are_rejected` checks pair-win and median-speedup rejection.
- `insufficient_and_mixed_inputs_are_rejected` rejects insufficient populations, environment drift, and stale binaries.
- `no_material_regression_policy_accepts_ties_but_not_tail_regressions` protects lower-is-better upper-tail admission.
- `higher_is_better_tail_direction_is_respected` requires p05, p01, and minimum reasons for a uniform throughput regression.
- `higher_is_better_low_tail_regression_blocks_publication` keeps median and reported upper summaries identical while degrading only low samples, then requires publication rejection and exact JSON p05/p01/minimum/direction evidence.
- `zero_baseline_median_is_rejected_before_report_serialization` rejects undefined relative speedup.
- `relative_tail_regression_boundaries_are_symmetric` freezes exact 3% quantile and 5% worst-extreme boundaries in both directions.
- `cold_browser_startup_persists_empty_warmups` preserves the cold-start exception.
- `analysis_and_json_are_byte_deterministic` freezes seeded analysis and serialization.
- `shared_cli_analyzes_and_persists_raw_evidence` verifies end-to-end reanalysis output.
- `paired_cli_rejects_incomplete_output_arguments` rejects partial CLI modes.

## Logic narrative

The common fixture builds 15 balanced pairs with three measured samples on each
side. Most tests scale complete distributions, while the isolated lower-tail
test changes only the lowest candidate sample in three pairs. Their medians do
not move, and the combined candidate p50, p95, p99, and maximum remain identical
to baseline. A rejection therefore proves that higher-is-better publication is
gated by p05, p01, and minimum rather than by unrelated upper summaries. The
test then serializes the report and requires its schema version, direction, and
both sides' three lower-tail values to equal the fields used by the in-memory
decision.

The boundary test uses constant distributions so a value exactly at the 3% or
5% allowance is accepted and the smallest value beyond it is rejected. It
selects p95/p99/maximum reason names for lower-is-better and
p05/p01/minimum names for higher-is-better.

The invalidation tests expand the same deterministic fixture to 16 or 30 pairs.
The exploit supplies a mechanically true missing-samples reason for every AB
pair, proving that typed reasons alone cannot authorize an order-selected
result. Separate cases freeze the 10% ceiling, at-most-one survivor-order
difference, equal within-pair sample populations, and validation of evidence
that does not contribute to statistics. A 16-pair case invalidates one genuine
environment mismatch and still publishes from 15 survivors, proving that the
guardrails retain the intended diagnostic path. The schema test parses an
existing experiment-report pair to freeze null compatibility without rewriting
historical reports.

## Preconditions and postconditions

- Every admitted fixture has at least 15 valid pairs, survivor AB/BA counts
  differing by at most one, equal A/B sample populations, and the exact
  declared artifact/environment identity.
- Lower-is-better tests retain the historical p95, p99, and maximum gates.
- Higher-is-better tests require p05, p01, and minimum gates.
- A passing isolated-tail test proves publication changes without a median or
  descriptive upper-tail change, and that the exact decision inputs survive
  serialization.

## Edge cases and failure modes

- Zero medians, invalid hashes, missing pairs, mixed environments, and partial
  CLI arguments fail before publication.
- Free-text or unevidenced invalidation reasons, exclusions above 10%,
  order-selected survivors, and unequal valid-pair populations fail before
  statistics are computed.
- Invalid pairs remain fully validated and persist in the report; only their
  measured contribution is excluded.
- Exact threshold boundaries remain allowed; only strict excess is rejected.
- Cold browser startup may omit warmup samples; warm workloads may not.

## Concurrency and memory behavior

Tests run independent owned inputs and share only immutable string constants.
The CLI test uses a process-specific temporary directory and removes it before
returning. No sleeps, network access, threads, or device resources are used.

## Performance notes

These are deterministic reducer correctness tests, not performance
measurements. Broad decision coverage compiles the production reducer source
with a private 1,024-resample budget. The CLI test alone executes the exported
100,000-resample publication path and requires that count in its report. The
isolated-tail fixture adds no repeated soak matrix.

## Feature flags and cfgs

No feature flags or target-specific branches.

## Testing and benchmarks

```sh
cargo test --locked -p oxide-perf-runner --test paired_experiment_tests
```

## Examples

```sh
cargo test --locked -p oxide-perf-runner --test paired_experiment_tests \
  higher_is_better_low_tail_regression_blocks_publication
```

## Changelog

- 2026-08-07: reduced repeated correctness bootstrap work by 94.48% while
  retaining one end-to-end 100,000-resample CLI publication check.
- 2026-08-06: added exploit, closed-schema, invalidation-cap, survivor-balance,
  equal-population, invalid-evidence, and persisted-null compatibility coverage.
- 2026-08-06: required serialized metric direction and exact baseline/candidate p05, p01, and minimum decision inputs.
- 2026-08-06: documented direction-correct adverse-tail publication coverage and the isolated lower-tail regression.
