# platform-ios tests `abi_layout_tests.rs`

## Intention and purpose
- Freeze iOS platform `#[repr(C)]` ABI sizes and alignments before generated ABI or bridge cleanup work starts.
- Verify iOS Objective-C bridge files keep `_Static_assert` guards beside camera, location, and motion payload mirrors.

## Relation to the rest of the code
- Protects the Rust declarations exported by `oxide-platform-ios` and consumed by iOS native bridges.
- Complements shared Apple ABI coverage in `platform-apple` and host camera typedef coverage in `oxide-host-ios`.

## Entry points list
- `ios_platform_ffi_layouts_are_frozen`
  Checks Rust-side sizes and alignments for camera, location, motion, and contact ABI structs.
- `ios_objc_bridges_keep_abi_static_asserts`
  Checks the iOS Objective-C camera, location, and motion source files retain matching `_Static_assert` size/alignment guards, including camera perf/contract snapshot guards.

## Logic narrative
- The test uses `std::mem::size_of` and `std::mem::align_of` because those values are the Rust-side ABI contract native code must mirror.
- Source checks are intentionally simple string guards: they make accidental deletion of the native compile-time assertions visible in Rust tests.

## Preconditions and postconditions
- Passing tests mean the frozen iOS ABI shapes remain unchanged on the 64-bit Apple host contract.
- Intentional ABI changes must update Rust layout checks, native `_Static_assert` guards, docs, and versioning notes together.

## Edge cases and failure modes
- The test does not launch UIKit, CoreLocation, CoreMotion, Contacts, or AVFoundation.
- Contact ABI structs are Rust-side frozen here even though they are not part of the current native static-assert slice.

## Concurrency and memory behavior
- Tests are single-threaded and use only compile-time source strings plus `std::mem` layout queries.

## Performance notes
- This is measurement harness only. It changes no runtime behavior and prevents future A/B evidence from being invalidated by silent ABI drift.

## Feature flags and cfgs
- No feature-specific branches.

## Testing and benchmarks
- Run with `cargo test --locked -j$(sysctl -n hw.ncpu) -p oxide-platform-ios --test abi_layout_tests`.

## Changelog
- 2026-06-22: expanded iOS camera static-assert retention checks to include perf/contract snapshot ABI guards.
- 2026-06-22: added iOS Rust and Objective-C ABI layout freeze coverage for architecture densification.
