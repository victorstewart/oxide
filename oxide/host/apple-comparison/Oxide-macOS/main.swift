import AppKit
import Darwin

final class OxideMacComparisonAppDelegate: NSObject, NSApplicationDelegate
{
   private var window: NSWindow?
   private var adapter: OxideMacScenarioAdapter?
   private var campaignExecutor: BenchmarkCampaignExecutor?
   private var targetedExecutor: BenchmarkTargetedExecutor?
   private var releaseCandidateExecutor: BenchmarkReleaseCandidateCaptureExecutor?
   private var canonicalLaunchCoordinator: MacOSCanonicalLaunchApplicationCoordinator?

   func applicationDidFinishLaunching(_ notification: Notification)
   {
      let applicationDidFinishTimestamp = mach_continuous_time()
      let window = MacOSTrustedInputWindow(
         contentRect: NSRect(x: 0, y: 0, width: 390, height: 844),
         styleMask: [.titled, .closable],
         backing: .buffered,
         defer: false
      )
      window.title = "Oxide Production Comparison"
      window.isReleasedWhenClosed = false
      window.colorSpace = .sRGB
      window.isOpaque = true
      window.center()
      self.window = window
      window.makeKeyAndOrderFront(nil)
      NSApp.activate(ignoringOtherApps: true)
      guard let adapter = OxideMacScenarioAdapter(window: window) else
      {
         NSApp.terminate(nil)
         return
      }
      let campaign = CommandLine.arguments.contains("-oxide-compare-chunk")
      let targeted = CommandLine.arguments.contains("-oxide-compare-targeted")
      let releaseCandidateCapture = CommandLine.arguments.contains("-oxide-compare-release-candidates")
      do
      {
         let root = try benchmarkResourceRoot()
         if targeted
         {
            let executor = try BenchmarkTargetedExecutor(
               side: "oxide",
               selection: try benchmarkTargetedSelection(),
               loader: BenchmarkSpecLoader(root: root),
               adapter: adapter,
               store: DurableArtifactStore(root: try campaignOutputRoot())
            )
            targetedExecutor = executor
            self.adapter = adapter
            DispatchQueue.main.async
            {
               do {try executor.run()}
               catch {fputs("Oxide targeted comparison failed: \(error)\n", stderr)}
               NSApp.terminate(nil)
            }
            return
         }
         if releaseCandidateCapture
         {
            let executor = try BenchmarkReleaseCandidateCaptureExecutor(
               side: "oxide",
               planSHA256: try benchmarkReleaseCandidateCapturePlanSHA256(),
               loader: BenchmarkSpecLoader(root: root),
               adapter: adapter,
               store: DurableArtifactStore(root: try campaignOutputRoot())
            )
            releaseCandidateExecutor = executor
            self.adapter = adapter
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.1)
            {
               do {try executor.run()}
               catch {fputs("Oxide release-candidate capture failed: \(error)\n", stderr)}
               NSApp.terminate(nil)
            }
            return
         }
         if campaign
         {
            let invocation = try BenchmarkCampaignInvocation.commandLine()
            if invocation.request.passID == "canonical-launch"
            {
               try runCanonicalLaunch(
                  adapter: adapter,
                  root: root,
                  invocation: invocation,
                  applicationDidFinishTimestamp: applicationDidFinishTimestamp
               )
            }
            else
            {
               try runCampaign(window: window, adapter: adapter, root: root, invocation: invocation)
            }
            self.adapter = adapter
            return
         }
         let scenarioID = UserDefaults.standard.string(forKey: "comparison-scenario") ?? "startup.first-screen"
         let loader = BenchmarkSpecLoader(root: root)
         let scenario = try loader.loadScenario(relativePath: "scenarios/\(scenarioID).json")
         try adapter.prepare(scenario: scenario, loader: loader)
      }
      catch
      {
         if campaign || targeted || releaseCandidateCapture
         {
            fputs("Oxide comparison campaign preparation failed: \(error)\n", stderr)
            fflush(stderr)
         }
         else
         {
            NSAlert(error: error).runModal()
         }
         NSApp.terminate(nil)
         return
      }
      self.adapter = adapter
   }

   private func runCampaign(window: MacOSTrustedInputWindow, adapter: OxideMacScenarioAdapter, root: URL, invocation: BenchmarkCampaignInvocation) throws
   {
      guard invocation.request.side == .oxide else
      {
         throw ComparisonContractError.invalidArgument("Oxide macOS campaign side")
      }
      let output = try campaignOutputRoot()
      let store = DurableArtifactStore(root: output)
      let executor = BenchmarkCampaignExecutor(
         invocation: invocation,
         loader: BenchmarkSpecLoader(root: root),
         adapter: adapter,
         store: store
      )
      {
         result in
         switch result
         {
         case .success:
            finishMacOSCampaign(invocation) {NSApp.terminate(nil)}
         case .failure(let error):
            fputs("Oxide comparison campaign failed: \(error)\n", stderr)
            NSApp.terminate(nil)
         }
      }
      try executor.installTrustedInputObserver(window: window) {adapter.stateGeneration}
      try prepareMacOSCampaign(executor, invocation: invocation, store: store)
      campaignExecutor = executor
      let startBarrier = CommandLine.arguments.contains("-oxide-compare-controlled-start") ? MacOSControlledStartWindowBarrier(window: window) : nil
      startMacOSCampaign(
         executor,
         invocation: invocation,
         onStart:
         {
            startBarrier?.restore()
            window.makeKeyAndOrderFront(nil)
            NSApp.activate(ignoringOtherApps: true)
         }
      )
      {
         fputs("Oxide comparison campaign start barrier timed out\n", stderr)
         NSApp.terminate(nil)
      }
   }

   private func runCanonicalLaunch(adapter: OxideMacScenarioAdapter, root: URL, invocation: BenchmarkCampaignInvocation, applicationDidFinishTimestamp: UInt64) throws
   {
      guard invocation.request.side == .oxide else
      {
         throw ComparisonContractError.invalidArgument("Oxide canonical launch side")
      }
      let coordinator = MacOSCanonicalLaunchApplicationCoordinator(
         invocation: try MacOSCanonicalLaunchInvocation(
            runID: invocation.request.runID,
            planSHA256: invocation.request.planSHA256,
            chunkID: invocation.chunkID,
            pairIndex: invocation.request.pairIndex,
            side: invocation.request.side,
            generation: invocation.request.generation,
            launchClass: try MacOSCanonicalLaunchClass(commandLineArguments: CommandLine.arguments),
            dataRoot: MacOSCanonicalLaunchClass.dataRoot(commandLineArguments: CommandLine.arguments),
            packID: invocation.packID,
            planArtifact: invocation.planArtifact,
            trustedInputOffsetNs: 50_000_000,
            trustedInputDeadlineNs: 2_000_000_000
         ),
         loader: BenchmarkSpecLoader(root: root),
         adapter: adapter,
         responseSource: adapter,
         store: DurableArtifactStore(root: try campaignOutputRoot())
      )
      {
         result in
         if case .failure(let error) = result
         {
            fputs("Oxide canonical launch failed: \(error)\n", stderr)
            NSApp.terminate(nil)
         }
      }
      canonicalLaunchCoordinator = coordinator
      try coordinator.start(applicationDidFinishTimestamp: applicationDidFinishTimestamp)
   }

   private func campaignOutputRoot() throws -> URL
   {
      let arguments = CommandLine.arguments
      guard let index = arguments.firstIndex(of: "-oxide-compare-output-root"), index + 1 < arguments.count else
      {
         throw ComparisonContractError.missingArgument("-oxide-compare-output-root")
      }
      return URL(fileURLWithPath: arguments[index + 1], isDirectory: true)
   }

   func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool
   {
      true
   }

   private func benchmarkResourceRoot() throws -> URL
   {
      guard let resources = Bundle.main.resourceURL else {throw OxideMacScenarioAdapterFailure.metalView}
      let nested = resources.appendingPathComponent("v1", isDirectory: true)
      return FileManager.default.fileExists(atPath: nested.path) ? nested : resources
   }
}

let app = NSApplication.shared
let delegate = OxideMacComparisonAppDelegate()
app.setActivationPolicy(.regular)
app.delegate = delegate
app.run()
