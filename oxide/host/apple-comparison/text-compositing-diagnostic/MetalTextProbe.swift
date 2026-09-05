import CryptoKit
import Foundation
import Metal

private enum ProbeFailure: Error, CustomStringConvertible
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
   let signatures: [KnownSignature]
}

private struct DiagnosticRequest: Decodable
{
   let schemaVersion: UInt32
   let canvasWidth: Int
   let canvasHeight: Int
   let deviceScale: Int
   let backgroundSrgb: [UInt8]
   let textSrgb: [UInt8]
   let cases: [TitleCase]
}

private struct ManifestInstance: Decodable
{
   let dstBits: [UInt32]
   let uvBits: [UInt32]
   let colorBits: [UInt32]
}

private struct ProbeManifest: Decodable
{
   let schemaVersion: UInt32
   let caseId: String
   let canvasWidth: Int
   let canvasHeight: Int
   let viewportWidthBits: UInt32
   let viewportHeightBits: UInt32
   let atlasPath: String
   let atlasWidth: Int
   let atlasHeight: Int
   let atlasRowBytes: Int
   let atlasSha256: String
   let instances: [ManifestInstance]
}

private struct GlyphInstance
{
   let dst: SIMD4<Float>
   let uv: SIMD4<Float>
   let color: SIMD4<Float>
}

private let metalSource = """
#include <metal_stdlib>
using namespace metal;

struct GlyphInstance { packed_float4 dst; packed_float4 uv; packed_float4 color; };
struct GlyphOut { float4 position [[position]]; float2 uv; float4 color; };
constant float2 corners[4] = {
   float2(0.0, 0.0), float2(1.0, 0.0),
   float2(0.0, 1.0), float2(1.0, 1.0),
};

vertex GlyphOut probe_vertex(
   uint vertexId [[vertex_id]],
   uint instanceId [[instance_id]],
   device const GlyphInstance* instances [[buffer(0)]],
   constant float2& viewport [[buffer(1)]])
{
   GlyphInstance instance = instances[instanceId];
   float2 corner = corners[vertexId];
   float4 dst = float4(instance.dst);
   float4 uv = float4(instance.uv);
   float2 position = dst.xy + corner * dst.zw;
   GlyphOut output;
   output.position = float4(
      (position.x / max(viewport.x, 1.0)) * 2.0 - 1.0,
      1.0 - (position.y / max(viewport.y, 1.0)) * 2.0,
      0.0,
      1.0
   );
   output.uv = mix(uv.xy, uv.zw, corner);
   output.color = float4(instance.color);
   return output;
}

fragment float probe_coverage(
   GlyphOut input [[stage_in]],
   texture2d<float> atlas [[texture(0)]],
   sampler atlasSampler [[sampler(0)]])
{
   return atlas.sample(atlasSampler, input.uv).r;
}

fragment float4 probe_glyph(
   GlyphOut input [[stage_in]],
   texture2d<float> atlas [[texture(0)]],
   sampler atlasSampler [[sampler(0)]])
{
   float coverage = atlas.sample(atlasSampler, input.uv).r;
   return float4(input.color.rgb, input.color.a * coverage);
}
"""

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

private func floats(_ bits: [UInt32], label: String) throws -> SIMD4<Float>
{
   guard bits.count == 4 else {throw ProbeFailure.invalid("\(label) must contain four float identities")}
   return SIMD4(bits.map(Float.init(bitPattern:)))
}

private func instances(_ manifest: ProbeManifest) throws -> [GlyphInstance]
{
   try manifest.instances.map
   {
      GlyphInstance(
         dst: try floats($0.dstBits, label: "dst"),
         uv: try floats($0.uvBits, label: "uv"),
         color: try floats($0.colorBits, label: "color")
      )
   }
}

private func makeSampler(_ device: MTLDevice, linear: Bool) throws -> MTLSamplerState
{
   let descriptor = MTLSamplerDescriptor()
   descriptor.minFilter = linear ? .linear : .nearest
   descriptor.magFilter = linear ? .linear : .nearest
   descriptor.mipFilter = linear ? .linear : .nearest
   descriptor.sAddressMode = .clampToEdge
   descriptor.tAddressMode = .clampToEdge
   guard let sampler = device.makeSamplerState(descriptor: descriptor) else
   {
      throw ProbeFailure.unavailable("Metal sampler is unavailable")
   }
   return sampler
}

private func makePipeline(
   device: MTLDevice,
   library: MTLLibrary,
   fragment: String,
   format: MTLPixelFormat,
   blended: Bool
) throws -> MTLRenderPipelineState
{
   let descriptor = MTLRenderPipelineDescriptor()
   descriptor.vertexFunction = library.makeFunction(name: "probe_vertex")
   descriptor.fragmentFunction = library.makeFunction(name: fragment)
   let attachment = descriptor.colorAttachments[0]!
   attachment.pixelFormat = format
   if blended
   {
      attachment.isBlendingEnabled = true
      attachment.rgbBlendOperation = .add
      attachment.alphaBlendOperation = .add
      attachment.sourceRGBBlendFactor = .sourceAlpha
      attachment.sourceAlphaBlendFactor = .sourceAlpha
      attachment.destinationRGBBlendFactor = .oneMinusSourceAlpha
      attachment.destinationAlphaBlendFactor = .oneMinusSourceAlpha
   }
   return try device.makeRenderPipelineState(descriptor: descriptor)
}

private func makeTexture(
   device: MTLDevice,
   format: MTLPixelFormat,
   width: Int,
   height: Int,
   usage: MTLTextureUsage
) throws -> MTLTexture
{
   let descriptor = MTLTextureDescriptor.texture2DDescriptor(
      pixelFormat: format,
      width: width,
      height: height,
      mipmapped: false
   )
   descriptor.storageMode = .shared
   descriptor.usage = usage
   guard let texture = device.makeTexture(descriptor: descriptor) else
   {
      throw ProbeFailure.unavailable("Metal texture \(format.rawValue) is unavailable")
   }
   return texture
}

private func makeAtlas(device: MTLDevice, manifest: ProbeManifest, output: URL) throws -> MTLTexture
{
   let data = try Data(contentsOf: output.appendingPathComponent(manifest.atlasPath))
   guard data.count >= manifest.atlasRowBytes * manifest.atlasHeight,
         sha256(data) == manifest.atlasSha256 else
   {
      throw ProbeFailure.invalid("production atlas identity differs")
   }
   let texture = try makeTexture(
      device: device,
      format: .r8Unorm,
      width: manifest.atlasWidth,
      height: manifest.atlasHeight,
      usage: .shaderRead
   )
   data.withUnsafeBytes
   {
      texture.replace(
         region: MTLRegionMake2D(0, 0, manifest.atlasWidth, manifest.atlasHeight),
         mipmapLevel: 0,
         withBytes: $0.baseAddress!,
         bytesPerRow: manifest.atlasRowBytes
      )
   }
   return texture
}

private func encode(
   queue: MTLCommandQueue,
   pipeline: MTLRenderPipelineState,
   target: MTLTexture,
   atlas: MTLTexture,
   sampler: MTLSamplerState,
   instances: [GlyphInstance],
   viewport: SIMD2<Float>,
   clear: MTLClearColor
) throws
{
   guard !instances.isEmpty,
         let buffer = queue.device.makeBuffer(
            bytes: instances,
            length: instances.count * MemoryLayout<GlyphInstance>.stride,
            options: .storageModeShared
         ),
         let command = queue.makeCommandBuffer() else
   {
      throw ProbeFailure.unavailable("Metal probe command resources are unavailable")
   }
   let pass = MTLRenderPassDescriptor()
   pass.colorAttachments[0].texture = target
   pass.colorAttachments[0].loadAction = .clear
   pass.colorAttachments[0].storeAction = .store
   pass.colorAttachments[0].clearColor = clear
   guard let encoder = command.makeRenderCommandEncoder(descriptor: pass) else
   {
      throw ProbeFailure.unavailable("Metal probe encoder is unavailable")
   }
   var viewport = viewport
   encoder.setRenderPipelineState(pipeline)
   encoder.setVertexBuffer(buffer, offset: 0, index: 0)
   encoder.setVertexBytes(&viewport, length: MemoryLayout<SIMD2<Float>>.stride, index: 1)
   encoder.setFragmentTexture(atlas, index: 0)
   encoder.setFragmentSamplerState(sampler, index: 0)
   encoder.drawPrimitives(type: .triangleStrip, vertexStart: 0, vertexCount: 4, instanceCount: instances.count)
   encoder.endEncoding()
   command.commit()
   command.waitUntilCompleted()
   guard command.status == .completed else
   {
      throw ProbeFailure.unavailable("Metal probe command failed: \(command.error?.localizedDescription ?? "unknown error")")
   }
}

private func renderCoverage(
   device: MTLDevice,
   queue: MTLCommandQueue,
   pipeline: MTLRenderPipelineState,
   atlas: MTLTexture,
   sampler: MTLSamplerState,
   instances: [GlyphInstance],
   manifest: ProbeManifest
) throws -> [Float]
{
   let target = try makeTexture(
      device: device,
      format: .r32Float,
      width: manifest.canvasWidth,
      height: manifest.canvasHeight,
      usage: .renderTarget
   )
   try encode(
      queue: queue,
      pipeline: pipeline,
      target: target,
      atlas: atlas,
      sampler: sampler,
      instances: instances,
      viewport: SIMD2(Float(bitPattern: manifest.viewportWidthBits), Float(bitPattern: manifest.viewportHeightBits)),
      clear: MTLClearColorMake(0, 0, 0, 0)
   )
   var coverage = [Float](repeating: 0, count: manifest.canvasWidth * manifest.canvasHeight)
   target.getBytes(
      &coverage,
      bytesPerRow: manifest.canvasWidth * MemoryLayout<Float>.stride,
      from: MTLRegionMake2D(0, 0, manifest.canvasWidth, manifest.canvasHeight),
      mipmapLevel: 0
   )
   return coverage
}

private func renderFull(
   device: MTLDevice,
   queue: MTLCommandQueue,
   pipeline: MTLRenderPipelineState,
   atlas: MTLTexture,
   sampler: MTLSamplerState,
   instances: [GlyphInstance],
   manifest: ProbeManifest,
   background: [Float]
) throws -> Data
{
   let target = try makeTexture(
      device: device,
      format: .bgra8Unorm_srgb,
      width: manifest.canvasWidth,
      height: manifest.canvasHeight,
      usage: .renderTarget
   )
   try encode(
      queue: queue,
      pipeline: pipeline,
      target: target,
      atlas: atlas,
      sampler: sampler,
      instances: instances,
      viewport: SIMD2(Float(bitPattern: manifest.viewportWidthBits), Float(bitPattern: manifest.viewportHeightBits)),
      clear: MTLClearColorMake(Double(background[0]), Double(background[1]), Double(background[2]), 1)
   )
   var bgra = [UInt8](repeating: 0, count: manifest.canvasWidth * manifest.canvasHeight * 4)
   target.getBytes(
      &bgra,
      bytesPerRow: manifest.canvasWidth * 4,
      from: MTLRegionMake2D(0, 0, manifest.canvasWidth, manifest.canvasHeight),
      mipmapLevel: 0
   )
   var rgb = Data(capacity: manifest.canvasWidth * manifest.canvasHeight * 3)
   for pixel in stride(from: 0, to: bgra.count, by: 4)
   {
      rgb.append(bgra[pixel + 2])
      rgb.append(bgra[pixel + 1])
      rgb.append(bgra[pixel])
   }
   return rgb
}

private func floatData(_ values: [Float]) -> Data
{
   values.withUnsafeBytes {Data($0)}
}

private func cpuComposite(_ coverage: [Float], foreground: [Float], background: [Float]) -> Data
{
   var rgb = Data(capacity: coverage.count * 3)
   for value in coverage
   {
      let alpha = max(0, min(1, value))
      for channel in 0..<3
      {
         rgb.append(linearToSrgb(foreground[channel] * alpha + background[channel] * (1 - alpha)))
      }
   }
   return rgb
}

private func rgbPixel(_ data: Data, width: Int, x: Int, y: Int) throws -> [UInt8]
{
   let offset = (y * width + x) * 3
   guard x >= 0, y >= 0, x < width, offset + 3 <= data.count else
   {
      throw ProbeFailure.invalid("signature coordinate is outside Metal output")
   }
   return Array(data[offset..<offset + 3])
}

private func execute() throws
{
   guard CommandLine.arguments.count == 4 else
   {
      throw ProbeFailure.invalid("usage: MetalTextProbe.swift <request> <output-directory> <report>")
   }
   let requestUrl = URL(fileURLWithPath: CommandLine.arguments[1])
   let output = URL(fileURLWithPath: CommandLine.arguments[2], isDirectory: true)
   let reportUrl = URL(fileURLWithPath: CommandLine.arguments[3])
   let decoder = JSONDecoder()
   decoder.keyDecodingStrategy = .convertFromSnakeCase
   let request = try decoder.decode(DiagnosticRequest.self, from: Data(contentsOf: requestUrl))
   guard request.schemaVersion == 1, request.backgroundSrgb.count == 3, request.textSrgb.count == 3,
         let device = MTLCreateSystemDefaultDevice(),
         let queue = device.makeCommandQueue() else
   {
      throw ProbeFailure.unavailable("Metal probe prerequisites are unavailable")
   }
   let library = try device.makeLibrary(source: metalSource, options: nil)
   let coveragePipeline = try makePipeline(
      device: device,
      library: library,
      fragment: "probe_coverage",
      format: .r32Float,
      blended: false
   )
   let glyphPipeline = try makePipeline(
      device: device,
      library: library,
      fragment: "probe_glyph",
      format: .bgra8Unorm_srgb,
      blended: true
   )
   let pointSampler = try makeSampler(device, linear: false)
   let linearSampler = try makeSampler(device, linear: true)
   let foreground = request.textSrgb.map(srgbToLinear)
   let background = request.backgroundSrgb.map(srgbToLinear)
   var caseReports = [[String: Any]]()
   var signatureCount = 0
   var pointMatches = 0
   var linearMatches = 0
   var fullMatches = 0
   var earliestScalar = 0
   var earliestAtlas = 0
   var earliestFilter = 0
   var earliestBlend = 0
   var unexplained = 0

   for title in request.cases
   {
      let manifestUrl = output.appendingPathComponent("\(title.id).production-probe.json")
      let manifestData = try Data(contentsOf: manifestUrl)
      let manifest = try decoder.decode(ProbeManifest.self, from: manifestData)
      guard manifest.schemaVersion == 1, manifest.caseId == title.id,
            manifest.canvasWidth == request.canvasWidth, manifest.canvasHeight == request.canvasHeight else
      {
         throw ProbeFailure.invalid("production Metal manifest identity differs")
      }
      let glyphs = try instances(manifest)
      let atlas = try makeAtlas(device: device, manifest: manifest, output: output)
      let pointCoverage = try renderCoverage(
         device: device,
         queue: queue,
         pipeline: coveragePipeline,
         atlas: atlas,
         sampler: pointSampler,
         instances: glyphs,
         manifest: manifest
      )
      let linearCoverage = try renderCoverage(
         device: device,
         queue: queue,
         pipeline: coveragePipeline,
         atlas: atlas,
         sampler: linearSampler,
         instances: glyphs,
         manifest: manifest
      )
      let pointCpu = cpuComposite(pointCoverage, foreground: foreground, background: background)
      let linearCpu = cpuComposite(linearCoverage, foreground: foreground, background: background)
      let scalarCpu = try Data(contentsOf: output.appendingPathComponent("\(title.id).metal-cpu.rgb"))
      guard scalarCpu.count == pointCpu.count else
      {
         throw ProbeFailure.invalid("scalar CPU replay shape differs from Metal probe")
      }
      let full = try renderFull(
         device: device,
         queue: queue,
         pipeline: glyphPipeline,
         atlas: atlas,
         sampler: linearSampler,
         instances: glyphs,
         manifest: manifest,
         background: background
      )
      let pointCoverageData = floatData(pointCoverage)
      let linearCoverageData = floatData(linearCoverage)
      let pointCoverageName = "\(title.id).metal-point-coverage.f32"
      let linearCoverageName = "\(title.id).metal-linear-coverage.f32"
      let pointCpuName = "\(title.id).metal-point-cpu.rgb"
      let linearCpuName = "\(title.id).metal-linear-cpu.rgb"
      let fullName = "\(title.id).metal-full.rgb"
      try pointCoverageData.write(to: output.appendingPathComponent(pointCoverageName))
      try linearCoverageData.write(to: output.appendingPathComponent(linearCoverageName))
      try pointCpu.write(to: output.appendingPathComponent(pointCpuName))
      try linearCpu.write(to: output.appendingPathComponent(linearCpuName))
      try full.write(to: output.appendingPathComponent(fullName))

      var differingCoveragePixels = 0
      var maximumCoverageDelta: Float = 0
      for (point, linear) in zip(pointCoverage, linearCoverage)
      {
         let delta = abs(point - linear)
         differingCoveragePixels += delta == 0 ? 0 : 1
         maximumCoverageDelta = max(maximumCoverageDelta, delta)
      }
      var scalarVsPointDifferingPixels = 0
      for index in stride(from: 0, to: scalarCpu.count, by: 3)
      {
         scalarVsPointDifferingPixels += scalarCpu[index..<index + 3] == pointCpu[index..<index + 3] ? 0 : 1
      }
      var signatures = [[String: Any]]()
      for signature in title.signatures
      {
         let index = signature.y * request.canvasWidth + signature.x
         guard index >= 0, index < pointCoverage.count else
         {
            throw ProbeFailure.invalid("signature coordinate is outside coverage output")
         }
         let pointPixel = try rgbPixel(pointCpu, width: request.canvasWidth, x: signature.x, y: signature.y)
         let linearPixel = try rgbPixel(linearCpu, width: request.canvasWidth, x: signature.x, y: signature.y)
         let fullPixel = try rgbPixel(full, width: request.canvasWidth, x: signature.x, y: signature.y)
         let scalarPixel = try rgbPixel(scalarCpu, width: request.canvasWidth, x: signature.x, y: signature.y)
         let scalarMatch = scalarPixel == signature.expectedOxideRgb
         let pointMatch = pointPixel == signature.expectedOxideRgb
         let linearMatch = linearPixel == signature.expectedOxideRgb
         let fullMatch = fullPixel == signature.expectedOxideRgb
         pointMatches += pointMatch ? 1 : 0
         linearMatches += linearMatch ? 1 : 0
         fullMatches += fullMatch ? 1 : 0
         let earliest: String
         if scalarMatch
         {
            earliest = "scalar-a8-replay"
            earliestScalar += 1
         }
         else if pointMatch
         {
            earliest = "atlas-raster-and-geometry"
            earliestAtlas += 1
         }
         else if linearMatch
         {
            earliest = "linear-sampler"
            earliestFilter += 1
         }
         else if fullMatch
         {
            earliest = "fixed-blend-srgb-target"
            earliestBlend += 1
         }
         else
         {
            earliest = "unexplained"
            unexplained += 1
         }
         signatureCount += 1
         signatures.append([
            "x": signature.x,
            "y": signature.y,
            "glyph": signature.glyph,
            "expected_native_rgb": signature.expectedNativeRgb,
            "expected_oxide_rgb": signature.expectedOxideRgb,
            "point_coverage_bits": pointCoverage[index].bitPattern,
            "linear_coverage_bits": linearCoverage[index].bitPattern,
            "scalar_cpu_rgb": scalarPixel,
            "point_cpu_rgb": pointPixel,
            "linear_cpu_rgb": linearPixel,
            "metal_full_rgb": fullPixel,
            "point_cpu_matches_oxide": pointMatch,
            "linear_cpu_matches_oxide": linearMatch,
            "metal_full_matches_oxide": fullMatch,
            "earliest_matching_stage": earliest,
         ])
      }
      caseReports.append([
         "id": title.id,
         "manifest_sha256": sha256(manifestData),
         "atlas_path": manifest.atlasPath,
         "atlas_sha256": manifest.atlasSha256,
         "instance_count": glyphs.count,
         "point_coverage_path": pointCoverageName,
         "point_coverage_sha256": sha256(pointCoverageData),
         "linear_coverage_path": linearCoverageName,
         "linear_coverage_sha256": sha256(linearCoverageData),
         "point_cpu_rgb_path": pointCpuName,
         "point_cpu_rgb_sha256": sha256(pointCpu),
         "linear_cpu_rgb_path": linearCpuName,
         "linear_cpu_rgb_sha256": sha256(linearCpu),
         "metal_full_rgb_path": fullName,
         "metal_full_rgb_sha256": sha256(full),
         "point_vs_linear_differing_pixel_count": differingCoveragePixels,
         "point_vs_linear_maximum_coverage_delta_bits": maximumCoverageDelta.bitPattern,
         "scalar_vs_point_differing_pixel_count": scalarVsPointDifferingPixels,
         "signature_count": title.signatures.count,
         "signatures": signatures,
      ])
   }
   let report: [String: Any] = [
      "schema_version": 1,
      "algorithm": "production-atlas-metal-stage-attribution-v1",
      "device_name": device.name,
      "case_count": request.cases.count,
      "signature_count": signatureCount,
      "point_cpu_oxide_match_count": pointMatches,
      "linear_cpu_oxide_match_count": linearMatches,
      "metal_full_oxide_match_count": fullMatches,
      "earliest_scalar_a8_replay_count": earliestScalar,
      "earliest_atlas_raster_and_geometry_count": earliestAtlas,
      "earliest_linear_sampler_count": earliestFilter,
      "earliest_fixed_blend_srgb_target_count": earliestBlend,
      "unexplained_count": unexplained,
      "cases": caseReports,
   ]
   let data = try JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys])
   try data.write(to: reportUrl)
}

do
{
   try execute()
}
catch
{
   FileHandle.standardError.write(Data("Metal text probe failed: \(error)\n".utf8))
   exit(1)
}
