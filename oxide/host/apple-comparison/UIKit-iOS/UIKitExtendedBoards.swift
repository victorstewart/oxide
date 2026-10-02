import UIKit
import CoreText

private struct ExtendedSpec: Decodable
{
   let font: String
   let palette: Palette
   struct Palette: Decodable {let text: [CGFloat]; let blue: [CGFloat]; let panel: [CGFloat]; let green: [CGFloat]}
   struct Board: Decodable
   {
      let title: String
      let panels: [[CGFloat]]?; let rectangles: [[CGFloat]]?; let colors: [[CGFloat]]?
      let group_alpha: [CGFloat]?; let outer_alpha: [CGFloat]?; let inner_alpha: CGFloat?; let restore_alpha: [CGFloat]?
      let assets: [String]?; let fit_rects: [[CGFloat]]?; let alpha_rects: [[CGFloat]]?; let crop_rects: [[CGFloat]]?; let crop_source: [CGFloat]?; let nine_rects: [[CGFloat]]?; let slice: CGFloat?; let replacement_rect: [CGFloat]?; let captions: [String]?
      let offsets: [CGFloat]?; let border_rects: [[CGFloat]]?; let border_width: CGFloat?; let rrects: [[CGFloat]]?; let radii: [[CGFloat]]?; let text_rects: [[CGFloat]]?; let font_sizes: [CGFloat]?; let texts: [String]?; let scaled_rect: [CGFloat]?; let scaled_text: String?; let scaled_font_px: CGFloat?; let text_scales: [CGFloat]?
   }
}

final class UIKitExtendedBoards: NSObject, CoreSuiteAdapter
{
   private let root: UIView
   private let name: String
   private let checkpoint: Bool
   private let spec: ExtendedSpec
   private let board: ExtendedSpec.Board
   private let font: UIFont
   private var lastStage = -1
   private var opacityGroups = [UIView]()
   private var crop: UIImageView?
   private var nine: UIImageView?
   private var replacement: UIImageView?
   private var geometryViews = [UIView]()
   private var scaledLabel: UILabel?

   init(window: UIWindow, caseName: String, checkpoint: Bool) throws
   {
      guard ["visual-opacity", "visual-images", "visual-geometry"].contains(caseName) else {throw NSError(domain: "UIKitExtendedBoards", code: 1, userInfo: [NSLocalizedDescriptionKey: "Unsupported extended visual case \(caseName)"])}
      guard let url = Bundle.main.url(forResource: "visual", withExtension: "json") else {throw NSError(domain: "UIKitExtendedBoards", code: 2, userInfo: [NSLocalizedDescriptionKey: "Missing visual.json"])}
      let data = try Data(contentsOf: url)
      spec = try JSONDecoder().decode(ExtendedSpec.self, from: data)
      guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            let boards = object["boards"] as? [String: Any], let selected = boards[caseName] else {throw NSError(domain: "UIKitExtendedBoards", code: 3)}
      let board = try JSONDecoder().decode(ExtendedSpec.Board.self, from: JSONSerialization.data(withJSONObject: selected))
      self.board = board; name = caseName; self.checkpoint = checkpoint
      let parts = spec.font.split(separator: ".", maxSplits: 1).map(String.init)
      guard parts.count == 2, let url = Bundle.main.url(forResource: parts[0], withExtension: parts[1]), let provider = CGDataProvider(url: url as CFURL), let cg = CGFont(provider) else {throw NSError(domain: "UIKitExtendedBoards", code: 4, userInfo: [NSLocalizedDescriptionKey: "Missing bundled Noto Sans"])}
      var registrationError: Unmanaged<CFError>?
      guard CTFontManagerRegisterFontsForURL(url as CFURL, .process, &registrationError) || registrationError == nil, let postScript = cg.postScriptName as String?, let font = UIFont(name: postScript, size: 14) else {throw registrationError?.takeRetainedValue() ?? NSError(domain: "UIKitExtendedBoards", code: 5)}
      self.font = font
      root = UIView(frame: CGRect(x: (window.bounds.width - 390) * 0.5, y: (window.bounds.height - 844) * 0.5, width: 390, height: 844))
      super.init(); root.backgroundColor = .white; window.rootViewController?.view.addSubview(root)
      addLabel(board.title, rect: CGRect(x: 20, y: 12, width: 350, height: 32), size: 18)
      switch name {case "visual-opacity": opacity(); case "visual-images": images(); case "visual-geometry": geometry(); default: break}
   }

   func render(time: Double, generation: UInt64) throws
   {
      let stage = stage(for: time)
      guard stage != lastStage else {return}; lastStage = stage
      CATransaction.begin(); CATransaction.setDisableActions(true)
      switch name {case "visual-opacity": renderOpacity(stage); case "visual-images": renderImages(stage); case "visual-geometry": renderGeometry(stage); default: break}
      CATransaction.commit()
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
      if checkpoint {try render(time: time, generation: generation)}
      else
      {
         lastStage = stage
         UIView.animate(withDuration: 0.25, delay: 0, options: [.beginFromCurrentState, .curveEaseInOut])
         {
            switch self.name {case "visual-opacity": self.renderOpacity(stage); case "visual-images": self.renderImages(stage); case "visual-geometry": self.renderGeometry(stage); default: break}
         }
      }
      return true
   }
   func nextWakeup(after time: Double) -> Double {checkpoint ? .infinity : (floor(max(0, time) / 2) + 1) * 2}
   func settle() {}

   private func stage(for time: Double) -> Int {checkpoint ? max(0, min(2, Int(time.rounded()))) : Int(floor(max(0, time) / 2)) % 3}

   private func opacity()
   {
      for panel in board.panels!
      {
         let background = UIView(frame: rect(panel)); background.backgroundColor = color(spec.palette.panel); root.addSubview(background)
         let view = UIView(frame: background.bounds); background.addSubview(view); opacityGroups.append(view)
      }
      let first = opacityGroups[0]
      addRectLocal(localGlobal(rect(board.rectangles![0]), parentOrigin: CGPoint(x: 20, y: 72)), color: color(board.colors![0]), into: first); addRectLocal(localGlobal(rect(board.rectangles![1]), parentOrigin: CGPoint(x: 20, y: 72)), color: color(board.colors![1]), into: first)
      let outer = opacityGroups[1]
      addRectLocal(localGlobal(rect(board.rectangles![2]), parentOrigin: CGPoint(x: 20, y: 300)), color: color(board.colors![2]), into: outer)
      let inner = UIView(frame: localGlobal(rect(board.rectangles![3]), parentOrigin: CGPoint(x: 20, y: 300))); inner.backgroundColor = color(board.colors![0]); inner.alpha = board.inner_alpha!; outer.addSubview(inner)
      let blue = UIView(frame: localGlobal(rect(board.rectangles![4]), parentOrigin: CGPoint(x: 100, y: 350))); blue.backgroundColor = color(board.colors![1]); inner.addSubview(blue)
      let third = opacityGroups[2]
      addRectLocal(localGlobal(rect(board.rectangles![5]), parentOrigin: CGPoint(x: 20, y: 560)), color: color(board.colors![0]), into: third); addRectLocal(localGlobal(rect(board.rectangles![6]), parentOrigin: CGPoint(x: 20, y: 560)), color: color(board.colors![1]), into: third)
   }
   private func renderOpacity(_ stage: Int) {opacityGroups[0].alpha = board.group_alpha![stage]; opacityGroups[1].alpha = board.outer_alpha![stage]; opacityGroups[2].alpha = board.restore_alpha![stage]}

   private func images()
   {
      let assets = board.assets!.map(loadImage)
      for (index, item) in board.fit_rects!.enumerated()
      {
         let view = UIImageView(frame: rect(item)); view.image = assets[0]; view.backgroundColor = color(spec.palette.panel); view.clipsToBounds = true; view.contentMode = [.scaleAspectFit, .scaleAspectFill, .scaleToFill][index]; root.addSubview(view)
      }
      for (index, item) in board.alpha_rects!.enumerated()
      {
         let view = UIImageView(frame: rect(item)); view.image = assets[0]; view.contentMode = .scaleToFill; view.backgroundColor = index == 0 ? .white : color(spec.palette.text); root.addSubview(view)
      }
      let crop = UIImageView(frame: rect(board.crop_rects![0])); crop.image = cropped(assets[0]); crop.contentMode = .scaleToFill; root.addSubview(crop); self.crop = crop
      let cap = assets[2].resizableImage(withCapInsets: UIEdgeInsets(top: board.slice!, left: board.slice!, bottom: board.slice!, right: board.slice!), resizingMode: .stretch)
      let nine = UIImageView(frame: rect(board.nine_rects![0])); nine.image = cap; nine.contentMode = .scaleToFill; root.addSubview(nine); self.nine = nine
      let replacement = UIImageView(frame: rect(board.replacement_rect!)); replacement.image = assets[0]; replacement.contentMode = .scaleToFill; replacement.backgroundColor = color(spec.palette.panel); root.addSubview(replacement); self.replacement = replacement
      for (index, caption) in board.captions!.enumerated() {addLabel(caption, rect: CGRect(x: 20, y: [54, 220, 394, 594][index], width: 350, height: 24), size: 13)}
   }
   private func renderImages(_ stage: Int)
   {
      crop?.frame = rect(board.crop_rects![stage]); nine?.frame = rect(board.nine_rects![stage])
      replacement?.image = stage == 0 ? loadImage(board.assets![0]) : stage == 1 ? loadImage(board.assets![1]) : nil
   }

   private func geometry()
   {
      for (index, item) in board.border_rects!.enumerated()
      {
         let view = UIView(frame: rect(item)); view.backgroundColor = .white; view.layer.borderColor = (index == 0 ? color(spec.palette.blue) : color(spec.palette.text)).cgColor; view.layer.borderWidth = board.border_width!; root.addSubview(view); geometryViews.append(view)
      }
      for (index, item) in board.rrects!.enumerated()
      {
         let holder = UIView(frame: rect(item)); let layer = CAShapeLayer(); layer.fillColor = (index == 0 ? color(spec.palette.blue) : color(spec.palette.green)).cgColor; layer.path = asymmetricPath(CGRect(origin: .zero, size: holder.bounds.size), radii: board.radii![index]).cgPath; holder.layer.addSublayer(layer); root.addSubview(holder); geometryViews.append(holder)
      }
      for index in board.text_rects!.indices {addLabel(board.texts![index], rect: rect(board.text_rects![index]), size: board.font_sizes![index])}
      let scaled = addLabel(board.scaled_text!, rect: rect(board.scaled_rect!), size: board.scaled_font_px!); scaled.layer.anchorPoint = CGPoint(x: 0, y: 0); scaled.layer.position = CGPoint(x: board.scaled_rect![0], y: board.scaled_rect![1]); scaledLabel = scaled
   }
   private func renderGeometry(_ stage: Int)
   {
      let offset = board.offsets![stage]
      for view in geometryViews {view.transform = CGAffineTransform(translationX: offset, y: offset)}
      let scale = board.text_scales![stage]; scaledLabel?.layer.contentsScale = 3; scaledLabel?.transform = CGAffineTransform(scaleX: scale, y: scale)
   }

   @discardableResult private func addLabel(_ text: String, rect: CGRect, size: CGFloat) -> UILabel
   {
      let label = CoreTopLabel(frame: rect); let valueFont = font.withSize(size); label.font = valueFont; label.attributedText = matchedText(text, font: valueFont); label.numberOfLines = 0; root.addSubview(label); return label
   }
   private func addRectLocal(_ rect: CGRect, color: UIColor, into parent: UIView) {let view = UIView(frame: rect); view.backgroundColor = color; parent.addSubview(view)}
   private func loadImage(_ name: String) -> UIImage {UIImage(named: name) ?? UIImage(contentsOfFile: Bundle.main.path(forResource: name.split(separator: ".")[0].description, ofType: name.split(separator: ".")[1].description)!)!}
   private func cropped(_ image: UIImage) -> UIImage {UIImage(cgImage: image.cgImage!.cropping(to: CGRect(x: board.crop_source![0], y: board.crop_source![1], width: board.crop_source![2], height: board.crop_source![3]))!, scale: 1, orientation: .up)}
   private func localGlobal(_ rect: CGRect, parentOrigin: CGPoint) -> CGRect {rect.offsetBy(dx: -parentOrigin.x, dy: -parentOrigin.y)}
   private func rect(_ values: [CGFloat]) -> CGRect {CGRect(x: values[0], y: values[1], width: values[2], height: values[3])}
   private func color(_ values: [CGFloat]) -> UIColor {UIColor(red: values[0], green: values[1], blue: values[2], alpha: values[3])}
   private func matchedText(_ text: String, font: UIFont) -> NSAttributedString
   {
      let paragraph = NSMutableParagraphStyle(); paragraph.lineBreakMode = .byWordWrapping; paragraph.lineBreakStrategy = []
      paragraph.minimumLineHeight = ceil(font.pointSize * 1.25); paragraph.maximumLineHeight = ceil(font.pointSize * 1.25)
      return NSAttributedString(string: text, attributes: [.font: font, .foregroundColor: color(spec.palette.text), .paragraphStyle: paragraph, .baselineOffset: min(0, paragraph.maximumLineHeight - font.lineHeight)])
   }
   private func asymmetricPath(_ rect: CGRect, radii: [CGFloat]) -> UIBezierPath
   {
      let path = UIBezierPath(); path.move(to: CGPoint(x: rect.minX + radii[0], y: rect.minY)); path.addLine(to: CGPoint(x: rect.maxX - radii[1], y: rect.minY)); path.addArc(withCenter: CGPoint(x: rect.maxX - radii[1], y: rect.minY + radii[1]), radius: radii[1], startAngle: -.pi / 2, endAngle: 0, clockwise: true); path.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY - radii[2])); path.addArc(withCenter: CGPoint(x: rect.maxX - radii[2], y: rect.maxY - radii[2]), radius: radii[2], startAngle: 0, endAngle: .pi / 2, clockwise: true); path.addLine(to: CGPoint(x: rect.minX + radii[3], y: rect.maxY)); path.addArc(withCenter: CGPoint(x: rect.minX + radii[3], y: rect.maxY - radii[3]), radius: radii[3], startAngle: .pi / 2, endAngle: .pi, clockwise: true); path.addLine(to: CGPoint(x: rect.minX, y: rect.minY + radii[0])); path.addArc(withCenter: CGPoint(x: rect.minX + radii[0], y: rect.minY + radii[0]), radius: radii[0], startAngle: .pi, endAngle: .pi * 1.5, clockwise: true); path.close(); return path
   }
}
