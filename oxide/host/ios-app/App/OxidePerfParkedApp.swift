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
    private var visibleTestOverlay: PerfVisibleTestOverlay?
    private var didRunBenchmark = false
    private var didScheduleTraceAutostart = false
    private var oxidePerfRunnerSmoke = false
    private var pendingLaunchScenario: (
        scenario: OxideUIKitLaunchScenario,
        route: String?,
        style: OxideUIKitLaunchStyle
    )?

    private var traceAutostartEnabled: Bool
    {
        ProcessInfo.processInfo.environment[perfTraceAutostartEnv] == "1"
    }

    private var watchModeEnabled: Bool
    {
        perfWatchModeEnabled()
    }

    private func scheduleTraceAutostartIfRequested(_ body: @escaping @MainActor () -> Void)
    {
        guard ProcessInfo.processInfo.environment[perfTraceAutostartEnv] == "1" else
        {
            return
        }
        let delay = resolvePerfTraceSettleSeconds()
        emitConsoleLine("OXIDE_STAGE parked.autostart.schedule delay=\(delay)")
        DispatchQueue.main.asyncAfter(deadline: .now() + delay)
        {
            emitConsoleLine("OXIDE_STAGE parked.autostart.fire")
            body()
        }
    }

    private func installVisibleTestOverlay(for window: UIWindow, text: String?)
    {
        let overlay = PerfVisibleTestOverlay(
            referenceWindow: window,
            preferBottom: watchModeEnabled
        )
        overlay.setText(text)
        visibleTestOverlay = overlay
    }

    private func scheduleWatchAutostartIfNeeded()
    {
        guard watchModeEnabled,
              !traceAutostartEnabled,
              !didRunBenchmark,
              benchmark != nil else
        {
            return
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.25)
        {
            [weak self] in
            self?.runBenchmarkIfNeeded()
        }
    }

    private func schedulePendingTraceAutostartIfNeeded()
    {
        guard traceAutostartEnabled,
              !didScheduleTraceAutostart else
        {
            return
        }
        didScheduleTraceAutostart = true
        if ProcessInfo.processInfo.environment[perfOxideRunnerEnv] == "1"
        {
            scheduleTraceAutostartIfRequested
            {
                [weak self] in
                self?.runOxidePerfSuiteIfNeeded()
            }
            return
        }
        if pendingLaunchScenario != nil
        {
            scheduleTraceAutostartIfRequested
            {
                [weak self] in
                self?.runLaunchScenarioIfNeeded()
            }
            return
        }
        if benchmark != nil
        {
            scheduleTraceAutostartIfRequested
            {
                [weak self] in
                self?.runBenchmarkIfNeeded()
            }
        }
    }

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
            installVisibleTestOverlay(
                for: window,
                text: resolvePerfDisplayLabel(environment: environment)
            )
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
                    installVisibleTestOverlay(
                        for: window,
                        text: resolvePerfDisplayLabel(environment: environment)
                    )
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
                    route: launch.route,
                    style: launch.style
                )
                window.rootViewController = rootViewController
                window.makeKeyAndVisible()
                self.window = window
                installVisibleTestOverlay(
                    for: window,
                    text: resolvePerfDisplayLabel(environment: environment)
                )
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
                installVisibleTestOverlay(
                    for: window,
                    text: resolvePerfDisplayLabel(environment: environment)
                )
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
        host.setVisibleTestLabel(resolvePerfDisplayLabel(environment: environment))
        self.benchmark = withVisibleOutputValidation(
            withWatchFrameCapture(benchmark, host: host),
            host: host
        )
        self.startObserver = DarwinNotificationObserver(name: startNotificationName)
        {
            [weak self] in
            emitConsoleLine("OXIDE_STAGE parked.start.notify \(caseName)")
            self?.runBenchmarkIfNeeded()
        }

        emitConsoleLine("OXIDE_READY \(caseName)")
        postDarwinNotification(readyNotificationName)
    }

    func sceneDidBecomeActive(_ scene: UIScene)
    {
        emitConsoleLine("OXIDE_STAGE parked.sceneDidBecomeActive")
        schedulePendingTraceAutostartIfNeeded()
        scheduleWatchAutostartIfNeeded()
    }

    private func runBenchmarkIfNeeded()
    {
        guard !didRunBenchmark, let benchmark else
        {
            return
        }
        didRunBenchmark = true
        emitConsoleLine("OXIDE_START \(benchmark.testName)")
        emitBenchmarkMetadataLine(
            testName: benchmark.testName,
            measureIterations: benchmark.consoleMeasureIterations,
            benchmarkIterations: benchmark.iterations
        )
        let consoleSamples = runConsoleMeasuredBenchmarkPasses(benchmark)
        if benchmark.emitGenericWorkloadSummary,
           let workloadSummary = summarizeStageSamples(consoleSamples.workloadMs),
           let stageLine = encodeOxideStageSummaryLine(stages: ["workload": workloadSummary])
        {
            emitConsoleLine(stageLine)
        }
        if benchmark.emitGenericResidentMemorySummary,
           let residentSummary = summarizeSamples(
                consoleSamples.residentBytes,
                unit: "bytes"
           ),
           let memoryLine = encodeOxideMemorySummaryLine(
                categories: ["process.rss_bytes": residentSummary]
           )
        {
            emitConsoleLine(memoryLine)
        }
        for line in benchmark.summaryLines()
        {
            emitConsoleLine(line)
        }
        if let failure = takeBenchmarkBuildFailure()
        {
            emitConsoleLine("OXIDE_STAGE parked.fail \(benchmark.testName) \(failure)")
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.1)
            {
                _exit(1)
            }
            return
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
        emitBenchmarkMetadataLine(
            testName: launch.scenario.rawValue,
            measureIterations: 1,
            benchmarkIterations: 1
        )
        window.rootViewController = makeUIKitLaunchRootViewController(
            scenario: launch.scenario,
            route: launch.route,
            style: launch.style
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
