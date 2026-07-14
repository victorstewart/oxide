# renderer-metal shader `text.metal`

## Intention and purpose

Render A8 and SDF glyph geometry plus image meshes. C27 lets prepared image meshes share the affine vertex path and apply frame opacity without rewriting immutable geometry.

## Relation to the rest of the code

`renderer-metal::prepared` binds persistent glyph buffers, atlas textures, and one frame-dynamic `PreparedInstance`; the flat renderer continues to use `v_text`, `f_text`, and `f_text_sdf`.

## Entry points list

- `v_prepared_text(...) -> TextVSOut`: applies affine transform and viewport mapping to local glyph vertices.
- `f_prepared_text(...) -> float4`: samples A8 coverage and multiplies run alpha by dynamic opacity.
- `f_prepared_text_sdf(...) -> float4`: applies the SDF edge function and dynamic opacity.
- `f_prepared_image_mesh(...) -> float4`: samples an RGBA image mesh and multiplies the immutable mesh alpha by dynamic instance opacity.
- Existing text and image-mesh entry points preserve flat behavior.

## Logic narrative

The prepared vertex path transforms only position; UVs remain immutable atlas coordinates. A8 and SDF fragments retain their established coverage calculations and multiply the final alpha by the property record.

## Preconditions and postconditions

The CPU and shader agree on the 48-byte prepared record. Glyph resource dependencies must match the renderer's current atlas generation before encoding.

## Edge cases and failure modes

Missing or stale atlases prevent prepared admission. Unsupported snapshot structure uses the checked flat adapter.

## Concurrency and memory behavior

All shader inputs are read-only. No writable shared state or allocation exists.

## Performance notes

Clean glyph and image-mesh replay retain vertex/index/color buffers. Small retained damage selects the prepared command without scanning those vertex buffers.

## Feature flags and cfgs

The build script compiles this source into the default metallib.

## Testing and benchmarks

Mixed prepared snapshots include A8 glyphs, transform and opacity changes, exact flat parity, cache hits, and zero clean upload assertions.

## Examples

Changing an opacity property from `0.75` to `0.5` reuses the same atlas and glyph buffers.

## Changelog
- 2026-07-13: added the C27 prepared image-mesh fragment path with dynamic opacity.

- 2026-07-13: added prepared affine glyph and A8/SDF opacity paths.
