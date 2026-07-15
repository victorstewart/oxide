# oxide-renderer-metal shader `effects.metal`

## Intention and purpose

This shader unit implements full-screen effect primitives: separable Gaussian blur, downsample, upsample, backdrop composite, and visual-effect composite.

## Relation to the rest of the code

`renderer-metal::MetalRenderer` selects effect target resolution, binds source textures and blur parameters, and encodes these functions inside the Metal effect render graph. The output feeds backdrop, visual-effect, camera, and Scene3D bloom composition.

## Entry points list

- `v_fullscreen(uint) -> VSOut`: emits one full-screen triangle.
- `f_blur(...) -> float4`: executes the exact per-tap Gaussian fallback.
- `f_blur_paired(...) -> float4`: executes a normalized paired-bilinear wide kernel.
- `f_downsample(...) -> float4`: samples the next lower-resolution effect level.
- `f_upsample(...) -> float4`: reconstructs the next higher-resolution level.
- `f_backdrop(...) -> float4`: composites a blurred backdrop region.
- `f_visual_effect(...) -> float4`: applies the declared visual-effect material.

## Logic narrative

Rust always binds direction, sigma, and radius. Subthreshold and noncanonical inputs select the persistent exact pipeline, whose fragment function evaluates one Gaussian weight per positive tap. Canonical inputs select a separate persistent pipeline and bind one prepacked direction/header/record block. Each `(offset, weight)` record combines two adjacent positive taps into one linear-filtered fetch and mirrors it around the center, preserving the discrete kernel weights while reducing samples. Downsample and upsample passes remain explicit render-graph stages, so the optimization changes neither pass topology nor target lifetime.

## Preconditions and postconditions

The source texture must use the renderer's linear clamp sampler. Paired records must be normalized, ordered from the center outward, and correspond to the bound radius. Both pipelines return the same premultiplied color contract expected by the composite shaders.

## Edge cases and failure modes

Rust selects the exact pipeline for pass sigma below 2, non-finite, non-bucket, or radius-mismatched inputs. The exact shader clamps radii to 2–192. The paired entry point is never encoded without a nonempty validated record set.

## Concurrency and memory behavior

The shader has no writable storage or inter-thread synchronization. Exact parameters occupy 16 bytes. A paired pass binds one 24-byte direction/header prefix plus at most 768 bytes of offset/weight records; horizontal and vertical blocks are cached after first use.

## Performance notes

For radius `r`, exact blur performs `1 + 2r` texture samples and `r` exponential evaluations per fragment. Paired blur performs `1 + 2 * ceil(r / 2)` samples and no runtime exponentials. Both remain separable two-pass filters over the existing quarter/eighth-resolution targets.

## Feature flags and cfgs

The metallib build compiles the same shader for supported Apple targets. Snapshot-feature selection is implemented in Rust and does not change shader bytecode.

## Testing and benchmarks

`metal_sequence_golden_tests` compares the paired output with exact-render controls across sigma 2, 8, 16, 32, and 64. `gpu.architecture.effects.blur_sigma_*` reports frame, encode, direct GPU, sample, ALU-proxy, pass, and memory metrics.

## Examples

For pass sigma 8 and radius 24, the exact path uses 49 samples; the paired path uses 25 samples with the same two render passes.

## Changelog

- 2026-07-14: added normalized paired-bilinear blur kernels in a dedicated persistent pipeline while retaining the exact Gaussian pipeline.
