# renderer-api `retained.rs`

## Intention and purpose

`retained.rs` defines backend-neutral immutable render chunks, persistent sequence composition, frame snapshots, compact dynamic property metadata, and conservative spatial metadata. C27 computes command bounds, clip state, scope ranges, and ordered damage indices once instead of rediscovering them during every backend frame.

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
- `RenderChunk::{command_spatial,paint_spans,query_damage_commands}` expose prepared local bounds, matched clip/layer scopes, and paint-ordered damage selection.
- `RenderSnapshot::{query_damage_instances,resolved_instance,precomputed_resolved_instances}` resolve world transforms/clips and query the stable instance index. Large property-free snapshots retain their resolved array; dynamic-property snapshots keep the C26 construction path allocation-free and resolve only when queried.

## Invariants and failure modes

- Dynamic index zero and generation zero are reserved; out-of-range packing returns `None`.
- One snapshot cannot contain two generations of the same dynamic index.
- Every property and dynamic clip reference must resolve exactly once.
- Dynamic clips must reference transform values, not opacity values.
- Flat replay supports translation-only transforms. Scale or rotation returns `UnsupportedFlatTransform` rather than drawing different pixels.
- Raster bounds include a one-point AA outset; blur and visual-effect bounds include a three-sigma effect outset. Glyph, solid, and image-mesh spans are scanned only while constructing their immutable chunk.
- Damage-query results are sorted by original paint order. Non-finite bounds remain unbounded and therefore cannot be incorrectly culled.

## Concurrency and memory behavior

Chunks, sequences, clip arrays, property-slot arrays, spatial arrays, and compact x-sweep indices are immutable `Arc` payloads. Snapshots with at least 32 property-free instances retain resolved world metadata and an instance index; smaller or property-driven snapshots avoid that storage. Slot generations prevent an old in-flight snapshot from aliasing a logically reused UI slot; backend rings retain the physical bytes for their own completion boundary.

## Performance notes

Property IDs sort by dense index then generation, so generation conflict validation is adjacent and linear. Chunk spatial queries use sorted minimum-x entries plus prefix maximum-x rejection and then restore paint order. A two-pixel query over 10,000 ordered glyph/mesh instances visits one spatial entry without revisiting any source vertex. Full damage bypasses querying and remains linear.

## Testing and benchmarks

- `renderer-api/tests/render_chunk_tests.rs` freezes packing, generation conflicts, clip typing, translated flat clips, affine rejection, AA/effect bounds, nested scope matching, transformed bounds, ordered queries, and zero vertex revisits.
- C27's CPU and Metal architecture/authoring rows cover 10,000 alternating glyph/mesh instances.

## Changelog

- 2026-07-13: added C27 conservative command/instance bounds, resolved clips, matching scope ranges, ordered compact spatial indices, and exact metadata accounting.
- 2026-07-13: added generation-checked dense dynamic property IDs, transform-linked clip metadata, exact validation/accounting, and checked flat translation support for C26.
