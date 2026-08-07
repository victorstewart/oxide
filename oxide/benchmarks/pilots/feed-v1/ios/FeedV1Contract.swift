import CryptoKit
import Foundation

public struct FeedV1RGBA: Equatable, Sendable
{
   public let red: UInt8
   public let green: UInt8
   public let blue: UInt8
   public let alpha: UInt8

   public init(_ red: UInt8, _ green: UInt8, _ blue: UInt8, _ alpha: UInt8 = 255)
   {
      self.red = red
      self.green = green
      self.blue = blue
      self.alpha = alpha
   }
}

public struct FeedV1FontReference: Equatable, Sendable
{
   public let repositoryPath: String
   public let bundleName: String
   public let postScriptName: String
   public let sha256: String
   public let byteCount: Int

   public init(repositoryPath: String, bundleName: String, postScriptName: String, sha256: String, byteCount: Int)
   {
      self.repositoryPath = repositoryPath
      self.bundleName = bundleName
      self.postScriptName = postScriptName
      self.sha256 = sha256
      self.byteCount = byteCount
   }
}

public struct FeedV1Row: Equatable, Sendable
{
   public let id: String
   public let title: String
   public let caption: String
   public let metadata: String
   public let heightPoints: Int
   public let checkerVariant: Int

   public init(id: String, title: String, caption: String, metadata: String, heightPoints: Int, checkerVariant: Int)
   {
      self.id = id
      self.title = title
      self.caption = caption
      self.metadata = metadata
      self.heightPoints = heightPoints
      self.checkerVariant = checkerVariant
   }

   public func componentID(_ kind: FeedV1ComponentKind) -> String
   {
      "\(id)/\(kind.rawValue)"
   }
}

public enum FeedV1ComponentKind: String, CaseIterable, Codable, Sendable
{
   case row
   case image
   case title
   case caption
   case metadata
   case separator
}

public struct FeedV1PhysicalRect: Codable, Equatable, Sendable
{
   public let x: Int
   public let y: Int
   public let width: Int
   public let height: Int

   public init(x: Int, y: Int, width: Int, height: Int)
   {
      self.x = x
      self.y = y
      self.width = width
      self.height = height
   }
}

public struct FeedV1ComponentRecord: Codable, Equatable, Sendable
{
   public let id: String
   public let kind: FeedV1ComponentKind
   public let rowIndex: Int
   public let contentRectPx: FeedV1PhysicalRect
   public let viewportClipPx: FeedV1PhysicalRect

   public init(id: String, kind: FeedV1ComponentKind, rowIndex: Int, contentRectPx: FeedV1PhysicalRect, viewportClipPx: FeedV1PhysicalRect)
   {
      self.id = id
      self.kind = kind
      self.rowIndex = rowIndex
      self.contentRectPx = contentRectPx
      self.viewportClipPx = viewportClipPx
   }

   private enum CodingKeys: String, CodingKey
   {
      case id
      case kind
      case rowIndex = "row_index"
      case contentRectPx = "content_rect_px"
      case viewportClipPx = "viewport_clip_px"
   }
}

public enum FeedV1Direction: String, CaseIterable, Sendable
{
   case forward
   case reverse
}

public enum FeedV1StartState: String, CaseIterable, Sendable
{
   case top
   case bottom

   public var outboundDirection: FeedV1Direction
   {
      switch self
      {
         case .top:
            return .forward
         case .bottom:
            return .reverse
      }
   }
}

public enum FeedV1ContractError: Error, Equatable, CustomStringConvertible
{
   case invariant(String)
   case canonicalByteCount(expected: Int, observed: Int)
   case canonicalHash(expected: String, observed: String)

   public var description: String
   {
      switch self
      {
         case .invariant(let message):
            return "feed-v1 invariant failed: \(message)"
         case .canonicalByteCount(let expected, let observed):
            return "feed-v1 canonical byte count mismatch: expected \(expected), observed \(observed)"
         case .canonicalHash(let expected, let observed):
            return "feed-v1 canonical hash mismatch: expected \(expected), observed \(observed)"
      }
   }
}

func feedV1CaptureEnvironmentAdmission<Snapshot, Environment>(
   snapshot: () -> Snapshot,
   environment: () -> Environment?
) -> (snapshot: Snapshot, environment: Environment)?
{
   let capturedSnapshot = snapshot()
   guard let capturedEnvironment = environment() else
   {
      return nil
   }
   return (capturedSnapshot, capturedEnvironment)
}

public enum FeedV1Contract
{
   public static let schema = "oxide.feed-v1.fixture"
   public static let revision = 1
   public static let localeIdentifier = "en_US_POSIX"
   public static let rgbaColorSpaceName = "sRGB IEC61966-2.1"
   public static let checkerPixelRule = "RGBA8:premultiplied-last:opaque"
   public static let imageRasterRule = "nearest:circular-corner"
   public static let interfaceStyleRule = "light-fixed:left-to-right:portrait-locked"
   public static let expectedCanonicalByteCount = 717_745
   public static let expectedCanonicalSHA256 = "a1de9b4a914734fe21d21e9b6f8a9b61970f7e22e0fa4ef0103031e399881473"

   public static let rowCount = 2_000
   public static let hostWidthPoints = 440
   public static let hostHeightPoints = 956
   public static let surfaceWidthPoints = 390
   public static let surfaceHeightPoints = 844
   public static let surfaceScale = 3
   public static let surfaceOriginXPoints = 25
   public static let surfaceOriginYPoints = 56
   public static let surfacePlacementRule = "center-exact:host440x956:surface390x844:origin25x56"
   public static let safeAreaTopPoints = 0
   public static let safeAreaLeftPoints = 0
   public static let safeAreaBottomPoints = 0
   public static let safeAreaRightPoints = 0

   public static let treatmentEnvironmentKey = "OXIDE_FEED_V1_TREATMENT"
   public static let startStateEnvironmentKey = "OXIDE_FEED_V1_START_STATE"
   public static let completionNonceEnvironmentKey = "OXIDE_FEED_V1_COMPLETION_NONCE"
   public static let idiomaticUIKitVariantValue = "uikit-idiomatic"
   public static let optimizedUIKitVariantValue = "uikit-optimized"
   public static let oxideVariantValue = "oxide"
   public static let resultDirectoryName = "Documents"
   public static let resultFilePrefix = "oxide-feed-v1-"
   public static let resultFileSuffix = ".json"
   public static let readyNotificationPrefix = "com.oxide.feed-v1.ready."
   public static let completionNotificationPrefix = "com.oxide.feed-v1.complete."
   public static let failureNotificationPrefix = "com.oxide.feed-v1.failed."
   public static let runRecordSchema = "oxide.feed-v1.run"
   public static let runRecordSchemaRevision = 1
   public static let displayLinkMinimumFramesPerSecond = 120
   public static let displayLinkMaximumFramesPerSecond = 120
   public static let displayLinkPreferredFramesPerSecond = 120

   public static func completionNonceIsValid(_ nonce: String) -> Bool
   {
      let bytes = nonce.utf8
      guard !bytes.isEmpty, bytes.count <= 128 else
      {
         return false
      }
      return bytes.allSatisfy
      {
         (48 ... 57).contains($0)
            || (65 ... 90).contains($0)
            || (97 ... 122).contains($0)
            || $0 == 45
      }
   }

   public static let rowLeadingPoints = 14
   public static let rowTrailingPoints = 14
   public static let rowTopPoints = 12
   public static let imageSidePoints = 56
   public static let imageTextGapPoints = 12
   public static let imageCornerRadiusPoints = 10
   public static let shadowOffsetXPoints = 2
   public static let shadowOffsetYPoints = 2
   public static let shadowBlurRadiusPoints = 0
   public static let shadowLayerOpacityByte = 255
   public static let titleHeightPoints = 20
   public static let captionTopPoints = 36
   public static let captionLineHeightPoints = 18
   public static let captionParagraphRule = "line-height:min=18,max=18;line-spacing=0;paragraph-spacing=0;clip"
   public static let metadataGapPoints = 4
   public static let metadataHeightPoints = 16
   public static let separatorPhysicalPixels = 1

   public static let titleFontPoints = 16
   public static let captionFontPoints = 14
   public static let metadataFontPoints = 12

   public static let background = FeedV1RGBA(247, 244, 238)
   public static let titleColor = FeedV1RGBA(28, 36, 48)
   public static let captionColor = FeedV1RGBA(79, 93, 107)
   public static let metadataColor = FeedV1RGBA(123, 132, 144)
   public static let separatorColor = FeedV1RGBA(215, 209, 199)
   public static let shadowColor = FeedV1RGBA(38, 43, 50, 102)

   public static let viewportClipPhysicalPixels = FeedV1PhysicalRect(
      x: 0,
      y: 0,
      width: surfaceWidthPoints * surfaceScale,
      height: surfaceHeightPoints * surfaceScale
   )

   public static let regularFont = FeedV1FontReference(
      repositoryPath: "oxide/crates/ui-core/assets/Asap-Regular.ttf",
      bundleName: "Asap-Regular.ttf",
      postScriptName: "Asap-Regular",
      sha256: "7d494f276293fb0a8e2aab1fc0e386baa3e8a1d90927f518abb152b5c73e29f9",
      byteCount: 30_740
   )
   public static let boldFont = FeedV1FontReference(
      repositoryPath: "oxide/crates/ui-core/assets/Asap-Bold.ttf",
      bundleName: "Asap-Bold.ttf",
      postScriptName: "Asap-Bold",
      sha256: "7f4feacd835eed23e104413f800a74b9f0270ce8c754c990bfc09b796a3ca628",
      byteCount: 30_352
   )

   public static func materialize() throws -> FeedV1Fixture
   {
      let fixture = try materializeUnchecked()
      guard fixture.canonicalByteCount == expectedCanonicalByteCount else
      {
         throw FeedV1ContractError.canonicalByteCount(
            expected: expectedCanonicalByteCount,
            observed: fixture.canonicalByteCount
         )
      }
      guard fixture.canonicalSHA256 == expectedCanonicalSHA256 else
      {
         throw FeedV1ContractError.canonicalHash(
            expected: expectedCanonicalSHA256,
            observed: fixture.canonicalSHA256
         )
      }
      return fixture
   }

   static func materializeUnchecked() throws -> FeedV1Fixture
   {
      var prefix = [Int]()
      prefix.reserveCapacity(rowCount + 1)
      prefix.append(0)

      for index in 0 ..< rowCount
      {
         prefix.append(prefix[index] + FeedV1Recipe.rowHeightPoints(at: index))
      }

      guard prefix.count == rowCount + 1 else
      {
         throw FeedV1ContractError.invariant("prefix table length is not row count plus one")
      }

      let canonicalBytes = FeedV1CanonicalEncoder.encode(prefix: prefix)
      let canonicalSHA256 = FeedV1Hash.sha256Hex(canonicalBytes)
      return FeedV1Fixture(
         rowHeightPrefixPoints: prefix,
         canonicalSHA256: canonicalSHA256,
         canonicalByteCount: canonicalBytes.count
      )
   }

   public static func checkerRGBABytes(variant: Int) throws -> Data
   {
      guard (0 ..< FeedV1Recipe.checkerVariantCount).contains(variant) else
      {
         throw FeedV1ContractError.invariant("checker variant \(variant) is out of range")
      }
      return FeedV1Recipe.checkerRGBABytes(variant: variant)
   }

   public static func canonicalBytesForAudit() throws -> Data
   {
      let fixture = try materialize()
      return FeedV1CanonicalEncoder.encode(prefix: fixture.rowHeightPrefixPoints)
   }
}

public struct FeedV1Fixture: Sendable
{
   public let rowHeightPrefixPoints: [Int]
   public let canonicalSHA256: String
   public let canonicalByteCount: Int

   public var rowCount: Int
   {
      FeedV1Contract.rowCount
   }

   public func row(at index: Int) -> FeedV1Row?
   {
      guard (0 ..< rowCount).contains(index) else
      {
         return nil
      }
      return FeedV1Recipe.row(at: index)
   }

   public func rowHeightPoints(at index: Int) -> Int?
   {
      guard (0 ..< rowCount).contains(index) else
      {
         return nil
      }
      return rowHeightPrefixPoints[index + 1] - rowHeightPrefixPoints[index]
   }

   public var contentExtentPoints: Int
   {
      rowHeightPrefixPoints[rowHeightPrefixPoints.count - 1]
   }

   public var maximumContentOffsetPoints: Int
   {
      max(contentExtentPoints - FeedV1Contract.surfaceHeightPoints, 0)
   }

   public func contentOffsetPoints(for state: FeedV1StartState) -> Int
   {
      switch state
      {
         case .top:
            return 0
         case .bottom:
            return maximumContentOffsetPoints
      }
   }

   public func firstRowIndex(intersectingContentY contentY: Int) -> Int
   {
      let clampedY = min(max(contentY, 0), max(contentExtentPoints - 1, 0))
      var lower = 0
      var upper = rowCount
      while lower < upper
      {
         let middle = lower + ((upper - lower) / 2)
         if rowHeightPrefixPoints[middle + 1] <= clampedY
         {
            lower = middle + 1
         }
         else
         {
            upper = middle
         }
      }
      return min(lower, max(rowCount - 1, 0))
   }

   public func componentRectPhysicalPixels(rowIndex: Int, kind: FeedV1ComponentKind) throws -> FeedV1PhysicalRect
   {
      guard let rowHeight = rowHeightPoints(at: rowIndex) else
      {
         throw FeedV1ContractError.invariant("component row index \(rowIndex) is out of range")
      }
      let scale = FeedV1Contract.surfaceScale
      let rowY = rowHeightPrefixPoints[rowIndex]
      let textX = FeedV1Contract.rowLeadingPoints + FeedV1Contract.imageSidePoints + FeedV1Contract.imageTextGapPoints
      let textWidth = FeedV1Contract.surfaceWidthPoints - textX - FeedV1Contract.rowTrailingPoints
      let captionHeight = FeedV1Recipe.lineCount(forHeight: rowHeight) * FeedV1Contract.captionLineHeightPoints
      let metadataY = FeedV1Contract.captionTopPoints + captionHeight + FeedV1Contract.metadataGapPoints

      switch kind
      {
         case .row:
            return FeedV1PhysicalRect(x: 0, y: rowY * scale, width: FeedV1Contract.surfaceWidthPoints * scale, height: rowHeight * scale)
         case .image:
            return FeedV1PhysicalRect(
               x: FeedV1Contract.rowLeadingPoints * scale,
               y: (rowY + FeedV1Contract.rowTopPoints) * scale,
               width: FeedV1Contract.imageSidePoints * scale,
               height: FeedV1Contract.imageSidePoints * scale
            )
         case .title:
            return FeedV1PhysicalRect(
               x: textX * scale,
               y: (rowY + FeedV1Contract.rowTopPoints) * scale,
               width: textWidth * scale,
               height: FeedV1Contract.titleHeightPoints * scale
            )
         case .caption:
            return FeedV1PhysicalRect(
               x: textX * scale,
               y: (rowY + FeedV1Contract.captionTopPoints) * scale,
               width: textWidth * scale,
               height: captionHeight * scale
            )
         case .metadata:
            return FeedV1PhysicalRect(
               x: textX * scale,
               y: (rowY + metadataY) * scale,
               width: textWidth * scale,
               height: FeedV1Contract.metadataHeightPoints * scale
            )
         case .separator:
            return FeedV1PhysicalRect(
               x: 0,
               y: ((rowY + rowHeight) * scale) - FeedV1Contract.separatorPhysicalPixels,
               width: FeedV1Contract.surfaceWidthPoints * scale,
               height: FeedV1Contract.separatorPhysicalPixels
            )
      }
   }

   public func visibleComponentRecords(contentOffsetPoints: Double) throws -> [FeedV1ComponentRecord]
   {
      guard contentOffsetPoints.isFinite else
      {
         throw FeedV1ContractError.invariant("visible component offset is not finite")
      }
      let clampedOffset = min(max(contentOffsetPoints, 0), Double(maximumContentOffsetPoints))
      let viewportTopPhysicalPixels = Int(
         (clampedOffset * Double(FeedV1Contract.surfaceScale)).rounded()
      )
      let viewportBottomPhysicalPixels = viewportTopPhysicalPixels
         + FeedV1Contract.viewportClipPhysicalPixels.height
      let first = firstRowIndex(intersectingContentY: Int(floor(clampedOffset)))
      let last = firstRowIndex(intersectingContentY: Int(ceil(clampedOffset + Double(FeedV1Contract.surfaceHeightPoints))) - 1)
      var records = [FeedV1ComponentRecord]()
      records.reserveCapacity((last - first + 1) * FeedV1ComponentKind.allCases.count)

      for rowIndex in first ... last
      {
         guard let row = row(at: rowIndex) else
         {
            throw FeedV1ContractError.invariant("visible component row index \(rowIndex) is out of range")
         }
         for kind in FeedV1ComponentKind.allCases
         {
            let rect = try componentRectPhysicalPixels(rowIndex: rowIndex, kind: kind)
            let left = max(rect.x, 0)
            let top = max(rect.y, viewportTopPhysicalPixels)
            let right = min(
               rect.x + rect.width,
               FeedV1Contract.viewportClipPhysicalPixels.width
            )
            let bottom = min(rect.y + rect.height, viewportBottomPhysicalPixels)
            if left < right && top < bottom
            {
               records.append(FeedV1ComponentRecord(
                  id: row.componentID(kind),
                  kind: kind,
                  rowIndex: rowIndex,
                  contentRectPx: rect,
                  viewportClipPx: FeedV1PhysicalRect(
                     x: left,
                     y: top - viewportTopPhysicalPixels,
                     width: right - left,
                     height: bottom - top
                  )
               ))
            }
         }
      }
      return records
   }
}

enum FeedV1Recipe
{
   static let checkerSidePixels = 12
   static let checkerVariantCount = 64
   static let rowHeightPoints = [92, 110, 128, 146]
   static let mixAlgorithm = "splitmix32-v1:seed=0x6f786964"
   static let checkerAlgorithm = "checker12-v1:palette3,tile2|3|4|6,phase"

   private static let authors = [
      "Avery Chen", "Maya Singh", "Theo Brooks", "Nora Kim",
      "Iris Martin", "Leo Foster", "Zoe Parker", "Evan Reyes",
      "Mina Patel", "Owen Price", "Clara Stone", "Noah Grant"
   ]
   private static let captionLines = [
      "Stable identity keeps work local.",
      "Cold images arrive during motion.",
      "One viewport, one measured feed.",
      "Frames preserve the visible contract.",
      "Rows reuse exact cached resources.",
      "Geometry is frozen before results."
   ]
   private static let palettes = [
      CheckerPalette(FeedV1RGBA(228, 87, 46), FeedV1RGBA(243, 167, 18)),
      CheckerPalette(FeedV1RGBA(46, 134, 171), FeedV1RGBA(113, 180, 141)),
      CheckerPalette(FeedV1RGBA(114, 70, 145), FeedV1RGBA(240, 160, 190)),
      CheckerPalette(FeedV1RGBA(27, 153, 139), FeedV1RGBA(237, 201, 81)),
      CheckerPalette(FeedV1RGBA(197, 61, 75), FeedV1RGBA(95, 173, 199)),
      CheckerPalette(FeedV1RGBA(83, 74, 183), FeedV1RGBA(242, 180, 66)),
      CheckerPalette(FeedV1RGBA(67, 160, 71), FeedV1RGBA(236, 103, 65)),
      CheckerPalette(FeedV1RGBA(36, 106, 115), FeedV1RGBA(232, 164, 74))
   ]

   static func rowHeightPoints(at index: Int) -> Int
   {
      let mixed = mix32(UInt32(index))
      return rowHeightPoints[Int((mixed >> 5) & 3)]
   }

   static func row(at index: Int) -> FeedV1Row
   {
      let mixed = mix32(UInt32(index))
      let heightIndex = Int((mixed >> 5) & 3)
      let height = rowHeightPoints[heightIndex]
      let lines = heightIndex + 1
      let author = authors[Int(mixed % UInt32(authors.count))]
      let number = fourDigits(index)
      let captionStart = Int((mixed >> 9) % UInt32(captionLines.count))
      var selectedLines = [String]()
      selectedLines.reserveCapacity(lines)
      for line in 0 ..< lines
      {
         selectedLines.append(captionLines[(captionStart + line) % captionLines.count])
      }
      let replyCount = Int((mixed >> 23) % 97)

      return FeedV1Row(
         id: "feed-v1-row-\(number)",
         title: "\(author) · Update \(number)",
         caption: selectedLines.joined(separator: "\n"),
         metadata: "Row \(number) · \(replyCount) replies",
         heightPoints: height,
         checkerVariant: Int((mixed >> 16) & 63)
      )
   }

   static func lineCount(forHeight height: Int) -> Int
   {
      guard let index = rowHeightPoints.firstIndex(of: height) else
      {
         return 0
      }
      return index + 1
   }

   static func checkerRGBABytes(variant: Int) -> Data
   {
      let palette = palettes[variant & 7]
      let tileSizes = [2, 3, 4, 6]
      let tileSize = tileSizes[(variant >> 3) & 3]
      let phase = (variant >> 5) & 1
      var bytes = Data()
      bytes.reserveCapacity(checkerSidePixels * checkerSidePixels * 4)

      for y in 0 ..< checkerSidePixels
      {
         for x in 0 ..< checkerSidePixels
         {
            let color = (((x / tileSize) + (y / tileSize) + phase) & 1) == 0 ? palette.first : palette.second
            bytes.append(color.red)
            bytes.append(color.green)
            bytes.append(color.blue)
            bytes.append(color.alpha)
         }
      }
      return bytes
   }

   private static func mix32(_ value: UInt32) -> UInt32
   {
      var mixed = value &+ 0x6f78_6964
      mixed ^= mixed >> 16
      mixed &*= 0x7feb_352d
      mixed ^= mixed >> 15
      mixed &*= 0x846c_a68b
      mixed ^= mixed >> 16
      return mixed
   }

   private static func fourDigits(_ value: Int) -> String
   {
      let digits = [
         UInt8(48 + ((value / 1_000) % 10)),
         UInt8(48 + ((value / 100) % 10)),
         UInt8(48 + ((value / 10) % 10)),
         UInt8(48 + (value % 10))
      ]
      return String(decoding: digits, as: UTF8.self)
   }
}

private struct CheckerPalette
{
   let first: FeedV1RGBA
   let second: FeedV1RGBA

   init(_ first: FeedV1RGBA, _ second: FeedV1RGBA)
   {
      self.first = first
      self.second = second
   }
}

private enum FeedV1CanonicalEncoder
{
   static func encode(prefix: [Int]) -> Data
   {
      var bytes = Data()
      bytes.reserveCapacity(320_000)
      bytes.appendString(FeedV1Contract.schema)
      bytes.appendUInt32(UInt32(FeedV1Contract.revision))
      bytes.appendString(FeedV1Recipe.mixAlgorithm)
      bytes.appendString(FeedV1Recipe.checkerAlgorithm)
      bytes.appendString(FeedV1Contract.localeIdentifier)
      bytes.appendString(FeedV1Contract.rgbaColorSpaceName)
      bytes.appendString(FeedV1Contract.checkerPixelRule)
      bytes.appendString(FeedV1Contract.imageRasterRule)
      bytes.appendString(FeedV1Contract.interfaceStyleRule)
      bytes.appendString(FeedV1Contract.surfacePlacementRule)
      bytes.appendString(FeedV1Contract.treatmentEnvironmentKey)
      bytes.appendString(FeedV1Contract.startStateEnvironmentKey)
      bytes.appendString(FeedV1Contract.completionNonceEnvironmentKey)
      bytes.appendString(FeedV1Contract.idiomaticUIKitVariantValue)
      bytes.appendString(FeedV1Contract.optimizedUIKitVariantValue)
      bytes.appendString(FeedV1Contract.oxideVariantValue)
      bytes.appendString(FeedV1Contract.resultDirectoryName)
      bytes.appendString(FeedV1Contract.resultFilePrefix)
      bytes.appendString(FeedV1Contract.resultFileSuffix)
      bytes.appendString(FeedV1Contract.readyNotificationPrefix)
      bytes.appendString(FeedV1Contract.completionNotificationPrefix)
      bytes.appendString(FeedV1Contract.failureNotificationPrefix)
      bytes.appendString(FeedV1Contract.runRecordSchema)
      bytes.appendUInt32(UInt32(FeedV1Contract.runRecordSchemaRevision))
      bytes.appendString(FeedV1Contract.captionParagraphRule)
      for kind in FeedV1ComponentKind.allCases
      {
         bytes.appendString(kind.rawValue)
      }
      for state in FeedV1StartState.allCases
      {
         bytes.appendString(state.rawValue)
         bytes.appendString(state.outboundDirection.rawValue)
      }

      let integers = [
         FeedV1Contract.rowCount,
         FeedV1Contract.hostWidthPoints,
         FeedV1Contract.hostHeightPoints,
         FeedV1Contract.surfaceWidthPoints,
         FeedV1Contract.surfaceHeightPoints,
         FeedV1Contract.surfaceScale,
         FeedV1Contract.surfaceOriginXPoints,
         FeedV1Contract.surfaceOriginYPoints,
         FeedV1Contract.safeAreaTopPoints,
         FeedV1Contract.safeAreaLeftPoints,
         FeedV1Contract.safeAreaBottomPoints,
         FeedV1Contract.safeAreaRightPoints,
         FeedV1Contract.displayLinkMinimumFramesPerSecond,
         FeedV1Contract.displayLinkMaximumFramesPerSecond,
         FeedV1Contract.displayLinkPreferredFramesPerSecond,
         FeedV1Contract.rowLeadingPoints,
         FeedV1Contract.rowTrailingPoints,
         FeedV1Contract.rowTopPoints,
         FeedV1Contract.imageSidePoints,
         FeedV1Contract.imageTextGapPoints,
         FeedV1Contract.imageCornerRadiusPoints,
         FeedV1Contract.shadowOffsetXPoints,
         FeedV1Contract.shadowOffsetYPoints,
         FeedV1Contract.shadowBlurRadiusPoints,
         FeedV1Contract.shadowLayerOpacityByte,
         FeedV1Contract.titleHeightPoints,
         FeedV1Contract.captionTopPoints,
         FeedV1Contract.captionLineHeightPoints,
         FeedV1Contract.metadataGapPoints,
         FeedV1Contract.metadataHeightPoints,
         FeedV1Contract.separatorPhysicalPixels,
         FeedV1Contract.titleFontPoints,
         FeedV1Contract.captionFontPoints,
         FeedV1Contract.metadataFontPoints,
         FeedV1Recipe.checkerSidePixels,
         FeedV1Recipe.checkerVariantCount
      ]
      for integer in integers
      {
         bytes.appendUInt32(UInt32(integer))
      }

      for color in [
         FeedV1Contract.background,
         FeedV1Contract.titleColor,
         FeedV1Contract.captionColor,
         FeedV1Contract.metadataColor,
         FeedV1Contract.separatorColor,
         FeedV1Contract.shadowColor
      ]
      {
         bytes.append(color.red)
         bytes.append(color.green)
         bytes.append(color.blue)
         bytes.append(color.alpha)
      }

      for font in [FeedV1Contract.regularFont, FeedV1Contract.boldFont]
      {
         bytes.appendString(font.repositoryPath)
         bytes.appendString(font.bundleName)
         bytes.appendString(font.postScriptName)
         bytes.appendString(font.sha256)
         bytes.appendUInt32(UInt32(font.byteCount))
      }

      for variant in 0 ..< FeedV1Recipe.checkerVariantCount
      {
         bytes.appendUInt32(UInt32(variant))
         let checkerBytes = FeedV1Recipe.checkerRGBABytes(variant: variant)
         bytes.appendUInt32(UInt32(checkerBytes.count))
         bytes.append(checkerBytes)
      }

      bytes.appendUInt32(UInt32(FeedV1Contract.rowCount))
      for index in 0 ..< FeedV1Contract.rowCount
      {
         let row = FeedV1Recipe.row(at: index)
         bytes.appendString(row.id)
         bytes.appendString(row.title)
         bytes.appendString(row.caption)
         bytes.appendString(row.metadata)
         bytes.appendUInt32(UInt32(row.heightPoints))
         bytes.appendUInt32(UInt32(row.checkerVariant))
         for kind in FeedV1ComponentKind.allCases
         {
            bytes.appendString(row.componentID(kind))
         }
      }

      bytes.appendUInt32(UInt32(prefix.count))
      for value in prefix
      {
         bytes.appendUInt32(UInt32(value))
      }
      bytes.appendUInt32(UInt32(prefix[prefix.count - 1]))
      bytes.appendUInt32(UInt32(max(prefix[prefix.count - 1] - FeedV1Contract.surfaceHeightPoints, 0)))
      return bytes
   }
}

enum FeedV1Hash
{
   static func sha256Hex(_ data: Data) -> String
   {
      let digest = SHA256.hash(data: data)
      let hexDigits = Array("0123456789abcdef".utf8)
      var output = [UInt8]()
      output.reserveCapacity(64)
      for byte in digest
      {
         output.append(hexDigits[Int(byte >> 4)])
         output.append(hexDigits[Int(byte & 15)])
      }
      return String(decoding: output, as: UTF8.self)
   }
}

private extension Data
{
   mutating func appendUInt32(_ value: UInt32)
   {
      append(UInt8(value & 0xff))
      append(UInt8((value >> 8) & 0xff))
      append(UInt8((value >> 16) & 0xff))
      append(UInt8((value >> 24) & 0xff))
   }

   mutating func appendString(_ value: String)
   {
      let utf8 = Array(value.utf8)
      appendUInt32(UInt32(utf8.count))
      append(contentsOf: utf8)
   }
}
