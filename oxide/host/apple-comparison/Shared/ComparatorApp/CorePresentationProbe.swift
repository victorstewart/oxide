import UIKit
import os.signpost

enum CoreProbeError: Error {case renderer, traceTimeout, hostCancelled}

protocol CoreProbeAdapter: AnyObject
{
   func render(values: [Float], generation: UInt64) throws
}

// One diagnostic workload, not the six-case scorecard. No measurement is inferred
// from callbacks: presentation must be established through the external trace.
final class CorePresentationProbe: NSObject
{
   private let adapter: CoreProbeAdapter
   private let log = OSLog(subsystem: "com.oxide.compare-core", category: .pointsOfInterest)
   private var link: CADisplayLink?
   private var startTime: CFTimeInterval = 0
   private var values = [Float](repeating: 0.5, count: 64)
   private var delayed = false
   private var measured = false
   private var generation: UInt64 = 0
   private var traceStart: CoreTraceStart?
   private let runID: String
   private var finished = false
   private var savedBrightness: CGFloat?
   private let injectDelay = CommandLine.arguments.contains("-oxide-core-probe-delay")
   private let singleUpdate = CommandLine.arguments.contains("-oxide-core-probe-single-update")

   init(window: UIWindow) throws
   {
      let arguments = CommandLine.arguments
      let index = arguments.firstIndex(of: "-oxide-core-run-id")
      runID = index.flatMap {$0 + 1 < arguments.count ? arguments[$0 + 1] : nil} ?? UUID().uuidString
      adapter = try makeCoreProbeAdapter(window: window)
      super.init()
      NotificationCenter.default.addObserver(self, selector: #selector(backgrounded), name: UIApplication.didEnterBackgroundNotification, object: nil)
      try adapter.render(values: values, generation: 0)
   }

   deinit {NotificationCenter.default.removeObserver(self)}

   @objc private func backgrounded()
   {
      finish(error: CoreProbeError.traceTimeout)
   }

   func start()
   {
      UIDevice.current.isBatteryMonitoringEnabled = true
      savedBrightness = UIScreen.main.brightness
      UIScreen.main.brightness = 0.5
      UIApplication.shared.isIdleTimerDisabled = true
      writeResult(status: "waiting", error: "")
      traceStart = CoreTraceStart(runID: runID, action: {self.beginMeasurement()}, failure:
      {
         UIApplication.shared.isIdleTimerDisabled = false
         self.finish(error: CoreProbeError.traceTimeout)
      }, abort: {self.finish(error: CoreProbeError.hostCancelled)})
   }

   private func beginMeasurement()
   {
      guard !finished else {return}
      DispatchQueue.main.asyncAfter(deadline: .now() + 35)
      {
         UIApplication.shared.isIdleTimerDisabled = false
      }
      startTime = CACurrentMediaTime()
      CoreTraceStart.postStarted(runID: runID)
      let link = CADisplayLink(target: self, selector: #selector(tick(_:)))
      link.preferredFrameRateRange = CAFrameRateRange(minimum: 120, maximum: 120, preferred: 120)
      link.add(to: .main, forMode: .common)
      self.link = link
      os_signpost(.event, log: log, name: "ProbeStart")
   }

   @objc private func tick(_ link: CADisplayLink)
   {
      let elapsed = link.targetTimestamp - startTime
      guard elapsed >= 5 else {return}
      if elapsed >= 25
      {
         self.link?.invalidate()
         self.link = nil
         os_signpost(.end, log: log, name: "CoreProbe")
         finish(error: nil)
         return
      }
      if !measured
      {
         measured = true
         os_signpost(.begin, log: log, name: "CoreProbe")
      }
      if singleUpdate && (elapsed < 8 || generation > 0) {return}
      generation += 1
      os_signpost(.event, log: log, name: "ProbeMutation", "generation=%{public}llu scheduled=%{public}.9f source=%{public}.9f", generation, link.targetTimestamp, CACurrentMediaTime())
      if injectDelay && !delayed && elapsed >= 8
      {
         delayed = true
         os_signpost(.begin, log: log, name: "InjectedDelay")
         Thread.sleep(forTimeInterval: 0.1)
         os_signpost(.end, log: log, name: "InjectedDelay")
      }
      let first = Int(generation % 8) * 8
      let value = Float((sin(elapsed * 3) + 1) / 2)
      for index in first..<(first + 8) {values[index] = value}
      do
      {
         try adapter.render(values: values, generation: generation)
         os_signpost(.event, log: log, name: "ProbeUpdateSubmitted", "generation=%{public}llu", generation)
      }
      catch
      {
         self.link?.invalidate()
         self.link = nil
         os_signpost(.end, log: log, name: "CoreProbe")
         finish(error: error)
      }
   }

   private func finish(error: Error?)
   {
      guard !finished else {return}
      finished = true
      link?.invalidate()
      link = nil
      UIApplication.shared.isIdleTimerDisabled = false
      writeResult(status: error == nil ? "diagnostic-complete" : "failed", error: error.map {String(describing: $0)} ?? "")
      if let savedBrightness {UIScreen.main.brightness = savedBrightness}
      traceStart?.cancel()
      CoreTraceStart.postCompleted(runID: runID)
   }

   private func writeResult(status: String, error: String)
   {
      let root = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
      let result: [String: Any] = [
         "schema_version": 1,
         "status": status,
         "error": error,
         "run_id": runID,
         "injected_delay": injectDelay,
         "single_update": singleUpdate,
         "updates": generation,
         "measured_seconds": 20,
         "brightness": UIScreen.main.brightness,
         "battery_state": UIDevice.current.batteryState.rawValue,
         "low_power_mode": ProcessInfo.processInfo.isLowPowerModeEnabled,
         "thermal_state_at_end": ProcessInfo.processInfo.thermalState.rawValue,
         "presentation_validated": false
      ]
      do
      {
         let data = try JSONSerialization.data(withJSONObject: result, options: [.prettyPrinted, .sortedKeys])
         try data.write(to: root.appendingPathComponent("core-probe.json"), options: .atomic)
      }
      catch
      {
         os_signpost(.event, log: log, name: "ProbeArtifactFailure")
      }
   }
}
