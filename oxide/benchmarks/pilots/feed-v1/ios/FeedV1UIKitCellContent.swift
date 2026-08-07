import QuartzCore
import UIKit

struct FeedV1CellFrames
{
   let image: CGRect
   let title: CGRect
   let caption: CGRect
   let metadata: CGRect
   let separator: CGRect

   static func make(rowHeightPoints: Int) -> FeedV1CellFrames
   {
      let imageX = CGFloat(FeedV1Contract.rowLeadingPoints)
      let textX = imageX + CGFloat(FeedV1Contract.imageSidePoints + FeedV1Contract.imageTextGapPoints)
      let textWidth = CGFloat(
         FeedV1Contract.surfaceWidthPoints
         - FeedV1Contract.rowLeadingPoints
         - FeedV1Contract.imageSidePoints
         - FeedV1Contract.imageTextGapPoints
         - FeedV1Contract.rowTrailingPoints
      )
      let lineCount = FeedV1Recipe.lineCount(forHeight: rowHeightPoints)
      let captionHeight = CGFloat(lineCount * FeedV1Contract.captionLineHeightPoints)
      let metadataY = CGFloat(FeedV1Contract.captionTopPoints) + captionHeight + CGFloat(FeedV1Contract.metadataGapPoints)
      let separatorHeight = CGFloat(FeedV1Contract.separatorPhysicalPixels) / CGFloat(FeedV1Contract.surfaceScale)

      return FeedV1CellFrames(
         image: CGRect(
            x: imageX,
            y: CGFloat(FeedV1Contract.rowTopPoints),
            width: CGFloat(FeedV1Contract.imageSidePoints),
            height: CGFloat(FeedV1Contract.imageSidePoints)
         ),
         title: CGRect(
            x: textX,
            y: CGFloat(FeedV1Contract.rowTopPoints),
            width: textWidth,
            height: CGFloat(FeedV1Contract.titleHeightPoints)
         ),
         caption: CGRect(
            x: textX,
            y: CGFloat(FeedV1Contract.captionTopPoints),
            width: textWidth,
            height: captionHeight
         ),
         metadata: CGRect(
            x: textX,
            y: metadataY,
            width: textWidth,
            height: CGFloat(FeedV1Contract.metadataHeightPoints)
         ),
         separator: CGRect(
            x: 0,
            y: CGFloat(rowHeightPoints) - separatorHeight,
            width: CGFloat(FeedV1Contract.surfaceWidthPoints),
            height: separatorHeight
         )
      )
   }
}

@MainActor
final class FeedV1CellContent
{
   private let resources: FeedV1UIKitResources
   private let cacheCaptionComposition: Bool
   private let imageShadowView = UIView(frame: .zero)
   private let imageView = UIImageView(frame: .zero)
   private let titleLabel = UILabel(frame: .zero)
   private let captionLabel = UILabel(frame: .zero)
   private let metadataLabel = UILabel(frame: .zero)
   private let separatorView = UIView(frame: .zero)

   init(contentView: UIView, resources: FeedV1UIKitResources, cacheCaptionComposition: Bool)
   {
      self.resources = resources
      self.cacheCaptionComposition = cacheCaptionComposition

      contentView.backgroundColor = resources.backgroundColor
      contentView.clipsToBounds = true
      contentView.contentScaleFactor = CGFloat(FeedV1Contract.surfaceScale)
      contentView.semanticContentAttribute = .forceLeftToRight

      imageShadowView.backgroundColor = .clear
      imageShadowView.layer.shadowColor = resources.shadowColor.cgColor
      imageShadowView.layer.shadowOpacity = Float(FeedV1Contract.shadowLayerOpacityByte) / 255
      imageShadowView.layer.shadowRadius = CGFloat(FeedV1Contract.shadowBlurRadiusPoints)
      imageShadowView.layer.shadowOffset = CGSize(
         width: FeedV1Contract.shadowOffsetXPoints,
         height: FeedV1Contract.shadowOffsetYPoints
      )
      imageShadowView.layer.masksToBounds = false
      imageShadowView.layer.shadowPath = UIBezierPath(
         roundedRect: CGRect(
            x: 0,
            y: 0,
            width: FeedV1Contract.imageSidePoints,
            height: FeedV1Contract.imageSidePoints
         ),
         cornerRadius: CGFloat(FeedV1Contract.imageCornerRadiusPoints)
      ).cgPath
      imageShadowView.contentScaleFactor = CGFloat(FeedV1Contract.surfaceScale)

      imageView.contentMode = .scaleToFill
      imageView.clipsToBounds = true
      imageView.layer.cornerRadius = CGFloat(FeedV1Contract.imageCornerRadiusPoints)
      imageView.layer.cornerCurve = .circular
      imageView.layer.magnificationFilter = .nearest
      imageView.layer.minificationFilter = .nearest
      imageView.contentScaleFactor = CGFloat(FeedV1Contract.surfaceScale)
      imageShadowView.addSubview(imageView)

      configure(
         titleLabel,
         font: resources.titleFont,
         color: resources.titleColor,
         numberOfLines: 1
      )
      configure(
         captionLabel,
         font: resources.captionFont,
         color: resources.captionColor,
         numberOfLines: 4
      )
      configure(
         metadataLabel,
         font: resources.metadataFont,
         color: resources.metadataColor,
         numberOfLines: 1
      )
      separatorView.backgroundColor = resources.separatorColor
      separatorView.contentScaleFactor = CGFloat(FeedV1Contract.surfaceScale)

      contentView.addSubview(imageShadowView)
      contentView.addSubview(titleLabel)
      contentView.addSubview(captionLabel)
      contentView.addSubview(metadataLabel)
      contentView.addSubview(separatorView)
   }

   func apply(row: FeedV1Row) throws
   {
      titleLabel.text = row.title
      captionLabel.attributedText = resources.caption(
         row: row,
         cacheComposition: cacheCaptionComposition
      )
      metadataLabel.text = row.metadata
      imageView.image = try resources.checkerImage(variant: row.checkerVariant)
   }

   func layout(frames: FeedV1CellFrames)
   {
      imageShadowView.frame = frames.image
      imageView.frame = imageShadowView.bounds
      titleLabel.frame = frames.title
      captionLabel.frame = frames.caption
      metadataLabel.frame = frames.metadata
      separatorView.frame = frames.separator
   }

   func clear()
   {
      titleLabel.text = nil
      captionLabel.attributedText = nil
      metadataLabel.text = nil
      imageView.image = nil
   }

   private func configure(_ label: UILabel, font: UIFont, color: UIColor, numberOfLines: Int)
   {
      label.backgroundColor = .clear
      label.font = font
      label.textColor = color
      label.numberOfLines = numberOfLines
      label.lineBreakMode = .byClipping
      label.textAlignment = .left
      label.contentMode = .topLeft
      label.semanticContentAttribute = .forceLeftToRight
      label.contentScaleFactor = CGFloat(FeedV1Contract.surfaceScale)
      label.adjustsFontSizeToFitWidth = false
   }
}
