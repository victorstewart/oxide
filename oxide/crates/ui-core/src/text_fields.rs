use crate::{
    bitmap_text::{draw_text_aligned, line_height, text_width, TextAlign, TextStyle},
    elements::{CharFilter, ShiftingTextInputState, ShiftingTextValidation},
};
use core::ops::Range;
use oxide_platform_api::TouchId;
use oxide_renderer_api as gfx;

const FIELD_FAIL_DURATION_MS: u32 = 420;
const FIELD_FAIL_SHAKE_AMPLITUDE_PX: f32 = 12.0;
const FIELD_FAIL_SHAKE_CYCLES: f32 = 4.0;
const SECURE_TEXT_DEFAULT_SHIFT_DISTANCE: f32 = 32.0;
const SECURE_TEXT_DEFAULT_ANIMATION_DURATION_MS: u32 = 1_200;
const LEGACY_SECURE_TEXT_REVEAL_DURATION_MS: u32 = 1_000;
const TEXT_INPUT_OPTIONS_HEIGHT_PT: f32 = 30.0;
const TEXT_INPUT_OPTIONS_ARROW_HEIGHT_PT: f32 = 6.0;
const TEXT_INPUT_OPTIONS_ARROW_HALF_WIDTH_PT: f32 = 8.0;
const TEXT_INPUT_OPTIONS_OUTLINE_PT: f32 = 1.0;
const TEXT_INPUT_OPTIONS_VIEWPORT_MARGIN_PT: f32 = 8.0;
const TEXT_INPUT_OPTIONS_FIELD_GAP_PT: f32 = 5.0;
const TEXT_INPUT_OPTIONS_HORIZONTAL_PADDING_PT: f32 = 10.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldFailRestoreMode {
    Clear,
    RestoreValue,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextCaretDragState<FieldId> {
    pub touch_id: TouchId,
    pub field: FieldId,
    pub start_x: f32,
    pub start_y: f32,
    pub current_x: f32,
    pub current_y: f32,
    pub max_move_sq: f32,
    pub started_focused: bool,
}

impl<FieldId: Copy> TextCaretDragState<FieldId> {
    #[must_use]
    pub const fn new(touch_id: TouchId, field: FieldId, x: f32, y: f32) -> Self {
        Self {
            touch_id,
            field,
            start_x: x,
            start_y: y,
            current_x: x,
            current_y: y,
            max_move_sq: 0.0,
            started_focused: false,
        }
    }

    #[must_use]
    pub const fn with_started_focused(mut self, started_focused: bool) -> Self {
        self.started_focused = started_focused;
        self
    }

    pub fn update(&mut self, x: f32, y: f32) {
        let dx = x - self.start_x;
        let dy = y - self.start_y;
        self.current_x = x;
        self.current_y = y;
        self.max_move_sq = self.max_move_sq.max(dx * dx + dy * dy);
    }

    #[must_use]
    pub fn is_tap_candidate(&self, tap_max_move: f32) -> bool {
        self.max_move_sq <= tap_max_move * tap_max_move
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextTapMemory<FieldId> {
    pub field: FieldId,
    pub x: f32,
    pub y: f32,
    pub ended_at_ms: u64,
}

impl<FieldId: Copy + PartialEq> TextTapMemory<FieldId> {
    #[must_use]
    pub const fn new(field: FieldId, x: f32, y: f32, ended_at_ms: u64) -> Self {
        Self { field, x, y, ended_at_ms }
    }

    #[must_use]
    pub fn is_double_tap(
        self,
        field: FieldId,
        x: f32,
        y: f32,
        now_ms: u64,
        max_ms: u64,
        max_move: f32,
    ) -> bool {
        if self.field != field || now_ms.saturating_sub(self.ended_at_ms) > max_ms {
            return false;
        }
        let dx = x - self.x;
        let dy = y - self.y;
        dx * dx + dy * dy <= max_move * max_move
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextSelectionState<FieldId> {
    pub field: FieldId,
    pub range: Range<usize>,
}

impl<FieldId> TextSelectionState<FieldId> {
    #[must_use]
    pub const fn new(field: FieldId, range: Range<usize>) -> Self {
        Self { field, range }
    }

    #[must_use]
    pub fn is_active(&self) -> bool {
        self.range.start < self.range.end
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextSelectionDragAnchor {
    Start,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextSelectionDragState<FieldId> {
    pub touch_id: TouchId,
    pub field: FieldId,
    pub anchor: TextSelectionDragAnchor,
    pub fixed_index: usize,
    pub current_index: usize,
}

impl<FieldId: Copy> TextSelectionDragState<FieldId> {
    #[must_use]
    pub const fn new(
        touch_id: TouchId,
        field: FieldId,
        anchor: TextSelectionDragAnchor,
        fixed_index: usize,
        current_index: usize,
    ) -> Self {
        Self { touch_id, field, anchor, fixed_index, current_index }
    }

    pub fn update_current_index(&mut self, current_index: usize) {
        self.current_index = current_index;
    }

    #[must_use]
    pub fn range(&self) -> Range<usize> {
        self.fixed_index.min(self.current_index)..self.fixed_index.max(self.current_index)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextSelectionHighlightStyle {
    pub fill: gfx::Color,
    pub border: gfx::Color,
    pub selected_text: gfx::Color,
    pub border_px: f32,
    pub y_pad: f32,
    pub radius_px: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextSelectionHighlightLayout {
    pub text_rect: gfx::RectF,
    pub token_rect: gfx::RectF,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextInputOption {
    SelectAll,
    Paste,
}

impl TextInputOption {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::SelectAll => "select all",
            Self::Paste => "paste",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextInputOptionsConfig {
    pub select_all: bool,
    pub paste: bool,
}

impl TextInputOptionsConfig {
    #[must_use]
    pub const fn none() -> Self {
        Self { select_all: false, paste: false }
    }

    #[must_use]
    pub const fn all() -> Self {
        Self { select_all: true, paste: true }
    }

    #[must_use]
    pub const fn option_count(self) -> usize {
        self.select_all as usize + self.paste as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextInputOptionsPopoverState<FieldId> {
    pub field: FieldId,
    pub opened_at_ms: u64,
}

impl<FieldId> TextInputOptionsPopoverState<FieldId> {
    #[must_use]
    pub const fn new(field: FieldId, opened_at_ms: u64) -> Self {
        Self { field, opened_at_ms }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextInputOptionsLayout {
    pub bubble_rect: gfx::RectF,
    pub select_all_rect: Option<gfx::RectF>,
    pub paste_rect: Option<gfx::RectF>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextInputOptionsPopoverStyle {
    pub background: gfx::Color,
    pub divider: gfx::Color,
    pub text: gfx::Color,
    pub text_px: f32,
}

#[must_use]
pub fn text_input_options_layout(
    field_rect: gfx::RectF,
    viewport: gfx::RectF,
    scale: f32,
    config: TextInputOptionsConfig,
    text_px: f32,
) -> Option<TextInputOptionsLayout> {
    let option_count = config.option_count();
    if option_count == 0 || field_rect.w <= 1.0 || field_rect.h <= 1.0 {
        return None;
    }
    let label_style = TextStyle::new(text_px, gfx::Color::rgba(1.0, 1.0, 1.0, 1.0)).bold();
    let padding = TEXT_INPUT_OPTIONS_HORIZONTAL_PADDING_PT * scale;
    let select_all_w =
        config.select_all.then(|| text_width(TextInputOption::SelectAll.label(), label_style));
    let paste_w = config.paste.then(|| text_width(TextInputOption::Paste.label(), label_style));
    let content_w = select_all_w.unwrap_or(0.0) + paste_w.unwrap_or(0.0);
    let bubble_w = content_w + padding * option_count as f32 * 2.0;
    let bubble_h = TEXT_INPUT_OPTIONS_HEIGHT_PT * scale;
    let arrow_h = TEXT_INPUT_OPTIONS_ARROW_HEIGHT_PT * scale;
    let gap = TEXT_INPUT_OPTIONS_FIELD_GAP_PT * scale;
    let margin = TEXT_INPUT_OPTIONS_VIEWPORT_MARGIN_PT * scale;
    let field_center_x = field_rect.x + field_rect.w * 0.50;
    let min_x = viewport.x + margin;
    let max_x = (viewport.x + viewport.w - bubble_w - margin).max(min_x);
    let bubble_x = (field_center_x - bubble_w * 0.50).clamp(min_x, max_x);
    let bubble_y = (field_rect.y - bubble_h - arrow_h - gap).max(viewport.y + margin);
    let bubble_rect = gfx::RectF::new(bubble_x, bubble_y, bubble_w, bubble_h);
    let mut option_x = bubble_rect.x;
    let select_all_rect = select_all_w.map(|width| {
        let rect = gfx::RectF::new(option_x, bubble_rect.y, width + padding * 2.0, bubble_rect.h);
        option_x += rect.w;
        rect
    });
    let paste_rect = paste_w.map(|width| {
        gfx::RectF::new(option_x, bubble_rect.y, width + padding * 2.0, bubble_rect.h)
    });
    Some(TextInputOptionsLayout { bubble_rect, select_all_rect, paste_rect })
}

#[must_use]
pub fn text_input_option_at(
    layout: TextInputOptionsLayout,
    x: f32,
    y: f32,
) -> Option<TextInputOption> {
    if layout.select_all_rect.is_some_and(|rect| rect_contains(rect, x, y)) {
        return Some(TextInputOption::SelectAll);
    }
    if layout.paste_rect.is_some_and(|rect| rect_contains(rect, x, y)) {
        return Some(TextInputOption::Paste);
    }
    None
}

pub fn draw_text_input_options_popover(
    encoder: &mut dyn gfx::RenderEncoder,
    layout: TextInputOptionsLayout,
    style: TextInputOptionsPopoverStyle,
) {
    let scale = (layout.bubble_rect.h / TEXT_INPUT_OPTIONS_HEIGHT_PT).max(1.0);
    let outline_w = (TEXT_INPUT_OPTIONS_OUTLINE_PT * scale).clamp(1.0, 2.0);
    let radius = layout.bubble_rect.h * 0.50;
    encoder.draw_rrect(layout.bubble_rect, [radius; 4], style.divider);
    let inner_rect = gfx::RectF::new(
        layout.bubble_rect.x + outline_w,
        layout.bubble_rect.y + outline_w,
        (layout.bubble_rect.w - outline_w * 2.0).max(1.0),
        (layout.bubble_rect.h - outline_w * 2.0).max(1.0),
    );
    encoder.draw_rrect(inner_rect, [(radius - outline_w).max(0.0); 4], style.background);
    let arrow_h = TEXT_INPUT_OPTIONS_ARROW_HEIGHT_PT * scale;
    let arrow_half_w = TEXT_INPUT_OPTIONS_ARROW_HALF_WIDTH_PT * scale;
    let arrow_center_x = layout.bubble_rect.x + layout.bubble_rect.w * 0.50;
    let arrow_top_y = layout.bubble_rect.y + layout.bubble_rect.h - outline_w * 0.50;
    let outline_arrow_h = arrow_h + outline_w;
    let outline_arrow_half_w = arrow_half_w + outline_w;
    encoder.draw_solid(
        &[
            vertex(arrow_center_x - outline_arrow_half_w, arrow_top_y),
            vertex(arrow_center_x + outline_arrow_half_w, arrow_top_y),
            vertex(arrow_center_x, arrow_top_y + outline_arrow_h),
        ],
        style.divider,
    );
    encoder.draw_solid(
        &[
            vertex(arrow_center_x - arrow_half_w, arrow_top_y),
            vertex(arrow_center_x + arrow_half_w, arrow_top_y),
            vertex(arrow_center_x, arrow_top_y + arrow_h),
        ],
        style.background,
    );
    let text_style = TextStyle::new(style.text_px, style.text).bold();
    if let (Some(select_all), Some(_)) = (layout.select_all_rect, layout.paste_rect) {
        let divider_x = select_all.x + select_all.w;
        let divider_w =
            (1.0 * (layout.bubble_rect.h / TEXT_INPUT_OPTIONS_HEIGHT_PT).max(1.0)).clamp(1.0, 2.0);
        let divider_margin = layout.bubble_rect.h * 0.22;
        encoder.draw_rrect(
            gfx::RectF::new(
                divider_x - divider_w * 0.50,
                select_all.y + divider_margin,
                divider_w,
                select_all.h - divider_margin * 2.0,
            ),
            [0.0; 4],
            style.divider,
        );
    }
    if let Some(rect) = layout.select_all_rect {
        let label_gap = TEXT_INPUT_OPTIONS_HORIZONTAL_PADDING_PT * scale;
        let label_rect = gfx::RectF::new(
            rect.x + label_gap,
            rect.y,
            (rect.w - label_gap * 2.0).max(1.0),
            rect.h,
        );
        draw_input_option_label(
            encoder,
            TextInputOption::SelectAll.label(),
            label_rect,
            text_style,
        );
    }
    if let Some(rect) = layout.paste_rect {
        let label_gap = TEXT_INPUT_OPTIONS_HORIZONTAL_PADDING_PT * scale;
        let label_rect = gfx::RectF::new(
            rect.x + label_gap,
            rect.y,
            (rect.w - label_gap * 2.0).max(1.0),
            rect.h,
        );
        draw_input_option_label(encoder, TextInputOption::Paste.label(), label_rect, text_style);
    }
}

#[must_use]
pub fn text_word_range_at_char_index(text: &str, char_index: usize) -> Range<usize> {
    let chars: alloc::vec::Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return 0..0;
    }
    let mut index = char_index.min(chars.len());
    if index == chars.len() || !text_word_char(chars[index]) {
        if index > 0 && text_word_char(chars[index - 1]) {
            index -= 1;
        }
    }
    if index >= chars.len() || !text_word_char(chars[index]) {
        return char_index.min(chars.len())..char_index.min(chars.len());
    }
    let mut start = index;
    while start > 0 && text_word_char(chars[start - 1]) {
        start -= 1;
    }
    let mut end = index + 1;
    while end < chars.len() && text_word_char(chars[end]) {
        end += 1;
    }
    start..end
}

#[must_use]
pub fn text_char_slice(input: &str, range: Range<usize>) -> String {
    let start = byte_index_for_char(input, range.start);
    let end = byte_index_for_char(input, range.end);
    input[start..end].to_owned()
}

#[must_use]
pub fn single_line_text_selection_rect(
    field_rect: gfx::RectF,
    text_x: f32,
    text: &str,
    style: TextStyle,
    range: Range<usize>,
) -> Option<gfx::RectF> {
    let char_len = char_count(text);
    let start = range.start.min(char_len);
    let end = range.end.min(char_len).max(start);
    if start >= end {
        return None;
    }
    let before = text_char_slice(text, 0..start);
    let selected = text_char_slice(text, start..end);
    let x = text_x + text_width(before.as_str(), style);
    let w = text_width(selected.as_str(), style).max(1.0);
    let h = line_height(style);
    Some(gfx::RectF::new(x, field_rect.y + (field_rect.h - h) * 0.50, w, h))
}

#[must_use]
pub fn single_line_text_selection_highlight_layout(
    field_rect: gfx::RectF,
    text_x: f32,
    text: &str,
    style: TextStyle,
    range: Range<usize>,
    highlight: TextSelectionHighlightStyle,
) -> Option<TextSelectionHighlightLayout> {
    let char_len = char_count(text);
    let start = range.start.min(char_len);
    let end = range.end.min(char_len).max(start);
    if start >= end {
        return None;
    }
    let before = text_char_slice(text, 0..start);
    let selected = text_char_slice(text, start..end);
    let selected_x = text_x + text_width(before.as_str(), style);
    let selected_w = text_width(selected.as_str(), style).max(1.0);
    let text_rect = gfx::RectF::new(
        selected_x,
        field_rect.y + (field_rect.h - line_height(style)) * 0.50,
        selected_w,
        line_height(style),
    );
    let token_rect = gfx::RectF::new(
        selected_x,
        text_rect.y - highlight.y_pad,
        selected_w,
        text_rect.h + highlight.y_pad * 2.0,
    );
    Some(TextSelectionHighlightLayout { text_rect, token_rect })
}

#[must_use]
pub fn single_line_text_selection_index_for_x(
    text_x: f32,
    text: &str,
    style: TextStyle,
    x: f32,
    anchor: TextSelectionDragAnchor,
) -> usize {
    let char_len = char_count(text);
    if char_len == 0 {
        return 0;
    }
    let mut boundaries = alloc::vec::Vec::with_capacity(char_len + 1);
    boundaries.push(text_x);
    let mut prefix = String::new();
    for ch in text.chars() {
        prefix.push(ch);
        boundaries.push(text_x + text_width(prefix.as_str(), style));
    }
    match anchor {
        TextSelectionDragAnchor::Start => {
            if x <= boundaries[0] {
                return 0;
            }
            for index in 1..boundaries.len() {
                if x < boundaries[index] {
                    return index - 1;
                }
            }
            char_len
        }
        TextSelectionDragAnchor::End => {
            if x <= boundaries[0] {
                return 0;
            }
            for index in 1..boundaries.len() {
                if x <= boundaries[index] {
                    return index;
                }
            }
            char_len
        }
    }
}

pub fn draw_text_selection_highlight(
    encoder: &mut dyn gfx::RenderEncoder,
    layout: TextSelectionHighlightLayout,
    style: TextSelectionHighlightStyle,
) {
    let radius = style.radius_px.min(layout.token_rect.h * 0.50).max(0.0);
    encoder.draw_rrect(layout.token_rect, [radius; 4], style.border);
    let border = style.border_px.clamp(0.0, layout.token_rect.w.min(layout.token_rect.h) * 0.45);
    if border > 0.0 {
        let inner = gfx::RectF::new(
            layout.token_rect.x + border,
            layout.token_rect.y + border,
            (layout.token_rect.w - border * 2.0).max(1.0),
            (layout.token_rect.h - border * 2.0).max(1.0),
        );
        encoder.draw_rrect(inner, [(radius - border).max(0.0); 4], style.fill);
    } else {
        encoder.draw_rrect(layout.token_rect, [radius; 4], style.fill);
    }
}

#[must_use]
pub fn text_selection_drag_anchor_at(
    layout: TextSelectionHighlightLayout,
    x: f32,
    y: f32,
    hit_padding: f32,
) -> Option<TextSelectionDragAnchor> {
    let hit_rect = gfx::RectF::new(
        layout.token_rect.x - hit_padding,
        layout.token_rect.y - hit_padding,
        layout.token_rect.w + hit_padding * 2.0,
        layout.token_rect.h + hit_padding * 2.0,
    );
    if !rect_contains(hit_rect, x, y) {
        return None;
    }
    let start_distance = (x - layout.token_rect.x).abs();
    let end_distance = (x - (layout.token_rect.x + layout.token_rect.w)).abs();
    Some(if start_distance <= end_distance {
        TextSelectionDragAnchor::Start
    } else {
        TextSelectionDragAnchor::End
    })
}

#[derive(Clone, Debug)]
pub struct TextFieldPolicy {
    filter: CharFilter,
    max_length: Option<usize>,
    lowercase: bool,
    trim_on_blur: bool,
    first_token_only_on_set: bool,
}

impl Default for TextFieldPolicy {
    fn default() -> Self {
        Self {
            filter: CharFilter::None,
            max_length: None,
            lowercase: false,
            trim_on_blur: true,
            first_token_only_on_set: false,
        }
    }
}

impl TextFieldPolicy {
    #[must_use]
    pub fn new(filter: CharFilter) -> Self {
        Self { filter, ..Self::default() }
    }

    #[must_use]
    pub fn with_max_length(mut self, max_length: Option<usize>) -> Self {
        self.max_length = max_length;
        self
    }

    #[must_use]
    pub fn with_lowercase(mut self, lowercase: bool) -> Self {
        self.lowercase = lowercase;
        self
    }

    #[must_use]
    pub fn with_trim_on_blur(mut self, trim_on_blur: bool) -> Self {
        self.trim_on_blur = trim_on_blur;
        self
    }

    #[must_use]
    pub fn with_first_token_only_on_set(mut self, first_token_only_on_set: bool) -> Self {
        self.first_token_only_on_set = first_token_only_on_set;
        self
    }

    #[must_use]
    pub fn filter(&self) -> &CharFilter {
        &self.filter
    }

    #[must_use]
    pub fn max_length(&self) -> Option<usize> {
        self.max_length
    }

    #[must_use]
    pub fn lowercases(&self) -> bool {
        self.lowercase
    }

    #[must_use]
    pub fn trim_on_blur(&self) -> bool {
        self.trim_on_blur
    }

    #[must_use]
    pub fn accepts_edit(&self, input: &str) -> bool {
        input.chars().count() <= self.max_length.unwrap_or(usize::MAX)
            && input.chars().all(|ch| self.filter.allows(ch))
    }

    #[must_use]
    pub fn accept_edit(&self, input: &str) -> Option<String> {
        self.accepts_edit(input).then(|| self.normalize_case(input))
    }

    #[must_use]
    pub fn filter_input(&self, input: &str) -> String {
        let mut output = String::new();
        for ch in input.chars() {
            if self.filter.allows(ch) {
                output.push(ch);
            }
        }
        output
    }

    #[must_use]
    pub fn sanitize(&self, input: &str) -> String {
        let filtered = self.filter_input(input);
        let normalized = self.normalize_case(&filtered);
        truncate(&normalized, self.max_length.unwrap_or(usize::MAX))
    }

    #[must_use]
    pub fn sanitize_external_input(&self, input: &str) -> String {
        let normalized = if self.first_token_only_on_set {
            input.split_whitespace().next().unwrap_or_default()
        } else {
            input
        };
        self.sanitize(normalized)
    }

    fn normalize_case(&self, input: &str) -> String {
        if self.lowercase {
            input.to_lowercase()
        } else {
            input.to_owned()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FieldFailState {
    original_text: String,
    message: String,
    restore_mode: FieldFailRestoreMode,
    elapsed_ms: u32,
}

pub struct HorizontalShiftingText {
    policy: TextFieldPolicy,
    state: ShiftingTextInputState,
    caret_index: usize,
    shift_distance: f32,
    animation_duration_ms: u32,
    elapsed_ms: u32,
    paused: bool,
    fail_state: Option<FieldFailState>,
}

impl core::fmt::Debug for HorizontalShiftingText {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("HorizontalShiftingText")
            .field("policy", &self.policy)
            .field("text", &self.state.text)
            .field("caret_index", &self.caret_index)
            .field("shift_distance", &self.shift_distance)
            .field("animation_duration_ms", &self.animation_duration_ms)
            .field("elapsed_ms", &self.elapsed_ms)
            .field("paused", &self.paused)
            .field("fail_state", &self.fail_state)
            .finish()
    }
}

impl Clone for HorizontalShiftingText {
    fn clone(&self) -> Self {
        let mut state = ShiftingTextInputState::default();
        state.set_text(self.state.text.clone(), self.policy.filter(), self.policy.max_length());
        if self.state.focused {
            state.on_focus();
        } else {
            state.on_blur();
        }
        if self.state.validation == ShiftingTextValidation::Invalid {
            state.fail();
        } else {
            state.clear_fail();
        }
        Self {
            policy: self.policy.clone(),
            state,
            caret_index: self.caret_index.min(char_count(&self.state.text)),
            shift_distance: self.shift_distance,
            animation_duration_ms: self.animation_duration_ms,
            elapsed_ms: self.elapsed_ms,
            paused: self.paused,
            fail_state: self.fail_state.clone(),
        }
    }
}

impl HorizontalShiftingText {
    #[must_use]
    pub fn new(policy: TextFieldPolicy, shift_distance: f32, animation_duration_ms: u32) -> Self {
        Self {
            policy,
            state: ShiftingTextInputState::default(),
            caret_index: 0,
            shift_distance: shift_distance.max(0.0),
            animation_duration_ms: animation_duration_ms.max(1),
            elapsed_ms: 0,
            paused: false,
            fail_state: None,
        }
    }

    #[must_use]
    pub fn with_text(mut self, text: &str) -> Self {
        self.set_text(text);
        self
    }

    #[must_use]
    pub fn policy(&self) -> &TextFieldPolicy {
        &self.policy
    }

    pub fn set_text(&mut self, input: &str) {
        self.cancel_fail_mode();
        let sanitized = self.policy.sanitize_external_input(input);
        self.set_state_text(sanitized);
        self.caret_index = self.caret_index.min(char_count(&self.state.text));
        self.state.tick();
    }

    pub fn set(&mut self, input: &str) {
        self.set_text(input);
    }

    #[must_use]
    pub fn value(&self) -> &str {
        self.text()
    }

    pub fn clear(&mut self) {
        self.cancel_fail_mode();
        self.state.set_text(String::new(), self.policy.filter(), self.policy.max_length());
        self.caret_index = 0;
        self.state.tick();
    }

    pub fn apply_commit(&mut self, input: &str) {
        if self.is_in_fail_mode() {
            return;
        }
        let mut next = self.state.text.clone();
        let mut caret_index = self.caret_index;
        let mut pending = String::new();
        let mut text_changed = false;
        for ch in input.chars() {
            match ch {
                '\u{8}' | '\u{7f}' => {
                    text_changed |=
                        self.flush_pending_insert(&mut next, &mut caret_index, &mut pending);
                    if caret_index > 0 {
                        let remove_start = byte_index_for_char(&next, caret_index - 1);
                        let remove_end = byte_index_for_char(&next, caret_index);
                        next.replace_range(remove_start..remove_end, "");
                        caret_index -= 1;
                        text_changed = true;
                    }
                }
                '\u{1c}' => {
                    text_changed |=
                        self.flush_pending_insert(&mut next, &mut caret_index, &mut pending);
                    if caret_index > 0 {
                        caret_index -= 1;
                    }
                }
                '\u{1d}' => {
                    text_changed |=
                        self.flush_pending_insert(&mut next, &mut caret_index, &mut pending);
                    let len = char_count(&next);
                    if caret_index < len {
                        caret_index += 1;
                    }
                }
                '\r' | '\n' => {
                    text_changed |=
                        self.flush_pending_insert(&mut next, &mut caret_index, &mut pending);
                }
                _ => {
                    pending.push(ch);
                }
            }
        }
        text_changed |= self.flush_pending_insert(&mut next, &mut caret_index, &mut pending);
        if text_changed {
            self.set_state_text(next);
            self.state.tick();
        }
        self.caret_index = caret_index.min(char_count(&self.state.text));
    }

    pub fn replace_char_range(&mut self, range: Range<usize>, input: &str) -> bool {
        if self.is_in_fail_mode() {
            return false;
        }
        let current = self.state.text.clone();
        let char_len = char_count(current.as_str());
        let start = range.start.min(char_len);
        let end = range.end.min(char_len).max(start);
        let prefix = text_char_slice(current.as_str(), 0..start);
        let suffix = text_char_slice(current.as_str(), end..char_len);
        let prefix_len = char_count(prefix.as_str());
        let suffix_len = char_count(suffix.as_str());
        let allowed_insert_len =
            self.policy.max_length().unwrap_or(usize::MAX).saturating_sub(prefix_len + suffix_len);
        let mut replacement =
            truncate(self.policy.filter_input(input).as_str(), allowed_insert_len);
        if self.policy.lowercases() {
            replacement = replacement.to_lowercase();
        }
        let mut next = String::with_capacity(prefix.len() + replacement.len() + suffix.len());
        next.push_str(prefix.as_str());
        next.push_str(replacement.as_str());
        next.push_str(suffix.as_str());
        let next = self.policy.sanitize(next.as_str());
        let caret_index =
            (prefix_len + char_count(replacement.as_str())).min(char_count(next.as_str()));
        let changed = next != current;
        if changed {
            self.set_state_text(next);
            self.state.tick();
        }
        self.caret_index = caret_index;
        changed
    }

    pub fn apply_selection_commit(&mut self, selection: Range<usize>, input: &str) -> Option<bool> {
        if selection.start >= selection.end {
            return None;
        }
        if input.chars().all(|ch| ch == '\u{1c}') {
            self.set_caret_index(selection.start);
            return Some(false);
        }
        if input.chars().all(|ch| ch == '\u{1d}') {
            self.set_caret_index(selection.end);
            return Some(false);
        }
        let replacement =
            if input.chars().all(|ch| ch == '\u{8}' || ch == '\u{7f}') { "" } else { input };
        Some(self.replace_char_range(selection, replacement))
    }

    pub fn focus(&mut self) {
        if self.is_in_fail_mode() {
            return;
        }
        self.state.on_focus();
        self.caret_index = char_count(&self.state.text);
        self.state.tick();
    }

    pub fn blur(&mut self) {
        if self.is_in_fail_mode() {
            self.blur_preserving_text();
            return;
        }
        self.trim_finished_text();
        self.state.on_blur();
        self.state.tick();
    }

    pub fn blur_preserving_text(&mut self) {
        self.state.on_blur();
        self.state.tick();
    }

    #[must_use]
    pub fn is_focused(&self) -> bool {
        self.state.focused
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.state.text
    }

    #[must_use]
    pub fn display_text(&self) -> &str {
        self.fail_state.as_ref().map_or(self.text(), |state| state.message.as_str())
    }

    #[must_use]
    pub fn shift_distance(&self) -> f32 {
        self.shift_distance
    }

    #[must_use]
    pub fn animation_duration_ms(&self) -> u32 {
        self.animation_duration_ms
    }

    pub fn advance(&mut self, delta_ms: u32) {
        if delta_ms == 0 {
            return;
        }
        if self.advance_fail_state(delta_ms) {
            return;
        }
        if self.paused {
            return;
        }
        let duration = self.animation_duration_ms.max(1);
        let increment = delta_ms % duration;
        self.elapsed_ms = (self.elapsed_ms + increment) % duration;
        self.state.tick();
    }

    pub fn pause(&mut self) {
        self.paused = true;
    }

    pub fn resume(&mut self) {
        self.paused = false;
    }

    pub fn reset(&mut self) {
        self.elapsed_ms = 0;
    }

    #[must_use]
    pub fn caret_index(&self) -> usize {
        self.caret_index
    }

    pub fn set_caret_index(&mut self, index: usize) {
        if self.is_in_fail_mode() {
            return;
        }
        self.caret_index = index.min(char_count(&self.state.text));
    }

    pub fn move_caret_left(&mut self) {
        if self.is_in_fail_mode() {
            return;
        }
        if self.caret_index > 0 {
            self.caret_index -= 1;
        }
    }

    pub fn move_caret_right(&mut self) {
        if self.is_in_fail_mode() {
            return;
        }
        let len = char_count(&self.state.text);
        if self.caret_index < len {
            self.caret_index += 1;
        }
    }

    #[must_use]
    pub fn text_before_caret(&self) -> &str {
        let end = byte_index_for_char(&self.state.text, self.caret_index);
        &self.state.text[..end]
    }

    #[must_use]
    pub fn display_text_before_caret(&self) -> &str {
        self.fail_state.as_ref().map_or(self.text_before_caret(), |state| state.message.as_str())
    }

    #[must_use]
    pub fn offset(&self) -> f32 {
        if self.is_in_fail_mode() {
            return 0.0;
        }
        if self.shift_distance <= 0.0 {
            return 0.0;
        }
        let duration = self.animation_duration_ms.max(1) as f32;
        let progress = self.elapsed_ms as f32 / duration;
        self.shift_distance * progress
    }

    pub fn fail_with_message(&mut self, message: &str, restore_mode: FieldFailRestoreMode) {
        assert!(!message.is_empty(), "fail message must not be empty");
        self.blur_preserving_text();
        self.fail_state = Some(FieldFailState {
            original_text: self.state.text.clone(),
            message: message.to_owned(),
            restore_mode,
            elapsed_ms: 0,
        });
        self.state.fail();
        self.state.tick();
    }

    pub fn clear_fail(&mut self) {
        self.fail_state = None;
        self.state.clear_fail();
        self.state.tick();
    }

    #[must_use]
    pub fn is_in_fail_mode(&self) -> bool {
        self.fail_state.is_some()
    }

    #[must_use]
    pub fn can_interact(&self) -> bool {
        !self.is_in_fail_mode()
    }

    #[must_use]
    pub fn fail_offset_px(&self) -> f32 {
        let Some(state) = self.fail_state.as_ref() else {
            return 0.0;
        };
        let progress = (state.elapsed_ms as f32 / FIELD_FAIL_DURATION_MS as f32).clamp(0.0, 1.0);
        let wave = (progress * std::f32::consts::TAU * FIELD_FAIL_SHAKE_CYCLES).sin();
        let envelope = (1.0 - progress).max(0.0);
        wave * envelope * FIELD_FAIL_SHAKE_AMPLITUDE_PX
    }

    #[must_use]
    pub const fn fail_duration_ms() -> u32 {
        FIELD_FAIL_DURATION_MS
    }

    #[must_use]
    pub fn validation(&self) -> ShiftingTextValidation {
        self.state.validation
    }

    fn set_state_text(&mut self, text: String) {
        self.state.text = text;
    }

    fn advance_fail_state(&mut self, delta_ms: u32) -> bool {
        let Some(state) = self.fail_state.as_mut() else {
            return false;
        };
        state.elapsed_ms = state.elapsed_ms.saturating_add(delta_ms);
        if state.elapsed_ms < FIELD_FAIL_DURATION_MS {
            self.state.tick();
            return true;
        }

        let Some(completed) = self.fail_state.take() else {
            return true;
        };
        self.state.clear_fail();
        match completed.restore_mode {
            FieldFailRestoreMode::Clear => {
                self.state.text.clear();
                self.caret_index = 0;
            }
            FieldFailRestoreMode::RestoreValue => {
                self.state.text = completed.original_text;
                self.caret_index = self.caret_index.min(char_count(&self.state.text));
            }
        }
        self.state.tick();
        true
    }

    fn cancel_fail_mode(&mut self) {
        if self.fail_state.take().is_some() {
            self.state.clear_fail();
        }
    }

    fn trim_finished_text(&mut self) {
        if !self.policy.trim_on_blur() {
            return;
        }
        let trimmed = self.state.text.trim();
        if trimmed.len() == self.state.text.len() {
            return;
        }
        if trimmed.is_empty() {
            self.state.text.clear();
            self.caret_index = 0;
            return;
        }
        self.state.text = trimmed.to_owned();
        self.caret_index = self.caret_index.min(char_count(&self.state.text));
    }

    fn flush_pending_insert(
        &self,
        next: &mut String,
        caret_index: &mut usize,
        pending: &mut String,
    ) -> bool {
        if pending.is_empty() {
            return false;
        }
        let insert_at = byte_index_for_char(next, *caret_index);
        let pending_len = char_count(pending);
        let mut candidate = next.clone();
        candidate.insert_str(insert_at, pending);
        pending.clear();
        let Some(accepted) = self.policy.accept_edit(&candidate) else {
            return false;
        };
        *next = accepted;
        *caret_index = (*caret_index + pending_len).min(char_count(next));
        true
    }
}

#[derive(Debug, Clone)]
pub struct EditableText {
    policy: TextFieldPolicy,
    value: String,
}

impl EditableText {
    #[must_use]
    pub fn new(policy: TextFieldPolicy) -> Self {
        Self { policy, value: String::new() }
    }

    #[must_use]
    pub fn policy(&self) -> &TextFieldPolicy {
        &self.policy
    }

    pub fn set(&mut self, input: &str) {
        self.value = self.policy.sanitize(input);
    }

    pub fn append(&mut self, input: &str) {
        let mut combined = self.value.clone();
        combined.push_str(input);
        if let Some(next) = self.policy.accept_edit(&combined) {
            self.value = next;
        }
    }

    pub fn pop_last(&mut self) {
        let _ = self.value.pop();
    }

    pub fn apply_commit(&mut self, input: &str) {
        let mut pending = String::new();
        for ch in input.chars() {
            match ch {
                '\u{8}' | '\u{7f}' => {
                    self.flush_pending(&mut pending);
                    self.pop_last();
                }
                '\u{1c}' | '\u{1d}' | '\r' | '\n' => {
                    self.flush_pending(&mut pending);
                }
                _ => {
                    pending.push(ch);
                }
            }
        }
        self.flush_pending(&mut pending);
    }

    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn clear(&mut self) {
        self.value.clear();
    }

    fn flush_pending(&mut self, pending: &mut String) {
        if pending.is_empty() {
            return;
        }
        self.append(pending);
        pending.clear();
    }
}

#[derive(Debug, Clone)]
pub struct SecureText {
    inner: HorizontalShiftingText,
    revealed_ms_remaining: u32,
}

impl SecureText {
    #[must_use]
    pub fn new(password: EditableText) -> Self {
        Self::from_horizontal_shifting_text(
            HorizontalShiftingText::new(
                password.policy().clone(),
                SECURE_TEXT_DEFAULT_SHIFT_DISTANCE,
                SECURE_TEXT_DEFAULT_ANIMATION_DURATION_MS,
            )
            .with_text(password.value()),
        )
    }

    #[must_use]
    pub fn from_horizontal_shifting_text(password: HorizontalShiftingText) -> Self {
        Self { inner: password, revealed_ms_remaining: 0 }
    }

    #[must_use]
    pub fn masked(&self) -> String {
        self.display_text()
    }

    #[must_use]
    pub fn display_text(&self) -> String {
        if self.inner.is_in_fail_mode() {
            return self.inner.display_text().to_owned();
        }
        if self.revealed_ms_remaining > 0 {
            self.inner.value().to_owned()
        } else {
            "*".repeat(self.inner.value().chars().count())
        }
    }

    #[must_use]
    pub fn display_text_before_caret(&self) -> String {
        if self.inner.is_in_fail_mode() {
            return self.inner.display_text_before_caret().to_owned();
        }
        if self.revealed_ms_remaining > 0 {
            self.inner.text_before_caret().to_owned()
        } else {
            "*".repeat(self.inner.text_before_caret().chars().count())
        }
    }

    #[must_use]
    pub fn value(&self) -> &str {
        self.inner.value()
    }

    pub fn set(&mut self, input: &str) {
        self.inner.set(input);
        self.secure_now();
    }

    pub fn append(&mut self, input: &str) {
        let previous = self.inner.value().to_owned();
        self.inner.apply_commit(input);
        self.refresh_reveal_after_edit(previous.as_str());
    }

    pub fn apply_commit(&mut self, input: &str) {
        let previous = self.inner.value().to_owned();
        self.inner.apply_commit(input);
        self.refresh_reveal_after_edit(previous.as_str());
    }

    pub fn focus(&mut self) {
        self.inner.focus();
    }

    pub fn blur(&mut self) {
        self.inner.blur_preserving_text();
        self.secure_now();
    }

    #[must_use]
    pub fn is_focused(&self) -> bool {
        self.inner.is_focused()
    }

    pub fn advance(&mut self, delta_ms: u32) {
        self.inner.advance(delta_ms);
        if self.inner.is_in_fail_mode() {
            self.revealed_ms_remaining = 0;
            return;
        }
        self.revealed_ms_remaining = self.revealed_ms_remaining.saturating_sub(delta_ms);
    }

    pub fn secure_now(&mut self) {
        self.revealed_ms_remaining = 0;
    }

    #[must_use]
    pub fn reveal_active(&self) -> bool {
        self.revealed_ms_remaining > 0
    }

    pub fn fail_with_message(&mut self, message: &str, restore_mode: FieldFailRestoreMode) {
        self.secure_now();
        self.inner.fail_with_message(message, restore_mode);
    }

    pub fn clear_fail(&mut self) {
        self.inner.clear_fail();
    }

    #[must_use]
    pub fn is_in_fail_mode(&self) -> bool {
        self.inner.is_in_fail_mode()
    }

    #[must_use]
    pub fn can_interact(&self) -> bool {
        self.inner.can_interact()
    }

    #[must_use]
    pub fn fail_offset_px(&self) -> f32 {
        self.inner.fail_offset_px()
    }

    #[must_use]
    pub fn offset(&self) -> f32 {
        self.inner.offset()
    }

    #[must_use]
    pub fn caret_index(&self) -> usize {
        self.inner.caret_index()
    }

    pub fn set_caret_index(&mut self, index: usize) {
        self.inner.set_caret_index(index);
    }

    pub fn move_caret_left(&mut self) {
        self.inner.move_caret_left();
    }

    pub fn move_caret_right(&mut self) {
        self.inner.move_caret_right();
    }

    fn refresh_reveal_after_edit(&mut self, previous: &str) {
        if self.inner.value() == previous {
            return;
        }
        self.refresh_reveal();
    }

    fn refresh_reveal(&mut self) {
        if self.inner.value().is_empty() {
            self.revealed_ms_remaining = 0;
        } else {
            self.revealed_ms_remaining = LEGACY_SECURE_TEXT_REVEAL_DURATION_MS;
        }
    }
}

fn truncate(input: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if max == usize::MAX {
        return input.to_owned();
    }

    let mut count = 0usize;
    let mut end = input.len();
    for (idx, _) in input.char_indices() {
        if count == max {
            end = idx;
            break;
        }
        count += 1;
    }

    input[..end].to_owned()
}

fn char_count(input: &str) -> usize {
    input.chars().count()
}

fn byte_index_for_char(input: &str, char_index: usize) -> usize {
    if char_index == 0 {
        return 0;
    }
    input.char_indices().nth(char_index).map(|(idx, _)| idx).unwrap_or(input.len())
}

fn rect_contains(rect: gfx::RectF, x: f32, y: f32) -> bool {
    x >= rect.x && y >= rect.y && x <= rect.x + rect.w && y <= rect.y + rect.h
}

fn draw_input_option_label(
    encoder: &mut dyn gfx::RenderEncoder,
    label: &str,
    rect: gfx::RectF,
    style: TextStyle,
) {
    let line_h = line_height(style);
    let label_rect = gfx::RectF::new(rect.x, rect.y + (rect.h - line_h) * 0.50, rect.w, line_h);
    draw_text_aligned(encoder, label, label_rect, TextAlign::Center, style);
}

fn vertex(x: f32, y: f32) -> gfx::Vertex {
    gfx::Vertex { x, y, u: 0.0, v: 0.0, rgba: 0xFFFF_FFFF }
}

fn text_word_char(ch: char) -> bool {
    !ch.is_whitespace()
}
