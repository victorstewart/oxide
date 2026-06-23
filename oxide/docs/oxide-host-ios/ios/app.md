# oxide-host-ios `src/ios/app.m`

## Intention and purpose
- Own the Objective-C iOS host shell for the Oxide app runtime, renderer surface, input delivery, OS service glue, and performance harness hooks.
- Keep UIKit responsible for lifecycle and raw OS delivery while Rust/Oxide owns UI behavior, rendering, and product interaction semantics.

## Relation to the rest of the code
- `oxide-host-ios/src/lib.rs` exports the Rust callbacks and frame APIs this file invokes.
- `oxide-platform-ios` and `oxide-platform-apple` provide shared service bridges consumed by the host.
- `tests/abi_layout_tests.rs` source-checks the camera callback typedef static assertions added beside the Objective-C ABI definitions.

## Entry points list
- `main(argc, argv)` / `UIApplicationMain`
  Launch the native iOS app host.
- `oxide_host_*` Objective-C bridge calls
  Forward window, input, text/IME, permission, push, camera, and perf events into Rust exports.
- `OxCameraFrame`, `OxCameraAudio`, and `OxCameraRecordEvent`
  Host-local camera callback payload typedefs mirrored by Rust callback declarations.

## Logic narrative
- The host installs UIKit objects and forwards raw input, lifecycle, and service events into Rust without owning product gesture state.
- Camera perf hooks translate AVFoundation sample/event data into compact C typedefs before invoking Rust callbacks.
- `_Static_assert` guards freeze the host-local camera typedef size/alignment so changes are caught before callbacks decode incompatible payloads.
- Additional `_Static_assert` guards freeze `oxide_host_stats_t`, `oxide_host_camera_tick_perf_t`, and `oxide_host_app_debug_perf_t`, because those structs are read by benchmark harnesses and feed persisted device evidence.

## Preconditions and postconditions
- Rust callback declarations and Objective-C typedefs must stay ABI-compatible.
- Camera callback payload pointers are valid for callback processing only unless Rust copies referenced buffers.
- UIKit lifecycle and layer setup must remain a shell around Oxide-owned renderer work.

## Edge cases and failure modes
- Missing callbacks drop events or use diagnostic logging depending on the bridge path.
- Camera record events can report success, cancellation, or failure with optional path/error payloads.

## Concurrency and memory behavior
- UIKit and CAMetalLayer work remain on the main-thread host path where required.
- The ABI guard change is compile-time only and adds no callback-time allocation or branching.

## Performance notes
- The 2026-06-22 change is measurement harness only. It does not affect frame preparation, drawable acquisition, encode, present, or camera sample delivery timing.
- Host camera performance changes still require device A/B proof before being retained.

## Feature flags and cfgs
- iOS host Objective-C source; behavior depends on runtime launch arguments and perf environment gates already documented in host tests.

## Testing and benchmarks
- Host camera typedef, host stats, tick perf, debug perf, and Swift mirror guard retention are covered by `cargo test --locked -j$(sysctl -n hw.ncpu) -p oxide-host-ios --test abi_layout_tests`.

## Changelog
- 2026-06-22: added host stats, camera tick perf, and app debug perf ABI layout guards.
- 2026-06-22: added and documented host camera callback typedef ABI layout guards.
