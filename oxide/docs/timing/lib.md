# timing::lib

## Intention and purpose

`oxide-timing` provides the monotonic clock, timer queue, and animation stepping utilities shared by Oxide hosts and UI code. It exists so frame loops and animation controllers can use one small timing surface instead of each host inventing its own clock.

## Relation to the rest of the code

The crate depends on `oxide-platform-api` for animation descriptors and values. UI scenes call `now_ms`, schedule timers, and step animations. Browser hosts need this crate during WebAssembly startup, so the monotonic clock uses `performance.now()` on wasm and `std::time::Instant` on native targets.

Call flow:

- Host frame loop
- `oxide_timing::now_ms`
- scene/router update
- `oxide_timing::anim::step`
- app or UI reads animation values

## Entry points list

- `oxide_timing::now_ns() -> u64`: returns monotonic nanoseconds since an arbitrary target-specific origin.
- `oxide_timing::now_ms() -> u64`: returns monotonic milliseconds since an arbitrary target-specific origin.
- `oxide_timing::schedule_after(delay_ms: u64, f: F) -> TimerId`: stores a callback for a future timer tick.
- `oxide_timing::advance_timers(now_ms_val: u64)`: drains and runs due timer callbacks.
- `oxide_timing::set_reduce_motion(enabled: bool)`: toggles global animation duration reduction.
- `oxide_timing::testing::*`: test-only reset and inspection helpers.
- `oxide_timing::anim::start(desc: &AnimDesc) -> AnimId`: starts an animation descriptor.
- `oxide_timing::anim::cancel(id: AnimId)`: cancels an animation by id.
- `oxide_timing::anim::cancel_prop(prop: AnimProp)`: cancels the animation currently owning a property.
- `oxide_timing::anim::step(now_ms_val: u64)`: advances global animation state.
- `oxide_timing::anim::value_at(desc: &AnimDesc, elapsed_ms: u32) -> AnimValue`: samples an animation descriptor.

## Logic narrative

Native targets keep a lazily initialized `Instant` and report elapsed time from that process-local origin. WebAssembly targets cannot call `Instant::now` on `wasm32-unknown-unknown`, so they read `window.performance().now()` and clamp invalid values to zero. Timers are stored in a `BTreeMap` keyed by due millisecond so `advance_timers` can split due work from future work. Animations are stored by id with a property-to-id map so starting a new animation on a property replaces the old one.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

Clock values are monotonic when the underlying host clock is monotonic. Timer callbacks run at most once unless they schedule new timers. Animation maps do not retain replaced property animations. The crate does not use unsafe code.

## Edge cases and failure modes

Clock overflow clamps to `u64::MAX`. Missing browser `window` or `performance` returns `0` rather than panicking, which lets wasm hosts fail gracefully in unusual harnesses. Timer advancement at `u64::MAX` drains the full queue.

## Concurrency and memory behavior

Timer and animation state use global mutex-protected maps. Timer scheduling allocates one callback box and one map entry. The monotonic clock path performs no heap allocation.

## Performance notes

`now_ms` and `now_ns` are hot-path helpers and stay minimal. Timer advancement is proportional to due entries plus map split cost. Animation stepping is linear in active animations.

## Feature flags and cfgs

`target_arch = "wasm32"` switches the clock implementation to browser `performance.now()` and enables the `web-sys` dependency. Native targets continue using `std::time::Instant`.

## Testing and benchmarks

`oxide/crates/timing/tests/lib_tests.rs` covers monotonic ordering, timer firing, timer callback behavior, id allocation, and animation lifecycle/value behavior. Browser clock verification is covered by the wasm host browser boot test.

## Examples

```rust
pub fn frame_delta(previous: u64) -> u64
{
   oxide_timing::now_ms().saturating_sub(previous)
}
```

## Changelog

- Added a wasm32 browser monotonic clock backed by `performance.now()`.
