# platform-ios `tests/media_library_tests.rs`

## Intention and purpose
- Verify that the display-image BGRA helper still routes through the shared full-image RGBA bridge when the optional cached-image variant is not part of the Rust-side contract.

## Relation to the rest of the code
- Exercises [`IosMediaLibraryManager`](/Users/victorstewart/oxide/oxide/crates/platform-ios/src/lib.rs:2520) from the `platform-ios` crate.
- Stubs the media-library FFI exported by host-side Objective-C glue so the Rust test can validate the fallback path without iOS runtime dependencies.

## Entry points list
- `display_image_loader_reuses_full_rgba_loader_until_cached_variant_exists()`
  Confirms that `load_display_image_bgra_data_if_available` succeeds through `oxide_media_load_full_image_rgba` and returns the bridged pixel buffer.

## Logic narrative
- Reset the shared call counter for the stubbed full-image loader.
- Expose a minimal `oxide_media_load_full_image_rgba` test symbol that returns one BGRA pixel and metadata.
- Call the display-image helper and assert that the loader ran exactly once and that the returned dimensions and bytes match the stub payload.

## Preconditions and postconditions
- The Rust media-library helper must continue importing only the full-image RGBA bridge for this fallback path.
- The test must leave no leaked image buffer after `oxide_media_free_image_data` runs.

## Edge cases and failure modes
- A missing or renamed full-image bridge symbol would fail the test at link time or panic before the assertions.
- Incorrect buffer dimensions or bytes fail the final equality checks.

## Concurrency and memory behavior
- Uses a single `AtomicUsize` counter to observe the stub call without shared mutable test state races.
- The stub transfers ownership of the temporary byte buffer to the code under test and the free stub reconstructs and drops that allocation.

## Performance notes
- This is a tiny unit-style bridge test with one heap allocation for the sample pixel buffer.

## Feature flags and cfgs
- No special feature gating beyond whatever `platform-ios` enables for its default test build.

## Testing and benchmarks
- Run with `cargo test --manifest-path oxide/Cargo.toml -p oxide-platform-ios --locked --test media_library_tests -j$(sysctl -n hw.ncpu)`.

## Examples
- The test itself is the minimal usage example for the display-image BGRA fallback path.

## Changelog
- 2026-04-22: removed the obsolete cached-image stub symbol from the test because the Rust bridge no longer imports it.
