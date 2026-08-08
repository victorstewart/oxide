# oxide-feed-v1-reducer `lib.rs`

## Intention and purpose

`oxide-feed-v1-reducer` is the fail-closed evidence reducer for the benchmark-only `feed-v1` physical-iPhone pilot. It admits one frozen UIKit/Oxide comparison population, verifies that its visual, geometry, device, timing, and cleanup contracts are honest, and writes a publication report only for a complete full run.

It is not an Oxide runtime dependency and does not add behavior to either measured app.

## Relation to the rest of the code

- `benchmarks/pilots/feed-v1/ios/device-pilot` creates all run JSON, two immediate full-screen PNGs for each of the six publication smoke tuples, device and runner records, attachment exports, evidence manifest, and cleanup proof consumed here. Primary retains no screenshots.
- `benchmarks/pilots/feed-v1/ios/FeedV1Contract.swift` owns the source fixture recipe. The reducer independently reconstructs its frozen extent and visible-component geometry instead of trusting app output.
- `benchmarks/pilots/feed-v1/latest.json` and `latest.md` are publication targets only after full-population admission.

## Entry points list

- `reduce(&ReducePaths)` admits exactly six smoke diagnostics and one 54-run primary block, then atomically writes canonical JSON with all primary rows/raw callback samples and one compact aggregate Markdown report bound to the JSON path and SHA-256.
- `verify_smoke(run_root)` applies the six-run smoke gates without timing classification or report output.
- `admit_smoke_prefix(run_root)` applies the publication smoke gate before a full run is allowed to launch its primary phase, without requiring final runner/cleanup proof.
- `verify_attachment_export(root)` requires exactly 12 publication smoke files plus one manifest detail for the exact physical-device controller test, in-root canonical paths, unique names and file identities, and non-empty files.
- `verify_no_attachment_export(root)` requires a manifest-authorized export with zero primary attachments.
- `build_evidence_manifest(source_root, repository_root, uikit_app, oxide_app, controller_runner, controller_xctest, build_provenance, output)` requires a clean named Git `HEAD`; strictly admits the frozen device, toolchain, resolved-build, production-dependency, and signing provenance; and hashes the ref/commit/tree, strict `protocol.md` plus `ios/**` and `reducer/**` source allowlist, both built app bundles, both compiled controller products/executables, the executing reducer, and both frozen fonts.
- `strict_validate_run_json`, `strict_validate_failure_json`, `strict_validate_runner_json`, and `strict_validate_controller_runtime_json` expose strict schema admission to regression tests.
- `visual_metrics` evaluates frozen luma SSIM, worst-tile RGB error, and exact whole-image RGB error for equal-size RGBA images; `travel_equivalence_passes` applies the inclusive frozen median and confidence-interval margins.
- `MedianConfidenceInterval` carries the exact method, requested and achieved
  coverage, sample count, one-based rank bounds, and numeric interval bounds.
- `frozen_components`, `frozen_order_index`, `callback_deadline_counts`, and
  `exact_median_confidence_interval` expose small deterministic kernels for
  focused contract tests. `nearest_rank_quantile` implements the publication
  quantile rule with one-based rank `ceil(q * n)`.

## Logic narrative

1. Recursively enumerate every retained evidence directory, rejecting a symlink root, nested symlinks, and non-regular filesystem entries rather than silently omitting them, while rejecting retained result bundles/tools/reducer binaries, more than 512 MiB, more than 512 files, or any file above 128 MiB. Names such as `.git`, `target`, and `build` receive no discovery exemption. Hash every retained regular file below `raw/` into a lexicographically sorted relative-path/byte-count/SHA-256 inventory, then rehash immediately before publication so an input mutation cannot leave the report bound to stale bytes. Cleanup revision 3 additionally proves the prelaunch fuse pass, bounded external build/result bundle, exact absence of both measured apps plus the controller, and absence of the reducer binary from the retained root. The runner separately removes its external temporary reducer after reduction and owns the final exit status.
2. Parse success, failure, and runner records with unknown-field rejection. Require each nonce-derived app record under its treatment's retrieved documents tree, exactly one authority manifest and cleanup proof at their frozen paths, and the 12 nonce/capture-derived smoke PNGs in the exact controller test's single-detail attachment manifest. Reject canonical-path and Unix hard-link aliases. Any app failure record blocks the population, while `raw/runner.json` preserves the first orchestration blocker and whether full-mode smoke admission preceded primary.
3. Admit one booted physical 120 Hz iPhone with unchanged pre/post identity, unlocked preflight records, nominal thermal state, low-power mode off, zero thermal/power transition counts, and exact configured 120 Hz ranges. Publication smoke requires the mode-specific revision-2 smoke controller-runtime proof; full publication additionally requires the primary proof with exactly three ordered admissible session environments. Their runtimes are summed before enforcing 20 minutes total and 10 minutes per treatment.
4. Validate fixture identity, canvas, frozen geometry, direction, at least 524 points of travel, observed inertial entry, order, and the exact smoke or full tuple population. Smoke makes no unreplicated cross-treatment travel claim. Full mode forms nine balanced primary deltas for each candidate treatment and direction, then requires the median inside 5 percent and its exact rank-2-to-rank-8 interval inside 10 percent.
5. Derive callback pacing from each preceding `targetTimestamp - timestamp`. Every sample must be finite and forward; at least 95 percent of observed target periods must lie in `7.5 .. 9.2 ms`. Per-run p50/p95/p99 and per-cluster p50/p95 values use one-based nearest-rank selection rather than interpolation.
6. Require each smoke PNG to be the normalized full `1320 x 2868` XCUIScreen capture, reject already-cropped input, and extract `(75, 168) + 1170 x 2532`. Publication requires admission/repeat pairs for all six tuples, independently gates both images, and requires each normalized pair to be byte-identical. Primary tuples add no screenshots and must pass the zero-attachment verifier.
7. Require luma SSIM `>= 0.96`, worst non-overlapping 48-by-48 RGB mean absolute error `<= 18`, and rejection of all nine hostile visual mutations.
8. Concatenate forward/reverse intervals only within each treatment/session/pair cluster, persist each cluster's p50/p95, and use the median of the nine cluster p50s/p95s as the treatment aggregates. Four travel-equivalence decisions and two pacing intervals use exact ranks 2 and 8. Serialize the complete statistical, admission-threshold, classification-threshold, and pacing-guardrail policy beside those results.
9. Reject a malformed or inadmissible controller proof before writing `latest.json` or `latest.md`. A complete decision encodes the idiomatic and optimized UIKit classifications independently in fixed order; a blocked population reports only `blocked`. Canonical JSON retains all 54 raw-sample rows, the observed cleanup proof, and the sorted raw-file inventory. Markdown keeps only aggregate tables and links itself to the canonical JSON bytes by relative path and hash. Writes are atomic and exclude only the declared outputs from repeat discovery.

## Preconditions and postconditions

- Inputs use fixture SHA-256 `a1de9b4a914734fe21d21e9b6f8a9b61970f7e22e0fa4ef0103031e399881473` and the frozen `440 x 956 pt @3x` canvas.
- Smoke admission requires exactly six treatment/direction tuples and produces no report.
- Smoke and full evidence each retain exactly 12 full-screen smoke PNG attachments and no controller capture records. Primary evidence retains zero attachments.
- Publication requires six diagnostic smoke records plus exactly one primary block: 54 runs arranged as three sessions, three pairs per session, three treatments, and two directions, yielding nine paired clusters. Canonical JSON contains those 54 primary rows and their raw callback samples; compact Markdown contains no per-run table.
- Publication additionally requires a mode-specific smoke controller proof and, for full mode, one primary controller proof.
- A successful full reduction produces revision-5 JSON and Markdown with status `complete`, including the four auditable travel-equivalence decisions; a blocked population produces no timing classification. Historical revision-3 and revision-4 reports remain unchanged.

Controller-runtime revision 2 requires `session_environments`. Smoke uses `[]`;
primary uses exactly three
ordered entries for session indices 0, 1, and 2, sampled before and after each
session's 18 launches. Both endpoints require thermal `nominal`, Low Power Mode
off, maximum refresh exactly 120 Hz, and configured minimum, maximum, and
preferred refresh exactly 120 Hz; both transition counts must be zero.

## Edge cases and failure modes

- Missing, foreign, duplicated, malformed, non-finite, misplaced, aliased, symlinked, out-of-order, or environment-contradictory records block admission. Attachment authority additionally requires the exact controller test identifier, one manifest detail, 12 attachments, in-root paths, and distinct file identities on Unix; primary requires zero attachments.
- Source evidence rejects symlinks and unclassified files, includes only `protocol.md` and admitted implementation files below `ios/` and `reducer/`, and excludes targets, builds, result/evidence trees, stale latest reports, logs, traces, XCTest result bundles, and generated Xcode projects. `project.yml` is authoritative; `FeedV1Pilot.xcodeproj` is generated only in the external build root. Build provenance additionally rejects a foreign CoreDevice/hardware pair, a non-120-Hz contract, translated Rust host, malformed dependency/build hashes, or any product outside one Apple Development team.
- Device identity changes, a locked phone, Simulator evidence, 60 Hz target periods, thermal/power transitions, drag-only gestures, total/per-treatment runtime overflow, a breached prelaunch/build/result/evidence/file fuse, a surviving UIKit/Oxide/controller process, a changed post-export source snapshot, incomplete cleanup, Xcode-test failure, an attachment population other than 12 smoke PNGs or zero primary PNGs, or stale retained tooling block admission.
- PNG decode, full-screen dimensions, crop, component geometry, travel, or adversarial-gate failures block admission.
- Output paths must have a filename. Missing parent directories are created; atomic write or rename errors are returned to the caller.

## Concurrency and memory behavior

- Reduction is deterministic and single-process; it creates no worker pool and launches no app.
- Raw evidence hashing is linear in retained bytes and runs in two bounded
  passes so the pre-analysis inventory can be checked immediately before write;
  it adds no measured app work and keeps only one digest state plus one compact
  inventory entry per file.
- Visual evaluation is linear in the cropped RGBA surfaces. Exact-population admission runs before expensive screenshot and hostile-mutation work. Hostile images are constructed, measured, and dropped one at a time instead of retaining all nine full surfaces together.
- Each of the six nine-cluster confidence intervals sorts nine values once and
  selects ranks 2 and 8. The exact binomial method achieves 96.09375 percent
  coverage for the requested 95 percent level without an RNG or resampling.
- Callback quantiles sort only one run or one two-direction cluster at a time;
  treatment p50/p95 aggregation then sorts nine cluster values, so unequal raw
  callback counts cannot weight one cluster as multiple independent trials.

## Performance notes

- This crate measures and validates evidence; its own runtime is not an Oxide/UIKit performance result.
- Worst-tile and exact RGB error share one tile traversal, and the idiomatic reference rows use their mathematical identity result instead of rescanning a surface against itself.
- Dense near-duplicate device work is avoided: smoke is six tuples, while the 54 primary tuples exist only for the publication-grade clustered comparison.
- Reports label display-link observations as callback pacing, never presented-frame or photon latency.
- Mixed comparator outcomes remain mixed in both JSON rows and the fixed-order
  terminal decision; the reducer never synthesizes a global framework verdict.

## Feature flags and cfgs

No feature flags are used. On Unix, attachment admission compares device/inode file identities so hard links cannot authorize two capture names; canonical-path alias checks remain active on every platform. The reducer is a non-default root workspace member, so it shares dependency resolution and build artifacts without expanding ordinary product builds. Its root-owned `feed-v1-reducer` profile preserves the pre-workspace release panic policy.

## Testing and benchmarks

Run:

```sh
cd oxide
cargo test --locked -p oxide-feed-v1-reducer
```

Unit and integration coverage includes strict app, runner, and controller schemas; run-nonce binding; the frozen recipe; nondegenerate travel and inertia; thermal/power transitions; session environment endpoints; visual corruption and repeatability; exact interval ranks/coverage; travel-equivalence boundaries; order; callback math/admission; failure blocking; non-secret device output; smoke and empty attachment provenance and alias rejection; smoke-before-primary admission; clean-Git manifests; cleanup; blocked-primary quarantine; and byte-identical full reduction.

## Changelog

- 2026-08-07: Added cleanup revision 3 with predeclared build/result/evidence/file fuses and exact three-process absence.
- 2026-08-07: Bound evidence revision 3 to the exact phone, OS/toolchains, resolved Release settings, production Cargo graph, and signed product identities.
- 2026-08-07: Retained raw callback samples, frozen structured policy, observed
  cleanup proof, and sorted raw-file hashes in canonical JSON; reduced Markdown
  to aggregate results with the JSON path/hash and replaced obsolete follow-on
  language with fixed-order dual-comparator classifications.
- 2026-08-07: Made all publication callback quantiles one-based nearest-rank,
  changed treatment p50/p95 to medians of nine cluster summaries, and based
  faster classification on those aggregates in revision-5 reports.
- 2026-08-07: Restored two immediate publication captures per smoke tuple, split smoke/primary admission and runtime proofs, added runner blockers, and enforced revision-2 session environments.
- 2026-08-07: hard-cut revision-4 reports to exact nine-cluster median
  intervals with ranks 2 and 8 and 96.09375 percent achieved coverage; removed
  all bootstrap seeds, resample fields, and resampling loops.
- 2026-08-07: Joined the non-default root workspace graph and removed the nested lock/profile boundary.
- 2026-08-06: Replaced unreplicated smoke travel matching with balanced primary travel equivalence, persisted its four decisions in revision-3 reports, and bound screenshot authority to the exact controller test and distinct file identities.
- 2026-08-06: Reduced visual evidence to one full-screen PNG for each of the six smoke treatment/direction tuples; removed capture JSON, repeat images, and the repeat gate.
- 2026-08-06: Reused the six smoke visuals as the immutable-build gate for one 54-run primary block and kept publication tables limited to those primary rows.
- 2026-08-06: Bound evidence to a clean named Git commit/tree and compiled controller, enforced run/screenshot provenance, inertial entry, transition counts, cleanup/test proof, per-run reporting, and idempotent full reduction.
- 2026-08-06: Restricted source evidence to the protocol/iOS/reducer implementation allowlist and added stale report/result/build contamination coverage.
- 2026-08-06: Added the strict smoke/full physical-device reducer, visual/adversarial gates, deterministic clustered comparison, evidence/cleanup admission, and dense single-report output.
