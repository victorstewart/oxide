use oxide_platform_api::TouchId;
use oxide_renderer_api::RectF;
use oxide_ui_core::{PanelPopupState, PopupTapRegion, PopupWheelPickerState, WheelPickerState};

#[test]
fn panel_popup_classifies_panel_and_outside_taps()
{
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
fn wheel_picker_drag_updates_position_and_snaps_with_legacy_rounding()
{
   let mut picker = WheelPickerState::new(4, 0).with_overscroll_rows(0.35);
   picker.begin_drag(TouchId(7), 100.0);
   assert!(picker.update_drag(TouchId(7), 49.0, 34.0));
   assert!(picker.position() > 1.49 && picker.position() < 1.51);
   assert_eq!(picker.finish_drag(TouchId(7)), Some(1));
   assert_eq!(picker.selected_index(), 1);

   picker.begin_drag(TouchId(8), 100.0);
   assert!(picker.update_drag(TouchId(8), 49.2, 34.0));
   assert_eq!(picker.finish_drag(TouchId(8)), Some(2));
   assert_eq!(picker.selected_index(), 2);
}

#[test]
fn wheel_picker_clamps_overscroll_and_linear_taps_to_valid_indices()
{
   let mut picker = WheelPickerState::new(5, 2).with_overscroll_rows(0.25);
   picker.begin_drag(TouchId(11), 120.0);
   assert!(picker.update_drag(TouchId(11), -400.0, 20.0));
   assert!(picker.position() <= 4.25);
   assert_eq!(picker.finish_drag(TouchId(11)), Some(4));

   picker.sync_to_index(2);
   let surface = RectF::new(40.0, 80.0, 100.0, 36.0);
   assert_eq!(picker.index_for_linear_tap(surface, 18.0, surface.y), 1);
   assert_eq!(picker.index_for_linear_tap(surface, 18.0, surface.y + surface.h * 0.50), 2);
   assert_eq!(picker.index_for_linear_tap(surface, 18.0, surface.y + surface.h), 3);
}

#[test]
fn popup_wheel_picker_tracks_popup_visibility_and_drag_lifecycle()
{
   let mut picker = PopupWheelPickerState::new(3, 1).with_overscroll_rows(0.35);
   assert!(!picker.is_open());
   picker.open();
   assert!(picker.is_open());
   assert_eq!(picker.classify_panel_tap(RectF::new(0.0, 0.0, 40.0, 40.0), 8.0, 8.0), PopupTapRegion::Panel);
   assert_eq!(picker.classify_panel_tap(RectF::new(0.0, 0.0, 40.0, 40.0), 80.0, 8.0), PopupTapRegion::Outside);

   picker.begin_drag(TouchId(3), 50.0);
   assert!(picker.is_dragging());
   assert_eq!(picker.drag_touch_id(), Some(TouchId(3)));
   assert!(picker.update_drag(TouchId(3), 20.0, 20.0));
   assert_eq!(picker.finish_drag(TouchId(3)), Some(2));
   assert!(!picker.is_dragging());

   picker.close();
   assert!(!picker.is_open());
   assert!(!picker.is_dragging());
}
