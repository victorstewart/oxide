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

@MainActor
public enum FeedV1RootFactory
{
   public static func make(variant: FeedV1UIKitVariant, resourceBundle: Bundle = .main) throws -> FeedV1RootViewController
   {
      let fixture = try FeedV1Contract.materialize()
      let resources = try FeedV1UIKitResources(bundle: resourceBundle)
      let surface: FeedV1UIKitSurface
      switch variant
      {
         case .idiomatic:
            surface = FeedV1IdiomaticUIKitView(fixture: fixture, resources: resources)
         case .optimized:
            surface = FeedV1OptimizedUIKitView(fixture: fixture, resources: resources)
      }
      return FeedV1RootViewController(variant: variant, surface: surface)
   }
}

@MainActor
public final class FeedV1RootViewController: UIViewController
{
   public let variant: FeedV1UIKitVariant
   public let surface: FeedV1UIKitSurface
   public private(set) var hostContractErrorDescription: String?

   public weak var observationSink: FeedV1UIKitObservationSink?
   {
      didSet
      {
         surface.observationSink = observationSink
      }
   }

   public var configuredFrameRateRange: CAFrameRateRange?
   {
      displayLink?.preferredFrameRateRange
   }

   private var displayLink: CADisplayLink?

   public init(variant: FeedV1UIKitVariant, surface: FeedV1UIKitSurface)
   {
      self.variant = variant
      self.surface = surface
      super.init(nibName: nil, bundle: nil)
      overrideUserInterfaceStyle = .light
   }

   public required init?(coder: NSCoder)
   {
      nil
   }

   public override var prefersStatusBarHidden: Bool
   {
      true
   }

   public override var prefersHomeIndicatorAutoHidden: Bool
   {
      true
   }

   public override var supportedInterfaceOrientations: UIInterfaceOrientationMask
   {
      .portrait
   }

   public override var preferredInterfaceOrientationForPresentation: UIInterfaceOrientation
   {
      .portrait
   }

   public override var shouldAutorotate: Bool
   {
      false
   }

   public override func loadView()
   {
      let root = UIView(frame: CGRect(
         x: 0,
         y: 0,
         width: FeedV1Contract.hostWidthPoints,
         height: FeedV1Contract.hostHeightPoints
      ))
      root.backgroundColor = .black
      root.contentScaleFactor = CGFloat(FeedV1Contract.surfaceScale)
      root.semanticContentAttribute = .forceLeftToRight
      surface.collectionView.frame = CGRect(
         x: FeedV1Contract.surfaceOriginXPoints,
         y: FeedV1Contract.surfaceOriginYPoints,
         width: FeedV1Contract.surfaceWidthPoints,
         height: FeedV1Contract.surfaceHeightPoints
      )
      root.addSubview(surface.collectionView)
      view = root
   }

   public override func viewDidLayoutSubviews()
   {
      super.viewDidLayoutSubviews()
      let expected = CGSize(
         width: FeedV1Contract.hostWidthPoints,
         height: FeedV1Contract.hostHeightPoints
      )
      guard view.bounds.size == expected else
      {
         hostContractErrorDescription = "feed-v1 host bounds are \(view.bounds.size); expected \(expected)"
         return
      }
      surface.collectionView.frame = CGRect(
         x: FeedV1Contract.surfaceOriginXPoints,
         y: FeedV1Contract.surfaceOriginYPoints,
         width: FeedV1Contract.surfaceWidthPoints,
         height: FeedV1Contract.surfaceHeightPoints
      )
   }

   public override func viewDidAppear(_ animated: Bool)
   {
      super.viewDidAppear(animated)
      if let observedScale = view.window?.screen.scale, observedScale != CGFloat(FeedV1Contract.surfaceScale)
      {
         hostContractErrorDescription = "feed-v1 host scale is \(observedScale); expected \(FeedV1Contract.surfaceScale)"
      }
      startDisplayLinkIfNeeded()
   }

   public override func viewDidDisappear(_ animated: Bool)
   {
      super.viewDidDisappear(animated)
      displayLink?.invalidate()
      displayLink = nil
   }

   public func mount(at state: FeedV1StartState)
   {
      surface.mount(at: state)
   }

   @objc
   private func onDisplayLink(_ link: CADisplayLink)
   {
      observationSink?.feedV1DidReceiveDisplayLink(
         timestamp: link.timestamp,
         targetTimestamp: link.targetTimestamp
      )
   }

   private func startDisplayLinkIfNeeded()
   {
      guard displayLink == nil else
      {
         return
      }
      let link = CADisplayLink(target: self, selector: #selector(onDisplayLink(_:)))
      link.preferredFrameRateRange = CAFrameRateRange(
         minimum: Float(FeedV1Contract.displayLinkMinimumFramesPerSecond),
         maximum: Float(FeedV1Contract.displayLinkMaximumFramesPerSecond),
         preferred: Float(FeedV1Contract.displayLinkPreferredFramesPerSecond)
      )
      link.add(to: .main, forMode: .common)
      displayLink = link
   }
}
