import Metal
import QuartzCore
import UIKit
import os.signpost

@_silgen_name("oxide_core_init")
private func oxideCoreInit() -> Int32
@_silgen_name("oxide_core_draw")
private func oxideCoreDraw(_ drawable: UnsafeMutableRawPointer, _ values: UnsafePointer<Float>, _ count: Int) -> Int32
@_silgen_name("oxide_core_suite_init")
private func oxideCoreSuiteInit(_ name: UnsafePointer<CChar>, _ checkpoint: UInt8) -> Int32
@_silgen_name("oxide_core_suite_draw")
private func oxideCoreSuiteDraw(_ drawable: UnsafeMutableRawPointer, _ time: Double, _ generation: UInt64) -> Int32
@_silgen_name("oxide_core_suite_settle")
private func oxideCoreSuiteSettle()
@_silgen_name("oxide_core_suite_reset")
private func oxideCoreSuiteReset()
@_silgen_name("oxide_core_suite_advance")
private func oxideCoreSuiteAdvance(_ elapsed: Double) -> Int32
@_silgen_name("oxide_core_suite_next_wakeup")
private func oxideCoreSuiteNextWakeup(_ elapsed: Double) -> Double
@_silgen_name("oxide_core_suite_draw_control")
private func oxideCoreSuiteDrawControl(_ drawable: UnsafeMutableRawPointer, _ elapsed: Double, _ generation: UInt64) -> Int32
@_silgen_name("oxide_core_suite_next_control_wakeup")
private func oxideCoreSuiteNextControlWakeup(_ elapsed: Double) -> Double
@_silgen_name("oxide_core_suite_gpu_ms")
private func oxideCoreSuiteGpuMs(_ frameID: UnsafeMutablePointer<UInt64>) -> Double

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
      layer.pixelFormat = .bgra8Unorm
      layer.colorspace = CGColorSpace(name: CGColorSpace.sRGB)
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
      #if !targetEnvironment(simulator)
      drawable.addPresentedHandler
      {
         presented in
         os_signpost(.event, log: log, name: "OxidePresented", "generation=%{public}llu time=%{public}.9f", generation, presented.presentedTime)
      }
      #endif
      let result = values.withUnsafeBufferPointer
      {
         oxideCoreDraw(Unmanaged.passUnretained(drawable).toOpaque(), $0.baseAddress!, $0.count)
      }
      guard result == 0 else {throw CoreProbeError.renderer}
   }
}

final class OxideCoreSuiteAdapter: CoreSuiteAdapter, CoreSuiteInstrumentation
{
   private let layer: CAMetalLayer
   private let caseName: String
   private let log = OSLog(subsystem: "com.oxide.compare-core", category: .pointsOfInterest)
   private var lastGPUFrameID: UInt64 = 0
   private var energyInstrumentationEnabled = true

   init(window: UIWindow, caseName: String, checkpoint: Bool) throws
   {
      self.caseName = caseName
      let view = CoreMetalView(frame: CGRect(x: (window.bounds.width - 390) / 2, y: (window.bounds.height - 844) / 2, width: 390, height: 844))
      layer = view.layer as! CAMetalLayer
      layer.device = MTLCreateSystemDefaultDevice()
      layer.pixelFormat = .bgra8Unorm
      layer.colorspace = CGColorSpace(name: CGColorSpace.sRGB)
      layer.framebufferOnly = true
      layer.contentsScale = 3
      layer.drawableSize = CGSize(width: 1170, height: 2532)
      window.rootViewController?.view.addSubview(view)
      let result = caseName.withCString {oxideCoreSuiteInit($0, checkpoint ? 1 : 0)}
      guard result == 0 else {throw CoreProbeError.renderer}
   }

   func render(time: Double, generation: UInt64) throws
   {
      guard let drawable = layer.nextDrawable() else {throw CoreProbeError.renderer}
      let log = self.log
      #if !targetEnvironment(simulator)
      if energyInstrumentationEnabled
      {
         drawable.addPresentedHandler
         {
            presented in
            guard presented.presentedTime != 0 else {return}
            os_signpost(.event, log: log, name: "CorePresented", "generation=%{public}llu time=%{public}.9f", generation, presented.presentedTime)
         }
      }
      #endif
      let result = oxideCoreSuiteDraw(Unmanaged.passUnretained(drawable).toOpaque(), time, generation)
      guard result == 0 else {throw CoreProbeError.renderer}
      if energyInstrumentationEnabled && time >= 0
      {
         var frameID: UInt64 = 0
         let milliseconds = oxideCoreSuiteGpuMs(&frameID)
         if frameID > 0 && frameID != lastGPUFrameID && milliseconds.isFinite
         {
            lastGPUFrameID = frameID
            os_signpost(.event, log: log, name: "CoreGPU", "frame=%{public}llu ms=%{public}.6f", frameID, milliseconds)
         }
      }
   }

   func reset()
   {
      oxideCoreSuiteReset()
   }

   func advance(time: Double, generation: UInt64) throws -> Bool
   {
      guard caseName.hasPrefix("visual-") else
      {
         if caseName == "text" || caseName == "local"
         {
            return generation > 0
         }
         return true
      }
      let result = oxideCoreSuiteAdvance(time)
      guard result >= 0 else {throw CoreProbeError.renderer}
      return result != 0
   }

   func nextWakeup(after time: Double) -> Double
   {
      if caseName == "text" || caseName == "local"
      {
         return (floor(max(0, time) * 10) + 1) / 10
      }
      guard caseName.hasPrefix("visual-") else {return 0}
      return oxideCoreSuiteNextWakeup(time)
   }

   func renderControl(time: Double, generation: UInt64) throws
   {
      guard caseName.hasPrefix("visual-") else
      {
         try render(time: 0, generation: generation)
         return
      }
      guard let drawable = layer.nextDrawable() else {throw CoreProbeError.renderer}
      let result = oxideCoreSuiteDrawControl(Unmanaged.passUnretained(drawable).toOpaque(), time, generation)
      guard result == 0 else {throw CoreProbeError.renderer}
   }

   func nextControlWakeup(after time: Double) -> Double
   {
      guard caseName.hasPrefix("visual-") else {return .nan}
      return oxideCoreSuiteNextControlWakeup(time)
   }

   func settle()
   {
      oxideCoreSuiteSettle()
   }

   func setEnergyInstrumentationEnabled(_ enabled: Bool)
   {
      energyInstrumentationEnabled = enabled
   }
}

func makeCoreSuiteAdapter(window: UIWindow, caseName: String, checkpoint: Bool) throws -> CoreSuiteAdapter
{
   try OxideCoreSuiteAdapter(window: window, caseName: caseName, checkpoint: checkpoint)
}
