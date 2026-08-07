# oxide-host-ios `tests/production_shell_tests.rs`

## Intention and purpose

Prevent the legacy iOS selector, benchmark, and camera host from entering the default production Objective-C artifact.

## Relation to the rest of the code

- Source-checks `build.rs` feature selection.
- Source-checks `src/ios/product_app.m` boundaries and bounded size.
- Complements ARM64 target checks and later physical-device validation.

## Entry points list

- `build_selects_exactly_one_objective_c_host_by_feature()` checks source selection and benchmark-stub ownership.
- `product_host_is_a_bounded_injection_shell()` rejects legacy UI, benchmark code, and positive accessibility surfaces.
- Raw-event, camera-lifecycle, environment-counter, display-link, and single-scene tests freeze the remaining native boundary contracts.
- `display_link_observation_is_unavailable_off_ios()` checks the public Rust fallback without UIKit.
- `tokio_spawn_api_and_runtime_installation_share_one_host_feature()` freezes explicit Tokio forwarding and spawn-hook ownership.
- `legacy_rust_exports_and_state_are_feature_owned()` rejects legacy dependencies and state from the default artifact.
- Submit/lifecycle/shutdown tests preserve injected-app frame ownership while the legacy fields are absent.

## Logic narrative

The tests require `build.rs` to pass exactly one selected source into `cc::Build`. They reject legacy UI, dependencies, benchmark identifiers, resource loading, exports, and state from the default product artifact while requiring the raw input, text/IME, lifecycle, frame scheduling, and shell-owned platform hooks a real injected app needs. The only permitted accessibility references are explicit negative assignments on the Metal root and hidden text adapter. The shell must claim one scene before creating its window, reject concurrent sessions, publish display-link range through an atomic snapshot, and wait for actual native startup metrics. Key end/cancel phases cannot masquerade as repeat events, and the camera publication wake must be installed and cleared with foreground lifecycle. Tokio spawn support remains one explicit additive feature shared by host and platform provider.

## Preconditions and postconditions

The default native build selects the product shell. `test-scenes-entrypoint` selects `app.m` and is the only route by which `perf-host-stubs` may be compiled.

## Edge cases and failure modes

The source gates fail if a second window is created before scene ownership, if native timestamps are replaced with synthesized host time, or if camera callback ownership survives foreground teardown.

## Concurrency and memory behavior

The tests read immutable source. Runtime cross-thread state is represented by atomics and the shell performs no test-only allocation or instrumentation.

## Performance notes

The gates preserve late drawable acquisition, idle display-link pausing, and lock-free camera publication wakes. They do not substitute for device A/B evidence.

## Feature flags and cfgs

`test-scenes-entrypoint` controls Objective-C source selection, legacy Rust dependencies, state, and exports. `tokio-runtime` independently forwards to the platform provider.

## Testing and benchmarks

Run `cargo test --locked -p oxide-host-ios --test production_shell_tests`.

## Examples

The build contract contains `.file(app_source)` exactly once.

## Changelog

- 2026-08-06: froze single-scene ownership, explicit accessibility opt-out, atomic frame-range observation, exact native timestamps, environment transition counters, and foreground camera callback lifecycle.
- 2026-08-06: froze legacy Rust dependency/export/state ownership and explicit Tokio forwarding.
- 2026-08-06: added production Objective-C artifact selection guards.
