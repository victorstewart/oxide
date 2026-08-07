import UIKit

@MainActor
public final class FeedV1OptimizedUIKitView: NSObject, FeedV1UIKitSurface, UICollectionViewDataSource
{
   public let fixture: FeedV1Fixture
   public let collectionView: UICollectionView
   public private(set) var renderingErrorDescription: String?
   public weak var observationSink: FeedV1UIKitObservationSink?

   private let resources: FeedV1UIKitResources
   private let cachedLayout: FeedV1CachedCollectionLayout
   private static let reuseIdentifier = "FeedV1OptimizedCell"

   public init(fixture: FeedV1Fixture, resources: FeedV1UIKitResources)
   {
      self.fixture = fixture
      self.resources = resources
      cachedLayout = FeedV1CachedCollectionLayout(fixture: fixture)
      collectionView = UICollectionView(frame: .zero, collectionViewLayout: cachedLayout)
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
      collectionView.register(FeedV1OptimizedCell.self, forCellWithReuseIdentifier: Self.reuseIdentifier)
      collectionView.dataSource = self
      collectionView.delegate = self
   }

   public func mount(at state: FeedV1StartState)
   {
      collectionView.setNeedsLayout()
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
      guard let cell = dequeued as? FeedV1OptimizedCell else
      {
         renderingErrorDescription = "feed-v1 optimized reuse returned an unexpected cell class"
         return dequeued
      }
      guard let row = fixture.row(at: indexPath.item) else
      {
         renderingErrorDescription = "feed-v1 optimized data source requested an out-of-range row"
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

   public func collectionView(_ collectionView: UICollectionView, shouldSelectItemAt indexPath: IndexPath) -> Bool
   {
      false
   }
}

extension FeedV1OptimizedUIKitView: UICollectionViewDelegate
{
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

@MainActor
private final class FeedV1OptimizedCell: UICollectionViewCell
{
   private static let cachedFrames: [Int: FeedV1CellFrames] = Dictionary(
      uniqueKeysWithValues: FeedV1Recipe.rowHeightPoints.map
      {
         ($0, FeedV1CellFrames.make(rowHeightPoints: $0))
      }
   )

   private var cellContent: FeedV1CellContent?
   private var representedRowID: String?
   private var frames: FeedV1CellFrames?

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
      if let frames
      {
         cellContent?.layout(frames: frames)
      }
   }

   override func prepareForReuse()
   {
      super.prepareForReuse()
      representedRowID = nil
      frames = nil
      cellContent?.clear()
   }

   func apply(row: FeedV1Row, resources: FeedV1UIKitResources) throws
   {
      if cellContent == nil
      {
         cellContent = FeedV1CellContent(
            contentView: contentView,
            resources: resources,
            cacheCaptionComposition: true
         )
      }
      guard representedRowID != row.id else
      {
         return
      }
      guard let cachedFrames = Self.cachedFrames[row.heightPoints] else
      {
         throw FeedV1ContractError.invariant("optimized cell received unknown row height \(row.heightPoints)")
      }

      try cellContent?.apply(row: row)
      representedRowID = row.id
      frames = cachedFrames
      setNeedsLayout()
   }
}

@MainActor
private final class FeedV1CachedCollectionLayout: UICollectionViewLayout
{
   private let fixture: FeedV1Fixture
   private var itemAttributes = [UICollectionViewLayoutAttributes]()
   private var cachedVisibleRanges = [UInt64: [UICollectionViewLayoutAttributes]]()

   init(fixture: FeedV1Fixture)
   {
      self.fixture = fixture
      super.init()
      buildCache()
   }

   required init?(coder: NSCoder)
   {
      nil
   }

   override var collectionViewContentSize: CGSize
   {
      CGSize(
         width: FeedV1Contract.surfaceWidthPoints,
         height: fixture.contentExtentPoints
      )
   }

   override func layoutAttributesForElements(in rect: CGRect) -> [UICollectionViewLayoutAttributes]?
   {
      guard !rect.isNull, !rect.isEmpty, !itemAttributes.isEmpty else
      {
         return []
      }
      let first = fixture.firstRowIndex(intersectingContentY: Int(floor(rect.minY)))
      let last = fixture.firstRowIndex(intersectingContentY: Int(ceil(rect.maxY)) - 1)
      guard first <= last else
      {
         return []
      }
      if let cached = cachedVisibleRanges[rangeKey(first: first, last: last)]
      {
         return cached
      }
      return Array(itemAttributes[first ... last])
   }

   override func layoutAttributesForItem(at indexPath: IndexPath) -> UICollectionViewLayoutAttributes?
   {
      guard itemAttributes.indices.contains(indexPath.item) else
      {
         return nil
      }
      return itemAttributes[indexPath.item]
   }

   override func shouldInvalidateLayout(forBoundsChange newBounds: CGRect) -> Bool
   {
      guard let currentBounds = collectionView?.bounds else
      {
         return false
      }
      return newBounds.size != currentBounds.size
   }

   private func buildCache()
   {
      itemAttributes.reserveCapacity(fixture.rowCount)
      for index in 0 ..< fixture.rowCount
      {
         let rowHeight = fixture.rowHeightPrefixPoints[index + 1] - fixture.rowHeightPrefixPoints[index]
         let attributes = UICollectionViewLayoutAttributes(forCellWith: IndexPath(item: index, section: 0))
         attributes.frame = CGRect(
            x: 0,
            y: fixture.rowHeightPrefixPoints[index],
            width: FeedV1Contract.surfaceWidthPoints,
            height: rowHeight
         )
         itemAttributes.append(attributes)
      }

      cachedVisibleRanges.reserveCapacity(fixture.rowCount * 4)
      for first in 0 ..< fixture.rowCount
      {
         let firstStart = fixture.rowHeightPrefixPoints[first]
         let firstEnd = fixture.rowHeightPrefixPoints[first + 1] - 1
         for queryHeight in [FeedV1Contract.surfaceHeightPoints, FeedV1Contract.surfaceHeightPoints * 2]
         {
            let firstLast = fixture.firstRowIndex(intersectingContentY: firstStart + queryHeight - 1)
            let finalLast = fixture.firstRowIndex(intersectingContentY: firstEnd + queryHeight - 1)
            for last in firstLast ... finalLast
            {
               cachedVisibleRanges[rangeKey(first: first, last: last)] = Array(itemAttributes[first ... last])
            }
         }
      }
   }

   private func rangeKey(first: Int, last: Int) -> UInt64
   {
      (UInt64(UInt32(first)) << 32) | UInt64(UInt32(last))
   }
}
