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
