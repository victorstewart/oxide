import UIKit

@MainActor
public final class FeedV1IdiomaticUIKitView: NSObject, FeedV1UIKitSurface, UICollectionViewDataSource
{
   public let fixture: FeedV1Fixture
   public let collectionView: UICollectionView
   public private(set) var renderingErrorDescription: String?
   public weak var observationSink: FeedV1UIKitObservationSink?

   private let resources: FeedV1UIKitResources
   private let layout: UICollectionViewFlowLayout
   private static let reuseIdentifier = "FeedV1IdiomaticCell"

   public init(fixture: FeedV1Fixture, resources: FeedV1UIKitResources)
   {
      self.fixture = fixture
      self.resources = resources
      layout = UICollectionViewFlowLayout()
      layout.scrollDirection = .vertical
      layout.minimumLineSpacing = 0
      layout.minimumInteritemSpacing = 0
      layout.sectionInset = .zero
      layout.estimatedItemSize = .zero
      collectionView = UICollectionView(frame: .zero, collectionViewLayout: layout)
      super.init()

      collectionView.backgroundColor = resources.backgroundColor
      collectionView.clipsToBounds = true
      collectionView.contentInset = .zero
      collectionView.scrollIndicatorInsets = .zero
      collectionView.contentInsetAdjustmentBehavior = .never
      collectionView.alwaysBounceVertical = true
      collectionView.alwaysBounceHorizontal = false
      collectionView.showsVerticalScrollIndicator = false
      collectionView.showsHorizontalScrollIndicator = false
      collectionView.scrollsToTop = false
      collectionView.decelerationRate = .normal
      collectionView.isPrefetchingEnabled = false
      collectionView.contentScaleFactor = CGFloat(FeedV1Contract.surfaceScale)
      collectionView.semanticContentAttribute = .forceLeftToRight
      collectionView.register(FeedV1IdiomaticCell.self, forCellWithReuseIdentifier: Self.reuseIdentifier)
      collectionView.dataSource = self
      collectionView.delegate = self
   }

   public func mount(at state: FeedV1StartState)
   {
      collectionView.setNeedsLayout()
      collectionView.collectionViewLayout.invalidateLayout()
      collectionView.layoutIfNeeded()
      collectionView.setContentOffset(
         CGPoint(x: 0, y: fixture.contentOffsetPoints(for: state)),
         animated: false
      )
      collectionView.layoutIfNeeded()
   }

   public func collectionView(_ collectionView: UICollectionView, numberOfItemsInSection section: Int) -> Int
   {
      fixture.rowCount
   }

   public func collectionView(_ collectionView: UICollectionView, cellForItemAt indexPath: IndexPath) -> UICollectionViewCell
   {
      let dequeued = collectionView.dequeueReusableCell(withReuseIdentifier: Self.reuseIdentifier, for: indexPath)
      guard let cell = dequeued as? FeedV1IdiomaticCell else
      {
         renderingErrorDescription = "feed-v1 idiomatic reuse returned an unexpected cell class"
         return dequeued
      }
      guard let row = fixture.row(at: indexPath.item) else
      {
         renderingErrorDescription = "feed-v1 idiomatic data source requested an out-of-range row"
         return cell
      }

      do
      {
         try cell.apply(row: row, resources: resources)
      }
      catch
      {
         renderingErrorDescription = String(describing: error)
      }
      return cell
   }

   public func collectionView(_ collectionView: UICollectionView, layout collectionViewLayout: UICollectionViewLayout, sizeForItemAt indexPath: IndexPath) -> CGSize
   {
      guard let rowHeight = fixture.rowHeightPoints(at: indexPath.item) else
      {
         renderingErrorDescription = "feed-v1 idiomatic layout requested an out-of-range row"
         return .zero
      }
      return CGSize(
         width: FeedV1Contract.surfaceWidthPoints,
         height: rowHeight
      )
   }

   public func collectionView(_ collectionView: UICollectionView, shouldSelectItemAt indexPath: IndexPath) -> Bool
   {
      false
   }

   public func scrollViewDidScroll(_ scrollView: UIScrollView)
   {
      observationSink?.feedV1DidObserveScroll(scrollState)
   }

   public func scrollViewWillBeginDragging(_ scrollView: UIScrollView)
   {
      observationSink?.feedV1DidBeginDragging(scrollState)
   }

   public func scrollViewDidEndDragging(_ scrollView: UIScrollView, willDecelerate decelerate: Bool)
   {
      observationSink?.feedV1DidEndDragging(scrollState, willDecelerate: decelerate)
   }

   public func scrollViewWillBeginDecelerating(_ scrollView: UIScrollView)
   {
      observationSink?.feedV1DidBeginDecelerating(scrollState)
   }

   public func scrollViewDidEndDecelerating(_ scrollView: UIScrollView)
   {
      observationSink?.feedV1DidEndDecelerating(scrollState)
   }
}

extension FeedV1IdiomaticUIKitView: UICollectionViewDelegateFlowLayout
{
}

@MainActor
private final class FeedV1IdiomaticCell: UICollectionViewCell
{
   private var cellContent: FeedV1CellContent?

   override init(frame: CGRect)
   {
      super.init(frame: frame)
      backgroundColor = .clear
   }

   required init?(coder: NSCoder)
   {
      super.init(coder: coder)
      backgroundColor = .clear
   }

   override func layoutSubviews()
   {
      super.layoutSubviews()
      cellContent?.layout(frames: FeedV1CellFrames.make(rowHeightPoints: Int(bounds.height.rounded())))
   }

   override func prepareForReuse()
   {
      super.prepareForReuse()
      cellContent?.clear()
   }

   func apply(row: FeedV1Row, resources: FeedV1UIKitResources) throws
   {
      if cellContent == nil
      {
         cellContent = FeedV1CellContent(
            contentView: contentView,
            resources: resources,
            cacheCaptionComposition: false
         )
      }
      try cellContent?.apply(row: row)
      setNeedsLayout()
   }
}
