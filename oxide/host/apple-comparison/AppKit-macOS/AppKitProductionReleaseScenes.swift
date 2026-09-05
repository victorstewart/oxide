import AppKit
import Foundation

struct AppKitGridRecord
{
   let index: Int
   let image: NSImage
   let atlas: NSImage
   let title: String
   let titleFont: NSFont
}

final class AppKitGridCollectionItem: NSCollectionViewItem
{
   static let reuseIdentifier = NSUserInterfaceItemIdentifier("grid.tile")
   let thumbnail = AppKitProductionRoundedImageView(frame: CGRect(x: 8, y: 8, width: 100, height: 104))
   let titleField = AppKitProductionExactTextField(frame: CGRect(x: 8, y: 116, width: 100, height: 20))
   private(set) var actionButton = NSButton(frame: CGRect(x: 0, y: 0, width: 116, height: 144))

   override func loadView()
   {
      let view = AppKitProductionRoundedSurfaceView(frame: CGRect(x: 0, y: 0, width: 116, height: 144))
      thumbnail.imageScaling = .scaleAxesIndependently
      actionButton.title = ""
      actionButton.isBordered = false
      actionButton.isTransparent = true
      view.addSubview(thumbnail)
      view.addSubview(titleField)
      view.addSubview(actionButton)
      self.view = view
   }

   func configure(_ record: AppKitGridRecord, target: AnyObject, action: Selector)
   {
      loadViewIfNeeded()
      let identifier = String(format: "grid:tile:%05d", record.index)
      view.identifier = NSUserInterfaceItemIdentifier(identifier)
      thumbnail.identifier = NSUserInterfaceItemIdentifier("grid:thumbnail:\(String(format: "%05d", record.index))")
      thumbnail.configure(image: record.image, atlas: record.atlas, tileIndex: record.index % 128)
      titleField.configure(
         identifier: "grid:label:\(String(format: "%05d", record.index))",
         value: record.title,
         font: record.titleFont,
         color: AppKitProductionPalette.text
      )
      actionButton.identifier = NSUserInterfaceItemIdentifier(identifier)
      actionButton.setAccessibilityIdentifier(identifier)
      actionButton.setAccessibilityLabel(record.title)
      actionButton.setAccessibilityRole(.button)
      actionButton.target = target
      actionButton.action = action
      actionButton.tag = record.index
   }
}

final class AppKitGridCollectionController: NSObject, NSCollectionViewDataSource
{
   private(set) var records = [AppKitGridRecord]()
   private(set) var reloadCount = 0
   var onActivate: ((Int) -> Void)?

   func install(on collection: NSCollectionView)
   {
      collection.dataSource = self
   }

   func replace(_ records: [AppKitGridRecord], in collection: NSCollectionView)
   {
      guard self.records.count != records.count else {return}
      self.records = records
      reloadCount += 1
      collection.reloadData()
   }

   func numberOfSections(in collectionView: NSCollectionView) -> Int {1}
   func collectionView(_ collectionView: NSCollectionView, numberOfItemsInSection section: Int) -> Int {records.count}

   func collectionView(_ collectionView: NSCollectionView, itemForRepresentedObjectAt indexPath: IndexPath) -> NSCollectionViewItem
   {
      let item = collectionView.makeItem(withIdentifier: AppKitGridCollectionItem.reuseIdentifier, for: indexPath)
      guard let gridItem = item as? AppKitGridCollectionItem, indexPath.item < records.count else {return item}
      gridItem.configure(records[indexPath.item], target: self, action: #selector(activate(_:)))
      return gridItem
   }

   @objc private func activate(_ sender: NSButton)
   {
      onActivate?(sender.tag)
   }

   func uninstall(from collection: NSCollectionView)
   {
      onActivate = nil
      collection.dataSource = nil
      records.removeAll(keepingCapacity: false)
   }
}

final class AppKitGridProductionSceneRoot: NSObject, AppKitProductionSceneRoot
{
   static let sceneID = AppKitProductionSceneID.gridLargeScroll
   static let componentContracts = [
      AppKitProductionComponentContract(identifier: "grid.scroll", componentClass: NSScrollView.self),
      AppKitProductionComponentContract(identifier: "grid.collection", componentClass: NSCollectionView.self),
      AppKitProductionComponentContract(identifier: "grid.detail", componentClass: NSView.self),
      AppKitProductionComponentContract(identifier: "grid.back", componentClass: NSButton.self),
   ]
   static let reusableComponentContracts = [
      AppKitProductionReusableComponentContract(role: "grid-tile", componentClass: AppKitGridCollectionItem.self),
   ]

   let storage = AppKitProductionSceneStorage(sceneID: .gridLargeScroll)
   let collection: NSCollectionView
   let scrollView: NSScrollView
   let heading: AppKitProductionExactTextField
   let detail: NSView
   let detailImage: AppKitProductionRoundedImageView
   let detailTitle: AppKitProductionExactTextField
   let detailFont: NSFont
   let back: NSButton
   let controller = AppKitGridCollectionController()
   private var suppressScrollCallback = false
   private var scrollObserver: NSObjectProtocol?
   var onSelect: ((Int) -> Void)?
   var onBack: (() -> Void)?
   var onScroll: ((Int) -> Void)?

   var columnCount: Int
   {
      let width = scrollView.contentSize.width
      return max(1, Int((width - 24 + 8) / (116 + 8)))
   }

   init(
      headingFont: NSFont = .systemFont(ofSize: 20),
      detailFont: NSFont = .systemFont(ofSize: 20),
      backFont: NSFont = .systemFont(ofSize: 15)
   )
   {
      storage.useDirectLayout()
      collection = storage.makeCollection(identifier: "grid.collection")
      collection.translatesAutoresizingMaskIntoConstraints = true
      collection.wantsLayer = true
      collection.layer?.backgroundColor = AppKitProductionPalette.background.cgColor
      collection.backgroundColors = [AppKitProductionPalette.background]
      let layout = collection.collectionViewLayout as! NSCollectionViewFlowLayout
      layout.itemSize = NSSize(width: 116, height: 144)
      layout.minimumInteritemSpacing = 8
      layout.minimumLineSpacing = 8
      layout.sectionInset = NSEdgeInsets(top: 4, left: 12, bottom: 12, right: 14)
      collection.register(AppKitGridCollectionItem.self, forItemWithIdentifier: AppKitGridCollectionItem.reuseIdentifier)
      scrollView = storage.makeScrollView(identifier: "grid.scroll", clipIdentifier: "grid.clip")
      scrollView.translatesAutoresizingMaskIntoConstraints = true
      scrollView.autoresizingMask = []
      scrollView.frame = CGRect(x: 0, y: 48, width: 390, height: 796)
      scrollView.hasVerticalScroller = false
      scrollView.drawsBackground = false
      scrollView.backgroundColor = AppKitProductionPalette.background
      scrollView.contentView.backgroundColor = AppKitProductionPalette.background
      scrollView.documentView = collection
      scrollView.contentView.postsBoundsChangedNotifications = true
      heading = AppKitProductionExactTextField(frame: CGRect(x: 16, y: 8, width: 240, height: 32))
      heading.configure(identifier: "grid.heading", value: "Thumbnail Grid", font: headingFont, color: AppKitProductionPalette.text)
      detail = AppKitProductionRootView(frame: CGRect(x: 0, y: 0, width: 390, height: 844))
      detail.identifier = NSUserInterfaceItemIdentifier("grid.detail")
      detail.setAccessibilityIdentifier("grid.detail")
      detailImage = AppKitProductionRoundedImageView(frame: CGRect(x: 24, y: 92, width: 342, height: 342))
      detailImage.imageScaling = .scaleAxesIndependently
      self.detailFont = detailFont
      detailTitle = AppKitProductionExactTextField(frame: CGRect(x: 24, y: 452, width: 342, height: 32))
      detailTitle.configure(identifier: "grid.detail.title", value: "", font: detailFont, color: AppKitProductionPalette.text)
      back = storage.nativeControls.makeButton(identifier: "grid.back", title: "‹ Back")
      back.translatesAutoresizingMaskIntoConstraints = true
      back.autoresizingMask = []
      back.frame = CGRect(x: 16, y: 16, width: 72, height: 44)
      back.isBordered = false
      back.contentTintColor = AppKitProductionPalette.accent
      back.font = backFont
      detail.addSubview(detailImage)
      detail.addSubview(detailTitle)
      detail.addSubview(back)
      detail.isHidden = true
      super.init()
      storage.rootView.addSubview(heading)
      storage.rootView.addSubview(scrollView)
      storage.rootView.addSubview(detail)
      controller.install(on: collection)
      controller.onActivate = {[weak self] index in self?.onSelect?(index)}
      back.target = self
      back.action = #selector(performBack(_:))
      scrollObserver = NotificationCenter.default.addObserver(
         forName: NSView.boundsDidChangeNotification,
         object: scrollView.contentView,
         queue: nil
      )
      {
         [weak self] _ in self?.didScroll()
      }
   }

   func present(records: [AppKitGridRecord], scrollMillionths: Int, detailRecord: AppKitGridRecord?)
   {
      let columns = columnCount
      let rows = (records.count + columns - 1) / columns
      let contentHeight = 4 + CGFloat(rows * 144 + max(rows - 1, 0) * 8) + 12
      collection.frame.size = NSSize(width: scrollView.contentSize.width, height: max(contentHeight, scrollView.contentSize.height))
      controller.replace(records, in: collection)
      detail.isHidden = detailRecord == nil
      scrollView.isHidden = detailRecord != nil
      heading.isHidden = detailRecord != nil
      if let detailRecord
      {
         detailImage.configure(image: detailRecord.image, atlas: detailRecord.atlas, tileIndex: detailRecord.index % 128, radius: 12)
         detailTitle.configure(identifier: "grid.detail.title", value: detailRecord.title, font: detailFont, color: AppKitProductionPalette.text)
      }
      guard detailRecord == nil else {return}
      collection.collectionViewLayout?.invalidateLayout()
      collection.layoutSubtreeIfNeeded()
      let maximum = max(collection.frame.height - scrollView.contentSize.height, 0)
      suppressScrollCallback = true
      scrollView.contentView.scroll(to: CGPoint(x: 0, y: maximum * CGFloat(scrollMillionths) / 1_000_000))
      scrollView.reflectScrolledClipView(scrollView.contentView)
      suppressScrollCallback = false
   }

   @objc private func performBack(_ sender: NSButton)
   {
      onBack?()
   }

   private func didScroll()
   {
      guard !suppressScrollCallback else {return}
      let maximum = max((collection.frame.height - scrollView.contentSize.height), 0)
      let value = maximum > 0 ? Int((scrollView.contentView.bounds.minY / maximum * 1_000_000).rounded()) : 0
      onScroll?(min(max(value, 0), 1_000_000))
   }

   func teardown()
   {
      if let scrollObserver {NotificationCenter.default.removeObserver(scrollObserver)}
      scrollObserver = nil
      onSelect = nil
      onBack = nil
      onScroll = nil
      controller.uninstall(from: collection)
      scrollView.documentView = nil
      storage.nativeControls.removeActionBindings()
   }
}

final class AppKitEffectsCardView: NSView
{
   let content = AppKitProductionRoundedSurfaceView(frame: CGRect(x: 0, y: 0, width: 68, height: 68))

   init(index: Int, shadowed: Bool)
   {
      super.init(frame: CGRect(x: 0, y: 0, width: 68, height: 68))
      identifier = NSUserInterfaceItemIdentifier(String(format: "effects:card:%03d", index))
      wantsLayer = true
      layer?.masksToBounds = false
      if shadowed
      {
         layer?.shadowColor = AppKitProductionPalette.shadow.cgColor
         layer?.shadowOpacity = 1
         layer?.shadowRadius = 0
         layer?.shadowOffset = CGSize(width: 0, height: -2)
         layer?.shadowPath = CGPath(roundedRect: bounds, cornerWidth: 12, cornerHeight: 12, transform: nil)
      }
      content.wantsLayer = true
      content.layer?.cornerRadius = 12
      content.layer?.masksToBounds = true
      addSubview(content)
   }

   required init?(coder: NSCoder) {nil}
}

final class AppKitEffectsProductionSceneRoot: NSObject, AppKitProductionSceneRoot
{
   static let sceneID = AppKitProductionSceneID.effectsLayers
   static let componentContracts = [AppKitProductionComponentContract(identifier: "effects.surface", componentClass: NSView.self)]

   let storage = AppKitProductionSceneStorage(sceneID: .effectsLayers)
   let surface = AppKitProductionRootView(frame: CGRect(x: 0, y: 0, width: 390, height: 844))
   let heading: AppKitProductionExactTextField
   private(set) var cards = [AppKitEffectsCardView]()
   private(set) var backdrops = [NSVisualEffectView]()
   private(set) var lastAnimatedCardCount = 0
   private(set) var lastAnimatedBackdropCount = 0
   private(set) var dirtyInvalidationCount = 0
   private var lastDirtyGeneration = 0

   init(headingFont: NSFont = NSFont.systemFont(ofSize: 20))
   {
      storage.useDirectLayout()
      surface.identifier = NSUserInterfaceItemIdentifier("effects.surface")
      surface.setAccessibilityIdentifier("effects.surface")
      surface.wantsLayer = true
      heading = AppKitProductionExactTextField(frame: CGRect(x: 16, y: 8, width: 280, height: 32))
      heading.configure(identifier: "effects.heading", value: "Layer Effects", font: headingFont, color: AppKitProductionPalette.text)
      backdrops = [5, 17, 29, 41, 53, 65, 77, 89].enumerated().map
      {
         ordinal, cardIndex in
         let row = cardIndex / 10
         let column = cardIndex % 10
         let effect = NSVisualEffectView(frame: CGRect(x: 8 + CGFloat(column) * 74, y: 64 + CGFloat(row) * 74, width: 68, height: 68))
         effect.identifier = NSUserInterfaceItemIdentifier(String(format: "effects:backdrop:%02d", ordinal))
         effect.material = .contentBackground
         effect.blendingMode = .withinWindow
         effect.state = .active
         effect.alphaValue = 56 / 255
         effect.wantsLayer = true
         effect.layer?.cornerRadius = 12
         effect.layer?.masksToBounds = true
         return effect
      }
      cards = (0..<100).map {AppKitEffectsCardView(index: $0, shadowed: $0 < 32)}
      super.init()
      storage.rootView.addSubview(surface)
      backdrops.forEach {surface.addSubview($0)}
      cards.forEach {surface.addSubview($0)}
      surface.addSubview(heading)
   }

   func present(progress: CGFloat, dirtyGeneration: Int)
   {
      CATransaction.begin()
      CATransaction.setDisableActions(true)
      for (index, card) in cards.enumerated()
      {
         let row = index / 10
         let column = index % 10
         card.frame.origin = CGPoint(x: 8 + CGFloat(column) * 74 + 12 * progress, y: 64 + CGFloat(row) * 74 + 8 * progress)
         card.alphaValue = 0.65 + 0.35 * progress
      }
      for (ordinal, cardIndex) in [5, 17, 29, 41, 53, 65, 77, 89].enumerated()
      {
         let row = cardIndex / 10
         let column = cardIndex % 10
         backdrops[ordinal].frame.origin = CGPoint(
            x: 8 + CGFloat(column) * 74 + 12 * progress,
            y: 64 + CGFloat(row) * 74 + 8 * progress
         )
         backdrops[ordinal].alphaValue = 56 / 255
      }
      CATransaction.commit()
      lastAnimatedCardCount = cards.count
      lastAnimatedBackdropCount = backdrops.count
      if dirtyGeneration != lastDirtyGeneration
      {
         cards[37].content.needsDisplay = true
         dirtyInvalidationCount += 1
         lastDirtyGeneration = dirtyGeneration
      }
   }

   func teardown()
   {
      cards.forEach {$0.layer?.removeAllAnimations()}
      backdrops.forEach {$0.layer?.removeAllAnimations()}
      storage.nativeControls.removeActionBindings()
   }
}

final class AppKitMutationProductionSceneRoot: NSObject, AppKitProductionSceneRoot
{
   static let sceneID = AppKitProductionSceneID.mutationDamage
   static let componentContracts = [AppKitProductionComponentContract(identifier: "mutation.surface", componentClass: NSView.self)]

   let storage = AppKitProductionSceneStorage(sceneID: .mutationDamage)
   let surface = AppKitProductionRootView(frame: CGRect(x: 0, y: 0, width: 390, height: 844))
   let heading: AppKitProductionExactTextField
   private(set) var nodes = [CALayer]()
   private(set) var lastSelectedNodeCount = 0
   private(set) var lastLayerWriteCount = 0
   private var lastChanged = IndexSet()
   private var generation = 0

   init(headingFont: NSFont = NSFont.systemFont(ofSize: 20))
   {
      storage.useDirectLayout()
      surface.identifier = NSUserInterfaceItemIdentifier("mutation.surface")
      surface.setAccessibilityIdentifier("mutation.surface")
      surface.wantsLayer = true
      heading = AppKitProductionExactTextField(frame: CGRect(x: 16, y: 8, width: 300, height: 32))
      heading.configure(identifier: "mutation.heading", value: "10,000-node mutation", font: headingFont, color: AppKitProductionPalette.text)
      nodes = (0..<10_000).map
      {
         index in
         let layer = CALayer()
         layer.name = String(format: "mutation:node:%05d", index)
         layer.frame = CGRect(
            x: 8 + CGFloat(index % 100) * 11 / 3,
            y: 52 + CGFloat(index / 100) * 22 / 3,
            width: 10 / 3,
            height: 7
         )
         layer.backgroundColor = AppKitProductionPalette.surface.cgColor
         return layer
      }
      super.init()
      storage.rootView.addSubview(surface)
      nodes.forEach {surface.layer?.addSublayer($0)}
      surface.addSubview(heading)
   }

   func present(changed: IndexSet, generation: Int)
   {
      guard generation != self.generation else {return}
      CATransaction.begin()
      CATransaction.setDisableActions(true)
      for index in lastChanged.subtracting(changed) {nodes[index].backgroundColor = AppKitProductionPalette.surface.cgColor}
      for index in changed {nodes[index].backgroundColor = AppKitProductionPalette.accent.cgColor}
      CATransaction.commit()
      lastSelectedNodeCount = changed.count
      lastLayerWriteCount = lastChanged.subtracting(changed).count + changed.count
      lastChanged = changed
      self.generation = generation
   }

   func teardown()
   {
      nodes.forEach {$0.removeFromSuperlayer()}
      storage.nativeControls.removeActionBindings()
   }
}

struct AppKitTextRecord
{
   let id: String
   let categoryID: String
   let value: String
   let direction: String
   let latinFont: NSFont
   let arabicFont: NSFont
   let cjkFont: NSFont
   let inlineText: AppKitProductionInlineText?

   func attributedString(scale: CGFloat) -> NSAttributedString
   {
      let result = NSMutableAttributedString()
      let scaledLatin = scaled(latinFont, by: scale)
      let scaledArabic = scaled(arabicFont, by: scale)
      let scaledCJK = scaled(cjkFont, by: scale)
      let pieces = inlineText?.pieces(value, font: scaledLatin)
         ?? [AppKitProductionInlinePiece(text: value, image: nil, advance: 0, width: 0, height: 0, topFromBaseline: 0)]
      for piece in pieces
      {
         if let image = piece.image
         {
            let attachment = NSTextAttachment()
            attachment.image = NSImage(cgImage: image, size: NSSize(width: piece.width, height: piece.height))
            result.append(NSAttributedString(attachment: attachment))
         }
         else
         {
            for character in piece.text
            {
               let font = font(for: character, latin: scaledLatin, arabic: scaledArabic, cjk: scaledCJK)
               result.append(NSAttributedString(string: String(character), attributes: [.font: font, .foregroundColor: AppKitProductionPalette.text]))
            }
         }
      }
      let paragraph = NSMutableParagraphStyle()
      paragraph.baseWritingDirection = direction == "rtl" ? .rightToLeft : .natural
      result.addAttribute(.paragraphStyle, value: paragraph, range: NSRange(location: 0, length: result.length))
      return result
   }

   private func font(for character: Character, latin: NSFont, arabic: NSFont, cjk: NSFont) -> NSFont
   {
      if character.unicodeScalars.contains(where: {(0x4E00...0x9FFF).contains(Int($0.value))}) {return cjk}
      if character.unicodeScalars.contains(where: {(0x0600...0x06FF).contains(Int($0.value))}) {return arabic}
      return latin
   }

   private func scaled(_ font: NSFont, by scale: CGFloat) -> NSFont
   {
      NSFont(descriptor: font.fontDescriptor, size: font.pointSize * scale) ?? font
   }
}

final class AppKitTextTableCellView: NSTableCellView
{
   let label = NSTextField(wrappingLabelWithString: "")

   override init(frame frameRect: NSRect)
   {
      super.init(frame: frameRect)
      addSubview(label)
      textField = label
   }

   required init?(coder: NSCoder)
   {
      nil
   }
}

final class AppKitTextProductionSceneRoot: NSObject, AppKitProductionSceneRoot, NSTableViewDataSource, NSTableViewDelegate
{
   static let sceneID = AppKitProductionSceneID.textMultilingual
   static let componentContracts = [
      AppKitProductionComponentContract(identifier: "text.scroll", componentClass: NSScrollView.self),
      AppKitProductionComponentContract(identifier: "text.table", componentClass: NSTableView.self),
   ]

   let storage = AppKitProductionSceneStorage(sceneID: .textMultilingual)
   let table: NSTableView
   let scrollView: NSScrollView
   let heading: AppKitProductionExactTextField
   private(set) var records = [AppKitTextRecord]()
   private(set) var renderedStrings = [NSAttributedString]()
   private(set) var fullReloadCount = 0
   private var wrapWidth = CGFloat(96)
   private var scale = CGFloat(1)

   init(headingFont: NSFont)
   {
      storage.useDirectLayout()
      table = storage.makeTable(identifier: "text.table")
      table.style = .plain
      table.headerView = nil
      table.rowHeight = 72
      table.intercellSpacing = .zero
      table.backgroundColor = AppKitProductionPalette.background
      table.frame = CGRect(x: 0, y: 0, width: 390, height: 2_880)
      table.translatesAutoresizingMaskIntoConstraints = true
      table.autoresizingMask = []
      scrollView = storage.makeScrollView(identifier: "text.scroll", clipIdentifier: "text.clip")
      scrollView.frame = CGRect(x: 0, y: 40, width: 390, height: 804)
      scrollView.translatesAutoresizingMaskIntoConstraints = true
      scrollView.autoresizingMask = []
      scrollView.hasVerticalScroller = false
      scrollView.drawsBackground = false
      scrollView.backgroundColor = AppKitProductionPalette.background
      scrollView.contentView.backgroundColor = AppKitProductionPalette.background
      scrollView.documentView = table
      heading = AppKitProductionExactTextField(frame: CGRect(x: 16, y: 8, width: 280, height: 32))
      heading.configure(identifier: "text.heading", value: "Multilingual Text", font: headingFont, color: AppKitProductionPalette.text)
      super.init()
      table.dataSource = self
      table.delegate = self
      storage.rootView.addSubview(heading)
      storage.rootView.addSubview(scrollView)
   }

   func present(records: [AppKitTextRecord], wrapWidth: CGFloat, scale: CGFloat)
   {
      let needsInitialLoad = self.records.isEmpty
      let needsTextUpdate = needsInitialLoad || self.scale != scale
      let needsWrapUpdate = self.wrapWidth != wrapWidth
      self.records = records
      self.wrapWidth = wrapWidth
      self.scale = scale
      table.tableColumns.first?.width = wrapWidth
      if needsTextUpdate
      {
         renderedStrings = records.map {$0.attributedString(scale: scale)}
      }
      if needsInitialLoad
      {
         fullReloadCount += 1
         table.reloadData()
         return
      }
      guard needsTextUpdate || needsWrapUpdate else {return}
      let visible = table.rows(in: table.visibleRect)
      guard visible.location != NSNotFound else {return}
      for row in visible.location..<min(visible.location + visible.length, records.count)
      {
         guard let cell = table.view(atColumn: 0, row: row, makeIfNecessary: false) as? AppKitTextTableCellView else {continue}
         configure(cell.label, row: row)
      }
   }

   func numberOfRows(in tableView: NSTableView) -> Int {records.count}

   func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView?
   {
      let identifier = NSUserInterfaceItemIdentifier("text.label")
      let cell = tableView.makeView(withIdentifier: identifier, owner: self) as? AppKitTextTableCellView ?? AppKitTextTableCellView()
      cell.identifier = identifier
      configure(cell.label, row: row)
      return cell
   }

   private func configure(_ field: NSTextField, row: Int)
   {
      guard row < records.count, row < renderedStrings.count else {return}
      field.frame = CGRect(x: 16, y: 0, width: wrapWidth, height: 72)
      field.attributedStringValue = renderedStrings[row]
      field.maximumNumberOfLines = 2
      field.setAccessibilityIdentifier(records[row].id)
      field.setAccessibilityLabel(records[row].value)
   }

   func teardown()
   {
      table.dataSource = nil
      table.delegate = nil
      scrollView.documentView = nil
      records.removeAll(keepingCapacity: false)
      renderedStrings.removeAll(keepingCapacity: false)
      storage.nativeControls.removeActionBindings()
   }
}

final class AppKitResizeProductionSceneRoot: NSObject, AppKitProductionSceneRoot
{
   static let sceneID = AppKitProductionSceneID.resizeTheme
   static let componentContracts = [
      AppKitProductionComponentContract(identifier: "resize.metrics", componentClass: NSCollectionView.self),
      AppKitProductionComponentContract(identifier: "resize.metrics-scroll", componentClass: NSScrollView.self),
   ]

   let storage = AppKitProductionSceneStorage(sceneID: .resizeTheme)
   let metrics: NSCollectionView
   let scrollView: NSScrollView
   let collectionController = AppKitDashboardCollectionController()
   private(set) var backdrops = [NSVisualEffectView]()

   override init()
   {
      storage.useDirectLayout()
      metrics = storage.makeCollection(identifier: "resize.metrics")
      metrics.backgroundColors = [.clear]
      metrics.isSelectable = false
      metrics.register(AppKitDashboardCardCollectionViewItem.self, forItemWithIdentifier: AppKitDashboardCardCollectionViewItem.reuseIdentifier)
      scrollView = NSScrollView(frame: CGRect(x: 16, y: 48, width: 358, height: 730))
      scrollView.identifier = NSUserInterfaceItemIdentifier("resize.metrics-scroll")
      scrollView.setAccessibilityIdentifier("resize.metrics-scroll")
      scrollView.borderType = .noBorder
      scrollView.drawsBackground = false
      scrollView.hasHorizontalScroller = false
      scrollView.hasVerticalScroller = false
      scrollView.contentView.drawsBackground = false
      scrollView.documentView = metrics
      backdrops = (0..<4).map
      {
         index in
         let effect = NSVisualEffectView(frame: CGRect(x: 16, y: 64 + CGFloat(index) * 204, width: 358, height: 96))
         effect.identifier = NSUserInterfaceItemIdentifier("resize.backdrop.\(index)")
         effect.material = .sidebar
         effect.blendingMode = .withinWindow
         effect.state = .active
         return effect
      }
      super.init()
      backdrops.forEach {storage.rootView.addSubview($0)}
      storage.rootView.addSubview(scrollView)
      collectionController.install(on: metrics)
   }

   func present(records: [AppKitDashboardCardRecord], orientation: String, theme: String)
   {
      let landscape = orientation == "landscape"
      let appearance = NSAppearance(named: theme == "dark" ? .darkAqua : .aqua)
      storage.rootView.appearance = appearance
      metrics.appearance = appearance
      scrollView.appearance = appearance
      scrollView.frame = landscape ? CGRect(x: 24, y: 36, width: 796, height: 318) : CGRect(x: 16, y: 48, width: 358, height: 730)
      for (index, backdrop) in backdrops.enumerated()
      {
         backdrop.frame = landscape
            ? CGRect(x: 24 + CGFloat(index % 2) * 398, y: 48 + CGFloat(index / 2) * 144, width: 386, height: 120)
            : CGRect(x: 16, y: 64 + CGFloat(index) * 204, width: 358, height: 96)
      }
      let layout = metrics.collectionViewLayout as! NSCollectionViewFlowLayout
      layout.itemSize = NSSize(width: landscape ? 190 : 173, height: 40)
      layout.minimumInteritemSpacing = 12
      layout.minimumLineSpacing = landscape ? 0 : 6
      layout.invalidateLayout()
      collectionController.replace(with: records, in: metrics)
   }

   func teardown()
   {
      collectionController.uninstall(from: metrics)
      scrollView.documentView = nil
      storage.nativeControls.removeActionBindings()
   }
}
