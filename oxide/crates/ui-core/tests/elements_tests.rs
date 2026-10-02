use oxide_platform_api::{
    clipboard, AutoCapitalization, KeyCode, KeyEvent, KeyboardAppearance, Modifiers, ReturnKeyType,
    TextContentType, TextEvent,
};
use oxide_renderer_api::{Color, DrawCmd, ImageHandle, RectF};
use oxide_ui_core::elements::{
    encode_label_text, encode_label_text_profiled, Align, Badge, BadgeState, ButtonState, ImageFit,
    ImageRegionView, ImageUploader, ImageView, ImageZoomState, Label, LabelWrapMode, Overlay, OverlayState, OverlayStyle,
    PickerState, PickerStyle,
    PopupWindow, SliderState, SlidingSwitchMode, SlidingSwitchState, SlidingSwitchStyle, Spinner,
    TextCtx, TextInput, TextInputState, TextInputStyle, TextValidation, ToggleState, UICameraView,
};
use oxide_ui_core::DrawListBuilder;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

const MACOS_HEBREW_FONT: &str = "/System/Library/Fonts/Supplemental/Arial Unicode.ttf";

fn image_draw(image: &ImageView, rect: RectF, zoom: Option<&ImageZoomState>) -> (RectF, RectF, f32) {
    let mut builder = DrawListBuilder::new();
    image.encode(rect, zoom, &mut builder);
    assert_eq!(builder.drawlist().items.len(), 1);
    match builder.drawlist().items[0] {
        DrawCmd::Image { dst, src, alpha, .. } => (dst, src, alpha),
        ref other => panic!("expected image draw, got {other:?}"),
    }
}

fn image_region_draw(image: &ImageRegionView, rect: RectF) -> (RectF, RectF, f32)
{
   let mut builder = DrawListBuilder::new();
   image.encode(rect, &mut builder);
   assert_eq!(builder.drawlist().items.len(), 1);
   match builder.drawlist().items[0]
   {
      DrawCmd::Image { dst, src, alpha, .. } => (dst, src, alpha),
      ref other => panic!("expected image draw, got {other:?}"),
   }
}

fn assert_rect_close(actual: RectF, expected: RectF) {
    for (actual, expected) in [
        (actual.x, expected.x),
        (actual.y, expected.y),
        (actual.w, expected.w),
        (actual.h, expected.h),
    ] {
        assert!((actual - expected).abs() <= 0.0001, "actual={actual} expected={expected}");
    }
}

#[test]
fn image_view_contain_emits_full_source_inside_bounds() {
    let image = ImageView {
        image: ImageHandle(7),
        natural_w: 200,
        natural_h: 100,
        fit: ImageFit::Contain,
        alpha: 0.75,
    };
    let (dst, src, alpha) = image_draw(&image, RectF::new(10.0, 20.0, 100.0, 80.0), None);
    assert_rect_close(dst, RectF::new(10.0, 35.0, 100.0, 50.0));
    assert_rect_close(src, RectF::new(0.0, 0.0, 200.0, 100.0));
    assert_eq!(alpha, 0.75);
}

#[test]
fn image_view_stretch_preserves_full_source_and_alpha_clamp() {
    let image = ImageView {
        image: ImageHandle(7),
        natural_w: 201,
        natural_h: 99,
        fit: ImageFit::Stretch,
        alpha: 2.0,
    };
    let rect = RectF::new(3.0, 5.0, 101.0, 77.0);
    let (dst, src, alpha) = image_draw(&image, rect, None);
    assert_rect_close(dst, rect);
    assert_rect_close(src, RectF::new(0.0, 0.0, 201.0, 99.0));
    assert_eq!(alpha, 1.0);
}

#[test]
fn image_view_cover_bounds_destination_and_crops_source_pixels() {
    let image = ImageView {
        image: ImageHandle(7),
        natural_w: 200,
        natural_h: 100,
        fit: ImageFit::Cover,
        alpha: 1.0,
    };
    let rect = RectF::new(10.0, 20.0, 100.0, 80.0);
    let (dst, src, _) = image_draw(&image, rect, None);
    assert_rect_close(dst, rect);
    assert_rect_close(src, RectF::new(37.5, 0.0, 125.0, 100.0));
}

#[test]
fn image_region_view_cover_keeps_crop_inside_atlas_slot()
{
   let image = ImageRegionView {
      image: ImageHandle(9),
      source: RectF::new(130.0, 66.0, 80.0, 40.0),
      fit: ImageFit::Cover,
      alpha: 0.75,
   };
   let rect = RectF::new(10.0, 20.0, 50.0, 50.0);
   let (dst, src, alpha) = image_region_draw(&image, rect);
   assert_rect_close(dst, rect);
   assert_rect_close(src, RectF::new(150.0, 66.0, 40.0, 40.0));
   assert_eq!(alpha, 0.75);
}

#[test]
fn image_view_zoom_and_pan_crop_source_without_clip_commands() {
    let image = ImageView {
        image: ImageHandle(7),
        natural_w: 200,
        natural_h: 100,
        fit: ImageFit::Cover,
        alpha: 1.0,
    };
    let zoom = ImageZoomState { scale: 2.0, offset: [20.0, -10.0] };
    let rect = RectF::new(10.0, 20.0, 100.0, 80.0);
    let (dst, src, _) = image_draw(&image, rect, Some(&zoom));
    assert_rect_close(dst, rect);
    assert_rect_close(src, RectF::new(56.25, 31.25, 62.5, 50.0));
}

#[test]
fn image_view_odd_dimensions_keep_fractional_source_crop() {
    let image = ImageView {
        image: ImageHandle(7),
        natural_w: 7,
        natural_h: 5,
        fit: ImageFit::Cover,
        alpha: 1.0,
    };
    let rect = RectF::new(0.0, 0.0, 11.0, 9.0);
    let (dst, src, _) = image_draw(&image, rect, None);
    assert_rect_close(dst, rect);
    assert_rect_close(src, RectF::new(0.44444445, 0.0, 6.111111, 5.0));
}

#[test]
fn image_view_prevalidated_emission_respects_effective_empty_clip() {
    let image = ImageView {
        image: ImageHandle(7),
        natural_w: 29,
        natural_h: 7,
        fit: ImageFit::Cover,
        alpha: 1.0,
    };
    let mut builder = DrawListBuilder::new();
    builder.clip_push(oxide_renderer_api::RectI::new(0, 0, 0, 12));
    image.encode(RectF::new(0.0, 0.0, 24.0, 12.0), None, &mut builder);
    builder.clip_pop();
    assert_eq!(builder.drawlist().items.len(), 2);
    assert!(!builder.drawlist().items.iter().any(|item| matches!(item, DrawCmd::Image { .. })));
}

#[test]
fn image_view_rejects_invalid_bounds_and_noncontributing_alpha() {
    let mut image = ImageView {
        image: ImageHandle(7),
        natural_w: 29,
        natural_h: 7,
        fit: ImageFit::Cover,
        alpha: 0.0,
    };
    let mut builder = DrawListBuilder::new();
    image.encode(RectF::new(0.0, 0.0, 24.0, 12.0), None, &mut builder);
    image.alpha = 1.0;
    image.encode(RectF::new(f32::NAN, 0.0, 24.0, 12.0), None, &mut builder);
    image.encode(RectF::new(0.0, 0.0, -24.0, 12.0), None, &mut builder);
    assert!(builder.drawlist().items.is_empty());
}

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
fn transparent_popup_omits_the_backdrop_draw() {
    let popup = PopupWindow {style: oxide_ui_core::elements::PopupStyle {panel_backdrop_alpha: 0.0, ..PopupWindow::default().style}};
    let mut builder = oxide_ui_core::DrawListBuilder::new();
    popup.encode(RectF::new(10.0, 20.0, 200.0, 120.0), 1.0, &mut builder);

    assert_eq!(builder.drawlist().items.len(), 2);
    assert!(builder.drawlist().items.iter().all(|item| !matches!(item, DrawCmd::Backdrop { .. })));
}

#[test]
fn black_zero_blur_overlay_encodes_a_flat_source_over_tint() {
    let overlay = Overlay {style: OverlayStyle {tint: Color::rgba(0.0, 0.0, 0.0, 0.5), alpha: 0.8, blur_sigma: 0.0}};
    let mut state = OverlayState::new();
    state.open();
    state.tick(16);
    let progress = state.progress();
    let mut builder = oxide_ui_core::DrawListBuilder::new();

    assert!(overlay.encode(&state, RectF::new(0.0, 0.0, 120.0, 80.0), 1.0, &mut builder));
    assert_eq!(builder.drawlist().items.len(), 1);
    match builder.drawlist().items[0] {
        DrawCmd::RRect { color, .. } => assert!((color.a - 0.5 * 0.8 * progress).abs() <= f32::EPSILON),
        ref other => panic!("expected flat overlay, got {other:?}"),
    }
}

#[test]
fn colored_zero_blur_overlay_preserves_backdrop_semantics() {
    let overlay = Overlay {style: OverlayStyle {tint: Color::rgba(0.2, 0.4, 0.6, 0.5), alpha: 0.8, blur_sigma: 0.0}};
    let mut state = OverlayState::new();
    state.open();
    state.tick(16);
    let mut builder = oxide_ui_core::DrawListBuilder::new();

    assert!(overlay.encode(&state, RectF::new(0.0, 0.0, 120.0, 80.0), 1.0, &mut builder));
    assert!(matches!(builder.drawlist().items[0], DrawCmd::Backdrop { .. }));
}

#[test]
fn popup_window_places_above_when_space_allows_and_clamps_horizontal_edges() {
    let viewport = RectF::new(0.0, 0.0, 390.0, 844.0);
    let popup = PopupWindow::place_near_anchor(
        RectF::new(350.0, 700.0, 32.0, 32.0),
        [150.0, 44.0],
        viewport,
        8.0,
    );

    assert_eq!(popup, RectF::new(232.0, 648.0, 150.0, 44.0));
}

#[test]
fn popup_window_places_below_when_above_would_escape_viewport() {
    let viewport = RectF::new(0.0, 0.0, 390.0, 844.0);
    let popup = PopupWindow::place_near_anchor(
        RectF::new(0.0, 12.0, 100.0, 32.0),
        [150.0, 44.0],
        viewport,
        8.0,
    );

    assert_eq!(popup, RectF::new(8.0, 52.0, 150.0, 44.0));
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

#[derive(Default)]
struct PagedUploader {
    next: u32,
    creates: usize,
    appends: usize,
    releases: Vec<ImageHandle>,
}

impl ImageUploader for PagedUploader {
    fn create_a8(
        &mut self,
        _w: u32,
        _h: u32,
        _data: &[u8],
        _row_bytes: usize,
    ) -> ImageHandle {
        self.next = self.next.saturating_add(1).max(1);
        self.creates += 1;
        ImageHandle(self.next)
    }

    fn update_a8(
        &mut self,
        _handle: ImageHandle,
        _x: u32,
        _y: u32,
        _w: u32,
        _h: u32,
        _data: &[u8],
        _row_bytes: usize,
    ) {
        panic!("paged atlas must publish append-only updates");
    }

    fn append_a8(
        &mut self,
        _handle: ImageHandle,
        _x: u32,
        _y: u32,
        _w: u32,
        _h: u32,
        _data: &[u8],
        _row_bytes: usize,
    ) {
        self.appends += 1;
    }

    fn release_a8(&mut self, handle: ImageHandle) {
        self.releases.push(handle);
    }
}

fn encode_retina_label(
   font_id: usize,
   scale: f32,
   text: &mut TextCtx,
   uploader: &mut PagedUploader,
   builder: &mut DrawListBuilder,
)
{
   encode_label_text(
      "Retina",
      Color::rgba(0.1, 0.2, 0.3, 1.0),
      Align::Left,
      false,
      font_id,
      16.0,
      RectF::new(0.0, 0.0, 120.0, 24.0),
      scale,
      text,
      uploader,
      builder,
   );
}

#[test]
fn paged_text_recycles_one_gpu_page_and_preserves_the_other_retained_identity() {
    let mut text = TextCtx::default();
    text.atlas = oxide_text::PagedAtlas::new(24, 24, 2);
    let font_id = text.fonts.add_font(oxide_text::Font::from_bytes(
        include_bytes!("../assets/Asap-Regular.ttf").to_vec(),
    ));
    let mut uploader = PagedUploader::default();
    let mut builder = DrawListBuilder::new();
    let mut labels = Vec::new();

    text.begin_frame_at_scale(1.0);
    for ch in 'A'..='Z' {
        let value = ch.to_string();
        encode_label_text(
            &value,
            Color::rgba(0.1, 0.2, 0.3, 1.0),
            Align::Left,
            false,
            font_id,
            16.0,
            RectF::new(0.0, labels.len() as f32 * 20.0, 40.0, 20.0),
            1.0,
            &mut text,
            &mut uploader,
            &mut builder,
        );
        labels.push(value);
        if text.atlas.page_count() == 2 {
            break;
        }
    }
    let _ = text.finish_frame(&mut uploader, &mut builder);
    let first_revisions = text
        .retained_text_atlas_revisions()
        .expect("clean first atlas pages")
        .to_vec();
    assert_eq!(first_revisions.len(), 2);
    assert_ne!(first_revisions[0].0, first_revisions[1].0);
    assert_eq!(uploader.creates, 2);

    let resolved_runs = builder
        .drawlist()
        .items
        .iter()
        .filter_map(|item| match item {
            DrawCmd::GlyphRun { run } => Some(*run),
            _ => None,
        })
        .collect::<Vec<_>>();
    let second_handle = first_revisions[1].0;
    let pinned_index = resolved_runs
        .iter()
        .position(|run| run.atlas == second_handle)
        .expect("glyph on second page");
    let pinned_label = labels[pinned_index].clone();
    let first_handle = first_revisions[0].0;

    builder.clear();
    text.begin_frame_at_scale(1.0);
    encode_label_text(
        &pinned_label,
        Color::rgba(0.1, 0.2, 0.3, 1.0),
        Align::Left,
        false,
        font_id,
        16.0,
        RectF::new(0.0, 0.0, 40.0, 20.0),
        1.0,
        &mut text,
        &mut uploader,
        &mut builder,
    );
    'pressure: for px in 17..=30 {
        for ch in 'a'..='z' {
            encode_label_text(
                &ch.to_string(),
                Color::rgba(0.1, 0.2, 0.3, 1.0),
                Align::Left,
                false,
                font_id,
                px as f32,
                RectF::new(0.0, 0.0, 40.0, 34.0),
                1.0,
                &mut text,
                &mut uploader,
                &mut builder,
            );
            if text.atlas.eviction_count() > 0 {
                break 'pressure;
            }
        }
    }
    assert_eq!(text.atlas.eviction_count(), 1);
    let _ = text.finish_frame(&mut uploader, &mut builder);
    let second_revisions = text
        .retained_text_atlas_revisions()
        .expect("clean recycled atlas pages");
    assert_eq!(second_revisions.len(), 2);
    assert!(second_revisions.iter().any(|(handle, _)| *handle == second_handle));
    assert!(!second_revisions.iter().any(|(handle, _)| *handle == first_handle));
    assert_eq!(uploader.releases, vec![first_handle]);

    text.trim_memory_with_uploader(&mut uploader);
    assert_eq!(uploader.releases.len(), 3);
    assert_eq!(text.atlas.page_count(), 1);
    assert_eq!(text.retained_text_atlas_revisions(), None);
}

#[test]
fn finish_frame_many_patches_every_builder_before_retiring_atlas_pages()
{
   let mut text = TextCtx::default();
   text.atlas = oxide_text::PagedAtlas::new(24, 24, 1);
   let font_id = text.fonts.add_font(oxide_text::Font::from_bytes(
      include_bytes!("../assets/Asap-Regular.ttf").to_vec(),
   ));
   let mut uploader = PagedUploader::default();
   let mut first = DrawListBuilder::new();
   let mut second = DrawListBuilder::new();

   text.begin_frame_at_scale(1.0);
   encode_label_text("A", Color::rgba(0.1, 0.2, 0.3, 1.0), Align::Left, false, font_id, 16.0, RectF::new(0.0, 0.0, 40.0, 20.0), 1.0, &mut text, &mut uploader, &mut first);
   for (index, value) in ["B", "C", "D", "E", "F", "G", "H"].iter().enumerate()
   {
      encode_label_text(value, Color::rgba(0.1, 0.2, 0.3, 1.0), Align::Left, false, font_id, 16.0, RectF::new(0.0, index as f32 * 20.0, 40.0, 20.0), 1.0, &mut text, &mut uploader, &mut second);
   }
   let _ = text.finish_frame_many(&mut uploader, &mut [&mut first, &mut second]);
   let live: Vec<ImageHandle> = text.retained_text_atlas_revisions().expect("published atlas pages").iter().map(|(handle, _)| *handle).collect();

   for builder in [&first, &second]
   {
      for item in &builder.drawlist().items
      {
         if let DrawCmd::GlyphRun { run } = item
         {
            assert_ne!(run.atlas, ImageHandle(0));
            assert!(live.contains(&run.atlas), "retired glyph atlas handle {:?}", run.atlas);
         }
      }
   }
}

#[test]
fn device_scale_change_invalidates_retained_text_without_republishing_pages()
{
   let mut text = TextCtx::default();
   let font_id = text.fonts.add_font(oxide_text::Font::from_bytes(
      include_bytes!("../assets/Asap-Regular.ttf").to_vec(),
   ));
   let mut uploader = PagedUploader::default();
   let mut builder = DrawListBuilder::new();

   text.begin_frame_at_scale(1.0);
   encode_retina_label(font_id, 1.0, &mut text, &mut uploader, &mut builder);
   let _ = text.finish_frame(&mut uploader, &mut builder);
   let scale_one = builder.drawlist().clone();
   let scale_one_revisions = text.retained_text_atlas_revisions().expect("clean 1x atlas").to_vec();
   let page_handle = scale_one_revisions[0].0;
   assert_eq!(uploader.creates, 1);
   assert!(DrawListBuilder::new().append_retained_drawlist_with_text_atlas_revisions(
      &scale_one,
      &scale_one_revisions,
   ));

   text.begin_frame_at_scale(3.0);
   let scale_three_revisions = text.retained_text_atlas_revisions().expect("clean 3x context").to_vec();
   assert_eq!(scale_three_revisions[0].0, page_handle);
   assert_ne!(scale_three_revisions[0].1, scale_one_revisions[0].1);
   assert!(!DrawListBuilder::new().append_retained_drawlist_with_text_atlas_revisions(
      &scale_one,
      &scale_three_revisions,
   ));

   builder.clear();
   encode_retina_label(font_id, 3.0, &mut text, &mut uploader, &mut builder);
   let _ = text.finish_frame(&mut uploader, &mut builder);
   let scale_three = builder.drawlist().clone();
   let scale_three_revisions = text.retained_text_atlas_revisions().expect("uploaded 3x atlas").to_vec();
   assert_eq!(scale_three_revisions[0].0, page_handle);
   assert_eq!(uploader.creates, 1);
   assert!(uploader.appends > 0);
   assert!(uploader.releases.is_empty());
   assert!(DrawListBuilder::new().append_retained_drawlist_with_text_atlas_revisions(
      &scale_three,
      &scale_three_revisions,
   ));

   let appends = uploader.appends;
   text.begin_frame_at_scale(1.0);
   let scale_one_return_revisions = text.retained_text_atlas_revisions().expect("warm 1x context").to_vec();
   assert_ne!(scale_one_return_revisions[0].1, scale_one_revisions[0].1);
   assert!(!DrawListBuilder::new().append_retained_drawlist_with_text_atlas_revisions(
      &scale_one,
      &scale_one_return_revisions,
   ));
   builder.clear();
   encode_retina_label(font_id, 1.0, &mut text, &mut uploader, &mut builder);
   let _ = text.finish_frame(&mut uploader, &mut builder);
   assert_eq!(uploader.appends, appends, "warm 1x switchback must reuse its resident glyphs");
   assert!(DrawListBuilder::new().append_retained_drawlist_with_text_atlas_revisions(
      builder.drawlist(),
      text.retained_text_atlas_revisions().expect("republished 1x geometry"),
   ));

   let explicit_scale_revisions =
      text.retained_text_atlas_revisions().expect("explicit 1x context").to_vec();
   text.begin_frame();
   assert_eq!(
      text.retained_text_atlas_revisions().expect("unbound compatibility frame"),
      explicit_scale_revisions,
   );
   builder.clear();
   encode_retina_label(font_id, 3.0, &mut text, &mut uploader, &mut builder);
   let _ = text.finish_frame(&mut uploader, &mut builder);
   let legacy_bound_revisions =
      text.retained_text_atlas_revisions().expect("legacy frame bound on encode");
   assert_ne!(legacy_bound_revisions, explicit_scale_revisions);
   assert!(DrawListBuilder::new().append_retained_drawlist_with_text_atlas_revisions(
      builder.drawlist(),
      legacy_bound_revisions,
   ));
}

#[test]
fn paged_text_recreates_gpu_pages_after_device_loss() {
   let mut text = TextCtx::default();
   text.atlas = oxide_text::PagedAtlas::new(64, 64, 2);
   let font_id = text.fonts.add_font(oxide_text::Font::from_bytes(
      include_bytes!("../assets/Asap-Regular.ttf").to_vec(),
   ));
   let mut uploader = PagedUploader::default();
   let mut builder = DrawListBuilder::new();

   text.begin_frame_at_scale(1.0);
   encode_label_text(
      "device loss",
      Color::rgba(1.0, 1.0, 1.0, 1.0),
      Align::Left,
      false,
      font_id,
      16.0,
      RectF::new(0.0, 0.0, 100.0, 24.0),
      1.0,
      &mut text,
      &mut uploader,
      &mut builder,
   );
   let _ = text.finish_frame(&mut uploader, &mut builder);
   let first_handle = text
      .retained_text_atlas_revisions()
      .expect("published atlas page")[0]
      .0;

   text.handle_device_loss();
   assert_eq!(text.atlas.page_count(), 1);
   assert_eq!(text.retained_text_atlas_revisions(), None);
   assert!(uploader.releases.is_empty());

   builder.clear();
   text.begin_frame_at_scale(1.0);
   encode_label_text(
      "device loss",
      Color::rgba(1.0, 1.0, 1.0, 1.0),
      Align::Left,
      false,
      font_id,
      16.0,
      RectF::new(0.0, 0.0, 100.0, 24.0),
      1.0,
      &mut text,
      &mut uploader,
      &mut builder,
   );
   let _ = text.finish_frame(&mut uploader, &mut builder);
   let recreated_handle = text
      .retained_text_atlas_revisions()
      .expect("recreated atlas page")[0]
      .0;
   assert_ne!(recreated_handle, first_handle);
   assert_eq!(uploader.creates, 2);
}

#[test]
fn paged_text_repatches_prior_immediate_runs_after_page_replacement() {
   let mut text = TextCtx::default();
   let font_id = text.fonts.add_font(oxide_text::Font::from_bytes(
      include_bytes!("../assets/Asap-Regular.ttf").to_vec(),
   ));
   let mut uploader = PagedUploader::default();
   let mut builder = DrawListBuilder::new();

   encode_label_text(
      "title",
      Color::rgba(0.1, 0.1, 0.1, 1.0),
      Align::Left,
      false,
      font_id,
      15.0,
      RectF::new(0.0, 0.0, 100.0, 24.0),
      1.0,
      &mut text,
      &mut uploader,
      &mut builder,
   );
   let first_handle = match builder.drawlist().items.first() {
      Some(DrawCmd::GlyphRun { run }) => run.atlas,
      _ => panic!("title glyph run"),
   };

   encode_label_text(
      "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
      Color::rgba(0.1, 0.1, 0.1, 1.0),
      Align::Left,
      false,
      font_id,
      28.0,
      RectF::new(0.0, 30.0, 800.0, 40.0),
      1.0,
      &mut text,
      &mut uploader,
      &mut builder,
   );

   assert_eq!(uploader.releases, vec![first_handle]);
   let current_handle = text
      .retained_text_atlas_revisions()
      .expect("replacement atlas page")[0]
      .0;
   assert_ne!(current_handle, first_handle);
   assert!(builder.drawlist().items.iter().all(|item| {
      !matches!(item, DrawCmd::GlyphRun { run } if run.atlas != current_handle)
   }));
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
fn picker_reuses_cached_label_shapes_and_skips_clean_atlas_uploads() {
    let picker = PickerState::new(vec![
        "Red".to_string(),
        "Green".to_string(),
        "Blue".to_string(),
        "Orange".to_string(),
    ]);
    let mut text = TextCtx::default();
    let font_id = text.fonts.add_font(oxide_text::Font::from_bytes(
        include_bytes!("../assets/Asap-Regular.ttf").to_vec(),
    ));
    let style = PickerStyle { font_id, ..PickerStyle::default() };
    let rect = RectF::new(0.0, 0.0, 200.0, 180.0);
    let mut builder = DrawListBuilder::new();
    let mut uploader = CountingUploader::default();

    picker.encode(&style, rect, 2.0, &mut text, &mut uploader, &mut builder);
    assert_eq!(uploader.creates, 1);
    assert_eq!(uploader.updates, 0);
    assert!(builder.drawlist().items.iter().any(|cmd| matches!(cmd, DrawCmd::GlyphRun { .. })));

    builder.clear();
    picker.encode(&style, rect, 2.0, &mut text, &mut uploader, &mut builder);
    assert_eq!(uploader.creates, 1);
    assert_eq!(uploader.updates, 0);
    assert!(builder.drawlist().items.iter().any(|cmd| matches!(cmd, DrawCmd::GlyphRun { .. })));
}

#[test]
fn label_batches_configured_fallback_font_runs() {
    let mut text = TextCtx::default();
    let latin_id = text.fonts.add_font(oxide_text::Font::from_bytes(
        include_bytes!("../../text/tests/fixtures/test_text_latin.ttf").to_vec(),
    ));
    let cjk_id = text.fonts.add_font(oxide_text::Font::from_bytes(
        include_bytes!("../../text/tests/fixtures/test_text_cjk.ttf").to_vec(),
    ));
    text.set_fallback_fonts(&[cjk_id]);
    let mut builder = DrawListBuilder::new();
    let mut uploader = CountingUploader::default();

    encode_label_text(
        "A漢B",
        Color::rgba(0.1, 0.1, 0.1, 1.0),
        Align::Left,
        false,
        latin_id,
        22.0,
        RectF::new(0.0, 0.0, 160.0, 40.0),
        2.0,
        &mut text,
        &mut uploader,
        &mut builder,
    );

    let glyph_runs = builder
        .drawlist()
        .items
        .iter()
        .filter(|cmd| matches!(cmd, DrawCmd::GlyphRun { .. }))
        .count();
    assert_eq!(glyph_runs, 1);
    assert!(builder.drawlist().vertices.len() >= 12);
    assert_eq!(uploader.creates, 1);
    assert_eq!(uploader.updates, 0);
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
    assert_eq!(uploader.updates, 0);

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
    assert_eq!(uploader.updates, 0);
    assert!(builder.drawlist().items.iter().all(|item| {
        !matches!(item, DrawCmd::GlyphRun { run } if run.atlas.0 == 0)
    }));
}

#[test]
fn text_frame_preflights_visible_labels_and_publishes_once() {
    let mut text = TextCtx::default();
    text.set_frame_stats_enabled(true);
    let _ = text.fonts.add_font(oxide_text::Font::from_bytes(
        include_bytes!("../assets/Asap-Regular.ttf").to_vec(),
    ));
    let labels = (0..200).map(|index| format!("Prepared label {index:03}")).collect::<Vec<_>>();
    let mut builder = DrawListBuilder::new();
    let mut uploader = CountingUploader::default();

    text.begin_frame_at_scale(2.0);
    for (index, label) in labels.iter().enumerate() {
        encode_label_text_profiled(
            label,
            Color::rgba(0.1, 0.1, 0.1, 1.0),
            Align::Left,
            false,
            0,
            14.0,
            RectF::new(0.0, index as f32 * 18.0, 320.0, 18.0),
            2.0,
            &mut text,
            &mut uploader,
            &mut builder,
        );
    }
    assert_eq!(uploader.creates, 0);
    assert_eq!(uploader.updates, 0);
    let cold = text.finish_frame(&mut uploader, &mut builder);
    assert_eq!(cold.visible_labels, 200);
    assert_eq!(cold.shaping_calls, 200);
    assert!(cold.rasterizations > 0);
    assert_eq!(cold.atlas_upload_calls, 1);
    assert_eq!(cold.invalidated_runs, 0);
    assert_eq!(uploader.creates, 1);
    assert_eq!(uploader.updates, 0);
    assert_eq!(
        builder
            .drawlist()
            .items
            .iter()
            .filter(|item| matches!(item, DrawCmd::GlyphRun { run } if run.vb.len > 0))
            .count(),
        200,
    );

    builder.clear();
    text.begin_frame_at_scale(2.0);
    for (index, label) in labels.iter().enumerate() {
        encode_label_text_profiled(
            label,
            Color::rgba(0.1, 0.1, 0.1, 1.0),
            Align::Left,
            false,
            0,
            14.0,
            RectF::new(0.0, index as f32 * 18.0, 320.0, 18.0),
            2.0,
            &mut text,
            &mut uploader,
            &mut builder,
        );
    }
    let warm = text.finish_frame(&mut uploader, &mut builder);
    assert_eq!(warm.visible_labels, 200);
    assert_eq!(warm.shaping_calls, 0);
    assert_eq!(warm.rasterizations, 0);
    assert_eq!(warm.layout_cache_hits, 200);
    assert_eq!(warm.layout_cache_misses, 0);
    assert_eq!(warm.atlas_upload_calls, 0);
    assert_eq!(warm.atlas_upload_pixels, 0);
    assert_eq!(warm.atlas_upload_bytes, 0);
    assert_eq!(warm.atlas_evictions, 0);
    assert_eq!(warm.invalidated_runs, 0);
    assert_eq!(uploader.creates, 1);
    assert_eq!(uploader.updates, 0);

    builder.clear();
    text.begin_frame_at_scale(2.0);
    encode_label_text_profiled(
        "Z",
        Color::rgba(0.1, 0.1, 0.1, 1.0),
        Align::Left,
        false,
        0,
        14.0,
        RectF::new(0.0, 0.0, 32.0, 18.0),
        2.0,
        &mut text,
        &mut uploader,
        &mut builder,
    );
    let incremental = text.finish_frame(&mut uploader, &mut builder);
    let (_, _, width, height, _) = uploader.last_update.expect("one dirty atlas update");
    assert_eq!(incremental.atlas_upload_calls, 1);
    assert_eq!(incremental.atlas_upload_pixels, u64::from(width) * u64::from(height));
    assert_eq!(incremental.atlas_upload_bytes, incremental.atlas_upload_pixels);
    assert!(incremental.atlas_upload_pixels < 1024 * 1024);
    assert_eq!(uploader.creates, 1);
    assert_eq!(uploader.updates, 1);
}

#[test]
fn text_frame_patches_provisional_runs_without_changing_draw_order() {
    let mut text = TextCtx::default();
    let _ = text.fonts.add_font(oxide_text::Font::from_bytes(
        include_bytes!("../assets/Asap-Regular.ttf").to_vec(),
    ));
    let mut builder = DrawListBuilder::new();
    let mut uploader = CountingUploader::default();
    let color = Color::rgba(0.2, 0.3, 0.4, 1.0);

    text.begin_frame_at_scale(2.0);
    builder.rrect(RectF::new(0.0, 0.0, 120.0, 32.0), [4.0; 4], color);
    encode_label_text(
        "first",
        color,
        Align::Left,
        false,
        0,
        14.0,
        RectF::new(4.0, 4.0, 100.0, 20.0),
        2.0,
        &mut text,
        &mut uploader,
        &mut builder,
    );
    builder.rrect(RectF::new(0.0, 40.0, 120.0, 32.0), [4.0; 4], color);
    encode_label_text(
        "second",
        color,
        Align::Left,
        false,
        0,
        14.0,
        RectF::new(4.0, 44.0, 100.0, 20.0),
        2.0,
        &mut text,
        &mut uploader,
        &mut builder,
    );
    let _ = text.finish_frame(&mut uploader, &mut builder);

    assert_eq!(builder.drawlist().items.len(), 4);
    assert!(matches!(builder.drawlist().items[0], DrawCmd::RRect { .. }));
    assert!(matches!(builder.drawlist().items[1], DrawCmd::GlyphRun { run } if run.atlas.0 == 41));
    assert!(matches!(builder.drawlist().items[2], DrawCmd::RRect { .. }));
    assert!(matches!(builder.drawlist().items[3], DrawCmd::GlyphRun { run } if run.atlas.0 == 41));
}

#[test]
fn wrapped_ascii_label_reuses_fast_fit_layout_and_clean_atlas() {
    let mut text = TextCtx::default();
    let _ = text.fonts.add_font(oxide_text::Font::from_bytes(
        include_bytes!("../assets/Asap-Regular.ttf").to_vec(),
    ));
    let label = Label {
        text: "Orbit telemetry cache labels wrap across several narrow rows without reshaping every candidate word".to_string(),
        color: Color::rgba(0.1, 0.1, 0.1, 1.0),
        align: Align::Left,
        wrap: true,
        font_id: 0,
        font_px: 13.0,
    };
    let rect = RectF::new(0.0, 0.0, 118.0, 180.0);
    let mut builder = DrawListBuilder::new();
    let mut uploader = CountingUploader::default();

    label.encode(rect, 2.0, &mut text, &mut uploader, &mut builder);
    let first_glyph_runs = builder
        .drawlist()
        .items
        .iter()
        .filter(|cmd| matches!(cmd, DrawCmd::GlyphRun { .. }))
        .count();
    assert!(first_glyph_runs > 1);
    assert_eq!(uploader.creates, 1);
    assert_eq!(uploader.updates, 0);

    builder.clear();
    label.encode(rect, 2.0, &mut text, &mut uploader, &mut builder);
    let warm_glyph_runs = builder
        .drawlist()
        .items
        .iter()
        .filter(|cmd| matches!(cmd, DrawCmd::GlyphRun { .. }))
        .count();
    assert_eq!(warm_glyph_runs, first_glyph_runs);
    assert_eq!(uploader.creates, 1);
    assert_eq!(uploader.updates, 0);
}

#[test]
fn label_measure_uses_the_actual_wrapped_line_layout() {
    let mut text = TextCtx::default();
    let _ = text.fonts.add_font(oxide_text::Font::from_bytes(
        include_bytes!("../assets/Asap-Regular.ttf").to_vec(),
    ));
    let label = Label {
        text: "Words wrap according to shaped line widths, rather than a character estimate.".into(),
        color: Color::rgba(0.1, 0.1, 0.1, 1.0),
        align: Align::Left,
        wrap: true,
        font_id: 0,
        font_px: 14.0,
    };
    let measured = label.measure(100.0, &mut text).expect("font is present");
    let metrics = text.fonts.font(0).unwrap().line_metrics(label.font_px).unwrap();
    let line_advance = (label.font_px * 1.25).ceil();
    let mut builder = DrawListBuilder::new();
    let mut uploader = CountingUploader::default();
    label.encode(RectF::new(0.0, 0.0, 100.0, 300.0), 2.0, &mut text, &mut uploader, &mut builder);
    let rendered_lines = builder.drawlist().items.iter().filter(|item| matches!(item, DrawCmd::GlyphRun { .. })).count();
    let expected_height = metrics.ascent + metrics.descent + rendered_lines.saturating_sub(1) as f32 * line_advance;

    assert!(rendered_lines > 1);
    assert!(measured[0] <= 100.0);
    assert!((measured[1] - expected_height).abs() <= f32::EPSILON);

    let adjacent = Label {text: "Top\nBottom".into(), color: label.color, align: label.align, wrap: label.wrap, font_id: label.font_id, font_px: label.font_px}.measure(100.0, &mut text).unwrap();
    let blank = Label {text: "Top\n\nBottom".into(), color: label.color, align: label.align, wrap: label.wrap, font_id: label.font_id, font_px: label.font_px}.measure(100.0, &mut text).unwrap();
    assert!((blank[1] - adjacent[1] - line_advance).abs() <= f32::EPSILON);
}

#[test]
fn label_grapheme_wrap_mode_bounds_long_tokens_and_matches_measurement()
{
   let mut text = TextCtx::default();
   let _ = text.fonts.add_font(oxide_text::Font::from_bytes(
      include_bytes!("../assets/Asap-Regular.ttf").to_vec(),
   ));
   let label = Label {
      text: "Supercalifragilisticexpialidocious".into(),
      color: Color::rgba(0.1, 0.1, 0.1, 1.0),
      align: Align::Left,
      wrap: true,
      font_id: 0,
      font_px: 14.0,
   };
   let narrow = 72.0;
   let legacy = label.measure(narrow, &mut text).unwrap();
   let measured = label.measure_with_wrap_mode(narrow, LabelWrapMode::WordOrGrapheme, &mut text).unwrap();
   let metrics = text.fonts.font(0).unwrap().line_metrics(label.font_px).unwrap();
   let line_advance = (label.font_px * 1.25).ceil();
   let mut builder = DrawListBuilder::new();
   let mut uploader = CountingUploader::default();
   label.encode_with_wrap_mode(
      RectF::new(0.0, 0.0, narrow, 300.0),
      2.0,
      LabelWrapMode::WordOrGrapheme,
      &mut text,
      &mut uploader,
      &mut builder,
   );
   let rendered_lines = builder.drawlist().items.iter().filter(|item| matches!(item, DrawCmd::GlyphRun { .. })).count();

   assert!(legacy[0] > narrow);
   assert!(measured[0] <= narrow);
   assert!(rendered_lines > 1);
   assert!((measured[1] - (metrics.ascent + metrics.descent + (rendered_lines - 1) as f32 * line_advance)).abs() <= f32::EPSILON);

   let packed = Label {
      text: "Supercalifragilisticexpialidocious a".into(),
      color: label.color,
      align: label.align,
      wrap: label.wrap,
      font_id: label.font_id,
      font_px: label.font_px,
   };
   let packing_width = 90.0;
   let long_at_packing_width = label.measure_with_wrap_mode(packing_width, LabelWrapMode::WordOrGrapheme, &mut text).unwrap();
   assert_eq!(
      packed.measure_with_wrap_mode(packing_width, LabelWrapMode::WordOrGrapheme, &mut text).unwrap()[1],
      long_at_packing_width[1],
   );

   let cluster = Label {
      text: "e\u{301}".into(),
      color: label.color,
      align: label.align,
      wrap: label.wrap,
      font_id: label.font_id,
      font_px: label.font_px,
   };
   let cluster_width = cluster.measure(f32::INFINITY, &mut text).unwrap()[0];
   assert!(cluster.measure_with_wrap_mode(1.0, LabelWrapMode::WordOrGrapheme, &mut text).unwrap()[0] > 1.0);
   let combining = Label {
      text: "e\u{301}".repeat(4),
      color: label.color,
      align: label.align,
      wrap: label.wrap,
      font_id: label.font_id,
      font_px: label.font_px,
   };
   builder.clear();
   combining.encode_with_wrap_mode(
      RectF::new(0.0, 0.0, cluster_width + 0.1, 300.0),
      2.0,
      LabelWrapMode::WordOrGrapheme,
      &mut text,
      &mut uploader,
      &mut builder,
   );
   assert_eq!(builder.drawlist().items.iter().filter(|item| matches!(item, DrawCmd::GlyphRun { .. })).count(), 4);
}

#[test]
fn wrapped_ascii_fast_fit_preserves_leading_space_advance() {
    let mut text = TextCtx::default();
    let _ = text.fonts.add_font(oxide_text::Font::from_bytes(
        include_bytes!("../assets/Asap-Regular.ttf").to_vec(),
    ));
    let rect = RectF::new(0.0, 0.0, 220.0, 80.0);
    let mut builder = DrawListBuilder::new();
    let mut uploader = CountingUploader::default();

    encode_label_text(
        "Lead",
        Color::rgba(0.1, 0.1, 0.1, 1.0),
        Align::Left,
        true,
        0,
        14.0,
        rect,
        2.0,
        &mut text,
        &mut uploader,
        &mut builder,
    );
    let plain_x =
        builder.drawlist().vertices.iter().map(|vertex| vertex.x).fold(f32::INFINITY, f32::min);

    builder.clear();
    encode_label_text(
        "   Lead",
        Color::rgba(0.1, 0.1, 0.1, 1.0),
        Align::Left,
        true,
        0,
        14.0,
        rect,
        2.0,
        &mut text,
        &mut uploader,
        &mut builder,
    );
    let leading_x =
        builder.drawlist().vertices.iter().map(|vertex| vertex.x).fold(f32::INFINITY, f32::min);

    assert!(leading_x > plain_x + 6.0);
}

#[test]
fn text_ctx_retained_atlas_snapshot_requires_clean_gpu_upload() {
    let mut text = TextCtx::default();
    let _ = text.fonts.add_font(oxide_text::Font::from_bytes(
        include_bytes!("../assets/Asap-Regular.ttf").to_vec(),
    ));
    let label = Label {
        text: "Retained atlas".to_string(),
        color: Color::rgba(0.1, 0.1, 0.1, 1.0),
        align: Align::Left,
        wrap: false,
        font_id: 0,
        font_px: 14.0,
    };
    let mut builder = DrawListBuilder::new();
    let mut uploader = CountingUploader::default();

    assert_eq!(text.retained_text_atlas_revision(), None);
    label.encode(RectF::new(0.0, 0.0, 160.0, 40.0), 2.0, &mut text, &mut uploader, &mut builder);
    assert_eq!(text.retained_text_atlas_revision(), Some((ImageHandle(41), text.atlas_revision())),);

    text.atlas.reset();
    assert_eq!(text.retained_text_atlas_revision(), None);
}

#[test]
fn text_input_reuses_shape_cache_and_skips_clean_atlas_uploads() {
    let mut text = TextCtx::default();
    let _ = text.fonts.add_font(oxide_text::Font::from_bytes(
        include_bytes!("../assets/Asap-Regular.ttf").to_vec(),
    ));
    let input = TextInput::default();
    let mut state = TextInputState::new("Name");
    state.focus();
    state.handle_text_event(&TextEvent::Commit { text: "cached text".into() });
    let mut builder = DrawListBuilder::new();
    let mut uploader = CountingUploader::default();

    input.encode(
        &state,
        RectF::new(0.0, 0.0, 180.0, 44.0),
        2.0,
        &mut text,
        &mut uploader,
        &mut builder,
    );
    assert_eq!(uploader.creates, 1);
    assert_eq!(uploader.updates, 0);

    builder.clear();
    input.encode(
        &state,
        RectF::new(0.0, 0.0, 180.0, 44.0),
        2.0,
        &mut text,
        &mut uploader,
        &mut builder,
    );
    assert_eq!(uploader.creates, 1);
    assert_eq!(uploader.updates, 0);
}

#[test]
fn text_input_centers_glyphs_inside_its_padded_content_box() {
    let mut text = TextCtx::default();
    let _ = text.fonts.add_font(oxide_text::Font::from_bytes(
        include_bytes!("../assets/Asap-Regular.ttf").to_vec(),
    ));
    let input = TextInput::default();
    let mut state = TextInputState::new("Name");
    state.focus();
    state.handle_text_event(&TextEvent::Commit { text: "Baseline".into() });
    let rect = RectF::new(0.0, 0.0, 180.0, 68.0);
    let mut builder = DrawListBuilder::new();
    let mut uploader = CountingUploader::default();

    input.encode(&state, rect, 2.0, &mut text, &mut uploader, &mut builder);
    let glyph_top = builder.drawlist().vertices.iter().map(|vertex| vertex.y).fold(f32::INFINITY, f32::min);
    let inner_top = 1.5 + input.style.padding.top;
    assert!(glyph_top.is_finite());
    assert!(glyph_top >= inner_top - 1.0, "glyph_top={glyph_top} inner_top={inner_top}");
}

#[test]
fn text_input_keeps_an_end_caret_inside_its_horizontal_content_viewport() {
    let mut text = TextCtx::default();
    let _ = text.fonts.add_font(oxide_text::Font::from_bytes(
        include_bytes!("../assets/Asap-Regular.ttf").to_vec(),
    ));
    let input = TextInput::default();
    let mut state = TextInputState::new("Name");
    state.focus();
    state.handle_text_event(&TextEvent::Commit {text: "A long field value that must scroll to the end".into()});
    let rect = RectF::new(0.0, 0.0, 180.0, 52.0);
    let mut builder = DrawListBuilder::new();
    let mut uploader = CountingUploader::default();

    input.encode(&state, rect, 2.0, &mut text, &mut uploader, &mut builder);

    let content_right = rect.w - 1.5 - input.style.padding.right;
    let caret = builder.drawlist().items.iter().rev().find_map(|item| match item {
        DrawCmd::RRect {rect, color, ..} if *color == input.style.caret => Some(*rect),
        _ => None,
    }).expect("focused field emits a caret");
    assert!(caret.x >= input.style.padding.left);
    assert!(caret.x + caret.w <= content_right + f32::EPSILON);
}

#[test]
fn text_input_pointer_pick_uses_the_focused_horizontal_viewport_offset() {
    let mut text = TextCtx::default();
    let _ = text.fonts.add_font(oxide_text::Font::from_bytes(
        include_bytes!("../assets/Asap-Regular.ttf").to_vec(),
    ));
    let style = TextInputStyle {font_id: 0, ..TextInputStyle::default()};
    let mut state = TextInputState::new("Name");
    state.focus();
    state.handle_text_event(&TextEvent::Commit {text: "A long field value that must scroll to the end".into()});
    let length = state.text().chars().count();
    let rect = RectF::new(0.0, 0.0, 180.0, 52.0);

    state.handle_pointer_in_rect([style.padding.left + 4.0, 20.0], rect, &style, &mut text);

    assert!(state.cursor_index() > 0);
    assert!(state.cursor_index() < length);
}

#[test]
fn text_input_keeps_floating_placeholder_outside_the_scrolling_clip() {
    let mut text = TextCtx::default();
    let _ = text.fonts.add_font(oxide_text::Font::from_bytes(
        include_bytes!("../assets/Asap-Regular.ttf").to_vec(),
    ));
    let input = TextInput::default();
    let mut state = TextInputState::new("Email address");
    state.focus();
    for _ in 0..20 {state.tick(100);}
    let rect = RectF::new(0.0, 40.0, 180.0, 64.0);
    let mut builder = DrawListBuilder::new();
    let mut uploader = CountingUploader::default();

    input.encode(&state, rect, 2.0, &mut text, &mut uploader, &mut builder);

    let content_top = rect.y + 1.5 + input.style.padding.top;
    let glyph_top = builder.drawlist().vertices.iter().map(|vertex| vertex.y).fold(f32::INFINITY, f32::min);
    assert!(glyph_top < content_top, "floating placeholder was clipped: glyph_top={glyph_top} content_top={content_top}");
}

#[test]
fn text_input_encodes_unfocused_marked_text_without_a_caret() {
    let mut text = TextCtx::default();
    let _ = text.fonts.add_font(oxide_text::Font::from_bytes(
        include_bytes!("../assets/Asap-Regular.ttf").to_vec(),
    ));
    let input = TextInput::default();
    let mut state = TextInputState::new("");
    state.handle_text_event(&TextEvent::Composition {range: 0..0, text: "cafe\u{301}".into()});
    let mut builder = DrawListBuilder::new();
    let mut uploader = CountingUploader::default();

    input.encode(&state, RectF::new(0.0, 0.0, 180.0, 64.0), 2.0, &mut text, &mut uploader, &mut builder);

    assert!(builder.drawlist().items.iter().any(|item| matches!(item, DrawCmd::RRect {color, ..} if *color == input.style.composition)));
    assert!(!builder.drawlist().items.iter().any(|item| matches!(item, DrawCmd::RRect {color, ..} if *color == input.style.caret)));
}

#[test]
fn text_input_composition_underline_stays_close_to_baseline()
{
   let mut text = TextCtx::default();
   let font_id = text.fonts.add_font(oxide_text::Font::from_bytes(
      include_bytes!("../assets/Asap-Regular.ttf").to_vec(),
   ));
   for font_px in [18.0, 32.0]
   {
      let input = TextInput {style: TextInputStyle {font_id, font_px, ..TextInputStyle::default()}, ..TextInput::default()};
      let mut state = TextInputState::new("");
      state.handle_text_event(&TextEvent::Composition {range: 0..0, text: "cafe\u{301}".into()});
      let rect = RectF::new(0.0, 20.0, 240.0, 100.0);
      let mut builder = DrawListBuilder::new();
      let mut uploader = CountingUploader::default();
      input.encode(&state, rect, 3.0, &mut text, &mut uploader, &mut builder);
      let metrics = text.fonts.font(font_id).unwrap().line_metrics(font_px).unwrap();
      let content_height = rect.h - 3.0 - input.style.padding.top - input.style.padding.bottom;
      let baseline = rect.y + 1.5 + input.style.padding.top
         + (content_height - metrics.ascent - metrics.descent).max(0.0) * 0.5 + metrics.ascent;
      let underline = builder.drawlist().items.iter().find_map(|item| match item
      {
         DrawCmd::RRect {rect, color, ..} if *color == input.style.composition => Some(rect),
         _ => None,
      }).expect("marked text underline");
      assert!(underline.y >= baseline && underline.y <= baseline + 2.0,
         "underline must follow the baseline at {font_px}px: baseline={baseline}, underline={underline:?}");
   }
}

#[test]
fn text_input_centers_an_empty_placeholder_on_its_text_baseline() {
    let mut text = TextCtx::default();
    let _ = text.fonts.add_font(oxide_text::Font::from_bytes(
        include_bytes!("../assets/Asap-Regular.ttf").to_vec(),
    ));
    let input = TextInput::default();
    let state = TextInputState::new("Placeholder");
    let mut builder = DrawListBuilder::new();
    let mut uploader = CountingUploader::default();

    input.encode(&state, RectF::new(0.0, 0.0, 180.0, 68.0), 2.0, &mut text, &mut uploader, &mut builder);
    let top = builder.drawlist().vertices.iter().map(|vertex| vertex.y).fold(f32::INFINITY, f32::min);
    let bottom = builder.drawlist().vertices.iter().map(|vertex| vertex.y).fold(f32::NEG_INFINITY, f32::max);
    assert!(top > 18.0, "placeholder top={top}");
    assert!(bottom < 50.0, "placeholder bottom={bottom}");
}

#[test]
fn picker_centers_the_selected_row_on_its_font_baseline() {
    let mut text = TextCtx::default();
    let font_id = text.fonts.add_font(oxide_text::Font::from_bytes(
        include_bytes!("../assets/Asap-Regular.ttf").to_vec(),
    ));
    let mut picker = PickerState::new(vec!["Only".into()]);
    picker.set_column_selection(0, 0);
    let mut builder = DrawListBuilder::new();
    let mut uploader = CountingUploader::default();
    picker.encode(&PickerStyle {font_id, font_px: 18.0, baseline_shift: 0.0, ..PickerStyle::default()}, RectF::new(0.0, 0.0, 180.0, 192.0), 2.0, &mut text, &mut uploader, &mut builder);
    let top = builder.drawlist().vertices.iter().map(|vertex| vertex.y).fold(f32::INFINITY, f32::min);
    let bottom = builder.drawlist().vertices.iter().map(|vertex| vertex.y).fold(f32::NEG_INFINITY, f32::max);
    assert!(((top + bottom) * 0.5 - 96.0).abs() < 4.0, "top={top} bottom={bottom}");
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
fn text_input_max_length_counts_grapheme_clusters() {
    let mut st = TextInputState::new("Name");
    st.set_max_length(Some(1));
    st.focus();
    st.handle_text_event(&TextEvent::Commit { text: "e\u{301}x".into() });

    assert_eq!(st.text(), "e\u{301}");
    assert_eq!(st.cursor_index(), 1);
}

#[test]
fn text_input_composition_replaces_marked_range_on_commit() {
    let mut st = TextInputState::new("Name");
    st.focus();
    st.handle_text_event(&TextEvent::Commit { text: "oxide".into() });
    st.handle_text_event(&TextEvent::Composition { range: 1..4, text: "化".into() });
    assert_eq!(st.text(), "oxide");

    st.handle_text_event(&TextEvent::Commit { text: "化".into() });

    assert_eq!(st.text(), "o化e");
}

#[test]
fn text_input_composition_can_insert_at_cursor_and_cancel() {
    let mut st = TextInputState::new("Name");
    st.focus();
    st.handle_text_event(&TextEvent::Commit { text: "ab".into() });
    st.handle_text_event(&TextEvent::SelectionChanged { range: 1..1 });
    st.handle_text_event(&TextEvent::Composition { range: 1..1, text: "日".into() });
    assert_eq!(st.text(), "ab");
    st.handle_text_event(&TextEvent::Composition { range: 1..1, text: String::new() });
    st.handle_text_event(&TextEvent::Commit { text: "c".into() });

    assert_eq!(st.text(), "acb");
}

#[test]
fn text_input_cursor_and_delete_preserve_grapheme_clusters() {
    let mut st = TextInputState::new("Name");
    st.focus();
    st.handle_text_event(&TextEvent::Commit { text: "e\u{301}👨‍👩‍👧‍👦x".into() });
    st.move_cursor_to_end();
    let left = KeyEvent {
        code: KeyCode::ArrowLeft,
        chars: None,
        repeat: false,
        modifiers: Modifiers::empty(),
    };
    st.handle_key(&left);
    st.handle_key(&left);
    st.handle_text_event(&TextEvent::Commit { text: "!".into() });
    assert_eq!(st.text(), "e\u{301}!👨‍👩‍👧‍👦x");

    st.handle_key(&KeyEvent {
        code: KeyCode::Delete,
        chars: None,
        repeat: false,
        modifiers: Modifiers::empty(),
    });
    assert_eq!(st.text(), "e\u{301}!x");

    st.handle_key(&KeyEvent {
        code: KeyCode::Backspace,
        chars: None,
        repeat: false,
        modifiers: Modifiers::empty(),
    });
    st.handle_key(&KeyEvent {
        code: KeyCode::Backspace,
        chars: None,
        repeat: false,
        modifiers: Modifiers::empty(),
    });
    assert_eq!(st.text(), "x");
}

#[test]
fn text_input_pointer_pick_uses_grapheme_prefix_map() {
    let mut text = TextCtx::default();
    let font_id = text.fonts.add_font(oxide_text::Font::from_bytes(
        include_bytes!("../assets/Asap-Regular.ttf").to_vec(),
    ));
    let style = TextInputStyle { font_id, font_px: 16.0, ..TextInputStyle::default() };
    let mut shaper = oxide_text::TextShaper::default();
    let measure_font =
        oxide_text::Font::from_bytes(include_bytes!("../assets/Asap-Regular.ttf").to_vec());
    let first_width = shaper
        .shape(&measure_font, font_id, "e\u{301}", style.font_px)
        .expect("shape first cluster")
        .width();
    let mut st = TextInputState::new("Name");
    st.set_text("e\u{301}x");

    st.handle_pointer([style.padding.left + first_width + 1.0, 0.0], &style, &mut text);
    assert_eq!(st.cursor_index(), 1);
    st.handle_text_event(&TextEvent::Commit { text: "!".into() });

    assert_eq!(st.text(), "e\u{301}!x");
}

#[test]
fn text_input_pointer_pick_keeps_zwj_cluster_atomic() {
    let mut text = TextCtx::default();
    let font_bytes = include_bytes!("../assets/Asap-Regular.ttf").to_vec();
    let font_id = text.fonts.add_font(oxide_text::Font::from_bytes(font_bytes.clone()));
    let style = TextInputStyle { font_id, font_px: 16.0, ..TextInputStyle::default() };
    let measure_font = oxide_text::Font::from_bytes(font_bytes);
    let mut shaper = oxide_text::TextShaper::default();
    let first_width = shaper
        .shape(&measure_font, font_id, "a", style.font_px)
        .expect("shape first cluster")
        .width();
    let family = "👨‍👩‍👧‍👦";
    let mut st = TextInputState::new("Name");
    st.set_text(format!("a{family}b"));

    st.handle_pointer([style.padding.left + first_width + 1.0, 0.0], &style, &mut text);
    assert_eq!(st.cursor_index(), 1);
    st.handle_text_event(&TextEvent::Commit { text: "!".into() });

    assert_eq!(st.text(), format!("a!{family}b"));
}

#[test]
fn text_input_pointer_pick_handles_rtl_visual_order() {
    let Ok(font_bytes) = std::fs::read(MACOS_HEBREW_FONT) else {
        eprintln!("skipping RTL text-input pick test; {MACOS_HEBREW_FONT} is unavailable");
        return;
    };
    let mut text = TextCtx::default();
    let font_id = text.fonts.add_font(oxide_text::Font::from_bytes(font_bytes.clone()));
    let style = TextInputStyle { font_id, font_px: 16.0, ..TextInputStyle::default() };
    let measure_font = oxide_text::Font::from_bytes(font_bytes);
    let mut shaper = oxide_text::TextShaper::default();
    let rtl = "אבגדה";
    let width = shaper
        .shape(&measure_font, font_id, rtl, style.font_px)
        .expect("shape rtl text")
        .cursor_map_for_text(rtl)
        .width_at(0);
    let mut st = TextInputState::new("Name");
    st.set_text(rtl);

    st.handle_pointer([style.padding.left - 4.0, 0.0], &style, &mut text);
    assert_eq!(st.cursor_index(), 5);
    st.handle_text_event(&TextEvent::Commit { text: "!".into() });
    assert_eq!(st.text(), "אבגדה!");

    st.set_text(rtl);
    st.handle_pointer([style.padding.left + width + 4.0, 0.0], &style, &mut text);
    assert_eq!(st.cursor_index(), 0);
    st.handle_text_event(&TextEvent::Commit { text: "!".into() });
    assert_eq!(st.text(), "!אבגדה");
}

#[test]
fn text_input_pointer_pick_handles_mixed_bidi_run_interior() {
    let Ok(font_bytes) = std::fs::read(MACOS_HEBREW_FONT) else {
        eprintln!("skipping mixed-bidi text-input pick test; {MACOS_HEBREW_FONT} is unavailable");
        return;
    };
    let mut text = TextCtx::default();
    let font_id = text.fonts.add_font(oxide_text::Font::from_bytes(font_bytes.clone()));
    let style = TextInputStyle { font_id, font_px: 16.0, ..TextInputStyle::default() };
    let measure_font = oxide_text::Font::from_bytes(font_bytes);
    let mut shaper = oxide_text::TextShaper::default();
    let mixed = "AאבB";
    let map = shaper
        .shape(&measure_font, font_id, mixed, style.font_px)
        .expect("shape mixed-bidi text")
        .cursor_map_for_text(mixed);
    let mut st = TextInputState::new("Name");
    st.set_text(mixed);

    st.handle_pointer([style.padding.left + map.width_at(2), 0.0], &style, &mut text);
    assert_eq!(st.cursor_index(), 2);
    st.handle_text_event(&TextEvent::Commit { text: "!".into() });

    assert_eq!(st.text(), "Aא!בB");
}

#[test]
fn text_input_pointer_pick_uses_configured_fallback_font_widths() {
    let latin_bytes = include_bytes!("../../text/tests/fixtures/test_text_latin.ttf").to_vec();
    let cjk_bytes = include_bytes!("../../text/tests/fixtures/test_text_cjk.ttf").to_vec();
    let mut text = TextCtx::default();
    let latin_id = text.fonts.add_font(oxide_text::Font::from_bytes(latin_bytes.clone()));
    let cjk_id = text.fonts.add_font(oxide_text::Font::from_bytes(cjk_bytes.clone()));
    text.set_fallback_fonts(&[cjk_id]);
    let style = TextInputStyle { font_id: latin_id, font_px: 16.0, ..TextInputStyle::default() };
    let latin_font = oxide_text::Font::from_bytes(latin_bytes);
    let cjk_font = oxide_text::Font::from_bytes(cjk_bytes);
    let mut shaper = oxide_text::TextShaper::default();
    let a_width = shaper.shape(&latin_font, latin_id, "A", style.font_px).expect("shape A").width();
    let cjk_width =
        shaper.shape(&cjk_font, cjk_id, "漢", style.font_px).expect("shape cjk").width();
    let mut st = TextInputState::new("Name");
    st.set_text("A漢B");

    st.handle_pointer([style.padding.left + a_width + cjk_width + 1.0, 0.0], &style, &mut text);
    assert_eq!(st.cursor_index(), 2);
    st.handle_text_event(&TextEvent::Commit { text: "!".into() });

    assert_eq!(st.text(), "A漢!B");
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
fn text_input_one_time_code_centers_glyphs_on_the_slot_font_baseline() {
    let mut text = TextCtx::default();
    let font_id = text.fonts.add_font(oxide_text::Font::from_bytes(
        include_bytes!("../assets/Asap-Regular.ttf").to_vec(),
    ));
    let input = TextInput {style: TextInputStyle {font_id, font_px: 18.0, ..TextInputStyle::default()}, ..TextInput::default()};
    let mut state = TextInputState::new("OTP");
    state.configure_one_time_code(1);
    state.focus();
    state.handle_text_event(&TextEvent::Commit {text: "4".into()});
    let mut builder = DrawListBuilder::new();
    let mut uploader = CountingUploader::default();

    input.encode(&state, RectF::new(0.0, 0.0, 180.0, 64.0), 2.0, &mut text, &mut uploader, &mut builder);

    let glyph_top = builder.drawlist().vertices.iter().map(|vertex| vertex.y).fold(f32::INFINITY, f32::min);
    let glyph_bottom = builder.drawlist().vertices.iter().map(|vertex| vertex.y).fold(f32::NEG_INFINITY, f32::max);
    assert!(((glyph_top + glyph_bottom) * 0.5 - 32.0).abs() < 4.0, "top={glyph_top} bottom={glyph_bottom}");
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
