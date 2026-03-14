use crate::elements::{CharFilter, ShiftingTextInputState, ShiftingTextValidation};

const FIELD_FAIL_DURATION_MS: u32 = 420;
const FIELD_FAIL_SHAKE_AMPLITUDE_PX: f32 = 12.0;
const FIELD_FAIL_SHAKE_CYCLES: f32 = 4.0;
const SECURE_TEXT_DEFAULT_SHIFT_DISTANCE: f32 = 32.0;
const SECURE_TEXT_DEFAULT_ANIMATION_DURATION_MS: u32 = 1_200;
const LEGACY_SECURE_TEXT_REVEAL_DURATION_MS: u32 = 1_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldFailRestoreMode {
    Clear,
    RestoreValue,
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
        Self {
            filter,
            ..Self::default()
        }
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
        self.accepts_edit(input)
            .then(|| self.normalize_case(input))
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
        state.set_text(
            self.state.text.clone(),
            self.policy.filter(),
            self.policy.max_length(),
        );
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
        self.state
            .set_text(String::new(), self.policy.filter(), self.policy.max_length());
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
        self.fail_state
            .as_ref()
            .map_or(self.text(), |state| state.message.as_str())
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
        self.fail_state
            .as_ref()
            .map_or(self.text_before_caret(), |state| state.message.as_str())
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
        Self {
            policy,
            value: String::new(),
        }
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
        Self {
            inner: password,
            revealed_ms_remaining: 0,
        }
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
    input
        .char_indices()
        .nth(char_index)
        .map(|(idx, _)| idx)
        .unwrap_or(input.len())
}
