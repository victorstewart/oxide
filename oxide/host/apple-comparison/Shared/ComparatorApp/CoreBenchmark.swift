import UIKit
import Darwin
import os.signpost

protocol CoreSuiteAdapter: AnyObject
{
   func render(time: Double, generation: UInt64) throws
   func reset()
   // Returns true only when visible state changed and a draw is required.
   func advance(time: Double, generation: UInt64) throws -> Bool
   // A positive value is the next elapsed-time deadline. Infinity means idle.
   func nextWakeup(after time: Double) -> Double
   func settle()
}

extension CoreSuiteAdapter
{
   func renderControl(time: Double, generation: UInt64) throws
   {
      try render(time: 0, generation: generation)
   }

   func nextControlWakeup(after time: Double) -> Double {.nan}
}

protocol CoreSuiteInstrumentation: AnyObject
{
   func setEnergyInstrumentationEnabled(_ enabled: Bool)
}

// A single, bounded start handshake shared by the probe and the six fixtures.
final class CoreTraceStart
{
   private let name: CFString
   private let cancelName: CFString
   private let runID: String
   private var action: (() -> Void)?
   private var failure: (() -> Void)?
   private var abort: (() -> Void)?
   private var completed = false

   init(runID: String, action: @escaping () -> Void, failure: @escaping () -> Void, abort: @escaping () -> Void)
   {
      self.runID = runID
      name = "com.oxide.compare-core.start.\(runID)" as CFString
      cancelName = "com.oxide.compare-core.cancel.\(runID)" as CFString
      self.action = action
      self.failure = failure
      self.abort = abort
      CFNotificationCenterAddObserver(CFNotificationCenterGetDarwinNotifyCenter(), Unmanaged.passUnretained(self).toOpaque(),
      { _, observer, _, _, _ in
         guard let observer = observer else {return}
         let start = Unmanaged<CoreTraceStart>.fromOpaque(observer).takeUnretainedValue()
         DispatchQueue.main.async
         {
            guard let abort = start.abort else {return}
            abort()
            start.cancel()
         }
      }, cancelName, nil, .deliverImmediately)
      guard CommandLine.arguments.contains("-oxide-core-wait-for-trace") else
      {
         complete(success: true)
         return
      }
      CFNotificationCenterAddObserver(CFNotificationCenterGetDarwinNotifyCenter(), Unmanaged.passUnretained(self).toOpaque(),
      { _, observer, _, _, _ in
         guard let observer = observer else {return}
         let start = Unmanaged<CoreTraceStart>.fromOpaque(observer).takeUnretainedValue()
         DispatchQueue.main.async {start.complete(success: true)}
      }, name, nil, .deliverImmediately)
      Self.post("com.oxide.compare-core.ready.\(runID)")
      print("CORE_READY \(runID)")
      DispatchQueue.main.asyncAfter(deadline: .now() + 150)
      {
         [weak self] in self?.complete(success: false)
      }
   }

   deinit
   {
      removeAllObservers()
   }

   func cancel()
   {
      guard abort != nil else {return}
      action = nil
      failure = nil
      abort = nil
      removeAllObservers()
   }

   static func postStarted(runID: String)
   {
      post("com.oxide.compare-core.started.\(runID)")
      print("CORE_STARTED \(runID)")
   }

   static func postCompleted(runID: String)
   {
      post("com.oxide.compare-core.complete.\(runID)")
      print("CORE_COMPLETE \(runID)")
   }

   private static func post(_ name: String)
   {
      CFNotificationCenterPostNotification(CFNotificationCenterGetDarwinNotifyCenter(), CFNotificationName(name as CFString), nil, nil, true)
   }

   private func complete(success: Bool)
   {
      guard !completed else {return}
      completed = true
      let callback = success ? action : failure
      action = nil
      failure = nil
      if success {removeStartObserver()}
      else
      {
         abort = nil
         removeAllObservers()
      }
      callback?()
   }

   private func removeStartObserver()
   {
      CFNotificationCenterRemoveObserver(CFNotificationCenterGetDarwinNotifyCenter(), Unmanaged.passUnretained(self).toOpaque(), CFNotificationName(name), nil)
   }

   private func removeAllObservers()
   {
      removeStartObserver()
      CFNotificationCenterRemoveObserver(CFNotificationCenterGetDarwinNotifyCenter(), Unmanaged.passUnretained(self).toOpaque(), CFNotificationName(cancelName), nil)
   }
}

private enum CoreBenchmarkMode: String
{
   case legacy
   case diagnostic
   case energy
}

final class CoreBenchmark: NSObject
{
   private let adapter: CoreSuiteAdapter
   private let caseName: String
   private let checkpoint: Double?
   private let runID: String
   private let mode: CoreBenchmarkMode
   private let log = OSLog(subsystem: "com.oxide.compare-core", category: .pointsOfInterest)
   private var traceStart: CoreTraceStart?
   private var link: CADisplayLink?
   private var startTime: Double = 0
   private var measured = false
   private var settled = false
   private var generation: UInt64 = 0
   private var updates = 0
   private var lastStep = -1
   private var skippedUpdates = 0
   private var lastActionSegment = -1
   private var checkpointFrames = 0
   private var savedBrightness: CGFloat = 0
   private var finished = false
   private var refresh120 = 0
   private var refreshOther = 0
   private var phase: String?
   private var phaseStartedAt: Double = 0
   private var phaseBoundaries = [[String: Any]]()
   private var energyPhaseTimer: Timer?
   private var activeWakeTimer: Timer?
   private var energyPowerObserver: NSObjectProtocol?
   private var energyThermalObserver: NSObjectProtocol?
   private var diagnosticMemoryTimer: Timer?
   private var sampledPhysicalMemoryPeak: UInt64 = 0

   init(window: UIWindow, caseName: String, checkpoint: Double?) throws
   {
      if let checkpoint = checkpoint, !checkpoint.isFinite || checkpoint < 0 || checkpoint > 20
      {
         throw NSError(domain: "CoreBenchmark", code: 1, userInfo: [NSLocalizedDescriptionKey: "Checkpoint must be between 0 and 20 seconds"])
      }
      if caseName.hasPrefix("visual-"), checkpoint != nil && ![0.0, 1.0, 2.0].contains(checkpoint!)
      {
         throw NSError(domain: "CoreBenchmark", code: 1, userInfo: [NSLocalizedDescriptionKey: "Visual boards require checkpoint 0, 1, or 2"])
      }
      self.caseName = caseName
      self.checkpoint = checkpoint
      let arguments = CommandLine.arguments
      let index = arguments.firstIndex(of: "-oxide-core-run-id")
      runID = index.flatMap {$0 + 1 < arguments.count ? arguments[$0 + 1] : nil} ?? UUID().uuidString
      let modeIndex = arguments.firstIndex(of: "-oxide-core-mode")
      let modeName = modeIndex.flatMap {$0 + 1 < arguments.count ? arguments[$0 + 1] : nil} ?? "legacy"
      guard let mode = CoreBenchmarkMode(rawValue: modeName) else
      {
         throw NSError(domain: "CoreBenchmark", code: 2, userInfo: [NSLocalizedDescriptionKey: "Unknown core benchmark mode \(modeName)"])
      }
      guard mode == .legacy || checkpoint == nil else
      {
         throw NSError(domain: "CoreBenchmark", code: 4, userInfo: [NSLocalizedDescriptionKey: "Checkpoints use legacy mode"])
      }
      guard mode != .legacy || !caseName.hasPrefix("visual-") || checkpoint != nil else
      {
         throw NSError(domain: "CoreBenchmark", code: 1, userInfo: [NSLocalizedDescriptionKey: "Visual boards require a checkpoint in legacy mode"])
      }
      self.mode = mode
      adapter = try makeCoreSuiteAdapter(window: window, caseName: caseName, checkpoint: checkpoint != nil)
      super.init()
      NotificationCenter.default.addObserver(self, selector: #selector(backgrounded), name: UIApplication.didEnterBackgroundNotification, object: nil)
   }

   deinit {NotificationCenter.default.removeObserver(self)}

   @objc private func backgrounded()
   {
      if checkpoint == nil {finish(error: "app-backgrounded")}
      else {restoreDevice()}
   }

   func start()
   {
      savedBrightness = UIScreen.main.brightness
      UIScreen.main.brightness = 0.5
      UIApplication.shared.isIdleTimerDisabled = true
      UIDevice.current.isBatteryMonitoringEnabled = true
      writeResult(status: "waiting", error: "")
      traceStart = CoreTraceStart(runID: runID, action: {self.begin()}, failure: {self.finish(error: "trace-start-timeout")}, abort: {self.finish(error: "host-cancelled")})
   }

   private func begin()
   {
      guard !finished else {return}
      phaseBoundaries.append(powerBoundary(phase: "admission", event: "begin"))
      if mode == .energy && !energyAdmissionValid()
      {
         finish(error: "energy-requires-known-unplugged-nominal-device-with-low-power-mode-off")
         return
      }
      guard checkpoint != nil || mode == .energy || (ProcessInfo.processInfo.thermalState == .nominal && !ProcessInfo.processInfo.isLowPowerModeEnabled) else
      {
         finish(error: "requires-nominal-thermal-state-and-low-power-mode-off")
         return
      }
      startTime = CACurrentMediaTime()
      CoreTraceStart.postStarted(runID: runID)
      (adapter as? CoreSuiteInstrumentation)?.setEnergyInstrumentationEnabled(mode != .energy)
      observeEnergyPowerChanges()
      startDiagnosticMemorySampling()
      startLink()
   }

   private func startLink()
   {
      guard !finished && link == nil else {return}
      let link = CADisplayLink(target: self, selector: #selector(tick(_:)))
      link.preferredFrameRateRange = CAFrameRateRange(minimum: 120, maximum: 120, preferred: 120)
      link.add(to: .main, forMode: .common)
      self.link = link
   }

   @objc private func tick(_ link: CADisplayLink)
   {
      do
      {
         if let checkpoint = checkpoint
         {
            // Repeated fixed-state submissions allow drawable setup to settle;
            // these are visual diagnostics and never performance samples.
            let visualStage = min(checkpoint, Double(checkpointFrames / 4))
            let renderTime = caseName.hasPrefix("visual-") ? visualStage : checkpoint
            try adapter.render(time: renderTime, generation: UInt64(renderTime * 120))
            checkpointFrames += 1
            if checkpointFrames == 12
            {
               self.link?.invalidate()
               self.link = nil
               adapter.settle()
               writeResult(status: "checkpoint-ready", error: "")
               DispatchQueue.main.asyncAfter(deadline: .now() + 60) {self.restoreDevice()}
            }
            return
         }
         if mode == .energy
         {
            tickEnergy(link)
            return
         }
         try tickTimed(link, warmup: 5, active: mode == .diagnostic ? 60 : 20, drain: 5)
      }
      catch {finish(error: String(describing: error))}
   }

   private func tickTimed(_ link: CADisplayLink, warmup: Double, active: Double, drain: Double) throws
   {
         let elapsed = link.targetTimestamp - startTime
         if elapsed >= warmup + active + drain
         {
            finish(error: "")
            return
         }
         if elapsed >= warmup + active
         {
            if !settled
            {
               settled = true
               os_signpost(.end, log: log, name: "CoreRun", "source=%{public}.9f", CACurrentMediaTime())
               adapter.settle()
               self.link?.invalidate()
               self.link = nil
               DispatchQueue.main.asyncAfter(deadline: .now() + max(0, startTime + warmup + active + drain - CACurrentMediaTime()))
               {
                  self.finish(error: "")
               }
            }
            return
         }
         if elapsed < warmup
         {
            try driveActive(time: elapsed, emitMarkers: false)
            return
         }
         if !measured
         {
            measured = true
            adapter.reset()
            generation = 0
            updates = 0
            skippedUpdates = 0
            refresh120 = 0
            refreshOther = 0
            lastStep = -1
            lastActionSegment = -1
            os_signpost(.begin, log: log, name: "CoreRun", "source=%{public}.9f", CACurrentMediaTime())
         }
         let time = elapsed - warmup
         try driveActive(time: time)
   }

   private func tickEnergy(_ link: CADisplayLink)
   {
      guard let phase else
      {
         beginEnergyPhase("CoreWarmup")
         return
      }
      switch phase
      {
      case "CoreWarmup", "CoreRun":
         do
         {
            let elapsed = CACurrentMediaTime() - phaseStartedAt
            try driveActive(time: elapsed, emitMarkers: false)
            pauseEnergyLinkUntilNextWakeup(after: elapsed)
         }
         catch {finish(error: String(describing: error))}
      case "CoreControlBefore", "CoreControlAfter":
         do
         {
            try driveControl(time: CACurrentMediaTime() - phaseStartedAt)
         }
         catch {finish(error: String(describing: error))}
      default:
         // Control and drain phases intentionally have no display-link work.
         break
      }
   }

   private func beginEnergyPhase(_ next: String)
   {
      guard !finished else {return}
      activeWakeTimer?.invalidate()
      activeWakeTimer = nil
      transition(to: next)
      switch next
      {
      case "CoreWarmup":
         adapter.reset()
         scheduleEnergyPhaseEnd(after: 30) {[weak self] in self?.beginEnergyPhase("CoreControlBefore")}
      case "CoreControlBefore":
         do
         {
            adapter.reset()
            try adapter.renderControl(time: 0, generation: 0)
         }
         catch
         {
            finish(error: String(describing: error))
            return
         }
         if adapter.nextControlWakeup(after: 0).isFinite {startLink()}
         else {link?.invalidate(); link = nil}
         scheduleEnergyPhaseEnd(after: 30) {[weak self] in self?.beginEnergyPhase("CoreRun")}
      case "CoreRun":
         adapter.reset()
         generation = 0
         lastStep = -1
         lastActionSegment = -1
         updates = 0
         skippedUpdates = 0
         refresh120 = 0
         refreshOther = 0
         startLink()
         scheduleEnergyPhaseEnd(after: 120) {[weak self] in self?.beginEnergyPhase("CoreDrain")}
      case "CoreDrain":
         link?.invalidate()
         link = nil
         adapter.settle()
         scheduleEnergyPhaseEnd(after: 10) {[weak self] in self?.beginEnergyPhase("CoreControlAfter")}
      case "CoreControlAfter":
         do
         {
            adapter.reset()
            try adapter.renderControl(time: 0, generation: 0)
         }
         catch
         {
            finish(error: String(describing: error))
            return
         }
         if adapter.nextControlWakeup(after: 0).isFinite {startLink()}
         else {link?.invalidate(); link = nil}
         scheduleEnergyPhaseEnd(after: 30) {[weak self] in self?.finish(error: "")}
      default:
         finish(error: "unknown-energy-phase")
      }
   }

   private func scheduleEnergyPhaseEnd(after duration: Double, action: @escaping () -> Void)
   {
      energyPhaseTimer?.invalidate()
      let remaining = max(0, phaseStartedAt + duration - CACurrentMediaTime())
      let timer = Timer(timeInterval: remaining, repeats: false)
      {
         [weak self] timer in
         guard let self, self.energyPhaseTimer === timer, !self.finished else {return}
         self.energyPhaseTimer = nil
         action()
      }
      // The energy protocol treats the phase boundary as a measurement boundary.
      // Do not allow RunLoop coalescing to extend a hold and shorten the next phase.
      timer.tolerance = 0
      energyPhaseTimer = timer
      RunLoop.main.add(timer, forMode: .common)
   }

   private func pauseEnergyLinkUntilNextWakeup(after elapsed: Double)
   {
      guard mode == .energy else {return}
      let wakeup = adapter.nextWakeup(after: elapsed)
      if wakeup == .infinity
      {
         link?.invalidate()
         link = nil
         activeWakeTimer?.invalidate()
         activeWakeTimer = nil
         return
      }
      guard wakeup.isFinite, wakeup > elapsed + 0.02 else {return}
      link?.invalidate()
      link = nil
      activeWakeTimer?.invalidate()
      let remaining = max(0, wakeup - elapsed)
      let timer = Timer(timeInterval: remaining, repeats: false)
      {
         [weak self] timer in
         guard let self, self.activeWakeTimer === timer, !self.finished else {return}
         self.activeWakeTimer = nil
         self.startLink()
      }
      timer.tolerance = 0
      activeWakeTimer = timer
      RunLoop.main.add(timer, forMode: .common)
   }

   private func driveControl(time: Double) throws
   {
      try adapter.renderControl(time: time, generation: generation)
      let wakeup = adapter.nextControlWakeup(after: time)
      if wakeup == .infinity || wakeup.isNaN
      {
         link?.invalidate()
         link = nil
         activeWakeTimer?.invalidate()
         activeWakeTimer = nil
         return
      }
      guard wakeup.isFinite, wakeup > time + 0.02 else {return}
      link?.invalidate()
      link = nil
      activeWakeTimer?.invalidate()
      let timer = Timer(timeInterval: wakeup - time, repeats: false)
      {
         [weak self] timer in
         guard let self, self.activeWakeTimer === timer, !self.finished else {return}
         self.activeWakeTimer = nil
         self.startLink()
      }
      timer.tolerance = 0
      activeWakeTimer = timer
      RunLoop.main.add(timer, forMode: .common)
   }

   private func driveActive(time: Double, emitMarkers: Bool = true) throws
   {
      if abs((link!.targetTimestamp - link!.timestamp) - 1.0 / 120.0) < 0.001 {refresh120 += 1}
      else {refreshOther += 1}
      let previousGeneration = generation
      if caseName == "text" || caseName == "local"
      {
         let step = Int(floor(max(0, time) * 10))
         guard step > lastStep else {return}
         skippedUpdates += max(0, step - lastStep - 1)
         lastStep = step
         generation = UInt64(step + 1)
      }
      else {generation += 1}
      let changed = try adapter.advance(time: time, generation: generation)
      let continuouslyActive = adapter.nextWakeup(after: time) <= time
      guard changed || continuouslyActive else
      {
         generation = previousGeneration
         return
      }
      updates += 1
      let scheduled = caseName == "text" || caseName == "local"
         ? (mode == .energy ? phaseStartedAt : startTime + 5) + Double(lastStep) / 10
         : CACurrentMediaTime()
      if mode == .diagnostic, measured, caseName.hasPrefix("visual-")
      {
         let segment = Int(floor(max(0, time) / 2))
         if segment != lastActionSegment
         {
            lastActionSegment = segment
            let actionDeadline = startTime + 5 + Double(segment) * 2
            os_signpost(.event, log: log, name: "CoreAction", "generation=%{public}llu scheduled=%{public}.9f source=%{public}.9f", generation, actionDeadline, CACurrentMediaTime())
         }
      }
      if emitMarkers {os_signpost(.event, log: log, name: "CoreMutation", "generation=%{public}llu scheduled=%{public}.9f source=%{public}.9f", generation, scheduled, CACurrentMediaTime())}
      try adapter.render(time: time, generation: generation)
      if emitMarkers {os_signpost(.event, log: log, name: "CoreSubmitted", "generation=%{public}llu", generation)}
   }

   private func transition(to next: String)
   {
      guard phase != next else {return}
      let now = CACurrentMediaTime()
      if let phase
      {
         phaseBoundaries.append(powerBoundary(phase: phase, event: "end", at: now))
         signpostEnd(phase)
      }
      phase = next
      phaseStartedAt = now
      phaseBoundaries.append(powerBoundary(phase: next, event: "begin", at: now))
      signpostBegin(next)
   }

   private func signpostBegin(_ phase: String)
   {
      switch phase
      {
      case "CoreWarmup": os_signpost(.begin, log: log, name: "CoreWarmup", "source=%{public}.9f", CACurrentMediaTime())
      case "CoreControlBefore": os_signpost(.begin, log: log, name: "CoreControlBefore", "source=%{public}.9f", CACurrentMediaTime())
      case "CoreRun": os_signpost(.begin, log: log, name: "CoreRun", "source=%{public}.9f", CACurrentMediaTime())
      case "CoreDrain": os_signpost(.begin, log: log, name: "CoreDrain", "source=%{public}.9f", CACurrentMediaTime())
      case "CoreControlAfter": os_signpost(.begin, log: log, name: "CoreControlAfter", "source=%{public}.9f", CACurrentMediaTime())
      default: break
      }
   }

   private func signpostEnd(_ phase: String)
   {
      switch phase
      {
      case "CoreWarmup": os_signpost(.end, log: log, name: "CoreWarmup", "source=%{public}.9f", CACurrentMediaTime())
      case "CoreControlBefore": os_signpost(.end, log: log, name: "CoreControlBefore", "source=%{public}.9f", CACurrentMediaTime())
      case "CoreRun": os_signpost(.end, log: log, name: "CoreRun", "source=%{public}.9f", CACurrentMediaTime())
      case "CoreDrain": os_signpost(.end, log: log, name: "CoreDrain", "source=%{public}.9f", CACurrentMediaTime())
      case "CoreControlAfter": os_signpost(.end, log: log, name: "CoreControlAfter", "source=%{public}.9f", CACurrentMediaTime())
      default: break
      }
   }

   private func pluggedIn() -> Bool
   {
      UIDevice.current.batteryState == .charging || UIDevice.current.batteryState == .full
   }

   private func energyAdmissionValid() -> Bool
   {
      UIDevice.current.batteryState != .unknown && !pluggedIn()
         && ProcessInfo.processInfo.thermalState == .nominal && !ProcessInfo.processInfo.isLowPowerModeEnabled
   }

   private func observeEnergyPowerChanges()
   {
      guard mode == .energy else {return}
      energyPowerObserver = NotificationCenter.default.addObserver(forName: UIDevice.batteryStateDidChangeNotification, object: nil, queue: .main)
      {
         [weak self] _ in self?.energyPowerChanged()
      }
      energyThermalObserver = NotificationCenter.default.addObserver(forName: .NSProcessInfoPowerStateDidChange, object: nil, queue: .main)
      {
         [weak self] _ in self?.energyPowerChanged()
      }
   }

   private func energyPowerChanged()
   {
      guard !finished else {return}
      phaseBoundaries.append(powerBoundary(phase: phase ?? "admission", event: "power-change"))
      if UIDevice.current.batteryState == .unknown || pluggedIn() || ProcessInfo.processInfo.isLowPowerModeEnabled
      {
         finish(error: "energy-power-state-changed")
      }
   }

   private func stopObservingEnergyPowerChanges()
   {
      if let observer = energyPowerObserver {NotificationCenter.default.removeObserver(observer)}
      if let observer = energyThermalObserver {NotificationCenter.default.removeObserver(observer)}
      energyPowerObserver = nil
      energyThermalObserver = nil
   }

   private func startDiagnosticMemorySampling()
   {
      guard mode == .diagnostic else {return}
      diagnosticMemoryTimer?.invalidate()
      let timer = Timer(timeInterval: 1, repeats: true)
      {
         [weak self] _ in self?.samplePhysicalMemory()
      }
      timer.tolerance = 0.1
      diagnosticMemoryTimer = timer
      RunLoop.main.add(timer, forMode: .common)
      samplePhysicalMemory()
   }

   private func samplePhysicalMemory()
   {
      guard measured, CACurrentMediaTime() < startTime + 65 else {return}
      var info = task_vm_info_data_t()
      var count = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<integer_t>.size)
      let result = withUnsafeMutablePointer(to: &info)
      {
         pointer in pointer.withMemoryRebound(to: integer_t.self, capacity: Int(count))
         {
            task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count)
         }
      }
      guard result == KERN_SUCCESS else {return}
      let bytes = UInt64(info.phys_footprint)
      sampledPhysicalMemoryPeak = max(sampledPhysicalMemoryPeak, bytes)
      os_signpost(.event, log: log, name: "CoreMemory", "bytes=%{public}llu source=%{public}.9f", bytes, CACurrentMediaTime())
   }

   private func powerBoundary(phase: String, event: String, at time: Double = CACurrentMediaTime()) -> [String: Any]
   {
      ["phase": phase, "event": event, "time": time,
       "charging_or_plugged": pluggedIn(), "battery_state": UIDevice.current.batteryState.rawValue,
       "low_power_mode": ProcessInfo.processInfo.isLowPowerModeEnabled,
       "thermal_state": ProcessInfo.processInfo.thermalState.rawValue,
       "brightness": UIScreen.main.brightness]
   }

   private func restoreDevice()
   {
      UIScreen.main.brightness = savedBrightness
      UIApplication.shared.isIdleTimerDisabled = false
   }

   private func finish(error: String)
   {
      guard !finished else {return}
      finished = true
      energyPhaseTimer?.invalidate()
      energyPhaseTimer = nil
      activeWakeTimer?.invalidate()
      activeWakeTimer = nil
      diagnosticMemoryTimer?.invalidate()
      diagnosticMemoryTimer = nil
      link?.invalidate()
      link = nil
      if let phase
      {
         phaseBoundaries.append(powerBoundary(phase: phase, event: "end"))
         signpostEnd(phase)
         self.phase = nil
      }
      else if measured && !settled {os_signpost(.end, log: log, name: "CoreRun", "source=%{public}.9f", CACurrentMediaTime())}
      settled = true
      adapter.settle()
      restoreDevice()
      stopObservingEnergyPowerChanges()
      writeResult(status: error.isEmpty ? (mode == .energy ? "energy-complete" : "diagnostic-complete") : "failed", error: error)
      traceStart?.cancel()
      CoreTraceStart.postCompleted(runID: runID)
   }

   private func writeResult(status: String, error: String)
   {
      let result: [String: Any] = [
         "schema_version": 1, "status": status, "error": error, "case": caseName,
         "run_id": runID, "mode": mode.rawValue,
         "updates": updates, "skipped_updates": skippedUpdates,
         "warmup_seconds": mode == .energy ? 30 : 5,
         "measured_seconds": mode == .energy ? 120 : (mode == .diagnostic ? 60 : 20),
         "settled_seconds": mode == .energy ? 10 : 5,
         "control_before_seconds": mode == .energy ? 30 : 0,
         "control_after_seconds": mode == .energy ? 30 : 0,
         "brightness": 0.5, "thermal_state_at_end": ProcessInfo.processInfo.thermalState.rawValue,
         "battery_state": UIDevice.current.batteryState.rawValue,
         "low_power_mode": ProcessInfo.processInfo.isLowPowerModeEnabled,
         "screen_maximum_fps": UIScreen.main.maximumFramesPerSecond,
         "requested_fps": 120,
         "display_link_scheduled_120hz_ticks": refresh120,
         "display_link_scheduled_other_ticks": refreshOther,
         "phase_boundaries": phaseBoundaries,
         "charging_or_plugged_at_end": pluggedIn(),
         "low_power_mode_at_end": ProcessInfo.processInfo.isLowPowerModeEnabled,
         "checkpoint": checkpoint.map {$0 as Any} ?? NSNull(),
         "sampled_physical_memory_peak_bytes": sampledPhysicalMemoryPeak,
         "physical_memory_sampling": mode == .diagnostic ? "1hz-task-vm-info-phys-footprint-sampled" : "disabled",
         "presentation_validated": false
      ]
      do
      {
         let root = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
         let data = try JSONSerialization.data(withJSONObject: result, options: [.prettyPrinted, .sortedKeys])
         try data.write(to: root.appendingPathComponent("core-result.json"), options: .atomic)
      }
      catch {os_signpost(.event, log: log, name: "CoreArtifactFailure")}
   }
}
