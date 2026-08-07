# oxide-feed-v1-reducer `lib.rs`

## Intention and purpose

`oxide-feed-v1-reducer` is the fail-closed evidence reducer for the benchmark-only `feed-v1` physical-iPhone pilot. It admits one frozen UIKit/Oxide comparison population, verifies that its visual, geometry, device, timing, and cleanup contracts are honest, and writes a publication report only for a complete full run.

It is not an Oxide runtime dependency and does not add behavior to either measured app.

## Relation to the rest of the code

- `benchmarks/pilots/feed-v1/ios/device-pilot` creates all run JSON plus one full-screen PNG for each of the six smoke tuples, device records, attachment export, evidence manifest, and cleanup proof consumed here.
- `benchmarks/pilots/feed-v1/ios/FeedV1Contract.swift` owns the source fixture recipe. The reducer independently reconstructs its frozen extent and visible-component geometry instead of trusting app output.
- `benchmarks/pilots/feed-v1/latest.json` and `latest.md` are publication targets only after full-population admission.

## Entry points list

- `reduce(&ReducePaths)` admits exactly six smoke diagnostics and one 54-run primary block, then atomically writes JSON and one dense Markdown report containing four travel-equivalence decisions and 54 primary timing rows.
- `verify_smoke(run_root)` applies the six-run smoke gates without timing classification or report output.
- `verify_attachment_export(root)` requires exactly six files, one manifest detail for the exact physical-device controller test, in-root canonical paths, unique names and file identities, and non-empty files.
- `build_evidence_manifest(source_root, repository_root, uikit_app, oxide_app, controller_runner, controller_xctest, output)` requires a clean named Git `HEAD` and hashes its ref/commit/tree, the strict `protocol.md` plus `ios/**` and `reducer/**` source allowlist, both built app bundles, both compiled controller products/executables, the executing reducer, and both frozen fonts.
- `strict_validate_run_json` and `strict_validate_failure_json` expose strict schema admission to regression tests.
- `visual_metrics` evaluates frozen luma SSIM, worst-tile RGB error, and exact whole-image RGB error for equal-size RGBA images; `travel_equivalence_passes` applies the inclusive frozen median and confidence-interval margins.
- `frozen_components`, `frozen_order_index`, `callback_deadline_counts`, and `deterministic_bootstrap_interval` expose small deterministic kernels for focused contract tests.

## Logic narrative

1. Recursively enumerate every retained evidence directory, rejecting a symlink root, nested symlinks, and non-regular filesystem entries rather than silently omitting them, while rejecting retained result bundles/tools/reducer binaries and more than 512 MiB of input. Names such as `.git`, `target`, and `build` receive no discovery exemption. The cleanup proof attests only that the reducer is absent from this retained root; the runner separately removes its external temporary executable after reduction and owns the final exit status.
2. Parse success and failure records with unknown-field rejection. Require each nonce-derived app record under its treatment's retrieved documents tree, exactly one authority manifest and cleanup proof at their frozen paths, and the six nonce-derived smoke PNGs in the exact controller test's single-detail attachment manifest. Reject canonical-path and Unix hard-link aliases. Any app failure record blocks the population.
3. Admit one booted physical 120 Hz iPhone with unchanged pre/post identity, unlocked preflight records, nominal thermal state, low-power mode off, zero thermal/power transition counts, and exact configured 120 Hz ranges. Require the controller's app-container runtime proof to remain within 20 minutes total and 10 minutes per treatment.
4. Validate fixture identity, canvas, frozen geometry, direction, at least 524 points of travel, observed inertial entry, order, and the exact smoke or full tuple population. Smoke makes no unreplicated cross-treatment travel claim. Full mode forms nine balanced primary deltas for each candidate treatment and direction, then requires the median inside 5 percent and its deterministic bootstrap interval inside 10 percent.
5. Derive callback pacing from each preceding `targetTimestamp - timestamp`. Every sample must be finite and forward; at least 95 percent of observed target periods must lie in `7.5 .. 9.2 ms`.
6. Require each smoke PNG to be the full `1320 x 2868` XCUIScreen capture, reject already-cropped input, extract `(75, 168) + 1170 x 2532`, and compare each Oxide/optimized surface with the matching idiomatic UIKit state. The six smoke visuals gate the manifest-hashed app builds used by the 54 primary timing runs; primary tuples add no screenshots, and there is no capture JSON or repeat gate.
7. Require luma SSIM `>= 0.96`, worst non-overlapping 48-by-48 RGB mean absolute error `<= 18`, and rejection of all nine hostile visual mutations.
8. Persist the four full-population travel-equivalence results with their pair counts, medians, confidence intervals, frozen margins, and decisions. For an otherwise admitted full population only, emit the 54 primary per-run callback summaries with inertia, environment-transition, and signed/absolute-travel evidence, summarize treatment p50/p95/p99/peak and missed/hitch metrics, then run the fixed 100,000-resample clustered pacing bootstrap over nine session/pair deltas. The six smoke records remain diagnostic evidence; blocked/smoke evaluations contain no timing rows.
9. Write JSON and one Markdown report through same-directory temporary files and atomic renames, excluding those exact output paths from future discovery so repeated reduction is byte-identical.

## Preconditions and postconditions

- Inputs use fixture SHA-256 `a1de9b4a914734fe21d21e9b6f8a9b61970f7e22e0fa4ef0103031e399881473` and the frozen `440 x 956 pt @3x` canvas.
- Smoke admission requires exactly six treatment/direction tuples and produces no report.
- Smoke and full evidence each retain exactly six full-screen smoke PNG attachments and no controller capture records.
- Publication requires six diagnostic smoke records plus exactly one primary block: 54 runs arranged as three sessions, three pairs per session, three treatments, and two directions, yielding nine paired clusters. The publication tables contain only those 54 primary runs.
- A successful full reduction produces revision-3 JSON and Markdown with status `complete`, including the four auditable travel-equivalence decisions; a blocked population produces no timing classification.

## Edge cases and failure modes

- Missing, foreign, duplicated, malformed, non-finite, misplaced, aliased, symlinked, or out-of-order records block admission. Attachment authority additionally requires the exact controller test identifier, one manifest detail, six attachments, in-root paths, and distinct file identities on Unix.
- Source evidence rejects symlinks and unclassified files, includes only `protocol.md` and admitted implementation files below `ios/` and `reducer/`, and excludes targets, builds, result/evidence trees, stale latest reports, logs, traces, XCTest result bundles, and generated Xcode projects. `project.yml` is authoritative; `FeedV1Pilot.xcodeproj` is generated only in the external build root.
- Device identity changes, a locked phone, Simulator evidence, 60 Hz target periods, thermal/power transitions, drag-only gestures, total/per-treatment runtime overflow, a changed post-export source snapshot, incomplete cleanup, Xcode-test failure, an attachment population other than the exact six smoke PNGs, or stale retained tooling block admission.
- PNG decode, full-screen dimensions, crop, component geometry, travel, or adversarial-gate failures block admission.
- Output paths must have a filename. Missing parent directories are created; atomic write or rename errors are returned to the caller.

## Concurrency and memory behavior

- Reduction is deterministic and single-process; it creates no worker pool and launches no app.
- Visual evaluation is linear in the cropped RGBA surfaces. Exact-population admission runs before expensive screenshot and hostile-mutation work. Hostile images are constructed, measured, and dropped one at a time instead of retaining all nine full surfaces together.
- The clustered bootstrap uses a fixed SplitMix64 seed and bounded 100,000-resample loop, making repeated reductions reproducible.

## Performance notes

- This crate measures and validates evidence; its own runtime is not an Oxide/UIKit performance result.
- Worst-tile and exact RGB error share one tile traversal, and the idiomatic reference rows use their mathematical identity result instead of rescanning a surface against itself.
- Dense near-duplicate device work is avoided: smoke is six tuples, while the 54 primary tuples exist only for the publication-grade clustered comparison.
- Reports label display-link observations as callback pacing, never presented-frame or photon latency.

## Feature flags and cfgs

No feature flags are used. On Unix, attachment admission compares device/inode file identities so hard links cannot authorize two capture names; canonical-path alias checks remain active on every platform. The reducer is a standalone Cargo workspace member so it does not expand the product workspace graph.

## Testing and benchmarks

Run:

```sh
CARGO_TARGET_DIR=/tmp/oxide-feed-v1-reducer-test \
  cargo test --locked --manifest-path oxide/benchmarks/pilots/feed-v1/reducer/Cargo.toml
```

`tests/reducer_tests.rs` covers strict schemas, the frozen recipe, nondegenerate travel and inertia, thermal/power transitions, visual corruption, bootstrap determinism, travel-equivalence margin and confidence boundaries, order, callback math/admission, failure blocking, non-secret device output, the six-smoke/six-PNG attachment bijection and exact test provenance, path and hard-link alias rejection, rejection of pre-cropped screenshot input, clean-Git source manifests, authority cleanup, the six-run smoke/full split, and byte-identical repeated full reduction with four travel decisions and 54 primary per-run summaries.

## Changelog

- 2026-08-06: Replaced unreplicated smoke travel matching with balanced primary travel equivalence, persisted its four decisions in revision-3 reports, and bound screenshot authority to the exact controller test and distinct file identities.
- 2026-08-06: Reduced visual evidence to one full-screen PNG for each of the six smoke treatment/direction tuples; removed capture JSON, repeat images, and the repeat gate.
- 2026-08-06: Reused the six smoke visuals as the immutable-build gate for one 54-run primary block and kept publication tables limited to those primary rows.
- 2026-08-06: Bound evidence to a clean named Git commit/tree and compiled controller, enforced run/screenshot provenance, inertial entry, transition counts, cleanup/test proof, per-run reporting, and idempotent full reduction.
- 2026-08-06: Restricted source evidence to the protocol/iOS/reducer implementation allowlist and added stale report/result/build contamination coverage.
- 2026-08-06: Added the strict smoke/full physical-device reducer, visual/adversarial gates, deterministic clustered comparison, evidence/cleanup admission, and dense single-report output.
