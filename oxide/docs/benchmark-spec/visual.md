# oxide-benchmark-spec::visual

## Intention and purpose

`visual` implements host-side screenshot contracts. Cross-framework static admission uses full-frame SSIM, a stable-interior differing-pixel limit, and an unmasked 16×16 physical-pixel voxel guard after opaque sRGB8 normalization and canonical-dimension validation. Text-line regions may be excluded only from the stable-interior count; full-frame and local guards still evaluate every pixel. Exact RGB and Delta-E remain diagnostics and exact RGB is the same-renderer repeatability gate.

## Relation to the rest of the code

- Scenario manifests identify the checkpoint PNG and content-addressed layout JSON.
- Apple, macOS, and browser hosts capture candidate PNGs and line/baseline/ink evidence outside measured intervals.
- `xtask compare-ui` supplies those bytes to this module and persists `VisualParityReport` beside raw correctness evidence.
- Comparative analysis may consume timing data only when the report's `accepted` field is true.

Call flow:

- `reduce_normalized_png_visual_parity`
  - validate frozen thresholds, logical coordinate space, root, and scale
  - decode reference and candidate PNG channels into opaque sRGB8 RGB
  - require identical PNG dimensions and exact `root × canonical_scale` dimensions
  - floor mask origins and ceil mask far edges into physical pixels
  - compute non-text 8×8 windowed SSIM, differing-pixel ratio, and CIEDE2000 distribution
  - validate text line count, baselines, and ink-bound edges or report `pending`/`rejected`
  - accept only when both non-text thresholds and text geometry pass
- `compare_calibrated_static_pngs`
  - derive text regions only from live `textLineBounds`, expanded by one physical pixel
  - compare every pixel, including text and rounded edges, with full-frame SSIM
  - apply the differing-pixel ratio only to the stable interior outside those narrow text regions
  - average each RGB channel inside fixed 16×16 physical-pixel voxels, then compare the two voxel colors
  - reject concentrated defects whose mean absolute voxel-channel delta exceeds `45/255`, even when whole-frame averages pass
  - retain the complete exact comparator report as diagnostics

## Entry points list

- `oxide_benchmark_spec::visual::reduce_normalized_png_visual_parity(reference_png: &[u8], candidate_png: &[u8], layout_json: &[u8], canonical_scale: u32, text_geometry: Option<&TextGeometryEvidence>, thresholds: VisualThresholds) -> anyhow::Result<VisualParityReport>` performs one deterministic checkpoint reduction.
- `oxide_benchmark_spec::visual::compare_calibrated_static_pngs(...) -> anyhow::Result<CalibratedStaticVisualParityReport>` performs cross-framework static screenshot admission.
- `oxide_benchmark_spec::visual::compare_exact_static_pngs(oxide_png: &[u8], uikit_png: &[u8], layout_json: &[u8], canonical_scale: u32) -> anyhow::Result<ExactStaticVisualParityReport>` performs the full-frame exact diagnostic comparison with no masks or tolerances.
- `oxide_benchmark_spec::visual::CalibratedStaticVisualThresholds::default() -> CalibratedStaticVisualThresholds` returns full-frame SSIM `>= 0.9795`, stable-interior differing-pixel ratio `<= 0.03` with a per-channel tolerance of `2`, and a maximum 16×16 mean absolute voxel-channel delta of `45`.
- `oxide_benchmark_spec::visual::VisualThresholds::default() -> VisualThresholds` returns SSIM `>= 0.995`, differing-pixel ratio `<= 0.005` with a per-channel tolerance of `2`, median Delta-E `<= 1`, and p99 Delta-E `<= 3`.
- `oxide_benchmark_spec::visual::TextValidationStatus::is_validated(&self) -> bool` identifies the only text status eligible for overall acceptance.
- `oxide_benchmark_spec::visual::{LogicalRect, PhysicalRect}` describe logical geometry evidence and the exact physical text regions persisted in a report.
- `oxide_benchmark_spec::visual::{TextLineGeometry, TextGeometryEvidence}` carry ordered reference/candidate baseline and ink-bound evidence in logical points.
- `oxide_benchmark_spec::visual::{NonTextVisualMetrics, RasterVisualMetrics, VisualParityReport, CalibratedStaticVisualParityReport}` are serde-compatible deterministic result records for `xtask` and report rendering.
- `oxide_benchmark_spec::visual::NORMALIZED_PNG_VISUAL_ALGORITHM` freezes the legacy masked algorithm identity `normalized-srgb8-ssim8-ciede2000-v1`.
- `oxide_benchmark_spec::visual::CALIBRATED_STATIC_VISUAL_ALGORITHM` freezes rapid admission as `normalized-srgb8-semantic-region-ssim8-voxel-average16-v5`.

## Logic narrative

PNG decoding uses `png`'s color8 normalization, then expands RGB, RGBA, grayscale, or grayscale-alpha into a single RGB buffer. Alpha must be fully opaque because choosing a backdrop inside the reducer would change pixels and could create framework-specific bias. PNG color samples are interpreted as normalized sRGB8; CIEDE2000 linearizes them before D65 XYZ and Lab conversion.

The exact static comparator uses the same decoding and opacity rules. RGB and opaque RGBA encodings of the same pixels therefore compare equal, while a one-level change in one channel rejects its diagnostic `accepted` field. Cross-framework admission is owned by the calibrated report, which persists that exact result without treating sparse platform raster differences as a visual failure.

Every `[x, y, width, height]` text mask in layout JSON is expressed in logical points. Origins use floor and far edges use ceil after multiplication by the positive integer canonical scale, ensuring fractional coverage is never left in the non-text population. Overlapping masks are unioned in one boolean bitmap. A mask extending beyond the exact physical root fails instead of being clipped.

The calibrated algorithm does not consume the broad legacy layout masks. It reads each runtime node's actual text-line rectangles, expands each by exactly one physical pixel, clips the expansion to the root, and unions those narrow regions. Node bounds are never used as text regions. Rounded edges and every text pixel remain in full-frame SSIM and in the voxel guard. The voxel guard first averages each framework's RGB pixels into the same physical cell and then compares those two averaged colors, so harmless subpixel redistribution can cancel without allowing missing content or a localized structural defect to disappear. Only the stable-interior differing-pixel ratio excludes the narrow text regions.

The differing-pixel ratio counts a pixel when any RGB channel differs by more than `2`. SSIM uses Rec.709 sRGB luma and fixed non-overlapping 8×8 physical-pixel windows; excluded samples are omitted and each local score is weighted by its remaining sample count. The calibrated path does not perform CIEDE2000 conversion, avoiding duplicate full-frame color-space work that cannot affect acceptance. There is no random sampling, parallel reduction, clock, filesystem, or platform-dependent branch, so identical input bytes produce identical serialized fields on the supported target.

Text pixels can be omitted from pixel statistics only when explicit logical masks exist. Pixel exclusion does not validate text: ordered line counts must match and logical baseline/ink-bound edge deltas must remain within layout tolerances, defaulting to 0.5 logical point. Missing geometry, empty geometry, or a requested mask policy without mask rectangles yields `pending`; mismatched geometry yields `rejected`. Neither status can set overall `accepted`.

## Preconditions and postconditions

- Both inputs are complete decodable PNG byte slices whose pixels are opaque after color8 normalization.
- Layout JSON declares `coordinate_space: "logical-points"`, a positive origin-zero root, and optional positive text masks.
- `canonical_scale` is a nonzero integer and the scaled root dimensions are exact integers.
- Optional text geometry is finite, nonnegative, ordered identically on both sides, and expressed in logical points.
- Success returns full-frame metrics for every pixel, stable-interior metrics for every pixel outside narrow text regions, and at least one SSIM window with two samples in each population. Calibrated `accepted` implies the full-frame SSIM, stable-interior differing-pixel, and unmasked voxel thresholds all pass.

## Edge cases and failure modes

Malformed PNG/JSON, indexed output that survives normalization, nonopaque pixels, zero scale, non-finite or negative geometry, fractional physical root dimensions, different PNG sizes, wrong canonical dimensions, out-of-bounds/empty legacy masks, text regions covering the whole image, invalid thresholds, and insufficient SSIM samples return an error. Calibrated text regions that touch the root are clipped after their one-pixel expansion. Missing legacy text evidence remains a reportable pending result rather than a parser error because hosts may need to preserve partial correctness evidence while refusing a legacy parity claim.

## Concurrency and memory behavior

Reduction is synchronous and owns two RGB buffers plus bounded full-frame and text-region mask bitmaps. The calibrated path allocates no Delta-E vector. No global state, locks, threads, or unsafe code are used. The input byte slices and optional geometry are borrowed for the call; the report owns only summary fields and physical text rectangles.

## Performance notes

This work runs outside all benchmark measurement intervals. Decode, region construction, raster metrics, exact diagnostics, and fixed-window SSIM are linear in physical pixels. The calibrated path avoids the prior two full-image Delta-E populations and never traverses a node's full bounds to create a text exclusion. The implementation deliberately favors deterministic scalar math and a small API over a benchmark-hot optimization path. App frame, draw, GPU, allocation, and latency counters are unaffected.

## Feature flags and cfgs

The reducer has no feature or target-specific branches. The `png` dependency is a normal dependency because production `xtask` analysis invokes the reducer.

## Testing and benchmarks

`crates/benchmark-spec/tests/visual_tests.rs` covers exact full-frame equality and one-channel rejection alongside exact dimensions, conservative legacy mask scaling, calibrated defaults, deterministic report JSON, opacity rejection, validated/rejected/pending legacy text evidence, sparse accepted noise, accepted within-voxel redistribution, a wrong interior inside a text-bearing node, missing text hidden inside a declared line, bounded edge noise, and a half-sized defect rejected by the unmasked voxel guard. Run `cargo test --locked -j12 -p oxide-benchmark-spec --test visual_tests`.

## Examples

Load the design PNG, captured PNG, and the scenario's `scene.layout_assertions` JSON. Pass the scenario's canonical integer scale and ordered host text geometry to `reduce_normalized_png_visual_parity`. Persist the returned report even when `accepted` is false so optimization results remain linked to their exact parity failure.

For every selected static or settled cross-framework endpoint, pass the matched Oxide and native PNGs plus live geometry to `compare_calibrated_static_pngs` and persist its report. Use `compare_exact_static_pngs` only for diagnostics and the one same-implementation repeatability sentinel.

## Changelog

- 2026-07-26: froze v5 true voxel averaging and the rapid equivalence thresholds at `0.9795` full-frame SSIM, `3%` stable-interior difference, channel tolerance `2`, and maximum voxel delta `45`.
- 2026-07-26: replaced broad text-node and rounded-edge exclusions with v4 narrow text-line regions, full-frame SSIM, stable-interior differing-pixel checks, and an unmasked voxel guard; removed Delta-E from calibrated admission work.
- 2026-07-20: replaced cross-framework exact admission with frozen full-frame perceptual thresholds and a localized 16×16 voxel guard; exact RGB remains diagnostic.
- 2026-07-18: added exact full-frame static Oxide↔UIKit comparison with zero pixel/channel tolerance; masked and perceptual results remain diagnostic only.
- 2026-07-18: added normalized opaque sRGB8 decoding, exact dimensions, logical text masks, frozen non-text metrics, CIEDE2000 quantiles, fail-closed text geometry, and deterministic public reports.
