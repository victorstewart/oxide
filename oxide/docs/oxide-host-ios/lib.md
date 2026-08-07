# oxide-host-ios `lib.rs`

## Intention and purpose
- Own the Rust side of the production iOS app host: explicit app injection, UIApplication entry, renderer setup, raw event delivery, two-stage frame submission, and platform services.
- Keep legacy scene routing, camera benchmarks, fixture loading, and perf exports behind one explicit test-host feature.
- Provide the iOS counterpart used to keep Apple host callback behavior aligned with the macOS host.

## Relation to the rest of the code
- Objective-C code in the iOS app calls exported `oxide_host_*` and `rust_entry` symbols from this file.
- `build.rs` selects the bounded `src/ios/product_app.m` shell by default and the legacy `src/ios/app.m` host only for `test-scenes-entrypoint`.
- The host uses `oxide-input` for raw touch/pointer/key delivery, `oxide-platform-ios` for native Apple services, and `oxide-renderer-metal` for frame rendering.
- Shared Apple services moved into `oxide-platform-apple` are consumed through `oxide-platform-ios`; this host remains responsible for UIKit shell behavior and OS event delivery.
- The callback lock policy now mirrors `oxide-host-macos`: callback registries recover poisoned mutexes instead of panicking at FFI boundaries.
- The `perf-host-stubs` Objective-C source supplies benchmark-only missing host services, but it no longer shadows secure storage because the iOS build now compiles the shared Apple Keychain bridge.

## Entry points list
- `rust_entry(argc, argv) -> libc::c_int`
  Starts the legacy benchmark/test host and exists only with `test-scenes-entrypoint`.
- `install_app(Box<dyn App>)` and `run_app(argc, argv, Box<dyn App>)`
  Install exactly one production app before host initialization and start the native UIApplication shell. The raw C `argc`/`argv` start call is explicitly unsafe and leaves the app slot untouched off iOS.
- `display_link_frame_rate_range() -> Option<DisplayLinkFrameRateRange>`
  Reads the active shell's atomically published display-link range without dereferencing UIKit or Core Animation state from Rust.
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
- `oxide_host_app_prepare_frame_timed(...)`, `oxide_host_app_submit_prepared_frame_with_drawable(...)`, and `oxide_host_app_cancel_prepared_frame()`
  Split CPU frame preparation from drawable-backed present work so UIKit acquires a `CAMetalDrawable` only after Rust has updated state, built the draw list, and decided the frame will submit.
- `oxide_host_app_stats(out) -> libc::c_int`
  Exports the legacy host stats ABI consumed by Objective-C and Swift benchmark harnesses under `test-scenes-entrypoint`.
- `oxide_host_on_memory_warning()`
  Purges retained effect/bloom targets, layer storage, prepared render chunks, and immutable ID-mask fields, marks the frame dirty, and forwards critical pressure to telemetry.

## Logic narrative
- Callback registries store plain `extern "C" fn` pointers in `OnceLock<Mutex<Option<_>>>` slots because Objective-C code can install callbacks before the app renderer is active.
- Registration and emission paths recover poisoned mutexes. At native FFI boundaries, preserving host liveness is preferable to panicking because an earlier Rust unwind may have occurred outside the current OS callback.
- Emitters copy the function pointer out of the slot before invoking it, so callback code does not run while holding the registry mutex.
- Fallback logging for text, key, and push payloads validates null/length pairs before constructing slices; a null pointer with zero length is treated as an empty payload.
- Renderer and app lifecycle behavior remains unchanged by callback hardening.
- The drawable-backed iOS path now mirrors macOS: prepare Rust frame work first, acquire `nextDrawable` late with timeout enabled in Objective-C or the Swift perf runtime, then submit the prepared frame to Metal or cancel it if no drawable is returned.
- The runtime-image uploader forwards row-major RGBA bytes and immutable nearest-or-linear sampling directly into the Metal resource owner. It maps Metal's invalid zero-handle sentinel to `None` and releases successful handles through the same owner. The host performs no channel conversion, staging copy, or per-draw sampling decision.
- Apps that use the original `App::draw` contract can render through one persistent host-owned `DrawListBuilder` adapter. `RenderContext` borrows it for the draw callback, and the host clears and reuses its storage rather than allocating an intermediate command graph each frame.
- Apps with an owned `PreparedFrame` submit that draw list directly. The host only copies damage into reusable scratch and preserves the prepared frame across a generation-bound retry when drawable acquisition, Metal backpressure, or submission fails.
- Frame wake generations are acknowledged only after a successful Metal submit. A newer wake or changed drawable geometry invalidates a retained retry, while `FrameDemand::NextVsync` continues scheduling without legacy settle frames.
- Window, raw input, text, IME, and lifecycle events are delivered directly to the installed app. Posted tasks are drained before preparation and successful submits report observational `AppEvent::RendererStats` without invalidating frame state.
- Production `AppState` contains only renderer, injected-app, prepared-frame, damage, and wake ownership. The scene router, camera/perf snapshots, telemetry services, fixture loaders, and report storage compile only for the legacy feature.
- The product shell forwards native touch and display-link timestamps, prepares Rust work before acquiring a drawable, and publishes its configured display-link range through a lock-free snapshot.
- The host exposes no platform motion-preference state, control, or ABI; authored Oxide animation durations pass through unchanged.
- Compile-time layout assertions freeze `OxideHostStats` and the private camera perf/contract snapshot mirrors so benchmark out-parameters cannot silently drift from their native or Swift consumers.

## Preconditions and postconditions
- Native callers must pass valid payload pointers when `len > 0`.
- Callback functions must remain ABI-compatible with their exported setter signatures.
- A missing callback registration drops the event after optional fallback logging.

## Edge cases and failure modes
- Poisoned callback mutexes are recovered, preserving the last registered callback state.
- Null payload pointers with zero length are accepted by fallback log paths.
- Null payload pointers with non-zero length are ignored by fallback log paths instead of building invalid slices.
- Injected-app cancellation and submission failure retain the immutable prepared frame for retry; legacy test-host cancellation continues clearing pending damage.
- App lifecycle/router `expect` calls are outside this callback-hardening slice and remain separate cleanup candidates.

## Concurrency and memory behavior
- Callback slots are process-global and protected by small mutexes.
- The hot input callback path performs a mutex lock to copy the callback pointer, then releases it before dispatch.
- Raw touch callbacks preserve the OS sample timestamp in `TouchEvent::timestamp_ns` before routing through `oxide-input`.
- No heap allocation is added to the callback-installed input path; fallback logging may format strings only when no callback is registered.
- The compatibility draw adapter retains its draw-list capacities across frames.
- App-owned prepared draw lists are not copied, and posted tasks are drained outside the app-state lock.
- Display-link range reads perform one native call over an atomic integer snapshot; no UIKit object crosses the Rust boundary.

## Performance notes
- Renderer construction selects the normal three-slot visible-host resource mode; actual Metal command-buffer completion still protects reuse and saturated frames coalesce without blocking.
- The product display link sleeps when frame demand is idle and drawable acquisition remains after app preparation.
- Callback pointer copying is constant-time and mirrors the macOS host pattern.
- Real Metal shader compilation is available after installing Apple's Metal Toolchain component; renderer-metal checks now generate `default.metallib` instead of a placeholder.

## Feature flags and cfgs
- iOS-only native services are compiled behind `target_os = "ios"` guards.
- `tokio-runtime` is additive and forwards to `oxide-platform-ios/tokio-runtime`; both UIApplication entry paths install the spawn hook only under that feature.
- `test-scenes-entrypoint` selects the legacy Objective-C host and is the only feature that includes `oxide-test-scenes`, `oxide-perf-runner`, text fixtures, permissions, networking, telemetry, and PNG support.
- Production app installation fails with `LegacyTestHostSelected` when the mutually exclusive legacy host is selected.
- `perf-host-stubs` adds its Objective-C source only when `test-scenes-entrypoint` is also active.
- Critical memory warnings purge renderer-owned effect/bloom targets, retained and pooled layer textures, persistent prepared chunks, and ID-mask raster/JFA fields, then mark the frame dirty so visible content rebuilds lazily through the normal Rust render path.

## Testing and benchmarks
- Production coverage uses `cargo test --locked -p oxide-host-ios --no-default-features --tests`.
- Legacy coverage uses `cargo test --locked -p oxide-host-ios --features test-scenes-entrypoint --tests`.
- ARM64 device-target checks run both feature configurations with `IPHONEOS_DEPLOYMENT_TARGET=18.0`.
- Host camera typedef, stats, tick/debug perf, and camera snapshot ABI guard retention is covered by [abi_layout_tests.md](tests/abi_layout_tests.md).
- Camera benchmark contract coverage lives in [camera_benchmark_tests.md](tests/camera_benchmark_tests.md); it statically gates `AVCaptureVideoPreviewLayer` to explicit baseline or diagnostic-only paths.
- [injected_app_tests.md](tests/injected_app_tests.md) freezes direct sampled-RGBA uploader wiring, invalid-handle mapping, and renderer-owned release.
- `tests/drawable_tests.rs` freezes injected wake acknowledgement, retry retention, and lifecycle timing reset behavior.
- `tests/production_shell_tests.rs` freezes native source selection, bounded shell ownership, exact timestamp delivery, and atomic display-link observation.

## Examples
```rust
oxide_host_set_touch_callback(Some(touch_cb));
oxide_host_emit_touch(10, 0, 1.0, 2.0, 0.5, 1, 0.0, 0.0, 0, 0, 100);
```

## Changelog
- 2026-08-06: isolated legacy dependencies, state, exports, native services, and test suites behind `test-scenes-entrypoint`; made Tokio support independently additive.
- 2026-08-06: added the bounded product Objective-C shell, exact native frame/input timing, idle display-link scheduling, and thread-safe display-link range observation.
- 2026-08-06: added explicit production app injection, app-owned prepared frames with a persistent legacy fallback encoder, display-link timing, wake-generation retry scheduling, direct event delivery, and renderer feedback.
- 2026-08-06: adapted app draw commands to reusable host draw-list storage and wired sampled runtime images directly into Metal-owned resources.
- 2026-08-06: removed the product motion toggle and the obsolete motion-preference host state and ABI.
- 2026-07-14: purged immutable ID-mask raster/JFA fields on critical memory pressure.
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
