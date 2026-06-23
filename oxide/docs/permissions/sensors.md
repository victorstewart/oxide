# permissions `sensors.rs`

## Intention and purpose
- Own the Rust-side sensor bridge that turns permission-gated platform events into bounded, snapshot-friendly state for location, motion, Bluetooth, and push notifications.
- Keep OS bridge state in Oxide-owned Rust data structures so app scenes and telemetry consumers can read coherent snapshots without retaining platform objects.
- Preserve bridge performance by pruning bounded histories and avoiding avoidable payload copies or hash lookups on hot event paths.

## Relation to the rest of the code
- `oxide_platform_api` defines the platform event and snapshot payload types consumed by this module.
- `oxide_permissions::PermissionManager` feeds permission state into `SensorBridge::bind_permissions`.
- `oxide_ui_core::sensors` and test scenes consume `SensorBridge` snapshots instead of talking to platform services directly.
- `oxide-perf-runner` measures bridge overhead through `cpu.bridge.sensor_location_snapshot` and `cpu.bridge.bluetooth_cache_update`.

## Entry points list
- `oxide_permissions::sensors::SensorBridgeConfig`
  Public configuration for bounded history sizes and age limits.
- `oxide_permissions::sensors::SensorBridge::new_with_config(clock, config) -> Self`
  Builds a bridge with explicit time source and limits; main callers are tests and configured app hosts.
- `oxide_permissions::sensors::SensorBridge::new(clock) -> Self`
  Builds a bridge with default limits; main callers are simple host setup paths.
- `oxide_permissions::sensors::SensorBridge::with_clock(clock) -> Self`
  Builds a default-limit bridge with a deterministic clock; main callers are tests.
- `oxide_permissions::sensors::SensorBridge::with_default_clock() -> Self`
  Builds the production default bridge; main callers are app/runtime setup.
- `oxide_permissions::sensors::SensorBridge::with_config(config) -> Self`
  Builds a configured bridge with the production clock; main callers are benchmark and host setup.
- `oxide_permissions::sensors::SensorBridge::permission_status(domain) -> Option<PermissionStatus>`
  Reads cached permission state for one domain.
- `oxide_permissions::sensors::SensorBridge::update_permission(state)`
  Updates permission state and clears revoked domain state.
- `oxide_permissions::sensors::SensorBridge::bind_permissions(manager) -> SensorPermissionBinding`
  Subscribes the bridge to a permission manager.
- `oxide_permissions::sensors::SensorBridge::handle_location_event(event)`
  Ingests authorized location events.
- `oxide_permissions::sensors::SensorBridge::last_location() -> Option<LocationReading>`
  Returns the most recent location reading.
- `oxide_permissions::sensors::SensorBridge::location_history() -> Vec<LocationReading>`
  Returns bounded location history.
- `oxide_permissions::sensors::SensorBridge::handle_motion_sample(sample)`
  Ingests authorized motion samples.
- `oxide_permissions::sensors::SensorBridge::last_motion() -> Option<MotionSample>`
  Returns the most recent motion sample.
- `oxide_permissions::sensors::SensorBridge::motion_history() -> Vec<MotionSample>`
  Returns bounded motion history.
- `oxide_permissions::sensors::SensorBridge::handle_bluetooth_event(event)`
  Ingests authorized Bluetooth state, discovery, cache, connection, notification, and restoration events.
- `oxide_permissions::sensors::SensorBridge::bluetooth_snapshot() -> BluetoothSnapshot`
  Returns current Bluetooth power state and cached peripherals.
- `oxide_permissions::sensors::SensorBridge::prune_bluetooth()`
  Applies Bluetooth age and capacity limits.
- `oxide_permissions::sensors::SensorBridge::trim_memory()`
  Shrinks histories for memory pressure.
- `oxide_permissions::sensors::SensorBridge::set_push_token(token)`
  Stores or clears an authorized push token.
- `oxide_permissions::sensors::SensorBridge::handle_push_notification(notification)`
  Ingests authorized push notifications.
- `oxide_permissions::sensors::SensorBridge::push_token() -> Option<PushToken>`
  Returns the current push token.
- `oxide_permissions::sensors::SensorBridge::push_notifications() -> Vec<PushNotification>`
  Returns bounded push notification history.
- `oxide_permissions::sensors::SensorBridge::snapshot() -> SensorSnapshot`
  Materializes all sensor families in one snapshot.
- `LocationSnapshot`, `MotionSnapshot`, `BluetoothSnapshot`, `PushSnapshot`, and `SensorSnapshot`
  Public snapshot structs returned by the bridge.

## Logic narrative
- The bridge caches permission status by domain in fixed slots for the eight platform permission domains. Event ingestion first checks that cache so denied domains do not update app-visible state.
- Location and motion samples update a latest value and a bounded history. Location history is also pruned by age because stale coordinates are user-visible correctness risk.
- Bluetooth state uses a `BTreeMap<PeripheralId, BleCacheEntry>` so cache snapshots have deterministic ordering. Discovery events save the id, move the discovered `PeripheralInfo` into the cache entry, and insert by the saved id. This preserves the exact peripheral payload while avoiding the prior clone of name and advertisement vectors.
- Snapshot methods clone or copy state at the boundary so consumers get owned values and never hold bridge locks.
- Push tokens and notifications are cleared when notification permission is revoked.

Call graph:
- platform service event -> `handle_location_event` / `handle_motion_sample` / `handle_bluetooth_event` / `handle_push_notification`
- permission manager -> `update_permission` -> domain state clear on revocation
- app scene or telemetry -> `snapshot` / family snapshot -> owned state values

## Preconditions and postconditions
- Callers must update relevant permission state before expecting events to affect snapshots.
- Authorized events update only their own family state.
- Revoking a permission clears app-visible state for that domain.
- `BluetoothEvent::Discovered` preserves peripheral id, name, RSSI, services, manufacturer data, connectability, and last-seen time in subsequent snapshots.

## Edge cases and failure modes
- Unauthorized events are ignored except Bluetooth powered-off events can clear power state.
- Zero location age disables age pruning.
- Bluetooth pruning removes entries older than `bluetooth_max_age_ms` and then removes oldest entries until `bluetooth_cache_max` is satisfied.
- Non-sensor permission domains are cached but do not clear sensor state.

## Concurrency and memory behavior
- Each sensor family has its own `parking_lot::Mutex`, keeping unrelated updates from sharing one lock.
- Snapshot calls allocate owned `Vec` outputs because app consumers must not borrow bridge internals across frame or thread boundaries.
- The Bluetooth discovery path moves `PeripheralInfo` into the cache entry and avoids a duplicate allocation of the optional name, service vector, and manufacturer-data vector.

## Performance notes
- Location, motion, Bluetooth, and push histories are bounded by configuration.
- Permission status reads use fixed domain slots instead of a `HashMap`, avoiding hashing/probing on every permission-gated sensor event.
- Bluetooth snapshots clone cached entries by design at the API boundary; the optimized discovery insert avoids a clone before that boundary.
- Same-workload A/B for `cpu.bridge.sensor_location_snapshot` and `cpu.bridge.bluetooth_cache_update` proved the fixed-slot permission cache: baseline location median/p95/p99 `0.091`/`0.175`/`0.186 us/op` and `0.092`/`0.168`/`0.199 us/op`, baseline Bluetooth median/p95/p99 `0.823`/`0.860`/`0.861 us/op` and `0.823`/`0.857`/`0.865 us/op`; after location median/p95/p99 `0.087`/`0.124`/`0.137 us/op` and `0.087`/`0.132`/`0.151 us/op`, Bluetooth median/p95/p99 `0.793`/`0.819`/`0.826 us/op` and `0.786`/`0.813`/`0.823 us/op`.
- Same-workload A/B for `cpu.bridge.bluetooth_cache_update` proved the discovery move: baseline median/p95/p99 `1.205`/`2.819`/`3.224 us/op` and `1.193`/`1.954`/`2.130 us/op`; after median/p95/p99 `0.802`/`0.922`/`0.977 us/op` and `0.922`/`1.293`/`1.346 us/op`.

## Feature flags and cfgs
- The module is target-agnostic Rust and has no feature-gated behavior.

## Testing and benchmarks
- `crates/permissions/tests/sensor_bridge_tests.rs` covers all permission-domain cache slots, permission gating, pruning, and Bluetooth discovery snapshot payload preservation.
- `cpu.bridge.sensor_location_snapshot` and `cpu.bridge.bluetooth_cache_update` in `oxide-perf-runner` cover permission-gated sensor bridge hot paths.

## Examples
```rust
let bridge = oxide_permissions::SensorBridge::with_default_clock();
bridge.update_permission(oxide_permissions::PermissionState::new(
   oxide_platform_api::PermissionDomain::Bluetooth,
   oxide_platform_api::PermissionStatus::Authorized,
   0,
));
let snapshot = bridge.bluetooth_snapshot();
```

## Changelog
- 2026-06-23: replaced the bridge-local permission `HashMap` with fixed domain slots after same-workload A/B proved lower location and Bluetooth bridge latency with preserved permission status semantics.
- 2026-06-23: moved discovered Bluetooth peripheral payloads directly into the cache entry after same-workload A/B proved lower `cpu.bridge.bluetooth_cache_update` latency with preserved snapshot payloads.
