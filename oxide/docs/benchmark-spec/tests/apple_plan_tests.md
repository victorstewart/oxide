# `oxide-benchmark-spec::tests::apple_plan_tests`

## Intention and purpose

These integration tests freeze the transitive root identity for the Apple PR comparison plan.

## Relation to the rest of the code

The tests call the public Apple-plan loader, canonical serializer, hash function, and validator against both committed artifacts and an isolated temporary spec root.

## Entry points list

- `apple_pr_plan_transitively_binds_every_selected_scenario()` proves one changed scenario byte invalidates the prior plan and changes its rematerialized hash.
- `committed_apple_pr_plan_is_canonical_and_valid()` proves committed bytes equal canonical serialization and bind the acquisition, budget, comparator audit, and six scenarios.

## Logic narrative

The temporary fixture copies every directly bound artifact, constructs identities from observed bytes, validates the complete root, mutates one scenario, and proves fail-closed behavior before rebuilding the identity.

## Preconditions and postconditions

All copied inputs are regular files under the temporary root. Success proves deterministic plan closure, not comparator acceptance.

## Edge cases and failure modes

Missing inputs, hash drift, noncanonical paths, duplicate or unordered comparator identities, and reordered scenarios are rejected by the public validator.

## Concurrency and memory behavior

Tests are synchronous and use bounded temporary files.

## Performance notes

This is control-plane validation outside measured applications.

## Feature flags and cfgs

None.

## Testing and benchmarks

Run `cargo test --locked -p oxide-benchmark-spec --test apple_plan_tests`.

## Examples

The `plan_for_root` helper shows a minimal plan with one platform-specific comparator binding.

## Changelog

- 2026-07-19: added comparator-audit plan closure coverage.
