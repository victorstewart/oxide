#if os(macOS)
import AppKit
import Darwin
import Dispatch
import Foundation
import os

enum MacOSCanonicalLaunchFailure: Error, Equatable
{
   case existingArtifact(String)
   case invalidInvocation(String)
   case invalidProbe(String)
   case invalidState(String)
   case stateDidNotTransition
}

enum MacOSCanonicalLaunchClass: String, Codable, Equatable
{
   case terminatedWarmSystemCache = "terminated-warm-system-cache"
   case freshInstallFirstLaunch = "fresh-install-first-launch"
   case warmResume = "warm-resume"

   init(commandLineArguments: [String]) throws
   {
      guard let index = commandLineArguments.firstIndex(of: "-oxide-compare-launch-class") else
      {
         self = .terminatedWarmSystemCache
         return
      }
      guard index + 1 < commandLineArguments.count,
            let value = Self(rawValue: commandLineArguments[index + 1]) else
      {
         throw MacOSCanonicalLaunchFailure.invalidInvocation("launch class")
      }
      self = value
   }

   static func dataRoot(commandLineArguments: [String]) -> URL?
   {
      guard let index = commandLineArguments.firstIndex(of: "-oxide-compare-data-root"),
            index + 1 < commandLineArguments.count else {return nil}
      return URL(fileURLWithPath: commandLineArguments[index + 1], isDirectory: true)
   }

   var checkpointID: String
   {
      switch self
      {
      case .terminatedWarmSystemCache: return "terminated-ready"
      case .freshInstallFirstLaunch: return "fresh-install-ready"
      case .warmResume: return "warm-resume-ready"
      }
   }

   var cacheClass: String
   {
      switch self
      {
      case .terminatedWarmSystemCache: return "warm-system-cache"
      case .freshInstallFirstLaunch: return "unclassified-system-cache"
      case .warmResume: return "resident-process"
      }
   }
}

struct MacOSCanonicalLaunchIdentity: Codable, Equatable
{
   let schemaVersion: UInt32
   let runID: String
   let planSHA256: String
   let chunkID: String
   let passID: String
   let packID: String
   let pairIndex: UInt64
   let side: ComparisonSide
   let generation: String
   let scenarioID: String
   let launchClass: String
   let cacheClass: String
}

struct MacOSCanonicalLaunchInvocation: Equatable
{
   let identity: MacOSCanonicalLaunchIdentity
   let launchClass: MacOSCanonicalLaunchClass
   let dataRoot: URL?
   let planArtifact: BenchmarkArtifactIdentity?
   let trustedInputOffsetNs: UInt64
   let trustedInputDeadlineNs: UInt64

   init(runID: String, planSHA256: String, chunkID: String, pairIndex: UInt64, side: ComparisonSide, generation: String, launchClass: MacOSCanonicalLaunchClass = .terminatedWarmSystemCache, dataRoot: URL? = nil, packID: String = "pr-launch", planArtifact: BenchmarkArtifactIdentity? = nil, trustedInputOffsetNs: UInt64, trustedInputDeadlineNs: UInt64) throws
   {
      try validateComparisonSHA256(planSHA256)
      try validateComparisonSHA256(generation)
      for (label, value) in [("run", runID), ("chunk", chunkID)]
      {
         guard macOSCanonicalLaunchSafeComponent(value) else
         {
            throw MacOSCanonicalLaunchFailure.invalidInvocation(label)
         }
      }
      guard trustedInputOffsetNs > 0,
            trustedInputDeadlineNs > trustedInputOffsetNs,
            macOSCanonicalLaunchSafeComponent(packID),
            planArtifact == nil || planArtifact?.sha256 == planSHA256,
            launchClass != .freshInstallFirstLaunch || dataRoot != nil else
      {
         throw MacOSCanonicalLaunchFailure.invalidInvocation("trusted input timing")
      }
      identity = MacOSCanonicalLaunchIdentity(
         schemaVersion: 1,
         runID: runID,
         planSHA256: planSHA256,
         chunkID: chunkID,
         passID: "canonical-launch",
         packID: packID,
         pairIndex: pairIndex,
         side: side,
         generation: generation,
         scenarioID: "startup.first-screen",
         launchClass: launchClass.rawValue,
         cacheClass: launchClass.cacheClass
      )
      self.launchClass = launchClass
      self.dataRoot = dataRoot
      self.planArtifact = planArtifact
      self.trustedInputOffsetNs = trustedInputOffsetNs
      self.trustedInputDeadlineNs = trustedInputDeadlineNs
   }
}

struct MacOSCanonicalLaunchProbeDescriptor: Codable, Equatable
{
   let schemaVersion: UInt32
   let probeID: String
   let dispatchPath: String
   let targetIdentity: String
   let actionIdentity: String
   let windowNumber: Int
   let xPoints: Double
   let yPoints: Double
}

protocol MacOSCanonicalLaunchProbeTarget: AnyObject
{
   var stateGeneration: UInt64 {get}
   func armTrustedInputProbe(_ observer: @escaping () -> Void) throws -> MacOSCanonicalLaunchProbeDescriptor
   func disarmTrustedInputProbe()
}

protocol MacOSCanonicalLaunchResponseSource: MacOSCanonicalLaunchProbeTarget
{
   var onCanonicalLaunchResponsePresented: ((Result<Void, Error>) -> Void)? {get set}
}

enum MacOSCanonicalLaunchMarkerKind: String, Codable, Equatable
{
   case inputReceived
   case visualGeneration
}

struct MacOSCanonicalLaunchMarker: Codable, Equatable
{
   let kind: MacOSCanonicalLaunchMarkerKind
   let generation: UInt64
   let timestamp: UInt64
}

protocol MacOSCanonicalLaunchMarkerSink
{
   func emit(_ marker: MacOSCanonicalLaunchMarker)
}

struct MacOSCanonicalLaunchOSLogMarkerSink: MacOSCanonicalLaunchMarkerSink
{
   private let log = OSLog(subsystem: "com.oxide.comparison", category: "Presentation")

   func emit(_ marker: MacOSCanonicalLaunchMarker)
   {
      switch marker.kind
      {
      case .inputReceived:
         os_signpost(.event, log: log, name: "InputReceived", "scenario=%{public}llu generation=%{public}llu", 0, marker.generation)
      case .visualGeneration:
         os_signpost(.event, log: log, name: "VisualGeneration", "scenario=%{public}llu generation=%{public}llu", 0, marker.generation)
      }
   }
}

protocol MacOSCanonicalLaunchClock
{
   func now() -> UInt64
}

struct MacOSCanonicalLaunchContinuousClock: MacOSCanonicalLaunchClock
{
   func now() -> UInt64
   {
      mach_continuous_time()
   }
}

struct MacOSCanonicalLaunchApplicationDidFinishReceipt: Codable, Equatable
{
   let identity: MacOSCanonicalLaunchIdentity
   let applicationDidFinishTimestamp: UInt64
   let validation: String
}

struct MacOSCanonicalLaunchFirstCompleteUIReceipt: Codable, Equatable
{
   let identity: MacOSCanonicalLaunchIdentity
   let visualGeneration: UInt64
   let generationMarkerTimestamp: UInt64
   let firstCompleteUITimestamp: UInt64
   let checkpointID: String
   let validation: String
}

struct MacOSCanonicalLaunchReadinessReceipt: Codable, Equatable
{
   let identity: MacOSCanonicalLaunchIdentity
   let readinessTimestamp: UInt64
   let trustedInputOffsetNs: UInt64
   let trustedInputDeadlineNs: UInt64
   let probe: MacOSCanonicalLaunchProbeDescriptor
   let initialStateGeneration: UInt64
   let durable: Bool
}

struct MacOSCanonicalLaunchCompleteReceipt: Codable, Equatable
{
   let identity: MacOSCanonicalLaunchIdentity
   let applicationDidFinishTimestamp: UInt64
   let firstCompleteUITimestamp: UInt64
   let readinessTimestamp: UInt64
   let trustedInputReceivedTimestamp: UInt64
   let responseGenerationTimestamp: UInt64
   let responseCompleteUITimestamp: UInt64
   let initialStateGeneration: UInt64
   let responseStateGeneration: UInt64
   let responseVisualGeneration: UInt64
   let probe: MacOSCanonicalLaunchProbeDescriptor
   let validation: String
   let complete: Bool
}

struct MacOSCanonicalLaunchAcknowledgement: Codable, Equatable
{
   let schemaVersion: UInt32
   let generation: String
   let artifactSHA256: String
   let durable: Bool
}

final class MacOSCanonicalLaunchExecutor
{
   private enum Phase: Equatable
   {
      case initialized
      case awaitingWarmResume
      case awaitingFirstCompleteUI
      case awaitingTrustedInput
      case awaitingResponseCompleteUI
      case complete
   }

   private let invocation: MacOSCanonicalLaunchInvocation
   private let loader: BenchmarkSpecLoader
   private let adapter: BenchmarkScenarioAdapter
   private let probeTarget: MacOSCanonicalLaunchProbeTarget
   private let store: DurableArtifactStore
   private let clock: MacOSCanonicalLaunchClock
   private let markers: MacOSCanonicalLaunchMarkerSink
   private var phase = Phase.initialized
   private var scenario: BenchmarkScenario?
   private var applicationDidFinishTimestamp = UInt64(0)
   private var firstCompleteUITimestamp = UInt64(0)
   private var readinessTimestamp = UInt64(0)
   private var generationMarkerTimestamp = UInt64(0)
   private var trustedInputReceivedTimestamp = UInt64(0)
   private var responseGenerationTimestamp = UInt64(0)
   private var initialStateGeneration = UInt64(0)
   private var probe: MacOSCanonicalLaunchProbeDescriptor?

   init(invocation: MacOSCanonicalLaunchInvocation, loader: BenchmarkSpecLoader, adapter: BenchmarkScenarioAdapter, probeTarget: MacOSCanonicalLaunchProbeTarget, store: DurableArtifactStore, clock: MacOSCanonicalLaunchClock = MacOSCanonicalLaunchContinuousClock(), markers: MacOSCanonicalLaunchMarkerSink = MacOSCanonicalLaunchOSLogMarkerSink())
   {
      self.invocation = invocation
      self.loader = loader
      self.adapter = adapter
      self.probeTarget = probeTarget
      self.store = store
      self.clock = clock
      self.markers = markers
   }

   var isWaitingForTrustedInput: Bool
   {
      phase == .awaitingTrustedInput
   }

   var isComplete: Bool
   {
      phase == .complete
   }

   func applicationDidFinishLaunching() throws
   {
      try applicationDidFinishLaunching(timestamp: clock.now())
   }

   func applicationDidFinishLaunching(timestamp observedApplicationDidFinishTimestamp: UInt64) throws
   {
      guard phase == .initialized else
      {
         throw MacOSCanonicalLaunchFailure.invalidState("applicationDidFinishLaunching")
      }
      guard observedApplicationDidFinishTimestamp > 0 else
      {
         throw MacOSCanonicalLaunchFailure.invalidInvocation("applicationDidFinishLaunching timestamp")
      }
      applicationDidFinishTimestamp = observedApplicationDidFinishTimestamp
      try requireAbsentArtifacts()
      try claimFreshDataContainer()
      let finished = MacOSCanonicalLaunchApplicationDidFinishReceipt(
         identity: invocation.identity,
         applicationDidFinishTimestamp: applicationDidFinishTimestamp,
         validation: "application-delegate-did-finish-launching-mach-continuous-time"
      )
      _ = try store.durableJSON(finished, relativePath: artifactPath("launch.application-did-finish.json"))

      let startup: BenchmarkScenario
      if let planArtifact = invocation.planArtifact
      {
         let plan = try loadAppleCampaignPlan(loader: loader, identity: planArtifact)
         let selection = try selectAppleCampaignExecution(
            plan: plan,
            passID: invocation.identity.passID,
            requestedPackID: invocation.identity.packID
         )
         guard selection.pass.role == .launch,
               selection.scenarioBindings.count == 1,
               let binding = selection.scenarioBindings.first,
               binding.id == invocation.identity.scenarioID,
               let artifact = binding.artifact else
         {
            throw MacOSCanonicalLaunchFailure.invalidInvocation("generic startup selection")
         }
         startup = try loader.loadScenario(artifact)
      }
      else
      {
         startup = try loader.loadScenario(relativePath: "scenarios/startup.first-screen.json")
      }
      guard startup.id == invocation.identity.scenarioID,
            startup.parityCheckpoints.contains(where: {$0.id == invocation.launchClass.checkpointID}) else
      {
         throw MacOSCanonicalLaunchFailure.invalidInvocation("frozen startup scenario")
      }
      scenario = startup
      (adapter as? BenchmarkPassConfiguredAdapter)?.configure(passID: invocation.identity.passID)
      try adapter.prepare(scenario: startup, loader: loader)
      try applyInitialLifecycle(to: startup)
      if invocation.launchClass == .warmResume
      {
         phase = .awaitingWarmResume
         return
      }
      markFirstVisualGeneration()
      phase = .awaitingFirstCompleteUI
   }

   func warmResumeObserved() throws
   {
      guard phase == .awaitingWarmResume, let scenario else
      {
         throw MacOSCanonicalLaunchFailure.invalidState("warmResumeObserved")
      }
      try applyPhase("warm-resume", from: scenario)
      markFirstVisualGeneration()
      phase = .awaitingFirstCompleteUI
   }

   func firstCompleteUIObserved() throws -> MacOSCanonicalLaunchReadinessReceipt
   {
      guard phase == .awaitingFirstCompleteUI, let scenario else
      {
         throw MacOSCanonicalLaunchFailure.invalidState("firstCompleteUIObserved")
      }
      guard let expected = scenario.parityCheckpoints.first(where: {$0.id == invocation.launchClass.checkpointID}) else
      {
         throw MacOSCanonicalLaunchFailure.invalidInvocation("launch-class checkpoint")
      }
      let observed = try adapter.checkpoint(id: expected.id)
      _ = try validateBenchmarkCheckpoint(observed, expected: expected, loader: loader)
      firstCompleteUITimestamp = clock.now()
      let firstComplete = MacOSCanonicalLaunchFirstCompleteUIReceipt(
         identity: invocation.identity,
         visualGeneration: 1,
         generationMarkerTimestamp: generationMarkerTimestamp,
         firstCompleteUITimestamp: firstCompleteUITimestamp,
         checkpointID: expected.id,
         validation: "frozen-\(expected.id)-state-accessibility-role-counts-and-first-complete-ui"
      )
      _ = try store.durableJSON(firstComplete, relativePath: artifactPath("launch.first-complete-ui.json"))

      initialStateGeneration = probeTarget.stateGeneration
      let probe = try probeTarget.armTrustedInputProbe
      {
         [weak self] in
         do
         {
            try self?.trustedTargetActionObserved()
         }
         catch
         {
            assertionFailure("canonical launch trusted target/action failed: \(error)")
         }
      }
      try validateProbe(probe)
      self.probe = probe
      readinessTimestamp = clock.now()
      let ready = MacOSCanonicalLaunchReadinessReceipt(
         identity: invocation.identity,
         readinessTimestamp: readinessTimestamp,
         trustedInputOffsetNs: invocation.trustedInputOffsetNs,
         trustedInputDeadlineNs: invocation.trustedInputDeadlineNs,
         probe: probe,
         initialStateGeneration: initialStateGeneration,
         durable: true
      )
      _ = try store.durableJSON(ready, relativePath: artifactPath("launch.ready.json"))
      phase = .awaitingTrustedInput
      return ready
   }

   func trustedTargetActionObserved() throws
   {
      guard phase == .awaitingTrustedInput else
      {
         throw MacOSCanonicalLaunchFailure.invalidState("trustedTargetActionObserved")
      }
      let responseStateGeneration = probeTarget.stateGeneration
      guard responseStateGeneration > initialStateGeneration else
      {
         throw MacOSCanonicalLaunchFailure.stateDidNotTransition
      }
      trustedInputReceivedTimestamp = clock.now()
      markers.emit(MacOSCanonicalLaunchMarker(kind: .inputReceived, generation: 1, timestamp: trustedInputReceivedTimestamp))
      responseGenerationTimestamp = clock.now()
      markers.emit(MacOSCanonicalLaunchMarker(kind: .visualGeneration, generation: 2, timestamp: responseGenerationTimestamp))
      phase = .awaitingResponseCompleteUI
   }

   func responseCompleteUIObserved() throws -> MacOSCanonicalLaunchCompleteReceipt
   {
      guard phase == .awaitingResponseCompleteUI, let probe else
      {
         throw MacOSCanonicalLaunchFailure.invalidState("responseCompleteUIObserved")
      }
      let responseStateGeneration = probeTarget.stateGeneration
      guard responseStateGeneration > initialStateGeneration else
      {
         throw MacOSCanonicalLaunchFailure.stateDidNotTransition
      }
      let responseCompleteUITimestamp = clock.now()
      let complete = MacOSCanonicalLaunchCompleteReceipt(
         identity: invocation.identity,
         applicationDidFinishTimestamp: applicationDidFinishTimestamp,
         firstCompleteUITimestamp: firstCompleteUITimestamp,
         readinessTimestamp: readinessTimestamp,
         trustedInputReceivedTimestamp: trustedInputReceivedTimestamp,
         responseGenerationTimestamp: responseGenerationTimestamp,
         responseCompleteUITimestamp: responseCompleteUITimestamp,
         initialStateGeneration: initialStateGeneration,
         responseStateGeneration: responseStateGeneration,
         responseVisualGeneration: 2,
         probe: probe,
         validation: "\(invocation.launchClass.rawValue)-real-target-action-state-transition-and-response-generation",
         complete: true
      )
      let artifact = try store.durableJSON(complete, relativePath: artifactPath("launch.complete.json"))
      let acknowledgement = MacOSCanonicalLaunchAcknowledgement(
         schemaVersion: 1,
         generation: invocation.identity.generation,
         artifactSHA256: artifact.sha256,
         durable: true
      )
      _ = try store.durableJSON(acknowledgement, relativePath: artifactPath("launch.complete.ack.json"))
      probeTarget.disarmTrustedInputProbe()
      phase = .complete
      return complete
   }

   func cancel()
   {
      probeTarget.disarmTrustedInputProbe()
   }

   private func validateProbe(_ probe: MacOSCanonicalLaunchProbeDescriptor) throws
   {
      guard probe.schemaVersion == 1,
            macOSCanonicalLaunchSafeComponent(probe.probeID),
            probe.dispatchPath == "trusted-os-input-target-action",
            !probe.targetIdentity.isEmpty,
            !probe.actionIdentity.isEmpty,
            probe.windowNumber > 0,
            probe.xPoints.isFinite,
            probe.yPoints.isFinite else
      {
         throw MacOSCanonicalLaunchFailure.invalidProbe(probe.probeID)
      }
   }

   private func applyInitialLifecycle(to scenario: BenchmarkScenario) throws
   {
      try applyPhase("terminated-warm-cache", from: scenario)
      if invocation.launchClass != .terminatedWarmSystemCache
      {
         try applyPhase("fresh-install-first-launch", from: scenario)
      }
   }

   private func applyPhase(_ id: String, from scenario: BenchmarkScenario) throws
   {
      guard let trace = scenario.phases.first(where: {$0.id == id})?.trace else
      {
         throw MacOSCanonicalLaunchFailure.invalidInvocation("\(id) trace")
      }
      for event in try loader.loadTrace(trace)
      {
         try adapter.apply(event: event)
      }
   }

   private func markFirstVisualGeneration()
   {
      generationMarkerTimestamp = clock.now()
      markers.emit(MacOSCanonicalLaunchMarker(kind: .visualGeneration, generation: 1, timestamp: generationMarkerTimestamp))
   }

   private func requireAbsentArtifacts() throws
   {
      for name in [
         "launch.application-did-finish.json",
         "launch.first-complete-ui.json",
         "launch.ready.json",
         "launch.complete.json",
         "launch.complete.ack.json",
      ]
      {
         let relativePath = artifactPath(name)
         if FileManager.default.fileExists(atPath: store.root.appendingPathComponent(relativePath).path)
         {
            throw MacOSCanonicalLaunchFailure.existingArtifact(relativePath)
         }
      }
   }

   private func claimFreshDataContainer() throws
   {
      guard invocation.launchClass == .freshInstallFirstLaunch,
            let root = invocation.dataRoot else {return}
      let contents = try FileManager.default.contentsOfDirectory(atPath: root.path)
      guard contents.isEmpty else
      {
         throw MacOSCanonicalLaunchFailure.invalidInvocation("fresh data container was not empty")
      }
      let marker = root.appendingPathComponent("install-identity.\(invocation.identity.generation)")
      try Data(invocation.identity.generation.utf8).write(to: marker, options: .withoutOverwriting)
   }

   private func artifactPath(_ name: String) -> String
   {
      let identity = invocation.identity
      return "Runs/\(identity.runID)/\(identity.chunkID)/\(identity.passID)/\(identity.packID)/\(identity.pairIndex)/\(identity.side.rawValue).\(name)"
   }
}

final class MacOSCanonicalLaunchApplicationCoordinator
{
   private let invocation: MacOSCanonicalLaunchInvocation
   private let responseSource: MacOSCanonicalLaunchResponseSource
   private let executor: MacOSCanonicalLaunchExecutor
   private let completion: (Result<MacOSCanonicalLaunchCompleteReceipt, Error>) -> Void

   init(invocation: MacOSCanonicalLaunchInvocation, loader: BenchmarkSpecLoader, adapter: BenchmarkScenarioAdapter, responseSource: MacOSCanonicalLaunchResponseSource, store: DurableArtifactStore, completion: @escaping (Result<MacOSCanonicalLaunchCompleteReceipt, Error>) -> Void)
   {
      self.invocation = invocation
      self.responseSource = responseSource
      executor = MacOSCanonicalLaunchExecutor(
         invocation: invocation,
         loader: loader,
         adapter: adapter,
         probeTarget: responseSource,
         store: store
      )
      self.completion = completion
   }

   func start(applicationDidFinishTimestamp: UInt64) throws
   {
      try executor.applicationDidFinishLaunching(timestamp: applicationDidFinishTimestamp)
      if invocation.launchClass == .warmResume
      {
         let resumeName = "com.oxide.compare.launch.warm-resume.g\(invocation.identity.generation)"
         let preparedName = "com.oxide.compare.launch.warm-prepared.g\(invocation.identity.generation)"
         DispatchQueue.global(qos: .userInitiated).async
         {
            [weak self] in
            guard let self else {return}
            let resumed = runWaitingForMacOSCanonicalLaunchNotification(resumeName, timeout: .seconds(30))
            {
               postMacOSCanonicalLaunchNotification(preparedName)
            }
            DispatchQueue.main.async
            {
               guard resumed else
               {
                  self.completion(.failure(MacOSCanonicalLaunchFailure.invalidState("warm-resume signal timeout")))
                  return
               }
               do
               {
                  try self.executor.warmResumeObserved()
                  try self.observeFirstCompleteUI()
               }
               catch
               {
                  self.completion(.failure(error))
               }
            }
         }
         return
      }
      DispatchQueue.main.async
      {
         [weak self] in
         guard let self else {return}
         do
         {
            try observeFirstCompleteUI()
         }
         catch
         {
            completion(.failure(error))
         }
      }
   }

   private func observeFirstCompleteUI() throws
   {
      _ = try executor.firstCompleteUIObserved()
      responseSource.onCanonicalLaunchResponsePresented =
      {
         [weak self] result in
         self?.responsePresented(result)
      }
      postMacOSCanonicalLaunchNotification("com.oxide.compare.launch.ready.g\(invocation.identity.generation)")
   }

   func cancel()
   {
      executor.cancel()
      responseSource.onCanonicalLaunchResponsePresented = nil
   }

   private func responsePresented(_ result: Result<Void, Error>)
   {
      do
      {
         try result.get()
         let receipt = try executor.responseCompleteUIObserved()
         postMacOSCanonicalLaunchNotification("com.oxide.compare.launch.complete.g\(invocation.identity.generation)")
         completion(.success(receipt))
      }
      catch
      {
         completion(.failure(error))
      }
   }
}

private func macOSCanonicalLaunchSafeComponent(_ value: String) -> Bool
{
   !value.isEmpty
      && value.count <= 128
      && value.utf8.allSatisfy
      {
         byte in
         (byte >= 48 && byte <= 57)
            || (byte >= 65 && byte <= 90)
            || (byte >= 97 && byte <= 122)
            || byte == 45
            || byte == 46
            || byte == 95
      }
}

private func postMacOSCanonicalLaunchNotification(_ name: String)
{
   CFNotificationCenterPostNotification(
      CFNotificationCenterGetDarwinNotifyCenter(),
      CFNotificationName(name as CFString),
      nil,
      nil,
      true
   )
}

private final class MacOSCanonicalLaunchNotificationWaiter
{
   let semaphore = DispatchSemaphore(value: 0)
}

private func macOSCanonicalLaunchNotificationCallback(center: CFNotificationCenter?, observer: UnsafeMutableRawPointer?, name: CFNotificationName?, object: UnsafeRawPointer?, userInfo: CFDictionary?)
{
   guard let observer else {return}
   Unmanaged<MacOSCanonicalLaunchNotificationWaiter>.fromOpaque(observer).takeUnretainedValue().semaphore.signal()
}

private func runWaitingForMacOSCanonicalLaunchNotification(_ name: String, timeout: DispatchTimeInterval, action: () -> Void) -> Bool
{
   let waiter = MacOSCanonicalLaunchNotificationWaiter()
   let observer = Unmanaged.passUnretained(waiter).toOpaque()
   let center = CFNotificationCenterGetDarwinNotifyCenter()
   CFNotificationCenterAddObserver(center, observer, macOSCanonicalLaunchNotificationCallback, name as CFString, nil, .deliverImmediately)
   defer {CFNotificationCenterRemoveObserver(center, observer, CFNotificationName(name as CFString), nil)}
   action()
   return waiter.semaphore.wait(timeout: .now() + timeout) == .success
}

func macOSCanonicalLaunchProbeDescriptor(view: NSView, point: CGPoint? = nil, probeID: String, targetIdentity: String, actionIdentity: String) throws -> MacOSCanonicalLaunchProbeDescriptor
{
   guard let window = view.window,
         let screen = window.screen,
         let screenNumber = screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber,
         window.windowNumber > 0,
         !view.isHidden else
   {
      throw MacOSCanonicalLaunchFailure.invalidProbe(probeID)
   }
   let center = point ?? CGPoint(x: view.bounds.midX, y: view.bounds.midY)
   let windowPoint = view.convert(center, to: nil)
   let screenPoint = window.convertPoint(toScreen: windowPoint)
   let displayBounds = CGDisplayBounds(CGDirectDisplayID(screenNumber.uint32Value))
   let eventPoint = CGPoint(
      x: displayBounds.minX + screenPoint.x - screen.frame.minX,
      y: displayBounds.minY + screen.frame.maxY - screenPoint.y
   )
   return MacOSCanonicalLaunchProbeDescriptor(
      schemaVersion: 1,
      probeID: probeID,
      dispatchPath: "trusted-os-input-target-action",
      targetIdentity: targetIdentity,
      actionIdentity: actionIdentity,
      windowNumber: window.windowNumber,
      xPoints: Double(eventPoint.x),
      yPoints: Double(eventPoint.y)
   )
}
#endif
