use oxide_platform_api::{
    clipboard, AutoCapitalization, KeyboardAppearance, ReturnKeyType, TextContentType, TextEvent,
};
use oxide_renderer_api::{Color, DrawCmd, ImageHandle, RectF};
use oxide_ui_core::elements::{
    encode_label_text, Align, Badge, BadgeState, ButtonState, ImageUploader, Label, Overlay,
    OverlayState, PickerState, PickerStyle, PopupWindow, SliderState, SlidingSwitchMode,
    SlidingSwitchState, SlidingSwitchStyle, Spinner, TextCtx, TextInputState, TextValidation,
    ToggleState, UICameraView,
};
use oxide_ui_core::DrawListBuilder;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

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
fn overlay_default_backdrop_matches_legacy_popup_blur_base_targets() {
    let overlay = Overlay::default();
    let backdrop =
        overlay.backdrop_spec(RectF::new(0.0, 0.0, 240.0, 480.0), 1.0, 1.0).expect("backdrop");

    assert_eq!(backdrop.sigma, 18.0);
    assert_eq!(backdrop.tint, oxide_renderer_api::Color::rgba(0.0, 0.0, 0.0, 1.0));
    assert_eq!(backdrop.alpha, 0.90);
}

#[test]
fn popup_window_default_chrome_matches_legacy_popup_blur_base_targets() {
    let popup = PopupWindow::default();
    let chrome = popup.chrome(RectF::new(10.0, 20.0, 200.0, 120.0), 2.0);

    assert_eq!(chrome.panel_backdrop.sigma, 32.0);
    assert_eq!(chrome.panel_backdrop.alpha, 0.50);
    assert!((chrome.panel_radius - 18.0).abs() <= f32::EPSILON);
    assert!((chrome.border_width - 2.0).abs() <= f32::EPSILON);
    assert!((chrome.panel_inner_rect.x - 12.0).abs() <= f32::EPSILON);
    assert!((chrome.panel_inner_rect.y - 22.0).abs() <= f32::EPSILON);
    assert!((chrome.panel_inner_rect.w - 196.0).abs() <= f32::EPSILON);
    assert!((chrome.panel_inner_rect.h - 116.0).abs() <= f32::EPSILON);
    assert!((chrome.panel_inner_radius - 16.0).abs() <= f32::EPSILON);
}

#[test]
fn popup_window_encode_emits_backdrop_shell_and_inner_fill() {
    let popup = PopupWindow::default();
    let mut builder = oxide_ui_core::DrawListBuilder::new();
    popup.encode(RectF::new(10.0, 20.0, 200.0, 120.0), 1.0, &mut builder);

    assert_eq!(builder.drawlist().items.len(), 3);
    assert!(matches!(builder.drawlist().items[0], DrawCmd::Backdrop { .. }));
    assert!(matches!(builder.drawlist().items[1], DrawCmd::RRect { .. }));
    assert!(matches!(builder.drawlist().items[2], DrawCmd::RRect { .. }));
}

#[test]
fn badge_rect_matches_legacy_badgeable_button_geometry() {
    let badge = Badge::default();
    let host_rect = RectF::new(12.0, 24.0, 80.0, 80.0);

    assert_eq!(badge.style.bounce_duration_ms, 450);
    assert_eq!(badge.style.color, Color::rgba(231.0 / 255.0, 76.0 / 255.0, 60.0 / 255.0, 1.0));
    assert_eq!(badge.rect(host_rect), RectF::new(82.0, 24.0, 20.0, 20.0));
}

#[test]
fn badge_encode_prefers_image_draw_when_handle_present() {
    let badge = Badge { image: ImageHandle(91), style: Badge::default().style };
    let mut builder = oxide_ui_core::DrawListBuilder::new();
    badge.encode(RectF::new(10.0, 20.0, 80.0, 80.0), &BadgeState::default(), &mut builder);

    assert_eq!(builder.drawlist().items.len(), 1);
    match &builder.drawlist().items[0] {
        DrawCmd::Image { tex, dst, src, alpha } => {
            assert_eq!(*tex, ImageHandle(91));
            assert_eq!(*dst, RectF::new(80.0, 20.0, 20.0, 20.0));
            assert_eq!(*src, RectF::new(0.0, 0.0, 1.0, 1.0));
            assert_eq!(*alpha, 1.0);
        }
        other => panic!("expected badge image draw, got {:?}", other),
    }
}

#[test]
fn badge_encode_falls_back_to_legacy_red_circle_when_image_missing() {
    let badge = Badge::default();
    let mut builder = oxide_ui_core::DrawListBuilder::new();
    badge.encode(RectF::new(10.0, 20.0, 80.0, 80.0), &BadgeState::default(), &mut builder);

    assert_eq!(builder.drawlist().items.len(), 1);
    match &builder.drawlist().items[0] {
        DrawCmd::RRect { rect, radii, color } => {
            assert_eq!(*rect, RectF::new(80.0, 20.0, 20.0, 20.0));
            assert_eq!(*radii, [10.0; 4]);
            assert_eq!(*color, badge.style.color);
        }
        other => panic!("expected badge fallback draw, got {:?}", other),
    }
}

#[test]
fn spinner_encode_uses_rect_atom_without_caller_phase() {
    let spinner = Spinner { alpha: 0.55 };
    let mut builder = oxide_ui_core::DrawListBuilder::new();
    spinner.encode(RectF::new(10.0, 20.0, 24.0, 30.0), &mut builder);

    assert_eq!(builder.drawlist().items.len(), 1);
    match &builder.drawlist().items[0] {
        DrawCmd::Spinner { center, atom, alpha } => {
            assert_eq!(*center, [22.0, 35.0]);
            assert_eq!(*atom, 24.0);
            assert_eq!(*alpha, 0.55);
        }
        other => panic!("expected spinner draw, got {:?}", other),
    }
}

#[test]
fn sliding_switch_waits_for_legacy_long_press_before_dragging() {
    let bounds = RectF::new(0.0, 0.0, 120.0, 24.0);
    let mut state = SlidingSwitchState::default();

    assert!(state.begin_drag([12.0, 12.0], bounds));
    assert_eq!(state.mode, SlidingSwitchMode::Pressing);
    assert!(!state.drag_to([48.0, 12.0], bounds));
    assert_eq!(state.progress(bounds), 0.0);

    thread::sleep(Duration::from_millis(350));

    assert!(!state.drag_to([48.0, 12.0], bounds));
    assert_eq!(state.mode, SlidingSwitchMode::Dragging);
    assert!(state.progress(bounds) > 0.0);
}

#[test]
fn sliding_switch_triggers_at_legacy_max_offset() {
    let bounds = RectF::new(0.0, 0.0, 120.0, 24.0);
    let mut state = SlidingSwitchState::default();

    assert!(state.begin_drag([12.0, 12.0], bounds));
    thread::sleep(Duration::from_millis(350));

    assert!(state.drag_to([96.0, 12.0], bounds));
    assert_eq!(state.mode, SlidingSwitchMode::Triggered);
    assert!((state.progress(bounds) - 1.0).abs() <= f32::EPSILON);
}

#[test]
fn sliding_switch_cancels_when_pointer_leaves_bounds() {
    let bounds = RectF::new(0.0, 0.0, 120.0, 24.0);
    let mut state = SlidingSwitchState::default();

    assert!(state.begin_drag([12.0, 12.0], bounds));
    thread::sleep(Duration::from_millis(350));
    assert!(!state.drag_to([40.0, 12.0], bounds));
    assert!(state.progress(bounds) > 0.0);

    assert!(!state.drag_to([140.0, 12.0], bounds));
    assert_eq!(state.mode, SlidingSwitchMode::Idle);
    assert_eq!(state.progress(bounds), 0.0);
}

#[test]
fn sliding_switch_inactive_event_fires_once_after_timeout() {
    let style = SlidingSwitchStyle { inactive_timeout_ms: 10, ..SlidingSwitchStyle::default() };
    let mut state = SlidingSwitchState::default();

    state.start(&style);
    thread::sleep(Duration::from_millis(20));

    assert!(state.take_inactive());
    assert!(!state.take_inactive());
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
fn picker_multi_column_layout_matches_legacy_three_row_contract() {
    let style = PickerStyle::default();
    let rect = RectF::new(10.0, 20.0, 180.0, 90.0);

    assert_eq!(style.row_height(rect), 30.0);
    assert_eq!(style.center_band_rect(rect), RectF::new(10.0, 50.0, 180.0, 30.0));
    assert_eq!(style.center_band_radius(rect), 7.5);
    assert_eq!(style.column_rect(rect, 2, 0), Some(RectF::new(10.0, 20.0, 90.0, 90.0)));
    assert_eq!(style.column_rect(rect, 2, 1), Some(RectF::new(100.0, 20.0, 90.0, 90.0)));
    assert_eq!(style.item_rect(rect, 2, 0, 0.0, 0), Some(RectF::new(10.0, 50.0, 90.0, 30.0)));
    assert_eq!(style.item_rect(rect, 2, 1, 1.0, 1), Some(RectF::new(100.0, 50.0, 90.0, 30.0)));
}

#[test]
fn picker_multi_column_state_tracks_each_column_selection() {
    let mut picker = PickerState::from_columns(vec![
        vec!["One".to_string(), "Two".to_string()],
        vec!["Red".to_string(), "Blue".to_string(), "Green".to_string()],
    ]);

    assert_eq!(picker.column_count(), 2);
    assert_eq!(picker.column_selection(0), Some(0));
    assert_eq!(picker.column_selection(1), Some(0));

    assert!(picker.scroll_column(1, -1.0));
    picker.tick(0);
    assert_eq!(picker.column_selection(1), Some(1));
    assert_eq!(picker.column_selection_label(1), Some("Blue"));

    assert!(picker.set_column_selection(0, 1));
    assert_eq!(picker.column_selection_label(0), Some("Two"));
}

#[test]
fn picker_encode_no_panic() {
    let picker = PickerState::new(vec!["Red".to_string(), "Green".to_string(), "Blue".to_string()]);
    let style = PickerStyle::default();
    let mut text = oxide_ui_core::elements::TextCtx::default();
    let mut builder = oxide_ui_core::DrawListBuilder::new();
    struct DummyUploader;
    impl oxide_ui_core::elements::ImageUploader for DummyUploader {
        fn create_a8(
            &mut self,
            _w: u32,
            _h: u32,
            _data: &[u8],
            _row_bytes: usize,
        ) -> oxide_renderer_api::ImageHandle {
            oxide_renderer_api::ImageHandle(1)
        }
        fn update_a8(
            &mut self,
            _handle: oxide_renderer_api::ImageHandle,
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
        oxide_renderer_api::RectF::new(0.0, 0.0, 200.0, 180.0),
        1.0,
        &mut text,
        &mut uploader,
        &mut builder,
    );
}

#[derive(Default)]
struct CountingUploader {
    creates: usize,
    updates: usize,
    last_update: Option<(u32, u32, u32, u32, usize)>,
}

impl ImageUploader for CountingUploader {
    fn create_a8(
        &mut self,
        _w: u32,
        _h: u32,
        _data: &[u8],
        _row_bytes: usize,
    ) -> oxide_renderer_api::ImageHandle {
        self.creates += 1;
        oxide_renderer_api::ImageHandle(41)
    }

    fn update_a8(
        &mut self,
        _handle: oxide_renderer_api::ImageHandle,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        _data: &[u8],
        row_bytes: usize,
    ) {
        self.updates += 1;
        self.last_update = Some((x, y, w, h, row_bytes));
    }
}

#[test]
fn label_reuses_layout_and_skips_clean_atlas_upload() {
    let mut text = TextCtx::default();
    let _ = text.fonts.add_font(oxide_text::Font::from_bytes(
        include_bytes!("../assets/Asap-Regular.ttf").to_vec(),
    ));
    let label = Label {
        text: "Cached label".to_string(),
        color: Color::rgba(0.1, 0.1, 0.1, 1.0),
        align: Align::Left,
        wrap: true,
        font_id: 0,
        font_px: 13.0,
    };
    let mut builder = DrawListBuilder::new();
    let mut uploader = CountingUploader::default();

    label.encode(RectF::new(0.0, 0.0, 160.0, 40.0), 2.0, &mut text, &mut uploader, &mut builder);
    assert_eq!(uploader.creates, 1);
    assert_eq!(uploader.updates, 1);
    let Some((_, _, w, h, row_bytes)) = uploader.last_update else {
        panic!("expected dirty atlas upload");
    };
    assert!(w < 1024);
    assert!(h < 1024);
    assert_eq!(row_bytes, 1024);

    builder.clear();
    encode_label_text(
        "Cached label",
        Color::rgba(0.1, 0.1, 0.1, 1.0),
        Align::Left,
        true,
        0,
        13.0,
        RectF::new(0.0, 0.0, 160.0, 40.0),
        2.0,
        &mut text,
        &mut uploader,
        &mut builder,
    );
    assert_eq!(uploader.creates, 1);
    assert_eq!(uploader.updates, 1);
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

    st.set_text("aé日b");
    st.set_selection(1, 3);
    assert!(st.copy_selection_to_clipboard());
    assert_eq!(clipboard::read_string(), Some(String::from("é日")));
    assert!(st.cut_selection_to_clipboard());
    assert_eq!(st.text(), "ab");
}

#[test]
fn camera_view_encodes_draw_cmd() {
    let mut dl = DrawListBuilder::new();
    let rect = RectF::new(0.0, 0.0, 320.0, 240.0);
    let cam = UICameraView {
        tint: Color::rgba(0.8, 0.7, 0.6, 0.5),
        alpha: 0.75,
        grayscale: true,
        blur: true,
        sigma: 8.0,
    };
    cam.encode(rect, &mut dl);
    let items = dl.drawlist().items.clone();
    assert_eq!(items.len(), 1);
    match &items[0] {
        DrawCmd::CameraBg { rect: r, tint, alpha, grayscale, blur, sigma } => {
            assert_eq!(r.x, rect.x);
            assert_eq!(r.y, rect.y);
            assert_eq!(r.w, rect.w);
            assert_eq!(r.h, rect.h);
            assert!((*alpha - 0.75).abs() < 1e-6);
            assert!(*grayscale);
            assert!(*blur);
            assert!((*sigma - 8.0).abs() < 1e-6);
            assert!((*tint).r > 0.79 && (*tint).g > 0.69 && (*tint).b > 0.59);
        }
        _ => panic!("expected CameraBg draw command"),
    }
}

#[test]
fn button_press_release_tap() {
    let mut st = ButtonState::default();
    st.on_pointer_down();
    assert!(st.is_pressed());
    let tapped = st.on_pointer_up();
    assert!(tapped);
    assert!(!st.is_pressed());
}

#[test]
fn toggle_drag_and_tap() {
    let mut st = ToggleState::default();
    st.begin_drag(0.0);
    st.drag_to(50.0, RectF::new(0.0, 0.0, 100.0, 20.0));
    st.end_drag();
    assert!(st.on);
    st.on_tap();
    assert!(!st.on);
}

#[test]
fn slider_keyboard_adjust() {
    let mut st = SliderState::default();
    st.set(0.5, Some(0.1));
    st.arrow_right(Some(0.1));
    assert!((st.value - 0.6).abs() < 1e-6);
    st.arrow_left(Some(0.1));
    assert!((st.value - 0.5).abs() < 1e-6);
}
