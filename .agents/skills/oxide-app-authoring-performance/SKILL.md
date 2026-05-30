---
name: oxide-app-authoring-performance
description: "Use when building, reviewing, or optimizing applications, demos, examples, documentation, or sample UI code that uses Oxide as a library without modifying Oxide internals. Covers UI tree shape, state updates, layout invalidation, lists, text, images, animation, Scene3D usage, input latency, accessibility, and app-level performance discipline."
---

# Oxide App Authoring Performance Skill

## Prime directive

Use this skill for application code that consumes Oxide. Optimize the way the app describes UI, state, animation, input, text, images, and scenes so Oxide can keep its renderer hot paths predictable.

Do not treat this as a license to change Oxide internals. If the task requires editing `oxide/crates/ui-core`, `oxide/crates/renderer-api`, `oxide/crates/renderer-metal`, `oxide/crates/renderer-web`, `oxide/crates/text`, `oxide/crates/input`, platform hosts, benchmark runners, snapshot runners, or backend architecture, use `$oxide-internals-performance` before making those edits.

## Activation triggers

Use this skill when the user asks to:

- Build or optimize an app, demo, example, tutorial, or sample screen with Oxide.
- Improve perceived speed, frame time, hitches, scrolling, animation smoothness, input response, text rendering behavior, image-heavy screens, or Scene3D usage in an Oxide app.
- Design public-facing Oxide usage examples, docs, migration guides, or authoring patterns.
- Compare two app-level UI designs for performance.
- Explain why an Oxide app is slow without immediately changing renderer internals.

Do not use this skill as the primary guide when the task is to design renderer backends, Metal command encoding, shader code, renderer traits, perf harness internals, snapshot machinery, or host lifecycle plumbing. Those are internals tasks.

## First-response behavior

For any app-authoring task:

1. Inspect the current Oxide API, examples, docs, and tests before inventing names, APIs, or patterns.
2. Identify the workload class: static screen, form, list/grid/chat, text-heavy document, image gallery, animation, gesture surface, camera/media composition, Scene3D, or mixed UI/3D.
3. State the app-level performance hypothesis: invalidation, layout, text shaping/rasterization, atlas churn, image decode/upload, excessive draw variety, animation churn, input routing, overdraw, or Scene3D complexity.
4. Prefer changes that avoid work: smaller dirty regions, fewer state writes, stable identity, virtualized lists, cached text/images, simpler clipping/effects, fewer layout dependencies.
5. Do not lower visible quality unless the task explicitly asks for a quality/performance tradeoff.
6. Include the measurement plan: benchmark, trace, device run, screenshot/snapshot, or manual interaction evidence needed to prove the improvement.

## Mental model for app authors

An Oxide app influences this chain:

```text
user/input/event
  -> app state mutation
  -> UI description/tree update
  -> diff/invalidation
  -> layout
  -> text measurement/shaping/rasterization requests
  -> draw list
  -> renderer batching/pass planning
  -> GPU work
  -> present
```

App code cannot directly optimize every renderer step, but it can keep the renderer fed with stable, coherent work.

Good app code gives Oxide:

- Stable node identity.
- Predictable layout constraints.
- Coalesced state updates.
- Minimal dirty regions.
- Reused fonts, text styles, images, and materials.
- Virtualized collections.
- Limited visual state variety in dense views.
- Animation state scoped to the nodes that actually change.
- Gesture handling that preserves input samples and avoids platform-owned product behavior.
- Clear scene boundaries for UI, media, and Scene3D content.

Bad app code forces Oxide to do avoidable work:

- Rebuilding large UI subtrees on tiny state changes.
- Recreating text/image/material resources each frame.
- Using unstable keys in lists.
- Animating layout-affecting properties unnecessarily.
- Applying shadows, blurs, masks, clips, and translucent overlays across huge regions.
- Updating every row in a list because one row changed.
- Using nested dynamic measurement where fixed or bounded geometry would do.
- Coupling input state to rendering state in a way that drops, batches, or delays interaction samples.

## Do not invent Oxide APIs

Oxide is evolving. Before writing app code, inspect current source for public builders, widgets, element APIs, routing APIs, scene APIs, benchmark examples, and test scenes.

If a desired API does not exist:

- For app work, use existing public APIs and note the limitation.
- For library work, switch to `$oxide-internals-performance` and design the API with benchmark coverage.
- Do not write plausible but nonexistent Oxide method names into examples unless the task is explicitly to propose an API.

## App state discipline

State changes are the usual root of unnecessary UI work.

Prefer:

- State split by ownership and redraw scope.
- Stable IDs for collection items, tabs, panes, route entries, and reusable components.
- Coalescing multiple logical writes into one UI update when same-frame visibility is unchanged.
- Separating hot interactive state from cold data.
- Bounded, fixed-shape state for tight gesture and animation loops.
- Deterministic state transitions that are easy to replay in tests and perf cases.

Avoid:

- Global state writes that invalidate the whole screen.
- Deriving a full UI model from scratch on every pointer move when only one value changed.
- Generating new IDs, strings, images, or layout objects each frame.
- Storing presentation-only transient state in a way that forces broad model diffing.
- Logging, formatting, allocation-heavy debug paths, or file IO in visible interaction loops.

## UI tree and layout guidance

The best layout optimization is stable, bounded geometry.

Prefer:

- Shallow, predictable trees for dense UI.
- Fixed or bounded item sizes in scrolling collections when visual design permits.
- Explicit constraints for expensive text or image regions.
- Grouping static content separately from highly dynamic content.
- Dirty-region boundaries around independent panels, rows, overlays, and animated subtrees.
- Reuse of element shapes/styles so the renderer sees repeated work.

Avoid:

- Measuring long text repeatedly while scrolling.
- Animating properties that require full layout when transform/opacity would preserve layout.
- Deep wrapper stacks for simple visual effects.
- Huge offscreen or invisible subtrees that remain active.
- Dynamic parent sizing that forces child remeasurement across large siblings.

## Lists, grids, feeds, and chat

Large collections need identity, virtualization, and bounded per-item work.

Checklist:

- Use stable item keys.
- Keep row composition predictable.
- Virtualize offscreen content when the API supports it.
- Keep insertion/removal/reorder operations local.
- Cache text measurement or use bounded text layouts for repeated rows.
- Reuse image decode/upload results.
- Avoid per-row unique effects that split batches.
- Avoid full-list state writes for hover, selection, unread, progress, or animation changes.
- Test fast scroll, slow scroll, item insertion at head/tail, row expansion, and image loading under motion.

For chat-like screens:

- Separate message identity from transient delivery/read/progress state.
- Avoid remeasuring historical messages unless width/font/content changed.
- Batch new-message arrival into frame-aligned updates where latency allows.
- Preserve scroll anchoring explicitly.
- Measure first interactive, scroll hitch ratio, and keyboard/input latency.

## Text-heavy UI

Text cost comes from shaping, measurement, rasterization, atlas upload, fallback fonts, line breaking, and invalidation.

Prefer:

- Reusing font families, sizes, weights, colors, and text styles.
- Separating static labels from dynamic text.
- Bounded text regions.
- Caching repeated strings and measurements where the app owns repeat structure.
- Avoiding per-frame string formatting in animated UI.
- Deferring expensive rich text composition until visible.
- Testing CJK, emoji, combining marks, bidirectional text, IME composition, and font fallback where the UI accepts user text.

Avoid:

- Creating a unique style per row or glyph run without visual need.
- Rebuilding entire paragraphs on cursor blink or small selection changes.
- Measuring text in a loop while pointer input is active unless the UI truly changes.
- Assuming ASCII-only or Latin-only behavior in examples.

## Images, icons, and media

Image-heavy apps should keep decode, resize, upload, and filtering out of critical interaction loops.

Prefer:

- Decode/resize before the frame that needs the image when possible.
- Reuse loaded image handles and icon atlases.
- Use consistent icon sizes and styles in dense UI.
- Bound large images to the visible display size.
- Separate placeholders, progressive loading, and final image replacement in a way that dirties only affected regions.
- Test cold cache, warm cache, and hot cache separately.

Avoid:

- Uploading the same image repeatedly.
- Animating large image masks or blurs at full resolution unless measured.
- Keeping hidden galleries active.
- Forcing layout shifts when image metadata arrives after initial layout.

## Animation guidance

Animations are performance-sensitive because they repeat every frame.

Prefer:

- Animating transform, opacity, color, or shader parameters when layout need not change.
- Animating the smallest subtree that visually changes.
- Keeping animation clocks deterministic and frame-rate independent.
- Coalescing multiple animation state updates per frame.
- Respecting reduce-motion or accessibility preferences when the app exposes them.
- Measuring hitch ratio and p95/p99 frame time, not only mean FPS.

Avoid:

- Allocating, formatting strings, decoding images, reshaping text, or rebuilding large UI trees inside animation ticks.
- Animating blur, shadow, mask, clip, or full-screen alpha overlays without a measurement-backed reason.
- Starting many independent timers where one scene clock would do.
- Treating a simulator run as proof of device performance.

## Input and gestures

Oxide should own product interaction semantics. App code should consume Oxide input/gesture abstractions and avoid moving behavior into platform-specific glue.

Prefer:

- Stable gesture state owned by Rust/Oxide app code.
- Small state updates on pointer/touch movement.
- Preserving raw input timing and identity when building custom interactions.
- Testing real host input paths for touch, pointer, scroll wheel/trackpad, keyboard, IME, and accessibility actions.
- Measuring event-to-visible-response for latency-sensitive surfaces.

Avoid:

- Platform-only behavior that cannot be represented in Oxide app state.
- Per-sample layout of large regions during drag/pinch/scroll.
- Gesture implementations that drop samples, lose touch identity, or reset velocity/anchor unexpectedly.
- Debug overlays or logging that change input timing during measurement.

## Scene3D and mixed UI/3D

Use Scene3D deliberately. It is powerful, but it can consume frame budget that ordinary UI expects.

Prefer:

- Clear ownership between UI overlays and 3D scene state.
- Bounded mesh/material/light counts for interactive UI scenes.
- Reused materials, textures, and geometry.
- Lower-resolution or cached intermediates only when visually acceptable and measured.
- Fixed camera/update cadence unless the scene must react every frame.
- Device measurement for bloom, MSAA, post effects, shadows, transparency, and large textures.

Avoid:

- Treating a 3D scene as a dumping ground for ordinary 2D UI.
- Uploading mesh/material data every frame when only transforms changed.
- Full-screen post effects under translucent UI without a strong reason.
- Hidden 3D scenes that continue rendering.

## Accessibility and correctness

Performance optimizations must preserve correctness.

Keep:

- Accessibility labels, roles, focus order, actions, and hit regions aligned with visual UI.
- Keyboard navigation and IME composition behavior for text entry.
- Dynamic text/font scaling support where the app promises it.
- Reduced motion and high-contrast behavior where supported.
- Snapshot or manual evidence for visual changes.

Never count a speedup as valid if it drops text correctness, hit testing, accessibility semantics, image quality, animation semantics, or input behavior.

## App-level benchmarking policy

Every significant app-facing example, demo, screen, or workflow should have a measurement story.

Prefer measuring:

- First frame.
- First interactive.
- p50/p95/p99 frame time.
- Hitch ratio and missed frames during scroll/animation.
- Event-to-visible-response for gestures and input.
- Text measurement/shaping work for text-heavy screens.
- Image decode/upload/cache state for image-heavy screens.
- Memory and peak resource use for long-lived screens.

When adding or changing public authoring patterns inside the Oxide repo, add or update an `authoring` perf case if the code path becomes something app authors will copy.

## Documentation and examples

When writing examples or docs for app authors:

- Show stable identity in collections.
- Show bounded layout where appropriate.
- Show how to isolate state updates.
- Show text/image loading patterns that do not block interaction loops.
- Show measurement commands or benchmark IDs when available.
- State whether code is illustrative, benchmarked, or production-recommended.
- Avoid examples that create per-frame resources, unstable IDs, global invalidation, or platform-owned product behavior.

## Review checklist

Before considering app authoring work complete, answer:

- Which UI regions become dirty for the changed interaction?
- Does the app rebuild more tree than necessary?
- Are list keys stable?
- Are text and image resources reused?
- Does any animation force layout unnecessarily?
- Are expensive effects scoped to small regions?
- Is input latency preserved?
- Are accessibility semantics still correct?
- Is there a benchmark, snapshot, device run, or manual evidence for the user-visible path?
- Did the work avoid changing Oxide internals? If not, was `$oxide-internals-performance` used?

## Failure modes to flag

Flag these immediately in review:

- Example code that cannot compile against current Oxide APIs.
- App code that silently relies on UIKit, SwiftUI, DOM, or platform gesture recognizers for product behavior.
- Per-frame allocation/resource creation in visible loops.
- Full-screen invalidation from local state changes.
- Full-list or full-document remeasurement on tiny edits.
- Benchmark claims based only on simulator, debug builds, averages, or subjective smoothness.
- Faster UI that renders fewer pixels, fewer effects, lower precision, or different content.
