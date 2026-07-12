# oxide-snapshot-runner library

## Intention and purpose

The snapshot-runner library owns deterministic, test-only CPU reference algorithms shared by renderer correctness tests. It does not participate in an Oxide application or renderer hot path.

## Relation to the rest of the code

- Snapshot-runner integration tests use `reference` to validate intermediate renderer fields before comparing backend images.
- Metal and WebGPU remain production renderers; neither depends on this library.

## Entry points list

- `parity::PARITY_CASES` freezes scene, layout, DPR, backend, dimensions, and pixel tolerance for every still-image parity case.
- `parity::SEQUENCE_CASES` freezes the required multi-frame renderer transitions.
- `reference::id_mask_seed_fields` builds exact nearest-city and same-city neighborhood-seam seed records from integer masks.
- `reference::id_mask_jump_schedule` matches the backend power-of-two jump schedule.
- `reference::id_mask_jump_fields` runs one deterministic 3x3 jump-flood step with shader-compatible strict-distance tie handling.
- `reference::id_mask_field_rgba` encodes field coordinates and semantic IDs as a deterministic reference image.
- `reference::asymmetric_id_mask_fixture` supplies the shared sparse non-power-of-two semantic masks.
- `reference::id_mask_fields_rgba` stacks city and seam coordinate/ID fields into one exact stage image.

## Logic narrative

Seed records preserve their integer source coordinate, city ID, and neighborhood ID. Invalid records use negative coordinates and zero IDs. A jump step checks the current record followed by the eight offset records in the same order as the Metal and WGSL shaders, replacing the winner only for a strictly smaller squared distance.

## Preconditions and postconditions

- City and neighborhood masks must match `width * height`.
- Jump sizes must be non-zero.
- Output dimensions and record counts always match the input masks.

## Edge cases and failure modes

- Empty semantic pixels do not create seeds.
- Out-of-bounds jump candidates remain invalid.
- Equal-distance candidates preserve the earlier seed, matching shader comparison semantics.

## Concurrency and memory behavior

Reference functions own their output vectors and share no mutable state. They are test tooling and are not frame-loop code.

## Performance notes

This is a correctness oracle, not an optimized rasterizer. Its work is deliberately direct and deterministic.

## Feature flags and cfgs

None.

## Testing and benchmarks

`reference_tests.rs` freezes asymmetric seeds, every JFA jump, coordinate/ID encoding, and six committed exact stage images.

## Examples

```rust
let fields = oxide_snapshot_runner::reference::id_mask_seed_fields(1, 1, &[1], &[2]);
assert!(fields.city[0].valid());
```

## Changelog

- 2026-07-12: added the shared still-image and sequence parity manifests.
- 2026-07-12: added the CPU ID-mask seed and jump-flood reference used by C03 parity fixtures.
