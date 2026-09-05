import UIKit

func makeBenchmarkScenarioAdapter(window: UIWindow) -> BenchmarkScenarioAdapter?
{
   OxideScenarioAdapter(window: window)
}

UIApplicationMain(CommandLine.argc, CommandLine.unsafeArgv, nil, NSStringFromClass(TransportProbeAppDelegate.self))
