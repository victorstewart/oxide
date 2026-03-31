# ui-core tests `emitter_tests.rs`

## Intention and purpose
- Verify that the shared CAEmitter-style burst sampler preserves the timing, capacity, source-volume, and angular-spread rules that downstream app particle effects depend on.

## Relation to the rest of the code
- Exercises `ui-core::emitter`.
- Serves as the framework-level regression suite for the Nametag Radar ghost burst contract.

## Entry points list
- `burst_emitter_matches_legacy_ghost_capacity_and_tail_window`
- `burst_emitter_particles_stay_inside_legacy_spherical_source_volume_at_birth`
- `burst_emitter_uses_full_range_legacy_emission_angles`

## Logic narrative
- The first test locks the legacy emission window, total emitted capacity, and cleanup boundary after the visible tail.
- The second test verifies that sampled particles are born inside the configured spherical source volume and inherit the expected sprite scale.
- The third test verifies that deterministic angle sampling still spans the full configured emission range.

## Preconditions and postconditions
- Tests build the helper only through the public API.
- Passing results mean downstream apps can trust the shared helper for both lifetime cleanup and particle placement.

## Edge cases and failure modes
- Visible-tail expiration, source-boundary sampling, and angle spread are all covered directly.

## Concurrency and memory behavior
- Tests are single-threaded and deterministic.

## Performance notes
- This suite targets correctness, not benchmarking.

## Feature flags and cfgs
- No feature-specific branches.

## Testing and benchmarks
- Run with `cargo test --locked -p oxide-ui-core --test emitter_tests`.

## Examples
```rust
cargo test --locked -p oxide-ui-core --test emitter_tests
```

## Changelog
- 2026-03-26: added regression coverage for the shared CAEmitter-style burst sampler.
