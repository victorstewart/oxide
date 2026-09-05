import AppKit
import Foundation
import ImageIO

enum AppKitProductionScenarioFailure: Error
{
   case activeAnimations
   case invalidFixture
   case missingScenario
   case retiredResourcesUnreleased([String])
   case unsupportedEvent(String)
   case unsupportedScenario(String)
}

private struct AppKitProductionChatMessage
{
   let id: String
   let sequence: Int
   let authorIndex: Int
   let avatarIndex: Int
   let direction: String
   var text: String
}

private struct AppKitProductionChatSelection
{
   let messageID: String
   let startUTF8: Int
   let endUTF8: Int
   let replacement: String
}

final class AppKitProductionReferenceAssets
{
   let atlas: NSImage?
   let atlasTiles: [NSImage]
   let thumbnailVariants: [NSImage]
   let inlineText: AppKitProductionInlineText?
   let sourceData: Data?
   let thumbnail: NSImage?
   private var retainedSource: NSImage?
   private var retainedSourceCache: CGImageSource?

   var hasRetainedSource: Bool
   {
      retainedSource != nil
   }

   var hasRetainedSourceCache: Bool
   {
      retainedSourceCache != nil
   }

   init(scenario: BenchmarkScenario, fixture: [String: Any], loader: BenchmarkSpecLoader) throws
   {
      let manifest = try loader.loadAssetManifest(scenario.assets)
      if let contract = manifest.inlineTextAtlas
      {
         var rasterImages = [UInt32: CGImage]()
         for variant in contract.variants
         {
            guard let artifact = manifest.artifacts.first(where: {$0.role == variant.artifactRole}),
                  let image = NSImage(data: try loader.read(artifact.artifact)),
                  let representation = image.representations.first,
                  representation.pixelsWide == Int(variant.pixelWidth),
                  representation.pixelsHigh == Int(variant.pixelHeight),
                  let cgImage = image.cgImage(forProposedRect: nil, context: nil, hints: nil) else
            {
               throw AppKitProductionScenarioFailure.invalidFixture
            }
            rasterImages[variant.emPixels] = cgImage
         }
         inlineText = try AppKitProductionInlineText(contract: contract, rasterImages: rasterImages)
      }
      else
      {
         inlineText = nil
      }
      if scenario.id == AppKitProductionSceneID.imageDecodeZoom.rawValue
      {
         guard let sourceFixture = fixture["source"] as? [String: Any],
               let thumbnailFixture = fixture["thumbnail"] as? [String: Any],
               sourceFixture["width"] as? Int == 4_096,
               sourceFixture["height"] as? Int == 3_072,
               sourceFixture["format"] as? String == "png",
               sourceFixture["color_space"] as? String == "srgb",
               thumbnailFixture["width"] as? Int == 384,
               thumbnailFixture["height"] as? Int == 288,
               thumbnailFixture["format"] as? String == "png",
               thumbnailFixture["color_space"] as? String == "srgb",
               let source = manifest.artifacts.first(where: {$0.role == "source-image"}),
               let thumbnail = manifest.artifacts.first(where: {$0.role == "thumbnail"}),
               source.artifact.path == sourceFixture["artifact"].flatMap({($0 as? [String: Any])?["path"] as? String}),
               thumbnail.artifact.path == thumbnailFixture["artifact"].flatMap({($0 as? [String: Any])?["path"] as? String}),
               let image = NSImage(data: try loader.read(thumbnail.artifact)) else
         {
            throw AppKitProductionScenarioFailure.invalidFixture
         }
         atlas = nil
         atlasTiles = []
         thumbnailVariants = []
         sourceData = try loader.read(source.artifact)
         self.thumbnail = image
         return
      }

      guard let artifact = manifest.artifacts.first(where: {$0.role == "thumbnail-atlas"}),
            let image = NSImage(data: try loader.read(artifact.artifact)),
            let representation = image.representations.first,
            representation.pixelsWide == 384,
            representation.pixelsHigh == 192,
            let cgImage = image.cgImage(forProposedRect: nil, context: nil, hints: nil) else
      {
         throw AppKitProductionScenarioFailure.invalidFixture
      }
      atlas = image
      let atlasTiles = try (0..<128).map
      {
         index -> NSImage in
         let source = CGRect(x: (index % 16) * 24, y: (index / 16) * 24, width: 24, height: 24)
         guard let tile = cgImage.cropping(to: source) else
         {
            throw AppKitProductionScenarioFailure.invalidFixture
         }
         return NSImage(cgImage: tile, size: NSSize(width: 24, height: 24))
      }
      self.atlasTiles = atlasTiles
      thumbnailVariants = scenario.id == AppKitProductionSceneID.gridLargeScroll.rawValue
         ? atlasTiles + atlasTiles.compactMap(Self.accentedThumbnail)
         : atlasTiles
      guard scenario.id != AppKitProductionSceneID.gridLargeScroll.rawValue || thumbnailVariants.count == 256 else
      {
         throw AppKitProductionScenarioFailure.invalidFixture
      }
      sourceData = nil
      thumbnail = nil
   }

   private static func accentedThumbnail(_ image: NSImage) -> NSImage?
   {
      guard let source = image.cgImage(forProposedRect: nil, context: nil, hints: nil),
            let colorSpace = CGColorSpace(name: CGColorSpace.sRGB),
            let context = CGContext(
               data: nil,
               width: source.width,
               height: source.height,
               bitsPerComponent: 8,
               bytesPerRow: source.width * 4,
               space: colorSpace,
               bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
            ) else
      {
         return nil
      }
      context.draw(source, in: CGRect(x: 0, y: 0, width: source.width, height: source.height))
      context.setFillColor(AppKitProductionPalette.accent.withAlphaComponent(0.2).cgColor)
      context.fill(CGRect(x: 0, y: 0, width: source.width, height: source.height))
      guard let result = context.makeImage() else {return nil}
      return NSImage(cgImage: result, size: image.size)
   }

   func decodedSource() throws -> NSImage
   {
      if let retainedSource
      {
         return retainedSource
      }
      guard let sourceData,
            let source = CGImageSourceCreateWithData(sourceData as CFData, nil),
            let decoded = CGImageSourceCreateImageAtIndex(
               source,
               0,
               [
                  kCGImageSourceShouldCache: true,
                  kCGImageSourceShouldCacheImmediately: true,
               ] as CFDictionary
            ),
            decoded.width == 4_096,
            decoded.height == 3_072 else
      {
         throw AppKitProductionScenarioFailure.invalidFixture
      }
      let image = NSImage(cgImage: decoded, size: NSSize(width: 4_096, height: 3_072))
      retainedSourceCache = source
      retainedSource = image
      return image
   }

   func releaseDecodedSource()
   {
      if let retainedSourceCache
      {
         CGImageSourceRemoveCacheAtIndex(retainedSourceCache, 0)
      }
      retainedSource = nil
      retainedSourceCache = nil
   }
}

final class AppKitProductionScenarioModel
{
   private struct NavigationTransition
   {
      let target: CGFloat
      let startedAtUs: UInt64
   }

   let scenario: BenchmarkScenario
   let fixture: [String: Any]
   let assets: AppKitProductionReferenceAssets
   let fonts: AppKitProductionFonts

   private var startupForegroundCount = 0
   private var startupBackgroundCount = 0
   private var startupFreshInstallReady = false
   private var startupLifecycleStateID: String?
   private(set) var startupVisible = true
   private(set) var startupCards = [AppKitStartupCardRecord]()

   private var dashboardLeafUpdates = 0
   private var dashboardBulkUpdates = 0
   private var dashboardLabels = [String]()
   private var dashboardInitialLabels = [String]()

   private(set) var enduranceHeavyScreenVisible = true
   private var enduranceHeavyScreenTransitions = 0
   private var enduranceOpenCloseCycles = 0
   private var enduranceActiveTabIndex = 0
   private var enduranceTabSwitches = 0
   private var enduranceAnimationFrameIndex = 0
   private var enduranceAnimationFrames = 0

   private var feedRows = [[String: Any]]()
   private var feedInitialRows = [[String: Any]]()
   private var feedPrependRows = [[String: Any]]()
   private var feedRowByID = [String: [String: Any]]()
   private(set) var feedScrollMillionths: UInt32 = 0
   private(set) var feedScrollOffset: CGFloat = 0
   private var feedFavoriteID: String?
   private var feedPrependCount = 0

   private var chatInitialMessages = [AppKitProductionChatMessage]()
   private var chatMessages = [AppKitProductionChatMessage]()
   private var chatPrependMessages = [AppKitProductionChatMessage]()
   private var chatAppendTemplates = [AppKitProductionChatMessage]()
   private var chatIndexByID = [String: Int]()
   private var chatSelection: AppKitProductionChatSelection?
   private var chatSelectedMessageID: String?
   private var chatSelectedUTF8Range: Range<Int>?
   private(set) var chatComposer = ""
   private var chatPrependCount = 0
   private var chatAppendCount = 0
   private var chatSendCount = 0
   private var chatReplacementApplied = false

   private(set) var navigationRoute = "list"
   private(set) var navigationModalProgress: CGFloat = 0
   private var navigationTransition: NavigationTransition?
   private var navigationCycle = 0
   private var virtualTimeUs = UInt64(0)

   private(set) var imageDecoded: NSImage?
   private var imageBytesReady = false
   private var imageDidDecode = false
   private var imageUploaded = false
   private(set) var imageFirstVisible = false
   private var imagePointers = [UInt32: CGPoint]()
   private var imagePanOrigin: CGPoint?
   private var imagePanStart = CGPoint.zero
   private(set) var imageTranslation = CGPoint.zero
   private var imagePinchStartDistance: CGFloat?
   private var imagePinchStartScale: CGFloat = 1
   private(set) var imageScale: CGFloat = 1

   private var gridScrollMillionths = 0
   private var gridDetailVisible = false
   private var gridAllRecords = [AppKitGridRecord]()
   private var effectsAnimationProgress = 0
   private var effectsDirtyGeneration = 0
   private var mutationClass: String?
   private var mutationGeneration = 0
   private(set) var mutationChanged = IndexSet()
   private var textFontAtlasCold = true
   private var textReplayGeneration = 0
   private var textScaleMillionths = 1_000_000
   private var textWrapRotation = 0
   private var textAllRecords = [AppKitTextRecord]()
   private var resizeChangeIndex = 0
   private(set) var resizeOrientation = "portrait"
   private(set) var resizeTheme = "light"
   private(set) var resizeWidth = 390
   private(set) var resizeHeight = 844

   init(scenario: BenchmarkScenario, fixtureData: Data, loader: BenchmarkSpecLoader) throws
   {
      guard let fixture = try JSONSerialization.jsonObject(with: fixtureData) as? [String: Any],
            fixture["schema_version"] as? Int == 1,
            fixture["id"] as? String == Self.fixtureID(for: scenario.id) else
      {
         throw AppKitProductionScenarioFailure.invalidFixture
      }
      self.scenario = scenario
      self.fixture = fixture
      fonts = try AppKitProductionFonts(catalog: loader.loadRegisteredFontCatalog(scenario.fontPack))
      assets = try AppKitProductionReferenceAssets(scenario: scenario, fixture: fixture, loader: loader)
      try validateAndPrepareFixture()
      reset()
   }

   var state: [String: Any]
   {
      switch scenario.id
      {
      case AppKitProductionSceneID.startupFirstScreen.rawValue:
         return [
            "foreground_count": startupForegroundCount,
            "background_count": startupBackgroundCount,
            "fresh_install_ready": startupFreshInstallReady,
            "scene_visible": startupVisible,
            "lifecycle_state_id": startupLifecycleStateID ?? NSNull(),
            "card_count": fixture["card_count"] as? Int ?? 0,
         ]
      case AppKitProductionSceneID.dashboardMixedStatic.rawValue, "idle.steady":
         return [
            "leaf_update_count": dashboardLeafUpdates,
            "bulk_update_count": dashboardBulkUpdates,
            "visible_node_count": 301,
         ]
      case AppKitProductionSceneID.enduranceChurn.rawValue:
         return [
            "open_close_cycle_count": enduranceOpenCloseCycles,
            "heavy_screen_transition_count": enduranceHeavyScreenTransitions,
            "heavy_screen_visible": enduranceHeavyScreenVisible,
            "tab_switch_count": enduranceTabSwitches,
            "active_tab_index": enduranceActiveTabIndex,
            "animation_frame_count": enduranceAnimationFrames,
            "animation_frame_index": enduranceAnimationFrameIndex,
            "visible_node_count": enduranceHeavyScreenVisible ? 301 : 1,
         ]
      case AppKitProductionSceneID.feedVariableScroll.rawValue:
         return [
            "row_count": feedRows.count,
            "scroll_position_millionths": feedScrollMillionths,
            "favorite_id": feedFavoriteID ?? NSNull(),
            "prepend_count": feedPrependCount,
         ]
      case AppKitProductionSceneID.chatLiveUpdate.rawValue:
         return [
            "message_count": chatMessages.count,
            "prepend_count": chatPrependCount,
            "append_count": chatAppendCount,
            "composer_utf8_count": chatComposer.utf8.count,
            "focused_message_id": chatSelectedMessageID ?? NSNull(),
            "selection_active": chatSelectedUTF8Range != nil,
            "replacement_applied": chatReplacementApplied,
         ]
      case AppKitProductionSceneID.navigationModal.rawValue:
         return [
            "route": navigationRoute,
            "modal_visible": navigationModalVisible,
            "completed_cycles": navigationCycle,
         ]
      case AppKitProductionSceneID.imageDecodeZoom.rawValue:
         return [
            "resource_stage": imageResourceStage,
            "pan_x_millionths": Int((imageTranslation.x / 390 * 1_000_000).rounded()),
            "pan_y_millionths": Int((imageTranslation.y / 844 * 1_000_000).rounded()),
            "scale_millionths": Int((imageScale * 1_000_000).rounded()),
            "active_pointer_count": imagePointers.count,
         ]
      case AppKitProductionSceneID.gridLargeScroll.rawValue:
         return [
            "detail_tile_id": gridDetailVisible ? String(format: "grid:tile:%05d", fixture["detail_tile_index"] as? Int ?? 7_500) : NSNull(),
            "route": gridDetailVisible ? "detail" : "grid",
            "scroll_position_millionths": gridScrollMillionths,
            "thumbnail_count": fixture["thumbnail_count"] as? Int ?? 0,
            "tile_count": fixture["tile_count"] as? Int ?? 0,
         ]
      case AppKitProductionSceneID.effectsLayers.rawValue:
         return [
            "animation_progress_millionths": effectsAnimationProgress,
            "backdrop_blur_count": fixture["backdrop_blur_count"] as? Int ?? 0,
            "card_count": fixture["card_count"] as? Int ?? 0,
            "dirty_layer_generation": effectsDirtyGeneration,
            "shadow_count": fixture["shadow_count"] as? Int ?? 0,
         ]
      case AppKitProductionSceneID.mutationDamage.rawValue:
         return [
            "changed_node_count": mutationChanged.count,
            "generation": mutationGeneration,
            "mutation_class": mutationClass ?? NSNull(),
            "node_count": fixture["node_count"] as? Int ?? 0,
         ]
      case AppKitProductionSceneID.textMultilingual.rawValue:
         return [
            "font_atlas_cold": textFontAtlasCold,
            "label_count": fixture["label_count"] as? Int ?? 0,
            "replay_generation": textReplayGeneration,
            "scale_millionths": textScaleMillionths,
            "wrap_rotation": textWrapRotation,
         ]
      case AppKitProductionSceneID.resizeTheme.rawValue:
         return [
            "change_index": resizeChangeIndex,
            "height": resizeHeight,
            "orientation": resizeOrientation,
            "theme": resizeTheme,
            "visible_node_count": 300,
            "width": resizeWidth,
         ]
      default:
         return [:]
      }
   }

   var roleCounts: [BenchmarkRoleCount]
   {
      switch scenario.id
      {
      case AppKitProductionSceneID.startupFirstScreen.rawValue:
         return roles([("header", 1), ("navigation", 1), ("card", 6), ("initial-image", 6), ("primary-control", 1)])
      case AppKitProductionSceneID.dashboardMixedStatic.rawValue, "idle.steady":
         return roles([("dashboard", 1), ("label", 176), ("icon-image", 64), ("rounded-card", 32), ("control", 24), ("backdrop-region", 4)])
      case AppKitProductionSceneID.enduranceChurn.rawValue:
         return roles([
            ("endurance", 1),
            ("label", enduranceHeavyScreenVisible ? 176 : 0),
            ("icon-image", enduranceHeavyScreenVisible ? 64 : 0),
            ("rounded-card", enduranceHeavyScreenVisible ? 32 : 0),
            ("control", enduranceHeavyScreenVisible ? 24 : 0),
            ("backdrop-region", enduranceHeavyScreenVisible ? 4 : 0),
         ])
      case AppKitProductionSceneID.feedVariableScroll.rawValue:
         let visible = visibleFeedRowCount
         return roles([("navigation-bar", 1), ("feed", 1), ("feed-card", visible), ("thumbnail", visible), ("favorite-control", visible)])
      case AppKitProductionSceneID.chatLiveUpdate.rawValue:
         return roles([("chat-thread", 1), ("message", 10), ("avatar", 10), ("composer", 1), ("send-control", 1)])
      case AppKitProductionSceneID.navigationModal.rawValue:
         if navigationModalVisible
         {
            return roles([("detail", 1), ("modal", 1), ("dismiss-control", 1), ("back-control", 1)])
         }
         return roles([("navigation-list", 1), ("list-item", 12)])
      case AppKitProductionSceneID.imageDecodeZoom.rawValue:
         return roles([("image-canvas", 1), ("image", 1), ("zoom-control", 1)])
      case AppKitProductionSceneID.gridLargeScroll.rawValue:
         return gridDetailVisible
            ? roles([("detail-view", 1), ("detail-thumbnail", 1), ("detail-title", 1), ("back-control", 1)])
            : roles([("thumbnail-grid", 1), ("grid-tile", 18), ("thumbnail", 18), ("tile-label", 18)])
      case AppKitProductionSceneID.effectsLayers.rawValue:
         return roles([("effects-scene", 1), ("rounded-card", 100), ("clip-region", 100), ("shadow", 32), ("backdrop-region", 8)])
      case AppKitProductionSceneID.mutationDamage.rawValue:
         return roles([("mutation-surface", 1), ("simple-node", 10_000)])
      case AppKitProductionSceneID.textMultilingual.rawValue:
         return roles([("text-surface", 1), ("multilingual-label", 12)])
      case AppKitProductionSceneID.resizeTheme.rawValue:
         return roles([("dashboard", 1), ("label", 176), ("icon-image", 64), ("rounded-card", 32), ("control", 24), ("backdrop-region", 4)])
      default:
         return []
      }
   }

   var dashboardRecords: [AppKitDashboardCardRecord]
   {
      makeDashboardRecords(prefix: "dashboard")
   }

   var enduranceRecords: [AppKitDashboardCardRecord]
   {
      makeDashboardRecords(prefix: "endurance")
   }

   private func makeDashboardRecords(prefix: String) -> [AppKitDashboardCardRecord]
   {
      (0..<32).map
      {
         cardIndex in
         let labelCount = cardIndex < 16 ? 6 : 5
         let labelBase = cardIndex < 16 ? cardIndex * 6 : 96 + (cardIndex - 16) * 5
         let labels = (0..<labelCount).map
         {
            labelIndex in
            let index = labelBase + labelIndex
            return AppKitDashboardLabelRecord(
               id: String(format: "\(prefix):label:%03d", index),
               value: dashboardLabels[index],
               accent: dashboardBulkUpdates > 0 && index < dashboardBulkUpdates
            )
         }
         return AppKitDashboardCardRecord(
            id: String(format: "\(prefix):card:%02d", cardIndex),
            labels: labels,
            leadingIcon: assets.atlasTiles[cardIndex * 2],
            trailingIcon: assets.atlasTiles[cardIndex * 2 + 1],
            font: fonts.latin7,
            actionTarget: cardIndex < 24 ? labels[0].id : nil
         )
      }
   }

   var feedRecords: [AppKitFeedRowRecord]
   {
      feedRows.enumerated().map
      {
         makeFeedRecord(row: $0.element, index: $0.offset)
      }
   }

   func feedRecord(id: String) -> AppKitFeedRowRecord?
   {
      guard let row = feedRowByID[id] else {return nil}
      return makeFeedRecord(row: row, index: row["thumbnail_index"] as? Int ?? 0)
   }

   var feedPrependedRecords: [AppKitFeedRowRecord]
   {
      feedRows.prefix(feedPrependCount).enumerated().map
      {
         makeFeedRecord(row: $0.element, index: $0.offset)
      }
   }

   var chatRecords: [AppKitChatMessageRecord]
   {
      chatMessages.map(makeChatRecord)
   }

   var chatPrependedRecords: [AppKitChatMessageRecord]
   {
      chatMessages.prefix(chatPrependCount).map(makeChatRecord)
   }

   var chatAppendedRecord: AppKitChatMessageRecord?
   {
      guard chatAppendCount > 0, let message = chatMessages.last else {return nil}
      return makeChatRecord(message)
   }

   func chatRecord(id: String) -> AppKitChatMessageRecord?
   {
      guard let index = chatIndexByID[id], index >= 0, index < chatMessages.count else {return nil}
      return makeChatRecord(chatMessages[index])
   }

   private func makeFeedRecord(row: [String: Any], index: Int) -> AppKitFeedRowRecord
   {
      let id = row["id"] as? String ?? ""
      let title = row["text"] as? String ?? ""
      let thumbnailIndex = row["thumbnail_index"] as? Int ?? index
      let titleFont: NSFont
      if title.unicodeScalars.contains(where: {(0x4E00...0x9FFF).contains(Int($0.value))})
      {
         titleFont = fonts.cjk15
      }
      else if title.unicodeScalars.contains(where: {(0x0600...0x06FF).contains(Int($0.value))})
      {
         titleFont = fonts.arabic15
      }
      else
      {
         titleFont = fonts.latin15
      }
      return AppKitFeedRowRecord(
         id: id,
         title: title,
         height: CGFloat(row["height"] as? Int ?? 76),
         thumbnail: assets.atlasTiles[thumbnailIndex % assets.atlasTiles.count],
         thumbnailAtlas: assets.atlas!,
         thumbnailIndex: thumbnailIndex % assets.atlasTiles.count,
         titleFont: titleFont,
         secondaryFont: fonts.latin11,
         inlineText: assets.inlineText,
         favorite: id == feedFavoriteID || row["favorite"] as? Bool == true
      )
   }

   private func makeChatRecord(_ message: AppKitProductionChatMessage) -> AppKitChatMessageRecord
   {
      let textFont: NSFont
      if message.text.unicodeScalars.contains(where: {(0x4E00...0x9FFF).contains(Int($0.value))})
      {
         textFont = fonts.cjk15
      }
      else if message.text.unicodeScalars.contains(where: {(0x0600...0x06FF).contains(Int($0.value))})
      {
         textFont = fonts.arabic15
      }
      else
      {
         textFont = fonts.latin15
      }
      return AppKitChatMessageRecord(
         id: message.id,
         sequence: message.sequence,
         direction: message.direction,
         text: message.text,
         avatar: assets.atlasTiles[message.avatarIndex % assets.atlasTiles.count],
         avatarAtlas: assets.atlas!,
         avatarIndex: message.avatarIndex % assets.atlasTiles.count,
         textFont: textFont,
         inlineText: assets.inlineText
      )
   }

   var navigationRecords: [AppKitNavigationDestinationRecord]
   {
      (0..<12).map
      {
         AppKitNavigationDestinationRecord(
            id: String(format: "navigation:item:%02d", $0),
            title: "Item \($0 + 1)",
            subtitle: "Canonical navigation row",
            titleFont: fonts.latin15,
            subtitleFont: fonts.latin11
         )
      }
   }

   var gridRecords: [AppKitGridRecord]
   {
      gridAllRecords
   }

   var gridDetailRecord: AppKitGridRecord?
   {
      guard gridDetailVisible else {return nil}
      let index = fixture["detail_tile_index"] as? Int ?? 7_500
      return AppKitGridRecord(
         index: index,
         image: assets.thumbnailVariants[index % 256],
         atlas: assets.atlas!,
         title: String(format: "Tile %05d", index),
         titleFont: fonts.latin11
      )
   }

   var textRecords: [AppKitTextRecord]
   {
      textAllRecords
   }

   var gridScrollPositionMillionths: Int {gridScrollMillionths}
   var mutationGenerationValue: Int {mutationGeneration}
   var textScale: CGFloat {CGFloat(textScaleMillionths) / 1_000_000}

   var textWrapWidth: CGFloat
   {
      guard let widths = fixture["wrap_width_rotation"] as? [Int], !widths.isEmpty else {return 96}
      return CGFloat(widths[textWrapRotation % widths.count])
   }

   func reset()
   {
      startupForegroundCount = 0
      startupBackgroundCount = 0
      startupFreshInstallReady = false
      startupLifecycleStateID = nil
      startupVisible = true
      dashboardLeafUpdates = 0
      dashboardBulkUpdates = 0
      dashboardLabels = dashboardInitialLabels
      enduranceHeavyScreenVisible = true
      enduranceHeavyScreenTransitions = 0
      enduranceOpenCloseCycles = 0
      enduranceActiveTabIndex = 0
      enduranceTabSwitches = 0
      enduranceAnimationFrameIndex = 0
      enduranceAnimationFrames = 0
      feedRows = feedInitialRows
      feedScrollMillionths = 0
      feedScrollOffset = 0
      feedFavoriteID = nil
      feedPrependCount = 0
      chatMessages = chatInitialMessages
      chatIndexByID = Dictionary(uniqueKeysWithValues: chatMessages.enumerated().map {($0.element.id, $0.offset)})
      chatSelectedMessageID = nil
      chatSelectedUTF8Range = nil
      chatComposer = ""
      chatPrependCount = 0
      chatAppendCount = 0
      chatSendCount = 0
      chatReplacementApplied = false
      navigationRoute = "list"
      navigationModalProgress = 0
      navigationTransition = nil
      navigationCycle = 0
      virtualTimeUs = 0
      imageDecoded = nil
      imageBytesReady = false
      imageDidDecode = false
      imageUploaded = false
      imageFirstVisible = false
      imagePointers.removeAll(keepingCapacity: true)
      imagePanOrigin = nil
      imagePanStart = .zero
      imageTranslation = .zero
      imagePinchStartDistance = nil
      imagePinchStartScale = 1
      imageScale = 1
      gridScrollMillionths = 0
      gridDetailVisible = false
      effectsAnimationProgress = 0
      effectsDirtyGeneration = 0
      mutationClass = nil
      mutationGeneration = 0
      mutationChanged.removeAll()
      textFontAtlasCold = true
      textReplayGeneration = 0
      textScaleMillionths = 1_000_000
      textWrapRotation = 0
      resizeChangeIndex = 0
      resizeOrientation = fixture["initial_orientation"] as? String ?? "portrait"
      resizeTheme = fixture["initial_theme"] as? String ?? "light"
      let viewport = fixture["initial_viewport"] as? [String: Any]
      resizeWidth = viewport?["width"] as? Int ?? 390
      resizeHeight = viewport?["height"] as? Int ?? 844
   }

   func teardown()
   {
      imageDecoded = nil
      assets.releaseDecodedSource()
   }

   func apply(event: BenchmarkTraceEvent) throws
   {
      switch scenario.id
      {
      case AppKitProductionSceneID.startupFirstScreen.rawValue: try applyStartup(event)
      case AppKitProductionSceneID.dashboardMixedStatic.rawValue: try applyDashboard(event)
      case AppKitProductionSceneID.enduranceChurn.rawValue: try applyEndurance(event)
      case "idle.steady": throw AppKitProductionScenarioFailure.unsupportedEvent(event.op)
      case AppKitProductionSceneID.feedVariableScroll.rawValue: try applyFeed(event)
      case AppKitProductionSceneID.chatLiveUpdate.rawValue: try applyChat(event)
      case AppKitProductionSceneID.navigationModal.rawValue: try applyNavigation(event)
      case AppKitProductionSceneID.imageDecodeZoom.rawValue: try applyImage(event)
      case AppKitProductionSceneID.gridLargeScroll.rawValue: try applyGrid(event)
      case AppKitProductionSceneID.effectsLayers.rawValue: try applyEffects(event)
      case AppKitProductionSceneID.mutationDamage.rawValue: try applyMutation(event)
      case AppKitProductionSceneID.textMultilingual.rawValue: try applyText(event)
      case AppKitProductionSceneID.resizeTheme.rawValue: try applyResize(event)
      default: throw AppKitProductionScenarioFailure.unsupportedScenario(scenario.id)
      }
   }

   func setVirtualTimeUs(_ timeUs: UInt64)
   {
      virtualTimeUs = timeUs
      guard let transition = navigationTransition else {return}
      let durationMs = (fixture["transition"] as? [String: Any])?["duration_ms"] as? Int ?? 300
      let elapsedUs = timeUs >= transition.startedAtUs ? timeUs - transition.startedAtUs : 0
      let linear = min(max(CGFloat(elapsedUs) / CGFloat(max(durationMs, 1) * 1_000), 0), 1)
      let eased = linear * linear * (3 - 2 * linear)
      navigationModalProgress = transition.target > 0 ? eased : 1 - eased
      if linear >= 1
      {
         navigationModalProgress = transition.target
         navigationTransition = nil
      }
   }

   func favoriteFeedItem(_ id: String)
   {
      feedFavoriteID = id
   }

   @discardableResult
   func setFeedScrollOffset(_ offset: CGFloat) -> Bool
   {
      let maximum = max(0, feedContentHeight - 792)
      let clamped = min(max(offset, 0), maximum)
      let millionths = maximum > 0 ? UInt32((clamped / maximum * 1_000_000).rounded()) : 0
      guard millionths != feedScrollMillionths || clamped != feedScrollOffset else {return false}
      feedScrollMillionths = millionths
      feedScrollOffset = clamped
      return true
   }

   func appendChatComposer(_ value: String)
   {
      chatComposer.append(value)
   }

   @discardableResult
   func selectChatMessage(_ id: String, range: NSRange) -> Bool
   {
      guard let selection = chatSelection,
            id == selection.messageID,
            range.location == selection.startUTF8,
            range.length == selection.endUTF8 - selection.startUTF8,
            chatSelectedMessageID != id || chatSelectedUTF8Range != selection.startUTF8..<selection.endUTF8 else {return false}
      chatSelectedMessageID = id
      chatSelectedUTF8Range = selection.startUTF8..<selection.endUTF8
      return true
   }

   @discardableResult
   func replaceSelectedChatMessage(_ id: String, value: String) -> Bool
   {
      guard let selection = chatSelection,
            id == selection.messageID,
            chatSelectedMessageID == id,
            chatSelectedUTF8Range == selection.startUTF8..<selection.endUTF8,
            let index = chatIndexByID[id],
            let expected = replacingUTF8(
               in: chatMessages[index].text,
               range: selection.startUTF8..<selection.endUTF8,
               with: selection.replacement
            ),
            value == expected else {return false}
      chatMessages[index].text = value
      chatSelectedUTF8Range = nil
      chatReplacementApplied = true
      return true
   }

   func sendChatComposer() -> AppKitChatMessageRecord?
   {
      guard !chatComposer.isEmpty else {return nil}
      let message = AppKitProductionChatMessage(
         id: String(format: "chat:sent:%02d", chatSendCount),
         sequence: 6_000 + chatSendCount,
         authorIndex: 0,
         avatarIndex: 0,
         direction: "ltr",
         text: chatComposer
      )
      chatIndexByID[message.id] = chatMessages.count
      chatMessages.append(message)
      chatComposer = ""
      chatSendCount += 1
      return makeChatRecord(message)
   }

   func selectNavigationDestination(_ id: String)
   {
      if id == "navigation:item:05"
      {
         navigationRoute = "detail"
      }
   }

   func presentNavigationModal()
   {
      navigationTransition = NavigationTransition(target: 1, startedAtUs: virtualTimeUs)
   }

   func dismissNavigationModal()
   {
      navigationTransition = NavigationTransition(target: 0, startedAtUs: virtualTimeUs)
   }

   func updateNavigationInteractiveCancel(progress: CGFloat)
   {
      navigationRoute = "detail"
      navigationModalProgress = max(0, min(1, progress))
      navigationTransition = nil
   }

   func finishNavigationInteractiveCancel()
   {
      navigationRoute = "list"
      navigationModalProgress = 0
      navigationTransition = nil
   }

   func returnToNavigationList()
   {
      navigationRoute = "list"
      navigationModalProgress = 0
      navigationTransition = nil
      navigationCycle += 1
   }

   func setImageScale(_ scale: Double)
   {
      imageScale = max(1, min(2, CGFloat(scale)))
   }

   func setGridScrollPositionMillionths(_ value: Int)
   {
      gridScrollMillionths = min(max(value, 0), 1_000_000)
   }

   func showGridDetail(tileIndex: Int)
   {
      guard tileIndex == fixture["detail_tile_index"] as? Int else {return}
      gridDetailVisible = true
   }

   func hideGridDetail()
   {
      gridDetailVisible = false
   }

   private func validateAndPrepareFixture() throws
   {
      switch scenario.id
      {
      case AppKitProductionSceneID.startupFirstScreen.rawValue:
         guard fixture["data_size_bytes"] as? Int == 24 * 1_024,
               let startupData = fixture["data"] as? String,
               startupData.utf8.count == 24 * 1_024,
               fixture["card_count"] as? Int == 24,
               let cards = fixture["cards"] as? [[String: Any]],
               cards.count == 24,
               fixture["initial_image_indices"] as? [Int] == [0, 1, 2, 3, 4, 5],
               fixture["header_id"] as? String == "startup:header",
               fixture["navigation_id"] as? String == "startup:navigation",
               fixture["control_id"] as? String == "startup:primary-control",
               assets.atlasTiles.count == 128 else
         {
            throw AppKitProductionScenarioFailure.invalidFixture
         }
         let startupBytes = Array(startupData.utf8)
         startupCards = try cards.enumerated().compactMap
         {
            index, card -> AppKitStartupCardRecord? in
            guard card["id"] as? String == String(format: "startup:card:%02d", index),
                  card["data_offset"] as? Int == index * 1_024,
                  card["data_length"] as? Int == 1_024,
                  card["thumbnail_index"] as? Int == index % 6,
                  card["initially_visible"] as? Bool == (index < 6) else
            {
               throw AppKitProductionScenarioFailure.invalidFixture
            }
            guard index < 6 else {return nil}
            let detailStart = index * 1_024
            return AppKitStartupCardRecord(
               id: String(format: "startup:card:%02d", index),
               title: String(format: "startup:card:%02d", index),
               detail: String(decoding: startupBytes[detailStart..<detailStart + 48], as: UTF8.self),
               thumbnail: assets.atlasTiles[index],
               thumbnailAtlas: assets.atlas,
               thumbnailIndex: index,
               titleFont: fonts.latin15,
               detailFont: fonts.latin11
            )
         }
      case AppKitProductionSceneID.gridLargeScroll.rawValue:
         guard fixture["tile_count"] as? Int == 10_000,
               fixture["thumbnail_count"] as? Int == 256,
               fixture["detail_tile_index"] as? Int == 7_500,
               assets.thumbnailVariants.count == 256,
               assets.atlas != nil else
         {
            throw AppKitProductionScenarioFailure.invalidFixture
         }
         gridAllRecords = (0..<10_000).map
         {
            index in
            AppKitGridRecord(
               index: index,
               image: assets.thumbnailVariants[index % 256],
               atlas: assets.atlas!,
               title: String(format: "Tile %05d", index),
               titleFont: fonts.latin11
            )
         }
      case AppKitProductionSceneID.effectsLayers.rawValue:
         guard fixture["card_count"] as? Int == 100,
               fixture["clip_count"] as? Int == 100,
               fixture["shadow_count"] as? Int == 32,
               fixture["backdrop_blur_count"] as? Int == 8,
               fixture["dirty_layer_index"] as? Int == 37 else
         {
            throw AppKitProductionScenarioFailure.invalidFixture
         }
      case AppKitProductionSceneID.mutationDamage.rawValue:
         guard fixture["node_count"] as? Int == 10_000,
               fixture["mutation_repetitions"] as? Int == 5,
               let classes = fixture["mutation_classes"] as? [[String: Any]],
               classes.compactMap({$0["changed_node_count"] as? Int}) == [100, 1_000, 10_000],
               let formula = fixture["selection_formula"] as? [String: Any],
               formula["modulus"] as? Int == 10_000 else
         {
            throw AppKitProductionScenarioFailure.invalidFixture
         }
      case AppKitProductionSceneID.textMultilingual.rawValue:
         guard fixture["label_count"] as? Int == 1_000,
               let categories = fixture["categories"] as? [[String: Any]],
               categories.compactMap({$0["count"] as? Int}).reduce(0, +) == 1_000,
               fixture["wrap_width_rotation"] as? [Int] == [96, 144, 192, 240] else
         {
            throw AppKitProductionScenarioFailure.invalidFixture
         }
         textAllRecords.removeAll(keepingCapacity: true)
         textAllRecords.reserveCapacity(1_000)
         for index in 0..<1_000
         {
            let category = categories[index % categories.count]
            let categoryID = category["id"] as? String ?? ""
            let direction = category["direction"] as? String ?? "ltr"
            let value = category["text"] as? String ?? ""
            textAllRecords.append(AppKitTextRecord(
               id: String(format: "text:label:%04d", index),
               categoryID: categoryID,
               value: value,
               direction: direction,
               latinFont: fonts.latin15,
               arabicFont: fonts.arabic15,
               cjkFont: fonts.cjk15,
               inlineText: assets.inlineText
            ))
         }
      case AppKitProductionSceneID.resizeTheme.rawValue:
         guard fixture["change_count"] as? Int == 10,
               (fixture["changes"] as? [[String: Any]])?.count == 10,
               assets.atlasTiles.count == 128 else
         {
            throw AppKitProductionScenarioFailure.invalidFixture
         }
         dashboardInitialLabels.removeAll(keepingCapacity: true)
         for cardIndex in 0..<32
         {
            let labelCount = cardIndex < 16 ? 6 : 5
            for labelIndex in 0..<labelCount
            {
               dashboardInitialLabels.append("Node \(cardIndex * 6 + labelIndex)")
            }
         }
         dashboardLabels = dashboardInitialLabels
      case AppKitProductionSceneID.dashboardMixedStatic.rawValue, "idle.steady":
         guard fixture["visible_node_count"] as? Int == 300,
               fixture["shadow_count"] as? Int == 32,
               fixture["clipped_rounded_cards"] as? Int == 32,
               fixture["backdrop_blur_count"] as? Int == 4,
               assets.atlasTiles.count == 128 else
         {
            throw AppKitProductionScenarioFailure.invalidFixture
         }
         dashboardInitialLabels.removeAll(keepingCapacity: true)
         for cardIndex in 0..<32
         {
            let labelCount = cardIndex < 16 ? 6 : 5
            for labelIndex in 0..<labelCount
            {
               dashboardInitialLabels.append("Node \(cardIndex * 6 + labelIndex)")
            }
         }
         dashboardLabels = dashboardInitialLabels
      case AppKitProductionSceneID.enduranceChurn.rawValue:
         guard fixture["visible_node_count"] as? Int == 300,
               fixture["shadow_count"] as? Int == 32,
               fixture["clipped_rounded_cards"] as? Int == 32,
               fixture["backdrop_blur_count"] as? Int == 4,
               fixture["heavy_screen_cycle_count"] as? Int == 100,
               fixture["tab_switch_count"] as? Int == 500,
               fixture["animation_frame_count"] as? Int == 600,
               fixture["tab_count"] as? Int == 2,
               fixture["initial_tab_index"] as? Int == 0,
               fixture["heavy_screen_target_id"] as? String == "endurance:heavy-screen-visible",
               fixture["active_tab_target_id"] as? String == "endurance:active-tab",
               fixture["animation_frame_target_id"] as? String == "endurance:animation-frame",
               let categories = fixture["categories"] as? [String: Any],
               categories["label"] as? Int == 176,
               categories["icon_image"] as? Int == 64,
               categories["rounded_card"] as? Int == 32,
               categories["control"] as? Int == 24,
               categories["backdrop_region"] as? Int == 4,
               assets.atlasTiles.count == 128 else
         {
            throw AppKitProductionScenarioFailure.invalidFixture
         }
         dashboardInitialLabels.removeAll(keepingCapacity: true)
         for cardIndex in 0..<32
         {
            let labelCount = cardIndex < 16 ? 6 : 5
            for labelIndex in 0..<labelCount
            {
               dashboardInitialLabels.append("Node \(cardIndex * 6 + labelIndex)")
            }
         }
         dashboardLabels = dashboardInitialLabels
      case AppKitProductionSceneID.feedVariableScroll.rawValue:
         guard let rows = fixture["rows"] as? [[String: Any]],
               rows.count == 2_000,
               fixture["row_count"] as? Int == rows.count,
               let prependIDs = fixture["prepend_rows"] as? [String],
               prependIDs.count == 20,
               assets.atlasTiles.count == 128 else
         {
            throw AppKitProductionScenarioFailure.invalidFixture
         }
         feedInitialRows = rows
         feedRows = rows
         feedPrependRows = prependIDs.enumerated().map
         {
            index, id in
            ["id": id, "height": 76, "text": "Prepended \(index)", "thumbnail_index": index] as [String: Any]
         }
         feedRowByID = Dictionary(uniqueKeysWithValues: (feedRows + feedPrependRows).compactMap
         {
            row in
            (row["id"] as? String).map {($0, row)}
         })
      case AppKitProductionSceneID.chatLiveUpdate.rawValue:
         guard fixture["message_count"] as? Int == 5_000,
               fixture["avatar_count"] as? Int == 64,
               let messages = fixture["messages"] as? [[String: Any]],
               messages.count == 5_000,
               let prepend = fixture["prepend_messages"] as? [[String: Any]],
               prepend.count == 50,
               fixture["append_rate_hz"] as? Int == 10,
               (fixture["typed_text"] as? String)?.count == 100,
               (fixture["pasted_text"] as? String)?.utf8.count == 10 * 1_024,
               let selection = fixture["selection_replacement"] as? [String: Any],
               selection["message_id"] as? String == "chat:append:16",
               selection["start_utf8"] as? Int == 0,
               selection["end_utf8"] as? Int == 6,
               selection["replacement"] as? String == "Oxide",
               assets.atlasTiles.count == 128 else
         {
            throw AppKitProductionScenarioFailure.invalidFixture
         }
         chatInitialMessages = try messages.map(decodeChatMessage)
         chatMessages = chatInitialMessages
         chatPrependMessages = try prepend.map(decodeChatMessage)
         chatIndexByID = Dictionary(uniqueKeysWithValues: chatMessages.enumerated().map {($0.element.id, $0.offset)})
         chatAppendTemplates = Array(chatInitialMessages.prefix(8))
         chatSelection = AppKitProductionChatSelection(
            messageID: "chat:append:16",
            startUTF8: 0,
            endUTF8: 6,
            replacement: "Oxide"
         )
      case AppKitProductionSceneID.navigationModal.rawValue:
         guard fixture["list_item_count"] as? Int == 12,
               fixture["cycle_count"] as? Int == 4,
               fixture["selected_item_id"] as? String == "navigation:item:05" else
         {
            throw AppKitProductionScenarioFailure.invalidFixture
         }
      case AppKitProductionSceneID.imageDecodeZoom.rawValue:
         guard fixture["pan_distance_millionths"] as? Int == 450_000,
               fixture["pinch_scale_millionths"] as? Int == 2_000_000,
               assets.thumbnail != nil else
         {
            throw AppKitProductionScenarioFailure.invalidFixture
         }
      default:
         throw AppKitProductionScenarioFailure.unsupportedScenario(scenario.id)
      }
   }

   private static func fixtureID(for scenarioID: String) -> String
   {
      scenarioID == "idle.steady" ? AppKitProductionSceneID.dashboardMixedStatic.rawValue : scenarioID
   }

   private func applyStartup(_ event: BenchmarkTraceEvent) throws
   {
      switch event.op
      {
      case "foreground":
         startupForegroundCount += 1
         startupVisible = true
      case "background":
         startupBackgroundCount += 1
         startupVisible = false
      case "resource-arrival":
         guard event.target == "fresh-install" else
         {
            throw AppKitProductionScenarioFailure.unsupportedEvent(event.target ?? event.op)
         }
         startupFreshInstallReady = true
      default:
         throw AppKitProductionScenarioFailure.unsupportedEvent(event.op)
      }
      startupLifecycleStateID = event.stateId
   }

   private func applyDashboard(_ event: BenchmarkTraceEvent) throws
   {
      guard event.op == "mutate", let target = event.target else
      {
         throw AppKitProductionScenarioFailure.unsupportedEvent(event.op)
      }
      if target == "dashboard:update-count"
      {
         guard case .integer(let count)? = event.value, count == 30 else
         {
            throw AppKitProductionScenarioFailure.unsupportedEvent(target)
         }
         dashboardBulkUpdates = Int(count)
      }
      else if target.hasPrefix("dashboard:label:")
      {
         guard let index = Int(target.dropFirst("dashboard:label:".count)), index < dashboardLabels.count else
         {
            throw AppKitProductionScenarioFailure.unsupportedEvent(target)
         }
         dashboardLeafUpdates += 1
         dashboardLabels[index] = "Updated \(dashboardLeafUpdates)"
      }
      else
      {
         throw AppKitProductionScenarioFailure.unsupportedEvent(target)
      }
   }

   private func applyEndurance(_ event: BenchmarkTraceEvent) throws
   {
      guard event.op == "mutate", let target = event.target else
      {
         throw AppKitProductionScenarioFailure.unsupportedEvent(event.op)
      }
      if target == "endurance:heavy-screen-visible"
      {
         guard case .boolean(let visible)? = event.value,
               visible != enduranceHeavyScreenVisible,
               enduranceHeavyScreenTransitions < 200 else
         {
            throw AppKitProductionScenarioFailure.unsupportedEvent(target)
         }
         enduranceHeavyScreenVisible = visible
         enduranceHeavyScreenTransitions += 1
         if visible
         {
            enduranceOpenCloseCycles += 1
         }
         return
      }
      if target == "endurance:active-tab"
      {
         guard case .integer(let index)? = event.value,
               index >= 0,
               index < 2,
               Int(index) != enduranceActiveTabIndex,
               enduranceTabSwitches < 500 else
         {
            throw AppKitProductionScenarioFailure.unsupportedEvent(target)
         }
         enduranceActiveTabIndex = Int(index)
         enduranceTabSwitches += 1
         dashboardLabels = dashboardInitialLabels.map {enduranceActiveTabIndex == 0 ? $0 : "Tab \($0)"}
         return
      }
      if target == "endurance:animation-frame"
      {
         guard case .integer(let frame)? = event.value,
               frame == Int64(enduranceAnimationFrameIndex + 1),
               frame <= 600 else
         {
            throw AppKitProductionScenarioFailure.unsupportedEvent(target)
         }
         enduranceAnimationFrameIndex = Int(frame)
         enduranceAnimationFrames += 1
         dashboardBulkUpdates = Int(frame) % 30
         return
      }
      throw AppKitProductionScenarioFailure.unsupportedEvent(target)
   }

   private func applyFeed(_ event: BenchmarkTraceEvent) throws
   {
      switch event.op
      {
      case "pointer-down":
         break
      case "pointer-move", "pointer-up":
         guard let y = event.yMillionths else {return}
         feedScrollMillionths = UInt32(max(0, 1_000_000 - y))
         feedScrollOffset = max(0, feedContentHeight - 792) * CGFloat(feedScrollMillionths) / 1_000_000
      case "mutate":
         if event.target == "feed:prepend-count"
         {
            guard case .integer(let count)? = event.value, count == 20, feedPrependCount == 0 else
            {
               throw AppKitProductionScenarioFailure.unsupportedEvent(event.target ?? event.op)
            }
            feedScrollOffset += feedPrependRows.reduce(0) {$0 + CGFloat($1["height"] as? Int ?? 76)}
            feedRows.insert(contentsOf: feedPrependRows, at: 0)
            feedPrependCount = feedPrependRows.count
         }
         else if let target = event.target, target.hasSuffix(":favorite")
         {
            feedFavoriteID = String(target.dropLast(":favorite".count))
         }
         else
         {
            throw AppKitProductionScenarioFailure.unsupportedEvent(event.target ?? event.op)
         }
      default:
         throw AppKitProductionScenarioFailure.unsupportedEvent(event.op)
      }
   }

   private func applyChat(_ event: BenchmarkTraceEvent) throws
   {
      switch event.op
      {
      case "mutate": try applyChatMutation(event)
      case "focus": try applyChatSelection(event)
      case "commit-text": try applyChatText(event)
      default: throw AppKitProductionScenarioFailure.unsupportedEvent(event.op)
      }
   }

   private func applyChatMutation(_ event: BenchmarkTraceEvent) throws
   {
      if event.target == "chat:prepend-count"
      {
         guard case .integer(let count)? = event.value, count == 50, chatPrependCount == 0 else
         {
            throw AppKitProductionScenarioFailure.unsupportedEvent(event.target ?? event.op)
         }
         chatMessages.insert(contentsOf: chatPrependMessages, at: 0)
         chatIndexByID = Dictionary(uniqueKeysWithValues: chatMessages.enumerated().map {($0.element.id, $0.offset)})
         chatPrependCount = chatPrependMessages.count
         return
      }
      guard event.target == "chat:append",
            case .text(let id)? = event.value,
            let suffix = Int(id.split(separator: ":").last ?? ""),
            suffix == chatAppendCount,
            !chatAppendTemplates.isEmpty else
      {
         throw AppKitProductionScenarioFailure.unsupportedEvent(event.target ?? event.op)
      }
      let template = chatAppendTemplates[suffix % chatAppendTemplates.count]
      let message = AppKitProductionChatMessage(
         id: id,
         sequence: 5_050 + suffix,
         authorIndex: suffix % 64,
         avatarIndex: suffix % 64,
         direction: template.direction,
         text: template.text
      )
      chatIndexByID[id] = chatMessages.count
      chatMessages.append(message)
      chatAppendCount += 1
   }

   private func applyChatSelection(_ event: BenchmarkTraceEvent) throws
   {
      guard let selection = chatSelection,
            event.target == selection.messageID,
            case .text(let value)? = event.value else
      {
         throw AppKitProductionScenarioFailure.unsupportedEvent(event.target ?? event.op)
      }
      let components = value.split(separator: ":")
      guard components.count == 2,
            let start = Int(components[0]),
            let end = Int(components[1]),
            start == selection.startUTF8,
            end == selection.endUTF8 else
      {
         throw AppKitProductionScenarioFailure.unsupportedEvent(value)
      }
      chatSelectedMessageID = event.target
      chatSelectedUTF8Range = start..<end
   }

   private func applyChatText(_ event: BenchmarkTraceEvent) throws
   {
      guard case .text(let value)? = event.value else
      {
         throw AppKitProductionScenarioFailure.unsupportedEvent(event.op)
      }
      if event.target == "chat:composer"
      {
         chatComposer.append(value)
         return
      }
      guard let selection = chatSelection,
            event.target == chatSelectedMessageID,
            event.target == selection.messageID,
            value == selection.replacement,
            let range = chatSelectedUTF8Range,
            let id = event.target,
            let index = chatIndexByID[id],
            let replaced = replacingUTF8(in: chatMessages[index].text, range: range, with: value) else
      {
         throw AppKitProductionScenarioFailure.unsupportedEvent(event.target ?? event.op)
      }
      chatMessages[index].text = replaced
      chatSelectedUTF8Range = nil
      chatReplacementApplied = true
   }

   private func applyNavigation(_ event: BenchmarkTraceEvent) throws
   {
      if event.op == "pointer-down"
      {
         navigationRoute = "detail"
         return
      }
      if event.op == "pointer-move"
      {
         navigationModalProgress = CGFloat(event.xMillionths ?? 0) / 1_000_000
         navigationTransition = nil
         return
      }
      if event.op == "pointer-cancel"
      {
         navigationTransition = NavigationTransition(target: 0, startedAtUs: virtualTimeUs)
         return
      }
      guard event.op == "navigate", let target = event.target else
      {
         throw AppKitProductionScenarioFailure.unsupportedEvent(event.op)
      }
      switch target
      {
      case "navigation:item:05": navigationRoute = "detail"
      case "navigation:modal": navigationTransition = NavigationTransition(target: 1, startedAtUs: virtualTimeUs)
      case "navigation:dismiss-control": navigationTransition = NavigationTransition(target: 0, startedAtUs: virtualTimeUs)
      case "navigation:back-control":
         navigationRoute = "list"
         navigationModalProgress = 0
         navigationTransition = nil
         navigationCycle += 1
      case "navigation:cancel":
         navigationRoute = "list"
         navigationModalProgress = 0
         navigationTransition = nil
      default: throw AppKitProductionScenarioFailure.unsupportedEvent(target)
      }
   }

   private func applyImage(_ event: BenchmarkTraceEvent) throws
   {
      if event.op == "resource-arrival"
      {
         try applyImageResource(event)
         return
      }
      guard let pointer = event.pointer,
            let x = event.xMillionths,
            let y = event.yMillionths else
      {
         throw AppKitProductionScenarioFailure.unsupportedEvent(event.op)
      }
      let point = CGPoint(x: CGFloat(x) * 390 / 1_000_000, y: CGFloat(y) * 844 / 1_000_000 - 52)
      switch event.op
      {
      case "pointer-down": imagePointerDown(pointer, point: point)
      case "pointer-move": imagePointerMove(pointer, point: point)
      case "pointer-up":
         imagePointerMove(pointer, point: point)
         imagePointerEnded(pointer)
      case "pointer-cancel": imagePointerEnded(pointer)
      default: throw AppKitProductionScenarioFailure.unsupportedEvent(event.op)
      }
   }

   private func applyGrid(_ event: BenchmarkTraceEvent) throws
   {
      switch event.op
      {
      case "wheel":
         guard event.target == "grid:collection" else {throw AppKitProductionScenarioFailure.unsupportedEvent(event.target ?? event.op)}
         gridScrollMillionths = min(max(gridScrollMillionths + Int(event.deltaYMillionths ?? 0), 0), 1_000_000)
      case "pointer-down", "pointer-up": break
      case "navigate":
         if event.target == "grid:detail:07500" {gridDetailVisible = true}
         else if event.target == "grid:collection" {gridDetailVisible = false}
         else {throw AppKitProductionScenarioFailure.unsupportedEvent(event.target ?? event.op)}
      default: throw AppKitProductionScenarioFailure.unsupportedEvent(event.op)
      }
   }

   private func applyEffects(_ event: BenchmarkTraceEvent) throws
   {
      if event.op == "resource-arrival", event.target == "effects:layer-resources" {return}
      guard event.op == "mutate", case .integer(let value) = event.value else
      {
         throw AppKitProductionScenarioFailure.unsupportedEvent(event.op)
      }
      switch event.target
      {
      case "effects:animation-progress-millionths": effectsAnimationProgress = Int(value)
      case "effects:layer:037:dirty-generation": effectsDirtyGeneration = Int(value)
      default: throw AppKitProductionScenarioFailure.unsupportedEvent(event.target ?? event.op)
      }
   }

   private func applyMutation(_ event: BenchmarkTraceEvent) throws
   {
      guard event.op == "mutate",
            let target = event.target,
            target.hasPrefix("mutation:"),
            target.hasSuffix(":generation"),
            case .integer(let generation) = event.value,
            let classes = fixture["mutation_classes"] as? [[String: Any]],
            let formula = fixture["selection_formula"] as? [String: Any] else
      {
         throw AppKitProductionScenarioFailure.unsupportedEvent(event.op)
      }
      let classID = String(target.dropFirst("mutation:".count).dropLast(":generation".count))
      guard let mutation = classes.first(where: {$0["id"] as? String == classID}),
            let changedCount = mutation["changed_node_count"] as? Int,
            let seed = formula["seed"] as? Int,
            let multiplier = formula["multiplier"] as? Int,
            let increment = formula["increment"] as? Int,
            let modulus = formula["modulus"] as? Int else
      {
         throw AppKitProductionScenarioFailure.unsupportedEvent(target)
      }
      mutationChanged.removeAll()
      for ordinal in 0..<changedCount
      {
         mutationChanged.insert((seed + Int(generation) * increment + ordinal * multiplier) % modulus)
      }
      mutationClass = classID
      mutationGeneration = Int(generation)
   }

   private func applyText(_ event: BenchmarkTraceEvent) throws
   {
      if event.op == "resource-arrival", event.target == "text:font-pack" || event.target == "text:inline-atlas" {return}
      switch (event.op, event.target, event.value)
      {
      case ("mutate", "text:replay-generation", .integer(let value)):
         textReplayGeneration = Int(value)
         textFontAtlasCold = false
      case ("scale", "text:scale-millionths", .integer(let value)): textScaleMillionths = Int(value)
      case ("mutate", "text:wrap-width-rotation", .integer(let value)): textWrapRotation = Int(value)
      default: throw AppKitProductionScenarioFailure.unsupportedEvent(event.op)
      }
   }

   private func applyResize(_ event: BenchmarkTraceEvent) throws
   {
      guard let changes = fixture["changes"] as? [[String: Any]], resizeChangeIndex < changes.count else
      {
         throw AppKitProductionScenarioFailure.unsupportedEvent(event.op)
      }
      let change = changes[resizeChangeIndex]
      switch (event.op, event.target, event.value)
      {
      case ("orientation", "resize:orientation", .text(let value)) where value == change["orientation"] as? String:
         resizeOrientation = value
      case ("resize", "resize:viewport", .text(let value)):
         let width = change["width"] as? Int ?? 0
         let height = change["height"] as? Int ?? 0
         guard value == "\(width)x\(height)" else {throw AppKitProductionScenarioFailure.unsupportedEvent(value)}
         resizeWidth = width
         resizeHeight = height
      case ("theme", "resize:theme", .text(let value)) where value == change["theme"] as? String:
         resizeTheme = value
         resizeChangeIndex += 1
      default: throw AppKitProductionScenarioFailure.unsupportedEvent(event.op)
      }
   }

   private func applyImageResource(_ event: BenchmarkTraceEvent) throws
   {
      switch event.target
      {
      case "image:source-bytes": imageBytesReady = true
      case "image:decoded":
         imageDecoded = try assets.decodedSource()
         imageDidDecode = true
      case "image:texture":
         guard imageDecoded != nil else
         {
            throw AppKitProductionScenarioFailure.unsupportedEvent(event.target ?? event.op)
         }
         imageUploaded = true
      case "image:presented":
         guard imageDecoded != nil, imageUploaded else
         {
            throw AppKitProductionScenarioFailure.unsupportedEvent(event.target ?? event.op)
         }
         imageFirstVisible = true
      default: throw AppKitProductionScenarioFailure.unsupportedEvent(event.target ?? event.op)
      }
   }

   private func imagePointerDown(_ pointer: UInt32, point: CGPoint)
   {
      imagePointers[pointer] = point
      if imagePointers.count == 1
      {
         imagePanOrigin = point
         imagePanStart = imageTranslation
      }
      else if imagePointers.count == 2
      {
         imagePinchStartDistance = imagePointerDistance
         imagePinchStartScale = imageScale
      }
   }

   private func imagePointerMove(_ pointer: UInt32, point: CGPoint)
   {
      guard imagePointers[pointer] != nil else {return}
      imagePointers[pointer] = point
      if imagePointers.count >= 2,
         let start = imagePinchStartDistance,
         start > 0,
         let distance = imagePointerDistance
      {
         imageScale = max(1, min(2, imagePinchStartScale * distance / start))
      }
      else if let origin = imagePanOrigin
      {
         imageTranslation = CGPoint(
            x: imagePanStart.x + point.x - origin.x,
            y: imagePanStart.y + point.y - origin.y
         )
      }
   }

   private func imagePointerEnded(_ pointer: UInt32)
   {
      imagePointers.removeValue(forKey: pointer)
      imagePinchStartDistance = nil
      if let remaining = imagePointers.values.first
      {
         imagePanOrigin = remaining
         imagePanStart = imageTranslation
      }
      else
      {
         imagePanOrigin = nil
      }
   }

   private var imagePointerDistance: CGFloat?
   {
      let points = Array(imagePointers.values.prefix(2))
      guard points.count == 2 else {return nil}
      return hypot(points[1].x - points[0].x, points[1].y - points[0].y)
   }

   private var imageResourceStage: String
   {
      if imageFirstVisible {return "visible"}
      if imageUploaded {return "uploaded"}
      if imageDidDecode {return "decoded"}
      if imageBytesReady {return "bytes-ready"}
      return "thumbnail"
   }

   var navigationModalVisible: Bool
   {
      navigationModalProgress > 0 || navigationTransition?.target == 1
   }

   private var visibleFeedRowCount: Int
   {
      guard !feedRows.isEmpty else {return 0}
      let limit = feedScrollOffset + 792
      var cursor: CGFloat = 0
      var visible = 0
      for row in feedRows
      {
         let height = CGFloat(row["height"] as? Int ?? 76)
         if cursor + height > feedScrollOffset && cursor < limit
         {
            visible += 1
         }
         if cursor >= limit {break}
         cursor += height
      }
      return max(visible, 1)
   }

   private var feedContentHeight: CGFloat
   {
      feedRows.reduce(0) {$0 + CGFloat($1["height"] as? Int ?? 76)}
   }

   private func roles(_ values: [(String, Int)]) -> [BenchmarkRoleCount]
   {
      values.map {BenchmarkRoleCount(role: $0.0, count: UInt32($0.1))}
   }

   private func decodeChatMessage(_ dictionary: [String: Any]) throws -> AppKitProductionChatMessage
   {
      guard let id = dictionary["id"] as? String,
            let sequence = dictionary["sequence"] as? Int,
            let authorIndex = dictionary["author_index"] as? Int,
            let avatarIndex = dictionary["avatar_index"] as? Int,
            let direction = dictionary["direction"] as? String,
            let text = dictionary["text"] as? String else
      {
         throw AppKitProductionScenarioFailure.invalidFixture
      }
      return AppKitProductionChatMessage(
         id: id,
         sequence: sequence,
         authorIndex: authorIndex,
         avatarIndex: avatarIndex,
         direction: direction,
         text: text
      )
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
}
