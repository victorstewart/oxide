import AppKit
import Foundation

enum AppKitProductionSceneID: String, CaseIterable
{
   case startupFirstScreen = "startup.first-screen"
   case dashboardMixedStatic = "dashboard.mixed-static"
   case enduranceChurn = "endurance.churn"
   case feedVariableScroll = "feed.variable-scroll"
   case chatLiveUpdate = "chat.live-update"
   case navigationModal = "navigation.modal"
   case imageDecodeZoom = "image.decode-zoom"
   case gridLargeScroll = "grid.large-scroll"
   case effectsLayers = "effects.layers"
   case mutationDamage = "mutation.damage"
   case textMultilingual = "text.multilingual"
   case resizeTheme = "resize.theme"
}

struct AppKitProductionComponentContract
{
   let identifier: String
   let componentClass: NSView.Type

   var componentClassName: String
   {
      NSStringFromClass(componentClass)
   }

   func isSatisfied(by view: NSView) -> Bool
   {
      view.identifier?.rawValue == identifier && view.isKind(of: componentClass)
   }
}

struct AppKitProductionReusableComponentContract
{
   let role: String
   let componentClass: AnyClass

   var componentClassName: String
   {
      NSStringFromClass(componentClass)
   }
}

protocol AppKitProductionSceneRoot: AnyObject
{
   static var sceneID: AppKitProductionSceneID {get}
   static var componentContracts: [AppKitProductionComponentContract] {get}
   static var reusableComponentContracts: [AppKitProductionReusableComponentContract] {get}

   var storage: AppKitProductionSceneStorage {get}
   func teardown()
}

extension AppKitProductionSceneRoot
{
   static var reusableComponentContracts: [AppKitProductionReusableComponentContract]
   {
      []
   }

   var rootView: NSView
   {
      storage.rootView
   }

   var nativeControls: AppKitNativeControlOwnership
   {
      storage.nativeControls
   }

   func component(for contract: AppKitProductionComponentContract) -> NSView?
   {
      rootView.appKitDescendant(with: contract.identifier)
   }

   func unsatisfiedComponentContracts() -> [AppKitProductionComponentContract]
   {
      Self.componentContracts.filter
      {
         contract in
         guard let view = component(for: contract) else {return true}
         return !contract.isSatisfied(by: view)
      }
   }
}

final class AppKitProductionSceneHost: NSViewController
{
   private(set) var activeScene: (any AppKitProductionSceneRoot)?
   private var activeSceneConstraints = [NSLayoutConstraint]()

   override func loadView()
   {
      let view = NSView(frame: .zero)
      view.identifier = NSUserInterfaceItemIdentifier("production-reference.host")
      self.view = view
   }

   func install(_ scene: any AppKitProductionSceneRoot)
   {
      loadViewIfNeeded()
      activeScene?.teardown()
      NSLayoutConstraint.deactivate(activeSceneConstraints)
      activeSceneConstraints.removeAll(keepingCapacity: true)
      activeScene?.rootView.removeFromSuperview()
      activeScene = scene

      let sceneView = scene.rootView
      sceneView.translatesAutoresizingMaskIntoConstraints = false
      view.addSubview(sceneView)
      activeSceneConstraints = [
         sceneView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
         sceneView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
         sceneView.topAnchor.constraint(equalTo: view.topAnchor),
         sceneView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
      ]
      NSLayoutConstraint.activate(activeSceneConstraints)
   }

   func uninstall()
   {
      activeScene?.teardown()
      NSLayoutConstraint.deactivate(activeSceneConstraints)
      activeSceneConstraints.removeAll(keepingCapacity: true)
      activeScene?.rootView.removeFromSuperview()
      activeScene = nil
   }
}

final class AppKitNativeControlOwnership
{
   private(set) var buttons = [NSButton]()
   private(set) var textFields = [NSTextField]()
   private(set) var sliders = [NSSlider]()
   private(set) var textViews = [AppKitOwnedTextView]()

   func makeButton(identifier: String, title: String) -> NSButton
   {
      let button = NSButton(title: title, target: nil, action: nil)
      configure(button, identifier: identifier, accessibilityLabel: title)
      button.setAccessibilityRole(.button)
      buttons.append(button)
      return button
   }

   func makeLabel(identifier: String, text: String) -> NSTextField
   {
      let field = NSTextField(labelWithString: text)
      configure(field, identifier: identifier, accessibilityLabel: text)
      textFields.append(field)
      return field
   }

   func makeTextField(identifier: String, placeholder: String) -> NSTextField
   {
      let field = NSTextField(string: "")
      field.placeholderString = placeholder
      configure(field, identifier: identifier, accessibilityLabel: placeholder)
      textFields.append(field)
      return field
   }

   func makeSlider(identifier: String, minimum: Double, maximum: Double, value: Double) -> NSSlider
   {
      let slider = NSSlider(value: value, minValue: minimum, maxValue: maximum, target: nil, action: nil)
      return ownSlider(slider, identifier: identifier)
   }

   func ownSlider<T: NSSlider>(_ slider: T, identifier: String) -> T
   {
      configure(slider, identifier: identifier, accessibilityLabel: identifier)
      sliders.append(slider)
      return slider
   }

   func makeTextView(identifier: String, accessibilityLabel: String) -> AppKitOwnedTextView
   {
      let textView = AppKitOwnedTextView(frame: .zero)
      textView.identifier = NSUserInterfaceItemIdentifier(identifier)
      textView.setAccessibilityIdentifier(identifier)
      textView.setAccessibilityLabel(accessibilityLabel)
      textView.setAccessibilityRole(.textArea)
      textView.isEditable = true
      textView.isSelectable = true
      textView.isRichText = false
      textView.allowsUndo = false
      textView.translatesAutoresizingMaskIntoConstraints = false
      textViews.append(textView)
      return textView
   }

   func removeActionBindings()
   {
      buttons.forEach
      {
         $0.target = nil
         $0.action = nil
      }
      textFields.forEach
      {
         $0.delegate = nil
         $0.target = nil
         $0.action = nil
      }
      sliders.forEach
      {
         $0.target = nil
         $0.action = nil
      }
      textViews.forEach
      {
         $0.delegate = nil
         $0.onInsertText = nil
      }
   }

   private func configure(_ control: NSControl, identifier: String, accessibilityLabel: String)
   {
      control.identifier = NSUserInterfaceItemIdentifier(identifier)
      control.setAccessibilityIdentifier(identifier)
      control.setAccessibilityLabel(accessibilityLabel)
      control.translatesAutoresizingMaskIntoConstraints = false
   }
}

final class AppKitProductionSceneStorage
{
   let rootView: NSView
   let contentStack: NSStackView
   let nativeControls = AppKitNativeControlOwnership()
   private let contentStackConstraints: [NSLayoutConstraint]

   init(sceneID: AppKitProductionSceneID)
   {
      let rootView = AppKitProductionRootView(frame: .zero)
      rootView.identifier = NSUserInterfaceItemIdentifier("production-reference.\(sceneID.rawValue).root")
      rootView.setAccessibilityIdentifier("production-reference.\(sceneID.rawValue).root")

      let contentStack = NSStackView()
      contentStack.identifier = NSUserInterfaceItemIdentifier("production-reference.\(sceneID.rawValue).content")
      contentStack.orientation = .vertical
      contentStack.alignment = .leading
      contentStack.distribution = .fill
      contentStack.spacing = 12
      contentStack.edgeInsets = NSEdgeInsets(top: 16, left: 16, bottom: 16, right: 16)
      contentStack.translatesAutoresizingMaskIntoConstraints = false
      rootView.addSubview(contentStack)
      let contentStackConstraints = [
         contentStack.leadingAnchor.constraint(equalTo: rootView.leadingAnchor),
         contentStack.trailingAnchor.constraint(equalTo: rootView.trailingAnchor),
         contentStack.topAnchor.constraint(equalTo: rootView.topAnchor),
         contentStack.bottomAnchor.constraint(equalTo: rootView.bottomAnchor),
      ]
      NSLayoutConstraint.activate(contentStackConstraints)

      self.rootView = rootView
      self.contentStack = contentStack
      self.contentStackConstraints = contentStackConstraints
   }

   func useDirectLayout()
   {
      NSLayoutConstraint.deactivate(contentStackConstraints)
      contentStack.removeFromSuperview()
   }

   func addArrangedSubview(_ view: NSView)
   {
      contentStack.addArrangedSubview(view)
   }

   func makeCollection(identifier: String) -> NSCollectionView
   {
      let layout = NSCollectionViewFlowLayout()
      layout.itemSize = NSSize(width: 240, height: 56)
      layout.minimumLineSpacing = 8

      let collection = NSCollectionView(frame: .zero)
      collection.identifier = NSUserInterfaceItemIdentifier(identifier)
      collection.setAccessibilityIdentifier(identifier)
      collection.setAccessibilityLabel(identifier)
      collection.collectionViewLayout = layout
      collection.isSelectable = true
      collection.translatesAutoresizingMaskIntoConstraints = false
      return collection
   }

   func makeTable(identifier: String) -> NSTableView
   {
      let table = NSTableView(frame: .zero)
      table.identifier = NSUserInterfaceItemIdentifier(identifier)
      table.setAccessibilityIdentifier(identifier)
      table.setAccessibilityLabel(identifier)
      table.headerView = nil
      table.usesAlternatingRowBackgroundColors = false
      table.selectionHighlightStyle = .none
      table.rowHeight = 64
      table.intercellSpacing = .zero
      let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("\(identifier).content"))
      column.resizingMask = .autoresizingMask
      table.addTableColumn(column)
      table.translatesAutoresizingMaskIntoConstraints = false
      return table
   }

   func addCollection(_ collection: NSCollectionView, scrollIdentifier: String)
   {
      let scrollView = NSScrollView(frame: .zero)
      scrollView.identifier = NSUserInterfaceItemIdentifier(scrollIdentifier)
      scrollView.setAccessibilityIdentifier(scrollIdentifier)
      scrollView.hasVerticalScroller = true
      scrollView.drawsBackground = false
      scrollView.documentView = collection
      scrollView.translatesAutoresizingMaskIntoConstraints = false
      contentStack.addArrangedSubview(scrollView)
      NSLayoutConstraint.activate([
         scrollView.widthAnchor.constraint(equalTo: contentStack.widthAnchor),
         scrollView.heightAnchor.constraint(greaterThanOrEqualToConstant: 220),
      ])
   }

   @discardableResult
   func addTable(_ table: NSTableView, scrollIdentifier: String, clipIdentifier: String) -> NSScrollView
   {
      let scrollView = makeScrollView(identifier: scrollIdentifier, clipIdentifier: clipIdentifier)
      scrollView.documentView = table
      contentStack.addArrangedSubview(scrollView)
      constrainScrollableRegion(scrollView)
      return scrollView
   }

   func makeScrollView(identifier: String, clipIdentifier: String) -> NSScrollView
   {
      let scrollView = NSScrollView(frame: .zero)
      scrollView.identifier = NSUserInterfaceItemIdentifier(identifier)
      scrollView.setAccessibilityIdentifier(identifier)
      scrollView.hasVerticalScroller = true
      scrollView.drawsBackground = false
      scrollView.translatesAutoresizingMaskIntoConstraints = false

      let clipView = NSClipView(frame: .zero)
      clipView.identifier = NSUserInterfaceItemIdentifier(clipIdentifier)
      clipView.setAccessibilityIdentifier(clipIdentifier)
      clipView.drawsBackground = false
      scrollView.contentView = clipView
      return scrollView
   }

   func constrainScrollableRegion(_ scrollView: NSScrollView)
   {
      NSLayoutConstraint.activate([
         scrollView.widthAnchor.constraint(equalTo: contentStack.widthAnchor),
         scrollView.heightAnchor.constraint(greaterThanOrEqualToConstant: 220),
      ])
   }
}

struct AppKitCollectionItemReuseCounters: Equatable
{
   let bindingCount: UInt64
   let preparationCount: UInt64
   let cleanupCount: UInt64
}

class AppKitReusableCollectionViewItem: NSCollectionViewItem
{
   private(set) var representedIdentifier: String?
   private var bindingCount: UInt64 = 0
   private var preparationCount: UInt64 = 0
   private var cleanupCount: UInt64 = 0

   var reuseCounters: AppKitCollectionItemReuseCounters
   {
      AppKitCollectionItemReuseCounters(
         bindingCount: bindingCount,
         preparationCount: preparationCount,
         cleanupCount: cleanupCount
      )
   }

   override func loadView()
   {
      let view = NSView(frame: .zero)
      view.identifier = NSUserInterfaceItemIdentifier("production-reference.collection-item")
      self.view = view
   }

   final func bind(representedIdentifier: String)
   {
      self.representedIdentifier = representedIdentifier
      bindingCount += 1
      configureReusableContent(representedIdentifier: representedIdentifier)
   }

   override func prepareForReuse()
   {
      super.prepareForReuse()
      preparationCount += 1
      cleanupReusableContent()
   }

   func configureReusableContent(representedIdentifier: String)
   {
   }

   func didCleanupReusableContent()
   {
   }

   private func cleanupReusableContent()
   {
      representedIdentifier = nil
      representedObject = nil
      cleanupCount += 1
      appKitResetReusableTree(view)
      didCleanupReusableContent()
   }
}

struct AppKitTableCellReuseCounters: Equatable
{
   let bindingCount: UInt64
   let preparationCount: UInt64
   let cleanupCount: UInt64
}

class AppKitReusableTableCellView: NSTableCellView
{
   private(set) var representedIdentifier: String?
   private var bindingCount: UInt64 = 0
   private var preparationCount: UInt64 = 0
   private var cleanupCount: UInt64 = 0

   var reuseCounters: AppKitTableCellReuseCounters
   {
      AppKitTableCellReuseCounters(
         bindingCount: bindingCount,
         preparationCount: preparationCount,
         cleanupCount: cleanupCount
      )
   }

   final func bind(representedIdentifier: String)
   {
      self.representedIdentifier = representedIdentifier
      bindingCount += 1
      configureReusableContent(representedIdentifier: representedIdentifier)
   }

   override func prepareForReuse()
   {
      super.prepareForReuse()
      preparationCount += 1
      representedIdentifier = nil
      objectValue = nil
      cleanupCount += 1
      appKitResetReusableTree(self)
      didCleanupReusableContent()
   }

   func configureReusableContent(representedIdentifier: String)
   {
   }

   func didCleanupReusableContent()
   {
   }
}

final class AppKitOwnedTextView: NSTextView
{
   private(set) var insertionCount: UInt64 = 0
   private(set) var lastInsertedText: String?
   private(set) var ownedSelectionRange = NSRange(location: 0, length: 0)
   var onInsertText: ((String) -> Void)?

   override func insertText(_ insertString: Any, replacementRange: NSRange)
   {
      insertionCount += 1
      lastInsertedText = insertString as? String ?? (insertString as? NSAttributedString)?.string
      super.insertText(insertString, replacementRange: replacementRange)
      ownedSelectionRange = selectedRange()
      if let lastInsertedText
      {
         onInsertText?(lastInsertedText)
      }
   }

   override func setSelectedRange(_ charRange: NSRange)
   {
      super.setSelectedRange(charRange)
      ownedSelectionRange = selectedRange()
   }

   override func setSelectedRanges(_ ranges: [NSValue], affinity: NSSelectionAffinity, stillSelecting stillSelectingFlag: Bool)
   {
      super.setSelectedRanges(ranges, affinity: affinity, stillSelecting: stillSelectingFlag)
      ownedSelectionRange = selectedRange()
   }
}

struct AppKitNativeAccessibilityNodeEvidence
{
   let objectIdentifier: ObjectIdentifier
   let hierarchyDepth: Int
   let viewClass: AnyClass
   let identifier: String?
   let role: NSAccessibility.Role?
   let label: String?
   let isAccessibilityElement: Bool
}

enum AppKitNativeAccessibilityEvidence
{
   static func capture(from rootView: NSView) -> [AppKitNativeAccessibilityNodeEvidence]
   {
      var evidence = [AppKitNativeAccessibilityNodeEvidence]()
      var visited = Set<ObjectIdentifier>()
      append(rootView, depth: 0, visited: &visited, to: &evidence)
      return evidence
   }

   private static func append(_ object: AnyObject, depth: Int, visited: inout Set<ObjectIdentifier>, to evidence: inout [AppKitNativeAccessibilityNodeEvidence])
   {
      let objectIdentifier = ObjectIdentifier(object)
      guard visited.insert(objectIdentifier).inserted else {return}

      let identifier: String?
      let role: NSAccessibility.Role?
      let label: String?
      let isAccessibilityElement: Bool
      let children: [Any]
      switch object
      {
      case let view as NSView:
         let accessibilityIdentifier = view.accessibilityIdentifier()
         identifier = accessibilityIdentifier.isEmpty ? view.identifier?.rawValue : accessibilityIdentifier
         role = view.accessibilityRole()
         label = view.accessibilityLabel()
         isAccessibilityElement = view.isAccessibilityElement()
         children = view.accessibilityChildren() ?? []
      case let element as NSAccessibilityElement:
         identifier = element.accessibilityIdentifier()
         role = element.accessibilityRole()
         label = element.accessibilityLabel()
         isAccessibilityElement = element.isAccessibilityElement()
         children = element.accessibilityChildren() ?? []
      case let cell as NSCell:
         identifier = cell.accessibilityIdentifier()
         role = cell.accessibilityRole()
         label = cell.accessibilityLabel()
         isAccessibilityElement = cell.isAccessibilityElement()
         children = cell.accessibilityChildren() ?? []
      default:
         return
      }

      evidence.append(AppKitNativeAccessibilityNodeEvidence(
         objectIdentifier: objectIdentifier,
         hierarchyDepth: depth,
         viewClass: type(of: object),
         identifier: identifier,
         role: role,
         label: label,
         isAccessibilityElement: isAccessibilityElement
      ))
      children.forEach
      {
         guard let child = $0 as AnyObject? else {return}
         append(child, depth: depth + 1, visited: &visited, to: &evidence)
      }
   }
}

private extension NSView
{
   func appKitDescendant(with identifier: String) -> NSView?
   {
      if self.identifier?.rawValue == identifier || accessibilityIdentifier() == identifier
      {
         return self
      }
      for child in subviews
      {
         if let match = child.appKitDescendant(with: identifier)
         {
            return match
         }
      }
      return nil
   }

}

private func appKitResetReusableTree(_ view: NSView)
{
   appKitResetReusableView(view)
   view.subviews.forEach {appKitResetReusableTree($0)}
}

private func appKitResetReusableView(_ view: NSView)
{
   view.layer?.removeAllAnimations()
   switch view
   {
   case let button as NSButton:
      button.target = nil
      button.action = nil
      button.state = .off
   case let field as NSTextField:
      field.delegate = nil
      field.target = nil
      field.action = nil
      field.stringValue = ""
   case let textView as NSTextView:
      textView.delegate = nil
      textView.string = ""
      textView.setSelectedRange(NSRange(location: 0, length: 0))
   case let imageView as NSImageView:
      imageView.image = nil
   case let slider as NSSlider:
      slider.target = nil
      slider.action = nil
      slider.doubleValue = slider.minValue
   default:
      break
   }
}
