# oxide-benchmark-spec::tests::comparator_acceptance_tests

## Intention and purpose

These integration tests prove the ComparatorAcceptance gate cannot be replaced by a status string or an unverified reviewer claim.

## Relation to the rest of the code

- Exercises the public audit schema, canonical serializer, signed-payload hash, loader, and strict admission function.
- Uses isolated temporary workspace roots for admitted synthetic audits.
- Reads the committed AppKit audit and its frozen source files without launching the target.

## Entry points list

- `fully_signed_accepted_audit_is_admitted()` covers the complete positive contract.
- `missing_or_changed_audit_fails_admission()` covers missing bytes and top-level digest drift.
- `pending_audit_fails_admission()` proves generic pending serde cannot enter acquisition.
- `accepted_audit_without_reviewer_signoff_fails_admission()` proves an `accepted` status cannot substitute for a signed disposition.
- `missing_retained_scenario_profile_fails_admission()` proves exact scenario-profile coverage.
- `five_percent_cpu_stack_without_disposition_fails_admission()` freezes the inclusive 500-basis-point threshold.
- `one_refresh_stall_without_disposition_fails_admission()` freezes the inclusive refresh-interval threshold.
- `retrospectively_selected_variant_fails_admission()` rejects post-result variant selection.
- `unexplained_greater_than_ten_percent_ceiling_gap_fails_admission()` freezes the strict greater-than-1,000-basis-point publication block.
- `changed_referenced_evidence_fails_admission()` proves nested evidence hashes are checked.
- `incomplete_source_snapshot_fails_admission()` proves an audit cannot omit a required compiled source input.
- `committed_appkit_audit_is_canonical_truthful_and_pending_independent_qualification()` prevents the current AppKit target from acquiring an invented acceptance or reviewer signoff.

## Logic narrative

The positive fixture materializes all evidence, computes the unsigned canonical payload hash, attaches detached signature evidence, materializes the canonical audit, and admits it against an exact identity/scenario/cell expectation. Negative tests change one contract condition, rematerialize and resign when needed so they exercise semantic admission rather than an incidental stale-signature failure, and assert rejection at the intended gate.

## Preconditions and postconditions

Tests run from the Cargo workspace with the committed AppKit sources present. Passing proves all named publication blockers fail strict admission and the committed rejected audit remains canonical and source-addressed.

## Edge cases and failure modes

Temporary roots include a process ID, nanosecond timestamp, and atomic sequence so concurrent tests cannot share artifact paths. Each test removes its own root after the assertion.

## Concurrency and memory behavior

Tests may run concurrently but use independent temporary directories. Fixtures are small and perform no networking, process launch, app launch, or device access.

## Performance notes

This is control-plane contract coverage, not a throughput benchmark.

## Feature flags and cfgs

No feature or target cfg changes these tests.

## Testing and benchmarks

Run `cargo test --locked -p oxide-benchmark-spec --test comparator_acceptance_tests` from the `oxide` workspace.

## Examples

The positive fixture demonstrates how to compute `signed_payload_sha256` before attaching `ComparatorReviewerSignoff`.

## Changelog

- 2026-07-19: added strict ComparatorAcceptance admission and committed rejected-AppKit artifact coverage.
- 2026-07-21: bound admission expectations to required source, dependency, and build-recipe paths.
