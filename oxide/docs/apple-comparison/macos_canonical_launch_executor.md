# macOS canonical launch executor

`host/apple-comparison/Shared/ComparatorApp/MacOSCanonicalLaunchExecutor.swift` is shared benchmark-only application infrastructure for the paired AppKit and Oxide macOS startup cell. It is compiled into both comparator targets but does not enter a production Oxide crate or shipping host.

## Contract

The executor accepts the generic-plan launch classes `terminated-warm-system-cache`, `fresh-install-first-launch`, and `warm-resume`. It loads `startup.first-screen`, applies the exact frozen lifecycle trace for the selected class, and validates `terminated-ready`, `fresh-install-ready`, or `warm-resume-ready` respectively. A launch class cannot be relabeled as another class in its application, controller, preparation, or final evidence receipt.

The state sequence is:

```text
applicationDidFinish entry
  -> durable application-did-finish receipt
  -> adapter prepare and class-specific initial lifecycle trace
  -> warm-resume only: controller proves background and SIGSTOP suspension,
     sends SIGCONT, activates the same PID, and signals the app
  -> visual-generation 1 marker and first complete UI
  -> frozen class-specific ready validation
  -> arm real target-action probe
  -> durable ready receipt
  -> controller injects trusted OS input
  -> target-action changes state generation
  -> input-received marker
  -> visual-generation 2 marker
  -> response complete UI
  -> durable complete receipt and hash acknowledgement
```

The application delegate timestamp is captured immediately on method entry before artifact checks or fixture I/O. The response-generation marker is emitted inside trusted target-action handling, after the state transition and before the subsequent draw. A separate `responseCompleteUITimestamp` proves that UI completion followed generation.

All durable identities bind run, plan, chunk, pass, pack, pair, side, generation, scenario, launch class, and cache class. Existing artifacts fail closed and are never overwritten. Completion requires an increased state generation and the real window number, coordinates, target identity, and action identity from the probe.

The Rust controller owns launch preparation. Terminated/warm-cache runs execute the cache primer before the measured cold process launch. Fresh-install runs copy the verified `.app` with `ditto` into a previously absent session-owned install root, rehash the copied executable, require a previously absent data root, and require the launched app to claim that empty root with the frozen generation. The exact copied executable path is used for PID discovery and exit checks. Warm-resume runs use the verified installed app, require XCTest to observe it backgrounded, prove `SSTOP` through `proc_pidinfo`, resume with `SIGCONT`, and require the foreground process path and PID to remain identical. Missing or ambiguous proof fails closed.

## Integration state

The source is present in both macOS comparator targets and the unit-test target. Both delegates route `canonical-launch` through the shared executor, while the out-of-process controller provisions the selected launch class, injects trusted input, records clock anchors, and persists typed lifecycle proof. Rust validates the application, controller, preparation, resource, exact-PID presentation, and final evidence receipts before accepting a session.

Presentation calibration remains a separate eligibility gate. These classes are executable and fail closed on incomplete lifecycle proof, but their presentation-derived numbers remain non-authoritative while `pending-external-sensor-endpoint-and-trace-overhead-calibration` is recorded.

## Verification

```text
xcodebuild -project host/apple-comparison/AppleComparison.xcodeproj -scheme AppKitComparison -configuration Debug -destination 'platform=macOS,arch=arm64' test
xcodebuild -project host/apple-comparison/AppleComparison.xcodeproj -target AppKitBenchMacOS -configuration Release build
xcodebuild -project host/apple-comparison/AppleComparison.xcodeproj -target OxideBenchMacOS -configuration Release ARCHS=arm64 ONLY_ACTIVE_ARCH=YES build
```

The tests and builds do not launch either comparator application.

## Changelog

- 2026-07-19: Added the shared terminated-process/warm-system-cache executor, durable evidence sequence, real target-action probe contract, ordered response generation, and focused tests.
- 2026-07-21: Added generic-plan fresh-install and warm-resume execution, session-owned install/data identities, exact suspended/resumed PID proof, and class-specific application/controller/preparation evidence.
