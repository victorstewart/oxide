# oxide-renderer-metal tests `performance_contract_tests.rs`

## Intention and purpose

This test file protects renderer performance contracts that are easy to regress silently: build-time Metal shader compilation, no runtime Metal source compilation, frame-ring reuse without CPU/GPU waits, explicit readback-only blocking waits, direct GPU-duration attribution, and single-owner layer-cache rendering.

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
- `layer_cache_uses_one_plan_and_reports_single_ownership()`
  Freezes the generation-based plan, nested invalidation, same-size texture reuse, one materialization site, and public ownership counters.
- `layer_cache_clean_and_dirty_frames_have_single_body_owner()`
  Submits missing, clean, and dirty frames on macOS and proves clean frames only composite while refresh frames render one offscreen body, reuse same-size textures, and never inline the same body.
- `dirty_nested_child_refreshes_its_cached_parent_once()`
  Proves a dirty child invalidates and refreshes both retained nesting levels once, while the intervening clean frame skips both bodies.
- `metal_draw_cmd_debug_capture_names_are_frozen()`
  Freezes the private `DrawCmd` debug/capture tag names emitted by the Metal encode diagnostics before backend packet migrations.
- `renderer_initializes_default_pipelines_from_embedded_metallib_on_macos()`
  Constructs the default macOS Metal renderer, proving the embedded metallib and default PSO set initialize at runtime.
- `disabled_accounting_path_keeps_new_stats_zero()`
  Submits a real Metal frame with accounting disabled and verifies every new work and memory statistic remains zero.

## Logic narrative

Source-contract tests catch forbidden APIs and required guard strings before runtime. The layer source contract rejects the former independent hash/materialization path and requires child-to-parent invalidation propagation. The debug/capture-name freeze keeps Metal's command tags deterministic for future capture and A/B packet comparisons. The macOS runtime tests then exercise the actual Metal path: device resolution, command queue creation, embedded shader-library loading, default pipeline-state creation, and three consecutive cache states. A placeholder metallib, a missing shader entry point, or duplicate layer-body ownership cannot satisfy these tests.

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

The source tests allocate only small strings borrowed from `include_str!`. The initializer runtime test constructs and drops one renderer instance. The layer runtime test submits three bounded frames through the production command queue and reuses the same renderer/cache.

## Performance notes

The runtime test is an initialization guard, not a throughput benchmark. It protects the startup discipline required before frame-time A/B tests are meaningful: all default Metal pipelines must be resident before normal frame encoding.
The debug/capture-name freeze is measurement harness only. It changes no runtime path and does not claim a performance win.

## Feature flags and cfgs

- The runtime initializer test is guarded with `#[cfg(target_os = "macos")]`.
- The source-contract tests run on every target that compiles the crate tests.

## Testing and benchmarks

Run with:

```rust
cargo test --locked -j$(sysctl -n hw.ncpu) -p oxide-renderer-metal --test performance_contract_tests
```

Related local Metal evidence is produced by the perf-runner filtered GPU row `gpu.system.id_mask_compositor.current`; the slower legacy-upload audit row was retired after same-workload A/B proof.

The accounting schema test constructs the public stats value, freezes the previously omitted depth, bloom, ID-mask, Scene3D mesh, and layer fields, and source-checks saturating arithmetic plus separate retained texture/buffer identity sets whose capacity survives each scan. The disabled-path runtime test proves a real submission leaves the new fields zero. Actual nonzero resource ownership is verified by the filtered architecture report test.

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

- 2026-07-12: added source and real-Metal missing/clean/dirty layer-cache ownership, nested invalidation, and same-size texture-reuse coverage.
- 2026-07-12: added renderer memory-schema coverage for omitted resource families, overflow-safe accumulation, and cross-kind identity separation.
- 2026-07-12: added a real-frame guard for the complete disabled accounting path.
- 2026-06-22: froze Metal draw-command debug/capture names as measurement harness for architecture densification A/B work.
- 2026-06-01: added a macOS runtime initialization test proving the default Metal renderer starts from the embedded metallib and default PSO set.
