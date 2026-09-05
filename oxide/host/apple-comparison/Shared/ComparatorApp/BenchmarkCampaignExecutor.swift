import Darwin
import os
import QuartzCore

private enum BenchmarkPresentationTrace
{
   private static let log = OSLog(subsystem: "com.oxide.comparison", category: "Presentation")

   static func inputReceived(scenarioIndex: UInt64, generation: UInt64)
   {
      os_signpost(.event, log: log, name: "InputReceived", "scenario=%{public}llu generation=%{public}llu", scenarioIndex, generation)
   }

   static func visualGeneration(scenarioIndex: UInt64, generation: UInt64)
   {
      os_signpost(.event, log: log, name: "VisualGeneration", "scenario=%{public}llu generation=%{public}llu", scenarioIndex, generation)
   }

   static func displayOpportunity(scenarioIndex: UInt64, generation: UInt64)
   {
      os_signpost(.event, log: log, name: "DisplayOpportunity", "scenario=%{public}llu generation=%{public}llu", scenarioIndex, generation)
   }

   static func scenarioBoundary(begin: Bool, scenarioIndex: UInt64, identifier: UInt64)
   {
      if begin
      {
         os_signpost(.event, log: log, name: "ScenarioBegin", "scenario=%{public}llu identifier=%{public}llu", scenarioIndex, identifier)
      }
      else
      {
         os_signpost(.event, log: log, name: "ScenarioEnd", "scenario=%{public}llu identifier=%{public}llu", scenarioIndex, identifier)
      }
   }

   static func phaseBoundary(begin: Bool, scenarioIndex: UInt64, identifier: UInt64, measured: Bool)
   {
      if begin
      {
         os_signpost(.event, log: log, name: "PhaseBegin", "scenario=%{public}llu identifier=%{public}llu measured=%{public}d", scenarioIndex, identifier, measured ? 1 : 0)
      }
      else
      {
         os_signpost(.event, log: log, name: "PhaseEnd", "scenario=%{public}llu identifier=%{public}llu measured=%{public}d", scenarioIndex, identifier, measured ? 1 : 0)
      }
   }
}

private struct PreparedBenchmarkScenario
{
   let scenario: BenchmarkScenario
   var schedule: BenchmarkPhaseSchedule
   let resetSegment: ApplePrResetSegment?
   let recoverySegment: ApplePrResetSegment?
   let resetDeadlineMs: UInt64
}

final class BenchmarkCampaignExecutor: NSObject
{
   private let invocation: BenchmarkCampaignInvocation
   private let loader: BenchmarkSpecLoader
   private let adapter: BenchmarkScenarioAdapter
   private let store: DurableArtifactStore
   private let completion: (Result<BenchmarkCampaignEnvelope, Error>) -> Void
   private var ring: BenchmarkTelemetryRing
   private var prepared = [PreparedBenchmarkScenario]()
   private var scenarioEvidence = [BenchmarkScenarioEvidence]()
   private var resetEvidence = [BenchmarkResetEvidence]()
   private var resetRecoveryEvidence = [BenchmarkResetRecoveryEvidence]()
   private var fixedPackBaselinePhysicalFootprintBytes: UInt64?
   private var scenarioIndex = 0
   private var scenarioStartedAt = UInt64(0)
   private var scenarioFirstSequence = UInt64(0)
   private var logicalUpdates = UInt64(0)
   private var measuredPhase = false
   private var displayLink: CADisplayLink?
   private var timebase = mach_timebase_info_data_t()
   private var terminal = false
#if os(macOS)
   private var trustedInputCoordinator: MacOSTrustedInputApplicationCoordinator?
   private var trustedInputPlan: MacOSTrustedInputPlan?
   private var trustedInputPlans = [MacOSTrustedInputPlan]()
   private var trustedInputEventIndex = 0
   private var trustedInputCommandSequence = UInt64(0)
   private var awaitingTrustedInput = false
   private var deferredActions = [BenchmarkScheduledAction]()
   private var previousDisplayTargetNs: UInt64?
   private var observedDisplayIntervalNs = UInt64(0)
   private var observedDisplayIntervalCount = UInt64(0)
   private var scaleRuntimeAttestation: (effectiveCardinality: UInt64, completed: Bool)?
   private var surfaceRuntimeSnapshot: BenchmarkMacOSSurfaceSnapshot?
#endif

   init(invocation: BenchmarkCampaignInvocation, loader: BenchmarkSpecLoader, adapter: BenchmarkScenarioAdapter, store: DurableArtifactStore, completion: @escaping (Result<BenchmarkCampaignEnvelope, Error>) -> Void)
   {
      self.invocation = invocation
      self.loader = loader
      self.adapter = adapter
      self.store = store
      self.completion = completion
      ring = BenchmarkTelemetryRing(capacity: 1)
      super.init()
   }

#if os(macOS)
   func installTrustedInputObserver(window: MacOSTrustedInputWindow, stateGeneration: @escaping () -> UInt64) throws
   {
      guard trustedInputCoordinator == nil, window.trustedInputEventObserver == nil else
      {
         throw MacOSTrustedInputFailure.invalidState("trusted input observer already installed")
      }
      let observer = MacOSTrustedInputApplicationEventObserver(stateGeneration: stateGeneration)
      window.trustedInputEventObserver = observer
      trustedInputCoordinator = MacOSTrustedInputApplicationCoordinator(invocation: invocation, store: store, observer: observer)
   }
#endif

   func prepare() throws -> BenchmarkCampaignReadyEnvelope
   {
      if let planArtifact = invocation.planArtifact
      {
         return try prepareGenericCampaign(planArtifact: planArtifact)
      }
      guard invocation.request.passID == "correctness"
               || invocation.request.passID == "minimal-presentation"
               || invocation.request.passID == "canonical-launch" else
      {
         throw BenchmarkCampaignFailure.invalidPack(invocation.request.passID)
      }
      mach_timebase_info(&timebase)
      let plan = try loadApplePrPlan(loader: loader, expectedSHA256: invocation.request.planSHA256)
      let acquisition = try loadApplePrAcquisition(loader: loader, identity: plan.acquisition)
      try plan.validate(acquisition: acquisition)
      guard let pack = acquisition.packs.first(where: {$0.id == invocation.packID}) else
      {
         throw BenchmarkCampaignFailure.invalidPack(invocation.packID)
      }
      ring = BenchmarkTelemetryRing(capacity: try benchmarkTelemetryCapacity(pack: pack, passID: invocation.request.passID))
      prepared = try benchmarkPackedScenarios(pack).map
      {
         packed in
         let scenario = try loader.loadScenario(plan.scenarioArtifact(id: packed.scenarioID))
         guard scenario.id == packed.scenarioID else
         {
            throw BenchmarkCampaignFailure.invalidPlan
         }
         return PreparedBenchmarkScenario(
            scenario: scenario,
            schedule: try BenchmarkPhaseSchedule(
               scenario: scenario,
               setupDurationUs: pack.setupSecondsPerScenario * 1_000_000,
               loader: loader
            ),
            resetSegment: packed.resetSegment,
            recoverySegment: pack.resetSegments.first(where: {$0.orderedScenarioIds.last == packed.scenarioID}),
            resetDeadlineMs: pack.resetSeconds * 1_000
         )
      }
      guard let first = prepared.first else
      {
         throw BenchmarkCampaignFailure.invalidPack(invocation.packID)
      }
      if let identity = invocation.scaleOverlayArtifact
      {
         guard prepared.count == 1,
               let scaled = adapter as? BenchmarkMacOSScaleConfiguredAdapter else
         {
            throw BenchmarkCampaignFailure.invalidPack("macos-scale")
         }
         let overlay = try loader.loadMacOSComparatorScaleOverlay(identity, scenario: first.scenario)
         try scaled.configure(scaleOverlay: overlay, identity: identity)
      }
      (adapter as? BenchmarkPassConfiguredAdapter)?.configure(passID: invocation.request.passID)
      if pack.resetSegments.isEmpty
      {
         try adapter.prepare(scenario: first.scenario, loader: loader)
      }
      else
      {
         guard let quiescence = adapter as? BenchmarkQuiescenceAdapter else
         {
            throw BenchmarkCampaignFailure.quiescenceUnavailable
         }
         if invocation.request.passID == "correctness", let capture = adapter as? BenchmarkPreviewCapture
         {
            var warmPackHighWaterBytes = UInt64(0)
            var warmPackSamples = [[UInt64]]()
            var previousWarmPackHighWaterBytes: UInt64?
            var stableWarmPackTransitions = 0
            for _ in 0..<10
            {
               let packSamples = try prewarmCorrectnessPack(quiescence: quiescence)
               warmPackSamples.append(packSamples)
               let observed = packSamples.max() ?? 0
               warmPackHighWaterBytes = max(warmPackHighWaterBytes, observed)
               if let previousWarmPackHighWaterBytes,
                  benchmarkWarmFootprintConverged(previousWarmPackHighWaterBytes, warmPackHighWaterBytes)
               {
                  stableWarmPackTransitions += 1
                  if stableWarmPackTransitions == 2
                  {
                     break
                  }
               }
               else
               {
                  stableWarmPackTransitions = 0
               }
               previousWarmPackHighWaterBytes = warmPackHighWaterBytes
            }
            guard stableWarmPackTransitions == 2 else
            {
               throw BenchmarkCampaignFailure.packWarmupUnstable(warmPackSamples)
            }
            try removeCorrectnessWarmupArtifacts()
            try autoreleasepool
            {
               try adapter.prepare(scenario: first.scenario, loader: loader)
               try quiescence.quiesce()
            }
            let warmCaptureBytes = try warmCorrectnessCaptureBaseline(capture: capture, quiescence: quiescence)
            let fixedBaselineBytes = max(warmPackHighWaterBytes, warmCaptureBytes)
            fixedPackBaselinePhysicalFootprintBytes = fixedBaselineBytes
            let warmupEvidence = BenchmarkWarmupFootprintEvidence(
               schemaVersion: 1,
               packSamples: warmPackSamples,
               packHighWaterBytes: warmPackHighWaterBytes,
               captureBaselineBytes: warmCaptureBytes,
               fixedBaselineBytes: fixedBaselineBytes,
               convergenceDeltaLimitBytes: 4 * 1_024 * 1_024,
               convergenceBasis: "running-pack-high-water",
               requiredStableTransitions: 2,
               attemptLimit: 10
            )
            _ = try store.durableJSON(warmupEvidence, relativePath: relativePath("qualification/warmup-footprint.json"))
         }
         else
         {
            for _ in 0..<2
            {
               try prewarmTimedPack(quiescence: quiescence)
            }
            try autoreleasepool
            {
               try adapter.prepare(scenario: first.scenario, loader: loader)
               try quiescence.quiesce()
            }
         }
      }
      #if os(macOS)
      if invocation.request.passID != "correctness"
      {
         try prepareTrustedInputSession()
      }
      #endif
      let readyTimestamp = mach_continuous_time()
      if invocation.request.passID != "correctness"
      {
         ring.append(kind: .readyToInput, timestamp: readyTimestamp)
      }
      let ready = BenchmarkCampaignReadyEnvelope(
         schemaVersion: 1,
         runID: invocation.request.runID,
         planSHA256: invocation.request.planSHA256,
         chunkID: invocation.chunkID,
         passID: invocation.request.passID,
         pairIndex: invocation.request.pairIndex,
         side: invocation.request.side,
         generation: invocation.request.generation,
         packID: invocation.packID,
         scenarioIDs: prepared.map { $0.scenario.id },
         readyTimestamp: readyTimestamp,
         timebaseNumerator: timebase.numer,
         timebaseDenominator: timebase.denom,
         durable: true
      )
      _ = try store.durableJSON(ready, relativePath: relativePath("ready.json"))
      if !pack.resetSegments.isEmpty
      {
         if fixedPackBaselinePhysicalFootprintBytes == nil
         {
            malloc_zone_pressure_relief(nil, 0)
            fixedPackBaselinePhysicalFootprintBytes = try benchmarkPhysicalFootprintBytes()
         }
      }
      return ready
   }

   private func prepareGenericCampaign(planArtifact: BenchmarkArtifactIdentity) throws -> BenchmarkCampaignReadyEnvelope
   {
      mach_timebase_info(&timebase)
      let plan = try loadAppleCampaignPlan(loader: loader, identity: planArtifact)
      let requestedPackID = invocation.packID == "direct" ? nil : invocation.packID
      let selection = try selectAppleCampaignExecution(
         plan: plan,
         passID: invocation.request.passID,
         requestedPackID: requestedPackID
      )
      guard selection.pass.role != .launch else
      {
         throw BenchmarkCampaignFailure.invalidPack("canonical-launch-requires-launch-executor")
      }
      prepared = try selection.scenarioBindings.map
      {
         binding in
         guard let artifact = binding.artifact else
         {
            throw BenchmarkCampaignFailure.invalidPlan
         }
         let scenario = try loader.loadScenario(artifact)
         guard scenario.id == binding.id else
         {
            throw BenchmarkCampaignFailure.invalidPlan
         }
         let timing = selection.timing?.scenarios.first(where: {$0.scenarioId == binding.id})
         if selection.pass.role == .primary || selection.pass.role == .attribution || selection.pass.role == .idle || selection.pass.role == .endurance,
            timing == nil
         {
            throw BenchmarkCampaignFailure.invalidPlan
         }
         return PreparedBenchmarkScenario(
            scenario: scenario,
            schedule: try BenchmarkPhaseSchedule(scenario: scenario, setupDurationUs: 0, timing: timing, loader: loader),
            resetSegment: nil,
            recoverySegment: nil,
            resetDeadlineMs: 0
         )
      }
      guard let first = prepared.first else
      {
         throw BenchmarkCampaignFailure.invalidPack(invocation.packID)
      }
      if let identity = invocation.scaleOverlayArtifact
      {
         guard prepared.count == 1,
               let scaled = adapter as? BenchmarkMacOSScaleConfiguredAdapter else
         {
            throw BenchmarkCampaignFailure.invalidPack("macos-scale")
         }
         let overlay = try loader.loadMacOSComparatorScaleOverlay(identity, scenario: first.scenario)
         try scaled.configure(scaleOverlay: overlay, identity: identity)
      }
      let (durationUs, overflow) = prepared.reduce((UInt64(0), false))
      {
         partial, scenario in
         guard !partial.1 else {return partial}
         let (sum, overflow) = partial.0.addingReportingOverflow(scenario.schedule.durationUs)
         return (sum, overflow)
      }
      guard !overflow else
      {
         throw BenchmarkCampaignFailure.invalidPack(invocation.packID)
      }
      let wholeSeconds = durationUs / 1_000_000
      let sideSeconds = max(1, wholeSeconds + (durationUs % 1_000_000 == 0 ? 0 : 1))
      let capacityPack = ApplePrAcquisitionPack(
         id: invocation.packID,
         orderedScenarioIds: prepared.map {$0.scenario.id},
         resetSegments: [],
         pairCount: selection.pass.pairCount,
         sidesPerPair: 2,
         resetCountPerSide: 0,
         resetSeconds: 0,
         setupSecondsPerScenario: 0,
         warmupSecondsPerScenario: 0,
         measureSecondsPerScenario: 0,
         sideSeconds: sideSeconds
      )
      ring = BenchmarkTelemetryRing(capacity: try benchmarkTelemetryCapacity(pack: capacityPack, passID: invocation.request.passID))
      (adapter as? BenchmarkPassConfiguredAdapter)?.configure(passID: invocation.request.passID)
      try adapter.prepare(scenario: first.scenario, loader: loader)
      #if os(macOS)
      try prepareTrustedInputSession()
      #endif
      let readyTimestamp = mach_continuous_time()
      ring.append(kind: .readyToInput, timestamp: readyTimestamp)
      let ready = BenchmarkCampaignReadyEnvelope(
         schemaVersion: 1,
         runID: invocation.request.runID,
         planSHA256: invocation.request.planSHA256,
         chunkID: invocation.chunkID,
         passID: invocation.request.passID,
         pairIndex: invocation.request.pairIndex,
         side: invocation.request.side,
         generation: invocation.request.generation,
         packID: invocation.packID,
         scenarioIDs: prepared.map {$0.scenario.id},
         readyTimestamp: readyTimestamp,
         timebaseNumerator: timebase.numer,
         timebaseDenominator: timebase.denom,
         durable: true
      )
      _ = try store.durableJSON(ready, relativePath: relativePath("ready.json"))
      return ready
   }

   private func prewarmCorrectnessPack(quiescence: BenchmarkQuiescenceAdapter) throws -> [UInt64]
   {
      var samples = [UInt64]()
      for preparedScenario in prepared
      {
         try autoreleasepool
         {
            try adapter.prepare(scenario: preparedScenario.scenario, loader: loader)
            var schedule = try BenchmarkPhaseSchedule(scenario: preparedScenario.scenario, setupDurationUs: 0, loader: loader)
            try schedule.drainTimed(until: schedule.durationUs)
            {
               timeUs, action in
               try (adapter as? BenchmarkVirtualClockAdapter)?.setVirtualTimeUs(timeUs)
               switch action
               {
               case .traceEvent(let event): try adapter.apply(event: event)
               case .checkpoint(let checkpointID):
                  _ = try captureCheckpoint(
                     scenario: preparedScenario.scenario,
                     checkpointID: checkpointID,
                     evidenceDirectory: "warmup"
                  )
               default: break
               }
            }
            try quiescence.quiesce()
            try adapter.teardown()
         }
         try drainRetiredAdapterResources()
         malloc_zone_pressure_relief(nil, 0)
         samples.append(try benchmarkPhysicalFootprintBytes())
      }
      return samples
   }

   private func prewarmTimedPack(quiescence: BenchmarkQuiescenceAdapter) throws
   {
      for preparedScenario in prepared
      {
         try autoreleasepool
         {
            try adapter.prepare(scenario: preparedScenario.scenario, loader: loader)
            var schedule = preparedScenario.schedule
#if os(macOS)
            let trustedPlan = try MacOSTrustedInputCompiler.compile(
               schedule.orderedTraceEvents,
               allowNonClaimLifecycleStimuli: allowsNonClaimLifecycleStimuli
            )
            var trustedEventIndex = 0
#endif
            try schedule.drainTimed(until: schedule.durationUs)
            {
               timeUs, action in
               try (adapter as? BenchmarkVirtualClockAdapter)?.setVirtualTimeUs(timeUs)
               switch action
               {
               case .traceEvent(let event):
#if os(macOS)
                  if try trustedPlan.disposition(eventIndex: trustedEventIndex) == .applicationStimulus
                  {
                     try adapter.apply(event: event)
                  }
                  trustedEventIndex += 1
#else
                  try adapter.apply(event: event)
#endif
               case .checkpoint: try quiescence.quiesce()
               default: break
               }
            }
            try quiescence.quiesce()
            try adapter.teardown()
         }
         try drainRetiredAdapterResources()
      }
      malloc_zone_pressure_relief(nil, 0)
   }

   private func warmCorrectnessCaptureBaseline(capture: BenchmarkPreviewCapture, quiescence: BenchmarkQuiescenceAdapter) throws -> UInt64
   {
      var previous: UInt64?
      for _ in 0..<6
      {
         try autoreleasepool {try warmCorrectnessCollectors(capture: capture)}
         malloc_zone_pressure_relief(nil, 0)
         try quiescence.quiesce()
         let observed = try benchmarkPhysicalFootprintBytes()
         if let previous
         {
            let previousLimit = try benchmarkFixedFootprintRecoveryLimit(previous)
            let observedLimit = try benchmarkFixedFootprintRecoveryLimit(observed)
            if observed <= previousLimit,
               previous <= observedLimit
            {
               return max(previous, observed)
            }
         }
         previous = observed
      }
      throw BenchmarkCampaignFailure.captureWarmupUnstable(previous ?? 0)
   }

   private func warmCorrectnessCollectors(capture: BenchmarkPreviewCapture) throws
   {
      if let rawAccessibility = adapter as? BenchmarkRawAccessibilityCapture
      {
         _ = try rawAccessibility.rawAccessibilityTree()
      }
      _ = try capture.previewPNG()
   }

   private func benchmarkWarmFootprintConverged(_ previous: UInt64, _ observed: UInt64) -> Bool
   {
      let delta = previous > observed ? previous - observed : observed - previous
      return delta <= 4 * 1_024 * 1_024
   }

   func start()
   {
      guard !terminal, !prepared.isEmpty else {return}
      do
      {
         try performResetIfNeeded(for: prepared[0])
         try recordRendererDiagnostics()
      }
      catch
      {
         fail(error)
         return
      }
      if invocation.request.passID == "correctness"
      {
         do
         {
            try runCorrectness()
         }
         catch
         {
            fail(error)
         }
         return
      }
      scenarioStartedAt = mach_continuous_time()
      scenarioFirstSequence = UInt64(ring.count)
      #if os(macOS)
      do
      {
         try prepareTrustedInputPlan()
      }
      catch
      {
         fail(error)
         return
      }
      guard let provider = adapter as? BenchmarkDisplayLinkProvider else
      {
         fail(BenchmarkCampaignFailure.displayLinkUnavailable)
         return
      }
      let displayLink = provider.makeDisplayLink(target: self, selector: #selector(displayTick(_:)))
      #else
      let displayLink = CADisplayLink(target: self, selector: #selector(displayTick(_:)))
      #endif
      displayLink.preferredFrameRateRange = .default
      displayLink.add(to: .main, forMode: .common)
      self.displayLink = displayLink
      drainSchedule(now: scenarioStartedAt)
   }

   @objc private func displayTick(_ link: CADisplayLink)
   {
      guard !terminal else {return}
      let now = mach_continuous_time()
      if measuredPhase
      {
         let targetNs = UInt64(max(0, link.targetTimestamp * 1_000_000_000))
         if let previousDisplayTargetNs, targetNs > previousDisplayTargetNs
         {
            let (sum, overflow) = observedDisplayIntervalNs.addingReportingOverflow(targetNs - previousDisplayTargetNs)
            if !overflow
            {
               observedDisplayIntervalNs = sum
               observedDisplayIntervalCount += 1
            }
         }
         previousDisplayTargetNs = targetNs
         BenchmarkPresentationTrace.displayOpportunity(scenarioIndex: UInt64(scenarioIndex), generation: logicalUpdates)
         ring.append(
            kind: .displayOpportunity,
            identifier: UInt64(scenarioIndex),
            value0: Int64(link.targetTimestamp * 1_000_000_000),
            value1: Int64(link.duration * 1_000_000_000),
            timestamp: now
         )
         ring.append(kind: .callbackCadence, identifier: UInt64(scenarioIndex), timestamp: now)
      }
      do
      {
         try (adapter as? BenchmarkFrameDrivenAdapter)?.displayTick()
         try recordRendererDiagnostics()
         drainSchedule(now: now)
      }
      catch
      {
         fail(error)
      }
   }

   private func drainSchedule(now: UInt64)
   {
      guard scenarioIndex < prepared.count else {return}
#if os(macOS)
      guard !awaitingTrustedInput else {return}
#endif
      let elapsedUs = machTicksToNanoseconds(now &- scenarioStartedAt) / 1_000
      do
      {
#if os(macOS)
         var due = deferredActions
         deferredActions.removeAll(keepingCapacity: true)
#else
         var due = [BenchmarkScheduledAction]()
#endif
         prepared[scenarioIndex].schedule.drain(until: elapsedUs)
         {
            action in
            due.append(action)
         }
         for (index, action) in due.enumerated()
         {
            switch action
            {
            case .scenarioEnd:
               try consume(action, timestamp: mach_continuous_time())
            default:
               try autoreleasepool
               {
                  try consume(action, timestamp: mach_continuous_time())
               }
            }
#if os(macOS)
            if awaitingTrustedInput
            {
               deferredActions.append(contentsOf: due.dropFirst(index + 1))
               break
            }
#endif
         }
      }
      catch
      {
         fail(error)
      }
   }

   private func consume(_ action: BenchmarkScheduledAction, timestamp: UInt64) throws
   {
      switch action
      {
      case .scenarioBegin(let id):
         let identifier = benchmarkStableID(id)
         BenchmarkPresentationTrace.scenarioBoundary(begin: true, scenarioIndex: UInt64(scenarioIndex), identifier: identifier)
         ring.append(kind: .scenarioBegin, identifier: identifier, timestamp: timestamp)
      case .phaseBegin(let id, let measured):
         measuredPhase = measured
         let identifier = benchmarkStableID(id)
         BenchmarkPresentationTrace.phaseBoundary(begin: true, scenarioIndex: UInt64(scenarioIndex), identifier: identifier, measured: measured)
         ring.append(kind: .phaseBegin, identifier: identifier, flags: measured ? 1 : 0, timestamp: timestamp)
      case .traceEvent(let event):
#if os(macOS)
         try consumeMacOSTraceEvent(event, timestamp: timestamp)
#else
         let mutation = event.op == "mutate" || event.op == "navigate"
         if measuredPhase
         {
            BenchmarkPresentationTrace.inputReceived(scenarioIndex: UInt64(scenarioIndex), generation: logicalUpdates)
         }
         ring.append(kind: mutation ? .mutationBegin : .inputReceived, identifier: event.stateId.map(benchmarkStableID) ?? 0, value0: Int64(logicalUpdates), timestamp: timestamp)
         ring.append(kind: .sceneUpdateBegin, identifier: event.stateId.map(benchmarkStableID) ?? 0, timestamp: timestamp)
         try adapter.apply(event: event)
         try recordRendererDiagnostics()
         logicalUpdates += 1
         if measuredPhase
         {
            BenchmarkPresentationTrace.visualGeneration(scenarioIndex: UInt64(scenarioIndex), generation: logicalUpdates)
         }
         ring.append(kind: .sceneUpdateEnd, identifier: event.stateId.map(benchmarkStableID) ?? 0, timestamp: mach_continuous_time())
         ring.append(kind: .logicalUpdateCompleted, identifier: event.stateId.map(benchmarkStableID) ?? 0, value0: Int64(logicalUpdates), timestamp: mach_continuous_time())
         if mutation
         {
            ring.append(kind: .mutationEnd, identifier: event.stateId.map(benchmarkStableID) ?? 0, value0: Int64(logicalUpdates), timestamp: mach_continuous_time())
         }
#endif
      case .checkpoint(let id):
         ring.append(kind: .checkpointBegin, identifier: benchmarkStableID(id), timestamp: timestamp)
         ring.append(kind: .checkpointEnd, identifier: benchmarkStableID(id), timestamp: timestamp)
      case .phaseEnd(let id, let measured):
         let identifier = benchmarkStableID(id)
         BenchmarkPresentationTrace.phaseBoundary(begin: false, scenarioIndex: UInt64(scenarioIndex), identifier: identifier, measured: measured)
         ring.append(kind: .phaseEnd, identifier: identifier, flags: measured ? 1 : 0, timestamp: timestamp)
         measuredPhase = false
      case .scenarioEnd(let id):
         let identifier = benchmarkStableID(id)
         BenchmarkPresentationTrace.scenarioBoundary(begin: false, scenarioIndex: UInt64(scenarioIndex), identifier: identifier)
         ring.append(kind: .scenarioEnd, identifier: identifier, timestamp: timestamp)
         scenarioEvidence.append(BenchmarkScenarioEvidence(
            scenarioID: id,
            firstTelemetrySequence: scenarioFirstSequence,
            lastTelemetrySequence: UInt64(ring.count - 1),
            checkpointStateSHA256: [:],
            checkpointAccessibilitySHA256: [:],
            checkpoints: [],
            completedLogicalUpdates: logicalUpdates
         ))
         try finishScenario(prepared[scenarioIndex])
         try advanceScenario(after: timestamp)
      }
   }

   private func advanceScenario(after timestamp: UInt64) throws
   {
      scenarioIndex += 1
      if scenarioIndex == prepared.count
      {
         try finish()
         return
      }
      logicalUpdates = 0
      try autoreleasepool
      {
         try adapter.prepare(scenario: prepared[scenarioIndex].scenario, loader: loader)
         try performResetIfNeeded(for: prepared[scenarioIndex])
         try recordRendererDiagnostics()
      }
#if os(macOS)
      try prepareTrustedInputPlan()
#endif
      scenarioStartedAt = timestamp
      scenarioFirstSequence = UInt64(ring.count)
      displayLink?.isPaused = false
      drainSchedule(now: timestamp)
   }

#if os(macOS)
   private var allowsNonClaimLifecycleStimuli: Bool
   {
      invocation.request.passID == "attribution-time-profiler"
         && invocation.scaleOverlayArtifact != nil
   }

   private func prepareTrustedInputSession() throws
   {
      guard let trustedInputCoordinator else
      {
         throw MacOSTrustedInputFailure.invalidState("trusted input observer is not installed")
      }
      guard trustedInputPlans.isEmpty else
      {
         throw MacOSTrustedInputFailure.invalidState("trusted input session is already prepared")
      }
      trustedInputPlans = try prepared.map
      {
         try MacOSTrustedInputCompiler.compile(
            $0.schedule.orderedTraceEvents,
            allowNonClaimLifecycleStimuli: allowsNonClaimLifecycleStimuli
         )
      }
      var sequence = UInt64(0)
      for (scenario, plan) in zip(prepared, trustedInputPlans)
      {
         for command in plan.commands
         {
            try trustedInputCoordinator.prepare(command: command, scenarioID: scenario.scenario.id, sequence: sequence)
            sequence += 1
         }
      }
      try trustedInputCoordinator.finishPreparation(commandCount: sequence)
   }

   private func prepareTrustedInputPlan() throws
   {
      guard trustedInputPlans.indices.contains(scenarioIndex) else
      {
         throw MacOSTrustedInputFailure.invalidState("missing prepared scenario input plan")
      }
      trustedInputPlan = trustedInputPlans[scenarioIndex]
      trustedInputEventIndex = 0
      awaitingTrustedInput = false
      deferredActions.removeAll(keepingCapacity: true)
   }

   private func consumeMacOSTraceEvent(_ event: BenchmarkTraceEvent, timestamp: UInt64) throws
   {
      guard let trustedInputPlan else
      {
         throw MacOSTrustedInputFailure.invalidState("missing scenario input plan")
      }
      let disposition = try trustedInputPlan.disposition(eventIndex: trustedInputEventIndex)
      trustedInputEventIndex += 1
      switch disposition
      {
      case .applicationStimulus:
         try consumeMacOSApplicationStimulus(event, timestamp: timestamp)
      case .absorbed:
         break
      case .command(let commandIndex):
         guard trustedInputPlan.commands.indices.contains(commandIndex), !awaitingTrustedInput else
         {
            throw MacOSTrustedInputFailure.invalidState("trusted command \(commandIndex)")
         }
         let command = trustedInputPlan.commands[commandIndex]
         let commandSequence = trustedInputCommandSequence
         trustedInputCommandSequence += 1
         let mutation = event.op == "mutate" || event.op == "navigate"
         let identifier = event.stateId.map(benchmarkStableID) ?? 0
         awaitingTrustedInput = true
         do
         {
            guard let trustedInputCoordinator else
            {
               throw MacOSTrustedInputFailure.invalidState("trusted input observer is not installed")
            }
            try trustedInputCoordinator.submit(
               command: command,
               scenarioID: prepared[scenarioIndex].scenario.id,
               sequence: commandSequence
            )
            {
               [weak self] result in
               guard let self, !self.terminal else {return}
               switch result
               {
               case .success(let receipt):
                  do
                  {
                     if self.measuredPhase
                     {
                        BenchmarkPresentationTrace.inputReceived(scenarioIndex: UInt64(self.scenarioIndex), generation: self.logicalUpdates)
                     }
                     self.ring.append(kind: mutation ? .mutationBegin : .inputReceived, identifier: identifier, value0: Int64(self.logicalUpdates), timestamp: receipt.application.applicationReceivedTimestamp)
                     self.ring.append(kind: .sceneUpdateBegin, identifier: identifier, timestamp: receipt.application.applicationReceivedTimestamp)
                     try self.recordRendererDiagnostics()
                     self.logicalUpdates += 1
                     if self.measuredPhase
                     {
                        BenchmarkPresentationTrace.visualGeneration(scenarioIndex: UInt64(self.scenarioIndex), generation: self.logicalUpdates)
                     }
                     let completed = mach_continuous_time()
                     self.ring.append(kind: .sceneUpdateEnd, identifier: identifier, timestamp: completed)
                     self.ring.append(kind: .logicalUpdateCompleted, identifier: identifier, value0: Int64(self.logicalUpdates), timestamp: completed)
                     if mutation
                     {
                        self.ring.append(kind: .mutationEnd, identifier: identifier, value0: Int64(self.logicalUpdates), timestamp: completed)
                     }
                     self.awaitingTrustedInput = false
                     self.drainSchedule(now: completed)
                  }
                  catch
                  {
                     self.fail(error)
                  }
               case .failure(let error):
                  self.fail(error)
               }
            }
         }
         catch
         {
            awaitingTrustedInput = false
            throw error
         }
      }
   }

   private func consumeMacOSApplicationStimulus(_ event: BenchmarkTraceEvent, timestamp: UInt64) throws
   {
      let mutation = event.op == "mutate" || event.op == "navigate"
      let identifier = event.stateId.map(benchmarkStableID) ?? 0
      ring.append(kind: mutation ? .mutationBegin : .inputReceived, identifier: identifier, value0: Int64(logicalUpdates), timestamp: timestamp)
      ring.append(kind: .sceneUpdateBegin, identifier: identifier, timestamp: timestamp)
      try adapter.apply(event: event)
      try recordRendererDiagnostics()
      logicalUpdates += 1
      if measuredPhase
      {
         BenchmarkPresentationTrace.visualGeneration(scenarioIndex: UInt64(scenarioIndex), generation: logicalUpdates)
      }
      let completed = mach_continuous_time()
      ring.append(kind: .sceneUpdateEnd, identifier: identifier, timestamp: completed)
      ring.append(kind: .logicalUpdateCompleted, identifier: identifier, value0: Int64(logicalUpdates), timestamp: completed)
      if mutation
      {
         ring.append(kind: .mutationEnd, identifier: identifier, value0: Int64(logicalUpdates), timestamp: completed)
      }
   }
#endif

   private func runCorrectness() throws
   {
      guard let virtualClock = adapter as? BenchmarkVirtualClockAdapter else
      {
         throw BenchmarkCampaignFailure.virtualClockUnavailable
      }
      scenarioIndex = 0
      while scenarioIndex < prepared.count
      {
         let scenario = prepared[scenarioIndex].scenario
         var schedule = prepared[scenarioIndex].schedule
         var checkpoints = [BenchmarkCheckpointEvidence]()
         logicalUpdates = 0
         try autoreleasepool
         {
            try schedule.drainTimed(until: schedule.durationUs)
            {
               timeUs, action in
               try virtualClock.setVirtualTimeUs(timeUs)
               switch action
               {
               case .traceEvent(let event):
                  try adapter.apply(event: event)
                  logicalUpdates += 1
               case .checkpoint(let id):
                  checkpoints.append(try autoreleasepool
                  {
                     try captureCheckpoint(scenario: scenario, checkpointID: id)
                  })
               case .scenarioBegin, .phaseBegin, .phaseEnd, .scenarioEnd:
                  break
               }
            }
         }
         guard schedule.isComplete,
               checkpoints.map(\.checkpointID) == scenario.parityCheckpoints.map(\.id) else
         {
            throw BenchmarkCampaignFailure.checkpointMismatch("\(scenario.id):schedule")
         }
         scenarioEvidence.append(BenchmarkScenarioEvidence(
            scenarioID: scenario.id,
            firstTelemetrySequence: 0,
            lastTelemetrySequence: 0,
            checkpointStateSHA256: Dictionary(uniqueKeysWithValues: checkpoints.map {($0.checkpointID, $0.actualState.sha256)}),
            checkpointAccessibilitySHA256: Dictionary(uniqueKeysWithValues: checkpoints.map {($0.checkpointID, $0.actualAccessibility.sha256)}),
            checkpoints: checkpoints,
            completedLogicalUpdates: logicalUpdates
         ))
         try finishScenario(prepared[scenarioIndex])
         scenarioIndex += 1
         if scenarioIndex < prepared.count
         {
            try autoreleasepool
            {
               try adapter.prepare(scenario: prepared[scenarioIndex].scenario, loader: loader)
               try performResetIfNeeded(for: prepared[scenarioIndex])
            }
         }
      }
      try finishCorrectness()
   }

   private func captureCheckpoint(scenario: BenchmarkScenario, checkpointID: String, evidenceDirectory: String = "evidence") throws -> BenchmarkCheckpointEvidence
   {
      guard let quiescence = adapter as? BenchmarkQuiescenceAdapter else
      {
         throw BenchmarkCampaignFailure.quiescenceUnavailable
      }
      try quiescence.quiesce()
      guard let expected = scenario.parityCheckpoints.first(where: {$0.id == checkpointID}) else
      {
         throw BenchmarkCampaignFailure.checkpointMismatch("\(scenario.id):\(checkpointID):missing")
      }
      guard let expectedScreenshot = expected.screenshot else
      {
         throw BenchmarkCampaignFailure.checkpointMismatch("\(scenario.id):\(checkpointID):screenshot")
      }
      let evidenceRoot = "\(evidenceDirectory)/\(scenario.id)/\(checkpointID)"
      let observed = try adapter.checkpoint(id: checkpointID)
      let state = try writeEvidence(observed.state, name: "\(evidenceRoot)/state.actual.json")
      let accessibility = try writeEvidence(observed.accessibility, name: "\(evidenceRoot)/accessibility.actual.json")
      let actual = try validateBenchmarkCheckpoint(observed, expected: expected, loader: loader)
      let rawAccessibility: BenchmarkArtifactIdentity?
      if let capture = adapter as? BenchmarkRawAccessibilityCapture
      {
         rawAccessibility = try writeEvidence(capture.rawAccessibilityTree(), name: "\(evidenceRoot)/accessibility.platform.raw.json")
      }
      else
      {
         rawAccessibility = nil
      }
      let geometry: BenchmarkArtifactIdentity?
#if os(macOS)
      guard let geometryCapture = adapter as? BenchmarkMacOSCorrectnessGeometryCapture else
      {
         throw BenchmarkCampaignFailure.correctnessGeometryUnavailable
      }
      let geometryPath = relativePath("\(evidenceRoot)/geometry.actual.json")
      let storedGeometry = try store.durableJSON(geometryCapture.correctnessGeometry(), relativePath: geometryPath)
      geometry = BenchmarkArtifactIdentity(path: geometryPath, sha256: storedGeometry.sha256)
#else
      geometry = nil
#endif
      let screenshot: BenchmarkArtifactIdentity?
      let screenshotValidation: String
      if let capture = adapter as? BenchmarkPreviewCapture
      {
         let png = try capture.previewPNG()
         screenshot = try writeEvidence(png, name: "\(evidenceRoot)/screenshot.actual.png")
         screenshotValidation = "host-static-calibrated-oxide-native-pair-pending"
      }
      else
      {
         screenshot = nil
         screenshotValidation = "capture-unavailable-host-static-calibrated-pair-pending"
      }
      let evidence = BenchmarkCheckpointEvidence(
         checkpointID: checkpointID,
         actualState: state,
         expectedStateSHA256: expected.state.sha256,
         actualAccessibility: accessibility,
         expectedAccessibilitySHA256: expected.accessibility.sha256,
         actualRawAccessibility: rawAccessibility,
         actualGeometry: geometry,
         actualScreenshot: screenshot,
         expectedScreenshotSHA256: expectedScreenshot.sha256,
         visibleRoleCounts: actual.visibleRoleCounts,
         validation: "exact-canonical-state-accessibility-and-role-counts",
         screenshotValidation: screenshotValidation
      )
      _ = try store.durableJSON(evidence, relativePath: relativePath("\(evidenceRoot)/evidence.json"))
      return evidence
   }

   private func removeCorrectnessWarmupArtifacts() throws
   {
      let directory = store.root.appendingPathComponent(relativePath("warmup"), isDirectory: true)
      if FileManager.default.fileExists(atPath: directory.path)
      {
         try FileManager.default.removeItem(at: directory)
      }
   }

   private func performResetIfNeeded(for preparedScenario: PreparedBenchmarkScenario) throws
   {
      guard let segment = preparedScenario.resetSegment,
            let expected = preparedScenario.scenario.parityCheckpoints.first else
      {
         return
      }
      guard let quiescence = adapter as? BenchmarkQuiescenceAdapter else
      {
         throw BenchmarkCampaignFailure.quiescenceUnavailable
      }
      try quiescence.quiesce()
      let before = try adapter.checkpoint(id: expected.id)
      try adapter.reset()
      try quiescence.quiesce()
      let after = try adapter.checkpoint(id: expected.id)
      guard try benchmarkCanonicalJSON(before.state) == benchmarkCanonicalJSON(after.state),
            try benchmarkCanonicalJSON(before.accessibility) == benchmarkCanonicalJSON(after.accessibility),
            try benchmarkRoleCountMap(before.visibleRoleCounts) == benchmarkRoleCountMap(after.visibleRoleCounts) else
      {
         throw BenchmarkCampaignFailure.checkpointMismatch("\(segment.id):reset")
      }
      let evidenceRoot = "resets/\(segment.id)/sentinel"
      let evidence = BenchmarkResetEvidence(
         segmentID: segment.id,
         orderedScenarioIDs: segment.orderedScenarioIds,
         anchorScenarioID: preparedScenario.scenario.id,
         checkpointID: expected.id,
         beforeState: try writeEvidence(before.state, name: "\(evidenceRoot)/before.state.json"),
         afterState: try writeEvidence(after.state, name: "\(evidenceRoot)/after.state.json"),
         beforeAccessibility: try writeEvidence(before.accessibility, name: "\(evidenceRoot)/before.accessibility.json"),
         afterAccessibility: try writeEvidence(after.accessibility, name: "\(evidenceRoot)/after.accessibility.json"),
         visibleRoleCounts: after.visibleRoleCounts,
         quiescenceValidation: "adapter-owned-cpu-gpu-drain-and-no-active-scene-animations",
         validation: "adapter-reset-exact-seed-sentinel-equality"
      )
      resetEvidence.append(evidence)
      _ = try store.durableJSON(evidence, relativePath: relativePath("\(evidenceRoot)/evidence.json"))
   }

   private func finishScenario(_ preparedScenario: PreparedBenchmarkScenario) throws
   {
#if os(macOS)
      guard let surfaceProvider = adapter as? BenchmarkMacOSSurfaceProvider else
      {
         throw BenchmarkCampaignFailure.displayLinkUnavailable
      }
      do
      {
         surfaceRuntimeSnapshot = try surfaceProvider.macOSSurfaceSnapshot()
      }
      catch
      {
         throw BenchmarkCampaignFailure.invalidPack("macos-surface-attestation:\(error)")
      }
      if invocation.scaleOverlayArtifact != nil
      {
         guard scaleRuntimeAttestation == nil,
               let scaled = adapter as? BenchmarkMacOSScaleConfiguredAdapter else
         {
            throw BenchmarkCampaignFailure.invalidPack("macos-scale-attestation")
         }
         do
         {
            scaleRuntimeAttestation = try scaled.scaleRuntimeAttestation()
         }
         catch
         {
            throw BenchmarkCampaignFailure.invalidPack("macos-scale-attestation:\(error)")
         }
      }
#endif
      displayLink?.isPaused = true
      guard let segment = preparedScenario.recoverySegment else
      {
         if (adapter as? BenchmarkRendererDiagnosticsAdapter)?.rendererDiagnosticsEnabled == true
         {
            guard let quiescence = adapter as? BenchmarkQuiescenceAdapter else
            {
               throw BenchmarkCampaignFailure.quiescenceUnavailable
            }
            try quiescence.quiesce()
            try recordRendererDiagnostics()
         }
         try autoreleasepool {try adapter.teardown()}
         try drainRetiredAdapterResources()
         return
      }
      guard let quiescence = adapter as? BenchmarkQuiescenceAdapter else
      {
         throw BenchmarkCampaignFailure.quiescenceUnavailable
      }
      guard let fixedBaseline = fixedPackBaselinePhysicalFootprintBytes else
      {
         throw BenchmarkCampaignFailure.footprintUnavailable
      }
      try quiescence.quiesce()
      try recordRendererDiagnostics()
      try autoreleasepool {try adapter.teardown()}
      try drainRetiredAdapterResources()
      malloc_zone_pressure_relief(nil, 0)
      let recoveryLimit = try benchmarkFixedFootprintRecoveryLimit(fixedBaseline)
      let initialPostTeardownFootprint = try benchmarkPhysicalFootprintBytes()
      let attempt = BenchmarkResetRecoveryAttemptEvidence(
         schemaVersion: 1,
         segmentID: segment.id,
         fixedPackBaselinePhysicalFootprintBytes: fixedBaseline,
         initialPostTeardownPhysicalFootprintBytes: initialPostTeardownFootprint,
         recoveryLimitBytes: recoveryLimit,
         recoveryDeadlineMs: preparedScenario.resetDeadlineMs
      )
      _ = try store.durableJSON(attempt, relativePath: relativePath("recoveries/\(segment.id)/attempt.json"))
      let recoveredFootprint = try waitForFootprintRecovery(limit: recoveryLimit, deadlineMs: preparedScenario.resetDeadlineMs)
      if invocation.request.passID != "correctness"
      {
         let timestamp = mach_continuous_time()
         ring.append(kind: .quiescence, identifier: benchmarkStableID(segment.id), value0: Int64(recoveredFootprint), value1: Int64(recoveryLimit), timestamp: timestamp)
         ring.append(kind: .resetComplete, identifier: benchmarkStableID(segment.id), timestamp: timestamp)
      }
      let evidence = BenchmarkResetRecoveryEvidence(
         segmentID: segment.id,
         fixedPackBaselinePhysicalFootprintBytes: fixedBaseline,
         recoveredPhysicalFootprintBytes: recoveredFootprint,
         recoveryLimitBytes: recoveryLimit,
         recoveryDeadlineMs: preparedScenario.resetDeadlineMs,
         validation: "post-segment-quiesced-teardown-fixed-pack-baseline-five-percent-recovery"
      )
      resetRecoveryEvidence.append(evidence)
      _ = try store.durableJSON(evidence, relativePath: relativePath("recoveries/\(segment.id)/evidence.json"))
   }

   private func waitForFootprintRecovery(limit: UInt64, deadlineMs: UInt64) throws -> UInt64
   {
      let startedAt = mach_continuous_time()
      while true
      {
         malloc_zone_pressure_relief(nil, 0)
         let footprint = try benchmarkPhysicalFootprintBytes()
         if footprint <= limit
         {
            return footprint
         }
         let elapsedNs = machTicksToNanoseconds(mach_continuous_time() &- startedAt)
         if elapsedNs >= deadlineMs * 1_000_000
         {
            throw BenchmarkCampaignFailure.footprintRecoveryTimeout(footprint, limit)
         }
         autoreleasepool
         {
            RunLoop.current.run(until: Date(timeIntervalSinceNow: 0.01))
         }
      }
   }

   private func drainRetiredAdapterResources() throws
   {
      try (adapter as? BenchmarkRetiredResourceAdapter)?.drainRetiredResources()
   }

   private func recordRendererDiagnostics() throws
   {
      guard let source = adapter as? BenchmarkRendererDiagnosticsAdapter,
            source.rendererDiagnosticsEnabled,
            let diagnostics = try source.rendererDiagnostics() else {return}
      let timestamp = mach_continuous_time()
      if diagnostics.submittedFrameID > 0,
         diagnostics.renderPrepareBeginTicks >= (ring.lastTimestamp ?? 0),
         diagnostics.renderPrepareBeginTicks <= diagnostics.renderPrepareEndTicks,
         diagnostics.renderPrepareEndTicks <= diagnostics.drawableWaitBeginTicks,
         diagnostics.drawableWaitBeginTicks <= diagnostics.drawableWaitEndTicks,
         diagnostics.drawableWaitEndTicks <= diagnostics.encodeBeginTicks,
         diagnostics.encodeBeginTicks <= diagnostics.encodeEndTicks,
         diagnostics.encodeEndTicks <= diagnostics.commandSubmitTicks,
         diagnostics.commandSubmitTicks <= timestamp
      {
         ring.append(kind: .renderPrepareBegin, identifier: diagnostics.submittedFrameID, timestamp: diagnostics.renderPrepareBeginTicks)
         ring.append(kind: .renderPrepareEnd, identifier: diagnostics.submittedFrameID, timestamp: diagnostics.renderPrepareEndTicks)
         ring.append(
            kind: .drawableWait,
            identifier: diagnostics.submittedFrameID,
            value0: Int64(clamping: diagnostics.drawableWaitEndTicks - diagnostics.drawableWaitBeginTicks),
            timestamp: diagnostics.drawableWaitEndTicks
         )
         ring.append(kind: .encodeBegin, identifier: diagnostics.submittedFrameID, timestamp: diagnostics.encodeBeginTicks)
         ring.append(
            kind: .encodeEnd,
            identifier: diagnostics.submittedFrameID,
            value0: Int64(clamping: diagnostics.encodedBytes),
            value1: Int64(clamping: diagnostics.drawCalls),
            timestamp: diagnostics.encodeEndTicks
         )
         ring.append(
            kind: .commandSubmit,
            identifier: diagnostics.submittedFrameID,
            value0: Int64(clamping: diagnostics.damagePixels),
            value1: Int64(clamping: diagnostics.damageRects),
            timestamp: diagnostics.commandSubmitTicks
         )
      }
      if diagnostics.completedFrameID > 0
      {
         ring.append(
            kind: .gpuDuration,
            identifier: diagnostics.completedFrameID,
            value0: Int64(clamping: diagnostics.gpuDurationNs),
            value1: Int64(clamping: diagnostics.gpuRenderDurationNs),
            flags: 1,
            timestamp: timestamp
         )
      }
   }

   private func writeEvidence(_ data: Data, name: String) throws -> BenchmarkArtifactIdentity
   {
      let path = relativePath(name)
      try store.durableWrite(data, to: store.root.appendingPathComponent(path))
      return BenchmarkArtifactIdentity(path: path, sha256: comparisonSHA256(data))
   }

#if os(macOS)
   private func writeSurfaceReceipt() throws -> BenchmarkArtifactIdentity
   {
      guard let surfaceRuntimeSnapshot else
      {
         throw BenchmarkCampaignFailure.displayLinkUnavailable
      }
      let observedRefreshMillihz: UInt64?
      if observedDisplayIntervalCount > 0, observedDisplayIntervalNs > 0
      {
         let numerator = observedDisplayIntervalCount.multipliedFullWidth(by: 1_000_000_000_000)
         observedRefreshMillihz = observedDisplayIntervalNs.dividingFullWidth(numerator).quotient
      }
      else
      {
         observedRefreshMillihz = nil
      }
      let receipt = BenchmarkMacOSSurfaceReceipt(
         schemaVersion: 1,
         runID: invocation.request.runID,
         planSHA256: invocation.request.planSHA256,
         chunkID: invocation.chunkID,
         passID: invocation.request.passID,
         pairIndex: invocation.request.pairIndex,
         side: invocation.request.side,
         generation: invocation.request.generation,
         packID: invocation.packID,
         snapshot: surfaceRuntimeSnapshot,
         observedRefreshMillihz: observedRefreshMillihz,
         observedRefreshSource: observedRefreshMillihz == nil ? "unavailable-no-measured-display-opportunity" : "cadisplaylink-target-timestamp-interval-mean-v1",
         validation: "complete-app-reported-content-hash-bound-surface-v1"
      )
      let path = relativePath("surface.json")
      let artifact = try store.durableJSON(receipt, relativePath: path)
      return BenchmarkArtifactIdentity(path: path, sha256: artifact.sha256)
   }
#endif

   private func finishCorrectness() throws
   {
      let emptyTelemetry = Data()
#if os(macOS)
      let surfaceReceipt = try writeSurfaceReceipt()
#else
      let surfaceReceipt: BenchmarkArtifactIdentity? = nil
#endif
      let envelope = BenchmarkCampaignEnvelope(
         schemaVersion: 1,
         runID: invocation.request.runID,
         planSHA256: invocation.request.planSHA256,
         chunkID: invocation.chunkID,
         passID: invocation.request.passID,
         pairIndex: invocation.request.pairIndex,
         side: invocation.request.side,
         generation: invocation.request.generation,
         packID: invocation.packID,
         predecessorCheckpointSHA256: nil,
         telemetrySHA256: comparisonSHA256(emptyTelemetry),
         telemetryByteCount: 0,
         telemetryCoverage: nil,
         surfaceReceipt: surfaceReceipt,
         timebaseNumerator: timebase.numer,
         timebaseDenominator: timebase.denom,
         scenarios: scenarioEvidence,
         resets: resetEvidence,
         resetRecoveries: resetRecoveryEvidence,
         timingClaim: "none-correctness-untimed",
         injectionScope: "deterministic-direct-callback-correctness",
         validation: "complete-state-accessibility-exact-static-pair-host-reducer-pending"
      )
      let artifact = try store.durableJSON(envelope, relativePath: relativePath("complete.json"))
      _ = try store.durableJSON(
         ComparisonProbeAcknowledgement(
            schemaVersion: 1,
            generation: invocation.request.generation,
            artifactSHA256: artifact.sha256,
            durable: true
         ),
         relativePath: relativePath("complete.ack.json")
      )
      terminal = true
      completion(.success(envelope))
   }

   private func finish() throws
   {
#if os(macOS)
      try trustedInputCoordinator?.flush()
      trustedInputCoordinator?.cancel()
      deferredActions.removeAll(keepingCapacity: false)
#endif
      displayLink?.invalidate()
      displayLink = nil
      let sessionID = "\(invocation.request.runID):\(invocation.chunkID):\(invocation.request.pairIndex):\(invocation.request.side.rawValue)"
      let telemetry = try ring.binarySnapshot(identity: BenchmarkTelemetryIdentity(
         planSHA256: invocation.request.planSHA256,
         generation: invocation.request.generation,
         chunkID: invocation.chunkID,
         passID: invocation.request.passID,
         sessionID: sessionID,
         timebaseNumerator: timebase.numer,
         timebaseDenominator: timebase.denom
      ))
      let telemetryPath = relativePath("telemetry.bin")
      try store.durableWrite(telemetry, to: store.root.appendingPathComponent(telemetryPath))
      let coveragePath = relativePath("telemetry.coverage.json")
      let coverage = try benchmarkTelemetryCoverage(
         records: ring.snapshot(),
         side: invocation.request.side,
         passID: invocation.request.passID,
         rendererDiagnosticsEnabled: (adapter as? BenchmarkRendererDiagnosticsAdapter)?.rendererDiagnosticsEnabled == true
      )
      let storedCoverage = try store.durableJSON(coverage, relativePath: coveragePath)
      let coverageIdentity = BenchmarkArtifactIdentity(path: coveragePath, sha256: storedCoverage.sha256)
#if os(macOS)
      let surfaceReceipt = try writeSurfaceReceipt()
#else
      let surfaceReceipt: BenchmarkArtifactIdentity? = nil
#endif
      if let identity = invocation.scaleOverlayArtifact,
         let first = prepared.first
      {
         let overlay = try loader.loadMacOSComparatorScaleOverlay(identity, scenario: first.scenario)
         guard let observed = scaleRuntimeAttestation else
         {
            throw BenchmarkCampaignFailure.invalidPack("macos-scale-attestation")
         }
         let attestation = BenchmarkMacOSComparatorRuntimeAttestation(
            schemaVersion: 1,
            scenarioID: first.scenario.id,
            side: invocation.request.side == .native ? "appkit" : "oxide",
            scale: overlay.scale,
            scaleOverlaySHA256: identity.sha256,
            applicationRunCount: 1,
            effectiveCardinality: observed.effectiveCardinality,
            completed: observed.completed
         )
         _ = try store.durableJSON(attestation, relativePath: relativePath("qualification/scale-attestation.json"))
      }
      let envelope = BenchmarkCampaignEnvelope(
         schemaVersion: 1,
         runID: invocation.request.runID,
         planSHA256: invocation.request.planSHA256,
         chunkID: invocation.chunkID,
         passID: invocation.request.passID,
         pairIndex: invocation.request.pairIndex,
         side: invocation.request.side,
         generation: invocation.request.generation,
         packID: invocation.packID,
         predecessorCheckpointSHA256: nil,
         telemetrySHA256: comparisonSHA256(telemetry),
         telemetryByteCount: UInt64(telemetry.count),
         telemetryCoverage: coverageIdentity,
         surfaceReceipt: surfaceReceipt,
         timebaseNumerator: timebase.numer,
         timebaseDenominator: timebase.denom,
         scenarios: scenarioEvidence,
         resets: resetEvidence,
         resetRecoveries: resetRecoveryEvidence,
         timingClaim: "diagnostic-telemetry-not-primary",
         injectionScope: "direct-callback-diagnostic",
         validation: "complete-diagnostic-not-claim-bearing"
      )
      let artifact = try store.durableJSON(envelope, relativePath: relativePath("complete.json"))
      _ = try store.durableJSON(
         ComparisonProbeAcknowledgement(
            schemaVersion: 1,
            generation: invocation.request.generation,
            artifactSHA256: artifact.sha256,
            durable: true
         ),
         relativePath: relativePath("complete.ack.json")
      )
      terminal = true
      completion(.success(envelope))
   }

   private func fail(_ error: Error)
   {
      guard !terminal else {return}
      terminal = true
#if os(macOS)
      trustedInputCoordinator?.cancel()
      deferredActions.removeAll(keepingCapacity: false)
#endif
      displayLink?.invalidate()
      displayLink = nil
      try? adapter.teardown()
      try? store.durableWrite(
         Data(String(describing: error).utf8),
         to: store.root.appendingPathComponent(relativePath("failure.txt"))
      )
      completion(.failure(error))
   }

   private func relativePath(_ name: String) -> String
   {
      "Runs/\(invocation.request.runID)/\(invocation.chunkID)/\(invocation.request.passID)/\(invocation.packID)/\(invocation.request.pairIndex)/\(invocation.request.side.rawValue).\(name)"
   }

   private func machTicksToNanoseconds(_ ticks: UInt64) -> UInt64
   {
      let product = ticks.multipliedFullWidth(by: UInt64(timebase.numer))
      return UInt64(timebase.denom).dividingFullWidth(product).quotient
   }
}

#if os(macOS)
enum BenchmarkTargetedMode: String, Codable
{
   case visual
   case timing
}

struct BenchmarkTargetedSelection: Equatable
{
   let scenarioID: String
   let checkpointID: String
   let mode: BenchmarkTargetedMode
   let iterations: UInt32
}

private struct BenchmarkTargetedEvidence: Codable
{
   let schemaVersion: UInt32
   let side: String
   let mode: BenchmarkTargetedMode
   let scenarioID: String
   let checkpointID: String
   let actualState: BenchmarkArtifactIdentity
   let actualGeometry: BenchmarkArtifactIdentity?
   let actualScreenshot: BenchmarkArtifactIdentity?
   let visibleRoleCounts: [BenchmarkRoleCount]
   let samplesNs: [UInt64]
   let outputReadyBoundary: String?
   let validation: String
}

private struct BenchmarkTargetedComplete: Codable
{
   let schemaVersion: UInt32
   let side: String
   let mode: BenchmarkTargetedMode
   let scenarioID: String
   let checkpointID: String
   let iterations: UInt32
   let evidence: BenchmarkArtifactIdentity
   let validation: String
}

final class BenchmarkTargetedExecutor
{
   private let side: String
   private let selection: BenchmarkTargetedSelection
   private let loader: BenchmarkSpecLoader
   private let adapter: BenchmarkScenarioAdapter
   private let store: DurableArtifactStore
   private var timebase = mach_timebase_info_data_t()

   init(side: String, selection: BenchmarkTargetedSelection, loader: BenchmarkSpecLoader, adapter: BenchmarkScenarioAdapter, store: DurableArtifactStore) throws
   {
      guard side == "native" || side == "oxide",
            !selection.scenarioID.isEmpty,
            !selection.checkpointID.isEmpty,
            (1...100).contains(selection.iterations) else
      {
         throw BenchmarkCampaignFailure.invalidPack("targeted-selection")
      }
      self.side = side
      self.selection = selection
      self.loader = loader
      self.adapter = adapter
      self.store = store
      mach_timebase_info(&timebase)
   }

   func run() throws
   {
      let scenario = try loader.loadScenario(relativePath: "scenarios/\(selection.scenarioID).json")
      guard scenario.id == selection.scenarioID,
            let checkpoint = scenario.parityCheckpoints.first(where: {$0.id == selection.checkpointID}) else
      {
         throw BenchmarkCampaignFailure.checkpointMismatch("\(selection.scenarioID):\(selection.checkpointID)")
      }
      (adapter as? BenchmarkPassConfiguredAdapter)?.configure(passID: "correctness")
      try adapter.prepare(scenario: scenario, loader: loader)
      defer {try? adapter.teardown()}

      let evidence: BenchmarkTargetedEvidence
      switch selection.mode
      {
      case .visual:
         guard selection.iterations == 1 else
         {
            throw BenchmarkCampaignFailure.invalidPack("targeted-visual-iterations")
         }
         evidence = try runVisual(scenario: scenario, checkpoint: checkpoint)
      case .timing:
         evidence = try runTiming(scenario: scenario, checkpoint: checkpoint)
      }
      let evidencePath = "\(side).evidence/\(scenario.id)/\(checkpoint.id)/\(selection.mode.rawValue).evidence.json"
      let stored = try store.durableJSON(evidence, relativePath: evidencePath)
      _ = try store.durableJSON(
         BenchmarkTargetedComplete(
            schemaVersion: 1,
            side: side,
            mode: selection.mode,
            scenarioID: scenario.id,
            checkpointID: checkpoint.id,
            iterations: selection.iterations,
            evidence: BenchmarkArtifactIdentity(path: evidencePath, sha256: stored.sha256),
            validation: "complete-targeted-\(selection.mode.rawValue)-v1"
         ),
         relativePath: "\(side).\(selection.mode.rawValue).complete.json"
      )
   }

   private func runVisual(scenario: BenchmarkScenario, checkpoint: BenchmarkParityCheckpoint) throws -> BenchmarkTargetedEvidence
   {
      let stateAndRoles = try replay(scenario: scenario, checkpoint: checkpoint, measure: false)
      guard let geometryCapture = adapter as? BenchmarkMacOSCorrectnessGeometryCapture,
            let preview = adapter as? BenchmarkPreviewCapture else
      {
         throw BenchmarkCampaignFailure.correctnessGeometryUnavailable
      }
      let root = "\(side).evidence/\(scenario.id)/\(checkpoint.id)"
      let state = try write(stateAndRoles.state, relativePath: "\(root)/state.actual.json")
      let storedGeometry = try store.durableJSON(geometryCapture.correctnessGeometry(), relativePath: "\(root)/geometry.actual.json")
      let geometry = BenchmarkArtifactIdentity(path: "\(root)/geometry.actual.json", sha256: storedGeometry.sha256)
      let screenshot = try write(preview.previewPNG(), relativePath: "\(root)/screenshot.actual.png")
      return BenchmarkTargetedEvidence(
         schemaVersion: 1,
         side: side,
         mode: .visual,
         scenarioID: scenario.id,
         checkpointID: checkpoint.id,
         actualState: state,
         actualGeometry: geometry,
         actualScreenshot: screenshot,
         visibleRoleCounts: stateAndRoles.roles,
         samplesNs: [],
         outputReadyBoundary: nil,
         validation: "exact-canonical-state-role-counts-geometry-and-png"
      )
   }

   private func runTiming(scenario: BenchmarkScenario, checkpoint: BenchmarkParityCheckpoint) throws -> BenchmarkTargetedEvidence
   {
      var samples = [UInt64]()
      samples.reserveCapacity(Int(selection.iterations))
      var finalState = Data()
      var finalRoles = [BenchmarkRoleCount]()
      for iteration in 0..<selection.iterations
      {
         if iteration > 0
         {
            try adapter.reset()
         }
         let result = try replay(scenario: scenario, checkpoint: checkpoint, measure: true)
         guard let duration = result.durationNs else
         {
            throw BenchmarkCampaignFailure.checkpointMismatch("\(scenario.id):\(checkpoint.id):timing")
         }
         samples.append(duration)
         finalState = result.state
         finalRoles = result.roles
      }
      let root = "\(side).evidence/\(scenario.id)/\(checkpoint.id)"
      return BenchmarkTargetedEvidence(
         schemaVersion: 1,
         side: side,
         mode: .timing,
         scenarioID: scenario.id,
         checkpointID: checkpoint.id,
         actualState: try write(finalState, relativePath: "\(root)/state.actual.json"),
         actualGeometry: nil,
         actualScreenshot: nil,
         visibleRoleCounts: finalRoles,
         samplesNs: samples,
         outputReadyBoundary: "benchmark-mutation-dispatch-to-app-output-ready",
         validation: "exact-canonical-state-and-role-counts-after-every-sample"
      )
   }

   private func replay(scenario: BenchmarkScenario, checkpoint: BenchmarkParityCheckpoint, measure: Bool) throws -> (state: Data, roles: [BenchmarkRoleCount], durationNs: UInt64?)
   {
      guard let targetPhase = scenario.phases.first(where: {$0.id == checkpoint.phaseId}) else
      {
         throw BenchmarkCampaignFailure.checkpointMismatch("\(scenario.id):\(checkpoint.id):phase")
      }
      let checkpointTimeUs = checkpoint.atUs ?? 0
      let targetEvent = try targetPhase.trace.map(loader.loadTrace)?.last(where: {$0.atUs <= checkpointTimeUs})
      if measure && targetEvent == nil
      {
         throw BenchmarkCampaignFailure.checkpointMismatch("\(scenario.id):\(checkpoint.id):mutation")
      }
      guard !measure || adapter is BenchmarkOutputReadyAdapter else
      {
         throw BenchmarkCampaignFailure.quiescenceUnavailable
      }
      guard let virtualClock = adapter as? BenchmarkVirtualClockAdapter else
      {
         throw BenchmarkCampaignFailure.virtualClockUnavailable
      }
      var schedule = try BenchmarkPhaseSchedule(scenario: scenario, setupDurationUs: 0, loader: loader)
      var activePhase = ""
      var state: Data?
      var roles = [BenchmarkRoleCount]()
      var durationNs: UInt64?
      try schedule.drainTimed(until: schedule.durationUs)
      {
         timeUs, action in
         guard state == nil else {return}
         try virtualClock.setVirtualTimeUs(timeUs)
         switch action
         {
         case .phaseBegin(let phaseID, _):
            activePhase = phaseID
         case .traceEvent(let event):
            if measure, activePhase == checkpoint.phaseId, event == targetEvent
            {
               let started = mach_continuous_time()
               try adapter.apply(event: event)
               try (adapter as! BenchmarkOutputReadyAdapter).outputReady()
               durationNs = ticksToNanoseconds(mach_continuous_time() - started)
            }
            else
            {
               try adapter.apply(event: event)
            }
         case .checkpoint(let checkpointID) where checkpointID == checkpoint.id:
            if !measure
            {
               guard let quiescence = adapter as? BenchmarkQuiescenceAdapter else
               {
                  throw BenchmarkCampaignFailure.quiescenceUnavailable
               }
               try quiescence.quiesce()
            }
            let actual = try adapter.checkpoint(id: checkpoint.id)
            let canonical = try benchmarkCanonicalJSON(actual.state)
            let expected = try benchmarkCanonicalJSON(loader.read(checkpoint.state))
            guard canonical == expected,
                  try benchmarkRoleCountMap(actual.visibleRoleCounts) == benchmarkRoleCountMap(checkpoint.expectedVisibleRoleCounts) else
            {
               throw BenchmarkCampaignFailure.checkpointMismatch("\(scenario.id):\(checkpoint.id):state-or-roles")
            }
            state = canonical
            roles = actual.visibleRoleCounts
         case .phaseEnd, .scenarioBegin, .scenarioEnd, .checkpoint:
            break
         }
      }
      guard let state else
      {
         throw BenchmarkCampaignFailure.checkpointMismatch("\(scenario.id):\(checkpoint.id):unreached")
      }
      return (state, roles, durationNs)
   }

   private func ticksToNanoseconds(_ ticks: UInt64) -> UInt64
   {
      let product = ticks.multipliedFullWidth(by: UInt64(timebase.numer))
      return UInt64(timebase.denom).dividingFullWidth(product).quotient
   }

   private func write(_ data: Data, relativePath: String) throws -> BenchmarkArtifactIdentity
   {
      try store.durableWrite(data, to: store.root.appendingPathComponent(relativePath))
      return BenchmarkArtifactIdentity(path: relativePath, sha256: comparisonSHA256(data))
   }
}

func benchmarkTargetedSelection(arguments: [String] = CommandLine.arguments) throws -> BenchmarkTargetedSelection
{
   func value(_ flag: String) throws -> String
   {
      guard let index = arguments.firstIndex(of: flag), index + 1 < arguments.count else
      {
         throw ComparisonContractError.missingArgument(flag)
      }
      return arguments[index + 1]
   }
   let modeText = try value("-oxide-compare-targeted-mode")
   guard let mode = BenchmarkTargetedMode(rawValue: modeText),
         let iterations = UInt32(try value("-oxide-compare-targeted-iterations")) else
   {
      throw ComparisonContractError.invalidArgument("-oxide-compare-targeted-mode-or-iterations")
   }
   return BenchmarkTargetedSelection(
      scenarioID: try value("-oxide-compare-targeted-scenario"),
      checkpointID: try value("-oxide-compare-targeted-checkpoint"),
      mode: mode,
      iterations: iterations
   )
}

private struct BenchmarkReleaseCandidateCheckpointEvidence: Codable
{
   let actualState: BenchmarkArtifactIdentity
   let actualAccessibility: BenchmarkArtifactIdentity
   let actualGeometry: BenchmarkArtifactIdentity
   let actualScreenshot: BenchmarkArtifactIdentity
   let validation: String
}

private struct BenchmarkReleaseCandidateCaptureComplete: Codable
{
   let schemaVersion: UInt32
   let planSHA256: String
   let side: String
   let scenarioIDs: [String]
   let checkpointCount: UInt32
   let timingClaim: String
   let validation: String
}

final class BenchmarkReleaseCandidateCaptureExecutor
{
   static let scenarioIDs = ["grid.large-scroll", "effects.layers", "mutation.damage", "text.multilingual", "resize.theme"]

   private let side: String
   private let planSHA256: String
   private let loader: BenchmarkSpecLoader
   private let adapter: BenchmarkReleaseCandidateScenarioAdapter
   private let store: DurableArtifactStore

   init(side: String, planSHA256: String, loader: BenchmarkSpecLoader, adapter: BenchmarkReleaseCandidateScenarioAdapter, store: DurableArtifactStore) throws
   {
      try validateComparisonSHA256(planSHA256)
      self.side = side
      self.planSHA256 = planSHA256
      self.loader = loader
      self.adapter = adapter
      self.store = store
   }

   func run() throws
   {
      guard side == "native" || side == "oxide" else
      {
         throw BenchmarkCampaignFailure.invalidPack("release-candidate-side")
      }
      var checkpointCount = UInt32(0)
      for scenarioID in Self.scenarioIDs
      {
         let scenario = try loader.loadReleaseCandidateCaptureScenario(id: scenarioID)
         try adapter.prepareReleaseCandidate(scenario: scenario, loader: loader)
         do
         {
            checkpointCount += try capture(scenario)
            try adapter.teardown()
         }
         catch
         {
            try? adapter.teardown()
            throw error
         }
      }
      guard checkpointCount == 18 else
      {
         throw BenchmarkCampaignFailure.checkpointMismatch("release-candidate-count")
      }
      _ = try store.durableJSON(
         BenchmarkReleaseCandidateCaptureComplete(
            schemaVersion: 1,
            planSHA256: planSHA256,
            side: side,
            scenarioIDs: Self.scenarioIDs,
            checkpointCount: checkpointCount,
            timingClaim: "none-correctness-untimed",
            validation: "complete-release-candidate-state-accessibility-geometry-and-png"
         ),
         relativePath: "\(side).evidence/capture.complete.json"
      )
   }

   private func capture(_ scenario: BenchmarkScenario) throws -> UInt32
   {
      guard let virtualClock = adapter as? BenchmarkVirtualClockAdapter,
            let quiescence = adapter as? BenchmarkQuiescenceAdapter,
            let geometry = adapter as? BenchmarkMacOSCorrectnessGeometryCapture,
            let preview = adapter as? BenchmarkPreviewCapture else
      {
         throw BenchmarkCampaignFailure.quiescenceUnavailable
      }
      var schedule = try BenchmarkPhaseSchedule(scenario: scenario, setupDurationUs: 0, loader: loader)
      var observedIDs = [String]()
      try schedule.drainTimed(until: schedule.durationUs)
      {
         timeUs, action in
         try virtualClock.setVirtualTimeUs(timeUs)
         switch action
         {
         case .traceEvent(let event): try adapter.apply(event: event)
         case .checkpoint(let checkpointID):
            try quiescence.quiesce()
            guard let expected = scenario.parityCheckpoints.first(where: {$0.id == checkpointID}) else
            {
               throw BenchmarkCampaignFailure.checkpointMismatch("\(scenario.id):\(checkpointID):missing")
            }
            let actual = try adapter.checkpoint(id: checkpointID)
            _ = try validateBenchmarkCheckpoint(actual, expected: expected, loader: loader)
            let root = "\(side).evidence/\(scenario.id)/\(checkpointID)"
            let state = try write(actual.state, relativePath: "\(root)/state.actual.json")
            let accessibility = try write(actual.accessibility, relativePath: "\(root)/accessibility.actual.json")
            let geometryArtifact = try store.durableJSON(geometry.correctnessGeometry(), relativePath: "\(root)/geometry.actual.json")
            let geometryIdentity = BenchmarkArtifactIdentity(path: "\(root)/geometry.actual.json", sha256: geometryArtifact.sha256)
            let screenshot = try write(preview.previewPNG(), relativePath: "\(root)/screenshot.actual.png")
            _ = try store.durableJSON(
               BenchmarkReleaseCandidateCheckpointEvidence(
                  actualState: state,
                  actualAccessibility: accessibility,
                  actualGeometry: geometryIdentity,
                  actualScreenshot: screenshot,
                  validation: "exact-canonical-state-accessibility-and-role-counts"
               ),
               relativePath: "\(root)/evidence.json"
            )
            observedIDs.append(checkpointID)
         case .scenarioBegin, .phaseBegin, .phaseEnd, .scenarioEnd: break
         }
      }
      guard schedule.isComplete, observedIDs == scenario.parityCheckpoints.map(\.id) else
      {
         throw BenchmarkCampaignFailure.checkpointMismatch("\(scenario.id):schedule")
      }
      return UInt32(observedIDs.count)
   }

   private func write(_ data: Data, relativePath: String) throws -> BenchmarkArtifactIdentity
   {
      try store.durableWrite(data, to: store.root.appendingPathComponent(relativePath))
      return BenchmarkArtifactIdentity(path: relativePath, sha256: comparisonSHA256(data))
   }
}

func benchmarkReleaseCandidateCapturePlanSHA256(arguments: [String] = CommandLine.arguments) throws -> String
{
   let flag = "-oxide-compare-release-candidate-plan-sha"
   guard let index = arguments.firstIndex(of: flag), index + 1 < arguments.count else
   {
      throw ComparisonContractError.missingArgument(flag)
   }
   let value = arguments[index + 1]
   try validateComparisonSHA256(value)
   return value
}
#endif
