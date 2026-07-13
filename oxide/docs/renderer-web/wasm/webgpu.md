# renderer-web `wasm/webgpu.rs`

## Intention and purpose

Lower Oxide draw lists into persistent wgpu/WebGPU buffers, pipelines, passes, and surface submissions.

## Relation to the rest of the code

Consumes renderer-api values and lowers generic 2D geometry through `packed_geometry`; embedded WGSL interpolates the normalized packed vertex color.

## Entry points list

- `BrowserRenderer::set_timestamp_readback_interval_for_benchmark`, `clear_completed_timestamp_samples`, and `drain_completed_timestamp_samples_into` control and collect bounded C00 GPU timestamp distributions without changing the normal eight-frame production sampling cadence.
- `BrowserRenderer::queue_completion_flag_for_benchmark` registers a benchmark-only completion fence used to serialize C01 primitive submissions before the next presented drawable.
- `BrowserRenderer::set_cpu_submit_timing_enabled_for_benchmark` and `last_cpu_submit_timing` expose bounded, opt-in CPU attribution for upload, surface, command encoding, queue submit, present, and readback bookkeeping; the normal renderer path retains only a disabled branch.
- `encode_solid`, `gpu_vertex`, and the three `append_*gpu_vertices` helpers implement this boundary.

## Logic narrative

Solid lowering passes `preserve_vertex_color = true` for local-indexed, rebased-indexed, and unindexed spans. Image and glyph paths pass false to retain existing tint semantics. Nonzero API `AABBGGRR` bits are copied unchanged; zero inherits one quantized uniform color. Generic frame geometry is retained as 20-byte POD vertices plus segmented u16 and fallback u32 indices, exposed to `Queue::write_buffer` by checked `bytemuck` slice views without a second serialization vector. Each draw packet records its index format and base vertex, so adjacent compatible ranges coalesce only inside the same segment.

Explicit benchmark capture lazily allocates a 4,096-entry completed-sample FIFO, samples every frame, clears stale completed samples, and drains results into host-owned reusable storage. Normal production timestamp sampling does not allocate or populate that history. When an active capture reaches the bound, the oldest completed sample is discarded; pending GPU readbacks retain their existing completion-safe slot ownership.

## Preconditions and postconditions

Indexed paths validate or rebase indices first. Generic shader locations remain unchanged; the color location is now `Unorm8x4` at byte 16 with a 20-byte stride. u16 writes are four-byte aligned at the stream tail, and large geometry retains a u32 fallback.

## Edge cases and failure modes

Invalid spans or indices clear scratch output and emit no draw. Packed zero exactly inherits the uniform.

## Concurrency and memory behavior

Frame scratch vectors and typed packed streams retain capacity across frames. The change adds no resource or synchronization work after warmup and contains no handwritten unsafe cast.

## Performance notes

Draw count is unchanged. Generic vertex uploads fall from 32 to 20 bytes each, u16-eligible index uploads fall from four to two bytes each, and frame-level vertex/index reserialization is deleted. The C16 browser workload separately measures 10,000 glyph quads, 10,000 image quads, and a 70,002-vertex u32-fallback solid mesh while retaining direct GPU timestamp and visual evidence.

## Feature flags and cfgs

Compiled only for `wasm32` with the existing WebGPU and WGSL features.

## Testing and benchmarks

Native contract tests exercise decoding/source paths; wasm `--lib` compilation verifies the implementation.

## Examples

Packed `0xFFFF_0000` uploads as opaque blue; packed zero uploads the draw uniform.

## Changelog

- 2026-07-12: replaced generic frame reserialization with directly uploaded 20-byte POD vertices, segmented u16 indices, and a correct u32 large-mesh fallback.
- 2026-07-12: exposed a benchmark-only queue completion flag for the opt-in C01 one-submit-per-RAF primitive matrix without changing normal submission behavior.
- 2026-07-12: added bounded per-frame timestamp history and caller-owned draining for C00 GPU distributions.
- 2026-07-12: added opt-in high-resolution WebGPU submit-stage CPU timing for the C00 one-submit-per-RAF harness.
- 2026-07-12: preserved packed colors on every solid lowering topology.
