# oxide-renderer-metal tests `performance_contract_tests.rs`

## Intention and purpose

This test file protects renderer performance contracts that are easy to regress silently: build-time Metal shader compilation, no runtime Metal source compilation, frame-ring reuse without CPU/GPU waits, explicit readback-only blocking waits, and direct GPU-duration attribution.

## Relation to the rest of the code

- Upstream callers:
  - Cargo test invokes this integration test outside the renderer hot path.
- Downstream dependencies:
  - The tests inspect `crates/renderer-metal/src/lib.rs` and `crates/renderer-metal/build.rs`.
  - On macOS, the runtime test constructs `oxide_renderer_metal::MetalRenderer` through the public `new_default` initializer.

## Entry points list

- `renderer_loads_build_time_metallib_instead_of_runtime_source()`
  Confirms the renderer includes `default.metallib`, loads it with `new_library_with_data`, and does not call `new_library_with_source`.
- `build_script_fails_apple_metallib_generation_instead_of_placeholder_fallback()`
  Confirms Apple-target shader build failures stop the build instead of emitting a placeholder metallib.
- `per_frame_reuse_never_waits_for_gpu_completion()`
  Confirms frame-ring slot selection skips under backpressure instead of waiting for GPU completion.
- `blocking_gpu_waits_are_limited_to_explicit_readback_helpers()`
  Confirms `wait_until_completed` only appears in explicit readback helpers, not frame hot paths.
- `command_buffer_gpu_duration_is_enabled_on_macos_and_ios()`
  Confirms command-buffer GPU timestamp support is compiled for macOS and iOS.
- `completed_gpu_duration_is_attributed_to_frame_id()`
  Confirms completed GPU timing is associated with the frame id that produced it.
- `renderer_initializes_default_pipelines_from_embedded_metallib_on_macos()`
  Constructs the default macOS Metal renderer, proving the embedded metallib and default PSO set initialize at runtime.

## Logic narrative

Source-contract tests catch forbidden APIs and required guard strings before runtime. The macOS runtime initializer test then exercises the actual Metal path: device resolution, command queue creation, embedded shader-library loading, and default pipeline-state creation. A placeholder metallib or a missing shader entry point cannot satisfy this test because `MetalRenderer::new_default` must complete successfully.

## Preconditions and postconditions

- Preconditions:
  - The runtime initializer test is compiled only on macOS.
  - macOS test hosts must expose a real Metal device.
- Postconditions:
  - Passing tests mean the default renderer can start from build-time shader bytecode without runtime Metal source compilation.
  - Passing tests do not prove physical iPhone device performance; device baselines remain a separate contract.

## Edge cases and failure modes

- A missing Metal device fails the macOS runtime test because this repository uses macOS Metal as the local renderer proof path.
- Empty placeholder metallibs fail during renderer initialization.
- Runtime source-compilation regressions fail source inspection even if pipeline creation later succeeds.
- Blocking GPU waits outside explicit readback helpers fail source inspection.

## Concurrency and memory behavior

The source tests allocate only small strings borrowed from `include_str!`. The runtime test constructs and drops one renderer instance; no frame loop is started and no command buffer is submitted.

## Performance notes

The runtime test is an initialization guard, not a throughput benchmark. It protects the startup discipline required before frame-time A/B tests are meaningful: all default Metal pipelines must be resident before normal frame encoding.

## Feature flags and cfgs

- The runtime initializer test is guarded with `#[cfg(target_os = "macos")]`.
- The source-contract tests run on every target that compiles the crate tests.

## Testing and benchmarks

Run with:

```rust
cargo test --locked -j$(sysctl -n hw.ncpu) -p oxide-renderer-metal --test performance_contract_tests
```

Related local Metal A/B evidence is produced by the perf-runner filtered GPU rows, for example `gpu.system.id_mask_compositor.current` and `gpu.system.id_mask_compositor.legacy_upload`.

## Examples

```rust
#[cfg(target_os = "macos")]
fn initialize_renderer_for_contract_check() -> Result<(), oxide_renderer_metal::MetalInitError>
{
   let _renderer = oxide_renderer_metal::MetalRenderer::new_default()?;
   Ok(())
}
```

## Changelog

- 2026-06-01: added a macOS runtime initialization test proving the default Metal renderer starts from the embedded metallib and default PSO set.
