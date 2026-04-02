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

Device perf runs reuse the same case mapping but add process-scoped Instruments attachment. CPU metrics still come from XCTest, while GPU timing, GPU counters, and the canonical device phase/signpost timings come from process-scoped Metal System Trace on the same case. Energy remains an optional imported input sourced from manual Power Profiler traces, and the report marks it as manual-pending when those traces are absent.

The default committed UIKit device battery is narrower than the full case table. For camera preview, the official today bucket is the parked microscope pair: the pure custom Oxide-owned NV12 live preview path and the matching `AVCaptureVideoPreviewLayer` baseline. Actual app-host camera runs and hybrid visible-preview-layer variants remain callable by explicit `--case`, but they are separate diagnostic or shipping-oriented buckets and are not part of the default committed camera baseline.

The Oxide device flow installs the host app on the same physical iPhone, launches the parked benchmark app with the in-process Rust perf suite enabled, triggers it over Darwin notifications, then reconstructs the JSON report from the console payload and persists it under `benchmarks/oxide-device/`. Markdown rendering rewrites the baseline workflow so the report points at the device-only command instead of the desktop workspace runner.

The report schema carries layer/scenario/style/cache/refresh metadata so the UIKit results can be compared directly against the Oxide-side battery. It now also persists `measure_iterations`, `benchmark_iterations`, `canonical_signpost_source`, and per-metric `source` plus `fallback_modes`, so the timing provenance is explicit instead of inferred from notes. On device runs, `signpost_*` keys are reserved for `xctrace`; any XCTest signpost metrics are preserved separately under `xctest_*`.

## Preconditions and postconditions

- Preconditions:
  - The Xcode project and schemes must build.
  - Required Apple command-line tools must be installed and resolvable through `xcrun`.
  - The requested physical-device destination must exist for the official workflow.
  - Imported power traces, when supplied, must correspond to the same workload/device/build being compared.
- Postconditions:
  - Successful device runs emit a device report with direct GPU timing and any available GPU counters, plus direct energy when imported traces are present.
  - The emitted UIKit case rows persist actual measure-loop counts, actual benchmark-loop counts, explicit canonical signpost source, and per-metric provenance/fallback metadata.
- Invariants maintained:
  - The UIKit case mapping is the single source of truth for report IDs and parity notes.
  - Local debug and device reports use the same case identity and metadata surface.
  - Regression gating only uses metrics that are actually present in both the current and baseline reports.
