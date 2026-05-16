#import <Foundation/Foundation.h>
#import <AppKit/AppKit.h>
#import <QuartzCore/QuartzCore.h>
#import <CoreVideo/CoreVideo.h>
#import <Metal/Metal.h>
#import <dispatch/dispatch.h>
#import <math.h>

static __weak NSView *gMetalView = nil;
static CVDisplayLinkRef gDisplayLinkLegacy = NULL;
static CADisplayLink *gDisplayLink = nil;
static BOOL gHighRefresh = YES;
static int gDeviceHz = 60;
static int gTargetHz = 60;
static int gDropEvery = 1;
static uint64_t gFrameCounter = 0;
static CFTimeInterval gLastSample = 0.0;
static unsigned gFpsCount = 0;
static CATextLayer *gFpsLayer = nil;

static BOOL ShouldRenderFrame(void);
static void DispatchFrameTick(void);

void macos_app_did_become_active(void);
void macos_app_will_resign_active(void);
void macos_app_will_terminate(void);
void macos_app_on_memory_pressure(uint32_t level);
uint8_t macos_app_should_render(void);

@interface MetalView : NSView <NSTextInputClient> {
    id<MTLDevice> _device;
    uint64_t _nextTouchId;
    uint64_t _activeTouchId;
    NSPoint _lastPointer;
    NSMutableAttributedString *_marked;
    NSRange _selected;
}
@end

@implementation MetalView
+ (Class)layerClass { return [CAMetalLayer class]; }
- (instancetype)initWithFrame:(NSRect)frameRect
{
    self = [super initWithFrame:frameRect];
    if (self) {
        [self setWantsLayer:YES];
        CAMetalLayer *layer = (CAMetalLayer *)self.layer;
        _device = MTLCreateSystemDefaultDevice();
        layer.device = _device;
        layer.pixelFormat = MTLPixelFormatBGRA8Unorm_sRGB;
        layer.framebufferOnly = YES;
        layer.presentsWithTransaction = NO;
        layer.maximumDrawableCount = 3;
        layer.allowsNextDrawableTimeout = NO;
        CGFloat scale = NSScreen.mainScreen.backingScaleFactor;
        layer.contentsScale = scale;
        _marked = [[NSMutableAttributedString alloc] initWithString:@""];
        _selected = NSMakeRange(0, 0);
        _nextTouchId = 1; _activeTouchId = 0; _lastPointer = NSMakePoint(0,0);
        NSTrackingAreaOptions opts = NSTrackingMouseMoved | NSTrackingActiveInKeyWindow | NSTrackingInVisibleRect;
        NSTrackingArea *ta = [[NSTrackingArea alloc] initWithRect:self.bounds options:opts owner:self userInfo:nil];
        [self addTrackingArea:ta];
    }
    return self;
}
- (void)drawRect:(NSRect)dirtyRect
{
    (void)dirtyRect;
    CAMetalLayer *layer = (CAMetalLayer *)self.layer;
    // Late acquire
    id<CAMetalDrawable> drawable = [layer nextDrawable];
    if (!drawable) { return; }
    CGSize ds = layer.drawableSize;
    CGFloat scale = layer.contentsScale;
    extern int32_t macos_app_init(uint32_t w, uint32_t h, float scale);
    extern int32_t macos_app_frame_with_drawable(uint32_t w, uint32_t h, float scale,
                                                 void *drawable_ptr);
    static BOOL sInited = NO;
    if (!sInited) { macos_app_init((uint32_t)ds.width, (uint32_t)ds.height, (float)scale); sInited = YES; }
    macos_app_frame_with_drawable((uint32_t)ds.width, (uint32_t)ds.height, (float)scale,
                                  (__bridge void*)drawable);
}

- (BOOL)acceptsFirstResponder { return YES; }
- (BOOL)becomeFirstResponder { return YES; }
- (BOOL)resignFirstResponder { return YES; }

// --- Input mapping
static inline uint64_t ts_now_ns(void) { return (uint64_t)(CACurrentMediaTime() * 1000000000.0); }
static inline uint32_t map_mods(NSEventModifierFlags f) {
    uint32_t m = 0;
    if (f & NSEventModifierFlagShift) m |= 1u << 0;
    if (f & NSEventModifierFlagControl) m |= 1u << 1;
    if (f & NSEventModifierFlagOption) m |= 1u << 2;
    if (f & NSEventModifierFlagCommand) m |= 1u << 3;
    if (f & NSEventModifierFlagCapsLock) m |= 1u << 4;
    return m;
}
extern void macos_emit_touch(uint64_t id, uint32_t phase, float x, float y, uint64_t ts);
extern void macos_emit_pointer(float x, float y, float dx, float dy, uint32_t buttons, uint32_t modifiers, uint64_t ts);
extern void macos_emit_key(uint32_t code, const uint8_t *chars, size_t len, uint8_t repeat, uint32_t modifiers, uint64_t ts);
extern void macos_emit_text_commit(const uint8_t *utf8, size_t len);
extern void macos_emit_text_composition(uint32_t start, uint32_t end, const uint8_t *utf8, size_t len);
extern void macos_emit_text_selection(uint32_t start, uint32_t end);
extern void macos_emit_pinch(float cx, float cy, float delta, uint64_t ts);
extern void macos_emit_rotate(float cx, float cy, float radians, uint64_t ts);

- (void)mouseDown:(NSEvent *)event
{
    _activeTouchId = _nextTouchId++;
    NSPoint p = [self convertPoint:event.locationInWindow fromView:nil];
    _lastPointer = p;
    macos_emit_touch(_activeTouchId, 0, p.x, p.y, ts_now_ns());
}
- (void)mouseDragged:(NSEvent *)event
{
    if (_activeTouchId == 0) return;
    NSPoint p = [self convertPoint:event.locationInWindow fromView:nil];
    macos_emit_touch(_activeTouchId, 1, p.x, p.y, ts_now_ns());
}
- (void)mouseUp:(NSEvent *)event
{
    if (_activeTouchId == 0) return;
    NSPoint p = [self convertPoint:event.locationInWindow fromView:nil];
    macos_emit_touch(_activeTouchId, 2, p.x, p.y, ts_now_ns());
    _activeTouchId = 0;
}
- (void)mouseMoved:(NSEvent *)event
{
    NSPoint p = [self convertPoint:event.locationInWindow fromView:nil];
    float dx = p.x - _lastPointer.x; float dy = p.y - _lastPointer.y;
    _lastPointer = p;
    macos_emit_pointer(p.x, p.y, dx, dy, (uint32_t)[NSEvent pressedMouseButtons], map_mods(event.modifierFlags), ts_now_ns());
}
- (void)scrollWheel:(NSEvent *)event
{
    NSPoint p = [self convertPoint:event.locationInWindow fromView:nil];
    float dx = (float)event.scrollingDeltaX; float dy = (float)event.scrollingDeltaY;
    macos_emit_pointer(p.x, p.y, dx, dy, (uint32_t)[NSEvent pressedMouseButtons], map_mods(event.modifierFlags), ts_now_ns());
}
- (void)magnifyWithEvent:(NSEvent *)event
{
    NSPoint p = [self convertPoint:event.locationInWindow fromView:nil];
    macos_emit_pinch(p.x, p.y, (float)event.magnification, ts_now_ns());
}
- (void)rotateWithEvent:(NSEvent *)event
{
    NSPoint p = [self convertPoint:event.locationInWindow fromView:nil];
    macos_emit_rotate(p.x, p.y, (float)event.rotation, ts_now_ns());
}
- (void)keyDown:(NSEvent *)event
{
    NSString *cs = event.charactersIgnoringModifiers ?: @"";
    NSData *d = [cs dataUsingEncoding:NSUTF8StringEncoding];
    macos_emit_key((uint32_t)event.keyCode, d.bytes, d.length, event.isARepeat ? 1 : 0, map_mods(event.modifierFlags), ts_now_ns());
}
- (void)keyUp:(NSEvent *)event
{
    NSString *cs = event.charactersIgnoringModifiers ?: @"";
    NSData *d = [cs dataUsingEncoding:NSUTF8StringEncoding];
    // Use repeat=2 to indicate key-up to the Rust handler
    macos_emit_key((uint32_t)event.keyCode, d.bytes, d.length, 2, map_mods(event.modifierFlags), ts_now_ns());
}

// --- NSTextInputClient ---
- (BOOL)hasMarkedText { return _marked.length > 0; }
- (NSRange)markedRange { return NSMakeRange(0, (NSUInteger)_marked.length); }
- (NSRange)selectedRange { return _selected; }
- (void)setMarkedText:(id)string selectedRange:(NSRange)selectedRange replacementRange:(NSRange)replacementRange
{
    (void)replacementRange;
    NSString *s = [string isKindOfClass:[NSAttributedString class]] ? [string string] : (NSString*)string;
    _marked = [[NSMutableAttributedString alloc] initWithString:(s ?: @"")];
    _selected = selectedRange;
    NSData *d = [(s ?: @"") dataUsingEncoding:NSUTF8StringEncoding];
    macos_emit_text_composition(0, (uint32_t)(s.length), (const uint8_t*)d.bytes, d.length);
}
- (void)unmarkText
{
    if (_marked.length > 0) {
        NSData *d = [[_marked string] dataUsingEncoding:NSUTF8StringEncoding];
        macos_emit_text_commit((const uint8_t*)d.bytes, d.length);
        _marked = [[NSMutableAttributedString alloc] initWithString:@""];
        _selected = NSMakeRange(0, 0);
    }
}
- (void)insertText:(id)string replacementRange:(NSRange)replacementRange
{
    (void)replacementRange;
    NSString *s = [string isKindOfClass:[NSAttributedString class]] ? [string string] : (NSString*)string;
    NSData *d = [(s ?: @"") dataUsingEncoding:NSUTF8StringEncoding];
    macos_emit_text_commit((const uint8_t*)d.bytes, d.length);
}
- (NSArray<NSAttributedStringKey> *)validAttributesForMarkedText { return @[]; }
- (NSAttributedString *)attributedSubstringForProposedRange:(NSRange)range actualRange:(NSRangePointer)actualRange
{ if (actualRange) *actualRange = NSMakeRange(NSNotFound, 0); return [[NSAttributedString alloc] initWithString:@""]; }
- (NSUInteger)characterIndexForPoint:(NSPoint)point { (void)point; return 0; }
- (NSRect)firstRectForCharacterRange:(NSRange)range actualRange:(NSRangePointer)actualRange
{ (void)range; if (actualRange) *actualRange = NSMakeRange(NSNotFound, 0); NSRect r = [self bounds]; NSRect f = NSMakeRect(8, r.size.height - 24, 1, 16); NSRect w = [self convertRect:f toView:nil]; return [self.window convertRectToScreen:w]; }
- (void)doCommandBySelector:(SEL)selector { (void)selector; }
- (void)setSelectedRange:(NSRange)selectedRange { _selected = selectedRange; macos_emit_text_selection((uint32_t)selectedRange.location, (uint32_t)(selectedRange.location + selectedRange.length)); }
- (NSInteger)conversationIdentifier { return 1; }
- (void)layout
{
    [super layout];
    CAMetalLayer *layer = (CAMetalLayer *)self.layer;
    CGFloat scale = NSScreen.mainScreen.backingScaleFactor;
    layer.contentsScale = scale;
    CGSize s = self.bounds.size;
    layer.drawableSize = (CGSize){ s.width * scale, s.height * scale };
}
@end

@interface AppDelegate : NSObject <NSApplicationDelegate>
@property (strong) NSWindow *window;
@property (nonatomic) dispatch_source_t memorySource;
@end

@implementation AppDelegate
- (void)applicationDidFinishLaunching:(NSNotification *)notification
{
    (void)notification;
    NSRect frame = NSMakeRect(100, 100, 800, 600);
    self.window = [[NSWindow alloc] initWithContentRect:frame
                                              styleMask:(NSWindowStyleMaskTitled | NSWindowStyleMaskClosable | NSWindowStyleMaskResizable)
                                                backing:NSBackingStoreBuffered
                                                  defer:NO];
    [self.window setTitle:@"Oxide macOS Test Host"];
    MetalView *view = [[MetalView alloc] initWithFrame:self.window.contentView.bounds];
    view.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    self.window.contentView = view;
    gMetalView = view;
    [self.window makeFirstResponder:view];
    [self.window setAcceptsMouseMovedEvents:YES];
    // FPS overlay layer
    gFpsLayer = [CATextLayer layer];
    gFpsLayer.contentsScale = NSScreen.mainScreen.backingScaleFactor;
    gFpsLayer.fontSize = 12.0;
    gFpsLayer.foregroundColor = [NSColor colorWithCalibratedWhite:0.1 alpha:0.95].CGColor;
    gFpsLayer.backgroundColor = [NSColor colorWithCalibratedWhite:1.0 alpha:0.75].CGColor;
    gFpsLayer.cornerRadius = 4.0; gFpsLayer.masksToBounds = YES;
    gFpsLayer.alignmentMode = kCAAlignmentLeft;
    gFpsLayer.frame = CGRectMake(8, view.bounds.size.height - 24, 80, 16);
    [view.layer addSublayer:gFpsLayer];
    [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(onWindowResize:) name:NSWindowDidResizeNotification object:self.window];
    [self.window makeKeyAndOrderFront:nil];
    [NSApp activateIgnoringOtherApps:YES];
    NSLog(@"[Oxide] App launched; window ready");
    // Power state change for LPM handling
    [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(onPowerStateChanged:) name:NSProcessInfoPowerStateDidChangeNotification object:nil];
    [self startDisplayLink];
    [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(onAppDidBecomeActive:) name:NSApplicationDidBecomeActiveNotification object:nil];
    [[NSNotificationCenter defaultCenter] addObserver:self selector:@selector(onAppWillResignActive:) name:NSApplicationWillResignActiveNotification object:nil];
    if (@available(macOS 10.9, *)) {
        dispatch_source_t src = dispatch_source_create(DISPATCH_SOURCE_TYPE_MEMORYPRESSURE, 0,
                                                      DISPATCH_MEMORYPRESSURE_NORMAL | DISPATCH_MEMORYPRESSURE_WARN | DISPATCH_MEMORYPRESSURE_CRITICAL,
                                                      dispatch_get_main_queue());
        if (src) {
            dispatch_source_set_event_handler(src, ^{
                unsigned long pressure = dispatch_source_get_data(src);
                if (pressure & DISPATCH_MEMORYPRESSURE_CRITICAL) {
                    macos_app_on_memory_pressure(2);
                } else if (pressure & DISPATCH_MEMORYPRESSURE_WARN) {
                    macos_app_on_memory_pressure(1);
                } else {
                    macos_app_on_memory_pressure(0);
                }
            });
            dispatch_resume(src);
            self.memorySource = src;
            macos_app_on_memory_pressure(0);
        }
    }
    macos_app_did_become_active();
}

- (void)applicationWillTerminate:(NSNotification *)notification
{
    (void)notification;
    if (self.memorySource) {
        dispatch_source_cancel(self.memorySource);
        self.memorySource = nil;
    }
    macos_app_will_resign_active();
    macos_app_will_terminate();
}

- (void)dealloc
{
    [[NSNotificationCenter defaultCenter] removeObserver:self];
    if (self.memorySource) {
        dispatch_source_cancel(self.memorySource);
        self.memorySource = nil;
    }
}

// C bridge for platform-macos
void macos_request_redraw(void)
{
    dispatch_async(dispatch_get_main_queue(), ^{
        if (gMetalView) { [gMetalView setNeedsDisplay:YES]; }
    });
}

// Forward declaration for display link callback used below
static CVReturn DisplayLinkCallback(CVDisplayLinkRef link,
                                    const CVTimeStamp *inNow,
                                    const CVTimeStamp *inOutputTime,
                                    CVOptionFlags flagsIn,
                                    CVOptionFlags *flagsOut,
                                    void *displayLinkContext);

void macos_set_idle_timer_disabled(uint8_t disabled)
{
    static id activity = nil;
    dispatch_async(dispatch_get_main_queue(), ^{
        if (disabled) {
            if (!activity) {
                activity = [[NSProcessInfo processInfo] beginActivityWithOptions:NSActivityIdleSystemSleepDisabled reason:@"Oxide test app running"];
            }
        } else {
            if (activity) { [[NSProcessInfo processInfo] endActivity:activity]; activity = nil; }
        }
    });
}

int macos_clipboard_get(char **out_ptr, size_t *out_len)
{
    NSPasteboard *pb = [NSPasteboard generalPasteboard];
    NSString *s = [pb stringForType:NSPasteboardTypeString];
    if (!s) return 0;
    NSData *d = [s dataUsingEncoding:NSUTF8StringEncoding];
    if (!d) return 0;
    void *buf = malloc(d.length);
    if (!buf) return 0;
    memcpy(buf, d.bytes, d.length);
    *out_ptr = (char*)buf; *out_len = (size_t)d.length; return 1;
}

void macos_clipboard_set(const char *utf8, size_t len)
{
    NSString *s = (utf8 && len>0) ? [[NSString alloc] initWithBytes:utf8 length:len encoding:NSUTF8StringEncoding] : @"";
    NSPasteboard *pb = [NSPasteboard generalPasteboard];
    [pb clearContents];
    if (s) { [pb setString:s forType:NSPasteboardTypeString]; }
}

void macos_haptics_play(uint32_t pattern)
{
    (void)pattern;
    // No-op on macOS test host; haptics are not typically available on Macs.
}

- (void)onAppDidBecomeActive:(NSNotification *)note
{
    (void)note;
    macos_app_did_become_active();
}

- (void)onAppWillResignActive:(NSNotification *)note
{
    (void)note;
    macos_app_will_resign_active();
}

void macos_free(void *p) { if (p) free(p); }

// Load a resource from the app bundle by name (e.g., "fonts/Inter-Regular.ttf").
// Returns a malloc'd buffer and sets out_len; caller must free via macos_free().
void * macos_resource_read(const char *name_utf8, size_t *out_len)
{
    if (!name_utf8 || !out_len) return NULL;
    NSString *rel = [NSString stringWithUTF8String:name_utf8];
    // Try main bundle resource path directly; allow subpaths under Resources.
    NSString *resPath = [[NSBundle mainBundle] resourcePath];
    if (!resPath) return NULL;
    NSString *full = [resPath stringByAppendingPathComponent:rel];
    NSData *data = [NSData dataWithContentsOfFile:full];
    if (!data) return NULL;
    void *buf = malloc(data.length);
    if (!buf) return NULL;
    memcpy(buf, data.bytes, data.length);
    *out_len = (size_t)data.length;
    return buf;
}

- (BOOL)applicationShouldTerminateAfterLastWindowClosed:(NSApplication *)sender { (void)sender; return YES; }

- (void)applicationWillResignActive:(NSNotification *)notification { (void)notification; [self stopDisplayLink]; }
- (void)applicationDidBecomeActive:(NSNotification *)notification { (void)notification; [self startDisplayLink]; }

- (void)onWindowResize:(NSNotification *)note
{
    (void)note;
    if (!gMetalView) return;
    NSView *v = gMetalView;
    gFpsLayer.frame = CGRectMake(8, v.bounds.size.height - 24, 80, 16);
}

- (void)onPowerStateChanged:(NSNotification *)note
{
    (void)note;
    [self updateTargetRate];
}

- (void)startDisplayLink
{
    if (@available(macOS 15.0, *)) {
        if (gDisplayLink) { return; }
        NSView *view = gMetalView;
        if (view) {
            gDisplayLink = [view displayLinkWithTarget:self selector:@selector(handleDisplayLink:)];
        } else {
            NSScreen *screen = NSScreen.mainScreen;
            if (!screen) { return; }
            gDisplayLink = [screen displayLinkWithTarget:self selector:@selector(handleDisplayLink:)];
        }
        if (!gDisplayLink) { return; }
        [self updateDeviceRate];
        [self updateTargetRate];
        gLastSample = CACurrentMediaTime(); gFpsCount = 0; gFrameCounter = 0;
        [gDisplayLink addToRunLoop:[NSRunLoop mainRunLoop] forMode:NSRunLoopCommonModes];
        return;
    }
    if (gDisplayLinkLegacy) return;
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
    CVDisplayLinkCreateWithActiveCGDisplays(&gDisplayLinkLegacy);
    CVDisplayLinkSetOutputCallback(gDisplayLinkLegacy, &DisplayLinkCallback, NULL);
    [self updateDeviceRate];
    [self updateTargetRate];
    gLastSample = CACurrentMediaTime(); gFpsCount = 0; gFrameCounter = 0;
    CVDisplayLinkStart(gDisplayLinkLegacy);
#pragma clang diagnostic pop
}

- (void)stopDisplayLink
{
    if (@available(macOS 15.0, *)) {
        if (gDisplayLink) {
            [gDisplayLink invalidate];
            gDisplayLink = nil;
        }
    }
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
    if (!gDisplayLinkLegacy) return;
    CVDisplayLinkStop(gDisplayLinkLegacy);
    CVDisplayLinkRelease(gDisplayLinkLegacy);
    gDisplayLinkLegacy = NULL;
#pragma clang diagnostic pop
}

- (void)updateDeviceRate
{
    if (@available(macOS 15.0, *)) {
        if (gDisplayLink) {
            double duration = gDisplayLink.duration;
            if (duration > 0.0) {
                gDeviceHz = (int)round(1.0 / duration);
            } else {
                NSInteger screenHz = 0;
                NSView *view = gMetalView;
                if (view.window.screen) {
                    screenHz = view.window.screen.maximumFramesPerSecond;
                }
                if (screenHz <= 0) {
                    NSScreen *screen = NSScreen.mainScreen;
                    screenHz = screen.maximumFramesPerSecond;
                }
                gDeviceHz = (screenHz > 0) ? (int)screenHz : 60;
            }
            return;
        }
    }
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
    if (!gDisplayLinkLegacy) {
        gDeviceHz = 60;
        return;
    }
    CVTime t = CVDisplayLinkGetNominalOutputVideoRefreshPeriod(gDisplayLinkLegacy);
    if (t.timeValue != 0) {
        double hz = (double)t.timeScale / (double)t.timeValue;
        gDeviceHz = (int)(hz + 0.5);
    } else { gDeviceHz = 60; }
#pragma clang diagnostic pop
}

- (void)updateTargetRate
{
    BOOL lowPower = NO;
    if (@available(macOS 12.0, *)) { lowPower = [NSProcessInfo processInfo].lowPowerModeEnabled; }
    gTargetHz = (gHighRefresh && !lowPower) ? gDeviceHz : 60;
    if (gTargetHz < 1) gTargetHz = 60;
    gDropEvery = (gDeviceHz > 0) ? (gDeviceHz / gTargetHz) : 1;
    if (gDropEvery < 1) gDropEvery = 1;
    if (@available(macOS 15.0, *)) {
        if (gDisplayLink) {
            CAFrameRateRange range;
            range.minimum = (float)gTargetHz;
            range.maximum = (float)gTargetHz;
            range.preferred = (float)gTargetHz;
            gDisplayLink.preferredFrameRateRange = range;
            gDisplayLink.paused = NO;
            gDropEvery = 1;
        }
    }
}

- (void)handleDisplayLink:(CADisplayLink *)link API_AVAILABLE(macos(15.0))
{
    if (!ShouldRenderFrame()) { return; }
    double duration = link.duration;
    if (duration > 0.0) {
        int measured = (int)round(1.0 / duration);
        if (measured > 0 && measured != gDeviceHz) {
            gDeviceHz = measured;
            [self updateTargetRate];
        }
    }
    DispatchFrameTick();
}
@end

int macos_host_start(void)
{
    @autoreleasepool {
        [NSApplication sharedApplication];
        AppDelegate *delegate = [AppDelegate new];
        [NSApp setDelegate:delegate];
        [NSApp run];
        return 0;
    }
}

static BOOL ShouldRenderFrame(void)
{
    if (!macos_app_should_render()) { return NO; }
    if (gDropEvery <= 1) { return YES; }
    uint64_t idx = __atomic_add_fetch(&gFrameCounter, 1, __ATOMIC_RELAXED);
    return (idx % gDropEvery) == 0;
}

static void DispatchFrameTick(void)
{
    gFpsCount += 1;
    CFTimeInterval now = CACurrentMediaTime();
    if (now - gLastSample >= 1.0) {
        double fps = (double)gFpsCount / (now - gLastSample);
        gLastSample = now; gFpsCount = 0;
        if (gFpsLayer) { gFpsLayer.string = [NSString stringWithFormat:@"%3.0f fps", fps]; }
    }
    if (gMetalView) { [gMetalView setNeedsDisplay:YES]; }
}

static CVReturn DisplayLinkCallback(CVDisplayLinkRef link,
                                    const CVTimeStamp *inNow,
                                    const CVTimeStamp *inOutputTime,
                                    CVOptionFlags flagsIn,
                                    CVOptionFlags *flagsOut,
                                    void *displayLinkContext)
{
    (void)link; (void)inNow; (void)inOutputTime; (void)flagsIn; (void)flagsOut; (void)displayLinkContext;
    if (!ShouldRenderFrame()) { return kCVReturnSuccess; }
    // Dispatch to main
    dispatch_async(dispatch_get_main_queue(), ^{
        DispatchFrameTick();
    });
    return kCVReturnSuccess;
}

void macos_set_high_refresh(uint8_t enable)
{
    gHighRefresh = (enable ? YES : NO);
    // Also propagate to CAMetalLayer when supported to minimize latency
    dispatch_async(dispatch_get_main_queue(), ^{
        AppDelegate *d = (AppDelegate*)NSApp.delegate;
        [d updateTargetRate];
        if (gMetalView) {
            CAMetalLayer *layer = (CAMetalLayer*)gMetalView.layer;
            if ([layer respondsToSelector:@selector(setDisplaySyncEnabled:)]) {
                layer.displaySyncEnabled = (enable ? YES : NO);
            }
        }
    });
}
