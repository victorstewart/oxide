# oxide-perf-runner::lib

## Intention and purpose

`oxide-perf-runner` runs the Rust-side performance workloads and reduces their samples
into a versioned `PerfReport`. It keeps workload selection, comparison, provenance, and
result serialization in one owner.

## Relation to the rest of the code

- `oxide/crates/perf-runner/src/main.rs` invokes `run_from_env()`.
- The runner exercises the workload owners in `oxide-ui-core`, `oxide-renderer-metal`,
  `oxide-test-scenes`, `oxide-platform-api`, `oxide-platform-web`, and related crates.
- Browser diagnostic runs enable `oxide-renderer-web`'s
  `diagnostic-instrumentation` feature explicitly.
- Benchmark contracts live under `oxide/benchmarks/`; implementations, fixtures and
  regression goldens remain with their owning crates, hosts and tests.
  The Rust runner defaults to `oxide/artifacts/perf-runner/latest.json` and
  `oxide/artifacts/perf-runner/latest.md`; the iOS device runner defaults to
  `oxide/artifacts/oxide-device/latest.json` and
  `oxide/artifacts/oxide-device/latest.md`. Other generated evidence uses an explicit
  artifact path outside tracked benchmark source.

## Entry points

- `oxide_perf_runner::run_from_env() -> anyhow::Result<()>` parses process arguments
  and dispatches the selected suite.
- `oxide_perf_runner::run_cli(args: &[String]) -> anyhow::Result<()>` runs a suite,
  compares reports, writes selected outputs, and exposes focused offline reducers.
- `oxide_perf_runner::compare_reports(current, baseline) -> PerfComparison` applies
  the gated comparison policy.
- `oxide_perf_runner::RepositoryProvenance::{resolve_root, capture, validate,
  ensure_unchanged}` records the source revision associated with an evidence run.
- `oxide_perf_runner::promote_files_atomically(outputs)` installs one complete output
  set without leaving a partial report behind.

## Focused diagnostic commands

The runner also provides offline, caller-directed diagnostics. They read existing report
input or fixed representative data and do not expand the selected suite:

- `--paired-analyze INPUT --paired-json-out OUTPUT` reduces already-collected paired
  samples.
- `--bench-markdown-render PATH [--bench-markdown-compare PATH]
  [--bench-markdown-iters N]` and `--bench-markdown-write PATH
  [--bench-markdown-compare PATH] [--bench-markdown-iters N]` load an existing
  `PerfReport` JSON and optional comparison baseline to exercise report generation or
  the Markdown output path without rerunning suite workloads.
- `--bench-json-render PATH [--bench-json-iters N]` exercises pretty JSON serialization
  through the shared pre-sized serializer and `to_writer_pretty`; `--bench-json-string-render
  PATH [--bench-json-iters N]` exercises the String-return JSON path used for host-facing
  JSON export changes.
- `--bench-sample-summary [--bench-sample-summary-iters N]`,
  `--bench-case-filter [--bench-case-filter-iters N]`,
  `--bench-frame-pacing-metrics [--bench-frame-pacing-iters N]`, and
  `--bench-distribution-metrics [--bench-distribution-iters N]` exercise fixed summary,
  selection, pacing, and distribution operations with deterministic checksums.
- `--bench-case-metric-contract PATH [--bench-case-metric-iters N]` and
  `--bench-contract-coverage PATH [--bench-contract-iters N]` validate report contracts
  from caller-provided data.
- `--bench-compare-reports CURRENT BASELINE [--bench-compare-iters N]` repeatedly runs
  `compare_reports` on caller-provided reports.

## Report contract

A report records its suite, selected cases, distributions, workload metadata, contract
coverage, and repository provenance. Report versioning preserves readable older input
where supported and requires complete source provenance for current official output.

A requested comparison rejects missing gated rows and regressions before any default
output is replaced. Publication validates the complete JSON/Markdown output set before
promotion. `--write-baseline` writes the canonical Rust result to
`oxide/artifacts/perf-runner/latest.json` and
`oxide/artifacts/perf-runner/latest.md`; the device flow writes to
`oxide/artifacts/oxide-device/latest.json` and
`oxide/artifacts/oxide-device/latest.md`. Diagnostic commands can write only to their
caller-provided artifact paths.

The runner deliberately selects the requested workload set; a filtered run is not a
claim about catalog-wide coverage. The benchmark contract documents the workload and
required result fields, while the artifact directory holds the evidence from a specific
machine and revision.

## Testing and benchmarks

Run focused checks with the requested workload or report fixture. Do not add generated
reports to Git. Retain deterministic source fixtures and regression goldens with
the workload or test that owns them.
