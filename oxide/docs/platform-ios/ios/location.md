# platform-ios `src/ios/location.m`

## Intention and purpose
- Provide the iOS CoreLocation bridge for Oxide location services.
- Translate native location updates into ABI-stable samples consumed by the shared Apple Rust location service.

## Relation to the rest of the code
- `oxide-platform-apple` owns Rust-side last-sample caching, callback fanout, history, and geofence logic.
- This iOS file owns the `CLLocationManager`, delegate callbacks, permission prompts, and native sample/config ABI.
- `tests/abi_layout_tests.rs` freezes the native `OxideLocationSample` and `OxideLocationConfig` layouts against the Rust mirrors.

## Entry points list
- `oxide_host_set_location_callback(cb)`
  Registers the Rust callback for translated location samples.
- `oxide_host_set_location_error_callback(cb)`
  Registers the Rust callback for native location error text.
- `oxide_host_location_start(cfg)`
  Applies native accuracy/background/distance configuration and starts CoreLocation updates.
- `oxide_host_location_stop()`
  Stops CoreLocation updates on the main queue.
- `oxide_host_location_last(out)`
  Copies the cached latest sample when one exists.

## Logic narrative
- Native delegate updates copy `CLLocation` fields into `OxideLocationSample` before invoking Rust, because Rust should not depend on Objective-C object lifetimes.
- Start applies `OxideLocationConfig` on the main queue to match CoreLocation ownership.
- `_Static_assert` guards make native compile fail if sample/config layout drifts away from the Rust `#[repr(C)]` declarations.

## Preconditions and postconditions
- Callbacks must remain ABI-compatible with Rust declarations.
- CoreLocation manager operations run on the main queue.
- A successful last-sample read copies the cached sample into caller-owned storage.

## Edge cases and failure modes
- Negative speed/course values from CoreLocation are clamped to zero before Rust delivery.
- Missing latest sample returns no sample instead of exposing uninitialized memory.
- Unknown accuracy kinds fall back to best accuracy.

## Concurrency and memory behavior
- Main-queue synchronization protects the `CLLocationManager` and cached sample ownership.
- The static assertion change adds no runtime branches, locks, or allocation.

## Performance notes
- The 2026-06-22 change is measurement harness only. It freezes ABI shape for future performance work and claims no runtime win.

## Feature flags and cfgs
- iOS-only native bridge compiled for UIKit/CoreLocation hosts.

## Testing and benchmarks
- ABI shape and native static-assert retention are covered by `cargo test --locked -j$(sysctl -n hw.ncpu) -p oxide-platform-ios --test abi_layout_tests`.

## Changelog
- 2026-06-22: added and documented native location sample/config ABI layout guards.
