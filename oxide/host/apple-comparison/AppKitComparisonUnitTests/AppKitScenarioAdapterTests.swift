import AppKit
import CryptoKit
import XCTest

final class AppKitScenarioAdapterTests: XCTestCase
{
   private let scenarioIDs = [
      "startup.first-screen",
      "dashboard.mixed-static",
      "feed.variable-scroll",
      "chat.live-update",
      "navigation.modal",
      "image.decode-zoom",
   ]

   func testAllFrozenScenariosPrepareResetAndCaptureOpaqueThreeXPreview() throws
   {
      _ = NSApplication.shared
      let loader = BenchmarkSpecLoader(root: try specRoot())
      for scenarioID in scenarioIDs
      {
         let window = comparisonWindow()
         let adapter = AppKitScenarioAdapter(window: window)
         let scenario = try loader.loadScenario(relativePath: "scenarios/\(scenarioID).json")
         try adapter.prepare(scenario: scenario, loader: loader)
         try adapter.reset()
         let png = try adapter.previewPNG()
         guard let representation = NSBitmapImageRep(data: png) else
         {
            return XCTFail("missing PNG for \(scenarioID)")
         }
         XCTAssertEqual(representation.pixelsWide, 1_170, scenarioID)
         XCTAssertEqual(representation.pixelsHigh, 2_532, scenarioID)
         XCTAssertFalse(representation.hasAlpha, scenarioID)
         try adapter.teardown()
      }
   }

   func testRepresentativeTransitionsMatchFrozenSemanticModelsAndResetInPlace() throws
   {
      _ = NSApplication.shared
      let loader = BenchmarkSpecLoader(root: try specRoot())
      let cases = [
         ("startup.first-screen", ["startup-terminated-warm-cache.json"], "terminated-ready"),
         ("dashboard.mixed-static", ["dashboard-leaf-updates.json"], "leaf-updated"),
         ("feed.variable-scroll", ["feed-forward-fling.json"], "mid-forward"),
         ("chat.live-update", ["chat-prepend-50.json"], "prepended-50"),
         ("navigation.modal", ["navigation-canonical-cycles.json"], "list-restored"),
         ("image.decode-zoom", ["image-bytes-ready.json", "image-decode.json", "image-upload.json", "image-first-visible.json"], "first-visible"),
      ]
      for (scenarioID, traceNames, checkpointID) in cases
      {
         let window = comparisonWindow()
         let adapter = AppKitScenarioAdapter(window: window)
         let scenario = try loader.loadScenario(relativePath: "scenarios/\(scenarioID).json")
         try adapter.prepare(scenario: scenario, loader: loader)
         let checkpoint = scenario.parityCheckpoints.first(where: {$0.id == checkpointID})
         XCTAssertNotNil(checkpoint, checkpointID)
         let cutoff = checkpoint?.atUs ?? UInt64.max
         for traceName in traceNames
         {
            for event in try decodedTrace(named: traceName, loader: loader) where event.atUs <= cutoff
            {
               try adapter.apply(event: event)
            }
         }
         let actual = try adapter.checkpoint(id: checkpointID)
         try assertJSONEqual(actual.state, try loader.read(checkpoint!.state), "\(scenarioID):\(checkpointID)")
         try adapter.reset()
         try adapter.quiesce()
         try adapter.teardown()
      }
   }

   func testFeedVisibleCountsComeFromFrozenGeometry() throws
   {
      _ = NSApplication.shared
      let loader = BenchmarkSpecLoader(root: try specRoot())
      let window = comparisonWindow()
      let adapter = AppKitScenarioAdapter(window: window)
      let scenario = try loader.loadScenario(relativePath: "scenarios/feed.variable-scroll.json")
      try adapter.prepare(scenario: scenario, loader: loader)
      let initial = try adapter.checkpoint(id: "initial")
      XCTAssertEqual(initial.visibleRoleCounts.first(where: {$0.role == "feed-card"})?.count, 9)
      for event in try decodedTrace(named: "feed-forward-fling.json", loader: loader)
      {
         try adapter.apply(event: event)
      }
      let forward = try adapter.checkpoint(id: "mid-forward")
      XCTAssertEqual(forward.visibleRoleCounts.first(where: {$0.role == "feed-card"})?.count, 10)
   }

   func testRawPlatformAccessibilityTreeIsRetainedFromAppKitElements() throws
   {
      _ = NSApplication.shared
      let loader = BenchmarkSpecLoader(root: try specRoot())
      let adapter = AppKitScenarioAdapter(window: comparisonWindow())
      let scenario = try loader.loadScenario(relativePath: "scenarios/navigation.modal.json")
      try adapter.prepare(scenario: scenario, loader: loader)
      let raw = try adapter.rawAccessibilityTree()
      guard let tree = try JSONSerialization.jsonObject(with: raw) as? [String: Any],
            let nodes = tree["nodes"] as? [[String: Any]] else
      {
         return XCTFail("invalid raw AppKit accessibility tree")
      }
      XCTAssertEqual(tree["platform"] as? String, "appkit")
      XCTAssertEqual(tree["source"] as? String, "NSAccessibilityElement")
      XCTAssertEqual(nodes.count, 13)
      XCTAssertEqual(nodes.filter({$0["role"] as? String == NSAccessibility.Role.button.rawValue}).count, 12)
      try adapter.teardown()
   }

   func testEveryScheduledCheckpointMatchesFrozenStateAndAccessibility() throws
   {
      _ = NSApplication.shared
      let loader = BenchmarkSpecLoader(root: try specRoot())
      for scenarioID in scenarioIDs
      {
         let adapter = AppKitScenarioAdapter(window: comparisonWindow())
         let scenario = try loader.loadScenario(relativePath: "scenarios/\(scenarioID).json")
         try adapter.prepare(scenario: scenario, loader: loader)
         var schedule = try BenchmarkPhaseSchedule(scenario: scenario, setupDurationUs: 0, loader: loader)
         try schedule.drain(until: UInt64.max)
         {
            action in
            switch action
            {
            case .traceEvent(let event): try adapter.apply(event: event)
            case .checkpoint(let checkpointID):
               guard let expected = scenario.parityCheckpoints.first(where: {$0.id == checkpointID}) else
               {
                  return XCTFail("missing checkpoint \(scenarioID):\(checkpointID)")
               }
               let actual = try adapter.checkpoint(id: checkpointID)
               try assertJSONEqual(actual.state, try loader.read(expected.state), "\(scenarioID):\(checkpointID):state")
               try assertJSONEqual(actual.accessibility, try loader.read(expected.accessibility), "\(scenarioID):\(checkpointID):accessibility")
            default: break
            }
         }
         XCTAssertTrue(schedule.isComplete, scenarioID)
         try adapter.teardown()
      }
   }

   private func decodedTrace(named name: String, loader: BenchmarkSpecLoader) throws -> [BenchmarkTraceEvent]
   {
      let url = loader.root.appendingPathComponent("traces/\(name)")
      return try JSONDecoder.snakeCase.decode([BenchmarkTraceEvent].self, from: Data(contentsOf: url))
   }

   private func comparisonWindow() -> NSWindow
   {
      NSWindow(
         contentRect: NSRect(x: 0, y: 0, width: 390, height: 844),
         styleMask: [.borderless],
         backing: .buffered,
         defer: false
      )
   }

   private func assertJSONEqual(_ actual: Data, _ expected: Data, _ context: String) throws
   {
      let actualObject = try JSONSerialization.jsonObject(with: actual) as? NSDictionary
      let expectedObject = try JSONSerialization.jsonObject(with: expected) as? NSDictionary
      XCTAssertEqual(actualObject, expectedObject, context)
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
         throw AppKitScenarioAdapterFailure.invalidFixture
      }
      return root
   }
}

private extension JSONDecoder
{
   static var snakeCase: JSONDecoder
   {
      let decoder = JSONDecoder()
      decoder.keyDecodingStrategy = .convertFromSnakeCase
      return decoder
   }
}
