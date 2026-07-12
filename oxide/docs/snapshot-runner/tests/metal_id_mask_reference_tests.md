# oxide-snapshot-runner tests `metal_id_mask_reference_tests.rs`

## Intention and purpose

This macOS test proves Metal's real ID-mask raster and final jump-flood fields match the test-owned CPU oracle exactly.

## Relation to the rest of the code

- The fixture comes from `oxide_snapshot_runner::reference`.
- `oxide_renderer_metal` renders the fixture and exposes readback only through its existing `snapshot-tests` feature.

## Entry points list

- `metal_asymmetric_id_mask_raster_and_final_fields_match_cpu_reference`

## Logic narrative

Four sparse semantic pixels are encoded as one-pixel triangles. The test reads back R8 city/neighborhood masks and the final RGBA32F city/seam fields, converts field pixels to integer seed records, and compares every element to the CPU seed plus 16/8/4/2/1 jump sequence.

## Preconditions and postconditions

The test requires macOS Metal. Every raster byte and every final field seed must match exactly.

## Edge cases and failure modes

Coverage-rule drift, field ping-pong selection errors, seed corruption, skipped jump sizes, or semantic-ID loss fail with an element-level diff.

## Concurrency and memory behavior

Blocking readback occurs only after submission in snapshot-only code. Production frame paths never wait.

## Performance notes

None; the 17x11 fixture is a correctness oracle.

## Feature flags and cfgs

The test is macOS-only and uses `renderer-metal/snapshot-tests`.

## Testing and benchmarks

Run `cargo test --locked -p oxide-snapshot-runner --test metal_id_mask_reference_tests`.

## Examples

See the test fixture.

## Changelog

- 2026-07-12: added exact Metal raster/final-field parity with the asymmetric CPU reference.
