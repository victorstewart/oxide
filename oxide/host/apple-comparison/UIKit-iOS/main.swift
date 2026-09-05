import UIKit

func makeBenchmarkScenarioAdapter(window: UIWindow) -> BenchmarkScenarioAdapter?
{
   UIKitScenarioAdapter(window: window)
}

UIApplicationMain(CommandLine.argc, CommandLine.unsafeArgv, nil, NSStringFromClass(TransportProbeAppDelegate.self))
