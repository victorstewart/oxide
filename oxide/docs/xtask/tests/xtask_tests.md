# xtask::tests::xtask_tests

## Intention and purpose

This integration-test unit verifies Oxide's repository automation, experiment-manifest policy, iOS/UIKit device-report parsing, device-run coordination, trace attribution, and comparison gates. It exists so `xtask` cannot silently weaken the persisted performance contract or misclassify device evidence.

## Relation to the rest of the code

Cargo discovers the functions marked `#[test]` in `oxide/xtask/tests/xtask_tests.rs`. They exercise public parsing, validation, comparison, environment, and report helpers from `xtask`; fixtures remain in-memory unless a filesystem path or generated xctestrun is part of the contract.

Call flow:

- `cargo test -p xtask --test xtask_tests`
- Rust integration-test discovery
- in-memory manifest/report/trace or temporary xctestrun fixture
- public `xtask` parser, validator, comparator, or coordinator helper
- exact result, error, classification, or persisted-shape assertion

## Entry points list

- `experiment_manifest_checker_accepts_current_manifest()`: validates every committed experiment entry and freezes the current total plus accepted/rejected decision counts.
- `experiment_manifest_checker_rejects_expired_undecided_entries()`: rejects an undecided experiment past its expiry.
- `experiment_manifest_checker_requires_perf_ab_gate_for_undecided_entries()`: requires a concrete A/B gate before an experiment may remain open.
- `experiment_manifest_checker_requires_proof_for_decided_entries()`: requires persisted proof for accepted and rejected decisions.
- `oxide_device_contract_source_lists_canonical_families()` and `xtask_docs_describe_experiment_manifest_check()`: keep policy source and documentation wired.
- UIKit and Oxide report-parser tests exercise JSON extraction, case classification, stage/memory/cadence/camera summaries, sharded merge behavior, and strict metric contracts.
- Device-runner tests exercise xctestrun environment generation, resumable result roots, launch/camera/watch controls, console markers, lock/display state, retry classification, process discovery, and case selection.
- Comparison tests exercise simulator-noise allowances, physical-device CPU/GPU/memory/cadence/energy gates, refresh-mode keys, case-set reuse, promotion prerequisites, and committed-baseline status.
- Trace tests exercise table/schema discovery, duration windows, signpost regions, CPU hotspots, GPU summaries, energy conversion, and unit normalization.
- Private fixture helpers such as `sample_perf_report`, `sample_uikit_report`, and `sample_oxide_device_report` build deterministic inputs and are reached only by tests in this unit.

## Logic narrative

Manifest tests parse the committed TOML through the same production checker used by `cargo xtask experiments check`. The acceptance test first requires important historical ids, then compares the returned summary with the exact committed population so a new experiment must intentionally update the contract. Negative tests isolate expiry, missing A/B policy, and missing decision proof.

Device/report tests construct minimal representative fixtures, call one production helper, and assert both preserved values and rejected gaps. Comparison tests keep simulator diagnostics separate from physical-device authority and require direct GPU plus cadence distributions where policy says they are mandatory. Trace tests reduce exported tables to bounded workload windows before attributing stages, GPU work, or energy.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

Dates use `YYYY-MM-DD`, metric distributions must be finite and ordered, report case sets must match their requested battery, and official device comparisons must not substitute simulator data. A successful manifest check means every entry has valid lifecycle fields and every decided entry has proof and cleanup. This unit contains no unsafe code.

## Edge cases and failure modes

Coverage includes empty/missing report markers, incomplete shards, stale resumable checkpoints, non-native refresh requests, absent device counters, invalid distribution fields, clipped or backdated signposts, unsupported GPU-counter profiles, retryable device streaming/install failures, and trace bundles versus raw exports. Errors must remain descriptive rather than being converted into an apparently valid empty report.

## Concurrency and memory behavior

Most tests use immutable in-memory fixtures. Environment-mutating tests serialize through one process-global mutex and restore prior values after the assertion. Filesystem tests use temporary directories, and device/trace parsers operate on bounded fixture strings without spawning real device work.

## Performance notes

These are contract tests, not timing benchmarks. They protect the experiment registry and parsers that decide whether measured p50/p95/p99/peak, hitch, GPU, memory, and energy changes are accepted. Fixture sizes are deliberately small so native CI remains fast.

## Feature flags and cfgs

The integration tests run on the native host with the workspace's normal `xtask` feature set. Platform-specific behavior is represented by fixtures and temporary files; no attached iPhone or Instruments session is required.

## Testing and benchmarks

Run the complete unit with `cargo test --locked -p xtask --test xtask_tests`. Run the C25-touched manifest gate with `cargo test --locked -p xtask --test xtask_tests experiment_manifest_checker_accepts_current_manifest`.

## Examples

```rust
let summary = xtask::check_experiment_manifest_text(text, "2026-07-13")?;
assert_eq!(summary.undecided, 0);
```

## Changelog

- 2026-07-13: refreshed the exact manifest summary after the completed C00--C26 experiment sequence reached 144 entries: 72 accepted and 72 rejected.
