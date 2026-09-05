# oxide-benchmark-spec::release_candidate

## Intention and purpose

This module admits the five frozen release candidates for screenshot acquisition without pretending they are runnable performance scenarios. It closes the bootstrap gap in which canonical screenshots are required for promotion but must be generated from the same hash-bound fixture, assets, fonts, traces, state, and accessibility contracts that promotion will later consume.

## Relation to the rest of the code

`oxide-comparison-runtime` calls this module before its capture-only FFI prepares an Oxide scene. `release_promotion` remains the only path that can add screenshots and produce runnable scenario manifests. Normal `ScenarioSpec` loading and validation do not call this module and continue to reject these candidates.

Call graph:

- `oxide_comparison_prepare_release_candidate`
  - `load_release_candidate_for_capture`
    - canonical candidate JSON and admission-field validation
    - transitive asset/font/trace/artifact hash validation
    - typed `ReleaseCandidateCaptureSpec`
  - identity-only comparison scene preparation

## Entry points list

- `oxide_benchmark_spec::load_release_candidate_for_capture(spec_root: &Path, scenario_id: &str) -> anyhow::Result<ReleaseCandidateCaptureSpec>` validates one frozen candidate and returns only data needed by correctness capture.
- `oxide_benchmark_spec::ReleaseCandidateCaptureSpec` carries the candidate SHA-256, identity, fixture, asset and font manifests, scene contract, phases, viewport classes, and ordered checkpoint identities.

## Logic narrative

The loader first rejects identities outside `RELEASE_CANDIDATE_IDS`. It resolves the fixed candidate path beneath the canonical specification root, parses JSON, re-encodes canonical compact JSON plus one newline, and requires byte equality. It then requires schema version one, exact identity, `blocked-on-canonical-screenshots`, the `minimal-presentation` owning pass, blocked screenshot materialization, and the identity-specific `scenarios/<id>.json` destination.

Every direct artifact is resolved beneath the canonical root and checked against its lowercase SHA-256. Asset and font manifests are parsed and their nested images, raster variants, font files, and licenses are validated through the same benchmark-spec validators used by runnable scenarios. Every trace is typed, ordered, contract-valid, and bounded by its phase duration. Every checkpoint must omit screenshot and geometry identities, refer to a real phase, bind hash-matched state and accessibility JSON to the candidate/checkpoint identity, and match the complete visible-role map.

## Preconditions and postconditions

The specification root must exist and contain the committed version-one tree. Success means all capture inputs are immutable and internally consistent, but does not make the candidate eligible for timing. Promotion still requires paired AppKit/Oxide screenshots, geometry, semantic equality, and calibrated visual acceptance.

## Edge cases and failure modes

Unknown IDs, path escape, symlink escape, noncanonical JSON, status or destination drift, missing checkpoints, prebound screenshots, phase overflow, malformed traces, direct or nested hash drift, state/accessibility identity drift, and visible-role drift fail before any scene is prepared.

## Concurrency and memory behavior

Loading is synchronous correctness-control work. It reads bounded manifests and checkpoint JSON into owned buffers and performs no shared mutation. The returned spec owns its strings and vectors and is safe to move between control-plane components.

## Performance notes

This API is never called from frame preparation, draw encoding, submission, or measured campaign phases. Its deliberate file I/O and cryptographic hashing are capture admission costs only, so the production and measured hot paths are unchanged.

## Feature flags and cfgs

None.

## Testing and benchmarks

`tests/release_candidate_tests.rs` accepts all five committed candidates and mutates identity, status, owning pass, destination, canonical encoding, declared hashes, and artifact bytes to prove fail-closed behavior. Runtime and test-scenes suites separately prove capture-only preparation and normal-admission rejection.

## Examples

Call `load_release_candidate_for_capture(root, "grid.large-scroll")`, then pass its validated fixture bytes and identity only to the capture-specific scene preparation path. Do not synthesize a `ScenarioSpec` or screenshot identity.

## Changelog

- 2026-07-21: introduced explicit release-candidate screenshot-capture admission with transitive artifact validation.
