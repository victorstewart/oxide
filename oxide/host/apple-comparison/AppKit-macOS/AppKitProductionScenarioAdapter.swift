import AppKit
import Darwin
import Foundation
import Metal

private final class AppKitProductionCanonicalSRGBEncoder
{
   let width: Int
   let height: Int
   private let queue: MTLCommandQueue
   private let pipeline: MTLRenderPipelineState
   private let source: MTLTexture
   private let output: MTLTexture

   init?(width: Int, height: Int)
   {
      guard let device = MTLCreateSystemDefaultDevice(),
            let queue = device.makeCommandQueue() else
      {
         return nil
      }
      let shader = """
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
      guard let library = try? device.makeLibrary(source: shader, options: nil),
            let vertex = library.makeFunction(name: "canonical_vertex"),
            let fragment = library.makeFunction(name: "canonical_fragment") else
      {
         return nil
      }
      let pipelineDescriptor = MTLRenderPipelineDescriptor()
      pipelineDescriptor.vertexFunction = vertex
      pipelineDescriptor.fragmentFunction = fragment
      pipelineDescriptor.colorAttachments[0].pixelFormat = .bgra8Unorm_srgb
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
      guard let pipeline = try? device.makeRenderPipelineState(descriptor: pipelineDescriptor),
            let source = device.makeTexture(descriptor: sourceDescriptor),
            let output = device.makeTexture(descriptor: outputDescriptor) else
      {
         return nil
      }
      self.width = width
      self.height = height
      self.queue = queue
      self.pipeline = pipeline
      self.source = source
      self.output = output
   }

   func encode(linearContext: CGContext) -> Data?
   {
      guard let sourceData = linearContext.data,
            let commandBuffer = queue.makeCommandBuffer() else {return nil}
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
         guard let baseAddress = $0.baseAddress else {return}
         output.getBytes(
            baseAddress,
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
               bitmapInfo: CGBitmapInfo.byteOrder32Little.union(CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipFirst.rawValue)),
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

private final class AppKitProductionPreviewSurface
{
   let width: Int
   let height: Int
   let bytesPerRow: Int
   let byteCount: Int
   private let memory: UnsafeMutableRawPointer
   private var retainedContext: CGContext?

   var context: CGContext
   {
      retainedContext!
   }

   init(width: Int, height: Int) throws
   {
      let bytesPerRow = width * 16
      let byteCount = bytesPerRow * height
      let memory = mmap(nil, byteCount, PROT_READ | PROT_WRITE, MAP_ANON | MAP_PRIVATE, -1, 0)
      guard memory != MAP_FAILED,
            let colorSpace = CGColorSpace(name: CGColorSpace.linearSRGB),
            let context = CGContext(
               data: memory,
               width: width,
               height: height,
               bitsPerComponent: 32,
               bytesPerRow: bytesPerRow,
               space: colorSpace,
               bitmapInfo: CGBitmapInfo.floatComponents.rawValue
                  | CGBitmapInfo.byteOrder32Little.rawValue
                  | CGImageAlphaInfo.noneSkipLast.rawValue
            ) else
      {
         if memory != MAP_FAILED
         {
            munmap(memory, byteCount)
         }
         throw AppKitProductionScenarioFailure.missingScenario
      }
      self.width = width
      self.height = height
      self.bytesPerRow = bytesPerRow
      self.byteCount = byteCount
      self.memory = memory!
      retainedContext = context
      context.setShouldAntialias(true)
      context.setAllowsAntialiasing(true)
      context.setShouldSmoothFonts(false)
      context.setAllowsFontSmoothing(false)
      context.setShouldSubpixelPositionFonts(true)
      context.setAllowsFontSubpixelPositioning(true)
      context.setShouldSubpixelQuantizeFonts(true)
      context.setAllowsFontSubpixelQuantization(true)
   }

   deinit
   {
      retainedContext = nil
      munmap(memory, byteCount)
   }
}

final class AppKitProductionScenarioAdapter: BenchmarkScenarioAdapter, BenchmarkReleaseCandidateScenarioAdapter, BenchmarkPreviewCapture, BenchmarkRawAccessibilityCapture, BenchmarkMacOSCorrectnessGeometryCapture, BenchmarkFrameDrivenAdapter, BenchmarkQuiescenceAdapter, BenchmarkOutputReadyAdapter, BenchmarkRetiredResourceAdapter, BenchmarkPassConfiguredAdapter, BenchmarkMacOSScaleConfiguredAdapter, BenchmarkVirtualClockAdapter, BenchmarkDisplayLinkProvider, BenchmarkMacOSSurfaceProvider, MacOSCanonicalLaunchResponseSource
{
   private static let minimumRetiredResourceDrainTurns = 6

   private let window: NSWindow
   private let host = AppKitProductionSceneHost()
   private var scenario: BenchmarkScenario?
   private var model: AppKitProductionScenarioModel?
   private var scaleShadowModel: AppKitProductionScenarioModel?
   private var scaleOverlay: BenchmarkMacOSComparatorScaleOverlay?
   private var previewSurface: AppKitProductionPreviewSurface?
   private var canonicalSRGBEncoder: AppKitProductionCanonicalSRGBEncoder?
   private weak var retiredModel: AppKitProductionScenarioModel?
   private weak var retiredScene: AnyObject?
   private var canonicalLaunchObserver: (() -> Void)?
   private var canonicalLaunchStateGeneration = UInt64(0)
   private var measuredInteractionGeneration = UInt64(0)
   private var configuredPassID = "correctness"
   private var suppressMeasuredInteractionObservation = false
   private var feedBoundsObserver: NSObjectProtocol?
   private var requireNativeUserDispatch = false
   private(set) var previewSurfaceAllocationCount = 0
   private(set) var retiredResourceDrainTurnCount = 0
   private(set) var lastEventDispatchRoute = AppKitProductionEventDispatchRoute.applicationUpdate
   var onCanonicalLaunchResponsePresented: ((Result<Void, Error>) -> Void)?

   var stateGeneration: UInt64
   {
      configuredPassID == "canonical-launch" ? canonicalLaunchStateGeneration : measuredInteractionGeneration
   }

   var hasPreviewSurface: Bool
   {
      previewSurface != nil
   }

   var hasRetiredResources: Bool
   {
      retiredModel != nil || retiredScene != nil
   }

   var retiredResourceKinds: [String]
   {
      var kinds = [String]()
      if retiredModel != nil {kinds.append("model")}
      if retiredScene != nil {kinds.append("scene")}
      return kinds
   }

   init(window: NSWindow)
   {
      self.window = window
   }

   func configure(passID: String)
   {
      configuredPassID = passID
      measuredInteractionGeneration = 0
      requireNativeUserDispatch = passID != "correctness" && passID != "canonical-launch"
      if passID != "canonical-launch"
      {
         disarmTrustedInputProbe()
      }
   }

   func configure(scaleOverlay: BenchmarkMacOSComparatorScaleOverlay, identity: BenchmarkArtifactIdentity) throws
   {
      try validateComparisonSHA256(identity.sha256)
      self.scaleOverlay = scaleOverlay
   }

   func prepare(scenario: BenchmarkScenario, loader: BenchmarkSpecLoader) throws
   {
      try teardown()
      measuredInteractionGeneration = 0
      let fixtureData = try loader.read(scenario.fixture)
      let model = try AppKitProductionScenarioModel(scenario: scenario, fixtureData: fixtureData, loader: loader)
      let scaleShadowModel = scaleOverlay?.scale == .twoX ? try AppKitProductionScenarioModel(scenario: scenario, fixtureData: fixtureData, loader: loader) : nil
      let scene = try makeScene(for: scenario.id, model: model)
      host.install(scene)
      self.scenario = scenario
      self.model = model
      self.scaleShadowModel = scaleShadowModel
      bindNativeActions(scene: scene, model: model)
      window.contentViewController = host
      window.setContentSize(NSSize(width: 390, height: 844))
      window.contentMinSize = NSSize(width: 390, height: 844)
      window.contentMaxSize = NSSize(width: 390, height: 844)
      window.makeKeyAndOrderFront(nil)
      withSuppressedMeasuredInteractionObservation {synchronizePresentation()}
      try quiesce()
   }

   func prepareReleaseCandidate(scenario: BenchmarkScenario, loader: BenchmarkSpecLoader) throws
   {
      try prepare(scenario: scenario, loader: loader)
   }

   func reset() throws
   {
      guard let model else
      {
         throw AppKitProductionScenarioFailure.missingScenario
      }
      model.reset()
      scaleShadowModel?.reset()
      measuredInteractionGeneration = 0
      withSuppressedMeasuredInteractionObservation {synchronizePresentation()}
      try quiesce()
   }

   func apply(event: BenchmarkTraceEvent) throws
   {
      guard let model, let activeScene = host.activeScene else
      {
         throw AppKitProductionScenarioFailure.missingScenario
      }
      if requireNativeUserDispatch, appKitProductionIsUserInteraction(event)
      {
         lastEventDispatchRoute = try dispatchNativeUserInteraction(event, scene: activeScene)
         try scaleShadowModel?.apply(event: event)
         return
      }
      try applyApplicationEvent(event, model: model)
      try scaleShadowModel?.apply(event: event)
   }

   func scaleRuntimeAttestation() throws -> (effectiveCardinality: UInt64, completed: Bool)
   {
      guard let scaleOverlay, let model else {throw AppKitProductionScenarioFailure.missingScenario}
      let primaryState = try JSONSerialization.data(withJSONObject: model.state, options: [.sortedKeys])
      if let scaleShadowModel
      {
         let shadowState = try JSONSerialization.data(withJSONObject: scaleShadowModel.state, options: [.sortedKeys])
         return (scaleOverlay.effectiveCardinality, primaryState == shadowState && model.roleCounts == scaleShadowModel.roleCounts)
      }
      return (scaleOverlay.effectiveCardinality, scaleOverlay.scale == .oneX)
   }

   func checkpoint(id: String) throws -> BenchmarkAdapterCheckpoint
   {
      guard let scenario, let model else
      {
         throw AppKitProductionScenarioFailure.missingScenario
      }
      return try benchmarkCheckpoint(
         scenario: scenario,
         checkpointID: id,
         model: model.state,
         visibleRoleCounts: model.roleCounts
      )
   }

   func teardown() throws
   {
      if let feedBoundsObserver
      {
         NotificationCenter.default.removeObserver(feedBoundsObserver)
         self.feedBoundsObserver = nil
      }
      disarmTrustedInputProbe()
      window.initialFirstResponder = nil
      _ = window.makeFirstResponder(nil)
      retiredScene = host.activeScene
      retiredModel = model
      host.uninstall()
      model?.teardown()
      scaleShadowModel?.teardown()
      model = nil
      scaleShadowModel = nil
      scenario = nil
      if window.contentViewController === host
      {
         window.contentViewController = nil
         window.contentView = NSView(frame: NSRect(x: 0, y: 0, width: 390, height: 844))
      }
   }

   func armTrustedInputProbe(_ observer: @escaping () -> Void) throws -> MacOSCanonicalLaunchProbeDescriptor
   {
      guard let scene = host.activeScene as? AppKitStartupProductionSceneRoot else
      {
         throw MacOSCanonicalLaunchFailure.invalidProbe("startup-primary-control")
      }
      canonicalLaunchObserver = observer
      scene.onPrimaryAction =
      {
         [weak self, weak scene] in
         guard let self, let scene, let observer = self.canonicalLaunchObserver else {return}
         if self.canonicalLaunchStateGeneration < UInt64.max
         {
            self.canonicalLaunchStateGeneration += 1
         }
         scene.showLaunchProbeResponse()
         observer()
         DispatchQueue.main.async
         {
            self.onCanonicalLaunchResponsePresented?(.success(()))
         }
      }
      return try macOSCanonicalLaunchProbeDescriptor(
         view: scene.primaryAction,
         probeID: "startup-primary-control",
         targetIdentity: "AppKitStartupProductionSceneRoot",
         actionIdentity: "performPrimaryAction:"
      )
   }

   func disarmTrustedInputProbe()
   {
      canonicalLaunchObserver = nil
      onCanonicalLaunchResponsePresented = nil
      if let scene = host.activeScene as? AppKitStartupProductionSceneRoot
      {
         scene.onPrimaryAction = nil
      }
   }

   func quiesce() throws
   {
      guard let view = host.activeScene?.rootView else
      {
         throw AppKitProductionScenarioFailure.missingScenario
      }
      // Campaign orchestration invokes quiescence only for preparation, reset, and
      // correctness capture. Measured display-link and event paths never call it.
      view.layoutSubtreeIfNeeded()
      view.displayIfNeeded()
      _ = CFRunLoopRunInMode(CFRunLoopMode.defaultMode, 0.01, false)
      view.layoutSubtreeIfNeeded()
      view.displayIfNeeded()
      if let layer = view.layer
      {
         appKitProductionRemoveAnimations(layer)
         CATransaction.flush()
         guard !appKitProductionHasAnimations(layer) else
         {
            throw AppKitProductionScenarioFailure.activeAnimations
         }
      }
   }

   func outputReady() throws
   {
      guard let view = host.activeScene?.rootView else
      {
         throw AppKitProductionScenarioFailure.missingScenario
      }
      view.layoutSubtreeIfNeeded()
      view.displayIfNeeded()
      CATransaction.flush()
   }

   func drainRetiredResources() throws
   {
      let deadline = Date(timeIntervalSinceNow: 2)
      retiredResourceDrainTurnCount = 0
      repeat
      {
         autoreleasepool
         {
            _ = CFRunLoopRunInMode(CFRunLoopMode.defaultMode, 0.01, false)
         }
         retiredResourceDrainTurnCount += 1
      }
      while retiredResourceDrainTurnCount < Self.minimumRetiredResourceDrainTurns ||
            hasRetiredResources && Date() < deadline
      guard !hasRetiredResources else
      {
         throw AppKitProductionScenarioFailure.retiredResourcesUnreleased(retiredResourceKinds)
      }
   }

   func displayTick() throws
   {
      guard host.activeScene != nil else
      {
         throw AppKitProductionScenarioFailure.missingScenario
      }
   }

   func setVirtualTimeUs(_ timeUs: UInt64) throws
   {
      guard let model else
      {
         throw AppKitProductionScenarioFailure.missingScenario
      }
      model.setVirtualTimeUs(timeUs)
      scaleShadowModel?.setVirtualTimeUs(timeUs)
      synchronizePresentation()
   }

   func makeDisplayLink(target: Any, selector: Selector) -> CADisplayLink
   {
      window.displayLink(target: target, selector: selector)
   }

   func macOSSurfaceSnapshot() throws -> BenchmarkMacOSSurfaceSnapshot
   {
      guard let view = window.contentView, let screen = window.screen else {throw AppKitProductionScenarioFailure.missingScenario}
      let viewport = view.bounds
      let backing = view.convertToBacking(viewport)
      let insets = view.safeAreaInsets
      let scale = window.backingScaleFactor
      return BenchmarkMacOSSurfaceSnapshot(
         windowLogicalWidthMilliPoints: UInt64((window.frame.width * 1_000).rounded()),
         windowLogicalHeightMilliPoints: UInt64((window.frame.height * 1_000).rounded()),
         viewportLogicalWidthMilliPoints: UInt64((viewport.width * 1_000).rounded()),
         viewportLogicalHeightMilliPoints: UInt64((viewport.height * 1_000).rounded()),
         backingPixelWidth: UInt64(backing.width.rounded()),
         backingPixelHeight: UInt64(backing.height.rounded()),
         insetTopMilliPoints: UInt64((insets.top * 1_000).rounded()),
         insetLeftMilliPoints: UInt64((insets.left * 1_000).rounded()),
         insetBottomMilliPoints: UInt64((insets.bottom * 1_000).rounded()),
         insetRightMilliPoints: UInt64((insets.right * 1_000).rounded()),
         backingScaleMilli: UInt64((scale * 1_000).rounded()),
         finalColorFormat: "srgb8-opaque-window-composite",
         finalColorSpace: window.colorSpace?.localizedName ?? screen.colorSpace?.localizedName ?? "unknown",
         alphaMode: window.isOpaque ? "opaque" : "premultiplied",
         sampleCount: 1,
         compositorScaling: backing.width == viewport.width * scale && backing.height == viewport.height * scale ? "none-1:1-backing-pixels" : "scaled",
         targetRefreshPolicy: "native-default",
         targetRefreshMillihz: UInt64(screen.maximumFramesPerSecond) * 1_000,
         surfaceImplementation: "appkit-view-tree-windowserver",
         internalColorFormat: "record-only-appkit-windowserver-unexposed"
      )
   }

   func previewPNG() throws -> Data
   {
      guard let view = host.activeScene?.rootView else
      {
         throw AppKitProductionScenarioFailure.missingScenario
      }
      let logicalSize = previewLogicalSize
      let scale = CGFloat(BenchmarkMacOSCorrectnessCaptureProfile.canonical.canonicalScale)
      let width = Int((logicalSize.width * scale).rounded())
      let height = Int((logicalSize.height * scale).rounded())
      let surface = try canonicalPreviewSurface(width: width, height: height)
      let bitmapContext = surface.context
      bitmapContext.saveGState()
      defer {bitmapContext.restoreGState()}
      bitmapContext.clear(CGRect(x: 0, y: 0, width: width, height: height))
      bitmapContext.setFillColor(NSColor(srgbRed: 243 / 255, green: 245 / 255, blue: 248 / 255, alpha: 1).cgColor)
      bitmapContext.fill(CGRect(x: 0, y: 0, width: width, height: height))
      bitmapContext.scaleBy(x: scale, y: scale)
      bitmapContext.translateBy(x: 0, y: logicalSize.height)
      bitmapContext.scaleBy(x: 1, y: -1)
      let context = NSGraphicsContext(cgContext: bitmapContext, flipped: true)
      NSGraphicsContext.saveGraphicsState()
      NSGraphicsContext.current = context
      view.displayIgnoringOpacity(view.bounds, in: context)
      NSGraphicsContext.restoreGraphicsState()
      guard let data = canonicalSRGBEncoder?.encode(linearContext: bitmapContext) else
      {
         throw AppKitProductionScenarioFailure.missingScenario
      }
      return data
   }

   func correctnessGeometry() throws -> BenchmarkMacOSCorrectnessGeometryEvidence
   {
      guard let scene = host.activeScene else
      {
         throw AppKitProductionScenarioFailure.missingScenario
      }
      let root = scene.rootView
      root.layoutSubtreeIfNeeded()
      let semanticNodes = try appKitProductionSemanticGeometryNodes(scene: scene, root: root)
      let nodes = semanticNodes.enumerated().map
      {
         index, node in
         BenchmarkMacOSCorrectnessGeometryNode(ordinal: UInt32(index), kind: node.kind, role: node.role, identifier: node.identifier, bounds: node.bounds, textLineBounds: node.textLineBounds)
      }
      return try benchmarkMacOSCorrectnessGeometry(rootBounds: root.bounds, source: "appkit-view-tree", nodes: nodes)
   }

   private func appKitProductionSemanticGeometryNodes(scene: any AppKitProductionSceneRoot, root: NSView) throws -> [BenchmarkMacOSCorrectnessGeometryNode]
   {
      switch scene
      {
      case let scene as AppKitStartupProductionSceneRoot:
         var nodes = [appKitProductionSemanticNode(root: root, role: "header", identifier: "startup:header", view: scene.header, text: [scene.header])]
         nodes.append(appKitProductionSemanticNode(root: root, role: "navigation", identifier: "startup:navigation", view: scene.navigation, text: [scene.navigationLabel]))
         for item in scene.cards.visibleItems().compactMap({$0 as? AppKitStartupCardCollectionViewItem}).sorted(by: {$0.view.identifier!.rawValue < $1.view.identifier!.rawValue})
         {
            guard let card = item.view as? AppKitStartupCardView,
                  let identifier = card.identifier?.rawValue else {continue}
            nodes.append(appKitProductionSemanticNode(root: root, role: "card", identifier: identifier, view: card, localBounds: card.renderedCardBounds, text: [item.titleField, item.detailField]))
            nodes.append(appKitProductionSemanticNode(root: root, role: "initial-image", identifier: "\(identifier):thumbnail", view: item.thumbnailView))
         }
         nodes.append(appKitProductionSemanticNode(root: root, role: "primary-control", identifier: "startup:primary-control", view: scene.primaryAction, text: [scene.primaryActionLabel]))
         return nodes
      case let scene as AppKitDashboardProductionSceneRoot:
         var nodes = [appKitProductionSemanticNode(root: root, role: "dashboard", identifier: "dashboard", view: root)]
         for view in scene.backdropViews.sorted(by: {$0.identifier!.rawValue < $1.identifier!.rawValue})
         {
            nodes.append(appKitProductionSemanticNode(root: root, role: "backdrop-region", identifier: view.identifier!.rawValue, view: view))
         }
         let items = scene.metrics.visibleItems().compactMap({$0 as? AppKitDashboardCardCollectionViewItem}).sorted(by: {$0.view.identifier!.rawValue < $1.view.identifier!.rawValue})
         for (index, item) in items.enumerated()
         {
            guard let card = item.view as? AppKitProductionRoundedCardView,
                  let identifier = card.identifier?.rawValue else {continue}
            nodes.append(appKitProductionSemanticNode(root: root, role: "rounded-card", identifier: identifier, view: card, localBounds: card.renderedCardBounds))
            nodes.append(appKitProductionSemanticNode(root: root, role: "icon-image", identifier: String(format: "dashboard:icon:%03d", index * 2), view: item.leadingIconView))
            nodes.append(appKitProductionSemanticNode(root: root, role: "icon-image", identifier: String(format: "dashboard:icon:%03d", index * 2 + 1), view: item.trailingIconView))
            for field in item.labelFields
            {
               guard let identifier = field.identifier?.rawValue else {continue}
               nodes.append(appKitProductionSemanticNode(root: root, role: "label", identifier: identifier, view: field, text: [field]))
            }
            if let button = item.actionButton, let identifier = button.identifier?.rawValue
            {
               nodes.append(appKitProductionSemanticNode(root: root, role: "control", identifier: identifier, view: button))
            }
         }
         return nodes
      case let scene as AppKitFeedProductionSceneRoot:
         var nodes = [
            appKitProductionSemanticNode(root: root, role: "navigation-bar", identifier: "feed.navigation-bar", view: scene.navigationBar, text: [scene.navigationTitle]),
            appKitProductionSemanticNode(root: root, role: "feed", identifier: "feed.table", view: scene.scrollView),
         ]
         for cell in appKitProductionVisibleDescendants(root).compactMap({$0 as? AppKitFeedTableCellView}).sorted(by: {$0.identifier!.rawValue < $1.identifier!.rawValue})
         {
            guard let identifier = cell.identifier?.rawValue else {continue}
            let cardBounds = root.convert(cell.renderedCardBounds, from: cell)
            let textLineBounds = [cell.titleField, cell.secondaryField].map {root.convert($0.bounds, from: $0)}
            nodes.append(appKitProductionSemanticRectNode(role: "feed-card", identifier: identifier, kind: NSStringFromClass(type(of: cell)), bounds: cardBounds, textLineBounds: textLineBounds))
            nodes.append(appKitProductionSemanticNode(root: root, role: "thumbnail", identifier: "\(identifier):thumbnail", view: cell.thumbnailView))
            nodes.append(appKitProductionSemanticNode(root: root, role: "favorite-control", identifier: "\(identifier):favorite", view: cell.favoriteButton))
         }
         return nodes
      case let scene as AppKitChatProductionSceneRoot:
         let headingBounds = root.convert(scene.heading.bounds, from: scene.heading)
         var nodes = [
            appKitProductionSemanticRectNode(role: "heading", identifier: "chat.heading", kind: NSStringFromClass(type(of: scene.heading)), bounds: headingBounds, textLineBounds: [headingBounds]),
            appKitProductionSemanticNode(root: root, role: "chat-thread", identifier: "chat.thread", view: scene.threadScrollView),
         ]
         for cell in appKitProductionVisibleDescendants(root).compactMap({$0 as? AppKitChatTableCellView}).sorted(by: {$0.identifier!.rawValue < $1.identifier!.rawValue})
         {
            guard let identifier = cell.identifier?.rawValue else {continue}
            let bubbleBounds = root.convert(cell.renderedBubbleBounds, from: cell)
            let textLineBounds = cell.messageFields.filter({!$0.isHidden}).map {root.convert($0.bounds, from: $0)}
            nodes.append(appKitProductionSemanticRectNode(role: "message", identifier: identifier, kind: NSStringFromClass(type(of: cell)), bounds: bubbleBounds, textLineBounds: textLineBounds))
            nodes.append(appKitProductionSemanticNode(root: root, role: "avatar", identifier: "\(identifier):avatar", view: cell.avatarView))
         }
         let composerBounds = root.convert(scene.composerSurface.bounds, from: scene.composerSurface)
         let composerLineBounds = root.convert(scene.composerPresentation.bounds, from: scene.composerPresentation)
         nodes.append(appKitProductionSemanticRectNode(role: "composer", identifier: "chat.composer", kind: NSStringFromClass(type(of: scene.composerSurface)), bounds: composerBounds, textLineBounds: [composerLineBounds]))
         let sendBounds = root.convert(scene.sendButton.bounds, from: scene.sendButton)
         nodes.append(appKitProductionSemanticRectNode(role: "send-control", identifier: "chat.send", kind: NSStringFromClass(type(of: scene.sendButton)), bounds: sendBounds, textLineBounds: [sendBounds]))
         return nodes
      case let scene as AppKitNavigationProductionSceneRoot:
         if !scene.scrollView.isHidden
         {
            let headingBounds = root.convert(scene.heading.bounds, from: scene.heading)
            var nodes = [
               appKitProductionSemanticNode(root: root, role: "navigation-list", identifier: "navigation.table", view: root),
               appKitProductionSemanticRectNode(role: "heading", identifier: "navigation.heading", kind: NSStringFromClass(type(of: scene.heading)), bounds: headingBounds, textLineBounds: [headingBounds]),
            ]
            let cells = appKitProductionVisibleDescendants(root).compactMap({$0 as? AppKitNavigationTableCellView}).sorted(by: {$0.identifier!.rawValue < $1.identifier!.rawValue})
            for (index, cell) in cells.enumerated()
            {
               nodes.append(appKitProductionSemanticNode(root: root, role: "list-item", identifier: String(format: "navigation:item:%02d", index + 1), view: cell, localBounds: cell.renderedCardBounds, text: [cell.titleField, cell.subtitleField]))
            }
            return nodes
         }
         var nodes = [
            appKitProductionSemanticNode(root: root, role: "detail", identifier: "navigation.detail", view: scene.detailView.modalButton),
            appKitProductionSemanticNode(root: root, role: "back-control", identifier: "navigation.back", view: scene.detailView.backField, text: [scene.detailView.backField]),
         ]
         for (role, identifier, field) in [
            ("heading", "navigation.detail-heading", scene.detailView.headingField),
            ("label", "navigation.detail-title", scene.detailView.titleField),
            ("label", "navigation.detail-subtitle", scene.detailView.subtitleField),
         ]
         {
            let bounds = root.convert(field.bounds, from: field)
            nodes.append(appKitProductionSemanticRectNode(role: role, identifier: identifier, kind: NSStringFromClass(type(of: field)), bounds: bounds, textLineBounds: [bounds]))
         }
         if !scene.modalView.isHidden
         {
            nodes.append(appKitProductionSemanticNode(root: root, role: "modal", identifier: "navigation.modal", view: scene.modalView, localBounds: scene.modalView.renderedModalBounds))
            nodes.append(appKitProductionSemanticNode(root: root, role: "dismiss-control", identifier: "navigation.dismiss", view: scene.modalView.dismissButton, text: [scene.modalView.dismissField]))
            for (role, identifier, field) in [
               ("heading", "navigation.modal-heading", scene.modalView.headingField),
               ("label", "navigation.modal-body-first", scene.modalView.firstBodyField),
               ("label", "navigation.modal-body-second", scene.modalView.secondBodyField),
            ]
            {
               let bounds = root.convert(field.bounds, from: field)
               nodes.append(appKitProductionSemanticRectNode(role: role, identifier: identifier, kind: NSStringFromClass(type(of: field)), bounds: bounds, textLineBounds: [bounds]))
            }
         }
         return nodes
      case let scene as AppKitImageProductionSceneRoot:
         guard let imageBounds = scene.imageView.renderedImageBounds,
               let model else {throw AppKitProductionScenarioFailure.missingScenario}
         let headingBounds = root.convert(scene.heading.bounds, from: scene.heading)
         return [
            appKitProductionSemanticRectNode(role: "heading", identifier: "image.heading", kind: NSStringFromClass(type(of: scene.heading)), bounds: headingBounds, textLineBounds: [headingBounds]),
            appKitProductionSemanticNode(root: root, role: "image-canvas", identifier: "image.canvas", view: scene.imageView),
            appKitProductionSemanticNode(root: root, role: "image", identifier: model.imageFirstVisible ? "image.source" : "image.thumbnail", view: scene.imageView, localBounds: imageBounds),
            appKitProductionSemanticNode(root: root, role: "zoom-control", identifier: "image.zoom", view: scene.zoomSlider, localBounds: CGRect(x: 0, y: 11, width: scene.zoomSlider.bounds.width, height: 6)),
         ]
      case let scene as AppKitGridProductionSceneRoot:
         if !scene.detail.isHidden
         {
            return [
               appKitProductionSemanticNode(root: root, role: "detail-view", identifier: "grid.detail", view: root),
               appKitProductionSemanticNode(root: root, role: "detail-thumbnail", identifier: "grid.detail.thumbnail", view: scene.detailImage),
               appKitProductionSemanticNode(root: root, role: "detail-title", identifier: "grid.detail.title", view: scene.detailTitle, text: [scene.detailTitle]),
               appKitProductionSemanticNode(root: root, role: "back-control", identifier: "grid.back", view: scene.back, text: [scene.back]),
            ]
         }
         let headingBounds = root.convert(scene.heading.bounds, from: scene.heading)
         var nodes = [
            appKitProductionSemanticNode(root: root, role: "thumbnail-grid", identifier: "grid.collection", view: root),
            appKitProductionSemanticRectNode(role: "heading", identifier: "grid.heading", kind: NSStringFromClass(type(of: scene.heading)), bounds: headingBounds, textLineBounds: [headingBounds]),
         ]
         scene.scrollView.layoutSubtreeIfNeeded()
         scene.collection.layoutSubtreeIfNeeded()
         let visibleItems = scene.collection.visibleItems().compactMap {$0 as? AppKitGridCollectionItem}.sorted
         {
            ($0.view.identifier?.rawValue ?? "") < ($1.view.identifier?.rawValue ?? "")
         }
         guard visibleItems.count >= 18 else
         {
            throw AppKitProductionScenarioFailure.unsupportedEvent("grid-visible-item-count:\(visibleItems.count)")
         }
         for item in visibleItems.prefix(18)
         {
            guard let identifier = item.view.identifier?.rawValue,
                  let tile = Int(identifier.suffix(5)) else {throw AppKitProductionScenarioFailure.missingScenario}
            let itemBounds = root.convert(item.view.bounds, from: item.view)
            let thumbnailBounds = root.convert(item.thumbnail.bounds, from: item.thumbnail)
            let labelBounds = root.convert(item.titleField.bounds, from: item.titleField)
            let labelLineBounds = item.titleField.renderedTextLineBounds
            let labelLine = root.convert(labelLineBounds, from: item.titleField)
            nodes.append(appKitProductionSemanticRectNode(role: "grid-tile", identifier: identifier, kind: NSStringFromClass(type(of: item.view)), bounds: itemBounds))
            nodes.append(appKitProductionSemanticRectNode(role: "thumbnail", identifier: String(format: "grid:thumbnail:%05d", tile), kind: NSStringFromClass(type(of: item.thumbnail)), bounds: thumbnailBounds))
            nodes.append(appKitProductionSemanticRectNode(role: "tile-label", identifier: String(format: "grid:label:%05d", tile), kind: NSStringFromClass(type(of: item.titleField)), bounds: labelBounds, textLineBounds: [labelLine]))
         }
         return nodes
      case let scene as AppKitEffectsProductionSceneRoot:
         let headingBounds = root.convert(scene.heading.bounds, from: scene.heading)
         var nodes = [
            appKitProductionSemanticNode(root: root, role: "effects-scene", identifier: "effects.scene", view: root),
            appKitProductionSemanticRectNode(role: "heading", identifier: "effects.heading", kind: NSStringFromClass(type(of: scene.heading)), bounds: headingBounds, textLineBounds: [headingBounds]),
         ]
         let backdropIndices = [5, 17, 29, 41, 53, 65, 77, 89]
         for (index, card) in scene.cards.enumerated()
         {
            nodes.append(appKitProductionSemanticNode(root: root, role: "rounded-card", identifier: String(format: "effects:card:%03d", index), view: card))
            nodes.append(appKitProductionSemanticNode(root: root, role: "clip-region", identifier: String(format: "effects:clip:%03d", index), view: card))
            if index < 32
            {
               nodes.append(appKitProductionSemanticNode(root: root, role: "shadow", identifier: String(format: "effects:shadow:%03d", index), view: card))
            }
            if let backdropOrdinal = backdropIndices.firstIndex(of: index)
            {
               nodes.append(appKitProductionSemanticNode(root: root, role: "backdrop-region", identifier: String(format: "effects:backdrop:%03d", index), view: scene.backdrops[backdropOrdinal]))
            }
         }
         return nodes
      case let scene as AppKitMutationProductionSceneRoot:
         let headingBounds = root.convert(scene.heading.bounds, from: scene.heading)
         var nodes = [
            appKitProductionSemanticNode(root: root, role: "mutation-surface", identifier: "mutation.surface", view: root),
            appKitProductionSemanticRectNode(role: "heading", identifier: "mutation.heading", kind: NSStringFromClass(type(of: scene.heading)), bounds: headingBounds, textLineBounds: [headingBounds]),
         ]
         nodes.reserveCapacity(scene.nodes.count + 2)
         for (index, layer) in scene.nodes.enumerated()
         {
            nodes.append(appKitProductionSemanticLayerNode(root: root, role: "simple-node", identifier: String(format: "mutation:node:%05d", index), layer: layer, owner: scene.surface))
         }
         return nodes
      case let scene as AppKitTextProductionSceneRoot:
         let headingBounds = root.convert(scene.heading.bounds, from: scene.heading)
         var nodes = [
            appKitProductionSemanticNode(root: root, role: "text-surface", identifier: "text.surface", view: root),
            appKitProductionSemanticRectNode(role: "heading", identifier: "text.heading", kind: NSStringFromClass(type(of: scene.heading)), bounds: headingBounds, textLineBounds: [headingBounds]),
         ]
         let visibleHeight = max(root.bounds.height - 40, 0)
         let visibleCount = min(Int(ceil(visibleHeight / scene.table.rowHeight)), scene.records.count)
         for row in 0..<visibleCount
         {
            guard let cell = scene.table.view(atColumn: 0, row: row, makeIfNecessary: true) as? AppKitTextTableCellView else {throw AppKitProductionScenarioFailure.missingScenario}
            let field = cell.label
            var rowBounds = root.convert(scene.table.rect(ofRow: row), from: scene.table)
            rowBounds.origin.x += field.frame.minX
            rowBounds.size.width = field.frame.width
            nodes.append(appKitProductionSemanticRectNode(role: "multilingual-label", identifier: String(format: "text:label:%03d", row), kind: NSStringFromClass(type(of: field)), bounds: rowBounds, textLineBounds: [rowBounds]))
         }
         return nodes
      case let scene as AppKitResizeProductionSceneRoot:
         var nodes = [appKitProductionSemanticNode(root: root, role: "dashboard", identifier: "dashboard", view: root)]
         for (index, view) in scene.backdrops.enumerated()
         {
            nodes.append(appKitProductionSemanticNode(root: root, role: "backdrop-region", identifier: "dashboard:backdrop:\(index)", view: view))
         }
         let items = scene.metrics.visibleItems().compactMap({$0 as? AppKitDashboardCardCollectionViewItem}).sorted(by: {$0.view.identifier!.rawValue < $1.view.identifier!.rawValue})
         for (index, item) in items.enumerated()
         {
            guard let card = item.view as? AppKitProductionRoundedCardView,
                  let identifier = card.identifier?.rawValue else {continue}
            nodes.append(appKitProductionSemanticNode(root: root, role: "rounded-card", identifier: identifier, view: card, localBounds: card.renderedCardBounds))
            nodes.append(appKitProductionSemanticNode(root: root, role: "icon-image", identifier: String(format: "dashboard:icon:%03d", index * 2), view: item.leadingIconView))
            nodes.append(appKitProductionSemanticNode(root: root, role: "icon-image", identifier: String(format: "dashboard:icon:%03d", index * 2 + 1), view: item.trailingIconView))
            for field in item.labelFields
            {
               guard let identifier = field.identifier?.rawValue else {continue}
               nodes.append(appKitProductionSemanticNode(root: root, role: "label", identifier: identifier, view: field, text: [field]))
            }
            if let button = item.actionButton, let identifier = button.identifier?.rawValue
            {
               nodes.append(appKitProductionSemanticNode(root: root, role: "control", identifier: identifier, view: button))
            }
         }
         return nodes
      default:
         return ([root] + appKitProductionVisibleDescendants(root)).compactMap
         {
            view in
            let bounds = root.convert(view.bounds, from: view)
            guard bounds.width > 0, bounds.height > 0 else {return nil}
            let identifier = view.accessibilityIdentifier().isEmpty ? view.identifier?.rawValue : view.accessibilityIdentifier()
            let role = view.accessibilityLabel().flatMap {$0.isEmpty ? nil : $0}
               ?? (view is NSTextField || view is NSTextView ? "text" : "container")
            let text = view is NSTextField ? [view] : []
            return appKitProductionSemanticNode(root: root, role: role, identifier: identifier, view: view, text: text)
         }
      }
   }

   private func appKitProductionSemanticNode(root: NSView, role: String, identifier: String?, view: NSView, localBounds: CGRect? = nil, text: [NSView] = []) -> BenchmarkMacOSCorrectnessGeometryNode
   {
      let bounds = root.convert(localBounds ?? view.bounds, from: view)
      return BenchmarkMacOSCorrectnessGeometryNode(
         ordinal: 0,
         kind: NSStringFromClass(type(of: view)),
         role: role,
         identifier: identifier,
         bounds: appKitProductionLogicalRect(bounds),
         textLineBounds: text.map
         {
            textView in
            let lineBounds = (textView as? AppKitProductionExactTextField)?.renderedTextLineBounds ?? textView.bounds
            return appKitProductionLogicalRect(root.convert(lineBounds, from: textView))
         }
      )
   }

   private func appKitProductionSemanticLayerNode(root: NSView, role: String, identifier: String, layer: CALayer, owner: NSView) -> BenchmarkMacOSCorrectnessGeometryNode
   {
      BenchmarkMacOSCorrectnessGeometryNode(
         ordinal: 0,
         kind: NSStringFromClass(type(of: layer)),
         role: role,
         identifier: identifier,
         bounds: appKitProductionLogicalRect(root.convert(layer.frame, from: owner)),
         textLineBounds: []
      )
   }

   private func appKitProductionSemanticRectNode(role: String, identifier: String, kind: String, bounds: CGRect, textLineBounds: [CGRect] = []) -> BenchmarkMacOSCorrectnessGeometryNode
   {
      BenchmarkMacOSCorrectnessGeometryNode(
         ordinal: 0,
         kind: kind,
         role: role,
         identifier: identifier,
         bounds: appKitProductionLogicalRect(bounds),
         textLineBounds: textLineBounds.map(appKitProductionLogicalRect)
      )
   }

   private func appKitProductionLogicalRect(_ rect: CGRect) -> BenchmarkMacOSCorrectnessLogicalRect
   {
      let canonical = {(value: CGFloat) in (Double(value) * 3).rounded() / 3}
      return BenchmarkMacOSCorrectnessLogicalRect(
         x: canonical(rect.origin.x),
         y: canonical(rect.origin.y),
         width: canonical(rect.width),
         height: canonical(rect.height)
      )
   }

   private func appKitProductionVisibleDescendants(_ root: NSView) -> [NSView]
   {
      var descendants = [NSView]()
      for view in root.subviews where !view.isHidden
      {
         descendants.append(view)
         descendants.append(contentsOf: appKitProductionVisibleDescendants(view))
      }
      return descendants
   }

   private var previewLogicalSize: NSSize
   {
      guard scenario?.id == "resize.theme", let model else {return NSSize(width: 390, height: 844)}
      return NSSize(width: model.resizeWidth, height: model.resizeHeight)
   }

   private func canonicalPreviewSurface(width: Int, height: Int) throws -> AppKitProductionPreviewSurface
   {
      if let previewSurface, previewSurface.width == width, previewSurface.height == height,
         let canonicalSRGBEncoder, canonicalSRGBEncoder.width == width, canonicalSRGBEncoder.height == height
      {
         return previewSurface
      }
      guard let encoder = AppKitProductionCanonicalSRGBEncoder(width: width, height: height) else
      {
         throw AppKitProductionScenarioFailure.missingScenario
      }
      let surface = try AppKitProductionPreviewSurface(width: width, height: height)
      previewSurface = surface
      canonicalSRGBEncoder = encoder
      previewSurfaceAllocationCount += 1
      return surface
   }

   func rawAccessibilityTree() throws -> Data
   {
      guard let root = host.activeScene?.rootView else
      {
         throw AppKitProductionScenarioFailure.missingScenario
      }
      let evidence = AppKitNativeAccessibilityEvidence.capture(from: root)
      let nodes = evidence.enumerated().compactMap
      {
         index, node -> [String: Any]? in
         guard node.isAccessibilityElement || node.identifier != nil else {return nil}
         return [
            "order": index,
            "identifier": node.identifier ?? NSNull(),
            "role": node.role?.rawValue ?? NSNull(),
            "label": node.label ?? NSNull(),
            "class": NSStringFromClass(node.viewClass),
            "hierarchy_depth": node.hierarchyDepth,
            "object_identity": String(describing: node.objectIdentifier),
         ]
      }
      let tree: [String: Any] = [
         "schema_version": 1,
         "platform": "appkit",
         "source": "NSView/NSAccessibility",
         "scenario_id": scenario?.id ?? "",
         "nodes": nodes,
      ]
      return try JSONSerialization.data(withJSONObject: tree, options: [.sortedKeys])
   }

   private func makeScene(for scenarioID: String, model: AppKitProductionScenarioModel) throws -> any AppKitProductionSceneRoot
   {
      switch scenarioID
      {
      case AppKitProductionSceneID.startupFirstScreen.rawValue:
         return AppKitStartupProductionSceneRoot(
            headingFont: model.fonts.latin20,
            bodyFont: model.fonts.latin15,
            controlFont: model.fonts.latin15
         )
      case AppKitProductionSceneID.dashboardMixedStatic.rawValue, "idle.steady": return AppKitDashboardProductionSceneRoot()
      case AppKitProductionSceneID.enduranceChurn.rawValue: return AppKitEnduranceProductionSceneRoot()
      case AppKitProductionSceneID.feedVariableScroll.rawValue: return AppKitFeedProductionSceneRoot(headingFont: model.fonts.latin20)
      case AppKitProductionSceneID.chatLiveUpdate.rawValue:
         return AppKitChatProductionSceneRoot(headingFont: model.fonts.latin20, sendFont: model.fonts.latin13)
      case AppKitProductionSceneID.navigationModal.rawValue:
         return AppKitNavigationProductionSceneRoot(
            headingFont: model.fonts.latin20,
            bodyFont: model.fonts.latin15,
            secondaryFont: model.fonts.latin11,
            controlFont: model.fonts.latin13
         )
      case AppKitProductionSceneID.imageDecodeZoom.rawValue: return AppKitImageProductionSceneRoot(headingFont: model.fonts.latin20)
      case AppKitProductionSceneID.gridLargeScroll.rawValue:
         return AppKitGridProductionSceneRoot(headingFont: model.fonts.latin20, detailFont: model.fonts.latin20, backFont: model.fonts.latin15)
      case AppKitProductionSceneID.effectsLayers.rawValue: return AppKitEffectsProductionSceneRoot(headingFont: model.fonts.latin20)
      case AppKitProductionSceneID.mutationDamage.rawValue: return AppKitMutationProductionSceneRoot(headingFont: model.fonts.latin20)
      case AppKitProductionSceneID.textMultilingual.rawValue: return AppKitTextProductionSceneRoot(headingFont: model.fonts.latin20)
      case AppKitProductionSceneID.resizeTheme.rawValue: return AppKitResizeProductionSceneRoot()
      default: throw AppKitProductionScenarioFailure.unsupportedScenario(scenarioID)
      }
   }

   private func applyApplicationEvent(_ event: BenchmarkTraceEvent, model: AppKitProductionScenarioModel) throws
   {
      if event.op == "resource-arrival", event.target == "image:texture"
      {
         guard let scene = host.activeScene as? AppKitImageProductionSceneRoot,
               let image = model.imageDecoded else
         {
            throw AppKitProductionScenarioFailure.unsupportedEvent(event.target ?? event.op)
         }
         try scene.prepareUpload(image: image)
      }
      try model.apply(event: event)
      lastEventDispatchRoute = .applicationUpdate
      withSuppressedMeasuredInteractionObservation {synchronizePresentation(after: event)}
   }

   private func dispatchNativeUserInteraction(_ event: BenchmarkTraceEvent, scene: any AppKitProductionSceneRoot) throws -> AppKitProductionEventDispatchRoute
   {
      switch scene
      {
      case let feed as AppKitFeedProductionSceneRoot:
         guard event.op == "mutate",
               let target = event.target,
               target.hasSuffix(":favorite"),
               let row = feed.tableController.row(for: String(target.dropLast(":favorite".count))),
               NSLocationInRange(row, feed.table.rows(in: feed.table.visibleRect)),
               let cell = feed.table.view(atColumn: 0, row: row, makeIfNecessary: false) as? AppKitFeedTableCellView else
         {
            return try rejectUnsupportedNativeInteraction(event)
         }
         try sendNativeAction(cell.favoriteButton, identifier: target)
         return .nativeTargetAction(target)
      case let chat as AppKitChatProductionSceneRoot:
         guard event.op == "commit-text", event.target == "chat:composer", case .text(let value)? = event.value else
         {
            return try rejectUnsupportedNativeInteraction(event)
         }
         guard window.makeFirstResponder(chat.composer) else
         {
            throw AppKitProductionScenarioFailure.unsupportedEvent("chat:composer:first-responder")
         }
         chat.composer.insertText(value, replacementRange: chat.composer.selectedRange())
         return .nativeTextInput("chat:composer")
      case let navigation as AppKitNavigationProductionSceneRoot:
         guard event.op == "navigate", let target = event.target else
         {
            return try rejectUnsupportedNativeInteraction(event)
         }
         switch target
         {
         case "navigation:item:05":
            guard let row = navigation.tableController.row(for: target),
                  let cell = navigation.table.view(atColumn: 0, row: row, makeIfNecessary: true) as? AppKitNavigationTableCellView else
            {
               throw AppKitProductionScenarioFailure.unsupportedEvent(target)
            }
            try sendNativeAction(cell.destinationButton, identifier: target)
         case "navigation:modal":
            try sendNativeAction(navigation.detailView.modalButton, identifier: target)
         case "navigation:dismiss-control":
            try sendNativeAction(navigation.modalView.dismissButton, identifier: target)
         case "navigation:back-control":
            try sendNativeAction(navigation.detailView.backButton, identifier: target)
         default:
            return try rejectUnsupportedNativeInteraction(event)
         }
         return .nativeTargetAction(target)
      default:
         return try rejectUnsupportedNativeInteraction(event)
      }
   }

   private func sendNativeAction(_ control: NSControl, identifier: String) throws
   {
      guard let action = control.action,
            NSApp.sendAction(action, to: control.target, from: control) else
      {
         throw AppKitProductionScenarioFailure.unsupportedEvent("(identifier):target-action")
      }
   }

   private func rejectUnsupportedNativeInteraction<T>(_ event: BenchmarkTraceEvent) throws -> T
   {
      let identity = event.target ?? event.op
      lastEventDispatchRoute = .unsupported(identity)
      throw AppKitProductionScenarioFailure.unsupportedEvent("native-dispatch:\(identity)")
   }

   private func bindNativeActions(scene: any AppKitProductionSceneRoot, model: AppKitProductionScenarioModel)
   {
      switch scene
      {
      case let dashboard as AppKitDashboardProductionSceneRoot:
         dashboard.collectionController.onAction =
         {
            [weak self, weak model] target in
            guard let self, let model else {return}
            let event = BenchmarkTraceEvent(
               atUs: 0,
               op: "mutate",
               pointer: nil,
               xMillionths: nil,
               yMillionths: nil,
               deltaXMillionths: nil,
               deltaYMillionths: nil,
               target: target,
               value: nil,
               stateId: nil
            )
            do
            {
               try model.apply(event: event)
               self.synchronizePresentation()
               self.recordMeasuredInteraction()
            }
            catch {}
         }
      case let feed as AppKitFeedProductionSceneRoot:
         feed.scrollView.contentView.postsBoundsChangedNotifications = true
         feedBoundsObserver = NotificationCenter.default.addObserver(
            forName: NSView.boundsDidChangeNotification,
            object: feed.scrollView.contentView,
            queue: .main
         )
         {
            [weak self, weak feed] _ in
            guard let self, let feed else {return}
            self.observeMeasuredFeedScroll(
               boundsOriginY: feed.scrollView.contentView.bounds.origin.y,
               eventType: NSApp.currentEvent?.type
            )
         }
         feed.tableController.onFavorite =
         {
            [weak self, weak feed, weak model] id in
            guard let self, let feed, let model else {return}
            model.favoriteFeedItem(id)
            guard let record = model.feedRecord(id: id) else {return}
            _ = feed.updateRow(id: id, record: record)
            self.recordMeasuredInteraction()
         }
      case let chat as AppKitChatProductionSceneRoot:
         chat.tableController.onMessageSelection =
         {
            [weak self, weak model] id, range in
            guard let self, let model, model.selectChatMessage(id, range: range) else {return}
            _ = self.scaleShadowModel?.selectChatMessage(id, range: range)
            self.recordMeasuredInteraction()
         }
         chat.tableController.onMessageValueChange =
         {
            [weak self, weak chat, weak model] id, value in
            guard let self, let chat, let model,
                  model.replaceSelectedChatMessage(id, value: value),
                  let record = model.chatRecord(id: id) else {return}
            _ = self.scaleShadowModel?.replaceSelectedChatMessage(id, value: value)
            _ = self.window.makeFirstResponder(nil)
            guard chat.updateMessage(id: id, record: record) else {return}
            if !chat.tableController.records.isEmpty
            {
               chat.thread.scrollRowToVisible(chat.tableController.records.count - 1)
            }
            self.recordMeasuredInteraction()
         }
         chat.onComposerInput =
         {
            [weak self, weak chat, weak model] value in
            guard let self, let chat, let model else {return}
            model.appendChatComposer(value)
            chat.presentComposer(model.chatComposer, font: model.fonts.latin15, inlineText: model.assets.inlineText)
            self.recordMeasuredInteraction()
         }
         chat.onSend =
         {
            [weak self, weak chat, weak model] _ in
            guard let self, let chat, let model, let record = model.sendChatComposer() else {return}
            chat.appendMessage(record)
            chat.presentComposer(model.chatComposer, font: model.fonts.latin15, inlineText: model.assets.inlineText)
            chat.thread.scrollRowToVisible(chat.tableController.records.count - 1)
            self.recordMeasuredInteraction()
         }
      case let navigation as AppKitNavigationProductionSceneRoot:
         navigation.tableController.onSelect =
         {
            [weak self, weak navigation, weak model] id in
            guard let self, let navigation, let model else {return}
            model.selectNavigationDestination(id)
            navigation.present(
               route: model.navigationRoute,
               modalProgress: model.navigationModalProgress,
               modalVisible: model.navigationModalVisible
            )
            self.recordMeasuredInteraction()
         }
         navigation.onPresentModal =
         {
            [weak self, weak navigation, weak model] in
            guard let self, let navigation, let model else {return}
            model.presentNavigationModal()
            navigation.present(
               route: model.navigationRoute,
               modalProgress: model.navigationModalProgress,
               modalVisible: model.navigationModalVisible
            )
            self.recordMeasuredInteraction()
         }
         navigation.onBack =
         {
            [weak self, weak navigation, weak model] in
            guard let self, let navigation, let model else {return}
            model.returnToNavigationList()
            navigation.present(
               route: model.navigationRoute,
               modalProgress: model.navigationModalProgress,
               modalVisible: model.navigationModalVisible
            )
            self.recordMeasuredInteraction()
         }
         navigation.onDismiss =
         {
            [weak self, weak navigation, weak model] in
            guard let self, let navigation, let model else {return}
            model.dismissNavigationModal()
            navigation.present(
               route: model.navigationRoute,
               modalProgress: model.navigationModalProgress,
               modalVisible: model.navigationModalVisible
            )
            self.recordMeasuredInteraction()
         }
         navigation.onInteractiveCancel =
         {
            [weak self, weak navigation, weak model] phase, progress in
            guard let self, let navigation, let model else {return}
            switch phase
            {
            case .changed:
               model.updateNavigationInteractiveCancel(progress: progress)
            case .ended:
               model.finishNavigationInteractiveCancel()
            }
            navigation.present(
               route: model.navigationRoute,
               modalProgress: model.navigationModalProgress,
               modalVisible: model.navigationModalVisible
            )
            self.recordMeasuredInteraction()
         }
      case let image as AppKitImageProductionSceneRoot:
         image.onZoom =
         {
            [weak self, weak image, weak model] scale in
            guard let self, let image, let model else {return}
            model.setImageScale(scale)
            image.present(
               image: model.imageFirstVisible ? model.imageDecoded : model.assets.thumbnail,
               translation: model.imageTranslation,
               scale: model.imageScale
            )
            self.recordMeasuredInteraction()
         }
      default:
         break
      }
   }

   private func synchronizePresentation()
   {
      guard let model, let activeScene = host.activeScene else {return}
      switch activeScene
      {
      case let scene as AppKitStartupProductionSceneRoot:
         scene.rootView.isHidden = !model.startupVisible
         scene.replaceCards(model.startupCards)
      case let scene as AppKitDashboardProductionSceneRoot:
         scene.replaceMetrics(model.dashboardRecords)
      case let scene as AppKitEnduranceProductionSceneRoot:
         scene.present(
            records: model.enduranceHeavyScreenVisible ? model.enduranceRecords : [],
            visible: model.enduranceHeavyScreenVisible
         )
      case let scene as AppKitFeedProductionSceneRoot:
         scene.replaceRows(model.feedRecords)
         scene.applyScrollOffset(model.feedScrollOffset)
      case let scene as AppKitChatProductionSceneRoot:
         scene.replaceMessages(model.chatRecords)
         let composerFont: NSFont
         if model.chatComposer.unicodeScalars.contains(where: {(0x4E00...0x9FFF).contains(Int($0.value))})
         {
            composerFont = model.fonts.cjk15
         }
         else if model.chatComposer.unicodeScalars.contains(where: {(0x0600...0x06FF).contains(Int($0.value))})
         {
            composerFont = model.fonts.arabic15
         }
         else
         {
            composerFont = model.fonts.latin15
         }
         scene.presentComposer(model.chatComposer, font: composerFont, inlineText: model.assets.inlineText)
         if !model.chatRecords.isEmpty
         {
            scene.thread.scrollRowToVisible(model.chatRecords.count - 1)
         }
      case let scene as AppKitNavigationProductionSceneRoot:
         scene.replaceDestinations(model.navigationRecords)
         scene.present(
            route: model.navigationRoute,
            modalProgress: model.navigationModalProgress,
            modalVisible: model.navigationModalVisible
         )
      case let scene as AppKitImageProductionSceneRoot:
         scene.present(
            image: model.imageFirstVisible ? model.imageDecoded : model.assets.thumbnail,
            translation: model.imageTranslation,
            scale: model.imageScale
         )
         scene.zoomSlider.doubleValue = Double(model.imageScale)
      case let scene as AppKitGridProductionSceneRoot:
         scene.present(
            records: model.gridRecords,
            scrollMillionths: model.gridScrollPositionMillionths,
            detailRecord: model.gridDetailRecord
         )
         scene.onSelect =
         {
            [weak self] index in
            guard let self, let model = self.model else {return}
            model.showGridDetail(tileIndex: index)
            self.synchronizePresentation()
            self.recordMeasuredInteraction()
         }
         scene.onBack =
         {
            [weak self] in
            guard let self, let model = self.model else {return}
            model.hideGridDetail()
            self.synchronizePresentation()
            self.recordMeasuredInteraction()
         }
         scene.onScroll =
         {
            [weak self] value in
            guard let self, let model = self.model else {return}
            model.setGridScrollPositionMillionths(value)
            self.recordMeasuredInteraction()
         }
      case let scene as AppKitEffectsProductionSceneRoot:
         scene.present(progress: CGFloat(model.state["animation_progress_millionths"] as? Int ?? 0) / 1_000_000, dirtyGeneration: model.state["dirty_layer_generation"] as? Int ?? 0)
      case let scene as AppKitMutationProductionSceneRoot:
         scene.present(changed: model.mutationChanged, generation: model.mutationGenerationValue)
      case let scene as AppKitTextProductionSceneRoot:
         scene.present(records: model.textRecords, wrapWidth: model.textWrapWidth, scale: model.textScale)
      case let scene as AppKitResizeProductionSceneRoot:
         let size = NSSize(width: model.resizeWidth, height: model.resizeHeight)
         window.contentMinSize = size
         window.contentMaxSize = size
         window.setContentSize(size)
         window.appearance = NSAppearance(named: model.resizeTheme == "dark" ? .darkAqua : .aqua)
         scene.present(records: model.dashboardRecords, orientation: model.resizeOrientation, theme: model.resizeTheme)
      default:
         break
      }
      activeScene.rootView.needsLayout = true
      activeScene.rootView.needsDisplay = true
   }

   private func recordMeasuredInteraction()
   {
      if configuredPassID != "canonical-launch", measuredInteractionGeneration < UInt64.max
      {
         measuredInteractionGeneration += 1
      }
   }

   func observeMeasuredFeedScroll(boundsOriginY: CGFloat, eventType: NSEvent.EventType?)
   {
      guard !suppressMeasuredInteractionObservation,
            eventType == .scrollWheel || eventType == .leftMouseDragged,
            let model,
            model.setFeedScrollOffset(boundsOriginY) else {return}
      _ = scaleShadowModel?.setFeedScrollOffset(boundsOriginY)
      recordMeasuredInteraction()
   }

   private func withSuppressedMeasuredInteractionObservation(_ operation: () -> Void)
   {
      let wasSuppressed = suppressMeasuredInteractionObservation
      suppressMeasuredInteractionObservation = true
      defer {suppressMeasuredInteractionObservation = wasSuppressed}
      operation()
   }

   private func synchronizePresentation(after event: BenchmarkTraceEvent)
   {
      guard let model, let activeScene = host.activeScene else {return}
      switch activeScene
      {
      case let scene as AppKitFeedProductionSceneRoot:
         if event.target == "feed:prepend-count"
         {
            scene.prependRows(model.feedPrependedRecords)
         }
         else if let target = event.target, target.hasSuffix(":favorite")
         {
            let id = String(target.dropLast(":favorite".count))
            if let record = model.feedRecord(id: id) {_ = scene.updateRow(id: id, record: record)}
         }
         scene.applyScrollOffset(model.feedScrollOffset)
      case let scene as AppKitChatProductionSceneRoot:
         if event.target == "chat:prepend-count"
         {
            scene.prependMessages(model.chatPrependedRecords)
         }
         else if event.target == "chat:append", let record = model.chatAppendedRecord
         {
            scene.appendMessage(record)
            scene.thread.scrollRowToVisible(scene.tableController.records.count - 1)
         }
         else if event.op == "commit-text", event.target != "chat:composer",
                 let id = event.target, let record = model.chatRecord(id: id)
         {
            _ = scene.updateMessage(id: id, record: record)
         }
         synchronizeChatComposer(scene, model: model)
      case let scene as AppKitNavigationProductionSceneRoot:
         scene.present(
            route: model.navigationRoute,
            modalProgress: model.navigationModalProgress,
            modalVisible: model.navigationModalVisible
         )
      case let scene as AppKitImageProductionSceneRoot:
         scene.present(
            image: model.imageFirstVisible ? model.imageDecoded : model.assets.thumbnail,
            translation: model.imageTranslation,
            scale: model.imageScale
         )
         scene.zoomSlider.doubleValue = Double(model.imageScale)
      default:
         synchronizePresentation()
         return
      }
      activeScene.rootView.needsLayout = true
      activeScene.rootView.needsDisplay = true
   }

   private func synchronizeChatComposer(_ scene: AppKitChatProductionSceneRoot, model: AppKitProductionScenarioModel)
   {
      let composerFont: NSFont
      if model.chatComposer.unicodeScalars.contains(where: {(0x4E00...0x9FFF).contains(Int($0.value))})
      {
         composerFont = model.fonts.cjk15
      }
      else if model.chatComposer.unicodeScalars.contains(where: {(0x0600...0x06FF).contains(Int($0.value))})
      {
         composerFont = model.fonts.arabic15
      }
      else
      {
         composerFont = model.fonts.latin15
      }
      scene.presentComposer(model.chatComposer, font: composerFont, inlineText: model.assets.inlineText)
   }
}

enum AppKitProductionEventDispatchRoute: Equatable
{
   case applicationUpdate
   case nativeTargetAction(String)
   case nativeTextInput(String)
   case unsupported(String)
}

private func appKitProductionIsUserInteraction(_ event: BenchmarkTraceEvent) -> Bool
{
   if ["pointer-down", "pointer-move", "pointer-up", "pointer-cancel", "wheel", "focus", "commit-text", "navigate"].contains(event.op)
   {
      return true
   }
   return event.op == "mutate" && event.target?.hasSuffix(":favorite") == true
}

private func appKitProductionHasAnimations(_ layer: CALayer) -> Bool
{
   if !(layer.animationKeys() ?? []).isEmpty
   {
      return true
   }
   return (layer.sublayers ?? []).contains(where: appKitProductionHasAnimations)
}

private func appKitProductionRemoveAnimations(_ layer: CALayer)
{
   layer.removeAllAnimations()
   (layer.sublayers ?? []).forEach(appKitProductionRemoveAnimations)
}
