import UIKit
import CoreText

private struct EditingEdgesFixture: Decodable
{
   let viewport: [CGFloat]
   let font: String
   let heading_rect: [CGFloat]
   let heading_px: CGFloat
   let palette: Palette

   struct Palette: Decodable {let text: [CGFloat]; let blue: [CGFloat]; let muted: [CGFloat]; let red: [CGFloat]}
   struct Board: Decodable
   {
      let title: String
      let rects: [[CGFloat]]
      let long_text: String
      let floating_placeholder: String
      let preedit_base: String
      let preedit_marked: String
      let preedit_commit: String
      let otp_length: Int
      let otp_partial: String
      let otp_full: String
      let font_px: CGFloat
   }
}

private final class QuietEditingEdgesTextField: UITextField
{
   var insets = UIEdgeInsets(top: 13.5, left: 11.5, bottom: 13.5, right: 11.5)
   override init(frame: CGRect)
   {
      super.init(frame: frame); inputView = UIView(frame: .zero)
   }
   required init?(coder: NSCoder) {fatalError("init(coder:) has not been implemented")}
   override func textRect(forBounds bounds: CGRect) -> CGRect {bounds.inset(by: insets)}
   override func editingRect(forBounds bounds: CGRect) -> CGRect {bounds.inset(by: insets)}
   override func placeholderRect(forBounds bounds: CGRect) -> CGRect {bounds.inset(by: insets)}
}

final class UIKitEditingEdges: NSObject, CoreSuiteAdapter
{
   private let root: UIView
   private let checkpoint: Bool
   private let board: EditingEdgesFixture.Board
   private let font: UIFont
   private let palette: EditingEdgesFixture.Palette
   private var fields = [QuietEditingEdgesTextField]()
   private var otpFields = [QuietEditingEdgesTextField]()
   private let floatingLabel = UILabel()
   private var lastStage = -1

   init(window: UIWindow, caseName: String, checkpoint: Bool) throws
   {
      guard caseName == "visual-editing-edges" else {throw NSError(domain: "UIKitEditingEdges", code: 1)}
      guard let url = Bundle.main.url(forResource: "visual", withExtension: "json") else {throw NSError(domain: "UIKitEditingEdges", code: 2)}
      let data = try Data(contentsOf: url)
      let fixture = try JSONDecoder().decode(EditingEdgesFixture.self, from: data)
      guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            let boards = object["boards"] as? [String: Any], let selectedObject = boards[caseName] else {throw NSError(domain: "UIKitEditingEdges", code: 3)}
      let selectedData = try JSONSerialization.data(withJSONObject: selectedObject)
      let selected = try JSONDecoder().decode(EditingEdgesFixture.Board.self, from: selectedData)
      board = selected; palette = fixture.palette; self.checkpoint = checkpoint
      let parts = fixture.font.split(separator: ".", maxSplits: 1).map(String.init)
      guard parts.count == 2, let fontURL = Bundle.main.url(forResource: parts[0], withExtension: parts[1]), let provider = CGDataProvider(url: fontURL as CFURL), let graphicsFont = CGFont(provider) else {throw NSError(domain: "UIKitEditingEdges", code: 4)}
      var registrationError: Unmanaged<CFError>?
      guard CTFontManagerRegisterFontsForURL(fontURL as CFURL, .process, &registrationError) || registrationError == nil,
            let name = graphicsFont.postScriptName as String?, let loaded = UIFont(name: name, size: board.font_px) else {throw NSError(domain: "UIKitEditingEdges", code: 5)}
      font = loaded
      root = UIView(frame: CGRect(x: (window.bounds.width - fixture.viewport[0]) * 0.5, y: (window.bounds.height - fixture.viewport[1]) * 0.5, width: fixture.viewport[0], height: fixture.viewport[1]))
      super.init()
      root.backgroundColor = .white; window.rootViewController?.view.addSubview(root)
      let heading = CoreTopLabel(frame: rect(fixture.heading_rect)); heading.attributedText = matchedText(board.title, font: font.withSize(fixture.heading_px)); root.addSubview(heading)
      makeFields()
   }

   func render(time: Double, generation: UInt64) throws
   {
      let stage = stage(for: time)
      guard stage != lastStage else {return}
      lastStage = stage
      if stage == 0 {renderInitial()}
      if stage == 1 {renderFocused()}
      if stage == 2 {renderCommitted()}
   }

   func reset()
   {
      lastStage = -1
      try? render(time: 0, generation: 0)
   }

   func advance(time: Double, generation: UInt64) throws -> Bool
   {
      let stage = stage(for: time)
      guard stage != lastStage else {return false}
      try render(time: time, generation: generation)
      return true
   }

   func nextWakeup(after time: Double) -> Double {checkpoint ? .infinity : (floor(max(0, time) / 2) + 1) * 2}

   func settle()
   {
      guard checkpoint else {return}
      guard lastStage < 2 else {return}
      let field = fields[lastStage == 0 ? 0 : 1]
      field.layoutIfNeeded()
      // Activate and lay out UIKit's own selection display before freezing its
      // blink phase. A long scrolled field can otherwise retain a hidden cursor.
      guard let selection = field.interactions.compactMap({$0 as? UITextSelectionDisplayInteraction}).first else
      {
         preconditionFailure("Native selection display was not available at the checkpoint")
      }
      selection.isActivated = true
      selection.setNeedsSelectionUpdate()
      selection.layoutManagedSubviews()
      selection.cursorView.resetBlinkAnimation()
      selection.cursorView.isBlinking = false
   }

   private func stage(for time: Double) -> Int {checkpoint ? max(0, min(2, Int(time.rounded()))) : Int(floor(max(0, time) / 2)) % 3}

   private func makeFields()
   {
      for index in 0..<3
      {
         let field = field(rect(board.rects[index]))
         if index == 0
         {
            let paragraph = NSMutableParagraphStyle(); paragraph.lineBreakMode = .byClipping
            field.defaultTextAttributes = [.font: font, .foregroundColor: color(palette.text), .paragraphStyle: paragraph]
         }
         fields.append(field)
      }
      let floatingRect = rect(board.rects[1])
      floatingLabel.frame = CGRect(x: floatingRect.minX + 13.5, y: floatingRect.minY - 0.5, width: floatingRect.width - 27, height: 18)
      floatingLabel.font = font.withSize(14); floatingLabel.textAlignment = .center; floatingLabel.textColor = color(palette.text); floatingLabel.text = board.floating_placeholder; floatingLabel.backgroundColor = .clear; floatingLabel.isHidden = true; root.addSubview(floatingLabel)
      addOTPFields(rect(board.rects[3])); addOTPFields(rect(board.rects[4]))
   }

   private func renderInitial()
   {
      fields[0].text = board.long_text; fields[0].becomeFirstResponder(); let end = fields[0].endOfDocument; fields[0].selectedTextRange = fields[0].textRange(from: end, to: end)
      fields[1].text = nil; fields[1].textAlignment = .center; fields[1].attributedPlaceholder = NSAttributedString(string: board.floating_placeholder, attributes: [.font: font, .foregroundColor: color(palette.text)]); floatingLabel.isHidden = true; updateFieldBorders(active: 0)
      fields[2].text = board.preedit_base; fields[2].resignFirstResponder(); let compositionEnd = fields[2].endOfDocument; fields[2].selectedTextRange = fields[2].textRange(from: compositionEnd, to: compositionEnd)
      setOTP(Array(repeating: nil, count: board.otp_length), offset: 0)
      setOTP(Array(board.otp_full).map(String.init), offset: board.otp_length)
   }

   private func renderFocused()
   {
      fields[0].resignFirstResponder()
      fields[1].textAlignment = .left; fields[1].placeholder = nil; fields[1].becomeFirstResponder(); floatingLabel.isHidden = false; updateFieldBorders(active: 1)
      fields[2].markedTextStyle = [.underlineStyle: NSUnderlineStyle.single.rawValue, .underlineColor: color(palette.blue)]
      fields[2].setMarkedText(board.preedit_marked, selectedRange: NSRange(location: board.preedit_marked.utf16.count, length: 0))
      setOTP(Array(board.otp_partial).map(String.init), offset: 0)
   }

   private func renderCommitted()
   {
      fields[1].resignFirstResponder(); fields[1].textAlignment = .center; fields[1].attributedPlaceholder = NSAttributedString(string: board.floating_placeholder, attributes: [.font: font, .foregroundColor: color(palette.text)]); floatingLabel.isHidden = true; updateFieldBorders(active: nil)
      fields[2].insertText(board.preedit_commit); fields[2].resignFirstResponder()
      setOTP(Array(repeating: nil, count: board.otp_length), offset: 0)
   }

   private func setOTP(_ values: [String?], offset: Int)
   {
      for index in 0..<board.otp_length
      {
         let field = otpFields[offset + index]; field.text = values.indices.contains(index) ? values[index] : nil
      }
   }

   private func addOTPFields(_ outer: CGRect)
   {
      let shell = UIView(frame: outer); shell.backgroundColor = .white; shell.layer.cornerRadius = 10; shell.layer.borderWidth = 1.5; shell.layer.borderColor = color(palette.muted).cgColor; root.addSubview(shell)
      let content = outer.insetBy(dx: 13.5, dy: 11.5)
      let gap: CGFloat = 12; let slot = (content.width - CGFloat(board.otp_length - 1) * gap) / CGFloat(board.otp_length)
      for index in 0..<board.otp_length
      {
         let frame = CGRect(x: content.minX - outer.minX + CGFloat(index) * (slot + gap), y: content.minY - outer.minY, width: slot, height: content.height)
         let field = QuietEditingEdgesTextField(frame: frame); field.font = font; field.textAlignment = .center; field.keyboardType = .numberPad; field.textColor = color(palette.text); field.tintColor = color(palette.blue); field.placeholder = "–"; field.attributedPlaceholder = NSAttributedString(string: "–", attributes: [.foregroundColor: color(palette.text)]); field.backgroundColor = color(palette.muted).withAlphaComponent(0.18); field.borderStyle = .none; field.layer.cornerRadius = 6; field.layer.borderWidth = 0; shell.addSubview(field); otpFields.append(field)
      }
   }

   private func field(_ frame: CGRect) -> QuietEditingEdgesTextField
   {
      let shell = UIView(frame: frame); shell.backgroundColor = .white; shell.layer.cornerRadius = 10; shell.layer.borderWidth = 1.5; shell.layer.borderColor = color(palette.muted).cgColor; root.addSubview(shell)
      let field = QuietEditingEdgesTextField(frame: shell.bounds.insetBy(dx: 13.5, dy: 11.5)); field.insets = .zero; field.clipsToBounds = true; field.font = font; field.textColor = color(palette.text); field.tintColor = color(palette.blue); field.backgroundColor = .white; field.borderStyle = .none; shell.addSubview(field); return field
   }

   private func updateFieldBorders(active: Int?)
   {
      for (index, field) in fields.enumerated() {field.superview?.layer.borderColor = (active == index ? color(palette.blue) : color(palette.muted)).cgColor}
   }

   private func rect(_ values: [CGFloat]) -> CGRect {CGRect(x: values[0], y: values[1], width: values[2], height: values[3])}
   private func color(_ values: [CGFloat]) -> UIColor {UIColor(red: values[0], green: values[1], blue: values[2], alpha: values[3])}
   private func matchedText(_ text: String, font: UIFont) -> NSAttributedString
   {
      let paragraph = NSMutableParagraphStyle(); paragraph.lineBreakMode = .byWordWrapping
      paragraph.minimumLineHeight = ceil(font.pointSize * 1.25); paragraph.maximumLineHeight = ceil(font.pointSize * 1.25)
      let baselineOffset = min(0, paragraph.maximumLineHeight - font.lineHeight)
      return NSAttributedString(string: text, attributes: [.font: font, .foregroundColor: color(palette.text), .paragraphStyle: paragraph, .baselineOffset: baselineOffset])
   }
}
