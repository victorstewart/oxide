# oxide-benchmark-spec::examples::materialize_pr_vertical_slice

## Intention and purpose

This deterministic generator materializes the complete runnable PR scenario set plus the separate nightly endurance scenario. It exists so Rust, Apple-native, and browser adapters consume the same committed bytes rather than reproducing fixture or trace constants in platform code.

## Relation to the rest of the code

- Reads and fully validates the pinned font-pack manifest already present under the supplied v1 root, including every hashed font's complete ordered `fvar` axis set and coordinate range.
- Materializes the shared inline-text raster variants from the pinned Noto Emoji source and freezes each feed/chat grapheme's grid cell and integer-millionths metrics in the asset manifest.
- Writes fixtures, layouts, assets, traces, checkpoint state/accessibility/design goldens, and canonical `ScenarioSpec` manifests.
- `pr_fixtures` gives those JSON fixtures typed semantic meaning.
- `pr_scenarios` and `validate_scenario_artifacts` reject incomplete, stale, or semantically weakened output before acquisition.

Call flow:

- `main`
  - parse and canonicalize `--root PATH`
  - materialize shared neutral style/atlas
  - materialize six PR scenario artifact closures and the nightly endurance closure
  - hash every referenced byte while constructing its manifest
  - refresh the canonical Apple PR plan scenario hashes

## Entry points list

- `materialize_pr_vertical_slice::main() -> anyhow::Result<()>` is the example binary entry point. It requires exactly `--root PATH`, writes the complete v1 artifact set, and reports each scenario manifest path.

## Logic narrative

The shared pass creates one neutral style contract with explicitly circular corners, a deterministic 128-tile content atlas, and a 5x2 inline-text grid pinned to Noto Emoji commit `8998f5dd683424a73e2314a8c1f1e359c19e8742` under the SIL Open Font License. It retains the 640x256 source raster and deterministically area-resamples premultiplied-alpha 225x90 and 195x78 variants whose 45- and 39-pixel cells exactly match canonical 15- and 13-point text at 3x. Adapters select the closest declared physical-em variant, making those canonical sizes 1:1 samples rather than runtime minification. The ten entries cover every symbol/emoji grapheme in feed and chat; adapters must emit these pixels as inline image runs and retain the original strings for accessibility. Scenario passes build frozen input data and integer-microsecond traces, then render implementation-neutral correctness goldens at 1170x2532 from the same fixture, layout, style, and pinned asset bytes. The rasterizer composites circular rounded clipping, a hard 2pt offset shadow at 16% opacity, and backdrop effects into fully opaque sRGB PNGs; it samples atlas cells and image source/thumbnail PNGs instead of substituting synthetic colors. Static Oxide-to-UIKit acceptance compares every normalized RGB channel in the full frame exactly. Layout text masks remain only for diagnostic SSIM/Delta-E and text-geometry attribution; they cannot exclude pixels from the static acceptance gate.

Feed row `height` is the total reusable-row extent. The frozen 8-point gap is realized by the card's 4-point top and bottom insets, so the materializer advances by `height` exactly and must not add a second gap. This preserves the declared nine initially visible rows.

Every checkpoint also freezes a schema-version-2 live model and semantic accessibility nodes containing role, name, value, state, order, focus, actions, logical frame, count, and visibility. Startup materializes 24 cards over an exact 24 KiB payload and six initial images. Chat materializes 5,000 multilingual messages, 64 avatars, 50 prepends, 10 Hz append, 100-character typing, 10 KiB paste, and selection replacement. Image materializes deterministic 4096x3072 and 384x288 PNGs plus separately attributable bytes-ready, decode, upload, first-visible, pan, and pinch traces. Every output reference stores its actual SHA-256 before canonical manifest serialization. After all six scenario manifests are stable, the generator refreshes their identities in `plans/apple-pr.json` through canonical typed serialization.

The materializer retains a typed pending-recapture marker on the three feed checkpoints affected by visible favorite targeting and the chat selection-replacement checkpoint. Re-materialization cannot silently make those stale correctness references eligible for timing; only a reviewed headed recapture removes the marker.

Endurance materializes a distinct typed fixture and 200 open/close, 500 tab-switch, and 600 animation-frame events across exactly 300,000 measured milliseconds. Its four final-state checkpoints own distinct state/accessibility artifacts but intentionally reuse the byte-identical dashboard idle screenshot; the nightly scenario is not added to the PR plan.

The shared style continues to emit `text_pixel_policy = normalized-srgb8-exact-static-full-frame`; font-axis pinning narrows glyph-instance inputs and does not weaken the zero-differing-pixel policy.

The startup lifecycle traces encode launch-class requests but do not turn the controller's 15-second readiness timeout into measured duration. Chat and image each carry exactly six seconds of measured phase duration. Gesture samples are elapsed-time driven rather than frame-count driven.

Navigation modal checkpoints start at the trace's 325,000-microsecond modal-open event and sample the following 300 milliseconds. They therefore cannot label the preceding detail-only state as modal evidence.

## Preconditions and postconditions

`--root` must identify an existing benchmark-spec v1 root containing `font-packs/oxide-bench-fonts-v1.json`, its referenced font/license files, and `plans/apple-pr.json`. Success writes all six canonical manifests, every referenced artifact, and refreshed plan scenario identities beneath that root. Re-running with unchanged source and font bytes produces byte-identical output.

## Edge cases and failure modes

Missing or extra CLI arguments, a nonexistent root, a missing or invalid font manifest, incomplete or out-of-range font axes, a missing Apple PR plan, JSON/PNG serialization failures, invalid output parents, and filesystem errors fail without partial success claims. Existing unrelated files are neither deleted nor traversed. The generator overwrites only its named deterministic outputs.

## Concurrency and memory behavior

Generation is single-threaded and owns all output buffers. The full 4096x3072 RGB image uses approximately 36 MiB during PNG encoding; checkpoint goldens use approximately 12 MiB each and are released between writes. Backdrop materialization uses bounded region-local blur buffers. No state is shared and no locks are used.

## Performance notes

All work occurs before acquisition. Best-compression Paeth filtering keeps the committed 4096x3072 image compact without changing decode dimensions or content. No generator allocation, encoding, or file I/O enters a renderer frame loop or benchmark sample.

## Feature flags and cfgs

No feature or target cfg changes generation. The `png` crate supports deterministic asset and golden encoding/decoding in the materializer and correctness tests.

## Testing and benchmarks

`pr_fixtures_tests` verifies typed workload semantics and drift rejection. `pr_scenario_tests` validates all six artifact closures, canonical manifests, complete diagnostic layout text masks, opaque 1170x2532 sRGB checkpoint PNGs, frozen navigation sampling times, dashboard card/backdrop geometry, and exact plus diagnostic reducer self-comparisons per scenario. Determinism is verified by rerunning the generator and comparing SHA-256 inventories.

## Examples

```text
cargo run --locked -p oxide-benchmark-spec --example materialize_pr_vertical_slice -- --root benchmarks/comparative/specs/v1
```

## Changelog

- 2026-07-21: preserved the four headed-recapture markers as typed checkpoint metadata across deterministic materialization.
- 2026-07-21: materialized the distinct five-minute `endurance.churn` artifact closure without expanding the PR plan.
- 2026-07-18: pinned circular corner geometry so Apple adapters cannot inherit OS-selected continuous/squircle coverage.
- 2026-07-18: materialized the pinned Noto Emoji inline-text source plus deterministic premultiplied-alpha 45- and 39-pixel variants and exact image-run metrics for all feed/chat fallback graphemes.
- 2026-07-18: validated pinned variable-font axes before generation and canonically refreshed Apple PR plan scenario hashes after font identity changes.
- 2026-07-18: made normalized full-frame RGB equality the static Oxide-to-UIKit acceptance policy; text masks remain diagnostic only.
- 2026-07-18: removed a duplicated feed row-gap advance so design goldens match the frozen 8-point gap and nine-row visible contract.
- 2026-07-18: made layout/style/fixture/pinned-asset materialization authoritative for opaque canonical goldens and complete text masks.
- 2026-07-18: extended materialization to startup, chat, and image decode/zoom, completing the six-scenario PR set.
- 2026-07-18: added dashboard, feed, and navigation vertical-slice materialization.
