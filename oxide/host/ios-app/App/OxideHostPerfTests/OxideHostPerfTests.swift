import UIKit
import XCTest

private let perfTestSignpostSubsystem = "com.oxide.perf"
private let perfTestSignpostCategory = "pointsOfInterest"

@MainActor
final class OxideHostPerfTests: XCTestCase
{
    private var host: PerfSurfaceHost!
    private var window: UIWindow!

    override func setUp()
    {
        super.setUp()
        continueAfterFailure = false
        let host = PerfSurfaceHost()
        let window = host.installInNewWindow(makeKey: true)
        self.host = host
        self.window = window
        host.reset()
    }

    override func tearDown()
    {
        host?.setVisibleTestLabel(nil)
        host?.reset()
        window?.isHidden = true
        window?.rootViewController = nil
        window = nil
        host = nil
        super.tearDown()
    }

    private func standardMetrics() -> [XCTMetric]
    {
        // Xcode 26.3 is crashing in XCTOSSignpostMetric teardown for this
        // app-hosted simulator harness. Keep the stable XCTest core metrics
        // here; the shared signpost phases are still captured by the device
        // xctrace path.
        [
            XCTClockMetric(),
            XCTCPUMetric(),
            XCTMemoryMetric(),
            XCTStorageMetric(),
        ]
    }

    private func deviceSignpostMetrics(names: [String]) -> [XCTMetric]
    {
        guard Self.requiresPhysicalDeviceTraceSettle() else
        {
            return []
        }
        var seen = Set<String>()
        return names.compactMap
        {
            name in
            guard seen.insert(name).inserted else
            {
                return nil
            }
            return XCTOSSignpostMetric(
                subsystem: perfTestSignpostSubsystem,
                category: perfTestSignpostCategory,
                name: name
            )
        }
    }

    private static func requiresPhysicalDeviceTraceSettle() -> Bool
    {
        ProcessInfo.processInfo.environment["RUN_DESTINATION_DEVICE_PLATFORM_IDENTIFIER"]?
            .contains("iphoneos") == true
    }

    private func settleForDeviceTraceAttachment(_ benchmark: OxideUIKitBenchmark)
    {
        guard Self.requiresPhysicalDeviceTraceSettle() else
        {
            return
        }
        guard !benchmark.signpostNames.isEmpty else
        {
            return
        }
        let settleSeconds = resolvePerfTraceSettleSeconds()
        guard settleSeconds > 0 else
        {
            return
        }
        RunLoop.main.run(until: Date().addingTimeInterval(settleSeconds))
    }

    func testCameraStageMeasurementEnabledUsesParkedCaseEnv()
    {
        XCTAssertFalse(cameraStageMeasurementEnabled(environment: [:]))
        XCTAssertFalse(cameraStageMeasurementEnabled(environment: [parkedCaseEnv: ""]))
        XCTAssertTrue(
            cameraStageMeasurementEnabled(
                environment: [parkedCaseEnv: "testCameraNV12LegacyLivePreview"]
            )
        )
        XCTAssertFalse(
            cameraStageMeasurementEnabled(
                environment: [
                    parkedCaseEnv: "testCameraNV12LegacyLivePreview",
                    perfCameraStageMeasurementEnv: "0",
                ]
            )
        )
    }

    func testConfigureDirectPreviewMetalLayerUsesLeanFullscreenSettings()
    {
        let view = UIView(frame: .zero)
        let layer = CAMetalLayer()
        view.isOpaque = false
        layer.isOpaque = false
        layer.framebufferOnly = false
        if #available(iOS 11.2, *)
        {
            layer.maximumDrawableCount = 3
        }

        configureDirectPreviewMetalLayer(view: view, layer: layer, environment: [:])

        XCTAssertTrue(view.isOpaque)
        XCTAssertTrue(layer.isOpaque)
        XCTAssertTrue(layer.framebufferOnly)
        if #available(iOS 11.2, *)
        {
            XCTAssertEqual(layer.maximumDrawableCount, 3)
        }
    }

    func testResolveDirectPreviewMaximumDrawableCountClampsToBenchmarkRange()
    {
        XCTAssertEqual(resolveDirectPreviewMaximumDrawableCount(environment: [:]), 3)
        XCTAssertEqual(
            resolveDirectPreviewMaximumDrawableCount(
                environment: [perfCameraMaxDrawableCountEnv: "2"]
            ),
            2
        )
        XCTAssertEqual(
            resolveDirectPreviewMaximumDrawableCount(
                environment: [perfCameraMaxDrawableCountEnv: "99"]
            ),
            3
        )
        XCTAssertEqual(
            resolveDirectPreviewMaximumDrawableCount(
                environment: [perfCameraMaxDrawableCountEnv: "junk"]
            ),
            3
        )
    }

    func testResolveDirectPreviewSurfaceScaleClampsToBenchmarkRange()
    {
        XCTAssertEqual(resolveDirectPreviewSurfaceScale(environment: [:]), 1.0)
        XCTAssertEqual(
            resolveDirectPreviewSurfaceScale(
                environment: [perfCameraPreviewSurfaceScaleEnv: "0.5"]
            ),
            0.5,
            accuracy: 0.000_001
        )
        XCTAssertEqual(
            resolveDirectPreviewSurfaceScale(
                environment: [perfCameraPreviewSurfaceScaleEnv: "0.01"]
            ),
            0.25,
            accuracy: 0.000_001
        )
        XCTAssertEqual(
            resolveDirectPreviewSurfaceScale(
                environment: [perfCameraPreviewSurfaceScaleEnv: "junk"]
            ),
            1.0
        )
    }

    func testVisibleOutputValidationRejectsUniformBlackImage()
    {
        let renderer = UIGraphicsImageRenderer(size: CGSize(width: 32, height: 32))
        let image = renderer.image
        {
            context in
            UIColor.black.setFill()
            context.cgContext.fill(CGRect(x: 0, y: 0, width: 32, height: 32))
        }
        let signature = visibleOutputSignature(from: image)
        XCTAssertNotNil(signature)
        XCTAssertFalse(visibleOutputLooksMeaningful(signature!))
    }

    func testVisibleOutputValidationAcceptsRenderedCheckerImage()
    {
        let renderer = UIGraphicsImageRenderer(size: CGSize(width: 32, height: 32))
        let image = renderer.image
        {
            context in
            UIColor.white.setFill()
            context.cgContext.fill(CGRect(x: 0, y: 0, width: 32, height: 32))
            UIColor.systemBlue.setFill()
            context.cgContext.fill(CGRect(x: 0, y: 0, width: 16, height: 16))
            context.cgContext.fill(CGRect(x: 16, y: 16, width: 16, height: 16))
            UIColor.systemOrange.setFill()
            context.cgContext.fill(CGRect(x: 16, y: 0, width: 16, height: 16))
            context.cgContext.fill(CGRect(x: 0, y: 16, width: 16, height: 16))
        }
        let signature = visibleOutputSignature(from: image)
        XCTAssertNotNil(signature)
        XCTAssertTrue(visibleOutputLooksMeaningful(signature!))
    }

    func testPerfDisplayLabelTextClassifiesOxideUIKitAndOptimizedCases()
    {
        XCTAssertEqual(
            perfDisplayLabelText(forBenchmarkNamed: "testOxideButtonPressResponse"),
            "OXIDE  testOxideButtonPressResponse"
        )
        XCTAssertEqual(
            perfDisplayLabelText(forBenchmarkNamed: "testButtonPressResponse"),
            "UIKIT  testButtonPressResponse"
        )
        XCTAssertEqual(
            perfDisplayLabelText(forBenchmarkNamed: "testOptimizedButtonPressResponse"),
            "UIKIT OPT  testOptimizedButtonPressResponse"
        )
    }

    func testResolvePerfDisplayLabelPrefersExplicitEnvironmentLabel()
    {
        let label = resolvePerfDisplayLabel(
            environment: [
                perfDisplayLabelEnv: "UIKIT  testButtonPressResponse",
                parkedCaseEnv: "testCameraNV12LegacyLivePreview",
            ]
        )
        XCTAssertEqual(label, "UIKIT  testButtonPressResponse")
    }

    func testResolveCameraCaptureContractModeParsesBenchmarkModes()
    {
        XCTAssertEqual(resolveCameraCaptureContractMode(environment: [:]), .inputPriority)
        XCTAssertEqual(
            resolveCameraCaptureContractMode(
                environment: [perfCameraCaptureContractModeEnv: "preset-720p"]
            ),
            .preset720p
        )
        XCTAssertEqual(
            resolveCameraCaptureContractMode(
                environment: [perfCameraCaptureContractModeEnv: "hd1280x720"]
            ),
            .preset720p
        )
    }

    func testRealAppCameraBenchmarkEnvParsesExpectedFlags()
    {
        XCTAssertFalse(realAppCameraBenchmarkEnabled(environment: [:]))
        XCTAssertTrue(
            realAppCameraBenchmarkEnabled(
                environment: [perfCameraRealAppHostEnv: "1"]
            )
        )
        XCTAssertFalse(realAppCameraBenchmarkUsesHybridVisiblePreview(environment: [:]))
        XCTAssertTrue(
            realAppCameraBenchmarkUsesHybridVisiblePreview(
                environment: [perfCameraRealAppHybridVisiblePreviewEnv: "1"]
            )
        )
        XCTAssertFalse(realAppCameraFrameDrivenSchedulingEnabled(environment: [:]))
        XCTAssertTrue(
            realAppCameraFrameDrivenSchedulingEnabled(
                environment: [
                    perfCameraRealAppHostEnv: "1",
                    perfCameraFrameDrivenSchedulingEnv: "1",
                ]
            )
        )
        XCTAssertFalse(
            realAppCameraFrameDrivenSchedulingEnabled(
                environment: [
                    perfCameraRealAppHostEnv: "1",
                    perfCameraFrameDrivenSchedulingEnv: "1",
                    perfCameraRealAppHybridVisiblePreviewEnv: "1",
                ]
            )
        )
    }

    func testConfigureDirectPreviewMetalLayerHonorsDrawableCountOverride()
    {
        let view = UIView(frame: .zero)
        let layer = CAMetalLayer()
        configureDirectPreviewMetalLayer(
            view: view,
            layer: layer,
            environment: [perfCameraMaxDrawableCountEnv: "2"]
        )
        if #available(iOS 11.2, *)
        {
            XCTAssertEqual(layer.maximumDrawableCount, 2)
        }
    }

    func testResolveCameraBenchmarkOpportunityIntervalUsesNativeRate()
    {
        XCTAssertEqual(
            resolveCameraBenchmarkOpportunityIntervalSeconds(
                maximumFramesPerSecond: 120,
                environment: [perfRefreshModeEnv: "native"]
            ),
            1.0 / 120.0,
            accuracy: 0.000_001
        )
        XCTAssertEqual(
            resolveCameraBenchmarkOpportunityIntervalSeconds(
                maximumFramesPerSecond: 120,
                environment: [:]
            ),
            1.0 / 120.0,
            accuracy: 0.000_001
        )
        XCTAssertEqual(
            resolveCameraBenchmarkOpportunityIntervalSeconds(
                maximumFramesPerSecond: nil,
                environment: [:]
            ),
            1.0 / 60.0,
            accuracy: 0.000_001
        )
    }

    func testResolveCameraBenchmarkOpportunityCountUsesNativeRate()
    {
        XCTAssertEqual(
            resolveCameraBenchmarkOpportunityCount(
                maximumFramesPerSecond: 120,
                environment: [perfRefreshModeEnv: "native"]
            ),
            120
        )
        XCTAssertEqual(
            resolveCameraBenchmarkOpportunityCount(
                maximumFramesPerSecond: 120,
                environment: [:]
            ),
            120
        )
        XCTAssertEqual(
            resolveCameraBenchmarkOpportunityCount(
                maximumFramesPerSecond: nil,
                environment: [:]
            ),
            60
        )
    }

    func testRunPacedFrameOpportunityWindowExecutesRequestedOpportunities()
    {
        var steps = 0
        let startedAt = CACurrentMediaTime()

        runPacedFrameOpportunityWindow(opportunities: 3, opportunityIntervalSeconds: 0.02)
        {
            steps += 1
        }

        let elapsedSeconds = CACurrentMediaTime() - startedAt
        XCTAssertEqual(steps, 3)
        XCTAssertGreaterThanOrEqual(elapsedSeconds, 0.05)
    }

    private func measureBenchmark(_ benchmark: OxideUIKitBenchmark)
    {
        defer
        {
            benchmark.tearDown()
        }
        host?.setVisibleTestLabel(perfDisplayLabelText(forBenchmarkNamed: benchmark.testName))
        let previousIdleTimerState = UIApplication.shared.isIdleTimerDisabled
        UIApplication.shared.isIdleTimerDisabled = true
        defer
        {
            UIApplication.shared.isIdleTimerDisabled = previousIdleTimerState
        }
        let options = XCTMeasureOptions()
        let isCameraBenchmark = benchmark.testName.starts(with: "testCamera")
        let defaultMeasureIterations = isCameraBenchmark ? 5 : 10
        options.iterationCount = resolveAdaptivePerfMeasureIterations(
            testName: benchmark.testName,
            benchmarkIterations: benchmark.iterations,
            defaultValue: defaultMeasureIterations
        )
        if !isCameraBenchmark
        {
            settleForDeviceTraceAttachment(benchmark)
        }
        emitBenchmarkMetadataLine(
            testName: benchmark.testName,
            measureIterations: options.iterationCount,
            benchmarkIterations: benchmark.iterations
        )
        // Camera signpost metrics are collected from the dedicated xctrace path.
        // Keeping them out of XCTest avoids per-iteration metric-set drift on device.
        let metrics = standardMetrics()
            + (isCameraBenchmark ? [] : deviceSignpostMetrics(names: benchmark.signpostNames))
        measure(metrics: metrics, options: options)
        {
            runMeasuredBenchmarkPass(benchmark)
        }
        for line in benchmark.summaryLines()
        {
            emitConsoleLine(line)
        }
    }

    private func measureBenchmark(named testName: String)
    {
        _ = takeBenchmarkBuildFailure()
        guard let host else
        {
            XCTFail("missing host for benchmark \(testName)")
            return
        }
        guard let benchmark = OxideUIKitBenchmarkCatalog.makeBenchmark(named: testName, host: host) else
        {
            XCTFail(
                takeBenchmarkBuildFailure()
                    ?? "missing benchmark definition for \(testName)"
            )
            return
        }
        measureBenchmark(
            withVisibleOutputValidation(
                withWatchFrameCapture(benchmark, host: host),
                host: host
            )
        )
        if let failure = takeBenchmarkBuildFailure()
        {
            XCTFail(failure)
        }
    }

    func testMeasureBenchmarkEmitsSummaryLinesAfterMeasuredPass()
    {
        var lines: [String] = []
        let previousEmitter = perfConsoleLineEmitterOverride
        perfConsoleLineEmitterOverride =
        {
            lines.append($0)
        }
        defer
        {
            perfConsoleLineEmitterOverride = previousEmitter
        }

        let benchmark = OxideUIKitBenchmark(
            testName: "testCameraSyntheticSummaryEmission",
            iterations: 1,
            summaryLines:
            {
                [
                    "OXIDE_STAGE_SUMMARY {\"stages\":{}}",
                    "OXIDE_MEMORY_SUMMARY {\"categories\":{}}",
                ]
            }
        )
        {
        }

        measureBenchmark(benchmark)

        XCTAssertTrue(lines.contains(where: { $0.contains(oxideBenchmarkMetadataPrefix) }))
        XCTAssertTrue(lines.contains("OXIDE_STAGE_SUMMARY {\"stages\":{}}"))
        XCTAssertTrue(lines.contains("OXIDE_MEMORY_SUMMARY {\"categories\":{}}"))
    }

    func testLabelEncode()
    {
        measureBenchmark(named: #function)
    }

    func testProgressBarEncode()
    {
        measureBenchmark(named: #function)
    }

    func testSpinnerEncode()
    {
        measureBenchmark(named: #function)
    }

    func testButtonEncode()
    {
        measureBenchmark(named: #function)
    }

    func testToggleEncode()
    {
        measureBenchmark(named: #function)
    }

    func testSliderEncode()
    {
        measureBenchmark(named: #function)
    }

    func testImageViewEncode()
    {
        measureBenchmark(named: #function)
    }

    func testNineSliceImageEncode()
    {
        measureBenchmark(named: #function)
    }

    func testCameraNV12OptimizedPreview()
    {
        measureBenchmark(named: #function)
    }

    func testCameraNV12LegacyPreview()
    {
        measureBenchmark(named: #function)
    }

    func testCameraBGRAPreview()
    {
        measureBenchmark(named: #function)
    }

    func testCameraBGRALivePreview()
    {
        measureBenchmark(named: #function)
    }

    func testCameraNV12OptimizedLivePreview()
    {
        measureBenchmark(named: #function)
    }

    func testCameraNV12LegacyLivePreview()
    {
        measureBenchmark(named: #function)
    }

    func testCameraNV12LegacyHybridPreviewLayerLivePreview()
    {
        measureBenchmark(named: #function)
    }

    func testCameraNV12LegacyRealAppLivePreview()
    {
        measureBenchmark(named: #function)
    }

    func testCameraNV12LegacyRealAppHybridPreviewLayerLivePreview()
    {
        measureBenchmark(named: #function)
    }

    func testCameraAVFoundationPreviewLayerLivePreview()
    {
        measureBenchmark(named: #function)
    }

    func testCameraAVFoundationPreviewLayerSidecarLivePreview()
    {
        measureBenchmark(named: #function)
    }

    func testCollectionViewEncode()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedCollectionViewEncode()
    {
        measureBenchmark(named: #function)
    }

    func testLayoutFlatGridRelayout()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedLayoutFlatGridRelayout()
    {
        measureBenchmark(named: #function)
    }

    func testLayoutDeepStackThemeSwap()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedLayoutDeepStackThemeSwap()
    {
        measureBenchmark(named: #function)
    }

    func testLayoutGridSafeAreaSwap()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedLayoutGridSafeAreaSwap()
    {
        measureBenchmark(named: #function)
    }

    func testLargeEditorKeystrokeBurst()
    {
        measureBenchmark(named: #function)
    }

    func testLargeEditorPaste10KB()
    {
        measureBenchmark(named: #function)
    }

    func testLargeEditorSelectionReplace()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedLargeEditorKeystrokeBurst()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedLargeEditorPaste10KB()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedLargeEditorSelectionReplace()
    {
        measureBenchmark(named: #function)
    }

    func testImagePNGDecode()
    {
        measureBenchmark(named: #function)
    }

    func testImageTextureUpload()
    {
        measureBenchmark(named: #function)
    }

    func testImageFirstVisible()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedImagePNGDecode()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedImageTextureUpload()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedImageFirstVisible()
    {
        measureBenchmark(named: #function)
    }

    func testButtonPressResponse()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedButtonPressResponse()
    {
        measureBenchmark(named: #function)
    }

    func testSliderScrubResponse()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedSliderScrubResponse()
    {
        measureBenchmark(named: #function)
    }

    func testTextFocusResponse()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedTextFocusResponse()
    {
        measureBenchmark(named: #function)
    }

    func testSingleNodeReconcile()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedSingleNodeReconcile()
    {
        measureBenchmark(named: #function)
    }

    func testTreeMutation1Pct()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedTreeMutation1Pct()
    {
        measureBenchmark(named: #function)
    }

    func testTreeMutation10Pct()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedTreeMutation10Pct()
    {
        measureBenchmark(named: #function)
    }

    func testThemeSwapFull()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedThemeSwapFull()
    {
        measureBenchmark(named: #function)
    }

    func testEmptyRootMount()
    {
        measureBenchmark(named: #function)
    }

    func testFlatRects10Mount()
    {
        measureBenchmark(named: #function)
    }

    func testFlatRects100Mount()
    {
        measureBenchmark(named: #function)
    }

    func testFlatRects1000Mount()
    {
        measureBenchmark(named: #function)
    }

    func testFlatRects10Mutate()
    {
        measureBenchmark(named: #function)
    }

    func testFlatRects100Mutate()
    {
        measureBenchmark(named: #function)
    }

    func testFlatRects1000Mutate()
    {
        measureBenchmark(named: #function)
    }

    func testFlatRects100RemoveAll()
    {
        measureBenchmark(named: #function)
    }

    func testFlatRects100Remount()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedFlatRects10Mount()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedFlatRects100Mount()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedFlatRects1000Mount()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedFlatRects10Mutate()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedFlatRects100Mutate()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedFlatRects1000Mutate()
    {
        measureBenchmark(named: #function)
    }

    func testLabels10Mount()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedLabels10Mount()
    {
        measureBenchmark(named: #function)
    }

    func testLabels100Mount()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedLabels100Mount()
    {
        measureBenchmark(named: #function)
    }

    func testLabels1000Mount()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedLabels1000Mount()
    {
        measureBenchmark(named: #function)
    }

    func testLabels10Mutate()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedLabels10Mutate()
    {
        measureBenchmark(named: #function)
    }

    func testLabels100Mutate()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedLabels100Mutate()
    {
        measureBenchmark(named: #function)
    }

    func testLabels1000Mutate()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedLabels1000Mutate()
    {
        measureBenchmark(named: #function)
    }

    func testCards10Mount()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedCards10Mount()
    {
        measureBenchmark(named: #function)
    }

    func testCards100Mount()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedCards100Mount()
    {
        measureBenchmark(named: #function)
    }

    func testCards10Mutate()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedCards10Mutate()
    {
        measureBenchmark(named: #function)
    }

    func testCards100Mutate()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedCards100Mutate()
    {
        measureBenchmark(named: #function)
    }

    func testImages10Mount()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedImages10Mount()
    {
        measureBenchmark(named: #function)
    }

    func testImages100Mount()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedImages100Mount()
    {
        measureBenchmark(named: #function)
    }

    func testImages10Mutate()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedImages10Mutate()
    {
        measureBenchmark(named: #function)
    }

    func testImages100Mutate()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedImages100Mutate()
    {
        measureBenchmark(named: #function)
    }

    func testControlSetMount()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedControlSetMount()
    {
        measureBenchmark(named: #function)
    }

    func testControlSetMutate()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedControlSetMutate()
    {
        measureBenchmark(named: #function)
    }

    func testSpinnerSpin()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedSpinnerSpin()
    {
        measureBenchmark(named: #function)
    }

    func testProgressIndeterminate()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedProgressIndeterminate()
    {
        measureBenchmark(named: #function)
    }

    func testButtonPressScale()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedButtonPressScale()
    {
        measureBenchmark(named: #function)
    }

    func testToggleThumbSpring()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedToggleThumbSpring()
    {
        measureBenchmark(named: #function)
    }

    func testSliderThumbMove()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedSliderThumbMove()
    {
        measureBenchmark(named: #function)
    }

    func testImageZoomPan()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedImageZoomPan()
    {
        measureBenchmark(named: #function)
    }

    func testAnimTimelineBars()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedAnimTimelineBars()
    {
        measureBenchmark(named: #function)
    }

    func testInputFormJourney()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedInputFormJourney()
    {
        measureBenchmark(named: #function)
    }

    func testCollectionNavigationJourney()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedCollectionNavigationJourney()
    {
        measureBenchmark(named: #function)
    }

    func testFeedScrollJourney()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedFeedScrollJourney()
    {
        measureBenchmark(named: #function)
    }

    func testThumbnailGridScrollJourney()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedThumbnailGridScrollJourney()
    {
        measureBenchmark(named: #function)
    }

    func testChatThreadScrollJourney()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedChatThreadScrollJourney()
    {
        measureBenchmark(named: #function)
    }

    func testZoomImageGestureJourney()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedZoomImageGestureJourney()
    {
        measureBenchmark(named: #function)
    }

    func testOrchestrationJourney()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedOrchestrationJourney()
    {
        measureBenchmark(named: #function)
    }

    func testTextFieldsEditCycle()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedTextFieldsEditCycle()
    {
        measureBenchmark(named: #function)
    }

    func testPopupWheelPickerInteraction()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedPopupWheelPickerInteraction()
    {
        measureBenchmark(named: #function)
    }

    func testBurstEmitterSample()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedBurstEmitterSample()
    {
        measureBenchmark(named: #function)
    }

    func testSurfaceRouterCompose()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedSurfaceRouterCompose()
    {
        measureBenchmark(named: #function)
    }

    func testOpenCloseHeavyScreen100x()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedOpenCloseHeavyScreen100x()
    {
        measureBenchmark(named: #function)
    }

    func testTabSwitchHeavy500x()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedTabSwitchHeavy500x()
    {
        measureBenchmark(named: #function)
    }

    func testIdleAnimation600Frames()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedIdleAnimation600Frames()
    {
        measureBenchmark(named: #function)
    }

    func testFlatRects10000Mount()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedFlatRects10000Mount()
    {
        measureBenchmark(named: #function)
    }

    func testStress300Animations()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedStress300Animations()
    {
        measureBenchmark(named: #function)
    }

    func testTicker100Hz()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedTicker100Hz()
    {
        measureBenchmark(named: #function)
    }

    func testPermissionCallbackBridge()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedPermissionCallbackBridge()
    {
        measureBenchmark(named: #function)
    }

    func testSensorLocationBridge()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedSensorLocationBridge()
    {
        measureBenchmark(named: #function)
    }

    func testBluetoothCacheBridge()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedBluetoothCacheBridge()
    {
        measureBenchmark(named: #function)
    }

    func testPhotoImportThumbnailBridge()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedPhotoImportThumbnailBridge()
    {
        measureBenchmark(named: #function)
    }

    func testFileImportRenderBridge()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedFileImportRenderBridge()
    {
        measureBenchmark(named: #function)
    }

    func testSharePayloadPrepareBridge()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedSharePayloadPrepareBridge()
    {
        measureBenchmark(named: #function)
    }

    func testLocalJSONTransportRenderBridge()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedLocalJSONTransportRenderBridge()
    {
        measureBenchmark(named: #function)
    }

    func testLocalImageTransportRenderBridge()
    {
        measureBenchmark(named: #function)
    }

    func testOptimizedLocalImageTransportRenderBridge()
    {
        measureBenchmark(named: #function)
    }
}
