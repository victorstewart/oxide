@import Foundation;
@import UIKit;
#include <stdlib.h>
#include <string.h>

int rust_entry(int argc, char **argv);

static BOOL gOxideLaunchPerfApp = NO;

static BOOL OxideArgPresent(int argc, char **argv, const char *needle) {
  if (!needle) {
    return NO;
  }
  for (int i = 1; i < argc; ++i) {
    if (argv[i] && strcmp(argv[i], needle) == 0) {
      return YES;
    }
  }
  return NO;
}

static NSString *OxideEnvValue(NSString *name) {
  return [NSProcessInfo.processInfo.environment objectForKey:name];
}

static BOOL OxideEnvTruthy(NSString *value) {
  return value.length > 0 &&
         ([value isEqualToString:@"1"] ||
          [value caseInsensitiveCompare:@"true"] == NSOrderedSame ||
          [value caseInsensitiveCompare:@"yes"] == NSOrderedSame);
}

static BOOL OxideShouldLaunchParkedBenchmark(int argc, char **argv) {
  NSString *env = OxideEnvValue(@"OXIDE_PERF_PARKED");
  if (!env) {
    NSString *realAppHost = OxideEnvValue(@"OXIDE_PERF_CAMERA_REAL_APP_HOST");
    const BOOL realAppHostEnabled = OxideEnvTruthy(realAppHost) ||
                                    OxideArgPresent(argc, argv,
                                                    "-oxide-perf-camera-real-app-host");
    NSString *staticIdleHost =
        OxideEnvValue(@"OXIDE_PERF_STATIC_IDLE_REAL_APP_HOST");
    const BOOL staticIdleHostEnabled = OxideEnvTruthy(staticIdleHost) ||
                                       OxideArgPresent(argc, argv,
                                                       "-oxide-perf-static-idle-real-app-host");
    if (realAppHostEnabled || staticIdleHostEnabled) {
      return NO;
    }
    NSString *bundlePath = OxideEnvValue(@"XCTestBundlePath");
    NSString *injectPath = OxideEnvValue(@"XCInjectBundleInto");
    return (bundlePath.length > 0 &&
            [bundlePath rangeOfString:@"OxideHostPerfTests.xctest"].location !=
                NSNotFound) ||
           (injectPath.length > 0 &&
            [injectPath rangeOfString:@"OxideHostPerfTests.xctest"].location !=
                NSNotFound);
  }
  return OxideEnvTruthy(env);
}

static BOOL OxideShouldLaunchUIKitPerfApp(int argc, char **argv) {
  NSString *env = OxideEnvValue(@"OXIDE_PERF_UIKIT_LAUNCH");
  return env ? OxideEnvTruthy(env)
             : OxideArgPresent(argc, argv, "-oxide-perf-uikit-launch");
}

static void OxideConfigurePerfProcessEnvironment(void) {
  unsetenv("MTL_HUD_ENABLED");
  setenv("MTL_HUD_ENABLED", "0", 1);
}

static BOOL OxideShouldUsePerfSceneDelegate(void) {
  return gOxideLaunchPerfApp ||
         OxideShouldLaunchParkedBenchmark(0, NULL) ||
         OxideShouldLaunchUIKitPerfApp(0, NULL);
}

@interface OxideSceneMuxDelegate : UIResponder <UIWindowSceneDelegate>
@property(nonatomic, strong) id<UIWindowSceneDelegate> delegate;
@end

@implementation OxideSceneMuxDelegate
- (id<UIWindowSceneDelegate>)oxide_delegate {
  if (_delegate) {
    return _delegate;
  }
  NSString *className = OxideShouldUsePerfSceneDelegate()
                            ? @"OxidePerfParkedSceneDelegate"
                            : @"RustSceneDelegate";
  Class delegateClass = NSClassFromString(className);
  if (!delegateClass) {
    @throw [NSException exceptionWithName:NSInternalInconsistencyException
                                   reason:[NSString stringWithFormat:
                                               @"missing scene delegate %@",
                                               className]
                                 userInfo:nil];
  }
  _delegate = [[delegateClass alloc] init];
  return _delegate;
}

- (void)scene:(UIScene *)scene
    willConnectToSession:(UISceneSession *)session
                 options:(UISceneConnectionOptions *)connectionOptions {
  id<UIWindowSceneDelegate> delegate = [self oxide_delegate];
  if ([delegate respondsToSelector:_cmd]) {
    [delegate scene:scene willConnectToSession:session options:connectionOptions];
  }
}

- (BOOL)respondsToSelector:(SEL)aSelector {
  if ([super respondsToSelector:aSelector]) {
    return YES;
  }
  return [[self oxide_delegate] respondsToSelector:aSelector];
}

- (id)forwardingTargetForSelector:(SEL)aSelector {
  id<UIWindowSceneDelegate> delegate = [self oxide_delegate];
  if ([delegate respondsToSelector:aSelector]) {
    return delegate;
  }
  return [super forwardingTargetForSelector:aSelector];
}
@end

int main(int argc, char **argv) {
  @autoreleasepool {
    const BOOL launchPerfApp = OxideShouldLaunchParkedBenchmark(argc, argv) ||
                               OxideShouldLaunchUIKitPerfApp(argc, argv);
    gOxideLaunchPerfApp = launchPerfApp;
    if (launchPerfApp) {
      OxideConfigurePerfProcessEnvironment();
      return UIApplicationMain(argc, argv, nil, @"OxidePerfParkedAppDelegate");
    }
    return rust_entry(argc, argv);
  }
}
