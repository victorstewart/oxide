# renderer-api `retained.rs`

## Intention and purpose

`retained.rs` defines backend-neutral immutable render chunks, persistent sequence composition, frame snapshots, and compact dynamic property metadata. C26 keeps node geometry immutable while transform and opacity change through stable property slots.

## Relation to the rest of the code

- `oxide-ui-core::NodeTree` owns slot allocation and emits node-local chunk instances plus frame property values.
- Metal and WebGPU prepare immutable chunk geometry once and resolve instance properties into their own completion-safe rings.
- The flat adapter remains a checked compatibility path for translation/opacity and rejects affine transforms it cannot preserve.

## Entry points

- `RenderPropertySlotId::dynamic(index, generation)` packs a nonzero dense index and generation into a sorted renderer-neutral ID.
- `RenderPropertySlotId::{dynamic_index,dynamic_generation}` expose generation validation without backend-specific state.
- `RenderSnapshot::uniform_property_revision` precomputes the shared epoch for the common one-transform/one-opacity-per-instance shape so backends can select a checked all-record update without rescanning immutable snapshot metadata.
- `RenderDynamicClip` associates a retained clip rectangle with the transform slot that places it.
- `RenderChunkInstance::{property_slots,dynamic_clips}` carry small frame-varying metadata beside immutable geometry.
- `RenderSnapshot::from_sequences` validates sorted unique properties, one live generation per dynamic index, complete references, finite values, and clip-transform types.

## Invariants and failure modes

- Dynamic index zero and generation zero are reserved; out-of-range packing returns `None`.
- One snapshot cannot contain two generations of the same dynamic index.
- Every property and dynamic clip reference must resolve exactly once.
- Dynamic clips must reference transform values, not opacity values.
- Flat replay supports translation-only transforms. Scale or rotation returns `UnsupportedFlatTransform` rather than drawing different pixels.

## Concurrency and memory behavior

Chunks, sequences, clip arrays, and property-slot arrays are immutable `Arc` payloads. Snapshot validation allocates no backend object. Slot generations prevent an old in-flight snapshot from aliasing a logically reused UI slot; backend rings retain the physical bytes for their own completion boundary.

## Performance notes

Property IDs sort by dense index then generation, so generation conflict validation is adjacent and linear. Dynamic clip metadata contributes to exact sequence byte accounting. Replacing instance metadata preserves chunk ownership and does not copy command or geometry arrays.

## Testing and benchmarks

- `renderer-api/tests/render_chunk_tests.rs` freezes packing, generation conflicts, clip typing, translated flat clips, and affine rejection.
- The C26 UI and backend rows prove zero geometry rebuild/upload after warmup.

## Changelog

- 2026-07-13: added generation-checked dense dynamic property IDs, transform-linked clip metadata, exact validation/accounting, and checked flat translation support for C26.
