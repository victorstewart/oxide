# oxide-perf-runner tests::comparative_analysis_tests

## Intention and purpose

This integration suite freezes the direction, resampling, and exact small-sample decision mathematics used by framework-comparison reports and planners.

## Relation to the rest of the code

- Exercises only the public `oxide_perf_runner::comparative` boundary.
- Protects the shared private implementation from direction, tie, multiplicity, and resolution regressions.
- Supplies golden vectors required before comparative schemas and reports can depend on the engine.

Call flow:

- fixed effect vectors
  - `exact_sign_test`
  - exact rational/count assertions
- fixed decision-family members
  - `holm_adjust`
  - lexical rank and monotonic adjusted-value assertions
- family size and alpha
  - `exact_sign_test_resolution_floor`
- paired session vectors
  - direction-normalized effect
  - complete-pair/contiguous-block bootstrap
  - sufficiency-aware classification

## Entry points list

The file exports no library entry points. Rust's test harness reaches each `#[test]` function.

## Logic narrative

Tests feed known effects around a zero or materiality boundary into both exact-test directions, verify ties are excluded, and check the exact rational before decimal conversion. Separate members with tied p-values prove lexical ordering and Holm's running-maximum rule. Invalid values exercise fail-closed validation.

## Preconditions and postconditions

- Test vectors use fixed decimal values and stable IDs.
- Passing results reproduce the normative exact counts, rational p-values, adjusted order, and pair floors.

## Edge cases and failure modes

Coverage includes both metric directions, ratio and zero-capable effects, zero ratio inputs, exact ties, unanimous effects, both alternatives, tied raw p-values, empty families, insufficient evidence, ambiguous decisions, terminal hard outcomes, invalid alpha, NaN effects, infinite boundaries, and p-values outside `[0, 1]`.

## Concurrency and memory behavior

Tests share no mutable state and allocate only small vectors, so parallel execution is safe.

## Performance notes

The vectors are intentionally tiny because release comparison populations are fixed small-pair designs. Test runtime is negligible beside acquisition or bootstrap analysis.

## Feature flags and cfgs

The tests have no feature- or target-specific behavior.

## Testing and benchmarks

Run with `cargo test --locked -p oxide-perf-runner --test comparative_analysis_tests` from the `oxide` workspace.

## Examples

The unanimous 12-pair lower-tail case freezes `1 / 4096`, while the three-member Holm case freezes lexical order `cell-a`, `cell-b`, `cell-c`.

## Changelog

- 2026-07-17: introduced exact sign-test, Holm, resolution-floor, and invalid-input golden vectors.
- 2026-07-17: added pair-effect, even-median, hierarchical bootstrap, and classification golden vectors.
