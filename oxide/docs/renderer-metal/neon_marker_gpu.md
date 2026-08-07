# oxide-renderer-metal::neon_marker_gpu

## Intention and purpose

`neon_marker_gpu` encodes a bounded array of analytic neon markers in one instanced Metal draw without expanding marker geometry on the CPU.

## Relation to the rest of the code

`MetalRenderer::encode_neon_markers` consumes renderer-api marker passes, writes compact 72-byte records into the active completion-protected uniform ring, and uses the prebuilt `v_neon_marker`/`f_neon_marker` pipeline.

## Entry points list

- `build_pso(...)` constructs the persistent marker pipeline.
- `MetalRenderer::encode_neon_markers(...)` validates, uploads, and draws one bounded marker pass.

## Logic narrative

The encoder clamps radii, widths, sigmas, alphas, and colors; copies the fixed-layout instance records once; and issues one six-vertex instanced draw. Marker fragments emit straight RGB and coverage alpha, so the pipeline uses straight-alpha source-over and preserves an opaque destination's alpha.

## Preconditions and postconditions

The renderer must be inside a single-sample frame. Empty passes do nothing; accepted passes retain existing color contents, draw once, and update renderer work counters.

## Edge cases and failure modes

Multisample renderers return `Unsupported`. Missing target storage returns `InvalidOperation`. Input beyond `NEON_MARKER_MAX_INSTANCES` is deterministically clamped by the pass contract.

## Concurrency and memory behavior

The renderer owns all mutable state. Warm passes reuse the selected frame ring and create no pipeline or texture resources.

## Performance notes

One aligned bulk copy and one instanced draw replace per-marker geometry and draw submission. Counters expose uploaded bytes, draw count, instance count, and ring growth.

## Feature flags and cfgs

The module is compiled with the Metal backend; snapshot readback coverage requires `snapshot-tests`.

## Testing and benchmarks

Renderer snapshots cover distinctive instance colors, nonoverlapping ring slices, exact opaque-target alpha, one draw, and warm ring reuse.

## Changelog

- 2026-08-07: documented straight-alpha source-over and opaque-target alpha preservation.
