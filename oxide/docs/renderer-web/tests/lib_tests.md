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

- `wasm_webgpu_solid_vertex_colors_decode_aabbggrr_and_interpolate()`: verifies zero inheritance, packed endpoints, all solid topology paths, and WGSL color interpolation.
- `canvas_colored_quad_classifies_flat_and_opposing_edge_colors()`: accepts flat, horizontal, vertical, and inherited colors.
- `canvas_colored_quad_rejects_other_nonzero_topologies()`: rejects four-vertex, skewed, per-corner, and duplicate-mismatch shapes.
- `color_conversion_clamps_channels()`: verifies CSS color conversion and packed color cache keys.
- `sanitize_scale_rejects_invalid_values()`: verifies invalid scale fallback.
- `native_stub_tracks_frame_shape_and_reports_unsupported_submit()`: verifies native frame counters and unsupported submit behavior.
- `native_stub_ignores_web_camera_background_commands()`: verifies unsupported web `CameraBg` commands do not count as web draw work.
- `wasm_webgpu_runtime_images_are_explicitly_reclaimable_without_arena_tombstones()`: verifies the production wrapper delegates image release and the WebGPU resource table recycles generation-checked slots without append-only tombstones or stale-handle ABA.
- `wasm_public_exports_are_webgpu_only()`: verifies wasm public exports remain limited to WebGPU production renderer types plus the narrow Canvas indexed-quad diagnostic helper, without exposing the raw Canvas renderer type.
- `wasm_webgpu_backend_packet_vocabulary_is_frozen()`: freezes private WebGPU `DrawKind`, `GpuDraw`, coalescing, and `DrawCmd` lowering vocabulary before backend packet migrations.
- `wasm_webgpu_draw_encoding_reuses_scratch_storage()`: requires retained packed frame streams, reusable primitive scratch, and removal of duplicate frame byte vectors.
- `webgpu_surface_config_uses_premultiplied_alpha()`: verifies browser WebGPU surfaces request premultiplied alpha for DOM composition.
- `wasm_webgpu_resource_counters_cover_uploads_and_passes()`: verifies the WebGPU stats struct, renderer source, and web host metric strings keep draw, clip-depth/scissor, pass, timestamp, upload, scratch, Scene3D, ID-mask, effect-uniform, and resource-creation counters synchronized.
- `wasm_webgpu_prepared_chunks_are_budgeted_and_resource_invalidated()`: freezes the persistent prepared cache, aggregate and hybrid bundle paths, logical-byte budget, resource/device/resize invalidation, static/dynamic prepared boundaries, and replay counters used by C25/C26 browser proof.
- The same source contract now freezes C26's three-slice property ring, dynamic uniform resolution, separate property counters, direct dynamic prepared boundary, and transform-linked clip handling while retaining C25 static bundle ownership.
- `wasm_webgpu_layers_are_generation_keyed_and_local_sized()`: freezes C30's complete retained-layer key, immutable-snapshot plan reuse, dependency invalidation, pixel-grid-snapped local targets, inherited nested viewport/scissor state, normalized UVs, local effect copies, and immediate body skip.
- `wasm_webgpu_id_mask_fields_use_exact_packed_targets_with_wide_fallback()`: freezes C35's `Rgba16Uint` capability gate, two-texture packed ownership, exact invalid-coordinate bounds, binding/pipeline entry points, semantic readback decoder, 2× field-byte accounting, and four-`Rgba16Float` fallback.

## Logic narrative

The tests intentionally avoid browser APIs. Solid-color tests include the crate-private pure Canvas classifier, while packed-geometry unit tests own exact WebGPU color bytes. Source inspection freezes wasm-only lowering and shader interpolation. Color tests clamp overrange and underrange values. Scale tests cover valid, zero, and NaN values. The native stub tests start frames, inspect counters, prove `CameraBg` is zero-work on web, and check that submitting on a non-wasm target returns `RenderError::Unsupported`. Source-inspection tests keep WebGPU production exports, the narrow Canvas indexed-quad diagnostic export, premultiplied-alpha surface setup, generation-checked image release/reuse, typed packed streams, u16/u32 draw packets, hot-path scratch reuse, timestamp-query readbacks, upload-scratch wiring, draw-state caching, clip-depth tracking, effect-uniform batching, prepared chunk/bundle/LRU ownership, local retained-layer target ownership, and private packet vocabulary visible to native CI.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

The tests require only the native Rust test runner. They maintain the invariant that native builds can compile and test the web crate without pretending to render.

## Edge cases and failure modes

NaN scale collapses to `1.0`. Overrange color channels clamp to byte limits. Colored-solid coverage includes indexed rejection, four vertices, skew, independent corner colors, and conflicting duplicate corners. Native submission must not succeed because it would mask missing browser execution.

## Concurrency and memory behavior

The tests are single-threaded and allocate only small strings/vectors.

## Performance notes

These are correctness and contract tests, not benchmark timers. They protect the counters consumed by the browser WebGPU performance report.
The packet-vocabulary freeze is measurement harness only. It changes no runtime path and does not claim a performance win.
C30 browser proof complements these structural checks with exact parent/candidate pixels and direct residency/pass counters; source matching does not substitute for runtime evidence. C35 likewise requires real Dawn shader creation, exact decoded field comparison, presented pixels, and paired direct GPU timestamps in addition to the source contract. C37 requires real WGSL pipeline creation, count/DPR timings, and one-pixel-boundary-classified captures beyond the static 36-byte ABI and no-tessellator assertions.

## Feature flags and cfgs

They run on native targets against the non-wasm `WebRenderer` stub.

## Testing and benchmarks

Run with `cargo test --locked -p oxide-renderer-web --test lib_tests`. Compile wasm behavior with `cargo check --locked --target wasm32-unknown-unknown -p oxide-renderer-web`. The local-layer runtime companions are the C30 browser capture and `run_webgpu_local_layers_c30.mjs`; mode `2` exercises the C31 bounded navigation/purge path. The C33 companion is `check_webgpu_browser_golden.mjs --id-mask-cache-only`, which executes real WebGPU hits, misses, one-entry thrash, bounded LRU reuse, and purge/reentry paths. C35 uses `check_webgpu_browser_golden.mjs --id-mask-matrix-out PATH` for the seven-dimension exact raster/final-field matrix, reuses the asymmetric multi-seed readback, and runs the 512-square forced-miss workload against parent and candidate packages. C37 uses `--rrect-architecture-only` and `--target rrect`.

## Examples

```rust
pub fn scale() -> f32
{
   oxide_renderer_web::sanitize_scale(0.0)
}
```

## Changelog

- 2026-07-14: froze the C37 36-byte RRect instance ABI, six-vertex analytic WGSL pipeline, adjacent compatible batching, prepared-buffer ownership, counters, and removal of tessellation/trigonometry scratch.
- 2026-07-14: connected C35 source contracts to the real-Dawn seven-dimension exact field matrix.
- 2026-07-14: froze C35 packed/wide field formats, capability selection, coordinate bounds, shader bindings, semantic decode, and exact 2× field-byte accounting.
- 2026-07-14: froze the C33 complete ID-mask key, compositor-only pass/uniform path, four-entry byte budget, exact 34-byte-per-pixel logical accounting, and device/memory purge contracts.
- 2026-07-14: froze C31 hard admission, protected LRU eviction, compatible pooling, absent aging, device/memory purge wiring, and telemetry fields.
- 2026-07-14: added C30 contracts for generation/resource-complete retained layers, pixel-grid-snapped local textures, inherited nested viewports/scissors, normalized composites, and local effect-copy accounting.
- 2026-07-13: added the C25 prepared-cache, bundle/direct, aggregate-replay, budget, invalidation, and counter source contract.
- 2026-07-13: extended the prepared contract with C26 dynamic property-ring and clip assertions.
- 2026-07-12: updated packet and scratch contracts for direct POD vertices, segmented u16/u32 index streams, and removal of duplicate byte vectors.
- 2026-07-12: added a source contract requiring one aligned ID-mask uniform arena upload and distinct dynamic offsets for raster, seed, every JFA jump, and compositor records.
- 2026-07-12: added a source contract for snapshot-only R8/RGBA16F ID-mask readback.
- 2026-07-12: added explicit logical-versus-allocated accounting defaults and overflow-safe texture-byte source coverage.
- 2026-07-12: added packed solid-color decode/interpolation coverage and behavioral Canvas gradient admission/rejection tests.
- 2026-07-10: added source contracts for bounded generation-checked WebGPU image-slot reuse and stale-handle rejection.
- 2026-07-09: added source-contract coverage for idempotent BrowserRenderer/WebGpuRenderer image reclamation.
- 2026-06-22: froze private WebGPU packet/lowering vocabulary as measurement harness for architecture densification A/B work.
- 2026-06-22: added static coverage for the narrow Canvas indexed-quad diagnostic export while keeping the raw Canvas renderer type private.
- 2026-06-22: added static coverage for browser WebGPU premultiplied-alpha surface setup.
- 2026-06-02: added native stub regression coverage proving web `CameraBg` commands remain zero-work.
- 2026-06-01: added static coverage for WebGPU effect-uniform batching, dynamic-offset wiring, and effect uniform report counters.
- 2026-06-01: added static coverage for WebGPU clip-depth tracking and clip-state cache A/B counters.
- Added initial native coverage for the web renderer support code.
