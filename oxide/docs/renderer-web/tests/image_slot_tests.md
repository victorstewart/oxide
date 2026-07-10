# renderer-web::tests::image_slot_tests

## Intention and purpose

These external tests execute the exact production `ImageSlots` source on the native test target so
resource-lifetime edge cases do not depend on source-string assertions or browser GPU availability.

## Relation to the rest of the code

The test crate includes `src/wasm/image_slots.rs` by path. WebGPU uses the same file for image
creation, update, draw, bind, and release handle validation.

## Entry points list

- `released_and_malformed_handles_never_resolve_after_slot_reuse()` covers invalid packed fields,
  exact release, reuse, and stale lookup.
- `double_release_cannot_duplicate_a_free_slot()` proves idempotent release and unique allocation.
- `generation_exhaustion_retires_instead_of_wrapping_a_slot()` executes every generation.
- `live_capacity_is_hard_bounded_and_recoverable_after_release()` fills all 65,535 live slots.
- `repeated_churn_keeps_resource_table_capacity_at_warm_peak()` proves bounded warm metadata.

## Logic narrative

Tests use small scalar values in place of GPU objects, then exercise the production table's packed
handles and ownership transitions. This isolates the lifetime algorithm while WebGPU compilation
and browser pixel/performance runs verify backend integration.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

Every test starts with a fresh table. Returned values and handles are asserted exactly. No unsafe
code, timing sleep, network, browser, or filesystem state is used.

## Edge cases and failure modes

Coverage includes zero, zero-slot, zero-generation, stale, double-release, generation overflow,
full capacity, post-release recovery, and sustained churn.

## Concurrency and memory behavior

Tests run independent table instances. The largest case holds 65,535 `u16` values and packed
handles, matching the production live-slot limit without GPU allocation.

## Performance notes

The churn test asserts capacity stability rather than wall-clock timing. Browser A/B evidence is
used for draw-path latency because native scalar timing is not representative of WebGPU.

## Feature flags and cfgs

The tests run on the native target and include the backend-neutral slot source directly.

## Testing and benchmarks

Run `cargo test --locked -p oxide-renderer-web --test image_slot_tests`.

## Examples

```text
cargo test --locked -p oxide-renderer-web --test image_slot_tests
```

## Changelog

- 2026-07-10: added executable stale-handle, retirement, capacity, and churn coverage.
