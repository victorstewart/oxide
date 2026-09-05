import UIKit
import os.signpost

enum CoreProbeError: Error {case renderer}

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
   private let injectDelay = CommandLine.arguments.contains("-oxide-core-probe-delay")

   init(window: UIWindow) throws
   {
      adapter = try makeCoreProbeAdapter(window: window)
      super.init()
      try adapter.render(values: values, generation: 0)
   }

   func start()
   {
      startTime = CACurrentMediaTime()
      let link = CADisplayLink(target: self, selector: #selector(tick(_:)))
      link.preferredFrameRateRange = CAFrameRateRange(minimum: 80, maximum: 120, preferred: 120)
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
      generation += 1
      os_signpost(.event, log: log, name: "ProbeMutation", "generation=%{public}llu", generation)
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
      let root = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
      let result: [String: Any] = [
         "schema_version": 1,
         "status": error == nil ? "diagnostic-complete" : "failed",
         "error": error.map {String(describing: $0)} ?? "",
         "injected_delay": injectDelay,
         "updates": generation,
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
