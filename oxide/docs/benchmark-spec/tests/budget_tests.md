# oxide-benchmark-spec tests::budget_tests

## Intention and purpose

This integration suite proves the committed default budget files reproduce benchmark specification Section 15 exactly and fail when required arithmetic is corrupted.

## Relation to the rest of the code

- Loads public fixtures from the workspace.
- Exercises public canonical serialization and validators.
- Protects `xtask compare-ui validate/plan` from silently accepting changed time contracts.

Call flow:

- committed JSON
  - loader
  - default-set validator
  - canonical byte comparison
- mutated in-memory budget
  - single-budget validator failure

## Entry points list

The file exports no library entry points. Rust's test harness reaches its four `#[test]` functions.

## Logic narrative

One test validates and byte-round-trips all nine fixtures. A focused Apple PR test freezes 155 + 480 + 120 seconds, its 151-second reserve, and the 906-second hard ceiling. A nightly web test proves two disjoint workers retain 6,408 aggregate seconds and 4,968 critical-wall seconds. Mutation tests delete launch time or reserve and require rejection.

## Preconditions and postconditions

The committed workspace fixture root is available relative to `CARGO_MANIFEST_DIR`. Passing tests prove budget-fixture consistency only, not runtime duration or scenario coverage.

## Edge cases and failure modes

Coverage includes missing arithmetic through changed component values, removed reserve, `u64` overflow, shard aggregation, and noncanonical JSON bytes.

## Concurrency and memory behavior

Tests are read-only except for local in-memory mutations and share no mutable state.

## Performance notes

Runtime is negligible because only nine small files are parsed.

## Feature flags and cfgs

No feature or target branches exist.

## Testing and benchmarks

Run `cargo test --locked -p oxide-benchmark-spec --test budget_tests` from the `oxide` workspace.

## Examples

The Apple PR assertion makes the 15.1-minute reserve-inclusive ceiling executable rather than prose.

## Changelog

- 2026-07-17: introduced canonical-set, Apple PR, nightly web shard, and corruption tests.
