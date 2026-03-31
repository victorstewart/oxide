#import "BenchmarkBridge.h"

NSString * const BenchmarkBridgeMarkerNotification = @"BenchmarkBridgeMarkerNotification";

@implementation BenchmarkBridge

RCT_EXPORT_MODULE();

+ (BOOL)requiresMainQueueSetup
{
  return YES;
}

RCT_EXPORT_METHOD(logMarker:(NSString *)marker)
{
  [[NSNotificationCenter defaultCenter] postNotificationName:BenchmarkBridgeMarkerNotification
                                                      object:nil
                                                    userInfo:@{@"marker": marker ?: @""}];
  NSLog(@"%@", marker);
}

RCT_EXPORT_METHOD(logContract:(NSDictionary *)contract)
{
  NSError *error = nil;
  NSData *jsonData = [NSJSONSerialization dataWithJSONObject:contract options:0 error:&error];
  if (jsonData == nil)
  {
    NSLog(@"OXIDE_CAMERA_CONTRACT_SUMMARY {\"error\":\"serialization-failed\"}");
    return;
  }

  NSString *json = [[NSString alloc] initWithData:jsonData encoding:NSUTF8StringEncoding];
  NSLog(@"OXIDE_CAMERA_CONTRACT_SUMMARY %@", json);
}

@end
