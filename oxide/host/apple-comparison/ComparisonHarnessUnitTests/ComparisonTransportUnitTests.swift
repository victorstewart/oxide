import CryptoKit
import CoreText
import Foundation
import XCTest

final class ComparisonTransportUnitTests: XCTestCase
{
   func testPreviewInvocationRequiresAValidatedGeneration() throws
   {
      let generation = String(repeating: "a", count: 64)
      XCTAssertEqual(
         try ComparisonPreviewInvocation.commandLine(arguments: [
            "OxideBenchIOS",
            "-oxide-compare-preview-generation", generation,
            "-oxide-compare-scenario", "feed.variable-scroll",
         ]),
         ComparisonPreviewInvocation(scenarioID: "feed.variable-scroll", generation: generation)
      )
      XCTAssertThrowsError(try ComparisonPreviewInvocation.commandLine(arguments: [
         "OxideBenchIOS",
         "-oxide-compare-scenario", "feed.variable-scroll",
      ]))
      XCTAssertThrowsError(try ComparisonPreviewInvocation.commandLine(arguments: [
         "OxideBenchIOS",
         "-oxide-compare-preview-generation", "stale",
         "-oxide-compare-scenario", "feed.variable-scroll",
      ]))
   }

   func testPreviewCompletionBindsGenerationAndPNGHash() throws
   {
      let complete = ComparisonPreviewComplete(
         schemaVersion: 1,
         generation: String(repeating: "b", count: 64),
         scenarioID: "feed.variable-scroll",
         bundleID: "com.oxide.comparison.oxidebenchios",
         fixtureSHA256: String(repeating: "c", count: 64),
         fontPackSHA256: String(repeating: "d", count: 64),
         previewSHA256: String(repeating: "e", count: 64),
         visibleRoleCounts: []
      )
      XCTAssertEqual(try JSONDecoder().decode(ComparisonPreviewComplete.self, from: JSONEncoder.comparisonCanonical.encode(complete)), complete)
   }

   func testTelemetryRingIsFixedCapacityContiguousAndFailClosed() throws
   {
      let ring = BenchmarkTelemetryRing(capacity: 2)
      ring.append(kind: .scenarioBegin, identifier: benchmarkStableID("dashboard.mixed-static"), timestamp: 10)
      ring.append(kind: .scenarioEnd, identifier: benchmarkStableID("dashboard.mixed-static"), timestamp: 20)
      try ring.validateComplete()
      XCTAssertEqual(ring.snapshot().map(\.sequence), [0, 1])
      XCTAssertEqual(ring.snapshot().map(\.timestamp), [10, 20])
      let binary = try ring.binarySnapshot(identity: telemetryIdentity())
      XCTAssertEqual(binary.count, BenchmarkTelemetryRing.binaryHeaderBytes + 2 * BenchmarkTelemetryRing.binaryRecordBytes + BenchmarkTelemetryRing.binaryFooterBytes)
      XCTAssertEqual(Array(binary.prefix(8)), Array("OXBTEL02".utf8))
      XCTAssertEqual(binary.withUnsafeBytes {$0.loadUnaligned(fromByteOffset: 24, as: UInt64.self)}, UInt64(2).littleEndian)
      XCTAssertEqual(Data(binary.dropLast(BenchmarkTelemetryRing.binaryFooterBytes)).withSHA256, Data(binary.suffix(BenchmarkTelemetryRing.binaryFooterBytes)))

      ring.append(kind: .phaseBegin, timestamp: 30)
      XCTAssertThrowsError(try ring.validateComplete())
      XCTAssertThrowsError(try ring.binarySnapshot(identity: telemetryIdentity()))
      XCTAssertTrue(ring.overflowed)
   }

   func testTelemetryRingResetReusesStorageWithoutStaleRecords() throws
   {
      let ring = BenchmarkTelemetryRing(capacity: 2)
      ring.append(kind: .scenarioBegin, identifier: 7, timestamp: 11)
      ring.reset()
      ring.append(kind: .scenarioBegin, identifier: 9, timestamp: 13)
      ring.append(kind: .scenarioEnd, identifier: 9, timestamp: 14)
      try ring.validateComplete()
      XCTAssertEqual(ring.snapshot().map(\.sequence), [0, 1])
      XCTAssertEqual(ring.snapshot().map(\.identifier), [9, 9])
   }

   func testTelemetryRingRejectsMalformedScopesAndClocks()
   {
      let nesting = BenchmarkTelemetryRing(capacity: 2)
      nesting.append(kind: .phaseBegin, identifier: 1, timestamp: 10)
      nesting.append(kind: .scenarioEnd, identifier: 1, timestamp: 11)
      XCTAssertThrowsError(try nesting.validateComplete())

      let clock = BenchmarkTelemetryRing(capacity: 2)
      clock.append(kind: .scenarioBegin, identifier: 1, timestamp: 11)
      clock.append(kind: .scenarioEnd, identifier: 1, timestamp: 10)
      XCTAssertThrowsError(try clock.validateComplete())
   }

   func testTelemetryCoverageDistinguishesObservedExternalAndUnavailableEvents() throws
   {
      let ring = completeCommonTelemetryRing()
      let coverage = try benchmarkTelemetryCoverage(records: ring.snapshot(), side: .native, passID: "primary-presentation", rendererDiagnosticsEnabled: false)
      XCTAssertEqual(coverage.entries.count, BenchmarkEventKind.allCases.count)
      XCTAssertEqual(coverage.entries.first(where: {$0.kind == "readyToInput"})?.availability, .ringRequired)
      XCTAssertEqual(coverage.entries.first(where: {$0.kind == "presentation"})?.availability, .externalEvidence)
      XCTAssertEqual(coverage.entries.first(where: {$0.kind == "layoutBegin"})?.availability, .unavailable)
      XCTAssertEqual(coverage.entries.first(where: {$0.kind == "gpuDuration"})?.observedCount, 0)
   }

   func testTelemetryCoverageRejectsUnsupportedEventsMasqueradingAsSamples()
   {
      let ring = completeCommonTelemetryRing()
      ring.append(kind: .presentation, timestamp: 10)
      XCTAssertThrowsError(try benchmarkTelemetryCoverage(records: ring.snapshot(), side: .native, passID: "primary-presentation", rendererDiagnosticsEnabled: false))
   }

   func testBenchmarkSpecLoaderRejectsHashMismatchAndTraversal() throws
   {
      let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
      defer { try? FileManager.default.removeItem(at: root) }
      try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
      let bytes = Data("fixture".utf8)
      try bytes.write(to: root.appendingPathComponent("fixture.json"))
      let loader = BenchmarkSpecLoader(root: root)
      XCTAssertEqual(try loader.read(BenchmarkArtifactIdentity(path: "fixture.json", sha256: comparisonSHA256(bytes))), bytes)
      XCTAssertThrowsError(try loader.read(BenchmarkArtifactIdentity(path: "fixture.json", sha256: String(repeating: "0", count: 64))))
      XCTAssertThrowsError(try loader.read(BenchmarkArtifactIdentity(path: "../fixture.json", sha256: comparisonSHA256(bytes))))
   }

   func testBenchmarkSpecLoaderValidatesAssetManifestMembers() throws
   {
      let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
      defer { try? FileManager.default.removeItem(at: root) }
      try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
      let asset = Data("pixels".utf8)
      try asset.write(to: root.appendingPathComponent("atlas.png"))
      let manifest = BenchmarkAssetManifest(
         schemaVersion: 1,
         id: "assets",
         artifacts: [BenchmarkAssetFile(
            role: "thumbnail-atlas",
            artifact: BenchmarkArtifactIdentity(path: "atlas.png", sha256: comparisonSHA256(asset)),
            mediaType: "image/png",
            colorSpace: "srgb"
         )],
         inlineTextAtlas: nil
      )
      let manifestBytes = try JSONEncoder.comparisonCanonical.encode(manifest)
      try manifestBytes.write(to: root.appendingPathComponent("manifest.json"))
      let identity = BenchmarkArtifactIdentity(path: "manifest.json", sha256: comparisonSHA256(manifestBytes))
      let loader = BenchmarkSpecLoader(root: root)

      XCTAssertEqual(try loader.loadAssetManifest(identity), manifest)
      try Data("different".utf8).write(to: root.appendingPathComponent("atlas.png"))
      XCTAssertThrowsError(try loader.loadAssetManifest(identity))
   }

   func testBenchmarkSpecLoaderValidatesInlineTextAtlasMetricsAndProvenance() throws
   {
      let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
      defer { try? FileManager.default.removeItem(at: root) }
      try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
      let pixels = Data("pixels".utf8)
      let license = Data("license".utf8)
      try pixels.write(to: root.appendingPathComponent("inline.png"))
      try license.write(to: root.appendingPathComponent("OFL.txt"))
      var manifest = BenchmarkAssetManifest(
         schemaVersion: 1,
         id: "inline-assets",
         artifacts: [BenchmarkAssetFile(
            role: "inline-text-atlas",
            artifact: BenchmarkArtifactIdentity(path: "inline.png", sha256: comparisonSHA256(pixels)),
            mediaType: "image/png",
            colorSpace: "srgb"
         )],
         inlineTextAtlas: BenchmarkInlineTextAtlas(
            columns: 2,
            rows: 2,
            sourceRepository: "https://github.com/googlefonts/noto-emoji",
            sourceCommit: "8998f5dd683424a73e2314a8c1f1e359c19e8742",
            license: BenchmarkArtifactIdentity(path: "OFL.txt", sha256: comparisonSHA256(license)),
            variants: [BenchmarkInlineTextAtlasVariant(
               artifactRole: "inline-text-atlas",
               pixelWidth: 128,
               pixelHeight: 128,
               emPixels: 64
            )],
            entries: [BenchmarkInlineTextAsset(
               grapheme: "🌍",
               column: 0,
               row: 0,
               advanceMillionths: 1_000_000,
               topFromBaselineMillionths: -800_000,
               widthMillionths: 1_000_000,
               heightMillionths: 1_000_000
            )]
         )
      )
      var bytes = try JSONEncoder.comparisonCanonical.encode(manifest)
      try bytes.write(to: root.appendingPathComponent("manifest.json"))
      let loader = BenchmarkSpecLoader(root: root)
      _ = try loader.loadAssetManifest(BenchmarkArtifactIdentity(path: "manifest.json", sha256: comparisonSHA256(bytes)))

      manifest = BenchmarkAssetManifest(
         schemaVersion: manifest.schemaVersion,
         id: manifest.id,
         artifacts: manifest.artifacts,
         inlineTextAtlas: BenchmarkInlineTextAtlas(
            columns: 2,
            rows: 2,
            sourceRepository: "https://github.com/googlefonts/noto-emoji",
            sourceCommit: "8998f5dd683424a73e2314a8c1f1e359c19e8742",
            license: BenchmarkArtifactIdentity(path: "OFL.txt", sha256: comparisonSHA256(license)),
            variants: [BenchmarkInlineTextAtlasVariant(
               artifactRole: "inline-text-atlas",
               pixelWidth: 128,
               pixelHeight: 128,
               emPixels: 64
            )],
            entries: [BenchmarkInlineTextAsset(
               grapheme: "🌍",
               column: 2,
               row: 0,
               advanceMillionths: 1_000_000,
               topFromBaselineMillionths: -800_000,
               widthMillionths: 1_000_000,
               heightMillionths: 1_000_000
            )]
         )
      )
      bytes = try JSONEncoder.comparisonCanonical.encode(manifest)
      try bytes.write(to: root.appendingPathComponent("manifest.json"))
      XCTAssertThrowsError(try loader.loadAssetManifest(BenchmarkArtifactIdentity(path: "manifest.json", sha256: comparisonSHA256(bytes))))
   }

   func testRegisteredFontCatalogAppliesEveryManifestVariationAxis() throws
   {
      let source = URL(fileURLWithPath: #filePath)
      let workspace = source.deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
      let loader = BenchmarkSpecLoader(root: workspace.appendingPathComponent("benchmarks/comparative/specs/v1"))
      let identity = try loader.loadScenario(relativePath: "scenarios/dashboard.mixed-static.json").fontPack
      let manifest = try loader.loadFontPack(identity)
      let catalog = try loader.loadRegisteredFontCatalog(identity)

      for entry in manifest.fonts
      {
         let font = try catalog.font(role: entry.role, size: 15)
         let applied = try XCTUnwrap(CTFontCopyVariation(font) as? [NSNumber: NSNumber], entry.role)
         let axes = try XCTUnwrap(CTFontCopyVariationAxes(font) as? [[CFString: Any]], entry.role)
         XCTAssertEqual(axes.count, entry.variationAxes.count, entry.role)
         let defaults = Dictionary(uniqueKeysWithValues: try axes.map
         {
            axis in
            (
               try XCTUnwrap(axis[kCTFontVariationAxisIdentifierKey] as? NSNumber),
               try XCTUnwrap(axis[kCTFontVariationAxisDefaultValueKey] as? NSNumber)
            )
         })
         XCTAssertEqual(
            CTFontCopyAttribute(font, kCTFontURLAttribute) as? URL,
            try loader.verifiedURL(entry.artifact).standardizedFileURL.resolvingSymlinksInPath(),
            entry.role
         )
         for (index, axis) in entry.variationAxes.enumerated()
         {
            let identifier = NSNumber(value: try XCTUnwrap(benchmarkFontVariationIdentifier(axis.tag)))
            XCTAssertEqual(axes[index][kCTFontVariationAxisIdentifierKey] as? NSNumber, identifier, "\(entry.role):\(axis.tag)")
            let value = try XCTUnwrap(applied[identifier] ?? defaults[identifier], "\(entry.role):\(axis.tag)")
            XCTAssertEqual(value.doubleValue * 1_000_000, Double(axis.valueMillionths), accuracy: 0.5, "\(entry.role):\(axis.tag)")
         }
      }
   }

   func testAppleAdaptersUsePreparedCatalogFontsWithoutNamedOrSystemFallback() throws
   {
      let source = URL(fileURLWithPath: #filePath)
      let workspace = source.deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
      for relativePath in [
         "host/apple-comparison/UIKit-iOS/UIKitScenarioAdapter.swift",
         "host/apple-comparison/AppKit-macOS/AppKitScenarioAdapter.swift",
      ]
      {
         let adapter = String(decoding: try Data(contentsOf: workspace.appendingPathComponent(relativePath)), as: UTF8.self)
         XCTAssertTrue(adapter.contains("loadRegisteredFontCatalog"), relativePath)
         XCTAssertFalse(adapter.contains("UIFont(name:"), relativePath)
         XCTAssertFalse(adapter.contains("NSFont(name:"), relativePath)
         XCTAssertFalse(adapter.contains("systemFont"), relativePath)
         XCTAssertFalse(adapter.contains("withSize"), relativePath)
      }
   }

   func testDurableWriterRoundTripsCanonicalEnvelope() throws
   {
      let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
      defer { try? FileManager.default.removeItem(at: root) }
      let store = DurableArtifactStore(root: root)
      let envelope = ComparisonProbeEnvelope(
         schemaVersion: 1,
         runID: "run",
         planSHA256: String(repeating: "a", count: 64),
         passID: "phase0",
         pairIndex: 0,
         side: .oxide,
         generation: "generation",
         predecessorSHA256: nil,
         payloadSHA256: String(repeating: "b", count: 64)
      )
      let written = try store.durableJSON(envelope, relativePath: "Runs/run/oxide.json")
      XCTAssertEqual(written.sha256, comparisonSHA256(try JSONEncoder.comparisonCanonical.encode(envelope)))
      XCTAssertEqual(try store.readJSON(ComparisonProbeEnvelope.self, relativePath: "Runs/run/oxide.json"), envelope)
      XCTAssertFalse(FileManager.default.fileExists(atPath: written.url.appendingPathExtension("tmp").path))
   }

   func testGenerationMismatchFailsClosed() throws
   {
      let request = ComparisonProbeRequest(
         schemaVersion: 1,
         runID: "run",
         planSHA256: String(repeating: "a", count: 64),
         passID: "phase0",
         pairIndex: 0,
         side: .oxide,
         generation: "current",
         transportMode: .appGroup,
         predecessorSHA256: nil
      )
      let stale = ComparisonProbeEnvelope(
         schemaVersion: 1,
         runID: request.runID,
         planSHA256: request.planSHA256,
         passID: request.passID,
         pairIndex: request.pairIndex,
         side: request.side,
         generation: "stale",
         predecessorSHA256: nil,
         payloadSHA256: String(repeating: "b", count: 64)
      )
      XCTAssertThrowsError(try validateComparisonEnvelope(stale, request: request))
   }

   func testControlNotificationNamesAreGenerationSpecific()
   {
      let first = comparisonGenerationNotification(comparisonReadyNotification, generation: "first")
      let second = comparisonGenerationNotification(comparisonReadyNotification, generation: "second")
      XCTAssertEqual(first, "com.oxide.compare.ready.gfirst")
      XCTAssertNotEqual(first, second)
      XCTAssertNotEqual(first, comparisonGenerationNotification(comparisonCompleteNotification, generation: "first"))
   }

   func testPerAppNativeRequiresCanonicalPredecessorHash() throws
   {
      let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
      defer { try? FileManager.default.removeItem(at: root) }
      let request = ComparisonProbeRequest(
         schemaVersion: 1,
         runID: "run",
         planSHA256: String(repeating: "a", count: 64),
         passID: "phase0",
         pairIndex: 0,
         side: .native,
         generation: "generation",
         transportMode: .perAppContainer,
         predecessorSHA256: nil
      )
      XCTAssertThrowsError(try ComparisonTransport(request: request, store: DurableArtifactStore(root: root)).runSide())
   }

   func testSemanticCheckpointEncodingMatchesFrozenStateAndAccessibilityContracts() throws
   {
      let source = URL(fileURLWithPath: #filePath)
      let workspace = source.deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
      let loader = BenchmarkSpecLoader(root: workspace.appendingPathComponent("benchmarks/comparative/specs/v1"))

      for id in [
         "startup.first-screen",
         "dashboard.mixed-static",
         "feed.variable-scroll",
         "chat.live-update",
         "navigation.modal",
         "image.decode-zoom",
      ]
      {
         let scenario = try loader.loadScenario(relativePath: "scenarios/\(id).json")
         for expected in scenario.parityCheckpoints
         {
            let expectedState = try XCTUnwrap(try JSONSerialization.jsonObject(with: loader.read(expected.state)) as? [String: Any])
            let model = try XCTUnwrap(expectedState["model"] as? [String: Any])
            let observed = try benchmarkCheckpoint(
               scenario: scenario,
               checkpointID: expected.id,
               model: model,
               visibleRoleCounts: expected.expectedVisibleRoleCounts
            )
            XCTAssertEqual(
               try JSONSerialization.jsonObject(with: observed.state) as? NSDictionary,
               try JSONSerialization.jsonObject(with: loader.read(expected.state)) as? NSDictionary
            )
            XCTAssertEqual(
               try JSONSerialization.jsonObject(with: observed.accessibility) as? NSDictionary,
               try JSONSerialization.jsonObject(with: loader.read(expected.accessibility)) as? NSDictionary
            )
            let validated = try validateBenchmarkCheckpoint(observed, expected: expected, loader: loader)
            XCTAssertEqual(validated.visibleRoleCounts, expected.expectedVisibleRoleCounts)
         }
      }
   }

   func testCheckpointValidationRejectsStateAndRoleMismatches() throws
   {
      let source = URL(fileURLWithPath: #filePath)
      let workspace = source.deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
      let loader = BenchmarkSpecLoader(root: workspace.appendingPathComponent("benchmarks/comparative/specs/v1"))
      let scenario = try loader.loadScenario(relativePath: "scenarios/image.decode-zoom.json")
      let expected = try XCTUnwrap(scenario.parityCheckpoints.first)
      let expectedState = try XCTUnwrap(try JSONSerialization.jsonObject(with: loader.read(expected.state)) as? [String: Any])
      let valid = try benchmarkCheckpoint(
         scenario: scenario,
         checkpointID: expected.id,
         model: try XCTUnwrap(expectedState["model"] as? [String: Any]),
         visibleRoleCounts: expected.expectedVisibleRoleCounts
      )
      let invalidState = BenchmarkAdapterCheckpoint(
         state: Data("{}".utf8),
         accessibility: valid.accessibility,
         visibleRoleCounts: valid.visibleRoleCounts
      )
      XCTAssertThrowsError(try validateBenchmarkCheckpoint(invalidState, expected: expected, loader: loader))
      let invalidRoles = BenchmarkAdapterCheckpoint(
         state: valid.state,
         accessibility: valid.accessibility,
         visibleRoleCounts: [BenchmarkRoleCount(role: "image", count: 0)]
      )
      XCTAssertThrowsError(try validateBenchmarkCheckpoint(invalidRoles, expected: expected, loader: loader))
      var accessibility = try XCTUnwrap(try JSONSerialization.jsonObject(with: valid.accessibility) as? [String: Any])
      var nodes = try XCTUnwrap(accessibility["nodes"] as? [[String: Any]])
      nodes[0]["name"] = "wrong-accessibility-name"
      accessibility["nodes"] = nodes
      let invalidAccessibility = BenchmarkAdapterCheckpoint(
         state: valid.state,
         accessibility: try JSONSerialization.data(withJSONObject: accessibility, options: [.sortedKeys]),
         visibleRoleCounts: valid.visibleRoleCounts
      )
      XCTAssertThrowsError(try validateBenchmarkCheckpoint(invalidAccessibility, expected: expected, loader: loader))
   }

   func testUIKitResetRestoresSixScenesInPlace() throws
   {
      let source = URL(fileURLWithPath: #filePath)
      let workspace = source.deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
      let adapterURL = workspace.appendingPathComponent("host/apple-comparison/UIKit-iOS/UIKitScenarioAdapter.swift")
      let adapter = String(decoding: try Data(contentsOf: adapterURL), as: UTF8.self)
      XCTAssertEqual(adapter.components(separatedBy: "func reset() throws").count - 1, 8)
      let start = try XCTUnwrap(adapter.range(of: "   func reset() throws\n   {\n      guard let scene, let viewport else"))
      let end = try XCTUnwrap(adapter.range(of: "\n   func quiesce() throws", range: start.upperBound..<adapter.endIndex))
      let reset = String(adapter[start.lowerBound..<end.lowerBound])
      XCTAssertTrue(reset.contains("try scene.reset()"))
      XCTAssertFalse(reset.contains("installScene"))
      XCTAssertFalse(reset.contains("scene?.teardown()"))
      XCTAssertFalse(reset.contains("window.rootViewController"))
      for seedRestoration in [
         "rows = initialRows",
         "messages = initialMessages",
         "foregroundCount = 0",
         "leafUpdates = 0",
         "navigationController.popToRootViewController(animated: false)",
         "imageView.image = assets.thumbnail",
      ]
      {
         XCTAssertTrue(adapter.contains(seedRestoration), seedRestoration)
      }
      XCTAssertTrue(adapter.contains("viewport?.detachContent()"))
      XCTAssertTrue(adapter.contains("window.rootViewController = hostController"))
      XCTAssertFalse(adapter.contains("window.rootViewController = nil"))
      XCTAssertTrue(adapter.contains("viewport?.removeFromParent()"))
      XCTAssertTrue(adapter.contains("content.view.removeFromSuperview()"))
      XCTAssertTrue(adapter.contains("addChild(content)"))
      XCTAssertTrue(adapter.contains("content.removeFromParent()"))
      XCTAssertTrue(adapter.contains("tableView.autoresizingMask = []"))
      XCTAssertTrue(adapter.contains("tableView.frame = CGRect(x: 0, y: 52, width: 390, height: 792)"))
      XCTAssertTrue(adapter.contains("imageView.image = nil"))
      XCTAssertTrue(adapter.contains("imageView.layer.contents = nil"))
      XCTAssertTrue(adapter.contains("decodedImage = nil"))
      XCTAssertTrue(adapter.contains("assets.releaseDecodedSource()"))
      XCTAssertTrue(adapter.contains("correctnessMode = passID == \"correctness\""))
      XCTAssertTrue(adapter.contains("animated: !correctnessMode"))
      XCTAssertTrue(adapter.contains("CGImageSourceCreateImageAtIndex"))
      XCTAssertTrue(adapter.contains("CGColorSpace(name: CGColorSpace.sRGB)"))
      XCTAssertTrue(adapter.contains("let context = CGContext("))
      XCTAssertTrue(adapter.contains("tableView.register(UIKitFeedCell.self"))
      let feedCellStart = try XCTUnwrap(adapter.range(of: "private final class UIKitFeedCell"))
      let feedCellEnd = try XCTUnwrap(adapter.range(of: "final class UIKitFeedScene", range: feedCellStart.upperBound..<adapter.endIndex))
      XCTAssertFalse(adapter[feedCellStart.lowerBound..<feedCellEnd.lowerBound].contains("defaultContentConfiguration()"))
      XCTAssertTrue(adapter.contains("card.frame = CGRect(x: 12, y: 4, width: bounds.width - 24, height: bounds.height - 8)"))
      XCTAssertTrue(adapter.contains("thumbnail.frame = CGRect(x: 10, y: 10, width: 48, height: 48)"))
      XCTAssertTrue(adapter.contains("favoriteControl.frame = CGRect(x: card.bounds.width - 34, y: 11, width: 18, height: 18)"))
      XCTAssertTrue(adapter.contains("blur.frame = CGRect(x: 16, y: 64 + CGFloat(index) * 204, width: width - 32, height: 96)"))
      XCTAssertTrue(adapter.contains("UIButton(type: .custom)"))
      XCTAssertEqual(adapter.components(separatedBy: ".layer.cornerRadius = ").count - 1, 14)
      XCTAssertEqual(adapter.components(separatedBy: ".layer.cornerRadius = 0").count - 1, 4)
      XCTAssertEqual(adapter.components(separatedBy: ".layer.cornerCurve = .circular").count - 1, 11)
   }

   func testUIKitPreviewCapturesTheStaticLayerTreeIntoOpaqueSRGB8() throws
   {
      let source = URL(fileURLWithPath: #filePath)
      let workspace = source.deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
      let adapterURL = workspace.appendingPathComponent("host/apple-comparison/UIKit-iOS/UIKitScenarioAdapter.swift")
      let adapter = String(decoding: try Data(contentsOf: adapterURL), as: UTF8.self)
      XCTAssertTrue(adapter.contains("format.scale = 3"))
      XCTAssertTrue(adapter.contains("format.opaque = true"))
      XCTAssertTrue(adapter.contains("format.preferredRange = .standard"))
      XCTAssertTrue(adapter.contains("captureView.layer.render(in: context.cgContext)"))
      XCTAssertFalse(adapter.contains("drawHierarchy"))
   }

   func testOxideRendererDiagnosticsAreIsolatedToAttributionPassesAndFailOnDroppedFrames() throws
   {
      let source = URL(fileURLWithPath: #filePath)
      let workspace = source.deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
      let adapter = String(decoding: try Data(contentsOf: workspace.appendingPathComponent("host/apple-comparison/Oxide-macOS/OxideScenarioAdapter.swift")), as: UTF8.self)
      let executor = String(decoding: try Data(contentsOf: workspace.appendingPathComponent("host/apple-comparison/Shared/ComparatorApp/BenchmarkCampaignExecutor.swift")), as: UTF8.self)
      let runtime = String(decoding: try Data(contentsOf: workspace.appendingPathComponent("host/apple-comparison/oxide-comparison-runtime/src/lib.rs")), as: UTF8.self)
      XCTAssertTrue(adapter.contains("rendererDiagnosticsEnabled = passID == \"common-gpu\" || passID == \"full-attribution\""))
      XCTAssertTrue(adapter.contains("noncontiguous renderer submission diagnostics"))
      XCTAssertTrue(adapter.contains("noncontiguous renderer completion diagnostics"))
      XCTAssertTrue(executor.contains("try recordRendererDiagnostics()"))
      XCTAssertTrue(executor.contains("flags: 1"))
      XCTAssertTrue(runtime.contains("renderer_diagnostics_enabled: bool"))
      XCTAssertTrue(runtime.contains("oxide_comparison_renderer_diagnostics"))
      XCTAssertTrue(runtime.contains("gpu_duration_ns: milliseconds_to_nanoseconds(gpu.gpu_ms)"))
   }

   func testMacOSCorrectnessGeometryUsesTheCanonicalCaptureProfileAndRuntimeBounds() throws
   {
      let node = BenchmarkMacOSCorrectnessGeometryNode(
         ordinal: 0,
         kind: "text",
         role: "heading",
         identifier: "title",
         bounds: BenchmarkMacOSCorrectnessLogicalRect(x: 16, y: 20, width: 200, height: 48),
         textLineBounds: [BenchmarkMacOSCorrectnessLogicalRect(x: 16, y: 20, width: 200, height: 48)]
      )
      let geometry = try benchmarkMacOSCorrectnessGeometry(rootBounds: CGRect(x: 0, y: 0, width: 844, height: 390), source: "appkit-view-tree", nodes: [node])
      XCTAssertEqual(geometry.schemaVersion, 1)
      XCTAssertEqual(geometry.coordinateSpace, "logical-points")
      XCTAssertEqual(geometry.captureProfile, BenchmarkMacOSCorrectnessCaptureProfile.canonical.id)
      XCTAssertEqual(geometry.canonicalScale, 3)
      XCTAssertEqual(geometry.source, "appkit-view-tree")
      XCTAssertEqual(geometry.nodes, [node])
      XCTAssertEqual(geometry.root, BenchmarkMacOSCorrectnessLogicalRect(x: 0, y: 0, width: 844, height: 390))
      XCTAssertThrowsError(try benchmarkMacOSCorrectnessGeometry(rootBounds: CGRect(x: 1, y: 0, width: 844, height: 390), source: "appkit-view-tree", nodes: [node]))

      let source = URL(fileURLWithPath: #filePath)
      let workspace = source.deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
      let executor = String(decoding: try Data(contentsOf: workspace.appendingPathComponent("host/apple-comparison/Shared/ComparatorApp/BenchmarkCampaignExecutor.swift")), as: UTF8.self)
      let oxide = String(decoding: try Data(contentsOf: workspace.appendingPathComponent("host/apple-comparison/Oxide-macOS/OxideScenarioAdapter.swift")), as: UTF8.self)
      XCTAssertTrue(executor.contains("geometry.actual.json"))
      XCTAssertTrue(executor.contains("BenchmarkMacOSCorrectnessGeometryCapture"))
      XCTAssertTrue(oxide.contains("source: \"oxide-semantic-tree\""))
      XCTAssertTrue(oxide.contains("oxideComparisonGeometryNodesJSON"))
      XCTAssertFalse(oxide.contains("scale: Float(3)"))
   }

   func testFixedPackRecoveryLimitAndEvidenceDoNotRatchet() throws
   {
      XCTAssertEqual(try benchmarkFixedFootprintRecoveryLimit(100), 105)
      XCTAssertEqual(try benchmarkFixedFootprintRecoveryLimit(19), 19)
      XCTAssertThrowsError(try benchmarkFixedFootprintRecoveryLimit(UInt64.max))
      let evidence = BenchmarkResetRecoveryEvidence(
         segmentID: "core-interaction",
         fixedPackBaselinePhysicalFootprintBytes: 100,
         recoveredPhysicalFootprintBytes: 104,
         recoveryLimitBytes: 105,
         recoveryDeadlineMs: 5_000,
         validation: "fixed-pack-baseline"
      )
      XCTAssertEqual(try JSONDecoder().decode(BenchmarkResetRecoveryEvidence.self, from: JSONEncoder.comparisonCanonical.encode(evidence)), evidence)
   }

   func testExecutorSeparatesResetSentinelFromPostSegmentRecovery() throws
   {
      let source = URL(fileURLWithPath: #filePath)
      let workspace = source.deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
      let executorURL = workspace.appendingPathComponent("host/apple-comparison/Shared/ComparatorApp/BenchmarkCampaignExecutor.swift")
      let executor = String(decoding: try Data(contentsOf: executorURL), as: UTF8.self)
      let warmedSample = try XCTUnwrap(executor.range(of: "let fixedBaselineBytes = max(warmPackHighWaterBytes, warmCaptureBytes)"))
      let readyWrite = try XCTUnwrap(executor.range(of: "_ = try store.durableJSON(ready, relativePath: relativePath(\"ready.json\"))"))
      let fixedSample = try XCTUnwrap(executor.range(of: "fixedPackBaselinePhysicalFootprintBytes = try benchmarkPhysicalFootprintBytes()", range: readyWrite.upperBound..<executor.endIndex))
      XCTAssertLessThan(warmedSample.lowerBound, readyWrite.lowerBound)
      XCTAssertLessThan(readyWrite.lowerBound, fixedSample.lowerBound)
      XCTAssertTrue(executor[readyWrite.lowerBound...].contains("if fixedPackBaselinePhysicalFootprintBytes == nil"))
      let warmCapture = try XCTUnwrap(executor.range(of: "if invocation.request.passID == \"correctness\", let capture = adapter as? BenchmarkPreviewCapture"))
      let start = try XCTUnwrap(executor.range(of: "\n   func start()", range: warmCapture.upperBound..<executor.endIndex))
      let captureWarmup = executor[warmCapture.lowerBound..<start.lowerBound]
      XCTAssertTrue(captureWarmup.contains("prewarmCorrectnessPack"))
      XCTAssertTrue(captureWarmup.contains("evidenceDirectory: \"warmup\""))
      let checkpointCapture = try XCTUnwrap(executor.range(of: "private func captureCheckpoint"))
      let removeWarmup = try XCTUnwrap(executor.range(of: "private func removeCorrectnessWarmupArtifacts", range: checkpointCapture.upperBound..<executor.endIndex))
      let checkpointCaptureSource = executor[checkpointCapture.lowerBound..<removeWarmup.lowerBound]
      XCTAssertTrue(checkpointCaptureSource.contains("guard let quiescence = adapter as? BenchmarkQuiescenceAdapter"))
      XCTAssertTrue(checkpointCaptureSource.contains("try quiescence.quiesce()"))
      XCTAssertTrue(captureWarmup.contains("removeCorrectnessWarmupArtifacts"))
      XCTAssertTrue(captureWarmup.contains("stableWarmPackTransitions == 2"))
      XCTAssertTrue(captureWarmup.contains("benchmarkWarmFootprintConverged"))
      XCTAssertTrue(captureWarmup.contains("delta <= 4 * 1_024 * 1_024"))
      XCTAssertTrue(captureWarmup.contains("warmPackHighWaterBytes"))
      XCTAssertTrue(captureWarmup.contains("previousWarmPackHighWaterBytes"))
      XCTAssertTrue(captureWarmup.contains("running-pack-high-water"))
      XCTAssertFalse(captureWarmup.contains("benchmarkWarmFootprintConverged(previousWarmPackBytes, observed)"))
      XCTAssertTrue(captureWarmup.contains("samples.append(try benchmarkPhysicalFootprintBytes())"))
      XCTAssertTrue(captureWarmup.contains("let observed = packSamples.max() ?? 0"))
      XCTAssertTrue(captureWarmup.contains("for _ in 0..<10"))
      XCTAssertTrue(captureWarmup.contains("BenchmarkWarmupFootprintEvidence"))
      XCTAssertTrue(captureWarmup.contains("qualification/warmup-footprint.json"))
      XCTAssertTrue(captureWarmup.contains("fixedBaselineBytes: fixedBaselineBytes"))
      XCTAssertTrue(captureWarmup.contains("prewarmTimedPack"))
      XCTAssertTrue(captureWarmup.contains("for preparedScenario in prepared"))
      XCTAssertTrue(captureWarmup.contains("var schedule = try BenchmarkPhaseSchedule"))
      XCTAssertTrue(captureWarmup.contains("try schedule.drainTimed(until: schedule.durationUs)"))
      XCTAssertTrue(captureWarmup.contains("case .traceEvent(let event): try adapter.apply(event: event)"))
      XCTAssertTrue(captureWarmup.contains("case .checkpoint:"))
      XCTAssertTrue(captureWarmup.contains("try adapter.teardown()"))
      let correctnessPack = try XCTUnwrap(captureWarmup.range(of: "private func prewarmCorrectnessPack"))
      let timedPack = try XCTUnwrap(captureWarmup.range(of: "private func prewarmTimedPack", range: correctnessPack.upperBound..<captureWarmup.endIndex))
      let captureBaseline = try XCTUnwrap(captureWarmup.range(of: "private func warmCorrectnessCaptureBaseline", range: timedPack.upperBound..<captureWarmup.endIndex))
      let correctnessPrewarm = captureWarmup[correctnessPack.lowerBound..<timedPack.lowerBound]
      let timedPrewarm = captureWarmup[timedPack.lowerBound..<captureBaseline.lowerBound]
      XCTAssertTrue(correctnessPrewarm.contains("try adapter.teardown()\n         }\n         try drainRetiredAdapterResources()\n         malloc_zone_pressure_relief"))
      XCTAssertTrue(timedPrewarm.contains("try adapter.teardown()\n         }\n         try drainRetiredAdapterResources()\n      }\n      malloc_zone_pressure_relief"))
      XCTAssertEqual(correctnessPrewarm.components(separatedBy: "try adapter.teardown()").count - 1, 1)
      XCTAssertEqual(timedPrewarm.components(separatedBy: "try adapter.teardown()").count - 1, 1)
      XCTAssertTrue(captureWarmup.contains("warmCorrectnessCaptureBaseline"))
      XCTAssertTrue(executor.contains("var due = [BenchmarkScheduledAction]()"))
      XCTAssertTrue(executor.contains("for (index, action) in due.enumerated()"))
      XCTAssertTrue(executor.contains("try consume(action, timestamp: mach_continuous_time())"))
      XCTAssertTrue(captureWarmup.contains("try autoreleasepool {try warmCorrectnessCollectors(capture: capture)}"))
      XCTAssertTrue(captureWarmup.contains("if let rawAccessibility = adapter as? BenchmarkRawAccessibilityCapture"))
      XCTAssertTrue(captureWarmup.contains("_ = try rawAccessibility.rawAccessibilityTree()"))
      XCTAssertTrue(captureWarmup.contains("_ = try capture.previewPNG()"))
      XCTAssertTrue(captureWarmup.contains("var previous: UInt64?"))
      XCTAssertEqual(captureWarmup.components(separatedBy: "for _ in 0..<10").count - 1, 1)
      XCTAssertEqual(captureWarmup.components(separatedBy: "for _ in 0..<6").count - 1, 1)
      XCTAssertEqual(captureWarmup.components(separatedBy: "for _ in 0..<2").count - 1, 1)
      XCTAssertTrue(captureWarmup.contains("packWarmupUnstable(warmPackSamples)"))
      XCTAssertTrue(captureWarmup.contains("captureWarmupUnstable"))
      XCTAssertTrue(executor.contains("checkpoints.append(try autoreleasepool"))
      XCTAssertTrue(executor.contains("try autoreleasepool\n         {\n            try schedule.drainTimed"))
      XCTAssertTrue(executor.contains("case .scenarioEnd:\n               try consume"))
      XCTAssertTrue(executor.contains("resetRecoveries: resetRecoveryEvidence"))
      XCTAssertTrue(executor.contains("host-static-calibrated-oxide-native-pair-pending"))
      XCTAssertTrue(executor.contains("complete-state-accessibility-exact-static-pair-host-reducer-pending"))
      XCTAssertFalse(executor.contains("screenshotValidation = \"host-reducer-pending\""))
      XCTAssertFalse(executor.contains("let baselineFootprint = try benchmarkPhysicalFootprintBytes()"))

      let resetStart = try XCTUnwrap(executor.range(of: "   private func performResetIfNeeded"))
      let recoveryStart = try XCTUnwrap(executor.range(of: "\n   private func finishScenario", range: resetStart.upperBound..<executor.endIndex))
      let reset = String(executor[resetStart.lowerBound..<recoveryStart.lowerBound])
      XCTAssertTrue(reset.contains("adapter-reset-exact-seed-sentinel-equality"))
      XCTAssertFalse(reset.contains("waitForFootprintRecovery"))
      XCTAssertFalse(reset.contains("ring.append"))

      let recoveryEnd = try XCTUnwrap(executor.range(of: "\n   private func waitForFootprintRecovery", range: recoveryStart.upperBound..<executor.endIndex))
      let recovery = String(executor[recoveryStart.lowerBound..<recoveryEnd.lowerBound])
      let surfaceAttestation = try XCTUnwrap(recovery.range(of: "surfaceRuntimeSnapshot = try surfaceProvider.macOSSurfaceSnapshot()"))
      let scaleAttestation = try XCTUnwrap(recovery.range(of: "scaleRuntimeAttestation = try scaled.scaleRuntimeAttestation()"))
      let quiesce = try XCTUnwrap(recovery.range(of: "try quiescence.quiesce()"))
      let teardown = try XCTUnwrap(recovery.range(of: "try autoreleasepool {try adapter.teardown()}", range: quiesce.upperBound..<recovery.endIndex))
      let drain = try XCTUnwrap(recovery.range(of: "try drainRetiredAdapterResources()", range: teardown.upperBound..<recovery.endIndex))
      let pressureRelief = try XCTUnwrap(recovery.range(of: "malloc_zone_pressure_relief(nil, 0)", range: drain.upperBound..<recovery.endIndex))
      let wait = try XCTUnwrap(recovery.range(of: "waitForFootprintRecovery", range: pressureRelief.upperBound..<recovery.endIndex))
      XCTAssertLessThan(surfaceAttestation.lowerBound, teardown.lowerBound)
      XCTAssertLessThan(scaleAttestation.lowerBound, teardown.lowerBound)
      XCTAssertLessThan(quiesce.lowerBound, teardown.lowerBound)
      XCTAssertLessThan(teardown.lowerBound, drain.lowerBound)
      XCTAssertLessThan(drain.lowerBound, pressureRelief.lowerBound)
      XCTAssertLessThan(pressureRelief.lowerBound, wait.lowerBound)
      XCTAssertTrue(executor[recoveryEnd.lowerBound...].contains("malloc_zone_pressure_relief(nil, 0)"))
      XCTAssertTrue(recovery.contains("if invocation.request.passID != \"correctness\""))
      XCTAssertTrue(recovery.contains("BenchmarkResetRecoveryAttemptEvidence"))
      XCTAssertTrue(recovery.contains("initialPostTeardownPhysicalFootprintBytes"))
      XCTAssertTrue(recovery.contains("recoveries/\\(segment.id)/attempt.json"))
      XCTAssertTrue(recovery.contains("displayLink?.isPaused = true"))
      XCTAssertFalse(recovery.contains("displayLink?.isPaused = false"))
      XCTAssertTrue(recovery.contains("kind: .quiescence"))
      XCTAssertTrue(recovery.contains("kind: .resetComplete"))
      XCTAssertTrue(recovery.contains("resetRecoveryEvidence.append(evidence)"))
      let advanceStart = try XCTUnwrap(executor.range(of: "   private func advanceScenario"))
      let advanceEnd = try XCTUnwrap(executor.range(of: "\n#if os(macOS)", range: advanceStart.upperBound..<executor.endIndex))
      let advance = String(executor[advanceStart.lowerBound..<advanceEnd.lowerBound])
      let prepare = try XCTUnwrap(advance.range(of: "try adapter.prepare"))
      let resume = try XCTUnwrap(advance.range(of: "displayLink?.isPaused = false"))
      XCTAssertLessThan(prepare.lowerBound, resume.lowerBound)
   }

   func testMacOSCampaignUsesNativeDisplayCadenceInsteadOfAnUnsatisfiableFloor() throws
   {
      let source = URL(fileURLWithPath: #filePath)
      let workspace = source.deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
      let executorURL = workspace.appendingPathComponent("host/apple-comparison/Shared/ComparatorApp/BenchmarkCampaignExecutor.swift")
      let executor = String(decoding: try Data(contentsOf: executorURL), as: UTF8.self)
      XCTAssertTrue(executor.contains("displayLink.preferredFrameRateRange = .default"))
      XCTAssertFalse(executor.contains("minimum: 80"))
   }

   func testApplePrAcquisitionLoadsByExactHashAndFreezesFourChunks() throws
   {
      let source = URL(fileURLWithPath: #filePath)
      let workspace = source.deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
      let root = workspace.appendingPathComponent("benchmarks/comparative/specs/v1")
      let data = try Data(contentsOf: root.appendingPathComponent("acquisition/apple-pr.json"))
      let loader = BenchmarkSpecLoader(root: root)
      let acquisition = try loadApplePrAcquisition(
         loader: loader,
         identity: BenchmarkArtifactIdentity(path: "acquisition/apple-pr.json", sha256: comparisonSHA256(data))
      )
      XCTAssertEqual(acquisition.controllerChunks.map(\.id), [
         "correctness",
         "presentation-pairs-0-1",
         "presentation-pairs-2-3",
         "launch-pairs-0-3",
      ])
      XCTAssertEqual(acquisition.packs.first(where: {$0.id == "pr-non-launch"})?.computedCampaignSeconds, 480)
      let dynamic = try XCTUnwrap(acquisition.packs.first(where: {$0.id == "pr-non-launch"}))
      let packed = try benchmarkPackedScenarios(dynamic)
      XCTAssertEqual(packed.map(\.scenarioID), dynamic.orderedScenarioIds)
      XCTAssertEqual(packed.compactMap {$0.resetSegment?.id}, ["core-interaction", "scroll-damage", "media-text-warm"])
      XCTAssertEqual(acquisition.controllerChunks.first?.packIds, ["pr-non-launch", "pr-launch"])
      XCTAssertEqual(
         try benchmarkCorrectnessPackRuns(acquisition.controllerChunks[0].packIds),
         [
            BenchmarkCorrectnessPackRun(packID: "pr-non-launch", executionIndex: 0),
            BenchmarkCorrectnessPackRun(packID: "pr-launch", executionIndex: 1),
         ]
      )
      XCTAssertEqual(try benchmarkCampaignPackID(passID: "correctness", requestedPackID: "pr-launch"), "pr-launch")
      XCTAssertEqual(try benchmarkCampaignPackID(passID: "minimal-presentation", requestedPackID: nil), "pr-non-launch")
      XCTAssertEqual(try benchmarkCampaignPackID(passID: "canonical-launch", requestedPackID: nil), "pr-launch")
      XCTAssertThrowsError(try benchmarkCampaignPackID(passID: "correctness", requestedPackID: nil))
      XCTAssertThrowsError(try benchmarkCampaignPackID(passID: "minimal-presentation", requestedPackID: "pr-launch"))
      XCTAssertEqual(try benchmarkTelemetryCapacity(pack: dynamic, passID: "minimal-presentation"), 65_536)
      let endurance = ApplePrAcquisitionPack(
         id: "nightly-endurance",
         orderedScenarioIds: ["endurance.churn"],
         resetSegments: [],
         pairCount: 2,
         sidesPerPair: 2,
         resetCountPerSide: 0,
         resetSeconds: 0,
         setupSecondsPerScenario: 1,
         warmupSecondsPerScenario: 5,
         measureSecondsPerScenario: 300,
         sideSeconds: 306
      )
      XCTAssertEqual(try benchmarkTelemetryCapacity(pack: endurance, passID: "minimal-presentation"), 131_072)
      XCTAssertEqual(try benchmarkTelemetryCapacity(pack: endurance, passID: "common-gpu"), 262_144)
      XCTAssertEqual(benchmarkCampaignPairOrder(0), [.oxide, .native])
      XCTAssertEqual(benchmarkCampaignPairOrder(1), [.native, .oxide])
      XCTAssertEqual(
         benchmarkCampaignGeneration(
            planSHA256: String(repeating: "a", count: 64),
            runID: "run",
            chunkID: "chunk",
            passID: "minimal-presentation",
            pairIndex: 2,
            side: .oxide
         ),
         "af184b3e74811077faa12cc2dd180da5a85162fcf543211f61c3ff32b3cd595a"
      )
      XCTAssertThrowsError(try loadApplePrAcquisition(
         loader: loader,
         identity: BenchmarkArtifactIdentity(path: "acquisition/apple-pr.json", sha256: String(repeating: "0", count: 64))
      ))
   }

   func testApplePrPlanLoadsByExactRootHashAndRejectsScenarioManifestDrift() throws
   {
      let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
      defer {try? FileManager.default.removeItem(at: root)}
      for directory in ["plans", "acquisition", "audits", "budgets", "scenarios"]
      {
         try FileManager.default.createDirectory(at: root.appendingPathComponent(directory), withIntermediateDirectories: true)
      }
      let acquisition = Data("acquisition".utf8)
      try acquisition.write(to: root.appendingPathComponent("acquisition/apple-pr.json"))
      let budget = Data("{\"schema_version\":1,\"id\":\"apple-pr\",\"platform\":\"apple\",\"tier\":\"pr\"}".utf8)
      try budget.write(to: root.appendingPathComponent("budgets/apple-pr.json"))
      let comparatorAudit = Data("{\"status\":\"rejected\"}".utf8)
      try comparatorAudit.write(to: root.appendingPathComponent("audits/macos-appkit-native-production.json"))
      var scenarios = [ApplePrPlanScenario]()
      for id in applePrCanonicalScenarioIDs
      {
         let data = Data("{\"id\":\"\(id)\"}".utf8)
         let path = "scenarios/\(id).json"
         try data.write(to: root.appendingPathComponent(path))
         scenarios.append(ApplePrPlanScenario(
            id: id,
            artifact: BenchmarkArtifactIdentity(path: path, sha256: comparisonSHA256(data))
         ))
      }
      let plan = ApplePrPlanSpec(
         schemaVersion: 1,
         id: "apple-pr",
         platform: "apple",
         tier: "pr",
         acquisition: BenchmarkArtifactIdentity(path: "acquisition/apple-pr.json", sha256: comparisonSHA256(acquisition)),
         budget: BenchmarkArtifactIdentity(path: "budgets/apple-pr.json", sha256: comparisonSHA256(budget)),
         comparatorAudits: [ApplePrComparatorAuditBinding(
            identity: ApplePrComparatorIdentity(
               platform: "macos",
               framework: "appkit",
               implementation: "appkit-production",
               variant: "native.production"
            ),
            audit: BenchmarkArtifactIdentity(path: "audits/macos-appkit-native-production.json", sha256: comparisonSHA256(comparatorAudit))
         )],
         scenarios: scenarios
      )
      let planData = try JSONEncoder.comparisonCanonical.encode(plan)
      try planData.write(to: root.appendingPathComponent("plans/apple-pr.json"))
      let loader = BenchmarkSpecLoader(root: root)

      XCTAssertEqual(try loadApplePrPlan(loader: loader, expectedSHA256: comparisonSHA256(planData)), plan)
      XCTAssertThrowsError(try loadApplePrPlan(loader: loader, expectedSHA256: String(repeating: "0", count: 64)))

      let reorderedPlan = ApplePrPlanSpec(
         schemaVersion: plan.schemaVersion,
         id: plan.id,
         platform: plan.platform,
         tier: plan.tier,
         acquisition: plan.acquisition,
         budget: plan.budget,
         comparatorAudits: plan.comparatorAudits,
         scenarios: Array(plan.scenarios.reversed())
      )
      let reorderedData = try JSONEncoder.comparisonCanonical.encode(reorderedPlan)
      try reorderedData.write(to: root.appendingPathComponent("plans/apple-pr.json"))
      XCTAssertThrowsError(try loadApplePrPlan(loader: loader, expectedSHA256: comparisonSHA256(reorderedData)))

      try planData.write(to: root.appendingPathComponent("plans/apple-pr.json"))
      let changed = Data("{\"id\":\"startup.first-screen\",\"drift\":true}".utf8)
      try changed.write(to: root.appendingPathComponent("scenarios/startup.first-screen.json"))
      XCTAssertThrowsError(try loadApplePrPlan(loader: loader, expectedSHA256: comparisonSHA256(planData)))
      {
         XCTAssertEqual($0 as? BenchmarkContractFailure, .artifactHashMismatch("scenarios/startup.first-screen.json"))
      }
   }

   func testGenericAppleCampaignLoadsByExactHashAndFailsClosedOnMissingScenarioArtifact() throws
   {
      let sourceRoot = try comparisonSpecRoot()
      let staging = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
      let root = staging.appendingPathComponent("v1", isDirectory: true)
      defer {try? FileManager.default.removeItem(at: staging)}
      try FileManager.default.createDirectory(at: staging, withIntermediateDirectories: true)
      try FileManager.default.copyItem(at: sourceRoot, to: root)
      let plan = try genericAppleCampaignPlan(tier: .nightly, root: root)
      let data = try JSONEncoder.comparisonCanonical.encode(plan)
      let planPath = "plans/nightly-runtime.json"
      try data.write(to: root.appendingPathComponent(planPath))
      let identity = BenchmarkArtifactIdentity(path: planPath, sha256: comparisonSHA256(data))
      let loader = BenchmarkSpecLoader(root: root)

      let loaded = try loadAppleCampaignPlan(loader: loader, identity: identity)
      XCTAssertEqual(loaded, plan)
      XCTAssertEqual(
         try selectAppleCampaignExecution(plan: loaded, passID: "primary-presentation", requestedPackID: "core-interaction").scenarioBindings.map(\.id),
         ["dashboard.mixed-static", "chat.live-update", "navigation.modal"]
      )
      XCTAssertEqual(try selectAppleCampaignExecution(plan: loaded, passID: "idle", requestedPackID: nil).pack?.id, "soak-idle")
      XCTAssertEqual(try selectAppleCampaignExecution(plan: loaded, passID: "endurance", requestedPackID: nil).pack?.id, "soak-endurance")
      XCTAssertEqual(try selectAppleCampaignExecution(plan: loaded, passID: "attribution-time-profiler", requestedPackID: nil).pack, nil)
      XCTAssertEqual(try selectAppleCampaignExecution(plan: loaded, passID: "attribution-physical-footprint", requestedPackID: nil).pack, nil)
      XCTAssertThrowsError(try loadAppleCampaignPlan(
         loader: loader,
         identity: BenchmarkArtifactIdentity(path: planPath, sha256: String(repeating: "0", count: 64))
      ))

      try FileManager.default.removeItem(at: root.appendingPathComponent("scenarios/endurance.churn.json"))
      XCTAssertThrowsError(try loadAppleCampaignPlan(loader: loader, identity: identity))
   }

   func testGenericAppleReleaseAndFullAttributionSelectOnlyDeclaredPassesAndPacks() throws
   {
      let root = try comparisonSpecRoot()
      let release = try genericAppleCampaignPlan(tier: .releaseCore, root: root)
      try release.validate()
      XCTAssertEqual(try selectAppleCampaignExecution(plan: release, passID: "attribution-system-trace", requestedPackID: nil).scenarioBindings.count, 4)
      XCTAssertEqual(try selectAppleCampaignExecution(plan: release, passID: "common-gpu", requestedPackID: nil).scenarioBindings.count, 4)
      XCTAssertEqual(try selectAppleCampaignExecution(plan: release, passID: "attribution-physical-footprint", requestedPackID: nil).scenarioBindings.count, 4)
      XCTAssertEqual(try benchmarkGenericCampaignPackID(passID: "primary-presentation", requestedPackID: "scroll-damage"), "scroll-damage")
      XCTAssertEqual(try benchmarkGenericCampaignPackID(passID: "idle", requestedPackID: nil), "soak-idle")
      XCTAssertEqual(try benchmarkGenericCampaignPackID(passID: "endurance", requestedPackID: nil), "soak-endurance")
      XCTAssertEqual(try benchmarkGenericCampaignPackID(passID: "attribution-system-trace", requestedPackID: nil), "direct")
      XCTAssertEqual(try benchmarkGenericCampaignPackID(passID: "attribution-time-profiler", requestedPackID: "startup.first-screen"), "startup.first-screen")
      XCTAssertEqual(try benchmarkGenericCampaignPackID(passID: "common-gpu", requestedPackID: nil), "direct")
      XCTAssertEqual(try benchmarkGenericCampaignPackID(passID: "attribution-physical-footprint", requestedPackID: nil), "direct")
      XCTAssertThrowsError(try benchmarkGenericCampaignPackID(passID: "attribution-time-profiler", requestedPackID: ""))
      XCTAssertThrowsError(try selectAppleCampaignExecution(plan: release, passID: "primary-presentation", requestedPackID: nil))
      XCTAssertThrowsError(try selectAppleCampaignExecution(plan: release, passID: "common-gpu", requestedPackID: "scroll-damage"))

      let full = try genericAppleCampaignPlan(tier: .fullAttribution, root: root)
      try full.validate()
      XCTAssertEqual(try selectAppleCampaignExecution(plan: full, passID: "full-attribution", requestedPackID: nil).scenarioBindings.map(\.id), appleReleaseCampaignScenarioIDs)
      XCTAssertEqual(try benchmarkGenericCampaignPackID(passID: "full-attribution", requestedPackID: nil), "direct")
      XCTAssertThrowsError(try selectAppleCampaignExecution(plan: full, passID: "common-gpu", requestedPackID: nil))
   }

   func testPhaseSchedulerUsesElapsedTimeAndExactTraceBoundaries() throws
   {
      let source = URL(fileURLWithPath: #filePath)
      let workspace = source.deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
      let loader = BenchmarkSpecLoader(root: workspace.appendingPathComponent("benchmarks/comparative/specs/v1"))
      let scenario = try loader.loadScenario(relativePath: "scenarios/dashboard.mixed-static.json")
      var schedule = try BenchmarkPhaseSchedule(scenario: scenario, setupDurationUs: 1_000_000, loader: loader)
      XCTAssertEqual(schedule.durationUs, 9_000_000)
      var actions = [BenchmarkScheduledAction]()
      schedule.drain(until: 0) {actions.append($0)}
      XCTAssertEqual(actions.prefix(2), [.scenarioBegin(scenario.id), .phaseBegin("setup", measured: false)])
      XCTAssertFalse(schedule.isComplete)
      schedule.drain(until: schedule.durationUs) {actions.append($0)}
      XCTAssertTrue(schedule.isComplete)
      XCTAssertEqual(actions.last, .scenarioEnd(scenario.id))
      let events = actions.compactMap
      {
         action -> BenchmarkTraceEvent? in
         guard case .traceEvent(let event) = action else {return nil}
         return event
      }
      XCTAssertEqual(events.count, 21)
      XCTAssertEqual(events.first?.atUs, 0)
      XCTAssertEqual(events.last?.target, "dashboard:update-count")
   }

   func testPhaseSchedulerDerivesTraceOnlyStartupDurationAndCheckpointOrder() throws
   {
      let source = URL(fileURLWithPath: #filePath)
      let workspace = source.deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
      let loader = BenchmarkSpecLoader(root: workspace.appendingPathComponent("benchmarks/comparative/specs/v1"))
      let scenario = try loader.loadScenario(relativePath: "scenarios/startup.first-screen.json")
      var schedule = try BenchmarkPhaseSchedule(scenario: scenario, setupDurationUs: 0, loader: loader)
      XCTAssertEqual(schedule.durationUs, 1)
      var actions = [BenchmarkScheduledAction]()
      schedule.drain(until: schedule.durationUs) {actions.append($0)}
      XCTAssertTrue(schedule.isComplete)
      XCTAssertEqual(actions.compactMap
      {
         action -> String? in
         guard case .checkpoint(let id) = action else {return nil}
         return id
      }, ["terminated-ready", "fresh-install-ready", "warm-resume-ready"])
      let lastTraceIndex = try XCTUnwrap(actions.lastIndex
      {
         guard case .traceEvent(let event) = $0 else {return false}
         return event.atUs == 1
      })
      let lastCheckpointIndex = try XCTUnwrap(actions.lastIndex(of: .checkpoint("warm-resume-ready")))
      XCTAssertLessThan(lastTraceIndex, lastCheckpointIndex)
   }

   func testMacOSComparatorScaleOverlayDecodesScenarioIDAcronym() throws
   {
      let source = URL(fileURLWithPath: #filePath)
      let workspace = source.deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
      let root = workspace.appendingPathComponent("benchmarks/comparative/specs/v1")
      let loader = BenchmarkSpecLoader(root: root)
      let scenario = try loader.loadScenario(relativePath: "scenarios/startup.first-screen.json")
      let relativePath = "qualification/macos-scale/startup.first-screen.1x.json"
      let data = try Data(contentsOf: root.appendingPathComponent(relativePath))
      let overlay = try loader.loadMacOSComparatorScaleOverlay(
         BenchmarkArtifactIdentity(path: relativePath, sha256: comparisonSHA256(data)),
         scenario: scenario
      )
      XCTAssertEqual(overlay.scenarioID, scenario.id)
      XCTAssertEqual(overlay.scale, .oneX)
      XCTAssertEqual(overlay.effectiveCardinality, overlay.baseCardinality)
   }

   func testPhaseSchedulerClosesAPhaseBeforeStartingItsEqualTimestampSuccessor() throws
   {
      let source = URL(fileURLWithPath: #filePath)
      let workspace = source.deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
      let loader = BenchmarkSpecLoader(root: workspace.appendingPathComponent("benchmarks/comparative/specs/v1"))
      let scenario = try loader.loadScenario(relativePath: "scenarios/navigation.modal.json")
      var schedule = try BenchmarkPhaseSchedule(scenario: scenario, setupDurationUs: 1_000_000, loader: loader)
      var actions = [BenchmarkScheduledAction]()
      schedule.drain(until: schedule.durationUs) {actions.append($0)}
      let restored = try XCTUnwrap(actions.lastIndex(of: .checkpoint("list-restored")))
      let canonicalEnd = try XCTUnwrap(actions.lastIndex(of: .phaseEnd("canonical-cycles", measured: true)))
      let interactiveBegin = try XCTUnwrap(actions.lastIndex(of: .phaseBegin("interactive-cancel", measured: true)))
      let interactiveInput = try XCTUnwrap(actions.firstIndex
      {
         guard case .traceEvent(let event) = $0 else {return false}
         return event.op == "pointer-down" && event.atUs == 0
      })
      XCTAssertLessThan(restored, canonicalEnd)
      XCTAssertLessThan(canonicalEnd, interactiveBegin)
      XCTAssertLessThan(interactiveBegin, interactiveInput)
   }

   private func telemetryIdentity() -> BenchmarkTelemetryIdentity
   {
      BenchmarkTelemetryIdentity(
         planSHA256: String(repeating: "a", count: 64),
         generation: "generation",
         chunkID: "chunk",
         passID: "pass",
         sessionID: "session",
         timebaseNumerator: 1,
         timebaseDenominator: 1
      )
   }

   private func completeCommonTelemetryRing() -> BenchmarkTelemetryRing
   {
      let ring = BenchmarkTelemetryRing(capacity: 16)
      ring.append(kind: .readyToInput, timestamp: 1)
      ring.append(kind: .scenarioBegin, identifier: 1, timestamp: 2)
      ring.append(kind: .phaseBegin, identifier: 2, timestamp: 3)
      ring.append(kind: .checkpointBegin, identifier: 3, timestamp: 4)
      ring.append(kind: .checkpointEnd, identifier: 3, timestamp: 5)
      ring.append(kind: .displayOpportunity, identifier: 1, timestamp: 6)
      ring.append(kind: .callbackCadence, identifier: 1, timestamp: 7)
      ring.append(kind: .phaseEnd, identifier: 2, timestamp: 8)
      ring.append(kind: .scenarioEnd, identifier: 1, timestamp: 9)
      return ring
   }

   private func comparisonSpecRoot() throws -> URL
   {
      let source = URL(fileURLWithPath: #filePath)
      return source.deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().appendingPathComponent("benchmarks/comparative/specs/v1")
   }

   private func genericAppleCampaignPlan(tier: AppleCampaignTier, root: URL) throws -> AppleCampaignPlanSpec
   {
      let nightly = tier == .nightly
      let scenarioIDs = nightly ? appleNightlyCampaignScenarioIDs : appleReleaseCampaignScenarioIDs
      let scenarios = try scenarioIDs.map
      {
         id -> AppleCampaignScenarioBinding in
         let path = "scenarios/\(id).json"
         let url = root.appendingPathComponent(path)
         let hash = FileManager.default.fileExists(atPath: url.path)
            ? comparisonSHA256(try Data(contentsOf: url))
            : String(repeating: "a", count: 64)
         return AppleCampaignScenarioBinding(
            id: id,
            expectedPath: path,
            artifact: BenchmarkArtifactIdentity(path: path, sha256: hash)
         )
      }
      let packs = [
         AppleCampaignPackSpec(id: "launch", orderedScenarioIds: ["startup.first-screen"], isolatedProcess: true),
         AppleCampaignPackSpec(id: "core-interaction", orderedScenarioIds: nightly ? ["dashboard.mixed-static", "chat.live-update", "navigation.modal"] : ["dashboard.mixed-static", "chat.live-update", "navigation.modal", "resize.theme"], isolatedProcess: false),
         AppleCampaignPackSpec(id: "scroll-damage", orderedScenarioIds: nightly ? ["feed.variable-scroll"] : ["feed.variable-scroll", "grid.large-scroll", "mutation.damage"], isolatedProcess: false),
         AppleCampaignPackSpec(id: "media-text-warm", orderedScenarioIds: nightly ? ["image.decode-zoom"] : ["image.decode-zoom", "effects.layers", "text.multilingual"], isolatedProcess: false),
         AppleCampaignPackSpec(id: "soak-idle", orderedScenarioIds: ["idle.steady"], isolatedProcess: true),
         AppleCampaignPackSpec(id: "soak-endurance", orderedScenarioIds: ["endurance.churn"], isolatedProcess: true),
      ]
      let primaryScenarioIDs = nightly
         ? Array(appleNightlyCampaignScenarioIDs[1..<6])
         : Array(appleReleaseCampaignScenarioIDs[1..<11])
      let attributionScenarioIDs = tier == .fullAttribution
         ? appleReleaseCampaignScenarioIDs
         : ["dashboard.mixed-static", "feed.variable-scroll", "navigation.modal", "image.decode-zoom"]
      var passes = [
         AppleCampaignPassSpec(id: "correctness", role: .correctness, evidenceRole: .correctnessOnly, pairCount: 0, packIds: [], scenarioIds: scenarioIDs, launchClasses: [], collector: nil),
         AppleCampaignPassSpec(id: "primary-presentation", role: .primary, evidenceRole: .claimBearing, pairCount: nightly ? 6 : 12, packIds: ["core-interaction", "scroll-damage", "media-text-warm"], scenarioIds: primaryScenarioIDs, launchClasses: [], collector: nil),
         AppleCampaignPassSpec(id: "canonical-launch", role: .launch, evidenceRole: .claimBearing, pairCount: nightly ? 10 : 14, packIds: ["launch"], scenarioIds: ["startup.first-screen"], launchClasses: ["terminated-warm-system-cache:8", "fresh-install-first-launch:2"], collector: nil),
         AppleCampaignPassSpec(id: "idle", role: .idle, evidenceRole: .descriptiveDiagnostic, pairCount: 2, packIds: ["soak-idle"], scenarioIds: ["idle.steady"], launchClasses: [], collector: nil),
         AppleCampaignPassSpec(id: "endurance", role: .endurance, evidenceRole: .descriptiveDiagnostic, pairCount: 2, packIds: ["soak-endurance"], scenarioIds: ["endurance.churn"], launchClasses: [], collector: nil),
      ]
      if !nightly
      {
         passes.append(AppleCampaignPassSpec(id: "energy", role: .energy, evidenceRole: .descriptiveDiagnostic, pairCount: 5, packIds: [], scenarioIds: ["dashboard.mixed-static"], launchClasses: [], collector: nil))
      }
      passes.append(AppleCampaignPassSpec(id: "attribution-time-profiler", role: .attribution, evidenceRole: .descriptiveDiagnostic, pairCount: 3, packIds: [], scenarioIds: attributionScenarioIDs, launchClasses: [], collector: "time-profiler"))
      if !nightly
      {
         passes.append(AppleCampaignPassSpec(id: "attribution-system-trace", role: .attribution, evidenceRole: .descriptiveDiagnostic, pairCount: 3, packIds: [], scenarioIds: attributionScenarioIDs, launchClasses: [], collector: "system-trace"))
         let gpuID = tier == .fullAttribution ? "full-attribution" : "common-gpu"
         passes.append(AppleCampaignPassSpec(id: gpuID, role: .attribution, evidenceRole: .descriptiveDiagnostic, pairCount: 3, packIds: [], scenarioIds: attributionScenarioIDs, launchClasses: [], collector: gpuID))
      }
      passes.append(AppleCampaignPassSpec(id: "attribution-physical-footprint", role: .attribution, evidenceRole: .descriptiveDiagnostic, pairCount: 3, packIds: [], scenarioIds: attributionScenarioIDs, launchClasses: [], collector: "physical-footprint"))
      let attributionPassIDs = passes.filter {$0.role == .attribution}.map(\.id)
      let budgetComponents = [
         AppleCampaignBudgetComponentSpec(component: .correctnessInstallPulls, occupiedSeconds: 1, passIds: ["correctness"]),
         AppleCampaignBudgetComponentSpec(component: .primaryDynamicPresentation, occupiedSeconds: 1, passIds: ["primary-presentation"]),
         AppleCampaignBudgetComponentSpec(component: .launchOrStartupDelivery, occupiedSeconds: 1, passIds: ["canonical-launch"]),
         AppleCampaignBudgetComponentSpec(component: .idleEndurance, occupiedSeconds: 1, passIds: ["idle", "endurance"]),
         AppleCampaignBudgetComponentSpec(component: .energy, occupiedSeconds: nightly ? 0 : 1, passIds: nightly ? [] : ["energy"]),
         AppleCampaignBudgetComponentSpec(component: .attribution, occupiedSeconds: 1, passIds: attributionPassIDs),
      ]
      let planID: String
      switch tier
      {
      case .nightly: planID = "nightly-apple"
      case .releaseCore: planID = "apple-release-core"
      case .claimComplete: planID = "apple-release-claim-complete"
      case .fullAttribution: planID = "apple-full-attribution-audit"
      default: planID = "apple-extended-audit"
      }
      let readiness: UInt64 = nightly ? 20 : 30
      let warmup: UInt64 = nightly ? 3 : 5
      let duration: UInt64 = nightly ? 12 : 20
      let navigationIterations: UInt32 = nightly ? 10 : 20
      let timing = AppleCampaignTimingSpec(passes: passes.filter
      {
         $0.role == .primary || $0.role == .attribution || $0.role == .idle || $0.role == .endurance
      }.map
      {
         pass in
         let passWarmup = pass.role == .idle ? UInt64(3) : (pass.role == .endurance ? UInt64(2) : warmup)
         let passDuration = pass.role == .idle ? UInt64(60) : (pass.role == .endurance ? UInt64(300) : duration)
         return AppleCampaignPassTimingSpec(
            passId: pass.id,
            resetSecondsPerSession: 5,
            readinessTimeoutSeconds: readiness,
            scenarios: pass.scenarioIds.map
            {
               scenarioID in
               AppleCampaignScenarioTimingSpec(
                  scenarioId: scenarioID,
                  setupSeconds: 1,
                  warmupSeconds: passWarmup,
                  measurement: expectedTimingMeasurement(
                     scenarioID: scenarioID,
                     duration: passDuration,
                     navigationIterations: navigationIterations
                  )
               )
            }
         )
      })
      return AppleCampaignPlanSpec(
         schemaVersion: 1,
         id: planID,
         platform: "apple",
         tier: tier,
         budgetId: planID,
         timing: timing,
         scenarios: scenarios,
         packs: packs,
         passes: passes,
         budgetComponents: budgetComponents
      )
   }
}

private extension Data
{
   var withSHA256: Data
   {
      Data(SHA256.hash(data: self))
   }
}

final class OxideMacResponderContractTests: XCTestCase
{
   func testControlledMacOSCampaignsDetachPreparedContentUntilTheControllerStartsMeasurement() throws
   {
      let test = URL(fileURLWithPath: #filePath)
      let root = test.deletingLastPathComponent().deletingLastPathComponent()
      for relativePath in ["Oxide-macOS/main.swift", "AppKit-macOS/main.swift"]
      {
         let source = try String(contentsOf: root.appendingPathComponent(relativePath), encoding: .utf8)
         let prepare = try XCTUnwrap(source.range(of: "try prepareMacOSCampaign(executor, invocation: invocation, store: store)")?.lowerBound)
         let detached = try XCTUnwrap(source.range(of: "MacOSControlledStartWindowBarrier(window: window)", range: prepare..<source.endIndex)?.lowerBound)
         let start = try XCTUnwrap(source.range(of: "onStart:", range: detached..<source.endIndex)?.lowerBound)
         let restored = try XCTUnwrap(source.range(of: "startBarrier?.restore()", range: start..<source.endIndex)?.lowerBound)
         XCTAssertLessThan(prepare, detached)
         XCTAssertLessThan(detached, start)
         XCTAssertLessThan(start, restored)
      }
   }

   func testOxideMacSurfaceRoutesPublicResponderEventsIntoRust() throws
   {
      let source = try oxideMacAdapterSource()
      for required in [
         "override func mouseDown(with event: NSEvent)",
         "override func mouseDragged(with event: NSEvent)",
         "override func mouseUp(with event: NSEvent)",
         "override func scrollWheel(with event: NSEvent)",
         "override func keyDown(with event: NSEvent)",
         "oxideComparisonMacOSPointerEvent",
         "oxideComparisonMacOSScroll",
         "oxideComparisonMacOSKey",
         "oxideComparisonInteractionGeneration()",
      ]
      {
         XCTAssertTrue(source.contains(required), "missing responder contract \(required)")
      }
   }

   func testOxideMacXCUIElementsComeFromRustSemanticGeometry() throws
   {
      let source = try oxideMacAdapterSource()
      XCTAssertTrue(source.contains("oxideComparisonGeometryNodesJSON"))
      XCTAssertTrue(source.contains("metalView.updateAccessibility"))
      XCTAssertTrue(source.contains("NSAccessibilityElement()"))
      XCTAssertTrue(source.contains("element.setAccessibilityIdentifier(node.identifier"))
      XCTAssertTrue(source.contains("identifier: \"navigation.modal-action\""))
      XCTAssertFalse(source.contains("MacOSTrustedInputCommand"))
   }

   func testGenerationIsPublishedOnlyAfterRenderingAndAccessibilityRefreshSucceed() throws
   {
      let source = try oxideMacAdapterSource()
      let render = try XCTUnwrap(source.range(of: "try self.renderFrame()")?.lowerBound)
      let accessibility = try XCTUnwrap(source.range(of: "try self.refreshAccessibility()", range: render..<source.endIndex)?.lowerBound)
      let generation = try XCTUnwrap(source.range(of: "self.presentedInteractionGeneration = oxideComparisonInteractionGeneration()", range: accessibility..<source.endIndex)?.lowerBound)
      XCTAssertLessThan(render, accessibility)
      XCTAssertLessThan(accessibility, generation)
      XCTAssertTrue(source.contains("self.inputPresentationFailure = error"))
   }

   func testChatKeyboardInputUsesNSEventKeyCodesWithoutTraceReplay() throws
   {
      let source = try oxideMacAdapterSource()
      let start = try XCTUnwrap(source.range(of: "override func keyDown(with event: NSEvent)")?.lowerBound)
      let end = try XCTUnwrap(source.range(of: "func updateAccessibility", range: start..<source.endIndex)?.lowerBound)
      let keyDown = source[start..<end]
      XCTAssertTrue(keyDown.contains("event.keyCode"))
      XCTAssertTrue(keyDown.contains("event.modifierFlags.contains(.shift)"))
      XCTAssertTrue(keyDown.contains("oxideComparisonMacOSKey"))
      XCTAssertFalse(keyDown.contains("oxideComparisonApplyTraceEvent"))
   }

   func testPointerDragsUsePublicResponderEventsAndTheRustRuntimeOnly() throws
   {
      let source = try oxideMacAdapterSource()
      for method in ["mouseDown", "mouseDragged", "mouseUp"]
      {
         let start = try XCTUnwrap(source.range(of: "override func \(method)(with event: NSEvent)")?.lowerBound)
         let end = try XCTUnwrap(source.range(of: "override func", range: source.index(after: start)..<source.endIndex)?.lowerBound)
         let body = source[start..<end]
         XCTAssertTrue(body.contains("oxideComparisonMacOSPointerEvent"))
         XCTAssertTrue(body.contains("accept("))
         XCTAssertFalse(body.contains("oxideComparisonApplyTraceEvent"))
         XCTAssertFalse(body.contains("MacOSTrustedInputCommand"))
      }
   }

   private func oxideMacAdapterSource() throws -> String
   {
      let test = URL(fileURLWithPath: #filePath)
      let source = test.deletingLastPathComponent().deletingLastPathComponent().appendingPathComponent("Oxide-macOS/OxideScenarioAdapter.swift")
      return try String(contentsOf: source, encoding: .utf8)
   }
}
