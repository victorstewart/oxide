import AppKit
import Darwin
import Foundation
import os
import XCTest

private struct MacOSLaunchProbeDescriptor: Decodable
{
   let schemaVersion: UInt32
   let probeID: String
   let dispatchPath: String
   let targetIdentity: String
   let actionIdentity: String
   let windowNumber: Int
   let xPoints: Double
   let yPoints: Double
}

private struct MacOSLaunchReadinessReceipt: Decodable
{
   let trustedInputOffsetNs: UInt64
   let trustedInputDeadlineNs: UInt64
   let probe: MacOSLaunchProbeDescriptor
   let durable: Bool
}

private struct MacOSLaunchControllerClockAnchor: Codable
{
   let id: UInt32
   let beforeTicks: UInt64
   let afterTicks: UInt64
}

private struct MacOSLaunchControllerReceipt: Codable
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
   let launchClass: String
   let mode: String
   let launchSource: String
   let inputSource: String?
   let launchRequestTicks: UInt64
   let readyObservedTicks: UInt64
   let inputRequestTicks: UInt64?
   let completeObservedTicks: UInt64?
   let clockAnchors: [MacOSLaunchControllerClockAnchor]
   let installedBundlePath: String
   let dataContainerPath: String?
   let dataContainerWasAbsent: Bool?
   let installIdentityClaimed: Bool?
   let initialProcessIdentifier: Int32
   let backgroundObservedTicks: UInt64?
   let suspendedObservedTicks: UInt64?
   let resumedProcessIdentifier: Int32?
   let resumedSameProcess: Bool?
   let appWasTerminated: Bool
   let complete: Bool
}

private struct MacOSLaunchControllerStartReceipt: Codable
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
   let launchClass: String
   let launchSource: String
   let launchRequestTicks: UInt64
   let readyObservedTicks: UInt64
   let firstClockAnchor: MacOSLaunchControllerClockAnchor
   let complete: Bool
}

final class MacOSComparisonControllerUITests: XCTestCase
{
   func testExactForegroundSession() throws
   {
      let environment = ProcessInfo.processInfo.environment
      func required(_ name: String) throws -> String
      {
         guard let value = environment[name], !value.isEmpty else
         {
            throw XCTSkip("Missing macOS comparison controller environment \(name)")
         }
         return value
      }

      let applicationURL = URL(fileURLWithPath: try required("OXIDE_COMPARISON_APP_BUNDLE"), isDirectory: true)
      guard applicationURL.pathExtension == "app",
            FileManager.default.fileExists(atPath: applicationURL.path) else
      {
         throw BenchmarkCampaignFailure.invalidPlan
      }
      let planSHA256 = try required("OXIDE_COMPARISON_PLAN_SHA")
      let runID = try required("OXIDE_COMPARISON_RUN_ID")
      let chunkID = try required("OXIDE_COMPARISON_CHUNK")
      let passID = try required("OXIDE_COMPARISON_PASS_ID")
      let packID = try required("OXIDE_COMPARISON_PACK_ID")
      let planPath = environment["OXIDE_COMPARISON_PLAN_PATH"]
      let pairText = try required("OXIDE_COMPARISON_PAIR_INDEX")
      let sideText = try required("OXIDE_COMPARISON_SIDE")
      guard let pairIndex = UInt64(pairText), let side = ComparisonSide(rawValue: sideText) else
      {
         throw BenchmarkCampaignFailure.invalidPlan
      }
      let generation = try required("OXIDE_COMPARISON_GENERATION")
      try validateComparisonSHA256(generation)
      if passID == "canonical-launch"
      {
         try runCanonicalLaunch(
            environment: environment,
            applicationURL: applicationURL,
            outputRoot: URL(fileURLWithPath: try required("OXIDE_COMPARISON_OUTPUT_ROOT"), isDirectory: true),
            planSHA256: planSHA256,
            runID: runID,
            chunkID: chunkID,
            packID: packID,
            planPath: planPath,
            pairText: pairText,
            pairIndex: pairIndex,
            sideText: sideText,
            side: side,
            generation: generation
         )
         return
      }
      let outputRoot = URL(fileURLWithPath: try required("OXIDE_COMPARISON_OUTPUT_ROOT"), isDirectory: true)
      let controllerRoot = FileManager.default.temporaryDirectory
         .appendingPathComponent("oxide-comparison-\(generation)-\(ProcessInfo.processInfo.globallyUniqueString)", isDirectory: true)
      try FileManager.default.createDirectory(at: controllerRoot, withIntermediateDirectories: false)
      let application = XCUIApplication(url: applicationURL)
      defer
      {
         if !application.wait(for: .notRunning, timeout: 5)
         {
            application.terminate()
            _ = application.wait(for: .notRunning, timeout: 5)
         }
         try? FileManager.default.removeItem(at: controllerRoot)
      }
      var launchArguments = [
         "-oxide-compare-plan-sha", planSHA256,
         "-oxide-compare-run-id", runID,
         "-oxide-compare-chunk", chunkID,
         "-oxide-compare-pass", passID,
         "-oxide-compare-pack", packID,
         "-oxide-compare-pair", pairText,
         "-oxide-compare-side", sideText,
         "-oxide-compare-generation", generation,
         "-oxide-compare-output-root", outputRoot.path,
         "-oxide-compare-controller-root", controllerRoot.path,
         "-oxide-compare-controlled-start",
      ]
      if let planPath
      {
         launchArguments.insert(contentsOf: ["-oxide-compare-plan-path", planPath], at: launchArguments.count - 1)
      }
      if let path = environment["OXIDE_COMPARISON_SCALE_OVERLAY_PATH"],
         let sha256 = environment["OXIDE_COMPARISON_SCALE_OVERLAY_SHA"]
      {
         launchArguments.insert(contentsOf: ["-oxide-compare-scale-overlay-path", path, "-oxide-compare-scale-overlay-sha", sha256], at: launchArguments.count - 1)
      }
      application.launchArguments = launchArguments
      let readyArtifact = outputRoot
         .appendingPathComponent("Runs", isDirectory: true)
         .appendingPathComponent(runID, isDirectory: true)
         .appendingPathComponent(chunkID, isDirectory: true)
         .appendingPathComponent(passID, isDirectory: true)
         .appendingPathComponent(packID, isDirectory: true)
         .appendingPathComponent(pairText, isDirectory: true)
         .appendingPathComponent("\(sideText).ready.json")
      let ready = runWaitingForComparisonDarwinNotificationOrArtifact(
         comparisonGenerationNotification(comparisonReadyNotification, generation: generation),
         artifactURL: readyArtifact,
         timeout: 15
      )
      {
         application.launch()
      }
      XCTAssertTrue(ready, "Exact macOS comparison app did not reach its start barrier")
      XCTAssertNotEqual(application.state, .notRunning)
      let controller = try MacOSTrustedInputXCUIController(
         application: application,
         outputRoot: outputRoot,
         controlRoot: controllerRoot,
         expectation: MacOSTrustedInputControllerExpectation(
            runID: runID,
            planSHA256: planSHA256,
            chunkID: chunkID,
            passID: passID,
            packID: packID,
            pairIndex: pairIndex,
            side: side,
            generation: generation
         )
      )
      if environment["OXIDE_COMPARISON_AUTO_START"] == "1"
      {
         postComparisonDarwinNotification(comparisonGenerationNotification(comparisonStartNotification, generation: generation))
      }
      let timeout = TimeInterval(try required("OXIDE_COMPARISON_TIMEOUT_SECONDS")) ?? 90
      do
      {
         try controller.serviceUntilApplicationExits(timeout: timeout)
      }
      catch
      {
         let directory = outputRoot
            .appendingPathComponent("Runs", isDirectory: true)
            .appendingPathComponent(runID, isDirectory: true)
            .appendingPathComponent(chunkID, isDirectory: true)
            .appendingPathComponent(passID, isDirectory: true)
            .appendingPathComponent(packID, isDirectory: true)
            .appendingPathComponent(pairText, isDirectory: true)
         let failure = directory.appendingPathComponent("\(sideText).failure.txt")
         let controllerFailure = directory.appendingPathComponent("\(sideText).controller.failure.txt")
         try? DurableArtifactStore(root: outputRoot).durableWrite(Data("controller: \(String(describing: error))".utf8), to: controllerFailure)
         if !FileManager.default.fileExists(atPath: failure.path)
         {
            try? DurableArtifactStore(root: outputRoot).durableWrite(Data("controller: \(String(describing: error))".utf8), to: failure)
         }
         throw error
      }
   }

   private func runCanonicalLaunch(environment: [String: String], applicationURL: URL, outputRoot: URL, planSHA256: String, runID: String, chunkID: String, packID: String, planPath: String?, pairText: String, pairIndex: UInt64, sideText: String, side: ComparisonSide, generation: String) throws
   {
      let mode = environment["OXIDE_COMPARISON_LAUNCH_MODE"] ?? "measure"
      let launchClass = environment["OXIDE_COMPARISON_LAUNCH_CLASS"] ?? "terminated-warm-system-cache"
      guard mode == "cache-primer" || mode == "measure" else
      {
         throw BenchmarkCampaignFailure.invalidPlan
      }
      guard ["terminated-warm-system-cache", "fresh-install-first-launch", "warm-resume"].contains(launchClass),
            mode != "cache-primer" || launchClass == "terminated-warm-system-cache" else
      {
         throw BenchmarkCampaignFailure.invalidPlan
      }
      let dataRoot: URL?
      var dataContainerWasAbsent: Bool?
      if launchClass == "fresh-install-first-launch"
      {
         let root = URL(fileURLWithPath: try requiredLaunchValue(environment, "OXIDE_COMPARISON_DATA_ROOT"), isDirectory: true)
         dataContainerWasAbsent = !FileManager.default.fileExists(atPath: root.path)
         guard dataContainerWasAbsent == true else {throw BenchmarkCampaignFailure.invalidPlan}
         try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
         guard try FileManager.default.contentsOfDirectory(atPath: root.path).isEmpty else
         {
            throw BenchmarkCampaignFailure.invalidPlan
         }
         dataRoot = root
      }
      else
      {
         dataRoot = nil
      }
      let application = XCUIApplication(url: applicationURL)
      var launchArguments = [
         "-oxide-compare-plan-sha", planSHA256,
         "-oxide-compare-run-id", runID,
         "-oxide-compare-chunk", chunkID,
         "-oxide-compare-pass", "canonical-launch",
         "-oxide-compare-pack", packID,
         "-oxide-compare-pair", pairText,
         "-oxide-compare-side", sideText,
         "-oxide-compare-generation", generation,
         "-oxide-compare-output-root", outputRoot.path,
         "-oxide-compare-launch-class", launchClass,
         "-oxide-compare-controlled-start",
      ]
      if let dataRoot
      {
         launchArguments.insert(contentsOf: ["-oxide-compare-data-root", dataRoot.path], at: launchArguments.count - 1)
      }
      if let planPath
      {
         launchArguments.insert(contentsOf: ["-oxide-compare-plan-path", planPath], at: launchArguments.count - 1)
      }
      application.launchArguments = launchArguments
      let relativePrefix = "Runs/\(runID)/\(chunkID)/canonical-launch/\(packID)/\(pairIndex)/\(side.rawValue)"
      let readyURL = outputRoot.appendingPathComponent("\(relativePrefix).launch.ready.json")
      let readyName = "com.oxide.compare.launch.ready.g\(generation)"
      let completeName = "com.oxide.compare.launch.complete.g\(generation)"
      let controllerFinishedName = "com.oxide.compare.launch.controller-finished.g\(generation)"
      let warmPreparedName = "com.oxide.compare.launch.warm-prepared.g\(generation)"
      let warmResumeName = "com.oxide.compare.launch.warm-resume.g\(generation)"
      let clockLog = OSLog(subsystem: "com.oxide.comparison", category: "ClockMap")
      var clockAnchors = [MacOSLaunchControllerClockAnchor]()
      var launchRequestTicks = UInt64(0)
      var initialProcessIdentifier = Int32(0)
      var backgroundObservedTicks: UInt64?
      var suspendedObservedTicks: UInt64?
      var resumedProcessIdentifier: Int32?
      var resumedSameProcess: Bool?
      defer
      {
         if initialProcessIdentifier > 0
         {
            _ = Darwin.kill(initialProcessIdentifier, SIGCONT)
         }
         if application.state != .notRunning
         {
            application.terminate()
         }
      }

      if mode == "measure"
      {
         let before = mach_continuous_time()
         os_signpost(.event, log: clockLog, name: "ClockAnchor", "anchor=%{public}u", 1)
         let after = mach_continuous_time()
         clockAnchors.append(MacOSLaunchControllerClockAnchor(id: 1, beforeTicks: before, afterTicks: after))
      }
      let initialReadyName = launchClass == "warm-resume" ? warmPreparedName : readyName
      let initialReady = runWaitingForComparisonDarwinNotification(initialReadyName, timeout: .seconds(30))
      {
         if launchClass != "warm-resume"
         {
            launchRequestTicks = mach_continuous_time()
         }
         application.launch()
      }
      guard initialReady, application.state == .runningForeground else
      {
         XCTFail("Canonical macOS launch did not reach its durable foreground ready barrier")
         throw BenchmarkCampaignFailure.invalidPlan
      }
      guard let observedProcessIdentifier = exactProcessIdentifier(applicationURL: applicationURL) else
      {
         throw BenchmarkCampaignFailure.invalidPlan
      }
      initialProcessIdentifier = observedProcessIdentifier
      if launchClass == "warm-resume"
      {
         let finder = XCUIApplication(bundleIdentifier: "com.apple.finder")
         finder.activate()
         guard waitForApplicationState(application, state: .runningBackground, timeout: 5) else
         {
            XCTFail("Warm-resume target never entered the background")
            throw BenchmarkCampaignFailure.invalidPlan
         }
         backgroundObservedTicks = mach_continuous_time()
         guard Darwin.kill(initialProcessIdentifier, SIGSTOP) == 0,
               waitForProcessStatus(initialProcessIdentifier, status: UInt32(SSTOP), timeout: 5) else
         {
            XCTFail("Warm-resume target suspension could not be proven")
            throw BenchmarkCampaignFailure.invalidPlan
         }
         suspendedObservedTicks = mach_continuous_time()
         let ready = runWaitingForComparisonDarwinNotification(readyName, timeout: .seconds(30))
         {
            launchRequestTicks = mach_continuous_time()
            guard Darwin.kill(initialProcessIdentifier, SIGCONT) == 0 else {return}
            application.activate()
            postComparisonDarwinNotification(warmResumeName)
         }
         resumedProcessIdentifier = exactProcessIdentifier(applicationURL: applicationURL)
         resumedSameProcess = ready
            && application.state == .runningForeground
            && resumedProcessIdentifier == initialProcessIdentifier
         guard resumedSameProcess == true else
         {
            XCTFail("Warm-resume target did not resume the proven suspended process")
            throw BenchmarkCampaignFailure.invalidPlan
         }
      }
      let readyObservedTicks = mach_continuous_time()
      let readiness = try JSONDecoder().decode(MacOSLaunchReadinessReceipt.self, from: Data(contentsOf: readyURL))
      XCTAssertTrue(readiness.durable)
      XCTAssertGreaterThan(readiness.trustedInputOffsetNs, 0)
      XCTAssertGreaterThan(readiness.trustedInputDeadlineNs, readiness.trustedInputOffsetNs)
      XCTAssertEqual(readiness.probe.schemaVersion, 1)
      XCTAssertEqual(readiness.probe.probeID, "startup-primary-control")
      XCTAssertEqual(readiness.probe.dispatchPath, "trusted-os-input-target-action")
      XCTAssertFalse(readiness.probe.targetIdentity.isEmpty)
      XCTAssertFalse(readiness.probe.actionIdentity.isEmpty)
      XCTAssertGreaterThan(readiness.probe.windowNumber, 0)
      XCTAssertTrue(readiness.probe.xPoints.isFinite)
      XCTAssertTrue(readiness.probe.yPoints.isFinite)

      var inputRequestTicks: UInt64?
      var completeObservedTicks: UInt64?
      if mode == "measure"
      {
         guard let firstClockAnchor = clockAnchors.first else
         {
            throw BenchmarkCampaignFailure.invalidPlan
         }
         let startReceipt = MacOSLaunchControllerStartReceipt(
            schemaVersion: 1,
            runID: runID,
            planSHA256: planSHA256,
            chunkID: chunkID,
            passID: "canonical-launch",
            packID: packID,
            pairIndex: pairIndex,
            side: side,
            generation: generation,
            launchClass: launchClass,
            launchSource: "XCUIApplication(url:).launch",
            launchRequestTicks: launchRequestTicks,
            readyObservedTicks: readyObservedTicks,
            firstClockAnchor: firstClockAnchor,
            complete: true
         )
         _ = try DurableArtifactStore(root: outputRoot).durableJSON(startReceipt, relativePath: "\(relativePrefix).launch.controller-start.json")
         let boundedOffset = Int(min(readiness.trustedInputOffsetNs, UInt64(Int.max)))
         _ = DispatchSemaphore(value: 0).wait(timeout: .now() + .nanoseconds(boundedOffset))
         let primaryAction = application.buttons["startup.primary-action"]
         guard primaryAction.waitForExistence(timeout: 2) else
         {
            XCTFail("Canonical startup primary action is not exposed to trusted XCTest input")
            throw BenchmarkCampaignFailure.invalidPlan
         }
         let complete = runWaitingForComparisonDarwinNotification(completeName, timeout: .seconds(10))
         {
            inputRequestTicks = mach_continuous_time()
            primaryAction.click()
         }
         completeObservedTicks = mach_continuous_time()
         guard complete else
         {
            XCTFail("Canonical startup response did not reach its durable completion barrier")
            throw BenchmarkCampaignFailure.invalidPlan
         }
         let before = mach_continuous_time()
         os_signpost(.event, log: clockLog, name: "ClockAnchor", "anchor=%{public}u", 2)
         let after = mach_continuous_time()
         clockAnchors.append(MacOSLaunchControllerClockAnchor(id: 2, beforeTicks: before, afterTicks: after))
         XCTAssertTrue(runWaitingForComparisonDarwinNotification(controllerFinishedName, timeout: .seconds(30)) {}, "Out-of-process launch collectors did not finish")
      }

      application.terminate()
      let appWasTerminated = application.wait(for: .notRunning, timeout: 5)
      XCTAssertTrue(appWasTerminated, "Canonical comparison application remained foreground after controller completion")
      let installIdentityClaimed: Bool?
      if let dataRoot
      {
         installIdentityClaimed = FileManager.default.fileExists(atPath: dataRoot.appendingPathComponent("install-identity.\(generation)").path)
      }
      else
      {
         installIdentityClaimed = nil
      }
      let receipt = MacOSLaunchControllerReceipt(
         schemaVersion: 1,
         runID: runID,
         planSHA256: planSHA256,
         chunkID: chunkID,
         passID: "canonical-launch",
         packID: packID,
         pairIndex: pairIndex,
         side: side,
         generation: generation,
         launchClass: launchClass,
         mode: mode,
         launchSource: "XCUIApplication(url:).launch",
         inputSource: mode == "measure" ? "XCUIElement.click" : nil,
         launchRequestTicks: launchRequestTicks,
         readyObservedTicks: readyObservedTicks,
         inputRequestTicks: inputRequestTicks,
         completeObservedTicks: completeObservedTicks,
         clockAnchors: clockAnchors,
         installedBundlePath: applicationURL.resolvingSymlinksInPath().path,
         dataContainerPath: dataRoot?.path,
         dataContainerWasAbsent: dataContainerWasAbsent,
         installIdentityClaimed: installIdentityClaimed,
         initialProcessIdentifier: initialProcessIdentifier,
         backgroundObservedTicks: backgroundObservedTicks,
         suspendedObservedTicks: suspendedObservedTicks,
         resumedProcessIdentifier: resumedProcessIdentifier,
         resumedSameProcess: resumedSameProcess,
         appWasTerminated: appWasTerminated,
         complete: true
      )
      _ = try DurableArtifactStore(root: outputRoot).durableJSON(receipt, relativePath: "\(relativePrefix).launch.controller.json")
   }

   private func requiredLaunchValue(_ environment: [String: String], _ name: String) throws -> String
   {
      guard let value = environment[name], !value.isEmpty else
      {
         throw BenchmarkCampaignFailure.invalidPlan
      }
      return value
   }

   private func waitForApplicationState(_ application: XCUIApplication, state: XCUIApplication.State, timeout: TimeInterval) -> Bool
   {
      let deadline = Date().addingTimeInterval(timeout)
      while Date() < deadline
      {
         if application.state == state {return true}
         RunLoop.current.run(until: Date().addingTimeInterval(0.025))
      }
      return application.state == state
   }

   private func exactProcessIdentifier(applicationURL: URL) -> pid_t?
   {
      let expected = applicationURL.resolvingSymlinksInPath().standardizedFileURL
      let matches = NSWorkspace.shared.runningApplications.filter
      {
         $0.bundleURL?.resolvingSymlinksInPath().standardizedFileURL == expected
      }
      guard matches.count == 1 else {return nil}
      return matches[0].processIdentifier
   }

   private func waitForProcessStatus(_ pid: pid_t, status: UInt32, timeout: TimeInterval) -> Bool
   {
      let deadline = Date().addingTimeInterval(timeout)
      while Date() < deadline
      {
         var info = proc_bsdinfo()
         let size = Int32(MemoryLayout<proc_bsdinfo>.size)
         if proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, size) == size,
            info.pbi_status == status
         {
            return true
         }
         RunLoop.current.run(until: Date().addingTimeInterval(0.025))
      }
      return false
   }
}
