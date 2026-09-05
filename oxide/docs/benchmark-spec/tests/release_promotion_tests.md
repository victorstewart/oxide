# oxide-benchmark-spec tests/release_promotion_tests

## Intention and purpose

This suite proves that release promotion is all-or-nothing and consumes the same hash-bound runtime evidence shape emitted by the macOS correctness applications.

## Relation to the rest of the code

Temporary fixtures model a controller pair directory, candidate specification tree, and absent output. Tests call `promote_release_candidates` only through its public API.

## Entry points list

- `missing_evidence_fails_without_publishing_output()` covers incomplete runtime capture.
- `invalid_visual_evidence_fails_without_publishing_output()` covers a hash-valid but visually divergent Oxide screenshot.
- `noncanonical_runtime_geometry_profile_fails_without_publishing_output()` covers profile drift on both sides.
- `valid_evidence_atomically_publishes_all_five_runnable_manifests()` covers the complete artifact and qualification output.

## Logic narrative

Helpers create per-side `evidence.json` records whose identities bind state, accessibility, geometry, and screenshot bytes. Mutation helpers update both the artifact and its declared hash so tests reach the intended semantic gate rather than failing earlier on integrity.

## Preconditions and postconditions

Each test owns a temporary directory. Rejection cases prove no output is published; success proves every manifest contains a real geometry identity and that the geometry file exists.

## Edge cases and failure modes

Missing bytes, visual mismatch, and invented capture profiles are exercised explicitly.

## Concurrency and memory behavior

Tests are synchronous and filesystem state is isolated by `TempDir`.

## Performance notes

The 24x24 PNG fixtures keep offline test cost bounded.

## Feature flags and cfgs

None.

## Testing and benchmarks

Run `cargo test --locked -p oxide-benchmark-spec --test release_promotion_tests`.

## Examples

`write_runtime_checkpoint` shows the expected controller artifact layout and camel-case Swift envelope fields.

## Changelog

- 2026-07-21: switched fixtures to real runtime checkpoint layout and added geometry-profile rejection.
