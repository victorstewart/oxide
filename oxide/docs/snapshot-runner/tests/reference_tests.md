# oxide-snapshot-runner tests `reference_tests.rs`

## Intention and purpose

These tests prevent renderer parity fixtures from passing merely because an output is nonblank. They freeze exact asymmetric ID-mask seed coordinates and require every jump-flood step to change distinguishable output.

## Relation to the rest of the code

- The tests exercise `oxide_snapshot_runner::reference` only.
- Later Metal and WebGPU capture comparisons consume the same asymmetric fixture and reference image contract.

## Entry points list

- `asymmetric_id_mask_seed_coordinates_and_values_are_exact`
- `every_asymmetric_id_mask_jfa_jump_changes_the_reference_fields`
- `id_mask_field_reference_image_encodes_seed_coordinates_and_ids`
- `committed_asymmetric_id_mask_stage_images_are_exact`

## Logic narrative

The fixture places four sparse semantic seeds in a 17x11 mask, including a same-city neighborhood boundary. Its non-power-of-two dimensions force jumps 16, 8, 4, 2, and 1; each step must alter at least one city or seam record.

## Preconditions and postconditions

The final city and seam fields must cover every output pixel, the encoded RGBA image must preserve all declared city IDs, and every committed seed/jump PNG must match byte-for-byte. `OXIDE_ID_MASK_REFERENCE_JSON` writes the same exact records for external backend comparison evidence.

## Edge cases and failure modes

Symmetric fixtures, seed-only signal checks, or broad image tolerances cannot satisfy these exact record assertions.

## Concurrency and memory behavior

Each test owns small vectors and performs no I/O or shared mutation.

## Performance notes

The fixture is intentionally small; it validates algorithm stages rather than throughput.

## Feature flags and cfgs

None.

## Testing and benchmarks

Run `cargo test --locked -p oxide-snapshot-runner --test reference_tests`.

## Examples

See the test fixture in `reference_tests.rs`.

## Changelog

- 2026-07-12: added asymmetric seed, per-jump, and field-image reference checks.
