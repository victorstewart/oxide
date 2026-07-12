# oxide-snapshot-runner tests `parity_manifest_tests.rs`

## Intention and purpose

These tests make C03 coverage omissions fail structurally before any renderer capture runs.

## Relation to the rest of the code

They validate the shared snapshot-runner parity manifest that will drive CPU, Metal, and WebGPU fixtures.

## Entry points list

- `parity_manifest_covers_every_required_scene_family`
- `id_mask_cross_backend_matrix_is_five_layouts_by_three_dprs`
- `sequence_manifest_freezes_every_required_transition`

## Logic narrative

The still-image manifest covers primitive, glyph, ID-mask, clip/layer/effect, Scene3D, image, animation, MSAA, and EDR families. ID-mask cases form an exact five-layout by three-DPR matrix. The sequence manifest contains each required renderer transition.

## Preconditions and postconditions

All IDs must be unique within the ID-mask matrix, every cross-backend row must name CPU, Metal, and WebGPU, and every tolerance must remain narrow.

## Edge cases and failure modes

Removing a DPR, layout, capability variant, backend, or sequence makes these tests fail even if unrelated goldens remain present.

## Concurrency and memory behavior

Tests inspect static manifest slices and allocate only one small ID set/vector.

## Performance notes

None; this is a correctness coverage gate.

## Feature flags and cfgs

None.

## Testing and benchmarks

Run `cargo test --locked -p oxide-snapshot-runner --test parity_manifest_tests`.

## Examples

See `PARITY_CASES` in `parity.rs`.

## Changelog

- 2026-07-12: froze C03 still-image, DPR/layout, and sequence coverage.
