#import "OxideMetalAppHost.h"
#import <Metal/Metal.h>
#import <QuartzCore/QuartzCore.h>
#include <stdint.h>
#include <string.h>

typedef struct OxideMetalAppHostAcquiredFrame {
  void *y_tex;
  void *uv_tex;
  int32_t width;
  int32_t height;
  int32_t bit_depth;
  int32_t matrix;
  int32_t video_range;
  int32_t color_space;
  uint32_t slot;
  uint64_t generation;
  uint64_t timestamp_ns;
} OxideMetalAppHostAcquiredFrame;

static const OxideMetalAppHostConfig *OxideMetalHostConfig(void) {
  return oxide_metal_app_host_config();
}

static BOOL OxideMetalHostEnvTruthy(NSString *name) {
  if (name == nil) {
    return NO;
  }
  NSString *value = NSProcessInfo.processInfo.environment[name];
  if (value == nil) {
    return NO;
  }
  NSString *normalized = [[value stringByTrimmingCharactersInSet:
                                      NSCharacterSet.whitespaceAndNewlineCharacterSet]
      lowercaseString];
  return [normalized isEqualToString:@"1"] ||
         [normalized isEqualToString:@"true"] ||
         [normalized isEqualToString:@"yes"];
}

static void OxideMetalHostLog(NSString *line) {
  const OxideMetalAppHostConfig *config = OxideMetalHostConfig();
  NSString *prefix = config != NULL && config->log_prefix != nil ? config->log_prefix : @"Oxide";
  NSLog(@"[%@] %@", prefix, line);
  NSString *env_name = config != NULL ? config->touch_log_env_name : nil;
  NSString *file_name = config != NULL ? config->touch_log_filename : nil;
  if (!OxideMetalHostEnvTruthy(env_name) || file_name == nil) {
    return;
  }
  NSArray<NSURL *> *dirs =
      [NSFileManager.defaultManager URLsForDirectory:NSDocumentDirectory
                                           inDomains:NSUserDomainMask];
  NSURL *url = [dirs.firstObject URLByAppendingPathComponent:file_name];
  NSString *entry = [line stringByAppendingString:@"\n"];
  NSData *data = [entry dataUsingEncoding:NSUTF8StringEncoding];
  if (![NSFileManager.defaultManager fileExistsAtPath:url.path]) {
    [data writeToURL:url atomically:YES];
    return;
  }
  NSFileHandle *handle = [NSFileHandle fileHandleForWritingToURL:url error:nil];
  [handle seekToEndOfFile];
  [handle writeData:data];
  [handle closeFile];
}

void oxide_host_ios_log(const char *utf8, size_t len) {
  if (utf8 == NULL || len == 0) {
    return;
  }
  NSString *message = [[NSString alloc] initWithBytes:utf8
                                               length:len
                                             encoding:NSUTF8StringEncoding];
  if (message != nil) {
    OxideMetalHostLog([NSString stringWithFormat:@"rust %@", message]);
  }
}

uint64_t oxide_host_perf_signpost_begin(const char *utf8, size_t len) {
  (void)utf8;
  (void)len;
  return 0;
}

void oxide_host_perf_signpost_end(const char *utf8, size_t len, uint64_t signpost_id) {
  (void)utf8;
  (void)len;
  (void)signpost_id;
}

int32_t oxide_host_power_lowpower(void) {
  if (@available(iOS 9.0, *)) {
    return NSProcessInfo.processInfo.lowPowerModeEnabled ? 1 : 0;
  }
  return 0;
}

int32_t oxide_host_thermal_state(void) {
  if (@available(iOS 11.0, *)) {
    switch (NSProcessInfo.processInfo.thermalState) {
    case NSProcessInfoThermalStateNominal:
      return 0;
    case NSProcessInfoThermalStateFair:
      return 1;
    case NSProcessInfoThermalStateSerious:
      return 2;
    case NSProcessInfoThermalStateCritical:
      return 3;
    }
  }
  return 0;
}

void oxide_cam_start_default(void) {}
void oxide_cam_stop(void) {}

void oxide_cam_mark_presented_generation(uint64_t generation) {
  (void)generation;
}

int32_t oxide_cam_acquire_latest_frame_ex(uint64_t min_generation_exclusive,
                                          OxideMetalAppHostAcquiredFrame *out_frame) {
  (void)min_generation_exclusive;
  if (out_frame != NULL) {
    memset(out_frame, 0, sizeof(*out_frame));
  }
  return 0;
}

void oxide_cam_release_acquired(uint32_t slot, uint64_t generation) {
  (void)slot;
  (void)generation;
}

uint64_t oxide_cam_peek_latest_generation(void) {
  return 0;
}

uint64_t oxide_cam_peek_latest_timestamp_ns(void) {
  return 0;
}

int32_t oxide_cam_get_latest_ex(void **y_tex, void **uv_tex, int32_t *w, int32_t *h,
                                int32_t *bit_depth, int32_t *matrix,
                                int32_t *video_range, int32_t *color_space) {
  if (y_tex != NULL) {
    *y_tex = NULL;
  }
  if (uv_tex != NULL) {
    *uv_tex = NULL;
  }
  if (w != NULL) {
    *w = 0;
  }
  if (h != NULL) {
    *h = 0;
  }
  if (bit_depth != NULL) {
    *bit_depth = 0;
  }
  if (matrix != NULL) {
    *matrix = 0;
  }
  if (video_range != NULL) {
    *video_range = 0;
  }
  if (color_space != NULL) {
    *color_space = 0;
  }
  return 0;
}

int32_t oxide_cam_get_latest_bgra(void **bgra_tex, int32_t *w, int32_t *h) {
  if (bgra_tex != NULL) {
    *bgra_tex = NULL;
  }
  if (w != NULL) {
    *w = 0;
  }
  if (h != NULL) {
    *h = 0;
  }
  return 0;
}

@interface OxideMetalHostView : UIView
- (void)emitTouch:(UITouch *)touch phase:(uint32_t)phase;
@end

static __weak OxideMetalHostView *gOxideMetalHostView = nil;

@interface OxideMetalHostApplication : UIApplication
@end

@implementation OxideMetalHostApplication
- (void)sendEvent:(UIEvent *)event {
  [super sendEvent:event];
  if (event.type != UIEventTypeTouches || gOxideMetalHostView == nil) {
    return;
  }
  for (UITouch *touch in event.allTouches) {
    uint32_t phase = 1;
    switch (touch.phase) {
    case UITouchPhaseBegan:
      phase = 0;
      break;
    case UITouchPhaseMoved:
    case UITouchPhaseStationary:
      phase = 1;
      break;
    case UITouchPhaseEnded:
      phase = 2;
      break;
    case UITouchPhaseCancelled:
    default:
      phase = 3;
      break;
    }
    [gOxideMetalHostView emitTouch:touch phase:phase];
  }
}
@end

@interface OxideMetalHostWindow : UIWindow
@end

@implementation OxideMetalHostWindow
@end

@interface OxideMetalHostViewController : UIViewController
@end

@implementation OxideMetalHostViewController
- (BOOL)prefersStatusBarHidden {
  return YES;
}

- (BOOL)prefersHomeIndicatorAutoHidden {
  return YES;
}
@end

@implementation OxideMetalHostView

+ (Class)layerClass {
  return CAMetalLayer.class;
}

- (instancetype)initWithFrame:(CGRect)frame {
  self = [super initWithFrame:frame];
  if (!self) {
    return nil;
  }
  self.multipleTouchEnabled = YES;
  self.opaque = YES;
  self.backgroundColor = UIColor.blackColor;
  CAMetalLayer *layer = (CAMetalLayer *)self.layer;
  layer.pixelFormat = MTLPixelFormatBGRA8Unorm;
  layer.framebufferOnly = YES;
  if (@available(iOS 13.0, *)) {
    layer.maximumDrawableCount = 3;
  }
  return self;
}

- (void)didMoveToWindow {
  [super didMoveToWindow];
  gOxideMetalHostView = self;
  [self updateDrawableSize];
}

- (void)layoutSubviews {
  [super layoutSubviews];
  [self updateDrawableSize];
}

- (void)updateDrawableSize {
  CAMetalLayer *layer = (CAMetalLayer *)self.layer;
  CGFloat scale = self.window.screen.nativeScale ?: UIScreen.mainScreen.nativeScale;
  self.contentScaleFactor = scale;
  layer.contentsScale = scale;
  layer.drawableSize =
      CGSizeMake(self.bounds.size.width * scale, self.bounds.size.height * scale);
}

- (void)emitTouch:(UITouch *)touch phase:(uint32_t)phase {
  const OxideMetalAppHostConfig *config = OxideMetalHostConfig();
  if (config == NULL || config->touch == NULL) {
    return;
  }
  CGPoint p = [touch locationInView:self];
  NSTimeInterval ts = touch.timestamp;
  config->touch((uint64_t)(uintptr_t)touch, phase, (float)p.x, (float)p.y,
                (uint64_t)(ts * 1000000000.0));
}

@end

@interface OxideMetalHostSceneDelegate : UIResponder <UIWindowSceneDelegate>
@property(nonatomic, strong) UIWindow *window;
@property(nonatomic, strong) CADisplayLink *displayLink;
@property(nonatomic, weak) OxideMetalHostView *metalView;
@property(nonatomic) BOOL initialized;
@end

@implementation OxideMetalHostSceneDelegate

- (void)scene:(UIScene *)scene
    willConnectToSession:(UISceneSession *)session
                 options:(UISceneConnectionOptions *)connectionOptions {
  (void)session;
  (void)connectionOptions;
  UIWindowScene *windowScene = (UIWindowScene *)scene;
  OxideMetalHostWindow *window = [[OxideMetalHostWindow alloc] initWithWindowScene:windowScene];
  UIViewController *vc = OxideMetalHostViewController.new;
  OxideMetalHostView *view = [[OxideMetalHostView alloc] initWithFrame:window.bounds];
  vc.view = view;
  window.rootViewController = vc;
  self.window = window;
  self.metalView = view;
  [window makeKeyAndVisible];

  self.displayLink = [CADisplayLink displayLinkWithTarget:self selector:@selector(onTick:)];
  if (@available(iOS 15.0, *)) {
    self.displayLink.preferredFrameRateRange = CAFrameRateRangeMake(60, 120, 120);
  }
  [self.displayLink addToRunLoop:NSRunLoop.mainRunLoop forMode:NSRunLoopCommonModes];
}

- (void)sceneDidBecomeActive:(UIScene *)scene {
  (void)scene;
  self.displayLink.paused = NO;
}

- (void)sceneWillResignActive:(UIScene *)scene {
  (void)scene;
  self.displayLink.paused = YES;
}

- (void)sceneDidDisconnect:(UIScene *)scene {
  (void)scene;
  [self.displayLink invalidate];
  self.displayLink = nil;
  const OxideMetalAppHostConfig *config = OxideMetalHostConfig();
  if (config != NULL && config->shutdown != NULL) {
    config->shutdown();
  }
}

- (void)onTick:(CADisplayLink *)displayLink {
  (void)displayLink;
  OxideMetalHostView *view = self.metalView;
  if (!view.window) {
    return;
  }
  const OxideMetalAppHostConfig *config = OxideMetalHostConfig();
  if (config == NULL || config->init == NULL || config->frame == NULL) {
    OxideMetalHostLog(@"missing app callbacks");
    return;
  }
  CAMetalLayer *layer = (CAMetalLayer *)view.layer;
  [view updateDrawableSize];
  CGSize size = layer.drawableSize;
  CGFloat scale = view.window.screen.nativeScale ?: UIScreen.mainScreen.nativeScale;
  uint32_t width = (uint32_t)lrint((double)size.width);
  uint32_t height = (uint32_t)lrint((double)size.height);
  if (!self.initialized) {
    if (config->init(width, height, (float)scale) != 0) {
      OxideMetalHostLog(@"app init failed");
      return;
    }
    self.initialized = YES;
  }
  id<CAMetalDrawable> drawable = [layer nextDrawable];
  if (!drawable) {
    return;
  }
  int32_t rc = config->frame(width, height, (float)scale, (__bridge void *)drawable);
  if (rc != 0) {
    OxideMetalHostLog([NSString stringWithFormat:@"frame failed rc=%d", rc]);
  }
}

@end

@interface OxideMetalHostAppDelegate : UIResponder <UIApplicationDelegate>
@end

@implementation OxideMetalHostAppDelegate
- (UISceneConfiguration *)application:(UIApplication *)application
    configurationForConnectingSceneSession:(UISceneSession *)connectingSceneSession
                                   options:(UISceneConnectionOptions *)options {
  (void)application;
  (void)options;
  const OxideMetalAppHostConfig *host_config = OxideMetalHostConfig();
  NSString *scene_name =
      host_config != NULL && host_config->scene_configuration_name != nil
          ? host_config->scene_configuration_name
          : @"oxide-metal-app";
  UISceneConfiguration *config =
      [[UISceneConfiguration alloc] initWithName:scene_name
                                     sessionRole:connectingSceneSession.role];
  config.delegateClass = OxideMetalHostSceneDelegate.class;
  return config;
}
@end

int oxide_metal_app_host_start(int argc, char **argv) {
  return UIApplicationMain(argc, argv, NSStringFromClass(OxideMetalHostApplication.class),
                           NSStringFromClass(OxideMetalHostAppDelegate.class));
}
