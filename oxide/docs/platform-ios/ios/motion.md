# platform-ios `src/ios/motion.m`

## Intention and purpose
- Provide the iOS CoreMotion altimeter bridge for Oxide motion services.
- Translate pressure and relative-altitude samples into an ABI-stable payload consumed by Rust.

## Relation to the rest of the code
- `oxide-platform-apple` owns Rust-side motion callback fanout and bounded history.
- This iOS file owns `CMAltimeter`, the native update queue, and C callback delivery.
- `tests/abi_layout_tests.rs` freezes the native `OxideMotionSample` layout against the Rust mirror.

## Entry points list
- `oxide_host_set_motion_callback(cb)`
  Registers the Rust callback for translated motion samples.
- `oxide_host_motion_start()`
  Starts relative-altitude updates when the device supports them.
- `oxide_host_motion_stop()`
  Stops active CoreMotion updates.
- `oxide_host_motion_is_active()`
  Reports whether native updates are currently running.

## Logic narrative
- CoreMotion data is copied into `OxideMotionSample` so Rust receives primitive fields and presence bits rather than Objective-C objects.
- Missing pressure or relative-altitude values are represented by explicit `has_*` flags and zeroed numeric fields.
- `_Static_assert` guards keep the native payload size/alignment in lockstep with the Rust `#[repr(C)]` type.

## Preconditions and postconditions
- The callback pointer must remain ABI-compatible with Rust declarations.
- Start returns a failure code when relative altitude is unavailable.
- Stop leaves the bridge inactive when an altimeter was running.

## Edge cases and failure modes
- Nil `CMAltitudeData` or non-nil native errors are dropped before Rust callback delivery.
- Repeated start calls while already active return success without starting duplicate native streams.

## Concurrency and memory behavior
- Native updates run on an `NSOperationQueue` owned by this bridge.
- The ABI guard change is compile-time only and adds no callback-time allocation.

## Performance notes
- The 2026-06-22 change is measurement harness only. It freezes ABI shape for future measured cleanup and claims no runtime win.

## Feature flags and cfgs
- iOS-only native bridge compiled for CoreMotion-capable hosts.

## Testing and benchmarks
- ABI shape and native static-assert retention are covered by `cargo test --locked -j$(sysctl -n hw.ncpu) -p oxide-platform-ios --test abi_layout_tests`.

## Changelog
- 2026-06-22: added and documented native motion sample ABI layout guards.
