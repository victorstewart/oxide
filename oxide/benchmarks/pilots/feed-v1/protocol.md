# Final Goal: `feed-v1` Visual and Callback-Pacing Study

## Objective

On one physical arm64 ProMotion iPhone at native refresh, determine whether one
frozen production-path Oxide feed:

1. passes a predeclared visual-equivalence gate against the same feed in UIKit;
2. has display-link callback pacing that is faster, non-inferior, slower, or
   inconclusive relative to idiomatic UIKit; and
3. has the same classification relative to reviewed optimized UIKit.

This is evidence for one workload on one device, not a global framework verdict.
A truthful `slower`, `inconclusive`, or `blocked` result completes the goal.
Do not optimize any implementation, change a threshold, add samples, or repair
the workload after the publication source is frozen.
This study is intentionally partial relative to the broader scroll-performance
contract: canonical Oxide/UIKit device baselines remain separate verification
artifacts, not extra app surfaces or publication pages.

## Non-negotiable scope

- Accessibility is outside this study and product contract. Do not add
  accessibility APIs, roles, labels, actions, tests, gates, or claims.
- Do not require or claim universal pixel identity. Exact RGB differences remain
  diagnostics; the publication claim is only that the frozen geometry and
  perceptual gates passed.
- Use exactly two app surfaces: one shared UIKit feed whose fresh-process
  configuration selects idiomatic or optimized implementation, and one Oxide
  feed. Produce one logical aggregate publication report. Add no dashboard,
  debug overlay, additional screen, second workload, style matrix, optimization
  matrix, mutation flow, or launch benchmark.
- Run only a Release arm64 device build on the named physical ProMotion iPhone.
  Simulator, x86_64, 60 Hz, desktop, and mixed-device numbers are inadmissible.
- The publication population is fixed below. There is no optional second block,
  bootstrap, Monte Carlo method, resampling, or post-result retry population.
- Any optimization trial must finish, be reviewed, and have all trial machinery
  removed before the publication commit is frozen.
- Every retained production optimization requires isolated, predeclared,
  matched A/B evidence before source freeze; otherwise restore its control. The
  publication population may never double as optimization-selection evidence.

## Frozen feed and implementations

Use the content-addressed `feed-v1` fixture:

- 2,000 deterministic variable-height rows;
- canonical SHA-256
  `a1de9b4a914734fe21d21e9b6f8a9b61970f7e22e0fa4ef0103031e399881473`;
- canonical byte count `717745`;
- exact row identities, strings, heights, 2,001-entry height prefix, checker
  bytes, font files and faces, colors, radii, shadow, clipping, and spacing;
- `390 x 844 pt` feed surface at `3x`, zero safe-area inset;
- content extent `237460 pt`, top offset `0 pt`, and bottom offset
  `236616 pt`.

Implement exactly:

1. idiomatic UIKit: ordinary `UICollectionView`, UIKit labels/images/layout,
   reuse, input, scrolling, and deceleration;
2. optimized UIKit: the identical visible contract using only the reviewed
   production-selected invalidation, reuse, and composition choices;
3. Oxide: ordinary production app composition, collection, raw-input, scrolling,
   inertia, host, scheduler, and renderer paths.

The three variants must derive the same visible rows on demand from the compact
recipe. Each fresh process may warm only the initial viewport naturally;
offscreen text and images remain cold. No implementation may pre-scroll,
pre-shape, pre-decode, pre-render, or retain a pre-expanded row-string table.

Before building the pilot, source-audit and name every Oxide production boundary
above. Benchmark code may supply workload data and observation only. If a
production boundary is absent, or the workload requires a benchmark-owned host,
scroll implementation, frame loop, draw path, or renderer submission path,
record `blocked`, remove the surrogate, and stop.

## Immutable source and build

Freeze one reviewed commit on a named clean branch before any official launch.
Record the branch, commit object, tree object, clean status, toolchain and SDK,
build settings, and hashes of:

- all treatment, controller, reducer, visual-gate, contract, font, and asset
  sources;
- the UIKit and Oxide app bundles;
- the UI-test runner and embedded xctest; and
- the reducer executable.

Use those exact binaries for every smoke and primary launch. Verify that the
source tree is clean and byte-identical after export. Production crates and
hosts must contain no benchmark scenario IDs, competitor branches, report
controllers, or publication-only switches.

Every record binds that frozen source branch, commit, and tree. Generated
reports land only in evidence-only child commits whose parent is the source
revision; those children may not change source, build, fixture, controller, or
reducer files.

The run manifest must bind CoreDevice ID
`1DEDF2A3-EC8E-5FCC-A437-8BD3A6F3D659`, hardware identifier
`00008150-001529C434F8401C`, model, OS version/build, maximum refresh rate,
Xcode build, selected iPhoneOS SDK, Rust toolchain, and code-signing identity.
Reject any mismatch before the first launch.

## Fixed interaction state

Every launch is a fresh process with a unique validated nonce. Forward starts at
top; reverse starts at bottom. After the identical app-owned ready boundary:

- forward drags at center x from normalized y `0.82` to `0.18`;
- reverse drags from `0.18` to `0.82`;
- press duration is `0.05 s`;
- XCTest velocity raw value is `2400`;
- settlement deadline is `6 s`, followed by a `7 s` controller timeout.

The app closes its nonce-derived record before posting completion. Every run
must travel at least `524 pt` in the requested direction and must record the
real transition into inertia. A fixed sleep, travel distance, or duration may
not infer settlement or inertia.

Gestures must enter through public XCTest/XCUIElement OS-level delivery on the
physical phone. Reject environment-triggered automation, direct app-event
injection, synthetic state mutation, or UIKit-owned Oxide scrolling/inertia.

## Visual admission

The official run begins with exactly six smoke launches: three treatments times
forward/top and reverse/bottom. Capture two immediate nonce-derived full-screen
`1320 x 2868` PNGs before each smoke gesture; the first is the admission image
and the second proves same-renderer repeatability. Crop only the frozen
`1170 x 2532` physical-pixel feed surface at the exact manifest-bound pixel
origin. Normalize orientation, flatten alpha over the frozen background, and
convert without resizing to opaque sRGB8 before comparison.

Before timing may be published, all six smoke tuples must pass:

- exact fixture, state, visible-content, component-count, font, asset, viewport,
  scale, and build identities;
- maximum extent and captured offsets within one physical pixel;
- manifest component and clip edges within one physical pixel;
- full-surface luma SSIM `>= 0.96` against idiomatic UIKit; and
- worst `48 x 48` physical-pixel tile RGB mean absolute channel error `<= 18`.

“Component count” means frozen manifest-visible components, not equality
between UIKit view internals and Oxide node internals. Each immediate repeat
capture must independently pass the same gate, and the two normalized crops
must be byte-identical.

The frozen gate must have deterministic offline tests proving rejection of
missing content, wrong text/image/color, shifted or half-sized geometry, bad
clipping, and localized tile corruption. Never tune the gate after observing a
device result. If admission fails, the aggregate report is `blocked` and must
not expose per-run or aggregate pacing results.

## Exact publication population

After the six-run smoke prefix, run exactly 54 primary fresh-process launches:

- sessions `0...2`;
- pair indices `0...2` per session;
- treatments ordered by rotation
  `(session + pair) % 3` over
  `[uikit-idiomatic, uikit-optimized, oxide]`; and
- forward then reverse for each treatment.

Thus every treatment has exactly 18 primary runs and occupies every order
position equally. Primary launches take no screenshots. No invalid launch is
silently replaced. Nine matched clusters are the smallest population that both
balances all three treatment orders and yields a greater-than-95% exact median
interval after excluding one extreme at each tail: ranks 2 and 8 cover
`96.09375%`; eight clusters would cover only `92.96875%`.

An admitted, fully completed run contains exactly 60 launches. On the first
blocker, stop without replacement or continuation. A blocked report contains
provenance, collected admission evidence, the blocker, and cleanup only; the
54-row and raw-sample requirements apply only to an admitted complete
population. The 20-minute bound is an operational runaway/thermal fuse, not a
scientific classification threshold.

Admit only a device reporting at least 120 Hz, Low Power Mode off, and thermal
state nominal before and after every session. Thermal and Low Power Mode
transition counters must remain zero. In every primary run, at least 95% of
active-gesture target periods must lie in `7.5...9.2 ms`; never substitute a
60 Hz result.

## Symmetric admissible measurements

Use the same app-owned display-link timestamp logger, 1,024-sample capacity, and
deadline formula for all three treatments. Persist raw callback intervals and
aligned `targetTimestamp - timestamp` periods. For interval `i` and target
period `p`:

- missed callback deadlines are `max(round(i / p) - 1, 0)`;
- hitch excess is `max(i - p, 0)`.

For every primary run report callback count, achieved cadence, interval
p50/p95/p99/peak, missed count and ratio, hitch excess in milliseconds per
elapsed second, gesture duration, signed/absolute travel, and inertia.

Name these values only as display-link callback pacing. They are not presented
frames, rendered FPS, displayed pixels, or photon latency. Do not publish
input-to-visible latency. CPU, main-thread, resident-memory, GPU, or energy
figures are allowed only when one collector observes the same boundary for all
three treatments; otherwise emit `missing`. Never compare Oxide
command-buffer timing with a UIKit/Core Animation scope.

Use nearest-rank quantiles with one-based rank `ceil(q × n)`. Each
treatment/session/pair is one independent cluster. Concatenate the two
directional runs only to compute that cluster's descriptive p50 and p95, then
use the median of the nine cluster p50s and p95s as the treatment aggregates.
Only the nine matched cluster deltas enter interval classification; raw callback
intervals are never independent trials. Aggregate missed-deadline ratio as
total missed deadlines divided by total eligible deadlines, and aggregate
hitch excess as total excess milliseconds divided by total elapsed seconds.

Compute every callback p50, p95, and p99 with the one-based nearest-rank rule
`rank = ceil(q * n)`. For each treatment/session/pair cluster, concatenate only
its forward and reverse intervals and compute its descriptive p50 and p95. The
treatment p50 and p95 are the medians of the nine corresponding cluster values,
not quantiles over pooled callback intervals.

The admitted canonical JSON contains one deterministic row for each of the 54
primary gestures, including its raw callback timestamp/targetTimestamp samples,
identity/order, direction, duration, signed and absolute travel, observed
inertia, both environment-transition counts, callback count and cadence,
p50/p95/p99/peak, missed and expected callback counts, missed ratio, hitch
excess, and target-period admission ratio. Markdown contains only aggregate
admission, travel, treatment, comparator, policy, cleanup, and limitation
summaries plus the canonical JSON path and SHA-256; it never duplicates raw
sample arrays or the 54-row table. Re-running the reducer over its own exact
output paths must be byte-identical; those two output files are excluded from
input discovery and byte accounting.

## Exact travel and pacing analysis

For travel admission, pair optimized UIKit and Oxide separately with idiomatic
UIKit by session, pair index, and direction. Each treatment/direction therefore
has nine relative travel deltas. Sort once. Require:

- absolute median at most `5%`; and
- exact two-sided median interval, one-based ranks 2 and 8, wholly inside
  `[-10%, +10%]`.

For pacing, combine forward and reverse callback intervals within each
treatment/session/pair and compute that cluster's p95. For each comparator,
form nine relative deltas `Oxide / comparator - 1`, where positive means
Oxide is slower. Sort once; the median is rank 5 and the exact two-sided
96.09375%-coverage interval is ranks 2 and 8.

Aggregate p50/p95 use the nine clusters; guardrails use their summed 18 primary
runs per treatment. Oxide's
missed-deadline ratio must be at most the comparator plus `0.5` percentage
points and at most `2%` absolute. Oxide hitch excess must be at most the
comparator plus `2 ms/s` and at most `10 ms/s` absolute.

Classify Oxide separately against each UIKit comparator, in this precedence:

- `blocked`: any source, build, fixture, visual, travel, inertia, thermal,
  refresh, population, collector, record, or cleanup admission is invalid;
- `slower`: interval rank 2 exceeds `+5%`, or either pacing guardrail fails;
- `faster`: aggregate p50 and p95 are both lower, interval rank 8 is below
  zero, and both guardrails pass;
- `non-inferior`: interval rank 8 is at most `+5%` and both guardrails pass;
- `inconclusive`: none of the above.

Do not pool frames as independent trials and do not invent confidence from
resampling. Report both comparator classifications even when they differ.

## One aggregate publication report

Retain one immutable evidence root and produce one logical aggregate report:
canonical JSON plus a deterministic Markdown rendering of that same JSON. For
an admitted complete population, it must contain:

- provenance and admission decisions;
- all 54 deterministic primary rows and the raw callback samples they summarize
  in canonical JSON;
- the four directional travel analyses;
- one Oxide-versus-idiomatic row and one Oxide-versus-optimized row;
- every threshold, exact rank interval, missing field, and blocker; and
- cleanup proof.

Markdown is the sole compact human-facing results page: admissions, the two
comparison rows, travel and pacing summaries, thresholds, missing fields,
blockers, cleanup, and the canonical JSON path/hash. It is deterministically
derived from JSON but does not duplicate raw sample arrays. Retain a sorted
evidence manifest with relative path, byte count, and SHA-256 for every raw
record and screenshot.

Reducer reruns over the same evidence must be byte-identical, excluding its own
two output paths from discovery. Do not create per-treatment publication
reports or additional result screens.

## Cleanup and terminal behavior

At the end, uninstall the UIKit app, Oxide app, and resolved controller runner;
prove their exact processes are gone; close observers and files; and remove
generated projects, derived data, temporary screenshots, result bundles, trace
scratch, and external reducer/build artifacts not retained in the bounded
evidence root. Record only cleanup facts already observed; a report may not
claim post-reduction cleanup before it occurs.

Retain a lexicographically sorted canonical-JSON inventory of every file below
`raw/`, with its evidence-root-relative path, exact byte count, and SHA-256.
The terminal decision reports the idiomatic and optimized UIKit classifications
in fixed order without collapsing a mixed pair into one global verdict. Any
admission failure reports only `blocked` and no timing classification.

All build, controller, and evidence roots must be task-owned and external to the
source tree. Before the official run, freeze maximum evidence bytes, file count,
per-file bytes, and XCTest result-bundle bytes from a reviewed dry run plus
bounded headroom. Check those fuses before each launch and stop on breach.

If any source change or optimization becomes desirable after source freeze,
finish this goal with the observed classification or `blocked`. A future
change requires a separately authorized A/B proof and a completely new frozen
publication run.

## Explicit non-goals

No global performance verdict, universal pixel identity, accessibility,
additional workload, launch timing, text editing, keyboard, Web, macOS,
Android, camera, energy proxy, external sensor, input-to-visible/photon latency,
new profiler, exhaustive matrix, production optimization, or automatic
follow-on work.
