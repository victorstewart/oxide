# oxide-benchmark-spec tests/correctness_geometry_tests

## Intention and purpose

This suite proves that only the frozen macOS correctness capture profile can supply reducer geometry.

## Relation to the rest of the code

The tests exercise `decode_macos_correctness_geometry` through its public API and protect runtime capture, controller reduction, and release promotion from independent scale defaults.

## Entry points list

- `canonical_runtime_geometry_round_trips_with_exact_profile_and_scale()` covers valid landscape evidence.
- `noncanonical_profile_scale_origin_and_pixel_grid_fail_closed()` covers the principal invalid contracts.

## Logic narrative

One test decodes a complete runtime artifact. The rejection test changes one invariant per input while preserving the remaining fields.

## Preconditions and postconditions

Fixtures are in-memory UTF-8 JSON. Passing proves the public decoder accepts the canonical profile and rejects each mutated invariant.

## Edge cases and failure modes

The suite covers an unknown profile, wrong scale, translated origin, and fractional logical dimension.

## Concurrency and memory behavior

Tests are synchronous and use no filesystem, threads, or shared state.

## Performance notes

Inputs are tiny and run outside benchmark windows.

## Feature flags and cfgs

None.

## Testing and benchmarks

Run `cargo test --locked -p oxide-benchmark-spec --test correctness_geometry_tests`.

## Examples

The valid fixture is a direct example of the schema emitted by Swift.

## Changelog

- 2026-07-21: added canonical and fail-closed geometry cases.
