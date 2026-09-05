# oxide-perf-runner::density_acquisition

## Intention and purpose

`density_acquisition.rs` owns the benchmark-only macOS acquisition control plane that feeds `density_calibration.rs`. It closes the prior reducer-only gap by executing the frozen `k = 1, 2, 4, 8, all` candidate sequence, rather than accepting an unaudited hand-built reducer input.

## Relation to the rest of the code

The module is called by `xtask compare-ui calibrate-density --platform macos`. It writes canonical driver requests and validated results to an external campaign directory, constructs `DensityCalibrationInput`, and invokes `reduce_density_calibration`. The driver is a benchmark adapter boundary: it runs the actual isolated and packed AppKit/Oxide sessions and returns the typed evidence; no comparison scheduling or result selection belongs in a production Oxide crate.

Call flow:

- `xtask compare-ui calibrate-density`
  - `acquire_macos_density`
    - canonical plan validation
    - `k=1,2,4,8,all` schedule construction
    - bounded macOS driver subprocesses
    - atomic request/result persistence
    - `reduce_density_calibration`
    - canonical evidence, reduction, and completion receipt

## Entry points list

- `canonical_macos_density_acquisition_plan_json(plan: &MacOsDensityAcquisitionPlan) -> anyhow::Result<Vec<u8>>` emits the exact pretty-JSON plan bytes with one trailing newline.
- `validate_macos_density_acquisition_plan(plan: &MacOsDensityAcquisitionPlan) -> anyhow::Result<()>` rejects non-macOS roles, incomplete identities, invalid pair blocks, duplicate implementations/scenarios, invalid margins/weights, missing sentinels, and any candidate surface that cannot produce all five required sizes.
- `acquire_macos_density(plan_path: &Path, driver_path: &Path, output_root: &Path, resume: bool) -> anyhow::Result<MacOsDensityAcquisitionReport>` hashes and runs an absolute driver executable through the bounded command protocol.
- `acquire_macos_density_with_runner(...) -> anyhow::Result<MacOsDensityAcquisitionReport>` exposes the same scheduler with an injected runner for deterministic tests and benchmark-station adapters.
- `MacOsDensityAcquisitionPlan`, `MacOsDensityScenario`, `MacOsDensityDriverRequest`, `MacOsDensityRunRequest`, `MacOsDensityObservationContract`, `MacOsDensityDriverResult`, and `MacOsDensityAcquisitionReport` are the versioned plan, request, evidence, and completion DTOs.
- `MacOsDensityRunMode::{Isolated, Packed}` makes the process-lifecycle mode explicit in every driver request.

## Logic narrative

The controller validates a canonical plan and derives exactly five sizes. Every size covers the complete scenario inventory: the size controls partitioning, not scenario selection. For each candidate it begins with four pairs. The four treatment orders balance each of `oxide-isolated`, `oxide-packed`, `native-production-isolated`, and `native-production-packed` across all four ordinal positions. Scenario order rotates deterministically from the frozen seed and reverses on alternating pairs; isolated and packed treatments receive the same ordered partition.

Each isolated treatment partitions the order into singleton packs, while each packed treatment uses the candidate-sized partition. Both preserve identical scenario order and declared primary-metric identity. Each request requires one observation for every implementation/scenario combination plus first and last reset-sentinel observations. Result admission checks every identity, position, metric, margin, estimator, timing, capacity, build/plan/session hash, and occupancy field before a result can be atomically committed. After each complete four-pair block the module runs the existing whole-pair-bootstrap reducer. It stops only when no candidate is inconclusive or when the preregistered maximum of 4 through 24 pairs is reached.

## Preconditions and postconditions

- The plan is canonical JSON, names `macos-apple-silicon`, declares exactly two implementations, and contains more than eight bounded scenarios.
- The output root is absolute. A new acquisition refuses an existing root; resume accepts only byte-identical plan, request, result, evidence, and report artifacts.
- The command driver is an absolute existing file and receives only `--platform macos --request PATH --output PATH`; the real driver additionally requires absolute build-manifest, campaign-plan, and session-root paths through its named environment contract.
- Successful completion persists all pair artifacts plus `density.input.json`, `density.report.json`, and `acquisition.complete.json`.

## Edge cases and failure modes

The harness rejects missing/extra observations, nonfinite or nonpositive estimators, duplicated identities, platform drift, stale request hashes, process-wall overruns, malformed pair blocks, and noncanonical driver JSON. A failed subprocess, timeout, or partial result cannot create a final result. Resume removes only the exact controller-owned `.tmp` file for the pair it is about to reacquire. Symlinks in the output tree are forbidden.

## Concurrency and memory behavior

Acquisitions are intentionally serial because concurrent AppKit/Oxide sessions would invalidate host-load and thermal controls. One driver process exists at a time. Driver stdout and stderr are discarded to prevent unbounded log growth; the typed result is capped at one MiB, and the complete output tree is capped at 128 MiB. Requests and reports allocate in control-plane code only, never in an Oxide frame loop.

## Performance notes

The module changes no production or rendering path. Packing candidates reduce process launches and setup occupancy only when the calibrated carryover, reset, memory, thermal, capacity, integrity, and wall-time gates accept them. The driver timeout is the tier wall limit plus a fixed 30-second finalization allowance.

## Feature flags and cfgs

There are no feature flags. The protocol is explicitly macOS-only and rejects any other platform role or CLI platform.

## Testing and benchmarks

[`density_acquisition_tests`](tests/density_acquisition_tests.md) proves the five candidate sizes, four-position treatment balance, both run modes, exact observation cardinality, deterministic selection, byte-exact resume, malformed-evidence rejection, and platform/candidate fail-closed behavior. Existing `density_calibration_tests` continue to prove reducer selection and guardrails.

## Examples

```text
cargo run --locked -p xtask -- compare-ui calibrate-density \
   --platform macos \
   --plan /absolute/path/macos-density-plan.json \
   --driver /absolute/path/macos-density-driver \
   --out /absolute/path/macos-density-evidence
```

## Changelog

- 2026-07-21: Connected singleton-isolated and candidate-packed scheduling to the content-addressed macOS comparison-session driver and bound primary metric plus build/plan/session hashes.
- 2026-07-21: Added the bounded macOS isolated-versus-packed acquisition scheduler, driver protocol, atomic resume, and reducer handoff.
