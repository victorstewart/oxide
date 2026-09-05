import Foundation

enum BenchmarkScheduledAction: Equatable
{
   case scenarioBegin(String)
   case phaseBegin(String, measured: Bool)
   case traceEvent(BenchmarkTraceEvent)
   case checkpoint(String)
   case phaseEnd(String, measured: Bool)
   case scenarioEnd(String)
}

private struct TimestampedBenchmarkAction
{
   let atUs: UInt64
   let priority: UInt8
   let action: BenchmarkScheduledAction
}

struct BenchmarkPhaseSchedule
{
   let scenarioID: String
   let durationUs: UInt64
   private let actions: [TimestampedBenchmarkAction]
   private var nextAction = 0

   init(scenario: BenchmarkScenario, setupDurationUs: UInt64, timing: AppleCampaignScenarioTimingSpec? = nil, loader: BenchmarkSpecLoader) throws
   {
      scenarioID = scenario.id
      var actions = [TimestampedBenchmarkAction(atUs: 0, priority: 0, action: .scenarioBegin(scenario.id))]
      var cursor = UInt64(0)
      let phaseTiming = try BenchmarkOverlayPhaseTiming(scenario: scenario, timing: timing, loader: loader)
      for phase in scenario.phases
      {
         let sourceTraceEvents = try phase.trace.map(loader.loadTrace) ?? []
         let lastTraceUs = sourceTraceEvents.map(\.atUs).max() ?? 0
         let lastCheckpointUs = scenario.parityCheckpoints
            .filter {$0.phaseId == phase.id}
            .compactMap(\.atUs)
            .max() ?? 0
         let sourceDurationUs = phase.durationMs.map {$0 * 1_000} ?? max(lastTraceUs, lastCheckpointUs)
         let durationUs = try phaseTiming.durationUs(phase: phase, sourceDurationUs: sourceDurationUs, fallbackSetupDurationUs: setupDurationUs)
         let traceEvents = try phaseTiming.traceEvents(phase: phase, source: sourceTraceEvents, sourceDurationUs: sourceDurationUs, targetDurationUs: durationUs)
         actions.append(TimestampedBenchmarkAction(atUs: cursor, priority: 1, action: .phaseBegin(phase.id, measured: phase.measured)))
         var phaseActions = [TimestampedBenchmarkAction]()
         for event in traceEvents
         {
            guard event.atUs <= durationUs else
            {
               throw BenchmarkCampaignFailure.invalidPack(scenario.id)
            }
            phaseActions.append(TimestampedBenchmarkAction(atUs: cursor + event.atUs, priority: 2, action: .traceEvent(event)))
         }
         for checkpoint in scenario.parityCheckpoints where checkpoint.phaseId == phase.id
         {
            let atUs = checkpoint.atUs ?? 0
            guard atUs <= durationUs else
            {
               throw BenchmarkCampaignFailure.invalidPack(scenario.id)
            }
            phaseActions.append(TimestampedBenchmarkAction(atUs: cursor + atUs, priority: 3, action: .checkpoint(checkpoint.id)))
         }
         phaseActions.sort
         {
            left, right in
            left.atUs == right.atUs ? left.priority < right.priority : left.atUs < right.atUs
         }
         actions.append(contentsOf: phaseActions)
         cursor += durationUs
         actions.append(TimestampedBenchmarkAction(atUs: cursor, priority: 4, action: .phaseEnd(phase.id, measured: phase.measured)))
      }
      actions.append(TimestampedBenchmarkAction(atUs: cursor, priority: 5, action: .scenarioEnd(scenario.id)))
      self.durationUs = cursor
      self.actions = actions
   }

   mutating func reset()
   {
      nextAction = 0
   }

   mutating func drain(until elapsedUs: UInt64, _ consume: (BenchmarkScheduledAction) throws -> Void) rethrows
   {
      try drainTimed(until: elapsedUs) {_, action in try consume(action)}
   }

   mutating func drainTimed(until elapsedUs: UInt64, _ consume: (UInt64, BenchmarkScheduledAction) throws -> Void) rethrows
   {
      while nextAction < actions.count && actions[nextAction].atUs <= elapsedUs
      {
         try consume(actions[nextAction].atUs, actions[nextAction].action)
         nextAction += 1
      }
   }

   var isComplete: Bool
   {
      nextAction == actions.count
   }

   var orderedTraceEvents: [BenchmarkTraceEvent]
   {
      actions.compactMap
      {
         if case .traceEvent(let event) = $0.action {return event}
         return nil
      }
   }
}

private struct BenchmarkOverlayPhaseTiming
{
   private let setupDurationUs: UInt64?
   private let warmupDurationUs: UInt64?
   private let measuredDurationUs: [String: UInt64]
   private let iterations: (phaseID: String, sourceCount: UInt32, count: UInt32)?

   init(scenario: BenchmarkScenario, timing: AppleCampaignScenarioTimingSpec?, loader: BenchmarkSpecLoader) throws
   {
      guard let timing else
      {
         setupDurationUs = nil
         warmupDurationUs = nil
         measuredDurationUs = [:]
         iterations = nil
         return
      }
      guard timing.scenarioId == scenario.id else
      {
         throw BenchmarkCampaignFailure.invalidPlan
      }
      setupDurationUs = try benchmarkSecondsToMicroseconds(timing.setupSeconds)
      warmupDurationUs = try benchmarkSecondsToMicroseconds(timing.warmupSeconds)
      let measured = try scenario.phases.filter(\.measured).map
      {
         phase -> (phase: BenchmarkPhase, durationUs: UInt64) in
         let traceEvents = try phase.trace.map(loader.loadTrace) ?? []
         let traceUs = traceEvents.map(\.atUs).max() ?? 0
         let checkpointUs = scenario.parityCheckpoints.filter {$0.phaseId == phase.id}.compactMap(\.atUs).max() ?? 0
         return (phase, phase.durationMs.map {$0 * 1_000} ?? max(traceUs, checkpointUs))
      }
      guard !measured.isEmpty else
      {
         throw BenchmarkCampaignFailure.invalidPlan
      }
      let targetUs = try benchmarkSecondsToMicroseconds(timing.measurement.occupiedSeconds)
      switch timing.measurement
      {
      case .duration:
         measuredDurationUs = try benchmarkDistributeMeasuredDuration(measured, targetUs: targetUs)
         iterations = nil
      case .iterations(let phaseID, let sourceCount, let count, _):
         guard sourceCount > 0,
               count > 0,
               measured.contains(where: {$0.phase.id == phaseID}) else
         {
            throw BenchmarkCampaignFailure.invalidPlan
         }
         let fixedUs = measured.filter {$0.phase.id != phaseID}.reduce(UInt64(0)) {$0 + $1.durationUs}
         guard fixedUs < targetUs else
         {
            throw BenchmarkCampaignFailure.invalidPlan
         }
         measuredDurationUs = Dictionary(uniqueKeysWithValues: measured.map
         {
            ($0.phase.id, $0.phase.id == phaseID ? targetUs - fixedUs : $0.durationUs)
         })
         iterations = (phaseID, sourceCount, count)
      }
   }

   func durationUs(phase: BenchmarkPhase, sourceDurationUs: UInt64, fallbackSetupDurationUs: UInt64) throws -> UInt64
   {
      if phase.id == "setup"
      {
         return setupDurationUs ?? fallbackSetupDurationUs
      }
      if phase.id == "prewarm", let warmupDurationUs
      {
         return warmupDurationUs
      }
      if phase.measured, let durationUs = measuredDurationUs[phase.id]
      {
         return durationUs
      }
      return sourceDurationUs
   }

   func traceEvents(phase: BenchmarkPhase, source: [BenchmarkTraceEvent], sourceDurationUs: UInt64, targetDurationUs: UInt64) throws -> [BenchmarkTraceEvent]
   {
      guard let iterations, iterations.phaseID == phase.id else {return source}
      guard !source.isEmpty,
            sourceDurationUs > 0,
            source.count % Int(iterations.sourceCount) == 0 else
      {
         throw BenchmarkCampaignFailure.invalidPlan
      }
      let eventsPerIteration = source.count / Int(iterations.sourceCount)
      let sourceStrideUs = sourceDurationUs / UInt64(iterations.sourceCount)
      let targetStrideUs = targetDurationUs / UInt64(iterations.count)
      guard eventsPerIteration > 0, sourceStrideUs > 0, targetStrideUs > 0 else
      {
         throw BenchmarkCampaignFailure.invalidPlan
      }
      var expanded = [BenchmarkTraceEvent]()
      expanded.reserveCapacity(eventsPerIteration * Int(iterations.count))
      for index in 0..<Int(iterations.count)
      {
         let sourceIndex = index % Int(iterations.sourceCount)
         let sourceStartUs = UInt64(sourceIndex) * sourceStrideUs
         let targetStartUs = UInt64(index) * targetStrideUs
         for event in source[(sourceIndex * eventsPerIteration)..<((sourceIndex + 1) * eventsPerIteration)]
         {
            guard event.atUs >= sourceStartUs else
            {
               throw BenchmarkCampaignFailure.invalidPlan
            }
            let relativeUs = event.atUs - sourceStartUs
            let scaledUs = relativeUs * targetStrideUs / sourceStrideUs
            expanded.append(event.replacingTimestamp(targetStartUs + scaledUs))
         }
      }
      return expanded
   }
}

private func benchmarkSecondsToMicroseconds(_ seconds: UInt64) throws -> UInt64
{
   let (microseconds, overflow) = seconds.multipliedReportingOverflow(by: 1_000_000)
   guard !overflow else {throw BenchmarkCampaignFailure.invalidPlan}
   return microseconds
}

private func benchmarkDistributeMeasuredDuration(_ measured: [(phase: BenchmarkPhase, durationUs: UInt64)], targetUs: UInt64) throws -> [String: UInt64]
{
   let sourceTotal = measured.reduce(UInt64(0)) {$0 + $1.durationUs}
   guard sourceTotal > 0, targetUs >= sourceTotal else
   {
      throw BenchmarkCampaignFailure.invalidPlan
   }
   var assigned = UInt64(0)
   var durations = [String: UInt64]()
   for (index, entry) in measured.enumerated()
   {
      let duration = index + 1 == measured.count ? targetUs - assigned : entry.durationUs * targetUs / sourceTotal
      guard duration >= entry.durationUs else
      {
         throw BenchmarkCampaignFailure.invalidPlan
      }
      durations[entry.phase.id] = duration
      assigned += duration
   }
   return durations
}

private extension BenchmarkTraceEvent
{
   func replacingTimestamp(_ timestamp: UInt64) -> BenchmarkTraceEvent
   {
      BenchmarkTraceEvent(
         atUs: timestamp,
         op: op,
         pointer: pointer,
         xMillionths: xMillionths,
         yMillionths: yMillionths,
         deltaXMillionths: deltaXMillionths,
         deltaYMillionths: deltaYMillionths,
         target: target,
         value: value,
         stateId: stateId
      )
   }
}
