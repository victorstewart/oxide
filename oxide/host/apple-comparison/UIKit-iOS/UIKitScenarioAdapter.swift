import UIKit
import CoreText

func makeCoreSuiteAdapter(window: UIWindow, caseName: String, checkpoint: Bool) throws -> CoreSuiteAdapter
{
   if caseName == "visual-editing-edges" {return try UIKitEditingEdges(window: window, caseName: caseName, checkpoint: checkpoint)}
   if ["visual-opacity", "visual-images", "visual-geometry"].contains(caseName) {return try UIKitExtendedBoards(window: window, caseName: caseName, checkpoint: checkpoint)}
   if caseName.hasPrefix("visual-")
   {
      return try UIKitVisualBoards(window: window, caseName: caseName, checkpoint: checkpoint)
   }
   let resources = try CoreResources.load()
   return UIKitCoreSuite(window: window, caseName: caseName, checkpoint: checkpoint, resources: resources)
}

private struct CoreResources
{
   let spec: CoreSpec
   let font: UIFont
   let images: [UIImage]

   static func load() throws -> CoreResources
   {
      guard let jsonURL = Bundle.main.url(forResource: "core", withExtension: "json") else
      {
         throw NSError(domain: "CoreSuite", code: 1, userInfo: [NSLocalizedDescriptionKey: "Missing core.json"])
      }
      let json = try Data(contentsOf: jsonURL)
      let spec = try JSONDecoder().decode(CoreSpec.self, from: json)
      guard spec.color_space == "srgb", spec.compositing == "srgb-source-over" else
      {
         throw NSError(domain: "CoreSuite", code: 6, userInfo: [NSLocalizedDescriptionKey: "Unsupported fixture color contract"])
      }
      let fontParts = spec.font.split(separator: ".", maxSplits: 1).map(String.init)
      guard fontParts.count == 2,
            let fontURL = Bundle.main.url(forResource: fontParts[0], withExtension: fontParts[1]),
            let provider = CGDataProvider(url: fontURL as CFURL),
            let graphicsFont = CGFont(provider) else
      {
         throw NSError(domain: "CoreSuite", code: 3, userInfo: [NSLocalizedDescriptionKey: "Missing bundled Noto Sans font"])
      }
      var registrationError: Unmanaged<CFError>?
      guard CTFontManagerRegisterFontsForURL(fontURL as CFURL, .process, &registrationError) || registrationError == nil else
      {
         throw registrationError!.takeRetainedValue()
      }
      guard let postScriptName = graphicsFont.postScriptName as String?,
            let font = UIFont(name: postScriptName, size: 14) else
      {
         throw NSError(domain: "CoreSuite", code: 4, userInfo: [NSLocalizedDescriptionKey: "Unable to load bundled Noto Sans"])
      }
      let images = try spec.images.map
      {
         name -> UIImage in
         let parts = name.split(separator: ".", maxSplits: 1).map(String.init)
         guard parts.count == 2, let image = UIImage(named: name) ?? Bundle.main.path(forResource: parts[0], ofType: parts[1]).flatMap(UIImage.init(contentsOfFile:)) else
         {
            throw NSError(domain: "CoreSuite", code: 5, userInfo: [NSLocalizedDescriptionKey: "Missing bundled \(name)"])
         }
         return image
      }
      return CoreResources(spec: spec, font: font, images: images)
   }
}

private struct CoreSpec: Decodable
{
   let color_space: String
   let compositing: String
   let viewport: [Double]
   let font: String
   let font_size: Double
   let text_color: [Double]
   let images: [String]
   let shapes: Shapes
   let text: Text
   let local: Local
   let images_case: ImagesCase
   let animation: Animation
   let scroll: Scroll

   struct Shapes: Decodable {let count: Int; let clip_rects: [[Double]]; let tile_size: [Double]; let column_stride: Double; let row_stride: Double; let origin: [Double]; let radii: [Double]; let alpha: Double; let updates_per_frame: Int}
   struct Text: Decodable {let count: Int; let origin: [Double]; let stride: [Double]; let size: [Double]; let columns: Int; let font_size: Double; let template: String; let interval: Double}
   struct Local: Decodable {let count: Int; let origin: [Double]; let stride: [Double]; let columns: Int; let label_size: [Double]; let button_rect: [Double]; let progress_rect: [Double]; let font_size: Double; let template: String; let button_title: String; let button_color: [Double]; let progress_track_color: [Double]; let progress_fill_color: [Double]; let interval: Double}
   struct ImagesCase: Decodable {let count: Int; let origin: [Double]; let stride: [Double]; let columns: Int; let size: [Double]; let scale_amplitude: Double; let period: Double}
   struct Animation: Decodable {let count: Int; let origin: [Double]; let stride: [Double]; let columns: Int; let size: [Double]; let image_rect: [Double]; let label_rect: [Double]; let font_size: Double; let period: Double; let translation: [Double]; let scale_center: Double; let scale_amplitude: Double; let alpha_center: Double; let alpha_amplitude: Double; let template: String}
   struct Scroll: Decodable {let count: Int; let row_height: Double; let distance: Double; let half_period: Double; let image_rect: [Double]; let label_rect: [Double]; let font_size: Double; let template: String}
}

private final class UIKitCoreSuite: NSObject, CoreSuiteAdapter, UICollectionViewDataSource
{
   private let root: UIView
   private let name: String
   private let checkpoint: Bool
   private let spec: CoreSpec
   private let font: UIFont
   private let images: [UIImage]
   private var views = [UIView]()
   private var labels = [UILabel]()
   private var scroll: UICollectionView?
   private var progressViews = [CoreProgressView]()
   private var imageViews = [UIImageView]()
   private var animationElements = [AnimationElement]()
   private var lastStep = -1
   private var nativeAnimationStarted = false

   init(window: UIWindow, caseName: String, checkpoint: Bool, resources: CoreResources)
   {
      name = caseName
      self.checkpoint = checkpoint
      spec = resources.spec
      font = resources.font
      images = resources.images
      root = UIView(frame: CGRect(x: (window.bounds.width - resources.spec.viewport[0]) / 2, y: (window.bounds.height - resources.spec.viewport[1]) / 2, width: resources.spec.viewport[0], height: resources.spec.viewport[1]))
      super.init()
      root.backgroundColor = .white
      window.rootViewController?.view.addSubview(root)
      switch name
      {
      case "shapes": shapes()
      case "text": text()
      case "local": local()
      case "images": imageCase()
      case "animation": animation()
      case "scroll": scrolling()
      default: break
      }
   }

   func render(time: Double, generation: UInt64) throws
   {
      CATransaction.begin()
      CATransaction.setDisableActions(true)
      switch name
      {
      case "shapes": renderShapes(time: time, generation: generation)
      case "text", "local": renderStep(time: time)
      case "images": renderImages(time: time)
      case "animation": renderAnimation(time: time)
      case "scroll": renderScroll(time: time)
      default: break
      }
      CATransaction.commit()
   }

   func reset()
   {
      lastStep = -1
      nativeAnimationStarted = false
      try? render(time: -1, generation: 0)
   }

   func advance(time: Double, generation: UInt64) throws -> Bool
   {
      switch name
      {
      case "text", "local":
         let step = Int(floor(max(0, time) * 10))
         guard step > lastStep else {return false}
      case "animation":
         guard !nativeAnimationStarted || checkpoint else {return false}
      default: break
      }
      return true
   }

   func nextWakeup(after time: Double) -> Double
   {
      switch name
      {
      case "text", "local": return (floor(max(0, time) * 10) + 1) / 10
      case "animation": return nativeAnimationStarted && !checkpoint ? .infinity : 0
      default: return 0
      }
   }

   func renderControl(time: Double, generation: UInt64) throws
   {
      if name == "animation"
      {
         settle()
         nativeAnimationStarted = false
         try render(time: -1, generation: generation)
      }
      else {try render(time: 0, generation: generation)}
   }

   func settle()
   {
      CATransaction.begin()
      CATransaction.setDisableActions(true)
      for element in animationElements
      {
         if let presentation = element.view.layer.presentation()
         {
            element.view.layer.position = presentation.position
            element.view.layer.transform = presentation.transform
            element.view.layer.opacity = presentation.opacity
         }
      }
      views.forEach {$0.layer.removeAllAnimations()}
      animationElements.forEach {$0.view.layer.removeAllAnimations()}
      root.layer.removeAllAnimations()
      CATransaction.commit()
   }

   private func renderShapes(time: Double, generation: UInt64)
   {
      let color: (Double) -> UIColor = {value in UIColor(red: value, green: 0.25, blue: 0.5, alpha: self.spec.shapes.alpha)}
      guard time >= 0 else
      {
         views.forEach {$0.backgroundColor = color(0)}
         return
      }
      if checkpoint
      {
         let end = Int(generation)
         for frame in max(0, end - 7)...end
         {
            let scalar = (sin(Double(frame) / 120 * 3) + 1) / 2
            let first = (frame % 8) * 8
            for index in first..<(first + 8) {views[index].backgroundColor = color(scalar)}
         }
      }
      else
      {
         let scalar = (sin(time * 3) + 1) / 2
         let first = Int(generation % 8) * 8
         for index in first..<(first + 8) {views[index].backgroundColor = color(scalar)}
      }
   }

   private func renderStep(time: Double)
   {
      guard time >= 0 else {return}
      let step = Int(floor(time * 10))
      guard step > lastStep else {return}
      for value in (lastStep + 1)...step
      {
         let index = value % labels.count
         labels[index].text = name == "text"
            ? String(format: spec.text.template, index, value + 1)
            : String(format: spec.local.template, index, value + 1)
         if name == "local" {progressViews[index].progress = Float((value + 1) % 100) / 100}
      }
      lastStep = step
   }

   private func renderImages(time: Double)
   {
      let t = max(0, time)
      for (index, view) in imageViews.enumerated()
      {
         let phase = Double(index) * 0.3
         let scale = 1 + spec.images_case.scale_amplitude * (sin(2 * Double.pi * t / spec.images_case.period + phase) + 1) / 2
         let side = CGFloat(spec.images_case.size[1] * scale)
         let width = CGFloat(spec.images_case.size[0])
         view.bounds = CGRect(x: 0, y: 0, width: side, height: side)
         view.center = CGPoint(x: min(0, max(width - side, (width - side) / 2 + CGFloat(sin(2 * Double.pi * t / spec.images_case.period + phase) * Double(width) * 0.1))) + side / 2, y: CGFloat(spec.images_case.size[1]) / 2)
      }
   }

   private func renderAnimation(time: Double)
   {
      guard !checkpoint, time >= 0 else
      {
         animationElements.forEach {applyAnimationState($0, time: max(0, time))}
         return
      }
      guard !nativeAnimationStarted else {return}
      nativeAnimationStarted = true
      let now = CACurrentMediaTime()
      for element in animationElements
      {
         let animations = animationAnimations(element)
         let group = CAAnimationGroup()
         group.animations = animations
         group.duration = spec.animation.period
         group.beginTime = now - time
         group.repeatCount = .greatestFiniteMagnitude
         element.view.layer.add(group, forKey: "core-animation")
      }
   }

   private func applyAnimationState(_ element: AnimationElement, time: Double)
   {
      let phase = 2 * Double.pi * time / spec.animation.period + Double(element.index) * 0.3
      let scale = spec.animation.scale_center + spec.animation.scale_amplitude * sin(phase)
      let translation = CGPoint(x: spec.animation.translation[0] * sin(phase), y: spec.animation.translation[1] * cos(phase))
      element.view.center = CGPoint(x: element.cardCenter.x + (element.baseCenter.x - element.cardCenter.x) * scale + translation.x, y: element.cardCenter.y + (element.baseCenter.y - element.cardCenter.y) * scale + translation.y)
      element.view.transform = CGAffineTransform(scaleX: scale, y: scale)
      element.view.alpha = spec.animation.alpha_center + spec.animation.alpha_amplitude * sin(phase)
   }

   private func animationAnimations(_ element: AnimationElement) -> [CAAnimation]
   {
      let samples = 0...32
      let positions = samples.map {sample -> NSValue in let phase = Double(sample) * Double.pi * 2 / 32 + Double(element.index) * 0.3; let scale = spec.animation.scale_center + spec.animation.scale_amplitude * sin(phase); return NSValue(cgPoint: CGPoint(x: element.cardCenter.x + (element.baseCenter.x - element.cardCenter.x) * scale + spec.animation.translation[0] * sin(phase), y: element.cardCenter.y + (element.baseCenter.y - element.cardCenter.y) * scale + spec.animation.translation[1] * cos(phase))) }
      let position = CAKeyframeAnimation(keyPath: "position"); position.values = positions; position.duration = spec.animation.period; position.calculationMode = .linear
      let scale = sineAnimation(keyPath: "transform.scale", amplitude: spec.animation.scale_amplitude, phase: Double(element.index) * 0.3, offset: 0, center: spec.animation.scale_center)
      let alpha = sineAnimation(keyPath: "opacity", amplitude: spec.animation.alpha_amplitude, phase: Double(element.index) * 0.3, offset: 0, center: spec.animation.alpha_center)
      return [position, scale, alpha]
   }

   private func sineAnimation(keyPath: String, amplitude: Double, phase: Double, offset: Double, center: Double = 0) -> CAKeyframeAnimation
   {
      let animation = CAKeyframeAnimation(keyPath: keyPath)
      animation.values = (0...32).map
      {
         sample in NSNumber(value: center + amplitude * sin(Double(sample) * Double.pi * 2 / 32 + phase + offset))
      }
      animation.keyTimes = (0...32).map {NSNumber(value: Double($0) / 32)}
      animation.duration = spec.animation.period
      animation.calculationMode = .linear
      return animation
   }

   private func renderScroll(time: Double)
   {
      let t = max(0, time).truncatingRemainder(dividingBy: spec.scroll.half_period * 2)
      let progress = t <= spec.scroll.half_period ? t / spec.scroll.half_period : (spec.scroll.half_period * 2 - t) / spec.scroll.half_period
      scroll?.contentOffset = CGPoint(x: 0, y: progress * spec.scroll.distance)
   }

   private func shapes()
   {
      for group in 0..<(spec.shapes.count / 16)
      {
         let rect = spec.shapes.clip_rects[group]
         let clip = UIView(frame: CGRect(x: rect[0], y: rect[1], width: rect[2], height: rect[3]))
         clip.clipsToBounds = true
         root.addSubview(clip)
         for index in 0..<16
         {
            let tile = UIView(frame: CGRect(x: spec.shapes.origin[0] + Double(index % 4) * spec.shapes.column_stride, y: spec.shapes.origin[1] + Double(index / 4) * spec.shapes.row_stride, width: spec.shapes.tile_size[0], height: spec.shapes.tile_size[1]))
            tile.layer.cornerRadius = spec.shapes.radii[index % spec.shapes.radii.count]
            tile.backgroundColor = UIColor(red: 0, green: 0.25, blue: 0.5, alpha: spec.shapes.alpha)
            clip.addSubview(tile)
            views.append(tile)
         }
      }
   }

   private func text()
   {
      for index in 0..<spec.text.count
      {
         let label = CoreTopLabel(frame: CGRect(x: spec.text.origin[0] + Double(index % spec.text.columns) * spec.text.stride[0], y: spec.text.origin[1] + Double(index / spec.text.columns) * spec.text.stride[1], width: spec.text.size[0], height: spec.text.size[1]))
         label.font = font.withSize(spec.text.font_size)
         label.textColor = UIColor(red: spec.text_color[0], green: spec.text_color[1], blue: spec.text_color[2], alpha: spec.text_color[3])
         label.numberOfLines = 2
         label.text = String(format: spec.text.template, index, 0)
         root.addSubview(label)
         labels.append(label)
      }
   }

   private func local()
   {
      for index in 0..<spec.local.count
      {
         let x = spec.local.origin[0] + Double(index % spec.local.columns) * spec.local.stride[0]
         let y = spec.local.origin[1] + Double(index / spec.local.columns) * spec.local.stride[1]
         let label = CoreTopLabel(frame: CGRect(x: x, y: y, width: spec.local.label_size[0], height: spec.local.label_size[1]))
         label.font = font.withSize(spec.local.font_size)
         label.textColor = UIColor(red: spec.text_color[0], green: spec.text_color[1], blue: spec.text_color[2], alpha: spec.text_color[3])
         label.numberOfLines = 2
         label.text = String(format: spec.local.template, index, 0)
         root.addSubview(label)
         labels.append(label)
         let button = UIButton(type: .custom)
         button.frame = CGRect(x: x + spec.local.button_rect[0], y: y + spec.local.button_rect[1], width: spec.local.button_rect[2], height: spec.local.button_rect[3])
         button.backgroundColor = UIColor(red: spec.local.button_color[0], green: spec.local.button_color[1], blue: spec.local.button_color[2], alpha: spec.local.button_color[3])
         button.layer.cornerRadius = 6
         button.titleLabel?.font = font.withSize(spec.local.font_size)
         button.setTitleColor(.white, for: .normal)
         button.setTitle(spec.local.button_title, for: .normal)
         root.addSubview(button)
         let progress = CoreProgressView(frame: CGRect(x: x + spec.local.progress_rect[0], y: y + spec.local.progress_rect[1], width: spec.local.progress_rect[2], height: spec.local.progress_rect[3]))
         progress.trackTintColor = UIColor(red: spec.local.progress_track_color[0], green: spec.local.progress_track_color[1], blue: spec.local.progress_track_color[2], alpha: spec.local.progress_track_color[3])
         progress.progressTintColor = UIColor(red: spec.local.progress_fill_color[0], green: spec.local.progress_fill_color[1], blue: spec.local.progress_fill_color[2], alpha: spec.local.progress_fill_color[3])
         progress.useFlatConfiguredColors()
         progress.progress = 0
         root.addSubview(progress)
         progressViews.append(progress)
      }
   }

   private func imageCase()
   {
      for index in 0..<spec.images_case.count
      {
         let x = spec.images_case.origin[0] + Double(index % spec.images_case.columns) * spec.images_case.stride[0]
         let y = spec.images_case.origin[1] + Double(index / spec.images_case.columns) * spec.images_case.stride[1]
         let clip = UIView(frame: CGRect(x: x, y: y, width: spec.images_case.size[0], height: spec.images_case.size[1]))
         clip.clipsToBounds = true
         let image = UIImageView(frame: CGRect(x: 0, y: 0, width: spec.images_case.size[1], height: spec.images_case.size[1]))
         image.image = images[index % 4]
         image.contentMode = .scaleAspectFill
         image.clipsToBounds = true
         clip.addSubview(image)
         root.addSubview(clip)
         imageViews.append(image)
      }
   }

   private func animation()
   {
      for index in 0..<spec.animation.count
      {
         let x = spec.animation.origin[0] + Double(index % spec.animation.columns) * spec.animation.stride[0]
         let y = spec.animation.origin[1] + Double(index / spec.animation.columns) * spec.animation.stride[1]
         let cardCenter = CGPoint(x: x + spec.animation.size[0] / 2, y: y + spec.animation.size[1] / 2)
         let card = UIView(frame: CGRect(x: x, y: y, width: spec.animation.size[0], height: spec.animation.size[1]))
         card.backgroundColor = UIColor(red: 0.94, green: 0.95, blue: 0.97, alpha: 1)
         card.layer.cornerRadius = 8
         root.addSubview(card)
         animationElements.append(AnimationElement(view: card, index: index, baseCenter: cardCenter, cardCenter: cardCenter))
         let image = UIImageView(frame: CGRect(x: x + spec.animation.image_rect[0], y: y + spec.animation.image_rect[1], width: spec.animation.image_rect[2], height: spec.animation.image_rect[3]))
         image.image = images[index % 4]
         image.contentMode = .scaleAspectFill
         image.clipsToBounds = true
         root.addSubview(image)
         animationElements.append(AnimationElement(view: image, index: index, baseCenter: image.center, cardCenter: cardCenter))
         let label = CoreTopLabel(frame: CGRect(x: x + spec.animation.label_rect[0], y: y + spec.animation.label_rect[1], width: spec.animation.label_rect[2], height: spec.animation.label_rect[3]))
         label.font = font.withSize(spec.animation.font_size)
         label.textColor = UIColor(red: spec.text_color[0], green: spec.text_color[1], blue: spec.text_color[2], alpha: spec.text_color[3])
         label.numberOfLines = 2
         label.text = String(format: spec.animation.template, index)
         root.addSubview(label)
         animationElements.append(AnimationElement(view: label, index: index, baseCenter: label.center, cardCenter: cardCenter))
      }
   }

   private func scrolling()
   {
      let layout = UICollectionViewFlowLayout()
      layout.itemSize = CGSize(width: spec.viewport[0], height: spec.scroll.row_height)
      layout.minimumLineSpacing = 0
      let collection = UICollectionView(frame: root.bounds, collectionViewLayout: layout)
      collection.dataSource = self
      collection.showsVerticalScrollIndicator = false
      collection.register(CoreScrollCell.self, forCellWithReuseIdentifier: "core-row")
      root.addSubview(collection)
      scroll = collection
   }

   func collectionView(_ collectionView: UICollectionView, numberOfItemsInSection section: Int) -> Int
   {
      spec.scroll.count
   }

   func collectionView(_ collectionView: UICollectionView, cellForItemAt indexPath: IndexPath) -> UICollectionViewCell
   {
      let cell = collectionView.dequeueReusableCell(withReuseIdentifier: "core-row", for: indexPath) as! CoreScrollCell
      cell.configure(index: indexPath.item, image: images[indexPath.item % 4], font: font, spec: spec)
      return cell
   }
}

private struct AnimationElement
{
   let view: UIView
   let index: Int
   let baseCenter: CGPoint
   let cardCenter: CGPoint
}

final class CoreProgressView: UIProgressView
{
   override init(frame: CGRect)
   {
      super.init(frame: CGRect(x: frame.minX, y: frame.midY - 1, width: frame.width, height: 2))
      // UIProgressView may choose a different native height than the requested two points.
      let nativeHeight = bounds.height
      center = CGPoint(x: frame.midX, y: frame.midY)
      transform = CGAffineTransform(scaleX: 1, y: frame.height / max(nativeHeight, 1))
   }

   // Native tint rendering may add a gradient. The shared fixture asks for flat colors.
   func useFlatConfiguredColors()
   {
      func image(_ color: UIColor) -> UIImage
      {
         let renderer = UIGraphicsImageRenderer(size: CGSize(width: 1, height: 1))
         return renderer.image {context in
            color.setFill()
            context.fill(CGRect(x: 0, y: 0, width: 1, height: 1))
         }.resizableImage(withCapInsets: .zero)
      }
      trackImage = image(trackTintColor ?? .clear)
      progressImage = image(progressTintColor ?? .clear)
      layer.cornerRadius = bounds.height * 0.5
      layer.masksToBounds = true
   }

   required init?(coder: NSCoder) {fatalError("init(coder:) has not been implemented")}
}

final class CoreTopLabel: UILabel
{
   override func drawText(in rect: CGRect)
   {
      super.drawText(in: CGRect(x: rect.minX, y: rect.minY, width: rect.width, height: sizeThatFits(rect.size).height))
   }
}

private final class CoreScrollCell: UICollectionViewCell
{
   private let image = UIImageView(frame: CGRect(x: 16, y: 24, width: 48, height: 48))
   private let label = CoreTopLabel(frame: CGRect(x: 80, y: 20, width: 294, height: 56))

   override init(frame: CGRect)
   {
      super.init(frame: frame)
      image.contentMode = .scaleAspectFill
      image.clipsToBounds = true
      contentView.addSubview(image)
      label.numberOfLines = 2
      label.textColor = UIColor(red: 0.12, green: 0.16, blue: 0.2, alpha: 1)
      contentView.addSubview(label)
   }

   required init?(coder: NSCoder)
   {
      fatalError("init(coder:) has not been implemented")
   }

   func configure(index: Int, image: UIImage, font: UIFont, spec: CoreSpec)
   {
      contentView.backgroundColor = index % 2 == 0 ? .white : UIColor(red: 0.94, green: 0.95, blue: 0.97, alpha: 1)
      self.image.image = image
      self.image.frame = CGRect(x: spec.scroll.image_rect[0], y: spec.scroll.image_rect[1], width: spec.scroll.image_rect[2], height: spec.scroll.image_rect[3])
      label.frame = CGRect(x: spec.scroll.label_rect[0], y: spec.scroll.label_rect[1], width: spec.scroll.label_rect[2], height: spec.scroll.label_rect[3])
      label.font = font.withSize(spec.scroll.font_size)
      label.text = String(format: spec.scroll.template, index)
   }
}

final class UIKitScenarioAdapter: CoreProbeAdapter
{
   private var tiles = [UIView]()

   init(window: UIWindow) throws
   {
      let root = UIView(frame: CGRect(x: (window.bounds.width - 390) / 2, y: (window.bounds.height - 844) / 2, width: 390, height: 844))
      root.backgroundColor = .white
      window.rootViewController?.view.addSubview(root)
      for index in 0..<64
      {
         let tile = UIView(frame: CGRect(x: 15 + (index % 8) * 45, y: 22 + (index / 8) * 100, width: 36, height: 80))
         tile.layer.cornerRadius = 6
         root.addSubview(tile)
         tiles.append(tile)
      }
   }

   func render(values: [Float], generation: UInt64) throws
   {
      CATransaction.begin()
      CATransaction.setDisableActions(true)
      for index in tiles.indices
      {
         tiles[index].backgroundColor = UIColor(red: CGFloat(values[index]), green: 0.25, blue: 0.5, alpha: 1)
      }
      CATransaction.commit()
   }
}
