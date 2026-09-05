# renderer-metal shader `text.metal`

## Intention and purpose

Render A8 and SDF glyph geometry plus image meshes. SDF coverage derives its transition width from screen-space fragment derivatives so the visible edge remains bounded across device scales. C27 lets prepared image meshes share the affine vertex path and apply frame opacity without rewriting immutable geometry.

## Relation to the rest of the code

`renderer-metal::prepared` binds persistent glyph buffers, atlas textures, and one frame-dynamic `PreparedInstance`; the flat renderer continues to use `v_text`, `f_text`, and `f_text_sdf`.

## Entry points list

- `v_prepared_text(...) -> TextVSOut`: applies affine transform and viewport mapping to local glyph vertices.
- `f_prepared_text(...) -> float4`: samples A8 coverage and multiplies run alpha by dynamic opacity.
- `f_prepared_text_sdf(...) -> float4`: applies the SDF edge function and dynamic opacity.
- `f_prepared_image_mesh(...) -> float4`: samples an RGBA image mesh and multiplies the immutable mesh alpha by dynamic instance opacity.
- Existing text and image-mesh entry points preserve flat behavior.

## Logic narrative

The prepared vertex path transforms only position; UVs remain immutable atlas coordinates. A8 fragments sample coverage directly. Every flat and prepared SDF fragment routes its sampled distance through one `fwidth`-based edge function centered at `0.5`, with a small lower bound for degenerate derivatives, then multiplies the final alpha by the property record.

## Preconditions and postconditions

The CPU and shader agree on the 48-byte prepared record. Glyph resource dependencies must match the renderer's current atlas generation before encoding.

## Edge cases and failure modes

Missing or stale atlases prevent prepared admission. Unsupported snapshot structure uses the checked flat adapter. Degenerate derivatives use the bounded minimum transition width instead of producing a zero-width step.

## Concurrency and memory behavior

All shader inputs are read-only. No writable shared state or allocation exists.

## Performance notes

Clean glyph and image-mesh replay retain vertex/index/color buffers. Small retained damage selects the prepared command without scanning those vertex buffers. SDF fragments add derivative evaluation in exchange for scale-stable edge coverage; the physical-device comparison must retain identical glyph geometry and visible output.

## Feature flags and cfgs

The build script compiles this source into the default metallib.

## Testing and benchmarks

Mixed prepared snapshots include A8/SDF glyphs, transform and opacity changes, exact flat parity, cache hits, zero clean upload assertions, and a 1x/3x bound on partial-edge pixel width.

## Examples

Changing an opacity property from `0.75` to `0.5` reuses the same atlas and glyph buffers.

## Changelog
- 2026-08-07: replaced fixed SDF smoothing bands with one screen-space derivative edge function across flat and prepared glyph paths.
- 2026-07-13: added the C27 prepared image-mesh fragment path with dynamic opacity.

- 2026-07-13: added prepared affine glyph and A8/SDF opacity paths.
