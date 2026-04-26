use oxide_ui_core::ScrollState;

#[test]
fn offset_is_zero_when_content_fits_viewport() {
    let mut state = ScrollState::new(200.0, 400.0);
    assert_eq!(state.max_offset(), 0.0);
    assert_eq!(state.set_offset(100.0), 0.0);
    assert_eq!(state.scroll_by(25.0), 0.0);
}

#[test]
fn scroll_state_clamps_to_bounds() {
    let mut state = ScrollState::new(800.0, 300.0);
    assert_eq!(state.max_offset(), 500.0);
    assert_eq!(state.scroll_by(200.0), 200.0);
    assert_eq!(state.scroll_by(700.0), 500.0);
    assert_eq!(state.scroll_by(-900.0), 0.0);
}

#[test]
fn progress_is_clamped_to_unit_interval() {
    let mut state = ScrollState::new(1_200.0, 300.0);
    assert_eq!(state.set_offset(9_999.0), state.max_offset());
    assert!((state.progress() - 1.0).abs() < f32::EPSILON);
    assert_eq!(state.set_offset(-5.0), 0.0);
    assert!((state.progress() - 0.0).abs() < f32::EPSILON);
}

#[test]
fn scroll_state_progress_tracks_extent_and_rejects_non_finite_values() {
    let mut state = ScrollState::new(900.0, 300.0);
    state.set_offset(300.0);
    assert!((state.progress() - 0.5).abs() < 0.0001);

    state.update_extents(200.0, 400.0);
    assert_eq!(state.max_offset(), 0.0);
    assert_eq!(state.offset(), 0.0);
    assert_eq!(state.progress(), 0.0);

    let mut invalid = ScrollState::new(f32::NAN, f32::INFINITY);
    assert_eq!(invalid.max_offset(), 0.0);
    assert_eq!(invalid.offset(), 0.0);

    invalid.update_extents(500.0, 100.0);
    invalid.set_offset(f32::NAN);
    assert_eq!(invalid.offset(), 0.0);
    invalid.scroll_by(f32::INFINITY);
    assert_eq!(invalid.offset(), 0.0);
}
