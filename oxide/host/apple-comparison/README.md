# Isolated iOS core comparison probe

These two optional apps explore whether Oxide and UIKit can be compared using
actual presentation evidence on the same physical ProMotion iPhone. They do not
replace the existing upstream benchmark tooling, which is preserved for Nametag.

## Status

Presentation measurement is **Verification Pending**. The first attempt could not
launch because the phone was locked. After unlocking, the UIKit app completed its
diagnostic workload (2,400 update callbacks); callbacks do not establish presentation.
An Instruments attach using the device name connected, but that exploratory run
was interrupted by another app launch and is invalid as performance evidence.
Device work was then explicitly paused to reconcile local main with origin/main.
No Oxide/UIKit performance claim or complete six-case suite exists yet.

Each app renders 64 rounded tiles in a fixed 390 × 844-point viewport at 3× scale.
After five seconds, it updates eight tiles per callback for 20 seconds and then
stops. `-oxide-core-probe-delay` injects one 100 ms main-thread stall. File-backed
`Documents/core-probe.json` records workload completion only. The Oxide app uses
the production Metal renderer and additionally signposts drawable presented time.

Resume only the measurement gate first: capture normal and delayed runs of each
app, establish process-attributed presentation/cadence and detect the injected
stall, verify native refresh and visible equivalence. Missing presentation evidence
stops additional workload construction. Never equate callback timing, app CPU,
or command-buffer completion with display presentation or total system cost.

## Build and run

Build both targets through the `CoreComparison` scheme in
`AppleComparison.xcodeproj`, Release configuration, iphoneos SDK, and your signing
team. Install the resulting OxideBenchIOS.app and UIKitBenchIOS.app using
`xcrun devicectl device install app`.

Bundle IDs are `com.oxide.comparison.oxidebenchios` and
`com.oxide.comparison.uikitbenchios`. Use `devicectl list devices` for the CoreDevice
identifier and `xctrace list devices` for Instruments identifiers. In the exploratory
run, the device's displayed name succeeded where the hardware-identifier path did
not. Record with Animation Hitches for 30 seconds to a fresh output path each time.
Do not launch another app during a capture.

`CoreProbe`, `ProbeMutation`, `ProbeUpdateSubmitted`, and `InjectedDelay` signposts
identify workload phases. `OxidePresented` uses the drawable presentation callback.
Export and inspect the actual trace schema before implementing a reducer.

## Proposed minimal suite, after measurement works

| Case | Fixed workload |
| --- | --- |
| Shapes | 64 tiles, four clip groups, eight updates per frame |
| Text | 32 matching Noto labels; one new string per 100 ms, prewarmed glyphs |
| Images | 16 instances of four precached assets, matching crop and scale |
| Local controls | 16 label/button/progress groups; one local update per 100 ms |
| Animation | Eight text/image cards translating, scaling and fading over two seconds |
| Scroll | 1,000 reused 96-point rows with two labels/image; 3,840-point trip over ten seconds and back |

Use the same device/content/geometry/cache state/native refresh and equivalent
visual quality. Five paired repetitions per case, alternating order, with five
seconds warm-up, 20 seconds measured, and five seconds settled. One replacement
pair per identified acquisition failure. The eventual runner and other five cases
remain unimplemented; no optimization is part of the measurement phase.

## Reconciliation and recovery

Local work had diverged from origin/main by 169 upstream commits. The reconciliation
uses origin/main as the source of truth and preserves its runtime, host, test,
benchmark, and Nametag-facing functionality. Local variable-font support is additive
to the upstream text/cache implementation; WGPU and this probe remain optional.
The earlier local cleanup commit does not authorize deleting upstream functionality.

Both original tips are preserved by local `codex/recovery-*-before-reconcile`
branches and the verified complete-history bundle:
`/Users/victorstewart/oxide-recovery/reconcile-20260905.bundle`.
The pre-cleanup file archive remains at
`/Users/victorstewart/oxide-recovery/comparison-reset-20260905T191612Z`.
