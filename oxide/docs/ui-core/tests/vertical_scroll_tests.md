# ui-core `tests/vertical_scroll_tests.rs`

## Intention and purpose

This integration suite proves the public vertical-scroll state machine is
deterministic, clamped, multi-touch safe, and honest about frame demand and
settlement.

## Relation to the rest of the code

Tests inject the same platform-neutral raw touches delivered by an iOS host and
explicit frame timestamps supplied by an app frame callback. They observe only
the public `VerticalScrollSurface` contract.

## Entry points list

- `drag_slop_applies_the_complete_displacement_once_crossed`
- `release_moves_through_inertia_and_settles`
- `terminal_travel_is_callback_partition_independent`
- `unchanged_extent_updates_preserve_partition_independent_travel`
- `reverse_fling_from_bottom_is_symmetric`
- `second_touch_cancels_drag_and_remaining_touch_restarts_without_a_jump`
- `cancel_never_flings_and_stationary_touch_requests_no_frames`
- `subthreshold_release_settles_without_a_reverse_frame`
- `clamped_bounds_extents_and_non_monotonic_time_stop_safely`
- `content_that_fits_the_viewport_never_starts_inertia`

## Logic narrative

A shared raw trace creates a known release velocity. The partition test clones
that state into 60 Hz, 120 Hz, and irregular callback schedules and advances
each until `wants_next_frame` clears. Terminal offsets must agree within 0.01
logical point. Separate tests cover direction, final end displacement,
multi-touch cancellation/restart, extent shrink, and invalid geometry.
The extent-feedback regression starts around offset 400,000 and feeds unchanged
content/viewport extents back after every advance, matching the production
collection loop while proving the internal `f64` accumulator is not replaced by
its published `f32` value.
The slow-release regression also requires velocity already at or below the
settle threshold to stop without requesting or moving on another frame.

## Preconditions and postconditions

Synthetic timestamps are monotonic except in the explicit backwards-time test.
Every settling loop has a hard iteration bound and contains no sleep.

## Edge cases and failure modes

The suite covers no-scroll content, zero frame demand under a stationary touch,
cancel versus end, reverse motion, bound collision, non-finite extents, and
backwards timestamps.

## Concurrency and memory behavior

Tests are single-threaded and deterministic. Surface and recognizer state remain
owned by the test thread.

## Performance notes

Callback-partition equivalence guards against frame-rate-dependent integration
and fixed-substep loops. The large-offset extent-feedback variant catches
subpixel drift that becomes visible after device scaling. These tests make no
allocation or timing claim.

## Feature flags and cfgs

None.

## Testing and benchmarks

Run with `cargo test --locked -p oxide-ui-core --test vertical_scroll_tests`.

## Examples

The forward and reverse fling helpers are executable usage traces.

## Changelog

- 2026-08-06: Added deterministic raw-touch, inertia, multi-touch, settlement,
  and large-offset per-frame extent-feedback coverage.
