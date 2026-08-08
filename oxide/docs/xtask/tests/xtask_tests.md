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

- `experiment_manifest_checker_accepts_current_manifest()`: validates every committed experiment entry and freezes the current total plus accepted/rejected decision counts, including C60's accepted image-store policy and two rejected UIKit proof paths.
- `experiment_manifest_checker_rejects_expired_undecided_entries()`: rejects an undecided experiment past its expiry.
- `experiment_manifest_checker_requires_perf_ab_gate_for_undecided_entries()`: requires a concrete A/B gate before an experiment may remain open.
- `experiment_manifest_checker_requires_proof_for_decided_entries()`: requires persisted proof for accepted and rejected decisions.
- `oxide_device_contract_source_lists_canonical_families()` and `xtask_docs_describe_experiment_manifest_check()`: keep policy source and documentation wired.
- `device_battery_policy_uses_canonical_mode_naming()`: rejects legacy mode terminology and requires the canonical-promotion/no-run-everything contract.
- `contract_coverage_status_distinguishes_absence_from_partial_coverage()`: prevents an unselected workload family from being reported as partial evidence.
- `test_all_checks_the_featureless_graph_without_rerunning_test_binaries()`: freezes one all-feature test execution plus a compile-only featureless graph check and requires all five Cargo subprocesses to honor the lockfile.
- `flat_rect_remove_rebuild_cycle_has_honest_parity_ids()`: requires UIKit and Oxide to publish the repeated teardown workload as a remove/rebuild cycle and rejects the misleading remove-all name.
- `canonical_device_battery_is_exactly_five_matched_comparison_pairs()`: freezes the exact ten UIKit rows, their five idiomatic/optimized pair keys, comparison families, required workload-family mappings, and matching Oxide ids.
- `noncanonical_device_cases_remain_exactly_addressable()`: proves retired default memberships, including the hybrid camera diagnostic, still resolve through exact case selection.
- `standalone_oxide_default_matches_the_five_unique_compare_rows()`: prevents the no-`--case` Oxide command from silently running a broader on-screen battery than matched promotion.
- `compare_device_watchable_smoke_is_the_six_row_visual_subset()`: freezes the six UIKit visual-QA rows and their five deduplicated Oxide mappings.
- `compare_device_promotion_validates_before_committed_baseline_writes()`: keeps report-contract and requested-comparison failures ahead of both committed baseline writes and rejects the removed proof-status gate.
- `compare_device_promotion_rejects_partial_case_selection()`: prevents `--write-baseline --case ...` from replacing the canonical committed reports with a partial selection.
- `device_build_for_testing_uses_release_iphoneos_configuration()`: freezes explicit Release/iphoneos build arguments and the generated scheme configuration.
- `react_device_perf_rejects_unstamped_external_derived_data_reuse()`: freezes the hard cut from existence-only React Native cache imports.
- `device_build_reuse_requires_toolchain_and_artifact_stamp()`: requires the shared exact toolchain/artifact stamp validator for automatic and explicit reuse.
- `paired_reports_serialize_identical_repository_revision()`: proves paired UIKit and Oxide reports publish the same repository ref/HEAD/tree.
- `uikit_version_two_rejects_missing_repository_revision_and_version_one_defaults_it()`: freezes strict version-2 serialization and historical version-1 compatibility.
- Resumable-root stamp tests prove only an exact full evidence identity retains checkpoints; changes to source, toolchain, configuration, artifact, device OS, stage, ordered cases, trace duration, environment, report label, or power input clear them while preserving DerivedData, and legacy build-only stamps fail closed.
- `standalone_device_comparison_failures_precede_all_report_outputs()`: freezes pre-write comparison admission for UIKit, React Native, and Oxide JSON/latest/datestamp writers.
- `ios_host_simulator_architecture_contract_is_arm64_only()`: freezes arm64-only device and Simulator settings in the Oxide host and React Native camera benchmark projects and requires every scripted Rust build to honor the workspace lockfile.
- UIKit and Oxide report-parser tests exercise JSON extraction, case classification, stage/memory/cadence/camera summaries, sharded merge behavior, and strict metric contracts.
- Device-runner tests exercise xctestrun environment generation, resumable result roots, launch/camera/watch controls, console markers, lock/display state, retry classification, process discovery, and case selection.
- Comparison tests exercise simulator-noise allowances, physical-device CPU/GPU/memory/cadence/energy gates, refresh-mode keys, case-set reuse, pre-write promotion gates, and committed-baseline status.
- Trace tests exercise table/schema discovery, duration windows, signpost regions, CPU hotspots, GPU summaries, energy conversion, and unit normalization.
- Private fixture helpers such as `sample_perf_report`, `sample_uikit_report`, and `sample_oxide_device_report` build deterministic inputs and are reached only by tests in this unit.

## Logic narrative

Manifest tests parse the committed TOML through the same production checker used by `cargo xtask experiments check`. The acceptance test first requires important historical ids, then compares the returned summary with the exact committed population so a new experiment must intentionally update the contract. Negative tests isolate expiry, missing A/B policy, and missing decision proof.

The `test-all` source guard isolates the command body and rejects a second
featureless `cargo test` pass while requiring the all-target `cargo check`
replacement. It validates orchestration without recursively launching the
workspace suite from inside an integration test.

Device/report tests construct minimal representative fixtures, call one production helper, and assert both preserved values and rejected gaps. The canonical-selector tests treat row count as a consequence of five matched comparisons: four idiomatic/optimized pairs plus the mandatory pure-custom-NV12/AVCaptureVideoPreviewLayer microscope pair. Comparison tests keep simulator diagnostics separate from physical-device authority and require direct GPU plus cadence distributions where policy says they are mandatory. Trace tests reduce exported tables to bounded workload windows before attributing stages, GPU work, or energy.

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

Run the complete unit with `cargo test --locked -p xtask --test xtask_tests`. Run the current manifest gate with `cargo test --locked -p xtask --test xtask_tests experiment_manifest_checker_accepts_current_manifest`.

## Examples

```rust
let summary = xtask::check_experiment_manifest_text(text, "2026-07-13")?;
assert_eq!(summary.undecided, 0);
```

## Changelog

- 2026-08-07: added full evidence-run invalidation, toolchain/artifact reuse, and the React cache hard cut.
- 2026-08-07: froze the Release/iphoneos configuration used by physical-device build-for-testing.
- 2026-08-07: added source-order coverage proving all three standalone device comparisons reject before resolving or writing report outputs.
- 2026-08-07: added paired-report source identity, versioned provenance, and revision-aware resumable checkpoint coverage.
- 2026-08-07: extended the iOS architecture contract to pin the React Native camera benchmark to arm64 for device and retained Simulator builds.
- 2026-08-07: froze the exact ten-row UIKit and five-row Oxide canonical device inventories, pair metadata, and explicit-only access for noncanonical cases.
- 2026-08-07: Required `test-all` to compile, rather than rerun, the featureless workspace test graph.
- 2026-07-15: froze C60's accepted image-store experiment, two rejected UIKit proof paths, and the 190-entry, 88-accepted, 102-rejected manifest totals.
- 2026-07-14: froze the accepted C35 WebGPU ID-mask field packing and the 170-entry, 81-accepted, 89-rejected manifest totals.
- 2026-07-14: froze the accepted C34 Metal ID-mask field packing, three rejected compositor guardrail refinements, and the 169-entry, 80-accepted, 89-rejected manifest totals.
- 2026-07-14: froze the accepted C33 WebGPU ID-mask field cache, rejected the one-entry cache after the required multi-map proof, and refreshed the 165-entry, 79-accepted, 86-rejected manifest totals.
- 2026-07-14: froze the accepted C32 Metal ID-mask field-cache experiment and the resulting 163-entry, 78-accepted, 85-rejected manifest totals.
- 2026-07-14: froze the C30 accepted local-layer experiment, rejected unsnapped-coordinate, two-pass-key, asymmetric-clock-warmup, single-long-burst, and no-postroll designs, and resulting 160-entry totals.
- 2026-07-13: froze the C29 accepted prepared-layer experiment, three rejected precision policies, and the resulting 154-entry accepted/rejected totals.

- 2026-07-13: refreshed the exact manifest summary after the completed C00--C26 experiment sequence reached 144 entries: 72 accepted and 72 rejected.
