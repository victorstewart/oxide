# `oxide-apple-comparison-controller` `tests/comparator_qualification_tests.rs`

## Intention and purpose

These integration tests freeze the public macOS comparator-qualification contract across all 13 release scenario families.

## Relation to the rest of the code

The tests call only public controller APIs and build isolated temporary content-addressed fixtures, overlays, runtime attestations, and Time Profiler artifacts.

## Entry points list

The five `#[test]` functions cover complete deterministic reduction, missing or changed artifacts, runtime-attestation mismatch, threshold disposition, and mandatory gates.

## Logic narrative

One fixture expands every scenario into AppKit/Oxide by 1x/2x evidence. A synthetic profile puts one stack exactly at 500 basis points while keeping all other persisted stacks below the threshold, and supplies one stall immediately below and one exactly at the refresh interval.

## Preconditions and postconditions

Temporary artifacts carry hashes of their exact bytes. A complete matrix accepts; every deliberate mutation either fails admission or produces a rejected report as specified.

## Edge cases and failure modes

The suite covers absent 2x variants, absent profiles, profile mutation, overlay-attestation drift, two application runs, missing dispositions, and each of the three gates.

## Concurrency and memory behavior

Tests are deterministic, use no sleeps or network, and own independent temporary directories.

## Performance notes

Fixtures are small synthetic metadata; no measured application runs in this unit suite.

## Feature flags and cfgs

None.

## Testing and benchmarks

Run `cargo test --manifest-path host/apple-comparison/comparison-controller/Cargo.toml --test comparator_qualification_tests --locked` from the `oxide` workspace root.

## Examples

The `QualificationFixture` helper demonstrates complete public input construction.

## Changelog

- 2026-07-21: added deterministic all-13 side/scale coverage and fail-closed mutation tests.
