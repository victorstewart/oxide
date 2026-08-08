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
- decision tests -> `analyze_paired_experiment` -> validation -> paired medians -> exact rank interval -> adverse-tail gates
- serialization test -> `report_json`
- CLI test -> `oxide_perf_runner::run_cli` -> paired analyzer -> report output

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
  genuine environment-mismatch diagnostic while retaining balanced survivors
  above the minimum population.
- `invalid_pairs_do_not_bypass_evidence_validation` rejects malformed samples,
  environments, and artifact identities even when the pair has a genuine
  invalidation condition.
- `invalidation_schema_is_closed_and_null_compatible` rejects arbitrary
  free-text reasons and parses a persisted all-valid v1 pair whose reason is
  null.
- `paired_evidence_schema_rejects_unknown_fields` rejects retired or foreign
  fields at every input and report evidence-struct boundary.
- `decisive_improvement_passes_statistical_gates` verifies all performance-policy gates and serialized direction for a clear lower-is-better win.
- `workspace_cpu_minimum_supports_a_finite_exact_interval` rejects five
  workspace pairs and requires the requirement-minimal six-pair population to
  report exact ranks 1 through 6 and 96.875 percent achieved coverage.
- `physical_device_minimum_supports_a_finite_exact_interval` rejects five
  physical-device pairs and proves the requirement-minimal six-pair population
  reports conservative ranks 1 through 6 with 96.875 percent coverage.
- `ties_and_regressions_are_rejected` checks pair-win and median-speedup rejection.
- `insufficient_and_mixed_inputs_are_rejected` rejects insufficient populations, environment drift, and stale binaries.
- `no_material_regression_policy_accepts_ties_but_not_tail_regressions` protects lower-is-better upper-tail admission.
- `noise_control_requires_symmetric_interval_and_pooled_tails` accepts bounded
  current/current movement, then independently rejects an exact interval beyond
  2%, pooled p95/p99 movement beyond 3%, and peak movement beyond 5%.
- `noise_control_requires_identical_binaries` rejects a structurally consistent
  control population whose A/B binary hashes differ.
- `higher_is_better_tail_direction_is_respected` requires p05, p01, and minimum reasons for a uniform throughput regression.
- `higher_is_better_low_tail_regression_blocks_publication` keeps median and reported upper summaries identical while degrading only low samples, then requires publication rejection and exact JSON p05/p01/minimum/direction evidence.
- `zero_baseline_median_is_rejected_before_report_serialization` rejects undefined relative speedup.
- `relative_tail_regression_boundaries_are_symmetric` freezes exact 3% quantile and 5% worst-extreme boundaries in both directions.
- `cold_browser_startup_persists_empty_warmups` preserves the cold-start exception.
- `analysis_and_json_are_byte_deterministic` freezes seeded analysis and serialization.
- `shared_cli_analyzes_and_persists_raw_evidence` verifies end-to-end reanalysis output.
- `paired_cli_rejects_incomplete_output_arguments` rejects partial CLI modes.

## Logic narrative

The common fixture predeclares one seed whose requirement-minimal six-pair order
is `AB`, `BA`, `BA`, `AB`, `AB`, `BA`, with three measured samples on each side.
Producers may predeclare another seed and balanced order; the reducer validates
that derived schedule rather than imposing this fixture's particular order.
Most tests scale complete distributions, while the isolated lower-tail
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

The noise-control fixture uses its predeclared seed's six-pair schedule and
twelve raw samples per side. Its accepted case spans negative and positive pair
movement inside 2%. Separate cases keep pair medians fixed while moving pooled
p95/p99 or one isolated peak, proving the symmetric current/current gate cannot
be replaced by a one-sided no-regression decision.

The exact-interval fixtures assign ordered one-through-six percent pair speedups
so their rank bounds are directly observable. The physical-device fixture
supplies the required 2,000 raw samples per side across six independent pairs.
Both demonstrate why six is admitted as the smallest finite at-least-95 percent
interval while five is rejected, without claiming that the resulting
minimum-to-maximum interval is narrow.

The invalidation tests expand the same deterministic fixture to 16 or 30 pairs.
The exploit supplies a mechanically true missing-samples reason for every AB
pair, proving that typed reasons alone cannot authorize an order-selected
result. Separate cases freeze the 10% ceiling, at-most-one survivor-order
difference, equal within-pair sample populations, and validation of evidence
that does not contribute to statistics. A 16-pair case invalidates one genuine
environment mismatch and still publishes from 15 balanced survivors, proving
that the guardrails retain the intended diagnostic path. The schema test parses an
existing experiment-report pair to freeze null compatibility without rewriting
historical reports.

## Preconditions and postconditions

- Every admitted fixture meets its workload-specific valid-pair minimum,
  survivor AB/BA counts differ by at most one, A/B sample populations are
  equal, and artifact/environment identity is exact.
- Lower-is-better tests retain the historical p95, p99, and maximum gates.
- Higher-is-better tests require p05, p01, and minimum gates.
- A passing isolated-tail test proves publication changes without a median or
  descriptive upper-tail change, and that the exact decision inputs survive
  serialization.

## Edge cases and failure modes

- Zero medians, invalid hashes, missing pairs, mixed environments, and partial
  CLI arguments fail before publication.
- Unknown evidence fields and different-binary noise controls fail before
  statistics are computed.
- Free-text or unevidenced invalidation reasons, exclusions above 10%,
  order-selected survivors, and unequal valid-pair populations fail before
  statistics are computed.
- Invalid pairs remain fully validated and persist in the report; only their
  measured contribution is excluded.
- Exact threshold boundaries remain allowed; only strict excess is rejected.
- Cold browser startup may omit warmup samples; warm workloads may not.
- Five physical-device pairs fail because no finite two-sided median interval
  from that population can reach 95 percent coverage.

## Concurrency and memory behavior

Tests run independent owned inputs and share only immutable string constants.
The CLI test uses a process-specific temporary directory and removes it before
returning. No sleeps, network access, threads, or device resources are used.

## Performance notes

These are deterministic reducer correctness tests, not performance
measurements. Every decision test uses the production exact-rank calculation;
there is no private budget, random generator, repeated resampling, or soak
matrix. The CLI test requires the exact method and achieved coverage in its
serialized report and rejects the retired bootstrap field.

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

- 2026-08-07: added strict unknown-field coverage at each evidence-struct
  boundary, enforced same-binary noise-control fixtures, and corrected the CLI
  output and producer-predeclared schedule descriptions.
- 2026-08-07: reduced the common workspace fixture to its predeclared seed's
  requirement-minimal six-pair schedule and froze ranks 1 through 6 at 96.875
  percent exact coverage, including symmetric current/current noise admission.
- 2026-08-07: replaced simulated bootstrap coverage with exact 15-pair and
  minimum six-pair rank/coverage assertions and removed the final 100,000-draw
  CLI test path.
- 2026-08-06: added exploit, closed-schema, invalidation-cap, survivor-balance,
  equal-population, invalid-evidence, and persisted-null compatibility coverage.
- 2026-08-06: required serialized metric direction and exact baseline/candidate p05, p01, and minimum decision inputs.
- 2026-08-06: documented direction-correct adverse-tail publication coverage and the isolated lower-tail regression.
