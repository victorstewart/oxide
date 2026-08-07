import CoreFoundation
import Foundation
import QuartzCore
import UIKit

private let feedV1CallbackSampleCapacity = 1_024

private struct FeedV1RunMetadata: Codable
{
   let nonce: String
   let phase: String
   let sessionIndex: Int
   let pairIndex: Int
   let orderIndex: Int
   let treatment: String
   let startState: String
   let direction: String
}

private struct FeedV1RunFixture: Codable
{
   let schema: String
   let revision: Int
   let canonicalSha256: String
   let canonicalByteCount: Int
}

private struct FeedV1RunCanvas: Codable
{
   let hostWidthPoints: Int
   let hostHeightPoints: Int
   let surfaceOriginXPoints: Int
   let surfaceOriginYPoints: Int
   let surfaceWidthPoints: Int
   let surfaceHeightPoints: Int
   let scale: Int
}

private struct FeedV1RunRect: Codable, Equatable
{
   let x: Int
   let y: Int
   let width: Int
   let height: Int
}

private struct FeedV1RunComponent: Codable
{
   let id: String
   let kind: String
   let rowIndex: Int
   let contentRectPx: FeedV1RunRect
   let viewportClipPx: FeedV1RunRect
}

private struct FeedV1RunGeometry: Codable
{
   let rowCount: Int
   let manifestComponentCount: Int
   let contentExtentPoints: Double
   let maximumContentOffsetPoints: Double
   let capturedContentOffsetPoints: Double
   let viewportClipPx: FeedV1RunRect
   let visibleComponents: [FeedV1RunComponent]
}

private struct FeedV1RunFrameRate: Codable
{
   let minimum: Double
   let maximum: Double
   let preferred: Double
}

private struct FeedV1RunEnvironmentState: Codable
{
   let thermalState: String
   let lowPowerMode: Bool
   let maximumFramesPerSecond: Int
   let configuredFrameRate: FeedV1RunFrameRate
}

private struct FeedV1RunEnvironment: Codable
{
   let before: FeedV1RunEnvironmentState
   let after: FeedV1RunEnvironmentState
   let thermalStateChangeCount: Int
   let lowPowerModeChangeCount: Int
}

private struct FeedV1EnvironmentTransitionSnapshot
{
   let thermalStateChangeCount: Int
   let lowPowerModeChangeCount: Int
}

private final class FeedV1EnvironmentTransitionCounter: @unchecked Sendable
{
   private let lock = NSLock()
   private var thermalStateChangeCount = 0
   private var lowPowerModeChangeCount = 0
   private var finished = false

   func recordThermalStateChange()
   {
      lock.lock()
      if !finished
      {
         thermalStateChangeCount += 1
      }
      lock.unlock()
   }

   func recordLowPowerModeChange()
   {
      lock.lock()
      if !finished
      {
         lowPowerModeChangeCount += 1
      }
      lock.unlock()
   }

   func snapshot() -> FeedV1EnvironmentTransitionSnapshot
   {
      lock.lock()
      let snapshot = FeedV1EnvironmentTransitionSnapshot(
         thermalStateChangeCount: thermalStateChangeCount,
         lowPowerModeChangeCount: lowPowerModeChangeCount
      )
      lock.unlock()
      return snapshot
   }

   func finish(since baseline: FeedV1EnvironmentTransitionSnapshot) -> FeedV1EnvironmentTransitionSnapshot
   {
      lock.lock()
      finished = true
      let snapshot = FeedV1EnvironmentTransitionSnapshot(
         thermalStateChangeCount: thermalStateChangeCount - baseline.thermalStateChangeCount,
         lowPowerModeChangeCount: lowPowerModeChangeCount - baseline.lowPowerModeChangeCount
      )
      lock.unlock()
      return snapshot
   }
}

private struct FeedV1RunGesture: Codable
{
   let startOffsetPoints: Double
   let endOffsetPoints: Double
   let signedTravelPoints: Double
   let travelDistancePoints: Double
   let durationSeconds: Double
   let inertiaObserved: Bool
   let settled: Bool
}

private struct FeedV1RunDisplaySample: Codable
{
   let timestampSeconds: Double
   let targetTimestampSeconds: Double
}

private struct FeedV1RunDisplayLink: Codable
{
   let clock: String
   let callbackOnly: Bool
   let samples: [FeedV1RunDisplaySample]
}

private struct FeedV1RunRecord: Codable
{
   let schema: String
   let schemaRevision: Int
   let fixture: FeedV1RunFixture
   let run: FeedV1RunMetadata
   let canvas: FeedV1RunCanvas
   let geometry: FeedV1RunGeometry
   let environment: FeedV1RunEnvironment
   let gesture: FeedV1RunGesture
   let displayLink: FeedV1RunDisplayLink
   let status: String
   let failure: String?
}

private extension JSONEncoder.KeyEncodingStrategy
{
   static var feedV1SnakeCase: JSONEncoder.KeyEncodingStrategy
   {
      .convertToSnakeCase
   }
}

@MainActor
private final class FeedV1UIKitRunRecorder: NSObject, FeedV1UIKitObservationSink
{
   private let metadata: FeedV1RunMetadata
   private let startState: FeedV1StartState
   private unowned let root: FeedV1RootViewController
   private let screen: UIScreen
   private let environmentTransitionCounter = FeedV1EnvironmentTransitionCounter()
   private var environmentTransitionObservers = [NSObjectProtocol]()
   private var displaySamples = [FeedV1RunDisplaySample]()
   private var geometry: FeedV1RunGeometry?
   private var environmentBefore: FeedV1RunEnvironmentState?
   private var environmentTransitionBaseline: FeedV1EnvironmentTransitionSnapshot?
   private var gestureStartOffset = 0.0
   private var gestureStartTimestamp = 0.0
   private var lastScrollState: FeedV1UIKitScrollState
   private var readyFrames = 0
   private var readyPosted = false
   private var recording = false
   private var inertiaObserved = false
   private var settlementPending = false
   private var settledTimestamp: CFTimeInterval?
   private var terminal = false
   private var deadline: DispatchWorkItem?

   init(metadata: FeedV1RunMetadata, startState: FeedV1StartState, root: FeedV1RootViewController, screen: UIScreen)
   {
      self.metadata = metadata
      self.startState = startState
      self.root = root
      self.screen = screen
      lastScrollState = root.surface.scrollState
      super.init()
      displaySamples.reserveCapacity(feedV1CallbackSampleCapacity)
      environmentTransitionObservers = [
         NotificationCenter.default.addObserver(
            forName: ProcessInfo.thermalStateDidChangeNotification,
            object: nil,
            queue: nil
         ) { [environmentTransitionCounter] _ in
            environmentTransitionCounter.recordThermalStateChange()
         },
         NotificationCenter.default.addObserver(
            forName: Notification.Name.NSProcessInfoPowerStateDidChange,
            object: nil,
            queue: nil
         ) { [environmentTransitionCounter] _ in
            environmentTransitionCounter.recordLowPowerModeChange()
         }
      ]
   }

   deinit
   {
      for observer in environmentTransitionObservers
      {
         NotificationCenter.default.removeObserver(observer)
      }
   }

   func feedV1DidReceiveDisplayLink(timestamp: CFTimeInterval, targetTimestamp: CFTimeInterval)
   {
      guard !terminal else
      {
         return
      }
      if recording
      {
         guard displaySamples.count < feedV1CallbackSampleCapacity else
         {
            fail("display-link sample capacity exceeded")
            return
         }
         displaySamples.append(FeedV1RunDisplaySample(
            timestampSeconds: timestamp,
            targetTimestampSeconds: targetTimestamp
         ))
         if settlementPending
         {
            finish()
         }
         return
      }
      guard !readyPosted else
      {
         return
      }

      readyFrames += 1
      guard readyFrames >= 2 else
      {
         return
      }
      do
      {
         try admitInitialState()
         readyPosted = true
         postDarwin(FeedV1Contract.readyNotificationPrefix + metadata.nonce)
      }
      catch
      {
         fail(String(describing: error))
      }
   }

   func feedV1DidObserveScroll(_ state: FeedV1UIKitScrollState)
   {
      lastScrollState = state
   }

   func feedV1DidBeginDragging(_ state: FeedV1UIKitScrollState)
   {
      guard readyPosted, !recording, !terminal else
      {
         fail("drag began outside the admitted ready state")
         return
      }
      lastScrollState = state
      gestureStartOffset = state.contentOffsetPoints
      gestureStartTimestamp = CACurrentMediaTime()
      inertiaObserved = false
      settledTimestamp = nil
      guard environmentBefore != nil, environmentTransitionBaseline != nil else
      {
         fail("ready admission did not retain the environment baseline")
         return
      }
      recording = true

      let work = DispatchWorkItem { [weak self] in
         self?.fail("app-owned settle deadline exceeded 6 seconds")
      }
      deadline = work
      DispatchQueue.main.asyncAfter(deadline: .now() + 6.0, execute: work)
   }

   func feedV1DidEndDragging(_ state: FeedV1UIKitScrollState, willDecelerate: Bool)
   {
      lastScrollState = state
      if !willDecelerate
      {
         settledTimestamp = CACurrentMediaTime()
         settlementPending = true
      }
   }

   func feedV1DidBeginDecelerating(_ state: FeedV1UIKitScrollState)
   {
      guard recording, !terminal else
      {
         fail("deceleration began outside the measured gesture")
         return
      }
      lastScrollState = state
      inertiaObserved = true
   }

   func feedV1DidEndDecelerating(_ state: FeedV1UIKitScrollState)
   {
      lastScrollState = state
      settledTimestamp = CACurrentMediaTime()
      settlementPending = true
   }

   private func admitInitialState() throws
   {
      if let error = root.hostContractErrorDescription ?? root.surface.renderingErrorDescription
      {
         throw FeedV1ContractError.invariant(error)
      }
      let state = root.surface.scrollState
      guard state.isSettled else
      {
         throw FeedV1ContractError.invariant("initial collection state is not settled")
      }
      let fixture = root.surface.fixture
      let expectedOffset = Double(fixture.contentOffsetPoints(for: startState))
      guard abs(state.contentOffsetPoints - expectedOffset) <= (1.0 / Double(FeedV1Contract.surfaceScale)) else
      {
         throw FeedV1ContractError.invariant("initial content offset is not the frozen start")
      }
      guard abs(state.contentExtentPoints - Double(fixture.contentExtentPoints)) <= (1.0 / Double(FeedV1Contract.surfaceScale)) else
      {
         throw FeedV1ContractError.invariant("content extent is not the frozen extent")
      }
      geometry = try captureGeometry(state: state)
      guard let admission = feedV1CaptureEnvironmentAdmission(
         snapshot: environmentTransitionCounter.snapshot,
         environment: environmentState
      ) else
      {
         throw FeedV1ContractError.invariant("live display-link frame-rate range is unavailable")
      }
      environmentTransitionBaseline = admission.snapshot
      environmentBefore = admission.environment
      lastScrollState = state
   }

   private func captureGeometry(state: FeedV1UIKitScrollState) throws -> FeedV1RunGeometry
   {
      let fixture = root.surface.fixture
      let collection = root.surface.collectionView
      collection.layoutIfNeeded()
      let paths = collection.indexPathsForVisibleItems.sorted { $0.item < $1.item }
      var components = [FeedV1RunComponent]()
      components.reserveCapacity(paths.count * FeedV1ComponentKind.allCases.count)
      let viewport = FeedV1RunRect(
         x: 0,
         y: 0,
         width: FeedV1Contract.surfaceWidthPoints * FeedV1Contract.surfaceScale,
         height: FeedV1Contract.surfaceHeightPoints * FeedV1Contract.surfaceScale
      )

      for path in paths
      {
         guard fixture.rows.indices.contains(path.item), let cell = collection.cellForItem(at: path) else
         {
            throw FeedV1ContractError.invariant("visible UIKit row has no realized cell")
         }
         let subviews = cell.contentView.subviews
         guard subviews.count == 5 else
         {
            throw FeedV1ContractError.invariant("visible UIKit cell composition changed")
         }
         let observed: [(FeedV1ComponentKind, CGRect)] = [
            (.row, cell.frame),
            (.image, subviews[0].frame.offsetBy(dx: cell.frame.minX, dy: cell.frame.minY)),
            (.title, subviews[1].frame.offsetBy(dx: cell.frame.minX, dy: cell.frame.minY)),
            (.caption, subviews[2].frame.offsetBy(dx: cell.frame.minX, dy: cell.frame.minY)),
            (.metadata, subviews[3].frame.offsetBy(dx: cell.frame.minX, dy: cell.frame.minY)),
            (.separator, subviews[4].frame.offsetBy(dx: cell.frame.minX, dy: cell.frame.minY))
         ]
         for (kind, rect) in observed
         {
            let contentRect = physicalRect(rect)
            let expected = try fixture.componentRectPhysicalPixels(rowIndex: path.item, kind: kind)
            guard rectMatches(contentRect, expected, tolerance: 1) else
            {
               throw FeedV1ContractError.invariant("observed component geometry differs from the frozen manifest")
            }
            guard let clip = viewportClip(contentRect: contentRect, contentOffsetPoints: state.contentOffsetPoints) else
            {
               continue
            }
            components.append(FeedV1RunComponent(
               id: fixture.rows[path.item].componentID(kind),
               kind: kind.rawValue,
               rowIndex: path.item,
               contentRectPx: contentRect,
               viewportClipPx: clip
            ))
         }
      }

      return FeedV1RunGeometry(
         rowCount: fixture.rows.count,
         manifestComponentCount: fixture.rows.count * FeedV1ComponentKind.allCases.count,
         contentExtentPoints: state.contentExtentPoints,
         maximumContentOffsetPoints: state.maximumContentOffsetPoints,
         capturedContentOffsetPoints: state.contentOffsetPoints,
         viewportClipPx: viewport,
         visibleComponents: components
      )
   }

   private func finish()
   {
      guard recording,
            !terminal,
            let geometry,
            let environmentBefore,
            let environmentTransitionBaseline,
            let settledTimestamp else
      {
         fail("completion arrived without a complete admitted run")
         return
      }
      guard lastScrollState.isSettled else
      {
         fail("collection reported completion before settlement")
         return
      }
      guard displaySamples.count >= 2 else
      {
         fail("fewer than two display-link samples were recorded")
         return
      }
      guard gestureStartTimestamp.isFinite,
            settledTimestamp.isFinite,
            settledTimestamp > gestureStartTimestamp else
      {
         fail("settlement timestamp is not after gesture start")
         return
      }
      let durationSeconds = settledTimestamp - gestureStartTimestamp

      terminal = true
      deadline?.cancel()
      let signedTravel = lastScrollState.contentOffsetPoints - gestureStartOffset
      guard let environmentAfter = environmentState() else
      {
         persistFailure("live display-link frame-rate range disappeared before completion")
         postDarwin(FeedV1Contract.failureNotificationPrefix + metadata.nonce)
         return
      }
      let environmentTransitions = environmentTransitionCounter.finish(since: environmentTransitionBaseline)
      let record = FeedV1RunRecord(
         schema: FeedV1Contract.runRecordSchema,
         schemaRevision: FeedV1Contract.runRecordSchemaRevision,
         fixture: FeedV1RunFixture(
            schema: FeedV1Contract.schema,
            revision: FeedV1Contract.revision,
            canonicalSha256: FeedV1Contract.expectedCanonicalSHA256,
            canonicalByteCount: FeedV1Contract.expectedCanonicalByteCount
         ),
         run: metadata,
         canvas: FeedV1RunCanvas(
            hostWidthPoints: FeedV1Contract.hostWidthPoints,
            hostHeightPoints: FeedV1Contract.hostHeightPoints,
            surfaceOriginXPoints: FeedV1Contract.surfaceOriginXPoints,
            surfaceOriginYPoints: FeedV1Contract.surfaceOriginYPoints,
            surfaceWidthPoints: FeedV1Contract.surfaceWidthPoints,
            surfaceHeightPoints: FeedV1Contract.surfaceHeightPoints,
            scale: FeedV1Contract.surfaceScale
         ),
         geometry: geometry,
         environment: FeedV1RunEnvironment(
            before: environmentBefore,
            after: environmentAfter,
            thermalStateChangeCount: environmentTransitions.thermalStateChangeCount,
            lowPowerModeChangeCount: environmentTransitions.lowPowerModeChangeCount
         ),
         gesture: FeedV1RunGesture(
            startOffsetPoints: gestureStartOffset,
            endOffsetPoints: lastScrollState.contentOffsetPoints,
            signedTravelPoints: signedTravel,
            travelDistancePoints: abs(signedTravel),
            durationSeconds: durationSeconds,
            inertiaObserved: inertiaObserved,
            settled: true
         ),
         displayLink: FeedV1RunDisplayLink(
            clock: "CADisplayLink.timestamp/targetTimestamp",
            callbackOnly: true,
            samples: displaySamples
         ),
         status: "complete",
         failure: nil
      )
      do
      {
         try persist(record)
         postDarwin(FeedV1Contract.completionNotificationPrefix + metadata.nonce)
      }
      catch
      {
         persistFailure("success record persistence failed: \(error)", stage: "persistence")
         postDarwin(FeedV1Contract.failureNotificationPrefix + metadata.nonce)
      }
   }

   private func fail(_ message: String)
   {
      guard !terminal else
      {
         return
      }
      terminal = true
      deadline?.cancel()
      persistFailure(message)
      postDarwin(FeedV1Contract.failureNotificationPrefix + metadata.nonce)
   }

   private func persist(_ record: FeedV1RunRecord) throws
   {
      let encoder = JSONEncoder()
      encoder.keyEncodingStrategy = .feedV1SnakeCase
      encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
      let data = try encoder.encode(record)
      try data.write(to: resultURL(), options: .atomic)
   }

   private func persistFailure(_ message: String, stage: String? = nil)
   {
      let body: [String: Any] = [
         "schema": "oxide.feed-v1.failure",
         "schema_revision": 1,
         "fixture": [
            "schema": FeedV1Contract.schema,
            "revision": FeedV1Contract.revision,
            "canonical_sha256": FeedV1Contract.expectedCanonicalSHA256,
            "canonical_byte_count": FeedV1Contract.expectedCanonicalByteCount
         ],
         "nonce": metadata.nonce,
         "treatment": metadata.treatment,
         "stage": stage ?? (readyPosted ? "gesture" : "initial-admission"),
         "message": message
      ]
      guard JSONSerialization.isValidJSONObject(body), let data = try? JSONSerialization.data(withJSONObject: body, options: [.prettyPrinted, .sortedKeys]) else
      {
         return
      }
      try? data.write(to: resultURL(), options: .atomic)
   }

   private func resultURL() -> URL
   {
      let documents = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
      return documents.appendingPathComponent(
         FeedV1Contract.resultFilePrefix + metadata.nonce + FeedV1Contract.resultFileSuffix,
         isDirectory: false
      )
   }

   private func environmentState() -> FeedV1RunEnvironmentState?
   {
      guard let range = root.configuredFrameRateRange,
            let preferred = range.preferred else
      {
         return nil
      }
      return FeedV1RunEnvironmentState(
         thermalState: thermalStateName(ProcessInfo.processInfo.thermalState),
         lowPowerMode: ProcessInfo.processInfo.isLowPowerModeEnabled,
         maximumFramesPerSecond: screen.maximumFramesPerSecond,
         configuredFrameRate: FeedV1RunFrameRate(
            minimum: Double(range.minimum),
            maximum: Double(range.maximum),
            preferred: Double(preferred)
         )
      )
   }
}

@main
@MainActor
final class FeedV1UIKitAppDelegate: UIResponder, UIApplicationDelegate
{
   var window: UIWindow?
   private var recorder: FeedV1UIKitRunRecorder?

   func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool
   {
      let environment = ProcessInfo.processInfo.environment
      do
      {
         let metadata = try parseMetadata(environment)
         let startState = try parseStartState(metadata.startState)
         guard let variant = FeedV1UIKitVariant(rawValue: metadata.treatment) else
         {
            throw FeedV1ContractError.invariant("UIKit app received a non-UIKit treatment")
         }
         let root = try FeedV1RootFactory.make(variant: variant)
         let window = UIWindow(frame: CGRect(
            x: 0,
            y: 0,
            width: FeedV1Contract.hostWidthPoints,
            height: FeedV1Contract.hostHeightPoints
         ))
         window.rootViewController = root
         window.makeKeyAndVisible()
         root.view.layoutIfNeeded()
         root.mount(at: startState)
         let screen = window.windowScene?.screen ?? window.screen
         let recorder = FeedV1UIKitRunRecorder(
            metadata: metadata,
            startState: startState,
            root: root,
            screen: screen
         )
         root.observationSink = recorder
         self.window = window
         self.recorder = recorder
         return true
      }
      catch
      {
         persistLaunchFailure(environment: environment, message: String(describing: error))
         return false
      }
   }
}

private func parseMetadata(_ environment: [String: String]) throws -> FeedV1RunMetadata
{
   func required(_ key: String) throws -> String
   {
      guard let value = environment[key], !value.isEmpty else
      {
         throw FeedV1ContractError.invariant("missing launch environment \(key)")
      }
      return value
   }

   let nonce = try required(FeedV1Contract.completionNonceEnvironmentKey)
   guard FeedV1Contract.completionNonceIsValid(nonce) else
   {
      throw FeedV1ContractError.invariant("completion nonce is not 1...128 ASCII alphanumeric-or-hyphen bytes")
   }
   let phase = try required("OXIDE_FEED_V1_PHASE")
   guard phase == "smoke" || phase == "primary" else
   {
      throw FeedV1ContractError.invariant("unknown sampling phase")
   }
   func index(_ key: String) throws -> Int
   {
      guard let value = Int(try required(key)), value >= 0 else
      {
         throw FeedV1ContractError.invariant("invalid non-negative index \(key)")
      }
      return value
   }
   let treatment = try required(FeedV1Contract.treatmentEnvironmentKey)
   let startState = try required(FeedV1Contract.startStateEnvironmentKey)
   let parsedState = try parseStartState(startState)
   return FeedV1RunMetadata(
      nonce: nonce,
      phase: phase,
      sessionIndex: try index("OXIDE_FEED_V1_SESSION_INDEX"),
      pairIndex: try index("OXIDE_FEED_V1_PAIR_INDEX"),
      orderIndex: try index("OXIDE_FEED_V1_ORDER_INDEX"),
      treatment: treatment,
      startState: startState,
      direction: parsedState.outboundDirection.rawValue
   )
}

private func parseStartState(_ value: String) throws -> FeedV1StartState
{
   guard let state = FeedV1StartState(rawValue: value) else
   {
      throw FeedV1ContractError.invariant("unknown start state")
   }
   return state
}

private func physicalRect(_ rect: CGRect) -> FeedV1RunRect
{
   let scale = CGFloat(FeedV1Contract.surfaceScale)
   let minX = Int((rect.minX * scale).rounded())
   let minY = Int((rect.minY * scale).rounded())
   let maxX = Int((rect.maxX * scale).rounded())
   let maxY = Int((rect.maxY * scale).rounded())
   return FeedV1RunRect(x: minX, y: minY, width: maxX - minX, height: maxY - minY)
}

private func viewportClip(contentRect: FeedV1RunRect, contentOffsetPoints: Double) -> FeedV1RunRect?
{
   let offset = Int((contentOffsetPoints * Double(FeedV1Contract.surfaceScale)).rounded())
   let minX = max(contentRect.x, 0)
   let minY = max(contentRect.y, offset)
   let maxX = min(contentRect.x + contentRect.width, FeedV1Contract.surfaceWidthPoints * FeedV1Contract.surfaceScale)
   let maxY = min(contentRect.y + contentRect.height, offset + FeedV1Contract.surfaceHeightPoints * FeedV1Contract.surfaceScale)
   guard minX < maxX, minY < maxY else
   {
      return nil
   }
   return FeedV1RunRect(x: minX, y: minY - offset, width: maxX - minX, height: maxY - minY)
}

private func rectMatches(_ observed: FeedV1RunRect, _ expected: FeedV1PhysicalRect, tolerance: Int) -> Bool
{
   abs(observed.x - expected.x) <= tolerance
      && abs(observed.y - expected.y) <= tolerance
      && abs((observed.x + observed.width) - (expected.x + expected.width)) <= tolerance
      && abs((observed.y + observed.height) - (expected.y + expected.height)) <= tolerance
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

private func postDarwin(_ name: String)
{
   CFNotificationCenterPostNotification(
      CFNotificationCenterGetDarwinNotifyCenter(),
      CFNotificationName(rawValue: name as CFString),
      nil,
      nil,
      true
   )
}

private func persistLaunchFailure(environment: [String: String], message: String)
{
   guard let nonce = environment[FeedV1Contract.completionNonceEnvironmentKey],
         FeedV1Contract.completionNonceIsValid(nonce) else
   {
      return
   }
   let body: [String: Any] = [
      "schema": "oxide.feed-v1.failure",
      "schema_revision": 1,
      "fixture": [
         "schema": FeedV1Contract.schema,
         "revision": FeedV1Contract.revision,
         "canonical_sha256": FeedV1Contract.expectedCanonicalSHA256,
         "canonical_byte_count": FeedV1Contract.expectedCanonicalByteCount
      ],
      "nonce": nonce,
      "treatment": environment[FeedV1Contract.treatmentEnvironmentKey] ?? NSNull(),
      "stage": "launch",
      "message": message
   ]
   if JSONSerialization.isValidJSONObject(body), let data = try? JSONSerialization.data(withJSONObject: body, options: [.prettyPrinted, .sortedKeys])
   {
      let documents = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
      let url = documents.appendingPathComponent(FeedV1Contract.resultFilePrefix + nonce + FeedV1Contract.resultFileSuffix)
      try? data.write(to: url, options: .atomic)
   }
   postDarwin(FeedV1Contract.failureNotificationPrefix + nonce)
}
