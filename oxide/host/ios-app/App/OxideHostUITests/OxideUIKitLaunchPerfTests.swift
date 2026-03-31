import XCTest

final class OxideUIKitLaunchPerfTests: XCTestCase
{
    private enum LaunchScenario: String
    {
        case simpleHome = "simple_home"
        case heavyHome = "heavy_home"
        case detailRoute = "detail_route"
    }

    override func setUp()
    {
        super.setUp()
        continueAfterFailure = false
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

    private func makeApp(
        scenario: LaunchScenario,
        route: String? = nil
    ) -> XCUIApplication
    {
        let app = XCUIApplication()
        var environment = [
            "OXIDE_PERF_UIKIT_LAUNCH": "1",
            "OXIDE_PERF_LAUNCH_SCENARIO": scenario.rawValue,
            "NSUnbufferedIO": "YES",
        ]
        var arguments = [
            "-oxide-perf-uikit-launch",
            "-oxide-perf-launch-scenario",
            scenario.rawValue,
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

    private func measureColdLaunch(
        scenario: LaunchScenario,
        route: String? = nil
    )
    {
        let app = makeApp(scenario: scenario, route: route)
        let options = XCTMeasureOptions()
        options.iterationCount = 10
        measure(metrics: coldLaunchMetrics(), options: options)
        {
            app.launch()
            waitForLaunchReady(app)
            app.terminate()
        }
    }

    private func measureResume(scenario: LaunchScenario)
    {
        let app = makeApp(scenario: scenario)
        app.launch()
        waitForLaunchReady(app)

        let options = XCTMeasureOptions()
        options.iterationCount = 10
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

    func testHeavyHomeColdLaunch()
    {
        measureColdLaunch(scenario: .heavyHome)
    }

    func testDetailDeepLinkLaunch()
    {
        measureColdLaunch(
            scenario: .detailRoute,
            route: "oxide://detail/integration?item=42"
        )
    }

    func testSimpleHomeWarmResume()
    {
        measureResume(scenario: .simpleHome)
    }

    func testHeavyHomeForegroundAfterBackground()
    {
        measureResume(scenario: .heavyHome)
    }
}
