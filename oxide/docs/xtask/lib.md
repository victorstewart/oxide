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

- `xtask::run(args: Vec<String>) -> anyhow::Result<()>`
  - Dispatches command-line tasks, including the official iOS device perf flow and the Oxide device-report flow.
  - Main callers: `oxide/xtask/src/main.rs`.
- `xtask::parse_uikit_report_json(text: &str) -> anyhow::Result<UIKitPerfReport>`
  - Converts exported XCTest metric JSON into the UIKit report schema used by tests and any non-official local debug tools.
  - Main callers: tests.
- `xtask::extract_oxide_device_report_json(stdout: &str) -> anyhow::Result<String>`
  - Reconstructs the base64-chunked Oxide device report payload from the parked app's `devicectl --console` output.
  - Main callers: Oxide device perf flow and tests.
- `xtask::compare_uikit_reports(current: &UIKitPerfReport, baseline: &UIKitPerfReport) -> UIKitPerfComparison`
  - Applies regression gating to UIKit baselines.
  - Main callers: device perf flow and tests.
- `xtask::summarize_energy_table(...) -> anyhow::Result<UIKitMetricSummary>`
  - Reduces imported Power Profiler tables into direct device energy summaries when manual traces are available.
  - Main callers: device perf flow and tests.

## Logic narrative

The crate maintains one authoritative UIKit case table that maps XCTest methods to Oxide benchmark identifiers and contract metadata. That table now spans idiomatic UIKit coverage for components, animation effects, primitive lifecycle slices such as empty-root mount, retained-view remove-all/remount, and a shared control-set mount/mutate case, plus the first hand-optimized UIKit flat-rect family.

Device perf runs reuse the same case mapping but add process-scoped Instruments attachment. CPU metrics still come from XCTest, while external GPU timing, GPU counters, and the canonical device phase/signpost timings come from process-scoped Metal System Trace on the same case. Oxide on-screen rows also merge host-console stage summaries so in-app Metal command-buffer and timestamp-counter timings survive even when Apple rejects the optional Instruments hardware-counter profile. Energy remains an optional imported input sourced from manual Power Profiler traces, and the report marks it as manual-pending when those traces are absent.

The active device harness now trims a large amount of orchestration dead weight out of that path. Launched traces use a small case-aware time-limit buffer instead of a fixed multi-second pad, XCTest outer measurement counts are adaptive by workload family instead of a flat 10/5 policy, and the device trace-settle delay is reduced to a short default for signposted cases. Metrics shards are grouped by environment instead of forcing singleton shards for every UI-test/camera case, prepared `.xctestrun` files are hashed by their environment and only rewritten when their bytes change, and unchanged derived-data builds are reused through a persisted input fingerprint stamp. The device-side `devicectl ... -j` polling path now retries transient streaming/control-channel failures, transient launched `xctrace` wall-time watchdog overruns are retried once instead of aborting the full battery immediately, and `xctrace` reduction walks one parsed trace artifact per case instead of repeatedly re-exporting overlapping table sets.

Before any `xcodebuild test-without-building` device batch, the harness now also preflights the phone's interactive state through `devicectl device info lockState` and `devicectl device info displays`. If the phone is locked or the main display backlight is off, the run fails fast and keeps its checkpoints instead of burning time in Xcode destination-preflight limbo.

The default committed UIKit device battery is intentionally a compact representative signal battery, not the exhaustive case matrix. Dense count/style matrices are tiered down to a smaller high-signal subset in the default run so the official device baseline preserves distinct behaviors instead of every near-duplicate permutation. The full case table remains callable by explicit `--case` selection when a touched area or nightly/full-contract run needs the complete matrix.

The official compare flow is now staged instead of using the full baseline pass as a debugging tool. `cargo xtask ios compare-device-perf --watchable-smoke` runs a small visibly watchable representative set and writes its own checkpointed artifacts under `watchable/<family-or-all>/`. `cargo xtask ios compare-device-perf --family <animation|navigation|journey|camera>` runs the compact proof set for one family under `family/<family>/`. The root-level full `--write-baseline` promotion run keeps using `uikit/` and `oxide/`, but it now refuses to write official baselines until the corresponding family proofs for the current build stamp are green in `proof-status.json`.

Watchable smoke runs now also enable app-rendered frame capture for both Oxide and UIKit. Each watched case can persist a small PNG sequence under `<case-dir>/rendered-frames/`, copied back from the app's data container after the case finishes. Those frames are diagnostic artifacts for visual parity and black/blank-scene debugging; they are intentionally limited to watchable smoke so they do not slow the family-proof or promotion baseline paths.

For camera preview, the official today bucket is the parked microscope pair: the pure custom Oxide-owned NV12 live preview path and the matching `AVCaptureVideoPreviewLayer` baseline. Actual app-host camera runs and hybrid visible-preview-layer variants remain callable by explicit `--case`, but they are separate diagnostic or shipping-oriented buckets and are not part of the default committed camera baseline.

The Oxide device flow installs the host app on the same physical iPhone, launches the parked benchmark app with the in-process Rust perf suite enabled, triggers it over Darwin notifications, then reconstructs the JSON report from the console payload and persists it under `benchmarks/oxide-device/`. Markdown rendering rewrites the baseline workflow so the report points at the device-only command instead of the desktop workspace runner.

For the on-screen Oxide battery, the authoritative device workload window is no longer inferred from the older offscreen Rust suite. The parked host app now emits the bounded `PerfWorkload` interval through the host-side `com.oxide.perf` Points-of-Interest log, and the device harness traces that same live process through a launched Metal trace on the real app-hosted MetalView surface. That keeps the on-screen Oxide path on the real host view, preserves the parked-app console summaries when they are available, and lets the harness stop the trace on the app's `com.oxide.perf.complete` notification instead of relying on a blind wall-clock timeout.

The report schema carries layer/scenario/style/cache/refresh metadata so the UIKit results can be compared directly against the Oxide-side battery. For the official physical-device path, `refresh_mode` is now intentionally native-only; the old 60 Hz/device-default matrix was removed from the committed harness to cut wall time and keep the battery aligned with the target shipping path. The schema also persists `measure_iterations`, `benchmark_iterations`, `canonical_signpost_source`, and per-metric `source` plus `fallback_modes`, so the timing provenance is explicit instead of inferred from notes. On device runs, `signpost_*` keys are reserved for `xctrace`; any XCTest signpost metrics are preserved separately under `xctest_*`.

## Preconditions and postconditions

- Preconditions:
  - The Xcode project and schemes must build.
  - Required Apple command-line tools must be installed and resolvable through `xcrun`.
  - The requested physical-device destination must exist for the official workflow.
  - Imported power traces, when supplied, must correspond to the same workload/device/build being compared.
- Postconditions:
- Successful device runs emit a device report with Oxide in-app GPU timing, external Metal System Trace GPU timing, any available GPU counters, plus direct energy when imported traces are present.
  - The emitted UIKit case rows persist actual measure-loop counts, actual benchmark-loop counts, explicit canonical signpost source, and per-metric provenance/fallback metadata.
  - Repeated unchanged local runs should skip the expensive iOS rebuild path and reuse the previously fingerprinted derived data plus hashed `.xctestrun` variants.
- Invariants maintained:
  - The UIKit case mapping is the single source of truth for report IDs and parity notes.
  - Local debug and device reports use the same case identity and metadata surface.
  - Regression gating only uses metrics that are actually present in both the current and baseline reports.

## Changelog

- 2026-04-11: Removed the redundant manual `Default` implementation for `UIKitMetricSummary`; the derived default keeps the same metric fallback values while reducing report-schema implementation surface.
- 2026-04-26: Merged Oxide host-console stage summaries into on-screen device rows so in-app Metal GPU timings are persisted independently of Instruments counter-profile support.
