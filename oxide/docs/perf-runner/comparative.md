# oxide-perf-runner::comparative

## Intention and purpose

`oxide_perf_runner::comparative` is the framework-comparison analysis boundary. It exposes direction-normalized pair effects, median decision estimands, hierarchical time-block bootstrap intervals, exact sign tests, deterministic Holm adjustment, sufficiency-aware classification, and exact-resolution planning without overloading the existing baseline-commit/candidate-index paired schema.

## Relation to the rest of the code

- Reuses the crate-private `paired_statistics` implementation so source A/B and framework comparison share decision mechanics.
- Will consume `oxide-benchmark-spec` plans and typed comparison sessions as the comparison lab is built out.
- Keeps implementation identities and comparative decisions separate from `paired::PairedExperimentInput`.

Call flow:

- comparative analyzer
  - direction-normalized per-pair effects
  - `exact_sign_test`
  - `holm_adjust`
  - classification and report rendering

## Entry points list

- `oxide_perf_runner::comparative::exact_sign_test(values: &[f64], boundary: f64, alternative: DecisionAlternative) -> anyhow::Result<ExactSignTest>` computes an exact one-sided paired sign test after removing boundary ties.
- `oxide_perf_runner::comparative::holm_adjust(members: &[HolmMember]) -> anyhow::Result<Vec<HolmDecision>>` applies deterministic Holm multiplicity correction.
- `oxide_perf_runner::comparative::exact_sign_test_resolution_floor(family_size: usize, alpha: f64) -> anyhow::Result<usize>` computes the best-case pair floor for a decision family.
- `oxide_perf_runner::comparative::normalized_pair_effect(...)` maps ratio and zero-capable metrics into a worse-is-positive effect.
- `oxide_perf_runner::comparative::decision_estimand(pair_effects: &[f64])` computes the schema-v1 across-session median, including the arithmetic midpoint for an even population.
- `oxide_perf_runner::comparative::hierarchical_block_bootstrap_ci(...)` outer-resamples complete pair indices and inner-resamples contiguous within-session time blocks.
- `oxide_perf_runner::comparative::classify_comparison(...)` emits a framework label only from sufficient, unambiguous adjusted decisions and gives terminal hard outcomes precedence.
- Public data types describe alternatives, exact counts/rationals, Holm inputs, and adjusted decisions.

## Logic narrative

Strictly positive metrics use the natural log of Oxide/reference badness, reversing the ratio for higher-is-better metrics. Zero-capable metrics use a worse-is-positive native-unit difference. Hierarchical resampling preserves complete pairs at the outer level and time locality through circular contiguous blocks at the inner level. The sign test counts effects strictly below and above a frozen decision boundary, removes exact ties, and forms the requested binomial upper tail using exact integer coefficients before converting the rational to a decimal p-value. Holm adjustment sorts by raw p-value and then cell, metric, and boundary identifiers; each adjusted value is the running maximum of the current multiplicity-scaled p-value. The planner floor is `ceil(log2(family_size / alpha))`.

## Preconditions and postconditions

- Pair effects and boundaries are finite.
- Exact sign tests accept at most 63 non-tied pairs, which exceeds every frozen comparison tier maximum while keeping exact binomial arithmetic inside `u128`.
- Holm p-values are finite and inside `[0, 1]`; stable identifiers are nonempty.
- Output ordering and adjusted values are deterministic for identical members.

## Edge cases and failure modes

- Exact ties reduce effective sample size and contribute evidence to neither side.
- With no non-tied values the exact p-value is one; sufficiency validation must reject the cell before classification.
- Invalid alpha, empty families, nonfinite effects, oversized populations, and invalid p-values return errors.

## Concurrency and memory behavior

The helpers are single-threaded and allocate only bounded vectors proportional to the family or pair count. They use no mutable global state.

## Performance notes

Decision analysis runs after acquisition. Integer exactness and deterministic lexical ordering are favored over approximate or parallel hypothesis routines because comparison tiers contain small fixed pair populations.

## Feature flags and cfgs

The module has no feature- or target-specific behavior.

## Testing and benchmarks

`tests/comparative_analysis_tests.rs` freezes both metric directions and effect kinds, even-population median semantics, deterministic hierarchical resampling, classification precedence, both sign-test directions, exact tie removal, a unanimous 12-pair rational, Holm lexical ties and monotonicity, resolution floors, and invalid-input rejection.

## Examples

Twelve effects below an Oxide-win boundary yield the exact lower-tail probability `1 / 4096`; the family then passes that raw value through `holm_adjust` before classification.

## Changelog

- 2026-07-17: introduced the comparative decision boundary with exact paired sign tests, deterministic Holm adjustment, and exact-test resolution planning.
- 2026-07-17: added worse-is-positive effects, hierarchical block bootstrap intervals, and fail-closed classification vectors.
