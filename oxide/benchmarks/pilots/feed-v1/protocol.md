# Proposed Goal: One-Workload Oxide/UIKit Evidence Pilot

## Prompt

Determine whether Oxide is visually equivalent enough and display-link callback
pacing is non-inferior to both idiomatic UIKit and hand-optimized UIKit for one
fixed, production-like variable-height feed on one physical ProMotion iPhone.
Callback pacing is a scheduling signal, not proof that either framework
presented new pixels.

This is a one-workload evidence pilot, not a claim that either framework is
globally faster. Finish with a truthful result or a precise blocker in one
working day. Do not optimize Oxide during this goal.

### Trusted starting point

Start from one reviewed reconstruction commit and name whether it preserves the
0.1 public contract or is the explicit breaking-release candidate. The working
tree must be clean and `HEAD` must resolve through a named branch. Persist that
exact ref, commit object, and tree object with the hashes of both app bundles,
the compiled UI-test runner, its embedded xctest, and the reducer. Existing
comparison controllers, reports, thresholds, and
generated projects are untrusted until individually justified. Reuse a piece
only when it directly executes this workload and is smaller than replacing it.

This reconstruction is the explicit breaking-release candidate, not a claim of
0.1 source compatibility. It deliberately removes the former accessibility
surface and requires `'static` renderer encoders so an injected app frame can
own its backend encoder. The evidence manifest must identify the exact frozen
candidate commit and tree; a passing pilot does not retroactively make it a
0.1-compatible change.

All comparison code belongs in a benchmark-only target. Production Oxide,
UIKit hosts, renderer APIs, and default artifacts receive no scenario IDs,
benchmark switches, report controllers, or competitor-specific behavior.

### Mandatory production-path preflight

Before implementing a fixture, app, controller, reducer, or device runner, name
and source-audit the already-shipping Oxide app composition boundary, iOS host,
collection surface, raw-input route, scroll physics, and frame scheduler that
the treatment will exercise. A valid treatment may add only workload data,
ordinary cell composition, and benchmark observation around those production
boundaries.

Public low-level pieces are not enough by themselves. Do not invent a
benchmark-owned app host, scroll/fling implementation, frame loop, direct
draw-list runtime, or renderer submission path and label the result
`oxide-production-api`. Do not add a benchmark scenario or selector to a shared
production host to get around this rule. If the named production path does not
exist or cannot accept this workload without production architecture work,
record `blocked`, remove the surrogate, and stop before device measurement.

Amendment history:

- The production-path preflight and original frozen protocol were finalized
  after an exploratory surrogate was source-audited and rejected, but before
  any physical-device run or performance result was observed.
- Two source-identical six-run physical-device smokes then showed that XCTest
  velocity delivery was not an exact per-run ground truth: idiomatic UIKit's
  forward travel differed by `93 pt` (`6.95%`) between those smokes. Neither
  smoke produced a publication report, and no 54-run primary population had
  begun. The replicated `5%` median / `10%` confidence travel-equivalence rule
  below was frozen in a new source revision before any primary or full
  publication run.

### The only workload

Use a content-addressed fixture describing a 2,000-row variable-height feed.
Freeze identical strings, locale, font files and faces, images, colors, radii,
shadows, spacing, viewport, safe area, scale, row identities, exact row heights,
the complete row-height prefix table and content extent, initial offset, and
visible content for all three implementations.

Implement exactly three variants:

1. normal production UIKit using `UICollectionView` and ordinary UIKit text,
   image, layout, reuse, and input ownership;
2. optimized UIKit using the same visible contract, with only legitimate
   caching, invalidation, reuse, and composition tuning; and
3. the established production Oxide app/collection/scroll/host path with no
   benchmark-owned substitute for any of those layers.

Run one forward and one reverse XCTest OS-level fling from frozen start states.
Use the same gesture coordinates, duration, and velocity request. Persist actual
travel distance. The unreplicated smoke flings prove delivery, inertia, and
settlement but do not claim cross-treatment travel equivalence; that claim uses
only the balanced primary population.

Use one cache state only: fresh-process, initial-viewport-resources-warm,
offscreen-resources-cold. For each direction, launch a fresh process, load the
fixture's complete frozen row-height/prefix table, mount directly at that
direction's frozen start offset, load only naturally requested visible text and
image resources, and reach the same app-owned ready-admission boundary. Reading
the frozen geometry is workload input, not resource warming. Do not
programmatically shape, decode, render, or scroll through offscreen content.
The six smoke treatment/direction tuples take one full-canvas screenshot before
their gesture. The 54 primary tuples take no screenshots and begin the gesture
immediately after ready admission, so every
measured treatment has the same visible warmup and naturally encounters
uncached offscreen resources during travel.

Freeze that gesture before observing results: use the center x coordinate and
drag from normalized y `0.82` to `0.18` for forward or `0.18` to `0.82` for
reverse, with `0.05 s` press duration, XCTest gesture velocity raw value `2400`,
and an app-owned settle deadline of `6 s`. Each direction starts in a fresh
process with a unique completion nonce. The app synchronizes and closes its
record, marks the run finished, and only then emits that nonce's Darwin
completion notification; the controller waits at most `7 s` and never guesses
settlement with a fixed sleep. The maximum content extent must agree within one
physical pixel, each frozen start must agree within one physical pixel, and
every run must travel at least `524 pt`, the direct `540.16 pt` drag span minus
the frozen `16 pt` delivery tolerance. This floor proves nondegenerate delivery
only. Every treatment must also persist `inertia_observed: true` from its actual
transition into inertial scrolling; travel or duration can never be used to
infer inertia. UIKit uses the collection view's deceleration-entry delegate
callback, and Oxide uses its production scroll surface's transition into
inertial motion.

For full-population travel equivalence, pair each optimized UIKit or Oxide
primary run with idiomatic UIKit from the same session, pair index, and
direction. For each treatment and direction, the absolute median of those nine
relative travel deltas must be at most `5%`, and its deterministic 95% cluster
bootstrap interval must remain inside `[-10%, +10%]`. This replicated gate
rejects a systematic workload mismatch without treating one XCTest velocity
delivery as exact ground truth.

The completion nonce is 1 through 128 ASCII alphanumeric-or-hyphen bytes. Both
apps must reject the same invalid values before deriving a file or notification
name.

Do not add another screen, count, style, cache mode, launch row, or mutation row.

### Visual admission before classification

The two frozen renderer states are `top` at content offset `0` and `bottom` at
the exact maximum content offset. In the six smoke tuples, capture each state
after layout settles and before its outbound gesture; a reverse-session process
mounts directly at `bottom`. Those captures globally gate the unchanged,
manifest-hashed app builds used by the smoke and primary populations. The
maximum offset and each captured offset must agree across treatments within one
physical pixel.

The controller may collect raw timing samples in the same bounded run because
settled travel and submitted geometry exist only after a gesture. Those samples
remain quarantined: a blocked report must contain no aggregate or per-run
timing rows, and the reducer must emit `blocked` unless the
two frozen renderer states plus both gesture directions pass all applicable
checks:

- fixture, state, component count, visible content, viewport, scale, fonts,
  assets, and geometry identities are exact;
- each smoke attachment is the nonce-derived full `1320 x 2868` XCUIScreen
  capture for its implementation/state/build;
- manifest-owned component and clipping bounds match exactly or within a
  predeclared one-physical-pixel raster tolerance;
- a frozen full-frame perceptual comparison plus a localized tile guard passes;
  and
- hostile mutations of the real idiomatic-UIKit top capture prove the gate
  rejects a missing row, sparse missing caption, wrong checker variant, missing
  image, wrong color, shifted image, half-sized image, bad clipping, and a
  localized corrupt tile.

The frozen raster gate operates only on the `390 x 844 pt`, `3x` feed surface,
not device chrome. Cross-treatment admission uses idiomatic UIKit as the
reference, full-surface luma SSIM
`>= 0.96`, `48 x 48` physical-pixel tiles, and worst-tile RGB mean absolute
channel error `<= 18`. Observed component rectangles use content-space physical
pixels; clip rectangles use viewport-space physical pixels. Both tolerate at
most one physical pixel per edge. These values must not change after a device
result is observed.

Keep exact cross-framework RGB differences in the report as diagnostics, but
do not require universal byte equality. UIKit/Core Text and Oxide do not share
one rasterizer, and cloning UIKit edge pixels is not a product requirement.
Never loosen the visual gate after seeing performance results.

### Measurements that are allowed

Use the same app-owned display-link timestamp logger and the same frame-deadline
formula in all three variants. For each gesture/session persist raw frame
intervals, achieved cadence, p50/p95/p99/peak, missed-callback-deadline count,
callback-gap hitch ratio, gesture duration, and physical travel distance. Treat
p99 as descriptive at this bounded population size. Name every one of these
values as display-link callback pacing. Do not call them presented-frame
pacing, visible-frame pacing, or rendered FPS, because callback delivery does
not prove new pixels reached the display.

Each treatment preallocates the same 1,024-sample cap. Capacity exhaustion
invalidates the run; no treatment may truncate its timing population.

Persist an aligned `targetTimestamp - timestamp` period for every interval.
For interval `i` and target period `p`, missed callback deadlines are
`max(round(i / p) - 1, 0)` and callback-gap hitch excess is `max(i - p, 0)`.
Report the missed-callback-deadline ratio against total expected callbacks and
callback-gap hitch excess in milliseconds per elapsed second. Never infer a
fixed 60 Hz or 120 Hz deadline.

The admitted full JSON and Markdown reports contain one deterministic per-run
row for the 54 primary gestures, including identity/order, direction, duration, signed
and absolute travel, observed inertia, both environment-transition counts,
callback count and cadence,
p50/p95/p99/peak, missed and expected callback counts, missed ratio, hitch
excess, and target-period admission ratio. Re-running the reducer over its own
exact output paths must be byte-identical; those two output files are excluded
from input discovery and byte accounting.

Record main-thread CPU time, process CPU, and resident memory only when the same
collector and boundary work for all variants. Record direct GPU time only when
one existing collector observes an equivalent boundary on both Oxide and
UIKit. Otherwise write `missing`; never compare Oxide command-buffer GPU time
with a different UIKit/Core Animation scope.

Input receipt may be recorded as diagnostic attribution. Do not publish
`input-to-visible`, `presented`, `displayed`, or photon latency: the current
app-only paths do not expose one symmetric generation-bearing presentation
boundary for custom Metal and UIKit. `CADisplayLink` timestamps and drawable
deadlines are not presentation proof. A future latency study requires its own
authorized measurement design.

Before and after every session, record `ProcessInfo.thermalState`, Low Power
Mode, `UIScreen.maximumFramesPerSecond`, the configured display-link range, and
the observed target-period distribution. Admit only sessions that start and end
at thermal state `nominal`, keep Low Power Mode off, run on a display reporting
at least 120 Hz, and keep at least 95% of active-gesture target periods between
7.5 and 9.2 ms. Any state transition or failed bound blocks that session; do not
silently substitute a 60 Hz run. Install transition observers before readiness,
snapshot their monotonic counters immediately before the initial environment
endpoint query that admits readiness, and persist `thermal_state_change_count`
and `low_power_mode_change_count` through completion. Both counts must be zero;
equal endpoint values cannot hide a transient change.

### Population and decision

Use a balanced order so each implementation runs first, second, and third once
per block. Run one smoke prefix, then exactly three sessions of three
forward/reverse pairs per implementation. Those nine paired clusters are the
maximum population; do not add a second block after observing the result.

Bootstrap by session/gesture pair, not by pretending every frame is an
independent trial. Compare Oxide separately with idiomatic UIKit and optimized
UIKit:

- `faster`: Oxide callback-interval p50 and p95 are lower and the 95% interval
  excludes zero in Oxide's favor;
- `non-inferior`: the upper 95% bound is within +5% and the absolute
  missed-callback-deadline and callback-hitch guardrails pass;
- `slower`: the lower 95% bound exceeds +5% or an absolute guardrail fails;
- `inconclusive`: neither decision is supported by the frozen nine-cluster population;
- `blocked`: build, workload, travel, visual, thermal, or collector identity is
  invalid.

Use `100,000` deterministic cluster bootstrap resamples with seed
`0x6f786964655f7631` for both the travel-equivalence and callback-pacing
intervals. The pacing decision interval is the 95% percentile interval of the
median pair-level relative p95 delta, where positive means Oxide is slower.
The missed-callback-deadline guardrail requires Oxide to be at most `0.5`
percentage points above its comparator and at most `2%` absolute. The
callback-hitch guardrail requires Oxide to be at most `2 ms/s` above its
comparator and at most `10 ms/s` absolute. `faster` additionally requires both
aggregate p50 and p95 to be lower; `non-inferior` requires the p95 interval upper
bound to be at most `+5%`; `slower` applies when its lower bound exceeds `+5%` or
either callback guardrail fails. Otherwise the result is `inconclusive`.

A truthful `slower`, `inconclusive`, or `blocked` result completes this goal. It
does not authorize more sampling, a new profiler, or a production rewrite.

### Hard limits

- one working day;
- 20 minutes of official physical-device runtime;
- 10 minutes maximum per implementation across the primary population;
- 512 MiB retained result root and 4 GiB external build root;
- one bounded app/controller process at a time;
- two identical environment/controller failures block the row;
- no simulator numbers in the comparison; and
- mandatory end-of-run cleanup of apps, processes, file descriptors, temporary
  screenshots, result bundles, trace scratch, and build artifacts outside the
  retained caps.

The runner must prove Xcode test success, the exact verified attachment count,
uninstallation of both measured apps and the resolved
`com.oxide.feed-v1.controller.xctrunner`, and absence of its exact runner
process. The XCTest export manifest is a bijection over exactly one nonce-derived
PNG for each of the six smoke capture tuples. Full and smoke modes therefore
both retain exactly six attachments; primary tuples retain none. App records for all
six smoke and 54 primary runs must come from the matching retrieved app
container and canonical nonce-derived filename. Exactly one cleanup proof and
one evidence manifest may authorize a report, both at their frozen `raw/`
paths.
The cleanup proof may assert only state established before reduction, including
that no reducer executable is retained inside the evidence root. The runner
removes the external temporary reducer after reduction and fails its overall
exit status if that final cleanup fails; the report must not claim that this
post-reduction action already happened.
The controller persists its total runtime and separate primary-population
runtime for all three treatments in its own app container. The reducer enforces
the 20-minute total and each 10-minute treatment cap from that signed-controller
record; the runner separately fails an overlong Xcode test phase and proves the
source snapshot is still clean and unchanged after export.

### Deliverable

First commit the reviewed source on the named clean branch. The device runner
binds evidence to that immutable source commit/tree; a later child commit may
add the compact result package without pretending those result bytes existed in
the source commit. The package contains the fixture and source/build/font/asset
hashes; audited source for all three variants; the
visual gate code/hash, adversarial results, captures, and bounded diffs; raw
session/gesture/callback samples; collector and device state; p50/p95/p99/peak
and confidence intervals; the four treatment/direction travel-equivalence
medians, confidence intervals, frozen margins, and decisions; separate Oxide-versus-idiomatic and
Oxide-versus-optimized classifications; all `missing` fields; and cleanup
proof.

End with one decision: whether this single workload is trustworthy enough to
justify a separately authorized second workload. Do not start that second
workload in this goal.

### Explicit non-goals

No global framework verdict; no universal pixel identity; no launch benchmark;
no text editing or keyboard; no Web, macOS, Android, camera, energy, external sensor,
photon-latency, exhaustive matrix, density/calibration campaign, general
benchmark controller, new profiler, or production optimization.
