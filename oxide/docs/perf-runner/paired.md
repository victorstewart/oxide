# oxide-perf-runner::paired

## Purpose and boundary

`oxide_perf_runner::paired` is an offline validator and reducer for already
collected A/B samples. It owns one deterministic input schema, balanced pair
order, evidence validation, distribution summaries, acceptance decisions, and
stable report serialization.

It does not acquire samples, launch processes, inspect or mutate source trees,
create instrumentation patches, hash files, control a browser or device, or
export metrics from a shipping renderer or host. The producer is responsible
for those operations and supplies their identities in `PairedExperimentInput`.
The analyzer checks identity formats and consistency; it does not independently
prove that a supplied hash belongs to a file.

The command-line boundary is intentionally limited to deterministic reanalysis:

```text
oxide-perf-runner --paired-analyze INPUT --paired-json-out OUTPUT
```

## Public surface

- `balanced_pair_order(seed, pair_count)` returns deterministic balanced AB/BA
  blocks.
- `analyze_paired_experiment(input)` validates evidence and returns a
  `PairedExperimentReport`.
- `report_json(report)` emits stable pretty JSON with a trailing newline.
- `PairOrder`, `PairInvalidationReason`, `WorkloadKind`, `AcceptancePolicy`,
  `ExperimentIdentity`, `EnvironmentFingerprint`, `SamplePair`,
  `PairedExperimentInput`, `DistributionSummary`, `PairedDecision`, and
  `PairedExperimentReport` define the versioned input and output contract.
- `PairInvalidationReason` is a closed, kebab-case JSON vocabulary:
  `missing-warmup-samples`, `missing-measured-samples`,
  `unequal-measured-sample-counts`, or `environment-mismatch`.
- `DistributionSummary` serializes p50, p95, p99, peak, p05, p01, minimum,
  median absolute deviation, and coefficient of variation from one sorted raw
  population. `PairedExperimentReport.lower_is_better` preserves the direction
  needed to audit which summary fields gated its decision.

No workflow-plan, adapter, process-command, comparative-schema, sign-test,
multiple-comparison, or instrumentation-patch API is part of this module.

## Analysis contract

The analyzer rejects:

- unsupported schema versions, empty identifiers, or empty metric names;
- malformed Git or SHA-256 identities;
- non-contiguous pairs or orders that differ from the seed-derived schedule;
- missing required warmups or raw samples in a valid pair;
- unequal A/B measured-sample counts in a valid pair, or unequal A/B warmup
  counts when that workload requires warmup;
- non-finite or negative samples;
- a zero baseline pair median, because relative speedup is undefined and an
  infinite decision value cannot be serialized as JSON;
- a computed relative speedup that is non-finite;
- mixed A/B environments, cross-pair environment drift, or visible workloads
  not marked as production-path measurements;
- missing or inconsistent binary and instrumentation identities;
- an invalidation reason whose named condition is not present in its pair;
- more than 10% invalid pairs, or surviving AB/BA counts that differ by more
  than one;
- workloads below their valid-pair or raw-sample minima.

Invalidated pairs remain in the report but do not count toward minima or
statistics. They are diagnostic evidence, not an operator-controlled
free-text exclusion mechanism. Every invalid pair still undergoes index and
order validation, validation of every present sample, validation of both
environment fingerprints and production-path flags, and complete artifact and
declared-identity validation. Its typed reason is admitted only when the
corresponding condition is mechanically visible in the persisted pair. Valid
pairs must have equal A/B measured counts and, when warmup is required, equal
A/B warmup counts. The 10% invalidation ceiling and surviving-order balance
bound post-collection selection even when each individual invalidation is
genuine.

Each valid pair contributes its A and B medians to the relative-speedup and
fixed-seed bootstrap calculations. Reported p50, p95, p99, peak, p05, p01,
minimum, median absolute deviation, and coefficient of variation use all valid
raw samples. Those report fields remain direction-independent descriptive
summaries: p95/p99/peak always mean the upper quantiles/maximum, while
p05/p01/minimum always mean the lower quantiles/minimum. The bootstrap uses
100,000 deterministic resamples.

`Performance` acceptance requires at least 5% median speedup, a paired 95%
confidence lower bound of at least 2%, and candidate wins in at least 80% of
valid pairs. `NoMaterialRegression` permits median movement down to -3%.
Both policies also enforce direction-correct adverse-tail regression limits.

Metric direction applies to every gate:

- with `lower_is_better = true`, larger candidate p95, p99, or maximum values
  are regressions;
- with `lower_is_better = false`, smaller candidate p05, p01, or minimum values
  are regressions.

The two adverse-tail quantile thresholds are exact relative regressions greater
than 3%:
`(candidate - baseline) / baseline` for lower-is-better metrics and
`(baseline - candidate) / baseline` for higher-is-better metrics. The
worst-extreme gate uses the same symmetric definition with a 5% threshold for
the adverse maximum or minimum. Exact 3% and 5% boundaries are allowed; only
values beyond them fail. The gates read the same `DistributionSummary` fields
serialized beside the decision, and the report carries `lower_is_better`, so a
consumer can reproduce which three values were adverse without consulting the
input file. Decision reasons name the actual quantile or extreme used.
Direction and boundary behavior are covered externally because a
higher-is-better candidate can retain identical median and upper-tail summaries
while its lower service tail materially collapses.

Schema version remains 1 because this reconstructed paired-report contract has
not shipped. There is no compatibility reader or migration path for the earlier
unpublished shape. Existing all-valid input and pair JSON remains readable
because `"invalid_reason": null` still maps to no invalidation; arbitrary
free-text reasons are rejected.

## Runtime behavior

Analysis is single-threaded and has no global mutable state. Publication always
uses 100,000 bootstrap medians and one pair-sized resample buffer. The module is
benchmark tooling and is not reachable from a shipping host or renderer path.

## Tests

`oxide/crates/perf-runner/tests/paired_experiment_tests.rs` compiles the same
reducer source with a bounded private bootstrap count for broad correctness
coverage of balanced ordering, improvement and regression decisions,
no-material-regression tails,
both metric directions, an isolated higher-is-better lower-tail collapse with
unchanged p50/p95/p99/maximum summaries, typed and bounded invalidation,
survivor-order balance, equal per-pair sample counts, invalid-pair evidence
validation, persisted-null compatibility, cold-start warmup handling,
byte-deterministic serialization, and the reanalysis-only CLI. The CLI test
still executes the exported publication path and requires the persisted count
to remain 100,000. Tests are kept outside production source and documented in
[`tests/paired_experiment_tests.md`](tests/paired_experiment_tests.md).

## Changelog

- 2026-08-07: bounded broad reducer correctness tests to 1,024 deterministic
  bootstrap resamples while retaining one exported CLI analysis at the fixed
  100,000-resample publication contract.
- 2026-08-06: closed invalidation to four mechanically evidenced reasons,
  capped invalid pairs at 10%, preserved survivor AB/BA balance, required equal
  valid-pair sample counts, and validated all invalid-pair evidence.
- 2026-08-06: made unpublished v1 reports self-auditing with p05, p01, minimum, and metric direction; higher-is-better admission consumes those serialized fields while lower-is-better gates remain unchanged.
- 2026-08-02: reconstructed the offline reducer without acquisition,
  source-mutation, browser, host, workflow-adapter, or SHA-256 dependencies;
  corrected higher-is-better relative-regression direction.
