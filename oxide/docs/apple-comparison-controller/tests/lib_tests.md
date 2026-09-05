# `oxide-apple-comparison-controller` `tests/lib_tests.rs`

## Intention and purpose

These integration tests freeze the externally observable macOS controller plan, controller-owned resource collection contract, and the rule that comparison harness implementation cannot leak into production source roots.

## Relation to the rest of the code

The tests load the committed Apple PR acquisition through the public controller API. They do not launch applications or collect timings. Benchmark/spec/snapshot/test-scene crates are explicit non-shipping allow-list entries; production crates and hosts remain scanned.

## Entry points list

- `macos_plan_contains_correctness_then_four_balanced_pairs()` verifies exact session counts, content-derived decimal seed, and analyzer-compatible seeded ABBA/BAAB order evidence.
- `correctness_scope_selects_only_the_four_untimed_sessions()` verifies the Stage-1 scope cannot select presentation or launch sessions while full scope still selects all twenty.
- `correctness_reducer_accepts_the_complete_calibrated_pack_and_binds_evidence_paths()` verifies all launch checkpoints accept only with schema-5 calibrated pixels plus root-bound screenshot and runtime-geometry identities.
- `schema_one_pair_checkpoint_fails_closed_without_overwrite_or_launch()` verifies legacy checkpoints remain byte-identical and cannot cause an app launch without complete recoverable side evidence.
- `measured_pairs_are_owned_by_the_live_exact_static_gate()` freezes reducer persistence and the gate before measured acquisition.
- `session_arguments_bind_every_identity_and_output_root()` verifies immutable invocation arguments.
- `build_manifest_admission_rehashes_complete_application_bundles()` constructs two complete application identities, admits the unchanged build, and proves a resource-only mutation fails before launch.
- `run_plan_rejects_path_traversal_and_noncanonical_hashes()` verifies control-plane path and identity rejection.
- `comparison_harness_markers_remain_outside_production_sources()` scans production source roots for comparison-only campaign markers.
- `trace_exports_require_concrete_hitches_schemas_and_all_generation_markers()` verifies the concrete process-update/frame-lifetime schemas and all marker families are mandatory.
- `resource_collector_contract_stays_controller_owned_and_starts_at_pid_discovery()` verifies fixed-cadence V4 sampling starts at exact PID discovery, takes a terminal sample, and hashes its artifact into receipts.
- `comparison_xcode_builds_never_write_the_workspace_cargo_target()` verifies both the canonical XcodeGen source and generated project use an Xcode-owned target, invoke the 4 GiB fail-closed guard, and cannot regress to the workspace target.
- `pending_checkpoint_recaptures_allow_correctness_but_block_qualification_and_full()` verifies correctness remains available for reference replacement while both measured scopes fail admission on the exact selected checkpoint metadata.
- `density_gui_gate_rejects_locked_or_unknown_console_state()` verifies density acquisition admits only explicit unlocked `IOConsoleLocked` evidence.

## Logic narrative

Tests treat the committed acquisition as input, expand it, and assert only public results. The source-plan SHA-256 deterministically supplies the recorded seed; both PR and generic plans must expand it through benchmark-spec's four-pair block algorithm, and pair validation rejects order fields that disagree with launch-side order. Scope selection proves the correctness-only command is a strict projection of the frozen campaign rather than a second plan. The build-admission fixture duplicates the build command's deterministic bundle-tree digest and mutates only an Oxide resource, proving executable-only validation cannot admit changed application contents. The boundary test requires the dedicated Apple and Web comparison manifests and rejects campaign-controller names or arguments in production crates and hosts. The source contract confirms resource collection remains in the controller crate and cannot silently move after app activation. The Xcode target test inspects both configuration layers so regenerating the project cannot restore workspace-wide build accumulation.

## Preconditions and postconditions

The repository has its committed comparison specifications. A pass proves plan construction and current source placement; it does not prove live acquisition or pixel parity.

## Edge cases and failure modes

Traversal run IDs, uppercase hashes, malformed build identities, and changed application-bundle files are rejected. Any new comparison marker in a production source file fails with the exact path.

## Concurrency and memory behavior

Tests are single-process, bounded, and do not spawn comparator applications.

## Performance notes

No measured path is exercised.

## Feature flags and cfgs

None.

## Testing and benchmarks

Run `cargo test --locked -p oxide-apple-comparison-controller`.

## Examples

The tests are invoked through Cargo and require no flags.

## Changelog

- 2026-07-26: froze schema-5 correctness reports around calibrated visual algorithm v4.
- 2026-07-21: covered locked, unlocked, and missing kernel GUI-lock state.

- 2026-07-21: covered prelaunch pending-recapture admission for correctness, qualification, and full scopes.
- 2026-07-21: froze content-derived seeded ABBA/BAAB expansion and analyzer-compatible per-session order evidence for PR and generic macOS plans.
- 2026-07-21: correctness fixtures now include hash-bound runtime geometry and assert schema-3 profile/scale acceptance.
- 2026-07-19: added controller plan and source-boundary coverage.
- 2026-07-19: added resource collector ownership, cadence, terminal-sample, and receipt-hash coverage.
- 2026-07-19: covered strict correctness-only versus full-session selection.
- 2026-07-19: tightened trace qualification to the schemas required by the swap-ID correlator.
- 2026-07-19: added complete application-bundle admission and resource-mutation rejection coverage.
- 2026-07-19: added controller-owned exact-static reduction, measured-acquisition gate, and non-overwriting schema-1 recovery coverage.
- 2026-07-20: covered the XcodeGen source and generated-project Cargo target isolation and size fuse.
