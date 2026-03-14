use oxide_ui_core::elements::CharFilter;
use oxide_ui_core::{
    EditableText, FieldFailRestoreMode, HorizontalShiftingText, SecureText, TextFieldPolicy,
};
use std::sync::Arc;

fn username_policy() -> TextFieldPolicy {
    TextFieldPolicy::new(CharFilter::Custom(Arc::new(|ch| {
        ch.is_ascii_alphanumeric() || ch == '_'
    })))
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
    TextFieldPolicy::new(CharFilter::None)
        .with_max_length(Some(30))
        .with_lowercase(true)
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
    let mut trimmed = HorizontalShiftingText::new(bio_policy(), 32.0, 1_200).with_text("  build labs  ");
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
    let mut cleared = HorizontalShiftingText::new(username_policy(), 32.0, 1_200).with_text("pilot");
    cleared.focus();
    cleared.fail_with_message("taken", FieldFailRestoreMode::Clear);
    assert_eq!(cleared.display_text(), "taken");
    assert!(!cleared.can_interact());
    cleared.advance(HorizontalShiftingText::fail_duration_ms());
    assert_eq!(cleared.value(), "");
    assert!(!cleared.is_in_fail_mode());

    let mut restored = HorizontalShiftingText::new(username_policy(), 32.0, 1_200).with_text("pilot");
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
