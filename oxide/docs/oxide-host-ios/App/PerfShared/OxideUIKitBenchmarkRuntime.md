# oxide-host-ios `App/PerfShared/OxideUIKitBenchmarkRuntime.swift`

## Intention and purpose
- Provide the Swift-side UIKit performance runtime used by the iOS host benchmark app.
- Keep UIKit parity cases, host stats sampling, visible-output validation, and device report assembly in one benchmark harness.

## Relation to the rest of the code
- Calls Rust exports from `oxide-host-ios/src/lib.rs` through `@_silgen_name` functions.
- Mirrors Objective-C host perf structs from `oxide-host-ios/src/ios/app.m`.
- Persists device-facing metrics that are later consumed by `xtask` and benchmark report tooling.

## Entry points list
- `oxideHostAppStats(_:) -> Int32`
  FFI call into Rust `oxide_host_app_stats`, writing an `OxideHostStats` out-parameter.
- `oxideHostCameraTickPerf(_:) -> Int32`
  FFI call into the Objective-C host camera tick perf snapshot.
- `oxideHostAppDebugPerf(_:) -> Int32`
  FFI call into the Objective-C app debug perf snapshot.
- UIKit benchmark factory and runner functions
  Build camera, image, collection, launch, and interaction workloads for device comparison.

## Logic narrative
- The runtime chooses a benchmark case, drives UIKit or Oxide host paths, samples host stats, validates visible output, and serializes report rows.
- `OxideHostStats` must match the Rust `#[repr(C)]` layout exactly because Rust writes the whole struct through the Swift out-pointer.
- The 2026-06-22 change adds the host idle/submission tail fields to the Swift mirror so the out-parameter size matches Rust and does not overwrite adjacent Swift stack storage.
- The image-region journey builds the same 1,000 unique 28 x 28 sRGB icons for both UIKit styles. The idiomatic row uses reusable `UIImageView` cells, while the optimized row precomposes the immutable grid once and scrolls one non-animating `CALayer` so the measured warm path does not redraw hundreds of images per phase. Both rows await one display presentation after every phase so state-update speed cannot masquerade as visible scroll performance.

## Preconditions and postconditions
- The app must link Rust and Objective-C symbols named by the `@_silgen_name` declarations.
- Swift mirror structs must stay ABI-compatible with their Rust or Objective-C producers.
- Device report rows must use device-authoritative measurements, not simulator numbers, for official comparisons.

## Edge cases and failure modes
- Missing host stats return `nil` from snapshot helpers and the caller records partial or skipped evidence.
- A stale Swift mirror can corrupt benchmark harness memory because the producer writes through a raw out-pointer.

## Concurrency and memory behavior
- Stats mirrors are stack values passed by mutable pointer for synchronous fill.
- Benchmark workloads may run UIKit and Oxide host paths on main-thread UI code while collecting native snapshots.

## Performance notes
- This file is measurement harness. Changes here enable trustworthy A/B evidence but are not counted as product runtime wins by themselves.
- The host-stat mirror correction prevents invalid benchmark evidence; it does not claim a faster renderer or UI path.
- The optimized image-region layer trades one bounded 360 x 2352 warm image for constant-work layer-position updates; setup and precomposition occur before the measured scroll loop.

## Feature flags and cfgs
- iOS benchmark-app Swift runtime; individual cases are selected by test names, launch arguments, and benchmark configuration.

## Testing and benchmarks
- Mirror guard coverage is provided by `cargo test --locked -j$(sysctl -n hw.ncpu) -p oxide-host-ios --test abi_layout_tests`.

## Changelog
- 2026-07-15: added dedicated idiomatic and optimized 1,000-image region-grid parity paths; the optimized path precomposes the immutable grid and scrolls one layer instead of redrawing visible images at every phase.
- 2026-06-22: added missing host idle/submission tail fields to the Swift `OxideHostStats` mirror and documented the benchmark ABI contract.
