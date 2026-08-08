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
  `ConfidenceIntervalMethod`, `MedianConfidenceInterval`, `ExperimentIdentity`,
  `EnvironmentFingerprint`, `SamplePair`, `PairedExperimentInput`,
  `DistributionSummary`, `PairedDecision`, and `PairedExperimentReport` define
  the versioned input and output contract.
- `PairInvalidationReason` is a closed, kebab-case JSON vocabulary:
  `missing-warmup-samples`, `missing-measured-samples`,
  `unequal-measured-sample-counts`, or `environment-mismatch`.
- `AcceptancePolicy` is a closed, kebab-case JSON vocabulary: `performance`,
  `no-material-regression`, or `noise-control`.
- `DistributionSummary` serializes p50, p95, p99, peak, p05, p01, minimum,
  median absolute deviation, and coefficient of variation from one sorted raw
  population. `PairedExperimentReport.lower_is_better` preserves the direction
  needed to audit which summary fields gated its decision.

No workflow-plan, adapter, process-command, comparative-schema, sign-test,
multiple-comparison, or instrumentation-patch API is part of this module.

## Analysis contract

The analyzer rejects:

- unknown fields at every externally deserialized evidence-struct boundary;
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
- a `NoiseControl` population whose baseline and candidate binary hashes differ;
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

Each valid pair contributes its A and B medians to one relative-speedup value.
The analyzer sorts those independent pair values once and reports an exact
two-sided binomial confidence interval for their population median. It selects
the narrowest symmetric order-statistic bounds whose achieved coverage is at
least the requested 95 percent and persists the method, requested coverage,
achieved coverage, one-based ranks, and percentage bounds. This interval is
distribution-free for independent pairs from a continuous population; ties
make its coverage conservative rather than invalid. There is no random seed,
Monte Carlo approximation, or resampling loop in the confidence calculation.

Workspace CPU and physical-device studies use the requirement-minimal six valid
pairs. Ranks 1 through 6 provide 96.875 percent coverage, the smallest
population with any finite two-sided interval at or above 95 percent. The
interval is deliberately conservative and spans the entire six-pair population:
a performance claim must therefore improve by at least 2 percent in every pair
to satisfy the confidence lower-bound gate. Five pairs would cover only 93.75
percent even from minimum through maximum and are rejected. Higher-variance
browser, GPU-timestamp, and input-journey workloads retain their larger minima.

Each evidence producer predeclares a seed, and the analyzer derives and validates
the corresponding balanced order. The six-pair test fixture's frozen seed yields
`AB`, `BA`, `BA`, `AB`, `AB`, `BA`; another predeclared seed may yield another
balanced order. Neither acquisition nor the exact interval performs runtime
randomization or resampling.

Reported p50, p95, p99, peak, p05, p01, minimum, median absolute deviation, and
coefficient of variation use all valid raw samples. Those report fields remain
direction-independent descriptive summaries: p95/p99/peak always mean the
upper quantiles/maximum, while p05/p01/minimum always mean the lower
quantiles/minimum.

`Performance` acceptance requires at least 5% median speedup, a paired 95%
confidence lower bound of at least 2%, and candidate wins in at least 80% of
valid pairs. `NoMaterialRegression` permits median movement down to -3%.
Both promotion policies also enforce direction-correct adverse-tail regression
limits.

`NoiseControl` is the mandatory current/current admission gate, not a promotion
policy. The entire exact paired interval must stay within -2% through 2%, pooled
p95 and p99 may move by at most 3% in either direction, and pooled peak may move
by at most 5% in either direction. A zero baseline tail accepts only an equal
zero candidate tail. The reducer requires identical baseline and candidate binary
hashes before applying these symmetric checks, preventing an apparently
favorable different-binary comparison from masquerading as same-binary drift.

Metric direction applies to every promotion-policy tail gate:

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

Schema version 3 changes the workspace CPU admission minimum from fifteen pairs
to the smallest population that supplies a finite exact interval at the 95
percent target. There is no compatibility reader or migration path for earlier
unpublished input shapes. Historical reports remain unchanged as evidence of
the method used when they were created; they are not rewritten or treated as
version-3 output. Version-3 input and report structs reject unknown fields,
including retired bootstrap controls, instead of silently ignoring stale
evidence. Existing all-valid pair JSON remains readable in isolation because
`"invalid_reason": null` still maps to no invalidation; arbitrary free-text
reasons are rejected.

## Runtime behavior

Analysis is single-threaded and has no global mutable state. Confidence work
sorts one pair-sized speedup vector and computes exact binomial rank coverage in
linear time with one bounded half-population weight vector. The module is
benchmark tooling and is not reachable from a shipping host or renderer path.

## Tests

`oxide/crates/perf-runner/tests/paired_experiment_tests.rs` exercises the public
reducer API for the minimum finite six-pair workspace and physical-device
intervals, balanced ordering, improvement and regression decisions,
no-material-regression tails, same-binary current/current noise admission, strict
unknown-field rejection, both metric directions, an isolated higher-is-better
lower-tail collapse with unchanged p50/p95/p99/maximum summaries, typed and
bounded invalidation, survivor-order balance, equal per-pair sample counts,
invalid-pair evidence validation, persisted-null compatibility, cold-start
warmup handling, byte-deterministic serialization, and the reanalysis-only CLI.
The CLI test requires exact-method metadata and the absence of legacy bootstrap
fields.
Tests are kept outside production source and documented in
[`tests/paired_experiment_tests.md`](tests/paired_experiment_tests.md).

## Changelog

- 2026-08-07: made version-3 evidence structs reject unknown fields and required
  identical binary hashes for current/current noise-control admission.
- 2026-08-07: advanced the unpublished schema to version 3, reduced workspace
  CPU studies to the requirement-minimal six balanced pairs, required each
  producer's predeclared-seed balanced order without resampling, and added
  symmetric current/current noise admission.
- 2026-08-07: replaced the fixed-seed 100,000-resample approximation with an
  exact binomial median interval, persisted achieved coverage and rank bounds,
  raised physical-device evidence from five to the requirement-minimal six
  pairs, and advanced the unpublished schema to version 2.
- 2026-08-06: closed invalidation to four mechanically evidenced reasons,
  capped invalid pairs at 10%, preserved survivor AB/BA balance, required equal
  valid-pair sample counts, and validated all invalid-pair evidence.
- 2026-08-06: made unpublished v1 reports self-auditing with p05, p01, minimum, and metric direction; higher-is-better admission consumes those serialized fields while lower-is-better gates remain unchanged.
- 2026-08-02: reconstructed the offline reducer without acquisition,
  source-mutation, browser, host, workflow-adapter, or SHA-256 dependencies;
  corrected higher-is-better relative-regression direction.
