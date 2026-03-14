# ui-core `text_fields.rs`

## Intention and purpose
- Provide Oxide-owned, policy-driven text-input primitives that are generic enough for multiple apps.
- Centralize filtering, sanitization, caret editing, fail/restore animation state, and secure masking so app crates do not maintain parallel implementations.

## Relation to the rest of the code
- Built on top of `elements::CharFilter`, `elements::ShiftingTextInputState`, and `elements::ShiftingTextValidation`.
- Re-exported by `ui-core::lib`.
- Consumed directly by tests in `crates/ui-core/tests/text_fields_tests.rs` and indirectly by Nametag wrappers in `app/crates/ui/src/text_field.rs`, `horizontal_shifting_text.rs`, and `editable_text.rs`.

Call flow:
- `App field taxonomy` -> `TextFieldPolicy`
- `TextFieldPolicy` -> `HorizontalShiftingText` / `EditableText`
- `HorizontalShiftingText` -> `ShiftingTextInputState`
- `SecureText` -> `HorizontalShiftingText`

## Entry points list
- `FieldFailRestoreMode`
  Selects whether fail completion clears the field or restores the original value.
- `TextFieldPolicy::new(filter: CharFilter) -> TextFieldPolicy`
  Creates a policy with explicit character filtering.
- `TextFieldPolicy::{with_max_length, with_lowercase, with_trim_on_blur, with_first_token_only_on_set}`
  Builder methods that configure generic field behavior.
- `TextFieldPolicy::{filter, max_length, lowercases, trim_on_blur, accepts_edit, accept_edit, filter_input, sanitize, sanitize_external_input}`
  Read and execute the configured policy.
- `HorizontalShiftingText::{new, with_text, set_text, set, value, clear, apply_commit, focus, blur, blur_preserving_text, is_focused, text, display_text, shift_distance, animation_duration_ms, advance, pause, resume, reset, caret_index, set_caret_index, move_caret_left, move_caret_right, text_before_caret, display_text_before_caret, offset, fail_with_message, clear_fail, is_in_fail_mode, can_interact, fail_offset_px, fail_duration_ms, validation}`
  Generic caret-aware shifting input state with fail/restore handling.
- `EditableText::{new, policy, set, append, pop_last, apply_commit, value, clear}`
  Lightweight non-animated editable text state using the same policy contract.
- `SecureText::{new, from_horizontal_shifting_text, masked, display_text, display_text_before_caret, value, set, append, apply_commit, focus, blur, is_focused, advance, secure_now, reveal_active, fail_with_message, clear_fail, is_in_fail_mode, can_interact, fail_offset_px, offset, caret_index, set_caret_index, move_caret_left, move_caret_right}`
  Secure-text adapter that layers timed reveal and masking on top of `HorizontalShiftingText`.

## Logic narrative
- `TextFieldPolicy` is the reusable contract between app taxonomy and generic edit engines. It owns filtering, lowercasing, UTF-8-safe truncation, blur trimming, and the special "first token only on external set" hook needed by legacy username fields.
- `HorizontalShiftingText` owns the richer interactive state: caret position, deterministic horizontal offset animation, fail-state message swapping, and commit parsing for backspace and caret movement control characters.
- `EditableText` is the small value-only path for places that need the same edit acceptance rules without animation or focus state.
- `SecureText` wraps `HorizontalShiftingText` so secure entry reuses the same policy and caret engine while adding temporary reveal-after-edit and forced remasking on blur/failure.
- This split keeps generic edit behavior in one crate while allowing apps to define their own field taxonomies and copy.

## Preconditions and postconditions
- Callers must provide a valid `TextFieldPolicy`; all public constructors require one.
- Stored text always satisfies the configured character filter and max length.
- `sanitize_external_input()` is the only path that applies first-token-only normalization; plain `sanitize()` intentionally does not.
- `fail_with_message()` requires a non-empty message and asserts that contract.

## Invariants maintained
- Caret position never exceeds the current character count.
- Fail-state completion deterministically restores or clears according to `FieldFailRestoreMode`.
- `SecureText` masking state never exposes stale characters after `blur()` or `secure_now()`.
- Truncation never splits a UTF-8 scalar boundary.

## Edge cases and failure modes
- `max_length = Some(0)` produces empty sanitized output.
- Invalid or overflow commit candidates are rejected atomically instead of partially applied.
- Zero `shift_distance` produces zero offset.
- Zero animation duration is clamped to `1` millisecond.
- When fail mode is active, edit and caret mutation calls are ignored until the fail cycle ends.

## Concurrency and memory behavior
- All types are single-owner state machines with no internal synchronization.
- Mutating operations allocate only for owned `String` values used for filtered output, candidate edits, or masked display strings.

## Performance notes
- Policy filtering and commit application are O(n) over the affected text length.
- Animation advancement is O(1).
- Deduplicating these paths into Oxide removes repeated app-side logic without adding dynamic dispatch.

## Feature flags and cfgs
- No feature-specific behavior.

## Testing and benchmarks
- Covered by `crates/ui-core/tests/text_fields_tests.rs`.
- Downstream compatibility is also exercised by Nametag `crates/ui/tests/components_tests.rs`.

## Examples
```rust
use oxide_ui_core::{EditableText, FieldFailRestoreMode, HorizontalShiftingText, SecureText, TextFieldPolicy};
use oxide_ui_core::elements::CharFilter;

let policy = TextFieldPolicy::new(CharFilter::Alphabetic).with_max_length(Some(15));
let mut field = HorizontalShiftingText::new(policy.clone(), 32.0, 1_200).with_text("Victor");
field.focus();
field.apply_commit(" A");
assert_eq!(field.value(), "Victor A");

let mut secure = SecureText::new(EditableText::new(policy));
secure.focus();
secure.apply_commit("secret");
assert!(secure.reveal_active());
secure.fail_with_message("invalid", FieldFailRestoreMode::Clear);
```

## Changelog
- 2026-03-13: added Oxide-owned generic text-input primitives and moved app-agnostic editing behavior out of Nametag.
