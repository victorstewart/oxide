import AppKit
import XCTest

final class MacOSTrustedInputControlTests: XCTestCase
{
   func testCompilerSeparatesCommandsAbsorbedEventsAndApplicationStimuli() throws
   {
      let events = [
         pointer(atUs: 0, op: "pointer-down", x: 500_000, y: 500_000, target: "grid:tile:07500"),
         pointer(atUs: 50_000, op: "pointer-up", x: 500_000, y: 500_000, target: "grid:tile:07500"),
         event(atUs: 100_000, op: "navigate", target: "grid:detail:07500"),
         event(atUs: 200_000, op: "wheel", target: "grid:collection", deltaY: 125_000),
         event(atUs: 300_000, op: "resource-arrival", target: "image:texture"),
      ]
      let plan = try MacOSTrustedInputCompiler.compile(events)
      XCTAssertEqual(plan.commands.map(\.kind), [.click, .wheel])
      XCTAssertEqual(plan.dispositions, [.command(0), .absorbed(0), .absorbed(0), .command(1), .applicationStimulus])
   }

   func testCompilerBindsTheFrozenChatSelectionAndReplacementIntoOneCommand() throws
   {
      let events = [
         text(atUs: 0, op: "focus", target: "chat:append:16", value: "0:6"),
         text(atUs: 500_000, op: "commit-text", target: "chat:append:16", value: "Oxide"),
      ]
      let plan = try MacOSTrustedInputCompiler.compile(events)
      XCTAssertEqual(plan.dispositions, [.command(0), .absorbed(0)])
      let command = try XCTUnwrap(plan.commands.first)
      XCTAssertEqual(command.kind, .selectReplace)
      XCTAssertEqual(command.firstEventIndex, 0)
      XCTAssertEqual(command.lastEventIndex, 1)
      XCTAssertEqual(command.target, "chat:append:16")
      XCTAssertEqual(command.selectionStartUTF8, 0)
      XCTAssertEqual(command.selectionEndUTF8, 6)
      XCTAssertEqual(command.value, "Oxide")
   }

   func testCompilerRejectsSelectionRangesWithoutAnExactPublicXCUISequence()
   {
      for events in [
         [text(atUs: 0, op: "focus", target: "chat:append:16", value: "1:6"), text(atUs: 1, op: "commit-text", target: "chat:append:16", value: "Oxide")],
         [text(atUs: 0, op: "focus", target: "chat:append:16", value: "0:7"), text(atUs: 1, op: "commit-text", target: "chat:append:16", value: "Oxide")],
         [text(atUs: 0, op: "focus", target: "chat:message:4096", value: "0:6"), text(atUs: 1, op: "commit-text", target: "chat:message:4096", value: "Oxide")],
         [text(atUs: 0, op: "focus", target: "chat:append:16", value: "0:6"), text(atUs: 1, op: "commit-text", target: "chat:append:16", value: "Other")],
      ]
      {
         XCTAssertThrowsError(try MacOSTrustedInputCompiler.compile(events))
      }
   }

   func testCompilerBindsFrozenImageAndNavigationOverridesToFullTraceRanges() throws
   {
      let image = try MacOSTrustedInputCompiler.compile(currentTrace("image-pinch.json"))
      let imageCommand = try XCTUnwrap(image.commands.first)
      XCTAssertEqual(image.commands.count, 1)
      XCTAssertEqual(imageCommand.kind, .imagePinchOverride)
      XCTAssertEqual(imageCommand.firstEventIndex, 0)
      XCTAssertEqual(imageCommand.lastEventIndex, 17)
      XCTAssertEqual(imageCommand.target, "image.zoom")
      XCTAssertEqual(imageCommand.startXMillionths, 0)
      XCTAssertEqual(imageCommand.endXMillionths, 1_000_000)
      XCTAssertEqual(imageCommand.durationUs, 2_000_000)
      XCTAssertEqual(image.dispositions.first, .command(0))
      XCTAssertTrue(image.dispositions.dropFirst().allSatisfy {$0 == .absorbed(0)})

      let navigation = try MacOSTrustedInputCompiler.compile(currentTrace("navigation-interactive-cancel.json"))
      let navigationCommand = try XCTUnwrap(navigation.commands.first)
      XCTAssertEqual(navigation.commands.count, 1)
      XCTAssertEqual(navigationCommand.kind, .navigationInteractiveCancelOverride)
      XCTAssertEqual(navigationCommand.firstEventIndex, 0)
      XCTAssertEqual(navigationCommand.lastEventIndex, 4)
      XCTAssertEqual(navigationCommand.target, "navigation.table")
      XCTAssertEqual(navigationCommand.startXMillionths, 950_000)
      XCTAssertEqual(navigationCommand.endXMillionths, 500_000)
      XCTAssertEqual(navigationCommand.durationUs, 750_000)
      XCTAssertEqual(navigation.dispositions.first, .command(0))
      XCTAssertTrue(navigation.dispositions.dropFirst().allSatisfy {$0 == .absorbed(0)})
   }

   func testCompilerRejectsEveryFrozenOverrideTimestampMutation() throws
   {
      for name in ["image-pinch.json", "navigation-interactive-cancel.json"]
      {
         let source = try currentTrace(name)
         for index in source.indices
         {
            var events = source
            let event = events[index]
            events[index] = BenchmarkTraceEvent(
               atUs: event.atUs + 1,
               op: event.op,
               pointer: event.pointer,
               xMillionths: event.xMillionths,
               yMillionths: event.yMillionths,
               deltaXMillionths: event.deltaXMillionths,
               deltaYMillionths: event.deltaYMillionths,
               target: event.target,
               value: event.value,
               stateId: event.stateId
            )
            XCTAssertThrowsError(try MacOSTrustedInputCompiler.compile(events), "\(name) event \(index)")
         }
      }
   }

   func testCompilerRejectsUnreceiptedLifecycleAndUnsupportedInput() throws
   {
      for (events, reason) in [
         ([event(atUs: 0, op: "background", target: "application")], "lifecycle"),
         ([event(atUs: 0, op: "ime-start", target: "chat:composer")], "IME"),
         ([
            pointer(atUs: 0, op: "pointer-down", pointer: 1, x: 400_000, y: 500_000),
            pointer(atUs: 0, op: "pointer-down", pointer: 2, x: 600_000, y: 500_000),
         ], "pinch/multipointer"),
      ]
      {
         XCTAssertThrowsError(try MacOSTrustedInputCompiler.compile(events))
         {
            XCTAssertTrue(String(describing: $0).contains(reason))
         }
      }
   }

   func testCompilerAllowsLifecycleOnlyAsAnExplicitNonClaimStimulus() throws
   {
      let plan = try MacOSTrustedInputCompiler.compile(
         [event(atUs: 0, op: "foreground", target: "application:lifecycle")],
         allowNonClaimLifecycleStimuli: true
      )
      XCTAssertTrue(plan.commands.isEmpty)
      XCTAssertEqual(plan.dispositions, [.applicationStimulus])
   }

   func testTransactionRequiresExactDurableForegroundReceipt() throws
   {
      let identity = MacOSTrustedInputIdentity(
         schemaVersion: 1,
         runID: "run",
         planSHA256: String(repeating: "a", count: 64),
         chunkID: "chunk",
         passID: "minimal-presentation",
         packID: "pack",
         pairIndex: 0,
         side: .native,
         generation: String(repeating: "b", count: 64),
         scenarioID: "grid.large-scroll",
         commandSequence: 0
      )
      let command = try XCTUnwrap(MacOSTrustedInputCompiler.compile([
         pointer(atUs: 0, op: "pointer-down", x: 500_000, y: 500_000, target: "grid:tile:07500"),
         pointer(atUs: 50_000, op: "pointer-up", x: 500_000, y: 500_000, target: "grid:tile:07500"),
      ]).commands.first)
      let transaction = MacOSTrustedInputTransaction(identity: identity, command: command)
      try transaction.begin(preparedRequest: MacOSTrustedInputRequest(identity: identity, command: command, descriptorPreparedTimestamp: 1, durable: true))
      try transaction.controllerReadyObserved()
      try transaction.acceptController(
         receipt: MacOSTrustedInputControllerReceipt(
            identity: identity,
            requestSHA256: String(repeating: "c", count: 64),
            dispatchPath: "XCUIElement.click",
            dispatchStartedTimestamp: 2,
            dispatchCompletedTimestamp: 3,
            applicationWasForeground: true,
            durable: true,
            complete: true
         ),
         requestSHA256: String(repeating: "c", count: 64)
      )
      try transaction.complete(
         applicationObservation: MacOSTrustedInputApplicationObservation(
            identity: identity,
            requestSHA256: String(repeating: "c", count: 64),
            firstEventIndex: command.firstEventIndex,
            lastEventIndex: command.lastEventIndex,
            rawEventFamilies: ["mouse"],
            rawEventTypes: [UInt64(NSEvent.EventType.leftMouseDown.rawValue)],
            rawEventTimestamp: 1,
            rawEventLocationX: 195,
            rawEventLocationY: 422,
            windowNumber: 7,
            applicationReceivedTimestamp: 4,
            applicationWasForeground: true,
            stateGenerationBefore: 11,
            stateGenerationAfter: 12,
            complete: true
         ),
         requestSHA256: String(repeating: "c", count: 64)
      )
   }

   func testApplicationObserverRecordsRawEventAndExactStateTransition() throws
   {
      let identity = MacOSTrustedInputIdentity(
         schemaVersion: 1,
         runID: "run",
         planSHA256: String(repeating: "a", count: 64),
         chunkID: "chunk",
         passID: "minimal-presentation",
         packID: "pack",
         pairIndex: 0,
         side: .oxide,
         generation: String(repeating: "b", count: 64),
         scenarioID: "grid.large-scroll",
         commandSequence: 0
      )
      let command = try XCTUnwrap(MacOSTrustedInputCompiler.compile([
         pointer(atUs: 0, op: "pointer-down", x: 500_000, y: 500_000, target: "grid:tile:07500"),
         pointer(atUs: 50_000, op: "pointer-up", x: 500_000, y: 500_000, target: "grid:tile:07500"),
      ]).commands.first)
      var stateGeneration = UInt64(41)
      let observer = MacOSTrustedInputApplicationEventObserver {stateGeneration}
      try observer.arm(identity: identity, requestSHA256: String(repeating: "c", count: 64), command: command)
      let event = try XCTUnwrap(NSEvent.mouseEvent(
         with: .leftMouseDown,
         location: NSPoint(x: 195, y: 422),
         modifierFlags: [],
         timestamp: 1,
         windowNumber: 7,
         context: nil,
         eventNumber: 9,
         clickCount: 1,
         pressure: 1
      ))
      observer.observe(event: event, applicationWasForeground: true) {stateGeneration += 1}
      let receipt = try observer.finalize(identity: identity)
      XCTAssertEqual(receipt.firstEventIndex, command.firstEventIndex)
      XCTAssertEqual(receipt.lastEventIndex, command.lastEventIndex)
      XCTAssertEqual(receipt.rawEventFamilies, ["mouse"])
      XCTAssertEqual(receipt.rawEventTypes, [UInt64(NSEvent.EventType.leftMouseDown.rawValue)])
      XCTAssertEqual(receipt.windowNumber, 7)
      XCTAssertEqual(receipt.stateGenerationBefore, 41)
      XCTAssertEqual(receipt.stateGenerationAfter, 42)
      XCTAssertTrue(receipt.applicationWasForeground)
      XCTAssertTrue(receipt.complete)
   }

   func testApplicationObserverRequiresBothMouseAndKeyEvidenceForSelectReplace() throws
   {
      let identity = MacOSTrustedInputIdentity(
         schemaVersion: 1,
         runID: "run",
         planSHA256: String(repeating: "a", count: 64),
         chunkID: "chunk",
         passID: "minimal-presentation",
         packID: "pack",
         pairIndex: 0,
         side: .native,
         generation: String(repeating: "b", count: 64),
         scenarioID: "chat.live-update",
         commandSequence: 0
      )
      let command = try XCTUnwrap(MacOSTrustedInputCompiler.compile([
         text(atUs: 0, op: "focus", target: "chat:append:16", value: "0:6"),
         text(atUs: 500_000, op: "commit-text", target: "chat:append:16", value: "Oxide"),
      ]).commands.first)
      var stateGeneration = UInt64(9)
      let observer = MacOSTrustedInputApplicationEventObserver {stateGeneration}
      try observer.arm(identity: identity, requestSHA256: String(repeating: "c", count: 64), command: command)
      let mouse = try XCTUnwrap(NSEvent.mouseEvent(
         with: .leftMouseDown,
         location: NSPoint(x: 195, y: 422),
         modifierFlags: [],
         timestamp: 1,
         windowNumber: 7,
         context: nil,
         eventNumber: 9,
         clickCount: 1,
         pressure: 1
      ))
      observer.observe(event: mouse, applicationWasForeground: true) {}
      let key = try XCTUnwrap(NSEvent.keyEvent(
         with: .keyDown,
         location: NSPoint(x: 195, y: 422),
         modifierFlags: [],
         timestamp: 2,
         windowNumber: 7,
         context: nil,
         characters: "Oxide",
         charactersIgnoringModifiers: "Oxide",
         isARepeat: false,
         keyCode: 0
      ))
      observer.observe(event: key, applicationWasForeground: true) {stateGeneration += 2}
      let receipt = try observer.finalize(identity: identity)
      XCTAssertEqual(receipt.firstEventIndex, 0)
      XCTAssertEqual(receipt.lastEventIndex, 1)
      XCTAssertEqual(receipt.rawEventFamilies, ["mouse", "key"])
      XCTAssertEqual(receipt.rawEventTypes, [UInt64(NSEvent.EventType.leftMouseDown.rawValue), UInt64(NSEvent.EventType.keyDown.rawValue)])
      XCTAssertEqual(receipt.stateGenerationBefore, 9)
      XCTAssertEqual(receipt.stateGenerationAfter, 11)
      XCTAssertTrue(receipt.complete)
   }

   func testSemanticOverrideReceiptRequiresRawMouseDraggedEvidence() throws
   {
      let identity = MacOSTrustedInputIdentity(
         schemaVersion: 1,
         runID: "run",
         planSHA256: String(repeating: "a", count: 64),
         chunkID: "chunk",
         passID: "minimal-presentation",
         packID: "pack",
         pairIndex: 0,
         side: .oxide,
         generation: String(repeating: "b", count: 64),
         scenarioID: "image.decode-zoom",
         commandSequence: 0
      )
      let command = try XCTUnwrap(MacOSTrustedInputCompiler.compile(currentTrace("image-pinch.json")).commands.first)
      var stateGeneration = UInt64(21)
      let observer = MacOSTrustedInputApplicationEventObserver {stateGeneration}
      try observer.arm(identity: identity, requestSHA256: String(repeating: "c", count: 64), command: command)
      let drag = try XCTUnwrap(NSEvent.mouseEvent(
         with: .leftMouseDragged,
         location: NSPoint(x: 374, y: 814),
         modifierFlags: [],
         timestamp: 1,
         windowNumber: 7,
         context: nil,
         eventNumber: 9,
         clickCount: 1,
         pressure: 1
      ))
      observer.observe(event: drag, applicationWasForeground: true) {stateGeneration += 1}
      let receipt = try observer.finalize(identity: identity)
      XCTAssertEqual(receipt.firstEventIndex, 0)
      XCTAssertEqual(receipt.lastEventIndex, 17)
      XCTAssertEqual(receipt.rawEventFamilies, ["mouse"])
      XCTAssertEqual(receipt.rawEventTypes, [UInt64(NSEvent.EventType.leftMouseDragged.rawValue)])
      XCTAssertTrue(receipt.complete)
   }

   func testApplicationObserverLeavesReceiptIncompleteWithoutARealStateTransition() throws
   {
      let identity = MacOSTrustedInputIdentity(
         schemaVersion: 1,
         runID: "run",
         planSHA256: String(repeating: "a", count: 64),
         chunkID: "chunk",
         passID: "minimal-presentation",
         packID: "pack",
         pairIndex: 0,
         side: .oxide,
         generation: String(repeating: "b", count: 64),
         scenarioID: "grid.large-scroll",
         commandSequence: 0
      )
      let command = try XCTUnwrap(MacOSTrustedInputCompiler.compile([
         pointer(atUs: 0, op: "pointer-down", x: 500_000, y: 500_000, target: "grid:tile:07500"),
         pointer(atUs: 50_000, op: "pointer-up", x: 500_000, y: 500_000, target: "grid:tile:07500"),
      ]).commands.first)
      let observer = MacOSTrustedInputApplicationEventObserver {17}
      try observer.arm(identity: identity, requestSHA256: String(repeating: "c", count: 64), command: command)
      let event = try XCTUnwrap(NSEvent.mouseEvent(
         with: .leftMouseDown,
         location: NSPoint(x: 195, y: 422),
         modifierFlags: [],
         timestamp: 1,
         windowNumber: 7,
         context: nil,
         eventNumber: 9,
         clickCount: 1,
         pressure: 1
      ))
      observer.observe(event: event, applicationWasForeground: true) {}
      let receipt = try observer.finalize(identity: identity)
      XCTAssertEqual(receipt.stateGenerationBefore, 17)
      XCTAssertEqual(receipt.stateGenerationAfter, 17)
      XCTAssertFalse(receipt.complete)
   }

   func testExecutorUsesOneSessionGlobalArtifactSequenceAcrossScenarios() throws
   {
      let source = URL(fileURLWithPath: #filePath)
      let executorURL = source.deletingLastPathComponent().deletingLastPathComponent().appendingPathComponent("Shared/ComparatorApp/BenchmarkCampaignExecutor.swift")
      let executor = String(decoding: try Data(contentsOf: executorURL), as: UTF8.self)
      XCTAssertTrue(executor.contains("private var trustedInputCommandSequence = UInt64(0)"))
      XCTAssertTrue(executor.contains("let commandSequence = trustedInputCommandSequence\n         trustedInputCommandSequence += 1"))
      XCTAssertTrue(executor.contains("sequence: commandSequence"))
      XCTAssertFalse(executor.contains("sequence: UInt64(commandIndex)"))
   }

   func testXCUIControllerUsesOnlyPublicInputAndAlwaysHasOuterDismissal() throws
   {
      let source = URL(fileURLWithPath: #filePath)
      let root = source.deletingLastPathComponent().deletingLastPathComponent()
      let controller = String(decoding: try Data(contentsOf: root.appendingPathComponent("MacOSComparisonControllerUITests/MacOSTrustedInputXCUIController.swift")), as: UTF8.self)
      let test = String(decoding: try Data(contentsOf: root.appendingPathComponent("MacOSComparisonControllerUITests/MacOSComparisonControllerUITests.swift")), as: UTF8.self)
      for operation in [".click()", ".scroll(", ".typeText(", ".typeKey(", ".press(forDuration:"]
      {
         XCTAssertTrue(controller.contains(operation))
      }
      XCTAssertTrue(controller.contains("target.typeKey(.home, modifierFlags: [])"))
      XCTAssertTrue(controller.contains("target.typeKey(.rightArrow, modifierFlags: .shift)"))
      XCTAssertTrue(controller.contains("target.typeText(\"Oxide\")"))
      XCTAssertTrue(controller.contains("case .imagePinchOverride:"))
      XCTAssertTrue(controller.contains("target.coordinate(withNormalizedOffset:"))
      XCTAssertTrue(controller.contains("case .navigationInteractiveCancelOverride:"))
      XCTAssertTrue(controller.contains("coordinate(x: command.startXMillionths"))
      XCTAssertTrue(controller.contains("requests = try preloadRequests()"))
      XCTAssertTrue(controller.contains("sha256: comparisonSHA256(data)"))
      XCTAssertTrue(controller.contains("JSONEncoder.comparisonCanonical.encode(request) == data"))
      XCTAssertTrue(controller.contains("macOSTrustedInputDispatchReceiptNotification"))
      XCTAssertTrue(controller.contains("private func flushReceipts() throws"))
      XCTAssertTrue(controller.contains("if try consumeFlushRequest()"))
      XCTAssertTrue(controller.contains("controlStore.durableJSON(envelope"))
      XCTAssertFalse(controller.contains("MacOSTrustedInputFlushSignal"))
      XCTAssertFalse(controller.contains("store.durableJSON"))
      let flushRequest = try XCTUnwrap(controller.range(of: "if try consumeFlushRequest()"))
      let applicationState = try XCTUnwrap(controller.range(of: "if flushed && application.state == .notRunning"))
      XCTAssertLessThan(flushRequest.lowerBound, applicationState.lowerBound)
      XCTAssertFalse(controller.contains("DistributedNotificationCenter.default().addObserver"))
      XCTAssertFalse(controller.contains("adapter.apply"))
      let contract = String(decoding: try Data(contentsOf: root.appendingPathComponent("Shared/ComparatorApp/MacOSTrustedInputControl.swift")), as: UTF8.self)
      XCTAssertTrue(contract.contains("final class MacOSTrustedInputWindow: NSWindow"))
      XCTAssertTrue(contract.contains("controllerReceiptSHA256"))
      XCTAssertTrue(contract.contains("trusted-input.flush.\\(suffix).json"))
      XCTAssertTrue(contract.contains("stateGenerationAfter > applicationObservation.stateGenerationBefore"))
      XCTAssertFalse(contract.contains("raw application input receipt is not implemented"))
      XCTAssertTrue(test.contains("defer\n      {\n         if application.state != .notRunning"))
      XCTAssertTrue(test.contains("application.terminate()"))
      XCTAssertTrue(test.contains("-oxide-compare-controller-root"))
      XCTAssertTrue(test.contains("FileManager.default.removeItem(at: controllerRoot)"))
      XCTAssertTrue(test.contains("MacOSTrustedInputXCUIController("))
   }

   func testScheduledInputUsesPrevalidatedMemoryAndDefersReceiptPersistence() throws
   {
      let source = URL(fileURLWithPath: #filePath)
      let root = source.deletingLastPathComponent().deletingLastPathComponent()
      let applicationSource = String(decoding: try Data(contentsOf: root.appendingPathComponent("Shared/ComparatorApp/MacOSTrustedInputControl.swift")), as: UTF8.self)
      let controllerSource = String(decoding: try Data(contentsOf: root.appendingPathComponent("MacOSComparisonControllerUITests/MacOSTrustedInputXCUIController.swift")), as: UTF8.self)
      let executorSource = String(decoding: try Data(contentsOf: root.appendingPathComponent("Shared/ComparatorApp/BenchmarkCampaignExecutor.swift")), as: UTF8.self)
      let submit = try XCTUnwrap(applicationSource.range(of: "   func submit(command:"))
      let flush = try XCTUnwrap(applicationSource.range(of: "   func flush() throws", range: submit.lowerBound..<applicationSource.endIndex))
      let scheduledApplicationPath = String(applicationSource[submit.lowerBound..<flush.lowerBound])
      for forbidden in ["Data(contentsOf:", "JSONDecoder", "JSONEncoder", "comparisonSHA256", "durableJSON", "durableWrite", "FileManager"]
      {
         XCTAssertFalse(scheduledApplicationPath.contains(forbidden), forbidden)
      }
      XCTAssertTrue(scheduledApplicationPath.contains("mailbox.wait"))
      let service = try XCTUnwrap(controllerSource.range(of: "   private func service(sequence:"))
      let preload = try XCTUnwrap(controllerSource.range(of: "   private func preloadRequests()", range: service.lowerBound..<controllerSource.endIndex))
      let scheduledControllerPath = String(controllerSource[service.lowerBound..<preload.lowerBound])
      for forbidden in ["Data(contentsOf:", "JSONDecoder", "JSONEncoder", "comparisonSHA256", "durableJSON", "durableWrite", "FileManager"]
      {
         XCTAssertFalse(scheduledControllerPath.contains(forbidden), forbidden)
      }
      XCTAssertTrue(controllerSource.contains("requests = try preloadRequests()"))
      XCTAssertTrue(applicationSource.contains("let artifact = try store.durableJSON(request"))
      XCTAssertTrue(applicationSource.contains("let descriptorPreparedTimestamp: UInt64"))
      XCTAssertFalse(applicationSource.contains("requestedTimestamp"))
      XCTAssertTrue(controllerSource.contains("let started = mach_continuous_time()\n      try execute(request.command)"))
      XCTAssertTrue(applicationSource.contains("func durableReceipt(controllerReceiptSHA256:"))
      XCTAssertTrue(executorSource.contains("try trustedInputCoordinator?.flush()"))
      XCTAssertTrue(applicationSource.contains("MacOSTrustedInputFlushEnvelope("))
      XCTAssertTrue(applicationSource.contains("macOSTrustedInputFlushControlFile(suffix:"))
      XCTAssertTrue(applicationSource.contains("let controlStore = DurableArtifactStore(root:"))
      XCTAssertTrue(applicationSource.contains("store.durableWrite(controllerData"))
      XCTAssertTrue(controllerSource.contains("private func consumeFlushRequest() throws"))
   }

   private func currentTrace(_ name: String) throws -> [BenchmarkTraceEvent]
   {
      let source = URL(fileURLWithPath: #filePath)
      let root = source.deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
      let url = root.appendingPathComponent("benchmarks/comparative/specs/v1/traces/\(name)")
      let decoder = JSONDecoder()
      decoder.keyDecodingStrategy = .convertFromSnakeCase
      return try decoder.decode([BenchmarkTraceEvent].self, from: Data(contentsOf: url))
   }

   private func pointer(atUs: UInt64, op: String, pointer: UInt32 = 1, x: Int32, y: Int32, target: String? = nil) -> BenchmarkTraceEvent
   {
      BenchmarkTraceEvent(atUs: atUs, op: op, pointer: pointer, xMillionths: x, yMillionths: y, deltaXMillionths: nil, deltaYMillionths: nil, target: target, value: nil, stateId: nil)
   }

   private func event(atUs: UInt64, op: String, target: String? = nil, deltaY: Int32? = nil) -> BenchmarkTraceEvent
   {
      BenchmarkTraceEvent(atUs: atUs, op: op, pointer: nil, xMillionths: nil, yMillionths: nil, deltaXMillionths: nil, deltaYMillionths: deltaY, target: target, value: nil, stateId: nil)
   }

   private func text(atUs: UInt64, op: String, target: String, value: String) -> BenchmarkTraceEvent
   {
      BenchmarkTraceEvent(atUs: atUs, op: op, pointer: nil, xMillionths: nil, yMillionths: nil, deltaXMillionths: nil, deltaYMillionths: nil, target: target, value: .text(value), stateId: nil)
   }
}
