# ui-core `tests/overlay_tests.rs`

## Intention and purpose

This test file locks the shared popup lifecycle contract in place. It exists so `PopupManager` cannot regress back to a generic overlay wrapper without breaking explicit assertions around key-popup lookup, dismissal approval, touch exceptions, and content-size refresh behavior.

## Covered behavior

- `overlay_background_tap_dismisses`
  Confirms the generic `OverlayStack` release-time background dismissal behavior remains intact.
- `popup_z_order_prefers_topmost`
  Confirms popup input routing still prefers the highest-`z_index` popup.
- `popup_key_window_tracks_topmost_popup`
  Confirms `key_popup()`, `popup_is_key_window()`, and `focus_target()` follow the top popup.
- `popup_dismissal_obeys_approve_dismissal_and_runs_once`
  Confirms dismissal approval can veto removal and that the dismissal callback runs only once when approval later succeeds.
- `popup_pointer_dismisses_outside_touch_region_on_press`
  Confirms popup touch-exception dismissal happens immediately on an outside press when a dismissal path exists.
- `popup_approve_touch_can_veto_inside_touches`
  Confirms `approve_touch` can reject an otherwise in-bounds touch and trigger dismissal.
- `popup_content_size_changed_refreshes_content_touch_region`
  Confirms `content_size_changed()` re-layouts the popup and refreshes the default content-root touch region.
- `popup_manual_touch_region_overrides_content_root_resync`
  Confirms a manual touch rectangle survives later content-size resyncs.

## Why this matters

The old popup-window behavior is a subtle lifecycle contract, not just a draw primitive. These tests protect the behaviors that higher-level app code depends on when it treats the top popup like a key window with approval-gated dismissal and touch-exception routing.

## Changelog

- 2026-03-28: added popup lifecycle regression coverage for key-popup lookup, approval-gated dismissal, touch veto routing, and content-size resync.
