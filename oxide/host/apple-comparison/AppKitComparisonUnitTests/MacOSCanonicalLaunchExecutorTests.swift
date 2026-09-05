#if os(macOS)
import Foundation
import XCTest

final class MacOSCanonicalLaunchExecutorTests: XCTestCase
{
   func testTerminatedWarmCacheLaunchPersistsOrderedReceiptsAndTrustedResponse() throws
   {
      let temporary = try temporaryRoot()
      let loader = BenchmarkSpecLoader(root: try specRoot())
      let events = LaunchTestEvents()
      let adapter = try LaunchTestAdapter(loader: loader, events: events)
      let probe = LaunchTestProbe()
      let clock = LaunchTestClock([100, 110, 200, 210, 300, 310, 400])
      let markers = LaunchTestMarkers(events: events)
      let invocation = try launchInvocation()
      let executor = MacOSCanonicalLaunchExecutor(
         invocation: invocation,
         loader: loader,
         adapter: adapter,
         probeTarget: probe,
         store: DurableArtifactStore(root: temporary),
         clock: clock,
         markers: markers
      )

      try executor.applicationDidFinishLaunching()
      XCTAssertEqual(events.values, ["adapter.prepare", "marker:visualGeneration:1"])
      XCTAssertEqual(adapter.appliedEventCount, 1)
      let ready = try executor.firstCompleteUIObserved()
      XCTAssertTrue(executor.isWaitingForTrustedInput)
      XCTAssertEqual(ready.identity.launchClass, "terminated-warm-system-cache")
      XCTAssertEqual(ready.identity.cacheClass, "warm-system-cache")
      XCTAssertEqual(ready.trustedInputOffsetNs, 50_000_000)
      XCTAssertEqual(ready.initialStateGeneration, 7)

      try probe.deliverTargetAction()
      XCTAssertEqual(events.values, [
         "adapter.prepare",
         "marker:visualGeneration:1",
         "marker:inputReceived:1",
         "marker:visualGeneration:2",
      ])
      let complete = try executor.responseCompleteUIObserved()
      XCTAssertTrue(executor.isComplete)
      XCTAssertEqual(complete.applicationDidFinishTimestamp, 100)
      XCTAssertEqual(complete.firstCompleteUITimestamp, 200)
      XCTAssertEqual(complete.readinessTimestamp, 210)
      XCTAssertEqual(complete.trustedInputReceivedTimestamp, 300)
      XCTAssertEqual(complete.responseGenerationTimestamp, 310)
      XCTAssertEqual(complete.responseCompleteUITimestamp, 400)
      XCTAssertEqual(complete.initialStateGeneration, 7)
      XCTAssertEqual(complete.responseStateGeneration, 8)
      XCTAssertEqual(complete.responseVisualGeneration, 2)
      XCTAssertEqual(adapter.appliedEventCount, 1)
      XCTAssertEqual(events.values, [
         "adapter.prepare",
         "marker:visualGeneration:1",
         "marker:inputReceived:1",
         "marker:visualGeneration:2",
      ])

      let prefix = "Runs/run/launch-pairs-0-3/canonical-launch/0/native"
      let finished = try decode(MacOSCanonicalLaunchApplicationDidFinishReceipt.self, root: temporary, path: "\(prefix).launch.application-did-finish.json")
      let firstUI = try decode(MacOSCanonicalLaunchFirstCompleteUIReceipt.self, root: temporary, path: "\(prefix).launch.first-complete-ui.json")
      let storedReady = try decode(MacOSCanonicalLaunchReadinessReceipt.self, root: temporary, path: "\(prefix).launch.ready.json")
      let acknowledgement = try decode(MacOSCanonicalLaunchAcknowledgement.self, root: temporary, path: "\(prefix).launch.complete.ack.json")
      let completeData = try Data(contentsOf: temporary.appendingPathComponent("\(prefix).launch.complete.json"))
      XCTAssertEqual(finished.applicationDidFinishTimestamp, 100)
      XCTAssertEqual(firstUI.generationMarkerTimestamp, 110)
      XCTAssertEqual(storedReady, ready)
      XCTAssertEqual(acknowledgement.artifactSHA256, comparisonSHA256(completeData))
      XCTAssertTrue(acknowledgement.durable)
      XCTAssertTrue(probe.disarmed)
   }

   func testFreshInstallAndWarmResumeUseTheirExactLifecycleTracesAndCheckpoints() throws
   {
      let loader = BenchmarkSpecLoader(root: try specRoot())
      for (launchClass, expectedEvents, expectedCheckpoint) in [
         (MacOSCanonicalLaunchClass.freshInstallFirstLaunch, 2, "fresh-install-ready"),
         (MacOSCanonicalLaunchClass.warmResume, 4, "warm-resume-ready"),
      ]
      {
         let adapter = try LaunchTestAdapter(loader: loader, events: LaunchTestEvents())
         let temporary = try temporaryRoot()
         let dataRoot = temporary.appendingPathComponent("fresh-data", isDirectory: true)
         if launchClass == .freshInstallFirstLaunch
         {
            try FileManager.default.createDirectory(at: dataRoot, withIntermediateDirectories: false)
         }
         let executor = MacOSCanonicalLaunchExecutor(
            invocation: try launchInvocation(launchClass: launchClass, dataRoot: launchClass == .freshInstallFirstLaunch ? dataRoot : nil),
            loader: loader,
            adapter: adapter,
            probeTarget: LaunchTestProbe(),
            store: DurableArtifactStore(root: temporary),
            clock: LaunchTestClock([100, 110, 200, 210]),
            markers: LaunchTestMarkers(events: LaunchTestEvents())
         )

         try executor.applicationDidFinishLaunching()
         if launchClass == .warmResume
         {
            XCTAssertFalse(executor.isWaitingForTrustedInput)
            try executor.warmResumeObserved()
         }
         let ready = try executor.firstCompleteUIObserved()

         XCTAssertEqual(adapter.appliedEventCount, expectedEvents)
         XCTAssertEqual(ready.identity.launchClass, launchClass.rawValue)
         let prefix = "Runs/run/launch-pairs-0-3/canonical-launch/0/native"
         let first = try decode(MacOSCanonicalLaunchFirstCompleteUIReceipt.self, root: temporary, path: "\(prefix).launch.first-complete-ui.json")
         XCTAssertEqual(first.checkpointID, expectedCheckpoint)
      }
   }

   func testLaunchRejectsMissingTargetActionTransitionAndDoesNotComplete() throws
   {
      let temporary = try temporaryRoot()
      let loader = BenchmarkSpecLoader(root: try specRoot())
      let adapter = try LaunchTestAdapter(loader: loader, events: LaunchTestEvents())
      let probe = LaunchTestProbe()
      let executor = MacOSCanonicalLaunchExecutor(
         invocation: try launchInvocation(),
         loader: loader,
         adapter: adapter,
         probeTarget: probe,
         store: DurableArtifactStore(root: temporary),
         clock: LaunchTestClock([1, 2, 3, 4]),
         markers: LaunchTestMarkers(events: LaunchTestEvents())
      )
      try executor.applicationDidFinishLaunching()
      _ = try executor.firstCompleteUIObserved()
      XCTAssertThrowsError(try executor.trustedTargetActionObserved())
      {
         XCTAssertEqual($0 as? MacOSCanonicalLaunchFailure, .stateDidNotTransition)
      }
      XCTAssertFalse(executor.isComplete)
      XCTAssertFalse(FileManager.default.fileExists(atPath: temporary.appendingPathComponent("Runs/run/launch-pairs-0-3/canonical-launch/0/native.launch.complete.json").path))
   }

   func testApplicationDidFinishUsesTheDelegateEntryTimestampWithoutSamplingTheClock() throws
   {
      let temporary = try temporaryRoot()
      let loader = BenchmarkSpecLoader(root: try specRoot())
      let adapter = try LaunchTestAdapter(loader: loader, events: LaunchTestEvents())
      let clock = LaunchTestClock([110])
      let executor = MacOSCanonicalLaunchExecutor(
         invocation: try launchInvocation(),
         loader: loader,
         adapter: adapter,
         probeTarget: LaunchTestProbe(),
         store: DurableArtifactStore(root: temporary),
         clock: clock,
         markers: LaunchTestMarkers(events: LaunchTestEvents())
      )

      try executor.applicationDidFinishLaunching(timestamp: 99)

      let receipt = try decode(
         MacOSCanonicalLaunchApplicationDidFinishReceipt.self,
         root: temporary,
         path: "Runs/run/launch-pairs-0-3/canonical-launch/0/native.launch.application-did-finish.json"
      )
      XCTAssertEqual(receipt.applicationDidFinishTimestamp, 99)
      XCTAssertEqual(clock.invocationCount, 1)
   }

   func testGenericCanonicalLaunchValidatesTheContentAddressedPlanAndLaunchSelection() throws
   {
      let root = try specRoot()
      let planPath = "plans/nightly-apple.json"
      let planData = try Data(contentsOf: root.appendingPathComponent(planPath))
      let loader = BenchmarkSpecLoader(root: root)
      let adapter = try LaunchTestAdapter(loader: loader, events: LaunchTestEvents())
      let invocation = try MacOSCanonicalLaunchInvocation(
         runID: "nightly-run",
         planSHA256: comparisonSHA256(planData),
         chunkID: "canonical-launch",
         pairIndex: 0,
         side: .native,
         generation: String(repeating: "b", count: 64),
         packID: "launch",
         planArtifact: BenchmarkArtifactIdentity(path: planPath, sha256: comparisonSHA256(planData)),
         trustedInputOffsetNs: 50_000_000,
         trustedInputDeadlineNs: 2_000_000_000
      )
      let executor = MacOSCanonicalLaunchExecutor(
         invocation: invocation,
         loader: loader,
         adapter: adapter,
         probeTarget: LaunchTestProbe(),
         store: DurableArtifactStore(root: try temporaryRoot()),
         clock: LaunchTestClock([100, 110]),
         markers: LaunchTestMarkers(events: LaunchTestEvents())
      )

      try executor.applicationDidFinishLaunching()

      XCTAssertEqual(invocation.identity.packID, "launch")
      XCTAssertEqual(invocation.planArtifact?.path, planPath)
      XCTAssertEqual(adapter.prepareCount, 1)
   }

   func testLaunchIsFailClosedOnExistingEvidenceAndUnsupportedTiming() throws
   {
      XCTAssertThrowsError(try MacOSCanonicalLaunchInvocation(
         runID: "run",
         planSHA256: String(repeating: "a", count: 64),
         chunkID: "launch-pairs-0-3",
         pairIndex: 0,
         side: .native,
         generation: String(repeating: "b", count: 64),
         trustedInputOffsetNs: 0,
         trustedInputDeadlineNs: 1
      ))

      let temporary = try temporaryRoot()
      let prefix = temporary.appendingPathComponent("Runs/run/launch-pairs-0-3/canonical-launch/0")
      try FileManager.default.createDirectory(at: prefix, withIntermediateDirectories: true)
      let existing = prefix.appendingPathComponent("native.launch.application-did-finish.json")
      try Data("preserve".utf8).write(to: existing)
      let loader = BenchmarkSpecLoader(root: try specRoot())
      let adapter = try LaunchTestAdapter(loader: loader, events: LaunchTestEvents())
      let clock = LaunchTestClock([1])
      let executor = MacOSCanonicalLaunchExecutor(
         invocation: try launchInvocation(),
         loader: loader,
         adapter: adapter,
         probeTarget: LaunchTestProbe(),
         store: DurableArtifactStore(root: temporary),
         clock: clock,
         markers: LaunchTestMarkers(events: LaunchTestEvents())
      )
      XCTAssertThrowsError(try executor.applicationDidFinishLaunching())
      XCTAssertEqual(clock.invocationCount, 1)
      XCTAssertEqual(try Data(contentsOf: existing), Data("preserve".utf8))
      XCTAssertEqual(adapter.prepareCount, 0)
   }

   private func launchInvocation(launchClass: MacOSCanonicalLaunchClass = .terminatedWarmSystemCache, dataRoot: URL? = nil) throws -> MacOSCanonicalLaunchInvocation
   {
      try MacOSCanonicalLaunchInvocation(
         runID: "run",
         planSHA256: String(repeating: "a", count: 64),
         chunkID: "launch-pairs-0-3",
         pairIndex: 0,
         side: .native,
         generation: String(repeating: "b", count: 64),
         launchClass: launchClass,
         dataRoot: dataRoot,
         trustedInputOffsetNs: 50_000_000,
         trustedInputDeadlineNs: 2_000_000_000
      )
   }

   private func temporaryRoot() throws -> URL
   {
      let root = FileManager.default.temporaryDirectory.appendingPathComponent("macos-canonical-launch-\(UUID().uuidString)", isDirectory: true)
      try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
      addTeardownBlock {try? FileManager.default.removeItem(at: root)}
      return root
   }

   private func decode<T: Decodable>(_ type: T.Type, root: URL, path: String) throws -> T
   {
      try JSONDecoder().decode(type, from: Data(contentsOf: root.appendingPathComponent(path)))
   }

   private func specRoot() throws -> URL
   {
      let source = URL(fileURLWithPath: #filePath)
      let root = source
         .deletingLastPathComponent()
         .deletingLastPathComponent()
         .deletingLastPathComponent()
         .deletingLastPathComponent()
         .appendingPathComponent("benchmarks/comparative/specs/v1", isDirectory: true)
      guard FileManager.default.fileExists(atPath: root.path) else
      {
         throw MacOSCanonicalLaunchFailure.invalidInvocation("spec root")
      }
      return root
   }
}

private final class LaunchTestEvents
{
   var values = [String]()
}

private final class LaunchTestAdapter: BenchmarkScenarioAdapter, BenchmarkPassConfiguredAdapter
{
   private let checkpoints: [String: BenchmarkAdapterCheckpoint]
   private let events: LaunchTestEvents
   private var currentCheckpointID = "terminated-ready"
   private(set) var prepareCount = 0
   private(set) var appliedEventCount = 0

   init(loader: BenchmarkSpecLoader, events: LaunchTestEvents) throws
   {
      let scenario = try loader.loadScenario(relativePath: "scenarios/startup.first-screen.json")
      checkpoints = try Dictionary(uniqueKeysWithValues: scenario.parityCheckpoints.map
      {
         checkpoint in
         (
            checkpoint.id,
            BenchmarkAdapterCheckpoint(
               state: try loader.read(checkpoint.state),
               accessibility: try loader.read(checkpoint.accessibility),
               visibleRoleCounts: checkpoint.expectedVisibleRoleCounts
            )
         )
      })
      self.events = events
   }

   func configure(passID: String)
   {
      precondition(passID == "canonical-launch")
   }

   func prepare(scenario: BenchmarkScenario, loader: BenchmarkSpecLoader) throws
   {
      prepareCount += 1
      events.values.append("adapter.prepare")
      XCTAssertEqual(scenario.id, "startup.first-screen")
   }

   func reset() throws {}

   func apply(event: BenchmarkTraceEvent) throws
   {
      appliedEventCount += 1
      switch event.stateId
      {
      case "startup:launch-requested": currentCheckpointID = "terminated-ready"
      case "startup:fresh-install-ready": currentCheckpointID = "fresh-install-ready"
      case "startup:resume-requested": currentCheckpointID = "warm-resume-ready"
      default: break
      }
   }

   func checkpoint(id: String) throws -> BenchmarkAdapterCheckpoint
   {
      guard id == currentCheckpointID, let checkpoint = checkpoints[id] else
      {
         throw MacOSCanonicalLaunchFailure.invalidInvocation(id)
      }
      return checkpoint
   }

   func teardown() throws {}
}

private final class LaunchTestProbe: MacOSCanonicalLaunchProbeTarget
{
   var stateGeneration = UInt64(7)
   var disarmed = false
   private var observer: (() -> Void)?

   func armTrustedInputProbe(_ observer: @escaping () -> Void) throws -> MacOSCanonicalLaunchProbeDescriptor
   {
      self.observer = observer
      return MacOSCanonicalLaunchProbeDescriptor(
         schemaVersion: 1,
         probeID: "startup-primary-control",
         dispatchPath: "trusted-os-input-target-action",
         targetIdentity: "StartupPrimaryControlTarget",
         actionIdentity: "activatePrimaryControl:",
         windowNumber: 1,
         xPoints: 195,
         yPoints: 800
      )
   }

   func disarmTrustedInputProbe()
   {
      disarmed = true
      observer = nil
   }

   func deliverTargetAction() throws
   {
      guard let observer else
      {
         throw MacOSCanonicalLaunchFailure.invalidState("unarmed probe")
      }
      stateGeneration += 1
      observer()
   }
}

private final class LaunchTestClock: MacOSCanonicalLaunchClock
{
   private var values: [UInt64]
   private(set) var invocationCount = 0

   init(_ values: [UInt64])
   {
      self.values = values.reversed()
   }

   func now() -> UInt64
   {
      precondition(!values.isEmpty)
      invocationCount += 1
      return values.removeLast()
   }
}

private struct LaunchTestMarkers: MacOSCanonicalLaunchMarkerSink
{
   let events: LaunchTestEvents

   func emit(_ marker: MacOSCanonicalLaunchMarker)
   {
      events.values.append("marker:\(marker.kind.rawValue):\(marker.generation)")
   }
}
#endif
