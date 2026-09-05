# `oxide-apple-comparison-controller::launch`

## Intention and purpose

This module defines the fail-closed evidence contract for the macOS Apple PR launch cell. It prevents a prepared application or a trace attached after readiness from being mislabeled as launch measurement.

## Relation to the rest of the code

The macOS comparison controller will materialize this contract around each `canonical-launch` session. A cache primer must complete before the controller starts an all-process Instruments trace; the controller then records its launch request, discovers the exact new PID, and reduces the trace and application receipts to that PID. The presentation endpoint remains calibration-pending until the required external-sensor experiment succeeds.

Call flow:

- macOS launch controller
  - warm-cache primer receipt
  - prelaunch all-process trace barrier
  - exact application launch and PID discovery
  - application lifecycle and complete-UI receipts
  - exact-PID compositor presentation reduction
  - trusted-input first-interactive probe
  - `validate_macos_launch_evidence`

## Entry points list

- `validate_macos_launch_evidence(evidence: &MacOsLaunchEvidence, expected: &MacOsLaunchExpectation) -> Result<()>` validates identity, launch class, trace scope, cache-primer binding, milestone order, and calibration status.
- `MacOsLaunchClass` freezes the Apple PR class to `terminated-process-warm-system-cache`.
- `MacOsLaunchExpectation` carries the preregistered run, plan, generation, executable, side, and launch-class identity.
- `MacOsLaunchEvidence` carries the controller and application milestones needed to prove a real launch and first interaction.
- `MACOS_LAUNCH_PRESENTATION_CALIBRATION_PENDING` is the only accepted status before external presentation calibration.

## Logic narrative

Validation first checks every immutable identity and content hash. It then requires one nonzero launched PID, a trace already running before the launch request, all-process acquisition followed by exact-PID reduction, and the frozen terminated-process/warm-system-cache class. Finally, it requires strict milestone order from cache priming through launch, application lifecycle, complete UI generation, attributed present proxy, trusted input, and the corresponding response generation. Strict ordering rejects direct-callback schedules whose “launch” work happened before the controller request.

## Preconditions and postconditions

All hashes are canonical lowercase SHA-256. Run IDs obey the controller path-safety contract. Tick values share one monotonic clock domain and are nonzero. Success proves contract completeness only; it does not make the presentation proxy claim-capable while calibration is pending. Schema 3 retains both the out-of-process `XCUIElement.click` request timestamp and the later app-observed target/action timestamp. Interaction response starts at the controller request; input delivery remains separately attributable instead of being silently omitted.

## Edge cases and failure modes

Unknown schemas or classes, missing PIDs, incomplete evidence, malformed or changed identities, a late/process-only trace, missing exact-PID filtering, absent cache priming, equal/reversed milestones, or an invented calibration status fail closed.

## Concurrency and memory behavior

Validation is synchronous, allocation-bounded by fixed strings, and has no background work. Live trace and launch ownership remains in the controller rather than the measured application.

## Performance notes

This module is comparison-only and is not linked into production. The prelaunch trace requirement removes the current blind interval without adding instrumentation inside Oxide or AppKit application work.

## Feature flags and cfgs

None.

## Testing and benchmarks

`tests/launch_tests.rs` proves valid terminated-warm evidence and rejects late traces, pre-lifecycle UI generation, changed generations, and malformed cache-primer identities.

## Examples

Construct `MacOsLaunchExpectation` from the frozen campaign session, decode the host/application launch artifact into `MacOsLaunchEvidence`, and call `validate_macos_launch_evidence` before admitting the session.

## Changelog

- 2026-07-19: added the macOS terminated-process/warm-system-cache launch evidence contract.
