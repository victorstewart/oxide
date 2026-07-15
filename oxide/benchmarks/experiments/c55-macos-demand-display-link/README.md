# C55 macOS demand-driven display scheduling

## Decision

Accepted against C54 parent `8f1135fd5c69c16b0d5ae0957e6e383802aa4c4f`.

The bottleneck was CPU scheduling in the macOS host: a settled static scene kept
entering `CADisplayLink`, locking Rust app state, and checking a render plan even
though it submitted nothing. The candidate makes frame demand explicit, pauses
modern and legacy display links once damage and settlement are clean, and
publishes dirty generations without taking the app-state mutex merely to wake.
The correctness risk was losing a publication at the pause boundary or delaying
the first visible response after a suspended wake.

The retained wake bridge uses one process-lifetime main-run-loop source. Its
callback renders the first suspended wake immediately after the Rust publisher
has released app state. A reusable timer delays only the required settlement
frame by one target refresh period, preventing an immediate second drawable
from expanding the layer pool. Neither operation allocates per wake.

## A/B protocol

- Hardware: Mac14,6, Apple M2 Max with 38-core GPU and 96 GB RAM; built-in
  3456x2234 ProMotion display at native adaptive refresh (about 120 Hz).
- OS/toolchain: macOS 26.5.2 (25F84), Xcode 26.6 (17F113), Rust/Cargo 1.89.0.
- Power: AC power, battery charged at 100%, and no recorded thermal,
  performance, or CPU-power warning.
- Build: `cargo build --locked -j12 -p oxide-macos-app --features host-testing --release`,
  packaged and ad-hoc signed as separate parent/candidate `.app` bundles.
- Candidate implementation-source tree: `404863c5dcf3485e827d5215b5081c13c1994559`.
- Shared instrumentation patch SHA-256:
  `5ad3d3edecb65dc6911ff442aa2dfef60033fc376071ada8fda505c2dbe64fa6`.
- Parent/candidate executable SHA-256:
  `7ab8b4bbd86fbeaa2564a6bfef22fae290bdac197636aab1811224d9b1b1a4f9` /
  `d41e7245603b85e32be0c80625f2e7e0a511188d636247968d163714348cc1fe`.
- Every launch completed a fresh-process warmup and one second of continuously
  active foreground state before measurement. Unrelated native input was
  quarantined in the `host-testing` build; foreground loss invalidated a row.
- Fifteen fixed-seed (`55`) pairs used exact order:
  `BA AB AB BA AB BA BA AB BA AB AB BA AB BA BA`.
- Idle measured 525 ms after a static Text Layout scene settled. Wake measured
  64 independently scheduled Rust pointer publications per launch, retaining
  960 event-to-successful-Metal-submission samples per side.
- The shared paired analyzer used 100,000 fixed-seed bootstrap resamples.

## Accepted results

| Metric | Parent p50 / p95 / p99 / peak | Candidate p50 / p95 / p99 / peak | Paired decision |
| --- | ---: | ---: | --- |
| Idle display callbacks / 525 ms | 63 / 63 / 63 / 63 | 0 / 0 / 0 / 0 | 100.000% reduction, CI 100.000%..100.000%, 15/15 wins |
| Idle process CPU (us) | 5,981 / 7,383 / 8,178 / 8,377 | 367 / 595 / 625 / 633 | 93.101% reduction, CI 92.696%..94.096%, 15/15 wins |
| Pointer-to-submit latency (ms) | 3.595 / 4.986 / 5.444 / 9.308 | 0.436 / 0.672 / 0.981 / 9.057 | 86.509% reduction, CI 83.916%..90.287%, 15/15 wins |
| CPU for 64 wakes (us) | 194,258 / 218,900 / 219,012 / 219,040 | 196,404 / 210,797 / 211,520 / 211,701 | accepted no-material-regression; paired median 2.555%, CI -3.991%..6.565%, 10/15 wins |
| Idle resident bytes | 71,615,328 / 71,838,126 / 71,856,476 / 71,861,064 | 71,615,304 / 71,777,506 / 71,805,031 / 71,811,912 | accepted no-material-regression |
| Wake resident bytes | 73,827,144 / 74,122,063 / 74,122,077 / 74,122,080 | 73,876,296 / 74,241,659 / 74,342,585 / 74,367,816 | accepted no-material-regression |

Each 64-event launch made 127 successful submissions on both sides. Parent
launches executed 192-193 display callbacks and Rust demand checks; candidate
launches executed exactly 63 display callbacks and 127 checks because its 64
first-response frames came from the run-loop source. All 1,920 retained event
samples completed, every row reported zero missed wakeups, and no submission was
lost. Process CPU is the declared energy proxy; no direct energy number is
claimed.

Fresh parent and candidate foreground windows were captured at the same 1600x1264
physical dimensions after the static scene settled. The PNGs are byte-identical:
0 differing pixels, maximum channel error 0, MSE 0, and SHA-256
`031b5c4036d05b2c7ea8e9246bdc389bc5fbfcdcdeebe4e01ce612c7779f1d40`
on both sides. See [`parent-static-capture.png`](parent-static-capture.png) and
[`candidate-static-capture.png`](candidate-static-capture.png).

The complete accepted raw-pair reports are:

- [`accepted-idle-callback-report.json`](accepted-idle-callback-report.json)
- [`accepted-idle-cpu-report.json`](accepted-idle-cpu-report.json)
- [`accepted-idle-memory-report.json`](accepted-idle-memory-report.json)
- [`accepted-wake-latency-report.json`](accepted-wake-latency-report.json)
- [`accepted-wake-cpu-report.json`](accepted-wake-cpu-report.json)
- [`accepted-wake-memory-report.json`](accepted-wake-memory-report.json)

## Rejected alternatives

The first candidate only unpaused the display link. Idle callbacks fell to zero,
but the first event after suspension could wait nearly another display period:
candidate wake p99/peak rose from 5.510/5.674 ms to 7.939/9.460 ms, and wake CPU
p99/peak also failed. That design was rejected, not relabeled as a win.

Immediate wake rendering removed that tail, but immediately resuming the link
for settlement let two drawable-backed frames overlap. The short wake workload's
resident p50 rose from 73,925,448 to 75,449,184 bytes and p95 from 74,734,858 to
78,167,297 bytes. The retained timer delays only settlement, restoring neutral
memory without sacrificing the visible-response win.

Rejected evidence is preserved in:

- [`rejected-unpause-only-wake-latency-report.json`](rejected-unpause-only-wake-latency-report.json)
- [`rejected-unpause-only-wake-cpu-report.json`](rejected-unpause-only-wake-cpu-report.json)
- [`rejected-overlapping-settlement-memory-report.json`](rejected-overlapping-settlement-memory-report.json)

## Verification

- The headless scheduler settles a static scene, retains 256 rapid redraw
  generations, and proves animation demand remains renderable.
- Source contracts cover raw input, explicit redraw, lifecycle, memory pressure,
  async/timer publication through the redraw bridge, resize, modern/legacy pause,
  foreground ownership, and the allocation-free run-loop wake path.
- The real signed foreground app proves actual AppKit display suspension,
  pointer wake latency, successful Metal submissions, resident memory, and
  missed-wakeup accounting.
- Focused host tests, production and `host-testing` release builds, xtask
  manifest tests, and final diff checks are recorded in the commit proof.

## Cleanup

Benchmark counters, foreground input quarantine, and the process harness compile
only with `host-testing`. The release path retains no polling timer, per-wake
allocation, test-only wake bypass, or redundant modern pre-render Rust lock.
