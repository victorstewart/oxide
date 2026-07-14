# oxide-host-ios `lib.rs`

## Intention and purpose
- Own the Rust side of the iOS host static library: UIApplication entry, renderer setup, scene routing, input callback bridges, text/IME bridges, push/permission bridges, camera benchmarking hooks, and performance report export.
- Provide the iOS counterpart used to keep Apple host callback behavior aligned with the macOS host.

## Relation to the rest of the code
- Objective-C code in the iOS app calls exported `oxide_host_*` and `rust_entry` symbols from this file.
- The host uses `oxide-input` for raw touch/pointer/key delivery, `oxide-platform-ios` for native Apple services, and `oxide-renderer-metal` for frame rendering.
- Shared Apple services moved into `oxide-platform-apple` are consumed through `oxide-platform-ios`; this host remains responsible for UIKit shell behavior and OS event delivery.
- The callback lock policy now mirrors `oxide-host-macos`: callback registries recover poisoned mutexes instead of panicking at FFI boundaries.
- The `perf-host-stubs` Objective-C source supplies benchmark-only missing host services, but it no longer shadows secure storage because the iOS build now compiles the shared Apple Keychain bridge.

## Entry points list
- `rust_entry(argc, argv) -> libc::c_int`
  Starts the native iOS host through the Objective-C UIApplication shim.
- `oxide_host_set_window_resized_callback(...)` and `oxide_host_emit_window_resized(...)`
  Register and emit window-size/safe-area updates.
- `oxide_host_set_text_commit_callback(...)`, `oxide_host_set_text_composition_callback(...)`, `oxide_host_set_text_selection_callback(...)`, and matching emitters
  Register and emit text/IME payloads from UIKit into Rust.
- `oxide_host_set_ime_callbacks(...)`, `oxide_host_emit_ime_shown(...)`, and `oxide_host_emit_ime_hidden()`
  Bridge keyboard visibility geometry.
- `oxide_host_set_perm_callback(...)` and `oxide_host_emit_perm(...)`
  Bridge native permission status changes.
- `oxide_host_set_push_token_callback(...)`, `oxide_host_set_push_notify_callback(...)`, and matching emitters
  Bridge APNs/FCM token and notification payload events.
- `oxide_host_set_touch_callback(...)`, `oxide_host_set_pointer_callback(...)`, `oxide_host_set_key_callback(...)`, and matching emitters
  Bridge raw input samples into Oxide.
- `oxide_host_app_init(...)`, `oxide_host_app_frame(...)`, and related state/configuration exports
  Initialize and drive the renderer, scene router, camera paths, and perf harness.
- `oxide_host_app_prepare_frame(...)`, `oxide_host_app_submit_prepared_frame_with_drawable(...)`, and `oxide_host_app_cancel_prepared_frame()`
  Split CPU frame preparation from drawable-backed present work so UIKit acquires a `CAMetalDrawable` only after Rust has updated state, built the draw list, and decided the frame will submit.
- `oxide_host_app_stats(out) -> libc::c_int`
  Exports the host stats ABI consumed by Objective-C and Swift benchmark harnesses.
- `oxide_host_on_memory_warning()`
  Purges retained effect/bloom targets and prepared render chunks, marks the frame dirty, and forwards critical pressure to telemetry.

## Logic narrative
- Callback registries store plain `extern "C" fn` pointers in `OnceLock<Mutex<Option<_>>>` slots because Objective-C code can install callbacks before the app renderer is active.
- Registration and emission paths recover poisoned mutexes. At native FFI boundaries, preserving host liveness is preferable to panicking because an earlier Rust unwind may have occurred outside the current OS callback.
- Emitters copy the function pointer out of the slot before invoking it, so callback code does not run while holding the registry mutex.
- Fallback logging for text, key, and push payloads validates null/length pairs before constructing slices; a null pointer with zero length is treated as an empty payload.
- Renderer and app lifecycle behavior remains unchanged by callback hardening.
- The drawable-backed iOS path now mirrors macOS: prepare Rust frame work first, acquire `nextDrawable` late with timeout enabled in Objective-C or the Swift perf runtime, then submit the prepared frame to Metal or cancel it if no drawable is returned.
- Compile-time layout assertions freeze `OxideHostStats` and the private camera perf/contract snapshot mirrors so benchmark out-parameters cannot silently drift from their native or Swift consumers.

## Preconditions and postconditions
- Native callers must pass valid payload pointers when `len > 0`.
- Callback functions must remain ABI-compatible with their exported setter signatures.
- A missing callback registration drops the event after optional fallback logging.

## Edge cases and failure modes
- Poisoned callback mutexes are recovered, preserving the last registered callback state.
- Null payload pointers with zero length are accepted by fallback log paths.
- Null payload pointers with non-zero length are ignored by fallback log paths instead of building invalid slices.
- App lifecycle/router `expect` calls are outside this callback-hardening slice and remain separate cleanup candidates.

## Concurrency and memory behavior
- Callback slots are process-global and protected by small mutexes.
- The hot input callback path performs a mutex lock to copy the callback pointer, then releases it before dispatch.
- Raw touch callbacks preserve the OS sample timestamp in `TouchEvent::timestamp_ns` before routing through `oxide-input`.
- No heap allocation is added to the callback-installed input path; fallback logging may format strings only when no callback is registered.

## Performance notes
- Renderer construction selects the normal three-slot visible-host resource mode; actual Metal command-buffer completion still protects reuse and saturated frames coalesce without blocking.
- Callback pointer copying is constant-time and mirrors the macOS host pattern.
- Real Metal shader compilation is available after installing Apple's Metal Toolchain component; renderer-metal checks now generate `default.metallib` instead of a placeholder.

## Feature flags and cfgs
- iOS-only native services are compiled behind `target_os = "ios"` guards.
- Host unit tests compile the Rust callback bridge on the local host without launching UIKit.
- Critical memory warnings purge renderer-owned effect/bloom targets, retained and pooled layer textures, and persistent prepared chunks, then mark the frame dirty so visible content rebuilds lazily through the normal Rust render path.

## Testing and benchmarks
- Covered by `cargo test -p oxide-host-ios --tests --locked`.
- Device-target compile coverage uses `cargo check -p oxide-host-ios --target aarch64-apple-ios --tests --locked`.
- Host camera typedef, stats, tick/debug perf, and camera snapshot ABI guard retention is covered by [abi_layout_tests.md](tests/abi_layout_tests.md).
- Camera benchmark contract coverage lives in [camera_benchmark_tests.md](tests/camera_benchmark_tests.md); it statically gates `AVCaptureVideoPreviewLayer` to explicit baseline or diagnostic-only paths.

## Examples
```rust
oxide_host_set_touch_callback(Some(touch_cb));
oxide_host_emit_touch(10, 0, 1.0, 2.0, 0.5, 1, 0.0, 0.0, 0, 0, 100);
```

## Changelog
- 2026-07-14: routed critical memory warnings through the production retained-layer storage purge before requesting the rebuild frame.
- 2026-07-13: purged byte-budgeted prepared Metal chunks alongside effect targets on critical memory pressure.
- 2026-07-13: selected the three-slot visible Metal frame-resource mode instead of retaining the deeper offscreen/perf allocation.
- 2026-06-22: froze host stats and camera benchmark snapshot ABI layouts, including Swift benchmark-runtime host-stat mirror fields.
- 2026-06-22: added iOS host camera typedef ABI static-assert retention coverage.
- 2026-06-01: added macOS-side source gates keeping `AVCaptureVideoPreviewLayer` out of the product custom camera preview path.
- 2026-06-01: enabled timeout-capable `CAMetalLayer.nextDrawable` acquisition on the product iOS host so prepared frames can cancel instead of blocking indefinitely under drawable pressure.
- 2026-05-31: split iOS frame preparation from drawable submission so `nextDrawable` is acquired after CPU frame work in app and perf-host paths.
- 2026-05-31: preserved raw touch sample timestamps in `TouchEvent::timestamp_ns`.
- 2026-05-19: removed secure-storage ABI definitions from `perf-host-stubs` so iOS host builds use the shared Apple Keychain bridge.
- 2026-05-19: recovered iOS host callback mutex poisoning, hardened null/empty fallback payload handling, and aligned callback bridge behavior with the macOS host.
