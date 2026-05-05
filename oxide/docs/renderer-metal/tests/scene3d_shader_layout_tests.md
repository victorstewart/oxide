# oxide-renderer-metal::tests::scene3d_shader_layout_tests

## Intention and purpose

These tests lock the CPU-to-Metal ABI for the `Scene3dMaterial` payload sent with `set_fragment_bytes` and guard the Scene3D bloom API path. The shader must keep its padding field packed so the following `float4 params` begins at the same byte offset as the Rust `#[repr(C)]` payload, and `Pass3d::bloom` must reach the offscreen bloom encoder rather than being collapsed into an unblurred main-pass draw.

## Relation to the rest of the code

The renderer builds a `Scene3dGpuMaterial` in `oxide_renderer_metal::MetalRenderer::encode_scene3d_instance()` and binds it to the Scene3D fragment shader. The shader reads `params` for neighborhood fill radius, center, edge darkening, and emissive intensity, so a layout mismatch corrupts visible 3D material output. The bloom guard tracks `oxide_renderer_metal::MetalRenderer::encode_scene3d()` because the public `Bloom3d` payload is only meaningful when it calls `encode_scene3d_bloom()`.

## Entry points list

- `scene3d_material_shader_matches_cpu_set_bytes_layout()` verifies the 48-byte CPU layout and checks the shader source uses `packed_float3 _pad;`.
- `scene3d_bloom_payload_reaches_bloom_encoder()` verifies `encode_scene3d()` calls the bloom encoder and does not collapse bloom layers into one main-pass strength.

## Logic narrative

The layout test mirrors the public ABI shape of the renderer's private Scene3D material payload, verifies the important offsets, then checks the shader declaration. `packed_float3` is required because a normal Metal `float3` can carry 16-byte alignment before the following `float4`, while the Rust payload intentionally uses three packed `f32` padding slots after the `u32` material id.

The bloom test reads the renderer source and checks the intended handoff from `Pass3d::bloom` into `encode_scene3d_bloom()`. This is a narrow regression guard because the defect was a committed wiring mismatch between an exposed payload and a dormant helper.

## Preconditions and postconditions

The tests assume Scene3D material bytes are still sent as one contiguous `set_fragment_bytes` payload and the bloom helper remains in the Metal renderer implementation. Passing means the shader source preserves the packed padding marker needed for `params` offset parity and bloom payloads are routed to the blur/composite pass.

## Edge cases and failure modes

If the shader padding regresses to a plain `float3`, the test fails before a device run can silently render neighborhood fill or emissive materials with corrupted parameters.

If bloom routing regresses to a direct main-pass emissive draw, the test fails before benchmark or snapshot output can imply that `sigma_px` and `downsample_divisor` were exercised.

## Concurrency and memory behavior

The tests are single-threaded and allocation-free except for normal test harness setup. They read source files at compile time with `include_str!`.

## Performance notes

The tests have no renderer hot-path cost. The production fixes avoid runtime conversion, larger material payloads, or per-frame allocation changes beyond the already existing bloom render passes.

## Feature flags and cfgs

No feature flags or target cfgs affect these tests.

## Testing and benchmarks

Run with `cargo test --locked -p oxide-renderer-metal --test scene3d_shader_layout_tests`.

## Examples

```rust
let shader = include_str!("../shaders/scene3d.metal");
assert!(shader.contains("packed_float3 _pad;"));
```

## Changelog

- 2026-04-30: Added ABI regression coverage for Scene3D material shader padding.
- 2026-04-30: Added regression coverage for routing `Pass3d::bloom` through the offscreen bloom encoder.
