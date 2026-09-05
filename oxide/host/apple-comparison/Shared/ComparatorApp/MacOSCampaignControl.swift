#if os(macOS)
import AppKit
import Darwin
import Dispatch
import Foundation

final class MacOSControlledStartWindowBarrier
{
   private unowned let window: NSWindow
   private let preparedContentViewController: NSViewController?
   private let preparedContentView: NSView?

   init(window: NSWindow)
   {
      self.window = window
      preparedContentViewController = window.contentViewController
      preparedContentView = window.contentView
      let placeholder = NSView(frame: window.contentView?.frame ?? NSRect(x: 0, y: 0, width: 390, height: 844))
      placeholder.setAccessibilityElement(false)
      window.contentView = placeholder
   }

   func restore()
   {
      if let preparedContentViewController
      {
         window.contentViewController = preparedContentViewController
      }
      else
      {
         window.contentView = preparedContentView
      }
   }
}

func prepareMacOSCampaign(_ executor: BenchmarkCampaignExecutor, invocation: BenchmarkCampaignInvocation, store: DurableArtifactStore) throws
{
   do
   {
      _ = try executor.prepare()
   }
   catch
   {
      let path = "Runs/\(invocation.request.runID)/\(invocation.chunkID)/\(invocation.request.passID)/\(invocation.packID)/\(invocation.request.pairIndex)/\(invocation.request.side.rawValue).failure.txt"
      try? store.durableWrite(Data(String(describing: error).utf8), to: store.root.appendingPathComponent(path))
      throw error
   }
}

func startMacOSCampaign(_ executor: BenchmarkCampaignExecutor, invocation: BenchmarkCampaignInvocation, timeout: DispatchTimeInterval = .seconds(30), onStart: @escaping () -> Void, onTimeout: @escaping () -> Void)
{
   guard CommandLine.arguments.contains("-oxide-compare-controlled-start") else
   {
      DispatchQueue.main.async
      {
         onStart()
         executor.start()
      }
      return
   }
   let ready = comparisonGenerationNotification(comparisonReadyNotification, generation: invocation.request.generation)
   let start = comparisonGenerationNotification(comparisonStartNotification, generation: invocation.request.generation)
   DispatchQueue.global(qos: .userInitiated).async
   {
      let started = runWaitingForComparisonDarwinNotification(start, timeout: timeout)
      {
         postComparisonDarwinNotification(ready)
      }
      DispatchQueue.main.async
      {
         if started
         {
            onStart()
            executor.start()
         }
         else
         {
            onTimeout()
         }
      }
   }
}

func finishMacOSCampaign(_ invocation: BenchmarkCampaignInvocation, timeout: DispatchTimeInterval = .seconds(180), onDismiss: @escaping () -> Void)
{
   guard CommandLine.arguments.contains("-oxide-compare-controlled-start") else
   {
      onDismiss()
      return
   }
   let stop = comparisonGenerationNotification("com.oxide.compare.stop", generation: invocation.request.generation)
   let ready = comparisonGenerationNotification("com.oxide.compare.stop-ready", generation: invocation.request.generation)
   DispatchQueue.global(qos: .userInitiated).async
   {
      let stopped = runWaitingForComparisonDarwinNotification(stop, timeout: timeout)
      {
         postComparisonDarwinNotification(ready)
      }
      if stopped
      {
         do
         {
            guard let rootIndex = CommandLine.arguments.firstIndex(of: "-oxide-compare-controller-root"),
                  rootIndex + 1 < CommandLine.arguments.count else
            {
               throw MacOSTrustedInputFailure.invalidState("trusted input controller root")
            }
            let controlStore = DurableArtifactStore(root: URL(fileURLWithPath: CommandLine.arguments[rootIndex + 1], isDirectory: true))
            let request = controlStore.root.appendingPathComponent(macOSTrustedInputFlushControlFile(suffix: "request"))
            let data = try Data(contentsOf: request)
            let envelope = try JSONDecoder().decode(MacOSTrustedInputFlushEnvelope.self, from: data)
            guard envelope.runID == invocation.request.runID,
                  envelope.planSHA256 == invocation.request.planSHA256,
                  envelope.chunkID == invocation.chunkID,
                  envelope.passID == invocation.request.passID,
                  envelope.packID == invocation.packID,
                  envelope.pairIndex == invocation.request.pairIndex,
                  envelope.side == invocation.request.side,
                  envelope.generation == invocation.request.generation,
                  try JSONEncoder.comparisonCanonical.encode(envelope) == data else
            {
               throw MacOSTrustedInputFailure.invalidState("trusted input stop receipt identity")
            }
            try controlStore.durableWrite(data, to: controlStore.root.appendingPathComponent(macOSTrustedInputStopReceiptControlFile()))
         }
         catch
         {
            fputs("macOS comparison stop receipt failed: \(error)\n", stderr)
            fflush(stderr)
         }
      }
      DispatchQueue.main.async {onDismiss()}
   }
}
#endif
