# oxide-host-ios `tests/bridge_tests.rs`

## Intention and purpose
- Verify iOS host callback bridge behavior without launching UIKit.
- Keep callback registration, callback fanout, fallback null handling, overlay state, and reduce-motion state covered through exported Rust host APIs.

## Relation to the rest of the code
- Exercises the callback bridge entry points in `oxide-host-ios/src/lib.rs`.
- Complements the macOS host callback tests so both Apple hosts have direct coverage for their callback registries.
- Runs on the local host toolchain and does not require a simulator.

## Entry points list
- `window_resize_callback_invoked()`
  Verifies window resize callback registration and payload forwarding.
- `text_callbacks_forward_payload()`
  Verifies text commit, composition, and selection callback forwarding.
- `push_callbacks_capture_data()`
  Verifies push token and notification callback forwarding.
- `permission_and_input_callbacks_forward_events()`
  Verifies permission, touch, pointer, and key callback forwarding.
- `fallback_emitters_accept_null_empty_payloads()`
  Verifies fallback paths tolerate null pointers when length is zero.
- `ime_callbacks_record_events()`
  Verifies keyboard shown/hidden callback forwarding.
- `overlay_toggle_succeeds_without_router()`
  Verifies overlay state can be toggled without an initialized router.
- `reduce_motion_toggle_succeeds_without_router()`
  Verifies reduce-motion state can be toggled without an initialized router.

## Logic narrative
- The tests install one callback family at a time, emit a host event, assert the recorded payload, then unregister callbacks.
- Static mutexes and atomics hold callback observations because the exported host callbacks use plain C function pointers.
- A process-local test mutex serializes tests that mutate global callback slots and app state.
- The null/empty fallback test intentionally leaves callbacks unregistered and emits zero-length payloads through null pointers, proving fallback logging does not create invalid slices.

## Preconditions and postconditions
- Tests assume callback slots are process-global and therefore serialize access.
- Each callback test unregisters the callbacks it installs.

## Edge cases and failure modes
- Null payload plus zero length is accepted in fallback paths.
- Registered callbacks receive the original raw pointers and lengths so native ownership semantics are unchanged.
- Overlay and reduce-motion tests start from `oxide_host_app_shutdown()` to avoid depending on router initialization.

## Concurrency and memory behavior
- Test observation state is protected by mutexes or atomics.
- Tests do not spawn threads and do not depend on timing.

## Performance notes
- These are callback-contract tests, not renderer or input-latency benchmarks.
- The tested changes do not add allocation to callback-installed input paths.

## Feature flags and cfgs
- Tests run under the host test target for `oxide-host-ios`.

## Testing and benchmarks
- Run with `cargo test -p oxide-host-ios --tests --locked`.

## Examples
```rust
oxide_host_set_perm_callback(Some(perm_cb));
oxide_host_emit_perm(4, 2);
```

## Changelog
- 2026-05-19: added permission/input callback coverage and null/empty fallback payload coverage.
