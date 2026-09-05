import UIKit

final class TransportProbeAppDelegate: NSObject, UIApplicationDelegate
{
   var window: UIWindow?
   private var presentationProbe: CorePresentationProbe?

   func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool
   {
      let window = UIWindow(frame: UIScreen.main.bounds)
      window.rootViewController = UIViewController()
      window.rootViewController?.view.backgroundColor = .white
      window.makeKeyAndVisible()
      self.window = window
      do
      {
         let probe = try CorePresentationProbe(window: window)
         presentationProbe = probe
         probe.start()
      }
      catch
      {
         let label = UILabel(frame: window.bounds)
         label.numberOfLines = 0
         label.text = "Probe failed: \(error)"
         window.rootViewController?.view.addSubview(label)
      }
      return true
   }
}
