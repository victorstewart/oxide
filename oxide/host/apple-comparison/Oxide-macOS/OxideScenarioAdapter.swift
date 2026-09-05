import AppKit
import Metal
import QuartzCore

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
private func oxideComparisonPrepareScenario(_ root: UnsafePointer<UInt8>?, _ rootLength: Int, _ scenario: UnsafePointer<UInt8>?, _ scenarioLength: Int) -> Int32

@_silgen_name("oxide_comparison_prepare_release_candidate")
private func oxideComparisonPrepareReleaseCandidate(_ root: UnsafePointer<UInt8>?, _ rootLength: Int, _ scenario: UnsafePointer<UInt8>?, _ scenarioLength: Int) -> Int32

@_silgen_name("oxide_comparison_configure_scale_overlay")
private func oxideComparisonConfigureScaleOverlay(_ json: UnsafePointer<UInt8>?, _ jsonLength: Int) -> Int32

@_silgen_name("oxide_comparison_scale_attestation")
private func oxideComparisonScaleAttestation(_ effectiveCardinality: UnsafeMutablePointer<UInt64>?, _ completed: UnsafeMutablePointer<UInt32>?) -> Int32

@_silgen_name("oxide_comparison_reset_scenario")
private func oxideComparisonResetScenario() -> Int32

@_silgen_name("oxide_comparison_quiesce")
private func oxideComparisonQuiesce() -> Int32

@_silgen_name("oxide_comparison_apply_trace_event")
private func oxideComparisonApplyTraceEvent(_ phase: UnsafePointer<UInt8>?, _ phaseLength: Int, _ eventIndex: Int) -> Int32

@_silgen_name("oxide_comparison_pointer_down")
private func oxideComparisonPointerDown(_ x: Float, _ y: Float, _ timestampSeconds: Double) -> Int32

@_silgen_name("oxide_comparison_launch_probe_generation")
private func oxideComparisonLaunchProbeGeneration() -> UInt64

@_silgen_name("oxide_comparison_interaction_generation")
private func oxideComparisonInteractionGeneration() -> UInt64

@_silgen_name("oxide_comparison_macos_pointer_event")
private func oxideComparisonMacOSPointerEvent(_ kind: UInt32, _ x: Float, _ y: Float, _ timestampSeconds: Double) -> Int32

@_silgen_name("oxide_comparison_macos_scroll")
private func oxideComparisonMacOSScroll(_ deltaYPoints: Float) -> Int32

@_silgen_name("oxide_comparison_macos_key")
private func oxideComparisonMacOSKey(_ keyCode: UInt16, _ shift: Int32, _ value: UnsafePointer<UInt8>?, _ valueLength: Int) -> Int32

@_silgen_name("oxide_comparison_role_count")
private func oxideComparisonRoleCount() -> UInt32

@_silgen_name("oxide_comparison_role")
private func oxideComparisonRole(_ index: UInt32, _ name: UnsafeMutablePointer<UInt8>?, _ nameLength: Int, _ count: UnsafeMutablePointer<UInt32>?) -> UInt32

@_silgen_name("oxide_comparison_checkpoint_json")
private func oxideComparisonCheckpointJSON(_ checkpoint: UnsafePointer<UInt8>?, _ checkpointLength: Int, _ artifact: UInt32, _ output: UnsafeMutablePointer<UInt8>?, _ outputLength: Int) -> UInt32

@_silgen_name("oxide_comparison_geometry_nodes_json")
private func oxideComparisonGeometryNodesJSON(_ output: UnsafeMutablePointer<UInt8>?, _ outputLength: Int) -> UInt32

@_silgen_name("oxide_comparison_take_snapshot")
private func oxideComparisonTakeSnapshot() -> Int32

@_silgen_name("oxide_comparison_snapshot_png")
private func oxideComparisonSnapshotPNG(_ output: UnsafeMutablePointer<UInt8>?, _ outputLength: Int) -> Int

@_silgen_name("oxide_comparison_snapshot_status")
private func oxideComparisonSnapshotStatus(_ output: UnsafeMutablePointer<CChar>?, _ outputLength: UInt32) -> UInt32

@_silgen_name("oxide_comparison_set_renderer_diagnostics_enabled")
private func oxideComparisonSetRendererDiagnosticsEnabled(_ enabled: Int32)

@_silgen_name("oxide_comparison_renderer_diagnostics")
private func oxideComparisonRendererDiagnostics(_ output: UnsafeMutablePointer<OxideComparisonRendererDiagnosticsRaw>?) -> Int32

private struct OxideComparisonRendererDiagnosticsRaw
{
   var schemaVersion: UInt32 = 0
   var reserved: UInt32 = 0
   var submittedFrameID: UInt64 = 0
   var completedFrameID: UInt64 = 0
   var renderPrepareBeginTicks: UInt64 = 0
   var renderPrepareEndTicks: UInt64 = 0
   var encodeBeginTicks: UInt64 = 0
   var encodeEndTicks: UInt64 = 0
   var commandSubmitTicks: UInt64 = 0
   var encodedBytes: UInt64 = 0
   var drawCalls: UInt64 = 0
   var damagePixels: UInt64 = 0
   var damageRects: UInt64 = 0
   var gpuDurationNs: UInt64 = 0
   var gpuRenderDurationNs: UInt64 = 0
}

private struct OxideTraceLocation
{
   let phaseBytes: [UInt8]
   let index: Int
}

enum OxideMacScenarioAdapterFailure: Error
{
   case host(String, Int32)
   case metalView
   case trace(BenchmarkTraceEvent)
}

private final class OxideComparisonMetalView: NSView
{
   var pointerDown: ((CGPoint, TimeInterval) -> Void)?
   var stateChanged: (() -> Void)?

   override var isFlipped: Bool {true}

   override var acceptsFirstResponder: Bool {true}

   override func makeBackingLayer() -> CALayer
   {
      CAMetalLayer()
   }

   override init(frame frameRect: NSRect)
   {
      super.init(frame: frameRect)
      wantsLayer = true
   }

   required init?(coder: NSCoder)
   {
      nil
   }

   override func mouseDown(with event: NSEvent)
   {
      window?.makeFirstResponder(self)
      let point = convert(event.locationInWindow, from: nil)
      pointerDown?(point, event.timestamp)
      accept(oxideComparisonMacOSPointerEvent(0, Float(point.x), Float(point.y), event.timestamp))
   }

   override func mouseDragged(with event: NSEvent)
   {
      let point = convert(event.locationInWindow, from: nil)
      accept(oxideComparisonMacOSPointerEvent(1, Float(point.x), Float(point.y), event.timestamp))
   }

   override func mouseUp(with event: NSEvent)
   {
      let point = convert(event.locationInWindow, from: nil)
      accept(oxideComparisonMacOSPointerEvent(2, Float(point.x), Float(point.y), event.timestamp))
   }

   override func scrollWheel(with event: NSEvent)
   {
      accept(oxideComparisonMacOSScroll(Float(event.scrollingDeltaY)))
   }

   override func keyDown(with event: NSEvent)
   {
      let blockedModifiers: NSEvent.ModifierFlags = [.command, .control, .option]
      guard event.modifierFlags.intersection(blockedModifiers).isEmpty,
            let characters = event.characters,
            !characters.isEmpty else
      {
         super.keyDown(with: event)
         return
      }
      let bytes = Array(characters.utf8)
      let result = bytes.withUnsafeBufferPointer
      {
         buffer in
         oxideComparisonMacOSKey(event.keyCode, event.modifierFlags.contains(.shift) ? 1 : 0, buffer.baseAddress, buffer.count)
      }
      if result != 1
      {
         super.keyDown(with: event)
      }
      else
      {
         accept(result)
      }
   }

   func updateAccessibility(nodes: [BenchmarkMacOSCorrectnessGeometryNode])
   {
      var children = nodes.map
      {
         node -> NSAccessibilityElement in
         let element = NSAccessibilityElement()
         let local = NSRect(x: CGFloat(node.bounds.x), y: CGFloat(node.bounds.y), width: CGFloat(node.bounds.width), height: CGFloat(node.bounds.height))
         let windowRect = convert(local, to: nil)
         let screenRect = window?.convertToScreen(windowRect) ?? windowRect
         element.setAccessibilityElement(true)
         element.setAccessibilityParent(self)
         element.setAccessibilityIdentifier(node.identifier ?? "")
         element.setAccessibilityLabel(node.role)
         element.setAccessibilityRole(accessibilityRole(for: node.role))
         element.setAccessibilityFrame(screenRect)
         element.setAccessibilityEnabled(true)
         return element
      }
      if let detail = nodes.first(where: {$0.identifier == "navigation.detail"}),
         let element = accessibilityElement(node: detail, identifier: "navigation.modal-action", role: .button)
      {
         children.append(element)
      }
      setAccessibilityElement(false)
      setAccessibilityChildren(children)
   }

   private func accept(_ result: Int32)
   {
      if result == 1
      {
         stateChanged?()
      }
   }

   private func accessibilityRole(for role: String) -> NSAccessibility.Role
   {
      if role.contains("control") || role == "list-item" || role == "grid-tile"
      {
         return .button
      }
      if role == "composer"
      {
         return .textField
      }
      if role.contains("image") || role == "thumbnail" || role == "avatar"
      {
         return .image
      }
      if role == "feed" || role == "chat-thread" || role == "thumbnail-grid"
      {
         return .scrollArea
      }
      return .group
   }

   private func accessibilityElement(node: BenchmarkMacOSCorrectnessGeometryNode, identifier: String, role: NSAccessibility.Role) -> NSAccessibilityElement?
   {
      guard window != nil else {return nil}
      let local = NSRect(x: CGFloat(node.bounds.x), y: CGFloat(node.bounds.y), width: CGFloat(node.bounds.width), height: CGFloat(node.bounds.height))
      let element = NSAccessibilityElement()
      element.setAccessibilityElement(true)
      element.setAccessibilityParent(self)
      element.setAccessibilityIdentifier(identifier)
      element.setAccessibilityLabel(node.role)
      element.setAccessibilityRole(role)
      element.setAccessibilityFrame(window?.convertToScreen(convert(local, to: nil)) ?? local)
      element.setAccessibilityEnabled(true)
      return element
   }
}

final class OxideMacScenarioAdapter: NSObject, BenchmarkScenarioAdapter, BenchmarkReleaseCandidateScenarioAdapter, BenchmarkPreviewCapture, BenchmarkMacOSCorrectnessGeometryCapture, BenchmarkFrameDrivenAdapter, BenchmarkQuiescenceAdapter, BenchmarkOutputReadyAdapter, BenchmarkPassConfiguredAdapter, BenchmarkMacOSScaleConfiguredAdapter, BenchmarkVirtualClockAdapter, BenchmarkDisplayLinkProvider, BenchmarkRendererDiagnosticsAdapter, BenchmarkMacOSSurfaceProvider, MacOSCanonicalLaunchResponseSource
{
   private let window: NSWindow
   private let metalView: OxideComparisonMetalView
   private let layer: CAMetalLayer
   private var scenario: BenchmarkScenario?
   private var eventLocations = [BenchmarkTraceEvent: OxideTraceLocation]()
   private var scaleOverlayData: Data?
   private var correctnessOffscreen = false
   private var logicalSize = NSSize(width: 390, height: 844)
   private(set) var rendererDiagnosticsEnabled = false
   private var lastDiagnosticSubmittedFrameID = UInt64(0)
   private var lastDiagnosticCompletedFrameID = UInt64(0)
   private var drawableWaitBeginTicks = UInt64(0)
   private var drawableWaitEndTicks = UInt64(0)
   private var canonicalLaunchObserver: (() -> Void)?
   private var presentedInteractionGeneration = UInt64(0)
   private var inputPresentationFailure: Error?
   private var nativeScale: CGFloat {max(window.backingScaleFactor, 1)}
   var onCanonicalLaunchResponsePresented: ((Result<Void, Error>) -> Void)?

   var stateGeneration: UInt64
   {
      presentedInteractionGeneration
   }

   init?(window: NSWindow)
   {
      let view = OxideComparisonMetalView(frame: NSRect(x: 0, y: 0, width: 390, height: 844))
      guard let layer = view.layer as? CAMetalLayer else {return nil}
      self.window = window
      metalView = view
      self.layer = layer
      super.init()
      metalView.stateChanged =
      {
         [weak self] in
         guard let self else {return}
         do
         {
            try self.renderFrame()
            try self.refreshAccessibility()
            self.presentedInteractionGeneration = oxideComparisonInteractionGeneration()
            self.inputPresentationFailure = nil
         }
         catch
         {
            self.inputPresentationFailure = error
         }
      }
   }

   func prepare(scenario: BenchmarkScenario, loader: BenchmarkSpecLoader) throws
   {
      try prepare(scenario: scenario, loader: loader, releaseCandidate: false)
   }

   func prepareReleaseCandidate(scenario: BenchmarkScenario, loader: BenchmarkSpecLoader) throws
   {
      try prepare(scenario: scenario, loader: loader, releaseCandidate: true)
   }

   private func prepare(scenario: BenchmarkScenario, loader: BenchmarkSpecLoader, releaseCandidate: Bool) throws
   {
      self.scenario = scenario
      logicalSize = NSSize(width: 390, height: 844)
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
      let dimensions = renderDimensions
      try require(oxideComparisonInit(dimensions.width, dimensions.height, dimensions.scale), operation: "app init")
      if let scaleOverlayData
      {
         let result = scaleOverlayData.withUnsafeBytes
         {
            buffer in
            oxideComparisonConfigureScaleOverlay(buffer.bindMemory(to: UInt8.self).baseAddress, buffer.count)
         }
         try require(result, operation: "scale overlay")
      }
      let root = Array(loader.root.path.utf8)
      let id = Array(scenario.id.utf8)
      let result = root.withUnsafeBufferPointer
      {
         rootBuffer in
         id.withUnsafeBufferPointer
         {
            idBuffer in
            if releaseCandidate
            {
               return oxideComparisonPrepareReleaseCandidate(rootBuffer.baseAddress, rootBuffer.count, idBuffer.baseAddress, idBuffer.count)
            }
            return oxideComparisonPrepareScenario(rootBuffer.baseAddress, rootBuffer.count, idBuffer.baseAddress, idBuffer.count)
         }
      }
      try require(result, operation: releaseCandidate ? "release candidate prepare" : "scenario prepare")
      try renderForPass()
      try refreshAccessibility()
      presentedInteractionGeneration = oxideComparisonInteractionGeneration()
      inputPresentationFailure = nil
   }

   func configure(scaleOverlay: BenchmarkMacOSComparatorScaleOverlay, identity: BenchmarkArtifactIdentity) throws
   {
      try validateComparisonSHA256(identity.sha256)
      let encoder = JSONEncoder()
      encoder.keyEncodingStrategy = .convertToSnakeCase
      encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
      scaleOverlayData = try encoder.encode(scaleOverlay)
   }

   func scaleRuntimeAttestation() throws -> (effectiveCardinality: UInt64, completed: Bool)
   {
      var cardinality = UInt64(0)
      var completed = UInt32(0)
      guard oxideComparisonScaleAttestation(&cardinality, &completed) == 1 else
      {
         throw OxideMacScenarioAdapterFailure.host("scale attestation", -1)
      }
      return (cardinality, completed == 1)
   }

   func configure(passID: String)
   {
      correctnessOffscreen = passID == "correctness"
      rendererDiagnosticsEnabled = passID == "common-gpu" || passID == "full-attribution"
      lastDiagnosticSubmittedFrameID = 0
      lastDiagnosticCompletedFrameID = 0
      oxideComparisonSetRendererDiagnosticsEnabled(rendererDiagnosticsEnabled ? 1 : 0)
      if passID != "canonical-launch"
      {
         disarmTrustedInputProbe()
      }
   }

   func setVirtualTimeUs(_ timeUs: UInt64) throws
   {
      try require(oxideComparisonSetVirtualTimeUs(timeUs), operation: "virtual clock")
   }

   func reset() throws
   {
      try require(oxideComparisonResetScenario(), operation: "scenario reset")
      logicalSize = NSSize(width: 390, height: 844)
      resizeSurface()
      try renderForPass()
      try refreshAccessibility()
      presentedInteractionGeneration = oxideComparisonInteractionGeneration()
      inputPresentationFailure = nil
   }

   func quiesce() throws
   {
      try renderOffscreenFrame(dimensions: renderDimensions)
      try require(oxideComparisonQuiesce(), operation: "comparison quiescence")
   }

   func outputReady() throws
   {
      // `apply(event:)` has already encoded and committed the selected frame.
   }

   func apply(event: BenchmarkTraceEvent) throws
   {
      guard let location = eventLocations[event] else {throw OxideMacScenarioAdapterFailure.trace(event)}
      let result = location.phaseBytes.withUnsafeBufferPointer
      {
         buffer in
         oxideComparisonApplyTraceEvent(buffer.baseAddress, buffer.count, location.index)
      }
      try require(result, operation: "trace event")
      try applyViewportChange(event)
      try renderForPass()
   }

   func checkpoint(id: String) throws -> BenchmarkAdapterCheckpoint
   {
      guard scenario != nil else {throw OxideMacScenarioAdapterFailure.metalView}
      if let inputPresentationFailure {throw inputPresentationFailure}
      return BenchmarkAdapterCheckpoint(
         state: try checkpointJSON(id: id, artifact: 0),
         accessibility: try checkpointJSON(id: id, artifact: 1),
         visibleRoleCounts: visibleRoleCounts()
      )
   }

   func teardown() throws
   {
      disarmTrustedInputProbe()
      oxideComparisonTeardownScenario()
      window.contentView = NSView(frame: NSRect(x: 0, y: 0, width: 390, height: 844))
      logicalSize = NSSize(width: 390, height: 844)
      window.contentMinSize = logicalSize
      window.contentMaxSize = logicalSize
      window.setContentSize(logicalSize)
      scenario = nil
      eventLocations.removeAll(keepingCapacity: false)
   }

   func armTrustedInputProbe(_ observer: @escaping () -> Void) throws -> MacOSCanonicalLaunchProbeDescriptor
   {
      canonicalLaunchObserver = observer
      metalView.pointerDown =
      {
         [weak self] point, timestamp in
         guard let self, let observer = self.canonicalLaunchObserver else {return}
         guard oxideComparisonPointerDown(Float(point.x), Float(point.y), timestamp) == 1 else {return}
         observer()
         DispatchQueue.main.async
         {
            do
            {
               try self.renderFrame()
               try self.refreshAccessibility()
               self.presentedInteractionGeneration = oxideComparisonInteractionGeneration()
               self.onCanonicalLaunchResponsePresented?(.success(()))
            }
            catch
            {
               self.onCanonicalLaunchResponsePresented?(.failure(error))
            }
         }
      }
      return try macOSCanonicalLaunchProbeDescriptor(
         view: metalView,
         point: CGPoint(x: 195, y: 800),
         probeID: "startup-primary-control",
         targetIdentity: "OxideComparisonMetalView",
         actionIdentity: "oxide_comparison_pointer_down"
      )
   }

   func disarmTrustedInputProbe()
   {
      canonicalLaunchObserver = nil
      onCanonicalLaunchResponsePresented = nil
      metalView.pointerDown = nil
   }

   func previewPNG() throws -> Data
   {
      let scale = BenchmarkMacOSCorrectnessCaptureProfile.canonical.canonicalScale
      let canonical = (
         width: UInt32((logicalSize.width * CGFloat(scale)).rounded()),
         height: UInt32((logicalSize.height * CGFloat(scale)).rounded()),
         scale: Float(scale)
      )
      try renderOffscreenFrame(dimensions: canonical)
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
      guard needed > 0 else {throw OxideMacScenarioAdapterFailure.metalView}
      var data = Data(count: needed)
      let copied = data.withUnsafeMutableBytes
      {
         bytes in
         oxideComparisonSnapshotPNG(bytes.bindMemory(to: UInt8.self).baseAddress, bytes.count)
      }
      guard copied == needed else {throw OxideMacScenarioAdapterFailure.metalView}
      return data
   }

   func correctnessGeometry() throws -> BenchmarkMacOSCorrectnessGeometryEvidence
   {
      guard scenario != nil else {throw OxideMacScenarioAdapterFailure.metalView}
      let scale = BenchmarkMacOSCorrectnessCaptureProfile.canonical.canonicalScale
      try renderOffscreenFrame(dimensions: (
         width: UInt32((logicalSize.width * CGFloat(scale)).rounded()),
         height: UInt32((logicalSize.height * CGFloat(scale)).rounded()),
         scale: Float(scale)
      ))
      let needed = Int(oxideComparisonGeometryNodesJSON(nil, 0))
      guard needed > 0 else {throw OxideMacScenarioAdapterFailure.host("runtime geometry", -1)}
      var bytes = [UInt8](repeating: 0, count: needed)
      let written = bytes.withUnsafeMutableBufferPointer
      {
         buffer in
         oxideComparisonGeometryNodesJSON(buffer.baseAddress, buffer.count)
      }
      guard written == needed else {throw OxideMacScenarioAdapterFailure.host("runtime geometry", -2)}
      let decoder = JSONDecoder()
      decoder.keyDecodingStrategy = .convertFromSnakeCase
      let nodes = try decoder.decode([BenchmarkMacOSCorrectnessGeometryNode].self, from: Data(bytes))
      return try benchmarkMacOSCorrectnessGeometry(rootBounds: metalView.bounds, source: "oxide-semantic-tree", nodes: nodes)
   }

   func displayTick() throws
   {
      try renderFrame()
   }

   func makeDisplayLink(target: Any, selector: Selector) -> CADisplayLink
   {
      window.displayLink(target: target, selector: selector)
   }

   func macOSSurfaceSnapshot() throws -> BenchmarkMacOSSurfaceSnapshot
   {
      guard let screen = window.screen else {throw OxideMacScenarioAdapterFailure.metalView}
      let viewport = metalView.bounds
      let backing = metalView.convertToBacking(viewport)
      let insets = metalView.safeAreaInsets
      let scale = window.backingScaleFactor
      return BenchmarkMacOSSurfaceSnapshot(
         windowLogicalWidthMilliPoints: UInt64((window.frame.width * 1_000).rounded()),
         windowLogicalHeightMilliPoints: UInt64((window.frame.height * 1_000).rounded()),
         viewportLogicalWidthMilliPoints: UInt64((viewport.width * 1_000).rounded()),
         viewportLogicalHeightMilliPoints: UInt64((viewport.height * 1_000).rounded()),
         backingPixelWidth: UInt64(layer.drawableSize.width.rounded()),
         backingPixelHeight: UInt64(layer.drawableSize.height.rounded()),
         insetTopMilliPoints: UInt64((insets.top * 1_000).rounded()),
         insetLeftMilliPoints: UInt64((insets.left * 1_000).rounded()),
         insetBottomMilliPoints: UInt64((insets.bottom * 1_000).rounded()),
         insetRightMilliPoints: UInt64((insets.right * 1_000).rounded()),
         backingScaleMilli: UInt64((scale * 1_000).rounded()),
         finalColorFormat: "srgb8-opaque-window-composite",
         finalColorSpace: window.colorSpace?.localizedName ?? screen.colorSpace?.localizedName ?? "unknown",
         alphaMode: window.isOpaque ? "opaque" : "premultiplied",
         sampleCount: 1,
         compositorScaling: backing.width == layer.drawableSize.width && backing.height == layer.drawableSize.height ? "none-1:1-backing-pixels" : "scaled",
         targetRefreshPolicy: "native-default",
         targetRefreshMillihz: UInt64(screen.maximumFramesPerSecond) * 1_000,
         surfaceImplementation: "oxide-cametallayer",
         internalColorFormat: layer.pixelFormat == .bgra8Unorm_srgb ? "bgra8unorm-srgb" : "other-metal-pixel-format"
      )
   }

   func rendererDiagnostics() throws -> BenchmarkRendererDiagnostics?
   {
      guard rendererDiagnosticsEnabled else {return nil}
      var raw = OxideComparisonRendererDiagnosticsRaw()
      let result = oxideComparisonRendererDiagnostics(&raw)
      guard result >= 0 else {throw OxideMacScenarioAdapterFailure.host("renderer diagnostics", result)}
      guard result == 1, raw.schemaVersion == 1 else {return nil}
      let submittedChanged = raw.submittedFrameID > lastDiagnosticSubmittedFrameID
      let completedChanged = raw.completedFrameID > lastDiagnosticCompletedFrameID
      guard submittedChanged || completedChanged else {return nil}
      if submittedChanged && lastDiagnosticSubmittedFrameID > 0 && raw.submittedFrameID != lastDiagnosticSubmittedFrameID + 1
      {
         throw OxideMacScenarioAdapterFailure.host("noncontiguous renderer submission diagnostics", -5)
      }
      if completedChanged && lastDiagnosticCompletedFrameID > 0 && raw.completedFrameID != lastDiagnosticCompletedFrameID + 1
      {
         throw OxideMacScenarioAdapterFailure.host("noncontiguous renderer completion diagnostics", -6)
      }
      if submittedChanged {lastDiagnosticSubmittedFrameID = raw.submittedFrameID}
      if completedChanged {lastDiagnosticCompletedFrameID = raw.completedFrameID}
      return BenchmarkRendererDiagnostics(
         submittedFrameID: submittedChanged ? raw.submittedFrameID : 0,
         completedFrameID: completedChanged ? raw.completedFrameID : 0,
         renderPrepareBeginTicks: submittedChanged ? raw.renderPrepareBeginTicks : 0,
         renderPrepareEndTicks: submittedChanged ? raw.renderPrepareEndTicks : 0,
         encodeBeginTicks: submittedChanged ? raw.encodeBeginTicks : 0,
         encodeEndTicks: submittedChanged ? raw.encodeEndTicks : 0,
         commandSubmitTicks: submittedChanged ? raw.commandSubmitTicks : 0,
         drawableWaitBeginTicks: submittedChanged ? drawableWaitBeginTicks : 0,
         drawableWaitEndTicks: submittedChanged ? drawableWaitEndTicks : 0,
         encodedBytes: submittedChanged ? raw.encodedBytes : 0,
         drawCalls: submittedChanged ? raw.drawCalls : 0,
         damagePixels: submittedChanged ? raw.damagePixels : 0,
         damageRects: submittedChanged ? raw.damageRects : 0,
         gpuDurationNs: completedChanged ? raw.gpuDurationNs : 0,
         gpuRenderDurationNs: completedChanged ? raw.gpuRenderDurationNs : 0
      )
   }

   private var renderDimensions: (width: UInt32, height: UInt32, scale: Float)
   {
      if correctnessOffscreen
      {
         let scale = BenchmarkMacOSCorrectnessCaptureProfile.canonical.canonicalScale
         return (
            UInt32((logicalSize.width * CGFloat(scale)).rounded()),
            UInt32((logicalSize.height * CGFloat(scale)).rounded()),
            Float(scale)
         )
      }
      let scale = nativeScale
      return (
         UInt32((logicalSize.width * scale).rounded()),
         UInt32((logicalSize.height * scale).rounded()),
         Float(scale)
      )
   }

   private func installSurface()
   {
      metalView.autoresizingMask = []
      layer.device = MTLCreateSystemDefaultDevice()
      layer.pixelFormat = .bgra8Unorm_srgb
      layer.framebufferOnly = true
      window.contentView = metalView
      resizeSurface()
      window.makeKeyAndOrderFront(nil)
   }

   private func refreshAccessibility() throws
   {
      let needed = Int(oxideComparisonGeometryNodesJSON(nil, 0))
      guard needed > 0 else {throw OxideMacScenarioAdapterFailure.host("runtime accessibility geometry", -1)}
      var bytes = [UInt8](repeating: 0, count: needed)
      let written = bytes.withUnsafeMutableBufferPointer
      {
         buffer in
         oxideComparisonGeometryNodesJSON(buffer.baseAddress, buffer.count)
      }
      guard written == needed else {throw OxideMacScenarioAdapterFailure.host("runtime accessibility geometry", -2)}
      let decoder = JSONDecoder()
      decoder.keyDecodingStrategy = .convertFromSnakeCase
      metalView.updateAccessibility(nodes: try decoder.decode([BenchmarkMacOSCorrectnessGeometryNode].self, from: Data(bytes)))
   }

   private func resizeSurface()
   {
      metalView.frame = NSRect(origin: .zero, size: logicalSize)
      window.contentMinSize = logicalSize
      window.contentMaxSize = logicalSize
      window.setContentSize(logicalSize)
      let scale = nativeScale
      layer.contentsScale = scale
      layer.drawableSize = CGSize(width: logicalSize.width * scale, height: logicalSize.height * scale)
   }

   private func applyViewportChange(_ event: BenchmarkTraceEvent) throws
   {
      guard scenario?.id == "resize.theme", event.op == "resize", event.target == "resize:viewport" else {return}
      guard case .text(let value)? = event.value else {throw OxideMacScenarioAdapterFailure.trace(event)}
      let components = value.split(separator: "x", omittingEmptySubsequences: false)
      guard components.count == 2,
            let width = Double(components[0]), width > 0,
            let height = Double(components[1]), height > 0 else
      {
         throw OxideMacScenarioAdapterFailure.trace(event)
      }
      logicalSize = NSSize(width: width, height: height)
      resizeSurface()
   }

   private func renderFrame() throws
   {
      let dimensions = renderDimensions
      layer.contentsScale = CGFloat(dimensions.scale)
      layer.drawableSize = CGSize(width: Int(dimensions.width), height: Int(dimensions.height))
      try require(oxideComparisonPrepareFrame(dimensions.width, dimensions.height, dimensions.scale), operation: "frame prepare")
      let drawableWaitBeginTicks = rendererDiagnosticsEnabled ? mach_continuous_time() : 0
      guard let drawable = layer.nextDrawable() else
      {
         oxideComparisonCancelPreparedFrame()
         throw OxideMacScenarioAdapterFailure.metalView
      }
      if rendererDiagnosticsEnabled
      {
         self.drawableWaitBeginTicks = drawableWaitBeginTicks
         drawableWaitEndTicks = mach_continuous_time()
      }
      try require(oxideComparisonSubmitPreparedFrame(Unmanaged.passUnretained(drawable).toOpaque()), operation: "frame submit")
   }

   private func renderOffscreenFrame(dimensions: (width: UInt32, height: UInt32, scale: Float)) throws
   {
      try require(oxideComparisonPrepareFrame(dimensions.width, dimensions.height, dimensions.scale), operation: "frame prepare")
      try require(oxideComparisonSubmitPreparedFrame(nil), operation: "offscreen frame submit")
   }

   private func renderForPass() throws
   {
      if correctnessOffscreen
      {
         try renderOffscreenFrame(dimensions: renderDimensions)
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
      guard needed > 0 else {throw OxideMacScenarioAdapterFailure.host("checkpoint evidence", -1)}
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
      guard written == needed else {throw OxideMacScenarioAdapterFailure.host("checkpoint evidence", -2)}
      return Data(bytes)
   }

   private func require(_ result: Int32, operation: String) throws
   {
      guard result == 0 else {throw OxideMacScenarioAdapterFailure.host(operation, result)}
   }
}
