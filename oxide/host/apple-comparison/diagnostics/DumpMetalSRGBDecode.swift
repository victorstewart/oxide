import Foundation
import Metal

guard let device = MTLCreateSystemDefaultDevice(), let queue = device.makeCommandQueue() else
{
   fatalError("Metal is unavailable")
}
let shader = """
#include <metal_stdlib>
using namespace metal;

struct Output
{
   float4 position [[position]];
};

vertex Output vertex_main(uint vertexID [[vertex_id]])
{
   float2 corner = float2((vertexID << 1) & 2, vertexID & 2);
   Output output;
   output.position = float4(corner * float2(2.0, -2.0) + float2(-1.0, 1.0), 0.0, 1.0);
   return output;
}

fragment float4 fragment_main(Output input [[stage_in]], texture2d<float, access::read> source [[texture(0)]])
{
   return source.read(uint2(input.position.xy));
}
"""
let library = try device.makeLibrary(source: shader, options: nil)
let descriptor = MTLRenderPipelineDescriptor()
descriptor.vertexFunction = library.makeFunction(name: "vertex_main")
descriptor.fragmentFunction = library.makeFunction(name: "fragment_main")
descriptor.colorAttachments[0].pixelFormat = .rgba32Float
let pipeline = try device.makeRenderPipelineState(descriptor: descriptor)
let sourceDescriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba8Unorm_srgb, width: 256, height: 1, mipmapped: false)
sourceDescriptor.storageMode = .shared
sourceDescriptor.usage = [.shaderRead]
let outputDescriptor = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba32Float, width: 256, height: 1, mipmapped: false)
outputDescriptor.storageMode = .shared
outputDescriptor.usage = [.renderTarget]
guard let source = device.makeTexture(descriptor: sourceDescriptor), let output = device.makeTexture(descriptor: outputDescriptor) else
{
   fatalError("texture allocation failed")
}
var encoded = [UInt8](repeating: 255, count: 256 * 4)
for value in 0..<256
{
   encoded[value * 4] = UInt8(value)
   encoded[value * 4 + 1] = UInt8(value)
   encoded[value * 4 + 2] = UInt8(value)
}
source.replace(region: MTLRegionMake2D(0, 0, 256, 1), mipmapLevel: 0, withBytes: encoded, bytesPerRow: 256 * 4)
let pass = MTLRenderPassDescriptor()
pass.colorAttachments[0].texture = output
pass.colorAttachments[0].loadAction = .dontCare
pass.colorAttachments[0].storeAction = .store
guard let commandBuffer = queue.makeCommandBuffer(), let encoder = commandBuffer.makeRenderCommandEncoder(descriptor: pass) else
{
   fatalError("encoder creation failed")
}
encoder.setRenderPipelineState(pipeline)
encoder.setFragmentTexture(source, index: 0)
encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
encoder.endEncoding()
commandBuffer.commit()
commandBuffer.waitUntilCompleted()
guard commandBuffer.status == .completed else {fatalError("Metal decode failed")}
var decoded = [Float](repeating: 0, count: 256 * 4)
output.getBytes(&decoded, bytesPerRow: 256 * 16, from: MTLRegionMake2D(0, 0, 256, 1), mipmapLevel: 0)
let result: [String: Any] = [
   "schema_version": 1,
   "device": device.name,
   "rgba32float_red_bit_patterns": (0..<256).map {decoded[$0 * 4].bitPattern},
]
let data = try JSONSerialization.data(withJSONObject: result, options: [.sortedKeys])
FileHandle.standardOutput.write(data)
FileHandle.standardOutput.write(Data([0x0a]))
