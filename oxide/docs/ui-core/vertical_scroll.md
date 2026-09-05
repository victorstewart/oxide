# ui-core `vertical_scroll.rs`

## Intention and purpose

`VerticalScrollSurface` is the production Rust-owned interaction state for a
vertically scrolling Oxide surface. It turns raw platform touches into a
clamped content offset, continues a release with deterministic inertia, tells
the host whether another frame is required, and exposes an unambiguous settled
state for app behavior and bounded measurement.

The type intentionally does not own `CollectionView`. Keeping geometry and
cell composition separate lets the same motion state drive any vertical Oxide
content while a collection app uses `surface.offset()` as its
`CollectionView::set_scroll` input.

## Relation to the rest of the code

- `oxide-platform-ios` forwards raw contacts as
  `oxide_platform_api::TouchEvent` values.
- `oxide-input::TouchSurfaceRecognizer` owns contact identity and one/two-touch
  comprehension.
- `VerticalScrollSurface` owns drag admission, release-velocity sampling,
  inertia, scheduling, and settlement.
- `ScrollState` owns finite extent normalization and offset clamping.
- `CollectionView` consumes the resulting offset before laying out only the
  visible cells.

```text
raw TouchEvent
  -> TouchSurfaceRecognizer::on_touch
  -> VerticalScrollSurface::input_touch
  -> ScrollState
  -> CollectionView::set_scroll
  -> visible collection layout and draw encoding

display callback timestamp
  -> VerticalScrollSurface::advance_to
  -> request another frame while wants_next_frame() is true
```

## Entry points list

- `oxide_ui_core::VerticalScrollSurface::new(content_extent: f32, viewport_extent: f32) -> Self`
  creates an idle surface at offset zero after normalizing both extents.
- `oxide_ui_core::VerticalScrollSurface::update_extents(&mut self, content_extent: f32, viewport_extent: f32) -> bool`
  updates geometry, clamps the offset, stops outward motion at a new bound, and
  reports whether visible position changed.
- `oxide_ui_core::VerticalScrollSurface::set_offset(&mut self, offset: f32) -> f32`
  performs a programmatic jump and cancels contacts and inertia.
- `oxide_ui_core::VerticalScrollSurface::offset(&self) -> f32`
  returns the current clamped content offset in logical points.
- `oxide_ui_core::VerticalScrollSurface::max_offset(&self) -> f32`
  returns `max(content_extent - viewport_extent, 0)`.
- `oxide_ui_core::VerticalScrollSurface::progress(&self) -> f32`
  returns normalized progress in `[0, 1]`, or zero when content fits.
- `oxide_ui_core::VerticalScrollSurface::input_touch(&mut self, event: &TouchEvent) -> bool`
  consumes one raw contact update and reports whether the visible offset changed.
- `oxide_ui_core::VerticalScrollSurface::advance_to(&mut self, timestamp_ns: u64) -> bool`
  advances inertia to an explicit monotonic time and reports an offset change.
- `oxide_ui_core::VerticalScrollSurface::wants_next_frame(&self) -> bool`
  reports whether inertia requires another display callback.
- `oxide_ui_core::VerticalScrollSurface::is_settled(&self) -> bool`
  reports true only when there are no contacts, no drag state, and no inertia.
- `oxide_ui_core::VerticalScrollSurface::cancel_motion(&mut self)`
  clears contact/drag/inertial state without changing offset or extents.

## Logic narrative

1. A first touch stops an existing fling and starts a pending drag at the raw
   contact position.
2. Motion remains pending until vertical displacement reaches six logical
   points. Crossing the threshold applies the complete displacement from the
   initial anchor, avoiding a visible jump between finger and content.
3. Each accepted one-finger movement applies `-dy` to content offset and adds
   its timestamp/position to an eight-entry inline velocity ring.
4. A second touch cancels drag ownership and velocity history. Two-touch pan and
   pinch events are ignored by this vertical surface.
5. When two contacts return to one, the remaining contact becomes a fresh
   pending drag anchor. Motion that happened during the two-touch interval is
   never replayed as collection scroll.
6. Cancellation stops immediately. A normal end derives finger velocity from
   the oldest usable sample in the preceding 100 milliseconds, converts it to
   opposite-direction content velocity, and clamps it to 8,000 points/second.
7. `advance_to` applies a frame-partition-independent geometric decay of `0.998`
   per millisecond. It stops at exactly five points/second or at a content bound.
8. The host continues callbacks only while `wants_next_frame()` is true. A
   stationary finger is not settled but does not request idle frames.
9. Per-frame `update_extents` feedback clamps the existing `f64` position and
   republishes its `f32` projection; unchanged geometry never replaces the
   high-precision accumulator with that projection.

For elapsed milliseconds `t`, decay is integrated as:

```text
r = 0.998 ^ t
distance = velocity * (1 - r) / (1000 * (1 - 0.998))
next_velocity = velocity * r
```

When a step crosses the settle threshold, integration ends at the exact decay
factor that reaches five points/second. Therefore terminal travel is stable
across 60 Hz, 120 Hz, and irregular callback partitions.

## Preconditions and postconditions

- Touch timestamps and frame timestamps share one monotonic nanosecond domain.
- The caller supplies frame time explicitly; the surface never reads a global
  clock, sleeps, or schedules a timer.
- Returned offsets are always finite and within `[0, max_offset]`.
- `set_offset` is a hard programmatic jump and therefore ends the current
  interaction.
- A caller should set `CollectionView::set_scroll(surface.offset())` before
  layout and then feed the resulting content/viewport extents back into
  `update_extents`.

## Edge cases and failure modes

- Non-finite extents normalize to zero through `ScrollState`.
- Non-finite move coordinates are ignored by `TouchSurfaceRecognizer`; terminal
  events still remove the contact so input cannot become stuck.
- Zero, equal, or backwards frame timestamps do not advance motion.
- A release without at least eight milliseconds of usable recent motion does
  not fling.
- A release at or below five points/second settles before requesting a frame.
- A release toward an already-reached bound settles immediately.
- Content no larger than its viewport always remains at offset zero.
- More than one active contact keeps the surface non-settled but cannot move it.

## Concurrency and memory behavior

`VerticalScrollSurface` is ordinary owned mutable state. It contains no locks,
atomics, callbacks, or shared ownership and is intended to be updated on the
app/UI thread in raw-event order.

Gesture outputs are returned by `TouchSurfaceRecognizer::on_touch`, and velocity
history is stored in `[VelocitySample; 8]`. The recognizer retains an overflow
vector only for contact counts above its four inline slots. The returned event
vector is part of the current public API; this surface does not promise a
particular allocation count for raw-touch handling.

## Performance notes

- Touch handling is O(1) for the normal one/two-contact path.
- Every inertial advance is O(1) and uses closed-form decay rather than fixed
  simulation substeps.
- Position is accumulated in `f64` and published as a clamped `f32`, preventing
  callback partitioning from accumulating materially different rounding error.
- No timer, log, or formatted string exists in the interaction loop.
- The collection remains responsible for virtualizing visible work.

## Feature flags and cfgs

The surface is always available. It has no target-specific branches or feature
flags.

## Testing and benchmarks

- `crates/ui-core/tests/vertical_scroll_tests.rs` verifies slop, direction,
  release, cancellation, multi-touch restart, bounds, invalid extents,
  settlement, 60/120/irregular callback partition equivalence, and unchanged
  per-frame extent feedback at a large content offset.
- The production authoring and feed-journey perf cases must measure this API
  before the interaction is promoted in persisted performance reports.

## Examples

```rust
use oxide_ui_core::{VerticalScrollSurface, collection::CollectionView};

fn prepare_collection(collection: &mut CollectionView, scroll: &VerticalScrollSurface)
{
   collection.set_scroll(scroll.offset());
}
```

## Changelog

- 2026-08-06: Added Rust-owned raw-touch vertical drag, deterministic inertia,
  frame-request, cancellation, and settled-state semantics.
