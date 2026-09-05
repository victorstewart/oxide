import AppKit
import CoreGraphics
import CryptoKit
import Foundation
import ImageIO
import Metal

private enum DiagnosticFailure: Error, CustomStringConvertible
{
   case invalidArgument(String)
   case invalidInput(String)
   case unavailable(String)

   var description: String
   {
      switch self
      {
      case .invalidArgument(let message): return message
      case .invalidInput(let message): return message
      case .unavailable(let message): return message
      }
   }
}

private struct SamplingCase: Decodable
{
   let id: String
   let destinationWidthMillionths: UInt64
   let destinationHeightMillionths: UInt64
}

private struct SamplingRequest: Decodable
{
   let schemaVersion: UInt32
   let sourceWidth: Int
   let sourceHeight: Int
   let roiWidth: Int
   let roiHeight: Int
   let phasesMillionths: [Int]
   let cases: [SamplingCase]
}

private struct DifferenceBounds: Encodable
{
   let x: Int
   let y: Int
   let width: Int
   let height: Int
}

private struct DifferenceMetrics: Encodable
{
   let comparedPixelCount: Int
   let differingPixelCount: Int
   let differingChannelCount: Int
   let maximumChannelDelta: UInt8
   let differingBounds: DifferenceBounds?
   let channelDeltaHistogram: [String: Int]
   let exact: Bool
}

private struct PairReport: Encodable
{
   let caseID: String
   let destinationWidthMillionths: UInt64
   let destinationHeightMillionths: UInt64
   let phaseMillionths: Int
   let appKitInterpolation: String
   let metalBGRASHA256: String
   let appKitBGRASHA256: String
   let difference: DifferenceMetrics

   private enum CodingKeys: String, CodingKey
   {
      case caseID = "case_id"
      case destinationWidthMillionths = "destination_width_millionths"
      case destinationHeightMillionths = "destination_height_millionths"
      case phaseMillionths = "phase_millionths"
      case appKitInterpolation = "appkit_interpolation"
      case metalBGRASHA256 = "metal_bgra_sha256"
      case appKitBGRASHA256 = "appkit_bgra_sha256"
      case difference
   }
}

private struct DiagnosticReport: Encodable
{
   let schemaVersion: UInt32
   let algorithm: String
   let sourcePNGPath: String
   let sourcePNGWidth: Int
   let sourcePNGHeight: Int
   let sourcePNGSHA256: String
   let oxideDecodedRGBASHA256: String
   let appKitDecodedRGBASHA256: String
   let decodedRGBAExact: Bool
   let roiWidth: Int
   let roiHeight: Int
   let phasesMillionths: [Int]
   let pairCount: Int
   let pairs: [PairReport]
   let reportPath: String
   let remainingGap: String

   private enum CodingKeys: String, CodingKey
   {
      case schemaVersion = "schema_version"
      case algorithm
      case sourcePNGPath = "source_png_path"
      case sourcePNGWidth = "source_png_width"
      case sourcePNGHeight = "source_png_height"
      case sourcePNGSHA256 = "source_png_sha256"
      case oxideDecodedRGBASHA256 = "oxide_decoded_rgba_sha256"
      case appKitDecodedRGBASHA256 = "appkit_decoded_rgba_sha256"
      case decodedRGBAExact = "decoded_rgba_exact"
      case roiWidth = "roi_width"
      case roiHeight = "roi_height"
      case phasesMillionths = "phases_millionths"
      case pairCount = "pair_count"
      case pairs
      case reportPath = "report_path"
      case remainingGap = "remaining_gap"
   }
}

private struct MetalParameters
{
   var destination: SIMD4<Float>
   var sourceSize: SIMD2<Float>
   var outputSize: SIMD2<Float>
}

private final class MetalLinearReference
{
   private let device: MTLDevice
   private let queue: MTLCommandQueue
   private let pipeline: MTLRenderPipelineState
   private let sampler: MTLSamplerState
   private let source: MTLTexture

   init(width: Int, height: Int, bgra: Data) throws
   {
      guard bgra.count == width * height * 4 else
      {
         throw DiagnosticFailure.invalidInput("Oxide BGRA byte count differs from source dimensions")
      }
      guard let device = MTLCreateSystemDefaultDevice(),
            let queue = device.makeCommandQueue() else
      {
         throw DiagnosticFailure.unavailable("Metal device or command queue is unavailable")
      }
      let shader = """
      #include <metal_stdlib>
      using namespace metal;

      struct Parameters
      {
         float4 destination;
         float2 source_size;
         float2 output_size;
      };

      struct VertexOutput
      {
         float4 position [[position]];
         float2 pixel_position;
      };

      vertex VertexOutput sampling_vertex(uint vertex_id [[vertex_id]], constant Parameters& parameters [[buffer(0)]])
      {
         float2 corners[6] = {
            float2(0.0, 0.0), float2(1.0, 0.0), float2(0.0, 1.0),
            float2(0.0, 1.0), float2(1.0, 0.0), float2(1.0, 1.0)
         };
         float2 pixel = parameters.destination.xy + corners[vertex_id] * parameters.destination.zw;
         float2 clip = float2(pixel.x / parameters.output_size.x * 2.0 - 1.0,
                              1.0 - pixel.y / parameters.output_size.y * 2.0);
         VertexOutput output;
         output.position = float4(clip, 0.0, 1.0);
         output.pixel_position = pixel;
         return output;
      }

      fragment float4 sampling_fragment(VertexOutput input [[stage_in]],
                                        constant Parameters& parameters [[buffer(0)]],
                                        texture2d<float> source [[texture(0)]],
                                        sampler linear_sampler [[sampler(0)]])
      {
         float2 local = input.pixel_position - parameters.destination.xy;
         float2 source_pixel = local * parameters.source_size / parameters.destination.zw;
         source_pixel = clamp(source_pixel, float2(0.5), parameters.source_size - 0.5);
         return source.sample(linear_sampler, source_pixel / parameters.source_size);
      }
      """
      guard let library = try? device.makeLibrary(source: shader, options: nil),
            let vertex = library.makeFunction(name: "sampling_vertex"),
            let fragment = library.makeFunction(name: "sampling_fragment") else
      {
         throw DiagnosticFailure.unavailable("Metal sampling shader compilation failed")
      }
      let pipelineDescriptor = MTLRenderPipelineDescriptor()
      pipelineDescriptor.vertexFunction = vertex
      pipelineDescriptor.fragmentFunction = fragment
      pipelineDescriptor.colorAttachments[0].pixelFormat = .bgra8Unorm_srgb
      guard let pipeline = try? device.makeRenderPipelineState(descriptor: pipelineDescriptor) else
      {
         throw DiagnosticFailure.unavailable("Metal sampling pipeline creation failed")
      }
      let samplerDescriptor = MTLSamplerDescriptor()
      samplerDescriptor.minFilter = .linear
      samplerDescriptor.magFilter = .linear
      samplerDescriptor.mipFilter = .notMipmapped
      samplerDescriptor.sAddressMode = .clampToEdge
      samplerDescriptor.tAddressMode = .clampToEdge
      guard let sampler = device.makeSamplerState(descriptor: samplerDescriptor) else
      {
         throw DiagnosticFailure.unavailable("Metal linear sampler creation failed")
      }
      let textureDescriptor = MTLTextureDescriptor.texture2DDescriptor(
         pixelFormat: .bgra8Unorm_srgb,
         width: width,
         height: height,
         mipmapped: false
      )
      textureDescriptor.storageMode = .shared
      textureDescriptor.usage = [.shaderRead]
      guard let source = device.makeTexture(descriptor: textureDescriptor) else
      {
         throw DiagnosticFailure.unavailable("Metal source texture creation failed")
      }
      bgra.withUnsafeBytes
      {
         source.replace(
            region: MTLRegionMake2D(0, 0, width, height),
            mipmapLevel: 0,
            withBytes: $0.baseAddress!,
            bytesPerRow: width * 4
         )
      }
      self.device = device
      self.queue = queue
      self.pipeline = pipeline
      self.sampler = sampler
      self.source = source
   }

   func render(width: Int, height: Int, destination: CGRect) throws -> Data
   {
      let descriptor = MTLTextureDescriptor.texture2DDescriptor(
         pixelFormat: .bgra8Unorm_srgb,
         width: width,
         height: height,
         mipmapped: false
      )
      descriptor.storageMode = .shared
      descriptor.usage = [.renderTarget]
      guard let output = device.makeTexture(descriptor: descriptor),
            let commandBuffer = queue.makeCommandBuffer() else
      {
         throw DiagnosticFailure.unavailable("Metal output texture or command buffer is unavailable")
      }
      let pass = MTLRenderPassDescriptor()
      pass.colorAttachments[0].texture = output
      pass.colorAttachments[0].loadAction = .dontCare
      pass.colorAttachments[0].storeAction = .store
      guard let encoder = commandBuffer.makeRenderCommandEncoder(descriptor: pass) else
      {
         throw DiagnosticFailure.unavailable("Metal render encoder creation failed")
      }
      var parameters = MetalParameters(
         destination: SIMD4(Float(destination.minX), Float(destination.minY), Float(destination.width), Float(destination.height)),
         sourceSize: SIMD2(Float(source.width), Float(source.height)),
         outputSize: SIMD2(Float(width), Float(height))
      )
      encoder.setRenderPipelineState(pipeline)
      encoder.setVertexBytes(&parameters, length: MemoryLayout<MetalParameters>.stride, index: 0)
      encoder.setFragmentBytes(&parameters, length: MemoryLayout<MetalParameters>.stride, index: 0)
      encoder.setFragmentTexture(source, index: 0)
      encoder.setFragmentSamplerState(sampler, index: 0)
      encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 6)
      encoder.endEncoding()
      commandBuffer.commit()
      commandBuffer.waitUntilCompleted()
      guard commandBuffer.status == .completed else
      {
         throw DiagnosticFailure.unavailable("Metal sampling command failed")
      }
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
      return bytes
   }
}

private final class CanonicalSRGBEncoder
{
   private let device: MTLDevice
   private let queue: MTLCommandQueue
   private let pipeline: MTLRenderPipelineState

   init() throws
   {
      guard let device = MTLCreateSystemDefaultDevice(),
            let queue = device.makeCommandQueue() else
      {
         throw DiagnosticFailure.unavailable("canonical Metal device or command queue is unavailable")
      }
      let shader = """
      #include <metal_stdlib>
      using namespace metal;

      struct VertexOutput { float4 position [[position]]; };

      vertex VertexOutput canonical_vertex(uint vertex_id [[vertex_id]])
      {
         float2 corner = float2((vertex_id << 1) & 2, vertex_id & 2);
         VertexOutput output;
         output.position = float4(corner * float2(2.0, -2.0) + float2(-1.0, 1.0), 0.0, 1.0);
         return output;
      }

      fragment float4 canonical_fragment(VertexOutput input [[stage_in]], texture2d<float, access::read> source [[texture(0)]])
      {
         return source.read(uint2(input.position.xy));
      }
      """
      guard let library = try? device.makeLibrary(source: shader, options: nil),
            let vertex = library.makeFunction(name: "canonical_vertex"),
            let fragment = library.makeFunction(name: "canonical_fragment") else
      {
         throw DiagnosticFailure.unavailable("canonical Metal shader compilation failed")
      }
      let descriptor = MTLRenderPipelineDescriptor()
      descriptor.vertexFunction = vertex
      descriptor.fragmentFunction = fragment
      descriptor.colorAttachments[0].pixelFormat = .bgra8Unorm_srgb
      guard let pipeline = try? device.makeRenderPipelineState(descriptor: descriptor) else
      {
         throw DiagnosticFailure.unavailable("canonical Metal pipeline creation failed")
      }
      self.device = device
      self.queue = queue
      self.pipeline = pipeline
   }

   func encode(context: CGContext, width: Int, height: Int) throws -> Data
   {
      guard let sourceData = context.data else
      {
         throw DiagnosticFailure.unavailable("linear AppKit context has no storage")
      }
      let sourceDescriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba32Float, width: width, height: height, mipmapped: false)
      sourceDescriptor.storageMode = .shared
      sourceDescriptor.usage = [.shaderRead]
      let outputDescriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .bgra8Unorm_srgb, width: width, height: height, mipmapped: false)
      outputDescriptor.storageMode = .shared
      outputDescriptor.usage = [.renderTarget]
      guard let source = device.makeTexture(descriptor: sourceDescriptor),
            let output = device.makeTexture(descriptor: outputDescriptor),
            let commandBuffer = queue.makeCommandBuffer() else
      {
         throw DiagnosticFailure.unavailable("canonical Metal resources are unavailable")
      }
      source.replace(region: MTLRegionMake2D(0, 0, width, height), mipmapLevel: 0, withBytes: sourceData, bytesPerRow: width * 16)
      let pass = MTLRenderPassDescriptor()
      pass.colorAttachments[0].texture = output
      pass.colorAttachments[0].loadAction = .dontCare
      pass.colorAttachments[0].storeAction = .store
      guard let encoder = commandBuffer.makeRenderCommandEncoder(descriptor: pass) else
      {
         throw DiagnosticFailure.unavailable("canonical Metal encoder is unavailable")
      }
      encoder.setRenderPipelineState(pipeline)
      encoder.setFragmentTexture(source, index: 0)
      encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
      encoder.endEncoding()
      commandBuffer.commit()
      commandBuffer.waitUntilCompleted()
      guard commandBuffer.status == .completed else
      {
         throw DiagnosticFailure.unavailable("canonical Metal conversion failed")
      }
      var bytes = Data(count: width * height * 4)
      bytes.withUnsafeMutableBytes
      {
         output.getBytes($0.baseAddress!, bytesPerRow: width * 4, from: MTLRegionMake2D(0, 0, width, height), mipmapLevel: 0)
      }
      return bytes
   }
}

private func makeLinearContext(width: Int, height: Int) throws -> CGContext
{
   guard let colorSpace = CGColorSpace(name: CGColorSpace.linearSRGB),
         let context = CGContext(
            data: nil,
            width: width,
            height: height,
            bitsPerComponent: 32,
            bytesPerRow: width * 16,
            space: colorSpace,
            bitmapInfo: CGBitmapInfo.floatComponents.rawValue
               | CGBitmapInfo.byteOrder32Little.rawValue
               | CGImageAlphaInfo.noneSkipLast.rawValue
         ) else
   {
      throw DiagnosticFailure.unavailable("linear-sRGB float context creation failed")
   }
   context.translateBy(x: 0, y: CGFloat(height))
   context.scaleBy(x: 1, y: -1)
   return context
}

private func renderAppKit(image: NSImage, width: Int, height: Int, destination: CGRect, quality: NSImageInterpolation, encoder: CanonicalSRGBEncoder) throws -> Data
{
   let context = try makeLinearContext(width: width, height: height)
   let graphics = NSGraphicsContext(cgContext: context, flipped: true)
   NSGraphicsContext.saveGraphicsState()
   NSGraphicsContext.current = graphics
   image.draw(
      in: destination,
      from: .zero,
      operation: .sourceOver,
      fraction: 1,
      respectFlipped: true,
      hints: [.interpolation: quality]
   )
   NSGraphicsContext.restoreGraphicsState()
   return try encoder.encode(context: context, width: width, height: height)
}

private func canonicalDecodedRGBA(sourceData: Data, width: Int, height: Int) throws -> Data
{
   guard let representation = NSBitmapImageRep(data: sourceData),
         representation.pixelsWide == width,
         representation.pixelsHigh == height,
         representation.bitsPerSample == 8,
         !representation.isPlanar,
         let source = representation.bitmapData else
   {
      throw DiagnosticFailure.invalidInput("AppKit could not expose canonical packed 8-bit source pixels")
   }
   let samples = representation.samplesPerPixel
   let bytesPerPixel = representation.bitsPerPixel / 8
   guard (samples == 3 || samples == 4),
         (bytesPerPixel == 3 || bytesPerPixel == 4),
         representation.bytesPerRow >= width * bytesPerPixel else
   {
      throw DiagnosticFailure.invalidInput("AppKit decoded source has an unsupported pixel layout")
   }
   let alphaFirst = representation.bitmapFormat.contains(.alphaFirst)
   var rgba = Data(count: width * height * 4)
   rgba.withUnsafeMutableBytes
   {
      let destination = $0.bindMemory(to: UInt8.self)
      for y in 0..<height
      {
         let row = source.advanced(by: y * representation.bytesPerRow)
         for x in 0..<width
         {
            let input = row.advanced(by: x * bytesPerPixel)
            let output = (y * width + x) * 4
            if samples == 3 && bytesPerPixel == 3
            {
               destination[output] = input[0]
               destination[output + 1] = input[1]
               destination[output + 2] = input[2]
               destination[output + 3] = 255
            }
            else if samples == 4 && alphaFirst
            {
               destination[output] = input[1]
               destination[output + 1] = input[2]
               destination[output + 2] = input[3]
               destination[output + 3] = input[0]
            }
            else
            {
               destination[output] = input[0]
               destination[output + 1] = input[1]
               destination[output + 2] = input[2]
               destination[output + 3] = input[3]
            }
         }
      }
   }
   return rgba
}

private func difference(_ metal: Data, _ appKit: Data, width: Int, height: Int) throws -> DifferenceMetrics
{
   guard metal.count == appKit.count, metal.count == width * height * 4 else
   {
      throw DiagnosticFailure.invalidInput("sample output byte counts differ")
   }
   var differingPixels = 0
   var differingChannels = 0
   var maximumDelta: UInt8 = 0
   var minimumX = width
   var minimumY = height
   var maximumX = 0
   var maximumY = 0
   var histogram = [String: Int]()
   metal.withUnsafeBytes
   {
      metalBytes in
      appKit.withUnsafeBytes
      {
         appKitBytes in
         let metal = metalBytes.bindMemory(to: UInt8.self)
         let appKit = appKitBytes.bindMemory(to: UInt8.self)
         for pixel in 0..<(width * height)
         {
            var pixelDiffers = false
            for channel in 0..<3
            {
               let delta = metal[pixel * 4 + channel] > appKit[pixel * 4 + channel]
                  ? metal[pixel * 4 + channel] - appKit[pixel * 4 + channel]
                  : appKit[pixel * 4 + channel] - metal[pixel * 4 + channel]
               if delta != 0
               {
                  pixelDiffers = true
                  differingChannels += 1
                  maximumDelta = max(maximumDelta, delta)
                  histogram[String(delta), default: 0] += 1
               }
            }
            if pixelDiffers
            {
               differingPixels += 1
               let x = pixel % width
               let y = pixel / width
               minimumX = min(minimumX, x)
               minimumY = min(minimumY, y)
               maximumX = max(maximumX, x)
               maximumY = max(maximumY, y)
            }
         }
      }
   }
   let bounds = differingPixels == 0 ? nil : DifferenceBounds(
      x: minimumX,
      y: minimumY,
      width: maximumX - minimumX + 1,
      height: maximumY - minimumY + 1
   )
   return DifferenceMetrics(
      comparedPixelCount: width * height,
      differingPixelCount: differingPixels,
      differingChannelCount: differingChannels,
      maximumChannelDelta: maximumDelta,
      differingBounds: bounds,
      channelDeltaHistogram: histogram,
      exact: differingPixels == 0
   )
}

private func sha256(_ data: Data) -> String
{
   SHA256.hash(data: data).map({String(format: "%02x", $0)}).joined()
}

private func destination(case samplingCase: SamplingCase, request: SamplingRequest, phaseMillionths: Int) -> CGRect
{
   let width = CGFloat(samplingCase.destinationWidthMillionths) / 1_000_000
   let height = CGFloat(samplingCase.destinationHeightMillionths) / 1_000_000
   let phase = CGFloat(phaseMillionths) / 1_000_000
   return CGRect(
      x: CGFloat(request.roiWidth) * 0.5 - width * 0.5 + phase,
      y: CGFloat(request.roiHeight) * 0.5 - height * 0.5 + phase,
      width: width,
      height: height
   )
}

private func run() throws
{
   let arguments = CommandLine.arguments
   guard arguments.count == 7 else
   {
      throw DiagnosticFailure.invalidArgument("expected source.png oxide.bgra request.json report.json source-sha oxide-rgba-sha")
   }
   let sourceURL = URL(fileURLWithPath: arguments[1])
   let oxideBGRAURL = URL(fileURLWithPath: arguments[2])
   let requestURL = URL(fileURLWithPath: arguments[3])
   let reportURL = URL(fileURLWithPath: arguments[4])
   let expectedSourceSHA = arguments[5]
   let oxideDecodedRGBASHA = arguments[6]
   let sourceData = try Data(contentsOf: sourceURL)
   guard sha256(sourceData) == expectedSourceSHA else
   {
      throw DiagnosticFailure.invalidInput("source PNG SHA-256 differs across the Rust/Swift boundary")
   }
   let decoder = JSONDecoder()
   decoder.keyDecodingStrategy = .convertFromSnakeCase
   let request = try decoder.decode(SamplingRequest.self, from: Data(contentsOf: requestURL))
   guard request.schemaVersion == 1,
         request.sourceWidth == 4_096,
         request.sourceHeight == 3_072,
         request.roiWidth > 0,
         request.roiHeight > 0,
         request.phasesMillionths == [0, 250_000, 500_000],
         request.cases.count == 4 else
   {
      throw DiagnosticFailure.invalidInput("sampling request violates the bounded v1 matrix")
   }
   guard let image = NSImage(data: sourceData) else
   {
      throw DiagnosticFailure.invalidInput("AppKit could not decode the source PNG")
   }
   let appKitDecodedRGBA = try canonicalDecodedRGBA(sourceData: sourceData, width: request.sourceWidth, height: request.sourceHeight)
   let appKitDecodedRGBASHA = sha256(appKitDecodedRGBA)
   let oxideBGRA = try Data(contentsOf: oxideBGRAURL)
   let metal = try MetalLinearReference(width: request.sourceWidth, height: request.sourceHeight, bgra: oxideBGRA)
   let encoder = try CanonicalSRGBEncoder()
   let qualities: [(String, NSImageInterpolation)] = [
      ("none", .none),
      ("low", .low),
      ("high", .high),
   ]
   var pairs = [PairReport]()
   pairs.reserveCapacity(request.cases.count * request.phasesMillionths.count * qualities.count)
   for samplingCase in request.cases
   {
      for phase in request.phasesMillionths
      {
         let target = destination(case: samplingCase, request: request, phaseMillionths: phase)
         let metalBGRA = try metal.render(width: request.roiWidth, height: request.roiHeight, destination: target)
         for (qualityName, quality) in qualities
         {
            let appKitBGRA = try renderAppKit(
               image: image,
               width: request.roiWidth,
               height: request.roiHeight,
               destination: target,
               quality: quality,
               encoder: encoder
            )
            pairs.append(PairReport(
               caseID: samplingCase.id,
               destinationWidthMillionths: samplingCase.destinationWidthMillionths,
               destinationHeightMillionths: samplingCase.destinationHeightMillionths,
               phaseMillionths: phase,
               appKitInterpolation: qualityName,
               metalBGRASHA256: sha256(metalBGRA),
               appKitBGRASHA256: sha256(appKitBGRA),
               difference: try difference(metalBGRA, appKitBGRA, width: request.roiWidth, height: request.roiHeight)
            ))
         }
      }
   }
   let report = DiagnosticReport(
      schemaVersion: 1,
      algorithm: "appkit-metal-image-sampling-exact-v1",
      sourcePNGPath: sourceURL.path,
      sourcePNGWidth: request.sourceWidth,
      sourcePNGHeight: request.sourceHeight,
      sourcePNGSHA256: expectedSourceSHA,
      oxideDecodedRGBASHA256: oxideDecodedRGBASHA,
      appKitDecodedRGBASHA256: appKitDecodedRGBASHA,
      decodedRGBAExact: oxideDecodedRGBASHA == appKitDecodedRGBASHA,
      roiWidth: request.roiWidth,
      roiHeight: request.roiHeight,
      phasesMillionths: request.phasesMillionths,
      pairCount: pairs.count,
      pairs: pairs,
      reportPath: reportURL.path,
      remainingGap: "The Metal side reproduces Oxide's BGRA8Unorm_sRGB texture, linear sampler, clamp, UV mapping, and output format, but does not execute oxide-renderer-metal's production pipeline or ui.metal shader. Promote no production change without a renderer-backed A/B."
   )
   let encoderJSON = JSONEncoder()
   encoderJSON.keyEncodingStrategy = .convertToSnakeCase
   encoderJSON.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
   try encoderJSON.encode(report).write(to: reportURL, options: .atomic)
}

do
{
   try run()
}
catch
{
   FileHandle.standardError.write(Data("image sampling reference failed: \(error)\n".utf8))
   exit(1)
}
