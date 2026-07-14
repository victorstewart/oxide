# ui-core `anim.rs`

## Intention and purpose
- Centralize reusable animation math for Oxide UI crates and downstream apps.
- Keep app crates from carrying local easing solvers or duplicated keyframe-sampling code when the behavior is generic.

## Relation to the rest of the code
- `anim.rs` sits beside `overlay`, `picker_popup`, and `text_fields` as shared UI infrastructure inside `oxide-ui-core`.
- App crates such as Nametag consume `anim::helpers` for deterministic easing/offset math while keeping their own scene-specific layout and visual styling.

## Entry points
- `Animator::step(now_ms) -> &AnimOverrideSlots`
  Samples active animations, compacts completed entries in one retain pass, and returns renderer-facing dense node-indexed overrides without rebuilding a map.
- `AnimOverrideSlots::{changed_nodes,paint_changed_nodes}`
  Separates transform/opacity metadata changes from color/radius/shadow changes that still require a paint chunk rebuild.
- `Animator::{overrides,overrides_mut}`
  Exposes animator-owned retained slot storage for `UiSurface` and explicit diagnostic overrides.
- `anim::helpers::cubic_bezier_ease(progress, x1, y1, x2, y2) -> f32`
  - Solves a cubic-bezier easing curve in Rust and returns the eased y value for the requested progress.
- `anim::helpers::cubic_bezier_ease_in_out(progress) -> f32`
  - Shared standard ease-in-out profile using the legacy `(0.42, 0.0, 0.58, 1.0)` control points.
- `anim::helpers::sample_keyframed_offset(elapsed_ms, phase_duration_ms, phase_targets) -> f32`
  - Samples a fixed-duration phase-target sequence that starts at zero and returns to zero after the full duration.
- `anim::helpers::required_field_shake_offset(elapsed_ms, scale) -> f32`
  - Shared required-field shake profile layered on top of the generic keyframed sampler.
- `anim::helpers::REQUIRED_FIELD_SHAKE_PHASE_DURATION_MS`
- `anim::helpers::REQUIRED_FIELD_SHAKE_DURATION_MS`
  - Export the shared required-field shake timing constants so app crates can align lifecycle state with the same curve they render.

## Logic narrative
- At frame start, active and previous node lists swap while entry storage remains allocated. Sampling writes each node directly by `NodeId`; frame finish compares current and previous values once and records precise dirty classes.
- Completion keeps the final sampled value for that frame, removes finished animations with `retain_mut`, and publishes override removal on the next step.
- The cubic-bezier helper inverts the x-axis with a bounded Newton iteration and then samples the matching y-axis value.
- The keyframed-offset helper treats each phase target as the end of a fixed-duration segment and linearly interpolates from the prior target.
- The required-field shake helper is just a named shared profile on top of that generic sampler.

## Preconditions, postconditions, invariants
- Bezier progress is clamped to `[0.0, 1.0]`.
- Keyframed sampling with an empty target list always returns `0.0`.
- Shake sampling returns `0.0` once the configured duration has elapsed.

## Edge cases and failure modes
- Zero-duration phases are clamped to `1` millisecond.
- The bezier solver falls back to the current parameter when the derivative is too small to advance safely.
- Negative shake scales clamp to zero movement.

## Concurrency and memory behavior
- Animator state is render-thread-owned. Dense entry and work vectors retain capacity across frames; warm stepping performs no map-node allocation, repeated `Vec::remove`, I/O, or synchronization.

## Performance notes
- Sampling is linear in active animations plus the nodes active in the previous frame. Dense `NodeId` lookup is O(1).
- The bezier helper uses six Newton iterations, matching the previous app-local solver without introducing external dependencies.
- Keyframe sampling is O(1) and slice-based.

## Testing and benchmarks
- `crates/ui-core/tests/anim_prop.rs` covers repeats, finite sampling, dense storage reuse, interruption, completion, and override clearing.
- [`/Users/victorstewart/oxide/oxide/crates/ui-core/tests/anim_helpers.rs`](/Users/victorstewart/oxide/oxide/crates/ui-core/tests/anim_helpers.rs) covers segment construction, bezier non-linearity, keyframe interpolation, and the shared required-field shake profile.

## Changelog
- 2026-07-13: replaced per-frame map reconstruction and repeated finished-entry removal with animator-owned dense override slots and one-pass compaction for C26.
- 2026-03-13: moved shared cubic-bezier easing and required-field shake sampling into `oxide-ui-core` so app crates can drop duplicated animation helpers.
