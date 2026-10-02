# iOS core comparison

Two optional iOS apps implement the six workloads in [the contract](../../benchmarks/CONTRACT.md).
The [fixture](fixtures/core.json) fixes geometry, font, images and timelines. Production
library APIs are unchanged. This phase contains no renderer optimization.

## Status

Current measurement preparation is recorded in the
[images baseline preparation](../../benchmarks/core-baseline-2026-09-06/README.md).
It includes refreshed timed-image visuals and app-attributed cadence admission.
The remainder of this status section preserves the earlier attempt.

The measurement feasibility gate passed on 2026-09-06 within its resumed 60-minute
window. UIKit update intervals join to display swaps by exact surface/swap identity.
The isolated Oxide single-update probe required manual foreground attribution because
its drawable presentation timestamps were zero. Those zero timestamps are excluded,
never substituted with callback times. Continuous Oxide captures supplied nonzero
presentation timestamps. This is a practical measurement proof, not complete automated
attribution for every update or a general performance result.

The fixed image acquisition stopped after its one replacement: the second trace
lost the measurement-begin marker and 72 updates. There are zero accepted pairs.

All six fixtures build. All 36 visual checkpoints were captured and reviewed; only
the image case currently passes equivalence. The other cases are explicitly blocked
by visible baseline, newline, control or color/compositing differences. See
[visual evidence](../../benchmarks/core-visuals/visual-evidence.json) and
[the scorecard](../../benchmarks/core-comparison.md). Failed cases must not be timed
as equivalent workloads. Their correction is not silently folded into optimization.

## Color and progress configuration

The fixture now declares sRGB inputs and sRGB source-over compositing. Oxide decodes
fixture colors with `Color::from_srgba` and selects
`MetalRenderer::new_with_config_and_sdr_compositing(..., SdrCompositing::SrgbSourceOver)`.
Its host uses an sRGB-tagged `BGRA8Unorm` drawable. This makes the blend space explicit
and preserves the existing linear-light renderer default and linear `Color::rgba` API.
The option is SDR-only; HDR requests are rejected. Source shaders encode RGB before
blending; alpha and cached premultiplied layer samples are not encoded again.

Button and progress colors are explicit shared inputs. Both progress tracks use the
fixture's four-point height. The UIKit adapter scales from its actual native height
and supplies flat track/fill images to avoid implicit native gradients. Oxide retains
fractional fill widths (including subpixel values) and bounds indeterminate segments.
These changes invalidate the historical visual identity above; earlier scorecards
remain historical evidence and are not automatically promoted to accepted baselines.

## Run

From the `oxide` Cargo workspace, with an unlocked physical ProMotion iPhone:

```sh
cargo xtask ios compare-core --case images --output /absolute/new/output
```

The default selects all six cases and refuses recording while any selected case has
failed visual equivalence. Optional `--device`, `--team`, `--output`, `--case` (repeatable),
`--apps` (existing Release iphoneos products), and `--visual-evidence` select inputs.
The default evidence file is `benchmarks/core-visuals/visual-evidence.json`.
`--source-identity` prints the fixture/app source hash. Changed sources require a new
visual review; a hash change alone must never promote failed checkpoints to passed.

The runner builds/installs both apps, launches with a unique run identity, attaches
Animation Hitches plus Points of Interest to that app, waits for recording readiness,
and sends one Darwin start notification. Keep the phone foreground and untouched.
Each segment has five seconds warmup, twenty measured and five settled, at brightness
0.5 with nominal thermal state and low-power mode off. Five pairs alternate framework
order. One replacement pair per case is allowed after an acquisition failure; repeated
failure blocks that case. Trace finalization is bounded, and can add substantial time
to the thirty-second app workload. Outputs are never overwritten.

Outputs include device/source/binary identities, completion signals, raw Instruments
archives, exact XML exports, per-run distributions, attempts, paired differences and
a scorecard. Raw traces are losslessly compressed and checked before their unpacked
copies are removed. CPU/memory and cross-framework GPU/hitch costs are unavailable.
Oxide GPU diagnostics use completed-command-buffer samples. Display-link callbacks
are not presentation evidence. System swap cadence assumes an isolated foreground
workload; discrete latency requires complete source-generation attribution.

Visual checkpoints use `-oxide-core-case CASE -oxide-core-checkpoint TIME` at 0, 10,
and 19.9 seconds. The app writes `Documents/core-result.json` with checkpoint-ready
before screenshots are taken. Simulator screenshots are visual diagnostics only;
simulator presentation callbacks are omitted and simulator timing is never official.

## Retained evidence

- Feasibility traces, exports, exact probe source snapshots, executable identities and
  checksums: `/Users/victorstewart/oxide-recovery/core-suite-20260906/measurement/`.
- Fixed-protocol baseline attempts: `/Users/victorstewart/oxide-recovery/core-suite-20260906/baseline/`.
- Reviewed checkpoint PNGs: `oxide/benchmarks/core-visuals/`.

## Cleanup performed

- Removed the old feed-v1 pilot, its reducer/apps/controllers, workspace members,
  profiles and documentation. Their production host APIs remain unchanged.
- Removed embedded UIKit benchmark runtime, parked app, performance-test target,
  UIKit launch test and OxideUIKitPerf scheme; removed only their app-entry/delegate
  routing and stale XCTest detection.
- Retained the ordinary OxideHost and window screenshot smoke test, production
  injected-app host, Nametag bridges, camera/idle profiling, all renderer code,
  snapshots/goldens, engine benchmarks and standalone paired analysis.
- Retired UIKit/React comparison commands and obsolete UIKit reports. Kept standalone
  Oxide device profiling, iOS preparation, profiler summaries and test orchestration.
- The generalized Apple/Web campaigns and benchmark-spec were already absent from
  the reconciled upstream tree. The neutral Noto fixtures/licenses remain unchanged.
- Previously authorized obsolete worktrees/branches were already retired and were
  not deleted again. Other worktrees and remote branches were left alone.

Recovery source and complete reachable history were verified before deletion:
`/Users/victorstewart/oxide-recovery/core-suite-20260905/source.bundle`.
That directory's `RECOVERY.txt` records the starting checkout and earlier branch
archive. Independently used upstream product behavior is not a cleanup target.

## Verification

Both comparator apps and the ordinary host build in Release. Focused production-shell,
drawable, retained-report and reducer checks passed. The ordinary host's physical
launch and coordinate-input test passed after the phone was unlocked, but the
after-input screenshot is black. Rendering verification therefore remains pending. No hosted CI ran. Consult the scorecard
for observed baseline results and pending gates; successful compilation does not
establish visual equivalence or relative speed.
