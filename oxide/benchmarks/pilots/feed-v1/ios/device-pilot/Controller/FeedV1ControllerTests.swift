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

final class FeedV1ControllerTests: XCTestCase
{
   private let treatments = FeedV1Treatment.allCases
   private let launchTimeout = 7.0
   private let settleTimeout = 7.0
   private let officialRuntimeLimit = 20.0 * 60.0
   private var treatmentRuntime = [FeedV1Treatment: Double]()

   override func setUp()
   {
      super.setUp()
      continueAfterFailure = false
      executionTimeAllowance = officialRuntimeLimit
      treatmentRuntime.removeAll(keepingCapacity: true)
   }

   func testFeedV1PhysicalDevicePilot() throws
   {
      let started = ProcessInfo.processInfo.systemUptime
      let mode = ProcessInfo.processInfo.environment["OXIDE_FEED_V1_CONTROLLER_MODE"] ?? "smoke"
      guard mode == "smoke" || mode == "full" else
      {
         XCTFail("unknown controller mode \(mode)")
         return
      }

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
      if mode == "smoke"
      {
         try persistRuntime(mode: mode, started: started)
         return
      }

      treatmentRuntime.removeAll(keepingCapacity: true)

      for sessionIndex in 0 ..< 3
      {
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

   private func persistRuntime(mode: String, started: TimeInterval) throws
   {
      let record = FeedV1ControllerRuntimeRecord(
         schema: "oxide.feed-v1.controller-runtime",
         schemaRevision: 1,
         mode: mode,
         totalRuntimeSeconds: ProcessInfo.processInfo.systemUptime - started,
         uikitIdiomaticRuntimeSeconds: treatmentRuntime[.idiomatic, default: 0],
         uikitOptimizedRuntimeSeconds: treatmentRuntime[.optimized, default: 0],
         oxideRuntimeSeconds: treatmentRuntime[.oxide, default: 0]
      )
      let encoder = JSONEncoder()
      encoder.keyEncodingStrategy = .convertToSnakeCase
      encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
      let data = try encoder.encode(record)
      let documents = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
      try data.write(
         to: documents.appendingPathComponent("oxide-feed-v1-controller-runtime.json", isDirectory: false),
         options: .atomic
      )
   }

   private func makeNonce(phase: String, sessionIndex: Int, pairIndex: Int, orderIndex: Int, treatment: FeedV1Treatment, direction: FeedV1Direction) -> String
   {
      let uuid = UUID().uuidString.lowercased()
      return String(format: "%@-s%02d-p%02d-o%d-%@-%@-%@", phase, sessionIndex, pairIndex, orderIndex, treatment.rawValue, direction.rawValue, uuid)
   }

}
