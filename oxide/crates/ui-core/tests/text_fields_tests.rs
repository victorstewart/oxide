use oxide_renderer_api::{Color, RectF};
use oxide_ui_core::elements::CharFilter;
use oxide_ui_core::{
    bitmap_text::{text_width, TextStyle},
    single_line_text_selection_highlight_layout, single_line_text_selection_index_for_x,
    single_line_text_selection_rect, text_input_option_at, text_input_options_layout,
    text_selection_drag_anchor_at, text_word_range_at_char_index, EditableText,
    FieldFailRestoreMode, HorizontalShiftingText, SecureText, TextFieldPolicy, TextInputOption,
    TextInputOptionsConfig, TextSelectionDragAnchor, TextSelectionDragState,
    TextSelectionHighlightStyle, TextSelectionState, TextTapMemory,
};
use std::sync::Arc;

fn username_policy() -> TextFieldPolicy {
    TextFieldPolicy::new(CharFilter::Custom(Arc::new(|ch| ch.is_ascii_alphanumeric() || ch == '_')))
        .with_max_length(Some(15))
        .with_lowercase(true)
        .with_first_token_only_on_set(true)
}

fn login_username_policy() -> TextFieldPolicy {
    TextFieldPolicy::new(CharFilter::Custom(Arc::new(|ch| {
        ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '$')
    })))
    .with_max_length(Some(15))
    .with_lowercase(true)
    .with_first_token_only_on_set(true)
}

fn bio_policy() -> TextFieldPolicy {
    TextFieldPolicy::new(CharFilter::AlphanumericPlus(String::from("@-&(),.' +")))
        .with_max_length(Some(30))
}

fn login_password_policy() -> TextFieldPolicy {
    TextFieldPolicy::new(CharFilter::None).with_max_length(Some(30)).with_lowercase(true)
}

#[test]
fn text_fields_editable_rejects_invalid_and_overflow_commits_without_partial_changes() {
    let mut invalid = EditableText::new(login_username_policy());
    invalid.set("victor");
    invalid.apply_commit("A B");
    assert_eq!(invalid.value(), "victor");

    let mut overflow = EditableText::new(login_username_policy());
    overflow.set("abcdefghijklmn");
    overflow.apply_commit("OP");
    assert_eq!(overflow.value(), "abcdefghijklmn");
}

#[test]
fn text_fields_horizontal_set_uses_external_normalization_policy() {
    let mut text = HorizontalShiftingText::new(username_policy(), 48.0, 1_200);
    text.set_text("Oxide UI!!");
    assert_eq!(text.text(), "oxide");
}

#[test]
fn text_fields_editable_set_keeps_generic_sanitization_without_external_token_split() {
    let mut text = EditableText::new(username_policy());
    text.set("user name!@#");
    assert_eq!(text.value(), "username");
}

#[test]
fn text_fields_horizontal_blur_trims_finished_text_and_clears_blank_values() {
    let mut trimmed =
        HorizontalShiftingText::new(bio_policy(), 32.0, 1_200).with_text("  build labs  ");
    trimmed.focus();
    trimmed.blur();
    assert_eq!(trimmed.value(), "build labs");
    assert!(!trimmed.is_focused());

    let mut cleared = HorizontalShiftingText::new(bio_policy(), 32.0, 1_200).with_text("   ");
    cleared.focus();
    cleared.blur();
    assert_eq!(cleared.value(), "");
    assert!(!cleared.is_focused());
}

#[test]
fn text_fields_horizontal_fail_modes_clear_and_restore_value() {
    let mut cleared =
        HorizontalShiftingText::new(username_policy(), 32.0, 1_200).with_text("pilot");
    cleared.focus();
    cleared.fail_with_message("taken", FieldFailRestoreMode::Clear);
    assert_eq!(cleared.display_text(), "taken");
    assert!(!cleared.can_interact());
    cleared.advance(HorizontalShiftingText::fail_duration_ms());
    assert_eq!(cleared.value(), "");
    assert!(!cleared.is_in_fail_mode());

    let mut restored =
        HorizontalShiftingText::new(username_policy(), 32.0, 1_200).with_text("pilot");
    restored.fail_with_message("checking", FieldFailRestoreMode::RestoreValue);
    restored.advance(HorizontalShiftingText::fail_duration_ms());
    assert_eq!(restored.value(), "pilot");
    assert!(!restored.is_in_fail_mode());
}

#[test]
fn text_fields_horizontal_supports_caret_movement_and_mid_string_insertions() {
    let mut text = HorizontalShiftingText::new(bio_policy(), 32.0, 1_200).with_text("Victor");
    text.focus();
    text.move_caret_left();
    text.move_caret_left();
    assert_eq!(text.caret_index(), 4);
    text.apply_commit(" X");
    assert_eq!(text.value(), "Vict Xor");
    assert_eq!(text.caret_index(), 6);
    text.apply_commit("\u{8}");
    assert_eq!(text.value(), "Vict or");
    assert_eq!(text.caret_index(), 5);
}

#[test]
fn text_fields_horizontal_replaces_selected_char_range_with_policy_filtering() {
    let mut text = HorizontalShiftingText::new(bio_policy(), 32.0, 1_200).with_text("Victor Lee");

    assert!(text.replace_char_range(7..10, "Stewart!!!"));
    assert_eq!(text.value(), "Victor Stewart");
    assert_eq!(text.caret_index(), 14);

    assert_eq!(text.apply_selection_commit(0..6, "\u{8}"), Some(true));
    assert_eq!(text.value(), " Stewart");
    assert_eq!(text.caret_index(), 0);
}

#[test]
fn text_fields_text_selection_helpers_find_words_and_rects() {
    assert_eq!(text_word_range_at_char_index("Victor Lee", 1), 0..6);
    assert_eq!(text_word_range_at_char_index("Victor Lee", 7), 7..10);
    assert_eq!(text_word_range_at_char_index("Victor Lee", 6), 0..6);
    assert_eq!(text_word_range_at_char_index("Victor Lee", 100), 7..10);

    let selection = TextSelectionState::new("name", 7..10);
    assert!(selection.is_active());

    let style = TextStyle::new(24.0, Color::rgba(1.0, 1.0, 1.0, 1.0));
    let rect = single_line_text_selection_rect(
        RectF::new(10.0, 20.0, 200.0, 40.0),
        12.0,
        "Victor Lee",
        style,
        selection.range,
    )
    .expect("selected word should produce a highlight rect");
    assert!(rect.x > 12.0);
    assert!(rect.w > 1.0);
    assert!(rect.h > 1.0);

    let highlight = TextSelectionHighlightStyle {
        fill: Color::rgba(0.10, 0.02, 0.08, 1.0),
        border: Color::rgba(1.0, 0.20, 0.46, 1.0),
        selected_text: Color::rgba(1.0, 0.20, 0.46, 1.0),
        border_px: 1.0,
        y_pad: 2.0,
        radius_px: 7.0,
    };
    let highlight_layout = single_line_text_selection_highlight_layout(
        RectF::new(10.0, 20.0, 200.0, 40.0),
        12.0,
        "Victor Lee",
        style,
        7..10,
        highlight,
    )
    .expect("selected word should produce a token layout");
    assert!((highlight_layout.token_rect.x - rect.x).abs() < 0.001);
    assert!((highlight_layout.token_rect.w - rect.w).abs() < 0.001);
    assert!(highlight_layout.token_rect.h > rect.h);
    assert_eq!(
        text_selection_drag_anchor_at(
            highlight_layout,
            highlight_layout.token_rect.x + 1.0,
            highlight_layout.token_rect.y + highlight_layout.token_rect.h * 0.50,
            4.0,
        ),
        Some(TextSelectionDragAnchor::Start)
    );
    assert_eq!(
        text_selection_drag_anchor_at(
            highlight_layout,
            highlight_layout.token_rect.x + highlight_layout.token_rect.w - 1.0,
            highlight_layout.token_rect.y + highlight_layout.token_rect.h * 0.50,
            4.0,
        ),
        Some(TextSelectionDragAnchor::End)
    );
    assert_eq!(
        text_selection_drag_anchor_at(
            highlight_layout,
            highlight_layout.token_rect.x - 12.0,
            highlight_layout.token_rect.y,
            4.0,
        ),
        None
    );

    let text_x = 100.0;
    let s_width = text_width("S", style);
    let st_width = text_width("St", style);
    let ste_width = text_width("Ste", style);
    let e_body_x = text_x + st_width + (ste_width - st_width) * 0.25;
    assert_eq!(
        single_line_text_selection_index_for_x(
            text_x,
            "Stewart",
            style,
            e_body_x,
            TextSelectionDragAnchor::End,
        ),
        3
    );
    assert_eq!(
        single_line_text_selection_index_for_x(
            text_x,
            "Stewart",
            style,
            text_x + s_width + (st_width - s_width) * 0.25,
            TextSelectionDragAnchor::Start,
        ),
        1
    );

    let mut drag = TextSelectionDragState::new(
        oxide_platform_api::TouchId(1),
        "name",
        TextSelectionDragAnchor::End,
        0,
        6,
    );
    assert_eq!(drag.range(), 0..6);
    drag.update_current_index(10);
    assert_eq!(drag.range(), 0..10);
    drag.update_current_index(3);
    assert_eq!(drag.range(), 0..3);
}

#[test]
fn text_fields_tap_memory_detects_same_field_double_taps_only() {
    let tap = TextTapMemory::new("first", 20.0, 40.0, 1_000);

    assert!(tap.is_double_tap("first", 22.0, 43.0, 1_200, 300, 8.0));
    assert!(!tap.is_double_tap("last", 22.0, 43.0, 1_200, 300, 8.0));
    assert!(!tap.is_double_tap("first", 22.0, 43.0, 1_400, 300, 8.0));
    assert!(!tap.is_double_tap("first", 40.0, 43.0, 1_200, 300, 8.0));
}

#[test]
fn text_fields_input_options_layout_matches_field_width_and_hit_tests_options() {
    let field = RectF::new(40.0, 80.0, 120.0, 44.0);
    let layout = text_input_options_layout(
        field,
        RectF::new(0.0, 0.0, 320.0, 480.0),
        1.0,
        TextInputOptionsConfig::all(),
        10.6,
    )
    .expect("enabled options should lay out");

    let style = TextStyle::new(10.6, Color::rgba(1.0, 1.0, 1.0, 1.0)).bold();
    let select_all_w = text_width(TextInputOption::SelectAll.label(), style);
    let paste_w = text_width(TextInputOption::Paste.label(), style);
    let padding = 10.0;
    assert!((layout.bubble_rect.w - (select_all_w + paste_w + padding * 4.0)).abs() < 0.001);
    assert_eq!(layout.bubble_rect.h, 30.0);
    assert!(
        (layout.bubble_rect.x - (field.x + field.w * 0.50 - layout.bubble_rect.w * 0.50)).abs()
            < 0.001
    );
    let select_all = layout.select_all_rect.expect("select all rect");
    let paste = layout.paste_rect.expect("paste rect");
    assert!((select_all.w - (select_all_w + padding * 2.0)).abs() < 0.001);
    assert!((paste.w - (paste_w + padding * 2.0)).abs() < 0.001);
    assert!((select_all.x - layout.bubble_rect.x).abs() < 0.001);
    assert!((paste.x - (select_all.x + select_all.w)).abs() < 0.001);
    assert!((paste.x + paste.w - (layout.bubble_rect.x + layout.bubble_rect.w)).abs() < 0.001);
    assert_eq!(
        text_input_option_at(
            layout,
            select_all.x + select_all.w * 0.50,
            layout.bubble_rect.y + 8.0
        ),
        Some(TextInputOption::SelectAll)
    );
    assert_eq!(
        text_input_option_at(layout, paste.x + paste.w * 0.50, layout.bubble_rect.y + 8.0),
        Some(TextInputOption::Paste)
    );
    assert_eq!(text_input_option_at(layout, 10.0, layout.bubble_rect.y + 8.0), None);

    let oversized = text_input_options_layout(
        RectF::new(10.0, 40.0, 500.0, 30.0),
        RectF::new(0.0, 0.0, 320.0, 480.0),
        1.0,
        TextInputOptionsConfig { select_all: true, paste: false },
        10.6,
    )
    .expect("oversized fields should still produce a layout");
    assert!(oversized.bubble_rect.w > select_all_w);
    assert!(oversized.bubble_rect.x <= 320.0 - oversized.bubble_rect.w - 8.0 + 0.001);
}

#[test]
fn text_fields_secure_reveals_then_re_masks_and_bypasses_masking_for_failures() {
    let mut secure = SecureText::new(EditableText::new(login_password_policy()));
    secure.focus();
    secure.apply_commit("secret");
    assert_eq!(secure.display_text(), "secret");
    secure.advance(999);
    assert_eq!(secure.display_text(), "secret");
    secure.advance(1);
    assert_eq!(secure.display_text(), "******");

    secure.fail_with_message("invalid", FieldFailRestoreMode::Clear);
    assert_eq!(secure.display_text(), "invalid");
    secure.advance(HorizontalShiftingText::fail_duration_ms());
    assert_eq!(secure.display_text(), "");
    assert!(!secure.is_in_fail_mode());
}
