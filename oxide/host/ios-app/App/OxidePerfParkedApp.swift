import Darwin
import UIKit

@objc(OxidePerfParkedAppDelegate)
@MainActor
final class OxidePerfParkedAppDelegate: UIResponder, UIApplicationDelegate
{
    func application(
        _ application: UIApplication,
        configurationForConnecting connectingSceneSession: UISceneSession,
        options: UIScene.ConnectionOptions
    ) -> UISceneConfiguration
    {
        let configuration = UISceneConfiguration(
            name: "OxidePerfParkedScene",
            sessionRole: connectingSceneSession.role
        )
        configuration.delegateClass = OxidePerfParkedSceneDelegate.self
        return configuration
    }
}

@objc(OxidePerfParkedSceneDelegate)
@MainActor
final class OxidePerfParkedSceneDelegate: UIResponder, UIWindowSceneDelegate
{
    var window: UIWindow?

    private var host: PerfSurfaceHost?
    private var benchmark: OxideUIKitBenchmark?
    private var startObserver: DarwinNotificationObserver?
    private var refreshUpdateLink: UIUpdateLink?
    private var didRunBenchmark = false
    private var oxidePerfRunnerSmoke = false
    private var pendingLaunchScenario: (scenario: OxideUIKitLaunchScenario, route: String?)?

    func scene(_ scene: UIScene, willConnectTo session: UISceneSession, options: UIScene.ConnectionOptions)
    {
        guard let windowScene = scene as? UIWindowScene else
        {
            fatalError("expected UIWindowScene for parked benchmark mode")
        }
        let environment = ProcessInfo.processInfo.environment
        refreshUpdateLink = makeUIKitRefreshUpdateLink(for: windowScene, environment: environment)
        if environment[perfOxideRunnerEnv] == "1"
        {
            oxidePerfRunnerSmoke = environment[perfOxideRunnerSmokeEnv] == "1"
            let window = UIWindow(windowScene: windowScene)
            let rootViewController = UIViewController()
            let view = UIView(frame: window.bounds)
            view.backgroundColor = .white
            rootViewController.view = view
            window.rootViewController = rootViewController
            window.makeKeyAndVisible()
            self.window = window
            self.startObserver = DarwinNotificationObserver(name: startNotificationName)
            {
                [weak self] in
                self?.runOxidePerfSuiteIfNeeded()
            }
            emitConsoleLine("OXIDE_READY oxide-perf-runner")
            postDarwinNotification(readyNotificationName)
            return
        }
        guard let caseName = environment[parkedCaseEnv],
              !caseName.isEmpty else
        {
            if let launch = resolveUIKitLaunchScenario(environment: environment)
            {
                if environment[perfTraceHandshakeEnv] == "1"
                {
                    let window = UIWindow(windowScene: windowScene)
                    let rootViewController = UIViewController()
                    let view = UIView(frame: window.bounds)
                    view.backgroundColor = .white
                    rootViewController.view = view
                    window.rootViewController = rootViewController
                    window.makeKeyAndVisible()
                    self.window = window
                    self.pendingLaunchScenario = launch
                    self.startObserver = DarwinNotificationObserver(name: startNotificationName)
                    {
                        [weak self] in
                        self?.runLaunchScenarioIfNeeded()
                    }
                    emitConsoleLine("OXIDE_READY \(launch.scenario.rawValue)")
                    postDarwinNotification(readyNotificationName)
                    return
                }
                let window = UIWindow(windowScene: windowScene)
                let rootViewController = makeUIKitLaunchRootViewController(
                    scenario: launch.scenario,
                    route: launch.route
                )
                window.rootViewController = rootViewController
                window.makeKeyAndVisible()
                self.window = window
                return
            }
            if ProcessInfo.processInfo.environment["XCTestConfigurationFilePath"] != nil
            {
                let window = UIWindow(windowScene: windowScene)
                let rootViewController = UIViewController()
                let view = UIView(frame: window.bounds)
                view.backgroundColor = .white
                rootViewController.view = view
                window.rootViewController = rootViewController
                window.makeKeyAndVisible()
                self.window = window
                return
            }
            fatalError("missing \(parkedCaseEnv) for parked benchmark mode")
        }

        let host = PerfSurfaceHost()
        let window = UIWindow(windowScene: windowScene)
        host.attach(to: window)
        guard let benchmark = OxideUIKitBenchmarkCatalog.makeBenchmark(named: caseName, host: host) else
        {
            fatalError("unknown parked benchmark case \(caseName)")
        }

        self.window = window
        self.host = host
        self.benchmark = benchmark
        self.startObserver = DarwinNotificationObserver(name: startNotificationName)
        {
            [weak self] in
            self?.runBenchmarkIfNeeded()
        }

        emitConsoleLine("OXIDE_READY \(caseName)")
        postDarwinNotification(readyNotificationName)
    }

    private func runBenchmarkIfNeeded()
    {
        guard !didRunBenchmark, let benchmark else
        {
            return
        }
        didRunBenchmark = true
        emitConsoleLine("OXIDE_START \(benchmark.testName)")
        runMeasuredBenchmarkPass(benchmark)
        for line in benchmark.summaryLines()
        {
            emitConsoleLine(line)
        }
        emitConsoleLine("OXIDE_COMPLETE \(benchmark.testName)")
        postDarwinNotification(completeNotificationName)
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.1)
        {
            _exit(0)
        }
    }

    private func runLaunchScenarioIfNeeded()
    {
        guard !didRunBenchmark,
              let launch = pendingLaunchScenario,
              let window else
        {
            return
        }
        didRunBenchmark = true
        emitConsoleLine("OXIDE_START \(launch.scenario.rawValue)")
        window.rootViewController = makeUIKitLaunchRootViewController(
            scenario: launch.scenario,
            route: launch.route
        )
        window.makeKeyAndVisible()
        window.layoutIfNeeded()
        CATransaction.flush()
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05)
        {
            emitConsoleLine("OXIDE_COMPLETE \(launch.scenario.rawValue)")
            postDarwinNotification(completeNotificationName)
        }
    }

    private func runOxidePerfSuiteIfNeeded()
    {
        guard !didRunBenchmark else
        {
            return
        }
        didRunBenchmark = true
        emitConsoleLine("OXIDE_START oxide-perf-runner")
        guard let json = collectOxidePerfRunnerJSON(smoke: oxidePerfRunnerSmoke) else
        {
            emitConsoleLine("OXIDE_COMPLETE oxide-perf-runner failed")
            postDarwinNotification(completeNotificationName)
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.1)
            {
                _exit(0)
            }
            return
        }
        emitOxidePerfRunnerJSON(json)
        emitConsoleLine("OXIDE_COMPLETE oxide-perf-runner")
        postDarwinNotification(completeNotificationName)
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.1)
        {
            _exit(0)
        }
    }
}
