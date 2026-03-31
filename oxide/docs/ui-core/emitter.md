# ui-core `emitter.rs`

## Intention and purpose
- Provide a reusable, deterministic CAEmitter-style burst sampler for app scenes that need legacy particle timing and source-shape behavior without owning their own particle math.
- Keep the burst engine renderer-agnostic: Oxide computes particle positions and app crates decide which image or fallback primitive to draw.

## Relation to the rest of the code
- Lives in `ui-core` beside other reusable state and geometry helpers such as `anim.rs` and `picker_popup.rs`.
- Uses `oxide_renderer_api::RectF` for the sampled destination rectangles.
- Re-exported from `ui-core::lib` and consumed by Nametag Radar ghost bursts.

## Entry points list
- `BurstEmitterShape`
  Enumerates the supported source-volume shapes for burst emission.
- `BurstEmitterCellConfig::sanitized(self) -> Self`
  Clamps negative per-particle fields into safe non-negative values before sampling.
- `BurstEmitterConfig::{sanitized, emitter_size, visible_duration_s}`
  Sanitizes layer-level settings, derives the emitter size from the caller’s base sprite side, and reports the full visible lifetime including the post-emission tail.
- `BurstEmitter::{new, config, started_ms, seed, emission_end_ms, visible_end_ms, emitted_particle_capacity, spawned_particle_count, particles, particle, spawn_time_s_for_index}`
  Constructs a deterministic burst sampler, exposes timing metadata, reports how many particles have been emitted so far, and samples one or all visible particles at a target timestamp.
- `BurstEmitterParticle`
  Reports the sampled particle index, timing, source offset, emission angle, and resolved destination rect for one particle.

## Logic narrative
- `BurstEmitterConfig` separates layer-owned settings from per-particle `BurstEmitterCellConfig`, mirroring the legacy `CAEmitterLayer` and `CAEmitterCell` split.
- `new()` sanitizes the config up front so downstream sampling logic never has to special-case negative birth rates, durations, or scales.
- `emitted_particle_capacity()` computes the total number of particles that can be born during the active emission window.
- `spawned_particle_count()` uses the current timestamp plus the configured birth rate to decide how many particles have been emitted so far.
- `particle()` resolves one particle deterministically from `(seed, index)`:
  - derive the spawn time
  - reject particles outside the integer-millisecond visible window
  - sample a source offset inside the configured emitter volume
  - sample an emission angle inside the configured angular spread
  - advance the particle by `velocity * age`
  - scale the destination rect from the caller’s base sprite side
- `particles()` is a thin collector over `particle()` for the currently visible set.
- `sample_ellipsoid_point()` approximates the legacy spherical emitter source with a deterministic ellipsoid sampler whose XY size comes from the caller and whose Z depth comes from the config.

## Preconditions and postconditions
- Callers provide the emitter center and the base sprite side used to scale both the source volume and the emitted sprite size.
- Returned particle rects are deterministic for the same config, seed, start time, emitter center, base side, and sample time.
- `visible_end_ms()` is the first timestamp where the helper reports no live particles.

## Invariants maintained
- Negative durations, scale, birth rate, and velocity never leak into the sampling path.
- Particle lifetimes are clipped on integer-millisecond boundaries so cleanup aligns exactly with `visible_end_ms()`.
- The same `(seed, index)` pair always yields the same source offset and emission angle.

## Edge cases and failure modes
- Zero birth rate or zero active duration yields zero capacity and no particles.
- Zero particle lifetime yields no visible particles even if births are configured.
- Zero or negative base sprite sides clamp to zero-sized source geometry and zero-sized particle rects instead of panicking.

## Concurrency and memory behavior
- `BurstEmitter` is a small `Copy` value with no interior mutability.
- Sampling allocates only when `particles()` collects visible results into a `Vec`; `particle()` itself is allocation-free.

## Performance notes
- Sampling is O(visible_particles) and deterministic.
- The helper avoids dynamic dispatch and uses simple integer and scalar floating-point math so app crates can call it every frame for short-lived bursts.

## Feature flags and cfgs
- No feature-specific branches.

## Testing and benchmarks
- Covered by `crates/ui-core/tests/emitter_tests.rs`.
- Downstream Nametag Radar tests also exercise the same helper through the app-level ghost burst path.

## Examples
```rust
use oxide_ui_core::{
   BurstEmitter, BurstEmitterCellConfig, BurstEmitterConfig, BurstEmitterShape,
};

let emitter = BurstEmitter::new(
   BurstEmitterConfig
   {
      active_duration_s: 1.1,
      emitter_size_scale: [1.5, 1.5],
      emitter_depth: 15.0,
      emitter_shape: BurstEmitterShape::Sphere,
      cell: BurstEmitterCellConfig
      {
         birth_rate: 25.0,
         lifetime_s: 1.0,
         velocity_points_per_s: 300.0,
         scale: 0.10,
         emission_range_rad: std::f32::consts::TAU,
         emission_longitude_rad: 0.0,
      },
   },
   2_000,
   77,
);
let particles = emitter.particles(3_100, [120.0, 90.0], 32.0);
assert!(!particles.is_empty());
```

## Changelog
- 2026-03-26: added the shared CAEmitter-style burst sampler so app crates can reuse legacy particle timing and source-shape behavior without carrying app-local particle engines.
