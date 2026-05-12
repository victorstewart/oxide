# ui-core `lib.rs`

## Intention and purpose
- Define the framework-level UI primitives that higher-level apps consume.
- Re-export the generic text-input building blocks so apps can share one implementation for field-policy execution, shifting/caret behavior, and secure masking.
- Keep shared legacy modal chrome in `elements.rs` so apps do not duplicate fullscreen blur, popup blur, border, and inner-fill math per scene.
- Keep shared legacy badge overlay geometry in `elements.rs` so apps do not duplicate the old iOS quarter-size top-right badge placement or bounce defaults.
- Keep shared legacy spinner defaults in `elements.rs` so apps do not duplicate the old iOS large activity-indicator sizing and fallback animation rules.
- Keep shared legacy sliding-switch gesture semantics in `elements.rs` so apps do not duplicate the old iOS long-press gate, inactivity timeout, or outside-cancel behavior.

## Relation to the rest of the code
- `oxide-ui-core` sits above renderer/platform crates and below app crates such as Nametag.
- `text_fields.rs` now owns the generic policy-driven text-editing state machines.
- App crates can either consume the Oxide types directly or wrap them with app-local taxonomy adapters such as Nametag `FieldKind`.

## Entry points list
- `pub mod overlay`
  Exposes the shared overlay and popup-lifecycle infrastructure used by higher-level surface routers.
- `pub mod anim`
  Exposes shared animation helpers for reusable easing and keyframed offset sampling.
- `pub mod text_fields`
  Exposes the generic policy-driven text-input module.
- `pub mod picker_popup`
  Exposes the generic popup/legacy-picker interaction module.
- `pub mod emitter`
  Exposes the shared CAEmitter-style burst sampler used by downstream app particle effects.
- `pub use text_fields::{EditableText, FieldFailRestoreMode, HorizontalShiftingText, SecureText, TextFieldPolicy}`
  Makes the text-input primitives available from the crate root so app wrappers do not need to reach into module internals.
- `pub use picker_popup::{PanelPopupState, PickerColumnCommit, PickerColumnState, PopupPickerState, PopupTapRegion}`
  Makes the shared popup dismissal and legacy picker drag/snap/commit controllers available from the crate root.
- `pub use emitter::{BurstEmitter, BurstEmitterCellConfig, BurstEmitterConfig, BurstEmitterParticle, BurstEmitterShape}`
  Makes the shared CAEmitter-style burst API available from the crate root so app crates can reuse the same particle timing and source-shape logic.
- `pub use overlay::{PopupCallbacks, PopupManager, PopupSpec, PopupTouchRegion}`
  Makes the shared popup lifecycle contract available from the crate root so app crates can reuse key-popup lookup, dismissal approval, touch-exception routing, and content-size resync without rebuilding window semantics per scene.

## Logic narrative
- `lib.rs` remains the crate aggregation layer.
- The animation move adds shared bezier/keyframed-offset helpers so app crates no longer keep duplicate motion math for standard swap or recovery-shake profiles.
- The text-input move adds one more crate-root export surface so generic input primitives live beside the existing drawing, overlay, animation, and design-system utilities.
- `elements.rs` now also owns the old iOS modal popup chrome contract, so downstream apps can reuse one resolved blur-card treatment instead of hard-coding sigma, alpha, radius, and border math in scene code.
- `elements.rs` also owns the old iOS badge overlay contract, so downstream apps can reuse one image-first badge treatment instead of keeping count-pill logic or local placement math.
- `elements.rs` also owns the old iOS spinner contract, so downstream apps stop passing phase or stroke data and instead issue one atom-driven large-indicator request.
- `elements.rs` also owns the old iOS sliding-switch interaction contract, so downstream apps stop re-implementing the 0.3s press gate, one-shot inactivity callback semantics, and bounds cancellation around `SlidingSwitchState`.
- The popup-picker move follows the same boundary: Oxide owns the reusable multi-column legacy-picker interaction state, scroll-end commit result, and fixed medium-impact haptic intent, while apps keep their own anchored layouts, copy, and visual treatments.
- The emitter move follows that same pattern: Oxide owns the reusable burst timing, source-shape, and particle sampling math, while apps keep scene-specific asset choice and draw calls.
- The spinner move follows the same rule at runtime too: the iOS host can now promote spinner draws into native `UIActivityIndicatorViewStyleLarge` views while non-iOS fallbacks still share one Oxide-owned contract.
- The popup lifecycle move follows that same rule: Oxide now owns the reusable key-popup, approval-gated dismissal, manual or content-root touch-exception, and content-size refresh contract, while apps keep scene-specific copy and mutation policy.
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
- `prepare_draws` preallocates the resolved clip stack for the common shallow nested-clip path, avoiding the first frame-loop stack growth on representative clipping workloads.
- `elements::Label` keeps disabled watch logging off the allocation path, preallocates the common wrapped-line buffers, and lets internal non-wrapped label call sites encode borrowed text directly instead of cloning through temporary `Label` values.
- Wrapped `elements::Label` now reuses the shaped line outputs it created during width fitting and uploads the text atlas once after all line baking, avoiding the old second shape pass over every final wrapped line.

## Feature flags and cfgs
- The text-fields export is always enabled.

## Testing and benchmarks
- `crates/ui-core/tests/elements_tests.rs` covers the shared overlay and popup chrome contract.
- `crates/ui-core/tests/overlay_tests.rs` covers the shared popup lifecycle contract.
- `crates/ui-core/tests/elements_tests.rs` also covers the shared legacy badge overlay contract.
- `crates/ui-core/tests/elements_tests.rs` also covers the shared legacy spinner defaults and atom-driven encoding contract.
- `crates/ui-core/tests/elements_tests.rs` also covers the shared legacy sliding-switch long-press, timeout, and bounds-cancel contract.
- `crates/ui-core/tests/anim_helpers.rs` covers the shared animation-helper surface.
- `crates/ui-core/tests/text_fields_tests.rs` covers the text-input surface.
- `crates/ui-core/tests/picker_popup_tests.rs` covers the popup-picker interaction surface.
- `crates/ui-core/tests/emitter_tests.rs` covers the CAEmitter-style burst surface.

## Examples
```rust
use oxide_ui_core::{HorizontalShiftingText, TextFieldPolicy};
use oxide_ui_core::elements::CharFilter;

let policy = TextFieldPolicy::new(CharFilter::Alphabetic).with_max_length(Some(15));
let text = HorizontalShiftingText::new(policy, 32.0, 1_200);
assert_eq!(text.value(), "");
```

## Changelog
- 2026-05-10: reused the existing character-range byte mapping in `elements.rs` text insertion and removed the single-point byte helper.
- 2026-04-25: reused wrapped-label shaping results after release-mode A/B showed `cpu.component.label.encode` improving from p50 1155.122 us/op, p95 1165.781 to p50 1013.186 us/op, p95 1037.539 in focused runs, with the refreshed full workspace row at p50 987.312 us/op, p95 1004.876.
- 2026-04-25: preallocated the `prepare_draws` clip stack after release-mode A/B showed the representative clipping workload improving from p50 6.881 us/op, p95 10.977 to p50 5.368 us/op, p95 5.398.
- 2026-03-28: moved the legacy iOS modal overlay and popup blur-card contract into shared `elements.rs` popup primitives so downstream apps can draw one common fullscreen/panel blur treatment.
- 2026-03-28: moved the popup key-window, dismissal approval, touch-exception, and content-size refresh lifecycle contract into shared `overlay.rs` popup primitives so downstream apps stop rebuilding those window semantics locally.
- 2026-03-28: moved the legacy iOS `Badge` / `BadgeableButton` overlay contract into shared `elements.rs` badge primitives so downstream apps draw the image-backed quarter-size top-right badge instead of a numeric pill.
- 2026-03-28: moved the legacy iOS `Spinner` contract into shared `elements.rs` so downstream apps stop supplying phase/stroke data and the iOS host can promote spinner draws into native `UIActivityIndicatorViewStyleLarge` views.
- 2026-03-28: moved the legacy iOS `SlidingSwitch` long-press gate, inactivity timeout, and out-of-bounds cancellation contract into shared `SlidingSwitchState` so downstream apps stop rebuilding those gesture rules around the primitive.
- 2026-03-26: added and re-exported the shared `emitter` module so app crates can reuse deterministic CAEmitter-style burst sampling instead of keeping app-local particle math.
- 2026-03-13: centralized shared cubic-bezier easing and required-field shake helpers in `anim.rs` so app crates can drop duplicated motion math.
- 2026-03-13: re-exported the generic text-input primitives from the crate root so app crates can deduplicate their input state machines onto Oxide.
- 2026-03-28: re-exported the shared legacy multi-column picker controller and scroll-end commit types from the crate root so app crates can consume the old iOS picker contract directly.
- 2026-03-13: re-exported the generic popup/wheel-picker interaction primitives so app crates can share one drag/snap/dismiss controller instead of reimplementing them in scene code.
