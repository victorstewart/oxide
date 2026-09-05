#if os(macOS)
import AppKit
import Darwin
import Dispatch
import Foundation

enum MacOSTrustedInputFailure: Error, Equatable
{
   case existingArtifact(String)
   case invalidCommand(String)
   case invalidReceipt(String)
   case invalidState(String)
   case unsupportedEvent(Int, String)
}

enum MacOSTrustedInputCommandKind: String, Codable, Equatable
{
   case click
   case wheel
   case text
   case selectReplace = "select-replace"
   // Frozen semantic substitutions backed by public XCUI mouse drags.
   case imagePinchOverride = "image-pinch-override"
   case navigationInteractiveCancelOverride = "navigation-interactive-cancel-override"
   case keyPair = "key-pair"
   case singlePointerDrag = "single-pointer-drag"
}

struct MacOSTrustedInputCommand: Codable, Equatable
{
   let kind: MacOSTrustedInputCommandKind
   let firstEventIndex: Int
   let lastEventIndex: Int
   let target: String?
   let pointer: UInt32?
   let startXMillionths: Int32?
   let startYMillionths: Int32?
   let endXMillionths: Int32?
   let endYMillionths: Int32?
   let deltaXMillionths: Int32?
   let deltaYMillionths: Int32?
   let durationUs: UInt64?
   let selectionStartUTF8: Int?
   let selectionEndUTF8: Int?
   let value: String?
}

enum MacOSTrustedInputEventDisposition: Equatable
{
   case command(Int)
   case absorbed(Int)
   case applicationStimulus
}

struct MacOSTrustedInputPlan: Equatable
{
   let commands: [MacOSTrustedInputCommand]
   let dispositions: [MacOSTrustedInputEventDisposition]

   func disposition(eventIndex: Int) throws -> MacOSTrustedInputEventDisposition
   {
      guard dispositions.indices.contains(eventIndex) else
      {
         throw MacOSTrustedInputFailure.invalidState("trace event index \(eventIndex)")
      }
      return dispositions[eventIndex]
   }
}

enum MacOSTrustedInputCompiler
{
   static func compile(_ events: [BenchmarkTraceEvent], allowNonClaimLifecycleStimuli: Bool = false) throws -> MacOSTrustedInputPlan
   {
      var commands = [MacOSTrustedInputCommand]()
      var dispositions = [MacOSTrustedInputEventDisposition?](repeating: nil, count: events.count)
      var index = 0
      while index < events.count
      {
         let event = events[index]
         switch event.op
         {
         case "pointer-down":
            let compiled: (command: MacOSTrustedInputCommand, nextIndex: Int)
            if index + 1 < events.count, events[index + 1].op == "pointer-down"
            {
               compiled = try imagePinchOverrideCommand(events, first: index)
            }
            else if index + 3 < events.count, events[index + 3].op == "pointer-cancel"
            {
               compiled = try navigationCancelOverrideCommand(events, first: index)
            }
            else
            {
               compiled = try pointerCommand(events, first: index)
            }
            append(compiled.command, through: compiled.nextIndex, commands: &commands, dispositions: &dispositions)
            index = compiled.nextIndex
         case "pointer-move", "pointer-up":
            throw MacOSTrustedInputFailure.unsupportedEvent(index, "unpaired \(event.op)")
         case "pointer-cancel":
            throw MacOSTrustedInputFailure.unsupportedEvent(index, "pointer-cancel requires a faithful public XCUI cancellation path")
         case "wheel":
            let target = try requiredTarget(event, index: index)
            let deltaX = event.deltaXMillionths ?? 0
            let deltaY = event.deltaYMillionths ?? 0
            guard deltaX != 0 || deltaY != 0 else
            {
               throw MacOSTrustedInputFailure.invalidCommand("wheel event \(index) has no delta")
            }
            append(command(
               kind: .wheel,
               first: index,
               last: index,
               target: target,
               deltaX: deltaX,
               deltaY: deltaY
            ), through: index + 1, commands: &commands, dispositions: &dispositions)
            index += 1
         case "commit-text":
            let target = try requiredTarget(event, index: index)
            guard target == "chat:composer" else
            {
               throw MacOSTrustedInputFailure.unsupportedEvent(index, "selected replacement has no adjacent exact focus event")
            }
            append(command(
               kind: .text,
               first: index,
               last: index,
               target: target,
               value: try textValue(event, index: index)
            ), through: index + 1, commands: &commands, dispositions: &dispositions)
            index += 1
         case "focus":
            if case .text(let value)? = event.value, value.contains(":")
            {
               let compiled = try selectReplaceCommand(events, first: index)
               append(compiled.command, through: compiled.nextIndex, commands: &commands, dispositions: &dispositions)
               index = compiled.nextIndex
               continue
            }
            append(command(
               kind: .click,
               first: index,
               last: index,
               target: try requiredTarget(event, index: index),
               startX: event.xMillionths,
               startY: event.yMillionths
            ), through: index + 1, commands: &commands, dispositions: &dispositions)
            index += 1
         case "navigate":
            let target = try requiredTarget(event, index: index)
            guard target != "navigation:cancel" else
            {
               throw MacOSTrustedInputFailure.unsupportedEvent(index, "interactive cancellation requires controller-owned Escape or capture loss")
            }
            append(command(
               kind: .click,
               first: index,
               last: index,
               target: target,
               startX: event.xMillionths,
               startY: event.yMillionths
            ), through: index + 1, commands: &commands, dispositions: &dispositions)
            index += 1
         case "mutate" where event.target?.hasSuffix(":favorite") == true:
            append(command(
               kind: .click,
               first: index,
               last: index,
               target: try requiredTarget(event, index: index),
               startX: event.xMillionths,
               startY: event.yMillionths
            ), through: index + 1, commands: &commands, dispositions: &dispositions)
            index += 1
         case "key-down":
            guard index + 1 < events.count, events[index + 1].op == "key-up" else
            {
               throw MacOSTrustedInputFailure.unsupportedEvent(index, "key-down has no adjacent key-up")
            }
            let keyUp = events[index + 1]
            guard keyUp.target == event.target, keyUp.value == event.value else
            {
               throw MacOSTrustedInputFailure.invalidCommand("key pair \(index) changes identity")
            }
            append(command(
               kind: .keyPair,
               first: index,
               last: index + 1,
               target: event.target,
               value: try textValue(event, index: index)
            ), through: index + 2, commands: &commands, dispositions: &dispositions)
            index += 2
         case "key-up":
            throw MacOSTrustedInputFailure.unsupportedEvent(index, "key-up has no adjacent key-down")
         case "ime-start", "ime-update", "ime-end":
            throw MacOSTrustedInputFailure.unsupportedEvent(index, "IME composition is not faithfully synthesizable by public XCUI")
         case "background", "foreground":
            guard allowNonClaimLifecycleStimuli else
            {
               throw MacOSTrustedInputFailure.unsupportedEvent(index, "lifecycle requires controller-owned activation or suspension receipts")
            }
            dispositions[index] = .applicationStimulus
            index += 1
         case "resize", "orientation", "theme", "scale", "mutate", "resource-arrival", "pressure":
            dispositions[index] = .applicationStimulus
            index += 1
         default:
            throw MacOSTrustedInputFailure.unsupportedEvent(index, event.op)
         }
      }
      return MacOSTrustedInputPlan(
         commands: commands,
         dispositions: try dispositions.enumerated().map
         {
            index, disposition in
            guard let disposition else
            {
               throw MacOSTrustedInputFailure.invalidState("unclassified trace event \(index)")
            }
            return disposition
         }
      )
   }

   private static func selectReplaceCommand(_ events: [BenchmarkTraceEvent], first: Int) throws -> (command: MacOSTrustedInputCommand, nextIndex: Int)
   {
      let focus = events[first]
      let target = try requiredTarget(focus, index: first)
      guard target == "chat:append:16",
            case .text("0:6")? = focus.value,
            first + 1 < events.count else
      {
         throw MacOSTrustedInputFailure.unsupportedEvent(first, "selection is not the frozen public-XCUI-safe chat:append:16 UTF-8 range 0:6")
      }
      let replacement = events[first + 1]
      guard replacement.op == "commit-text",
            replacement.target == target,
            case .text("Oxide")? = replacement.value else
      {
         throw MacOSTrustedInputFailure.unsupportedEvent(first, "selection is not followed by the frozen adjacent Oxide replacement")
      }
      return (command(
         kind: .selectReplace,
         first: first,
         last: first + 1,
         target: target,
         selectionStartUTF8: 0,
         selectionEndUTF8: 6,
         value: "Oxide"
      ), first + 2)
   }

   private static func imagePinchOverrideCommand(_ events: [BenchmarkTraceEvent], first: Int) throws -> (command: MacOSTrustedInputCommand, nextIndex: Int)
   {
      let count = 18
      guard first < events.count, events.count - first >= count else
      {
         throw MacOSTrustedInputFailure.unsupportedEvent(first, "frozen image pinch/multipointer override is truncated")
      }
      let baseUs = events[first].atUs
      for step in 0...8
      {
         let atUs = baseUs + UInt64(step) * 250_000
         let operation = step == 0 ? "pointer-down" : step == 8 ? "pointer-up" : "pointer-move"
         let firstX = Int32(375_000 - step * 31_250)
         let secondX = Int32(625_000 + step * 31_250)
         guard exactPointerEvent(events[first + step * 2], atUs: atUs, operation: operation, pointer: 1, x: firstX, y: 500_000),
               exactPointerEvent(events[first + step * 2 + 1], atUs: atUs, operation: operation, pointer: 2, x: secondX, y: 500_000) else
         {
            throw MacOSTrustedInputFailure.unsupportedEvent(first, "image pinch differs from the frozen public-XCUI override")
         }
      }
      return (command(
         kind: .imagePinchOverride,
         first: first,
         last: first + count - 1,
         target: "image.zoom",
         startX: 0,
         startY: 500_000,
         endX: 1_000_000,
         endY: 500_000,
         durationUs: 2_000_000
      ), first + count)
   }

   private static func navigationCancelOverrideCommand(_ events: [BenchmarkTraceEvent], first: Int) throws -> (command: MacOSTrustedInputCommand, nextIndex: Int)
   {
      let count = 5
      guard first < events.count, events.count - first >= count else
      {
         throw MacOSTrustedInputFailure.unsupportedEvent(first, "frozen navigation cancellation override is truncated")
      }
      let baseUs = events[first].atUs
      guard exactPointerEvent(events[first], atUs: baseUs, operation: "pointer-down", pointer: 1, x: 950_000, y: 500_000),
            exactPointerEvent(events[first + 1], atUs: baseUs + 250_000, operation: "pointer-move", pointer: 1, x: 750_000, y: 500_000),
            exactPointerEvent(events[first + 2], atUs: baseUs + 500_000, operation: "pointer-move", pointer: 1, x: 500_000, y: 500_000),
            exactPointerEvent(events[first + 3], atUs: baseUs + 750_000, operation: "pointer-cancel", pointer: 1, x: 500_000, y: 500_000) else
      {
         throw MacOSTrustedInputFailure.unsupportedEvent(first, "navigation cancellation differs from the frozen public-XCUI override")
      }
      let consequence = events[first + 4]
      guard consequence.atUs == baseUs + 1_000_000,
            consequence.op == "navigate",
            consequence.pointer == nil,
            consequence.xMillionths == nil,
            consequence.yMillionths == nil,
            consequence.deltaXMillionths == nil,
            consequence.deltaYMillionths == nil,
            consequence.target == "navigation:cancel",
            consequence.value == nil,
            consequence.stateId == "navigation:list-restored" else
      {
         throw MacOSTrustedInputFailure.unsupportedEvent(first, "navigation cancellation consequence differs from the frozen public-XCUI override")
      }
      return (command(
         kind: .navigationInteractiveCancelOverride,
         first: first,
         last: first + count - 1,
         target: "navigation.table",
         startX: 950_000,
         startY: 500_000,
         endX: 500_000,
         endY: 500_000,
         durationUs: 750_000
      ), first + count)
   }

   private static func exactPointerEvent(_ event: BenchmarkTraceEvent, atUs: UInt64, operation: String, pointer: UInt32, x: Int32, y: Int32) -> Bool
   {
      event.atUs == atUs
         && event.op == operation
         && event.pointer == pointer
         && event.xMillionths == x
         && event.yMillionths == y
         && event.deltaXMillionths == nil
         && event.deltaYMillionths == nil
         && event.target == nil
         && event.value == nil
         && event.stateId == nil
   }

   private static func pointerCommand(_ events: [BenchmarkTraceEvent], first: Int) throws -> (command: MacOSTrustedInputCommand, nextIndex: Int)
   {
      let down = events[first]
      guard let pointer = down.pointer,
            let startX = down.xMillionths,
            let startY = down.yMillionths else
      {
         throw MacOSTrustedInputFailure.invalidCommand("pointer-down \(first) is incomplete")
      }
      var moved = false
      var index = first + 1
      while index < events.count
      {
         let event = events[index]
         switch event.op
         {
         case "pointer-down":
            throw MacOSTrustedInputFailure.unsupportedEvent(first, "pinch/multipointer requires an explicit symmetric macOS override")
         case "pointer-cancel":
            throw MacOSTrustedInputFailure.unsupportedEvent(first, "pointer-cancel requires a faithful public XCUI cancellation path")
         case "pointer-move", "pointer-up":
            guard event.pointer == pointer else
            {
               throw MacOSTrustedInputFailure.invalidCommand("pointer sequence \(first) changes identity")
            }
            if event.op == "pointer-move"
            {
               moved = true
               index += 1
               continue
            }
            guard let endX = event.xMillionths,
                  let endY = event.yMillionths else
            {
               throw MacOSTrustedInputFailure.invalidCommand("pointer-up \(index) is incomplete")
            }
            var next = index + 1
            if !moved, next < events.count, events[next].op == "navigate"
            {
               guard events[next].target != "navigation:cancel" else
               {
                  throw MacOSTrustedInputFailure.unsupportedEvent(first, "interactive cancellation requires controller-owned Escape or capture loss")
               }
               next += 1
            }
            if !moved, startX == endX, startY == endY
            {
               guard let target = down.target ?? event.target, !target.isEmpty else
               {
                  throw MacOSTrustedInputFailure.invalidCommand("pointer click \(first) has no target")
               }
               return (command(
                  kind: .click,
                  first: first,
                  last: next - 1,
                  target: target,
                  startX: startX,
                  startY: startY
               ), next)
            }
            guard event.atUs > down.atUs else
            {
               throw MacOSTrustedInputFailure.invalidCommand("pointer drag \(first) has no positive duration")
            }
            return (command(
               kind: .singlePointerDrag,
               first: first,
               last: index,
               pointer: pointer,
               startX: startX,
               startY: startY,
               endX: endX,
               endY: endY,
               durationUs: event.atUs - down.atUs
            ), index + 1)
         default:
            throw MacOSTrustedInputFailure.unsupportedEvent(first, "pointer sequence interrupted by \(event.op)")
         }
      }
      throw MacOSTrustedInputFailure.unsupportedEvent(first, "pointer-down has no pointer-up")
   }

   private static func append(_ command: MacOSTrustedInputCommand, through nextIndex: Int, commands: inout [MacOSTrustedInputCommand], dispositions: inout [MacOSTrustedInputEventDisposition?])
   {
      let commandIndex = commands.count
      commands.append(command)
      for index in command.firstEventIndex..<nextIndex
      {
         dispositions[index] = index == command.firstEventIndex ? .command(commandIndex) : .absorbed(commandIndex)
      }
   }

   private static func command(kind: MacOSTrustedInputCommandKind, first: Int, last: Int, target: String? = nil, pointer: UInt32? = nil, startX: Int32? = nil, startY: Int32? = nil, endX: Int32? = nil, endY: Int32? = nil, deltaX: Int32? = nil, deltaY: Int32? = nil, durationUs: UInt64? = nil, selectionStartUTF8: Int? = nil, selectionEndUTF8: Int? = nil, value: String? = nil) -> MacOSTrustedInputCommand
   {
      MacOSTrustedInputCommand(
         kind: kind,
         firstEventIndex: first,
         lastEventIndex: last,
         target: target,
         pointer: pointer,
         startXMillionths: startX,
         startYMillionths: startY,
         endXMillionths: endX,
         endYMillionths: endY,
         deltaXMillionths: deltaX,
         deltaYMillionths: deltaY,
         durationUs: durationUs,
         selectionStartUTF8: selectionStartUTF8,
         selectionEndUTF8: selectionEndUTF8,
         value: value
      )
   }

   private static func requiredTarget(_ event: BenchmarkTraceEvent, index: Int) throws -> String
   {
      guard let target = event.target, !target.isEmpty else
      {
         throw MacOSTrustedInputFailure.invalidCommand("event \(index) has no target")
      }
      return target
   }

   private static func textValue(_ event: BenchmarkTraceEvent, index: Int) throws -> String
   {
      guard case .text(let value)? = event.value, !value.isEmpty else
      {
         throw MacOSTrustedInputFailure.invalidCommand("event \(index) has no text value")
      }
      return value
   }
}

struct MacOSTrustedInputIdentity: Codable, Equatable
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
   let commandSequence: UInt64
}

struct MacOSTrustedInputRequest: Codable, Equatable
{
   let identity: MacOSTrustedInputIdentity
   let command: MacOSTrustedInputCommand
   let descriptorPreparedTimestamp: UInt64
   let durable: Bool
}

struct MacOSTrustedInputControllerReceipt: Codable, Equatable
{
   let identity: MacOSTrustedInputIdentity
   let requestSHA256: String
   let dispatchPath: String
   let dispatchStartedTimestamp: UInt64
   let dispatchCompletedTimestamp: UInt64
   let applicationWasForeground: Bool
   let durable: Bool
   let complete: Bool
}

struct MacOSTrustedInputApplicationReceipt: Codable, Equatable
{
   let identity: MacOSTrustedInputIdentity
   let requestSHA256: String
   let controllerReceiptSHA256: String
   let firstEventIndex: Int
   let lastEventIndex: Int
   let rawEventFamilies: [String]
   let rawEventTypes: [UInt64]
   let rawEventTimestamp: Double
   let rawEventLocationX: Double
   let rawEventLocationY: Double
   let windowNumber: Int
   let applicationReceivedTimestamp: UInt64
   let applicationWasForeground: Bool
   let stateGenerationBefore: UInt64
   let stateGenerationAfter: UInt64
   let durable: Bool
   let complete: Bool
}

struct MacOSTrustedInputFlushEnvelope: Codable, Equatable
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
   let commandCount: UInt64
   let durable: Bool
}

struct MacOSTrustedInputApplicationObservation: Equatable
{
   let identity: MacOSTrustedInputIdentity
   let requestSHA256: String
   let firstEventIndex: Int
   let lastEventIndex: Int
   let rawEventFamilies: [String]
   let rawEventTypes: [UInt64]
   let rawEventTimestamp: Double
   let rawEventLocationX: Double
   let rawEventLocationY: Double
   let windowNumber: Int
   let applicationReceivedTimestamp: UInt64
   let applicationWasForeground: Bool
   let stateGenerationBefore: UInt64
   let stateGenerationAfter: UInt64
   let complete: Bool

   func durableReceipt(controllerReceiptSHA256: String) -> MacOSTrustedInputApplicationReceipt
   {
      MacOSTrustedInputApplicationReceipt(
         identity: identity,
         requestSHA256: requestSHA256,
         controllerReceiptSHA256: controllerReceiptSHA256,
         firstEventIndex: firstEventIndex,
         lastEventIndex: lastEventIndex,
         rawEventFamilies: rawEventFamilies,
         rawEventTypes: rawEventTypes,
         rawEventTimestamp: rawEventTimestamp,
         rawEventLocationX: rawEventLocationX,
         rawEventLocationY: rawEventLocationY,
         windowNumber: windowNumber,
         applicationReceivedTimestamp: applicationReceivedTimestamp,
         applicationWasForeground: applicationWasForeground,
         stateGenerationBefore: stateGenerationBefore,
         stateGenerationAfter: stateGenerationAfter,
         durable: true,
         complete: complete
      )
   }
}

struct MacOSTrustedInputValidatedReceipt
{
   let controller: MacOSTrustedInputControllerReceipt
   let application: MacOSTrustedInputApplicationObservation
}

final class MacOSTrustedInputApplicationEventObserver
{
   private struct Pending
   {
      let identity: MacOSTrustedInputIdentity
      let requestSHA256: String
      let command: MacOSTrustedInputCommand
      let stateGenerationBefore: UInt64
   }

   private struct Observed
   {
      let identity: MacOSTrustedInputIdentity
      let requestSHA256: String
      var rawEventFamilies: [String]
      var rawEventTypes: [UInt64]
      let rawEventTimestamp: Double
      let rawEventLocationX: Double
      let rawEventLocationY: Double
      let windowNumber: Int
      let applicationReceivedTimestamp: UInt64
      var applicationWasForeground: Bool
      let stateGenerationBefore: UInt64
   }

   private let lock = NSLock()
   private let stateGeneration: () -> UInt64
   private var pending: Pending?
   private var observed: Observed?

   init(stateGeneration: @escaping () -> UInt64)
   {
      self.stateGeneration = stateGeneration
   }

   func arm(identity: MacOSTrustedInputIdentity, requestSHA256: String, command: MacOSTrustedInputCommand) throws
   {
      let before = readStateGeneration()
      lock.lock()
      defer {lock.unlock()}
      guard pending == nil, observed == nil else
      {
         throw MacOSTrustedInputFailure.invalidState("application event observer already armed")
      }
      pending = Pending(identity: identity, requestSHA256: requestSHA256, command: command, stateGenerationBefore: before)
   }

   func observe(event: NSEvent, applicationWasForeground: Bool, dispatch: () -> Void)
   {
      lock.lock()
      let pending = self.pending
      lock.unlock()
      guard let pending, pending.command.kind.accepts(event.type) else
      {
         dispatch()
         return
      }
      let timestamp = mach_continuous_time()
      let location = event.locationInWindow
      dispatch()
      lock.lock()
      defer {lock.unlock()}
      guard self.pending?.identity == pending.identity else {return}
      let family = MacOSTrustedInputCommandKind.rawEventFamily(event.type)
      let type = UInt64(event.type.rawValue)
      if var observed
      {
         if !observed.rawEventFamilies.contains(family)
         {
            observed.rawEventFamilies.append(family)
            observed.rawEventTypes.append(type)
         }
         observed.applicationWasForeground = observed.applicationWasForeground && applicationWasForeground
         self.observed = observed
         return
      }
      observed = Observed(
         identity: pending.identity,
         requestSHA256: pending.requestSHA256,
         rawEventFamilies: [family],
         rawEventTypes: [type],
         rawEventTimestamp: event.timestamp,
         rawEventLocationX: Double(location.x),
         rawEventLocationY: Double(location.y),
         windowNumber: event.windowNumber,
         applicationReceivedTimestamp: timestamp,
         applicationWasForeground: applicationWasForeground,
         stateGenerationBefore: pending.stateGenerationBefore
      )
   }

   func finalize(identity: MacOSTrustedInputIdentity) throws -> MacOSTrustedInputApplicationObservation
   {
      let after = readStateGeneration()
      lock.lock()
      defer
      {
         pending = nil
         observed = nil
         lock.unlock()
      }
      guard let observed,
            let pending,
            observed.identity == identity,
            pending.identity == identity else
      {
         throw MacOSTrustedInputFailure.invalidReceipt("application event \(identity.commandSequence)")
      }
      return MacOSTrustedInputApplicationObservation(
         identity: observed.identity,
         requestSHA256: observed.requestSHA256,
         firstEventIndex: pending.command.firstEventIndex,
         lastEventIndex: pending.command.lastEventIndex,
         rawEventFamilies: observed.rawEventFamilies,
         rawEventTypes: observed.rawEventTypes,
         rawEventTimestamp: observed.rawEventTimestamp,
         rawEventLocationX: observed.rawEventLocationX,
         rawEventLocationY: observed.rawEventLocationY,
         windowNumber: observed.windowNumber,
         applicationReceivedTimestamp: observed.applicationReceivedTimestamp,
         applicationWasForeground: observed.applicationWasForeground,
         stateGenerationBefore: observed.stateGenerationBefore,
         stateGenerationAfter: after,
         complete: after > observed.stateGenerationBefore
            && pending.command.kind.accepts(families: observed.rawEventFamilies, types: observed.rawEventTypes)
      )
   }

   func cancel()
   {
      lock.lock()
      pending = nil
      observed = nil
      lock.unlock()
   }

   private func readStateGeneration() -> UInt64
   {
      if Thread.isMainThread {return stateGeneration()}
      return DispatchQueue.main.sync(execute: stateGeneration)
   }
}

final class MacOSTrustedInputWindow: NSWindow
{
   weak var trustedInputEventObserver: MacOSTrustedInputApplicationEventObserver?

   override func sendEvent(_ event: NSEvent)
   {
      guard let observer = trustedInputEventObserver else
      {
         super.sendEvent(event)
         return
      }
      observer.observe(event: event, applicationWasForeground: NSApp.isActive && isKeyWindow)
      {
         super.sendEvent(event)
      }
   }
}

final class MacOSTrustedInputTransaction
{
   private enum Phase: Equatable
   {
      case initialized
      case awaitingControllerReady
      case awaitingControllerReceipt
      case awaitingApplicationReceipt
      case complete
      case cancelled
   }

   let identity: MacOSTrustedInputIdentity
   let command: MacOSTrustedInputCommand
   private let lock = NSLock()
   private var phase = Phase.initialized

   init(identity: MacOSTrustedInputIdentity, command: MacOSTrustedInputCommand)
   {
      self.identity = identity
      self.command = command
   }

   func begin(preparedRequest: MacOSTrustedInputRequest) throws
   {
      lock.lock()
      defer {lock.unlock()}
      guard phase == .initialized,
            preparedRequest.identity == identity,
            preparedRequest.command == command,
            preparedRequest.descriptorPreparedTimestamp > 0,
            preparedRequest.durable else
      {
         throw MacOSTrustedInputFailure.invalidState("begin")
      }
      phase = .awaitingControllerReady
   }

   func controllerReadyObserved() throws
   {
      lock.lock()
      defer {lock.unlock()}
      guard phase == .awaitingControllerReady else
      {
         throw MacOSTrustedInputFailure.invalidState("controller ready")
      }
      phase = .awaitingControllerReceipt
   }

   func acceptController(receipt: MacOSTrustedInputControllerReceipt, requestSHA256: String) throws
   {
      lock.lock()
      defer {lock.unlock()}
      guard phase == .awaitingControllerReceipt else
      {
         throw MacOSTrustedInputFailure.invalidState("receipt")
      }
      guard receipt.identity == identity,
            receipt.requestSHA256 == requestSHA256,
            receipt.dispatchPath == command.kind.dispatchPath,
            receipt.dispatchStartedTimestamp > 0,
            receipt.dispatchCompletedTimestamp >= receipt.dispatchStartedTimestamp,
            receipt.applicationWasForeground,
            receipt.durable,
            receipt.complete else
      {
         throw MacOSTrustedInputFailure.invalidReceipt("command \(identity.commandSequence)")
      }
      phase = .awaitingApplicationReceipt
   }

   func complete(applicationObservation: MacOSTrustedInputApplicationObservation, requestSHA256: String) throws
   {
      lock.lock()
      defer {lock.unlock()}
      guard phase == .awaitingApplicationReceipt,
            applicationObservation.identity == identity,
            applicationObservation.requestSHA256 == requestSHA256,
            applicationObservation.firstEventIndex == command.firstEventIndex,
            applicationObservation.lastEventIndex == command.lastEventIndex,
            command.kind.accepts(families: applicationObservation.rawEventFamilies, types: applicationObservation.rawEventTypes),
            applicationObservation.rawEventTimestamp >= 0,
            applicationObservation.windowNumber > 0,
            applicationObservation.applicationReceivedTimestamp > 0,
            applicationObservation.applicationWasForeground,
            applicationObservation.stateGenerationAfter > applicationObservation.stateGenerationBefore,
            applicationObservation.complete else
      {
         throw MacOSTrustedInputFailure.invalidReceipt("application command \(identity.commandSequence)")
      }
      phase = .complete
   }

   func cancel()
   {
      lock.lock()
      phase = .cancelled
      lock.unlock()
   }
}

extension MacOSTrustedInputCommandKind
{
   var dispatchPath: String
   {
      switch self
      {
      case .click: return "XCUIElement.click"
      case .wheel: return "XCUIElement.scroll"
      case .text: return "XCUIElement.typeText"
      case .selectReplace: return "XCUIElement.click-typeKey-typeText"
      case .imagePinchOverride: return "XCUICoordinate.click-drag"
      case .navigationInteractiveCancelOverride: return "XCUICoordinate.click-drag-mouseUp-cancel"
      case .keyPair: return "XCUIElement.typeKey"
      case .singlePointerDrag: return "XCUICoordinate.click-drag"
      }
   }

   static func rawEventFamily(_ type: NSEvent.EventType) -> String
   {
      switch type
      {
      case .leftMouseDown, .leftMouseDragged: return "mouse"
      case .scrollWheel: return "scroll"
      case .keyDown: return "key"
      default: return "unsupported"
      }
   }

   func accepts(_ type: NSEvent.EventType) -> Bool
   {
      switch self
      {
      case .click: return type == .leftMouseDown
      case .wheel: return type == .scrollWheel
      case .text, .keyPair: return type == .keyDown
      case .selectReplace: return type == .leftMouseDown || type == .keyDown
      case .imagePinchOverride, .navigationInteractiveCancelOverride, .singlePointerDrag: return type == .leftMouseDragged
      }
   }

   func accepts(families: [String], types: [UInt64]) -> Bool
   {
      guard families.count == types.count else {return false}
      switch self
      {
      case .click: return families == ["mouse"] && types == [UInt64(NSEvent.EventType.leftMouseDown.rawValue)]
      case .wheel: return families == ["scroll"] && types == [UInt64(NSEvent.EventType.scrollWheel.rawValue)]
      case .text, .keyPair: return families == ["key"] && types == [UInt64(NSEvent.EventType.keyDown.rawValue)]
      case .selectReplace:
         return families == ["mouse", "key"]
            && types == [UInt64(NSEvent.EventType.leftMouseDown.rawValue), UInt64(NSEvent.EventType.keyDown.rawValue)]
      case .imagePinchOverride, .navigationInteractiveCancelOverride, .singlePointerDrag:
         return families == ["mouse"] && types == [UInt64(NSEvent.EventType.leftMouseDragged.rawValue)]
      }
   }
}

func macOSTrustedInputArtifactPath(identity: MacOSTrustedInputIdentity, suffix: String) -> String
{
   "Runs/\(identity.runID)/\(identity.chunkID)/\(identity.passID)/\(identity.packID)/\(identity.pairIndex)/\(identity.side.rawValue).trusted-input.\(identity.commandSequence).\(suffix)"
}

func macOSTrustedInputControllerReadyNotification(generation: String, sequence: UInt64) -> String
{
   "com.oxide.compare.input.controller-ready.g\(generation).c\(sequence)"
}

func macOSTrustedInputRequestNotification(generation: String, sequence: UInt64) -> String
{
   "com.oxide.compare.input.request.g\(generation).c\(sequence)"
}

func macOSTrustedInputReceiptNotification(generation: String, sequence: UInt64) -> String
{
   "com.oxide.compare.input.receipt.g\(generation).c\(sequence)"
}

func macOSTrustedInputApplicationReceiptNotification(generation: String, sequence: UInt64) -> String
{
   "com.oxide.compare.input.application-receipt.g\(generation).c\(sequence)"
}

func macOSTrustedInputDispatchReceiptNotification(generation: String) -> Notification.Name
{
   Notification.Name("com.oxide.compare.input.dispatch-receipt.g\(generation)")
}

func macOSTrustedInputFlushArtifactPath(runID: String, chunkID: String, passID: String, packID: String, pairIndex: UInt64, side: ComparisonSide, suffix: String) -> String
{
   "Runs/\(runID)/\(chunkID)/\(passID)/\(packID)/\(pairIndex)/\(side.rawValue).trusted-input.flush.\(suffix).json"
}

func macOSTrustedInputFlushControlFile(suffix: String) -> String
{
   "trusted-input.flush.\(suffix).json"
}

func macOSTrustedInputStopReceiptControlFile() -> String
{
   "trusted-input.stop.receipt.json"
}

private struct MacOSTrustedInputControllerDispatchReceipt
{
   let sequence: UInt64
   let dispatchStartedTimestamp: UInt64
   let dispatchCompletedTimestamp: UInt64
   let applicationWasForeground: Bool
}

private final class MacOSTrustedInputControllerReceiptMailbox
{
   private let center = DistributedNotificationCenter.default()
   private let lock = NSLock()
   private let semaphore = DispatchSemaphore(value: 0)
   private var observer: NSObjectProtocol?
   private var receipts = [UInt64: MacOSTrustedInputControllerDispatchReceipt]()

   init(generation: String)
   {
      observer = center.addObserver(forName: macOSTrustedInputDispatchReceiptNotification(generation: generation), object: nil, queue: nil)
      {
         [weak self] notification in
         self?.receive(notification)
      }
   }

   deinit
   {
      if let observer {center.removeObserver(observer)}
   }

   func wait(sequence: UInt64, timeout: DispatchTimeInterval) throws -> MacOSTrustedInputControllerDispatchReceipt
   {
      guard semaphore.wait(timeout: .now() + timeout) == .success else
      {
         throw MacOSTrustedInputFailure.invalidState("controller receipt timeout")
      }
      lock.lock()
      defer {lock.unlock()}
      guard let receipt = receipts.removeValue(forKey: sequence) else
      {
         throw MacOSTrustedInputFailure.invalidReceipt("controller dispatch sequence \(sequence)")
      }
      return receipt
   }

   private func receive(_ notification: Notification)
   {
      guard let sequence = (notification.userInfo?["sequence"] as? NSNumber)?.uint64Value,
            let started = (notification.userInfo?["dispatchStartedTimestamp"] as? NSNumber)?.uint64Value,
            let completed = (notification.userInfo?["dispatchCompletedTimestamp"] as? NSNumber)?.uint64Value,
            let foreground = (notification.userInfo?["applicationWasForeground"] as? NSNumber)?.boolValue else {return}
      let receipt = MacOSTrustedInputControllerDispatchReceipt(
         sequence: sequence,
         dispatchStartedTimestamp: started,
         dispatchCompletedTimestamp: completed,
         applicationWasForeground: foreground
      )
      lock.lock()
      guard receipts[sequence] == nil else
      {
         lock.unlock()
         return
      }
      receipts[sequence] = receipt
      lock.unlock()
      semaphore.signal()
   }
}

final class MacOSTrustedInputApplicationCoordinator
{
   private struct Prepared
   {
      let request: MacOSTrustedInputRequest
      let requestSHA256: String
   }

   private struct Completed
   {
      let prepared: Prepared
      let controller: MacOSTrustedInputControllerReceipt
      let application: MacOSTrustedInputApplicationObservation
   }

   private let invocation: BenchmarkCampaignInvocation
   private let store: DurableArtifactStore
   private let observer: MacOSTrustedInputApplicationEventObserver
   private let worker: DispatchQueue
   private let mailbox: MacOSTrustedInputControllerReceiptMailbox
   private var prepared = [UInt64: Prepared]()
   private var completed = [Completed]()
   private var preparationComplete = false
   private var current: MacOSTrustedInputTransaction?

   init(invocation: BenchmarkCampaignInvocation, store: DurableArtifactStore, observer: MacOSTrustedInputApplicationEventObserver, worker: DispatchQueue = DispatchQueue(label: "com.oxide.comparison.trusted-input", qos: .userInitiated))
   {
      self.invocation = invocation
      self.store = store
      self.observer = observer
      self.worker = worker
      mailbox = MacOSTrustedInputControllerReceiptMailbox(generation: invocation.request.generation)
   }

   func prepare(command: MacOSTrustedInputCommand, scenarioID: String, sequence: UInt64) throws
   {
      guard !preparationComplete, current == nil, prepared[sequence] == nil else
      {
         throw MacOSTrustedInputFailure.invalidState("trusted input preparation \(sequence)")
      }
      let identity = MacOSTrustedInputIdentity(
         schemaVersion: 1,
         runID: invocation.request.runID,
         planSHA256: invocation.request.planSHA256,
         chunkID: invocation.chunkID,
         passID: invocation.request.passID,
         packID: invocation.packID,
         pairIndex: invocation.request.pairIndex,
         side: invocation.request.side,
         generation: invocation.request.generation,
         scenarioID: scenarioID,
         commandSequence: sequence
      )
      let request = MacOSTrustedInputRequest(identity: identity, command: command, descriptorPreparedTimestamp: mach_continuous_time(), durable: true)
      let requestPath = macOSTrustedInputArtifactPath(identity: identity, suffix: "request.json")
      let controllerPath = macOSTrustedInputArtifactPath(identity: identity, suffix: "controller.json")
      let applicationPath = macOSTrustedInputArtifactPath(identity: identity, suffix: "application.json")
      for path in [requestPath, controllerPath, applicationPath]
      {
         if FileManager.default.fileExists(atPath: store.root.appendingPathComponent(path).path)
         {
            throw MacOSTrustedInputFailure.existingArtifact(path)
         }
      }
      let artifact = try store.durableJSON(request, relativePath: requestPath)
      prepared[sequence] = Prepared(request: request, requestSHA256: artifact.sha256)
   }

   func finishPreparation(commandCount: UInt64) throws
   {
      guard !preparationComplete,
            UInt64(prepared.count) == commandCount,
            (0..<commandCount).allSatisfy({prepared[$0] != nil}) else
      {
         throw MacOSTrustedInputFailure.invalidState("trusted input preparation is not contiguous")
      }
      preparationComplete = true
   }

   func submit(command: MacOSTrustedInputCommand, scenarioID: String, sequence: UInt64, completion: @escaping (Result<MacOSTrustedInputValidatedReceipt, Error>) -> Void) throws
   {
      guard preparationComplete,
            current == nil,
            let prepared = prepared[sequence],
            prepared.request.command == command,
            prepared.request.identity.scenarioID == scenarioID else
      {
         throw MacOSTrustedInputFailure.invalidState("unprepared or overlapping command \(sequence)")
      }
      let transaction = MacOSTrustedInputTransaction(identity: prepared.request.identity, command: command)
      current = transaction
      worker.async
      {
         [weak self, weak transaction] in
         guard let self, let transaction else {return}
         do
         {
            try transaction.begin(preparedRequest: prepared.request)
            try self.observer.arm(identity: transaction.identity, requestSHA256: prepared.requestSHA256, command: command)
            let ready = runWaitingForComparisonDarwinNotification(macOSTrustedInputControllerReadyNotification(generation: transaction.identity.generation, sequence: sequence), timeout: .seconds(30)) {}
            guard ready else {throw MacOSTrustedInputFailure.invalidState("controller ready timeout")}
            try transaction.controllerReadyObserved()
            postComparisonDarwinNotification(macOSTrustedInputRequestNotification(generation: transaction.identity.generation, sequence: sequence))
            let dispatch = try self.mailbox.wait(sequence: sequence, timeout: .seconds(30))
            let receipt = MacOSTrustedInputControllerReceipt(
               identity: transaction.identity,
               requestSHA256: prepared.requestSHA256,
               dispatchPath: command.kind.dispatchPath,
               dispatchStartedTimestamp: dispatch.dispatchStartedTimestamp,
               dispatchCompletedTimestamp: dispatch.dispatchCompletedTimestamp,
               applicationWasForeground: dispatch.applicationWasForeground,
               durable: true,
               complete: true
            )
            try transaction.acceptController(receipt: receipt, requestSHA256: prepared.requestSHA256)
            let application = try self.observer.finalize(identity: transaction.identity)
            try transaction.complete(applicationObservation: application, requestSHA256: prepared.requestSHA256)
            self.completed.append(Completed(prepared: prepared, controller: receipt, application: application))
            DispatchQueue.main.async
            {
               self.resolve(
                  transaction: transaction,
                  result: .success(MacOSTrustedInputValidatedReceipt(controller: receipt, application: application)),
                  completion: completion
               )
            }
         }
         catch
         {
            self.observer.cancel()
            DispatchQueue.main.async {self.resolve(transaction: transaction, result: .failure(error), completion: completion)}
         }
      }
   }

   func flush() throws
   {
      guard preparationComplete, current == nil else
      {
         throw MacOSTrustedInputFailure.invalidState("trusted input flush while active")
      }
      let completed = worker.sync {self.completed}
      guard completed.count == prepared.count else
      {
         throw MacOSTrustedInputFailure.invalidState("trusted input flush is incomplete")
      }
      let envelope = MacOSTrustedInputFlushEnvelope(
         schemaVersion: 1,
         runID: invocation.request.runID,
         planSHA256: invocation.request.planSHA256,
         chunkID: invocation.chunkID,
         passID: invocation.request.passID,
         packID: invocation.packID,
         pairIndex: invocation.request.pairIndex,
         side: invocation.request.side,
         generation: invocation.request.generation,
         commandCount: UInt64(prepared.count),
         durable: true
      )
      let requestPath = macOSTrustedInputFlushArtifactPath(
         runID: envelope.runID,
         chunkID: envelope.chunkID,
         passID: envelope.passID,
         packID: envelope.packID,
         pairIndex: envelope.pairIndex,
         side: envelope.side,
         suffix: "request"
      )
      let receiptPath = macOSTrustedInputFlushArtifactPath(
         runID: envelope.runID,
         chunkID: envelope.chunkID,
         passID: envelope.passID,
         packID: envelope.packID,
         pairIndex: envelope.pairIndex,
         side: envelope.side,
         suffix: "receipt"
      )
      guard !FileManager.default.fileExists(atPath: store.root.appendingPathComponent(requestPath).path),
            !FileManager.default.fileExists(atPath: store.root.appendingPathComponent(receiptPath).path) else
      {
         throw MacOSTrustedInputFailure.existingArtifact(requestPath)
      }
      guard let rootIndex = CommandLine.arguments.firstIndex(of: "-oxide-compare-controller-root"),
            rootIndex + 1 < CommandLine.arguments.count else
      {
         throw MacOSTrustedInputFailure.invalidState("trusted input controller root")
      }
      let controlStore = DurableArtifactStore(root: URL(fileURLWithPath: CommandLine.arguments[rootIndex + 1], isDirectory: true))
      let controlRequest = macOSTrustedInputFlushControlFile(suffix: "request")
      let controlReceipt = macOSTrustedInputFlushControlFile(suffix: "receipt")
      guard !FileManager.default.fileExists(atPath: controlStore.root.appendingPathComponent(controlRequest).path),
            !FileManager.default.fileExists(atPath: controlStore.root.appendingPathComponent(controlReceipt).path) else
      {
         throw MacOSTrustedInputFailure.existingArtifact(controlRequest)
      }
      _ = try controlStore.durableJSON(envelope, relativePath: controlRequest)
      let controlReceiptURL = controlStore.root.appendingPathComponent(controlReceipt)
      let deadline = Date().addingTimeInterval(30)
      while !FileManager.default.fileExists(atPath: controlReceiptURL.path) && Date() < deadline
      {
         Thread.sleep(forTimeInterval: 0.01)
      }
      guard FileManager.default.fileExists(atPath: controlReceiptURL.path) else
      {
         throw MacOSTrustedInputFailure.invalidState("trusted input controller flush timeout")
      }
      let receiptData = try Data(contentsOf: controlReceiptURL)
      let receipt = try JSONDecoder().decode(MacOSTrustedInputFlushEnvelope.self, from: receiptData)
      guard receipt == envelope, try JSONEncoder.comparisonCanonical.encode(receipt) == receiptData else
      {
         throw MacOSTrustedInputFailure.invalidState("trusted input controller flush receipt")
      }
      _ = try store.durableJSON(envelope, relativePath: requestPath)
      for item in completed
      {
         let controllerData = try JSONEncoder.comparisonCanonical.encode(item.controller)
         let controllerPath = macOSTrustedInputArtifactPath(identity: item.controller.identity, suffix: "controller.json")
         _ = try store.durableWrite(controllerData, to: store.root.appendingPathComponent(controllerPath))
         let application = item.application.durableReceipt(controllerReceiptSHA256: comparisonSHA256(controllerData))
         let path = macOSTrustedInputArtifactPath(identity: item.controller.identity, suffix: "application.json")
         _ = try store.durableJSON(application, relativePath: path)
         postComparisonDarwinNotification(macOSTrustedInputApplicationReceiptNotification(generation: item.controller.identity.generation, sequence: item.controller.identity.commandSequence))
      }
      _ = try store.durableJSON(envelope, relativePath: receiptPath)
   }

   func cancel()
   {
      current?.cancel()
      observer.cancel()
      current = nil
   }

   private func resolve(transaction: MacOSTrustedInputTransaction, result: Result<MacOSTrustedInputValidatedReceipt, Error>, completion: @escaping (Result<MacOSTrustedInputValidatedReceipt, Error>) -> Void)
   {
      guard current === transaction else {return}
      current = nil
      completion(result)
   }

}
#endif
