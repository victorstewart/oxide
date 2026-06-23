# oxide-host-ios tests `abi_layout_tests.rs`

## Intention and purpose
- Ensure the iOS host Objective-C camera typedefs keep native `_Static_assert` guards beside their definitions.
- Protect the host camera callback ABI while broader architecture work prepares generated or consolidated ABI contracts.
- Freeze host stats, camera tick perf, app debug perf, and private camera snapshot layout guards used by benchmark evidence collection.

## Relation to the rest of the code
- Covers `oxide-host-ios/src/ios/app.m` source guards for host-local camera frame, audio, and recording event payloads.
- Covers `OxideHostStats` Rust layout and Objective-C/Swift host stats mirrors consumed by the iOS benchmark runtime.
- Covers private camera perf/contract snapshot Rust assertions and iOS native camera bridge static assertions.
- Complements Rust-side platform camera ABI layout tests in `oxide-platform-ios`.

## Entry points list
- `ios_host_camera_typedefs_keep_abi_static_asserts`
  Checks `app.m` retains `_Static_assert` size/alignment guards for `OxCameraFrame`, `OxCameraAudio`, and `OxCameraRecordEvent`.
- `ios_host_stats_layout_is_frozen`
  Checks Rust `OxideHostStats` size/alignment, native host stats/tick/debug static assertions, and Swift host-stat tail fields.
- `ios_host_private_camera_snapshots_keep_abi_static_asserts`
  Checks Rust camera snapshot compile-time layout assertions and native camera snapshot `_Static_assert` guards.

## Logic narrative
- The test reads the Objective-C source with `include_str!` and checks for the exact guard fragments.
- The test sizes `OxideHostStats` directly through Rust because it is public to the host crate tests.
- Other checks are intentionally source-level because host-local typedefs and private iOS snapshot structs are not exported as public Rust types the test can size directly.

## Preconditions and postconditions
- Passing tests mean the Objective-C source still has native compile-time guardrails for host camera callback payloads, host stats mirrors, tick/debug payloads, and camera snapshot payloads.
- Intentional ABI changes must update typedefs, static asserts, Rust callback declarations, and this test together.

## Edge cases and failure modes
- The test does not launch UIKit, AVFoundation, or the host app.
- It detects deleted or stale guard text, not semantic camera behavior.

## Concurrency and memory behavior
- Single-threaded source check; no runtime host state is initialized.

## Performance notes
- This is measurement harness only. It changes no product renderer behavior and is not counted as an implementation performance win.
- The Swift host-stat mirror now includes the Rust tail fields so benchmark out-parameters do not corrupt adjacent Swift stack memory before A/B evidence is collected.

## Feature flags and cfgs
- No feature-specific branches.

## Testing and benchmarks
- Run with `cargo test --locked -j$(sysctl -n hw.ncpu) -p oxide-host-ios --test abi_layout_tests`.

## Changelog
- 2026-06-22: expanded host ABI coverage to stats, tick/debug perf, private camera snapshots, and the Swift benchmark-runtime host-stat mirror.
- 2026-06-22: added host camera typedef ABI static-assert retention coverage.
