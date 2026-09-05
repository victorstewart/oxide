import AppKit
import Foundation
import Metal

private enum AppKitProductionEdgeCorner: UInt8, CaseIterable
{
   case topLeft
   case topRight
   case bottomRight
   case bottomLeft
}

private struct AppKitProductionEdgeKey: Hashable
{
   let widthParity: UInt8
   let heightParity: UInt8
   let originXParity: UInt8
   let originYParity: UInt8
   let materialRowStart: UInt8
   let materialRowEnd: UInt8
   let corner: AppKitProductionEdgeCorner
}

enum AppKitProductionEdgeUnderlay
{
   case plain
   case dashboardBackdrop
}

private enum AppKitProductionAnalyticCoverage
{
   static func circular(radius: Float, rect: AppKitProductionPhysicalRect, x: Int, y: Int) -> Float
   {
      let sampleX = Float(x) + 0.5
      let sampleY = Float(y) + 0.5
      let signedDistance = distance(rect: rect, radius: radius, x: sampleX, y: sampleY)
      let quadX = Float(x & ~1)
      let quadY = Float(y & ~1)
      let dx = distance(rect: rect, radius: radius, x: quadX + 1.5, y: sampleY)
         - distance(rect: rect, radius: radius, x: quadX + 0.5, y: sampleY)
      let dy = distance(rect: rect, radius: radius, x: sampleX, y: quadY + 1.5)
         - distance(rect: rect, radius: radius, x: sampleX, y: quadY + 0.5)
      return min(1, max(0, 0.5 - signedDistance / max(0.0001, abs(dx) + abs(dy))))
   }

   private static func distance(rect: AppKitProductionPhysicalRect, radius: Float, x: Float, y: Float) -> Float
   {
      let centerX = Float(rect.x) + Float(rect.width) * 0.5
      let centerY = Float(rect.y) + Float(rect.height) * 0.5
      let qX = abs(x - centerX) - (Float(rect.width) * 0.5 - radius)
      let qY = abs(y - centerY) - (Float(rect.height) * 0.5 - radius)
      let outsideX = max(qX, 0)
      let outsideY = max(qY, 0)
      return sqrt(outsideX * outsideX + outsideY * outsideY) + min(max(qX, qY), 0) - radius
   }
}

struct AppKitProductionShadowedSurfaceEdges
{
   let topLeft: NSImage
   let topRight: NSImage
   let bottomRight: NSImage
   let bottomLeft: NSImage

   func draw(in card: CGRect)
   {
      draw(topLeft, in: CGRect(x: card.minX, y: card.minY, width: 12, height: 12))
      draw(topRight, in: CGRect(x: card.maxX - 12, y: card.minY, width: 12, height: 12))
      draw(bottomRight, in: CGRect(x: card.maxX - 12, y: card.maxY - 12, width: 12, height: 14))
      draw(bottomLeft, in: CGRect(x: card.minX, y: card.maxY - 12, width: 12, height: 14))
   }

   private func draw(_ image: NSImage, in rect: CGRect)
   {
      image.draw(
         in: rect,
         from: .zero,
         operation: .copy,
         fraction: 1,
         respectFlipped: true,
         hints: [.interpolation: NSImageInterpolation.none]
      )
   }
}

final class AppKitProductionEdgeRasterCache
{
   static let shared = AppKitProductionEdgeRasterCache()

   private static let scale = 3
   private static let shadowOffset = 2 * scale
   private static let background: [Float] = [0.896269353, 0.913098652, 0.938685728]
   private static let shadow: [Float] = [0.014443844, 0.017641954, 0.025186860, 0.160784314]
   private static let surface: [Float] = [1, 1, 1]
   private var images = [AppKitProductionEdgeKey: NSImage]()
   private let lock = NSLock()

   private(set) var imageCreationCount = 0

   func shadowedSurfaceEdges(card: CGRect, in view: NSView, underlay: AppKitProductionEdgeUnderlay = .plain) -> AppKitProductionShadowedSurfaceEdges?
   {
      let windowOrigin = view.convert(card.origin, to: nil)
      guard let contentView = view.window?.contentView else {return nil}
      let width = Int((card.width * CGFloat(Self.scale)).rounded())
      let height = Int((card.height * CGFloat(Self.scale)).rounded())
      let originX = Int((windowOrigin.x * CGFloat(Self.scale)).rounded())
      let originY = Int((windowOrigin.y * CGFloat(Self.scale)).rounded())
      let underlayOriginY = Int(((contentView.bounds.height - windowOrigin.y) * CGFloat(Self.scale)).rounded())
      guard width > 24 * Self.scale, height > 24 * Self.scale else {return nil}
      let key: (AppKitProductionEdgeCorner) -> AppKitProductionEdgeKey =
      {
         corner in
         let materialRows = Self.materialRowSpan(
            underlay,
            corner: corner,
            cardOriginY: underlayOriginY,
            cardHeight: height
         )
         return AppKitProductionEdgeKey(
            widthParity: UInt8(width & 1),
            heightParity: UInt8(height & 1),
            originXParity: UInt8(originX & 1),
            originYParity: UInt8(originY & 1),
            materialRowStart: materialRows.start,
            materialRowEnd: materialRows.end,
            corner: corner
         )
      }
      guard let topLeft = image(key(.topLeft), width: width, height: height),
            let topRight = image(key(.topRight), width: width, height: height),
            let bottomRight = image(key(.bottomRight), width: width, height: height),
            let bottomLeft = image(key(.bottomLeft), width: width, height: height) else {return nil}
      return AppKitProductionShadowedSurfaceEdges(topLeft: topLeft, topRight: topRight, bottomRight: bottomRight, bottomLeft: bottomLeft)
   }

   private func image(_ key: AppKitProductionEdgeKey, width: Int, height: Int) -> NSImage?
   {
      lock.lock()
      defer {lock.unlock()}
      if let image = images[key] {return image}
      guard let image = makeImage(key: key, width: width, height: height) else {return nil}
      images[key] = image
      imageCreationCount += 1
      return image
   }

   private func makeImage(key: AppKitProductionEdgeKey, width: Int, height: Int) -> NSImage?
   {
      let bottom = key.corner == .bottomLeft || key.corner == .bottomRight
      let right = key.corner == .topRight || key.corner == .bottomRight
      let radius = 12 * Self.scale
      let patchWidth = radius
      let patchHeight = radius + (bottom ? Self.shadowOffset : 0)
      let originX = Int(key.originXParity)
      let originY = Int(key.originYParity)
      let patchX = right ? originX + width - radius : originX
      let patchY = bottom ? originY + height - radius : originY
      let surface = AppKitProductionPhysicalRect(x: originX, y: originY, width: width, height: height)
      let shadow = AppKitProductionPhysicalRect(
         x: originX,
         y: originY + Self.shadowOffset,
         width: width,
         height: height
      )
      var pixels = [Float](repeating: 1, count: patchWidth * patchHeight * 4)
      for y in 0..<patchHeight
      {
         for x in 0..<patchWidth
         {
            let globalX = patchX + x
            let globalY = patchY + y
            let shadowCoverage = AppKitProductionAnalyticCoverage.circular(radius: Float(radius), rect: shadow, x: globalX, y: globalY)
            let surfaceCoverage = AppKitProductionAnalyticCoverage.circular(radius: Float(radius), rect: surface, x: globalX, y: globalY)
            let underlay = Self.underlay(
               row: y,
               materialRowStart: Int(key.materialRowStart),
               materialRowEnd: Int(key.materialRowEnd)
            )
            let color = Self.compose(underlay: underlay, shadowCoverage: shadowCoverage, surfaceCoverage: surfaceCoverage)
            let offset = (y * patchWidth + x) * 4
            pixels[offset] = color[0]
            pixels[offset + 1] = color[1]
            pixels[offset + 2] = color[2]
         }
      }
      guard let provider = CGDataProvider(data: Data(bytes: pixels, count: pixels.count * MemoryLayout<Float>.size) as CFData),
            let colorSpace = CGColorSpace(name: CGColorSpace.linearSRGB),
            let image = CGImage(
               width: patchWidth,
               height: patchHeight,
               bitsPerComponent: 32,
               bitsPerPixel: 128,
               bytesPerRow: patchWidth * 16,
               space: colorSpace,
               bitmapInfo: CGBitmapInfo.floatComponents.union(.byteOrder32Little).union(CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue)),
               provider: provider,
               decode: nil,
               shouldInterpolate: false,
               intent: .defaultIntent
            ) else {return nil}
      return NSImage(
         cgImage: image,
         size: NSSize(
            width: CGFloat(radius) / CGFloat(Self.scale),
            height: CGFloat(patchHeight) / CGFloat(Self.scale)
         )
      )
   }

   private static func materialRowSpan(_ underlay: AppKitProductionEdgeUnderlay, corner: AppKitProductionEdgeCorner, cardOriginY: Int, cardHeight: Int) -> (start: UInt8, end: UInt8)
   {
      guard underlay == .dashboardBackdrop else {return (0, 0)}
      let bottom = corner == .bottomLeft || corner == .bottomRight
      let radius = 12 * scale
      let patchHeight = radius + (bottom ? shadowOffset : 0)
      let patchOriginY = cardOriginY + (bottom ? cardHeight - radius : 0)
      for index in 0..<4
      {
         let backdrop = (192 + index * 612)..<(192 + index * 612 + 288)
         let start = max(patchOriginY, backdrop.lowerBound)
         let end = min(patchOriginY + patchHeight, backdrop.upperBound)
         if start < end
         {
            return (UInt8(start - patchOriginY), UInt8(end - patchOriginY))
         }
      }
      return (0, 0)
   }

   private static func underlay(row: Int, materialRowStart: Int, materialRowEnd: Int) -> [Float]
   {
      return row >= materialRowStart && row < materialRowEnd
         ? [srgb8ToLinear(239), srgb8ToLinear(241), srgb8ToLinear(246)]
         : background
   }

   private static func compose(underlay sourceUnderlay: [Float], shadowCoverage: Float, surfaceCoverage: Float) -> [Float]
   {
      let shadowAlpha = shadow[3] * shadowCoverage
      var underlay = [Float](repeating: 0, count: 3)
      for channel in 0..<3
      {
         let shadowed = shadow[channel] * shadowAlpha + sourceUnderlay[channel] * (1 - shadowAlpha)
         underlay[channel] = surfaceCoverage > 0 ? srgb8ToLinear(linearToSRGB8(shadowed)) : shadowed
      }
      return (0..<3).map
      {
         channel in
         let composed = surface[channel] * surfaceCoverage + underlay[channel] * (1 - surfaceCoverage)
         return srgb8ToLinear(linearToSRGB8(composed))
      }
   }

   private static func srgb8ToLinear(_ value: UInt8) -> Float
   {
      let encoded = Double(value) / 255
      return Float(encoded <= 0.04045 ? encoded / 12.92 : pow((encoded + 0.055) / 1.055, 2.4))
   }

   private static func linearToSRGB8(_ value: Float) -> UInt8
   {
      let linear = Double(min(1, max(0, value)))
      let encoded = linear <= 0.0031308 ? linear * 12.92 : 1.055 * pow(linear, 1 / 2.4) - 0.055
      return UInt8(min(255, max(0, Int((encoded * 255).rounded(.toNearestOrEven)))))
   }
}

private struct AppKitProductionNavigationModalEdgeKey: Hashable
{
   let originXParity: UInt8
   let originYParity: UInt8
   let progressBits: UInt32
   let corner: AppKitProductionEdgeCorner
}

private struct AppKitProductionNavigationDismissEdgeKey: Hashable
{
   let originXParity: UInt8
   let originYParity: UInt8
   let corner: AppKitProductionEdgeCorner
}

struct AppKitProductionNavigationModalEdges
{
   let topLeft: NSImage
   let topRight: NSImage
   let bottomRight: NSImage
   let bottomLeft: NSImage

   func draw(in card: CGRect)
   {
      draw(topLeft, in: CGRect(x: card.minX, y: card.minY, width: 16, height: 16))
      draw(topRight, in: CGRect(x: card.maxX - 16, y: card.minY, width: 16, height: 16))
      draw(bottomRight, in: CGRect(x: card.maxX - 16, y: card.maxY - 16, width: 16, height: 18))
      draw(bottomLeft, in: CGRect(x: card.minX, y: card.maxY - 16, width: 16, height: 18))
   }

   private func draw(_ image: NSImage, in rect: CGRect)
   {
      image.draw(
         in: rect,
         from: .zero,
         operation: .copy,
         fraction: 1,
         respectFlipped: true,
         hints: [.interpolation: NSImageInterpolation.none]
      )
   }
}

struct AppKitProductionNavigationDismissEdges
{
   let topLeft: NSImage
   let topRight: NSImage
   let bottomRight: NSImage
   let bottomLeft: NSImage

   func draw(in control: CGRect)
   {
      draw(topLeft, in: CGRect(x: control.minX, y: control.minY, width: 10, height: 10))
      draw(topRight, in: CGRect(x: control.maxX - 10, y: control.minY, width: 10, height: 10))
      draw(bottomRight, in: CGRect(x: control.maxX - 10, y: control.maxY - 10, width: 10, height: 10))
      draw(bottomLeft, in: CGRect(x: control.minX, y: control.maxY - 10, width: 10, height: 10))
   }

   private func draw(_ image: NSImage, in rect: CGRect)
   {
      image.draw(
         in: rect,
         from: .zero,
         operation: .copy,
         fraction: 1,
         respectFlipped: true,
         hints: [.interpolation: NSImageInterpolation.none]
      )
   }
}

final class AppKitProductionNavigationEdgeRasterCache
{
   static let shared = AppKitProductionNavigationEdgeRasterCache()

   private static let scale = 3
   private static let modalWidth = 342 * scale
   private static let modalHeight = 580 * scale
   private static let modalRadius = 16 * scale
   private static let shadowOffset = 2 * scale
   private static let dismissWidth = 50 * scale
   private static let dismissHeight = 28 * scale
   private static let dismissRadius = 10 * scale
   private static let background: [Float] = [0.896269353, 0.913098652, 0.938685728]
   private static let overlay: [Float] = [0.009721217, 0.011612245, 0.016807376]
   private static let shadow: [Float] = [0.014443844, 0.017641954, 0.025186860]
   private static let shadowAlpha: Float = 0.160784314
   private static let surface: [Float] = [1, 1, 1]
   private static let accent: [Float] = [0.046665086, 0.155926464, 0.863157213]

   private var modalImages = [AppKitProductionNavigationModalEdgeKey: NSImage]()
   private var dismissImages = [AppKitProductionNavigationDismissEdgeKey: NSImage]()
   private let lock = NSLock()

   private(set) var modalImageCreationCount = 0
   private(set) var dismissImageCreationCount = 0

   func modalEdges(card: CGRect, in view: NSView, progress: CGFloat) -> AppKitProductionNavigationModalEdges?
   {
      let windowOrigin = view.convert(card.origin, to: nil)
      let originX = Int((windowOrigin.x * CGFloat(Self.scale)).rounded())
      let originY = Int((windowOrigin.y * CGFloat(Self.scale)).rounded())
      let progressBits = Float(min(1, max(0, progress))).bitPattern
      let key: (AppKitProductionEdgeCorner) -> AppKitProductionNavigationModalEdgeKey =
      {
         corner in
         AppKitProductionNavigationModalEdgeKey(
            originXParity: UInt8(originX & 1),
            originYParity: UInt8(originY & 1),
            progressBits: progressBits,
            corner: corner
         )
      }
      guard let topLeft = modalImage(key(.topLeft)),
            let topRight = modalImage(key(.topRight)),
            let bottomRight = modalImage(key(.bottomRight)),
            let bottomLeft = modalImage(key(.bottomLeft)) else {return nil}
      return AppKitProductionNavigationModalEdges(
         topLeft: topLeft,
         topRight: topRight,
         bottomRight: bottomRight,
         bottomLeft: bottomLeft
      )
   }

   func dismissEdges(control: CGRect, in view: NSView) -> AppKitProductionNavigationDismissEdges?
   {
      let windowOrigin = view.convert(control.origin, to: nil)
      let originX = Int((windowOrigin.x * CGFloat(Self.scale)).rounded())
      let originY = Int((windowOrigin.y * CGFloat(Self.scale)).rounded())
      let key: (AppKitProductionEdgeCorner) -> AppKitProductionNavigationDismissEdgeKey =
      {
         corner in
         AppKitProductionNavigationDismissEdgeKey(
            originXParity: UInt8(originX & 1),
            originYParity: UInt8(originY & 1),
            corner: corner
         )
      }
      guard let topLeft = dismissImage(key(.topLeft)),
            let topRight = dismissImage(key(.topRight)),
            let bottomRight = dismissImage(key(.bottomRight)),
            let bottomLeft = dismissImage(key(.bottomLeft)) else {return nil}
      return AppKitProductionNavigationDismissEdges(
         topLeft: topLeft,
         topRight: topRight,
         bottomRight: bottomRight,
         bottomLeft: bottomLeft
      )
   }

   private func modalImage(_ key: AppKitProductionNavigationModalEdgeKey) -> NSImage?
   {
      lock.lock()
      defer {lock.unlock()}
      if let image = modalImages[key] {return image}
      guard let image = makeModalImage(key) else {return nil}
      modalImages[key] = image
      modalImageCreationCount += 1
      return image
   }

   private func dismissImage(_ key: AppKitProductionNavigationDismissEdgeKey) -> NSImage?
   {
      lock.lock()
      defer {lock.unlock()}
      if let image = dismissImages[key] {return image}
      guard let image = makeDismissImage(key) else {return nil}
      dismissImages[key] = image
      dismissImageCreationCount += 1
      return image
   }

   private func makeModalImage(_ key: AppKitProductionNavigationModalEdgeKey) -> NSImage?
   {
      let bottom = key.corner == .bottomLeft || key.corner == .bottomRight
      let right = key.corner == .topRight || key.corner == .bottomRight
      let patchWidth = Self.modalRadius
      let patchHeight = Self.modalRadius + (bottom ? Self.shadowOffset : 0)
      let originX = Int(key.originXParity)
      let originY = Int(key.originYParity)
      let patchX = right ? originX + Self.modalWidth - Self.modalRadius : originX
      let patchY = bottom ? originY + Self.modalHeight - Self.modalRadius : originY
      let surfaceRect = AppKitProductionPhysicalRect(
         x: originX,
         y: originY,
         width: Self.modalWidth,
         height: Self.modalHeight
      )
      let shadowRect = AppKitProductionPhysicalRect(
         x: originX,
         y: originY + Self.shadowOffset,
         width: Self.modalWidth,
         height: Self.modalHeight
      )
      let progress = Float(bitPattern: key.progressBits)
      let overlayAlpha = Float(176) / 255 * progress
      let sourceUnderlay = bottom ? Self.background : Self.surface
      let underlay = Self.composite(source: Self.overlay, alpha: overlayAlpha, over: sourceUnderlay)
      var pixels = [Float](repeating: 1, count: patchWidth * patchHeight * 4)
      for y in 0..<patchHeight
      {
         for x in 0..<patchWidth
         {
            let globalX = patchX + x
            let globalY = patchY + y
            let shadowCoverage = AppKitProductionAnalyticCoverage.circular(
               radius: Float(Self.modalRadius),
               rect: shadowRect,
               x: globalX,
               y: globalY
            )
            let surfaceCoverage = AppKitProductionAnalyticCoverage.circular(
               radius: Float(Self.modalRadius),
               rect: surfaceRect,
               x: globalX,
               y: globalY
            )
            let color = Self.compositeModal(
               underlay: underlay,
               shadowCoverage: shadowCoverage,
               surfaceCoverage: surfaceCoverage
            )
            let offset = (y * patchWidth + x) * 4
            pixels[offset] = color[0]
            pixels[offset + 1] = color[1]
            pixels[offset + 2] = color[2]
         }
      }
      return Self.image(pixels: pixels, width: patchWidth, height: patchHeight)
   }

   private func makeDismissImage(_ key: AppKitProductionNavigationDismissEdgeKey) -> NSImage?
   {
      let bottom = key.corner == .bottomLeft || key.corner == .bottomRight
      let right = key.corner == .topRight || key.corner == .bottomRight
      let originX = Int(key.originXParity)
      let originY = Int(key.originYParity)
      let patchX = right ? originX + Self.dismissWidth - Self.dismissRadius : originX
      let patchY = bottom ? originY + Self.dismissHeight - Self.dismissRadius : originY
      let control = AppKitProductionPhysicalRect(
         x: originX,
         y: originY,
         width: Self.dismissWidth,
         height: Self.dismissHeight
      )
      var pixels = [Float](repeating: 1, count: Self.dismissRadius * Self.dismissRadius * 4)
      for y in 0..<Self.dismissRadius
      {
         for x in 0..<Self.dismissRadius
         {
            let coverage = AppKitProductionAnalyticCoverage.circular(
               radius: Float(Self.dismissRadius),
               rect: control,
               x: patchX + x,
               y: patchY + y
            )
            let color = Self.composite(source: Self.accent, alpha: coverage, over: Self.surface)
            let offset = (y * Self.dismissRadius + x) * 4
            pixels[offset] = color[0]
            pixels[offset + 1] = color[1]
            pixels[offset + 2] = color[2]
         }
      }
      return Self.image(pixels: pixels, width: Self.dismissRadius, height: Self.dismissRadius)
   }

   private static func compositeModal(underlay: [Float], shadowCoverage: Float, surfaceCoverage: Float) -> [Float]
   {
      let shadowed = composite(source: shadow, alpha: shadowAlpha * shadowCoverage, over: underlay)
      return composite(source: surface, alpha: surfaceCoverage, over: shadowed)
   }

   private static func composite(source: [Float], alpha: Float, over underlay: [Float]) -> [Float]
   {
      (0..<3).map
      {
         channel in
         let composed = source[channel] * alpha + underlay[channel] * (1 - alpha)
         return srgb8ToLinear(linearToSRGB8(composed))
      }
   }

   private static func image(pixels: [Float], width: Int, height: Int) -> NSImage?
   {
      guard let provider = CGDataProvider(data: Data(bytes: pixels, count: pixels.count * MemoryLayout<Float>.size) as CFData),
            let colorSpace = CGColorSpace(name: CGColorSpace.linearSRGB),
            let image = CGImage(
               width: width,
               height: height,
               bitsPerComponent: 32,
               bitsPerPixel: 128,
               bytesPerRow: width * 16,
               space: colorSpace,
               bitmapInfo: CGBitmapInfo.floatComponents.union(.byteOrder32Little).union(CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue)),
               provider: provider,
               decode: nil,
               shouldInterpolate: false,
               intent: .defaultIntent
            ) else {return nil}
      return NSImage(
         cgImage: image,
         size: NSSize(width: CGFloat(width) / CGFloat(scale), height: CGFloat(height) / CGFloat(scale))
      )
   }

   private static func srgb8ToLinear(_ value: UInt8) -> Float
   {
      let encoded = Double(value) / 255
      return Float(encoded <= 0.04045 ? encoded / 12.92 : pow((encoded + 0.055) / 1.055, 2.4))
   }

   private static func linearToSRGB8(_ value: Float) -> UInt8
   {
      let linear = Double(min(1, max(0, value)))
      let encoded = linear <= 0.0031308 ? linear * 12.92 : 1.055 * pow(linear, 1 / 2.4) - 0.055
      return UInt8(min(255, max(0, Int((encoded * 255).rounded(.toNearestOrEven)))))
   }
}

private struct AppKitProductionPhysicalRect
{
   let x: Int
   let y: Int
   let width: Int
   let height: Int
}

private enum AppKitProductionCircularControlState: UInt8
{
   case inactive
   case accent
}

private struct AppKitProductionCircularControlKey: Hashable
{
   let originXParity: UInt8
   let originYParity: UInt8
   let state: AppKitProductionCircularControlState
}

struct AppKitProductionCircularControlImages
{
   let inactive: NSImage
   let accent: NSImage
}

final class AppKitProductionCircularControlRasterCache
{
   static let shared = AppKitProductionCircularControlRasterCache()

   private static let scale = 3
   private static let diameter = 18 * scale
   private static let radius = Float(diameter) * 0.5
   private static let inactive: [Float] = [0.456411023, 0.491020850, 0.564711506]
   private static let accent: [Float] = [0.046665086, 0.155926464, 0.863157213]

   private var images = [AppKitProductionCircularControlKey: NSImage]()
   private let lock = NSLock()

   private(set) var imageCreationCount = 0

   func images(origin: CGPoint) -> AppKitProductionCircularControlImages?
   {
      let originX = Int((origin.x * CGFloat(Self.scale)).rounded())
      let originY = Int((origin.y * CGFloat(Self.scale)).rounded())
      let inactiveKey = AppKitProductionCircularControlKey(
         originXParity: UInt8(originX & 1),
         originYParity: UInt8(originY & 1),
         state: .inactive
      )
      let accentKey = AppKitProductionCircularControlKey(
         originXParity: inactiveKey.originXParity,
         originYParity: inactiveKey.originYParity,
         state: .accent
      )
      guard let inactive = image(inactiveKey), let accent = image(accentKey) else {return nil}
      return AppKitProductionCircularControlImages(inactive: inactive, accent: accent)
   }

   private func image(_ key: AppKitProductionCircularControlKey) -> NSImage?
   {
      lock.lock()
      defer {lock.unlock()}
      if let image = images[key] {return image}
      guard let image = makeImage(key) else {return nil}
      images[key] = image
      imageCreationCount += 1
      return image
   }

   private func makeImage(_ key: AppKitProductionCircularControlKey) -> NSImage?
   {
      let originX = Int(key.originXParity)
      let originY = Int(key.originYParity)
      let source = key.state == .accent ? Self.accent : Self.inactive
      var pixels = [Float](repeating: 1, count: Self.diameter * Self.diameter * 4)
      for y in 0..<Self.diameter
      {
         for x in 0..<Self.diameter
         {
            let rect = AppKitProductionPhysicalRect(
               x: originX,
               y: originY,
               width: Self.diameter,
               height: Self.diameter
            )
            let coverage = AppKitProductionAnalyticCoverage.circular(
               radius: Self.radius,
               rect: rect,
               x: originX + x,
               y: originY + y
            )
            let offset = (y * Self.diameter + x) * 4
            for channel in 0..<3
            {
               let composed = source[channel] * coverage + 1 - coverage
               pixels[offset + channel] = Self.srgb8ToLinear(Self.linearToSRGB8(composed))
            }
         }
      }
      guard let provider = CGDataProvider(data: Data(bytes: pixels, count: pixels.count * MemoryLayout<Float>.size) as CFData),
            let colorSpace = CGColorSpace(name: CGColorSpace.linearSRGB),
            let image = CGImage(
               width: Self.diameter,
               height: Self.diameter,
               bitsPerComponent: 32,
               bitsPerPixel: 128,
               bytesPerRow: Self.diameter * 16,
               space: colorSpace,
               bitmapInfo: CGBitmapInfo.floatComponents.union(.byteOrder32Little).union(CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue)),
               provider: provider,
               decode: nil,
               shouldInterpolate: false,
               intent: .defaultIntent
            ) else {return nil}
      return NSImage(cgImage: image, size: NSSize(width: 18, height: 18))
   }

   private static func srgb8ToLinear(_ value: UInt8) -> Float
   {
      let encoded = Double(value) / 255
      return Float(encoded <= 0.04045 ? encoded / 12.92 : pow((encoded + 0.055) / 1.055, 2.4))
   }

   private static func linearToSRGB8(_ value: Float) -> UInt8
   {
      let linear = Double(min(1, max(0, value)))
      let encoded = linear <= 0.0031308 ? linear * 12.92 : 1.055 * pow(linear, 1 / 2.4) - 0.055
      return UInt8(min(255, max(0, Int((encoded * 255).rounded(.toNearestOrEven)))))
   }
}

final class AppKitProductionInlineImageMetalRasterizer
{
   static let shared = AppKitProductionInlineImageMetalRasterizer()

   private static let maximumTileSize = 128

   private let device: MTLDevice
   private let queue: MTLCommandQueue
   private let pipeline: MTLRenderPipelineState
   private let target: MTLTexture
   private var sourceIdentity: ObjectIdentifier?
   private var sourceTexture: MTLTexture?

   private init?()
   {
      guard let device = MTLCreateSystemDefaultDevice(), let queue = device.makeCommandQueue() else {return nil}
      let shader = """
      #include <metal_stdlib>
      using namespace metal;

      struct Output
      {
         float4 position [[position]];
      };

      vertex Output inline_image_vertex(uint vertexID [[vertex_id]])
      {
         float2 corner = float2((vertexID << 1) & 2, vertexID & 2);
         Output output;
         output.position = float4(corner * float2(2.0, -2.0) + float2(-1.0, 1.0), 0.0, 1.0);
         return output;
      }

      fragment float4 inline_image_fragment(
         Output input [[stage_in]],
         texture2d<float, access::read> source [[texture(0)]],
         constant uint2& sourceOrigin [[buffer(0)]])
      {
         return source.read(sourceOrigin + uint2(input.position.xy));
      }
      """
      guard let library = try? device.makeLibrary(source: shader, options: nil),
            let vertex = library.makeFunction(name: "inline_image_vertex"),
            let fragment = library.makeFunction(name: "inline_image_fragment") else {return nil}
      let pipelineDescriptor = MTLRenderPipelineDescriptor()
      pipelineDescriptor.vertexFunction = vertex
      pipelineDescriptor.fragmentFunction = fragment
      let attachment = pipelineDescriptor.colorAttachments[0]!
      attachment.pixelFormat = .bgra8Unorm_srgb
      attachment.isBlendingEnabled = true
      attachment.rgbBlendOperation = .add
      attachment.alphaBlendOperation = .add
      attachment.sourceRGBBlendFactor = .sourceAlpha
      attachment.destinationRGBBlendFactor = .oneMinusSourceAlpha
      attachment.sourceAlphaBlendFactor = .one
      attachment.destinationAlphaBlendFactor = .zero
      let targetDescriptor = MTLTextureDescriptor.texture2DDescriptor(
         pixelFormat: .bgra8Unorm_srgb,
         width: Self.maximumTileSize,
         height: Self.maximumTileSize,
         mipmapped: false
      )
      targetDescriptor.storageMode = .shared
      targetDescriptor.usage = [.renderTarget]
      guard let pipeline = try? device.makeRenderPipelineState(descriptor: pipelineDescriptor),
            let target = device.makeTexture(descriptor: targetDescriptor) else {return nil}
      self.device = device
      self.queue = queue
      self.pipeline = pipeline
      self.target = target
   }

   func raster(atlas: CGImage, source sourceRect: CGRect) -> CGImage?
   {
      let width = Int(sourceRect.width)
      let height = Int(sourceRect.height)
      guard width > 0, height > 0,
            width <= Self.maximumTileSize, height <= Self.maximumTileSize,
            sourceRect.minX >= 0, sourceRect.minY >= 0,
            sourceRect.maxX <= CGFloat(atlas.width), sourceRect.maxY <= CGFloat(atlas.height),
            let source = prepareSource(atlas) else {return nil}
      let pass = MTLRenderPassDescriptor()
      pass.colorAttachments[0].texture = target
      pass.colorAttachments[0].loadAction = .clear
      pass.colorAttachments[0].storeAction = .store
      pass.colorAttachments[0].clearColor = MTLClearColorMake(1, 1, 1, 1)
      guard let commandBuffer = queue.makeCommandBuffer(),
            let encoder = commandBuffer.makeRenderCommandEncoder(descriptor: pass) else {return nil}
      var sourceOrigin = SIMD2<UInt32>(UInt32(sourceRect.minX), UInt32(sourceRect.minY))
      encoder.setViewport(MTLViewport(originX: 0, originY: 0, width: Double(width), height: Double(height), znear: 0, zfar: 1))
      encoder.setScissorRect(MTLScissorRect(x: 0, y: 0, width: width, height: height))
      encoder.setRenderPipelineState(pipeline)
      encoder.setFragmentTexture(source, index: 0)
      encoder.setFragmentBytes(&sourceOrigin, length: MemoryLayout<SIMD2<UInt32>>.size, index: 0)
      encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
      encoder.endEncoding()
      commandBuffer.commit()
      commandBuffer.waitUntilCompleted()
      guard commandBuffer.status == .completed else {return nil}
      var pixels = [UInt8](repeating: 255, count: width * height * 4)
      target.getBytes(
         &pixels,
         bytesPerRow: width * 4,
         from: MTLRegionMake2D(0, 0, width, height),
         mipmapLevel: 0
      )
      for offset in stride(from: 3, to: pixels.count, by: 4)
      {
         pixels[offset] = 255
      }
      guard let provider = CGDataProvider(data: Data(pixels) as CFData),
            let colorSpace = CGColorSpace(name: CGColorSpace.sRGB) else {return nil}
      return CGImage(
         width: width,
         height: height,
         bitsPerComponent: 8,
         bitsPerPixel: 32,
         bytesPerRow: width * 4,
         space: colorSpace,
         bitmapInfo: CGBitmapInfo.byteOrder32Little.union(CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedFirst.rawValue)),
         provider: provider,
         decode: nil,
         shouldInterpolate: false,
         intent: .defaultIntent
      )
   }

   private func prepareSource(_ image: CGImage) -> MTLTexture?
   {
      let identity = ObjectIdentifier(image)
      if sourceIdentity == identity {return sourceTexture}
      guard image.bitsPerComponent == 8,
            image.bitsPerPixel == 32,
            image.bytesPerRow == image.width * 4,
            image.alphaInfo == .last,
            image.colorSpace?.name == CGColorSpace.sRGB,
            let data = image.dataProvider?.data as Data?,
            data.count == image.width * image.height * 4 else {return nil}
      let descriptor = MTLTextureDescriptor.texture2DDescriptor(
         pixelFormat: .rgba8Unorm_srgb,
         width: image.width,
         height: image.height,
         mipmapped: false
      )
      descriptor.storageMode = .shared
      descriptor.usage = [.shaderRead]
      guard let texture = device.makeTexture(descriptor: descriptor) else {return nil}
      data.withUnsafeBytes
      {
         bytes in
         guard let baseAddress = bytes.baseAddress else {return}
         texture.replace(
            region: MTLRegionMake2D(0, 0, image.width, image.height),
            mipmapLevel: 0,
            withBytes: baseAddress,
            bytesPerRow: image.bytesPerRow
         )
      }
      sourceIdentity = identity
      sourceTexture = texture
      return texture
   }
}

private struct AppKitProductionRoundedImageKey: Hashable
{
   let tileIndex: UInt8
   let originX: Int
   let originY: Int
   let pointSize: UInt8
   let radius: UInt8
   let underlay: AppKitProductionRoundedImageUnderlay
}

enum AppKitProductionRoundedImageUnderlay: UInt8
{
   case surface
   case background
}

struct AppKitProductionRoundedImageRaster
{
   let image: CGImage

   func draw(in bounds: CGRect)
   {
      guard let context = NSGraphicsContext.current?.cgContext else {return}
      context.saveGState()
      context.setBlendMode(.copy)
      context.interpolationQuality = .none
      context.translateBy(x: bounds.minX, y: bounds.maxY)
      context.scaleBy(x: 1, y: -1)
      context.draw(image, in: CGRect(origin: .zero, size: bounds.size))
      context.restoreGState()
   }
}

final class AppKitProductionRoundedImageRasterCache
{
   static let shared = AppKitProductionRoundedImageRasterCache()

   private var images = [AppKitProductionRoundedImageKey: AppKitProductionRoundedImageRaster]()
   private let lock = NSLock()
   private let rasterizer = AppKitProductionRoundedImageMetalRasterizer()

   private(set) var imageCreationCount = 0

   func raster(atlas: NSImage, tileIndex: Int, origin: CGPoint, pointSize: Int = 48, radius: Int = 8, underlay: AppKitProductionRoundedImageUnderlay = .surface) -> AppKitProductionRoundedImageRaster?
   {
      guard tileIndex >= 0, tileIndex < 128,
            pointSize > 0, pointSize <= 48,
            radius > 0, radius <= pointSize / 2 else {return nil}
      let originX = Int((origin.x * 3).rounded())
      let originY = Int((origin.y * 3).rounded())
      let key = AppKitProductionRoundedImageKey(
         tileIndex: UInt8(tileIndex),
         originX: originX,
         originY: originY,
         pointSize: UInt8(pointSize),
         radius: UInt8(radius),
         underlay: underlay
      )
      lock.lock()
      defer {lock.unlock()}
      if let image = images[key] {return image}
      guard let image = rasterizer?.raster(
         atlas: atlas,
         tileIndex: tileIndex,
         originX: key.originX,
         originY: key.originY,
         pointSize: pointSize,
         radius: radius,
         underlay: underlay
      ) else {return nil}
      images[key] = image
      imageCreationCount += 1
      return image
   }
}

private final class AppKitProductionRoundedImageMetalRasterizer
{
   private static let sourceWidth = 384
   private static let sourceHeight = 192
   private static let maximumDestinationSize = 144
   private static let targetWidth = 1_170
   private static let targetHeight = 2_532

   private let queue: MTLCommandQueue
   private let pipeline: MTLRenderPipelineState
   private let sampler: MTLSamplerState
   private let source: MTLTexture
   private let target: MTLTexture
   private let encodedInputTable: [UInt8]
   private var sourceLoaded = false

   init?()
   {
      guard let device = MTLCreateSystemDefaultDevice(), let queue = device.makeCommandQueue() else {return nil}
      let shader = """
      #include <metal_stdlib>
      using namespace metal;

      struct Output
      {
         float4 position [[position]];
         float2 positionPoints;
         float2 rectOrigin [[flat]];
      };

      vertex Output rounded_image_vertex(
         uint vertexID [[vertex_id]],
         constant float4& rect [[buffer(0)]],
         constant float2& viewport [[buffer(1)]])
      {
         float2 offsets[6] = {
            float2(0.0, 0.0), float2(1.0, 0.0), float2(0.0, 1.0),
            float2(0.0, 1.0), float2(1.0, 0.0), float2(1.0, 1.0)
         };
         float2 point = rect.xy + offsets[vertexID] * rect.zw;
         float2 clip = float2(
            point.x / max(viewport.x, 1e-5) * 2.0 - 1.0,
            1.0 - point.y / max(viewport.y, 1e-5) * 2.0
         );
         Output output;
         output.position = float4(clip, 0.0, 1.0);
         output.positionPoints = point;
         output.rectOrigin = rect.xy;
         return output;
      }

      fragment float4 rounded_image_fragment(
         Output input [[stage_in]],
         texture2d<float> source [[texture(0)]],
         sampler linearSampler [[sampler(0)]],
         constant float2& sourceOrigin [[buffer(0)]],
         constant float2& shape [[buffer(1)]])
      {
         float2 xy = input.positionPoints - input.rectOrigin;
         float2 size = float2(shape.x);
         if (xy.x < 0.0 || xy.y < 0.0 || xy.x > size.x || xy.y > size.y)
         {
            discard_fragment();
         }
         float2 center = size * 0.5;
         float2 q = abs(xy - center) - (center - shape.y);
         float signedDistance = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - shape.y;
         float shapeCoverage = clamp(0.5 - signedDistance / max(fwidth(signedDistance), 1e-5), 0.0, 1.0);
         if (shapeCoverage <= 0.0)
         {
            discard_fragment();
         }
         float2 uvPixels = sourceOrigin + xy * (24.0 / shape.x);
         uvPixels = clamp(uvPixels, sourceOrigin + 0.5, sourceOrigin + 23.5);
         float4 color = source.sample(linearSampler, uvPixels / float2(384.0, 192.0));
         color.a *= shapeCoverage;
         return color;
      }

      struct EncodeOutput
      {
         float4 position [[position]];
      };

      vertex EncodeOutput encode_vertex(uint vertexID [[vertex_id]])
      {
         float2 corner = float2((vertexID << 1) & 2, vertexID & 2);
         EncodeOutput output;
         output.position = float4(corner * float2(2.0, -2.0) + float2(-1.0, 1.0), 0.0, 1.0);
         return output;
      }

      fragment float4 encode_fragment(
         EncodeOutput input [[stage_in]],
         texture2d<float, access::read> source [[texture(0)]])
      {
         return source.read(uint2(input.position.xy));
      }
      """
      guard let library = try? device.makeLibrary(source: shader, options: nil),
            let vertex = library.makeFunction(name: "rounded_image_vertex"),
            let fragment = library.makeFunction(name: "rounded_image_fragment") else {return nil}
      let descriptor = MTLRenderPipelineDescriptor()
      descriptor.vertexFunction = vertex
      descriptor.fragmentFunction = fragment
      let attachment = descriptor.colorAttachments[0]!
      attachment.pixelFormat = .bgra8Unorm_srgb
      attachment.isBlendingEnabled = true
      attachment.rgbBlendOperation = .add
      attachment.alphaBlendOperation = .add
      attachment.sourceRGBBlendFactor = .sourceAlpha
      attachment.destinationRGBBlendFactor = .oneMinusSourceAlpha
      attachment.sourceAlphaBlendFactor = .one
      attachment.destinationAlphaBlendFactor = .zero
      let samplerDescriptor = MTLSamplerDescriptor()
      samplerDescriptor.minFilter = .linear
      samplerDescriptor.magFilter = .linear
      samplerDescriptor.mipFilter = .linear
      samplerDescriptor.sAddressMode = .clampToEdge
      samplerDescriptor.tAddressMode = .clampToEdge
      let sourceDescriptor = MTLTextureDescriptor.texture2DDescriptor(
         pixelFormat: .rgba8Unorm_srgb,
         width: Self.sourceWidth,
         height: Self.sourceHeight,
         mipmapped: false
      )
      sourceDescriptor.storageMode = .shared
      sourceDescriptor.usage = [.shaderRead]
      let targetDescriptor = MTLTextureDescriptor.texture2DDescriptor(
         pixelFormat: .bgra8Unorm_srgb,
         width: Self.targetWidth,
         height: Self.targetHeight,
         mipmapped: false
      )
      targetDescriptor.storageMode = .shared
      targetDescriptor.usage = [.renderTarget]
      guard let pipeline = try? device.makeRenderPipelineState(descriptor: descriptor),
            let sampler = device.makeSamplerState(descriptor: samplerDescriptor),
            let source = device.makeTexture(descriptor: sourceDescriptor),
            let target = device.makeTexture(descriptor: targetDescriptor),
            let encodedInputTable = Self.makeEncodedInputTable(device: device, queue: queue, library: library) else {return nil}
      self.queue = queue
      self.pipeline = pipeline
      self.sampler = sampler
      self.source = source
      self.target = target
      self.encodedInputTable = encodedInputTable
   }

   func raster(atlas image: NSImage, tileIndex: Int, originX: Int, originY: Int, pointSize: Int, radius: Int, underlay: AppKitProductionRoundedImageUnderlay) -> AppKitProductionRoundedImageRaster?
   {
      let destinationSize = pointSize * 3
      guard destinationSize > 0, destinationSize <= Self.maximumDestinationSize else {return nil}
      let readMinX = max(0, originX)
      let readMinY = max(0, originY)
      let readMaxX = min(Self.targetWidth, originX + destinationSize)
      let readMaxY = min(Self.targetHeight, originY + destinationSize)
      guard readMinX < readMaxX, readMinY < readMaxY else {return nil}
      if !sourceLoaded
      {
         guard let bytes = sourceBytes(image) else {return nil}
         source.replace(
            region: MTLRegionMake2D(0, 0, Self.sourceWidth, Self.sourceHeight),
            mipmapLevel: 0,
            withBytes: bytes,
            bytesPerRow: Self.sourceWidth * 4
         )
         sourceLoaded = true
      }
      let pass = MTLRenderPassDescriptor()
      pass.colorAttachments[0].texture = target
      pass.colorAttachments[0].loadAction = .clear
      pass.colorAttachments[0].storeAction = .store
      pass.colorAttachments[0].clearColor = underlay == .surface
         ? MTLClearColorMake(1, 1, 1, 1)
         : MTLClearColorMake(0.896269353, 0.913098652, 0.938685728, 1)
      guard let commandBuffer = queue.makeCommandBuffer(),
            let encoder = commandBuffer.makeRenderCommandEncoder(descriptor: pass) else {return nil}
      var rect = SIMD4<Float>(
         Float(originX) / 3,
         Float(originY) / 3,
         Float(pointSize),
         Float(pointSize)
      )
      var viewport = SIMD2<Float>(390, 844)
      var sourceOrigin = SIMD2<Float>(Float(tileIndex % 16) * 24, Float(tileIndex / 16) * 24)
      var shape = SIMD2<Float>(Float(pointSize), Float(radius))
      encoder.setRenderPipelineState(pipeline)
      encoder.setFragmentTexture(source, index: 0)
      encoder.setFragmentSamplerState(sampler, index: 0)
      encoder.setFragmentBytes(&sourceOrigin, length: MemoryLayout<SIMD2<Float>>.size, index: 0)
      encoder.setFragmentBytes(&shape, length: MemoryLayout<SIMD2<Float>>.size, index: 1)
      encoder.setVertexBytes(&rect, length: MemoryLayout<SIMD4<Float>>.size, index: 0)
      encoder.setVertexBytes(&viewport, length: MemoryLayout<SIMD2<Float>>.size, index: 1)
      encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 6)
      encoder.endEncoding()
      commandBuffer.commit()
      commandBuffer.waitUntilCompleted()
      guard commandBuffer.status == .completed else {return nil}
      let readWidth = readMaxX - readMinX
      let readHeight = readMaxY - readMinY
      let readBytesPerRow = readWidth * 4
      var readback = [UInt8](repeating: 0, count: readBytesPerRow * readHeight)
      target.getBytes(
         &readback,
         bytesPerRow: readBytesPerRow,
         from: MTLRegionMake2D(readMinX, readMinY, readWidth, readHeight),
         mipmapLevel: 0
      )
      let underlayPixel: [UInt8] = underlay == .surface ? [255, 255, 255, 255] : [248, 245, 243, 255]
      var output = [UInt8]()
      output.reserveCapacity(destinationSize * destinationSize * 4)
      for _ in 0..<(destinationSize * destinationSize)
      {
         output.append(contentsOf: underlayPixel)
      }
      let destinationX = readMinX - originX
      let destinationY = readMinY - originY
      for row in 0..<readHeight
      {
         let sourceStart = row * readBytesPerRow
         let destinationStart = ((destinationY + row) * destinationSize + destinationX) * 4
         output.replaceSubrange(
            destinationStart..<(destinationStart + readBytesPerRow),
            with: readback[sourceStart..<(sourceStart + readBytesPerRow)]
         )
      }
      guard let image = makeImage(output, size: destinationSize) else {return nil}
      return AppKitProductionRoundedImageRaster(image: image)
   }

   private func sourceBytes(_ image: NSImage) -> [UInt8]?
   {
      guard let cgImage = image.cgImage(forProposedRect: nil, context: nil, hints: nil),
            cgImage.width == Self.sourceWidth,
            cgImage.height == Self.sourceHeight,
            cgImage.bitsPerComponent == 8,
            cgImage.bitsPerPixel == 32,
            cgImage.bytesPerRow == Self.sourceWidth * 4,
            cgImage.alphaInfo == .last,
            cgImage.colorSpace?.name == CGColorSpace.sRGB,
            let data = cgImage.dataProvider?.data as Data?,
            data.count == Self.sourceWidth * Self.sourceHeight * 4 else {return nil}
      return [UInt8](data)
   }

   private func makeImage(_ output: [UInt8], size: Int) -> CGImage?
   {
      var pixels = output
      for offset in stride(from: 0, to: output.count, by: 4)
      {
         pixels[offset] = encodedInputTable[Int(output[offset])]
         pixels[offset + 1] = encodedInputTable[Int(output[offset + 1])]
         pixels[offset + 2] = encodedInputTable[Int(output[offset + 2])]
         pixels[offset + 3] = 255
      }
      guard let provider = CGDataProvider(data: Data(pixels) as CFData),
            let colorSpace = CGColorSpace(name: CGColorSpace.sRGB) else {return nil}
      return CGImage(
               width: size,
               height: size,
               bitsPerComponent: 8,
               bitsPerPixel: 32,
               bytesPerRow: size * 4,
               space: colorSpace,
               bitmapInfo: CGBitmapInfo.byteOrder32Little.union(CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedFirst.rawValue)),
               provider: provider,
               decode: nil,
               shouldInterpolate: false,
               intent: .defaultIntent
      )
   }

   private static func makeEncodedInputTable(device: MTLDevice, queue: MTLCommandQueue, library: MTLLibrary) -> [UInt8]?
   {
      var encodedSource = [UInt8](repeating: 255, count: 256 * 4)
      for value in 0..<256
      {
         encodedSource[value * 4] = UInt8(value)
         encodedSource[value * 4 + 1] = UInt8(value)
         encodedSource[value * 4 + 2] = UInt8(value)
      }
      guard let provider = CGDataProvider(data: Data(encodedSource) as CFData),
            let encodedColorSpace = CGColorSpace(name: CGColorSpace.sRGB),
            let encodedImage = CGImage(
               width: 256,
               height: 1,
               bitsPerComponent: 8,
               bitsPerPixel: 32,
               bytesPerRow: 256 * 4,
               space: encodedColorSpace,
               bitmapInfo: CGBitmapInfo.byteOrder32Little.union(CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedFirst.rawValue)),
               provider: provider,
               decode: nil,
               shouldInterpolate: false,
               intent: .defaultIntent
            ),
            let linearColorSpace = CGColorSpace(name: CGColorSpace.linearSRGB) else {return nil}
      var linear = [Float](repeating: 1, count: 256 * 4)
      let rendered = linear.withUnsafeMutableBytes
      {
         bytes -> Bool in
         guard let baseAddress = bytes.baseAddress,
               let context = CGContext(
                  data: baseAddress,
                  width: 256,
                  height: 1,
                  bitsPerComponent: 32,
                  bytesPerRow: 256 * 16,
                  space: linearColorSpace,
                  bitmapInfo: CGBitmapInfo.floatComponents.rawValue
                     | CGBitmapInfo.byteOrder32Little.rawValue
                     | CGImageAlphaInfo.noneSkipLast.rawValue
               ) else {return false}
         context.setBlendMode(.copy)
         context.interpolationQuality = .none
         context.draw(encodedImage, in: CGRect(x: 0, y: 0, width: 256, height: 1))
         return true
      }
      guard rendered,
            let vertex = library.makeFunction(name: "encode_vertex"),
            let fragment = library.makeFunction(name: "encode_fragment") else {return nil}
      let pipelineDescriptor = MTLRenderPipelineDescriptor()
      pipelineDescriptor.vertexFunction = vertex
      pipelineDescriptor.fragmentFunction = fragment
      pipelineDescriptor.colorAttachments[0].pixelFormat = .bgra8Unorm_srgb
      guard let pipeline = try? device.makeRenderPipelineState(descriptor: pipelineDescriptor) else {return nil}
      let sourceDescriptor = MTLTextureDescriptor.texture2DDescriptor(
         pixelFormat: .rgba32Float,
         width: 256,
         height: 1,
         mipmapped: false
      )
      sourceDescriptor.storageMode = .shared
      sourceDescriptor.usage = [.shaderRead]
      let targetDescriptor = MTLTextureDescriptor.texture2DDescriptor(
         pixelFormat: .bgra8Unorm_srgb,
         width: 256,
         height: 1,
         mipmapped: false
      )
      targetDescriptor.storageMode = .shared
      targetDescriptor.usage = [.renderTarget]
      guard let source = device.makeTexture(descriptor: sourceDescriptor),
            let target = device.makeTexture(descriptor: targetDescriptor),
            let commandBuffer = queue.makeCommandBuffer() else {return nil}
      source.replace(
         region: MTLRegionMake2D(0, 0, 256, 1),
         mipmapLevel: 0,
         withBytes: linear,
         bytesPerRow: 256 * 16
      )
      let pass = MTLRenderPassDescriptor()
      pass.colorAttachments[0].texture = target
      pass.colorAttachments[0].loadAction = .dontCare
      pass.colorAttachments[0].storeAction = .store
      guard let encoder = commandBuffer.makeRenderCommandEncoder(descriptor: pass) else {return nil}
      encoder.setRenderPipelineState(pipeline)
      encoder.setFragmentTexture(source, index: 0)
      encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
      encoder.endEncoding()
      commandBuffer.commit()
      commandBuffer.waitUntilCompleted()
      guard commandBuffer.status == .completed else {return nil}
      var output = [UInt8](repeating: 0, count: 256 * 4)
      target.getBytes(
         &output,
         bytesPerRow: 256 * 4,
         from: MTLRegionMake2D(0, 0, 256, 1),
         mipmapLevel: 0
      )
      var inverse = [UInt8](repeating: 0, count: 256)
      for desired in 0..<256
      {
         guard let candidate = (0..<256).min(by: {
            let leftOutput = Int(output[$0 * 4])
            let rightOutput = Int(output[$1 * 4])
            let leftDelta = abs(leftOutput - desired)
            let rightDelta = abs(rightOutput - desired)
            if leftDelta != rightDelta {return leftDelta < rightDelta}
            return abs($0 - desired) < abs($1 - desired)
         }), output[candidate * 4] == UInt8(desired) else {return nil}
         inverse[desired] = UInt8(candidate)
      }
      return inverse
   }
}
