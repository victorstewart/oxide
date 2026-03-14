# ui-core `picker_popup.rs`

## Intention and purpose
- Provide reusable popup and wheel-picker interaction state for apps that render their own popup chrome and picker visuals.
- Centralize outside-tap dismissal, drag tracking, position clamping, legacy snap rounding, and linear picker tap-to-index mapping in Oxide instead of scene code.

## Relation to the rest of the code
- Lives beside `elements.rs` and `overlay.rs` as the state-only interaction layer.
- Uses `oxide_platform_api::TouchId` and `oxide_renderer_api::RectF`.
- Consumed by Nametag shell radar/mobile picker flows and framework tests in `crates/ui-core/tests/picker_popup_tests.rs`.

Call flow:
- `App popup layout` -> `PanelPopupState`
- `App picker geometry` -> `WheelPickerState`
- `PopupWheelPickerState` -> `PanelPopupState + WheelPickerState`

## Entry points list
- `PopupTapRegion`
  Reports whether a tap landed inside the panel or outside it.
- `PanelPopupState::{new, open, close, toggle, is_open, classify_tap}`
  Generic popup visibility and outside-tap classification.
- `WheelPickerState::{new, with_overscroll_rows, item_count, position, overscroll_rows, drag_touch_id, is_dragging, set_item_count, sync_to_index, selected_index, snap_index, snap_index_for, clamp_position, clamp_position_for, begin_drag, update_drag, finish_drag, cancel_drag, index_for_linear_tap}`
  Generic vertical wheel-picker controller with drag and legacy rounding behavior.
- `PopupWheelPickerState::{new, with_overscroll_rows, open, close, is_open, sync_to_index, position, selected_index, classify_panel_tap, begin_drag, update_drag, finish_drag, cancel_drag, is_dragging, drag_touch_id, index_for_linear_tap}`
  Combined popup + wheel-picker controller for anchored modal pickers.

## Logic narrative
- `PanelPopupState` is intentionally small: it owns only visibility and panel/outside classification because popup layout, blur, and drawing already live elsewhere in `ui-core`.
- `WheelPickerState` owns the reusable mechanics Nametag had been duplicating: storing a float position over item rows, tracking the active drag pointer, converting drag delta into picker travel, clamping with configurable overscroll, and snapping with the legacy `fraction > 0.5` rule.
- `index_for_linear_tap()` exists for flat pickers like the mobile country selector where the app already knows the visible picker surface and row height.
- `PopupWheelPickerState` bundles the two states so app code can treat modal wheel pickers as one controller while still feeding in app-specific geometry.

## Preconditions and postconditions
- Constructors require the current item count and selected index.
- Position is always clamped into the configured valid range, including overscroll allowance.
- `finish_drag()` only commits when the provided touch id matches the active drag.
- `classify_tap()` and `classify_panel_tap()` are pure geometry checks and never mutate visibility.

## Invariants maintained
- Drag state is either absent or bound to one `TouchId`.
- `selected_index()` and `snap_index()` always return an index inside `0..item_count` when items exist.
- Empty pickers never panic; they collapse to index `0` and position `0.0`.

## Edge cases and failure modes
- `item_count == 0` collapses all clamp/snap/tap calculations to zero.
- `row_height <= 0` is normalized to `1.0` during drag and linear-tap mapping so callers cannot divide by zero.
- `cancel_drag(Some(id))` ignores mismatched touch ids instead of destroying another pointer's session.

## Concurrency and memory behavior
- All state is single-owner and `Copy`.
- No heap allocation or synchronization is involved.

## Performance notes
- All operations are O(1).
- Deduplicating this code into Oxide removes repeated scene-local drag bookkeeping without adding dynamic dispatch.

## Feature flags and cfgs
- No feature-specific branches.

## Testing and benchmarks
- Covered by `crates/ui-core/tests/picker_popup_tests.rs`.
- Downstream Nametag shell tests also exercise the same controller through mobile-picker and radar popup flows.

## Examples
```rust
use oxide_platform_api::TouchId;
use oxide_renderer_api::RectF;
use oxide_ui_core::PopupWheelPickerState;

let mut picker = PopupWheelPickerState::new(4, 1).with_overscroll_rows(0.35);
picker.open();
picker.begin_drag(TouchId(9), 100.0);
picker.update_drag(TouchId(9), 64.0, 18.0);
let index = picker.finish_drag(TouchId(9)).unwrap();
assert!(index <= 3);
assert_eq!(picker.index_for_linear_tap(RectF::new(0.0, 0.0, 120.0, 36.0), 18.0, 18.0), picker.selected_index());
```

## Changelog
- 2026-03-13: added Oxide-owned popup/wheel-picker interaction controllers and moved generic mobile/radar picker mechanics out of Nametag shell code.
