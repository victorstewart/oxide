@import Foundation;
@import UIKit;
#include <string.h>

int rust_entry(int argc, char **argv);

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

static BOOL OxideShouldLaunchParkedBenchmark(int argc, char **argv) {
  const char *env = getenv("OXIDE_PERF_PARKED");
  if (!env) {
    const char *realAppHost = getenv("OXIDE_PERF_CAMERA_REAL_APP_HOST");
    const BOOL realAppHostEnabled =
        (realAppHost && (strcmp(realAppHost, "1") == 0 ||
                         strcasecmp(realAppHost, "true") == 0 ||
                         strcasecmp(realAppHost, "yes") == 0)) ||
        OxideArgPresent(argc, argv, "-oxide-perf-camera-real-app-host");
    if (realAppHostEnabled) {
      return NO;
    }
    const char *bundlePath = getenv("XCTestBundlePath");
    const char *injectPath = getenv("XCInjectBundleInto");
    return (bundlePath &&
            strstr(bundlePath, "OxideHostPerfTests.xctest") != NULL) ||
           (injectPath &&
            strstr(injectPath, "OxideHostPerfTests.xctest") != NULL);
  }
  return strcmp(env, "1") == 0 || strcasecmp(env, "true") == 0 ||
         strcasecmp(env, "yes") == 0;
}

static BOOL OxideShouldLaunchUIKitPerfApp(int argc, char **argv) {
  const char *env = getenv("OXIDE_PERF_UIKIT_LAUNCH");
  if (env) {
    return strcmp(env, "1") == 0 || strcasecmp(env, "true") == 0 ||
           strcasecmp(env, "yes") == 0;
  }
  return OxideArgPresent(argc, argv, "-oxide-perf-uikit-launch");
}

static void OxideConfigurePerfProcessEnvironment(void) {
  unsetenv("MTL_HUD_ENABLED");
  setenv("MTL_HUD_ENABLED", "0", 1);
}

int main(int argc, char **argv) {
  @autoreleasepool {
    const BOOL launchPerfApp = OxideShouldLaunchParkedBenchmark(argc, argv) ||
                               OxideShouldLaunchUIKitPerfApp(argc, argv);
    if (launchPerfApp) {
      OxideConfigurePerfProcessEnvironment();
      return UIApplicationMain(argc, argv, nil, @"OxidePerfParkedAppDelegate");
    }
    return rust_entry(argc, argv);
  }
}
