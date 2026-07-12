# oxide-perf-runner::paired

## Intention and purpose

`oxide_perf_runner::paired` defines the shared raw-evidence and statistical-analysis contract for renderer A/B experiments. It prevents workspace, Metal, browser, WebGPU, and device workflows from inventing incompatible percentile or acceptance rules.

## Relation to the rest of the code

- `PairedWorkflowPlan` selects the workspace CPU, Metal, WebGPU, browser-startup, or device adapter and describes fresh-process A/B commands with JSON pointers for warmups and samples.
- `run_paired_workflow` verifies the baseline commit and staged candidate tree, hashes the instrumentation patch and executable artifacts, executes the fixed balanced order, and produces `PairedExperimentInput` records containing immutable identities, environments, warmups, raw samples, invalidated attempts, and artifact hashes.
- `analyze_paired_experiment` validates those records and produces `PairedExperimentReport` for experiment evidence and commit proof blocks.
- `oxide-perf-runner --paired-run PLAN --paired-json-out OUTPUT` executes any supported adapter through the common acquisition and analysis boundary.
- `oxide-perf-runner --paired-analyze INPUT --paired-json-out OUTPUT` deterministically reanalyzes already collected raw evidence.
- `oxide-perf-runner --paired-create-instrumentation-patch OUT --paired-instrumentation-root ROOT --paired-instrumentation-path PATH [...]` creates one binary Git patch from declared benchmark-only paths and prints its SHA-256 so the same patch can be applied to both worktrees.
- Browser RAF, workspace, Metal, startup, and device adapters remain responsible for acquiring their platform-specific samples; this module owns their common validation and statistics.

Call flow:

- workflow adapter
  - `balanced_pair_order`
  - verify `git rev-parse HEAD` for A and `git write-tree` for B
  - fresh platform-specific A/B processes with `{pair}`, `{side}`, and `{result}` substitutions
  - hash binary, instrumentation, raw result, stdout, and stderr artifacts
  - `analyze_paired_experiment`
    - identity/environment/sample validation
    - pair medians and speedups
    - fixed-seed paired bootstrap
    - acceptance guardrails
  - `report_json`

## Entry points list

- `oxide_perf_runner::paired::balanced_pair_order(seed: u64, pair_count: usize) -> Vec<PairOrder>` creates deterministic balanced AB/BA blocks.
- `oxide_perf_runner::paired::run_paired_workflow(plan: PairedWorkflowPlan) -> anyhow::Result<PairedExperimentReport>` acquires, persists, validates, and analyzes one manifest-driven experiment.
- `oxide_perf_runner::paired::create_instrumentation_patch(source_root, paths, output_path) -> anyhow::Result<String>` creates a nonempty binary patch from declared paths and returns its SHA-256 identity.
- `oxide_perf_runner::paired::analyze_paired_experiment(input: PairedExperimentInput) -> anyhow::Result<PairedExperimentReport>` validates raw evidence and computes the shared decision.
- `oxide_perf_runner::paired::report_json(report: &PairedExperimentReport) -> anyhow::Result<Vec<u8>>` emits stable pretty JSON with a trailing newline.
- Public schema types describe workload class, identities, environments, raw pairs, distributions, decisions, and reports.

## Logic narrative

The analyzer first verifies schema and build identities. It derives the only valid pair order from the fixed seed, rejects missing warmups/raw samples/artifact hashes, rejects invalid numbers and mixed environments, and enforces workload-specific pair and raw-sample minima. Visible browser/device workloads must identify themselves as production-path measurements.

Each valid pair is reduced to an A and B median only for paired speedup and bootstrap calculations. Reported p50/p95/p99/peak, median absolute deviation, and coefficient of variation are computed from every persisted raw sample, never from batch averages or pair medians. Relative pair speedups feed a deterministic 100,000-resample paired bootstrap. Performance acceptance uses the program-wide speedup, confidence, pair-win, raw-tail, and raw-peak gates; measurement/correctness work may select the explicit no-material-regression policy while retaining the raw-tail gates.

## Preconditions and postconditions

- Samples are finite, nonnegative values of the declared metric. Warm-cache workflows require nonempty warmups; cold browser-startup workflows persist empty warmup arrays because warming would change the workload.
- Pair indices are contiguous and orders equal the seed-derived schedule.
- A and B environments are identical within every valid pair and across the complete experiment.
- Git identities are lowercase 40-character hashes; instrumentation and binary identities are lowercase SHA-256 hashes.
- Every valid pair's binary and instrumentation artifact hashes equal the declared experiment identities, rejecting stale or differently instrumented artifacts.
- Successful analysis preserves all raw pairs and returns deterministic statistics for identical input bytes.

## Edge cases and failure modes

- Missing, invalid, insufficient, or mixed evidence returns an error rather than a partial report.
- Invalidated pairs remain persisted with their reason but do not count toward minima or statistics.
- A zero baseline paired with a nonzero candidate produces a losing infinite speedup, preventing accidental acceptance.
- Empty percentile inputs are used only by internal defensive helpers; validated analysis never reaches them.

## Concurrency and memory behavior

Analysis is single-threaded and deterministic. Bootstrap storage is bounded by 100,000 medians plus one pair-sized resample buffer. No global mutable state or synchronization is used.

## Performance notes

Analysis is evidence-generation work outside production renderer paths. Runtime cost is dominated by the fixed 100,000 bootstrap resamples; it deliberately favors reproducibility over interactive latency.

## Feature flags and cfgs

The module has no feature or target-specific behavior.

## Testing and benchmarks

`tests/paired_experiment_tests.rs` covers deterministic balanced ordering, accepted improvements, parity policy, ties, regressions, raw percentile cardinality, insufficient inputs, within-pair and cross-session environment mixing, stale binary identities, byte-deterministic reports, and a complete 15-pair fresh-process workflow with Git commit/index identity checks.

## Examples

Create the seed-derived order before executing any side, persist every raw pair, then call `analyze_paired_experiment(input)` and write `report_json(&report)` to the experiment evidence directory.

## Changelog

- 2026-07-12: introduced the shared paired evidence schema, manifest-driven five-workflow execution boundary, exact source/artifact validation, raw-distribution summaries, deterministic order/bootstrap analysis, and acceptance guardrails for C00.
