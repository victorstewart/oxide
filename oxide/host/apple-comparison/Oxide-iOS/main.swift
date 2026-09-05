import UIKit

func makeCoreProbeAdapter(window: UIWindow) throws -> CoreProbeAdapter
{
   try OxideScenarioAdapter(window: window)
}

UIApplicationMain(CommandLine.argc, CommandLine.unsafeArgv, nil, NSStringFromClass(TransportProbeAppDelegate.self))
