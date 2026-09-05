import AppKit
import XCTest

final class AppKitProductionScenarioAdapterTests: XCTestCase
{
   private let scenarioIDs = [
      AppKitProductionSceneID.startupFirstScreen,
      .dashboardMixedStatic,
      .enduranceChurn,
      .feedVariableScroll,
      .chatLiveUpdate,
      .navigationModal,
      .imageDecodeZoom,
   ].map(\.rawValue) + ["idle.steady"]

   func testEveryFrozenCheckpointMatchesProductionReferenceStateAndAccessibility() throws
   {
      _ = NSApplication.shared
      let loader = BenchmarkSpecLoader(root: try productionSpecRoot())
      var observedCheckpointCount = 0
      for scenarioID in scenarioIDs
      {
         let window = productionComparisonWindow()
         let adapter = AppKitProductionScenarioAdapter(window: window)
         let scenario = try loader.loadScenario(relativePath: "scenarios/\(scenarioID).json")
         try adapter.prepare(scenario: scenario, loader: loader)
         var schedule = try BenchmarkPhaseSchedule(scenario: scenario, setupDurationUs: 0, loader: loader)
         try schedule.drainTimed(until: UInt64.max)
         {
            timeUs, action in
            try adapter.setVirtualTimeUs(timeUs)
            switch action
            {
            case .traceEvent(let event):
               try adapter.apply(event: event)
            case .checkpoint(let checkpointID):
               guard let expected = scenario.parityCheckpoints.first(where: {$0.id == checkpointID}) else
               {
                  return XCTFail("missing checkpoint \(scenarioID):\(checkpointID)")
               }
               let actual = try adapter.checkpoint(id: checkpointID)
               try assertProductionJSONEqual(actual.state, try loader.read(expected.state), "\(scenarioID):\(checkpointID):state")
               try assertProductionJSONEqual(actual.accessibility, try loader.read(expected.accessibility), "\(scenarioID):\(checkpointID):accessibility")
               XCTAssertEqual(actual.visibleRoleCounts, expected.expectedVisibleRoleCounts, "\(scenarioID):\(checkpointID):roles")
               observedCheckpointCount += 1
            default:
               break
            }
         }
         XCTAssertTrue(schedule.isComplete, scenarioID)
         try adapter.teardown()
         window.close()
      }
      XCTAssertEqual(observedCheckpointCount, 37)
   }

   func testEveryProductionSceneCapturesOpaqueCanonicalThreeXOutput() throws
   {
      _ = NSApplication.shared
      let loader = BenchmarkSpecLoader(root: try productionSpecRoot())
      for scenarioID in scenarioIDs
      {
         let window = productionComparisonWindow()
         let adapter = AppKitProductionScenarioAdapter(window: window)
         let scenario = try loader.loadScenario(relativePath: "scenarios/\(scenarioID).json")
         try adapter.prepare(scenario: scenario, loader: loader)
         let png = try adapter.previewPNG()
         guard let representation = NSBitmapImageRep(data: png) else
         {
            return XCTFail("missing PNG for \(scenarioID)")
         }
         XCTAssertEqual(representation.pixelsWide, 1_170, scenarioID)
         XCTAssertEqual(representation.pixelsHigh, 2_532, scenarioID)
         XCTAssertFalse(representation.hasAlpha, scenarioID)
         try adapter.teardown()
         window.close()
      }
   }

   func testCanonicalLaunchProbeUsesTheNativeStartupButtonAndDisplaysAResponse() throws
   {
      _ = NSApplication.shared
      let loader = BenchmarkSpecLoader(root: try productionSpecRoot())
      let window = productionComparisonWindow()
      let adapter = AppKitProductionScenarioAdapter(window: window)
      let scenario = try loader.loadScenario(relativePath: "scenarios/startup.first-screen.json")
      adapter.configure(passID: "canonical-launch")
      try adapter.prepare(scenario: scenario, loader: loader)
      var observedTargetAction = false
      var responseResult: Result<Void, Error>?
      adapter.onCanonicalLaunchResponsePresented = {responseResult = $0}
      let descriptor = try adapter.armTrustedInputProbe {observedTargetAction = true}
      let initialGeneration = adapter.stateGeneration
      let allViews = productionDescendants(of: window.contentView)
      guard let primaryAction = allViews.first(where: {$0.identifier?.rawValue == "startup.primary-action"}) as? NSButton,
            let responsePatch = allViews.first(where: {$0.identifier?.rawValue == "startup.launch-probe-response"}) else
      {
         return XCTFail("missing canonical launch controls")
      }

      XCTAssertEqual(descriptor.dispatchPath, "trusted-os-input-target-action")
      XCTAssertEqual(descriptor.windowNumber, window.windowNumber)
      XCTAssertTrue(descriptor.xPoints.isFinite)
      XCTAssertTrue(descriptor.yPoints.isFinite)
      XCTAssertTrue(responsePatch.isHidden)
      primaryAction.performClick(nil)
      XCTAssertTrue(observedTargetAction)
      XCTAssertEqual(adapter.stateGeneration, initialGeneration + 1)
      XCTAssertFalse(responsePatch.isHidden)
      let deadline = Date(timeIntervalSinceNow: 1)
      while responseResult == nil && RunLoop.current.run(mode: .default, before: deadline) {}
      XCTAssertNoThrow(try responseResult?.get())

      try adapter.teardown()
      window.close()
   }

   func testCanonicalCaptureSurfaceIsReusedAcrossScenesAndReleasedWithAdapter() throws
   {
      _ = NSApplication.shared
      let loader = BenchmarkSpecLoader(root: try productionSpecRoot())
      let window = productionComparisonWindow()
      let adapter = AppKitProductionScenarioAdapter(window: window)
      let dashboard = try loader.loadScenario(relativePath: "scenarios/dashboard.mixed-static.json")
      try adapter.prepare(scenario: dashboard, loader: loader)
      _ = try adapter.previewPNG()
      _ = try adapter.previewPNG()
      XCTAssertEqual(adapter.previewSurfaceAllocationCount, 1)
      XCTAssertTrue(adapter.hasPreviewSurface)
      try adapter.teardown()
      XCTAssertTrue(adapter.hasPreviewSurface)

      let feed = try loader.loadScenario(relativePath: "scenarios/feed.variable-scroll.json")
      try adapter.prepare(scenario: feed, loader: loader)
      _ = try adapter.previewPNG()
      XCTAssertEqual(adapter.previewSurfaceAllocationCount, 1)
      try adapter.teardown()
      XCTAssertTrue(adapter.hasPreviewSurface)
      window.close()
   }

   func testResizeCaptureUsesTheCurrentLandscapeViewport() throws
   {
      _ = NSApplication.shared
      let root = try productionSpecRoot()
      let loader = BenchmarkSpecLoader(root: root)
      let window = productionComparisonWindow()
      let adapter = AppKitProductionScenarioAdapter(window: window)
      let scenario = try releaseCandidateScenario("resize.theme", root: root)
      try adapter.prepare(scenario: scenario, loader: loader)
      for event in try loader.loadTrace(try XCTUnwrap(scenario.phases.first(where: {$0.id == "ten-changes"})?.trace)).prefix(3)
      {
         try adapter.apply(event: event)
      }
      guard let representation = NSBitmapImageRep(data: try adapter.previewPNG()) else
      {
         return XCTFail("missing landscape PNG")
      }
      XCTAssertEqual(window.contentView?.bounds.size, NSSize(width: 844, height: 390))
      let geometry = try adapter.correctnessGeometry()
      XCTAssertEqual(geometry.captureProfile, BenchmarkMacOSCorrectnessCaptureProfile.canonical.id)
      XCTAssertEqual(geometry.canonicalScale, 3)
      XCTAssertEqual(geometry.root, BenchmarkMacOSCorrectnessLogicalRect(x: 0, y: 0, width: 844, height: 390))
      XCTAssertEqual(representation.pixelsWide, 2_532)
      XCTAssertEqual(representation.pixelsHigh, 1_170)
      XCTAssertFalse(representation.hasAlpha)
      try adapter.teardown()
      window.close()
   }

   func testSixPrimaryScenesExposeTheOxideSemanticIdentifierSets() throws
   {
      _ = NSApplication.shared
      let loader = BenchmarkSpecLoader(root: try productionSpecRoot())
      for scenarioID in [
         "startup.first-screen",
         "dashboard.mixed-static",
         "feed.variable-scroll",
         "chat.live-update",
         "navigation.modal",
         "image.decode-zoom",
      ]
      {
         let window = productionComparisonWindow()
         let adapter = AppKitProductionScenarioAdapter(window: window)
         let scenario = try loader.loadScenario(relativePath: "scenarios/\(scenarioID).json")
         try adapter.prepare(scenario: scenario, loader: loader)
         let geometry = try adapter.correctnessGeometry()
         let identifiers = Set(try geometry.nodes.map {try XCTUnwrap($0.identifier)})
         XCTAssertEqual(identifiers, expectedInitialSemanticIdentifiers(scenarioID: scenarioID), scenarioID)
         XCTAssertEqual(identifiers.count, geometry.nodes.count, "duplicate semantic identifier: \(scenarioID)")
         try adapter.teardown()
         window.close()
      }
   }

   func testExactSemanticGeometryRejectsInjectedLayoutAndTextDrift() throws
   {
      _ = NSApplication.shared
      let loader = BenchmarkSpecLoader(root: try productionSpecRoot())
      let window = productionComparisonWindow()
      let adapter = AppKitProductionScenarioAdapter(window: window)
      let scenario = try loader.loadScenario(relativePath: "scenarios/startup.first-screen.json")
      try adapter.prepare(scenario: scenario, loader: loader)
      let baseline = try adapter.correctnessGeometry()
      let views = productionDescendants(of: window.contentView)

      let header = try XCTUnwrap(views.first {$0.identifier?.rawValue == "startup.header"})
      header.frame.origin.x += 1
      let layoutDrift = try adapter.correctnessGeometry()
      XCTAssertNotEqual(layoutDrift, baseline)
      XCTAssertNotEqual(
         layoutDrift.nodes.first {$0.identifier == "startup:header"}?.bounds,
         baseline.nodes.first {$0.identifier == "startup:header"}?.bounds
      )
      header.frame.origin.x -= 1

      let title = try XCTUnwrap(views.first {$0.identifier?.rawValue == "startup:card:00:title"})
      title.frame.origin.y += 1
      let textDrift = try adapter.correctnessGeometry()
      XCTAssertNotEqual(textDrift, baseline)
      XCTAssertNotEqual(
         textDrift.nodes.first {$0.identifier == "startup:card:00"}?.textLineBounds,
         baseline.nodes.first {$0.identifier == "startup:card:00"}?.textLineBounds
      )
      try adapter.teardown()
      window.close()
   }

   func testFiveReleaseCandidatesExposeTheOxideSemanticIdentifierSets() throws
   {
      _ = NSApplication.shared
      let root = try productionSpecRoot()
      let loader = BenchmarkSpecLoader(root: root)
      for scenarioID in ["grid.large-scroll", "effects.layers", "mutation.damage", "text.multilingual", "resize.theme"]
      {
         let window = productionComparisonWindow()
         let adapter = AppKitProductionScenarioAdapter(window: window)
         try adapter.prepare(scenario: try releaseCandidateScenario(scenarioID, root: root), loader: loader)
         let geometry = try adapter.correctnessGeometry()
         let identifiers = Set(try geometry.nodes.map {try XCTUnwrap($0.identifier)})
         XCTAssertEqual(identifiers, expectedInitialSemanticIdentifiers(scenarioID: scenarioID), scenarioID)
         XCTAssertEqual(identifiers.count, geometry.nodes.count, "duplicate semantic identifier: \(scenarioID)")
         try adapter.teardown()
         window.close()
      }
   }

   func testReleaseCandidateCaptureLoaderIsExplicitAndScreenshotFree() throws
   {
      let root = try productionSpecRoot()
      let loader = BenchmarkSpecLoader(root: root)
      var checkpointCount = 0
      for scenarioID in BenchmarkReleaseCandidateCaptureExecutor.scenarioIDs
      {
         XCTAssertThrowsError(try loader.loadScenario(relativePath: "release-candidates/\(scenarioID).candidate.json"))
         let scenario = try loader.loadReleaseCandidateCaptureScenario(id: scenarioID)
         XCTAssertEqual(scenario.id, scenarioID)
         XCTAssertTrue(scenario.parityCheckpoints.allSatisfy {$0.screenshot == nil})
         XCTAssertTrue(scenario.parityCheckpoints.allSatisfy {!$0.expectedVisibleRoleCounts.isEmpty})
         checkpointCount += scenario.parityCheckpoints.count
      }
      XCTAssertEqual(checkpointCount, 18)
      XCTAssertThrowsError(try loader.loadReleaseCandidateCaptureScenario(id: "dashboard.mixed-static"))
   }

   func testReleaseCandidateGeometryRejectsInjectedElementAndTextDrift() throws
   {
      _ = NSApplication.shared
      let root = try productionSpecRoot()
      let loader = BenchmarkSpecLoader(root: root)

      let effectsWindow = productionComparisonWindow()
      let effectsAdapter = AppKitProductionScenarioAdapter(window: effectsWindow)
      try effectsAdapter.prepare(scenario: try releaseCandidateScenario("effects.layers", root: root), loader: loader)
      let effectsBaseline = try effectsAdapter.correctnessGeometry()
      let card = try XCTUnwrap(productionDescendants(of: effectsWindow.contentView).first {$0.identifier?.rawValue == "effects:card:037"})
      card.frame.origin.x += 1
      let effectsDrift = try effectsAdapter.correctnessGeometry()
      XCTAssertNotEqual(
         effectsDrift.nodes.first {$0.identifier == "effects:card:037"}?.bounds,
         effectsBaseline.nodes.first {$0.identifier == "effects:card:037"}?.bounds
      )
      try effectsAdapter.teardown()
      effectsWindow.close()

      let textWindow = productionComparisonWindow()
      let textAdapter = AppKitProductionScenarioAdapter(window: textWindow)
      try textAdapter.prepare(scenario: try releaseCandidateScenario("text.multilingual", root: root), loader: loader)
      let textBaseline = try textAdapter.correctnessGeometry()
      let scrollView = try XCTUnwrap(productionDescendants(of: textWindow.contentView).first {$0.identifier?.rawValue == "text.scroll"})
      scrollView.frame.origin.y += 1
      let textDrift = try textAdapter.correctnessGeometry()
      XCTAssertNotEqual(
         textDrift.nodes.first {$0.identifier == "text:label:000"}?.textLineBounds,
         textBaseline.nodes.first {$0.identifier == "text:label:000"}?.textLineBounds
      )
      try textAdapter.teardown()
      textWindow.close()
   }

   func testMeasuredNavigationAndComposerEventsDispatchThroughNativeAppKitControls() throws
   {
      _ = NSApplication.shared
      let loader = BenchmarkSpecLoader(root: try productionSpecRoot())

      let navigationWindow = productionComparisonWindow()
      let navigationAdapter = AppKitProductionScenarioAdapter(window: navigationWindow)
      navigationAdapter.configure(passID: "minimal-presentation")
      let navigationScenario = try loader.loadScenario(relativePath: "scenarios/navigation.modal.json")
      try navigationAdapter.prepare(scenario: navigationScenario, loader: loader)
      XCTAssertEqual(navigationAdapter.stateGeneration, 0)
      let navigationTrace = try loader.loadTrace(try XCTUnwrap(navigationScenario.phases.first(where: {$0.id == "canonical-cycles"})?.trace))
      for (index, event) in navigationTrace.prefix(4).enumerated()
      {
         try navigationAdapter.apply(event: event)
         XCTAssertEqual(navigationAdapter.lastEventDispatchRoute, .nativeTargetAction(try XCTUnwrap(event.target)))
         XCTAssertEqual(navigationAdapter.stateGeneration, UInt64(index + 1))
      }
      try navigationAdapter.reset()
      XCTAssertEqual(navigationAdapter.stateGeneration, 0)
      try navigationAdapter.teardown()
      navigationWindow.close()

      let chatWindow = productionComparisonWindow()
      let chatAdapter = AppKitProductionScenarioAdapter(window: chatWindow)
      chatAdapter.configure(passID: "minimal-presentation")
      let chatScenario = try loader.loadScenario(relativePath: "scenarios/chat.live-update.json")
      try chatAdapter.prepare(scenario: chatScenario, loader: loader)
      XCTAssertEqual(chatAdapter.stateGeneration, 0)
      let composerTrace = try loader.loadTrace(try XCTUnwrap(chatScenario.phases.first(where: {$0.id == "type-100"})?.trace))
      try chatAdapter.apply(event: try XCTUnwrap(composerTrace.first))
      XCTAssertEqual(chatAdapter.lastEventDispatchRoute, .nativeTextInput("chat:composer"))
      XCTAssertEqual(chatAdapter.stateGeneration, 1)
      let chatTable = try XCTUnwrap(productionDescendants(of: chatWindow.contentView).first {$0.identifier?.rawValue == "chat.thread"} as? NSTableView)
      let initialMessageCount = chatTable.numberOfRows
      let sendButton = try XCTUnwrap(productionDescendants(of: chatWindow.contentView).first {$0.identifier?.rawValue == "chat.send"} as? NSButton)
      sendButton.performClick(nil)
      XCTAssertEqual(chatTable.numberOfRows, initialMessageCount + 1)
      XCTAssertEqual(chatAdapter.stateGeneration, 2)
      let composer = try XCTUnwrap(productionDescendants(of: chatWindow.contentView).first {$0.identifier?.rawValue == "chat.composer"} as? NSTextView)
      XCTAssertEqual(composer.string, "")
      try chatAdapter.teardown()
      chatWindow.close()
   }

   func testMeasuredNavigationInteractiveCancelUsesNativeMouseDragAndRestoresListOnMouseUp() throws
   {
      _ = NSApplication.shared
      let loader = BenchmarkSpecLoader(root: try productionSpecRoot())
      let window = productionInteractiveWindow()
      let adapter = AppKitProductionScenarioAdapter(window: window)
      adapter.configure(passID: "minimal-presentation")
      let scenario = try loader.loadScenario(relativePath: "scenarios/navigation.modal.json")
      try adapter.prepare(scenario: scenario, loader: loader)
      let root = try XCTUnwrap(productionDescendants(of: window.contentView).first {$0.identifier?.rawValue == "production-reference.navigation.modal.root"})
      let recognizer = try XCTUnwrap(root.gestureRecognizers.compactMap {$0 as? AppKitNavigationInteractiveCancelGestureRecognizer}.first)
      let timestamp = ProcessInfo.processInfo.systemUptime
      let down = try productionMouseEvent(type: .leftMouseDown, window: window, view: root, point: NSPoint(x: 370, y: 422), timestamp: timestamp, eventNumber: 1)
      let drag = try productionMouseEvent(type: .leftMouseDragged, window: window, view: root, point: NSPoint(x: 195, y: 422), timestamp: timestamp + 0.25, eventNumber: 2)
      let up = try productionMouseEvent(type: .leftMouseUp, window: window, view: root, point: NSPoint(x: 195, y: 422), timestamp: timestamp + 0.5, eventNumber: 3)

      recognizer.mouseDown(with: down)
      recognizer.mouseDragged(with: drag)
      XCTAssertGreaterThan(adapter.stateGeneration, 0)
      recognizer.mouseUp(with: up)

      let checkpoint = try adapter.checkpoint(id: "cancel-restored")
      let state = try XCTUnwrap(JSONSerialization.jsonObject(with: checkpoint.state) as? [String: Any])
      let model = try XCTUnwrap(state["model"] as? [String: Any])
      XCTAssertEqual(model["route"] as? String, "list")
      XCTAssertEqual(model["modal_visible"] as? Bool, false)
      XCTAssertFalse(try XCTUnwrap(productionDescendants(of: window.contentView).first {$0.identifier?.rawValue == "navigation.scroll"}).isHidden)

      try adapter.teardown()
      window.close()
   }

   func testMeasuredImageZoomUsesNativeSliderMouseTrackingAndAdvancesGeneration() throws
   {
      _ = NSApplication.shared
      let loader = BenchmarkSpecLoader(root: try productionSpecRoot())
      let window = productionInteractiveWindow()
      let adapter = AppKitProductionScenarioAdapter(window: window)
      adapter.configure(passID: "minimal-presentation")
      let scenario = try loader.loadScenario(relativePath: "scenarios/image.decode-zoom.json")
      try adapter.prepare(scenario: scenario, loader: loader)
      let slider = try XCTUnwrap(productionDescendants(of: window.contentView).first {$0.identifier?.rawValue == "image.zoom"} as? NSSlider)
      let timestamp = ProcessInfo.processInfo.systemUptime
      let down = try productionMouseEvent(type: .leftMouseDown, window: window, view: slider, point: NSPoint(x: 8, y: 14), timestamp: timestamp, eventNumber: 1)
      let drag = try productionMouseEvent(type: .leftMouseDragged, window: window, view: slider, point: NSPoint(x: slider.bounds.maxX, y: 14), timestamp: timestamp + 0.25, eventNumber: 2)
      let up = try productionMouseEvent(type: .leftMouseUp, window: window, view: slider, point: NSPoint(x: slider.bounds.maxX, y: 14), timestamp: timestamp + 0.5, eventNumber: 3)

      slider.mouseDown(with: down)
      slider.mouseDragged(with: drag)
      slider.mouseUp(with: up)

      let checkpoint = try adapter.checkpoint(id: "thumbnail")
      let state = try XCTUnwrap(JSONSerialization.jsonObject(with: checkpoint.state) as? [String: Any])
      let model = try XCTUnwrap(state["model"] as? [String: Any])
      XCTAssertEqual(model["scale_millionths"] as? Int, 2_000_000)
      XCTAssertGreaterThan(adapter.stateGeneration, 0)

      try adapter.teardown()
      window.close()
   }

   func testMeasuredFeedScrollTracksOnlyUserDrivenBoundsChanges() throws
   {
      _ = NSApplication.shared
      let loader = BenchmarkSpecLoader(root: try productionSpecRoot())
      let window = productionComparisonWindow()
      let adapter = AppKitProductionScenarioAdapter(window: window)
      adapter.configure(passID: "minimal-presentation")
      let scenario = try loader.loadScenario(relativePath: "scenarios/feed.variable-scroll.json")
      try adapter.prepare(scenario: scenario, loader: loader)
      let scrollView = try XCTUnwrap(productionDescendants(of: window.contentView).first {$0.identifier?.rawValue == "feed.scroll"} as? NSScrollView)

      scrollView.contentView.scroll(to: NSPoint(x: 0, y: 240))
      scrollView.reflectScrolledClipView(scrollView.contentView)
      adapter.observeMeasuredFeedScroll(boundsOriginY: 240, eventType: nil)
      XCTAssertEqual(adapter.stateGeneration, 0)
      var state = try XCTUnwrap(JSONSerialization.jsonObject(with: adapter.checkpoint(id: "initial").state) as? [String: Any])
      var model = try XCTUnwrap(state["model"] as? [String: Any])
      XCTAssertEqual(model["scroll_position_millionths"] as? Int, 0)

      adapter.observeMeasuredFeedScroll(boundsOriginY: 240, eventType: .scrollWheel)
      XCTAssertEqual(adapter.stateGeneration, 1)
      state = try XCTUnwrap(JSONSerialization.jsonObject(with: adapter.checkpoint(id: "initial").state) as? [String: Any])
      model = try XCTUnwrap(state["model"] as? [String: Any])
      XCTAssertGreaterThan(model["scroll_position_millionths"] as? Int ?? 0, 0)

      try adapter.reset()
      XCTAssertEqual(adapter.stateGeneration, 0)
      state = try XCTUnwrap(JSONSerialization.jsonObject(with: adapter.checkpoint(id: "initial").state) as? [String: Any])
      model = try XCTUnwrap(state["model"] as? [String: Any])
      XCTAssertEqual(model["scroll_position_millionths"] as? Int, 0)
      try adapter.teardown()
      window.close()
   }

   func testMeasuredChatSelectionReplacementUsesTheNativeEditableMessage() throws
   {
      _ = NSApplication.shared
      let loader = BenchmarkSpecLoader(root: try productionSpecRoot())
      let window = productionComparisonWindow()
      let adapter = AppKitProductionScenarioAdapter(window: window)
      adapter.configure(passID: "minimal-presentation")
      let scenario = try loader.loadScenario(relativePath: "scenarios/chat.live-update.json")
      try adapter.prepare(scenario: scenario, loader: loader)
      let prependTrace = try loader.loadTrace(try XCTUnwrap(scenario.phases.first(where: {$0.id == "prepend-50"})?.trace))
      for event in prependTrace
      {
         try adapter.apply(event: event)
      }
      let appendTrace = try loader.loadTrace(try XCTUnwrap(scenario.phases.first(where: {$0.id == "append-10hz"})?.trace))
      for event in appendTrace
      {
         try adapter.apply(event: event)
      }
      let table = try XCTUnwrap(productionDescendants(of: window.contentView).first {$0.identifier?.rawValue == "chat.thread"} as? NSTableView)
      let controller = try XCTUnwrap(table.dataSource as? AppKitChatTableController)
      table.layoutSubtreeIfNeeded()
      let row = try XCTUnwrap(controller.row(for: "chat:append:16"))
      XCTAssertTrue(table.rows(in: table.visibleRect).contains(row))
      let cell = try XCTUnwrap(table.view(atColumn: 0, row: row, makeIfNecessary: false) as? AppKitChatTableCellView)
      let field = try XCTUnwrap(cell.messageFields[0] as? AppKitChatEditableMessageField)
      XCTAssertEqual(field.identifier?.rawValue, "chat:append:16")
      XCTAssertTrue(field.isEditable)
      XCTAssertTrue(field.isSelectable)
      XCTAssertEqual(field.stringValue, "Stable frames keep conversation feeling immediate.")
      XCTAssertEqual(adapter.stateGeneration, 0)

      field.selectText(nil)
      RunLoop.current.run(until: Date(timeIntervalSinceNow: 0.01))
      let editor = try XCTUnwrap(field.currentEditor() as? NSTextView)
      field.observeNativeEditor(editor)
      editor.setSelectedRange(NSRange(location: 0, length: 6))
      RunLoop.current.run(until: Date(timeIntervalSinceNow: 0.01))
      XCTAssertEqual(adapter.stateGeneration, 1)
      editor.insertText("Oxide", replacementRange: editor.selectedRange())
      RunLoop.current.run(until: Date(timeIntervalSinceNow: 0.01))
      XCTAssertEqual(adapter.stateGeneration, 2)

      let checkpoint = try adapter.checkpoint(id: "selection-replaced")
      let state = try XCTUnwrap(JSONSerialization.jsonObject(with: checkpoint.state) as? [String: Any])
      let model = try XCTUnwrap(state["model"] as? [String: Any])
      XCTAssertEqual(model["focused_message_id"] as? String, "chat:append:16")
      XCTAssertEqual(model["selection_active"] as? Bool, false)
      XCTAssertEqual(model["replacement_applied"] as? Bool, true)
      let updatedRow = try XCTUnwrap(controller.row(for: "chat:append:16"))
      XCTAssertTrue(controller.records[updatedRow].text.hasPrefix("Oxide frames"))
      XCTAssertTrue(table.rows(in: table.visibleRect).contains(controller.records.count - 1))
      try adapter.teardown()
      window.close()
   }

   func testUnsupportedMeasuredGesturesFailClosedInsteadOfApplyingTheModelDirectly() throws
   {
      _ = NSApplication.shared
      let loader = BenchmarkSpecLoader(root: try productionSpecRoot())
      let window = productionComparisonWindow()
      let adapter = AppKitProductionScenarioAdapter(window: window)
      adapter.configure(passID: "minimal-presentation")
      let scenario = try loader.loadScenario(relativePath: "scenarios/feed.variable-scroll.json")
      try adapter.prepare(scenario: scenario, loader: loader)
      let trace = try loader.loadTrace(try XCTUnwrap(scenario.phases.first(where: {$0.id == "forward-fling"})?.trace))
      let event = try XCTUnwrap(trace.first)

      XCTAssertThrowsError(try adapter.apply(event: event))
      XCTAssertEqual(adapter.lastEventDispatchRoute, .unsupported(event.target ?? event.op))

      try adapter.teardown()
      window.close()
   }

   func testImageUploadPhasePerformsTexturePreparationBeforeFirstVisiblePresentation() throws
   {
      _ = NSApplication.shared
      let loader = BenchmarkSpecLoader(root: try productionSpecRoot())
      let window = productionComparisonWindow()
      let adapter = AppKitProductionScenarioAdapter(window: window)
      let scenario = try loader.loadScenario(relativePath: "scenarios/image.decode-zoom.json")
      try adapter.prepare(scenario: scenario, loader: loader)
      guard let canvas = productionDescendants(of: window.contentView).compactMap({$0 as? AppKitProductionImageCanvas}).first else
      {
         return XCTFail("missing image canvas")
      }
      let decode = try XCTUnwrap(try loader.loadTrace(try XCTUnwrap(scenario.phases.first(where: {$0.id == "decode"})?.trace)).first)
      let upload = try XCTUnwrap(try loader.loadTrace(try XCTUnwrap(scenario.phases.first(where: {$0.id == "upload"})?.trace)).first)
      let firstVisible = try XCTUnwrap(try loader.loadTrace(try XCTUnwrap(scenario.phases.first(where: {$0.id == "first-visible"})?.trace)).first)
      let initialPresentationPreparations = canvas.presentationPreparationCount

      try adapter.apply(event: decode)
      XCTAssertEqual(canvas.uploadPreparationCount, 0)
      try adapter.apply(event: upload)
      XCTAssertEqual(canvas.uploadPreparationCount, 1)
      XCTAssertEqual(canvas.uploadedTextureBytes, 2_048 * 1_536 * 4)
      XCTAssertEqual(canvas.presentationPreparationCount, initialPresentationPreparations)
      try adapter.apply(event: firstVisible)
      XCTAssertEqual(canvas.uploadPreparationCount, 1)
      XCTAssertEqual(canvas.presentationPreparationCount, initialPresentationPreparations)

      try adapter.teardown()
      window.close()
   }

   func testFeedAndChatEventsUseIncrementalNativeTableMutations() throws
   {
      _ = NSApplication.shared
      let loader = BenchmarkSpecLoader(root: try productionSpecRoot())

      let feedWindow = productionComparisonWindow()
      let feedAdapter = AppKitProductionScenarioAdapter(window: feedWindow)
      let feedScenario = try loader.loadScenario(relativePath: "scenarios/feed.variable-scroll.json")
      try feedAdapter.prepare(scenario: feedScenario, loader: loader)
      let feedTable = try XCTUnwrap(productionDescendants(of: feedWindow.contentView).first {$0.identifier?.rawValue == "feed.table"} as? NSTableView)
      let feedController = try XCTUnwrap(feedTable.dataSource as? AppKitFeedTableController)
      let feedReloads = feedController.fullReloadCount
      let prepend = try XCTUnwrap(try loader.loadTrace(try XCTUnwrap(feedScenario.phases.first(where: {$0.id == "prepend-20"})?.trace)).first)
      try feedAdapter.apply(event: prepend)
      XCTAssertEqual(feedController.fullReloadCount, feedReloads)
      XCTAssertEqual(feedController.incrementalMutationCount, 1)
      XCTAssertEqual(feedController.linearComparisonCount, 0)
      try feedAdapter.teardown()
      feedWindow.close()

      let chatWindow = productionComparisonWindow()
      let chatAdapter = AppKitProductionScenarioAdapter(window: chatWindow)
      let chatScenario = try loader.loadScenario(relativePath: "scenarios/chat.live-update.json")
      try chatAdapter.prepare(scenario: chatScenario, loader: loader)
      let chatTable = try XCTUnwrap(productionDescendants(of: chatWindow.contentView).first {$0.identifier?.rawValue == "chat.thread"} as? NSTableView)
      let chatController = try XCTUnwrap(chatTable.dataSource as? AppKitChatTableController)
      let chatReloads = chatController.fullReloadCount
      let append = try XCTUnwrap(try loader.loadTrace(try XCTUnwrap(chatScenario.phases.first(where: {$0.id == "append-10hz"})?.trace)).first)
      try chatAdapter.apply(event: append)
      XCTAssertEqual(chatController.fullReloadCount, chatReloads)
      XCTAssertEqual(chatController.incrementalMutationCount, 1)
      XCTAssertEqual(chatController.linearComparisonCount, 0)
      try chatAdapter.teardown()
      chatWindow.close()
   }

   func testMeasuredAdapterSourceContainsNoSynchronousLayoutOrDisplayForcing() throws
   {
      let adapterSource = try String(contentsOf: productionAdapterSourceURL(), encoding: .utf8)
      let sceneSource = try String(contentsOf: productionSceneSourceURL(), encoding: .utf8)
      let apply = try sourceSlice(adapterSource, from: "   func apply(event: BenchmarkTraceEvent) throws", through: "\n   func checkpoint")
      let displayTick = try sourceSlice(adapterSource, from: "   func displayTick() throws", through: "\n   func setVirtualTimeUs")
      let quiesce = try sourceSlice(adapterSource, from: "   func quiesce() throws", through: "\n   func drainRetiredResources")
      let feedScroll = try sourceSlice(sceneSource, from: "   func applyScrollOffset(_ offset: CGFloat)", through: "\n   func teardown()")

      for measuredPath in [apply, displayTick, feedScroll]
      {
         XCTAssertFalse(measuredPath.contains("layoutSubtreeIfNeeded"))
         XCTAssertFalse(measuredPath.contains("displayIfNeeded"))
      }
      XCTAssertFalse(apply.contains("model.apply(event: event)"))
      XCTAssertTrue(apply.contains("dispatchNativeUserInteraction"))
      XCTAssertTrue(quiesce.contains("layoutSubtreeIfNeeded"))
      XCTAssertTrue(quiesce.contains("displayIfNeeded"))
   }

   func testDashboardModelBuildsThirtyTwoNativeCardsWithAllFrozenLeaves() throws
   {
      let loader = BenchmarkSpecLoader(root: try productionSpecRoot())
      let scenario = try loader.loadScenario(relativePath: "scenarios/dashboard.mixed-static.json")
      let model = try AppKitProductionScenarioModel(scenario: scenario, fixtureData: try loader.read(scenario.fixture), loader: loader)
      let records = model.dashboardRecords
      XCTAssertEqual(records.count, 32)
      XCTAssertEqual(records.flatMap(\.labels).count, 176)
      XCTAssertEqual(records.filter {$0.actionTarget != nil}.count, 24)
      XCTAssertEqual(records.flatMap {$0.labels.map(\.id)}, (0..<176).map {String(format: "dashboard:label:%03d", $0)})
      XCTAssertTrue(records.allSatisfy {$0.leadingIcon.size == NSSize(width: 24, height: 24)})
      XCTAssertTrue(records.allSatisfy {$0.trailingIcon.size == NSSize(width: 24, height: 24)})
      XCTAssertEqual(records[0].font.pointSize, 7)
      XCTAssertFalse(records[0].font.fontName.localizedCaseInsensitiveContains("system"))
      model.teardown()
   }

   func testEnduranceModelExecutesExactChurnAndRestoresDashboardPresentation() throws
   {
      let loader = BenchmarkSpecLoader(root: try productionSpecRoot())
      let scenario = try loader.loadScenario(relativePath: "scenarios/endurance.churn.json")
      let model = try AppKitProductionScenarioModel(scenario: scenario, fixtureData: try loader.read(scenario.fixture), loader: loader)
      XCTAssertEqual(model.enduranceRecords.count, 32)
      XCTAssertEqual(model.enduranceRecords.flatMap(\.labels).count, 176)
      XCTAssertEqual(model.enduranceRecords.flatMap {$0.labels.map(\.id)}, (0..<176).map {String(format: "endurance:label:%03d", $0)})

      let checkpoints = [
         "open-close-heavy-screen": "heavy-screen-recovered",
         "tab-switch-heavy": "tab-restored",
         "idle-animation": "animation-settled",
      ]
      for phaseID in ["open-close-heavy-screen", "tab-switch-heavy", "idle-animation"]
      {
         guard let trace = scenario.phases.first(where: {$0.id == phaseID})?.trace else
         {
            return XCTFail("missing endurance trace \(phaseID)")
         }
         for event in try loader.loadTrace(trace)
         {
            try model.apply(event: event)
         }
         let checkpointID = checkpoints[phaseID]!
         let expected = scenario.parityCheckpoints.first(where: {$0.id == checkpointID})!
         let actual = try benchmarkCheckpoint(
            scenario: scenario,
            checkpointID: checkpointID,
            model: model.state,
            visibleRoleCounts: model.roleCounts
         )
         try assertProductionJSONEqual(actual.state, try loader.read(expected.state), "endurance:\(checkpointID):state")
         try assertProductionJSONEqual(actual.accessibility, try loader.read(expected.accessibility), "endurance:\(checkpointID):accessibility")
      }
      XCTAssertEqual(model.state["open_close_cycle_count"] as? Int, 100)
      XCTAssertEqual(model.state["heavy_screen_transition_count"] as? Int, 200)
      XCTAssertEqual(model.state["tab_switch_count"] as? Int, 500)
      XCTAssertEqual(model.state["active_tab_index"] as? Int, 0)
      XCTAssertEqual(model.state["animation_frame_count"] as? Int, 600)
      XCTAssertEqual(model.state["animation_frame_index"] as? Int, 600)
      XCTAssertEqual(model.enduranceRecords.map {$0.labels.map(\.value)}, model.dashboardRecords.map {$0.labels.map(\.value)})
      XCTAssertTrue(model.enduranceRecords.flatMap(\.labels).allSatisfy {!$0.accent})
      let recovered = scenario.parityCheckpoints.first(where: {$0.id == "recovered"})!
      let recoveredActual = try benchmarkCheckpoint(
         scenario: scenario,
         checkpointID: "recovered",
         model: model.state,
         visibleRoleCounts: model.roleCounts
      )
      try assertProductionJSONEqual(recoveredActual.state, try loader.read(recovered.state), "endurance:recovered:state")
      try assertProductionJSONEqual(recoveredActual.accessibility, try loader.read(recovered.accessibility), "endurance:recovered:accessibility")
      model.teardown()
   }

   func testFeedModelBuildsFrozenThumbnailsAndLanguageFonts() throws
   {
      let loader = BenchmarkSpecLoader(root: try productionSpecRoot())
      let scenario = try loader.loadScenario(relativePath: "scenarios/feed.variable-scroll.json")
      let model = try AppKitProductionScenarioModel(scenario: scenario, fixtureData: try loader.read(scenario.fixture), loader: loader)
      let records = model.feedRecords
      XCTAssertEqual(records.count, 2_000)
      XCTAssertEqual(records[0].id, "feed:item:0000")
      XCTAssertEqual(records[0].height, 68)
      XCTAssertTrue(records[0].thumbnail === model.assets.atlasTiles[0])
      XCTAssertTrue(records[1].thumbnail === model.assets.atlasTiles[1])
      XCTAssertEqual(records[0].titleFont.pointSize, 15)
      XCTAssertEqual(records[1].titleFont.fontName, model.fonts.arabic15.fontName)
      XCTAssertEqual(records[2].titleFont.fontName, model.fonts.cjk15.fontName)
      XCTAssertEqual(records[0].secondaryFont.fontName, model.fonts.latin11.fontName)
      XCTAssertNotNil(model.assets.inlineText)
      XCTAssertTrue(records.allSatisfy {$0.inlineText === model.assets.inlineText})
      guard let status = records.first(where: {$0.title.hasPrefix("Status icons:")}) else
      {
         return XCTFail("missing inline status row")
      }
      let field = AppKitProductionExactTextField(frame: CGRect(x: 0, y: 0, width: 254, height: 24))
      field.configure(identifier: "feed.status", value: status.title, font: status.titleFont, color: AppKitProductionPalette.text, inlineText: status.inlineText)
      XCTAssertEqual(field.inlineImageCount, 4)
      let bitmap = try XCTUnwrap(field.bitmapImageRepForCachingDisplay(in: field.bounds))
      field.cacheDisplay(in: field.bounds, to: bitmap)
      let pixelScale = CGFloat(bitmap.pixelsWide) / field.bounds.width
      let inlineStart = Int(("Status icons: " as NSString).size(withAttributes: [.font: status.titleFont]).width * pixelScale)
      let hasVisibleInlinePixel = (0..<bitmap.pixelsHigh).contains
      {
         y in
         (inlineStart..<bitmap.pixelsWide).contains
         {
            x in
            guard let color = bitmap.colorAt(x: x, y: y)?.usingColorSpace(.sRGB),
                  color.alphaComponent > 0 else {return false}
            return min(color.redComponent, color.greenComponent, color.blueComponent) < 0.8
         }
      }
      XCTAssertTrue(hasVisibleInlinePixel)
      XCTAssertFalse(records[0].favorite)
      model.teardown()
   }

   func testFeedNativeViewportUsesTheFrozenVariableHeightOffset() throws
   {
      _ = NSApplication.shared
      let loader = BenchmarkSpecLoader(root: try productionSpecRoot())
      let scenario = try loader.loadScenario(relativePath: "scenarios/feed.variable-scroll.json")
      let model = try AppKitProductionScenarioModel(scenario: scenario, fixtureData: try loader.read(scenario.fixture), loader: loader)
      guard let trace = scenario.phases.first(where: {$0.id == "forward-fling"})?.trace else
      {
         return XCTFail("missing forward feed trace")
      }
      for event in try loader.loadTrace(trace)
      {
         try model.apply(event: event)
      }

      let window = productionComparisonWindow()
      let host = AppKitProductionSceneHost()
      let feed = AppKitFeedProductionSceneRoot(headingFont: model.fonts.latin20)
      host.install(feed)
      window.contentViewController = host
      window.setContentSize(NSSize(width: 390, height: 844))
      window.makeKeyAndOrderFront(nil)
      window.contentView?.layoutSubtreeIfNeeded()
      host.view.layoutSubtreeIfNeeded()
      feed.replaceRows(model.feedRecords)
      feed.applyScrollOffset(model.feedScrollOffset)

      XCTAssertEqual(model.feedScrollOffset, 140_427.75, accuracy: 0.001)
      XCTAssertEqual(feed.table.tableColumns[0].width, 390)
      XCTAssertEqual(feed.table.intercellSpacing, .zero)
      XCTAssertEqual(feed.table.rowSizeStyle, .custom)
      XCTAssertFalse(feed.table.usesAutomaticRowHeights)
      XCTAssertEqual(feed.tableController.records.reduce(CGFloat(0)) {$0 + $1.height}, 188_029)
      for row in [0, 1, 1_492, 1_493, 1_999]
      {
         XCTAssertEqual(feed.table.rect(ofRow: row).height, feed.tableController.records[row].height, accuracy: 0.001, "row \(row): \(feed.table.rect(ofRow: row))")
      }
      XCTAssertEqual(feed.table.frame, CGRect(x: 0, y: 0, width: 390, height: 188_029))
      XCTAssertEqual(feed.scrollView.contentView.bounds.origin.y, model.feedScrollOffset, accuracy: 0.001)
      XCTAssertEqual(feed.table.visibleRect.origin.y, model.feedScrollOffset, accuracy: 0.001)
      XCTAssertEqual(feed.table.rect(ofRow: 1_493).minY, 140_349, accuracy: 0.001)
      XCTAssertEqual(feed.table.rows(in: feed.table.visibleRect).location, 1_493)
      guard let cell = feed.table.view(atColumn: 0, row: 1_493, makeIfNecessary: false) as? AppKitFeedTableCellView else
      {
         return XCTFail("missing first visible feed row")
      }
      cell.layoutSubtreeIfNeeded()
      let windowCardY = cell.convert(CGPoint(x: 0, y: 4 + cell.canonicalContentYOffset), to: nil).y
      XCTAssertEqual(windowCardY * 3, (windowCardY * 3).rounded(), accuracy: 0.000_001)
      let windowBaseline = cell.titleField.convert(CGPoint(x: 0, y: cell.titleField.resolvedBaseline()), to: nil).y
      XCTAssertEqual(windowBaseline * 3, (windowBaseline * 3).rounded(), accuracy: 0.000_001)

      host.uninstall()
      window.close()
      model.teardown()
   }

   func testImageModelOwnsAndExplicitlyReleasesDecodedImageIOCache() throws
   {
      let loader = BenchmarkSpecLoader(root: try productionSpecRoot())
      let scenario = try loader.loadScenario(relativePath: "scenarios/image.decode-zoom.json")
      let model = try AppKitProductionScenarioModel(scenario: scenario, fixtureData: try loader.read(scenario.fixture), loader: loader)
      _ = try model.assets.decodedSource()
      XCTAssertTrue(model.assets.hasRetainedSource)
      XCTAssertTrue(model.assets.hasRetainedSourceCache)
      model.teardown()
      XCTAssertFalse(model.assets.hasRetainedSource)
      XCTAssertFalse(model.assets.hasRetainedSourceCache)
   }

   func testFullFeedScheduleReleasesNativeSceneModelAndCaptureSurface() throws
   {
      _ = NSApplication.shared
      let loader = BenchmarkSpecLoader(root: try productionSpecRoot())
      let window = productionComparisonWindow()
      let adapter = AppKitProductionScenarioAdapter(window: window)
      let scenario = try loader.loadScenario(relativePath: "scenarios/feed.variable-scroll.json")
      try autoreleasepool
      {
         try adapter.prepare(scenario: scenario, loader: loader)
         var schedule = try BenchmarkPhaseSchedule(scenario: scenario, setupDurationUs: 0, loader: loader)
         try schedule.drainTimed(until: UInt64.max)
         {
            timeUs, action in
            try adapter.setVirtualTimeUs(timeUs)
            switch action
            {
            case .traceEvent(let event): try adapter.apply(event: event)
            case .checkpoint: _ = try adapter.previewPNG()
            default: break
            }
         }
         XCTAssertTrue(schedule.isComplete)
         try adapter.teardown()
      }
      try adapter.drainRetiredResources()
      XCTAssertTrue(adapter.hasPreviewSurface)
      XCTAssertEqual(adapter.previewSurfaceAllocationCount, 1)
      XCTAssertEqual(adapter.retiredResourceKinds, [])
      XCTAssertGreaterThanOrEqual(adapter.retiredResourceDrainTurnCount, 6)
      window.close()
   }

   func testRetiredResourceDrainRunsMinimumAppKitTurnsWithoutALeakSentinel() throws
   {
      _ = NSApplication.shared
      let window = productionComparisonWindow()
      let adapter = AppKitProductionScenarioAdapter(window: window)
      XCTAssertEqual(adapter.retiredResourceKinds, [])
      try adapter.drainRetiredResources()
      XCTAssertEqual(adapter.retiredResourceDrainTurnCount, 6)
      window.close()
   }

   func testReleaseCandidateModelsReplayTheirFrozenFinalStates() throws
   {
      let root = try productionSpecRoot()
      let loader = BenchmarkSpecLoader(root: root)
      let cases = [
         ("grid.large-scroll", ["grid-scroll-to-75-percent.json", "grid-reverse-scroll.json", "grid-select-detail-back.json"], "restored"),
         ("effects.layers", ["effects-cold-build.json", "effects-warm-animation.json", "effects-dirty-layer.json"], "dirty-layer"),
         ("mutation.damage", ["mutation-1-percent.json", "mutation-10-percent.json", "mutation-100-percent.json"], "mutated-100-percent"),
         ("text.multilingual", ["text-font-atlas-cold.json", "text-warm-replay.json", "text-scale-wrap-change.json"], "scale-wrap-changed"),
         ("resize.theme", ["resize-theme-ten-changes.json"], "change-10"),
      ]
      for (scenarioID, traceNames, checkpointID) in cases
      {
         let scenario = try releaseCandidateScenario(scenarioID, root: root)
         let model = try AppKitProductionScenarioModel(scenario: scenario, fixtureData: try loader.read(scenario.fixture), loader: loader)
         for traceName in traceNames
         {
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = .convertFromSnakeCase
            let events = try decoder.decode([BenchmarkTraceEvent].self, from: Data(contentsOf: root.appendingPathComponent("traces/\(traceName)")))
            for event in events {try model.apply(event: event)}
         }
         let expected = try JSONSerialization.jsonObject(with: Data(contentsOf: root.appendingPathComponent("checkpoints/\(scenarioID)/\(checkpointID)/state.json"))) as! [String: Any]
         XCTAssertEqual(model.state as NSDictionary, expected["model"] as? NSDictionary, scenarioID)
         let expectedRoles = try JSONDecoder().decode([BenchmarkRoleCount].self, from: JSONSerialization.data(withJSONObject: expected["visible_role_counts"]!))
         XCTAssertEqual(model.roleCounts, expectedRoles, scenarioID)
         model.teardown()
      }
   }

   func testReleaseScenesExecuteDeclaredNativeWorkloads() throws
   {
      _ = NSApplication.shared
      let root = try productionSpecRoot()
      let loader = BenchmarkSpecLoader(root: root)

      let gridModel = try AppKitProductionScenarioModel(
         scenario: releaseCandidateScenario("grid.large-scroll", root: root),
         fixtureData: Data(contentsOf: root.appendingPathComponent("fixtures/grid.large-scroll.json")),
         loader: loader
      )
      XCTAssertEqual(gridModel.gridRecords.count, 10_000)
      XCTAssertEqual(Set(gridModel.gridRecords.prefix(256).map {ObjectIdentifier($0.image)}).count, 256)
      let grid = AppKitGridProductionSceneRoot()
      grid.present(records: gridModel.gridRecords, scrollMillionths: 0, detailRecord: nil)
      grid.present(records: gridModel.gridRecords, scrollMillionths: 750_000, detailRecord: nil)
      XCTAssertEqual(grid.controller.records.count, 10_000)
      XCTAssertEqual(grid.controller.reloadCount, 1)
      XCTAssertEqual(grid.columnCount, 3)
      grid.scrollView.frame.size.width = 844
      XCTAssertEqual(grid.columnCount, 6)
      let actionTarget = AppKitReleaseActionTarget()
      let item = AppKitGridCollectionItem()
      item.configure(gridModel.gridRecords[7_500], target: actionTarget, action: #selector(AppKitReleaseActionTarget.invoke(_:)))
      XCTAssertTrue(item.actionButton.sendAction(item.actionButton.action, to: item.actionButton.target))
      XCTAssertEqual(actionTarget.invocationCount, 1)
      var backInvoked = false
      grid.onBack = {backInvoked = true}
      XCTAssertTrue(grid.back.sendAction(grid.back.action, to: grid.back.target))
      XCTAssertTrue(backInvoked)

      let effects = AppKitEffectsProductionSceneRoot()
      XCTAssertEqual(effects.cards.count, 100)
      XCTAssertEqual(effects.cards.filter {$0.layer?.shadowOpacity == 1}.count, 32)
      XCTAssertEqual(effects.backdrops.count, 8)
      XCTAssertTrue(effects.backdrops.allSatisfy {$0.isKind(of: NSVisualEffectView.self)})
      effects.present(progress: 0.5, dirtyGeneration: 1)
      XCTAssertEqual(effects.lastAnimatedCardCount, 100)
      XCTAssertEqual(effects.lastAnimatedBackdropCount, 8)
      XCTAssertEqual(effects.dirtyInvalidationCount, 1)

      let mutation = AppKitMutationProductionSceneRoot()
      let onePercent = IndexSet(0..<100)
      mutation.present(changed: onePercent, generation: 1)
      XCTAssertEqual(mutation.nodes.count, 10_000)
      XCTAssertEqual(mutation.lastSelectedNodeCount, 100)
      XCTAssertEqual(mutation.lastLayerWriteCount, 100)
      mutation.present(changed: IndexSet(100..<200), generation: 2)
      XCTAssertEqual(mutation.lastLayerWriteCount, 200)

      let textModel = try AppKitProductionScenarioModel(
         scenario: releaseCandidateScenario("text.multilingual", root: root),
         fixtureData: Data(contentsOf: root.appendingPathComponent("fixtures/text.multilingual.json")),
         loader: loader
      )
      XCTAssertEqual(textModel.textRecords.count, 1_000)
      XCTAssertEqual(Dictionary(grouping: textModel.textRecords, by: \.categoryID).mapValues(\.count), [
         "latin": 200,
         "cjk": 200,
         "arabic-rtl": 200,
         "emoji": 200,
         "fallback": 200,
      ])
      let text = AppKitTextProductionSceneRoot(headingFont: NSFont.systemFont(ofSize: 20))
      text.present(records: textModel.textRecords, wrapWidth: 96, scale: 1)
      XCTAssertEqual(text.renderedStrings.count, 1_000)
      XCTAssertEqual(text.fullReloadCount, 1)
      var attachmentCount = 0
      text.renderedStrings.forEach
      {
         $0.enumerateAttribute(.attachment, in: NSRange(location: 0, length: $0.length))
         {
            value, _, _ in
            if value != nil {attachmentCount += 1}
         }
      }
      XCTAssertGreaterThan(attachmentCount, 0)
      text.present(records: textModel.textRecords, wrapWidth: 144, scale: 1.25)
      XCTAssertEqual(text.renderedStrings.count, 1_000)
      XCTAssertEqual(text.fullReloadCount, 1)

      let resize = AppKitResizeProductionSceneRoot()
      XCTAssertEqual(resize.backdrops.count, 4)
      resize.present(records: [], orientation: "landscape", theme: "dark")
      XCTAssertEqual(resize.storage.rootView.appearance?.name, .darkAqua)
      XCTAssertEqual(resize.scrollView.frame.size, NSSize(width: 796, height: 318))
      resize.present(records: [], orientation: "portrait", theme: "light")
      XCTAssertEqual(resize.storage.rootView.appearance?.name, .aqua)
      XCTAssertEqual(resize.scrollView.frame.size, NSSize(width: 358, height: 730))

      grid.teardown()
      gridModel.teardown()
      effects.teardown()
      mutation.teardown()
      text.teardown()
      textModel.teardown()
      resize.teardown()
   }

   func testRawAccessibilityEvidenceComesFromNativeAppKitObjects() throws
   {
      _ = NSApplication.shared
      let loader = BenchmarkSpecLoader(root: try productionSpecRoot())
      let window = productionComparisonWindow()
      let adapter = AppKitProductionScenarioAdapter(window: window)
      let scenario = try loader.loadScenario(relativePath: "scenarios/chat.live-update.json")
      try adapter.prepare(scenario: scenario, loader: loader)
      let raw = try adapter.rawAccessibilityTree()
      guard let tree = try JSONSerialization.jsonObject(with: raw) as? [String: Any],
            let nodes = tree["nodes"] as? [[String: Any]] else
      {
         return XCTFail("invalid raw AppKit accessibility evidence")
      }
      XCTAssertEqual(tree["platform"] as? String, "appkit")
      XCTAssertEqual(tree["source"] as? String, "NSView/NSAccessibility")
      XCTAssertTrue(nodes.contains(where: {$0["class"] as? String == NSStringFromClass(AppKitOwnedTextView.self)}))
      XCTAssertTrue(nodes.contains(where: {$0["class"] as? String == NSStringFromClass(NSButtonCell.self)}))
      XCTAssertTrue(nodes.allSatisfy {($0["object_identity"] as? String)?.isEmpty == false})
      try adapter.teardown()
      window.close()
   }

   func testProductionApplicationTargetPhysicallyExcludesDiagnosticCanvasSource() throws
   {
      let project = try String(contentsOf: productionProjectSpecURL(), encoding: .utf8)
      guard let appKitTarget = project.range(of: "  AppKitBenchMacOS:\n    type: application\n"),
            let oxideTarget = project.range(of: "  OxideBenchMacOS:\n    type: application\n", range: appKitTarget.upperBound..<project.endIndex) else
      {
         return XCTFail("missing explicit macOS targets")
      }
      let sourceGraph = String(project[appKitTarget.upperBound..<oxideTarget.lowerBound])
      XCTAssertTrue(sourceGraph.contains("AppKitProductionScenarioAdapter.swift"))
      XCTAssertTrue(sourceGraph.contains("AppKitProductionReferenceScenarioModel.swift"))
      XCTAssertFalse(sourceGraph.contains("AppKitScenarioAdapter.swift"))
      XCTAssertFalse(sourceGraph.contains("- path: AppKit-macOS\n"))
   }

   private func productionComparisonWindow() -> NSWindow
   {
      let window = NSWindow(
         contentRect: NSRect(x: 0, y: 0, width: 390, height: 844),
         styleMask: [.borderless],
         backing: .buffered,
         defer: false
      )
      window.isReleasedWhenClosed = false
      return window
   }

   private func productionInteractiveWindow() -> NSWindow
   {
      let window = AppKitProductionInteractiveTestWindow(
         contentRect: NSRect(x: 0, y: 0, width: 390, height: 844),
         styleMask: [.borderless],
         backing: .buffered,
         defer: false
      )
      window.isReleasedWhenClosed = false
      return window
   }

   private func productionDescendants(of view: NSView?) -> [NSView]
   {
      guard let view else {return []}
      return [view] + view.subviews.flatMap {productionDescendants(of: $0)}
   }

   private func productionMouseEvent(type: NSEvent.EventType, window: NSWindow, view: NSView, point: NSPoint, timestamp: TimeInterval, eventNumber: Int) throws -> NSEvent
   {
      try XCTUnwrap(NSEvent.mouseEvent(
         with: type,
         location: view.convert(point, to: nil),
         modifierFlags: [],
         timestamp: timestamp,
         windowNumber: window.windowNumber,
         context: nil,
         eventNumber: eventNumber,
         clickCount: 1,
         pressure: type == .leftMouseUp ? 0 : 1
      ))
   }

   private func assertProductionJSONEqual(_ actual: Data, _ expected: Data, _ context: String) throws
   {
      let actualObject = try JSONSerialization.jsonObject(with: actual) as? NSDictionary
      let expectedObject = try JSONSerialization.jsonObject(with: expected) as? NSDictionary
      XCTAssertEqual(actualObject, expectedObject, context)
   }

   private func productionProjectSpecURL() -> URL
   {
      URL(fileURLWithPath: #filePath)
         .deletingLastPathComponent()
         .deletingLastPathComponent()
         .appendingPathComponent("project.yml")
   }

   private func productionAdapterSourceURL() -> URL
   {
      productionProjectSpecURL()
         .deletingLastPathComponent()
         .appendingPathComponent("AppKit-macOS/AppKitProductionScenarioAdapter.swift")
   }

   private func productionSceneSourceURL() -> URL
   {
      productionProjectSpecURL()
         .deletingLastPathComponent()
         .appendingPathComponent("AppKit-macOS/AppKitProductionReferenceScenes.swift")
   }

   private func sourceSlice(_ source: String, from start: String, through end: String) throws -> String
   {
      guard let lower = source.range(of: start),
            let upper = source.range(of: end, range: lower.upperBound..<source.endIndex) else
      {
         throw AppKitProductionScenarioFailure.invalidFixture
      }
      return String(source[lower.lowerBound..<upper.lowerBound])
   }

   private func releaseCandidateScenario(_ id: String, root: URL) throws -> BenchmarkScenario
   {
      let baseURL = root.appendingPathComponent("scenarios/dashboard.mixed-static.json")
      let candidateURL = root.appendingPathComponent("release-candidates/\(id).candidate.json")
      var base = try JSONSerialization.jsonObject(with: Data(contentsOf: baseURL)) as! [String: Any]
      let candidate = try JSONSerialization.jsonObject(with: Data(contentsOf: candidateURL)) as! [String: Any]
      base["id"] = id
      base["fixture"] = candidate["fixture"]
      base["scene"] = candidate["scene"]
      base["primary_metric"] = candidate["primary_metric"]
      base["phases"] = candidate["phases"]
      let decoder = JSONDecoder()
      decoder.keyDecodingStrategy = .convertFromSnakeCase
      return try decoder.decode(BenchmarkScenario.self, from: JSONSerialization.data(withJSONObject: base, options: [.sortedKeys]))
   }

   private func productionSpecRoot() throws -> URL
   {
      let root = URL(fileURLWithPath: #filePath)
         .deletingLastPathComponent()
         .deletingLastPathComponent()
         .deletingLastPathComponent()
         .deletingLastPathComponent()
         .appendingPathComponent("benchmarks/comparative/specs/v1", isDirectory: true)
      guard FileManager.default.fileExists(atPath: root.path) else
      {
         throw AppKitProductionScenarioFailure.invalidFixture
      }
      return root
   }
}

private func expectedInitialSemanticIdentifiers(scenarioID: String) -> Set<String>
{
   switch scenarioID
   {
   case "startup.first-screen":
      return Set(["startup:header", "startup:navigation", "startup:primary-control"]
         + (0..<6).flatMap
         {
            index in
            let card = String(format: "startup:card:%02d", index)
            return [card, "\(card):thumbnail"]
         })
   case "dashboard.mixed-static":
      return Set(["dashboard"]
         + (0..<4).map {"dashboard:backdrop:\($0)"}
         + (0..<32).map {String(format: "dashboard:card:%02d", $0)}
         + (0..<64).map {String(format: "dashboard:icon:%03d", $0)}
         + (0..<176).map {String(format: "dashboard:label:%03d", $0)}
         + (0..<24).map {String(format: "dashboard:control:%02d", $0)})
   case "feed.variable-scroll":
      return Set(["feed.navigation-bar", "feed.table"]
         + (0..<9).flatMap
         {
            index in
            let row = String(format: "feed:item:%04d", index)
            return [row, "\(row):thumbnail", "\(row):favorite"]
         })
   case "chat.live-update":
      return Set(["chat.thread", "chat.composer", "chat.send"]
         + (4_990..<5_000).flatMap
         {
            index in
            let message = String(format: "chat:message:%04d", index)
            return [message, "\(message):avatar"]
         })
   case "navigation.modal":
      return Set(["navigation.table"] + (1...12).map {String(format: "navigation:item:%02d", $0)})
   case "image.decode-zoom":
      return Set(["image.canvas", "image.thumbnail", "image.zoom"])
   case "grid.large-scroll":
      return Set(["grid.collection"]
         + (0..<18).flatMap
         {
            index in
            return [
               String(format: "grid:tile:%05d", index),
               String(format: "grid:thumbnail:%05d", index),
               String(format: "grid:label:%05d", index),
            ]
         })
   case "effects.layers":
      return Set(["effects.scene"]
         + (0..<100).map {String(format: "effects:card:%03d", $0)}
         + (0..<100).map {String(format: "effects:clip:%03d", $0)}
         + (0..<32).map {String(format: "effects:shadow:%03d", $0)}
         + [5, 17, 29, 41, 53, 65, 77, 89].map {String(format: "effects:backdrop:%03d", $0)})
   case "mutation.damage":
      return Set(["mutation.surface"] + (0..<10_000).map {String(format: "mutation:node:%05d", $0)})
   case "text.multilingual":
      return Set(["text.surface"] + (0..<12).map {String(format: "text:label:%03d", $0)})
   case "resize.theme":
      return Set(["dashboard"]
         + (0..<4).map {"dashboard:backdrop:\($0)"}
         + (0..<32).map {String(format: "dashboard:card:%02d", $0)}
         + (0..<64).map {String(format: "dashboard:icon:%03d", $0)}
         + (0..<176).map {String(format: "dashboard:label:%03d", $0)}
         + (0..<24).map {String(format: "dashboard:control:%02d", $0)})
   default:
      return []
   }
}

private final class AppKitProductionInteractiveTestWindow: NSWindow
{
   override var canBecomeKey: Bool {true}
   override var canBecomeMain: Bool {true}
}

private final class AppKitReleaseActionTarget: NSObject
{
   private(set) var invocationCount = 0

   @objc func invoke(_ sender: NSButton)
   {
      invocationCount += 1
   }
}
