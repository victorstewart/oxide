import QuartzCore
import UIKit
import XCTest
import os.signpost

private let benchmarkBridgeMarkerNotification = Notification.Name("BenchmarkBridgeMarkerNotification")
private let reactPerfSignpostSubsystem = "com.oxide.perf"
private let reactPerfSignpostCategory = OSLog.Category.pointsOfInterest.rawValue
private let reactPerfSignpostLog = OSLog(
  subsystem: reactPerfSignpostSubsystem,
  category: reactPerfSignpostCategory
)
private let reactPerfReadyNotification = "com.oxide.perf.ready"
private let reactPerfStartNotification = "com.oxide.perf.start"
private let reactPerfCompleteNotification = "com.oxide.perf.complete"
private let reactPerfTraceHandshakeEnv = "OXIDE_PERF_TRACE_HANDSHAKE"
private let reactPerfMeasureIterationsEnv = "OXIDE_PERF_MEASURE_ITERATIONS"
private let reactPerfBenchmarkIterationsEnv = "OXIDE_PERF_BENCHMARK_ITERATIONS"
private let reactPerfWorkloadName: StaticString = "PerfWorkload"
private let reactPerfBaselineStepName: StaticString = "baseline.preview.step"
private let reactPerfBaselineLayoutName: StaticString = "baseline.preview.layout"
private let reactPerfBaselineFlushName: StaticString = "baseline.preview.flush"
private let reactPerfBaselineRunloopName: StaticString = "baseline.preview.runloop"
private let reactPerfCaseLabel = "testReactNativeVisionCameraLivePreview"

@MainActor
final class ReactNativeCameraBenchPerfTests: XCTestCase
{
  private var markerObserver: NSObjectProtocol?

  override func tearDown()
  {
    if let markerObserver
    {
      NotificationCenter.default.removeObserver(markerObserver)
      self.markerObserver = nil
    }
    super.tearDown()
  }

  func testReactNativeVisionCameraLivePreview()
  {
    continueAfterFailure = false
    let ready = expectation(description: "camera ready")
    markerObserver = NotificationCenter.default.addObserver(
      forName: benchmarkBridgeMarkerNotification,
      object: nil,
      queue: .main
    ) {
      notification in
      guard let marker = notification.userInfo?["marker"] as? String else
      {
        return
      }
      if marker.contains("OXIDE_READY")
      {
        ready.fulfill()
      }
    }

    wait(for: [ready], timeout: 20.0)
    emitConsoleLine("OXIDE_READY \(reactPerfCaseLabel)")
    postDarwinNotification(reactPerfReadyNotification)

    let startObserver = DarwinNotificationObserver(name: reactPerfStartNotification) {
      [weak self] in
      self?.fulfillStartExpectationIfNeeded()
    }

    if traceHandshakeEnabled()
    {
      self.startExpectation = expectation(description: "benchmark start")
      guard let startExpectation else
      {
        XCTFail("missing benchmark start expectation")
        withExtendedLifetime(startObserver) {}
        return
      }
      wait(for: [startExpectation], timeout: 30.0)
    }
    emitConsoleLine("OXIDE_START \(reactPerfCaseLabel)")

    let options = XCTMeasureOptions()
    options.iterationCount = resolvePerfMeasureIterations(defaultValue: 5)
    measure(
      metrics: [
        XCTClockMetric(),
        XCTCPUMetric(),
        XCTMemoryMetric(),
        XCTStorageMetric(),
      ],
      options: options
    ) {
      runMeasuredBenchmarkPass()
    }

    emitConsoleLine("OXIDE_COMPLETE \(reactPerfCaseLabel)")
    postDarwinNotification(reactPerfCompleteNotification)
    withExtendedLifetime(startObserver) {}
  }

  private var startExpectation: XCTestExpectation?

  private func fulfillStartExpectationIfNeeded()
  {
    startExpectation?.fulfill()
    startExpectation = nil
  }
}

private func withPerfSignpost<T>(_ name: StaticString, body: () -> T) -> T
{
  let signpostID = OSSignpostID(log: reactPerfSignpostLog)
  os_signpost(.begin, log: reactPerfSignpostLog, name: name, signpostID: signpostID)
  let result = body()
  os_signpost(.end, log: reactPerfSignpostLog, name: name, signpostID: signpostID)
  return result
}

private func emitConsoleLine(_ line: String)
{
  guard let data = "\(line)\n".data(using: .utf8) else
  {
    return
  }
  try? FileHandle.standardOutput.write(contentsOf: data)
}

private func postDarwinNotification(_ name: String)
{
  CFNotificationCenterPostNotification(
    CFNotificationCenterGetDarwinNotifyCenter(),
    CFNotificationName(name as CFString),
    nil,
    nil,
    true
  )
}

private func traceHandshakeEnabled(
  environment: [String: String] = ProcessInfo.processInfo.environment
) -> Bool
{
  environment[reactPerfTraceHandshakeEnv].map({ !$0.isEmpty }) == true
}

private func resolvePerfIterationOverride(
  env: String,
  minimum: Int,
  defaultValue: Int,
  environment: [String: String] = ProcessInfo.processInfo.environment
) -> Int
{
  guard let raw = environment[env]?.trimmingCharacters(in: .whitespacesAndNewlines),
        let parsed = Int(raw) else
  {
    return defaultValue
  }
  return max(parsed, minimum)
}

private func resolvePerfMeasureIterations(defaultValue: Int) -> Int
{
  resolvePerfIterationOverride(
    env: reactPerfMeasureIterationsEnv,
    minimum: 3,
    defaultValue: defaultValue
  )
}

private func resolvePerfBenchmarkIterations(defaultValue: Int) -> Int
{
  resolvePerfIterationOverride(
    env: reactPerfBenchmarkIterationsEnv,
    minimum: 12,
    defaultValue: defaultValue
  )
}

@MainActor
private func runMeasuredBenchmarkPass()
{
  let signpostID = OSSignpostID(log: reactPerfSignpostLog)
  os_signpost(.begin, log: reactPerfSignpostLog, name: reactPerfWorkloadName, signpostID: signpostID)
  for _ in 0..<resolvePerfBenchmarkIterations(defaultValue: 24)
  {
    runMeasuredPreviewStep()
  }
  os_signpost(.end, log: reactPerfSignpostLog, name: reactPerfWorkloadName, signpostID: signpostID)
}

@MainActor
private func runMeasuredPreviewStep()
{
  withPerfSignpost(reactPerfBaselineStepName)
  {
    withPerfSignpost(reactPerfBaselineLayoutName)
    {
      preferredBenchmarkWindow()?.layoutIfNeeded()
    }
    withPerfSignpost(reactPerfBaselineFlushName)
    {
      CATransaction.flush()
    }
    withPerfSignpost(reactPerfBaselineRunloopName)
    {
      RunLoop.main.run(until: Date().addingTimeInterval(1.0 / 60.0))
    }
  }
}

@MainActor
private func preferredBenchmarkWindow() -> UIWindow?
{
  let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
  for scene in scenes
  {
    if let window = scene.windows.first(where: { !$0.isHidden && $0.rootViewController != nil })
    {
      return window
    }
  }
  return nil
}

private final class DarwinNotificationObserver
{
  private let name: String
  private let callback: () -> Void

  init(name: String, callback: @escaping () -> Void)
  {
    self.name = name
    self.callback = callback
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
        let token = Unmanaged<DarwinNotificationObserver>
          .fromOpaque(observer)
          .takeUnretainedValue()
        DispatchQueue.main.async
        {
          token.callback()
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
}
