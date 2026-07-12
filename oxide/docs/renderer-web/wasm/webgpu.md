# renderer-web `wasm/webgpu.rs`

## Intention and purpose

Lower Oxide draw lists into persistent wgpu/WebGPU buffers, pipelines, passes, and surface submissions.

## Relation to the rest of the code

Consumes renderer-api values and `solid_color` decoding; embedded WGSL interpolates `GpuVertex::color`.

## Entry points list

- `BrowserRenderer::set_timestamp_readback_interval_for_benchmark`, `clear_completed_timestamp_samples`, and `drain_completed_timestamp_samples_into` control and collect bounded C00 GPU timestamp distributions without changing the normal eight-frame production sampling cadence.
- `BrowserRenderer::queue_completion_flag_for_benchmark` registers a benchmark-only completion fence used to serialize C01 primitive submissions before the next presented drawable.
- `BrowserRenderer::set_cpu_submit_timing_enabled_for_benchmark` and `last_cpu_submit_timing` expose bounded, opt-in CPU attribution for upload, surface, command encoding, queue submit, present, and readback bookkeeping; the normal renderer path retains only a disabled branch.
- `encode_solid`, `gpu_vertex`, and the three `append_*gpu_vertices` helpers implement this boundary.

## Logic narrative

Solid lowering passes `preserve_vertex_color = true` for local-indexed, rebased-indexed, and unindexed spans. Image and glyph paths pass false to retain existing tint semantics. `gpu_vertex` resolves packed color before upload; WGSL interpolates it.

Explicit benchmark capture lazily allocates a 4,096-entry completed-sample FIFO, samples every frame, clears stale completed samples, and drains results into host-owned reusable storage. Normal production timestamp sampling does not allocate or populate that history. When an active capture reaches the bound, the oldest completed sample is discarded; pending GPU readbacks retain their existing completion-safe slot ownership.

## Preconditions and postconditions

Indexed paths validate or rebase indices first. Existing stride, pipeline, bind groups, draw packets, and shader locations remain unchanged.

## Edge cases and failure modes

Invalid spans or indices clear scratch output and emit no draw. Packed zero exactly inherits the uniform.

## Concurrency and memory behavior

Frame scratch vectors are reused; the change adds no resource or synchronization work after warmup.

## Performance notes

Draw count and upload size are unchanged; solid vertices add bounded bit decoding.

## Feature flags and cfgs

Compiled only for `wasm32` with the existing WebGPU and WGSL features.

## Testing and benchmarks

Native contract tests exercise decoding/source paths; wasm `--lib` compilation verifies the implementation.

## Examples

Packed `0xFFFF_0000` uploads as opaque blue; packed zero uploads the draw uniform.

## Changelog

- 2026-07-12: exposed a benchmark-only queue completion flag for the opt-in C01 one-submit-per-RAF primitive matrix without changing normal submission behavior.
- 2026-07-12: added bounded per-frame timestamp history and caller-owned draining for C00 GPU distributions.
- 2026-07-12: added opt-in high-resolution WebGPU submit-stage CPU timing for the C00 one-submit-per-RAF harness.
- 2026-07-12: preserved packed colors on every solid lowering topology.
