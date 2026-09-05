import UIKit

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
