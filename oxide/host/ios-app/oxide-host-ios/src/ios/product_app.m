#import <Foundation/Foundation.h>
#import <Metal/Metal.h>
#import <QuartzCore/CAMetalLayer.h>
#import <UIKit/UIKit.h>
#import <UserNotifications/UserNotifications.h>
#include <dispatch/dispatch.h>
#include <limits.h>
#include <math.h>
#include <stdatomic.h>
#include <stdint.h>

void oxide_host_emit_window_resized(float width, float height, float scale,
                                    float safe_left, float safe_top,
                                    float safe_right, float safe_bottom);
void oxide_host_emit_text_commit(const char *utf8, size_t len);
void oxide_host_emit_text_composition(uint32_t start, uint32_t end,
                                      const char *utf8, size_t len);
void oxide_host_emit_text_selection(uint32_t start, uint32_t end);
void oxide_host_emit_ime_shown(float x, float y, float width, float height);
void oxide_host_emit_ime_hidden(void);
void oxide_host_emit_touch(uint64_t id, uint32_t phase, float x, float y,
                           float pressure, uint8_t has_pressure,
                           float tilt_altitude, float tilt_azimuth,
                           uint8_t has_tilt, uint32_t device,
                           uint64_t timestamp_ns);
void oxide_host_emit_pointer(float x, float y, float dx, float dy,
                             uint32_t buttons, uint32_t modifiers,
                             uint64_t timestamp_ns);
void oxide_host_emit_key(uint32_t code, const char *chars, size_t chars_len,
                         uint8_t repeat, uint32_t modifiers,
                         uint64_t timestamp_ns);
int32_t oxide_host_app_init(uint32_t width, uint32_t height, float scale);
uint8_t oxide_host_app_should_render(void);
uint64_t oxide_host_app_wake_generation(void);
int32_t oxide_host_app_prepare_frame_timed(uint32_t width, uint32_t height,
                                           float scale, uint64_t timestamp_ns,
                                           uint64_t target_timestamp_ns);
int32_t oxide_host_app_submit_prepared_frame_with_drawable(void *drawable);
void oxide_host_app_cancel_prepared_frame(void);
void oxide_host_app_did_enter_background(void);
void oxide_host_app_will_enter_foreground(void);
void oxide_host_app_will_terminate(void);
void oxide_host_request_redraw(void);
void oxide_cam_set_preview_publish_callback(
    void (*callback)(uint64_t generation, uint64_t timestamp_ns));
void oxide_host_on_memory_warning(void);

void oxide_host_push_bootstrap(void);
void oxide_host_push_application_did_register(NSData *device_token);
void oxide_host_push_application_did_fail(NSError *error);
void oxide_host_push_application_did_receive(NSDictionary *user_info);

@class OxideProductSceneDelegate;

static __weak UIView *gMetalView = nil;
static __weak UITextView *gTextInput = nil;
static __weak OxideProductSceneDelegate *gActiveSceneDelegate = nil;
static __weak UISceneSession *gOwnedSceneSession = nil;
static BOOL gHostReady = NO;
static _Atomic(uint64_t) gWakeGeneration = 0;
static _Atomic(uint64_t) gThermalStateChanges = 0;
static _Atomic(uint64_t) gLowPowerModeChanges = 0;
static _Atomic(uint8_t) gWakeDispatchPending = 0;
static _Atomic(uint8_t) gHighRefreshEnabled = 1;
static _Atomic(uint32_t) gDisplayLinkRangeHz = 0;
static _Atomic(uint32_t) gPointerModifiers = 0;

static inline uint64_t seconds_to_ns(CFTimeInterval seconds) {
  if (!isfinite(seconds) || seconds <= 0.0) {
    return 0;
  }
  double nanoseconds = seconds * 1000000000.0;
  return nanoseconds >= (double)UINT64_MAX
             ? UINT64_MAX
             : (uint64_t)llround(nanoseconds);
}

static void dispatch_on_main(void (^block)(void)) {
  if (NSThread.isMainThread) {
    block();
  } else {
    dispatch_async(dispatch_get_main_queue(), block);
  }
}

static UIScreen *active_screen(void) {
  UIScreen *screen = gMetalView.window.windowScene.screen;
  if (screen != nil) {
    return screen;
  }
  for (UIScene *scene in UIApplication.sharedApplication.connectedScenes) {
    if ([scene isKindOfClass:UIWindowScene.class]) {
      return ((UIWindowScene *)scene).screen;
    }
  }
  return nil;
}

static uint32_t oxide_modifiers(UIKeyModifierFlags flags) {
  uint32_t modifiers = 0;
  if ((flags & UIKeyModifierShift) != 0) {
    modifiers |= 1u << 0;
  }
  if ((flags & UIKeyModifierControl) != 0) {
    modifiers |= 1u << 1;
  }
  if ((flags & UIKeyModifierAlternate) != 0) {
    modifiers |= 1u << 2;
  }
  if ((flags & UIKeyModifierCommand) != 0) {
    modifiers |= 1u << 3;
  }
  if ((flags & UIKeyModifierAlphaShift) != 0) {
    modifiers |= 1u << 4;
  }
  return modifiers;
}

static uint32_t pointer_buttons(UIEventButtonMask mask) {
  return (uint32_t)mask;
}

static CGFloat view_scale(UIView *view) {
  CGFloat scale = view.window.screen.nativeScale;
  if (scale > 0.0) {
    return scale;
  }
  scale = view.traitCollection.displayScale;
  return scale > 0.0 ? scale : 1.0;
}

static void emit_window_metrics(UIView *view) {
  if (view == nil || view.window == nil) {
    return;
  }
  UIEdgeInsets safe = view.safeAreaInsets;
  CGSize size = view.bounds.size;
  oxide_host_emit_window_resized(
      (float)size.width, (float)size.height, (float)view_scale(view),
      (float)safe.left, (float)safe.top, (float)safe.right,
      (float)safe.bottom);
}

static void ensure_host_initialized(UIView *view) {
  if (gHostReady || view == nil || view.window == nil) {
    return;
  }
  CAMetalLayer *layer = (CAMetalLayer *)view.layer;
  CGSize size = layer.drawableSize;
  if (size.width < 1.0 || size.height < 1.0) {
    return;
  }
  int32_t result = oxide_host_app_init(
      (uint32_t)llround(size.width), (uint32_t)llround(size.height),
      (float)view_scale(view));
  if (result == 0) {
    gHostReady = YES;
    emit_window_metrics(view);
  }
}

static void camera_preview_did_advance(uint64_t generation,
                                       uint64_t timestamp_ns) {
  (void)generation;
  (void)timestamp_ns;
  oxide_host_request_redraw();
}

@interface OxideProductMetalView : UIView
@property(nonatomic) CGPoint previousHoverPoint;
@property(nonatomic) BOOL hasPreviousHoverPoint;
@end

@implementation OxideProductMetalView

+ (Class)layerClass {
  return CAMetalLayer.class;
}

- (instancetype)initWithFrame:(CGRect)frame {
  self = [super initWithFrame:frame];
  if (self != nil) {
    self.multipleTouchEnabled = YES;
    self.isAccessibilityElement = NO;
    self.accessibilityElementsHidden = YES;
    self.opaque = YES;
    self.backgroundColor = UIColor.whiteColor;
    CAMetalLayer *layer = (CAMetalLayer *)self.layer;
    layer.device = MTLCreateSystemDefaultDevice();
    layer.pixelFormat = MTLPixelFormatBGRA8Unorm_sRGB;
    layer.framebufferOnly = YES;
    layer.presentsWithTransaction = NO;
    layer.allowsNextDrawableTimeout = YES;
    layer.maximumDrawableCount = 3;

    UIPanGestureRecognizer *pan =
        [[UIPanGestureRecognizer alloc] initWithTarget:self
                                               action:@selector(onPointerPan:)];
    pan.cancelsTouchesInView = NO;
    pan.allowedTouchTypes =
        @[ @(UITouchTypeIndirect), @(UITouchTypeIndirectPointer) ];
    pan.allowedScrollTypesMask = UIScrollTypeMaskAll;
    [self addGestureRecognizer:pan];

    UIHoverGestureRecognizer *hover =
        [[UIHoverGestureRecognizer alloc] initWithTarget:self
                                                  action:@selector(onHover:)];
    [self addGestureRecognizer:hover];
  }
  return self;
}

- (instancetype)init {
  return [self initWithFrame:CGRectZero];
}

- (void)layoutSubviews {
  [super layoutSubviews];
  CAMetalLayer *layer = (CAMetalLayer *)self.layer;
  CGFloat scale = view_scale(self);
  layer.contentsScale = scale;
  layer.drawableSize = CGSizeMake(self.bounds.size.width * scale,
                                  self.bounds.size.height * scale);
  emit_window_metrics(self);
}

- (void)onPointerPan:(UIPanGestureRecognizer *)recognizer {
  CGPoint point = [recognizer locationInView:self];
  CGPoint delta = [recognizer translationInView:self];
  if (!isfinite(point.x) || !isfinite(point.y) || !isfinite(delta.x) ||
      !isfinite(delta.y)) {
    return;
  }
  uint32_t buttons = pointer_buttons(recognizer.buttonMask);
  uint32_t modifiers =
      atomic_load_explicit(&gPointerModifiers, memory_order_acquire);
  oxide_host_emit_pointer((float)point.x, (float)point.y, (float)delta.x,
                          (float)delta.y, buttons, modifiers,
                          seconds_to_ns(CACurrentMediaTime()));
  [recognizer setTranslation:CGPointZero inView:self];
}

- (void)onHover:(UIHoverGestureRecognizer *)recognizer {
  CGPoint point = [recognizer locationInView:self];
  CGPoint delta = CGPointZero;
  if (self.hasPreviousHoverPoint) {
    delta.x = point.x - self.previousHoverPoint.x;
    delta.y = point.y - self.previousHoverPoint.y;
  }
  self.previousHoverPoint = point;
  self.hasPreviousHoverPoint = recognizer.state != UIGestureRecognizerStateEnded &&
                               recognizer.state != UIGestureRecognizerStateCancelled;
  uint32_t modifiers =
      atomic_load_explicit(&gPointerModifiers, memory_order_acquire);
  oxide_host_emit_pointer((float)point.x, (float)point.y, (float)delta.x,
                          (float)delta.y, 0, modifiers,
                          seconds_to_ns(CACurrentMediaTime()));
}

@end

@interface OxideProductTextInput : UITextView <UITextViewDelegate>
@property(nonatomic) BOOL resettingText;
@property(nonatomic) BOOL composingText;
@end

@implementation OxideProductTextInput

- (instancetype)init {
  self = [super initWithFrame:CGRectMake(-1.0, -1.0, 1.0, 1.0)
                textContainer:nil];
  if (self != nil) {
    self.delegate = self;
    self.isAccessibilityElement = NO;
    self.accessibilityElementsHidden = YES;
    self.backgroundColor = UIColor.clearColor;
    self.textColor = UIColor.clearColor;
    self.tintColor = UIColor.clearColor;
    self.scrollEnabled = NO;
    self.alpha = 0.01;
  }
  return self;
}

- (BOOL)textView:(UITextView *)text_view
    shouldChangeTextInRange:(NSRange)range
            replacementText:(NSString *)replacement {
  (void)text_view;
  (void)range;
  if (replacement.length == 0) {
    const char backspace = '\b';
    oxide_host_emit_text_commit(&backspace, 1);
    return NO;
  }
  return YES;
}

- (void)textViewDidChange:(UITextView *)text_view {
  if (self.resettingText) {
    return;
  }
  UITextRange *marked = text_view.markedTextRange;
  if (marked != nil) {
    NSInteger start = [text_view offsetFromPosition:text_view.beginningOfDocument
                                         toPosition:marked.start];
    NSInteger end = [text_view offsetFromPosition:text_view.beginningOfDocument
                                       toPosition:marked.end];
    NSString *text = [text_view textInRange:marked] ?: @"";
    NSData *bytes = [text dataUsingEncoding:NSUTF8StringEncoding];
    oxide_host_emit_text_composition(
        (uint32_t)MAX(start, 0), (uint32_t)MAX(end, 0), bytes.bytes,
        bytes.length);
    self.composingText = YES;
    return;
  }

  if (self.composingText) {
    oxide_host_emit_text_composition(0, 0, NULL, 0);
    self.composingText = NO;
  }
  NSString *text = text_view.text ?: @"";
  NSData *bytes = [text dataUsingEncoding:NSUTF8StringEncoding];
  if (bytes.length > 0) {
    oxide_host_emit_text_commit(bytes.bytes, bytes.length);
  }
  self.resettingText = YES;
  text_view.text = @"";
  text_view.selectedRange = NSMakeRange(0, 0);
  self.resettingText = NO;
}

- (void)textViewDidChangeSelection:(UITextView *)text_view {
  if (self.resettingText) {
    return;
  }
  NSRange selection = text_view.selectedRange;
  NSUInteger end = selection.location == NSNotFound
                       ? 0
                       : selection.location + selection.length;
  oxide_host_emit_text_selection(
      selection.location == NSNotFound ? 0 : (uint32_t)selection.location,
      (uint32_t)end);
}

@end

@interface OxideTouchWindow : UIWindow
@property(nonatomic, strong)
    NSMutableDictionary<NSValue *, NSNumber *> *touchIds;
@property(nonatomic) uint64_t nextTouchId;
@end

@implementation OxideTouchWindow

- (instancetype)initWithWindowScene:(UIWindowScene *)window_scene {
  self = [super initWithWindowScene:window_scene];
  if (self != nil) {
    self.touchIds = [NSMutableDictionary dictionary];
    self.nextTouchId = 1;
  }
  return self;
}

- (NSNumber *)identifierForTouch:(UITouch *)touch {
  NSValue *key = [NSValue valueWithNonretainedObject:touch];
  NSNumber *identifier = self.touchIds[key];
  if (identifier == nil) {
    identifier = @(self.nextTouchId++);
    self.touchIds[key] = identifier;
  }
  return identifier;
}

- (void)emitTouch:(UITouch *)touch inView:(UIView *)view {
  uint32_t phase = 0;
  switch (touch.phase) {
  case UITouchPhaseBegan:
    phase = 0;
    break;
  case UITouchPhaseMoved:
    phase = 1;
    break;
  case UITouchPhaseEnded:
    phase = 2;
    break;
  case UITouchPhaseCancelled:
    phase = 3;
    break;
  case UITouchPhaseStationary:
    return;
  default:
    return;
  }

  NSValue *key = [NSValue valueWithNonretainedObject:touch];
  uint64_t identifier = [self identifierForTouch:touch].unsignedLongLongValue;
  CGPoint point = [touch locationInView:view];
  float pressure = 0.0f;
  uint8_t has_pressure = 0;
  if (touch.maximumPossibleForce > 0.0f) {
    pressure = (float)(touch.force / touch.maximumPossibleForce);
    has_pressure = 1;
  }
  float altitude = 0.0f;
  float azimuth = 0.0f;
  uint8_t has_tilt = 0;
  uint32_t device = 0;
  if (touch.type == UITouchTypePencil) {
    device = 1;
    altitude = (float)touch.altitudeAngle;
    azimuth = (float)[touch azimuthAngleInView:view];
    has_tilt = 1;
  } else if (touch.type == UITouchTypeIndirectPointer) {
    device = 2;
  }
  oxide_host_emit_touch(identifier, phase, (float)point.x, (float)point.y,
                        pressure, has_pressure, altitude, azimuth, has_tilt,
                        device, seconds_to_ns(touch.timestamp));
  if (phase == 2 || phase == 3) {
    [self.touchIds removeObjectForKey:key];
  }
}

- (void)emitKeyPress:(UIPress *)press {
  UIKey *key = press.key;
  if (key == nil) {
    return;
  }
  uint8_t repeat = 0;
  switch (press.phase) {
  case UIPressPhaseBegan:
    repeat = 0;
    break;
  case UIPressPhaseChanged:
  case UIPressPhaseStationary:
    repeat = 1;
    break;
  case UIPressPhaseEnded:
  case UIPressPhaseCancelled:
    return;
  }
  NSData *characters =
      [key.characters dataUsingEncoding:NSUTF8StringEncoding];
  oxide_host_emit_key((uint32_t)key.keyCode, characters.bytes,
                      characters.length, repeat,
                      oxide_modifiers(key.modifierFlags),
                      seconds_to_ns(press.timestamp));
}

- (void)sendEvent:(UIEvent *)event {
  atomic_store_explicit(&gPointerModifiers,
                        oxide_modifiers(event.modifierFlags),
                        memory_order_release);
  UIView *metal_view = gMetalView;
  if (event.type == UIEventTypeTouches && metal_view != nil) {
    for (UITouch *touch in event.allTouches) {
      [self emitTouch:touch inView:metal_view];
    }
  } else if (event.type == UIEventTypePresses &&
             [event isKindOfClass:UIPressesEvent.class]) {
    for (UIPress *press in ((UIPressesEvent *)event).allPresses) {
      [self emitKeyPress:press];
    }
  }
  [super sendEvent:event];
}

@end

@interface OxideProductSceneDelegate : UIResponder <UIWindowSceneDelegate>
@property(nonatomic, strong) UIWindow *window;
@property(nonatomic, strong) CADisplayLink *displayLink;
@property(nonatomic) BOOL foregroundActive;
- (void)requestDisplayLinkWake:(uint64_t)generation;
- (void)updateDisplayLinkRange;
@end

@implementation OxideProductSceneDelegate

- (void)updateDisplayLinkRange {
  if (self.displayLink == nil) {
    atomic_store_explicit(&gDisplayLinkRangeHz, 0, memory_order_release);
    return;
  }
  UIScreen *screen = active_screen();
  NSInteger maximum = screen != nil ? MAX(screen.maximumFramesPerSecond, 1)
                                    : 60;
  NSInteger target =
      atomic_load_explicit(&gHighRefreshEnabled, memory_order_acquire) != 0
          ? maximum
          : MIN(maximum, 60);
  self.displayLink.preferredFrameRateRange =
      CAFrameRateRangeMake((float)target, (float)target, (float)target);
  atomic_store_explicit(&gDisplayLinkRangeHz, (uint32_t)target,
                        memory_order_release);
}

- (void)requestDisplayLinkWake:(uint64_t)generation {
  (void)generation;
  if (self.foregroundActive && self.displayLink != nil) {
    self.displayLink.paused = NO;
  }
}

- (void)updateDisplayLinkState {
  if (self.displayLink == nil) {
    return;
  }
  if (!self.foregroundActive) {
    self.displayLink.paused = YES;
    return;
  }
  if (oxide_host_app_should_render() != 0) {
    self.displayLink.paused = NO;
    return;
  }
  uint64_t generation = oxide_host_app_wake_generation();
  self.displayLink.paused = YES;
  uint64_t latest =
      atomic_load_explicit(&gWakeGeneration, memory_order_acquire);
  if (latest > generation) {
    [self requestDisplayLinkWake:latest];
  }
}

- (void)scene:(UIScene *)scene
    willConnectToSession:(UISceneSession *)session
                 options:(UISceneConnectionOptions *)connection_options {
  (void)session;
  (void)connection_options;
  if (![scene isKindOfClass:UIWindowScene.class]) {
    return;
  }
  if (gOwnedSceneSession != nil && gOwnedSceneSession != session) {
    [UIApplication.sharedApplication
        requestSceneSessionDestruction:session
                              options:nil
                         errorHandler:^(NSError *error) {
                           (void)error;
                         }];
    return;
  }
  gOwnedSceneSession = session;
  UIWindowScene *window_scene = (UIWindowScene *)scene;
  OxideTouchWindow *window =
      [[OxideTouchWindow alloc] initWithWindowScene:window_scene];
  UIViewController *controller = [UIViewController new];
  OxideProductMetalView *metal_view = [OxideProductMetalView new];
  controller.view = metal_view;
  window.rootViewController = controller;
  self.window = window;
  gMetalView = metal_view;

  OxideProductTextInput *text_input = [OxideProductTextInput new];
  [metal_view addSubview:text_input];
  gTextInput = text_input;
  [window makeKeyAndVisible];
  [metal_view setNeedsLayout];
  [metal_view layoutIfNeeded];

  self.displayLink = [CADisplayLink displayLinkWithTarget:self
                                                 selector:@selector(onTick:)];
  [self updateDisplayLinkRange];
  [self.displayLink addToRunLoop:NSRunLoop.mainRunLoop
                         forMode:NSRunLoopCommonModes];
  self.foregroundActive = YES;
  ensure_host_initialized(metal_view);
  [self updateDisplayLinkState];

  NSNotificationCenter *notifications = NSNotificationCenter.defaultCenter;
  [notifications addObserver:self
                     selector:@selector(keyboardWillShow:)
                         name:UIKeyboardWillShowNotification
                       object:nil];
  [notifications addObserver:self
                     selector:@selector(keyboardWillHide:)
                         name:UIKeyboardWillHideNotification
                       object:nil];
}

- (void)onTick:(CADisplayLink *)link {
  if (!self.foregroundActive) {
    link.paused = YES;
    return;
  }
  UIView *view = gMetalView;
  ensure_host_initialized(view);
  if (!gHostReady || oxide_host_app_should_render() == 0) {
    [self updateDisplayLinkState];
    return;
  }
  CAMetalLayer *layer = (CAMetalLayer *)view.layer;
  CGSize size = layer.drawableSize;
  int32_t prepared = oxide_host_app_prepare_frame_timed(
      (uint32_t)llround(size.width), (uint32_t)llround(size.height),
      (float)view_scale(view), seconds_to_ns(link.timestamp),
      seconds_to_ns(link.targetTimestamp));
  if (prepared != 0) {
    [self updateDisplayLinkState];
    return;
  }
  id<CAMetalDrawable> drawable = [layer nextDrawable];
  if (drawable == nil) {
    oxide_host_app_cancel_prepared_frame();
    [self updateDisplayLinkState];
    return;
  }
  (void)oxide_host_app_submit_prepared_frame_with_drawable(
      (__bridge void *)drawable);
  [self updateDisplayLinkState];
}

- (void)keyboardWillShow:(NSNotification *)notification {
  NSValue *value = notification.userInfo[UIKeyboardFrameEndUserInfoKey];
  CGRect frame = value != nil ? value.CGRectValue : CGRectZero;
  UIView *view = gMetalView;
  if (view != nil && view.window != nil) {
    frame = [view convertRect:frame fromView:nil];
  }
  oxide_host_emit_ime_shown((float)frame.origin.x, (float)frame.origin.y,
                            (float)frame.size.width,
                            (float)frame.size.height);
}

- (void)keyboardWillHide:(NSNotification *)notification {
  (void)notification;
  oxide_host_emit_ime_hidden();
}

- (void)sceneDidBecomeActive:(UIScene *)scene {
  (void)scene;
  gActiveSceneDelegate = self;
  oxide_cam_set_preview_publish_callback(camera_preview_did_advance);
  self.foregroundActive = YES;
  ensure_host_initialized(gMetalView);
  [self updateDisplayLinkRange];
  [self requestDisplayLinkWake:oxide_host_app_wake_generation()];
}

- (void)sceneWillResignActive:(UIScene *)scene {
  (void)scene;
  if (gActiveSceneDelegate == self) {
    oxide_cam_set_preview_publish_callback(NULL);
  }
  self.foregroundActive = NO;
  self.displayLink.paused = YES;
}

- (void)sceneDidEnterBackground:(UIScene *)scene {
  (void)scene;
  if (gActiveSceneDelegate == self) {
    oxide_cam_set_preview_publish_callback(NULL);
  }
  self.foregroundActive = NO;
  self.displayLink.paused = YES;
  oxide_host_app_did_enter_background();
}

- (void)sceneWillEnterForeground:(UIScene *)scene {
  (void)scene;
  self.foregroundActive = YES;
  oxide_host_app_will_enter_foreground();
  [self updateDisplayLinkRange];
  [self requestDisplayLinkWake:oxide_host_app_wake_generation()];
}

- (void)sceneDidDisconnect:(UIScene *)scene {
  (void)scene;
  [self.displayLink invalidate];
  self.displayLink = nil;
  [NSNotificationCenter.defaultCenter removeObserver:self];
  if (gActiveSceneDelegate == self) {
    oxide_cam_set_preview_publish_callback(NULL);
    gActiveSceneDelegate = nil;
  }
  if (gOwnedSceneSession == scene.session) {
    gOwnedSceneSession = nil;
    gMetalView = nil;
    gTextInput = nil;
    gHostReady = NO;
    atomic_store_explicit(&gDisplayLinkRangeHz, 0, memory_order_release);
  }
}

@end

void oxide_host_request_display_link_wake(uint64_t generation) {
  uint64_t current =
      atomic_load_explicit(&gWakeGeneration, memory_order_acquire);
  while (current < generation &&
         !atomic_compare_exchange_weak_explicit(
             &gWakeGeneration, &current, generation, memory_order_acq_rel,
             memory_order_acquire)) {
  }
  if (NSThread.isMainThread) {
    [gActiveSceneDelegate requestDisplayLinkWake:generation];
    return;
  }
  if (atomic_exchange_explicit(&gWakeDispatchPending, 1,
                               memory_order_acq_rel) != 0) {
    return;
  }
  dispatch_async(dispatch_get_main_queue(), ^{
    atomic_store_explicit(&gWakeDispatchPending, 0, memory_order_release);
    uint64_t latest =
        atomic_load_explicit(&gWakeGeneration, memory_order_acquire);
    [gActiveSceneDelegate requestDisplayLinkWake:latest];
  });
}

void oxide_host_set_high_refresh(uint8_t enable) {
  atomic_store_explicit(&gHighRefreshEnabled, enable != 0,
                        memory_order_release);
  dispatch_on_main(^{
    [gActiveSceneDelegate updateDisplayLinkRange];
  });
}

void oxide_host_ime_show(void) {
  dispatch_on_main(^{
    [gTextInput becomeFirstResponder];
  });
}

void oxide_host_ime_hide(void) {
  dispatch_on_main(^{
    [gTextInput resignFirstResponder];
  });
}

int32_t oxide_host_display_link_frame_rate_range(float *minimum,
                                                 float *maximum,
                                                 float *preferred) {
  if (minimum == NULL || maximum == NULL || preferred == NULL) {
    return 0;
  }
  *minimum = 0.0f;
  *maximum = 0.0f;
  *preferred = 0.0f;
  uint32_t hertz =
      atomic_load_explicit(&gDisplayLinkRangeHz, memory_order_acquire);
  if (hertz == 0) {
    return 0;
  }
  *minimum = (float)hertz;
  *maximum = (float)hertz;
  *preferred = (float)hertz;
  return 1;
}

void oxide_host_environment_transition_counts(uint64_t *thermal,
                                               uint64_t *low_power) {
  if (thermal != NULL) {
    *thermal = atomic_load_explicit(&gThermalStateChanges,
                                    memory_order_acquire);
  }
  if (low_power != NULL) {
    *low_power = atomic_load_explicit(&gLowPowerModeChanges,
                                      memory_order_acquire);
  }
}

@interface OxideProductAppDelegate : UIResponder <UIApplicationDelegate>
@end

@implementation OxideProductAppDelegate

- (BOOL)application:(UIApplication *)application
    didFinishLaunchingWithOptions:(NSDictionary *)launch_options {
  (void)application;
  (void)launch_options;
  NSNotificationCenter *notifications = NSNotificationCenter.defaultCenter;
  NSProcessInfo *process_info = NSProcessInfo.processInfo;
  [notifications addObserver:self
                     selector:@selector(thermalStateDidChange:)
                         name:NSProcessInfoThermalStateDidChangeNotification
                       object:process_info];
  [notifications addObserver:self
                     selector:@selector(lowPowerModeDidChange:)
                         name:NSProcessInfoPowerStateDidChangeNotification
                       object:process_info];
  oxide_host_push_bootstrap();
  return YES;
}

- (void)thermalStateDidChange:(NSNotification *)notification {
  (void)notification;
  atomic_fetch_add_explicit(&gThermalStateChanges, 1, memory_order_release);
}

- (void)lowPowerModeDidChange:(NSNotification *)notification {
  (void)notification;
  atomic_fetch_add_explicit(&gLowPowerModeChanges, 1, memory_order_release);
}

- (UISceneConfiguration *)application:(UIApplication *)application
    configurationForConnectingSceneSession:
        (UISceneSession *)connecting_session
                                   options:(UISceneConnectionOptions *)options {
  (void)application;
  (void)options;
  UISceneConfiguration *configuration =
      [UISceneConfiguration configurationWithName:@"OxideProduct"
                                      sessionRole:connecting_session.role];
  configuration.delegateClass = OxideProductSceneDelegate.class;
  return configuration;
}

- (void)applicationDidReceiveMemoryWarning:(UIApplication *)application {
  (void)application;
  oxide_host_on_memory_warning();
}

- (void)applicationWillTerminate:(UIApplication *)application {
  (void)application;
  atomic_store_explicit(&gDisplayLinkRangeHz, 0, memory_order_release);
  oxide_cam_set_preview_publish_callback(NULL);
  oxide_host_app_will_terminate();
}

- (void)application:(UIApplication *)application
    didRegisterForRemoteNotificationsWithDeviceToken:(NSData *)device_token {
  (void)application;
  oxide_host_push_application_did_register(device_token);
}

- (void)application:(UIApplication *)application
    didFailToRegisterForRemoteNotificationsWithError:(NSError *)error {
  (void)application;
  oxide_host_push_application_did_fail(error);
}

- (void)application:(UIApplication *)application
    didReceiveRemoteNotification:(NSDictionary *)user_info
          fetchCompletionHandler:
              (void (^)(UIBackgroundFetchResult result))completion_handler {
  (void)application;
  oxide_host_push_application_did_receive(user_info);
  if (completion_handler != nil) {
    completion_handler(UIBackgroundFetchResultNoData);
  }
}

@end

int32_t oxide_host_start(int argc, char **argv) {
  @autoreleasepool {
    char *fallback_argv[] = {(char *)"oxide-host", NULL};
    int launch_argc = argc > 0 && argv != NULL ? argc : 1;
    char **launch_argv = argc > 0 && argv != NULL ? argv : fallback_argv;
    return UIApplicationMain(launch_argc, launch_argv, nil,
                             NSStringFromClass(OxideProductAppDelegate.class));
  }
}

void oxide_host_ios_log(const char *utf8, size_t len) {
  if (utf8 == NULL || len == 0) {
    return;
  }
  NSString *message = [[NSString alloc] initWithBytes:utf8
                                              length:len
                                            encoding:NSUTF8StringEncoding];
  if (message != nil) {
    NSLog(@"[Oxide] %@", message);
  }
}
