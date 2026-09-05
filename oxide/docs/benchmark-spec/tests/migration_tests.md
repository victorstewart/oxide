# benchmark-spec tests::migration_tests

## Intention and purpose

These tests protect the legacy Apple migration gate from silent method or risk loss.

## Relation to the rest of the code

They exercise `validate_legacy_case_disposition` with small reviewed inventories independently of Swift source parsing.

## Entry points list

Rust's test harness invokes the exact-mapping and removed-risk cases.

## Logic narrative

The suite accepts one complete two-method manifest, then rejects duplicate mapping, missing mapping, and a required risk whose only apparent owner is marked `remove_duplicate`.

## Preconditions and postconditions

No repository or device state is required.

## Edge cases and failure modes

The cases cover the failure modes that could otherwise make legacy deletion appear complete while dropping coverage.

## Concurrency and memory behavior

Tests use bounded in-memory values without shared state.

## Performance notes

No timing or acquisition is performed.

## Feature flags and cfgs

None.

## Testing and benchmarks

Run `cargo test --locked -p oxide-benchmark-spec --test migration_tests`.

## Examples

See the two-method sample manifest in the test source.

## Changelog

- 2026-07-17: added exact mapping and risk-preservation regression coverage.
