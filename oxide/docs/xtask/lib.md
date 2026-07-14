# xtask::lib

## Intention and purpose

`xtask` owns the iOS-side build, benchmark, trace, and report plumbing for Oxide. In the performance workflow, it is the adapter that turns XCTest results, device-console exports, and Instruments traces into persisted physical-device Oxide and UIKit reports.

## Relation to the rest of the code

- Upstream callers:
  - `cargo run -p xtask -- ios device-perf ...`
  - `cargo run -p xtask -- ios oxide-device-perf ...`
  - CI jobs and local `Justfile` shortcuts.
- Downstream dependencies:
  - `oxide/host/ios-app/App/OxideHostPerfTests` provides the XCTest/UIKit workload harness.
  - `oxide/host/ios-app/App/OxidePerfParkedApp.swift` provides the parked Oxide host path that can run the in-process Rust perf suite on the phone and emit the JSON payload over the device console.
  - `oxide/benchmarks/oxide-device/*.json|*.md` are the persisted Oxide device outputs.
  - `oxide/benchmarks/uikit-device/*.json|*.md` are the persisted official outputs.
  - Xcode, `xcodebuild`, `xctrace`, `devicectl`, and `.xcresult` exports provide the raw measurement inputs.

## Entry points list

- `xtask::run() -> anyhow::Result<()>`
  - Dispatches command-line tasks, including the official iOS device perf flow and the Oxide device-report flow.
  - Main callers: `oxide/xtask/src/main.rs`.
- `xtask::run_cli(args: &[String]) -> anyhow::Result<()>`
  - Dispatches explicit command arguments for tests and local tooling, including `experiments check`.
  - Main callers: `oxide/xtask/src/main.rs` and xtask CLI tests.
- `xtask::check_experiment_manifest_text(text: &str, today: &str) -> anyhow::Result<ExperimentCheckSummary>`
  - Validates `perf-experiments.toml`, rejects expired undecided experiments, requires undecided alternatives to name a `perf-ab` gate, and requires accepted/rejected experiments to carry proof and cleanup notes.
  - Main callers: `cargo xtask experiments check` and xtask tests.
- `xtask::parse_uikit_report_json(text: &str) -> anyhow::Result<UIKitPerfReport>`
  - Converts exported XCTest metric JSON into the UIKit report schema used by tests and any non-official local debug tools.
  - Main callers: tests.
- `xtask::extract_oxide_device_report_json(stdout: &str) -> anyhow::Result<String>`
  - Reconstructs the base64-chunked Oxide device report payload from the parked app's `devicectl --console` output.
  - Main callers: Oxide device perf flow and tests.
- `xtask::compare_uikit_reports(current: &UIKitPerfReport, baseline: &UIKitPerfReport) -> UIKitPerfComparison`
  - Applies regression gating to UIKit baselines.
  - Main callers: device perf flow and tests.
- `xtask::uikit_report_matches_case_ids(report: &UIKitPerfReport, expected_case_ids: &[&str]) -> bool`
  - Validates that a checkpointed UIKit report contains exactly the case set requested by the current resumable run before reuse.
  - Main callers: device perf flow and tests.
- `xtask::summarize_energy_table(...) -> anyhow::Result<UIKitMetricSummary>`
  - Reduces imported Power Profiler tables into direct device energy summaries when manual traces are available.
  - Main callers: device perf flow and tests.

## Logic narrative

The crate maintains one authoritative UIKit case table that maps XCTest methods to Oxide benchmark identifiers and contract metadata. That table now spans idiomatic UIKit coverage for components, animation effects, primitive lifecycle slices such as empty-root mount, retained-view remove-all/remount, and a shared control-set mount/mutate case, plus the first hand-optimized UIKit flat-rect family.

`cargo xtask experiments check` validates `perf-experiments.toml` before Phase 4 alternatives can age into permanent architecture. Each manifest entry records an id, introduced commit/date, required backends and devices, correctness and performance gates, an expiry date, and a decision state. Undecided entries must name a `perf-ab...` gate because alternate implementations must stay off the default path until same-workload A/B evidence promotes them. Accepted or rejected entries must keep proof and cleanup notes because the losing path, runtime switch, comparison rows, tests, and docs are expected to be deleted after a decision.

The current manifest records accepted proof for the native audit-row retirement, default browser WebGPU standalone-row retirement, default browser upload-scratch row retirement, default browser ID-mask legacy-row retirement, default browser upload legacy-row retirement, default browser glyph-run legacy-row retirement, default browser backdrop-batch legacy-row retirement, clip-state diagnostic export retirement, Canvas indexed-quad stack-array cleanup, Markdown metric-summary direct streaming, Markdown result-row direct streaming, Markdown inline metric-summary streaming, Markdown contract-row direct streaming, Markdown summary direct streaming, Markdown tail-line direct streaming, Markdown metric-priority index lookup, Markdown latest-plus-dated baseline single rendering, persisted PerfReport JSON pre-sized serialization and capacity-hint tightening, compare-reports small-baseline scanning, compare-reports output-vector capacity hints, compare-reports missing-baseline capacity ceiling, compare-reports lookup-path improvement ordering, compare-reports same-order fast path, compare-reports same-order single-pass refinement, compare-reports same-order equal-median fast path, compare-reports same-order-before-small-baseline lookup, distribution metric static keys, distribution-only summaries, distribution stack summary buffers, fixed 24-sample distribution summaries, sample summary stack buffers, fixed-count sample summary quantiles, sample summary unstable sorting, permissions Bluetooth discovery cache move, permissions sensor permission-cache fixed slots, and permissions manager state fixed slots. It also records rejected cleanup attempts for coalescing compaction, Markdown A/B audit branch deletion, Markdown preallocation, Markdown metric-summary vector capacity, Markdown metric-priority table hoisting, Markdown metric-priority bitmasking, Markdown literal string-line writes, Markdown metric-summary cap early return, Markdown empty-priority fast paths, case-filter explicit state fast paths, compare-reports HashMap lookup, compare-reports regression-vector capacity, compare-reports specialized small-baseline looping, compare-reports same-order lazy improvements allocation, upload-scratch diagnostics, standalone WebGPU diagnostics, and draw-item, draw-state, and ID-mask diagnostic exports so mixed or losing A/B evidence remains machine-checkable. The checker treats those entries as evidence metadata; it does not rerun the original browser or perf workloads.

Device perf runs reuse the same case mapping but add process-scoped Instruments attachment. CPU metrics still come from XCTest, while external GPU timing, GPU counters, and the canonical device phase/signpost timings come from process-scoped Metal System Trace on the same case. Oxide on-screen rows also merge host-console stage summaries so in-app Metal command-buffer and timestamp-counter timings survive even when Apple rejects the optional Instruments hardware-counter profile. Energy remains an optional imported input sourced from manual Power Profiler traces, and the report marks it as manual-pending when those traces are absent.

The active device harness now trims a large amount of orchestration dead weight out of that path. Launched traces use a small case-aware time-limit buffer instead of a fixed multi-second pad, XCTest outer measurement counts are adaptive by workload family instead of a flat 10/5 policy, and the device trace-settle delay is reduced to a short default for signposted cases. Metrics shards are grouped by environment instead of forcing singleton shards for every UI-test/camera case, prepared `.xctestrun` files are hashed by their environment and only rewritten when their bytes change, and unchanged derived-data builds are reused through a persisted input fingerprint stamp. The device-side `devicectl ... -j` polling path now retries transient streaming/control-channel failures, transient launched `xctrace` wall-time watchdog overruns are retried once instead of aborting the full battery immediately, and `xctrace` reduction walks one parsed trace artifact per case instead of repeatedly re-exporting overlapping table sets.

Before any `xcodebuild test-without-building` device batch, the harness now also preflights the phone's interactive state through `devicectl device info lockState` and `devicectl device info displays`. If the phone is locked or the main display backlight is off, the run fails fast and keeps its checkpoints instead of burning time in Xcode destination-preflight limbo.

The default committed UIKit device battery is intentionally a compact representative signal battery, not the exhaustive case matrix. It now includes headline UI object rows for labels, progress bars, spinners, buttons, toggles, sliders, images, nine-slice images, and collection views, plus common animation rows for spinner, indeterminate progress, button press scale, toggle spring, slider movement, image zoom/pan, and timeline bars. Dense count/style matrices are tiered down to a smaller high-signal subset in the default run so the official device baseline preserves distinct behaviors instead of every near-duplicate permutation. The full case table remains callable by explicit `--case` selection when a touched area or nightly/full-contract run needs the complete matrix.

The official compare flow is now staged instead of using the full baseline pass as a debugging tool. `cargo xtask ios compare-device-perf --watchable-smoke` runs a small visibly watchable representative set and writes its own checkpointed artifacts under `watchable/<family-or-all>/`. `cargo xtask ios compare-device-perf --family <component|animation|navigation|journey|camera>` runs the compact proof set for one family under `family/<family>/`. The root-level full `--write-baseline` promotion run keeps using `uikit/` and `oxide/`, but it now refuses to write official baselines until the corresponding family proofs for the current build stamp are green in `proof-status.json`.

Watchable smoke runs now also enable app-rendered frame capture for both Oxide and UIKit. Each watched case can persist a small PNG sequence under `<case-dir>/rendered-frames/`, copied back from the app's data container after the case finishes. Those frames are diagnostic artifacts for visual parity and black/blank-scene debugging; they are intentionally limited to watchable smoke so they do not slow the family-proof or promotion baseline paths.

Resumable UIKit and Oxide device flows only reuse a completed `current.json` when the report case IDs exactly match the selected case set. This keeps a prior smoke, family, or explicit `--case` run from satisfying a different requested run through a stale checkpoint.

For camera preview, the official today bucket is the parked microscope pair: the pure custom Oxide-owned NV12 live preview path and the matching `AVCaptureVideoPreviewLayer` baseline. Actual app-host camera runs and hybrid visible-preview-layer variants remain callable by explicit `--case`, but they are separate diagnostic or shipping-oriented buckets and are not part of the default committed camera baseline.

The Oxide device flow installs the host app on the same physical iPhone, launches the parked benchmark app with the in-process Rust perf suite enabled, triggers it over Darwin notifications, then reconstructs the JSON report from the console payload and persists it under `benchmarks/oxide-device/`. Markdown rendering rewrites the baseline workflow so the report points at the device-only command instead of the desktop workspace runner.

For the on-screen Oxide battery, the authoritative device workload window is no longer inferred from the older offscreen Rust suite. The parked host app now emits the bounded `PerfWorkload` interval through the host-side `com.oxide.perf` Points-of-Interest log, and the device harness traces that same live process through a launched Metal trace on the real app-hosted MetalView surface. That keeps the on-screen Oxide path on the real host view, preserves the parked-app console summaries when they are available, and lets the harness stop the trace on the app's `com.oxide.perf.complete` notification instead of relying on a blind wall-clock timeout.

Oxide on-screen device reports emit the same canonical workload-family contract rows as the workspace battery. Families not yet captured on physical hardware are reported as `missing` or `partial` instead of being omitted, so the device report cannot imply comprehensive launch, layout, text-input, bridge, endurance, or stress coverage before those rows exist.

Headline comparisons use visible workload, transition, interaction, or present signposts as the first-order statistic. CPU and memory columns remain process-attribution metrics, because UIKit and iOS can place some framework, compositor, or service work outside the app process; the report labels that scope instead of treating uncharged system work as free.

The report schema carries layer/scenario/style/cache/refresh metadata so the UIKit results can be compared directly against the Oxide-side battery. For the official physical-device path, `refresh_mode` is now intentionally native-only; the old 60 Hz/device-default matrix was removed from the committed harness to cut wall time and keep the battery aligned with the target shipping path. The schema also persists `measure_iterations`, `benchmark_iterations`, `canonical_signpost_source`, and per-metric `source` plus `fallback_modes`, so the timing provenance is explicit instead of inferred from notes. On device runs, `signpost_*` keys are reserved for `xctrace`; any XCTest signpost metrics are preserved separately under `xctest_*`.

Official device reports now validate their metric contract before cache reuse, JSON writes, markdown writes, baseline comparisons, and baseline promotion. UIKit device rows must carry wall-clock, CPU, memory, direct GPU time, GPU latency, hitch, and missed-frame metrics with finite distribution fields. Oxide on-screen device rows must carry headline clock, memory, direct GPU time, GPU latency, hitch, and missed-frame metrics; GPU and cadence metrics also persist flattened `_p50`, `_p95`, `_p99`, `_peak`, and `_samples` keys. Missing values fail the local report path instead of rendering as zero-valued markdown cells.

The committed `benchmarks/oxide-device/latest.json` and `benchmarks/uikit-device/latest.json` files are also read by the macOS xtask test suite. A refreshed baseline may satisfy the strict metric contract directly; a stale checked-in baseline that predates the contract must explicitly mark its metric-contract status as stale partial so it cannot imply complete official device coverage while the physical phone rerun is pending.

## Preconditions and postconditions

- Preconditions:
  - The Xcode project and schemes must build.
  - Required Apple command-line tools must be installed and resolvable through `xcrun`.
  - The requested physical-device destination must exist for the official workflow.
  - Imported power traces, when supplied, must correspond to the same workload/device/build being compared.
  - Experiment manifest entries must use `YYYY-MM-DD` calendar dates and non-empty proof fields for decided entries.
- Postconditions:
  - `cargo xtask experiments check` fails expired undecided experiments before their alternate paths can remain in default architecture.
  - Undecided experiment entries are required to name a `perf-ab` gate, while decided entries are required to name proof and cleanup notes.
  - Successful device runs emit a device report with Oxide in-app GPU timing, external Metal System Trace GPU timing, any available GPU counters, plus direct energy when imported traces are present.
  - Device report writes and cache reuse fail if required direct GPU, frame-cadence, or memory metrics are missing from the physical-device report rows.
  - The emitted UIKit case rows persist actual measure-loop counts, actual benchmark-loop counts, explicit canonical signpost source, and per-metric provenance/fallback metadata.
  - Repeated unchanged local runs should skip the expensive iOS rebuild path and reuse the previously fingerprinted derived data plus hashed `.xctestrun` variants.
- Invariants maintained:
  - The UIKit case mapping is the single source of truth for report IDs and parity notes.
  - Local debug and device reports use the same case identity and metadata surface.
  - Device report validation fails required metric omissions before regression gating; optional metric regressions are gated only when those metrics are present in both current and baseline reports.

## Changelog

- 2026-07-14: recorded the accepted C34 Metal ID-mask field packing and three rejected compositor guardrail refinements, bringing the manifest to 169 decided entries with 80 accepted and 89 rejected.
- 2026-07-14: recorded the accepted C33 WebGPU ID-mask field cache and rejected one-entry alternative, bringing the manifest to 165 decided entries with 79 accepted and 86 rejected.
- 2026-06-23: recorded the accepted permissions manager state fixed-slots cleanup, bringing the manifest to 73 decided entries with 52 accepted and 21 rejected.
- 2026-06-23: recorded the accepted permissions sensor permission-cache fixed-slots cleanup, bringing the manifest to 72 decided entries with 51 accepted and 21 rejected.
- 2026-06-23: recorded the accepted permissions Bluetooth discovery cache move, bringing the manifest to 71 decided entries with 50 accepted and 21 rejected.
- 2026-06-23: recorded the accepted perf-runner sample summary unstable-sort cleanup, bringing the manifest to 70 decided entries with 49 accepted and 21 rejected.
- 2026-06-22: recorded the accepted perf-runner sample summary fixed-count quantile cleanup, bringing the manifest to 69 decided entries with 48 accepted and 21 rejected.
- 2026-06-22: recorded the accepted perf-runner sample summary stack-buffer cleanup, bringing the manifest to 68 decided entries with 47 accepted and 21 rejected.
- 2026-06-22: recorded the accepted perf-runner fixed 24-sample distribution summary cleanup, bringing the manifest to 67 decided entries with 46 accepted and 21 rejected.
- 2026-06-22: recorded the accepted perf-runner distribution stack summary buffer cleanup, bringing the manifest to 66 decided entries with 45 accepted and 21 rejected.
- 2026-06-22: recorded the accepted perf-runner distribution summary unused-field cleanup, bringing the manifest to 65 decided entries with 44 accepted and 21 rejected.
- 2026-06-22: recorded the accepted perf-runner contract coverage tail-only phrase match, bringing the manifest to 64 decided entries with 43 accepted and 21 rejected.
- 2026-06-22: recorded the accepted perf-runner contract coverage first-byte ASCII fold, bringing the manifest to 63 decided entries with 42 accepted and 21 rejected.
- 2026-06-22: recorded the accepted perf-runner contract coverage allocation-free gap-note scan, bringing the manifest to 62 decided entries with 41 accepted and 21 rejected.
- 2026-06-22: recorded the accepted perf-runner case metric contract static required-key validation, bringing the manifest to 61 decided entries with 40 accepted and 21 rejected.
- 2026-06-22: recorded the accepted perf-runner distribution metric static-key insertion, bringing the manifest to 60 decided entries with 39 accepted and 21 rejected.
- 2026-06-22: recorded the accepted perf-runner frame-pacing static metric-key insertion, bringing the manifest to 59 decided entries with 38 accepted and 21 rejected.
- 2026-06-22: recorded the accepted perf-runner compare-reports lookup-path improvement ordering, bringing the manifest to 58 decided entries with 37 accepted and 21 rejected.
- 2026-06-22: recorded the accepted perf-runner compare-reports same-order-before-small-baseline lookup, bringing the manifest to 57 decided entries with 36 accepted and 21 rejected.
- 2026-06-22: recorded the accepted perf-runner compare-reports same-order equal-median fast path, bringing the manifest to 56 decided entries with 35 accepted and 21 rejected.
- 2026-06-22: recorded the rejected perf-runner compare-reports same-order lazy improvements allocation attempt, bringing the manifest to 55 decided entries with 34 accepted and 21 rejected.
- 2026-06-22: recorded the rejected perf-runner case-filter state fast-path attempt, bringing the manifest to 54 decided entries with 34 accepted and 20 rejected.
- 2026-06-22: recorded the accepted perf-runner compare-reports same-order single-pass refinement, bringing the manifest to 53 decided entries with 34 accepted and 19 rejected.
- 2026-06-22: recorded the accepted perf-runner compare-reports same-order fast path, bringing the manifest to 52 decided entries with 33 accepted and 19 rejected.
- 2026-06-22: recorded the rejected perf-runner JSON String pre-sized serialization attempt, bringing the manifest to 51 decided entries with 32 accepted and 19 rejected.
- 2026-06-22: recorded the rejected compare-reports regression-vector capacity attempt, bringing the manifest to 38 decided entries with 20 accepted and 18 rejected.
- 2026-06-22: updated `perf-experiments.toml` validation expectations for the accepted compare-reports output-vector capacity proof, bringing the manifest to 37 decided entries with 20 accepted and 17 rejected.
- 2026-06-22: added `perf-experiments.toml` validation through `cargo xtask experiments check` so undecided A/B alternatives must remain behind `perf-ab` gates and expire unless accepted or rejected with proof and cleanup notes; the manifest now includes the accepted default browser glyph-run/backdrop-batch legacy-row retirement proof, accepted perf-runner case-filter, Markdown comparison-render, and compare-reports small-baseline proof, plus rejected perf-runner compare-reports HashMap and specialized-loop proof.
- 2026-06-01: added macOS-only static tests for the committed Oxide/UIKit device baseline files so stale baselines must either satisfy the strict direct GPU/cadence/memory contract or explicitly mark the metric-contract gap as stale partial.
- 2026-05-31: added explicit Oxide/UIKit device report metric-contract validation for direct GPU timing, GPU latency, frame cadence, and memory before cache reuse, report writes, and baseline comparisons; Oxide GPU/cadence rows now persist flattened p50/p95/p99/peak/sample distribution keys.
- 2026-05-31: made Oxide on-screen device contract reports list the canonical workload families and mark absent device rows as partial or missing.
- 2026-05-05: Collapsed duplicate resumable report case-set checks and table-backed Oxide contract status checks.
- 2026-05-04: Added UIKit `current.json` case-set validation before resumable device-report reuse.
- 2026-04-26: Added headline UI object and common animation cases to the official Oxide/UIKit device battery, plus fairness wording for system-attributed iOS work.
- 2026-04-11: Removed the redundant manual `Default` implementation for `UIKitMetricSummary`; the derived default keeps the same metric fallback values while reducing report-schema implementation surface.
- 2026-04-26: Merged Oxide host-console stage summaries into on-screen device rows so in-app Metal GPU timings are persisted independently of Instruments counter-profile support.
