# renderer-web `solid_color.rs`

## Intention and purpose

Own the narrow Canvas colored-quad classifier.

## Relation to the rest of the code

Canvas2D calls `colored_quad`. WebGPU now preserves packed colors in its POD geometry stream. The module depends only on renderer-api values.

## Entry points list

- `colored_quad(vertices, uniform) -> Option<...>`: crate-private Canvas admission and gradient endpoints.

## Logic narrative

Classification finds four exact rectangle corners, validates two complete triangles and duplicate colors, resolves zero to the packed uniform, then accepts flat or one-axis opposing-edge colors.

## Preconditions and postconditions

Accepted geometry has six finite vertices, positive dimensions, a valid two-triangle rectangle, and a single flat or linear color field.

## Edge cases and failure modes

Every unsupported shape returns `None`; no approximate or general rasterization is attempted.

## Concurrency and memory behavior

Pure bounded stack operations; no allocation or shared state.

## Performance notes

Classification is O(6).

## Feature flags and cfgs

Used on wasm and retained on native for behavioral integration tests.

## Testing and benchmarks

Renderer-web tests cover flat, horizontal, vertical, inherited, malformed, and unsupported topologies. Packed WebGPU color bytes are covered in `wasm/packed_geometry.rs`.

## Examples

Equal left-corner colors and equal right-corner colors produce a horizontal gradient.

## Changelog

- 2026-07-12: moved WebGPU color ownership into the compact POD geometry stream and removed its obsolete float-decoding helper.
- 2026-07-12: added packed-color resolution and Canvas quad admission.
