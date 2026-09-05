import CoreFoundation
import Dispatch
import Foundation

private final class ComparisonDarwinWaiter
{
   let semaphore = DispatchSemaphore(value: 0)
}

private func comparisonDarwinNotificationCallback(
   center: CFNotificationCenter?,
   observer: UnsafeMutableRawPointer?,
   name: CFNotificationName?,
   object: UnsafeRawPointer?,
   userInfo: CFDictionary?
)
{
   guard let observer else
   {
      return
   }
   Unmanaged<ComparisonDarwinWaiter>.fromOpaque(observer).takeUnretainedValue().semaphore.signal()
}

func postComparisonDarwinNotification(_ name: String)
{
   CFNotificationCenterPostNotification(
      CFNotificationCenterGetDarwinNotifyCenter(),
      CFNotificationName(name as CFString),
      nil,
      nil,
      true
   )
}

func runWaitingForComparisonDarwinNotification(_ name: String, timeout: DispatchTimeInterval, action: () -> Void) -> Bool
{
   let waiter = ComparisonDarwinWaiter()
   let observer = Unmanaged.passUnretained(waiter).toOpaque()
   let center = CFNotificationCenterGetDarwinNotifyCenter()
   CFNotificationCenterAddObserver(
      center,
      observer,
      comparisonDarwinNotificationCallback,
      name as CFString,
      nil,
      .deliverImmediately
   )
   defer { CFNotificationCenterRemoveObserver(center, observer, CFNotificationName(name as CFString), nil) }
   action()
   return waiter.semaphore.wait(timeout: .now() + timeout) == .success
}

func runWaitingForComparisonDarwinNotificationOrArtifact(_ name: String, artifactURL: URL, timeout: TimeInterval, action: () -> Void) -> Bool
{
   let waiter = ComparisonDarwinWaiter()
   let observer = Unmanaged.passUnretained(waiter).toOpaque()
   let center = CFNotificationCenterGetDarwinNotifyCenter()
   CFNotificationCenterAddObserver(
      center,
      observer,
      comparisonDarwinNotificationCallback,
      name as CFString,
      nil,
      .deliverImmediately
   )
   defer { CFNotificationCenterRemoveObserver(center, observer, CFNotificationName(name as CFString), nil) }
   action()
   let deadline = Date().addingTimeInterval(timeout)
   while Date() < deadline
   {
      if FileManager.default.fileExists(atPath: artifactURL.path)
      {
         return true
      }
      if waiter.semaphore.wait(timeout: .now() + .milliseconds(10)) == .success
      {
         return true
      }
   }
   return FileManager.default.fileExists(atPath: artifactURL.path)
}
