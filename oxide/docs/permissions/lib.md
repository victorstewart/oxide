# permissions `lib.rs`

## Intention and purpose
- Own the Rust permission manager that wraps a platform `Permissions` service with cached status reads and app-level subscriptions.
- Keep permission state in Oxide-owned data structures so app code can query status and receive updates without retaining platform objects.
- Preserve bridge performance by avoiding hash lookups in the fixed-domain status cache while leaving listener fanout behavior explicit.

## Relation to the rest of the code
- `oxide_platform_api` defines `PermissionDomain`, `PermissionStatus`, and the platform `Permissions` trait.
- `oxide_permissions::sensors::SensorBridge` binds to `PermissionManager` for location, motion, Bluetooth, and notifications state.
- `oxide-perf-runner` measures manager update/read overhead through `cpu.bridge.permission_callback_fanout`.

## Entry points list
- `oxide_permissions::PermissionState`
  Copyable public state record for one domain, status, and timestamp.
- `oxide_permissions::PermissionState::new(domain, status, timestamp_ms) -> Self`
  Constructs a state record for manager and bridge updates.
- `oxide_permissions::PermissionManager::new(permissions, clock) -> Self`
  Wraps a platform permission service with an explicit clock.
- `oxide_permissions::PermissionManager::with_default_clock(permissions) -> Self`
  Wraps a platform permission service with wall-clock timestamps.
- `oxide_permissions::PermissionManager::status(domain) -> PermissionStatus`
  Returns cached state or queries the platform and caches the result.
- `oxide_permissions::PermissionManager::request(domain)`
  Forwards a permission request to the platform service.
- `oxide_permissions::PermissionManager::snapshot() -> Vec<PermissionState>`
  Returns cached permission states as owned values.
- `oxide_permissions::PermissionManager::subscribe(domain, callback) -> PermissionSubscription`
  Registers a domain listener, immediately emits cached state when present, and unregisters on drop.

## Logic narrative
- The manager subscribes once to the platform permission service during construction.
- Platform updates create a `PermissionState`, write it into a fixed slot for the domain, clone listeners for that domain, drop the manager lock, and invoke callbacks in registration order.
- `status` first checks the fixed-slot cache. On a miss it queries the platform service, timestamps the result, stores it, and returns the status.
- `snapshot` materializes the occupied fixed slots into an owned vector.
- Subscriptions call `status` first so listeners receive the current cached state immediately when one exists.

## Preconditions and postconditions
- Callers provide a platform `Permissions` implementation and clock.
- Cached status reads return the latest state observed through `status` or platform notification.
- Dropping `PermissionSubscription` removes the listener for future updates.
- Callbacks are invoked after the manager lock is released.

## Edge cases and failure modes
- Domains with no cached state query the platform service on first `status` call.
- Snapshot order follows fixed domain slot order and should not be treated as a semantic contract.
- Listener storage remains a map of per-domain vectors because listener counts are sparse and callback lists are not the optimized status-cache path.

## Concurrency and memory behavior
- The manager holds one `parking_lot::Mutex` around state and listener tables.
- Status state uses `[Option<PermissionState>; 8]`, one slot for each platform permission domain.
- Update fanout still clones listener `Arc`s into a temporary vector so callbacks run outside the lock.

## Performance notes
- Same-workload A/B for `cpu.bridge.permission_callback_fanout` proved the fixed-slot manager status cache: baseline median/p95/p99 `0.075`/`0.212`/`0.226 us/op` and `0.075`/`0.183`/`0.206 us/op`; after median/p95/p99 `0.071`/`0.168`/`0.195 us/op` and `0.070`/`0.179`/`0.201 us/op`.
- The listener fanout path is intentionally unchanged; a prior inline-slot callback staging attempt was rejected by same-workload A/B.

## Feature flags and cfgs
- The module is target-agnostic Rust and has no feature-gated behavior.

## Testing and benchmarks
- `crates/permissions/tests/manager_tests.rs` covers status updates, all-domain slot mapping, snapshot contents, subscription initial callbacks, update callbacks, and unsubscribe behavior.
- `cpu.bridge.permission_callback_fanout` in `oxide-perf-runner` covers the permission manager update/read hot path.

## Examples
```rust
let manager = oxide_permissions::PermissionManager::with_default_clock(platform.permissions());
let sub = manager.subscribe(oxide_platform_api::PermissionDomain::Camera, |state| {
   let _ = state.status;
});
drop(sub);
```

## Changelog
- 2026-06-23: replaced the manager status `HashMap` with fixed domain slots after same-workload A/B proved lower permission callback fanout latency with preserved manager semantics.
