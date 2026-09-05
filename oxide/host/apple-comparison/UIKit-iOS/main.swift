import UIKit

func makeCoreProbeAdapter(window: UIWindow) throws -> CoreProbeAdapter
{
   try UIKitScenarioAdapter(window: window)
}

UIApplicationMain(CommandLine.argc, CommandLine.unsafeArgv, nil, NSStringFromClass(TransportProbeAppDelegate.self))
