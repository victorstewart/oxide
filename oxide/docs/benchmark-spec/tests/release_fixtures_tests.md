# oxide-benchmark-spec `tests/release_fixtures_tests`

## Intention and purpose

These tests lock the public release-fixture decoding surface to the frozen candidate files.

## Relation to the rest of the code

The tests read comparison-only fixture JSON and exercise `oxide_benchmark_spec::release_fixtures` through public types.

## Entry points

- `release_fixtures_decode_through_strict_public_types()` validates all five fixtures.
- `release_fixture_types_reject_unknown_work()` validates fail-closed decoding.

## Logic narrative

Each frozen fixture is decoded, its defining cardinalities are checked, and an injected unknown field is rejected.

## Preconditions and postconditions

The repository fixture tree must exist. A pass means the checked JSON remains structurally consumable without undeclared work.

## Edge cases and failure modes

Missing fixture files, schema drift, or permissive unknown-field handling fail the suite.

## Concurrency and memory behavior

Tests are synchronous and bounded by the small fixture files.

## Performance notes

No production performance path is exercised.

## Feature flags and cfgs

None.

## Testing and benchmarks

Run `cargo test --locked -p oxide-benchmark-spec --test release_fixtures_tests`.

## Examples

Not applicable; this file is the executable example.

## Changelog

- 2026-07-21: Added strict fixture decoding coverage.
