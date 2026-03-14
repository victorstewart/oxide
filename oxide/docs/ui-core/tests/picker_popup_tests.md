# ui-core tests `picker_popup_tests.rs`

## Intention and purpose
- Verify that the shared popup and wheel-picker interaction controllers preserve the drag/snap/dismiss behavior that downstream apps depend on.

## Relation to the rest of the code
- Exercises `ui-core::picker_popup`.
- Serves as the framework-level regression suite for Nametag radar/mobile picker mechanics.

## Entry points list
- `panel_popup_classifies_panel_and_outside_taps`
- `wheel_picker_drag_updates_position_and_snaps_with_legacy_rounding`
- `wheel_picker_clamps_overscroll_and_linear_taps_to_valid_indices`
- `popup_wheel_picker_tracks_popup_visibility_and_drag_lifecycle`

## Logic narrative
- The suite first checks popup inside/outside classification.
- It then verifies the legacy picker snap threshold and drag lifecycle.
- The remaining cases cover overscroll clamping, tap-to-index mapping, and the combined popup+picker wrapper.

## Preconditions and postconditions
- Tests build the controllers only through the public API.
- Passing results mean app wrappers can rely on the shared Oxide mechanics without carrying scene-local duplicates.

## Edge cases and failure modes
- Overscroll bounds, mismatched drag completion, and outside-tap detection are all covered directly or indirectly.

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
- 2026-03-13: added regression coverage for the popup/wheel-picker mechanics moved into Oxide.
