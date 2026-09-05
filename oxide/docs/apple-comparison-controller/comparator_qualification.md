# `oxide-apple-comparison-controller` `src/comparator_qualification.rs`

> **Extended diagnostic path:** the 13-scenario, two-scale, two-side Time
> Profiler matrix is not a mainline admission gate. Use it only when explicitly
> requested or when a measured loss specifically requires scale/profile
> attribution. The rapid lane is defined in
> `.tasks/plan-production-comparison-benchmarks.md`.

## Intention and purpose

This benchmark-only module defines and reduces the macOS AppKit/Oxide comparator-qualification contract. It freezes exact 1x and 2x workload variants, requires one bounded Time Profiler artifact for every retained scenario, side, and scale, and prevents comparator admission unless significant CPU stacks and stalls have explicit passing dispositions.

## Relation to the rest of the code

The qualification controller consumes content-addressed fixtures, scale overlays, runtime attestations, and `MacOsTimeProfilerArtifact` files. Its output supplies the scaling and bounded-profile evidence required by `oxide-benchmark-spec` comparator acceptance; it never enters an Oxide production crate or measured renderer path.

Call flow:

- qualification acquisition writes frozen overlays and exact runtime attestations
- `validate_macos_comparator_qualification_plan` verifies the complete 1x/2x matrix and artifact closure
- `reduce_macos_comparator_qualification` verifies all AppKit/Oxide profiles and emits threshold findings
- comparator audit review disposes every surfaced finding before admission

## Entry points list

- `canonical_macos_comparator_qualification_plan_json(plan: &MacOsComparatorQualificationPlan) -> Result<Vec<u8>>` emits deterministic plan bytes.
- `validate_macos_comparator_qualification_plan(workspace_root: &Path, plan: &MacOsComparatorQualificationPlan, retained_scenario_ids: &[String]) -> Result<BTreeMap<...>>` verifies exact scenario/scale coverage, canonical overlays, and fixture hashes.
- `reduce_macos_comparator_qualification(workspace_root: &Path, plan: &MacOsComparatorQualificationPlan, retained_scenario_ids: &[String], evidence: &[MacOsComparatorProfileEvidence], dispositions: &[MacOsComparatorFindingDisposition]) -> Result<MacOsComparatorQualificationReport>` reduces the complete side/scale matrix.
- Public plan, overlay, evidence, attestation, finding, and report structures expose the serialized qualification boundary.

## Logic narrative

Each retained scenario must have one canonical 1x overlay and one canonical 2x overlay. Startup, feed, grid, chat, mutation, and text scale dataset cardinality; dashboard, navigation, image, effects, resize, idle, and endurance scale declared operation cardinality. The 2x effective cardinality must be exactly twice the positive base and both scales must bind the same original fixture and dimension.

For every scenario, AppKit and Oxide each provide one profile at each scale. The reducer reads a content-addressed runtime attestation rather than trusting caller metadata. That attestation must bind the side, scenario, scale, overlay hash, effective cardinality, a single application run, and completed work. It then verifies the exact Time Profiler artifact and aggregates stack weights across measured phases. Stacks at or above 500 basis points and stalls at or above one declared refresh interval are emitted with matching dispositions. Parity, artifact, and Release-build gates remain independent mandatory inputs. A report is accepted only when all gates and all surfaced dispositions pass.

## Preconditions and postconditions

Artifact paths are normalized workspace-relative paths and identities are lowercase SHA-256. Profile windows are 1 through 30 seconds, refresh intervals are nonzero, retained IDs are unique and nonempty, and the evidence matrix is exact. Success returns a deterministic report; acceptance additionally proves every required gate and disposition passed.

## Edge cases and failure modes

Missing or duplicate variants/profiles, changed fixture/overlay/profile/attestation bytes, unknown scenarios, incorrect scale dimensions, overflow, zero work, asymmetric 1x/2x fixtures, a second application run, mismatched effective cardinality, incomplete profiler totals, and unexpected evidence fail closed. Missing or failing finding dispositions produce a non-accepted report while preserving the finding for review.

## Concurrency and memory behavior

Reduction is single-threaded and offline. Maps and vectors are bounded by retained scenarios, four profiles per scenario, at most the persisted profiler stacks, and declared stalls. No code is linked into measured applications.

## Performance notes

Reduction performs linear artifact reads plus ordered-map aggregation and is outside measurement. The runtime attestation boundary is intended to prove real overlay consumption without charging transformation verification to a production path.

## Feature flags and cfgs

None.

## Testing and benchmarks

`tests/comparator_qualification_tests.rs` covers all 13 release scenario IDs, both scales, both sides, deterministic output, exact 5% CPU and one-refresh stall boundaries, all visual/artifact/Release gates, changed artifacts, missing matrix entries, mismatched overlays, and the one-run rule.

## Examples

Create canonical overlay and attestation artifacts during a bounded qualification acquisition, then call the reducer with the retained scenario list from the exact release plan. Do not use a report as comparator-admission evidence unless `accepted` is true.

## Changelog

- 2026-07-21: introduced the all-release-scenario 1x/2x qualification contract, content-addressed runtime attestations, and bounded profile reducer.
