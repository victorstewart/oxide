# renderer-api `lib.rs`

## Intention and purpose
- Define the renderer-neutral draw-list contract shared by UI core, Metal, WebGPU, and tests.
- Keep draw commands semantic enough for batching and retained replay without exposing backend-specific Metal or WebGPU state.

## Relation to the rest of the code
- `oxide-ui-core::DrawListBuilder` produces `DrawList` values.
- `oxide-text` bakes shaped glyphs into `GlyphRun` spans carried inside `DrawCmd::GlyphRun`.
- Renderer backends consume the command stream and backing vertex/index arrays.

## Entry points list
- `DrawList`
  Owns draw commands plus optional span-addressed vertex and index buffers.
- `DrawList::text_atlas_revision_compatible(atlas, revision) -> bool`
  Checks whether every cached glyph run was baked against the one supplied atlas revision.
- `DrawList::text_atlas_revisions_compatible(atlases) -> bool`
  Checks whether every cached glyph run has an explicit matching atlas handle and revision.
- `GlyphRun`
  Carries atlas handle, atlas revision, vertex span, index span, SDF mode, and color for a shaped text run.
- `DrawCmd`
  Enumerates renderer-neutral commands for layers, solids, images, glyphs, rounded rectangles, effects, camera backgrounds, custom embeds, spinners, and clips.
- `RenderEncoder`
  Backend-facing immediate encoder trait used by replay and test encoders.
- `RuntimeImageUploader`
  Narrow renderer-owned upload boundary for runtime A8 image resources such as app-generated text/icon atlases. Apps call through this trait instead of constructing backend-specific textures.

## Logic narrative
- Draw commands reference geometry by span so retained or translated replay can rebase buffer offsets without reconstructing high-level widgets.
- Text atlas revision is part of `GlyphRun` because atlas slot eviction can make old UVs point at different glyph pixels while the texture handle stays the same.
- Cached draw-list replay rejects stale or unknown text geometry while preserving normal replay for non-text commands.
- Runtime image uploads stay outside draw commands: app code publishes changed atlas bytes to the renderer, then emits normal `ImageHandle`/`GlyphRun` draw work for the frame.

## Preconditions and postconditions
- Span offsets and lengths must address the `DrawList` backing arrays.
- `GlyphRun::atlas_revision` must match the atlas revision at the end of glyph baking.

## Edge cases and failure modes
- Backends that ignore glyph atlas revisions still receive the same geometry, but retained caches should check compatibility before replaying cached glyph runs.
- A draw list with glyph runs for an atlas absent from the supplied revision set is incompatible. A draw list with no glyph runs remains compatible.

## Concurrency and memory behavior
- `DrawList` is caller-owned data with no synchronization.
- Revision checks scan command items without allocation.

## Performance notes
- Revision compatibility is a cheap linear scan over retained command metadata and avoids a broader forced redraw when all atlas resources are known unchanged.

## Feature flags and cfgs
- No feature-specific draw-list behavior.

## Testing and benchmarks
- `crates/renderer-api/tests/draw_list_tests.rs` covers draw-list structure, `DrawCmd` taxonomy freeze, and stale text-atlas revision detection.
- Retained replay integration is covered by `crates/ui-core/tests/draw_builder_tests.rs`.

## Changelog
- 2026-06-22: added a measurement-harness freeze for the `DrawCmd` variant set and declaration order before packed draw-stream work.
- 2026-06-06: added `RuntimeImageUploader` so apps can publish runtime A8 atlas resources through a renderer-neutral boundary.
- 2026-05-31: tightened retained text replay checks so every glyph atlas handle must have an explicit matching revision, including multi-atlas draw lists.
- 2026-05-31: added glyph atlas revision metadata and compatibility checks for retained text draw caches.
