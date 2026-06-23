# permissions `tests/sensor_bridge_tests.rs`

## Intention and purpose
- Verify that `SensorBridge` applies permission gates, bounded history pruning, and snapshot materialization correctly.
- Protect bridge semantics while performance work removes unnecessary payload copies.

## Relation to the rest of the code
- Tests exercise the public `oxide_permissions::SensorBridge` API and `oxide_platform_api` event payloads.
- The Bluetooth discovery preservation test covers the same event family measured by `cpu.bridge.bluetooth_cache_update`.

## Entry points list
- `location_history_prunes_by_length_and_age()`
  Verifies location history length and age pruning.
- `permission_status_tracks_every_domain_slot()`
  Verifies all eight permission domains preserve independent cached statuses.
- `location_events_ignored_without_permission()`
  Verifies unauthorized location events do not affect snapshots.
- `motion_history_clears_on_revocation()`
  Verifies motion state clears when motion permission is denied.
- `bluetooth_prunes_by_age_limit()`
  Verifies Bluetooth cache age pruning.
- `bluetooth_discovery_snapshot_preserves_payload_fields()`
  Verifies discovered peripherals keep id, name, RSSI, services, manufacturer data, connectability, and last-seen time after insertion.
- `push_notifications_tracked_when_authorized()`
  Verifies authorized push token and notification tracking plus revocation clearing.

## Logic narrative
- Tests use a deterministic atomic clock so pruning and last-seen behavior do not depend on wall time.
- The `permit` helper feeds public permission states into the bridge before each authorized event path.
- The permission slot test writes every platform permission domain because the bridge cache is optimized as a fixed-size array rather than a hash map.
- The Bluetooth discovery preservation test constructs a payload with allocated name, service vector, and manufacturer-data vector because those were the fields previously copied by the cache insert path.

## Preconditions and postconditions
- Each test constructs an isolated bridge.
- Tests set permission state explicitly before expecting event ingestion.
- Permission status reads must return the exact last status written for every domain.
- After a discovery event, snapshots must expose the full peripheral payload and the clock value used as `last_seen_ms`.

## Edge cases and failure modes
- Unauthorized location events are ignored.
- Permission revocation clears state for the revoked domain.
- Bluetooth cache age pruning removes stale entries even when the original discovery payload was valid.

## Concurrency and memory behavior
- The deterministic clock uses `Arc<AtomicU64>`.
- Tests assert through owned snapshot values, matching production consumers and avoiding lock borrowing in assertions.

## Performance notes
- The Bluetooth payload preservation test guards the move-into-cache optimization so future changes cannot reduce work by dropping name or advertisement fields.
- The permission slot test guards the fixed-slot cache optimization so future changes cannot reduce work by aliasing two domains.
- The measured A/B cases for the optimized paths are `cpu.bridge.sensor_location_snapshot` and `cpu.bridge.bluetooth_cache_update`.

## Feature flags and cfgs
- Tests are host-only Rust tests with no feature-gated behavior.

## Testing and benchmarks
- Run with `cargo test --locked -j$(sysctl -n hw.ncpu) -p oxide-permissions --test sensor_bridge_tests`.
- The related benchmark is `OXIDE_PERF_RUNNER_FILTER=cpu.bridge.sensor_location_snapshot,cpu.bridge.bluetooth_cache_update cargo run --release --locked -j$(sysctl -n hw.ncpu) -p oxide-perf-runner -- --run-suite`.

## Examples
```rust
let service = oxide_platform_api::BleUuid([9; 16]);
let snapshot = bridge.bluetooth_snapshot();
assert_eq!(snapshot.devices[0].peripheral.advertisement.services.as_slice(), &[service]);
```

## Changelog
- 2026-06-23: added all-domain permission slot coverage for the fixed-slot bridge permission cache optimization.
- 2026-06-23: added Bluetooth discovery payload preservation coverage for the cache move optimization.
