# platform-apple `src/apple/bluetooth.m`

## Intention and purpose
- Provide the shared native CoreBluetooth bridge used by Oxide Apple hosts.
- Keep scan, connection, GATT, notification, restoration, and permission delivery behind one Apple-family Objective-C implementation.

## Relation to the rest of the code
- `oxide-platform-apple/src/lib.rs` declares the Rust-facing Bluetooth ABI and callback surface.
- `oxide-platform-ios/build.rs` compiles this file for iOS builds, while Apple-family hosts consume the same scan/result struct shapes.
- `tests/abi_layout_tests.rs` checks that the native scan config/result structs keep the same layout as the Rust `AppleBleScanConfig` and `AppleBleScanInfo` mirrors.

## Entry points list
- `oxide_ble_start_scan(config)`
  Starts CoreBluetooth scanning with optional service filters and duplicate delivery policy.
- `oxide_ble_stop_scan()`
  Stops the active CoreBluetooth scan if one is running.
- `oxide_ble_connect(id)` / `oxide_ble_disconnect(id)`
  Connect and disconnect a cached peripheral by stable UUID bytes.
- `oxide_ble_read`, `oxide_ble_write`, and `oxide_ble_set_notify`
  Bridge GATT characteristic operations through the shared manager/delegate state.
- `oxide_host_ble_emit_*`
  Weak host callbacks used to forward state, discovery, restore, connection, disconnection, and notification events back into Rust.

## Logic narrative
- The bridge keeps CoreBluetooth manager and delegate state on a serial queue so native callbacks and Rust requests observe one ordered peripheral cache.
- Discovery results are copied into `OxideBleScanInfo` before callback delivery because Rust must not depend on Objective-C object lifetimes after the native delegate returns.
- `_Static_assert` guards freeze the C layout of `OxideBleScanConfig` and `OxideBleScanInfo`; this makes drift visible at native compile time instead of silently corrupting Rust callback decoding.

## Preconditions and postconditions
- Hosts must link CoreBluetooth and install the Rust callback symbols or tolerate weak callbacks being absent.
- Callers must pass valid UUID/service/data pointers when their paired lengths are non-zero.
- Scan result callbacks provide borrowed native pointers only for the duration of callback decoding.

## Edge cases and failure modes
- Missing or malformed UUID bytes are rejected before CoreBluetooth object lookup.
- Permission updates are forwarded to the Oxide host callback when present and to the legacy iOS callback only when that weak symbol exists.
- Cached peripherals are bounded by discovery/update flow and may be absent after process restart unless restored by CoreBluetooth.

## Concurrency and memory behavior
- CoreBluetooth work is serialized through `bluetooth_queue()`.
- The scan config/result ABI structs are stack/local callback payloads and do not allocate by themselves.
- Objective-C containers may allocate while translating service UUID and advertisement data outside Rust hot loops.

## Performance notes
- The 2026-06-22 change is measurement harness only: it adds compile-time ABI guards and no runtime branches or allocations.
- Future Bluetooth bridge cleanups still need same-workload A/B proof before deleting or retaining behavior changes.

## Feature flags and cfgs
- iOS builds can also call the legacy weak permission symbol; non-iOS Apple builds skip that path with `TARGET_OS_IPHONE`.

## Testing and benchmarks
- ABI layout and static-assert retention are covered by `cargo test --locked -j$(sysctl -n hw.ncpu) -p oxide-platform-apple --test abi_layout_tests`.

## Changelog
- 2026-06-22: documented and froze the native Bluetooth scan config/result ABI layout with `_Static_assert` guards.
