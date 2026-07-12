# renderer-web `wasm/webgpu.rs`

## Intention and purpose

Lower Oxide draw lists into persistent wgpu/WebGPU buffers, pipelines, passes, and surface submissions.

## Relation to the rest of the code

Consumes renderer-api values and `solid_color` decoding; embedded WGSL interpolates `GpuVertex::color`.

## Entry points list

- Existing `BrowserRenderer` and `WebGpuRenderer` public methods are unchanged.
- `encode_solid`, `gpu_vertex`, and the three `append_*gpu_vertices` helpers implement this boundary.

## Logic narrative

Solid lowering passes `preserve_vertex_color = true` for local-indexed, rebased-indexed, and unindexed spans. Image and glyph paths pass false to retain existing tint semantics. `gpu_vertex` resolves packed color before upload; WGSL interpolates it.

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

- 2026-07-12: preserved packed colors on every solid lowering topology.
