# ui-core tests `text_fields_tests.rs`

## Intention and purpose
- Prove the new Oxide-owned text-input primitives preserve the behavior that downstream apps were relying on before deduplication.

## Relation to the rest of the code
- Exercises `ui-core::text_fields`.
- Acts as the framework-level regression suite for app wrappers such as Nametag `FieldKind` adapters.

## Entry points list
- `text_fields_editable_rejects_invalid_and_overflow_commits_without_partial_changes`
- `text_fields_horizontal_set_uses_external_normalization_policy`
- `text_fields_editable_set_keeps_generic_sanitization_without_external_token_split`
- `text_fields_horizontal_blur_trims_finished_text_and_clears_blank_values`
- `text_fields_horizontal_fail_modes_clear_and_restore_value`
- `text_fields_horizontal_supports_caret_movement_and_mid_string_insertions`
- `text_fields_horizontal_edits_by_grapheme_cluster`
- `text_fields_editable_backspace_removes_grapheme_cluster`
- `text_fields_secure_mask_counts_grapheme_clusters`
- `text_fields_secure_reveals_then_re_masks_and_bypasses_masking_for_failures`

## Logic narrative
- The suite starts by validating the small policy-only path (`EditableText`) so rejected edits do not partially mutate state.
- It then checks the external-set normalization hook that legacy username fields need while proving plain sanitization does not accidentally split tokens.
- The remaining cases cover blur trimming, fail/restore timing, caret-aware insert/delete behavior, grapheme-safe edit boundaries, and secure reveal/remask rules.

## Preconditions and postconditions
- Each test constructs policies directly through the public Oxide API.
- Passing results mean the generic contract remains stable for downstream app wrappers.

## Edge cases and failure modes
- Invalid edits and max-length overflow are explicitly covered.
- Blank blur trimming and fail-cycle completion are covered.
- Secure fail-state display is covered so masking does not hide validation errors.
- Combining-mark and ZWJ-style grapheme clusters are covered so backspace, secure masks, and caret-aware replacement cannot split user-visible characters.

## Concurrency and memory behavior
- Tests are single-threaded and do not rely on wall-clock sleeps; time advancement uses deterministic `advance()` calls.

## Performance notes
- This suite targets correctness, not benchmarking.

## Feature flags and cfgs
- No feature-specific branches.

## Testing and benchmarks
- Run with `cargo test --locked -p oxide-ui-core --test text_fields_tests`.

## Examples
```rust
cargo test --locked -p oxide-ui-core --test text_fields_tests
```

## Changelog
- 2026-05-31: added grapheme-cluster coverage for generic editable backspace, horizontal editing, and secure text masking.
- 2026-03-13: added framework-level regression coverage for the text-input primitives migrated out of Nametag.
