# xtask legacy_migration

## Intention and purpose

This module binds the committed legacy disposition review to the live Apple XCTest source inventory.

## Relation to the rest of the code

`compare-ui validate` reads the two canonical legacy Swift sources, extracts every `func test...` declaration, and passes the exact qualified set to `oxide-benchmark-spec` migration validation. It also requires canonical committed JSON.

## Entry points list

- `validate_workspace_legacy_disposition`: end-to-end workspace migration gate.
- `extract_swift_test_methods`: strict source declaration extractor.
- `LEGACY_PERF_SOURCE_FILES`: the two reviewed source paths.

## Logic narrative

Each source file contributes `FileStem.testMethod` identities. Duplicate declarations, malformed test signatures, source-file drift, manifest mapping drift, risk loss, and noncanonical JSON all fail before planning or acquisition.

## Preconditions and postconditions

The workspace contains both canonical Swift files and `benchmarks/comparative/legacy_case_disposition.json`. Success returns the exact method and review-group counts.

## Edge cases and failure modes

The extractor ignores helpers and accepts only line-leading Swift `func test...(` declarations. A newly added test is immediately missing from the manifest and fails validation.

## Concurrency and memory behavior

Two bounded source files are read synchronously into memory. Ordered sets make diagnostics deterministic.

## Performance notes

This offline gate prevents stale migration plans from consuming physical-device time.

## Feature flags and cfgs

None.

## Testing and benchmarks

`tests/legacy_migration_tests.rs` covers valid extraction and malformed declarations. CLI tests exercise the real workspace manifest.

## Examples

Run `cargo xtask compare-ui validate` before planning or deleting legacy wrappers.

## Changelog

- 2026-07-17: added live Swift inventory and canonical manifest validation.
