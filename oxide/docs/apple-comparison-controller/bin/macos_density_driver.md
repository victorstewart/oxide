# `oxide-apple-comparison-controller` `macos_density_driver`

## Intention and purpose

This benchmark-only executable connects the `oxide-perf-runner` density request protocol to real AppKit and Oxide macOS comparison-controller sessions without `xctrace`.

## Relation to the rest of the code

`xtask compare-ui calibrate-density` launches this executable with `--platform macos --request PATH --output PATH`. The driver reads absolute `OXIDE_MACOS_DENSITY_BUILD_MANIFEST`, `OXIDE_MACOS_DENSITY_CAMPAIGN_PLAN`, and `OXIDE_MACOS_DENSITY_SESSION_ROOT` paths, resolves exact content-matched packs, calls `run_macos_density_sessions`, reduces integrity-protected telemetry, and emits the typed result.

## Entry points list

- `main()` parses the fixed driver CLI and returns a nonzero status for every identity, lock-state, session, telemetry, metric, resource, or persistence failure.

## Logic narrative

Each treatment runs an isolated sentinel, its ordered singleton or candidate-sized packs, and a final isolated sentinel. The generic campaign plan must contain exact packs whose `isolated_process` flag matches the requested mode. The driver accepts only `scene-update-p50-ns`; any other declared primary metric fails closed instead of relabeling telemetry. It validates the binary-ring header, sequence ranges, plan hash, footer hash, envelope hash, telemetry hash and size, then computes each scenario median from paired `sceneUpdateBegin`/`sceneUpdateEnd` records. A canonical session manifest binds build, plan, request, executable, controller artifact, acknowledgement, resource, telemetry, and session identities.

## Preconditions and postconditions

All configuration paths are absolute, the GUI is explicitly unlocked, the build manifest matches the exact campaign/specification source identity, and every requested pack is selected by the bundled `primary-presentation` pass. Success creates one canonical result and one canonical session manifest using create-new writes.

## Edge cases and failure modes

Locked or unknown GUI state, unsupported metrics, missing singleton sentinel packs, pack-content or isolation mismatches, duplicate estimators, telemetry corruption, incomplete scene updates, nonpositive estimators, thermal warnings, and changed hashes fail closed or produce rejecting guardrail evidence.

## Concurrency and memory behavior

Sessions execute serially. Parsing is bounded by the telemetry header capacity and allocates only in benchmark control-plane code.

## Performance notes

The estimator is symmetric app-owned scene-update duration used only to calibrate session packing. It is not on-glass presentation evidence and cannot support an Oxide-versus-AppKit winner claim.

## Feature flags and cfgs

No feature flags apply; real execution is macOS-only through the controller's GUI gate.

## Testing and benchmarks

`density_acquisition_tests` covers schedule shape and identity admission; controller `lib_tests` covers locked-state parsing. The real headed proof must run only while `IOConsoleLocked` is false.

## Examples

Set the three absolute environment paths, then pass this built executable to `compare-ui calibrate-density --driver`.

## Changelog

- 2026-07-21: Added the real no-`xctrace` AppKit/Oxide density-session driver.
