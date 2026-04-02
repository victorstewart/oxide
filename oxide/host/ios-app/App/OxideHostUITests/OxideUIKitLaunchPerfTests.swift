import Darwin
import Foundation
import XCTest

private let oxideBenchmarkMetadataPrefix = "OXIDE_BENCHMARK_METADATA "

private struct OxideBenchmarkMetadataPayload: Codable
{
    let benchmarkIterations: Int
    let measureIterations: Int
    let testName: String
}

private func emitBenchmarkMetadataLine(
    testName: String,
    measureIterations: Int,
    benchmarkIterations: Int
)
{
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys]
    let payload = OxideBenchmarkMetadataPayload(
        benchmarkIterations: benchmarkIterations,
        measureIterations: measureIterations,
        testName: testName
    )
    guard let data = try? encoder.encode(payload),
          let json = String(data: data, encoding: .utf8)
    else
    {
        return
    }
    fputs("\(oxideBenchmarkMetadataPrefix)\(json)\n", stdout)
    fflush(stdout)
}

final class OxideUIKitLaunchPerfTests: XCTestCase
{
    private final class DarwinNotificationCounter
    {
        private let name: String
        private var count: UInt32 = 0

        init(name: String)
        {
            self.name = name
            let rawObserver = UnsafeMutableRawPointer(Unmanaged.passUnretained(self).toOpaque())
            CFNotificationCenterAddObserver(
                CFNotificationCenterGetDarwinNotifyCenter(),
                rawObserver,
                {
                    _, observer, _, _, _ in
                    guard let observer else
                    {
                        return
                    }
                    let token = Unmanaged<DarwinNotificationCounter>
                        .fromOpaque(observer)
                        .takeUnretainedValue()
                    DispatchQueue.main.async
                    {
                        token.count &+= 1
                    }
                },
                name as CFString,
                nil,
                .deliverImmediately
            )
        }

        deinit
        {
            let rawObserver = UnsafeMutableRawPointer(Unmanaged.passUnretained(self).toOpaque())
            CFNotificationCenterRemoveObserver(
                CFNotificationCenterGetDarwinNotifyCenter(),
                rawObserver,
                CFNotificationName(name as CFString),
                nil
            )
        }

        func waitForCount(
            _ expectedCount: UInt32,
            timeout: TimeInterval,
            context: @autoclosure () -> String
        )
        {
            let deadline = Date().addingTimeInterval(timeout)
            while count < expectedCount && Date() < deadline
            {
                RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.05))
            }
            XCTAssertGreaterThanOrEqual(
                count,
                expectedCount,
                "\(name) count=\(count) expected=\(expectedCount)\n\(context())"
            )
        }
    }

    private let perfStartNotificationName = "com.oxide.perf.start"
    private let perfReadyNotificationName = "com.oxide.perf.ready"
    private let perfCompleteNotificationName = "com.oxide.perf.complete"

    private enum LaunchScenario: String
    {
        case simpleHome = "simple_home"
        case heavyHome = "heavy_home"
        case detailRoute = "detail_route"

        func coldLaunchTestName(route: String?) -> String
        {
            switch self
            {
            case .simpleHome:
                return "testSimpleHomeColdLaunch"
            case .heavyHome:
                return "testHeavyHomeColdLaunch"
            case .detailRoute:
                _ = route
                return "testDetailDeepLinkLaunch"
            }
        }

        var resumeTestName: String
        {
            switch self
            {
            case .simpleHome:
                return "testSimpleHomeWarmResume"
            case .heavyHome:
                return "testHeavyHomeForegroundAfterBackground"
            case .detailRoute:
                return "testDetailDeepLinkLaunch"
            }
        }
    }

    private enum LaunchStyle: String
    {
        case idiomatic = "idiomatic"
        case optimized = "optimized"
    }

    override func setUp()
    {
        super.setUp()
        continueAfterFailure = false
    }

    private func resolvePerfMeasureIterations(defaultValue: Int) -> Int
    {
        guard let raw = ProcessInfo.processInfo.environment["OXIDE_PERF_MEASURE_ITERATIONS"],
              let parsed = Int(raw),
              parsed > 0
        else
        {
            return defaultValue
        }
        return parsed
    }

    private func coldLaunchMetrics() -> [XCTMetric]
    {
        [
            XCTApplicationLaunchMetric(),
            XCTClockMetric(),
            XCTCPUMetric(),
            XCTMemoryMetric(),
            XCTStorageMetric(),
        ]
    }

    private func lifecycleMetrics() -> [XCTMetric]
    {
        [
            XCTClockMetric(),
            XCTCPUMetric(),
            XCTMemoryMetric(),
            XCTStorageMetric(),
        ]
    }

    private func cameraMetrics() -> [XCTMetric]
    {
        lifecycleMetrics()
    }

    private func makeApp(
        scenario: LaunchScenario,
        route: String? = nil,
        style: LaunchStyle = .idiomatic
    ) -> XCUIApplication
    {
        let app = XCUIApplication()
        var environment = [
            "OXIDE_PERF_UIKIT_LAUNCH": "1",
            "OXIDE_PERF_LAUNCH_SCENARIO": scenario.rawValue,
            "OXIDE_PERF_LAUNCH_STYLE": style.rawValue,
            "NSUnbufferedIO": "YES",
        ]
        var arguments = [
            "-oxide-perf-uikit-launch",
            "-oxide-perf-launch-scenario",
            scenario.rawValue,
            "-oxide-perf-launch-style",
            style.rawValue,
        ]
        if let route
        {
            environment["OXIDE_PERF_LAUNCH_ROUTE"] = route
            arguments.append("-oxide-perf-launch-route")
            arguments.append(route)
        }
        app.launchEnvironment = environment
        app.launchArguments = arguments
        return app
    }

    private func waitForLaunchReady(_ app: XCUIApplication)
    {
        let launchRoot = app.otherElements["uikitLaunchRoot"]
        XCTAssertTrue(launchRoot.waitForExistence(timeout: 15.0))
        let readyLabel = app.staticTexts["launchReadyLabel"]
        XCTAssertTrue(readyLabel.waitForExistence(timeout: 15.0))
    }

    private func makeRealAppCameraBenchmarkApp(caseName: String) -> XCUIApplication
    {
        let app = XCUIApplication()
        let inherited = ProcessInfo.processInfo.environment
        var environment = [
            "OXIDE_PERF_CASE": caseName,
            "OXIDE_PERF_CAMERA_REAL_APP_HOST": "1",
            "OXIDE_RENDER_IN_TEST": "1",
            "NSUnbufferedIO": "YES",
            "UITEST": "1",
        ]
        var arguments = [
            "UITEST",
            "-oxide-perf-case", caseName,
            "-oxide-perf-camera-real-app-host", "1",
            "-oxide-render-in-test", "1",
        ]
        for (key, value) in inherited
        {
            if key == "OXIDE_RENDER_IN_TEST" || key.hasPrefix("OXIDE_PERF_")
            {
                environment[key] = value
            }
        }
        app.launchEnvironment = environment
        app.launchArguments = arguments
        return app
    }

    private func postPerfStartNotification()
    {
        CFNotificationCenterPostNotification(
            CFNotificationCenterGetDarwinNotifyCenter(),
            CFNotificationName(perfStartNotificationName as CFString),
            nil,
            nil,
            true
        )
    }

    private func measureRealAppCamera(caseName: String)
    {
        let app = makeRealAppCameraBenchmarkApp(caseName: caseName)
        let readyCounter = DarwinNotificationCounter(name: perfReadyNotificationName)
        let completeCounter = DarwinNotificationCounter(name: perfCompleteNotificationName)
        app.launch()
        readyCounter.waitForCount(1, timeout: 20.0, context: app.debugDescription)

        let options = XCTMeasureOptions()
        options.iterationCount = resolvePerfMeasureIterations(defaultValue: 5)
        emitBenchmarkMetadataLine(
            testName: caseName,
            measureIterations: options.iterationCount,
            benchmarkIterations: 1
        )
        var completedRuns: UInt32 = 0
        measure(metrics: cameraMetrics(), options: options)
        {
            completedRuns &+= 1
            postPerfStartNotification()
            completeCounter.waitForCount(completedRuns, timeout: 20.0, context: app.debugDescription)
            RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.25))
        }

        app.terminate()
    }

    private func measureColdLaunch(
        scenario: LaunchScenario,
        route: String? = nil,
        style: LaunchStyle = .idiomatic
    )
    {
        let app = makeApp(scenario: scenario, route: route, style: style)
        let options = XCTMeasureOptions()
        options.iterationCount = 10
        emitBenchmarkMetadataLine(
            testName: style == .optimized
                ? "testOptimized\(scenario.coldLaunchTestName(route: route).dropFirst(4))"
                : scenario.coldLaunchTestName(route: route),
            measureIterations: options.iterationCount,
            benchmarkIterations: 1
        )
        measure(metrics: coldLaunchMetrics(), options: options)
        {
            app.launch()
            waitForLaunchReady(app)
            app.terminate()
        }
    }

    private func measureResume(
        scenario: LaunchScenario,
        style: LaunchStyle = .idiomatic
    )
    {
        let app = makeApp(scenario: scenario, style: style)
        app.launch()
        waitForLaunchReady(app)

        let options = XCTMeasureOptions()
        options.iterationCount = 10
        emitBenchmarkMetadataLine(
            testName: style == .optimized
                ? "testOptimized\(scenario.resumeTestName.dropFirst(4))"
                : scenario.resumeTestName,
            measureIterations: options.iterationCount,
            benchmarkIterations: 1
        )
        measure(metrics: lifecycleMetrics(), options: options)
        {
            XCUIDevice.shared.press(.home)
            app.activate()
            waitForLaunchReady(app)
        }

        app.terminate()
    }

    func testSimpleHomeColdLaunch()
    {
        measureColdLaunch(scenario: .simpleHome)
    }

    func testOptimizedSimpleHomeColdLaunch()
    {
        measureColdLaunch(scenario: .simpleHome, style: .optimized)
    }

    func testHeavyHomeColdLaunch()
    {
        measureColdLaunch(scenario: .heavyHome)
    }

    func testOptimizedHeavyHomeColdLaunch()
    {
        measureColdLaunch(scenario: .heavyHome, style: .optimized)
    }

    func testDetailDeepLinkLaunch()
    {
        measureColdLaunch(
            scenario: .detailRoute,
            route: "oxide://detail/integration?item=42"
        )
    }

    func testOptimizedDetailDeepLinkLaunch()
    {
        measureColdLaunch(
            scenario: .detailRoute,
            route: "oxide://detail/integration?item=42",
            style: .optimized
        )
    }

    func testSimpleHomeWarmResume()
    {
        measureResume(scenario: .simpleHome)
    }

    func testOptimizedSimpleHomeWarmResume()
    {
        measureResume(scenario: .simpleHome, style: .optimized)
    }

    func testHeavyHomeForegroundAfterBackground()
    {
        measureResume(scenario: .heavyHome)
    }

    func testOptimizedHeavyHomeForegroundAfterBackground()
    {
        measureResume(scenario: .heavyHome, style: .optimized)
    }

    func testCameraNV12LegacyRealAppLivePreview()
    {
        measureRealAppCamera(caseName: #function)
    }

    func testCameraAVFoundationPreviewLayerRealAppLivePreview()
    {
        measureRealAppCamera(caseName: #function)
    }
}
