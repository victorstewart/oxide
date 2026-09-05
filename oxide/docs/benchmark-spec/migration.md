# benchmark-spec migration

## Intention and purpose

This module freezes the reviewed cutover from the legacy Apple per-method XCTest surface to data-driven comparison scenarios, internal microbenchmarks, fast correctness tests, specialized suites, and separately labeled native-ceiling work.

## Relation to the rest of the code

`benchmarks/comparative/legacy_case_disposition.json` groups every source method by one disposition and replacement target. It separately records pre-existing v1 coverage gaps so weak legacy evidence cannot be credited as complete. `xtask compare-ui validate` extracts the current XCTest method inventory and requires exact set equality before legacy deletion or packed-plan routing can proceed.

## Entry points list

- `LegacyCaseDisposition`: complete reviewed manifest.
- `LegacyDispositionGroup`: one replacement target and its methods/risks.
- `LegacyDisposition`: the six Section 3 disposition categories.
- `validate_legacy_case_disposition`: exact mapping and risk-preservation gate.

## Logic narrative

Validation rejects duplicate group IDs, duplicate or empty method mappings, unknown risk dimensions, missing or unexpected source methods, required risks credited only to removed duplicates, malformed gap records, and risks claimed as both covered and missing. The validator compares sets only after every group-level invariant passes.

## Preconditions and postconditions

The caller supplies the exact current method inventory from both reviewed XCTest files. Success proves each method appears once and each required risk dimension remains owned by at least one non-removed target.

## Edge cases and failure modes

A source method added after review, a deleted method left in the manifest, overlapping groups, empty targets, or loss of the only risk owner fails validation.

## Concurrency and memory behavior

Validation is synchronous over bounded ordered sets and allocates only the reviewed manifest inventory.

## Performance notes

This is an offline correctness gate. It avoids device acquisition when migration coverage is stale.

## Feature flags and cfgs

None.

## Testing and benchmarks

`tests/migration_tests.rs` covers exact mapping, duplicate/missing methods, and risk loss through duplicate removal.

## Examples

Load the committed manifest, extract `Suite.testMethod` names from both Swift files, and call `validate_legacy_case_disposition` before planning.

## Changelog

- 2026-07-17: added the exact legacy-method disposition and risk-preservation contract.
