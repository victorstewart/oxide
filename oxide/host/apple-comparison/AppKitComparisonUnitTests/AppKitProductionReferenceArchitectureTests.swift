import AppKit
import CoreText
import XCTest

final class AppKitProductionReferenceArchitectureTests: XCTestCase
{
   func testAllSceneRootsExposeExactNativeHierarchyContracts()
   {
      let scenes: [any AppKitProductionSceneRoot] = [
         AppKitStartupProductionSceneRoot(),
         AppKitDashboardProductionSceneRoot(),
         AppKitEnduranceProductionSceneRoot(),
         AppKitFeedProductionSceneRoot(),
         AppKitChatProductionSceneRoot(),
         AppKitNavigationProductionSceneRoot(),
         AppKitImageProductionSceneRoot(),
         AppKitGridProductionSceneRoot(),
         AppKitEffectsProductionSceneRoot(),
         AppKitMutationProductionSceneRoot(),
         AppKitTextProductionSceneRoot(headingFont: NSFont.systemFont(ofSize: 20)),
         AppKitResizeProductionSceneRoot(),
      ]
      let expectedClasses: [AppKitProductionSceneID: [String: NSView.Type]] = [
         .startupFirstScreen: [
            "startup.header": NSTextField.self,
            "startup.navigation": AppKitProductionRoundedSurfaceView.self,
            "startup.cards-scroll": NSScrollView.self,
            "startup.cards-clip": NSClipView.self,
            "startup.cards": NSCollectionView.self,
            "startup.primary-action": NSButton.self,
            "startup.primary-label": NSTextField.self,
            "startup.launch-probe-response": AppKitProductionSurfaceView.self,
         ],
         .dashboardMixedStatic: [
            "dashboard.metrics": NSCollectionView.self,
            "dashboard.metrics-scroll": NSScrollView.self,
         ],
         .enduranceChurn: [
            "endurance.metrics": NSCollectionView.self,
            "endurance.metrics-scroll": NSScrollView.self,
         ],
         .feedVariableScroll: [
            "feed.navigation-bar": AppKitProductionSurfaceView.self,
            "feed.scroll": NSScrollView.self,
            "feed.clip": NSClipView.self,
            "feed.table": NSTableView.self,
            "feed.navigation-title": NSTextField.self,
         ],
         .chatLiveUpdate: [
            "chat.thread-scroll": NSScrollView.self,
            "chat.thread-clip": NSClipView.self,
            "chat.thread": NSTableView.self,
            "chat.composer-scroll": NSScrollView.self,
            "chat.composer-clip": NSClipView.self,
            "chat.composer": AppKitOwnedTextView.self,
            "chat.send": NSButton.self,
         ],
         .navigationModal: [
            "navigation.heading": NSTextField.self,
            "navigation.scroll": NSScrollView.self,
            "navigation.clip": NSClipView.self,
            "navigation.table": NSTableView.self,
            "navigation.detail": AppKitNavigationDetailView.self,
            "navigation.back": NSButton.self,
            "navigation.modal-action": NSButton.self,
            "navigation.overlay": AppKitNavigationOverlayView.self,
            "navigation.modal": AppKitNavigationModalView.self,
            "navigation.dismiss": NSButton.self,
         ],
         .imageDecodeZoom: [
            "image.scroll": NSScrollView.self,
            "image.clip": NSClipView.self,
            "image.canvas": NSImageView.self,
            "image.zoom": NSSlider.self,
         ],
         .gridLargeScroll: [
            "grid.scroll": NSScrollView.self,
            "grid.collection": NSCollectionView.self,
            "grid.detail": NSView.self,
            "grid.back": NSButton.self,
         ],
         .effectsLayers: ["effects.surface": NSView.self],
         .mutationDamage: ["mutation.surface": NSView.self],
         .textMultilingual: ["text.scroll": NSScrollView.self, "text.table": NSTableView.self],
         .resizeTheme: ["resize.metrics": NSCollectionView.self, "resize.metrics-scroll": NSScrollView.self],
      ]

      XCTAssertEqual(Set(scenes.map {type(of: $0).sceneID}), Set(AppKitProductionSceneID.allCases))
      XCTAssertEqual(Set(scenes.map {ObjectIdentifier(type(of: $0))}).count, 12)
      for scene in scenes
      {
         let expected = expectedClasses[type(of: scene).sceneID] ?? [:]
         let declared = Dictionary(uniqueKeysWithValues: type(of: scene).componentContracts.map {($0.identifier, $0.componentClass)})
         XCTAssertEqual(Set(declared.keys), Set(expected.keys), type(of: scene).sceneID.rawValue)
         XCTAssertTrue(scene.unsatisfiedComponentContracts().isEmpty, type(of: scene).sceneID.rawValue)
         for (identifier, expectedClass) in expected
         {
            guard let component = scene.component(for: AppKitProductionComponentContract(identifier: identifier, componentClass: expectedClass)) else
            {
               XCTFail("missing \(identifier)")
               continue
            }
            XCTAssertTrue(declared[identifier] == expectedClass, identifier)
            XCTAssertTrue(component.isKind(of: expectedClass), identifier)
            let initiallyHidden = type(of: scene).sceneID == .navigationModal && ["navigation.detail", "navigation.overlay", "navigation.modal"].contains(identifier)
               || type(of: scene).sceneID == .startupFirstScreen && identifier == "startup.launch-probe-response"
               || type(of: scene).sceneID == .gridLargeScroll && identifier == "grid.detail"
            if initiallyHidden
            {
               XCTAssertTrue(component.isHidden, identifier)
            }
            else
            {
               XCTAssertFalse(component.isHidden, identifier)
            }
            XCTAssertTrue(component.isDescendant(of: scene.rootView), identifier)
         }
         let authoredViews = descendants(of: scene.rootView).filter
         {
            !$0.accessibilityIdentifier().isEmpty || $0.identifier != nil
         }
         let hiddenIdentifiers = Set(authoredViews.filter(\.isHidden).compactMap {$0.identifier?.rawValue})
         if type(of: scene).sceneID == .navigationModal
         {
            XCTAssertEqual(hiddenIdentifiers, ["navigation.detail", "navigation.overlay", "navigation.modal"])
         }
         else if type(of: scene).sceneID == .startupFirstScreen
         {
            XCTAssertEqual(hiddenIdentifiers, ["startup.launch-probe-response"])
         }
         else if type(of: scene).sceneID == .gridLargeScroll
         {
            XCTAssertEqual(hiddenIdentifiers, ["grid.detail"])
         }
         else
         {
            XCTAssertTrue(hiddenIdentifiers.isEmpty, type(of: scene).sceneID.rawValue)
         }
      }
   }

   func testScrollingScenesUseTheirRequiredDocumentViewArchitecture()
   {
      let feed = AppKitFeedProductionSceneRoot()
      XCTAssertTrue(feed.scrollView.contentView.isKind(of: NSClipView.self))
      XCTAssertTrue(feed.scrollView.documentView === feed.table)
      XCTAssertFalse(feed.table.isKind(of: NSCollectionView.self))

      let chat = AppKitChatProductionSceneRoot()
      XCTAssertTrue(chat.threadScrollView.contentView.isKind(of: NSClipView.self))
      XCTAssertTrue(chat.threadScrollView.documentView === chat.thread)
      XCTAssertTrue(chat.composerScrollView.contentView.isKind(of: NSClipView.self))
      XCTAssertTrue(chat.composerScrollView.documentView === chat.composer)

      let navigation = AppKitNavigationProductionSceneRoot()
      XCTAssertTrue(navigation.scrollView.contentView.isKind(of: NSClipView.self))
      XCTAssertTrue(navigation.scrollView.documentView === navigation.table)

      let image = AppKitImageProductionSceneRoot()
      XCTAssertTrue(image.scrollView.contentView.isKind(of: NSClipView.self))
      XCTAssertTrue(image.scrollView.documentView === image.imageView)
   }

   func testDashboardUsesFrozenGridAndNativeLeafGeometry()
   {
      let dashboard = AppKitDashboardProductionSceneRoot()
      guard let layout = dashboard.metrics.collectionViewLayout as? NSCollectionViewFlowLayout else
      {
         return XCTFail("dashboard must use native flow layout")
      }
      XCTAssertEqual(dashboard.scrollView.frame, CGRect(x: 16, y: 48, width: 358, height: 730))
      XCTAssertEqual(layout.itemSize, NSSize(width: 173, height: 40))
      XCTAssertEqual(layout.minimumInteritemSpacing, 12)
      XCTAssertEqual(layout.minimumLineSpacing, 6)
      XCTAssertEqual(dashboard.backdropViews.map(\.frame), [
         CGRect(x: 16, y: 64, width: 358, height: 96),
         CGRect(x: 16, y: 268, width: 358, height: 96),
         CGRect(x: 16, y: 472, width: 358, height: 96),
         CGRect(x: 16, y: 676, width: 358, height: 96),
      ])

      let labels = (0..<6).map {AppKitDashboardLabelRecord(id: String(format: "dashboard:label:%03d", $0), value: "Node \($0)", accent: false)}
      let seed = dashboardRecord(labels: labels)
      let item = AppKitDashboardCardCollectionViewItem()
      item.configure(record: seed, target: ReuseActionTarget.shared, action: #selector(ReuseActionTarget.invoke), row: 0)
      XCTAssertEqual(item.view.frame.size, NSSize(width: 173, height: 40))
      XCTAssertEqual(item.leadingIconView.frame, CGRect(x: 6, y: 6, width: 14, height: 14))
      XCTAssertEqual(item.trailingIconView.frame, CGRect(x: 24, y: 6, width: 14, height: 14))
      XCTAssertEqual(item.labelFields.count, 6)
      XCTAssertEqual(item.labelFields[0].frame, CGRect(x: 42, y: 2 - 8.0 / 3.0, width: 125, height: 11))
      XCTAssertEqual(item.labelFields[5].frame, CGRect(x: 42, y: 2 - 8.0 / 3.0 + 30, width: 125, height: 11))
      XCTAssertEqual(item.actionButton?.frame, CGRect(x: 145, y: 13, width: 18, height: 12))
      XCTAssertTrue(item.labelFields.allSatisfy {$0.shapePreparationCount == 1})
      item.configure(record: seed, target: ReuseActionTarget.shared, action: #selector(ReuseActionTarget.invoke), row: 0)
      XCTAssertTrue(item.labelFields.allSatisfy {$0.shapePreparationCount == 1})
   }

   func testFeedUsesFrozenViewportAndNativeReusableLeafGeometry()
   {
      let feed = AppKitFeedProductionSceneRoot(headingFont: NSFont.systemFont(ofSize: 20))
      XCTAssertEqual(feed.navigationBar.frame, CGRect(x: 0, y: 0, width: 390, height: 52))
      XCTAssertEqual(feed.navigationTitle.frame, CGRect(x: 16, y: 12, width: 220, height: 28))
      XCTAssertEqual(feed.navigationTitle.stringValue, "Measured Feed")
      XCTAssertEqual(feed.scrollView.frame, CGRect(x: 0, y: 52, width: 390, height: 792))
      XCTAssertEqual(feed.table.tableColumns[0].width, 390)
      XCTAssertEqual(feed.table.backgroundColor, AppKitProductionPalette.background)
      XCTAssertEqual(feed.scrollView.contentView.backgroundColor, AppKitProductionPalette.background)
      XCTAssertFalse(feed.scrollView.hasVerticalScroller)

      let cell = AppKitFeedTableCellView(frame: CGRect(x: 0, y: 0, width: 390, height: 68))
      let record = feedRecord(height: 68, favorite: true)
      cell.configure(record: record, target: ReuseActionTarget.shared, action: #selector(ReuseActionTarget.invoke), row: 0)
      cell.layoutSubtreeIfNeeded()
      XCTAssertEqual(cell.thumbnailView.frame, CGRect(x: 22, y: 14, width: 48, height: 48))
      XCTAssertEqual(cell.titleField.frame, CGRect(x: 82, y: 12, width: 254, height: 24))
      XCTAssertEqual(cell.secondaryField.frame, CGRect(x: 82, y: 38, width: 254, height: 18))
      XCTAssertEqual(cell.favoriteButton.frame, CGRect(x: 344, y: 15, width: 18, height: 18))
      XCTAssertTrue(cell.thumbnailView.image === record.thumbnail)
      XCTAssertEqual(cell.secondaryField.stringValue, record.id)
      XCTAssertEqual(cell.favoriteButton.state, .on)
      XCTAssertTrue(cell.thumbnailView.isKind(of: NSImageView.self))
      XCTAssertTrue(cell.titleField.isKind(of: NSTextField.self))
      XCTAssertTrue(cell.favoriteButton.isKind(of: NSButton.self))
   }

   func testImageSceneUsesFrozenDirectLayoutAndNativeZoomAction()
   {
      let image = AppKitImageProductionSceneRoot()
      XCTAssertEqual(image.heading.frame, CGRect(x: 18, y: 8, width: 204, height: 36))
      XCTAssertEqual(image.heading.stringValue, "Decode & Zoom")
      XCTAssertEqual(image.scrollView.frame, CGRect(x: 0, y: 52, width: 390, height: 740))
      XCTAssertFalse(image.scrollView.hasHorizontalScroller)
      XCTAssertFalse(image.scrollView.hasVerticalScroller)
      XCTAssertTrue(image.scrollView.documentView === image.imageView)
      XCTAssertEqual(image.imageView.frame, CGRect(x: 0, y: 0, width: 390, height: 740))
      XCTAssertTrue(image.imageView.isKind(of: NSImageView.self))
      XCTAssertEqual(image.zoomSlider.frame, CGRect(x: 16, y: 800, width: 358, height: 28))
      XCTAssertTrue(image.zoomSlider.isKind(of: NSSlider.self))
      XCTAssertEqual(image.zoomSlider.minValue, 1)
      XCTAssertEqual(image.zoomSlider.maxValue, 2)
      var observedScale: Double?
      image.onZoom = {observedScale = $0}
      image.zoomSlider.doubleValue = 2
      XCTAssertTrue(image.zoomSlider.sendAction(image.zoomSlider.action, to: image.zoomSlider.target))
      XCTAssertEqual(observedScale, 2)
   }

   func testExactTextFieldKeepsCenteredGlyphInkInsideItsNativeBounds()
   {
      let field = AppKitProductionExactTextField(frame: CGRect(x: 0, y: 0, width: 220, height: 28))
      field.configure(
         identifier: "text.capture",
         value: "Measured Feed",
         font: NSFont.systemFont(ofSize: 20),
         color: AppKitProductionPalette.text
      )
      guard let bitmap = field.bitmapImageRepForCachingDisplay(in: field.bounds) else
      {
         return XCTFail("missing exact text bitmap")
      }
      field.cacheDisplay(in: field.bounds, to: bitmap)
      var inkRows = IndexSet()
      for y in 0..<bitmap.pixelsHigh
      {
         for x in 0..<bitmap.pixelsWide where (bitmap.colorAt(x: x, y: y)?.alphaComponent ?? 0) > 0
         {
            inkRows.insert(y)
         }
      }
      XCTAssertFalse(inkRows.isEmpty)
      XCTAssertGreaterThan(inkRows.first ?? 0, 0)
      XCTAssertLessThan(inkRows.last ?? bitmap.pixelsHigh, bitmap.pixelsHigh - 1)
   }

   func testEditableChatMessageFieldKeepsTheExactStaticRaster() throws
   {
      let frame = CGRect(x: 0, y: 0, width: 266, height: 54)
      let font = NSFont.systemFont(ofSize: 15)
      let visible = "Stable frames keep conversation"
      let full = "Stable frames keep conversation feeling immediate."
      let reference = AppKitProductionExactTextField(frame: frame)
      reference.configure(identifier: "reference", value: visible, font: font, color: AppKitProductionPalette.text)
      let editable = AppKitChatEditableMessageField(frame: frame)
      editable.configureMessage(
         identifier: "chat:append:16",
         fullValue: full,
         visibleValue: visible,
         font: font,
         color: AppKitProductionPalette.text,
         inlineText: nil,
         editable: true
      )

      func png(_ field: NSTextField) throws -> Data
      {
         let bitmap = try XCTUnwrap(field.bitmapImageRepForCachingDisplay(in: field.bounds))
         field.cacheDisplay(in: field.bounds, to: bitmap)
         return try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
      }

      XCTAssertEqual(try png(editable), try png(reference))
      XCTAssertEqual(editable.identifier?.rawValue, "chat:append:16")
      XCTAssertEqual(editable.stringValue, full)
      XCTAssertTrue(editable.isEditable)
      XCTAssertTrue(editable.isSelectable)
   }

   func testExactTextFieldHonorsHorizontalCenterAlignment()
   {
      let font = NSFont.systemFont(ofSize: 13)
      let value = "Done"
      func inkColumns(alignment: NSTextAlignment) -> (columns: IndexSet, pixelWidth: Int)
      {
         let field = AppKitProductionExactTextField(frame: CGRect(x: 0, y: 0, width: 50, height: 28))
         field.configure(identifier: "text.alignment", value: value, font: font, color: AppKitProductionPalette.surface)
         field.alignment = alignment
         guard let bitmap = field.bitmapImageRepForCachingDisplay(in: field.bounds) else {return ([], 0)}
         field.cacheDisplay(in: field.bounds, to: bitmap)
         var columns = IndexSet()
         for y in 0..<bitmap.pixelsHigh
         {
            for x in 0..<bitmap.pixelsWide where (bitmap.colorAt(x: x, y: y)?.alphaComponent ?? 0) > 0
            {
               columns.insert(x)
            }
         }
         return (columns, bitmap.pixelsWide)
      }
      let left = inkColumns(alignment: .left)
      let centered = inkColumns(alignment: .center)
      let line = CTLineCreateWithAttributedString(NSAttributedString(
         string: value,
         attributes: [kCTFontAttributeName as NSAttributedString.Key: font]
      ))
      let expectedOffset = Int(((50 - CTLineGetTypographicBounds(line, nil, nil, nil)) * 0.5 * Double(centered.pixelWidth) / 50).rounded())
      XCTAssertFalse(left.columns.isEmpty)
      XCTAssertFalse(centered.columns.isEmpty)
      XCTAssertEqual((centered.columns.first ?? 0) - (left.columns.first ?? 0), expectedOffset, accuracy: 1)
   }

   func testFeedIncrementalMutationsAvoidFullReloadsAndLinearComparisons()
   {
      let table = FeedReloadTrackingTableView(frame: .zero)
      table.addTableColumn(NSTableColumn(identifier: NSUserInterfaceItemIdentifier("feed.content")))
      let controller = AppKitFeedTableController()
      controller.install(on: table)
      let records = [feedRecord(height: 68, favorite: false)]
      let initialReloadCount = table.reloadDataCount
      controller.replace(with: records, in: table)
      XCTAssertEqual(table.reloadDataCount, initialReloadCount + 1)
      XCTAssertEqual(controller.fullReloadCount, 1)

      XCTAssertTrue(controller.update(id: "feed:0", record: feedRecord(height: 68, favorite: true), in: table))
      XCTAssertEqual(table.reloadDataCount, initialReloadCount + 1)
      XCTAssertEqual(controller.incrementalMutationCount, 1)
      XCTAssertEqual(controller.linearComparisonCount, 0)

      controller.prepend([feedRecord(id: "feed:prepend:0", height: 85, favorite: false)], in: table)
      XCTAssertEqual(table.reloadDataCount, initialReloadCount + 1)
      XCTAssertEqual(table.insertedRowCount, 1)
      XCTAssertEqual(controller.incrementalMutationCount, 2)
      XCTAssertEqual(controller.linearComparisonCount, 0)

      controller.uninstall(from: table)
      XCTAssertEqual(table.reloadDataCount, initialReloadCount + 2)
   }

   func testChatAndNavigationUpdatesAvoidRepeatedFullReloads()
   {
      let chatTable = FeedReloadTrackingTableView(frame: .zero)
      chatTable.addTableColumn(NSTableColumn(identifier: NSUserInterfaceItemIdentifier("chat.content")))
      let chat = AppKitChatTableController()
      chat.install(on: chatTable)
      chat.replace(with: [chatRecord(id: "chat:0")], in: chatTable)
      let initialChatReloads = chatTable.reloadDataCount
      chat.append(chatRecord(id: "chat:1"), in: chatTable)
      chat.prepend([chatRecord(id: "chat:prepend:0")], in: chatTable)
      XCTAssertTrue(chat.update(id: "chat:0", record: chatRecord(id: "chat:0", text: "Updated"), in: chatTable))
      XCTAssertEqual(chatTable.reloadDataCount, initialChatReloads)
      XCTAssertEqual(chatTable.insertedRowCount, 2)
      XCTAssertEqual(chat.fullReloadCount, 1)
      XCTAssertEqual(chat.incrementalMutationCount, 3)
      XCTAssertEqual(chat.linearComparisonCount, 0)

      let navigationTable = FeedReloadTrackingTableView(frame: .zero)
      navigationTable.addTableColumn(NSTableColumn(identifier: NSUserInterfaceItemIdentifier("navigation.content")))
      let navigation = AppKitNavigationTableController()
      navigation.install(on: navigationTable)
      let destinations = [navigationRecord()]
      navigation.replace(with: destinations, in: navigationTable)
      let initialNavigationReloads = navigationTable.reloadDataCount
      navigation.replace(with: destinations, in: navigationTable)
      XCTAssertEqual(navigationTable.reloadDataCount, initialNavigationReloads)
      XCTAssertEqual(navigation.fullReloadCount, 1)
      XCTAssertEqual(navigation.linearComparisonCount, 0)
   }

   func testFeedMaterializesNativeRowsInAttachedViewport()
   {
      _ = NSApplication.shared
      let window = NSWindow(
         contentRect: NSRect(x: 0, y: 0, width: 390, height: 844),
         styleMask: [.borderless],
         backing: .buffered,
         defer: false
      )
      window.isReleasedWhenClosed = false
      let feed = AppKitFeedProductionSceneRoot()
      let host = AppKitProductionSceneHost()
      host.install(feed)
      window.contentViewController = host
      window.setContentSize(NSSize(width: 390, height: 844))
      window.makeKeyAndOrderFront(nil)
      window.contentView?.layoutSubtreeIfNeeded()
      host.view.layoutSubtreeIfNeeded()
      feed.replaceRows((0..<20).map {feedRecord(id: "feed:\($0)", height: 68, favorite: false)})
      feed.applyScrollOffset(0)
      feed.rootView.displayIfNeeded()
      let hierarchy = "window=\(String(describing: feed.table.window)) host=\(host.view.frame) root=\(feed.rootView.frame) scroll=\(feed.scrollView.frame)/\(feed.scrollView.visibleRect) clip=\(feed.scrollView.contentView.frame)/\(feed.scrollView.contentView.bounds)/\(feed.scrollView.contentView.visibleRect)/\(feed.scrollView.contentView.documentVisibleRect) table=\(feed.table.frame)/\(feed.table.bounds)/\(feed.table.visibleRect) attached=\(feed.table.superview === feed.scrollView.contentView) hidden=\(feed.table.isHidden)"
      XCTAssertEqual(feed.scrollView.frame, CGRect(x: 0, y: 52, width: 390, height: 792))
      XCTAssertEqual(host.view.frame.size, NSSize(width: 390, height: 844))
      XCTAssertEqual(feed.rootView.frame.size, NSSize(width: 390, height: 844))
      XCTAssertEqual(feed.table.numberOfRows, 20)
      XCTAssertEqual(feed.table.frame, CGRect(x: 0, y: 0, width: 390, height: 1_360))
      XCTAssertGreaterThan(feed.table.frame.height, feed.scrollView.contentView.bounds.height)
      XCTAssertFalse(feed.table.visibleRect.isEmpty, hierarchy)
      XCTAssertGreaterThan(feed.table.rows(in: feed.table.visibleRect).length, 0, hierarchy)
      XCTAssertTrue(feed.table.view(atColumn: 0, row: 0, makeIfNecessary: false) is AppKitFeedTableCellView)
      host.uninstall()
      window.close()
   }

   func testDashboardLeafMutationRebindsVisibleNativeItemInPlace()
   {
      _ = NSApplication.shared
      let window = NSWindow(
         contentRect: NSRect(x: 0, y: 0, width: 390, height: 844),
         styleMask: [.borderless],
         backing: .buffered,
         defer: false
      )
      window.isReleasedWhenClosed = false
      let dashboard = AppKitDashboardProductionSceneRoot()
      window.contentView = dashboard.rootView
      dashboard.replaceMetrics([dashboardRecord()])
      dashboard.rootView.layoutSubtreeIfNeeded()
      dashboard.rootView.displayIfNeeded()
      guard let initial = dashboard.metrics.item(at: IndexPath(item: 0, section: 0)) as? AppKitDashboardCardCollectionViewItem else
      {
         return XCTFail("missing visible dashboard item")
      }
      let changed = AppKitDashboardLabelRecord(id: "dashboard:label:000", value: "Updated 1", accent: false)
      dashboard.replaceMetrics([dashboardRecord(labels: [changed])])
      guard let rebound = dashboard.metrics.item(at: IndexPath(item: 0, section: 0)) as? AppKitDashboardCardCollectionViewItem else
      {
         return XCTFail("missing rebound dashboard item")
      }
      XCTAssertTrue(initial === rebound)
      XCTAssertEqual(rebound.labelFields[0].stringValue, "Updated 1")
      XCTAssertEqual(rebound.reuseCounters.bindingCount, 2)
      XCTAssertEqual(rebound.reuseCounters.preparationCount, 0)
      XCTAssertEqual(rebound.reuseCounters.cleanupCount, 0)
      dashboard.teardown()
      window.close()
   }

   func testDashboardSameCountUpdatesAvoidCollectionReloads()
   {
      let collection = DashboardReloadTrackingCollectionView(frame: .zero)
      collection.collectionViewLayout = NSCollectionViewFlowLayout()
      collection.register(AppKitDashboardCardCollectionViewItem.self, forItemWithIdentifier: AppKitDashboardCardCollectionViewItem.reuseIdentifier)
      let controller = AppKitDashboardCollectionController()
      controller.install(on: collection)
      let initialReloadCount = collection.reloadDataCount
      controller.replace(with: [dashboardRecord()], in: collection)
      XCTAssertEqual(collection.reloadDataCount, initialReloadCount + 1)
      XCTAssertEqual(collection.reloadItemsCount, 0)

      controller.replace(with: [dashboardRecord()], in: collection)
      XCTAssertEqual(collection.reloadDataCount, initialReloadCount + 1)
      XCTAssertEqual(collection.reloadItemsCount, 0)

      let changed = AppKitDashboardLabelRecord(id: "dashboard:label:000", value: "Updated 1", accent: false)
      controller.replace(with: [dashboardRecord(labels: [changed])], in: collection)
      XCTAssertEqual(collection.reloadDataCount, initialReloadCount + 1)
      XCTAssertEqual(collection.reloadItemsCount, 0)

      controller.onAction = {_ in}
      controller.uninstall(from: collection)
      XCTAssertEqual(collection.reloadDataCount, initialReloadCount + 2)
      XCTAssertEqual(collection.reloadItemsCount, 0)
      XCTAssertNil(controller.onAction)
      XCTAssertNil(collection.dataSource)
      XCTAssertNil(collection.delegate)
   }

   func testReusableComponentContractsPutControlsInRowsAndItems()
   {
      XCTAssertTrue(AppKitStartupProductionSceneRoot.reusableComponentContracts[0].componentClass === AppKitStartupCardCollectionViewItem.self)
      XCTAssertTrue(AppKitDashboardProductionSceneRoot.reusableComponentContracts[0].componentClass === AppKitDashboardCardCollectionViewItem.self)
      XCTAssertTrue(AppKitFeedProductionSceneRoot.reusableComponentContracts[0].componentClass === AppKitFeedTableCellView.self)
      XCTAssertTrue(AppKitChatProductionSceneRoot.reusableComponentContracts[0].componentClass === AppKitChatTableCellView.self)
      XCTAssertTrue(AppKitNavigationProductionSceneRoot.reusableComponentContracts[0].componentClass === AppKitNavigationTableCellView.self)

      let dashboardItem = AppKitDashboardCardCollectionViewItem()
      _ = dashboardItem.view
      dashboardItem.configure(record: dashboardRecord(), target: ReuseActionTarget.shared, action: #selector(ReuseActionTarget.invoke), row: 0)
      XCTAssertTrue(dashboardItem.actionButton?.isDescendant(of: dashboardItem.view) == true)
      XCTAssertFalse(dashboardItem.actionButton?.isHidden ?? true)
      XCTAssertEqual(dashboardItem.labelFields.count, 1)
      XCTAssertTrue(dashboardItem.labelFields[0].isKind(of: NSTextField.self))
      XCTAssertTrue(dashboardItem.leadingIconView.isKind(of: NSImageView.self))

      let feedRow = AppKitFeedTableCellView(frame: .zero)
      XCTAssertTrue(feedRow.favoriteButton.isDescendant(of: feedRow))
      XCTAssertFalse(feedRow.favoriteButton.isHidden)

      let navigationRow = AppKitNavigationTableCellView(frame: .zero)
      XCTAssertTrue(navigationRow.destinationButton.isDescendant(of: navigationRow))
      XCTAssertFalse(navigationRow.destinationButton.isHidden)
   }

   func testSceneRootsUseOnlyWorkloadOwnedInteractiveControls()
   {
      let dashboard = AppKitDashboardProductionSceneRoot()
      let feed = AppKitFeedProductionSceneRoot()
      let navigation = AppKitNavigationProductionSceneRoot()
      XCTAssertTrue(dashboard.nativeControls.buttons.isEmpty)
      XCTAssertTrue(feed.nativeControls.buttons.isEmpty)
      XCTAssertTrue(navigation.nativeControls.buttons.isEmpty)
      XCTAssertFalse(descendants(of: dashboard.rootView).contains(where: {$0 is NSButton}))
      XCTAssertFalse(descendants(of: feed.rootView).contains(where: {$0 is NSButton}))
      XCTAssertEqual(
         descendants(of: navigation.rootView).compactMap {$0 as? NSButton},
         [navigation.detailView.backButton, navigation.detailView.modalButton, navigation.modalView.dismissButton]
      )
   }

   func testNavigationDismissLabelSharesTheButtonGeometry()
   {
      let navigation = AppKitNavigationProductionSceneRoot()
      navigation.modalView.layoutSubtreeIfNeeded()
      XCTAssertEqual(navigation.modalView.dismissField.frame, navigation.modalView.dismissButton.frame)
   }

   func testNavigationModalTransitionOriginStaysOnCanonicalThreeXPixelGrid()
   {
      let navigation = AppKitNavigationProductionSceneRoot()
      let checkpoints: [(progress: CGFloat, expectedX: CGFloat)] = [(0.15625, 353.0), (0.5, 219.0), (0.84375, 85.0), (1.0, 24.0)]
      for (progress, expectedX) in checkpoints
      {
         navigation.present(route: "detail", modalProgress: progress, modalVisible: true)
         XCTAssertEqual(navigation.modalView.frame.minX, expectedX, accuracy: 0.000_001)
         XCTAssertEqual((navigation.modalView.frame.minX * 3).truncatingRemainder(dividingBy: 1), 0, accuracy: 0.000_001)
      }
   }

   func testNavigationModalBottomShadowUsesMetalEquivalentOpaqueSRGB8Colors()
   {
      let checkpoints: [(progress: CGFloat, expected: [Int])] = [
         (0.15625, [214, 216, 219]),
         (0.5, [187, 189, 191]),
         (0.84375, [154, 155, 158]),
         (1.0, [134, 136, 139]),
      ]
      for (progress, expected) in checkpoints
      {
         let color = appKitProductionModalBottomShadowColor(progress: progress).usingColorSpace(.sRGB)
         XCTAssertNotNil(color)
         XCTAssertEqual(Int(((color?.redComponent ?? 0) * 255).rounded()), expected[0])
         XCTAssertEqual(Int(((color?.greenComponent ?? 0) * 255).rounded()), expected[1])
         XCTAssertEqual(Int(((color?.blueComponent ?? 0) * 255).rounded()), expected[2])
         XCTAssertEqual(color?.alphaComponent, 1)
      }
   }

   func testNavigationDetailBottomShadowUsesMetalEquivalentOpaqueSRGB8Color()
   {
      let color = appKitProductionDetailBottomShadowColor().usingColorSpace(.sRGB)
      XCTAssertNotNil(color)
      XCTAssertEqual(Int(((color?.redComponent ?? 0) * 255).rounded()), 225)
      XCTAssertEqual(Int(((color?.greenComponent ?? 0) * 255).rounded()), 227)
      XCTAssertEqual(Int(((color?.blueComponent ?? 0) * 255).rounded()), 230)
      XCTAssertEqual(color?.alphaComponent, 1)
   }

   func testHostOwnsExactlyOneInstalledNativeSceneHierarchy()
   {
      let host = AppKitProductionSceneHost()
      let startup = AppKitStartupProductionSceneRoot()
      let chat = AppKitChatProductionSceneRoot()

      host.install(startup)
      XCTAssertTrue(host.activeScene?.rootView === startup.rootView)
      XCTAssertTrue(startup.rootView.superview === host.view)
      host.install(chat)
      XCTAssertNil(startup.rootView.superview)
      XCTAssertTrue(host.activeScene?.rootView === chat.rootView)
      XCTAssertEqual(host.view.subviews, [chat.rootView])
      host.uninstall()
      XCTAssertNil(host.activeScene)
      XCTAssertTrue(host.view.subviews.isEmpty)
   }

   func testChatComposerOwnsSelectionAndInsertText()
   {
      let scene = AppKitChatProductionSceneRoot()
      XCTAssertEqual(scene.nativeControls.textViews.count, 1)
      XCTAssertTrue(scene.nativeControls.textViews[0] === scene.composer)

      scene.composer.setSelectedRange(NSRange(location: 0, length: 0))
      scene.composer.insertText("hello", replacementRange: NSRange(location: 0, length: 0))
      XCTAssertEqual(scene.composer.string, "hello")
      XCTAssertEqual(scene.composer.insertionCount, 1)
      XCTAssertEqual(scene.composer.lastInsertedText, "hello")
      XCTAssertEqual(scene.composer.ownedSelectionRange, NSRange(location: 5, length: 0))
   }

   func testReusableCollectionAndTableCellsCountAndCleanReuse()
   {
      let collectionItem = CollectionReuseProbeItem()
      _ = collectionItem.view
      configureStaleControls(collectionItem.button, collectionItem.field, collectionItem.slider)
      collectionItem.bind(representedIdentifier: "card-1")
      collectionItem.prepareForReuse()
      XCTAssertEqual(collectionItem.reuseCounters, AppKitCollectionItemReuseCounters(bindingCount: 1, preparationCount: 1, cleanupCount: 1))
      assertControlsWereCleaned(collectionItem.button, collectionItem.field, collectionItem.slider)

      let tableCell = TableReuseProbeCell(frame: .zero)
      configureStaleControls(tableCell.button, tableCell.field, tableCell.slider)
      tableCell.bind(representedIdentifier: "row-1")
      tableCell.prepareForReuse()
      XCTAssertEqual(tableCell.reuseCounters, AppKitTableCellReuseCounters(bindingCount: 1, preparationCount: 1, cleanupCount: 1))
      XCTAssertNil(tableCell.representedIdentifier)
      assertControlsWereCleaned(tableCell.button, tableCell.field, tableCell.slider)
      XCTAssertEqual(tableCell.cleanupHookCount, 1)
   }

   func testSceneRootsOwnDataSourcesAndBindReusableContent()
   {
      let startup = AppKitStartupProductionSceneRoot()
      startup.replaceCards([
         AppKitStartupCardRecord(id: "startup:card:00", title: "Card 00", thumbnail: nil),
         AppKitStartupCardRecord(id: "startup:card:01", title: "Card 01", thumbnail: nil),
      ])
      XCTAssertTrue(startup.cards.dataSource === startup.collectionController)
      XCTAssertTrue(startup.cards.delegate === startup.collectionController)
      XCTAssertEqual(startup.collectionController.collectionView(startup.cards, numberOfItemsInSection: 0), 2)
      let startupItem = startup.collectionController.collectionView(startup.cards, itemForRepresentedObjectAt: IndexPath(item: 0, section: 0)) as? AppKitStartupCardCollectionViewItem
      XCTAssertEqual(startupItem?.representedIdentifier, "startup:card:00")
      XCTAssertEqual(startupItem?.titleField.stringValue, "Card 00")

      let dashboard = AppKitDashboardProductionSceneRoot()
      dashboard.replaceMetrics([dashboardRecord()])
      XCTAssertTrue(dashboard.metrics.dataSource === dashboard.collectionController)
      let dashboardItem = dashboard.collectionController.collectionView(dashboard.metrics, itemForRepresentedObjectAt: IndexPath(item: 0, section: 0)) as? AppKitDashboardCardCollectionViewItem
      XCTAssertEqual(dashboardItem?.representedIdentifier, "dashboard:card:00")
      XCTAssertEqual(dashboardItem?.labelFields[0].stringValue, "42")
      XCTAssertNotNil(dashboardItem?.actionButton)

      let feed = AppKitFeedProductionSceneRoot()
      feed.replaceRows([feedRecord(height: 76, favorite: true)])
      XCTAssertTrue(feed.table.dataSource === feed.tableController)
      XCTAssertTrue(feed.table.delegate === feed.tableController)
      XCTAssertEqual(feed.tableController.numberOfRows(in: feed.table), 1)
      XCTAssertEqual(feed.tableController.tableView(feed.table, heightOfRow: 0), 76)
      let feedCell = feed.tableController.tableView(feed.table, viewFor: feed.table.tableColumns[0], row: 0) as? AppKitFeedTableCellView
      XCTAssertEqual(feedCell?.representedIdentifier, "feed:0")
      XCTAssertEqual(feedCell?.titleField.stringValue, "First")
      XCTAssertEqual(feedCell?.favoriteButton.state, .on)

      let chat = AppKitChatProductionSceneRoot()
      chat.replaceMessages([chatRecord()])
      XCTAssertTrue(chat.thread.dataSource === chat.tableController)
      XCTAssertEqual(chat.tableController.numberOfRows(in: chat.thread), 1)
      let chatCell = chat.tableController.tableView(chat.thread, viewFor: chat.thread.tableColumns[0], row: 0) as? AppKitChatTableCellView
      XCTAssertEqual(chatCell?.representedIdentifier, "chat:0")
      XCTAssertEqual(chatCell?.messageFields[0].stringValue, "Hello")

      let navigation = AppKitNavigationProductionSceneRoot()
      navigation.replaceDestinations([navigationRecord()])
      XCTAssertTrue(navigation.table.dataSource === navigation.tableController)
      XCTAssertEqual(navigation.tableController.numberOfRows(in: navigation.table), 1)
      let navigationCell = navigation.tableController.tableView(navigation.table, viewFor: navigation.table.tableColumns[0], row: 0) as? AppKitNavigationTableCellView
      XCTAssertEqual(navigationCell?.representedIdentifier, "destination:0")
      XCTAssertEqual(navigationCell?.titleField.stringValue, "Detail")
      XCTAssertEqual(navigationCell?.destinationButton.title, "")
   }

   func testNativeTargetActionsAndTeardownRemainOwnedBySceneHierarchy()
   {
      let startup = AppKitStartupProductionSceneRoot()
      var primaryActionCount = 0
      startup.onPrimaryAction = {primaryActionCount += 1}
      startup.primaryAction.performClick(nil)
      XCTAssertEqual(primaryActionCount, 1)

      let dashboard = AppKitDashboardProductionSceneRoot()
      dashboard.replaceMetrics([dashboardRecord()])
      var dashboardActionID: String?
      dashboard.collectionController.onAction = {dashboardActionID = $0}
      let dashboardItem = dashboard.collectionController.collectionView(dashboard.metrics, itemForRepresentedObjectAt: IndexPath(item: 0, section: 0)) as? AppKitDashboardCardCollectionViewItem
      dashboardItem?.actionButton?.performClick(nil)
      XCTAssertEqual(dashboardActionID, "dashboard:label:000")

      let feed = AppKitFeedProductionSceneRoot()
      feed.replaceRows([feedRecord(height: 76, favorite: false)])
      var favoriteID: String?
      feed.tableController.onFavorite = {favoriteID = $0}
      let feedCell = feed.tableController.tableView(feed.table, viewFor: feed.table.tableColumns[0], row: 0) as? AppKitFeedTableCellView
      feedCell?.favoriteButton.performClick(nil)
      XCTAssertEqual(favoriteID, "feed:0")

      let chat = AppKitChatProductionSceneRoot()
      var sentText: String?
      chat.onSend = {sentText = $0}
      chat.composer.string = "Message"
      chat.sendButton.performClick(nil)
      XCTAssertEqual(sentText, "Message")

      let navigation = AppKitNavigationProductionSceneRoot()
      navigation.replaceDestinations([navigationRecord()])
      var destinationID: String?
      navigation.tableController.onSelect = {destinationID = $0}
      let navigationCell = navigation.tableController.tableView(navigation.table, viewFor: navigation.table.tableColumns[0], row: 0) as? AppKitNavigationTableCellView
      navigationCell?.destinationButton.performClick(nil)
      XCTAssertEqual(destinationID, "destination:0")

      let image = AppKitImageProductionSceneRoot()
      var zoomValue: Double?
      image.onZoom = {zoomValue = $0}
      image.zoomSlider.doubleValue = 2
      XCTAssertTrue(NSApp.sendAction(image.zoomSlider.action!, to: image.zoomSlider.target, from: image.zoomSlider))
      XCTAssertEqual(zoomValue, 2)

      startup.teardown()
      dashboard.teardown()
      feed.teardown()
      chat.teardown()
      navigation.teardown()
      image.teardown()
      XCTAssertNil(startup.cards.dataSource)
      XCTAssertNil(dashboard.metrics.dataSource)
      XCTAssertNil(feed.table.dataSource)
      XCTAssertNil(chat.thread.dataSource)
      XCTAssertNil(navigation.table.dataSource)
      XCTAssertNil(image.zoomSlider.target)
   }

   func testAccessibilityEvidenceRetainsNativeObjectIdentityClassAndRole()
   {
      let scene = AppKitChatProductionSceneRoot()
      let evidence = AppKitNativeAccessibilityEvidence.capture(from: scene.rootView)
      let send = evidence.first(where: {$0.role == .button && $0.label == "Send"})
      let composer = evidence.first(where: {$0.role == .textArea && $0.label == "Message"})

      XCTAssertTrue(send?.viewClass == NSButtonCell.self)
      XCTAssertEqual(send?.role, NSAccessibility.Role.button)
      XCTAssertEqual(composer?.objectIdentifier, ObjectIdentifier(scene.composer))
      XCTAssertTrue(composer?.viewClass == AppKitOwnedTextView.self)
      XCTAssertEqual(composer?.role, NSAccessibility.Role.textArea)
   }

   func testAccessibilityEvidenceTraversesTheNativeAccessibilityTree()
   {
      let root = AppKitVirtualAccessibilityRoot(frame: .zero)
      let evidence = AppKitNativeAccessibilityEvidence.capture(from: root)

      XCTAssertTrue(evidence.contains(where: {$0.objectIdentifier == ObjectIdentifier(root.virtualChild)}))
      XCTAssertEqual(evidence.first(where: {$0.identifier == "virtual.child"})?.hierarchyDepth, 1)
   }

   private func descendants(of view: NSView) -> [NSView]
   {
      view.subviews + view.subviews.flatMap {descendants(of: $0)}
   }

   private func dashboardRecord(labels: [AppKitDashboardLabelRecord]? = nil) -> AppKitDashboardCardRecord
   {
      let image = NSImage(size: NSSize(width: 24, height: 24))
      return AppKitDashboardCardRecord(
         id: "dashboard:card:00",
         labels: labels ?? [AppKitDashboardLabelRecord(id: "dashboard:label:000", value: "42", accent: false)],
         leadingIcon: image,
         trailingIcon: image,
         font: NSFont.systemFont(ofSize: 7),
         actionTarget: "dashboard:label:000"
      )
   }

   private func feedRecord(id: String = "feed:0", height: CGFloat, favorite: Bool) -> AppKitFeedRowRecord
   {
      let thumbnail = NSImage(size: NSSize(width: 24, height: 24))
      return AppKitFeedRowRecord(
         id: id,
         title: "First",
         height: height,
         thumbnail: thumbnail,
         thumbnailAtlas: thumbnail,
         thumbnailIndex: 0,
         titleFont: NSFont.systemFont(ofSize: 15),
         secondaryFont: NSFont.systemFont(ofSize: 11),
         inlineText: nil,
         favorite: favorite
      )
   }

   private func chatRecord(id: String = "chat:0", text: String = "Hello") -> AppKitChatMessageRecord
   {
      let avatar = NSImage(size: NSSize(width: 24, height: 24))
      return AppKitChatMessageRecord(
         id: id,
         sequence: 0,
         direction: "ltr",
         text: text,
         avatar: avatar,
         avatarAtlas: avatar,
         avatarIndex: 0,
         textFont: NSFont.systemFont(ofSize: 15),
         inlineText: nil
      )
   }

   private func navigationRecord() -> AppKitNavigationDestinationRecord
   {
      AppKitNavigationDestinationRecord(
         id: "destination:0",
         title: "Detail",
         subtitle: "Canonical navigation row",
         titleFont: NSFont.systemFont(ofSize: 15),
         subtitleFont: NSFont.systemFont(ofSize: 11)
      )
   }

   private func configureStaleControls(_ button: NSButton, _ field: NSTextField, _ slider: NSSlider)
   {
      let target = ReuseActionTarget.shared
      button.target = target
      button.action = #selector(ReuseActionTarget.invoke)
      field.delegate = target
      field.stringValue = "stale"
      slider.target = target
      slider.action = #selector(ReuseActionTarget.invoke)
      slider.doubleValue = 0.75
   }

   private func assertControlsWereCleaned(_ button: NSButton, _ field: NSTextField, _ slider: NSSlider)
   {
      XCTAssertNil(button.target)
      XCTAssertNil(button.action)
      XCTAssertNil(field.delegate)
      XCTAssertEqual(field.stringValue, "")
      XCTAssertNil(slider.target)
      XCTAssertNil(slider.action)
      XCTAssertEqual(slider.doubleValue, slider.minValue)
   }
}

private final class AppKitVirtualAccessibilityRoot: NSView
{
   let virtualChild: NSAccessibilityElement

   override init(frame frameRect: NSRect)
   {
      virtualChild = NSAccessibilityElement()
      super.init(frame: frameRect)
      virtualChild.setAccessibilityIdentifier("virtual.child")
      virtualChild.setAccessibilityRole(.staticText)
      virtualChild.setAccessibilityLabel("Virtual child")
   }

   required init?(coder: NSCoder)
   {
      nil
   }

   override func accessibilityChildren() -> [Any]?
   {
      [virtualChild]
   }
}

private final class ReuseActionTarget: NSObject, NSTextFieldDelegate
{
   static let shared = ReuseActionTarget()

   @objc func invoke()
   {
   }
}

private final class DashboardReloadTrackingCollectionView: NSCollectionView
{
   private(set) var reloadDataCount = 0
   private(set) var reloadItemsCount = 0

   override func reloadData()
   {
      reloadDataCount += 1
      super.reloadData()
   }

   override func reloadItems(at indexPaths: Set<IndexPath>)
   {
      reloadItemsCount += 1
      super.reloadItems(at: indexPaths)
   }
}

private final class FeedReloadTrackingTableView: NSTableView
{
   private(set) var reloadDataCount = 0
   private(set) var insertedRowCount = 0

   override func reloadData()
   {
      reloadDataCount += 1
      super.reloadData()
   }

   override func insertRows(at indexes: IndexSet, withAnimation animationOptions: NSTableView.AnimationOptions = [])
   {
      insertedRowCount += indexes.count
      super.insertRows(at: indexes, withAnimation: animationOptions)
   }
}

private final class CollectionReuseProbeItem: AppKitReusableCollectionViewItem
{
   let button = NSButton(title: "Action", target: nil, action: nil)
   let field = NSTextField(string: "")
   let slider = NSSlider(value: 0.5, minValue: 0, maxValue: 1, target: nil, action: nil)

   override func loadView()
   {
      super.loadView()
      view.addSubview(button)
      view.addSubview(field)
      view.addSubview(slider)
   }
}

private final class TableReuseProbeCell: AppKitReusableTableCellView
{
   let button = NSButton(title: "Action", target: nil, action: nil)
   let field = NSTextField(string: "")
   let slider = NSSlider(value: 0.5, minValue: 0, maxValue: 1, target: nil, action: nil)
   private(set) var cleanupHookCount = 0

   override init(frame frameRect: NSRect)
   {
      super.init(frame: frameRect)
      addSubview(button)
      addSubview(field)
      addSubview(slider)
   }

   required init?(coder: NSCoder)
   {
      fatalError("TableReuseProbeCell does not support coder initialization")
   }

   override func didCleanupReusableContent()
   {
      cleanupHookCount += 1
   }
}
