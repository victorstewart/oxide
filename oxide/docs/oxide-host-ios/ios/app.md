# oxide-host-ios Objective-C app hosts

## Intention and purpose
- Keep the production app shell physically separate from the legacy benchmark/test host.
- Keep UIKit responsible for lifecycle and raw OS delivery while Rust/Oxide owns UI behavior, rendering, and product interaction semantics.

## Relation to the rest of the code
- `oxide-host-ios/src/ios/product_app.m` is the default production shell.
- `oxide-host-ios/src/ios/app.m` is the legacy benchmark/test host compiled only by `test-scenes-entrypoint`.
- `oxide-host-ios/src/lib.rs` exports the Rust callbacks and frame APIs both hosts invoke.
- `oxide-platform-ios` and `oxide-platform-apple` provide shared service bridges consumed by the host.
- `tests/abi_layout_tests.rs` source-checks the camera callback typedef static assertions added beside the Objective-C ABI definitions.

## Entry points list
- `oxide_host_start(argc, argv)` / `UIApplicationMain`
  Launch the native iOS app host.
- `oxide_host_*` Objective-C bridge calls
  Forward window, input, text/IME, permission, push, camera, and perf events into Rust exports.
- `OxCameraFrame`, `OxCameraAudio`, and `OxCameraRecordEvent` in `app.m`
  Host-local camera callback payload typedefs mirrored by Rust callback declarations.

## Logic narrative
- `build.rs` selects exactly one Objective-C app source. A no-feature build cannot archive `app.m` or `perf_stubs.m`.
- The production source installs one full-screen `OxideTouchWindow`, one Metal surface, a text/IME adapter, and one display link. It contains no selector UI, scene router, camera switch, benchmark launch mode, or test-host chrome.
- The hosts forward raw input, lifecycle, and service events into Rust without owning product gesture state.
- Window-level `sendEvent:` forwards every touch phase with stable identity and the original `UITouch.timestamp`. Display-link callbacks forward both `timestamp` and `targetTimestamp` into Rust before late drawable acquisition.
- Rust redraw wakes coalesce onto the main thread and unpause the display link. The link sleeps when the app reports idle and wakes again without polling settle frames.
- Each shell owns redraw, display-link range, and IME because those depend on its live view state. `oxide-platform-ios/src/ios/host_services.m` owns idle timer, settings/URL, device capability, simulation, and writable-path ABI once for both hosts.
- The production shell counts thermal-state and Low Power Mode notifications cumulatively. Benchmark apps snapshot those counters at readiness and completion so a transient environment change cannot disappear behind matching endpoint state.
- Camera perf hooks translate AVFoundation sample/event data into compact C typedefs before invoking Rust callbacks.
- `_Static_assert` guards freeze the host-local camera typedef size/alignment so changes are caught before callbacks decode incompatible payloads.
- Additional `_Static_assert` guards freeze `oxide_host_stats_t`, `oxide_host_camera_tick_perf_t`, and `oxide_host_app_debug_perf_t`, because those structs are read by benchmark harnesses and feed persisted device evidence.

## Preconditions and postconditions
- Rust callback declarations and Objective-C typedefs must stay ABI-compatible.
- Camera callback payload pointers are valid for callback processing only unless Rust copies referenced buffers.
- UIKit lifecycle and layer setup must remain a shell around Oxide-owned renderer work.
- Production setup must remain full-screen and free of test-host controls or scene selection.

## Edge cases and failure modes
- Missing callbacks drop events or use diagnostic logging depending on the bridge path.
- A product target must opt into UIKit scene lifecycle so the configured production scene delegate is installed.
- Camera record events can report success, cancellation, or failure with optional path/error payloads.

## Concurrency and memory behavior
- UIKit and CAMetalLayer work remain on the main-thread host path where required.
- The ABI guard change is compile-time only and adds no callback-time allocation or branching.

## Performance notes
- The production frame loop prepares CPU work before acquiring a drawable, and sleeps at idle using wake generations instead of polling.
- Host camera performance changes still require device A/B proof before being retained.

## Feature flags and cfgs
- `test-scenes-entrypoint` selects `app.m`; its absence selects `product_app.m`.
- `perf-host-stubs` is ignored unless `test-scenes-entrypoint` is also enabled.

## Testing and benchmarks
- Production source selection and purity are covered by `production_shell_tests`.
- Host camera typedef, host stats, tick perf, debug perf, and Swift mirror guard retention are covered by `cargo test --locked -j$(sysctl -n hw.ncpu) -p oxide-host-ios --test abi_layout_tests`.

## Changelog
- 2026-08-06: added cumulative thermal-state and Low Power Mode transition counters for honest run admission.
- 2026-08-06: split the bounded production shell from the roughly 6,000-line feature-only legacy host; added exact OS/display-link timestamps and generation-based production display-link wakes while centralizing shell-independent services.
- 2026-06-22: added host stats, camera tick perf, and app debug perf ABI layout guards.
- 2026-06-22: added and documented host camera callback typedef ABI layout guards.
