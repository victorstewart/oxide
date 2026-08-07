# oxide-host-ios `tests/drawable_tests.rs`

## Intention and purpose

Protect late drawable acquisition and prepared-frame ownership in the iOS product and benchmark hosts.

## Relation to the rest of the code

- Reads production `src/ios/product_app.m`, legacy `src/ios/app.m`, the Swift benchmark runtime, and Rust `src/lib.rs` as static host contracts.
- Complements fresh external A/B measurement of the real native host preparation path without adding a benchmark dependency or toggle to the product host.

## Entry points list

- `frame_with_drawable_stub()` verifies the uninitialized status code.
- `ios_tick_prepares_frame_before_acquiring_drawable()` and `ios_perf_runtime_prepares_frame_before_acquiring_drawable()` verify prepare/acquire/submit ordering and cancellation.
- `ios_metal_layer_uses_timeout_capable_drawable_acquisition()` verifies timeout support.
- `native_frame_coalescing_reuses_app_storage()` verifies app-owned command storage survives host coalescing without a duplicate frame allocation.
- `native_damage_handoff_reuses_router_and_submit_storage()` verifies router damage and submit scratch remain reusable across native frames.
- `injected_shell_is_full_screen_and_bypasses_test_chrome()` protects the pure production shell and permits only explicit negative accessibility assignments.
- `raw_touch_and_display_link_timestamps_preserve_os_samples()` protects exact OS timing.
- `injected_frame_demand_is_acknowledged_only_after_submit()` protects retry and wake-generation semantics, including rejection and drawable cancellation before a backpressure-skipped frame can emit observational submit feedback.
- `memory_warnings_purge_effect_targets_and_request_a_frame()` requires critical pressure to purge effect targets, retained layers, prepared chunks, and immutable ID-mask fields before requesting a rebuild.
- The remaining tests protect parked benchmark launch routing and foreground execution.

## Logic narrative

Static checks are used for Objective-C/Swift ordering because drawable timeout pressure is nondeterministic in libtest. Rust-source checks ensure the frame loop calls the caller-storage variants, recovers the damage vector after `begin_frame`, and keeps renderer-cache recovery in Rust.

## Preconditions and postconditions

The native entry-point names and source locations must remain stable. Passing preserves late acquisition, cancellation, and reusable frame storage.

## Edge cases and failure modes

The suite rejects early drawable acquisition, blocking timeout policy, allocating convenience helpers, test chrome in the production branch, or acknowledging a renderer backpressure skip as presentation. Injected submit-failure and cancellation retry ownership is covered by `production_shell_tests.rs` and `tests/unit/internal_injected_app.rs`.

## Concurrency and memory behavior

These tests inspect immutable source text and do not start UIKit or share mutable native host state.

## Performance notes

Source gates prove ownership; allocation and latency evidence comes from the external device harness so no measurement allocator enters the host package graph. Memory-pressure purges add no ordinary frame work.

## Feature flags and cfgs

No feature flag is required.

## Testing and benchmarks

Run `cargo test --locked -p oxide-host-ios --test drawable_tests`.

## Examples

```rust
let source = include_str!("../../src/lib.rs");
assert!(source.contains("coalesce_adjacent_draws_reuse"));
```

## Changelog

- 2026-08-07: aligned the entry list with the current coalescing/damage tests and moved retry-policy ownership to its actual production-shell and injected-app regressions.
- 2026-08-06: preserved production renderer-cache purging across the injected/legacy host split.
- 2026-08-06: Rejected backpressure-skipped frames before encode/submit and froze exact prepared-frame retry plus no-feedback semantics.
- 2026-08-06: Added injected-shell, OS timestamp, wake acknowledgement, and deployment-target gates.
- 2026-08-02: Added command/damage reuse and clear-on-cancel ownership gates.
