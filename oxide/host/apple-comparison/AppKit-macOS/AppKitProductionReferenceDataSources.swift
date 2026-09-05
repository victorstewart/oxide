import AppKit
import Foundation

struct AppKitStartupCardRecord
{
   let id: String
   let title: String
   let detail: String
   let thumbnail: NSImage?
   let thumbnailAtlas: NSImage?
   let thumbnailIndex: Int
   let titleFont: NSFont
   let detailFont: NSFont

   init(
      id: String,
      title: String,
      detail: String = "",
      thumbnail: NSImage? = nil,
      thumbnailAtlas: NSImage? = nil,
      thumbnailIndex: Int = 0,
      titleFont: NSFont = NSFont.systemFont(ofSize: 15),
      detailFont: NSFont = NSFont.systemFont(ofSize: 11)
   )
   {
      self.id = id
      self.title = title
      self.detail = detail
      self.thumbnail = thumbnail
      self.thumbnailAtlas = thumbnailAtlas
      self.thumbnailIndex = thumbnailIndex
      self.titleFont = titleFont
      self.detailFont = detailFont
   }
}

struct AppKitDashboardLabelRecord: Equatable
{
   let id: String
   let value: String
   let accent: Bool
}

struct AppKitDashboardCardRecord
{
   let id: String
   let labels: [AppKitDashboardLabelRecord]
   let leadingIcon: NSImage
   let trailingIcon: NSImage
   let font: NSFont
   let actionTarget: String?

   func hasSamePresentation(as other: AppKitDashboardCardRecord) -> Bool
   {
      id == other.id &&
      labels == other.labels &&
      leadingIcon === other.leadingIcon &&
      trailingIcon === other.trailingIcon &&
      font === other.font &&
      actionTarget == other.actionTarget
   }
}

struct AppKitFeedRowRecord
{
   let id: String
   let title: String
   let height: CGFloat
   let thumbnail: NSImage
   let thumbnailAtlas: NSImage
   let thumbnailIndex: Int
   let titleFont: NSFont
   let secondaryFont: NSFont
   let inlineText: AppKitProductionInlineText?
   let favorite: Bool

   func hasSamePresentation(as other: AppKitFeedRowRecord) -> Bool
   {
      id == other.id &&
      title == other.title &&
      height == other.height &&
      thumbnail === other.thumbnail &&
      thumbnailAtlas === other.thumbnailAtlas &&
      thumbnailIndex == other.thumbnailIndex &&
      titleFont === other.titleFont &&
      secondaryFont === other.secondaryFont &&
      inlineText === other.inlineText &&
      favorite == other.favorite
   }
}

struct AppKitChatMessageRecord
{
   let id: String
   let sequence: Int
   let direction: String
   let text: String
   let avatar: NSImage
   let avatarAtlas: NSImage
   let avatarIndex: Int
   let textFont: NSFont
   let inlineText: AppKitProductionInlineText?

   var height: CGFloat
   {
      let heights: [CGFloat] = [58, 62, 66, 70, 74, 78, 82, 76, 70, 64]
      return heights[sequence % heights.count]
   }

   func hasSamePresentation(as other: AppKitChatMessageRecord) -> Bool
   {
      id == other.id &&
      sequence == other.sequence &&
      direction == other.direction &&
      text == other.text &&
      avatar === other.avatar &&
      avatarAtlas === other.avatarAtlas &&
      avatarIndex == other.avatarIndex &&
      textFont === other.textFont &&
      inlineText === other.inlineText
   }
}

struct AppKitNavigationDestinationRecord
{
   let id: String
   let title: String
   let subtitle: String
   let titleFont: NSFont
   let subtitleFont: NSFont

   func hasSamePresentation(as other: AppKitNavigationDestinationRecord) -> Bool
   {
      id == other.id &&
      title == other.title &&
      subtitle == other.subtitle &&
      titleFont === other.titleFont &&
      subtitleFont === other.subtitleFont
   }
}

final class AppKitStartupCollectionController: NSObject, NSCollectionViewDataSource, NSCollectionViewDelegate
{
   private(set) var records = [AppKitStartupCardRecord]()

   func install(on collection: NSCollectionView)
   {
      collection.dataSource = self
      collection.delegate = self
   }

   func replace(with records: [AppKitStartupCardRecord], in collection: NSCollectionView)
   {
      self.records = records
      collection.reloadData()
   }

   func uninstall(from collection: NSCollectionView)
   {
      records.removeAll(keepingCapacity: false)
      collection.reloadData()
      collection.dataSource = nil
      collection.delegate = nil
   }

   func numberOfSections(in collectionView: NSCollectionView) -> Int
   {
      1
   }

   func collectionView(_ collectionView: NSCollectionView, numberOfItemsInSection section: Int) -> Int
   {
      records.count
   }

   func collectionView(_ collectionView: NSCollectionView, itemForRepresentedObjectAt indexPath: IndexPath) -> NSCollectionViewItem
   {
      let item = collectionView.makeItem(withIdentifier: AppKitStartupCardCollectionViewItem.reuseIdentifier, for: indexPath)
      guard let card = item as? AppKitStartupCardCollectionViewItem, indexPath.item < records.count else
      {
         return item
      }
      card.configure(record: records[indexPath.item])
      return card
   }
}

final class AppKitDashboardCollectionController: NSObject, NSCollectionViewDataSource, NSCollectionViewDelegate
{
   private(set) var records = [AppKitDashboardCardRecord]()
   var onAction: ((String) -> Void)?

   func install(on collection: NSCollectionView)
   {
      collection.dataSource = self
      collection.delegate = self
   }

   func replace(with records: [AppKitDashboardCardRecord], in collection: NSCollectionView)
   {
      let prior = self.records
      self.records = records
      guard prior.count == records.count, !prior.isEmpty else
      {
         collection.reloadData()
         return
      }
      for index in records.indices
      {
         guard !prior[index].hasSamePresentation(as: records[index]),
               let card = collection.item(at: IndexPath(item: index, section: 0)) as? AppKitDashboardCardCollectionViewItem else
         {
            continue
         }
         card.configure(record: records[index], target: self, action: #selector(performAction(_:)), row: index)
      }
   }

   func uninstall(from collection: NSCollectionView)
   {
      onAction = nil
      records.removeAll(keepingCapacity: false)
      collection.reloadData()
      collection.dataSource = nil
      collection.delegate = nil
   }

   func numberOfSections(in collectionView: NSCollectionView) -> Int
   {
      1
   }

   func collectionView(_ collectionView: NSCollectionView, numberOfItemsInSection section: Int) -> Int
   {
      records.count
   }

   func collectionView(_ collectionView: NSCollectionView, itemForRepresentedObjectAt indexPath: IndexPath) -> NSCollectionViewItem
   {
      let item = collectionView.makeItem(withIdentifier: AppKitDashboardCardCollectionViewItem.reuseIdentifier, for: indexPath)
      guard let card = item as? AppKitDashboardCardCollectionViewItem, indexPath.item < records.count else
      {
         return item
      }
      card.configure(record: records[indexPath.item], target: self, action: #selector(performAction(_:)), row: indexPath.item)
      return card
   }

   @objc private func performAction(_ sender: NSButton)
   {
      guard sender.tag >= 0, sender.tag < records.count else {return}
      guard let actionTarget = records[sender.tag].actionTarget else {return}
      onAction?(actionTarget)
   }
}

final class AppKitFeedTableController: NSObject, NSTableViewDataSource, NSTableViewDelegate
{
   private(set) var records = [AppKitFeedRowRecord]()
   private var baseRowByID = [String: Int]()
   private var prependedRowByID = [String: Int]()
   private var leadingRowCount = 0
   private(set) var fullReloadCount = 0
   private(set) var incrementalMutationCount = 0
   private(set) var linearComparisonCount = 0
   var onFavorite: ((String) -> Void)?

   func install(on table: NSTableView)
   {
      table.dataSource = self
      table.delegate = self
   }

   func replace(with records: [AppKitFeedRowRecord], in table: NSTableView)
   {
      self.records = records
      baseRowByID = Dictionary(uniqueKeysWithValues: records.enumerated().map {($0.element.id, $0.offset)})
      prependedRowByID.removeAll(keepingCapacity: true)
      leadingRowCount = 0
      fullReloadCount += 1
      table.reloadData()
      for row in records.indices
      {
         _ = table.rect(ofRow: row)
      }
   }

   func prepend(_ inserted: [AppKitFeedRowRecord], in table: NSTableView)
   {
      guard !inserted.isEmpty else {return}
      records.insert(contentsOf: inserted, at: 0)
      let priorPrependedRows = prependedRowByID
      for (id, row) in priorPrependedRows
      {
         prependedRowByID[id] = row + inserted.count
      }
      for (row, record) in inserted.enumerated()
      {
         prependedRowByID[record.id] = row
      }
      leadingRowCount += inserted.count
      incrementalMutationCount += 1
      table.insertRows(at: IndexSet(integersIn: 0..<inserted.count), withAnimation: [])
   }

   @discardableResult
   func update(id: String, record: AppKitFeedRowRecord, in table: NSTableView) -> Bool
   {
      guard let row = row(for: id), row >= 0, row < records.count else {return false}
      let prior = records[row]
      guard !prior.hasSamePresentation(as: record) else {return true}
      records[row] = record
      incrementalMutationCount += 1
      if prior.height != record.height
      {
         table.noteHeightOfRows(withIndexesChanged: IndexSet(integer: row))
      }
      if let cell = table.view(atColumn: 0, row: row, makeIfNecessary: false) as? AppKitFeedTableCellView
      {
         cell.configure(record: record, target: self, action: #selector(performFavorite(_:)), row: row)
      }
      return true
   }

   func row(for id: String) -> Int?
   {
      prependedRowByID[id] ?? baseRowByID[id].map {$0 + leadingRowCount}
   }

   func uninstall(from table: NSTableView)
   {
      onFavorite = nil
      records.removeAll(keepingCapacity: false)
      baseRowByID.removeAll(keepingCapacity: false)
      prependedRowByID.removeAll(keepingCapacity: false)
      leadingRowCount = 0
      table.reloadData()
      table.dataSource = nil
      table.delegate = nil
   }

   func numberOfRows(in tableView: NSTableView) -> Int
   {
      records.count
   }

   func tableView(_ tableView: NSTableView, heightOfRow row: Int) -> CGFloat
   {
      guard row >= 0, row < records.count else {return tableView.rowHeight}
      return records[row].height
   }

   func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView?
   {
      guard row >= 0, row < records.count else {return nil}
      let identifier = AppKitFeedTableCellView.reuseIdentifier
      let cell = tableView.makeView(withIdentifier: identifier, owner: self) as? AppKitFeedTableCellView
         ?? AppKitFeedTableCellView(frame: .zero)
      cell.identifier = identifier
      cell.configure(record: records[row], target: self, action: #selector(performFavorite(_:)), row: row)
      return cell
   }

   @objc private func performFavorite(_ sender: NSButton)
   {
      guard sender.tag >= 0, sender.tag < records.count else {return}
      onFavorite?(records[sender.tag].id)
   }
}

final class AppKitChatTableController: NSObject, NSTableViewDataSource, NSTableViewDelegate
{
   private(set) var records = [AppKitChatMessageRecord]()
   private var baseRowByID = [String: Int]()
   private var prependedRowByID = [String: Int]()
   private var leadingRowCount = 0
   private(set) var fullReloadCount = 0
   private(set) var incrementalMutationCount = 0
   private(set) var linearComparisonCount = 0
   var onMessageSelection: ((String, NSRange) -> Void)?
   var onMessageValueChange: ((String, String) -> Void)?

   func install(on table: NSTableView)
   {
      table.dataSource = self
      table.delegate = self
   }

   func replace(with records: [AppKitChatMessageRecord], in table: NSTableView)
   {
      self.records = records
      baseRowByID = Dictionary(uniqueKeysWithValues: records.enumerated().map {($0.element.id, $0.offset)})
      prependedRowByID.removeAll(keepingCapacity: true)
      leadingRowCount = 0
      fullReloadCount += 1
      table.reloadData()
   }

   func prepend(_ inserted: [AppKitChatMessageRecord], in table: NSTableView)
   {
      guard !inserted.isEmpty else {return}
      records.insert(contentsOf: inserted, at: 0)
      let priorPrependedRows = prependedRowByID
      for (id, row) in priorPrependedRows
      {
         prependedRowByID[id] = row + inserted.count
      }
      for (row, record) in inserted.enumerated()
      {
         prependedRowByID[record.id] = row
      }
      leadingRowCount += inserted.count
      incrementalMutationCount += 1
      table.insertRows(at: IndexSet(integersIn: 0..<inserted.count), withAnimation: [])
   }

   func append(_ record: AppKitChatMessageRecord, in table: NSTableView)
   {
      guard row(for: record.id) == nil else {return}
      let row = records.count
      records.append(record)
      baseRowByID[record.id] = row - leadingRowCount
      incrementalMutationCount += 1
      table.insertRows(at: IndexSet(integer: row), withAnimation: [])
   }

   @discardableResult
   func update(id: String, record: AppKitChatMessageRecord, in table: NSTableView) -> Bool
   {
      guard let row = row(for: id), row >= 0, row < records.count else {return false}
      let prior = records[row]
      guard !prior.hasSamePresentation(as: record) else {return true}
      records[row] = record
      incrementalMutationCount += 1
      if prior.height != record.height
      {
         table.noteHeightOfRows(withIndexesChanged: IndexSet(integer: row))
      }
      if let cell = table.view(atColumn: 0, row: row, makeIfNecessary: false) as? AppKitChatTableCellView
      {
         cell.onMessageSelection = onMessageSelection
         cell.onMessageValueChange = onMessageValueChange
         cell.configure(record: record)
      }
      return true
   }

   func row(for id: String) -> Int?
   {
      prependedRowByID[id] ?? baseRowByID[id].map {$0 + leadingRowCount}
   }

   func uninstall(from table: NSTableView)
   {
      records.removeAll(keepingCapacity: false)
      baseRowByID.removeAll(keepingCapacity: false)
      prependedRowByID.removeAll(keepingCapacity: false)
      leadingRowCount = 0
      table.reloadData()
      table.dataSource = nil
      table.delegate = nil
      onMessageSelection = nil
      onMessageValueChange = nil
   }

   func numberOfRows(in tableView: NSTableView) -> Int
   {
      records.count
   }

   func tableView(_ tableView: NSTableView, heightOfRow row: Int) -> CGFloat
   {
      guard row >= 0, row < records.count else {return tableView.rowHeight}
      return records[row].height
   }

   func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView?
   {
      guard row >= 0, row < records.count else {return nil}
      let identifier = AppKitChatTableCellView.reuseIdentifier
      let cell = tableView.makeView(withIdentifier: identifier, owner: self) as? AppKitChatTableCellView
         ?? AppKitChatTableCellView(frame: .zero)
      cell.identifier = identifier
      cell.onMessageSelection = onMessageSelection
      cell.onMessageValueChange = onMessageValueChange
      cell.configure(record: records[row])
      return cell
   }
}

final class AppKitNavigationTableController: NSObject, NSTableViewDataSource, NSTableViewDelegate
{
   private(set) var records = [AppKitNavigationDestinationRecord]()
   private var rowByID = [String: Int]()
   private(set) var fullReloadCount = 0
   private(set) var linearComparisonCount = 0
   var onSelect: ((String) -> Void)?

   func install(on table: NSTableView)
   {
      table.dataSource = self
      table.delegate = self
   }

   func replace(with records: [AppKitNavigationDestinationRecord], in table: NSTableView)
   {
      if self.records.count == records.count,
         self.records.first?.id == records.first?.id,
         self.records.last?.id == records.last?.id
      {
         return
      }
      self.records = records
      rowByID = Dictionary(uniqueKeysWithValues: records.enumerated().map {($0.element.id, $0.offset)})
      fullReloadCount += 1
      table.reloadData()
   }

   func row(for id: String) -> Int?
   {
      rowByID[id]
   }

   func uninstall(from table: NSTableView)
   {
      onSelect = nil
      records.removeAll(keepingCapacity: false)
      rowByID.removeAll(keepingCapacity: false)
      table.reloadData()
      table.dataSource = nil
      table.delegate = nil
   }

   func numberOfRows(in tableView: NSTableView) -> Int
   {
      records.count
   }

   func tableView(_ tableView: NSTableView, heightOfRow row: Int) -> CGFloat
   {
      62
   }

   func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView?
   {
      guard row >= 0, row < records.count else {return nil}
      let identifier = AppKitNavigationTableCellView.reuseIdentifier
      let cell = tableView.makeView(withIdentifier: identifier, owner: self) as? AppKitNavigationTableCellView
         ?? AppKitNavigationTableCellView(frame: .zero)
      cell.identifier = identifier
      cell.configure(record: records[row], target: self, action: #selector(performSelection(_:)), row: row)
      return cell
   }

   @objc private func performSelection(_ sender: NSButton)
   {
      guard sender.tag >= 0, sender.tag < records.count else {return}
      onSelect?(records[sender.tag].id)
   }
}
