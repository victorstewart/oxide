import UIKit

final class TransportProbeAppDelegate: NSObject, UIApplicationDelegate
{
   var window: UIWindow?
   private var previewAdapter: BenchmarkScenarioAdapter?
   private var campaignExecutor: BenchmarkCampaignExecutor?

   func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool
   {
      let window = UIWindow(frame: UIScreen.main.bounds)
      let controller = UIViewController()
      controller.view.backgroundColor = .systemBackground
      let label = UILabel(frame: controller.view.bounds)
      label.autoresizingMask = [.flexibleWidth, .flexibleHeight]
      label.textAlignment = .center
      label.text = Bundle.main.bundleIdentifier
      controller.view.addSubview(label)
      window.rootViewController = controller
      window.makeKeyAndVisible()
      self.window = window

      if CommandLine.arguments.contains("-oxide-compare-preview")
      {
         runPreview(window: window)
         return true
      }

      if CommandLine.arguments.contains("-oxide-compare-chunk")
      {
         runCampaign(window: window)
         return true
      }

      do
      {
         let request = try ComparisonProbeRequest.commandLine()
         let ready = comparisonGenerationNotification(comparisonReadyNotification, generation: request.generation)
         let complete = comparisonGenerationNotification(comparisonCompleteNotification, generation: request.generation)
         let failed = comparisonGenerationNotification(comparisonFailedNotification, generation: request.generation)
         DispatchQueue.global(qos: .userInitiated).async
         {
            do
            {
               let root = try DurableArtifactStore.root(for: request.transportMode)
               try ComparisonTransport(request: request, store: DurableArtifactStore(root: root)).runSide()
               postComparisonDarwinNotification(complete)
            }
            catch
            {
               postComparisonDarwinNotification(failed)
            }
         }
         postComparisonDarwinNotification(ready)
      }
      catch
      {
         postComparisonDarwinNotification(comparisonFailedNotification)
      }
      return true
   }

   private func runCampaign(window: UIWindow)
   {
      var failedNotification = comparisonFailedNotification
      var failureStore: DurableArtifactStore?
      var failurePath: String?
      do
      {
         let invocation = try BenchmarkCampaignInvocation.commandLine()
         failedNotification = comparisonGenerationNotification(comparisonFailedNotification, generation: invocation.request.generation)
         guard let resourceURL = Bundle.main.resourceURL,
               let adapter = makeBenchmarkScenarioAdapter(window: window) else
         {
            throw BenchmarkContractFailure.invalidScenario("campaign")
         }
         let loader = BenchmarkSpecLoader(root: resourceURL.appendingPathComponent("v1", isDirectory: true))
         let store = DurableArtifactStore(root: try DurableArtifactStore.root(for: invocation.request.transportMode))
         failureStore = store
         failurePath = "Runs/\(invocation.request.runID)/\(invocation.chunkID)/\(invocation.request.passID)/\(invocation.packID)/\(invocation.request.pairIndex)/\(invocation.request.side.rawValue).failure.txt"
         let executor = BenchmarkCampaignExecutor(
            invocation: invocation,
            loader: loader,
            adapter: adapter,
            store: store
         )
         {
            result in
            let complete = comparisonGenerationNotification(comparisonCompleteNotification, generation: invocation.request.generation)
            let failed = comparisonGenerationNotification(comparisonFailedNotification, generation: invocation.request.generation)
            switch result
            {
            case .success: postComparisonDarwinNotification(complete)
            case .failure: postComparisonDarwinNotification(failed)
            }
         }
         _ = try executor.prepare()
         campaignExecutor = executor
         let ready = comparisonGenerationNotification(comparisonReadyNotification, generation: invocation.request.generation)
         let start = comparisonGenerationNotification(comparisonStartNotification, generation: invocation.request.generation)
         let failed = comparisonGenerationNotification(comparisonFailedNotification, generation: invocation.request.generation)
         DispatchQueue.global(qos: .userInitiated).async
         {
            let started = runWaitingForComparisonDarwinNotification(start, timeout: .seconds(15))
            {
               postComparisonDarwinNotification(ready)
            }
            DispatchQueue.main.async
            {
               if started
               {
                  executor.start()
               }
               else
               {
                  postComparisonDarwinNotification(failed)
               }
            }
         }
      }
      catch
      {
         if let failureStore, let failurePath
         {
            try? failureStore.durableWrite(Data(String(describing: error).utf8), to: failureStore.root.appendingPathComponent(failurePath))
         }
         postComparisonDarwinNotification(failedNotification)
      }
   }

   private func runPreview(window: UIWindow)
   {
      do
      {
         let invocation = try ComparisonPreviewInvocation.commandLine()
         guard let resourceURL = Bundle.main.resourceURL,
               let adapter = makeBenchmarkScenarioAdapter(window: window) else
         {
            throw BenchmarkContractFailure.invalidScenario("preview")
         }
         let loader = BenchmarkSpecLoader(root: resourceURL.appendingPathComponent("v1", isDirectory: true))
         let scenario = try loader.loadScenario(relativePath: "scenarios/\(invocation.scenarioID).json")
         let documents = try FileManager.default.url(for: .documentDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
         let previewDirectory = documents.appendingPathComponent("ComparisonPreview", isDirectory: true)
         if FileManager.default.fileExists(atPath: previewDirectory.path)
         {
            try FileManager.default.removeItem(at: previewDirectory)
         }
         try adapter.prepare(scenario: scenario, loader: loader)
         previewAdapter = adapter
         let checkpointID = scenario.parityCheckpoints.first?.id ?? "prepared"
         let checkpoint = try adapter.checkpoint(id: checkpointID)
         guard let capture = adapter as? BenchmarkPreviewCapture else
         {
            throw BenchmarkContractFailure.invalidScenario("preview-capture")
         }
         let png = try capture.previewPNG()
         let previewSHA256 = comparisonSHA256(png)
         let store = DurableArtifactStore(root: documents)
         try store.durableWrite(png, to: previewDirectory.appendingPathComponent("preview.png"))
         let complete = ComparisonPreviewComplete(
            schemaVersion: 1,
            generation: invocation.generation,
            scenarioID: scenario.id,
            bundleID: Bundle.main.bundleIdentifier ?? "unknown",
            fixtureSHA256: scenario.fixture.sha256,
            fontPackSHA256: scenario.fontPack.sha256,
            previewSHA256: previewSHA256,
            visibleRoleCounts: checkpoint.visibleRoleCounts
         )
         _ = try store.durableJSON(complete, relativePath: "ComparisonPreview/complete.json")
         postComparisonDarwinNotification(comparisonGenerationNotification(comparisonCompleteNotification, generation: invocation.generation))
      }
      catch
      {
         if let documents = try? FileManager.default.url(for: .documentDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
         {
            let message = Data(String(describing: error).utf8)
            try? DurableArtifactStore(root: documents).durableWrite(message, to: documents.appendingPathComponent("ComparisonPreview/error.txt"))
         }
         postComparisonDarwinNotification(comparisonFailedNotification)
      }
   }
}
