import AppKit
import CoreText
import Foundation
import ImageIO
import Metal

enum AppKitScenarioAdapterFailure: Error
{
   case activeAnimations
   case invalidFixture
   case missingScenario
   case unsupportedEvent(String)
   case unsupportedScenario(String)
}

private final class AppKitCanonicalSRGBEncoder
{
   private let device: MTLDevice
   private let queue: MTLCommandQueue
   private let pipeline: MTLRenderPipelineState

   init?()
   {
      guard let device = MTLCreateSystemDefaultDevice(),
            let queue = device.makeCommandQueue() else
      {
         return nil
      }
      let source = """
      #include <metal_stdlib>
      using namespace metal;

      struct CanonicalVertex
      {
         float4 position [[position]];
      };

      vertex CanonicalVertex canonical_vertex(uint vertex_id [[vertex_id]])
      {
         float2 corner = float2((vertex_id << 1) & 2, vertex_id & 2);
         CanonicalVertex output;
         output.position = float4(corner * float2(2.0, -2.0) + float2(-1.0, 1.0), 0.0, 1.0);
         return output;
      }

      fragment float4 canonical_fragment(
         CanonicalVertex input [[stage_in]],
         texture2d<float, access::read> source [[texture(0)]])
      {
         return source.read(uint2(input.position.xy));
      }
      """
      guard let library = try? device.makeLibrary(source: source, options: nil),
            let vertex = library.makeFunction(name: "canonical_vertex"),
            let fragment = library.makeFunction(name: "canonical_fragment") else
      {
         return nil
      }
      let descriptor = MTLRenderPipelineDescriptor()
      descriptor.vertexFunction = vertex
      descriptor.fragmentFunction = fragment
      descriptor.colorAttachments[0].pixelFormat = .bgra8Unorm_srgb
      guard let pipeline = try? device.makeRenderPipelineState(descriptor: descriptor) else
      {
         return nil
      }
      self.device = device
      self.queue = queue
      self.pipeline = pipeline
   }

   func encode(linearContext: CGContext, width: Int, height: Int) -> Data?
   {
      guard let sourceData = linearContext.data else {return nil}
      let sourceDescriptor = MTLTextureDescriptor.texture2DDescriptor(
         pixelFormat: .rgba32Float,
         width: width,
         height: height,
         mipmapped: false
      )
      sourceDescriptor.storageMode = .shared
      sourceDescriptor.usage = [.shaderRead]
      let outputDescriptor = MTLTextureDescriptor.texture2DDescriptor(
         pixelFormat: .bgra8Unorm_srgb,
         width: width,
         height: height,
         mipmapped: false
      )
      outputDescriptor.storageMode = .shared
      outputDescriptor.usage = [.renderTarget]
      guard let source = device.makeTexture(descriptor: sourceDescriptor),
            let output = device.makeTexture(descriptor: outputDescriptor),
            let commandBuffer = queue.makeCommandBuffer() else
      {
         return nil
      }
      source.replace(
         region: MTLRegionMake2D(0, 0, width, height),
         mipmapLevel: 0,
         withBytes: sourceData,
         bytesPerRow: width * 16
      )
      let pass = MTLRenderPassDescriptor()
      pass.colorAttachments[0].texture = output
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

      var bytes = Data(count: width * height * 4)
      bytes.withUnsafeMutableBytes
      {
         output.getBytes(
            $0.baseAddress!,
            bytesPerRow: width * 4,
            from: MTLRegionMake2D(0, 0, width, height),
            mipmapLevel: 0
         )
      }
      guard let provider = CGDataProvider(data: bytes as CFData),
            let colorSpace = CGColorSpace(name: CGColorSpace.sRGB),
            let image = CGImage(
               width: width,
               height: height,
               bitsPerComponent: 8,
               bitsPerPixel: 32,
               bytesPerRow: width * 4,
               space: colorSpace,
               bitmapInfo: CGBitmapInfo.byteOrder32Little.union(
                  CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipFirst.rawValue)
               ),
               provider: provider,
               decode: nil,
               shouldInterpolate: false,
               intent: .defaultIntent
            ) else
      {
         return nil
      }
      return NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:])
   }
}

private struct AppKitStartupCard: Decodable
{
   let id: String
   let dataOffset: Int
   let dataLength: Int
   let thumbnailIndex: Int
   let initiallyVisible: Bool
}

private struct AppKitStartupFixture: Decodable
{
   let schemaVersion: UInt32
   let id: String
   let dataSizeBytes: Int
   let data: String
   let cardCount: Int
   let cards: [AppKitStartupCard]
   let initialImageIndices: [Int]
   let headerId: String
   let navigationId: String
   let controlId: String
}

private struct AppKitChatMessage: Decodable
{
   let id: String
   let sequence: Int
   let authorIndex: Int
   let avatarIndex: Int
   let direction: String
   var text: String
}

private struct AppKitChatSelectionReplacement: Decodable
{
   let messageId: String
   let startUtf8: Int
   let endUtf8: Int
   let replacement: String
}

private struct AppKitChatFixture: Decodable
{
   let schemaVersion: UInt32
   let id: String
   let messageCount: Int
   let avatarCount: Int
   let messages: [AppKitChatMessage]
   let prependMessages: [AppKitChatMessage]
   let appendRateHz: Int
   let typedText: String
   let pastedText: String
   let selectionReplacement: AppKitChatSelectionReplacement
}

private struct AppKitImageFileFixture: Decodable
{
   let artifact: BenchmarkArtifactIdentity
   let width: Int
   let height: Int
   let format: String
   let colorSpace: String
}

private struct AppKitImageFixture: Decodable
{
   let schemaVersion: UInt32
   let id: String
   let source: AppKitImageFileFixture
   let thumbnail: AppKitImageFileFixture
   let panDistanceMillionths: Int32
   let pinchScaleMillionths: Int32
}

private let appKitFixtureDecoder: JSONDecoder =
{
   let decoder = JSONDecoder()
   decoder.keyDecodingStrategy = .convertFromSnakeCase
   return decoder
}()

private let appKitBackground = NSColor(srgbRed: 243 / 255, green: 245 / 255, blue: 248 / 255, alpha: 1)
private let appKitSurface = NSColor.white
private let appKitText = NSColor(srgbRed: 32 / 255, green: 36 / 255, blue: 44 / 255, alpha: 1)
private let appKitSecondaryText = NSColor(srgbRed: 105 / 255, green: 113 / 255, blue: 129 / 255, alpha: 1)
private let appKitAccent = NSColor(srgbRed: 61 / 255, green: 110 / 255, blue: 239 / 255, alpha: 1)
private let appKitInactiveControl = NSColor(srgbRed: 180 / 255, green: 186 / 255, blue: 198 / 255, alpha: 1)
private let appKitSeparator = NSColor(srgbRed: 188 / 255, green: 188 / 255, blue: 188 / 255, alpha: 1)
private let appKitModalOverlay = NSColor(srgbRed: 25 / 255, green: 28 / 255, blue: 35 / 255, alpha: 1)
private let appKitShadow = NSColor(srgbRed: 32 / 255, green: 36 / 255, blue: 44 / 255, alpha: 41 / 255)
private let appKitDashboardMaterial = NSColor(srgbRed: 224 / 255, green: 226 / 255, blue: 239 / 255, alpha: 56 / 255)
private let appKitDisabledSliderTrack = NSColor(srgbRed: 231 / 255, green: 233 / 255, blue: 234 / 255, alpha: 1)

private func appKitAccessibilityRole(_ role: String) -> NSAccessibility.Role
{
   switch role
   {
   case "header", "label", "message", "navigation-bar": return .staticText
   case "initial-image", "icon-image", "thumbnail", "avatar", "image": return .image
   case "primary-control", "control", "favorite-control", "send-control", "list-item", "dismiss-control", "back-control": return .button
   case "feed", "chat-thread", "navigation-list": return .list
   case "composer": return .textField
   case "zoom-control": return .slider
   default: return .group
   }
}

private func appKitLayerTreeHasAnimations(_ layer: CALayer) -> Bool
{
   if !(layer.animationKeys() ?? []).isEmpty
   {
      return true
   }
   return (layer.sublayers ?? []).contains(where: appKitLayerTreeHasAnimations)
}

private func appKitRemoveLayerTreeAnimations(_ layer: CALayer)
{
   layer.removeAllAnimations()
   (layer.sublayers ?? []).forEach(appKitRemoveLayerTreeAnimations)
}

private struct AppKitInlinePiece
{
   let text: String
   let image: CGImage?
   let advance: CGFloat
   let width: CGFloat
   let height: CGFloat
   let topFromBaseline: CGFloat
}

private final class AppKitPreparedInlineText
{
   private let contract: BenchmarkInlineTextAtlas
   private let images: [UInt32: [String: CGImage]]
   private var cache = [String: NSAttributedString]()

   init(contract: BenchmarkInlineTextAtlas, rasterImages: [UInt32: CGImage]) throws
   {
      var images = [UInt32: [String: CGImage]]()
      for variant in contract.variants
      {
         guard let raster = rasterImages[variant.emPixels] else
         {
            throw AppKitScenarioAdapterFailure.invalidFixture
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
            guard let tile = raster.cropping(to: source) else
            {
               throw AppKitScenarioAdapterFailure.invalidFixture
            }
            tiles[entry.grapheme] = tile
         }
         images[variant.emPixels] = tiles
      }
      self.contract = contract
      self.images = images
   }

   func attributedText(_ value: String, font: NSFont, color: NSColor, alignment: NSTextAlignment) -> NSAttributedString?
   {
      guard contract.entries.contains(where: {value.contains($0.grapheme)}) else {return nil}
      let key = "\(font.fontName)|\(font.pointSize)|\(color.description)|\(alignment.rawValue)|\(value)"
      if let cached = cache[key] {return cached}
      let targetPixels = UInt32((font.pointSize * 3).rounded())
      guard let variant = contract.variants.min(by: {
         abs(Int64($0.emPixels) - Int64(targetPixels)) < abs(Int64($1.emPixels) - Int64(targetPixels))
      }), let tiles = images[variant.emPixels] else {return nil}
      let paragraph = NSMutableParagraphStyle()
      paragraph.alignment = alignment
      let attributes: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: color, .paragraphStyle: paragraph]
      let result = NSMutableAttributedString()
      var remaining = value[...]
      while let match = nextMatch(in: remaining)
      {
         let plain = String(remaining[..<match.range.lowerBound])
         if !plain.isEmpty
         {
            result.append(NSAttributedString(string: plain, attributes: attributes))
         }
         let attachment = NSTextAttachment()
         guard let tile = tiles[match.entry.grapheme] else {return nil}
         attachment.image = NSImage(cgImage: tile, size: NSSize(width: font.pointSize, height: font.pointSize))
         let unit = font.pointSize / 1_000_000
         let width = CGFloat(match.entry.widthMillionths) * unit
         let height = CGFloat(match.entry.heightMillionths) * unit
         let top = CGFloat(match.entry.topFromBaselineMillionths) * unit
         attachment.bounds = CGRect(x: 0, y: -(top + height), width: width, height: height)
         result.append(NSAttributedString(attachment: attachment))
         let advance = CGFloat(match.entry.advanceMillionths) * unit
         if advance > width
         {
            let spacer = NSTextAttachment()
            spacer.bounds = CGRect(x: 0, y: 0, width: advance - width, height: 1)
            result.append(NSAttributedString(attachment: spacer))
         }
         remaining = remaining[match.range.upperBound...]
      }
      if !remaining.isEmpty
      {
         result.append(NSAttributedString(string: String(remaining), attributes: attributes))
      }
      cache[key] = result
      return result
   }

   func pieces(_ value: String, font: NSFont) -> [AppKitInlinePiece]?
   {
      guard contract.entries.contains(where: {value.contains($0.grapheme)}) else {return nil}
      let targetPixels = UInt32((font.pointSize * 3).rounded())
      guard let variant = contract.variants.min(by: {
         abs(Int64($0.emPixels) - Int64(targetPixels)) < abs(Int64($1.emPixels) - Int64(targetPixels))
      }), let tiles = images[variant.emPixels] else {return nil}
      var result = [AppKitInlinePiece]()
      var remaining = value[...]
      while let match = nextMatch(in: remaining)
      {
         let plain = String(remaining[..<match.range.lowerBound])
         if !plain.isEmpty
         {
            result.append(AppKitInlinePiece(text: plain, image: nil, advance: 0, width: 0, height: 0, topFromBaseline: 0))
         }
         guard let image = tiles[match.entry.grapheme] else {return nil}
         let unit = font.pointSize / 1_000_000
         result.append(AppKitInlinePiece(
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
         result.append(AppKitInlinePiece(text: String(remaining), image: nil, advance: 0, width: 0, height: 0, topFromBaseline: 0))
      }
      return result
   }

   func measure(_ value: String, font: NSFont) -> CGFloat
   {
      guard let pieces = pieces(value, font: font) else {return lineWidth(value, font: font)}
      return pieces.reduce(0) {$0 + ($1.image == nil ? lineWidth($1.text, font: font) : $1.advance)}
   }

   private func lineWidth(_ value: String, font: NSFont) -> CGFloat
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

private final class AppKitPreparedAssets
{
   let atlas: NSImage?
   let atlasTiles: [NSImage]
   let inlineText: AppKitPreparedInlineText?
   let sourceData: Data?
   let thumbnail: NSImage?
   private var retainedSource: NSImage?

   init(atlas: NSImage? = nil, atlasTiles: [NSImage] = [], inlineText: AppKitPreparedInlineText? = nil, sourceData: Data? = nil, thumbnail: NSImage? = nil)
   {
      self.atlas = atlas
      self.atlasTiles = atlasTiles
      self.inlineText = inlineText
      self.sourceData = sourceData
      self.thumbnail = thumbnail
   }

   func decodedSource() throws -> NSImage
   {
      if let retainedSource
      {
         return retainedSource
      }
      guard let sourceData, let image = NSImage(data: sourceData) else
      {
         throw AppKitScenarioAdapterFailure.invalidFixture
      }
      retainedSource = image
      return image
   }
}

private struct AppKitPreparedFonts
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

   func latin(size: CGFloat) -> NSFont
   {
      switch size
      {
      case 7: return latin7
      case 11: return latin11
      case 13: return latin13
      case 15: return latin15
      case 20: return latin20
      default: preconditionFailure("unsupported prepared benchmark font size")
      }
   }
}

final class AppKitScenarioAdapter: BenchmarkScenarioAdapter, BenchmarkPreviewCapture, BenchmarkRawAccessibilityCapture, BenchmarkFrameDrivenAdapter, BenchmarkQuiescenceAdapter, BenchmarkVirtualClockAdapter, BenchmarkDisplayLinkProvider
{
   private let window: NSWindow
   private lazy var canonicalSRGBEncoder = AppKitCanonicalSRGBEncoder()
   private var scenario: BenchmarkScenario?
   private var scene: AppKitSemanticScene?

   init(window: NSWindow)
   {
      self.window = window
   }

   func prepare(scenario: BenchmarkScenario, loader: BenchmarkSpecLoader) throws
   {
      scene?.teardown()
      let fonts = try AppKitPreparedFonts(catalog: loader.loadRegisteredFontCatalog(scenario.fontPack))
      let fixtureData = try loader.read(scenario.fixture)
      guard let fixture = try JSONSerialization.jsonObject(with: fixtureData) as? [String: Any] else
      {
         throw AppKitScenarioAdapterFailure.invalidFixture
      }
      let style = try decodedDictionary(scenario.scene.styleTokens, loader: loader)
      let layout = try decodedDictionary(scenario.scene.layoutAssertions, loader: loader)
      let assets = try loadAssets(scenario: scenario, fixtureData: fixtureData, loader: loader)
      let scene = try AppKitSemanticScene(
         scenario: scenario,
         fixture: fixture,
         fixtureData: fixtureData,
         style: style,
         layout: layout,
         assets: assets,
         fonts: fonts
      )
      self.scenario = scenario
      self.scene = scene
      window.contentView = scene.view
      window.setContentSize(NSSize(width: 390, height: 844))
      window.contentMinSize = NSSize(width: 390, height: 844)
      window.contentMaxSize = NSSize(width: 390, height: 844)
      window.makeKeyAndOrderFront(nil)
      scene.refreshAccessibility()
      scene.view.layoutSubtreeIfNeeded()
      scene.view.displayIfNeeded()
   }

   func reset() throws
   {
      guard let scene else
      {
         throw AppKitScenarioAdapterFailure.missingScenario
      }
      try scene.reset()
      try quiesce()
   }

   func apply(event: BenchmarkTraceEvent) throws
   {
      guard let scene else
      {
         throw AppKitScenarioAdapterFailure.missingScenario
      }
      try scene.apply(event: event)
   }

   func checkpoint(id: String) throws -> BenchmarkAdapterCheckpoint
   {
      guard let scenario, let scene else
      {
         throw AppKitScenarioAdapterFailure.missingScenario
      }
      return try benchmarkCheckpoint(
         scenario: scenario,
         checkpointID: id,
         model: scene.state,
         visibleRoleCounts: scene.roleCounts
      )
   }

   func teardown() throws
   {
      scene?.teardown()
      scene = nil
      scenario = nil
      window.contentView = NSView(frame: NSRect(x: 0, y: 0, width: 390, height: 844))
   }

   func quiesce() throws
   {
      guard let view = scene?.view else
      {
         throw AppKitScenarioAdapterFailure.missingScenario
      }
      view.layoutSubtreeIfNeeded()
      view.displayIfNeeded()
      if let layer = view.layer
      {
         appKitRemoveLayerTreeAnimations(layer)
         CATransaction.flush()
         guard !appKitLayerTreeHasAnimations(layer) else
         {
            throw AppKitScenarioAdapterFailure.activeAnimations
         }
      }
   }

   func displayTick() throws
   {
      guard let view = scene?.view else
      {
         throw AppKitScenarioAdapterFailure.missingScenario
      }
      view.layoutSubtreeIfNeeded()
      view.displayIfNeeded()
   }

   func setVirtualTimeUs(_ timeUs: UInt64) throws
   {
      guard let scene else
      {
         throw AppKitScenarioAdapterFailure.missingScenario
      }
      scene.setVirtualTimeUs(timeUs)
   }

   func makeDisplayLink(target: Any, selector: Selector) -> CADisplayLink
   {
      window.displayLink(target: target, selector: selector)
   }

   func previewPNG() throws -> Data
   {
      guard let view = scene?.view,
            let colorSpace = CGColorSpace(name: CGColorSpace.linearSRGB),
            let bitmapContext = CGContext(
               data: nil,
               width: 1_170,
               height: 2_532,
               bitsPerComponent: 32,
               bytesPerRow: 1_170 * 16,
               space: colorSpace,
               bitmapInfo: CGBitmapInfo.floatComponents.rawValue
                  | CGBitmapInfo.byteOrder32Little.rawValue
                  | CGImageAlphaInfo.noneSkipLast.rawValue
            ) else
      {
         throw AppKitScenarioAdapterFailure.missingScenario
      }
      bitmapContext.setShouldAntialias(true)
      bitmapContext.setAllowsAntialiasing(true)
      bitmapContext.setShouldSmoothFonts(false)
      bitmapContext.setAllowsFontSmoothing(false)
      bitmapContext.setShouldSubpixelPositionFonts(true)
      bitmapContext.setAllowsFontSubpixelPositioning(true)
      bitmapContext.setShouldSubpixelQuantizeFonts(true)
      bitmapContext.setAllowsFontSubpixelQuantization(true)
      bitmapContext.scaleBy(x: 3, y: 3)
      bitmapContext.translateBy(x: 0, y: 844)
      bitmapContext.scaleBy(x: 1, y: -1)
      let context = NSGraphicsContext(cgContext: bitmapContext, flipped: true)
      NSGraphicsContext.saveGraphicsState()
      NSGraphicsContext.current = context
      view.displayIgnoringOpacity(view.bounds, in: context)
      NSGraphicsContext.restoreGraphicsState()
      guard let data = canonicalSRGBEncoder?.encode(
         linearContext: bitmapContext,
         width: 1_170,
         height: 2_532
      ) else
      {
         throw AppKitScenarioAdapterFailure.missingScenario
      }
      return data
   }

   func rawAccessibilityTree() throws -> Data
   {
      guard let scene else
      {
         throw AppKitScenarioAdapterFailure.missingScenario
      }
      return try scene.rawAccessibilityTree()
   }

   private func decodedDictionary(_ identity: BenchmarkArtifactIdentity, loader: BenchmarkSpecLoader) throws -> [String: Any]
   {
      guard let dictionary = try JSONSerialization.jsonObject(with: loader.read(identity)) as? [String: Any] else
      {
         throw AppKitScenarioAdapterFailure.invalidFixture
      }
      return dictionary
   }

   private func loadAssets(scenario: BenchmarkScenario, fixtureData: Data, loader: BenchmarkSpecLoader) throws -> AppKitPreparedAssets
   {
      let manifest = try loader.loadAssetManifest(scenario.assets)
      let inlineText: AppKitPreparedInlineText?
      if let contract = manifest.inlineTextAtlas
      {
         var rasterImages = [UInt32: CGImage]()
         for variant in contract.variants
         {
            guard let artifact = manifest.artifacts.first(where: {$0.role == variant.artifactRole}),
                  let image = NSImage(data: try loader.read(artifact.artifact)),
                  let representation = image.representations.first,
                  representation.pixelsWide == Int(variant.pixelWidth),
                  representation.pixelsHigh == Int(variant.pixelHeight),
                  let cgImage = image.cgImage(forProposedRect: nil, context: nil, hints: nil) else
            {
               throw AppKitScenarioAdapterFailure.invalidFixture
            }
            rasterImages[variant.emPixels] = cgImage
         }
         inlineText = try AppKitPreparedInlineText(contract: contract, rasterImages: rasterImages)
      }
      else if manifest.inlineTextAtlas == nil
      {
         inlineText = nil
      }
      else
      {
         throw AppKitScenarioAdapterFailure.invalidFixture
      }
      if scenario.id == "image.decode-zoom"
      {
         let fixture = try appKitFixtureDecoder.decode(AppKitImageFixture.self, from: fixtureData)
         guard fixture.schemaVersion == 1,
               fixture.id == scenario.id,
               fixture.source.width == 4_096,
               fixture.source.height == 3_072,
               fixture.source.format == "png",
               fixture.source.colorSpace == "srgb",
               fixture.thumbnail.width == 384,
               fixture.thumbnail.height == 288,
               fixture.thumbnail.format == "png",
               fixture.thumbnail.colorSpace == "srgb",
               fixture.panDistanceMillionths == 450_000,
               fixture.pinchScaleMillionths == 2_000_000,
               let source = manifest.artifacts.first(where: {$0.role == "source-image"}),
               source.artifact == fixture.source.artifact,
               let thumbnail = manifest.artifacts.first(where: {$0.role == "thumbnail"}),
               thumbnail.artifact == fixture.thumbnail.artifact,
               let image = NSImage(data: try loader.read(thumbnail.artifact)) else
         {
            throw AppKitScenarioAdapterFailure.invalidFixture
         }
         return AppKitPreparedAssets(atlas: nil, sourceData: try loader.read(source.artifact), thumbnail: image)
      }
      guard let artifact = manifest.artifacts.first(where: {$0.role == "thumbnail-atlas"}),
            let image = NSImage(data: try loader.read(artifact.artifact)),
            let representation = image.representations.first,
            representation.pixelsWide == 384,
            representation.pixelsHigh == 192,
            let cgImage = image.cgImage(forProposedRect: nil, context: nil, hints: nil) else
      {
         throw AppKitScenarioAdapterFailure.invalidFixture
      }
      let tiles = try (0..<128).map
      {
         index -> NSImage in
         let source = CGRect(x: (index % 16) * 24, y: (index / 16) * 24, width: 24, height: 24)
         guard let tile = cgImage.cropping(to: source) else
         {
            throw AppKitScenarioAdapterFailure.invalidFixture
         }
         return NSImage(cgImage: tile, size: NSSize(width: 24, height: 24))
      }
      return AppKitPreparedAssets(atlas: image, atlasTiles: tiles, inlineText: inlineText)
   }
}

private final class AppKitSemanticScene
{
   private struct NavigationTransition
   {
      let target: CGFloat
      let startedAtUs: UInt64
   }

   let view: AppKitSceneView
   private let scenario: BenchmarkScenario
   private let fixture: [String: Any]
   private let fixtureData: Data
   private let assets: AppKitPreparedAssets
   private var startupForegroundCount = 0
   private var startupBackgroundCount = 0
   private var startupFreshInstallReady = false
   private var startupLifecycleStateID: String?
   private var dashboardLeafUpdates = 0
   private var dashboardBulkUpdates = 0
   private var dashboardLabels = [String]()
   private var dashboardInitialLabels = [String]()
   private var feedRows = [[String: Any]]()
   private var feedInitialRows = [[String: Any]]()
   private var feedPrependRows = [[String: Any]]()
   private var feedScrollMillionths: UInt32 = 0
   private var feedScrollOffset: CGFloat = 0
   private var feedFavoriteID: String?
   private var feedPrependCount = 0
   private var chatInitialMessages = [AppKitChatMessage]()
   private var chatMessages = [AppKitChatMessage]()
   private var chatPrependMessages = [AppKitChatMessage]()
   private var chatAppendTemplates = [AppKitChatMessage]()
   private var chatSelection: AppKitChatSelectionReplacement?
   private var chatSelectedMessageID: String?
   private var chatSelectedUTF8Range: Range<Int>?
   private var chatComposer = ""
   private var chatPrependCount = 0
   private var chatAppendCount = 0
   private var chatReplacementApplied = false
   private var navigationRoute = "list"
   private var navigationModalProgress: CGFloat = 0
   private var navigationTransition: NavigationTransition?
   private var navigationCycle = 0
   private var virtualTimeUs = UInt64(0)
   private var imageDecoded: NSImage?
   private var imageBytesReady = false
   private var imageDidDecode = false
   private var imageUploaded = false
   private var imageFirstVisible = false
   private var imagePointers = [UInt32: CGPoint]()
   private var imagePanOrigin: CGPoint?
   private var imagePanStart = CGPoint.zero
   private var imageTranslation = CGPoint.zero
   private var imagePinchStartDistance: CGFloat?
   private var imagePinchStartScale: CGFloat = 1
   private var imageScale: CGFloat = 1

   init(scenario: BenchmarkScenario, fixture: [String: Any], fixtureData: Data, style: [String: Any], layout: [String: Any], assets: AppKitPreparedAssets, fonts: AppKitPreparedFonts) throws
   {
      self.scenario = scenario
      self.fixture = fixture
      self.fixtureData = fixtureData
      self.assets = assets
      view = AppKitSceneView(frame: NSRect(x: 0, y: 0, width: 390, height: 844), fonts: fonts)
      try validateFixture()
      view.scene = self
      view.configure(scenarioID: scenario.id, fixture: fixture, style: style, layout: layout, assets: assets)
      try reset()
   }

   var state: [String: Any]
   {
      switch scenario.id
      {
      case "startup.first-screen":
         return [
            "foreground_count": startupForegroundCount,
            "background_count": startupBackgroundCount,
            "fresh_install_ready": startupFreshInstallReady,
            "scene_visible": !view.isHidden,
            "lifecycle_state_id": startupLifecycleStateID ?? NSNull(),
            "card_count": fixture["card_count"] as? Int ?? 0,
         ]
      case "dashboard.mixed-static":
         return [
            "leaf_update_count": dashboardLeafUpdates,
            "bulk_update_count": dashboardBulkUpdates,
            "visible_node_count": 301,
         ]
      case "feed.variable-scroll":
         return [
            "row_count": feedRows.count,
            "scroll_position_millionths": feedScrollMillionths,
            "favorite_id": feedFavoriteID ?? NSNull(),
            "prepend_count": feedPrependCount,
         ]
      case "chat.live-update":
         return [
            "message_count": chatMessages.count,
            "prepend_count": chatPrependCount,
            "append_count": chatAppendCount,
            "composer_utf8_count": chatComposer.utf8.count,
            "focused_message_id": chatSelectedMessageID ?? NSNull(),
            "selection_active": chatSelectedUTF8Range != nil,
            "replacement_applied": chatReplacementApplied,
         ]
      case "navigation.modal":
         return [
            "route": navigationRoute,
            "modal_visible": navigationModalIsVisible,
            "completed_cycles": navigationCycle,
         ]
      case "image.decode-zoom":
         return [
            "resource_stage": imageResourceStage,
            "pan_x_millionths": Int((imageTranslation.x / 390 * 1_000_000).rounded()),
            "pan_y_millionths": Int((imageTranslation.y / 844 * 1_000_000).rounded()),
            "scale_millionths": Int((imageScale * 1_000_000).rounded()),
            "active_pointer_count": imagePointers.count,
         ]
      default: return [:]
      }
   }

   var roleCounts: [BenchmarkRoleCount]
   {
      switch scenario.id
      {
      case "startup.first-screen":
         return roles([("header", 1), ("navigation", 1), ("card", 6), ("initial-image", 6), ("primary-control", 1)])
      case "dashboard.mixed-static":
         return roles([("dashboard", 1), ("label", 176), ("icon-image", 64), ("rounded-card", 32), ("control", 24), ("backdrop-region", 4)])
      case "feed.variable-scroll":
         let visible = visibleFeedRowCount
         return roles([("navigation-bar", 1), ("feed", 1), ("feed-card", visible), ("thumbnail", visible), ("favorite-control", visible)])
      case "chat.live-update":
         return roles([("chat-thread", 1), ("message", 10), ("avatar", 10), ("composer", 1), ("send-control", 1)])
      case "navigation.modal":
         if navigationModalIsVisible
         {
            return roles([("detail", 1), ("modal", 1), ("dismiss-control", 1), ("back-control", 1)])
         }
         return roles([("navigation-list", 1), ("list-item", 12)])
      case "image.decode-zoom":
         return roles([("image-canvas", 1), ("image", 1), ("zoom-control", 1)])
      default: return []
      }
   }

   func reset() throws
   {
      startupForegroundCount = 0
      startupBackgroundCount = 0
      startupFreshInstallReady = false
      startupLifecycleStateID = nil
      dashboardLeafUpdates = 0
      dashboardBulkUpdates = 0
      dashboardLabels = dashboardInitialLabels
      feedRows = feedInitialRows
      feedScrollMillionths = 0
      feedScrollOffset = 0
      feedFavoriteID = nil
      feedPrependCount = 0
      chatMessages = chatInitialMessages
      chatSelectedMessageID = nil
      chatSelectedUTF8Range = nil
      chatComposer = ""
      chatPrependCount = 0
      chatAppendCount = 0
      chatReplacementApplied = false
      navigationRoute = "list"
      navigationModalProgress = 0
      navigationTransition = nil
      navigationCycle = 0
      virtualTimeUs = 0
      imageDecoded = nil
      imageBytesReady = false
      imageDidDecode = false
      imageUploaded = false
      imageFirstVisible = false
      imagePointers.removeAll(keepingCapacity: true)
      imagePanOrigin = nil
      imagePanStart = .zero
      imageTranslation = .zero
      imagePinchStartDistance = nil
      imagePinchStartScale = 1
      imageScale = 1
      view.isHidden = false
      view.needsDisplay = true
      view.refreshAccessibility(scenarioID: scenario.id, model: state, roleCounts: roleCounts)
   }

   func apply(event: BenchmarkTraceEvent) throws
   {
      switch scenario.id
      {
      case "startup.first-screen": try applyStartup(event)
      case "dashboard.mixed-static": try applyDashboard(event)
      case "feed.variable-scroll": try applyFeed(event)
      case "chat.live-update": try applyChat(event)
      case "navigation.modal": try applyNavigation(event)
      case "image.decode-zoom": try applyImage(event)
      default: throw AppKitScenarioAdapterFailure.unsupportedScenario(scenario.id)
      }
      view.needsDisplay = true
      view.displayIfNeeded()
      view.refreshAccessibility(scenarioID: scenario.id, model: state, roleCounts: roleCounts)
   }

   func teardown()
   {
      if let layer = view.layer
      {
         appKitRemoveLayerTreeAnimations(layer)
      }
      imagePointers.removeAll(keepingCapacity: true)
      view.scene = nil
   }

   func draw(in bounds: CGRect)
   {
      switch scenario.id
      {
      case "startup.first-screen": view.drawStartup(in: bounds, visible: !view.isHidden)
      case "dashboard.mixed-static": view.drawDashboard(in: bounds, labels: dashboardLabels, bulkUpdates: dashboardBulkUpdates)
      case "feed.variable-scroll": view.drawFeed(in: bounds, rows: feedRows, scrollOffset: feedScrollOffset, favoriteID: feedFavoriteID)
      case "chat.live-update": view.drawChat(in: bounds, messages: chatMessages, composer: chatComposer)
      case "navigation.modal": view.drawNavigation(in: bounds, route: navigationRoute, modalProgress: navigationModalProgress)
      case "image.decode-zoom": view.drawImageScene(in: bounds, image: imageFirstVisible ? imageDecoded : assets.thumbnail, translation: imageTranslation, scale: imageScale)
      default: break
      }
   }

   private func validateFixture() throws
   {
      guard fixture["schema_version"] as? Int == 1, fixture["id"] as? String == scenario.id else
      {
         throw AppKitScenarioAdapterFailure.invalidFixture
      }
      switch scenario.id
      {
      case "startup.first-screen":
         let startup = try appKitFixtureDecoder.decode(AppKitStartupFixture.self, from: fixtureData)
         guard startup.dataSizeBytes == 24 * 1_024,
               startup.data.utf8.count == startup.dataSizeBytes,
               startup.cardCount == 24,
               startup.cards.count == 24,
               startup.initialImageIndices == [0, 1, 2, 3, 4, 5],
               startup.headerId == "startup:header",
               startup.navigationId == "startup:navigation",
               startup.controlId == "startup:primary-control",
               assets.atlas != nil else
         {
            throw AppKitScenarioAdapterFailure.invalidFixture
         }
         for (index, card) in startup.cards.enumerated()
         {
            guard card.id == String(format: "startup:card:%02d", index),
                  card.dataOffset == index * 1_024,
                  card.dataLength == 1_024,
                  card.thumbnailIndex == index % 6,
                  card.initiallyVisible == (index < 6) else
            {
               throw AppKitScenarioAdapterFailure.invalidFixture
            }
         }
      case "dashboard.mixed-static":
         guard fixture["visible_node_count"] as? Int == 300,
               fixture["shadow_count"] as? Int == 32,
               fixture["clipped_rounded_cards"] as? Int == 32,
               fixture["backdrop_blur_count"] as? Int == 4,
               assets.atlas != nil else
         {
            throw AppKitScenarioAdapterFailure.invalidFixture
         }
         dashboardInitialLabels.removeAll(keepingCapacity: true)
         for cardIndex in 0..<32
         {
            let labelCount = cardIndex < 16 ? 6 : 5
            for labelIndex in 0..<labelCount
            {
               dashboardInitialLabels.append("Node \(cardIndex * 6 + labelIndex)")
            }
         }
         dashboardLabels = dashboardInitialLabels
      case "feed.variable-scroll":
         guard let rows = fixture["rows"] as? [[String: Any]],
               rows.count == 2_000,
               let prependIDs = fixture["prepend_rows"] as? [String],
               prependIDs.count == 20,
               assets.atlas != nil else
         {
            throw AppKitScenarioAdapterFailure.invalidFixture
         }
         feedInitialRows = rows
         feedRows = rows
         feedPrependRows = prependIDs.enumerated().map
         {
            index, id in
            ["id": id, "height": 76, "text": "Prepended \(index)", "thumbnail_index": index] as [String: Any]
         }
      case "chat.live-update":
         let chat = try appKitFixtureDecoder.decode(AppKitChatFixture.self, from: fixtureData)
         guard chat.messageCount == 5_000,
               chat.messages.count == chat.messageCount,
               chat.avatarCount == 64,
               chat.prependMessages.count == 50,
               chat.appendRateHz == 10,
               chat.typedText.count == 100,
               chat.pastedText.utf8.count == 10 * 1_024,
               chat.selectionReplacement.messageId == "chat:append:16",
               chat.selectionReplacement.startUtf8 == 0,
               chat.selectionReplacement.endUtf8 == 6,
               chat.selectionReplacement.replacement == "Oxide",
               assets.atlas != nil else
         {
            throw AppKitScenarioAdapterFailure.invalidFixture
         }
         chatInitialMessages = chat.messages
         chatMessages = chat.messages
         chatPrependMessages = chat.prependMessages
         chatAppendTemplates = Array(chat.messages.prefix(8))
         chatSelection = chat.selectionReplacement
      case "navigation.modal":
         guard fixture["list_item_count"] as? Int == 12,
               fixture["cycle_count"] as? Int == 4,
               fixture["selected_item_id"] as? String == "navigation:item:05" else
         {
            throw AppKitScenarioAdapterFailure.invalidFixture
         }
      case "image.decode-zoom":
         let image = try appKitFixtureDecoder.decode(AppKitImageFixture.self, from: fixtureData)
         guard image.source.width == 4_096,
               image.source.height == 3_072,
               image.thumbnail.width == 384,
               image.thumbnail.height == 288,
               image.panDistanceMillionths == 450_000,
               image.pinchScaleMillionths == 2_000_000,
               assets.thumbnail != nil else
         {
            throw AppKitScenarioAdapterFailure.invalidFixture
         }
      default: throw AppKitScenarioAdapterFailure.unsupportedScenario(scenario.id)
      }
   }

   private func applyStartup(_ event: BenchmarkTraceEvent) throws
   {
      switch event.op
      {
      case "foreground":
         startupForegroundCount += 1
         view.isHidden = false
      case "background":
         startupBackgroundCount += 1
         view.isHidden = true
      case "resource-arrival":
         guard event.target == "fresh-install" else
         {
            throw AppKitScenarioAdapterFailure.unsupportedEvent(event.target ?? event.op)
         }
         startupFreshInstallReady = true
      default: throw AppKitScenarioAdapterFailure.unsupportedEvent(event.op)
      }
      startupLifecycleStateID = event.stateId
   }

   private func applyDashboard(_ event: BenchmarkTraceEvent) throws
   {
      guard event.op == "mutate", let target = event.target else
      {
         throw AppKitScenarioAdapterFailure.unsupportedEvent(event.op)
      }
      if target == "dashboard:update-count"
      {
         guard case .integer(let count)? = event.value, count == 30 else
         {
            throw AppKitScenarioAdapterFailure.unsupportedEvent(target)
         }
         dashboardBulkUpdates = Int(count)
      }
      else if target.hasPrefix("dashboard:label:")
      {
         guard let index = Int(target.dropFirst("dashboard:label:".count)), index < dashboardLabels.count else
         {
            throw AppKitScenarioAdapterFailure.unsupportedEvent(target)
         }
         dashboardLeafUpdates += 1
         dashboardLabels[index] = "Updated \(dashboardLeafUpdates)"
      }
      else
      {
         throw AppKitScenarioAdapterFailure.unsupportedEvent(target)
      }
   }

   private func applyFeed(_ event: BenchmarkTraceEvent) throws
   {
      switch event.op
      {
      case "pointer-down": break
      case "pointer-move", "pointer-up":
         guard let y = event.yMillionths else {return}
         feedScrollMillionths = UInt32(max(0, 1_000_000 - y))
         feedScrollOffset = max(0, feedContentHeight - 792) * CGFloat(feedScrollMillionths) / 1_000_000
      case "mutate":
         if event.target == "feed:prepend-count"
         {
            guard case .integer(let count)? = event.value, count == 20, feedPrependCount == 0 else
            {
               throw AppKitScenarioAdapterFailure.unsupportedEvent(event.target ?? event.op)
            }
            feedScrollOffset += feedPrependRows.reduce(0) {partial, row in partial + CGFloat(row["height"] as? Int ?? 76)}
            feedRows.insert(contentsOf: feedPrependRows, at: 0)
            feedPrependCount = feedPrependRows.count
         }
         else if let target = event.target, target.hasSuffix(":favorite")
         {
            feedFavoriteID = String(target.dropLast(":favorite".count))
         }
         else
         {
            throw AppKitScenarioAdapterFailure.unsupportedEvent(event.target ?? event.op)
         }
      default: throw AppKitScenarioAdapterFailure.unsupportedEvent(event.op)
      }
   }

   private func applyChat(_ event: BenchmarkTraceEvent) throws
   {
      switch event.op
      {
      case "mutate": try applyChatMutation(event)
      case "focus": try applyChatSelection(event)
      case "commit-text": try applyChatText(event)
      default: throw AppKitScenarioAdapterFailure.unsupportedEvent(event.op)
      }
   }

   private func applyChatMutation(_ event: BenchmarkTraceEvent) throws
   {
      if event.target == "chat:prepend-count"
      {
         guard case .integer(let count)? = event.value, count == 50, chatPrependCount == 0 else
         {
            throw AppKitScenarioAdapterFailure.unsupportedEvent(event.target ?? event.op)
         }
         chatMessages.insert(contentsOf: chatPrependMessages, at: 0)
         chatPrependCount = chatPrependMessages.count
         return
      }
      guard event.target == "chat:append",
            case .text(let id)? = event.value,
            let suffix = Int(id.split(separator: ":").last ?? ""),
            suffix == chatAppendCount,
            !chatAppendTemplates.isEmpty else
      {
         throw AppKitScenarioAdapterFailure.unsupportedEvent(event.target ?? event.op)
      }
      let template = chatAppendTemplates[suffix % chatAppendTemplates.count]
      chatMessages.append(AppKitChatMessage(
         id: id,
         sequence: 5_050 + suffix,
         authorIndex: suffix % 64,
         avatarIndex: suffix % 64,
         direction: template.direction,
         text: template.text
      ))
      chatAppendCount += 1
   }

   private func applyChatSelection(_ event: BenchmarkTraceEvent) throws
   {
      guard let selection = chatSelection,
            event.target == selection.messageId,
            case .text(let value)? = event.value else
      {
         throw AppKitScenarioAdapterFailure.unsupportedEvent(event.target ?? event.op)
      }
      let components = value.split(separator: ":")
      guard components.count == 2,
            let start = Int(components[0]),
            let end = Int(components[1]),
            start == selection.startUtf8,
            end == selection.endUtf8 else
      {
         throw AppKitScenarioAdapterFailure.unsupportedEvent(value)
      }
      chatSelectedMessageID = event.target
      chatSelectedUTF8Range = start..<end
   }

   private func applyChatText(_ event: BenchmarkTraceEvent) throws
   {
      guard case .text(let value)? = event.value else
      {
         throw AppKitScenarioAdapterFailure.unsupportedEvent(event.op)
      }
      if event.target == "chat:composer"
      {
         chatComposer.append(value)
         return
      }
      guard let selection = chatSelection,
            event.target == chatSelectedMessageID,
            event.target == selection.messageId,
            value == selection.replacement,
            let range = chatSelectedUTF8Range,
            let index = chatMessages.firstIndex(where: {$0.id == event.target}),
            let replaced = replacingUTF8(in: chatMessages[index].text, range: range, with: value) else
      {
         throw AppKitScenarioAdapterFailure.unsupportedEvent(event.target ?? event.op)
      }
      chatMessages[index].text = replaced
      chatSelectedUTF8Range = nil
      chatReplacementApplied = true
   }

   private func applyNavigation(_ event: BenchmarkTraceEvent) throws
   {
      if event.op == "pointer-down"
      {
         navigationRoute = "detail"
         return
      }
      if event.op == "pointer-move"
      {
         navigationModalProgress = CGFloat(event.xMillionths ?? 0) / 1_000_000
         navigationTransition = nil
         return
      }
      if event.op == "pointer-cancel"
      {
         navigationTransition = NavigationTransition(target: 0, startedAtUs: virtualTimeUs)
         return
      }
      guard event.op == "navigate", let target = event.target else
      {
         throw AppKitScenarioAdapterFailure.unsupportedEvent(event.op)
      }
      switch target
      {
      case "navigation:item:05": navigationRoute = "detail"
      case "navigation:modal": navigationTransition = NavigationTransition(target: 1, startedAtUs: virtualTimeUs)
      case "navigation:dismiss-control": navigationTransition = NavigationTransition(target: 0, startedAtUs: virtualTimeUs)
      case "navigation:back-control":
         navigationRoute = "list"
         navigationModalProgress = 0
         navigationTransition = nil
         navigationCycle += 1
      case "navigation:cancel":
         navigationRoute = "list"
         navigationModalProgress = 0
         navigationTransition = nil
      default: throw AppKitScenarioAdapterFailure.unsupportedEvent(target)
      }
   }

   func setVirtualTimeUs(_ timeUs: UInt64)
   {
      virtualTimeUs = timeUs
      guard let transition = navigationTransition else {return}
      let durationMs = (fixture["transition"] as? [String: Any])?["duration_ms"] as? Int ?? 300
      let elapsedUs = timeUs >= transition.startedAtUs ? timeUs - transition.startedAtUs : 0
      let linear = min(max(CGFloat(elapsedUs) / CGFloat(max(durationMs, 1) * 1_000), 0), 1)
      let eased = linear * linear * (3 - 2 * linear)
      navigationModalProgress = transition.target > 0 ? eased : 1 - eased
      if linear >= 1
      {
         navigationModalProgress = transition.target
         navigationTransition = nil
      }
      view.needsDisplay = true
      view.refreshAccessibility(scenarioID: scenario.id, model: state, roleCounts: roleCounts)
   }

   func rawAccessibilityTree() throws -> Data
   {
      view.refreshAccessibility(scenarioID: scenario.id, model: state, roleCounts: roleCounts)
      return try view.rawAccessibilityTree(scenarioID: scenario.id)
   }

   func refreshAccessibility()
   {
      view.refreshAccessibility(scenarioID: scenario.id, model: state, roleCounts: roleCounts)
   }

   private var navigationModalIsVisible: Bool
   {
      navigationModalProgress > 0 || navigationTransition?.target == 1
   }

   private func applyImage(_ event: BenchmarkTraceEvent) throws
   {
      if event.op == "resource-arrival"
      {
         try applyImageResource(event)
         return
      }
      guard let pointer = event.pointer,
            let x = event.xMillionths,
            let y = event.yMillionths else
      {
         throw AppKitScenarioAdapterFailure.unsupportedEvent(event.op)
      }
      let point = CGPoint(x: CGFloat(x) * 390 / 1_000_000, y: CGFloat(y) * 844 / 1_000_000 - 52)
      switch event.op
      {
      case "pointer-down": imagePointerDown(pointer, point: point)
      case "pointer-move": imagePointerMove(pointer, point: point)
      case "pointer-up":
         imagePointerMove(pointer, point: point)
         imagePointerEnded(pointer)
      case "pointer-cancel": imagePointerEnded(pointer)
      default: throw AppKitScenarioAdapterFailure.unsupportedEvent(event.op)
      }
   }

   private func applyImageResource(_ event: BenchmarkTraceEvent) throws
   {
      switch event.target
      {
      case "image:source-bytes": imageBytesReady = true
      case "image:decoded":
         imageDecoded = try assets.decodedSource()
         imageDidDecode = true
      case "image:texture":
         guard imageDecoded != nil else
         {
            throw AppKitScenarioAdapterFailure.unsupportedEvent(event.target ?? event.op)
         }
         imageUploaded = true
      case "image:presented":
         guard imageDecoded != nil, imageUploaded else
         {
            throw AppKitScenarioAdapterFailure.unsupportedEvent(event.target ?? event.op)
         }
         imageFirstVisible = true
      default: throw AppKitScenarioAdapterFailure.unsupportedEvent(event.target ?? event.op)
      }
   }

   private func imagePointerDown(_ pointer: UInt32, point: CGPoint)
   {
      imagePointers[pointer] = point
      if imagePointers.count == 1
      {
         imagePanOrigin = point
         imagePanStart = imageTranslation
      }
      else if imagePointers.count == 2
      {
         imagePinchStartDistance = imagePointerDistance
         imagePinchStartScale = imageScale
      }
   }

   private func imagePointerMove(_ pointer: UInt32, point: CGPoint)
   {
      guard imagePointers[pointer] != nil else {return}
      imagePointers[pointer] = point
      if imagePointers.count >= 2,
         let start = imagePinchStartDistance,
         start > 0,
         let distance = imagePointerDistance
      {
         imageScale = max(1, min(3, imagePinchStartScale * distance / start))
      }
      else if let origin = imagePanOrigin
      {
         imageTranslation = CGPoint(
            x: imagePanStart.x + point.x - origin.x,
            y: imagePanStart.y + point.y - origin.y
         )
      }
   }

   private func imagePointerEnded(_ pointer: UInt32)
   {
      imagePointers.removeValue(forKey: pointer)
      imagePinchStartDistance = nil
      if let remaining = imagePointers.values.first
      {
         imagePanOrigin = remaining
         imagePanStart = imageTranslation
      }
      else
      {
         imagePanOrigin = nil
      }
   }

   private var imagePointerDistance: CGFloat?
   {
      let points = Array(imagePointers.values.prefix(2))
      guard points.count == 2 else {return nil}
      return hypot(points[1].x - points[0].x, points[1].y - points[0].y)
   }

   private var imageResourceStage: String
   {
      if imageFirstVisible {return "visible"}
      if imageUploaded {return "uploaded"}
      if imageDidDecode {return "decoded"}
      if imageBytesReady {return "bytes-ready"}
      return "thumbnail"
   }

   private var visibleFeedRowCount: Int
   {
      guard !feedRows.isEmpty else {return 0}
      let heights = feedRows.map {CGFloat($0["height"] as? Int ?? 76)}
      let limit = feedScrollOffset + 792
      var cursor: CGFloat = 0
      var visible = 0
      for height in heights
      {
         if cursor + height > feedScrollOffset && cursor < limit
         {
            visible += 1
         }
         if cursor >= limit {break}
         cursor += height
      }
      return max(visible, 1)
   }

   private var feedContentHeight: CGFloat
   {
      feedRows.reduce(0) {partial, row in partial + CGFloat(row["height"] as? Int ?? 76)}
   }

   private func roles(_ values: [(String, Int)]) -> [BenchmarkRoleCount]
   {
      values.map {BenchmarkRoleCount(role: $0.0, count: UInt32($0.1))}
   }

   private func replacingUTF8(in text: String, range: Range<Int>, with replacement: String) -> String?
   {
      let bytes = Data(text.utf8)
      guard range.lowerBound >= 0, range.upperBound <= bytes.count else {return nil}
      var result = Data()
      result.reserveCapacity(bytes.count - range.count + replacement.utf8.count)
      result.append(bytes.prefix(range.lowerBound))
      result.append(contentsOf: replacement.utf8)
      result.append(bytes.suffix(from: range.upperBound))
      return String(data: result, encoding: .utf8)
   }
}

private final class AppKitSceneView: NSView
{
   weak var scene: AppKitSemanticScene?
   private var scenarioID = ""
   private var fixture = [String: Any]()
   private var style = [String: Any]()
   private var layout = [String: Any]()
   private var assets: AppKitPreparedAssets?
   private var platformAccessibilityChildren = [(semanticRole: String, element: NSAccessibilityElement)]()
   private var platformAccessibilityRoleCounts = [BenchmarkRoleCount]()
   private var platformAccessibilityMessageFocused = false
   private let fonts: AppKitPreparedFonts
   override var isFlipped: Bool {true}
   override var isOpaque: Bool {true}

   init(frame frameRect: NSRect, fonts: AppKitPreparedFonts)
   {
      self.fonts = fonts
      super.init(frame: frameRect)
   }

   required init?(coder: NSCoder)
   {
      fatalError("AppKitSceneView does not support coder initialization")
   }

   func configure(scenarioID: String, fixture: [String: Any], style: [String: Any], layout: [String: Any], assets: AppKitPreparedAssets)
   {
      self.scenarioID = scenarioID
      self.fixture = fixture
      self.style = style
      self.layout = layout
      self.assets = assets
      wantsLayer = true
      layer?.backgroundColor = appKitBackground.cgColor
   }

   override func draw(_ dirtyRect: NSRect)
   {
      super.draw(dirtyRect)
      appKitBackground.setFill()
      bounds.fill()
      scene?.draw(in: bounds)
   }

   func refreshAccessibility(scenarioID: String, model: [String: Any], roleCounts: [BenchmarkRoleCount])
   {
      guard let window else {return}
      let messageFocused = benchmarkSemanticRoleFocused(scenarioID: scenarioID, role: "message", model: model)
      guard roleCounts != platformAccessibilityRoleCounts || messageFocused != platformAccessibilityMessageFocused else {return}
      var children = [(semanticRole: String, element: NSAccessibilityElement)]()
      children.reserveCapacity(roleCounts.reduce(0) {$0 + Int($1.count)})
      for roleCount in roleCounts
      {
         let frame = benchmarkSemanticRoleFrame(scenarioID: scenarioID, role: roleCount.role)
         guard frame.count == 4 else {continue}
         let localFrame = CGRect(x: frame[0], y: frame[1], width: frame[2], height: frame[3])
         let screenFrame = window.convertToScreen(convert(localFrame, to: nil))
         for index in 0..<roleCount.count
         {
            let element = NSAccessibilityElement()
            element.setAccessibilityElement(true)
            element.setAccessibilityParent(self)
            element.setAccessibilityRole(appKitAccessibilityRole(roleCount.role))
            element.setAccessibilityLabel(roleCount.role)
            element.setAccessibilityValue(String(index))
            element.setAccessibilityFrame(screenFrame)
            element.setAccessibilityEnabled(true)
            element.setAccessibilityFocused(benchmarkSemanticRoleFocused(scenarioID: scenarioID, role: roleCount.role, model: model))
            children.append((semanticRole: roleCount.role, element: element))
         }
      }
      platformAccessibilityChildren = children
      platformAccessibilityRoleCounts = roleCounts
      platformAccessibilityMessageFocused = messageFocused
      setAccessibilityElement(false)
      setAccessibilityChildren(children.map(\.element))
   }

   func rawAccessibilityTree(scenarioID: String) throws -> Data
   {
      let nodes = platformAccessibilityChildren.enumerated().map
      {
         order, child -> [String: Any] in
         let frame = child.element.accessibilityFrame()
         return [
            "role": child.element.accessibilityRole()?.rawValue ?? "",
            "label": child.element.accessibilityLabel() ?? "",
            "value": String(describing: child.element.accessibilityValue() ?? ""),
            "enabled": child.element.isAccessibilityEnabled(),
            "focused": child.element.isAccessibilityFocused(),
            "declared_actions": benchmarkSemanticRoleActions(child.semanticRole),
            "frame": [frame.origin.x, frame.origin.y, frame.size.width, frame.size.height],
            "order": order,
         ]
      }
      return try JSONSerialization.data(withJSONObject: [
         "schema_version": 1,
         "platform": "appkit",
         "source": "NSAccessibilityElement",
         "scenario_id": scenarioID,
         "nodes": nodes,
      ], options: [.sortedKeys])
   }

   func drawStartup(in bounds: CGRect, visible: Bool)
   {
      guard visible else {return}
      text("Production Comparison", rect: CGRect(x: 16, y: 20, width: 358, height: 48), size: 20, color: appKitText)
      let navigation = CGRect(x: 16, y: 76, width: 358, height: 44)
      roundedRect(navigation, radius: 12, color: appKitSurface)
      text("First Screen", rect: navigation, size: 15, color: appKitSecondaryText)
      guard let cards = fixture["cards"] as? [[String: Any]], let data = fixture["data"] as? String else {return}
      NSGraphicsContext.current?.saveGraphicsState()
      NSBezierPath(rect: CGRect(x: 16, y: 132, width: 358, height: 552)).addClip()
      for (index, card) in cards.filter({$0["initially_visible"] as? Bool == true}).prefix(6).enumerated()
      {
         let column = CGFloat(index % 2)
         let row = CGFloat(index / 2)
         let rect = CGRect(x: 16 + column * 185, y: 132 + row * 188, width: 173, height: 176)
         roundedRect(rect.offsetBy(dx: 0, dy: 2), radius: 12, color: appKitShadow)
         roundedRect(rect, radius: 12, color: appKitSurface)
         drawAtlasTile(index: card["thumbnail_index"] as? Int ?? index, rect: CGRect(x: rect.minX + 12, y: rect.minY + 12, width: 48, height: 48), radius: 8)
         text(card["id"] as? String ?? "", rect: CGRect(x: rect.minX + 72, y: rect.minY + 14, width: rect.width - 84, height: 22), size: 15, color: appKitText)
         let offset = card["data_offset"] as? Int ?? 0
         let start = data.index(data.startIndex, offsetBy: min(offset, data.count))
         let end = data.index(start, offsetBy: min(48, data.distance(from: start, to: data.endIndex)))
         text(String(data[start..<end]), rect: CGRect(x: rect.minX + 72, y: rect.minY + 42, width: rect.width - 84, height: 54), size: 11, color: appKitSecondaryText)
      }
      NSGraphicsContext.current?.restoreGraphicsState()
      let control = CGRect(x: 16, y: 776, width: 358, height: 48)
      appKitAccent.setFill()
      control.fill()
      text("Continue", rect: CGRect(x: control.minX - 1.0 / 3.0, y: control.minY + 1.0 / 3.0, width: control.width, height: control.height), size: 15, color: appKitSurface, alignment: .center)
   }

   func drawDashboard(in bounds: CGRect, labels: [String], bulkUpdates: Int)
   {
      for index in 0..<4
      {
         appKitDashboardMaterial.setFill()
         CGRect(x: 16, y: 64 + CGFloat(index) * 204, width: 358, height: 96).fill()
      }
      for index in 0..<32
      {
         let column = CGFloat(index % 2)
         let row = CGFloat(index / 2)
         let width = (bounds.width - 44) / 2
         let rect = CGRect(x: 16 + column * (width + 12), y: 48 + row * 46, width: width, height: 38)
         roundedRect(rect.offsetBy(dx: 0, dy: 2), radius: 12, color: appKitShadow)
         roundedRect(rect, radius: 12, color: appKitSurface)
         drawAtlasTile(index: index * 2, rect: CGRect(x: rect.minX + 6, y: rect.minY + 6, width: 14, height: 14))
         drawAtlasTile(index: index * 2 + 1, rect: CGRect(x: rect.minX + 24, y: rect.minY + 6, width: 14, height: 14))
         let labelCount = index < 16 ? 6 : 5
         let labelBase = index < 16 ? index * 6 : 96 + (index - 16) * 5
         for labelIndex in 0..<labelCount
         {
            let label = labels[labelBase + labelIndex]
            let color = bulkUpdates > 0 && labelBase + labelIndex < bulkUpdates ? appKitAccent : labelIndex == 0 ? appKitText : appKitSecondaryText
            text(label, rect: CGRect(x: rect.minX + 42, y: rect.minY + 2 - 4.0 / 3.0 + CGFloat(labelIndex) * 6, width: width - 48, height: 6), font: fonts.latin7, color: color, centered: false)
         }
         if index < 24
         {
            appKitAccent.setFill()
            CGRect(x: rect.minX + 145, y: rect.minY + 13, width: 18, height: 12).fill()
         }
      }
   }

   func drawFeed(in bounds: CGRect, rows: [[String: Any]], scrollOffset: CGFloat, favoriteID: String?)
   {
      appKitSurface.setFill()
      CGRect(x: 0, y: 0, width: 390, height: 52).fill()
      text("Measured Feed", rect: CGRect(x: 16, y: 12, width: 220, height: 28), size: 20, color: appKitText)
      var cursor: CGFloat = 0
      for (index, row) in rows.enumerated()
      {
         let height = CGFloat(row["height"] as? Int ?? 76)
         let y = 52 + cursor - scrollOffset
         cursor += height
         if y + height <= 52 {continue}
         if y >= 844 {break}
         let card = CGRect(x: 12, y: y + 4, width: 366, height: height - 8)
         roundedRect(card.offsetBy(dx: 0, dy: 2), radius: 12, color: appKitShadow)
         roundedRect(card, radius: 12, color: appKitSurface)
         drawAtlasTile(index: row["thumbnail_index"] as? Int ?? index, rect: CGRect(x: card.minX + 10, y: card.minY + 10, width: 48, height: 48), radius: 8)
         let value = row["text"] as? String ?? ""
         text(value, rect: CGRect(x: card.minX + 70, y: card.minY + 8, width: card.width - 112, height: 24), font: font(for: value), color: appKitText)
         let id = row["id"] as? String ?? ""
         text(id, rect: CGRect(x: card.minX + 70, y: card.minY + 34, width: card.width - 112, height: 18), size: 11, color: appKitSecondaryText)
         roundedRect(CGRect(x: card.maxX - 34, y: card.minY + 11, width: 18, height: 18), radius: 9, color: favoriteID == id ? appKitAccent : appKitInactiveControl)
      }
   }

   func drawChat(in bounds: CGRect, messages: [AppKitChatMessage], composer: String)
   {
      text("Live Chat", rect: CGRect(x: 16, y: 8, width: 358, height: 36), size: 20, color: appKitText)
      var start = messages.count
      var tailHeight: CGFloat = 0
      while start > 0 && messages.count - start < 10 && tailHeight < 700
      {
         start -= 1
         tailHeight += chatRowHeight(messages[start])
      }
      var rowY = 52 + 700 - tailHeight
      for message in messages[start...]
      {
         let rowHeight = chatRowHeight(message)
         let rtl = message.direction == "rtl"
         let avatarX: CGFloat = rtl ? 342 : 12
         let bubbleX: CGFloat = rtl ? 12 : 60
         drawAtlasTile(index: message.avatarIndex, rect: CGRect(x: avatarX, y: rowY + 4, width: 36, height: 36), radius: 18)
         roundedRect(CGRect(x: bubbleX, y: rowY + 4, width: 286, height: rowHeight - 4), radius: 12, color: appKitShadow)
         roundedRect(CGRect(x: bubbleX, y: rowY + 2, width: 286, height: rowHeight - 4), radius: 12, color: appKitSurface)
         wrappedText(message.text, rect: CGRect(x: bubbleX + 10, y: rowY + 8, width: 266, height: rowHeight - 16), font: font(for: message.text), color: appKitText, alignment: rtl ? .right : .left)
         rowY += rowHeight
      }
      roundedRect(CGRect(x: 12, y: 780, width: 318, height: 48), radius: 12, color: appKitSurface)
      let visibleComposer = String(composer.suffix(80))
      text(visibleComposer, rect: CGRect(x: 22, y: 788, width: 298, height: 32), font: font(for: visibleComposer), color: appKitText)
      text("Send", rect: CGRect(x: 338, y: 780, width: 40, height: 48), size: 13, color: appKitAccent, alignment: .center)
   }

   func drawNavigation(in bounds: CGRect, route: String, modalProgress: CGFloat)
   {
      text("Navigation", rect: CGRect(x: -1.0 / 3.0, y: 6, width: 390, height: 52), size: 20, color: appKitText, alignment: .center)
      if route == "list"
      {
         let rowHeight = 212.0 / 3.0
         let list = CGRect(x: 20, y: 99, width: 350, height: rowHeight * 12)
         appKitSurface.setFill()
         list.fill()
         for index in 0..<12
         {
            let rowY = list.minY + CGFloat(index) * rowHeight
            text("Item \(index + 1)", rect: CGRect(x: list.minX + 16, y: rowY + 10 + 11.0 / 3.0, width: 260, height: 24), size: 15, color: appKitText)
            text("Canonical navigation row", rect: CGRect(x: list.minX + 16, y: rowY + 34 + 10.0 / 3.0, width: 260, height: 20), size: 11, color: appKitSecondaryText)
            text("›", rect: CGRect(x: list.maxX - 34 + 5.0 / 3.0, y: rowY + 18 + 10.0 / 3.0, width: 20, height: 28), size: 15, color: appKitInactiveControl, alignment: .center)
            if index + 1 < 12
            {
               appKitSeparator.setFill()
               CGRect(x: list.minX + 16, y: rowY + rowHeight - 1, width: list.width - 32, height: 1).fill()
            }
         }
      }
      else
      {
         text("Detail", rect: CGRect(x: 16, y: 64, width: 220, height: 28), font: fonts.latin20, color: appKitText, centered: false)
      }
      if modalProgress > 0
      {
         appKitModalOverlay.withAlphaComponent(0.69 * modalProgress).setFill()
         bounds.fill()
         let offset = (1 - modalProgress) * 390
         let modal = CGRect(x: 24 + offset, y: 132, width: 342, height: 580)
         roundedRect(modal, radius: 16, color: appKitSurface)
         text("Modal", rect: CGRect(x: modal.minX + 18, y: modal.minY + 18, width: 220, height: 28), font: fonts.latin20, color: appKitText, centered: false)
      }
   }

   func drawImageScene(in bounds: CGRect, image: NSImage?, translation: CGPoint, scale: CGFloat)
   {
      text("Decode & Zoom", rect: CGRect(x: 18, y: 8, width: 204, height: 36), size: 20, color: appKitText)
      let canvas = CGRect(x: 0, y: 52, width: 390, height: 740)
      NSGraphicsContext.current?.saveGraphicsState()
      NSBezierPath(rect: canvas).addClip()
      if let image
      {
         let rect: CGRect
         if scale == 1 && translation == .zero && image.size.width <= 384
         {
            rect = CGRect(x: 16, y: 68, width: 128, height: 96)
         }
         else
         {
            let width = 4_096 / (3 * 2) * scale
            let height = 3_072 / (3 * 2) * scale
            let originX = canvas.minX + (canvas.width - width) / 2 + translation.x
            let originY = canvas.minY + (canvas.height - height) / 2 + translation.y * canvas.height / 844
            rect = CGRect(
               x: exactImageSamplingOrigin(originX, imageScale: scale),
               y: exactImageSamplingOrigin(originY, imageScale: scale),
               width: width,
               height: height
            )
         }
         image.draw(in: rect, from: CGRect(origin: .zero, size: image.size), operation: .sourceOver, fraction: 1, respectFlipped: true, hints: [.interpolation: NSImageInterpolation.low])
         if image.size.width > 384
         {
            appKitBackground.setFill()
            let borderHeight: CGFloat = 1 / 3
            let top = (rect.minY * 3).rounded(.down) / 3
            let bottom = (rect.maxY * 3).rounded(.down) / 3
            CGRect(x: canvas.minX, y: top, width: canvas.width, height: borderHeight).fill()
            CGRect(x: canvas.minX, y: bottom, width: canvas.width, height: borderHeight).fill()
         }
      }
      NSGraphicsContext.current?.restoreGraphicsState()
      appKitDisabledSliderTrack.setFill()
      CGRect(x: 16, y: 811, width: 358, height: 6).fill()
   }

   private func exactImageSamplingOrigin(_ value: CGFloat, imageScale: CGFloat) -> CGFloat
   {
      let phase = max(0, min(1, 2 - imageScale)) * 0.25
      return ((value * 3 - phase).rounded(.up) + phase) / 3
   }

   private func drawAtlasTile(index: Int, rect: CGRect, radius: CGFloat = 0)
   {
      guard let tiles = assets?.atlasTiles, !tiles.isEmpty else {return}
      NSGraphicsContext.current?.saveGraphicsState()
      if radius > 0
      {
         NSBezierPath(roundedRect: rect, xRadius: radius, yRadius: radius).addClip()
      }
      tiles[index % tiles.count].draw(in: rect, from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: [.interpolation: NSImageInterpolation.low])
      NSGraphicsContext.current?.restoreGraphicsState()
   }

   private func chatRowHeight(_ message: AppKitChatMessage) -> CGFloat
   {
      let heights: [CGFloat] = [58, 62, 66, 70, 74, 78, 82, 76, 70, 64]
      return heights[message.sequence % heights.count]
   }

   private func roundedRect(_ rect: CGRect, radius: CGFloat, color: NSColor)
   {
      color.setFill()
      NSBezierPath(roundedRect: rect, xRadius: radius, yRadius: radius).fill()
   }

   private func text(_ value: String, rect: CGRect, size: CGFloat, color: NSColor, alignment: NSTextAlignment = .left)
   {
      text(value, rect: rect, font: fonts.latin(size: size), color: color, alignment: alignment)
   }

   private func text(_ value: String, rect: CGRect, font: NSFont, color: NSColor, alignment: NSTextAlignment = .left, centered: Bool = true)
   {
      let drawRect: CGRect
      if centered
      {
         let lineHeight = font.ascender - font.descender + font.leading
         let baseline = ((rect.minY + (rect.height - lineHeight) / 2 + font.ascender) * 3).rounded() / 3
         drawRect = CGRect(x: rect.minX, y: baseline - font.ascender, width: rect.width, height: rect.height)
      }
      else
      {
         drawRect = rect
      }
      let attributed = assets?.inlineText?.attributedText(value, font: font, color: color, alignment: alignment)
      if let attributed
      {
         attributed.draw(in: drawRect)
         return
      }
      if (centered || rect.height <= font.pointSize * 2), drawCoreText(value, rect: rect, font: font, color: color, alignment: alignment, centered: centered)
      {
         return
      }
      let paragraph = NSMutableParagraphStyle()
      paragraph.alignment = alignment
      value.draw(in: drawRect, withAttributes: [.font: font, .foregroundColor: color, .paragraphStyle: paragraph])
   }

   private func drawCoreText(_ value: String, rect: CGRect, font: NSFont, color: NSColor, alignment: NSTextAlignment, centered: Bool) -> Bool
   {
      guard let context = NSGraphicsContext.current?.cgContext else {return false}
      let line = CTLineCreateWithAttributedString(NSAttributedString(
         string: value,
         attributes: [kCTFontAttributeName as NSAttributedString.Key: font]
      ))
      let width = CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil))
      let originX: CGFloat
      switch alignment
      {
      case .center: originX = rect.minX + (rect.width - width) / 2
      case .right: originX = rect.maxX - width
      default: originX = rect.minX
      }
      let quantizedOriginX = (originX * 3).rounded() / 3
      let lineHeight = font.ascender - font.descender + font.leading
      let baseline = centered
         ? ((rect.minY + (rect.height - lineHeight) / 2 + font.ascender) * 3).rounded() / 3
         : ((rect.minY + font.pointSize) * 3).rounded() / 3
      context.saveGState()
      context.clip(to: CGRect(x: rect.minX, y: 0, width: rect.width, height: 844))
      context.setFillColor(color.cgColor)
      context.setTextDrawingMode(.fill)
      context.textMatrix = CGAffineTransform(scaleX: 1, y: -1)
      draw(line: line, originX: quantizedOriginX, baseline: -baseline, context: context)
      context.restoreGState()
      return true
   }

   private func wrappedText(_ value: String, rect: CGRect, font: NSFont, color: NSColor, alignment: NSTextAlignment)
   {
      let words = value.split(whereSeparator: {$0.isWhitespace})
      var lines = [String]()
      var current = ""
      for word in words
      {
         let candidate = current.isEmpty ? String(word) : "\(current) \(word)"
         let width = assets?.inlineText?.measure(candidate, font: font) ?? coreTextWidth(candidate, font: font)
         if !current.isEmpty && width > rect.width
         {
            lines.append(current)
            current = String(word)
         }
         else
         {
            current = candidate
         }
      }
      if !current.isEmpty {lines.append(current)}
      let visible = Array(lines.prefix(2))
      let lineHeight = 61.0 / 3.0
      let centerOffset = visible.count > 1 ? lineHeight * 0.5 : 0
      for (index, line) in visible.enumerated()
      {
         let lineRect = CGRect(x: rect.minX, y: rect.minY - centerOffset + CGFloat(index) * lineHeight, width: rect.width, height: rect.height)
         if let inline = assets?.inlineText, let pieces = inline.pieces(line, font: font)
         {
            let lineWidth = inline.measure(line, font: font)
            var x = alignment == .right ? lineRect.maxX - lineWidth : lineRect.minX
            let baseline = centeredBaseline(rect: lineRect, font: font)
            for piece in pieces
            {
               if let image = piece.image
               {
                  let imageX = (x * 3).rounded() / 3
                  let imageY = ((baseline + piece.topFromBaseline) * 3).rounded() / 3
                  NSImage(cgImage: image, size: NSSize(width: piece.width, height: piece.height)).draw(
                     in: CGRect(x: imageX, y: imageY, width: piece.width, height: piece.height),
                     from: .zero,
                     operation: .sourceOver,
                     fraction: 1,
                     respectFlipped: true,
                     hints: [.interpolation: NSImageInterpolation.low]
                  )
                  x += piece.advance
               }
               else
               {
                  let width = coreTextWidth(piece.text, font: font)
                  _ = drawCoreText(piece.text, rect: CGRect(x: x, y: lineRect.minY, width: width, height: lineRect.height), font: font, color: color, alignment: .left, centered: true)
                  x += width
               }
            }
         }
         else
         {
            _ = drawCoreText(line, rect: lineRect, font: font, color: color, alignment: alignment, centered: true)
         }
      }
   }

   private func coreTextWidth(_ value: String, font: NSFont) -> CGFloat
   {
      let line = CTLineCreateWithAttributedString(NSAttributedString(
         string: value,
         attributes: [kCTFontAttributeName as NSAttributedString.Key: font]
      ))
      return CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil))
   }

   private func centeredBaseline(rect: CGRect, font: NSFont) -> CGFloat
   {
      let lineHeight = font.ascender - font.descender + font.leading
      return ((rect.minY + (rect.height - lineHeight) / 2 + font.ascender) * 3).rounded() / 3
   }

   private func draw(line: CTLine, originX: CGFloat, baseline: CGFloat, context: CGContext)
   {
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
            positions[index].y += baseline
         }
         CTFontDrawGlyphs(runFont, glyphs, positions, count, context)
      }
   }

   private func font(for value: String) -> NSFont
   {
      if value.unicodeScalars.contains(where: {(0x4E00...0x9FFF).contains(Int($0.value))})
      {
         return fonts.cjk15
      }
      if value.unicodeScalars.contains(where: {(0x0600...0x06FF).contains(Int($0.value))})
      {
         return fonts.arabic15
      }
      return fonts.latin15
   }
}

private extension NSShadow
{
   func apply(color: NSColor, offset: NSSize, blur: CGFloat)
   {
      shadowColor = color
      shadowOffset = offset
      shadowBlurRadius = blur
      set()
   }
}
