import AppKit
import CoreText
import Foundation

final class AppKitStartupCardView: NSView
{
   private var cardEdges: AppKitProductionShadowedSurfaceEdges?

   var renderedCardBounds: CGRect
   {
      CGRect(x: 0, y: 0, width: bounds.width, height: 176)
   }

   override var isFlipped: Bool {true}

   override func layout()
   {
      super.layout()
      cardEdges = AppKitProductionEdgeRasterCache.shared.shadowedSurfaceEdges(
         card: renderedCardBounds,
         in: self
      )
   }

   override func draw(_ dirtyRect: NSRect)
   {
      let card = renderedCardBounds
      guard let cardEdges else
      {
         AppKitProductionPalette.shadow.setFill()
         NSBezierPath(roundedRect: card.offsetBy(dx: 0, dy: 2), xRadius: 12, yRadius: 12).fill()
         AppKitProductionPalette.surface.setFill()
         NSBezierPath(roundedRect: card, xRadius: 12, yRadius: 12).fill()
         return
      }
      let radius: CGFloat = 12
      AppKitProductionPalette.shadow.setFill()
      CGRect(x: card.minX + radius, y: card.minY + 2, width: card.width - radius * 2, height: card.height).fill()
      CGRect(x: card.minX, y: card.minY + 2 + radius, width: card.width, height: card.height - radius * 2).fill()
      AppKitProductionPalette.surface.setFill()
      CGRect(x: card.minX + radius, y: card.minY, width: card.width - radius * 2, height: card.height).fill()
      CGRect(x: card.minX, y: card.minY + radius, width: card.width, height: card.height - radius * 2).fill()
      cardEdges.draw(in: card)
   }
}

final class AppKitStartupCardCollectionViewItem: AppKitReusableCollectionViewItem
{
   static let reuseIdentifier = NSUserInterfaceItemIdentifier("startup.card")

   let titleField = AppKitProductionExactTextField(frame: CGRect(x: 72, y: 14, width: 89, height: 22))
   let detailField = AppKitProductionExactTextField(frame: CGRect(x: 72, y: 42, width: 89, height: 54))
   let thumbnailView = AppKitProductionRoundedImageView(frame: CGRect(x: 12, y: 12, width: 48, height: 48))

   func configure(record: AppKitStartupCardRecord)
   {
      bind(representedIdentifier: record.id)
      view.identifier = NSUserInterfaceItemIdentifier(record.id)
      view.setAccessibilityIdentifier(record.id)
      view.setAccessibilityLabel("card")
      view.setAccessibilityRole(.group)
      titleField.configure(identifier: "\(record.id):title", value: record.title, font: record.titleFont, color: AppKitProductionPalette.text)
      detailField.configure(identifier: "\(record.id):detail", value: record.detail, font: record.detailFont, color: AppKitProductionPalette.secondaryText)
      if let thumbnail = record.thumbnail, let atlas = record.thumbnailAtlas
      {
         thumbnailView.configure(image: thumbnail, atlas: atlas, tileIndex: record.thumbnailIndex)
      }
      else
      {
         thumbnailView.image = record.thumbnail
      }
      thumbnailView.identifier = NSUserInterfaceItemIdentifier("\(record.id):thumbnail")
      thumbnailView.setAccessibilityIdentifier("\(record.id):thumbnail")
      thumbnailView.setAccessibilityLabel("initial-image")
      thumbnailView.setAccessibilityRole(.image)
      view.needsLayout = true
      view.needsDisplay = true
   }

   override func loadView()
   {
      view = AppKitStartupCardView(frame: CGRect(x: 0, y: 0, width: 173, height: 188))
      view.identifier = NSUserInterfaceItemIdentifier("production-reference.startup.card")
      view.addSubview(thumbnailView)
      view.addSubview(titleField)
      view.addSubview(detailField)
   }

   override func viewDidLayout()
   {
      super.viewDidLayout()
      thumbnailView.prepareAnalyticRaster()
   }

   override func didCleanupReusableContent()
   {
      titleField.clearPreparedText()
      detailField.clearPreparedText()
   }
}

final class AppKitDashboardCardCollectionViewItem: AppKitReusableCollectionViewItem
{
   static let reuseIdentifier = NSUserInterfaceItemIdentifier("dashboard.card")

   let leadingIconView = AppKitProductionExactImageView(frame: CGRect(x: 6, y: 6, width: 14, height: 14))
   let trailingIconView = AppKitProductionExactImageView(frame: CGRect(x: 24, y: 6, width: 14, height: 14))
   private(set) var labelFields = [AppKitProductionExactTextField]()
   private(set) var actionButton: AppKitProductionSolidButton?

   func configure(record: AppKitDashboardCardRecord, target: AnyObject, action: Selector, row: Int)
   {
      bind(representedIdentifier: record.id)
      view.identifier = NSUserInterfaceItemIdentifier(record.id)
      view.setAccessibilityIdentifier(record.id)
      view.setAccessibilityLabel("rounded-card")
      view.setAccessibilityRole(.group)
      leadingIconView.image = record.leadingIcon
      leadingIconView.identifier = NSUserInterfaceItemIdentifier("\(record.id):icon:0")
      leadingIconView.setAccessibilityIdentifier("\(record.id):icon:0")
      leadingIconView.setAccessibilityLabel("icon-image")
      leadingIconView.setAccessibilityRole(.image)
      trailingIconView.image = record.trailingIcon
      trailingIconView.identifier = NSUserInterfaceItemIdentifier("\(record.id):icon:1")
      trailingIconView.setAccessibilityIdentifier("\(record.id):icon:1")
      trailingIconView.setAccessibilityLabel("icon-image")
      trailingIconView.setAccessibilityRole(.image)
      reconcileLabels(record)
      reconcileAction(record, target: target, action: action, row: row)
   }

   override func loadView()
   {
      let card = AppKitProductionRoundedCardView(frame: CGRect(x: 0, y: 0, width: 173, height: 40))
      card.identifier = NSUserInterfaceItemIdentifier("production-reference.dashboard.card")
      view = card
      view.addSubview(leadingIconView)
      view.addSubview(trailingIconView)
   }

   override func didCleanupReusableContent()
   {
      labelFields.forEach {$0.clearPreparedText()}
      actionButton?.removeFromSuperview()
      actionButton = nil
   }

   private func reconcileLabels(_ record: AppKitDashboardCardRecord)
   {
      while labelFields.count < record.labels.count
      {
         let field = AppKitProductionExactTextField(frame: .zero)
         labelFields.append(field)
         view.addSubview(field)
      }
      while labelFields.count > record.labels.count
      {
         labelFields.removeLast().removeFromSuperview()
      }
      for (index, label) in record.labels.enumerated()
      {
         let field = labelFields[index]
         field.frame = CGRect(x: 42, y: 2 - 8.0 / 3.0 + CGFloat(index) * 6, width: 125, height: 11)
         field.configure(
            identifier: label.id,
            value: label.value,
            font: record.font,
            color: label.accent ? AppKitProductionPalette.accent : index == 0 ? AppKitProductionPalette.text : AppKitProductionPalette.secondaryText
         )
      }
   }

   private func reconcileAction(_ record: AppKitDashboardCardRecord, target: AnyObject, action: Selector, row: Int)
   {
      guard record.actionTarget != nil else
      {
         actionButton?.removeFromSuperview()
         actionButton = nil
         return
      }
      let button = actionButton ?? AppKitProductionSolidButton(frame: CGRect(x: 145, y: 13, width: 18, height: 12))
      if button.superview == nil
      {
         view.addSubview(button)
      }
      let prefix = record.id.split(separator: ":", maxSplits: 1).first.map(String.init) ?? "dashboard"
      button.identifier = NSUserInterfaceItemIdentifier("\(prefix):control:\(String(format: "%02d", row))")
      button.setAccessibilityIdentifier(button.identifier!.rawValue)
      button.tag = row
      button.target = target
      button.action = action
      actionButton = button
   }
}

final class AppKitFeedTableCellView: AppKitReusableTableCellView
{
   static let reuseIdentifier = NSUserInterfaceItemIdentifier("feed.row")

   let thumbnailView = AppKitProductionRoundedImageView(frame: .zero)
   let titleField = AppKitProductionExactTextField(frame: .zero)
   let secondaryField = AppKitProductionExactTextField(frame: .zero)
   let favoriteButton = AppKitProductionCircularControl(frame: .zero)
   private var cardEdges: AppKitProductionShadowedSurfaceEdges?

   override var isFlipped: Bool {true}

   var renderedCardBounds: CGRect
   {
      CGRect(x: 12, y: 4 + canonicalContentYOffset, width: max(0, bounds.width - 24), height: max(0, bounds.height - 8))
   }

   func configure(record: AppKitFeedRowRecord, target: AnyObject, action: Selector, row: Int)
   {
      bind(representedIdentifier: record.id)
      identifier = NSUserInterfaceItemIdentifier(record.id)
      setAccessibilityIdentifier(record.id)
      setAccessibilityLabel("feed-card")
      setAccessibilityRole(.group)
      thumbnailView.configure(image: record.thumbnail, atlas: record.thumbnailAtlas, tileIndex: record.thumbnailIndex)
      thumbnailView.identifier = NSUserInterfaceItemIdentifier("\(record.id):thumbnail")
      thumbnailView.setAccessibilityIdentifier("\(record.id):thumbnail")
      thumbnailView.setAccessibilityLabel("thumbnail")
      thumbnailView.setAccessibilityRole(.image)
      titleField.configure(
         identifier: "\(record.id):title",
         value: record.title,
         font: record.titleFont,
         color: AppKitProductionPalette.text,
         inlineText: record.inlineText
      )
      secondaryField.configure(
         identifier: "\(record.id):secondary",
         value: record.id,
         font: record.secondaryFont,
         color: AppKitProductionPalette.secondaryText
      )
      favoriteButton.identifier = NSUserInterfaceItemIdentifier("\(record.id):favorite")
      favoriteButton.setAccessibilityIdentifier("\(record.id):favorite")
      favoriteButton.state = record.favorite ? .on : .off
      favoriteButton.tag = row
      favoriteButton.target = target
      favoriteButton.action = action
      favoriteButton.needsDisplay = true
      needsLayout = true
      needsDisplay = true
   }

   override init(frame frameRect: NSRect)
   {
      super.init(frame: frameRect)
      addSubview(thumbnailView)
      addSubview(titleField)
      addSubview(secondaryField)
      addSubview(favoriteButton)
   }

   required init?(coder: NSCoder)
   {
      fatalError("AppKitFeedTableCellView does not support coder initialization")
   }

   override func layout()
   {
      super.layout()
      let cardWidth = max(0, bounds.width - 24)
      let offset = canonicalContentYOffset
      thumbnailView.frame = CGRect(x: 22, y: 14 + offset, width: 48, height: 48)
      thumbnailView.prepareAnalyticRaster()
      titleField.frame = CGRect(x: 82, y: 12 + offset, width: max(0, cardWidth - 112), height: 24)
      secondaryField.frame = CGRect(x: 82, y: 38 + offset, width: max(0, cardWidth - 112), height: 18)
      favoriteButton.frame = CGRect(x: max(12, bounds.width - 46), y: 15 + offset, width: 18, height: 18)
      favoriteButton.prepareAnalyticImages()
      cardEdges = AppKitProductionEdgeRasterCache.shared.shadowedSurfaceEdges(card: renderedCardBounds, in: self)
   }

   override func draw(_ dirtyRect: NSRect)
   {
      let card = renderedCardBounds
      guard let cardEdges else
      {
         AppKitProductionPalette.shadow.setFill()
         NSBezierPath(roundedRect: card.offsetBy(dx: 0, dy: 2), xRadius: 12, yRadius: 12).fill()
         AppKitProductionPalette.surface.setFill()
         NSBezierPath(roundedRect: card, xRadius: 12, yRadius: 12).fill()
         return
      }
      let radius: CGFloat = 12
      AppKitProductionPalette.shadow.setFill()
      CGRect(x: card.minX + radius, y: card.minY + 2, width: card.width - radius * 2, height: card.height).fill()
      CGRect(x: card.minX, y: card.minY + 2 + radius, width: card.width, height: card.height - radius * 2).fill()
      AppKitProductionPalette.surface.setFill()
      CGRect(x: card.minX + radius, y: card.minY, width: card.width - radius * 2, height: card.height).fill()
      CGRect(x: card.minX, y: card.minY + radius, width: card.width, height: card.height - radius * 2).fill()
      cardEdges.draw(in: card)
   }

   var canonicalContentYOffset: CGFloat
   {
      guard window != nil else {return 0}
      let localY: CGFloat = 4
      let windowY = convert(CGPoint(x: 0, y: localY), to: nil).y
      let nextWindowY = convert(CGPoint(x: 0, y: localY + 1), to: nil).y
      let direction = nextWindowY - windowY
      guard abs(direction) > 0.5 else {return 0}
      return ((windowY * 3).rounded() / 3 - windowY) / direction
   }

   override func didCleanupReusableContent()
   {
      titleField.clearPreparedText()
      secondaryField.clearPreparedText()
   }
}

final class AppKitChatEditableMessageField: AppKitProductionExactTextField, NSTextFieldDelegate
{
   var onSelectionChange: ((NSRange) -> Void)?
   var onValueChange: ((String) -> Void)?
   private var selectionObserver: NSObjectProtocol?

   override init(frame frameRect: NSRect)
   {
      super.init(frame: frameRect)
      delegate = self
   }

   required init?(coder: NSCoder)
   {
      fatalError("AppKitChatEditableMessageField does not support coder initialization")
   }

   func configureMessage(identifier: String, fullValue: String, visibleValue: String, font: NSFont, color: NSColor, inlineText: AppKitProductionInlineText?, editable: Bool)
   {
      isEditable = editable
      isSelectable = editable
      focusRingType = .none
      setAccessibilityRole(editable ? .textField : .staticText)
      configure(
         identifier: identifier,
         value: visibleValue,
         font: font,
         color: color,
         inlineText: inlineText,
         nativeValue: editable ? fullValue : visibleValue
      )
      if !editable {stopObservingSelection()}
   }

   func controlTextDidBeginEditing(_ notification: Notification)
   {
      guard let editor = notification.userInfo?["NSFieldEditor"] as? NSTextView
         ?? window?.fieldEditor(false, for: self) as? NSTextView else {return}
      observeNativeEditor(editor)
   }

   override func mouseDown(with event: NSEvent)
   {
      super.mouseDown(with: event)
      guard let editor = currentEditor() as? NSTextView else {return}
      observeNativeEditor(editor)
   }

   func observeNativeEditor(_ editor: NSTextView)
   {
      stopObservingSelection()
      selectionObserver = NotificationCenter.default.addObserver(
         forName: NSTextView.didChangeSelectionNotification,
         object: editor,
         queue: .main
      )
      {
         [weak self, weak editor] _ in
         guard let self, let editor else {return}
         self.onSelectionChange?(editor.selectedRange())
      }
      onSelectionChange?(editor.selectedRange())
   }

   func controlTextDidChange(_ notification: Notification)
   {
      let editor = notification.userInfo?["NSFieldEditor"] as? NSTextView
         ?? currentEditor() as? NSTextView
      onValueChange?(editor?.string ?? stringValue)
   }

   func controlTextDidEndEditing(_ notification: Notification)
   {
      stopObservingSelection()
   }

   deinit
   {
      stopObservingSelection()
   }

   private func stopObservingSelection()
   {
      if let selectionObserver
      {
         NotificationCenter.default.removeObserver(selectionObserver)
         self.selectionObserver = nil
      }
   }
}

final class AppKitChatTableCellView: AppKitReusableTableCellView
{
   static let reuseIdentifier = NSUserInterfaceItemIdentifier("chat.message")

   let avatarView = AppKitProductionRoundedImageView(frame: .zero)
   let messageFields: [AppKitProductionExactTextField] = [
      AppKitChatEditableMessageField(frame: .zero),
      AppKitProductionExactTextField(frame: .zero),
   ]
   var onMessageSelection: ((String, NSRange) -> Void)?
   var onMessageValueChange: ((String, String) -> Void)?
   private var bubbleX: CGFloat = 60
   private var cardEdges: AppKitProductionShadowedSurfaceEdges?

   override var isFlipped: Bool {true}

   var renderedBubbleBounds: CGRect
   {
      CGRect(x: bubbleX, y: 2, width: 286, height: max(0, bounds.height - 4))
   }

   func configure(record: AppKitChatMessageRecord)
   {
      bind(representedIdentifier: record.id)
      identifier = NSUserInterfaceItemIdentifier(record.id)
      setAccessibilityIdentifier(record.id)
      setAccessibilityLabel("message")
      setAccessibilityRole(.group)
      let rtl = record.direction == "rtl"
      bubbleX = rtl ? 12 : 60
      avatarView.frame = CGRect(x: rtl ? 342 : 12, y: 4, width: 36, height: 36)
      avatarView.identifier = NSUserInterfaceItemIdentifier("\(record.id):avatar")
      avatarView.setAccessibilityIdentifier("\(record.id):avatar")
      avatarView.setAccessibilityLabel("avatar")
      avatarView.setAccessibilityRole(.image)
      avatarView.configure(
         image: record.avatar,
         atlas: record.avatarAtlas,
         tileIndex: record.avatarIndex,
         radius: 18,
         underlay: .background
      )
      let textRect = CGRect(x: bubbleX + 10, y: 8, width: 266, height: record.height - 16)
      let lines = wrappedLines(record.text, width: textRect.width, font: record.textFont, inlineText: record.inlineText)
      let visible = Array(lines.prefix(2))
      let lineHeight: CGFloat = 61 / 3
      let centerOffset = visible.count > 1 ? lineHeight * 0.5 : 0
      for index in messageFields.indices
      {
         let field = messageFields[index]
         guard index < visible.count else
         {
            field.isHidden = true
            field.clearPreparedText()
            continue
         }
         let line = visible[index]
         field.frame = CGRect(
            x: textRect.minX,
            y: textRect.minY - centerOffset + CGFloat(index) * lineHeight,
            width: textRect.width,
            height: textRect.height
         )
         if index == 0, let editable = field as? AppKitChatEditableMessageField
         {
            let isSelectionTarget = record.id == "chat:append:16"
            editable.onSelectionChange = isSelectionTarget ? {[weak self] range in self?.onMessageSelection?(record.id, range)} : nil
            editable.onValueChange = isSelectionTarget ? {[weak self] value in self?.onMessageValueChange?(record.id, value)} : nil
            editable.configureMessage(
               identifier: isSelectionTarget ? record.id : "\(record.id):line:\(index)",
               fullValue: record.text,
               visibleValue: line,
               font: record.textFont,
               color: AppKitProductionPalette.text,
               inlineText: record.inlineText,
               editable: isSelectionTarget
            )
         }
         else
         {
            field.configure(
               identifier: "\(record.id):line:\(index)",
               value: line,
               font: record.textFont,
               color: AppKitProductionPalette.text,
               inlineText: record.inlineText,
               inlineImageYOffset: index == 1 ? 1 / 3 : 0
            )
         }
         field.alignment = rtl ? .right : .left
         field.isHidden = false
      }
      needsLayout = true
      needsDisplay = true
   }

   override init(frame frameRect: NSRect)
   {
      super.init(frame: frameRect)
      addSubview(avatarView)
      messageFields.forEach(addSubview)
   }

   required init?(coder: NSCoder)
   {
      fatalError("AppKitChatTableCellView does not support coder initialization")
   }

   override func layout()
   {
      super.layout()
      cardEdges = AppKitProductionEdgeRasterCache.shared.shadowedSurfaceEdges(card: renderedBubbleBounds, in: self)
      avatarView.prepareAnalyticRaster()
   }

   override func draw(_ dirtyRect: NSRect)
   {
      let card = CGRect(x: bubbleX, y: 2, width: 286, height: max(0, bounds.height - 4))
      guard let cardEdges else
      {
         AppKitProductionPalette.shadow.setFill()
         NSBezierPath(roundedRect: card.offsetBy(dx: 0, dy: 2), xRadius: 12, yRadius: 12).fill()
         AppKitProductionPalette.surface.setFill()
         NSBezierPath(roundedRect: card, xRadius: 12, yRadius: 12).fill()
         return
      }
      let radius: CGFloat = 12
      AppKitProductionPalette.shadow.setFill()
      CGRect(x: card.minX + radius, y: card.minY + 2, width: card.width - radius * 2, height: card.height).fill()
      CGRect(x: card.minX, y: card.minY + 2 + radius, width: card.width, height: card.height - radius * 2).fill()
      AppKitProductionPalette.surface.setFill()
      CGRect(x: card.minX + radius, y: card.minY, width: card.width - radius * 2, height: card.height).fill()
      CGRect(x: card.minX, y: card.minY + radius, width: card.width, height: card.height - radius * 2).fill()
      cardEdges.draw(in: card)
   }

   override func didCleanupReusableContent()
   {
      if let editable = messageFields[0] as? AppKitChatEditableMessageField
      {
         editable.onSelectionChange = nil
         editable.onValueChange = nil
      }
      messageFields.forEach {$0.clearPreparedText()}
      cardEdges = nil
   }

   private func wrappedLines(_ value: String, width: CGFloat, font: NSFont, inlineText: AppKitProductionInlineText?) -> [String]
   {
      let words = value.split(whereSeparator: {$0.isWhitespace})
      var lines = [String]()
      var current = ""
      for word in words
      {
         let candidate = current.isEmpty ? String(word) : "\(current) \(word)"
         let candidateWidth = inlineText?.measure(candidate, font: font) ?? textWidth(candidate, font: font)
         if !current.isEmpty && candidateWidth > width
         {
            lines.append(current)
            current = String(word)
         }
         else
         {
            current = candidate
         }
      }
      if !current.isEmpty {lines.append(current)}
      return lines
   }

   private func textWidth(_ value: String, font: NSFont) -> CGFloat
   {
      let line = CTLineCreateWithAttributedString(NSAttributedString(
         string: value,
         attributes: [kCTFontAttributeName as NSAttributedString.Key: font]
      ))
      return CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil))
   }
}

final class AppKitNavigationTableCellView: AppKitReusableTableCellView
{
   static let reuseIdentifier = NSUserInterfaceItemIdentifier("navigation.destination")

   let titleField = AppKitProductionExactTextField(frame: .zero)
   let subtitleField = AppKitProductionExactTextField(frame: .zero)
   let destinationButton = NSButton(title: "", target: nil, action: nil)
   private var cardEdges: AppKitProductionShadowedSurfaceEdges?

   override var isFlipped: Bool {true}

   var renderedCardBounds: CGRect
   {
      CGRect(x: 16, y: 0, width: max(0, bounds.width - 32), height: 54)
   }

   func configure(record: AppKitNavigationDestinationRecord, target: AnyObject, action: Selector, row: Int)
   {
      bind(representedIdentifier: record.id)
      identifier = NSUserInterfaceItemIdentifier(record.id)
      setAccessibilityIdentifier(record.id)
      setAccessibilityLabel("list-item")
      setAccessibilityRole(.group)
      titleField.configure(
         identifier: "\(record.id):title",
         value: record.title,
         font: record.titleFont,
         color: AppKitProductionPalette.text
      )
      subtitleField.configure(
         identifier: "\(record.id):subtitle",
         value: record.subtitle,
         font: record.subtitleFont,
         color: AppKitProductionPalette.secondaryText
      )
      destinationButton.title = ""
      destinationButton.setAccessibilityIdentifier(record.id)
      destinationButton.setAccessibilityLabel(record.title)
      destinationButton.tag = row
      destinationButton.target = target
      destinationButton.action = action
      needsLayout = true
      needsDisplay = true
   }

   override init(frame frameRect: NSRect)
   {
      super.init(frame: frameRect)
      destinationButton.isBordered = false
      destinationButton.isTransparent = true
      destinationButton.focusRingType = .none
      addSubview(titleField)
      addSubview(subtitleField)
      addSubview(destinationButton)
   }

   required init?(coder: NSCoder)
   {
      fatalError("AppKitNavigationTableCellView does not support coder initialization")
   }

   override func layout()
   {
      super.layout()
      let card = renderedCardBounds
      titleField.frame = CGRect(x: card.minX + 10, y: card.minY + 10 + 2.0 / 3.0, width: 260, height: 24)
      subtitleField.frame = CGRect(x: card.minX + 10, y: card.minY + 30 + 1.0 / 3.0, width: 260, height: 20)
      destinationButton.frame = card
      cardEdges = AppKitProductionEdgeRasterCache.shared.shadowedSurfaceEdges(card: card, in: self)
   }

   override func draw(_ dirtyRect: NSRect)
   {
      let card = renderedCardBounds
      guard let cardEdges else
      {
         AppKitProductionPalette.shadow.setFill()
         NSBezierPath(roundedRect: card.offsetBy(dx: 0, dy: 2), xRadius: 12, yRadius: 12).fill()
         AppKitProductionPalette.surface.setFill()
         NSBezierPath(roundedRect: card, xRadius: 12, yRadius: 12).fill()
         return
      }
      AppKitProductionPalette.shadow.setFill()
      CGRect(x: card.minX + 12, y: card.minY + 2, width: card.width - 24, height: card.height).fill()
      CGRect(x: card.minX, y: card.minY + 14, width: card.width, height: card.height - 24).fill()
      AppKitProductionPalette.surface.setFill()
      CGRect(x: card.minX + 12, y: card.minY, width: card.width - 24, height: card.height).fill()
      CGRect(x: card.minX, y: card.minY + 12, width: card.width, height: card.height - 24).fill()
      cardEdges.draw(in: card)
   }

   override func didCleanupReusableContent()
   {
      titleField.clearPreparedText()
      subtitleField.clearPreparedText()
      destinationButton.target = nil
      destinationButton.action = nil
      cardEdges = nil
   }
}

final class AppKitNavigationDetailView: NSView
{
   let backButton = NSButton(title: "", target: nil, action: nil)
   let modalButton = NSButton(title: "", target: nil, action: nil)
   let backField = AppKitProductionExactTextField(frame: .zero)
   let headingField = AppKitProductionExactTextField(frame: .zero)
   let titleField = AppKitProductionExactTextField(frame: .zero)
   let subtitleField = AppKitProductionExactTextField(frame: .zero)
   private var cardEdges: AppKitProductionShadowedSurfaceEdges?

   override var isFlipped: Bool {true}

   init(headingFont: NSFont, bodyFont: NSFont, secondaryFont: NSFont)
   {
      super.init(frame: CGRect(x: 0, y: 0, width: 390, height: 844))
      identifier = NSUserInterfaceItemIdentifier("navigation.detail")
      setAccessibilityIdentifier("navigation.detail")
      setAccessibilityLabel("detail")
      setAccessibilityRole(.group)
      backField.configure(identifier: "navigation.back-label", value: "‹ Back", font: bodyFont, color: AppKitProductionPalette.accent)
      headingField.configure(identifier: "navigation.detail-heading", value: "Detail", font: headingFont, color: AppKitProductionPalette.text)
      titleField.configure(identifier: "navigation.detail-title", value: "Selected destination", font: bodyFont, color: AppKitProductionPalette.text)
      subtitleField.configure(identifier: "navigation.detail-subtitle", value: "Canonical navigation detail", font: secondaryFont, color: AppKitProductionPalette.secondaryText)
      backButton.identifier = NSUserInterfaceItemIdentifier("navigation.back")
      backButton.setAccessibilityIdentifier("navigation.back")
      backButton.setAccessibilityLabel("Back")
      backButton.isBordered = false
      backButton.isTransparent = true
      backButton.focusRingType = .none
      modalButton.identifier = NSUserInterfaceItemIdentifier("navigation.modal-action")
      modalButton.setAccessibilityIdentifier("navigation.modal-action")
      modalButton.setAccessibilityLabel("Open modal")
      modalButton.isBordered = false
      modalButton.isTransparent = true
      modalButton.focusRingType = .none
      addSubview(backField)
      addSubview(headingField)
      addSubview(titleField)
      addSubview(subtitleField)
      addSubview(backButton)
      addSubview(modalButton)
   }

   required init?(coder: NSCoder)
   {
      fatalError("AppKitNavigationDetailView does not support coder initialization")
   }

   override func layout()
   {
      super.layout()
      backField.frame = CGRect(x: 16, y: 6, width: 58, height: 52)
      headingField.frame = CGRect(x: 82, y: 6, width: 180, height: 52)
      titleField.frame = CGRect(x: 28, y: 74 + 2.0 / 3.0, width: 280, height: 24)
      subtitleField.frame = CGRect(x: 28, y: 98 + 1.0 / 3.0, width: 310, height: 20)
      backButton.frame = CGRect(x: 8, y: 6, width: 70, height: 52)
      modalButton.frame = CGRect(x: 16, y: 64, width: 358, height: 180)
      cardEdges = AppKitProductionEdgeRasterCache.shared.shadowedSurfaceEdges(
         card: CGRect(x: 16, y: 64, width: 358, height: 180),
         in: self
      )
   }

   override func draw(_ dirtyRect: NSRect)
   {
      let card = CGRect(x: 16, y: 64, width: 358, height: 180)
      guard let cardEdges else
      {
         AppKitProductionPalette.shadow.setFill()
         NSBezierPath(roundedRect: card.offsetBy(dx: 0, dy: 2), xRadius: 12, yRadius: 12).fill()
         AppKitProductionPalette.surface.setFill()
         NSBezierPath(roundedRect: card, xRadius: 12, yRadius: 12).fill()
         return
      }
      AppKitProductionPalette.shadow.setFill()
      CGRect(x: card.minX + 12, y: card.minY + 2, width: card.width - 24, height: card.height).fill()
      CGRect(x: card.minX, y: card.minY + 14, width: card.width, height: card.height - 24).fill()
      AppKitProductionPalette.surface.setFill()
      CGRect(x: card.minX + 12, y: card.minY, width: card.width - 24, height: card.height).fill()
      CGRect(x: card.minX, y: card.minY + 12, width: card.width, height: card.height - 24).fill()
      cardEdges.draw(in: card)
      appKitProductionDetailBottomShadowColor().setFill()
      CGRect(x: card.minX + 12, y: card.maxY, width: card.width - 24, height: 2).fill()
   }
}

func appKitProductionDetailBottomShadowColor() -> NSColor
{
   NSColor(srgbRed: 225 / 255, green: 227 / 255, blue: 230 / 255, alpha: 1)
}

final class AppKitNavigationOverlayView: NSView
{
   var progress: CGFloat = 0
   {
      didSet {needsDisplay = true}
   }

   override var isFlipped: Bool {true}

   override func draw(_ dirtyRect: NSRect)
   {
      AppKitProductionPalette.modalOverlay.withAlphaComponent((176 / 255) * progress).setFill()
      bounds.fill()
   }
}

final class AppKitNavigationModalView: NSView
{
   let headingField = AppKitProductionExactTextField(frame: .zero)
   let firstBodyField = AppKitProductionExactTextField(frame: .zero)
   let secondBodyField = AppKitProductionExactTextField(frame: .zero)
   let dismissField = AppKitProductionExactTextField(frame: .zero)
   let dismissButton = NSButton(title: "", target: nil, action: nil)
   private var modalEdges: AppKitProductionNavigationModalEdges?
   private var dismissEdges: AppKitProductionNavigationDismissEdges?
   var progress: CGFloat = 0
   {
      didSet
      {
         needsLayout = true
         needsDisplay = true
      }
   }

   override var isFlipped: Bool {true}

   var renderedModalBounds: CGRect
   {
      CGRect(x: 0, y: 0, width: bounds.width, height: 580)
   }

   init(headingFont: NSFont, bodyFont: NSFont, controlFont: NSFont)
   {
      super.init(frame: CGRect(x: 414, y: 132, width: 342, height: 582))
      identifier = NSUserInterfaceItemIdentifier("navigation.modal")
      setAccessibilityIdentifier("navigation.modal")
      setAccessibilityLabel("modal")
      setAccessibilityRole(.group)
      headingField.configure(identifier: "navigation.modal-heading", value: "Modal", font: headingFont, color: AppKitProductionPalette.text)
      firstBodyField.configure(identifier: "navigation.modal-body-first", value: "Canonical modal content", font: bodyFont, color: AppKitProductionPalette.secondaryText)
      secondBodyField.configure(identifier: "navigation.modal-body-second", value: "Transition remains apples-to-apples", font: bodyFont, color: AppKitProductionPalette.secondaryText)
      dismissField.configure(identifier: "navigation.dismiss-label", value: "Done", font: controlFont, color: AppKitProductionPalette.surface)
      dismissField.alignment = .center
      dismissButton.identifier = NSUserInterfaceItemIdentifier("navigation.dismiss")
      dismissButton.setAccessibilityIdentifier("navigation.dismiss")
      dismissButton.setAccessibilityLabel("Done")
      dismissButton.isBordered = false
      dismissButton.isTransparent = true
      dismissButton.focusRingType = .none
      addSubview(headingField)
      addSubview(firstBodyField)
      addSubview(secondBodyField)
      addSubview(dismissField)
      addSubview(dismissButton)
   }

   required init?(coder: NSCoder)
   {
      fatalError("AppKitNavigationModalView does not support coder initialization")
   }

   override func layout()
   {
      super.layout()
      headingField.frame = CGRect(x: 18, y: 18, width: 220, height: 28)
      firstBodyField.frame = CGRect(x: 18, y: 48, width: 270, height: 20)
      secondBodyField.frame = CGRect(x: 18, y: 67, width: 240, height: 20)
      dismissField.frame = CGRect(x: 268, y: 18, width: 50, height: 28)
      dismissButton.frame = CGRect(x: 268, y: 18, width: 50, height: 28)
      modalEdges = AppKitProductionNavigationEdgeRasterCache.shared.modalEdges(
         card: renderedModalBounds,
         in: self,
         progress: progress
      )
      dismissEdges = AppKitProductionNavigationEdgeRasterCache.shared.dismissEdges(
         control: dismissButton.frame,
         in: self
      )
   }

   override func draw(_ dirtyRect: NSRect)
   {
      let card = renderedModalBounds
      if let modalEdges
      {
         AppKitProductionPalette.shadow.setFill()
         CGRect(x: card.minX + 16, y: card.minY + 2, width: card.width - 32, height: card.height).fill()
         CGRect(x: card.minX, y: card.minY + 18, width: card.width, height: card.height - 32).fill()
         AppKitProductionPalette.surface.setFill()
         CGRect(x: card.minX + 16, y: card.minY, width: card.width - 32, height: card.height).fill()
         CGRect(x: card.minX, y: card.minY + 16, width: card.width, height: card.height - 32).fill()
         modalEdges.draw(in: card)
      }
      else
      {
         AppKitProductionPalette.shadow.setFill()
         NSBezierPath(roundedRect: card.offsetBy(dx: 0, dy: 2), xRadius: 16, yRadius: 16).fill()
         AppKitProductionPalette.surface.setFill()
         NSBezierPath(roundedRect: card, xRadius: 16, yRadius: 16).fill()
      }
      appKitProductionModalBottomShadowColor(progress: progress).setFill()
      CGRect(x: 16, y: 580, width: bounds.width - 32, height: 2).fill()
      AppKitProductionPalette.accent.setFill()
      let dismiss = CGRect(x: 268, y: 18, width: 50, height: 28)
      if let dismissEdges
      {
         CGRect(x: dismiss.minX + 10, y: dismiss.minY, width: dismiss.width - 20, height: dismiss.height).fill()
         CGRect(x: dismiss.minX, y: dismiss.minY + 10, width: dismiss.width, height: dismiss.height - 20).fill()
         dismissEdges.draw(in: dismiss)
      }
      else
      {
         NSBezierPath(roundedRect: dismiss, xRadius: 10, yRadius: 10).fill()
      }
   }
}

func appKitProductionModalBottomShadowColor(progress: CGFloat) -> NSColor
{
   let overlayAlpha = Double(176) / 255 * Double(min(1, max(0, progress)))
   let shadowAlpha = Double(41) / 255
   let background = [243, 245, 248]
   let overlay = [25, 28, 35]
   let shadow = [32, 36, 44]
   let channels = (0..<3).map
   {
      channel -> CGFloat in
      let underlay = appKitProductionLinearToSRGB8(
         appKitProductionSRGB8ToLinear(overlay[channel]) * overlayAlpha
            + appKitProductionSRGB8ToLinear(background[channel]) * (1 - overlayAlpha)
      )
      let composed = appKitProductionSRGB8ToLinear(shadow[channel]) * shadowAlpha
         + appKitProductionSRGB8ToLinear(underlay) * (1 - shadowAlpha)
      return CGFloat(appKitProductionLinearToSRGB8(composed)) / 255
   }
   return NSColor(srgbRed: channels[0], green: channels[1], blue: channels[2], alpha: 1)
}

private func appKitProductionSRGB8ToLinear(_ value: Int) -> Double
{
   let encoded = Double(value) / 255
   return encoded <= 0.04045 ? encoded / 12.92 : pow((encoded + 0.055) / 1.055, 2.4)
}

private func appKitProductionLinearToSRGB8(_ value: Double) -> Int
{
   let linear = min(1, max(0, value))
   let encoded = linear <= 0.0031308 ? linear * 12.92 : 1.055 * pow(linear, 1 / 2.4) - 0.055
   return min(255, max(0, Int((encoded * 255).rounded(.toNearestOrEven))))
}

final class AppKitStartupProductionSceneRoot: NSObject, AppKitProductionSceneRoot
{
   static let sceneID = AppKitProductionSceneID.startupFirstScreen
   static let componentContracts = [
      AppKitProductionComponentContract(identifier: "startup.header", componentClass: NSTextField.self),
      AppKitProductionComponentContract(identifier: "startup.navigation", componentClass: AppKitProductionRoundedSurfaceView.self),
      AppKitProductionComponentContract(identifier: "startup.cards-scroll", componentClass: NSScrollView.self),
      AppKitProductionComponentContract(identifier: "startup.cards-clip", componentClass: NSClipView.self),
      AppKitProductionComponentContract(identifier: "startup.cards", componentClass: NSCollectionView.self),
      AppKitProductionComponentContract(identifier: "startup.primary-action", componentClass: NSButton.self),
      AppKitProductionComponentContract(identifier: "startup.primary-label", componentClass: NSTextField.self),
      AppKitProductionComponentContract(identifier: "startup.launch-probe-response", componentClass: AppKitProductionSurfaceView.self),
   ]
   static let reusableComponentContracts = [
      AppKitProductionReusableComponentContract(role: "card", componentClass: AppKitStartupCardCollectionViewItem.self),
   ]

   let storage: AppKitProductionSceneStorage
   let header: AppKitProductionExactTextField
   let navigation: AppKitProductionRoundedSurfaceView
   let navigationLabel: AppKitProductionExactTextField
   let cards: NSCollectionView
   let cardsScrollView: NSScrollView
   let primaryAction: AppKitProductionSolidButton
   let primaryActionLabel: AppKitProductionExactTextField
   let launchProbeResponsePatch: AppKitProductionSurfaceView
   let collectionController = AppKitStartupCollectionController()
   var onPrimaryAction: (() -> Void)?

   init(
      headingFont: NSFont = NSFont.systemFont(ofSize: 20),
      bodyFont: NSFont = NSFont.systemFont(ofSize: 15),
      controlFont: NSFont = NSFont.systemFont(ofSize: 15)
   )
   {
      let storage = AppKitProductionSceneStorage(sceneID: Self.sceneID)
      storage.useDirectLayout()
      let header = AppKitProductionExactTextField(frame: CGRect(x: 16, y: 20, width: 358, height: 48))
      header.configure(identifier: "startup.header", value: "Production Comparison", font: headingFont, color: AppKitProductionPalette.text)
      let navigation = AppKitProductionRoundedSurfaceView(frame: CGRect(x: 16, y: 76, width: 358, height: 44))
      navigation.identifier = NSUserInterfaceItemIdentifier("startup.navigation")
      navigation.setAccessibilityIdentifier("startup.navigation")
      navigation.setAccessibilityLabel("navigation")
      navigation.setAccessibilityRole(.group)
      let navigationLabel = AppKitProductionExactTextField(frame: navigation.bounds)
      navigationLabel.configure(identifier: "startup.navigation-label", value: "First Screen", font: bodyFont, color: AppKitProductionPalette.secondaryText)
      navigation.addSubview(navigationLabel)
      let cards = storage.makeCollection(identifier: "startup.cards")
      let layout = cards.collectionViewLayout as! NSCollectionViewFlowLayout
      layout.itemSize = NSSize(width: 173, height: 188)
      layout.minimumInteritemSpacing = 12
      layout.minimumLineSpacing = 0
      layout.sectionInset = NSEdgeInsetsZero
      layout.scrollDirection = .vertical
      cards.backgroundColors = [.clear]
      cards.isSelectable = false
      cards.frame = CGRect(x: 0, y: 0, width: 358, height: 564)
      cards.translatesAutoresizingMaskIntoConstraints = true
      cards.autoresizingMask = []
      cards.register(AppKitStartupCardCollectionViewItem.self, forItemWithIdentifier: AppKitStartupCardCollectionViewItem.reuseIdentifier)
      let cardsScrollView = storage.makeScrollView(identifier: "startup.cards-scroll", clipIdentifier: "startup.cards-clip")
      cardsScrollView.frame = CGRect(x: 16, y: 132, width: 358, height: 552)
      cardsScrollView.translatesAutoresizingMaskIntoConstraints = true
      cardsScrollView.autoresizingMask = []
      cardsScrollView.borderType = .noBorder
      cardsScrollView.drawsBackground = false
      cardsScrollView.hasHorizontalScroller = false
      cardsScrollView.hasVerticalScroller = false
      cardsScrollView.horizontalScrollElasticity = .none
      cardsScrollView.verticalScrollElasticity = .none
      cardsScrollView.documentView = cards
      let primaryAction = AppKitProductionSolidButton(frame: CGRect(x: 16, y: 776, width: 358, height: 48))
      primaryAction.identifier = NSUserInterfaceItemIdentifier("startup.primary-action")
      primaryAction.setAccessibilityIdentifier("startup.primary-action")
      primaryAction.setAccessibilityLabel("Continue")
      let primaryActionLabel = AppKitProductionExactTextField(frame: CGRect(x: -1.0 / 3.0, y: 1.0 / 3.0, width: 358, height: 48))
      primaryActionLabel.configure(identifier: "startup.primary-label", value: "Continue", font: controlFont, color: AppKitProductionPalette.surface)
      primaryActionLabel.alignment = .center
      primaryAction.addSubview(primaryActionLabel)
      let launchProbeResponsePatch = AppKitProductionSurfaceView(frame: CGRect(x: 16, y: 776, width: 24, height: 24))
      launchProbeResponsePatch.identifier = NSUserInterfaceItemIdentifier("startup.launch-probe-response")
      launchProbeResponsePatch.isHidden = true
      self.storage = storage
      self.header = header
      self.navigation = navigation
      self.navigationLabel = navigationLabel
      self.cards = cards
      self.cardsScrollView = cardsScrollView
      self.primaryAction = primaryAction
      self.primaryActionLabel = primaryActionLabel
      self.launchProbeResponsePatch = launchProbeResponsePatch
      super.init()
      storage.rootView.addSubview(header)
      storage.rootView.addSubview(navigation)
      storage.rootView.addSubview(cardsScrollView)
      storage.rootView.addSubview(primaryAction)
      storage.rootView.addSubview(launchProbeResponsePatch)
      collectionController.install(on: cards)
      primaryAction.target = self
      primaryAction.action = #selector(performPrimaryAction)
   }

   func replaceCards(_ records: [AppKitStartupCardRecord])
   {
      collectionController.replace(with: records, in: cards)
   }

   func teardown()
   {
      collectionController.uninstall(from: cards)
      cardsScrollView.documentView = nil
      storage.nativeControls.removeActionBindings()
      onPrimaryAction = nil
      launchProbeResponsePatch.isHidden = true
   }

   func showLaunchProbeResponse()
   {
      launchProbeResponsePatch.isHidden = false
      launchProbeResponsePatch.needsDisplay = true
   }

   @objc private func performPrimaryAction()
   {
      onPrimaryAction?()
   }
}

final class AppKitDashboardProductionSceneRoot: NSObject, AppKitProductionSceneRoot
{
   static let sceneID = AppKitProductionSceneID.dashboardMixedStatic
   static let componentContracts = [
      AppKitProductionComponentContract(identifier: "dashboard.metrics", componentClass: NSCollectionView.self),
      AppKitProductionComponentContract(identifier: "dashboard.metrics-scroll", componentClass: NSScrollView.self),
   ]
   static let reusableComponentContracts = [
      AppKitProductionReusableComponentContract(role: "rounded-card", componentClass: AppKitDashboardCardCollectionViewItem.self),
   ]

   let storage: AppKitProductionSceneStorage
   let metrics: NSCollectionView
   let scrollView: NSScrollView
   let backdropViews: [AppKitProductionBackdropView]
   let collectionController = AppKitDashboardCollectionController()

   override init()
   {
      let storage = AppKitProductionSceneStorage(sceneID: Self.sceneID)
      storage.useDirectLayout()
      let metrics = storage.makeCollection(identifier: "dashboard.metrics")
      let layout = metrics.collectionViewLayout as! NSCollectionViewFlowLayout
      layout.itemSize = NSSize(width: 173, height: 40)
      layout.minimumInteritemSpacing = 12
      layout.minimumLineSpacing = 6
      layout.sectionInset = NSEdgeInsetsZero
      metrics.backgroundColors = [.clear]
      metrics.isSelectable = false
      metrics.frame = CGRect(x: 0, y: 0, width: 358, height: 730)
      metrics.register(AppKitDashboardCardCollectionViewItem.self, forItemWithIdentifier: AppKitDashboardCardCollectionViewItem.reuseIdentifier)
      let scrollView = NSScrollView(frame: CGRect(x: 16, y: 48, width: 358, height: 730))
      scrollView.identifier = NSUserInterfaceItemIdentifier("dashboard.metrics-scroll")
      scrollView.setAccessibilityIdentifier("dashboard.metrics-scroll")
      scrollView.borderType = .noBorder
      scrollView.drawsBackground = false
      scrollView.hasHorizontalScroller = false
      scrollView.hasVerticalScroller = false
      scrollView.contentView.drawsBackground = false
      scrollView.documentView = metrics
      let backdropViews = (0..<4).map
      {
         index -> AppKitProductionBackdropView in
         let view = AppKitProductionBackdropView(frame: CGRect(x: 16, y: 64 + CGFloat(index) * 204, width: 358, height: 96))
         view.identifier = NSUserInterfaceItemIdentifier("dashboard:backdrop:\(index)")
         view.setAccessibilityIdentifier("dashboard:backdrop:\(index)")
         view.setAccessibilityLabel("backdrop-region")
         view.setAccessibilityRole(.group)
         return view
      }
      self.storage = storage
      self.metrics = metrics
      self.scrollView = scrollView
      self.backdropViews = backdropViews
      super.init()
      backdropViews.forEach {storage.rootView.addSubview($0)}
      storage.rootView.addSubview(scrollView)
      collectionController.install(on: metrics)
   }

   func replaceMetrics(_ records: [AppKitDashboardCardRecord])
   {
      collectionController.replace(with: records, in: metrics)
   }

   func teardown()
   {
      collectionController.uninstall(from: metrics)
      scrollView.documentView = nil
      storage.nativeControls.removeActionBindings()
   }
}

final class AppKitEnduranceProductionSceneRoot: NSObject, AppKitProductionSceneRoot
{
   static let sceneID = AppKitProductionSceneID.enduranceChurn
   static let componentContracts = [
      AppKitProductionComponentContract(identifier: "endurance.metrics", componentClass: NSCollectionView.self),
      AppKitProductionComponentContract(identifier: "endurance.metrics-scroll", componentClass: NSScrollView.self),
   ]
   static let reusableComponentContracts = [
      AppKitProductionReusableComponentContract(role: "rounded-card", componentClass: AppKitDashboardCardCollectionViewItem.self),
   ]

   let storage: AppKitProductionSceneStorage
   let metrics: NSCollectionView
   let scrollView: NSScrollView
   let backdropViews: [AppKitProductionBackdropView]
   let collectionController = AppKitDashboardCollectionController()

   override init()
   {
      let storage = AppKitProductionSceneStorage(sceneID: Self.sceneID)
      storage.useDirectLayout()
      let metrics = storage.makeCollection(identifier: "endurance.metrics")
      let layout = metrics.collectionViewLayout as! NSCollectionViewFlowLayout
      layout.itemSize = NSSize(width: 173, height: 40)
      layout.minimumInteritemSpacing = 12
      layout.minimumLineSpacing = 6
      layout.sectionInset = NSEdgeInsetsZero
      metrics.backgroundColors = [.clear]
      metrics.isSelectable = false
      metrics.frame = CGRect(x: 0, y: 0, width: 358, height: 730)
      metrics.register(AppKitDashboardCardCollectionViewItem.self, forItemWithIdentifier: AppKitDashboardCardCollectionViewItem.reuseIdentifier)
      let scrollView = NSScrollView(frame: CGRect(x: 16, y: 48, width: 358, height: 730))
      scrollView.identifier = NSUserInterfaceItemIdentifier("endurance.metrics-scroll")
      scrollView.setAccessibilityIdentifier("endurance.metrics-scroll")
      scrollView.borderType = .noBorder
      scrollView.drawsBackground = false
      scrollView.hasHorizontalScroller = false
      scrollView.hasVerticalScroller = false
      scrollView.contentView.drawsBackground = false
      scrollView.documentView = metrics
      let backdropViews = (0..<4).map
      {
         index -> AppKitProductionBackdropView in
         let view = AppKitProductionBackdropView(frame: CGRect(x: 16, y: 64 + CGFloat(index) * 204, width: 358, height: 96))
         view.identifier = NSUserInterfaceItemIdentifier("endurance:backdrop:\(index)")
         view.setAccessibilityIdentifier("endurance:backdrop:\(index)")
         view.setAccessibilityLabel("backdrop-region")
         view.setAccessibilityRole(.group)
         return view
      }
      self.storage = storage
      self.metrics = metrics
      self.scrollView = scrollView
      self.backdropViews = backdropViews
      super.init()
      backdropViews.forEach {storage.rootView.addSubview($0)}
      storage.rootView.addSubview(scrollView)
      collectionController.install(on: metrics)
   }

   func present(records: [AppKitDashboardCardRecord], visible: Bool)
   {
      collectionController.replace(with: visible ? records : [], in: metrics)
      scrollView.isHidden = !visible
      backdropViews.forEach {$0.isHidden = !visible}
   }

   func teardown()
   {
      collectionController.uninstall(from: metrics)
      scrollView.documentView = nil
      storage.nativeControls.removeActionBindings()
   }
}

final class AppKitFeedProductionSceneRoot: NSObject, AppKitProductionSceneRoot
{
   static let sceneID = AppKitProductionSceneID.feedVariableScroll
   static let componentContracts = [
      AppKitProductionComponentContract(identifier: "feed.navigation-bar", componentClass: AppKitProductionSurfaceView.self),
      AppKitProductionComponentContract(identifier: "feed.navigation-title", componentClass: NSTextField.self),
      AppKitProductionComponentContract(identifier: "feed.scroll", componentClass: NSScrollView.self),
      AppKitProductionComponentContract(identifier: "feed.clip", componentClass: NSClipView.self),
      AppKitProductionComponentContract(identifier: "feed.table", componentClass: NSTableView.self),
   ]
   static let reusableComponentContracts = [
      AppKitProductionReusableComponentContract(role: "feed-card", componentClass: AppKitFeedTableCellView.self),
   ]

   let storage: AppKitProductionSceneStorage
   let navigationBar: AppKitProductionSurfaceView
   let navigationTitle: AppKitProductionExactTextField
   let table: NSTableView
   let scrollView: NSScrollView
   let tableController = AppKitFeedTableController()

   init(headingFont: NSFont = NSFont.systemFont(ofSize: 20))
   {
      let storage = AppKitProductionSceneStorage(sceneID: Self.sceneID)
      storage.useDirectLayout()
      let navigationBar = AppKitProductionSurfaceView(frame: CGRect(x: 0, y: 0, width: 390, height: 52))
      navigationBar.identifier = NSUserInterfaceItemIdentifier("feed.navigation-bar")
      navigationBar.setAccessibilityIdentifier("feed.navigation-bar")
      navigationBar.setAccessibilityLabel("navigation-bar")
      navigationBar.setAccessibilityRole(.group)
      let navigationTitle = AppKitProductionExactTextField(frame: CGRect(x: 16, y: 12, width: 220, height: 28))
      navigationTitle.configure(
         identifier: "feed.navigation-title",
         value: "Measured Feed",
         font: headingFont,
         color: AppKitProductionPalette.text
      )
      navigationBar.addSubview(navigationTitle)
      let table = storage.makeTable(identifier: "feed.table")
      table.style = .plain
      table.backgroundColor = AppKitProductionPalette.background
      table.columnAutoresizingStyle = .noColumnAutoresizing
      table.tableColumns[0].width = 390
      table.tableColumns[0].minWidth = 390
      table.tableColumns[0].maxWidth = 390
      table.frame = CGRect(x: 0, y: 0, width: 390, height: 792)
      table.translatesAutoresizingMaskIntoConstraints = true
      table.autoresizingMask = []
      let scrollView = storage.makeScrollView(identifier: "feed.scroll", clipIdentifier: "feed.clip")
      scrollView.frame = CGRect(x: 0, y: 52, width: 390, height: 792)
      scrollView.translatesAutoresizingMaskIntoConstraints = true
      scrollView.autoresizingMask = []
      scrollView.borderType = .noBorder
      scrollView.backgroundColor = AppKitProductionPalette.background
      scrollView.contentView.backgroundColor = AppKitProductionPalette.background
      scrollView.hasHorizontalScroller = false
      scrollView.hasVerticalScroller = false
      scrollView.autohidesScrollers = true
      scrollView.automaticallyAdjustsContentInsets = false
      scrollView.contentInsets = NSEdgeInsetsZero
      scrollView.horizontalScrollElasticity = .none
      scrollView.verticalScrollElasticity = .none
      scrollView.documentView = table
      self.storage = storage
      self.navigationBar = navigationBar
      self.navigationTitle = navigationTitle
      self.table = table
      self.scrollView = scrollView
      super.init()
      storage.rootView.addSubview(navigationBar)
      storage.rootView.addSubview(scrollView)
      tableController.install(on: table)
   }

   func replaceRows(_ records: [AppKitFeedRowRecord])
   {
      tableController.replace(with: records, in: table)
   }

   func prependRows(_ records: [AppKitFeedRowRecord])
   {
      tableController.prepend(records, in: table)
   }

   @discardableResult
   func updateRow(id: String, record: AppKitFeedRowRecord) -> Bool
   {
      tableController.update(id: id, record: record, in: table)
   }

   func applyScrollOffset(_ offset: CGFloat)
   {
      scrollView.contentView.scroll(to: NSPoint(x: 0, y: offset))
      scrollView.reflectScrolledClipView(scrollView.contentView)
      table.needsDisplay = true
   }

   func teardown()
   {
      tableController.uninstall(from: table)
      scrollView.documentView = nil
      storage.nativeControls.removeActionBindings()
   }
}

final class AppKitChatProductionSceneRoot: NSObject, AppKitProductionSceneRoot
{
   static let sceneID = AppKitProductionSceneID.chatLiveUpdate
   static let componentContracts = [
      AppKitProductionComponentContract(identifier: "chat.thread-scroll", componentClass: NSScrollView.self),
      AppKitProductionComponentContract(identifier: "chat.thread-clip", componentClass: NSClipView.self),
      AppKitProductionComponentContract(identifier: "chat.thread", componentClass: NSTableView.self),
      AppKitProductionComponentContract(identifier: "chat.composer-scroll", componentClass: NSScrollView.self),
      AppKitProductionComponentContract(identifier: "chat.composer-clip", componentClass: NSClipView.self),
      AppKitProductionComponentContract(identifier: "chat.composer", componentClass: AppKitOwnedTextView.self),
      AppKitProductionComponentContract(identifier: "chat.send", componentClass: NSButton.self),
   ]
   static let reusableComponentContracts = [
      AppKitProductionReusableComponentContract(role: "message", componentClass: AppKitChatTableCellView.self),
   ]

   let storage: AppKitProductionSceneStorage
   let heading: AppKitProductionExactTextField
   let thread: NSTableView
   let threadScrollView: NSScrollView
   let composerSurface: AppKitProductionRoundedSurfaceView
   let composer: AppKitOwnedTextView
   let composerScrollView: NSScrollView
   let composerPresentation: AppKitProductionExactTextField
   let sendButton: NSButton
   let sendPresentation: AppKitProductionExactTextField
   let tableController = AppKitChatTableController()
   var onSend: ((String) -> Void)?
   var onComposerInput: ((String) -> Void)?

   init(headingFont: NSFont = NSFont.systemFont(ofSize: 20), sendFont: NSFont = NSFont.systemFont(ofSize: 13))
   {
      let storage = AppKitProductionSceneStorage(sceneID: Self.sceneID)
      storage.useDirectLayout()
      let heading = AppKitProductionExactTextField(frame: CGRect(x: 16, y: 8, width: 358, height: 36))
      heading.configure(
         identifier: "chat.heading",
         value: "Live Chat",
         font: headingFont,
         color: AppKitProductionPalette.text
      )
      let thread = storage.makeTable(identifier: "chat.thread")
      thread.translatesAutoresizingMaskIntoConstraints = true
      thread.style = .plain
      thread.backgroundColor = .clear
      thread.enclosingScrollView?.drawsBackground = false
      thread.columnAutoresizingStyle = .noColumnAutoresizing
      thread.tableColumns[0].width = 390
      thread.tableColumns[0].minWidth = 390
      thread.tableColumns[0].maxWidth = 390
      let threadScrollView = storage.makeScrollView(identifier: "chat.thread-scroll", clipIdentifier: "chat.thread-clip")
      threadScrollView.frame = CGRect(x: 0, y: 52, width: 390, height: 700)
      threadScrollView.translatesAutoresizingMaskIntoConstraints = true
      threadScrollView.hasHorizontalScroller = false
      threadScrollView.hasVerticalScroller = false
      threadScrollView.horizontalScrollElasticity = .none
      threadScrollView.verticalScrollElasticity = .none
      threadScrollView.borderType = .noBorder
      threadScrollView.automaticallyAdjustsContentInsets = false
      threadScrollView.contentInsets = NSEdgeInsetsZero
      threadScrollView.documentView = thread
      let composerSurface = AppKitProductionRoundedSurfaceView(frame: CGRect(x: 12, y: 780, width: 318, height: 48))
      let composer = storage.nativeControls.makeTextView(identifier: "chat.composer", accessibilityLabel: "Message")
      let composerScrollView = storage.makeScrollView(identifier: "chat.composer-scroll", clipIdentifier: "chat.composer-clip")
      composerScrollView.frame = CGRect(x: 12, y: 780, width: 318, height: 48)
      composerScrollView.translatesAutoresizingMaskIntoConstraints = true
      composerScrollView.hasHorizontalScroller = false
      composerScrollView.hasVerticalScroller = false
      composerScrollView.drawsBackground = false
      composerScrollView.automaticallyAdjustsContentInsets = false
      composerScrollView.contentInsets = NSEdgeInsetsZero
      composer.frame = CGRect(x: 0, y: 0, width: 318, height: 48)
      composer.translatesAutoresizingMaskIntoConstraints = true
      composer.drawsBackground = false
      composer.textColor = .clear
      composerScrollView.documentView = composer
      let composerPresentation = AppKitProductionExactTextField(frame: CGRect(x: 22, y: 788, width: 298, height: 32))
      let sendButton = storage.nativeControls.makeButton(identifier: "chat.send", title: "Send")
      sendButton.frame = CGRect(x: 338, y: 780, width: 40, height: 48)
      sendButton.translatesAutoresizingMaskIntoConstraints = true
      sendButton.title = ""
      sendButton.setAccessibilityLabel("Send")
      sendButton.cell?.setAccessibilityLabel("Send")
      sendButton.cell?.setAccessibilityRole(.button)
      sendButton.isBordered = false
      sendButton.isTransparent = true
      let sendLine = CTLineCreateWithAttributedString(NSAttributedString(
         string: "Send",
         attributes: [kCTFontAttributeName as NSAttributedString.Key: sendFont]
      ))
      let sendWidth = CGFloat(CTLineGetTypographicBounds(sendLine, nil, nil, nil))
      let sendPresentation = AppKitProductionExactTextField(frame: CGRect(
         x: 338 + (40 - sendWidth) / 2,
         y: 780,
         width: sendWidth,
         height: 48
      ))
      sendPresentation.configure(
         identifier: "chat.send.presentation",
         value: "Send",
         font: sendFont,
         color: AppKitProductionPalette.accent
      )
      self.storage = storage
      self.heading = heading
      self.thread = thread
      self.threadScrollView = threadScrollView
      self.composerSurface = composerSurface
      self.composer = composer
      self.composerScrollView = composerScrollView
      self.composerPresentation = composerPresentation
      self.sendButton = sendButton
      self.sendPresentation = sendPresentation
      super.init()
      storage.rootView.addSubview(threadScrollView)
      storage.rootView.addSubview(heading)
      storage.rootView.addSubview(composerSurface)
      storage.rootView.addSubview(composerScrollView)
      storage.rootView.addSubview(composerPresentation)
      storage.rootView.addSubview(sendButton)
      storage.rootView.addSubview(sendPresentation)
      tableController.install(on: thread)
      sendButton.target = self
      sendButton.action = #selector(performSend)
      composer.onInsertText =
      {
         [weak self] value in
         self?.onComposerInput?(value)
      }
   }

   func replaceMessages(_ records: [AppKitChatMessageRecord])
   {
      tableController.replace(with: records, in: thread)
   }

   func prependMessages(_ records: [AppKitChatMessageRecord])
   {
      let priorOrigin = threadScrollView.contentView.bounds.origin
      let insertedHeight = records.reduce(CGFloat.zero) {$0 + $1.height}
      tableController.prepend(records, in: thread)
      threadScrollView.contentView.scroll(to: NSPoint(x: priorOrigin.x, y: priorOrigin.y + insertedHeight))
      threadScrollView.reflectScrolledClipView(threadScrollView.contentView)
   }

   func appendMessage(_ record: AppKitChatMessageRecord)
   {
      tableController.append(record, in: thread)
   }

   @discardableResult
   func updateMessage(id: String, record: AppKitChatMessageRecord) -> Bool
   {
      tableController.update(id: id, record: record, in: thread)
   }

   func presentComposer(_ value: String, font: NSFont, inlineText: AppKitProductionInlineText?)
   {
      let visible = String(value.suffix(80))
      composer.string = value
      composerPresentation.configure(
         identifier: "chat.composer.presentation",
         value: visible,
         font: font,
         color: AppKitProductionPalette.text,
         inlineText: inlineText
      )
   }

   func teardown()
   {
      tableController.uninstall(from: thread)
      threadScrollView.documentView = nil
      composerScrollView.documentView = nil
      storage.nativeControls.removeActionBindings()
      onSend = nil
      onComposerInput = nil
   }

   @objc private func performSend()
   {
      onSend?(composer.string)
   }
}

enum AppKitNavigationInteractiveCancelPhase
{
   case changed
   case ended
}

final class AppKitNavigationInteractiveCancelGestureRecognizer: NSGestureRecognizer
{
   var canBegin: (() -> Bool)?
   var onInteractiveCancel: ((AppKitNavigationInteractiveCancelPhase, CGFloat) -> Void)?
   private var initialPoint: NSPoint?
   private var active = false

   override func mouseDown(with event: NSEvent)
   {
      guard canBegin?() == true else
      {
         state = .failed
         return
      }
      initialPoint = event.locationInWindow
      active = false
      state = .possible
   }

   override func mouseDragged(with event: NSEvent)
   {
      guard let view, let initialPoint else {return}
      let delta = NSPoint(x: event.locationInWindow.x - initialPoint.x, y: event.locationInWindow.y - initialPoint.y)
      if !active
      {
         guard delta.x < 0, abs(delta.x) > abs(delta.y) else {return}
         active = true
         state = .began
      }
      else
      {
         state = .changed
      }
      let point = view.convert(event.locationInWindow, from: nil)
      let progress = view.bounds.width > 0 ? max(0, min(1, point.x / view.bounds.width)) : 0
      onInteractiveCancel?(.changed, progress)
   }

   override func mouseUp(with event: NSEvent)
   {
      if active
      {
         onInteractiveCancel?(.ended, 0)
         state = .ended
      }
      else
      {
         state = .failed
      }
      initialPoint = nil
      active = false
   }

   override func reset()
   {
      initialPoint = nil
      active = false
      super.reset()
   }
}

final class AppKitNavigationProductionSceneRoot: NSObject, AppKitProductionSceneRoot
{
   static let sceneID = AppKitProductionSceneID.navigationModal
   static let componentContracts = [
      AppKitProductionComponentContract(identifier: "navigation.heading", componentClass: NSTextField.self),
      AppKitProductionComponentContract(identifier: "navigation.scroll", componentClass: NSScrollView.self),
      AppKitProductionComponentContract(identifier: "navigation.clip", componentClass: NSClipView.self),
      AppKitProductionComponentContract(identifier: "navigation.table", componentClass: NSTableView.self),
      AppKitProductionComponentContract(identifier: "navigation.detail", componentClass: AppKitNavigationDetailView.self),
      AppKitProductionComponentContract(identifier: "navigation.back", componentClass: NSButton.self),
      AppKitProductionComponentContract(identifier: "navigation.modal-action", componentClass: NSButton.self),
      AppKitProductionComponentContract(identifier: "navigation.overlay", componentClass: AppKitNavigationOverlayView.self),
      AppKitProductionComponentContract(identifier: "navigation.modal", componentClass: AppKitNavigationModalView.self),
      AppKitProductionComponentContract(identifier: "navigation.dismiss", componentClass: NSButton.self),
   ]
   static let reusableComponentContracts = [
      AppKitProductionReusableComponentContract(role: "list-item", componentClass: AppKitNavigationTableCellView.self),
   ]

   let storage: AppKitProductionSceneStorage
   let heading: AppKitProductionExactTextField
   let table: NSTableView
   let scrollView: NSScrollView
   let detailView: AppKitNavigationDetailView
   let overlayView: AppKitNavigationOverlayView
   let modalView: AppKitNavigationModalView
   let tableController = AppKitNavigationTableController()
   let interactiveCancelRecognizer = AppKitNavigationInteractiveCancelGestureRecognizer()
   var onBack: (() -> Void)?
   var onDismiss: (() -> Void)?
   var onPresentModal: (() -> Void)?
   var onInteractiveCancel: ((AppKitNavigationInteractiveCancelPhase, CGFloat) -> Void)?

   init(
      headingFont: NSFont = NSFont.systemFont(ofSize: 20),
      bodyFont: NSFont = NSFont.systemFont(ofSize: 15),
      secondaryFont: NSFont = NSFont.systemFont(ofSize: 11),
      controlFont: NSFont = NSFont.systemFont(ofSize: 13)
   )
   {
      let storage = AppKitProductionSceneStorage(sceneID: Self.sceneID)
      storage.useDirectLayout()
      let heading = AppKitProductionExactTextField(frame: CGRect(x: 16, y: 6, width: 358, height: 52))
      heading.configure(identifier: "navigation.heading", value: "Navigation", font: headingFont, color: AppKitProductionPalette.text)
      let table = storage.makeTable(identifier: "navigation.table")
      table.style = .plain
      table.backgroundColor = AppKitProductionPalette.background
      table.columnAutoresizingStyle = .noColumnAutoresizing
      table.tableColumns[0].width = 390
      table.tableColumns[0].minWidth = 390
      table.tableColumns[0].maxWidth = 390
      table.rowHeight = 62
      table.frame = CGRect(x: 0, y: 0, width: 390, height: 744)
      table.translatesAutoresizingMaskIntoConstraints = true
      table.autoresizingMask = []
      let scrollView = storage.makeScrollView(identifier: "navigation.scroll", clipIdentifier: "navigation.clip")
      scrollView.frame = CGRect(x: 0, y: 56, width: 390, height: 744)
      scrollView.translatesAutoresizingMaskIntoConstraints = true
      scrollView.autoresizingMask = []
      scrollView.borderType = .noBorder
      scrollView.backgroundColor = AppKitProductionPalette.background
      scrollView.contentView.backgroundColor = AppKitProductionPalette.background
      scrollView.hasHorizontalScroller = false
      scrollView.hasVerticalScroller = false
      scrollView.autohidesScrollers = true
      scrollView.automaticallyAdjustsContentInsets = false
      scrollView.contentInsets = NSEdgeInsetsZero
      scrollView.horizontalScrollElasticity = .none
      scrollView.verticalScrollElasticity = .none
      scrollView.documentView = table
      let detailView = AppKitNavigationDetailView(headingFont: headingFont, bodyFont: bodyFont, secondaryFont: secondaryFont)
      let overlayView = AppKitNavigationOverlayView(frame: CGRect(x: 0, y: 0, width: 390, height: 844))
      overlayView.identifier = NSUserInterfaceItemIdentifier("navigation.overlay")
      overlayView.setAccessibilityIdentifier("navigation.overlay")
      let modalView = AppKitNavigationModalView(headingFont: headingFont, bodyFont: secondaryFont, controlFont: controlFont)
      self.storage = storage
      self.heading = heading
      self.table = table
      self.scrollView = scrollView
      self.detailView = detailView
      self.overlayView = overlayView
      self.modalView = modalView
      super.init()
      storage.rootView.addSubview(heading)
      storage.rootView.addSubview(scrollView)
      storage.rootView.addSubview(detailView)
      storage.rootView.addSubview(overlayView)
      storage.rootView.addSubview(modalView)
      tableController.install(on: table)
      detailView.backButton.target = self
      detailView.backButton.action = #selector(performBack)
      detailView.modalButton.target = self
      detailView.modalButton.action = #selector(performPresentModal)
      modalView.dismissButton.target = self
      modalView.dismissButton.action = #selector(performDismiss)
      interactiveCancelRecognizer.onInteractiveCancel =
      {
         [weak self] phase, progress in
         self?.onInteractiveCancel?(phase, progress)
      }
      interactiveCancelRecognizer.canBegin = {[weak self] in self?.scrollView.isHidden == false}
      storage.rootView.addGestureRecognizer(interactiveCancelRecognizer)
      present(route: "list", modalProgress: 0, modalVisible: false)
   }

   func replaceDestinations(_ records: [AppKitNavigationDestinationRecord])
   {
      tableController.replace(with: records, in: table)
   }

   func present(route: String, modalProgress: CGFloat, modalVisible: Bool)
   {
      let listVisible = route == "list"
      heading.isHidden = !listVisible
      scrollView.isHidden = !listVisible
      detailView.isHidden = listVisible
      overlayView.isHidden = !modalVisible
      modalView.isHidden = !modalVisible
      overlayView.progress = modalProgress
      modalView.progress = modalProgress
      let modalX = ((24 + (1 - modalProgress) * 390) * 3).rounded() / 3
      modalView.frame = CGRect(x: modalX, y: 132, width: 342, height: 582)
      storage.rootView.needsLayout = true
      overlayView.needsDisplay = true
      modalView.needsDisplay = true
   }

   func teardown()
   {
      tableController.uninstall(from: table)
      scrollView.documentView = nil
      detailView.backButton.target = nil
      detailView.backButton.action = nil
      detailView.modalButton.target = nil
      detailView.modalButton.action = nil
      modalView.dismissButton.target = nil
      modalView.dismissButton.action = nil
      storage.rootView.removeGestureRecognizer(interactiveCancelRecognizer)
      interactiveCancelRecognizer.canBegin = nil
      interactiveCancelRecognizer.onInteractiveCancel = nil
      storage.nativeControls.removeActionBindings()
      onBack = nil
      onDismiss = nil
      onPresentModal = nil
      onInteractiveCancel = nil
   }

   @objc private func performBack()
   {
      onBack?()
   }

   @objc private func performDismiss()
   {
      onDismiss?()
   }

   @objc private func performPresentModal()
   {
      onPresentModal?()
   }

}

final class AppKitImageProductionSceneRoot: NSObject, AppKitProductionSceneRoot
{
   static let sceneID = AppKitProductionSceneID.imageDecodeZoom
   static let componentContracts = [
      AppKitProductionComponentContract(identifier: "image.scroll", componentClass: NSScrollView.self),
      AppKitProductionComponentContract(identifier: "image.clip", componentClass: NSClipView.self),
      AppKitProductionComponentContract(identifier: "image.canvas", componentClass: NSImageView.self),
      AppKitProductionComponentContract(identifier: "image.zoom", componentClass: NSSlider.self),
   ]

   let storage: AppKitProductionSceneStorage
   let heading: AppKitProductionExactTextField
   let scrollView: NSScrollView
   let imageView: AppKitProductionImageCanvas
   let zoomSlider: AppKitProductionZoomSlider
   var onZoom: ((Double) -> Void)?

   init(headingFont: NSFont = NSFont.systemFont(ofSize: 20))
   {
      let storage = AppKitProductionSceneStorage(sceneID: Self.sceneID)
      storage.useDirectLayout()
      let heading = AppKitProductionExactTextField(frame: CGRect(x: 18, y: 8, width: 204, height: 36))
      heading.configure(
         identifier: "image.heading",
         value: "Decode & Zoom",
         font: headingFont,
         color: AppKitProductionPalette.text
      )
      let scrollView = storage.makeScrollView(identifier: "image.scroll", clipIdentifier: "image.clip")
      scrollView.frame = CGRect(x: 0, y: 52, width: 390, height: 740)
      scrollView.translatesAutoresizingMaskIntoConstraints = true
      scrollView.borderType = .noBorder
      scrollView.hasHorizontalScroller = false
      scrollView.hasVerticalScroller = false
      scrollView.automaticallyAdjustsContentInsets = false
      scrollView.contentInsets = NSEdgeInsetsZero
      let imageView = AppKitProductionImageCanvas(frame: CGRect(x: 0, y: 0, width: 390, height: 740))
      imageView.identifier = NSUserInterfaceItemIdentifier("image.canvas")
      imageView.setAccessibilityIdentifier("image.canvas")
      imageView.setAccessibilityLabel("Image canvas")
      imageView.setAccessibilityRole(.image)
      scrollView.documentView = imageView
      let zoomSlider = storage.nativeControls.ownSlider(
         AppKitProductionZoomSlider(value: 1, minValue: 1, maxValue: 2, target: nil, action: nil),
         identifier: "image.zoom"
      )
      zoomSlider.frame = CGRect(x: 16, y: 800, width: 358, height: 28)
      zoomSlider.translatesAutoresizingMaskIntoConstraints = true
      self.storage = storage
      self.heading = heading
      self.scrollView = scrollView
      self.imageView = imageView
      self.zoomSlider = zoomSlider
      super.init()
      storage.rootView.addSubview(heading)
      storage.rootView.addSubview(scrollView)
      storage.rootView.addSubview(zoomSlider)
      zoomSlider.target = self
      zoomSlider.action = #selector(performZoom(_:))
   }

   func present(image: NSImage?, translation: CGPoint, scale: CGFloat)
   {
      imageView.present(image: image, translation: translation, scale: scale)
   }

   func prepareUpload(image: NSImage) throws
   {
      guard imageView.prepareUpload(image: image) else
      {
         throw AppKitProductionScenarioFailure.invalidFixture
      }
   }

   func teardown()
   {
      imageView.present(image: nil, translation: .zero, scale: 1)
      scrollView.documentView = nil
      storage.nativeControls.removeActionBindings()
      onZoom = nil
   }

   @objc private func performZoom(_ sender: NSSlider)
   {
      onZoom?(sender.doubleValue)
   }
}
