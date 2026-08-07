import QuartzCore
import UIKit

public enum FeedV1UIKitVariant: String, Sendable
{
   case idiomatic = "uikit-idiomatic"
   case optimized = "uikit-optimized"
}

public struct FeedV1UIKitScrollState: Equatable, Sendable
{
   public let contentOffsetPoints: Double
   public let contentExtentPoints: Double
   public let maximumContentOffsetPoints: Double
   public let isTracking: Bool
   public let isDragging: Bool
   public let isDecelerating: Bool

   public var isSettled: Bool
   {
      !isTracking && !isDragging && !isDecelerating
   }

   public init(contentOffsetPoints: Double, contentExtentPoints: Double, maximumContentOffsetPoints: Double, isTracking: Bool, isDragging: Bool, isDecelerating: Bool)
   {
      self.contentOffsetPoints = contentOffsetPoints
      self.contentExtentPoints = contentExtentPoints
      self.maximumContentOffsetPoints = maximumContentOffsetPoints
      self.isTracking = isTracking
      self.isDragging = isDragging
      self.isDecelerating = isDecelerating
   }
}

@MainActor
public protocol FeedV1UIKitObservationSink: AnyObject
{
   func feedV1DidReceiveDisplayLink(timestamp: CFTimeInterval, targetTimestamp: CFTimeInterval)
   func feedV1DidObserveScroll(_ state: FeedV1UIKitScrollState)
   func feedV1DidBeginDragging(_ state: FeedV1UIKitScrollState)
   func feedV1DidEndDragging(_ state: FeedV1UIKitScrollState, willDecelerate: Bool)
   func feedV1DidBeginDecelerating(_ state: FeedV1UIKitScrollState)
   func feedV1DidEndDecelerating(_ state: FeedV1UIKitScrollState)
}

@MainActor
public protocol FeedV1UIKitSurface: AnyObject
{
   var fixture: FeedV1Fixture { get }
   var collectionView: UICollectionView { get }
   var renderingErrorDescription: String? { get }
   var observationSink: FeedV1UIKitObservationSink? { get set }
   var scrollState: FeedV1UIKitScrollState { get }

   func mount(at state: FeedV1StartState)
}

public extension FeedV1UIKitSurface
{
   var scrollState: FeedV1UIKitScrollState
   {
      FeedV1UIKitScrollState(
         contentOffsetPoints: collectionView.contentOffset.y,
         contentExtentPoints: collectionView.contentSize.height,
         maximumContentOffsetPoints: max(collectionView.contentSize.height - collectionView.bounds.height, 0),
         isTracking: collectionView.isTracking,
         isDragging: collectionView.isDragging,
         isDecelerating: collectionView.isDecelerating
      )
   }
}
