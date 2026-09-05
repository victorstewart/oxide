import CoreGraphics
import Darwin
import Foundation
import XCTest

struct MacOSTrustedInputControllerExpectation
{
   let runID: String
   let planSHA256: String
   let chunkID: String
   let passID: String
   let packID: String
   let pairIndex: UInt64
   let side: ComparisonSide
   let generation: String
}

final class MacOSTrustedInputXCUIController
{
   private struct PreloadedRequest
   {
      let request: MacOSTrustedInputRequest
      let sha256: String
   }

   private let application: XCUIApplication
   private let outputRoot: URL
   private let expectation: MacOSTrustedInputControllerExpectation
   private let controlStore: DurableArtifactStore
   private var requests = [PreloadedRequest]()
   private var receipts = [MacOSTrustedInputControllerReceipt]()
   private var flushed = false

   init(application: XCUIApplication, outputRoot: URL, controlRoot: URL, expectation: MacOSTrustedInputControllerExpectation) throws
   {
      self.application = application
      self.outputRoot = outputRoot
      self.expectation = expectation
      controlStore = DurableArtifactStore(root: controlRoot)
      requests = try preloadRequests()
   }

   func serviceUntilApplicationExits(timeout: TimeInterval) throws
   {
      let deadline = Date().addingTimeInterval(timeout)
      try serviceUntilFlush(deadline: deadline)
      try waitForStopReceipt(timeout: 15)
      guard receipts.count == requests.count else
      {
         throw MacOSTrustedInputFailure.invalidState("comparison application stopped before trusted input receipts were flushed")
      }
   }

   private func serviceUntilFlush(deadline: Date) throws
   {
      var sequence = UInt64(0)
      while Date() < deadline
      {
         if try consumeFlushRequest()
         {
            try flushReceipts()
            return
         }
         let received = runWaitingForComparisonDarwinNotification(
            macOSTrustedInputRequestNotification(generation: expectation.generation, sequence: sequence),
            timeout: .milliseconds(100)
         )
         {
            postComparisonDarwinNotification(macOSTrustedInputControllerReadyNotification(generation: expectation.generation, sequence: sequence))
         }
         if !received {continue}
         try service(sequence: sequence)
         sequence += 1
      }
      throw MacOSTrustedInputFailure.invalidState("comparison application did not request trusted input flush before the controller deadline")
   }

   private func waitForStopReceipt(timeout: TimeInterval) throws
   {
      let url = controlStore.root.appendingPathComponent(macOSTrustedInputStopReceiptControlFile())
      let deadline = Date().addingTimeInterval(timeout)
      while !FileManager.default.fileExists(atPath: url.path) && Date() < deadline
      {
         Thread.sleep(forTimeInterval: 0.01)
      }
      guard FileManager.default.fileExists(atPath: url.path) else
      {
         throw MacOSTrustedInputFailure.invalidState("comparison application did not durably acknowledge the controller stop signal")
      }
      let data = try Data(contentsOf: url)
      let envelope = try JSONDecoder().decode(MacOSTrustedInputFlushEnvelope.self, from: data)
      guard envelope.schemaVersion == 1,
            envelope.runID == expectation.runID,
            envelope.planSHA256 == expectation.planSHA256,
            envelope.chunkID == expectation.chunkID,
            envelope.passID == expectation.passID,
            envelope.packID == expectation.packID,
            envelope.pairIndex == expectation.pairIndex,
            envelope.side == expectation.side,
            envelope.generation == expectation.generation,
            envelope.commandCount == UInt64(requests.count),
            envelope.durable,
            try JSONEncoder.comparisonCanonical.encode(envelope) == data else
      {
         throw MacOSTrustedInputFailure.invalidState("comparison application stop receipt identity")
      }
   }

   private func service(sequence: UInt64) throws
   {
      guard sequence <= UInt64(Int.max), requests.indices.contains(Int(sequence)) else
      {
         throw MacOSTrustedInputFailure.invalidCommand("unprepared request sequence \(sequence)")
      }
      let prepared = requests[Int(sequence)]
      let request = prepared.request
      guard application.state == .runningForeground else
      {
         throw MacOSTrustedInputFailure.invalidState("trusted input target is not foreground")
      }
      let started = mach_continuous_time()
      try execute(request.command)
      let completed = mach_continuous_time()
      guard application.state == .runningForeground else
      {
         throw MacOSTrustedInputFailure.invalidState("trusted input target left the foreground during dispatch")
      }
      let receipt = MacOSTrustedInputControllerReceipt(
         identity: request.identity,
         requestSHA256: prepared.sha256,
         dispatchPath: request.command.kind.dispatchPath,
         dispatchStartedTimestamp: started,
         dispatchCompletedTimestamp: completed,
         applicationWasForeground: true,
         durable: true,
         complete: true
      )
      receipts.append(receipt)
      DistributedNotificationCenter.default().postNotificationName(
         macOSTrustedInputDispatchReceiptNotification(generation: expectation.generation),
         object: nil,
         userInfo: [
            "sequence": NSNumber(value: sequence),
            "dispatchStartedTimestamp": NSNumber(value: started),
            "dispatchCompletedTimestamp": NSNumber(value: completed),
            "applicationWasForeground": NSNumber(value: true),
         ],
         deliverImmediately: true
      )
   }

   private func preloadRequests() throws -> [PreloadedRequest]
   {
      let directory = outputRoot
         .appendingPathComponent("Runs", isDirectory: true)
         .appendingPathComponent(expectation.runID, isDirectory: true)
         .appendingPathComponent(expectation.chunkID, isDirectory: true)
         .appendingPathComponent(expectation.passID, isDirectory: true)
         .appendingPathComponent(expectation.packID, isDirectory: true)
         .appendingPathComponent(String(expectation.pairIndex), isDirectory: true)
      let prefix = "\(expectation.side.rawValue).trusted-input."
      let suffix = ".request.json"
      let names = try FileManager.default.contentsOfDirectory(atPath: directory.path)
         .filter {$0.hasPrefix(prefix) && $0.hasSuffix(suffix)}
      var indexed = [(UInt64, PreloadedRequest)]()
      for name in names
      {
         let sequenceText = String(name.dropFirst(prefix.count).dropLast(suffix.count))
         guard let sequence = UInt64(sequenceText) else
         {
            throw MacOSTrustedInputFailure.invalidCommand("request filename \(name)")
         }
         let data = try Data(contentsOf: directory.appendingPathComponent(name))
         let request = try JSONDecoder().decode(MacOSTrustedInputRequest.self, from: data)
         guard try JSONEncoder.comparisonCanonical.encode(request) == data else
         {
            throw MacOSTrustedInputFailure.invalidCommand("request bytes are not canonical")
         }
         try validate(request: request, expectedSequence: sequence)
         indexed.append((sequence, PreloadedRequest(request: request, sha256: comparisonSHA256(data))))
      }
      indexed.sort {$0.0 < $1.0}
      guard indexed.enumerated().allSatisfy({UInt64($0.offset) == $0.element.0}) else
      {
         throw MacOSTrustedInputFailure.invalidCommand("request sequence is not contiguous")
      }
      return indexed.map(\.1)
   }

   private func flushReceipts() throws
   {
      guard !flushed, receipts.count == requests.count else
      {
         throw MacOSTrustedInputFailure.invalidState("trusted input controller flush is incomplete or repeated")
      }
      let envelope = MacOSTrustedInputFlushEnvelope(
         schemaVersion: 1,
         runID: expectation.runID,
         planSHA256: expectation.planSHA256,
         chunkID: expectation.chunkID,
         passID: expectation.passID,
         packID: expectation.packID,
         pairIndex: expectation.pairIndex,
         side: expectation.side,
         generation: expectation.generation,
         commandCount: UInt64(requests.count),
         durable: true
      )
      _ = try controlStore.durableJSON(envelope, relativePath: macOSTrustedInputFlushControlFile(suffix: "receipt"))
      flushed = true
   }

   private func consumeFlushRequest() throws -> Bool
   {
      guard !flushed else {return false}
      let url = controlStore.root.appendingPathComponent(macOSTrustedInputFlushControlFile(suffix: "request"))
      guard FileManager.default.fileExists(atPath: url.path) else {return false}
      let data = try Data(contentsOf: url)
      let envelope = try JSONDecoder().decode(MacOSTrustedInputFlushEnvelope.self, from: data)
      guard envelope.schemaVersion == 1,
            envelope.runID == expectation.runID,
            envelope.planSHA256 == expectation.planSHA256,
            envelope.chunkID == expectation.chunkID,
            envelope.passID == expectation.passID,
            envelope.packID == expectation.packID,
            envelope.pairIndex == expectation.pairIndex,
            envelope.side == expectation.side,
            envelope.generation == expectation.generation,
            envelope.commandCount == UInt64(requests.count),
            envelope.durable,
            try JSONEncoder.comparisonCanonical.encode(envelope) == data else
      {
         throw MacOSTrustedInputFailure.invalidState("trusted input flush request")
      }
      return true
   }

   private func validate(request: MacOSTrustedInputRequest, expectedSequence: UInt64) throws
   {
      let identity = request.identity
      guard identity.schemaVersion == 1,
            identity.runID == expectation.runID,
            identity.planSHA256 == expectation.planSHA256,
            identity.chunkID == expectation.chunkID,
            identity.passID == expectation.passID,
            identity.packID == expectation.packID,
            identity.pairIndex == expectation.pairIndex,
            identity.side == expectation.side,
            identity.generation == expectation.generation,
            !identity.scenarioID.isEmpty,
            identity.commandSequence == expectedSequence,
            request.descriptorPreparedTimestamp > 0,
            request.durable else
      {
         throw MacOSTrustedInputFailure.invalidCommand("request identity")
      }
      let command = request.command
      guard command.firstEventIndex >= 0,
            command.lastEventIndex >= command.firstEventIndex else
      {
         throw MacOSTrustedInputFailure.invalidCommand("event range")
      }
      switch command.kind
      {
      case .click:
         guard command.target?.isEmpty == false else {throw MacOSTrustedInputFailure.invalidCommand("click target")}
         try validateCoordinatePair(command.startXMillionths, command.startYMillionths, optional: true)
      case .wheel:
         guard command.target?.isEmpty == false,
               (command.deltaXMillionths ?? 0) != 0 || (command.deltaYMillionths ?? 0) != 0 else
         {
            throw MacOSTrustedInputFailure.invalidCommand("wheel")
         }
      case .text, .keyPair:
         guard command.value?.isEmpty == false else {throw MacOSTrustedInputFailure.invalidCommand(command.kind.rawValue)}
      case .selectReplace:
         guard command.target == "chat:append:16",
               command.lastEventIndex == command.firstEventIndex + 1,
               command.selectionStartUTF8 == 0,
               command.selectionEndUTF8 == 6,
               command.value == "Oxide" else
         {
            throw MacOSTrustedInputFailure.invalidCommand("select-replace")
         }
      case .imagePinchOverride:
         guard command.target == "image.zoom",
               command.lastEventIndex == command.firstEventIndex + 17,
               command.pointer == nil,
               command.startXMillionths == 0,
               command.startYMillionths == 500_000,
               command.endXMillionths == 1_000_000,
               command.endYMillionths == 500_000,
               command.durationUs == 2_000_000 else
         {
            throw MacOSTrustedInputFailure.invalidCommand("image-pinch-override")
         }
      case .navigationInteractiveCancelOverride:
         guard command.target == "navigation.table",
               command.lastEventIndex == command.firstEventIndex + 4,
               command.pointer == nil,
               command.startXMillionths == 950_000,
               command.startYMillionths == 500_000,
               command.endXMillionths == 500_000,
               command.endYMillionths == 500_000,
               command.durationUs == 750_000 else
         {
            throw MacOSTrustedInputFailure.invalidCommand("navigation-interactive-cancel-override")
         }
      case .singlePointerDrag:
         try validateCoordinatePair(command.startXMillionths, command.startYMillionths, optional: false)
         try validateCoordinatePair(command.endXMillionths, command.endYMillionths, optional: false)
         guard command.pointer != nil, (command.durationUs ?? 0) > 0 else
         {
            throw MacOSTrustedInputFailure.invalidCommand("single-pointer drag")
         }
      }
   }

   private func validateCoordinatePair(_ x: Int32?, _ y: Int32?, optional: Bool) throws
   {
      if optional, x == nil, y == nil {return}
      guard let x, let y, (0...1_000_000).contains(x), (0...1_000_000).contains(y) else
      {
         throw MacOSTrustedInputFailure.invalidCommand("normalized coordinate")
      }
   }

   private func execute(_ command: MacOSTrustedInputCommand) throws
   {
      switch command.kind
      {
      case .click:
         if let x = command.startXMillionths, let y = command.startYMillionths
         {
            coordinate(x: x, y: y).click()
         }
         else
         {
            try element(identifier: command.target).click()
         }
      case .wheel:
         let target = try element(identifier: command.target)
         target.scroll(
            byDeltaX: CGFloat(command.deltaXMillionths ?? 0) * 390 / 1_000_000,
            deltaY: CGFloat(command.deltaYMillionths ?? 0) * 844 / 1_000_000
         )
      case .text:
         let target = try element(identifier: command.target)
         target.click()
         target.typeText(command.value ?? "")
      case .selectReplace:
         let target = try element(identifier: command.target)
         target.click()
         target.typeKey(.home, modifierFlags: [])
         for _ in 0..<6
         {
            target.typeKey(.rightArrow, modifierFlags: .shift)
         }
         target.typeText("Oxide")
      case .imagePinchOverride:
         let target = try element(identifier: command.target)
         let start = target.coordinate(withNormalizedOffset: CGVector(dx: CGFloat(command.startXMillionths ?? 0) / 1_000_000, dy: CGFloat(command.startYMillionths ?? 0) / 1_000_000))
         let end = target.coordinate(withNormalizedOffset: CGVector(dx: CGFloat(command.endXMillionths ?? 0) / 1_000_000, dy: CGFloat(command.endYMillionths ?? 0) / 1_000_000))
         let duration = TimeInterval(command.durationUs ?? 0) / 1_000_000
         start.click(forDuration: duration, thenDragTo: end)
      case .navigationInteractiveCancelOverride:
         let start = coordinate(x: command.startXMillionths ?? 0, y: command.startYMillionths ?? 0)
         let end = coordinate(x: command.endXMillionths ?? 0, y: command.endYMillionths ?? 0)
         let duration = TimeInterval(command.durationUs ?? 0) / 1_000_000
         start.click(forDuration: duration, thenDragTo: end)
      case .keyPair:
         let target: XCUIElement
         if let identifier = command.target
         {
            target = try element(identifier: identifier)
         }
         else
         {
            target = application
         }
         target.typeKey(XCUIKeyboardKey(rawValue: command.value ?? ""), modifierFlags: [])
      case .singlePointerDrag:
         let start = coordinate(x: command.startXMillionths ?? 0, y: command.startYMillionths ?? 0)
         let end = coordinate(x: command.endXMillionths ?? 0, y: command.endYMillionths ?? 0)
         let duration = TimeInterval(command.durationUs ?? 0) / 1_000_000
         start.click(forDuration: duration, thenDragTo: end)
      }
   }

   private func element(identifier: String?) throws -> XCUIElement
   {
      guard let identifier, !identifier.isEmpty else
      {
         throw MacOSTrustedInputFailure.invalidCommand("missing XCUI identifier")
      }
      let element = application.descendants(matching: .any).matching(identifier: xcuiIdentifier(identifier)).firstMatch
      guard element.waitForExistence(timeout: 2) else
      {
         throw MacOSTrustedInputFailure.invalidState("XCUI target is unavailable: \(identifier)")
      }
      return element
   }

   private func xcuiIdentifier(_ traceTarget: String) -> String
   {
      switch traceTarget
      {
      case "chat:composer": return "chat.composer"
      case "grid:collection": return "grid.collection"
      case "navigation:modal": return "navigation.modal-action"
      case "navigation:dismiss-control": return "navigation.dismiss"
      case "navigation:back-control": return "navigation.back"
      default: return traceTarget
      }
   }

   private func coordinate(x: Int32, y: Int32) -> XCUICoordinate
   {
      application.coordinate(withNormalizedOffset: CGVector(dx: CGFloat(x) / 1_000_000, dy: CGFloat(y) / 1_000_000))
   }
}
