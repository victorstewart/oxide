# C54 iOS demand-driven display link

## Decision

Accepted against C53 parent `5576b3ca9abbe5fa436d2f4b761e9f82205506f5`.

The iOS host now pauses its foreground `CADisplayLink` after Oxide is clean and
settled. Dirty sources publish a lock-free generation and coalesce a main-queue
wake, so a truly idle host performs neither an Objective-C display callback nor
a Rust app-state lock/render-plan check. The generation captured during frame
preparation is acknowledged only after successful submission, preserving a wake
that races with an in-flight frame.

Camera frame-driven mode and the existing background, resign-active, foreground,
and disconnect behavior remain separate and intact. Wake counters, idle pauses,
and missed-wakeup detection are visible through the debug ABI.

## Physical-device evidence

All device measurements used the attached iPhone 17 Pro Max (`iPhone18,2`, iOS
26.5.1) with Xcode 26.6. Five interleaved fresh launches per side measured the
settled static foreground window:

| Metric per ~100 ms window | Parent p50/p95/p99/peak | Candidate p50/p95/p99/peak |
| --- | ---: | ---: |
| `CADisplayLink` callbacks | 6 / 6 / 6 / 6 | 0 / 0 / 0 / 0 |
| Rust render-plan checks | 6 / 6 / 6 / 6 | 0 / 0 / 0 / 0 |
| drawables / command buffers / submissions | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 |
| missed wakeups | unavailable in parent ABI | 0 / 0 / 0 / 0 |

That is a deterministic 100% reduction in repeated idle callbacks and Rust
checks, with 5/5 candidate wins and no rendering work in either tree.

Two independent button-response traces retained 256 event-to-visible samples
per side. The candidate p50 was effectively neutral in both pairs; p99 and peak
were lower in both. Candidate p95 lost once (19.517 versus 17.014 ms) and won on
the repeat (17.092 versus 18.834 ms), so there was no repeatable tail regression.
Repeated no-trace button and animation launches also reached `OXIDE_COMPLETE`.

The real-camera guard processed 120 NV12 1280x720 frames at 30 fps through the
Oxide-owned Metal preview on both trees with zero missed frames. Candidate versus
parent custom-preview GPU p99 was 1.354 versus 1.448 ms; host-tick p99 was 0.341
versus 0.311 ms. This is a correctness/regression guard for C54, not a camera
architecture decision; the broader custom-preview/UIKit comparison remains C61.

Automatic `Power Profiler` recording was attempted. Xcode created an empty trace
and export failed with `Document Missing Template Error`; no energy number or
proxy is claimed. Metal System Trace and Oxide's in-app command-buffer timings
remained available.

The compact measured rows are preserved in
[`physical-device-summary.json`](physical-device-summary.json).

## Verification

- `cargo test --locked -j12 -p oxide-host-ios --tests --quiet`
- `cargo test --locked -j12 -p xtask --test xtask_tests --quiet`
- `cargo build --locked -j12 --target aarch64-apple-ios -p oxide-host-ios`
- Five interleaved physical-device static-idle launches per side
- Two physical-device button traces per side, plus repeated button/animation runs
- One physical-device 120-frame real-camera run per side

## Cleanup

The product path contains no polling timer, recognizer-owned product behavior,
or benchmark-only wake bypass. The reporting changes terminate the launched app
before joining the console stream, preventing the harness itself from waiting
forever after a completed workload.
