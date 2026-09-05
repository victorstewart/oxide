# Typed PR fixture tests

## Intention and purpose

`crates/benchmark-spec/tests/pr_fixtures_tests.rs` proves all six exported PR fixture types match the committed materialized bytes and that semantic fixture drift cannot pass merely by updating an artifact hash.

## Relation to the rest of the code

- Reads all six committed fixture JSON files through the public schema types.
- Calls `validate_pr_vertical_slice` through its public API after copying the versioned spec root and corrupting one deterministic feed row.
- Complements `pr_scenario_tests.rs`, which proves the unchanged artifact closure succeeds.

## Entry points list

The integration-test binary has no public entry points. Cargo's test harness reaches the vertical and remaining-PR deserialization cases, the unknown-field rejection case, and the matching-hash feed/chat semantic-drift cases.

## Logic narrative

The happy-path tests parse each committed fixture and check exact headline counts, byte sizes, image dimensions, and visible-card state. The schema test inserts an undeclared dashboard key and proves Serde rejects it. Semantic-drift tests copy the spec root, alter feed favorite state or chat append frequency, recompute the fixture SHA-256 in the in-memory scenario, and prove exact validation still rejects the workload.

## Preconditions and postconditions

The Cargo workspace contains the committed v1 comparison artifacts. Tests never modify those artifacts; corruption occurs only in a uniquely named temporary copy, which is removed on drop.

## Edge cases and failure modes

Unreadable fixtures, malformed JSON, copy failures, unexpected schema acceptance, an artifact-only validator that overlooks semantic drift, or a changed error classification fail the tests.

## Concurrency and memory behavior

Each test owns its data. The semantic-drift test copies approximately one spec-root artifact set to a process-and-time-unique temporary directory, avoiding shared mutations across concurrent test processes.

## Performance notes

The test copy is bounded by the committed v1 artifact set and occurs only in tests. Production validation remains a single linear parse and comparison per fixture.

## Feature flags and cfgs

No feature or target cfg changes the tests.

## Testing and benchmarks

Run `cargo test --locked -p oxide-benchmark-spec --test pr_fixtures_tests`. No benchmark is needed for planning-time validation.

## Examples

The tests themselves are executable examples of reading the public fixture types and validating a full vertical slice.

## Changelog

- 2026-07-18: expanded typed deserialization and matching-hash drift coverage to startup, chat, and image.
- 2026-07-18: added typed deserialization, unknown-field, and matching-hash semantic-drift coverage.
