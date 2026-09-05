# oxide-benchmark-spec::tests::comparison_tests

## Intention and purpose

These integration tests freeze the serde surface of the Section 8 comparative schema before platform adapters depend on it.

## Relation to the rest of the code

- Exercises only the public re-exports from `oxide_benchmark_spec`.
- Builds representative nested `ComparisonPlan` and `ComparisonSession` values.
- Detects accidental key additions, removals, reordering, enum spelling changes, or unsafe integer encoding.

Call flow:

- representative Rust value
  - serde JSON serialization
  - key-set and key-order assertions
  - serde JSON deserialization
  - typed equality assertion

## Entry points list

- `comparison_plan_round_trip_preserves_section_8_keys_and_order()` covers the plan and all nested identity, pack, chunk, cell, metric, family, and ordered-member objects.
- `comparison_session_and_raw_row_round_trip_with_typed_decimal_values()` covers session summaries, timestamps, and tagged raw observation values.
- `decimal_u64_requires_a_string_that_fits_u64()` covers u64 maximum, numeric-token rejection, overflow, and malformed decimal text.

## Logic narrative

Fixtures are constructed through the public Rust structs. Tests serialize them once, compare the declared top-level order in the raw JSON string, compare unordered key sets for every nested object, assert normative enum and decimal-string spellings, then deserialize and compare the complete typed value. A separate boundary test ensures JavaScript-unsafe u64 values never become JSON numbers.

## Preconditions and postconditions

The serde implementation is deterministic for struct fields. Passing tests prove the representative schema round-trips and all asserted Section 8 keys retain their spelling and presence.

## Edge cases and failure modes

The tests cover nullable optional fields, u64 maximum, overflow, numeric JSON tokens, malformed counter text, nested arrays, and tagged decimal raw values. Semantic schema validation remains out of scope.

## Concurrency and memory behavior

Tests are deterministic, single-process, and use no filesystem, network, sleeps, or shared mutable state.

## Performance notes

Fixtures are small and intended to maximize contract coverage, not benchmark serde throughput.

## Feature flags and cfgs

No feature or target cfg changes these tests.

## Testing and benchmarks

Run `cargo test --locked -p oxide-benchmark-spec --test comparison_tests` from the `oxide` workspace.

## Examples

The plan test demonstrates that `pass_pair_count: DecimalU64(12)` serializes as `"pass_pair_count":"12"`.

## Changelog

- 2026-07-17: added round-trip, frozen-key, ordering, enum-spelling, and decimal-u64 boundary coverage.
