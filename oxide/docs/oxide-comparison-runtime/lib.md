# `oxide-comparison-runtime`

## Intention and purpose

This non-publishable crate owns the Metal renderer, canonical scenario loading, benchmark trace application, checkpoint evidence, and snapshot readback used only by the Apple framework-comparison applications. It keeps comparison-only behavior out of the production iOS and macOS host crates.

Attribution passes record comparison-only continuous-clock boundaries around render preparation, Metal encoding, and command submission. The FFI exposes these timestamps with the submitted-frame diagnostic; non-diagnostic passes and production Oxide contain no added instrumentation.

## Entry points

- `oxide_comparison_init` and `oxide_comparison_shutdown` own the isolated renderer lifetime; `oxide_comparison_teardown_scenario` releases scenario-owned assets and reusable storage without rebuilding the process's Metal device and pipelines.
- `oxide_comparison_prepare_scenario`, `oxide_comparison_reset_scenario`, and `oxide_comparison_apply_trace_event` drive the manifest-selected canonical scene.
- `oxide_comparison_prepare_release_candidate` drives only the five separately validated screenshot-blocked candidates for correctness capture; it never participates in measured campaign admission.
- `oxide_comparison_prepare_frame`, `oxide_comparison_submit_prepared_frame`, and `oxide_comparison_cancel_prepared_frame` expose late drawable acquisition without moving scene semantics into Swift/AppKit/UIKit.
- `oxide_comparison_checkpoint_json`, `oxide_comparison_role_count`, and `oxide_comparison_role` emit semantic parity evidence.
- `oxide_comparison_take_snapshot`, `oxide_comparison_snapshot_status`, and `oxide_comparison_quiesce` support untimed exact-pixel correctness capture.
- `oxide_comparison_macos_pointer_event`, `oxide_comparison_macos_scroll`, and `oxide_comparison_macos_text` route real AppKit responder events into Rust scene state; `oxide_comparison_interaction_generation` exposes the monotonic change generation consumed only after successful presentation.

## Call graph

- Apple comparison app adapter
  - comparison runtime FFI
    - `oxide-benchmark-spec` validation and immutable fixture loading
    - `oxide-test-scenes` canonical state and draw-list construction
    - `oxide-renderer-metal` encoding, presentation, and readback

The production `oxide-host-ios` and `oxide-host-macos` crates are not dependencies of this crate and do not own comparison campaign state.

## Invariants and failure behavior

- Normal preparation accepts only the six validated Apple PR scenario identities. Candidate preparation has a separate named FFI, accepts only the five frozen release identities, validates canonical candidate JSON and every transitive hash-bound input, and cannot fall back between modes.
- Exactly one Metal renderer exists per process. Scenario teardown releases uploaded comparison images and glyph pages, rebuilds only the scene router, and discards trace/draw/damage/snapshot capacities; full `shutdown` drops the router before its renderer and replaces the runtime wholesale.
- A frame must be prepared exactly once before submission. A missing renderer, invalid ordering, drawable-bridge failure, or Metal submission failure returns a nonzero code.
- Correctness snapshots are opaque sRGB PNGs produced outside measured intervals through the renderer's existing allocating readback API. Headline samples remain drawable-backed and on screen.
- Pointer-down only establishes drag ownership. A click mutates on pointer-up only when no drag occurred, matching AppKit button activation and preventing a drag from also activating its origin.

## Concurrency and memory behavior

One poison-recovering process mutex protects runtime state because the Swift FFI entry points can arrive from native application callbacks. The renderer remains boxed so the image uploader's raw pointer stays stable. Frame vectors, draw lists, and damage storage are retained and reused after warmup within a scenario lifetime, then released by `shutdown` for fixed-footprint recovery.

## Performance notes

The measured frame path performs no manifest I/O, font construction, image decoding, or pipeline creation after scenario preparation. Scenario preparation preserves every validated font-manifest variation coordinate and constructs matching immutable Oxide font instances before measurement. Drawable acquisition remains outside Rust frame construction and occurs immediately before submission in the platform adapter.

## Testing

`tests/runtime_tests.rs` initializes a real macOS Metal renderer, validates and prepares the canonical feed scenario, submits an offscreen correctness frame, reads semantic checkpoint evidence, and resets the same runtime in place. It also prepares and resets all five blocked candidates through the capture-only FFI while proving normal admission rejects them.

## Changelog

- 2026-07-26: forwarded the validated comparison font-pack variation axes into immutable Oxide font instances.
- 2026-07-21: added explicit, fail-closed release-candidate screenshot capture preparation while preserving strict runnable-scenario admission.
- 2026-07-19: split scenario teardown from full Metal shutdown after a real macOS correctness pack showed that rebuilding device/pipeline state contaminated fixed-baseline recovery with process-level driver caches.
- 2026-07-21: added comparison-only AppKit responder ingestion and monotonic state-change generations without modifying production hosts.
- 2026-07-19: introduced the comparison-only Apple Metal runtime so benchmark scene/FFI code is separate from production Oxide host crates.
