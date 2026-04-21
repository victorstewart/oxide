# oxide-perf-runner::lib

## Intention and purpose

`oxide-perf-runner` owns the persisted Rust-side performance contract for Oxide. It exists to benchmark the engine, representative flows, author-facing APIs, and bridge paths under one regression-gated report format instead of leaving performance validation to ad hoc local timing.

## Relation to the rest of the code

- Upstream callers:
  - `oxide/crates/perf-runner/src/main.rs` invokes `run_from_env()`.
  - CI and local commands route through `cargo run -p oxide-perf-runner -- --run-suite ...`.
- Downstream dependencies:
  - `oxide_ui_core`, `oxide_renderer_metal`, `oxide_test_scenes`, `oxide_platform_api`, and related crates provide the actual workload surfaces being measured.
  - `oxide/benchmarks/workspace/latest.json` and `oxide/benchmarks/workspace/latest.md` are the persisted outputs consumed by review and CI.

## Entry points list

- `oxide_perf_runner::run_from_env() -> anyhow::Result<()>`
  - Parses CLI arguments from the current process and dispatches into either the persisted suite flow or the legacy summary flow.
  - Main callers: `oxide/crates/perf-runner/src/main.rs`.
- `oxide_perf_runner::run_cli(args: &[String]) -> anyhow::Result<()>`
  - Handles the suite CLI, baseline writes, comparisons, and legacy fallback.
  - Main callers: tests and the binary entry point.
- `oxide_perf_runner::compare_reports(current: &PerfReport, baseline: &PerfReport) -> PerfComparison`
  - Applies regression gating against the persisted baseline.
  - Main callers: suite execution and report tests.
- `oxide_perf_runner::assert_full_coverage(coverage: &CoverageReport) -> anyhow::Result<()>`
  - Enforces that every required battery family has an implemented case.
  - Main callers: suite execution and report tests.

## Logic narrative

The crate builds a case inventory spanning component/animation microbenchmarks, primitive lifecycle workloads, scene and journey flows, author-facing API workloads, and bridge workloads. Primitive lifecycle coverage now includes empty-root mount, flat-rect mount/mutate/remove-all/remount, label mount/mutate, card mount/mutate, image mount/mutate, and a shared control-set mount/mutate slice so the report can speak to creation, invalidation, teardown, and remount costs instead of only encode hot paths. The authoring inventory also keeps `cpu.authoring.surface_router.compose` aligned with the public popup lifecycle API by exercising key-popup lookup, touch-region refresh, and approval-gated dismissal through `SurfaceRouter`. Each case is measured into a `PerfCaseResult` that carries both latency distributions and contract metadata such as layer, scenario, variant, cache state, and refresh mode.

CPU-oriented cases run through a shared warmup-and-sample loop that amortizes timer noise by executing enough operations to hit a target sample duration. Journey-style workloads run one full interaction cycle per sample. GPU scene cases execute live Metal frames and persist renderer counters such as draw, encode, cull, and damage summaries when available.

The resulting `PerfReport` is serialized to JSON and Markdown. Baseline comparison is median-based and only applies to gated cases. Coverage validation is structural: the suite is considered incomplete if any required family lacks a case.
Contract coverage battery entries share one status-and-note helper so each required case set is evaluated once, then rendered consistently into the persisted report.

## Preconditions and postconditions

- Preconditions:
  - The workspace must build with the relevant crates and test scenes available.
  - Benchmark cases must record at least one sample.
  - Persisted baselines must be comparable to the current report schema when regression checks are requested.
- Postconditions:
  - A successful suite run produces a complete `PerfReport` with coverage accounting.
  - Baseline comparisons either report no gated regressions or fail with explicit mismatches.
- Invariants maintained:
  - Every persisted case carries contract metadata alongside latency distributions.
  - Coverage counts and covered-name inventories stay synchronized with the registered case inventory.
  - Missing metrics default safely through serde so older baselines remain readable while the schema grows.

## Changelog

- 2026-04-18: Collapsed duplicated contract-battery status and note conditionals into shared helper logic while preserving report output semantics.
- 2026-04-14: Collapsed repetitive coverage assertions into one shared check table while preserving the same incomplete-family error messages.
- 2026-04-11: Removed redundant manual `Default` implementations for internal CLI options and `PerfCaseResult`; derived defaults preserve the same serde fallback behavior with less implementation surface.
