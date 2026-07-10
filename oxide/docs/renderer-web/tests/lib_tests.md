# renderer-web::tests::lib_tests

## Intention and purpose

These tests verify the renderer-web behavior that can be exercised on native test targets without a browser DOM. They exist to keep shared conversion logic, native unsupported stubs, WebGPU source contracts, and report-counter wiring deterministic.

## Relation to the rest of the code

The tests import `oxide_renderer_web` public helpers, `WebRenderer`, and source files for the wasm backend and web host. Native CI can run them even though the real renderer implementation is compiled only for wasm32.

Call flow:

- cargo test
- `renderer-web/tests/lib_tests.rs`
- public helper functions or native `WebRenderer`
- `oxide_renderer_api::Renderer` trait methods

## Entry points list

- `color_conversion_clamps_channels()`: verifies CSS color conversion and packed color cache keys.
- `sanitize_scale_rejects_invalid_values()`: verifies invalid scale fallback.
- `native_stub_tracks_frame_shape_and_reports_unsupported_submit()`: verifies native frame counters and unsupported submit behavior.
- `native_stub_ignores_web_camera_background_commands()`: verifies unsupported web `CameraBg` commands do not count as web draw work.
- `wasm_webgpu_runtime_images_are_explicitly_reclaimable_without_arena_tombstones()`: verifies the production wrapper delegates image release and the WebGPU resource table recycles generation-checked slots without append-only tombstones or stale-handle ABA.
- `wasm_public_exports_are_webgpu_only()`: verifies wasm public exports remain limited to WebGPU production renderer types plus the narrow Canvas indexed-quad diagnostic helper, without exposing the raw Canvas renderer type.
- `wasm_webgpu_backend_packet_vocabulary_is_frozen()`: freezes private WebGPU `DrawKind`, `GpuDraw`, coalescing, and `DrawCmd` lowering vocabulary before backend packet migrations.
- `webgpu_surface_config_uses_premultiplied_alpha()`: verifies browser WebGPU surfaces request premultiplied alpha for DOM composition.
- `wasm_webgpu_resource_counters_cover_uploads_and_passes()`: verifies the WebGPU stats struct, renderer source, and web host metric strings keep draw, clip-depth/scissor, pass, timestamp, upload, scratch, Scene3D, ID-mask, effect-uniform, and resource-creation counters synchronized.

## Logic narrative

The tests intentionally avoid browser APIs. Color tests clamp overrange and underrange values. Scale tests cover valid, zero, and NaN values. The native stub tests start frames, inspect counters, prove `CameraBg` is zero-work on web, and check that submitting on a non-wasm target returns `RenderError::Unsupported`. Source-inspection tests keep WebGPU production exports, the narrow Canvas indexed-quad diagnostic export, premultiplied-alpha surface setup, generation-checked image release/reuse, hot-path scratch reuse, timestamp-query readbacks, upload-scratch wiring, draw-state caching, clip-depth tracking, effect-uniform batching, and private packet vocabulary visible to native CI.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

The tests require only the native Rust test runner. They maintain the invariant that native builds can compile and test the web crate without pretending to render.

## Edge cases and failure modes

NaN scale collapses to `1.0`. Overrange color channels clamp to byte limits. Native submission must not succeed because it would mask missing browser execution.

## Concurrency and memory behavior

The tests are single-threaded and allocate only small strings/vectors.

## Performance notes

These are correctness and contract tests, not benchmark timers. They protect the counters consumed by the browser WebGPU performance report.
The packet-vocabulary freeze is measurement harness only. It changes no runtime path and does not claim a performance win.

## Feature flags and cfgs

They run on native targets against the non-wasm `WebRenderer` stub.

## Testing and benchmarks

Run with `cargo test -p oxide-renderer-web --tests`.

## Examples

```rust
pub fn scale() -> f32
{
   oxide_renderer_web::sanitize_scale(0.0)
}
```

## Changelog

- 2026-07-10: added source contracts for bounded generation-checked WebGPU image-slot reuse and stale-handle rejection.
- 2026-07-09: added source-contract coverage for idempotent BrowserRenderer/WebGpuRenderer image reclamation.
- 2026-06-22: froze private WebGPU packet/lowering vocabulary as measurement harness for architecture densification A/B work.
- 2026-06-22: added static coverage for the narrow Canvas indexed-quad diagnostic export while keeping the raw Canvas renderer type private.
- 2026-06-22: added static coverage for browser WebGPU premultiplied-alpha surface setup.
- 2026-06-02: added native stub regression coverage proving web `CameraBg` commands remain zero-work.
- 2026-06-01: added static coverage for WebGPU effect-uniform batching, dynamic-offset wiring, and effect uniform report counters.
- 2026-06-01: added static coverage for WebGPU clip-depth tracking and clip-state cache A/B counters.
- Added initial native coverage for the web renderer support code.
