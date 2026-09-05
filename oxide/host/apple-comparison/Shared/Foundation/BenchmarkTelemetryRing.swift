import Darwin
import CryptoKit
import Foundation

enum BenchmarkEventKind: UInt16, CaseIterable
{
   case scenarioBegin = 1
   case scenarioEnd
   case phaseBegin
   case phaseEnd
   case checkpointBegin
   case checkpointEnd
   case inputReceived
   case mutationBegin
   case mutationEnd
   case layoutBegin
   case layoutEnd
   case sceneUpdateBegin
   case sceneUpdateEnd
   case renderPrepareBegin
   case renderPrepareEnd
   case encodeBegin
   case encodeEnd
   case commandSubmit
   case gpuStart
   case gpuEnd
   case presentation
   case firstMeaningfulFrame
   case readyToInput
   case displayOpportunity
   case resetComplete
   case callbackCadence
   case drawableWait
   case inflightDepth
   case updateBacklog
   case logicalUpdateCompleted
   case logicalUpdateSkipped
   case quiescence
   case gpuDuration

   var name: String
   {
      switch self
      {
      case .scenarioBegin: return "scenarioBegin"
      case .scenarioEnd: return "scenarioEnd"
      case .phaseBegin: return "phaseBegin"
      case .phaseEnd: return "phaseEnd"
      case .checkpointBegin: return "checkpointBegin"
      case .checkpointEnd: return "checkpointEnd"
      case .inputReceived: return "inputReceived"
      case .mutationBegin: return "mutationBegin"
      case .mutationEnd: return "mutationEnd"
      case .layoutBegin: return "layoutBegin"
      case .layoutEnd: return "layoutEnd"
      case .sceneUpdateBegin: return "sceneUpdateBegin"
      case .sceneUpdateEnd: return "sceneUpdateEnd"
      case .renderPrepareBegin: return "renderPrepareBegin"
      case .renderPrepareEnd: return "renderPrepareEnd"
      case .encodeBegin: return "encodeBegin"
      case .encodeEnd: return "encodeEnd"
      case .commandSubmit: return "commandSubmit"
      case .gpuStart: return "gpuStart"
      case .gpuEnd: return "gpuEnd"
      case .presentation: return "presentation"
      case .firstMeaningfulFrame: return "firstMeaningfulFrame"
      case .readyToInput: return "readyToInput"
      case .displayOpportunity: return "displayOpportunity"
      case .resetComplete: return "resetComplete"
      case .callbackCadence: return "callbackCadence"
      case .drawableWait: return "drawableWait"
      case .inflightDepth: return "inflightDepth"
      case .updateBacklog: return "updateBacklog"
      case .logicalUpdateCompleted: return "logicalUpdateCompleted"
      case .logicalUpdateSkipped: return "logicalUpdateSkipped"
      case .quiescence: return "quiescence"
      case .gpuDuration: return "gpuDuration"
      }
   }
}

enum BenchmarkTelemetryAvailability: String, Codable
{
   case ringRequired = "ring-required"
   case ringConditional = "ring-conditional"
   case oxideDiagnostic = "oxide-diagnostic"
   case diagnosticNotEnabled = "diagnostic-not-enabled-for-pass"
   case externalEvidence = "external-evidence"
   case unavailable = "unavailable"
}

struct BenchmarkTelemetryCoverageEntry: Codable, Equatable
{
   let kind: String
   let rawValue: UInt16
   let availability: BenchmarkTelemetryAvailability
   let observedCount: UInt64
   let source: String
}

struct BenchmarkTelemetryCoverage: Codable, Equatable
{
   let schemaVersion: UInt32
   let side: ComparisonSide
   let passID: String
   let entries: [BenchmarkTelemetryCoverageEntry]
   let validation: String
}

struct BenchmarkTelemetryIdentity: Equatable
{
   let planSHA256: String
   let generation: String
   let chunkID: String
   let passID: String
   let sessionID: String
   let timebaseNumerator: UInt32
   let timebaseDenominator: UInt32
}

struct BenchmarkTelemetryRecord: Equatable
{
   let sequence: UInt64
   let timestamp: UInt64
   let kind: UInt16
   let flags: UInt16
   let identifier: UInt64
   let value0: Int64
   let value1: Int64
}

struct BenchmarkRendererDiagnostics: Equatable
{
   let submittedFrameID: UInt64
   let completedFrameID: UInt64
   let renderPrepareBeginTicks: UInt64
   let renderPrepareEndTicks: UInt64
   let encodeBeginTicks: UInt64
   let encodeEndTicks: UInt64
   let commandSubmitTicks: UInt64
   let drawableWaitBeginTicks: UInt64
   let drawableWaitEndTicks: UInt64
   let encodedBytes: UInt64
   let drawCalls: UInt64
   let damagePixels: UInt64
   let damageRects: UInt64
   let gpuDurationNs: UInt64
   let gpuRenderDurationNs: UInt64
}

protocol BenchmarkRendererDiagnosticsAdapter: AnyObject
{
   var rendererDiagnosticsEnabled: Bool {get}
   func rendererDiagnostics() throws -> BenchmarkRendererDiagnostics?
}

final class BenchmarkTelemetryRing
{
   static let binaryHeaderBytes = 136
   static let binaryRecordBytes = 44
   static let binaryFooterBytes = 32

   let capacity: Int
   private let records: UnsafeMutablePointer<BenchmarkTelemetryRecord>
   private(set) var count = 0
   private(set) var overflowed = false
   var lastTimestamp: UInt64? {count == 0 ? nil : records[count - 1].timestamp}

   init(capacity: Int)
   {
      precondition(capacity > 0)
      self.capacity = capacity
      records = .allocate(capacity: capacity)
      records.initialize(repeating: BenchmarkTelemetryRecord(
         sequence: 0,
         timestamp: 0,
         kind: 0,
         flags: 0,
         identifier: 0,
         value0: 0,
         value1: 0
      ), count: capacity)
   }

   deinit
   {
      records.deinitialize(count: capacity)
      records.deallocate()
   }

   @inline(__always)
   func append(kind: BenchmarkEventKind, identifier: UInt64 = 0, value0: Int64 = 0, value1: Int64 = 0, flags: UInt16 = 0, timestamp: UInt64 = mach_continuous_time())
   {
      guard count < capacity else
      {
         overflowed = true
         return
      }
      records[count] = BenchmarkTelemetryRecord(
         sequence: UInt64(count),
         timestamp: timestamp,
         kind: kind.rawValue,
         flags: flags,
         identifier: identifier,
         value0: value0,
         value1: value1
      )
      count += 1
   }

   func reset()
   {
      count = 0
      overflowed = false
   }

   func snapshot() -> [BenchmarkTelemetryRecord]
   {
      (0..<count).map {records[$0]}
   }

   func validateComplete() throws
   {
      guard !overflowed else
      {
         throw BenchmarkTelemetryFailure.overflow
      }
      for index in 0..<count
      {
         guard records[index].sequence == UInt64(index) else
         {
            throw BenchmarkTelemetryFailure.noncontiguousSequence
         }
         guard BenchmarkEventKind(rawValue: records[index].kind) != nil else
         {
            throw BenchmarkTelemetryFailure.unknownEventKind
         }
         if index > 0, records[index].timestamp < records[index - 1].timestamp
         {
            throw BenchmarkTelemetryFailure.nonmonotonicTimestamp
         }
      }
      try validateNesting()
   }

   func binarySnapshot(identity: BenchmarkTelemetryIdentity) throws -> Data
   {
      try validateComplete()
      guard let plan = Data(canonicalSHA256: identity.planSHA256),
            identity.timebaseNumerator > 0,
            identity.timebaseDenominator > 0 else
      {
         throw BenchmarkTelemetryFailure.invalidIdentity
      }
      let generation = Data(SHA256.hash(data: Data(identity.generation.utf8)))
      var data = Data()
      data.reserveCapacity(Self.binaryHeaderBytes + count * Self.binaryRecordBytes + Self.binaryFooterBytes)
      data.append(contentsOf: [0x4f, 0x58, 0x42, 0x54, 0x45, 0x4c, 0x30, 0x32])
      data.appendLittleEndian(UInt32(2))
      data.appendLittleEndian(UInt32(Self.binaryHeaderBytes))
      data.appendLittleEndian(UInt32(Self.binaryRecordBytes))
      data.appendLittleEndian(UInt32(1))
      data.appendLittleEndian(UInt64(count))
      data.appendLittleEndian(UInt64(capacity))
      data.append(plan)
      data.append(generation)
      data.appendLittleEndian(benchmarkStableID(identity.chunkID))
      data.appendLittleEndian(benchmarkStableID(identity.passID))
      data.appendLittleEndian(benchmarkStableID(identity.sessionID))
      data.appendLittleEndian(identity.timebaseNumerator)
      data.appendLittleEndian(identity.timebaseDenominator)
      for record in snapshot()
      {
         data.appendLittleEndian(record.sequence)
         data.appendLittleEndian(record.timestamp)
         data.appendLittleEndian(record.kind)
         data.appendLittleEndian(record.flags)
         data.appendLittleEndian(record.identifier)
         data.appendLittleEndian(record.value0)
         data.appendLittleEndian(record.value1)
      }
      data.append(Data(SHA256.hash(data: data)))
      return data
   }

   private func validateNesting() throws
   {
      var stack = [(begin: BenchmarkEventKind, identifier: UInt64)]()
      for index in 0..<count
      {
         let record = records[index]
         guard let kind = BenchmarkEventKind(rawValue: record.kind) else
         {
            throw BenchmarkTelemetryFailure.unknownEventKind
         }
         switch kind
         {
         case .scenarioBegin, .phaseBegin, .checkpointBegin:
            stack.append((kind, record.identifier))
         case .scenarioEnd:
            try popExpected(.scenarioBegin, identifier: record.identifier, stack: &stack)
         case .phaseEnd:
            try popExpected(.phaseBegin, identifier: record.identifier, stack: &stack)
         case .checkpointEnd:
            try popExpected(.checkpointBegin, identifier: record.identifier, stack: &stack)
         default: break
         }
      }
      guard stack.isEmpty else
      {
         throw BenchmarkTelemetryFailure.unterminatedScope
      }
   }

   private func popExpected(_ begin: BenchmarkEventKind, identifier: UInt64, stack: inout [(begin: BenchmarkEventKind, identifier: UInt64)]) throws
   {
      guard let open = stack.popLast(), open.begin == begin, open.identifier == identifier else
      {
         throw BenchmarkTelemetryFailure.invalidNesting
      }
   }
}

func benchmarkTelemetryCoverage(records: [BenchmarkTelemetryRecord], side: ComparisonSide, passID: String, rendererDiagnosticsEnabled: Bool) throws -> BenchmarkTelemetryCoverage
{
   var counts = [UInt16: UInt64]()
   for record in records
   {
      counts[record.kind, default: 0] += 1
   }
   let entries = BenchmarkEventKind.allCases.map
   {
      kind in
      let definition = benchmarkTelemetryDefinition(kind: kind, side: side, rendererDiagnosticsEnabled: rendererDiagnosticsEnabled)
      return BenchmarkTelemetryCoverageEntry(
         kind: kind.name,
         rawValue: kind.rawValue,
         availability: definition.availability,
         observedCount: counts[kind.rawValue, default: 0],
         source: definition.source
      )
   }
   try validateBenchmarkTelemetryCoverage(entries: entries, records: records, side: side, rendererDiagnosticsEnabled: rendererDiagnosticsEnabled)
   return BenchmarkTelemetryCoverage(
      schemaVersion: 1,
      side: side,
      passID: passID,
      entries: entries,
      validation: "complete-kind-availability-and-observed-counts"
   )
}

private func benchmarkTelemetryDefinition(kind: BenchmarkEventKind, side: ComparisonSide, rendererDiagnosticsEnabled: Bool) -> (availability: BenchmarkTelemetryAvailability, source: String)
{
   switch kind
   {
   case .scenarioBegin, .scenarioEnd, .phaseBegin, .phaseEnd, .checkpointBegin, .checkpointEnd, .readyToInput, .displayOpportunity, .callbackCadence:
      return (.ringRequired, "comparison-executor-observed-boundary")
   case .inputReceived, .mutationBegin, .mutationEnd, .sceneUpdateBegin, .sceneUpdateEnd, .resetComplete, .logicalUpdateCompleted, .quiescence:
      return (.ringConditional, "comparison-executor-observed-boundary")
   case .renderPrepareBegin, .renderPrepareEnd, .encodeBegin, .encodeEnd, .commandSubmit, .drawableWait, .gpuDuration:
      guard side == .oxide else
      {
         return (.unavailable, "appkit-framework-internal-stage-not-observable-symmetrically")
      }
      return rendererDiagnosticsEnabled
         ? (.oxideDiagnostic, "oxide-comparison-runtime-observed-boundary")
         : (.diagnosticNotEnabled, "oxide-diagnostic-pass-gate")
   case .layoutBegin, .layoutEnd:
      return (.unavailable, "framework-internal-layout-boundary-not-observable-symmetrically")
   case .gpuStart, .gpuEnd:
      return (.unavailable, "metal-duration-available-without-clock-aligned-gpu-boundary")
   case .presentation:
      return (.externalEvidence, "animation-hitches-frame-lifetime-correlation-not-display-callback")
   case .firstMeaningfulFrame:
      return (.externalEvidence, "canonical-launch-executor-evidence")
   case .inflightDepth:
      return (.unavailable, "renderer-inflight-depth-not-exposed-to-comparison-runtime")
   case .updateBacklog:
      return (.unavailable, "framework-update-backlog-not-observable-symmetrically")
   case .logicalUpdateSkipped:
      return (.unavailable, "trace-command-coalescing-is-not-a-renderer-logical-skip")
   }
}

private func validateBenchmarkTelemetryCoverage(entries: [BenchmarkTelemetryCoverageEntry], records: [BenchmarkTelemetryRecord], side: ComparisonSide, rendererDiagnosticsEnabled: Bool) throws
{
   guard entries.count == BenchmarkEventKind.allCases.count,
         Set(entries.map(\.rawValue)).count == entries.count,
         Set(entries.map(\.kind)).count == entries.count else
   {
      throw BenchmarkTelemetryFailure.invalidCoverage
   }
   for entry in entries
   {
      switch entry.availability
      {
      case .ringRequired:
         guard entry.observedCount > 0 else {throw BenchmarkTelemetryFailure.invalidCoverage}
      case .externalEvidence, .unavailable, .diagnosticNotEnabled:
         guard entry.observedCount == 0 else {throw BenchmarkTelemetryFailure.invalidCoverage}
      case .ringConditional, .oxideDiagnostic:
         break
      }
   }
   try validateBenchmarkTelemetryPair(records: records, begin: .scenarioBegin, end: .scenarioEnd)
   try validateBenchmarkTelemetryPair(records: records, begin: .phaseBegin, end: .phaseEnd)
   try validateBenchmarkTelemetryPair(records: records, begin: .checkpointBegin, end: .checkpointEnd)
   try validateBenchmarkTelemetryPair(records: records, begin: .mutationBegin, end: .mutationEnd)
   try validateBenchmarkTelemetryPair(records: records, begin: .sceneUpdateBegin, end: .sceneUpdateEnd)
   let sceneUpdates = records.filter {$0.kind == BenchmarkEventKind.sceneUpdateEnd.rawValue}.count
   let completedUpdates = records.filter {$0.kind == BenchmarkEventKind.logicalUpdateCompleted.rawValue}.count
   guard sceneUpdates == completedUpdates else {throw BenchmarkTelemetryFailure.invalidCoverage}
   if side == .oxide && rendererDiagnosticsEnabled
   {
      try validateBenchmarkTelemetryPair(records: records, begin: .renderPrepareBegin, end: .renderPrepareEnd)
      try validateBenchmarkTelemetryPair(records: records, begin: .encodeBegin, end: .encodeEnd)
      let encoded = records.filter {$0.kind == BenchmarkEventKind.encodeEnd.rawValue}
      let submitted = records.filter {$0.kind == BenchmarkEventKind.commandSubmit.rawValue}
      let waits = records.filter {$0.kind == BenchmarkEventKind.drawableWait.rawValue}
      guard !encoded.isEmpty,
            encoded.map(\.identifier) == submitted.map(\.identifier),
            encoded.map(\.identifier) == waits.map(\.identifier) else
      {
         throw BenchmarkTelemetryFailure.invalidCoverage
      }
   }
}

private func validateBenchmarkTelemetryPair(records: [BenchmarkTelemetryRecord], begin: BenchmarkEventKind, end: BenchmarkEventKind) throws
{
   let begins = records.filter {$0.kind == begin.rawValue}
   let ends = records.filter {$0.kind == end.rawValue}
   guard begins.map(\.identifier) == ends.map(\.identifier) else
   {
      throw BenchmarkTelemetryFailure.invalidCoverage
   }
}

enum BenchmarkTelemetryFailure: Error, Equatable
{
   case overflow
   case noncontiguousSequence
   case nonmonotonicTimestamp
   case unknownEventKind
   case invalidNesting
   case unterminatedScope
   case invalidIdentity
   case invalidCoverage
}

@inline(__always)
func benchmarkStableID(_ value: String) -> UInt64
{
   var hash: UInt64 = 0xcbf29ce484222325
   for byte in value.utf8
   {
      hash ^= UInt64(byte)
      hash &*= 0x100000001b3
   }
   return hash
}

private extension Data
{
   init?(canonicalSHA256 value: String)
   {
      guard value.count == 64 else {return nil}
      var decoded = Data()
      decoded.reserveCapacity(32)
      var index = value.startIndex
      for _ in 0..<32
      {
         let next = value.index(index, offsetBy: 2)
         guard let byte = UInt8(value[index..<next], radix: 16) else {return nil}
         decoded.append(byte)
         index = next
      }
      self = decoded
   }

   mutating func appendLittleEndian<T: FixedWidthInteger>(_ value: T)
   {
      var encoded = value.littleEndian
      Swift.withUnsafeBytes(of: &encoded) {append(contentsOf: $0)}
   }
}
