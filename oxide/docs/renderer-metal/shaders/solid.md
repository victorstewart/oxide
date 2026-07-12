# renderer-metal shader `solid.metal`

## Intention and purpose

Transform and color solid geometry through the existing Metal solid pipeline.

## Relation to the rest of the code

`renderer-metal/src/lib.rs` binds API vertices at buffer 0 and `SolidUniform` at vertex buffer 1.

## Entry points list

- `v_solid(SolidVSIn, SolidUniform) -> SolidVSOut`: resolves zero packed color and emits interpolants.
- `f_solid(SolidVSOut) -> float4`: returns the interpolated final color.

## Logic narrative

The normalized `uchar4` attribute arrives as RGBA floats. Exact zero selects the draw uniform; every other value passes through and rasterization interpolates it.

## Preconditions and postconditions

The CPU descriptor keeps `UChar4Normalized` at byte offset 16 and the uniform remains a `float4` at buffer 1.

## Edge cases and failure modes

Only exact packed zero inherits. Partially zero colors are explicit colors.

## Concurrency and memory behavior

Shader invocations have no writable shared state or dynamic allocation.

## Performance notes

One vertex-stage select replaces the fragment uniform read; no extra pass, texture, or draw is introduced.

## Feature flags and cfgs

The build script compiles this source into the Apple metallib.

## Testing and benchmarks

Source-contract and macOS readback tests cover binding, endpoints, interpolation, and zero inheritance.

## Examples

Packed `0xFF00_00FF` produces opaque red; packed zero uses `SolidUniform::color`.

## Changelog

- 2026-07-12: added resolved per-vertex color interpolation.
