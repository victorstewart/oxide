# C16 WebGPU packed-geometry evidence

C16 changes only the generic WebGPU 2D vertex/index stream. Specialized Scene3D, ID-mask, texture, uniform, and presentation formats retain their existing ownership.

The performance hypothesis is that storing generic vertices as eight floats, copying them into a frame vector, and serializing them again before upload wastes CPU work and bandwidth. The affected stages are generic draw lowering, retained frame scratch, queue buffer upload, vertex fetch, and index fetch. The target workloads are immutable 10,000-glyph and 10,000-image quad lists plus a 70,002-vertex unindexed arbitrary solid mesh. Expected movement is 32 to 20 bytes per vertex, four to two bytes per u16-eligible index, deletion of duplicate frame byte vectors, and a correct u32 fallback for a mesh at or above 65,536 vertices. The correctness risk is byte-order/color drift, incorrect base-vertex rebasing, segment-boundary topology corruption, misaligned u16 writes, or silently dropping arbitrary large geometry.

The retained implementation uses derive-checked `repr(C)` POD vertices and typed index vectors. `bytemuck` supplies checked byte views for direct WebGPU upload; no handwritten unsafe alignment cast exists. Adjacent draw coalescing requires equal index format and base vertex. Capacity survives frame clearing, and the u16 stream receives only final tail padding required by WebGPU's four-byte queue-write size.

The locally ignored `raw/` directory retains the shared parent instrumentation, balanced fresh-process samples, plans, paired reports, parent/candidate app captures, package identities, environment controls, and hashes. Official `latest.*` promotion remains deferred to C62.
