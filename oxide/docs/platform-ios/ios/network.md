# platform-ios `src/ios/network.m`

## Intention and purpose
- Provide the native Network.framework bridge used by the iOS platform crate for transport and reachability services.

## Relation to the rest of the code
- `oxide-platform-ios/src/lib.rs` calls the exported `oxide_host_net_*` symbols from this file.
- Reachability snapshots are decoded in Rust through `oxide-platform-apple` so iOS and macOS share path-kind semantics.
- HTTP symbols are now compiled from `oxide-platform-apple/src/apple/http.m` so iOS and macOS share the native `NSURLSession` bridge.

## Entry points list
- `oxide_host_net_set_reachability_callback(cb)`
  Installs the Rust reachability callback.
- `oxide_host_net_start_reachability()`
  Starts a persistent `nw_path_monitor_t` and emits the latest snapshot.
- `oxide_host_net_stop_reachability()`
  Stops and releases the monitor.
- `path_kind_for_path(path)`
  Maps active Network.framework interfaces to Oxide's Apple path-kind constants.

## Logic narrative
- The monitor updates cached reachability fields from its serial queue.
- Every path update emits connected/offline state, the preferred path kind, and whether the path is expensive.
- Wired Ethernet is detected directly through `nw_interface_type_wired`, matching the current SDK enum rather than using preprocessor checks that do not apply to enum values.

## Preconditions and postconditions
- Network.framework must be available and linked by the platform-iOS build.
- Rust callbacks must remain ABI-compatible with `(uint32_t status, uint32_t iface, uint8_t expensive)`.

## Edge cases and failure modes
- Unknown interface types map to the `Other` path kind.
- A stopped monitor releases native ownership and future starts allocate a fresh monitor.

## Testing and benchmarks
- Compiled by `cargo check -p oxide-platform-ios --locked`.
- Transport retry invariants are covered by `crates/platform-ios/tests/network_bridge_tests.rs`.

## Changelog
- 2026-05-19: moved the native HTTP `NSURLSession` bridge out to `oxide-platform-apple/src/apple/http.m`.
- 2026-05-19: wired reachability decoding now checks `nw_interface_type_wired` directly and shares Rust-side path decoding with `oxide-platform-apple`.
