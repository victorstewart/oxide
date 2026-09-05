# oxide-feed-v1-app `src/contract.rs`

## Intention and purpose

This module is the Rust reconstruction of the frozen Swift feed-v1 fixture. It gives the Oxide app and its tests one deterministic source for workload identity, row content, geometry, image bytes, typography metrics, endpoints, record names, and controller protocol constants. Independent reconstruction is intentional: agreement is meaningful only when Rust does not consume UIKit-produced fixture output.

## Relation to the rest of the code

- `src/lib.rs` consumes geometry, colors, row recipes, font metrics, environment keys, and endpoint offsets while composing the Oxide scene.
- [`observation.rs`](observation.md) consumes fixture identity, schemas, component names, result paths, and notification prefixes while serializing evidence.
- [`contract_tests.rs`](tests/contract_tests.md) drives the production canonical streamer and verifies its exact 717,745-byte Swift identity together with every row, prefix value, checker, and font metric.
- The reducer independently repeats the same frozen recipe and rejects records whose identity or geometry differs.

Call graph:

- `FeedFixture::new` -> `RowRecipe::at` -> `mix32` -> height prefix
- runner Rust host preflight -> `canonical_fixture_identity` -> every canonical field/row/checker/prefix -> `CanonicalHasher` -> byte count + SHA-256
- visible rendering -> `FeedFixture::visible_row_range` -> `RowRecipe` string/height/checker accessors
- ready admission/reducer evidence -> `FeedFixture::component_rect_physical_pixels`
- checker upload -> `checker_rgba_bytes` -> frozen palette/tile/phase recipe

## Entry points list

- Identity constants `oxide_feed_v1_app::contract::{SCHEMA, REVISION, LOCALE_IDENTIFIER, RGBA_COLOR_SPACE_NAME, CHECKER_PIXEL_RULE, IMAGE_RASTER_RULE, INTERFACE_STYLE_RULE, EXPECTED_CANONICAL_SHA256, EXPECTED_CANONICAL_BYTE_COUNT}` freeze canonical fixture semantics and digest expectations.
- Canvas constants `{ROW_COUNT, HOST_WIDTH_POINTS, HOST_HEIGHT_POINTS, SURFACE_WIDTH_POINTS, SURFACE_HEIGHT_POINTS, SURFACE_SCALE, SURFACE_ORIGIN_X_POINTS, SURFACE_ORIGIN_Y_POINTS, SURFACE_PLACEMENT_RULE, SAFE_AREA_TOP_POINTS, SAFE_AREA_LEFT_POINTS, SAFE_AREA_BOTTOM_POINTS, SAFE_AREA_RIGHT_POINTS}` freeze the scene and viewport.
- Controller/result constants `{TREATMENT_ENVIRONMENT_KEY, START_STATE_ENVIRONMENT_KEY, COMPLETION_NONCE_ENVIRONMENT_KEY, IDIOMATIC_UIKIT_VARIANT_VALUE, OPTIMIZED_UIKIT_VARIANT_VALUE, OXIDE_VARIANT_VALUE, RESULT_DIRECTORY_NAME, RESULT_FILE_PREFIX, RESULT_FILE_SUFFIX, READY_NOTIFICATION_PREFIX, COMPLETION_NOTIFICATION_PREFIX, FAILURE_NOTIFICATION_PREFIX, RUN_RECORD_SCHEMA, FAILURE_RECORD_SCHEMA, RUN_RECORD_SCHEMA_REVISION}` freeze cross-process names and record versions.
- Refresh constants `{DISPLAY_LINK_MINIMUM_FRAMES_PER_SECOND, DISPLAY_LINK_MAXIMUM_FRAMES_PER_SECOND, DISPLAY_LINK_PREFERRED_FRAMES_PER_SECOND}` freeze the native 120 Hz configuration.
- Row geometry constants `{ROW_LEADING_POINTS, ROW_TRAILING_POINTS, ROW_TOP_POINTS, IMAGE_SIDE_POINTS, IMAGE_TEXT_GAP_POINTS, IMAGE_CORNER_RADIUS_POINTS, SHADOW_OFFSET_X_POINTS, SHADOW_OFFSET_Y_POINTS, SHADOW_BLUR_RADIUS_POINTS, SHADOW_LAYER_OPACITY_BYTE, TITLE_HEIGHT_POINTS, CAPTION_TOP_POINTS, CAPTION_LINE_HEIGHT_POINTS, CAPTION_PARAGRAPH_RULE, METADATA_GAP_POINTS, METADATA_HEIGHT_POINTS, SEPARATOR_PHYSICAL_PIXELS}` define exact component boxes.
- Typography constants `{TITLE_FONT_POINTS, CAPTION_FONT_POINTS, METADATA_FONT_POINTS, ASAP_ASCENDER_UNITS, ASAP_DESCENDER_UNITS, ASAP_UNITS_PER_EM}` freeze UIKit-compatible label baselines.
- Workload constants `{CHECKER_SIDE_PIXELS, CHECKER_VARIANT_COUNT, CHECKER_RGBA_BYTE_COUNT, CONTENT_EXTENT_POINTS, MAXIMUM_CONTENT_OFFSET_POINTS, MIX_ALGORITHM, CHECKER_ALGORITHM, COMPONENT_KINDS}` freeze image cardinality, extent, deterministic algorithms, and component order.
- Font constants `{REGULAR_FONT_REPOSITORY_PATH, REGULAR_FONT_BUNDLE_NAME, REGULAR_FONT_POSTSCRIPT_NAME, REGULAR_FONT_SHA256, REGULAR_FONT_BYTE_COUNT, BOLD_FONT_REPOSITORY_PATH, BOLD_FONT_BUNDLE_NAME, BOLD_FONT_POSTSCRIPT_NAME, BOLD_FONT_SHA256, BOLD_FONT_BYTE_COUNT}` identify the exact embedded Asap inputs.
- Color constants `{BACKGROUND, TITLE_COLOR, CAPTION_COLOR, METADATA_COLOR, SEPARATOR_COLOR, SHADOW_COLOR}` expose frozen sRGB RGBA bytes.
- `oxide_feed_v1_app::contract::PhysicalRect { x, y, width, height }` is an unsigned physical-pixel box used for expected components and clips.
- `oxide_feed_v1_app::contract::Rgba8 { red, green, blue, alpha }` is the canonical byte color; `Rgba8::new(red, green, blue, alpha) -> Rgba8` preserves explicit alpha and `Rgba8::rgb(red, green, blue) -> Rgba8` supplies opaque alpha.
- `oxide_feed_v1_app::contract::StartState::{Top, Bottom}` defines the two endpoints. `StartState::parse(&str) -> Option<StartState>`, `as_str() -> &'static str`, `direction() -> &'static str`, and `offset_points() -> u32` map controller text to frozen state, direction, and offset.
- `oxide_feed_v1_app::contract::RowRecipe::at(index: usize) -> Option<RowRecipe>` bounds and constructs one deterministic row. Accessors `index()`, `height_points()`, `caption_line_count()`, `checker_variant()`, `reply_count()`, `author()`, and `caption_line(line)` expose derived fields. Writers `write_id`, `write_component_id`, `write_title`, `write_caption`, and `write_metadata` reuse caller-owned `String` capacity.
- `oxide_feed_v1_app::contract::FeedFixture::new() -> FeedFixture` builds the full height prefix. `row_height_prefix_points`, `row_height_points`, `content_extent_points`, `maximum_content_offset_points`, `visible_row_range`, and `component_rect_physical_pixels` expose exact layout and endpoint geometry. `Default` delegates to `new`.
- `oxide_feed_v1_app::contract::CanonicalFixtureIdentity { byte_count, sha256 }` contains the observed complete-stream identity. `is_expected() -> bool` requires both fields to match the frozen Swift constants.
- `oxide_feed_v1_app::contract::canonical_fixture_identity(&FeedFixture) -> Option<CanonicalFixtureIdentity>` streams the complete canonical serialization through SHA-256 without materializing the 717,745-byte payload. It returns `None` if the frozen row or checker recipe cannot be completed.
- `oxide_feed_v1_app::contract::checker_rgba_bytes(variant, output) -> bool` fills one caller-owned 12-by-12 RGBA8 checker and rejects variants outside `0..64`.
- `oxide_feed_v1_app::contract::font_baseline_from_top(font_points) -> f32` and `caption_baseline_from_line_top() -> f32` reproduce Asap/UIKit vertical placement from embedded font units.

## Logic narrative

`RowRecipe::at` applies the frozen SplitMix-style 32-bit permutation to a row index. Disjoint bit ranges select one of four heights, one of 64 checker variants, an author, caption start/line count, and reply count. Text writers format stable four-digit IDs into caller-owned buffers so canonical identity and visible content use the same recipe without per-row stored objects.

`FeedFixture::new` evaluates all 2,000 heights once and stores a 2,001-element inclusive prefix. Endpoint extent is the last prefix entry. Visible-range lookup binary-searches the first intersecting row and linearly advances only across the small viewport tail. Component rectangles combine the prefix Y coordinate with fixed row-local geometry and convert once to 3x physical pixels.

`canonical_fixture_identity` is the sole Rust encoder of the Swift canonical order. It streams length-prefixed strings, little-endian integers, colors, font identities, all checker bytes, every row/component field, the supplied fixture prefix, and endpoint geometry into one SHA-256 state while counting the same bytes. The authoritative runner invokes its exact integration test before building device apps; runtime construction deliberately omits the pass so offscreen content remains cold. Tests call this production function instead of carrying a second encoder that could drift.

`checker_rgba_bytes` maps the low three variant bits to one of eight palettes, the next two to tile size, and the high bit to phase. It fills every source texel and forces opaque alpha. Baseline helpers scale Asap ascender/descender font units; caption leading is split around the glyph box because UIKit positions a fixed-height paragraph line while Oxide positions glyph baselines.

## Preconditions and postconditions

- Valid row indices are `0..ROW_COUNT`; valid checker variants are `0..CHECKER_VARIANT_COUNT`.
- The height prefix begins at zero, never decreases, has exactly 2,001 entries, and ends at `CONTENT_EXTENT_POINTS`.
- `maximum_content_offset_points()` equals extent minus viewport height and therefore equals `MAXIMUM_CONTENT_OFFSET_POINTS`.
- Every successful checker fill writes exactly `CHECKER_RGBA_BYTE_COUNT` bytes with alpha 255.
- `component_rect_physical_pixels` returns only the six names in `COMPONENT_KINDS` and uses content coordinates, not viewport coordinates.
- String writers clear their destination first and leave a complete deterministic value.
- Before a device build, the runner's Rust host check must stream exactly `EXPECTED_CANONICAL_BYTE_COUNT` bytes and produce `EXPECTED_CANONICAL_SHA256`.

## Edge cases and failure modes

- Out-of-range rows, caption lines, checker variants, and foreign component names return `None`/`false`; there is no fallback recipe.
- `visible_row_range` clamps offsets beyond the extent and uses saturating arithmetic at the viewport end.
- Prefix accumulation is saturating as a defensive boundary, while canonical tests prove the frozen workload does not approach overflow.
- Unknown `StartState` strings are rejected rather than silently mapped to an endpoint.
- Canonical identity returns `None` rather than hashing an incomplete stream if any frozen row or checker cannot be constructed.
- Pixel boxes use `u32`; all frozen dimensions and products fit exactly.

## Concurrency and memory behavior

Constants and recipes are immutable and require no synchronization. `RowRecipe`, `StartState`, `Rgba8`, `PhysicalRect`, and `CanonicalFixtureIdentity` are small copy values. `FeedFixture` owns approximately 8 KiB of prefix data and performs no mutation after construction. String/checker writers borrow exclusive caller-provided scratch, so they add no shared state or hidden allocation beyond growth chosen by the caller. Canonical verification owns one SHA-256 state, one fixed checker buffer, and five reusable strings; it never allocates the complete byte stream.

## Performance notes

- Row derivation is constant-time integer arithmetic with no heap access.
- Prefix construction is `O(ROW_COUNT)` once; first-visible lookup is `O(log ROW_COUNT)` and viewport enumeration is `O(visible rows)`.
- Caller-owned text buffers and checker bytes avoid temporary per-frame objects.
- Geometry is integer-first and converts to float only where the renderer needs point coordinates, preserving repeatability and avoiding cumulative rounding drift.
- Complete identity verification is one bounded `O(717,745 bytes)` host-preflight pass before device build. It never runs in a measured app process.

## Feature flags and cfgs

No feature flags or target cfg branches. The same recipe runs in native tests and on iOS.

## Testing and benchmarks

[`tests/contract_tests.md`](tests/contract_tests.md) verifies the production streamer's exact count and digest, every prefix/row, all checker variants, exact checker storage, and actual embedded font metrics. App integration tests additionally inspect uploaded checker bytes, rendered colors, endpoint offsets, and ready-frame geometry. The fixture helpers themselves are deterministic contract code, not timed benchmark results.

## Examples

```rust
use oxide_feed_v1_app::contract::{FeedFixture, RowRecipe};

let fixture = FeedFixture::new();
let row = RowRecipe::at(42).ok_or("row is outside the frozen feed")?;
assert_eq!(fixture.row_height_points(42), Some(row.height_points()));
# Ok::<(), &'static str>(())
```

## Changelog

- 2026-08-07: Moved complete-stream admission from app startup to the runner's authoritative Rust host preflight.
- 2026-08-06: Made one allocation-bounded production streamer own Rust canonical serialization and fail startup unless its complete count and SHA match Swift.
- 2026-08-06: Added mapped documentation and external integration coverage for the complete public fixture API.
- 2026-08-06: Added the independent deterministic feed-v1 recipe, exact geometry, image bytes, colors, fonts, schemas, and canonical identity.
