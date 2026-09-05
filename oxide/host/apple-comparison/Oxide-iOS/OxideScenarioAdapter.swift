import Metal
import QuartzCore
import UIKit

@_silgen_name("oxide_comparison_init")
private func oxideComparisonInit(_ width: UInt32, _ height: UInt32, _ scale: Float) -> Int32

@_silgen_name("oxide_comparison_prepare_frame")
private func oxideComparisonPrepareFrame(_ width: UInt32, _ height: UInt32, _ scale: Float) -> Int32

@_silgen_name("oxide_comparison_submit_prepared_frame")
private func oxideComparisonSubmitPreparedFrame(_ drawable: UnsafeMutableRawPointer?) -> Int32

@_silgen_name("oxide_comparison_cancel_prepared_frame")
private func oxideComparisonCancelPreparedFrame()

@_silgen_name("oxide_comparison_shutdown")
private func oxideComparisonShutdown()

@_silgen_name("oxide_comparison_teardown_scenario")
private func oxideComparisonTeardownScenario()

@_silgen_name("oxide_comparison_set_virtual_time_us")
private func oxideComparisonSetVirtualTimeUs(_ timeUs: UInt64) -> Int32

@_silgen_name("oxide_comparison_prepare_scenario")
private func oxideComparisonPrepareScenario(
   _ root: UnsafePointer<UInt8>?,
   _ rootLength: Int,
   _ scenario: UnsafePointer<UInt8>?,
   _ scenarioLength: Int
) -> Int32

@_silgen_name("oxide_comparison_reset_scenario")
private func oxideComparisonResetScenario() -> Int32

@_silgen_name("oxide_comparison_quiesce")
private func oxideComparisonQuiesce() -> Int32

@_silgen_name("oxide_comparison_apply_trace_event")
private func oxideComparisonApplyTraceEvent(_ phase: UnsafePointer<UInt8>?, _ phaseLength: Int, _ eventIndex: Int) -> Int32

@_silgen_name("oxide_comparison_role_count")
private func oxideComparisonRoleCount() -> UInt32

@_silgen_name("oxide_comparison_role")
private func oxideComparisonRole(_ index: UInt32, _ name: UnsafeMutablePointer<UInt8>?, _ nameLength: Int, _ count: UnsafeMutablePointer<UInt32>?) -> UInt32

@_silgen_name("oxide_comparison_checkpoint_json")
private func oxideComparisonCheckpointJSON(
   _ checkpoint: UnsafePointer<UInt8>?,
   _ checkpointLength: Int,
   _ artifact: UInt32,
   _ output: UnsafeMutablePointer<UInt8>?,
   _ outputLength: Int
) -> UInt32

@_silgen_name("oxide_comparison_take_snapshot")
private func oxideComparisonTakeSnapshot() -> Int32

@_silgen_name("oxide_comparison_snapshot_png")
private func oxideComparisonSnapshotPNG(_ output: UnsafeMutablePointer<UInt8>?, _ outputLength: Int) -> Int

@_silgen_name("oxide_comparison_snapshot_status")
private func oxideComparisonSnapshotStatus(_ output: UnsafeMutablePointer<CChar>?, _ outputLength: UInt32) -> UInt32

private final class OxideComparisonMetalView: UIView
{
   override class var layerClass: AnyClass
   {
      CAMetalLayer.self
   }
}

private struct OxideTraceLocation
{
   let phaseBytes: [UInt8]
   let index: Int
}

enum OxideScenarioAdapterFailure: Error
{
   case host(String, Int32)
   case metalView
   case trace(BenchmarkTraceEvent)
}

final class OxideScenarioAdapter: NSObject, BenchmarkScenarioAdapter, BenchmarkPreviewCapture, BenchmarkFrameDrivenAdapter, BenchmarkQuiescenceAdapter, BenchmarkPassConfiguredAdapter, BenchmarkVirtualClockAdapter
{
   private let window: UIWindow
   private let metalView: UIView
   private let layer: CAMetalLayer
   private var scenario: BenchmarkScenario?
   private var eventLocations = [BenchmarkTraceEvent: OxideTraceLocation]()
   private var correctnessOffscreen = false

   init?(window: UIWindow)
   {
      let view = OxideComparisonMetalView(frame: CGRect(x: 0, y: 0, width: 390, height: 844))
      guard let layer = view.layer as? CAMetalLayer else
      {
         return nil
      }
      self.window = window
      metalView = view
      self.layer = layer
      super.init()
   }

   func prepare(scenario: BenchmarkScenario, loader: BenchmarkSpecLoader) throws
   {
      self.scenario = scenario
      eventLocations.removeAll(keepingCapacity: true)
      for phase in scenario.phases
      {
         guard let trace = phase.trace else {continue}
         for (index, event) in try loader.loadTrace(trace).enumerated()
         {
            eventLocations[event] = OxideTraceLocation(phaseBytes: Array(phase.id.utf8), index: index)
         }
      }

      installSurface()
      try require(oxideComparisonInit(1_170, 2_532, 3), operation: "app init")
      let root = Array(loader.root.path.utf8)
      let id = Array(scenario.id.utf8)
      let result = root.withUnsafeBufferPointer
      {
         rootBuffer in
         id.withUnsafeBufferPointer
         {
            idBuffer in
            oxideComparisonPrepareScenario(rootBuffer.baseAddress, rootBuffer.count, idBuffer.baseAddress, idBuffer.count)
         }
      }
      try require(result, operation: "scenario prepare")
      try renderForPass()
   }

   func configure(passID: String)
   {
      correctnessOffscreen = passID == "correctness"
   }

   func setVirtualTimeUs(_ timeUs: UInt64) throws
   {
      try require(oxideComparisonSetVirtualTimeUs(timeUs), operation: "virtual clock")
   }

   func reset() throws
   {
      try require(oxideComparisonResetScenario(), operation: "scenario reset")
      try renderForPass()
   }

   func quiesce() throws
   {
      try renderOffscreenFrame()
      try require(oxideComparisonQuiesce(), operation: "comparison quiescence")
   }

   func apply(event: BenchmarkTraceEvent) throws
   {
      guard let location = eventLocations[event] else
      {
         throw OxideScenarioAdapterFailure.trace(event)
      }
      let result = location.phaseBytes.withUnsafeBufferPointer
      {
         buffer in
         oxideComparisonApplyTraceEvent(buffer.baseAddress, buffer.count, location.index)
      }
      try require(result, operation: "trace event")
      try renderForPass()
   }

   func checkpoint(id: String) throws -> BenchmarkAdapterCheckpoint
   {
      guard scenario != nil else
      {
         throw OxideScenarioAdapterFailure.metalView
      }
      return BenchmarkAdapterCheckpoint(
         state: try checkpointJSON(id: id, artifact: 0),
         accessibility: try checkpointJSON(id: id, artifact: 1),
         visibleRoleCounts: visibleRoleCounts()
      )
   }

   func teardown() throws
   {
      oxideComparisonTeardownScenario()
      metalView.removeFromSuperview()
      scenario = nil
   }

   func previewPNG() throws -> Data
   {
      try renderOffscreenFrame()
      let snapshotResult = oxideComparisonTakeSnapshot()
      if snapshotResult != 0
      {
         let capacity = 1_024
         let buffer = UnsafeMutablePointer<CChar>.allocate(capacity: capacity)
         defer {buffer.deallocate()}
         _ = oxideComparisonSnapshotStatus(buffer, UInt32(capacity))
         try require(snapshotResult, operation: String(cString: buffer))
      }
      let needed = oxideComparisonSnapshotPNG(nil, 0)
      guard needed > 0 else {throw OxideScenarioAdapterFailure.metalView}
      var data = Data(count: needed)
      let copied = data.withUnsafeMutableBytes
      {
         bytes in
         oxideComparisonSnapshotPNG(bytes.bindMemory(to: UInt8.self).baseAddress, bytes.count)
      }
      guard copied == needed else {throw OxideScenarioAdapterFailure.metalView}
      return data
   }

   private func installSurface()
   {
      metalView.frame = CGRect(
         x: (window.bounds.width - 390) * 0.5,
         y: (window.bounds.height - 844) * 0.5,
         width: 390,
         height: 844
      )
      metalView.autoresizingMask = []
      layer.device = MTLCreateSystemDefaultDevice()
      layer.pixelFormat = .bgra8Unorm_srgb
      layer.framebufferOnly = true
      layer.contentsScale = 3
      layer.drawableSize = CGSize(width: 1_170, height: 2_532)
      window.rootViewController?.view.addSubview(metalView)
   }

   func displayTick() throws
   {
      try renderFrame()
   }

   private func renderFrame() throws
   {
      try require(oxideComparisonPrepareFrame(1_170, 2_532, 3), operation: "frame prepare")
      guard let drawable = layer.nextDrawable() else
      {
         oxideComparisonCancelPreparedFrame()
         throw OxideScenarioAdapterFailure.metalView
      }
      try require(
         oxideComparisonSubmitPreparedFrame(Unmanaged.passUnretained(drawable).toOpaque()),
         operation: "frame submit"
      )
   }

   private func renderOffscreenFrame() throws
   {
      try require(oxideComparisonPrepareFrame(1_170, 2_532, 3), operation: "frame prepare")
      try require(oxideComparisonSubmitPreparedFrame(nil), operation: "offscreen frame submit")
   }

   private func renderForPass() throws
   {
      if correctnessOffscreen
      {
         try renderOffscreenFrame()
      }
      else
      {
         try renderFrame()
      }
   }

   private func visibleRoleCounts() -> [BenchmarkRoleCount]
   {
      (0..<oxideComparisonRoleCount()).compactMap
      {
         index in
         let needed = Int(oxideComparisonRole(index, nil, 0, nil))
         guard needed > 1 else {return nil}
         let buffer = UnsafeMutablePointer<UInt8>.allocate(capacity: needed)
         defer {buffer.deallocate()}
         var count: UInt32 = 0
         guard oxideComparisonRole(index, buffer, needed, &count) == needed else {return nil}
         return BenchmarkRoleCount(role: String(cString: UnsafePointer<CChar>(OpaquePointer(buffer))), count: count)
      }
   }

   private func checkpointJSON(id: String, artifact: UInt32) throws -> Data
   {
      let checkpoint = Array(id.utf8)
      let needed = checkpoint.withUnsafeBufferPointer
      {
         oxideComparisonCheckpointJSON($0.baseAddress, $0.count, artifact, nil, 0)
      }
      guard needed > 0 else {throw OxideScenarioAdapterFailure.host("checkpoint evidence", -1)}
      var bytes = [UInt8](repeating: 0, count: Int(needed))
      let written = checkpoint.withUnsafeBufferPointer
      {
         checkpointBuffer in
         bytes.withUnsafeMutableBufferPointer
         {
            outputBuffer in
            oxideComparisonCheckpointJSON(checkpointBuffer.baseAddress, checkpointBuffer.count, artifact, outputBuffer.baseAddress, outputBuffer.count)
         }
      }
      guard written == needed else {throw OxideScenarioAdapterFailure.host("checkpoint evidence", -2)}
      return Data(bytes)
   }

   private func require(_ result: Int32, operation: String) throws
   {
      guard result == 0 else
      {
         throw OxideScenarioAdapterFailure.host(operation, result)
      }
   }
}
