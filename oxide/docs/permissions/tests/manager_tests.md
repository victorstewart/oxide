# permissions `tests/manager_tests.rs`

## Intention and purpose
- Verify that `PermissionManager` caches platform permission state, materializes snapshots, and dispatches app subscriptions correctly.
- Protect manager semantics while performance work removes unnecessary status-cache hash lookups.

## Relation to the rest of the code
- Tests exercise the public `oxide_permissions::PermissionManager` API against a fake `oxide_platform_api::Permissions` service.
- The all-domain slot test covers the same fixed-domain cache shape measured by `cpu.bridge.permission_callback_fanout`.

## Entry points list
- `manager_tracks_status_updates()`
  Verifies first status reads query the platform and later platform notifications update the manager cache.
- `manager_cache_tracks_every_domain_slot_and_snapshot()`
  Verifies all eight permission domains preserve independent cached states and appear in snapshots.
- `subscription_receives_initial_and_updates()`
  Verifies subscriptions receive initial cached state, later updates, and no updates after drop.

## Logic narrative
- `FakePermissions` owns mutable permission statuses and a subscriber list.
- Tests construct a manager with a deterministic clock so cached `last_changed_ms` values can be asserted.
- Platform notifications flow through the manager's platform subscription path, matching production update behavior.

## Preconditions and postconditions
- Each test constructs an isolated fake platform service and manager.
- Status calls must return the current cached or platform-backed status for the requested domain.
- Snapshot results must contain one state per domain after all eight domains have been notified.
- Dropping a subscription must remove its callback from future updates.

## Edge cases and failure modes
- First reads populate the cache from the platform service.
- Initial subscription callbacks are emitted from cached state.
- Unsubscribe behavior is checked after a follow-up platform notification.

## Concurrency and memory behavior
- The fake platform and assertions use `parking_lot::Mutex`.
- Tests assert through public owned values and do not borrow manager internals.

## Performance notes
- The all-domain slot test guards the fixed-slot status-cache optimization so future changes cannot reduce work by aliasing two permission domains.
- The measured A/B case for the optimized path is `cpu.bridge.permission_callback_fanout`.

## Feature flags and cfgs
- Tests are host-only Rust tests with no feature-gated behavior.

## Testing and benchmarks
- Run with `cargo test --locked -j$(sysctl -n hw.ncpu) -p oxide-permissions --test manager_tests`.
- The related benchmark is `OXIDE_PERF_RUNNER_FILTER=cpu.bridge.permission_callback_fanout cargo run --release --locked -j$(sysctl -n hw.ncpu) -p oxide-perf-runner -- --run-suite`.

## Examples
```rust
fake.notify(PermissionDomain::Camera, PermissionStatus::Authorized);
assert_eq!(mgr.status(PermissionDomain::Camera), PermissionStatus::Authorized);
```

## Changelog
- 2026-06-23: added all-domain manager cache and unsubscribe coverage for the fixed-slot permission manager status cache optimization.
