import AppKit
import CoreText
import CryptoKit
import Foundation
import Metal

enum AppKitProductionPalette
{
   static let accent = NSColor(srgbRed: 61 / 255, green: 110 / 255, blue: 239 / 255, alpha: 1)
   static let background = NSColor(srgbRed: 243 / 255, green: 245 / 255, blue: 248 / 255, alpha: 1)
   static let backdrop = NSColor(srgbRed: 224 / 255, green: 226 / 255, blue: 239 / 255, alpha: 56 / 255)
   static let inactiveControl = NSColor(srgbRed: 180 / 255, green: 186 / 255, blue: 198 / 255, alpha: 1)
   static let modalOverlay = NSColor(srgbRed: 25 / 255, green: 28 / 255, blue: 35 / 255, alpha: 1)
   static let secondaryText = NSColor(srgbRed: 105 / 255, green: 113 / 255, blue: 129 / 255, alpha: 1)
   static let shadow = NSColor(srgbRed: 32 / 255, green: 36 / 255, blue: 44 / 255, alpha: 41 / 255)
   static let surface = NSColor(srgbRed: 1, green: 1, blue: 1, alpha: 1)
   static let text = NSColor(srgbRed: 32 / 255, green: 36 / 255, blue: 44 / 255, alpha: 1)
}

struct AppKitProductionFonts
{
   let latin7: NSFont
   let latin11: NSFont
   let latin13: NSFont
   let latin15: NSFont
   let latin20: NSFont
   let arabic15: NSFont
   let cjk15: NSFont

   init(catalog: BenchmarkAppleFontCatalog) throws
   {
      latin7 = try catalog.font(role: "latin", size: 7) as NSFont
      latin11 = try catalog.font(role: "latin", size: 11) as NSFont
      latin13 = try catalog.font(role: "latin", size: 13) as NSFont
      latin15 = try catalog.font(role: "latin", size: 15) as NSFont
      latin20 = try catalog.font(role: "latin", size: 20) as NSFont
      arabic15 = try catalog.font(role: "arabic", size: 15) as NSFont
      cjk15 = try catalog.font(role: "cjk-simplified", size: 15) as NSFont
   }
}

final class AppKitProductionRootView: NSView
{
   override var isFlipped: Bool {true}

   override func draw(_ dirtyRect: NSRect)
   {
      AppKitProductionPalette.background.setFill()
      bounds.fill()
   }
}

final class AppKitProductionSurfaceView: NSView
{
   override var isFlipped: Bool {true}

   override func draw(_ dirtyRect: NSRect)
   {
      AppKitProductionPalette.surface.setFill()
      bounds.fill()
   }
}

final class AppKitProductionRoundedSurfaceView: NSView
{
   override var isFlipped: Bool {true}

   override func draw(_ dirtyRect: NSRect)
   {
      AppKitProductionPalette.surface.setFill()
      NSBezierPath(roundedRect: bounds, xRadius: 12, yRadius: 12).fill()
   }
}

final class AppKitProductionBackdropView: NSView
{
   override var isFlipped: Bool {true}

   override func draw(_ dirtyRect: NSRect)
   {
      AppKitProductionPalette.backdrop.setFill()
      bounds.fill()
   }
}

final class AppKitProductionRoundedCardView: NSView
{
   private var cardEdges: AppKitProductionShadowedSurfaceEdges?

   var renderedCardBounds: CGRect
   {
      CGRect(x: 0, y: 0, width: bounds.width, height: 38)
   }

   override var isFlipped: Bool {true}

   override func layout()
   {
      super.layout()
      cardEdges = AppKitProductionEdgeRasterCache.shared.shadowedSurfaceEdges(
         card: renderedCardBounds,
         in: self,
         underlay: .dashboardBackdrop
      )
   }

   override func draw(_ dirtyRect: NSRect)
   {
      guard let cardEdges else
      {
         AppKitProductionPalette.shadow.setFill()
         NSBezierPath(roundedRect: CGRect(x: 0, y: 2, width: bounds.width, height: 38), xRadius: 12, yRadius: 12).fill()
         AppKitProductionPalette.surface.setFill()
         NSBezierPath(roundedRect: CGRect(x: 0, y: 0, width: bounds.width, height: 38), xRadius: 12, yRadius: 12).fill()
         return
      }
      let card = renderedCardBounds
      let radius: CGFloat = 12
      AppKitProductionPalette.shadow.setFill()
      CGRect(x: card.minX + radius, y: card.minY + 2, width: card.width - radius * 2, height: card.height).fill()
      CGRect(x: card.minX, y: card.minY + 2 + radius, width: card.width, height: card.height - radius * 2).fill()
      AppKitProductionPalette.surface.setFill()
      CGRect(x: card.minX + radius, y: card.minY, width: card.width - radius * 2, height: card.height).fill()
      CGRect(x: card.minX, y: card.minY + radius, width: card.width, height: card.height - radius * 2).fill()
      cardEdges.draw(in: card)
   }
}

struct AppKitProductionInlinePiece
{
   let text: String
   let image: CGImage?
   let advance: CGFloat
   let width: CGFloat
   let height: CGFloat
   let topFromBaseline: CGFloat
}

private struct AppKitProductionInlineImageKey: Hashable
{
   let atlas: Data
   let atlasWidth: Int
   let atlasHeight: Int
   let sourceX: Int
   let sourceY: Int
   let sourceWidth: Int
   let sourceHeight: Int
}

private final class AppKitProductionInlineImageCache
{
   static let shared = AppKitProductionInlineImageCache()

   private var images = [AppKitProductionInlineImageKey: CGImage]()
   private let lock = NSLock()

   func image(atlas: CGImage, source: CGRect) -> CGImage?
   {
      guard let data = atlas.dataProvider?.data as Data? else {return nil}
      let key = AppKitProductionInlineImageKey(
         atlas: data,
         atlasWidth: atlas.width,
         atlasHeight: atlas.height,
         sourceX: Int(source.minX),
         sourceY: Int(source.minY),
         sourceWidth: Int(source.width),
         sourceHeight: Int(source.height)
      )
      lock.lock()
      defer {lock.unlock()}
      if let image = images[key] {return image}
      guard let image = atlas.cropping(to: source) else {return nil}
      images[key] = image
      return image
   }
}

final class AppKitProductionInlineText
{
   private let contract: BenchmarkInlineTextAtlas
   private let images: [UInt32: [String: CGImage]]

   init(contract: BenchmarkInlineTextAtlas, rasterImages: [UInt32: CGImage]) throws
   {
      var images = [UInt32: [String: CGImage]]()
      for variant in contract.variants
      {
         guard let raster = rasterImages[variant.emPixels] else
         {
            throw AppKitProductionScenarioFailure.invalidFixture
         }
         var tiles = [String: CGImage]()
         tiles.reserveCapacity(contract.entries.count)
         for entry in contract.entries
         {
            let source = CGRect(
               x: Int(entry.column * variant.emPixels),
               y: Int(entry.row * variant.emPixels),
               width: Int(variant.emPixels),
               height: Int(variant.emPixels)
            )
            guard let tile = AppKitProductionInlineImageCache.shared.image(atlas: raster, source: source) else
            {
               throw AppKitProductionScenarioFailure.invalidFixture
            }
            tiles[entry.grapheme] = tile
         }
         images[variant.emPixels] = tiles
      }
      self.contract = contract
      self.images = images
   }

   func pieces(_ value: String, font: NSFont) -> [AppKitProductionInlinePiece]?
   {
      guard contract.entries.contains(where: {value.contains($0.grapheme)}) else {return nil}
      let targetPixels = UInt32((font.pointSize * 3).rounded())
      guard let variant = contract.variants.min(by: {
         abs(Int64($0.emPixels) - Int64(targetPixels)) < abs(Int64($1.emPixels) - Int64(targetPixels))
      }), let tiles = images[variant.emPixels] else {return nil}
      var result = [AppKitProductionInlinePiece]()
      var remaining = value[...]
      while let match = nextMatch(in: remaining)
      {
         let plain = String(remaining[..<match.range.lowerBound])
         if !plain.isEmpty
         {
            result.append(AppKitProductionInlinePiece(text: plain, image: nil, advance: 0, width: 0, height: 0, topFromBaseline: 0))
         }
         guard let image = tiles[match.entry.grapheme] else {return nil}
         let unit = font.pointSize / 1_000_000
         result.append(AppKitProductionInlinePiece(
            text: "",
            image: image,
            advance: CGFloat(match.entry.advanceMillionths) * unit,
            width: CGFloat(match.entry.widthMillionths) * unit,
            height: CGFloat(match.entry.heightMillionths) * unit,
            topFromBaseline: CGFloat(match.entry.topFromBaselineMillionths) * unit
         ))
         remaining = remaining[match.range.upperBound...]
      }
      if !remaining.isEmpty
      {
         result.append(AppKitProductionInlinePiece(text: String(remaining), image: nil, advance: 0, width: 0, height: 0, topFromBaseline: 0))
      }
      return result
   }

   func measure(_ value: String, font: NSFont) -> CGFloat
   {
      guard let pieces = pieces(value, font: font) else {return textWidth(value, font: font)}
      return pieces.reduce(0)
      {
         width, piece in
         width + (piece.image == nil ? textWidth(piece.text, font: font) : piece.advance)
      }
   }

   private func textWidth(_ value: String, font: NSFont) -> CGFloat
   {
      let line = CTLineCreateWithAttributedString(NSAttributedString(
         string: value,
         attributes: [kCTFontAttributeName as NSAttributedString.Key: font]
      ))
      return CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil))
   }

   private func nextMatch(in value: Substring) -> (range: Range<Substring.Index>, entry: BenchmarkInlineTextAsset)?
   {
      var selected: (range: Range<Substring.Index>, entry: BenchmarkInlineTextAsset)?
      for entry in contract.entries
      {
         guard let range = value.range(of: entry.grapheme) else {continue}
         if selected == nil || range.lowerBound < selected!.range.lowerBound
         {
            selected = (range, entry)
         }
      }
      return selected
   }
}

class AppKitProductionExactTextField: NSTextField
{
   private struct PreparedRun
   {
      let font: CTFont
      let glyphs: [CGGlyph]
      let positions: [CGPoint]
   }

   private struct PreparedInlineImage
   {
      let image: CGImage
      let x: CGFloat
      let width: CGFloat
      let height: CGFloat
      let topFromBaseline: CGFloat
   }

   private var exactColor = AppKitProductionPalette.text
   private var inlineImageYOffset: CGFloat = 0
   private var inlineText: AppKitProductionInlineText?
   private var preparedInlineImages = [PreparedInlineImage]()
   private var preparedRuns = [PreparedRun]()
   private var preparedValue = ""
   private var preparedWidth: CGFloat = 0
   private(set) var inlineImageCount = 0
   private(set) var shapePreparationCount: UInt64 = 0

   override var isFlipped: Bool {true}

   override init(frame frameRect: NSRect)
   {
      super.init(frame: frameRect)
      isBordered = false
      isBezeled = false
      isEditable = false
      isSelectable = false
      drawsBackground = false
      focusRingType = .none
      lineBreakMode = .byClipping
      setAccessibilityRole(.staticText)
   }

   required init?(coder: NSCoder)
   {
      fatalError("AppKitProductionExactTextField does not support coder initialization")
   }

   func configure(identifier: String, value: String, font: NSFont, color: NSColor, inlineText: AppKitProductionInlineText? = nil, inlineImageYOffset: CGFloat = 0, nativeValue: String? = nil)
   {
      self.identifier = NSUserInterfaceItemIdentifier(identifier)
      setAccessibilityIdentifier(identifier)
      setAccessibilityLabel(value)
      let nativeValue = nativeValue ?? value
      if stringValue != nativeValue || preparedValue != value || self.font !== font || self.inlineText !== inlineText
      {
         stringValue = nativeValue
         self.font = font
         self.inlineText = inlineText
         prepareContent(value: value, font: font)
         preparedValue = value
      }
      exactColor = color
      self.inlineImageYOffset = inlineImageYOffset
      needsDisplay = true
   }

   func clearPreparedText()
   {
      inlineText = nil
      preparedInlineImages.removeAll(keepingCapacity: false)
      preparedRuns.removeAll(keepingCapacity: false)
      preparedValue = ""
      preparedWidth = 0
      inlineImageCount = 0
   }

   override func draw(_ dirtyRect: NSRect)
   {
      guard let context = NSGraphicsContext.current?.cgContext, let font else {return}
      let baseline = resolvedBaseline(font: font)
      let horizontalOffset: CGFloat
      if alignment == .center
      {
         horizontalOffset = (bounds.width - preparedWidth) * 0.5
      }
      else if alignment == .right
      {
         horizontalOffset = bounds.width - preparedWidth
      }
      else
      {
         horizontalOffset = 0
      }
      context.saveGState()
      context.clip(to: bounds)
      context.setFillColor(exactColor.cgColor)
      context.setTextDrawingMode(.fill)
      context.textMatrix = CGAffineTransform(a: 1, b: 0, c: 0, d: -1, tx: horizontalOffset, ty: baseline)
      for run in preparedRuns
      {
         run.glyphs.withUnsafeBufferPointer
         {
            glyphs in
            run.positions.withUnsafeBufferPointer
            {
               positions in
               guard let glyphBase = glyphs.baseAddress, let positionBase = positions.baseAddress else {return}
               CTFontDrawGlyphs(run.font, glyphBase, positionBase, glyphs.count, context)
            }
         }
      }
      for image in preparedInlineImages
      {
         let imageX = ((image.x + horizontalOffset) * 3).rounded() / 3
         let imageY = ((baseline + image.topFromBaseline) * 3).rounded() / 3 + inlineImageYOffset
         context.saveGState()
         context.setBlendMode(.normal)
         context.interpolationQuality = .none
         context.translateBy(x: imageX, y: imageY + image.height)
         context.scaleBy(x: 1, y: -1)
         context.draw(image.image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
         context.restoreGState()
      }
      context.restoreGState()
   }

   func resolvedBaseline() -> CGFloat
   {
      guard let font else {return 0}
      return resolvedBaseline(font: font)
   }

   var renderedTextLineBounds: CGRect
   {
      CGRect(x: bounds.minX, y: resolvedBaseline(), width: bounds.width, height: bounds.height)
   }

   private func prepareContent(value: String, font: NSFont)
   {
      var runs = [PreparedRun]()
      var images = [PreparedInlineImage]()
      var cursor: CGFloat = 0
      if let pieces = inlineText?.pieces(value, font: font)
      {
         for piece in pieces
         {
            if let image = piece.image
            {
               images.append(PreparedInlineImage(
                  image: image,
                  x: cursor,
                  width: piece.width,
                  height: piece.height,
                  topFromBaseline: piece.topFromBaseline
               ))
               cursor += piece.advance
            }
            else
            {
               cursor += appendGlyphRuns(value: piece.text, font: font, originX: cursor, to: &runs)
            }
         }
      }
      else
      {
         cursor = appendGlyphRuns(value: value, font: font, originX: 0, to: &runs)
      }
      preparedRuns = runs
      preparedInlineImages = images
      preparedWidth = cursor
      inlineImageCount = images.count
      shapePreparationCount += 1
   }

   private func resolvedBaseline(font: NSFont) -> CGFloat
   {
      let lineHeight = font.ascender - font.descender + font.leading
      let local = bounds.minY + (bounds.height - lineHeight) / 2 + font.ascender
      let windowY = convert(CGPoint(x: 0, y: local), to: nil).y
      let nextWindowY = convert(CGPoint(x: 0, y: local + 1), to: nil).y
      let direction = nextWindowY - windowY
      guard abs(direction) > 0.5 else {return local}
      return local + ((windowY * 3).rounded() / 3 - windowY) / direction
   }

   private func appendGlyphRuns(value: String, font: NSFont, originX: CGFloat, to runs: inout [PreparedRun]) -> CGFloat
   {
      let line = CTLineCreateWithAttributedString(NSAttributedString(
         string: value,
         attributes: [kCTFontAttributeName as NSAttributedString.Key: font]
      ))
      runs.reserveCapacity(runs.count + CFArrayGetCount(CTLineGetGlyphRuns(line)))
      for case let run as CTRun in CTLineGetGlyphRuns(line) as NSArray
      {
         let count = CTRunGetGlyphCount(run)
         guard count > 0,
               let fontAttribute = (CTRunGetAttributes(run) as NSDictionary)[kCTFontAttributeName] else {continue}
         let runFont = fontAttribute as! CTFont
         var glyphs = [CGGlyph](repeating: 0, count: count)
         var positions = [CGPoint](repeating: .zero, count: count)
         CTRunGetGlyphs(run, CFRange(location: 0, length: 0), &glyphs)
         CTRunGetPositions(run, CFRange(location: 0, length: 0), &positions)
         for index in positions.indices
         {
            positions[index].x += originX
         }
         runs.append(PreparedRun(font: runFont, glyphs: glyphs, positions: positions))
      }
      return CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil))
   }
}

final class AppKitProductionExactImageView: NSImageView
{
   override var isFlipped: Bool {true}

   override func draw(_ dirtyRect: NSRect)
   {
      guard let image else {return}
      image.draw(
         in: bounds,
         from: .zero,
         operation: .sourceOver,
         fraction: 1,
         respectFlipped: true,
         hints: [.interpolation: NSImageInterpolation.low]
      )
   }
}

private final class AppKitProductionHalfImageCache
{
   static let shared = AppKitProductionHalfImageCache()

   private var images = [Data: CGImage]()
   private let lock = NSLock()

   func image(for image: CGImage) -> CGImage?
   {
      guard image.width == 4_096,
            image.height == 3_072,
            image.bitsPerComponent == 8,
            image.bitsPerPixel == 32,
            image.bytesPerRow == image.width * 4,
            image.alphaInfo == .noneSkipLast,
            image.colorSpace?.name == CGColorSpace.sRGB,
            let source = image.dataProvider?.data as Data?,
            source.count == image.width * image.height * 4 else {return nil}
      let key = Data(SHA256.hash(data: source))
      lock.lock()
      defer {lock.unlock()}
      if let image = images[key] {return image}
      let width = image.width / 2
      let height = image.height / 2
      var output = [UInt32](repeating: 0, count: width * height)
      source.withUnsafeBytes
      {
         sourceBytes in
         output.withUnsafeMutableBytes
         {
            outputBytes in
            let sourcePixels = sourceBytes.bindMemory(to: UInt32.self)
            let outputPixels = outputBytes.bindMemory(to: UInt32.self)
            for y in 0..<height
            {
               let sourceRow = y * 2 * image.width
               let outputRow = y * width
               for x in 0..<width
               {
                  outputPixels[outputRow + x] = sourcePixels[sourceRow + x * 2]
               }
            }
         }
      }
      guard let provider = CGDataProvider(data: Data(bytes: output, count: output.count * MemoryLayout<UInt32>.size) as CFData),
            let colorSpace = image.colorSpace,
            let halfImage = CGImage(
               width: width,
               height: height,
               bitsPerComponent: 8,
               bitsPerPixel: 32,
               bytesPerRow: width * 4,
               space: colorSpace,
               bitmapInfo: image.bitmapInfo,
               provider: provider,
               decode: nil,
               shouldInterpolate: false,
               intent: .defaultIntent
            ) else {return nil}
      images[key] = halfImage
      return halfImage
   }
}

final class AppKitProductionImageCanvas: NSImageView
{
   private var decodedImage: CGImage?
   private var halfImage: CGImage?
   private var imageIdentity: ObjectIdentifier?
   private var preparedDecodedImage: CGImage?
   private var preparedHalfImage: CGImage?
   private var preparedImageIdentity: ObjectIdentifier?
   private var preparedTexture: MTLTexture?
   private var imageScale: CGFloat = 1
   private var imageTranslation = CGPoint.zero
   private(set) var uploadPreparationCount = 0
   private(set) var presentationPreparationCount = 0
   private(set) var uploadedTextureBytes = 0

   var renderedImageBounds: CGRect?
   {
      guard let decodedImage else {return nil}
      if decodedImage.width <= 384
      {
         return CGRect(x: 16, y: 16, width: 128, height: 96)
      }
      let destinationWidth = CGFloat(decodedImage.width) / 6 * imageScale
      let destinationHeight = CGFloat(decodedImage.height) / 6 * imageScale
      let rawOriginX = (bounds.width - destinationWidth) / 2 + imageTranslation.x
      let rawOriginY = (bounds.height - destinationHeight) / 2 + imageTranslation.y * bounds.height / 844
      let samplingCorrectionX: CGFloat = imageTranslation == .zero ? 0 : 1 / 6
      return CGRect(
         x: exactSamplingGridOrigin(rawOriginX) + samplingCorrectionX,
         y: exactSamplingGridOrigin(rawOriginY),
         width: destinationWidth,
         height: destinationHeight
      )
   }

   override var isFlipped: Bool {true}

   func prepareUpload(image: NSImage) -> Bool
   {
      let identity = ObjectIdentifier(image)
      if identity == preparedImageIdentity {return true}
      guard let decoded = image.cgImage(forProposedRect: nil, context: nil, hints: nil),
            let downsampled = AppKitProductionHalfImageCache.shared.image(for: decoded),
            let data = downsampled.dataProvider?.data,
            let bytes = CFDataGetBytePtr(data),
            let device = MTLCreateSystemDefaultDevice() else {return false}
      let descriptor = MTLTextureDescriptor.texture2DDescriptor(
         pixelFormat: .rgba8Unorm_srgb,
         width: downsampled.width,
         height: downsampled.height,
         mipmapped: false
      )
      descriptor.storageMode = .shared
      descriptor.usage = [.shaderRead]
      guard let texture = device.makeTexture(descriptor: descriptor) else {return false}
      texture.replace(
         region: MTLRegionMake2D(0, 0, downsampled.width, downsampled.height),
         mipmapLevel: 0,
         withBytes: bytes,
         bytesPerRow: downsampled.bytesPerRow
      )
      preparedDecodedImage = decoded
      preparedHalfImage = downsampled
      preparedImageIdentity = identity
      preparedTexture = texture
      uploadedTextureBytes = downsampled.bytesPerRow * downsampled.height
      uploadPreparationCount += 1
      return true
   }

   func present(image: NSImage?, translation: CGPoint, scale: CGFloat)
   {
      let nextIdentity = image.map(ObjectIdentifier.init)
      if nextIdentity != imageIdentity
      {
         if nextIdentity == preparedImageIdentity
         {
            decodedImage = preparedDecodedImage
            halfImage = preparedHalfImage
         }
         else
         {
            decodedImage = image?.cgImage(forProposedRect: nil, context: nil, hints: nil)
            halfImage = decodedImage.flatMap {AppKitProductionHalfImageCache.shared.image(for: $0)}
            if image != nil {presentationPreparationCount += 1}
         }
         imageIdentity = nextIdentity
         self.image = image
      }
      imageTranslation = translation
      imageScale = scale
      needsDisplay = true
   }

   override func draw(_ dirtyRect: NSRect)
   {
      AppKitProductionPalette.background.setFill()
      bounds.fill()
      guard let decodedImage,
            let context = NSGraphicsContext.current?.cgContext else {return}
      guard let destination = renderedImageBounds else {return}
      if decodedImage.width <= 384
      {
         draw(decodedImage, in: destination, context: context)
         return
      }
      let presentedImage = imageScale == 1 ? halfImage ?? decodedImage : decodedImage
      draw(presentedImage, in: destination, context: context)
      AppKitProductionPalette.background.setFill()
      let borderHeight: CGFloat = 1 / 3
      CGRect(x: bounds.minX, y: destination.minY, width: bounds.width, height: borderHeight).fill()
      CGRect(x: bounds.minX, y: (destination.maxY * 3).rounded(.down) / 3, width: bounds.width, height: borderHeight).fill()
   }

   private func draw(_ image: CGImage, in rect: CGRect, context: CGContext)
   {
      context.saveGState()
      context.clip(to: bounds)
      context.setBlendMode(.copy)
      context.interpolationQuality = .none
      context.translateBy(x: rect.minX, y: rect.maxY)
      context.scaleBy(x: 1, y: -1)
      context.draw(image, in: CGRect(x: 0, y: 0, width: rect.width, height: rect.height))
      context.restoreGState()
   }

   private func exactSamplingGridOrigin(_ value: CGFloat) -> CGFloat
   {
      let phase = max(0, min(1, 2 - imageScale)) * 0.25
      let snapped = ((value * 3 - phase).rounded(.up) + phase) / 3
      return (snapped * 3).rounded(.down) / 3
   }

}

final class AppKitProductionZoomSlider: NSSlider
{
   override var isFlipped: Bool {true}

   override func mouseDown(with event: NSEvent)
   {
      updateValue(with: event)
   }

   override func mouseDragged(with event: NSEvent)
   {
      updateValue(with: event)
   }

   override func mouseUp(with event: NSEvent)
   {
      updateValue(with: event)
   }

   override func draw(_ dirtyRect: NSRect)
   {
      NSColor(srgbRed: 231 / 255, green: 233 / 255, blue: 234 / 255, alpha: 1).setFill()
      CGRect(x: 0, y: 11, width: bounds.width, height: 6).fill()
   }

   private func updateValue(with event: NSEvent)
   {
      guard bounds.width > 0 else {return}
      let x = convert(event.locationInWindow, from: nil).x
      let fraction = Double(max(0, min(1, x / bounds.width)))
      let value = minValue + fraction * (maxValue - minValue)
      guard value != doubleValue else {return}
      doubleValue = value
      if let action {_ = NSApp.sendAction(action, to: target, from: self)}
   }
}

final class AppKitProductionRoundedImageView: NSImageView
{
   private var analyticRaster: AppKitProductionRoundedImageRaster?
   private var atlas: NSImage?
   private var tileIndex: Int?
   private var roundedRadius = 8
   private var roundedUnderlay = AppKitProductionRoundedImageUnderlay.surface

   override var isFlipped: Bool {true}

   func configure(image: NSImage, atlas: NSImage, tileIndex: Int, radius: Int = 8, underlay: AppKitProductionRoundedImageUnderlay = .surface)
   {
      self.image = image
      self.atlas = atlas
      self.tileIndex = tileIndex
      roundedRadius = radius
      roundedUnderlay = underlay
      analyticRaster = nil
      needsDisplay = true
   }

   func prepareAnalyticRaster()
   {
      guard let window, let contentView = window.contentView, let atlas, let tileIndex else {return}
      let windowOrigin = convert(CGPoint.zero, to: nil)
      let topDownOrigin = CGPoint(
         x: windowOrigin.x,
         y: contentView.bounds.height - windowOrigin.y - bounds.height
      )
      analyticRaster = AppKitProductionRoundedImageRasterCache.shared.raster(
         atlas: atlas,
         tileIndex: tileIndex,
         origin: topDownOrigin,
         pointSize: Int(bounds.width.rounded()),
         radius: roundedRadius,
         underlay: roundedUnderlay
      )
   }

   override func draw(_ dirtyRect: NSRect)
   {
      guard let image else {return}
      if let analyticRaster
      {
         analyticRaster.draw(in: bounds)
         return
      }
      NSGraphicsContext.current?.saveGraphicsState()
      NSBezierPath(roundedRect: bounds, xRadius: CGFloat(roundedRadius), yRadius: CGFloat(roundedRadius)).addClip()
      image.draw(
         in: bounds,
         from: .zero,
         operation: .sourceOver,
         fraction: 1,
         respectFlipped: true,
         hints: [.interpolation: NSImageInterpolation.low]
      )
      NSGraphicsContext.current?.restoreGraphicsState()
   }
}

final class AppKitProductionCircularControl: NSButton
{
   private var analyticImages: AppKitProductionCircularControlImages?

   override var isFlipped: Bool {true}

   override init(frame frameRect: NSRect)
   {
      super.init(frame: frameRect)
      title = ""
      isBordered = false
      focusRingType = .none
      setButtonType(.toggle)
      setAccessibilityRole(.button)
      setAccessibilityLabel("Favorite")
   }

   required init?(coder: NSCoder)
   {
      fatalError("AppKitProductionCircularControl does not support coder initialization")
   }

   override func draw(_ dirtyRect: NSRect)
   {
      if let image = state == .on ? analyticImages?.accent : analyticImages?.inactive
      {
         image.draw(
            in: bounds,
            from: .zero,
            operation: .copy,
            fraction: 1,
            respectFlipped: true,
            hints: [.interpolation: NSImageInterpolation.none]
         )
         return
      }
      (state == .on ? AppKitProductionPalette.accent : AppKitProductionPalette.inactiveControl).setFill()
      NSBezierPath(ovalIn: bounds).fill()
   }

   func prepareAnalyticImages()
   {
      guard window != nil else {return}
      analyticImages = AppKitProductionCircularControlRasterCache.shared.images(origin: convert(.zero, to: nil))
   }
}

final class AppKitProductionSolidButton: NSButton
{
   override var isFlipped: Bool {true}

   override init(frame frameRect: NSRect)
   {
      super.init(frame: frameRect)
      title = ""
      isBordered = false
      isTransparent = false
      focusRingType = .none
      setAccessibilityRole(.button)
      setAccessibilityLabel("control")
   }

   required init?(coder: NSCoder)
   {
      fatalError("AppKitProductionSolidButton does not support coder initialization")
   }

   override func draw(_ dirtyRect: NSRect)
   {
      AppKitProductionPalette.accent.setFill()
      bounds.fill()
   }
}
