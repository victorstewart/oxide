# `MetalTextProbe.swift`

## Intention and purpose

This offscreen helper isolates production Metal title stages after CoreText/A8 rasterization.

## Relation to the rest of the code

It consumes the production atlas and exact glyph-instance manifests captured by Rust. Its vertex and fragment functions mirror production glyph lowering, texture sampling, source-alpha blend factors, and `BGRA8Unorm_sRGB` target behavior without importing or modifying the production renderer.

## Entry points

- Top-level `execute()`: validates inputs, builds the bounded Metal resources and pipelines, runs stage probes, persists artifacts, and writes `metal-report.json`.

## Logic narrative

The helper uploads the exact 1024² bytes as `R8Unorm`. It renders the same instances to `R32Float` once with point sampling and once with production linear sampling, then CPU-composites each coverage plane. A third pass runs the production linear sampler and glyph fragment through the exact fixed-function source-alpha factors into `BGRA8Unorm_sRGB`. All passes use the production viewport mapping and triangle-strip instance topology.

The frozen result is 5/35 Oxide matches before the final pass and 35/35 after it. Point and linear coverage are identical over both full canvases. Earliest attribution is 5 scalar replay, 0 atlas/geometry, 0 linear sampler, 30 fixed-blend/sRGB target, and 0 unexplained.

## Preconditions and postconditions

The system default Metal device must support `R8Unorm` sampling, `R32Float` rendering, and `BGRA8Unorm_sRGB` rendering. Successful completion writes two coverage planes, two CPU RGB composites, one complete Metal RGB output per case, and a sorted-key report.

## Edge cases and failure modes

Missing devices, queues, shaders, pipelines, textures, buffers, encoders, invalid manifests, atlas/hash mismatch, failed command buffers, or file errors return a nonzero status.

## Concurrency and memory behavior

The helper is sequential and uses one command queue. `waitUntilCompleted` is confined to this command-line diagnostic and cannot enter a product frame loop.

## Performance notes

No timing claim is made. Persistent production-object policy does not apply to this bounded one-shot attribution process.

## Feature flags and cfgs

It requires macOS Foundation, CryptoKit, and Metal.

## Testing and benchmarks

Static tests freeze formats, filtering, blend factors, shader stages, and the absence of app/window ownership. The integration test validates all output hashes and attribution counts.

## Examples

Use the crate CLI rather than invoking this helper directly.

## Changelog

- 2026-07-19: added exact production atlas sampling and fixed-blend sRGB target attribution.
