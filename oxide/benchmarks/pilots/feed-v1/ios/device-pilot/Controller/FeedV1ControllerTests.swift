import CoreFoundation
import CoreImage
import Foundation
import UIKit
import XCTest

private let feedV1UIKitBundleID = "com.oxide.feed-v1.uikit"
private let feedV1OxideBundleID = "com.oxide.feed-v1.oxide"

private enum FeedV1Treatment: String, CaseIterable, Codable
{
   case idiomatic = "uikit-idiomatic"
   case optimized = "uikit-optimized"
   case oxide

   var bundleID: String
   {
      switch self
      {
         case .idiomatic, .optimized:
            return feedV1UIKitBundleID
         case .oxide:
            return feedV1OxideBundleID
      }
   }
}

private enum FeedV1Direction: String, CaseIterable, Codable
{
   case forward
   case reverse

   var startState: String
   {
      switch self
      {
         case .forward:
            return "top"
         case .reverse:
            return "bottom"
      }
   }
}

private struct FeedV1ControllerRuntimeRecord: Codable
{
   let schema: String
   let schemaRevision: Int
   let mode: String
   let totalRuntimeSeconds: Double
   let uikitIdiomaticRuntimeSeconds: Double
   let uikitOptimizedRuntimeSeconds: Double
   let oxideRuntimeSeconds: Double
   let sessionEnvironments: [FeedV1ControllerSessionEnvironment]
}

private struct FeedV1ControllerFrameRateRange: Codable
{
   let minimum: Double
   let maximum: Double
   let preferred: Double
}

private struct FeedV1ControllerEnvironmentState: Codable
{
   let thermalState: String
   let lowPowerMode: Bool
   let maximumFramesPerSecond: Int
   let configuredFrameRate: FeedV1ControllerFrameRateRange
}

private struct FeedV1ControllerSessionEnvironment: Codable
{
   let sessionIndex: Int
   let before: FeedV1ControllerEnvironmentState
   let after: FeedV1ControllerEnvironmentState
   let thermalStateChangeCount: Int
   let lowPowerModeChangeCount: Int
}

private struct FeedV1ControllerEnvironmentTransitionSnapshot
{
   let thermalStateChangeCount: Int
   let lowPowerModeChangeCount: Int
}

private final class FeedV1ControllerEnvironmentTransitionCounter: @unchecked Sendable
{
   private let lock = NSLock()
   private var thermalStateChangeCount = 0
   private var lowPowerModeChangeCount = 0

   func recordThermalStateChange()
   {
      lock.lock()
      thermalStateChangeCount += 1
      lock.unlock()
   }

   func recordLowPowerModeChange()
   {
      lock.lock()
      lowPowerModeChangeCount += 1
      lock.unlock()
   }

   func snapshot() -> FeedV1ControllerEnvironmentTransitionSnapshot
   {
      lock.lock()
      let snapshot = FeedV1ControllerEnvironmentTransitionSnapshot(
         thermalStateChangeCount: thermalStateChangeCount,
         lowPowerModeChangeCount: lowPowerModeChangeCount
      )
      lock.unlock()
      return snapshot
   }

   func changes(since baseline: FeedV1ControllerEnvironmentTransitionSnapshot) -> FeedV1ControllerEnvironmentTransitionSnapshot
   {
      let current = snapshot()
      return FeedV1ControllerEnvironmentTransitionSnapshot(
         thermalStateChangeCount: current.thermalStateChangeCount - baseline.thermalStateChangeCount,
         lowPowerModeChangeCount: current.lowPowerModeChangeCount - baseline.lowPowerModeChangeCount
      )
   }
}

private final class FeedV1ControllerEnvironmentMonitor
{
   private let counter = FeedV1ControllerEnvironmentTransitionCounter()
   private var observers = [NSObjectProtocol]()

   init()
   {
      let counter = counter
      observers = [
         NotificationCenter.default.addObserver(
            forName: ProcessInfo.thermalStateDidChangeNotification,
            object: nil,
            queue: nil
         ) { [counter] _ in
            counter.recordThermalStateChange()
         },
         NotificationCenter.default.addObserver(
            forName: Notification.Name.NSProcessInfoPowerStateDidChange,
            object: nil,
            queue: nil
         ) { [counter] _ in
            counter.recordLowPowerModeChange()
         }
      ]
   }

   deinit
   {
      for observer in observers
      {
         NotificationCenter.default.removeObserver(observer)
      }
   }

   func snapshot() -> FeedV1ControllerEnvironmentTransitionSnapshot
   {
      counter.snapshot()
   }

   func changes(since baseline: FeedV1ControllerEnvironmentTransitionSnapshot) -> FeedV1ControllerEnvironmentTransitionSnapshot
   {
      counter.changes(since: baseline)
   }
}

private final class FeedV1ControllerDisplayLinkTarget: NSObject
{
   @objc func tick(_ displayLink: CADisplayLink)
   {
   }
}

private final class FeedV1DarwinObserver
{
   private let name: String
   private let handler: () -> Void

   init(name: String, handler: @escaping () -> Void)
   {
      self.name = name
      self.handler = handler
      CFNotificationCenterAddObserver(
         CFNotificationCenterGetDarwinNotifyCenter(),
         Unmanaged.passUnretained(self).toOpaque(),
         feedV1DarwinCallback,
         name as CFString,
         nil,
         .deliverImmediately
      )
   }

   deinit
   {
      CFNotificationCenterRemoveObserver(
         CFNotificationCenterGetDarwinNotifyCenter(),
         Unmanaged.passUnretained(self).toOpaque(),
         CFNotificationName(rawValue: name as CFString),
         nil
      )
   }

   func receive()
   {
      handler()
   }
}

private func feedV1DarwinCallback(_ center: CFNotificationCenter?, _ observer: UnsafeMutableRawPointer?, _ name: CFNotificationName?, _ object: UnsafeRawPointer?, _ userInfo: CFDictionary?)
{
   guard let observer else
   {
      return
   }
   Unmanaged<FeedV1DarwinObserver>.fromOpaque(observer).takeUnretainedValue().receive()
}

private func controllerError(_ description: String) -> NSError
{
   NSError(
      domain: "com.oxide.feed-v1.controller",
      code: 1,
      userInfo: [NSLocalizedDescriptionKey: description]
   )
}

final class FeedV1ControllerTests: XCTestCase
{
   private let treatments = FeedV1Treatment.allCases
   private let launchTimeout = 7.0
   private let settleTimeout = 7.0
   private let officialRuntimeLimit = 20.0 * 60.0
   private var treatmentRuntime = [FeedV1Treatment: Double]()
   private var sessionEnvironments = [FeedV1ControllerSessionEnvironment]()

   override func setUp()
   {
      super.setUp()
      continueAfterFailure = false
      executionTimeAllowance = officialRuntimeLimit
      treatmentRuntime.removeAll(keepingCapacity: true)
      sessionEnvironments.removeAll(keepingCapacity: true)
   }

   func testFeedV1PhysicalDevicePilot() throws
   {
      let started = ProcessInfo.processInfo.systemUptime
      let mode = ProcessInfo.processInfo.environment["OXIDE_FEED_V1_CONTROLLER_MODE"] ?? "smoke"
      guard mode == "smoke" || mode == "primary" else
      {
         XCTFail("unknown controller mode \(mode)")
         return
      }

      if mode == "smoke"
      {
         for (orderIndex, treatment) in treatments.enumerated()
         {
            for direction in FeedV1Direction.allCases
            {
               try run(
                  phase: "smoke",
                  sessionIndex: 0,
                  pairIndex: 0,
                  orderIndex: orderIndex,
                  treatment: treatment,
                  direction: direction
               )
            }
         }
         try persistRuntime(mode: mode, started: started)
         return
      }

      sessionEnvironments.reserveCapacity(3)
      let environmentMonitor = FeedV1ControllerEnvironmentMonitor()
      for sessionIndex in 0 ..< 3
      {
         let transitionBaseline = environmentMonitor.snapshot()
         let environmentBefore = try captureEnvironmentState()
         try requireAdmissible(environmentBefore, label: "primary session \(sessionIndex) before")
         for pairIndex in 0 ..< 3
         {
            let rotation = (sessionIndex + pairIndex) % treatments.count
            for orderIndex in treatments.indices
            {
               let treatment = treatments[(rotation + orderIndex) % treatments.count]
               for direction in FeedV1Direction.allCases
               {
                  guard ProcessInfo.processInfo.systemUptime - started < officialRuntimeLimit else
                  {
                     XCTFail("official physical-device runtime exceeded 20 minutes")
                     return
                  }
                  try run(
                     phase: "primary",
                     sessionIndex: sessionIndex,
                     pairIndex: pairIndex,
                     orderIndex: orderIndex,
                     treatment: treatment,
                     direction: direction
                  )
               }
            }
         }
         let environmentAfter = try captureEnvironmentState()
         let transitions = environmentMonitor.changes(since: transitionBaseline)
         let environment = FeedV1ControllerSessionEnvironment(
            sessionIndex: sessionIndex,
            before: environmentBefore,
            after: environmentAfter,
            thermalStateChangeCount: transitions.thermalStateChangeCount,
            lowPowerModeChangeCount: transitions.lowPowerModeChangeCount
         )
         try requireAdmissible(environment)
         sessionEnvironments.append(environment)
      }
      try persistRuntime(mode: mode, started: started)
   }

   private func run(phase: String, sessionIndex: Int, pairIndex: Int, orderIndex: Int, treatment: FeedV1Treatment, direction: FeedV1Direction) throws
   {
      let treatmentStarted = ProcessInfo.processInfo.systemUptime
      defer
      {
         treatmentRuntime[treatment, default: 0] += ProcessInfo.processInfo.systemUptime - treatmentStarted
         if treatmentRuntime[treatment, default: 0] > 10.0 * 60.0
         {
            XCTFail("\(treatment.rawValue) exceeded its 10-minute runtime limit")
         }
      }
      let nonce = makeNonce(
         phase: phase,
         sessionIndex: sessionIndex,
         pairIndex: pairIndex,
         orderIndex: orderIndex,
         treatment: treatment,
         direction: direction
      )
      let app = XCUIApplication(bundleIdentifier: treatment.bundleID)
      app.launchEnvironment = [
         "OXIDE_FEED_V1_TREATMENT": treatment.rawValue,
         "OXIDE_FEED_V1_START_STATE": direction.startState,
         "OXIDE_FEED_V1_COMPLETION_NONCE": nonce,
         "OXIDE_FEED_V1_PHASE": phase,
         "OXIDE_FEED_V1_SESSION_INDEX": String(sessionIndex),
         "OXIDE_FEED_V1_PAIR_INDEX": String(pairIndex),
         "OXIDE_FEED_V1_ORDER_INDEX": String(orderIndex)
      ]

      var readyOutcome: String?
      let readyExpectation = expectation(description: "ready-or-failed-\(nonce)")
      let readyObserver = FeedV1DarwinObserver(name: "com.oxide.feed-v1.ready.\(nonce)")
      {
         readyOutcome = "ready"
         readyExpectation.fulfill()
      }
      let earlyFailureObserver = FeedV1DarwinObserver(name: "com.oxide.feed-v1.failed.\(nonce)")
      {
         readyOutcome = "failed"
         readyExpectation.fulfill()
      }
      withExtendedLifetime([readyObserver, earlyFailureObserver])
      {
         app.launch()
         XCTAssertTrue(app.wait(for: .runningForeground, timeout: launchTimeout), "app did not enter foreground for \(nonce)")
         wait(for: [readyExpectation], timeout: launchTimeout)
      }
      guard readyOutcome == "ready" else
      {
         app.terminate()
         XCTFail("app failed before ready admission for \(nonce)")
         return
      }

      if phase == "smoke"
      {
         let admissionCapture = XCUIScreen.main.screenshot()
         let repeatCapture = XCUIScreen.main.screenshot()
         attach(
            try normalizedCapturePNG(admissionCapture),
            name: "feed-v1-\(nonce)-admission.png",
            uniformTypeIdentifier: "public.png"
         )
         attach(
            try normalizedCapturePNG(repeatCapture),
            name: "feed-v1-\(nonce)-repeat.png",
            uniformTypeIdentifier: "public.png"
         )
      }

      var terminalOutcome: String?
      let terminalExpectation = expectation(description: "complete-or-failed-\(nonce)")
      let completionObserver = FeedV1DarwinObserver(name: "com.oxide.feed-v1.complete.\(nonce)")
      {
         terminalOutcome = "complete"
         terminalExpectation.fulfill()
      }
      let failureObserver = FeedV1DarwinObserver(name: "com.oxide.feed-v1.failed.\(nonce)")
      {
         terminalOutcome = "failed"
         terminalExpectation.fulfill()
      }
      withExtendedLifetime([completionObserver, failureObserver])
      {
         performGesture(app: app, direction: direction)
         wait(for: [terminalExpectation], timeout: settleTimeout)
      }
      app.terminate()
      XCTAssertEqual(terminalOutcome, "complete", "app did not produce a complete run for \(nonce)")
   }

   private func performGesture(app: XCUIApplication, direction: FeedV1Direction)
   {
      let surfaceX = 25.0 + (390.0 * 0.5)
      let forwardStartY = 56.0 + (844.0 * 0.82)
      let forwardEndY = 56.0 + (844.0 * 0.18)
      let startY = direction == .forward ? forwardStartY : forwardEndY
      let endY = direction == .forward ? forwardEndY : forwardStartY
      let origin = app.coordinate(withNormalizedOffset: CGVector(dx: 0, dy: 0))
      let start = origin.withOffset(CGVector(dx: surfaceX, dy: startY))
      let end = origin.withOffset(CGVector(dx: surfaceX, dy: endY))
      start.press(
         forDuration: 0.05,
         thenDragTo: end,
         withVelocity: XCUIGestureVelocity(rawValue: 2_400),
         thenHoldForDuration: 0
      )
   }

   private func attach(_ data: Data, name: String, uniformTypeIdentifier: String)
   {
      let attachment = XCTAttachment(data: data, uniformTypeIdentifier: uniformTypeIdentifier)
      attachment.name = name
      attachment.lifetime = .keepAlways
      add(attachment)
   }

   private func normalizedCapturePNG(_ capture: XCUIScreenshot) throws -> Data
   {
      let expectedSize = CGSize(width: 1_320, height: 2_868)
      guard let colorSpace = CGColorSpace(name: CGColorSpace.sRGB),
            let source = CIImage(image: capture.image, options: [.applyOrientationProperty: true]) else
      {
         throw controllerError("could not construct the sRGB smoke capture")
      }
      let extent = source.extent.integral
      guard extent.width == expectedSize.width && extent.height == expectedSize.height else
      {
         throw controllerError("smoke capture is \(Int(extent.width))x\(Int(extent.height)), expected 1320x2868")
      }
      let normalized = source
         .transformed(by: CGAffineTransform(translationX: -extent.minX, y: -extent.minY))
         .composited(over: CIImage(color: CIColor(
            red: 247.0 / 255.0,
            green: 244.0 / 255.0,
            blue: 238.0 / 255.0,
            alpha: 1
         )).cropped(to: CGRect(origin: .zero, size: expectedSize)))
         .cropped(to: CGRect(origin: .zero, size: expectedSize))
      let context = CIContext(options: [
         .workingColorSpace: colorSpace,
         .outputColorSpace: colorSpace,
         .cacheIntermediates: false
      ])
      guard let data = context.pngRepresentation(
         of: normalized,
         format: .RGBA8,
         colorSpace: colorSpace
      ) else
      {
         throw controllerError("could not encode the opaque sRGB8 smoke capture")
      }
      return data
   }

   private func captureEnvironmentState() throws -> FeedV1ControllerEnvironmentState
   {
      try onMainThread
      {
         let screens = UIApplication.shared.connectedScenes.compactMap
         {
            ($0 as? UIWindowScene)?.screen
         }
         guard let screen = screens.first,
               screens.allSatisfy({ $0 === screen }) else
         {
            throw controllerError("controller runner has no unambiguous contextual screen")
         }
         let target = FeedV1ControllerDisplayLinkTarget()
         guard let displayLink = screen.displayLink(
            withTarget: target,
            selector: #selector(FeedV1ControllerDisplayLinkTarget.tick(_:))
         ) else
         {
            throw controllerError("controller runner could not create a screen-specific display link")
         }
         defer
         {
            displayLink.invalidate()
         }
         displayLink.preferredFrameRateRange = CAFrameRateRange(
            minimum: 120.0,
            maximum: 120.0,
            preferred: 120.0
         )
         let range = displayLink.preferredFrameRateRange
         guard let preferred = range.preferred else
         {
            throw controllerError("controller display-link range has no preferred rate")
         }
         return FeedV1ControllerEnvironmentState(
            thermalState: thermalStateName(ProcessInfo.processInfo.thermalState),
            lowPowerMode: ProcessInfo.processInfo.isLowPowerModeEnabled,
            maximumFramesPerSecond: screen.maximumFramesPerSecond,
            configuredFrameRate: FeedV1ControllerFrameRateRange(
               minimum: Double(range.minimum),
               maximum: Double(range.maximum),
               preferred: Double(preferred)
            )
         )
      }
   }

   private func requireAdmissible(_ state: FeedV1ControllerEnvironmentState, label: String) throws
   {
      let range = state.configuredFrameRate
      guard range.minimum.isFinite,
            range.maximum.isFinite,
            range.preferred.isFinite,
            state.thermalState == "nominal",
            !state.lowPowerMode,
            state.maximumFramesPerSecond == 120,
            range.minimum == 120.0,
            range.maximum == 120.0,
            range.preferred == 120.0 else
      {
         throw controllerError("\(label) is not nominal Low-Power-off exact native 120 Hz")
      }
   }

   private func requireAdmissible(_ environment: FeedV1ControllerSessionEnvironment) throws
   {
      try requireAdmissible(environment.before, label: "primary session \(environment.sessionIndex) before")
      try requireAdmissible(environment.after, label: "primary session \(environment.sessionIndex) after")
      guard environment.thermalStateChangeCount == 0,
            environment.lowPowerModeChangeCount == 0 else
      {
         throw controllerError("primary session \(environment.sessionIndex) observed a thermal or Low Power Mode transition")
      }
   }

   private func onMainThread<T>(_ body: () throws -> T) rethrows -> T
   {
      if Thread.isMainThread
      {
         return try body()
      }
      return try DispatchQueue.main.sync(execute: body)
   }

   private func thermalStateName(_ state: ProcessInfo.ThermalState) -> String
   {
      switch state
      {
         case .nominal:
            return "nominal"
         case .fair:
            return "fair"
         case .serious:
            return "serious"
         case .critical:
            return "critical"
         @unknown default:
            return "unknown"
      }
   }

   private func persistRuntime(mode: String, started: TimeInterval) throws
   {
      let record = FeedV1ControllerRuntimeRecord(
         schema: "oxide.feed-v1.controller-runtime",
         schemaRevision: 2,
         mode: mode,
         totalRuntimeSeconds: ProcessInfo.processInfo.systemUptime - started,
         uikitIdiomaticRuntimeSeconds: treatmentRuntime[.idiomatic, default: 0],
         uikitOptimizedRuntimeSeconds: treatmentRuntime[.optimized, default: 0],
         oxideRuntimeSeconds: treatmentRuntime[.oxide, default: 0],
         sessionEnvironments: sessionEnvironments
      )
      let encoder = JSONEncoder()
      encoder.keyEncodingStrategy = .convertToSnakeCase
      encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
      let data = try encoder.encode(record)
      let documents = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
      try data.write(
         to: documents.appendingPathComponent("oxide-feed-v1-controller-runtime-\(mode).json", isDirectory: false),
         options: .atomic
      )
   }

   private func makeNonce(phase: String, sessionIndex: Int, pairIndex: Int, orderIndex: Int, treatment: FeedV1Treatment, direction: FeedV1Direction) -> String
   {
      let uuid = UUID().uuidString.lowercased()
      return String(format: "%@-s%02d-p%02d-o%d-%@-%@-%@", phase, sessionIndex, pairIndex, orderIndex, treatment.rawValue, direction.rawValue, uuid)
   }

}
