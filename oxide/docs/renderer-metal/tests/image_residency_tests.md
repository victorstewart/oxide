# oxide-renderer-metal tests `image_residency_tests.rs`

## Intention and purpose

These physical-Metal integration tests freeze C59's image storage, mip-quality, update, release, memory-pressure, and renderer-recreation contracts.

## Relation to the rest of the code

- The tests exercise `MetalRenderer`'s public dynamic and immutable RGBA8 image entry points.
- Benchmark-only policy selection isolates Shared, Private, and mipmapped storage without changing draw content.
- Exact readback verifies that storage and mip policy never substitute reduced visual work for lower timing.

## Entry points list

- `immutable_policy_keeps_nonminified_images_shared_and_allows_explicit_private_staging()` freezes Shared production residency for non-minified and repeatedly minified images plus current/cumulative counters and the explicit Private evidence control.
- `dynamic_rgba_images_remain_shared_at_large_sizes()` keeps update-heavy resources on direct Shared storage regardless of size.
- `mipmapped_immutable_upload_and_partial_update_match_dynamic_pixels()` requires both Shared-mip and Private-mip partial updates to preserve exact level-zero pixels and match a freshly rebuilt chain when minified.
- `mipmapped_minification_reduces_checkerboard_aliasing()` compares identical Shared/Private mip output with the non-mip control and requires a material variance reduction.
- `immutable_images_survive_cache_pressure_and_recreate_with_a_new_renderer()` proves cache purges retain author-owned images and source replay reproduces the same pixels after renderer replacement.

## Logic narrative

Every case constructs a real offscreen Metal renderer and uploads deterministic BGRA8 source bytes. Full-size draws isolate storage equivalence; a 256-to-31-pixel checkerboard isolates minification quality. The update case changes a non-aligned 13 by 9 region, compares exact full-size pixels, then compares 17-square Shared/Private output against a fresh mip chain so row bytes, destination origin, and lower-level regeneration are all observable. The recreation case invokes every production cache-pressure purge that is safe to run independently of app-owned resources, renders once, drops the renderer, reuploads the same source bytes into a fresh renderer, and compares readback.

Call flow:

- deterministic bytes -> dynamic/immutable upload -> optional staging and mip generation
- image draw -> Metal submission -> explicit test readback
- cache purge or renderer replacement -> source replay -> exact pixel comparison

## Preconditions and postconditions

- A usable Metal device must be available.
- Dynamic and non-minified immutable images remain Shared and issue no upload command buffer.
- A released image contributes zero current residency even though cumulative upload counters remain observable.
- Renderer-owned cache purges do not invalidate app-owned image handles.
- Renderer replacement requires source replay and produces equivalent pixels.

## Edge cases and failure modes

- The tests cover small and large non-minified images, non-aligned partial updates, complete mip chains, extreme checkerboard minification, release, memory pressure, and recreation.
- A stale mip chain fails the exact Shared/Private comparison or the variance bound.
- Incorrect storage choice, staging accounting, or resource release fails exact counters.

## Concurrency and memory behavior

Upload and mip-generation command buffers use the renderer's existing serial queue and do not synchronously wait in production. Test readback supplies the completion boundary. Private upload staging remains alive through Metal command ownership, while current residency accounts only the retained sampled texture.

## Performance notes

The tests freeze structural work, not noisy timing. C59 timing lives in the perf-runner rows and the physical-device experiment report.

## Feature flags and cfgs

The test target requires `snapshot-tests` and is compiled only on macOS, where a real Metal device is available. The identical implementation is additionally exercised by the installed physical-iPhone perf harness.

## Testing and benchmarks

Run `cargo test --locked -p oxide-renderer-metal --features snapshot-tests --test image_residency_tests`.

## Examples

Use `image_create_rgba8_immutable(width, height, bytes, row_bytes, true)` only when the source is stable and will be sampled repeatedly below its source resolution. Keep atlases, video, camera, and frequently updated images on `image_create_rgba8`.

## Changelog

- 2026-07-15: added C59 storage-policy, staging, mip-quality, partial-update, release, cache-pressure, and renderer-recreation coverage.
