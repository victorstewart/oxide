use oxideui_platform_api::{
    clipboard, AutoCapitalization, KeyboardAppearance, ReturnKeyType, TextContentType, TextEvent,
};
use oxideui_ui_core::elements::{
    OverlayState, PickerState, PickerStyle, TextInputState, TextValidation,
};
use std::sync::{Arc, RwLock};

#[test]
fn text_input_validates_and_commits() {
    let mut st = TextInputState::new("Username");
    st.set_validator(|t| t.chars().count() >= 3);
    assert_eq!(st.validation(), TextValidation::Pending);

    st.set_text("ab");
    assert_eq!(st.validation(), TextValidation::Invalid);

    st.focus();
    st.move_cursor_to_end();
    st.handle_text_event(&TextEvent::Commit { text: "c".into() });
    assert_eq!(st.validation(), TextValidation::Valid);
    assert_eq!(st.text(), "abc");

    st.handle_text_event(&TextEvent::Commit { text: "\n".into() });
    assert!(st.take_submit());
    assert!(!st.take_submit());
}

#[test]
fn overlay_animates_open_close() {
    let mut overlay = OverlayState::new();
    assert!(!overlay.is_visible());
    overlay.open();
    overlay.tick(32);
    assert!(overlay.progress() > 0.0);
    overlay.close();
    overlay.tick(180);
    assert!(overlay.progress() <= 1.0);
}

#[test]
fn picker_selection_tracks_scroll() {
    let mut picker =
        PickerState::new(vec!["Alpha".to_string(), "Beta".to_string(), "Gamma".to_string()]);
    assert_eq!(picker.selection(), 0);
    picker.scroll(-1.0);
    picker.tick(0);
    assert_eq!(picker.selection(), 1);
    picker.scroll(-1.0);
    picker.tick(0);
    assert_eq!(picker.selection(), 2);

    picker.set_items(vec!["Solo".to_string()]);
    assert_eq!(picker.selection(), 0);
    assert_eq!(picker.selection_label(), Some("Solo"));
}

#[test]
fn picker_encode_no_panic() {
    let picker = PickerState::new(vec!["Red".to_string(), "Green".to_string(), "Blue".to_string()]);
    let style = PickerStyle::default();
    let mut text = oxideui_ui_core::elements::TextCtx::default();
    let mut builder = oxideui_ui_core::DrawListBuilder::new();
    struct DummyUploader;
    impl oxideui_ui_core::elements::ImageUploader for DummyUploader {
        fn create_a8(
            &mut self,
            _w: u32,
            _h: u32,
            _data: &[u8],
            _row_bytes: usize,
        ) -> oxideui_renderer_api::ImageHandle {
            oxideui_renderer_api::ImageHandle(1)
        }
        fn update_a8(
            &mut self,
            _handle: oxideui_renderer_api::ImageHandle,
            _x: u32,
            _y: u32,
            _w: u32,
            _h: u32,
            _data: &[u8],
            _row_bytes: usize,
        ) {
        }
    }
    let mut uploader = DummyUploader;
    picker.encode(
        &style,
        oxideui_renderer_api::RectF::new(0.0, 0.0, 200.0, 180.0),
        1.0,
        &mut text,
        &mut uploader,
        &mut builder,
    );
}

#[derive(Default)]
struct MemoryClipboard {
    buf: RwLock<Option<String>>,
}

impl clipboard::ClipboardProvider for MemoryClipboard {
    fn read_string(&self) -> Option<String> {
        self.buf.read().ok().and_then(|g| g.clone())
    }

    fn write_string(&self, value: &str) {
        if let Ok(mut guard) = self.buf.write() {
            *guard = Some(value.to_owned());
        }
    }
}

#[test]
fn text_input_keyboard_config_mutations() {
    let mut st = TextInputState::new("Email");
    st.set_autocorrect(false);
    st.set_autocapitalization(AutoCapitalization::Words);
    st.set_keyboard_appearance(KeyboardAppearance::Dark);
    st.set_return_key(ReturnKeyType::Done);
    st.set_content_type(TextContentType::Email);
    let cfg = st.keyboard_config();
    assert!(!cfg.autocorrect);
    assert_eq!(cfg.autocapitalization, AutoCapitalization::Words);
    assert_eq!(cfg.keyboard, KeyboardAppearance::Dark);
    assert_eq!(cfg.return_key, ReturnKeyType::Done);
    assert_eq!(cfg.content_type, TextContentType::Email);
}

#[test]
fn text_input_filter_and_length() {
    let mut st = TextInputState::new("Code");
    st.set_max_length(Some(4));
    st.set_filter_digits();
    st.focus();
    st.handle_text_event(&TextEvent::Commit { text: "12ab34".into() });
    assert_eq!(st.text(), "1234");
    st.set_filter_alphanumeric();
    st.set_text("9a");
    let len = st.text().chars().count() as u32;
    st.handle_text_event(&TextEvent::SelectionChanged { range: len..len });
    st.handle_text_event(&TextEvent::Commit { text: "b7?".into() });
    assert_eq!(st.text(), "9ab7");
}

#[test]
fn text_input_one_time_code_setup() {
    let mut st = TextInputState::new("OTP");
    st.configure_one_time_code(6);
    assert_eq!(st.keyboard_config().content_type, TextContentType::OneTimeCode);
    st.focus();
    st.handle_text_event(&TextEvent::Commit { text: "12-34".into() });
    assert_eq!(st.text(), "1234");
    assert_eq!(st.max_length(), Some(6));
    assert!(st.otp_config().is_some());
}

#[test]
fn text_input_clipboard_roundtrip() {
    clipboard::set_clipboard_provider(Arc::new(MemoryClipboard::default()));
    let mut st = TextInputState::new("Username");
    st.set_text("oxide");
    st.set_selection(0, 5);
    assert!(st.copy_selection_to_clipboard());
    assert_eq!(clipboard::read_string(), Some(String::from("oxide")));
    assert!(st.cut_selection_to_clipboard());
    assert_eq!(st.text(), "");
    st.focus();
    assert!(st.paste_from_clipboard());
    assert_eq!(st.text(), "oxide");
}
