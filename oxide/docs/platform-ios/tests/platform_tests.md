# platform-ios `tests/platform_tests.rs`

## Intention and purpose

These integration tests verify the production `IosPlatform` behavior on the native build host without launching UIKit or contacting a network.

## Relation to the rest of the code

- Links the public `oxide-platform-ios` Rust API.
- Supplies deterministic C-ABI stubs for only the host symbols exercised by each test.
- Complements host/device tests, which must verify the real UIKit definitions and application lifecycle.

## Entry points list

- `host_controls_and_device_caps_forward_without_policy_in_uikit()`: verifies exact redraw, refresh, idle, settings, URL, IME, device-capability, simulation, and haptic forwarding.
- `native_external_url_requires_synchronous_openability_check()`: freezes the native main-thread `canOpenURL` gate and its ordering before `openURL`.
- `clipboard_preserves_empty_and_nonempty_utf8()`: verifies native allocation ownership and the distinction between failure and a successful empty clipboard value.
- `standard_paths_use_host_owned_sandbox_directories()`: verifies all `StandardPath` variants map to the correct host kind and returned path.
- `native_standard_paths_append_the_oxide_directory_before_creation()`: verifies the Objective-C bridge appends and creates the promised app-owned directory instead of returning a bare sandbox root.
- `telephony_reports_no_home_country_without_a_supported_ios_source()`: verifies the service returns `None` instead of binding removed carrier-country APIs or substituting locale.
- `network_status_tracks_reachability_and_notifies_subscribers()`: verifies lazy monitor start, Wi-Fi/cellular/offline mapping, initial subscription delivery, and subsequent fanout.

## Logic narrative

The test binary exports deterministic host symbols with the same C ABI as the iOS shell. Atomic counters capture scalar control calls, mutexes own variable-size clipboard and callback state, and malloc-backed byte copies exercise the production free contract. Reachability emits path updates through the real public Rust trampoline and manager rather than bypassing that layer.

## Preconditions and postconditions

- Stub signatures must remain identical to the declarations in `platform.rs`.
- Every returned host buffer is either null with zero length or malloc-owned and released through `oxide_host_string_free`.
- A passing suite means the Rust aggregate maps values and service events correctly; it does not prove UIKit behavior on a device.

## Edge cases and failure modes

- Clipboard coverage includes read failure, zero-length success, and non-ASCII UTF-8 values.
- URL coverage includes Rust-side success/rejection mapping, empty-input rejection before FFI, and a source contract requiring synchronous native openability validation before scheduling.
- Reachability coverage includes two known interfaces, an unknown connected interface, and the disconnected state.
- Device coverage forces a non-finite native scale and verifies the deterministic `1.0` fallback.

## Concurrency and memory behavior

- Atomics make scalar observations safe under the test harness's parallel execution.
- Mutexes serialize owned test buffers and callback pointers.
- The reachability callback is copied out of its mutex before invocation so the callback can re-enter Rust without deadlocking the stub.
- Tests perform no live network access and do not modify the filesystem.

## Performance notes

The suite executes bounded in-memory operations. It is correctness coverage and contributes no persisted benchmark sample.

## Feature flags and cfgs

The tests use the crate's default feature set and run on the native Apple build host. iOS linking and runtime behavior remain separate host/device checks.

## Testing and benchmarks

```bash
cargo test --locked -p oxide-platform-ios --test platform_tests
```

## Examples

The host-control test demonstrates direct use of the public trait:

```rust
use oxide_platform_api::Platform;

let platform = oxide_platform_ios::IosPlatform::new();
platform.request_redraw();
```

## Changelog

- 2026-08-06: added native-source coverage for the app-owned standard-path suffix and creation order.
- 2026-08-06: added native source-contract coverage for the synchronous `canOpenURL` gate.
- 2026-08-06: added explicit no-home-country coverage for current iOS telephony behavior.
- 2026-08-06: added native-linkable coverage for the production iOS platform aggregate.
