import CoreGraphics
import CoreText
import CryptoKit
import Foundation

@_silgen_name("CGBitmapContextCreate")
private func createAlphaOnlyBitmapContext(
   _ data: UnsafeMutableRawPointer?,
   _ width: Int,
   _ height: Int,
   _ bitsPerComponent: Int,
   _ bytesPerRow: Int,
   _ colorSpace: CGColorSpace?,
   _ bitmapInfo: UInt32
) -> CGContext?

private enum DiagnosticFailure: Error, CustomStringConvertible
{
   case invalid(String)
   case unavailable(String)

   var description: String
   {
      switch self
      {
      case .invalid(let message): return message
      case .unavailable(let message): return message
      }
   }
}

private struct FontVariation: Decodable
{
   let tag: String
   let valueMillionths: Int64
}

private struct KnownSignature: Decodable
{
   let x: Int
   let y: Int
   let glyph: String
   let expectedNativeRgb: [UInt8]
   let expectedOxideRgb: [UInt8]
}

private struct TitleCase: Decodable
{
   let id: String
   let text: String
   let frameMillionths: [Int64]
   let alignment: String
   let signatures: [KnownSignature]
}

private struct DiagnosticRequest: Decodable
{
   let schemaVersion: UInt32
   let canvasWidth: Int
   let canvasHeight: Int
   let deviceScale: Int
   let pointSizeMillionths: UInt32
   let backgroundSrgb: [UInt8]
   let textSrgb: [UInt8]
   let fontSha256: String
   let variations: [FontVariation]
   let cases: [TitleCase]
}

private struct GlyphRun
{
   let font: CTFont
   let glyphs: [CGGlyph]
   let positions: [CGPoint]
}

private struct PreparedTitle
{
   let originX: CGFloat
   let baseline: CGFloat
   let runs: [GlyphRun]
}

private struct FloatSurface
{
   let width: Int
   let height: Int
   var pixels: [Float]
}

private func sha256(_ data: Data) -> String
{
   SHA256.hash(data: data).map {String(format: "%02x", $0)}.joined()
}

private func srgbToLinear(_ value: UInt8) -> Float
{
   let encoded = Float(value) / 255
   return encoded <= 0.04045 ? encoded / 12.92 : pow((encoded + 0.055) / 1.055, 2.4)
}

private func linearToSrgb(_ value: Float) -> UInt8
{
   let linear = max(0, min(1, value))
   let encoded = linear <= 0.0031308 ? linear * 12.92 : 1.055 * pow(linear, 1 / 2.4) - 0.055
   return UInt8(max(0, min(255, Int((encoded * 255).rounded()))))
}

private func rgbData(_ surface: FloatSurface) -> Data
{
   var rgb = Data(capacity: surface.width * surface.height * 3)
   for pixel in stride(from: 0, to: surface.pixels.count, by: 4)
   {
      rgb.append(linearToSrgb(surface.pixels[pixel]))
      rgb.append(linearToSrgb(surface.pixels[pixel + 1]))
      rgb.append(linearToSrgb(surface.pixels[pixel + 2]))
   }
   return rgb
}

private func color(_ values: [UInt8]) throws -> CGColor
{
   guard values.count == 3,
         let space = CGColorSpace(name: CGColorSpace.sRGB),
         let color = CGColor(colorSpace: space, components: values.map {CGFloat($0) / 255} + [1]) else
   {
      throw DiagnosticFailure.invalid("RGB color contract is invalid")
   }
   return color
}

private func makeFloatSurface(width: Int, height: Int, background: CGColor, draw: (CGContext) throws -> Void) throws -> FloatSurface
{
   guard let space = CGColorSpace(name: CGColorSpace.linearSRGB) else
   {
      throw DiagnosticFailure.unavailable("linear sRGB is unavailable")
   }
   var surface = FloatSurface(width: width, height: height, pixels: [Float](repeating: 0, count: width * height * 4))
   try surface.pixels.withUnsafeMutableBytes
   {
      guard let context = CGContext(
         data: $0.baseAddress,
         width: width,
         height: height,
         bitsPerComponent: 32,
         bytesPerRow: width * 16,
         space: space,
         bitmapInfo: CGBitmapInfo.floatComponents.rawValue
            | CGBitmapInfo.byteOrder32Little.rawValue
            | CGImageAlphaInfo.noneSkipLast.rawValue
      ) else
      {
         throw DiagnosticFailure.unavailable("float linear context is unavailable")
      }
      context.setFillColor(background)
      context.fill(CGRect(x: 0, y: 0, width: width, height: height))
      try draw(context)
   }
   return surface
}

private func configureText(_ context: CGContext)
{
   context.setTextDrawingMode(.fill)
   context.setShouldAntialias(true)
   context.setAllowsAntialiasing(true)
   context.setShouldSmoothFonts(false)
   context.setAllowsFontSmoothing(false)
   context.setShouldSubpixelPositionFonts(true)
   context.setAllowsFontSubpixelPositioning(true)
   context.setShouldSubpixelQuantizeFonts(true)
   context.setAllowsFontSubpixelQuantization(true)
}

private func variationIdentifier(_ tag: String) throws -> NSNumber
{
   guard tag.utf8.count == 4 else {throw DiagnosticFailure.invalid("font variation tag must contain four bytes")}
   let value = tag.utf8.reduce(UInt32(0)) {(value, byte) in value << 8 | UInt32(byte)}
   return NSNumber(value: value)
}

private func loadFont(path: String, request: DiagnosticRequest) throws -> CTFont
{
   let url = URL(fileURLWithPath: path)
   var registrationError: Unmanaged<CFError>?
   if !CTFontManagerRegisterFontsForURL(url as CFURL, .process, &registrationError)
   {
      let error = registrationError?.takeRetainedValue() as Error?
      guard (error as NSError?)?.code == CTFontManagerError.alreadyRegistered.rawValue else
      {
         throw DiagnosticFailure.invalid("font registration failed")
      }
   }
   guard let descriptors = CTFontManagerCreateFontDescriptorsFromURL(url as CFURL) as? [CTFontDescriptor],
         let descriptor = descriptors.first else
   {
      throw DiagnosticFailure.invalid("font descriptor is unavailable")
   }
   var variations = [NSNumber: NSNumber]()
   for variation in request.variations
   {
      variations[try variationIdentifier(variation.tag)] = NSNumber(value: Double(variation.valueMillionths) / 1_000_000)
   }
   let varied = CTFontDescriptorCreateCopyWithAttributes(
      descriptor,
      [kCTFontVariationAttribute as String: variations] as CFDictionary
   )
   return CTFontCreateWithFontDescriptor(varied, CGFloat(request.pointSizeMillionths) / 1_000_000, nil)
}

private func prepare(_ value: TitleCase, font: CTFont, scale: CGFloat) throws -> PreparedTitle
{
   guard value.frameMillionths.count == 4 else {throw DiagnosticFailure.invalid("title frame must contain four values")}
   let frame = value.frameMillionths.map {CGFloat($0) / 1_000_000}
   let line = CTLineCreateWithAttributedString(NSAttributedString(
      string: value.text,
      attributes: [kCTFontAttributeName as NSAttributedString.Key: font]
   ))
   let width = CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil))
   let unquantizedX = value.alignment == "center" ? frame[0] + (frame[2] - width) / 2 : frame[0]
   let originX = (unquantizedX * scale).rounded() / scale
   let lineHeight = CTFontGetAscent(font) + CTFontGetDescent(font) + CTFontGetLeading(font)
   let baseline = ((frame[1] + (frame[3] - lineHeight) / 2 + CTFontGetAscent(font)) * scale).rounded() / scale
   var runs = [GlyphRun]()
   for case let run as CTRun in CTLineGetGlyphRuns(line) as NSArray
   {
      let count = CTRunGetGlyphCount(run)
      guard count > 0,
            let fontAttribute = (CTRunGetAttributes(run) as NSDictionary)[kCTFontAttributeName] else {continue}
      var glyphs = [CGGlyph](repeating: 0, count: count)
      var positions = [CGPoint](repeating: .zero, count: count)
      CTRunGetGlyphs(run, CFRange(), &glyphs)
      CTRunGetPositions(run, CFRange(), &positions)
      runs.append(GlyphRun(font: fontAttribute as! CTFont, glyphs: glyphs, positions: positions))
   }
   return PreparedTitle(originX: originX, baseline: baseline, runs: runs)
}

private func drawRuns(_ prepared: PreparedTitle, context: CGContext, originY: CGFloat)
{
   for run in prepared.runs
   {
      var positions = run.positions
      for index in positions.indices
      {
         positions[index].x += prepared.originX
         positions[index].y += originY
      }
      CTFontDrawGlyphs(run.font, run.glyphs, positions, run.glyphs.count, context)
   }
}

private func renderDirect(_ prepared: PreparedTitle, request: DiagnosticRequest) throws -> Data
{
   let background = try color(request.backgroundSrgb)
   let foreground = try color(request.textSrgb)
   let scale = CGFloat(request.deviceScale)
   let logicalHeight = CGFloat(request.canvasHeight) / scale
   let surface = try makeFloatSurface(width: request.canvasWidth, height: request.canvasHeight, background: background)
   {
      context in
      configureText(context)
      context.setFillColor(foreground)
      context.scaleBy(x: scale, y: scale)
      context.translateBy(x: 0, y: logicalHeight)
      context.scaleBy(x: 1, y: -1)
      context.textMatrix = CGAffineTransform(scaleX: 1, y: -1)
      drawRuns(prepared, context: context, originY: -prepared.baseline)
   }
   return rgbData(surface)
}

private func renderA8(_ prepared: PreparedTitle, request: DiagnosticRequest) throws -> Data
{
   let width = request.canvasWidth
   let height = request.canvasHeight
   var bottomUp = [UInt8](repeating: 0, count: width * height)
   try bottomUp.withUnsafeMutableBytes
   {
      guard let context = createAlphaOnlyBitmapContext(
         $0.baseAddress,
         width,
         height,
         8,
         width,
         nil,
         CGImageAlphaInfo.alphaOnly.rawValue
      ) else
      {
         throw DiagnosticFailure.unavailable("A8 context is unavailable")
      }
      configureText(context)
      context.setFillColor(gray: 1, alpha: 1)
      let scale = CGFloat(request.deviceScale)
      context.scaleBy(x: scale, y: scale)
      context.textMatrix = .identity
      let logicalHeight = CGFloat(height) / scale
      drawRuns(prepared, context: context, originY: logicalHeight - prepared.baseline)
   }
   return Data(bottomUp)
}

private func renderMaskFill(_ a8: Data, request: DiagnosticRequest) throws -> Data
{
   let width = request.canvasWidth
   let height = request.canvasHeight
   guard a8.count == width * height,
         let provider = CGDataProvider(data: a8 as CFData),
         let gray = CGColorSpace(name: CGColorSpace.genericGrayGamma2_2),
         let mask = CGImage(
            width: width,
            height: height,
            bitsPerComponent: 8,
            bitsPerPixel: 8,
            bytesPerRow: width,
            space: gray,
            bitmapInfo: [],
            provider: provider,
            decode: nil,
            shouldInterpolate: false,
            intent: .defaultIntent
         ) else
   {
      throw DiagnosticFailure.unavailable("A8 mask image is unavailable")
   }
   let background = try color(request.backgroundSrgb)
   let foreground = try color(request.textSrgb)
   let surface = try makeFloatSurface(width: width, height: height, background: background)
   {
      context in
      context.clip(to: CGRect(x: 0, y: 0, width: width, height: height), mask: mask)
      context.setFillColor(foreground)
      context.fill(CGRect(x: 0, y: 0, width: width, height: height))
   }
   return rgbData(surface)
}

private func renderMetalCPU(_ a8: Data, request: DiagnosticRequest) throws -> Data
{
   guard a8.count == request.canvasWidth * request.canvasHeight else
   {
      throw DiagnosticFailure.invalid("A8 byte count differs from the canvas")
   }
   let background = request.backgroundSrgb.map(srgbToLinear)
   let foreground = request.textSrgb.map(srgbToLinear)
   var rgb = Data(capacity: a8.count * 3)
   for coverage in a8
   {
      let alpha = Float(coverage) / 255
      for channel in 0..<3
      {
         rgb.append(linearToSrgb(foreground[channel] * alpha + background[channel] * (1 - alpha)))
      }
   }
   return rgb
}

private func pixel(_ rgb: Data, width: Int, x: Int, y: Int) throws -> [UInt8]
{
   guard x >= 0, y >= 0, x < width else {throw DiagnosticFailure.invalid("signature coordinate is outside the canvas")}
   let offset = (y * width + x) * 3
   guard offset + 3 <= rgb.count else {throw DiagnosticFailure.invalid("signature coordinate is outside RGB data")}
   return Array(rgb[offset..<offset + 3])
}

private func difference(_ first: Data, _ second: Data) throws -> [String: Any]
{
   guard first.count == second.count, first.count % 3 == 0 else {throw DiagnosticFailure.invalid("RGB comparison shapes differ")}
   var pixels = 0
   var channels = 0
   var maximum = 0
   for index in stride(from: 0, to: first.count, by: 3)
   {
      var pixelDiffers = false
      for channel in 0..<3
      {
         let delta = abs(Int(first[index + channel]) - Int(second[index + channel]))
         if delta > 0
         {
            pixelDiffers = true
            channels += 1
            maximum = max(maximum, delta)
         }
      }
      pixels += pixelDiffers ? 1 : 0
   }
   return ["differing_pixel_count": pixels, "differing_channel_count": channels, "maximum_channel_delta": maximum]
}

private func execute() throws
{
   guard CommandLine.arguments.count == 5 else
   {
      throw DiagnosticFailure.invalid("usage: TextCompositingReference.swift <font> <request> <output-directory> <report>")
   }
   let fontPath = CommandLine.arguments[1]
   let requestPath = CommandLine.arguments[2]
   let output = URL(fileURLWithPath: CommandLine.arguments[3], isDirectory: true)
   let reportPath = CommandLine.arguments[4]
   let decoder = JSONDecoder()
   decoder.keyDecodingStrategy = .convertFromSnakeCase
   let request = try decoder.decode(DiagnosticRequest.self, from: Data(contentsOf: URL(fileURLWithPath: requestPath)))
   let fontData = try Data(contentsOf: URL(fileURLWithPath: fontPath))
   guard request.schemaVersion == 1, sha256(fontData) == request.fontSha256 else
   {
      throw DiagnosticFailure.invalid("request or font identity is invalid")
   }
   let font = try loadFont(path: fontPath, request: request)
   var caseReports = [[String: Any]]()
   var signatureCount = 0
   var directNativeMatches = 0
   var maskNativeMatches = 0
   var metalOxideMatches = 0
   for title in request.cases
   {
      let prepared = try prepare(title, font: font, scale: CGFloat(request.deviceScale))
      let direct = try renderDirect(prepared, request: request)
      let a8 = try renderA8(prepared, request: request)
      let maskFill = try renderMaskFill(a8, request: request)
      let metalCPU = try renderMetalCPU(a8, request: request)
      let a8Name = "\(title.id).a8"
      let directName = "\(title.id).direct.rgb"
      let maskName = "\(title.id).mask-fill.rgb"
      let metalName = "\(title.id).metal-cpu.rgb"
      try a8.write(to: output.appendingPathComponent(a8Name))
      try direct.write(to: output.appendingPathComponent(directName))
      try maskFill.write(to: output.appendingPathComponent(maskName))
      try metalCPU.write(to: output.appendingPathComponent(metalName))
      var signatures = [[String: Any]]()
      for signature in title.signatures
      {
         let directPixel = try pixel(direct, width: request.canvasWidth, x: signature.x, y: signature.y)
         let maskPixel = try pixel(maskFill, width: request.canvasWidth, x: signature.x, y: signature.y)
         let metalPixel = try pixel(metalCPU, width: request.canvasWidth, x: signature.x, y: signature.y)
         let a8Value = a8[signature.y * request.canvasWidth + signature.x]
         let directMatch = directPixel == signature.expectedNativeRgb
         let maskMatch = maskPixel == signature.expectedNativeRgb
         let metalMatch = metalPixel == signature.expectedOxideRgb
         directNativeMatches += directMatch ? 1 : 0
         maskNativeMatches += maskMatch ? 1 : 0
         metalOxideMatches += metalMatch ? 1 : 0
         signatureCount += 1
         signatures.append([
            "x": signature.x,
            "y": signature.y,
            "glyph": signature.glyph,
            "a8": a8Value,
            "expected_native_rgb": signature.expectedNativeRgb,
            "expected_oxide_rgb": signature.expectedOxideRgb,
            "direct_rgb": directPixel,
            "mask_fill_rgb": maskPixel,
            "metal_cpu_rgb": metalPixel,
            "direct_matches_native": directMatch,
            "mask_fill_matches_native": maskMatch,
            "metal_cpu_matches_oxide": metalMatch,
         ])
      }
      caseReports.append([
         "id": title.id,
         "text": title.text,
         "origin_x_millionths": Int64((prepared.originX * 1_000_000).rounded()),
         "baseline_millionths": Int64((prepared.baseline * 1_000_000).rounded()),
         "a8_path": a8Name,
         "a8_sha256": sha256(a8),
         "direct_rgb_path": directName,
         "direct_rgb_sha256": sha256(direct),
         "mask_fill_rgb_path": maskName,
         "mask_fill_rgb_sha256": sha256(maskFill),
         "metal_cpu_rgb_path": metalName,
         "metal_cpu_rgb_sha256": sha256(metalCPU),
         "signature_count": title.signatures.count,
         "direct_vs_mask_fill": try difference(direct, maskFill),
         "mask_fill_vs_metal_cpu": try difference(maskFill, metalCPU),
         "direct_vs_metal_cpu": try difference(direct, metalCPU),
         "signatures": signatures,
      ])
   }
   let report: [String: Any] = [
      "schema_version": 1,
      "algorithm": "coretext-a8-metal-text-compositing-v1",
      "font_sha256": request.fontSha256,
      "canvas_width": request.canvasWidth,
      "canvas_height": request.canvasHeight,
      "case_count": request.cases.count,
      "signature_count": signatureCount,
      "direct_native_match_count": directNativeMatches,
      "mask_fill_native_match_count": maskNativeMatches,
      "metal_oxide_match_count": metalOxideMatches,
      "cases": caseReports,
      "acceptance_note": "comparison-only attribution; static acceptance remains exact full-frame RGB equality",
   ]
   let data = try JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys])
   try data.write(to: URL(fileURLWithPath: reportPath))
}

do
{
   try execute()
}
catch
{
   FileHandle.standardError.write(Data("text compositing helper failed: \(error)\n".utf8))
   exit(1)
}
