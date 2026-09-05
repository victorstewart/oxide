import Foundation
import ImageIO
import UIKit

enum UIKitScenarioAdapterFailure: Error
{
   case activeAnimations
   case invalidFixture
   case missingScenario
   case unsupportedEvent(String)
   case unsupportedScenario(String)
}

protocol UIKitBenchmarkScene: AnyObject
{
   var viewController: UIViewController {get}
   func apply(event: BenchmarkTraceEvent) throws
   func reset() throws
   func state(checkpointID: String) -> [String: Any]
   func roleCounts() -> [BenchmarkRoleCount]
   func teardown()
}

private struct UIKitStartupCard: Decodable
{
   let id: String
   let dataOffset: Int
   let dataLength: Int
   let thumbnailIndex: Int
   let initiallyVisible: Bool
}

private struct UIKitStartupFixture: Decodable
{
   let schemaVersion: UInt32
   let id: String
   let dataSizeBytes: Int
   let data: String
   let cardCount: Int
   let cards: [UIKitStartupCard]
   let initialImageIndices: [Int]
   let headerId: String
   let navigationId: String
   let controlId: String
}

private struct UIKitChatMessage: Decodable
{
   let id: String
   let sequence: Int
   let authorIndex: Int
   let avatarIndex: Int
   let direction: String
   var text: String
}

private struct UIKitChatSelectionReplacement: Decodable
{
   let messageId: String
   let startUtf8: Int
   let endUtf8: Int
   let replacement: String
}

private struct UIKitChatFixture: Decodable
{
   let schemaVersion: UInt32
   let id: String
   let messageCount: Int
   let avatarCount: Int
   let messages: [UIKitChatMessage]
   let prependMessages: [UIKitChatMessage]
   let appendRateHz: Int
   let typedText: String
   let pastedText: String
   let selectionReplacement: UIKitChatSelectionReplacement
}

private struct UIKitImageFileFixture: Decodable
{
   let artifact: BenchmarkArtifactIdentity
   let width: Int
   let height: Int
   let format: String
   let colorSpace: String
}

private struct UIKitImageFixture: Decodable
{
   let schemaVersion: UInt32
   let id: String
   let source: UIKitImageFileFixture
   let thumbnail: UIKitImageFileFixture
   let panDistanceMillionths: Int32
   let pinchScaleMillionths: Int32
}

private let uikitFixtureDecoder: JSONDecoder =
{
   let decoder = JSONDecoder()
   decoder.keyDecodingStrategy = .convertFromSnakeCase
   return decoder
}()

private func uikitLayerTreeHasAnimations(_ layer: CALayer) -> Bool
{
   if !(layer.animationKeys() ?? []).isEmpty
   {
      return true
   }
   return (layer.sublayers ?? []).contains(where: uikitLayerTreeHasAnimations)
}

private func uikitRemoveLayerTreeAnimations(_ layer: CALayer)
{
   layer.removeAllAnimations()
   (layer.sublayers ?? []).forEach(uikitRemoveLayerTreeAnimations)
}

private let comparisonBackground = UIColor(red: 243 / 255, green: 245 / 255, blue: 248 / 255, alpha: 1)
private let comparisonSurface = UIColor.white
private let comparisonText = UIColor(red: 32 / 255, green: 36 / 255, blue: 44 / 255, alpha: 1)
private let comparisonSecondaryText = UIColor(red: 105 / 255, green: 113 / 255, blue: 129 / 255, alpha: 1)
private let comparisonAccent = UIColor(red: 61 / 255, green: 110 / 255, blue: 239 / 255, alpha: 1)
private let comparisonInactiveControl = UIColor(red: 180 / 255, green: 186 / 255, blue: 198 / 255, alpha: 1)

final class UIKitPreparedInlineText
{
   let contract: BenchmarkInlineTextAtlas
   private let images: [UInt32: [String: CGImage]]

   init(contract: BenchmarkInlineTextAtlas, rasterImages: [UInt32: CGImage]) throws
   {
      var images = [UInt32: [String: CGImage]]()
      for variant in contract.variants
      {
         guard let raster = rasterImages[variant.emPixels] else
         {
            throw UIKitScenarioAdapterFailure.invalidFixture
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
               throw UIKitScenarioAdapterFailure.invalidFixture
            }
            tiles[entry.grapheme] = tile
         }
         images[variant.emPixels] = tiles
      }
      self.contract = contract
      self.images = images
   }

   func attributedText(_ value: String, font: UIFont, color: UIColor) -> NSAttributedString?
   {
      guard contract.entries.contains(where: {value.contains($0.grapheme)}) else {return nil}
      let targetPixels = UInt32((font.pointSize * UIScreen.main.scale).rounded())
      guard let variant = contract.variants.min(by: {
         abs(Int64($0.emPixels) - Int64(targetPixels)) < abs(Int64($1.emPixels) - Int64(targetPixels))
      }), let tiles = images[variant.emPixels] else {return nil}
      let result = NSMutableAttributedString()
      var remaining = value[...]
      while let match = nextMatch(in: remaining)
      {
         append(String(remaining[..<match.range.lowerBound]), font: font, color: color, to: result)
         let attachment = NSTextAttachment()
         guard let tile = tiles[match.entry.grapheme] else {return nil}
         attachment.image = UIImage(cgImage: tile, scale: CGFloat(variant.emPixels) / font.pointSize, orientation: .up)
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
      append(String(remaining), font: font, color: color, to: result)
      return result
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

   private func append(_ value: String, font: UIFont, color: UIColor, to result: NSMutableAttributedString)
   {
      guard !value.isEmpty else {return}
      result.append(NSAttributedString(string: value, attributes: [.font: font, .foregroundColor: color]))
   }
}

private final class UIKitPreparedAssets
{
   let atlasImage: UIImage?
   let atlas: [UIImage]
   let inlineText: UIKitPreparedInlineText?
   let sourceData: Data?
   let sourceSize: CGSize?
   let thumbnail: UIImage?
   private var retainedSource: UIImage?

   init(atlasImage: UIImage? = nil, atlas: [UIImage] = [], inlineText: UIKitPreparedInlineText? = nil, sourceData: Data? = nil, sourceSize: CGSize? = nil, thumbnail: UIImage? = nil)
   {
      self.atlasImage = atlasImage
      self.atlas = atlas
      self.inlineText = inlineText
      self.sourceData = sourceData
      self.sourceSize = sourceSize
      self.thumbnail = thumbnail
   }

   func decodedSource() throws -> UIImage
   {
      if let retainedSource
      {
         return retainedSource
      }
      guard let sourceData,
            let sourceSize,
            let source = CGImageSourceCreateWithData(sourceData as CFData, nil),
            let image = CGImageSourceCreateImageAtIndex(source, 0, nil),
            image.width == Int(sourceSize.width),
            image.height == Int(sourceSize.height),
            let colorSpace = CGColorSpace(name: CGColorSpace.sRGB),
            let context = CGContext(
               data: nil,
               width: image.width,
               height: image.height,
               bitsPerComponent: 8,
               bytesPerRow: image.width * 4,
               space: colorSpace,
               bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue | CGBitmapInfo.byteOrder32Big.rawValue
            ) else
      {
         throw UIKitScenarioAdapterFailure.invalidFixture
      }
      context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
      guard let decoded = context.makeImage() else
      {
         throw UIKitScenarioAdapterFailure.invalidFixture
      }
      let prepared = UIImage(cgImage: decoded, scale: 1, orientation: .up)
      retainedSource = prepared
      return prepared
   }

   func releaseDecodedSource()
   {
      retainedSource = nil
   }
}

struct UIKitPreparedFonts
{
   let latin7: UIFont
   let latin11: UIFont
   let latin13: UIFont
   let latin15: UIFont
   let latin20: UIFont
   let arabic15: UIFont
   let cjk15: UIFont

   init(catalog: BenchmarkAppleFontCatalog) throws
   {
      latin7 = try catalog.font(role: "latin", size: 7) as UIFont
      latin11 = try catalog.font(role: "latin", size: 11) as UIFont
      latin13 = try catalog.font(role: "latin", size: 13) as UIFont
      latin15 = try catalog.font(role: "latin", size: 15) as UIFont
      latin20 = try catalog.font(role: "latin", size: 20) as UIFont
      arabic15 = try catalog.font(role: "arabic", size: 15) as UIFont
      cjk15 = try catalog.font(role: "cjk-simplified", size: 15) as UIFont
   }
}

final class UIKitScenarioAdapter: BenchmarkScenarioAdapter, BenchmarkPreviewCapture, BenchmarkQuiescenceAdapter, BenchmarkPassConfiguredAdapter
{
   private let hostController = UIViewController()
   private let window: UIWindow
   private var correctnessMode = false
   private var scenario: BenchmarkScenario?
   private var fixture: [String: Any]?
   private var fixtureData: Data?
   private var assets: UIKitPreparedAssets?
   private var scene: UIKitBenchmarkScene?
   private var viewport: BenchmarkViewportController?

   init(window: UIWindow)
   {
      self.window = window
      hostController.view.backgroundColor = .black
      window.rootViewController = hostController
   }

   func configure(passID: String)
   {
      correctnessMode = passID == "correctness"
   }

   func prepare(scenario: BenchmarkScenario, loader: BenchmarkSpecLoader) throws
   {
      if scene != nil
      {
         try teardown()
      }
      let fonts = try UIKitPreparedFonts(catalog: loader.loadRegisteredFontCatalog(scenario.fontPack))
      let fixtureData = try loader.read(scenario.fixture)
      let fixture = try JSONSerialization.jsonObject(with: fixtureData)
      guard let fixture = fixture as? [String: Any] else
      {
         throw UIKitScenarioAdapterFailure.invalidFixture
      }
      let assets = try loadAssets(scenario: scenario, fixtureData: fixtureData, loader: loader)
      self.scenario = scenario
      self.fixture = fixture
      self.fixtureData = fixtureData
      self.assets = assets
      try installScene(scenario: scenario, fixture: fixture, fixtureData: fixtureData, assets: assets, fonts: fonts)
   }

   private func installScene(scenario: BenchmarkScenario, fixture: [String: Any], fixtureData: Data, assets: UIKitPreparedAssets, fonts: UIKitPreparedFonts) throws
   {
      let scene: UIKitBenchmarkScene
      switch scenario.id
      {
      case "startup.first-screen": scene = try UIKitStartupScene(fixtureData: fixtureData, atlas: assets.atlas, fonts: fonts)
      case "dashboard.mixed-static": scene = try UIKitDashboardScene(fixture: fixture, atlas: assets.atlas, fonts: fonts)
      case "feed.variable-scroll": scene = try UIKitFeedScene(fixture: fixture, atlas: assets.atlas, inlineText: assets.inlineText, fonts: fonts)
      case "chat.live-update": scene = try UIKitChatScene(fixtureData: fixtureData, atlas: assets.atlas, inlineText: assets.inlineText, fonts: fonts)
      case "navigation.modal": scene = try UIKitNavigationScene(fixture: fixture, correctnessMode: correctnessMode, fonts: fonts)
      case "image.decode-zoom": scene = try UIKitImageScene(fixtureData: fixtureData, assets: assets, fonts: fonts)
      default: throw UIKitScenarioAdapterFailure.unsupportedScenario(scenario.id)
      }
      self.scene = scene
      let viewport = BenchmarkViewportController(content: scene.viewController)
      self.viewport = viewport
      hostController.addChild(viewport)
      hostController.view.addSubview(viewport.view)
      viewport.didMove(toParent: hostController)
      viewport.view.frame = hostController.view.bounds
      viewport.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
      window.makeKeyAndVisible()
      viewport.view.setNeedsLayout()
      viewport.view.layoutIfNeeded()
   }

   private func loadAssets(scenario: BenchmarkScenario, fixtureData: Data, loader: BenchmarkSpecLoader) throws -> UIKitPreparedAssets
   {
      let manifest = try loader.loadAssetManifest(scenario.assets)
      let inlineText: UIKitPreparedInlineText?
      if let contract = manifest.inlineTextAtlas
      {
         var rasterImages = [UInt32: CGImage]()
         for variant in contract.variants
         {
            guard let artifact = manifest.artifacts.first(where: {$0.role == variant.artifactRole}),
                  let image = UIImage(data: try loader.read(artifact.artifact), scale: 1)?.cgImage,
                  image.width == Int(variant.pixelWidth),
                  image.height == Int(variant.pixelHeight) else
            {
               throw UIKitScenarioAdapterFailure.invalidFixture
            }
            rasterImages[variant.emPixels] = image
         }
         inlineText = try UIKitPreparedInlineText(contract: contract, rasterImages: rasterImages)
      }
      else if manifest.inlineTextAtlas == nil
      {
         inlineText = nil
      }
      else
      {
         throw UIKitScenarioAdapterFailure.invalidFixture
      }
      if scenario.id == "image.decode-zoom"
      {
         let fixture = try uikitFixtureDecoder.decode(UIKitImageFixture.self, from: fixtureData)
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
               let sourceArtifact = manifest.artifacts.first(where: {$0.role == "source-image"}),
               sourceArtifact.artifact.path == fixture.source.artifact.path,
               sourceArtifact.artifact.sha256 == fixture.source.artifact.sha256,
               let thumbnailArtifact = manifest.artifacts.first(where: {$0.role == "thumbnail"}),
               thumbnailArtifact.artifact.path == fixture.thumbnail.artifact.path,
               thumbnailArtifact.artifact.sha256 == fixture.thumbnail.artifact.sha256 else
         {
            throw UIKitScenarioAdapterFailure.invalidFixture
         }
         let sourceData = try loader.read(fixture.source.artifact)
         guard let thumbnail = UIImage(data: try loader.read(fixture.thumbnail.artifact), scale: 1),
               let thumbnailImage = thumbnail.cgImage,
               thumbnailImage.width == fixture.thumbnail.width,
               thumbnailImage.height == fixture.thumbnail.height else
         {
            throw UIKitScenarioAdapterFailure.invalidFixture
         }
         return UIKitPreparedAssets(
            sourceData: sourceData,
            sourceSize: CGSize(width: fixture.source.width, height: fixture.source.height),
            thumbnail: thumbnail
         )
      }
      guard let artifact = manifest.artifacts.first(where: {$0.role == "thumbnail-atlas"}),
            let image = UIImage(data: try loader.read(artifact.artifact)),
            let cgImage = image.cgImage,
            cgImage.width == 384,
            cgImage.height == 192 else
      {
         throw UIKitScenarioAdapterFailure.invalidFixture
      }
      var tiles = [UIImage]()
      tiles.reserveCapacity(128)
      for index in 0..<128
      {
         let rect = CGRect(x: (index % 16) * 24, y: (index / 16) * 24, width: 24, height: 24)
         guard let tile = cgImage.cropping(to: rect) else
         {
            throw UIKitScenarioAdapterFailure.invalidFixture
         }
         tiles.append(UIImage(cgImage: tile, scale: 0.5, orientation: .up))
      }
      return UIKitPreparedAssets(atlasImage: image, atlas: tiles, inlineText: inlineText)
   }

   func reset() throws
   {
      guard let scene, let viewport else
      {
         throw UIKitScenarioAdapterFailure.missingScenario
      }
      try scene.reset()
      UIView.performWithoutAnimation
      {
         viewport.view.setNeedsLayout()
         viewport.view.layoutIfNeeded()
      }
      try quiesce()
   }

   func quiesce() throws
   {
      guard let view = scene?.viewController.view else
      {
         throw UIKitScenarioAdapterFailure.missingScenario
      }
      UIView.performWithoutAnimation
      {
         view.setNeedsLayout()
         view.layoutIfNeeded()
      }
      uikitRemoveLayerTreeAnimations(view.layer)
      CATransaction.flush()
      guard !uikitLayerTreeHasAnimations(view.layer) else
      {
         throw UIKitScenarioAdapterFailure.activeAnimations
      }
   }

   func apply(event: BenchmarkTraceEvent) throws
   {
      guard let scene else
      {
         throw UIKitScenarioAdapterFailure.missingScenario
      }
      try scene.apply(event: event)
   }

   func checkpoint(id: String) throws -> BenchmarkAdapterCheckpoint
   {
      guard let scene, let scenario else
      {
         throw UIKitScenarioAdapterFailure.missingScenario
      }
      let counts = scene.roleCounts()
      return try benchmarkCheckpoint(scenario: scenario, checkpointID: id, model: scene.state(checkpointID: id), visibleRoleCounts: counts)
   }

   func teardown() throws
   {
      hostController.view.endEditing(true)
      scene?.teardown()
      viewport?.detachContent()
      viewport?.willMove(toParent: nil)
      viewport?.view.removeFromSuperview()
      viewport?.removeFromParent()
      hostController.view.setNeedsLayout()
      hostController.view.layoutIfNeeded()
      scene = nil
      viewport = nil
      scenario = nil
      fixture = nil
      fixtureData = nil
      assets = nil
      CATransaction.flush()
   }

   func previewPNG() throws -> Data
   {
      guard let captureView = viewport?.captureView else
      {
         throw UIKitScenarioAdapterFailure.missingScenario
      }
      let format = UIGraphicsImageRendererFormat()
      format.scale = 3
      format.opaque = true
      format.preferredRange = .standard
      let bounds = CGRect(x: 0, y: 0, width: 390, height: 844)
      let renderer = UIGraphicsImageRenderer(bounds: bounds, format: format)
      let image = renderer.image {context in captureView.layer.render(in: context.cgContext)}
      guard let png = image.pngData() else
      {
         throw UIKitScenarioAdapterFailure.missingScenario
      }
      return png
   }
}

private final class BenchmarkViewportController: UIViewController
{
   private let content: UIViewController
   var captureView: UIView {content.view}

   init(content: UIViewController)
   {
      self.content = content
      super.init(nibName: nil, bundle: nil)
   }

   required init?(coder: NSCoder)
   {
      fatalError("BenchmarkViewportController does not support coder initialization")
   }

   override func viewDidLoad()
   {
      super.viewDidLoad()
      view.backgroundColor = .black
      addChild(content)
      view.addSubview(content.view)
      content.didMove(toParent: self)
   }

   override func viewDidLayoutSubviews()
   {
      super.viewDidLayoutSubviews()
      content.view.bounds = CGRect(x: 0, y: 0, width: 390, height: 844)
      content.view.center = CGPoint(x: view.bounds.midX, y: view.bounds.midY)
      content.view.setNeedsLayout()
      content.view.layoutIfNeeded()
   }

   func detachContent()
   {
      content.willMove(toParent: nil)
      content.view.removeFromSuperview()
      content.removeFromParent()
   }
}

private final class UIKitStartupScene: NSObject, UIKitBenchmarkScene, UICollectionViewDataSource, UICollectionViewDelegateFlowLayout
{
   private let controller = StartupViewController()
   var viewController: UIViewController {controller}
   private let header = UILabel()
   private let navigation = UILabel()
   private let primaryControl = UIButton(type: .custom)
   private let collectionView: UICollectionView
   private let cards: [UIKitStartupCard]
   private let payload: Data
   private let atlas: [UIImage]
   private let latinFont: UIFont
   private let detailFont: UIFont
   private var foregroundCount = 0
   private var backgroundCount = 0
   private var freshInstallReady = false
   private var lifecycleStateID: String?

   init(fixtureData: Data, atlas: [UIImage], fonts: UIKitPreparedFonts) throws
   {
      let fixture = try uikitFixtureDecoder.decode(UIKitStartupFixture.self, from: fixtureData)
      guard fixture.schemaVersion == 1,
            fixture.id == "startup.first-screen",
            fixture.dataSizeBytes == 24 * 1_024,
            fixture.data.utf8.count == fixture.dataSizeBytes,
            fixture.cardCount == 24,
            fixture.cards.count == fixture.cardCount,
            fixture.initialImageIndices == [0, 1, 2, 3, 4, 5],
            fixture.headerId == "startup:header",
            fixture.navigationId == "startup:navigation",
            fixture.controlId == "startup:primary-control",
            atlas.count >= 6,
            let payload = fixture.data.data(using: .utf8) else
      {
         throw UIKitScenarioAdapterFailure.invalidFixture
      }
      for (index, card) in fixture.cards.enumerated()
      {
         guard card.id == String(format: "startup:card:%02d", index),
               card.dataOffset == index * 1_024,
               card.dataLength == 1_024,
               card.thumbnailIndex == index % 6,
               card.initiallyVisible == (index < 6) else
         {
            throw UIKitScenarioAdapterFailure.invalidFixture
         }
      }
      let layout = UICollectionViewFlowLayout()
      layout.scrollDirection = .vertical
      layout.minimumInteritemSpacing = 12
      layout.minimumLineSpacing = 12
      collectionView = UICollectionView(frame: .zero, collectionViewLayout: layout)
      cards = fixture.cards
      self.payload = payload
      self.atlas = atlas
      latinFont = fonts.latin15
      detailFont = fonts.latin11
      super.init()

      controller.view.backgroundColor = comparisonBackground
      header.text = "Production Comparison"
      header.font = fonts.latin20
      header.textColor = comparisonText
      header.accessibilityIdentifier = "header"
      navigation.text = "First Screen"
      navigation.font = latinFont
      navigation.textColor = comparisonSecondaryText
      navigation.backgroundColor = comparisonSurface
      navigation.layer.cornerRadius = 12
      navigation.layer.cornerCurve = .circular
      navigation.layer.masksToBounds = true
      navigation.accessibilityIdentifier = "navigation"
      primaryControl.setTitle("Continue", for: .normal)
      primaryControl.setTitleColor(comparisonSurface, for: .normal)
      primaryControl.titleLabel?.font = latinFont
      primaryControl.backgroundColor = comparisonAccent
      primaryControl.layer.cornerRadius = 0
      primaryControl.accessibilityIdentifier = "primary-control"
      collectionView.backgroundColor = .clear
      collectionView.showsVerticalScrollIndicator = false
      collectionView.dataSource = self
      collectionView.delegate = self
      collectionView.register(UIKitStartupCardCell.self, forCellWithReuseIdentifier: UIKitStartupCardCell.reuseIdentifier)
      controller.view.addSubview(header)
      controller.view.addSubview(navigation)
      controller.view.addSubview(collectionView)
      controller.view.addSubview(primaryControl)
      controller.layoutHandler = {[weak self] in self?.layout()}
   }

   func apply(event: BenchmarkTraceEvent) throws
   {
      switch event.op
      {
      case "foreground":
         foregroundCount += 1
         controller.view.isHidden = false
      case "background":
         backgroundCount += 1
         controller.view.isHidden = true
      case "resource-arrival":
         guard event.target == "fresh-install" else
         {
            throw UIKitScenarioAdapterFailure.unsupportedEvent(event.target ?? event.op)
         }
         freshInstallReady = true
      default: throw UIKitScenarioAdapterFailure.unsupportedEvent(event.op)
      }
      lifecycleStateID = event.stateId
   }

   func reset() throws
   {
      foregroundCount = 0
      backgroundCount = 0
      freshInstallReady = false
      lifecycleStateID = nil
      controller.view.isHidden = false
      controller.view.endEditing(true)
      collectionView.setContentOffset(.zero, animated: false)
      collectionView.reloadData()
      collectionView.layoutIfNeeded()
      controller.view.layer.removeAllAnimations()
   }

   func state(checkpointID: String) -> [String: Any]
   {
      [
         "foreground_count": foregroundCount,
         "background_count": backgroundCount,
         "fresh_install_ready": freshInstallReady,
         "scene_visible": !controller.view.isHidden,
         "lifecycle_state_id": lifecycleStateID ?? NSNull(),
         "card_count": cards.count,
      ]
   }

   func roleCounts() -> [BenchmarkRoleCount]
   {
      [
         BenchmarkRoleCount(role: "header", count: 1),
         BenchmarkRoleCount(role: "navigation", count: 1),
         BenchmarkRoleCount(role: "card", count: 6),
         BenchmarkRoleCount(role: "initial-image", count: 6),
         BenchmarkRoleCount(role: "primary-control", count: 1),
      ]
   }

   func teardown()
   {
      controller.view.layer.removeAllAnimations()
      collectionView.dataSource = nil
      collectionView.delegate = nil
   }

   func collectionView(_ collectionView: UICollectionView, numberOfItemsInSection section: Int) -> Int
   {
      cards.count
   }

   func collectionView(_ collectionView: UICollectionView, cellForItemAt indexPath: IndexPath) -> UICollectionViewCell
   {
      let cell = collectionView.dequeueReusableCell(withReuseIdentifier: UIKitStartupCardCell.reuseIdentifier, for: indexPath)
      guard let cell = cell as? UIKitStartupCardCell else {return cell}
      let card = cards[indexPath.item]
      let range = card.dataOffset..<(card.dataOffset + card.dataLength)
      cell.configure(
         card: card,
         payload: payload.subdata(in: range),
         image: card.initiallyVisible ? atlas[card.thumbnailIndex] : nil,
         font: latinFont,
         detailFont: detailFont
      )
      return cell
   }

   func collectionView(_ collectionView: UICollectionView, layout collectionViewLayout: UICollectionViewLayout, sizeForItemAt indexPath: IndexPath) -> CGSize
   {
      CGSize(width: 173, height: 176)
   }

   private func layout()
   {
      header.frame = CGRect(x: 16, y: 20, width: 358, height: 48)
      navigation.frame = CGRect(x: 16, y: 76, width: 358, height: 44)
      collectionView.frame = CGRect(x: 16, y: 132, width: 358, height: 552)
      primaryControl.frame = CGRect(x: 16, y: 776, width: 358, height: 48)
      collectionView.collectionViewLayout.invalidateLayout()
   }
}

private final class StartupViewController: UIViewController
{
   var layoutHandler: (() -> Void)?

   override func viewDidLayoutSubviews()
   {
      super.viewDidLayoutSubviews()
      layoutHandler?()
   }
}

private final class UIKitStartupCardCell: UICollectionViewCell
{
   static let reuseIdentifier = "startup-card"
   private let imageView = UIImageView()
   private let titleLabel = UILabel()
   private let detailLabel = UILabel()

   override init(frame: CGRect)
   {
      super.init(frame: frame)
      contentView.backgroundColor = .white
      contentView.layer.cornerRadius = 12
      contentView.layer.cornerCurve = .circular
      contentView.layer.shadowColor = comparisonText.cgColor
      contentView.layer.shadowOpacity = 41 / 255
      contentView.layer.shadowRadius = 0
      contentView.layer.shadowOffset = CGSize(width: 0, height: 2)
      contentView.addSubview(imageView)
      contentView.addSubview(titleLabel)
      contentView.addSubview(detailLabel)
      imageView.contentMode = .scaleAspectFill
      imageView.layer.cornerRadius = 8
      imageView.layer.cornerCurve = .circular
      imageView.clipsToBounds = true
      titleLabel.lineBreakMode = .byClipping
      detailLabel.numberOfLines = 1
      detailLabel.lineBreakMode = .byClipping
      accessibilityIdentifier = "card"
   }

   required init?(coder: NSCoder)
   {
      fatalError("UIKitStartupCardCell does not support coder initialization")
   }

   func configure(card: UIKitStartupCard, payload: Data, image: UIImage?, font: UIFont, detailFont: UIFont)
   {
      titleLabel.font = font
      titleLabel.textColor = comparisonText
      titleLabel.text = card.id
      detailLabel.font = detailFont
      detailLabel.textColor = comparisonSecondaryText
      detailLabel.text = String(decoding: payload.prefix(48), as: UTF8.self)
      imageView.image = image
      imageView.accessibilityIdentifier = image == nil ? nil : "initial-image"
   }

   override func prepareForReuse()
   {
      super.prepareForReuse()
      imageView.image = nil
      imageView.accessibilityIdentifier = nil
      titleLabel.text = nil
      detailLabel.text = nil
   }

   override func layoutSubviews()
   {
      super.layoutSubviews()
      imageView.frame = CGRect(x: 12, y: 12, width: 48, height: 48)
      titleLabel.frame = CGRect(x: 72, y: 14, width: bounds.width - 84, height: 22)
      detailLabel.frame = CGRect(x: 72, y: 42, width: bounds.width - 84, height: 54)
   }
}

final class UIKitDashboardScene: UIKitBenchmarkScene
{
   private let controller = DashboardViewController()
   var viewController: UIViewController {controller}
   private var labels = [UILabel]()
   private var images = [UIImageView]()
   private var cards = [UIView]()
   private var controls = [UIButton]()
   private var backdrops = [UIVisualEffectView]()
   private var initialLabelTexts = [String]()
   private var initialLabelColors = [UIColor]()
   private var leafUpdates = 0
   private var bulkUpdates = 0

   init(fixture: [String: Any], atlas: [UIImage], fonts: UIKitPreparedFonts) throws
   {
      guard fixture["visible_node_count"] as? Int == 300 else
      {
         throw UIKitScenarioAdapterFailure.invalidFixture
      }
      viewController.view.backgroundColor = comparisonBackground
      for index in 0..<32
      {
         let card = UIView()
         card.backgroundColor = .white
         card.layer.cornerRadius = 12
         card.layer.cornerCurve = .circular
         card.layer.shadowColor = comparisonText.cgColor
         card.layer.shadowOpacity = 41 / 255
         card.layer.shadowRadius = 0
         card.layer.shadowOffset = CGSize(width: 0, height: 2)
         card.accessibilityIdentifier = "rounded-card"
         cards.append(card)
         viewController.view.addSubview(card)
         for imageIndex in 0..<2
         {
            let image = UIImageView(image: atlas[index * 2 + imageIndex])
            image.accessibilityIdentifier = "icon-image"
            images.append(image)
            card.addSubview(image)
         }
         let labelCount = index < 16 ? 6 : 5
         for labelIndex in 0..<labelCount
         {
            let label = UILabel()
            label.font = fonts.latin7
            label.textColor = labelIndex == 0 ? comparisonText : comparisonSecondaryText
            label.text = "Node \(index * 6 + labelIndex)"
            label.accessibilityIdentifier = "label"
            labels.append(label)
            card.addSubview(label)
         }
         if index < 24
         {
            let control = UIButton(type: .custom)
            control.backgroundColor = comparisonAccent
            control.layer.cornerRadius = 0
            control.accessibilityLabel = "Action"
            control.accessibilityIdentifier = "control"
            controls.append(control)
            card.addSubview(control)
         }
      }
      initialLabelTexts = labels.map {$0.text ?? ""}
      initialLabelColors = labels.map {$0.textColor ?? comparisonText}
      for _ in 0..<4
      {
         let blur = UIVisualEffectView(effect: UIBlurEffect(style: .systemMaterialLight))
         blur.layer.cornerRadius = 0
         blur.clipsToBounds = false
         blur.contentView.backgroundColor = UIColor(red: 225 / 255, green: 229 / 255, blue: 238 / 255, alpha: 56 / 255)
         blur.accessibilityIdentifier = "backdrop-region"
         backdrops.append(blur)
         viewController.view.insertSubview(blur, at: 0)
      }
      controller.layoutHandler = {[weak self] in self?.layout()}
   }

   func apply(event: BenchmarkTraceEvent) throws
   {
      guard event.op == "mutate", let target = event.target else
      {
         throw UIKitScenarioAdapterFailure.unsupportedEvent(event.op)
      }
      if target == "dashboard:update-count"
      {
         bulkUpdates = 30
         for label in labels.prefix(30)
         {
            label.textColor = comparisonAccent
         }
      }
      else if target.hasPrefix("dashboard:label:")
      {
         leafUpdates += 1
         let components = target.split(separator: ":")
         if let last = components.last, let index = Int(last), index < labels.count
         {
            labels[index].text = "Updated \(leafUpdates)"
         }
      }
   }

   func reset() throws
   {
      leafUpdates = 0
      bulkUpdates = 0
      for index in labels.indices
      {
         labels[index].text = initialLabelTexts[index]
         labels[index].textColor = initialLabelColors[index]
      }
      for control in controls
      {
         control.isHighlighted = false
         control.isSelected = false
      }
      controller.view.endEditing(true)
      controller.view.layer.removeAllAnimations()
      controller.view.setNeedsLayout()
      controller.view.layoutIfNeeded()
   }

   func state(checkpointID: String) -> [String: Any]
   {
      [
         "leaf_update_count": leafUpdates,
         "bulk_update_count": bulkUpdates,
         "visible_node_count": 1 + labels.count + images.count + cards.count + controls.count + backdrops.count,
      ]
   }

   func roleCounts() -> [BenchmarkRoleCount]
   {
      [
         BenchmarkRoleCount(role: "dashboard", count: 1),
         BenchmarkRoleCount(role: "label", count: UInt32(labels.count)),
         BenchmarkRoleCount(role: "icon-image", count: UInt32(images.count)),
         BenchmarkRoleCount(role: "rounded-card", count: UInt32(cards.count)),
         BenchmarkRoleCount(role: "control", count: UInt32(controls.count)),
         BenchmarkRoleCount(role: "backdrop-region", count: UInt32(backdrops.count)),
      ]
   }

   func teardown()
   {
      viewController.view.layer.removeAllAnimations()
   }

   func layout()
   {
      let width = viewController.view.bounds.width
      let cardWidth = (width - 44) / 2
      for (index, card) in cards.enumerated()
      {
         let column = CGFloat(index % 2)
         let row = CGFloat(index / 2)
         card.frame = CGRect(x: 16 + column * (cardWidth + 12), y: 48 + row * 46, width: cardWidth, height: 38)
         let cardLabels = card.subviews.compactMap {$0 as? UILabel}
         let cardImages = card.subviews.compactMap {$0 as? UIImageView}
         for (imageIndex, image) in cardImages.enumerated()
         {
            image.frame = CGRect(x: 6 + CGFloat(imageIndex) * 18, y: 6, width: 14, height: 14)
         }
         for (labelIndex, label) in cardLabels.enumerated()
         {
            label.frame = CGRect(x: 42, y: 2 + CGFloat(labelIndex) * 6, width: cardWidth - 48, height: 6)
         }
         card.subviews.compactMap {$0 as? UIButton}.first?.frame = CGRect(x: 145, y: 13, width: 18, height: 12)
      }
      for (index, blur) in backdrops.enumerated()
      {
         blur.frame = CGRect(x: 16, y: 64 + CGFloat(index) * 204, width: width - 32, height: 96)
      }
   }
}

private final class DashboardViewController: UIViewController
{
   var layoutHandler: (() -> Void)?

   override func viewDidLayoutSubviews()
   {
      super.viewDidLayoutSubviews()
      layoutHandler?()
   }
}

private final class UIKitFeedCell: UITableViewCell
{
   private let card = UIView()
   private let thumbnail = UIImageView()
   private let primaryLabel = UILabel()
   private let secondaryLabel = UILabel()
   private let favoriteControl = UIButton(type: .custom)

   override init(style: UITableViewCell.CellStyle, reuseIdentifier: String?)
   {
      super.init(style: style, reuseIdentifier: reuseIdentifier)
      backgroundColor = .clear
      contentView.backgroundColor = .clear
      selectionStyle = .none
      clipsToBounds = false
      contentView.clipsToBounds = false

      card.backgroundColor = comparisonSurface
      card.layer.cornerRadius = 12
      card.layer.cornerCurve = .circular
      card.layer.shadowColor = comparisonText.cgColor
      card.layer.shadowOpacity = 41 / 255
      card.layer.shadowRadius = 0
      card.layer.shadowOffset = CGSize(width: 0, height: 2)
      card.accessibilityIdentifier = "feed-card"
      contentView.addSubview(card)

      thumbnail.layer.cornerRadius = 8
      thumbnail.layer.cornerCurve = .circular
      thumbnail.clipsToBounds = true
      thumbnail.accessibilityIdentifier = "thumbnail"
      card.addSubview(thumbnail)

      primaryLabel.textColor = comparisonText
      primaryLabel.lineBreakMode = .byClipping
      secondaryLabel.textColor = comparisonSecondaryText
      secondaryLabel.lineBreakMode = .byClipping
      card.addSubview(primaryLabel)
      card.addSubview(secondaryLabel)

      favoriteControl.layer.cornerRadius = 9
      favoriteControl.layer.cornerCurve = .circular
      favoriteControl.accessibilityIdentifier = "favorite-control"
      favoriteControl.accessibilityLabel = "Favorite"
      card.addSubview(favoriteControl)
      accessibilityIdentifier = "feed-card"
   }

   required init?(coder: NSCoder)
   {
      fatalError("init(coder:) has not been implemented")
   }

   func configure(row: [String: Any], image: UIImage, font: UIFont, secondaryFont: UIFont, attributedText: NSAttributedString?, favorite: Bool)
   {
      thumbnail.image = image
      primaryLabel.font = font
      let text = row["text"] as? String
      primaryLabel.attributedText = attributedText
      if attributedText == nil
      {
         primaryLabel.text = text
      }
      primaryLabel.accessibilityLabel = text
      secondaryLabel.font = secondaryFont
      secondaryLabel.text = row["id"] as? String
      favoriteControl.backgroundColor = favorite ? comparisonAccent : comparisonInactiveControl
      favoriteControl.isSelected = favorite
   }

   override func prepareForReuse()
   {
      super.prepareForReuse()
      thumbnail.image = nil
      primaryLabel.text = nil
      primaryLabel.attributedText = nil
      primaryLabel.accessibilityLabel = nil
      secondaryLabel.text = nil
      favoriteControl.isSelected = false
   }

   override func layoutSubviews()
   {
      super.layoutSubviews()
      card.frame = CGRect(x: 12, y: 4, width: bounds.width - 24, height: bounds.height - 8)
      card.layer.shadowPath = UIBezierPath(roundedRect: card.bounds, cornerRadius: 12).cgPath
      thumbnail.frame = CGRect(x: 10, y: 10, width: 48, height: 48)
      primaryLabel.frame = CGRect(x: 70, y: 8, width: card.bounds.width - 112, height: 24)
      secondaryLabel.frame = CGRect(x: 70, y: 34, width: card.bounds.width - 112, height: 18)
      favoriteControl.frame = CGRect(x: card.bounds.width - 34, y: 11, width: 18, height: 18)
   }
}

final class UIKitFeedScene: NSObject, UIKitBenchmarkScene, UITableViewDataSource, UITableViewDelegate
{
   let viewController = UIViewController()
   private let header = UIView()
   private let title = UILabel()
   private let tableView = UITableView(frame: .zero, style: .plain)
   private let initialRows: [[String: Any]]
   private var rows = [[String: Any]]()
   private var traceScrollMillionths: UInt32 = 0
   private var favoriteID: String?
   private var prependCount = 0
   private let latinFont: UIFont
   private let secondaryFont: UIFont
   private let arabicFont: UIFont
   private let cjkFont: UIFont
   private let atlas: [UIImage]
   private let inlineTexts: [String: NSAttributedString]

   init(fixture: [String: Any], atlas: [UIImage], inlineText: UIKitPreparedInlineText?, fonts: UIKitPreparedFonts) throws
   {
      guard let rows = fixture["rows"] as? [[String: Any]], rows.count == 2_000 else
      {
         throw UIKitScenarioAdapterFailure.invalidFixture
      }
      initialRows = rows
      self.rows = rows
      self.atlas = atlas
      latinFont = fonts.latin15
      secondaryFont = fonts.latin11
      arabicFont = fonts.arabic15
      cjkFont = fonts.cjk15
      var inlineTexts = [String: NSAttributedString]()
      if let inlineText
      {
         for row in rows
         {
            guard let text = row["text"] as? String, inlineTexts[text] == nil,
                  let attributed = inlineText.attributedText(text, font: fonts.latin15, color: comparisonText) else {continue}
            inlineTexts[text] = attributed
         }
      }
      self.inlineTexts = inlineTexts
      super.init()
      tableView.dataSource = self
      tableView.delegate = self
      tableView.register(UIKitFeedCell.self, forCellReuseIdentifier: "feed")
      tableView.accessibilityIdentifier = "feed"
      tableView.backgroundColor = comparisonBackground
      tableView.contentInsetAdjustmentBehavior = .never
      tableView.estimatedRowHeight = 0
      tableView.separatorStyle = .none
      tableView.frame = CGRect(x: 0, y: 52, width: 390, height: 792)
      tableView.autoresizingMask = []
      header.backgroundColor = comparisonSurface
      header.frame = CGRect(x: 0, y: 0, width: 390, height: 52)
      header.autoresizingMask = []
      title.font = fonts.latin20
      title.textColor = comparisonText
      title.text = "Measured Feed"
      title.frame = CGRect(x: 16, y: 12, width: 220, height: 28)
      header.addSubview(title)
      viewController.view.backgroundColor = comparisonBackground
      viewController.view.addSubview(header)
      viewController.view.addSubview(tableView)
      layout()
   }

   func apply(event: BenchmarkTraceEvent) throws
   {
      layout()
      switch event.op
      {
      case "pointer-move", "pointer-up":
         guard let y = event.yMillionths else {return}
         traceScrollMillionths = UInt32(max(0, 1_000_000 - y))
         let fraction = CGFloat(y) / 1_000_000
         let extent = max(0, tableView.contentSize.height - tableView.bounds.height)
         tableView.setContentOffset(CGPoint(x: 0, y: extent * (1 - fraction)), animated: false)
         tableView.layoutIfNeeded()
      case "mutate":
         if event.target == "feed:prepend-count"
         {
            let anchor = tableView.contentOffset.y
            prependCount = 20
            let prepended = (0..<20).map {index in ["id": String(format: "feed:prepend:%02d", index), "height": 76, "text": "Prepended \(index)", "thumbnail_index": index] as [String: Any]}
            rows.insert(contentsOf: prepended, at: 0)
            tableView.reloadData()
            tableView.layoutIfNeeded()
            tableView.setContentOffset(CGPoint(x: 0, y: anchor + 20 * 76), animated: false)
            tableView.layoutIfNeeded()
         }
         else if let target = event.target, target.hasSuffix(":favorite")
         {
            favoriteID = String(target.dropLast(":favorite".count))
            tableView.reloadData()
            tableView.layoutIfNeeded()
         }
      case "pointer-down": break
      default: throw UIKitScenarioAdapterFailure.unsupportedEvent(event.op)
      }
   }

   func reset() throws
   {
      rows = initialRows
      traceScrollMillionths = 0
      favoriteID = nil
      prependCount = 0
      viewController.view.endEditing(true)
      tableView.layer.removeAllAnimations()
      tableView.reloadData()
      tableView.setContentOffset(.zero, animated: false)
      layout()
      tableView.layoutIfNeeded()
   }

   func state(checkpointID: String) -> [String: Any]
   {
      return [
         "row_count": rows.count,
         "scroll_position_millionths": traceScrollMillionths,
         "favorite_id": favoriteID ?? NSNull(),
         "prepend_count": prependCount,
      ]
   }

   func roleCounts() -> [BenchmarkRoleCount]
   {
      layout()
      tableView.layoutIfNeeded()
      let visible = max(tableView.indexPathsForVisibleRows?.count ?? 0, 1)
      return [
         BenchmarkRoleCount(role: "navigation-bar", count: 1),
         BenchmarkRoleCount(role: "feed", count: 1),
         BenchmarkRoleCount(role: "feed-card", count: UInt32(visible)),
         BenchmarkRoleCount(role: "thumbnail", count: UInt32(visible)),
         BenchmarkRoleCount(role: "favorite-control", count: UInt32(visible)),
      ]
   }

   func teardown()
   {
      tableView.layer.removeAllAnimations()
      tableView.dataSource = nil
      tableView.delegate = nil
   }

   func tableView(_ tableView: UITableView, numberOfRowsInSection section: Int) -> Int
   {
      rows.count
   }

   func tableView(_ tableView: UITableView, heightForRowAt indexPath: IndexPath) -> CGFloat
   {
      CGFloat(rows[indexPath.row]["height"] as? Int ?? 76)
   }

   private func layout()
   {
      header.frame = CGRect(x: 0, y: 0, width: 390, height: 52)
      tableView.frame = CGRect(x: 0, y: 52, width: 390, height: 792)
   }

   func tableView(_ tableView: UITableView, cellForRowAt indexPath: IndexPath) -> UITableViewCell
   {
      guard let cell = tableView.dequeueReusableCell(withIdentifier: "feed", for: indexPath) as? UIKitFeedCell else
      {
         return UITableViewCell()
      }
      let row = rows[indexPath.row]
      let text = row["text"] as? String ?? ""
      let font: UIFont
      if text.unicodeScalars.contains(where: {(0x4E00...0x9FFF).contains(Int($0.value))})
      {
         font = cjkFont
      }
      else if text.unicodeScalars.contains(where: {(0x0600...0x06FF).contains(Int($0.value))})
      {
         font = arabicFont
      }
      else
      {
         font = latinFont
      }
      let thumbnailIndex = row["thumbnail_index"] as? Int ?? indexPath.row
      cell.configure(
         row: row,
         image: atlas[thumbnailIndex % atlas.count],
         font: font,
         secondaryFont: secondaryFont,
         attributedText: inlineTexts[text],
         favorite: favoriteID == row["id"] as? String
      )
      return cell
   }
}

private let uikitChatRowHeights: [CGFloat] = [58, 62, 66, 70, 74, 78, 82, 76, 70, 64]

private final class UIKitChatScene: NSObject, UIKitBenchmarkScene, UITableViewDataSource, UITableViewDelegate
{
   private let controller = ChatViewController()
   var viewController: UIViewController {controller}
   private let titleLabel = UILabel()
   private let tableView = UITableView(frame: .zero, style: .plain)
   private let composer = UITextView()
   private let sendControl = UIButton(type: .system)
   private let atlas: [UIImage]
   private let latinFont: UIFont
   private let arabicFont: UIFont
   private let cjkFont: UIFont
   private let inlineTexts: [String: NSAttributedString]
   private let appendTemplates: [UIKitChatMessage]
   private let prependMessages: [UIKitChatMessage]
   private let selectionContract: UIKitChatSelectionReplacement
   private let initialMessages: [UIKitChatMessage]
   private var messages: [UIKitChatMessage]
   private var selectedMessageID: String?
   private var selectedUTF8Range: Range<Int>?
   private var prependCount = 0
   private var appendCount = 0
   private var replacementApplied = false
   private var didInitialScroll = false

   init(fixtureData: Data, atlas: [UIImage], inlineText: UIKitPreparedInlineText?, fonts: UIKitPreparedFonts) throws
   {
      let fixture = try uikitFixtureDecoder.decode(UIKitChatFixture.self, from: fixtureData)
      guard fixture.schemaVersion == 1,
            fixture.id == "chat.live-update",
            fixture.messageCount == 5_000,
            fixture.messages.count == fixture.messageCount,
            fixture.avatarCount == 64,
            fixture.prependMessages.count == 50,
            fixture.appendRateHz == 10,
            fixture.typedText.count == 100,
            fixture.pastedText.utf8.count == 10 * 1_024,
            fixture.selectionReplacement.messageId == "chat:message:4096",
            fixture.selectionReplacement.startUtf8 == 0,
            fixture.selectionReplacement.endUtf8 == 6,
            fixture.selectionReplacement.replacement == "Oxide",
            atlas.count >= fixture.avatarCount else
      {
         throw UIKitScenarioAdapterFailure.invalidFixture
      }
      initialMessages = fixture.messages
      messages = fixture.messages
      prependMessages = fixture.prependMessages
      appendTemplates = Array(fixture.messages.prefix(8))
      selectionContract = fixture.selectionReplacement
      self.atlas = atlas
      latinFont = fonts.latin15
      arabicFont = fonts.arabic15
      cjkFont = fonts.cjk15
      var inlineTexts = [String: NSAttributedString]()
      if let inlineText
      {
         for message in fixture.messages + fixture.prependMessages
         {
            guard inlineTexts[message.text] == nil,
                  let attributed = inlineText.attributedText(message.text, font: fonts.latin15, color: comparisonText) else {continue}
            inlineTexts[message.text] = attributed
         }
      }
      self.inlineTexts = inlineTexts
      super.init()

      controller.view.backgroundColor = comparisonBackground
      titleLabel.text = "Live Chat"
      titleLabel.font = fonts.latin20
      titleLabel.textColor = comparisonText
      tableView.backgroundColor = .clear
      tableView.separatorStyle = .none
      tableView.showsVerticalScrollIndicator = false
      tableView.dataSource = self
      tableView.delegate = self
      tableView.register(UIKitChatCell.self, forCellReuseIdentifier: UIKitChatCell.reuseIdentifier)
      tableView.accessibilityIdentifier = "chat-thread"
      composer.font = latinFont
      composer.textColor = comparisonText
      composer.backgroundColor = .white
      composer.layer.cornerRadius = 12
      composer.layer.cornerCurve = .circular
      composer.textContainerInset = UIEdgeInsets(top: 8, left: 10, bottom: 8, right: 10)
      composer.accessibilityIdentifier = "composer"
      sendControl.setTitle("Send", for: .normal)
      sendControl.titleLabel?.font = fonts.latin13
      sendControl.setTitleColor(comparisonAccent, for: .normal)
      sendControl.accessibilityIdentifier = "send-control"
      controller.view.addSubview(titleLabel)
      controller.view.addSubview(tableView)
      controller.view.addSubview(composer)
      controller.view.addSubview(sendControl)
      controller.layoutHandler = {[weak self] in self?.layout()}
   }

   func apply(event: BenchmarkTraceEvent) throws
   {
      switch event.op
      {
      case "mutate": try applyMutation(event)
      case "focus": try applySelection(event)
      case "commit-text": try commitText(event)
      default: throw UIKitScenarioAdapterFailure.unsupportedEvent(event.op)
      }
   }

   func reset() throws
   {
      messages = initialMessages
      selectedMessageID = nil
      selectedUTF8Range = nil
      prependCount = 0
      appendCount = 0
      replacementApplied = false
      composer.resignFirstResponder()
      composer.text = ""
      composer.selectedRange = NSRange(location: 0, length: 0)
      composer.undoManager?.removeAllActions()
      tableView.layer.removeAllAnimations()
      tableView.reloadData()
      tableView.layoutIfNeeded()
      scrollToBottom()
      didInitialScroll = true
      controller.view.layer.removeAllAnimations()
   }

   func state(checkpointID: String) -> [String: Any]
   {
      [
         "message_count": messages.count,
         "prepend_count": prependCount,
         "append_count": appendCount,
         "composer_utf8_count": composer.text.utf8.count,
         "focused_message_id": selectedMessageID ?? NSNull(),
         "selection_active": selectedUTF8Range != nil,
         "replacement_applied": replacementApplied,
      ]
   }

   func roleCounts() -> [BenchmarkRoleCount]
   {
      [
         BenchmarkRoleCount(role: "chat-thread", count: 1),
         BenchmarkRoleCount(role: "message", count: 10),
         BenchmarkRoleCount(role: "avatar", count: 10),
         BenchmarkRoleCount(role: "composer", count: 1),
         BenchmarkRoleCount(role: "send-control", count: 1),
      ]
   }

   func teardown()
   {
      controller.view.layer.removeAllAnimations()
      tableView.dataSource = nil
      tableView.delegate = nil
   }

   func tableView(_ tableView: UITableView, numberOfRowsInSection section: Int) -> Int
   {
      messages.count
   }

   func tableView(_ tableView: UITableView, heightForRowAt indexPath: IndexPath) -> CGFloat
   {
      uikitChatRowHeights[messages[indexPath.row].sequence % uikitChatRowHeights.count]
   }

   func tableView(_ tableView: UITableView, cellForRowAt indexPath: IndexPath) -> UITableViewCell
   {
      let cell = tableView.dequeueReusableCell(withIdentifier: UIKitChatCell.reuseIdentifier, for: indexPath)
      guard let cell = cell as? UIKitChatCell else {return cell}
      let message = messages[indexPath.row]
      cell.configure(
         message: message,
         avatar: atlas[message.avatarIndex % atlas.count],
         font: font(for: message.text),
         attributedText: inlineTexts[message.text]
      )
      return cell
   }

   private func applyMutation(_ event: BenchmarkTraceEvent) throws
   {
      if event.target == "chat:prepend-count"
      {
         guard case .integer(let count)? = event.value, count == 50, prependCount == 0 else
         {
            throw UIKitScenarioAdapterFailure.unsupportedEvent(event.target ?? event.op)
         }
         messages.insert(contentsOf: prependMessages, at: 0)
         prependCount = prependMessages.count
         tableView.reloadData()
         scrollToBottom()
         return
      }
      guard event.target == "chat:append",
            case .text(let id)? = event.value,
            let suffix = Int(id.split(separator: ":").last ?? ""),
            suffix == appendCount else
      {
         throw UIKitScenarioAdapterFailure.unsupportedEvent(event.target ?? event.op)
      }
      let template = appendTemplates[suffix % appendTemplates.count]
      let message = UIKitChatMessage(
         id: id,
         sequence: 5_050 + suffix,
         authorIndex: suffix % 64,
         avatarIndex: suffix % 64,
         direction: template.direction,
         text: template.text
      )
      messages.append(message)
      appendCount += 1
      tableView.insertRows(at: [IndexPath(row: messages.count - 1, section: 0)], with: .none)
      scrollToBottom()
   }

   private func applySelection(_ event: BenchmarkTraceEvent) throws
   {
      guard event.target == selectionContract.messageId,
            case .text(let value)? = event.value else
      {
         throw UIKitScenarioAdapterFailure.unsupportedEvent(event.target ?? event.op)
      }
      let components = value.split(separator: ":")
      guard components.count == 2,
            let start = Int(components[0]),
            let end = Int(components[1]),
            start == selectionContract.startUtf8,
            end == selectionContract.endUtf8 else
      {
         throw UIKitScenarioAdapterFailure.unsupportedEvent(value)
      }
      selectedMessageID = event.target
      selectedUTF8Range = start..<end
   }

   private func commitText(_ event: BenchmarkTraceEvent) throws
   {
      guard case .text(let value)? = event.value else
      {
         throw UIKitScenarioAdapterFailure.unsupportedEvent(event.op)
      }
      if event.target == "chat:composer"
      {
         composer.insertText(value)
         return
      }
      guard event.target == selectedMessageID,
            event.target == selectionContract.messageId,
            value == selectionContract.replacement,
            let selectedUTF8Range,
            let index = messages.firstIndex(where: {$0.id == event.target}),
            let replaced = replacingUTF8(in: messages[index].text, range: selectedUTF8Range, with: value) else
      {
         throw UIKitScenarioAdapterFailure.unsupportedEvent(event.target ?? event.op)
      }
      messages[index].text = replaced
      self.selectedUTF8Range = nil
      replacementApplied = true
      if let visible = tableView.indexPathsForVisibleRows, visible.contains(IndexPath(row: index, section: 0))
      {
         tableView.reloadRows(at: [IndexPath(row: index, section: 0)], with: .none)
      }
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

   private func font(for text: String) -> UIFont
   {
      if text.unicodeScalars.contains(where: {(0x4E00...0x9FFF).contains(Int($0.value))})
      {
         return cjkFont
      }
      if text.unicodeScalars.contains(where: {(0x0600...0x06FF).contains(Int($0.value))})
      {
         return arabicFont
      }
      return latinFont
   }

   private func layout()
   {
      titleLabel.frame = CGRect(x: 16, y: 8, width: 358, height: 36)
      tableView.frame = CGRect(x: 0, y: 52, width: 390, height: 700)
      composer.frame = CGRect(x: 12, y: 780, width: 318, height: 48)
      sendControl.frame = CGRect(x: 338, y: 780, width: 40, height: 48)
      if !didInitialScroll
      {
         tableView.reloadData()
         scrollToBottom()
         didInitialScroll = true
      }
   }

   private func scrollToBottom()
   {
      tableView.layoutIfNeeded()
      guard !messages.isEmpty else {return}
      tableView.scrollToRow(at: IndexPath(row: messages.count - 1, section: 0), at: .bottom, animated: false)
   }
}

private final class ChatViewController: UIViewController
{
   var layoutHandler: (() -> Void)?

   override func viewDidLayoutSubviews()
   {
      super.viewDidLayoutSubviews()
      layoutHandler?()
   }
}

private final class UIKitChatCell: UITableViewCell
{
   static let reuseIdentifier = "chat-message"
   private let avatarView = UIImageView()
   private let bubbleView = UIView()
   private let messageLabel = UILabel()
   private var rightToLeft = false

   override init(style: UITableViewCell.CellStyle, reuseIdentifier: String?)
   {
      super.init(style: style, reuseIdentifier: reuseIdentifier)
      backgroundColor = .clear
      selectionStyle = .none
      avatarView.contentMode = .scaleAspectFill
      avatarView.layer.cornerRadius = 18
      avatarView.layer.cornerCurve = .circular
      avatarView.clipsToBounds = true
      avatarView.accessibilityIdentifier = "avatar"
      bubbleView.backgroundColor = .white
      bubbleView.layer.cornerRadius = 12
      bubbleView.layer.cornerCurve = .circular
      bubbleView.layer.shadowColor = comparisonText.cgColor
      bubbleView.layer.shadowOpacity = 41 / 255
      bubbleView.layer.shadowRadius = 0
      bubbleView.layer.shadowOffset = CGSize(width: 0, height: 2)
      messageLabel.numberOfLines = 2
      messageLabel.accessibilityIdentifier = "message"
      bubbleView.addSubview(messageLabel)
      contentView.addSubview(avatarView)
      contentView.addSubview(bubbleView)
      accessibilityIdentifier = "message"
   }

   required init?(coder: NSCoder)
   {
      fatalError("UIKitChatCell does not support coder initialization")
   }

   func configure(message: UIKitChatMessage, avatar: UIImage, font: UIFont, attributedText: NSAttributedString?)
   {
      rightToLeft = message.direction == "rtl"
      avatarView.image = avatar
      messageLabel.font = font
      messageLabel.attributedText = attributedText
      if attributedText == nil
      {
         messageLabel.text = message.text
      }
      messageLabel.accessibilityLabel = message.text
      messageLabel.textColor = comparisonText
      messageLabel.textAlignment = rightToLeft ? .right : .left
      messageLabel.semanticContentAttribute = rightToLeft ? .forceRightToLeft : .forceLeftToRight
      setNeedsLayout()
   }

   override func prepareForReuse()
   {
      super.prepareForReuse()
      avatarView.image = nil
      messageLabel.text = nil
      messageLabel.attributedText = nil
      messageLabel.accessibilityLabel = nil
   }

   override func layoutSubviews()
   {
      super.layoutSubviews()
      let avatarX: CGFloat = rightToLeft ? bounds.width - 48 : 12
      let bubbleX: CGFloat = rightToLeft ? 12 : 60
      avatarView.frame = CGRect(x: avatarX, y: 4, width: 36, height: 36)
      bubbleView.frame = CGRect(x: bubbleX, y: 2, width: 286, height: bounds.height - 4)
      bubbleView.layer.shadowPath = UIBezierPath(roundedRect: bubbleView.bounds, cornerRadius: 12).cgPath
      messageLabel.frame = bubbleView.bounds.insetBy(dx: 10, dy: 6)
   }
}

final class UIKitNavigationScene: NSObject, UIKitBenchmarkScene, UITableViewDataSource, UITableViewDelegate
{
   let navigationController: UINavigationController
   var viewController: UIViewController {navigationController}
   private let list = UITableView(frame: .zero, style: .insetGrouped)
   private let listBackdrop = UIView()
   private let listController = UIViewController()
   private var route = "list"
   private var modalVisible = false
   private var cycle = 0
   private let correctnessMode: Bool
   private let latinFont: UIFont
   private let secondaryFont: UIFont

   init(fixture: [String: Any], correctnessMode: Bool, fonts: UIKitPreparedFonts) throws
   {
      guard fixture["list_item_count"] as? Int == 12, fixture["cycle_count"] as? Int == 4 else
      {
         throw UIKitScenarioAdapterFailure.invalidFixture
      }
      self.correctnessMode = correctnessMode
      latinFont = fonts.latin15
      secondaryFont = fonts.latin11
      navigationController = UINavigationController(rootViewController: listController)
      super.init()
      let navigationAppearance = UINavigationBarAppearance()
      navigationAppearance.configureWithOpaqueBackground()
      navigationAppearance.backgroundColor = comparisonBackground
      navigationAppearance.shadowColor = .clear
      navigationAppearance.titleTextAttributes = [
         .font: fonts.latin20,
         .foregroundColor: comparisonText,
      ]
      navigationController.navigationBar.standardAppearance = navigationAppearance
      navigationController.navigationBar.scrollEdgeAppearance = navigationAppearance
      navigationController.navigationBar.compactAppearance = navigationAppearance
      list.dataSource = self
      list.delegate = self
      list.register(UITableViewCell.self, forCellReuseIdentifier: "item")
      list.backgroundColor = .clear
      list.separatorColor = UIColor(white: 235 / 255, alpha: 1)
      list.accessibilityIdentifier = "navigation-list"
      list.frame = UIScreen.main.bounds
      list.autoresizingMask = [.flexibleWidth, .flexibleHeight]
      listBackdrop.backgroundColor = comparisonSurface
      listBackdrop.frame = CGRect(x: 20, y: 99, width: 350, height: UIScreen.main.bounds.height)
      listBackdrop.autoresizingMask = [.flexibleHeight]
      listController.title = "Navigation"
      listController.view.backgroundColor = comparisonBackground
      listController.view.addSubview(listBackdrop)
      listController.view.addSubview(list)
   }

   func apply(event: BenchmarkTraceEvent) throws
   {
      guard event.op == "navigate", let target = event.target else
      {
         if event.op == "pointer-down"
         {
            showInteractiveModal()
            return
         }
         if event.op == "pointer-move" || event.op == "pointer-cancel" {return}
         throw UIKitScenarioAdapterFailure.unsupportedEvent(event.op)
      }
      switch target
      {
      case "navigation:item:05":
         let detail = UIViewController()
         detail.title = "Detail"
         detail.view.backgroundColor = comparisonBackground
         detail.view.accessibilityIdentifier = "detail"
         navigationController.pushViewController(detail, animated: !correctnessMode)
         route = "detail"
      case "navigation:modal":
         let modal = UIViewController()
         modal.view.backgroundColor = comparisonBackground
         modal.view.accessibilityIdentifier = "modal"
         modal.modalPresentationStyle = .formSheet
         navigationController.present(modal, animated: !correctnessMode)
         modalVisible = true
      case "navigation:dismiss-control":
         navigationController.dismiss(animated: !correctnessMode)
         modalVisible = false
      case "navigation:back-control":
         navigationController.popToRootViewController(animated: !correctnessMode)
         route = "list"
         cycle += 1
      case "navigation:cancel":
         navigationController.dismiss(animated: true)
         navigationController.popToRootViewController(animated: false)
         modalVisible = false
         route = "list"
      default: throw UIKitScenarioAdapterFailure.unsupportedEvent(target)
      }
   }

   func reset() throws
   {
      navigationController.dismiss(animated: false)
      navigationController.popToRootViewController(animated: false)
      route = "list"
      modalVisible = false
      cycle = 0
      list.setContentOffset(.zero, animated: false)
      list.reloadData()
      list.layoutIfNeeded()
      navigationController.view.endEditing(true)
      navigationController.view.layer.removeAllAnimations()
   }

   func state(checkpointID: String) -> [String: Any]
   {
      ["route": route, "modal_visible": modalVisible, "completed_cycles": cycle]
   }

   func roleCounts() -> [BenchmarkRoleCount]
   {
      if modalVisible
      {
         return [
            BenchmarkRoleCount(role: "detail", count: 1),
            BenchmarkRoleCount(role: "modal", count: 1),
            BenchmarkRoleCount(role: "dismiss-control", count: 1),
            BenchmarkRoleCount(role: "back-control", count: 1),
         ]
      }
      return [BenchmarkRoleCount(role: "navigation-list", count: 1), BenchmarkRoleCount(role: "list-item", count: 12)]
   }

   func teardown()
   {
      navigationController.view.layer.removeAllAnimations()
      navigationController.dismiss(animated: false)
      navigationController.popToRootViewController(animated: false)
      list.dataSource = nil
      list.delegate = nil
   }

   func tableView(_ tableView: UITableView, numberOfRowsInSection section: Int) -> Int
   {
      12
   }

   func tableView(_ tableView: UITableView, cellForRowAt indexPath: IndexPath) -> UITableViewCell
   {
      let cell = tableView.dequeueReusableCell(withIdentifier: "item", for: indexPath)
      var content = cell.defaultContentConfiguration()
      content.text = "Item \(indexPath.row + 1)"
      content.secondaryText = "Canonical navigation row"
      content.textProperties.font = latinFont
      content.textProperties.color = comparisonText
      content.secondaryTextProperties.font = secondaryFont
      content.secondaryTextProperties.color = comparisonSecondaryText
      cell.contentConfiguration = content
      let disclosure = UILabel()
      disclosure.text = "›"
      disclosure.font = latinFont
      disclosure.textColor = comparisonInactiveControl
      disclosure.sizeToFit()
      cell.accessoryType = .none
      cell.accessoryView = disclosure
      cell.accessibilityIdentifier = "list-item"
      return cell
   }

   private func showInteractiveModal()
   {
      if route == "list"
      {
         let detail = UIViewController()
         detail.title = "Detail"
         detail.view.backgroundColor = comparisonBackground
         detail.view.accessibilityIdentifier = "detail"
         navigationController.pushViewController(detail, animated: false)
         route = "detail"
      }
      guard !modalVisible else {return}
      let modal = UIViewController()
      modal.view.backgroundColor = comparisonBackground
      modal.view.accessibilityIdentifier = "modal"
      modal.modalPresentationStyle = .formSheet
      navigationController.present(modal, animated: false)
      modalVisible = true
   }
}

private final class UIKitImageScene: UIKitBenchmarkScene
{
   private let controller = ImageViewController()
   var viewController: UIViewController {controller}
   private let titleLabel = UILabel()
   private let canvas = UIView()
   private let imageView = UIImageView()
   private let zoomControl = UISlider()
   private let zoomTrack = UIView()
   private let assets: UIKitPreparedAssets
   private var decodedImage: UIImage?
   private var activePointers = [UInt32: CGPoint]()
   private var panOrigin: CGPoint?
   private var panStartTranslation = CGPoint.zero
   private var translation = CGPoint.zero
   private var pinchStartDistance: CGFloat?
   private var pinchStartScale: CGFloat = 1
   private var scale: CGFloat = 1
   private var bytesReady = false
   private var decoded = false
   private var uploaded = false
   private var firstVisible = false

   init(fixtureData: Data, assets: UIKitPreparedAssets, fonts: UIKitPreparedFonts) throws
   {
      let fixture = try uikitFixtureDecoder.decode(UIKitImageFixture.self, from: fixtureData)
      guard fixture.schemaVersion == 1,
            fixture.id == "image.decode-zoom",
            fixture.source.width == 4_096,
            fixture.source.height == 3_072,
            fixture.thumbnail.width == 384,
            fixture.thumbnail.height == 288,
            fixture.panDistanceMillionths == 450_000,
            fixture.pinchScaleMillionths == 2_000_000,
            let thumbnail = assets.thumbnail else
      {
         throw UIKitScenarioAdapterFailure.invalidFixture
      }
      self.assets = assets
      controller.view.backgroundColor = comparisonBackground
      titleLabel.text = "Decode & Zoom"
      titleLabel.font = fonts.latin20
      titleLabel.textColor = comparisonText
      canvas.backgroundColor = comparisonBackground
      canvas.clipsToBounds = true
      canvas.accessibilityIdentifier = "image-canvas"
      imageView.image = thumbnail
      imageView.contentMode = .scaleAspectFit
      imageView.accessibilityIdentifier = "image"
      zoomControl.minimumValue = 1
      zoomControl.maximumValue = 3
      zoomControl.value = 1
      let disabledTrack = UIColor(red: 231 / 255, green: 233 / 255, blue: 234 / 255, alpha: 1)
      zoomControl.minimumTrackTintColor = disabledTrack
      zoomControl.maximumTrackTintColor = disabledTrack
      zoomControl.isUserInteractionEnabled = false
      zoomControl.accessibilityIdentifier = "zoom-control"
      zoomTrack.backgroundColor = disabledTrack
      zoomTrack.layer.cornerRadius = 0
      zoomTrack.layer.cornerCurve = .circular
      canvas.addSubview(imageView)
      controller.view.addSubview(titleLabel)
      controller.view.addSubview(canvas)
      controller.view.addSubview(zoomControl)
      controller.view.addSubview(zoomTrack)
      controller.layoutHandler = {[weak self] in self?.layout()}
   }

   func apply(event: BenchmarkTraceEvent) throws
   {
      if event.op == "resource-arrival"
      {
         try applyResourceArrival(event)
         return
      }
      guard event.op == "pointer-down" || event.op == "pointer-move" || event.op == "pointer-up" || event.op == "pointer-cancel",
            let pointer = event.pointer,
            let x = event.xMillionths,
            let y = event.yMillionths else
      {
         throw UIKitScenarioAdapterFailure.unsupportedEvent(event.op)
      }
      let point = CGPoint(x: CGFloat(x) * 390 / 1_000_000, y: CGFloat(y) * 844 / 1_000_000 - 52)
      switch event.op
      {
      case "pointer-down": pointerDown(pointer, at: point)
      case "pointer-move": pointerMove(pointer, to: point)
      case "pointer-up":
         pointerMove(pointer, to: point)
         pointerEnded(pointer)
      case "pointer-cancel": pointerEnded(pointer)
      default: break
      }
   }

   func reset() throws
   {
      decodedImage = nil
      activePointers.removeAll(keepingCapacity: true)
      panOrigin = nil
      panStartTranslation = .zero
      translation = .zero
      pinchStartDistance = nil
      pinchStartScale = 1
      scale = 1
      bytesReady = false
      decoded = false
      uploaded = false
      firstVisible = false
      imageView.image = assets.thumbnail
      imageView.transform = .identity
      zoomControl.value = 1
      controller.view.endEditing(true)
      controller.view.layer.removeAllAnimations()
      layout()
   }

   func state(checkpointID: String) -> [String: Any]
   {
      let stage: String
      if firstVisible {stage = "visible"}
      else if uploaded {stage = "uploaded"}
      else if decoded {stage = "decoded"}
      else if bytesReady {stage = "bytes-ready"}
      else {stage = "thumbnail"}
      return [
         "resource_stage": stage,
         "pan_x_millionths": Int((translation.x / 390 * 1_000_000).rounded()),
         "pan_y_millionths": Int((translation.y / 844 * 1_000_000).rounded()),
         "scale_millionths": Int((scale * 1_000_000).rounded()),
         "active_pointer_count": activePointers.count,
      ]
   }

   func roleCounts() -> [BenchmarkRoleCount]
   {
      [
         BenchmarkRoleCount(role: "image-canvas", count: 1),
         BenchmarkRoleCount(role: "image", count: 1),
         BenchmarkRoleCount(role: "zoom-control", count: 1),
      ]
   }

   func teardown()
   {
      controller.view.layer.removeAllAnimations()
      activePointers.removeAll(keepingCapacity: true)
      imageView.image = nil
      imageView.layer.contents = nil
      decodedImage = nil
      assets.releaseDecodedSource()
   }

   private func applyResourceArrival(_ event: BenchmarkTraceEvent) throws
   {
      switch event.target
      {
      case "image:source-bytes": bytesReady = true
      case "image:decoded":
         decodedImage = try assets.decodedSource()
         decoded = true
      case "image:texture":
         guard decodedImage != nil else
         {
            throw UIKitScenarioAdapterFailure.unsupportedEvent(event.target ?? event.op)
         }
         uploaded = true
      case "image:presented":
         guard let decodedImage, uploaded else
         {
            throw UIKitScenarioAdapterFailure.unsupportedEvent(event.target ?? event.op)
         }
         imageView.image = decodedImage
         firstVisible = true
         layout()
      default: throw UIKitScenarioAdapterFailure.unsupportedEvent(event.target ?? event.op)
      }
   }

   private func pointerDown(_ pointer: UInt32, at point: CGPoint)
   {
      activePointers[pointer] = point
      if activePointers.count == 1
      {
         panOrigin = point
         panStartTranslation = translation
      }
      else if activePointers.count == 2
      {
         pinchStartDistance = pointerDistance()
         pinchStartScale = scale
      }
   }

   private func pointerMove(_ pointer: UInt32, to point: CGPoint)
   {
      guard activePointers[pointer] != nil else {return}
      activePointers[pointer] = point
      if activePointers.count >= 2, let start = pinchStartDistance, start > 0, let distance = pointerDistance()
      {
         scale = max(1, min(3, pinchStartScale * distance / start))
      }
      else if let panOrigin
      {
         translation = CGPoint(
            x: panStartTranslation.x + point.x - panOrigin.x,
            y: panStartTranslation.y + point.y - panOrigin.y
         )
      }
      applyTransform()
   }

   private func pointerEnded(_ pointer: UInt32)
   {
      activePointers.removeValue(forKey: pointer)
      pinchStartDistance = nil
      if let remaining = activePointers.values.first
      {
         panOrigin = remaining
         panStartTranslation = translation
      }
      else
      {
         panOrigin = nil
      }
   }

   private func pointerDistance() -> CGFloat?
   {
      let points = Array(activePointers.values.prefix(2))
      guard points.count == 2 else {return nil}
      return hypot(points[1].x - points[0].x, points[1].y - points[0].y)
   }

   private func applyTransform()
   {
      imageView.transform = CGAffineTransform(translationX: translation.x, y: translation.y).scaledBy(x: scale, y: scale)
      zoomControl.value = Float(scale)
   }

   private func layout()
   {
      titleLabel.frame = CGRect(x: 18, y: 8, width: 204, height: 36)
      canvas.frame = CGRect(x: 0, y: 52, width: 390, height: 740)
      zoomControl.frame = CGRect(x: 16, y: 800, width: 358, height: 28)
      zoomTrack.frame = CGRect(x: 16, y: 811, width: 358, height: 6)
      imageView.frame = firstVisible ? canvas.bounds : CGRect(x: 16, y: 16, width: 128, height: 96)
      applyTransform()
   }
}

private final class ImageViewController: UIViewController
{
   var layoutHandler: (() -> Void)?

   override func viewDidLayoutSubviews()
   {
      super.viewDidLayoutSubviews()
      layoutHandler?()
   }
}
