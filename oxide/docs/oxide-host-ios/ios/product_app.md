# oxide-host-ios `ios/product_app.m`

## Intention and purpose

- Provide the production UIKit shell selected when `test-scenes-entrypoint` is disabled.
- Own only UIApplication/scene lifecycle, one active Oxide-owned window and Metal view, raw OS event delivery, text-input adaptation, display-link scheduling, and drawable handoff.

## Relation to the rest of the code

- `oxide-host-ios::run_app` installs the Rust `App` before this shell enters `UIApplicationMain`.
- `oxide-platform-ios/src/ios/host_services.m` owns platform-service ABI that does not depend on a particular window or display link.
- Rust owns product composition, touch interpretation, scrolling, inertia, frame demand, and renderer submission.

## Entry points list

- `oxide_host_start(argc, argv)` enters `UIApplicationMain` after Rust has installed the app.
- `OxideTouchWindow::sendEvent:` forwards raw touch and hardware-key samples into Rust.
- `OxideProductSceneDelegate` owns one window, lifecycle delivery, display-link scheduling, and drawable handoff.
- `oxide_host_request_display_link_wake`, `oxide_host_set_high_refresh`, and `oxide_host_display_link_frame_rate_range` expose lock-free scheduling controls and observation.
- `oxide_host_ime_show` and `oxide_host_ime_hide` adapt Rust text-input demand to the hidden UIKit text client.
- `oxide_host_environment_transition_counts` exposes monotonic thermal and Low Power Mode transition counters.

## Logic narrative

- `OxideTouchWindow::sendEvent:` forwards every raw touch phase, stable identity, coordinate, pressure/tilt, device kind, and OS timestamp before normal UIKit dispatch.
- The first window scene claims the process-global surface. Any concurrent additional session is refused and destroyed before it can create a second window; disconnect releases ownership for a later replacement scene.
- The Metal root and hidden text-input adapter explicitly opt out of UIKit accessibility exposure. They carry no labels, identifiers, traits, or platform-authored semantics.
- Rust app initialization installs the window callback, then the shell sends the actual view bounds and safe-area metrics once. It does not synthesize a second zero-inset resize.
- The display link asks Rust to prepare a frame before acquiring a drawable. Missing drawables cancel the prepared frame; successful submission is the only point that acknowledges its wake generation.
- The display link pauses when the app is idle and wakes through one monotonic generation, including redraw requests from platform services.
- The native camera-publication callback is registered while the product scene is active, atomically cleared when it resigns, backgrounds, disconnects, or terminates, and requests the same generation wake. Shared camera dispatch performs no host lookup and legacy camera benchmarks retain their own scheduling callback.
- Thermal and Low Power Mode observers maintain monotonic transition counters so benchmark admission cannot mistake equal endpoint states for an unchanged environment.
- The shell forwards indirect pointer measurements as raw deltas only; it owns no product gesture state, transform, scrolling, or physics.
- Hardware presses emit only the down and repeat states represented by the public Rust key event; UIKit end/cancel phases are not fabricated as a second key event.

## Preconditions and postconditions

- Exactly one Rust app must be installed before native host initialization. The shell permits exactly one connected window scene at a time.
- The app supplies app-owned prepared-frame storage or uses the persistent compatibility encoder owned by Rust.
- UIKit acquires and presents only the drawable that Rust submits through the established Metal renderer.

## Edge cases and failure modes

- An unavailable drawable preserves frame demand and requests a retry.
- Backgrounding pauses callback delivery and resets Rust display timing before foreground work resumes.
- Invalid pointer measurements are rejected at the native boundary.

## Concurrency and memory behavior

- UI event and display-link callbacks execute on the main thread.
- Cross-thread redraw requests coalesce into one main-queue wake dispatch.
- Display-link configuration publishes its exact integer range into an atomic snapshot on the main thread. Rust reads only that snapshot, so observation never dereferences UIKit or Core Animation objects off-main.
- Camera publication reads one lock-free callback pointer; scene lifecycle changes publish or clear it with release ordering.

## Performance notes

- The frame loop creates no pipelines, samplers, command queues, or image resources after warmup.
- Drawable acquisition happens after app update and draw-list preparation.

## Feature flags and cfgs

- The shell is selected when `test-scenes-entrypoint` is absent. That feature selects the legacy `app.m` source instead, so both UIApplication implementations are never linked together.

## Testing and benchmarks

- `tests/production_shell_tests.rs` freezes source ownership, raw-event delivery, transition counters, and feature isolation.
- `tests/drawable_tests.rs` freezes late drawable acquisition, retry semantics, lifecycle timing, and frame-resource reuse.
- Physical-device pilots must exercise this exact shell; Simulator output is never official comparison evidence.

## Examples

```rust
let result = unsafe
{
   oxide_host_ios::run_app(argc, argv, Box::new(app))
};
```

## Changelog

- 2026-08-06: enforced single-scene ownership, explicit accessibility opt-out, actual-only startup metrics, and thread-safe display-link range observation.
- 2026-08-06: bound the atomic camera wake callback to foreground lifecycle and stopped duplicating key releases as repeat events.
- 2026-08-06: added the bounded production app-injection shell and environment-transition accounting.
