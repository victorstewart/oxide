# platform-apple `src/apple/secure_storage.m`

## Intention and purpose
- Provide the shared native Apple Keychain bridge consumed by `AppleSecureStorage`.
- Remove host-local secure-storage ABI implementations from macOS and avoid fake iOS app-host secure-storage stubs where the real Keychain bridge is available.

## Relation to the rest of the code
- `oxide-platform-apple/src/lib.rs` declares the `oxide_secure_storage_*` ABI that this file exports.
- `oxide-platform-ios/build.rs` compiles this source for iOS platform builds.
- `oxide-host-macos/build.rs` compiles this source into the AppKit host so `MacPlatform::secure_storage()` reaches the same native bridge.

## Entry points list
- `oxide_secure_storage_save(key_ptr, key_len, data_ptr, data_len)`
  Deletes any existing generic-password item for the account key, then stores the supplied bytes under the Oxide Keychain service.
- `oxide_secure_storage_load(key_ptr, key_len, out_data_ptr, out_data_len)`
  Loads a generic-password item, copies non-empty data into a native-owned buffer, and reports missing items through the shared not-found return code.
- `oxide_secure_storage_delete(key_ptr, key_len)`
  Deletes the generic-password item for the account key and treats missing items as the shared not-found code.
- `oxide_secure_storage_free_data(data_ptr, data_len)`
  Releases native-owned load buffers after Rust copies them.

## Logic narrative
- The bridge stores bytes as generic-password Keychain items using `com.oxide.secure-storage` as the shared service name and the Oxide key bytes as the account field.
- Save uses hard replacement semantics by deleting the existing item before adding the new payload.
- Empty payloads are stored and loaded as successful zero-length values rather than missing items.
- Loaded non-empty values are copied into `malloc` buffers because ownership crosses the C ABI into Rust.

## Preconditions and postconditions
- Foundation and Security.framework must be linked by the consuming Apple host.
- `key_ptr` must be non-null with non-zero length.
- `data_ptr` may be null only when `data_len == 0`.
- Non-empty load results must be released with `oxide_secure_storage_free_data`.

## Edge cases and failure modes
- Invalid pointers or empty keys return `errSecParam`.
- Missing load/delete returns the shared positive not-found code accepted by `AppleSecureStorage`.
- Allocation failure during load returns `errSecAllocate`.
- Unexpected non-NSData Keychain results return `errSecInternalComponent`.

## Testing and benchmarks
- Rust-side ABI mapping is covered by `cargo test -p oxide-platform-apple --locked`.
- macOS installed-platform behavior is covered by `cargo test -p oxide-host-macos --features host-testing --tests --locked`, which performs a live Keychain save/load/delete round trip through `MacPlatform::secure_storage()`.
- iOS target linkage is covered by `cargo check -p oxide-platform-ios --target aarch64-apple-ios --locked` and `cargo check -p oxide-host-ios --target aarch64-apple-ios --tests --locked`.

## Changelog
- 2026-05-19: added shared native Apple Keychain bridge and removed macOS host-local plus iOS perf-stub secure-storage ABI definitions.
