# oxide-benchmark-spec::comparison

## Intention and purpose

`comparison` defines the framework-neutral serde contract for Section 8 comparison plans, cells, decision families, scenario packs, controller chunks, implementation/common identities, sessions, metric definitions, and typed raw JSONL observation rows. It keeps acquisition identity separate from the existing source A/B experiment identity.

## Relation to the rest of the code

- Planners will materialize `ComparisonPlan` before platform-specific acquisition starts.
- Apple, web, and future platform controllers consume `ScenarioPack` and `ControllerChunk` without importing a framework adapter type.
- Session writers persist `ComparisonSession` summaries and `RawObservationRow` streams.
- The validation layer checks plan identities and every cross-reference before platform acquisition.
- Analyzers consume `ComparisonCell`, `MetricDefinition`, and `DecisionFamily` after acquisition.

Call flow:

- versioned plan JSON
  - `ComparisonPlan`
  - controller chunks and scenario packs
  - paired `ComparisonSession` records
  - typed raw observation JSONL
  - metric/family analysis

## Entry points list

- `oxide_benchmark_spec::comparison::DecimalU64(pub u64)` encodes u64 timestamps, durations, indices, generations, byte counts, and counters as JSON decimal strings.
- `EvidenceRole` serializes the finite `required_claim` and `descriptive_diagnostic` cell roles.
- `DecisionClaimKind` serializes the five frozen Section 7 decision-family kinds.
- `DecisionAlternative` serializes lower- or upper-tail exact-test alternatives.
- `MetricDirection` serializes lower-is-better or higher-is-better polarity.
- `ComparisonOrder` serializes the finite AB or BA order for one paired session.
- `comparison_seed_from_content_sha256(content_sha256: &str) -> Result<DecimalU64>` freezes a plan-specific seed as the big-endian u64 represented by the first sixteen lowercase hexadecimal digits of its immutable content SHA-256.
- `balanced_comparison_order(seed: u64, pair_count: usize) -> Vec<ComparisonOrder>` expands that seed through the frozen `sha256-prefix-be64-xorshift64-abba-baab-v1` algorithm, emitting one deterministic ABBA or BAAB block for each four pairs and truncating only the final block.
- `ImplementationIdentity` identifies a source tree, build, executable or bundle, shipping payload, and comparator audit digest/status. Its status string is descriptive only; headline reference admission requires `comparator_acceptance::admit_comparator_acceptance` against the exact `reference_audit_sha256` bytes.
- `CommonIdentity` identifies the common harness, instrumentation, scenarios, traces, fixtures, assets, and fonts.
- `ComparisonPlan` holds every Section 8 plan field and embeds the complete cells, metrics, families, packs, chunks, and identities selected before acquisition.
- `ComparisonCell` binds one scenario/environment/reference/contender cell to its primary metric, pass, materiality/sufficiency policy, guardrails, and decision families.
- `DecisionFamily` holds a fixed claim kind, alpha, ordered members, exact-test resolution floor, and pair ceiling.
- `DecisionFamilyMember` identifies one cell/metric/boundary/alternative member in deterministic Holm order.
- `ScenarioPack` freezes scenario order, isolation/reset/readiness policy, timing ceilings, sentinel, trace capacity, and calibration evidence.
- `ControllerChunk` freezes pair order, included packs, pass, occupied-time ceiling, heartbeat expectation, bundled resource hash, and checkpoint generation.
- `ComparisonSession` records one implementation session within a pair, its monotonic bounds, environments, warmup rows, raw artifact, validation outcome, durable commit identity, and artifact hashes.
- `MetricDefinition` freezes metric polarity, scope, source, estimators, sample/block semantics, effect/zero/availability policy, boundaries, decision test, resolution floor, and clock uncertainty.
- `RawObservationTimestamp` records one named clock-domain timestamp as a decimal-string nanosecond count.
- `RawObservationValue` is a tagged value union for finite floating-point values, decimal u64 values, signed integers, booleans, and text.
- `RawObservationRow` is the typed JSONL row containing session/pass/scenario/phase identity, sample index, timestamps, metric/value, optional event/state identity, and quality flags.
- `validate_comparison_plan` rejects malformed identities, unknown scenarios, cells outside their selected pack, invalid pack/chunk/cell/metric/family references, family references with the wrong claim kind, unbounded controller chunks, out-of-range pair indices, and impossible exact-test floors.

## Logic narrative

Serde derives preserve the declared Section 8 field order when writing JSON. Every value that can exceed JavaScript's exact integer range, or that is semantically a timestamp/duration/index/generation/counter, passes through `DecimalU64`; serialization emits a JSON string and deserialization requires a string that parses as u64. Apple campaign materialization derives this decimal seed from the exact source-plan content identity, then records the resulting `ComparisonOrder` on both sessions in each pair so the analyzer can reject an order or launch-side mismatch. `RawObservationValue` uses explicit `kind` and `value` keys so a reader does not infer measurement type from JSON number width. Optional family, guardrail, invalidation, and hard-outcome fields remain present as `null`, keeping one stable object key shape across valid, descriptive, and failed records.

Policy-controlled vocabularies that Section 8 names but does not enumerate remain strings. Explicitly finite semantics use enums. The environment fields use `serde_json::Value` until the separate reproducibility-manifest schema owns their nested field set. `validate_comparison_plan` deliberately does not interpret a comparator status string as approval; the separate strict audit validator verifies the materialized artifact, evidence closure, and reviewer signoff before acquisition.

## Preconditions and postconditions

- SHA-256 and policy strings are identities supplied by the planner or collector; `validate_comparison_plan` checks hash encoding and required identity/policy presence.
- Floating-point raw values must be finite before JSON serialization.
- Successfully deserialized `DecimalU64` values fit in u64 and originated as JSON strings.
- Round-tripping any comparison value preserves its typed data and the frozen top-level key set.

## Edge cases and failure modes

Numeric JSON tokens, negative values, non-decimal strings, and values above u64 maximum fail `DecimalU64` deserialization. Nullable family and outcome fields distinguish absence from an empty identity. Metric consistency, missing identities, invalid decision families, claim-kind mismatches, pack/cell mismatches, and nonfinite decision alphas fail in the validation layer.

## Concurrency and memory behavior

All types own their data, use no global state, and contain no synchronization. They are safe to move between acquisition and persistence workers according to their derived field types. Plans and sessions allocate for strings, vectors, maps, and environment JSON; raw collection code should batch or reuse buffers rather than construct these rows on a renderer hot path.

## Performance notes

The contract is designed for planning, checkpoint, artifact, and JSONL boundaries outside measured rendering work. `BTreeMap` gives deterministic artifact-hash ordering. Decimal conversion formats during serde I/O, which is acceptable only outside measurement windows.

## Feature flags and cfgs

No feature or target cfg changes the schema or serialized key/value spellings.

## Testing and benchmarks

`tests/comparison_tests.rs` round-trips representative plans and sessions, freezes every Section 8 object key set and top-level key order, verifies enum spellings, and proves u64 values serialize as decimal strings. No performance case is needed because this slice defines off-window artifact schema and does not alter acquisition or renderer behavior.

## Examples

```rust
let timestamp = DecimalU64(18_446_744_073_709_551_615);
let json = serde_json::to_string(&timestamp)?;
assert_eq!(json, "\"18446744073709551615\"");
```

## Changelog

- 2026-07-21: made content-derived seeds and deterministic four-pair ABBA/BAAB ordering a benchmark-spec-owned contract shared by Apple campaign materialization and analyzer-compatible session evidence.
- 2026-07-19: documented that comparison identity status is descriptive and cannot replace strict ComparatorAcceptance admission.
- 2026-07-17: added the Section 8 comparison plan/session/metric/decision contract and typed decimal-safe raw observation rows.
