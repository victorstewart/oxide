import Foundation
import XCTest

final class ComparisonControllerUITests: XCTestCase
{
   func testManifestCampaign() throws
   {
      let environment = ProcessInfo.processInfo.environment
      if environment["OXIDE_COMPARISON_CHUNK"] != nil
      {
         try runManifestChunk(environment: environment)
         return
      }
      let transportMode = ComparisonTransportMode(rawValue: environment["OXIDE_COMPARISON_TRANSPORT"] ?? "") ?? .appGroup
      if transportMode == .perAppContainer
      {
         try runPerAppContainerSide(environment: environment)
         return
      }
      let root = try DurableArtifactStore.root(for: transportMode)
      let store = DurableArtifactStore(root: root)
      let runID = "phase0-\(UUID().uuidString.lowercased())"
      let generation = UUID().uuidString.lowercased()
      let request = ComparisonProbeRequest(
         schemaVersion: 1,
         runID: runID,
         planSHA256: String(repeating: "a", count: 64),
         passID: "phase0-transport",
         pairIndex: 0,
         side: .oxide,
         generation: generation,
         transportMode: transportMode,
         predecessorSHA256: nil
      )
      let pairRoot = "Runs/\(runID)/\(request.passID)/\(request.pairIndex)"

      let stale = ComparisonProbeEnvelope(
         schemaVersion: 1,
         runID: runID,
         planSHA256: request.planSHA256,
         passID: request.passID,
         pairIndex: request.pairIndex,
         side: .oxide,
         generation: "stale-generation",
         predecessorSHA256: nil,
         payloadSHA256: String(repeating: "0", count: 64)
      )
      _ = try store.durableJSON(stale, relativePath: "\(pairRoot)/oxide.json")
      XCTAssertThrowsError(try validateComparisonEnvelope(stale, request: request))
      _ = try store.durableJSON(request, relativePath: "\(pairRoot)/controller.seed.json")

      try runSide(
         bundleIdentifier: "com.oxide.comparison.oxidebenchios",
         request: request,
         expectedAcknowledgement: "\(pairRoot)/oxide.ack.json",
         store: store
      )
      let oxide = try store.readJSON(ComparisonProbeEnvelope.self, relativePath: "\(pairRoot)/oxide.json")
      try validateComparisonEnvelope(oxide, request: request)

      let nativeRequest = ComparisonProbeRequest(
         schemaVersion: request.schemaVersion,
         runID: request.runID,
         planSHA256: request.planSHA256,
         passID: request.passID,
         pairIndex: request.pairIndex,
         side: .native,
         generation: request.generation,
         transportMode: request.transportMode,
         predecessorSHA256: nil
      )
      try runSide(
         bundleIdentifier: "com.oxide.comparison.uikitbenchios",
         request: nativeRequest,
         expectedAcknowledgement: "\(pairRoot)/native.ack.json",
         store: store
      )
      let native = try store.readJSON(ComparisonProbeEnvelope.self, relativePath: "\(pairRoot)/native.json")
      try validateComparisonEnvelope(native, request: nativeRequest)
      XCTAssertEqual(native.predecessorSHA256, comparisonSHA256(try JSONEncoder.comparisonCanonical.encode(oxide)))

      let pairComplete = ComparisonPairComplete(
         schemaVersion: 1,
         runID: runID,
         pairIndex: request.pairIndex,
         generation: generation,
         oxideSHA256: comparisonSHA256(try JSONEncoder.comparisonCanonical.encode(oxide)),
         nativeSHA256: comparisonSHA256(try JSONEncoder.comparisonCanonical.encode(native))
      )
      _ = try store.durableJSON(pairComplete, relativePath: "\(pairRoot)/pair.complete.json")
      let runnerRoot = try XCTUnwrap(FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first)
      _ = try DurableArtifactStore(root: runnerRoot).durableJSON(pairComplete, relativePath: "ComparisonRuns/\(runID)/pair.complete.json")
   }

   func testLaunchCampaign() throws
   {
      let environment = ProcessInfo.processInfo.environment
      guard environment["OXIDE_COMPARISON_CHUNK"] != nil else
      {
         throw XCTSkip("Launch acquisition requires a frozen campaign chunk environment.")
      }
      let identity = try campaignIdentity(environment: environment)
      guard identity.passID == "canonical-launch", !identity.pairIndices.isEmpty else
      {
         throw BenchmarkCampaignFailure.invalidPack(identity.passID)
      }
      for pairIndex in identity.pairIndices
      {
         for side in benchmarkCampaignPairOrder(pairIndex)
         {
            try runLaunchSide(identity: identity, pairIndex: pairIndex, side: side, packID: "pr-launch")
         }
      }
   }

   private func runManifestChunk(environment: [String: String]) throws
   {
      let identity = try campaignIdentity(environment: environment)
      if identity.passID == "correctness"
      {
         guard identity.pairIndices.isEmpty else
         {
            throw BenchmarkCampaignFailure.invalidPack("correctness-pairs")
         }
         for run in try benchmarkCorrectnessPackRuns(["pr-non-launch", "pr-launch"])
         {
            for side in benchmarkCampaignPairOrder(run.executionIndex)
            {
               try runManifestSide(identity: identity, pairIndex: run.executionIndex, side: side, packID: run.packID)
            }
         }
         return
      }
      guard identity.passID == "minimal-presentation", !identity.pairIndices.isEmpty else
      {
         throw BenchmarkCampaignFailure.invalidPack(identity.passID)
      }
      for pairIndex in identity.pairIndices
      {
         for side in benchmarkCampaignPairOrder(pairIndex)
         {
            try runManifestSide(identity: identity, pairIndex: pairIndex, side: side, packID: "pr-non-launch")
         }
      }
   }

   private func runManifestSide(identity: ControllerCampaignIdentity, pairIndex: UInt32, side: ComparisonSide, packID: String) throws
   {
      let application = campaignApplication(side: side)
      application.launchArguments = campaignLaunchArguments(identity: identity, pairIndex: pairIndex, side: side, packID: packID)
      let generation = campaignGeneration(identity: identity, pairIndex: pairIndex, side: side)
      let readyName = comparisonGenerationNotification(comparisonReadyNotification, generation: generation)
      let startName = comparisonGenerationNotification(comparisonStartNotification, generation: generation)
      let completeName = comparisonGenerationNotification(comparisonCompleteNotification, generation: generation)
      let launchT0 = mach_continuous_time()
      let ready = runWaitingForComparisonDarwinNotification(readyName, timeout: .seconds(15))
      {
         application.launch()
      }
      let readyTimestamp = mach_continuous_time()
      XCTAssertTrue(ready, "Timed out waiting for campaign readiness")
      let complete = runWaitingForComparisonDarwinNotification(completeName, timeout: .seconds(75))
      {
         postComparisonDarwinNotification(startName)
      }
      let completeTimestamp = mach_continuous_time()
      application.terminate()
      XCTAssertTrue(complete, "Timed out waiting for campaign completion")
      try persistControllerReceipt(
         identity: identity,
         pairIndex: pairIndex,
         side: side,
         launchT0: launchT0,
         readyTimestamp: readyTimestamp,
         completeTimestamp: complete ? completeTimestamp : nil,
         complete: complete,
         packID: packID,
         primaryAvailability: identity.passID == "correctness"
            ? "not-applicable-correctness-untimed"
            : "unavailable-common-presentation-trace-not-attached"
      )
   }

   private func runLaunchSide(identity: ControllerCampaignIdentity, pairIndex: UInt32, side: ComparisonSide, packID: String) throws
   {
      let application = campaignApplication(side: side)
      application.launchArguments = campaignLaunchArguments(identity: identity, pairIndex: pairIndex, side: side, packID: packID)
      let generation = campaignGeneration(identity: identity, pairIndex: pairIndex, side: side)
      let readyName = comparisonGenerationNotification(comparisonReadyNotification, generation: generation)
      var launchT0 = UInt64(0)
      let ready = runWaitingForComparisonDarwinNotification(readyName, timeout: .seconds(15))
      {
         launchT0 = mach_continuous_time()
         application.launch()
      }
      let readyTimestamp = mach_continuous_time()
      application.terminate()
      XCTAssertTrue(ready, "Timed out waiting for startup readiness")
      try persistControllerReceipt(
         identity: identity,
         pairIndex: pairIndex,
         side: side,
         launchT0: launchT0,
         readyTimestamp: readyTimestamp,
         completeTimestamp: nil,
         complete: ready,
         packID: packID,
         primaryAvailability: "unavailable-attributed-presentation-and-clock-map-pending"
      )
   }

   private func campaignIdentity(environment: [String: String]) throws -> ControllerCampaignIdentity
   {
      func required(_ name: String) throws -> String
      {
         try XCTUnwrap(environment[name], "Missing \(name) in frozen xctestrun environment")
      }
      let pairs = try required("OXIDE_COMPARISON_PAIR_INDICES").split(separator: ",").map
      {
         value -> UInt32 in
         try XCTUnwrap(UInt32(value), "Invalid campaign pair index \(value)")
      }
      return ControllerCampaignIdentity(
         planSHA256: try required("OXIDE_COMPARISON_PLAN_SHA"),
         runID: try required("OXIDE_COMPARISON_RUN_ID"),
         chunkID: try required("OXIDE_COMPARISON_CHUNK"),
         passID: try required("OXIDE_COMPARISON_PASS_ID"),
         pairIndices: pairs
      )
   }

   private func campaignApplication(side: ComparisonSide) -> XCUIApplication
   {
      XCUIApplication(bundleIdentifier: side == .oxide
         ? "com.oxide.comparison.oxidebenchios"
         : "com.oxide.comparison.uikitbenchios")
   }

   private func campaignLaunchArguments(identity: ControllerCampaignIdentity, pairIndex: UInt32, side: ComparisonSide, packID: String) -> [String]
   {
      [
         "-oxide-compare-plan-sha", identity.planSHA256,
         "-oxide-compare-run-id", identity.runID,
         "-oxide-compare-chunk", identity.chunkID,
         "-oxide-compare-pass", identity.passID,
         "-oxide-compare-pack", packID,
         "-oxide-compare-pair", String(pairIndex),
         "-oxide-compare-side", side.rawValue,
      ]
   }

   private func persistControllerReceipt(identity: ControllerCampaignIdentity, pairIndex: UInt32, side: ComparisonSide, launchT0: UInt64, readyTimestamp: UInt64, completeTimestamp: UInt64?, complete: Bool, packID: String, primaryAvailability: String) throws
   {
      let root = try XCTUnwrap(FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first)
      let receipt = BenchmarkControllerReceipt(
         schemaVersion: 1,
         runID: identity.runID,
         planSHA256: identity.planSHA256,
         chunkID: identity.chunkID,
         passID: identity.passID,
         pairIndex: UInt64(pairIndex),
         side: side,
         generation: campaignGeneration(identity: identity, pairIndex: pairIndex, side: side),
         packID: packID,
         launchT0: launchT0,
         readyTimestamp: readyTimestamp,
         completeTimestamp: completeTimestamp,
         complete: complete,
         primaryAvailability: primaryAvailability
      )
      _ = try DurableArtifactStore(root: root).durableJSON(
         receipt,
         relativePath: "ComparisonRuns/\(identity.runID)/\(identity.chunkID)/\(pairIndex).\(side.rawValue).controller.json"
      )
   }

   private func campaignGeneration(identity: ControllerCampaignIdentity, pairIndex: UInt32, side: ComparisonSide) -> String
   {
      benchmarkCampaignGeneration(
         planSHA256: identity.planSHA256,
         runID: identity.runID,
         chunkID: identity.chunkID,
         passID: identity.passID,
         pairIndex: UInt64(pairIndex),
         side: side
      )
   }

   private func runSide(bundleIdentifier: String, request: ComparisonProbeRequest, expectedAcknowledgement: String, store: DurableArtifactStore) throws
   {
      let application = XCUIApplication(bundleIdentifier: bundleIdentifier)
      application.launchArguments = [
         "-oxide-compare-plan-sha", request.planSHA256,
         "-oxide-compare-run-id", request.runID,
         "-oxide-compare-pass", request.passID,
         "-oxide-compare-pair", String(request.pairIndex),
         "-oxide-compare-side", request.side.rawValue,
         "-oxide-compare-generation", request.generation,
         "-oxide-compare-transport", request.transportMode.rawValue,
      ]
      application.launch()
      let deadline = Date().addingTimeInterval(15)
      while Date() < deadline
      {
         if let acknowledgement = try? store.readJSON(ComparisonProbeAcknowledgement.self, relativePath: expectedAcknowledgement),
            acknowledgement.generation == request.generation,
            acknowledgement.durable
         {
            application.terminate()
            return
         }
         Thread.sleep(forTimeInterval: 0.05)
      }
      application.terminate()
      XCTFail("Timed out waiting for durable acknowledgement \(expectedAcknowledgement)")
   }

   private func runPerAppContainerSide(environment: [String: String]) throws
   {
      func required(_ name: String) throws -> String
      {
         try XCTUnwrap(environment[name], "Missing \(name) in frozen xctestrun environment")
      }

      guard let side = ComparisonSide(rawValue: try required("OXIDE_COMPARISON_SIDE")),
            let pairIndex = UInt64(try required("OXIDE_COMPARISON_PAIR_INDEX")) else
      {
         throw ComparisonContractError.invalidArgument("per-app controller environment")
      }
      let request = ComparisonProbeRequest(
         schemaVersion: 1,
         runID: try required("OXIDE_COMPARISON_RUN_ID"),
         planSHA256: try required("OXIDE_COMPARISON_PLAN_SHA"),
         passID: try required("OXIDE_COMPARISON_PASS_ID"),
         pairIndex: pairIndex,
         side: side,
         generation: try required("OXIDE_COMPARISON_GENERATION"),
         transportMode: .perAppContainer,
         predecessorSHA256: environment["OXIDE_COMPARISON_PREDECESSOR_SHA"]
      )
      let bundleIdentifier = side == .oxide
         ? "com.oxide.comparison.oxidebenchios"
         : "com.oxide.comparison.uikitbenchios"
      let application = XCUIApplication(bundleIdentifier: bundleIdentifier)
      application.launchArguments = [
         "-oxide-compare-plan-sha", request.planSHA256,
         "-oxide-compare-run-id", request.runID,
         "-oxide-compare-pass", request.passID,
         "-oxide-compare-pair", String(request.pairIndex),
         "-oxide-compare-side", request.side.rawValue,
         "-oxide-compare-generation", request.generation,
         "-oxide-compare-transport", request.transportMode.rawValue,
      ]
      if let predecessor = request.predecessorSHA256
      {
         application.launchArguments.append(contentsOf: ["-oxide-compare-predecessor-sha", predecessor])
      }
      let completed = runWaitingForComparisonDarwinNotification(
         comparisonGenerationNotification(comparisonCompleteNotification, generation: request.generation),
         timeout: .seconds(15)
      )
      {
         application.launch()
      }
      application.terminate()
      XCTAssertTrue(completed, "Timed out waiting for comparator completion control signal")
   }
}

private struct ComparisonPairComplete: Codable
{
   let schemaVersion: Int
   let runID: String
   let pairIndex: UInt64
   let generation: String
   let oxideSHA256: String
   let nativeSHA256: String
}

private struct ControllerCampaignIdentity
{
   let planSHA256: String
   let runID: String
   let chunkID: String
   let passID: String
   let pairIndices: [UInt32]
}
