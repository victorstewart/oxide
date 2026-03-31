# ui-core `picker_popup.rs`

## Intention and purpose
- Provide reusable popup and legacy picker interaction state for apps that render their own popup chrome and picker visuals.
- Centralize outside-tap dismissal, per-column drag tracking, valid-range clamping, legacy snap rounding, and scroll-end commit/haptic intent in Oxide instead of scene code.

## Relation to the rest of the code
- Lives beside `elements.rs` and `overlay.rs` as the state-only interaction layer.
- Uses `oxide_platform_api::TouchId` and `oxide_renderer_api::RectF`.
- Consumed by Nametag shell radar/mobile picker flows, `perf-runner`, and `crates/ui-core/tests/picker_popup_tests.rs`.

Call flow:
- `App popup layout` -> `PanelPopupState`
- `App picker column geometry` -> `PickerColumnState`
- `Column scroll-end commit` -> `PickerColumnCommit`
- `PopupPickerState` -> `PanelPopupState + Vec<PickerColumnState>`

## Entry points list
- `PopupTapRegion`
  Reports whether a tap landed inside the panel or outside it.
- `PanelPopupState::{new, open, close, toggle, is_open, classify_tap}`
  Generic popup visibility and outside-tap classification.
- `PickerColumnCommit::{selected_index, haptic_pattern}`
  Immutable old iOS scroll-end commit result with the fixed medium-impact haptic contract.
- `PickerColumnState::{new, item_count, position, drag_touch_id, is_dragging, set_item_count, sync_to_index, selected_index, snap_index, snap_index_for, clamp_position, clamp_position_for, begin_drag, update_drag, finish_drag, cancel_drag}`
  Legacy flat picker-column controller with drag and old iOS `fraction > 0.5` rounding.
- `PopupPickerState::{new, from_columns, column_count, open, close, is_open, set_column_item_count, position, selected_index, sync_to_index, sync_to_indices, classify_panel_tap, begin_drag, update_drag, finish_drag, cancel_drag, is_dragging, drag_touch_id}`
  Combined popup + multi-column picker controller for anchored modal pickers.

## Logic narrative
- `PanelPopupState` is intentionally small: it owns only visibility and panel/outside classification because popup layout, blur, and drawing already live elsewhere in `ui-core`.
- `PickerColumnState` owns the reusable mechanics Nametag had been duplicating: storing a float position over item rows, tracking the active drag pointer, converting drag delta into picker travel, clamping to valid item rows, and snapping with the legacy `fraction > 0.5` rule.
- `PickerColumnCommit` preserves the old iOS ownership boundary: the picker column decides the committed row and the fixed medium-impact haptic intent when scrolling ends.
- `PopupPickerState` bundles popup visibility with one or more picker columns so apps can treat legacy modal pickers as one controller while still feeding in app-specific geometry.

## Preconditions and postconditions
- Constructors require the current item count and selected index per column.
- Position is always clamped into the valid row range for that column.
- `finish_drag()` only commits when the provided touch id matches the active drag, and every commit carries one fixed medium-impact haptic intent.
- `classify_tap()` and `classify_panel_tap()` are pure geometry checks and never mutate visibility.

## Invariants maintained
- Each picker column tracks at most one active `TouchId`.
- `selected_index()` and `snap_index()` always return an index inside `0..item_count` when items exist.
- Empty pickers never panic; they collapse to index `0` and position `0.0`.

## Edge cases and failure modes
- `item_count == 0` collapses all clamp/snap calculations to zero.
- `row_height <= 0` is normalized to `1.0` during drag updates so callers cannot divide by zero.
- `cancel_drag(Some(id))` ignores mismatched touch ids instead of destroying another pointer's session.

## Concurrency and memory behavior
- All state is single-owner.
- `PopupPickerState` stores a `Vec` of columns; no synchronization is involved.

## Performance notes
- All hot-path operations are O(1) per touched column.
- Deduplicating this code into Oxide removes repeated scene-local drag bookkeeping without adding dynamic dispatch.

## Feature flags and cfgs
- No feature-specific branches.

## Testing and benchmarks
- Covered by `crates/ui-core/tests/picker_popup_tests.rs`.
- Downstream Nametag shell tests also exercise the same controller through mobile-picker and radar popup flows.
- `crates/perf-runner/src/lib.rs` benchmarks the popup picker interaction surface.

## Examples
```rust
use oxide_platform_api::TouchId;
use oxide_ui_core::PopupPickerState;

let mut picker = PopupPickerState::from_columns(vec![4, 3], vec![1, 0]);
picker.open();
picker.begin_drag(0, TouchId(9), 100.0);
picker.update_drag(0, TouchId(9), 64.0, 18.0);
let commit = picker.finish_drag(0, TouchId(9)).unwrap();
assert!(commit.selected_index() <= 3);
assert_eq!(commit.haptic_pattern(), oxide_platform_api::HapticPattern::ImpactMedium);
```

## Changelog
- 2026-03-28: replaced app-owned picker drag-end selection/haptic wiring with shared `PickerColumnCommit` results and removed the non-legacy linear tap selection path.
- 2026-03-28: replaced the wheel-only popup picker controller with legacy flat-picker column state and multi-column popup picker support, matching the old iOS picker contract.
- 2026-03-13: added Oxide-owned popup picker interaction controllers and moved generic mobile/radar picker mechanics out of Nametag shell code.
