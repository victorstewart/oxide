# xtask tests::legacy_migration_tests

## Intention and purpose

These tests protect strict extraction of the legacy XCTest method inventory.

## Relation to the rest of the code

They isolate `extract_swift_test_methods`; the CLI suite separately validates the committed 197-method manifest.

## Entry points list

Rust's test harness invokes the valid-inventory and malformed-declaration cases.

## Logic narrative

The valid case extracts two test declarations while ignoring a helper. The failure case rejects missing argument lists and nonidentifier characters.

## Preconditions and postconditions

No repository or device state is required.

## Edge cases and failure modes

Malformed declarations fail rather than silently disappearing from migration coverage.

## Concurrency and memory behavior

Tests operate on static bounded strings.

## Performance notes

No timing or acquisition is performed.

## Feature flags and cfgs

None.

## Testing and benchmarks

Run `cargo test --locked -p xtask --test legacy_migration_tests`.

## Examples

See the inline Swift sample in the test source.

## Changelog

- 2026-07-17: added inventory extraction coverage.
