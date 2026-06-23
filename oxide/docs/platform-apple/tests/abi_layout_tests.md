# platform-apple tests `abi_layout_tests.rs`

## Intention and purpose
- Freeze shared Apple `#[repr(C)]` ABI sizes and alignments before generated ABI work starts.
- Verify native Objective-C bridge files keep matching `_Static_assert` guards beside their mirrored C structs.

## Relation to the rest of the code
- Protects `oxide-platform-apple` Rust wrappers and the Objective-C HTTP/Bluetooth bridge structs they call through.
- Complements platform-iOS and host-iOS ABI layout tests for iOS-only camera, location, motion, contact, and host camera typedefs.

## Entry points list
- `shared_apple_ffi_layouts_are_frozen`
  Checks Rust-side sizes and alignments for HTTP, Bluetooth, camera, location, motion, media, and camera-capability ABI structs.
- `shared_apple_objc_bridges_keep_abi_static_asserts`
  Checks the Objective-C HTTP and Bluetooth bridge sources retain matching `_Static_assert` size/alignment guards.

## Logic narrative
- The test uses `std::mem::size_of` and `std::mem::align_of` on the Rust types because those values are the host ABI contract consumed by native code.
- The source checks keep Apple-side assertions visible in the compiled bridge files, so iOS/macOS native builds fail if the C mirror drifts.

## Preconditions and postconditions
- Passing tests mean the currently frozen shared Apple ABI shapes remain unchanged on the 64-bit Apple host contract.
- Intentional ABI changes must update the Rust layout test, Objective-C static assertions, and schema/versioning notes together.

## Edge cases and failure modes
- The test does not call OS services; it only validates ABI shape and source guards.

## Concurrency and memory behavior
- Tests are single-threaded and do not allocate beyond source strings compiled into the test binary.

## Performance notes
- This is measurement harness only. It changes no runtime behavior and makes future A/B evidence harder to invalidate through silent ABI drift.

## Feature flags and cfgs
- No feature-specific branches.

## Testing and benchmarks
- Run with `cargo test --locked -j$(sysctl -n hw.ncpu) -p oxide-platform-apple --test abi_layout_tests`.

## Changelog
- 2026-06-22: added shared Apple Rust and Objective-C ABI layout freeze coverage for architecture densification.
