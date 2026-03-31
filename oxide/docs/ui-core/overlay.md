# ui-core `overlay.rs`

## Intention and purpose

`overlay.rs` owns two related but distinct contracts:

- `OverlayStack` keeps the generic fullscreen-backdrop overlay stack used for simple modal blocking and z-ordered surface composition.
- `PopupManager` keeps the popup-window lifecycle contract used when the caller needs key-popup lookup, approval-gated dismissal, touch-exception routing, optional touch veto hooks, and explicit content-size resync.

The split keeps generic overlays simple while giving popups the stricter window-style behavior expected by higher-level app code.

## Relation to the rest of the code

- `surface.rs` embeds both `OverlayStack` and `PopupManager` inside `SurfaceRouter`.
- Higher-level app crates push popup `UiSurface` instances through `PopupManager`.
- `DrawListBuilder` receives the resolved backdrop and popup surface draws after the manager orders the stack by `z_index`.

## Entry points list

- `OverlayStack::push(surface, visual, behavior) -> OverlayHandle`
  Pushes a generic overlay surface and lays it out against the current viewport.
- `OverlayStack::pointer_event(x, y, buttons) -> OverlayPointerResult`
  Applies generic content-root hit testing and optional release-time background dismissal.
- `PopupManager::push(surface, spec) -> PopupHandle`
  Pushes a popup surface plus its lifecycle callbacks and touch-region contract.
- `PopupManager::key_popup() -> Option<PopupHandle>`
  Returns the topmost popup handle, matching the key-window lookup pattern.
- `PopupManager::popup_is_key_window() -> bool`
  Reports whether any popup currently owns the top window slot.
- `PopupManager::dismiss(handle) -> bool`
  Runs `approve_dismissal`, removes the popup when approved, and then runs the one-shot dismissal callback.
- `PopupManager::dismiss_key_popup() -> Option<PopupHandle>`
  Dismisses the current key popup through the same approval path.
- `PopupManager::content_size_changed(handle) -> bool`
  Re-layouts the popup surface against the current viewport and refreshes the default content-root touch exception.
- `PopupManager::set_touch_region(handle, region) -> bool`
  Overrides the popup touch-exception behavior with either `None`, `ContentRoot`, or a manual rectangle.
- `PopupManager::pointer_event(x, y, buttons) -> OverlayPointerResult`
  Applies popup hit testing, `approve_touch`, touch-exception dismissal, and release-time background dismissal while preserving key-window input blocking when dismissal is denied.

## Logic narrative

`OverlayStack` is still the minimal overlay primitive. It lays out surfaces to the current viewport, orders them by `z_index`, draws one backdrop per entry, and only knows about `dismiss_on_background_tap`, `content_root`, and `focus_root`.

`PopupManager` is popup-specific and does not delegate to `OverlayStack`. Each popup entry carries:

- the composed `UiSurface`
- visual ordering
- popup behavior and focus metadata
- a declared touch-exception source
- a cached resolved touch rectangle
- dismissal approval, dismissal, and touch-approval callbacks

The cached touch rectangle is refreshed on push, viewport changes, manual touch-region overrides, and explicit `content_size_changed` calls. `PopupTouchRegion::ContentRoot` resolves through the popup surface tree so callers can keep the old “touch exception follows the popup content rect” behavior without hand-maintaining the rectangle.

Pointer routing checks the topmost popup first. A popup attempts dismissal when any of these are true:

- `approve_touch` rejects the point
- the point lands outside the resolved touch-exception rect
- `dismiss_on_background_tap` is enabled and a release lands outside the popup content root

When dismissal is attempted, `approve_dismissal` runs first. If it returns `false`, the popup remains key and the event is consumed so underlying content does not receive the touch. When dismissal is approved, the popup is removed and the one-shot dismissal callback runs exactly once.

## Preconditions and postconditions

- Preconditions:
  - Callers must set `behavior.content_root` when they want `PopupTouchRegion::ContentRoot` to track a real rect.
  - Callers that mutate popup layout-affecting style after push must call `content_size_changed`.
- Postconditions:
  - `key_popup()` always returns the highest-`z_index` popup, with insertion order breaking ties.
  - Dismissal callbacks run at most once per popup entry.
  - Manual `PopupTouchRegion::Rect` overrides survive later `content_size_changed` calls until the caller replaces them.

## Edge cases and failure modes

- If `dismiss(handle)` is called for an unknown handle, it returns `false`.
- If `PopupTouchRegion::ContentRoot` is requested without a `content_root`, the popup falls back to having no resolved touch-exception rect.
- If dismissal approval fails, the popup stays mounted and input remains blocked at the popup layer.

## Concurrency and memory behavior

- The manager is single-threaded UI state and does not share mutable state across threads.
- Popup callbacks are boxed trait objects because they sit on the public authoring boundary, not in the hot draw path.

## Performance notes

- Popup routing still only inspects the topmost popup for input.
- Touch-exception resolution is cached and only recomputed on push, viewport changes, explicit resync, or manual override.
- The public authoring perf case in `oxide-perf-runner` exercises the popup lifecycle APIs alongside the existing surface-router composition path.

## Testing and benchmarks

- `crates/ui-core/tests/overlay_tests.rs`
  Verifies z-ordering, key-popup lookup, approval-gated dismissal, touch veto routing, content-size resync, and manual touch-region override behavior.
- `crates/perf-runner/src/lib.rs`
  Extends the `cpu.authoring.surface_router.compose` authoring case to cover popup lifecycle APIs inside the existing surface-router composition benchmark.

## Changelog

- 2026-03-28: upgraded `PopupManager` from a thin overlay wrapper into the shared popup-window lifecycle contract with key-popup lookup, approval-gated dismissal, touch exceptions, and explicit content-size refresh support.
