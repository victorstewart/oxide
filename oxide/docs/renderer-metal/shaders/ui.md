# renderer-metal shader `ui.metal`

## Intention and purpose

Render analytic 2D UI rectangles, images, effects, spinners, and layer composites. C24 adds prepared rectangle/image vertices and opacity fragments that keep immutable parameter buffers reusable.

## Relation to the rest of the code

`renderer-metal::lib` owns legacy pipelines; `renderer-metal::prepared` binds persistent parameter and image-table buffers plus one frame-dynamic `PreparedInstance` record.

## Entry points list

- `v_prepared_inst_rect(...) -> UIVSOut`: transforms a persistent local rectangle into the current viewport.
- `f_prepared_rrect(...) -> float4`: evaluates the local rounded-rectangle SDF and applies dynamic opacity.
- `f_prepared_image(...) -> float4`: samples the prepared argument-buffer image table and applies dynamic opacity.
- `f_prepared_image_single(...) -> float4`: performs equivalent sampling on devices without the image argument-buffer path.
- Existing `v_inst_rect`, primitive, image, layer, and effect functions retain flat draw-list behavior.

## Logic narrative

Prepared rectangle and image records remain in local coordinates. The vertex function reads each record's first `float4`, applies the affine matrix and translation, and emits clip coordinates. Translation-only instances carry world position plus a flat world-space rectangle origin so fragment subtraction reproduces the established flat path's rounding at fractional raster edges. General affine instances retain local position and origin for coverage and UV calculation. Dynamic opacity is bound separately only when it differs from one; opaque instances reuse the existing fragments.

## Preconditions and postconditions

The CPU parameter records retain their three-`float4` stride, and `PreparedInstance` retains its 48-byte layout. Viewport dimensions are positive after clamping.

## Edge cases and failure modes

Analytic fragments discard outside coverage. Resource-generation validation occurs before the shader can receive a stale image table.

## Concurrency and memory behavior

Shader invocations are read-only and allocation-free.

## Performance notes

Persistent parameters remove clean-frame geometry uploads. Opaque property instances avoid the extra fragment constant binding and shader variant.

## Feature flags and cfgs

The build script compiles this source into the default metallib.

## Testing and benchmarks

Prepared mixed and fractional opaque snapshot tests compare exact readback against the flat path under Metal validation. C24 architecture rows report uploads, binds, draws, and GPU duration.

## Examples

A translation-only property changes `PreparedInstance.translation` while leaving the cached RRect/image parameter buffer untouched.

## Changelog

- 2026-07-13: added prepared affine rectangle/image vertices, exact fractional-translation fragment coordinates, and dynamic-opacity fragments.
