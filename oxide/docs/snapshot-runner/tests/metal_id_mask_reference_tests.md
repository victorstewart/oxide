# oxide-snapshot-runner tests `metal_id_mask_reference_tests.rs`

## Intention and purpose

These macOS tests prove Metal's real ID-mask raster and final jump-flood fields match test-owned CPU references exactly, including packed storage at the contract dimensions.

## Relation to the rest of the code

- The fixture comes from `oxide_snapshot_runner::reference`.
- `oxide_renderer_metal` renders the fixture and exposes readback only through its existing `snapshot-tests` feature.

## Entry points list

- `metal_asymmetric_id_mask_raster_and_final_fields_match_cpu_reference`
- `metal_packed_id_mask_fields_match_cpu_reference_at_contract_dimensions` (explicit ignored release matrix)

## Logic narrative

Four sparse semantic pixels are encoded as one-pixel triangles. The regular test reads back R8 city/neighborhood masks and decoded city/seam fields, converts every field pixel to an integer seed record, and compares it to the CPU seed plus 16/8/4/2/1 jump sequence. The explicit release matrix propagates a known seed and checks every decoded final pixel at 256, 512, 1024, 2048, 257x509, 2048x257, and 511x1024 while also requiring two packed fields to use exactly one quarter of four wide fields' logical bytes.

## Preconditions and postconditions

The test requires macOS Metal. Every raster byte and every final field seed must match exactly.

## Edge cases and failure modes

Coverage-rule drift, field ping-pong selection errors, coordinate truncation, sentinel corruption, skipped jump sizes, semantic-ID recovery loss, or a field-byte ratio other than 4x fail with an element-level diff.

## Concurrency and memory behavior

Blocking readback occurs only after submission in snapshot-only code. Production frame paths never wait.

## Performance notes

The regular 17x11 fixture is a fast correctness oracle. The large matrix is ignored by default and intended for explicit optimized contract verification.

## Feature flags and cfgs

The test is macOS-only and uses `renderer-metal/snapshot-tests`.

## Testing and benchmarks

Run the fast oracle with `cargo test --locked -p oxide-snapshot-runner --test metal_id_mask_reference_tests`. Run the full matrix with `cargo test --locked --release -p oxide-snapshot-runner --test metal_id_mask_reference_tests metal_packed_id_mask_fields_match_cpu_reference_at_contract_dimensions -- --ignored`.

## Examples

See the test fixture.

## Changelog

- 2026-07-14: added packed-field byte-ratio and every-pixel reference coverage at 256/512/1024/2048 plus unusual aspect ratios.
- 2026-07-12: added exact Metal raster/final-field parity with the asymmetric CPU reference.
