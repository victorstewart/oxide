# ui-core tests `picker_popup_tests.rs`

## Intention and purpose
- Verify that the shared popup and picker-column interaction controllers preserve the legacy drag/snap/commit behavior that downstream apps depend on.

## Relation to the rest of the code
- Exercises `ui-core::picker_popup`.
- Serves as the framework-level regression suite for Nametag radar/mobile picker mechanics.

## Entry points list
- `panel_popup_classifies_panel_and_outside_taps`
- `picker_column_drag_updates_position_and_snaps_with_legacy_rounding`
- `picker_column_clamps_drag_to_valid_indices`
- `popup_picker_tracks_popup_visibility_and_drag_lifecycle`
- `picker_column_commit_carries_legacy_medium_haptic_intent`

## Logic narrative
- The suite first checks popup inside/outside classification.
- It then verifies the legacy picker snap threshold and drag lifecycle.
- The remaining cases cover clamped drag commits, fixed medium-impact commit intent, and the combined popup+picker wrapper.

## Preconditions and postconditions
- Tests build the controllers only through the public API.
- Passing results mean app wrappers can rely on the shared Oxide mechanics without carrying scene-local duplicates.

## Edge cases and failure modes
- Drag bounds, fixed haptic intent, mismatched drag completion, and outside-tap detection are all covered directly or indirectly.

## Concurrency and memory behavior
- Tests are single-threaded and deterministic.

## Performance notes
- This suite targets correctness, not benchmarking.

## Feature flags and cfgs
- No feature-specific branches.

## Testing and benchmarks
- Run with `cargo test --locked -p oxide-ui-core --test picker_popup_tests`.

## Examples
```rust
cargo test --locked -p oxide-ui-core --test picker_popup_tests
```

## Changelog
- 2026-03-28: updated the suite for the shared `PickerColumnCommit` contract and removed the non-legacy linear tap selection path.
- 2026-03-13: added regression coverage for the popup/wheel-picker mechanics moved into Oxide.
