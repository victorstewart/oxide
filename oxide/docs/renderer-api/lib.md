# renderer-api `lib.rs`

## Intention and purpose
- Define the renderer-neutral draw-list contract shared by UI core, Metal, WebGPU, and tests.
- Keep draw commands semantic enough for batching and retained replay without exposing backend-specific Metal or WebGPU state.

## Relation to the rest of the code
- `oxide-ui-core::DrawListBuilder` produces `DrawList` values.
- `oxide-text` bakes shaped glyphs into `GlyphRun` spans carried inside `DrawCmd::GlyphRun`.
- Renderer backends consume the command stream and backing vertex/index arrays.

## Entry points list
- `Color::pack_rgba8(self) -> u32`
  Clamps finite channels and packs `AABBGGRR`, with red in the least-significant byte.
- `Vertex::rgba`
  On solid draws, zero inherits the command color and nonzero is the final interpolated vertex color.
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
  `RenderContext` borrows the active encoder for one draw callback, so backend
  encoders retain ownership without type erasure or a `'static` constraint.
- `ImageSampling`
  Declares immutable `Linear` or `Nearest` filtering for a runtime image resource.
- `RuntimeImageUploader`
  Narrow renderer-owned create/append/update/release boundary for runtime A8 resources and optional sRGB RGBA8 resources. Apps call through this trait instead of constructing backend-specific textures.
- `RenderPropertySlotId::dynamic` and `RenderDynamicClip`
  Carry generation-checked transform/opacity identities and transform-linked retained clip metadata without exposing a backend ring.
- `RenderSnapshot`
  Validates one live generation per dense property index and keeps immutable chunk geometry separate from frame property values.
- `RenderSpatialBounds`, `RenderCommandSpatial`, `RenderPaintSpan`, and `RenderSpatialQueryStats`
  Carry conservative bounds, resolved clip state, matched scope endpoints, ordered paint spans, and explicit query work without exposing a backend index implementation.
- `EffectGraphEvent`, `EffectGraphPlan`, and related graph records
  Describe renderer-neutral snapshot, extraction, filter, downsample, blur, and composite dependencies with transient-resource lifetimes and alias slots.

## Logic narrative
- Packed solid color remains part of the existing `Vertex` ABI: backends resolve zero to the draw uniform before interpolation and pass every nonzero value through as final color.
- Draw commands reference geometry by span so retained or translated replay can rebase buffer offsets without reconstructing high-level widgets.
- Text atlas revision is part of `GlyphRun` because atlas slot eviction can make old UVs point at different glyph pixels while the texture handle stays the same.
- Cached draw-list replay rejects stale or unknown text geometry while preserving normal replay for non-text commands.
- Runtime image uploads stay outside draw commands: app code publishes changed atlas bytes to the renderer, then emits normal `ImageHandle`/`GlyphRun` draw work for the frame.
- `append_a8` distinguishes never-before-sampled atlas texels from destructive updates, so dependency-aware prepared caches can preserve existing users. Its compatibility default delegates to `update_a8`; `release_a8` defaults to a no-op for legacy uploaders.
- `try_create_rgba8_sampled` keeps linear filtering as the compatibility default. A backend that does not implement nearest filtering returns `None` instead of silently producing different pixels; a successful backend stores the selected mode with the image handle.
- The default sampled-upload method forwards RGBA bytes and `row_bytes`
  unchanged to the legacy linear method.
- `release_rgba8` gives successful runtime uploads an explicit owner-driven end
  of life. Its default is a no-op so A8-only and legacy uploaders remain source
  compatible; resource-owning backends override it.
- Explicit extraction events let backend-owned effects render one reusable source, derive ordered filter layers from contained subregions, and schedule each composite without making backend textures part of the shared API.

## Preconditions and postconditions
- Span offsets and lengths must address the `DrawList` backing arrays.
- `GlyphRun::atlas_revision` must match the atlas revision at the end of glyph baking.
- RGBA upload dimensions must be non-zero. `row_bytes == 0` requests tightly
  packed rows; otherwise the stride must cover one RGBA row, and the byte slice
  must cover every requested source row.
- A successful optional RGBA upload returns a non-zero handle. Callers release
  that handle when it is no longer used; invalid backend sentinels are reported
  as `None`, never `Some(ImageHandle(0))`.
- An A8 append may cover only texels that no previously issued draw can reference. Replacing existing texels requires `update_a8` and its normal dependency invalidation.

## Edge cases and failure modes
- `Color::pack_rgba8` clamps negative and above-one channels; NaN and infinities pack as zero.
- Backends that ignore glyph atlas revisions still receive the same geometry, but retained caches should check compatibility before replaying cached glyph runs.
- A draw list with glyph runs for an atlas absent from the supplied revision set is incompatible. A draw list with no glyph runs remains compatible.
- Unsupported optional RGBA uploads return `None`. In particular, the default sampled method refuses `Nearest` unless the backend explicitly implements it.
- Releasing a handle through an uploader without the corresponding owned runtime resource is a no-op.

## Concurrency and memory behavior
- `DrawList` is caller-owned data with no synchronization.
- Revision checks scan command items without allocation.
- Packed color conversion is a pure, allocation-free operation on a small `Copy` value.
- `ImageSampling` is a small `Copy` value retained once per resource; it does not add per-frame ownership or synchronization.

## Performance notes
- Revision compatibility is a cheap linear scan over retained command metadata and avoids a broader forced redraw when all atlas resources are known unchanged.
- Dense dynamic IDs sort by index and generation, making stale-generation conflict detection a linear adjacent scan; per-frame property values remain small metadata rather than geometry copies.
- Retained glyph/mesh bounds are computed once from immutable vertex spans. Ordered chunk and instance queries use those stored bounds without rescanning source geometry.
- Packed color conversion adds no draw, upload, or renderer object; it is internal renderer data preparation rather than a new authoring or user-journey path.
- Effect graph plans report logical and physical transient bytes. A terminal single filter can alias its vertical output back onto a dead extraction source, while multiple filters retain the source and reuse two serial filter slots.
- Runtime-image filtering is selected from immutable resource metadata, so backends can prebuild sampler state and preserve same-mode batches without per-frame sampler creation.
- Append-only A8 publication lets prepared text users survive atlas growth without weakening destructive-update invalidation or retaining retired pages.

## Feature flags and cfgs
- No feature-specific draw-list behavior.

## Testing and benchmarks
- `crates/renderer-api/tests/draw_list_tests.rs` covers draw-list structure, `DrawCmd` taxonomy freeze, and stale text-atlas revision detection.
- The same test file freezes packed byte order, clamping, non-finite handling, RGBA byte/stride forwarding, the linear compatibility default, explicit refusal of unsupported nearest sampling, and the source-compatible release hook.
- Retained replay integration is covered by `crates/ui-core/tests/draw_builder_tests.rs`.
- `crates/renderer-api/tests/render_chunk_tests.rs` covers retained chunk identity, sequence sharing, dynamic property generations, clip references, conservative spatial metadata, ordered damage queries, and checked flat fallback.

## Examples

`Color::rgba(1.0, 0.0, 0.0, 1.0).pack_rgba8()` produces `0xFF00_00FF`.

## Changelog
- 2026-08-07: added explicit append-only publication and release hooks for runtime A8 resources while preserving update/no-op compatibility defaults.
- 2026-08-06: added explicit owner-driven RGBA release with a source-compatible no-op default for unsupported uploaders.
- 2026-08-06: added immutable runtime-image sampling with no silent nearest-to-linear fallback.
- 2026-08-06: added an explicit optional sRGB RGBA8 upload boundary.
- 2026-07-15: extended the common effect graph with explicit extraction/filter events, fused extraction-downsample and upsample-composite pass reasons, contained-region validation, and terminal-output aliasing.
- 2026-07-13: exported C27 conservative spatial bounds, command/span metadata, resolved instances, and query statistics.
- 2026-07-13: added C26 generation-checked dynamic property slots and transform-linked retained clip metadata.
- 2026-07-12: added packed `AABBGGRR` conversion and documented solid vertex-color inheritance.
- 2026-06-22: added a measurement-harness freeze for the `DrawCmd` variant set and declaration order before packed draw-stream work.
- 2026-06-06: added `RuntimeImageUploader` so apps can publish runtime A8 atlas resources through a renderer-neutral boundary.
- 2026-05-31: tightened retained text replay checks so every glyph atlas handle must have an explicit matching revision, including multi-atlas draw lists.
- 2026-05-31: added glyph atlas revision metadata and compatibility checks for retained text draw caches.
