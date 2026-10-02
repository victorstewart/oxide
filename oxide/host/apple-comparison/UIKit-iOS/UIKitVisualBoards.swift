import UIKit
import CoreText

private struct VisualSpec: Decodable
{
   let viewport: [CGFloat]
   let font: String
   let heading_rect: [CGFloat]
   let heading_px: CGFloat
   let palette: Palette

   struct Palette: Decodable {let text: [CGFloat]; let blue: [CGFloat]; let muted: [CGFloat]; let panel: [CGFloat]; let green: [CGFloat]; let red: [CGFloat]}
   struct Board: Decodable
   {
      let title: String
      let buttons: [[CGFloat]]?
      let button_titles: [String]?
      let toggle_rect: [CGFloat]?
      let slider_rects: [[CGFloat]]?
      let progress_rects: [[CGFloat]]?
      let progress_values: [CGFloat?]?
      let stage_elapsed_ms: [CGFloat]?
      let rects: [[CGFloat]]?
      let placeholders: [String]?
      let initial: [String]?
      let changed: [String]?
      let settled: [String]?
      let font_px: CGFloat?
      let selection: [Int]?
      let validator_expected: String?
      let texts: [[String]]?
      let sizes: [CGFloat]?
      let bold_rows: [Int]?
      let narrow_width: CGFloat?
      let wrap_mode: String?
      let outer: [CGFloat]?
      let inner: [CGFloat]?
      let tile_a: [CGFloat]?
      let tile_b: [CGFloat]?
      let image: [CGFloat]?
      let label: [CGFloat]?
      let label_text: String?
      let transforms: [[CGFloat]]?
      let colors: [[CGFloat]]?
      let retained_rect: [CGFloat]?
      let retained_tile: [CGFloat]?
      let retained_text: [CGFloat]?
      let retained_strings: [String]?
      let container_origins: [[CGFloat]]?
      let container_widths: [CGFloat]?
      let container_height: CGFloat?
      let padding: CGFloat?
      let gap: CGFloat?
      let short: String?
      let long: String?
      let list_rect: [CGFloat]?
      let row_count: Int?
      let row_height_base: CGFloat?
      let row_height_step: CGFloat?
      let row_gap: CGFloat?
      let changed_order_prefix: [Int]?
      let scroll_changed: CGFloat?
      let scroll_away: CGFloat?
      let picker_rect: [CGFloat]?
      let columns: [[String]]?
      let selections: [Int]?
      let popup_progress: [CGFloat]?
      let popup_rect: [CGFloat]?
      let popup_title: String?
      let popup_body: String?
      let left_anchor: [CGFloat]?
      let right_anchor: [CGFloat]?
      let popover_text: String?
      let popover_size: [CGFloat]?
      let viewport_margin: CGFloat?
   }
}

private final class QuietTextField: UITextField
{
   private lazy var quietInput = UIView(frame: .zero)
   override var inputView: UIView?
   {
      get {quietInput}
      set {}
   }
   private let contentInsets = UIEdgeInsets(top: 13.5, left: 11.5, bottom: 13.5, right: 11.5)
   override func textRect(forBounds bounds: CGRect) -> CGRect {bounds.inset(by: contentInsets)}
   override func editingRect(forBounds bounds: CGRect) -> CGRect {bounds.inset(by: contentInsets)}
   override func placeholderRect(forBounds bounds: CGRect) -> CGRect {bounds.inset(by: contentInsets)}
}

private final class VisualSlider: UISlider
{
   override func trackRect(forBounds bounds: CGRect) -> CGRect
   {
      CGRect(x: bounds.minX, y: bounds.midY - 8, width: bounds.width, height: 16)
   }
}

// A flat picker style uses native table layout, scrolling and selection. This
// avoids UIPickerView's fixed cylindrical projection and private animation clock.
private final class FlatPickerColumn: UITableView, UITableViewDataSource
{
   let values: [String]
   let rowFont: UIFont
   let rowColor: UIColor
   init(frame: CGRect, values: [String], font: UIFont, color: UIColor)
   {
      self.values = values; rowFont = font; rowColor = color
      super.init(frame: frame, style: .plain)
      backgroundColor = .clear; separatorStyle = .none; rowHeight = frame.height / 3
      estimatedRowHeight = 0; contentInsetAdjustmentBehavior = .never
      contentInset = UIEdgeInsets(top: rowHeight, left: 0, bottom: rowHeight, right: 0)
      showsVerticalScrollIndicator = false; dataSource = self
      register(UITableViewCell.self, forCellReuseIdentifier: "picker-row")
   }
   required init?(coder: NSCoder) {fatalError("init(coder:) has not been implemented")}
   func tableView(_ tableView: UITableView, numberOfRowsInSection section: Int) -> Int {values.count}
   func tableView(_ tableView: UITableView, cellForRowAt indexPath: IndexPath) -> UITableViewCell
   {
      let cell = dequeueReusableCell(withIdentifier: "picker-row", for: indexPath)
      cell.backgroundColor = .clear; cell.selectionStyle = .none
      cell.textLabel?.text = values[indexPath.row]; cell.textLabel?.font = rowFont
      cell.textLabel?.textColor = rowColor; cell.textLabel?.textAlignment = .center
      cell.layoutMargins = .zero; cell.preservesSuperviewLayoutMargins = false
      return cell
   }
   func position(selection: Int, fraction: CGFloat, animated: Bool)
   {
      layoutIfNeeded()
      selectRow(at: IndexPath(row: selection, section: 0), animated: animated, scrollPosition: .none)
      setContentOffset(CGPoint(x: 0, y: (CGFloat(selection) - fraction - 1) * rowHeight), animated: animated)
      layoutIfNeeded()
   }
}

final class UIKitVisualBoards: NSObject, CoreSuiteAdapter, UICollectionViewDataSource, UICollectionViewDelegateFlowLayout
{
   private let root: UIView
   private let rootIdentity: ObjectIdentifier
   private let name: String
   private let checkpoint: Bool
   private let spec: VisualSpec
   private let board: VisualSpec.Board
   private let font: UIFont
   private let palette: VisualSpec.Palette
   private var editable = [QuietTextField]()
   private var textRows = [UILabel]()
   private var stageViews = [UIView]()
   private var list: UICollectionView?
   private var listKeys = [Int]()
   private var pickerColumns = [[String]]()
   private var controlButtons = [UIButton]()
   private var controlToggle: UISwitch?
   private var controlSliders = [UISlider]()
   private var pickerViews = [FlatPickerColumn]()
   private var popup: UIView?
   private var popupOverlay: UIView?
   private var popovers = [UIView]()
   private var indeterminateTrack: UIView?
   private var indeterminateFill: UIView?
   private var lastStage = -1
   private var compositionPrimitives = [UIView]()
   private var retainedTile: UIView?
   private var retainedLabel: UILabel?
   private var layoutLabels = [UILabel]()
   #if DEBUG
   private var restoredCycles = 0
   #endif

   init(window: UIWindow, caseName: String, checkpoint: Bool) throws
   {
      guard ["visual-controls", "visual-editing", "visual-typography", "visual-composition", "visual-layout", "visual-pickers"].contains(caseName) else
      {
         throw NSError(domain: "UIKitVisualBoards", code: 1, userInfo: [NSLocalizedDescriptionKey: "Unsupported visual case \(caseName)"])
      }
      guard let url = Bundle.main.url(forResource: "visual", withExtension: "json") else
      {
         throw NSError(domain: "UIKitVisualBoards", code: 2, userInfo: [NSLocalizedDescriptionKey: "Missing visual.json"])
      }
      let data = try Data(contentsOf: url)
      spec = try JSONDecoder().decode(VisualSpec.self, from: data)
      guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            let boards = object["boards"] as? [String: Any], let selected = boards[caseName] else {throw NSError(domain: "UIKitVisualBoards", code: 3)}
      board = try JSONDecoder().decode(VisualSpec.Board.self, from: JSONSerialization.data(withJSONObject: selected))
      let parts = spec.font.split(separator: ".", maxSplits: 1).map(String.init)
      guard parts.count == 2, let fontURL = Bundle.main.url(forResource: parts[0], withExtension: parts[1]), let provider = CGDataProvider(url: fontURL as CFURL), let graphicsFont = CGFont(provider) else
      {
         throw NSError(domain: "UIKitVisualBoards", code: 4, userInfo: [NSLocalizedDescriptionKey: "Missing bundled Noto Sans font"])
      }
      var registrationError: Unmanaged<CFError>?
      guard CTFontManagerRegisterFontsForURL(fontURL as CFURL, .process, &registrationError) || registrationError == nil,
            let name = graphicsFont.postScriptName as String?, let loaded = UIFont(name: name, size: 14) else
      {
         throw registrationError?.takeRetainedValue() ?? NSError(domain: "UIKitVisualBoards", code: 5, userInfo: [NSLocalizedDescriptionKey: "Unable to load bundled Noto Sans"])
      }
      font = loaded
      palette = spec.palette
      self.name = caseName
      self.checkpoint = checkpoint
      root = UIView(frame: CGRect(x: (window.bounds.width - spec.viewport[0]) / 2, y: (window.bounds.height - spec.viewport[1]) / 2, width: spec.viewport[0], height: spec.viewport[1]))
      rootIdentity = ObjectIdentifier(root)
      super.init()
      root.backgroundColor = .white
      window.rootViewController?.view.addSubview(root)
      addLabel(board.title, frame: rect(spec.heading_rect), size: spec.heading_px, bold: false)
      switch caseName
      {
      case "visual-controls": controls()
      case "visual-editing": editing()
      case "visual-typography": typography()
      case "visual-composition": composition()
      case "visual-layout": layout()
      case "visual-pickers": pickers()
      default: break
      }
   }

   func render(time: Double, generation: UInt64) throws
   {
      CATransaction.begin()
      CATransaction.setDisableActions(true)
      apply(stage: stage(for: time))
      CATransaction.commit()
   }

   func reset()
   {
      lastStage = -1
      CATransaction.begin(); CATransaction.setDisableActions(true); apply(stage: 0); CATransaction.commit()
   }

   func advance(time: Double, generation: UInt64) throws -> Bool
   {
      let stage = stage(for: time)
      guard stage != lastStage else {return false}
      if checkpoint {try render(time: time, generation: generation)}
      else
      {
         UIView.animate(withDuration: 0.25, delay: 0, options: [.beginFromCurrentState, .curveEaseInOut])
         {
            self.apply(stage: stage)
         }
      }
      return true
   }

   func nextWakeup(after time: Double) -> Double
   {
      checkpoint ? .infinity : (floor(max(0, time) / 2) + 1) * 2
   }

   func settle() {}

   private func stage(for time: Double) -> Int
   {
      checkpoint ? max(0, min(2, Int(time.rounded()))) : Int(floor(max(0, time) / 2)) % 3
   }

   private func apply(stage: Int)
   {
      guard stage != lastStage else {return}
      let previousStage = lastStage
      lastStage = stage
      switch name
      {
      case "visual-controls": renderControls(stage)
      case "visual-editing": renderEditing(stage)
      case "visual-typography": renderTypography(stage)
      case "visual-composition": renderComposition(stage)
      case "visual-layout": renderLayout(stage)
      case "visual-pickers": renderPickers(stage)
      default: break
      }
      #if DEBUG
      if !checkpoint, previousStage == 2, stage == 0 {verifyRestoredInitialState()}
      #endif
   }

   private func controls()
   {
      for (_, pair) in zip(board.buttons ?? [], board.button_titles ?? []).enumerated()
      {
         let button = UIButton(type: .custom)
         button.frame = rect(pair.0)
         button.setTitle(pair.1, for: .normal)
         button.titleLabel?.font = font.withSize(14)
         button.backgroundColor = color(palette.blue)
         button.tintColor = .white
         button.layer.cornerRadius = 6
         button.isEnabled = pair.1 != "Disabled"
         if pair.1 == "Disabled" {button.backgroundColor = color(palette.muted)}
         root.addSubview(button); controlButtons.append(button)
      }
      if let value = board.toggle_rect
      {
         let toggle = UISwitch(frame: rect(value)); toggle.preferredStyle = .sliding; toggle.onTintColor = color(palette.green); toggle.thumbTintColor = .white; toggle.isOn = false
         toggle.sizeToFit(); let target = rect(value)
         toggle.transform = CGAffineTransform(scaleX: target.width / toggle.bounds.width, y: target.height / toggle.bounds.height)
         toggle.center = CGPoint(x: target.midX, y: target.midY); root.addSubview(toggle); controlToggle = toggle
      }
      for (index, value) in (board.slider_rects ?? []).enumerated()
      {
         let slider = VisualSlider(frame: rect(value)); slider.tintColor = color(palette.blue); slider.minimumTrackTintColor = color(palette.blue); slider.maximumTrackTintColor = color(palette.muted); slider.value = [0, 0.5, 1][index]
         slider.setThumbImage(sliderThumb(), for: .normal)
         root.addSubview(slider); controlSliders.append(slider)
      }
      for (index, value) in (board.progress_rects ?? []).enumerated()
      {
         if board.progress_values![index] == nil
         {
            let track = UIView(frame: rect(value)); track.backgroundColor = color(palette.muted); track.layer.cornerRadius = track.bounds.height * 0.5; root.addSubview(track)
            let fill = UIView(frame: CGRect(x: 0, y: 0, width: max(8, track.bounds.width * 0.3), height: track.bounds.height)); fill.backgroundColor = color(palette.blue); fill.layer.cornerRadius = fill.bounds.height * 0.5; track.addSubview(fill)
            indeterminateTrack = track; indeterminateFill = fill
            if !checkpoint {startIndeterminateAnimation(track: track, fill: fill)}
         }
         else
         {
            let progress = CoreProgressView(frame: rect(value)); progress.progressTintColor = color(palette.blue); progress.trackTintColor = color(palette.muted); progress.progress = Float(board.progress_values![index] ?? 0); progress.useFlatConfiguredColors(); root.addSubview(progress); stageViews.append(progress)
         }
      }
   }

   private func editing()
   {
      for (index, value) in (board.rects ?? []).enumerated()
      {
         let field = QuietTextField(frame: rect(value)); field.font = font.withSize(board.font_px ?? 18); field.placeholder = board.placeholders![index]; if index == 0 {field.textAlignment = .center}; field.text = board.initial![index]; field.borderStyle = .none
         field.textColor = color(palette.text); field.tintColor = color(palette.blue); field.backgroundColor = .white; field.layer.cornerRadius = 10; field.layer.borderWidth = 1.5; field.layer.borderColor = (index == 3 ? color(palette.red) : color(palette.muted)).cgColor
         let paragraph = NSMutableParagraphStyle(); paragraph.alignment = .center
         field.attributedPlaceholder = NSAttributedString(string: board.placeholders![index], attributes: [.foregroundColor: color(palette.text), .paragraphStyle: paragraph])
         if index == 2 {field.isSecureTextEntry = true}
         root.addSubview(field); editable.append(field)
      }
   }

   private func typography()
   {
      precondition(board.wrap_mode == "word-or-grapheme")
      for (index, value) in (board.rects ?? []).enumerated()
      {
         let label = addLabel("", frame: rect(value), size: board.sizes![index], bold: (board.bold_rows ?? []).contains(index))
         label.numberOfLines = 0; label.lineBreakStrategy = []
         if index == 1 {label.frame.size.width = board.narrow_width ?? label.frame.width}
         textRows.append(label)
      }
   }

   private func composition()
   {
      let outer = UIView(frame: rect(board.outer!)); outer.backgroundColor = color(palette.panel); outer.clipsToBounds = true; root.addSubview(outer)
      let inner = UIView(frame: rect(board.inner!)); inner.backgroundColor = .white; inner.clipsToBounds = true; outer.addSubview(inner); inner.frame = rect(board.inner!).offsetBy(dx: -outer.frame.minX, dy: -outer.frame.minY)
      let origin = rect(board.inner!).origin
      func local(_ source: [CGFloat]) -> CGRect {CGRect(x: source[0] - origin.x, y: source[1] - origin.y, width: source[2], height: source[3])}
      let a = UIView(frame: local(board.tile_a!)); a.backgroundColor = color(board.colors![0]); inner.addSubview(a)
      let b = UIView(frame: local(board.tile_b!)); b.backgroundColor = color(board.colors![1]); inner.addSubview(b)
      let image = UIImageView(frame: local(board.image!)); image.image = UIImage(named: "image-0.png"); image.contentMode = .scaleAspectFill; image.clipsToBounds = true; inner.addSubview(image)
      let label = addLabel(board.label_text!, frame: local(board.label!), size: 18, bold: false); label.numberOfLines = 0; inner.addSubview(label)
      label.attributedText = matchedText(board.label_text!, font: label.font)
      compositionPrimitives = [a, b, image, label]
      let retained = UIView(frame: rect(board.retained_rect!)); retained.backgroundColor = color(palette.panel); retained.layer.cornerRadius = 12; root.addSubview(retained)
      let retainedOrigin = retained.frame.origin
      let tileFrame = rect(board.retained_tile!).offsetBy(dx: -retainedOrigin.x, dy: -retainedOrigin.y)
      let tile = UIView(frame: tileFrame); tile.backgroundColor = color(palette.blue); tile.layer.cornerRadius = 10; retained.addSubview(tile); retainedTile = tile
      let textFrame = rect(board.retained_text!).offsetBy(dx: -retainedOrigin.x, dy: -retainedOrigin.y)
      let retainedLabel = addLabel(board.retained_strings![0], frame: textFrame, size: 18, bold: false); retained.addSubview(retainedLabel); self.retainedLabel = retainedLabel
   }

   private func layout()
   {
      for index in 0..<2
      {
         let origin = board.container_origins![index]
         let box = UIView(frame: CGRect(x: origin[0], y: origin[1], width: board.container_widths![index], height: board.container_height!))
         box.backgroundColor = color(palette.panel); box.layer.cornerRadius = 8; root.addSubview(box)
         let label = addLabel(index == 0 ? board.short! : board.long!, frame: .zero, size: board.font_px!, bold: false)
         label.numberOfLines = 0
         let button = UIButton(type: .custom)
         button.setTitle("Continue", for: .normal); button.titleLabel?.font = font.withSize(14)
         button.backgroundColor = color(palette.blue); button.layer.cornerRadius = 6
         let stack = UIStackView(arrangedSubviews: [label, button]); stack.axis = .vertical; stack.spacing = board.gap!
         stack.translatesAutoresizingMaskIntoConstraints = false; box.addSubview(stack)
         NSLayoutConstraint.activate([stack.leadingAnchor.constraint(equalTo: box.leadingAnchor, constant: board.padding!),
            stack.trailingAnchor.constraint(equalTo: box.trailingAnchor, constant: -board.padding!),
            stack.topAnchor.constraint(equalTo: box.topAnchor, constant: board.padding!), button.heightAnchor.constraint(equalToConstant: 40)])
         layoutLabels.append(label)
      }
      let flow = UICollectionViewFlowLayout(); flow.minimumLineSpacing = board.row_gap!; flow.sectionInset = .zero
      let collection = UICollectionView(frame: rect(board.list_rect!), collectionViewLayout: flow)
      collection.backgroundColor = color(palette.panel); collection.showsVerticalScrollIndicator = false
      collection.dataSource = self; collection.delegate = self
      collection.register(UICollectionViewCell.self, forCellWithReuseIdentifier: "row")
      root.addSubview(collection); list = collection; listKeys = Array(0..<board.row_count!)
   }

   private func pickers()
   {
      pickerColumns = board.columns ?? []
      let bounds = rect(board.picker_rect!)
      let band = UIView(frame: CGRect(x: bounds.minX, y: bounds.minY + bounds.height / 3, width: bounds.width, height: bounds.height / 3))
      band.backgroundColor = color(palette.panel); band.layer.cornerRadius = bounds.height / 12
      root.addSubview(band)
      for (index, values) in pickerColumns.enumerated()
      {
         let column = FlatPickerColumn(frame: CGRect(x: bounds.minX + CGFloat(index) * bounds.width / 2, y: bounds.minY, width: bounds.width / 2, height: bounds.height), values: values, font: font.withSize(18), color: color(palette.text))
         root.addSubview(column); pickerViews.append(column)
      }
      let overlay = UIView(frame: root.bounds); overlay.backgroundColor = .black; overlay.alpha = 0; root.addSubview(overlay); popupOverlay = overlay
      let popup = UIView(frame: rect(board.popup_rect!)); popup.backgroundColor = color(palette.panel); popup.layer.cornerRadius = 12; popup.isHidden = true; root.addSubview(popup); self.popup = popup
      let title = addLabel(board.popup_title!, frame: CGRect(x: 0, y: popup.bounds.height * 0.16, width: popup.bounds.width, height: 30), size: 18, bold: true); title.textAlignment = .center; popup.addSubview(title)
      let body = addLabel(board.popup_body!, frame: CGRect(x: 20, y: popup.bounds.height * 0.46, width: popup.bounds.width - 40, height: 50), size: 14, bold: false); body.textAlignment = .center; body.numberOfLines = 0; popup.addSubview(body)
      for anchor in [board.left_anchor!, board.right_anchor!]
      {
         let popover = UIView(frame: placeNearAnchor(rect(anchor), size: board.popover_size!, margin: board.viewport_margin ?? 8)); popover.backgroundColor = color(palette.panel); popover.layer.cornerRadius = 8; popover.isHidden = true; root.addSubview(popover)
         let title = addLabel(board.popover_text!, frame: popover.bounds, size: 14, bold: false); title.textAlignment = .center; popover.addSubview(title); popovers.append(popover)
      }
   }

   private func renderControls(_ stage: Int)
   {
      controlButtons.first?.transform = stage == 1 ? CGAffineTransform(scaleX: 0.99, y: 0.99) : .identity
      if controlButtons.count > 2 {controlButtons[2].transform = stage == 1 ? CGAffineTransform(scaleX: 0.99, y: 0.99) : .identity}
      for (index, button) in controlButtons.enumerated()
      {
         button.backgroundColor = index == 1 ? color(palette.muted) : stage == 1 ? UIColor(red: 0.18, green: 0.50, blue: 0.95, alpha: 1) : color(palette.blue)
      }
      if stage == 1, let toggle = controlToggle, checkpoint
      {
         // Freeze the native layer clock, including UISwitch's own animations.
         let start = toggle.layer.convertTime(CACurrentMediaTime(), from: nil)
         toggle.layer.speed = 0
         toggle.layer.timeOffset = start
         CATransaction.begin()
         CATransaction.setDisableActions(false)
         toggle.setOn(true, animated: true)
         CATransaction.commit()
         toggle.layer.timeOffset = start + 0.1
      }
      else if stage == 1
      {
         controlToggle?.setOn(true, animated: true)
      }
      else
      {
         controlToggle?.layer.speed = 1; controlToggle?.layer.timeOffset = 0
         controlToggle?.setOn(stage == 2, animated: false)
      }
      let values: [Float] = stage == 0 ? [0, 0.5, 1] : stage == 1 ? [0.25, 0.75, 1] : [0.5, 1, 1]
      for (slider, value) in zip(controlSliders, values) {slider.value = value}
      stageViews.forEach {$0.alpha = 1}
      if checkpoint, let track = indeterminateTrack, let fill = indeterminateFill
      {
         let phase: CGFloat = stage == 1 ? 0.25 : 0
         fill.frame.origin.x = (track.bounds.width - fill.bounds.width) * phase
      }
   }
   private func renderEditing(_ stage: Int)
   {
      let values = stage < 2 ? (stage == 0 ? board.initial! : board.changed!) : board.initial!
      for (field, text) in zip(editable, values) {field.text = text; field.backgroundColor = .white}
      if stage == 1, editable.count > 1
      {
         editable[1].becomeFirstResponder()
         let selection = board.selection!
         let start = editable[1].position(from: editable[1].beginningOfDocument, offset: selection[0])
         let end = editable[1].position(from: editable[1].beginningOfDocument, offset: selection[1])
         if let start, let end {editable[1].selectedTextRange = editable[1].textRange(from: start, to: end)}
      }
      else if stage == 2, editable.count > 3
      {
         editable[0].becomeFirstResponder(); editable[0].insertText(board.settled![0]); editable[0].resignFirstResponder()
         editable[1].becomeFirstResponder()
         let selection = board.selection!
         let start = editable[1].position(from: editable[1].beginningOfDocument, offset: selection[0])
         let end = editable[1].position(from: editable[1].beginningOfDocument, offset: selection[1])
         if let start, let end, let range = editable[1].textRange(from: start, to: end) {editable[1].replace(range, withText: "NEW")}
         if let cursor = editable[1].position(from: editable[1].beginningOfDocument, offset: 6) {editable[1].selectedTextRange = editable[1].textRange(from: cursor, to: cursor); editable[1].insertText("!")}
         editable[1].resignFirstResponder()
         editable[3].becomeFirstResponder(); editable[3].selectAll(nil); editable[3].insertText(board.validator_expected ?? "valid"); editable[3].resignFirstResponder()
      }
      else {editable.forEach {$0.resignFirstResponder()}}
      for (index, field) in editable.enumerated()
      {
         field.textAlignment = index == 0 && (field.text ?? "").isEmpty ? .center : .left
         field.layer.borderColor = (index == 3 && stage < 2 ? color(palette.red) : index == 1 && stage == 1 ? color(palette.blue) : color(palette.muted)).cgColor
      }
   }
   private func renderTypography(_ stage: Int)
   {
      for (index, pair) in zip(textRows.indices, zip(textRows, board.texts![stage]))
      {
         let alignment: NSTextAlignment = stage == 2 && index >= 3 ? (index == 3 ? .left : index == 4 ? .center : .right) : .left
         pair.0.frame.size.width = index == 1 || (index == 3 && stage == 1) ? board.narrow_width! : board.rects![index][2]
         pair.0.attributedText = matchedText(pair.1, font: pair.0.font, alignment: alignment)
      }
   }
   private func renderComposition(_ stage: Int)
   {
      let values = board.transforms![stage]
      let original = [board.tile_a!, board.tile_b!, board.image!, board.label!]
      let origin = rect(board.inner!).origin
      for (primitive, source) in zip(compositionPrimitives, original)
      {
         primitive.transform = .identity
         primitive.layer.anchorPoint = .zero
         primitive.bounds = CGRect(x: 0, y: 0, width: source[2], height: source[3])
         primitive.layer.position = CGPoint(x: source[0] * values[2] + values[0] - origin.x, y: source[1] * values[2] + values[1] - origin.y)
         primitive.transform = CGAffineTransform(scaleX: values[2], y: values[2])
      }
      if stage == 1 {compositionPrimitives[0].superview?.insertSubview(compositionPrimitives[0], aboveSubview: compositionPrimitives[1])}
      compositionPrimitives.first?.isHidden = stage == 2
      retainedTile?.isHidden = stage == 2
      retainedLabel?.text = board.retained_strings![stage]
   }
   private func renderLayout(_ stage: Int)
   {
      layoutLabels[0].attributedText = matchedText(stage == 0 ? board.short! : board.long!, font: layoutLabels[0].font)
      layoutLabels[1].attributedText = matchedText(board.long!, font: layoutLabels[1].font)
      root.layoutIfNeeded()
      if stage == 0 {listKeys = Array(0..<board.row_count!)}
      else {listKeys = board.changed_order_prefix! + Array(5..<board.row_count!)}
      list?.reloadData(); list?.collectionViewLayout.invalidateLayout(); list?.layoutIfNeeded()
      if stage == 2 {list?.contentOffset = CGPoint(x: 0, y: board.scroll_away!); list?.layoutIfNeeded()}
      list?.contentOffset = CGPoint(x: 0, y: stage == 1 ? board.scroll_changed! : 0)
      list?.layoutIfNeeded()
   }
   private func renderPickers(_ stage: Int)
   {
      for (index, column) in pickerViews.enumerated()
      {
         column.position(selection: board.selections![index], fraction: stage == 1 ? (index == 0 ? 0.35 : -0.35) : 0, animated: !checkpoint)
      }
      popup?.isHidden = stage == 0
      let progress = board.popup_progress![stage]
      popupOverlay?.alpha = 0.08 * progress
      popup?.alpha = progress
      popup?.transform = CGAffineTransform(scaleX: 0.92 + progress * 0.08, y: 0.92 + progress * 0.08)
      if !popovers.isEmpty {popovers[0].isHidden = stage == 0}
      if popovers.count > 1 {popovers[1].isHidden = stage != 1}
   }

   func collectionView(_ collectionView: UICollectionView, numberOfItemsInSection section: Int) -> Int {listKeys.count}
   func collectionView(_ collectionView: UICollectionView, cellForItemAt indexPath: IndexPath) -> UICollectionViewCell
   {
      let cell = collectionView.dequeueReusableCell(withReuseIdentifier: "row", for: indexPath)
      let label: UILabel
      if let existing = cell.contentView.subviews.first as? UILabel {label = existing}
      else
      {
         label = CoreTopLabel(frame: cell.contentView.bounds.insetBy(dx: 10, dy: 0)); label.autoresizingMask = [.flexibleWidth, .flexibleHeight]; label.font = font.withSize(board.font_px!); label.textColor = color(palette.text); cell.contentView.addSubview(label)
      }
      label.text = "Row \(listKeys[indexPath.item])"
      cell.contentView.backgroundColor = .white; cell.contentView.layer.cornerRadius = 6; cell.contentView.clipsToBounds = true
      return cell
   }
   func collectionView(_ collectionView: UICollectionView, layout collectionViewLayout: UICollectionViewLayout, sizeForItemAt indexPath: IndexPath) -> CGSize
   {
      let key = listKeys[indexPath.item]
      let height = board.row_height_base! + CGFloat(key % 3) * board.row_height_step! + (key == 1 && lastStage > 0 ? 36 : 0)
      return CGSize(width: collectionView.bounds.width, height: height)
   }

   @discardableResult private func addLabel(_ text: String, frame: CGRect, size: CGFloat, bold: Bool) -> UILabel
   {
      let label = CoreTopLabel(frame: frame); label.text = text; label.font = font.withSize(size); if bold {label.font = UIFont(descriptor: label.font.fontDescriptor.withSymbolicTraits(.traitBold) ?? label.font.fontDescriptor, size: size)}; label.textColor = color(palette.text); root.addSubview(label); return label
   }
   private func matchedText(_ text: String, font: UIFont, alignment: NSTextAlignment = .left) -> NSAttributedString
   {
      let paragraph = NSMutableParagraphStyle()
      paragraph.alignment = alignment; paragraph.lineBreakMode = .byWordWrapping
      paragraph.lineBreakStrategy = []
      paragraph.minimumLineHeight = ceil(font.pointSize * 1.25)
      paragraph.maximumLineHeight = ceil(font.pointSize * 1.25)
      // Preserve the natural first baseline while matching subsequent line spacing.
      let baselineOffset = min(0, paragraph.maximumLineHeight - font.lineHeight)
      return NSAttributedString(string: text, attributes: [.font: font, .foregroundColor: color(palette.text), .paragraphStyle: paragraph, .baselineOffset: baselineOffset])
   }
   private func placeNearAnchor(_ anchor: CGRect, size: [CGFloat], margin: CGFloat) -> CGRect
   {
      let margin = max(0, margin)
      let width = min(max(0, size[0]), max(0, root.bounds.width - margin * 2))
      let height = min(max(0, size[1]), max(0, root.bounds.height - margin * 2))
      let minX = margin; let maxX = max(minX, root.bounds.width - margin - width)
      let minY = margin; let maxY = max(minY, root.bounds.height - margin - height)
      let x = min(max(anchor.midX - width * 0.5, minX), maxX)
      let above = anchor.minY - height - margin
      let below = anchor.maxY + margin
      let y = min(max(above >= minY ? above : below, minY), maxY)
      return CGRect(x: x, y: y, width: width, height: height)
   }
   private func rect(_ values: [CGFloat]) -> CGRect {CGRect(x: values[0], y: values[1], width: values[2], height: values[3])}
   private func color(_ values: [CGFloat]) -> UIColor {UIColor(red: values[0], green: values[1], blue: values[2], alpha: values[3])}
   private func sliderThumb() -> UIImage
   {
      let renderer = UIGraphicsImageRenderer(size: CGSize(width: 8, height: 8))
      return renderer.image
      {
         context in
         UIColor.white.setFill()
         context.cgContext.fillEllipse(in: CGRect(x: 0, y: 0, width: 8, height: 8))
      }
   }

   private func startIndeterminateAnimation(track: UIView, fill: UIView)
   {
      fill.frame.origin.x = 0
      let animation = CABasicAnimation(keyPath: "position.x")
      animation.fromValue = fill.bounds.width * 0.5
      animation.toValue = track.bounds.width - fill.bounds.width * 0.5
      animation.duration = 1
      animation.repeatCount = .infinity
      animation.timingFunction = CAMediaTimingFunction(name: .linear)
      fill.layer.add(animation, forKey: "core-indeterminate-progress")
   }

   #if DEBUG
   private func verifyRestoredInitialState()
   {
      precondition(ObjectIdentifier(root) == rootIdentity)
      switch name
      {
      case "visual-controls":
         precondition(controlToggle?.isOn == false)
         precondition(controlSliders.map(\.value) == [0, 0.5, 1])
         precondition(indeterminateFill?.layer.animation(forKey: "core-indeterminate-progress") != nil)
      case "visual-composition":
         precondition(compositionPrimitives.first?.isHidden == false)
         precondition(retainedTile?.isHidden == false)
         precondition(retainedLabel?.text == board.retained_strings?.first)
      case "visual-layout":
         precondition(listKeys == Array(0..<board.row_count!))
         precondition(list != nil)
      default: break
      }
      restoredCycles += 1
      precondition(restoredCycles <= 20)
   }
   #endif
}
