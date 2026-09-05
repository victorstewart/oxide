# oxide-test-scenes::comparative

## Intention and purpose

`comparative` owns the real Oxide implementations of the six canonical PR comparison scenarios, the distinct nightly idle/endurance scenarios, and the five frozen release candidates. It converts validated framework-neutral fixtures and traces into normal Oxide text, image, primitive, clipping, layer, backdrop, and draw-list work without introducing a benchmark renderer.

## Relation to the rest of the code

- `oxide-benchmark-spec` supplies typed fixtures, scenarios, roles, and trace events.
- `Router` prepares one `ComparisonScene`, installs shared fonts/thumbnail atlas, all inline-text raster-variant textures and their contract, plus standalone uploaded image handles, applies trace events, and delegates draw/update/input.
- `oxide-ui-core` shapes visible text and emits draw-list commands.
- The host or renderer image-store path decodes and uploads exact PNG bytes before passing hash-matched handles to `set_image_resources`; this module never decodes or uploads in draw.

Call flow:

- validated runnable manifest + fixture bytes -> `ComparisonScene::prepare`
- validated release-candidate capture identity + fixture bytes -> `ComparisonScene::prepare_id`
- renderer-owned resources -> `set_resources` / `set_image_resources`
- canonical trace -> `apply`
- elapsed host tick -> `update`
- scene state -> bounded `role_counts`, correctness-only `checkpoint_json`, and `draw`

## Entry points list

- `ComparisonScene::prepare(scenario: &ScenarioSpec, fixture: &[u8]) -> Result<Self, String>` deserializes the exact typed fixture and constructs reset state.
- `ComparisonScene::prepare_id(scenario_id: &str, fixture: &[u8]) -> Result<Self, String>` constructs the same comparison-only scene after the caller has independently validated a blocked release-candidate capture contract.
- `ComparisonScene::set_resources(thumbnail_atlas: ImageHandle, font_ids: [usize; 3])` binds shared immutable assets.
- `ComparisonScene::set_inline_text_resources(images: Vec<ImageHandle>, atlas: InlineTextAtlas)` binds the retained raster-variant textures and frozen grapheme metrics.
- `ComparisonScene::set_image_resources(source: ImageHandle, source_sha256: &str, thumbnail: ImageHandle, thumbnail_sha256: &str) -> Result<(), String>` binds normal renderer-uploaded image textures only when their identities match the fixture.
- `ComparisonScene::update(dt_ms: u32)` advances elapsed-time navigation transitions.
- `ComparisonScene::wants_next_frame() -> bool` reports active autonomous transition work.
- `ComparisonScene::apply(event: &TraceEvent) -> Result<(), String>` applies one canonical state transition.
- `ComparisonScene::input_pointer(...)` handles direct feed/image pointer fallback input.
- `ComparisonScene::host_click(x: f32, y: f32) -> bool`, `host_pointer_delta(dx: f32, dy: f32) -> bool`, `host_wheel(delta_y_millionths: i32) -> bool`, and `host_text(value: &str) -> bool` apply comparison-host input and report whether visible Rust state actually changed.
- `ComparisonScene::role_counts() -> Vec<RoleCount>` reports currently visible semantic work.
- `ComparisonScene::checkpoint_json(checkpoint_id: &str) -> Result<(Vec<u8>, Vec<u8>), String>` serializes the live model and semantic accessibility tree only when the correctness harness requests evidence.
- `ComparisonScene::draw(...)` emits the active scene through standard Oxide draw/text APIs.

## Logic narrative

Startup retains all 24 cards and exact 24 KiB payload but emits only the six initially visible cards, their pinned atlas tiles, header, navigation, and control. Lifecycle events update state without changing visible work.

Dashboard retains its fixed labels/cards/effects. Its 24 canonical hard-edged controls are appended after the non-overlapping card bodies as one indexed `Solid` command, matching AppKit `CGRect.fill()` semantics without a zero-radius rounded-distance shader or an additional renderer pipeline. The fixed geometry is emitted directly into the reusable draw-list buffers, so warm frames do not allocate a temporary rectangle collection. Feed owns all 2,000 variable rows plus an offset index and uses binary search plus a nine-row cap; every visible thumbnail emits a benchmark-owned rounded `ImageMesh`, keeping comparison masking outside the production image ABI. Each feed single-line label uses the comparison-only pinned Noto Sans ascent and line-height ratios to reproduce native vertical centering, rounding only the final baseline to the backing pixel. Feed and chat text is clipped to its declared label rectangle. A bounded scanner splits only strings containing a pinned grapheme: ordinary spans use the comparison scene's pinned-font shaper measurement, while matching graphemes select the closest declared physical-em raster, use the same centered baseline passed to glyph baking, emit the raster's grid cell with the frozen advance/baseline/size, and snap only the image origin to the backing-pixel grid. Logical advance remains unchanged. At canonical 3x sizes the 45- and 39-pixel variants sample 1:1. The fixture string remains unchanged for semantic evidence. Chat similarly owns 5,000 mixed-direction messages but emits at most ten visible messages/avatars. Each visible bubble emits the shared zero-blur, 2-point-offset shadow before its surface, preserving the frozen style rather than silently dropping an effect. Its 50-row prepend preserves the visible anchor; 10 Hz appends extend offsets in constant time; composer storage is preallocated for the 100-character type and 10 KiB paste phases; only the visible 80-character tail is shaped; select/replace validates UTF-8 boundaries. Font choice scans each visible string for CJK or Arabic and uses the pinned corresponding font.

Endurance uses its own fixture, scene identity, state, roles, and checkpoints while reusing the dashboard component implementation. Each close destroys the dashboard scene data and each reopen reconstructs all 300 visible nodes. Tab events rebuild all 176 labels, animation events update a deterministic 30-label accent cycle, and the final 600th frame restores the exact base dashboard draw list.

Navigation advances modal transitions from elapsed milliseconds. Image tracks the ordered bytes-ready -> decoded -> uploaded -> first-visible states separately. Before first-visible it draws the exact uploaded thumbnail; afterward it draws the exact uploaded 4096x3072 source. Raw pointer identities implement one-finger pan, cancel drag when two touches activate, derive pinch from two-touch distance ratio with the fixture's 2x cap, and restart drag from a remaining touch. Draw only reads transform/resource state.

The macOS comparison host path resolves clicks against the same retained geometry used by draw, applies feed/image drag deltas, normalizes grid wheel deltas, and appends committed chat text. Each operation returns `true` only when its intended scene state changes, allowing the host to publish a monotonic presentation generation without counting pointer-down setup, repeated clicks, empty text, or clamped movement.

The release adapters preserve the candidate cardinalities and transitions: grid retains 10,000 tile identities while emitting the frozen 18-tile phone window and detail route; effects emits 100 clipped retained layers with 32 shadows and eight backdrops; mutation retains a 10,000-bit changed-node map driven by the frozen affine permutation; multilingual text retains all 1,000 labels and emits the 40-label checkpoint window; resize/theme reuses the complete dashboard work while tracking all ten orientation, viewport, and theme triplets. These implementations remain comparison-only and do not alter production renderer behavior.

Checkpoint models record runtime-owned lifecycle visibility, mutation counts, scroll position, message/focus state, navigation state, and image resource/transform state. The paired accessibility tree records canonical role, name, value, state, order, focus, actions, logical frame, count, and visibility. Scroll, pan, and scale use integer millionths so cross-platform evidence does not depend on floating-point JSON formatting.

## Preconditions and postconditions

The caller validates scenario artifacts and typed fixture semantics before prepare. The identity-only entry point does not validate files and is reserved for the comparison runtime after `load_release_candidate_for_capture` succeeds; it is not a fallback for runnable manifests. Shared fonts and both atlas resources must be installed before authoritative drawing. Image handles must be nonzero and accompanied by exact fixture SHA-256 strings. Successful trace application preserves bounded visible roles and deterministic state; preparing the fixture again restores the initial scene.

## Edge cases and failure modes

Unknown scenario IDs, malformed fixture JSON, unsupported operations/targets, missing trace values, invalid UTF-8 replacement ranges, out-of-order image resource stages, zero or hash-mismatched image handles, and missing pointer fields fail explicitly. Repeated prepend or append IDs are idempotent. Scroll and image transforms clamp to fixture bounds.

## Concurrency and memory behavior

Scenes are single-threaded router-owned state. Startup/feed/chat fixtures allocate once at prepare. Chat preallocates its append, offset, and composer capacities; draw allocates no scene containers and visits at most ten rows. Dashboard hard rectangles extend the builder's retained vertex/index capacities directly and require no intermediate `Vec`; after warmup those buffers are reused. The inline scanner borrows the frozen entry list and reuses the text layout cache; all declared variants are retained image handles rather than per-frame crops, decodes, or resamples. Image touch storage reserves two entries and has no per-draw decode, upload, or heap growth. Checkpoint JSON allocates only in the explicit correctness path and never during update, draw, or presentation. Renderer resources remain handle-owned outside this module.

## Performance notes

Feed/chat first-visible lookup is logarithmic and draw is constant-bounded. Centered label placement uses frozen comparison-font metrics and performs only scalar arithmetic during draw. Chat append is `O(1)`; prepend and selection-driven height changes rebuild the offset index in `O(n)` only during their named mutation phases. Large paste content is retained as real editor state while shaping remains bounded to the visible tail. Image resource creation stays outside draw and normal frames emit one image command.

## Feature flags and cfgs

No feature flag or target cfg changes the comparative scenes.

## Testing and benchmarks

`tests/comparative_scenario_tests.rs` validates all six PR manifests plus all five release fixtures, exact initial roles/resources, complete release-trace replay, all 18 frozen release state/accessibility checkpoints, bounded visible work, ordered image stages, elapsed transforms, and reset behavior. Cross-framework screenshot admission remains outside this crate and uses the frozen calibrated full-frame/voxel gate after canonical PNG materialization.

## Examples

Prepare a validated scenario, bind shared resources, bind standalone image resources for `image.decode-zoom`, apply each phase trace, and call `Router::draw` at 390x844.

## Changelog

- 2026-07-26: corrected comparison inline-image baseline placement so pinned graphemes remain wholly inside the same clipped line as adjacent glyph runs.
- 2026-07-21: added all five frozen release-candidate Oxide adapters with complete trace replay and exact semantic-checkpoint coverage.
- 2026-07-21: added explicit identity-only preparation for already-validated release-candidate screenshot capture without manufacturing a runnable manifest.
- 2026-07-21: added comparison-only macOS host input transitions with explicit state-change results for trustworthy responder receipts.
- 2026-07-21: added `endurance.churn` with exact 100/500/600 event replay, real dashboard teardown/recreation, and final draw-list recovery.
- 2026-07-21: Added `idle.steady` as a distinct scenario identity that reuses the complete dashboard scene, rejects all logical events, and remains quiescent after preparation.
- 2026-07-19: encoded the 24 dashboard `CGRect` controls as one benchmark-owned indexed solid batch instead of zero-radius rounded rectangles.
- 2026-07-18: restored the shared zero-blur shadow on every visible chat message bubble.

- 2026-07-18: aligned feed single-line text and inline image runs to the pinned font's device-rounded native vertical-centering baseline.
- 2026-07-18: added retained multi-density pinned inline-text image runs, physical-pixel origin snapping, and hard label clipping for exact static parity work.
- 2026-07-18: emitted canonical rounded thumbnails through benchmark-owned image meshes without changing the production image command.
- 2026-07-18: added startup, virtualized live chat, and exact image decode/zoom comparison scenes.
- 2026-07-18: added dashboard, feed, and navigation vertical-slice scenes.
