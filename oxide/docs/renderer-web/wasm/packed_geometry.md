# renderer-web `wasm/packed_geometry.rs`

## Intention and purpose

Own the compact, directly uploadable WebGPU geometry stream used by generic 2D draw lowering.

## Relation to the rest of the code

`wasm/webgpu.rs` resolves renderer-api vertices into `PackedVertex` values, appends them to `PackedGeometry`, uploads the three retained vectors directly through checked `bytemuck` slice views, and records each returned `PackedIndexRange` in a GPU draw packet. The module has no renderer-api or wgpu dependency.

## Entry points list

- `PackedVertex::new(x, y, u, v, rgba) -> PackedVertex`: constructs one 20-byte position/UV/Unorm8 color vertex.
- `PackedGeometry::clear(&mut self)`: clears lengths while retaining all three stream capacities.
- `PackedGeometry::append_validated(&mut self, vertices, indices) -> Option<PackedIndexRange>`: appends caller-validated arbitrary geometry using a u16 segment when its vertex span is below 65,536 and u32 otherwise.
- `PackedGeometry::append_quad(&mut self, vertices) -> Option<PackedIndexRange>`: appends the canonical two-triangle quad without rebuilding its index pattern.
- `PackedGeometry::align_uploads(&mut self)`: pads only the tail of the u16 stream to WebGPU's four-byte write size.
- `PackedGeometry::byte_len(&self) -> usize`: returns exact populated upload bytes.
- `PackedGeometry::capacity_bytes(&self) -> usize`: returns exact retained CPU stream capacity.

## Logic narrative

Vertices use a C-compatible POD layout of four `f32` fields followed by packed `AABBGGRR`; `Unorm8x4` converts those four bytes to shader floats. Every append advances one global vertex cursor. Small draws share a u16 segment until the next append would make its span reach 65,536 vertices, at which point a new segment begins and the draw packet carries that segment as `base_vertex`. A single larger arbitrary mesh uses local u32 indices and its own global base. Following small draws restart u16 segmentation. The final typed vectors are passed directly to WebGPU; there is no frame-level vertex or index byte reserialization.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

`append_validated` requires every index to be local to the supplied vertex slice; debug builds assert that contract. Returned ranges always address the selected typed index stream, and their base vertex is representable by WebGPU's signed base-vertex argument. `PackedVertex` is `Pod` and `Zeroable`; derive-time layout checks plus the exact 20-byte test make `bytemuck` the only byte-view boundary. The module contains no handwritten unsafe code.

## Edge cases and failure modes

Empty geometry, count overflow, or an unrepresentable base returns `None`. The checked test-only entry rejects invalid indices without mutating any stream. A 65,536-vertex draw takes the u32 fallback, and small draws after it resume a fresh u16 segment. Odd u16 index counts receive one ignored tail element solely for upload alignment.

## Concurrency and memory behavior

The stream is frame-local and single-threaded. `clear` retains peak capacity, so normal warm frames allocate neither vertex nor index storage. Typed POD vectors avoid duplicate byte vectors and unsafe alignment conversions.

## Performance notes

Generic vertices occupy 20 bytes instead of 32. u16-eligible geometry uses two index bytes instead of four. Canonical quads append four POD values and six u16 indices directly; large meshes keep a correct u32 path.

## Feature flags and cfgs

The module is compiled on wasm for production and on native targets for unit coverage.

## Testing and benchmarks

Unit tests freeze exact field bytes and color order, u16 segment rollover/rebasing, canonical triangle order, u32 fallback, post-fallback u16 restart, upload padding, and invalid-index nonmutation. The C16 Chrome adapter measures 10,000 glyph quads, 10,000 image quads, and a 70,002-vertex arbitrary solid mesh.

## Examples

`PackedVertex::new(1.0, 2.0, 0.0, 1.0, 0xFF00_00FF)` stores opaque red in the final four bytes and is consumed as a normalized shader color.

## Changelog

- 2026-07-12: added the 20-byte POD vertex stream, segmented u16 indices, u32 large-mesh fallback, retained capacity, and direct checked uploads.
