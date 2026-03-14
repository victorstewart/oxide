# ui-core `lib.rs`

## Intention and purpose
- Define the framework-level UI primitives that higher-level apps consume.
- Re-export the generic text-input building blocks so apps can share one implementation for field-policy execution, shifting/caret behavior, and secure masking.

## Relation to the rest of the code
- `oxide-ui-core` sits above renderer/platform crates and below app crates such as Nametag.
- `text_fields.rs` now owns the generic policy-driven text-editing state machines.
- App crates can either consume the Oxide types directly or wrap them with app-local taxonomy adapters such as Nametag `FieldKind`.

## Entry points list
- `pub mod anim`
  Exposes shared animation helpers for reusable easing and keyframed offset sampling.
- `pub mod text_fields`
  Exposes the generic policy-driven text-input module.
- `pub mod picker_popup`
  Exposes the generic popup/wheel-picker interaction module.
- `pub use text_fields::{EditableText, FieldFailRestoreMode, HorizontalShiftingText, SecureText, TextFieldPolicy}`
  Makes the text-input primitives available from the crate root so app wrappers do not need to reach into module internals.
- `pub use picker_popup::{PanelPopupState, PopupTapRegion, PopupWheelPickerState, WheelPickerState}`
  Makes the shared popup dismissal and wheel-picker drag/snap controllers available from the crate root.

## Logic narrative
- `lib.rs` remains the crate aggregation layer.
- The animation move adds shared bezier/keyframed-offset helpers so app crates no longer keep duplicate motion math for standard swap or recovery-shake profiles.
- The text-input move adds one more crate-root export surface so generic input primitives live beside the existing drawing, overlay, animation, and design-system utilities.
- The popup-picker move follows the same boundary: Oxide owns the reusable interaction state, while apps keep their own anchored layouts, copy, and visual treatments.
- This keeps ownership clear: Oxide owns reusable UI state machines; app crates own field naming, copy, and scene composition.

## Preconditions and postconditions
- Downstream code that imports the crate root now receives the same text-input types regardless of which app consumes Oxide.
- No public Nametag-specific concepts are exposed from this file.

## Edge cases and failure modes
- The crate root does not add new runtime failure paths; it only re-exports the new module.

## Concurrency and memory behavior
- No additional state is introduced at the crate root.
- Memory and synchronization behavior are defined by the exported modules themselves.

## Performance notes
- Crate-root re-exports are zero-cost.
- Consolidating the text-input engines here removes duplicate app-side implementations without adding runtime indirection.

## Feature flags and cfgs
- The text-fields export is always enabled.

## Testing and benchmarks
- `crates/ui-core/tests/anim_helpers.rs` covers the shared animation-helper surface.
- `crates/ui-core/tests/text_fields_tests.rs` covers the text-input surface.
- `crates/ui-core/tests/picker_popup_tests.rs` covers the popup-picker interaction surface.

## Examples
```rust
use oxide_ui_core::{HorizontalShiftingText, TextFieldPolicy};
use oxide_ui_core::elements::CharFilter;

let policy = TextFieldPolicy::new(CharFilter::Alphabetic).with_max_length(Some(15));
let text = HorizontalShiftingText::new(policy, 32.0, 1_200);
assert_eq!(text.value(), "");
```

## Changelog
- 2026-03-13: centralized shared cubic-bezier easing and required-field shake helpers in `anim.rs` so app crates can drop duplicated motion math.
- 2026-03-13: re-exported the generic text-input primitives from the crate root so app crates can deduplicate their input state machines onto Oxide.
- 2026-03-13: re-exported the generic popup/wheel-picker interaction primitives so app crates can share one drag/snap/dismiss controller instead of reimplementing them in scene code.
