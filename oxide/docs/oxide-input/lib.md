# `oxide-input`

## Purpose

`oxide-input` turns platform-neutral raw input records into reusable Oxide-owned
interaction state. Platform hosts deliver events; this crate owns touch identity,
gesture transitions, deltas, velocity, and cancellation semantics.

Product behavior must not be implemented in an operating-system gesture
recognizer. A host may adapt an OS event into `oxide_platform_api`, but the
resulting gesture state belongs here or in the consuming Oxide surface.

## Main entry points

- `GestureRecognizer::on_touch` recognizes tap, double-tap, long-press, and pan.
- `GestureRecognizer::on_touch_with_feedback` adds optional haptic intent.
- `PrimaryTouchTracker::on_touch` maps the primary contact to pointer-style
  samples.
- `TouchSurfaceRecognizer::on_touch` tracks a continuous multi-touch surface and
  returns zero or more `TouchSurfaceEvent` values.
- `ScrollAccumulator` combines wheel or scroll deltas and phase changes.
- `touch_phase_from_raw` and `pointer_device_from_raw` validate raw host values.

`VerticalScrollSurface` in `oxide-ui-core` consumes
`TouchSurfaceRecognizer::on_touch`; input recognition remains independent from
layout, rendering, and scene ownership.

## Continuous-surface behavior

`TouchSurfaceRecognizer` stores active contacts by stable `TouchId`. Its frame is
derived from the two lowest active touch identifiers so host iteration order does
not change pan or pinch output.

- One active contact emits pan deltas from its previous position.
- Two active contacts emit pinch scale and center-pan deltas.
- A contact-count change emits `ActiveTouchesChanged` and resets the two-touch
  cumulative pinch anchor when appropriate.
- `End` and `Cancel` always release the matching identity, including when the OS
  supplies non-finite terminal coordinates.
- Non-finite `Start` and `Move` coordinates are ignored.
- More than four simultaneous contacts use overflow storage while preserving the
  same deterministic pair selection.

The recognizer reports local gesture facts. Momentum, bounds, overscroll policy,
and visible transforms belong to the consuming surface.

## Example

```rust
use oxide_input::{TouchSurfaceEvent, TouchSurfaceRecognizer};
use oxide_platform_api::TouchEvent;

fn consume(recognizer: &mut TouchSurfaceRecognizer, event: &TouchEvent)
{
   for surface_event in recognizer.on_touch(event)
   {
      match surface_event
      {
         TouchSurfaceEvent::Pan { dx, dy, .. } => apply_pan(dx, dy),
         TouchSurfaceEvent::Pinch { scale_delta, .. } => apply_pinch(scale_delta),
         TouchSurfaceEvent::ActiveTouchesChanged { touch_count, .. } =>
            update_contact_count(touch_count),
      }
   }
}
```

## Preconditions and invariants

- `TouchId` must remain stable from `Start` through `End` or `Cancel`.
- Host timestamps are monotonic nanoseconds where the event type provides them.
- Coordinates use the host surface's point space.
- Duplicate starts replace the stored coordinates for that identity.
- Unknown moves and terminal events do not create contacts.
- `reset` drops all active recognition state.

## Memory and concurrency

Recognizers are mutable state machines and are not internally synchronized. Keep
one instance with its owning input surface and feed that instance in event order.

`TouchSurfaceRecognizer::on_touch` returns a `Vec<TouchSurfaceEvent>`. Four
contacts are stored inline; only additional simultaneous contacts require contact
overflow storage. No stronger allocation guarantee is part of this API contract.

## Verification

`crates/input/tests/lib_tests.rs` covers:

- tap, double-tap, long-press, pan, velocity, and cancellation;
- timestamp precedence;
- stable two-touch selection and overflow contacts;
- per-move pan and cumulative pinch values;
- terminal cleanup with invalid coordinates; and
- restoration of the remaining contact's pan anchor after a two-touch release.

Consumers must also test their own bounds, inertia, and visible response. Passing
recognizer tests alone does not prove a host delivered real device events.

## Change record

- Documented the input state-machine boundaries and continuous-surface contract.
- Guaranteed cleanup for invalid-coordinate terminal touch events.
