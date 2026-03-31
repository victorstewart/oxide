use oxide_platform_api::{HapticPattern, TouchId};
use oxide_renderer_api::RectF;
use oxide_ui_core::{PanelPopupState, PickerColumnState, PopupPickerState, PopupTapRegion};

#[test]
fn panel_popup_classifies_panel_and_outside_taps() {
    let mut popup = PanelPopupState::new();
    assert!(!popup.is_open());
    popup.open();
    assert!(popup.is_open());

    let panel = RectF::new(20.0, 40.0, 120.0, 60.0);
    assert_eq!(popup.classify_tap(panel, 80.0, 70.0), PopupTapRegion::Panel);
    assert_eq!(popup.classify_tap(panel, 10.0, 10.0), PopupTapRegion::Outside);

    popup.close();
    assert!(!popup.is_open());
}

#[test]
fn picker_column_drag_updates_position_and_snaps_with_legacy_rounding() {
    let mut picker = PickerColumnState::new(4, 0);
    picker.begin_drag(TouchId(7), 100.0);
    assert!(picker.update_drag(TouchId(7), 49.0, 34.0));
    assert!(picker.position() > 1.49 && picker.position() < 1.51);
    let commit = picker.finish_drag(TouchId(7)).expect("commit");
    assert_eq!(commit.selected_index(), 1);
    assert_eq!(commit.haptic_pattern(), HapticPattern::ImpactMedium);
    assert_eq!(picker.selected_index(), 1);

    picker.begin_drag(TouchId(8), 100.0);
    assert!(picker.update_drag(TouchId(8), 49.2, 34.0));
    let commit = picker.finish_drag(TouchId(8)).expect("commit");
    assert_eq!(commit.selected_index(), 2);
    assert_eq!(commit.haptic_pattern(), HapticPattern::ImpactMedium);
    assert_eq!(picker.selected_index(), 2);
}

#[test]
fn picker_column_clamps_drag_to_valid_indices() {
    let mut picker = PickerColumnState::new(5, 2);
    picker.begin_drag(TouchId(11), 120.0);
    assert!(picker.update_drag(TouchId(11), -400.0, 20.0));
    assert_eq!(picker.position(), 4.0);
    let commit = picker.finish_drag(TouchId(11)).expect("commit");
    assert_eq!(commit.selected_index(), 4);
}

#[test]
fn popup_picker_tracks_popup_visibility_and_drag_lifecycle() {
    let mut picker = PopupPickerState::new(3, 1);
    assert!(!picker.is_open());
    picker.open();
    assert!(picker.is_open());
    assert_eq!(
        picker.classify_panel_tap(RectF::new(0.0, 0.0, 40.0, 40.0), 8.0, 8.0),
        PopupTapRegion::Panel
    );
    assert_eq!(
        picker.classify_panel_tap(RectF::new(0.0, 0.0, 40.0, 40.0), 80.0, 8.0),
        PopupTapRegion::Outside
    );

    assert!(picker.begin_drag(0, TouchId(3), 50.0));
    assert!(picker.is_dragging(0));
    assert_eq!(picker.drag_touch_id(0), Some(TouchId(3)));
    assert!(picker.update_drag(0, TouchId(3), 20.0, 20.0));
    let commit = picker.finish_drag(0, TouchId(3)).expect("commit");
    assert_eq!(commit.selected_index(), 2);
    assert_eq!(commit.haptic_pattern(), HapticPattern::ImpactMedium);
    assert!(!picker.is_dragging(0));

    picker.close();
    assert!(!picker.is_open());
    assert!(!picker.is_dragging(0));
}

#[test]
fn popup_picker_supports_independent_multi_column_selection() {
    let mut picker = PopupPickerState::from_columns(vec![3, 4], vec![1, 2]);
    picker.open();

    assert_eq!(picker.column_count(), 2);
    assert_eq!(picker.selected_index(0), Some(1));
    assert_eq!(picker.selected_index(1), Some(2));

    assert!(picker.begin_drag(1, TouchId(4), 100.0));
    assert!(picker.update_drag(1, TouchId(4), 69.0, 30.0));
    let commit = picker.finish_drag(1, TouchId(4)).expect("commit");
    assert_eq!(commit.selected_index(), 3);
    assert_eq!(commit.haptic_pattern(), HapticPattern::ImpactMedium);
    assert_eq!(picker.selected_index(1), Some(3));
    assert_eq!(picker.selected_index(0), Some(1));
}

#[test]
fn picker_column_commit_carries_legacy_medium_haptic_intent() {
    let mut picker = PickerColumnState::new(3, 1);
    picker.begin_drag(TouchId(19), 100.0);
    assert!(picker.update_drag(TouchId(19), 70.0, 30.0));
    let commit = picker.finish_drag(TouchId(19)).expect("commit");
    assert_eq!(commit.selected_index(), 2);
    assert_eq!(commit.haptic_pattern(), HapticPattern::ImpactMedium);
}
