# oxide-benchmark-spec::comparator_acceptance

## Intention and purpose

`comparator_acceptance` defines the versioned, fail-closed admission contract that every headline reference must pass before Oxide contender data is acquired. It distinguishes generic serde representation of pending or rejected audits from strict publication admission of a fully evidenced, independently reviewed, signed `accepted` audit.

## Relation to the rest of the code

- A comparison plan's `reference_audit_sha256` binds the exact canonical audit bytes passed to admission.
- Source files, dependencies, build recipe, selection evidence, checklist evidence, profiles, gates, ceiling preregistration, and detached reviewer-signature evidence are content-addressed workspace-relative artifacts.
- `admit_comparator_acceptance` is a control-plane validator. It does not run inside application, renderer, or measured benchmark paths.
- The committed AppKit artifact at `benchmarks/comparative/specs/v1/audits/macos-appkit-native-production.json` is intentionally `rejected`; it records the current single-custom-`NSView`, manual-input, synthesized-accessibility failure without claiming reviewer approval.

Call flow:

- plan-selected audit path and SHA-256
  - exact artifact read and hash check
  - canonical JSON parse
  - expected comparator identity and retained-scenario coverage
  - source/evidence/profile/signature artifact hash checks
  - checklist, preregistration, hotspot, gate, ceiling-gap, and signoff admission

## Entry points list

- `ComparatorAcceptanceAudit` stores schema version, overall status, platform/framework/implementation/variant identity, frozen source/build inputs, reviewer, preregistration disclosure, fixed checklists, per-scenario profiles, gates, optional native ceiling, signoff, and rejection reasons.
- `ComparatorAdmissionExpectation` supplies the exact expected comparator identity, retained scenario IDs, and primary comparison-cell IDs.
- `admit_comparator_acceptance(workspace_root, audit_artifact, expected)` verifies the audit and every referenced artifact before returning an admitted audit.
- `canonical_comparator_acceptance_json(audit)` emits canonical pretty JSON with a trailing newline.
- `comparator_acceptance_signed_payload_sha256(audit)` hashes the canonical audit with `reviewer_signoff` removed, allowing a detached signoff to bind every reviewed field without a circular audit hash.
- `ProductionArchitectureChecklist` fixes dispositions for virtualization/reuse, layout, text, image decode/cache, animation/compositing, input, accessibility, and cleanup.
- `ForbiddenWorkChecklist` fixes dispositions for debug work, synchronous sleeps, benchmark logging, forced layout/render loops, accidental full-tree rebuild, unbounded native/DOM growth, and harness profiler hotspots.
- `ScalingChecklist` contains exact `1x` and `2x` checks.
- `ScenarioBoundedProfile` identifies one bounded retained-scenario profile and enumerates top CPU stacks and stalls.
- `NativeCeilingAudit` represents an optional preregistered, separately labeled `native.ceiling` sanity track and its primary-cell gaps.

## Logic narrative

Admission first verifies the caller-supplied audit SHA-256 and canonical byte representation. An accepted audit must match the exact expected identity, carry nonempty source-tree/dependency/build-flag snapshots, and reference only normalized workspace-relative artifact paths. The reviewer must disclose current production experience in the named framework, role, identity, and independence from the Oxide implementation. Selection must be evidenced as occurring before Oxide results were known.

Every fixed architecture, forbidden-work, scaling, parity, artifact, and release-build disposition must be `pass`, contain a rationale, and point to verified evidence. Retained-scenario profile IDs must exactly equal the expected set. Every listed top CPU stack at or above 500 basis points of scenario CPU and every stall at or above that profile's refresh interval requires a passing disposition with evidence.

An optional ceiling must be preregistered, use the `native.ceiling` variant, prohibit headline substitution, and cover the expected primary cells. A `native.production` gap greater than 1,000 basis points blocks admission unless it has a passing expert disposition. Finally, the accepted signoff must bind the recomputed unsigned-payload SHA-256 and detached signature evidence. Pending and rejected audits remain serializable and reviewable but cannot pass this function.

## Preconditions and postconditions

- `workspace_root` is the Cargo workspace against which every artifact path is resolved.
- `audit_artifact.sha256` came from the comparison plan's frozen `reference_audit_sha256` identity.
- Expected scenarios and primary cells were frozen before contender acquisition.
- Success proves the exact audit is canonical, accepted, complete for the expected identity and scenarios, fully signed, and free of missing or changed referenced evidence.

## Edge cases and failure modes

Admission rejects missing or changed audit bytes, path traversal, malformed hashes, pending/rejected status, identity drift, incomplete reviewer independence or experience, retrospective selection, incomplete fixed checklists, missing 1x/2x evidence, missing or duplicate scenario profiles, undisposed threshold stacks/stalls, failed parity/artifact/release gates, retrospective ceiling substitution, incomplete ceiling-cell coverage, unexplained greater-than-10-percent ceiling gaps, stale signed-payload hashes, and missing or changed signature evidence.

## Concurrency and memory behavior

Validation is synchronous, read-only, and has no global state. It reads bounded control-plane audit and evidence files into owned buffers; callers keep it outside measured sessions.

## Performance notes

This module is in the separate `oxide-benchmark-spec` crate and makes no production-runtime changes. Hashing and JSON serialization occur only before acquisition, so application and renderer performance are neutral.

## Feature flags and cfgs

No feature or target cfg changes the audit schema or admission behavior.

## Testing and benchmarks

`tests/comparator_acceptance_tests.rs` proves valid signed admission and fail-closed handling for missing/changed audit and evidence, pending status, missing retained-scenario profile, undisposed 5-percent CPU stack, undisposed one-refresh stall, retrospective selection, and unexplained greater-than-10-percent ceiling gap. It also freezes the canonical truthful rejected AppKit artifact and checks its source hashes.

## Examples

Build an `ArtifactIdentity` from the plan's audit path and `reference_audit_sha256`, construct the exact `ComparatorAdmissionExpectation`, then call `admit_comparator_acceptance` before any contender process is launched.

## Changelog

- 2026-07-19: added ComparatorAcceptance v1, strict signed admission, threshold hotspot and ceiling-gap enforcement, and the rejected current AppKit audit artifact.
