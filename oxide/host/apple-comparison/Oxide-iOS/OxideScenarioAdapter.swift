import Metal
import QuartzCore
import UIKit
import os.signpost

@_silgen_name("oxide_core_init")
private func oxideCoreInit() -> Int32
@_silgen_name("oxide_core_draw")
private func oxideCoreDraw(_ drawable: UnsafeMutableRawPointer, _ values: UnsafePointer<Float>, _ count: Int) -> Int32

private final class CoreMetalView: UIView
{
   override class var layerClass: AnyClass {CAMetalLayer.self}
}

final class OxideScenarioAdapter: CoreProbeAdapter
{
   private let layer: CAMetalLayer
   private let log = OSLog(subsystem: "com.oxide.compare-core", category: .pointsOfInterest)

   init(window: UIWindow) throws
   {
      let view = CoreMetalView(frame: CGRect(x: (window.bounds.width - 390) / 2, y: (window.bounds.height - 844) / 2, width: 390, height: 844))
      layer = view.layer as! CAMetalLayer
      layer.device = MTLCreateSystemDefaultDevice()
      layer.pixelFormat = .bgra8Unorm_srgb
      layer.framebufferOnly = true
      layer.contentsScale = 3
      layer.drawableSize = CGSize(width: 1170, height: 2532)
      window.rootViewController?.view.addSubview(view)
      guard oxideCoreInit() == 0 else {throw CoreProbeError.renderer}
   }

   func render(values: [Float], generation: UInt64) throws
   {
      guard let drawable = layer.nextDrawable() else {throw CoreProbeError.renderer}
      let log = self.log
      drawable.addPresentedHandler
      {
         presented in
         os_signpost(.event, log: log, name: "OxidePresented", "generation=%{public}llu time=%{public}.9f", generation, presented.presentedTime)
      }
      let result = values.withUnsafeBufferPointer
      {
         oxideCoreDraw(Unmanaged.passUnretained(drawable).toOpaque(), $0.baseAddress!, $0.count)
      }
      guard result == 0 else {throw CoreProbeError.renderer}
   }
}
