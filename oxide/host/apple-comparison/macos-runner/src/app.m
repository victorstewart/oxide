#import <AppKit/AppKit.h>
#import <Foundation/Foundation.h>
#import <QuartzCore/QuartzCore.h>
#import <Metal/Metal.h>
#import <CoreGraphics/CoreGraphics.h>
#import <mach/mach.h>
#import <sys/resource.h>
#import <math.h>
#import <os/signpost.h>

int32_t comparison_init(double host_time);
double comparison_next_wakeup(double now);
double comparison_end_time(void);
uint64_t comparison_begin_frame(double now, double target_time);
int32_t comparison_draw(void *drawable, uint64_t frame_id, double acquire_ms);
void comparison_presented(uint64_t frame_id, double presented_time);
void comparison_host_sample(double now, uint64_t cpu_us, uint64_t resident_bytes,
                            int32_t thermal, uint8_t foreground, uint8_t visible, uint8_t display_awake,
                            double screen_hz, double backing_scale);
uint8_t comparison_finished(double now);
void comparison_finish(double now);
void comparison_external_input(void);
void comparison_display_slept(void);

static const CGFloat kLogicalWidth = 390.0;

void comparison_profile_interval(uint8_t begin)
{
   static os_log_t log;
   static dispatch_once_t once;
   dispatch_once(&once, ^{log = os_log_create("org.oxide.comparison", OS_LOG_CATEGORY_POINTS_OF_INTEREST);});
   if (begin) {os_signpost_interval_begin(log, 1, "OxideOffscreenProfile");}
   else {os_signpost_interval_end(log, 1, "OxideOffscreenProfile");}
}
static const CGFloat kLogicalHeight = 844.0;
static const CGFloat kBackingScale = 3.0;
static const CFTimeInterval kSampleInterval = 1.0;
static const CFTimeInterval kFinishDrainInterval = 0.25;

@class ComparisonHost;

@interface ComparisonMetalView : NSView
@property(nonatomic, weak) ComparisonHost *host;
- (void)configureLayer;
@end

@interface ComparisonHost : NSObject <NSApplicationDelegate, CAMetalDisplayLinkDelegate>
@property(nonatomic, strong) NSWindow *window;
@property(nonatomic, strong) ComparisonMetalView *metalView;
@property(nonatomic, strong) CADisplayLink *displayLink API_AVAILABLE(macos(15.0));
@property(nonatomic, strong) CAMetalDisplayLink *metalDisplayLink API_AVAILABLE(macos(14.0));
@property(nonatomic, strong) NSTimer *wakeupTimer;
@property(nonatomic, strong) id localEventMonitor;
@property(nonatomic) uint64_t wakeupGeneration;
@property(nonatomic) CFTimeInterval lastSample;
@property(nonatomic) BOOL initialized;
@property(nonatomic) BOOL finishing;
@property(nonatomic) BOOL useMetalDisplayLink;
- (void)startControlledRun;
- (void)requestFrame;
- (void)checkRunDeadline;
- (void)runFrameAt:(double)now target:(double)target;
- (void)runFrameAt:(double)now target:(double)target drawable:(id<CAMetalDrawable>)drawable;
@end

static uint64_t CpuMicroseconds(void)
{
   struct rusage usage = {0};
   if (getrusage(RUSAGE_SELF, &usage) != 0)
   {
      return 0;
   }
   return (uint64_t)usage.ru_utime.tv_sec * 1000000u +
      (uint64_t)usage.ru_utime.tv_usec +
      (uint64_t)usage.ru_stime.tv_sec * 1000000u +
      (uint64_t)usage.ru_stime.tv_usec;
}

static uint64_t ResidentBytes(void)
{
   task_vm_info_data_t info = {0};
   mach_msg_type_number_t count = TASK_VM_INFO_COUNT;
   kern_return_t result = task_info(mach_task_self(), TASK_VM_INFO,
      (task_info_t)&info, &count);
   return result == KERN_SUCCESS ? info.phys_footprint : 0;
}

void comparison_offscreen_host_sample(uint64_t *resident_bytes, int32_t *thermal)
{
   if (resident_bytes)
   {
      *resident_bytes = ResidentBytes();
   }
   if (thermal)
   {
      *thermal = (int32_t)NSProcessInfo.processInfo.thermalState;
   }
}

@implementation ComparisonMetalView
- (CALayer *)makeBackingLayer
{
   return [CAMetalLayer layer];
}

- (instancetype)initWithFrame:(NSRect)frame
{
   self = [super initWithFrame:frame];
   if (self)
   {
      self.wantsLayer = YES;
      [self configureLayer];
   }
   return self;
}

- (void)configureLayer
{
   CAMetalLayer *layer = (CAMetalLayer *)self.layer;
   if (!layer.device)
   {
      layer.device = MTLCreateSystemDefaultDevice();
   }
   layer.pixelFormat = MTLPixelFormatBGRA8Unorm;
   if (!layer.colorspace)
   {
      CGColorSpaceRef colorSpace = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
      layer.colorspace = colorSpace;
      CGColorSpaceRelease(colorSpace);
   }
   layer.contentsScale = kBackingScale;
   layer.drawableSize = CGSizeMake(kLogicalWidth * kBackingScale,
      kLogicalHeight * kBackingScale);
   layer.framebufferOnly = NSProcessInfo.processInfo.environment[@"OXIDE_MAC_CAPTURE_DIR"] == nil;
   layer.maximumDrawableCount = 3;
   layer.presentsWithTransaction = NO;
   layer.allowsNextDrawableTimeout = YES;
}

- (void)layout
{
   [super layout];
   [self configureLayer];
}

- (void)drawRect:(NSRect)dirtyRect
{
   (void)dirtyRect;
   [self.host requestFrame];
}
@end

@implementation ComparisonHost
- (void)applicationDidFinishLaunching:(NSNotification *)notification
{
   (void)notification;
   self.useMetalDisplayLink = [NSProcessInfo.processInfo.environment[@"OXIDE_MAC_DISPLAY_LINK"] isEqualToString:@"metal"];
   NSRect content = NSMakeRect(0.0, 0.0, kLogicalWidth, kLogicalHeight);
   self.window = [[NSWindow alloc] initWithContentRect:content
      styleMask:NSWindowStyleMaskTitled | NSWindowStyleMaskClosable
      backing:NSBackingStoreBuffered defer:NO];
   self.window.title = NSProcessInfo.processInfo.environment[@"OXIDE_MAC_CASE"] ?: @"Oxide comparison";
   self.metalView = [[ComparisonMetalView alloc] initWithFrame:content];
   self.metalView.host = self;
   self.window.contentView = self.metalView;
   [self.window center];
   [self.window makeKeyAndOrderFront:nil];
   [NSApp activateIgnoringOtherApps:YES];
   __weak ComparisonHost *weakSelf = self;
   NSEventMask externalInput = NSEventMaskLeftMouseDown | NSEventMaskLeftMouseUp |
      NSEventMaskRightMouseDown | NSEventMaskRightMouseUp | NSEventMaskOtherMouseDown |
      NSEventMaskOtherMouseUp | NSEventMaskMouseMoved | NSEventMaskLeftMouseDragged |
      NSEventMaskRightMouseDragged | NSEventMaskOtherMouseDragged | NSEventMaskScrollWheel |
      NSEventMaskKeyDown | NSEventMaskKeyUp | NSEventMaskMagnify | NSEventMaskRotate |
      NSEventMaskBeginGesture | NSEventMaskEndGesture;
   self.localEventMonitor = [NSEvent addLocalMonitorForEventsMatchingMask:externalInput
      handler:^NSEvent *(NSEvent *event) {
         comparison_external_input();
         [weakSelf requestFrame];
         return event;
      }];
   dispatch_async(dispatch_get_main_queue(), ^{
      [weakSelf startControlledRun];
   });
}

- (void)applicationWillTerminate:(NSNotification *)notification
{
   (void)notification;
   if (self.localEventMonitor)
   {
      [NSEvent removeMonitor:self.localEventMonitor];
      self.localEventMonitor = nil;
   }
}

- (void)startControlledRun
{
   if (self.initialized || self.finishing)
   {
      return;
   }
   [self.metalView configureLayer];
   double now = CACurrentMediaTime();
   if (comparison_init(now) != 0)
   {
      [NSApp terminate:nil];
      return;
   }
   self.initialized = YES;
   [NSWorkspace.sharedWorkspace.notificationCenter addObserver:self
      selector:@selector(displayDidSleep:) name:NSWorkspaceScreensDidSleepNotification object:nil];
   self.lastSample = 0.0;
   [self sampleAt:now];
   // Completion must not depend on a display callback: a sleeping display or
   // hidden session can suspend callbacks entirely. Keep the invalid receipt.
   [self checkRunDeadline];
   [self requestFrame];
}

- (void)displayDidSleep:(NSNotification *)notification
{
   (void)notification;
   comparison_display_slept();
}

- (void)checkRunDeadline
{
   if (self.finishing)
   {
      return;
   }
   double now = CACurrentMediaTime();
   if (comparison_finished(now))
   {
      [self finishAt:now];
      return;
   }
   double remaining = fmax(0.001, comparison_end_time() - now + 0.001);
   dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(remaining * NSEC_PER_SEC)),
      dispatch_get_main_queue(), ^{
         [self checkRunDeadline];
      });
}

- (void)requestFrame
{
   if (!self.initialized || self.finishing)
   {
      return;
   }
   [self scheduleFor:comparison_next_wakeup(CACurrentMediaTime())];
}

- (void)scheduleFor:(double)deadline
{
   if (self.finishing)
   {
      return;
   }
   double now = CACurrentMediaTime();
   self.wakeupGeneration += 1;
   uint64_t generation = self.wakeupGeneration;
   [self.wakeupTimer invalidate];
   self.wakeupTimer = nil;
   if (isinf(deadline))
   {
      [self pauseDisplayLink];
      [self finishAt:now];
      return;
   }
   if (deadline <= now)
   {
      if (self.useMetalDisplayLink)
      {
         [self startDisplayLink];
         return;
      }
      BOOL wasPaused = YES;
      if (@available(macOS 15.0, *))
      {
         wasPaused = !self.displayLink || self.displayLink.paused;
      }
      [self startDisplayLink];
      if (wasPaused)
      {
         dispatch_async(dispatch_get_main_queue(), ^{
            if (generation == self.wakeupGeneration && !self.finishing)
            {
               [self runFrameAt:CACurrentMediaTime() target:CACurrentMediaTime()];
            }
         });
      }
      return;
   }
   [self pauseDisplayLink];
   __weak ComparisonHost *weakSelf = self;
   self.wakeupTimer = [NSTimer scheduledTimerWithTimeInterval:deadline - now repeats:NO
      block:^(NSTimer *timer) {
         (void)timer;
         ComparisonHost *host = weakSelf;
         if (!host || generation != host.wakeupGeneration || host.finishing)
         {
            return;
         }
         [host startDisplayLink];
         if (!host.useMetalDisplayLink)
         {
            [host runFrameAt:CACurrentMediaTime() target:CACurrentMediaTime()];
         }
      }];
}

- (void)startDisplayLink
{
   if (self.useMetalDisplayLink)
   {
      if (@available(macOS 14.0, *))
      {
         if (!self.metalDisplayLink)
         {
            CAMetalLayer *layer = (CAMetalLayer *)self.metalView.layer;
            self.metalDisplayLink = [[CAMetalDisplayLink alloc] initWithMetalLayer:layer];
            self.metalDisplayLink.delegate = self;
            // One frame is an explicit latency request, not a GPU completion wait.
            self.metalDisplayLink.preferredFrameLatency = 1.0f;
            NSScreen *screen = self.window.screen ?: NSScreen.mainScreen;
            float hz = (float)screen.maximumFramesPerSecond;
            if (hz > 0.0f)
            {
               self.metalDisplayLink.preferredFrameRateRange = CAFrameRateRangeMake(hz, hz, hz);
            }
            [self.metalDisplayLink addToRunLoop:NSRunLoop.mainRunLoop forMode:NSRunLoopCommonModes];
         }
         self.metalDisplayLink.paused = NO;
      }
      return;
   }
   if (@available(macOS 15.0, *))
   {
      if (!self.displayLink)
      {
         self.displayLink = [self.metalView displayLinkWithTarget:self selector:@selector(displayTick:)];
         NSScreen *screen = self.window.screen ?: NSScreen.mainScreen;
         float hz = (float)screen.maximumFramesPerSecond;
         if (hz > 0.0f)
         {
            self.displayLink.preferredFrameRateRange = CAFrameRateRangeMake(hz, hz, hz);
         }
         [self.displayLink addToRunLoop:NSRunLoop.mainRunLoop forMode:NSRunLoopCommonModes];
      }
      self.displayLink.paused = NO;
   }
}

- (void)pauseDisplayLink
{
   if (self.useMetalDisplayLink)
   {
      if (@available(macOS 14.0, *))
      {
         self.metalDisplayLink.paused = YES;
      }
      return;
   }
   if (@available(macOS 15.0, *))
   {
      self.displayLink.paused = YES;
   }
}

- (void)displayTick:(CADisplayLink *)link API_AVAILABLE(macos(15.0))
{
   [self runFrameAt:CACurrentMediaTime() target:link.targetTimestamp];
}

- (void)metalDisplayLink:(CAMetalDisplayLink *)link needsUpdate:(CAMetalDisplayLinkUpdate *)update API_AVAILABLE(macos(14.0))
{
   (void)link;
   [self runFrameAt:CACurrentMediaTime() target:update.targetPresentationTimestamp drawable:update.drawable];
}

- (void)runFrameAt:(double)now target:(double)target
{
   [self runFrameAt:now target:target drawable:nil];
}

- (void)runFrameAt:(double)now target:(double)target drawable:(id<CAMetalDrawable>)providedDrawable
{
   if (!self.initialized || self.finishing)
   {
      return;
   }
   [self sampleAt:now];
   if (comparison_finished(now))
   {
      [self finishAt:now];
      return;
   }
   uint64_t frame = comparison_begin_frame(now, target);
   if (frame != 0)
   {
      @autoreleasepool
      {
         id<CAMetalDrawable> drawable = providedDrawable;
         double acquireMs = 0.0;
         if (!drawable && !self.useMetalDisplayLink)
         {
            CAMetalLayer *layer = (CAMetalLayer *)self.metalView.layer;
            double acquireStart = CACurrentMediaTime();
            drawable = [layer nextDrawable];
            acquireMs = (CACurrentMediaTime() - acquireStart) * 1000.0;
         }
         if (!drawable)
         {
            comparison_draw(NULL, frame, acquireMs);
         }
         else
         {
            [drawable addPresentedHandler:^(id<MTLDrawable> presented) {
               comparison_presented(frame, presented.presentedTime);
            }];
            comparison_draw((__bridge void *)drawable, frame, acquireMs);
         }
      }
   }
   now = CACurrentMediaTime();
   if (comparison_finished(now))
   {
      [self finishAt:now];
      return;
   }
   [self scheduleFor:comparison_next_wakeup(now)];
}

- (void)sampleAt:(double)now
{
   if (self.lastSample != 0.0 && now - self.lastSample < kSampleInterval)
   {
      return;
   }
   self.lastSample = now;
   NSScreen *screen = self.window.screen ?: NSScreen.mainScreen;
   BOOL visible = self.window.visible &&
      (self.window.occlusionState & NSWindowOcclusionStateVisible) != 0;
   CGDirectDisplayID display = [screen.deviceDescription[@"NSScreenNumber"] unsignedIntValue];
   comparison_host_sample(now, CpuMicroseconds(), ResidentBytes(),
      (int32_t)NSProcessInfo.processInfo.thermalState, NSApp.active ? 1 : 0,
      visible ? 1 : 0, CGDisplayIsAsleep(display) ? 0 : 1,
      screen.maximumFramesPerSecond, screen.backingScaleFactor);
}

- (void)finishAt:(double)now
{
   if (self.finishing)
   {
      return;
   }
   self.finishing = YES;
   [NSWorkspace.sharedWorkspace.notificationCenter removeObserver:self
      name:NSWorkspaceScreensDidSleepNotification object:nil];
   self.wakeupGeneration += 1;
   [self pauseDisplayLink];
   [self.wakeupTimer invalidate];
   self.wakeupTimer = nil;
   self.lastSample = 0.0;
   [self sampleAt:now];
   dispatch_after(dispatch_time(DISPATCH_TIME_NOW,
      (int64_t)(kFinishDrainInterval * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
      comparison_finish(CACurrentMediaTime());
      [NSApp terminate:nil];
   });
}
@end

int comparison_main(void)
{
   @autoreleasepool
   {
      NSApplication *application = NSApplication.sharedApplication;
      [application setActivationPolicy:NSApplicationActivationPolicyRegular];
      // NSApplication and CAMetalDisplayLink both hold weak delegates. Keep the
      // host alive until the run loop exits, including under ARC optimization.
      __attribute__((objc_precise_lifetime)) ComparisonHost *host = [[ComparisonHost alloc] init];
      application.delegate = host;
      [application run];
   }
   return 0;
}
