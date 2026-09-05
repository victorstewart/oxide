# iOS core comparison

The previous cross-platform comparison campaigns have been retired. This directory
contains two small iOS apps: UIKit views and the production Oxide Metal renderer.
They currently implement only a presentation-measurement feasibility probe.

## Current result: Verification Pending

Both apps build in Release for physical iOS. On 2026-09-05, installation succeeded,
but direct launch was denied by iOS because the iPhone was locked. Animation Hitches
also timed out waiting for the device. No presentation metric, visual parity result,
or Oxide/UIKit performance claim was obtained. Diagnostic evidence is under
`../../benchmarks/uikit-device/core-probe/`; both device report roots record pending
status. A completed display-link callback is never counted as a presented frame.

The approved feasibility budget was at most 60 minutes. Device retries stopped on
the repeated external blocker. The other five workloads and the `cargo xtask ios
compare-core` runner are deliberately not implemented until measurement works.
No optimizer changes belong to this phase.

## Resume the measurement gate

Use an unlocked physical ProMotion iPhone, kept awake, with Developer Mode enabled.
Generate with `xcodegen generate` in this directory only if project.yml changed.
Build both apps:

```sh
xcodebuild -project AppleComparison.xcodeproj -scheme CoreComparison \
  -configuration Release -sdk iphoneos -destination 'generic/platform=iOS' \
  DEVELOPMENT_TEAM=YOUR_TEAM CODE_SIGN_STYLE=Automatic \
  SYMROOT=/private/tmp/oxide-core/products OBJROOT=/private/tmp/oxide-core/objects build
```

Install each app from `/private/tmp/oxide-core/products/Release-iphoneos/` with
`xcrun devicectl device install app --device COREDEVICE_ID APP_PATH`.
Use `xcrun devicectl list devices` for the CoreDevice identifier and
`xcrun xctrace list devices` for the Instruments hardware identifier; they differ.
For each bundle ID, capture normal and injected-delay runs to distinct output paths:

```sh
xcrun xctrace record --template 'Animation Hitches' --device INSTRUMENTS_ID \
  --time-limit 30s --output OUTPUT.trace --launch -- BUNDLE_ID
# Repeat with -oxide-core-probe-delay after BUNDLE_ID.
```

Bundle IDs are `com.oxide.comparison.uikitbenchios` and
`com.oxide.comparison.oxidebenchios`. Each run waits five seconds, updates eight of
64 tiles per frame for 20 seconds, then stops callbacks. The delayed variant inserts
one 100 ms main-thread stall. The fixed viewport is 390 × 844 points at 3× scale.
File-backed `Documents/core-probe.json` reports only workload completion, never a
performance result. `CoreProbe`, `ProbeMutation`, `ProbeUpdateSubmitted`, and
`InjectedDelay` signposts delimit work. Oxide additionally records
`OxidePresented` from the drawable's presented callback.

Inspect exported trace schemas before writing a reducer. The gate passes only if
both apps yield process-attributed actual presentation/cadence evidence, and the
injected stall is visible in that evidence. Establish the relationship between
mutation, submission, and presentation; signpost duration alone cannot establish
input-to-visible latency. Native-refresh behavior and visible scene equivalence
must also be observed. Missing presentation evidence stops suite construction.

## Frozen minimal suite after the gate

| Case | Workload |
| --- | --- |
| Shapes | 64 rectangular/rounded/translucent tiles, four clip groups, eight updates per frame |
| Text | 32 labels using the same Noto font and wrapping; one new string per 100 ms, glyphs prewarmed |
| Images | 16 instances of four precached assets, matching crop and scale |
| Local controls | 16 label/button/progress groups, one local update per 100 ms |
| Animation | Eight text/image cards translating, scaling, and fading on the same two-second timeline; native UIKit animation allowed |
| Scroll | 1,000 reused 96-point rows with two labels and an image; public Oxide CollectionView and UIKit reused cells; 0–3,840 points over ten seconds and back over ten seconds |

Use matching content, geometry, quality, cache state, native refresh, and input on
the same physical iPhone. Require visual equivalence, not byte-identical rasterization.
For each case use five paired repetitions, alternating which app runs first:
five seconds warm-up, 20 seconds measured, five seconds settled. That is 60 measured
segments (20 minutes measured, approximately 30 minutes plus launch overhead).
Allow only one replacement pair per identified acquisition failure.

Primary evidence is presentation cadence and hitches; text/local updates additionally
need event-to-present p50/p95 when directly attributable. App CPU and memory are
diagnostics, not total system cost. Oxide command-buffer GPU duration is diagnostic
unless an equivalent common GPU measure is available. Do not substitute simulation,
callback FPS, proxy energy, or unscoped all-process traces for missing evidence.
The eventual runner is one command with optional case/device/output selection;
it must not grow back into a campaign framework.

## Cleanup and recovery

Before deletion, all preexisting work was checkpointed in commit
`f6e750f0815e13b7ce60891a9256d65f3c8c402a`. Its production text, WGPU/Web, and
renderer changes were preserved, not validated by that checkpoint.
A verified recovery archive of 1,550 files and a verified complete Git bundle are at
`/Users/victorstewart/oxide-recovery/comparison-reset-20260905T191612Z`.
See its `RECOVERY.txt` for restoration. The three retired local branches
`agent/compact-uikit-pilot`, `agent/compact-uikit-pilot-final`, and
`agent/controlled-ab-harness` were deleted after archival. The two corresponding
worktree directories were already absent when execution checked them.

Removed machinery includes the embedded UIKit benchmark app/test runners, old
comparison controllers/specs/fixtures/reports/calibration/density systems, macOS
and web comparator apps, and comparative scene routing. General engine benchmarks,
paired before/after statistics, snapshots, and normal platform hosts remain.
Shared Noto fonts and licenses now live in `../../crates/text/tests/fixtures/`.

## Cleanup verification

Observed on 2026-09-05:

- Release iPhone builds passed for OxideBenchIOS, UIKitBenchIOS, and OxideHost.
- Focused Rust check passed for the comparator runtime, test-scenes, perf-runner,
  and xtask. The retained Oxide device runner builds the normal OxideHost app
  and no longer requires the deleted UIKit scheme or XCTest artifacts.
- Normal-host drawable tests: 11 passed. Test-scene tests: nine passed.
- Text shaping tests: 26 passed, including the relocated variable font fixture.
- Paired experiment tests: ten passed. Focused xtask tests passed.
- A broader report test run exposed three obsolete device-fixture expectations,
  which were removed while retaining workspace report invariants. The other 115
  report tests pass with the failing glyph test explicitly skipped.
- The retained `smoke_suite_keeps_popup_wheel_picker_case_id_stable` test fails,
  including in isolation, at the unmodified architecture_matrix.rs assertion
  `Apple glyph-instance case must use the native bitmap backend`. No glyph-backend
  or renderer behavior was changed to mask this separate failure.

Physical launch/render/input smoke for the normal host and visual/presentation
validation for the comparator apps remain pending because device launch was locked.
Build success is not a substitute for those observations. Hosted CI was not run.
