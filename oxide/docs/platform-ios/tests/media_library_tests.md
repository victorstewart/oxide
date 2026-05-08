# platform-ios `tests/media_library_tests.rs`

## Intention and purpose
- Verify that the display-image BGRA helper still routes through the shared full-image RGBA bridge when the optional cached-image variant is not part of the Rust-side contract.
- Verify that explicit Photos permission status reads refresh current authorization while boot-time Nametag sync stays lazy and the legacy Nametag bridge keeps its separate Photos status mapping.

## Relation to the rest of the code
- Exercises [`IosMediaLibraryManager`](/Users/victorstewart/oxide/oxide/crates/platform-ios/src/lib.rs:2520) from the `platform-ios` crate.
- Stubs the media-library FFI exported by host-side Objective-C glue so the Rust test can validate the fallback path without iOS runtime dependencies.

## Entry points list
- `display_image_loader_reuses_full_rgba_loader_until_cached_variant_exists()`
  Confirms that `load_display_image_bgra_data_if_available` succeeds through `oxide_media_load_full_image_rgba` and returns the bridged pixel buffer.
- `media_library_permission_status_refreshes_on_explicit_status_call()`
  Confirms that Oxide and Nametag media-library status reads query current Photos authorization and update the shared cache before returning.
- `nametag_media_library_cache_preserves_legacy_limited_mapping()`
  Confirms that the cache stores separate Oxide and Nametag Photos statuses so iOS limited-library access is not emitted through the legacy bridge as the Oxide limited code.
- `nametag_bootstrap_permission_sync_skips_media_library()`
  Confirms that boot-time Nametag permission sync does not publish media-library status before an explicit request.

## Logic narrative
- Reset the shared call counter for the stubbed full-image loader.
- Expose a minimal `oxide_media_load_full_image_rgba` test symbol that returns one BGRA pixel and metadata.
- Call the display-image helper and assert that the loader ran exactly once and that the returned dimensions and bytes match the stub payload.
- Parse `src/ios/host_services.m` for permission bridge invariants that are otherwise only observable on iOS, keeping the test host-independent while still locking the explicit Photos status refresh, lazy boot sync, and legacy Nametag mapping behavior.
- Reuse a single source loader and marker-slicing helper so each assertion states only the Objective-C function span it needs.

## Preconditions and postconditions
- The Rust media-library helper must continue importing only the full-image RGBA bridge for this fallback path.
- The test must leave no leaked image buffer after `oxide_media_free_image_data` runs.

## Edge cases and failure modes
- A missing or renamed full-image bridge symbol would fail the test at link time or panic before the assertions.
- Incorrect buffer dimensions or bytes fail the final equality checks.
- Returning the initial cached Photos status without an explicit authorization refresh fails the source-level invariant because existing Settings grants or denials would be reported as not determined.
- Reusing the Oxide limited-status cache for Nametag Photos status fails the source-level invariant because legacy Nametag maps limited Photos access to denied.

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
- 2026-05-05: shared the host-services source loader and marker slicing helper across the Objective-C source invariants.
- 2026-05-04: added coverage that explicit Photos status reads refresh current authorization while boot-time Nametag sync remains lazy.
- 2026-05-01: added coverage for separate Oxide and legacy Nametag media-library permission caches after lazy Photos status caching was introduced.
- 2026-04-22: removed the obsolete cached-image stub symbol from the test because the Rust bridge no longer imports it.
