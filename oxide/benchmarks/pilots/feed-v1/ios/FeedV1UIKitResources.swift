import CoreText
import Foundation
import UIKit

public enum FeedV1UIKitError: Error, CustomStringConvertible
{
   case missingFont(String)
   case fontByteCount(name: String, expected: Int, observed: Int)
   case fontHash(name: String, expected: String, observed: String)
   case fontRegistration(name: String, reason: String)
   case fontFace(String)
   case checkerImage(Int)

   public var description: String
   {
      switch self
      {
         case .missingFont(let name):
            return "feed-v1 font resource is missing: \(name)"
         case .fontByteCount(let name, let expected, let observed):
            return "feed-v1 font \(name) has \(observed) bytes; expected \(expected)"
         case .fontHash(let name, let expected, let observed):
            return "feed-v1 font \(name) hash mismatch: expected \(expected), observed \(observed)"
         case .fontRegistration(let name, let reason):
            return "feed-v1 font \(name) could not be registered: \(reason)"
         case .fontFace(let name):
            return "feed-v1 registered font face is unavailable: \(name)"
         case .checkerImage(let variant):
            return "feed-v1 checker image could not be created for variant \(variant)"
      }
   }
}

@MainActor
public final class FeedV1UIKitResources
{
   public let titleFont: UIFont
   public let captionFont: UIFont
   public let metadataFont: UIFont
   public let backgroundColor: UIColor
   public let titleColor: UIColor
   public let captionColor: UIColor
   public let metadataColor: UIColor
   public let separatorColor: UIColor
   public let shadowColor: UIColor

   private var checkerImages = [UIImage?](repeating: nil, count: FeedV1Recipe.checkerVariantCount)
   private let captionAttributes: [NSAttributedString.Key: Any]
   private var cachedCaptionComposition = [String: NSAttributedString]()

   public init(bundle: Bundle) throws
   {
      try Self.register(FeedV1Contract.regularFont, from: bundle)
      try Self.register(FeedV1Contract.boldFont, from: bundle)

      guard let titleFont = UIFont(name: FeedV1Contract.boldFont.postScriptName, size: CGFloat(FeedV1Contract.titleFontPoints)) else
      {
         throw FeedV1UIKitError.fontFace(FeedV1Contract.boldFont.postScriptName)
      }
      guard let captionFont = UIFont(name: FeedV1Contract.regularFont.postScriptName, size: CGFloat(FeedV1Contract.captionFontPoints)) else
      {
         throw FeedV1UIKitError.fontFace(FeedV1Contract.regularFont.postScriptName)
      }
      guard let metadataFont = UIFont(name: FeedV1Contract.regularFont.postScriptName, size: CGFloat(FeedV1Contract.metadataFontPoints)) else
      {
         throw FeedV1UIKitError.fontFace(FeedV1Contract.regularFont.postScriptName)
      }

      self.titleFont = titleFont
      self.captionFont = captionFont
      self.metadataFont = metadataFont
      backgroundColor = UIColor(feedV1: FeedV1Contract.background)
      titleColor = UIColor(feedV1: FeedV1Contract.titleColor)
      captionColor = UIColor(feedV1: FeedV1Contract.captionColor)
      metadataColor = UIColor(feedV1: FeedV1Contract.metadataColor)
      separatorColor = UIColor(feedV1: FeedV1Contract.separatorColor)
      shadowColor = UIColor(feedV1: FeedV1Contract.shadowColor)

      let paragraph = NSMutableParagraphStyle()
      paragraph.minimumLineHeight = CGFloat(FeedV1Contract.captionLineHeightPoints)
      paragraph.maximumLineHeight = CGFloat(FeedV1Contract.captionLineHeightPoints)
      paragraph.lineSpacing = 0
      paragraph.paragraphSpacing = 0
      paragraph.lineBreakMode = .byClipping
      captionAttributes = [
         .font: captionFont,
         .foregroundColor: captionColor,
         .paragraphStyle: paragraph.copy() as Any
      ]
   }

   public func caption(row: FeedV1Row, cacheComposition: Bool) -> NSAttributedString
   {
      if cacheComposition, let cached = cachedCaptionComposition[row.id]
      {
         return cached
      }
      let caption = NSAttributedString(string: row.caption, attributes: captionAttributes)
      if cacheComposition
      {
         cachedCaptionComposition[row.id] = caption
      }
      return caption
   }

   public func checkerImage(variant: Int) throws -> UIImage
   {
      guard checkerImages.indices.contains(variant) else
      {
         throw FeedV1UIKitError.checkerImage(variant)
      }
      if let cached = checkerImages[variant]
      {
         return cached
      }

      let rgba = try FeedV1Contract.checkerRGBABytes(variant: variant)
      guard let provider = CGDataProvider(data: rgba as CFData) else
      {
         throw FeedV1UIKitError.checkerImage(variant)
      }
      guard let colorSpace = CGColorSpace(name: CGColorSpace.sRGB) else
      {
         throw FeedV1UIKitError.checkerImage(variant)
      }
      let alphaInfo = CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue)
      guard let cgImage = CGImage(
         width: FeedV1Recipe.checkerSidePixels,
         height: FeedV1Recipe.checkerSidePixels,
         bitsPerComponent: 8,
         bitsPerPixel: 32,
         bytesPerRow: FeedV1Recipe.checkerSidePixels * 4,
         space: colorSpace,
         bitmapInfo: alphaInfo,
         provider: provider,
         decode: nil,
         shouldInterpolate: false,
         intent: .defaultIntent
      ) else
      {
         throw FeedV1UIKitError.checkerImage(variant)
      }

      let image = UIImage(cgImage: cgImage, scale: 1, orientation: .up)
      checkerImages[variant] = image
      return image
   }

   private static func register(_ reference: FeedV1FontReference, from bundle: Bundle) throws
   {
      guard let url = bundle.url(forResource: reference.bundleName, withExtension: nil) else
      {
         throw FeedV1UIKitError.missingFont(reference.bundleName)
      }
      let bytes = try Data(contentsOf: url, options: .mappedIfSafe)
      guard bytes.count == reference.byteCount else
      {
         throw FeedV1UIKitError.fontByteCount(
            name: reference.bundleName,
            expected: reference.byteCount,
            observed: bytes.count
         )
      }
      let observedHash = FeedV1Hash.sha256Hex(bytes)
      guard observedHash == reference.sha256 else
      {
         throw FeedV1UIKitError.fontHash(
            name: reference.bundleName,
            expected: reference.sha256,
            observed: observedHash
         )
      }
      if UIFont(name: reference.postScriptName, size: 12) != nil
      {
         return
      }

      var registrationError: Unmanaged<CFError>?
      guard CTFontManagerRegisterFontsForURL(url as CFURL, .process, &registrationError) else
      {
         let reason = registrationError?.takeRetainedValue().localizedDescription ?? "Core Text returned no reason"
         throw FeedV1UIKitError.fontRegistration(name: reference.bundleName, reason: reason)
      }
   }
}

private extension UIColor
{
   convenience init(feedV1 color: FeedV1RGBA)
   {
      self.init(
         red: CGFloat(color.red) / 255,
         green: CGFloat(color.green) / 255,
         blue: CGFloat(color.blue) / 255,
         alpha: CGFloat(color.alpha) / 255
      )
   }
}
