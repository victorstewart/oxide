use oxide_ui_core::ScrollState;

#[test]
fn offset_is_zero_when_content_fits_viewport() {
    let mut state = ScrollState::new(200.0, 400.0);
    assert_eq!(state.max_offset(), 0.0);
    assert_eq!(state.set_offset(100.0), 0.0);
    assert_eq!(state.scroll_by(25.0), 0.0);
}

#[test]
fn progress_is_clamped_to_unit_interval() {
    let mut state = ScrollState::new(1_200.0, 300.0);
    assert_eq!(state.set_offset(9_999.0), state.max_offset());
    assert!((state.progress() - 1.0).abs() < f32::EPSILON);
    assert_eq!(state.set_offset(-5.0), 0.0);
    assert!((state.progress() - 0.0).abs() < f32::EPSILON);
}
