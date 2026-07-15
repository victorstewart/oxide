# renderer-metal::tests::image_store_tests

## Intention and purpose

These Metal integration tests prove C60's image store against the production renderer rather than only a mock backend.

## Relation to the rest of the code

The tests use `oxide-image-store` to decode and place images, `MetalRenderer` as `ImageResidencyBackend`, and real prepared render chunks/readback to verify pixels and invalidation scope.

## Entry points list

- `image_store_atlas_matches_standalone_rgba_and_has_no_neighbor_bleed`
- `image_store_slot_eviction_invalidates_only_its_prepared_chunk`
- `image_store_slot_eviction_invalidates_only_its_retained_layer`

## Logic narrative

The pixel test uploads distinct neighboring colors into one atlas page, rejects short and out-of-bounds append attempts, draws resolved slots and standalone controls, completes Metal work, and compares readback. The invalidation tests prepare two chunks or retained layers sharing one page, release one slot, and verify only the referencing chunk/layer misses while the unrelated peer remains a cache hit.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

A compatible Metal device is required. Atlas and standalone output must match exactly, gutter sampling must not expose a neighbor, and slot eviction must preserve the unrelated prepared chunk. No test-only branch enters production renderer code.

## Edge cases and failure modes

Tests catch sRGB format mismatch, missing gutters, source-rectangle errors, slot-generation aliasing, whole-page invalidation, and loss of prepared-cache locality.

## Concurrency and memory behavior

Metal completion/readback establishes the GPU lifetime boundary. The renderer's serial submission queue owns texture mutation; tests do not access in-flight storage.

## Performance notes

The invalidation case protects the key append-only property: publishing or evicting one slot must not force every chunk that uses the page to rebuild.

## Feature flags and cfgs

Run with renderer-metal's `snapshot-tests` feature so readback helpers and integration targets are available.

## Testing and benchmarks

Run `cargo test --locked -p oxide-renderer-metal --features snapshot-tests --test image_store_tests` on Metal hardware. C60 physical-device perf rows supply latency/GPU distributions beyond these correctness checks.

## Examples

`image_chunk` shows how a resolved store region becomes a prepared renderer chunk.

## Changelog

- 2026-07-15: added physical Metal atlas/standalone pixel parity and exact prepared-chunk invalidation coverage.
