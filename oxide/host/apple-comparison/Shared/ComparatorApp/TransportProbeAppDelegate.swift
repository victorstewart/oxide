import UIKit

final class TransportProbeAppDelegate: NSObject, UIApplicationDelegate
{
   var window: UIWindow?
   private var presentationProbe: CorePresentationProbe?
   private var benchmark: CoreBenchmark?

   func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool
   {
      let window = UIWindow(frame: UIScreen.main.bounds)
      window.rootViewController = UIViewController()
      window.rootViewController?.view.backgroundColor = .white
      window.makeKeyAndVisible()
      self.window = window
      do
      {
         let arguments = CommandLine.arguments
         if let index = arguments.firstIndex(of: "-oxide-core-case"), index + 1 < arguments.count
         {
            let checkpointIndex = arguments.firstIndex(of: "-oxide-core-checkpoint")
            let checkpoint = checkpointIndex.flatMap {$0 + 1 < arguments.count ? Double(arguments[$0 + 1]) : nil}
            let suite = try CoreBenchmark(window: window, caseName: arguments[index + 1], checkpoint: checkpoint)
            benchmark = suite
            suite.start()
         }
         else
         {
            let probe = try CorePresentationProbe(window: window)
            presentationProbe = probe
            probe.start()
         }
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
