# renderer-web `solid_color.rs`

## Intention and purpose

Own packed solid-color resolution and the narrow Canvas colored-quad classifier.

## Relation to the rest of the code

WebGPU calls `resolve_vertex_color`; Canvas2D calls `colored_quad`. The module depends only on renderer-api values.

## Entry points list

- `resolve_vertex_color(rgba, uniform) -> Color`: crate-private zero inheritance and `AABBGGRR` decode.
- `colored_quad(vertices, uniform) -> Option<...>`: crate-private Canvas admission and gradient endpoints.

## Logic narrative

Resolution returns the uniform for zero or decodes four bytes. Classification finds four exact rectangle corners, validates two complete triangles and duplicate colors, then accepts flat or one-axis opposing-edge colors.

## Preconditions and postconditions

Accepted geometry has six finite vertices, positive dimensions, a valid two-triangle rectangle, and a single flat or linear color field.

## Edge cases and failure modes

Every unsupported shape returns `None`; no approximate or general rasterization is attempted.

## Concurrency and memory behavior

Pure bounded stack operations; no allocation or shared state.

## Performance notes

Classification is O(6); byte decode is constant-time.

## Feature flags and cfgs

Used on wasm and retained on native for behavioral integration tests.

## Testing and benchmarks

Renderer-web tests cover flat, horizontal, vertical, inherited, malformed, and unsupported topologies.

## Examples

Equal left-corner colors and equal right-corner colors produce a horizontal gradient.

## Changelog

- 2026-07-12: added packed-color resolution and Canvas quad admission.
